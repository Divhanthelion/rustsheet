//! RustSheet - A high-performance spreadsheet engine in Rust
//!
//! This library provides a complete spreadsheet engine with:
//! - Sparse cell storage optimized for large grids
//! - Excel-compatible formula parsing and evaluation
//! - Incremental computation with dependency tracking
//! - Excel file I/O (xlsx format)
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

pub mod cell;
pub mod grid;
pub mod formula;
pub mod calc;
pub mod chart;

#[cfg(feature = "xlsx")]
pub mod xlsx;

#[cfg(feature = "gui")]
pub mod gui;

#[cfg(feature = "gpu")]
pub mod render;

#[cfg(feature = "wasm")]
pub mod wasm;

/// Re-exports of commonly used types
pub mod prelude {
    pub use crate::cell::{CellCoord, CellRange, CellValue, CellError, StringPool};
    pub use crate::grid::{Sheet, SparseGrid};
    pub use crate::formula::{Expr, BinaryOp, UnaryOp, FunctionCall, FormulaParser};
    pub use crate::calc::{CalcEngine, CellResult, CellValueInput};
    pub use crate::chart::{
        ChartId, ChartKind, ChartDefinition, ChartSeries, ChartStyle,
        ChartOverlayArea, ChartDataResolver, SheetObject, SheetObjectManager,
    };

    #[cfg(feature = "xlsx")]
    pub use crate::xlsx::{XlsxReader, XlsxWriter};

    #[cfg(feature = "gpu")]
    pub use crate::render::{GpuRenderer, RenderConfig, TextRenderer, GridRenderer};
}

pub use calc::CellValueInput;
pub use calc::CellResult;
