//! Main spreadsheet application

use crate::calc::{CalcEngine, CellResult, CellValueInput};
use crate::cell::{CellCoord, StringPool};
use crate::chart::{ChartDataResolver, ChartDefinition, ChartId};
use eframe::egui::{self, CentralPanel, Key, TopBottomPanel, Vec2};
use std::path::PathBuf;

use super::chart_editor::ChartEditor;
use super::chart_widget::ChartWindowManager;
use super::formula_bar::FormulaBar;
use super::grid::{GridConfig, NavigationKey, ScrollState, SpreadsheetGrid};
use super::help_panel::HelpPanel;
use super::selection::Selection;
use super::sheet_tabs::SheetTabs;
use super::theme::Theme;

/// Input mode FSM - decouples input handling from render order
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Standard navigation mode; Grid captures focus
    Navigation,
    /// Transition frame; Focus is being transferred to editor
    TransitionToEdit { initial_char: Option<char> },
    /// Editing mode; FormulaBar captures focus
    Editing { cell: CellCoord },
}

impl Default for InputMode {
    fn default() -> Self {
        Self::Navigation
    }
}

/// Per-sheet state that gets saved/restored when switching sheets
#[derive(Clone)]
pub struct SheetState {
    pub selection: Selection,
    pub scroll: ScrollState,
}

/// Actions from sheet tab interactions (to avoid borrow conflicts)
enum SheetAction {
    Switch(u32),
    Add,
    Delete(u32),
    Rename(u32, String),
}

/// A single undoable action
#[derive(Clone)]
enum UndoAction {
    /// Cell value change: (sheet, coord, old_value, old_formula)
    CellChange {
        sheet: u32,
        coord: CellCoord,
        old_value: Option<CellSnapshot>,
        new_value: Option<CellSnapshot>,
    },
    /// Clear cell
    CellClear {
        sheet: u32,
        coord: CellCoord,
        old_value: Option<CellSnapshot>,
    },
}

/// Snapshot of a cell's state for undo/redo
#[derive(Clone)]
struct CellSnapshot {
    /// The input that was used to set the cell
    input: String,
}

/// Undo/redo history
struct UndoHistory {
    undo_stack: Vec<UndoAction>,
    redo_stack: Vec<UndoAction>,
    max_history: usize,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: 100,
        }
    }
}

impl UndoHistory {
    fn push(&mut self, action: UndoAction) {
        self.undo_stack.push(action);
        self.redo_stack.clear(); // Clear redo stack on new action
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }

    fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn pop_undo(&mut self) -> Option<UndoAction> {
        self.undo_stack.pop()
    }

    fn pop_redo(&mut self) -> Option<UndoAction> {
        self.redo_stack.pop()
    }

    fn push_redo(&mut self, action: UndoAction) {
        self.redo_stack.push(action);
    }

    fn push_undo_for_redo(&mut self, action: UndoAction) {
        self.undo_stack.push(action);
    }

    fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(feature = "xlsx")]
use crate::xlsx::{XlsxReader, XlsxWriter};

/// Main application state
pub struct SpreadsheetApp {
    /// The calculation engine
    engine: CalcEngine,
    /// String pool for text interning
    string_pool: StringPool,
    /// Current sheet index
    current_sheet: u32,
    /// Sheet names (one per sheet)
    sheet_names: Vec<String>,
    /// Per-sheet state (selection, scroll)
    sheet_states: Vec<SheetState>,
    /// Cell selection state (current sheet)
    selection: Selection,
    /// Grid configuration (column widths, row heights)
    grid_config: GridConfig,
    /// Scroll state (current sheet)
    scroll: ScrollState,
    /// Formula bar state
    formula_bar: FormulaBar,
    /// Help panel state
    help_panel: HelpPanel,
    /// UI theme
    theme: Theme,
    /// Input mode FSM (replaces boolean editing flag)
    input_mode: InputMode,
    /// Edit buffer for inline cell editing
    edit_buffer: String,
    /// Maximum row index with data
    max_row: u32,
    /// Maximum column index with data
    max_col: u32,
    /// Current file path (if saved)
    current_file: Option<PathBuf>,
    /// Whether the document has unsaved changes
    modified: bool,
    /// Status message to display
    status_message: Option<(String, std::time::Instant)>,
    /// Undo/redo history
    undo_history: UndoHistory,
    /// Chart window manager
    chart_windows: ChartWindowManager,
    /// Chart editor dialog
    chart_editor: ChartEditor,
    /// Chart data resolver for caching
    chart_data_resolver: ChartDataResolver,
}

impl Default for SpreadsheetApp {
    fn default() -> Self {
        Self::new()
    }
}

impl SpreadsheetApp {
    pub fn new() -> Self {
        let initial_state = SheetState {
            selection: Selection::default(),
            scroll: ScrollState::default(),
        };

        let mut app = Self {
            engine: CalcEngine::new(),
            string_pool: StringPool::new(),
            current_sheet: 0,
            sheet_names: vec!["Sheet1".to_string()],
            sheet_states: vec![initial_state],
            selection: Selection::default(),
            grid_config: GridConfig::default(),
            scroll: ScrollState::default(),
            formula_bar: FormulaBar::new(),
            help_panel: HelpPanel::default(),
            theme: Theme::light(),
            input_mode: InputMode::Navigation,
            edit_buffer: String::new(),
            max_row: 999,
            max_col: 25,
            current_file: None,
            modified: false,
            status_message: None,
            undo_history: UndoHistory::default(),
            chart_windows: ChartWindowManager::new(),
            chart_editor: ChartEditor::new(),
            chart_data_resolver: ChartDataResolver::new(),
        };

        // Add some demo data
        app.set_demo_data();
        app
    }

    fn set_demo_data(&mut self) {
        // Headers
        self.engine.set_value(
            0,
            CellCoord::new(0, 0),
            CellValueInput::Text("Item".to_string()),
        );
        self.engine.set_value(
            0,
            CellCoord::new(0, 1),
            CellValueInput::Text("Quantity".to_string()),
        );
        self.engine.set_value(
            0,
            CellCoord::new(0, 2),
            CellValueInput::Text("Price".to_string()),
        );
        self.engine.set_value(
            0,
            CellCoord::new(0, 3),
            CellValueInput::Text("Total".to_string()),
        );

        // Data rows
        self.engine.set_value(
            0,
            CellCoord::new(1, 0),
            CellValueInput::Text("Apples".to_string()),
        );
        self.engine
            .set_value(0, CellCoord::new(1, 1), CellValueInput::Number(10.0));
        self.engine
            .set_value(0, CellCoord::new(1, 2), CellValueInput::Number(1.50));
        let _ = self.engine.set_formula(0, CellCoord::new(1, 3), "=B2*C2");

        self.engine.set_value(
            0,
            CellCoord::new(2, 0),
            CellValueInput::Text("Oranges".to_string()),
        );
        self.engine
            .set_value(0, CellCoord::new(2, 1), CellValueInput::Number(8.0));
        self.engine
            .set_value(0, CellCoord::new(2, 2), CellValueInput::Number(2.00));
        let _ = self.engine.set_formula(0, CellCoord::new(2, 3), "=B3*C3");

        self.engine.set_value(
            0,
            CellCoord::new(3, 0),
            CellValueInput::Text("Bananas".to_string()),
        );
        self.engine
            .set_value(0, CellCoord::new(3, 1), CellValueInput::Number(15.0));
        self.engine
            .set_value(0, CellCoord::new(3, 2), CellValueInput::Number(0.75));
        let _ = self.engine.set_formula(0, CellCoord::new(3, 3), "=B4*C4");

        // Summary row
        self.engine.set_value(
            0,
            CellCoord::new(5, 2),
            CellValueInput::Text("Grand Total:".to_string()),
        );
        let _ = self
            .engine
            .set_formula(0, CellCoord::new(5, 3), "=SUM(D2:D4)");
    }

    /// Get the display content of the current cell
    fn get_cell_display(&self, coord: CellCoord) -> String {
        let value = self.engine.get_value(self.current_sheet, coord);
        match value {
            crate::calc::CellResult::Empty => String::new(),
            crate::calc::CellResult::Value(n) => {
                if n.fract() == 0.0 && n.abs() < 1e10 {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                }
            }
            crate::calc::CellResult::Text(s) => s,
            crate::calc::CellResult::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_string(),
            crate::calc::CellResult::Error(e) => format!("{:?}", e),
        }
    }

    /// Get the formula or value for the formula bar
    fn get_cell_formula_or_value(&self, coord: CellCoord) -> String {
        if let Some(formula) = self.engine.get_formula(self.current_sheet, coord) {
            return formula_bar_text(&formula);
        }

        self.get_cell_display(coord)
    }

    /// Get cell content as a string for undo/redo snapshots
    fn get_cell_content_string(&self, coord: CellCoord) -> Option<String> {
        if let Some(formula) = self.engine.get_formula(self.current_sheet, coord) {
            return Some(formula_bar_text(&formula));
        }

        // Otherwise get the value
        match self.engine.get_value(self.current_sheet, coord) {
            CellResult::Empty => None,
            CellResult::Value(n) => Some(n.to_string()),
            CellResult::Text(s) => Some(s),
            CellResult::Bool(b) => Some(if b { "TRUE" } else { "FALSE" }.to_string()),
            CellResult::Error(_) => None, // Don't snapshot errors
        }
    }

    /// Set cell content from user input (with undo support)
    fn set_cell_content(&mut self, coord: CellCoord, content: &str) {
        let content = content.trim();

        // Capture old value for undo
        let old_value = self
            .get_cell_content_string(coord)
            .map(|s| CellSnapshot { input: s });
        let new_value = if content.is_empty() {
            None
        } else {
            Some(CellSnapshot {
                input: content.to_string(),
            })
        };

        // Record undo action
        if content.is_empty() {
            self.undo_history.push(UndoAction::CellClear {
                sheet: self.current_sheet,
                coord,
                old_value,
            });
        } else {
            self.undo_history.push(UndoAction::CellChange {
                sheet: self.current_sheet,
                coord,
                old_value,
                new_value,
            });
        }

        // Apply the change
        if content.is_empty() {
            self.engine.clear(self.current_sheet, coord);
        } else if content.starts_with('=') {
            // Formula
            if let Err(e) = self.engine.set_formula(self.current_sheet, coord, content) {
                self.set_status(&format!("Formula error: {:?}", e));
            }
        } else if let Ok(n) = content.parse::<f64>() {
            // Number
            self.engine
                .set_value(self.current_sheet, coord, CellValueInput::Number(n));
        } else if content.eq_ignore_ascii_case("true") {
            self.engine
                .set_value(self.current_sheet, coord, CellValueInput::Bool(true));
        } else if content.eq_ignore_ascii_case("false") {
            self.engine
                .set_value(self.current_sheet, coord, CellValueInput::Bool(false));
        } else {
            // Text
            self.engine.set_value(
                self.current_sheet,
                coord,
                CellValueInput::Text(content.to_string()),
            );
        }
        self.modified = true;

        // Refresh charts that may depend on this cell
        self.refresh_all_charts();
    }

    /// Apply a cell content string (used by undo/redo)
    fn apply_cell_content(&mut self, sheet: u32, coord: CellCoord, content: Option<&str>) {
        match content {
            None => {
                self.engine.clear(sheet, coord);
            }
            Some(s) if s.starts_with('=') => {
                let _ = self.engine.set_formula(sheet, coord, s);
            }
            Some(s) if s.parse::<f64>().is_ok() => {
                self.engine
                    .set_value(sheet, coord, CellValueInput::Number(s.parse().unwrap()));
            }
            Some(s) if s.eq_ignore_ascii_case("true") => {
                self.engine
                    .set_value(sheet, coord, CellValueInput::Bool(true));
            }
            Some(s) if s.eq_ignore_ascii_case("false") => {
                self.engine
                    .set_value(sheet, coord, CellValueInput::Bool(false));
            }
            Some(s) => {
                self.engine
                    .set_value(sheet, coord, CellValueInput::Text(s.to_string()));
            }
        }
        self.modified = true;

        // Refresh charts that may depend on this cell
        self.refresh_all_charts();
    }

    /// Undo the last action
    fn undo(&mut self) {
        if let Some(action) = self.undo_history.pop_undo() {
            match &action {
                UndoAction::CellChange {
                    sheet,
                    coord,
                    old_value,
                    new_value: _,
                } => {
                    self.apply_cell_content(
                        *sheet,
                        *coord,
                        old_value.as_ref().map(|s| s.input.as_str()),
                    );
                    self.selection.move_to(*coord);
                }
                UndoAction::CellClear {
                    sheet,
                    coord,
                    old_value,
                } => {
                    self.apply_cell_content(
                        *sheet,
                        *coord,
                        old_value.as_ref().map(|s| s.input.as_str()),
                    );
                    self.selection.move_to(*coord);
                }
            }
            // Push to redo stack (with inverted action)
            self.undo_history.push_redo(action);
            self.set_status("Undo");
        }
    }

    /// Redo the last undone action
    fn redo(&mut self) {
        if let Some(action) = self.undo_history.pop_redo() {
            match &action {
                UndoAction::CellChange {
                    sheet,
                    coord,
                    old_value: _,
                    new_value,
                } => {
                    self.apply_cell_content(
                        *sheet,
                        *coord,
                        new_value.as_ref().map(|s| s.input.as_str()),
                    );
                    self.selection.move_to(*coord);
                }
                UndoAction::CellClear {
                    sheet,
                    coord,
                    old_value: _,
                } => {
                    self.engine.clear(*sheet, *coord);
                    self.selection.move_to(*coord);
                    self.modified = true;
                }
            }
            // Push back to undo stack
            self.undo_history.push_undo_for_redo(action);
            self.set_status("Redo");
        }
    }

    /// Handle navigation keys
    fn handle_navigation(&mut self, key: NavigationKey, shift: bool, viewport_size: Vec2) {
        let (row_delta, col_delta) = match key {
            NavigationKey::Up => (-1, 0),
            NavigationKey::Down => (1, 0),
            NavigationKey::Left => (0, -1),
            NavigationKey::Right => (0, 1),
            NavigationKey::Home => {
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(self.selection.active.row, 0));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(self.selection.active.row, 0));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::End => {
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(self.selection.active.row, self.max_col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(self.selection.active.row, self.max_col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::CtrlHome => {
                if !shift {
                    self.selection.move_to(CellCoord::new(0, 0));
                } else {
                    self.selection.extend_to(CellCoord::new(0, 0));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::CtrlEnd => {
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(self.max_row, self.max_col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(self.max_row, self.max_col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::PageUp => (-20, 0),
            NavigationKey::PageDown => (20, 0),
            NavigationKey::CtrlUp => {
                // Jump to top of data region or row 0
                let new_row = 0;
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(new_row, self.selection.active.col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(new_row, self.selection.active.col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::CtrlDown => {
                let new_row = self.max_row;
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(new_row, self.selection.active.col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(new_row, self.selection.active.col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::CtrlLeft => {
                let new_col = 0;
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(self.selection.active.row, new_col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(self.selection.active.row, new_col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
            NavigationKey::CtrlRight => {
                let new_col = self.max_col;
                if !shift {
                    self.selection
                        .move_to(CellCoord::new(self.selection.active.row, new_col));
                } else {
                    self.selection
                        .extend_to(CellCoord::new(self.selection.active.row, new_col));
                }
                self.scroll
                    .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
                return;
            }
        };

        self.selection
            .move_by(row_delta, col_delta, shift, self.max_row, self.max_col);
        self.scroll
            .scroll_to_cell(self.selection.active, &self.grid_config, viewport_size);
    }

    /// Start editing the current cell - initiates TransitionToEdit state
    fn start_editing(&mut self, initial_char: Option<char>) {
        self.input_mode = InputMode::TransitionToEdit { initial_char };
    }

    /// Returns true if currently in editing mode
    fn is_editing(&self) -> bool {
        matches!(
            self.input_mode,
            InputMode::Editing { .. } | InputMode::TransitionToEdit { .. }
        )
    }

    /// Get the cell being edited (if any)
    fn editing_cell(&self) -> Option<CellCoord> {
        match self.input_mode {
            InputMode::Editing { cell } => Some(cell),
            _ => None,
        }
    }

    /// Confirm the current edit
    fn confirm_edit(&mut self, move_down: bool, move_right: bool) {
        // Use editing_cell if in Editing state, otherwise fall back to selection.active
        let coord = self.editing_cell().unwrap_or(self.selection.active);
        let content = self.edit_buffer.clone();
        self.set_cell_content(coord, &content);
        self.input_mode = InputMode::Navigation;
        self.edit_buffer.clear();
        self.formula_bar.editing = false;

        if move_down {
            self.selection
                .move_by(1, 0, false, self.max_row, self.max_col);
        } else if move_right {
            self.selection
                .move_by(0, 1, false, self.max_row, self.max_col);
        }
    }

    /// Cancel the current edit
    fn cancel_edit(&mut self) {
        self.input_mode = InputMode::Navigation;
        self.edit_buffer.clear();
        self.formula_bar.editing = false;
    }

    /// Set a status message
    fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), std::time::Instant::now()));
    }

    /// Save current sheet's state before switching
    fn save_current_sheet_state(&mut self) {
        if let Some(state) = self.sheet_states.get_mut(self.current_sheet as usize) {
            state.selection = self.selection.clone();
            state.scroll = self.scroll.clone();
        }
    }

    /// Load a sheet's state after switching
    fn load_sheet_state(&mut self, sheet_index: u32) {
        if let Some(state) = self.sheet_states.get(sheet_index as usize) {
            self.selection = state.selection.clone();
            self.scroll = state.scroll.clone();
        }
    }

    /// Switch to a different sheet
    fn switch_sheet(&mut self, sheet_index: u32) {
        if sheet_index as usize >= self.sheet_names.len() {
            return;
        }
        if sheet_index == self.current_sheet {
            return;
        }

        // Cancel any active editing
        if self.is_editing() {
            self.cancel_edit();
        }

        // Save current state
        self.save_current_sheet_state();

        // Switch
        self.current_sheet = sheet_index;

        // Load new state
        self.load_sheet_state(sheet_index);
    }

    /// Add a new sheet with a unique name
    fn add_sheet(&mut self) {
        // Generate unique name
        let mut counter = self.sheet_names.len() + 1;
        let mut name = format!("Sheet{}", counter);
        while self.sheet_names.contains(&name) {
            counter += 1;
            name = format!("Sheet{}", counter);
        }

        self.sheet_names.push(name.clone());
        self.sheet_states.push(SheetState {
            selection: Selection::default(),
            scroll: ScrollState::default(),
        });
        self.engine.set_sheet_names(self.sheet_names.clone());

        // Switch to the new sheet
        let new_index = (self.sheet_names.len() - 1) as u32;
        self.switch_sheet(new_index);
        self.modified = true;
        self.set_status(&format!("Added {}", name));
    }

    /// Delete a sheet by index (keeps at least one sheet)
    fn delete_sheet(&mut self, sheet_index: u32) {
        if self.sheet_names.len() <= 1 {
            self.set_status("Cannot delete the last sheet");
            return;
        }

        let index = sheet_index as usize;
        if index >= self.sheet_names.len() {
            return;
        }

        let name = self.sheet_names[index].clone();

        self.engine.remove_sheet_and_shift(sheet_index);
        self.chart_windows.remove_sheet_and_shift(sheet_index);
        self.sheet_names.remove(index);
        self.sheet_states.remove(index);
        self.engine.set_sheet_names(self.sheet_names.clone());

        // Adjust current sheet index if needed
        if self.current_sheet as usize >= self.sheet_names.len() {
            self.current_sheet = (self.sheet_names.len() - 1) as u32;
        } else if self.current_sheet > sheet_index {
            self.current_sheet -= 1;
        }

        // Reload state for current sheet
        self.load_sheet_state(self.current_sheet);
        self.modified = true;
        self.set_status(&format!("Deleted {}", name));
    }

    /// Rename a sheet
    fn rename_sheet(&mut self, sheet_index: u32, new_name: String) {
        let index = sheet_index as usize;
        if index >= self.sheet_names.len() {
            return;
        }
        if new_name.is_empty() {
            self.set_status("Sheet name cannot be empty");
            return;
        }
        // Check for duplicate names (excluding current)
        for (i, name) in self.sheet_names.iter().enumerate() {
            if i != index && name == &new_name {
                self.set_status("Sheet name already exists");
                return;
            }
        }
        let old_name = self.sheet_names[index].clone();
        self.engine.rewrite_sheet_name(&old_name, &new_name);
        self.sheet_names[index] = new_name;
        self.engine.set_sheet_names(self.sheet_names.clone());
        self.modified = true;
    }

    /// Create a new empty workbook
    fn new_workbook(&mut self) {
        self.engine = CalcEngine::new();
        self.string_pool = StringPool::new();
        self.current_sheet = 0;
        self.sheet_names = vec!["Sheet1".to_string()];
        self.engine.set_sheet_names(self.sheet_names.clone());
        self.chart_windows.clear();
        self.chart_data_resolver.invalidate_all();
        self.sheet_states = vec![SheetState {
            selection: Selection::default(),
            scroll: ScrollState::default(),
        }];
        self.selection = Selection::default();
        self.scroll = ScrollState::default();
        self.input_mode = InputMode::Navigation;
        self.edit_buffer.clear();
        self.formula_bar.editing = false;
        self.current_file = None;
        self.modified = false;
        self.undo_history.clear();
        self.set_status("New workbook created");
    }

    fn extension_is(path: &PathBuf, ext: &str) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Open file dialog and load workbook
    fn open_file(&mut self) {
        use rfd::FileDialog;

        let mut dialog = FileDialog::new();
        #[cfg(feature = "csv")]
        {
            dialog = dialog.add_filter("CSV", &["csv"]);
        }
        #[cfg(feature = "xlsx")]
        {
            dialog = dialog.add_filter("Excel Files", &["xlsx", "xls"]);
        }
        let file = dialog.add_filter("All Files", &["*"]).pick_file();

        if let Some(path) = file {
            self.load_file(&path);
        }
    }

    fn load_file(&mut self, path: &PathBuf) {
        #[cfg(feature = "csv")]
        if Self::extension_is(path, "csv") {
            self.load_csv(path);
            return;
        }
        #[cfg(feature = "xlsx")]
        {
            self.load_xlsx(path);
            return;
        }
        #[cfg(not(feature = "xlsx"))]
        self.set_status("Excel support not enabled. Rebuild with --features xlsx");
    }

    fn finish_open(&mut self, path: &PathBuf) {
        self.current_sheet = 0;
        self.current_file = Some(path.clone());
        self.modified = false;
        self.selection = Selection::default();
        self.scroll = ScrollState::default();
        self.input_mode = InputMode::Navigation;
        self.formula_bar.editing = false;
        self.undo_history.clear();
        let sheet_count = self.sheet_names.len();
        self.set_status(&format!(
            "Opened: {} ({} sheet{})",
            path.display(),
            sheet_count,
            if sheet_count == 1 { "" } else { "s" }
        ));
    }

    #[cfg(feature = "csv")]
    fn load_csv(&mut self, path: &PathBuf) {
        let mut loaded = CalcEngine::new();
        match crate::csv_io::read_path(&mut loaded, 0, path) {
            Ok(()) => {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Sheet1")
                    .to_string();
                self.engine = loaded;
                self.string_pool = StringPool::new();
                self.sheet_names = vec![name];
                self.engine.set_sheet_names(self.sheet_names.clone());
                self.chart_windows.clear();
                self.chart_data_resolver.invalidate_all();
                self.sheet_states = vec![SheetState {
                    selection: Selection::default(),
                    scroll: ScrollState::default(),
                }];
                self.finish_open(path);
            }
            Err(e) => self.set_status(&format!("Failed to open CSV: {e}")),
        }
    }

    #[cfg(feature = "xlsx")]
    fn load_xlsx(&mut self, path: &PathBuf) {
        match XlsxReader::open(path) {
            Ok(mut reader) => {
                let sheet_names = reader.sheet_names();
                if sheet_names.is_empty() {
                    self.set_status("Workbook has no sheets");
                    return;
                }

                self.engine = CalcEngine::new();
                self.string_pool = StringPool::new();
                self.sheet_names = sheet_names.clone();
                self.engine.set_sheet_names(self.sheet_names.clone());
                self.sheet_states = self
                    .sheet_names
                    .iter()
                    .map(|_| SheetState {
                        selection: Selection::default(),
                        scroll: ScrollState::default(),
                    })
                    .collect();

                for (sheet_index, sheet_name) in sheet_names.iter().enumerate() {
                    if let Err(e) =
                        reader.read_into_engine(sheet_name, &mut self.engine, sheet_index as u32)
                    {
                        self.set_status(&format!("Error reading sheet '{sheet_name}': {e:?}"));
                    }
                }

                if self.sheet_names.is_empty() {
                    self.sheet_names.push("Sheet1".to_string());
                    self.engine.set_sheet_names(self.sheet_names.clone());
                    self.sheet_states.push(SheetState {
                        selection: Selection::default(),
                        scroll: ScrollState::default(),
                    });
                }

                self.chart_windows.clear();
                self.chart_data_resolver.invalidate_all();
                if let Ok(charts) = crate::xlsx::ChartReader::read_charts(path) {
                    for (_sheet, chart) in charts {
                        let id = chart.id;
                        self.chart_windows.add_chart(chart);
                        self.update_chart_data(id);
                    }
                }

                self.finish_open(path);
            }
            Err(e) => self.set_status(&format!("Failed to open file: {e:?}")),
        }
    }

    fn save_file(&mut self) {
        if let Some(path) = self.current_file.clone() {
            self.save_to_path(&path);
        } else {
            self.save_file_as();
        }
    }

    fn save_file_as(&mut self) {
        use rfd::FileDialog;

        let mut dialog = FileDialog::new();
        #[cfg(feature = "csv")]
        {
            dialog = dialog.add_filter("CSV", &["csv"]);
        }
        #[cfg(feature = "xlsx")]
        {
            dialog = dialog.add_filter("Excel Files", &["xlsx"]);
        }
        let default_name = if cfg!(feature = "xlsx") {
            "workbook.xlsx"
        } else {
            "workbook.csv"
        };
        let file = dialog.set_file_name(default_name).save_file();

        if let Some(path) = file {
            self.save_to_path(&path);
        }
    }

    fn save_to_path(&mut self, path: &PathBuf) {
        #[cfg(feature = "csv")]
        if Self::extension_is(path, "csv") {
            self.save_csv(path);
            return;
        }
        #[cfg(feature = "xlsx")]
        {
            self.save_xlsx(path);
            return;
        }
        #[cfg(not(feature = "xlsx"))]
        self.set_status("Excel support not enabled. Rebuild with --features xlsx");
    }

    #[cfg(feature = "csv")]
    fn save_csv(&mut self, path: &PathBuf) {
        match crate::csv_io::write_path(&self.engine, self.current_sheet, path) {
            Ok(()) => {
                self.current_file = Some(path.clone());
                self.modified = false;
                let extra = if self.sheet_names.len() > 1 {
                    " (current sheet only)"
                } else {
                    ""
                };
                self.set_status(&format!("Saved: {}{extra}", path.display()));
            }
            Err(e) => self.set_status(&format!("Failed to save CSV: {e}")),
        }
    }

    #[cfg(feature = "xlsx")]
    fn save_xlsx(&mut self, path: &PathBuf) {
        let mut writer = XlsxWriter::new();

        let charts = self.chart_windows.all_charts();
        for (sheet_index, sheet_name) in self.sheet_names.iter().enumerate() {
            if let Err(e) = writer.add_engine_sheet_with_charts(
                sheet_name,
                &self.engine,
                sheet_index as u32,
                &charts,
            ) {
                self.set_status(&format!("Error creating sheet '{sheet_name}': {e:?}"));
                return;
            }
        }

        match writer.save_with_charts(path, &charts) {
            Ok(()) => {
                self.current_file = Some(path.clone());
                self.modified = false;
                let sheet_count = self.sheet_names.len();
                self.set_status(&format!(
                    "Saved: {} ({} sheet{})",
                    path.display(),
                    sheet_count,
                    if sheet_count == 1 { "" } else { "s" }
                ));
            }
            Err(e) => self.set_status(&format!("Failed to save: {e:?}")),
        }
    }

    /// Add a new chart
    fn add_chart(&mut self, chart: ChartDefinition) {
        let id = chart.id;
        self.chart_windows.add_chart(chart.clone());
        self.update_chart_data(id);
        self.modified = true;
        self.set_status("Chart added");
    }

    /// Update a chart
    fn update_chart(&mut self, chart: ChartDefinition) {
        let id = chart.id;
        if let Some(window) = self.chart_windows.get_chart_mut(id) {
            window.chart = chart;
            self.update_chart_data(id);
        }
        self.modified = true;
        self.set_status("Chart updated");
    }

    /// Remove a chart
    fn remove_chart(&mut self, id: ChartId) {
        self.chart_windows.remove_chart(id);
        self.chart_data_resolver.invalidate_all();
        self.modified = true;
        self.set_status("Chart removed");
    }

    /// Update chart data from spreadsheet cells
    fn update_chart_data(&mut self, id: ChartId) {
        if let Some(window) = self.chart_windows.get_chart(id) {
            let chart = window.chart.clone();
            let data = self
                .chart_data_resolver
                .get_chart_data(&chart, &self.engine);
            if let Some(window) = self.chart_windows.get_chart_mut(id) {
                window.set_data(data);
            }
        }
    }

    /// Refresh all chart data (called after cell edits)
    fn refresh_all_charts(&mut self) {
        // Mark resolver as needing refresh
        self.chart_data_resolver.invalidate_all();

        // Update all charts
        let ids: Vec<ChartId> = self.chart_windows.chart_ids();
        for id in ids {
            self.update_chart_data(id);
        }
    }

    /// Open the chart editor for a new chart
    fn open_new_chart_editor(&mut self) {
        // Use current selection as the default data range
        let selection_range = self.selection.primary_range();
        if selection_range.width() > 1 || selection_range.height() > 1 {
            // Multi-cell selection - use it as the data range
            self.chart_editor
                .open_new_with_selection(self.current_sheet, &selection_range);
        } else {
            // Single cell - open without pre-filled range
            self.chart_editor.open_new(self.current_sheet);
        }
    }

    /// Open the chart editor to edit an existing chart
    fn open_edit_chart_editor(&mut self, id: ChartId) {
        if let Some(window) = self.chart_windows.get_chart(id) {
            self.chart_editor.open_edit(&window.chart);
        }
    }
}

impl eframe::App for SpreadsheetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ======================================================================
        // 1. STATE TRANSITION HANDLING (Pre-Render)
        // Process InputMode transitions BEFORE any UI rendering to ensure
        // focus is set correctly for the current frame
        // ======================================================================
        if let InputMode::TransitionToEdit { initial_char } = self.input_mode.clone() {
            let active_cell = self.selection.active;

            // Initialize edit buffer
            let initial_text = if let Some(c) = initial_char {
                c.to_string()
            } else {
                self.get_cell_formula_or_value(active_cell)
            };

            self.edit_buffer = initial_text.clone();
            self.formula_bar.content = initial_text;
            self.formula_bar.start_editing();

            // FORCE FOCUS immediately for this frame
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("formula_bar_editor")));

            // Transition to Editing state
            self.input_mode = InputMode::Editing { cell: active_cell };
        }

        // Update formula bar with current cell info
        self.formula_bar.set_cell(self.selection.active);
        if !self.is_editing() {
            self.formula_bar
                .set_content(self.get_cell_formula_or_value(self.selection.active));
        }

        // Track if formula bar has focus
        let mut formula_bar_has_focus = false;

        // Top panel for toolbar
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New (Cmd+N)").clicked() {
                        self.new_workbook();
                        ui.close_menu();
                    }
                    if ui.button("Open (Cmd+O)").clicked() {
                        self.open_file();
                        ui.close_menu();
                    }
                    if ui.button("Save (Cmd+S)").clicked() {
                        self.save_file();
                        ui.close_menu();
                    }
                    if ui.button("Save As...").clicked() {
                        self.save_file_as();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let can_undo = self.undo_history.can_undo();
                    let can_redo = self.undo_history.can_redo();

                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo (Cmd+Z)"))
                        .clicked()
                    {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo (Cmd+Shift+Z)"))
                        .clicked()
                    {
                        self.redo();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Insert", |ui| {
                    if ui.button("Chart...").clicked() {
                        self.open_new_chart_editor();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Light Theme").clicked() {
                        self.theme = Theme::light();
                        ui.close_menu();
                    }
                    if ui.button("Dark Theme").clicked() {
                        self.theme = Theme::dark();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Help (F1)").clicked() {
                        self.help_panel.toggle();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("About").clicked() {
                        self.help_panel.visible = true;
                        self.help_panel.tab = super::help_panel::HelpTab::About;
                        ui.close_menu();
                    }
                });

                // Show modified indicator and filename
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(path) = &self.current_file {
                        if let Some(name) = path.file_name() {
                            let display = if self.modified {
                                format!("{}*", name.to_string_lossy())
                            } else {
                                name.to_string_lossy().to_string()
                            };
                            ui.label(display);
                        }
                    } else if self.modified {
                        ui.label("Untitled*");
                    }
                });
            });
        });

        // Formula bar panel
        let mut formula_response = None;
        TopBottomPanel::top("formula_bar").show(ctx, |ui| {
            let response = self.formula_bar.show(ui, &self.theme);
            formula_bar_has_focus = response.has_focus;
            formula_response = Some(response);
        });

        // Handle formula bar response (keys are now handled inside show())
        if let Some(response) = formula_response {
            if response.open_help {
                self.help_panel.visible = true;
                self.help_panel.tab = super::help_panel::HelpTab::Functions;
            }

            if let Some(content) = response.confirmed {
                self.edit_buffer = content;
                self.confirm_edit(response.move_down, response.move_right);
                // Give focus back to grid after confirming edit
                let grid_id = egui::Id::new("spreadsheet_grid");
                ctx.memory_mut(|m| m.request_focus(grid_id));
            }

            if response.cancelled {
                self.cancel_edit();
                // Give focus back to grid after cancelling edit
                let grid_id = egui::Id::new("spreadsheet_grid");
                ctx.memory_mut(|m| m.request_focus(grid_id));
            }
        }

        // Help panel
        self.help_panel.show(ctx);

        // Chart windows
        let chart_response = self.chart_windows.show(ctx);

        // Handle chart window actions
        if let Some(edit_id) = chart_response.edit_requested {
            self.open_edit_chart_editor(edit_id);
        }

        // Chart editor dialog
        let editor_response = self.chart_editor.show(ctx);
        if let Some(chart) = editor_response.chart {
            if editor_response.is_edit {
                self.update_chart(chart);
            } else {
                self.add_chart(chart);
            }
        }

        // Status bar at bottom
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Show status message if recent (within 5 seconds)
                if let Some((msg, time)) = &self.status_message {
                    if time.elapsed().as_secs() < 5 {
                        ui.label(msg);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Show current cell info
                    let coord = self.selection.active;
                    let value = self.engine.get_value(self.current_sheet, coord);
                    let type_str = match value {
                        crate::calc::CellResult::Empty => "Empty",
                        crate::calc::CellResult::Value(_) => "Number",
                        crate::calc::CellResult::Text(_) => "Text",
                        crate::calc::CellResult::Bool(_) => "Boolean",
                        crate::calc::CellResult::Error(_) => "Error",
                    };
                    ui.label(format!("Cell: {} | Type: {}", coord.to_a1(), type_str));
                });
            });
        });

        // Sheet tabs panel (above status bar)
        let mut sheet_action: Option<SheetAction> = None;
        TopBottomPanel::bottom("sheet_tabs").show(ctx, |ui| {
            let tabs = SheetTabs::new(&self.sheet_names, self.current_sheet, &self.theme);
            let response = tabs.show(ui);

            if let Some(index) = response.switch_to {
                sheet_action = Some(SheetAction::Switch(index));
            }
            if response.add_sheet {
                sheet_action = Some(SheetAction::Add);
            }
            if let Some(index) = response.delete_sheet {
                sheet_action = Some(SheetAction::Delete(index));
            }
            if let Some((index, name)) = response.rename_sheet {
                sheet_action = Some(SheetAction::Rename(index, name));
            }
        });

        // Handle sheet actions (outside of the borrow)
        match sheet_action {
            Some(SheetAction::Switch(index)) => self.switch_sheet(index),
            Some(SheetAction::Add) => self.add_sheet(),
            Some(SheetAction::Delete(index)) => self.delete_sheet(index),
            Some(SheetAction::Rename(index, name)) => self.rename_sheet(index, name),
            None => {}
        }

        // Global keyboard shortcuts (only when not editing in formula bar)
        let shift = ctx.input(|i| i.modifiers.shift);

        if !formula_bar_has_focus {
            // F1 for help
            if ctx.input(|i| i.key_pressed(Key::F1)) {
                self.help_panel.toggle();
            }

            // Escape to close help
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                if self.help_panel.visible {
                    self.help_panel.visible = false;
                }
            }

            // Delete to clear cell (with undo support)
            if ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
                && !self.is_editing()
            {
                let coord = self.selection.active;
                self.set_cell_content(coord, "");
            }
        }

        // Cmd+key shortcuts work globally
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::S)) {
            self.save_file();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::O)) {
            self.open_file();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::N)) {
            self.new_workbook();
        }
        // Undo: Cmd+Z
        if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z)) {
            self.undo();
        }
        // Redo: Cmd+Shift+Z or Cmd+Y
        if ctx.input(|i| {
            i.modifiers.command
                && (i.modifiers.shift && i.key_pressed(Key::Z) || i.key_pressed(Key::Y))
        }) {
            self.redo();
        }

        // Main grid area
        CentralPanel::default().show(ctx, |ui| {
            let viewport_size = ui.available_size();

            let grid = SpreadsheetGrid::new(
                self.current_sheet,
                &self.engine,
                &self.selection,
                &self.grid_config,
                &self.scroll,
                &self.theme,
            );

            let grid_response = grid.show(ui);

            // Handle drag for multi-cell selection
            if let Some(coord) = grid_response.drag_started {
                if self.is_editing() {
                    // Confirm current edit before starting drag selection
                    self.edit_buffer = self.formula_bar.content.clone();
                    self.confirm_edit(false, false);
                }
                self.selection.move_to(coord);
                // Request focus on drag start
                let grid_id = egui::Id::new("spreadsheet_grid");
                ctx.memory_mut(|m| m.request_focus(grid_id));
            }

            if let Some(coord) = grid_response.drag_to {
                // Extend selection while dragging
                self.selection.extend_to(coord);
                self.scroll
                    .scroll_to_cell(coord, &self.grid_config, viewport_size);
            }

            // Handle grid clicks (non-drag single click)
            if let Some(coord) = grid_response.clicked_cell {
                if self.is_editing() {
                    // Confirm current edit before moving
                    self.edit_buffer = self.formula_bar.content.clone();
                    self.confirm_edit(false, false);
                }
                self.selection.move_to(coord);
                self.scroll
                    .scroll_to_cell(coord, &self.grid_config, viewport_size);
                // Request focus on click
                let grid_id = egui::Id::new("spreadsheet_grid");
                ctx.memory_mut(|m| m.request_focus(grid_id));
            }

            // Handle double-click for editing
            if let Some(coord) = grid_response.double_clicked_cell {
                self.selection.move_to(coord);
                // Start editing - FSM will handle focus in next pre-render
                self.start_editing(None);
            }

            // Handle F2/Enter to edit (only when formula bar doesn't have focus)
            if !formula_bar_has_focus {
                if grid_response.edit_cell.is_some() {
                    if !self.is_editing() {
                        // Start editing - FSM will handle focus in next pre-render
                        self.start_editing(None);
                    }
                }

                // Handle direct text input (start editing with that character)
                if let Some(c) = grid_response.text_input_char {
                    if !self.is_editing() {
                        // Start editing with the typed character - FSM handles focus
                        self.start_editing(Some(c));
                    }
                }
            }

            // Handle navigation (only when formula bar doesn't have focus and not editing)
            if !formula_bar_has_focus && !self.is_editing() {
                if let Some(nav) = grid_response.navigation {
                    self.handle_navigation(nav, shift, viewport_size);
                    // Keep focus on grid
                    let grid_id = egui::Id::new("spreadsheet_grid");
                    ctx.memory_mut(|m| m.request_focus(grid_id));
                }
            }
        });

        // Handle scroll with mouse wheel
        ctx.input(|i| {
            let scroll_delta = i.raw_scroll_delta;
            if scroll_delta != Vec2::ZERO {
                self.scroll.offset_x = (self.scroll.offset_x - scroll_delta.x).max(0.0);
                self.scroll.offset_y = (self.scroll.offset_y - scroll_delta.y).max(0.0);
                self.scroll.first_visible_col = self.grid_config.column_at_x(self.scroll.offset_x);
                self.scroll.first_visible_row = self.grid_config.row_at_y(self.scroll.offset_y);
            }
        });
    }
}

/// Stored formulas already include `=`. Do not prefix another one.
fn formula_bar_text(stored: &str) -> String {
    if stored.starts_with('=') {
        stored.to_string()
    } else {
        format!("={stored}")
    }
}

/// Run the application (native only)
#[cfg(not(target_arch = "wasm32"))]
pub fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("RustSheet"),
        ..Default::default()
    };

    eframe::run_native(
        "RustSheet",
        options,
        Box::new(|_cc| Ok(Box::new(SpreadsheetApp::new()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::formula::FormulaParser;

    #[test]
    fn displayed_formula_reparses() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        let coord = CellCoord::from_a1("A3").unwrap();
        app.set_cell_content(coord, "=SUM(A1:A2)");

        let displayed = app.get_cell_formula_or_value(coord);
        assert_eq!(displayed, "=SUM(A1:A2)");
        FormulaParser::new()
            .parse(&displayed)
            .expect("formula bar text must re-parse");

        let snapshot = app
            .get_cell_content_string(coord)
            .expect("formula snapshot");
        assert_eq!(snapshot, "=SUM(A1:A2)");
        FormulaParser::new()
            .parse(&snapshot)
            .expect("undo snapshot must re-parse");
    }

    #[test]
    fn formula_bar_text_does_not_double_equals() {
        assert_eq!(formula_bar_text("=SUM(A1:A2)"), "=SUM(A1:A2)");
        assert_eq!(formula_bar_text("SUM(A1:A2)"), "=SUM(A1:A2)");
        FormulaParser::new()
            .parse(&formula_bar_text("=A1*2"))
            .unwrap();
    }

    #[test]
    #[cfg(feature = "xlsx")]
    fn save_load_preserves_formula_string() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        let a1 = CellCoord::from_a1("A1").unwrap();
        let a2 = CellCoord::from_a1("A2").unwrap();
        let a3 = CellCoord::from_a1("A3").unwrap();
        app.engine.set_value(0, a1, CellValueInput::Number(1.0));
        app.engine.set_value(0, a2, CellValueInput::Number(2.0));
        app.engine.set_formula(0, a3, "=SUM(A1:A2)").unwrap();

        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustsheet_gui_f1_{}_{}.xlsx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        app.save_to_path(&path);
        app.load_file(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            app.engine.get_formula(0, a3).as_deref(),
            Some("=SUM(A1:A2)")
        );
        assert_eq!(app.engine.get_value(0, a3), CellResult::Value(3.0));
    }

    #[test]
    #[cfg(feature = "csv")]
    fn save_load_csv_preserves_formula_string() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        let a1 = CellCoord::from_a1("A1").unwrap();
        let a2 = CellCoord::from_a1("A2").unwrap();
        let a3 = CellCoord::from_a1("A3").unwrap();
        app.engine.set_value(0, a1, CellValueInput::Number(1.0));
        app.engine.set_value(0, a2, CellValueInput::Number(2.0));
        app.engine.set_formula(0, a3, "=SUM(A1:A2)").unwrap();

        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustsheet_gui_csv_{}_{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        app.save_to_path(&path);
        app.load_file(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            app.engine.get_formula(0, a3).as_deref(),
            Some("=SUM(A1:A2)")
        );
        assert_eq!(app.engine.get_value(0, a3), CellResult::Value(3.0));
    }

    /// F2: deleting sheet 0 must not leave the remaining tab pointing at sheet 0's cells.
    #[test]
    fn delete_sheet_zero_preserves_remaining_sheet_cells() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        let a1 = CellCoord::new(0, 0);
        app.engine.set_value(0, a1, CellValueInput::Number(1.0));
        app.add_sheet();
        app.engine.set_value(1, a1, CellValueInput::Number(2.0));
        app.delete_sheet(0);

        assert_eq!(app.sheet_names.len(), 1);
        assert_eq!(
            app.engine.get_value(app.current_sheet, a1),
            CellResult::Value(2.0),
            "remaining tab must still show the surviving sheet's A1"
        );
    }
}
