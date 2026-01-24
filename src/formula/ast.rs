use crate::cell::{CellCoord, CellRange, CellError};
use serde::{Deserialize, Serialize};

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
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
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
            Expr::Binary { left, right, .. } => {
                left.has_dependencies() || right.has_dependencies()
            }
            Expr::Unary { operand, .. } => operand.has_dependencies(),
            Expr::Function(f) => f.args.iter().any(|a| a.has_dependencies()),
            _ => false,
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
}
