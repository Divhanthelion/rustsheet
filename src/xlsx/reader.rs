use super::limits::{self, contain};
use super::styles::{attr, parse_rels, parse_sheet_list};
use crate::calc::{CalcEngine, CellValueInput};
use crate::cell::{CellCoord, CellError, CellRange, CellValue, MAX_ROW};
use crate::formula::normalize_formula;
use crate::grid::Sheet;
use calamine::{DataRef, Reader, Xlsx, XlsxError, open_workbook};
use quick_xml::events::Event;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum XlsxReadError {
    #[error("Failed to open workbook: {0}")]
    Open(#[from] XlsxError),
    #[error("Sheet not found: {0}")]
    SheetNotFound(String),
    #[error("Failed to read sheet: {0}")]
    SheetRead(String),
    /// An OLE compound file: a password-protected workbook, or an .xls.
    #[error("Password-protected and older (.xls) workbooks can't be opened")]
    Unsupported,
    /// Damaged, or asking for more than the reader allows.
    #[error("Damaged workbook: {0}")]
    Damaged(String),
    /// Declares more unpacked data than the reader will inflate.
    #[error("The workbook is too large to open: {0}")]
    TooLarge(String),
}

/// calamine expands each shared formula into an entry per cell of its
/// `ref`, and keeps the groups in a list indexed by `si`. Past these a
/// sheet's formulas aren't read, rather than let the file size either.
const MAX_SHARED_CELLS: u64 = 8 * (MAX_ROW as u64 + 1);
const MAX_SHARED_INDEX: u64 = 1 << 20;

/// OLE compound files start with this. calamine sizes buffers from their
/// headers unchecked, so they are turned away before it looks.
const OLE_SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Excel file reader using calamine
pub struct XlsxReader {
    workbook: Xlsx<BufReader<std::fs::File>>,
    path: PathBuf,
}

impl XlsxReader {
    /// Open an Excel file for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, XlsxReadError> {
        let path = path.as_ref();
        let mut magic = [0u8; 8];
        let read = std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic));
        if read.is_ok() && magic == OLE_SIGNATURE {
            return Err(XlsxReadError::Unsupported);
        }
        // calamine's own reads of a part aren't capped: check what the
        // package declares before it looks.
        if let Ok(file) = std::fs::File::open(path) {
            contain("The workbook", || {
                limits::check_declared_sizes(BufReader::new(file))
            })
            .map_err(XlsxReadError::Damaged)?
            .map_err(XlsxReadError::TooLarge)?;
        }
        let workbook: Xlsx<_> =
            contain("The workbook", || open_workbook(path)).map_err(XlsxReadError::Damaged)??;
        Ok(Self {
            workbook,
            path: path.to_path_buf(),
        })
    }

    /// Get list of sheet names
    pub fn sheet_names(&self) -> Vec<String> {
        self.workbook.sheet_names().to_vec()
    }

    /// Read a sheet into our Sheet structure
    pub fn read_sheet(&mut self, name: &str) -> Result<Sheet, XlsxReadError> {
        let mut sheet = Sheet::new(name);
        self.each_value(name, |coord, value| match value {
            CellValueInput::Number(n) => sheet.set_number(coord, n),
            CellValueInput::Text(s) => sheet.set_text(coord, &s),
            CellValueInput::Bool(b) => sheet.set_bool(coord, b),
            CellValueInput::Error(e) => {
                sheet.set(coord, CellValue::Error(e));
            }
        })?;
        Ok(sheet)
    }

    /// Read values and formulas into the calculation engine.
    pub fn read_into_engine(
        &mut self,
        name: &str,
        engine: &mut CalcEngine,
        sheet_index: u32,
    ) -> Result<(), XlsxReadError> {
        let is_worksheet = self.each_value(name, |coord, value| {
            engine.set_value(sheet_index, coord, value)
        })?;
        if !is_worksheet {
            return Ok(());
        }
        self.check_shared_formulas(name)?;
        // As before, a sheet whose formulas can't be read keeps its values.
        // A formula the parser refuses keeps its cell's cached value, or
        // with none its text, so the cell isn't lost.
        let _ = self.each_formula(name, |coord, formula| {
            let formula = normalize_formula(formula);
            if engine.set_formula(sheet_index, coord, &formula).is_err()
                && engine.get_input(sheet_index, coord).is_none()
            {
                engine.set_value(sheet_index, coord, CellValueInput::Text(formula));
            }
        });
        Ok(())
    }

    /// Read a sheet by index
    pub fn read_sheet_by_index(&mut self, index: usize) -> Result<Sheet, XlsxReadError> {
        let names = self.sheet_names();
        let name = names
            .get(index)
            .ok_or_else(|| XlsxReadError::SheetNotFound(format!("index {}", index)))?
            .clone();
        self.read_sheet(&name)
    }

    /// Call `f` with each value on sheet `name`, streamed: nothing is sized
    /// from the cells' extent, and cells off an Excel-sized sheet are
    /// dropped. `false` for chart and dialog sheets, which have no cells.
    fn each_value(
        &mut self,
        name: &str,
        mut f: impl FnMut(CellCoord, CellValueInput),
    ) -> Result<bool, XlsxReadError> {
        let what = format!("Sheet '{name}'");
        let workbook = &mut self.workbook;
        let cells = contain(&what, move || workbook.worksheet_cells_reader(name));
        let mut cells = match cells.map_err(XlsxReadError::Damaged)? {
            Ok(cells) => cells,
            Err(XlsxError::NotAWorksheet(_)) => return Ok(false),
            Err(e) => return Err(XlsxReadError::SheetRead(e.to_string())),
        };
        loop {
            match contain(&what, || cells.next_cell()).map_err(XlsxReadError::Damaged)? {
                Ok(Some(cell)) => {
                    let (row, col) = cell.get_position();
                    let coord = CellCoord::new(row, col);
                    if let (true, Some(value)) = (limits::on_sheet(coord), input(cell.get_value()))
                    {
                        f(coord, value);
                    }
                }
                Ok(None) => return Ok(true),
                Err(e) => return Err(XlsxReadError::SheetRead(e.to_string())),
            }
        }
    }

    /// Call `f` with each formula on sheet `name`, streamed like
    /// [`Self::each_value`].
    fn each_formula(
        &mut self,
        name: &str,
        mut f: impl FnMut(CellCoord, &str),
    ) -> Result<(), XlsxReadError> {
        let what = format!("Sheet '{name}'");
        let workbook = &mut self.workbook;
        let cells = contain(&what, move || workbook.worksheet_cells_reader(name));
        let mut cells = cells
            .map_err(XlsxReadError::Damaged)?
            .map_err(|e| XlsxReadError::SheetRead(e.to_string()))?;
        loop {
            match contain(&what, || cells.next_formula()).map_err(XlsxReadError::Damaged)? {
                Ok(Some(cell)) => {
                    let (row, col) = cell.get_position();
                    let coord = CellCoord::new(row, col);
                    let formula = cell.get_value();
                    if limits::on_sheet(coord) && !formula.is_empty() {
                        f(coord, formula);
                    }
                }
                Ok(None) => return Ok(()),
                Err(e) => return Err(XlsxReadError::SheetRead(e.to_string())),
            }
        }
    }

    /// Refuse sheet `name`'s formulas if its shared formulas would have
    /// calamine expand more than [`MAX_SHARED_CELLS`] cells or index a group
    /// past [`MAX_SHARED_INDEX`]. A streamed pass over the sheet's XML.
    fn check_shared_formulas(&self, name: &str) -> Result<(), XlsxReadError> {
        let damaged = |why: &str| {
            XlsxReadError::Damaged(format!("formulas on sheet '{name}' not read: {why}"))
        };
        let file = std::fs::File::open(&self.path).map_err(|e| damaged(&e.to_string()))?;
        let mut zip =
            zip::ZipArchive::new(BufReader::new(file)).map_err(|e| damaged(&e.to_string()))?;
        let part = sheet_part(&mut zip, name).ok_or_else(|| damaged("sheet part not found"))?;
        let entry = zip.by_name(&part).map_err(|e| damaged(&e.to_string()))?;
        let mut xml = quick_xml::Reader::from_reader(BufReader::new(entry));
        // As lenient as calamine's reader, so both see the same elements.
        let config = xml.config_mut();
        config.check_end_names = false;
        config.check_comments = false;
        let mut buf = Vec::new();
        let mut cells = 0u64;
        loop {
            buf.clear();
            match xml.read_event_into(&mut buf) {
                Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == b"f" => {
                    let Some(r) = attr(&e, b"ref") else {
                        continue;
                    };
                    if attr(&e, b"t").as_deref() != Some("shared") {
                        continue;
                    }
                    let index = attr(&e, b"si").and_then(|s| s.parse::<u64>().ok());
                    if !index.is_some_and(|i| i <= MAX_SHARED_INDEX) {
                        return Err(damaged("a shared formula's index is out of range"));
                    }
                    let area = CellRange::from_a1(&r).map_or(u64::MAX, |r| r.cell_count());
                    cells = cells.saturating_add(area);
                    if cells > MAX_SHARED_CELLS {
                        return Err(damaged("shared formulas cover too many cells"));
                    }
                }
                Ok(Event::End(e)) if e.local_name().as_ref() == b"sheetData" => return Ok(()),
                Ok(Event::Eof) => return Ok(()),
                Err(e) => return Err(damaged(&e.to_string())),
                _ => {}
            }
        }
    }
}

/// The part calamine reads for sheet `name`, resolved the way it resolves
/// it.
fn sheet_part<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let mut text = |path: &str| {
        let part = stored_name(zip, path);
        String::from_utf8(limits::read_part(zip, &part)?).ok()
    };
    let rels = parse_rels(&text("xl/_rels/workbook.xml.rels")?);
    let sheets = parse_sheet_list(&text("xl/workbook.xml")?).ok()?;
    let (_, id) = sheets.into_iter().find(|(n, _)| n == name)?;
    let target = rels.get(&id)?;
    let path = match target.strip_prefix('/') {
        Some(abs) if abs.starts_with("xl/") => abs.to_string(),
        _ if target.starts_with("xl/") => target.clone(),
        _ => format!("xl/{target}"),
    };
    Some(stored_name(zip, &path))
}

/// A part's name as the package stores it: calamine matches names ignoring
/// case and slash direction.
fn stored_name<R: Read + Seek>(zip: &zip::ZipArchive<R>, path: &str) -> String {
    zip.file_names()
        .find(|n| n.replace('\\', "/").eq_ignore_ascii_case(path))
        .unwrap_or(path)
        .to_string()
}

/// A cell's value as engine input; `None` for empty cells. Excel can't
/// store NaN or infinity, so a file that does gets #NUM!.
fn input(value: &DataRef<'_>) -> Option<CellValueInput> {
    let number = |n: f64| {
        if n.is_finite() {
            CellValueInput::Number(n)
        } else {
            CellValueInput::Error(CellError::Num)
        }
    };
    Some(match value {
        DataRef::Empty => return None,
        DataRef::Int(i) => CellValueInput::Number(*i as f64),
        DataRef::Float(f) => number(*f),
        DataRef::String(s) | DataRef::DateTimeIso(s) | DataRef::DurationIso(s) => {
            CellValueInput::Text(s.clone())
        }
        DataRef::SharedString(s) => CellValueInput::Text(s.to_string()),
        DataRef::Bool(b) => CellValueInput::Bool(*b),
        DataRef::DateTime(dt) => number(dt.as_f64()),
        DataRef::Error(e) => CellValueInput::Error(match e {
            calamine::CellErrorType::Div0 => CellError::DivZero,
            calamine::CellErrorType::NA => CellError::NA,
            calamine::CellErrorType::Name => CellError::Name,
            calamine::CellErrorType::Null => CellError::Null,
            calamine::CellErrorType::Num => CellError::Num,
            calamine::CellErrorType::Ref => CellError::Ref,
            calamine::CellErrorType::Value => CellError::Value,
            calamine::CellErrorType::GettingData => CellError::GettingData,
        }),
    })
}
