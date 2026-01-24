//! GUI module for the spreadsheet application
//!
//! Uses egui/eframe for cross-platform immediate-mode UI rendering.

pub mod app;
mod grid;
mod formula_bar;
mod selection;
mod sheet_tabs;
mod theme;
mod functions_help;
mod help_panel;

pub use app::SpreadsheetApp;
pub use selection::{Selection, SelectionRange};
