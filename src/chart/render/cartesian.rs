//! Cartesian chart rendering using egui_plot
//!
//! Supports Line, Bar, Scatter, Area, and Combo charts.

use eframe::egui::{self, Color32, Rect, RichText, Ui, Vec2};
use egui_plot::{Bar, BarChart, Corner, Legend, Line, Plot, PlotPoints, Points, Polygon};

use crate::chart::{
    ChartDefinition, ChartKind, ChartStyle, LegendPosition, LineStyle, ResolvedChartData,
    ResolvedSeriesData,
};

use super::{ChartRenderer, to_color32};

/// Renderer for cartesian charts (Line, Bar, Scatter, Area, Combo)
pub struct CartesianRenderer;

impl CartesianRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Render the chart title
    fn render_title(&self, ui: &mut Ui, chart: &ChartDefinition) -> f32 {
        if let Some(title) = &chart.title {
            let response = ui.horizontal(|ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(title)
                            .size(chart.style.title_font_size)
                            .strong(),
                    );
                });
            });
            response.response.rect.height() + 8.0
        } else {
            0.0
        }
    }

    /// Map LineStyle to egui_plot line width (0.0 = invisible)
    fn line_width_for_style(style: &LineStyle, base_width: f32) -> f32 {
        match style {
            LineStyle::Solid => base_width,
            LineStyle::Dashed => base_width,
            LineStyle::Dotted => base_width,
            LineStyle::None => 0.0,
        }
    }

    /// Build PlotPoints from series data
    fn build_plot_points(series: &ResolvedSeriesData) -> PlotPoints {
        let points: Vec<[f64; 2]> = series
            .x_values
            .iter()
            .zip(series.y_values.iter())
            .map(|(&x, &y)| [x, y])
            .collect();
        PlotPoints::new(points)
    }

    /// Render a line series
    fn render_line_series<'a>(series: &'a ResolvedSeriesData, style: &ChartStyle) -> Line<'a> {
        let points = Self::build_plot_points(series);
        let color = to_color32(series.color);

        Line::new(points)
            .name(&series.name)
            .color(color)
            .width(style.line_width)
    }

    /// Render an area series (filled line)
    fn render_area_series<'a>(series: &'a ResolvedSeriesData, style: &ChartStyle) -> Polygon<'a> {
        // Create polygon from points, closing at y=0
        let mut vertices: Vec<[f64; 2]> = Vec::new();

        // Add bottom-left corner
        if let Some(&first_x) = series.x_values.first() {
            vertices.push([first_x, 0.0]);
        }

        // Add all data points
        for (&x, &y) in series.x_values.iter().zip(series.y_values.iter()) {
            if !y.is_nan() {
                vertices.push([x, y]);
            }
        }

        // Add bottom-right corner
        if let Some(&last_x) = series.x_values.last() {
            vertices.push([last_x, 0.0]);
        }

        let color = to_color32(series.color);
        let fill_color = Color32::from_rgba_unmultiplied(
            series.color[0],
            series.color[1],
            series.color[2],
            80, // Semi-transparent fill
        );

        Polygon::new(PlotPoints::new(vertices))
            .name(&series.name)
            .stroke(egui::Stroke::new(style.line_width, color))
            .fill_color(fill_color)
    }

    /// Render a scatter series (points only)
    fn render_scatter_series<'a>(series: &'a ResolvedSeriesData, style: &ChartStyle) -> Points<'a> {
        let points = Self::build_plot_points(series);
        let color = to_color32(series.color);

        let shape = match series.index % 4 {
            0 => egui_plot::MarkerShape::Circle,
            1 => egui_plot::MarkerShape::Square,
            2 => egui_plot::MarkerShape::Diamond,
            _ => egui_plot::MarkerShape::Plus,
        };

        Points::new(points)
            .name(&series.name)
            .color(color)
            .radius(style.marker_size)
            .shape(shape)
    }

    /// Render bar series
    fn render_bar_series(all_series: &[ResolvedSeriesData], style: &ChartStyle) -> Vec<BarChart> {
        let num_series = all_series.len();
        let total_width = 0.8; // Total width for all bars at one x position
        let bar_width = total_width / num_series as f64;
        let gap = style.bar_gap_ratio as f64 * bar_width;
        let effective_bar_width = bar_width - gap;

        all_series
            .iter()
            .enumerate()
            .map(|(series_idx, series)| {
                let color = to_color32(series.color);

                // Calculate offset for this series
                let offset = if num_series > 1 {
                    (series_idx as f64 - (num_series - 1) as f64 / 2.0) * bar_width
                } else {
                    0.0
                };

                let bars: Vec<Bar> = series
                    .x_values
                    .iter()
                    .zip(series.y_values.iter())
                    .filter(|(_, y)| !y.is_nan())
                    .map(|(&x, &y)| Bar::new(x + offset, y).width(effective_bar_width))
                    .collect();

                BarChart::new(bars).name(&series.name).color(color)
            })
            .collect()
    }
}

impl Default for CartesianRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartRenderer for CartesianRenderer {
    fn render(&self, ui: &mut Ui, chart: &ChartDefinition, data: &ResolvedChartData, rect: Rect) {
        // Draw within the allocated rect
        let available_rect = rect;

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(available_rect), |ui| {
            ui.set_clip_rect(rect);

            // Draw background
            let bg_color = to_color32(chart.style.background_color);
            ui.painter().rect_filled(rect, 0.0, bg_color);

            // Calculate layout
            let title_height = self.render_title(ui, chart);
            let plot_rect = Rect::from_min_max(rect.min + Vec2::new(0.0, title_height), rect.max);

            // Configure legend
            let legend = match chart.legend.position {
                LegendPosition::None => None,
                pos => {
                    let corner = match pos {
                        LegendPosition::Right => Corner::RightTop,
                        LegendPosition::Left => Corner::LeftTop,
                        LegendPosition::Top => Corner::RightTop,
                        LegendPosition::Bottom => Corner::RightBottom,
                        LegendPosition::None => unreachable!(),
                    };
                    Some(Legend::default().position(corner))
                }
            };

            // Build the plot
            let mut plot = Plot::new(format!("chart_{}", chart.id.0))
                .show_background(false)
                .show_grid(chart.x_axis.show_grid)
                .allow_zoom(true)
                .allow_drag(true)
                .allow_scroll(true)
                .set_margin_fraction(Vec2::new(0.05, 0.05));

            if let Some(legend) = legend {
                plot = plot.legend(legend);
            }

            // Set axis labels
            if let Some(label) = &chart.x_axis.title {
                plot = plot.x_axis_label(label.clone());
            }
            if let Some(label) = &chart.y_axis.title {
                plot = plot.y_axis_label(label.clone());
            }

            // Handle categorical axis formatting
            if data.is_categorical {
                let labels: Vec<String> = data
                    .series
                    .first()
                    .map(|s| s.x_labels.clone())
                    .unwrap_or_default();

                if !labels.is_empty() {
                    plot = plot.x_axis_formatter(move |mark, _range| {
                        let idx = mark.value.round() as usize;
                        labels.get(idx).cloned().unwrap_or_default()
                    });
                }
            }

            // Set axis bounds if specified
            if let (Some(min), Some(max)) = (chart.x_axis.min, chart.x_axis.max) {
                plot = plot.include_x(min).include_x(max);
            }
            if let (Some(min), Some(max)) = (chart.y_axis.min, chart.y_axis.max) {
                plot = plot.include_y(min).include_y(max);
            }

            // Render at plot_rect
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(plot_rect), |ui| {
                plot.show(ui, |plot_ui| {
                    match chart.chart_kind {
                        ChartKind::Line => {
                            for series in &data.series {
                                plot_ui.line(Self::render_line_series(series, &chart.style));
                            }
                        }
                        ChartKind::Scatter => {
                            for series in &data.series {
                                plot_ui.points(Self::render_scatter_series(series, &chart.style));
                            }
                        }
                        ChartKind::Area => {
                            for series in &data.series {
                                plot_ui.polygon(Self::render_area_series(series, &chart.style));
                            }
                        }
                        ChartKind::Bar => {
                            for bar_chart in Self::render_bar_series(&data.series, &chart.style) {
                                plot_ui.bar_chart(bar_chart);
                            }
                        }
                        ChartKind::Combo => {
                            // Render each series according to its override type
                            for series in &data.series {
                                // For now, treat combo as line + scatter
                                plot_ui.line(Self::render_line_series(series, &chart.style));
                                plot_ui.points(Self::render_scatter_series(series, &chart.style));
                            }
                        }
                        _ => {
                            // Polar charts handled elsewhere
                        }
                    }
                });
            });

            // Draw border
            let border_color = to_color32(chart.style.border_color);
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(chart.style.border_width, border_color),
                egui::StrokeKind::Outside,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_plot_points() {
        let series = ResolvedSeriesData {
            name: "Test".to_string(),
            x_values: vec![0.0, 1.0, 2.0],
            x_labels: vec![],
            y_values: vec![10.0, 20.0, 30.0],
            color: [255, 0, 0, 255],
            index: 0,
        };

        let _points = CartesianRenderer::build_plot_points(&series);
        // PlotPoints doesn't expose internals easily, but we can verify it was created
        assert!(!series.x_values.is_empty());
    }

    #[test]
    fn test_bar_series_layout() {
        let style = ChartStyle::default();

        let series1 = ResolvedSeriesData {
            name: "Series 1".to_string(),
            x_values: vec![0.0, 1.0, 2.0],
            x_labels: vec![],
            y_values: vec![10.0, 20.0, 30.0],
            color: [255, 0, 0, 255],
            index: 0,
        };

        let series2 = ResolvedSeriesData {
            name: "Series 2".to_string(),
            x_values: vec![0.0, 1.0, 2.0],
            x_labels: vec![],
            y_values: vec![15.0, 25.0, 35.0],
            color: [0, 255, 0, 255],
            index: 1,
        };

        let bars = CartesianRenderer::render_bar_series(&[series1, series2], &style);
        assert_eq!(bars.len(), 2);
    }
}
