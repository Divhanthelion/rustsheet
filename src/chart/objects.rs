//! Floating objects (charts, images) that exist above the grid layer

use super::{ChartDefinition, ChartId};
use serde::{Deserialize, Serialize};

/// Unique identifier for a floating object
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        Self((timestamp << 20) | (count & 0xFFFFF))
    }
}

impl Default for ObjectId {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of floating object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectType {
    Chart(ChartDefinition),
    // Future: Image(ImageData), TextBox(TextBoxData), Shape(ShapeData)
}

/// A floating object that exists above the grid layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetObject {
    /// Unique identifier
    pub id: ObjectId,
    /// The object's data/configuration
    pub object_type: ObjectType,
    /// Position in grid coordinates (floating point for sub-cell positioning)
    /// (row, col) where 0.5 means halfway through the cell
    pub anchor_top_left: (f64, f64),
    /// Size in pixels
    pub size_pixels: (f32, f32),
    /// Z-index for layering (higher = on top)
    pub z_index: u32,
    /// Whether this object is currently selected
    #[serde(skip)]
    pub selected: bool,
    /// Whether the object is being dragged
    #[serde(skip)]
    pub dragging: bool,
    /// Whether the object is being resized
    #[serde(skip)]
    pub resizing: bool,
}

impl SheetObject {
    /// Create a new chart object
    pub fn new_chart(id: ObjectId, config: ChartDefinition, position: (f64, f64)) -> Self {
        let size = config.overlay_area.size;
        Self {
            id,
            object_type: ObjectType::Chart(config),
            anchor_top_left: position,
            size_pixels: size,
            z_index: 0,
            selected: false,
            dragging: false,
            resizing: false,
        }
    }

    /// Create a chart object from a ChartDefinition, using its overlay_area for positioning
    pub fn from_chart_definition(def: ChartDefinition) -> Self {
        let position = (
            def.overlay_area.anchor_cell.0 as f64,
            def.overlay_area.anchor_cell.1 as f64,
        );
        let size = def.overlay_area.size;
        Self {
            id: ObjectId::new(),
            object_type: ObjectType::Chart(def),
            anchor_top_left: position,
            size_pixels: size,
            z_index: 0,
            selected: false,
            dragging: false,
            resizing: false,
        }
    }

    /// Get the chart config if this is a chart
    pub fn as_chart(&self) -> Option<&ChartDefinition> {
        match &self.object_type {
            ObjectType::Chart(config) => Some(config),
        }
    }

    /// Get mutable chart config if this is a chart
    pub fn as_chart_mut(&mut self) -> Option<&mut ChartDefinition> {
        match &mut self.object_type {
            ObjectType::Chart(config) => Some(config),
        }
    }

    /// Get the chart ID if this is a chart
    pub fn chart_id(&self) -> Option<ChartId> {
        self.as_chart().map(|c| c.id)
    }

    /// Check if a point (in pixel coordinates relative to grid origin) is inside this object
    pub fn contains_point(&self, point: (f32, f32), grid_offset: (f32, f32)) -> bool {
        let (obj_x, obj_y) = (grid_offset.0, grid_offset.1);
        let (w, h) = self.size_pixels;

        point.0 >= obj_x && point.0 <= obj_x + w && point.1 >= obj_y && point.1 <= obj_y + h
    }

    /// Check if a point is in the resize handle area (bottom-right corner)
    pub fn is_in_resize_handle(&self, point: (f32, f32), grid_offset: (f32, f32)) -> bool {
        let handle_size = 12.0;
        let (obj_x, obj_y) = (grid_offset.0, grid_offset.1);
        let (w, h) = self.size_pixels;

        let handle_x = obj_x + w - handle_size;
        let handle_y = obj_y + h - handle_size;

        point.0 >= handle_x && point.0 <= obj_x + w && point.1 >= handle_y && point.1 <= obj_y + h
    }

    /// Update size from drag operation
    pub fn resize(&mut self, delta: (f32, f32)) {
        let min_size = 100.0;
        self.size_pixels.0 = (self.size_pixels.0 + delta.0).max(min_size);
        self.size_pixels.1 = (self.size_pixels.1 + delta.1).max(min_size);

        // Update the chart definition's overlay area too
        let new_size = self.size_pixels;
        if let Some(chart) = self.as_chart_mut() {
            chart.overlay_area.size = new_size;
        }
    }

    /// Move the object by delta in grid coordinates
    pub fn move_by(&mut self, delta_row: f64, delta_col: f64) {
        self.anchor_top_left.0 = (self.anchor_top_left.0 + delta_row).max(0.0);
        self.anchor_top_left.1 = (self.anchor_top_left.1 + delta_col).max(0.0);

        // Update the chart definition's overlay area too
        let new_anchor = (self.anchor_top_left.0 as u32, self.anchor_top_left.1 as u32);
        if let Some(chart) = self.as_chart_mut() {
            chart.overlay_area.anchor_cell = new_anchor;
        }
    }
}

/// Cached chart data for efficient rendering
#[derive(Debug, Clone, Default)]
pub struct ChartDataCache {
    /// Cached Y values for each series
    pub series_y_values: Vec<Vec<Option<f64>>>,
    /// Cached X values for each series (numeric)
    pub series_x_values: Vec<Vec<Option<f64>>>,
    /// Cached X labels for each series (categorical)
    pub series_x_labels: Vec<Vec<String>>,
    /// Whether the cache is valid
    pub valid: bool,
    /// Cache version (increments when invalidated)
    pub version: u64,
}

impl ChartDataCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
        self.version = self.version.wrapping_add(1);
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn mark_valid(&mut self) {
        self.valid = true;
    }
}

/// Manager for all floating objects on a sheet
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SheetObjectManager {
    /// All objects on this sheet
    objects: Vec<SheetObject>,
    /// Next z-index to assign
    next_z_index: u32,
}

impl SheetObjectManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new object and return its ID
    pub fn add(&mut self, mut object: SheetObject) -> ObjectId {
        object.z_index = self.next_z_index;
        self.next_z_index += 1;
        let id = object.id;
        self.objects.push(object);
        id
    }

    /// Remove an object by ID
    pub fn remove(&mut self, id: ObjectId) -> Option<SheetObject> {
        if let Some(pos) = self.objects.iter().position(|o| o.id == id) {
            Some(self.objects.remove(pos))
        } else {
            None
        }
    }

    /// Get an object by ID
    pub fn get(&self, id: ObjectId) -> Option<&SheetObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Get a mutable reference to an object by ID
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut SheetObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    /// Get all objects
    pub fn objects(&self) -> &[SheetObject] {
        &self.objects
    }

    /// Get all objects mutably
    pub fn objects_mut(&mut self) -> &mut [SheetObject] {
        &mut self.objects
    }

    /// Get all chart objects
    pub fn charts(&self) -> impl Iterator<Item = &SheetObject> {
        self.objects
            .iter()
            .filter(|o| matches!(o.object_type, ObjectType::Chart(_)))
    }

    /// Bring an object to the front
    pub fn bring_to_front(&mut self, id: ObjectId) {
        let new_z = self.next_z_index;
        if let Some(obj) = self.get_mut(id) {
            obj.z_index = new_z;
            self.next_z_index = new_z + 1;
        }
    }

    /// Find the topmost object at a given point
    pub fn object_at_point(
        &self,
        point: (f32, f32),
        grid_offset_fn: impl Fn(&SheetObject) -> (f32, f32),
    ) -> Option<ObjectId> {
        // Sort by z-index descending to find topmost first
        let mut sorted: Vec<_> = self.objects.iter().collect();
        sorted.sort_by_key(|a| std::cmp::Reverse(a.z_index));

        for obj in sorted {
            let offset = grid_offset_fn(obj);
            if obj.contains_point(point, offset) {
                return Some(obj.id);
            }
        }
        None
    }

    /// Clear selection on all objects
    pub fn deselect_all(&mut self) {
        for obj in &mut self.objects {
            obj.selected = false;
        }
    }

    /// Select an object by ID
    pub fn select(&mut self, id: ObjectId) {
        self.deselect_all();
        if let Some(obj) = self.get_mut(id) {
            obj.selected = true;
        }
    }

    /// Get the selected object, if any
    pub fn selected(&self) -> Option<&SheetObject> {
        self.objects.iter().find(|o| o.selected)
    }

    /// Get the selected object mutably, if any
    pub fn selected_mut(&mut self) -> Option<&mut SheetObject> {
        self.objects.iter_mut().find(|o| o.selected)
    }

    /// Check if there are any objects
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Get the number of objects
    pub fn len(&self) -> usize {
        self.objects.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{CellCoord, CellRange};
    use crate::chart::{ChartKind, ChartSeries};

    #[test]
    fn test_object_manager() {
        let mut manager = SheetObjectManager::new();

        let chart = ChartDefinition::new(ChartKind::Line)
            .with_title("Test Chart")
            .with_series(ChartSeries::new(CellRange::new(
                CellCoord::new(0, 0),
                CellCoord::new(10, 0),
            )));

        let obj = SheetObject::from_chart_definition(chart);
        let id = manager.add(obj);

        assert_eq!(manager.len(), 1);
        assert!(manager.get(id).is_some());
        assert!(manager.get(id).unwrap().as_chart().is_some());

        manager.select(id);
        assert!(manager.selected().is_some());

        manager.remove(id);
        assert!(manager.is_empty());
    }
}
