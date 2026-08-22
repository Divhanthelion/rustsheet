mod ast;
mod grammar;
mod parser;

pub use ast::{BinaryOp, CellRef, Expr, FunctionCall, RangeRef, UnaryOp};
pub use parser::FormulaParser;

/// Ensure a formula string starts with `=` so it re-parses.
pub fn normalize_formula(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with('=') {
        trimmed.to_string()
    } else {
        format!("={trimmed}")
    }
}
