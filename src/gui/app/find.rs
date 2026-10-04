//! Find and Replace (Ctrl+F / Ctrl+H). Like Excel: searches by rows, looks in
//! formulas or in displayed values, supports `*` and `?` wildcards, and can
//! match whole cells only or case.

use super::*;

pub(super) struct FindDialog {
    pub query: String,
    pub replacement: String,
    pub replace_mode: bool,
    pub match_case: bool,
    pub whole_cell: bool,
    /// Search what was typed (formulas) rather than what is shown
    pub in_formulas: bool,
    pub whole_workbook: bool,
    pub message: Option<String>,
    pub focus: bool,
}

impl FindDialog {
    fn new(replace_mode: bool) -> Self {
        Self {
            query: String::new(),
            replacement: String::new(),
            replace_mode,
            match_case: false,
            whole_cell: false,
            in_formulas: true,
            whole_workbook: false,
            message: None,
            focus: true,
        }
    }
}

/// What a search compares and how.
#[derive(Clone)]
pub(super) struct Query {
    pattern: Vec<char>,
    match_case: bool,
    whole_cell: bool,
}

impl Query {
    pub(super) fn new(text: &str, match_case: bool, whole_cell: bool) -> Self {
        let text = if match_case {
            text.to_string()
        } else {
            text.to_lowercase()
        };
        Self {
            pattern: text.chars().collect(),
            match_case,
            whole_cell,
        }
    }

    fn norm(&self, s: &str) -> Vec<char> {
        if self.match_case {
            s.chars().collect()
        } else {
            s.to_lowercase().chars().collect()
        }
    }

    pub(super) fn matches(&self, text: &str) -> bool {
        let t = self.norm(text);
        if self.whole_cell {
            match_here(&t, 0, &self.pattern) == Some(t.len())
                || (self.pattern.is_empty() && t.is_empty())
        } else {
            (0..=t.len()).any(|i| match_here(&t, i, &self.pattern).is_some())
        }
    }

    /// `text` with every match replaced by `with`.
    pub(super) fn replace_all(&self, text: &str, with: &str) -> String {
        if self.whole_cell {
            return with.to_string();
        }
        let original: Vec<char> = text.chars().collect();
        let t = self.norm(text);
        // Lowercasing can change length for a few scripts; fall back safely.
        if t.len() != original.len() || self.pattern.is_empty() {
            return text.to_string();
        }
        let mut out = String::new();
        let mut i = 0;
        while i < t.len() {
            match match_here(&t, i, &self.pattern).filter(|&end| end > i) {
                Some(end) => {
                    out.push_str(with);
                    i = end;
                }
                None => {
                    out.push(original[i]);
                    i += 1;
                }
            }
        }
        out
    }
}

/// Match `pattern` (with `*`, `?` and `~` escapes) at `text[i..]`; returns
/// where the shortest match ends.
fn match_here(text: &[char], i: usize, pattern: &[char]) -> Option<usize> {
    match pattern.first() {
        None => Some(i),
        Some('*') => (i..=text.len()).find_map(|j| match_here(text, j, &pattern[1..])),
        Some('?') => (i < text.len())
            .then(|| match_here(text, i + 1, &pattern[1..]))
            .flatten(),
        Some('~') if pattern.len() > 1 => (text.get(i) == Some(&pattern[1]))
            .then(|| match_here(text, i + 1, &pattern[2..]))
            .flatten(),
        Some(c) => (text.get(i) == Some(c))
            .then(|| match_here(text, i + 1, &pattern[1..]))
            .flatten(),
    }
}

impl SpreadsheetApp {
    pub(super) fn open_find(&mut self, replace: bool) {
        match &mut self.find_dialog {
            Some(d) => {
                d.replace_mode = replace;
                d.focus = true;
            }
            None => self.find_dialog = Some(FindDialog::new(replace)),
        }
    }

    /// The text a search looks at in one cell.
    fn searchable(&self, sheet: u32, coord: CellCoord, in_formulas: bool) -> Option<String> {
        if in_formulas {
            self.cell_content_string(sheet, coord)
        } else {
            let value = self.engine.get_value(sheet, coord);
            let format = self
                .engine
                .formatting(sheet)
                .and_then(|f| f.effective(coord));
            Some(display_text(&value, format)).filter(|s| !s.is_empty())
        }
    }

    /// Every matching cell, in search order (sheet, then row, then column).
    pub(super) fn find_all(
        &self,
        query: &Query,
        in_formulas: bool,
        whole_workbook: bool,
    ) -> Vec<(u32, CellCoord)> {
        let sheets: Vec<u32> = if whole_workbook {
            (0..self.sheet_names.len() as u32).collect()
        } else {
            vec![self.current_sheet]
        };
        let mut hits = Vec::new();
        for sheet in sheets {
            let mut cells: Vec<CellCoord> = self
                .engine
                .iter_sheet_inputs(sheet)
                .map(|(c, _)| c)
                .collect();
            cells.sort_by_key(|c| (c.row, c.col));
            for coord in cells {
                if self
                    .searchable(sheet, coord, in_formulas)
                    .is_some_and(|text| query.matches(&text))
                {
                    hits.push((sheet, coord));
                }
            }
        }
        hits
    }

    /// Go to the next match after the active cell (or before, backward).
    fn find_next(&mut self, forward: bool) -> Option<(u32, CellCoord)> {
        let d = self.find_dialog.as_ref()?;
        if d.query.is_empty() {
            return None;
        }
        let query = Query::new(&d.query, d.match_case, d.whole_cell);
        let hits = self.find_all(&query, d.in_formulas, d.whole_workbook);
        let here = (
            self.current_sheet,
            self.selection.active.row,
            self.selection.active.col,
        );
        let key = |h: &(u32, CellCoord)| (h.0, h.1.row, h.1.col);
        let next = if forward {
            hits.iter().find(|h| key(h) > here).or_else(|| hits.first())
        } else {
            hits.iter()
                .rev()
                .find(|h| key(h) < here)
                .or_else(|| hits.last())
        }
        .copied();
        match next {
            Some((sheet, coord)) => {
                if sheet != self.current_sheet {
                    self.switch_sheet(sheet);
                }
                self.selection.move_to(coord);
                self.scroll
                    .scroll_to_cell(coord, &self.grid_config, self.last_viewport);
                let message = format!(
                    "{} of {} matches",
                    hits.iter()
                        .position(|h| *h == (sheet, coord))
                        .map_or(0, |i| i + 1),
                    hits.len()
                );
                if let Some(d) = &mut self.find_dialog {
                    d.message = Some(message);
                }
            }
            None => {
                if let Some(d) = &mut self.find_dialog {
                    d.message = Some("No matches".into());
                }
            }
        }
        next
    }

    /// Replace in the active cell if it matches, then go to the next match.
    fn replace_current(&mut self) {
        let Some(d) = self.find_dialog.as_ref() else {
            return;
        };
        let query = Query::new(&d.query, d.match_case, d.whole_cell);
        let replacement = d.replacement.clone();
        let coord = self.selection.active;
        let sheet = self.current_sheet;
        if let Some(text) = self
            .cell_content_string(sheet, coord)
            .filter(|t| query.matches(t))
        {
            let new = query.replace_all(&text, &replacement);
            self.set_cell_content(coord, &new);
        }
        self.find_next(true);
    }

    pub(super) fn replace_everywhere(&mut self) -> usize {
        let Some(d) = self.find_dialog.as_ref() else {
            return 0;
        };
        let query = Query::new(&d.query, d.match_case, d.whole_cell);
        let replacement = d.replacement.clone();
        // Replace works on what was typed, as in Excel.
        let hits = self.find_all(&query, true, d.whole_workbook);
        if hits.is_empty() {
            return 0;
        }
        let current = self.current_sheet;
        let actions = self.batch(|app| {
            let mut actions = Vec::new();
            for &(sheet, coord) in &hits {
                let Some(text) = app.cell_content_string(sheet, coord) else {
                    continue;
                };
                let new = query.replace_all(&text, &replacement);
                if new != text {
                    // set_cell_content_action works on the current sheet.
                    app.current_sheet = sheet;
                    actions.push(app.set_cell_content_action(coord, &new));
                }
            }
            app.current_sheet = current;
            actions
        });
        let n = actions.len();
        if n > 0 {
            self.undo_history.push(UndoAction::Group(actions));
            self.modified = true;
        }
        n
    }

    pub(super) fn show_find_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.find_dialog.take() else {
            return;
        };
        let mut open = true;
        let mut action: Option<&'static str> = None;
        let title = if d.replace_mode {
            "Find and Replace"
        } else {
            "Find"
        };
        egui::Window::new(title)
            .order(egui::Order::Foreground)
            .id(egui::Id::new("find_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-24.0, 120.0))
            .show(ctx, |ui| {
                egui::Grid::new("find_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Find:");
                    let r = ui.add(egui::TextEdit::singleline(&mut d.query).desired_width(220.0));
                    if d.focus {
                        r.request_focus();
                        d.focus = false;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        action = Some(if ui.input(|i| i.modifiers.shift) {
                            "prev"
                        } else {
                            "next"
                        });
                    }
                    if r.changed() {
                        d.message = None;
                    }
                    ui.end_row();
                    if d.replace_mode {
                        ui.label("Replace:");
                        ui.add(egui::TextEdit::singleline(&mut d.replacement).desired_width(220.0));
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut d.match_case, "Match case");
                    ui.checkbox(&mut d.whole_cell, "Entire cell");
                });
                ui.horizontal(|ui| {
                    ui.label("Look in:");
                    ui.selectable_value(&mut d.in_formulas, true, "Formulas");
                    ui.selectable_value(&mut d.in_formulas, false, "Values");
                    ui.separator();
                    ui.selectable_value(&mut d.whole_workbook, false, "Sheet");
                    ui.selectable_value(&mut d.whole_workbook, true, "Workbook");
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Find Previous").clicked() {
                        action = Some("prev");
                    }
                    if ui.button("Find Next").clicked() {
                        action = Some("next");
                    }
                    if d.replace_mode {
                        if ui.button("Replace").clicked() {
                            action = Some("replace");
                        }
                        if ui.button("Replace All").clicked() {
                            action = Some("all");
                        }
                    } else if ui.small_button("Replace...").clicked() {
                        d.replace_mode = true;
                    }
                });
                if let Some(m) = &d.message {
                    ui.label(m);
                }
                ui.label(
                    RichText::new("Use * for any text and ? for one character.")
                        .small()
                        .weak(),
                );
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        self.find_dialog = open.then_some(d);
        match action {
            Some("next") => {
                self.find_next(true);
            }
            Some("prev") => {
                self.find_next(false);
            }
            Some("replace") => self.replace_current(),
            Some("all") => {
                let n = self.replace_everywhere();
                if let Some(d) = &mut self.find_dialog {
                    d.message = Some(match n {
                        0 => "No matches".into(),
                        1 => "Replaced 1 cell".into(),
                        n => format!("Replaced {n} cells"),
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Query;

    #[test]
    fn plain_and_wildcard_matching() {
        let q = Query::new("tea", false, false);
        assert!(q.matches("Green Tea"));
        assert!(!q.matches("Coffee"));
        assert!(Query::new("t?a", false, false).matches("TOAST TEA"));
        assert!(Query::new("a*e", false, true).matches("Apple"));
        assert!(!Query::new("a*e", false, true).matches("Apples"));
        assert!(Query::new("~*", false, false).matches("5*2"));
        assert!(!Query::new("~*", false, false).matches("52"));
        assert!(!Query::new("Tea", true, false).matches("tea"));
    }

    #[test]
    fn replacing_keeps_the_rest_of_the_text() {
        let q = Query::new("cat", false, false);
        assert_eq!(q.replace_all("Cat and cat", "dog"), "dog and dog");
        assert_eq!(Query::new("x", false, true).replace_all("x", "y"), "y");
        assert_eq!(
            Query::new("1", false, false).replace_all("=A1+B1", "2"),
            "=A2+B2"
        );
    }
}
