//! WebAssembly bindings for RustSheet
//!
//! This module provides JavaScript-friendly bindings for the spreadsheet engine
//! when compiled to WASM.

use wasm_bindgen::prelude::*;
use crate::calc::{CalcEngine, CellResult, CellValueInput};
use crate::cell::CellCoord;

/// WASM-friendly spreadsheet engine wrapper
#[wasm_bindgen]
pub struct WasmSpreadsheet {
    engine: CalcEngine,
}

#[wasm_bindgen]
impl WasmSpreadsheet {
    /// Create a new spreadsheet engine
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            engine: CalcEngine::new(),
        }
    }

    /// Set a numeric value in a cell
    ///
    /// # Arguments
    /// * `sheet` - Sheet index (0-based)
    /// * `cell` - Cell address (e.g., "A1", "B2")
    /// * `value` - Numeric value
    #[wasm_bindgen(js_name = setNumber)]
    pub fn set_number(&mut self, sheet: u32, cell: &str, value: f64) -> Result<(), JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        self.engine.set_value(sheet, coord, CellValueInput::Number(value));
        Ok(())
    }

    /// Set a text value in a cell
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(&mut self, sheet: u32, cell: &str, value: &str) -> Result<(), JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        self.engine.set_value(sheet, coord, CellValueInput::Text(value.to_string()));
        Ok(())
    }

    /// Set a boolean value in a cell
    #[wasm_bindgen(js_name = setBool)]
    pub fn set_bool(&mut self, sheet: u32, cell: &str, value: bool) -> Result<(), JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        self.engine.set_value(sheet, coord, CellValueInput::Bool(value));
        Ok(())
    }

    /// Set a formula in a cell
    #[wasm_bindgen(js_name = setFormula)]
    pub fn set_formula(&mut self, sheet: u32, cell: &str, formula: &str) -> Result<(), JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        self.engine
            .set_formula(sheet, coord, formula)
            .map_err(|e| JsValue::from_str(&format!("Formula error: {}", e)))?;
        Ok(())
    }

    /// Get the value of a cell as a JavaScript value
    #[wasm_bindgen(js_name = getValue)]
    pub fn get_value(&self, sheet: u32, cell: &str) -> Result<JsValue, JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        let result = self.engine.get_value(sheet, coord);
        Ok(cell_result_to_js(&result))
    }

    /// Get the display string for a cell
    #[wasm_bindgen(js_name = getDisplayValue)]
    pub fn get_display_value(&self, sheet: u32, cell: &str) -> Result<String, JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        let result = self.engine.get_value(sheet, coord);
        Ok(cell_result_to_string(&result))
    }

    /// Check if a cell has an error
    #[wasm_bindgen(js_name = isError)]
    pub fn is_error(&self, sheet: u32, cell: &str) -> Result<bool, JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        let result = self.engine.get_value(sheet, coord);
        Ok(result.is_error())
    }

    /// Get error message if cell has an error
    #[wasm_bindgen(js_name = getError)]
    pub fn get_error(&self, sheet: u32, cell: &str) -> Result<Option<String>, JsValue> {
        let coord = CellCoord::from_a1(cell)
            .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
        let result = self.engine.get_value(sheet, coord);
        Ok(match result {
            CellResult::Error(e) => Some(e.as_str().to_string()),
            _ => None,
        })
    }
}

impl Default for WasmSpreadsheet {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert CellResult to JavaScript value
fn cell_result_to_js(result: &CellResult) -> JsValue {
    match result {
        CellResult::Value(n) => JsValue::from_f64(*n),
        CellResult::Text(s) => JsValue::from_str(s),
        CellResult::Bool(b) => JsValue::from_bool(*b),
        CellResult::Empty => JsValue::NULL,
        CellResult::Error(e) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &"error".into(), &JsValue::from_str(e.as_str()));
            obj.into()
        }
    }
}

/// Convert CellResult to display string
fn cell_result_to_string(result: &CellResult) -> String {
    match result {
        CellResult::Value(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        CellResult::Text(s) => s.clone(),
        CellResult::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellResult::Empty => String::new(),
        CellResult::Error(e) => e.as_str().to_string(),
    }
}

/// Initialize panic hook for better error messages in browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Convert column index (0-based) to letter(s)
#[wasm_bindgen(js_name = columnToLetter)]
pub fn column_to_letter(col: u32) -> String {
    col_to_letters(col)
}

/// Convert column letter(s) to index (0-based)
#[wasm_bindgen(js_name = letterToColumn)]
pub fn letter_to_column(letters: &str) -> Option<u32> {
    letters_to_col(letters)
}

/// Parse a cell address and return row/column indices
#[wasm_bindgen(js_name = parseCell)]
pub fn parse_cell(cell: &str) -> Result<JsValue, JsValue> {
    let coord = CellCoord::from_a1(cell)
        .ok_or_else(|| JsValue::from_str(&format!("Invalid cell address: {}", cell)))?;
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"row".into(), &JsValue::from(coord.row));
    let _ = js_sys::Reflect::set(&obj, &"col".into(), &JsValue::from(coord.col));
    Ok(obj.into())
}

/// Convert 0-based column index to Excel-style letters (A, B, ... Z, AA, AB, ...)
fn col_to_letters(col: u32) -> String {
    let mut result = String::new();
    let mut n = col + 1; // Convert to 1-based for the algorithm

    while n > 0 {
        n -= 1;
        let c = ((n % 26) as u8 + b'A') as char;
        result.insert(0, c);
        n /= 26;
    }

    result
}

/// Convert Excel-style column letters to 0-based index
fn letters_to_col(letters: &str) -> Option<u32> {
    let mut col: u32 = 0;
    for c in letters.chars() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        col = col * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
    }
    if col == 0 {
        None
    } else {
        Some(col - 1) // Convert back to 0-based
    }
}
