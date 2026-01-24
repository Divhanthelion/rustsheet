use crate::cell::{CellCoord, CellRange, CellValue, StringPool};
use crate::grid::SparseGrid;

/// A single worksheet containing cells, formatting, and metadata
pub struct Sheet {
    /// Sheet name
    name: String,
    /// Cell data storage
    grid: SparseGrid,
    /// String interning pool (shared across workbook, but ref held here)
    string_pool: StringPool,
    /// Column widths (None = default width)
    col_widths: Vec<Option<f64>>,
    /// Row heights (None = default height)
    row_heights: Vec<Option<f64>>,
    /// Default column width in points
    default_col_width: f64,
    /// Default row height in points
    default_row_height: f64,
    /// Frozen rows (header rows)
    frozen_rows: u32,
    /// Frozen columns
    frozen_cols: u32,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            grid: SparseGrid::new(),
            string_pool: StringPool::new(),
            col_widths: Vec::new(),
            row_heights: Vec::new(),
            default_col_width: 64.0,  // ~8 characters
            default_row_height: 20.0, // Standard row height
            frozen_rows: 0,
            frozen_cols: 0,
        }
    }

    pub fn with_string_pool(name: impl Into<String>, pool: StringPool) -> Self {
        Self {
            name: name.into(),
            grid: SparseGrid::new(),
            string_pool: pool,
            col_widths: Vec::new(),
            row_heights: Vec::new(),
            default_col_width: 64.0,
            default_row_height: 20.0,
            frozen_rows: 0,
            frozen_cols: 0,
        }
    }

    // ========== Cell Operations ==========

    pub fn get(&self, coord: CellCoord) -> Option<&CellValue> {
        self.grid.get(coord)
    }

    pub fn get_or_empty(&self, coord: CellCoord) -> &CellValue {
        static EMPTY: CellValue = CellValue::Empty;
        self.grid.get(coord).unwrap_or(&EMPTY)
    }

    pub fn set(&mut self, coord: CellCoord, value: CellValue) -> Option<CellValue> {
        self.grid.set(coord, value)
    }

    /// Set a number value
    pub fn set_number(&mut self, coord: CellCoord, n: f64) {
        self.grid.set(coord, CellValue::Number(n));
    }

    /// Set a text value (automatically interned)
    pub fn set_text(&mut self, coord: CellCoord, s: &str) {
        if let Some(key) = self.string_pool.intern(s) {
            self.grid.set(coord, CellValue::Text(key));
        }
    }

    /// Set a boolean value
    pub fn set_bool(&mut self, coord: CellCoord, b: bool) {
        self.grid.set(coord, CellValue::Bool(b));
    }

    pub fn remove(&mut self, coord: CellCoord) -> Option<CellValue> {
        self.grid.remove(coord)
    }

    pub fn clear(&mut self) {
        self.grid.clear();
    }

    /// Resolve text cell to string
    pub fn resolve_text(&self, value: &CellValue) -> Option<&str> {
        match value {
            CellValue::Text(key) => self.string_pool.resolve(*key),
            _ => None,
        }
    }

    // ========== Iteration ==========

    pub fn iter(&self) -> impl Iterator<Item = (CellCoord, &CellValue)> {
        self.grid.iter()
    }

    pub fn iter_range(&self, range: CellRange) -> impl Iterator<Item = (CellCoord, &CellValue)> {
        self.grid.iter_range(range)
    }

    // ========== Metadata ==========

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.grid.is_empty()
    }

    pub fn bounds(&mut self) -> Option<CellRange> {
        self.grid.bounds()
    }

    pub fn string_pool(&self) -> &StringPool {
        &self.string_pool
    }

    pub fn string_pool_mut(&mut self) -> &mut StringPool {
        &mut self.string_pool
    }

    // ========== Dimensions ==========

    pub fn col_width(&self, col: u32) -> f64 {
        self.col_widths
            .get(col as usize)
            .and_then(|w| *w)
            .unwrap_or(self.default_col_width)
    }

    pub fn set_col_width(&mut self, col: u32, width: f64) {
        let idx = col as usize;
        if idx >= self.col_widths.len() {
            self.col_widths.resize(idx + 1, None);
        }
        self.col_widths[idx] = Some(width);
    }

    pub fn row_height(&self, row: u32) -> f64 {
        self.row_heights
            .get(row as usize)
            .and_then(|h| *h)
            .unwrap_or(self.default_row_height)
    }

    pub fn set_row_height(&mut self, row: u32, height: f64) {
        let idx = row as usize;
        if idx >= self.row_heights.len() {
            self.row_heights.resize(idx + 1, None);
        }
        self.row_heights[idx] = Some(height);
    }

    pub fn default_col_width(&self) -> f64 {
        self.default_col_width
    }

    pub fn set_default_col_width(&mut self, width: f64) {
        self.default_col_width = width;
    }

    pub fn default_row_height(&self) -> f64 {
        self.default_row_height
    }

    pub fn set_default_row_height(&mut self, height: f64) {
        self.default_row_height = height;
    }

    // ========== Freeze Panes ==========

    pub fn frozen_rows(&self) -> u32 {
        self.frozen_rows
    }

    pub fn frozen_cols(&self) -> u32 {
        self.frozen_cols
    }

    pub fn set_frozen(&mut self, rows: u32, cols: u32) {
        self.frozen_rows = rows;
        self.frozen_cols = cols;
    }
}

impl Default for Sheet {
    fn default() -> Self {
        Self::new("Sheet1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_basic() {
        let mut sheet = Sheet::new("Test");
        let coord = CellCoord::from_a1("A1").unwrap();

        sheet.set_number(coord, 42.0);
        assert_eq!(sheet.get(coord), Some(&CellValue::Number(42.0)));
    }

    #[test]
    fn test_text_interning() {
        let mut sheet = Sheet::new("Test");
        let a1 = CellCoord::from_a1("A1").unwrap();
        let b1 = CellCoord::from_a1("B1").unwrap();

        sheet.set_text(a1, "hello");
        sheet.set_text(b1, "hello");

        // Both should point to same interned string
        match (sheet.get(a1), sheet.get(b1)) {
            (Some(CellValue::Text(k1)), Some(CellValue::Text(k2))) => {
                assert_eq!(k1, k2);
            }
            _ => panic!("Expected text values"),
        }
    }
}
