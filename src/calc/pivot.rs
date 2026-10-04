//! Reading a pivot table's source data from its sheet.

use super::engine::{CalcEngine, CellResult, CellValueInput};
use crate::cell::{CellCoord, CellRange, LineEdit, MAX_COL, MAX_ROW};
use crate::format::{CellFormat, Rgb, display_text, format_general, format_number};
use crate::pivot::{self, Aggregate, Item, PivotCell, PivotTable, RowKind};

/// Fill for header and grand total rows
const HEADER_FILL: Rgb = Rgb(0xDD, 0xEB, 0xF7);
/// The grid's default column width, in points
const DEFAULT_WIDTH: f32 = 80.0;

fn inside(r: CellRange, c: CellCoord) -> bool {
    (r.start.row..=r.end.row).contains(&c.row) && (r.start.col..=r.end.col).contains(&c.col)
}

/// A pivot table's source: field names, records (the rows under the field
/// names, without blank rows) and each field's number format.
#[derive(Clone, Debug, PartialEq)]
pub struct PivotSource {
    pub headers: Vec<String>,
    pub records: Vec<Vec<Item>>,
    pub formats: Vec<Option<String>>,
}

impl CalcEngine {
    pub fn pivot_source(&self, table: &PivotTable) -> Result<PivotSource, String> {
        let sheet = self
            .resolve_sheet(Some(&table.source_sheet), 0)
            .map_err(|_| {
                format!(
                    "The sheet \"{}\" with the PivotTable's data is gone.",
                    table.source_sheet
                )
            })?;
        let r = table.source;
        if r.end.row <= r.start.row {
            return Err(
                "The PivotTable's data needs a row of field names and at least one row of data."
                    .into(),
            );
        }
        let formatting = self.formatting(sheet);
        let item = |c: CellCoord| {
            let value = self.get_value(sheet, c);
            Item {
                text: display_text(&value, formatting.and_then(|f| f.effective(c))),
                number: match value {
                    CellResult::Value(n) => Some(n),
                    _ => None,
                },
            }
        };
        let cols = r.start.col..=r.end.col;
        let headers = cols
            .clone()
            .map(|col| item(CellCoord::new(r.start.row, col)).text)
            .collect();
        // Whole columns as a source stop at the last used row.
        let last = self
            .sheet_max_coord(sheet)
            .map_or(r.start.row, |m| m.row.min(r.end.row));
        let records = (r.start.row + 1..=last)
            .map(|row| {
                cols.clone()
                    .map(|col| item(CellCoord::new(row, col)))
                    .collect::<Vec<_>>()
            })
            .filter(|rec| rec.iter().any(|i| !i.text.is_empty()))
            .collect();
        let formats = cols
            .map(|col| {
                formatting
                    .and_then(|f| f.effective(CellCoord::new(r.start.row + 1, col)))
                    .and_then(|f| f.number_format.clone())
            })
            .collect();
        Ok(PivotSource {
            headers,
            records,
            formats,
        })
    }

    /// Lay out pivot table `index` on `sheet` and write its cells as values
    /// with formats, replacing what it wrote before. Nothing changes if it
    /// fails: the source is gone, or other data is in the way.
    pub fn refresh_pivot(&mut self, sheet: u32, index: usize) -> Result<(), String> {
        let table = self
            .formatting(sheet)
            .and_then(|f| f.pivots.get(index))
            .cloned()
            .ok_or("That PivotTable is gone.")?;
        let src = self.pivot_source(&table)?;
        let out = pivot::compute(&table, &src.headers, &src.records);
        let (h, w) = (out.cells.len() as u32, out.width() as u32);
        let a = table.anchor;
        if h == 0 || w == 0 {
            return Err("The PivotTable is empty.".into());
        }
        if a.row as u64 + h as u64 > MAX_ROW as u64 + 1
            || a.col as u64 + w as u64 > MAX_COL as u64 + 1
        {
            return Err("The PivotTable doesn't fit on the sheet there.".into());
        }
        let area = CellRange::new(a, CellCoord::new(a.row + h - 1, a.col + w - 1));
        let ours = |c: CellCoord| table.output.is_some_and(|o| inside(o, c));
        if self
            .iter_sheet_inputs(sheet)
            .any(|(c, _)| inside(area, c) && !ours(c))
        {
            return Err(
                "There's data where the PivotTable would go. Move it, or put the PivotTable somewhere else."
                    .into(),
            );
        }
        if let Some(old) = table.output {
            self.clear_area(sheet, old);
        }

        let mut widths = vec![0usize; w as usize];
        for (r, row) in out.cells.iter().enumerate() {
            let kind = out.kinds[r];
            for (c, cell) in row.iter().enumerate() {
                let coord = CellCoord::new(a.row + r as u32, a.col + c as u32);
                // Values keep their source's number format; counts don't.
                let number_format = out.value_columns[c]
                    .map(|v| table.values[v])
                    .filter(|v| v.aggregate != Aggregate::Count)
                    .and_then(|v| src.formats.get(v.field).cloned().flatten());
                let mut format = CellFormat::default();
                match kind {
                    RowKind::Header => {
                        format.bold = true;
                        format.fill = Some(HEADER_FILL);
                        format.borders.bottom =
                            r + 1 < out.kinds.len() && out.kinds[r + 1] != RowKind::Header;
                    }
                    RowKind::Filter => format.bold = c == 0,
                    RowKind::Subtotal => format.bold = true,
                    RowKind::GrandTotal => {
                        format.bold = true;
                        format.fill = Some(HEADER_FILL);
                        format.borders.top = true;
                    }
                    RowKind::Data => {}
                }
                let shown = match cell {
                    PivotCell::Empty => String::new(),
                    PivotCell::Text(t) => {
                        self.set_value(sheet, coord, CellValueInput::Text(t.clone()));
                        t.clone()
                    }
                    PivotCell::Number(n) => {
                        self.set_value(sheet, coord, CellValueInput::Number(*n));
                        format.number_format = number_format.clone();
                        match &number_format {
                            Some(code) => format_number(*n, code).text,
                            None => format_general(*n, 11),
                        }
                    }
                };
                widths[c] = widths[c].max(shown.chars().count());
                if !format.is_default() {
                    self.set_cell_format(sheet, coord, format);
                }
            }
        }

        // Widen columns to fit, as Excel does on refresh.
        let f = self.formatting_mut(sheet);
        for (c, chars) in widths.into_iter().enumerate() {
            let col = a.col + c as u32;
            let fit = (chars as f32 * 7.5 + 16.0).min(360.0);
            if fit > f.column_widths.get(&col).copied().unwrap_or(DEFAULT_WIDTH) {
                f.column_widths.insert(col, fit);
            }
        }
        if let Some(t) = f.pivots.get_mut(index) {
            t.output = Some(area);
        }
        Ok(())
    }

    /// Remove pivot table `index` on `sheet` and the cells it wrote.
    pub fn delete_pivot(&mut self, sheet: u32, index: usize) -> bool {
        let Some(table) = self
            .formatting(sheet)
            .and_then(|f| f.pivots.get(index))
            .cloned()
        else {
            return false;
        };
        if let Some(out) = table.output {
            self.clear_area(sheet, out);
        }
        self.formatting_mut(sheet).pivots.remove(index);
        true
    }

    /// Clear the values and formats in `area`.
    fn clear_area(&mut self, sheet: u32, area: CellRange) {
        let cells: Vec<CellCoord> = self
            .iter_sheet_inputs(sheet)
            .map(|(c, _)| c)
            .filter(|c| inside(area, *c))
            .collect();
        for c in cells {
            self.clear(sheet, c);
        }
        let formatted: Vec<CellCoord> = self
            .formatting(sheet)
            .map(|f| {
                f.cells()
                    .map(|(c, _)| c)
                    .filter(|c| inside(area, *c))
                    .collect()
            })
            .unwrap_or_default();
        for c in formatted {
            self.set_cell_format(sheet, c, CellFormat::default());
        }
    }

    /// Pivot tables whose source is on `sheet` follow its inserted and
    /// deleted rows and columns.
    pub(super) fn move_pivot_sources(&mut self, sheet: u32, edit: &LineEdit) {
        let Some(name) = self.sheet_names().get(sheet as usize).cloned() else {
            return;
        };
        for formatting in self.all_formatting_mut().values_mut() {
            for p in &mut formatting.pivots {
                if p.source_sheet.eq_ignore_ascii_case(&name) {
                    if let Some(moved) = edit.map_range(p.source) {
                        p.source = moved;
                    }
                }
            }
        }
    }

    /// Pivot tables follow a renamed source sheet.
    pub(super) fn rename_pivot_sources(&mut self, old: &str, new: &str) {
        for formatting in self.all_formatting_mut().values_mut() {
            for p in &mut formatting.pivots {
                if p.source_sheet.eq_ignore_ascii_case(old) {
                    p.source_sheet = new.to_string();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::cell::{Axis, CellRange};

    #[test]
    fn reads_source_and_follows_edits() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Data".into(), "Report".into()]);
        let at = |a1: &str| CellCoord::from_a1(a1).unwrap();
        e.set_value(0, at("A1"), CellValueInput::Text("Region".into()));
        e.set_value(0, at("B1"), CellValueInput::Text("Sales".into()));
        e.set_value(0, at("A2"), CellValueInput::Text("North".into()));
        e.set_value(0, at("B2"), CellValueInput::Number(5.0));
        e.set_value(0, at("A4"), CellValueInput::Text("South".into()));
        e.formatting_mut(0).set(
            at("B2"),
            crate::format::CellFormat {
                number_format: Some("$#,##0".into()),
                ..Default::default()
            },
        );
        let mut table = PivotTable::new(
            "PivotTable1".into(),
            "data".into(),
            CellRange::from_a1("A1:B1048576").unwrap(),
            at("A1"),
        );
        let src = e.pivot_source(&table).unwrap();
        assert_eq!(src.headers, vec!["Region", "Sales"]);
        assert_eq!(
            src.records.len(),
            2,
            "blank rows and rows past the data are skipped"
        );
        assert_eq!(src.records[0][1].number, Some(5.0));
        assert_eq!(src.records[0][1].text, "$5");
        assert_eq!(src.formats, vec![None, Some("$#,##0".into())]);

        table.source = CellRange::from_a1("A1:B4").unwrap();
        e.formatting_mut(1).pivots.push(table);
        e.apply_line_edit(0, &LineEdit::insert(Axis::Row, 0, 1));
        assert_eq!(
            e.formatting(1).unwrap().pivots[0].source,
            CellRange::from_a1("A2:B5").unwrap()
        );
        e.rewrite_sheet_name("Data", "Sales data");
        assert_eq!(
            e.formatting(1).unwrap().pivots[0].source_sheet,
            "Sales data"
        );

        // Writing it: values and formats, then a refresh replaces them.
        let mut e2 = CalcEngine::new();
        e2.set_value(0, at("A1"), CellValueInput::Text("Region".into()));
        e2.set_value(0, at("B1"), CellValueInput::Text("Sales".into()));
        e2.set_value(0, at("A2"), CellValueInput::Text("North".into()));
        e2.set_value(0, at("B2"), CellValueInput::Number(5.0));
        let mut t = PivotTable::new(
            "P".into(),
            "Sheet1".into(),
            CellRange::from_a1("A1:B2").unwrap(),
            at("D1"),
        );
        t.rows = vec![0];
        t.values = vec![crate::pivot::PivotValue {
            field: 1,
            aggregate: Aggregate::Sum,
        }];
        e2.formatting_mut(0).pivots.push(t);
        e2.refresh_pivot(0, 0).unwrap();
        assert_eq!(e2.get_value(0, at("E2")), CellResult::Value(5.0));
        assert!(e2.cell_format(0, at("D1")).is_some_and(|f| f.bold));
        e2.set_value(0, at("B2"), CellValueInput::Number(9.0));
        e2.refresh_pivot(0, 0).unwrap();
        assert_eq!(e2.get_value(0, at("E3")), CellResult::Value(9.0));
        assert!(e2.delete_pivot(0, 0));
        assert_eq!(e2.get_value(0, at("D1")), CellResult::Empty);
        assert!(e2.cell_format(0, at("D1")).is_none());

        let missing = PivotTable::new(
            "P".into(),
            "Nope".into(),
            CellRange::from_a1("A1:B4").unwrap(),
            at("A1"),
        );
        assert!(e.pivot_source(&missing).is_err());
    }
}
