//! Data validation in the app: checking typed entries, the in-cell
//! drop-down list, the input message, and the Data Validation dialog.

use super::*;
use crate::format::validation::{
    self as dv, CompareOp, DataValidation, ErrorStyle, ValidationKind,
};

/// What the user chose in a validation alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AlertAnswer {
    /// Keep the entry (Warning: Yes, Information: OK)
    Keep,
    /// Go back to editing (Stop: Retry, Warning: No)
    Retry,
    /// Undo the entry
    Cancel,
}

pub(super) struct ValidationDialog {
    pub rule: DataValidation,
    /// The rule being edited covered the selection already
    pub existing: bool,
}

pub(super) struct ListPopup {
    pub coord: CellCoord,
    pub pos: egui::Pos2,
    pub items: Vec<String>,
}

const KINDS: [(ValidationKind, &str); 8] = [
    (ValidationKind::Any, "Any value"),
    (ValidationKind::Whole, "Whole number"),
    (ValidationKind::Decimal, "Decimal"),
    (ValidationKind::List, "List"),
    (ValidationKind::Date, "Date"),
    (ValidationKind::Time, "Time"),
    (ValidationKind::TextLength, "Text length"),
    (ValidationKind::Custom, "Custom formula"),
];

impl SpreadsheetApp {
    /// Enter typed content, then check it against the cell's validation.
    /// Returns false if the entry was rejected (the edit should continue).
    pub(super) fn commit_typed(&mut self, coord: CellCoord, content: &str) -> bool {
        let sheet = self.current_sheet;
        let action = self.set_cell_content_action(coord, content);
        let Some(rule) = self.engine.validation_at(sheet, coord).cloned() else {
            self.undo_history.push(action);
            return true;
        };
        if !rule.show_error || self.engine.passes_validation(sheet, coord, &rule) {
            self.undo_history.push(action);
            return true;
        }
        match self.validation_alert(&rule) {
            AlertAnswer::Keep => {
                self.undo_history.push(action);
                true
            }
            AlertAnswer::Retry => {
                self.replay(&action, false);
                self.retry_text = Some(content.to_string());
                self.start_editing(None);
                false
            }
            AlertAnswer::Cancel => {
                self.replay(&action, false);
                true
            }
        }
    }

    fn validation_alert(&mut self, rule: &DataValidation) -> AlertAnswer {
        #[cfg(test)]
        if let Some(answer) = self.alert_answer {
            return answer;
        }
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
        let title = if rule.error_title.is_empty() {
            "RustSheet"
        } else {
            &rule.error_title
        };
        let message = if rule.error_message.is_empty() {
            "This value doesn't match the data validation rules defined for this cell."
        } else {
            &rule.error_message
        };
        match rule.error_style {
            ErrorStyle::Stop => {
                let r = MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title(title)
                    .set_description(message)
                    .set_buttons(MessageButtons::OkCancelCustom(
                        "Retry".into(),
                        "Cancel".into(),
                    ))
                    .show();
                match r {
                    MessageDialogResult::Custom(s) if s == "Retry" => AlertAnswer::Retry,
                    MessageDialogResult::Ok => AlertAnswer::Retry,
                    _ => AlertAnswer::Cancel,
                }
            }
            ErrorStyle::Warning => {
                let r = MessageDialog::new()
                    .set_level(MessageLevel::Warning)
                    .set_title(title)
                    .set_description(format!("{message}\n\nContinue?"))
                    .set_buttons(MessageButtons::YesNoCancel)
                    .show();
                match r {
                    MessageDialogResult::Yes => AlertAnswer::Keep,
                    MessageDialogResult::No => AlertAnswer::Retry,
                    _ => AlertAnswer::Cancel,
                }
            }
            ErrorStyle::Information => {
                let r = MessageDialog::new()
                    .set_level(MessageLevel::Info)
                    .set_title(title)
                    .set_description(message)
                    .set_buttons(MessageButtons::OkCancel)
                    .show();
                match r {
                    MessageDialogResult::Ok => AlertAnswer::Keep,
                    _ => AlertAnswer::Cancel,
                }
            }
        }
    }

    pub(super) fn open_list_popup(&mut self, pos: egui::Pos2) {
        let coord = self.selection.active;
        let sheet = self.current_sheet;
        let Some(rule) = self.engine.validation_at(sheet, coord).cloned() else {
            return;
        };
        if rule.kind != ValidationKind::List {
            return;
        }
        let items = self.engine.list_items(sheet, &rule);
        self.list_popup = Some(ListPopup { coord, pos, items });
    }

    pub(super) fn show_list_popup(&mut self, ctx: &egui::Context) {
        let Some(popup) = self.list_popup.take() else {
            return;
        };
        let mut chosen = None;
        let mut keep = true;
        let current = self.get_cell_display(popup.coord);
        let area = egui::Area::new(egui::Id::new("validation_list"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(140.0);
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            if popup.items.is_empty() {
                                ui.label(RichText::new("No items").weak());
                            }
                            for item in &popup.items {
                                if ui.selectable_label(*item == current, item).clicked() {
                                    chosen = Some(item.clone());
                                }
                            }
                        });
                });
            });
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && !area.response.hovered();
        if ctx.input(|i| i.key_pressed(Key::Escape)) || clicked_outside {
            keep = false;
        }
        if let Some(item) = chosen {
            // A chosen item is valid by construction.
            self.set_cell_content(popup.coord, &item);
            keep = false;
        }
        if keep {
            self.list_popup = Some(popup);
        }
    }

    /// The rule's input message, shown under the active cell.
    pub(super) fn show_input_message(&self, ctx: &egui::Context) {
        if self.is_editing() && self.list_popup.is_some() {
            return;
        }
        let coord = self.selection.active;
        let Some(rule) = self.engine.validation_at(self.current_sheet, coord) else {
            return;
        };
        if !rule.show_input || (rule.input_title.is_empty() && rule.input_message.is_empty()) {
            return;
        }
        let c = &self.grid_config;
        let below = self.cell_screen_pos(coord)
            + Vec2::new(
                -c.column_width(coord.col) + 8.0,
                c.row_height(coord.row) + 4.0,
            );
        egui::Area::new(egui::Id::new("validation_input"))
            .order(egui::Order::Tooltip)
            .fixed_pos(below)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(255, 255, 225))
                    .show(ui, |ui| {
                        ui.set_max_width(240.0);
                        let ink = egui::Color32::from_rgb(30, 30, 30);
                        if !rule.input_title.is_empty() {
                            ui.label(RichText::new(&rule.input_title).strong().color(ink));
                        }
                        if !rule.input_message.is_empty() {
                            ui.label(RichText::new(&rule.input_message).color(ink));
                        }
                    });
            });
    }

    pub(super) fn open_validation_dialog(&mut self) {
        let sheet = self.current_sheet;
        let existing = self
            .engine
            .validation_at(sheet, self.selection.active)
            .cloned();
        let range = self.clamp_to_used_or_self(self.selection.primary_range());
        self.validation_dialog = Some(match existing {
            Some(rule) => ValidationDialog {
                rule,
                existing: true,
            },
            None => ValidationDialog {
                rule: DataValidation {
                    ranges: vec![range],
                    ..Default::default()
                },
                existing: false,
            },
        });
    }

    /// Put `rule` on the selection, replacing rules there.
    fn apply_validation(&mut self, mut rule: Option<DataValidation>) {
        let sheet = self.current_sheet;
        let area = self.selection.primary_range();
        if let Some(rule) = &mut rule {
            rule.ranges = vec![area];
        }
        self.with_snapshot(|app| {
            let list = &mut app.engine.formatting_mut(sheet).validations;
            dv::clear_area(list, area);
            if let Some(rule) = rule {
                if rule.kind != ValidationKind::Any || rule.show_input {
                    list.push(rule);
                }
            }
            true
        });
    }

    pub(super) fn show_validation_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.validation_dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut action: Option<Option<DataValidation>> = None;
        egui::Window::new("Data Validation")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let r = &mut d.rule;
                ui.heading("Settings");
                egui::Grid::new("dv_settings")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Allow:");
                        egui::ComboBox::from_id_salt("dv_kind")
                            .selected_text(KINDS.iter().find(|k| k.0 == r.kind).map_or("", |k| k.1))
                            .show_ui(ui, |ui| {
                                for (k, label) in KINDS {
                                    ui.selectable_value(&mut r.kind, k, label);
                                }
                            });
                        ui.end_row();
                        let comparing = matches!(
                            r.kind,
                            ValidationKind::Whole
                                | ValidationKind::Decimal
                                | ValidationKind::Date
                                | ValidationKind::Time
                                | ValidationKind::TextLength
                        );
                        if comparing {
                            ui.label("Data:");
                            egui::ComboBox::from_id_salt("dv_op")
                                .selected_text(r.operator.label())
                                .show_ui(ui, |ui| {
                                    for op in CompareOp::ALL {
                                        ui.selectable_value(&mut r.operator, op, op.label());
                                    }
                                });
                            ui.end_row();
                            let hint = match r.kind {
                                ValidationKind::Date => "2026-01-31 or a formula",
                                ValidationKind::Time => "9:00 or a formula",
                                _ => "a number or a formula",
                            };
                            let (first, second) = if r.operator.needs_second() {
                                ("Minimum:", Some("Maximum:"))
                            } else {
                                ("Value:", None)
                            };
                            ui.label(first);
                            ui.add(egui::TextEdit::singleline(&mut r.formula1).hint_text(hint));
                            ui.end_row();
                            if let Some(label) = second {
                                let f2 = r.formula2.get_or_insert_with(String::new);
                                ui.label(label);
                                ui.add(egui::TextEdit::singleline(f2).hint_text(hint));
                                ui.end_row();
                            } else {
                                r.formula2 = None;
                            }
                        }
                        if r.kind == ValidationKind::List {
                            ui.label("Source:");
                            let mut source = r
                                .literal_items()
                                .map(|items| items.join(", "))
                                .unwrap_or_else(|| format!("={}", r.formula1));
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut source)
                                    .hint_text("Red, Green, Blue  or  =$A$1:$A$9"),
                            );
                            if edit.changed() {
                                r.formula1 = match source.strip_prefix('=') {
                                    Some(range) => range.trim().to_string(),
                                    None => format!("\"{}\"", source.replace('"', "\"\"")),
                                };
                            }
                            ui.end_row();
                        }
                        if r.kind == ValidationKind::Custom {
                            ui.label("Formula:");
                            ui.add(
                                egui::TextEdit::singleline(&mut r.formula1)
                                    .hint_text("e.g. ISNUMBER(A1), relative to the first cell"),
                            );
                            ui.end_row();
                        }
                    });
                ui.checkbox(&mut r.allow_blank, "Ignore blank");
                if r.kind == ValidationKind::List {
                    ui.checkbox(&mut r.dropdown, "In-cell drop-down");
                }
                ui.separator();
                ui.heading("Input message");
                ui.checkbox(&mut r.show_input, "Show when the cell is selected");
                egui::Grid::new("dv_input").num_columns(2).show(ui, |ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut r.input_title);
                    ui.end_row();
                    ui.label("Message:");
                    ui.text_edit_multiline(&mut r.input_message);
                    ui.end_row();
                });
                ui.separator();
                ui.heading("Error alert");
                ui.checkbox(
                    &mut r.show_error,
                    "Show an alert after invalid data is entered",
                );
                ui.horizontal(|ui| {
                    ui.label("Style:");
                    ui.selectable_value(&mut r.error_style, ErrorStyle::Stop, "Stop");
                    ui.selectable_value(&mut r.error_style, ErrorStyle::Warning, "Warning");
                    ui.selectable_value(&mut r.error_style, ErrorStyle::Information, "Information");
                });
                egui::Grid::new("dv_error").num_columns(2).show(ui, |ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut r.error_title);
                    ui.end_row();
                    ui.label("Message:");
                    ui.text_edit_multiline(&mut r.error_message);
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        action = Some(Some(d.rule.clone()));
                        keep = false;
                    }
                    if ui.button("Clear All").clicked() {
                        action = Some(None);
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
                let _ = d.existing;
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if let Some(rule) = action {
            self.apply_validation(rule);
        }
        if keep {
            self.validation_dialog = Some(d);
        }
    }
}
