//! Sheet tab bar widget for switching between sheets

use super::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Response from the sheet tabs widget
pub struct SheetTabsResponse {
    /// Sheet index that was clicked to switch to
    pub switch_to: Option<u32>,
    /// Whether the add sheet button was clicked
    pub add_sheet: bool,
    /// Sheet index to delete (from context menu)
    pub delete_sheet: Option<u32>,
    /// Sheet index and new name for rename
    pub rename_sheet: Option<(u32, String)>,
}

/// Sheet tabs widget
pub struct SheetTabs<'a> {
    sheet_names: &'a [String],
    current_sheet: u32,
    theme: &'a Theme,
}

impl<'a> SheetTabs<'a> {
    pub fn new(sheet_names: &'a [String], current_sheet: u32, theme: &'a Theme) -> Self {
        Self {
            sheet_names,
            current_sheet,
            theme,
        }
    }

    pub fn show(self, ui: &mut Ui) -> SheetTabsResponse {
        let mut response = SheetTabsResponse {
            switch_to: None,
            add_sheet: false,
            delete_sheet: None,
            rename_sheet: None,
        };

        let available_width = ui.available_width();
        let tab_height = 24.0;
        let add_button_width = 28.0;

        // A tab being renamed: (index, text, just started).
        let rename_id = egui::Id::new("sheet_rename");
        let mut renaming: Option<(usize, String, bool)> = ui.data(|d| d.get_temp(rename_id));
        let inactive_bg = self.theme.header_bg;
        let hover_bg = self.theme.selection_bg;

        ui.horizontal(|ui| {
            ui.set_height(tab_height);

            // Calculate tab widths (min 60, max 150)
            let max_tabs_width = available_width - add_button_width - 20.0;
            let tab_count = self.sheet_names.len();
            let ideal_tab_width = 100.0;
            let tab_width = if tab_count as f32 * ideal_tab_width > max_tabs_width {
                (max_tabs_width / tab_count as f32).max(60.0)
            } else {
                ideal_tab_width
            };

            // Render each tab
            for (index, name) in self.sheet_names.iter().enumerate() {
                let is_active = index as u32 == self.current_sheet;

                // Renaming: an edit box in place of the tab. Enter or
                // clicking away keeps the name; Escape cancels.
                if let Some((_, text, started)) = renaming.as_mut().filter(|(i, _, _)| *i == index)
                {
                    let box_id = egui::Id::new("sheet_rename_box");
                    let edit = ui.add_sized(
                        [tab_width, tab_height],
                        egui::TextEdit::singleline(text).id(box_id),
                    );
                    if *started {
                        *started = false;
                        edit.request_focus();
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), box_id) {
                            let all = egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(text.chars().count()),
                            );
                            state.cursor.set_char_range(Some(all));
                            state.store(ui.ctx(), box_id);
                        }
                    } else if edit.lost_focus() {
                        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            response.rename_sheet = Some((index as u32, text.trim().to_string()));
                        }
                        renaming = None;
                    }
                    continue;
                }

                let (rect, tab_response) =
                    ui.allocate_exact_size(Vec2::new(tab_width, tab_height), Sense::click());

                // Draw tab background
                let bg_color = if is_active {
                    self.theme.cell_bg
                } else if tab_response.hovered() {
                    hover_bg
                } else {
                    inactive_bg
                };

                let rounding = CornerRadius {
                    nw: 4,
                    ne: 4,
                    sw: 0,
                    se: 0,
                };

                ui.painter().rect_filled(rect, rounding, bg_color);

                // Draw border for active tab
                if is_active {
                    ui.painter().rect_stroke(
                        rect,
                        rounding,
                        Stroke::new(1.0_f32, self.theme.selection_border),
                        StrokeKind::Outside,
                    );
                    // Draw bottom line to "connect" to grid
                    ui.painter().line_segment(
                        [
                            Pos2::new(rect.left() + 1.0, rect.bottom()),
                            Pos2::new(rect.right() - 1.0, rect.bottom()),
                        ],
                        Stroke::new(2.0_f32, bg_color),
                    );
                }

                // Draw tab text
                let text_color = if is_active {
                    self.theme.text_normal
                } else {
                    self.theme.header_text
                };

                let truncated_name = if name.len() > 12 {
                    format!("{}...", &name[..10])
                } else {
                    name.clone()
                };

                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &truncated_name,
                    egui::FontId::proportional(12.0),
                    text_color,
                );

                // Handle click
                if tab_response.clicked() {
                    response.switch_to = Some(index as u32);
                }
                if tab_response.double_clicked() {
                    renaming = Some((index, name.clone(), true));
                }
                tab_response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name)
                });

                // Context menu for rename/delete
                tab_response.context_menu(|ui| {
                    if ui.button("Rename...").clicked() {
                        renaming = Some((index, name.clone(), true));
                        ui.close_menu();
                    }
                    if self.sheet_names.len() > 1 && ui.button("Delete").clicked() {
                        response.delete_sheet = Some(index as u32);
                        ui.close_menu();
                    }
                });
            }

            // Add sheet button
            ui.add_space(4.0);
            let (add_rect, add_response) =
                ui.allocate_exact_size(Vec2::new(add_button_width, tab_height), Sense::click());

            let add_bg = if add_response.hovered() {
                hover_bg
            } else {
                inactive_bg
            };

            ui.painter()
                .rect_filled(add_rect, CornerRadius::same(4), add_bg);

            ui.painter().text(
                add_rect.center(),
                egui::Align2::CENTER_CENTER,
                "+",
                egui::FontId::proportional(16.0),
                self.theme.header_text,
            );

            if add_response.clicked() {
                response.add_sheet = true;
            }

            add_response.on_hover_text("Add new sheet");
        });

        ui.data_mut(|d| match renaming {
            Some(r) => d.insert_temp(rename_id, r),
            None => d.remove::<(usize, String, bool)>(rename_id),
        });
        response
    }
}
