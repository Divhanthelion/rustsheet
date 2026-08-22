//! Chart creation and editing wizard
//!
//! A multi-step dialog for creating and configuring charts.

use eframe::egui::{self, Color32, Context, Id, RichText, Ui, Vec2, Window};

use crate::cell::CellRange;
use crate::chart::{
    AxisConfig, ChartDefinition, ChartId, ChartKind, ChartSeries, ChartStyle, LegendConfig,
    LegendPosition,
};

/// Wizard step
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardStep {
    /// Step 1: Select chart type
    ChartType,
    /// Step 2: Select data range
    DataRange,
    /// Step 3: Configure series
    SeriesConfig,
    /// Step 4: Set titles and labels
    TitlesLabels,
    /// Step 5: Legend and style
    StyleOptions,
}

impl WizardStep {
    fn next(self) -> Option<Self> {
        match self {
            Self::ChartType => Some(Self::DataRange),
            Self::DataRange => Some(Self::SeriesConfig),
            Self::SeriesConfig => Some(Self::TitlesLabels),
            Self::TitlesLabels => Some(Self::StyleOptions),
            Self::StyleOptions => None,
        }
    }

    fn prev(self) -> Option<Self> {
        match self {
            Self::ChartType => None,
            Self::DataRange => Some(Self::ChartType),
            Self::SeriesConfig => Some(Self::DataRange),
            Self::TitlesLabels => Some(Self::SeriesConfig),
            Self::StyleOptions => Some(Self::TitlesLabels),
        }
    }

    fn index(self) -> usize {
        match self {
            Self::ChartType => 0,
            Self::DataRange => 1,
            Self::SeriesConfig => 2,
            Self::TitlesLabels => 3,
            Self::StyleOptions => 4,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::ChartType => "Chart Type",
            Self::DataRange => "Data Range",
            Self::SeriesConfig => "Series",
            Self::TitlesLabels => "Titles",
            Self::StyleOptions => "Style",
        }
    }
}

/// Chart editor state
pub struct ChartEditor {
    /// Whether the editor is open
    pub open: bool,
    /// Current wizard step
    step: WizardStep,
    /// The chart being edited (None for new chart)
    editing_id: Option<ChartId>,
    /// Sheet index for the chart
    sheet_index: u32,

    // Step 1: Chart type
    selected_kind: ChartKind,

    // Step 2: Data range
    data_range_text: String,
    data_range_valid: bool,
    categories_range_text: String,
    categories_valid: bool,

    // Step 3: Series configuration
    series_list: Vec<SeriesConfig>,

    // Step 4: Titles
    chart_title: String,
    x_axis_title: String,
    y_axis_title: String,

    // Step 5: Style
    legend_visible: bool,
    legend_position: LegendPosition,
    show_grid: bool,

    /// Error message to display
    error_message: Option<String>,
}

/// Configuration for a single series
#[derive(Clone)]
struct SeriesConfig {
    name: String,
    range_text: String,
    color: [u8; 4],
}

impl Default for ChartEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartEditor {
    pub fn new() -> Self {
        Self {
            open: false,
            step: WizardStep::ChartType,
            editing_id: None,
            sheet_index: 0,
            selected_kind: ChartKind::Line,
            data_range_text: String::new(),
            data_range_valid: false,
            categories_range_text: String::new(),
            categories_valid: true, // Optional field
            series_list: Vec::new(),
            chart_title: String::new(),
            x_axis_title: String::new(),
            y_axis_title: String::new(),
            legend_visible: true,
            legend_position: LegendPosition::Right,
            show_grid: true,
            error_message: None,
        }
    }

    /// Open the editor for creating a new chart
    pub fn open_new(&mut self, sheet_index: u32) {
        self.reset();
        self.sheet_index = sheet_index;
        self.editing_id = None;
        self.open = true;
    }

    /// Open the editor for creating a new chart with a pre-selected data range
    pub fn open_new_with_selection(&mut self, sheet_index: u32, selection: &CellRange) {
        self.reset();
        self.sheet_index = sheet_index;
        self.editing_id = None;

        // Pre-fill the data range from selection
        self.data_range_text = selection.to_a1();
        self.data_range_valid = true;

        self.open = true;
    }

    /// Open the editor for editing an existing chart
    pub fn open_edit(&mut self, chart: &ChartDefinition) {
        self.reset();
        self.editing_id = Some(chart.id);
        self.sheet_index = chart.sheet_index;
        self.selected_kind = chart.chart_kind;

        // Populate from existing chart
        self.chart_title = chart.title.clone().unwrap_or_default();
        self.x_axis_title = chart.x_axis.title.clone().unwrap_or_default();
        self.y_axis_title = chart.y_axis.title.clone().unwrap_or_default();
        self.legend_visible = chart.legend.visible;
        self.legend_position = chart.legend.position;
        self.show_grid = chart.x_axis.show_grid;

        // Populate series
        self.series_list = chart
            .series
            .iter()
            .map(|s| SeriesConfig {
                name: s.name.clone().unwrap_or_default(),
                range_text: s.y_range.to_a1(),
                color: s.color.unwrap_or([0, 0, 255, 255]),
            })
            .collect();

        if let Some(first_series) = chart.series.first() {
            self.data_range_text = first_series.y_range.to_a1();
            self.data_range_valid = true;
            if let Some(x_range) = &first_series.x_range {
                self.categories_range_text = x_range.to_a1();
                self.categories_valid = true;
            }
        }

        self.open = true;
    }

    /// Reset the editor state
    fn reset(&mut self) {
        self.step = WizardStep::ChartType;
        self.selected_kind = ChartKind::Line;
        self.data_range_text.clear();
        self.data_range_valid = false;
        self.categories_range_text.clear();
        self.categories_valid = true;
        self.series_list.clear();
        self.chart_title.clear();
        self.x_axis_title.clear();
        self.y_axis_title.clear();
        self.legend_visible = true;
        self.legend_position = LegendPosition::Right;
        self.show_grid = true;
        self.error_message = None;
    }

    /// Build the chart definition from current state
    pub fn build_chart(&self) -> Option<ChartDefinition> {
        let y_range = CellRange::from_a1(&self.data_range_text)?;

        let x_range = if self.categories_range_text.is_empty() {
            None
        } else {
            CellRange::from_a1(&self.categories_range_text)
        };

        let mut chart = ChartDefinition::new(self.selected_kind);
        chart.sheet_index = self.sheet_index;

        // Set ID if editing
        if let Some(id) = self.editing_id {
            chart.id = id;
        }

        // Set title
        if !self.chart_title.is_empty() {
            chart.title = Some(self.chart_title.clone());
        }

        // Set axis titles
        if !self.x_axis_title.is_empty() {
            chart.x_axis.title = Some(self.x_axis_title.clone());
        }
        if !self.y_axis_title.is_empty() {
            chart.y_axis.title = Some(self.y_axis_title.clone());
        }

        // Set grid
        chart.x_axis.show_grid = self.show_grid;
        chart.y_axis.show_grid = self.show_grid;

        // Set legend
        chart.legend.visible = self.legend_visible;
        chart.legend.position = self.legend_position;

        // Add series
        if self.series_list.is_empty() {
            // Create default series from data range
            let mut series = ChartSeries::new(y_range);
            series.x_range = x_range;
            chart.series.push(series);
        } else {
            for (i, config) in self.series_list.iter().enumerate() {
                if let Some(range) = CellRange::from_a1(&config.range_text) {
                    let mut series = ChartSeries::new(range);
                    series.name = if config.name.is_empty() {
                        None
                    } else {
                        Some(config.name.clone())
                    };
                    series.color = Some(config.color);
                    if i == 0 {
                        series.x_range = x_range.clone();
                    }
                    chart.series.push(series);
                }
            }
        }

        Some(chart)
    }

    /// Show the editor dialog
    pub fn show(&mut self, ctx: &Context) -> ChartEditorResponse {
        let mut response = ChartEditorResponse::default();

        if !self.open {
            return response;
        }

        let title = if self.editing_id.is_some() {
            "Edit Chart"
        } else {
            "Create Chart"
        };

        let mut still_open = self.open;
        Window::new(title)
            .id(Id::new("chart_editor"))
            .resizable(true)
            .collapsible(false)
            .default_size(Vec2::new(500.0, 400.0))
            .open(&mut still_open)
            .show(ctx, |ui| {
                // Progress indicator
                self.show_progress(ui);
                ui.separator();

                // Step content
                egui::ScrollArea::vertical().show(ui, |ui| match self.step {
                    WizardStep::ChartType => self.show_chart_type_step(ui),
                    WizardStep::DataRange => self.show_data_range_step(ui),
                    WizardStep::SeriesConfig => self.show_series_config_step(ui),
                    WizardStep::TitlesLabels => self.show_titles_step(ui),
                    WizardStep::StyleOptions => self.show_style_step(ui),
                });

                // Error message
                if let Some(error) = &self.error_message {
                    ui.separator();
                    ui.colored_label(Color32::RED, error);
                }

                ui.separator();

                // Navigation buttons
                ui.horizontal(|ui| {
                    // Back button
                    let can_go_back = self.step.prev().is_some();
                    if ui
                        .add_enabled(can_go_back, egui::Button::new("< Back"))
                        .clicked()
                    {
                        if let Some(prev) = self.step.prev() {
                            self.step = prev;
                            self.error_message = None;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            self.open = false;
                            response.cancelled = true;
                        }

                        // Next/Finish button
                        let is_last_step = self.step.next().is_none();
                        let button_text = if is_last_step { "Create" } else { "Next >" };

                        if ui.button(button_text).clicked() {
                            if self.validate_step() {
                                if let Some(next) = self.step.next() {
                                    self.step = next;
                                    self.error_message = None;
                                } else {
                                    // Finish - build and return chart
                                    if let Some(chart) = self.build_chart() {
                                        response.chart = Some(chart);
                                        response.is_edit = self.editing_id.is_some();
                                        self.open = false;
                                    } else {
                                        self.error_message = Some(
                                            "Failed to create chart. Check your data ranges."
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    });
                });
            });

        self.open = still_open;
        response
    }

    /// Show progress indicator
    fn show_progress(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let steps = [
                WizardStep::ChartType,
                WizardStep::DataRange,
                WizardStep::SeriesConfig,
                WizardStep::TitlesLabels,
                WizardStep::StyleOptions,
            ];

            for (i, step) in steps.iter().enumerate() {
                let is_current = *step == self.step;
                let is_completed = step.index() < self.step.index();

                let text = if is_current {
                    RichText::new(step.name()).strong()
                } else if is_completed {
                    RichText::new(step.name()).color(Color32::GREEN)
                } else {
                    RichText::new(step.name()).color(Color32::GRAY)
                };

                ui.label(text);

                if i < steps.len() - 1 {
                    ui.label(" > ");
                }
            }
        });
    }

    /// Show chart type selection step
    fn show_chart_type_step(&mut self, ui: &mut Ui) {
        ui.heading("Select Chart Type");
        ui.add_space(10.0);

        let chart_types = [
            (ChartKind::Line, "Line", "Best for trends over time"),
            (ChartKind::Bar, "Bar", "Compare categories"),
            (
                ChartKind::Scatter,
                "Scatter",
                "Show relationships between variables",
            ),
            (ChartKind::Area, "Area", "Show cumulative values over time"),
            (ChartKind::Pie, "Pie", "Show parts of a whole"),
            (ChartKind::Doughnut, "Doughnut", "Pie chart with a hole"),
        ];

        egui::Grid::new("chart_type_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                for (i, (kind, name, description)) in chart_types.iter().enumerate() {
                    let is_selected = self.selected_kind == *kind;
                    let button = egui::Button::new(RichText::new(*name).size(16.0))
                        .min_size(Vec2::new(120.0, 60.0))
                        .selected(is_selected);

                    if ui.add(button).clicked() {
                        self.selected_kind = *kind;
                    }

                    ui.label(*description);

                    if i % 2 == 1 || i == chart_types.len() - 1 {
                        ui.end_row();
                    }
                }
            });
    }

    /// Show data range selection step
    fn show_data_range_step(&mut self, ui: &mut Ui) {
        ui.heading("Select Data Range");
        ui.add_space(10.0);

        ui.label("Enter the range containing your data values (Y-axis):");
        ui.add_space(5.0);

        let response = ui.text_edit_singleline(&mut self.data_range_text);
        if response.changed() {
            self.data_range_valid = CellRange::from_a1(&self.data_range_text).is_some();
        }

        if !self.data_range_text.is_empty() {
            if self.data_range_valid {
                ui.colored_label(Color32::GREEN, "Valid range");
            } else {
                ui.colored_label(Color32::RED, "Invalid range format (e.g., A1:A10)");
            }
        }

        ui.add_space(15.0);
        ui.label("Category labels range (optional, for X-axis):");
        ui.add_space(5.0);

        let cat_response = ui.text_edit_singleline(&mut self.categories_range_text);
        if cat_response.changed() {
            self.categories_valid = self.categories_range_text.is_empty()
                || CellRange::from_a1(&self.categories_range_text).is_some();
        }

        if !self.categories_range_text.is_empty() && !self.categories_valid {
            ui.colored_label(Color32::RED, "Invalid range format");
        }

        ui.add_space(10.0);
        ui.label("Examples: A1:A10, B2:D2, Sheet1!A1:A20");
    }

    /// Show series configuration step
    fn show_series_config_step(&mut self, ui: &mut Ui) {
        ui.heading("Configure Data Series");
        ui.add_space(10.0);

        // Initialize series list if empty
        if self.series_list.is_empty() {
            self.series_list.push(SeriesConfig {
                name: "Series 1".to_string(),
                range_text: self.data_range_text.clone(),
                color: [66, 133, 244, 255], // Google Blue
            });
        }

        let mut to_remove = None;
        let series_count = self.series_list.len();

        for (i, series) in self.series_list.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Series {}:", i + 1));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if series_count > 1 {
                            if ui.small_button("Remove").clicked() {
                                to_remove = Some(i);
                            }
                        }
                    });
                });

                egui::Grid::new(format!("series_grid_{}", i))
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut series.name);
                        ui.end_row();

                        ui.label("Range:");
                        ui.text_edit_singleline(&mut series.range_text);
                        ui.end_row();

                        ui.label("Color:");
                        let mut color = Color32::from_rgba_unmultiplied(
                            series.color[0],
                            series.color[1],
                            series.color[2],
                            series.color[3],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            series.color = [color.r(), color.g(), color.b(), color.a()];
                        }
                        ui.end_row();
                    });
            });
        }

        if let Some(idx) = to_remove {
            self.series_list.remove(idx);
        }

        ui.add_space(10.0);
        if ui.button("+ Add Series").clicked() {
            let idx = self.series_list.len();
            let colors = [
                [234, 67, 53, 255],  // Red
                [251, 188, 5, 255],  // Yellow
                [52, 168, 83, 255],  // Green
                [103, 58, 183, 255], // Purple
            ];
            self.series_list.push(SeriesConfig {
                name: format!("Series {}", idx + 1),
                range_text: String::new(),
                color: colors[idx % colors.len()],
            });
        }
    }

    /// Show titles and labels step
    fn show_titles_step(&mut self, ui: &mut Ui) {
        ui.heading("Titles and Labels");
        ui.add_space(10.0);

        egui::Grid::new("titles_grid")
            .num_columns(2)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                ui.label("Chart Title:");
                ui.text_edit_singleline(&mut self.chart_title);
                ui.end_row();

                ui.label("X-Axis Label:");
                ui.text_edit_singleline(&mut self.x_axis_title);
                ui.end_row();

                ui.label("Y-Axis Label:");
                ui.text_edit_singleline(&mut self.y_axis_title);
                ui.end_row();
            });
    }

    /// Show style options step
    fn show_style_step(&mut self, ui: &mut Ui) {
        ui.heading("Style Options");
        ui.add_space(10.0);

        ui.checkbox(&mut self.legend_visible, "Show Legend");

        if self.legend_visible {
            ui.horizontal(|ui| {
                ui.label("Legend Position:");
                egui::ComboBox::from_id_salt("legend_position")
                    .selected_text(match self.legend_position {
                        LegendPosition::Right => "Right",
                        LegendPosition::Left => "Left",
                        LegendPosition::Top => "Top",
                        LegendPosition::Bottom => "Bottom",
                        LegendPosition::None => "None",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.legend_position,
                            LegendPosition::Right,
                            "Right",
                        );
                        ui.selectable_value(
                            &mut self.legend_position,
                            LegendPosition::Left,
                            "Left",
                        );
                        ui.selectable_value(&mut self.legend_position, LegendPosition::Top, "Top");
                        ui.selectable_value(
                            &mut self.legend_position,
                            LegendPosition::Bottom,
                            "Bottom",
                        );
                    });
            });
        }

        ui.add_space(10.0);
        ui.checkbox(&mut self.show_grid, "Show Grid Lines");

        ui.add_space(20.0);
        ui.separator();
        ui.heading("Preview");
        ui.label("(Chart preview will appear here in future versions)");
    }

    /// Validate the current step
    fn validate_step(&mut self) -> bool {
        match self.step {
            WizardStep::ChartType => true, // Always valid
            WizardStep::DataRange => {
                if self.data_range_text.is_empty() {
                    self.error_message = Some("Please enter a data range.".to_string());
                    return false;
                }
                if !self.data_range_valid {
                    self.error_message = Some("Invalid data range format.".to_string());
                    return false;
                }
                if !self.categories_valid {
                    self.error_message = Some("Invalid categories range format.".to_string());
                    return false;
                }
                true
            }
            WizardStep::SeriesConfig => {
                // Validate all series have valid ranges
                for (i, series) in self.series_list.iter().enumerate() {
                    if series.range_text.is_empty() {
                        self.error_message = Some(format!("Series {} needs a data range.", i + 1));
                        return false;
                    }
                    if CellRange::from_a1(&series.range_text).is_none() {
                        self.error_message =
                            Some(format!("Series {} has an invalid range.", i + 1));
                        return false;
                    }
                }
                true
            }
            WizardStep::TitlesLabels => true, // All optional
            WizardStep::StyleOptions => true, // All optional
        }
    }
}

/// Response from the chart editor
#[derive(Default)]
pub struct ChartEditorResponse {
    /// The completed chart (if user clicked Create/Update)
    pub chart: Option<ChartDefinition>,
    /// Whether this was an edit (true) or new chart (false)
    pub is_edit: bool,
    /// Whether the user cancelled
    pub cancelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_steps() {
        assert_eq!(WizardStep::ChartType.next(), Some(WizardStep::DataRange));
        assert_eq!(WizardStep::StyleOptions.next(), None);
        assert_eq!(WizardStep::ChartType.prev(), None);
        assert_eq!(WizardStep::DataRange.prev(), Some(WizardStep::ChartType));
    }

    #[test]
    fn test_build_chart() {
        let mut editor = ChartEditor::new();
        editor.selected_kind = ChartKind::Bar;
        editor.data_range_text = "A1:A5".to_string();
        editor.data_range_valid = true;
        editor.chart_title = "Test Chart".to_string();

        let chart = editor.build_chart().unwrap();
        assert_eq!(chart.chart_kind, ChartKind::Bar);
        assert_eq!(chart.title, Some("Test Chart".to_string()));
        assert_eq!(chart.series.len(), 1);
    }
}
