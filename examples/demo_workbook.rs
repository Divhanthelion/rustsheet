//! Write a sample workbook for store screenshots and manual testing.
//!
//! cargo run --example demo_workbook -- target/demo.xlsx [target/sales.xlsx]
//!
//! The optional second file is a filtered sales list.

use rustsheet::cell::{Axis, CellRange};
use rustsheet::format::AutoFilter;
use rustsheet::prelude::*;
use std::collections::BTreeMap;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo.xlsx".to_string());

    let mut e = CalcEngine::new();
    e.set_sheet_names(vec!["Budget".to_string()]);

    let at = |a1: &str| CellCoord::from_a1(a1).unwrap();
    let text = |e: &mut CalcEngine, a1: &str, s: &str| {
        e.set_value(0, at(a1), CellValueInput::Text(s.into()))
    };
    let num =
        |e: &mut CalcEngine, a1: &str, n: f64| e.set_value(0, at(a1), CellValueInput::Number(n));
    let formula = |e: &mut CalcEngine, a1: &str, f: &str| e.set_formula(0, at(a1), f).unwrap();

    text(&mut e, "A1", "Household budget, first half");
    for (col, header) in ["Month", "Income", "Expenses", "Savings", "Rate", "Notes"]
        .iter()
        .enumerate()
    {
        text(&mut e, &format!("{}2", (b'A' + col as u8) as char), header);
    }

    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
    let income = [4200.0, 4200.0, 4550.0, 4550.0, 4800.0, 5100.0];
    let expenses = [3100.0, 2950.0, 3400.0, 3050.0, 3200.0, 3350.0];
    let notes = [
        "",
        "",
        "Car service and new tires",
        "",
        "Raise starts",
        "Summer trip deposit paid",
    ];
    for (i, month) in months.iter().enumerate() {
        let row = i + 3;
        text(&mut e, &format!("A{row}"), month);
        num(&mut e, &format!("B{row}"), income[i]);
        num(&mut e, &format!("C{row}"), expenses[i]);
        formula(&mut e, &format!("D{row}"), &format!("=B{row}-C{row}"));
        formula(
            &mut e,
            &format!("E{row}"),
            &format!("=ROUND(D{row}/B{row},2)"),
        );
        if !notes[i].is_empty() {
            text(&mut e, &format!("F{row}"), notes[i]);
        }
    }

    text(&mut e, "A10", "Total");
    formula(&mut e, "B10", "=SUM(B3:B8)");
    formula(&mut e, "C10", "=SUM(C3:C8)");
    formula(&mut e, "D10", "=SUM(D3:D8)");
    formula(&mut e, "E10", "=ROUND(D10/B10,2)");
    text(&mut e, "A11", "Best month");
    formula(&mut e, "B11", "=INDEX(A3:A8,MATCH(MAX(D3:D8),D3:D8,0))");
    text(&mut e, "A12", "Average");
    formula(&mut e, "D12", "=AVERAGE(D3:D8)");

    // Formatting: a title, a header band, money and percent columns, a
    // totals rule, wrapped notes.
    let style = |e: &mut CalcEngine, cells: &str, f: &dyn Fn(&mut CellFormat)| {
        let r = CellRange::from_a1(cells).unwrap();
        for row in r.start.row..=r.end.row {
            for col in r.start.col..=r.end.col {
                let coord = CellCoord::new(row, col);
                let mut format = e.cell_format(0, coord).cloned().unwrap_or_default();
                f(&mut format);
                e.set_cell_format(0, coord, format);
            }
        }
    };
    style(&mut e, "A1", &|f| {
        f.bold = true;
        f.font_size = Some(16);
        f.font_name = Some("Georgia".into());
        f.h_align = HAlign::Center;
        f.v_align = rustsheet::format::VAlign::Center;
    });
    style(&mut e, "A2:F2", &|f| {
        f.bold = true;
        f.fill = Some(Rgb(0x2E, 0x7D, 0x46));
        f.font_color = Some(Rgb::WHITE);
    });
    style(&mut e, "B2:E2", &|f| f.h_align = HAlign::Right);
    style(&mut e, "B3:D10", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    style(&mut e, "D12", &|f| {
        f.number_format = Some("$#,##0.00".into())
    });
    style(&mut e, "E3:E10", &|f| f.number_format = Some("0%".into()));
    style(&mut e, "A10:E10", &|f| {
        f.bold = true;
        f.borders = Borders {
            top: true,
            ..Borders::NONE
        };
    });
    style(&mut e, "A11:A12", &|f| f.italic = true);
    {
        let f = e.formatting_mut(0);
        f.merges.push(CellRange::from_a1("A1:F1").unwrap());
        f.row_heights.insert(0, 34.0);
        f.column_widths.insert(0, 96.0);
        f.column_widths.insert(5, 170.0);
        f.set_line_format(
            Axis::Column,
            5,
            CellFormat {
                wrap: true,
                italic: true,
                font_color: Some(Rgb(0x59, 0x59, 0x59)),
                ..Default::default()
            },
        );
        f.frozen = (2, 0);
        f.filter = Some(AutoFilter {
            range: CellRange::from_a1("A2:F8").unwrap(),
            allowed: BTreeMap::new(),
        });
    }

    let range = |a1: &str| CellRange::from_a1(a1).unwrap();
    let chart = ChartDefinition::new(ChartKind::Bar)
        .with_title("Income vs. expenses")
        .with_series(
            ChartSeries::new(range("B3:B8"))
                .with_name("Income")
                .with_x_range(range("A3:A8"))
                .with_color(46, 125, 70, 255),
        )
        .with_series(
            ChartSeries::new(range("C3:C8"))
                .with_name("Expenses")
                .with_x_range(range("A3:A8"))
                .with_color(217, 83, 43, 255),
        )
        .with_overlay_area(ChartOverlayArea::new(2, 7, 480.0, 300.0));
    let charts = [chart];

    let mut writer = XlsxWriter::new();
    writer
        .add_engine_sheet_with_charts("Budget", &e, 0, &charts)
        .unwrap();
    writer.save_with_charts(&path, &charts).unwrap();
    println!("Wrote {path}");

    if let Some(sales) = std::env::args().nth(2) {
        write_sales(&sales);
        println!("Wrote {sales}");
    }
}

/// A sales list with a frozen header and an AutoFilter showing two regions.
fn write_sales(path: &str) {
    use std::collections::BTreeSet;
    let mut e = CalcEngine::new();
    e.set_sheet_names(vec!["Sales".to_string()]);
    let headers = [
        "Date", "Region", "Rep", "Product", "Units", "Price", "Revenue",
    ];
    for (c, h) in headers.iter().enumerate() {
        e.set_value(
            0,
            CellCoord::new(0, c as u32),
            CellValueInput::Text(h.to_string()),
        );
    }
    let regions = ["North", "South", "East", "West"];
    let reps = ["Avery", "Blake", "Casey", "Drew", "Emery", "Finley"];
    let products = [
        ("Notebook", 4.5),
        ("Pen set", 12.0),
        ("Desk lamp", 39.0),
        ("Backpack", 54.0),
    ];
    for i in 0..40u32 {
        let row = i + 1;
        let (product, price) = products[(i as usize * 7 + 3) % products.len()];
        let units = ((i * 37 + 11) % 48 + 2) as f64;
        let set = |e: &mut CalcEngine, c: u32, v: CellValueInput| {
            e.set_value(0, CellCoord::new(row, c), v)
        };
        set(&mut e, 0, CellValueInput::Number(46023.0 + (i / 2) as f64)); // Jan 2026
        set(
            &mut e,
            1,
            CellValueInput::Text(regions[(i as usize * 3) % 4].into()),
        );
        set(
            &mut e,
            2,
            CellValueInput::Text(reps[(i as usize * 5) % reps.len()].into()),
        );
        set(&mut e, 3, CellValueInput::Text(product.into()));
        set(&mut e, 4, CellValueInput::Number(units));
        set(&mut e, 5, CellValueInput::Number(price));
        e.set_formula(
            0,
            CellCoord::new(row, 6),
            &format!("=E{}*F{}", row + 1, row + 1),
        )
        .unwrap();
    }
    let header = CellFormat {
        bold: true,
        fill: Some(Rgb(0x1F, 0x4E, 0x79)),
        font_color: Some(Rgb::WHITE),
        ..Default::default()
    };
    for c in 0..7 {
        e.set_cell_format(0, CellCoord::new(0, c), header.clone());
    }
    {
        let f = e.formatting_mut(0);
        let date = CellFormat {
            number_format: Some("mmm d, yyyy".into()),
            ..Default::default()
        };
        let money = CellFormat {
            number_format: Some("$#,##0.00".into()),
            ..Default::default()
        };
        f.set_line_format(Axis::Column, 0, date);
        f.set_line_format(Axis::Column, 5, money.clone());
        f.set_line_format(
            Axis::Column,
            6,
            CellFormat {
                bold: true,
                ..money
            },
        );
        f.column_widths.insert(0, 110.0);
        f.column_widths.insert(3, 100.0);
        f.frozen = (1, 0);
        let mut allowed = BTreeMap::new();
        allowed.insert(1, BTreeSet::from(["North".to_string(), "West".to_string()]));
        f.filter = Some(AutoFilter {
            range: CellRange::from_a1("A1:G41").unwrap(),
            allowed,
        });
    }
    e.refresh_filter(0);
    // Header formats take precedence over the column formats.
    for c in 0..7 {
        e.set_cell_format(0, CellCoord::new(0, c), header.clone());
    }
    let mut writer = XlsxWriter::new();
    writer.add_engine_sheet("Sales", &e, 0).unwrap();
    writer.save(path).unwrap();
}
