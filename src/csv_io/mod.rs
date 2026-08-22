//! CSV import/export for one sheet of a CalcEngine.

use crate::calc::{CalcEngine, CellResult, CellValueInput};
use crate::cell::CellCoord;
use crate::formula::normalize_formula;
use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CsvError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn write_path(engine: &CalcEngine, sheet: u32, path: impl AsRef<Path>) -> Result<(), CsvError> {
    let file = std::fs::File::create(path)?;
    write_sheet(engine, sheet, file)
}

pub fn read_path(
    engine: &mut CalcEngine,
    sheet: u32,
    path: impl AsRef<Path>,
) -> Result<(), CsvError> {
    let file = std::fs::File::open(path)?;
    read_sheet(engine, sheet, file)
}

pub fn write_sheet<W: Write>(engine: &CalcEngine, sheet: u32, writer: W) -> Result<(), CsvError> {
    let Some(max) = engine.sheet_max_coord(sheet) else {
        return Ok(());
    };

    let mut wtr = csv::Writer::from_writer(writer);
    for row in 0..=max.row {
        let mut record = Vec::with_capacity(max.col as usize + 1);
        for col in 0..=max.col {
            record.push(cell_to_csv(engine, sheet, CellCoord::new(row, col)));
        }
        wtr.write_record(&record)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn read_sheet<R: Read>(engine: &mut CalcEngine, sheet: u32, reader: R) -> Result<(), CsvError> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(reader);

    for (row, result) in rdr.records().enumerate() {
        let record = result?;
        for (col, field) in record.iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let coord = CellCoord::new(row as u32, col as u32);
            apply_field(engine, sheet, coord, field);
        }
    }
    Ok(())
}

fn cell_to_csv(engine: &CalcEngine, sheet: u32, coord: CellCoord) -> String {
    if let Some(formula) = engine.get_formula(sheet, coord) {
        return formula;
    }
    match engine.get_value(sheet, coord) {
        CellResult::Empty => String::new(),
        CellResult::Value(n) => n.to_string(),
        CellResult::Text(s) => s,
        CellResult::Bool(b) => {
            if b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        CellResult::Error(e) => e.as_str().to_string(),
    }
}

fn apply_field(engine: &mut CalcEngine, sheet: u32, coord: CellCoord, field: &str) {
    if field.starts_with('=') {
        if engine
            .set_formula(sheet, coord, &normalize_formula(field))
            .is_err()
        {
            engine.set_value(sheet, coord, CellValueInput::Text(field.to_string()));
        }
        return;
    }
    if field.eq_ignore_ascii_case("true") {
        engine.set_value(sheet, coord, CellValueInput::Bool(true));
        return;
    }
    if field.eq_ignore_ascii_case("false") {
        engine.set_value(sheet, coord, CellValueInput::Bool(false));
        return;
    }
    if let Ok(n) = field.parse::<f64>() {
        engine.set_value(sheet, coord, CellValueInput::Number(n));
        return;
    }
    engine.set_value(sheet, coord, CellValueInput::Text(field.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_roundtrip_through_csv() {
        let mut engine = CalcEngine::new();
        let a1 = CellCoord::from_a1("A1").unwrap();
        let a2 = CellCoord::from_a1("A2").unwrap();
        let a3 = CellCoord::from_a1("A3").unwrap();
        engine.set_value(0, a1, CellValueInput::Number(1.0));
        engine.set_value(0, a2, CellValueInput::Number(2.0));
        engine.set_formula(0, a3, "=SUM(A1:A2)").unwrap();

        let mut buf = Vec::new();
        write_sheet(&engine, 0, &mut buf).unwrap();

        let mut loaded = CalcEngine::new();
        read_sheet(&mut loaded, 0, buf.as_slice()).unwrap();

        assert_eq!(loaded.get_formula(0, a3).as_deref(), Some("=SUM(A1:A2)"));
        assert_eq!(loaded.get_value(0, a3), CellResult::Value(3.0));
    }
}
