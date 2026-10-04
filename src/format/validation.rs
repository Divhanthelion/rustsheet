//! Data validation rules, stored the way .xlsx stores them: a kind, an
//! operator and up to two operands written as Excel formulas ("10",
//! "DATE(2026,1,1)", "$B$1", or for lists `"Red,Green"` / `$A$1:$A$5`).

use crate::cell::{CellCoord, CellRange, LineEdit};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValidationKind {
    #[default]
    Any,
    Whole,
    Decimal,
    List,
    Date,
    Time,
    TextLength,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompareOp {
    #[default]
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

impl CompareOp {
    pub const ALL: [CompareOp; 8] = [
        CompareOp::Between,
        CompareOp::NotBetween,
        CompareOp::Equal,
        CompareOp::NotEqual,
        CompareOp::Greater,
        CompareOp::Less,
        CompareOp::GreaterOrEqual,
        CompareOp::LessOrEqual,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CompareOp::Between => "between",
            CompareOp::NotBetween => "not between",
            CompareOp::Equal => "equal to",
            CompareOp::NotEqual => "not equal to",
            CompareOp::Greater => "greater than",
            CompareOp::Less => "less than",
            CompareOp::GreaterOrEqual => "greater than or equal to",
            CompareOp::LessOrEqual => "less than or equal to",
        }
    }

    /// The .xlsx `operator` attribute.
    pub fn xml(self) -> &'static str {
        match self {
            CompareOp::Between => "between",
            CompareOp::NotBetween => "notBetween",
            CompareOp::Equal => "equal",
            CompareOp::NotEqual => "notEqual",
            CompareOp::Greater => "greaterThan",
            CompareOp::Less => "lessThan",
            CompareOp::GreaterOrEqual => "greaterThanOrEqual",
            CompareOp::LessOrEqual => "lessThanOrEqual",
        }
    }

    pub fn from_xml(s: &str) -> Self {
        CompareOp::ALL
            .into_iter()
            .find(|op| op.xml() == s)
            .unwrap_or_default()
    }

    pub fn needs_second(self) -> bool {
        matches!(self, CompareOp::Between | CompareOp::NotBetween)
    }

    pub fn test(self, v: f64, a: f64, b: f64) -> bool {
        let (lo, hi) = (a.min(b), a.max(b));
        match self {
            CompareOp::Between => v >= lo && v <= hi,
            CompareOp::NotBetween => v < lo || v > hi,
            CompareOp::Equal => v == a,
            CompareOp::NotEqual => v != a,
            CompareOp::Greater => v > a,
            CompareOp::Less => v < a,
            CompareOp::GreaterOrEqual => v >= a,
            CompareOp::LessOrEqual => v <= a,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErrorStyle {
    /// Reject the entry
    #[default]
    Stop,
    /// Ask whether to keep it
    Warning,
    /// Tell, then keep it unless cancelled
    Information,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataValidation {
    pub ranges: Vec<CellRange>,
    pub kind: ValidationKind,
    pub operator: CompareOp,
    /// First operand as an Excel formula, without `=`
    pub formula1: String,
    pub formula2: Option<String>,
    /// Blank cells always pass
    pub allow_blank: bool,
    /// Lists show a drop-down button
    pub dropdown: bool,
    pub show_input: bool,
    pub input_title: String,
    pub input_message: String,
    pub show_error: bool,
    pub error_style: ErrorStyle,
    pub error_title: String,
    pub error_message: String,
}

impl Default for DataValidation {
    fn default() -> Self {
        Self {
            ranges: Vec::new(),
            kind: ValidationKind::Any,
            operator: CompareOp::Between,
            formula1: String::new(),
            formula2: None,
            allow_blank: true,
            dropdown: true,
            show_input: true,
            input_title: String::new(),
            input_message: String::new(),
            show_error: true,
            error_style: ErrorStyle::Stop,
            error_title: String::new(),
            error_message: String::new(),
        }
    }
}

impl DataValidation {
    pub fn covers(&self, coord: CellCoord) -> bool {
        self.ranges.iter().any(|r| contains(*r, coord))
    }

    /// Top-left of the first range: relative references in the operands
    /// are relative to this cell.
    pub fn origin(&self) -> CellCoord {
        self.ranges
            .first()
            .map_or(CellCoord::new(0, 0), |r| r.start)
    }

    /// A list given as literal values (`"Red,Green"`), if it is one.
    pub fn literal_items(&self) -> Option<Vec<String>> {
        let f = self.formula1.trim();
        let inner = f.strip_prefix('"')?.strip_suffix('"')?;
        Some(
            inner
                .replace("\"\"", "\"")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }
}

pub fn contains(r: CellRange, c: CellCoord) -> bool {
    (r.start.row..=r.end.row).contains(&c.row) && (r.start.col..=r.end.col).contains(&c.col)
}

/// `r` minus `cut`: up to four rectangles.
pub fn subtract(r: CellRange, cut: CellRange) -> Vec<CellRange> {
    let overlaps = r.start.row <= cut.end.row
        && r.end.row >= cut.start.row
        && r.start.col <= cut.end.col
        && r.end.col >= cut.start.col;
    if !overlaps {
        return vec![r];
    }
    let mut out = Vec::new();
    let rect = |r0, c0, r1, c1| CellRange::new(CellCoord::new(r0, c0), CellCoord::new(r1, c1));
    if r.start.row < cut.start.row {
        out.push(rect(r.start.row, r.start.col, cut.start.row - 1, r.end.col));
    }
    if r.end.row > cut.end.row {
        out.push(rect(cut.end.row + 1, r.start.col, r.end.row, r.end.col));
    }
    let (top, bottom) = (r.start.row.max(cut.start.row), r.end.row.min(cut.end.row));
    if r.start.col < cut.start.col {
        out.push(rect(top, r.start.col, bottom, cut.start.col - 1));
    }
    if r.end.col > cut.end.col {
        out.push(rect(top, cut.end.col + 1, bottom, r.end.col));
    }
    out
}

/// Follow inserted/deleted rows and columns.
pub fn apply_line_edit(list: &mut Vec<DataValidation>, edit: &LineEdit) {
    for dv in list.iter_mut() {
        dv.ranges = dv
            .ranges
            .iter()
            .filter_map(|r| edit.map_range(*r))
            .collect();
    }
    list.retain(|dv| !dv.ranges.is_empty());
}

/// Remove validation from `area`, keeping it on the rest of each range.
pub fn clear_area(list: &mut Vec<DataValidation>, area: CellRange) {
    for dv in list.iter_mut() {
        dv.ranges = dv.ranges.iter().flat_map(|r| subtract(*r, area)).collect();
    }
    list.retain(|dv| !dv.ranges.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(a1: &str) -> CellRange {
        CellRange::from_a1(a1).unwrap()
    }

    #[test]
    fn subtracting_ranges() {
        let parts = subtract(r("A1:C3"), r("B2:B2"));
        let cells: usize = parts
            .iter()
            .map(|p| ((p.end.row - p.start.row + 1) * (p.end.col - p.start.col + 1)) as usize)
            .sum();
        assert_eq!(cells, 8);
        assert!(parts.iter().all(|p| !contains(*p, CellCoord::new(1, 1))));
        assert_eq!(subtract(r("A1:A3"), r("C1:C3")), vec![r("A1:A3")]);
        assert!(subtract(r("A1:A3"), r("A1:B9")).is_empty());
    }

    #[test]
    fn literal_lists() {
        let dv = DataValidation {
            formula1: "\"Red, Green,Blue\"".into(),
            ..Default::default()
        };
        assert_eq!(dv.literal_items().unwrap(), vec!["Red", "Green", "Blue"]);
        let range = DataValidation {
            formula1: "$A$1:$A$3".into(),
            ..Default::default()
        };
        assert!(range.literal_items().is_none());
    }

    #[test]
    fn comparisons() {
        assert!(CompareOp::Between.test(5.0, 10.0, 1.0));
        assert!(!CompareOp::NotBetween.test(5.0, 1.0, 10.0));
        assert!(CompareOp::GreaterOrEqual.test(3.0, 3.0, 0.0));
        assert_eq!(CompareOp::from_xml("lessThan"), CompareOp::Less);
    }
}
