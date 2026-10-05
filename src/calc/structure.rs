//! Whole-sheet edits: inserting and deleting rows and columns, sorting, and
//! applying an AutoFilter.

use super::engine::{CalcEngine, CellInput, CellResult};
use crate::cell::{Axis, CellCoord, CellRange, LineEdit};
use crate::format::display_text;
use crate::formula::{FormulaParser, RefMut};
use std::cmp::Ordering;

/// One sort level: a column and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    pub col: u32,
    pub ascending: bool,
}

/// Every cell and all formatting, for undoing whole-sheet edits.
#[derive(Clone, Default)]
pub struct EngineSnapshot {
    inputs: Vec<((u32, CellCoord), CellInput)>,
    formatting: std::collections::HashMap<u32, crate::format::SheetFormatting>,
}

impl CalcEngine {
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            inputs: self.all_inputs().map(|(k, v)| (k, v.clone())).collect(),
            formatting: self.all_formatting().clone(),
        }
    }

    pub fn restore(&mut self, snapshot: &EngineSnapshot) {
        self.take_all_inputs();
        for ((sheet, coord), input) in &snapshot.inputs {
            match input {
                CellInput::Empty => {}
                CellInput::Value(v) => self.set_value(*sheet, *coord, v.clone()),
                CellInput::Formula(f) => {
                    let _ = self.set_formula(*sheet, *coord, f);
                }
            }
        }
        *self.all_formatting_mut() = snapshot.formatting.clone();
    }

    /// Insert or delete rows or columns on `sheet`. Cells, formats and layout
    /// move, and formulas on every sheet that refer to `sheet` follow them.
    /// References to deleted cells become `#REF!`, as in Excel.
    pub fn apply_line_edit(&mut self, sheet: u32, edit: &LineEdit) {
        let parser = FormulaParser::new();
        let cells: Vec<(u32, CellCoord, CellInput)> = self
            .take_all_inputs()
            .into_iter()
            .map(|((s, c), input)| (s, c, input))
            .collect();

        for (s, coord, input) in cells {
            let coord = if s == sheet {
                match edit.map_coord(coord) {
                    Some(c) => c,
                    None => continue,
                }
            } else {
                coord
            };
            match input {
                CellInput::Empty => {}
                CellInput::Value(v) => self.set_value(s, coord, v),
                CellInput::Formula(f) => {
                    let text = self
                        .rewrite_references(&parser, &f, s, sheet, |r| match r {
                            RefMut::Cell(c) => match edit.map_coord(c.coord) {
                                Some(moved) => {
                                    c.coord = moved;
                                    true
                                }
                                None => false,
                            },
                            RefMut::Range(r) => match edit.map_range(r.range) {
                                Some(moved) => {
                                    r.range = moved;
                                    true
                                }
                                None => false,
                            },
                        })
                        .unwrap_or(f);
                    let _ = self.set_formula(s, coord, &text);
                }
            }
        }
        self.formatting_mut(sheet).apply_line_edit(edit);
        self.move_pivot_sources(sheet, edit);
    }

    /// Re-parse `formula` (on sheet `formula_sheet`), apply `f` to each
    /// reference that points at `target_sheet`, and return the new text if
    /// anything changed.
    fn rewrite_references(
        &self,
        parser: &FormulaParser,
        formula: &str,
        formula_sheet: u32,
        target_sheet: u32,
        mut f: impl FnMut(RefMut<'_>) -> bool,
    ) -> Option<String> {
        let mut expr = parser.parse(formula).ok()?;
        let before = expr.clone();
        expr.visit_references_mut(&mut |r| {
            let qualifier = match &r {
                RefMut::Cell(c) => c.sheet.clone(),
                RefMut::Range(r) => r.sheet.clone(),
            };
            if self.resolve_sheet(qualifier.as_deref(), formula_sheet) == Ok(target_sheet) {
                f(r)
            } else {
                true
            }
        });
        (expr != before).then(|| format!("={expr}"))
    }

    /// Sort the rows of `range` by `keys`. With `has_header`, the first row
    /// stays put. Formulas move like copies, so references to their own row
    /// follow them; cell formats move with their rows. Blanks sort last in
    /// either direction, as in Excel. Returns whether any row moved.
    pub fn sort_range(
        &mut self,
        sheet: u32,
        range: CellRange,
        keys: &[SortKey],
        has_header: bool,
    ) -> bool {
        let first = range.start.row + u32::from(has_header);
        if first >= range.end.row || keys.is_empty() {
            return false;
        }
        let rows: Vec<u32> = (first..=range.end.row).collect();
        let formatting = self.formatting(sheet).cloned().unwrap_or_default();
        let key_of = |row: u32, col: u32| -> SortValue {
            let coord = CellCoord::new(row, col);
            sort_value(&self.get_value(sheet, coord))
        };
        let mut order = rows.clone();
        order.sort_by(|&a, &b| {
            for key in keys {
                let ord = compare(&key_of(a, key.col), &key_of(b, key.col), key.ascending);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.cmp(&b)
        });
        if order == rows {
            return false;
        }

        // Take every cell of the moving rows, then put them back in order.
        let cols = range.start.col..=range.end.col;
        let mut taken = Vec::new();
        for &row in &rows {
            for col in cols.clone() {
                let coord = CellCoord::new(row, col);
                let input = self.get_input(sheet, coord).cloned();
                let format = formatting.get(coord).cloned();
                let note = self.formatting_mut(sheet).notes.remove(&coord);
                if input.is_some() {
                    self.clear(sheet, coord);
                }
                if format.is_some() {
                    self.set_cell_format(sheet, coord, Default::default());
                }
                taken.push((row, col, input, format, note));
            }
        }
        let parser = FormulaParser::new();
        for (new_index, &old_row) in order.iter().enumerate() {
            let new_row = first + new_index as u32;
            for (_, col, input, format, note) in taken.iter().filter(|t| t.0 == old_row) {
                let coord = CellCoord::new(new_row, *col);
                if let Some(note) = note {
                    self.formatting_mut(sheet).notes.insert(coord, note.clone());
                }
                if let Some(format) = format {
                    self.set_cell_format(sheet, coord, format.clone());
                }
                match input {
                    Some(CellInput::Value(v)) => self.set_value(sheet, coord, v.clone()),
                    Some(CellInput::Formula(f)) => {
                        let moved = match parser.parse(f) {
                            Ok(mut expr) if new_row != old_row => {
                                expr.offset_references(new_row as i64 - old_row as i64, 0);
                                format!("={expr}")
                            }
                            _ => f.clone(),
                        };
                        let _ = self.set_formula(sheet, coord, &moved);
                    }
                    Some(CellInput::Empty) | None => {}
                }
            }
        }
        true
    }

    /// Hide the filter's data rows that don't match, and show the rest.
    pub fn refresh_filter(&mut self, sheet: u32) {
        let Some(filter) = self.formatting(sheet).and_then(|f| f.filter.clone()) else {
            return;
        };
        let formatting = self.formatting(sheet).cloned().unwrap_or_default();
        let range = filter.range;
        let mut hide = Vec::new();
        let mut show = Vec::new();
        for row in range.start.row + 1..=range.end.row {
            let visible = filter.allowed.iter().all(|(&offset, allowed)| {
                let coord = CellCoord::new(row, range.start.col + offset);
                let value = self.get_value(sheet, coord);
                allowed.contains(&display_text(&value, formatting.effective(coord)))
            });
            if visible {
                show.push(row);
            } else {
                hide.push(row);
            }
        }
        let f = self.formatting_mut(sheet);
        for row in show {
            f.hidden_rows.remove(&row);
        }
        f.hidden_rows.extend(hide);
    }

    /// The distinct displayed values in one filter column, for its menu.
    pub fn filter_values(&self, sheet: u32, range: CellRange, col: u32) -> Vec<String> {
        let formatting = self.formatting(sheet);
        let mut values: Vec<(SortValue, String)> = (range.start.row + 1..=range.end.row)
            .map(|row| {
                let coord = CellCoord::new(row, col);
                let value = self.get_value(sheet, coord);
                let text = display_text(&value, formatting.and_then(|f| f.effective(coord)));
                (sort_value(&value), text)
            })
            .collect();
        values.sort_by(|a, b| compare(&a.0, &b.0, true));
        let mut out: Vec<String> = Vec::new();
        for (_, text) in values {
            if !out.contains(&text) {
                out.push(text);
            }
        }
        out
    }

    /// The contiguous block of data around `coord` (Excel's "current region"),
    /// or `None` if `coord` and its neighbors are empty.
    pub fn current_region(&self, sheet: u32, coord: CellCoord) -> Option<CellRange> {
        let filled = |r: u32, c: u32| self.get_input(sheet, CellCoord::new(r, c)).is_some();
        let (mut top, mut left, mut bottom, mut right) =
            (coord.row, coord.col, coord.row, coord.col);
        let any_in_row = |r: u32, l: u32, rt: u32| (l..=rt).any(|c| filled(r, c));
        let any_in_col = |c: u32, t: u32, b: u32| (t..=b).any(|r| filled(r, c));
        loop {
            let mut grew = false;
            // Diagonal neighbors count too, so look one beyond each edge.
            let (l, rt) = (
                left.saturating_sub(1),
                right.saturating_add(1).min(Axis::Column.max()),
            );
            let (t, b) = (
                top.saturating_sub(1),
                bottom.saturating_add(1).min(Axis::Row.max()),
            );
            if top > 0 && any_in_row(top - 1, l, rt) {
                top -= 1;
                grew = true;
            }
            if bottom < Axis::Row.max() && any_in_row(bottom + 1, l, rt) {
                bottom += 1;
                grew = true;
            }
            if left > 0 && any_in_col(left - 1, t, b) {
                left -= 1;
                grew = true;
            }
            if right < Axis::Column.max() && any_in_col(right + 1, t, b) {
                right += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        let region = CellRange::new(CellCoord::new(top, left), CellCoord::new(bottom, right));
        let empty = region.start == region.end && !filled(coord.row, coord.col);
        (!empty).then_some(region)
    }
}

/// Sort classes in Excel's ascending order; blanks are handled separately.
#[derive(Debug, Clone, PartialEq)]
enum SortValue {
    Number(f64),
    Text(String),
    Bool(bool),
    Error,
    Blank,
}

fn sort_value(v: &CellResult) -> SortValue {
    match v {
        CellResult::Empty => SortValue::Blank,
        CellResult::Value(n) => SortValue::Number(*n),
        CellResult::Text(s) if s.is_empty() => SortValue::Blank,
        CellResult::Text(s) => SortValue::Text(s.to_lowercase()),
        CellResult::Bool(b) => SortValue::Bool(*b),
        CellResult::Error(_) => SortValue::Error,
    }
}

fn compare(a: &SortValue, b: &SortValue, ascending: bool) -> Ordering {
    use SortValue::*;
    let rank = |v: &SortValue| match v {
        Number(_) => 0,
        Text(_) => 1,
        Bool(_) => 2,
        Error => 3,
        Blank => 4,
    };
    // Blanks go last whichever way the sort runs.
    match (a, b) {
        (Blank, Blank) => return Ordering::Equal,
        (Blank, _) => return Ordering::Greater,
        (_, Blank) => return Ordering::Less,
        _ => {}
    }
    let ord = match (a, b) {
        (Number(x), Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Text(x), Text(y)) => x.cmp(y),
        (Bool(x), Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    };
    if ascending { ord } else { ord.reverse() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::format::AutoFilter;
    use std::collections::{BTreeMap, BTreeSet};

    fn at(a1: &str) -> CellCoord {
        CellCoord::from_a1(a1).unwrap()
    }

    fn num(e: &mut CalcEngine, s: u32, a1: &str, n: f64) {
        e.set_value(s, at(a1), CellValueInput::Number(n));
    }

    fn text(e: &mut CalcEngine, s: u32, a1: &str, t: &str) {
        e.set_value(s, at(a1), CellValueInput::Text(t.into()));
    }

    #[test]
    fn inserting_rows_moves_cells_and_formulas() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Data".into(), "Other".into()]);
        num(&mut e, 0, "A1", 1.0);
        num(&mut e, 0, "A2", 2.0);
        num(&mut e, 0, "A3", 3.0);
        e.set_formula(0, at("A4"), "=SUM(A1:A3)").unwrap();
        e.set_formula(0, at("B1"), "=A3*10").unwrap();
        e.set_formula(1, at("A1"), "=Data!A3+1").unwrap();
        // A same-sheet ref on another sheet must not move.
        e.set_formula(1, at("A2"), "=A3").unwrap();

        // Insert one row above row 2.
        e.apply_line_edit(0, &LineEdit::insert(Axis::Row, 1, 1));
        assert_eq!(e.get_value(0, at("A3")), CellResult::Value(2.0));
        assert_eq!(e.get_formula(0, at("A5")).as_deref(), Some("=SUM(A1:A4)"));
        assert_eq!(e.get_value(0, at("A5")), CellResult::Value(6.0));
        assert_eq!(e.get_formula(0, at("B1")).as_deref(), Some("=(A4*10)"));
        assert_eq!(e.get_formula(1, at("A1")).as_deref(), Some("=(Data!A4+1)"));
        assert_eq!(e.get_formula(1, at("A2")).as_deref(), Some("=A3"));
    }

    #[test]
    fn deleting_referenced_cells_gives_ref_errors() {
        let mut e = CalcEngine::new();
        num(&mut e, 0, "B1", 5.0);
        num(&mut e, 0, "C1", 7.0);
        e.set_formula(0, at("A2"), "=B1+C1").unwrap();
        e.set_formula(0, at("A3"), "=SUM(B1:D1)").unwrap();
        // Delete column B.
        e.apply_line_edit(0, &LineEdit::delete(Axis::Column, 1, 1));
        assert_eq!(e.get_value(0, at("B1")), CellResult::Value(7.0));
        assert_eq!(
            e.get_value(0, at("A2")),
            CellResult::Error(crate::cell::CellError::Ref)
        );
        assert_eq!(e.get_formula(0, at("A3")).as_deref(), Some("=SUM(B1:C1)"));
        assert_eq!(e.get_value(0, at("A3")), CellResult::Value(7.0));
    }

    #[test]
    fn whole_columns_and_rows_follow_line_edits() {
        let mut e = CalcEngine::new();
        num(&mut e, 0, "B1", 5.0);
        num(&mut e, 0, "A3", 1.0);
        e.set_formula(0, at("D1"), "=SUM(B:B)").unwrap();
        e.set_formula(0, at("D2"), "=SUM(3:4)").unwrap();

        // Insert a column before B: rows don't change.
        e.apply_line_edit(0, &LineEdit::insert(Axis::Column, 1, 1));
        assert_eq!(e.get_formula(0, at("E1")).as_deref(), Some("=SUM(C:C)"));
        assert_eq!(e.get_formula(0, at("E2")).as_deref(), Some("=SUM(3:4)"));
        assert_eq!(e.get_value(0, at("E1")), CellResult::Value(5.0));

        // Delete row 4: the row range shrinks, the column range stays whole.
        e.apply_line_edit(0, &LineEdit::delete(Axis::Row, 3, 1));
        assert_eq!(e.get_formula(0, at("E1")).as_deref(), Some("=SUM(C:C)"));
        assert_eq!(e.get_formula(0, at("E2")).as_deref(), Some("=SUM(3:3)"));
        assert_eq!(e.get_value(0, at("E2")), CellResult::Value(1.0));

        // Delete column C.
        e.apply_line_edit(0, &LineEdit::delete(Axis::Column, 2, 1));
        assert_eq!(
            e.get_value(0, at("D1")),
            CellResult::Error(crate::cell::CellError::Ref)
        );
    }

    #[test]
    fn sorting_moves_rows_with_their_formulas_and_formats() {
        let mut e = CalcEngine::new();
        text(&mut e, 0, "A1", "Name");
        text(&mut e, 0, "B1", "Qty");
        for (i, (name, qty)) in [("pear", 3.0), ("Apple", 10.0), ("fig", 1.0)]
            .iter()
            .enumerate()
        {
            let row = i + 2;
            text(&mut e, 0, &format!("A{row}"), name);
            num(&mut e, 0, &format!("B{row}"), *qty);
            e.set_formula(0, at(&format!("C{row}")), &format!("=B{row}*2"))
                .unwrap();
        }
        e.set_cell_format(
            0,
            at("A3"),
            crate::format::CellFormat {
                bold: true,
                ..Default::default()
            },
        );
        let range = CellRange::from_a1("A1:C4").unwrap();

        e.sort_range(
            0,
            range,
            &[SortKey {
                col: 0,
                ascending: true,
            }],
            true,
        );
        assert_eq!(e.get_value(0, at("A1")), CellResult::Text("Name".into()));
        let names: Vec<CellResult> = (2..=4)
            .map(|r| e.get_value(0, at(&format!("A{r}"))))
            .collect();
        assert_eq!(
            names,
            ["Apple", "fig", "pear"]
                .map(|s| CellResult::Text(s.into()))
                .to_vec()
        );
        // Apple's row (10 * 2) came along, formula and all.
        assert_eq!(e.get_value(0, at("C2")), CellResult::Value(20.0));
        assert_eq!(e.get_formula(0, at("C2")).as_deref(), Some("=(B2*2)"));
        assert!(e.cell_format(0, at("A2")).is_some_and(|f| f.bold));

        e.sort_range(
            0,
            range,
            &[SortKey {
                col: 1,
                ascending: false,
            }],
            true,
        );
        let qty: Vec<CellResult> = (2..=4)
            .map(|r| e.get_value(0, at(&format!("B{r}"))))
            .collect();
        assert_eq!(qty, [10.0, 3.0, 1.0].map(CellResult::Value).to_vec());
    }

    #[test]
    fn blanks_sort_last_both_ways() {
        let mut e = CalcEngine::new();
        num(&mut e, 0, "A1", 2.0);
        num(&mut e, 0, "A3", 1.0);
        text(&mut e, 0, "A4", "x");
        let range = CellRange::from_a1("A1:A4").unwrap();
        e.sort_range(
            0,
            range,
            &[SortKey {
                col: 0,
                ascending: false,
            }],
            false,
        );
        assert_eq!(e.get_value(0, at("A1")), CellResult::Text("x".into()));
        assert_eq!(e.get_value(0, at("A4")), CellResult::Empty);
    }

    #[test]
    fn filters_hide_rows_that_do_not_match() {
        let mut e = CalcEngine::new();
        text(&mut e, 0, "A1", "Fruit");
        for (i, v) in ["Tea", "Cake", "Tea", "Pie"].iter().enumerate() {
            text(&mut e, 0, &format!("A{}", i + 2), v);
        }
        let range = CellRange::from_a1("A1:A5").unwrap();
        assert_eq!(e.filter_values(0, range, 0), vec!["Cake", "Pie", "Tea"]);
        let mut allowed = BTreeMap::new();
        allowed.insert(0, BTreeSet::from(["Tea".to_string()]));
        e.formatting_mut(0).filter = Some(AutoFilter { range, allowed });
        e.refresh_filter(0);
        let hidden: Vec<u32> = e
            .formatting(0)
            .unwrap()
            .hidden_rows
            .iter()
            .copied()
            .collect();
        assert_eq!(hidden, vec![2, 4]);
    }

    #[test]
    fn current_region_finds_the_block() {
        let mut e = CalcEngine::new();
        for a1 in ["B2", "C2", "B3", "D4"] {
            num(&mut e, 0, a1, 1.0);
        }
        // D4 touches C3 diagonally only through the C3/C2 corner: Excel joins it.
        assert_eq!(e.current_region(0, at("B2")), CellRange::from_a1("B2:D4"));
        assert_eq!(e.current_region(0, at("H9")), None);
    }
}
