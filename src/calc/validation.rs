//! Checking cells against data validation rules.

use super::engine::{CalcEngine, CellResult};
use crate::cell::CellCoord;
use crate::format::validation::{DataValidation, ValidationKind};
use crate::format::{display_text, parse_typed_number};
use crate::formula::{Expr, FormulaParser, RefMut};

impl CalcEngine {
    /// The validation rule covering a cell, if any (the last one wins).
    pub fn validation_at(&self, sheet: u32, coord: CellCoord) -> Option<&DataValidation> {
        self.formatting(sheet)?
            .validations
            .iter()
            .rev()
            .find(|dv| dv.covers(coord))
    }

    /// Evaluate a validation operand for `coord`: a number, a date typed
    /// like 2026-01-31, or a formula whose relative references are relative
    /// to the rule's top-left cell.
    fn operand(&self, sheet: u32, dv: &DataValidation, text: &str, coord: CellCoord) -> CellResult {
        let t = text.trim();
        if let Some((n, _)) = parse_typed_number(t) {
            return CellResult::Value(n);
        }
        let Some(expr) = self.relative_expr(dv, t, coord) else {
            return CellResult::Error(crate::cell::CellError::Value);
        };
        self.evaluate_expr(sheet, &expr)
    }

    fn relative_expr(&self, dv: &DataValidation, formula: &str, coord: CellCoord) -> Option<Expr> {
        let text = formula.trim().trim_start_matches('=');
        let mut expr = FormulaParser::new().parse(&format!("={text}")).ok()?;
        let origin = dv.origin();
        expr.offset_references(
            coord.row as i64 - origin.row as i64,
            coord.col as i64 - origin.col as i64,
        );
        Some(expr)
    }

    /// The choices of a list rule, as shown.
    pub fn list_items(&self, sheet: u32, dv: &DataValidation) -> Vec<String> {
        if let Some(items) = dv.literal_items() {
            return items;
        }
        let Some(mut expr) = self.relative_expr(dv, &dv.formula1, dv.origin()) else {
            return Vec::new();
        };
        let mut found = None;
        expr.visit_references_mut(&mut |r| {
            if found.is_none() {
                found = Some(match r {
                    RefMut::Range(r) => (r.sheet.clone(), r.range),
                    RefMut::Cell(c) => (c.sheet.clone(), crate::cell::CellRange::single(c.coord)),
                });
            }
            true
        });
        let Some((qualifier, range)) = found else {
            return Vec::new();
        };
        let Ok(src) = self.resolve_sheet(qualifier.as_deref(), sheet) else {
            return Vec::new();
        };
        let formatting = self.formatting(src);
        let mut items = Vec::new();
        for row in range.start.row..=range.end.row {
            for col in range.start.col..=range.end.col {
                let c = CellCoord::new(row, col);
                let text = display_text(
                    &self.get_value(src, c),
                    formatting.and_then(|f| f.effective(c)),
                );
                if !text.is_empty() && !items.contains(&text) {
                    items.push(text);
                }
            }
        }
        items
    }

    /// Whether the cell's current value satisfies `dv`.
    pub fn passes_validation(&self, sheet: u32, coord: CellCoord, dv: &DataValidation) -> bool {
        let value = self.get_value(sheet, coord);
        if matches!(value, CellResult::Empty)
            || matches!(&value, CellResult::Text(s) if s.is_empty())
        {
            return dv.allow_blank || dv.kind == ValidationKind::Any;
        }
        let num = |r: CellResult| match r {
            CellResult::Value(n) => Some(n),
            CellResult::Bool(b) => Some(b as u8 as f64),
            _ => None,
        };
        let compare = |v: f64| -> bool {
            let a = num(self.operand(sheet, dv, &dv.formula1, coord));
            let b = dv
                .formula2
                .as_deref()
                .map(|f| num(self.operand(sheet, dv, f, coord)));
            match (a, b) {
                (Some(a), Some(Some(b))) => dv.operator.test(v, a, b),
                (Some(a), None) if !dv.operator.needs_second() => dv.operator.test(v, a, a),
                // Can't evaluate the rule: don't block the user.
                _ => true,
            }
        };
        match dv.kind {
            ValidationKind::Any => true,
            ValidationKind::Whole => {
                matches!(value, CellResult::Value(n) if n.fract() == 0.0 && compare(n))
            }
            ValidationKind::Decimal | ValidationKind::Date | ValidationKind::Time => {
                matches!(value, CellResult::Value(n) if compare(n))
            }
            ValidationKind::TextLength => {
                let shown = display_text(
                    &value,
                    self.formatting(sheet).and_then(|f| f.effective(coord)),
                );
                compare(shown.chars().count() as f64)
            }
            ValidationKind::List => {
                let shown = display_text(
                    &value,
                    self.formatting(sheet).and_then(|f| f.effective(coord)),
                );
                let items = self.list_items(sheet, dv);
                items.is_empty() || items.iter().any(|i| i.eq_ignore_ascii_case(shown.trim()))
            }
            ValidationKind::Custom => {
                match self
                    .relative_expr(dv, &dv.formula1, coord)
                    .map(|e| self.evaluate_expr(sheet, &e))
                {
                    Some(CellResult::Bool(b)) => b,
                    Some(CellResult::Value(n)) => n != 0.0,
                    Some(CellResult::Error(_)) | None => false,
                    _ => true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::cell::CellRange;
    use crate::format::validation::CompareOp;

    fn at(a1: &str) -> CellCoord {
        CellCoord::from_a1(a1).unwrap()
    }

    fn rule(
        kind: ValidationKind,
        op: CompareOp,
        f1: &str,
        f2: Option<&str>,
        range: &str,
    ) -> DataValidation {
        DataValidation {
            ranges: vec![CellRange::from_a1(range).unwrap()],
            kind,
            operator: op,
            formula1: f1.into(),
            formula2: f2.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn numbers_dates_and_lengths() {
        let mut e = CalcEngine::new();
        let whole = rule(
            ValidationKind::Whole,
            CompareOp::Between,
            "1",
            Some("10"),
            "A1:A9",
        );
        e.set_value(0, at("A1"), CellValueInput::Number(5.0));
        assert!(e.passes_validation(0, at("A1"), &whole));
        e.set_value(0, at("A1"), CellValueInput::Number(5.5));
        assert!(!e.passes_validation(0, at("A1"), &whole));
        e.set_value(0, at("A1"), CellValueInput::Text("five".into()));
        assert!(!e.passes_validation(0, at("A1"), &whole));
        e.clear(0, at("A1"));
        assert!(e.passes_validation(0, at("A1"), &whole), "blanks pass");

        let date = rule(
            ValidationKind::Date,
            CompareOp::GreaterOrEqual,
            "2026-01-01",
            None,
            "B1",
        );
        e.set_value(0, at("B1"), CellValueInput::Number(45000.0));
        assert!(!e.passes_validation(0, at("B1"), &date));

        let len = rule(
            ValidationKind::TextLength,
            CompareOp::LessOrEqual,
            "3",
            None,
            "C1",
        );
        e.set_value(0, at("C1"), CellValueInput::Text("abcd".into()));
        assert!(!e.passes_validation(0, at("C1"), &len));
    }

    #[test]
    fn operands_can_be_relative_formulas() {
        let mut e = CalcEngine::new();
        // B is valid when it doesn't exceed A in the same row.
        let dv = rule(
            ValidationKind::Decimal,
            CompareOp::LessOrEqual,
            "A1",
            None,
            "B1:B9",
        );
        e.set_value(0, at("A3"), CellValueInput::Number(10.0));
        e.set_value(0, at("B3"), CellValueInput::Number(12.0));
        assert!(!e.passes_validation(0, at("B3"), &dv));
        e.set_value(0, at("B3"), CellValueInput::Number(8.0));
        assert!(e.passes_validation(0, at("B3"), &dv));

        let custom = rule(
            ValidationKind::Custom,
            CompareOp::Between,
            "ISNUMBER(C1)",
            None,
            "C1:C9",
        );
        e.set_value(0, at("C4"), CellValueInput::Text("x".into()));
        assert!(!e.passes_validation(0, at("C4"), &custom));
    }

    #[test]
    fn lists_from_values_and_ranges() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Main".into(), "Lists".into()]);
        for (i, v) in ["Tea", "Coffee", "Tea"].iter().enumerate() {
            e.set_value(
                1,
                CellCoord::new(i as u32, 0),
                CellValueInput::Text(v.to_string()),
            );
        }
        let from_range = rule(
            ValidationKind::List,
            CompareOp::Between,
            "Lists!$A$1:$A$3",
            None,
            "A1:A9",
        );
        assert_eq!(e.list_items(0, &from_range), vec!["Tea", "Coffee"]);
        e.set_value(0, at("A2"), CellValueInput::Text("coffee".into()));
        assert!(
            e.passes_validation(0, at("A2"), &from_range),
            "case-insensitive"
        );
        e.set_value(0, at("A2"), CellValueInput::Text("Juice".into()));
        assert!(!e.passes_validation(0, at("A2"), &from_range));

        let literal = rule(
            ValidationKind::List,
            CompareOp::Between,
            "\"S,M,L\"",
            None,
            "B1",
        );
        e.set_value(0, at("B1"), CellValueInput::Text("M".into()));
        assert!(e.passes_validation(0, at("B1"), &literal));
    }
}
