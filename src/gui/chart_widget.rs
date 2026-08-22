//! Chart widget for rendering charts in floating egui::Window
//!
//! Provides drag, resize, and close functionality for chart overlays.

use eframe::egui::{self, Context, Id, Pos2, Rect, Response, Ui, Vec2, Window};

use crate::chart::{ChartDefinition, ChartId, ResolvedChartData, render::render_chart};

/// State for a chart window
#[derive(Clone)]
pub struct ChartWindowState {
    /// The chart definition
    pub chart: ChartDefinition,
    /// Resolved chart data (cached)
    pub data: Option<ResolvedChartData>,
    /// Whether the window is open
    pub open: bool,
    /// Window position (if set)
    pub position: Option<Pos2>,
    /// Window size
    pub size: Vec2,
}

impl ChartWindowState {
    pub fn new(chart: ChartDefinition) -> Self {
        let size = Vec2::new(
            chart.overlay_area.size.0 as f32,
            chart.overlay_area.size.1 as f32,
        );
        Self {
            chart,
            data: None,
            open: true,
            position: None,
            size,
        }
    }

    /// Update the resolved data
    pub fn set_data(&mut self, data: ResolvedChartData) {
        self.data = Some(data);
    }
}

/// Manager for multiple chart windows
pub struct ChartWindowManager {
    /// All chart windows
    windows: Vec<ChartWindowState>,
    /// Charts to remove after this frame
    pending_removes: Vec<ChartId>,
}

impl Default for ChartWindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartWindowManager {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            pending_removes: Vec::new(),
        }
    }

    /// Add a chart window
    pub fn add_chart(&mut self, chart: ChartDefinition) {
        self.windows.push(ChartWindowState::new(chart));
    }

    /// Remove a chart by ID
    pub fn remove_chart(&mut self, id: ChartId) {
        self.windows.retain(|w| w.chart.id != id);
    }

    /// Get a mutable reference to a chart window by ID
    pub fn get_chart_mut(&mut self, id: ChartId) -> Option<&mut ChartWindowState> {
        self.windows.iter_mut().find(|w| w.chart.id == id)
    }

    /// Get a reference to a chart window by ID
    pub fn get_chart(&self, id: ChartId) -> Option<&ChartWindowState> {
        self.windows.iter().find(|w| w.chart.id == id)
    }

    /// Get all chart IDs
    pub fn chart_ids(&self) -> Vec<ChartId> {
        self.windows.iter().map(|w| w.chart.id).collect()
    }

    pub fn all_charts(&self) -> Vec<ChartDefinition> {
        self.windows.iter().map(|w| w.chart.clone()).collect()
    }

    pub fn clear(&mut self) {
        self.windows.clear();
    }

    pub fn remove_sheet_and_shift(&mut self, index: u32) {
        self.windows.retain(|w| w.chart.sheet_index != index);
        for window in &mut self.windows {
            if window.chart.sheet_index > index {
                window.chart.sheet_index -= 1;
            }
        }
    }

    /// Get number of charts
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Render all chart windows
    pub fn show(&mut self, ctx: &Context) -> ChartWindowResponse {
        let mut response = ChartWindowResponse::default();
        self.pending_removes.clear();

        for window_state in &mut self.windows {
            if !window_state.open {
                self.pending_removes.push(window_state.chart.id);
                continue;
            }

            let chart_id = window_state.chart.id;
            let title = window_state
                .chart
                .title
                .clone()
                .unwrap_or_else(|| format!("Chart {}", chart_id.0));

            let window_id = Id::new(format!("chart_window_{}", chart_id.0));

            let mut window = Window::new(&title)
                .id(window_id)
                .resizable(true)
                .collapsible(true)
                .default_size(window_state.size);

            if let Some(pos) = window_state.position {
                window = window.default_pos(pos);
            }

            let window_response = window.show(ctx, |ui| {
                let available = ui.available_rect_before_wrap();

                // Render the chart
                if let Some(data) = &window_state.data {
                    render_chart(ui, &window_state.chart, data, available);
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Loading chart data...");
                    });
                }

                // Context menu for chart options
                ui.interact(available, window_id.with("context"), egui::Sense::click())
                    .context_menu(|ui| {
                        if ui.button("Edit Chart...").clicked() {
                            response.edit_requested = Some(chart_id);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Close").clicked() {
                            response.close_requested.push(chart_id);
                            ui.close_menu();
                        }
                    });
            });

            // Track if window was closed via title bar
            if let Some(inner) = window_response {
                // Update stored position and size
                window_state.position = Some(inner.response.rect.min);
                window_state.size = inner.response.rect.size();

                // Check if double-clicked (for edit)
                if inner.response.double_clicked() {
                    response.edit_requested = Some(chart_id);
                }
            }
        }

        // Remove closed charts
        for id in &self.pending_removes {
            self.windows.retain(|w| w.chart.id != *id);
        }

        for id in &response.close_requested {
            self.windows.retain(|w| w.chart.id != *id);
        }

        response
    }

    /// Update chart data for a specific chart
    pub fn update_chart_data(&mut self, id: ChartId, data: ResolvedChartData) {
        if let Some(window) = self.get_chart_mut(id) {
            window.set_data(data);
        }
    }

    /// Get all charts for a specific sheet
    pub fn charts_for_sheet(&self, sheet_index: u32) -> Vec<&ChartWindowState> {
        self.windows
            .iter()
            .filter(|w| w.chart.sheet_index == sheet_index)
            .collect()
    }
}

/// Response from chart window manager
#[derive(Default)]
pub struct ChartWindowResponse {
    /// Chart requested to be edited
    pub edit_requested: Option<ChartId>,
    /// Charts requested to be closed
    pub close_requested: Vec<ChartId>,
}

/// A simple chart widget for embedding in panels (non-windowed)
pub struct ChartWidget<'a> {
    chart: &'a ChartDefinition,
    data: &'a ResolvedChartData,
}

impl<'a> ChartWidget<'a> {
    pub fn new(chart: &'a ChartDefinition, data: &'a ResolvedChartData) -> Self {
        Self { chart, data }
    }

    /// Show the chart in the given UI
    pub fn show(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(
            self.chart.overlay_area.size.0 as f32,
            self.chart.overlay_area.size.1 as f32,
        );

        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        if ui.is_rect_visible(rect) {
            render_chart(ui, self.chart, self.data, rect);
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_window_manager() {
        let mut manager = ChartWindowManager::new();
        assert!(manager.is_empty());

        let chart = ChartDefinition::default();
        let id = chart.id;
        manager.add_chart(chart);

        assert_eq!(manager.len(), 1);
        assert!(manager.get_chart(id).is_some());

        manager.remove_chart(id);
        assert!(manager.is_empty());
    }
}
