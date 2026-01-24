//! Formula bar widget with autocomplete

use eframe::egui::{self, Ui, TextEdit, RichText, Key, Color32, ScrollArea};
use crate::cell::CellCoord;
use super::theme::Theme;
use super::functions_help::{self, FunctionInfo};

/// State for the formula bar
pub struct FormulaBar {
    /// Current cell address display (e.g., "A1")
    pub cell_address: String,
    /// Content being edited
    pub content: String,
    /// Whether we're in edit mode
    pub editing: bool,
    /// Original content before editing (for cancel)
    pub original_content: String,
    /// Autocomplete state
    pub autocomplete: AutocompleteState,
}

/// Autocomplete popup state
pub struct AutocompleteState {
    /// Whether autocomplete popup is visible
    pub visible: bool,
    /// Current suggestions
    pub suggestions: Vec<&'static FunctionInfo>,
    /// Currently selected index
    pub selected_index: usize,
    /// Position in content where the function name starts
    pub trigger_position: usize,
}

impl Default for AutocompleteState {
    fn default() -> Self {
        Self {
            visible: false,
            suggestions: Vec::new(),
            selected_index: 0,
            trigger_position: 0,
        }
    }
}

impl FormulaBar {
    pub fn new() -> Self {
        Self {
            cell_address: "A1".to_string(),
            content: String::new(),
            editing: false,
            original_content: String::new(),
            autocomplete: AutocompleteState::default(),
        }
    }

    /// Update the cell address display
    pub fn set_cell(&mut self, coord: CellCoord) {
        self.cell_address = coord.to_a1();
    }

    /// Set the content to display
    pub fn set_content(&mut self, content: String) {
        if !self.editing {
            self.content = content;
        }
    }

    /// Start editing mode
    pub fn start_editing(&mut self) {
        self.editing = true;
        self.original_content = self.content.clone();
        self.autocomplete = AutocompleteState::default();
    }

    /// Cancel editing and restore original content
    pub fn cancel_editing(&mut self) {
        self.editing = false;
        self.content = self.original_content.clone();
        self.autocomplete = AutocompleteState::default();
    }

    /// Confirm editing and return the new content
    pub fn confirm_editing(&mut self) -> String {
        self.editing = false;
        self.autocomplete = AutocompleteState::default();
        self.content.clone()
    }

    /// Update autocomplete suggestions based on current cursor position
    fn update_autocomplete(&mut self, cursor_pos: usize) {
        // Only trigger autocomplete in formulas
        if !self.content.starts_with('=') {
            self.autocomplete.visible = false;
            return;
        }

        // Find the start of the current "word" (function name)
        let content_before_cursor = &self.content[..cursor_pos.min(self.content.len())];

        // Look backwards for the start of a function name
        let mut start = cursor_pos;
        for (i, c) in content_before_cursor.char_indices().rev() {
            if c.is_alphabetic() || c == '_' {
                start = i;
            } else {
                break;
            }
        }

        let prefix = &self.content[start..cursor_pos.min(self.content.len())];

        if prefix.len() >= 1 && prefix.chars().all(|c| c.is_alphabetic() || c == '_') {
            let suggestions = functions_help::get_matching_functions(prefix);

            if !suggestions.is_empty() {
                self.autocomplete.visible = true;
                self.autocomplete.suggestions = suggestions;
                self.autocomplete.trigger_position = start;
                // Reset selection if it's out of bounds
                if self.autocomplete.selected_index >= self.autocomplete.suggestions.len() {
                    self.autocomplete.selected_index = 0;
                }
            } else {
                self.autocomplete.visible = false;
            }
        } else {
            self.autocomplete.visible = false;
        }
    }

    /// Apply the selected autocomplete suggestion
    fn apply_autocomplete(&mut self) {
        let func_name = self.autocomplete.suggestions
            .get(self.autocomplete.selected_index)
            .map(|f| f.name.to_string());

        if let Some(name) = func_name {
            // Find where the current partial function name ends
            let prefix_end = self.autocomplete.trigger_position;
            let mut end = prefix_end;
            for c in self.content[prefix_end..].chars() {
                if c.is_alphabetic() || c == '_' {
                    end += c.len_utf8();
                } else {
                    break;
                }
            }

            let before = self.content[..self.autocomplete.trigger_position].to_string();
            let after = self.content[end..].to_string();
            self.content = format!("{}{}({}", before, name, after);
        }
        // Clear everything to ensure popup closes immediately
        self.autocomplete.visible = false;
        self.autocomplete.suggestions.clear();
        self.autocomplete.selected_index = 0;
    }

    /// Render the formula bar
    pub fn show(&mut self, ui: &mut Ui, theme: &Theme) -> FormulaBarResponse {
        let mut response = FormulaBarResponse::default();

        // Track if we applied autocomplete this frame (to skip updating it)
        let mut applied_autocomplete = false;

        // IMPORTANT: Handle keys FIRST, before any widgets can consume them
        // This allows us to intercept Tab for autocomplete
        if self.editing {
            let key_response = self.handle_keys_internal(ui.ctx());
            if key_response.confirmed.is_some() || key_response.cancelled || key_response.consume_key {
                response.confirmed = key_response.confirmed;
                response.cancelled = key_response.cancelled;
                response.move_down = key_response.move_down;
                response.move_right = key_response.move_right;
                response.consume_key = key_response.consume_key;
                // If we consumed a key while autocomplete was visible, we applied it
                if !self.autocomplete.visible {
                    applied_autocomplete = true;
                }
            }
        }

        ui.horizontal(|ui| {
            // Cell address name box
            ui.add_sized(
                [60.0, 24.0],
                egui::Label::new(
                    RichText::new(&self.cell_address)
                        .monospace()
                        .color(theme.header_text)
                ),
            );

            ui.separator();

            // Function button (fx) - opens help
            if ui.button(RichText::new("fx").monospace()).on_hover_text("Insert function (or press F1 for help)").clicked() {
                response.open_help = true;
            }

            ui.separator();

            // Formula/content editor
            let text_edit = TextEdit::singleline(&mut self.content)
                .font(egui::TextStyle::Monospace)
                .desired_width(ui.available_width() - 10.0)
                .frame(true)
                .id_source("formula_bar_editor");

            let edit_response = ui.add(text_edit);
            response.has_focus = edit_response.has_focus();

            // CRITICAL: If we're in editing mode but don't have focus, request it
            // This ensures focus is acquired in the same frame as the state transition
            if self.editing && !edit_response.has_focus() {
                edit_response.request_focus();
            }

            // Handle focus and editing state
            // Only auto-start editing if user clicked on the text field (not if focus was set programmatically)
            if edit_response.gained_focus() && edit_response.clicked() {
                if !self.editing {
                    self.start_editing();
                }
            }
        });

        // Update autocomplete AFTER key handling (but skip if we just applied it)
        if response.has_focus && self.editing && !applied_autocomplete {
            let cursor_pos = self.content.len();
            self.update_autocomplete(cursor_pos);
        }

        // Show autocomplete popup (only if still visible)
        if self.autocomplete.visible && !self.autocomplete.suggestions.is_empty() && self.editing {
            self.show_autocomplete_popup(ui);
        }

        response
    }

    /// Internal key handling - called from show() before popup is rendered
    fn handle_keys_internal(&mut self, ctx: &egui::Context) -> FormulaBarResponse {
        let mut response = FormulaBarResponse::default();

        // Handle autocomplete navigation - use consume_key to intercept
        if self.autocomplete.visible && !self.autocomplete.suggestions.is_empty() {
            // Consume arrow keys for autocomplete navigation
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
                self.autocomplete.selected_index =
                    (self.autocomplete.selected_index + 1) % self.autocomplete.suggestions.len();
                response.consume_key = true;
                return response;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
                if self.autocomplete.selected_index > 0 {
                    self.autocomplete.selected_index -= 1;
                } else {
                    self.autocomplete.selected_index = self.autocomplete.suggestions.len().saturating_sub(1);
                }
                response.consume_key = true;
                return response;
            }
            // Tab to apply autocomplete
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab)) {
                self.apply_autocomplete();
                response.consume_key = true;
                return response;
            }
            // Enter to apply autocomplete
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                self.apply_autocomplete();
                response.consume_key = true;
                return response;
            }
            // Escape to close autocomplete
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                self.autocomplete.visible = false;
                self.autocomplete.suggestions.clear();
                response.consume_key = true;
                return response;
            }
        } else {
            // No autocomplete - handle normal key actions
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                response.confirmed = Some(self.confirm_editing());
                response.move_down = true;
                return response;
            }

            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab)) {
                response.confirmed = Some(self.confirm_editing());
                response.move_right = true;
                return response;
            }

            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                self.cancel_editing();
                response.cancelled = true;
                return response;
            }
        }

        response
    }

    fn show_autocomplete_popup(&mut self, ui: &mut Ui) {
        // Copy suggestions to avoid borrow issues
        let suggestions: Vec<_> = self.autocomplete.suggestions.iter().copied().collect();
        let selected_index = self.autocomplete.selected_index;
        let mut clicked_index: Option<usize> = None;

        // Use Area instead of Window for more immediate control
        egui::Area::new(egui::Id::new("autocomplete_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(70.0, 52.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| {
                        ui.set_max_height(200.0);
                        ui.set_max_width(350.0);

                        ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            for (i, func) in suggestions.iter().enumerate() {
                                let is_selected = i == selected_index;

                                let response = ui.selectable_label(
                                    is_selected,
                                    RichText::new(format!("{} - {}", func.name, truncate(func.description, 35)))
                                        .monospace()
                                );

                                if response.clicked() {
                                    clicked_index = Some(i);
                                }

                                // Show tooltip with full info
                                response.on_hover_ui(|ui| {
                                    ui.label(RichText::new(func.name).strong().monospace());
                                    ui.label(func.description);
                                    ui.separator();
                                    ui.label(RichText::new("Syntax:").small());
                                    ui.label(RichText::new(func.syntax).monospace().color(Color32::from_rgb(0, 100, 0)));
                                    if !func.examples.is_empty() {
                                        ui.label(RichText::new("Example:").small());
                                        ui.label(RichText::new(func.examples[0]).monospace().color(Color32::from_rgb(0, 0, 150)));
                                    }
                                });
                            }
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.small("Tab to insert | ↑↓ to navigate | Esc to close");
                        });
                    });
            });

        // Handle click outside the borrow
        if let Some(i) = clicked_index {
            self.autocomplete.selected_index = i;
            self.apply_autocomplete();
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

impl Default for FormulaBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from formula bar interaction
#[derive(Default)]
pub struct FormulaBarResponse {
    /// Content was confirmed (Enter pressed or focus lost)
    pub confirmed: Option<String>,
    /// Editing was cancelled (Escape pressed)
    pub cancelled: bool,
    /// Should move down after confirm
    pub move_down: bool,
    /// Should move right after confirm (Tab)
    pub move_right: bool,
    /// Key was consumed by autocomplete
    pub consume_key: bool,
    /// User wants to open help/function dialog
    pub open_help: bool,
    /// Formula bar has focus
    pub has_focus: bool,
}
