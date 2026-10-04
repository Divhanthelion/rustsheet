//! File > Export as PDF and File > Print: a small options dialog, then the
//! layout in `gui::print`.

use super::*;
use crate::gui::print::{self, PageSetup, Paper, PdfFont};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Output {
    Pdf,
    #[cfg_attr(not(windows), allow(dead_code))]
    Printer,
}

pub(super) struct PrintDialog {
    pub output: Output,
    pub setup: PageSetup,
    pub selection_only: bool,
}

impl SpreadsheetApp {
    pub(super) fn export_pdf(&mut self) {
        self.open_print_dialog(Output::Pdf);
    }

    pub(super) fn print(&mut self) {
        self.open_print_dialog(Output::Printer);
    }

    fn open_print_dialog(&mut self, output: Output) {
        if print::print_area(&self.engine, self.current_sheet).is_none() {
            self.set_status("This sheet is empty: nothing to print");
            return;
        }
        let setup = self
            .print_dialog
            .as_ref()
            .map(|d| d.setup)
            .unwrap_or_default();
        let selection = self.selection.primary_range();
        self.print_dialog = Some(PrintDialog {
            output,
            setup,
            selection_only: selection.start != selection.end,
        });
    }

    /// The cells to print: the selection, or the used area.
    fn print_range(&self, selection_only: bool) -> Option<CellRange> {
        if selection_only {
            Some(self.clamp_to_used(self.selection.primary_range()))
        } else {
            print::print_area(&self.engine, self.current_sheet)
        }
    }

    fn document_title(&self) -> String {
        let name = self.document_name();
        match name.rsplit_once('.') {
            Some((stem, _)) => stem.to_string(),
            None => name,
        }
    }

    pub(super) fn show_print_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.print_dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut go = false;
        let title = match dialog.output {
            Output::Pdf => "Export as PDF",
            Output::Printer => "Print",
        };
        egui::Window::new(title)
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Grid::new("page_setup").num_columns(2).show(ui, |ui| {
                    if dialog.output == Output::Pdf {
                        ui.label("Paper:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut dialog.setup.paper, Paper::Letter, "Letter");
                            ui.selectable_value(&mut dialog.setup.paper, Paper::A4, "A4");
                        });
                        ui.end_row();
                        ui.label("Orientation:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut dialog.setup.landscape, false, "Portrait");
                            ui.selectable_value(&mut dialog.setup.landscape, true, "Landscape");
                        });
                        ui.end_row();
                    }
                    ui.label("Print:");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut dialog.selection_only, false, "Whole sheet");
                        ui.selectable_value(&mut dialog.selection_only, true, "Selection");
                    });
                    ui.end_row();
                });
                ui.checkbox(&mut dialog.setup.gridlines, "Gridlines");
                ui.checkbox(&mut dialog.setup.fit_width, "Fit all columns on one page");
                if dialog.output == Output::Printer {
                    ui.label(
                        RichText::new("Paper and orientation are chosen in the next window.")
                            .small()
                            .weak(),
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let label = match dialog.output {
                        Output::Pdf => "Export...",
                        Output::Printer => "Print...",
                    };
                    if ui.button(label).clicked() {
                        go = true;
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
        if go {
            match dialog.output {
                Output::Pdf => self.write_pdf_file(&dialog),
                Output::Printer => self.send_to_printer(&dialog),
            }
        }
        // Remember the choices for next time.
        let setup = dialog.setup;
        self.print_dialog = if keep { Some(dialog) } else { None };
        self.last_page_setup = setup;
    }

    fn write_pdf_file(&mut self, dialog: &PrintDialog) {
        let Some(range) = self.print_range(dialog.selection_only) else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{}.pdf", self.document_title()))
            .save_file()
        else {
            return;
        };
        let Some(font) = PdfFont::new() else {
            self.set_status("Couldn't load the PDF font");
            return;
        };
        let pages = print::layout(
            &self.engine,
            self.current_sheet,
            range,
            &self.grid_config,
            &dialog.setup,
            &font,
        );
        let pdf = print::write_pdf(&pages, &dialog.setup, &font, &self.document_title());
        match write_atomically(&path, &pdf) {
            Ok(()) => {
                let n = pages.len();
                self.set_status(&format!(
                    "Exported {n} page{} to {}",
                    if n == 1 { "" } else { "s" },
                    path.display()
                ));
            }
            Err(e) => self.set_status(&format!("Couldn't save the PDF: {e}")),
        }
    }

    #[cfg(windows)]
    fn send_to_printer(&mut self, dialog: &PrintDialog) {
        let Some(range) = self.print_range(dialog.selection_only) else {
            return;
        };
        let Some(font) = PdfFont::new() else {
            return;
        };
        let (engine, sheet, config) = (&self.engine, self.current_sheet, &self.grid_config);
        let base = dialog.setup;
        let result = print::print(&self.document_title(), |page| {
            // The printer reports its printable area; keep a small margin.
            let setup = PageSetup {
                page_override: Some(page),
                margin: 18.0,
                ..base
            };
            print::layout(engine, sheet, range, config, &setup, &font)
        });
        match result {
            Ok(true) => self.set_status("Sent to the printer"),
            Ok(false) => {}
            Err(e) => self.set_status(&format!("Couldn't print: {e}")),
        }
    }

    #[cfg(not(windows))]
    fn send_to_printer(&mut self, _dialog: &PrintDialog) {
        self.set_status("Printing isn't available here; export a PDF instead");
    }
}

/// Write to a temporary file beside `path`, then rename over it, so a crash
/// mid-write never leaves a half-written file.
pub(super) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_beside(path);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A hidden temporary name next to `path`.
pub(super) fn temp_beside(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    path.with_file_name(format!(".~{name}.{}.tmp", std::process::id()))
}
