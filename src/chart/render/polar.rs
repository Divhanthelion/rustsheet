//! Polar chart rendering using egui::Mesh
//!
//! Supports Pie and Doughnut charts with custom mesh-based rendering.

use eframe::egui::{self, Ui, Rect, Color32, RichText, Vec2, Pos2, Stroke};
use eframe::epaint::{Mesh, Vertex};
use std::f32::consts::{PI, TAU};

use crate::chart::{ChartDefinition, ChartKind, ChartStyle, ResolvedChartData, LegendPosition, palette_color};

use super::{ChartRenderer, to_color32};

/// White UV coordinate for solid color meshes
const WHITE_UV: egui::Pos2 = egui::Pos2::ZERO;

/// Renderer for polar charts (Pie, Doughnut)
pub struct PolarRenderer;

impl PolarRenderer {
    pub fn new() -> Self {
        Self
    }

    /// Render the chart title
    fn render_title(&self, ui: &mut Ui, chart: &ChartDefinition) -> f32 {
        if let Some(title) = &chart.title {
            let response = ui.horizontal(|ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(title).size(chart.style.title_font_size).strong());
                });
            });
            response.response.rect.height() + 8.0
        } else {
            0.0
        }
    }

    /// Calculate slice values and angles from chart data
    fn calculate_slices(&self, data: &ResolvedChartData) -> Vec<PieSlice> {
        // For pie charts, we use the first series' y_values
        let values: Vec<f64> = data
            .series
            .first()
            .map(|s| {
                s.y_values
                    .iter()
                    .map(|&v| if v.is_nan() || v < 0.0 { 0.0 } else { v })
                    .collect()
            })
            .unwrap_or_default();

        let labels: Vec<String> = data
            .series
            .first()
            .map(|s| {
                if s.x_labels.iter().all(|l| l.is_empty()) {
                    s.y_values
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("Slice {}", i + 1))
                        .collect()
                } else {
                    s.x_labels.clone()
                }
            })
            .unwrap_or_default();

        let total: f64 = values.iter().sum();
        if total <= 0.0 {
            return vec![];
        }

        let mut slices = Vec::new();
        let mut current_angle: f32 = -PI / 2.0; // Start from top

        for (i, &value) in values.iter().enumerate() {
            let fraction = value / total;
            let sweep = (fraction * TAU as f64) as f32;

            slices.push(PieSlice {
                value,
                fraction: fraction as f32,
                start_angle: current_angle,
                sweep_angle: sweep,
                color: palette_color(i),
                label: labels.get(i).cloned().unwrap_or_default(),
                index: i,
            });

            current_angle += sweep;
        }

        slices
    }

    /// Build a pie sector mesh
    fn build_sector_mesh(
        &self,
        center: Pos2,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        sweep_angle: f32,
        color: Color32,
    ) -> Mesh {
        let mut mesh = Mesh::default();

        // Number of segments based on sweep angle (more segments for larger arcs)
        let segments = ((sweep_angle.abs() / TAU) * 64.0).max(8.0) as usize;

        if inner_radius <= 0.0 {
            // Pie slice: triangle fan from center
            let center_idx = mesh.vertices.len() as u32;
            mesh.vertices.push(Vertex {
                pos: center,
                color,
                uv: WHITE_UV,
            });

            for i in 0..=segments {
                let angle = start_angle + sweep_angle * (i as f32 / segments as f32);
                let pos = center + Vec2::angled(angle) * outer_radius;
                mesh.vertices.push(Vertex {
                    pos,
                    color,
                    uv: WHITE_UV,
                });
            }

            // Create triangles
            for i in 0..segments as u32 {
                mesh.indices.extend([center_idx, center_idx + 1 + i, center_idx + 2 + i]);
            }
        } else {
            // Doughnut slice: triangle strip between inner and outer radii
            for i in 0..=segments {
                let angle = start_angle + sweep_angle * (i as f32 / segments as f32);
                let inner_pos = center + Vec2::angled(angle) * inner_radius;
                let outer_pos = center + Vec2::angled(angle) * outer_radius;

                mesh.vertices.push(Vertex {
                    pos: inner_pos,
                    color,
                    uv: WHITE_UV,
                });
                mesh.vertices.push(Vertex {
                    pos: outer_pos,
                    color,
                    uv: WHITE_UV,
                });
            }

            // Create triangles from strip
            for i in 0..segments as u32 {
                let base = i * 2;
                // First triangle
                mesh.indices.extend([base, base + 1, base + 3]);
                // Second triangle
                mesh.indices.extend([base, base + 3, base + 2]);
            }
        }

        mesh
    }

    /// Test if a point is inside a pie/doughnut slice
    fn point_in_slice(
        &self,
        point: Pos2,
        center: Pos2,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) -> bool {
        let delta = point - center;
        let distance = delta.length();

        // Check radial distance
        if distance < inner_radius || distance > outer_radius {
            return false;
        }

        // Check angle
        let mut angle = delta.y.atan2(delta.x);
        let mut start = start_angle;
        let end = start_angle + sweep_angle;

        // Normalize angles to [0, TAU)
        while angle < 0.0 {
            angle += TAU;
        }
        while start < 0.0 {
            start += TAU;
        }

        // Handle wrap-around
        if sweep_angle >= 0.0 {
            if end > TAU {
                (angle >= start) || (angle <= end - TAU)
            } else {
                angle >= start && angle <= end
            }
        } else {
            // Negative sweep (shouldn't happen normally)
            false
        }
    }

    /// Render legend for pie chart
    fn render_legend(&self, ui: &mut Ui, slices: &[PieSlice], style: &ChartStyle, _position: LegendPosition) {
        if slices.is_empty() {
            return;
        }

        ui.vertical(|ui| {
            for slice in slices {
                ui.horizontal(|ui| {
                    let color = to_color32(slice.color);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, color);

                    let percentage = slice.fraction * 100.0;
                    ui.label(
                        RichText::new(format!("{} ({:.1}%)", slice.label, percentage))
                            .size(style.legend_font_size),
                    );
                });
            }
        });
    }
}

impl Default for PolarRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a single pie/doughnut slice
struct PieSlice {
    value: f64,
    fraction: f32,
    start_angle: f32,
    sweep_angle: f32,
    color: [u8; 4],
    label: String,
    #[allow(dead_code)]
    index: usize,
}

impl ChartRenderer for PolarRenderer {
    fn render(&self, ui: &mut Ui, chart: &ChartDefinition, data: &ResolvedChartData, rect: Rect) {
        // Calculate slices
        let slices = self.calculate_slices(data);
        if slices.is_empty() {
            // No data - show placeholder
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("No data");
                });
            });
            return;
        }

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_clip_rect(rect);

            // Draw background
            let bg_color = to_color32(chart.style.background_color);
            ui.painter().rect_filled(rect, 0.0, bg_color);

            // Render title
            let title_height = self.render_title(ui, chart);

            // Calculate chart area
            let chart_area = Rect::from_min_max(
                rect.min + Vec2::new(0.0, title_height),
                rect.max,
            );

            // Calculate legend area based on position
            let (pie_area, legend_area) = match chart.legend.position {
                LegendPosition::None => (chart_area, Rect::NOTHING),
                LegendPosition::Right => {
                    let legend_width = 120.0;
                    (
                        Rect::from_min_max(
                            chart_area.min,
                            Pos2::new(chart_area.max.x - legend_width, chart_area.max.y),
                        ),
                        Rect::from_min_max(
                            Pos2::new(chart_area.max.x - legend_width, chart_area.min.y),
                            chart_area.max,
                        ),
                    )
                }
                LegendPosition::Left => {
                    let legend_width = 120.0;
                    (
                        Rect::from_min_max(
                            Pos2::new(chart_area.min.x + legend_width, chart_area.min.y),
                            chart_area.max,
                        ),
                        Rect::from_min_max(
                            chart_area.min,
                            Pos2::new(chart_area.min.x + legend_width, chart_area.max.y),
                        ),
                    )
                }
                LegendPosition::Bottom => {
                    let legend_height = 80.0;
                    (
                        Rect::from_min_max(
                            chart_area.min,
                            Pos2::new(chart_area.max.x, chart_area.max.y - legend_height),
                        ),
                        Rect::from_min_max(
                            Pos2::new(chart_area.min.x, chart_area.max.y - legend_height),
                            chart_area.max,
                        ),
                    )
                }
                LegendPosition::Top => {
                    let legend_height = 80.0;
                    (
                        Rect::from_min_max(
                            Pos2::new(chart_area.min.x, chart_area.min.y + legend_height),
                            chart_area.max,
                        ),
                        Rect::from_min_max(
                            chart_area.min,
                            Pos2::new(chart_area.max.x, chart_area.min.y + legend_height),
                        ),
                    )
                }
            };

            // Calculate pie dimensions
            let center = pie_area.center();
            let max_radius = (pie_area.width().min(pie_area.height()) / 2.0) * 0.85;
            let inner_radius = match chart.chart_kind {
                ChartKind::Doughnut => max_radius * chart.style.inner_radius_ratio,
                _ => 0.0,
            };
            let outer_radius = max_radius;

            // Render legend first (before borrowing painter)
            if chart.legend.visible && chart.legend.position != LegendPosition::None {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(legend_area), |ui| {
                    self.render_legend(ui, &slices, &chart.style, chart.legend.position);
                });
            }

            // Now render slices and border
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let mut hovered_slice: Option<usize> = None;

            for (i, slice) in slices.iter().enumerate() {
                // Check hover
                let is_hovered = hover_pos
                    .map(|p| {
                        self.point_in_slice(
                            p,
                            center,
                            inner_radius,
                            outer_radius,
                            slice.start_angle,
                            slice.sweep_angle,
                        )
                    })
                    .unwrap_or(false);

                if is_hovered {
                    hovered_slice = Some(i);
                }

                // Calculate explode offset for hovered slice
                let explode_offset = if is_hovered { 8.0 } else { 0.0 };
                let mid_angle = slice.start_angle + slice.sweep_angle / 2.0;
                let offset = Vec2::angled(mid_angle) * explode_offset;
                let slice_center = center + offset;

                let color = if is_hovered {
                    // Lighten on hover
                    let [r, g, b, a] = slice.color;
                    Color32::from_rgba_unmultiplied(
                        r.saturating_add(30),
                        g.saturating_add(30),
                        b.saturating_add(30),
                        a,
                    )
                } else {
                    to_color32(slice.color)
                };

                // Build and draw mesh
                let mesh = self.build_sector_mesh(
                    slice_center,
                    inner_radius,
                    outer_radius,
                    slice.start_angle,
                    slice.sweep_angle,
                    color,
                );

                ui.painter().add(egui::Shape::mesh(mesh));

                // Draw slice border
                ui.painter().add(egui::Shape::circle_stroke(
                    slice_center,
                    outer_radius,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
                ));
            }

            // Render tooltip for hovered slice
            if let Some(idx) = hovered_slice {
                if let Some(slice) = slices.get(idx) {
                    let percentage = slice.fraction * 100.0;
                    let tooltip_text = format!(
                        "{}\n{:.1} ({:.1}%)",
                        slice.label, slice.value, percentage
                    );

                    if let Some(pos) = hover_pos {
                        egui::show_tooltip_at(
                            ui.ctx(),
                            ui.layer_id(),
                            egui::Id::new("pie_tooltip"),
                            pos + Vec2::new(10.0, 10.0),
                            |ui| {
                                ui.label(tooltip_text);
                            },
                        );
                    }
                }
            }

            // Draw border
            let border_color = to_color32(chart.style.border_color);
            ui.painter().rect_stroke(
                rect,
                0.0,
                Stroke::new(chart.style.border_width, border_color),
                egui::StrokeKind::Outside,
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::{ChartId, ResolvedSeriesData};

    #[test]
    fn test_calculate_slices() {
        let renderer = PolarRenderer::new();

        let data = ResolvedChartData {
            id: ChartId(1),
            title: Some("Test Pie".to_string()),
            series: vec![ResolvedSeriesData {
                name: "Data".to_string(),
                x_values: vec![0.0, 1.0, 2.0],
                x_labels: vec!["A".to_string(), "B".to_string(), "C".to_string()],
                y_values: vec![30.0, 50.0, 20.0],
                color: [255, 0, 0, 255],
                index: 0,
            }],
            x_range: None,
            y_range: None,
            is_categorical: true,
            version: 0,
        };

        let slices = renderer.calculate_slices(&data);
        assert_eq!(slices.len(), 3);

        // Check fractions sum to 1
        let total_fraction: f32 = slices.iter().map(|s| s.fraction).sum();
        assert!((total_fraction - 1.0).abs() < 0.001);

        // Check first slice
        assert_eq!(slices[0].label, "A");
        assert!((slices[0].fraction - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_point_in_slice() {
        use std::f32::consts::TAU;

        let renderer = PolarRenderer::new();
        let center = Pos2::new(100.0, 100.0);

        // Test with a full circle slice (all angles pass)
        let point = Pos2::new(150.0, 100.0); // 50 pixels to the right
        assert!(renderer.point_in_slice(
            point,
            center,
            0.0,    // inner radius
            60.0,   // outer radius
            0.0,    // start angle
            TAU,    // full circle sweep
        ));

        // Point too far from center
        let far_point = Pos2::new(200.0, 100.0); // 100 pixels away, outside outer_radius=60
        assert!(!renderer.point_in_slice(
            far_point,
            center,
            0.0,
            60.0,
            0.0,
            TAU,
        ));

        // Point inside inner radius (for doughnut)
        let inner_point = Pos2::new(110.0, 100.0); // 10 pixels away, inside inner_radius=20
        assert!(!renderer.point_in_slice(
            inner_point,
            center,
            20.0,   // inner radius
            60.0,   // outer radius
            0.0,
            TAU,
        ));
    }
}
