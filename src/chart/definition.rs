//! Chart configuration and type definitions

use crate::cell::CellRange;
use serde::{Deserialize, Serialize};

/// Unique identifier for a chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartId(pub u64);

impl ChartId {
    /// Generate a new unique chart ID based on timestamp and counter
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        // Combine timestamp (upper bits) with counter (lower bits)
        Self((timestamp << 20) | (count & 0xFFFFF))
    }
}

impl Default for ChartId {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported chart types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartKind {
    // Cartesian charts
    Line,
    Bar,
    Scatter,
    Area,
    // Polar charts
    Pie,
    Doughnut,
    // Combined
    Combo,
}

impl Default for ChartKind {
    fn default() -> Self {
        Self::Line
    }
}

impl ChartKind {
    /// Returns true if this chart type uses polar coordinates
    pub fn is_polar(&self) -> bool {
        matches!(self, ChartKind::Pie | ChartKind::Doughnut)
    }

    /// Returns true if this chart type uses cartesian coordinates
    pub fn is_cartesian(&self) -> bool {
        !self.is_polar()
    }

    /// Display name for the chart type
    pub fn display_name(&self) -> &'static str {
        match self {
            ChartKind::Line => "Line Chart",
            ChartKind::Bar => "Bar Chart",
            ChartKind::Scatter => "Scatter Plot",
            ChartKind::Area => "Area Chart",
            ChartKind::Pie => "Pie Chart",
            ChartKind::Doughnut => "Doughnut Chart",
            ChartKind::Combo => "Combo Chart",
        }
    }
}

/// Legacy alias for ChartKind
pub type ChartType = ChartKind;

/// Chart position anchored to grid cells
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChartOverlayArea {
    /// Top-left anchor cell (row, col)
    pub anchor_cell: (u32, u32),
    /// Offset from anchor cell in pixels (x, y)
    pub anchor_offset: (f32, f32),
    /// Size in pixels (width, height)
    pub size: (f32, f32),
}

impl Default for ChartOverlayArea {
    fn default() -> Self {
        Self {
            anchor_cell: (0, 0),
            anchor_offset: (10.0, 10.0),
            size: (400.0, 300.0),
        }
    }
}

impl ChartOverlayArea {
    pub fn new(anchor_row: u32, anchor_col: u32, width: f32, height: f32) -> Self {
        Self {
            anchor_cell: (anchor_row, anchor_col),
            anchor_offset: (10.0, 10.0),
            size: (width, height),
        }
    }
}

/// Legend position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LegendPosition {
    #[default]
    Right,
    Left,
    Top,
    Bottom,
    None,
}

/// Legend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegendConfig {
    pub visible: bool,
    pub position: LegendPosition,
}

impl Default for LegendConfig {
    fn default() -> Self {
        Self {
            visible: true,
            position: LegendPosition::Right,
        }
    }
}

/// Axis configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AxisConfig {
    /// Axis title/label
    pub title: Option<String>,
    /// Minimum value (auto if None)
    pub min: Option<f64>,
    /// Maximum value (auto if None)
    pub max: Option<f64>,
    /// Show grid lines
    pub show_grid: bool,
    /// Whether axis is visible
    pub visible: bool,
}

impl AxisConfig {
    pub fn new() -> Self {
        Self {
            title: None,
            min: None,
            max: None,
            show_grid: true,
            visible: true,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Line style for series
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    None,
}

/// Marker style for data points
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MarkerStyle {
    #[default]
    Circle,
    Square,
    Diamond,
    Triangle,
    None,
}

/// Style configuration for a chart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartStyle {
    /// Background color (RGBA)
    pub background_color: [u8; 4],
    /// Border color (RGBA)
    pub border_color: [u8; 4],
    /// Border width
    pub border_width: f32,
    /// Title font size
    pub title_font_size: f32,
    /// Axis label font size
    pub axis_font_size: f32,
    /// Legend font size
    pub legend_font_size: f32,
    /// Line width for line charts
    pub line_width: f32,
    /// Marker size
    pub marker_size: f32,
    /// Bar gap ratio (0.0 = no gap, 1.0 = gap equals bar width)
    pub bar_gap_ratio: f32,
    /// Pie/doughnut inner radius ratio (0.0 for pie, 0.4-0.7 for doughnut)
    pub inner_radius_ratio: f32,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            background_color: [255, 255, 255, 255],
            border_color: [200, 200, 200, 255],
            border_width: 1.0,
            title_font_size: 16.0,
            axis_font_size: 12.0,
            legend_font_size: 11.0,
            line_width: 2.0,
            marker_size: 5.0,
            bar_gap_ratio: 0.2,
            inner_radius_ratio: 0.0, // Pie by default
        }
    }
}

/// A single data series in a chart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    /// Series name (shown in legend)
    pub name: Option<String>,
    /// Range containing X-axis/Label data (optional - uses row index if None)
    pub x_range: Option<CellRange>,
    /// Range containing Y-axis/Value data (required)
    pub y_range: CellRange,
    /// RGBA color for this series (uses default palette if None)
    pub color: Option<[u8; 4]>,
    /// Line style (for line/area charts)
    pub line_style: LineStyle,
    /// Marker style (for line/scatter charts)
    pub marker_style: MarkerStyle,
    /// Whether to show data labels
    pub show_data_labels: bool,
    /// Secondary Y-axis (for combo charts)
    pub use_secondary_axis: bool,
    /// Chart type override for combo charts
    pub chart_type_override: Option<ChartKind>,
}

impl ChartSeries {
    pub fn new(y_range: CellRange) -> Self {
        Self {
            name: None,
            x_range: None,
            y_range,
            color: None,
            line_style: LineStyle::Solid,
            marker_style: MarkerStyle::Circle,
            show_data_labels: false,
            use_secondary_axis: false,
            chart_type_override: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_x_range(mut self, x_range: CellRange) -> Self {
        self.x_range = Some(x_range);
        self
    }

    pub fn with_color(mut self, r: u8, g: u8, b: u8, a: u8) -> Self {
        self.color = Some([r, g, b, a]);
        self
    }

    pub fn with_line_style(mut self, style: LineStyle) -> Self {
        self.line_style = style;
        self
    }

    pub fn with_marker_style(mut self, style: MarkerStyle) -> Self {
        self.marker_style = style;
        self
    }
}

/// Complete chart definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartDefinition {
    /// Unique identifier
    pub id: ChartId,
    /// Chart title
    pub title: Option<String>,
    /// Type of chart
    pub chart_kind: ChartKind,
    /// Position and size
    pub overlay_area: ChartOverlayArea,
    /// Data series
    pub series: Vec<ChartSeries>,
    /// X-axis configuration
    pub x_axis: AxisConfig,
    /// Primary Y-axis configuration
    pub y_axis: AxisConfig,
    /// Secondary Y-axis configuration (for combo charts)
    pub y_axis_secondary: Option<AxisConfig>,
    /// Legend configuration
    pub legend: LegendConfig,
    /// Visual style
    pub style: ChartStyle,
    /// Sheet index this chart belongs to
    pub sheet_index: u32,
}

impl Default for ChartDefinition {
    fn default() -> Self {
        Self {
            id: ChartId::new(),
            title: None,
            chart_kind: ChartKind::Line,
            overlay_area: ChartOverlayArea::default(),
            series: Vec::new(),
            x_axis: AxisConfig::new(),
            y_axis: AxisConfig::new(),
            y_axis_secondary: None,
            legend: LegendConfig::default(),
            style: ChartStyle::default(),
            sheet_index: 0,
        }
    }
}

impl ChartDefinition {
    pub fn new(chart_kind: ChartKind) -> Self {
        let mut def = Self::default();
        def.chart_kind = chart_kind;

        // Adjust style defaults based on chart type
        if chart_kind == ChartKind::Doughnut {
            def.style.inner_radius_ratio = 0.5;
        }

        def
    }

    pub fn with_id(mut self, id: ChartId) -> Self {
        self.id = id;
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_series(mut self, series: ChartSeries) -> Self {
        self.series.push(series);
        self
    }

    pub fn with_x_label(mut self, label: impl Into<String>) -> Self {
        self.x_axis.title = Some(label.into());
        self
    }

    pub fn with_y_label(mut self, label: impl Into<String>) -> Self {
        self.y_axis.title = Some(label.into());
        self
    }

    pub fn with_overlay_area(mut self, area: ChartOverlayArea) -> Self {
        self.overlay_area = area;
        self
    }

    pub fn with_sheet(mut self, sheet_index: u32) -> Self {
        self.sheet_index = sheet_index;
        self
    }

    /// Get all cell ranges this chart depends on (for cache invalidation)
    pub fn dependent_ranges(&self) -> Vec<CellRange> {
        let mut ranges = Vec::new();
        for series in &self.series {
            ranges.push(series.y_range);
            if let Some(x_range) = &series.x_range {
                ranges.push(*x_range);
            }
        }
        ranges
    }
}

/// Legacy alias for ChartDefinition (backwards compatibility with ChartConfig)
pub type ChartConfig = ChartDefinition;

/// Default color palette for chart series
pub const DEFAULT_PALETTE: &[[u8; 4]] = &[
    [66, 133, 244, 255],   // Google Blue
    [234, 67, 53, 255],    // Google Red
    [251, 188, 5, 255],    // Google Yellow
    [52, 168, 83, 255],    // Google Green
    [154, 103, 234, 255],  // Purple
    [255, 109, 0, 255],    // Orange
    [0, 172, 193, 255],    // Cyan
    [233, 30, 99, 255],    // Pink
];

/// Get a color from the default palette by index
pub fn palette_color(index: usize) -> [u8; 4] {
    DEFAULT_PALETTE[index % DEFAULT_PALETTE.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::CellCoord;

    #[test]
    fn test_chart_id_unique() {
        let id1 = ChartId::new();
        let id2 = ChartId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_chart_definition_builder() {
        let chart = ChartDefinition::new(ChartKind::Bar)
            .with_title("Sales Data")
            .with_series(
                ChartSeries::new(CellRange::new(
                    CellCoord::new(1, 1),
                    CellCoord::new(10, 1),
                ))
                .with_name("Sales")
                .with_color(66, 133, 244, 255),
            )
            .with_x_label("Month")
            .with_y_label("Revenue");

        assert_eq!(chart.chart_kind, ChartKind::Bar);
        assert_eq!(chart.title, Some("Sales Data".to_string()));
        assert_eq!(chart.series.len(), 1);
        assert_eq!(chart.x_axis.title, Some("Month".to_string()));
    }

    #[test]
    fn test_chart_kind_properties() {
        assert!(ChartKind::Pie.is_polar());
        assert!(ChartKind::Doughnut.is_polar());
        assert!(ChartKind::Line.is_cartesian());
        assert!(ChartKind::Bar.is_cartesian());
    }
}
