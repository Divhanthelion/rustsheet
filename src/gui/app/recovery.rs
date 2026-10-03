//! Crash safety: unsaved work is autosaved to the data folder every minute,
//! and a session that ends without closing normally leaves its autosave
//! behind for the next launch to offer back. Each session also writes a
//! heartbeat, so a second window doesn't mistake a running one for a crash.

use super::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often unsaved changes are written to the recovery folder.
pub(super) const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);
/// How often a session marks itself alive.
const HEARTBEAT_EVERY: Duration = Duration::from_secs(20);
/// A session silent this long has ended without cleaning up.
const STALE_AFTER: u64 = 90;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(super) struct RecoveryMeta {
    /// Where the workbook was last saved, if ever
    pub original: Option<PathBuf>,
    /// Unix seconds of the autosave
    pub saved_at: u64,
    /// Unix seconds the session was last alive
    pub heartbeat: u64,
}

/// This session's recovery files.
pub(super) struct Recovery {
    xlsx: PathBuf,
    meta: PathBuf,
    last_autosave: Instant,
    last_heartbeat: Instant,
    /// Something is saved in the recovery folder for this session
    pub has_autosave: bool,
}

/// A recovery file left by a session that crashed.
#[derive(Clone)]
pub(super) struct Recoverable {
    pub xlsx: PathBuf,
    pub meta_path: PathBuf,
    pub meta: RecoveryMeta,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn recovery_dir() -> Option<PathBuf> {
    let dir = crate::gui::settings::data_dir()?.join("Recovery");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl Recovery {
    pub(super) fn new() -> Option<Self> {
        let dir = recovery_dir()?;
        let id = format!("{}-{}", std::process::id(), now());
        Some(Self {
            xlsx: dir.join(format!("{id}.xlsx")),
            meta: dir.join(format!("{id}.json")),
            last_autosave: Instant::now(),
            last_heartbeat: Instant::now() - HEARTBEAT_EVERY,
            has_autosave: false,
        })
    }

    fn write_meta(&self, original: Option<&Path>) {
        let saved_at = std::fs::read_to_string(&self.meta)
            .ok()
            .and_then(|s| serde_json::from_str::<RecoveryMeta>(&s).ok())
            .map_or(0, |m| m.saved_at);
        let meta = RecoveryMeta {
            original: original.map(Path::to_path_buf),
            saved_at: if self.has_autosave {
                saved_at.max(1)
            } else {
                0
            },
            heartbeat: now(),
        };
        if let Ok(json) = serde_json::to_string(&meta) {
            let _ = std::fs::write(&self.meta, json);
        }
    }

    /// Whether `path` is in the recovery folder.
    pub(super) fn holds(&self, path: &Path) -> bool {
        self.xlsx.parent().is_some_and(|dir| path.starts_with(dir))
    }

    /// Remove this session's files (after a save, or on a clean exit).
    pub(super) fn clear(&mut self) {
        let _ = std::fs::remove_file(&self.xlsx);
        let _ = std::fs::remove_file(&self.meta);
        self.has_autosave = false;
    }
}

/// Autosaves from crashed sessions, newest first.
pub(super) fn find_recoverable() -> Vec<Recoverable> {
    match recovery_dir() {
        Some(dir) => find_recoverable_in(&dir),
        None => Vec::new(),
    }
}

pub(super) fn find_recoverable_in(dir: &Path) -> Vec<Recoverable> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Recoverable> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|meta_path| {
            let meta: RecoveryMeta =
                serde_json::from_str(&std::fs::read_to_string(&meta_path).ok()?).ok()?;
            let xlsx = meta_path.with_extension("xlsx");
            let stale = now().saturating_sub(meta.heartbeat) > STALE_AFTER;
            if !stale {
                return None; // another window is still running
            }
            if !xlsx.exists() || meta.saved_at == 0 {
                // A crash before any autosave: nothing to offer.
                let _ = std::fs::remove_file(&meta_path);
                return None;
            }
            Some(Recoverable {
                xlsx,
                meta_path,
                meta,
            })
        })
        .collect();
    found.sort_by_key(|r| std::cmp::Reverse(r.meta.saved_at));
    found
}

impl SpreadsheetApp {
    /// Called every frame: autosave if due, and keep the heartbeat fresh.
    pub(super) fn tick_recovery(&mut self, ctx: &egui::Context) {
        let Some(rec) = &self.recovery else {
            return;
        };
        let autosave_due = self.modified && rec.last_autosave.elapsed() >= AUTOSAVE_EVERY;
        let heartbeat_due = rec.last_heartbeat.elapsed() >= HEARTBEAT_EVERY;
        if autosave_due {
            self.autosave();
        } else if !self.modified && rec.has_autosave {
            // Saved (or discarded) since the last autosave.
            if let Some(rec) = &mut self.recovery {
                rec.clear();
            }
        }
        if heartbeat_due {
            if let Some(rec) = &mut self.recovery {
                rec.last_heartbeat = Instant::now();
                rec.write_meta(self.current_file.as_deref());
            }
        }
        // Wake up for the next heartbeat even when idle.
        ctx.request_repaint_after(HEARTBEAT_EVERY);
    }

    /// Write the workbook to this session's recovery file now.
    pub(super) fn autosave(&mut self) {
        let Some(path) = self.recovery.as_ref().map(|r| r.xlsx.clone()) else {
            return;
        };
        let ok = self.write_xlsx(&path).is_ok();
        if let Some(rec) = &mut self.recovery {
            rec.last_autosave = Instant::now();
            if ok {
                rec.has_autosave = true;
                let meta = RecoveryMeta {
                    original: self.current_file.clone(),
                    saved_at: now(),
                    heartbeat: now(),
                };
                if let Ok(json) = serde_json::to_string(&meta) {
                    let _ = std::fs::write(&rec.meta, json);
                }
            }
        }
    }

    /// On a normal exit: nothing to recover.
    pub(super) fn end_recovery(&mut self) {
        if let Some(rec) = &mut self.recovery {
            rec.clear();
        }
    }

    /// The startup prompt for work left by a crashed session.
    pub(super) fn show_recovery_prompt(&mut self, ctx: &egui::Context) {
        if self.recoverable.is_empty() {
            return;
        }
        let mut choice: Option<(usize, bool)> = None;
        let mut discard_all = false;
        egui::Window::new("Recover unsaved work")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("RustSheet didn't close normally last time. These workbooks had unsaved changes:");
                ui.add_space(6.0);
                for (i, r) in self.recoverable.iter().enumerate() {
                    let name = r
                        .meta
                        .original
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
                    let ago = now().saturating_sub(r.meta.saved_at);
                    let when = match ago {
                        0..=119 => "just now".to_string(),
                        120..=7199 => format!("{} minutes ago", ago / 60),
                        7200..=172_799 => format!("{} hours ago", ago / 3600),
                        _ => format!("{} days ago", ago / 86_400),
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).strong());
                        ui.label(RichText::new(format!("autosaved {when}")).weak());
                        if ui.button("Recover").clicked() {
                            choice = Some((i, true));
                        }
                        if ui.button("Discard").clicked() {
                            choice = Some((i, false));
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button("Discard all").clicked() {
                    discard_all = true;
                }
            });
        if discard_all {
            for r in std::mem::take(&mut self.recoverable) {
                let _ = std::fs::remove_file(&r.xlsx);
                let _ = std::fs::remove_file(&r.meta_path);
            }
            return;
        }
        let Some((i, recover)) = choice else {
            return;
        };
        let r = self.recoverable.remove(i);
        if recover && self.confirm_discard() {
            self.load_file(&r.xlsx);
            // Recovered work is unsaved: Save asks where, offering the old name.
            self.suggested_name = r
                .meta
                .original
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned());
            self.recovered_from = r.meta.original.clone();
            self.current_file = None;
            self.modified = true;
            self.set_status("Recovered. Save it to keep it.");
        }
        let _ = std::fs::remove_file(&r.xlsx);
        let _ = std::fs::remove_file(&r.meta_path);
    }
}
