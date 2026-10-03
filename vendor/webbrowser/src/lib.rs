//! No-op `webbrowser::open` for egui-winit's link handling.
//!
//! RustSheet has no hyperlinks. The real crate (and any shell call) would put
//! process-launch APIs in the exe, which the Windows App Certification Kit's
//! "Blocked executables" test flags. If links are added later, open them with
//! an explicit, reviewed call instead of restoring this dependency.

use std::io;

/// Always fails: opening URLs is not supported in this build.
pub fn open(_url: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening URLs is not supported in this build",
    ))
}
