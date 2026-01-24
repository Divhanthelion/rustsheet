use crate::cell::{CellCoord, CellRange, CellValue};
use hashbrown::HashMap;
use roaring::RoaringBitmap;

/// Sparse grid storage using HashMap with roaring bitmap indices.
/// Optimized for spreadsheet access patterns:
/// - O(1) random cell access
/// - O(1) check if row/column is populated
/// - Efficient iteration over non-empty cells
pub struct SparseGrid {
    /// Cell storage: (row, col) -> value
    cells: HashMap<(u32, u32), CellValue>,
    /// Bitmap of rows that contain at least one cell
    populated_rows: RoaringBitmap,
    /// Bitmap of columns that contain at least one cell
    populated_cols: RoaringBitmap,
    /// Cached bounds (lazily updated)
    bounds_dirty: bool,
    cached_bounds: Option<CellRange>,
}

impl SparseGrid {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
            populated_rows: RoaringBitmap::new(),
            populated_cols: RoaringBitmap::new(),
            bounds_dirty: true,
            cached_bounds: None,
        }
    }

    /// Get cell value at coordinate
    pub fn get(&self, coord: CellCoord) -> Option<&CellValue> {
        self.cells.get(&(coord.row, coord.col))
    }

    /// Get mutable reference to cell value
    pub fn get_mut(&mut self, coord: CellCoord) -> Option<&mut CellValue> {
        self.cells.get_mut(&(coord.row, coord.col))
    }

    /// Set cell value, returns old value if any
    pub fn set(&mut self, coord: CellCoord, value: CellValue) -> Option<CellValue> {
        self.bounds_dirty = true;

        if value.is_empty() {
            return self.remove(coord);
        }

        self.populated_rows.insert(coord.row);
        self.populated_cols.insert(coord.col);
        self.cells.insert((coord.row, coord.col), value)
    }

    /// Remove cell, returns old value if any
    pub fn remove(&mut self, coord: CellCoord) -> Option<CellValue> {
        self.bounds_dirty = true;
        let old = self.cells.remove(&(coord.row, coord.col));

        // Update bitmaps if row/col is now empty
        // This is expensive, so we do it lazily only when needed
        if old.is_some() {
            self.update_row_bitmap(coord.row);
            self.update_col_bitmap(coord.col);
        }

        old
    }

    /// Clear all cells
    pub fn clear(&mut self) {
        self.cells.clear();
        self.populated_rows.clear();
        self.populated_cols.clear();
        self.bounds_dirty = true;
        self.cached_bounds = None;
    }

    /// Number of non-empty cells
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Check if a row contains any cells
    pub fn row_is_populated(&self, row: u32) -> bool {
        self.populated_rows.contains(row)
    }

    /// Check if a column contains any cells
    pub fn col_is_populated(&self, col: u32) -> bool {
        self.populated_cols.contains(col)
    }

    /// Get the bounding range of all non-empty cells
    pub fn bounds(&mut self) -> Option<CellRange> {
        if !self.bounds_dirty {
            return self.cached_bounds;
        }

        if self.cells.is_empty() {
            self.cached_bounds = None;
            self.bounds_dirty = false;
            return None;
        }

        let min_row = self.populated_rows.min().unwrap_or(0);
        let max_row = self.populated_rows.max().unwrap_or(0);
        let min_col = self.populated_cols.min().unwrap_or(0);
        let max_col = self.populated_cols.max().unwrap_or(0);

        let bounds = CellRange::new(
            CellCoord::new(min_row, max_row),
            CellCoord::new(min_col, max_col),
        );

        self.cached_bounds = Some(bounds);
        self.bounds_dirty = false;
        Some(bounds)
    }

    /// Iterate over all non-empty cells
    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, &CellValue)> {
        self.cells
            .iter()
            .map(|(&(row, col), value)| (CellCoord::new(row, col), value))
    }

    /// Iterate over cells in a specific range (only non-empty)
    pub fn iter_range(&self, range: CellRange) -> impl Iterator<Item = (CellCoord, &CellValue)> {
        // Filter cells that fall within the range
        self.cells
            .iter()
            .filter(move |&(&(row, col), _)| {
                row >= range.start.row
                    && row <= range.end.row
                    && col >= range.start.col
                    && col <= range.end.col
            })
            .map(|(&(row, col), value)| (CellCoord::new(row, col), value))
    }

    /// Iterate over cells in a specific row
    pub fn iter_row(&self, row: u32) -> impl Iterator<Item = (u32, &CellValue)> {
        self.populated_cols.iter().filter_map(move |col| {
            self.cells.get(&(row, col)).map(|value| (col, value))
        })
    }

    /// Iterate over cells in a specific column
    pub fn iter_col(&self, col: u32) -> impl Iterator<Item = (u32, &CellValue)> {
        self.populated_rows.iter().filter_map(move |row| {
            self.cells.get(&(row, col)).map(|value| (row, value))
        })
    }

    /// Count of populated rows
    pub fn row_count(&self) -> u64 {
        self.populated_rows.len()
    }

    /// Count of populated columns
    pub fn col_count(&self) -> u64 {
        self.populated_cols.len()
    }

    // Helper: update row bitmap after removal
    fn update_row_bitmap(&mut self, row: u32) {
        let row_empty = !self
            .populated_cols
            .iter()
            .any(|col| self.cells.contains_key(&(row, col)));

        if row_empty {
            self.populated_rows.remove(row);
        }
    }

    // Helper: update col bitmap after removal
    fn update_col_bitmap(&mut self, col: u32) {
        let col_empty = !self
            .populated_rows
            .iter()
            .any(|row| self.cells.contains_key(&(row, col)));

        if col_empty {
            self.populated_cols.remove(col);
        }
    }
}

impl Default for SparseGrid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut grid = SparseGrid::new();

        let coord = CellCoord::new(0, 0);
        assert!(grid.get(coord).is_none());

        grid.set(coord, CellValue::Number(42.0));
        assert_eq!(grid.get(coord), Some(&CellValue::Number(42.0)));
        assert_eq!(grid.len(), 1);

        grid.remove(coord);
        assert!(grid.get(coord).is_none());
        assert_eq!(grid.len(), 0);
    }

    #[test]
    fn test_sparse_iteration() {
        let mut grid = SparseGrid::new();

        // Set cells at widely spaced coordinates
        grid.set(CellCoord::new(0, 0), CellValue::Number(1.0));
        grid.set(CellCoord::new(1000, 1000), CellValue::Number(2.0));
        grid.set(CellCoord::new(1_000_000, 0), CellValue::Number(3.0));

        assert_eq!(grid.len(), 3);
        assert!(grid.row_is_populated(0));
        assert!(grid.row_is_populated(1000));
        assert!(grid.row_is_populated(1_000_000));
        assert!(!grid.row_is_populated(500));
    }

    #[test]
    fn test_range_iteration() {
        let mut grid = SparseGrid::new();

        grid.set(CellCoord::new(0, 0), CellValue::Number(1.0));
        grid.set(CellCoord::new(1, 1), CellValue::Number(2.0));
        grid.set(CellCoord::new(100, 100), CellValue::Number(3.0)); // Outside range

        let range = CellRange::from_a1("A1:B2").unwrap();
        let cells: Vec<_> = grid.iter_range(range).collect();
        assert_eq!(cells.len(), 2);
    }
}
