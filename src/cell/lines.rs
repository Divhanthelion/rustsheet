//! Inserting and deleting whole rows or columns: where an index, cell or
//! range ends up afterwards. Cells, formulas, formats, merges and charts all
//! use the same rules, so they stay in step.

use super::{CellCoord, CellRange};
use serde::{Deserialize, Serialize};

/// Excel's last row and column index.
pub const MAX_ROW: u32 = 1_048_575;
pub const MAX_COL: u32 = 16_383;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    Row,
    Column,
}

impl Axis {
    pub fn max(self) -> u32 {
        match self {
            Axis::Row => MAX_ROW,
            Axis::Column => MAX_COL,
        }
    }

    /// The coordinate's index along this axis.
    pub fn of(self, coord: CellCoord) -> u32 {
        match self {
            Axis::Row => coord.row,
            Axis::Column => coord.col,
        }
    }

    /// The coordinate moved to `index` along this axis.
    pub fn with(self, coord: CellCoord, index: u32) -> CellCoord {
        match self {
            Axis::Row => CellCoord::new(index, coord.col),
            Axis::Column => CellCoord::new(coord.row, index),
        }
    }
}

/// Insert or delete `count` rows or columns starting at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEdit {
    pub axis: Axis,
    pub at: u32,
    pub count: u32,
    pub insert: bool,
}

impl LineEdit {
    pub fn insert(axis: Axis, at: u32, count: u32) -> Self {
        Self {
            axis,
            at,
            count,
            insert: true,
        }
    }

    pub fn delete(axis: Axis, at: u32, count: u32) -> Self {
        Self {
            axis,
            at,
            count,
            insert: false,
        }
    }

    /// Where line `i` goes; `None` if it is deleted or pushed off the sheet.
    pub fn map_index(&self, i: u32) -> Option<u32> {
        if self.insert {
            if i < self.at {
                Some(i)
            } else {
                i.checked_add(self.count).filter(|&n| n <= self.axis.max())
            }
        } else if i < self.at {
            Some(i)
        } else if i < self.at.saturating_add(self.count) {
            None
        } else {
            Some(i - self.count)
        }
    }

    pub fn map_coord(&self, coord: CellCoord) -> Option<CellCoord> {
        let i = self.map_index(self.axis.of(coord))?;
        Some(self.axis.with(coord, i))
    }

    /// A range grows when lines are inserted inside it and shrinks when lines
    /// inside it are deleted. `None` when every line it covered is deleted.
    pub fn map_range(&self, range: CellRange) -> Option<CellRange> {
        let (start, end) = (self.axis.of(range.start), self.axis.of(range.end));
        let (new_start, new_end) = if self.insert {
            let start = self.map_index(start)?;
            let end = self.map_index(end).unwrap_or(self.axis.max());
            (start, end)
        } else {
            let first_after = self.at;
            let new_start = self.map_index(start).unwrap_or(first_after);
            let new_end = match self.map_index(end) {
                Some(e) => e,
                None if self.at == 0 => return None,
                None => self.at - 1,
            };
            if new_end < new_start || (start >= self.at && end < self.at + self.count) {
                return None;
            }
            (new_start, new_end)
        };
        Some(CellRange::new(
            self.axis.with(range.start, new_start),
            self.axis.with(range.end, new_end),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(a1: &str) -> CellRange {
        CellRange::from_a1(a1).unwrap()
    }

    #[test]
    fn inserting_rows() {
        // Insert 2 rows above row 3 (index 2).
        let e = LineEdit::insert(Axis::Row, 2, 2);
        assert_eq!(e.map_index(1), Some(1));
        assert_eq!(e.map_index(2), Some(4));
        assert_eq!(e.map_index(MAX_ROW), None);
        assert_eq!(e.map_range(r("A1:A2")), Some(r("A1:A2")));
        assert_eq!(e.map_range(r("A1:A5")), Some(r("A1:A7")), "inside: grows");
        assert_eq!(e.map_range(r("A3:A5")), Some(r("A5:A7")), "at start: moves");
    }

    #[test]
    fn deleting_columns() {
        // Delete columns B:C (index 1, count 2).
        let e = LineEdit::delete(Axis::Column, 1, 2);
        assert_eq!(e.map_index(0), Some(0));
        assert_eq!(e.map_index(1), None);
        assert_eq!(e.map_index(3), Some(1));
        assert_eq!(e.map_range(r("A1:E1")), Some(r("A1:C1")), "spans: shrinks");
        assert_eq!(e.map_range(r("B1:C1")), None, "all deleted");
        assert_eq!(e.map_range(r("C1:E1")), Some(r("B1:C1")), "start deleted");
        assert_eq!(e.map_range(r("A1:B1")), Some(r("A1:A1")), "end deleted");
        assert_eq!(e.map_range(r("D1:D1")), Some(r("B1:B1")));
    }
}
