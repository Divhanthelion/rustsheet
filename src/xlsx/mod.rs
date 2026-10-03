#[cfg(feature = "xlsx")]
mod chart_reader;
#[cfg(feature = "xlsx")]
mod reader;
#[cfg(feature = "xlsx")]
mod writer;

#[cfg(feature = "xlsx")]
pub use chart_reader::{ChartReadError, ChartReader};
#[cfg(feature = "xlsx")]
pub use reader::XlsxReader;
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
}
