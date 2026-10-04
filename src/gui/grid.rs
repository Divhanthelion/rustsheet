//! Spreadsheet grid widget

use super::selection::Selection;
use super::theme::Theme;
use crate::calc::CalcEngine;
use crate::calc::CellResult;
use crate::cell::{CellCoord, CellError};
use crate::format::{
    CellFormat, DEFAULT_FONT_SIZE, HAlign, Rgb, SheetFormatting, format_general, format_number,
};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::BTreeMap;

/// Default cell dimensions
pub const DEFAULT_COLUMN_WIDTH: f32 = 80.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 22.0;
pub const HEADER_WIDTH: f32 = 50.0;
pub const HEADER_HEIGHT: f32 = 24.0;

/// Text size of an 11pt (default) cell.
pub const CELL_FONT_SIZE: f32 = 13.0;
/// Smallest width or height a drag can resize to.
const MIN_RESIZE: f32 = 8.0;
/// Excel's last row and column index.
const MAX_ROW: u32 = 1_048_575;
const MAX_COL: u32 = 16_383;

/// Column widths and row heights for the sheet on screen. Only sizes that
/// differ from the defaults are stored.
#[derive(Clone, Default)]
pub struct GridConfig {
    pub column_widths: BTreeMap<u32, f32>,
    pub row_heights: BTreeMap<u32, f32>,
}

impl GridConfig {
    pub fn for_sheet(formatting: Option<&SheetFormatting>) -> Self {
        formatting
            .map(|f| Self {
                column_widths: f.column_widths.clone(),
                row_heights: f.row_heights.clone(),
            })
            .unwrap_or_default()
    }

    pub fn column_width(&self, col: u32) -> f32 {
        self.column_widths
            .get(&col)
            .copied()
            .unwrap_or(DEFAULT_COLUMN_WIDTH)
    }

    pub fn row_height(&self, row: u32) -> f32 {
        self.row_heights
            .get(&row)
            .copied()
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    /// Get x position of column left edge relative to data area
    pub fn column_x(&self, col: u32) -> f32 {
        offset_of(&self.column_widths, DEFAULT_COLUMN_WIDTH, col)
    }

    /// Get y position of row top edge relative to data area
    pub fn row_y(&self, row: u32) -> f32 {
        offset_of(&self.row_heights, DEFAULT_ROW_HEIGHT, row)
    }

    /// Find column at x position
    pub fn column_at_x(&self, x: f32) -> u32 {
        index_at(&self.column_widths, DEFAULT_COLUMN_WIDTH, x, MAX_COL)
    }

    /// Find row at y position
    pub fn row_at_y(&self, y: f32) -> u32 {
        index_at(&self.row_heights, DEFAULT_ROW_HEIGHT, y, MAX_ROW)
    }
}

/// Start of item `index` when items are `default` long except for `sizes`.
fn offset_of(sizes: &BTreeMap<u32, f32>, default: f32, index: u32) -> f32 {
    index as f32 * default
        + sizes
            .range(..index)
            .map(|(_, size)| size - default)
            .sum::<f32>()
}

/// The item that contains position `pos`.
fn index_at(sizes: &BTreeMap<u32, f32>, default: f32, pos: f32, max: u32) -> u32 {
    if pos <= 0.0 {
        return 0;
    }
    let (mut next_index, mut next_pos) = (0u32, 0.0f32);
    for (&index, &size) in sizes {
        let start = next_pos + (index - next_index) as f32 * default;
        if pos < start {
            break;
        }
        if pos < start + size {
            return index;
        }
        next_index = index + 1;
        next_pos = start + size;
    }
    let index = next_index as f64 + ((pos - next_pos) / default).floor() as f64;
    (index as u32).min(max)
}

/// What a cell shows before theme colors are applied.
struct CellText {
    text: String,
    /// Color from the number format, e.g. `[Red]` for negatives.
    format_color: Option<Rgb>,
    kind: TextKind,
}

#[derive(Clone, Copy, PartialEq)]
enum TextKind {
    Number,
    Text,
    Centered,
    Error,
}

/// Format a cell's value for display. `max_len` bounds General numbers.
fn cell_text(value: &CellResult, format: &CellFormat, max_len: usize) -> Option<CellText> {
    let plain = |text: String, kind| CellText {
        text,
        format_color: None,
        kind,
    };
    Some(match value {
        CellResult::Empty => return None,
        CellResult::Value(n) => match &format.number_format {
            Some(code) => {
                let formatted = format_number(*n, code);
                CellText {
                    text: formatted.text,
                    format_color: formatted.color,
                    kind: TextKind::Number,
                }
            }
            None => plain(format_general(*n, max_len), TextKind::Number),
        },
        CellResult::Text(s) => plain(s.clone(), TextKind::Text),
        CellResult::Bool(b) => plain(
            if *b { "TRUE" } else { "FALSE" }.to_string(),
            TextKind::Centered,
        ),
        CellResult::Error(e) => plain(
            match e {
                CellError::DivZero => "#DIV/0!",
                CellError::Value => "#VALUE!",
                CellError::Ref => "#REF!",
                CellError::Name => "#NAME?",
                CellError::Num => "#NUM!",
                CellError::NA => "#N/A",
                CellError::Null => "#NULL!",
                CellError::Circular => "#CIRC!",
                CellError::GettingData => "#GETTING_DATA",
                CellError::Spill => "#SPILL!",
                CellError::Calc => "#CALC!",
            }
            .to_string(),
            TextKind::Error,
        ),
    })
}

/// A cell's value as shown on screen, e.g. for copying to other apps.
pub fn display_text(value: &CellResult, format: Option<&CellFormat>) -> String {
    let default_format = CellFormat::default();
    cell_text(value, format.unwrap_or(&default_format), 15)
        .map(|c| c.text)
        .unwrap_or_default()
}

fn font_for(format: &CellFormat) -> egui::FontId {
    egui::FontId::proportional(
        CELL_FONT_SIZE * format.font_size_or_default() as f32 / DEFAULT_FONT_SIZE as f32,
    )
}

fn to_color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Lay out a cell's text with its font style.
fn layout_text(
    fonts: &egui::text::Fonts,
    text: String,
    format: &CellFormat,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let line = |on: bool| {
        if on {
            Stroke::new(1.0_f32, color)
        } else {
            Stroke::NONE
        }
    };
    let job = egui::text::LayoutJob::single_section(
        text,
        egui::TextFormat {
            font_id: font_for(format),
            color,
            italics: format.italic,
            underline: line(format.underline),
            strikethrough: line(format.strikethrough),
            ..Default::default()
        },
    );
    fonts.layout_job(job)
}

/// Width that fits every value in `col`, for double-clicking a column border.
pub fn fit_column_width(
    ctx: &egui::Context,
    engine: &CalcEngine,
    sheet: u32,
    col: u32,
) -> Option<f32> {
    let default_format = CellFormat::default();
    let formatting = engine.formatting(sheet);
    let coords: Vec<CellCoord> = engine
        .iter_sheet_inputs(sheet)
        .map(|(coord, _)| coord)
        .filter(|coord| coord.col == col)
        .collect();
    ctx.fonts(|fonts| {
        coords
            .into_iter()
            .filter_map(|coord| {
                let format = formatting
                    .and_then(|f| f.get(coord))
                    .unwrap_or(&default_format);
                let value = engine.get_value(sheet, coord);
                let text = cell_text(&value, format, 11)?;
                let galley = layout_text(fonts, text.text, format, Color32::WHITE);
                let bold_extra = if format.bold { 1.0 } else { 0.0 };
                Some(galley.size().x + bold_extra + 12.0)
            })
            .reduce(f32::max)
    })
    .map(|w| w.max(MIN_RESIZE * 3.0))
}

/// Commands in the grid's right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Cut,
    Copy,
    Paste,
    ClearContents,
    ClearFormatting,
}

/// Which header border is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeAxis {
    Column,
    Row,
}

/// Scroll state for the grid
#[derive(Default, Clone)]
pub struct ScrollState {
    pub offset_x: f32,
    pub offset_y: f32,
    pub first_visible_row: u32,
    pub first_visible_col: u32,
}

impl ScrollState {
    pub fn scroll_to_cell(&mut self, coord: CellCoord, config: &GridConfig, viewport_size: Vec2) {
        // Calculate cell position
        let cell_x = config.column_x(coord.col);
        let cell_y = config.row_y(coord.row);
        let cell_w = config.column_width(coord.col);
        let cell_h = config.row_height(coord.row);

        // Viewport dimensions (excluding headers)
        let view_w = viewport_size.x - HEADER_WIDTH;
        let view_h = viewport_size.y - HEADER_HEIGHT;

        // Scroll to make cell visible
        if cell_x < self.offset_x {
            self.offset_x = cell_x;
        } else if cell_x + cell_w > self.offset_x + view_w {
            self.offset_x = cell_x + cell_w - view_w;
        }

        if cell_y < self.offset_y {
            self.offset_y = cell_y;
        } else if cell_y + cell_h > self.offset_y + view_h {
            self.offset_y = cell_y + cell_h - view_h;
        }

        // Update first visible row/col
        self.first_visible_row = config.row_at_y(self.offset_y);
        self.first_visible_col = config.column_at_x(self.offset_x);
    }
}

/// Response from grid interaction
#[derive(Default)]
pub struct GridResponse {
    /// Cell that was clicked
    pub clicked_cell: Option<CellCoord>,
    /// Cell that was double-clicked (start editing)
    pub double_clicked_cell: Option<CellCoord>,
    /// Cell the user wants to edit (pressed Enter or F2)
    pub edit_cell: Option<CellCoord>,
    /// Single character typed to start editing (triggers TransitionToEdit)
    pub text_input_char: Option<char>,
    /// Navigation key pressed
    pub navigation: Option<NavigationKey>,
    /// Drag started at this cell (for multi-cell selection)
    pub drag_started: Option<CellCoord>,
    /// Dragging over this cell (extend selection)
    pub drag_to: Option<CellCoord>,
    /// Drag ended
    pub drag_ended: bool,
    /// A header border is being dragged: new size for that column or row
    pub resize: Option<(ResizeAxis, u32, f32)>,
    /// The resize drag was released
    pub resize_ended: bool,
    /// A column border was double-clicked: fit the column to its contents
    pub autofit_column: Option<u32>,
    /// Cell under a right-click, to select before the menu acts
    pub right_clicked_cell: Option<CellCoord>,
    /// Command picked from the right-click menu
    pub context_action: Option<ContextAction>,
}

/// Navigation keys
#[derive(Debug, Clone, Copy)]
pub enum NavigationKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    CtrlHome,
    CtrlEnd,
    CtrlUp,
    CtrlDown,
    CtrlLeft,
    CtrlRight,
}

/// The spreadsheet grid widget
pub struct SpreadsheetGrid<'a> {
    sheet_index: u32,
    engine: &'a CalcEngine,
    selection: &'a Selection,
    config: &'a GridConfig,
    scroll: &'a ScrollState,
    theme: &'a Theme,
}

impl<'a> SpreadsheetGrid<'a> {
    pub fn new(
        sheet_index: u32,
        engine: &'a CalcEngine,
        selection: &'a Selection,
        config: &'a GridConfig,
        scroll: &'a ScrollState,
        theme: &'a Theme,
    ) -> Self {
        Self {
            sheet_index,
            engine,
            selection,
            config,
            scroll,
            theme,
        }
    }

    /// Render the grid and handle interaction
    pub fn show(&self, ui: &mut Ui) -> GridResponse {
        let mut response = GridResponse::default();

        let available_rect = ui.available_rect_before_wrap();
        let viewport_size = available_rect.size();

        // Use a stable ID for the grid
        let grid_id = egui::Id::new("spreadsheet_grid");

        // Allocate space and interact with the grid using the same ID
        let (grid_rect, _) = ui.allocate_exact_size(viewport_size, Sense::hover());
        let grid_response = ui.interact(grid_rect, grid_id, Sense::click_and_drag());

        // NOTE: We do NOT request focus every frame - this breaks TextEdit in dialogs
        // Focus is only requested on specific events (click, navigation key)
        // See egui bug #5187 - repeated request_focus() breaks text input

        if ui.is_rect_visible(grid_rect) {
            let painter = ui.painter_at(grid_rect);

            // Draw background
            painter.rect_filled(grid_rect, 0.0, self.theme.cell_bg);

            // Calculate visible range
            let data_rect = Rect::from_min_size(
                grid_rect.min + Vec2::new(HEADER_WIDTH, HEADER_HEIGHT),
                Vec2::new(
                    viewport_size.x - HEADER_WIDTH,
                    viewport_size.y - HEADER_HEIGHT,
                ),
            );

            // Draw cells
            self.draw_cells(&painter, data_rect);

            // Draw row headers
            self.draw_row_headers(
                &painter,
                Rect::from_min_size(
                    grid_rect.min + Vec2::new(0.0, HEADER_HEIGHT),
                    Vec2::new(HEADER_WIDTH, viewport_size.y - HEADER_HEIGHT),
                ),
            );

            // Draw column headers
            self.draw_column_headers(
                &painter,
                Rect::from_min_size(
                    grid_rect.min + Vec2::new(HEADER_WIDTH, 0.0),
                    Vec2::new(viewport_size.x - HEADER_WIDTH, HEADER_HEIGHT),
                ),
            );

            // Draw corner header
            painter.rect_filled(
                Rect::from_min_size(grid_rect.min, Vec2::new(HEADER_WIDTH, HEADER_HEIGHT)),
                0.0,
                self.theme.header_bg,
            );

            // Draw selection
            self.draw_selection(&painter, data_rect);

            // Resize handles sit on the header borders. They are added after
            // the grid's own interaction, so they take the pointer.
            self.resize_handles(ui, grid_id, grid_rect, viewport_size, &mut response);

            // Helper to convert screen position to cell coordinate
            let pos_to_cell = |pos: Pos2| -> Option<CellCoord> {
                if data_rect.contains(pos) {
                    let local_pos =
                        pos - data_rect.min + Vec2::new(self.scroll.offset_x, self.scroll.offset_y);
                    let col = self.config.column_at_x(local_pos.x);
                    let row = self.config.row_at_y(local_pos.y);
                    Some(CellCoord::new(row, col))
                } else {
                    None
                }
            };

            // Handle drag for multi-cell selection
            if grid_response.drag_started() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    if let Some(coord) = pos_to_cell(pos) {
                        response.drag_started = Some(coord);
                    }
                }
            }

            if grid_response.dragged() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    if let Some(coord) = pos_to_cell(pos) {
                        response.drag_to = Some(coord);
                    }
                }
            }

            if grid_response.drag_stopped() {
                response.drag_ended = true;
            }

            // Handle clicks (single click when not dragging)
            if grid_response.clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    if let Some(coord) = pos_to_cell(pos) {
                        response.clicked_cell = Some(coord);
                    }
                }
            }

            if grid_response.double_clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    if let Some(coord) = pos_to_cell(pos) {
                        response.double_clicked_cell = Some(coord);
                    }
                }
            }

            if grid_response.secondary_clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    response.right_clicked_cell = pos_to_cell(pos);
                }
            }
            grid_response.context_menu(|ui| {
                let items = [
                    ("Cut", "Ctrl+X", ContextAction::Cut),
                    ("Copy", "Ctrl+C", ContextAction::Copy),
                    ("Paste", "Ctrl+V", ContextAction::Paste),
                    ("Clear contents", "Delete", ContextAction::ClearContents),
                    ("Clear formatting", "", ContextAction::ClearFormatting),
                ];
                for (label, shortcut, action) in items {
                    if action == ContextAction::ClearContents {
                        ui.separator();
                    }
                    let button = egui::Button::new(label).shortcut_text(shortcut);
                    if ui.add(button).clicked() {
                        response.context_action = Some(action);
                        ui.close_menu();
                    }
                }
            });
        }

        // Handle keyboard navigation - use consume_key to prevent focus changes
        let has_focus = ui.ctx().memory(|m| m.has_focus(grid_id)) || grid_response.has_focus();

        if has_focus {
            // Consume arrow keys to prevent them from moving focus to other widgets
            response.navigation = self.handle_keyboard_consume(ui);

            // Handle F2 for edit mode
            if ui
                .ctx()
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F2))
            {
                response.edit_cell = Some(self.selection.active);
            }

            // Handle Enter for edit mode
            if ui
                .ctx()
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
            {
                response.edit_cell = Some(self.selection.active);
            }

            // Handle direct text input - capture only first character
            // The App will use this to trigger TransitionToEdit with the initial char
            if !ui.input(|i| i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command) {
                let first_char: Option<char> = ui.input(|i| {
                    for event in &i.events {
                        if let egui::Event::Text(t) = event {
                            if let Some(c) = t.chars().next() {
                                if !c.is_control() {
                                    return Some(c);
                                }
                            }
                        }
                    }
                    None
                });
                if first_char.is_some() {
                    response.text_input_char = first_char;
                }
            }
        }

        // Request focus if clicked
        if grid_response.clicked() {
            ui.ctx().memory_mut(|m| m.request_focus(grid_id));
        }

        response
    }

    fn resize_handles(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        grid_rect: Rect,
        viewport_size: Vec2,
        response: &mut GridResponse,
    ) {
        let col_header = Rect::from_min_size(
            grid_rect.min + Vec2::new(HEADER_WIDTH, 0.0),
            Vec2::new(viewport_size.x - HEADER_WIDTH, HEADER_HEIGHT),
        );
        let first_col = self.scroll.first_visible_col;
        let mut x = col_header.min.x - (self.scroll.offset_x - self.config.column_x(first_col));
        for col in first_col..=MAX_COL {
            if x >= col_header.max.x {
                break;
            }
            let left = x;
            x += self.config.column_width(col);
            if x <= col_header.min.x {
                continue;
            }
            let handle = Rect::from_center_size(
                Pos2::new(x, col_header.center().y),
                Vec2::new(8.0, HEADER_HEIGHT),
            );
            let r = ui.interact(
                handle,
                grid_id.with(("col_resize", col)),
                Sense::click_and_drag(),
            );
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            }
            if r.dragged() {
                if let Some(p) = r.interact_pointer_pos() {
                    response.resize = Some((ResizeAxis::Column, col, (p.x - left).max(MIN_RESIZE)));
                }
            }
            if r.drag_stopped() {
                response.resize_ended = true;
            }
            if r.double_clicked() {
                response.autofit_column = Some(col);
            }
        }

        let row_header = Rect::from_min_size(
            grid_rect.min + Vec2::new(0.0, HEADER_HEIGHT),
            Vec2::new(HEADER_WIDTH, viewport_size.y - HEADER_HEIGHT),
        );
        let first_row = self.scroll.first_visible_row;
        let mut y = row_header.min.y - (self.scroll.offset_y - self.config.row_y(first_row));
        for row in first_row..=MAX_ROW {
            if y >= row_header.max.y {
                break;
            }
            let top = y;
            y += self.config.row_height(row);
            if y <= row_header.min.y {
                continue;
            }
            let handle = Rect::from_center_size(
                Pos2::new(row_header.center().x, y),
                Vec2::new(HEADER_WIDTH, 6.0),
            );
            let r = ui.interact(handle, grid_id.with(("row_resize", row)), Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
            }
            if r.dragged() {
                if let Some(p) = r.interact_pointer_pos() {
                    response.resize = Some((ResizeAxis::Row, row, (p.y - top).max(MIN_RESIZE)));
                }
            }
            if r.drag_stopped() {
                response.resize_ended = true;
            }
        }
    }

    fn draw_cells(&self, painter: &egui::Painter, data_rect: Rect) {
        let clip_rect = data_rect;
        let formatting = self.engine.formatting(self.sheet_index);
        // Borders are drawn last so neighboring fills don't cover them.
        let mut borders: Vec<[Pos2; 2]> = Vec::new();

        // Calculate visible cell range
        let start_col = self.scroll.first_visible_col;
        let start_row = self.scroll.first_visible_row;

        let mut y = data_rect.min.y - (self.scroll.offset_y - self.config.row_y(start_row));

        for row in start_row.. {
            if y >= data_rect.max.y {
                break;
            }

            let row_height = self.config.row_height(row);
            let mut x = data_rect.min.x - (self.scroll.offset_x - self.config.column_x(start_col));

            for col in start_col.. {
                if x >= data_rect.max.x {
                    break;
                }

                let col_width = self.config.column_width(col);
                let cell_rect =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(col_width, row_height));

                // Only draw if visible
                if cell_rect.intersects(clip_rect) {
                    let coord = CellCoord::new(row, col);
                    let format = formatting.and_then(|f| f.get(coord));

                    // Fill, or alternating background. Filled cells hide grid lines, as in Excel.
                    let fill = format.and_then(|f| f.fill);
                    let bg_color = match fill {
                        Some(fill) => to_color32(fill),
                        None if (row + col) % 2 == 0 => self.theme.cell_bg,
                        None => self.theme.cell_bg_alt,
                    };
                    painter.rect_filled(cell_rect, 0.0, bg_color);

                    if fill.is_none() {
                        painter.line_segment(
                            [cell_rect.right_top(), cell_rect.right_bottom()],
                            self.theme.grid_stroke(),
                        );
                        painter.line_segment(
                            [cell_rect.left_bottom(), cell_rect.right_bottom()],
                            self.theme.grid_stroke(),
                        );
                    }

                    if let Some(b) = format.map(|f| f.borders) {
                        let r = cell_rect;
                        let edges = [
                            (b.top, [r.left_top(), r.right_top()]),
                            (b.right, [r.right_top(), r.right_bottom()]),
                            (b.bottom, [r.left_bottom(), r.right_bottom()]),
                            (b.left, [r.left_top(), r.left_bottom()]),
                        ];
                        borders.extend(edges.into_iter().filter(|(on, _)| *on).map(|(_, l)| l));
                    }

                    // Get cell value and render
                    let value = self.engine.get_value(self.sheet_index, coord);
                    self.draw_cell_content(painter, cell_rect, &value, format);
                }

                x += col_width;
            }

            y += row_height;
        }

        let border_stroke = Stroke::new(1.0_f32, self.theme.text_normal);
        for line in borders {
            painter.line_segment(line, border_stroke);
        }
    }

    fn draw_cell_content(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        value: &CellResult,
        format: Option<&CellFormat>,
    ) {
        let padding = 4.0;
        let text_rect = rect.shrink(padding);
        let default_format = CellFormat::default();
        let format = format.unwrap_or(&default_format);
        let font = font_for(format);

        let digit_width = painter.fonts(|f| f.glyph_width(&font, '0'));
        let max_len = (text_rect.width() / digit_width).floor().max(1.0) as usize;
        let Some(cell) = cell_text(value, format, max_len) else {
            return;
        };

        // A format color ([Red]) wins over the font color. With a fill and no
        // font color, pick black or white so the text stays readable.
        let color = match (cell.format_color, format.font_color, format.fill) {
            (Some(c), _, _) | (None, Some(c), _) => to_color32(c),
            (None, None, Some(fill)) if fill.luminance() > 0.5 => Color32::BLACK,
            (None, None, Some(_)) => Color32::WHITE,
            (None, None, None) => match cell.kind {
                TextKind::Number => self.theme.text_number,
                TextKind::Error => self.theme.text_error,
                _ => self.theme.text_normal,
            },
        };

        let align = match (format.h_align, cell.kind) {
            (HAlign::Left, _) => egui::Align::Min,
            (HAlign::Center, _) => egui::Align::Center,
            (HAlign::Right, _) => egui::Align::Max,
            (HAlign::General, TextKind::Number) => egui::Align::Max,
            (HAlign::General, TextKind::Text) => egui::Align::Min,
            (HAlign::General, _) => egui::Align::Center,
        };

        let mut galley = painter.fonts(|f| layout_text(f, cell.text, format, color));
        // Numbers never spill into neighbors; like Excel, show #### instead.
        if cell.kind == TextKind::Number && galley.size().x > text_rect.width() {
            let hash_width = painter.fonts(|f| f.glyph_width(&font, '#')).max(1.0);
            let count = (text_rect.width() / hash_width).floor().max(1.0) as usize;
            galley = painter.fonts(|f| layout_text(f, "#".repeat(count), format, color));
        }

        let y = text_rect.center().y - galley.size().y / 2.0;
        let x = match align {
            egui::Align::Min => text_rect.min.x,
            egui::Align::Center => text_rect.center().x - galley.size().x / 2.0,
            egui::Align::Max => text_rect.max.x - galley.size().x,
        };
        let pos = Pos2::new(x, y);
        if format.bold {
            // egui's bundled fonts have no bold face; overdraw to embolden.
            painter.galley(pos + Vec2::new(0.6, 0.0), galley.clone(), color);
        }
        painter.galley(pos, galley, color);
    }

    fn draw_row_headers(&self, painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, self.theme.header_bg);

        let start_row = self.scroll.first_visible_row;
        let mut y = rect.min.y - (self.scroll.offset_y - self.config.row_y(start_row));

        for row in start_row.. {
            if y >= rect.max.y {
                break;
            }

            let row_height = self.config.row_height(row);
            let header_rect = Rect::from_min_size(
                Pos2::new(rect.min.x, y),
                Vec2::new(HEADER_WIDTH, row_height),
            );

            // Highlight if selected
            if self
                .selection
                .contains(CellCoord::new(row, self.selection.active.col))
            {
                painter.rect_filled(header_rect, 0.0, self.theme.selection_bg);
            }

            // Draw header text (1-indexed)
            let text = format!("{}", row + 1);
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(12.0),
                self.theme.header_text,
            );
            let text_pos = Pos2::new(
                header_rect.center().x - galley.size().x / 2.0,
                header_rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(text_pos, galley, self.theme.header_text);

            // Draw bottom border
            painter.line_segment(
                [header_rect.left_bottom(), header_rect.right_bottom()],
                self.theme.grid_stroke(),
            );

            y += row_height;
        }

        // Draw right border
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            self.theme.grid_stroke(),
        );
    }

    fn draw_column_headers(&self, painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, self.theme.header_bg);

        let start_col = self.scroll.first_visible_col;
        let mut x = rect.min.x - (self.scroll.offset_x - self.config.column_x(start_col));

        for col in start_col.. {
            if x >= rect.max.x {
                break;
            }

            let col_width = self.config.column_width(col);
            let header_rect = Rect::from_min_size(
                Pos2::new(x, rect.min.y),
                Vec2::new(col_width, HEADER_HEIGHT),
            );

            // Highlight if selected
            if self
                .selection
                .contains(CellCoord::new(self.selection.active.row, col))
            {
                painter.rect_filled(header_rect, 0.0, self.theme.selection_bg);
            }

            // Draw header text (A, B, C, ..., AA, AB, ...)
            let text = column_to_letter(col);
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(12.0),
                self.theme.header_text,
            );
            let text_pos = Pos2::new(
                header_rect.center().x - galley.size().x / 2.0,
                header_rect.center().y - galley.size().y / 2.0,
            );
            painter.galley(text_pos, galley, self.theme.header_text);

            // Draw right border
            painter.line_segment(
                [header_rect.right_top(), header_rect.right_bottom()],
                self.theme.grid_stroke(),
            );

            x += col_width;
        }

        // Draw bottom border
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            self.theme.grid_stroke(),
        );
    }

    fn draw_selection(&self, painter: &egui::Painter, data_rect: Rect) {
        let range = self.selection.primary_range();

        // Calculate selection rectangle
        let sel_x = self.config.column_x(range.start.col) - self.scroll.offset_x + data_rect.min.x;
        let sel_y = self.config.row_y(range.start.row) - self.scroll.offset_y + data_rect.min.y;
        let sel_w: f32 = (range.start.col..=range.end.col)
            .map(|c| self.config.column_width(c))
            .sum();
        let sel_h: f32 = (range.start.row..=range.end.row)
            .map(|r| self.config.row_height(r))
            .sum();

        let sel_rect = Rect::from_min_size(Pos2::new(sel_x, sel_y), Vec2::new(sel_w, sel_h));

        // Draw selection fill
        if sel_rect.intersects(data_rect) {
            let clipped = sel_rect.intersect(data_rect);
            painter.rect_filled(clipped, 0.0, self.theme.selection_bg);
        }

        // Draw active cell border
        let active = self.selection.active;
        let active_x = self.config.column_x(active.col) - self.scroll.offset_x + data_rect.min.x;
        let active_y = self.config.row_y(active.row) - self.scroll.offset_y + data_rect.min.y;
        let active_rect = Rect::from_min_size(
            Pos2::new(active_x, active_y),
            Vec2::new(
                self.config.column_width(active.col),
                self.config.row_height(active.row),
            ),
        );

        if active_rect.intersects(data_rect) {
            painter.rect_stroke(
                active_rect,
                0.0,
                self.theme.active_cell_stroke(),
                StrokeKind::Outside,
            );
        }

        // Draw selection border (only if multi-cell)
        if (range.width() > 1 || range.height() > 1) && sel_rect.intersects(data_rect) {
            let clipped = sel_rect.intersect(data_rect);
            painter.rect_stroke(
                clipped,
                0.0,
                self.theme.selection_stroke(),
                StrokeKind::Outside,
            );
        }
    }

    /// Handle keyboard with consume_key - prevents keys from moving focus
    fn handle_keyboard_consume(&self, ui: &Ui) -> Option<NavigationKey> {
        let modifiers = ui.input(|i| i.modifiers);
        let ctx = ui.ctx();

        // Use consume_key to intercept arrow keys before egui focus system
        if modifiers.ctrl {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::ArrowUp)) {
                return Some(NavigationKey::CtrlUp);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::ArrowDown)) {
                return Some(NavigationKey::CtrlDown);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::ArrowLeft)) {
                return Some(NavigationKey::CtrlLeft);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::ArrowRight)) {
                return Some(NavigationKey::CtrlRight);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::Home)) {
                return Some(NavigationKey::CtrlHome);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::End)) {
                return Some(NavigationKey::CtrlEnd);
            }
        } else {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
                return Some(NavigationKey::Up);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
                return Some(NavigationKey::Down);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)) {
                return Some(NavigationKey::Left);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowRight)) {
                return Some(NavigationKey::Right);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Home)) {
                return Some(NavigationKey::Home);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::End)) {
                return Some(NavigationKey::End);
            }
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::PageUp)) {
            return Some(NavigationKey::PageUp);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::PageDown)) {
            return Some(NavigationKey::PageDown);
        }

        None
    }
}

/// Convert column index to letter(s): 0 -> A, 25 -> Z, 26 -> AA, etc.
fn column_to_letter(col: u32) -> String {
    let mut result = String::new();
    let mut n = col + 1;
    while n > 0 {
        n -= 1;
        result.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_past_the_first_screen() {
        let config = GridConfig::default();
        // Rows past 50 and columns past Z used to clamp to the first screen.
        assert_eq!(config.row_at_y(DEFAULT_ROW_HEIGHT * 120.5), 120);
        assert_eq!(config.column_at_x(DEFAULT_COLUMN_WIDTH * 30.5), 30);
        assert_eq!(config.row_at_y(-5.0), 0);
        assert_eq!(config.row_at_y(f32::MAX), MAX_ROW);
    }

    #[test]
    fn custom_sizes_shift_positions() {
        let mut config = GridConfig::default();
        config.column_widths.insert(1, 200.0);
        config.column_widths.insert(3, 0.0);
        assert_eq!(config.column_x(0), 0.0);
        assert_eq!(config.column_x(2), DEFAULT_COLUMN_WIDTH + 200.0);
        assert_eq!(config.column_x(5), DEFAULT_COLUMN_WIDTH * 3.0 + 200.0);
        assert_eq!(config.column_at_x(DEFAULT_COLUMN_WIDTH + 199.0), 1);
        assert_eq!(config.column_at_x(DEFAULT_COLUMN_WIDTH + 201.0), 2);
        // Column 3 is hidden (zero width), so the next pixel is column 4.
        assert_eq!(
            config.column_at_x(DEFAULT_COLUMN_WIDTH * 2.0 + 200.0 + 1.0),
            4
        );
        for col in [0, 1, 2, 4, 7, 40] {
            let x = config.column_x(col) + 0.5;
            assert_eq!(config.column_at_x(x), col, "column {col}");
        }
    }
}
