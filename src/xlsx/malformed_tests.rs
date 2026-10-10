//! Damaged and hostile packages. Opening one must give an error or lose the
//! damaged part; it must not panic, hang, or size memory from what the file
//! claims. Each test builds its package here: a valid container with broken
//! parts inside, or a container that is itself broken.

use super::reader::XlsxReadError;
use super::tests::TINY_PNG;
use super::{ChartReader, XlsxReader, XlsxWriter};
use crate::calc::{CalcEngine, CellResult, CellValueInput};
use crate::cell::{CellCoord, CellRange, MAX_COL, MAX_ROW};
use crate::chart::ChartDefinition;
use crate::format::SheetFormatting;
use std::io::{Cursor, Write};
use std::path::PathBuf;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A one-sheet package around `sheet_data` (the inside of `<sheetData>`),
/// with `extra` parts added or replacing the defaults.
fn package(sheet_data: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let sheet =
        format!(r#"<worksheet xmlns="{MAIN}"><sheetData>{sheet_data}</sheetData></worksheet>"#);
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec(),
        ),
        (
            "xl/workbook.xml".into(),
            format!(r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#).into_bytes(),
        ),
        (
            "xl/_rels/workbook.xml.rels".into(),
            format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).into_bytes(),
        ),
        ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
    ];
    for (name, bytes) in extra {
        parts.retain(|(n, _)| n != name);
        parts.push((name.to_string(), bytes.to_vec()));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Relationships for a part, pointing `Id` → (`type`, `target`).
fn rels(list: &[(&str, &str, &str)]) -> Vec<u8> {
    let items: String = list
        .iter()
        .map(|(id, kind, target)| {
            format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#)
        })
        .collect();
    format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{items}</Relationships>"#)
        .into_bytes()
}

/// Deleted when dropped, so a failed assertion doesn't leave files behind.
struct TempFile(PathBuf);

impl TempFile {
    fn new(bytes: &[u8]) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustsheet_malformed_{}_{}.xlsx",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Everything opening a file in the app does, in the app's order.
struct Opened {
    open: Result<(), String>,
    sheet_errors: Vec<XlsxReadError>,
    engine: CalcEngine,
    formatting: Result<Vec<(String, SheetFormatting)>, String>,
    charts: Result<Vec<(u32, ChartDefinition)>, String>,
}

impl Opened {
    fn formatting(&self) -> &SheetFormatting {
        &self.formatting.as_ref().unwrap()[0].1
    }
}

fn open(bytes: &[u8]) -> Opened {
    let file = TempFile::new(bytes);
    let mut engine = CalcEngine::new();
    let mut sheet_errors = Vec::new();
    let open = match XlsxReader::open(&file.0) {
        Ok(mut reader) => {
            let names = reader.sheet_names();
            engine.set_sheet_names(names.clone());
            for (i, name) in names.iter().enumerate() {
                if let Err(e) = reader.read_into_engine(name, &mut engine, i as u32) {
                    sheet_errors.push(e);
                }
            }
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    };
    Opened {
        open,
        sheet_errors,
        engine,
        formatting: super::read_formatting(Cursor::new(bytes)),
        charts: ChartReader::read_charts_from_reader(Cursor::new(bytes)).map_err(|e| e.to_string()),
    }
}

fn at(a1: &str) -> CellCoord {
    CellCoord::from_a1(a1).unwrap()
}

/// A workbook RustSheet saved, with a value and a chart.
fn saved_workbook() -> Vec<u8> {
    use crate::chart::{ChartKind, ChartSeries};
    let mut engine = CalcEngine::new();
    engine.set_value(0, at("A1"), CellValueInput::Number(1.0));
    let chart = ChartDefinition::new(ChartKind::Line)
        .with_series(ChartSeries::new(CellRange::from_a1("A1:A2").unwrap()))
        .with_sheet(0);
    let file = TempFile::new(b"");
    let mut writer = XlsxWriter::new();
    writer
        .add_engine_sheet_with_charts("Sheet1", &engine, 0, std::slice::from_ref(&chart))
        .unwrap();
    writer
        .save_with_charts(&file.0, std::slice::from_ref(&chart))
        .unwrap();
    std::fs::read(&file.0).unwrap()
}

#[test]
fn the_test_package_opens() {
    let opened = open(&package(r#"<row r="1"><c r="A1"><v>7</v></c></row>"#, &[]));
    assert_eq!(opened.open, Ok(()));
    assert!(opened.sheet_errors.is_empty());
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(7.0));
    assert!(opened.formatting.is_ok());
}

#[test]
fn truncated_and_garbage_files_are_errors() {
    let whole = saved_workbook();
    for len in [0, 3, 30, whole.len() / 2, whole.len() - 1] {
        let opened = open(&whole[..len]);
        assert!(opened.open.is_err(), "{len} bytes opened");
        assert!(opened.formatting.is_err());
        assert!(opened.charts.is_err());
    }
    let opened = open(&[0x5A; 4096]);
    assert!(opened.open.is_err() && opened.formatting.is_err() && opened.charts.is_err());
}

#[test]
fn ole_files_are_turned_away_before_calamine_parses_them() {
    // calamine reads an OLE header's sector counts and sizes buffers from
    // them; this one claims 4 billion FAT sectors.
    let mut ole = vec![0u8; 4096];
    ole[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    ole[30] = 9;
    ole[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
    let file = TempFile::new(&ole);
    assert!(matches!(
        XlsxReader::open(&file.0),
        Err(XlsxReadError::Unsupported)
    ));
}

#[test]
fn missing_and_wrong_parts_are_errors() {
    // calamine needs the workbook's relationships.
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"not a workbook").unwrap();
    let opened = open(&zip.finish().unwrap().into_inner());
    assert!(opened.open.is_err());
    assert!(opened.formatting.is_err());
    assert_eq!(opened.charts.unwrap().len(), 0);

    // Content types aren't consulted: garbage there still opens.
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><v>1</v></c></row>"#,
        &[("[Content_Types].xml", b"<Types><Override PartName=")],
    ));
    assert_eq!(opened.open, Ok(()));
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(1.0));

    // A workbook whose sheet points at a part that isn't there.
    let opened = open(&package(
        "",
        &[(
            "xl/_rels/workbook.xml.rels",
            &rels(&[("rId1", "worksheet", "worksheets/nowhere.xml")]),
        )],
    ));
    assert!(opened.open.is_err() || !opened.sheet_errors.is_empty());
    assert!(opened.formatting.is_ok());
}

#[test]
fn a_damaged_sheet_keeps_what_came_before_the_damage() {
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>99</v></c></row>"#,
        &[],
    ));
    assert_eq!(opened.open, Ok(()));
    assert_eq!(
        opened.sheet_errors.len(),
        1,
        "a shared string that isn't there"
    );
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(1.0));
}

#[test]
fn damaged_formatting_parts_lose_only_their_formatting() {
    // calamine reads only number formats and doesn't check end tags; the
    // formatting reader does, and leaves the styles out.
    let bold = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts><font/><font><b/></fnt></fonts><cellXfs><xf/><xf fontId="1"/></cellXfs></styleSheet>"#
    );
    let opened = open(&package(
        r#"<row r="1"><c r="A1" s="1"><v>1</v></c></row>"#,
        &[("xl/styles.xml", bold.as_bytes())],
    ));
    assert!(opened.sheet_errors.is_empty());
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(1.0));
    assert_eq!(opened.formatting().get(at("A1")), None);

    // Damage after the cells: the values load, the sheet's formatting doesn't.
    let fixed = bold.replace("</fnt>", "</font>");
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="1"><v>1</v></c></row></sheetData><x></y></worksheet>"#
    );
    let opened = open(&package(
        "",
        &[
            ("xl/styles.xml", fixed.as_bytes()),
            ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ],
    ));
    assert!(opened.sheet_errors.is_empty(), "{:?}", opened.sheet_errors);
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(1.0));
    assert_eq!(opened.formatting(), &SheetFormatting::default());
}

#[test]
fn cells_far_apart_are_not_a_dense_allocation() {
    // calamine's worksheet_range sizes a grid from the used corners: 17
    // billion cells here. Values are streamed instead.
    let bytes = package(
        r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>2</v></c></row>"#,
        &[],
    );
    let opened = open(&bytes);
    assert!(opened.sheet_errors.is_empty());
    assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(1.0));
    assert_eq!(
        opened.engine.get_value(0, CellCoord::new(MAX_ROW, MAX_COL)),
        CellResult::Value(2.0)
    );
    let file = TempFile::new(&bytes);
    let sheet = XlsxReader::open(&file.0)
        .unwrap()
        .read_sheet("Sheet1")
        .unwrap();
    assert_eq!(sheet.cell_count(), 2);
}

#[test]
fn cells_off_the_sheet_are_dropped() {
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><v>1</v></c><c r="XFE1"><v>2</v></c></row><row r="1048577"><c r="A1048577"><v>3</v></c></row>"#,
        &[],
    ));
    assert!(opened.sheet_errors.is_empty());
    assert_eq!(opened.engine.sheet_max_coord(0), Some(at("A1")));

    // A row number past u32: calamine's arithmetic overflows (a panic in
    // debug builds). It must come back as an error or a dropped cell.
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><v>1</v></c><c r="A99999999999999"><v>2</v></c></row>"#,
        &[],
    ));
    assert!(
        opened
            .engine
            .sheet_max_coord(0)
            .is_none_or(|c| c.row <= MAX_ROW)
    );
}

/// `bytes` with the central directory saying part `name` unpacks to
/// `size` bytes. The data stays as it is: a header that lies, as a zip
/// bomb's honest one would read.
fn declare_size(mut bytes: Vec<u8>, name: &str, size: u32) -> Vec<u8> {
    let at = (0..bytes.len() - 46)
        .find(|&i| {
            bytes[i..].starts_with(b"PK\x01\x02")
                && bytes[i + 28..i + 30] == (name.len() as u16).to_le_bytes()
                && bytes[i + 46..].starts_with(name.as_bytes())
        })
        .unwrap_or_else(|| panic!("no directory entry for {name}"));
    bytes[at + 24..at + 28].copy_from_slice(&size.to_le_bytes());
    bytes
}

fn too_large(bytes: &[u8]) -> Option<String> {
    let file = TempFile::new(bytes);
    match XlsxReader::open(&file.0) {
        Err(XlsxReadError::TooLarge(why)) => Some(why),
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => None,
    }
}

#[test]
fn packages_declaring_huge_parts_are_refused_before_calamine_reads_them() {
    let strings = br#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>x</t></si></sst>"#;
    let base = package(
        r#"<row r="1"><c r="A1" t="s"><v>0</v></c></row>"#,
        &[
            ("xl/sharedStrings.xml", strings),
            ("docProps/app.xml", b"<x/>"),
        ],
    );
    assert_eq!(too_large(&base), None);

    // One part claiming 4 GB: calamine would inflate it whole.
    let bomb = declare_size(base.clone(), "xl/sharedStrings.xml", 4_000_000_000);
    let why = too_large(&bomb).expect("a 4 GB part opened");
    assert!(why.contains("xl/sharedStrings.xml"), "{why}");
    let opened = open(&bomb);
    assert!(opened.open.unwrap_err().contains("too large"));

    // At the per-part limit is fine; parts that add up past 2 GB are not.
    let at_limit = declare_size(base.clone(), "docProps/app.xml", 512 << 20);
    assert_eq!(too_large(&at_limit), None);
    let mut many = base;
    for name in [
        "docProps/app.xml",
        "xl/sharedStrings.xml",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
    ] {
        many = declare_size(many, name, 500 << 20);
    }
    assert!(too_large(&many).unwrap().contains("2 GB"));

    // More parts than any workbook needs.
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..=super::limits::MAX_ENTRIES {
        zip.start_file(format!("{i:x}"), stored).unwrap();
    }
    let crowded = zip.finish().unwrap().into_inner();
    assert!(too_large(&crowded).unwrap().contains("parts"));
}

#[test]
fn hostile_shared_formulas_are_refused_before_calamine_expands_them() {
    // calamine would insert an entry for each of 17 billion cells.
    let huge_ref = r#"<row r="1"><c r="A1"><f t="shared" ref="A1:XFD1048576" si="0">B1+1</f><v>5</v></c></row>"#;
    // ...or grow its list of groups to 2^64 entries.
    let huge_index = r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="18446744073709551615">B1+1</f><v>5</v></c></row>"#;
    for sheet in [huge_ref, huge_index] {
        let opened = open(&package(sheet, &[]));
        assert!(
            matches!(opened.sheet_errors[..], [XlsxReadError::Damaged(_)]),
            "{:?}",
            opened.sheet_errors
        );
        // The values still load; only the formulas are left out.
        assert_eq!(opened.engine.get_value(0, at("A1")), CellResult::Value(5.0));
        assert_eq!(opened.engine.get_formula(0, at("A1")), None);
    }
}

#[test]
fn ordinary_shared_formulas_still_expand() {
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A3" si="0">B1*2</f><v>0</v></c><c r="B1"><v>1</v></c></row>
           <row r="2"><c r="A2"><f t="shared" si="0"/><v>0</v></c><c r="B2"><v>2</v></c></row>
           <row r="3"><c r="A3"><f t="shared" si="0"/><v>0</v></c><c r="B3"><v>3</v></c></row>"#,
        &[],
    ));
    assert!(opened.sheet_errors.is_empty(), "{:?}", opened.sheet_errors);
    assert_eq!(
        opened.engine.get_formula(0, at("A3")).as_deref(),
        Some("=B3*2")
    );
    assert_eq!(opened.engine.get_value(0, at("A3")), CellResult::Value(6.0));
}

#[test]
fn unreadable_formulas_keep_their_cached_value_or_their_text() {
    // Trailing junk once parsed as its first part; now the cell keeps what
    // Excel computed, or the formula's text when there is no cached value.
    let opened = open(&package(
        r#"<row r="1"><c r="A1"><f>D1 junk</f><v>5</v></c><c r="B1"><f>1+,</f></c><c r="C1"><f>D1 + 2 </f><v>0</v></c><c r="D1"><v>3</v></c></row>"#,
        &[],
    ));
    assert!(opened.sheet_errors.is_empty(), "{:?}", opened.sheet_errors);
    let engine = &opened.engine;
    assert_eq!(engine.get_value(0, at("A1")), CellResult::Value(5.0));
    assert_eq!(engine.get_formula(0, at("A1")), None);
    assert_eq!(
        engine.get_value(0, at("B1")),
        CellResult::Text("=1+,".into())
    );
    assert_eq!(engine.get_formula(0, at("B1")), None);
    assert_eq!(engine.get_formula(0, at("C1")).as_deref(), Some("=D1 + 2"));
    assert_eq!(engine.get_value(0, at("C1")), CellResult::Value(5.0));
}

#[test]
fn hostile_sizes_in_sheet_xml_are_bounded() {
    let sheet = format!(
        r#"<worksheet xmlns="{MAIN}">
  <sheetViews><sheetView><pane xSplit="-5" ySplit="1e300" state="frozen"/></sheetView></sheetViews>
  <cols><col min="1" max="3" width="1e308"/><col min="5" max="5" width="NaN"/>{}</cols>
  <sheetData>
    <row r="4294967295" ht="12" hidden="1"/><row r="2" ht="inf"/><row r="3" ht="-4"/>
    <c r="A1048577" s="0"/>
  </sheetData>
  <autoFilter ref="A1:B9"><filterColumn colId="4294967295"><filters><filter val="x"/></filters></filterColumn>
    <filterColumn colId="1"><filters><filter val="y"/></filters></filterColumn></autoFilter>
  <mergeCells><mergeCell ref="A1:A99999999999"/><mergeCell ref="B2:XFE3"/></mergeCells>
</worksheet>"#,
        // Many copies of a span covering every column: walked once.
        r#"<col min="1" max="16384" hidden="1"/>"#.repeat(10_000)
    );
    let opened = open(&package(
        "",
        &[("xl/worksheets/sheet1.xml", sheet.as_bytes())],
    ));
    let f = opened.formatting();
    assert_eq!(f.frozen, (MAX_ROW, 0));
    assert!(f.column_widths.values().all(|w| w.is_finite()));
    assert!(!f.column_widths.contains_key(&4), "NaN width");
    assert_eq!(f.hidden_columns.len(), MAX_COL as usize + 1);
    assert!(f.row_heights.is_empty(), "{:?}", f.row_heights);
    assert!(f.hidden_rows.is_empty());
    assert!(f.merges.is_empty());
    let filter = f.filter.as_ref().unwrap();
    assert_eq!(filter.allowed.keys().copied().collect::<Vec<_>>(), vec![1]);
}

#[test]
fn hostile_style_values_are_bounded() {
    let styles = format!(
        r#"<styleSheet xmlns="{MAIN}">
  <numFmts><numFmt numFmtId="164" formatCode="{}"/><numFmt numFmtId="165" formatCode="0.00"/></numFmts>
  <fonts><font><sz val="11"/></font><font><sz val="NaN"/><color rgb="Aé12345"/></font><font><sz val="1e999"/><color theme="4" tint="NaN"/></font></fonts>
  <cellXfs><xf numFmtId="164" fontId="1"/><xf numFmtId="165" fontId="2"/><xf fontId="99" fillId="99" borderId="99" numFmtId="4294967295"/></cellXfs>
</styleSheet>"#,
        "0".repeat(100_000)
    );
    let theme = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="x">
  <a:dk1><a:srgbClr val="Aé12345"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="ÿÿÿÿÿÿ"/></a:lt1></a:clrScheme></a:themeElements></a:theme>"#;
    let opened = open(&package(
        r#"<row r="1"><c r="A1" s="0"><v>1</v></c><c r="B1" s="1"><v>1</v></c><c r="C1" s="2"><v>1</v></c></row>"#,
        &[
            ("xl/styles.xml", styles.as_bytes()),
            ("xl/theme/theme1.xml", theme.as_bytes()),
        ],
    ));
    let f = opened.formatting();
    // A format code past Excel's 255 characters is dropped, not walked.
    assert_eq!(f.get(at("A1")), None);
    let b1 = f.get(at("B1")).unwrap();
    assert_eq!(b1.number_format.as_deref(), Some("0.00"));
    assert_eq!(b1.font_size, None);
}

#[test]
fn notes_off_the_sheet_and_damaged_notes_are_dropped() {
    let sheet_rels = rels(&[("rId1", "comments", "../comments1.xml")]);
    let comments = br#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>A</author></authors><commentList>
  <comment ref="A1" authorId="99"><text><t>kept</t></text></comment>
  <comment ref="A1048577" authorId="0"><text><t>off the sheet</t></text></comment>
</commentList></comments>"#;
    let opened = open(&package(
        "",
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &sheet_rels),
            ("xl/comments1.xml", comments),
        ],
    ));
    let notes = &opened.formatting().notes;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[&at("A1")].author, None);

    let opened = open(&package(
        "",
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &sheet_rels),
            (
                "xl/comments1.xml",
                b"<comments><commentList><comment ref=\"A1\"><text></t>",
            ),
        ],
    ));
    assert!(opened.formatting.is_ok());
}

/// A sheet whose drawing shows `image` once per anchor, with or without a
/// size of its own.
fn picture_package(image: &[u8], anchors: usize, with_size: bool) -> Vec<u8> {
    let ext = if with_size {
        r#"<xdr:ext cx="952500" cy="952500"/>"#
    } else {
        ""
    };
    let anchor = format!(
        r#"<xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>{ext}
<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="p"/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/></xdr:blipFill></xdr:pic><xdr:clientData/></xdr:oneCellAnchor>"#
    );
    let drawing = format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{REL}">{}</xdr:wsDr>"#,
        anchor.repeat(anchors)
    );
    package(
        "",
        &[
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                &rels(&[("rId1", "drawing", "../drawings/drawing1.xml")]),
            ),
            ("xl/drawings/drawing1.xml", drawing.as_bytes()),
            (
                "xl/drawings/_rels/drawing1.xml.rels",
                &rels(&[("rId1", "image", "../media/image1.png")]),
            ),
            ("xl/media/image1.png", image),
        ],
    )
}

#[test]
fn truncated_images_are_skipped_and_never_reach_the_writer() {
    use super::drawing::image_size;
    // Every prefix of a PNG with no size in the drawing: rust_xlsxwriter
    // read the size, indexing the header unchecked, and panicked on most.
    for len in 0..=TINY_PNG.len() {
        let prefix = &TINY_PNG[..len];
        let opened = open(&picture_package(prefix, 1, false));
        let kept = opened.formatting().pictures.len() == 1;
        assert_eq!(kept, image_size(prefix).is_some(), "{len} bytes");
        if kept {
            // What image_size accepts, rust_xlsxwriter reads without panicking.
            rust_xlsxwriter::Image::new_from_buffer(prefix).unwrap();
        }
    }
    assert_eq!(image_size(TINY_PNG), Some((1, 1)));

    // With a size in the drawing the image itself isn't read until saved;
    // saving leaves out what rust_xlsxwriter can't take.
    let opened = open(&picture_package(&TINY_PNG[..20], 1, true));
    assert_eq!(opened.formatting().pictures.len(), 1);
    let mut engine = opened.engine;
    *engine.formatting_mut(0) = opened.formatting.unwrap().remove(0).1;
    let file = TempFile::new(b"");
    let mut writer = XlsxWriter::new();
    writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
    writer.save(&file.0).unwrap();
}

#[test]
fn an_image_shown_many_times_is_read_once() {
    let opened = open(&picture_package(TINY_PNG, 200, true));
    let pictures = &opened.formatting().pictures;
    assert_eq!(pictures.len(), 200);
    assert!(
        pictures
            .iter()
            .all(|p| std::sync::Arc::ptr_eq(&p.data, &pictures[0].data))
    );
}

#[test]
fn image_sizes_never_index_past_the_end() {
    // Headers that claim chunks and segments longer than the data.
    let mut png = TINY_PNG[..33].to_vec();
    png.extend_from_slice(&u32::MAX.to_be_bytes());
    png.extend_from_slice(b"pHYs");
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    let sof = [0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x02, 0x08];
    for data in [&png[..], &jpeg, &sof, b"BM", b"GIF89a\x01", b"\x89PN", b""] {
        assert_eq!(super::drawing::image_size(data), None, "{data:?}");
    }
}

fn chart_xml(color: &str, range: &str) -> String {
    format!(
        r#"<c:chartSpace><c:chart><c:plotArea><c:barChart><c:ser><c:spPr><a:solidFill><a:srgbClr val="{color}"/></a:solidFill></c:spPr>
<c:val><c:numRef><c:f>Sheet1!{range}</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#
    )
}

#[test]
fn one_bad_chart_does_not_cost_the_others() {
    let good = chart_xml("4472C4", "$A$1:$A$3");
    // Six bytes that aren't six characters: sliced mid-character before.
    let odd_color = chart_xml("aé123", "$A$1:$A$3");
    let off_sheet = chart_xml("4472C4", "$A$1:$A$1048577");
    let whole_sheet = chart_xml("4472C4", "$A$1:$XFD$1048576");
    let opened = open(&package(
        "",
        &[
            ("xl/charts/chart1.xml", good.as_bytes()),
            ("xl/charts/chart2.xml", b"\xFF\xFE not utf-8"),
            ("xl/charts/chart3.xml", odd_color.as_bytes()),
            ("xl/charts/chart4.xml", off_sheet.as_bytes()),
            ("xl/charts/chart5.xml", whole_sheet.as_bytes()),
        ],
    ));
    let charts = opened.charts.unwrap();
    assert_eq!(charts.len(), 4, "the part that isn't text is skipped");
    let series: Vec<usize> = charts.iter().map(|(_, c)| c.series.len()).collect();
    assert_eq!(series, vec![1, 1, 0, 0], "ranges off or across the sheet");
    assert_eq!(charts[1].1.series[0].color, None);
}

#[test]
fn damaged_chart_json_falls_back_and_hostile_json_is_bounded() {
    let good = chart_xml("4472C4", "$A$1:$A$3");
    let opened = open(&package(
        "",
        &[
            (super::writer::CHARTS_MANIFEST, b"[{\"id\": "),
            ("xl/charts/chart1.xml", good.as_bytes()),
        ],
    ));
    assert_eq!(opened.charts.unwrap().len(), 1, "Excel's chart instead");

    let mut chart = serde_json::to_value(ChartDefinition::default()).unwrap();
    chart["overlay_area"]["size"] = serde_json::json!([1e30, -4.0]);
    chart["overlay_area"]["anchor_cell"] = serde_json::json!([u32::MAX, u32::MAX]);
    chart["style"]["title_font_size"] = serde_json::json!(1e300);
    chart["series"] = serde_json::json!([
        // Corners out of order: CellRange::new never made this.
        {"name": null, "x_range": null, "y_range": {"start": {"row": 9, "col": 0}, "end": {"row": 0, "col": 0}},
         "color": null, "line_style": "Solid", "marker_style": "Circle", "show_data_labels": false,
         "use_secondary_axis": false, "chart_type_override": null},
        {"name": null, "x_range": null, "y_range": {"start": {"row": 0, "col": 0}, "end": {"row": 9, "col": 0}},
         "color": null, "line_style": "Solid", "marker_style": "Circle", "show_data_labels": false,
         "use_secondary_axis": false, "chart_type_override": null},
    ]);
    let json = serde_json::to_vec(&vec![chart]).unwrap();
    let opened = open(&package("", &[(super::writer::CHARTS_MANIFEST, &json)]));
    let charts = opened.charts.unwrap();
    let c = &charts[0].1;
    assert_eq!(c.series.len(), 1);
    assert_eq!(c.overlay_area.anchor_cell, (MAX_ROW, MAX_COL));
    assert_eq!(c.overlay_area.size, (10_000.0, 300.0));
    assert_eq!(
        c.style.title_font_size, 16.0,
        "infinite after f32 conversion"
    );
}

#[test]
fn damaged_pivot_parts_are_skipped() {
    let cache = br#"<pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <cacheSource type="worksheet"><worksheetSource ref="A1:B9" sheet="Sheet1"/></cacheSource>
  <cacheFields><cacheField name="a"><sharedItems><s v="x"/></sharedItems></cacheField><cacheField name="b"/></cacheFields></pivotCacheDefinition>"#;
    // A page row count of u32::MAX overflowed the filter rows above it.
    let layout = br#"<pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="P">
  <location ref="D5:E9" rowPageCount="4294967295"/>
  <pivotFields><pivotField/><pivotField/></pivotFields>
  <pageFields><pageField fld="0" item="7"/></pageFields>
  <dataFields><dataField fld="1"/><dataField fld="99"/></dataFields></pivotTableDefinition>"#;
    let opened = open(&package(
        "",
        &[
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                &rels(&[("rId1", "pivotTable", "../pivotTables/pivotTable1.xml")]),
            ),
            ("xl/pivotTables/pivotTable1.xml", layout),
            (
                "xl/pivotTables/_rels/pivotTable1.xml.rels",
                &rels(&[("rId1", "pivotCacheDefinition", "../pivotCache/def1.xml")]),
            ),
            ("xl/pivotCache/def1.xml", cache),
        ],
    ));
    let pivots = &opened.formatting().pivots;
    assert_eq!(pivots.len(), 1);
    assert_eq!(pivots[0].anchor, at("D1"));
    assert_eq!(pivots[0].values.len(), 1, "field 99 isn't in the source");

    // RustSheet's own manifest, edited to name fields the source lacks.
    let mut table = crate::pivot::PivotTable::new(
        "T".into(),
        "Sheet1".into(),
        CellRange::from_a1("A1:B9").unwrap(),
        at("D1"),
    );
    table.rows = vec![0];
    let mut bad = table.clone();
    bad.rows = vec![usize::MAX];
    let mut reversed = table.clone();
    reversed.source = CellRange {
        start: at("B9"),
        end: at("A1"),
    };
    let json = serde_json::to_vec(&vec![("Sheet1", vec![table.clone(), bad, reversed])]).unwrap();
    let opened = open(&package("", &[(super::writer::PIVOTS_MANIFEST, &json)]));
    assert_eq!(opened.formatting().pivots, vec![table]);
}
