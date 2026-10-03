//! Chart rendering infrastructure
//!
//! This module provides rendering capabilities for charts using egui.
//! - Cartesian charts (Line, Bar, Scatter, Area) use egui_plot
//! - Polar charts (Pie, Doughnut) use custom egui::Mesh rendering

#[cfg(any(feature = "gui", feature = "web"))]
mod cartesian;

#[cfg(any(feature = "gui", feature = "web"))]
mod polar;

#[cfg(any(feature = "gui", feature = "web"))]
pub use cartesian::*;

#[cfg(any(feature = "gui", feature = "web"))]
pub use polar::*;

#[cfg(any(feature = "gui", feature = "web"))]
use eframe::egui::{Color32, Rect, Ui};

use super::{ChartDefinition, ChartKind, ResolvedChartData};

/// Trait for chart renderers
#[cfg(any(feature = "gui", feature = "web"))]
pub trait ChartRenderer {
    /// Render the chart into the given UI area
    fn render(&self, ui: &mut Ui, chart: &ChartDefinition, data: &ResolvedChartData, rect: Rect);
}

/// Render a chart based on its type
#[cfg(any(feature = "gui", feature = "web"))]
pub fn render_chart(ui: &mut Ui, chart: &ChartDefinition, data: &ResolvedChartData, rect: Rect) {
    match chart.chart_kind {
        ChartKind::Line
        | ChartKind::Scatter
        | ChartKind::Area
        | ChartKind::Bar
        | ChartKind::Combo => {
            let renderer = CartesianRenderer::new();
            renderer.render(ui, chart, data, rect);
        }
        ChartKind::Pie | ChartKind::Doughnut => {
            let renderer = PolarRenderer::new();
            renderer.render(ui, chart, data, rect);
        }
    }
}

/// Convert RGBA color array to egui Color32
#[cfg(any(feature = "gui", feature = "web"))]
pub fn to_color32(rgba: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}
