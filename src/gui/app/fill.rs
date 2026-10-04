//! Fill: Ctrl+D / Ctrl+R and dragging the fill handle. Series continue the
//! way Excel's do: numbers by their trend, a single date by one day, text
//! ending in a number counts up, month and day names continue, formulas
//! move like copies, anything else repeats.

use super::*;
use crate::calc::CellInput;
use crate::cell::Axis;

const LISTS: [&[&str]; 4] = [
    &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    &[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    &[
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
];

/// One source cell along the fill direction.
struct Source {
    coord: CellCoord,
    input: Option<CellInput>,
    format: Option<CellFormat>,
}

/// How a run of source cells continues.
enum Series {
    /// value = a + b * t, t = position from the first source cell
    Linear { a: f64, b: f64 },
    /// prefix + (n + t), keeping the number's width ("Item 1" -> "Item 2")
    Counting {
        prefix: String,
        start: i64,
        width: usize,
    },
    /// A named list (months, days): index of the first cell and the step
    List {
        list: &'static [&'static str],
        start: usize,
        step: i64,
        upper: bool,
    },
    /// Repeat the source cells
    Copy,
}

impl SpreadsheetApp {
    /// Where the fill handle would fill to if released at `target`:
    /// (cells to fill, direction axis, forward).
    pub(super) fn fill_plan(&self, target: CellCoord) -> Option<(CellRange, Axis, bool)> {
        let s = self.clamp_to_used_or_self(self.selection.primary_range());
        if target.row > s.end.row {
            let r = CellRange::new(
                CellCoord::new(s.end.row + 1, s.start.col),
                CellCoord::new(target.row, s.end.col),
            );
            Some((r, Axis::Row, true))
        } else if target.row < s.start.row {
            let r = CellRange::new(
                CellCoord::new(target.row, s.start.col),
                CellCoord::new(s.start.row - 1, s.end.col),
            );
            Some((r, Axis::Row, false))
        } else if target.col > s.end.col {
            let r = CellRange::new(
                CellCoord::new(s.start.row, s.end.col + 1),
                CellCoord::new(s.end.row, target.col),
            );
            Some((r, Axis::Column, true))
        } else if target.col < s.start.col {
            let r = CellRange::new(
                CellCoord::new(s.start.row, target.col),
                CellCoord::new(s.end.row, s.start.col - 1),
            );
            Some((r, Axis::Column, false))
        } else {
            None
        }
    }

    /// Release of the fill handle over `target`.
    pub(super) fn fill_to(&mut self, target: CellCoord) {
        let source = self.clamp_to_used_or_self(self.selection.primary_range());
        let Some((dest, axis, forward)) = self.fill_plan(target) else {
            return;
        };
        self.fill(source, dest, axis, forward);
        let all = CellRange::new(
            CellCoord::new(
                source.start.row.min(dest.start.row),
                source.start.col.min(dest.start.col),
            ),
            CellCoord::new(
                source.end.row.max(dest.end.row),
                source.end.col.max(dest.end.col),
            ),
        );
        self.selection.move_to(all.start);
        self.selection.extend_to(all.end);
    }

    /// Ctrl+D: copy the top row of the selection down through it, or the
    /// row above into a one-row selection.
    pub(super) fn fill_down(&mut self) {
        self.fill_copy(Axis::Row);
    }

    /// Ctrl+R: like Ctrl+D, rightward.
    pub(super) fn fill_right(&mut self) {
        self.fill_copy(Axis::Column);
    }

    fn fill_copy(&mut self, axis: Axis) {
        let r = self.clamp_to_used_or_self(self.selection.primary_range());
        let (first, last) = match axis {
            Axis::Row => (r.start.row, r.end.row),
            Axis::Column => (r.start.col, r.end.col),
        };
        let (source, dest) = if first == last {
            // One line selected: copy from the line before it.
            let Some(before) = first.checked_sub(1) else {
                return;
            };
            match axis {
                Axis::Row => (
                    CellRange::new(
                        CellCoord::new(before, r.start.col),
                        CellCoord::new(before, r.end.col),
                    ),
                    r,
                ),
                Axis::Column => (
                    CellRange::new(
                        CellCoord::new(r.start.row, before),
                        CellCoord::new(r.end.row, before),
                    ),
                    r,
                ),
            }
        } else {
            match axis {
                Axis::Row => (
                    CellRange::new(r.start, CellCoord::new(r.start.row, r.end.col)),
                    CellRange::new(CellCoord::new(r.start.row + 1, r.start.col), r.end),
                ),
                Axis::Column => (
                    CellRange::new(r.start, CellCoord::new(r.end.row, r.start.col)),
                    CellRange::new(CellCoord::new(r.start.row, r.start.col + 1), r.end),
                ),
            }
        };
        // Ctrl+D copies; it never extends a series.
        self.fill_with(source, dest, axis, true, false);
    }

    fn fill(&mut self, source: CellRange, dest: CellRange, axis: Axis, forward: bool) {
        self.fill_with(source, dest, axis, forward, true);
    }

    fn fill_with(
        &mut self,
        source: CellRange,
        dest: CellRange,
        axis: Axis,
        forward: bool,
        series: bool,
    ) {
        let sheet = self.current_sheet;
        // Lanes run along the fill direction: columns when filling down.
        let lanes: Vec<u32> = match axis {
            Axis::Row => (source.start.col..=source.end.col).collect(),
            Axis::Column => (source.start.row..=source.end.row).collect(),
        };
        let parser = FormulaParser::new();
        self.with_snapshot(|app| {
            for lane in lanes {
                let at = |i: u32| match axis {
                    Axis::Row => CellCoord::new(i, lane),
                    Axis::Column => CellCoord::new(lane, i),
                };
                let (s0, s1, d0, d1) = match axis {
                    Axis::Row => (
                        source.start.row,
                        source.end.row,
                        dest.start.row,
                        dest.end.row,
                    ),
                    Axis::Column => (
                        source.start.col,
                        source.end.col,
                        dest.start.col,
                        dest.end.col,
                    ),
                };
                let sources: Vec<Source> = (s0..=s1)
                    .map(|i| {
                        let coord = at(i);
                        Source {
                            coord,
                            input: app.engine.get_input(sheet, coord).cloned(),
                            format: app.engine.cell_format(sheet, coord).cloned(),
                        }
                    })
                    .collect();
                let n = sources.len() as i64;
                let kind = if series {
                    app.series_of(&sources)
                } else {
                    Series::Copy
                };
                for i in d0..=d1 {
                    // t: position relative to the first source cell
                    let t = i as i64 - s0 as i64;
                    let k = t.rem_euclid(n) as usize;
                    let src = &sources[k];
                    let dest_coord = at(i);
                    app.engine.set_cell_format(
                        sheet,
                        dest_coord,
                        src.format.clone().unwrap_or_default(),
                    );
                    let content = match (&kind, &src.input) {
                        (Series::Linear { a, b }, _) => Some(CellInput::Value(
                            crate::calc::CellValueInput::Number(a + b * t as f64),
                        )),
                        (
                            Series::Counting {
                                prefix,
                                start,
                                width,
                            },
                            _,
                        ) => {
                            let v = start + t;
                            let digits = format!("{:0width$}", v.max(0), width = *width);
                            Some(CellInput::Value(crate::calc::CellValueInput::Text(
                                format!("{prefix}{digits}"),
                            )))
                        }
                        (
                            Series::List {
                                list,
                                start,
                                step,
                                upper,
                            },
                            _,
                        ) => {
                            let idx =
                                (*start as i64 + step * t).rem_euclid(list.len() as i64) as usize;
                            let word = list[idx];
                            let word = if *upper {
                                word.to_uppercase()
                            } else {
                                word.to_string()
                            };
                            Some(CellInput::Value(crate::calc::CellValueInput::Text(word)))
                        }
                        (Series::Copy, Some(CellInput::Formula(f))) => {
                            let (dr, dc) = match axis {
                                Axis::Row => (i as i64 - src.coord.row as i64, 0),
                                Axis::Column => (0, i as i64 - src.coord.col as i64),
                            };
                            let moved = match parser.parse(f) {
                                Ok(mut expr) => {
                                    expr.offset_references(dr, dc);
                                    format!("={expr}")
                                }
                                Err(_) => f.clone(),
                            };
                            Some(CellInput::Formula(moved))
                        }
                        (Series::Copy, other) => other.clone(),
                    };
                    match content {
                        Some(CellInput::Value(v)) => app.engine.set_value(sheet, dest_coord, v),
                        Some(CellInput::Formula(f)) => {
                            let _ = app.engine.set_formula(sheet, dest_coord, &f);
                        }
                        Some(CellInput::Empty) | None => app.engine.clear(sheet, dest_coord),
                    }
                }
                let _ = forward;
            }
            true
        });
    }

    fn series_of(&self, sources: &[Source]) -> Series {
        use crate::calc::CellValueInput;
        let numbers: Option<Vec<f64>> = sources
            .iter()
            .map(|s| match &s.input {
                Some(CellInput::Value(CellValueInput::Number(n))) => Some(*n),
                _ => None,
            })
            .collect();
        if let Some(values) = numbers {
            let is_date = sources.iter().all(|s| {
                let format = self
                    .engine
                    .formatting(self.current_sheet)
                    .and_then(|f| f.effective(s.coord));
                format
                    .and_then(|f| f.number_format.as_deref())
                    .is_some_and(crate::format::is_date_format)
            });
            return match values.len() {
                // A single number repeats; a single date counts days.
                1 if is_date => Series::Linear {
                    a: values[0],
                    b: 1.0,
                },
                1 => Series::Copy,
                _ => {
                    let (a, b) = linear_fit(&values);
                    Series::Linear { a, b }
                }
            };
        }

        let texts: Option<Vec<&str>> = sources
            .iter()
            .map(|s| match &s.input {
                Some(CellInput::Value(CellValueInput::Text(t))) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        let Some(texts) = texts else {
            return Series::Copy;
        };

        // Month or day names, consecutive.
        for list in LISTS {
            let indices: Option<Vec<usize>> = texts
                .iter()
                .map(|t| list.iter().position(|w| w.eq_ignore_ascii_case(t)))
                .collect();
            if let Some(idx) = indices {
                let step = if idx.len() >= 2 {
                    let len = list.len() as i64;
                    let d = (idx[1] as i64 - idx[0] as i64).rem_euclid(len);
                    let steady = idx
                        .windows(2)
                        .all(|w| (w[1] as i64 - w[0] as i64).rem_euclid(len) == d);
                    if !steady {
                        return Series::Copy;
                    }
                    d
                } else {
                    1
                };
                let upper = texts[0].chars().all(|c| !c.is_lowercase());
                return Series::List {
                    list,
                    start: idx[0],
                    step,
                    upper,
                };
            }
        }

        // "Item 1", "Q3": a single cell ending in a number counts up.
        if texts.len() == 1 {
            let t = texts[0];
            let digits = t.chars().rev().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && digits < 18 {
                let (prefix, number) = t.split_at(t.len() - digits);
                if let Ok(start) = number.parse::<i64>() {
                    return Series::Counting {
                        prefix: prefix.to_string(),
                        start,
                        width: if number.starts_with('0') { digits } else { 1 },
                    };
                }
            }
        }
        Series::Copy
    }
}

/// Least-squares line through (0, v0), (1, v1), ...: exact for steady steps.
fn linear_fit(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean_t = (n - 1.0) / 2.0;
    let mean_v = values.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (t, v) in values.iter().enumerate() {
        let dt = t as f64 - mean_t;
        num += dt * (v - mean_v);
        den += dt * dt;
    }
    let b = if den == 0.0 { 0.0 } else { num / den };
    (mean_v - b * mean_t, b)
}
