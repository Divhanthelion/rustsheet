//! Write a sample workbook for store screenshots and manual testing.
//!
//! cargo run --example demo_workbook -- target/demo.xlsx [target/sales.xlsx [target/pivot.xlsx [target/loan.xlsx]]]
//!
//! The optional second file is a filtered sales list, the third a
//! PivotTable summarizing it, and the fourth a mortgage comparison built
//! on the financial functions.

use rustsheet::cell::{Axis, CellRange};
use rustsheet::format::AutoFilter;
use rustsheet::format::conditional::{CfRule, CfStyle, Cfvo, ConditionalFormat};
use rustsheet::format::validation::CompareOp;
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
        // Data bars for savings, a red-to-green scale for the savings rate.
        f.conditional = vec![
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("D3:D8").unwrap()],
                rule: CfRule::DataBar {
                    min: Cfvo::min(),
                    max: Cfvo::max(),
                    color: Rgb(0x63, 0x8E, 0xC6),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("E3:E8").unwrap()],
                rule: CfRule::ColorScale {
                    stops: vec![
                        (Cfvo::min(), Rgb(0xF8, 0x69, 0x6B)),
                        (Cfvo::percentile(50), Rgb(0xFF, 0xEB, 0x84)),
                        (Cfvo::max(), Rgb(0x63, 0xBE, 0x7B)),
                    ],
                },
                stop_if_true: false,
            },
        ];
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
    if let Some(pivot) = std::env::args().nth(3) {
        write_pivot(&pivot);
        println!("Wrote {pivot}");
    }
    if let Some(loan) = std::env::args().nth(4) {
        write_finance(&loan);
        println!("Wrote {loan}");
    }
}

/// Two mortgage offers side by side: PMT, CUMIPMT, EDATE, IF/TEXT/&, the
/// `%` operator, and a line chart of the balances from FV.
fn write_finance(path: &str) {
    let mut e = CalcEngine::new();
    e.set_sheet_names(vec!["Mortgage".to_string()]);

    let at = |a1: &str| CellCoord::from_a1(a1).unwrap();
    let text = |e: &mut CalcEngine, a1: &str, s: &str| {
        e.set_value(0, at(a1), CellValueInput::Text(s.into()))
    };
    let num =
        |e: &mut CalcEngine, a1: &str, n: f64| e.set_value(0, at(a1), CellValueInput::Number(n));
    let formula = |e: &mut CalcEngine, a1: &str, f: &str| e.set_formula(0, at(a1), f).unwrap();
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

    text(&mut e, "A1", "Compare two mortgage offers");
    text(&mut e, "A3", "Home price");
    num(&mut e, "B3", 420_000.0);
    text(&mut e, "A4", "Down payment");
    formula(&mut e, "B4", "=B3*20%");
    text(&mut e, "A5", "Loan amount");
    formula(&mut e, "B5", "=B3-B4");

    text(&mut e, "B7", "Offer A");
    text(&mut e, "C7", "Offer B");
    let rows: [(&str, Option<&str>); 6] = [
        ("Rate", None),
        ("Years", None),
        ("Monthly payment", Some("=PMT({c}8/12,{c}9*12,-$B$5)")),
        ("Total interest", Some("={c}10*{c}9*12-$B$5")),
        (
            "Interest, first year",
            Some("=-CUMIPMT({c}8/12,{c}9*12,$B$5,1,12,0)"),
        ),
        ("Paid off", Some("=EDATE(DATE(2026,12,1),{c}9*12)")),
    ];
    num(&mut e, "B8", 0.0549);
    num(&mut e, "C8", 0.0615);
    num(&mut e, "B9", 30.0);
    num(&mut e, "C9", 15.0);
    for (i, (label, f)) in rows.iter().enumerate() {
        let row = i + 8;
        text(&mut e, &format!("A{row}"), label);
        if let Some(f) = f {
            for col in ["B", "C"] {
                formula(&mut e, &format!("{col}{row}"), &f.replace("{c}", col));
            }
        }
    }
    formula(
        &mut e,
        "A15",
        "=IF(B11<C11,\"Offer A\",\"Offer B\")&\" pays \"&TEXT(ABS(B11-C11),\"$#,##0\")&\" less interest over the life of the loan\"",
    );

    // Balance by year, for the chart: FV of the loan after k years of
    // payments, floored at paid off.
    text(&mut e, "A18", "Year");
    text(&mut e, "B18", "Offer A");
    text(&mut e, "C18", "Offer B");
    for year in 0..=30u32 {
        let row = year + 19;
        num(&mut e, &format!("A{row}"), year as f64);
        for col in ["B", "C"] {
            formula(
                &mut e,
                &format!("{col}{row}"),
                &format!("=MAX(0,-FV({col}$8/12,A{row}*12,-{col}$10,$B$5))"),
            );
        }
    }

    style(&mut e, "A1", &|f| {
        f.bold = true;
        f.font_size = Some(16);
        f.font_name = Some("Georgia".into());
        f.h_align = HAlign::Center;
        f.v_align = rustsheet::format::VAlign::Center;
    });
    style(&mut e, "B7:C7", &|f| {
        f.bold = true;
        f.fill = Some(Rgb(0x1F, 0x4E, 0x79));
        f.font_color = Some(Rgb::WHITE);
        f.h_align = HAlign::Right;
    });
    style(&mut e, "B3:C5", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    style(&mut e, "B8:C8", &|f| f.number_format = Some("0.00%".into()));
    style(&mut e, "B10:C11", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    style(&mut e, "B12:C12", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    style(&mut e, "B10:C10", &|f| f.bold = true);
    style(&mut e, "B13:C13", &|f| {
        f.number_format = Some("mmm yyyy".into());
        f.h_align = HAlign::Right;
    });
    style(&mut e, "A15", &|f| {
        f.italic = true;
        f.font_color = Some(Rgb(0x59, 0x59, 0x59));
    });
    style(&mut e, "A18:C18", &|f| f.bold = true);
    style(&mut e, "B19:C49", &|f| {
        f.number_format = Some("$#,##0".into())
    });
    {
        let f = e.formatting_mut(0);
        f.merges.push(CellRange::from_a1("A1:F1").unwrap());
        f.merges.push(CellRange::from_a1("A15:E15").unwrap());
        f.row_heights.insert(0, 34.0);
        f.column_widths.insert(0, 150.0);
    }

    let range = |a1: &str| CellRange::from_a1(a1).unwrap();
    let chart = ChartDefinition::new(ChartKind::Line)
        .with_title("Balance remaining")
        .with_series(
            ChartSeries::new(range("B19:B49"))
                .with_name("Offer A, 30 years")
                .with_x_range(range("A19:A49"))
                .with_color(46, 125, 70, 255),
        )
        .with_series(
            ChartSeries::new(range("C19:C49"))
                .with_name("Offer B, 15 years")
                .with_x_range(range("A19:A49"))
                .with_color(217, 83, 43, 255),
        )
        .with_overlay_area(ChartOverlayArea::new(2, 5, 520.0, 320.0));
    let charts = [chart];

    let mut writer = XlsxWriter::new();
    writer
        .add_engine_sheet_with_charts("Mortgage", &e, 0, &charts)
        .unwrap();
    writer.save_with_charts(path, &charts).unwrap();
}

/// Forty orders on `sheet`, with a styled header and a frozen top row.
fn fill_sales(e: &mut CalcEngine, sheet: u32) {
    let headers = [
        "Date", "Region", "Rep", "Product", "Units", "Price", "Revenue",
    ];
    for (c, h) in headers.iter().enumerate() {
        e.set_value(
            sheet,
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
            e.set_value(sheet, CellCoord::new(row, c), v)
        };
        set(e, 0, CellValueInput::Number(46023.0 + (i / 2) as f64)); // Jan 2026
        set(
            e,
            1,
            CellValueInput::Text(regions[(i as usize * 3) % 4].into()),
        );
        set(
            e,
            2,
            CellValueInput::Text(reps[(i as usize * 5) % reps.len()].into()),
        );
        set(e, 3, CellValueInput::Text(product.into()));
        set(e, 4, CellValueInput::Number(units));
        set(e, 5, CellValueInput::Number(price));
        e.set_formula(
            sheet,
            CellCoord::new(row, 6),
            &format!("=E{}*F{}", row + 1, row + 1),
        )
        .unwrap();
    }
    let f = e.formatting_mut(sheet);
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
    // Header formats take precedence over the column formats.
    let header = CellFormat {
        bold: true,
        fill: Some(Rgb(0x1F, 0x4E, 0x79)),
        font_color: Some(Rgb::WHITE),
        ..Default::default()
    };
    for c in 0..7 {
        e.set_cell_format(sheet, CellCoord::new(0, c), header.clone());
    }
}

/// A sales list with a frozen header, highlights and an AutoFilter showing
/// two regions.
fn write_sales(path: &str) {
    use std::collections::BTreeSet;
    let mut e = CalcEngine::new();
    e.set_sheet_names(vec!["Sales".to_string()]);
    fill_sales(&mut e, 0);
    {
        let f = e.formatting_mut(0);
        // Best sellers in green, small orders in red.
        f.conditional = vec![
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("G2:G41").unwrap()],
                rule: CfRule::Top {
                    bottom: false,
                    rank: 5,
                    percent: false,
                    style: CfStyle::preset(2),
                },
                stop_if_true: false,
            },
            ConditionalFormat {
                ranges: vec![CellRange::from_a1("E2:E41").unwrap()],
                rule: CfRule::CellIs {
                    op: CompareOp::Less,
                    formula1: "10".into(),
                    formula2: None,
                    style: CfStyle::preset(0),
                },
                stop_if_true: false,
            },
        ];
        let mut allowed = BTreeMap::new();
        allowed.insert(1, BTreeSet::from(["North".to_string(), "West".to_string()]));
        f.filter = Some(AutoFilter {
            range: CellRange::from_a1("A1:G41").unwrap(),
            allowed,
        });
    }
    e.refresh_filter(0);
    let mut writer = XlsxWriter::new();
    writer.add_engine_sheet("Sales", &e, 0).unwrap();
    writer.save(path).unwrap();
}

/// Revenue by rep and region, with data bars on the totals, from the
/// sales list on a second sheet.
fn write_pivot(path: &str) {
    use rustsheet::pivot::{Aggregate, PivotTable, PivotValue};
    let mut e = CalcEngine::new();
    e.set_sheet_names(vec!["Summary".to_string(), "Sales".to_string()]);
    fill_sales(&mut e, 1);
    let mut table = PivotTable::new(
        "Revenue by rep".into(),
        "Sales".into(),
        CellRange::from_a1("A1:G41").unwrap(),
        CellCoord::new(0, 0),
    );
    table.rows = vec![2];
    table.columns = vec![1];
    table.values = vec![PivotValue {
        field: 6,
        aggregate: Aggregate::Sum,
    }];
    table.filters = vec![3];
    e.formatting_mut(0).pivots.push(table);
    e.refresh_pivot(0, 0).unwrap();
    // Data bars down the Grand Total column, beside the reps.
    let out = e.formatting(0).unwrap().pivots[0].output.unwrap();
    let totals = CellRange::new(
        CellCoord::new(out.start.row + 4, out.end.col),
        CellCoord::new(out.end.row - 1, out.end.col),
    );
    e.formatting_mut(0).conditional = vec![ConditionalFormat {
        ranges: vec![totals],
        rule: CfRule::DataBar {
            min: Cfvo::min(),
            max: Cfvo::max(),
            color: Rgb(0x63, 0x8E, 0xC6),
        },
        stop_if_true: false,
    }];
    let mut writer = XlsxWriter::new();
    writer.add_engine_sheet("Summary", &e, 0).unwrap();
    writer.add_engine_sheet("Sales", &e, 1).unwrap();
    writer.save(path).unwrap();
}
