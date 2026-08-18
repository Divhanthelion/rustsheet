use crate::calc::{CalcEngine, CellInput, CellValueInput};
use crate::cell::{CellCoord, CellValue};
use crate::chart::{ChartDefinition, ChartKind, ChartSeries, LegendPosition};
use crate::grid::Sheet;
use rust_xlsxwriter::{
    Chart, ChartLegendPosition, ChartType, Workbook, Worksheet, XlsxError,
};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum XlsxWriteError {
    #[error("Failed to write workbook: {0}")]
    Write(#[from] XlsxError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Column {0} exceeds Excel column limit")]
    ColumnLimit(u32),
    #[error("Zip error: {0}")]
    Zip(String),
    #[error("JSON error: {0}")]
    Json(String),
}

/// Excel file writer using rust_xlsxwriter
pub struct XlsxWriter {
    workbook: Workbook,
    /// Charts to add to sheets
    pending_charts: Vec<(u32, ChartDefinition)>,
}

impl XlsxWriter {
    /// Create a new Excel workbook writer
    pub fn new() -> Self {
        Self {
            workbook: Workbook::new(),
            pending_charts: Vec::new(),
        }
    }

    /// Write one engine sheet: stored values and formula strings, used cells only.
    pub fn add_engine_sheet(
        &mut self,
        name: &str,
        engine: &CalcEngine,
        sheet_index: u32,
    ) -> Result<(), XlsxWriteError> {
        self.add_engine_sheet_with_charts(name, engine, sheet_index, &[])
    }

    /// Write one engine sheet and embed Excel charts for that sheet.
    pub fn add_engine_sheet_with_charts(
        &mut self,
        name: &str,
        engine: &CalcEngine,
        sheet_index: u32,
        charts: &[ChartDefinition],
    ) -> Result<(), XlsxWriteError> {
        let worksheet = self.workbook.add_worksheet();
        worksheet.set_name(name)?;

        for (coord, input) in engine.iter_sheet_inputs(sheet_index) {
            Self::write_engine_cell(worksheet, coord, input)?;
        }

        for chart_def in charts.iter().filter(|c| c.sheet_index == sheet_index) {
            let chart = Self::create_chart(chart_def, name)?;
            let (row, col) = chart_def.overlay_area.anchor_cell;
            let col = u16::try_from(col).map_err(|_| XlsxWriteError::ColumnLimit(col))?;
            worksheet.insert_chart(row, col, &chart)?;
        }

        Ok(())
    }

    fn write_engine_cell(
        worksheet: &mut Worksheet,
        coord: CellCoord,
        input: &CellInput,
    ) -> Result<(), XlsxWriteError> {
        let row = coord.row;
        let col = u16::try_from(coord.col).map_err(|_| XlsxWriteError::ColumnLimit(coord.col))?;

        match input {
            CellInput::Empty => {}
            CellInput::Value(CellValueInput::Number(n)) => {
                worksheet.write_number(row, col, *n)?;
            }
            CellInput::Value(CellValueInput::Text(s)) => {
                worksheet.write_string(row, col, s)?;
            }
            CellInput::Value(CellValueInput::Bool(b)) => {
                worksheet.write_boolean(row, col, *b)?;
            }
            CellInput::Value(CellValueInput::Error(e)) => {
                worksheet.write_string(row, col, e.as_str())?;
            }
            CellInput::Formula(formula) => {
                worksheet.write_formula(row, col, formula.as_str())?;
            }
        }

        Ok(())
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

    /// Add a chart to be inserted into a worksheet
    pub fn add_chart(&mut self, sheet_index: u32, chart: ChartDefinition) {
        self.pending_charts.push((sheet_index, chart));
    }

    /// Add a sheet with charts
    pub fn add_sheet_with_charts(
        &mut self,
        sheet: &Sheet,
        sheet_index: u32,
        charts: &[ChartDefinition],
    ) -> Result<(), XlsxWriteError> {
        let worksheet = self.workbook.add_worksheet();
        worksheet.set_name(sheet.name())?;

        // Write cell data
        for (coord, value) in sheet.iter() {
            Self::write_cell(worksheet, sheet, coord, value)?;
        }

        // Add charts to this worksheet
        for chart_def in charts {
            if chart_def.sheet_index == sheet_index {
                let chart = Self::create_chart(chart_def, sheet.name())?;
                let (row, col) = chart_def.overlay_area.anchor_cell;
                worksheet.insert_chart(row, col as u16, &chart)?;
            }
        }

        Ok(())
    }

    /// Create a rust_xlsxwriter Chart from our ChartDefinition
    fn create_chart(
        chart_def: &ChartDefinition,
        sheet_name: &str,
    ) -> Result<Chart, XlsxWriteError> {
        // Map our ChartKind to rust_xlsxwriter ChartType
        let chart_type = Self::map_chart_type(chart_def.chart_kind);
        let mut chart = Chart::new(chart_type);

        // Set chart title
        if let Some(title) = &chart_def.title {
            chart.title().set_name(title);
        }

        // Add series
        for series in &chart_def.series {
            Self::add_series_to_chart(&mut chart, series, sheet_name)?;
        }

        // Set axis labels
        if let Some(label) = &chart_def.x_axis.title {
            chart.x_axis().set_name(label);
        }
        if let Some(label) = &chart_def.y_axis.title {
            chart.y_axis().set_name(label);
        }

        // Set legend
        if chart_def.legend.visible {
            let position = Self::map_legend_position(chart_def.legend.position);
            chart.legend().set_position(position);
        } else {
            chart.legend().set_hidden();
        }

        // Set size
        let (width, height) = chart_def.overlay_area.size;
        chart.set_width(width as u32);
        chart.set_height(height as u32);

        Ok(chart)
    }

    /// Map our ChartKind to rust_xlsxwriter ChartType
    fn map_chart_type(kind: ChartKind) -> ChartType {
        match kind {
            ChartKind::Line => ChartType::Line,
            ChartKind::Bar => ChartType::Column,
            ChartKind::Scatter => ChartType::Scatter,
            ChartKind::Area => ChartType::Area,
            ChartKind::Pie => ChartType::Pie,
            ChartKind::Doughnut => ChartType::Doughnut,
            ChartKind::Combo => ChartType::Line, // Combo defaults to line
        }
    }

    /// Map our LegendPosition to rust_xlsxwriter ChartLegendPosition
    fn map_legend_position(pos: LegendPosition) -> ChartLegendPosition {
        match pos {
            LegendPosition::Right => ChartLegendPosition::Right,
            LegendPosition::Left => ChartLegendPosition::Left,
            LegendPosition::Top => ChartLegendPosition::Top,
            LegendPosition::Bottom => ChartLegendPosition::Bottom,
            LegendPosition::None => ChartLegendPosition::Right, // Default to Right when hidden
        }
    }

    /// Add a series to a chart
    fn add_series_to_chart(
        chart: &mut Chart,
        series: &ChartSeries,
        sheet_name: &str,
    ) -> Result<(), XlsxWriteError> {
        let chart_series = chart.add_series();

        // Set series name
        if let Some(name) = &series.name {
            chart_series.set_name(name);
        }

        // Set values range
        let y_range = format_range_reference(sheet_name, &series.y_range);
        chart_series.set_values(&y_range);

        // Set categories/X values if present
        if let Some(x_range) = &series.x_range {
            let x_formula = format_range_reference(sheet_name, x_range);
            chart_series.set_categories(&x_formula);
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

    /// Save cells plus a rustsheet chart manifest inside the xlsx zip.
    pub fn save_with_charts<P: AsRef<Path>>(
        mut self,
        path: P,
        charts: &[ChartDefinition],
    ) -> Result<(), XlsxWriteError> {
        let bytes = self.workbook.save_to_buffer()?;
        embed_chart_manifest(bytes, charts, path.as_ref())
    }

    /// Save to a Vec<u8> for in-memory use
    pub fn save_to_buffer(mut self) -> Result<Vec<u8>, XlsxWriteError> {
        let buffer = self.workbook.save_to_buffer()?;
        Ok(buffer)
    }
}

fn embed_chart_manifest(
    xlsx: Vec<u8>,
    charts: &[ChartDefinition],
    path: &Path,
) -> Result<(), XlsxWriteError> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

    let mut archive =
        ZipArchive::new(Cursor::new(xlsx)).map_err(|e| XlsxWriteError::Zip(e.to_string()))?;
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut out);
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| XlsxWriteError::Zip(e.to_string()))?;
            let name = file.name().to_string();
            if name == "xl/rustsheet/charts.json" {
                continue;
            }
            zip.start_file(&name, SimpleFileOptions::default())
                .map_err(|e| XlsxWriteError::Zip(e.to_string()))?;
            std::io::copy(&mut file, &mut zip).map_err(XlsxWriteError::Io)?;
        }
        zip.start_file("xl/rustsheet/charts.json", SimpleFileOptions::default())
            .map_err(|e| XlsxWriteError::Zip(e.to_string()))?;
        let json = serde_json::to_vec(charts).map_err(|e| XlsxWriteError::Json(e.to_string()))?;
        zip.write_all(&json).map_err(XlsxWriteError::Io)?;
        zip.finish()
            .map_err(|e| XlsxWriteError::Zip(e.to_string()))?;
    }
    std::fs::write(path, out.into_inner())?;
    Ok(())
}

impl Default for XlsxWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a CellRange as an Excel formula reference
fn format_range_reference(sheet_name: &str, range: &crate::cell::CellRange) -> String {
    // Handle sheet names with spaces
    let quoted_sheet = if sheet_name.contains(' ') || sheet_name.contains('\'') {
        format!("'{}'", sheet_name.replace('\'', "''"))
    } else {
        sheet_name.to_string()
    };

    format!(
        "{}!${}${}:${}${}",
        quoted_sheet,
        col_to_letters(range.start.col),
        range.start.row + 1,
        col_to_letters(range.end.col),
        range.end.row + 1
    )
}

/// Convert 0-based column index to letters (A, B, ..., Z, AA, AB, ...)
fn col_to_letters(mut col: u32) -> String {
    let mut result = String::new();
    col += 1; // Convert to 1-based
    while col > 0 {
        col -= 1;
        result.insert(0, (b'A' + (col % 26) as u8) as char);
        col /= 26;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::CellRange;

    #[test]
    fn test_create_workbook() {
        let _writer = XlsxWriter::new();
        // Just verify it creates without error
    }

    #[test]
    fn test_format_range_reference() {
        let range = CellRange::from_a1("A1:A10").unwrap();
        let ref_str = format_range_reference("Sheet1", &range);
        assert_eq!(ref_str, "Sheet1!$A$1:$A$10");

        let range2 = CellRange::from_a1("B2:D5").unwrap();
        let ref_str2 = format_range_reference("Data Sheet", &range2);
        assert_eq!(ref_str2, "'Data Sheet'!$B$2:$D$5");
    }

    #[test]
    fn test_col_to_letters() {
        assert_eq!(col_to_letters(0), "A");
        assert_eq!(col_to_letters(25), "Z");
        assert_eq!(col_to_letters(26), "AA");
        assert_eq!(col_to_letters(27), "AB");
    }

    #[test]
    fn test_map_chart_type() {
        assert!(matches!(
            XlsxWriter::map_chart_type(ChartKind::Line),
            ChartType::Line
        ));
        assert!(matches!(
            XlsxWriter::map_chart_type(ChartKind::Bar),
            ChartType::Column
        ));
        assert!(matches!(
            XlsxWriter::map_chart_type(ChartKind::Pie),
            ChartType::Pie
        ));
    }
}
