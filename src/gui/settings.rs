//! Preferences kept between sessions (theme, time zone, recent files), and
//! the folder RustSheet keeps its own files in (settings, crash recovery).
//!
//! The folder is %LOCALAPPDATA%\RustSheet on Windows (inside an MSIX package
//! Windows redirects this to the package's private storage), Application
//! Support on macOS, and $XDG_CONFIG_HOME/rustsheet elsewhere.
//! `RUSTSHEET_DATA_DIR` overrides it, for tests and screenshots.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 10;

/// Light, dark, or follow the system setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// The time zone NOW() and TODAY() report in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimeZoneChoice {
    /// The PC's own, daylight saving time included
    #[default]
    Windows,
    Utc,
    /// Minutes ahead of UTC
    Fixed(i32),
}

/// Fixed offsets, in minutes: UTC-12:00 to UTC+14:00.
const FIXED_MIN: i32 = -12 * 60;
const FIXED_MAX: i32 = 14 * 60;

impl TimeZoneChoice {
    /// The fixed offsets on offer, by half hours.
    pub fn fixed_offsets() -> impl Iterator<Item = i32> {
        (FIXED_MIN..=FIXED_MAX).step_by(30)
    }

    /// Minutes to add to UTC now. A fixed offset out of range, as from an
    /// edited settings file, is clamped.
    pub fn offset_minutes(self) -> i32 {
        match self {
            Self::Windows => windows_offset_minutes(),
            Self::Utc => 0,
            Self::Fixed(m) => m.clamp(FIXED_MIN, FIXED_MAX),
        }
    }
}

/// `UTC+05:30`, `UTC-03:00`, as Windows writes offsets.
pub fn offset_label(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let m = minutes.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", m / 60, m % 60)
}

/// The PC's current offset from UTC in minutes, daylight saving time
/// included. One cheap call, read fresh each time so a change of clocks or
/// time zone mid-session shows; UTC if Windows can't say.
#[cfg(windows)]
fn windows_offset_minutes() -> i32 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    // TIME_ZONE_ID_STANDARD and _DAYLIGHT; 0 means the zone has no DST.
    const STANDARD: u32 = 1;
    const DAYLIGHT: u32 = 2;
    // SAFETY: TIME_ZONE_INFORMATION is plain data, which the call fills in.
    let (id, info) = unsafe {
        let mut info: TIME_ZONE_INFORMATION = std::mem::zeroed();
        (GetTimeZoneInformation(&mut info), info)
    };
    let bias = match id {
        STANDARD => info.Bias + info.StandardBias,
        DAYLIGHT => info.Bias + info.DaylightBias,
        0 => info.Bias,
        _ => return 0,
    };
    // Windows counts the other way: UTC = local time + bias.
    -bias
}

#[cfg(not(windows))]
fn windows_offset_minutes() -> i32 {
    0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// For NOW() and TODAY()
    pub time_zone: TimeZoneChoice,
    /// Most recent first
    pub recent_files: Vec<PathBuf>,
    /// Where the settings were loaded from; not saved
    #[serde(skip)]
    path: Option<PathBuf>,
}

/// RustSheet's own data folder, created on demand.
pub fn data_dir() -> Option<PathBuf> {
    let dir = if let Some(dir) = std::env::var_os("RUSTSHEET_DATA_DIR") {
        PathBuf::from(dir)
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("RustSheet")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/RustSheet")
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("rustsheet")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config/rustsheet")
    };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

impl Settings {
    /// Load from the data folder; defaults if missing or unreadable.
    pub fn load() -> Self {
        let path = data_dir().map(|d| d.join("settings.json"));
        let mut settings = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default();
        settings.path = path;
        settings
    }

    /// Settings that are never written to disk (tests).
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Ok(json) = serde_json::to_string_pretty(self) {
            // Write then rename, so a crash can't leave half a file.
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    /// Put `path` at the top of the recent list and save.
    pub fn add_recent(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        // canonicalize adds \\?\ on Windows; keep paths readable.
        let path = strip_verbatim(path);
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT);
        self.save();
    }
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_are_deduplicated_and_capped() {
        let mut s = Settings::in_memory();
        for i in 0..12 {
            s.add_recent(Path::new(&format!("C:/nowhere/file{i}.xlsx")));
        }
        s.add_recent(Path::new("C:/nowhere/file5.xlsx"));
        assert_eq!(s.recent_files.len(), MAX_RECENT);
        assert_eq!(s.recent_files[0], PathBuf::from("C:/nowhere/file5.xlsx"));
        assert_eq!(
            s.recent_files
                .iter()
                .filter(|p| p.ends_with("file5.xlsx"))
                .count(),
            1
        );
    }

    #[test]
    fn settings_round_trip_as_json() {
        let s = Settings {
            theme: ThemeChoice::Dark,
            time_zone: TimeZoneChoice::Fixed(330),
            recent_files: vec![PathBuf::from("a.xlsx")],
            path: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, ThemeChoice::Dark);
        assert_eq!(back.time_zone, TimeZoneChoice::Fixed(330));
        assert_eq!(back.recent_files, s.recent_files);
        // Unknown or missing fields fall back to defaults.
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.theme, ThemeChoice::System);
        assert_eq!(old.time_zone, TimeZoneChoice::Windows);
    }

    #[test]
    fn time_zone_choices_round_trip_and_give_offsets() {
        for choice in [
            TimeZoneChoice::Windows,
            TimeZoneChoice::Utc,
            TimeZoneChoice::Fixed(-570),
        ] {
            let s = Settings {
                time_zone: choice,
                ..Settings::in_memory()
            };
            let json = serde_json::to_string_pretty(&s).unwrap();
            let back: Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(back.time_zone, choice, "{json}");
        }
        assert_eq!(TimeZoneChoice::Utc.offset_minutes(), 0);
        assert_eq!(TimeZoneChoice::Fixed(-570).offset_minutes(), -570);
        // Edited files can't push past UTC-12:00 or UTC+14:00.
        assert_eq!(TimeZoneChoice::Fixed(5000).offset_minutes(), 840);
        assert_eq!(TimeZoneChoice::Fixed(-5000).offset_minutes(), -720);
        // Windows' own is a real zone, whole quarter hours from UTC.
        let windows = TimeZoneChoice::Windows.offset_minutes();
        assert!((-720..=840).contains(&windows) && windows % 15 == 0);

        let offsets: Vec<i32> = TimeZoneChoice::fixed_offsets().collect();
        assert_eq!((offsets.len(), offsets[0], offsets[52]), (53, -720, 840));
        assert_eq!(offset_label(-720), "UTC-12:00");
        assert_eq!(offset_label(330), "UTC+05:30");
        assert_eq!(offset_label(-30), "UTC-00:30");
        assert_eq!(offset_label(0), "UTC+00:00");
    }
}
