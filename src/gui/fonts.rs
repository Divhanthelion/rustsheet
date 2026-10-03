//! Installed fonts for cells that name one (Arial, Calibri, Times New Roman...).
//!
//! The installed fonts are scanned once, in the background, at startup.
//! When the grid meets a font it doesn't have yet, it notes the name; the
//! app then loads that family's regular, bold, italic and bold-italic faces
//! and gives them to egui. egui applies new fonts on the next frame, so a
//! family is only drawn with from the frame after it was registered. Until
//! then, and for fonts that aren't installed, cells use the UI font.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

/// egui font keys for the faces of one family.
#[derive(Clone, Debug)]
struct Faces {
    regular: String,
    bold: Option<String>,
    italic: Option<String>,
    bold_italic: Option<String>,
}

/// How to draw text in a named font.
pub struct Resolved {
    pub family: FontFamily,
    /// The face is really bold, so don't fake it
    pub bold: bool,
    /// The face is really italic, so don't slant it
    pub italic: bool,
}

pub struct FontLibrary {
    /// Filled in by the background scan
    db: Arc<Mutex<Option<fontdb::Database>>>,
    /// Installed family names, sorted, once the scan is done
    families: Vec<String>,
    /// Usable now, by lowercase family name
    ready: HashMap<String, Faces>,
    /// Registered this frame; usable next frame
    pending: HashMap<String, Faces>,
    /// Not installed (don't look again)
    missing: BTreeSet<String>,
    /// Asked for by the grid this frame
    wanted: RefCell<BTreeSet<String>>,
    defs: Option<FontDefinitions>,
}

/// Fonts listed first in the picker.
const COMMON: [&str; 10] = [
    "Aptos",
    "Arial",
    "Calibri",
    "Cambria",
    "Consolas",
    "Courier New",
    "Georgia",
    "Segoe UI",
    "Times New Roman",
    "Verdana",
];

impl FontLibrary {
    /// No installed fonts (tests, or before a scan).
    pub fn empty() -> Self {
        Self {
            db: Arc::new(Mutex::new(None)),
            families: Vec::new(),
            ready: HashMap::new(),
            pending: HashMap::new(),
            missing: BTreeSet::new(),
            wanted: RefCell::new(BTreeSet::new()),
            defs: None,
        }
    }

    /// Start scanning installed fonts on a background thread.
    pub fn scan_in_background() -> Self {
        let lib = Self::empty();
        let slot = lib.db.clone();
        std::thread::spawn(move || {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            if let Ok(mut s) = slot.lock() {
                *s = Some(db);
            }
        });
        lib
    }

    /// Installed family names for the font picker: common ones first.
    pub fn families(&mut self) -> &[String] {
        if self.families.is_empty() {
            if let Ok(db) = self.db.lock() {
                if let Some(db) = db.as_ref() {
                    let mut names: BTreeSet<String> = db
                        .faces()
                        .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
                        .filter(|n| !n.starts_with('@'))
                        .collect();
                    let mut list: Vec<String> = COMMON
                        .iter()
                        .filter(|c| names.remove(**c))
                        .map(|c| c.to_string())
                        .collect();
                    list.extend(names);
                    self.families = list;
                }
            }
        }
        &self.families
    }

    /// The face to draw with, if `name` is installed and ready.
    pub fn resolve(&self, name: &str, bold: bool, italic: bool) -> Option<Resolved> {
        let key = name.to_lowercase();
        let Some(faces) = self.ready.get(&key) else {
            if !self.missing.contains(&key) && !self.pending.contains_key(&key) {
                self.wanted.borrow_mut().insert(name.to_string());
            }
            return None;
        };
        let pick = |k: &Option<String>| k.clone().map(|k| FontFamily::Name(k.into()));
        let (family, real_bold, real_italic) = match (bold, italic) {
            (true, true) => match pick(&faces.bold_italic) {
                Some(f) => (f, true, true),
                None => match pick(&faces.bold) {
                    Some(f) => (f, true, false),
                    None => (FontFamily::Name(faces.regular.clone().into()), false, false),
                },
            },
            (true, false) => pick(&faces.bold).map_or(
                (FontFamily::Name(faces.regular.clone().into()), false, false),
                |f| (f, true, false),
            ),
            (false, true) => pick(&faces.italic).map_or(
                (FontFamily::Name(faces.regular.clone().into()), false, false),
                |f| (f, false, true),
            ),
            (false, false) => (FontFamily::Name(faces.regular.clone().into()), false, false),
        };
        Some(Resolved {
            family,
            bold: real_bold,
            italic: real_italic,
        })
    }

    /// Call once per frame, before drawing: fonts registered last frame are
    /// usable now, and fonts the grid asked for are loaded.
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        if !self.pending.is_empty() {
            // Cells drawn last frame in the fallback font redraw in the real one.
            ctx.request_repaint();
        }
        for (k, v) in self.pending.drain() {
            self.ready.insert(k, v);
        }
        let wanted: Vec<String> = std::mem::take(&mut *self.wanted.borrow_mut())
            .into_iter()
            .collect();
        if !wanted.is_empty() {
            self.load(ctx, &wanted);
        }
    }

    /// Load these families (if installed) and give them to egui.
    pub fn load(&mut self, ctx: &egui::Context, names: &[String]) {
        let Ok(guard) = self.db.lock() else {
            return;
        };
        let Some(db) = guard.as_ref() else {
            // Still scanning: ask again shortly, even if the app is idle.
            for n in names {
                self.wanted.borrow_mut().insert(n.clone());
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        };
        let defs = self.defs.get_or_insert_with(FontDefinitions::default);
        let fallbacks = defs
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        let mut added = false;
        for name in names {
            let key = name.to_lowercase();
            if self.ready.contains_key(&key)
                || self.pending.contains_key(&key)
                || self.missing.contains(&key)
            {
                continue;
            }
            let family = [fontdb::Family::Name(name)];
            let face = |weight: fontdb::Weight, style: fontdb::Style| {
                db.query(&fontdb::Query {
                    families: &family,
                    weight,
                    style,
                    stretch: fontdb::Stretch::Normal,
                })
            };
            let Some(regular_id) = face(fontdb::Weight::NORMAL, fontdb::Style::Normal) else {
                self.missing.insert(key);
                continue;
            };
            // A query falls back to the nearest face; only keep real variants.
            let variant = |w: fontdb::Weight, s: fontdb::Style| {
                face(w, s).filter(|id| {
                    *id != regular_id
                        && db.face(*id).is_some_and(|f| {
                            (w == fontdb::Weight::NORMAL || f.weight.0 >= 600)
                                && (s == fontdb::Style::Normal || f.style != fontdb::Style::Normal)
                        })
                })
            };
            let mut register = |id: fontdb::ID, suffix: &str| -> Option<String> {
                let key = format!("cell-font:{key}{suffix}");
                let data = db.with_face_data(id, |data, index| {
                    let mut font = FontData::from_owned(data.to_vec());
                    font.index = index;
                    font
                })?;
                defs.font_data.insert(key.clone(), Arc::new(data));
                let mut chain = vec![key.clone()];
                chain.extend(fallbacks.iter().cloned());
                defs.families
                    .insert(FontFamily::Name(key.clone().into()), chain);
                Some(key)
            };
            let Some(regular) = register(regular_id, "") else {
                self.missing.insert(key);
                continue;
            };
            let bold = variant(fontdb::Weight::BOLD, fontdb::Style::Normal)
                .and_then(|id| register(id, "#b"));
            let italic = variant(fontdb::Weight::NORMAL, fontdb::Style::Italic)
                .and_then(|id| register(id, "#i"));
            let bold_italic = variant(fontdb::Weight::BOLD, fontdb::Style::Italic)
                .and_then(|id| register(id, "#bi"));
            self.pending.insert(
                key,
                Faces {
                    regular,
                    bold,
                    italic,
                    bold_italic,
                },
            );
            added = true;
        }
        if added {
            ctx.set_fonts(defs.clone());
            // New fonts apply next frame; make sure there is one.
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fonts_fall_back_and_are_requested_once() {
        let lib = FontLibrary::empty();
        assert!(lib.resolve("Arial", false, false).is_none());
        assert!(lib.resolve("Arial", true, false).is_none());
        assert_eq!(lib.wanted.borrow().len(), 1);
    }

    #[test]
    fn ready_fonts_pick_real_bold_and_italic_faces() {
        let mut lib = FontLibrary::empty();
        lib.ready.insert(
            "demo".into(),
            Faces {
                regular: "r".into(),
                bold: Some("b".into()),
                italic: None,
                bold_italic: None,
            },
        );
        let bold = lib.resolve("Demo", true, false).unwrap();
        assert!(bold.bold && !bold.italic);
        assert_eq!(bold.family, FontFamily::Name("b".into()));
        // No italic face: draw the regular face and slant it.
        let italic = lib.resolve("demo", false, true).unwrap();
        assert!(!italic.italic);
        let both = lib.resolve("demo", true, true).unwrap();
        assert!(both.bold && !both.italic);
    }
}
