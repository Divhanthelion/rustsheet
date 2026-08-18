mod ast;
mod grammar;
mod parser;

pub use ast::{Expr, BinaryOp, UnaryOp, FunctionCall, CellRef, RangeRef};
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
