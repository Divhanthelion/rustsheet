//! Help panel showing functions and keyboard shortcuts

use eframe::egui::{self, RichText, ScrollArea, Ui, Color32};
use super::functions_help::{self, FunctionCategory, FunctionInfo};

/// State for the help panel
pub struct HelpPanel {
    pub visible: bool,
    pub tab: HelpTab,
    pub search_text: String,
    pub selected_category: Option<FunctionCategory>,
    pub selected_function: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HelpTab {
    Functions,
    Shortcuts,
    About,
}

impl Default for HelpPanel {
    fn default() -> Self {
        Self {
            visible: false,
            tab: HelpTab::Functions,
            search_text: String::new(),
            selected_category: None,
            selected_function: None,
        }
    }
}

impl HelpPanel {
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        let mut open = self.visible;

        egui::Window::new("Help")
            .open(&mut open)
            .default_width(600.0)
            .default_height(500.0)
            .resizable(true)
            .show(ctx, |ui| {
                // Tab bar
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, HelpTab::Functions, "Functions");
                    ui.selectable_value(&mut self.tab, HelpTab::Shortcuts, "Keyboard Shortcuts");
                    ui.selectable_value(&mut self.tab, HelpTab::About, "About");
                });

                ui.separator();

                match self.tab {
                    HelpTab::Functions => self.show_functions_tab(ui),
                    HelpTab::Shortcuts => self.show_shortcuts_tab(ui),
                    HelpTab::About => self.show_about_tab(ui),
                }
            });

        self.visible = open;
    }

    fn show_functions_tab(&mut self, ui: &mut Ui) {
        // Search box
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.text_edit_singleline(&mut self.search_text);
            if ui.button("Clear").clicked() {
                self.search_text.clear();
            }
        });

        ui.separator();

        // Two-column layout: categories on left, function list/details on right
        ui.columns(2, |columns| {
            // Left column: Categories
            ScrollArea::vertical().id_salt("categories").show(&mut columns[0], |ui| {
                ui.heading("Categories");
                ui.separator();

                if ui.selectable_label(self.selected_category.is_none(), "All Functions").clicked() {
                    self.selected_category = None;
                }

                for category in FunctionCategory::all() {
                    if ui.selectable_label(
                        self.selected_category == Some(*category),
                        category.name()
                    ).clicked() {
                        self.selected_category = Some(*category);
                    }
                }
            });

            // Right column: Function list and details
            ScrollArea::vertical().id_salt("functions").show(&mut columns[1], |ui| {
                let search_upper = self.search_text.to_uppercase();
                let functions: Vec<&FunctionInfo> = functions_help::get_all_functions()
                    .iter()
                    .filter(|f| {
                        // Filter by category
                        if let Some(cat) = self.selected_category {
                            if f.category != cat {
                                return false;
                            }
                        }
                        // Filter by search
                        if !self.search_text.is_empty() {
                            if !f.name.contains(&search_upper) &&
                               !f.description.to_uppercase().contains(&search_upper) {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();

                // Show selected function details or list
                if let Some(func_name) = self.selected_function {
                    if let Some(func) = functions_help::get_function(func_name) {
                        if ui.button("← Back to list").clicked() {
                            self.selected_function = None;
                        }
                        ui.separator();
                        show_function_details(ui, func);
                    } else {
                        self.selected_function = None;
                    }
                } else {
                    ui.heading(format!("Functions ({})", functions.len()));
                    ui.separator();

                    for func in functions {
                        ui.horizontal(|ui| {
                            if ui.link(RichText::new(func.name).strong().monospace()).clicked() {
                                self.selected_function = Some(func.name);
                            }
                            ui.label(format!("- {}", truncate(func.description, 40)));
                        });
                    }
                }
            });
        });
    }

    fn show_shortcuts_tab(&mut self, ui: &mut Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Navigation");
            ui.separator();
            shortcut_row(ui, "Arrow Keys", "Move one cell in direction");
            shortcut_row(ui, "Tab", "Move right, confirm edit");
            shortcut_row(ui, "Enter", "Move down, confirm edit");
            shortcut_row(ui, "Ctrl+Home", "Go to cell A1");
            shortcut_row(ui, "Ctrl+End", "Go to last used cell");
            shortcut_row(ui, "Ctrl+Arrow", "Jump to edge of data region");
            shortcut_row(ui, "Page Up/Down", "Scroll one screen");
            shortcut_row(ui, "Home", "Go to column A in current row");
            shortcut_row(ui, "End", "Go to last column in current row");

            ui.add_space(10.0);
            ui.heading("Selection");
            ui.separator();
            shortcut_row(ui, "Shift+Arrow", "Extend selection");
            shortcut_row(ui, "Shift+Click", "Select range from active cell");
            shortcut_row(ui, "Ctrl+A", "Select all cells");
            shortcut_row(ui, "Ctrl+Space", "Select entire column");
            shortcut_row(ui, "Shift+Space", "Select entire row");

            ui.add_space(10.0);
            ui.heading("Editing");
            ui.separator();
            shortcut_row(ui, "F2", "Edit current cell");
            shortcut_row(ui, "Enter", "Confirm edit and move down");
            shortcut_row(ui, "Tab", "Confirm edit and move right");
            shortcut_row(ui, "Escape", "Cancel editing");
            shortcut_row(ui, "Delete", "Clear cell contents");
            shortcut_row(ui, "Backspace", "Clear cell and start editing");
            shortcut_row(ui, "Type any character", "Start editing with that character");

            ui.add_space(10.0);
            ui.heading("Formulas");
            ui.separator();
            shortcut_row(ui, "= (equals)", "Start formula entry");
            shortcut_row(ui, "F4", "Toggle absolute/relative reference ($)");
            shortcut_row(ui, "Tab (while typing)", "Accept autocomplete suggestion");
            shortcut_row(ui, "Arrow Down (in autocomplete)", "Select next suggestion");
            shortcut_row(ui, "Arrow Up (in autocomplete)", "Select previous suggestion");

            ui.add_space(10.0);
            ui.heading("File Operations");
            ui.separator();
            shortcut_row(ui, "Ctrl+S", "Save");
            shortcut_row(ui, "Ctrl+O", "Open");
            shortcut_row(ui, "Ctrl+N", "New");
            shortcut_row(ui, "Ctrl+Z", "Undo");
            shortcut_row(ui, "Ctrl+Y", "Redo");

            ui.add_space(10.0);
            ui.heading("View");
            ui.separator();
            shortcut_row(ui, "F1", "Open Help");
            shortcut_row(ui, "Ctrl+`", "Show/hide formulas");
        });
    }

    fn show_about_tab(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.heading(RichText::new("RustSheet").size(24.0).strong());
            ui.label("High-Performance Spreadsheet Engine");
            ui.add_space(10.0);
            ui.label("Version 0.1.0");
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label("Built with Rust and egui");
            ui.add_space(20.0);

            ui.heading("Features");
            ui.label(format!(
                "• {} Excel-compatible functions",
                functions_help::get_all_functions().len()
            ));
            ui.label("• Formula parsing with dependency tracking and cycle detection");
            ui.label("• Cross-sheet references; sheet delete remaps cells");
            ui.label("• Excel (.xlsx) formulas, used range, and charts");
            ui.label("• CSV import/export of the current sheet");
            ui.label("• Cross-platform (Windows, macOS, Linux)");
            ui.add_space(20.0);

            ui.heading("Supported Function Categories");
            for category in FunctionCategory::all() {
                let count = functions_help::get_all_functions()
                    .iter()
                    .filter(|f| f.category == *category)
                    .count();
                ui.label(format!("• {} ({} functions)", category.name(), count));
            }
        });
    }
}

fn show_function_details(ui: &mut Ui, func: &FunctionInfo) {
    ui.heading(RichText::new(func.name).monospace().size(18.0));
    ui.label(RichText::new(func.category.name()).italics().color(Color32::GRAY));

    ui.add_space(10.0);
    ui.label(func.description);

    ui.add_space(10.0);
    ui.label(RichText::new("Syntax:").strong());
    ui.label(RichText::new(func.syntax).monospace().color(Color32::from_rgb(0, 100, 0)));

    ui.add_space(10.0);
    ui.label(RichText::new("Examples:").strong());
    for example in func.examples {
        ui.label(RichText::new(*example).monospace().color(Color32::from_rgb(200, 60, 60)));
    }
}

fn shortcut_row(ui: &mut Ui, keys: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(keys).monospace().strong().color(Color32::from_rgb(80, 80, 80)));
        ui.label("-");
        ui.label(description);
    });
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
