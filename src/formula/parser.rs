use crate::cell::{CellCoord, CellError, CellRange, MAX_COL, MAX_ROW};
use crate::formula::ast::{BinaryOp, CellRef, Expr, RangeKind, RangeRef, UnaryOp};
use crate::formula::grammar::{FormulaGrammar, Rule};
use pest::Parser;
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use thiserror::Error;

/// Longest formula Excel accepts, in characters.
pub const MAX_FORMULA_LEN: usize = 8192;
/// Deepest nesting of parentheses and function calls Excel accepts.
pub const MAX_NESTING: usize = 64;
/// Deepest expression tree the recursive passes over [`Expr`] (evaluation,
/// display, cloning) can take on a default-sized stack.
const MAX_TREE_DEPTH: usize = 512;

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
    #[error("Formula is longer than {} characters", MAX_FORMULA_LEN)]
    TooLong,
    #[error("Formula nests more than {} levels", MAX_NESTING)]
    TooDeep,
    #[error("Formula has too many operators")]
    TooComplex,
    #[error("Array rows must all be the same length")]
    RaggedArray,
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
            // Signs: -2^2 is (-2)^2, as in Excel
            .op(Op::prefix(Rule::neg) | Op::prefix(Rule::pos))
            // Percent binds tightest: 2^3% is 2^(3%), -2% is -(2%)
            .op(Op::postfix(Rule::percent));

        Self { pratt }
    }

    /// Parse a formula string (must start with '=')
    pub fn parse(&self, input: &str) -> Result<Expr, ParseError> {
        check_size(input)?;
        let pairs = FormulaGrammar::parse(Rule::formula, input)?;
        self.parse_formula(pairs)
    }

    /// Parse just an expression (without leading '=')
    pub fn parse_expr(&self, input: &str) -> Result<Expr, ParseError> {
        check_size(input)?;
        let pairs = FormulaGrammar::parse(Rule::bare_expr, input)?;
        self.parse_formula(pairs)
    }

    /// The expression inside a whole-input `formula` or `bare_expr` pair.
    fn parse_formula(&self, mut pairs: Pairs<Rule>) -> Result<Expr, ParseError> {
        let whole = pairs.next().ok_or(ParseError::UnexpectedRule(Rule::EOI))?;
        let inner = whole
            .into_inner()
            .find(|p| p.as_rule() == Rule::expr)
            .ok_or(ParseError::UnexpectedRule(Rule::EOI))?;
        check_tree_depth(&inner)?;
        self.parse_expression(inner.into_inner())
    }

    fn parse_expression(&self, pairs: Pairs<Rule>) -> Result<Expr, ParseError> {
        // `open` marks an operator node built at this level, which an
        // operator of the same level to its right extends into a chain;
        // a parenthesized group is closed, so (A1+A2)+A3 keeps its shape.
        struct Node {
            expr: Expr,
            open: bool,
        }
        let closed = |expr| Node { expr, open: false };
        self.pratt
            .map_primary(|pair| self.parse_primary(pair).map(closed))
            .map_prefix(|op, rhs| {
                let rhs = rhs?.expr;
                Ok(closed(match op.as_rule() {
                    Rule::neg => Expr::unary(UnaryOp::Neg, rhs),
                    Rule::pos => Expr::unary(UnaryOp::Pos, rhs),
                    _ => unreachable!(),
                }))
            })
            .map_postfix(|lhs, op| {
                let lhs = lhs?.expr;
                Ok(closed(match op.as_rule() {
                    Rule::percent => Expr::unary(UnaryOp::Percent, lhs),
                    _ => unreachable!(),
                }))
            })
            .map_infix(|lhs, op, rhs| {
                let lhs = lhs?;
                let rhs = rhs?.expr;
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
                let (level, open) = (bin_op.chain_level(), lhs.open);
                let extends = |op: BinaryOp| open && level.is_some() && level == op.chain_level();
                let expr = match lhs.expr {
                    Expr::Chain { first, mut rest }
                        if rest.first().is_some_and(|(op, _)| extends(*op)) =>
                    {
                        rest.push((bin_op, rhs));
                        Expr::Chain { first, rest }
                    }
                    Expr::Binary { op, left, right } if extends(op) => Expr::Chain {
                        first: left,
                        rest: vec![(op, *right), (bin_op, rhs)],
                    },
                    lhs => Expr::binary(bin_op, lhs, rhs),
                };
                Ok(Node { expr, open: true })
            })
            .parse(pairs)
            .map(|node| node.expr)
    }

    fn parse_primary(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        match pair.as_rule() {
            Rule::number => self.parse_number(pair),
            Rule::string => self.parse_string(pair),
            Rule::boolean => self.parse_boolean(pair),
            Rule::error_literal => self.parse_error(pair),
            Rule::cell_ref => self.parse_cell_ref(pair),
            Rule::range_ref => self.parse_range_ref(pair),
            Rule::column_range | Rule::row_range => self.parse_line_range(pair),
            Rule::function_call => self.parse_function(pair),
            Rule::array => self.parse_array(pair),
            Rule::expr => self.parse_expression(pair.into_inner()),
            _ => Err(ParseError::UnexpectedRule(pair.as_rule())),
        }
    }

    fn parse_array(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let rows = pair
            .into_inner()
            .map(|row| {
                row.into_inner()
                    .map(|item| self.parse_primary(item))
                    .collect()
            })
            .collect::<Result<Vec<Vec<Expr>>, _>>()?;
        if rows.iter().any(|row| row.len() != rows[0].len()) {
            return Err(ParseError::RaggedArray);
        }
        Ok(Expr::Array(rows))
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
                Rule::sheet_prefix => sheet = Some(parse_sheet_prefix(inner)),
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

        // CellRange::new puts the smaller row and column first; keep each
        // `$` with the corner it ends up on.
        let (s, e) = (&start_ref, &end_ref);
        let (top_row_abs, bottom_row_abs) = if s.coord.row <= e.coord.row {
            (s.row_absolute, e.row_absolute)
        } else {
            (e.row_absolute, s.row_absolute)
        };
        let (left_col_abs, right_col_abs) = if s.coord.col <= e.coord.col {
            (s.col_absolute, e.col_absolute)
        } else {
            (e.col_absolute, s.col_absolute)
        };

        // Use sheet from start ref (Excel behavior)
        Ok(Expr::RangeRef(RangeRef {
            range: CellRange::new(start_ref.coord, end_ref.coord),
            sheet: start_ref.sheet,
            start_absolute: (top_row_abs, left_col_abs),
            end_absolute: (bottom_row_abs, right_col_abs),
            kind: RangeKind::Cells,
        }))
    }

    /// Whole columns (`A:C`) or whole rows (`1:3`).
    fn parse_line_range(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let kind = match pair.as_rule() {
            Rule::column_range => RangeKind::Columns,
            _ => RangeKind::Rows,
        };
        let mut sheet = None;
        let mut ends = Vec::with_capacity(2);
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::sheet_prefix => sheet = Some(parse_sheet_prefix(inner)),
                Rule::column_address | Rule::row_address => {
                    ends.push(parse_line_address(inner.as_str(), kind)?)
                }
                _ => {}
            }
        }
        let [a, b] = ends[..] else {
            return Err(ParseError::InvalidCellRef("invalid range".into()));
        };
        // Smaller index first, each `$` staying with its index.
        let ((first, first_abs), (last, last_abs)) = if a.0 <= b.0 { (a, b) } else { (b, a) };

        let (range, start_absolute, end_absolute) = match kind {
            RangeKind::Columns => (
                CellRange::new(CellCoord::new(0, first), CellCoord::new(MAX_ROW, last)),
                (false, first_abs),
                (false, last_abs),
            ),
            _ => (
                CellRange::new(CellCoord::new(first, 0), CellCoord::new(last, MAX_COL)),
                (first_abs, false),
                (last_abs, false),
            ),
        };
        Ok(Expr::RangeRef(RangeRef {
            sheet,
            range,
            start_absolute,
            end_absolute,
            kind,
        }))
    }

    fn parse_function(&self, pair: Pair<Rule>) -> Result<Expr, ParseError> {
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_uppercase();

        let mut args = Vec::new();
        if let Some(arg_list) = inner.next() {
            for arg_pair in arg_list.into_inner() {
                match arg_pair.as_rule() {
                    Rule::expr => args.push(self.parse_expression(arg_pair.into_inner())?),
                    Rule::missing_arg => args.push(Expr::Missing),
                    _ => {}
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

/// Reject formulas over Excel's length or nesting limits before pest sees
/// them: pest recurses once per open parenthesis.
fn check_size(input: &str) -> Result<(), ParseError> {
    if input.chars().count() > MAX_FORMULA_LEN {
        return Err(ParseError::TooLong);
    }
    let mut depth = 0usize;
    let mut quote = None;
    for c in input.chars() {
        match (quote, c) {
            // Doubled quotes inside strings and sheet names toggle twice.
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(ParseError::TooDeep);
                }
            }
            (None, ')') => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

/// Reject expressions whose tree would be too deep to build or walk.
fn check_tree_depth(expr: &Pair<Rule>) -> Result<(), ParseError> {
    if tree_depth(expr) > MAX_TREE_DEPTH {
        return Err(ParseError::TooComplex);
    }
    Ok(())
}

/// Upper bound on the height of the tree an `expr` pair parses to. Signs,
/// `%`, `^` and comparisons each add at most one level above their deepest
/// operand. `&`, `+`/`-` and `*`/`/` add at most one level each however
/// many there are: a run of one of them is one [`Expr::Chain`], and a path
/// down the tree passes each of those levels once. Recurses only through
/// parentheses and calls, which `check_size` bounds.
fn tree_depth(expr: &Pair<Rule>) -> usize {
    let mut operators = 0;
    let mut chain_levels = [false; 3];
    let mut deepest = 0;
    for inner in expr.clone().into_inner() {
        match inner.as_rule() {
            Rule::expr => deepest = deepest.max(tree_depth(&inner)),
            Rule::function_call => {
                for args in inner.into_inner().filter(|p| p.as_rule() == Rule::arg_list) {
                    for arg in args.into_inner().filter(|p| p.as_rule() == Rule::expr) {
                        deepest = deepest.max(1 + tree_depth(&arg));
                    }
                }
            }
            Rule::concat => chain_levels[0] = true,
            Rule::add | Rule::sub => chain_levels[1] = true,
            Rule::mul | Rule::div => chain_levels[2] = true,
            Rule::neg
            | Rule::pos
            | Rule::percent
            | Rule::pow
            | Rule::eq
            | Rule::neq
            | Rule::lt
            | Rule::lte
            | Rule::gt
            | Rule::gte => operators += 1,
            _ => {}
        }
    }
    operators + chain_levels.iter().filter(|&&l| l).count() + 1 + deepest
}

/// Sheet name from a `sheet_prefix` pair, unquoted.
fn parse_sheet_prefix(pair: Pair<Rule>) -> String {
    let sheet_pair = pair.into_inner().next().unwrap();
    match sheet_pair.as_rule() {
        Rule::quoted_sheet_name => {
            let s = sheet_pair.as_str();
            s[1..s.len() - 1].replace("''", "'")
        }
        _ => sheet_pair.as_str().to_string(),
    }
}

/// Parse a column (`$C`) or row (`$3`) of a whole-line range into its index
/// and whether it is absolute.
fn parse_line_address(s: &str, kind: RangeKind) -> Result<(u32, bool), ParseError> {
    let absolute = s.starts_with('$');
    let bare = s.trim_start_matches('$');
    let index = match kind {
        RangeKind::Columns => CellCoord::from_a1(&format!("{bare}1"))
            .map(|c| c.col)
            .filter(|&c| c <= MAX_COL),
        _ => bare
            .parse::<u32>()
            .ok()
            .filter(|r| (1..=MAX_ROW + 1).contains(r))
            .map(|r| r - 1),
    };
    let index = index.ok_or_else(|| ParseError::InvalidCellRef(s.to_string()))?;
    Ok((index, absolute))
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
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => {
                assert_eq!(*left, Expr::Number(1.0));
                match *right {
                    Expr::Binary {
                        op: BinaryOp::Mul, ..
                    } => {}
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
    fn test_sheet_qualified_ref() {
        let expr = parser().parse("=Sheet2!A1").unwrap();
        match expr {
            Expr::CellRef(r) => {
                assert_eq!(r.sheet.as_deref(), Some("Sheet2"));
                assert_eq!(r.coord, CellCoord::new(0, 0));
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
            Expr::Binary {
                op: BinaryOp::Gt, ..
            } => {}
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn percent_binds_tighter_than_power_and_signs() {
        let pct = |e: Expr| Expr::unary(UnaryOp::Percent, e);
        assert_eq!(parser().parse("=10%").unwrap(), pct(Expr::Number(10.0)));
        assert_eq!(
            parser().parse("=-2%").unwrap(),
            Expr::unary(UnaryOp::Neg, pct(Expr::Number(2.0)))
        );
        assert_eq!(
            parser().parse("=2^3%").unwrap(),
            Expr::binary(BinaryOp::Pow, Expr::Number(2.0), pct(Expr::Number(3.0)))
        );
        assert_eq!(parser().parse("=.5").unwrap(), Expr::Number(0.5));
    }

    #[test]
    fn whole_rows_are_not_numbers() {
        let Expr::RangeRef(r) = parser().parse("=3:$5").unwrap() else {
            panic!("expected a range");
        };
        assert_eq!(r.kind, RangeKind::Rows);
        assert_eq!(
            r.range,
            CellRange::new(CellCoord::new(2, 0), CellCoord::new(4, MAX_COL))
        );
        assert_eq!(
            (r.start_absolute, r.end_absolute),
            ((false, false), (true, false))
        );
        assert!(parser().parse("=0:1").is_err());
        assert!(parser().parse("=XFE:XFE").is_err());
    }

    #[test]
    fn oversized_formulas_are_rejected() {
        let long = format!("=1{}", "+1".repeat(MAX_FORMULA_LEN / 2));
        assert!(matches!(parser().parse(&long), Err(ParseError::TooLong)));

        // Parentheses inside strings and sheet names don't count.
        let quoted = format!("=\"{}\"&'(('!A1", "(".repeat(100));
        assert!(parser().parse(&quoted).is_ok());
    }

    #[test]
    fn deep_nesting_is_rejected_without_crashing() {
        let nested = |n: usize| format!("={}1{}", "(".repeat(n), ")".repeat(n));
        assert!(parser().parse(&nested(MAX_NESTING)).is_ok());
        assert!(matches!(
            parser().parse(&nested(MAX_NESTING + 1)),
            Err(ParseError::TooDeep)
        ));
        assert!(matches!(
            parser().parse(&nested(4000)),
            Err(ParseError::TooDeep)
        ));
        assert!(parser().parse(&nested(100_000)).is_err());

        let calls = format!(
            "={}1{}",
            "ABS(".repeat(MAX_NESTING + 1),
            ")".repeat(MAX_NESTING + 1)
        );
        assert!(matches!(parser().parse(&calls), Err(ParseError::TooDeep)));
    }

    #[test]
    fn long_operator_chains_are_rejected_without_crashing() {
        for bomb in [
            format!("={}1", "-".repeat(8000)),
            format!("=2{}", "^2".repeat(4000)),
            format!("=1{}", "=1".repeat(4000)),
        ] {
            assert!(matches!(parser().parse(&bomb), Err(ParseError::TooComplex)));
        }
    }

    #[test]
    fn same_level_operators_chain_into_one_node() {
        let n = Expr::Number;
        let chain = |first: Expr, rest: Vec<(BinaryOp, Expr)>| Expr::Chain {
            first: Box::new(first),
            rest,
        };
        use BinaryOp::*;
        assert_eq!(
            parser().parse("=1+2-3+4").unwrap(),
            chain(n(1.0), vec![(Add, n(2.0)), (Sub, n(3.0)), (Add, n(4.0))])
        );
        assert_eq!(
            parser().parse("=1*2/3+4").unwrap(),
            Expr::binary(
                Add,
                chain(n(1.0), vec![(Mul, n(2.0)), (Div, n(3.0))]),
                n(4.0)
            )
        );
        assert_eq!(
            parser().parse("=1+2*3*4+5").unwrap(),
            chain(
                n(1.0),
                vec![
                    (Add, chain(n(2.0), vec![(Mul, n(3.0)), (Mul, n(4.0))])),
                    (Add, n(5.0))
                ]
            )
        );
        // Two operands stay a Binary, parentheses keep their grouping,
        // and ^ and comparisons never chain.
        assert_eq!(
            parser().parse("=1+2").unwrap(),
            Expr::binary(Add, n(1.0), n(2.0))
        );
        let one_two = Expr::binary(Add, n(1.0), n(2.0));
        assert_eq!(
            parser().parse("=(1+2)+3").unwrap(),
            Expr::binary(Add, one_two.clone(), n(3.0))
        );
        assert_eq!(
            parser().parse("=(1+2)+3+4").unwrap(),
            chain(one_two, vec![(Add, n(3.0)), (Add, n(4.0))])
        );
        assert_eq!(
            parser().parse("=1=2=3").unwrap(),
            Expr::binary(Eq, Expr::binary(Eq, n(1.0), n(2.0)), n(3.0))
        );
        assert_eq!(
            parser().parse("=2^3^4").unwrap(),
            Expr::binary(Pow, n(2.0), Expr::binary(Pow, n(3.0), n(4.0)))
        );
        // Chains show unparenthesized inside and read back the same.
        for (formula, shown) in [
            ("=A1+A2-A3", "=(A1+A2-A3)"),
            ("=\"a\"&B1&\"c\"", "=(\"a\"&B1&\"c\")"),
            ("=(1+2)+3+4", "=((1+2)+3+4)"),
            ("=-A1*2/B1%", "=(-A1*2/B1%)"),
            ("=(A1+A2+A3)%", "=(A1+A2+A3)%"),
            ("=1+2*3*4+5", "=(1+(2*3*4)+5)"),
        ] {
            let expr = parser().parse(formula).unwrap();
            assert_eq!(format!("={expr}"), shown, "{formula}");
            assert_eq!(parser().parse(shown).unwrap(), expr, "{formula}");
        }
    }

    #[test]
    fn chains_run_to_the_formula_length_limit() {
        // 1,500 cells: every term a reference, near Excel's 8,192 characters.
        let terms: Vec<String> = (1..=1500).map(|r| format!("A{r}")).collect();
        let sum = format!("={}", terms.join("+"));
        assert!(sum.len() > 7800);
        let Expr::Chain { rest, .. } = parser().parse(&sum).unwrap() else {
            panic!("expected a chain");
        };
        assert_eq!(rest.len(), 1499);
        // And 4,001 ones, the longest such formula.
        let ones = format!("=1{}", "+1".repeat(4000));
        let expr = parser().parse(&ones).unwrap();
        assert_eq!(parser().parse(&format!("={expr}")).unwrap(), expr);
        // Mixed levels still nest only once per level.
        let mixed = format!("=1{}", "+2*3&4".repeat(1000));
        assert!(parser().parse(&mixed).is_ok());
    }

    #[test]
    fn empty_argument_slots_parse_as_missing() {
        let args = |formula: &str| match parser().parse(formula).unwrap() {
            Expr::Function(f) => f.args,
            other => panic!("expected a call, got {other:?}"),
        };
        let n = Expr::Number;
        assert_eq!(
            args("=PMT(1,2,,4)"),
            vec![n(1.0), n(2.0), Expr::Missing, n(4.0)]
        );
        assert_eq!(args("=IF(1,,)"), vec![n(1.0), Expr::Missing, Expr::Missing]);
        assert_eq!(args("=F(,)"), vec![Expr::Missing, Expr::Missing]);
        assert_eq!(args("=F( , 1 )"), vec![Expr::Missing, n(1.0)]);
        // A lone empty slot is no argument.
        assert!(args("=PI()").is_empty());
        assert!(args("=PI( )").is_empty());
        assert!(parser().parse("=F(1,,").is_err());
        assert!(parser().parse("=(,)").is_err());

        // Single arguments are parsed once per level, not once per path.
        let nested = format!(
            "={}1{}",
            "ABS(".repeat(MAX_NESTING),
            ")".repeat(MAX_NESTING)
        );
        assert!(parser().parse(&nested).is_ok());
    }

    #[test]
    fn trailing_junk_is_an_error() {
        for bad in [
            "=1+,",
            "=A1 x",
            "=A1 B2",
            "=SUM(1) )",
            "=1 2",
            "=\"a\" \"b\"",
            "=A1;",
        ] {
            assert!(parser().parse(bad).is_err(), "{bad}");
        }
        assert!(parser().parse_expr("A1 x").is_err());
        // Blanks and line breaks around the expression are fine.
        let a1 = Expr::CellRef(CellRef::new(CellCoord::new(0, 0)));
        assert_eq!(parser().parse("=A1   ").unwrap(), a1);
        assert_eq!(parser().parse("=A1 \t\r\n").unwrap(), a1);
        assert_eq!(parser().parse_expr(" A1 ").unwrap(), a1);
        assert_eq!(
            parser().parse("=SUM(1,\n 2) ").unwrap(),
            Expr::function("SUM", vec![Expr::Number(1.0), Expr::Number(2.0)])
        );
        // Leading zeros and a bare trailing point stopped the number early,
        // so =007 was 0 with "07" ignored. Excel reads them as written.
        for (formula, n) in [("=007", 7.0), ("=00", 0.0), ("=1.", 1.0), ("=010.50", 10.5)] {
            assert_eq!(
                parser().parse(formula).unwrap(),
                Expr::Number(n),
                "{formula}"
            );
        }
        assert!(parser().parse("=1.5.3").is_err());
    }

    #[test]
    fn quoted_sheet_names_may_start_with_digits() {
        let Expr::CellRef(r) = parser().parse("='2024'!B2").unwrap() else {
            panic!("expected a cell");
        };
        assert_eq!(r.sheet.as_deref(), Some("2024"));
    }

    #[test]
    fn array_constants_hold_rows_of_literals() {
        let n = Expr::Number;
        assert_eq!(
            parser().parse("={1,2;3,4}").unwrap(),
            Expr::Array(vec![vec![n(1.0), n(2.0)], vec![n(3.0), n(4.0)]])
        );
        assert_eq!(
            parser().parse("={ -1.5 , \"a\" ; true , #N/A }").unwrap(),
            Expr::Array(vec![
                vec![n(-1.5), Expr::Text("a".into())],
                vec![Expr::Bool(true), Expr::Error(CellError::NA)],
            ])
        );
        assert!(matches!(
            parser().parse("={1,2;3}"),
            Err(ParseError::RaggedArray)
        ));
        // Literals only, as in Excel.
        for bad in ["={}", "={A1}", "={1+1}", "={(1)}", "={1,}", "={SUM(1)}"] {
            assert!(parser().parse(bad).is_err(), "{bad}");
        }
    }
}
