//! Evaluating conditional formatting. Rules that compare a cell with the
//! rest of its range (top N, averages, duplicates, color scales, data bars)
//! use statistics over the range, cached per rule until the workbook
//! changes (see `CalcEngine::revision`).

use super::engine::{CalcEngine, CellResult};
use crate::cell::CellCoord;
use crate::format::conditional::{
    AverageRule, CfRule, CfStyle, Cfvo, CfvoKind, ConditionalFormat, TextRule, mix,
};
use crate::format::validation::CompareOp;
use crate::format::{CellFormat, Rgb, display_text};
use std::collections::HashMap;

/// What conditional formatting does to one cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CfLook {
    pub style: CfStyle,
    /// Background from a color scale
    pub scale_fill: Option<Rgb>,
    /// Data bar: fraction of the cell's width, and color
    pub bar: Option<(f64, Rgb)>,
}

impl CfLook {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `format` as this look shows it. A highlight's fill wins over a
    /// color scale's.
    pub fn apply(&self, format: &CellFormat) -> CellFormat {
        let s = &self.style;
        let mut f = format.clone();
        f.bold = s.bold.unwrap_or(f.bold);
        f.italic = s.italic.unwrap_or(f.italic);
        f.underline = s.underline.unwrap_or(f.underline);
        f.strikethrough = s.strikethrough.unwrap_or(f.strikethrough);
        f.font_color = s.font_color.or(f.font_color);
        f.fill = s.fill.or(self.scale_fill).or(f.fill);
        if let Some(code) = &s.number_format {
            f.number_format = Some(code.clone());
        }
        f
    }
}

/// Statistics over a rule's ranges (non-empty cells only, as in Excel).
#[derive(Clone, Debug, Default)]
pub(crate) struct RuleStats {
    /// Numbers, ascending
    numbers: Vec<f64>,
    mean: f64,
    /// Lowercased shown text -> how many cells show it
    counts: HashMap<String, usize>,
}

/// Each rule's statistics by (sheet, rule index), with the engine revision
/// they were computed at.
pub(crate) type StatsCache = std::cell::RefCell<HashMap<(u32, usize), (u64, RuleStats)>>;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = (p / 100.0).clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

impl CalcEngine {
    fn rule_stats(&self, sheet: u32, index: usize, cf: &ConditionalFormat) -> RuleStats {
        let key = (sheet, index);
        if let Some((rev, stats)) = self.cf_cache().borrow().get(&key) {
            if *rev == self.revision() {
                return stats.clone();
            }
        }
        let formatting = self.formatting(sheet);
        let mut numbers = Vec::new();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for (coord, _) in self.iter_sheet_inputs(sheet) {
            if !cf.covers(coord) {
                continue;
            }
            let value = self.get_value(sheet, coord);
            if let CellResult::Value(n) = value {
                numbers.push(n);
            }
            let shown = display_text(&value, formatting.and_then(|f| f.effective(coord)));
            if !shown.is_empty() {
                *counts.entry(shown.to_lowercase()).or_default() += 1;
            }
        }
        numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean = if numbers.is_empty() {
            0.0
        } else {
            numbers.iter().sum::<f64>() / numbers.len() as f64
        };
        let stats = RuleStats {
            numbers,
            mean,
            counts,
        };
        self.cf_cache()
            .borrow_mut()
            .insert(key, (self.revision(), stats.clone()));
        stats
    }

    fn cfvo_value(
        &self,
        sheet: u32,
        cf: &ConditionalFormat,
        v: &Cfvo,
        stats: &RuleStats,
    ) -> Option<f64> {
        let (lo, hi) = (*stats.numbers.first()?, *stats.numbers.last()?);
        let num = || v.value.trim().parse::<f64>().ok();
        Some(match v.kind {
            CfvoKind::Min => lo,
            CfvoKind::Max => hi,
            CfvoKind::Number => num()?,
            CfvoKind::Percent => lo + (hi - lo) * num()? / 100.0,
            CfvoKind::Percentile => percentile(&stats.numbers, num()?),
            CfvoKind::Formula => {
                match self.operand_value(sheet, cf.origin(), &v.value, cf.origin()) {
                    CellResult::Value(n) => n,
                    _ => return None,
                }
            }
        })
    }

    /// The combined conditional formatting for a cell, or `None` if no rule
    /// covers it or none matches.
    pub fn conditional_look(&self, sheet: u32, coord: CellCoord) -> Option<CfLook> {
        let rules = &self.formatting(sheet)?.conditional;
        if rules.is_empty() {
            return None;
        }
        let value = self.get_value(sheet, coord);
        let shown = || {
            display_text(
                &value,
                self.formatting(sheet).and_then(|f| f.effective(coord)),
            )
        };
        let number = match value {
            CellResult::Value(n) => Some(n),
            _ => None,
        };
        let mut look = CfLook::default();
        for (index, cf) in rules.iter().enumerate() {
            if !cf.covers(coord) {
                continue;
            }
            let matched = match &cf.rule {
                CfRule::CellIs {
                    op,
                    formula1,
                    formula2,
                    ..
                } => {
                    let a = self.operand_value(sheet, cf.origin(), formula1, coord);
                    let b = formula2
                        .as_deref()
                        .map(|f| self.operand_value(sheet, cf.origin(), f, coord));
                    match (number, &a, &b) {
                        (Some(v), CellResult::Value(a), Some(CellResult::Value(b))) => {
                            op.test(v, *a, *b)
                        }
                        (Some(v), CellResult::Value(a), None) if !op.needs_second() => {
                            op.test(v, *a, *a)
                        }
                        // Text compared with text: equal / not equal only.
                        (None, CellResult::Text(a), _) if !matches!(value, CellResult::Empty) => {
                            match op {
                                CompareOp::Equal => shown().eq_ignore_ascii_case(a),
                                CompareOp::NotEqual => !shown().eq_ignore_ascii_case(a),
                                _ => false,
                            }
                        }
                        _ => false,
                    }
                }
                CfRule::Text { rule, text, .. } => {
                    let s = shown().to_lowercase();
                    let t = text.to_lowercase();
                    match rule {
                        TextRule::Contains => s.contains(&t),
                        TextRule::NotContains => !s.contains(&t),
                        TextRule::BeginsWith => s.starts_with(&t),
                        TextRule::EndsWith => s.ends_with(&t),
                    }
                }
                CfRule::Top {
                    bottom,
                    rank,
                    percent,
                    ..
                } => match number {
                    Some(v) => {
                        let stats = self.rule_stats(sheet, index, cf);
                        let n = stats.numbers.len();
                        let k = if *percent {
                            ((n as f64 * *rank as f64 / 100.0).floor() as usize).max(1)
                        } else {
                            *rank as usize
                        }
                        .clamp(1, n.max(1));
                        if n == 0 {
                            false
                        } else if *bottom {
                            v <= stats.numbers[k - 1]
                        } else {
                            v >= stats.numbers[n - k]
                        }
                    }
                    None => false,
                },
                CfRule::Average { rule, .. } => match number {
                    Some(v) => {
                        let m = self.rule_stats(sheet, index, cf).mean;
                        match rule {
                            AverageRule::Above => v > m,
                            AverageRule::Below => v < m,
                            AverageRule::EqualOrAbove => v >= m,
                            AverageRule::EqualOrBelow => v <= m,
                        }
                    }
                    None => false,
                },
                CfRule::Duplicate { unique, .. } => {
                    let s = shown().to_lowercase();
                    !s.is_empty() && {
                        let count = self
                            .rule_stats(sheet, index, cf)
                            .counts
                            .get(&s)
                            .copied()
                            .unwrap_or(0);
                        if *unique { count == 1 } else { count > 1 }
                    }
                }
                CfRule::Blanks { not, .. } => {
                    let blank = matches!(value, CellResult::Empty)
                        || matches!(&value, CellResult::Text(t) if t.trim().is_empty());
                    blank != *not
                }
                CfRule::Errors { not, .. } => matches!(value, CellResult::Error(_)) != *not,
                CfRule::Expression { formula, .. } => {
                    match self
                        .relative_formula(cf.origin(), formula, coord)
                        .map(|e| self.evaluate_expr(sheet, &e))
                    {
                        Some(CellResult::Bool(b)) => b,
                        Some(CellResult::Value(n)) => n != 0.0,
                        _ => false,
                    }
                }
                CfRule::ColorScale { stops } => {
                    if let (Some(v), None) = (number, look.scale_fill) {
                        let stats = self.rule_stats(sheet, index, cf);
                        let points: Vec<(f64, Rgb)> = stops
                            .iter()
                            .filter_map(|(cfvo, color)| {
                                Some((self.cfvo_value(sheet, cf, cfvo, &stats)?, *color))
                            })
                            .collect();
                        if points.len() >= 2 {
                            let color = if v <= points[0].0 {
                                points[0].1
                            } else if v >= points[points.len() - 1].0 {
                                points[points.len() - 1].1
                            } else {
                                let i = points
                                    .windows(2)
                                    .position(|w| v >= w[0].0 && v <= w[1].0)
                                    .unwrap_or(0);
                                let (a, b) = (points[i], points[i + 1]);
                                let t = if b.0 > a.0 {
                                    (v - a.0) / (b.0 - a.0)
                                } else {
                                    0.0
                                };
                                mix(a.1, b.1, t)
                            };
                            look.scale_fill = Some(color);
                        }
                    }
                    number.is_some()
                }
                CfRule::DataBar { min, max, color } => {
                    if let (Some(v), None) = (number, look.bar) {
                        let stats = self.rule_stats(sheet, index, cf);
                        if let (Some(lo), Some(hi)) = (
                            self.cfvo_value(sheet, cf, min, &stats),
                            self.cfvo_value(sheet, cf, max, &stats),
                        ) {
                            // Like Excel's automatic minimum, positive bars start at zero.
                            let lo = if min.kind == CfvoKind::Min {
                                lo.min(0.0)
                            } else {
                                lo
                            };
                            let f = if hi > lo { (v - lo) / (hi - lo) } else { 1.0 };
                            // Excel never draws an empty bar for the minimum.
                            look.bar = Some((f.clamp(0.0, 1.0).max(0.1), *color));
                        }
                    }
                    number.is_some()
                }
            };
            if !matched {
                continue;
            }
            if let Some(style) = cf.rule.style() {
                let s = &mut look.style;
                s.bold = s.bold.or(style.bold);
                s.italic = s.italic.or(style.italic);
                s.underline = s.underline.or(style.underline);
                s.strikethrough = s.strikethrough.or(style.strikethrough);
                s.font_color = s.font_color.or(style.font_color);
                s.fill = s.fill.or(style.fill);
                s.number_format = s.number_format.clone().or(style.number_format.clone());
            }
            if cf.stop_if_true {
                break;
            }
        }
        (!look.is_empty()).then_some(look)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::CellValueInput;
    use crate::cell::CellRange;

    fn at(a1: &str) -> CellCoord {
        CellCoord::from_a1(a1).unwrap()
    }

    fn red() -> CfStyle {
        CfStyle::preset(0)
    }

    fn engine_with(values: &[f64]) -> CalcEngine {
        let mut e = CalcEngine::new();
        for (i, v) in values.iter().enumerate() {
            e.set_value(0, CellCoord::new(i as u32, 0), CellValueInput::Number(*v));
        }
        e
    }

    fn add(e: &mut CalcEngine, range: &str, rule: CfRule) {
        e.formatting_mut(0).conditional.push(ConditionalFormat {
            ranges: vec![CellRange::from_a1(range).unwrap()],
            rule,
            stop_if_true: false,
        });
    }

    #[test]
    fn cell_value_and_text_rules() {
        let mut e = engine_with(&[5.0, 15.0]);
        add(
            &mut e,
            "A1:A9",
            CfRule::CellIs {
                op: CompareOp::Greater,
                formula1: "10".into(),
                formula2: None,
                style: red(),
            },
        );
        assert!(e.conditional_look(0, at("A1")).is_none());
        assert_eq!(
            e.conditional_look(0, at("A2")).unwrap().style.fill,
            red().fill
        );

        e.set_value(0, at("B1"), CellValueInput::Text("Overdue invoice".into()));
        add(
            &mut e,
            "B1:B9",
            CfRule::Text {
                rule: TextRule::Contains,
                text: "overdue".into(),
                style: red(),
            },
        );
        assert!(e.conditional_look(0, at("B1")).is_some());
    }

    #[test]
    fn rank_average_and_duplicates() {
        let mut e = engine_with(&[1.0, 2.0, 3.0, 4.0, 5.0, 5.0]);
        add(
            &mut e,
            "A1:A6",
            CfRule::Top {
                bottom: false,
                rank: 2,
                percent: false,
                style: red(),
            },
        );
        let hit = |e: &CalcEngine, a1: &str| e.conditional_look(0, at(a1)).is_some();
        assert!(hit(&e, "A5") && hit(&e, "A6") && !hit(&e, "A4"));

        e.formatting_mut(0).conditional.clear();
        add(
            &mut e,
            "A1:A6",
            CfRule::Average {
                rule: AverageRule::Above,
                style: red(),
            },
        );
        // Mean is 3.33.
        assert!(hit(&e, "A4") && !hit(&e, "A3"));

        e.formatting_mut(0).conditional.clear();
        add(
            &mut e,
            "A1:A6",
            CfRule::Duplicate {
                unique: false,
                style: red(),
            },
        );
        assert!(hit(&e, "A5") && !hit(&e, "A1"));
        // The cached statistics notice changes.
        e.set_value(0, at("A1"), CellValueInput::Number(2.0));
        assert!(hit(&e, "A1"));
    }

    #[test]
    fn scales_bars_and_formulas() {
        let mut e = engine_with(&[0.0, 50.0, 100.0]);
        add(
            &mut e,
            "A1:A3",
            CfRule::ColorScale {
                stops: vec![(Cfvo::min(), Rgb(255, 0, 0)), (Cfvo::max(), Rgb(0, 0, 255))],
            },
        );
        assert_eq!(
            e.conditional_look(0, at("A1")).unwrap().scale_fill,
            Some(Rgb(255, 0, 0))
        );
        assert_eq!(
            e.conditional_look(0, at("A2")).unwrap().scale_fill,
            Some(Rgb(128, 0, 128))
        );

        e.formatting_mut(0).conditional.clear();
        add(
            &mut e,
            "A1:A3",
            CfRule::DataBar {
                min: Cfvo::min(),
                max: Cfvo::max(),
                color: Rgb(99, 142, 198),
            },
        );
        let bar = |e: &CalcEngine, a1: &str| e.conditional_look(0, at(a1)).unwrap().bar.unwrap().0;
        assert!((bar(&e, "A2") - 0.5).abs() < 1e-9);
        assert_eq!(bar(&e, "A3"), 1.0);

        // Highlight rows where column A is over 40 (relative formula).
        e.formatting_mut(0).conditional.clear();
        add(
            &mut e,
            "A1:B3",
            CfRule::Expression {
                formula: "$A1>40".into(),
                style: red(),
            },
        );
        assert!(e.conditional_look(0, at("B2")).is_some());
        assert!(e.conditional_look(0, at("B1")).is_none());
    }

    #[test]
    fn first_rule_wins_and_stop_if_true() {
        let mut e = engine_with(&[20.0]);
        let green = CfStyle::preset(2);
        add(
            &mut e,
            "A1",
            CfRule::CellIs {
                op: CompareOp::Greater,
                formula1: "10".into(),
                formula2: None,
                style: red(),
            },
        );
        add(
            &mut e,
            "A1",
            CfRule::CellIs {
                op: CompareOp::Greater,
                formula1: "5".into(),
                formula2: None,
                style: CfStyle {
                    bold: Some(true),
                    ..green.clone()
                },
            },
        );
        let look = e.conditional_look(0, at("A1")).unwrap();
        assert_eq!(look.style.fill, red().fill, "earlier rule wins");
        assert_eq!(look.style.bold, Some(true), "later rules fill gaps");
        e.formatting_mut(0).conditional[0].stop_if_true = true;
        assert_eq!(e.conditional_look(0, at("A1")).unwrap().style.bold, None);
    }
}
