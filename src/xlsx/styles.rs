//! Read cell formatting, column widths and row heights from an .xlsx package.
//!
//! calamine reads values and formulas but not styles, so this parses
//! `xl/styles.xml`, the theme and each worksheet's XML directly.

use crate::cell::CellCoord;
use crate::format::{Borders, CellFormat, HAlign, Rgb, SheetFormatting, builtin_number_format};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::{Read, Seek};

/// A `<col width>` from the file to UI points. The stored width includes
/// Excel's 5px padding: the default 8.43-character column is stored as
/// 9.140625, which is 64px and maps to the grid's default 80 points.
pub fn excel_width_to_points(stored: f64) -> f32 {
    let pixels = (((256.0 * stored + 18.0) / 256.0) * 7.0).trunc();
    (pixels * 1.25) as f32
}

/// UI points to the character width `rust_xlsxwriter::Worksheet::set_column_width`
/// takes (it adds the padding back when writing).
pub fn points_to_excel_width(points: f32) -> f64 {
    ((points as f64 / 1.25 - 5.0) / 7.0).max(0.0)
}

/// Excel row height (font points) to UI points. Excel's default 15 maps to the
/// grid's default 22.
pub fn excel_height_to_points(height: f64) -> f32 {
    (height * 22.0 / 15.0) as f32
}

pub fn points_to_excel_height(points: f32) -> f64 {
    points as f64 * 15.0 / 22.0
}

/// Formatting for each sheet, in workbook order, keyed by sheet name.
pub fn read_formatting<R: Read + Seek>(
    reader: R,
) -> Result<Vec<(String, SheetFormatting)>, String> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    let mut read = |name: &str| -> Option<String> {
        let mut file = zip.by_name(name).ok()?;
        let mut s = String::new();
        file.read_to_string(&mut s).ok()?;
        Some(s)
    };

    let workbook = read("xl/workbook.xml").ok_or("missing xl/workbook.xml")?;
    let rels = read("xl/_rels/workbook.xml.rels").unwrap_or_default();
    let theme = read("xl/theme/theme1.xml")
        .map(|t| parse_theme(&t))
        .unwrap_or_else(default_theme);
    let styles = read("xl/styles.xml")
        .map(|s| parse_styles(&s, &theme))
        .transpose()?
        .unwrap_or_default();

    let targets = parse_rels(&rels);
    let mut out = Vec::new();
    for (name, rid) in parse_sheet_list(&workbook)? {
        let Some(target) = targets.get(&rid) else {
            continue;
        };
        let path = match target.strip_prefix('/') {
            Some(abs) => abs.to_string(),
            None => format!("xl/{target}"),
        };
        let formatting = match read(&path) {
            Some(xml) => parse_sheet(&xml, &styles)?,
            None => SheetFormatting::default(),
        };
        out.push((name, formatting));
    }
    Ok(out)
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| {
            a.decode_and_unescape_value(e.decoder())
                .ok()
                .map(|v| v.into_owned())
        })
}

fn attr_num<T: std::str::FromStr>(e: &BytesStart, name: &[u8]) -> Option<T> {
    attr(e, name)?.parse().ok()
}

/// `<b/>` is on; `<b val="0"/>` or `val="false"` is off.
fn flag(e: &BytesStart) -> bool {
    !matches!(attr(e, b"val").as_deref(), Some("0" | "false" | "none"))
}

/// Visit every start or empty element as (local name, element, is_empty),
/// and every end tag as (local name, None).
fn walk(xml: &str, mut visit: impl FnMut(&[u8], Option<&BytesStart>, bool)) -> Result<(), String> {
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => visit(e.local_name().as_ref(), Some(&e), false),
            Ok(Event::Empty(e)) => {
                visit(e.local_name().as_ref(), Some(&e), true);
                visit(e.local_name().as_ref(), None, true);
            }
            Ok(Event::End(e)) => visit(e.local_name().as_ref(), None, false),
            Ok(Event::Eof) => return Ok(()),
            Err(e) => return Err(format!("XML error: {e}")),
            Ok(_) => {}
        }
    }
}

fn parse_sheet_list(workbook: &str) -> Result<Vec<(String, String)>, String> {
    let mut sheets = Vec::new();
    walk(workbook, |name, e, _| {
        if let (b"sheet", Some(e)) = (name, e) {
            if let (Some(n), Some(id)) = (attr(e, b"name"), attr(e, b"id")) {
                sheets.push((n, id));
            }
        }
    })?;
    Ok(sheets)
}

fn parse_rels(rels: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let _ = walk(rels, |name, e, _| {
        if let (b"Relationship", Some(e)) = (name, e) {
            if let (Some(id), Some(target)) = (attr(e, b"Id"), attr(e, b"Target")) {
                map.insert(id, target);
            }
        }
    });
    map
}

/// Theme colors in Excel's index order: lt1, dk1, lt2, dk2, accent1-6, hlink, folHlink.
type Theme = Vec<Rgb>;

fn default_theme() -> Theme {
    [
        "FFFFFF", "000000", "E7E6E6", "44546A", "4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5",
        "70AD47", "0563C1", "954F72",
    ]
    .iter()
    .filter_map(|h| Rgb::from_hex(h))
    .collect()
}

fn parse_theme(xml: &str) -> Theme {
    // The scheme lists dk1, lt1, dk2, lt2, accents...; Excel indexes lt before dk.
    let mut scheme: Vec<(String, Rgb)> = Vec::new();
    let mut in_scheme = false;
    let mut current: Option<String> = None;
    let _ = walk(xml, |name, e, _| match (name, e) {
        (b"clrScheme", Some(_)) => in_scheme = true,
        (b"clrScheme", None) => in_scheme = false,
        (b"srgbClr" | b"sysClr", Some(e)) if in_scheme => {
            let hex = attr(e, b"lastClr").or_else(|| attr(e, b"val"));
            if let (Some(slot), Some(rgb)) =
                (current.take(), hex.as_deref().and_then(Rgb::from_hex))
            {
                scheme.push((slot, rgb));
            }
        }
        (slot, Some(_)) if in_scheme => current = Some(String::from_utf8_lossy(slot).into_owned()),
        _ => {}
    });
    let get = |slot: &str| scheme.iter().find(|(s, _)| s == slot).map(|(_, c)| *c);
    let order = [
        "lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ];
    let defaults = default_theme();
    order
        .iter()
        .zip(defaults)
        .map(|(slot, fallback)| get(slot).unwrap_or(fallback))
        .collect()
}

/// The legacy 64-color palette for `indexed` colors.
const INDEXED: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000,
    0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000,
    0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC,
    0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF,
    0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF,
    0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600,
    0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399,
    0x333333,
];

/// Resolve a `<color>`/`<fgColor>` element. `None` for automatic colors.
fn parse_color(e: &BytesStart, theme: &Theme) -> Option<Rgb> {
    if attr(e, b"auto").as_deref() == Some("1") {
        return None;
    }
    let base = if let Some(hex) = attr(e, b"rgb") {
        Rgb::from_hex(&hex)?
    } else if let Some(i) = attr_num::<usize>(e, b"theme") {
        *theme.get(i)?
    } else {
        let v = *INDEXED.get(attr_num::<usize>(e, b"indexed")?)?;
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    };
    let tint: f64 = attr_num(e, b"tint").unwrap_or(0.0);
    if tint == 0.0 {
        return Some(base);
    }
    let apply = |c: u8| -> u8 {
        let c = c as f64;
        let v = if tint < 0.0 {
            c * (1.0 + tint)
        } else {
            c + (255.0 - c) * tint
        };
        v.round().clamp(0.0, 255.0) as u8
    };
    Some(Rgb(apply(base.0), apply(base.1), apply(base.2)))
}

#[derive(Default, Clone)]
struct Font {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    size: Option<f64>,
    color: Option<Rgb>,
}

/// One `CellFormat` per `cellXfs` entry, indexed by a cell's `s` attribute.
#[derive(Default)]
struct StyleTable {
    xfs: Vec<CellFormat>,
}

fn parse_styles(xml: &str, theme: &Theme) -> Result<StyleTable, String> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Fonts,
        Fills,
        Borders,
        CellXfs,
        Other,
    }
    let mut section = Section::None;
    let mut num_fmts: HashMap<u32, String> = HashMap::new();
    let mut fonts: Vec<Font> = Vec::new();
    let mut fills: Vec<Option<Rgb>> = Vec::new();
    let mut borders: Vec<Borders> = Vec::new();
    let mut xfs: Vec<(u32, usize, usize, usize, HAlign)> = Vec::new();

    let mut font = Font::default();
    let mut fill: Option<Rgb> = None;
    let mut solid_or_pattern = false;
    let mut border = Borders::default();

    walk(xml, |name, e, _| {
        match (name, e) {
            (b"numFmt", Some(e)) => {
                if let (Some(id), Some(code)) = (attr_num(e, b"numFmtId"), attr(e, b"formatCode")) {
                    num_fmts.insert(id, code);
                }
            }
            (b"fonts", Some(_)) => section = Section::Fonts,
            (b"fills", Some(_)) => section = Section::Fills,
            (b"borders", Some(_)) => section = Section::Borders,
            (b"cellXfs", Some(_)) => section = Section::CellXfs,
            // Named styles and conditional-format styles also hold xf/font/fill.
            (b"cellStyleXfs" | b"dxfs" | b"cellStyles" | b"tableStyles" | b"colors", Some(_)) => {
                section = Section::Other
            }
            (b"fonts" | b"fills" | b"borders" | b"cellXfs" | b"cellStyleXfs" | b"dxfs", None) => {
                section = Section::None
            }

            (b"font", Some(_)) if section == Section::Fonts => font = Font::default(),
            (b"font", None) if section == Section::Fonts => fonts.push(std::mem::take(&mut font)),
            (b"b", Some(e)) if section == Section::Fonts => font.bold = flag(e),
            (b"i", Some(e)) if section == Section::Fonts => font.italic = flag(e),
            (b"u", Some(e)) if section == Section::Fonts => font.underline = flag(e),
            (b"strike", Some(e)) if section == Section::Fonts => font.strikethrough = flag(e),
            (b"sz", Some(e)) if section == Section::Fonts => font.size = attr_num(e, b"val"),
            (b"color", Some(e)) if section == Section::Fonts => font.color = parse_color(e, theme),

            (b"fill", Some(_)) if section == Section::Fills => {
                fill = None;
                solid_or_pattern = false;
            }
            (b"fill", None) if section == Section::Fills => {
                fills.push(if solid_or_pattern { fill } else { None })
            }
            (b"patternFill", Some(e)) if section == Section::Fills => {
                // Index 1 is always gray125; treat only real fills as colored.
                let pattern = attr(e, b"patternType").unwrap_or_default();
                solid_or_pattern = !matches!(pattern.as_str(), "" | "none" | "gray125");
            }
            (b"fgColor", Some(e)) if section == Section::Fills => fill = parse_color(e, theme),

            (b"border", Some(_)) if section == Section::Borders => border = Borders::default(),
            (b"border", None) if section == Section::Borders => borders.push(border),
            (side @ (b"left" | b"right" | b"top" | b"bottom" | b"start" | b"end"), Some(e))
                if section == Section::Borders =>
            {
                let on = attr(e, b"style").is_some_and(|s| s != "none");
                match side {
                    b"left" | b"start" => border.left = on,
                    b"right" | b"end" => border.right = on,
                    b"top" => border.top = on,
                    _ => border.bottom = on,
                }
            }

            (b"xf", Some(e)) if section == Section::CellXfs => xfs.push((
                attr_num(e, b"numFmtId").unwrap_or(0),
                attr_num(e, b"fontId").unwrap_or(0),
                attr_num(e, b"fillId").unwrap_or(0),
                attr_num(e, b"borderId").unwrap_or(0),
                HAlign::General,
            )),
            (b"alignment", Some(e)) if section == Section::CellXfs => {
                if let Some(xf) = xfs.last_mut() {
                    xf.4 = match attr(e, b"horizontal").as_deref() {
                        Some("left") => HAlign::Left,
                        Some("center" | "centerContinuous") => HAlign::Center,
                        Some("right") => HAlign::Right,
                        _ => HAlign::General,
                    };
                }
            }
            _ => {}
        }
    })?;

    // Sizes matching the workbook's default font count as default.
    let default_size = fonts.first().and_then(|f| f.size);
    let xfs = xfs
        .into_iter()
        .map(|(num_fmt, font_id, fill_id, border_id, h_align)| {
            let font = fonts.get(font_id).cloned().unwrap_or_default();
            CellFormat {
                bold: font.bold,
                italic: font.italic,
                underline: font.underline,
                strikethrough: font.strikethrough,
                font_size: font
                    .size
                    .filter(|&s| Some(s) != default_size)
                    .map(|s| s.round().clamp(1.0, 255.0) as u8),
                // Black text is the default; leave it to the theme so it reads in dark mode.
                font_color: font.color.filter(|&c| c != Rgb::BLACK),
                fill: fills.get(fill_id).copied().flatten(),
                h_align,
                borders: borders.get(border_id).copied().unwrap_or_default(),
                number_format: num_fmts
                    .get(&num_fmt)
                    .cloned()
                    .or_else(|| builtin_number_format(num_fmt).map(str::to_string))
                    .filter(|c| !c.eq_ignore_ascii_case("general")),
            }
        })
        .collect();
    Ok(StyleTable { xfs })
}

fn parse_sheet(xml: &str, styles: &StyleTable) -> Result<SheetFormatting, String> {
    let mut formatting = SheetFormatting::default();
    let default_points = excel_height_to_points(15.0);
    walk(xml, |name, e, _| match (name, e) {
        (b"col", Some(e)) => {
            let (Some(min), Some(max)) = (attr_num::<u32>(e, b"min"), attr_num::<u32>(e, b"max"))
            else {
                return;
            };
            let hidden = attr(e, b"hidden").as_deref() == Some("1");
            let width = if hidden {
                Some(0.0)
            } else {
                attr_num::<f64>(e, b"width").map(excel_width_to_points)
            };
            // Ranges like min=1 max=16384 cover every column; cap them.
            if let Some(w) = width {
                for col in min.max(1)..=max.min(min.max(1) + 1024) {
                    formatting.column_widths.insert(col - 1, w);
                }
            }
        }
        (b"row", Some(e)) => {
            if let (Some(r), Some(ht)) = (attr_num::<u32>(e, b"r"), attr_num::<f64>(e, b"ht")) {
                let points = excel_height_to_points(ht);
                if (points - default_points).abs() > 0.5 {
                    formatting.row_heights.insert(r.saturating_sub(1), points);
                }
            }
        }
        (b"c", Some(e)) => {
            let (Some(r), Some(s)) = (attr(e, b"r"), attr_num::<usize>(e, b"s")) else {
                return;
            };
            if let (Some(coord), Some(format)) = (CellCoord::from_a1(&r), styles.xfs.get(s)) {
                if !format.is_default() {
                    formatting.set(coord, format.clone());
                }
            }
        }
        _ => {}
    })?;
    let default_width = excel_width_to_points(9.140625);
    formatting
        .column_widths
        .retain(|_, w| (*w - default_width).abs() > 0.5);
    Ok(formatting)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a workbook saved by Excel: a theme font color with tint, an
    // indexed fill, built-in and custom number formats, and named-style and
    // conditional-format sections that must not be mistaken for cell styles.
    const STYLES: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <numFmts count="1"><numFmt numFmtId="164" formatCode="&quot;$&quot;#,##0.00"/></numFmts>
  <fonts count="3">
    <font><sz val="11"/><color theme="1"/><name val="Calibri"/></font>
    <font><b/><sz val="16"/><color theme="4" tint="-0.249977111117893"/><name val="Calibri"/></font>
    <font><i/><u val="none"/><sz val="11"/><color rgb="FFFF0000"/></font>
  </fonts>
  <fills count="3">
    <fill><patternFill patternType="none"/></fill>
    <fill><patternFill patternType="gray125"/></fill>
    <fill><patternFill patternType="solid"><fgColor indexed="13"/><bgColor indexed="64"/></patternFill></fill>
  </fills>
  <borders count="2">
    <border><left/><right/><top/><bottom/><diagonal/></border>
    <border><left style="thin"><color indexed="64"/></left><right/><top/><bottom style="medium"/></border>
  </borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="1" fillId="2" borderId="1"/></cellStyleXfs>
  <cellXfs count="4">
    <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
    <xf numFmtId="0" fontId="1" fillId="2" borderId="1" xfId="0" applyFont="1"><alignment horizontal="center"/></xf>
    <xf numFmtId="164" fontId="2" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>
    <xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0"/>
  </cellXfs>
  <dxfs count="1"><dxf><font><b/></font><fill><patternFill><bgColor rgb="FF00FF00"/></patternFill></fill></dxf></dxfs>
</styleSheet>"##;

    const SHEET: &str = r##"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <cols><col min="1" max="2" width="20.7109375" customWidth="1"/><col min="3" max="3" width="9.140625"/></cols>
  <sheetData>
    <row r="1" ht="30" customHeight="1"><c r="A1" s="1" t="s"><v>0</v></c><c r="B1" s="0"><v>1</v></c></row>
    <row r="2"><c r="A2" s="2"><v>5</v></c><c r="B2" s="3"><v>45000</v></c></row>
  </sheetData>
</worksheet>"##;

    #[test]
    fn reads_excel_styles() {
        let styles = parse_styles(STYLES, &default_theme()).unwrap();
        assert_eq!(styles.xfs.len(), 4);
        assert!(styles.xfs[0].is_default());

        let title = &styles.xfs[1];
        assert!(title.bold && !title.italic);
        assert_eq!(title.font_size, Some(16));
        // accent1 4472C4 darkened by 25%.
        assert_eq!(title.font_color, Some(Rgb(0x33, 0x56, 0x93)));
        assert_eq!(title.fill, Some(Rgb(0xFF, 0xFF, 0x00)));
        assert_eq!(title.h_align, HAlign::Center);
        assert_eq!(
            title.borders,
            Borders {
                left: true,
                bottom: true,
                ..Borders::NONE
            }
        );

        let money = &styles.xfs[2];
        assert!(money.italic && !money.underline);
        assert_eq!(money.font_size, None);
        assert_eq!(money.font_color, Some(Rgb(255, 0, 0)));
        assert_eq!(money.number_format.as_deref(), Some("\"$\"#,##0.00"));
        assert_eq!(styles.xfs[3].number_format.as_deref(), Some("m/d/yyyy"));
    }

    #[test]
    fn reads_sheet_styles_and_sizes() {
        let styles = parse_styles(STYLES, &default_theme()).unwrap();
        let sheet = parse_sheet(SHEET, &styles).unwrap();
        let a1 = CellCoord::from_a1("A1").unwrap();
        assert!(sheet.get(a1).is_some_and(|f| f.bold));
        assert!(sheet.get(CellCoord::from_a1("B1").unwrap()).is_none());
        assert!(sheet.get(CellCoord::from_a1("B2").unwrap()).is_some());
        assert_eq!(
            sheet.column_widths.len(),
            2,
            "column C has the default width"
        );
        assert!((sheet.column_widths[&0] - excel_width_to_points(20.7109375)).abs() < 0.01);
        assert!((sheet.row_heights[&0] - 44.0).abs() < 0.01);
    }

    #[test]
    fn theme_colors_come_from_the_workbook() {
        let theme = parse_theme(
            r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements>
            <a:clrScheme name="Custom">
              <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
              <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
              <a:dk2><a:srgbClr val="112233"/></a:dk2>
              <a:lt2><a:srgbClr val="EEEEEE"/></a:lt2>
              <a:accent1><a:srgbClr val="AA0000"/></a:accent1>
            </a:clrScheme></a:themeElements></a:theme>"#,
        );
        assert_eq!(theme[0], Rgb::WHITE);
        assert_eq!(theme[1], Rgb::BLACK);
        assert_eq!(theme[3], Rgb(0x11, 0x22, 0x33));
        assert_eq!(theme[4], Rgb(0xAA, 0, 0));
        // Missing slots fall back to the Office palette.
        assert_eq!(theme[5], Rgb(0xED, 0x7D, 0x31));
    }
}
