use crate::calc::{CalcEngine, CellInput, CellValueInput};
use crate::cell::{CellCoord, CellValue};
use crate::chart::{ChartDefinition, ChartKind, ChartSeries, LegendPosition};
use crate::format::{CellFormat, HAlign, VAlign};
use crate::grid::Sheet;
use rust_xlsxwriter::{
    Chart, ChartLegendPosition, ChartType, Color, FilterCondition, Format, FormatAlign,
    FormatBorder, FormatUnderline, Workbook, Worksheet, XlsxError,
};
use std::collections::{HashMap, HashSet};
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

        let formatting = engine.formatting(sheet_index);
        let mut cache = FormatCache::default();
        // Cells carry the format they show (own, row or column), as Excel writes them.
        let format_for = |cache: &mut FormatCache, coord: CellCoord| -> Option<Format> {
            Some(cache.get(formatting?.effective(coord)?))
        };

        if let Some(formatting) = formatting {
            // Merge first: merge_range writes the top-left cell, which the
            // value loop below then overwrites with the real value.
            for m in &formatting.merges {
                let format = format_for(&mut cache, m.start).unwrap_or_default();
                worksheet.merge_range(
                    m.start.row,
                    col_num(m.start.col)?,
                    m.end.row,
                    col_num(m.end.col)?,
                    "",
                    &format,
                )?;
            }
        }

        let mut written = HashSet::new();
        for (coord, input) in engine.iter_sheet_inputs(sheet_index) {
            let format = format_for(&mut cache, coord);
            Self::write_engine_cell(worksheet, coord, input, format.as_ref())?;
            written.insert(coord);
        }

        if let Some(formatting) = formatting {
            // Formatted cells with no value still carry their style.
            for (coord, _) in formatting.cells() {
                if !written.contains(&coord) {
                    if let Some(format) = format_for(&mut cache, coord) {
                        let col = u16::try_from(coord.col)
                            .map_err(|_| XlsxWriteError::ColumnLimit(coord.col))?;
                        worksheet.write_blank(coord.row, col, &format)?;
                    }
                }
            }
            for (&col, &points) in &formatting.column_widths {
                let col = u16::try_from(col).map_err(|_| XlsxWriteError::ColumnLimit(col))?;
                worksheet.set_column_width(col, super::styles::points_to_excel_width(points))?;
            }
            for (&row, &points) in &formatting.row_heights {
                worksheet.set_row_height(row, super::styles::points_to_excel_height(points))?;
            }
            // Runs of columns with the same format become one <col> range.
            let mut runs: Vec<(u32, u32, &CellFormat)> = Vec::new();
            for (&col, f) in &formatting.column_formats {
                match runs.last_mut() {
                    Some((_, last, prev)) if *last + 1 == col && *prev == f => *last = col,
                    _ => runs.push((col, col, f)),
                }
            }
            for (first, last, f) in runs {
                let format = cache.get(f);
                worksheet.set_column_range_format(col_num(first)?, col_num(last)?, &format)?;
            }
            for (&row, f) in &formatting.row_formats {
                let format = cache.get(f);
                worksheet.set_row_format(row, &format)?;
            }
            for &col in &formatting.hidden_columns {
                worksheet.set_column_hidden(col_num(col)?)?;
            }
            for &row in &formatting.hidden_rows {
                worksheet.set_row_hidden(row)?;
            }
            for dv in &formatting.validations {
                if let Some(v) = to_xlsx_validation(dv) {
                    let first = dv.ranges[0];
                    let mut v = v;
                    if dv.ranges.len() > 1 {
                        let all: Vec<String> = dv.ranges.iter().map(|r| r.to_string()).collect();
                        v = v.set_multi_range(all.join(" "));
                    }
                    worksheet.add_data_validation(
                        first.start.row,
                        col_num(first.start.col)?,
                        first.end.row,
                        col_num(first.end.col)?,
                        &v,
                    )?;
                }
            }
            for cf in &formatting.conditional {
                add_conditional_format(worksheet, cf)?;
            }
            for (coord, note) in &formatting.notes {
                let mut n = rust_xlsxwriter::Note::new(&note.text);
                if let Some(author) = &note.author {
                    n = n.set_author(author);
                }
                worksheet.insert_note(coord.row, col_num(coord.col)?, &n)?;
            }
            let (rows, cols) = formatting.frozen;
            if rows > 0 || cols > 0 {
                worksheet.set_freeze_panes(rows, col_num(cols)?)?;
            }
            if let Some(filter) = &formatting.filter {
                let r = filter.range;
                worksheet.autofilter(
                    r.start.row,
                    col_num(r.start.col)?,
                    r.end.row,
                    col_num(r.end.col)?,
                )?;
                // Rows are already hidden to match; don't let the writer redo it.
                worksheet.filter_automatic_off();
                for (&offset, values) in &filter.allowed {
                    let mut condition = FilterCondition::new();
                    for v in values {
                        condition = if v.is_empty() {
                            condition.add_list_blanks_filter()
                        } else {
                            condition.add_list_filter(v.as_str())
                        };
                    }
                    worksheet.filter_column(col_num(r.start.col + offset)?, &condition)?;
                }
            }
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
        format: Option<&Format>,
    ) -> Result<(), XlsxWriteError> {
        let row = coord.row;
        let col = u16::try_from(coord.col).map_err(|_| XlsxWriteError::ColumnLimit(coord.col))?;
        let plain = Format::new();
        let format = format.unwrap_or(&plain);

        match input {
            CellInput::Empty => {
                worksheet.write_blank(row, col, format)?;
            }
            CellInput::Value(CellValueInput::Number(n)) => {
                worksheet.write_number_with_format(row, col, *n, format)?;
            }
            CellInput::Value(CellValueInput::Text(s)) => {
                worksheet.write_string_with_format(row, col, s, format)?;
            }
            CellInput::Value(CellValueInput::Bool(b)) => {
                worksheet.write_boolean_with_format(row, col, *b, format)?;
            }
            CellInput::Value(CellValueInput::Error(e)) => {
                worksheet.write_string_with_format(row, col, e.as_str(), format)?;
            }
            CellInput::Formula(formula) => {
                worksheet.write_formula_with_format(row, col, formula.as_str(), format)?;
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

/// One rust_xlsxwriter `Format` per distinct `CellFormat`.
#[derive(Default)]
struct FormatCache(HashMap<CellFormat, Format>);

impl FormatCache {
    fn get(&mut self, f: &CellFormat) -> Format {
        self.0
            .entry(f.clone())
            .or_insert_with(|| to_xlsx_format(f))
            .clone()
    }
}

/// A rust_xlsxwriter validation for a rule; `None` if it has no ranges.
fn to_xlsx_validation(
    dv: &crate::format::validation::DataValidation,
) -> Option<rust_xlsxwriter::DataValidation> {
    use crate::format::validation::{CompareOp, ErrorStyle, ValidationKind};
    use rust_xlsxwriter::{
        DataValidation, DataValidationErrorStyle, DataValidationRule as R, Formula,
    };
    if dv.ranges.is_empty() {
        return None;
    }
    let f = |s: &str| Formula::new(s.trim_start_matches('='));
    let a = f(&dv.formula1);
    let b = f(dv.formula2.as_deref().unwrap_or(&dv.formula1));
    let rule = match dv.operator {
        CompareOp::Between => R::Between(a, b),
        CompareOp::NotBetween => R::NotBetween(a, b),
        CompareOp::Equal => R::EqualTo(a),
        CompareOp::NotEqual => R::NotEqualTo(a),
        CompareOp::Greater => R::GreaterThan(a),
        CompareOp::Less => R::LessThan(a),
        CompareOp::GreaterOrEqual => R::GreaterThanOrEqualTo(a),
        CompareOp::LessOrEqual => R::LessThanOrEqualTo(a),
    };
    let mut v = DataValidation::new();
    v = match dv.kind {
        ValidationKind::Any => v.allow_any_value(),
        ValidationKind::Whole => v.allow_whole_number_formula(rule),
        ValidationKind::Decimal => v.allow_decimal_number_formula(rule),
        ValidationKind::Date => v.allow_date_formula(rule),
        ValidationKind::Time => v.allow_time_formula(rule),
        ValidationKind::TextLength => v.allow_text_length_formula(rule),
        ValidationKind::Custom => v.allow_custom(f(&dv.formula1)),
        ValidationKind::List => match dv.literal_items() {
            Some(items) => v.allow_list_strings(&items).ok()?,
            None => v.allow_list_formula(f(&dv.formula1)),
        },
    };
    v = v
        .ignore_blank(dv.allow_blank)
        .show_dropdown(dv.dropdown)
        .show_input_message(dv.show_input)
        .show_error_message(dv.show_error)
        .set_error_style(match dv.error_style {
            ErrorStyle::Stop => DataValidationErrorStyle::Stop,
            ErrorStyle::Warning => DataValidationErrorStyle::Warning,
            ErrorStyle::Information => DataValidationErrorStyle::Information,
        });
    // Titles and messages have Excel's length limits; skip ones that don't fit.
    if !dv.input_title.is_empty() {
        v = v.clone().set_input_title(&dv.input_title).unwrap_or(v);
    }
    if !dv.input_message.is_empty() {
        v = v.clone().set_input_message(&dv.input_message).unwrap_or(v);
    }
    if !dv.error_title.is_empty() {
        v = v.clone().set_error_title(&dv.error_title).unwrap_or(v);
    }
    if !dv.error_message.is_empty() {
        v = v.clone().set_error_message(&dv.error_message).unwrap_or(v);
    }
    Some(v)
}

/// Add one conditional formatting rule. Rules are added in priority order;
/// rust_xlsxwriter numbers them that way, grouped by range.
fn add_conditional_format(
    worksheet: &mut Worksheet,
    cf: &crate::format::conditional::ConditionalFormat,
) -> Result<(), XlsxWriteError> {
    use crate::format::conditional::{AverageRule, CfRule, CfStyle, Cfvo, CfvoKind, TextRule};
    use crate::format::validation::CompareOp;
    use rust_xlsxwriter::{
        ConditionalFormat2ColorScale, ConditionalFormat3ColorScale, ConditionalFormatAverage,
        ConditionalFormatAverageRule as Avg, ConditionalFormatBlank, ConditionalFormatCell,
        ConditionalFormatCellRule as C, ConditionalFormatDataBar, ConditionalFormatDuplicate,
        ConditionalFormatError, ConditionalFormatFormula, ConditionalFormatText,
        ConditionalFormatTextRule as T, ConditionalFormatTop, ConditionalFormatTopRule as Top,
        ConditionalFormatType as V, ConditionalFormatValue, Formula,
    };
    let Some(first) = cf.ranges.first().copied() else {
        return Ok(());
    };
    let multi = (cf.ranges.len() > 1).then(|| {
        cf.ranges
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    });
    let f = |s: &str| Formula::new(s.trim_start_matches('='));
    let dxf = |s: &CfStyle| {
        let mut format = Format::new();
        if s.bold == Some(true) {
            format = format.set_bold();
        }
        if s.italic == Some(true) {
            format = format.set_italic();
        }
        if s.underline == Some(true) {
            format = format.set_underline(FormatUnderline::Single);
        }
        if s.strikethrough == Some(true) {
            format = format.set_font_strikethrough();
        }
        if let Some(c) = s.font_color {
            format = format.set_font_color(Color::RGB(c.to_u32()));
        }
        if let Some(c) = s.fill {
            format = format.set_background_color(Color::RGB(c.to_u32()));
        }
        if let Some(code) = &s.number_format {
            format = format.set_num_format(code);
        }
        format
    };
    // A scale point other than the lowest/highest value.
    let point = |v: &Cfvo| -> Option<(V, ConditionalFormatValue)> {
        let num = v.value.trim().parse::<f64>().ok();
        Some(match v.kind {
            CfvoKind::Min | CfvoKind::Max => return None,
            CfvoKind::Percent => (V::Percent, num?.into()),
            CfvoKind::Percentile => (V::Percentile, num?.into()),
            CfvoKind::Number => match num {
                Some(n) => (V::Number, n.into()),
                None => (V::Formula, f(&v.value).into()),
            },
            CfvoKind::Formula => (V::Formula, f(&v.value).into()),
        })
    };
    macro_rules! add {
        ($rule:expr) => {{
            let mut rule = $rule.set_stop_if_true(cf.stop_if_true);
            if let Some(m) = &multi {
                rule = rule.set_multi_range(m.as_str());
            }
            worksheet.add_conditional_format(
                first.start.row,
                col_num(first.start.col)?,
                first.end.row,
                col_num(first.end.col)?,
                &rule,
            )?;
        }};
    }
    match &cf.rule {
        CfRule::CellIs {
            op,
            formula1,
            formula2,
            style,
        } => {
            let (a, b) = (f(formula1), f(formula2.as_deref().unwrap_or(formula1)));
            let rule = match op {
                CompareOp::Between => C::Between(a, b),
                CompareOp::NotBetween => C::NotBetween(a, b),
                CompareOp::Equal => C::EqualTo(a),
                CompareOp::NotEqual => C::NotEqualTo(a),
                CompareOp::Greater => C::GreaterThan(a),
                CompareOp::Less => C::LessThan(a),
                CompareOp::GreaterOrEqual => C::GreaterThanOrEqualTo(a),
                CompareOp::LessOrEqual => C::LessThanOrEqualTo(a),
            };
            add!(
                ConditionalFormatCell::new()
                    .set_rule(rule)
                    .set_format(dxf(style))
            )
        }
        CfRule::Text { rule, text, style } => {
            let rule = match rule {
                TextRule::Contains => T::Contains(text.clone()),
                TextRule::NotContains => T::DoesNotContain(text.clone()),
                TextRule::BeginsWith => T::BeginsWith(text.clone()),
                TextRule::EndsWith => T::EndsWith(text.clone()),
            };
            add!(
                ConditionalFormatText::new()
                    .set_rule(rule)
                    .set_format(dxf(style))
            )
        }
        CfRule::Top {
            bottom,
            rank,
            percent,
            style,
        } => {
            let n = (*rank).clamp(1, if *percent { 100 } else { 1000 }) as u16;
            let rule = match (bottom, percent) {
                (false, false) => Top::Top(n),
                (true, false) => Top::Bottom(n),
                (false, true) => Top::TopPercent(n),
                (true, true) => Top::BottomPercent(n),
            };
            add!(
                ConditionalFormatTop::new()
                    .set_rule(rule)
                    .set_format(dxf(style))
            )
        }
        CfRule::Average { rule, style } => {
            let rule = match rule {
                AverageRule::Above => Avg::AboveAverage,
                AverageRule::Below => Avg::BelowAverage,
                AverageRule::EqualOrAbove => Avg::EqualOrAboveAverage,
                AverageRule::EqualOrBelow => Avg::EqualOrBelowAverage,
            };
            add!(
                ConditionalFormatAverage::new()
                    .set_rule(rule)
                    .set_format(dxf(style))
            )
        }
        CfRule::Duplicate { unique, style } => {
            let mut rule = ConditionalFormatDuplicate::new().set_format(dxf(style));
            if *unique {
                rule = rule.invert();
            }
            add!(rule)
        }
        CfRule::Blanks { not, style } => {
            let mut rule = ConditionalFormatBlank::new().set_format(dxf(style));
            if *not {
                rule = rule.invert();
            }
            add!(rule)
        }
        CfRule::Errors { not, style } => {
            let mut rule = ConditionalFormatError::new().set_format(dxf(style));
            if *not {
                rule = rule.invert();
            }
            add!(rule)
        }
        CfRule::Expression { formula, style } => {
            add!(
                ConditionalFormatFormula::new()
                    .set_rule(f(formula))
                    .set_format(dxf(style))
            )
        }
        CfRule::ColorScale { stops } => match stops.as_slice() {
            [(lo, lo_color), (hi, hi_color)] => {
                let mut rule = ConditionalFormat2ColorScale::new()
                    .set_minimum_color(Color::RGB(lo_color.to_u32()))
                    .set_maximum_color(Color::RGB(hi_color.to_u32()));
                if let Some((kind, value)) = point(lo) {
                    rule = rule.set_minimum(kind, value);
                }
                if let Some((kind, value)) = point(hi) {
                    rule = rule.set_maximum(kind, value);
                }
                add!(rule)
            }
            [(lo, lo_color), (mid, mid_color), (hi, hi_color)] => {
                let mut rule = ConditionalFormat3ColorScale::new()
                    .set_minimum_color(Color::RGB(lo_color.to_u32()))
                    .set_midpoint_color(Color::RGB(mid_color.to_u32()))
                    .set_maximum_color(Color::RGB(hi_color.to_u32()));
                if let Some((kind, value)) = point(lo) {
                    rule = rule.set_minimum(kind, value);
                }
                if let Some((kind, value)) = point(mid) {
                    rule = rule.set_midpoint(kind, value);
                }
                if let Some((kind, value)) = point(hi) {
                    rule = rule.set_maximum(kind, value);
                }
                add!(rule)
            }
            _ => {}
        },
        CfRule::DataBar { min, max, color } => {
            let mut rule = ConditionalFormatDataBar::new()
                .set_fill_color(Color::RGB(color.to_u32()))
                .set_border_color(Color::RGB(color.to_u32()));
            if let Some((kind, value)) = point(min) {
                rule = rule.set_minimum(kind, value);
            }
            if let Some((kind, value)) = point(max) {
                rule = rule.set_maximum(kind, value);
            }
            add!(rule)
        }
    }
    Ok(())
}

fn col_num(col: u32) -> Result<u16, XlsxWriteError> {
    u16::try_from(col).map_err(|_| XlsxWriteError::ColumnLimit(col))
}

/// The rust_xlsxwriter format for a cell. Sizes and widths use Excel units.
fn to_xlsx_format(f: &CellFormat) -> Format {
    let mut format = Format::new();
    if f.bold {
        format = format.set_bold();
    }
    if f.italic {
        format = format.set_italic();
    }
    if f.underline {
        format = format.set_underline(FormatUnderline::Single);
    }
    if f.strikethrough {
        format = format.set_font_strikethrough();
    }
    if let Some(size) = f.font_size {
        format = format.set_font_size(size as f64);
    }
    if let Some(name) = &f.font_name {
        format = format.set_font_name(name);
    }
    if let Some(color) = f.font_color {
        format = format.set_font_color(Color::RGB(color.to_u32()));
    }
    if let Some(fill) = f.fill {
        format = format.set_background_color(Color::RGB(fill.to_u32()));
    }
    format = match f.h_align {
        HAlign::General => format,
        HAlign::Left => format.set_align(FormatAlign::Left),
        HAlign::Center => format.set_align(FormatAlign::Center),
        HAlign::Right => format.set_align(FormatAlign::Right),
    };
    format = match f.v_align {
        VAlign::Bottom => format,
        VAlign::Center => format.set_align(FormatAlign::VerticalCenter),
        VAlign::Top => format.set_align(FormatAlign::Top),
    };
    if f.wrap {
        format = format.set_text_wrap();
    }
    if f.borders.top {
        format = format.set_border_top(FormatBorder::Thin);
    }
    if f.borders.right {
        format = format.set_border_right(FormatBorder::Thin);
    }
    if f.borders.bottom {
        format = format.set_border_bottom(FormatBorder::Thin);
    }
    if f.borders.left {
        format = format.set_border_left(FormatBorder::Thin);
    }
    if let Some(code) = &f.number_format {
        format = format.set_num_format(code);
    }
    format
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
