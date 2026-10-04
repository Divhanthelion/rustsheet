//! Help panel showing functions and keyboard shortcuts

use super::functions_help::{self, FunctionCategory, FunctionInfo};
use eframe::egui::{self, Color32, RichText, ScrollArea, Ui};

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
            ScrollArea::vertical()
                .id_salt("categories")
                .show(&mut columns[0], |ui| {
                    ui.heading("Categories");
                    ui.separator();

                    if ui
                        .selectable_label(self.selected_category.is_none(), "All Functions")
                        .clicked()
                    {
                        self.selected_category = None;
                    }

                    for category in FunctionCategory::all() {
                        if ui
                            .selectable_label(
                                self.selected_category == Some(*category),
                                category.name(),
                            )
                            .clicked()
                        {
                            self.selected_category = Some(*category);
                        }
                    }
                });

            // Right column: Function list and details
            ScrollArea::vertical()
                .id_salt("functions")
                .show(&mut columns[1], |ui| {
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
                            if !self.search_text.is_empty()
                                && !f.name.contains(&search_upper)
                                && !f.description.to_uppercase().contains(&search_upper)
                            {
                                return false;
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
                                if ui
                                    .link(RichText::new(func.name).strong().monospace())
                                    .clicked()
                                {
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
        let section = |ui: &mut Ui, title: &str, rows: &[(&str, &str)]| {
            ui.heading(title);
            ui.separator();
            for (keys, what) in rows {
                shortcut_row(ui, keys, what);
            }
            ui.add_space(10.0);
        };
        ScrollArea::vertical().show(ui, |ui| {
            section(
                ui,
                "Navigation",
                &[
                    ("Arrow keys", "Move one cell"),
                    ("Ctrl+Arrow", "Jump to the edge of the data"),
                    ("Home", "Go to column A"),
                    ("End", "Go to the last filled cell in the row"),
                    ("Ctrl+Home", "Go to the first cell (below frozen panes)"),
                    ("Ctrl+End", "Go to the last used cell"),
                    ("Page Up / Page Down", "Move one screen"),
                ],
            );
            section(
                ui,
                "Selection",
                &[
                    ("Shift+Arrow", "Extend the selection"),
                    ("Shift+Click", "Select from the active cell"),
                    (
                        "Click a row or column header",
                        "Select the row or column (drag for several)",
                    ),
                    ("Ctrl+A", "Select the data around the cell, then everything"),
                    ("Ctrl+Space", "Select the whole column"),
                    ("Shift+Space", "Select the whole row"),
                ],
            );
            section(
                ui,
                "Editing",
                &[
                    ("F2", "Edit the active cell"),
                    ("Type any character", "Start editing with that character"),
                    (
                        "Enter / Tab",
                        "Move down / right (confirms an edit); Shift goes back",
                    ),
                    ("Escape", "Cancel editing"),
                    (
                        "Delete or Backspace",
                        "Clear the selected cells (or delete the selected picture)",
                    ),
                    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
                    ("Ctrl+C / Ctrl+X", "Copy / cut"),
                    (
                        "Ctrl+V",
                        "Paste (from RustSheet, Excel, text, or a picture or screenshot)",
                    ),
                    ("Shift+F2", "Add or edit the cell's note"),
                    ("Alt+Down", "Open the cell's drop-down list"),
                    ("Ctrl+D / Ctrl+R", "Fill down / right"),
                    (
                        "Drag the corner square",
                        "Fill a series (1, 2, 3... Jan, Feb...)",
                    ),
                    ("Ctrl+F / Ctrl+H", "Find / Replace"),
                ],
            );
            section(
                ui,
                "Formulas",
                &[
                    ("= (equals)", "Start a formula"),
                    ("F4", "Toggle absolute/relative reference ($)"),
                    ("Tab (while typing)", "Accept the autocomplete suggestion"),
                    ("Arrow Up / Down (in autocomplete)", "Choose a suggestion"),
                ],
            );
            section(
                ui,
                "Rows and columns",
                &[
                    (
                        "Ctrl+Shift+=",
                        "Insert rows (or columns, if whole columns are selected)",
                    ),
                    ("Ctrl+-", "Delete rows (or columns)"),
                    ("Ctrl+9 / Ctrl+Shift+9", "Hide / unhide rows"),
                    ("Ctrl+0", "Hide columns"),
                    ("Drag a header border", "Resize a column or row"),
                    ("Double-click a header border", "Fit to contents"),
                ],
            );
            section(
                ui,
                "Formatting",
                &[
                    ("Ctrl+B / Ctrl+I / Ctrl+U", "Bold / italic / underline"),
                    (
                        "Type 12%, $5 or 2026-10-03",
                        "Enter a percent, amount or date",
                    ),
                ],
            );
            section(
                ui,
                "Data",
                &[
                    ("Ctrl+Shift+L", "Turn the filter on or off"),
                    ("Right-click > Sort A to Z", "Sort the data around the cell"),
                    ("Alt+F5", "Refresh the PivotTable at the cell"),
                    ("Ctrl+Alt+F5", "Refresh all PivotTables"),
                ],
            );
            section(
                ui,
                "Sheets and pictures",
                &[
                    ("Double-click a sheet tab", "Rename the sheet"),
                    ("Drag a picture", "Move it"),
                    ("Drag a picture's corner", "Resize it, keeping its shape"),
                    ("Right-click a picture", "Alt text, order, size, delete"),
                ],
            );
            section(
                ui,
                "Files",
                &[
                    ("Ctrl+N / Ctrl+O", "New / Open"),
                    ("Ctrl+S / Ctrl+Shift+S", "Save / Save As"),
                    ("Ctrl+P", "Print"),
                    ("F1", "Help"),
                ],
            );
        });
    }

    fn show_about_tab(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.heading(RichText::new("RustSheet").size(24.0).strong());
            ui.label("A fast spreadsheet with an Excel-compatible formula engine");
            ui.add_space(10.0);
            ui.label(concat!("Version ", env!("CARGO_PKG_VERSION")));
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
            ui.label("• Excel (.xlsx) formulas, formatting, and charts");
            ui.label("• Fonts, fills, borders, alignment, and number formats");
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
    ui.label(
        RichText::new(func.category.name())
            .italics()
            .color(Color32::GRAY),
    );

    ui.add_space(10.0);
    ui.label(func.description);

    ui.add_space(10.0);
    ui.label(RichText::new("Syntax:").strong());
    ui.label(
        RichText::new(func.syntax)
            .monospace()
            .color(Color32::from_rgb(0, 100, 0)),
    );

    ui.add_space(10.0);
    ui.label(RichText::new("Examples:").strong());
    for example in func.examples {
        ui.label(
            RichText::new(*example)
                .monospace()
                .color(Color32::from_rgb(200, 60, 60)),
        );
    }
}

fn shortcut_row(ui: &mut Ui, keys: &str, description: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(keys)
                .monospace()
                .strong()
                .color(Color32::from_rgb(80, 80, 80)),
        );
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
