//! Chart definitions, configuration, and data resolution
//!
//! This module provides:
//! - Chart type definitions (`ChartKind`, `ChartDefinition`, `ChartSeries`)
//! - Chart positioning and styling (`ChartOverlayArea`, `ChartStyle`)
//! - Floating object management (`SheetObject`, `SheetObjectManager`)
//! - Data resolution with caching (`ChartDataResolver`)
//!
//! # Example
//!
//! ```rust
//! use rustsheet::chart::{ChartDefinition, ChartKind, ChartSeries, ChartOverlayArea};
//! use rustsheet::cell::{CellCoord, CellRange};
//!
//! let chart = ChartDefinition::new(ChartKind::Bar)
//!     .with_title("Monthly Sales")
//!     .with_series(
//!         ChartSeries::new(CellRange::new(
//!             CellCoord::new(1, 1),
//!             CellCoord::new(12, 1),
//!         ))
//!         .with_name("Revenue")
//!         .with_x_range(CellRange::new(
//!             CellCoord::new(1, 0),
//!             CellCoord::new(12, 0),
//!         ))
//!     )
//!     .with_x_label("Month")
//!     .with_y_label("Revenue ($)")
//!     .with_overlay_area(ChartOverlayArea::new(0, 4, 500.0, 350.0));
//! ```

mod definition;
mod objects;
mod database;
mod downsample;

#[cfg(feature = "gui")]
pub mod render;

pub use definition::*;
pub use objects::*;
pub use database::*;
pub use downsample::*;

#[cfg(feature = "gui")]
pub use render::{render_chart, CartesianRenderer, PolarRenderer, ChartRenderer};
