//! RustSheet - A high-performance spreadsheet engine in Rust
//!
//! This library provides a spreadsheet engine with:
//! - Sparse cell storage optimized for large grids
//! - Excel-compatible formula parsing and evaluation
//! - Incremental computation with dependency tracking
//! - Cross-sheet references and sheet identity remapping
//! - Excel (`.xlsx`) and CSV file I/O
//!
//! # Example
//!
//! ```rust
//! use rustsheet::prelude::*;
//!
//! // Create a new sheet
//! let mut sheet = Sheet::new("Data");
//!
//! // Set some values
//! sheet.set_number(CellCoord::from_a1("A1").unwrap(), 10.0);
//! sheet.set_number(CellCoord::from_a1("A2").unwrap(), 20.0);
//!
//! // Create calculation engine and add formula
//! let mut engine = CalcEngine::new();
//! engine.set_value(0, CellCoord::from_a1("A1").unwrap(),
//!     CellValueInput::Number(10.0));
//! engine.set_value(0, CellCoord::from_a1("A2").unwrap(),
//!     CellValueInput::Number(20.0));
//! engine.set_formula(0, CellCoord::from_a1("A3").unwrap(),
//!     "=SUM(A1:A2)").unwrap();
//!
//! // Get computed value
//! let result = engine.get_value(0, CellCoord::from_a1("A3").unwrap());
//! // result == CellResult::Value(30.0)
//! ```

pub mod calc;
pub mod cell;
pub mod chart;
pub mod formula;
pub mod grid;

#[cfg(feature = "xlsx")]
pub mod xlsx;

#[cfg(feature = "csv")]
pub mod csv_io;

#[cfg(any(feature = "gui", feature = "web"))]
pub mod gui;

#[cfg(feature = "gpu")]
pub mod render;

#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "wasm")]
pub mod wasm;

/// Re-exports of commonly used types
pub mod prelude {
    pub use crate::calc::{CalcEngine, CellResult, CellValueInput};
    pub use crate::cell::{CellCoord, CellError, CellRange, CellValue, StringPool};
    pub use crate::chart::{
        ChartDataResolver, ChartDefinition, ChartId, ChartKind, ChartOverlayArea, ChartSeries,
        ChartStyle, SheetObject, SheetObjectManager,
    };
    pub use crate::formula::{BinaryOp, Expr, FormulaParser, FunctionCall, UnaryOp};
    pub use crate::grid::{Sheet, SparseGrid};

    #[cfg(feature = "xlsx")]
    pub use crate::xlsx::{XlsxReader, XlsxWriter};

    #[cfg(feature = "gpu")]
    pub use crate::render::{GpuRenderer, GridRenderer, RenderConfig, TextRenderer};
}

pub use calc::CellResult;
pub use calc::CellValueInput;

/// WASM entry point for web builds
#[cfg(feature = "web")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        // Get the canvas element
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("Failed to find canvas element");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(gui::app::SpreadsheetApp::new()))),
            )
            .await;

        // Remove loading screen
        if let Some(loading) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("loading"))
        {
            loading.remove();
        }

        if let Err(e) = start_result {
            web_sys::console::error_1(&format!("Failed to start: {:?}", e).into());
        }
    });
}
