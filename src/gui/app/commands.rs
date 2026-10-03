//! Menu bar and keyboard shortcuts. Both produce a [`Command`], which
//! [`SpreadsheetApp::run_command`] carries out, so menus, shortcuts and the
//! right-click menus share one code path.

use super::*;
use crate::cell::{Axis, MAX_COL, MAX_ROW};
use crate::gui::grid::ContextAction;
use crate::gui::settings::ThemeChoice;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Command {
    New,
    Open,
    OpenRecent(PathBuf),
    Save,
    SaveAs,
    ExportPdf,
    Print,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    ClearContents,
    SelectAll,
    Find,
    Replace,
    FillDown,
    FillRight,
    Insert(Axis),
    Delete(Axis),
    Hide(Axis),
    Unhide(Axis),
    SortAscending,
    SortDescending,
    SortDialog,
    ToggleFilter,
    FreezePanes,
    FreezeTopRow,
    FreezeFirstColumn,
    Unfreeze,
    InsertChart,
    EditNote,
    DeleteNote,
    DataValidation,
    OpenList,
    Format(FormatAction),
    Theme(ThemeChoice),
    Help,
    About,
}

impl From<ContextAction> for Command {
    fn from(a: ContextAction) -> Self {
        match a {
            ContextAction::Cut => Command::Cut,
            ContextAction::Copy => Command::Copy,
            ContextAction::Paste => Command::Paste,
            ContextAction::ClearContents => Command::ClearContents,
            ContextAction::ClearFormatting => Command::Format(FormatAction::Clear),
            ContextAction::Insert(axis) => Command::Insert(axis),
            ContextAction::Delete(axis) => Command::Delete(axis),
            ContextAction::Hide(axis) => Command::Hide(axis),
            ContextAction::Unhide(axis) => Command::Unhide(axis),
            ContextAction::EditNote => Command::EditNote,
            ContextAction::DeleteNote => Command::DeleteNote,
            ContextAction::SortAscending => Command::SortAscending,
            ContextAction::SortDescending => Command::SortDescending,
            ContextAction::ToggleFilter => Command::ToggleFilter,
        }
    }
}

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, cmd: Command, out: &mut Option<Command>) {
    item_enabled(ui, true, label, shortcut, cmd, out);
}

fn item_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    shortcut: &str,
    cmd: Command,
    out: &mut Option<Command>,
) {
    let shortcut = shortcut.replace("Ctrl", MOD);
    let button = egui::Button::new(label).shortcut_text(shortcut);
    if ui.add_enabled(enabled, button).clicked() {
        *out = Some(cmd);
        ui.close_menu();
    }
}

impl SpreadsheetApp {
    /// The menu bar; returns the command picked, if any.
    pub(super) fn menu_bar(&self, ui: &mut egui::Ui) -> Option<Command> {
        let mut out = None;
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                item(ui, "New", "Ctrl+N", Command::New, &mut out);
                item(ui, "Open...", "Ctrl+O", Command::Open, &mut out);
                let recent = &self.settings.recent_files;
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button("Open Recent", |ui| {
                        for path in recent {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            let r = ui.button(name).on_hover_text(path.display().to_string());
                            if r.clicked() {
                                out = Some(Command::OpenRecent(path.clone()));
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.separator();
                item(ui, "Save", "Ctrl+S", Command::Save, &mut out);
                item(ui, "Save As...", "Ctrl+Shift+S", Command::SaveAs, &mut out);
                ui.separator();
                item(ui, "Export as PDF...", "", Command::ExportPdf, &mut out);
                #[cfg(windows)]
                item(ui, "Print...", "Ctrl+P", Command::Print, &mut out);
            });

            ui.menu_button("Edit", |ui| {
                item_enabled(
                    ui,
                    self.undo_history.can_undo(),
                    "Undo",
                    "Ctrl+Z",
                    Command::Undo,
                    &mut out,
                );
                item_enabled(
                    ui,
                    self.undo_history.can_redo(),
                    "Redo",
                    "Ctrl+Y",
                    Command::Redo,
                    &mut out,
                );
                ui.separator();
                item(ui, "Cut", "Ctrl+X", Command::Cut, &mut out);
                item(ui, "Copy", "Ctrl+C", Command::Copy, &mut out);
                item(ui, "Paste", "Ctrl+V", Command::Paste, &mut out);
                item(
                    ui,
                    "Clear Contents",
                    "Del",
                    Command::ClearContents,
                    &mut out,
                );
                ui.separator();
                item(ui, "Fill Down", "Ctrl+D", Command::FillDown, &mut out);
                item(ui, "Fill Right", "Ctrl+R", Command::FillRight, &mut out);
                ui.separator();
                item(ui, "Find...", "Ctrl+F", Command::Find, &mut out);
                item(ui, "Replace...", "Ctrl+H", Command::Replace, &mut out);
                ui.separator();
                item(ui, "Select All", "Ctrl+A", Command::SelectAll, &mut out);
            });

            ui.menu_button("Insert", |ui| {
                item(
                    ui,
                    "Rows",
                    "Ctrl+Shift+=",
                    Command::Insert(Axis::Row),
                    &mut out,
                );
                item(ui, "Columns", "", Command::Insert(Axis::Column), &mut out);
                ui.separator();
                item(ui, "Chart...", "", Command::InsertChart, &mut out);
                item(ui, "Note", "Shift+F2", Command::EditNote, &mut out);
            });

            ui.menu_button("Format", |ui| {
                item(
                    ui,
                    "Bold",
                    "Ctrl+B",
                    Command::Format(FormatAction::ToggleBold),
                    &mut out,
                );
                item(
                    ui,
                    "Italic",
                    "Ctrl+I",
                    Command::Format(FormatAction::ToggleItalic),
                    &mut out,
                );
                item(
                    ui,
                    "Underline",
                    "Ctrl+U",
                    Command::Format(FormatAction::ToggleUnderline),
                    &mut out,
                );
                item(
                    ui,
                    "Strikethrough",
                    "",
                    Command::Format(FormatAction::ToggleStrikethrough),
                    &mut out,
                );
                ui.separator();
                item(
                    ui,
                    "Wrap Text",
                    "",
                    Command::Format(FormatAction::ToggleWrap),
                    &mut out,
                );
                item(
                    ui,
                    "Merge & Center",
                    "",
                    Command::Format(FormatAction::Merge),
                    &mut out,
                );
                ui.separator();
                ui.menu_button("Rows", |ui| {
                    item(ui, "Hide", "Ctrl+9", Command::Hide(Axis::Row), &mut out);
                    item(
                        ui,
                        "Unhide",
                        "Ctrl+Shift+9",
                        Command::Unhide(Axis::Row),
                        &mut out,
                    );
                    item(ui, "Delete", "Ctrl+-", Command::Delete(Axis::Row), &mut out);
                });
                ui.menu_button("Columns", |ui| {
                    item(ui, "Hide", "Ctrl+0", Command::Hide(Axis::Column), &mut out);
                    item(ui, "Unhide", "", Command::Unhide(Axis::Column), &mut out);
                    item(ui, "Delete", "", Command::Delete(Axis::Column), &mut out);
                });
                ui.separator();
                item(
                    ui,
                    "Clear Formatting",
                    "",
                    Command::Format(FormatAction::Clear),
                    &mut out,
                );
            });

            ui.menu_button("Data", |ui| {
                item(ui, "Sort A to Z", "", Command::SortAscending, &mut out);
                item(ui, "Sort Z to A", "", Command::SortDescending, &mut out);
                item(ui, "Sort...", "", Command::SortDialog, &mut out);
                ui.separator();
                let on = self
                    .engine
                    .formatting(self.current_sheet)
                    .is_some_and(|f| f.filter.is_some());
                let label = if on { "Remove Filter" } else { "Filter" };
                item(ui, label, "Ctrl+Shift+L", Command::ToggleFilter, &mut out);
                ui.separator();
                item(
                    ui,
                    "Data Validation...",
                    "",
                    Command::DataValidation,
                    &mut out,
                );
            });

            ui.menu_button("View", |ui| {
                let frozen = self.grid_config.frozen_rows > 0 || self.grid_config.frozen_cols > 0;
                if frozen {
                    item(ui, "Unfreeze Panes", "", Command::Unfreeze, &mut out);
                } else {
                    item(ui, "Freeze Panes", "", Command::FreezePanes, &mut out);
                }
                item(ui, "Freeze Top Row", "", Command::FreezeTopRow, &mut out);
                item(
                    ui,
                    "Freeze First Column",
                    "",
                    Command::FreezeFirstColumn,
                    &mut out,
                );
                ui.separator();
                for (label, choice) in [
                    ("Use System Theme", ThemeChoice::System),
                    ("Light Theme", ThemeChoice::Light),
                    ("Dark Theme", ThemeChoice::Dark),
                ] {
                    let checked = self.settings.theme == choice;
                    if ui.radio(checked, label).clicked() {
                        out = Some(Command::Theme(choice));
                        ui.close_menu();
                    }
                }
            });

            ui.menu_button("Help", |ui| {
                item(ui, "Help", "F1", Command::Help, &mut out);
                ui.separator();
                item(ui, "About", "", Command::About, &mut out);
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
        out
    }

    /// Keyboard shortcuts. `typing` is true while a text field (formula
    /// bar, dialog) has focus; only file shortcuts work then.
    pub(super) fn shortcut(&self, ctx: &egui::Context, typing: bool) -> Option<Command> {
        use egui::{Key, Modifiers};
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let take = |mods: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(mods, key));

        // Most specific first: Ctrl+Shift+S before Ctrl+S.
        if take(cmd_shift, Key::S) {
            return Some(Command::SaveAs);
        }
        if take(cmd, Key::S) {
            return Some(Command::Save);
        }
        if take(cmd, Key::O) {
            return Some(Command::Open);
        }
        if take(cmd, Key::N) {
            return Some(Command::New);
        }
        if typing || self.is_editing() {
            return None;
        }
        #[cfg(windows)]
        if take(cmd, Key::P) {
            return Some(Command::Print);
        }
        if take(cmd_shift, Key::Z) || take(cmd, Key::Y) {
            return Some(Command::Redo);
        }
        if take(cmd, Key::Z) {
            return Some(Command::Undo);
        }
        if take(Modifiers::ALT, Key::ArrowDown) {
            return Some(Command::OpenList);
        }
        if take(Modifiers::SHIFT, Key::F2) {
            return Some(Command::EditNote);
        }
        if take(cmd_shift, Key::L) {
            return Some(Command::ToggleFilter);
        }
        if take(cmd, Key::F) {
            return Some(Command::Find);
        }
        if take(cmd, Key::H) {
            return Some(Command::Replace);
        }
        if take(cmd, Key::D) {
            return Some(Command::FillDown);
        }
        if take(cmd, Key::R) {
            return Some(Command::FillRight);
        }
        if take(cmd_shift, Key::Num9) {
            return Some(Command::Unhide(Axis::Row));
        }
        if take(cmd, Key::Num9) {
            return Some(Command::Hide(Axis::Row));
        }
        if take(cmd, Key::Num0) {
            return Some(Command::Hide(Axis::Column));
        }
        // Insert/delete: whole columns selected means columns, else rows.
        let r = self.selection.primary_range();
        let axis = if r.start.row == 0 && r.end.row == MAX_ROW && r.end.col != MAX_COL {
            Axis::Column
        } else {
            Axis::Row
        };
        if take(cmd_shift, Key::Equals) || take(cmd_shift, Key::Plus) || take(cmd, Key::Plus) {
            return Some(Command::Insert(axis));
        }
        if take(cmd, Key::Minus) {
            return Some(Command::Delete(axis));
        }
        for (key, action) in [
            (Key::B, FormatAction::ToggleBold),
            (Key::I, FormatAction::ToggleItalic),
            (Key::U, FormatAction::ToggleUnderline),
        ] {
            if take(cmd, key) {
                return Some(Command::Format(action));
            }
        }
        if ctx.input(|i| i.key_pressed(Key::F1)) {
            return Some(Command::Help);
        }
        None
    }

    pub(super) fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::New => self.request_new_workbook(),
            Command::Open => self.open_file(),
            Command::OpenRecent(path) => {
                if self.confirm_discard() {
                    self.load_file(&path);
                }
            }
            Command::Save => self.save_file(),
            Command::SaveAs => self.save_file_as(),
            Command::ExportPdf => self.export_pdf(),
            Command::Print => self.print(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Cut => self.copy_selection(ctx, true),
            Command::Copy => self.copy_selection(ctx, false),
            Command::Paste => self.paste_from_system_clipboard(),
            Command::ClearContents => self.delete_selection(),
            Command::SelectAll => self.select_all_or_region(),
            Command::Find => self.open_find(false),
            Command::Replace => self.open_find(true),
            Command::FillDown => self.fill_down(),
            Command::FillRight => self.fill_right(),
            Command::Insert(axis) => self.insert_lines(axis),
            Command::Delete(axis) => self.delete_lines(axis),
            Command::Hide(axis) => self.set_lines_hidden(axis, true),
            Command::Unhide(axis) => self.set_lines_hidden(axis, false),
            Command::SortAscending => self.quick_sort(true),
            Command::SortDescending => self.quick_sort(false),
            Command::SortDialog => self.open_sort_dialog(),
            Command::ToggleFilter => self.toggle_filter(),
            Command::FreezePanes => {
                let a = self.selection.active;
                self.freeze(a.row, a.col);
            }
            Command::FreezeTopRow => self.freeze(1, 0),
            Command::FreezeFirstColumn => self.freeze(0, 1),
            Command::Unfreeze => self.freeze(0, 0),
            Command::InsertChart => self.open_new_chart_editor(),
            Command::EditNote => self.open_note_editor(),
            Command::DeleteNote => self.delete_note(),
            Command::DataValidation => self.open_validation_dialog(),
            Command::OpenList => {
                let a = self.selection.active;
                let c = &self.grid_config;
                let pos = self.cell_screen_pos(a)
                    + Vec2::new(-c.column_width(a.col), c.row_height(a.row));
                self.open_list_popup(pos);
            }
            Command::Format(action) => self.handle_format_action(action),
            Command::Theme(choice) => {
                self.settings.theme = choice;
                self.settings.save();
            }
            Command::Help => self.help_panel.toggle(),
            Command::About => {
                self.help_panel.visible = true;
                self.help_panel.tab = super::super::help_panel::HelpTab::About;
            }
        }
    }
}
