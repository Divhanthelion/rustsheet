use crate::cell::{CellCoord, CellRange, CellError};
use crate::formula::ast::{BinaryOp, CellRef, Expr, RangeRef, UnaryOp};
use crate::formula::grammar::{FormulaGrammar, Rule};
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::Parser;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    Pest(#[from] pest::error::Error<Rule>),
    #[error("Invalid cell reference: {0}")]
    InvalidCellRef(String),
    #[error("Invalid number: {0}")]
    InvalidNumber(String),
    #[error("Unexpected rule: {0:?}")]
    UnexpectedRule(Rule),
}

/// Formula parser using pest + Pratt parser for operator precedence
pub struct FormulaParser {
    pratt: PrattParser<Rule>,
}

impl FormulaParser {
    pub fn new() -> Self {
        // Define operator precedence (lowest to highest)
        let pratt = PrattParser::new()
            // Comparison operators (lowest precedence)
            .op(Op::infix(Rule::eq, Assoc::Left)
                | Op::infix(Rule::neq, Assoc::Left)
                | Op::infix(Rule::lt, Assoc::Left)
                | Op::infix(Rule::lte, Assoc::Left)
                | Op::infix(Rule::gt, Assoc::Left)
                | Op::infix(Rule::gte, Assoc::Left))
            // String concatenation
            .op(Op::infix(Rule::concat, Assoc::Left))
            // Addition and subtraction
            .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
            // Multiplication and division
            .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left))
            // Exponentiation (right associative)
            .op(Op::infix(Rule::pow, Assoc::Right))
            // Unary operators (highest precedence)
            .op(Op::prefix(Rule::neg) | Op::prefix(Rule::pos));

        Self { pratt }
    }

    /// Parse a formula string (must start with '=')
    pub fn parse(&self, input: &str) -> Result<Expr, ParseError> {
        let pairs = FormulaGrammar::parse(Rule::formula, input)?;
        self.parse_formula(pairs)
    }

    /// Parse just an expression (without leading '=')
    pub fn parse_expr(&self, input: &str) -> Result<Expr, ParseError> {
        let pairs = FormulaGrammar::parse(Rule::expr, input)?;
        let expr_pair = pairs.into_iter().next().unwrap();
        self.parse_expression(expr_pair.into_inner())
    }

    fn parse_formula(&self, mut pairs: Pairs<Rule>) -> Result<Expr, ParseError> {
        let formula_pair = pairs.next().unwrap();
        assert_eq!(formula_pair.as_rule(), Rule::formula);

        let inner = formula_pair.into_inner().next().unwrap();
        self.parse_expression(inner.into_inner())
    }

    fn parse_expression(&self, pairs: Pairs<Rule>) -> Result<Expr, ParseError> {
        self.pratt
            .map_primary(|pair| self.parse_primary(pair))
            .map_prefix(|op, rhs| {
                let rhs = rhs?;
                Ok(match op.as_rule() {
                    Rule::neg => Expr::unary(UnaryOp::Neg, rhs),
                    Rule::pos => Expr::unary(UnaryOp::Pos, rhs),
                    _ => unreachable!(),
                })
            })
            .map_infix(|lhs, op, rhs| {
                let lhs = lhs?;
                let rhs = rhs?;
                let bin_op = match op.as_rule() {
                    Rule::add => BinaryOp::Add,
                    Rule::sub => BinaryOp::Sub,
                    Rule::mul => BinaryOp::Mul,
                    Rule::div => BinaryOp::Div,
                    Rule::pow => BinaryOp::Pow,
                    Rule::concat => BinaryOp::Concat,
                    Rule::eq => BinaryOp::Eq,
                    Rule::neq => BinaryOp::Neq,
                    Rule::lt => BinaryOp::Lt,
                    Rule::lte => BinaryOp::Lte,
                    Rule::gt => BinaryOp::Gt,
                    Rule::gte => BinaryOp::Gte,
                    _ => unreachable!(),
                };
                Ok(Expr::binary(bin_op, lhs, rhs))
            })
            .parse(pairs)
    }

    fn parse_primary(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        match pair.as_rule() {
            Rule::number => self.parse_number(pair),
            Rule::string => self.parse_string(pair),
            Rule::boolean => self.parse_boolean(pair),
            Rule::error_literal => self.parse_error(pair),
            Rule::cell_ref => self.parse_cell_ref(pair),
            Rule::range_ref => self.parse_range_ref(pair),
            Rule::function_call => self.parse_function(pair),
            Rule::expr => self.parse_expression(pair.into_inner()),
            _ => Err(ParseError::UnexpectedRule(pair.as_rule())),
        }
    }

    fn parse_number(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let s = pair.as_str();
        let n: f64 = s
            .parse()
            .map_err(|_| ParseError::InvalidNumber(s.to_string()))?;
        Ok(Expr::Number(n))
    }

    fn parse_string(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let s = pair.as_str();
        // Remove surrounding quotes and unescape ""
        let inner = &s[1..s.len() - 1];
        let unescaped = inner.replace("\"\"", "\"");
        Ok(Expr::Text(unescaped))
    }

    fn parse_boolean(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let s = pair.as_str().to_uppercase();
        Ok(Expr::Bool(s == "TRUE"))
    }

    fn parse_error(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let error = match pair.as_str() {
            "#NULL!" => CellError::Null,
            "#DIV/0!" => CellError::DivZero,
            "#VALUE!" => CellError::Value,
            "#REF!" => CellError::Ref,
            "#NAME?" => CellError::Name,
            "#NUM!" => CellError::Num,
            "#N/A" => CellError::NA,
            "#CALC!" => CellError::Calc,
            "#SPILL!" => CellError::Spill,
            _ => CellError::Value,
        };
        Ok(Expr::Error(error))
    }

    fn parse_cell_ref(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let mut sheet: Option<String> = None;
        let mut coord: Option<CellCoord> = None;
        let mut row_absolute = false;
        let mut col_absolute = false;

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::sheet_prefix => {
                    let sheet_pair = inner.into_inner().next().unwrap();
                    let name = match sheet_pair.as_rule() {
                        Rule::quoted_sheet_name => {
                            let s = sheet_pair.as_str();
                            s[1..s.len() - 1].replace("''", "'")
                        }
                        Rule::sheet_name => sheet_pair.as_str().to_string(),
                        _ => unreachable!(),
                    };
                    sheet = Some(name);
                }
                Rule::cell_address => {
                    let addr = inner.as_str();
                    let (parsed_coord, row_abs, col_abs) = parse_cell_address(addr)?;
                    coord = Some(parsed_coord);
                    row_absolute = row_abs;
                    col_absolute = col_abs;
                }
                _ => {}
            }
        }

        let coord = coord.ok_or_else(|| ParseError::InvalidCellRef("missing address".into()))?;

        Ok(Expr::CellRef(CellRef {
            sheet,
            coord,
            row_absolute,
            col_absolute,
        }))
    }

    fn parse_range_ref(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let mut inner = pair.into_inner();
        let start = self.parse_cell_ref(inner.next().unwrap())?;
        let end = self.parse_cell_ref(inner.next().unwrap())?;

        let (start_ref, end_ref) = match (start, end) {
            (Expr::CellRef(s), Expr::CellRef(e)) => (s, e),
            _ => return Err(ParseError::InvalidCellRef("invalid range".into())),
        };

        // Use sheet from start ref (Excel behavior)
        Ok(Expr::RangeRef(RangeRef {
            sheet: start_ref.sheet,
            range: CellRange::new(start_ref.coord, end_ref.coord),
        }))
    }

    fn parse_function(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_uppercase();

        let mut args = Vec::new();
        if let Some(arg_list) = inner.next() {
            for arg_pair in arg_list.into_inner() {
                if arg_pair.as_rule() == Rule::expr {
                    args.push(self.parse_expression(arg_pair.into_inner())?);
                }
            }
        }

        Ok(Expr::function(name, args))
    }
}

impl Default for FormulaParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse cell address like "$A$1" into coordinate and absolute flags
fn parse_cell_address(s: &str) -> Result<(CellCoord, bool, bool), ParseError> {
    let mut col_absolute = false;
    let mut row_absolute = false;
    let mut col_str = String::new();
    let mut row_str = String::new();
    let mut in_col = true;

    for c in s.chars() {
        if c == '$' {
            if in_col && col_str.is_empty() {
                col_absolute = true;
            } else if !in_col || !col_str.is_empty() {
                row_absolute = true;
            }
            continue;
        }

        if in_col && c.is_ascii_alphabetic() {
            col_str.push(c.to_ascii_uppercase());
        } else if c.is_ascii_digit() {
            in_col = false;
            row_str.push(c);
        }
    }

    let coord = CellCoord::from_a1(&format!("{}{}", col_str, row_str))
        .ok_or_else(|| ParseError::InvalidCellRef(s.to_string()))?;

    Ok((coord, row_absolute, col_absolute))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> FormulaParser {
        FormulaParser::new()
    }

    #[test]
    fn test_simple_number() {
        let expr = parser().parse("=42").unwrap();
        assert_eq!(expr, Expr::Number(42.0));
    }

    #[test]
    fn test_arithmetic() {
        let expr = parser().parse("=1+2*3").unwrap();
        // Should parse as 1 + (2 * 3) due to precedence
        match expr {
            Expr::Binary { op: BinaryOp::Add, left, right } => {
                assert_eq!(*left, Expr::Number(1.0));
                match *right {
                    Expr::Binary { op: BinaryOp::Mul, .. } => {}
                    _ => panic!("Expected multiplication"),
                }
            }
            _ => panic!("Expected addition"),
        }
    }

    #[test]
    fn test_cell_ref() {
        let expr = parser().parse("=A1").unwrap();
        match expr {
            Expr::CellRef(r) => {
                assert_eq!(r.coord, CellCoord::new(0, 0));
                assert!(!r.row_absolute);
                assert!(!r.col_absolute);
            }
            _ => panic!("Expected cell ref"),
        }
    }

    #[test]
    fn test_absolute_ref() {
        let expr = parser().parse("=$A$1").unwrap();
        match expr {
            Expr::CellRef(r) => {
                assert!(r.row_absolute);
                assert!(r.col_absolute);
            }
            _ => panic!("Expected cell ref"),
        }
    }

    #[test]
    fn test_function() {
        let expr = parser().parse("=SUM(A1:B2, 10)").unwrap();
        match expr {
            Expr::Function(f) => {
                assert_eq!(f.name, "SUM");
                assert_eq!(f.args.len(), 2);
            }
            _ => panic!("Expected function"),
        }
    }

    #[test]
    fn test_string() {
        let expr = parser().parse("=\"hello\"").unwrap();
        assert_eq!(expr, Expr::Text("hello".to_string()));
    }

    #[test]
    fn test_comparison() {
        let expr = parser().parse("=A1>10").unwrap();
        match expr {
            Expr::Binary { op: BinaryOp::Gt, .. } => {}
            _ => panic!("Expected comparison"),
        }
    }
}
