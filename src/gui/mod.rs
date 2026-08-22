//! GUI module for the spreadsheet application
//!
//! Uses egui/eframe for cross-platform immediate-mode UI rendering.

pub mod app;
mod chart_editor;
mod chart_widget;
mod formula_bar;
mod functions_help;
mod grid;
mod help_panel;
mod selection;
mod sheet_tabs;
mod theme;

pub use app::SpreadsheetApp;
pub use chart_editor::{ChartEditor, ChartEditorResponse};
pub use chart_widget::{ChartWidget, ChartWindowManager, ChartWindowState};
pub use selection::{Selection, SelectionRange};
