//! Arrays the operators compute item by item over ranges and array
//! constants, as in `SUMPRODUCT((A1:A9="x")*B1:B9)` or `SUM({1,2,3}+1)`.

use crate::calc::engine::CellResult;
use crate::cell::CellError;
use crate::formula::Expr;

/// Most items a computed array may hold, a whole column's worth. A larger
/// one is #VALUE! rather than an allocation the size of the sheet.
pub(crate) const MAX_ARRAY_ITEMS: u64 = 1 << 20;

/// Values in rows, all the same length.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Array {
    rows: u32,
    cols: u32,
    /// Row by row
    items: Vec<CellResult>,
}

impl Array {
    /// `items` row by row, `rows * cols` of them.
    pub(crate) fn new(rows: u32, cols: u32, items: Vec<CellResult>) -> Self {
        debug_assert_eq!(items.len() as u64, rows as u64 * cols as u64);
        Self { rows, cols, items }
    }

    /// An array constant's items, which the parser keeps to literals.
    pub(crate) fn from_literals(rows: &[Vec<Expr>]) -> Self {
        let cols = rows.first().map_or(0, Vec::len) as u32;
        let items = rows.iter().flatten().map(literal).collect();
        Self::new(rows.len() as u32, cols, items)
    }

    pub(crate) fn rows(&self) -> u32 {
        self.rows
    }

    pub(crate) fn cols(&self) -> u32 {
        self.cols
    }

    /// The item `row` rows down and `col` columns across.
    pub(crate) fn get(&self, row: u32, col: u32) -> &CellResult {
        &self.items[row as usize * self.cols as usize + col as usize]
    }

    fn map(mut self, f: impl Fn(CellResult) -> CellResult) -> Self {
        for item in &mut self.items {
            *item = f(std::mem::replace(item, CellResult::Empty));
        }
        self
    }

    /// `f` over two arrays item by item. Along each axis an extent of 1
    /// repeats to match the other side, so a row against a column gives
    /// every pairing; otherwise the longer extent wins and items past the
    /// shorter side's end are #N/A, as in Excel. #VALUE! past
    /// [`MAX_ARRAY_ITEMS`].
    fn zip(
        &self,
        other: &Array,
        f: impl Fn(CellResult, CellResult) -> CellResult,
    ) -> Result<Array, CellError> {
        let extent = |a: u32, b: u32| match (a, b) {
            (1, n) | (n, 1) => n,
            (a, b) => a.max(b),
        };
        let (rows, cols) = (extent(self.rows, other.rows), extent(self.cols, other.cols));
        if rows as u64 * cols as u64 > MAX_ARRAY_ITEMS {
            return Err(CellError::Value);
        }
        let at = |a: &Array, row: u32, col: u32| {
            let row = if a.rows == 1 { 0 } else { row };
            let col = if a.cols == 1 { 0 } else { col };
            if row < a.rows && col < a.cols {
                a.get(row, col).clone()
            } else {
                CellResult::Error(CellError::NA)
            }
        };
        let mut items = Vec::with_capacity(rows as usize * cols as usize);
        for row in 0..rows {
            for col in 0..cols {
                items.push(f(at(self, row, col), at(other, row, col)));
            }
        }
        Ok(Array::new(rows, cols, items))
    }
}

/// What an expression gives where operators work item by item: one value,
/// or an array from a range, an array constant, or an operator over them.
pub(crate) enum Operand {
    Single(CellResult),
    Array(Array),
}

impl Operand {
    /// `f` on each item.
    pub(crate) fn map(self, f: impl Fn(CellResult) -> CellResult) -> Operand {
        match self {
            Operand::Single(v) => Operand::Single(f(v)),
            Operand::Array(a) => Operand::Array(a.map(f)),
        }
    }

    /// `f` over both sides: item by item when either is an array, a single
    /// value going with every item of the other.
    pub(crate) fn combine(
        self,
        other: Operand,
        f: impl Fn(CellResult, CellResult) -> CellResult,
    ) -> Operand {
        match (self, other) {
            (Operand::Single(a), Operand::Single(b)) => Operand::Single(f(a, b)),
            (Operand::Single(a), Operand::Array(b)) => {
                Operand::Array(b.map(|item| f(a.clone(), item)))
            }
            (Operand::Array(a), Operand::Single(b)) => {
                Operand::Array(a.map(|item| f(item, b.clone())))
            }
            (Operand::Array(a), Operand::Array(b)) => match a.zip(&b, f) {
                Ok(array) => Operand::Array(array),
                Err(e) => Operand::Single(CellResult::Error(e)),
            },
        }
    }

    /// As one value, as a cell shows it: an array's only item, while a
    /// larger array has no single value and is #VALUE!.
    pub(crate) fn into_single(self) -> CellResult {
        match self {
            Operand::Single(v) => v,
            Operand::Array(a) => match <[CellResult; 1]>::try_from(a.items) {
                Ok([v]) => v,
                Err(_) => CellResult::Error(CellError::Value),
            },
        }
    }
}

/// Whether `expr` is an operator applied to an array: to a range, an array
/// constant, a reference INDIRECT or OFFSET returns, or another such
/// operator. Other function calls are single values and aren't looked into.
pub(crate) fn computes_array(expr: &Expr) -> bool {
    match expr {
        Expr::Binary { left, right, .. } => holds_array(left) || holds_array(right),
        Expr::Unary { operand, .. } => holds_array(operand),
        Expr::Chain { first, rest } => Expr::chain_operands(first, rest).any(holds_array),
        _ => false,
    }
}

fn holds_array(expr: &Expr) -> bool {
    match expr {
        Expr::RangeRef(_) | Expr::Array(_) => true,
        Expr::Function(f) => f.name == "INDIRECT" || f.name == "OFFSET",
        other => computes_array(other),
    }
}

/// The value of an array constant's item.
pub(crate) fn literal(item: &Expr) -> CellResult {
    match item {
        Expr::Number(n) => CellResult::Value(*n),
        Expr::Text(s) => CellResult::Text(s.clone()),
        Expr::Bool(b) => CellResult::Bool(*b),
        Expr::Error(e) => CellResult::Error(*e),
        _ => CellResult::Error(CellError::Value),
    }
}
