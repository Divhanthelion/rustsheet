//! Conditional formatting rules: highlight cells by value, text, rank,
//! average, duplicates, blanks/errors or a formula, and color scales and
//! data bars. Rules apply in list order; the first one to set a property
//! wins, and `stop_if_true` ends the search, as in Excel.

use super::Rgb;
use super::validation::CompareOp;
use crate::cell::{CellCoord, CellRange, LineEdit};

/// What a highlighting rule changes. `None` leaves the cell's own value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CfStyle {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub font_color: Option<Rgb>,
    pub fill: Option<Rgb>,
    pub number_format: Option<String>,
}

impl CfStyle {
    /// Excel's built-in highlight styles.
    pub const PRESETS: [(&'static str, Rgb, Rgb); 4] = [
        (
            "Light red fill, dark red text",
            Rgb(0xFF, 0xC7, 0xCE),
            Rgb(0x9C, 0x00, 0x06),
        ),
        (
            "Yellow fill, dark yellow text",
            Rgb(0xFF, 0xEB, 0x9C),
            Rgb(0x9C, 0x57, 0x00),
        ),
        (
            "Green fill, dark green text",
            Rgb(0xC6, 0xEF, 0xCE),
            Rgb(0x00, 0x61, 0x00),
        ),
        (
            "Light blue fill, dark blue text",
            Rgb(0xDD, 0xEB, 0xF7),
            Rgb(0x1F, 0x4E, 0x79),
        ),
    ];

    pub fn preset(i: usize) -> Self {
        let (_, fill, ink) = Self::PRESETS[i.min(Self::PRESETS.len() - 1)];
        Self {
            fill: Some(fill),
            font_color: Some(ink),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextRule {
    #[default]
    Contains,
    NotContains,
    BeginsWith,
    EndsWith,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AverageRule {
    #[default]
    Above,
    Below,
    EqualOrAbove,
    EqualOrBelow,
}

/// A point on a color scale or data bar: the lowest/highest value, a
/// number, a percent of the range, a percentile, or a formula.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CfvoKind {
    #[default]
    Min,
    Max,
    Number,
    Percent,
    Percentile,
    Formula,
}

impl CfvoKind {
    pub fn xml(self) -> &'static str {
        match self {
            CfvoKind::Min => "min",
            CfvoKind::Max => "max",
            CfvoKind::Number => "num",
            CfvoKind::Percent => "percent",
            CfvoKind::Percentile => "percentile",
            CfvoKind::Formula => "formula",
        }
    }

    pub fn from_xml(s: &str) -> Self {
        match s {
            "max" | "autoMax" => CfvoKind::Max,
            "num" => CfvoKind::Number,
            "percent" => CfvoKind::Percent,
            "percentile" => CfvoKind::Percentile,
            "formula" => CfvoKind::Formula,
            _ => CfvoKind::Min,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cfvo {
    pub kind: CfvoKind,
    pub value: String,
}

impl Cfvo {
    pub fn min() -> Self {
        Self {
            kind: CfvoKind::Min,
            value: String::new(),
        }
    }
    pub fn max() -> Self {
        Self {
            kind: CfvoKind::Max,
            value: String::new(),
        }
    }
    pub fn percentile(p: u32) -> Self {
        Self {
            kind: CfvoKind::Percentile,
            value: p.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CfRule {
    /// Cell value compared with one or two operands (Excel formulas)
    CellIs {
        op: CompareOp,
        formula1: String,
        formula2: Option<String>,
        style: CfStyle,
    },
    Text {
        rule: TextRule,
        text: String,
        style: CfStyle,
    },
    /// Top or bottom N items (or N percent)
    Top {
        bottom: bool,
        rank: u32,
        percent: bool,
        style: CfStyle,
    },
    Average {
        rule: AverageRule,
        style: CfStyle,
    },
    /// Values that appear more than once (or exactly once)
    Duplicate {
        unique: bool,
        style: CfStyle,
    },
    Blanks {
        not: bool,
        style: CfStyle,
    },
    Errors {
        not: bool,
        style: CfStyle,
    },
    /// A formula, relative to the rule's top-left cell, that is true
    Expression {
        formula: String,
        style: CfStyle,
    },
    /// 2 or 3 points with colors
    ColorScale {
        stops: Vec<(Cfvo, Rgb)>,
    },
    DataBar {
        min: Cfvo,
        max: Cfvo,
        color: Rgb,
    },
}

impl CfRule {
    pub fn style(&self) -> Option<&CfStyle> {
        match self {
            CfRule::CellIs { style, .. }
            | CfRule::Text { style, .. }
            | CfRule::Top { style, .. }
            | CfRule::Average { style, .. }
            | CfRule::Duplicate { style, .. }
            | CfRule::Blanks { style, .. }
            | CfRule::Errors { style, .. }
            | CfRule::Expression { style, .. } => Some(style),
            CfRule::ColorScale { .. } | CfRule::DataBar { .. } => None,
        }
    }

    pub fn style_mut(&mut self) -> Option<&mut CfStyle> {
        match self {
            CfRule::CellIs { style, .. }
            | CfRule::Text { style, .. }
            | CfRule::Top { style, .. }
            | CfRule::Average { style, .. }
            | CfRule::Duplicate { style, .. }
            | CfRule::Blanks { style, .. }
            | CfRule::Errors { style, .. }
            | CfRule::Expression { style, .. } => Some(style),
            CfRule::ColorScale { .. } | CfRule::DataBar { .. } => None,
        }
    }

    /// A one-line description for the rules list.
    pub fn describe(&self) -> String {
        match self {
            CfRule::CellIs {
                op,
                formula1,
                formula2,
                ..
            } => match formula2 {
                Some(f2) if op.needs_second() => {
                    format!("Cell value {} {formula1} and {f2}", op.label())
                }
                _ => format!("Cell value {} {formula1}", op.label()),
            },
            CfRule::Text { rule, text, .. } => {
                let verb = match rule {
                    TextRule::Contains => "contains",
                    TextRule::NotContains => "does not contain",
                    TextRule::BeginsWith => "begins with",
                    TextRule::EndsWith => "ends with",
                };
                format!("Text {verb} \"{text}\"")
            }
            CfRule::Top {
                bottom,
                rank,
                percent,
                ..
            } => format!(
                "{} {rank}{}",
                if *bottom { "Bottom" } else { "Top" },
                if *percent { "%" } else { "" }
            ),
            CfRule::Average { rule, .. } => match rule {
                AverageRule::Above => "Above average".into(),
                AverageRule::Below => "Below average".into(),
                AverageRule::EqualOrAbove => "Equal to or above average".into(),
                AverageRule::EqualOrBelow => "Equal to or below average".into(),
            },
            CfRule::Duplicate { unique, .. } => if *unique {
                "Unique values"
            } else {
                "Duplicate values"
            }
            .into(),
            CfRule::Blanks { not, .. } => if *not { "No blanks" } else { "Blanks" }.into(),
            CfRule::Errors { not, .. } => if *not { "No errors" } else { "Errors" }.into(),
            CfRule::Expression { formula, .. } => format!("Formula: ={formula}"),
            CfRule::ColorScale { stops } => format!("{}-color scale", stops.len()),
            CfRule::DataBar { .. } => "Data bar".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionalFormat {
    pub ranges: Vec<CellRange>,
    pub rule: CfRule,
    pub stop_if_true: bool,
}

impl ConditionalFormat {
    pub fn covers(&self, coord: CellCoord) -> bool {
        self.ranges
            .iter()
            .any(|r| super::validation::contains(*r, coord))
    }

    pub fn origin(&self) -> CellCoord {
        self.ranges
            .first()
            .map_or(CellCoord::new(0, 0), |r| r.start)
    }
}

pub fn apply_line_edit(list: &mut Vec<ConditionalFormat>, edit: &LineEdit) {
    for cf in list.iter_mut() {
        cf.ranges = cf
            .ranges
            .iter()
            .filter_map(|r| edit.map_range(*r))
            .collect();
    }
    list.retain(|cf| !cf.ranges.is_empty());
}

/// Mix two colors: `t` = 0 gives `a`, 1 gives `b`.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}
