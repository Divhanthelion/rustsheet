//! Formatting toolbar: font style and size, colors, alignment, borders and
//! number formats for the selected cells.

use crate::format::{CellFormat, HAlign, Rgb, VAlign, is_date_format};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, Vec2};

/// A formatting change the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum FormatAction {
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrikethrough,
    FontSize(Option<u8>),
    FontColor(Option<Rgb>),
    Fill(Option<Rgb>),
    Align(HAlign),
    Borders(BorderPreset),
    NumberFormat(Option<String>),
    /// Add (+1) or remove (-1) a decimal place.
    Decimals(i32),
    Clear,
    ToggleWrap,
    VAlign(VAlign),
    /// Merge & Center, or unmerge
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderPreset {
    All,
    Outside,
    Bottom,
    None,
}

const FONT_SIZES: [u8; 14] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];

/// Office's theme colors, their light tints, and the standard colors.
const PALETTE: [[u32; 10]; 3] = [
    [
        0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5,
        0x70AD47,
    ],
    [
        0xF2F2F2, 0x808080, 0xD0CECE, 0xD6DCE4, 0xD9E1F2, 0xFCE4D6, 0xEDEDED, 0xFFF2CC, 0xDDEBF7,
        0xE2EFDA,
    ],
    [
        0xC00000, 0xFF0000, 0xFFC000, 0xFFFF00, 0x92D050, 0x00B050, 0x00B0F0, 0x0070C0, 0x002060,
        0x7030A0,
    ],
];

/// Number format presets, as (label, code). `None` is General.
pub const NUMBER_FORMATS: [(&str, Option<&str>); 11] = [
    ("General", None),
    ("Number", Some("#,##0.00")),
    ("Currency", Some("$#,##0.00")),
    ("Percent", Some("0.00%")),
    ("Scientific", Some("0.00E+00")),
    ("Fraction", Some("# ?/?")),
    ("Short date", Some("m/d/yyyy")),
    ("ISO date", Some("yyyy-mm-dd")),
    ("Long date", Some("dddd, mmmm d, yyyy")),
    ("Time", Some("h:mm:ss AM/PM")),
    ("Text", Some("@")),
];

fn rgb(v: u32) -> Rgb {
    Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Draw the toolbar for the active cell's format.
pub fn show(ui: &mut Ui, current: &CellFormat) -> Option<FormatAction> {
    let mut action = None;
    let mut set = |a: FormatAction| action = Some(a);
    let button_size = Vec2::new(26.0, 22.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        let size = current.font_size_or_default();
        egui::ComboBox::from_id_salt("font_size")
            .width(44.0)
            .selected_text(size.to_string())
            .show_ui(ui, |ui| {
                for s in FONT_SIZES {
                    if ui.selectable_label(s == size, s.to_string()).clicked() {
                        set(FormatAction::FontSize((s != 11).then_some(s)));
                    }
                }
            })
            .response
            .on_hover_text("Font size");

        ui.separator();

        let style_buttons = [
            (
                RichText::new("B").strong(),
                current.bold,
                "Bold (Ctrl+B)",
                FormatAction::ToggleBold,
            ),
            (
                RichText::new("I").italics(),
                current.italic,
                "Italic (Ctrl+I)",
                FormatAction::ToggleItalic,
            ),
            (
                RichText::new("U").underline(),
                current.underline,
                "Underline (Ctrl+U)",
                FormatAction::ToggleUnderline,
            ),
            (
                RichText::new("S").strikethrough(),
                current.strikethrough,
                "Strikethrough",
                FormatAction::ToggleStrikethrough,
            ),
        ];
        for (label, on, tip, a) in style_buttons {
            let b = egui::Button::new(label.size(15.0))
                .selected(on)
                .min_size(button_size);
            if ui.add(b).on_hover_text(tip).clicked() {
                set(a);
            }
        }

        ui.separator();

        // Like Excel: the letter stays readable, the bar shows the color.
        let r = ui
            .menu_button(RichText::new(" A ").strong().size(15.0), |ui| {
                if let Some(c) = palette(ui, "Automatic", current.font_color) {
                    set(FormatAction::FontColor(c));
                }
            })
            .response
            .on_hover_text("Font color");
        paint_color_bar(ui, r.rect, current.font_color);

        let r = ui
            .menu_button(" Fill ", |ui| {
                if let Some(c) = palette(ui, "No fill", current.fill) {
                    set(FormatAction::Fill(c));
                }
            })
            .response
            .on_hover_text("Fill color");
        paint_color_bar(ui, r.rect, current.fill);

        ui.separator();

        for (align, tip) in [
            (HAlign::Left, "Align left"),
            (HAlign::Center, "Center"),
            (HAlign::Right, "Align right"),
        ] {
            let selected = current.h_align == align;
            let r = ui
                .add(
                    egui::Button::new("")
                        .selected(selected)
                        .min_size(button_size),
                )
                .on_hover_text(tip);
            // Icon-only: give screen readers the name.
            r.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, tip)
            });
            paint_align_icon(ui, r.rect, align);
            if r.clicked() {
                // Clicking the active alignment returns to General, as in Excel.
                set(FormatAction::Align(if selected {
                    HAlign::General
                } else {
                    align
                }));
            }
        }

        for (v, tip) in [
            (VAlign::Top, "Align top"),
            (VAlign::Center, "Align middle"),
            (VAlign::Bottom, "Align bottom"),
        ] {
            let selected = current.v_align == v;
            let r = ui
                .add(
                    egui::Button::new("")
                        .selected(selected)
                        .min_size(button_size),
                )
                .on_hover_text(tip);
            r.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, tip)
            });
            paint_valign_icon(ui, r.rect, v);
            if r.clicked() {
                set(FormatAction::VAlign(v));
            }
        }

        let r = ui
            .add(egui::Button::new("Wrap").selected(current.wrap))
            .on_hover_text("Wrap text onto several lines");
        if r.clicked() {
            set(FormatAction::ToggleWrap);
        }
        if ui
            .button("Merge")
            .on_hover_text("Merge & Center the selection (click again to unmerge)")
            .clicked()
        {
            set(FormatAction::Merge);
        }

        ui.menu_button("Borders", |ui| {
            for (label, preset) in [
                ("All borders", BorderPreset::All),
                ("Outside borders", BorderPreset::Outside),
                ("Bottom border", BorderPreset::Bottom),
                ("No border", BorderPreset::None),
            ] {
                if ui.button(label).clicked() {
                    set(FormatAction::Borders(preset));
                    ui.close_menu();
                }
            }
        });

        ui.separator();

        let code = current.number_format.as_deref();
        let label = NUMBER_FORMATS
            .iter()
            .find(|(_, c)| *c == code)
            .map_or("Custom", |(l, _)| l);
        egui::ComboBox::from_id_salt("number_format")
            .width(96.0)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (l, c) in NUMBER_FORMATS {
                    if ui.selectable_label(c == code, l).clicked() {
                        set(FormatAction::NumberFormat(c.map(str::to_string)));
                    }
                }
            })
            .response
            .on_hover_text(code.unwrap_or("General"));

        if ui
            .add(egui::Button::new("-.0").min_size(button_size))
            .on_hover_text("Decrease decimals")
            .clicked()
        {
            set(FormatAction::Decimals(-1));
        }
        if ui
            .add(egui::Button::new("+.0").min_size(button_size))
            .on_hover_text("Increase decimals")
            .clicked()
        {
            set(FormatAction::Decimals(1));
        }

        ui.separator();

        if ui
            .button("Clear")
            .on_hover_text("Clear formatting")
            .clicked()
        {
            set(FormatAction::Clear);
        }
    });
    action
}

/// Color grid with a reset option. Returns the pick, if any.
fn palette(ui: &mut Ui, none_label: &str, current: Option<Rgb>) -> Option<Option<Rgb>> {
    let mut picked = None;
    if ui.button(none_label).clicked() {
        picked = Some(None);
    }
    egui::Grid::new(("palette", none_label))
        .spacing(Vec2::splat(3.0))
        .show(ui, |ui| {
            for row in PALETTE {
                for hex in row {
                    let c = rgb(hex);
                    let (rect, r) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                    let name = format!("Color #{:06X}", c.to_u32());
                    r.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name)
                    });
                    let r = r.on_hover_text(&name);
                    ui.painter().rect_filled(rect, 2.0, color32(c));
                    let outline = if current == Some(c) || r.hovered() {
                        ui.visuals().selection.stroke
                    } else {
                        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color)
                    };
                    ui.painter()
                        .rect_stroke(rect, 2.0, outline, egui::StrokeKind::Inside);
                    if r.clicked() {
                        picked = Some(Some(c));
                    }
                }
                ui.end_row();
            }
        });
    if picked.is_some() {
        ui.close_menu();
    }
    picked
}

/// A strip under a color button showing its current color.
fn paint_color_bar(ui: &Ui, rect: egui::Rect, color: Option<Rgb>) {
    let Some(c) = color else {
        return;
    };
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 4.0, rect.bottom() - 5.0),
        egui::pos2(rect.right() - 4.0, rect.bottom() - 2.0),
    );
    ui.painter().rect_filled(bar, 0.0, color32(c));
}

/// A bar placed at the top, middle or bottom of a box.
fn paint_valign_icon(ui: &Ui, rect: egui::Rect, v: VAlign) {
    let stroke = Stroke::new(1.5_f32, ui.visuals().text_color());
    let faint = Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    let b = egui::Rect::from_center_size(rect.center(), Vec2::new(14.0, 12.0));
    ui.painter()
        .line_segment([b.left_top(), b.right_top()], faint);
    ui.painter()
        .line_segment([b.left_bottom(), b.right_bottom()], faint);
    let y = match v {
        VAlign::Top => b.top() + 3.0,
        VAlign::Center => b.center().y,
        VAlign::Bottom => b.bottom() - 3.0,
    };
    ui.painter().line_segment(
        [
            egui::pos2(b.left() + 3.0, y),
            egui::pos2(b.right() - 3.0, y),
        ],
        stroke,
    );
}

/// Four lines, aligned the way the button aligns text.
fn paint_align_icon(ui: &Ui, rect: egui::Rect, align: HAlign) {
    let stroke = Stroke::new(1.5_f32, ui.visuals().text_color());
    let full = 14.0;
    let left = rect.center().x - full / 2.0;
    for (i, len) in [14.0, 9.0, 14.0, 9.0].into_iter().enumerate() {
        let y = rect.center().y - 4.5 + i as f32 * 3.0;
        let x0 = match align {
            HAlign::Center => rect.center().x - len / 2.0,
            HAlign::Right => left + full - len,
            _ => left,
        };
        ui.painter()
            .line_segment([egui::pos2(x0, y), egui::pos2(x0 + len, y)], stroke);
    }
}

/// Add or remove one decimal place in a number format code. General becomes
/// `0.00` or `0`; date and time formats are left alone.
pub fn adjust_decimals(code: Option<&str>, delta: i32) -> Option<String> {
    let Some(code) = code.filter(|c| !c.eq_ignore_ascii_case("general")) else {
        return Some(if delta > 0 { "0.00" } else { "0" }.to_string());
    };
    if is_date_format(code) || code == "@" {
        return Some(code.to_string());
    }
    let sections: Vec<String> = split_top_level(code)
        .into_iter()
        .map(|s| adjust_section(s, delta))
        .collect();
    Some(sections.join(";"))
}

fn split_top_level(code: &str) -> Vec<&str> {
    let (mut out, mut start, mut in_quote, mut in_bracket) = (Vec::new(), 0, false, false);
    for (i, c) in code.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            '[' if !in_quote => in_bracket = true,
            ']' if !in_quote => in_bracket = false,
            ';' if !in_quote && !in_bracket => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

fn adjust_section(section: &str, delta: i32) -> String {
    let mut chars: Vec<char> = section.chars().collect();
    let (mut in_quote, mut in_bracket) = (false, false);
    let (mut dot, mut last) = (None, None);
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '"' => in_quote = !in_quote,
            '[' if !in_quote => in_bracket = true,
            ']' if !in_quote => in_bracket = false,
            _ if in_quote || in_bracket => {}
            '.' if dot.is_none() && last.is_some() => dot = Some(i),
            '0' | '#' | '?' => last = Some(i),
            // Stop at an exponent; its digits are not decimals.
            'E' | 'e' if last.is_some() => break,
            _ => {}
        }
    }
    let Some(last) = last else {
        return section.to_string();
    };
    match (dot, delta > 0) {
        (Some(_), true) => chars.insert(last + 1, '0'),
        (Some(d), false) if last > d => {
            chars.remove(last);
            if last == d + 1 {
                chars.remove(d);
            }
        }
        (None, true) => {
            chars.insert(last + 1, '.');
            chars.insert(last + 2, '0');
        }
        _ => {}
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::adjust_decimals;

    #[test]
    fn decimals_step_up_and_down() {
        assert_eq!(adjust_decimals(None, 1).as_deref(), Some("0.00"));
        assert_eq!(adjust_decimals(Some("0"), 1).as_deref(), Some("0.0"));
        assert_eq!(adjust_decimals(Some("0.0"), -1).as_deref(), Some("0"));
        assert_eq!(
            adjust_decimals(Some("#,##0.00"), 1).as_deref(),
            Some("#,##0.000")
        );
        assert_eq!(adjust_decimals(Some("0%"), 1).as_deref(), Some("0.0%"));
        assert_eq!(
            adjust_decimals(Some("0.00E+00"), -1).as_deref(),
            Some("0.0E+00")
        );
        assert_eq!(
            adjust_decimals(Some("$#,##0.00_);[Red]($#,##0.00)"), -1).as_deref(),
            Some("$#,##0.0_);[Red]($#,##0.0)")
        );
        assert_eq!(
            adjust_decimals(Some("yyyy-mm-dd"), 1).as_deref(),
            Some("yyyy-mm-dd")
        );
        assert_eq!(adjust_decimals(Some("0"), -1).as_deref(), Some("0"));
    }
}
