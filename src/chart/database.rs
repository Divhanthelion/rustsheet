//! Chart data caching and dependency tracking using Salsa incremental computation
//!
//! This module provides automatic invalidation of chart data when cells change,
//! using Salsa for fine-grained dependency tracking.

use crate::calc::{CalcEngine, CellResult};
use crate::cell::{CellCoord, CellRange};
use hashbrown::HashMap;
use std::sync::Arc;

use super::{ChartDefinition, ChartId, ChartSeries};

/// Represents the data resolved from a cell for chart purposes
#[derive(Debug, Clone, PartialEq)]
pub enum ChartCellData {
    Empty,
    Number(f64),
    Text(String),
    Error,
}

impl ChartCellData {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            ChartCellData::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ChartCellData::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// Resolved data for a single chart series
#[derive(Debug, Clone)]
pub struct ResolvedSeriesData {
    /// Series name (from definition or auto-generated)
    pub name: String,
    /// X values (numeric indices if no x_range specified)
    pub x_values: Vec<f64>,
    /// X labels (for categorical axis)
    pub x_labels: Vec<String>,
    /// Y values (NaN for missing/error values)
    pub y_values: Vec<f64>,
    /// Color for this series (RGBA)
    pub color: [u8; 4],
    /// Series index (for palette selection)
    pub index: usize,
}

impl ResolvedSeriesData {
    /// Get the data as (x, y) pairs, filtering out NaN y values for line gaps
    pub fn points(&self) -> Vec<(f64, f64)> {
        self.x_values
            .iter()
            .zip(self.y_values.iter())
            .map(|(&x, &y)| (x, y))
            .collect()
    }

    /// Get valid points only (no NaN)
    pub fn valid_points(&self) -> Vec<(f64, f64)> {
        self.x_values
            .iter()
            .zip(self.y_values.iter())
            .filter(|&(_, y)| !y.is_nan())
            .map(|(&x, &y)| (x, y))
            .collect()
    }
}

/// Resolved data for an entire chart, ready for rendering
#[derive(Debug, Clone)]
pub struct ResolvedChartData {
    /// Chart ID
    pub id: ChartId,
    /// Chart title
    pub title: Option<String>,
    /// All series data
    pub series: Vec<ResolvedSeriesData>,
    /// X-axis range (min, max) or None for auto
    pub x_range: Option<(f64, f64)>,
    /// Y-axis range (min, max) or None for auto
    pub y_range: Option<(f64, f64)>,
    /// Whether X-axis is categorical (uses labels instead of numbers)
    pub is_categorical: bool,
    /// Cache version when this was computed
    pub version: u64,
}

impl ResolvedChartData {
    /// Calculate automatic Y-axis range from data
    pub fn auto_y_range(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;

        for series in &self.series {
            for &y in &series.y_values {
                if !y.is_nan() {
                    min = min.min(y);
                    max = max.max(y);
                }
            }
        }

        if min.is_infinite() || max.is_infinite() {
            (0.0, 1.0) // Default range
        } else if (max - min).abs() < 1e-10 {
            (min - 1.0, max + 1.0) // Avoid zero range
        } else {
            // Add 10% padding
            let padding = (max - min) * 0.1;
            (min - padding, max + padding)
        }
    }

    /// Calculate automatic X-axis range from data
    pub fn auto_x_range(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;

        for series in &self.series {
            for &x in &series.x_values {
                min = min.min(x);
                max = max.max(x);
            }
        }

        if min.is_infinite() || max.is_infinite() {
            (0.0, 1.0)
        } else if (max - min).abs() < 1e-10 {
            (min - 1.0, max + 1.0)
        } else {
            (min, max)
        }
    }

    /// Get total number of data points across all series
    pub fn total_points(&self) -> usize {
        self.series.iter().map(|s| s.y_values.len()).sum()
    }
}

/// Chart data resolver that fetches and caches chart data from the CalcEngine
#[derive(Debug)]
pub struct ChartDataResolver {
    /// Cache of resolved chart data keyed by chart ID
    cache: HashMap<ChartId, ResolvedChartData>,
    /// Version counter for cache invalidation
    version: u64,
    /// Cell dependencies: maps (sheet, row, col) to set of chart IDs that depend on it
    dependencies: HashMap<(u32, u32, u32), Vec<ChartId>>,
    /// Downsampling configuration for performance
    downsample_config: super::DownsampleConfig,
}

impl Default for ChartDataResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ChartDataResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            version: 0,
            dependencies: HashMap::new(),
            downsample_config: super::DownsampleConfig::default(),
        }
    }

    /// Create with custom downsampling configuration
    pub fn with_downsample_config(config: super::DownsampleConfig) -> Self {
        Self {
            cache: HashMap::new(),
            version: 0,
            dependencies: HashMap::new(),
            downsample_config: config,
        }
    }

    /// Set the downsampling configuration
    pub fn set_downsample_config(&mut self, config: super::DownsampleConfig) {
        self.downsample_config = config;
        // Invalidate cache since downsampling changed
        self.invalidate_all();
    }

    /// Increment version and clear all cached data
    pub fn invalidate_all(&mut self) {
        self.version += 1;
        self.cache.clear();
        self.dependencies.clear();
    }

    /// Invalidate charts that depend on a specific cell
    pub fn invalidate_cell(&mut self, sheet: u32, row: u32, col: u32) {
        if let Some(chart_ids) = self.dependencies.get(&(sheet, row, col)) {
            for id in chart_ids.clone() {
                self.cache.remove(&id);
            }
        }
        self.version += 1;
    }

    /// Invalidate charts that depend on any cell in a range
    pub fn invalidate_range(&mut self, sheet: u32, range: &CellRange) {
        for coord in range.iter() {
            if let Some(chart_ids) = self.dependencies.get(&(sheet, coord.row, coord.col)) {
                for id in chart_ids.clone() {
                    self.cache.remove(&id);
                }
            }
        }
        self.version += 1;
    }

    /// Get resolved chart data, using cache if available
    pub fn get_chart_data(
        &mut self,
        chart: &ChartDefinition,
        engine: &CalcEngine,
    ) -> ResolvedChartData {
        // Check cache
        if let Some(cached) = self.cache.get(&chart.id) {
            if cached.version == self.version {
                return cached.clone();
            }
        }

        // Resolve the chart data
        let data = self.resolve_chart(chart, engine);

        // Cache it
        self.cache.insert(chart.id, data.clone());

        data
    }

    /// Resolve chart data from the CalcEngine
    fn resolve_chart(&mut self, chart: &ChartDefinition, engine: &CalcEngine) -> ResolvedChartData {
        let sheet = chart.sheet_index;
        let mut series_data = Vec::new();

        for (idx, series) in chart.series.iter().enumerate() {
            let resolved = self.resolve_series(series, idx, sheet, engine);
            series_data.push(resolved);
        }

        // Determine if categorical based on whether first series has x_labels
        let is_categorical = series_data
            .first()
            .map(|s| !s.x_labels.is_empty() && s.x_labels.iter().any(|l| !l.is_empty()))
            .unwrap_or(false);

        ResolvedChartData {
            id: chart.id,
            title: chart.title.clone(),
            series: series_data,
            x_range: None, // Auto-calculated
            y_range: None, // Auto-calculated
            is_categorical,
            version: self.version,
        }
    }

    /// Resolve a single series
    fn resolve_series(
        &mut self,
        series: &ChartSeries,
        index: usize,
        sheet: u32,
        engine: &CalcEngine,
    ) -> ResolvedSeriesData {
        // Resolve Y values
        let y_values = self.resolve_range_as_numbers(&series.y_range, sheet, engine);

        // Resolve X values/labels
        let (x_values, x_labels) = if let Some(x_range) = &series.x_range {
            let x_data = self.resolve_range(x_range, sheet, engine);

            // Try to interpret as numbers first, fall back to labels
            let nums: Vec<_> = x_data.iter().map(|d| d.as_number()).collect();
            if nums.iter().all(|n| n.is_some()) {
                // Numeric X values
                let x_vals: Vec<f64> = nums.into_iter().flatten().collect();
                (x_vals, vec![String::new(); x_data.len()])
            } else {
                // Categorical X labels
                let labels: Vec<String> = x_data
                    .iter()
                    .map(|d| match d {
                        ChartCellData::Text(s) => s.clone(),
                        ChartCellData::Number(n) => format!("{}", n),
                        _ => String::new(),
                    })
                    .collect();
                let indices: Vec<f64> = (0..labels.len()).map(|i| i as f64).collect();
                (indices, labels)
            }
        } else {
            // No X range: use indices
            let indices: Vec<f64> = (0..y_values.len()).map(|i| i as f64).collect();
            (indices, vec![String::new(); y_values.len()])
        };

        // Get color from series or palette
        let color = series.color.unwrap_or_else(|| super::palette_color(index));

        // Get series name
        let name = series
            .name
            .clone()
            .unwrap_or_else(|| format!("Series {}", index + 1));

        // Apply downsampling if enabled and needed
        let (x_values, y_values, x_labels) = if self.downsample_config.enabled
            && y_values.len() > self.downsample_config.threshold
        {
            // For categorical data, we can't easily downsample (labels would be lost)
            // so only downsample if x_labels are all empty
            let is_categorical = x_labels.iter().any(|l| !l.is_empty());
            if is_categorical {
                (x_values, y_values, x_labels)
            } else {
                let (ds_x, ds_y) = super::downsample_series(
                    &x_values,
                    &y_values,
                    self.downsample_config.threshold,
                );
                // Labels are empty so just resize
                let ds_labels = vec![String::new(); ds_x.len()];
                (ds_x, ds_y, ds_labels)
            }
        } else {
            (x_values, y_values, x_labels)
        };

        ResolvedSeriesData {
            name,
            x_values,
            x_labels,
            y_values,
            color,
            index,
        }
    }

    /// Resolve a cell range to chart cell data, tracking dependencies
    fn resolve_range(
        &mut self,
        range: &CellRange,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Vec<ChartCellData> {
        let mut data = Vec::new();

        for coord in range.iter() {
            let cell_data = self.resolve_cell(sheet, coord, engine);
            data.push(cell_data);
        }

        data
    }

    /// Resolve a cell range as numbers (NaN for non-numeric)
    fn resolve_range_as_numbers(
        &mut self,
        range: &CellRange,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Vec<f64> {
        self.resolve_range(range, sheet, engine)
            .into_iter()
            .map(|d| d.as_number().unwrap_or(f64::NAN))
            .collect()
    }

    /// Resolve a single cell
    fn resolve_cell(&mut self, sheet: u32, coord: CellCoord, engine: &CalcEngine) -> ChartCellData {
        match engine.get_value(sheet, coord) {
            CellResult::Empty => ChartCellData::Empty,
            CellResult::Value(n) => ChartCellData::Number(n),
            CellResult::Text(s) => ChartCellData::Text(s),
            CellResult::Bool(b) => ChartCellData::Number(if b { 1.0 } else { 0.0 }),
            CellResult::Error(_) => ChartCellData::Error,
        }
    }

    /// Register cell dependencies for a chart
    pub fn register_dependencies(&mut self, chart: &ChartDefinition) {
        let sheet = chart.sheet_index;

        for range in chart.dependent_ranges() {
            for coord in range.iter() {
                self.dependencies
                    .entry((sheet, coord.row, coord.col))
                    .or_default()
                    .push(chart.id);
            }
        }
    }

    /// Get the current cache version
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// Thread-safe wrapper for ChartDataResolver
pub type SharedChartResolver = Arc<std::sync::RwLock<ChartDataResolver>>;

/// Create a new shared chart resolver
pub fn create_shared_resolver() -> SharedChartResolver {
    Arc::new(std::sync::RwLock::new(ChartDataResolver::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::chart::ChartKind;

    #[test]
    fn test_resolve_chart_data() {
        let mut engine = CalcEngine::new();

        // Set up some test data
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(20.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(30.0));

        // Create a chart definition
        let chart = ChartDefinition::new(ChartKind::Line)
            .with_title("Test")
            .with_series(ChartSeries::new(CellRange::new(
                CellCoord::new(0, 0),
                CellCoord::new(2, 0),
            )))
            .with_sheet(0);

        let mut resolver = ChartDataResolver::new();
        let data = resolver.get_chart_data(&chart, &engine);

        assert_eq!(data.series.len(), 1);
        assert_eq!(data.series[0].y_values, vec![10.0, 20.0, 30.0]);
        assert_eq!(data.series[0].x_values, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_cache_invalidation() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));

        let chart = ChartDefinition::new(ChartKind::Line)
            .with_series(ChartSeries::new(CellRange::single(CellCoord::new(0, 0))))
            .with_sheet(0);

        let mut resolver = ChartDataResolver::new();

        // First resolve
        let data1 = resolver.get_chart_data(&chart, &engine);
        assert_eq!(data1.series[0].y_values[0], 10.0);

        let version1 = resolver.version();

        // Change the cell
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(99.0));
        resolver.invalidate_cell(0, 0, 0);

        // Second resolve should get new value
        let data2 = resolver.get_chart_data(&chart, &engine);
        assert_eq!(data2.series[0].y_values[0], 99.0);
        assert!(resolver.version() > version1);
    }
}
