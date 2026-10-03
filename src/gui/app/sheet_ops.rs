//! Navigation, selection and whole-sheet edits: insert/delete/hide rows and
//! columns, sort, AutoFilter, freeze panes and merged cells.

use super::*;
use crate::calc::{EngineSnapshot, SortKey};
use crate::cell::{Axis, LineEdit, MAX_COL, MAX_ROW};
use crate::format::AutoFilter;
use crate::gui::grid::{HeaderSelect, SCROLLBAR};
use std::collections::{BTreeMap, BTreeSet};

/// The whole workbook's cells, formats and charts, for undoing edits that
/// touch many cells at once.
#[derive(Clone)]
pub(super) struct WorkbookState {
    engine: EngineSnapshot,
    charts: Vec<ChartDefinition>,
}

/// The AutoFilter menu for one column.
pub(super) struct FilterPopup {
    pub col: u32,
    pub pos: egui::Pos2,
    /// Every distinct value and whether it is shown
    pub values: Vec<(String, bool)>,
    pub search: String,
}

/// The Data > Sort dialog.
pub(super) struct SortDialog {
    pub range: CellRange,
    pub has_header: bool,
    /// (column, ascending); `None` column means the level is unused
    pub levels: Vec<(Option<u32>, bool)>,
}

impl SpreadsheetApp {
    // ------------------------------------------------------------------
    // Extents and merges
    // ------------------------------------------------------------------

    /// Last used row and column on the current sheet (A1 when empty).
    pub(super) fn used_extent(&self) -> CellCoord {
        self.engine
            .sheet_max_coord(self.current_sheet)
            .unwrap_or(CellCoord::new(0, 0))
    }

    /// Trim whole-row/column selections to the used area, so copying or
    /// sorting column A doesn't touch a million empty cells.
    pub(super) fn clamp_to_used(&self, range: CellRange) -> CellRange {
        let used = self.used_extent();
        CellRange::new(
            range.start,
            CellCoord::new(
                range.end.row.min(used.row.max(range.start.row)),
                range.end.col.min(used.col.max(range.start.col)),
            ),
        )
    }

    fn formatting(&self) -> Option<&crate::format::SheetFormatting> {
        self.engine.formatting(self.current_sheet)
    }

    /// The top-left cell of the merge `coord` is in, or `coord` itself.
    pub(super) fn snap_to_merge(&self, coord: CellCoord) -> CellCoord {
        self.formatting()
            .and_then(|f| f.merge_at(coord))
            .map_or(coord, |m| m.start)
    }

    fn is_hidden(&self, axis: Axis, index: u32) -> bool {
        self.formatting().is_some_and(|f| f.is_hidden(axis, index))
    }

    // ------------------------------------------------------------------
    // Navigation
    // ------------------------------------------------------------------

    /// One cell in a direction, skipping hidden rows/columns and stepping
    /// out of (or onto the top-left of) merged cells.
    pub(super) fn step(&self, from: CellCoord, dr: i32, dc: i32) -> CellCoord {
        let merge = self.formatting().and_then(|f| f.merge_at(from));
        // Leave a merge from its far edge.
        let mut row = match (merge, dr.signum()) {
            (Some(m), 1) => m.end.row,
            (Some(m), -1) => m.start.row,
            _ => from.row,
        } as i64;
        let mut col = match (merge, dc.signum()) {
            (Some(m), 1) => m.end.col,
            (Some(m), -1) => m.start.col,
            _ => from.col,
        } as i64;
        for _ in 0..dr.unsigned_abs() {
            loop {
                let next = row + dr.signum() as i64;
                if !(0..=MAX_ROW as i64).contains(&next) {
                    break;
                }
                row = next;
                if !self.is_hidden(Axis::Row, row as u32) {
                    break;
                }
            }
        }
        for _ in 0..dc.unsigned_abs() {
            loop {
                let next = col + dc.signum() as i64;
                if !(0..=MAX_COL as i64).contains(&next) {
                    break;
                }
                col = next;
                if !self.is_hidden(Axis::Column, col as u32) {
                    break;
                }
            }
        }
        self.snap_to_merge(CellCoord::new(row as u32, col as u32))
    }

    /// Ctrl+Arrow: the edge of the data block, or the next filled cell, or
    /// the sheet edge.
    pub(super) fn data_edge(&self, from: CellCoord, dr: i32, dc: i32) -> CellCoord {
        let vertical = dr != 0;
        let forward = dr > 0 || dc > 0;
        let (line, pos, max) = if vertical {
            (from.col, from.row, MAX_ROW)
        } else {
            (from.row, from.col, MAX_COL)
        };
        let filled: BTreeSet<u32> = self
            .engine
            .iter_sheet_inputs(self.current_sheet)
            .filter_map(|(c, _)| {
                if vertical {
                    (c.col == line).then_some(c.row)
                } else {
                    (c.row == line).then_some(c.col)
                }
            })
            .collect();
        let next = |i: u32| {
            if forward {
                i.checked_add(1).filter(|&n| n <= max)
            } else {
                i.checked_sub(1)
            }
        };
        let target = match next(pos) {
            None => pos,
            Some(n) if filled.contains(&pos) && filled.contains(&n) => {
                // Run to the end of this block.
                let mut i = n;
                while let Some(m) = next(i).filter(|m| filled.contains(m)) {
                    i = m;
                }
                i
            }
            Some(_) => {
                let found = if forward {
                    filled.range(pos + 1..).next().copied()
                } else {
                    filled.range(..pos).next_back().copied()
                };
                found.unwrap_or(if forward { max } else { 0 })
            }
        };
        let coord = if vertical {
            CellCoord::new(target, line)
        } else {
            CellCoord::new(line, target)
        };
        self.snap_to_merge(coord)
    }

    fn go(&mut self, coord: CellCoord, extend: bool, viewport: Vec2) {
        if extend {
            self.selection.extend_to(coord);
        } else {
            self.selection.move_to(coord);
        }
        self.scroll
            .scroll_to_cell(self.selection.active, &self.grid_config, viewport);
    }

    pub(super) fn navigate(&mut self, key: NavigationKey, shift: bool, viewport: Vec2) {
        let active = self.selection.active;
        let page = {
            let view = viewport.y - HEADER_HEIGHT - SCROLLBAR - self.grid_config.frozen_size().y;
            ((view / DEFAULT_ROW_HEIGHT).floor() as i32).max(1)
        };
        let used = self.used_extent();
        let target = match key {
            NavigationKey::Up => self.step(active, -1, 0),
            NavigationKey::Down => self.step(active, 1, 0),
            NavigationKey::Left => self.step(active, 0, -1),
            NavigationKey::Right => self.step(active, 0, 1),
            NavigationKey::PageUp => self.step(active, -page, 0),
            NavigationKey::PageDown => self.step(active, page, 0),
            NavigationKey::Home => CellCoord::new(active.row, 0),
            NavigationKey::End => {
                // The last filled cell in this row.
                let last = self
                    .engine
                    .iter_sheet_inputs(self.current_sheet)
                    .filter(|(c, _)| c.row == active.row)
                    .map(|(c, _)| c.col)
                    .max()
                    .unwrap_or(active.col);
                CellCoord::new(active.row, last)
            }
            NavigationKey::CtrlHome => {
                // The first cell outside frozen panes, as in Excel.
                CellCoord::new(self.grid_config.frozen_rows, self.grid_config.frozen_cols)
            }
            NavigationKey::CtrlEnd => used,
            NavigationKey::CtrlUp => self.data_edge(active, -1, 0),
            NavigationKey::CtrlDown => self.data_edge(active, 1, 0),
            NavigationKey::CtrlLeft => self.data_edge(active, 0, -1),
            NavigationKey::CtrlRight => self.data_edge(active, 0, 1),
            NavigationKey::SelectAll => {
                self.select_all_or_region();
                return;
            }
            NavigationKey::SelectColumn => {
                let r = self.selection.primary_range();
                self.select_lines(Axis::Column, r.start.col, r.end.col);
                return;
            }
            NavigationKey::SelectRow => {
                let r = self.selection.primary_range();
                self.select_lines(Axis::Row, r.start.row, r.end.row);
                return;
            }
        };
        let target = self.snap_to_merge(target);
        self.go(target, shift, viewport);
    }

    // ------------------------------------------------------------------
    // Selection
    // ------------------------------------------------------------------

    /// Select whole rows or columns `from..=to`; the active cell is the
    /// first cell of `from`, as in Excel.
    pub(super) fn select_lines(&mut self, axis: Axis, from: u32, to: u32) {
        let (anchor, active) = match axis {
            Axis::Column => (CellCoord::new(MAX_ROW, to), CellCoord::new(0, from)),
            Axis::Row => (CellCoord::new(to, MAX_COL), CellCoord::new(from, 0)),
        };
        self.selection.move_to(anchor);
        self.selection.extend_to(active);
    }

    /// Ctrl+A: the data block around the active cell first, then everything.
    pub(super) fn select_all_or_region(&mut self) {
        let current = self.selection.primary_range();
        match self
            .engine
            .current_region(self.current_sheet, self.selection.active)
        {
            Some(region) if region != current => {
                self.selection.move_to(region.end);
                self.selection.extend_to(region.start);
            }
            _ => self.select_everything(),
        }
    }

    pub(super) fn select_everything(&mut self) {
        self.selection.move_to(CellCoord::new(MAX_ROW, MAX_COL));
        self.selection.extend_to(CellCoord::new(0, 0));
    }

    pub(super) fn handle_header_select(&mut self, hs: HeaderSelect) {
        let from = match (hs.extend, self.header_anchor) {
            (true, Some((axis, i))) if axis == hs.axis => i,
            _ => hs.index,
        };
        if !hs.extend {
            self.header_anchor = Some((hs.axis, hs.index));
        }
        let (lo, hi) = (from.min(hs.index), from.max(hs.index));
        self.select_lines(hs.axis, lo, hi);
        // Keep the clicked line's first cell active.
        if from > hs.index {
            self.select_lines(hs.axis, hi, lo);
        }
    }

    /// Rows or columns the selection covers on `axis`, as (first, count).
    fn selected_lines(&self, axis: Axis) -> (u32, u32) {
        let r = self.selection.primary_range();
        match axis {
            Axis::Row => (r.start.row, r.end.row - r.start.row + 1),
            Axis::Column => (r.start.col, r.end.col - r.start.col + 1),
        }
    }

    // ------------------------------------------------------------------
    // Snapshot undo
    // ------------------------------------------------------------------

    pub(super) fn workbook_state(&self) -> WorkbookState {
        WorkbookState {
            engine: self.engine.snapshot(),
            charts: self.chart_windows.all_charts(),
        }
    }

    pub(super) fn restore_workbook_state(&mut self, state: &WorkbookState) {
        self.engine.restore(&state.engine);
        self.chart_windows.replace_all(state.charts.clone());
        self.sync_grid_config();
        self.modified = true;
        self.refresh_all_charts();
    }

    /// Run a whole-sheet edit as one undo step. `f` returns false when it
    /// changed nothing.
    pub(super) fn with_snapshot(&mut self, f: impl FnOnce(&mut Self) -> bool) {
        let before = self.workbook_state();
        let changed = self.batch(f);
        if changed {
            let after = self.workbook_state();
            self.undo_history
                .push(UndoAction::Snapshot(Box::new((before, after))));
            self.sync_grid_config();
            self.modified = true;
            self.refresh_all_charts();
        }
    }

    // ------------------------------------------------------------------
    // Insert, delete, hide
    // ------------------------------------------------------------------

    pub(super) fn insert_lines(&mut self, axis: Axis) {
        let (at, count) = self.selected_lines(axis);
        // Excel refuses rather than pushing data off the sheet.
        let used = self.engine.sheet_max_coord(self.current_sheet);
        let last_used = used.map_or(0, |u| axis.of(u));
        if used.is_some() && last_used >= at && last_used as u64 + count as u64 > axis.max() as u64
        {
            self.set_status("Can't insert: data would be pushed off the end of the sheet");
            return;
        }
        let sheet = self.current_sheet;
        let edit = LineEdit::insert(axis, at, count);
        self.with_snapshot(|app| {
            app.engine.apply_line_edit(sheet, &edit);
            app.chart_windows.apply_line_edit(sheet, &edit);
            true
        });
        let noun = line_noun(axis, count);
        self.set_status(&format!("Inserted {count} {noun}"));
    }

    pub(super) fn delete_lines(&mut self, axis: Axis) {
        let (at, count) = self.selected_lines(axis);
        let sheet = self.current_sheet;
        let edit = LineEdit::delete(axis, at, count);
        self.with_snapshot(|app| {
            app.engine.apply_line_edit(sheet, &edit);
            app.chart_windows.apply_line_edit(sheet, &edit);
            true
        });
        self.selection
            .move_to(self.snap_to_merge(self.selection.primary_range().start));
        let noun = line_noun(axis, count);
        self.set_status(&format!("Deleted {count} {noun}"));
    }

    pub(super) fn set_lines_hidden(&mut self, axis: Axis, hidden: bool) {
        let (at, count) = self.selected_lines(axis);
        let sheet = self.current_sheet;
        self.with_snapshot(|app| {
            let f = app.engine.formatting_mut(sheet);
            let set = match axis {
                Axis::Row => &mut f.hidden_rows,
                Axis::Column => &mut f.hidden_columns,
            };
            let lines = at..at.saturating_add(count);
            let before = set.len();
            if hidden {
                set.extend(lines);
            } else {
                set.retain(|i| !lines.contains(i));
            }
            set.len() != before
        });
    }

    // ------------------------------------------------------------------
    // Sort and filter
    // ------------------------------------------------------------------

    /// The range a sort or filter should use: the filter's range when the
    /// active cell is in it, else a multi-cell selection, else the data
    /// block around the active cell.
    pub(super) fn data_range(&self) -> Option<CellRange> {
        let active = self.selection.active;
        if let Some(filter) = self.formatting().and_then(|f| f.filter.as_ref()) {
            let r = filter.range;
            if (r.start.row..=r.end.row).contains(&active.row)
                && (r.start.col..=r.end.col).contains(&active.col)
            {
                return Some(r);
            }
        }
        let selection = self.selection.primary_range();
        if selection.start != selection.end {
            return Some(self.clamp_to_used(selection));
        }
        self.engine.current_region(self.current_sheet, active)
    }

    /// Excel's guess: the first row is a header when it is all text and the
    /// row below isn't, or when it is bold and the row below isn't.
    pub(super) fn guess_header(&self, range: CellRange) -> bool {
        if self
            .formatting()
            .and_then(|f| f.filter.as_ref())
            .is_some_and(|f| f.range == range)
        {
            return true;
        }
        if range.start.row >= range.end.row {
            return false;
        }
        let sheet = self.current_sheet;
        let cols = range.start.col..=range.end.col;
        let row = |r: u32| cols.clone().map(move |c| CellCoord::new(r, c));
        let is_text = |c: CellCoord| matches!(self.engine.get_value(sheet, c), CellResult::Text(_));
        let bold = |c: CellCoord| {
            self.formatting()
                .and_then(|f| f.effective(c))
                .is_some_and(|f| f.bold)
        };
        let first_all_text = row(range.start.row).all(is_text);
        let second_has_other = row(range.start.row + 1).any(|c| !is_text(c));
        let first_bold = row(range.start.row).any(bold) && !row(range.start.row + 1).any(bold);
        (first_all_text && second_has_other) || (first_all_text && first_bold)
    }

    pub(super) fn sort_by(&mut self, range: CellRange, keys: Vec<SortKey>, has_header: bool) {
        if keys.is_empty() {
            return;
        }
        let merged = self.formatting().is_some_and(|f| {
            f.merges.iter().any(|m| {
                m.start.row <= range.end.row
                    && m.end.row >= range.start.row
                    && m.start.col <= range.end.col
                    && m.end.col >= range.start.col
            })
        });
        if merged {
            self.set_status("Can't sort a range that contains merged cells");
            return;
        }
        let sheet = self.current_sheet;
        self.with_snapshot(|app| {
            let moved = app.engine.sort_range(sheet, range, &keys, has_header);
            app.engine.refresh_filter(sheet);
            moved
        });
        self.set_status("Sorted");
    }

    /// Sort the data around the selection by the active cell's column.
    pub(super) fn quick_sort(&mut self, ascending: bool) {
        let Some(range) = self.data_range() else {
            self.set_status("Select some data to sort");
            return;
        };
        let has_header = self.guess_header(range);
        let col = self
            .selection
            .active
            .col
            .clamp(range.start.col, range.end.col);
        self.sort_by(range, vec![SortKey { col, ascending }], has_header);
    }

    pub(super) fn open_sort_dialog(&mut self) {
        let Some(range) = self.data_range() else {
            self.set_status("Select some data to sort");
            return;
        };
        let col = self
            .selection
            .active
            .col
            .clamp(range.start.col, range.end.col);
        self.sort_dialog = Some(SortDialog {
            range,
            has_header: self.guess_header(range),
            levels: vec![(Some(col), true), (None, true), (None, true)],
        });
    }

    /// Turn the AutoFilter on (for the data around the selection) or off.
    pub(super) fn toggle_filter(&mut self) {
        let sheet = self.current_sheet;
        if self.formatting().is_some_and(|f| f.filter.is_some()) {
            self.with_snapshot(|app| {
                let f = app.engine.formatting_mut(sheet);
                if let Some(filter) = f.filter.take() {
                    for row in filter.range.start.row..=filter.range.end.row {
                        f.hidden_rows.remove(&row);
                    }
                }
                true
            });
            self.filter_popup = None;
            self.set_status("Filter removed");
            return;
        }
        let Some(range) = self.data_range() else {
            self.set_status("Select some data to filter");
            return;
        };
        if range.start.row == range.end.row {
            self.set_status("A filter needs a header row and data below it");
            return;
        }
        self.with_snapshot(|app| {
            app.engine.formatting_mut(sheet).filter = Some(AutoFilter {
                range,
                allowed: BTreeMap::new(),
            });
            true
        });
        self.set_status("Filter on: use the buttons in the header row");
    }

    pub(super) fn open_filter_popup(&mut self, col: u32, pos: egui::Pos2) {
        let Some(filter) = self.formatting().and_then(|f| f.filter.clone()) else {
            return;
        };
        let offset = col - filter.range.start.col;
        let allowed = filter.allowed.get(&offset);
        let values = self
            .engine
            .filter_values(self.current_sheet, filter.range, col)
            .into_iter()
            .map(|v| {
                let shown = allowed.is_none_or(|a| a.contains(&v));
                (v, shown)
            })
            .collect();
        self.filter_popup = Some(FilterPopup {
            col,
            pos,
            values,
            search: String::new(),
        });
    }

    /// Apply the popup's choices to its column.
    fn apply_filter_popup(&mut self, popup: &FilterPopup) {
        let sheet = self.current_sheet;
        let Some(filter) = self.formatting().and_then(|f| f.filter.clone()) else {
            return;
        };
        let offset = popup.col - filter.range.start.col;
        let all = popup.values.iter().all(|(_, on)| *on);
        let chosen: BTreeSet<String> = popup
            .values
            .iter()
            .filter(|(_, on)| *on)
            .map(|(v, _)| v.clone())
            .collect();
        self.with_snapshot(|app| {
            let f = app.engine.formatting_mut(sheet);
            if let Some(filter) = f.filter.as_mut() {
                if all {
                    filter.allowed.remove(&offset);
                } else {
                    filter.allowed.insert(offset, chosen);
                }
            }
            app.engine.refresh_filter(sheet);
            true
        });
    }

    /// The AutoFilter menu: sort, search, pick values.
    pub(super) fn show_filter_popup(&mut self, ctx: &egui::Context) {
        let Some(mut popup) = self.filter_popup.take() else {
            return;
        };
        let mut keep = true;
        let mut apply = false;
        let mut sort: Option<bool> = None;
        egui::Window::new("filter_popup")
            .title_bar(false)
            .fixed_pos(popup.pos)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(220.0);
                if ui.button("Sort A to Z").clicked() {
                    sort = Some(true);
                }
                if ui.button("Sort Z to A").clicked() {
                    sort = Some(false);
                }
                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut popup.search)
                        .hint_text("Search")
                        .desired_width(f32::INFINITY),
                );
                let query = popup.search.to_lowercase();
                let matches = |v: &str| query.is_empty() || v.to_lowercase().contains(&query);
                let mut all = popup
                    .values
                    .iter()
                    .filter(|(v, _)| matches(v))
                    .all(|(_, on)| *on);
                if ui.checkbox(&mut all, "(Select all)").changed() {
                    for (v, on) in &mut popup.values {
                        if matches(v) {
                            *on = all;
                        }
                    }
                }
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (value, on) in popup.values.iter_mut().filter(|(v, _)| matches(v)) {
                            let label = if value.is_empty() {
                                "(Blanks)"
                            } else {
                                value.as_str()
                            };
                            ui.checkbox(on, label);
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let any = popup.values.iter().any(|(_, on)| *on);
                    if ui.add_enabled(any, egui::Button::new("OK")).clicked() {
                        apply = true;
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
        if let Some(ascending) = sort {
            if let Some(filter) = self.formatting().and_then(|f| f.filter.clone()) {
                let key = SortKey {
                    col: popup.col,
                    ascending,
                };
                self.sort_by(filter.range, vec![key], true);
            }
            keep = false;
        }
        if apply {
            self.apply_filter_popup(&popup);
        }
        if keep {
            self.filter_popup = Some(popup);
        }
    }

    /// The Data > Sort dialog.
    pub(super) fn show_sort_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.sort_dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut run = false;
        let range = dialog.range;
        let header_row = range.start.row;
        let column_name = |app: &Self, col: u32, has_header: bool| -> String {
            let letter = crate::gui::grid::column_to_letter(col);
            if has_header {
                let value = app
                    .engine
                    .get_value(app.current_sheet, CellCoord::new(header_row, col));
                let text = display_text(&value, None);
                if !text.is_empty() {
                    return format!("{text} ({letter})");
                }
            }
            format!("Column {letter}")
        };
        egui::Window::new("Sort")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Range {}:{}",
                    range.start.to_a1(),
                    range.end.to_a1()
                ));
                ui.checkbox(&mut dialog.has_header, "My data has headers");
                ui.separator();
                for (i, (col, ascending)) in dialog.levels.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(if i == 0 { "Sort by" } else { "Then by" });
                        let selected = col.map_or("(none)".to_string(), |c| {
                            column_name(self, c, dialog.has_header)
                        });
                        egui::ComboBox::from_id_salt(("sort_col", i))
                            .width(160.0)
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                if i > 0 {
                                    ui.selectable_value(col, None, "(none)");
                                }
                                for c in range.start.col..=range.end.col {
                                    let name = column_name(self, c, dialog.has_header);
                                    ui.selectable_value(col, Some(c), name);
                                }
                            });
                        ui.selectable_value(ascending, true, "A to Z");
                        ui.selectable_value(ascending, false, "Z to A");
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Sort").clicked() {
                        run = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if run {
            let keys = dialog
                .levels
                .iter()
                .filter_map(|(col, ascending)| {
                    col.map(|col| SortKey {
                        col,
                        ascending: *ascending,
                    })
                })
                .collect();
            self.sort_by(range, keys, dialog.has_header);
        }
        if keep {
            self.sort_dialog = Some(dialog);
        }
    }

    // ------------------------------------------------------------------
    // Freeze panes and merges
    // ------------------------------------------------------------------

    pub(super) fn freeze(&mut self, rows: u32, cols: u32) {
        let sheet = self.current_sheet;
        self.engine.formatting_mut(sheet).frozen = (rows, cols);
        self.sync_grid_config();
        self.scroll = ScrollState::default();
        self.modified = true;
    }

    /// Merge & Center: merge the selection, or unmerge merges it touches.
    pub(super) fn toggle_merge(&mut self) {
        let sheet = self.current_sheet;
        let range = self.clamp_to_used_or_self(self.selection.primary_range());
        let touching: Vec<CellRange> = self
            .formatting()
            .map(|f| {
                f.merges
                    .iter()
                    .copied()
                    .filter(|m| {
                        m.start.row <= range.end.row
                            && m.end.row >= range.start.row
                            && m.start.col <= range.end.col
                            && m.end.col >= range.start.col
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !touching.is_empty() {
            self.with_snapshot(|app| {
                app.engine
                    .formatting_mut(sheet)
                    .merges
                    .retain(|m| !touching.contains(m));
                true
            });
            self.set_status("Unmerged");
            return;
        }
        if range.start == range.end {
            self.set_status("Select two or more cells to merge");
            return;
        }
        if (range.end.row - range.start.row + 1) as u64
            * (range.end.col - range.start.col + 1) as u64
            > 100_000
        {
            self.set_status("That range is too large to merge");
            return;
        }
        // Like Excel, a merge keeps only the top-left value.
        let others: Vec<CellCoord> = self
            .engine
            .iter_sheet_inputs(sheet)
            .map(|(c, _)| c)
            .filter(|c| {
                *c != range.start
                    && (range.start.row..=range.end.row).contains(&c.row)
                    && (range.start.col..=range.end.col).contains(&c.col)
            })
            .collect();
        if !others.is_empty() {
            let keep = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Merge cells")
                .set_description("Merging keeps only the upper-left value and discards the others.")
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show();
            if keep != rfd::MessageDialogResult::Ok {
                return;
            }
        }
        self.with_snapshot(|app| {
            for c in &others {
                app.engine.clear(sheet, *c);
            }
            let mut format = app
                .engine
                .formatting(sheet)
                .and_then(|f| f.effective(range.start))
                .cloned()
                .unwrap_or_default();
            format.h_align = crate::format::HAlign::Center;
            app.engine.set_cell_format(sheet, range.start, format);
            app.engine.formatting_mut(sheet).merges.push(range);
            true
        });
        self.selection.move_to(range.start);
        self.set_status("Merged");
    }

    /// Whole-line selections trimmed to the data; other ranges unchanged.
    pub(super) fn clamp_to_used_or_self(&self, range: CellRange) -> CellRange {
        if range.end.row == MAX_ROW || range.end.col == MAX_COL {
            self.clamp_to_used(range)
        } else {
            range
        }
    }
}

fn line_noun(axis: Axis, count: u32) -> &'static str {
    match (axis, count) {
        (Axis::Row, 1) => "row",
        (Axis::Row, _) => "rows",
        (Axis::Column, 1) => "column",
        (Axis::Column, _) => "columns",
    }
}
