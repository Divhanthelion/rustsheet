//! Spreadsheet grid widget
//!
//! The data area has up to four panes: frozen rows and columns stay put, the
//! rest scrolls. Every screen position goes through [`SpreadsheetGrid::col_left`]
//! / [`SpreadsheetGrid::row_top`] and their inverses, so freezing, hidden
//! lines and custom sizes are handled in one place.

use super::selection::Selection;
use super::theme::Theme;
use crate::calc::CalcEngine;
use crate::calc::CellResult;
use crate::cell::{Axis, CellCoord, CellError, CellRange, MAX_COL, MAX_ROW};
use crate::format::{
    CellFormat, DEFAULT_FONT_SIZE, HAlign, Rgb, SheetFormatting, VAlign, format_general,
    format_number,
};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::{BTreeMap, HashSet};

/// Default cell dimensions
pub const DEFAULT_COLUMN_WIDTH: f32 = 80.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 22.0;
pub const HEADER_WIDTH: f32 = 50.0;
pub const HEADER_HEIGHT: f32 = 24.0;
/// Thickness of the grid's scrollbars.
pub const SCROLLBAR: f32 = 12.0;

/// Text size of an 11pt (default) cell.
pub const CELL_FONT_SIZE: f32 = 13.0;
/// Smallest width or height a drag can resize to.
const MIN_RESIZE: f32 = 8.0;
/// Padding between a cell's edge and its text.
const PADDING: f32 = 4.0;
/// How far text may spill into empty neighbors, in cells.
const MAX_OVERFLOW_CELLS: usize = 32;

/// Which header border is being dragged.
pub type ResizeAxis = Axis;

/// Column widths, row heights and frozen panes for the sheet on screen.
/// Only sizes that differ from the defaults are stored; hidden lines are
/// stored as size 0.
#[derive(Clone, Default)]
pub struct GridConfig {
    pub column_widths: BTreeMap<u32, f32>,
    pub row_heights: BTreeMap<u32, f32>,
    pub frozen_rows: u32,
    pub frozen_cols: u32,
}

impl GridConfig {
    pub fn for_sheet(formatting: Option<&SheetFormatting>) -> Self {
        let Some(f) = formatting else {
            return Self::default();
        };
        let mut config = Self {
            column_widths: f.column_widths.clone(),
            row_heights: f.row_heights.clone(),
            frozen_rows: f.frozen.0,
            frozen_cols: f.frozen.1,
        };
        for &col in &f.hidden_columns {
            config.column_widths.insert(col, 0.0);
        }
        for &row in &f.hidden_rows {
            config.row_heights.insert(row, 0.0);
        }
        config
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

    pub fn size(&self, axis: Axis, index: u32) -> f32 {
        match axis {
            Axis::Row => self.row_height(index),
            Axis::Column => self.column_width(index),
        }
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

    /// Height of the frozen rows, width of the frozen columns.
    pub fn frozen_size(&self) -> Vec2 {
        Vec2::new(
            self.column_x(self.frozen_cols),
            self.row_y(self.frozen_rows),
        )
    }

    /// Whether a row or column is hidden (size 0).
    pub fn is_hidden(&self, axis: Axis, index: u32) -> bool {
        self.size(axis, index) <= 0.0
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
    crate::format::display_text(value, format)
}

fn font_for(format: &CellFormat) -> egui::FontId {
    egui::FontId::proportional(
        CELL_FONT_SIZE * format.font_size_or_default() as f32 / DEFAULT_FONT_SIZE as f32,
    )
}

fn to_color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Lay out a cell's text with its font style; `wrap_width` wraps it.
fn layout_text(
    fonts: &egui::text::Fonts,
    text: String,
    format: &CellFormat,
    color: Color32,
    wrap_width: Option<f32>,
) -> std::sync::Arc<egui::Galley> {
    let line = |on: bool| {
        if on {
            Stroke::new(1.0_f32, color)
        } else {
            Stroke::NONE
        }
    };
    let mut job = egui::text::LayoutJob::single_section(
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
    if let Some(width) = wrap_width {
        job.wrap.max_width = width.max(1.0);
    }
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
        .filter(|&coord| formatting.is_none_or(|f| f.merge_at(coord).is_none()))
        .collect();
    ctx.fonts(|fonts| {
        coords
            .into_iter()
            .filter_map(|coord| {
                let format = formatting
                    .and_then(|f| f.effective(coord))
                    .unwrap_or(&default_format);
                let value = engine.get_value(sheet, coord);
                let text = cell_text(&value, format, 11)?;
                let galley = layout_text(fonts, text.text, format, Color32::WHITE, None);
                let bold_extra = if format.bold { 1.0 } else { 0.0 };
                Some(galley.size().x + bold_extra + 3.0 * PADDING)
            })
            .reduce(f32::max)
    })
    .map(|w| w.max(MIN_RESIZE * 3.0))
}

/// Height that fits every value in `row`, wrapping where cells wrap, for
/// double-clicking a row border.
pub fn fit_row_height(
    ctx: &egui::Context,
    engine: &CalcEngine,
    config: &GridConfig,
    sheet: u32,
    row: u32,
) -> f32 {
    let default_format = CellFormat::default();
    let formatting = engine.formatting(sheet);
    let coords: Vec<CellCoord> = engine
        .iter_sheet_inputs(sheet)
        .map(|(coord, _)| coord)
        .filter(|coord| coord.row == row)
        .collect();
    ctx.fonts(|fonts| {
        coords
            .into_iter()
            .filter_map(|coord| {
                let format = formatting
                    .and_then(|f| f.effective(coord))
                    .unwrap_or(&default_format);
                let value = engine.get_value(sheet, coord);
                let text = cell_text(&value, format, 11)?;
                let wrap = format
                    .wrap
                    .then(|| config.column_width(coord.col) - 2.0 * PADDING);
                let galley = layout_text(fonts, text.text, format, Color32::WHITE, wrap);
                Some(galley.size().y + 2.0 * PADDING)
            })
            .reduce(f32::max)
    })
    .unwrap_or(DEFAULT_ROW_HEIGHT)
    .max(DEFAULT_ROW_HEIGHT)
}

/// Commands in the grid's right-click menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextAction {
    Cut,
    Copy,
    Paste,
    ClearContents,
    ClearFormatting,
    /// Insert as many rows/columns as are selected, before the selection
    Insert(Axis),
    Delete(Axis),
    Hide(Axis),
    Unhide(Axis),
    SortAscending,
    SortDescending,
    ToggleFilter,
}

/// A click or drag on a row or column header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderSelect {
    pub axis: Axis,
    pub index: u32,
    /// Extend from the current anchor (Shift-click or dragging)
    pub extend: bool,
}

/// Scroll state for the grid. Offsets are into the scrolling pane, so the
/// first scrolled row is the first one after the frozen rows when 0.
#[derive(Default, Clone)]
pub struct ScrollState {
    pub offset_x: f32,
    pub offset_y: f32,
    pub first_visible_row: u32,
    pub first_visible_col: u32,
}

impl ScrollState {
    /// Size of the scrolling pane for a grid of `viewport_size`.
    fn scroll_view(config: &GridConfig, viewport_size: Vec2) -> Vec2 {
        viewport_size
            - Vec2::new(HEADER_WIDTH + SCROLLBAR, HEADER_HEIGHT + SCROLLBAR)
            - config.frozen_size()
    }

    pub fn scroll_to_cell(&mut self, coord: CellCoord, config: &GridConfig, viewport_size: Vec2) {
        let view = Self::scroll_view(config, viewport_size);
        let frozen_x = config.column_x(config.frozen_cols);
        let frozen_y = config.row_y(config.frozen_rows);

        // Frozen rows/columns are always visible on their axis.
        if coord.col >= config.frozen_cols {
            let cell_x = config.column_x(coord.col) - frozen_x;
            let cell_w = config.column_width(coord.col);
            if cell_x < self.offset_x {
                self.offset_x = cell_x;
            } else if cell_x + cell_w > self.offset_x + view.x {
                self.offset_x = (cell_x + cell_w - view.x).min(cell_x);
            }
        }
        if coord.row >= config.frozen_rows {
            let cell_y = config.row_y(coord.row) - frozen_y;
            let cell_h = config.row_height(coord.row);
            if cell_y < self.offset_y {
                self.offset_y = cell_y;
            } else if cell_y + cell_h > self.offset_y + view.y {
                self.offset_y = (cell_y + cell_h - view.y).min(cell_y);
            }
        }
        self.set_offset(self.offset_x, self.offset_y, config);
    }

    /// Move to an offset, clamped to the sheet.
    pub fn set_offset(&mut self, x: f32, y: f32, config: &GridConfig) {
        let frozen_x = config.column_x(config.frozen_cols);
        let frozen_y = config.row_y(config.frozen_rows);
        let max_x = config.column_x(MAX_COL + 1) - frozen_x;
        let max_y = config.row_y(MAX_ROW + 1) - frozen_y;
        self.offset_x = x.clamp(0.0, max_x.max(0.0));
        self.offset_y = y.clamp(0.0, max_y.max(0.0));
        self.first_visible_col = config
            .column_at_x(frozen_x + self.offset_x)
            .max(config.frozen_cols);
        self.first_visible_row = config
            .row_at_y(frozen_y + self.offset_y)
            .max(config.frozen_rows);
    }
}

/// Response from grid interaction
#[derive(Default)]
pub struct GridResponse {
    /// Cell that was clicked
    pub clicked_cell: Option<CellCoord>,
    /// Shift was held for the click (extend the selection)
    pub clicked_with_shift: bool,
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
    /// A row border was double-clicked: fit the row to its contents
    pub autofit_row: Option<u32>,
    /// Cell under a right-click, to select before the menu acts
    pub right_clicked_cell: Option<CellCoord>,
    /// Header under a right-click, to select before the menu acts
    pub right_clicked_header: Option<(Axis, u32)>,
    /// Command picked from a right-click menu
    pub context_action: Option<ContextAction>,
    /// Click or drag on a row/column header
    pub header_select: Option<HeaderSelect>,
    /// The corner box was clicked
    pub select_all: bool,
    /// The fill handle is being dragged to this cell
    pub fill_to: Option<CellCoord>,
    /// The fill handle was released
    pub fill_released: bool,
    /// A filter button was clicked: (column, where to open its menu)
    pub filter_button: Option<(u32, Pos2)>,
    /// New scroll offsets from the wheel or scrollbars
    pub scroll_to: Option<Vec2>,
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
    SelectAll,
    SelectColumn,
    SelectRow,
}

/// A visible row or column: index, screen start, size, frozen.
#[derive(Clone, Copy)]
struct Line {
    index: u32,
    start: f32,
    size: f32,
    frozen: bool,
}

/// The spreadsheet grid widget
pub struct SpreadsheetGrid<'a> {
    sheet_index: u32,
    engine: &'a CalcEngine,
    selection: &'a Selection,
    config: &'a GridConfig,
    scroll: &'a ScrollState,
    theme: &'a Theme,
    /// Last used row and column, which sizes the scrollbars
    used: CellCoord,
    /// Show the fill handle (off while editing)
    fill_handle: bool,
    /// Cells the fill handle would fill, outlined while dragging
    fill_preview: Option<CellRange>,
    /// Screen rect of the data area, set at the start of `show`
    data: Rect,
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
            used: CellCoord::new(0, 0),
            fill_handle: true,
            fill_preview: None,
            data: Rect::NOTHING,
        }
    }

    /// The last used cell, so the scrollbars cover the data.
    pub fn with_used_extent(mut self, used: CellCoord) -> Self {
        self.used = used;
        self
    }

    pub fn with_fill_handle(mut self, show: bool) -> Self {
        self.fill_handle = show;
        self
    }

    pub fn with_fill_preview(mut self, range: Option<CellRange>) -> Self {
        self.fill_preview = range;
        self
    }

    fn formatting(&self) -> Option<&'a SheetFormatting> {
        self.engine.formatting(self.sheet_index)
    }

    // ------------------------------------------------------------------
    // Geometry
    // ------------------------------------------------------------------

    /// Screen x of a column's left edge.
    fn col_left(&self, col: u32) -> f32 {
        let c = self.config;
        if col < c.frozen_cols {
            self.data.min.x + c.column_x(col)
        } else {
            self.data.min.x + c.column_x(c.frozen_cols) + c.column_x(col)
                - c.column_x(c.frozen_cols)
                - self.scroll.offset_x
        }
    }

    /// Screen y of a row's top edge.
    fn row_top(&self, row: u32) -> f32 {
        let c = self.config;
        if row < c.frozen_rows {
            self.data.min.y + c.row_y(row)
        } else {
            self.data.min.y + c.row_y(row) - self.scroll.offset_y
        }
    }

    /// Screen rect of a range, ignoring panes (for single-pane ranges).
    fn range_rect(&self, range: CellRange) -> Rect {
        Rect::from_min_max(
            Pos2::new(
                self.col_left(range.start.col),
                self.row_top(range.start.row),
            ),
            Pos2::new(
                self.col_left(range.end.col) + self.config.column_width(range.end.col),
                self.row_top(range.end.row) + self.config.row_height(range.end.row),
            ),
        )
    }

    fn col_at(&self, x: f32) -> u32 {
        let c = self.config;
        let local = x - self.data.min.x;
        let frozen_w = c.column_x(c.frozen_cols);
        if local < frozen_w {
            c.column_at_x(local).min(c.frozen_cols.saturating_sub(1))
        } else {
            c.column_at_x(local + self.scroll.offset_x)
        }
    }

    fn row_at(&self, y: f32) -> u32 {
        let c = self.config;
        let local = y - self.data.min.y;
        let frozen_h = c.row_y(c.frozen_rows);
        if local < frozen_h {
            c.row_at_y(local).min(c.frozen_rows.saturating_sub(1))
        } else {
            c.row_at_y(local + self.scroll.offset_y)
        }
    }

    /// The cell under a screen position, clamped into the data area.
    fn cell_at(&self, pos: Pos2) -> CellCoord {
        let p = self.data.clamp(pos);
        CellCoord::new(self.row_at(p.y), self.col_at(p.x))
    }

    fn visible_lines(&self, axis: Axis) -> Vec<Line> {
        let c = self.config;
        let (frozen, first, max_screen) = match axis {
            Axis::Row => (
                c.frozen_rows,
                self.scroll.first_visible_row,
                self.data.max.y,
            ),
            Axis::Column => (
                c.frozen_cols,
                self.scroll.first_visible_col,
                self.data.max.x,
            ),
        };
        let start_of = |i: u32| match axis {
            Axis::Row => self.row_top(i),
            Axis::Column => self.col_left(i),
        };
        let mut lines = Vec::new();
        for index in 0..frozen {
            let size = c.size(axis, index);
            let start = start_of(index);
            if start >= max_screen {
                break;
            }
            if size > 0.0 {
                lines.push(Line {
                    index,
                    start,
                    size,
                    frozen: true,
                });
            }
        }
        let mut index = first.max(frozen);
        let mut start = start_of(index);
        while start < max_screen && index <= axis.max() {
            let size = c.size(axis, index);
            if size > 0.0 {
                lines.push(Line {
                    index,
                    start,
                    size,
                    frozen: false,
                });
            }
            start += size;
            index += 1;
        }
        lines
    }

    /// Screen rect of one pane.
    fn pane_rect(&self, frozen_row: bool, frozen_col: bool) -> Rect {
        let f = self.config.frozen_size();
        let d = self.data;
        let (x0, x1) = if frozen_col {
            (d.min.x, d.min.x + f.x)
        } else {
            (d.min.x + f.x, d.max.x)
        };
        let (y0, y1) = if frozen_row {
            (d.min.y, d.min.y + f.y)
        } else {
            (d.min.y + f.y, d.max.y)
        };
        Rect::from_min_max(
            Pos2::new(x0, y0),
            Pos2::new(x1.min(d.max.x), y1.min(d.max.y)),
        )
    }

    // ------------------------------------------------------------------
    // Show
    // ------------------------------------------------------------------

    /// Render the grid and handle interaction
    pub fn show(mut self, ui: &mut Ui) -> GridResponse {
        let mut response = GridResponse::default();

        let available_rect = ui.available_rect_before_wrap();
        let viewport_size = available_rect.size();

        // Use a stable ID for the grid
        let grid_id = egui::Id::new("spreadsheet_grid");

        // Allocate space and interact with the grid using the same ID
        let (grid_rect, _) = ui.allocate_exact_size(viewport_size, Sense::hover());
        self.data = Rect::from_min_max(
            grid_rect.min + Vec2::new(HEADER_WIDTH, HEADER_HEIGHT),
            grid_rect.max - Vec2::splat(SCROLLBAR),
        );
        let data_rect = self.data;
        let grid_response = ui.interact(data_rect, grid_id, Sense::click_and_drag());

        // NOTE: We do NOT request focus every frame - this breaks TextEdit in dialogs
        // Focus is only requested on specific events (click, navigation key)
        // See egui bug #5187 - repeated request_focus() breaks text input

        if ui.is_rect_visible(grid_rect) {
            let painter = ui.painter_at(grid_rect);
            painter.rect_filled(grid_rect, 0.0, self.theme.cell_bg);

            let rows = self.visible_lines(Axis::Row);
            let cols = self.visible_lines(Axis::Column);
            self.draw_cells(&painter, &rows, &cols);
            self.draw_selection(&painter);
            if let Some(range) = self.fill_preview {
                let p = painter.with_clip_rect(self.data);
                let rect = self.range_rect(range);
                let stroke = Stroke::new(1.5_f32, self.theme.selection_border);
                // Dashed outline of the cells that will be filled.
                for (a, b) in [
                    (rect.left_top(), rect.right_top()),
                    (rect.right_top(), rect.right_bottom()),
                    (rect.right_bottom(), rect.left_bottom()),
                    (rect.left_bottom(), rect.left_top()),
                ] {
                    p.extend(egui::Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
                }
            }
            self.draw_frozen_lines(&painter);
            self.draw_headers(&painter, grid_rect, &rows, &cols);

            // Interactive pieces are added after the grid's own interaction,
            // so they take the pointer where they overlap.
            self.header_interaction(ui, grid_id, grid_rect, &mut response);
            self.resize_handles(ui, grid_id, grid_rect, &rows, &cols, &mut response);
            self.filter_buttons(ui, grid_id, &painter, &cols, &mut response);
            self.fill_handle(ui, grid_id, &painter, &mut response);
            self.scrollbars(ui, grid_id, grid_rect, &painter, &mut response);

            // Handle drag for multi-cell selection
            if grid_response.drag_started() {
                if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                    response.drag_started = Some(self.cell_at(pos));
                }
            }
            if grid_response.dragged() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    response.drag_to = Some(self.cell_at(pos));
                }
            }
            if grid_response.drag_stopped() {
                response.drag_ended = true;
            }

            // Handle clicks (single click when not dragging)
            if grid_response.clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    response.clicked_cell = Some(self.cell_at(pos));
                    response.clicked_with_shift = ui.input(|i| i.modifiers.shift);
                }
            }
            if grid_response.double_clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    response.double_clicked_cell = Some(self.cell_at(pos));
                }
            }
            if grid_response.secondary_clicked() {
                if let Some(pos) = grid_response.interact_pointer_pos() {
                    response.right_clicked_cell = Some(self.cell_at(pos));
                }
            }
            let filter_on = self.formatting().is_some_and(|f| f.filter.is_some());
            grid_response.context_menu(|ui| {
                cell_menu(ui, filter_on, &mut response.context_action);
            });

            // The wheel scrolls the grid only when the pointer is over it.
            if ui.rect_contains_pointer(grid_rect) && !ui.ctx().is_context_menu_open() {
                let (mut delta, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
                if shift && delta.x == 0.0 {
                    delta = Vec2::new(delta.y, 0.0);
                }
                if delta != Vec2::ZERO {
                    let base = response
                        .scroll_to
                        .unwrap_or(Vec2::new(self.scroll.offset_x, self.scroll.offset_y));
                    response.scroll_to = Some(base - delta);
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
        if grid_response.clicked() || grid_response.secondary_clicked() {
            ui.ctx().memory_mut(|m| m.request_focus(grid_id));
        }

        // A short description for screen readers: the active cell and value.
        let active = self.selection.active;
        let value = self.engine.get_value(self.sheet_index, active);
        let shown = display_text(&value, self.formatting().and_then(|f| f.effective(active)));
        let label = format!(
            "{}, {}",
            active.to_a1(),
            if shown.is_empty() { "blank" } else { &shown }
        );
        grid_response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label.clone())
        });

        response
    }

    // ------------------------------------------------------------------
    // Cells
    // ------------------------------------------------------------------

    fn draw_cells(&self, painter: &egui::Painter, rows: &[Line], cols: &[Line]) {
        let formatting = self.formatting();
        let default_format = CellFormat::default();
        let format_of = |coord: CellCoord| -> &CellFormat {
            formatting
                .and_then(|f| f.effective(coord))
                .unwrap_or(&default_format)
        };
        let merge_of = |coord: CellCoord| formatting.and_then(|f| f.merge_at(coord));

        for frozen_row in [true, false] {
            for frozen_col in [true, false] {
                let pane = self.pane_rect(frozen_row, frozen_col);
                if pane.width() <= 0.0 || pane.height() <= 0.0 {
                    continue;
                }
                let p = painter.with_clip_rect(pane.intersect(painter.clip_rect()));
                let pane_rows: Vec<&Line> =
                    rows.iter().filter(|l| l.frozen == frozen_row).collect();
                let pane_cols: Vec<&Line> =
                    cols.iter().filter(|l| l.frozen == frozen_col).collect();
                // Borders are drawn last so neighboring fills don't cover them.
                let mut borders: Vec<[Pos2; 2]> = Vec::new();
                let mut texts: Vec<(CellCoord, Rect)> = Vec::new();
                let mut drawn_merges: HashSet<(CellCoord, CellCoord)> = HashSet::new();

                // Pass 1: backgrounds, grid lines, borders.
                for row in &pane_rows {
                    for col in &pane_cols {
                        let coord = CellCoord::new(row.index, col.index);
                        let rect = Rect::from_min_size(
                            Pos2::new(col.start, row.start),
                            Vec2::new(col.size, row.size),
                        );
                        let merge = merge_of(coord);
                        // A merge looks like its top-left cell.
                        let owner = merge.map_or(coord, |m| m.start);
                        let format = format_of(owner);
                        let fill = format.fill;
                        let bg = match fill {
                            Some(fill) => to_color32(fill),
                            None if (row.index + col.index) % 2 == 0 => self.theme.cell_bg,
                            None => self.theme.cell_bg_alt,
                        };
                        p.rect_filled(rect, 0.0, bg);

                        // Filled cells hide grid lines, as in Excel; so do the
                        // inside edges of a merge.
                        if fill.is_none() {
                            let right_edge = merge.is_none_or(|m| col.index == m.end.col);
                            let bottom_edge = merge.is_none_or(|m| row.index == m.end.row);
                            if right_edge {
                                p.line_segment(
                                    [rect.right_top(), rect.right_bottom()],
                                    self.theme.grid_stroke(),
                                );
                            }
                            if bottom_edge {
                                p.line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    self.theme.grid_stroke(),
                                );
                            }
                        }

                        let own = formatting.and_then(|f| f.effective(coord));
                        if let Some(b) = own.map(|f| f.borders) {
                            let r = rect;
                            let edges = [
                                (b.top, [r.left_top(), r.right_top()]),
                                (b.right, [r.right_top(), r.right_bottom()]),
                                (b.bottom, [r.left_bottom(), r.right_bottom()]),
                                (b.left, [r.left_top(), r.left_bottom()]),
                            ];
                            borders.extend(edges.into_iter().filter(|(on, _)| *on).map(|(_, l)| l));
                        }

                        match merge {
                            Some(m) => {
                                if drawn_merges.insert((m.start, m.end)) {
                                    texts.push((m.start, self.range_rect(m)));
                                }
                            }
                            None => texts.push((coord, rect)),
                        }
                    }
                }

                // Pass 2: text. It may spill over empty neighbors, which is
                // why it waits until every background in the pane is down.
                for (coord, rect) in texts {
                    let value = self.engine.get_value(self.sheet_index, coord);
                    if matches!(value, CellResult::Empty) {
                        continue;
                    }
                    let format = format_of(coord);
                    let merged = merge_of(coord).is_some();
                    let room = if merged {
                        rect
                    } else {
                        self.overflow_room(coord, rect, &value, format, &pane_cols)
                    };
                    self.draw_cell_content(&p, rect, room, &value, format);
                }

                let border_stroke = Stroke::new(1.0_f32, self.theme.text_normal);
                for line in borders {
                    p.line_segment(line, border_stroke);
                }
            }
        }
    }

    /// The area text may use: its cell, widened over empty neighbors when
    /// unwrapped text doesn't fit (left text spills right, right text left,
    /// centered both ways). Numbers never spill.
    fn overflow_room(
        &self,
        coord: CellCoord,
        rect: Rect,
        value: &CellResult,
        format: &CellFormat,
        cols: &[&Line],
    ) -> Rect {
        if format.wrap || !matches!(value, CellResult::Text(_)) {
            return rect;
        }
        let formatting = self.formatting();
        let empty = |col: u32| {
            let c = CellCoord::new(coord.row, col);
            self.engine.get_input(self.sheet_index, c).is_none()
                && formatting.is_none_or(|f| f.merge_at(c).is_none())
        };
        let Some(pos) = cols.iter().position(|l| l.index == coord.col) else {
            return rect;
        };
        let (left, right) = match format.h_align {
            HAlign::General | HAlign::Left => (false, true),
            HAlign::Right => (true, false),
            HAlign::Center => (true, true),
        };
        let mut room = rect;
        if right {
            for l in cols.iter().skip(pos + 1).take(MAX_OVERFLOW_CELLS) {
                if !empty(l.index) {
                    break;
                }
                room.max.x = l.start + l.size;
            }
        }
        if left {
            for l in cols[..pos].iter().rev().take(MAX_OVERFLOW_CELLS) {
                if !empty(l.index) {
                    break;
                }
                room.min.x = l.start;
            }
        }
        // Centered text stays centered on its own cell.
        if left && right {
            let half = (rect.center().x - room.min.x).min(room.max.x - rect.center().x);
            room = Rect::from_x_y_ranges(
                rect.center().x - half..=rect.center().x + half,
                room.y_range(),
            );
        }
        room
    }

    fn draw_cell_content(
        &self,
        painter: &egui::Painter,
        cell: Rect,
        room: Rect,
        value: &CellResult,
        format: &CellFormat,
    ) {
        let text_rect = cell.shrink(PADDING);
        let room = room.shrink2(Vec2::new(PADDING, 0.0));
        let font = font_for(format);

        let digit_width = painter.fonts(|f| f.glyph_width(&font, '0'));
        let max_len = (text_rect.width() / digit_width).floor().max(1.0) as usize;
        let Some(text) = cell_text(value, format, max_len) else {
            return;
        };

        // A format color ([Red]) wins over the font color. With a fill and no
        // font color, pick black or white so the text stays readable.
        let color = match (text.format_color, format.font_color, format.fill) {
            (Some(c), _, _) | (None, Some(c), _) => to_color32(c),
            (None, None, Some(fill)) if fill.luminance() > 0.5 => Color32::BLACK,
            (None, None, Some(_)) => Color32::WHITE,
            (None, None, None) => match text.kind {
                TextKind::Number => self.theme.text_number,
                TextKind::Error => self.theme.text_error,
                _ => self.theme.text_normal,
            },
        };

        let align = match (format.h_align, text.kind) {
            (HAlign::Left, _) => egui::Align::Min,
            (HAlign::Center, _) => egui::Align::Center,
            (HAlign::Right, _) => egui::Align::Max,
            (HAlign::General, TextKind::Number) => egui::Align::Max,
            (HAlign::General, TextKind::Text) => egui::Align::Min,
            (HAlign::General, _) => egui::Align::Center,
        };

        let wrap = format.wrap.then_some(text_rect.width());
        let mut galley = painter.fonts(|f| layout_text(f, text.text, format, color, wrap));
        // Numbers never spill into neighbors; like Excel, show #### instead.
        if text.kind == TextKind::Number && galley.size().x > text_rect.width() {
            let hash_width = painter.fonts(|f| f.glyph_width(&font, '#')).max(1.0);
            let count = (text_rect.width() / hash_width).floor().max(1.0) as usize;
            galley = painter.fonts(|f| layout_text(f, "#".repeat(count), format, color, None));
        }

        let size = galley.size();
        let y = match format.v_align {
            VAlign::Top => text_rect.min.y,
            VAlign::Center => text_rect.center().y - size.y / 2.0,
            VAlign::Bottom => text_rect.max.y - size.y,
        }
        // Text taller than its row starts at the top, like Excel.
        .max(cell.min.y + 1.0)
        .min(text_rect.max.y - size.y.min(text_rect.height()));
        let x = match align {
            egui::Align::Min => text_rect.min.x,
            egui::Align::Center => text_rect.center().x - size.x / 2.0,
            egui::Align::Max => text_rect.max.x - size.x,
        };
        let clip = room
            .union(text_rect)
            .intersect(Rect::from_x_y_ranges(room.x_range(), cell.y_range()));
        let p = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let pos = Pos2::new(x, y);
        if format.bold {
            // egui's bundled fonts have no bold face; overdraw to embolden.
            p.galley(pos + Vec2::new(0.6, 0.0), galley.clone(), color);
        }
        p.galley(pos, galley, color);
    }

    // ------------------------------------------------------------------
    // Selection, frozen lines, headers
    // ------------------------------------------------------------------

    fn draw_selection(&self, painter: &egui::Painter) {
        let range = self.selection_range();
        let multi = range.start != range.end;
        let c = self.config;
        for frozen_row in [true, false] {
            for frozen_col in [true, false] {
                let pane = self.pane_rect(frozen_row, frozen_col);
                if pane.width() <= 0.0 || pane.height() <= 0.0 {
                    continue;
                }
                // The part of the selection in this pane.
                let (r0, r1) = if frozen_row {
                    (
                        range.start.row,
                        range.end.row.min(c.frozen_rows.saturating_sub(1)),
                    )
                } else {
                    (range.start.row.max(c.frozen_rows), range.end.row)
                };
                let (c0, c1) = if frozen_col {
                    (
                        range.start.col,
                        range.end.col.min(c.frozen_cols.saturating_sub(1)),
                    )
                } else {
                    (range.start.col.max(c.frozen_cols), range.end.col)
                };
                if r0 > r1
                    || c0 > c1
                    || (frozen_row && c.frozen_rows == 0)
                    || (frozen_col && c.frozen_cols == 0)
                {
                    continue;
                }
                let rect = self.range_rect(CellRange::new(
                    CellCoord::new(r0, c0),
                    CellCoord::new(r1, c1),
                ));
                let p = painter.with_clip_rect(pane.intersect(painter.clip_rect()));
                if multi {
                    p.rect_filled(rect, 0.0, self.theme.selection_bg);
                    p.rect_stroke(rect, 0.0, self.theme.selection_stroke(), StrokeKind::Inside);
                }
                // The active cell (or its merge) gets the strong outline.
                let active = self.selection.active;
                let active_range = self
                    .formatting()
                    .and_then(|f| f.merge_at(active))
                    .unwrap_or(CellRange::single(active));
                p.rect_stroke(
                    self.range_rect(active_range),
                    0.0,
                    self.theme.active_cell_stroke(),
                    StrokeKind::Inside,
                );
            }
        }
    }

    /// The selection, widened to cover any merge it touches.
    fn selection_range(&self) -> CellRange {
        let mut range = self.selection.primary_range();
        if let Some(f) = self.formatting() {
            for m in &f.merges {
                let overlaps = m.start.row <= range.end.row
                    && m.end.row >= range.start.row
                    && m.start.col <= range.end.col
                    && m.end.col >= range.start.col;
                if overlaps {
                    range = CellRange::new(
                        CellCoord::new(
                            range.start.row.min(m.start.row),
                            range.start.col.min(m.start.col),
                        ),
                        CellCoord::new(range.end.row.max(m.end.row), range.end.col.max(m.end.col)),
                    );
                }
            }
        }
        range
    }

    fn draw_frozen_lines(&self, painter: &egui::Painter) {
        let f = self.config.frozen_size();
        let stroke = Stroke::new(1.5_f32, self.theme.header_text);
        let d = self.data;
        if self.config.frozen_rows > 0 && f.y < d.height() {
            let y = d.min.y + f.y;
            painter.line_segment([Pos2::new(d.min.x, y), Pos2::new(d.max.x, y)], stroke);
        }
        if self.config.frozen_cols > 0 && f.x < d.width() {
            let x = d.min.x + f.x;
            painter.line_segment([Pos2::new(x, d.min.y), Pos2::new(x, d.max.y)], stroke);
        }
    }

    fn draw_headers(&self, painter: &egui::Painter, grid_rect: Rect, rows: &[Line], cols: &[Line]) {
        let range = self.selection.primary_range();
        let row_header = Rect::from_min_max(
            Pos2::new(grid_rect.min.x, self.data.min.y),
            Pos2::new(self.data.min.x, self.data.max.y),
        );
        let col_header = Rect::from_min_max(
            Pos2::new(self.data.min.x, grid_rect.min.y),
            Pos2::new(self.data.max.x, self.data.min.y),
        );
        let whole_rows = range.start.col == 0 && range.end.col == MAX_COL;
        let whole_cols = range.start.row == 0 && range.end.row == MAX_ROW;

        let p = painter.with_clip_rect(row_header);
        p.rect_filled(row_header, 0.0, self.theme.header_bg);
        for row in rows {
            let rect = Rect::from_min_size(
                Pos2::new(row_header.min.x, row.start),
                Vec2::new(HEADER_WIDTH, row.size),
            );
            if (range.start.row..=range.end.row).contains(&row.index) {
                let bg = if whole_rows {
                    self.theme.selection_border
                } else {
                    self.theme.selection_bg
                };
                p.rect_filled(
                    rect,
                    0.0,
                    bg.gamma_multiply(if whole_rows { 0.35 } else { 1.0 }),
                );
            }
            self.header_label(&p, rect, &(row.index + 1).to_string());
            p.line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                self.theme.grid_stroke(),
            );
        }
        p.line_segment(
            [row_header.right_top(), row_header.right_bottom()],
            self.theme.grid_stroke(),
        );

        let p = painter.with_clip_rect(col_header);
        p.rect_filled(col_header, 0.0, self.theme.header_bg);
        for col in cols {
            let rect = Rect::from_min_size(
                Pos2::new(col.start, col_header.min.y),
                Vec2::new(col.size, HEADER_HEIGHT),
            );
            if (range.start.col..=range.end.col).contains(&col.index) {
                let bg = if whole_cols {
                    self.theme.selection_border
                } else {
                    self.theme.selection_bg
                };
                p.rect_filled(
                    rect,
                    0.0,
                    bg.gamma_multiply(if whole_cols { 0.35 } else { 1.0 }),
                );
            }
            self.header_label(&p, rect, &column_to_letter(col.index));
            p.line_segment(
                [rect.right_top(), rect.right_bottom()],
                self.theme.grid_stroke(),
            );
        }
        p.line_segment(
            [col_header.left_bottom(), col_header.right_bottom()],
            self.theme.grid_stroke(),
        );

        // Corner box: click to select all. A small triangle hints at it.
        let corner = Rect::from_min_max(grid_rect.min, self.data.min);
        painter.rect_filled(corner, 0.0, self.theme.header_bg);
        let t = corner.right_bottom() - Vec2::splat(4.0);
        painter.add(egui::Shape::convex_polygon(
            vec![t, t - Vec2::new(10.0, 0.0), t - Vec2::new(0.0, 10.0)],
            self.theme.grid_line,
            Stroke::NONE,
        ));
    }

    fn header_label(&self, painter: &egui::Painter, rect: Rect, text: &str) {
        let galley = painter.layout_no_wrap(
            text.to_string(),
            egui::FontId::proportional(12.0),
            self.theme.header_text,
        );
        if galley.size().x > rect.width() {
            return;
        }
        painter.galley(
            rect.center() - galley.size() / 2.0,
            galley,
            self.theme.header_text,
        );
    }

    fn header_interaction(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        grid_rect: Rect,
        response: &mut GridResponse,
    ) {
        let row_header = Rect::from_min_max(
            Pos2::new(grid_rect.min.x, self.data.min.y),
            Pos2::new(self.data.min.x, self.data.max.y),
        );
        let col_header = Rect::from_min_max(
            Pos2::new(self.data.min.x, grid_rect.min.y),
            Pos2::new(self.data.max.x, self.data.min.y),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        for (axis, rect) in [(Axis::Row, row_header), (Axis::Column, col_header)] {
            let r = ui.interact(
                rect,
                grid_id.with(("headers", axis)),
                Sense::click_and_drag(),
            );
            let index_at = |pos: Pos2| match axis {
                Axis::Row => self.row_at(pos.y.clamp(self.data.min.y, self.data.max.y - 1.0)),
                Axis::Column => self.col_at(pos.x.clamp(self.data.min.x, self.data.max.x - 1.0)),
            };
            if r.drag_started() || r.clicked() {
                if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                    response.header_select = Some(HeaderSelect {
                        axis,
                        index: index_at(pos),
                        extend: shift,
                    });
                }
            }
            if r.dragged() {
                if let Some(pos) = r.interact_pointer_pos() {
                    response.header_select = Some(HeaderSelect {
                        axis,
                        index: index_at(pos),
                        extend: true,
                    });
                }
            }
            if r.secondary_clicked() {
                if let Some(pos) = r.interact_pointer_pos() {
                    response.right_clicked_header = Some((axis, index_at(pos)));
                }
            }
            r.context_menu(|ui| header_menu(ui, axis, &mut response.context_action));
        }

        let corner = Rect::from_min_max(grid_rect.min, self.data.min);
        if ui
            .interact(corner, grid_id.with("corner"), Sense::click())
            .clicked()
        {
            response.select_all = true;
        }
    }

    fn resize_handles(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        grid_rect: Rect,
        rows: &[Line],
        cols: &[Line],
        response: &mut GridResponse,
    ) {
        for col in cols {
            let x = col.start + col.size;
            if x <= self.data.min.x || x > self.data.max.x {
                continue;
            }
            let handle = Rect::from_center_size(
                Pos2::new(x, grid_rect.min.y + HEADER_HEIGHT / 2.0),
                Vec2::new(8.0, HEADER_HEIGHT),
            );
            let r = ui.interact(
                handle,
                grid_id.with(("col_resize", col.index)),
                Sense::click_and_drag(),
            );
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            }
            if r.dragged() {
                if let Some(p) = r.interact_pointer_pos() {
                    response.resize =
                        Some((Axis::Column, col.index, (p.x - col.start).max(MIN_RESIZE)));
                }
            }
            if r.drag_stopped() {
                response.resize_ended = true;
            }
            if r.double_clicked() {
                response.autofit_column = Some(col.index);
            }
        }
        for row in rows {
            let y = row.start + row.size;
            if y <= self.data.min.y || y > self.data.max.y {
                continue;
            }
            let handle = Rect::from_center_size(
                Pos2::new(grid_rect.min.x + HEADER_WIDTH / 2.0, y),
                Vec2::new(HEADER_WIDTH, 6.0),
            );
            let r = ui.interact(
                handle,
                grid_id.with(("row_resize", row.index)),
                Sense::click_and_drag(),
            );
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
            }
            if r.dragged() {
                if let Some(p) = r.interact_pointer_pos() {
                    response.resize =
                        Some((Axis::Row, row.index, (p.y - row.start).max(MIN_RESIZE)));
                }
            }
            if r.drag_stopped() {
                response.resize_ended = true;
            }
            if r.double_clicked() {
                response.autofit_row = Some(row.index);
            }
        }
    }

    /// Drop-down buttons in an AutoFilter's header row.
    fn filter_buttons(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        painter: &egui::Painter,
        cols: &[Line],
        response: &mut GridResponse,
    ) {
        let Some(filter) = self.formatting().and_then(|f| f.filter.as_ref()) else {
            return;
        };
        let header_row = filter.range.start.row;
        if self.config.is_hidden(Axis::Row, header_row) {
            return;
        }
        let top = self.row_top(header_row);
        let height = self.config.row_height(header_row);
        let in_view = |y: f32| y >= self.data.min.y - 1.0 && y < self.data.max.y;
        if !in_view(top + height / 2.0) {
            return;
        }
        let p = painter.with_clip_rect(self.data);
        for col in cols
            .iter()
            .filter(|c| (filter.range.start.col..=filter.range.end.col).contains(&c.index))
        {
            let size = (height - 4.0).clamp(10.0, 16.0);
            let rect = Rect::from_min_size(
                Pos2::new(
                    col.start + col.size - size - 2.0,
                    top + (height - size) / 2.0,
                ),
                Vec2::splat(size),
            );
            let active = filter
                .allowed
                .contains_key(&(col.index - filter.range.start.col));
            let r = ui.interact(rect, grid_id.with(("filter", col.index)), Sense::click());
            let bg = if active || r.hovered() {
                self.theme.selection_border
            } else {
                self.theme.header_bg
            };
            p.rect_filled(rect, 2.0, bg);
            p.rect_stroke(rect, 2.0, self.theme.grid_stroke(), StrokeKind::Inside);
            let c = rect.center();
            let fg = if active {
                Color32::WHITE
            } else {
                self.theme.header_text
            };
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(-4.0, -2.0),
                    c + Vec2::new(4.0, -2.0),
                    c + Vec2::new(0.0, 3.0),
                ],
                fg,
                Stroke::NONE,
            ));
            if r.clicked() {
                response.filter_button = Some((col.index, rect.left_bottom()));
            }
            r.on_hover_text(if active {
                "Filtered: change filter"
            } else {
                "Filter or sort"
            });
        }
    }

    /// The small square at the selection's corner that drags to fill.
    fn fill_handle(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        painter: &egui::Painter,
        response: &mut GridResponse,
    ) {
        if !self.fill_handle {
            return;
        }
        let range = self.selection_range();
        let corner = self.range_rect(range).right_bottom();
        if !self.data.expand(1.0).contains(corner) {
            return;
        }
        let square = Rect::from_center_size(corner, Vec2::splat(6.0));
        painter.rect_filled(square.expand(1.0), 0.0, self.theme.cell_bg);
        painter.rect_filled(square, 0.0, self.theme.active_cell_border);
        let r = ui.interact(
            square.expand(3.0),
            grid_id.with("fill_handle"),
            Sense::drag(),
        );
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                response.fill_to = Some(self.cell_at(pos));
            }
        }
        if r.drag_stopped() {
            response.fill_released = true;
        }
        r.on_hover_text("Drag to fill");
    }

    fn scrollbars(
        &self,
        ui: &mut Ui,
        grid_id: egui::Id,
        grid_rect: Rect,
        painter: &egui::Painter,
        response: &mut GridResponse,
    ) {
        let c = self.config;
        let frozen = c.frozen_size();
        let view = self.data.size() - frozen;
        // Scroll range: the data plus a screen, growing as you scroll on.
        let data_w = c.column_x(self.used.col + 1) - c.column_x(c.frozen_cols);
        let data_h = c.row_y(self.used.row + 1) - c.row_y(c.frozen_rows);
        let content = Vec2::new(
            data_w.max(self.scroll.offset_x + view.x) + view.x,
            data_h.max(self.scroll.offset_y + view.y) + view.y,
        );
        let offset = Vec2::new(self.scroll.offset_x, self.scroll.offset_y);

        let bars = [
            (
                Rect::from_min_max(
                    Pos2::new(self.data.max.x, self.data.min.y),
                    Pos2::new(grid_rect.max.x, self.data.max.y),
                ),
                1usize,
            ),
            (
                Rect::from_min_max(
                    Pos2::new(self.data.min.x, self.data.max.y),
                    Pos2::new(self.data.max.x, grid_rect.max.y),
                ),
                0usize,
            ),
        ];
        let mut new_offset = offset;
        for (track, axis) in bars {
            painter.rect_filled(track, 0.0, self.theme.header_bg);
            let track_len = if axis == 1 {
                track.height()
            } else {
                track.width()
            };
            let (view_len, content_len) = (view[axis], content[axis].max(1.0));
            let thumb_len = (track_len * view_len / content_len).clamp(24.0, track_len);
            let max_offset = (content_len - view_len).max(1.0);
            let thumb_pos = (offset[axis] / max_offset).clamp(0.0, 1.0) * (track_len - thumb_len);
            let thumb = if axis == 1 {
                Rect::from_min_size(
                    track.min + Vec2::new(2.0, thumb_pos),
                    Vec2::new(track.width() - 4.0, thumb_len),
                )
            } else {
                Rect::from_min_size(
                    track.min + Vec2::new(thumb_pos, 2.0),
                    Vec2::new(thumb_len, track.height() - 4.0),
                )
            };
            let r = ui.interact(
                track,
                grid_id.with(("scrollbar", axis)),
                Sense::click_and_drag(),
            );
            let color = if r.dragged() || r.hovered() {
                self.theme.header_text.gamma_multiply(0.7)
            } else {
                self.theme.grid_line.gamma_multiply(2.0)
            };
            painter.rect_filled(thumb, 4.0, color);
            if r.dragged() {
                let delta = r.drag_delta()[axis];
                new_offset[axis] += delta * max_offset / (track_len - thumb_len).max(1.0);
            } else if r.clicked() {
                // Click beside the thumb: page toward the click.
                if let Some(p) = r.interact_pointer_pos() {
                    let before = if axis == 1 {
                        p.y < thumb.min.y
                    } else {
                        p.x < thumb.min.x
                    };
                    new_offset[axis] += if before { -view_len } else { view_len };
                }
            }
        }
        // The square where the bars meet.
        painter.rect_filled(
            Rect::from_min_max(self.data.max, grid_rect.max),
            0.0,
            self.theme.header_bg,
        );
        if new_offset != offset {
            response.scroll_to = Some(new_offset);
        }
    }

    /// Handle keyboard with consume_key - prevents keys from moving focus
    fn handle_keyboard_consume(&self, ui: &Ui) -> Option<NavigationKey> {
        let modifiers = ui.input(|i| i.modifiers);
        let ctx = ui.ctx();

        if modifiers.command && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::A)) {
            return Some(NavigationKey::SelectAll);
        }
        if modifiers.command
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Space))
        {
            return Some(NavigationKey::SelectColumn);
        }
        if modifiers.shift && !modifiers.command {
            let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::Space));
            if pressed {
                // Don't also start editing with a space.
                ctx.input_mut(|i| {
                    i.events
                        .retain(|e| !matches!(e, egui::Event::Text(t) if t == " "))
                });
                return Some(NavigationKey::SelectRow);
            }
        }

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

fn menu_item(
    ui: &mut Ui,
    label: &str,
    shortcut: &str,
    action: ContextAction,
    out: &mut Option<ContextAction>,
) {
    let button = egui::Button::new(label).shortcut_text(shortcut);
    if ui.add(button).clicked() {
        *out = Some(action);
        ui.close_menu();
    }
}

/// Right-click menu on cells.
fn cell_menu(ui: &mut Ui, filter_on: bool, out: &mut Option<ContextAction>) {
    menu_item(ui, "Cut", "Ctrl+X", ContextAction::Cut, out);
    menu_item(ui, "Copy", "Ctrl+C", ContextAction::Copy, out);
    menu_item(ui, "Paste", "Ctrl+V", ContextAction::Paste, out);
    ui.separator();
    menu_item(
        ui,
        "Insert rows",
        "Ctrl+Shift+=",
        ContextAction::Insert(Axis::Row),
        out,
    );
    menu_item(
        ui,
        "Insert columns",
        "",
        ContextAction::Insert(Axis::Column),
        out,
    );
    menu_item(
        ui,
        "Delete rows",
        "Ctrl+-",
        ContextAction::Delete(Axis::Row),
        out,
    );
    menu_item(
        ui,
        "Delete columns",
        "",
        ContextAction::Delete(Axis::Column),
        out,
    );
    ui.separator();
    menu_item(ui, "Sort A to Z", "", ContextAction::SortAscending, out);
    menu_item(ui, "Sort Z to A", "", ContextAction::SortDescending, out);
    let filter_label = if filter_on { "Remove filter" } else { "Filter" };
    menu_item(
        ui,
        filter_label,
        "Ctrl+Shift+L",
        ContextAction::ToggleFilter,
        out,
    );
    ui.separator();
    menu_item(
        ui,
        "Clear contents",
        "Delete",
        ContextAction::ClearContents,
        out,
    );
    menu_item(
        ui,
        "Clear formatting",
        "",
        ContextAction::ClearFormatting,
        out,
    );
}

/// Right-click menu on row or column headers.
fn header_menu(ui: &mut Ui, axis: Axis, out: &mut Option<ContextAction>) {
    let noun = match axis {
        Axis::Row => "rows",
        Axis::Column => "columns",
    };
    menu_item(ui, "Cut", "Ctrl+X", ContextAction::Cut, out);
    menu_item(ui, "Copy", "Ctrl+C", ContextAction::Copy, out);
    menu_item(ui, "Paste", "Ctrl+V", ContextAction::Paste, out);
    ui.separator();
    menu_item(
        ui,
        &format!("Insert {noun}"),
        "",
        ContextAction::Insert(axis),
        out,
    );
    menu_item(
        ui,
        &format!("Delete {noun}"),
        "",
        ContextAction::Delete(axis),
        out,
    );
    ui.separator();
    menu_item(
        ui,
        &format!("Hide {noun}"),
        "",
        ContextAction::Hide(axis),
        out,
    );
    menu_item(
        ui,
        &format!("Unhide {noun}"),
        "",
        ContextAction::Unhide(axis),
        out,
    );
    ui.separator();
    menu_item(
        ui,
        "Clear contents",
        "Delete",
        ContextAction::ClearContents,
        out,
    );
}

/// Convert column index to letter(s): 0 -> A, 25 -> Z, 26 -> AA, etc.
pub fn column_to_letter(col: u32) -> String {
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

    #[test]
    fn hidden_lines_and_frozen_panes_come_from_formatting() {
        let mut f = SheetFormatting::default();
        f.hidden_rows.insert(3);
        f.hidden_columns.insert(1);
        f.frozen = (2, 1);
        let config = GridConfig::for_sheet(Some(&f));
        assert!(config.is_hidden(Axis::Row, 3));
        assert!(config.is_hidden(Axis::Column, 1));
        assert_eq!(
            config.frozen_size(),
            Vec2::new(DEFAULT_COLUMN_WIDTH, 2.0 * DEFAULT_ROW_HEIGHT)
        );
    }

    #[test]
    fn scrolling_keeps_frozen_rows_and_finds_the_first_scrolled_row() {
        let config = GridConfig {
            frozen_rows: 2,
            ..Default::default()
        };
        let mut scroll = ScrollState::default();
        let viewport = Vec2::new(800.0, 400.0);
        scroll.scroll_to_cell(CellCoord::new(100, 0), &config, viewport);
        assert!(scroll.first_visible_row > 2);
        assert!(scroll.first_visible_row <= 100);
        // Frozen rows never scroll the view.
        let before = scroll.offset_y;
        scroll.scroll_to_cell(CellCoord::new(0, 0), &config, viewport);
        assert_eq!(scroll.offset_y, before);
        scroll.set_offset(-50.0, -50.0, &config);
        assert_eq!((scroll.offset_x, scroll.offset_y), (0.0, 0.0));
        assert_eq!(scroll.first_visible_row, 2);
    }

    #[test]
    fn column_letters() {
        assert_eq!(column_to_letter(0), "A");
        assert_eq!(column_to_letter(25), "Z");
        assert_eq!(column_to_letter(26), "AA");
        assert_eq!(column_to_letter(MAX_COL), "XFD");
    }
}
