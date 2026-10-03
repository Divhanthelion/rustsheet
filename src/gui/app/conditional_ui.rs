//! The Conditional Formatting dialog: the sheet's rules in priority order,
//! and an editor for one rule.

use super::*;
use crate::format::Rgb;
use crate::format::conditional::{
    AverageRule, CfRule, CfStyle, Cfvo, CfvoKind, ConditionalFormat, TextRule, mix,
};
use crate::format::validation::{self as dv, CompareOp};
use eframe::egui::{Color32, Rect, Sense, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuleKind {
    CellValue,
    Text,
    Top,
    Average,
    Duplicate,
    Blanks,
    Errors,
    Formula,
    TwoColorScale,
    ThreeColorScale,
    DataBar,
}

const KINDS: [(RuleKind, &str); 11] = [
    (RuleKind::CellValue, "Cell value"),
    (RuleKind::Text, "Specific text"),
    (RuleKind::Top, "Top or bottom values"),
    (RuleKind::Average, "Above or below average"),
    (RuleKind::Duplicate, "Duplicate or unique values"),
    (RuleKind::Blanks, "Blanks"),
    (RuleKind::Errors, "Errors"),
    (RuleKind::Formula, "Formula"),
    (RuleKind::TwoColorScale, "2-color scale"),
    (RuleKind::ThreeColorScale, "3-color scale"),
    (RuleKind::DataBar, "Data bar"),
];

const POINT_KINDS: [(CfvoKind, &str); 6] = [
    (CfvoKind::Min, "Lowest value"),
    (CfvoKind::Number, "Number"),
    (CfvoKind::Percent, "Percent"),
    (CfvoKind::Percentile, "Percentile"),
    (CfvoKind::Formula, "Formula"),
    (CfvoKind::Max, "Highest value"),
];

/// A rule being created or edited. It holds every kind's fields, so
/// switching kinds keeps what was typed.
#[derive(Clone, Debug)]
pub(super) struct RuleEditor {
    /// Position in the dialog's list, or `None` for a new rule
    index: Option<usize>,
    pub kind: RuleKind,
    pub applies_to: String,
    stop_if_true: bool,
    pub op: CompareOp,
    /// As typed: a number, a date, text, or `=formula`
    pub value1: String,
    pub value2: String,
    text_rule: TextRule,
    pub text: String,
    bottom: bool,
    rank: u32,
    percent: bool,
    average: AverageRule,
    unique: bool,
    not: bool,
    pub formula: String,
    two: [(Cfvo, Rgb); 2],
    three: [(Cfvo, Rgb); 3],
    bar: [Cfvo; 2],
    bar_color: Rgb,
    pub style: CfStyle,
    error: Option<String>,
}

pub(super) struct CfDialog {
    pub rules: Vec<ConditionalFormat>,
    selected: Option<usize>,
    pub editor: Option<RuleEditor>,
    /// Opened straight into a new rule: OK applies and closes
    quick: bool,
    /// "Applies to" for new rules
    selection: CellRange,
}

/// "A1:A10 C1:C10" (commas and `$` are accepted too).
fn parse_ranges(s: &str) -> Option<Vec<CellRange>> {
    let mut out = Vec::new();
    for part in s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
    {
        let p = part
            .trim_start_matches('=')
            .replace('$', "")
            .to_ascii_uppercase();
        out.push(CellRange::from_a1(&p).or_else(|| CellCoord::from_a1(&p).map(CellRange::single))?);
    }
    (!out.is_empty()).then_some(out)
}

fn ranges_text(ranges: &[CellRange]) -> String {
    ranges
        .iter()
        .map(|r| r.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A stored operand as the user would type it.
fn operand_text(formula: &str) -> String {
    let t = formula.trim();
    if let Some(inner) = t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return inner.replace("\"\"", "\"");
    }
    if t.parse::<f64>().is_ok() {
        return t.to_string();
    }
    format!("={t}")
}

/// What the user typed as a stored operand: `=` starts a formula, numbers
/// and dates are numbers, anything else is text.
fn typed_operand(s: &str) -> String {
    let t = s.trim();
    if let Some(f) = t.strip_prefix('=') {
        return f.trim().to_string();
    }
    if let Some((n, _)) = crate::format::parse_typed_number(t) {
        return n.to_string();
    }
    format!("\"{}\"", t.replace('"', "\"\""))
}

fn color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

impl RuleEditor {
    pub fn new(applies_to: String) -> Self {
        Self {
            index: None,
            kind: RuleKind::CellValue,
            applies_to,
            stop_if_true: false,
            op: CompareOp::Greater,
            value1: String::new(),
            value2: String::new(),
            text_rule: TextRule::Contains,
            text: String::new(),
            bottom: false,
            rank: 10,
            percent: false,
            average: AverageRule::Above,
            unique: false,
            not: false,
            formula: String::new(),
            two: [
                (Cfvo::min(), Rgb(0xFC, 0xFC, 0xFF)),
                (Cfvo::max(), Rgb(0x63, 0xBE, 0x7B)),
            ],
            three: [
                (Cfvo::min(), Rgb(0xF8, 0x69, 0x6B)),
                (Cfvo::percentile(50), Rgb(0xFF, 0xEB, 0x84)),
                (Cfvo::max(), Rgb(0x63, 0xBE, 0x7B)),
            ],
            bar: [Cfvo::min(), Cfvo::max()],
            bar_color: Rgb(0x63, 0x8E, 0xC6),
            style: CfStyle::preset(0),
            error: None,
        }
    }

    fn from_rule(index: usize, cf: &ConditionalFormat) -> Self {
        let mut e = Self::new(ranges_text(&cf.ranges));
        e.index = Some(index);
        e.stop_if_true = cf.stop_if_true;
        if let Some(style) = cf.rule.style() {
            e.style = style.clone();
        }
        match &cf.rule {
            CfRule::CellIs {
                op,
                formula1,
                formula2,
                ..
            } => {
                e.kind = RuleKind::CellValue;
                e.op = *op;
                e.value1 = operand_text(formula1);
                e.value2 = formula2.as_deref().map(operand_text).unwrap_or_default();
            }
            CfRule::Text { rule, text, .. } => {
                e.kind = RuleKind::Text;
                e.text_rule = *rule;
                e.text = text.clone();
            }
            CfRule::Top {
                bottom,
                rank,
                percent,
                ..
            } => {
                e.kind = RuleKind::Top;
                (e.bottom, e.rank, e.percent) = (*bottom, *rank, *percent);
            }
            CfRule::Average { rule, .. } => {
                e.kind = RuleKind::Average;
                e.average = *rule;
            }
            CfRule::Duplicate { unique, .. } => {
                e.kind = RuleKind::Duplicate;
                e.unique = *unique;
            }
            CfRule::Blanks { not, .. } => {
                e.kind = RuleKind::Blanks;
                e.not = *not;
            }
            CfRule::Errors { not, .. } => {
                e.kind = RuleKind::Errors;
                e.not = *not;
            }
            CfRule::Expression { formula, .. } => {
                e.kind = RuleKind::Formula;
                e.formula = format!("={formula}");
            }
            CfRule::ColorScale { stops } => match stops.as_slice() {
                [a, b] => {
                    e.kind = RuleKind::TwoColorScale;
                    e.two = [a.clone(), b.clone()];
                }
                [a, b, c] => {
                    e.kind = RuleKind::ThreeColorScale;
                    e.three = [a.clone(), b.clone(), c.clone()];
                }
                _ => {}
            },
            CfRule::DataBar { min, max, color } => {
                e.kind = RuleKind::DataBar;
                e.bar = [min.clone(), max.clone()];
                e.bar_color = *color;
            }
        }
        e
    }

    /// The rule as entered, or what's missing.
    pub fn build(&self) -> Result<ConditionalFormat, String> {
        let ranges = parse_ranges(&self.applies_to)
            .ok_or("Enter the cells it applies to, like A1:A10 or A1:A10 C1:C10.")?;
        let style = self.style.clone();
        let blank = |s: &str| s.trim().trim_start_matches('=').trim().is_empty();
        let rule = match self.kind {
            RuleKind::CellValue => {
                let second = self.op.needs_second();
                if blank(&self.value1) || (second && blank(&self.value2)) {
                    return Err("Enter the value to compare with.".into());
                }
                CfRule::CellIs {
                    op: self.op,
                    formula1: typed_operand(&self.value1),
                    formula2: second.then(|| typed_operand(&self.value2)),
                    style,
                }
            }
            RuleKind::Text => {
                if self.text.is_empty() {
                    return Err("Enter the text to look for.".into());
                }
                CfRule::Text {
                    rule: self.text_rule,
                    text: self.text.clone(),
                    style,
                }
            }
            RuleKind::Top => CfRule::Top {
                bottom: self.bottom,
                rank: self.rank.clamp(1, if self.percent { 100 } else { 1000 }),
                percent: self.percent,
                style,
            },
            RuleKind::Average => CfRule::Average {
                rule: self.average,
                style,
            },
            RuleKind::Duplicate => CfRule::Duplicate {
                unique: self.unique,
                style,
            },
            RuleKind::Blanks => CfRule::Blanks {
                not: self.not,
                style,
            },
            RuleKind::Errors => CfRule::Errors {
                not: self.not,
                style,
            },
            RuleKind::Formula => {
                if blank(&self.formula) {
                    return Err("Enter a formula, like =$B1>100.".into());
                }
                let formula = self
                    .formula
                    .trim()
                    .trim_start_matches('=')
                    .trim()
                    .to_string();
                if crate::formula::FormulaParser::new()
                    .parse(&format!("={formula}"))
                    .is_err()
                {
                    return Err("That formula can't be read.".into());
                }
                CfRule::Expression { formula, style }
            }
            RuleKind::TwoColorScale => CfRule::ColorScale {
                stops: self.two.to_vec(),
            },
            RuleKind::ThreeColorScale => CfRule::ColorScale {
                stops: self.three.to_vec(),
            },
            RuleKind::DataBar => CfRule::DataBar {
                min: self.bar[0].clone(),
                max: self.bar[1].clone(),
                color: self.bar_color,
            },
        };
        Ok(ConditionalFormat {
            ranges,
            rule,
            stop_if_true: self.stop_if_true,
        })
    }

    fn show(&mut self, ui: &mut Ui) {
        egui::Grid::new("cf_rule")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Applies to:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.applies_to)
                        .desired_width(220.0)
                        .hint_text("A1:A10"),
                );
                ui.end_row();
                ui.label("Format cells by:");
                egui::ComboBox::from_id_salt("cf_kind")
                    .width(220.0)
                    .selected_text(KINDS.iter().find(|k| k.0 == self.kind).map_or("", |k| k.1))
                    .show_ui(ui, |ui| {
                        for (kind, label) in KINDS {
                            ui.selectable_value(&mut self.kind, kind, label);
                        }
                    });
                ui.end_row();
                self.show_condition(ui);
            });
        let highlights = !matches!(
            self.kind,
            RuleKind::TwoColorScale | RuleKind::ThreeColorScale | RuleKind::DataBar
        );
        if highlights {
            ui.separator();
            style_editor(ui, &mut self.style);
            ui.checkbox(&mut self.stop_if_true, "Stop if true (skip later rules)");
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    fn show_condition(&mut self, ui: &mut Ui) {
        match self.kind {
            RuleKind::CellValue => {
                ui.label("Cell value is:");
                egui::ComboBox::from_id_salt("cf_op")
                    .width(220.0)
                    .selected_text(self.op.label())
                    .show_ui(ui, |ui| {
                        for op in CompareOp::ALL {
                            ui.selectable_value(&mut self.op, op, op.label());
                        }
                    });
                ui.end_row();
                let hint = "100, 2026-01-31, text or =$B$1";
                ui.label(if self.op.needs_second() {
                    "From:"
                } else {
                    "Value:"
                });
                ui.add(egui::TextEdit::singleline(&mut self.value1).hint_text(hint));
                ui.end_row();
                if self.op.needs_second() {
                    ui.label("To:");
                    ui.add(egui::TextEdit::singleline(&mut self.value2).hint_text(hint));
                    ui.end_row();
                }
            }
            RuleKind::Text => {
                ui.label("Text:");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("cf_text")
                        .selected_text(match self.text_rule {
                            TextRule::Contains => "contains",
                            TextRule::NotContains => "does not contain",
                            TextRule::BeginsWith => "begins with",
                            TextRule::EndsWith => "ends with",
                        })
                        .show_ui(ui, |ui| {
                            let r = &mut self.text_rule;
                            ui.selectable_value(r, TextRule::Contains, "contains");
                            ui.selectable_value(r, TextRule::NotContains, "does not contain");
                            ui.selectable_value(r, TextRule::BeginsWith, "begins with");
                            ui.selectable_value(r, TextRule::EndsWith, "ends with");
                        });
                    ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(120.0));
                });
                ui.end_row();
            }
            RuleKind::Top => {
                ui.label("Format the:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.bottom, false, "Top");
                    ui.selectable_value(&mut self.bottom, true, "Bottom");
                    ui.add(egui::DragValue::new(&mut self.rank).range(1..=1000));
                    ui.checkbox(&mut self.percent, "% of the range");
                });
                ui.end_row();
            }
            RuleKind::Average => {
                ui.label("Values:");
                egui::ComboBox::from_id_salt("cf_avg")
                    .width(220.0)
                    .selected_text(
                        CfRule::Average {
                            rule: self.average,
                            style: CfStyle::default(),
                        }
                        .describe(),
                    )
                    .show_ui(ui, |ui| {
                        let a = &mut self.average;
                        ui.selectable_value(a, AverageRule::Above, "Above average");
                        ui.selectable_value(a, AverageRule::Below, "Below average");
                        ui.selectable_value(
                            a,
                            AverageRule::EqualOrAbove,
                            "Equal to or above average",
                        );
                        ui.selectable_value(
                            a,
                            AverageRule::EqualOrBelow,
                            "Equal to or below average",
                        );
                    });
                ui.end_row();
            }
            RuleKind::Duplicate => {
                ui.label("Values that are:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.unique, false, "Duplicates");
                    ui.selectable_value(&mut self.unique, true, "Unique");
                });
                ui.end_row();
            }
            RuleKind::Blanks | RuleKind::Errors => {
                let (yes, no) = if self.kind == RuleKind::Blanks {
                    ("Blank cells", "Cells that aren't blank")
                } else {
                    ("Errors", "Cells without errors")
                };
                ui.label("Format:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.not, false, yes);
                    ui.selectable_value(&mut self.not, true, no);
                });
                ui.end_row();
            }
            RuleKind::Formula => {
                ui.label("Formula is true:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.formula)
                        .desired_width(220.0)
                        .hint_text("=$B1>100 (for the first cell)"),
                );
                ui.end_row();
            }
            RuleKind::TwoColorScale => {
                let [lo, hi] = &mut self.two;
                scale_point(ui, "Minimum:", "cf_lo", lo, true);
                scale_point(ui, "Maximum:", "cf_hi", hi, true);
            }
            RuleKind::ThreeColorScale => {
                let [lo, mid, hi] = &mut self.three;
                scale_point(ui, "Minimum:", "cf_lo", lo, true);
                scale_point(ui, "Midpoint:", "cf_mid", mid, false);
                scale_point(ui, "Maximum:", "cf_hi", hi, true);
            }
            RuleKind::DataBar => {
                let [lo, hi] = &mut self.bar;
                ui.label("Shortest bar:");
                point_value(ui, "cf_bar_lo", lo, true);
                ui.end_row();
                ui.label("Longest bar:");
                point_value(ui, "cf_bar_hi", hi, true);
                ui.end_row();
                ui.label("Bar color:");
                color_button(ui, &mut self.bar_color);
                ui.end_row();
            }
        }
    }
}

/// A color scale point: what value, and its color.
fn scale_point(ui: &mut Ui, label: &str, id: &str, point: &mut (Cfvo, Rgb), end: bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        point_value(ui, id, &mut point.0, end);
        color_button(ui, &mut point.1);
    });
    ui.end_row();
}

/// Lowest/highest value (ends only), or a number, percent, percentile or
/// formula.
fn point_value(ui: &mut Ui, id: &str, v: &mut Cfvo, end: bool) {
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id)
            .width(110.0)
            .selected_text(
                POINT_KINDS
                    .iter()
                    .find(|k| k.0 == v.kind)
                    .map_or("", |k| k.1),
            )
            .show_ui(ui, |ui| {
                for (kind, label) in POINT_KINDS {
                    let fixed = matches!(kind, CfvoKind::Min | CfvoKind::Max);
                    if fixed && !end {
                        continue;
                    }
                    if ui.selectable_label(v.kind == kind, label).clicked() {
                        v.kind = kind;
                        if fixed {
                            v.value.clear();
                        } else if v.value.is_empty() {
                            v.value = "50".into();
                        }
                    }
                }
            });
        if !matches!(v.kind, CfvoKind::Min | CfvoKind::Max) {
            ui.add(egui::TextEdit::singleline(&mut v.value).desired_width(70.0));
        }
    });
}

fn color_button(ui: &mut Ui, c: &mut Rgb) {
    let mut rgb = [c.0, c.1, c.2];
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        *c = Rgb(rgb[0], rgb[1], rgb[2]);
    }
}

/// Built-in highlight styles, or font and fill settings.
fn style_editor(ui: &mut Ui, style: &mut CfStyle) {
    let preset = (0..CfStyle::PRESETS.len()).find(|&i| *style == CfStyle::preset(i));
    ui.horizontal(|ui| {
        ui.label("Format with:");
        egui::ComboBox::from_id_salt("cf_style")
            .width(220.0)
            .selected_text(preset.map_or("Custom format", |i| CfStyle::PRESETS[i].0))
            .show_ui(ui, |ui| {
                for (i, (label, ..)) in CfStyle::PRESETS.iter().enumerate() {
                    if ui.selectable_label(preset == Some(i), *label).clicked() {
                        *style = CfStyle::preset(i);
                    }
                }
                if ui.selectable_label(false, "Bold text").clicked() {
                    *style = CfStyle {
                        bold: Some(true),
                        ..Default::default()
                    };
                }
                if ui.selectable_label(false, "Red text").clicked() {
                    *style = CfStyle {
                        font_color: Some(Rgb(0xC0, 0, 0)),
                        ..Default::default()
                    };
                }
            });
    });
    ui.horizontal(|ui| {
        let toggle = |ui: &mut Ui, v: &mut Option<bool>, label: &str| {
            let mut on = *v == Some(true);
            if ui.checkbox(&mut on, label).changed() {
                *v = on.then_some(true);
            }
        };
        toggle(ui, &mut style.bold, "Bold");
        toggle(ui, &mut style.italic, "Italic");
        toggle(ui, &mut style.underline, "Underline");
        toggle(ui, &mut style.strikethrough, "Strikethrough");
    });
    ui.horizontal(|ui| {
        let color = |ui: &mut Ui, v: &mut Option<Rgb>, label: &str, default: Rgb| {
            let mut on = v.is_some();
            if ui.checkbox(&mut on, label).changed() {
                *v = on.then(|| v.unwrap_or(default));
            }
            if let Some(c) = v {
                color_button(ui, c);
            }
        };
        color(
            ui,
            &mut style.font_color,
            "Font color",
            Rgb(0x9C, 0x00, 0x06),
        );
        color(ui, &mut style.fill, "Fill", Rgb(0xFF, 0xC7, 0xCE));
    });
    ui.horizontal(|ui| {
        ui.label("Preview:");
        style_sample(ui, style);
    });
}

fn style_sample(ui: &mut Ui, style: &CfStyle) {
    let mut text = RichText::new("AaBbCcYyZz");
    if style.bold == Some(true) {
        text = text.strong();
    }
    if style.italic == Some(true) {
        text = text.italics();
    }
    if style.underline == Some(true) {
        text = text.underline();
    }
    if style.strikethrough == Some(true) {
        text = text.strikethrough();
    }
    match (style.font_color, style.fill) {
        (Some(c), _) => text = text.color(color32(c)),
        (None, Some(fill)) if fill.luminance() > 0.5 => text = text.color(Color32::BLACK),
        (None, Some(_)) => text = text.color(Color32::WHITE),
        (None, None) => {}
    }
    egui::Frame::NONE
        .fill(style.fill.map_or(Color32::TRANSPARENT, color32))
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| ui.label(text));
}

/// A small picture of what a rule does, for the rules list.
fn rule_sample(ui: &mut Ui, rule: &CfRule) {
    match rule {
        CfRule::ColorScale { stops } if stops.len() >= 2 => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(90.0, 18.0), Sense::hover());
            let mut mesh = egui::Mesh::default();
            let n = stops.len() - 1;
            for (i, (_, color)) in stops.iter().enumerate() {
                let x = rect.left() + rect.width() * i as f32 / n as f32;
                mesh.colored_vertex(egui::pos2(x, rect.top()), color32(*color));
                mesh.colored_vertex(egui::pos2(x, rect.bottom()), color32(*color));
                if i > 0 {
                    let k = (i * 2) as u32;
                    mesh.add_triangle(k - 2, k - 1, k);
                    mesh.add_triangle(k - 1, k, k + 1);
                }
            }
            ui.painter().add(mesh);
        }
        CfRule::DataBar { color, .. } => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(90.0, 18.0), Sense::hover());
            let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width() * 0.7, rect.height()));
            let mut mesh = egui::Mesh::default();
            let faded = color32(mix(*color, Rgb::WHITE, 0.85));
            mesh.colored_vertex(bar.left_top(), color32(*color));
            mesh.colored_vertex(bar.right_top(), faded);
            mesh.colored_vertex(bar.right_bottom(), faded);
            mesh.colored_vertex(bar.left_bottom(), color32(*color));
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(mesh);
        }
        _ => {
            if let Some(style) = rule.style() {
                style_sample(ui, style);
            } else {
                ui.label("");
            }
        }
    }
}

impl SpreadsheetApp {
    pub(super) fn open_conditional_dialog(&mut self) {
        let sheet = self.current_sheet;
        let rules = self
            .engine
            .formatting(sheet)
            .map(|f| f.conditional.clone())
            .unwrap_or_default();
        let selection = self.clamp_to_used_or_self(self.selection.primary_range());
        let active = self.selection.active;
        let covering = rules.iter().position(|r| r.covers(active));
        // With nothing on the selection yet, start with a new rule for it.
        let quick = covering.is_none();
        self.cf_dialog = Some(CfDialog {
            editor: quick.then(|| RuleEditor::new(selection.to_string())),
            rules,
            selected: covering,
            quick,
            selection,
        });
    }

    /// Replace the sheet's rules, as one undo step.
    pub(super) fn apply_conditional(&mut self, rules: Vec<ConditionalFormat>) {
        let sheet = self.current_sheet;
        let unchanged = self
            .engine
            .formatting(sheet)
            .map_or(rules.is_empty(), |f| f.conditional == rules);
        if unchanged {
            return;
        }
        self.with_snapshot(|app| {
            app.engine.formatting_mut(sheet).conditional = rules;
            true
        });
    }

    pub(super) fn show_conditional_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.cf_dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut apply = false;
        let active = self.selection.active;
        egui::Window::new("Conditional Formatting")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                if let Some(editor) = &mut d.editor {
                    editor.show(ui);
                    ui.separator();
                    let mut done = None;
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            done = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            done = Some(false);
                        }
                        if d.quick && ui.button("All Rules...").clicked() {
                            d.quick = false;
                            d.editor = None;
                        }
                    });
                    match done {
                        Some(true) => {
                            if let Some(editor) = &mut d.editor {
                                match editor.build() {
                                    Ok(rule) => {
                                        let i = match editor.index {
                                            Some(i) if i < d.rules.len() => {
                                                d.rules[i] = rule;
                                                i
                                            }
                                            // New rules go first, as in Excel.
                                            _ => {
                                                d.rules.insert(0, rule);
                                                0
                                            }
                                        };
                                        d.selected = Some(i);
                                        d.editor = None;
                                        if d.quick {
                                            apply = true;
                                            keep = false;
                                        }
                                    }
                                    Err(e) => editor.error = Some(e),
                                }
                            }
                        }
                        Some(false) => {
                            d.editor = None;
                            keep = !d.quick;
                        }
                        None => {}
                    }
                    return;
                }

                ui.label("Rules for this sheet. Where they overlap, the higher rule wins.");
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        if d.rules.is_empty() {
                            ui.label(RichText::new("No rules yet.").weak());
                        }
                        egui::Grid::new("cf_rules")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(RichText::new("Rule").strong());
                                ui.label(RichText::new("Format").strong());
                                ui.label(RichText::new("Applies to").strong());
                                ui.label(RichText::new("Stop if true").strong());
                                ui.end_row();
                                for i in 0..d.rules.len() {
                                    let cf = &mut d.rules[i];
                                    let mut label = RichText::new(cf.rule.describe());
                                    if cf.covers(active) {
                                        label = label.strong();
                                    }
                                    let r = ui.selectable_label(d.selected == Some(i), label);
                                    if r.clicked() {
                                        d.selected = Some(i);
                                    }
                                    if r.double_clicked() {
                                        d.editor = Some(RuleEditor::from_rule(i, cf));
                                    }
                                    rule_sample(ui, &cf.rule);
                                    ui.label(ranges_text(&cf.ranges));
                                    if cf.rule.style().is_some() {
                                        ui.checkbox(&mut cf.stop_if_true, "");
                                    } else {
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                let selected = d.selected.filter(|&i| i < d.rules.len());
                ui.horizontal(|ui| {
                    if ui.button("New Rule...").clicked() {
                        d.editor = Some(RuleEditor::new(d.selection.to_string()));
                    }
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Edit Rule..."))
                        .clicked()
                    {
                        if let Some(i) = selected {
                            d.editor = Some(RuleEditor::from_rule(i, &d.rules[i]));
                        }
                    }
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Delete Rule"))
                        .clicked()
                    {
                        if let Some(i) = selected {
                            d.rules.remove(i);
                            d.selected = None;
                        }
                    }
                    let up = selected.is_some_and(|i| i > 0);
                    if ui.add_enabled(up, egui::Button::new("Move Up")).clicked() {
                        if let Some(i) = selected {
                            d.rules.swap(i, i - 1);
                            d.selected = Some(i - 1);
                        }
                    }
                    let down = selected.is_some_and(|i| i + 1 < d.rules.len());
                    if ui
                        .add_enabled(down, egui::Button::new("Move Down"))
                        .clicked()
                    {
                        if let Some(i) = selected {
                            d.rules.swap(i, i + 1);
                            d.selected = Some(i + 1);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Clear Rules from Selection").clicked() {
                        let area = d.selection;
                        for cf in &mut d.rules {
                            cf.ranges = cf
                                .ranges
                                .iter()
                                .flat_map(|r| dv::subtract(*r, area))
                                .collect();
                        }
                        d.rules.retain(|cf| !cf.ranges.is_empty());
                        d.selected = None;
                    }
                    if ui.button("Clear All Rules").clicked() {
                        d.rules.clear();
                        d.selected = None;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        apply = true;
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if d.editor.is_some() && !d.quick {
                d.editor = None;
            } else {
                keep = false;
            }
        }
        if apply {
            self.apply_conditional(d.rules.clone());
        }
        if keep {
            self.cf_dialog = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operands_as_typed() {
        assert_eq!(typed_operand("100"), "100");
        assert_eq!(typed_operand("=$B$1"), "$B$1");
        assert_eq!(typed_operand("Done"), "\"Done\"");
        assert_eq!(typed_operand("say \"hi\""), "\"say \"\"hi\"\"\"");
        for stored in ["100", "$B$1", "\"Done\""] {
            assert_eq!(typed_operand(&operand_text(stored)), stored);
        }
        assert_eq!(
            parse_ranges("$A$1:$A$10, c1:c10"),
            Some(vec![
                CellRange::from_a1("A1:A10").unwrap(),
                CellRange::from_a1("C1:C10").unwrap()
            ])
        );
        assert_eq!(parse_ranges("A1:"), None);
    }

    #[test]
    fn editing_a_rule_keeps_it() {
        let rules = [
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("A1:B5").unwrap()],
                rule: CfRule::CellIs {
                    op: CompareOp::Between,
                    formula1: "1".into(),
                    formula2: Some("$C$1".into()),
                    style: CfStyle::preset(1),
                },
                stop_if_true: true,
            },
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("D1:D9").unwrap()],
                rule: CfRule::Expression {
                    formula: "$A1>5".into(),
                    style: CfStyle::preset(2),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("E1:E9").unwrap()],
                rule: CfRule::ColorScale {
                    stops: vec![(Cfvo::min(), Rgb::WHITE), (Cfvo::max(), Rgb::BLACK)],
                },
                stop_if_true: false,
            },
        ];
        for cf in rules {
            assert_eq!(RuleEditor::from_rule(0, &cf).build().unwrap(), cf);
        }
    }
}
