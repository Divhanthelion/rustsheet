//! Cell presentation: fonts, fills, borders, alignment and number formats.
//!
//! Formatting is stored per sheet in [`SheetFormatting`], next to the cells in
//! `CalcEngine`, and is independent of cell values: clearing a cell keeps its
//! format, as in Excel.

pub mod conditional;
mod input;
mod number;
pub mod picture;
pub mod validation;

pub use input::parse_typed_number;
pub use number::{
    FormattedNumber, builtin_number_format, format_general, format_number, is_date_format,
};

use crate::cell::{Axis, CellCoord, CellRange, LineEdit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A cell's value as shown on screen (General numbers get up to 15 characters).
pub fn display_text(value: &crate::calc::CellResult, format: Option<&CellFormat>) -> String {
    use crate::calc::CellResult;
    match value {
        CellResult::Empty => String::new(),
        CellResult::Value(n) => match format.and_then(|f| f.number_format.as_deref()) {
            Some(code) => format_number(*n, code).text,
            None => format_general(*n, 15),
        },
        CellResult::Text(s) => s.clone(),
        CellResult::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellResult::Error(e) => e.as_str().to_string(),
    }
}

/// Font size Excel uses when a cell has none set.
pub const DEFAULT_FONT_SIZE: u8 = 11;

/// An sRGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    /// Parse `RRGGBB` or `AARRGGBB` hex, with or without a leading `#`.
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let s = s.trim_start_matches('#');
        let s = match s.len() {
            8 => &s[2..],
            6 => s,
            _ => return None,
        };
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// `0xRRGGBB`.
    pub fn to_u32(self) -> u32 {
        (self.0 as u32) << 16 | (self.1 as u32) << 8 | self.2 as u32
    }

    /// Perceived brightness in 0..=1, for picking readable text on a fill.
    pub fn luminance(self) -> f32 {
        (0.2126 * self.0 as f32 + 0.7152 * self.1 as f32 + 0.0722 * self.2 as f32) / 255.0
    }
}

/// Horizontal alignment. `General` puts numbers right and text left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
}

/// Vertical alignment. Excel's default is bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VAlign {
    #[default]
    Bottom,
    Center,
    Top,
}

/// Which cell edges have a thin border.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Borders {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl Borders {
    pub const NONE: Borders = Borders {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };
    pub const ALL: Borders = Borders {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };

    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

/// How one cell looks. The default is unformatted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Font size in points; `None` is [`DEFAULT_FONT_SIZE`].
    pub font_size: Option<u8>,
    /// Font family, e.g. "Arial"; `None` is the workbook's default font.
    pub font_name: Option<String>,
    /// Text color; `None` follows the theme.
    pub font_color: Option<Rgb>,
    /// Solid background fill.
    pub fill: Option<Rgb>,
    pub h_align: HAlign,
    pub v_align: VAlign,
    /// Wrap text onto several lines within the cell.
    pub wrap: bool,
    pub borders: Borders,
    /// Excel number format code such as `#,##0.00` or `yyyy-mm-dd`;
    /// `None` is General.
    pub number_format: Option<String>,
}

impl CellFormat {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn font_size_or_default(&self) -> u8 {
        self.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }
}

/// A note (Excel's legacy comment) attached to a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub author: Option<String>,
}

/// An AutoFilter: a header row plus the data below it. Rows whose value in a
/// filtered column is not in that column's allowed set are hidden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoFilter {
    /// Header row through the last data row.
    pub range: CellRange,
    /// Allowed displayed values, by column; unlisted columns allow all.
    pub allowed: BTreeMap<u32, BTreeSet<String>>,
}

/// Cell formats, sizes and sheet layout (merges, frozen panes, hidden lines,
/// filter) for one sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetFormatting {
    cells: HashMap<CellCoord, CellFormat>,
    /// Whole-column formats, used where a cell has none of its own
    pub column_formats: BTreeMap<u32, CellFormat>,
    /// Whole-row formats; they win over column formats
    pub row_formats: BTreeMap<u32, CellFormat>,
    /// Column widths in UI points, only where they differ from the default.
    pub column_widths: BTreeMap<u32, f32>,
    /// Row heights in UI points, only where they differ from the default.
    pub row_heights: BTreeMap<u32, f32>,
    pub hidden_rows: BTreeSet<u32>,
    pub hidden_columns: BTreeSet<u32>,
    /// Merged ranges; the top-left cell holds the value
    pub merges: Vec<CellRange>,
    /// Rows and columns kept on screen while scrolling: (rows, columns)
    pub frozen: (u32, u32),
    pub filter: Option<AutoFilter>,
    /// Notes, by cell
    pub notes: BTreeMap<CellCoord, Note>,
    /// Data validation rules; where they overlap, the last one applies
    pub validations: Vec<validation::DataValidation>,
    /// Conditional formatting rules, highest priority first
    pub conditional: Vec<conditional::ConditionalFormat>,
    /// Pictures, back to front
    pub pictures: Vec<picture::Picture>,
}

impl SheetFormatting {
    /// The cell's own format (not row or column formats).
    pub fn get(&self, coord: CellCoord) -> Option<&CellFormat> {
        self.cells.get(&coord)
    }

    /// The format a cell shows: its own, else its row's, else its column's.
    pub fn effective(&self, coord: CellCoord) -> Option<&CellFormat> {
        self.cells
            .get(&coord)
            .or_else(|| self.row_formats.get(&coord.row))
            .or_else(|| self.column_formats.get(&coord.col))
    }

    pub fn line_format(&self, axis: Axis, index: u32) -> Option<&CellFormat> {
        match axis {
            Axis::Row => self.row_formats.get(&index),
            Axis::Column => self.column_formats.get(&index),
        }
    }

    /// Set a whole row's or column's format. The default removes it.
    pub fn set_line_format(&mut self, axis: Axis, index: u32, format: CellFormat) {
        let map = match axis {
            Axis::Row => &mut self.row_formats,
            Axis::Column => &mut self.column_formats,
        };
        if format.is_default() {
            map.remove(&index);
        } else {
            map.insert(index, format);
        }
    }

    /// The merged range covering `coord`, if any.
    pub fn merge_at(&self, coord: CellCoord) -> Option<CellRange> {
        self.merges.iter().copied().find(|m| {
            (m.start.row..=m.end.row).contains(&coord.row)
                && (m.start.col..=m.end.col).contains(&coord.col)
        })
    }

    pub fn is_hidden(&self, axis: Axis, index: u32) -> bool {
        match axis {
            Axis::Row => self.hidden_rows.contains(&index),
            Axis::Column => self.hidden_columns.contains(&index),
        }
    }

    /// Shift everything for an inserted or deleted block of rows/columns.
    pub fn apply_line_edit(&mut self, edit: &LineEdit) {
        fn map_keys<T>(m: &mut BTreeMap<u32, T>, edit: &LineEdit) {
            *m = std::mem::take(m)
                .into_iter()
                .filter_map(|(i, v)| Some((edit.map_index(i)?, v)))
                .collect();
        }
        let map_set = |s: &mut BTreeSet<u32>| {
            *s = std::mem::take(s)
                .into_iter()
                .filter_map(|i| edit.map_index(i))
                .collect();
        };
        self.cells = std::mem::take(&mut self.cells)
            .into_iter()
            .filter_map(|(c, f)| Some((edit.map_coord(c)?, f)))
            .collect();
        self.notes = std::mem::take(&mut self.notes)
            .into_iter()
            .filter_map(|(c, n)| Some((edit.map_coord(c)?, n)))
            .collect();
        validation::apply_line_edit(&mut self.validations, edit);
        conditional::apply_line_edit(&mut self.conditional, edit);
        picture::apply_line_edit(&mut self.pictures, edit);
        match edit.axis {
            Axis::Row => {
                map_keys(&mut self.row_formats, edit);
                map_keys(&mut self.row_heights, edit);
                map_set(&mut self.hidden_rows);
            }
            Axis::Column => {
                map_keys(&mut self.column_formats, edit);
                map_keys(&mut self.column_widths, edit);
                map_set(&mut self.hidden_columns);
            }
        }
        // A new line inside a merge widens it, like Excel.
        self.merges = std::mem::take(&mut self.merges)
            .into_iter()
            .filter_map(|m| edit.map_range(m))
            .filter(|m| m.start != m.end)
            .collect();
        let frozen = match edit.axis {
            Axis::Row => &mut self.frozen.0,
            Axis::Column => &mut self.frozen.1,
        };
        if *frozen > 0 {
            // Frozen counts lines, so map the last frozen line.
            *frozen = match edit.map_range(CellRange::new(
                CellCoord::new(0, 0),
                match edit.axis {
                    Axis::Row => CellCoord::new(*frozen - 1, 0),
                    Axis::Column => CellCoord::new(0, *frozen - 1),
                },
            )) {
                Some(r) => edit.axis.of(r.end) + 1,
                None => 0,
            };
        }
        if let Some(filter) = &mut self.filter {
            match edit.map_range(filter.range) {
                Some(range) => {
                    let start_col = filter.range.start.col;
                    if edit.axis == Axis::Column {
                        filter.allowed = std::mem::take(&mut filter.allowed)
                            .into_iter()
                            .filter_map(|(c, v)| {
                                let col = edit.map_index(start_col + c)?;
                                Some((col.checked_sub(range.start.col)?, v))
                            })
                            .collect();
                    }
                    filter.range = range;
                }
                None => self.filter = None,
            }
        }
    }

    /// Set a cell's format. The default format removes the entry.
    pub fn set(&mut self, coord: CellCoord, format: CellFormat) {
        if format.is_default() {
            self.cells.remove(&coord);
        } else {
            self.cells.insert(coord, format);
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = (CellCoord, &CellFormat)> {
        self.cells.iter().map(|(&c, f)| (c, f))
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_format_is_not_stored() {
        let mut f = SheetFormatting::default();
        let a1 = CellCoord::new(0, 0);
        f.set(
            a1,
            CellFormat {
                bold: true,
                ..Default::default()
            },
        );
        assert!(f.get(a1).is_some_and(|c| c.bold));
        f.set(a1, CellFormat::default());
        assert!(f.get(a1).is_none());
        assert!(f.is_empty());
    }

    #[test]
    fn effective_format_prefers_cell_then_row_then_column() {
        let mut f = SheetFormatting::default();
        let bold = CellFormat {
            bold: true,
            ..Default::default()
        };
        let italic = CellFormat {
            italic: true,
            ..Default::default()
        };
        let red = CellFormat {
            font_color: Some(Rgb(255, 0, 0)),
            ..Default::default()
        };
        f.set_line_format(Axis::Column, 1, bold.clone());
        f.set_line_format(Axis::Row, 2, italic.clone());
        f.set(CellCoord::new(2, 1), red.clone());
        assert_eq!(f.effective(CellCoord::new(5, 1)), Some(&bold));
        assert_eq!(f.effective(CellCoord::new(2, 4)), Some(&italic));
        assert_eq!(f.effective(CellCoord::new(2, 1)), Some(&red));
        assert_eq!(f.effective(CellCoord::new(0, 0)), None);
    }

    #[test]
    fn line_edits_move_layout() {
        let mut f = SheetFormatting::default();
        let r = |a1: &str| CellRange::from_a1(a1).unwrap();
        f.set(
            CellCoord::new(4, 0),
            CellFormat {
                bold: true,
                ..Default::default()
            },
        );
        f.row_heights.insert(4, 40.0);
        f.hidden_rows.insert(6);
        f.merges.push(r("A3:B6"));
        f.frozen = (3, 0);
        f.filter = Some(AutoFilter {
            range: r("A1:C10"),
            allowed: BTreeMap::new(),
        });

        // Insert two rows above row 2 (index 1).
        f.apply_line_edit(&LineEdit::insert(Axis::Row, 1, 2));
        assert!(f.get(CellCoord::new(6, 0)).is_some_and(|c| c.bold));
        assert_eq!(f.row_heights.get(&6), Some(&40.0));
        assert!(f.hidden_rows.contains(&8));
        assert_eq!(f.merges, vec![r("A5:B8")]);
        assert_eq!(f.frozen, (5, 0));
        assert_eq!(f.filter.as_ref().unwrap().range, r("A1:C12"));

        // Delete the merge's rows: the merge goes away.
        f.apply_line_edit(&LineEdit::delete(Axis::Row, 4, 4));
        assert!(f.merges.is_empty());
        assert!(f.get(CellCoord::new(6, 0)).is_none());
        assert_eq!(f.frozen, (4, 0));
    }

    #[test]
    fn hex_colors() {
        assert_eq!(Rgb::from_hex("FFFF0000"), Some(Rgb(255, 0, 0)));
        assert_eq!(Rgb::from_hex("#00ff00"), Some(Rgb(0, 255, 0)));
        assert_eq!(Rgb::from_hex("123"), None);
        assert_eq!(Rgb(0x12, 0x34, 0x56).to_u32(), 0x123456);
    }
}
