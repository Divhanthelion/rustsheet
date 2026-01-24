//! GPU-accelerated rendering for the spreadsheet grid
//!
//! This module provides a wgpu-based renderer for displaying spreadsheet data
//! with hardware-accelerated text rendering via glyphon/cosmic-text.

mod renderer;
mod text;
mod grid;

pub use renderer::{GpuRenderer, RenderConfig};
pub use text::TextRenderer;
pub use grid::GridRenderer;
