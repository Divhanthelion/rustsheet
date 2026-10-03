//! Write a sample workbook for store screenshots and manual testing.
//!
//! cargo run --example demo_workbook -- target/demo.xlsx

use rustsheet::prelude::*;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo.xlsx".to_string());

    let mut engine = CalcEngine::new();
    engine.set_sheet_names(vec!["Budget".to_string()]);

    let text = |engine: &mut CalcEngine, a1: &str, s: &str| {
        engine.set_value(
            0,
            CellCoord::from_a1(a1).unwrap(),
            CellValueInput::Text(s.into()),
        )
    };
    let num = |engine: &mut CalcEngine, a1: &str, n: f64| {
        engine.set_value(
            0,
            CellCoord::from_a1(a1).unwrap(),
            CellValueInput::Number(n),
        )
    };
    let formula = |engine: &mut CalcEngine, a1: &str, f: &str| {
        engine
            .set_formula(0, CellCoord::from_a1(a1).unwrap(), f)
            .unwrap()
    };

    for (col, header) in ["Month", "Income", "Expenses", "Savings", "Rate"]
        .iter()
        .enumerate()
    {
        text(
            &mut engine,
            &format!("{}1", (b'A' + col as u8) as char),
            header,
        );
    }

    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
    let income = [4200.0, 4200.0, 4550.0, 4550.0, 4800.0, 5100.0];
    let expenses = [3100.0, 2950.0, 3400.0, 3050.0, 3200.0, 3350.0];
    for (i, month) in months.iter().enumerate() {
        let row = i + 2;
        text(&mut engine, &format!("A{row}"), month);
        num(&mut engine, &format!("B{row}"), income[i]);
        num(&mut engine, &format!("C{row}"), expenses[i]);
        formula(&mut engine, &format!("D{row}"), &format!("=B{row}-C{row}"));
        formula(
            &mut engine,
            &format!("E{row}"),
            &format!("=ROUND(D{row}/B{row},2)"),
        );
    }

    text(&mut engine, "A9", "Total");
    formula(&mut engine, "B9", "=SUM(B2:B7)");
    formula(&mut engine, "C9", "=SUM(C2:C7)");
    formula(&mut engine, "D9", "=SUM(D2:D7)");
    formula(&mut engine, "E9", "=ROUND(D9/B9,2)");
    text(&mut engine, "A10", "Best month");
    formula(
        &mut engine,
        "B10",
        "=INDEX(A2:A7,MATCH(MAX(D2:D7),D2:D7,0))",
    );
    text(&mut engine, "A11", "Average");
    formula(&mut engine, "D11", "=AVERAGE(D2:D7)");

    // Formatting: a header band, money and percent formats, a totals rule.
    let style = |engine: &mut CalcEngine, cells: &str, f: &dyn Fn(&mut CellFormat)| {
        let r = CellRange::from_a1(cells).unwrap();
        for row in r.start.row..=r.end.row {
            for col in r.start.col..=r.end.col {
                let coord = CellCoord::new(row, col);
                let mut format = engine.cell_format(0, coord).cloned().unwrap_or_default();
                f(&mut format);
                engine.set_cell_format(0, coord, format);
            }
        }
    };
    style(&mut engine, "A1:E1", &|f| {
        f.bold = true;
        f.fill = Some(Rgb(0x2E, 0x7D, 0x46));
        f.font_color = Some(Rgb::WHITE);
    });
    style(&mut engine, "B1:E1", &|f| f.h_align = HAlign::Right);
    style(&mut engine, "B2:D9", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    style(&mut engine, "D11", &|f| {
        f.number_format = Some("$#,##0.00".into())
    });
    style(&mut engine, "E2:E9", &|f| {
        f.number_format = Some("0%".into())
    });
    style(&mut engine, "A9:E9", &|f| {
        f.bold = true;
        f.borders = Borders {
            top: true,
            ..Borders::NONE
        };
    });
    style(&mut engine, "A10:A11", &|f| f.italic = true);
    engine.formatting_mut(0).column_widths.insert(0, 96.0);

    let range = |a1: &str| CellRange::from_a1(a1).unwrap();
    let chart = ChartDefinition::new(ChartKind::Bar)
        .with_title("Income vs. expenses")
        .with_series(
            ChartSeries::new(range("B2:B7"))
                .with_name("Income")
                .with_x_range(range("A2:A7"))
                .with_color(46, 125, 70, 255),
        )
        .with_series(
            ChartSeries::new(range("C2:C7"))
                .with_name("Expenses")
                .with_x_range(range("A2:A7"))
                .with_color(217, 83, 43, 255),
        )
        .with_overlay_area(ChartOverlayArea::new(1, 6, 520.0, 320.0));
    let charts = [chart];

    let mut writer = XlsxWriter::new();
    writer
        .add_engine_sheet_with_charts("Budget", &engine, 0, &charts)
        .unwrap();
    writer.save_with_charts(&path, &charts).unwrap();
    println!("Wrote {path}");
}
