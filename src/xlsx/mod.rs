#[cfg(feature = "xlsx")]
mod chart_reader;
#[cfg(feature = "xlsx")]
mod drawing;
#[cfg(feature = "xlsx")]
mod pivot_reader;
#[cfg(feature = "xlsx")]
mod reader;
#[cfg(feature = "xlsx")]
mod styles;
#[cfg(feature = "xlsx")]
mod writer;

#[cfg(feature = "xlsx")]
pub use chart_reader::{ChartReadError, ChartReader};
#[cfg(feature = "xlsx")]
pub use reader::XlsxReader;
#[cfg(feature = "xlsx")]
pub use styles::read_formatting;

/// Read per-sheet formatting from an .xlsx file. See [`read_formatting`].
#[cfg(feature = "xlsx")]
pub fn read_formatting_from_path(
    path: impl AsRef<std::path::Path>,
) -> Result<Vec<(String, crate::format::SheetFormatting)>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    read_formatting(std::io::BufReader::new(file))
}
#[cfg(feature = "xlsx")]
pub use writer::{XlsxWriteError, XlsxWriter};

#[cfg(test)]
mod tests {
    use super::{XlsxReader, XlsxWriter};
    use crate::calc::{CalcEngine, CellResult, CellValueInput};
    use crate::cell::CellCoord;

    fn temp_xlsx(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustsheet_{}_{}_{}.xlsx",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    #[test]
    fn formula_roundtrip_through_xlsx() {
        let mut engine = CalcEngine::new();
        let a1 = CellCoord::from_a1("A1").unwrap();
        let a2 = CellCoord::from_a1("A2").unwrap();
        let a3 = CellCoord::from_a1("A3").unwrap();
        engine.set_value(0, a1, CellValueInput::Number(1.0));
        engine.set_value(0, a2, CellValueInput::Number(2.0));
        engine.set_formula(0, a3, "=SUM(A1:A2)").unwrap();

        let path = temp_xlsx("formula");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.get_formula(0, a3).as_deref(), Some("=SUM(A1:A2)"));
        assert_eq!(loaded.get_value(0, a3), CellResult::Value(3.0));
    }

    #[test]
    fn used_range_outside_visible_grid_survives_xlsx() {
        let mut engine = CalcEngine::new();
        let far = CellCoord::from_a1("AA1001").unwrap();
        engine.set_value(0, far, CellValueInput::Number(42.0));

        let path = temp_xlsx("used_range");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.get_value(0, far), CellResult::Value(42.0));
    }

    #[test]
    fn charts_roundtrip_through_xlsx() {
        use crate::cell::CellRange;
        use crate::chart::{ChartDefinition, ChartKind, ChartSeries};

        let mut engine = CalcEngine::new();
        engine.set_value(
            0,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(1.0),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A2").unwrap(),
            CellValueInput::Number(2.0),
        );

        let chart = ChartDefinition::new(ChartKind::Line)
            .with_title("Sales")
            .with_series(ChartSeries::new(CellRange::from_a1("A1:A2").unwrap()))
            .with_sheet(0);

        let path = temp_xlsx("charts");
        let mut writer = XlsxWriter::new();
        writer
            .add_engine_sheet_with_charts("Sheet1", &engine, 0, std::slice::from_ref(&chart))
            .unwrap();
        writer
            .save_with_charts(&path, std::slice::from_ref(&chart))
            .unwrap();

        let charts = super::ChartReader::read_charts(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].1.title.as_deref(), Some("Sales"));
        assert_eq!(charts[0].1.chart_kind, ChartKind::Line);
        assert_eq!(charts[0].1.series.len(), 1);
    }

    #[test]
    fn cross_sheet_formula_roundtrip_xlsx() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);
        engine.set_value(
            1,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(7.0),
        );
        engine
            .set_formula(0, CellCoord::from_a1("A1").unwrap(), "=Sheet2!A1")
            .unwrap();

        let path = temp_xlsx("cross_sheet");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.add_engine_sheet("Sheet2", &engine, 1).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        loaded.set_sheet_names(reader.sheet_names());
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        reader.read_into_engine("Sheet2", &mut loaded, 1).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            loaded
                .get_formula(0, CellCoord::from_a1("A1").unwrap())
                .as_deref(),
            Some("=Sheet2!A1")
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(7.0)
        );
    }

    #[test]
    fn mixed_cell_types_roundtrip_xlsx() {
        let mut engine = CalcEngine::new();

        // Numbers, text, booleans
        engine.set_value(
            0,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(123.456),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A2").unwrap(),
            CellValueInput::Text("Hello World".into()),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A3").unwrap(),
            CellValueInput::Bool(true),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A4").unwrap(),
            CellValueInput::Bool(false),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A5").unwrap(),
            CellValueInput::Number(-99.5),
        );

        let path = temp_xlsx("mixed_types");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(123.456)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A2").unwrap()),
            CellResult::Text("Hello World".into())
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A3").unwrap()),
            CellResult::Bool(true)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A4").unwrap()),
            CellResult::Bool(false)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A5").unwrap()),
            CellResult::Value(-99.5)
        );
    }

    #[test]
    fn complex_formula_roundtrip_xlsx() {
        let mut engine = CalcEngine::new();

        // Set up data
        engine.set_value(
            0,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(10.0),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A2").unwrap(),
            CellValueInput::Number(20.0),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("A3").unwrap(),
            CellValueInput::Number(30.0),
        );

        // Complex formulas
        engine
            .set_formula(0, CellCoord::from_a1("B1").unwrap(), "=SUM(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::from_a1("B2").unwrap(), "=AVERAGE(A1:A3)")
            .unwrap();
        engine
            .set_formula(
                0,
                CellCoord::from_a1("B3").unwrap(),
                "=IF(B1>50,\"High\",\"Low\")",
            )
            .unwrap();
        engine
            .set_formula(0, CellCoord::from_a1("B4").unwrap(), "=A1*2+A2/2")
            .unwrap();

        let path = temp_xlsx("complex_formula");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        let _ = std::fs::remove_file(&path);

        // Verify formulas are preserved
        assert_eq!(
            loaded
                .get_formula(0, CellCoord::from_a1("B1").unwrap())
                .as_deref(),
            Some("=SUM(A1:A3)")
        );
        assert_eq!(
            loaded
                .get_formula(0, CellCoord::from_a1("B2").unwrap())
                .as_deref(),
            Some("=AVERAGE(A1:A3)")
        );

        // Verify computed values
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("B1").unwrap()),
            CellResult::Value(60.0)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("B2").unwrap()),
            CellResult::Value(20.0)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("B3").unwrap()),
            CellResult::Text("High".into())
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("B4").unwrap()),
            CellResult::Value(30.0)
        );
    }

    #[test]
    fn multi_sheet_values_roundtrip_xlsx() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sales".into(), "Expenses".into(), "Summary".into()]);

        // Populate different sheets
        engine.set_value(
            0,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(1000.0),
        );
        engine.set_value(
            1,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(500.0),
        );
        engine
            .set_formula(
                2,
                CellCoord::from_a1("A1").unwrap(),
                "=Sales!A1-Expenses!A1",
            )
            .unwrap();

        let path = temp_xlsx("multi_sheet");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sales", &engine, 0).unwrap();
        writer.add_engine_sheet("Expenses", &engine, 1).unwrap();
        writer.add_engine_sheet("Summary", &engine, 2).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        loaded.set_sheet_names(reader.sheet_names());
        reader.read_into_engine("Sales", &mut loaded, 0).unwrap();
        reader.read_into_engine("Expenses", &mut loaded, 1).unwrap();
        reader.read_into_engine("Summary", &mut loaded, 2).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(1000.0)
        );
        assert_eq!(
            loaded.get_value(1, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(500.0)
        );
        assert_eq!(
            loaded.get_value(2, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(500.0)
        );
    }

    #[test]
    fn sparse_data_roundtrip_xlsx() {
        let mut engine = CalcEngine::new();

        // Sparse data with gaps
        engine.set_value(
            0,
            CellCoord::from_a1("A1").unwrap(),
            CellValueInput::Number(1.0),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("C5").unwrap(),
            CellValueInput::Number(5.0),
        );
        engine.set_value(
            0,
            CellCoord::from_a1("Z100").unwrap(),
            CellValueInput::Number(100.0),
        );

        let path = temp_xlsx("sparse_data");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();

        let mut loaded = CalcEngine::new();
        let mut reader = XlsxReader::open(&path).unwrap();
        reader.read_into_engine("Sheet1", &mut loaded, 0).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("A1").unwrap()),
            CellResult::Value(1.0)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("C5").unwrap()),
            CellResult::Value(5.0)
        );
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("Z100").unwrap()),
            CellResult::Value(100.0)
        );
        // Empty cells should return empty
        assert_eq!(
            loaded.get_value(0, CellCoord::from_a1("B2").unwrap()),
            CellResult::Empty
        );
    }

    #[test]
    fn formatting_roundtrip_through_xlsx() {
        use crate::format::{Borders, CellFormat, HAlign, Rgb};

        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Data".into(), "Second".into()]);
        let a1 = CellCoord::from_a1("A1").unwrap();
        let b2 = CellCoord::from_a1("B2").unwrap();
        let c3 = CellCoord::from_a1("C3").unwrap();
        engine.set_value(0, a1, CellValueInput::Text("Header".into()));
        engine.set_value(0, b2, CellValueInput::Number(0.256));

        let header = CellFormat {
            bold: true,
            italic: true,
            underline: true,
            font_size: Some(14),
            font_name: Some("Georgia".into()),
            font_color: Some(Rgb(0x1F, 0x4E, 0x79)),
            fill: Some(Rgb(0xFF, 0xF2, 0xCC)),
            h_align: HAlign::Center,
            borders: Borders::ALL,
            ..Default::default()
        };
        let percent = CellFormat {
            number_format: Some("0.0%".into()),
            strikethrough: true,
            ..Default::default()
        };
        // A formatted cell with no value.
        let empty_bottom = CellFormat {
            borders: Borders {
                bottom: true,
                ..Borders::NONE
            },
            ..Default::default()
        };
        engine.set_cell_format(0, a1, header.clone());
        engine.set_cell_format(0, b2, percent.clone());
        engine.set_cell_format(0, c3, empty_bottom.clone());
        engine.formatting_mut(0).column_widths.insert(0, 160.0);
        engine.formatting_mut(0).row_heights.insert(0, 44.0);
        engine.set_cell_format(
            1,
            a1,
            CellFormat {
                number_format: Some("yyyy-mm-dd".into()),
                ..Default::default()
            },
        );

        let path = temp_xlsx("formatting");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Data", &engine, 0).unwrap();
        writer.add_engine_sheet("Second", &engine, 1).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 2);
        let (name, data) = &read[0];
        assert_eq!(name, "Data");
        assert_eq!(data.get(a1), Some(&header));
        assert_eq!(data.get(b2), Some(&percent));
        assert_eq!(data.get(c3), Some(&empty_bottom));
        let width = data.column_widths.get(&0).copied().unwrap();
        assert!((width - 160.0).abs() < 2.0, "width {width}");
        let height = data.row_heights.get(&0).copied().unwrap();
        assert!((height - 44.0).abs() < 1.5, "height {height}");
        assert_eq!(
            read[1].1.get(a1).and_then(|f| f.number_format.as_deref()),
            Some("yyyy-mm-dd")
        );
    }

    #[test]
    fn layout_roundtrip_through_xlsx() {
        use crate::cell::{Axis, CellRange};
        use crate::format::{AutoFilter, CellFormat, VAlign};
        use std::collections::{BTreeMap, BTreeSet};

        let mut engine = CalcEngine::new();
        let r = |a1: &str| CellRange::from_a1(a1).unwrap();
        for (a1, v) in [
            ("A1", "Fruit"),
            ("A2", "Tea"),
            ("A3", "Cake"),
            ("A4", "Tea"),
        ] {
            engine.set_value(
                0,
                CellCoord::from_a1(a1).unwrap(),
                CellValueInput::Text(v.into()),
            );
        }
        let bold = CellFormat {
            bold: true,
            ..Default::default()
        };
        let wrapped = CellFormat {
            wrap: true,
            v_align: VAlign::Top,
            ..Default::default()
        };
        {
            let f = engine.formatting_mut(0);
            f.set_line_format(Axis::Column, 3, bold.clone());
            f.set_line_format(Axis::Column, 4, bold.clone());
            f.set_line_format(Axis::Row, 7, wrapped.clone());
            f.merges.push(r("C10:E11"));
            f.frozen = (1, 1);
            f.hidden_columns.insert(6);
            f.hidden_rows.insert(2);
            let mut allowed = BTreeMap::new();
            allowed.insert(0, BTreeSet::from(["Tea".to_string()]));
            f.filter = Some(AutoFilter {
                range: r("A1:A4"),
                allowed,
            });
        }
        engine.set_cell_format(0, CellCoord::from_a1("B2").unwrap(), wrapped.clone());
        let note = crate::format::Note {
            text: "Check this & that <later>".into(),
            author: Some("Avery".into()),
        };
        engine
            .formatting_mut(0)
            .notes
            .insert(CellCoord::from_a1("A3").unwrap(), note.clone());

        let path = temp_xlsx("layout");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let f = &read[0].1;

        assert_eq!(f.column_formats.get(&3), Some(&bold));
        assert_eq!(f.column_formats.get(&4), Some(&bold));
        assert_eq!(f.row_formats.get(&7), Some(&wrapped));
        assert_eq!(f.get(CellCoord::from_a1("B2").unwrap()), Some(&wrapped));
        // Cells in a formatted column show (and save) that format.
        assert_eq!(f.effective(CellCoord::from_a1("D2").unwrap()), Some(&bold));
        assert_eq!(f.merges, vec![r("C10:E11")]);
        assert_eq!(f.frozen, (1, 1));
        assert!(f.hidden_columns.contains(&6));
        assert!(f.hidden_rows.contains(&2));
        let read_note = &f.notes[&CellCoord::from_a1("A3").unwrap()];
        assert!(
            read_note.text.contains("Check this & that <later>"),
            "{read_note:?}"
        );
        assert_eq!(read_note.author.as_deref(), Some("Avery"));
        let filter = f.filter.as_ref().unwrap();
        assert_eq!(filter.range, r("A1:A4"));
        assert!(filter.allowed[&0].contains("Tea"));
    }

    #[test]
    fn validation_roundtrip_through_xlsx() {
        use crate::cell::CellRange;
        use crate::format::validation::{CompareOp, DataValidation, ErrorStyle, ValidationKind};

        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Main".into(), "Lists".into()]);
        let r = |a1: &str| CellRange::from_a1(a1).unwrap();
        let list = DataValidation {
            ranges: vec![r("A2:A20")],
            kind: ValidationKind::List,
            formula1: "\"Small,Medium,Large\"".into(),
            input_title: "Size".into(),
            input_message: "Pick a size".into(),
            ..Default::default()
        };
        let whole = DataValidation {
            ranges: vec![r("B2:B20"), r("D2:D5")],
            kind: ValidationKind::Whole,
            operator: CompareOp::Between,
            formula1: "1".into(),
            formula2: Some("100".into()),
            error_style: ErrorStyle::Warning,
            error_title: "Out of range".into(),
            error_message: "Use 1 to 100".into(),
            ..Default::default()
        };
        let from_sheet = DataValidation {
            ranges: vec![r("C2:C20")],
            kind: ValidationKind::List,
            formula1: "Lists!$A$1:$A$3".into(),
            ..Default::default()
        };
        engine.formatting_mut(0).validations =
            vec![list.clone(), whole.clone(), from_sheet.clone()];

        let path = temp_xlsx("validation");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Main", &engine, 0).unwrap();
        writer.add_engine_sheet("Lists", &engine, 1).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let got = &read[0].1.validations;
        assert_eq!(got.len(), 3, "{got:?}");
        let find = |range: CellRange| got.iter().find(|v| v.ranges.contains(&range)).unwrap();
        let l = find(r("A2:A20"));
        assert_eq!(l.kind, ValidationKind::List);
        assert_eq!(l.literal_items().unwrap(), vec!["Small", "Medium", "Large"]);
        assert_eq!(l.input_message, "Pick a size");
        assert!(l.dropdown);
        let w = find(r("B2:B20"));
        assert_eq!(w.ranges, vec![r("B2:B20"), r("D2:D5")]);
        assert_eq!(
            (w.formula1.as_str(), w.formula2.as_deref()),
            ("1", Some("100"))
        );
        assert_eq!(w.error_style, ErrorStyle::Warning);
        assert_eq!(w.error_message, "Use 1 to 100");
        assert_eq!(find(r("C2:C20")).formula1, "Lists!$A$1:$A$3");
    }

    #[test]
    fn conditional_formats_round_trip() {
        use crate::cell::CellRange;
        use crate::format::Rgb;
        use crate::format::conditional::{
            AverageRule, CfRule, CfStyle, Cfvo, ConditionalFormat, TextRule,
        };
        use crate::format::validation::CompareOp;
        let r = |a1: &str| CellRange::from_a1(a1).unwrap();
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        let bold_red = CfStyle {
            bold: Some(true),
            number_format: Some("0.0%".into()),
            ..CfStyle::preset(0)
        };
        let rules = vec![
            ConditionalFormat {
                ranges: vec![r("A1:A10"), r("C1:C10")],
                rule: CfRule::CellIs {
                    op: CompareOp::Between,
                    formula1: "1".into(),
                    formula2: Some("$B$1".into()),
                    style: bold_red.clone(),
                },
                stop_if_true: true,
            },
            ConditionalFormat {
                ranges: vec![r("A1:A10")],
                rule: CfRule::Text {
                    rule: TextRule::BeginsWith,
                    text: "Q\"1".into(),
                    style: CfStyle::preset(1),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("B1:B10")],
                rule: CfRule::Top {
                    bottom: true,
                    rank: 10,
                    percent: true,
                    style: CfStyle::preset(2),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("D1:D10")],
                rule: CfRule::Average {
                    rule: AverageRule::EqualOrAbove,
                    style: CfStyle::preset(3),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("E1:E10")],
                rule: CfRule::Expression {
                    formula: "$A1>AVERAGE($A$1:$A$10)".into(),
                    style: CfStyle::preset(0),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("F1:F10")],
                rule: CfRule::ColorScale {
                    stops: vec![
                        (Cfvo::min(), Rgb(0xF8, 0x69, 0x6B)),
                        (Cfvo::percentile(50), Rgb(0xFF, 0xEB, 0x84)),
                        (Cfvo::max(), Rgb(0x63, 0xBE, 0x7B)),
                    ],
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("G1:G10")],
                rule: CfRule::DataBar {
                    min: Cfvo::min(),
                    max: Cfvo::max(),
                    color: Rgb(0x5A, 0x8A, 0xC6),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![r("H1:H10")],
                rule: CfRule::Duplicate {
                    unique: true,
                    style: CfStyle::preset(1),
                },
                stop_if_true: false,
            },
        ];
        engine.formatting_mut(0).conditional = rules.clone();

        let path = temp_xlsx("conditional");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Main", &engine, 0).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(read[0].1.conditional, rules);
    }

    /// A 1x1 PNG.
    pub(crate) const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn pictures_round_trip() {
        use crate::format::picture::{Picture, PictureKind};
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        let mut logo = Picture::new(
            CellCoord::new(3, 2),
            std::sync::Arc::from(TINY_PNG),
            PictureKind::Png,
            (1, 1),
        );
        logo.size = (250.0, 125.0);
        logo.offset = (12.5, 2.2);
        logo.description = "Company logo".into();
        let mut second = logo.clone();
        second.anchor = CellCoord::new(20, 0);
        second.description.clear();
        engine.formatting_mut(0).pictures = vec![logo.clone(), second.clone()];

        let path = temp_xlsx("pictures");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Main", &engine, 0).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(read[0].1.pictures, vec![logo, second]);
    }

    #[test]
    fn pivot_tables_round_trip() {
        use crate::cell::CellRange;
        use crate::pivot::{Aggregate, PivotTable, PivotValue};
        use std::io::Read;
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Data".into(), "Report".into()]);
        engine.set_value(
            0,
            CellCoord::new(0, 0),
            CellValueInput::Text("Region".into()),
        );
        let mut table = PivotTable::new(
            "PivotTable1".into(),
            "Data".into(),
            CellRange::from_a1("A1:C40").unwrap(),
            CellCoord::new(2, 0),
        );
        table.rows = vec![0];
        table.values = vec![PivotValue {
            field: 2,
            aggregate: Aggregate::Average,
        }];
        table.filters = vec![1];
        table.hidden.insert(1, ["Pen".to_string()].into());
        table.output = Some(CellRange::from_a1("A3:B9").unwrap());
        engine.formatting_mut(1).pivots.push(table.clone());

        let path = temp_xlsx("pivots");
        let mut writer = XlsxWriter::new();
        writer.add_engine_sheet("Data", &engine, 0).unwrap();
        writer.add_engine_sheet("Report", &engine, 1).unwrap();
        writer.save(&path).unwrap();
        let read = super::read_formatting_from_path(&path).unwrap();

        // The manifest's type is declared, as Excel requires.
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let mut types = String::new();
        zip.by_name("[Content_Types].xml")
            .unwrap()
            .read_to_string(&mut types)
            .unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(types.contains(r#"<Default Extension="json" ContentType="application/json"/>"#));
        assert!(read[0].1.pivots.is_empty());
        assert_eq!(read[1].1.pivots, vec![table]);
    }
}
