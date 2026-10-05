//! XLSX Chart XML parsing
//!
//! This module provides basic chart parsing from XLSX files.
//! Note: Full chart import requires parsing the chart XML files within the XLSX archive.
//! This is a simplified implementation that extracts basic chart information.

use std::io::{BufReader, Read, Seek};
use std::path::Path;
use thiserror::Error;

use super::limits;
use crate::cell::{CellRange, MAX_COL, MAX_ROW};
use crate::chart::{ChartDefinition, ChartKind, ChartSeries, LegendPosition};

#[derive(Error, Debug)]
pub enum ChartReadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("Invalid chart reference: {0}")]
    InvalidReference(String),
    #[error("Zip error: {0}")]
    Zip(String),
}

/// Chart reader that extracts chart definitions from XLSX files
pub struct ChartReader;

impl ChartReader {
    /// Read all charts from an XLSX file
    ///
    /// Returns a list of (sheet_index, chart_definition) pairs.
    ///
    /// Note: This is a best-effort parser. Complex charts may not be fully supported.
    pub fn read_charts<P: AsRef<Path>>(
        path: P,
    ) -> Result<Vec<(u32, ChartDefinition)>, ChartReadError> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        Self::read_charts_from_reader(reader)
    }

    /// Read charts from a reader. A chart part that can't be read is left
    /// out; the others still load.
    pub fn read_charts_from_reader<R: Read + Seek>(
        reader: R,
    ) -> Result<Vec<(u32, ChartDefinition)>, ChartReadError> {
        limits::contain("A chart", || Self::read_all(reader)).map_err(ChartReadError::Xml)?
    }

    fn read_all<R: Read + Seek>(reader: R) -> Result<Vec<(u32, ChartDefinition)>, ChartReadError> {
        let mut archive =
            zip::ZipArchive::new(reader).map_err(|e| ChartReadError::Zip(e.to_string()))?;

        // RustSheet's own definitions, unless damaged: then Excel's charts.
        let saved = read_text(&mut archive, super::writer::CHARTS_MANIFEST)
            .and_then(|json| serde_json::from_str::<Vec<ChartDefinition>>(&json).ok());
        if let Some(charts) = saved {
            return Ok(charts
                .into_iter()
                .map(sanitize)
                .map(|c| (c.sheet_index, c))
                .collect());
        }

        let names: Vec<String> = archive
            .file_names()
            .filter(|n| n.starts_with("xl/charts/") && n.ends_with(".xml"))
            .map(|s| s.to_string())
            .collect();

        let mut charts = Vec::new();
        for name in names {
            let Some(xml) = read_text(&mut archive, &name) else {
                continue;
            };
            if let Ok(chart) = parse_chart_xml(&xml, 0) {
                charts.push((0, sanitize(chart)));
            }
        }
        Ok(charts)
    }
}

fn read_text<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    String::from_utf8(limits::read_part(archive, name)?).ok()
}

/// Points in one series at most: a full column. A series is a row or a
/// column of cells, and resolving it allocates a value per cell.
const MAX_SERIES_CELLS: u64 = MAX_ROW as u64 + 1;

/// A chart from a file, with series off the sheet dropped and unusable
/// sizes put back to the defaults. The JSON part's numbers are whatever
/// the file says.
fn sanitize(mut chart: ChartDefinition) -> ChartDefinition {
    let ok = |r: &CellRange| limits::fits(*r) && r.cell_count() <= MAX_SERIES_CELLS;
    chart
        .series
        .retain(|s| ok(&s.y_range) && s.x_range.as_ref().is_none_or(ok));
    let defaults = ChartDefinition::default();
    let fix = |v: &mut f32, default: f32, max: f32| {
        *v = limits::size(f64::from(*v), f64::from(max)).map_or(default, |v| v as f32);
    };
    let offset = |v: &mut f32, default: f32| {
        *v = if v.is_finite() {
            v.clamp(-MAX_CHART_SIZE, MAX_CHART_SIZE)
        } else {
            default
        };
    };
    let area = &mut chart.overlay_area;
    let (row, col) = area.anchor_cell;
    area.anchor_cell = (row.min(MAX_ROW), col.min(MAX_COL));
    let d = defaults.overlay_area;
    offset(&mut area.anchor_offset.0, d.anchor_offset.0);
    offset(&mut area.anchor_offset.1, d.anchor_offset.1);
    fix(&mut area.size.0, d.size.0, MAX_CHART_SIZE);
    fix(&mut area.size.1, d.size.1, MAX_CHART_SIZE);
    let (style, d) = (&mut chart.style, defaults.style);
    fix(&mut style.border_width, d.border_width, 100.0);
    fix(&mut style.title_font_size, d.title_font_size, 400.0);
    fix(&mut style.axis_font_size, d.axis_font_size, 400.0);
    fix(&mut style.legend_font_size, d.legend_font_size, 400.0);
    fix(&mut style.line_width, d.line_width, 100.0);
    fix(&mut style.marker_size, d.marker_size, 100.0);
    fix(&mut style.bar_gap_ratio, d.bar_gap_ratio, 10.0);
    fix(&mut style.inner_radius_ratio, d.inner_radius_ratio, 0.95);
    for axis in [&mut chart.x_axis, &mut chart.y_axis]
        .into_iter()
        .chain(chart.y_axis_secondary.as_mut())
    {
        axis.min = axis.min.filter(|v| v.is_finite());
        axis.max = axis.max.filter(|v| v.is_finite());
    }
    chart
}

/// Largest chart window side or offset, in pixels.
const MAX_CHART_SIZE: f32 = 10_000.0;

/// Parse chart XML content into a ChartDefinition (utility function)
pub fn parse_chart_xml(content: &str, sheet_index: u32) -> Result<ChartDefinition, ChartReadError> {
    let mut chart = ChartDefinition {
        sheet_index,
        chart_kind: detect_chart_type(content),
        ..Default::default()
    };

    // Parse title
    if let Some(title) = extract_chart_title(content) {
        chart.title = Some(title);
    }

    // Parse series
    let series = extract_series(content)?;
    chart.series = series;

    // Parse axis labels
    if let Some(label) = extract_axis_label(content, "c:catAx") {
        chart.x_axis.title = Some(label);
    }
    if let Some(label) = extract_axis_label(content, "c:valAx") {
        chart.y_axis.title = Some(label);
    }

    // Parse legend
    chart.legend.visible = content.contains("<c:legend>");
    if let Some(pos) = extract_legend_position(content) {
        chart.legend.position = pos;
    }

    Ok(chart)
}

/// Detect chart type from XML content
fn detect_chart_type(content: &str) -> ChartKind {
    if content.contains("<c:lineChart>") {
        ChartKind::Line
    } else if content.contains("<c:barChart>") || content.contains("<c:bar3DChart>") {
        ChartKind::Bar
    } else if content.contains("<c:scatterChart>") {
        ChartKind::Scatter
    } else if content.contains("<c:areaChart>") || content.contains("<c:area3DChart>") {
        ChartKind::Area
    } else if content.contains("<c:pieChart>") || content.contains("<c:pie3DChart>") {
        ChartKind::Pie
    } else if content.contains("<c:doughnutChart>") {
        ChartKind::Doughnut
    } else {
        ChartKind::Line
    }
}

/// Extract chart title from XML
fn extract_chart_title(content: &str) -> Option<String> {
    let title_start = content.find("<c:title>")?;
    let title_end = content[title_start..].find("</c:title>")? + title_start;
    let title_section = &content[title_start..title_end];

    let text_start = title_section.find("<a:t>")? + 5;
    let text_end = title_section[text_start..].find("</a:t>")? + text_start;
    let title = &title_section[text_start..text_end];

    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

/// Extract series from XML
fn extract_series(content: &str) -> Result<Vec<ChartSeries>, ChartReadError> {
    let mut series = Vec::new();
    let mut search_start = 0;

    while let Some(ser_start) = content[search_start..].find("<c:ser>") {
        let absolute_start = search_start + ser_start;
        let ser_end = match content[absolute_start..].find("</c:ser>") {
            Some(end) => absolute_start + end + 8,
            None => break,
        };
        let ser_content = &content[absolute_start..ser_end];

        let name = extract_series_name(ser_content);
        let y_range = extract_value_range(ser_content, "c:val");
        let x_range = extract_value_range(ser_content, "c:cat")
            .or_else(|| extract_value_range(ser_content, "c:xVal"));
        let color = extract_series_color(ser_content);

        if let Some(y_range) = y_range {
            let mut chart_series = ChartSeries::new(y_range);
            chart_series.name = name;
            chart_series.x_range = x_range;
            chart_series.color = color;
            series.push(chart_series);
        }

        search_start = ser_end;
    }

    Ok(series)
}

/// Extract series name from ser XML
fn extract_series_name(ser_content: &str) -> Option<String> {
    if let Some(tx_start) = ser_content.find("<c:tx>") {
        let tx_end = ser_content[tx_start..].find("</c:tx>")? + tx_start;
        let tx_section = &ser_content[tx_start..tx_end];

        if let Some(v_start) = tx_section.find("<c:v>") {
            let v_content_start = v_start + 5;
            let v_end = tx_section[v_content_start..].find("</c:v>")? + v_content_start;
            let name = &tx_section[v_content_start..v_end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract a cell range from a formula reference
fn extract_value_range(ser_content: &str, tag_name: &str) -> Option<CellRange> {
    let tag_open = format!("<{}>", tag_name);
    let tag_close = format!("</{}>", tag_name);

    let start = ser_content.find(&tag_open)?;
    let end = ser_content[start..].find(&tag_close)? + start;
    let section = &ser_content[start..end];

    let f_start = section.find("<c:f>")? + 5;
    let f_end = section[f_start..].find("</c:f>")? + f_start;
    let formula = &section[f_start..f_end];

    parse_range_reference(formula)
}

/// Parse a range reference like "Sheet1!$A$1:$A$10"
fn parse_range_reference(formula: &str) -> Option<CellRange> {
    let range_part = if let Some(bang_pos) = formula.find('!') {
        &formula[bang_pos + 1..]
    } else {
        formula
    };

    limits::range(range_part)
}

/// Extract series color from spPr/solidFill
fn extract_series_color(ser_content: &str) -> Option<[u8; 4]> {
    if let Some(start) = ser_content.find("<a:srgbClr val=\"") {
        let val_start = start + 16;
        let val_end = ser_content[val_start..].find('"')? + val_start;
        let hex_color = &ser_content[val_start..val_end];

        // Byte slicing below needs ASCII: "aé123" is six bytes too.
        if hex_color.len() == 6 && hex_color.is_ascii() {
            let r = u8::from_str_radix(&hex_color[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex_color[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex_color[4..6], 16).ok()?;
            return Some([r, g, b, 255]);
        }
    }
    None
}

/// Extract axis label
fn extract_axis_label(content: &str, axis_tag: &str) -> Option<String> {
    let tag_open = format!("<{}>", axis_tag);
    let tag_close = format!("</{}>", axis_tag);

    let start = content.find(&tag_open)?;
    let end = content[start..].find(&tag_close)? + start;
    let section = &content[start..end];

    extract_chart_title(section)
}

/// Extract legend position
fn extract_legend_position(content: &str) -> Option<LegendPosition> {
    if let Some(start) = content.find("<c:legendPos val=\"") {
        let val_start = start + 18;
        let val_end = content[val_start..].find('"')? + val_start;
        let pos = &content[val_start..val_end];

        return Some(match pos {
            "r" => LegendPosition::Right,
            "l" => LegendPosition::Left,
            "t" => LegendPosition::Top,
            "b" => LegendPosition::Bottom,
            "tr" => LegendPosition::Right,
            _ => LegendPosition::Right,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_chart_type() {
        assert_eq!(detect_chart_type("<c:lineChart>"), ChartKind::Line);
        assert_eq!(detect_chart_type("<c:barChart>"), ChartKind::Bar);
        assert_eq!(detect_chart_type("<c:pieChart>"), ChartKind::Pie);
        assert_eq!(detect_chart_type("<c:scatterChart>"), ChartKind::Scatter);
        assert_eq!(detect_chart_type("<c:doughnutChart>"), ChartKind::Doughnut);
    }

    #[test]
    fn test_parse_range_reference() {
        let range = parse_range_reference("Sheet1!$A$1:$A$10").unwrap();
        assert_eq!(range.start.row, 0);
        assert_eq!(range.start.col, 0);
        assert_eq!(range.end.row, 9);
        assert_eq!(range.end.col, 0);

        let range2 = parse_range_reference("$B$2:$D$5").unwrap();
        assert_eq!(range2.start.row, 1);
        assert_eq!(range2.start.col, 1);
        assert_eq!(range2.end.row, 4);
        assert_eq!(range2.end.col, 3);
    }

    #[test]
    fn test_extract_series_color() {
        let content = r#"<a:solidFill><a:srgbClr val="4472C4"/></a:solidFill>"#;
        let color = extract_series_color(content).unwrap();
        assert_eq!(color, [0x44, 0x72, 0xC4, 255]);
    }

    #[test]
    fn test_extract_chart_title() {
        let content = r#"<c:title><c:tx><c:rich><a:p><a:r><a:t>Sales Report</a:t></a:r></a:p></c:rich></c:tx></c:title>"#;
        let title = extract_chart_title(content).unwrap();
        assert_eq!(title, "Sales Report");
    }

    #[test]
    fn test_parse_chart_xml() {
        let xml = r#"
        <c:chart>
            <c:title><c:tx><c:rich><a:p><a:r><a:t>Test Chart</a:t></a:r></a:p></c:rich></c:tx></c:title>
            <c:plotArea>
                <c:lineChart>
                    <c:ser>
                        <c:val>
                            <c:numRef>
                                <c:f>Sheet1!$A$1:$A$10</c:f>
                            </c:numRef>
                        </c:val>
                    </c:ser>
                </c:lineChart>
            </c:plotArea>
            <c:legend>
                <c:legendPos val="r"/>
            </c:legend>
        </c:chart>
        "#;

        let chart = parse_chart_xml(xml, 0).unwrap();
        assert_eq!(chart.chart_kind, ChartKind::Line);
        assert_eq!(chart.title, Some("Test Chart".to_string()));
        assert_eq!(chart.series.len(), 1);
        assert!(chart.legend.visible);
        assert_eq!(chart.legend.position, LegendPosition::Right);
    }
}
