//! GPU-accelerated rendering for the spreadsheet grid
//!
//! This module provides a wgpu-based renderer for displaying spreadsheet data
//! with hardware-accelerated text rendering via glyphon/cosmic-text.

mod grid;
mod renderer;
mod text;

pub use grid::GridRenderer;
pub use renderer::{GpuRenderer, RenderConfig};
pub use text::TextRenderer;
