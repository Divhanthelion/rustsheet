use crate::cell::{CellCoord, CellValue};
use crate::grid::Sheet;
use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum XlsxWriteError {
    #[error("Failed to write workbook: {0}")]
    Write(#[from] XlsxError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Excel file writer using rust_xlsxwriter
pub struct XlsxWriter {
    workbook: Workbook,
}

impl XlsxWriter {
    /// Create a new Excel workbook writer
    pub fn new() -> Self {
        Self {
            workbook: Workbook::new(),
        }
    }

    /// Add a sheet to the workbook
    pub fn add_sheet(&mut self, sheet: &Sheet) -> Result<(), XlsxWriteError> {
        let worksheet = self.workbook.add_worksheet();
        worksheet.set_name(sheet.name())?;

        // Write cell data
        for (coord, value) in sheet.iter() {
            Self::write_cell(worksheet, sheet, coord, value)?;
        }

        Ok(())
    }

    /// Write a single cell value
    fn write_cell(
        worksheet: &mut Worksheet,
        sheet: &Sheet,
        coord: CellCoord,
        value: &CellValue,
    ) -> Result<(), XlsxWriteError> {
        let row = coord.row;
        let col = coord.col as u16;

        match value {
            CellValue::Empty => {}
            CellValue::Number(n) => {
                worksheet.write_number(row, col, *n)?;
            }
            CellValue::Bool(b) => {
                worksheet.write_boolean(row, col, *b)?;
            }
            CellValue::Text(spur) => {
                if let Some(s) = sheet.string_pool().resolve(*spur) {
                    worksheet.write_string(row, col, s)?;
                }
            }
            CellValue::Error(e) => {
                // Write error as string
                worksheet.write_string(row, col, e.as_str())?;
            }
            CellValue::Formula { ast_id: _, cached } => {
                // For formulas, we'd need to reconstruct the formula string
                // For now, write the cached value
                match cached.as_ref() {
                    CellValue::Number(n) => {
                        worksheet.write_number(row, col, *n)?;
                    }
                    CellValue::Text(spur) => {
                        if let Some(s) = sheet.string_pool().resolve(*spur) {
                            worksheet.write_string(row, col, s)?;
                        }
                    }
                    CellValue::Bool(b) => {
                        worksheet.write_boolean(row, col, *b)?;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Save the workbook to a file
    pub fn save<P: AsRef<Path>>(mut self, path: P) -> Result<(), XlsxWriteError> {
        self.workbook.save(path.as_ref())?;
        Ok(())
    }

    /// Save to a Vec<u8> for in-memory use
    pub fn save_to_buffer(mut self) -> Result<Vec<u8>, XlsxWriteError> {
        let buffer = self.workbook.save_to_buffer()?;
        Ok(buffer)
    }
}

impl Default for XlsxWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_workbook() {
        let _writer = XlsxWriter::new();
        // Just verify it creates without error
    }
}
