//! Spreadsheet grid widget

use super::selection::Selection;
use super::theme::Theme;
use crate::calc::CalcEngine;
use crate::calc::CellResult;
use crate::cell::{CellCoord, CellError};
use eframe::egui::{self, Key, Pos2, Rect, Sense, StrokeKind, Ui, Vec2};

/// Default cell dimensions
pub const DEFAULT_COLUMN_WIDTH: f32 = 80.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 22.0;
pub const HEADER_WIDTH: f32 = 50.0;
pub const HEADER_HEIGHT: f32 = 24.0;

/// Configuration for the grid widget
pub struct GridConfig {
    pub column_widths: Vec<f32>,
    pub row_heights: Vec<f32>,
    pub frozen_rows: u32,
    pub frozen_cols: u32,
    pub visible_rows: u32,
    pub visible_cols: u32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            column_widths: vec![DEFAULT_COLUMN_WIDTH; 100],
            row_heights: vec![DEFAULT_ROW_HEIGHT; 1000],
            frozen_rows: 0,
            frozen_cols: 0,
            visible_rows: 50,
            visible_cols: 26,
        }
    }
}

impl GridConfig {
    pub fn column_width(&self, col: u32) -> f32 {
        self.column_widths
            .get(col as usize)
            .copied()
            .unwrap_or(DEFAULT_COLUMN_WIDTH)
    }

    pub fn row_height(&self, row: u32) -> f32 {
        self.row_heights
            .get(row as usize)
            .copied()
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    /// Get x position of column left edge relative to data area
    pub fn column_x(&self, col: u32) -> f32 {
        (0..col).map(|c| self.column_width(c)).sum()
    }

    /// Get y position of row top edge relative to data area
    pub fn row_y(&self, row: u32) -> f32 {
        (0..row).map(|r| self.row_height(r)).sum()
    }

    /// Find column at x position
    pub fn column_at_x(&self, x: f32) -> u32 {
        let mut accum = 0.0;
        for col in 0..self.visible_cols {
            accum += self.column_width(col);
            if accum > x {
                return col;
            }
        }
        self.visible_cols.saturating_sub(1)
    }

    /// Find row at y position
    pub fn row_at_y(&self, y: f32) -> u32 {
        let mut accum = 0.0;
        for row in 0..self.visible_rows {
            accum += self.row_height(row);
            if accum > y {
                return row;
            }
        }
        self.visible_rows.saturating_sub(1)
    }
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

    fn draw_cells(&self, painter: &egui::Painter, data_rect: Rect) {
        let clip_rect = data_rect;

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

                    // Draw cell background (alternate colors)
                    let bg_color = if (row + col) % 2 == 0 {
                        self.theme.cell_bg
                    } else {
                        self.theme.cell_bg_alt
                    };
                    painter.rect_filled(cell_rect, 0.0, bg_color);

                    // Draw grid lines
                    painter.line_segment(
                        [cell_rect.right_top(), cell_rect.right_bottom()],
                        self.theme.grid_stroke(),
                    );
                    painter.line_segment(
                        [cell_rect.left_bottom(), cell_rect.right_bottom()],
                        self.theme.grid_stroke(),
                    );

                    // Get cell value and render
                    let value = self.engine.get_value(self.sheet_index, coord);
                    self.draw_cell_content(painter, cell_rect, &value);
                }

                x += col_width;
            }

            y += row_height;
        }
    }

    fn draw_cell_content(&self, painter: &egui::Painter, rect: Rect, value: &CellResult) {
        let padding = 4.0;
        let text_rect = rect.shrink(padding);

        let (text, color, align) = match value {
            CellResult::Empty => return,
            CellResult::Value(n) => {
                // Format number nicely
                let s = if n.fract() == 0.0 && n.abs() < 1e10 {
                    format!("{}", *n as i64)
                } else {
                    format!("{:.10}", n)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                (s, self.theme.text_number, egui::Align::Max) // Right align numbers
            }
            CellResult::Text(s) => {
                (s.clone(), self.theme.text_normal, egui::Align::Min) // Left align text
            }
            CellResult::Bool(b) => (
                if *b { "TRUE" } else { "FALSE" }.to_string(),
                self.theme.text_normal,
                egui::Align::Center,
            ),
            CellResult::Error(e) => {
                let s = match e {
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
                };
                (s.to_string(), self.theme.text_error, egui::Align::Center)
            }
        };

        // Clip text to cell bounds
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), color);

        let text_pos = match align {
            egui::Align::Min => Pos2::new(
                text_rect.min.x,
                text_rect.center().y - galley.size().y / 2.0,
            ),
            egui::Align::Center => Pos2::new(
                text_rect.center().x - galley.size().x / 2.0,
                text_rect.center().y - galley.size().y / 2.0,
            ),
            egui::Align::Max => Pos2::new(
                text_rect.max.x - galley.size().x,
                text_rect.center().y - galley.size().y / 2.0,
            ),
        };

        painter.galley(text_pos, galley, color);
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
        if range.width() > 1 || range.height() > 1 {
            if sel_rect.intersects(data_rect) {
                let clipped = sel_rect.intersect(data_rect);
                painter.rect_stroke(
                    clipped,
                    0.0,
                    self.theme.selection_stroke(),
                    StrokeKind::Outside,
                );
            }
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
