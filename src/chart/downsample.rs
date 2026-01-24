//! Downsampling algorithms for chart rendering performance
//!
//! Implements the LTTB (Largest Triangle Three Buckets) algorithm for
//! visually-preserving downsampling of time series data.

/// Largest Triangle Three Buckets (LTTB) downsampling algorithm
///
/// This algorithm preserves the visual characteristics of the original data
/// while reducing the number of points. It selects points that maximize the
/// area of triangles formed with adjacent selected points.
///
/// # Algorithm
/// 1. Always include the first and last points
/// 2. Divide remaining points into (target - 2) buckets
/// 3. For each bucket, select the point that forms the largest triangle with
///    the previously selected point and the average of the next bucket
///
/// # References
/// - Sveinn Steinarsson, "Downsampling Time Series for Visual Representation"
///   https://skemman.is/bitstream/1946/15343/3/SS_MSthesis.pdf
pub fn lttb_downsample(data: &[(f64, f64)], target: usize) -> Vec<(f64, f64)> {
    let data_len = data.len();

    // If target >= data length, return all points
    if target >= data_len {
        return data.to_vec();
    }

    // Need at least 3 points to downsample meaningfully
    if target < 3 {
        if data_len == 0 {
            return vec![];
        } else if data_len == 1 || target == 1 {
            return vec![data[0]];
        } else {
            // target == 2
            return vec![data[0], data[data_len - 1]];
        }
    }

    let mut result = Vec::with_capacity(target);

    // Always include the first point
    result.push(data[0]);

    // Number of buckets (excluding first and last points)
    let bucket_count = target - 2;
    let bucket_size = (data_len - 2) as f64 / bucket_count as f64;

    let mut prev_selected = data[0];

    for i in 0..bucket_count {
        // Calculate the current bucket's range
        let bucket_start = ((i as f64 * bucket_size) + 1.0) as usize;
        let bucket_end = (((i + 1) as f64 * bucket_size) + 1.0) as usize;
        let bucket_end = bucket_end.min(data_len - 1);

        // Calculate the average of the next bucket (for triangle area calculation)
        let (next_avg_x, next_avg_y) = if i + 1 < bucket_count {
            let next_start = bucket_end;
            let next_end = (((i + 2) as f64 * bucket_size) + 1.0) as usize;
            let next_end = next_end.min(data_len - 1);
            average_point(&data[next_start..next_end])
        } else {
            // Last bucket uses the final point
            data[data_len - 1]
        };

        // Find the point in this bucket that maximizes triangle area
        let mut max_area = -1.0;
        let mut selected_idx = bucket_start;

        for j in bucket_start..bucket_end {
            let area = triangle_area(
                prev_selected.0,
                prev_selected.1,
                data[j].0,
                data[j].1,
                next_avg_x,
                next_avg_y,
            );

            if area > max_area {
                max_area = area;
                selected_idx = j;
            }
        }

        prev_selected = data[selected_idx];
        result.push(prev_selected);
    }

    // Always include the last point
    result.push(data[data_len - 1]);

    result
}

/// Calculate the area of a triangle formed by three points
/// Uses the shoelace formula: Area = |x1(y2 - y3) + x2(y3 - y1) + x3(y1 - y2)| / 2
#[inline]
fn triangle_area(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> f64 {
    ((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0).abs()
}

/// Calculate the average point of a slice
#[inline]
fn average_point(points: &[(f64, f64)]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let sum: (f64, f64) = points.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
    (sum.0 / points.len() as f64, sum.1 / points.len() as f64)
}

/// Downsample chart series data if it exceeds the threshold
///
/// # Arguments
/// * `x_values` - X coordinates
/// * `y_values` - Y coordinates
/// * `threshold` - Maximum number of points to return
///
/// # Returns
/// Tuple of (downsampled_x, downsampled_y) vectors
pub fn downsample_series(
    x_values: &[f64],
    y_values: &[f64],
    threshold: usize,
) -> (Vec<f64>, Vec<f64>) {
    debug_assert_eq!(x_values.len(), y_values.len());

    let len = x_values.len();
    if len <= threshold {
        return (x_values.to_vec(), y_values.to_vec());
    }

    // Combine into points
    let points: Vec<(f64, f64)> = x_values
        .iter()
        .zip(y_values.iter())
        .map(|(&x, &y)| (x, y))
        .collect();

    // Downsample
    let downsampled = lttb_downsample(&points, threshold);

    // Split back into x and y vectors
    let x: Vec<f64> = downsampled.iter().map(|p| p.0).collect();
    let y: Vec<f64> = downsampled.iter().map(|p| p.1).collect();

    (x, y)
}

/// Default threshold for downsampling (tuned for 60 FPS rendering)
pub const DEFAULT_DOWNSAMPLE_THRESHOLD: usize = 2000;

/// Configuration for downsampling behavior
#[derive(Debug, Clone, Copy)]
pub struct DownsampleConfig {
    /// Maximum number of points per series
    pub threshold: usize,
    /// Whether downsampling is enabled
    pub enabled: bool,
}

impl Default for DownsampleConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_DOWNSAMPLE_THRESHOLD,
            enabled: true,
        }
    }
}

impl DownsampleConfig {
    /// Create a new configuration with custom threshold
    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            threshold,
            enabled: true,
        }
    }

    /// Disable downsampling
    pub fn disabled() -> Self {
        Self {
            threshold: usize::MAX,
            enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lttb_empty_data() {
        let data: Vec<(f64, f64)> = vec![];
        let result = lttb_downsample(&data, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_lttb_single_point() {
        let data = vec![(1.0, 2.0)];
        let result = lttb_downsample(&data, 10);
        assert_eq!(result, vec![(1.0, 2.0)]);
    }

    #[test]
    fn test_lttb_no_downsampling_needed() {
        let data = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)];
        let result = lttb_downsample(&data, 10);
        assert_eq!(result, data);
    }

    #[test]
    fn test_lttb_preserves_first_and_last() {
        let data: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, (i * i) as f64)).collect();
        let result = lttb_downsample(&data, 10);

        assert_eq!(result.len(), 10);
        assert_eq!(result[0], data[0]);
        assert_eq!(result[result.len() - 1], data[data.len() - 1]);
    }

    #[test]
    fn test_lttb_preserves_visual_features() {
        // Create data with a spike in the middle
        let mut data: Vec<(f64, f64)> = vec![];
        for i in 0..100 {
            let y = if i == 50 { 100.0 } else { 10.0 };
            data.push((i as f64, y));
        }

        let result = lttb_downsample(&data, 10);

        // The spike should be preserved
        let max_y = result.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        assert_eq!(max_y, 100.0, "LTTB should preserve the spike");
    }

    #[test]
    fn test_lttb_linear_data() {
        // Linear data - should distribute points evenly
        let data: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, i as f64)).collect();
        let result = lttb_downsample(&data, 10);

        assert_eq!(result.len(), 10);
        // First and last should be exact
        assert_eq!(result[0], (0.0, 0.0));
        assert_eq!(result[9], (99.0, 99.0));
    }

    #[test]
    fn test_downsample_series() {
        let x: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let y: Vec<f64> = (0..100).map(|i| (i * 2) as f64).collect();

        let (dx, dy) = downsample_series(&x, &y, 20);

        assert_eq!(dx.len(), 20);
        assert_eq!(dy.len(), 20);
        assert_eq!(dx[0], 0.0);
        assert_eq!(dx[19], 99.0);
    }

    #[test]
    fn test_triangle_area() {
        // Triangle with vertices at (0,0), (2,0), (1,2) has area 2
        let area = triangle_area(0.0, 0.0, 2.0, 0.0, 1.0, 2.0);
        assert!((area - 2.0).abs() < 0.001);

        // Collinear points have area 0
        let area_collinear = triangle_area(0.0, 0.0, 1.0, 1.0, 2.0, 2.0);
        assert!(area_collinear.abs() < 0.001);
    }

    #[test]
    fn test_downsample_config() {
        let default_config = DownsampleConfig::default();
        assert!(default_config.enabled);
        assert_eq!(default_config.threshold, DEFAULT_DOWNSAMPLE_THRESHOLD);

        let custom = DownsampleConfig::with_threshold(500);
        assert!(custom.enabled);
        assert_eq!(custom.threshold, 500);

        let disabled = DownsampleConfig::disabled();
        assert!(!disabled.enabled);
    }
}
