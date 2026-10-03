//! Cell presentation: fonts, fills, borders, alignment and number formats.
//!
//! Formatting is stored per sheet in [`SheetFormatting`], next to the cells in
//! `CalcEngine`, and is independent of cell values: clearing a cell keeps its
//! format, as in Excel.

mod input;
mod number;

pub use input::parse_typed_number;
pub use number::{
    FormattedNumber, builtin_number_format, format_general, format_number, is_date_format,
};

use crate::cell::CellCoord;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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
    /// Text color; `None` follows the theme.
    pub font_color: Option<Rgb>,
    /// Solid background fill.
    pub fill: Option<Rgb>,
    pub h_align: HAlign,
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

/// Cell formats and row/column sizes for one sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetFormatting {
    cells: HashMap<CellCoord, CellFormat>,
    /// Column widths in UI points, only where they differ from the default.
    pub column_widths: BTreeMap<u32, f32>,
    /// Row heights in UI points, only where they differ from the default.
    pub row_heights: BTreeMap<u32, f32>,
}

impl SheetFormatting {
    pub fn get(&self, coord: CellCoord) -> Option<&CellFormat> {
        self.cells.get(&coord)
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
        self.cells.is_empty() && self.column_widths.is_empty() && self.row_heights.is_empty()
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
    fn hex_colors() {
        assert_eq!(Rgb::from_hex("FFFF0000"), Some(Rgb(255, 0, 0)));
        assert_eq!(Rgb::from_hex("#00ff00"), Some(Rgb(0, 255, 0)));
        assert_eq!(Rgb::from_hex("123"), None);
        assert_eq!(Rgb(0x12, 0x34, 0x56).to_u32(), 0x123456);
    }
}
