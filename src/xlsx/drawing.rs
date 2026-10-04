//! Pictures in a sheet's drawing part (`xl/drawings/drawing1.xml`):
//! `<xdr:twoCellAnchor><xdr:from>` cell and offset, the size from
//! `<a:ext cx cy>`, and `<a:blip r:embed>` naming the image file.

use super::styles::attr;
use crate::cell::{CellCoord, MAX_COL, MAX_ROW};
use crate::format::picture::{POINTS_PER_PIXEL, Picture, PictureKind};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::sync::Arc;

pub(super) const EMU_PER_PIXEL: f64 = 9525.0;
/// Grid points per pixel down a row (Excel's 20px row is 22 points).
pub(super) const ROW_POINTS_PER_PIXEL: f32 = 1.1;

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Marker {
    pub col: u32,
    pub col_off: i64,
    pub row: u32,
    pub row_off: i64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct DrawnPicture {
    pub from: Marker,
    /// Width and height in EMU
    pub ext: Option<(i64, i64)>,
    /// Relationship id of the image
    pub embed: String,
    pub description: String,
}

/// The pictures in a drawing part. Charts and shapes are skipped.
pub(super) fn parse_drawing(xml: &str) -> Vec<DrawnPicture> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current: Option<DrawnPicture> = None;
    let mut is_pic = false;
    let mut in_from = false;
    let mut field: Option<u8> = None;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" => {
                    current = Some(DrawnPicture::default());
                    is_pic = false;
                }
                b"from" => in_from = true,
                name @ (b"col" | b"colOff" | b"row" | b"rowOff") if in_from => {
                    field = Some(match name {
                        b"col" => 0,
                        b"colOff" => 1,
                        b"row" => 2,
                        _ => 3,
                    });
                    text.clear();
                }
                // An absolute position: offsets from A1.
                b"pos" => {
                    if let Some(c) = &mut current {
                        c.from.col_off = attr(&e, b"x").and_then(|v| v.parse().ok()).unwrap_or(0);
                        c.from.row_off = attr(&e, b"y").and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                }
                // xdr:ext (one-cell anchors) or a:ext in the picture's
                // transform; extension lists' a:ext have no size.
                b"ext" => {
                    let size = (attr(&e, b"cx"), attr(&e, b"cy"));
                    if let (Some(c), (Some(cx), Some(cy))) = (&mut current, size) {
                        if c.ext.is_none() {
                            c.ext = cx.parse().ok().zip(cy.parse().ok());
                        }
                    }
                }
                b"pic" => is_pic = true,
                b"cNvPr" if is_pic => {
                    if let Some(c) = &mut current {
                        c.description = attr(&e, b"descr").unwrap_or_default();
                    }
                }
                b"blip" => {
                    if let Some(c) = &mut current {
                        c.embed = attr(&e, b"embed").unwrap_or_default();
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) if field.is_some() => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"col" | b"colOff" | b"row" | b"rowOff" => {
                    if let (Some(f), Some(c)) = (field.take(), &mut current) {
                        let v = text.trim();
                        match f {
                            0 => c.from.col = v.parse().unwrap_or(0),
                            1 => c.from.col_off = v.parse().unwrap_or(0),
                            2 => c.from.row = v.parse().unwrap_or(0),
                            _ => c.from.row_off = v.parse().unwrap_or(0),
                        }
                    }
                }
                b"from" => in_from = false,
                b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" => {
                    if let Some(c) = current.take() {
                        if is_pic && !c.embed.is_empty() {
                            out.push(c);
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// A picture from its drawing entry and image file; `None` if the file
/// isn't an image RustSheet can show.
pub(super) fn to_picture(d: DrawnPicture, bytes: Vec<u8>) -> Option<Picture> {
    let kind = PictureKind::detect(&bytes)?;
    let (w, h) = match d.ext {
        Some((cx, cy)) if cx > 0 && cy > 0 => {
            (cx as f64 / EMU_PER_PIXEL, cy as f64 / EMU_PER_PIXEL)
        }
        _ => {
            let image = rust_xlsxwriter::Image::new_from_buffer(&bytes).ok()?;
            (image.width(), image.height())
        }
    };
    let px = |emu: i64| (emu.max(0) as f64 / EMU_PER_PIXEL) as f32;
    Some(Picture {
        anchor: CellCoord::new(d.from.row.min(MAX_ROW), d.from.col.min(MAX_COL)),
        offset: (
            px(d.from.col_off) * POINTS_PER_PIXEL,
            px(d.from.row_off) * ROW_POINTS_PER_PIXEL,
        ),
        size: (w as f32 * POINTS_PER_PIXEL, h as f32 * POINTS_PER_PIXEL),
        data: Arc::from(bytes),
        kind,
        description: d.description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_picture_anchors() {
        let xml = r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <xdr:twoCellAnchor editAs="oneCell">
    <xdr:from><xdr:col>2</xdr:col><xdr:colOff>95250</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>19050</xdr:rowOff></xdr:from>
    <xdr:to><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>14</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
    <xdr:pic>
      <xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1" descr="Company logo"/><xdr:cNvPicPr/></xdr:nvPicPr>
      <xdr:blipFill><a:blip r:embed="rId1"><a:extLst><a:ext uri="{28A0092B}"/></a:extLst></a:blip></xdr:blipFill>
      <xdr:spPr><a:xfrm><a:off x="1" y="2"/><a:ext cx="1905000" cy="952500"/></a:xfrm></xdr:spPr>
    </xdr:pic>
    <xdr:clientData/>
  </xdr:twoCellAnchor>
  <xdr:twoCellAnchor>
    <xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
    <xdr:graphicFrame><a:graphic><a:graphicData><c:chart r:id="rId2"/></a:graphicData></a:graphic></xdr:graphicFrame>
  </xdr:twoCellAnchor>
</xdr:wsDr>"#;
        let pics = parse_drawing(xml);
        assert_eq!(pics.len(), 1, "charts are skipped");
        let p = &pics[0];
        assert_eq!(
            p.from,
            Marker {
                col: 2,
                col_off: 95250,
                row: 4,
                row_off: 19050
            }
        );
        assert_eq!(p.ext, Some((1905000, 952500)));
        assert_eq!(p.embed, "rId1");
        assert_eq!(p.description, "Company logo");

        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let picture = to_picture(p.clone(), png).unwrap();
        assert_eq!(picture.anchor, CellCoord::new(4, 2));
        assert_eq!(picture.offset, (12.5, 2.2));
        assert_eq!(picture.size, (250.0, 125.0));
    }
}
