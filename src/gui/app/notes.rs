//! Cell notes: the hover popup and the editor (Shift+F2).

use super::*;
use crate::format::Note;

pub(super) struct NoteEditor {
    pub coord: CellCoord,
    pub text: String,
    pub author: Option<String>,
    pub is_new: bool,
    pub focus: bool,
}

impl SpreadsheetApp {
    pub(super) fn note_at(&self, coord: CellCoord) -> Option<&Note> {
        self.engine
            .formatting(self.current_sheet)?
            .notes
            .get(&coord)
    }

    /// Set or remove one note, as an undo step.
    pub(super) fn set_note(&mut self, coord: CellCoord, note: Option<Note>) {
        let sheet = self.current_sheet;
        let old = self.note_at(coord).cloned();
        if old == note {
            return;
        }
        self.apply_note(sheet, coord, note.clone());
        self.undo_history.push(UndoAction::Note {
            sheet,
            coord,
            old,
            new: note,
        });
    }

    pub(super) fn apply_note(&mut self, sheet: u32, coord: CellCoord, note: Option<Note>) {
        let notes = &mut self.engine.formatting_mut(sheet).notes;
        match note {
            Some(n) => notes.insert(coord, n),
            None => notes.remove(&coord),
        };
        self.modified = true;
    }

    pub(super) fn open_note_editor(&mut self) {
        let coord = self.selection.active;
        let existing = self.note_at(coord).cloned();
        self.note_editor = Some(NoteEditor {
            coord,
            text: existing
                .as_ref()
                .map(|n| n.text.clone())
                .unwrap_or_default(),
            author: existing.as_ref().and_then(|n| n.author.clone()),
            is_new: existing.is_none(),
            focus: true,
        });
    }

    pub(super) fn delete_note(&mut self) {
        let coord = self.selection.active;
        self.set_note(coord, None);
    }

    pub(super) fn show_note_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.note_editor.take() else {
            return;
        };
        let mut keep = true;
        let mut save = false;
        let mut delete = false;
        let title = format!("Note on {}", editor.coord.to_a1());
        egui::Window::new(title)
            .order(egui::Order::Foreground)
            .id(egui::Id::new("note_editor"))
            .collapsible(false)
            .resizable(false)
            .default_pos(self.cell_screen_pos(editor.coord) + Vec2::new(12.0, 0.0))
            .show(ctx, |ui| {
                if let Some(author) = &editor.author {
                    ui.label(RichText::new(author).strong());
                }
                let r = ui.add(
                    egui::TextEdit::multiline(&mut editor.text)
                        .desired_width(260.0)
                        .desired_rows(5)
                        .hint_text("Type a note"),
                );
                if editor.focus {
                    r.request_focus();
                    editor.focus = false;
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save = true;
                        keep = false;
                    }
                    if !editor.is_new && ui.button("Delete").clicked() {
                        delete = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if save {
            let note = (!editor.text.trim().is_empty()).then(|| Note {
                text: editor.text.trim_end().to_string(),
                author: editor.author.clone(),
            });
            self.set_note(editor.coord, note);
        } else if delete {
            self.set_note(editor.coord, None);
        }
        if keep {
            self.note_editor = Some(editor);
        }
    }

    /// The note popup while the pointer rests on a cell that has one.
    pub(super) fn show_note_popup(&self, ctx: &egui::Context, coord: CellCoord, pos: egui::Pos2) {
        let Some(note) = self.note_at(coord) else {
            return;
        };
        egui::Area::new(egui::Id::new("note_popup"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + Vec2::new(6.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_rgb(255, 255, 225))
                    .show(ui, |ui| {
                        ui.set_max_width(260.0);
                        let ink = egui::Color32::from_rgb(30, 30, 30);
                        if let Some(author) = &note.author {
                            ui.label(RichText::new(author).strong().color(ink));
                        }
                        ui.label(RichText::new(&note.text).color(ink));
                    });
            });
    }

    /// Top-left of a cell on screen, from the last frame's layout.
    pub(super) fn cell_screen_pos(&self, coord: CellCoord) -> egui::Pos2 {
        let origin = self
            .grid_origin
            .unwrap_or(egui::Pos2::new(HEADER_WIDTH, HEADER_HEIGHT));
        let c = &self.grid_config;
        let x = if coord.col < c.frozen_cols {
            c.column_x(coord.col)
        } else {
            c.column_x(coord.col) - self.scroll.offset_x
        };
        let y = if coord.row < c.frozen_rows {
            c.row_y(coord.row)
        } else {
            c.row_y(coord.row) - self.scroll.offset_y
        };
        origin + Vec2::new(x + c.column_width(coord.col), y)
    }
}
