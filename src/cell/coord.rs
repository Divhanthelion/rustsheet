use serde::{Deserialize, Serialize};
use std::fmt;

/// A cell coordinate (row, column) using 0-based indexing internally.
/// Supports up to 2^32 rows and columns (far exceeding Excel's limits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CellCoord {
    pub row: u32,
    pub col: u32,
}

impl CellCoord {
    pub const fn new(row: u32, col: u32) -> Self {
        Self { row, col }
    }

    /// Parse A1-style reference (e.g., "A1", "AA100", "$B$2")
    pub fn from_a1(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut col_str = String::new();
        let mut row_str = String::new();
        let mut in_col = true;

        for c in s.chars() {
            if c == '$' {
                continue; // Skip absolute markers
            }
            if in_col && c.is_ascii_alphabetic() {
                col_str.push(c.to_ascii_uppercase());
            } else if c.is_ascii_digit() {
                in_col = false;
                row_str.push(c);
            } else {
                return None;
            }
        }

        if col_str.is_empty() || row_str.is_empty() {
            return None;
        }

        let col = col_from_letters(&col_str)?;
        let row: u32 = row_str.parse().ok()?;

        if row == 0 {
            return None; // A1 notation is 1-based
        }

        Some(Self::new(row - 1, col))
    }

    /// Convert to A1-style reference
    pub fn to_a1(&self) -> String {
        format!("{}{}", col_to_letters(self.col), self.row + 1)
    }
}

impl fmt::Display for CellCoord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// A rectangular range of cells
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

impl CellRange {
    pub fn new(start: CellCoord, end: CellCoord) -> Self {
        // Normalize so start <= end
        Self {
            start: CellCoord::new(start.row.min(end.row), start.col.min(end.col)),
            end: CellCoord::new(start.row.max(end.row), start.col.max(end.col)),
        }
    }

    pub fn single(coord: CellCoord) -> Self {
        Self { start: coord, end: coord }
    }

    /// Parse A1:B2 style range
    pub fn from_a1(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            1 => Some(Self::single(CellCoord::from_a1(parts[0])?)),
            2 => {
                let start = CellCoord::from_a1(parts[0])?;
                let end = CellCoord::from_a1(parts[1])?;
                Some(Self::new(start, end))
            }
            _ => None,
        }
    }

    pub fn width(&self) -> u32 {
        self.end.col - self.start.col + 1
    }

    pub fn height(&self) -> u32 {
        self.end.row - self.start.row + 1
    }

    pub fn cell_count(&self) -> u64 {
        self.width() as u64 * self.height() as u64
    }

    pub fn contains(&self, coord: CellCoord) -> bool {
        coord.row >= self.start.row
            && coord.row <= self.end.row
            && coord.col >= self.start.col
            && coord.col <= self.end.col
    }

    /// Iterate over all cells in the range (row-major order)
    pub fn iter(&self) -> impl Iterator<Item = CellCoord> {
        let start_row = self.start.row;
        let end_row = self.end.row;
        let start_col = self.start.col;
        let end_col = self.end.col;

        (start_row..=end_row)
            .flat_map(move |row| (start_col..=end_col).map(move |col| CellCoord::new(row, col)))
    }

    /// Convert to A1-style string representation
    pub fn to_a1(&self) -> String {
        if self.start == self.end {
            self.start.to_a1()
        } else {
            format!("{}:{}", self.start.to_a1(), self.end.to_a1())
        }
    }
}

impl fmt::Display for CellRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}:{}", self.start, self.end)
        }
    }
}

/// Convert column letters to 0-based index (A=0, Z=25, AA=26, etc.)
fn col_from_letters(s: &str) -> Option<u32> {
    let mut result: u32 = 0;
    for c in s.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        result = result.checked_mul(26)?;
        result = result.checked_add((c.to_ascii_uppercase() as u32) - ('A' as u32) + 1)?;
    }
    Some(result.saturating_sub(1))
}

/// Convert 0-based column index to letters
fn col_to_letters(mut col: u32) -> String {
    let mut result = String::new();
    col += 1; // Convert to 1-based for calculation
    while col > 0 {
        col -= 1;
        result.insert(0, (b'A' + (col % 26) as u8) as char);
        col /= 26;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord_parsing() {
        assert_eq!(CellCoord::from_a1("A1"), Some(CellCoord::new(0, 0)));
        assert_eq!(CellCoord::from_a1("B2"), Some(CellCoord::new(1, 1)));
        assert_eq!(CellCoord::from_a1("Z1"), Some(CellCoord::new(0, 25)));
        assert_eq!(CellCoord::from_a1("AA1"), Some(CellCoord::new(0, 26)));
        assert_eq!(CellCoord::from_a1("$A$1"), Some(CellCoord::new(0, 0)));
    }

    #[test]
    fn test_coord_to_a1() {
        assert_eq!(CellCoord::new(0, 0).to_a1(), "A1");
        assert_eq!(CellCoord::new(0, 25).to_a1(), "Z1");
        assert_eq!(CellCoord::new(0, 26).to_a1(), "AA1");
    }

    #[test]
    fn test_range_iter() {
        let range = CellRange::from_a1("A1:B2").unwrap();
        let cells: Vec<_> = range.iter().collect();
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0], CellCoord::new(0, 0));
        assert_eq!(cells[3], CellCoord::new(1, 1));
    }
}
