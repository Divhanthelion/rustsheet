//! Pictures placed on a sheet. A picture hangs from a cell: it moves with
//! that cell when rows or columns are inserted or deleted above or left of
//! it, and keeps its size.

use crate::cell::{CellCoord, LineEdit};
use std::sync::Arc;

/// Grid points per image pixel. Columns use 1.25 (the default 64px Excel
/// column is 80 points wide), so pictures line up with columns.
pub const POINTS_PER_PIXEL: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureKind {
    Png,
    Jpeg,
    Gif,
    Bmp,
}

impl PictureKind {
    /// The format of an image file, from its first bytes.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0x89, b'P', b'N', b'G', ..] => Some(PictureKind::Png),
            [0xFF, 0xD8, 0xFF, ..] => Some(PictureKind::Jpeg),
            [b'G', b'I', b'F', b'8', ..] => Some(PictureKind::Gif),
            [b'B', b'M', ..] => Some(PictureKind::Bmp),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// The cell its top-left corner is in
    pub anchor: CellCoord,
    /// From the anchor cell's top-left corner, in grid points
    pub offset: (f32, f32),
    /// Width and height in grid points
    pub size: (f32, f32),
    /// The image file. Shared, so undo snapshots don't copy it.
    pub data: Arc<[u8]>,
    pub kind: PictureKind,
    /// Alt text for screen readers
    pub description: String,
}

impl Picture {
    /// A picture at `anchor` of an image `width` x `height` pixels.
    pub fn new(anchor: CellCoord, data: Arc<[u8]>, kind: PictureKind, pixels: (u32, u32)) -> Self {
        Self {
            anchor,
            offset: (0.0, 0.0),
            size: (
                pixels.0 as f32 * POINTS_PER_PIXEL,
                pixels.1 as f32 * POINTS_PER_PIXEL,
            ),
            data,
            kind,
            description: String::new(),
        }
    }

    /// Scale down to fit `max`, keeping the shape.
    pub fn fit_within(&mut self, max: (f32, f32)) {
        let scale = (max.0 / self.size.0).min(max.1 / self.size.1).min(1.0);
        if scale.is_finite() && scale > 0.0 {
            self.size = (self.size.0 * scale, self.size.1 * scale);
        }
    }
}

/// Follow inserted and deleted rows and columns. A picture whose anchor is
/// deleted moves to the first line after the deletion.
pub fn apply_line_edit(list: &mut [Picture], edit: &LineEdit) {
    for p in list {
        p.anchor = edit
            .map_coord(p.anchor)
            .unwrap_or_else(|| edit.axis.with(p.anchor, edit.at.min(edit.axis.max())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Axis;

    #[test]
    fn detects_formats_and_follows_lines() {
        assert_eq!(PictureKind::detect(b"\x89PNG\r\n"), Some(PictureKind::Png));
        assert_eq!(
            PictureKind::detect(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(PictureKind::Jpeg)
        );
        assert_eq!(PictureKind::detect(b"GIF89a"), Some(PictureKind::Gif));
        assert_eq!(PictureKind::detect(b"hello"), None);

        let data: Arc<[u8]> = Arc::from(&b"\x89PNG"[..]);
        let mut list = vec![Picture::new(
            CellCoord::new(5, 2),
            data,
            PictureKind::Png,
            (400, 200),
        )];
        assert_eq!(list[0].size, (500.0, 250.0));
        list[0].fit_within((250.0, 1000.0));
        assert_eq!(list[0].size, (250.0, 125.0));

        apply_line_edit(&mut list, &LineEdit::insert(Axis::Row, 0, 2));
        assert_eq!(list[0].anchor, CellCoord::new(7, 2));
        apply_line_edit(&mut list, &LineEdit::delete(Axis::Column, 1, 3));
        assert_eq!(
            list[0].anchor,
            CellCoord::new(7, 1),
            "its column was deleted"
        );
    }
}
