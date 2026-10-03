use lasso::Spur;
use serde::{Deserialize, Serialize};

/// Represents all possible cell error types (Excel-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellError {
    /// #NULL! - Incorrect range operator
    Null,
    /// #DIV/0! - Division by zero
    DivZero,
    /// #VALUE! - Wrong type of argument
    Value,
    /// #REF! - Invalid cell reference
    Ref,
    /// #NAME? - Unrecognized formula name
    Name,
    /// #NUM! - Invalid numeric value
    Num,
    /// #N/A - Value not available
    NA,
    /// #GETTING_DATA - Async data loading
    GettingData,
    /// #SPILL! - Spill range blocked
    Spill,
    /// #CALC! - Calculation error
    Calc,
    /// #CIRC! - Circular reference detected
    Circular,
}

impl CellError {
    pub fn as_str(&self) -> &'static str {
        match self {
            CellError::Null => "#NULL!",
            CellError::DivZero => "#DIV/0!",
            CellError::Value => "#VALUE!",
            CellError::Ref => "#REF!",
            CellError::Name => "#NAME?",
            CellError::Num => "#NUM!",
            CellError::NA => "#N/A",
            CellError::GettingData => "#GETTING_DATA",
            CellError::Spill => "#SPILL!",
            CellError::Calc => "#CALC!",
            CellError::Circular => "#CIRC!",
        }
    }
}

/// Core cell value enum - optimized for memory efficiency.
/// Strings are interned via `Spur` (4-byte token) instead of heap String.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CellValue {
    /// Empty cell (distinct from blank string)
    #[default]
    Empty,
    /// Boolean value
    Bool(bool),
    /// Numeric value (f64 covers all Excel number types)
    Number(f64),
    /// Interned string token - resolve via StringPool
    Text(Spur),
    /// Error value
    Error(CellError),
    /// Formula (stores AST index, not raw string)
    Formula {
        /// Index into formula AST storage
        ast_id: u32,
        /// Cached computed value
        cached: Box<CellValue>,
    },
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, CellValue::Error(_))
    }

    pub fn is_formula(&self) -> bool {
        matches!(self, CellValue::Formula { .. })
    }

    /// Coerce to number for calculations (Excel semantics)
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            CellValue::Bool(true) => Some(1.0),
            CellValue::Bool(false) => Some(0.0),
            CellValue::Empty => Some(0.0),
            _ => None,
        }
    }

    /// Coerce to bool for logical operations
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            CellValue::Bool(b) => Some(*b),
            CellValue::Number(n) => Some(*n != 0.0),
            CellValue::Empty => Some(false),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_value_size() {
        // Ensure CellValue stays compact
        assert!(std::mem::size_of::<CellValue>() <= 24);
    }

    #[test]
    fn test_number_coercion() {
        assert_eq!(CellValue::Number(42.0).as_number(), Some(42.0));
        assert_eq!(CellValue::Bool(true).as_number(), Some(1.0));
        assert_eq!(CellValue::Empty.as_number(), Some(0.0));
    }
}
