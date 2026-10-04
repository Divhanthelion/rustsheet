//! Pictures in the app: Insert > Picture, pasting images, the textures the
//! grid draws, and the picture menu (delete, size, order, alt text).

use super::*;
use crate::format::picture::{Picture, PictureKind};
use crate::gui::grid::{PictureAction, PicturePlace, picture_key};
use std::sync::Arc;

/// Pictures larger than this many pixels on a side are shown scaled down.
const MAX_TEXTURE_SIDE: u32 = 2048;

/// Decoded pictures for the current sheet, by `picture_key`.
#[derive(Default)]
pub(super) struct PictureTextures {
    /// The image data is kept so its address (the key) stays unique.
    loaded: HashMap<usize, (Arc<[u8]>, Option<egui::TextureHandle>)>,
    pub ids: HashMap<usize, egui::TextureId>,
}

impl PictureTextures {
    /// Load textures for `pictures` and drop ones no longer shown.
    pub fn update(&mut self, ctx: &egui::Context, pictures: &[Picture]) {
        let live: HashSet<usize> = pictures.iter().map(picture_key).collect();
        let before = self.loaded.len();
        self.loaded.retain(|key, _| live.contains(key));
        let mut changed = self.loaded.len() != before;
        for p in pictures {
            let key = picture_key(p);
            if self.loaded.contains_key(&key) {
                continue;
            }
            let texture = decode(&p.data)
                .map(|image| ctx.load_texture(format!("picture-{key}"), image, Default::default()));
            self.loaded.insert(key, (p.data.clone(), texture));
            changed = true;
        }
        if changed {
            self.ids = self
                .loaded
                .iter()
                .filter_map(|(key, (_, t))| Some((*key, t.as_ref()?.id())))
                .collect();
        }
    }
}

fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut image = image::load_from_memory(bytes).ok()?;
    if image.width() > MAX_TEXTURE_SIDE || image.height() > MAX_TEXTURE_SIDE {
        image = image.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE);
    }
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

/// Width and height of an image file in pixels.
fn pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Editing a picture's alt text.
pub(super) struct AltTextEditor {
    index: usize,
    text: String,
}

impl SpreadsheetApp {
    pub(super) fn sheet_pictures(&self) -> &[Picture] {
        self.engine
            .formatting(self.current_sheet)
            .map_or(&[], |f| f.pictures.as_slice())
    }

    /// Insert > Picture...
    pub(super) fn insert_picture_from_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "bmp"])
            .pick_file()
        else {
            return;
        };
        let result = std::fs::read(&path)
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))
            .and_then(|bytes| self.insert_picture(bytes));
        if let Err(message) = result {
            self.set_status(&message);
        }
    }

    /// Put an image file on the sheet at the active cell, selected.
    pub(super) fn insert_picture(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let kind = PictureKind::detect(&bytes)
            .ok_or("That file isn't a PNG, JPEG, GIF or BMP picture.")?;
        let pixels = pixel_size(&bytes).ok_or("That picture can't be read.")?;
        let mut picture = Picture::new(self.selection.active, Arc::from(bytes), kind, pixels);
        // Large pictures start at a size that fits the window.
        let view = self.last_viewport;
        picture.fit_within(((view.x * 0.6).max(240.0), (view.y * 0.6).max(180.0)));
        let sheet = self.current_sheet;
        let mut index = 0;
        self.with_snapshot(|app| {
            let list = &mut app.engine.formatting_mut(sheet).pictures;
            list.push(picture);
            index = list.len() - 1;
            true
        });
        self.selected_picture = Some(index);
        self.set_status("Inserted picture. Drag to move it, or drag a corner to resize.");
        Ok(())
    }

    /// Paste a clipboard image (a screenshot, say) as a picture.
    pub(super) fn paste_image(&mut self, image: arboard::ImageData) {
        let Some(rgba) = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        ) else {
            return;
        };
        let mut png = std::io::Cursor::new(Vec::new());
        if rgba.write_to(&mut png, image::ImageFormat::Png).is_err() {
            self.set_status("That image can't be pasted");
            return;
        }
        if let Err(message) = self.insert_picture(png.into_inner()) {
            self.set_status(&message);
        }
    }

    /// Move or resize a picture, as one undo step.
    pub(super) fn place_picture(&mut self, index: usize, place: PicturePlace) {
        let sheet = self.current_sheet;
        self.with_snapshot(|app| {
            let Some(p) = app.engine.formatting_mut(sheet).pictures.get_mut(index) else {
                return false;
            };
            p.anchor = place.anchor;
            p.offset = place.offset;
            p.size = place.size;
            true
        });
    }

    pub(super) fn delete_picture(&mut self, index: usize) {
        let sheet = self.current_sheet;
        self.with_snapshot(|app| {
            let list = &mut app.engine.formatting_mut(sheet).pictures;
            if index >= list.len() {
                return false;
            }
            list.remove(index);
            true
        });
        self.selected_picture = None;
        self.set_status("Deleted picture");
    }

    pub(super) fn picture_action(&mut self, index: usize, action: PictureAction) {
        let sheet = self.current_sheet;
        let Some(p) = self.sheet_pictures().get(index).cloned() else {
            return;
        };
        match action {
            PictureAction::Delete => self.delete_picture(index),
            PictureAction::ResetSize => {
                if let Some(pixels) = pixel_size(&p.data) {
                    let natural = Picture::new(p.anchor, p.data.clone(), p.kind, pixels).size;
                    self.place_picture(
                        index,
                        PicturePlace {
                            anchor: p.anchor,
                            offset: p.offset,
                            size: natural,
                        },
                    );
                }
            }
            PictureAction::BringToFront | PictureAction::SendToBack => {
                let front = action == PictureAction::BringToFront;
                let mut moved_to = index;
                self.with_snapshot(|app| {
                    let list = &mut app.engine.formatting_mut(sheet).pictures;
                    let p = list.remove(index);
                    if front {
                        list.push(p);
                        moved_to = list.len() - 1;
                    } else {
                        list.insert(0, p);
                        moved_to = 0;
                    }
                    moved_to != index
                });
                self.selected_picture = Some(moved_to);
            }
            PictureAction::AltText => {
                self.alt_text_editor = Some(AltTextEditor {
                    index,
                    text: p.description,
                });
            }
        }
    }

    pub(super) fn show_alt_text_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.alt_text_editor.take() else {
            return;
        };
        let mut keep = true;
        let mut save = false;
        egui::Window::new("Alt Text")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Describe the picture for people who use a screen reader:");
                ui.add(
                    egui::TextEdit::multiline(&mut editor.text)
                        .desired_rows(3)
                        .desired_width(320.0),
                );
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        save = true;
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
        if save {
            let sheet = self.current_sheet;
            let text = editor.text.trim().to_string();
            self.with_snapshot(|app| {
                match app
                    .engine
                    .formatting_mut(sheet)
                    .pictures
                    .get_mut(editor.index)
                {
                    Some(p) if p.description != text => {
                        p.description = text;
                        true
                    }
                    _ => false,
                }
            });
        }
        if keep {
            self.alt_text_editor = Some(editor);
        }
    }
}
