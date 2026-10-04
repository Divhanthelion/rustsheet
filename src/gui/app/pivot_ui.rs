//! Pivot tables in the app: the PivotTable dialog, writing a table's cells,
//! and refreshing.

use super::*;
use crate::format::Rgb;
use crate::pivot::{self, Aggregate, PivotCell, PivotTable, PivotValue, RowKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Area {
    Rows,
    Columns,
    Values,
    Filters,
}

const AREAS: [(Area, &str); 4] = [
    (Area::Filters, "Filters"),
    (Area::Columns, "Columns"),
    (Area::Rows, "Rows"),
    (Area::Values, "Values"),
];

const HEADER_FILL: Rgb = Rgb(0xDD, 0xEB, 0xF7);

pub(super) struct PivotDialog {
    /// The table being changed, as (sheet, index); `None` for a new one
    existing: Option<(u32, usize)>,
    /// "Data!A1:D100"
    pub source: String,
    pub new_sheet: bool,
    /// Top-left cell when not on a new sheet
    pub destination: String,
    pub table: PivotTable,
    /// The source's field names and items, read for `loaded`
    headers: Vec<String>,
    items: Vec<Vec<String>>,
    loaded: String,
    /// The field whose items are shown for picking
    picking: Option<usize>,
    error: Option<String>,
}

/// "Sheet1!A1:D9", "'My data'!A:D" or "A1:D9" (on `current`).
fn parse_source(text: &str, current: &str) -> Option<(String, CellRange)> {
    let t = text.trim().trim_start_matches('=');
    let (sheet, range) = match t.rsplit_once('!') {
        Some((s, r)) => (
            s.trim()
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .map_or_else(|| s.trim().to_string(), |s| s.replace("''", "'")),
            r,
        ),
        None => (current.to_string(), t),
    };
    let r = range.trim().replace('$', "").to_ascii_uppercase();
    let range = CellRange::from_a1(&r).or_else(|| {
        // Whole columns: A:D
        let (a, b) = r.split_once(':')?;
        let col = |s: &str| CellCoord::from_a1(&format!("{s}1")).map(|c| c.col);
        Some(CellRange::new(
            CellCoord::new(0, col(a)?),
            CellCoord::new(crate::cell::MAX_ROW, col(b)?),
        ))
    })?;
    (!sheet.is_empty()).then_some((sheet, range))
}

fn source_text(sheet: &str, range: CellRange) -> String {
    let plain = sheet.chars().all(|c| c.is_alphanumeric() || c == '_');
    if plain {
        format!("{sheet}!{range}")
    } else {
        format!("'{}'!{range}", sheet.replace('\'', "''"))
    }
}

impl PivotDialog {
    fn area(&mut self, area: Area) -> &mut Vec<usize> {
        match area {
            Area::Rows => &mut self.table.rows,
            Area::Columns => &mut self.table.columns,
            Area::Filters => &mut self.table.filters,
            Area::Values => unreachable!("values have their own list"),
        }
    }

    pub fn add(&mut self, field: usize, area: Area) {
        if area == Area::Values {
            // Text fields count; number fields sum.
            let numeric = self.items.get(field).is_some_and(|items| {
                items
                    .iter()
                    .all(|i| i.is_empty() || crate::format::parse_typed_number(i).is_some())
            });
            self.table.values.push(PivotValue {
                field,
                aggregate: if numeric {
                    Aggregate::Sum
                } else {
                    Aggregate::Count
                },
            });
            return;
        }
        // A field is in one of rows, columns and filters at a time.
        for a in [Area::Rows, Area::Columns, Area::Filters] {
            self.area(a).retain(|&f| f != field);
        }
        self.area(area).push(field);
    }

    /// Read field names and items when the source text changes.
    pub fn load(&mut self, engine: &CalcEngine, current: &str) {
        if self.loaded == self.source {
            return;
        }
        self.loaded = self.source.clone();
        self.headers.clear();
        self.items.clear();
        let Some((sheet, range)) = parse_source(&self.source, current) else {
            self.error = Some("Enter the data as a range, like Sheet1!A1:D100.".into());
            return;
        };
        let changed = sheet != self.table.source_sheet || range != self.table.source;
        self.table.source_sheet = sheet;
        self.table.source = range;
        match engine.pivot_source(&self.table) {
            Ok(src) => {
                self.items = (0..src.headers.len())
                    .map(|f| pivot::distinct_items(&src.records, f))
                    .collect();
                self.headers = src.headers;
                self.error = None;
                if changed {
                    // Fields past the new range are dropped.
                    let n = self.headers.len();
                    for list in [
                        &mut self.table.rows,
                        &mut self.table.columns,
                        &mut self.table.filters,
                    ] {
                        list.retain(|&f| f < n);
                    }
                    self.table.values.retain(|v| v.field < n);
                    self.table.hidden.retain(|&f, _| f < n);
                }
            }
            Err(e) => self.error = Some(e),
        }
    }
}

impl SpreadsheetApp {
    /// The pivot table at the active cell, as (sheet, index).
    pub(super) fn pivot_at_active(&self) -> Option<(u32, usize)> {
        let sheet = self.current_sheet;
        let active = self.selection.active;
        self.engine
            .formatting(sheet)?
            .pivots
            .iter()
            .position(|p| p.contains(active))
            .map(|i| (sheet, i))
    }

    /// Insert > PivotTable (new) or PivotTable Fields (the table at the
    /// active cell).
    pub(super) fn open_pivot_dialog(&mut self, edit: bool) {
        let current = self.sheet_names[self.current_sheet as usize].clone();
        if edit {
            let Some((sheet, i)) = self.pivot_at_active() else {
                self.set_status("Select a cell in a PivotTable first");
                return;
            };
            let table = self.engine.formatting(sheet).unwrap().pivots[i].clone();
            self.pivot_dialog = Some(PivotDialog {
                existing: Some((sheet, i)),
                source: source_text(&table.source_sheet, table.source),
                new_sheet: false,
                destination: table.anchor.to_a1(),
                table,
                headers: Vec::new(),
                items: Vec::new(),
                loaded: String::new(),
                picking: None,
                error: None,
            });
            return;
        }
        // The data: the selection, or the block of cells around the active one.
        let selected = self.selection.primary_range();
        let range = if selected.start != selected.end {
            self.clamp_to_used_or_self(selected)
        } else {
            self.engine
                .current_region(self.current_sheet, self.selection.active)
                .unwrap_or(selected)
        };
        let taken: Vec<String> = self
            .engine
            .all_formatting()
            .values()
            .flat_map(|f| f.pivots.iter().map(|p| p.name.clone()))
            .collect();
        let name = (1..)
            .map(|n| format!("PivotTable{n}"))
            .find(|n| !taken.contains(n))
            .unwrap_or_default();
        self.pivot_dialog = Some(PivotDialog {
            existing: None,
            source: source_text(&current, range),
            new_sheet: true,
            destination: "A1".into(),
            table: PivotTable::new(name, current, range, CellCoord::new(0, 0)),
            headers: Vec::new(),
            items: Vec::new(),
            loaded: String::new(),
            picking: None,
            error: None,
        });
    }

    /// Lay out pivot table `index` on `sheet` and write its cells, replacing
    /// what it wrote before. Nothing changes if it fails.
    pub(super) fn write_pivot(&mut self, sheet: u32, index: usize) -> Result<(), String> {
        let table = self
            .engine
            .formatting(sheet)
            .and_then(|f| f.pivots.get(index))
            .cloned()
            .ok_or("That PivotTable is gone.")?;
        let src = self.engine.pivot_source(&table)?;
        let out = pivot::compute(&table, &src.headers, &src.records);
        let (h, w) = (out.cells.len() as u32, out.width() as u32);
        let a = table.anchor;
        if h == 0 || w == 0 {
            return Err("The PivotTable is empty.".into());
        }
        if a.row as u64 + h as u64 > crate::cell::MAX_ROW as u64 + 1
            || a.col as u64 + w as u64 > crate::cell::MAX_COL as u64 + 1
        {
            return Err("The PivotTable doesn't fit on the sheet there.".into());
        }
        let area = CellRange::new(a, CellCoord::new(a.row + h - 1, a.col + w - 1));
        let inside = |r: CellRange, c: CellCoord| {
            (r.start.row..=r.end.row).contains(&c.row) && (r.start.col..=r.end.col).contains(&c.col)
        };
        let ours = |c: CellCoord| table.output.is_some_and(|o| inside(o, c));
        if self
            .engine
            .iter_sheet_inputs(sheet)
            .any(|(c, _)| inside(area, c) && !ours(c))
        {
            return Err(
                "There's data where the PivotTable would go. Move it, or put the PivotTable somewhere else."
                    .into(),
            );
        }

        // Clear the old table's cells and formats.
        if let Some(old) = table.output {
            let cells: Vec<CellCoord> = self
                .engine
                .iter_sheet_inputs(sheet)
                .map(|(c, _)| c)
                .filter(|c| inside(old, *c))
                .collect();
            for c in cells {
                self.engine.clear(sheet, c);
            }
            let formatted: Vec<CellCoord> = self
                .engine
                .formatting(sheet)
                .map(|f| {
                    f.cells()
                        .map(|(c, _)| c)
                        .filter(|c| inside(old, *c))
                        .collect()
                })
                .unwrap_or_default();
            for c in formatted {
                self.engine.set_cell_format(sheet, c, CellFormat::default());
            }
        }

        // Write the new one.
        let mut widths = vec![0usize; w as usize];
        for (r, row) in out.cells.iter().enumerate() {
            let kind = out.kinds[r];
            for (c, cell) in row.iter().enumerate() {
                let coord = CellCoord::new(a.row + r as u32, a.col + c as u32);
                let number_format = out.value_columns[c]
                    .map(|v| table.values[v])
                    .filter(|v| v.aggregate != Aggregate::Count)
                    .and_then(|v| src.formats.get(v.field).cloned().flatten());
                let mut format = CellFormat::default();
                match kind {
                    RowKind::Header => {
                        format.bold = true;
                        format.fill = Some(HEADER_FILL);
                        format.borders.bottom =
                            r + 1 < out.kinds.len() && out.kinds[r + 1] != RowKind::Header;
                    }
                    RowKind::Filter => format.bold = c == 0,
                    RowKind::Subtotal => format.bold = true,
                    RowKind::GrandTotal => {
                        format.bold = true;
                        format.fill = Some(HEADER_FILL);
                        format.borders.top = true;
                    }
                    RowKind::Data => {}
                }
                let shown = match cell {
                    PivotCell::Empty => String::new(),
                    PivotCell::Text(t) => {
                        self.engine
                            .set_value(sheet, coord, CellValueInput::Text(t.clone()));
                        t.clone()
                    }
                    PivotCell::Number(n) => {
                        self.engine
                            .set_value(sheet, coord, CellValueInput::Number(*n));
                        format.number_format = number_format.clone();
                        match &number_format {
                            Some(code) => format_number(*n, code).text,
                            None => format_general(*n, 11),
                        }
                    }
                };
                widths[c] = widths[c].max(shown.chars().count());
                if !format.is_default() {
                    self.engine.set_cell_format(sheet, coord, format);
                }
            }
        }

        // Widen columns to fit, as Excel does on refresh.
        let f = self.engine.formatting_mut(sheet);
        for (c, chars) in widths.into_iter().enumerate() {
            let col = a.col + c as u32;
            let fit = (chars as f32 * 7.5 + 16.0).min(360.0);
            let current = f
                .column_widths
                .get(&col)
                .copied()
                .unwrap_or(DEFAULT_COLUMN_WIDTH);
            if fit > current {
                f.column_widths.insert(col, fit);
            }
        }
        if let Some(t) = f.pivots.get_mut(index) {
            t.output = Some(area);
        }
        Ok(())
    }

    /// Refresh the pivot table at the active cell.
    pub(super) fn refresh_pivot(&mut self) {
        match self.pivot_at_active() {
            Some(at) => self.refresh_pivots(&[at]),
            None => self.set_status("Select a cell in a PivotTable to refresh it"),
        }
    }

    pub(super) fn refresh_all_pivots(&mut self) {
        let all: Vec<(u32, usize)> = (0..self.sheet_names.len() as u32)
            .flat_map(|s| {
                let n = self.engine.formatting(s).map_or(0, |f| f.pivots.len());
                (0..n).map(move |i| (s, i))
            })
            .collect();
        if all.is_empty() {
            self.set_status("There are no PivotTables to refresh");
            return;
        }
        self.refresh_pivots(&all);
    }

    fn refresh_pivots(&mut self, list: &[(u32, usize)]) {
        let mut errors = Vec::new();
        self.with_snapshot(|app| {
            let mut any = false;
            for &(sheet, i) in list {
                match app.write_pivot(sheet, i) {
                    Ok(()) => any = true,
                    Err(e) => errors.push(e),
                }
            }
            any
        });
        match errors.first() {
            Some(e) => self.set_status(e),
            None if list.len() == 1 => self.set_status("Refreshed the PivotTable"),
            None => self.set_status(&format!("Refreshed {} PivotTables", list.len())),
        }
    }

    /// Remove a pivot table and its cells.
    pub(super) fn delete_pivot(&mut self, sheet: u32, index: usize) {
        self.with_snapshot(|app| {
            let Some(table) = app
                .engine
                .formatting(sheet)
                .and_then(|f| f.pivots.get(index))
                .cloned()
            else {
                return false;
            };
            if let Some(out) = table.output {
                let inside = |c: &CellCoord| {
                    (out.start.row..=out.end.row).contains(&c.row)
                        && (out.start.col..=out.end.col).contains(&c.col)
                };
                let cells: Vec<CellCoord> = app
                    .engine
                    .iter_sheet_inputs(sheet)
                    .map(|(c, _)| c)
                    .filter(inside)
                    .collect();
                for c in cells {
                    app.engine.clear(sheet, c);
                }
                let formatted: Vec<CellCoord> = app
                    .engine
                    .formatting(sheet)
                    .map(|f| f.cells().map(|(c, _)| c).filter(inside).collect())
                    .unwrap_or_default();
                for c in formatted {
                    app.engine.set_cell_format(sheet, c, CellFormat::default());
                }
            }
            app.engine.formatting_mut(sheet).pivots.remove(index);
            true
        });
        self.set_status("Deleted the PivotTable");
    }

    /// Create or update the table from the dialog.
    pub(super) fn apply_pivot_dialog(&mut self, d: &mut PivotDialog) -> Result<(), String> {
        let current = self.sheet_names[self.current_sheet as usize].clone();
        let (sheet_name, range) = parse_source(&d.source, &current)
            .ok_or("Enter the data as a range, like Sheet1!A1:D100.")?;
        if !self
            .sheet_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&sheet_name))
        {
            return Err(format!("There's no sheet called \"{sheet_name}\"."));
        }
        if d.table.rows.is_empty() && d.table.columns.is_empty() && d.table.values.is_empty() {
            return Err("Add fields to Rows, Columns or Values.".into());
        }
        let mut table = d.table.clone();
        table.source_sheet = sheet_name;
        table.source = range;
        let (sheet, index) = match d.existing {
            Some((sheet, i)) => {
                if let Some(t) = self.engine.formatting(sheet).and_then(|f| f.pivots.get(i)) {
                    table.output = t.output;
                    table.anchor = t.anchor;
                }
                (sheet, Some(i))
            }
            None if d.new_sheet => {
                self.add_sheet();
                let sheet = self.current_sheet;
                let mut n = 1;
                while self.sheet_names.iter().any(|s| *s == format!("Pivot{n}")) {
                    n += 1;
                }
                self.rename_sheet(sheet, format!("Pivot{n}"));
                table.anchor = CellCoord::new(0, 0);
                (sheet, None)
            }
            None => {
                let anchor =
                    CellCoord::from_a1(&d.destination.trim().replace('$', "").to_ascii_uppercase())
                        .ok_or("Enter where to put it as a cell, like H1.")?;
                table.anchor = anchor;
                (self.current_sheet, None)
            }
        };
        let mut result = Ok(());
        self.with_snapshot(|app| {
            let list = &mut app.engine.formatting_mut(sheet).pivots;
            let (i, old) = match index {
                Some(i) if i < list.len() => (i, Some(std::mem::replace(&mut list[i], table))),
                _ => {
                    list.push(table);
                    (list.len() - 1, None)
                }
            };
            result = app.write_pivot(sheet, i);
            if result.is_err() {
                let list = &mut app.engine.formatting_mut(sheet).pivots;
                match old {
                    Some(old) => list[i] = old,
                    None => {
                        list.remove(i);
                    }
                }
            }
            result.is_ok()
        });
        if result.is_ok() {
            self.set_status(
                "PivotTable ready. Data > Refresh All updates it after the data changes.",
            );
        }
        result
    }

    pub(super) fn show_pivot_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.pivot_dialog.take() else {
            return;
        };
        let current = self.sheet_names[self.current_sheet as usize].clone();
        d.load(&self.engine, &current);
        let mut keep = true;
        let mut apply = false;
        let mut delete = false;
        let title = if d.existing.is_some() {
            "PivotTable Fields"
        } else {
            "Create PivotTable"
        };
        egui::Window::new(title)
            .id(egui::Id::new("pivot_dialog"))
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("pivot_place")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("Data:");
                        ui.add(
                            egui::TextEdit::singleline(&mut d.source)
                                .desired_width(240.0)
                                .hint_text("Sheet1!A1:D100"),
                        );
                        ui.end_row();
                        if d.existing.is_none() {
                            ui.label("Put it:");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut d.new_sheet, true, "On a new sheet");
                                ui.radio_value(&mut d.new_sheet, false, "Here, at");
                                ui.add_enabled(
                                    !d.new_sheet,
                                    egui::TextEdit::singleline(&mut d.destination)
                                        .desired_width(60.0),
                                );
                            });
                            ui.end_row();
                        }
                    });
                ui.separator();
                if d.headers.is_empty() {
                    ui.label(RichText::new("The first row of the data names the fields.").weak());
                }
                ui.horizontal_top(|ui| {
                    // Fields, each with buttons to put it in an area.
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Fields").strong());
                        egui::ScrollArea::vertical()
                            .id_salt("pivot_fields")
                            .max_height(280.0)
                            .show(ui, |ui| {
                                egui::Grid::new("pivot_field_list")
                                    .num_columns(2)
                                    .show(ui, |ui| {
                                        for f in 0..d.headers.len() {
                                            ui.label(pivot::field_name(&d.headers, f));
                                            ui.horizontal(|ui| {
                                                for (area, label, tip) in [
                                                    (Area::Rows, "Row", "Add to Rows"),
                                                    (Area::Columns, "Col", "Add to Columns"),
                                                    (Area::Values, "Value", "Add to Values"),
                                                    (Area::Filters, "Filter", "Add to Filters"),
                                                ] {
                                                    if ui
                                                        .small_button(label)
                                                        .on_hover_text(tip)
                                                        .clicked()
                                                    {
                                                        d.add(f, area);
                                                    }
                                                }
                                            });
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
                    ui.add_space(16.0);
                    // The four areas.
                    ui.vertical(|ui| {
                        ui.set_min_width(260.0);
                        for (area, label) in AREAS {
                            ui.label(RichText::new(label).strong());
                            let len = if area == Area::Values {
                                d.table.values.len()
                            } else {
                                d.area(area).len()
                            };
                            if len == 0 {
                                ui.label(RichText::new("(none)").weak());
                            }
                            let mut action: Option<(usize, i32)> = None;
                            for i in 0..len {
                                ui.horizontal(|ui| {
                                    if area == Area::Values {
                                        let v = &mut d.table.values[i];
                                        egui::ComboBox::from_id_salt(("pivot_agg", i))
                                            .width(80.0)
                                            .selected_text(v.aggregate.label())
                                            .show_ui(ui, |ui| {
                                                for a in Aggregate::ALL {
                                                    ui.selectable_value(
                                                        &mut v.aggregate,
                                                        a,
                                                        a.label(),
                                                    );
                                                }
                                            });
                                        ui.label(format!(
                                            "of {}",
                                            pivot::field_name(&d.headers, v.field)
                                        ));
                                    } else {
                                        let f = d.area(area)[i];
                                        let hidden = d.table.hidden.get(&f).map_or(0, |h| h.len());
                                        let name = pivot::field_name(&d.headers, f);
                                        let label = if hidden > 0 {
                                            format!("{name} (filtered)")
                                        } else {
                                            name
                                        };
                                        if ui
                                            .selectable_label(d.picking == Some(f), label)
                                            .on_hover_text("Choose which items to show")
                                            .clicked()
                                        {
                                            d.picking =
                                                if d.picking == Some(f) { None } else { Some(f) };
                                        }
                                    }
                                    if ui.small_button("Up").clicked() && i > 0 {
                                        action = Some((i, -1));
                                    }
                                    if ui.small_button("Down").clicked() && i + 1 < len {
                                        action = Some((i, 1));
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        action = Some((i, 0));
                                    }
                                });
                            }
                            if let Some((i, step)) = action {
                                let swap = |list: &mut dyn FnMut(usize, usize)| {
                                    if step != 0 {
                                        list(i, (i as i32 + step) as usize);
                                    }
                                };
                                if area == Area::Values {
                                    if step == 0 {
                                        d.table.values.remove(i);
                                    } else {
                                        swap(&mut |a, b| d.table.values.swap(a, b));
                                    }
                                } else if step == 0 {
                                    let f = d.area(area).remove(i);
                                    if d.picking == Some(f) {
                                        d.picking = None;
                                    }
                                } else {
                                    swap(&mut |a, b| d.area(area).swap(a, b));
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
                // Items of the chosen field: unchecked ones are left out.
                if let Some(f) = d.picking.filter(|&f| f < d.items.len()) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Show items of {}",
                                pivot::field_name(&d.headers, f)
                            ))
                            .strong(),
                        );
                        if ui.small_button("All").clicked() {
                            d.table.hidden.remove(&f);
                        }
                        if ui.small_button("None").clicked() {
                            d.table
                                .hidden
                                .insert(f, d.items[f].iter().cloned().collect());
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("pivot_items")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for item in d.items[f].clone() {
                                let hidden =
                                    d.table.hidden.get(&f).is_some_and(|h| h.contains(&item));
                                let mut shown = !hidden;
                                let label = if item.is_empty() { pivot::BLANK } else { &item };
                                if ui.checkbox(&mut shown, label).changed() {
                                    let set = d.table.hidden.entry(f).or_default();
                                    if shown {
                                        set.remove(&item);
                                    } else {
                                        set.insert(item.clone());
                                    }
                                    if set.is_empty() {
                                        d.table.hidden.remove(&f);
                                    }
                                }
                            }
                        });
                }
                if let Some(e) = &d.error {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    if d.existing.is_some() && ui.button("Delete PivotTable").clicked() {
                        delete = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if apply {
            match self.apply_pivot_dialog(&mut d) {
                Ok(()) => keep = false,
                Err(e) => d.error = Some(e),
            }
        }
        if delete {
            if let Some((sheet, i)) = d.existing {
                self.delete_pivot(sheet, i);
            }
            keep = false;
        }
        if keep {
            self.pivot_dialog = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ranges() {
        let r = |a1: &str| CellRange::from_a1(a1).unwrap();
        assert_eq!(
            parse_source("Data!$A$1:$D$9", "S"),
            Some(("Data".into(), r("A1:D9")))
        );
        assert_eq!(
            parse_source("'My ''data'''!a1:b2", "S"),
            Some(("My 'data'".into(), r("A1:B2")))
        );
        assert_eq!(
            parse_source("A1:B2", "Sheet1"),
            Some(("Sheet1".into(), r("A1:B2")))
        );
        let whole = parse_source("Data!A:C", "S").unwrap().1;
        assert_eq!((whole.start, whole.end.col), (CellCoord::new(0, 0), 2));
        assert_eq!(parse_source("nonsense", "S"), None);
        assert_eq!(source_text("My data", r("A1:B2")), "'My data'!A1:B2");
        assert_eq!(source_text("Data", r("A1:B2")), "Data!A1:B2");
    }
}
