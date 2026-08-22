use crate::calc::{CalcEngine, CellValueInput};
use crate::cell::{CellCoord, CellValue};
use crate::formula::normalize_formula;
use crate::grid::Sheet;
use calamine::{Data, Range, Reader, Xlsx, XlsxError, open_workbook};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum XlsxReadError {
    #[error("Failed to open workbook: {0}")]
    Open(#[from] XlsxError),
    #[error("Sheet not found: {0}")]
    SheetNotFound(String),
    #[error("Failed to read sheet: {0}")]
    SheetRead(String),
}

/// Excel file reader using calamine
pub struct XlsxReader {
    workbook: Xlsx<std::io::BufReader<std::fs::File>>,
}

impl XlsxReader {
    /// Open an Excel file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, XlsxReadError> {
        let workbook: Xlsx<_> = open_workbook(path)?;
        Ok(Self { workbook })
    }

    /// Get list of sheet names
    pub fn sheet_names(&self) -> Vec<String> {
        self.workbook.sheet_names().to_vec()
    }

    /// Read a sheet into our Sheet structure
    pub fn read_sheet(&mut self, name: &str) -> Result<Sheet, XlsxReadError> {
        let range = self
            .workbook
            .worksheet_range(name)
            .map_err(|e| XlsxReadError::SheetRead(e.to_string()))?;

        let mut sheet = Sheet::new(name);
        self.populate_sheet(&mut sheet, &range);
        Ok(sheet)
    }

    /// Read values and formulas into the calculation engine.
    pub fn read_into_engine(
        &mut self,
        name: &str,
        engine: &mut CalcEngine,
        sheet_index: u32,
    ) -> Result<(), XlsxReadError> {
        let range = self
            .workbook
            .worksheet_range(name)
            .map_err(|e| XlsxReadError::SheetRead(e.to_string()))?;

        let start = range.start().unwrap_or((0, 0));
        for (rel_row, rel_col, cell) in range.used_cells() {
            let coord = CellCoord::new(start.0 + rel_row as u32, start.1 + rel_col as u32);
            apply_data_to_engine(engine, sheet_index, coord, cell);
        }

        if let Ok(formulas) = self.workbook.worksheet_formula(name) {
            let fstart = formulas.start().unwrap_or((0, 0));
            for (rel_row, rel_col, formula) in formulas.used_cells() {
                if formula.is_empty() {
                    continue;
                }
                let coord = CellCoord::new(fstart.0 + rel_row as u32, fstart.1 + rel_col as u32);
                let _ = engine.set_formula(sheet_index, coord, &normalize_formula(formula));
            }
        }

        Ok(())
    }

    /// Read a sheet by index
    pub fn read_sheet_by_index(&mut self, index: usize) -> Result<Sheet, XlsxReadError> {
        let names = self.sheet_names();
        let name = names
            .get(index)
            .ok_or_else(|| XlsxReadError::SheetNotFound(format!("index {}", index)))?
            .clone();
        self.read_sheet(&name)
    }

    /// Populate a Sheet from a calamine Range
    fn populate_sheet(&self, sheet: &mut Sheet, range: &Range<Data>) {
        let (rows, cols) = range.get_size();
        let start = range.start().unwrap_or((0, 0));

        for row in 0..rows {
            for col in 0..cols {
                let excel_row = start.0 + row as u32;
                let excel_col = start.1 + col as u32;

                if let Some(cell) = range.get((row, col)) {
                    let coord = CellCoord::new(excel_row, excel_col);
                    match cell {
                        Data::Empty => {}
                        Data::Int(i) => {
                            sheet.set_number(coord, *i as f64);
                        }
                        Data::Float(f) => {
                            sheet.set_number(coord, *f);
                        }
                        Data::String(s) => {
                            sheet.set_text(coord, s);
                        }
                        Data::Bool(b) => {
                            sheet.set_bool(coord, *b);
                        }
                        Data::DateTime(dt) => {
                            // Convert ExcelDateTime to serial number
                            sheet.set_number(coord, dt.as_f64());
                        }
                        Data::DateTimeIso(s) => {
                            sheet.set_text(coord, s);
                        }
                        Data::DurationIso(s) => {
                            sheet.set_text(coord, s);
                        }
                        Data::Error(e) => {
                            // Convert calamine error to our error type
                            let cell_error = match e {
                                calamine::CellErrorType::Div0 => crate::cell::CellError::DivZero,
                                calamine::CellErrorType::NA => crate::cell::CellError::NA,
                                calamine::CellErrorType::Name => crate::cell::CellError::Name,
                                calamine::CellErrorType::Null => crate::cell::CellError::Null,
                                calamine::CellErrorType::Num => crate::cell::CellError::Num,
                                calamine::CellErrorType::Ref => crate::cell::CellError::Ref,
                                calamine::CellErrorType::Value => crate::cell::CellError::Value,
                                calamine::CellErrorType::GettingData => {
                                    crate::cell::CellError::GettingData
                                }
                            };
                            sheet.set(coord, CellValue::Error(cell_error));
                        }
                    }
                }
            }
        }
    }
}

fn apply_data_to_engine(engine: &mut CalcEngine, sheet: u32, coord: CellCoord, cell: &Data) {
    match cell {
        Data::Empty => {}
        Data::Int(i) => engine.set_value(sheet, coord, CellValueInput::Number(*i as f64)),
        Data::Float(f) => engine.set_value(sheet, coord, CellValueInput::Number(*f)),
        Data::String(s) => engine.set_value(sheet, coord, CellValueInput::Text(s.clone())),
        Data::Bool(b) => engine.set_value(sheet, coord, CellValueInput::Bool(*b)),
        Data::DateTime(dt) => engine.set_value(sheet, coord, CellValueInput::Number(dt.as_f64())),
        Data::DateTimeIso(s) => engine.set_value(sheet, coord, CellValueInput::Text(s.clone())),
        Data::DurationIso(s) => engine.set_value(sheet, coord, CellValueInput::Text(s.clone())),
        Data::Error(e) => {
            let cell_error = match e {
                calamine::CellErrorType::Div0 => crate::cell::CellError::DivZero,
                calamine::CellErrorType::NA => crate::cell::CellError::NA,
                calamine::CellErrorType::Name => crate::cell::CellError::Name,
                calamine::CellErrorType::Null => crate::cell::CellError::Null,
                calamine::CellErrorType::Num => crate::cell::CellError::Num,
                calamine::CellErrorType::Ref => crate::cell::CellError::Ref,
                calamine::CellErrorType::Value => crate::cell::CellError::Value,
                calamine::CellErrorType::GettingData => crate::cell::CellError::GettingData,
            };
            engine.set_value(sheet, coord, CellValueInput::Error(cell_error));
        }
    }
}
