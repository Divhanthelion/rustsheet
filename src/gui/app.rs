//! Main spreadsheet application

use crate::calc::{CalcEngine, CellResult, CellValueInput};
use crate::cell::{CellCoord, CellRange, StringPool};
use crate::chart::{ChartDataResolver, ChartDefinition, ChartId};
use eframe::egui::{self, CentralPanel, Key, TopBottomPanel, Vec2};
use std::path::{Path, PathBuf};

use super::chart_editor::ChartEditor;
use super::chart_widget::ChartWindowManager;
use super::format_bar::{self, BorderPreset, FormatAction};
use super::formula_bar::FormulaBar;
use super::grid::{
    DEFAULT_COLUMN_WIDTH, DEFAULT_ROW_HEIGHT, GridConfig, HEADER_HEIGHT, HEADER_WIDTH,
    NavigationKey, ResizeAxis, ScrollState, SpreadsheetGrid, display_text, fit_column_width,
};
use super::help_panel::HelpPanel;
use super::selection::Selection;
use super::sheet_tabs::SheetTabs;
use super::theme::Theme;

mod commands;
mod conditional_ui;
#[cfg(test)]
mod feature_tests;
mod fill;
mod find;
mod notes;
mod pictures;
mod pivot_ui;
mod printing;
mod recovery;
mod sheet_ops;
mod validation_ui;

use super::fonts::FontLibrary;
use super::grid::fit_row_height;
use super::settings::{Settings, ThemeChoice};
use crate::format::{
    Borders, CellFormat, format_general, format_number, is_date_format, parse_typed_number,
};
use crate::formula::FormulaParser;
use commands::Command;
use conditional_ui::CfDialog;
use eframe::egui::RichText;
use find::FindDialog;
use notes::NoteEditor;
use pictures::{AltTextEditor, PictureTextures};
use pivot_ui::PivotDialog;
use printing::PrintDialog;
use recovery::{Recoverable, Recovery};
use sheet_ops::{FilterPopup, SortDialog, WorkbookState};
use std::collections::{HashMap, HashSet};
use validation_ui::{ListPopup, ValidationDialog};

/// Modifier key name shown in menu shortcut hints.
const MOD: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

/// Input mode FSM - decouples input handling from render order
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Standard navigation mode; Grid captures focus
    #[default]
    Navigation,
    /// Transition frame; Focus is being transferred to editor
    TransitionToEdit { initial_char: Option<char> },
    /// Editing mode; FormulaBar captures focus
    Editing { cell: CellCoord },
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
    /// Formats of several cells: (coord, old, new)
    Format {
        sheet: u32,
        changes: Vec<(CellCoord, Option<CellFormat>, Option<CellFormat>)>,
    },
    /// A column width or row height; `None` is the default size
    Resize {
        sheet: u32,
        axis: ResizeAxis,
        index: u32,
        old: Option<f32>,
        new: Option<f32>,
    },
    /// Several actions undone and redone together
    Group(Vec<UndoAction>),
    /// Whole-row/column formats: (index, old, new)
    LineFormat {
        sheet: u32,
        axis: ResizeAxis,
        changes: Vec<(u32, Option<CellFormat>, Option<CellFormat>)>,
    },
    /// The whole workbook before and after a structural edit
    Snapshot(Box<(WorkbookState, WorkbookState)>),
    /// A cell's note
    Note {
        sheet: u32,
        coord: CellCoord,
        old: Option<crate::format::Note>,
        new: Option<crate::format::Note>,
    },
}

/// Cells copied or cut inside RustSheet. When the system clipboard still
/// holds `text`, pasting uses these (formulas and formats); any other
/// clipboard text is pasted as values.
struct ClipboardCells {
    sheet: u32,
    origin: CellCoord,
    rows: u32,
    cols: u32,
    /// By (row, col) offset from `origin`: content as typed, and format
    cells: HashMap<(u32, u32), (Option<String>, Option<CellFormat>)>,
    /// Notes, by the same offsets
    notes: HashMap<(u32, u32), crate::format::Note>,
    text: String,
    /// Cut cells move on paste instead of being copied
    cut: bool,
}

/// One clipboard field: quoted like Excel when it holds a tab, newline or quote.
fn tsv_field(s: &str) -> String {
    if s.contains(['\t', '\n', '\r', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Parse tab-separated clipboard text (Excel, Sheets, etc.) into rows.
fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let mut rows = vec![Vec::new()];
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut at_field_start = true;
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if at_field_start => in_quotes = true,
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '\t' if !in_quotes => {
                rows.last_mut().unwrap().push(std::mem::take(&mut field));
                at_field_start = true;
                continue;
            }
            '\n' if !in_quotes => {
                rows.last_mut().unwrap().push(std::mem::take(&mut field));
                rows.push(Vec::new());
                at_field_start = true;
                continue;
            }
            _ => field.push(c),
        }
        at_field_start = false;
    }
    rows.last_mut().unwrap().push(field);
    rows
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
    /// Where a header click started, for Shift-click and drag
    header_anchor: Option<(ResizeAxis, u32)>,
    /// AutoFilter menu, when open
    filter_popup: Option<FilterPopup>,
    /// Data > Sort dialog, when open
    sort_dialog: Option<SortDialog>,
    /// Find and Replace, when open
    find_dialog: Option<FindDialog>,
    /// Note editor, when open
    note_editor: Option<NoteEditor>,
    /// Data Validation dialog, when open
    validation_dialog: Option<ValidationDialog>,
    /// Conditional Formatting dialog, when open
    cf_dialog: Option<CfDialog>,
    /// A validation list's drop-down, when open
    list_popup: Option<ListPopup>,
    /// Text to put back in the editor after a rejected entry
    retry_text: Option<String>,
    /// Decoded pictures for the grid
    picture_textures: PictureTextures,
    /// The selected picture on the current sheet, by index
    selected_picture: Option<usize>,
    /// Alt text editor, when open
    alt_text_editor: Option<AltTextEditor>,
    /// PivotTable dialog, when open
    pivot_dialog: Option<PivotDialog>,
    /// Ctrl+V was down last frame (for pasting images; see `handle_clipboard_events`)
    paste_key_down: bool,
    /// Tests answer validation alerts without a dialog
    #[cfg(test)]
    alert_answer: Option<validation_ui::AlertAnswer>,
    /// Export/Print options, when open
    print_dialog: Option<PrintDialog>,
    /// Page setup from the last export or print
    last_page_setup: super::print::PageSetup,
    /// Where the fill handle is being dragged
    fill_target: Option<CellCoord>,
    /// Size of the grid's area in the last frame
    last_viewport: Vec2,
    /// Theme and recent files
    settings: Settings,
    /// Installed fonts, for cells that name one
    fonts: FontLibrary,
    /// Which theme is applied (true = dark)
    dark_mode: Option<bool>,
    /// This session's autosave
    recovery: Option<Recovery>,
    /// Autosaves left by sessions that crashed
    recoverable: Vec<Recoverable>,
    /// Save As suggests this name (after recovering an unsaved file)
    suggested_name: Option<String>,
    /// The file recovered work came from
    recovered_from: Option<PathBuf>,
    /// Current file path (if saved)
    current_file: Option<PathBuf>,
    /// Whether the document has unsaved changes
    modified: bool,
    /// Last title sent to the window
    window_title: String,
    /// Cells last copied or cut in this app
    clipboard: Option<ClipboardCells>,
    /// While true, chart refreshes wait until the batch ends
    batching: bool,
    charts_dirty: bool,
    /// Size of the column or row being resized, before the drag began
    resize_origin: Option<(ResizeAxis, u32, Option<f32>)>,
    /// Screen position of cell A1's top-left corner, from the last frame
    grid_origin: Option<egui::Pos2>,
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
            header_anchor: None,
            filter_popup: None,
            sort_dialog: None,
            find_dialog: None,
            note_editor: None,
            validation_dialog: None,
            cf_dialog: None,
            list_popup: None,
            retry_text: None,
            picture_textures: PictureTextures::default(),
            selected_picture: None,
            alt_text_editor: None,
            pivot_dialog: None,
            paste_key_down: false,
            #[cfg(test)]
            alert_answer: None,
            print_dialog: None,
            last_page_setup: Default::default(),
            fill_target: None,
            last_viewport: Vec2::new(1200.0, 800.0),
            settings: Settings::in_memory(),
            fonts: FontLibrary::empty(),
            dark_mode: None,
            recovery: None,
            recoverable: Vec::new(),
            suggested_name: None,
            recovered_from: None,
            current_file: None,
            modified: false,
            window_title: String::new(),
            clipboard: None,
            batching: false,
            charts_dirty: false,
            resize_origin: None,
            grid_origin: None,
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

        // Dates and percents edit as typed (2026-10-03, 12%), not as raw numbers.
        if let (CellResult::Value(n), Some(code)) = (
            self.engine.get_value(self.current_sheet, coord),
            self.effective_format(self.current_sheet, coord)
                .and_then(|f| f.number_format.as_deref()),
        ) {
            if is_date_format(code) {
                let editable = match (n.fract() == 0.0, n < 1.0) {
                    (true, _) => "yyyy-mm-dd",
                    (false, true) => "h:mm:ss",
                    (false, false) => "yyyy-mm-dd h:mm:ss",
                };
                return format_number(n, editable).text;
            }
            if code
                .split('"')
                .step_by(2)
                .any(|unquoted| unquoted.contains('%'))
            {
                return format!("{}%", format_general(n * 100.0, 15));
            }
        }

        self.get_cell_display(coord)
    }

    /// Get cell content as a string for undo/redo snapshots
    fn get_cell_content_string(&self, coord: CellCoord) -> Option<String> {
        self.cell_content_string(self.current_sheet, coord)
    }

    /// A cell's content as it would be typed, or `None` when empty.
    fn cell_content_string(&self, sheet: u32, coord: CellCoord) -> Option<String> {
        if let Some(formula) = self.engine.get_formula(sheet, coord) {
            return Some(formula_bar_text(&formula));
        }

        // Otherwise get the value
        match self.engine.get_value(sheet, coord) {
            CellResult::Empty => None,
            CellResult::Value(n) => Some(n.to_string()),
            CellResult::Text(s) => Some(s),
            CellResult::Bool(b) => Some(if b { "TRUE" } else { "FALSE" }.to_string()),
            CellResult::Error(_) => None, // Don't snapshot errors
        }
    }

    /// Set cell content from user input (with undo support)
    fn set_cell_content(&mut self, coord: CellCoord, content: &str) {
        let action = self.set_cell_content_action(coord, content);
        self.undo_history.push(action);
    }

    /// Set cell content as if typed, returning the undo action for it.
    fn set_cell_content_action(&mut self, coord: CellCoord, content: &str) -> UndoAction {
        let content = content.trim();
        let sheet = self.current_sheet;

        // Capture old value for undo
        let old_value = self
            .get_cell_content_string(coord)
            .map(|s| CellSnapshot { input: s });
        let content_action = if content.is_empty() {
            UndoAction::CellClear {
                sheet,
                coord,
                old_value,
            }
        } else {
            UndoAction::CellChange {
                sheet,
                coord,
                old_value,
                new_value: Some(CellSnapshot {
                    input: content.to_string(),
                }),
            }
        };

        if content.starts_with('=') {
            if let Err(e) = self.engine.set_formula(sheet, coord, content) {
                self.set_status(&format!("Formula error: {e}"));
            }
            self.modified = true;
            self.refresh_all_charts();
        } else {
            self.apply_cell_content(sheet, coord, (!content.is_empty()).then_some(content));
        }

        // Typing 12% or a date into an unformatted cell gives it that format.
        let old_format = self.engine.cell_format(sheet, coord).cloned();
        let shown = self.effective_format(sheet, coord).cloned();
        let typed_format = match parse_typed_number(content) {
            Some((_, Some(code)))
                if !content.starts_with('=')
                    && shown.as_ref().is_none_or(|f| f.number_format.is_none()) =>
            {
                Some(code)
            }
            _ => None,
        };
        match typed_format {
            Some(code) => {
                let mut new_format = shown.unwrap_or_default();
                new_format.number_format = Some(code.to_string());
                self.engine
                    .set_cell_format(sheet, coord, new_format.clone());
                UndoAction::Group(vec![
                    content_action,
                    UndoAction::Format {
                        sheet,
                        changes: vec![(coord, old_format, Some(new_format))],
                    },
                ])
            }
            None => content_action,
        }
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
            // Text-formatted (@) cells keep what was typed as text.
            Some(s)
                if self
                    .effective_format(sheet, coord)
                    .is_some_and(|f| f.number_format.as_deref() == Some("@")) =>
            {
                self.engine
                    .set_value(sheet, coord, CellValueInput::Text(s.to_string()));
            }
            Some(s) if parse_typed_number(s).is_some() => {
                let (n, _) = parse_typed_number(s).unwrap_or_default();
                self.engine
                    .set_value(sheet, coord, CellValueInput::Number(n));
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
            self.selected_picture = None;
            self.replay(&action, false);
            self.undo_history.push_redo(action);
            self.set_status("Undo");
        }
    }

    /// Redo the last undone action
    fn redo(&mut self) {
        if let Some(action) = self.undo_history.pop_redo() {
            self.selected_picture = None;
            self.replay(&action, true);
            self.undo_history.push_undo_for_redo(action);
            self.set_status("Redo");
        }
    }

    /// Apply an action's new state (`forward`) or restore its old state.
    fn replay(&mut self, action: &UndoAction, forward: bool) {
        match action {
            UndoAction::CellChange {
                sheet,
                coord,
                old_value,
                new_value,
            } => {
                let value = if forward { new_value } else { old_value };
                self.apply_cell_content(*sheet, *coord, value.as_ref().map(|s| s.input.as_str()));
                self.selection.move_to(*coord);
            }
            UndoAction::CellClear {
                sheet,
                coord,
                old_value,
            } => {
                let value = if forward { None } else { old_value.as_ref() };
                self.apply_cell_content(*sheet, *coord, value.map(|s| s.input.as_str()));
                self.selection.move_to(*coord);
            }
            UndoAction::Format { sheet, changes } => {
                for (coord, old, new) in changes {
                    let format = if forward { new } else { old };
                    self.engine
                        .set_cell_format(*sheet, *coord, format.clone().unwrap_or_default());
                }
                self.modified = true;
            }
            UndoAction::Resize {
                sheet,
                axis,
                index,
                old,
                new,
            } => {
                let size = if forward { *new } else { *old };
                self.set_size(*sheet, *axis, *index, size);
            }
            UndoAction::LineFormat {
                sheet,
                axis,
                changes,
            } => {
                for (index, old, new) in changes {
                    let format = if forward { new } else { old };
                    self.engine.formatting_mut(*sheet).set_line_format(
                        *axis,
                        *index,
                        format.clone().unwrap_or_default(),
                    );
                }
                self.modified = true;
            }
            UndoAction::Note {
                sheet,
                coord,
                old,
                new,
            } => {
                let note = if forward { new } else { old };
                self.apply_note(*sheet, *coord, note.clone());
            }
            UndoAction::Snapshot(states) => {
                let state = if forward { &states.1 } else { &states.0 };
                self.restore_workbook_state(state);
            }
            UndoAction::Group(actions) => {
                if forward {
                    for a in actions {
                        self.replay(a, true);
                    }
                } else {
                    for a in actions.iter().rev() {
                        self.replay(a, false);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Clipboard
    // ------------------------------------------------------------------

    /// Run many cell changes, refreshing charts once at the end.
    fn batch<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.batching = true;
        let out = f(self);
        self.batching = false;
        if std::mem::take(&mut self.charts_dirty) {
            self.refresh_all_charts();
        }
        out
    }

    /// Copy (or cut) the selection: displayed values go to the system
    /// clipboard as tab-separated text; formulas and formats stay here.
    fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) {
        let sheet = self.current_sheet;
        let range = self.clamp_to_used_or_self(self.selection.primary_range());
        let mut cells = HashMap::new();
        let mut notes = HashMap::new();
        let mut lines = Vec::new();
        for row in range.start.row..=range.end.row {
            let mut fields = Vec::new();
            for col in range.start.col..=range.end.col {
                let coord = CellCoord::new(row, col);
                let format = self.effective_format(sheet, coord).cloned();
                let value = self.engine.get_value(sheet, coord);
                fields.push(tsv_field(&display_text(&value, format.as_ref())));
                let content = self.cell_content_string(sheet, coord);
                let offset = (row - range.start.row, col - range.start.col);
                let note = self.note_at(coord).cloned();
                if content.is_some() || format.is_some() || note.is_some() {
                    cells.insert(offset, (content, format));
                }
                if let Some(note) = note {
                    notes.insert(offset, note);
                }
            }
            lines.push(fields.join("\t"));
        }
        let text = lines.join("\r\n") + "\r\n";
        ctx.copy_text(text.clone());
        let count = (range.end.row - range.start.row + 1) * (range.end.col - range.start.col + 1);
        self.clipboard = Some(ClipboardCells {
            sheet,
            origin: range.start,
            rows: range.end.row - range.start.row + 1,
            cols: range.end.col - range.start.col + 1,
            cells,
            notes,
            text,
            cut,
        });
        let noun = if count == 1 { "cell" } else { "cells" };
        self.set_status(&if cut {
            format!("Cut {count} {noun}; paste to move them")
        } else {
            format!("Copied {count} {noun}")
        });
    }

    /// Paste clipboard text at the selection.
    fn paste_text(&mut self, text: &str) {
        let normalize = |t: &str| t.replace("\r\n", "\n").trim_end_matches('\n').to_string();
        let ours = self
            .clipboard
            .as_ref()
            .is_some_and(|c| normalize(&c.text) == normalize(text));
        if ours {
            if let Some(clip) = self.clipboard.take() {
                self.paste_cells(&clip);
                if !clip.cut {
                    self.clipboard = Some(clip);
                }
            }
        } else {
            self.paste_values(text);
        }
    }

    /// Top-left corners to paste a `rows` x `cols` block at. A single cell
    /// fills the whole selection, as in Excel.
    fn paste_targets(&self, rows: u32, cols: u32) -> Vec<CellCoord> {
        let range = self.clamp_to_used_or_self(self.selection.primary_range());
        if rows == 1 && cols == 1 {
            (range.start.row..=range.end.row)
                .flat_map(|r| (range.start.col..=range.end.col).map(move |c| CellCoord::new(r, c)))
                .collect()
        } else {
            vec![range.start]
        }
    }

    /// Paste cells copied in this app, keeping formulas and formats.
    fn paste_cells(&mut self, clip: &ClipboardCells) {
        let sheet = self.current_sheet;
        let targets = self.paste_targets(clip.rows, clip.cols);
        let parser = FormulaParser::new();
        let actions = self.batch(|app| {
            let mut actions = Vec::new();
            let mut format_changes = Vec::new();
            let mut written = HashSet::new();
            for origin in &targets {
                for r in 0..clip.rows {
                    for c in 0..clip.cols {
                        let dest = CellCoord::new(origin.row + r, origin.col + c);
                        let (content, format) =
                            clip.cells.get(&(r, c)).cloned().unwrap_or((None, None));
                        // Copied formulas follow their new position; cut ones keep
                        // their references, as in Excel.
                        let content = match content {
                            Some(f) if f.starts_with('=') && !clip.cut => {
                                let src = CellCoord::new(clip.origin.row + r, clip.origin.col + c);
                                let rows = dest.row as i64 - src.row as i64;
                                let cols = dest.col as i64 - src.col as i64;
                                match parser.parse(&f) {
                                    Ok(mut expr) if rows != 0 || cols != 0 => {
                                        expr.offset_references(rows, cols);
                                        Some(format!("={expr}"))
                                    }
                                    _ => Some(f),
                                }
                            }
                            other => other,
                        };
                        app.put_cell(
                            sheet,
                            dest,
                            content,
                            format,
                            &mut actions,
                            &mut format_changes,
                        );
                        app.put_note(sheet, dest, clip.notes.get(&(r, c)).cloned(), &mut actions);
                        written.insert(dest);
                    }
                }
            }
            if !format_changes.is_empty() {
                actions.push(UndoAction::Format {
                    sheet,
                    changes: format_changes,
                });
            }

            // Moving: clear what was left behind at the source.
            if clip.cut {
                let mut source_formats = Vec::new();
                for &(r, c) in clip.cells.keys() {
                    let src = CellCoord::new(clip.origin.row + r, clip.origin.col + c);
                    if clip.sheet != sheet || !written.contains(&src) {
                        app.put_cell(
                            clip.sheet,
                            src,
                            None,
                            None,
                            &mut actions,
                            &mut source_formats,
                        );
                        app.put_note(clip.sheet, src, None, &mut actions);
                    }
                }
                if !source_formats.is_empty() {
                    actions.push(UndoAction::Format {
                        sheet: clip.sheet,
                        changes: source_formats,
                    });
                }
            }
            actions
        });

        if targets.len() == 1 {
            let start = targets[0];
            self.selection.move_to(start);
            self.selection.extend_to(CellCoord::new(
                start.row + clip.rows - 1,
                start.col + clip.cols - 1,
            ));
        }
        self.finish_paste(actions);
    }

    /// Set or clear one cell's note, recording an undo step.
    fn put_note(
        &mut self,
        sheet: u32,
        coord: CellCoord,
        note: Option<crate::format::Note>,
        actions: &mut Vec<UndoAction>,
    ) {
        let old = self
            .engine
            .formatting(sheet)
            .and_then(|f| f.notes.get(&coord))
            .cloned();
        if old != note {
            self.apply_note(sheet, coord, note.clone());
            actions.push(UndoAction::Note {
                sheet,
                coord,
                old,
                new: note,
            });
        }
    }

    /// Write one cell's content and format, recording undo steps.
    fn put_cell(
        &mut self,
        sheet: u32,
        coord: CellCoord,
        content: Option<String>,
        format: Option<CellFormat>,
        actions: &mut Vec<UndoAction>,
        format_changes: &mut Vec<(CellCoord, Option<CellFormat>, Option<CellFormat>)>,
    ) {
        // Format first, so a Text (@) format applies to the content.
        let old_format = self.engine.cell_format(sheet, coord).cloned();
        if old_format != format {
            self.engine
                .set_cell_format(sheet, coord, format.clone().unwrap_or_default());
            format_changes.push((coord, old_format, format));
        }
        let old_value = self.cell_content_string(sheet, coord);
        if old_value != content {
            self.apply_cell_content(sheet, coord, content.as_deref());
            let old_value = old_value.map(|input| CellSnapshot { input });
            actions.push(match content {
                Some(input) => UndoAction::CellChange {
                    sheet,
                    coord,
                    old_value,
                    new_value: Some(CellSnapshot { input }),
                },
                None => UndoAction::CellClear {
                    sheet,
                    coord,
                    old_value,
                },
            });
        }
    }

    /// Paste text from another app: each field is entered as if typed.
    fn paste_values(&mut self, text: &str) {
        let rows = parse_tsv(text);
        let height = rows.len() as u32;
        let width = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
        if height == 0 || width == 0 {
            return;
        }
        let targets = self.paste_targets(height, width);
        let actions = self.batch(|app| {
            let mut actions = Vec::new();
            for origin in &targets {
                for (r, row) in rows.iter().enumerate() {
                    for (c, field) in row.iter().enumerate() {
                        let dest = CellCoord::new(origin.row + r as u32, origin.col + c as u32);
                        let unchanged = app.get_cell_content_string(dest).as_deref().unwrap_or("")
                            == field.trim();
                        if !unchanged {
                            actions.push(app.set_cell_content_action(dest, field));
                        }
                    }
                }
            }
            actions
        });
        if targets.len() == 1 {
            let start = targets[0];
            self.selection.move_to(start);
            self.selection.extend_to(CellCoord::new(
                start.row + height - 1,
                start.col + width - 1,
            ));
        }
        self.finish_paste(actions);
    }

    fn finish_paste(&mut self, actions: Vec<UndoAction>) {
        if actions.is_empty() {
            return;
        }
        self.undo_history.push(UndoAction::Group(actions));
        self.modified = true;
        self.set_status("Pasted");
    }

    /// Edit > Paste and the right-click menu read the clipboard directly;
    /// Ctrl+V arrives as an egui paste event instead.
    fn paste_from_system_clipboard(&mut self) {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            self.set_status("Nothing to paste");
            return;
        };
        match clipboard.get_text() {
            Ok(text) if !text.is_empty() => self.paste_text(&text),
            _ => match clipboard.get_image() {
                Ok(image) => self.paste_image(image),
                Err(_) => self.set_status("Nothing to paste"),
            },
        }
    }

    /// Clear the contents (not formats) of every selected cell.
    fn delete_selection(&mut self) {
        let sheet = self.current_sheet;
        let range = self.selection.primary_range();
        let in_range = |c: &CellCoord| {
            (range.start.row..=range.end.row).contains(&c.row)
                && (range.start.col..=range.end.col).contains(&c.col)
        };
        let coords: Vec<CellCoord> = self
            .engine
            .iter_sheet_inputs(sheet)
            .map(|(coord, _)| coord)
            .filter(in_range)
            .collect();
        if coords.is_empty() {
            return;
        }
        let actions = self.batch(|app| {
            coords
                .into_iter()
                .map(|coord| app.set_cell_content_action(coord, ""))
                .collect::<Vec<_>>()
        });
        self.undo_history.push(UndoAction::Group(actions));
    }

    /// Ctrl+C / X / V arrive as egui events. Text fields (formula bar,
    /// dialogs) handle their own, so act only when the grid or nothing has focus.
    fn handle_clipboard_events(&mut self, ctx: &egui::Context) {
        if self.is_editing() || text_field_focused(ctx) {
            return;
        }
        let events: Vec<egui::Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
                    )
                })
                .cloned()
                .collect()
        });
        let pasted_text = events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
        for event in events {
            match event {
                egui::Event::Copy => self.copy_selection(ctx, false),
                egui::Event::Cut => self.copy_selection(ctx, true),
                egui::Event::Paste(text) => self.paste_text(&text),
                _ => {}
            }
        }
        // egui pastes only text: with just an image on the clipboard (a
        // screenshot), Ctrl+V sends no event, so watch the key itself.
        let down = paste_key_down(ctx);
        if down && !self.paste_key_down && !pasted_text {
            if let Ok(image) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
                self.paste_image(image);
            }
        }
        self.paste_key_down = down;
    }

    // ------------------------------------------------------------------
    // Formatting
    // ------------------------------------------------------------------

    /// The format a cell shows: its own, else its row's, else its column's.
    fn effective_format(&self, sheet: u32, coord: CellCoord) -> Option<&CellFormat> {
        self.engine.formatting(sheet)?.effective(coord)
    }

    fn active_format(&self) -> CellFormat {
        self.effective_format(self.current_sheet, self.selection.active)
            .cloned()
            .unwrap_or_default()
    }

    /// Change every selected cell's format as one undo step. `change` gets
    /// each cell's position and the selection's bounds. Whole rows or
    /// columns get row/column formats instead of a million cell formats.
    fn apply_format(&mut self, change: impl Fn(CellCoord, CellRange, &mut CellFormat)) {
        let sheet = self.current_sheet;
        let range = self.selection.primary_range();
        let whole_cols = range.start.row == 0 && range.end.row == crate::cell::MAX_ROW;
        let whole_rows = range.start.col == 0 && range.end.col == crate::cell::MAX_COL;
        let mut actions = Vec::new();
        let mut changes = Vec::new();

        if whole_cols || whole_rows {
            let axis = if whole_cols {
                ResizeAxis::Column
            } else {
                ResizeAxis::Row
            };
            let (first, last) = match axis {
                ResizeAxis::Column => (range.start.col, range.end.col),
                ResizeAxis::Row => (range.start.row, range.end.row),
            };
            let mut line_changes = Vec::new();
            for i in first..=last {
                let old = self
                    .engine
                    .formatting(sheet)
                    .and_then(|f| f.line_format(axis, i))
                    .cloned();
                let mut new = old.clone().unwrap_or_default();
                // A coordinate inside the line, away from the selection's
                // top/bottom (or left/right) edge.
                let coord = match axis {
                    ResizeAxis::Column => CellCoord::new(1, i),
                    ResizeAxis::Row => CellCoord::new(i, 1),
                };
                change(coord, range, &mut new);
                let new = (!new.is_default()).then_some(new);
                if new != old {
                    self.engine.formatting_mut(sheet).set_line_format(
                        axis,
                        i,
                        new.clone().unwrap_or_default(),
                    );
                    line_changes.push((i, old, new));
                }
            }
            if !line_changes.is_empty() {
                actions.push(UndoAction::LineFormat {
                    sheet,
                    axis,
                    changes: line_changes,
                });
            }
            // Cells with their own format in those lines change too.
            let in_range = |c: &CellCoord| {
                (range.start.row..=range.end.row).contains(&c.row)
                    && (range.start.col..=range.end.col).contains(&c.col)
            };
            let own: Vec<CellCoord> = self
                .engine
                .formatting(sheet)
                .map(|f| f.cells().map(|(c, _)| c).filter(in_range).collect())
                .unwrap_or_default();
            for coord in own {
                let old = self.engine.cell_format(sheet, coord).cloned();
                let mut new = old.clone().unwrap_or_default();
                change(coord, range, &mut new);
                let new = (!new.is_default()).then_some(new);
                if new != old {
                    self.engine
                        .set_cell_format(sheet, coord, new.clone().unwrap_or_default());
                    changes.push((coord, old, new));
                }
            }
        } else {
            for row in range.start.row..=range.end.row {
                for col in range.start.col..=range.end.col {
                    let coord = CellCoord::new(row, col);
                    let old = self.engine.cell_format(sheet, coord).cloned();
                    // Start from what the cell shows, so a column's fill
                    // survives making one of its cells bold.
                    let mut new = self
                        .effective_format(sheet, coord)
                        .cloned()
                        .unwrap_or_default();
                    change(coord, range, &mut new);
                    let shown = self
                        .effective_format(sheet, coord)
                        .cloned()
                        .unwrap_or_default();
                    if new == shown {
                        continue;
                    }
                    let new = (!new.is_default()).then_some(new);
                    self.engine
                        .set_cell_format(sheet, coord, new.clone().unwrap_or_default());
                    changes.push((coord, old, new));
                }
            }
        }
        if !changes.is_empty() {
            actions.push(UndoAction::Format { sheet, changes });
        }
        if !actions.is_empty() {
            let action = if actions.len() == 1 {
                actions.pop().unwrap()
            } else {
                UndoAction::Group(actions)
            };
            self.undo_history.push(action);
            self.modified = true;
        }
    }

    fn handle_format_action(&mut self, action: FormatAction) {
        let current = self.active_format();
        match action {
            FormatAction::ToggleBold => {
                let on = !current.bold;
                self.apply_format(|_, _, f| f.bold = on);
            }
            FormatAction::ToggleItalic => {
                let on = !current.italic;
                self.apply_format(|_, _, f| f.italic = on);
            }
            FormatAction::ToggleUnderline => {
                let on = !current.underline;
                self.apply_format(|_, _, f| f.underline = on);
            }
            FormatAction::ToggleStrikethrough => {
                let on = !current.strikethrough;
                self.apply_format(|_, _, f| f.strikethrough = on);
            }
            FormatAction::FontSize(size) => self.apply_format(|_, _, f| f.font_size = size),
            FormatAction::FontColor(color) => self.apply_format(|_, _, f| f.font_color = color),
            FormatAction::Fill(fill) => self.apply_format(|_, _, f| f.fill = fill),
            FormatAction::Align(align) => self.apply_format(|_, _, f| f.h_align = align),
            FormatAction::Borders(preset) => self.apply_format(|coord, range, f| {
                f.borders = match preset {
                    BorderPreset::All => Borders::ALL,
                    BorderPreset::None => Borders::NONE,
                    BorderPreset::Outside => Borders {
                        top: coord.row == range.start.row,
                        bottom: coord.row == range.end.row,
                        left: coord.col == range.start.col,
                        right: coord.col == range.end.col,
                    },
                    BorderPreset::Bottom => Borders {
                        bottom: coord.row == range.end.row,
                        ..f.borders
                    },
                }
            }),
            FormatAction::NumberFormat(code) => {
                self.apply_format(|_, _, f| f.number_format = code.clone())
            }
            FormatAction::Decimals(delta) => {
                let code = format_bar::adjust_decimals(current.number_format.as_deref(), delta);
                self.apply_format(|_, _, f| f.number_format = code.clone());
            }
            FormatAction::Clear => self.apply_format(|_, _, f| *f = CellFormat::default()),
            FormatAction::FontName(name) => self.apply_format(|_, _, f| f.font_name = name.clone()),
            FormatAction::ToggleWrap => {
                let on = !current.wrap;
                self.apply_format(|_, _, f| f.wrap = on);
            }
            FormatAction::VAlign(v) => self.apply_format(|_, _, f| f.v_align = v),
            FormatAction::Merge => self.toggle_merge(),
        }
    }

    /// Set a column width or row height; `None` restores the default.
    fn set_size(&mut self, sheet: u32, axis: ResizeAxis, index: u32, size: Option<f32>) {
        let formatting = self.engine.formatting_mut(sheet);
        let (sizes, default) = match axis {
            ResizeAxis::Column => (&mut formatting.column_widths, DEFAULT_COLUMN_WIDTH),
            ResizeAxis::Row => (&mut formatting.row_heights, DEFAULT_ROW_HEIGHT),
        };
        match size.filter(|s| (s - default).abs() >= 0.5) {
            Some(s) => sizes.insert(index, s),
            None => sizes.remove(&index),
        };
        if sheet == self.current_sheet {
            self.sync_grid_config();
        }
        self.modified = true;
    }

    fn current_size(&self, axis: ResizeAxis, index: u32) -> Option<f32> {
        let formatting = self.engine.formatting(self.current_sheet)?;
        match axis {
            ResizeAxis::Column => formatting.column_widths.get(&index).copied(),
            ResizeAxis::Row => formatting.row_heights.get(&index).copied(),
        }
    }

    /// Point the grid at the current sheet's column widths and row heights.
    fn sync_grid_config(&mut self) {
        self.grid_config = GridConfig::for_sheet(self.engine.formatting(self.current_sheet));
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
        self.input_mode = InputMode::Navigation;
        self.edit_buffer.clear();
        self.formula_bar.editing = false;
        // A rejected entry (data validation) goes back to editing.
        if !self.commit_typed(coord, &content) {
            return;
        }

        if move_down {
            self.selection
                .move_to(self.step(self.selection.active, 1, 0));
        } else if move_right {
            self.selection
                .move_to(self.step(self.selection.active, 0, 1));
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
        self.filter_popup = None;
        self.fill_target = None;
        if let Some(state) = self.sheet_states.get(sheet_index as usize) {
            self.selection = state.selection.clone();
            self.scroll = state.scroll.clone();
        }
        self.sync_grid_config();
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
        self.selected_picture = None;

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

    /// Rename a sheet. Names follow Excel's rules, so the file saves.
    fn rename_sheet(&mut self, sheet_index: u32, new_name: String) {
        let index = sheet_index as usize;
        if index >= self.sheet_names.len() {
            return;
        }
        let new_name = new_name.trim().to_string();
        if new_name == self.sheet_names[index] {
            return;
        }
        let problem = if new_name.is_empty() {
            Some("Sheet names can't be blank")
        } else if new_name.chars().count() > 31 {
            Some("Sheet names can be at most 31 characters")
        } else if new_name.contains([':', '\\', '/', '?', '*', '[', ']']) {
            Some("Sheet names can't contain : \\ / ? * [ or ]")
        } else if new_name.starts_with('\'') || new_name.ends_with('\'') {
            Some("Sheet names can't start or end with an apostrophe")
        } else if self
            .sheet_names
            .iter()
            .enumerate()
            .any(|(i, name)| i != index && name.eq_ignore_ascii_case(&new_name))
        {
            Some("There's already a sheet with that name")
        } else {
            None
        };
        if let Some(problem) = problem {
            self.set_status(problem);
            return;
        }
        let old_name = self.sheet_names[index].clone();
        self.engine.rewrite_sheet_name(&old_name, &new_name);
        self.sheet_names[index] = new_name;
        self.engine.set_sheet_names(self.sheet_names.clone());
        self.modified = true;
    }

    /// Ask whether to save unsaved changes. Returns true when it is safe to
    /// discard the current workbook (saved, or the user chose not to save).
    fn confirm_discard(&mut self) -> bool {
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

        if !self.modified {
            return true;
        }
        let name = self.document_name();
        let choice = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("RustSheet")
            .set_description(format!("Do you want to save changes to {name}?"))
            .set_buttons(MessageButtons::YesNoCancel)
            .show();
        match choice {
            MessageDialogResult::Yes => {
                self.save_file();
                // Still modified means the save was cancelled or failed.
                !self.modified
            }
            MessageDialogResult::No => true,
            _ => false,
        }
    }

    /// File name shown in the title bar and prompts.
    fn document_name(&self) -> String {
        self.current_file
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn request_new_workbook(&mut self) {
        if self.confirm_discard() {
            self.new_workbook();
        }
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
        self.sync_grid_config();
        self.set_status("New workbook created");
    }

    fn extension_is(path: &Path, ext: &str) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    /// Open file dialog and load workbook
    fn open_file(&mut self) {
        use rfd::FileDialog;

        if !self.confirm_discard() {
            return;
        }

        // The first filter is the one selected by default, so lead with every
        // supported format rather than hiding .xlsx behind a CSV-only view.
        let supported = [
            #[cfg(feature = "xlsx")]
            "xlsx",
            #[cfg(feature = "csv")]
            "csv",
        ];

        let mut dialog = FileDialog::new().add_filter("Spreadsheets", &supported);
        #[cfg(feature = "xlsx")]
        {
            dialog = dialog.add_filter("Excel Files", &["xlsx"]);
        }
        #[cfg(feature = "csv")]
        {
            dialog = dialog.add_filter("CSV", &["csv"]);
        }
        let file = dialog.add_filter("All Files", &["*"]).pick_file();

        if let Some(path) = file {
            self.load_file(&path);
        }
    }

    fn load_file(&mut self, path: &Path) {
        #[cfg(feature = "csv")]
        if Self::extension_is(path, "csv") {
            self.load_csv(path);
            return;
        }
        #[cfg(feature = "xlsx")]
        {
            self.load_xlsx(path);
        }
        #[cfg(not(feature = "xlsx"))]
        self.set_status("Excel support not enabled. Rebuild with --features xlsx");
    }

    fn finish_open(&mut self, path: &Path) {
        // Autosaves being recovered aren't "recent files".
        let from_recovery = self.recovery.as_ref().is_some_and(|r| r.holds(path));
        if !from_recovery {
            self.settings.add_recent(path);
        }
        self.suggested_name = None;
        self.recovered_from = None;
        self.current_sheet = 0;
        self.sync_grid_config();
        self.current_file = Some(path.to_path_buf());
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
    fn load_csv(&mut self, path: &Path) {
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
    fn load_xlsx(&mut self, path: &Path) {
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

                // Fonts, fills, borders, number formats and column/row sizes.
                match crate::xlsx::read_formatting_from_path(path) {
                    Ok(sheets) => {
                        for (name, formatting) in sheets {
                            if let Some(i) = self.sheet_names.iter().position(|n| *n == name) {
                                *self.engine.formatting_mut(i as u32) = formatting;
                            }
                        }
                    }
                    Err(e) => self.set_status(&format!("Formatting not loaded: {e}")),
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

        // Excel first so the default filter matches the default file name.
        let mut dialog = FileDialog::new();
        #[cfg(feature = "xlsx")]
        {
            dialog = dialog.add_filter("Excel Files", &["xlsx"]);
        }
        #[cfg(feature = "csv")]
        {
            dialog = dialog.add_filter("CSV", &["csv"]);
        }
        let default_name = self.suggested_name.clone().unwrap_or_else(|| {
            if cfg!(feature = "xlsx") {
                "workbook.xlsx".into()
            } else {
                "workbook.csv".into()
            }
        });
        if let Some(dir) = self.recovered_from.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        let file = dialog.set_file_name(default_name).save_file();

        if let Some(path) = file {
            self.save_to_path(&path);
        }
    }

    fn save_to_path(&mut self, path: &Path) {
        #[cfg(feature = "csv")]
        if Self::extension_is(path, "csv") {
            self.save_csv(path);
            return;
        }
        #[cfg(feature = "xlsx")]
        {
            self.save_xlsx(path);
        }
        #[cfg(not(feature = "xlsx"))]
        self.set_status("Excel support not enabled. Rebuild with --features xlsx");
    }

    /// Bookkeeping after any successful save.
    fn saved_to(&mut self, path: &Path) {
        self.current_file = Some(path.to_path_buf());
        self.modified = false;
        self.suggested_name = None;
        self.recovered_from = None;
        self.settings.add_recent(path);
        if let Some(rec) = &mut self.recovery {
            rec.clear();
        }
    }

    #[cfg(feature = "csv")]
    fn save_csv(&mut self, path: &Path) {
        // Write beside the file, then swap it in, so a crash can't truncate it.
        let tmp = printing::temp_beside(path);
        let written = crate::csv_io::write_path(&self.engine, self.current_sheet, &tmp)
            .map_err(|e| e.to_string())
            .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| e.to_string()));
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        match written {
            Ok(()) => {
                self.saved_to(path);
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

    /// Write the workbook as .xlsx without touching the document's state
    /// (also used for autosave). The file is replaced atomically.
    #[cfg(feature = "xlsx")]
    fn write_xlsx(&self, path: &Path) -> Result<(), String> {
        let mut writer = XlsxWriter::new();
        let charts = self.chart_windows.all_charts();
        for (sheet_index, sheet_name) in self.sheet_names.iter().enumerate() {
            writer
                .add_engine_sheet_with_charts(sheet_name, &self.engine, sheet_index as u32, &charts)
                .map_err(|e| format!("sheet '{sheet_name}': {e}"))?;
        }
        let tmp = printing::temp_beside(path);
        let result = writer
            .save_with_charts(&tmp, &charts)
            .map_err(|e| e.to_string())
            .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| e.to_string()));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    #[cfg(not(feature = "xlsx"))]
    fn write_xlsx(&self, _path: &Path) -> Result<(), String> {
        Err("Excel support not enabled".into())
    }

    #[cfg(feature = "xlsx")]
    fn save_xlsx(&mut self, path: &Path) {
        match self.write_xlsx(path) {
            Ok(()) => {
                self.saved_to(path);
                let sheet_count = self.sheet_names.len();
                let has_pivots = self
                    .engine
                    .all_formatting()
                    .values()
                    .any(|f| !f.pivots.is_empty());
                self.set_status(&format!(
                    "Saved: {} ({} sheet{}){}",
                    path.display(),
                    sheet_count,
                    if sheet_count == 1 { "" } else { "s" },
                    if has_pivots {
                        ". In Excel, PivotTables show as values; RustSheet can still refresh them."
                    } else {
                        ""
                    }
                ));
            }
            Err(e) => {
                let _ = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("Couldn't save")
                    .set_description(format!(
                        "RustSheet couldn't save {}.\n\n{e}",
                        path.display()
                    ))
                    .show();
                self.set_status(&format!("Failed to save: {e}"));
            }
        }
    }

    /// Add a new chart
    fn add_chart(&mut self, mut chart: ChartDefinition) {
        // Place a new chart just right of its data, like Excel does.
        let ranges = chart.dependent_ranges();
        if let (Some(top), Some(right)) = (
            ranges.iter().map(|r| r.start.row.min(r.end.row)).min(),
            ranges.iter().map(|r| r.start.col.max(r.end.col)).max(),
        ) {
            chart.overlay_area.anchor_cell = (top, right + 1);
        }
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
        if self.batching {
            self.charts_dirty = true;
            return;
        }
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
        // A bug must not take the user's work with it: save what we can
        // where the next launch will offer it back, then explain and exit.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frame(ctx)));
        if let Err(panic) = result {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown error".into());
            let saved = self.modified && {
                self.autosave();
                self.recovery.as_ref().is_some_and(|r| r.has_autosave)
            };
            let detail = if saved {
                "Your unsaved changes were kept. Open RustSheet again to recover them."
            } else if self.modified {
                "RustSheet couldn't keep your unsaved changes."
            } else {
                "You had no unsaved changes."
            };
            if let Some(dir) = super::settings::data_dir() {
                let _ = std::fs::write(dir.join("last-crash.txt"), &message);
            }
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Error)
                .set_title("RustSheet has to close")
                .set_description(format!("RustSheet ran into a problem and has to close.\n\n{detail}\n\nDetails: {message}"))
                .show();
            std::process::exit(1);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // A normal exit leaves nothing to recover.
        self.end_recovery();
    }
}

impl SpreadsheetApp {
    fn frame(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.confirm_discard() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // Theme: follow Windows, or the user's choice.
        ctx.set_theme(match self.settings.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        });
        let dark = ctx.theme() == egui::Theme::Dark;
        if self.dark_mode != Some(dark) {
            self.theme = if dark { Theme::dark() } else { Theme::light() };
            self.dark_mode = Some(dark);
        }
        self.tick_recovery(ctx);
        self.fonts.begin_frame(ctx);

        let title = format!(
            "{}{} - RustSheet",
            self.document_name(),
            if self.modified { "*" } else { "" }
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }

        // ======================================================================
        // 1. STATE TRANSITION HANDLING (Pre-Render)
        // Process InputMode transitions BEFORE any UI rendering to ensure
        // focus is set correctly for the current frame
        // ======================================================================
        if let InputMode::TransitionToEdit { initial_char } = self.input_mode.clone() {
            let active_cell = self.selection.active;

            // Initialize edit buffer
            let initial_text = if let Some(text) = self.retry_text.take() {
                text
            } else if let Some(c) = initial_char {
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

        let mut format_action: Option<FormatAction> = None;
        let mut command: Option<Command> = None;

        // Top panel for toolbar
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if let Some(c) = self.menu_bar(ui) {
                command = Some(c);
            }
        });

        // Formatting toolbar
        let active_format = self.active_format();
        let font_families = self.fonts.families().to_vec();
        TopBottomPanel::top("format_bar").show(ctx, |ui| {
            ui.add_space(2.0);
            if let Some(action) = format_bar::show(ui, &active_format, &font_families) {
                format_action = Some(action);
            }
            ui.add_space(2.0);
        });
        if let Some(action) = format_action.take() {
            self.handle_format_action(action);
        }

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
        // Charts are placed relative to the grid, which is laid out below;
        // skip the first frame until its position is known.
        let chart_response = match self.grid_origin {
            Some(origin) => {
                let (config, scroll) = (&self.grid_config, &self.scroll);
                self.chart_windows.show(ctx, |(row, col)| {
                    origin
                        + Vec2::new(
                            config.column_x(col) - scroll.offset_x,
                            config.row_y(row) - scroll.offset_y,
                        )
                })
            }
            None => {
                ctx.request_repaint();
                Default::default()
            }
        };

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

        // Keyboard shortcuts. While a text field has focus it gets the keys.
        let shift = ctx.input(|i| i.modifiers.shift);
        let typing = formula_bar_has_focus || text_field_focused(ctx);
        if let Some(c) = self.shortcut(ctx, typing) {
            command = Some(c);
        }
        if !typing {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                if self.help_panel.visible {
                    self.help_panel.visible = false;
                }
                self.fill_target = None;
                self.selected_picture = None;
            }
            // Delete removes the selected picture, or clears every selected
            // cell (with undo support)
            if ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
                && !self.is_editing()
            {
                match self.selected_picture {
                    Some(i) => self.delete_picture(i),
                    None => self.delete_selection(),
                }
            }
            self.handle_clipboard_events(ctx);
        }

        // Main grid area
        CentralPanel::default().show(ctx, |ui| {
            let viewport_size = ui.available_size();
            self.grid_origin =
                Some(ui.available_rect_before_wrap().min + Vec2::new(HEADER_WIDTH, HEADER_HEIGHT));

            self.last_viewport = viewport_size;
            let used = self.used_extent();
            let fill_preview = self
                .fill_target
                .and_then(|t| self.fill_plan(t))
                .map(|p| p.0);
            let pictures = self
                .engine
                .formatting(self.current_sheet)
                .map_or(&[][..], |f| f.pictures.as_slice());
            self.picture_textures.update(ctx, pictures);
            if self.selected_picture.is_some_and(|i| i >= pictures.len()) {
                self.selected_picture = None;
            }
            let grid = SpreadsheetGrid::new(
                self.current_sheet,
                &self.engine,
                &self.selection,
                &self.grid_config,
                &self.scroll,
                &self.theme,
            )
            .with_used_extent(used)
            .with_fill_handle(!self.is_editing() && self.selected_picture.is_none())
            .with_fill_preview(fill_preview)
            .with_fonts(&self.fonts)
            .with_pictures(&self.picture_textures.ids, self.selected_picture);

            let grid_response = grid.show(ui);

            // Pictures: select, move or resize, menu. Working with cells
            // deselects them.
            if let Some(i) = grid_response.picture_clicked {
                if self.is_editing() {
                    self.edit_buffer = self.formula_bar.content.clone();
                    self.confirm_edit(false, false);
                }
                self.selected_picture = Some(i);
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("spreadsheet_grid")));
            } else if grid_response.clicked_cell.is_some()
                || grid_response.drag_started.is_some()
                || grid_response.right_clicked_cell.is_some()
                || grid_response.header_select.is_some()
                || grid_response.select_all
                || grid_response.navigation.is_some()
                || grid_response.text_input_char.is_some()
            {
                self.selected_picture = None;
            }
            if let Some((i, place)) = grid_response.picture_placed {
                self.place_picture(i, place);
            }
            if let Some((i, action)) = grid_response.picture_action {
                self.picture_action(i, action);
            }

            if let Some(offset) = grid_response.scroll_to {
                self.scroll
                    .set_offset(offset.x, offset.y, &self.grid_config);
            }
            if let Some(hs) = grid_response.header_select {
                if self.is_editing() {
                    self.edit_buffer = self.formula_bar.content.clone();
                    self.confirm_edit(false, false);
                }
                self.handle_header_select(hs);
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("spreadsheet_grid")));
            }
            if grid_response.select_all {
                self.select_everything();
            }
            if let Some((axis, index)) = grid_response.right_clicked_header {
                let r = self.selection.primary_range();
                let (lo, hi) = match axis {
                    ResizeAxis::Row => (r.start.row, r.end.row),
                    ResizeAxis::Column => (r.start.col, r.end.col),
                };
                let whole = match axis {
                    ResizeAxis::Row => r.start.col == 0 && r.end.col == crate::cell::MAX_COL,
                    ResizeAxis::Column => r.start.row == 0 && r.end.row == crate::cell::MAX_ROW,
                };
                if !whole || !(lo..=hi).contains(&index) {
                    self.select_lines(axis, index, index);
                }
            }
            if let Some((coord, pos)) = grid_response.hovered_note {
                if self.note_editor.is_none() {
                    self.show_note_popup(ctx, coord, pos);
                }
            }
            if let Some(pos) = grid_response.validation_dropdown {
                self.open_list_popup(pos);
            }
            if let Some((col, pos)) = grid_response.filter_button {
                self.open_filter_popup(col, pos);
            }
            if let Some(target) = grid_response.fill_to {
                self.fill_target = Some(target);
            }
            if grid_response.fill_released {
                if let Some(target) = self.fill_target.take() {
                    self.fill_to(target);
                }
            }
            if let Some(row) = grid_response.autofit_row {
                let height = fit_row_height(
                    ctx,
                    &self.engine,
                    Some(&self.fonts),
                    &self.grid_config,
                    self.current_sheet,
                    row,
                );
                let old = self.current_size(ResizeAxis::Row, row);
                self.set_size(self.current_sheet, ResizeAxis::Row, row, Some(height));
                let new = self.current_size(ResizeAxis::Row, row);
                if new != old {
                    self.undo_history.push(UndoAction::Resize {
                        sheet: self.current_sheet,
                        axis: ResizeAxis::Row,
                        index: row,
                        old,
                        new,
                    });
                }
            }

            // Right-click outside the selection selects that cell first.
            if let Some(coord) = grid_response.right_clicked_cell {
                if !self.selection.contains(coord) {
                    self.selection.move_to(coord);
                }
            }
            if let Some(action) = grid_response.context_action {
                command = Some(action.into());
            }

            // Column and row resizing; one undo step per drag
            if let Some((axis, index, size)) = grid_response.resize {
                if self.resize_origin.is_none() {
                    self.resize_origin = Some((axis, index, self.current_size(axis, index)));
                }
                self.set_size(self.current_sheet, axis, index, Some(size));
            }
            if grid_response.resize_ended {
                if let Some((axis, index, old)) = self.resize_origin.take() {
                    let new = self.current_size(axis, index);
                    if new != old {
                        self.undo_history.push(UndoAction::Resize {
                            sheet: self.current_sheet,
                            axis,
                            index,
                            old,
                            new,
                        });
                    }
                }
            }
            if let Some(col) = grid_response.autofit_column {
                if let Some(width) = fit_column_width(
                    ctx,
                    &self.engine,
                    Some(&self.fonts),
                    self.current_sheet,
                    col,
                ) {
                    let axis = ResizeAxis::Column;
                    let old = self.current_size(axis, col);
                    self.set_size(self.current_sheet, axis, col, Some(width));
                    let new = self.current_size(axis, col);
                    if new != old {
                        self.undo_history.push(UndoAction::Resize {
                            sheet: self.current_sheet,
                            axis,
                            index: col,
                            old,
                            new,
                        });
                    }
                }
            }

            // Handle drag for multi-cell selection
            if let Some(coord) = grid_response.drag_started {
                if self.is_editing() {
                    // Confirm current edit before starting drag selection
                    self.edit_buffer = self.formula_bar.content.clone();
                    self.confirm_edit(false, false);
                }
                self.selection.move_to(self.snap_to_merge(coord));
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
                let coord = self.snap_to_merge(coord);
                if grid_response.clicked_with_shift {
                    self.selection.extend_to(coord);
                } else {
                    self.selection.move_to(coord);
                }
                self.scroll
                    .scroll_to_cell(coord, &self.grid_config, viewport_size);
                // Request focus on click
                let grid_id = egui::Id::new("spreadsheet_grid");
                ctx.memory_mut(|m| m.request_focus(grid_id));
            }

            // Handle double-click for editing
            if let Some(coord) = grid_response.double_clicked_cell {
                self.selection.move_to(self.snap_to_merge(coord));
                // Start editing - FSM will handle focus in next pre-render
                self.start_editing(None);
            }

            // Handle F2/Enter to edit (only when formula bar doesn't have focus)
            if !formula_bar_has_focus {
                if grid_response.edit_cell.is_some() && !self.is_editing() {
                    // Start editing - FSM will handle focus in next pre-render
                    self.start_editing(None);
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
                    self.navigate(nav, shift, viewport_size);
                    // Keep focus on grid
                    let grid_id = egui::Id::new("spreadsheet_grid");
                    ctx.memory_mut(|m| m.request_focus(grid_id));
                }
            }
        });

        // Dialogs and popups float above everything.
        self.show_recovery_prompt(ctx);
        self.show_find_dialog(ctx);
        self.show_note_editor(ctx);
        self.show_validation_dialog(ctx);
        self.show_conditional_dialog(ctx);
        self.show_alt_text_editor(ctx);
        self.show_pivot_dialog(ctx);
        self.show_list_popup(ctx);
        self.show_input_message(ctx);
        self.show_filter_popup(ctx);
        self.show_sort_dialog(ctx);
        self.show_print_dialog(ctx);

        if let Some(c) = command {
            self.run_command(ctx, c);
        }
    }
}

/// Ctrl+V is held down, read from the keyboard directly.
#[cfg(windows)]
fn paste_key_down(ctx: &egui::Context) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_V};
    let focused = ctx.input(|i| i.focused && i.modifiers.command);
    // SAFETY: GetAsyncKeyState only reads key state.
    focused && unsafe { GetAsyncKeyState(i32::from(VK_V)) } as u16 & 0x8000 != 0
}

#[cfg(not(windows))]
fn paste_key_down(_ctx: &egui::Context) -> bool {
    false
}

/// Whether a text field (formula bar, dialog box, sheet rename) has focus.
fn text_field_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
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
    run_with_file(None)
}

/// Run the application, opening `path` at startup (e.g. from a file association).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_with_file(path: Option<PathBuf>) -> Result<(), eframe::Error> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([600.0, 400.0])
        .with_maximized(true)
        .with_title("RustSheet");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "RustSheet",
        options,
        Box::new(move |_cc| {
            let mut app = SpreadsheetApp::new();
            app.settings = Settings::load();
            app.fonts = FontLibrary::scan_in_background();
            app.recoverable = recovery::find_recoverable();
            app.recovery = Recovery::new();
            if let Some(path) = path {
                app.load_file(&path);
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn window_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../../assets/icon-256.png")).ok()?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::cell::CellError;

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

    fn select(app: &mut SpreadsheetApp, from: &str, to: &str) {
        app.selection.move_to(CellCoord::from_a1(from).unwrap());
        app.selection.extend_to(CellCoord::from_a1(to).unwrap());
    }

    fn format_of(app: &SpreadsheetApp, a1: &str) -> CellFormat {
        app.engine
            .cell_format(app.current_sheet, CellCoord::from_a1(a1).unwrap())
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn toolbar_actions_apply_to_selection_and_undo_in_one_step() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        select(&mut app, "A1", "B2");

        app.handle_format_action(FormatAction::ToggleBold);
        app.handle_format_action(FormatAction::Fill(Some(crate::format::Rgb(255, 242, 204))));
        for cell in ["A1", "A2", "B1", "B2"] {
            assert!(format_of(&app, cell).bold, "{cell} bold");
        }
        assert!(!format_of(&app, "C1").bold);

        // Toggling again turns bold off everywhere, following the active cell.
        app.handle_format_action(FormatAction::ToggleBold);
        assert!(!format_of(&app, "A1").bold);

        app.undo();
        assert!(format_of(&app, "B2").bold);
        app.undo();
        assert!(format_of(&app, "B2").fill.is_none());
        app.redo();
        assert!(format_of(&app, "B2").fill.is_some());

        app.handle_format_action(FormatAction::Clear);
        assert!(format_of(&app, "A1").is_default());
        assert!(
            app.engine
                .formatting(0)
                .is_none_or(|f| f.cells().next().is_none())
        );
    }

    #[test]
    fn outside_borders_follow_the_selection_edges() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        select(&mut app, "B2", "C3");
        app.handle_format_action(FormatAction::Borders(BorderPreset::Outside));
        let b2 = format_of(&app, "B2").borders;
        assert!(b2.top && b2.left && !b2.right && !b2.bottom);
        let c3 = format_of(&app, "C3").borders;
        assert!(c3.bottom && c3.right && !c3.top && !c3.left);
    }

    #[test]
    fn typed_percent_and_dates_get_a_format_and_undo_together() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        let a1 = CellCoord::from_a1("A1").unwrap();
        app.set_cell_content(a1, "12%");
        assert_eq!(app.engine.get_value(0, a1), CellResult::Value(0.12));
        assert_eq!(format_of(&app, "A1").number_format.as_deref(), Some("0%"));
        assert_eq!(app.get_cell_formula_or_value(a1), "12%");

        // One undo removes both the value and the format it brought.
        app.undo();
        assert_eq!(app.engine.get_value(0, a1), CellResult::Empty);
        assert!(format_of(&app, "A1").number_format.is_none());
        app.redo();
        assert_eq!(app.engine.get_value(0, a1), CellResult::Value(0.12));
        assert_eq!(format_of(&app, "A1").number_format.as_deref(), Some("0%"));

        let b1 = CellCoord::from_a1("B1").unwrap();
        app.set_cell_content(b1, "2023-03-15");
        assert_eq!(app.engine.get_value(0, b1), CellResult::Value(45000.0));
        assert_eq!(app.get_cell_formula_or_value(b1), "2023-03-15");

        // An explicit format is kept when a decorated number is typed.
        let c1 = CellCoord::from_a1("C1").unwrap();
        select(&mut app, "C1", "C1");
        app.handle_format_action(FormatAction::NumberFormat(Some("0.00".into())));
        app.set_cell_content(c1, "50%");
        assert_eq!(format_of(&app, "C1").number_format.as_deref(), Some("0.00"));

        // Text-formatted cells keep digits as text.
        let d1 = CellCoord::from_a1("D1").unwrap();
        select(&mut app, "D1", "D1");
        app.handle_format_action(FormatAction::NumberFormat(Some("@".into())));
        app.set_cell_content(d1, "00123");
        assert_eq!(
            app.engine.get_value(0, d1),
            CellResult::Text("00123".into())
        );
    }

    #[test]
    fn resizing_updates_the_grid_and_undoes() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        app.set_size(0, ResizeAxis::Column, 2, Some(150.0));
        assert_eq!(app.grid_config.column_width(2), 150.0);
        app.undo_history.push(UndoAction::Resize {
            sheet: 0,
            axis: ResizeAxis::Column,
            index: 2,
            old: None,
            new: Some(150.0),
        });
        app.undo();
        assert_eq!(app.grid_config.column_width(2), DEFAULT_COLUMN_WIDTH);
        app.redo();
        assert_eq!(app.grid_config.column_width(2), 150.0);
    }

    #[test]
    fn sheets_keep_their_own_sizes_and_formats() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        app.set_size(0, ResizeAxis::Row, 0, Some(40.0));
        app.handle_format_action(FormatAction::ToggleItalic);
        app.add_sheet();
        assert_eq!(app.current_sheet, 1);
        assert_eq!(app.grid_config.row_height(0), DEFAULT_ROW_HEIGHT);
        assert!(!format_of(&app, "A1").italic);
        app.switch_sheet(0);
        assert_eq!(app.grid_config.row_height(0), 40.0);
        assert!(format_of(&app, "A1").italic);

        // Deleting the first sheet moves the second sheet's (empty) formatting down.
        app.delete_sheet(0);
        assert!(!format_of(&app, "A1").italic);
        assert_eq!(app.grid_config.row_height(0), DEFAULT_ROW_HEIGHT);
    }

    #[test]
    #[cfg(feature = "xlsx")]
    fn formatting_survives_save_and_open() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        app.set_cell_content(CellCoord::from_a1("A1").unwrap(), "Total");
        app.set_cell_content(CellCoord::from_a1("B1").unwrap(), "$1,234.50");
        select(&mut app, "A1", "B1");
        app.handle_format_action(FormatAction::ToggleBold);
        app.handle_format_action(FormatAction::Borders(BorderPreset::Bottom));
        app.set_size(0, ResizeAxis::Column, 0, Some(120.0));

        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustsheet_gui_fmt_{}_{}.xlsx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        app.save_to_path(&path);
        app.new_workbook();
        assert!(!format_of(&app, "A1").bold);
        app.load_file(&path);
        let _ = std::fs::remove_file(&path);

        let a1 = format_of(&app, "A1");
        assert!(a1.bold && a1.borders.bottom);
        let b1 = format_of(&app, "B1");
        assert_eq!(b1.number_format.as_deref(), Some("$#,##0.00"));
        assert!(b1.bold);
        assert!((app.grid_config.column_width(0) - 120.0).abs() < 2.0);
        assert_eq!(
            app.engine.get_value(0, CellCoord::from_a1("B1").unwrap()),
            CellResult::Value(1234.5)
        );
    }

    fn cell(app: &SpreadsheetApp, a1: &str) -> CellResult {
        app.engine
            .get_value(app.current_sheet, CellCoord::from_a1(a1).unwrap())
    }

    fn put(app: &mut SpreadsheetApp, a1: &str, content: &str) {
        app.set_cell_content(CellCoord::from_a1(a1).unwrap(), content);
    }

    /// What the system clipboard would hand back for the last copy.
    fn clipboard_text(app: &SpreadsheetApp) -> String {
        app.clipboard.as_ref().unwrap().text.clone()
    }

    #[test]
    fn tsv_round_trips_awkward_fields() {
        let fields = ["plain", "tab\there", "two\nlines", "say \"hi\""];
        let line: Vec<String> = fields.iter().map(|f| tsv_field(f)).collect();
        let text = line.join("\t") + "\r\n";
        assert_eq!(parse_tsv(&text), vec![fields.map(String::from).to_vec()]);
        assert_eq!(
            parse_tsv("a\tb\r\nc\td\r\n"),
            vec![
                vec!["a".to_string(), "b".into()],
                vec!["c".into(), "d".into()]
            ]
        );
        assert_eq!(
            parse_tsv("x\t\ty"),
            vec![vec!["x".to_string(), "".into(), "y".into()]]
        );
    }

    #[test]
    fn copy_paste_shifts_formulas_and_keeps_formats() {
        let ctx = egui::Context::default();
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "5");
        put(&mut app, "B1", "=A1*2");
        put(&mut app, "A2", "7");
        put(&mut app, "B2", "=$A$1+A2");
        select(&mut app, "A1", "B2");
        app.handle_format_action(FormatAction::ToggleBold);
        app.copy_selection(&ctx, false);
        // The system clipboard gets what is on screen.
        assert_eq!(clipboard_text(&app), "5\t10\r\n7\t12\r\n");

        select(&mut app, "C4", "C4");
        let text = clipboard_text(&app);
        app.paste_text(&text);
        assert_eq!(
            app.engine
                .get_formula(0, CellCoord::from_a1("D4").unwrap())
                .as_deref(),
            Some("=(C4*2)")
        );
        // =$A$1+C5: the absolute part stays, the relative part moves.
        assert_eq!(cell(&app, "D5"), CellResult::Value(12.0));
        assert!(format_of(&app, "D5").bold);
        // The pasted block is selected.
        assert_eq!(
            app.selection.primary_range(),
            CellRange::from_a1("C4:D5").unwrap()
        );

        // One undo removes the whole paste.
        app.undo();
        assert_eq!(cell(&app, "D4"), CellResult::Empty);
        assert!(!format_of(&app, "C4").bold);
        app.redo();
        assert_eq!(cell(&app, "C4"), CellResult::Value(5.0));
    }

    #[test]
    fn formulas_pushed_off_the_sheet_become_ref_errors() {
        let ctx = egui::Context::default();
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "B2", "=A1");
        select(&mut app, "B2", "B2");
        app.copy_selection(&ctx, false);
        select(&mut app, "A1", "A1");
        let text = clipboard_text(&app);
        app.paste_text(&text);
        assert_eq!(cell(&app, "A1"), CellResult::Error(CellError::Ref));
    }

    #[test]
    fn cut_paste_moves_cells_without_rewriting_references() {
        let ctx = egui::Context::default();
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "3");
        put(&mut app, "A2", "=A1+1");
        select(&mut app, "A2", "A2");
        app.handle_format_action(FormatAction::ToggleItalic);
        app.copy_selection(&ctx, true);
        select(&mut app, "C5", "C5");
        let text = clipboard_text(&app);
        app.paste_text(&text);

        assert_eq!(cell(&app, "C5"), CellResult::Value(4.0));
        assert_eq!(
            app.engine
                .get_formula(0, CellCoord::from_a1("C5").unwrap())
                .as_deref(),
            Some("=A1+1")
        );
        assert!(format_of(&app, "C5").italic);
        assert_eq!(cell(&app, "A2"), CellResult::Empty);
        assert!(!format_of(&app, "A2").italic);
        // A cut pastes once.
        assert!(app.clipboard.is_none());

        app.undo();
        assert_eq!(cell(&app, "A2"), CellResult::Value(4.0));
        assert!(format_of(&app, "A2").italic);
        assert_eq!(cell(&app, "C5"), CellResult::Empty);
    }

    #[test]
    fn one_copied_cell_fills_the_selection() {
        let ctx = egui::Context::default();
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "1");
        put(&mut app, "B1", "=A1*10");
        select(&mut app, "B1", "B1");
        app.copy_selection(&ctx, false);
        put(&mut app, "A2", "2");
        put(&mut app, "A3", "3");
        select(&mut app, "B2", "B3");
        let text = clipboard_text(&app);
        app.paste_text(&text);
        assert_eq!(cell(&app, "B2"), CellResult::Value(20.0));
        assert_eq!(cell(&app, "B3"), CellResult::Value(30.0));
    }

    #[test]
    fn text_from_other_apps_is_entered_as_typed() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "old");
        select(&mut app, "A1", "A1");
        // Excel copies displayed values, tab-separated, with CRLF lines.
        app.paste_text("Item\tCost\r\nTea\t$4.50\r\nShare\t12%\r\n\"two\nlines\"\t=1+1\r\n");
        assert_eq!(cell(&app, "A1"), CellResult::Text("Item".into()));
        assert_eq!(cell(&app, "B2"), CellResult::Value(4.5));
        assert_eq!(
            format_of(&app, "B2").number_format.as_deref(),
            Some("$#,##0.00")
        );
        assert_eq!(cell(&app, "B3"), CellResult::Value(0.12));
        assert_eq!(cell(&app, "A4"), CellResult::Text("two\nlines".into()));
        assert_eq!(cell(&app, "B4"), CellResult::Value(2.0));

        app.undo();
        assert_eq!(cell(&app, "A1"), CellResult::Text("old".into()));
        assert_eq!(cell(&app, "B2"), CellResult::Empty);
        assert!(format_of(&app, "B2").number_format.is_none());
    }

    #[test]
    fn delete_clears_the_whole_selection_but_keeps_formats() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        for (a1, v) in [("A1", "1"), ("B1", "2"), ("A2", "=A1+B1"), ("C3", "keep")] {
            put(&mut app, a1, v);
        }
        select(&mut app, "A1", "B2");
        app.handle_format_action(FormatAction::ToggleBold);
        app.delete_selection();
        for a1 in ["A1", "B1", "A2"] {
            assert_eq!(cell(&app, a1), CellResult::Empty, "{a1}");
        }
        assert_eq!(cell(&app, "C3"), CellResult::Text("keep".into()));
        assert!(format_of(&app, "A1").bold);

        app.undo();
        assert_eq!(cell(&app, "A2"), CellResult::Value(3.0));
        assert_eq!(
            app.engine
                .get_formula(0, CellCoord::from_a1("A2").unwrap())
                .as_deref(),
            Some("=A1+B1")
        );
    }

    #[test]
    fn stale_internal_clipboard_is_ignored() {
        let ctx = egui::Context::default();
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "=1+1");
        select(&mut app, "A1", "A1");
        app.copy_selection(&ctx, false);
        // Something else was copied since: paste that text, not the formula.
        select(&mut app, "B1", "B1");
        app.paste_text("hello");
        assert_eq!(cell(&app, "B1"), CellResult::Text("hello".into()));
    }

    /// Run one egui frame with `events`, `focus` holding keyboard focus.
    fn frame_with(
        app: &mut SpreadsheetApp,
        events: Vec<egui::Event>,
        focus: &str,
    ) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        ctx.run(input, |ctx| {
            let id = egui::Id::new(focus);
            if focus != "spreadsheet_grid" {
                // A real text field has stored text-edit state.
                egui::text_edit::TextEditState::default().store(ctx, id);
            }
            ctx.memory_mut(|m| m.request_focus(id));
            app.handle_clipboard_events(ctx);
        })
    }

    #[test]
    fn keyboard_copy_and_paste_reach_the_grid() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        put(&mut app, "A1", "42");
        select(&mut app, "A1", "A1");

        let out = frame_with(&mut app, vec![egui::Event::Copy], "spreadsheet_grid");
        let copied: Vec<&String> = out
            .platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(copied, vec!["42\r\n"]);

        select(&mut app, "B2", "B2");
        frame_with(
            &mut app,
            vec![egui::Event::Paste("42\n".into())],
            "spreadsheet_grid",
        );
        assert_eq!(cell(&app, "B2"), CellResult::Value(42.0));
    }

    #[test]
    fn keyboard_paste_is_left_to_a_focused_text_field() {
        let mut app = SpreadsheetApp::new();
        app.new_workbook();
        select(&mut app, "A1", "A1");
        frame_with(
            &mut app,
            vec![egui::Event::Paste("oops".into())],
            "sheet_rename_box",
        );
        assert_eq!(cell(&app, "A1"), CellResult::Empty);
    }
}
