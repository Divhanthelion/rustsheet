use crate::cell::{CellCoord, CellError, CellRange, MAX_COL, MAX_ROW, col_to_letters};
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

    /// The precedence level whose operators the parser joins into one
    /// [`Expr::Chain`]: `+` and `-`, `*` and `/`, or `&`. `None` for `^`,
    /// which groups to the right, and comparisons.
    pub fn chain_level(&self) -> Option<u8> {
        match self {
            BinaryOp::Concat => Some(0),
            BinaryOp::Add | BinaryOp::Sub => Some(1),
            BinaryOp::Mul | BinaryOp::Div => Some(2),
            _ => None,
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Pos,
    /// Postfix `%`: divides by 100
    Percent,
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

/// How a range reference is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RangeKind {
    /// `A1:B2`
    #[default]
    Cells,
    /// `A:B`, every row of the columns
    Columns,
    /// `1:2`, every column of the rows
    Rows,
}

/// Range reference
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeRef {
    pub sheet: Option<String>,
    /// Whole columns and rows span the full sheet here.
    pub range: CellRange,
    /// `$` on the start corner's (row, column)
    #[serde(default)]
    pub start_absolute: (bool, bool),
    /// `$` on the end corner's (row, column)
    #[serde(default)]
    pub end_absolute: (bool, bool),
    #[serde(default)]
    pub kind: RangeKind,
}

/// A reference found by [`Expr::visit_references_mut`].
pub enum RefMut<'a> {
    Cell(&'a mut CellRef),
    Range(&'a mut RangeRef),
}

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
    ((0..=MAX_ROW as i64).contains(&row) && (0..=MAX_COL as i64).contains(&col))
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
    /// Three or more operands joined left to right by operators of one
    /// [`BinaryOp::chain_level`], as the parser reads `A1+A2-A3`: one node
    /// however long the sum, so nothing walks it by deep recursion. Means
    /// the same as the left-deep `Binary` nodes it replaces.
    Chain {
        first: Box<Expr>,
        rest: Vec<(BinaryOp, Expr)>,
    },
    /// Function call
    Function(FunctionCall),
    /// An argument slot left empty, as in `PMT(r,n,pv,,1)`. Only ever a
    /// function argument.
    Missing,
    /// Array constant `{1,2;3,4}`: rows of literals, all the same length
    Array(Vec<Vec<Expr>>),
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

    /// The operands of a [`Expr::Chain`], first to last.
    pub fn chain_operands<'a>(
        first: &'a Expr,
        rest: &'a [(BinaryOp, Expr)],
    ) -> impl Iterator<Item = &'a Expr> {
        std::iter::once(first).chain(rest.iter().map(|(_, e)| e))
    }

    fn chain_operands_mut<'a>(
        first: &'a mut Expr,
        rest: &'a mut [(BinaryOp, Expr)],
    ) -> impl Iterator<Item = &'a mut Expr> {
        std::iter::once(first).chain(rest.iter_mut().map(|(_, e)| e))
    }

    /// Check if this expression references other cells
    pub fn has_dependencies(&self) -> bool {
        match self {
            Expr::CellRef(_) | Expr::RangeRef(_) => true,
            Expr::Binary { left, right, .. } => left.has_dependencies() || right.has_dependencies(),
            Expr::Unary { operand, .. } => operand.has_dependencies(),
            Expr::Chain { first, rest } => {
                Expr::chain_operands(first, rest).any(Expr::has_dependencies)
            }
            Expr::Function(f) => f.args.iter().any(|a| a.has_dependencies()),
            _ => false,
        }
    }

    /// Visit every cell and range reference. Returning `false` replaces that
    /// reference with `#REF!`.
    pub fn visit_references_mut(&mut self, f: &mut dyn FnMut(RefMut<'_>) -> bool) {
        let keep = match self {
            Expr::CellRef(r) => f(RefMut::Cell(r)),
            Expr::RangeRef(r) => f(RefMut::Range(r)),
            Expr::Binary { left, right, .. } => {
                left.visit_references_mut(f);
                right.visit_references_mut(f);
                true
            }
            Expr::Unary { operand, .. } => {
                operand.visit_references_mut(f);
                true
            }
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands_mut(first, rest) {
                    operand.visit_references_mut(f);
                }
                true
            }
            Expr::Function(func) => {
                for arg in &mut func.args {
                    arg.visit_references_mut(f);
                }
                true
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::Missing
            | Expr::Array(_) => true,
        };
        if !keep {
            *self = Expr::Error(CellError::Ref);
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
                // Whole columns only move sideways, whole rows only up or down.
                let (rows, cols) = match r.kind {
                    RangeKind::Cells => (rows, cols),
                    RangeKind::Columns => (0, cols),
                    RangeKind::Rows => (rows, 0),
                };
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
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands_mut(first, rest) {
                    operand.offset_references(rows, cols);
                }
            }
            Expr::Function(f) => {
                for arg in &mut f.args {
                    arg.offset_references(rows, cols);
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::Missing
            | Expr::Array(_) => {}
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
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands_mut(first, rest) {
                    operand.rename_sheet(old, new);
                }
            }
            Expr::Function(f) => {
                for arg in &mut f.args {
                    arg.rename_sheet(old, new);
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::Missing
            | Expr::Array(_) => {}
        }
    }

    /// Collect all cell references in this expression, expanding each range
    /// cell by cell (a whole column is a million entries; see
    /// [`Expr::collect_references`]).
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
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands(first, rest) {
                    operand.collect_dependencies(deps);
                }
            }
            Expr::Function(f) => {
                for arg in &f.args {
                    arg.collect_dependencies(deps);
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::Missing
            | Expr::Array(_) => {}
        }
    }

    /// Collect cell references and range references, ranges unexpanded.
    pub fn collect_references<'a>(
        &'a self,
        cells: &mut Vec<&'a CellRef>,
        ranges: &mut Vec<&'a RangeRef>,
    ) {
        match self {
            Expr::CellRef(r) => cells.push(r),
            Expr::RangeRef(r) => ranges.push(r),
            Expr::Binary { left, right, .. } => {
                left.collect_references(cells, ranges);
                right.collect_references(cells, ranges);
            }
            Expr::Unary { operand, .. } => operand.collect_references(cells, ranges),
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands(first, rest) {
                    operand.collect_references(cells, ranges);
                }
            }
            Expr::Function(f) => {
                for arg in &f.args {
                    arg.collect_references(cells, ranges);
                }
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::Missing
            | Expr::Array(_) => {}
        }
    }

    /// Whether the expression calls any of `names` (upper case, as parsed).
    pub fn calls_any(&self, names: &[&str]) -> bool {
        match self {
            Expr::Function(f) => {
                names.contains(&f.name.as_str()) || f.args.iter().any(|a| a.calls_any(names))
            }
            Expr::Binary { left, right, .. } => left.calls_any(names) || right.calls_any(names),
            Expr::Unary { operand, .. } => operand.calls_any(names),
            Expr::Chain { first, rest } => {
                Expr::chain_operands(first, rest).any(|e| e.calls_any(names))
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Bool(_)
            | Expr::Error(_)
            | Expr::CellRef(_)
            | Expr::RangeRef(_)
            | Expr::Missing
            | Expr::Array(_) => false,
        }
    }
}

fn sheet_prefix(name: Option<&str>) -> String {
    name.map_or_else(String::new, |n| format!("{}!", quote_sheet_name(n)))
}

/// A sheet name as a reference spells it: in quotes unless the parser's
/// bare `sheet_name` rule reads it (a letter or `_`, then letters, digits,
/// `_` and `.`), and also when Excel would read the bare name as something
/// else: a cell such as `A1`, an R1C1 address such as `R2C3`, or a logical.
pub fn quote_sheet_name(name: &str) -> String {
    let mut chars = name.chars();
    let bare = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    let upper = name.to_ascii_uppercase();
    let cell_like =
        CellCoord::from_a1(&upper).is_some_and(|c| c.row <= MAX_ROW && c.col <= MAX_COL);
    if bare && !cell_like && !is_r1c1(&upper) && upper != "TRUE" && upper != "FALSE" {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// R, C, R2, C3, RC, R2C, RC3, R2C3.
fn is_r1c1(upper: &str) -> bool {
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    match upper.strip_prefix('R') {
        Some(rest) => match rest.split_once('C') {
            Some((row, col)) => digits(row) && digits(col),
            None => digits(rest),
        },
        None => upper.strip_prefix('C').is_some_and(digits),
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
            Expr::RangeRef(r) => {
                let (start, end) = (r.range.start, r.range.end);
                let dollar = |absolute: bool| if absolute { "$" } else { "" };
                write!(f, "{}", sheet_prefix(r.sheet.as_deref()))?;
                match r.kind {
                    RangeKind::Cells => write!(
                        f,
                        "{}:{}",
                        start.to_a1_abs(r.start_absolute.0, r.start_absolute.1),
                        end.to_a1_abs(r.end_absolute.0, r.end_absolute.1)
                    ),
                    RangeKind::Columns => write!(
                        f,
                        "{}{}:{}{}",
                        dollar(r.start_absolute.1),
                        col_to_letters(start.col),
                        dollar(r.end_absolute.1),
                        col_to_letters(end.col)
                    ),
                    RangeKind::Rows => write!(
                        f,
                        "{}{}:{}{}",
                        dollar(r.start_absolute.0),
                        start.row + 1,
                        dollar(r.end_absolute.0),
                        end.row + 1
                    ),
                }
            }
            Expr::Binary { op, left, right } => {
                write!(f, "({}{}{})", left, op.as_str(), right)
            }
            // Unparenthesized inside, so it reads back as one chain.
            Expr::Chain { first, rest } => {
                write!(f, "({first}")?;
                for (op, operand) in rest {
                    write!(f, "{}{operand}", op.as_str())?;
                }
                write!(f, ")")
            }
            Expr::Unary { op, operand } => match op {
                UnaryOp::Neg => write!(f, "-{operand}"),
                UnaryOp::Pos => write!(f, "+{operand}"),
                // `%` binds tighter than a sign, so (-2)% keeps its parentheses.
                UnaryOp::Percent => match **operand {
                    Expr::Unary {
                        op: UnaryOp::Neg | UnaryOp::Pos,
                        ..
                    } => write!(f, "({operand})%"),
                    _ => write!(f, "{operand}%"),
                },
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
            // Nothing between its commas.
            Expr::Missing => Ok(()),
            Expr::Array(rows) => {
                write!(f, "{{")?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, ";")?;
                    }
                    for (j, item) in row.iter().enumerate() {
                        if j > 0 {
                            write!(f, ",")?;
                        }
                        write!(f, "{item}")?;
                    }
                }
                write!(f, "}}")
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
        assert_eq!(moved("=$A$1+A$1+$A1", 3, 3), "=($A$1+D$1+$A4)");
        assert_eq!(moved("=SUM(B2:B7)", 0, 1), "=SUM(C2:C7)");
        assert_eq!(moved("=SUM($B$2:$B$7)", 5, 5), "=SUM($B$2:$B$7)");
        assert_eq!(moved("=Sheet2!A1", 1, 0), "=Sheet2!A2");
        // Off the top of the sheet: #REF!, as in Excel.
        assert_eq!(moved("=A1*2", -1, 0), "=(#REF!*2)");
        assert_eq!(moved("=SUM(A1:A3)", 0, -1), "=SUM(#REF!)");
    }

    fn round_trip(formula: &str) -> String {
        let expr = crate::formula::FormulaParser::new().parse(formula).unwrap();
        format!("={expr}")
    }

    #[test]
    fn percent_and_leading_dot_round_trip() {
        assert_eq!(round_trip("=10%"), "=10%");
        assert_eq!(round_trip("=A1*5%"), "=(A1*5%)");
        assert_eq!(round_trip("=-2%"), "=-2%");
        assert_eq!(round_trip("=(-2)%"), "=(-2)%");
        assert_eq!(round_trip("=(A1+1)%"), "=(A1+1)%");
        assert_eq!(round_trip("=.5"), "=0.5");
        assert_eq!(round_trip("=-.25E1"), "=-2.5");
    }

    #[test]
    fn whole_columns_and_rows_round_trip() {
        assert_eq!(round_trip("=SUM(A:A)"), "=SUM(A:A)");
        assert_eq!(round_trip("=SUM($A:$C)"), "=SUM($A:$C)");
        assert_eq!(round_trip("=SUM(C:a)"), "=SUM(A:C)");
        assert_eq!(round_trip("=SUM(1:1)"), "=SUM(1:1)");
        assert_eq!(round_trip("=SUM(3:$5)"), "=SUM(3:$5)");
        assert_eq!(round_trip("=SUM(Sheet1!A:A)"), "=SUM(Sheet1!A:A)");
        assert_eq!(round_trip("=SUM('My Sheet'!B:D)"), "=SUM('My Sheet'!B:D)");

        let Expr::RangeRef(r) = crate::formula::FormulaParser::new().parse("=B:C").unwrap() else {
            panic!("expected a range");
        };
        assert_eq!(r.kind, RangeKind::Columns);
        assert_eq!(
            r.range,
            CellRange::new(CellCoord::new(0, 1), CellCoord::new(MAX_ROW, 2))
        );
    }

    #[test]
    fn copied_whole_columns_and_rows_move_along_their_axis() {
        assert_eq!(moved("=SUM(A:A)", 5, 1), "=SUM(B:B)");
        assert_eq!(moved("=SUM($A:B)", 5, 2), "=SUM($A:D)");
        assert_eq!(moved("=SUM(1:1)", 2, 3), "=SUM(3:3)");
        assert_eq!(moved("=SUM(1:$2)", 1, 0), "=SUM(2:$2)");
        // Off the edge: #REF!, as in Excel.
        assert_eq!(moved("=SUM(A:A)", 0, -1), "=SUM(#REF!)");
        assert_eq!(moved("=SUM(1:1)", -1, 0), "=SUM(#REF!)");
        assert_eq!(moved("=SUM(XFD:XFD)", 0, 1), "=SUM(#REF!)");
    }

    #[test]
    fn sheet_names_are_quoted_unless_bare_ones_read_back() {
        for (formula, shown) in [
            ("='2024'!A1", "='2024'!A1"),
            ("='My Sheet'!A1", "='My Sheet'!A1"),
            ("=Data.2024!A1", "=Data.2024!A1"),
            ("='Data.2024'!A1", "=Data.2024!A1"),
            ("=_2024!A1", "=_2024!A1"),
            ("='it''s'!A1:B2", "='it''s'!A1:B2"),
            ("=SUM('2024'!A:A,'1Q'!3:3)", "=SUM('2024'!A:A,'1Q'!3:3)"),
            // Names Excel would read as a cell, an R1C1 address or a logical
            ("='A1'!B2", "='A1'!B2"),
            ("='r2c3'!B2", "='r2c3'!B2"),
            ("='True'!B2", "='True'!B2"),
        ] {
            let shown_once = round_trip(formula);
            assert_eq!(shown_once, shown, "{formula}");
            assert_eq!(round_trip(&shown_once), shown, "{formula}");
        }
        assert_eq!(quote_sheet_name("Sheet1"), "Sheet1");
        assert_eq!(quote_sheet_name("Rates"), "Rates");
        assert_eq!(quote_sheet_name("XFD1048576"), "'XFD1048576'");
        // Past the last column or row: not a cell.
        assert_eq!(quote_sheet_name("XFE1"), "XFE1");
        assert_eq!(quote_sheet_name("A1048577"), "A1048577");
        assert_eq!(quote_sheet_name("RC"), "'RC'");
        assert_eq!(quote_sheet_name("C"), "'C'");
        assert_eq!(quote_sheet_name("Données"), "'Données'");
    }

    #[test]
    fn array_constants_round_trip() {
        assert_eq!(round_trip("={1,2;3,4}"), "={1,2;3,4}");
        assert_eq!(
            round_trip("=SUM({ -1.5, .5 ; \"a\"\"b\", true })"),
            "=SUM({-1.5,0.5;\"a\"\"b\",TRUE})"
        );
        assert_eq!(
            round_trip("=VLOOKUP(2,{1,\"a\";2,\"b\"},2)"),
            "=VLOOKUP(2,{1,\"a\";2,\"b\"},2)"
        );
        assert_eq!(round_trip("={#N/A,FALSE}"), "={#N/A,FALSE}");
        assert_eq!(moved("=SUM({1,2},A1)", 1, 0), "=SUM({1,2},A2)");
    }

    #[test]
    fn empty_argument_slots_round_trip() {
        assert_eq!(
            round_trip("=PMT(5%/12,360,,100000)"),
            "=PMT((5%/12),360,,100000)"
        );
        assert_eq!(round_trip("=IF(A1>0,,5)"), "=IF((A1>0),,5)");
        assert_eq!(round_trip("=IF(A1, 1, )"), "=IF(A1,1,)");
        assert_eq!(round_trip("=F(,)"), "=F(,)");
        assert_eq!(round_trip("=PI()"), "=PI()");
        // Moving and renaming pass over them.
        assert_eq!(moved("=SUM(A1,,B2)", 1, 1), "=SUM(B2,,C3)");
        let mut expr = crate::formula::FormulaParser::new()
            .parse("=SUM(Old!A1,,)")
            .unwrap();
        expr.rename_sheet("Old", "2024");
        assert_eq!(format!("={expr}"), "=SUM('2024'!A1,,)");
        let (mut cells, mut ranges) = (Vec::new(), Vec::new());
        expr.collect_references(&mut cells, &mut ranges);
        assert_eq!((cells.len(), ranges.len()), (1, 0));
    }
}
