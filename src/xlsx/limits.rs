//! What an untrusted package may make the readers do. A damaged or hostile
//! file loses the damaged part, or fails with an error; it never takes the
//! app down or exhausts memory through a size field it declares.

use crate::cell::{CellCoord, CellRange, MAX_COL, MAX_ROW};
use std::io::{Read, Seek};

/// Largest part read whole into memory. A few kilobytes of deflate can claim
/// gigabytes, so the read stops here whatever the part's header says.
pub(super) const MAX_PART_BYTES: u64 = 1 << 30;

/// Longest number format code Excel accepts.
pub(super) const MAX_FORMAT_CODE_LEN: usize = 255;

/// Most bytes one part of a package may declare it unpacks to. calamine
/// inflates sharedStrings.xml and sheet XML whole, with no cap of its own.
pub(super) const MAX_ENTRY_BYTES: u64 = 512 << 20;
/// Most bytes all parts together may declare.
pub(super) const MAX_PACKAGE_BYTES: u64 = 2 << 30;
/// Most parts a package may hold.
pub(super) const MAX_ENTRIES: usize = 100_000;

/// Refuse a package whose central directory declares more than the readers
/// may inflate: a part past [`MAX_ENTRY_BYTES`], all parts past
/// [`MAX_PACKAGE_BYTES`], or more than [`MAX_ENTRIES`] parts. Only the
/// directory and local headers are read. A package the zip reader can't
/// open, or a part it can't find, passes: the readers that follow fail on
/// it with their own error.
pub(super) fn check_declared_sizes<R: Read + Seek>(reader: R) -> Result<(), String> {
    let Ok(mut zip) = zip::ZipArchive::new(reader) else {
        return Ok(());
    };
    if zip.len() > MAX_ENTRIES {
        return Err(format!(
            "it has {} parts, and at most {MAX_ENTRIES} are read",
            zip.len()
        ));
    }
    let mut total = 0u64;
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index_raw(i) else {
            continue;
        };
        let size = entry.size();
        if size > MAX_ENTRY_BYTES {
            return Err(format!(
                "part '{}' would unpack to {} MB, and parts are read up to {} MB",
                entry.name(),
                size >> 20,
                MAX_ENTRY_BYTES >> 20
            ));
        }
        total = total.saturating_add(size);
        if total > MAX_PACKAGE_BYTES {
            return Err(format!(
                "its parts would unpack to more than {} GB",
                MAX_PACKAGE_BYTES >> 30
            ));
        }
    }
    Ok(())
}

/// The whole of part `name`, or `None` if it is missing, unreadable or
/// larger than [`MAX_PART_BYTES`].
pub(super) fn read_part<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<Vec<u8>> {
    let file = zip.by_name(name).ok()?;
    if file.size() > MAX_PART_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    file.take(MAX_PART_BYTES + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= MAX_PART_BYTES).then_some(buf)
}

/// Whether a coordinate is on an Excel-sized sheet.
pub(super) fn on_sheet(c: CellCoord) -> bool {
    c.row <= MAX_ROW && c.col <= MAX_COL
}

/// A cell reference ("B7", "$B$7") on the sheet.
pub(super) fn cell(s: &str) -> Option<CellCoord> {
    CellCoord::from_a1(s).filter(|&c| on_sheet(c))
}

/// A range ("A1:C9") or single cell on the sheet.
pub(super) fn range(s: &str) -> Option<CellRange> {
    CellRange::from_a1(s).filter(|&r| fits(r))
}

/// Whether a range is on the sheet with its corners in order. Ranges
/// deserialized from RustSheet's own JSON parts skip `CellRange::new`.
pub(super) fn fits(r: CellRange) -> bool {
    r.start.row <= r.end.row && r.start.col <= r.end.col && on_sheet(r.end)
}

/// A size read from the file (a width, a height), if it is finite and not
/// negative, capped at `max`.
pub(super) fn size(v: f64, max: f64) -> Option<f64> {
    (v.is_finite() && v >= 0.0).then_some(v.min(max))
}

/// Run `f`, turning a panic in it, or in calamine, quick-xml or zip under
/// it, into an error naming `what`. The readers' own code shouldn't panic;
/// this keeps a bug in a dependency from closing the app over one file.
pub(super) fn contain<T>(what: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|panic| {
        let detail = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        format!("{what} is damaged ({detail})")
    })
}
