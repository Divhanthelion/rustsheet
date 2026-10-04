use crate::cell::{CellCoord, CellError, CellRange};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    // String
    Concat,
    // Comparison
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl BinaryOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Pow => "^",
            BinaryOp::Concat => "&",
            BinaryOp::Eq => "=",
            BinaryOp::Neq => "<>",
            BinaryOp::Lt => "<",
            BinaryOp::Lte => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Gte => ">=",
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Pos,
}

/// Function call representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<Expr>,
}

/// Cell reference with optional sheet qualifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellRef {
    pub sheet: Option<String>,
    pub coord: CellCoord,
    pub row_absolute: bool,
    pub col_absolute: bool,
}

impl CellRef {
    pub fn new(coord: CellCoord) -> Self {
        Self {
            sheet: None,
            coord,
            row_absolute: false,
            col_absolute: false,
        }
    }

    pub fn with_sheet(mut self, sheet: impl Into<String>) -> Self {
        self.sheet = Some(sheet.into());
        self
    }

    pub fn absolute(mut self) -> Self {
        self.row_absolute = true;
        self.col_absolute = true;
        self
    }
}

/// Range reference
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeRef {
    pub sheet: Option<String>,
    pub range: CellRange,
    /// `$` on the start corner's (row, column)
    #[serde(default)]
    pub start_absolute: (bool, bool),
    /// `$` on the end corner's (row, column)
    #[serde(default)]
    pub end_absolute: (bool, bool),
}

/// Excel's last row and column index.
const MAX_ROW: i64 = 1_048_575;
const MAX_COL: i64 = 16_383;

/// Move a coordinate by (`rows`, `cols`) except on absolute axes.
/// `None` if it would leave the sheet.
fn offset_coord(
    coord: CellCoord,
    absolute: (bool, bool),
    rows: i64,
    cols: i64,
) -> Option<CellCoord> {
    let row = coord.row as i64 + if absolute.0 { 0 } else { rows };
    let col = coord.col as i64 + if absolute.1 { 0 } else { cols };
    ((0..=MAX_ROW).contains(&row) && (0..=MAX_COL).contains(&col))
        .then(|| CellCoord::new(row as u32, col as u32))
}

/// Abstract Syntax Tree for formulas
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    /// Numeric literal
    Number(f64),
    /// String literal
    Text(String),
    /// Boolean literal
    Bool(bool),
    /// Error literal
    Error(CellError),
    /// Single cell reference
    CellRef(CellRef),
    /// Range reference (A1:B2)
    RangeRef(RangeRef),
    /// Binary operation
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary operation
    Unary { op: UnaryOp, operand: Box<Expr> },
    /// Function call
    Function(FunctionCall),
}

impl Expr {
    /// Create a binary expression
    pub fn binary(op: BinaryOp, left: Expr, right: Expr) -> Self {
        Expr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a unary expression
    pub fn unary(op: UnaryOp, operand: Expr) -> Self {
        Expr::Unary {
            op,
            operand: Box::new(operand),
        }
    }

    /// Create a function call
    pub fn function(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Function(FunctionCall {
            name: name.into(),
            args,
        })
    }

    /// Check if this expression references other cells
    pub fn has_dependencies(&self) -> bool {
        match self {
            Expr::CellRef(_) | Expr::RangeRef(_) => true,
            Expr::Binary { left, right, .. } => left.has_dependencies() || right.has_dependencies(),
            Expr::Unary { operand, .. } => operand.has_dependencies(),
            Expr::Function(f) => f.args.iter().any(|a| a.has_dependencies()),
            _ => false,
        }
    }

    /// Move relative references by (`rows`, `cols`), as when a formula is
    /// copied to another cell. `$` parts stay put; a reference pushed off the
    /// sheet becomes `#REF!`, as in Excel.
    pub fn offset_references(&mut self, rows: i64, cols: i64) {
        match self {
            Expr::CellRef(r) => {
                match offset_coord(r.coord, (r.row_absolute, r.col_absolute), rows, cols) {
                    Some(coord) => r.coord = coord,
                    None => *self = Expr::Error(CellError::Ref),
                }
            }
            Expr::RangeRef(r) => {
                let start = offset_coord(r.range.start, r.start_absolute, rows, cols);
                let end = offset_coord(r.range.end, r.end_absolute, rows, cols);
                match (start, end) {
                    (Some(start), Some(end)) => r.range = CellRange::new(start, end),
                    _ => *self = Expr::Error(CellError::Ref),
                }
            }
            Expr::Binary { left, right, .. } => {
                left.offset_references(rows, cols);
                right.offset_references(rows, cols);
            }
            Expr::Unary { operand, .. } => operand.offset_references(rows, cols),
            Expr::Function(f) => {
                for arg in &mut f.args {
                    arg.offset_references(rows, cols);
                }
            }
            Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) => {}
        }
    }

    /// Rewrite a sheet qualifier in place (used when a tab is renamed).
    pub fn rename_sheet(&mut self, old: &str, new: &str) {
        match self {
            Expr::CellRef(r) => {
                if r.sheet
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(old))
                {
                    r.sheet = Some(new.to_string());
                }
            }
            Expr::RangeRef(r) => {
                if r.sheet
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(old))
                {
                    r.sheet = Some(new.to_string());
                }
            }
            Expr::Binary { left, right, .. } => {
                left.rename_sheet(old, new);
                right.rename_sheet(old, new);
            }
            Expr::Unary { operand, .. } => operand.rename_sheet(old, new),
            Expr::Function(f) => {
                for arg in &mut f.args {
                    arg.rename_sheet(old, new);
                }
            }
            _ => {}
        }
    }

    /// Collect all cell references in this expression
    pub fn collect_dependencies(&self, deps: &mut Vec<CellRef>) {
        match self {
            Expr::CellRef(r) => deps.push(r.clone()),
            Expr::RangeRef(r) => {
                // Expand range to individual cells
                for coord in r.range.iter() {
                    deps.push(CellRef {
                        sheet: r.sheet.clone(),
                        coord,
                        row_absolute: false,
                        col_absolute: false,
                    });
                }
            }
            Expr::Binary { left, right, .. } => {
                left.collect_dependencies(deps);
                right.collect_dependencies(deps);
            }
            Expr::Unary { operand, .. } => operand.collect_dependencies(deps),
            Expr::Function(f) => {
                for arg in &f.args {
                    arg.collect_dependencies(deps);
                }
            }
            _ => {}
        }
    }
}

fn sheet_prefix(name: Option<&str>) -> String {
    match name {
        None => String::new(),
        Some(n) if n.chars().any(|c| !c.is_ascii_alphanumeric() && c != '_') => {
            format!("'{}'!", n.replace('\'', "''"))
        }
        Some(n) => format!("{n}!"),
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(n) => write!(f, "{n}"),
            Expr::Text(s) => write!(f, "\"{}\"", s.replace('"', "\"\"")),
            Expr::Bool(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Expr::Error(e) => write!(f, "{}", e.as_str()),
            Expr::CellRef(r) => write!(
                f,
                "{}{}",
                sheet_prefix(r.sheet.as_deref()),
                r.coord.to_a1_abs(r.row_absolute, r.col_absolute)
            ),
            Expr::RangeRef(r) => write!(
                f,
                "{}{}:{}",
                sheet_prefix(r.sheet.as_deref()),
                r.range
                    .start
                    .to_a1_abs(r.start_absolute.0, r.start_absolute.1),
                r.range.end.to_a1_abs(r.end_absolute.0, r.end_absolute.1)
            ),
            Expr::Binary { op, left, right } => {
                write!(f, "({}{}{})", left, op.as_str(), right)
            }
            Expr::Unary { op, operand } => match op {
                UnaryOp::Neg => write!(f, "-{operand}"),
                UnaryOp::Pos => write!(f, "+{operand}"),
            },
            Expr::Function(func) => {
                write!(f, "{}(", func.name)?;
                for (i, arg) in func.args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_collection() {
        // =A1 + B2
        let expr = Expr::binary(
            BinaryOp::Add,
            Expr::CellRef(CellRef::new(CellCoord::new(0, 0))),
            Expr::CellRef(CellRef::new(CellCoord::new(1, 1))),
        );

        let mut deps = Vec::new();
        expr.collect_dependencies(&mut deps);
        assert_eq!(deps.len(), 2);
    }

    fn moved(formula: &str, rows: i64, cols: i64) -> String {
        let mut expr = crate::formula::FormulaParser::new().parse(formula).unwrap();
        expr.offset_references(rows, cols);
        format!("={expr}")
    }

    #[test]
    fn absolute_ranges_keep_their_dollars() {
        let expr = crate::formula::FormulaParser::new()
            .parse("=SUM($B$2:$B$7)+SUM(B$1:$C2)")
            .unwrap();
        assert_eq!(format!("={expr}"), "=(SUM($B$2:$B$7)+SUM(B$1:$C2))");
    }

    #[test]
    fn copied_formulas_move_relative_parts_only() {
        assert_eq!(moved("=A1+B2", 1, 2), "=(C2+D3)");
        assert_eq!(moved("=$A$1+A$1+$A1", 3, 3), "=(($A$1+D$1)+$A4)");
        assert_eq!(moved("=SUM(B2:B7)", 0, 1), "=SUM(C2:C7)");
        assert_eq!(moved("=SUM($B$2:$B$7)", 5, 5), "=SUM($B$2:$B$7)");
        assert_eq!(moved("=Sheet2!A1", 1, 0), "=Sheet2!A2");
        // Off the top of the sheet: #REF!, as in Excel.
        assert_eq!(moved("=A1*2", -1, 0), "=(#REF!*2)");
        assert_eq!(moved("=SUM(A1:A3)", 0, -1), "=SUM(#REF!)");
    }
}
