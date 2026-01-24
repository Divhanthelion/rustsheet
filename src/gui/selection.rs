//! Cell selection handling

use crate::cell::{CellCoord, CellRange};

/// Represents the current selection state in the spreadsheet
#[derive(Debug, Clone)]
pub struct Selection {
    /// The anchor cell (where selection started)
    pub anchor: CellCoord,
    /// The active cell (current cursor position)
    pub active: CellCoord,
    /// Multiple selection ranges (for Ctrl+Click)
    pub ranges: Vec<SelectionRange>,
}

/// A single contiguous selection range
#[derive(Debug, Clone, Copy)]
pub struct SelectionRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

impl Selection {
    pub fn new(coord: CellCoord) -> Self {
        Self {
            anchor: coord,
            active: coord,
            ranges: vec![SelectionRange {
                start: coord,
                end: coord,
            }],
        }
    }

    /// Get the primary selection range (normalized so start <= end)
    pub fn primary_range(&self) -> CellRange {
        let start_row = self.anchor.row.min(self.active.row);
        let end_row = self.anchor.row.max(self.active.row);
        let start_col = self.anchor.col.min(self.active.col);
        let end_col = self.anchor.col.max(self.active.col);

        CellRange::new(
            CellCoord::new(start_row, start_col),
            CellCoord::new(end_row, end_col),
        )
    }

    /// Move selection to a new cell (no extending)
    pub fn move_to(&mut self, coord: CellCoord) {
        self.anchor = coord;
        self.active = coord;
        self.ranges = vec![SelectionRange {
            start: coord,
            end: coord,
        }];
    }

    /// Extend selection from anchor to new active cell
    pub fn extend_to(&mut self, coord: CellCoord) {
        self.active = coord;
        let range = self.primary_range();
        self.ranges = vec![SelectionRange {
            start: range.start,
            end: range.end,
        }];
    }

    /// Check if a cell is within the selection
    pub fn contains(&self, coord: CellCoord) -> bool {
        for range in &self.ranges {
            let start_row = range.start.row.min(range.end.row);
            let end_row = range.start.row.max(range.end.row);
            let start_col = range.start.col.min(range.end.col);
            let end_col = range.start.col.max(range.end.col);

            if coord.row >= start_row
                && coord.row <= end_row
                && coord.col >= start_col
                && coord.col <= end_col
            {
                return true;
            }
        }
        false
    }

    /// Move active cell by delta, optionally extending selection
    pub fn move_by(&mut self, row_delta: i32, col_delta: i32, extend: bool, max_row: u32, max_col: u32) {
        let new_row = (self.active.row as i32 + row_delta).clamp(0, max_row as i32) as u32;
        let new_col = (self.active.col as i32 + col_delta).clamp(0, max_col as i32) as u32;
        let new_coord = CellCoord::new(new_row, new_col);

        if extend {
            self.extend_to(new_coord);
        } else {
            self.move_to(new_coord);
        }
    }

    /// Select entire row
    pub fn select_row(&mut self, row: u32, max_col: u32) {
        self.anchor = CellCoord::new(row, 0);
        self.active = CellCoord::new(row, max_col);
        self.ranges = vec![SelectionRange {
            start: self.anchor,
            end: self.active,
        }];
    }

    /// Select entire column
    pub fn select_column(&mut self, col: u32, max_row: u32) {
        self.anchor = CellCoord::new(0, col);
        self.active = CellCoord::new(max_row, col);
        self.ranges = vec![SelectionRange {
            start: self.anchor,
            end: self.active,
        }];
    }

    /// Select all cells
    pub fn select_all(&mut self, max_row: u32, max_col: u32) {
        self.anchor = CellCoord::new(0, 0);
        self.active = CellCoord::new(max_row, max_col);
        self.ranges = vec![SelectionRange {
            start: self.anchor,
            end: self.active,
        }];
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new(CellCoord::new(0, 0))
    }
}

impl SelectionRange {
    pub fn single(coord: CellCoord) -> Self {
        Self {
            start: coord,
            end: coord,
        }
    }

    pub fn width(&self) -> u32 {
        self.end.col.abs_diff(self.start.col) + 1
    }

    pub fn height(&self) -> u32 {
        self.end.row.abs_diff(self.start.row) + 1
    }
}
