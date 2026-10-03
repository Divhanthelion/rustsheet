//! Printing and PDF export.
//!
//! [`layout`] turns a sheet into pages of simple drawing operations (fills,
//! lines, text boxes), in points from each page's top-left corner. Those
//! pages are written to a PDF with [`write_pdf`] (embedding the app's own
//! font, so the PDF looks like the screen) or sent to a printer with
//! [`print`] on Windows (GDI, through the standard Print dialog).

use super::grid::GridConfig;
use crate::calc::{CalcEngine, CellResult};
use crate::cell::{CellCoord, CellRange};
use crate::format::{
    CellFormat, DEFAULT_FONT_SIZE, HAlign, Rgb, VAlign, format_general, format_number,
};
use std::collections::BTreeMap;
use std::io::Write;

/// UI points (egui) to print points: the grid is drawn at 96 dpi.
const UI_TO_PT: f32 = 0.75;
const PADDING: f32 = 3.0;
const GRID_GRAY: Rgb = Rgb(200, 200, 200);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn right(&self) -> f32 {
        self.x + self.w
    }
    fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Fill {
        rect: Rect,
        color: Rgb,
    },
    Line {
        from: (f32, f32),
        to: (f32, f32),
        width: f32,
        color: Rgb,
    },
    Text(TextBox),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    /// The cell (or merged range) the text is aligned in
    pub rect: Rect,
    /// Where the text may draw (wider when it spills over empty cells)
    pub clip: Rect,
    pub text: String,
    pub size: f32,
    /// Font family for printing; `None` is the default
    pub font_name: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Rgb,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub wrap: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    Letter,
    A4,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSetup {
    pub paper: Paper,
    pub landscape: bool,
    pub gridlines: bool,
    /// Shrink to fit all columns across one page
    pub fit_width: bool,
    /// Printable page size in points, when a printer decides it
    pub page_override: Option<(f32, f32)>,
    pub margin: f32,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            paper: Paper::Letter,
            landscape: false,
            gridlines: true,
            fit_width: false,
            page_override: None,
            margin: 36.0,
        }
    }
}

impl PageSetup {
    /// Page size in points.
    pub fn page_size(&self) -> (f32, f32) {
        if let Some(size) = self.page_override {
            return size;
        }
        let (w, h) = match self.paper {
            Paper::Letter => (612.0, 792.0),
            Paper::A4 => (595.28, 841.89),
        };
        if self.landscape { (h, w) } else { (w, h) }
    }
}

/// Text measuring for layout decisions (#### and spill-over).
pub trait Measure {
    fn width(&self, text: &str, size: f32) -> f32;
}

/// Lay out `area` of `sheet` as pages. Pages go down, then across, like Excel.
pub fn layout(
    engine: &CalcEngine,
    sheet: u32,
    area: CellRange,
    config: &GridConfig,
    setup: &PageSetup,
    measure: &dyn Measure,
) -> Vec<Page> {
    let (page_w, page_h) = setup.page_size();
    let printable_w = page_w - 2.0 * setup.margin;
    let printable_h = page_h - 2.0 * setup.margin;
    let col_w = |c: u32| config.column_width(c) * UI_TO_PT;
    let row_h = |r: u32| config.row_height(r) * UI_TO_PT;

    let total_w: f32 = (area.start.col..=area.end.col).map(col_w).sum();
    let scale = if setup.fit_width && total_w > printable_w {
        printable_w / total_w
    } else {
        1.0
    };

    // Split columns and rows into page-sized bands, skipping hidden ones.
    let bands = |first: u32, last: u32, size: &dyn Fn(u32) -> f32, room: f32| {
        let mut bands: Vec<Vec<u32>> = vec![Vec::new()];
        let mut used = 0.0;
        for i in first..=last {
            let s = size(i) * scale;
            if s <= 0.0 {
                continue;
            }
            if used + s > room && !bands.last().unwrap().is_empty() {
                bands.push(Vec::new());
                used = 0.0;
            }
            bands.last_mut().unwrap().push(i);
            used += s;
        }
        bands.retain(|b| !b.is_empty());
        bands
    };
    let col_bands = bands(area.start.col, area.end.col, &col_w, printable_w);
    let row_bands = bands(area.start.row, area.end.row, &row_h, printable_h);

    let formatting = engine.formatting(sheet);
    let default_format = CellFormat::default();
    let format_of = |c: CellCoord| {
        formatting
            .and_then(|f| f.effective(c))
            .unwrap_or(&default_format)
    };
    let merge_of = |c: CellCoord| formatting.and_then(|f| f.merge_at(c));

    let mut pages = Vec::new();
    for cols in &col_bands {
        for rows in &row_bands {
            let mut page = Page::default();
            // Screen positions of this page's columns and rows.
            let mut xs = BTreeMap::new();
            let mut x = setup.margin;
            for &c in cols {
                xs.insert(c, (x, col_w(c) * scale));
                x += col_w(c) * scale;
            }
            let mut ys = BTreeMap::new();
            let mut y = setup.margin;
            for &r in rows {
                ys.insert(r, (y, row_h(r) * scale));
                y += row_h(r) * scale;
            }
            let rect_of = |range: CellRange| -> Rect {
                // Clamp to this page's lines; a merge may start off the page.
                let x0 = xs
                    .range(range.start.col..=range.end.col)
                    .next()
                    .map(|(_, v)| v.0);
                let x1 = xs
                    .range(range.start.col..=range.end.col)
                    .next_back()
                    .map(|(_, v)| v.0 + v.1);
                let y0 = ys
                    .range(range.start.row..=range.end.row)
                    .next()
                    .map(|(_, v)| v.0);
                let y1 = ys
                    .range(range.start.row..=range.end.row)
                    .next_back()
                    .map(|(_, v)| v.0 + v.1);
                match (x0, x1, y0, y1) {
                    (Some(x0), Some(x1), Some(y0), Some(y1)) => Rect {
                        x: x0,
                        y: y0,
                        w: x1 - x0,
                        h: y1 - y0,
                    },
                    _ => Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    },
                }
            };

            let mut lines = Vec::new();
            let mut borders = Vec::new();
            let mut texts = Vec::new();
            let mut merges_done = std::collections::HashSet::new();
            for &r in rows {
                for &c in cols {
                    let coord = CellCoord::new(r, c);
                    let (x, w) = xs[&c];
                    let (y, h) = ys[&r];
                    let cell = Rect { x, y, w, h };
                    let merge = merge_of(coord);
                    let owner = merge.map_or(coord, |m| m.start);
                    let format = format_of(owner);
                    if let Some(fill) = format.fill {
                        page.ops.push(Op::Fill {
                            rect: cell,
                            color: fill,
                        });
                    }
                    if setup.gridlines && format.fill.is_none() {
                        if merge.is_none_or(|m| c == m.end.col) {
                            lines.push(((cell.right(), cell.y), (cell.right(), cell.bottom())));
                        }
                        if merge.is_none_or(|m| r == m.end.row) {
                            lines.push(((cell.x, cell.bottom()), (cell.right(), cell.bottom())));
                        }
                    }
                    let b = format_of(coord).borders;
                    let edges = [
                        (b.top, (cell.x, cell.y), (cell.right(), cell.y)),
                        (
                            b.bottom,
                            (cell.x, cell.bottom()),
                            (cell.right(), cell.bottom()),
                        ),
                        (b.left, (cell.x, cell.y), (cell.x, cell.bottom())),
                        (
                            b.right,
                            (cell.right(), cell.y),
                            (cell.right(), cell.bottom()),
                        ),
                    ];
                    borders.extend(edges.into_iter().filter(|e| e.0).map(|e| (e.1, e.2)));
                    match merge {
                        Some(m) => {
                            if merges_done.insert((m.start, m.end)) {
                                texts.push((m.start, rect_of(m), true));
                            }
                        }
                        None => texts.push((coord, cell, false)),
                    }
                }
            }
            if setup.gridlines {
                // Outer edges, so the first row and column are closed.
                let all = rect_of(CellRange::new(
                    CellCoord::new(rows[0], cols[0]),
                    CellCoord::new(*rows.last().unwrap(), *cols.last().unwrap()),
                ));
                lines.push(((all.x, all.y), (all.right(), all.y)));
                lines.push(((all.x, all.y), (all.x, all.bottom())));
            }
            for (from, to) in lines {
                page.ops.push(Op::Line {
                    from,
                    to,
                    width: 0.5,
                    color: GRID_GRAY,
                });
            }

            for (coord, rect, merged) in texts {
                let value = engine.get_value(sheet, coord);
                let format = format_of(coord);
                let size = DEFAULT_FONT_SIZE.max(1) as f32 * format.font_size_or_default() as f32
                    / DEFAULT_FONT_SIZE as f32
                    * scale;
                let Some((mut text, number, format_color)) = cell_text(&value, format) else {
                    continue;
                };
                let inner = rect.w - 2.0 * PADDING;
                if number && measure.width(&text, size) > inner {
                    let n = (inner / measure.width("#", size).max(0.1)).floor().max(1.0) as usize;
                    text = "#".repeat(n);
                }
                let h_align = match (format.h_align, &value) {
                    (HAlign::General, CellResult::Value(_)) => HAlign::Right,
                    (HAlign::General, CellResult::Text(_)) => HAlign::Left,
                    (HAlign::General, _) => HAlign::Center,
                    (a, _) => a,
                };
                // Unwrapped text spills over empty cells to its right.
                let mut clip = rect;
                if !merged && !format.wrap && !number && h_align == HAlign::Left {
                    let needed = measure.width(&text, size) + 2.0 * PADDING;
                    for (&c, &(x, w)) in xs.range(coord.col + 1..) {
                        if clip.w >= needed {
                            break;
                        }
                        let neighbor = CellCoord::new(coord.row, c);
                        if engine.get_input(sheet, neighbor).is_some()
                            || merge_of(neighbor).is_some()
                        {
                            break;
                        }
                        clip.w = x + w - clip.x;
                    }
                }
                let color = match (format_color, format.font_color, format.fill) {
                    (Some(c), _, _) | (None, Some(c), _) => c,
                    (None, None, Some(fill)) if fill.luminance() <= 0.5 => Rgb::WHITE,
                    _ => match value {
                        CellResult::Error(_) => Rgb(192, 0, 0),
                        _ => Rgb::BLACK,
                    },
                };
                page.ops.push(Op::Text(TextBox {
                    rect,
                    clip,
                    text,
                    size,
                    font_name: format.font_name.clone(),
                    bold: format.bold,
                    italic: format.italic,
                    underline: format.underline,
                    strikethrough: format.strikethrough,
                    color,
                    h_align,
                    v_align: format.v_align,
                    wrap: format.wrap,
                }));
            }
            for (from, to) in borders {
                page.ops.push(Op::Line {
                    from,
                    to,
                    width: 0.75,
                    color: Rgb::BLACK,
                });
            }
            pages.push(page);
        }
    }
    pages
}

/// Displayed text, whether it is a number, and a `[Red]`-style color.
fn cell_text(value: &CellResult, format: &CellFormat) -> Option<(String, bool, Option<Rgb>)> {
    Some(match value {
        CellResult::Empty => return None,
        CellResult::Value(n) => match &format.number_format {
            Some(code) => {
                let f = format_number(*n, code);
                (f.text, true, f.color)
            }
            None => (format_general(*n, 11), true, None),
        },
        other => (
            crate::format::display_text(other, Some(format)),
            false,
            None,
        ),
    })
    .filter(|(t, _, _)| !t.is_empty())
}

/// The area to print: every used cell and merge, or `None` if blank.
pub fn print_area(engine: &CalcEngine, sheet: u32) -> Option<CellRange> {
    let mut end = engine.sheet_max_coord(sheet)?;
    if let Some(f) = engine.formatting(sheet) {
        for m in &f.merges {
            end = CellCoord::new(end.row.max(m.end.row), end.col.max(m.end.col));
        }
    }
    Some(CellRange::new(CellCoord::new(0, 0), end))
}

// ----------------------------------------------------------------------
// PDF
// ----------------------------------------------------------------------

/// The embedded font: egui's own, so PDFs match the screen.
pub struct PdfFont {
    data: &'static [u8],
    face: ttf_parser::Face<'static>,
    upem: f32,
}

impl PdfFont {
    pub fn new() -> Option<Self> {
        let data = epaint_default_fonts::UBUNTU_LIGHT;
        let face = ttf_parser::Face::parse(data, 0).ok()?;
        let upem = face.units_per_em() as f32;
        Some(Self { data, face, upem })
    }

    fn glyph(&self, c: char) -> u16 {
        self.face.glyph_index(c).map_or(0, |g| g.0)
    }

    fn advance(&self, gid: u16) -> f32 {
        self.face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0) as f32
            / self.upem
    }

    fn ascent(&self) -> f32 {
        self.face.ascender() as f32 / self.upem
    }

    fn descent(&self) -> f32 {
        self.face.descender() as f32 / self.upem
    }
}

impl Measure for PdfFont {
    fn width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| self.advance(self.glyph(c)))
            .sum::<f32>()
            * size
    }
}

/// Greedy word wrap to `width`.
fn wrap_lines(text: &str, width: f32, size: f32, m: &dyn Measure) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && m.width(&candidate, size) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn rgb_op(c: Rgb, op: &str) -> String {
    format!(
        "{:.3} {:.3} {:.3} {op}",
        c.0 as f32 / 255.0,
        c.1 as f32 / 255.0,
        c.2 as f32 / 255.0
    )
}

/// Render pages to a PDF file's bytes.
pub fn write_pdf(pages: &[Page], setup: &PageSetup, font: &PdfFont, title: &str) -> Vec<u8> {
    let (page_w, page_h) = setup.page_size();
    let mut used_glyphs: BTreeMap<u16, char> = BTreeMap::new();
    let mut contents = Vec::new();

    for page in pages {
        let mut s = String::new();
        for op in &page.ops {
            match op {
                Op::Fill { rect, color } => {
                    s += &format!(
                        "{} {:.2} {:.2} {:.2} {:.2} re f\n",
                        rgb_op(*color, "rg"),
                        rect.x,
                        page_h - rect.bottom(),
                        rect.w,
                        rect.h
                    );
                }
                Op::Line {
                    from,
                    to,
                    width,
                    color,
                } => {
                    s += &format!(
                        "{} {width:.2} w {:.2} {:.2} m {:.2} {:.2} l S\n",
                        rgb_op(*color, "RG"),
                        from.0,
                        page_h - from.1,
                        to.0,
                        page_h - to.1
                    );
                }
                Op::Text(t) => s += &pdf_text(t, page_h, font, &mut used_glyphs),
            }
        }
        contents.push(deflate(s.as_bytes()));
    }

    // Objects: 1 catalog, 2 pages, 3 Type0 font, 4 CID font, 5 descriptor,
    // 6 font file, 7 ToUnicode, 8 info, then a page and its content each.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 9 + 2 * i).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .into_bytes(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type0 /BaseFont /RustSheetSans /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 7 0 R >>"
            .to_vec(),
    );
    let widths: Vec<String> = used_glyphs
        .keys()
        .map(|&g| format!("{g} [{:.0}]", font.advance(g) * 1000.0))
        .collect();
    objects.push(
        format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /RustSheetSans /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 5 0 R /DW 500 /W [{}] /CIDToGIDMap /Identity >>",
            widths.join(" ")
        )
        .into_bytes(),
    );
    let bbox = font.face.global_bounding_box();
    let k = 1000.0 / font.upem;
    objects.push(
        format!(
            "<< /Type /FontDescriptor /FontName /RustSheetSans /Flags 32 /FontBBox [{:.0} {:.0} {:.0} {:.0}] /ItalicAngle 0 /Ascent {:.0} /Descent {:.0} /CapHeight {:.0} /StemV 80 /FontFile2 6 0 R >>",
            bbox.x_min as f32 * k,
            bbox.y_min as f32 * k,
            bbox.x_max as f32 * k,
            bbox.y_max as f32 * k,
            font.ascent() * 1000.0,
            font.descent() * 1000.0,
            font.face.capital_height().unwrap_or(700) as f32 * k,
        )
        .into_bytes(),
    );
    let font_data = deflate(font.data);
    let mut font_obj = format!(
        "<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n",
        font_data.len(),
        font.data.len()
    )
    .into_bytes();
    font_obj.extend_from_slice(&font_data);
    font_obj.extend_from_slice(b"\nendstream");
    objects.push(font_obj);

    // ToUnicode, so text in the PDF can be searched and copied.
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def /CMapType 2 def\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n",
    );
    let entries: Vec<(&u16, &char)> = used_glyphs.iter().collect();
    for chunk in entries.chunks(100) {
        cmap += &format!("{} beginbfchar\n", chunk.len());
        for (g, c) in chunk {
            let mut buf = [0u16; 2];
            let hex: String = c
                .encode_utf16(&mut buf)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            cmap += &format!("<{g:04X}> <{hex}>\n");
        }
        cmap += "endbfchar\n";
    }
    cmap += "endcmap CMapName currentdict /CMap defineresource pop end end";
    let mut cmap_obj = format!("<< /Length {} >>\nstream\n", cmap.len()).into_bytes();
    cmap_obj.extend_from_slice(cmap.as_bytes());
    cmap_obj.extend_from_slice(b"\nendstream");
    objects.push(cmap_obj);

    let title: String = title
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | '\\'))
        .collect();
    objects.push(format!("<< /Title ({title}) /Producer (RustSheet) >>").into_bytes());

    for (i, content) in contents.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w:.2} {page_h:.2}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page_ids[i] + 1
            )
            .into_bytes(),
        );
        let mut obj = format!(
            "<< /Length {} /Filter /FlateDecode >>\nstream\n",
            content.len()
        )
        .into_bytes();
        obj.extend_from_slice(content);
        obj.extend_from_slice(b"\nendstream");
        objects.push(obj);
    }

    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 8 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn pdf_text(t: &TextBox, page_h: f32, font: &PdfFont, used: &mut BTreeMap<u16, char>) -> String {
    let inner_w = t.rect.w - 2.0 * PADDING;
    let lines = if t.wrap {
        wrap_lines(&t.text, inner_w, t.size, font)
    } else {
        vec![t.text.replace('\n', " ")]
    };
    let line_h = t.size * 1.2;
    let block_h = line_h * lines.len() as f32;
    let top = match t.v_align {
        VAlign::Top => t.rect.y + PADDING,
        VAlign::Center => t.rect.y + (t.rect.h - block_h) / 2.0,
        VAlign::Bottom => t.rect.bottom() - PADDING - block_h,
    }
    .max(t.rect.y);
    let mut s = format!(
        "q {:.2} {:.2} {:.2} {:.2} re W n\n{}\n",
        t.clip.x,
        page_h - t.clip.bottom(),
        t.clip.w,
        t.clip.h,
        rgb_op(t.color, "rg")
    );
    if t.bold {
        // No bold face embedded: stroke the outline too.
        s += &format!("{} {:.2} w 2 Tr\n", rgb_op(t.color, "RG"), t.size * 0.04);
    }
    for (i, line) in lines.iter().enumerate() {
        let width = font.width(line, t.size);
        let x = match t.h_align {
            HAlign::Right => t.rect.right() - PADDING - width,
            HAlign::Center => t.rect.x + (t.rect.w - width) / 2.0,
            _ => t.rect.x + PADDING,
        };
        let baseline = top + i as f32 * line_h + (line_h - t.size) / 2.0 + font.ascent() * t.size;
        let y = page_h - baseline;
        let hex: String = line
            .chars()
            .map(|c| {
                let g = font.glyph(c);
                used.entry(g).or_insert(c);
                format!("{g:04X}")
            })
            .collect();
        let skew = if t.italic { 0.2 } else { 0.0 };
        s += &format!(
            "BT /F1 {:.2} Tf 1 0 {skew} 1 {x:.2} {y:.2} Tm <{hex}> Tj ET\n",
            t.size
        );
        let mut decorate = |offset: f32| {
            s += &format!(
                "{} {:.2} w {x:.2} {:.2} m {:.2} {:.2} l S\n",
                rgb_op(t.color, "RG"),
                t.size * 0.06,
                y + offset,
                x + width,
                y + offset
            );
        };
        if t.underline {
            decorate(-t.size * 0.12);
        }
        if t.strikethrough {
            decorate(t.size * 0.3);
        }
    }
    s += "Q\n";
    s
}

// ----------------------------------------------------------------------
// Printing (Windows)
// ----------------------------------------------------------------------

/// Show the Print dialog and print. `layout_for` lays out pages for the
/// chosen printer's page size. Returns `Ok(false)` if the user cancelled.
#[cfg(windows)]
pub fn print(doc_name: &str, layout_for: impl Fn((f32, f32)) -> Vec<Page>) -> Result<bool, String> {
    use windows_sys::Win32::Foundation::{COLORREF, GlobalFree, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
    use windows_sys::Win32::UI::Controls::Dialogs::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn color(c: Rgb) -> COLORREF {
        c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16
    }

    // SAFETY: PRINTDLGW is plain data; zeroed is its documented initial state.
    let mut pd: PRINTDLGW = unsafe { std::mem::zeroed() };
    pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
    pd.Flags = PD_RETURNDC | PD_NOPAGENUMS | PD_NOSELECTION | PD_USEDEVMODECOPIESANDCOLLATE;
    pd.nCopies = 1;
    // SAFETY: pd is initialized above and outlives the call.
    if unsafe { PrintDlgW(&mut pd) } == 0 {
        return Ok(false);
    }
    let dc = pd.hDC;
    let free = |pd: &PRINTDLGW| unsafe {
        if !pd.hDevMode.is_null() {
            GlobalFree(pd.hDevMode);
        }
        if !pd.hDevNames.is_null() {
            GlobalFree(pd.hDevNames);
        }
    };
    if dc.is_null() {
        free(&pd);
        return Err("The printer couldn't be opened".into());
    }

    let result = (|| -> Result<(), String> {
        // SAFETY: dc is a valid printer DC from PrintDlgW for this block.
        unsafe {
            let dpi_x = GetDeviceCaps(dc, LOGPIXELSX as i32) as f32;
            let dpi_y = GetDeviceCaps(dc, LOGPIXELSY as i32) as f32;
            let page_pts = (
                GetDeviceCaps(dc, HORZRES as i32) as f32 * 72.0 / dpi_x,
                GetDeviceCaps(dc, VERTRES as i32) as f32 * 72.0 / dpi_y,
            );
            let pages = layout_for(page_pts);
            let px = |pt: f32| (pt * dpi_x / 72.0).round() as i32;
            let py = |pt: f32| (pt * dpi_y / 72.0).round() as i32;
            let to_rect = |r: &Rect| RECT {
                left: px(r.x),
                top: py(r.y),
                right: px(r.right()),
                bottom: py(r.bottom()),
            };

            let name = wide(doc_name);
            let info = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: name.as_ptr(),
                lpszOutput: std::ptr::null(),
                lpszDatatype: std::ptr::null(),
                fwType: 0,
            };
            if StartDocW(dc, &info) <= 0 {
                return Err("The print job couldn't be started".into());
            }
            SetBkMode(dc, TRANSPARENT as i32);
            let default_face = wide("Segoe UI");
            for page in &pages {
                if StartPage(dc) <= 0 {
                    break;
                }
                for op in &page.ops {
                    match op {
                        Op::Fill { rect, color: c } => {
                            let brush = CreateSolidBrush(color(*c));
                            FillRect(dc, &to_rect(rect), brush);
                            DeleteObject(brush);
                        }
                        Op::Line {
                            from,
                            to,
                            width,
                            color: c,
                        } => {
                            let pen = CreatePen(PS_SOLID, px(*width).max(1), color(*c));
                            let old = SelectObject(dc, pen);
                            let mut p = POINT { x: 0, y: 0 };
                            MoveToEx(dc, px(from.0), py(from.1), &mut p);
                            LineTo(dc, px(to.0), py(to.1));
                            SelectObject(dc, old);
                            DeleteObject(pen);
                        }
                        Op::Text(t) => {
                            let face = t.font_name.as_deref().map(wide);
                            let face_ptr =
                                face.as_ref().map_or(default_face.as_ptr(), |f| f.as_ptr());
                            let font = CreateFontW(
                                -py(t.size),
                                0,
                                0,
                                0,
                                if t.bold { FW_BOLD as i32 } else { 400 },
                                t.italic as u32,
                                t.underline as u32,
                                t.strikethrough as u32,
                                DEFAULT_CHARSET as u32,
                                0,
                                0,
                                ANTIALIASED_QUALITY as u32,
                                0,
                                face_ptr,
                            );
                            let old = SelectObject(dc, font);
                            SetTextColor(dc, color(t.color));
                            let saved = SaveDC(dc);
                            let clip = to_rect(&t.clip);
                            IntersectClipRect(dc, clip.left, clip.top, clip.right, clip.bottom);
                            let mut r = to_rect(&Rect {
                                x: t.rect.x + PADDING,
                                y: t.rect.y + 1.0,
                                w: (t.clip.w.max(t.rect.w)) - 2.0 * PADDING,
                                h: t.rect.h - 2.0,
                            });
                            if t.h_align != HAlign::Left || !t.wrap {
                                // Right/centered text aligns in its own cell.
                                r.right = px(t.rect.right() - PADDING);
                                if t.h_align == HAlign::Left {
                                    r.right = px(t.clip.right() - PADDING);
                                }
                            }
                            let mut flags = DT_NOPREFIX
                                | match t.h_align {
                                    HAlign::Right => DT_RIGHT,
                                    HAlign::Center => DT_CENTER,
                                    _ => DT_LEFT,
                                };
                            flags |= if t.wrap {
                                DT_WORDBREAK
                            } else {
                                DT_SINGLELINE
                                    | match t.v_align {
                                        VAlign::Top => DT_TOP,
                                        VAlign::Center => DT_VCENTER,
                                        VAlign::Bottom => DT_BOTTOM,
                                    }
                            };
                            let text: Vec<u16> = t.text.encode_utf16().collect();
                            DrawTextW(dc, text.as_ptr(), text.len() as i32, &mut r, flags);
                            RestoreDC(dc, saved);
                            SelectObject(dc, old);
                            DeleteObject(font);
                        }
                    }
                }
                EndPage(dc);
            }
            EndDoc(dc);
        }
        Ok(())
    })();
    // SAFETY: dc came from PrintDlgW and is not used after this.
    unsafe {
        DeleteDC(dc);
    }
    free(&pd);
    result.map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    #[test]
    fn wide_sheets_split_across_pages() {
        let mut e = CalcEngine::new();
        for col in 0..20 {
            e.set_value(
                0,
                CellCoord::new(0, col),
                CellValueInput::Number(col as f64),
            );
        }
        e.set_value(
            0,
            CellCoord::new(120, 0),
            CellValueInput::Text("last".into()),
        );
        let area = print_area(&e, 0).unwrap();
        let config = GridConfig::default();
        let setup = PageSetup::default();
        let pages = layout(&e, 0, area, &config, &setup, &Fixed);
        // 20 columns of 60pt don't fit 540pt; 121 rows of 16.5pt don't fit 720pt.
        assert_eq!(pages.len(), 3 * 3);
        let fit = PageSetup {
            fit_width: true,
            ..setup
        };
        let pages = layout(&e, 0, area, &config, &fit, &Fixed);
        assert_eq!(pages.len(), 2, "fit to width scales rows too");
    }

    #[test]
    fn text_spills_and_numbers_overflow_to_hashes() {
        let mut e = CalcEngine::new();
        e.set_value(
            0,
            CellCoord::new(0, 0),
            CellValueInput::Text("a long heading here".into()),
        );
        e.set_value(
            0,
            CellCoord::new(1, 0),
            CellValueInput::Number(123456789012345.0),
        );
        e.set_cell_format(
            0,
            CellCoord::new(1, 0),
            CellFormat {
                number_format: Some("#,##0".into()),
                ..Default::default()
            },
        );
        let area = CellRange::new(CellCoord::new(0, 0), CellCoord::new(1, 3));
        let pages = layout(
            &e,
            0,
            area,
            &GridConfig::default(),
            &PageSetup::default(),
            &Fixed,
        );
        let texts: Vec<&TextBox> = pages[0]
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert!(texts[0].clip.w > texts[0].rect.w, "heading spills right");
        assert!(texts[1].text.chars().all(|c| c == '#'));
    }

    #[test]
    fn pdf_is_well_formed() {
        let mut e = CalcEngine::new();
        e.set_value(
            0,
            CellCoord::new(0, 0),
            CellValueInput::Text("Hello, PDF".into()),
        );
        e.set_value(0, CellCoord::new(1, 1), CellValueInput::Number(42.0));
        let font = PdfFont::new().unwrap();
        let area = print_area(&e, 0).unwrap();
        let pages = layout(
            &e,
            0,
            area,
            &GridConfig::default(),
            &PageSetup::default(),
            &font,
        );
        let pdf = write_pdf(&pages, &PageSetup::default(), &font, "Test");
        assert!(pdf.starts_with(b"%PDF-1.7"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 1"));
        // The xref offset points at the xref table.
        let start = text.rfind("startxref\n").unwrap() + "startxref\n".len();
        let offset: usize = text[start..].lines().next().unwrap().parse().unwrap();
        assert!(pdf[offset..].starts_with(b"xref"));
    }
}
