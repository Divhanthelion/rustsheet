//! Reading a pivot table's source data from its sheet.

use super::engine::{CalcEngine, CellResult};
use crate::cell::{CellCoord, LineEdit};
use crate::format::display_text;
use crate::pivot::{Item, PivotTable};

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

        let missing = PivotTable::new(
            "P".into(),
            "Nope".into(),
            CellRange::from_a1("A1:B4").unwrap(),
            at("A1"),
        );
        assert!(e.pivot_source(&missing).is_err());
    }
}
