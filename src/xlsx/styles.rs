//! Read cell formatting, column widths and row heights from an .xlsx package.
//!
//! calamine reads values and formulas but not styles, so this parses
//! `xl/styles.xml`, the theme and each worksheet's XML directly.

use crate::cell::CellCoord;
use crate::cell::CellRange;
use crate::format::conditional::{
    AverageRule, CfRule, CfStyle, Cfvo, CfvoKind, ConditionalFormat, TextRule,
};
use crate::format::validation::CompareOp;
use crate::format::{
    AutoFilter, Borders, CellFormat, HAlign, Rgb, SheetFormatting, VAlign, builtin_number_format,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
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
    let mut read_bytes = |name: &str| -> Option<Vec<u8>> {
        let mut file = zip.by_name(name).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;
        Some(buf)
    };
    let read = |read_bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>, name: &str| {
        String::from_utf8(read_bytes(name)?).ok()
    };

    let workbook = read(&mut read_bytes, "xl/workbook.xml").ok_or("missing xl/workbook.xml")?;
    let rels = read(&mut read_bytes, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let theme = read(&mut read_bytes, "xl/theme/theme1.xml")
        .map(|t| parse_theme(&t))
        .unwrap_or_else(default_theme);
    let styles = read(&mut read_bytes, "xl/styles.xml")
        .map(|s| parse_styles(&s, &theme))
        .transpose()?
        .unwrap_or_default();

    let targets = parse_rels(&rels);
    let mut out = Vec::new();
    let mut sheet_parts = Vec::new();
    let read_rel = |read_bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
                    sheet_path: &str,
                    kind: &str|
     -> Option<(String, String)> {
        let target = part_rels(read_bytes, sheet_path)
            .into_iter()
            .find(|(_, t, _)| t.ends_with(kind))
            .map(|(_, _, target)| target)?;
        let xml = read(read_bytes, &target)?;
        Some((target, xml))
    };
    for (name, rid) in parse_sheet_list(&workbook)? {
        let Some(target) = targets.get(&rid) else {
            continue;
        };
        let path = match target.strip_prefix('/') {
            Some(abs) => abs.to_string(),
            None => format!("xl/{target}"),
        };
        sheet_parts.push((name.clone(), path.clone()));
        let mut formatting = match read(&mut read_bytes, &path) {
            Some(xml) => {
                let mut f = parse_sheet(&xml, &styles)?;
                f.validations = parse_validations(&xml)?;
                f.conditional = parse_conditional(&xml, &styles.dxfs, &theme)?;
                f
            }
            None => SheetFormatting::default(),
        };
        if let Some((_, xml)) = read_rel(&mut read_bytes, &path, "/comments") {
            formatting.notes = parse_comments(&xml)?;
        }
        if let Some((drawing, xml)) = read_rel(&mut read_bytes, &path, "/drawing") {
            let images = part_rels(&mut read_bytes, &drawing);
            formatting.pictures = super::drawing::parse_drawing(&xml)
                .into_iter()
                .filter_map(|d| {
                    let (_, _, target) = images.iter().find(|(id, _, _)| *id == d.embed)?;
                    super::drawing::to_picture(d, read_bytes(target)?)
                })
                .collect();
        }
        out.push((name, formatting));
    }

    // Pivot tables: RustSheet's own definitions, or Excel's.
    let pivots: Vec<(String, Vec<crate::pivot::PivotTable>)> =
        match read(&mut read_bytes, super::writer::PIVOTS_MANIFEST) {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => super::pivot_reader::read_excel_pivots(&mut read_bytes, &sheet_parts),
        };
    for (name, list) in pivots {
        if let Some((_, f)) = out.iter_mut().find(|(n, _)| *n == name) {
            f.pivots = list;
        }
    }
    Ok(out)
}

/// A part's relationships as (id, type, resolved target):
/// xl/worksheets/sheet1.xml's are in xl/worksheets/_rels/sheet1.xml.rels.
pub(super) fn part_rels(
    read_bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    part: &str,
) -> Vec<(String, String, String)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let Some(rels) =
        read_bytes(&format!("{dir}/_rels/{file}.rels")).and_then(|b| String::from_utf8(b).ok())
    else {
        return Vec::new();
    };
    parse_typed_rels(&rels)
        .into_iter()
        .map(|(id, kind, target)| (id, kind, resolve_part(dir, &target)))
        .collect()
}

pub(super) fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
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

/// Relationships as (id, type, target).
fn parse_typed_rels(rels: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let _ = walk(rels, |name, e, _| {
        if let (b"Relationship", Some(e)) = (name, e) {
            if let (Some(id), Some(kind), Some(target)) =
                (attr(e, b"Id"), attr(e, b"Type"), attr(e, b"Target"))
            {
                out.push((id, kind, target));
            }
        }
    });
    out
}

/// A relationship target relative to `dir` ("../comments1.xml" from
/// "xl/worksheets" is "xl/comments1.xml"); absolute targets start at the root.
pub(super) fn resolve_part(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Text of an element, with entity references put back.
pub(super) fn entity_text(e: &quick_xml::events::BytesRef) -> String {
    if let Ok(Some(c)) = e.resolve_char_ref() {
        return c.to_string();
    }
    match e.decode().as_deref() {
        Ok("amp") => "&".into(),
        Ok("lt") => "<".into(),
        Ok("gt") => ">".into(),
        Ok("quot") => "\"".into(),
        Ok("apos") => "'".into(),
        _ => String::new(),
    }
}

/// A space-separated list of ranges or cells ("A1:A10 C3").
pub(super) fn parse_sqref(s: &str) -> Vec<CellRange> {
    s.split_whitespace()
        .filter_map(|r| {
            CellRange::from_a1(r).or_else(|| CellCoord::from_a1(r).map(CellRange::single))
        })
        .collect()
}

/// `<dataValidation type sqref operator ...><formula1>...`, and the x14
/// extension Excel uses for lists on other sheets
/// (`<x14:dataValidation><x14:formula1><xm:f>...</xm:f>...<xm:sqref>`).
fn parse_validations(xml: &str) -> Result<Vec<crate::format::validation::DataValidation>, String> {
    use crate::format::validation::{DataValidation, ErrorStyle, ValidationKind};
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current: Option<DataValidation> = None;
    // Which text we're collecting: 1, 2 = formulas, 3 = x14 sqref
    let mut target = 0u8;
    let mut text = String::new();
    let flag = |e: &BytesStart, name: &[u8], default: bool| match attr(e, name).as_deref() {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default,
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"dataValidation" => {
                    let kind = match attr(&e, b"type").as_deref() {
                        Some("whole") => ValidationKind::Whole,
                        Some("decimal") => ValidationKind::Decimal,
                        Some("list") => ValidationKind::List,
                        Some("date") => ValidationKind::Date,
                        Some("time") => ValidationKind::Time,
                        Some("textLength") => ValidationKind::TextLength,
                        Some("custom") => ValidationKind::Custom,
                        _ => ValidationKind::Any,
                    };
                    current = Some(DataValidation {
                        ranges: attr(&e, b"sqref")
                            .map(|s| parse_sqref(&s))
                            .unwrap_or_default(),
                        kind,
                        operator: CompareOp::from_xml(&attr(&e, b"operator").unwrap_or_default()),
                        allow_blank: flag(&e, b"allowBlank", false),
                        // Excel's attribute is inverted: showDropDown="1" hides it.
                        dropdown: !flag(&e, b"showDropDown", false),
                        show_input: flag(&e, b"showInputMessage", false),
                        input_title: attr(&e, b"promptTitle").unwrap_or_default(),
                        input_message: attr(&e, b"prompt").unwrap_or_default(),
                        show_error: flag(&e, b"showErrorMessage", false),
                        error_style: match attr(&e, b"errorStyle").as_deref() {
                            Some("warning") => ErrorStyle::Warning,
                            Some("information") => ErrorStyle::Information,
                            _ => ErrorStyle::Stop,
                        },
                        error_title: attr(&e, b"errorTitle").unwrap_or_default(),
                        error_message: attr(&e, b"error").unwrap_or_default(),
                        ..Default::default()
                    });
                }
                b"formula1" => {
                    target = 1;
                    text.clear();
                }
                b"formula2" => {
                    target = 2;
                    text.clear();
                }
                b"sqref" => {
                    target = 3;
                    text.clear();
                }
                _ => {}
            },
            Ok(Event::Text(t)) if target != 0 => {
                text.push_str(&t.decode().map_err(|e| e.to_string())?)
            }
            Ok(Event::GeneralRef(r)) if target != 0 => text.push_str(&entity_text(&r)),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"formula1" | b"formula2" | b"sqref" => {
                    if let Some(dv) = &mut current {
                        let value = std::mem::take(&mut text);
                        match target {
                            1 => dv.formula1 = value,
                            2 => dv.formula2 = Some(value),
                            _ => dv.ranges = parse_sqref(&value),
                        }
                    }
                    target = 0;
                }
                b"dataValidation" => {
                    if let Some(dv) = current.take() {
                        if !dv.ranges.is_empty() {
                            out.push(dv);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML error in validations: {e}")),
            _ => {}
        }
    }
    Ok(out)
}

/// A `<cfRule>` being read.
struct PendingRule {
    kind: String,
    dxf: Option<usize>,
    priority: i64,
    operator: String,
    text: Option<String>,
    rank: u32,
    percent: bool,
    bottom: bool,
    above: bool,
    equal: bool,
    std_dev: bool,
    stop: bool,
    formulas: Vec<String>,
    cfvos: Vec<Cfvo>,
    colors: Vec<Rgb>,
}

impl PendingRule {
    fn new(e: &BytesStart) -> Self {
        let on = |name: &[u8], default: bool| match attr(e, name).as_deref() {
            Some("1" | "true") => true,
            Some("0" | "false") => false,
            _ => default,
        };
        Self {
            kind: attr(e, b"type").unwrap_or_default(),
            dxf: attr_num(e, b"dxfId"),
            priority: attr_num(e, b"priority").unwrap_or(i64::MAX),
            operator: attr(e, b"operator").unwrap_or_default(),
            text: attr(e, b"text"),
            rank: attr_num(e, b"rank").unwrap_or(10),
            percent: on(b"percent", false),
            bottom: on(b"bottom", false),
            above: on(b"aboveAverage", true),
            equal: on(b"equalAverage", false),
            std_dev: attr(e, b"stdDev").is_some(),
            stop: on(b"stopIfTrue", false),
            formulas: Vec::new(),
            cfvos: Vec::new(),
            colors: Vec::new(),
        }
    }

    /// The rule with its priority, or `None` for kinds RustSheet doesn't
    /// draw (icon sets, standard-deviation averages).
    fn finish(self, ranges: &[CellRange], dxfs: &[CfStyle]) -> Option<(i64, ConditionalFormat)> {
        if ranges.is_empty() {
            return None;
        }
        let style = self
            .dxf
            .and_then(|i| dxfs.get(i))
            .cloned()
            .unwrap_or_default();
        let first = self.formulas.first().cloned();
        let not = self.kind.starts_with("not");
        let rule = match self.kind.as_str() {
            "cellIs" => CfRule::CellIs {
                op: CompareOp::from_xml(&self.operator),
                formula1: first?,
                formula2: self.formulas.get(1).cloned(),
                style,
            },
            kind @ ("containsText" | "notContainsText" | "beginsWith" | "endsWith") => {
                CfRule::Text {
                    rule: match kind {
                        "containsText" => TextRule::Contains,
                        "notContainsText" => TextRule::NotContains,
                        "beginsWith" => TextRule::BeginsWith,
                        _ => TextRule::EndsWith,
                    },
                    text: self.text.clone()?,
                    style,
                }
            }
            "top10" => CfRule::Top {
                bottom: self.bottom,
                rank: self.rank.max(1),
                percent: self.percent,
                style,
            },
            "aboveAverage" if !self.std_dev => CfRule::Average {
                rule: match (self.above, self.equal) {
                    (true, false) => AverageRule::Above,
                    (false, false) => AverageRule::Below,
                    (true, true) => AverageRule::EqualOrAbove,
                    (false, true) => AverageRule::EqualOrBelow,
                },
                style,
            },
            "duplicateValues" | "uniqueValues" => CfRule::Duplicate {
                unique: self.kind == "uniqueValues",
                style,
            },
            "containsBlanks" | "notContainsBlanks" => CfRule::Blanks { not, style },
            "containsErrors" | "notContainsErrors" => CfRule::Errors { not, style },
            // Date periods ("yesterday", "this week") carry an equivalent formula.
            "expression" | "timePeriod" => CfRule::Expression {
                formula: first?,
                style,
            },
            "colorScale" if self.cfvos.len() >= 2 && self.colors.len() == self.cfvos.len() => {
                CfRule::ColorScale {
                    stops: self.cfvos.into_iter().zip(self.colors).collect(),
                }
            }
            "dataBar" if self.cfvos.len() >= 2 => CfRule::DataBar {
                min: self.cfvos[0].clone(),
                max: self.cfvos[1].clone(),
                color: self.colors.first().copied().unwrap_or(DATA_BAR_BLUE),
            },
            _ => return None,
        };
        Some((
            self.priority,
            ConditionalFormat {
                ranges: ranges.to_vec(),
                rule,
                stop_if_true: self.stop,
            },
        ))
    }
}

const DATA_BAR_BLUE: Rgb = Rgb(0x63, 0x8E, 0xC6);

/// Rules from `<conditionalFormatting sqref><cfRule type dxfId priority>`,
/// in priority order. Excel 2010 extensions (`extLst`) are skipped: they
/// repeat data bars with extra styling, or hold icon sets.
fn parse_conditional(
    xml: &str,
    dxfs: &[CfStyle],
    theme: &Theme,
) -> Result<Vec<ConditionalFormat>, String> {
    let mut reader = Reader::from_str(xml);
    let mut found: Vec<(i64, ConditionalFormat)> = Vec::new();
    let mut ranges: Vec<CellRange> = Vec::new();
    let mut rule: Option<PendingRule> = None;
    let mut ext_depth = 0u32;
    let mut in_formula = false;
    let mut text = String::new();
    loop {
        let event = reader.read_event();
        match &event {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let empty = matches!(event, Ok(Event::Empty(_)));
                let name = e.local_name();
                match name.as_ref() {
                    b"extLst" => ext_depth += u32::from(!empty),
                    _ if ext_depth > 0 => {}
                    b"conditionalFormatting" => {
                        ranges = attr(e, b"sqref")
                            .map(|s| parse_sqref(&s))
                            .unwrap_or_default()
                    }
                    b"cfRule" => {
                        let pending = PendingRule::new(e);
                        if empty {
                            found.extend(pending.finish(&ranges, dxfs));
                        } else {
                            rule = Some(pending);
                        }
                    }
                    b"formula" if !empty => {
                        in_formula = true;
                        text.clear();
                    }
                    b"cfvo" => {
                        if let Some(p) = &mut rule {
                            let kind = CfvoKind::from_xml(&attr(e, b"type").unwrap_or_default());
                            // Lowest/highest carry a meaningless val="0".
                            let value = match kind {
                                CfvoKind::Min | CfvoKind::Max => String::new(),
                                _ => attr(e, b"val").unwrap_or_default(),
                            };
                            p.cfvos.push(Cfvo { kind, value });
                        }
                    }
                    b"color" => {
                        if let Some(p) = &mut rule {
                            p.colors
                                .push(parse_color(e, theme).unwrap_or(DATA_BAR_BLUE));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_formula => {
                text.push_str(&t.decode().map_err(|e| e.to_string())?)
            }
            Ok(Event::GeneralRef(r)) if in_formula => text.push_str(&entity_text(r)),
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"extLst" => ext_depth = ext_depth.saturating_sub(1),
                _ if ext_depth > 0 => {}
                b"formula" => {
                    in_formula = false;
                    if let Some(p) = &mut rule {
                        p.formulas.push(std::mem::take(&mut text));
                    }
                }
                b"cfRule" => {
                    if let Some(p) = rule.take() {
                        found.extend(p.finish(&ranges, dxfs));
                    }
                }
                b"conditionalFormatting" => ranges.clear(),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML error in conditional formats: {e}")),
            _ => {}
        }
    }
    found.sort_by_key(|(priority, _)| *priority);
    Ok(found.into_iter().map(|(_, cf)| cf).collect())
}

/// Notes from a comments part: `<comment ref="A1" authorId="0"><text>...`.
fn parse_comments(xml: &str) -> Result<BTreeMap<CellCoord, crate::format::Note>, String> {
    let mut reader = Reader::from_str(xml);
    let mut authors: Vec<String> = Vec::new();
    let mut notes = BTreeMap::new();
    let mut in_author = false;
    let mut in_text = false;
    let mut current: Option<(CellCoord, Option<usize>, String)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"author" => {
                    in_author = true;
                    authors.push(String::new());
                }
                b"comment" => {
                    current = attr(&e, b"ref")
                        .and_then(|r| CellCoord::from_a1(&r))
                        .map(|c| (c, attr_num(&e, b"authorId"), String::new()));
                }
                b"t" => in_text = true,
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"author" => in_author = false,
                b"t" => in_text = false,
                b"comment" => {
                    if let Some((coord, author, text)) = current.take() {
                        let author = author
                            .and_then(|i| authors.get(i).cloned())
                            .filter(|a| !a.is_empty());
                        notes.insert(coord, crate::format::Note { text, author });
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                let s = t.decode().map_err(|e| e.to_string())?;
                if in_author {
                    if let Some(a) = authors.last_mut() {
                        a.push_str(&s);
                    }
                } else if in_text {
                    if let Some((_, _, text)) = &mut current {
                        text.push_str(&s);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                let s = entity_text(&r);
                if in_author {
                    if let Some(a) = authors.last_mut() {
                        a.push_str(&s);
                    }
                } else if in_text {
                    if let Some((_, _, text)) = &mut current {
                        text.push_str(&s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML error in comments: {e}")),
            _ => {}
        }
    }
    Ok(notes)
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
    name: Option<String>,
    color: Option<Rgb>,
}

/// One `CellFormat` per `cellXfs` entry, indexed by a cell's `s` attribute.
#[derive(Default)]
struct StyleTable {
    xfs: Vec<CellFormat>,
    /// Conditional formatting styles, indexed by a rule's `dxfId`
    dxfs: Vec<CfStyle>,
}

fn parse_styles(xml: &str, theme: &Theme) -> Result<StyleTable, String> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Fonts,
        Fills,
        Borders,
        CellXfs,
        Dxfs,
        Other,
    }
    let mut section = Section::None;
    let mut num_fmts: HashMap<u32, String> = HashMap::new();
    let mut fonts: Vec<Font> = Vec::new();
    let mut fills: Vec<Option<Rgb>> = Vec::new();
    let mut borders: Vec<Borders> = Vec::new();
    #[derive(Default)]
    struct Xf {
        num_fmt: u32,
        font: usize,
        fill: usize,
        border: usize,
        h_align: HAlign,
        v_align: VAlign,
        wrap: bool,
    }
    let mut xfs: Vec<Xf> = Vec::new();

    let mut font = Font::default();
    let mut fill: Option<Rgb> = None;
    let mut solid_or_pattern = false;
    let mut border = Borders::default();
    // Conditional formats: fills use bgColor, and only some properties are set.
    let mut dxfs: Vec<CfStyle> = Vec::new();
    let mut dxf = CfStyle::default();
    let (mut dxf_in_font, mut dxf_no_fill) = (false, false);
    let (mut dxf_fg, mut dxf_bg): (Option<Rgb>, Option<Rgb>) = (None, None);

    walk(xml, |name, e, _| {
        match (name, e) {
            (b"numFmt", Some(e)) => {
                if let (Some(id), Some(code)) = (attr_num(e, b"numFmtId"), attr(e, b"formatCode")) {
                    if section == Section::Dxfs {
                        dxf.number_format = Some(code.clone());
                    }
                    num_fmts.insert(id, code);
                }
            }
            (b"fonts", Some(_)) => section = Section::Fonts,
            (b"fills", Some(_)) => section = Section::Fills,
            (b"borders", Some(_)) => section = Section::Borders,
            (b"cellXfs", Some(_)) => section = Section::CellXfs,
            (b"dxfs", Some(_)) => section = Section::Dxfs,
            // Named styles also hold xf/font/fill.
            (b"cellStyleXfs" | b"cellStyles" | b"tableStyles" | b"colors", Some(_)) => {
                section = Section::Other
            }

            (b"dxf", Some(_)) if section == Section::Dxfs => {
                dxf = CfStyle::default();
                (dxf_no_fill, dxf_fg, dxf_bg) = (false, None, None);
            }
            (b"dxf", None) if section == Section::Dxfs => {
                if !dxf_no_fill {
                    dxf.fill = dxf_bg.or(dxf_fg);
                }
                dxfs.push(std::mem::take(&mut dxf));
            }
            (b"font", Some(_)) if section == Section::Dxfs => dxf_in_font = true,
            (b"font", None) if section == Section::Dxfs => dxf_in_font = false,
            (b"b", Some(e)) if section == Section::Dxfs => dxf.bold = Some(flag(e)),
            (b"i", Some(e)) if section == Section::Dxfs => dxf.italic = Some(flag(e)),
            (b"u", Some(e)) if section == Section::Dxfs => dxf.underline = Some(flag(e)),
            (b"strike", Some(e)) if section == Section::Dxfs => dxf.strikethrough = Some(flag(e)),
            (b"color", Some(e)) if section == Section::Dxfs && dxf_in_font => {
                dxf.font_color = parse_color(e, theme)
            }
            (b"patternFill", Some(e)) if section == Section::Dxfs => {
                dxf_no_fill = attr(e, b"patternType").as_deref() == Some("none")
            }
            (b"fgColor", Some(e)) if section == Section::Dxfs => dxf_fg = parse_color(e, theme),
            (b"bgColor", Some(e)) if section == Section::Dxfs => dxf_bg = parse_color(e, theme),
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
            (b"name", Some(e)) if section == Section::Fonts => font.name = attr(e, b"val"),
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

            (b"xf", Some(e)) if section == Section::CellXfs => xfs.push(Xf {
                num_fmt: attr_num(e, b"numFmtId").unwrap_or(0),
                font: attr_num(e, b"fontId").unwrap_or(0),
                fill: attr_num(e, b"fillId").unwrap_or(0),
                border: attr_num(e, b"borderId").unwrap_or(0),
                ..Default::default()
            }),
            (b"alignment", Some(e)) if section == Section::CellXfs => {
                if let Some(xf) = xfs.last_mut() {
                    xf.h_align = match attr(e, b"horizontal").as_deref() {
                        Some("left") => HAlign::Left,
                        Some("center" | "centerContinuous") => HAlign::Center,
                        Some("right") => HAlign::Right,
                        _ => HAlign::General,
                    };
                    xf.v_align = match attr(e, b"vertical").as_deref() {
                        Some("top") => VAlign::Top,
                        Some("center") => VAlign::Center,
                        _ => VAlign::Bottom,
                    };
                    xf.wrap = matches!(attr(e, b"wrapText").as_deref(), Some("1" | "true"));
                }
            }
            _ => {}
        }
    })?;

    // Sizes matching the workbook's default font count as default.
    let default_size = fonts.first().and_then(|f| f.size);
    let default_name = fonts.first().and_then(|f| f.name.clone());
    let xfs = xfs
        .into_iter()
        .map(|xf| {
            let font = fonts.get(xf.font).cloned().unwrap_or_default();
            CellFormat {
                bold: font.bold,
                italic: font.italic,
                underline: font.underline,
                strikethrough: font.strikethrough,
                font_size: font
                    .size
                    .filter(|&s| Some(s) != default_size)
                    .map(|s| s.round().clamp(1.0, 255.0) as u8),
                // The workbook's own default font counts as no font name.
                font_name: font
                    .name
                    .clone()
                    .filter(|n| Some(n) != default_name.as_ref()),
                // Black text is the default; leave it to the theme so it reads in dark mode.
                font_color: font.color.filter(|&c| c != Rgb::BLACK),
                fill: fills.get(xf.fill).copied().flatten(),
                h_align: xf.h_align,
                v_align: xf.v_align,
                wrap: xf.wrap,
                borders: borders.get(xf.border).copied().unwrap_or_default(),
                number_format: num_fmts
                    .get(&xf.num_fmt)
                    .cloned()
                    .or_else(|| builtin_number_format(xf.num_fmt).map(str::to_string))
                    .filter(|c| !c.eq_ignore_ascii_case("general")),
            }
        })
        .collect();
    Ok(StyleTable { xfs, dxfs })
}

fn parse_sheet(xml: &str, styles: &StyleTable) -> Result<SheetFormatting, String> {
    let mut formatting = SheetFormatting::default();
    let default_points = excel_height_to_points(15.0);
    let style = |e: &BytesStart, name: &[u8]| -> Option<CellFormat> {
        let format = styles.xfs.get(attr_num::<usize>(e, name)?)?;
        (!format.is_default()).then(|| format.clone())
    };
    // AutoFilter: <autoFilter ref><filterColumn colId><filters><filter val/>
    let mut filter: Option<AutoFilter> = None;
    let mut filter_col: Option<u32> = None;

    walk(xml, |name, e, _| match (name, e) {
        (b"col", Some(e)) => {
            let (Some(min), Some(max)) = (attr_num::<u32>(e, b"min"), attr_num::<u32>(e, b"max"))
            else {
                return;
            };
            let (first, last) = (min.max(1) - 1, max.clamp(1, crate::cell::MAX_COL + 1) - 1);
            let hidden = matches!(attr(e, b"hidden").as_deref(), Some("1" | "true"));
            let width = attr_num::<f64>(e, b"width").map(excel_width_to_points);
            let format = style(e, b"style");
            for col in first..=last {
                if hidden {
                    formatting.hidden_columns.insert(col);
                }
                // Excel's "every column" range carries only a width or style;
                // per-column widths are kept for a reasonable span.
                if let Some(w) = width {
                    if col - first <= 1024 {
                        formatting.column_widths.insert(col, w);
                    }
                }
                if let Some(f) = &format {
                    formatting.column_formats.insert(col, f.clone());
                }
            }
        }
        (b"row", Some(e)) => {
            let Some(r) = attr_num::<u32>(e, b"r").and_then(|r| r.checked_sub(1)) else {
                return;
            };
            if let Some(ht) = attr_num::<f64>(e, b"ht") {
                let points = excel_height_to_points(ht);
                if (points - default_points).abs() > 0.5 {
                    formatting.row_heights.insert(r, points);
                }
            }
            if matches!(attr(e, b"hidden").as_deref(), Some("1" | "true")) {
                formatting.hidden_rows.insert(r);
            }
            if matches!(attr(e, b"customFormat").as_deref(), Some("1" | "true")) {
                if let Some(f) = style(e, b"s") {
                    formatting.row_formats.insert(r, f);
                }
            }
        }
        (b"c", Some(e)) => {
            let Some(coord) = attr(e, b"r").and_then(|r| CellCoord::from_a1(&r)) else {
                return;
            };
            if let Some(format) = style(e, b"s") {
                formatting.set(coord, format);
            }
        }
        (b"mergeCell", Some(e)) => {
            if let Some(range) = attr(e, b"ref").and_then(|r| CellRange::from_a1(&r)) {
                if range.start != range.end {
                    formatting.merges.push(range);
                }
            }
        }
        (b"pane", Some(e)) => {
            if matches!(attr(e, b"state").as_deref(), Some("frozen" | "frozenSplit")) {
                let rows = attr_num::<f64>(e, b"ySplit").unwrap_or(0.0) as u32;
                let cols = attr_num::<f64>(e, b"xSplit").unwrap_or(0.0) as u32;
                formatting.frozen = (rows, cols);
            }
        }
        (b"autoFilter", Some(e)) => {
            filter = attr(e, b"ref")
                .and_then(|r| CellRange::from_a1(&r))
                .map(|range| AutoFilter {
                    range,
                    allowed: BTreeMap::new(),
                });
        }
        (b"filterColumn", Some(e)) => filter_col = attr_num(e, b"colId"),
        (b"filterColumn", None) => filter_col = None,
        (b"filters", Some(e)) => {
            if let (Some(f), Some(col)) = (filter.as_mut(), filter_col) {
                let values = f.allowed.entry(col).or_insert_with(BTreeSet::new);
                if matches!(attr(e, b"blank").as_deref(), Some("1" | "true")) {
                    values.insert(String::new());
                }
            }
        }
        (b"filter", Some(e)) => {
            if let (Some(f), Some(col), Some(val)) = (filter.as_mut(), filter_col, attr(e, b"val"))
            {
                f.allowed.entry(col).or_default().insert(val);
            }
        }
        _ => {}
    })?;
    formatting.filter = filter;
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

        assert_eq!(styles.dxfs.len(), 1);
        assert_eq!(styles.dxfs[0].bold, Some(true));
        assert_eq!(styles.dxfs[0].italic, None);
        assert_eq!(styles.dxfs[0].fill, Some(Rgb(0, 0xFF, 0)));
    }

    #[test]
    fn reads_conditional_formats() {
        let dxfs = vec![CfStyle::preset(0), CfStyle::preset(2)];
        let xml = r##"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">
  <sheetData/>
  <conditionalFormatting sqref="A1:A10 C1:C10">
    <cfRule type="cellIs" dxfId="0" priority="3" operator="between"><formula>1</formula><formula>$B$1</formula></cfRule>
    <cfRule type="containsText" dxfId="1" priority="1" operator="containsText" text="a&amp;b"><formula>NOT(ISERROR(SEARCH("a&amp;b",A1)))</formula></cfRule>
  </conditionalFormatting>
  <conditionalFormatting sqref="B1:B5">
    <cfRule type="top10" dxfId="1" priority="2" rank="3" bottom="1" stopIfTrue="1"/>
    <cfRule type="aboveAverage" dxfId="0" priority="4" aboveAverage="0" equalAverage="1"/>
    <cfRule type="colorScale" priority="5"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/>
      <color rgb="FFF8696B"/><color rgb="FFFFEB84"/><color theme="4"/></colorScale></cfRule>
    <cfRule type="dataBar" priority="6"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>
      <extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}"><x14:id>{1}</x14:id></ext></extLst></cfRule>
    <cfRule type="iconSet" priority="7"><iconSet><cfvo type="percent" val="0"/></iconSet></cfRule>
  </conditionalFormatting>
  <extLst><ext><x14:conditionalFormattings><x14:conditionalFormatting>
    <x14:cfRule type="expression" priority="8"><xm:f>TRUE</xm:f></x14:cfRule>
  </x14:conditionalFormatting></x14:conditionalFormattings></ext></extLst>
</worksheet>"##;
        let rules = parse_conditional(xml, &dxfs, &default_theme()).unwrap();
        assert_eq!(rules.len(), 6, "icon sets and x14 rules are skipped");
        assert_eq!(
            rules[0].rule,
            CfRule::Text {
                rule: TextRule::Contains,
                text: "a&b".into(),
                style: CfStyle::preset(2),
            }
        );
        assert_eq!(rules[0].ranges.len(), 2);
        assert!(rules[1].stop_if_true);
        assert!(matches!(
            rules[1].rule,
            CfRule::Top {
                bottom: true,
                rank: 3,
                percent: false,
                ..
            }
        ));
        assert_eq!(
            rules[2].rule,
            CfRule::CellIs {
                op: CompareOp::Between,
                formula1: "1".into(),
                formula2: Some("$B$1".into()),
                style: CfStyle::preset(0),
            }
        );
        assert!(matches!(
            rules[3].rule,
            CfRule::Average {
                rule: AverageRule::EqualOrBelow,
                ..
            }
        ));
        let CfRule::ColorScale { stops } = &rules[4].rule else {
            panic!("expected a color scale");
        };
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[1].0, Cfvo::percentile(50));
        assert_eq!(stops[2].1, Rgb(0x44, 0x72, 0xC4));
        assert!(matches!(
            rules[5].rule,
            CfRule::DataBar {
                color: DATA_BAR_BLUE,
                ..
            }
        ));
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

    #[test]
    fn reads_sheet_layout() {
        let styles = parse_styles(STYLES, &default_theme()).unwrap();
        let xml = r##"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetViews><sheetView workbookViewId="0"><pane xSplit="1" ySplit="2" topLeftCell="B3" activePane="bottomRight" state="frozen"/></sheetView></sheetViews>
  <cols><col min="2" max="3" width="9.140625" style="1" customWidth="1"/><col min="5" max="5" width="0" hidden="1"/></cols>
  <sheetData>
    <row r="1" s="2" customFormat="1"><c r="A1" t="s"><v>0</v></c></row>
    <row r="4" hidden="1"><c r="A4"><v>1</v></c></row>
  </sheetData>
  <autoFilter ref="A1:C9"><filterColumn colId="1"><filters blank="1"><filter val="Tea"/><filter val="Cake"/></filters></filterColumn></autoFilter>
  <mergeCells count="1"><mergeCell ref="A6:C6"/></mergeCells>
</worksheet>"##;
        let sheet = parse_sheet(xml, &styles).unwrap();
        assert_eq!(sheet.frozen, (2, 1));
        assert!(sheet.column_formats.get(&1).is_some_and(|f| f.bold));
        assert!(sheet.column_formats.get(&2).is_some_and(|f| f.bold));
        assert!(sheet.hidden_columns.contains(&4));
        assert!(sheet.row_formats.get(&0).is_some_and(|f| f.italic));
        assert!(sheet.hidden_rows.contains(&3));
        assert_eq!(sheet.merges, vec![CellRange::from_a1("A6:C6").unwrap()]);
        let filter = sheet.filter.unwrap();
        assert_eq!(filter.range, CellRange::from_a1("A1:C9").unwrap());
        let allowed: Vec<&str> = filter.allowed[&1].iter().map(String::as_str).collect();
        assert_eq!(allowed, vec!["", "Cake", "Tea"]);
    }

    #[test]
    fn reads_vertical_alignment_and_wrap() {
        let xml = STYLES.replace(
            r#"<alignment horizontal="center"/>"#,
            r#"<alignment horizontal="center" vertical="top" wrapText="1"/>"#,
        );
        let styles = parse_styles(&xml, &default_theme()).unwrap();
        assert_eq!(styles.xfs[1].v_align, VAlign::Top);
        assert!(styles.xfs[1].wrap);
        assert!(!styles.xfs[2].wrap);
    }
}
