//! Theme and color definitions for the spreadsheet UI

use eframe::egui::{Color32, Stroke, CornerRadius};

/// Colors for the spreadsheet theme
pub struct Theme {
    // Grid colors
    pub grid_line: Color32,
    pub grid_line_major: Color32,
    pub cell_bg: Color32,
    pub cell_bg_alt: Color32,
    pub header_bg: Color32,
    pub header_text: Color32,

    // Selection colors
    pub selection_bg: Color32,
    pub selection_border: Color32,
    pub active_cell_border: Color32,

    // Text colors
    pub text_normal: Color32,
    pub text_number: Color32,
    pub text_formula: Color32,
    pub text_error: Color32,

    // Formula bar
    pub formula_bar_bg: Color32,
    pub formula_bar_border: Color32,

    // Toolbar
    pub toolbar_bg: Color32,
    pub toolbar_button_hover: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    /// Light theme (Excel-like)
    pub fn light() -> Self {
        Self {
            // Grid
            grid_line: Color32::from_rgb(228, 228, 228),
            grid_line_major: Color32::from_rgb(200, 200, 200),
            cell_bg: Color32::WHITE,
            cell_bg_alt: Color32::from_rgb(252, 252, 252),
            header_bg: Color32::from_rgb(242, 242, 242),
            header_text: Color32::from_rgb(68, 68, 68),

            // Selection
            selection_bg: Color32::from_rgba_unmultiplied(66, 133, 244, 40),
            selection_border: Color32::from_rgb(66, 133, 244),
            active_cell_border: Color32::from_rgb(26, 115, 232),

            // Text
            text_normal: Color32::BLACK,
            text_number: Color32::from_rgb(0, 0, 180),
            text_formula: Color32::from_rgb(0, 100, 0),
            text_error: Color32::from_rgb(200, 0, 0),

            // Formula bar
            formula_bar_bg: Color32::WHITE,
            formula_bar_border: Color32::from_rgb(200, 200, 200),

            // Toolbar
            toolbar_bg: Color32::from_rgb(248, 249, 250),
            toolbar_button_hover: Color32::from_rgb(232, 234, 237),
        }
    }

    /// Dark theme
    pub fn dark() -> Self {
        Self {
            // Grid
            grid_line: Color32::from_rgb(60, 60, 60),
            grid_line_major: Color32::from_rgb(80, 80, 80),
            cell_bg: Color32::from_rgb(30, 30, 30),
            cell_bg_alt: Color32::from_rgb(35, 35, 35),
            header_bg: Color32::from_rgb(45, 45, 45),
            header_text: Color32::from_rgb(200, 200, 200),

            // Selection
            selection_bg: Color32::from_rgba_unmultiplied(66, 133, 244, 60),
            selection_border: Color32::from_rgb(100, 150, 255),
            active_cell_border: Color32::from_rgb(130, 180, 255),

            // Text
            text_normal: Color32::from_rgb(230, 230, 230),
            text_number: Color32::from_rgb(130, 180, 255),
            text_formula: Color32::from_rgb(100, 200, 100),
            text_error: Color32::from_rgb(255, 100, 100),

            // Formula bar
            formula_bar_bg: Color32::from_rgb(40, 40, 40),
            formula_bar_border: Color32::from_rgb(70, 70, 70),

            // Toolbar
            toolbar_bg: Color32::from_rgb(38, 38, 38),
            toolbar_button_hover: Color32::from_rgb(55, 55, 55),
        }
    }

    pub fn grid_stroke(&self) -> Stroke {
        Stroke::new(1.0, self.grid_line)
    }

    pub fn selection_stroke(&self) -> Stroke {
        Stroke::new(2.0, self.selection_border)
    }

    pub fn active_cell_stroke(&self) -> Stroke {
        Stroke::new(2.0, self.active_cell_border)
    }

    pub fn cell_rounding(&self) -> CornerRadius {
        CornerRadius::ZERO
    }
}
