//! Properties: round trips that hold for any input, and code that takes
//! anything a file can hold without panicking. Case counts stay small so
//! the suite runs in a few seconds in debug builds.

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use rustsheet::calc::{CalcEngine, CellResult, CellValueInput};
use rustsheet::cell::{CellCoord, CellError, CellRange, MAX_COL, MAX_ROW};
use rustsheet::formula::{
    BinaryOp, CellRef, Expr, FormulaParser, FunctionCall, RangeKind, RangeRef, UnaryOp,
};

/// `cases` per property unless PROPTEST_CASES asks for a deeper run.
/// Failures are kept next to this file (`proptests.proptest-regressions`)
/// and replayed first.
fn config(cases: u32) -> ProptestConfig {
    let env = std::env::var("PROPTEST_CASES").ok();
    ProptestConfig {
        cases: env.and_then(|n| n.parse().ok()).unwrap_or(cases),
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))),
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------------------
// Formulas: parse(display(ast)) == ast
// ---------------------------------------------------------------------------

/// Numbers as the parser makes them: never negative (a minus is a unary
/// operator), and finite.
fn number() -> impl Strategy<Value = f64> {
    prop_oneof![
        (0u32..100_000).prop_map(f64::from),
        // .5, .25, .125: halves and eighths, and their whole parts
        (0u32..10_000).prop_map(|n| f64::from(n) / 8.0),
        // Decimals that aren't exact in binary
        (0u32..10_000).prop_map(|n| f64::from(n) / 10.0),
        1e-12f64..1.0,
        0.0f64..1e18,
    ]
}

fn text() -> impl Strategy<Value = String> {
    // Quotes to escape, parentheses and quotes check_size must skip,
    // operators, and characters outside ASCII.
    "[a-zA-Z0-9 \"'(),;:!%&*+=<>$#é€😀\t\n-]{0,12}"
}

fn error() -> impl Strategy<Value = CellError> {
    prop::sample::select(vec![
        CellError::Null,
        CellError::DivZero,
        CellError::Value,
        CellError::Ref,
        CellError::Name,
        CellError::Num,
        CellError::NA,
        CellError::Calc,
        CellError::Spill,
    ])
}

/// Sheet names a formula can name: bare ones, ones that need quotes
/// (spaces, quotes, punctuation, non-ASCII), names that start with a digit
/// ("2024", "1Q.2024"), and names Excel would take for a cell or R1C1
/// address ("A1", "R2C3").
fn sheet() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        3 => Just(None),
        1 => "[A-Za-z_][A-Za-z0-9_.]{0,8}".prop_map(Some),
        1 => "[A-Za-z][A-Za-z0-9 '!.&()é-]{0,10}".prop_map(Some),
        1 => "[0-9][A-Za-z0-9_. ]{0,6}".prop_map(Some),
        1 => "([A-Za-z]{1,3}[0-9]{1,7}|[RrCc][0-9]{0,3}|[Rr][0-9]{0,3}[Cc][0-9]{0,3})".prop_map(Some),
    ]
}

fn coord() -> impl Strategy<Value = CellCoord> {
    prop_oneof![
        (0..=MAX_ROW, 0..=MAX_COL),
        (0u32..100, 0u32..30),
        Just((MAX_ROW, MAX_COL)),
    ]
    .prop_map(|(row, col)| CellCoord::new(row, col))
}

fn cell_ref() -> impl Strategy<Value = Expr> {
    (sheet(), coord(), any::<bool>(), any::<bool>()).prop_map(
        |(sheet, coord, row_absolute, col_absolute)| {
            Expr::CellRef(CellRef {
                sheet,
                coord,
                row_absolute,
                col_absolute,
            })
        },
    )
}

/// Ranges as the parser builds them: corners in order, and whole columns
/// and rows spanning the sheet with `$` only on their own axis.
fn range_ref() -> impl Strategy<Value = Expr> {
    let cells = (coord(), coord(), any::<[bool; 4]>()).prop_map(|(a, b, abs)| {
        (
            CellRange::new(a, b),
            (abs[0], abs[1]),
            (abs[2], abs[3]),
            RangeKind::Cells,
        )
    });
    let columns = (0..=MAX_COL, 0..=MAX_COL, any::<[bool; 2]>()).prop_map(|(a, b, abs)| {
        (
            CellRange::new(
                CellCoord::new(0, a.min(b)),
                CellCoord::new(MAX_ROW, a.max(b)),
            ),
            (false, abs[0]),
            (false, abs[1]),
            RangeKind::Columns,
        )
    });
    let rows = (0..=MAX_ROW, 0..=MAX_ROW, any::<[bool; 2]>()).prop_map(|(a, b, abs)| {
        (
            CellRange::new(
                CellCoord::new(a.min(b), 0),
                CellCoord::new(a.max(b), MAX_COL),
            ),
            (abs[0], false),
            (abs[1], false),
            RangeKind::Rows,
        )
    });
    (sheet(), prop_oneof![cells, columns, rows]).prop_map(
        |(sheet, (range, start_absolute, end_absolute, kind))| {
            Expr::RangeRef(RangeRef {
                sheet,
                range,
                start_absolute,
                end_absolute,
                kind,
            })
        },
    )
}

/// Array constants: up to 3 x 3 literals, where numbers may be negative.
fn array() -> impl Strategy<Value = Expr> {
    let item = prop_oneof![
        number().prop_map(Expr::Number),
        number().prop_map(|n| Expr::Number(-n)),
        text().prop_map(Expr::Text),
        any::<bool>().prop_map(Expr::Bool),
        error().prop_map(Expr::Error),
    ];
    (1usize..4, prop::collection::vec(item, 1..10)).prop_map(|(cols, items)| {
        let cols = cols.min(items.len());
        let rows = items.len() / cols;
        Expr::Array(
            items[..rows * cols]
                .chunks(cols)
                .map(<[Expr]>::to_vec)
                .collect(),
        )
    })
}

fn leaf() -> impl Strategy<Value = Expr> {
    prop_oneof![
        number().prop_map(Expr::Number),
        text().prop_map(Expr::Text),
        any::<bool>().prop_map(Expr::Bool),
        error().prop_map(Expr::Error),
        cell_ref(),
        range_ref(),
        array(),
    ]
}

/// A function argument: an expression, or now and then an empty slot.
fn arg(inner: impl Strategy<Value = Expr>) -> impl Strategy<Value = Expr> {
    prop_oneof![5 => inner, 1 => Just(Expr::Missing)]
}

fn expr() -> impl Strategy<Value = Expr> {
    let binary = prop::sample::select(vec![
        BinaryOp::Add,
        BinaryOp::Sub,
        BinaryOp::Mul,
        BinaryOp::Div,
        BinaryOp::Pow,
        BinaryOp::Concat,
        BinaryOp::Eq,
        BinaryOp::Neq,
        BinaryOp::Lt,
        BinaryOp::Lte,
        BinaryOp::Gt,
        BinaryOp::Gte,
    ]);
    let unary = prop::sample::select(vec![UnaryOp::Neg, UnaryOp::Pos, UnaryOp::Percent]);
    let name = prop::sample::select(vec!["SUM", "IF", "MAX", "CONCAT", "LOG10", "T.TEST", "NOW"]);
    leaf().prop_recursive(4, 48, 4, move |inner| {
        prop_oneof![
            (binary.clone(), inner.clone(), inner.clone())
                .prop_map(|(op, l, r)| Expr::binary(op, l, r)),
            (unary.clone(), inner.clone()).prop_map(|(op, e)| Expr::unary(op, e)),
            (name.clone(), prop::collection::vec(arg(inner), 0..4)).prop_map(|(name, args)| {
                // A lone empty slot is no argument: F() has none.
                let args = if args == [Expr::Missing] {
                    Vec::new()
                } else {
                    args
                };
                Expr::Function(FunctionCall {
                    name: name.to_string(),
                    args,
                })
            }),
        ]
    })
}

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn formulas_survive_display_and_parse(ast in expr()) {
        let shown = format!("={ast}");
        let parsed = FormulaParser::new().parse(&shown);
        prop_assert!(parsed.is_ok(), "{shown} doesn't parse: {:?}", parsed.err());
        let parsed = parsed.unwrap();
        prop_assert_eq!(&parsed, &ast, "{}", shown);
        prop_assert_eq!(format!("={parsed}"), shown);
    }

    /// Every way to write a number reads back as that number: 0.5, .5,
    /// 5E-1, 5e-1.
    #[test]
    fn number_spellings_parse_to_the_same_value(n in number()) {
        let parser = FormulaParser::new();
        let mut spellings = vec![format!("{n}"), format!("{n:e}"), format!("{n:E}")];
        if let Some(fraction) = spellings[0].strip_prefix("0.") {
            spellings.push(format!(".{fraction}"));
        }
        for s in spellings {
            prop_assert_eq!(parser.parse(&format!("={s}")).ok(), Some(Expr::Number(n)), "{}", s);
        }
    }
}

// ---------------------------------------------------------------------------
// Number formats and colors: any code a file holds, any value
// ---------------------------------------------------------------------------

/// Format codes from the pieces Excel's are made of, in any order.
fn format_code() -> impl Strategy<Value = String> {
    let piece = prop::sample::select(vec![
        "0",
        "#",
        "?",
        ".",
        ",",
        "%",
        "E+",
        "e-",
        "/",
        " ",
        "\"lit\"",
        "\"",
        "\\x",
        "_)",
        "*-",
        "[Red]",
        "[h]",
        "[mm]",
        "[ss]",
        "[$€-407]",
        "[$-409]",
        "[>100]",
        "[",
        "]",
        "yyyy",
        "yy",
        "mmmm",
        "mmmmm",
        "mmm",
        "mm",
        "m",
        "dddd",
        "ddd",
        "dd",
        "d",
        "hh",
        "h",
        "ss",
        "s",
        ".000",
        "AM/PM",
        "A/P",
        ";",
        "@",
        "General",
        "?/?",
        "# ??/??",
        "0.00E+00",
    ]);
    prop_oneof![
        prop::collection::vec(piece, 0..12).prop_map(|p| p.concat()),
        ".{0,24}",
    ]
}

fn any_number() -> impl Strategy<Value = f64> {
    prop_oneof![
        any::<f64>(),
        -1e6f64..1e6,
        // Dates from 1900 to 9999 and a little either side
        -10.0f64..3e6,
        Just(f64::MAX),
        Just(f64::MIN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(f64::NAN),
        Just(-0.0),
        Just(f64::MIN_POSITIVE / 2.0),
    ]
}

proptest! {
    #![proptest_config(config(512))]

    #[test]
    fn number_formats_take_any_code_and_value(code in format_code(), n in any_number()) {
        let _ = rustsheet::format::format_number(n, &code);
        let _ = rustsheet::format::is_date_format(&code);
    }

    #[test]
    fn general_format_takes_any_value_and_width(n in any_number(), width in 0usize..20) {
        let _ = rustsheet::format::format_general(n, width);
    }

    #[test]
    fn hex_colors_take_any_string(s in ".{0,10}") {
        let _ = rustsheet::format::Rgb::from_hex(&s);
    }
}

// ---------------------------------------------------------------------------
// A1 references
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn a1_references_round_trip(row in 0..=MAX_ROW, col in 0..=MAX_COL, abs in any::<(bool, bool)>()) {
        let c = CellCoord::new(row, col);
        prop_assert_eq!(CellCoord::from_a1(&c.to_a1()), Some(c));
        prop_assert_eq!(CellCoord::from_a1(&c.to_a1_abs(abs.0, abs.1)), Some(c));
        prop_assert_eq!(CellCoord::from_a1(&c.to_a1().to_lowercase()), Some(c));
    }

    #[test]
    fn a1_ranges_round_trip(a in coord(), b in coord()) {
        let r = CellRange::new(a, b);
        prop_assert_eq!(CellRange::from_a1(&r.to_a1()), Some(r));
    }

    #[test]
    fn a1_parsing_takes_any_string(s in ".{0,12}") {
        let _ = CellCoord::from_a1(&s);
        let _ = CellRange::from_a1(&s);
    }
}

// ---------------------------------------------------------------------------
// Grids of values through CSV and .xlsx
// ---------------------------------------------------------------------------

/// A value a cell can hold that a file can carry back unchanged.
#[derive(Clone, Debug)]
enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
}

impl Value {
    fn input(&self) -> CellValueInput {
        match self {
            Value::Number(n) => CellValueInput::Number(*n),
            Value::Text(s) => CellValueInput::Text(s.clone()),
            Value::Bool(b) => CellValueInput::Bool(*b),
        }
    }

    fn result(&self) -> CellResult {
        match self {
            Value::Number(n) => CellResult::Value(*n),
            Value::Text(s) => CellResult::Text(s.clone()),
            Value::Bool(b) => CellResult::Bool(*b),
        }
    }
}

fn finite() -> impl Strategy<Value = f64> {
    prop_oneof![
        any::<f64>().prop_filter("finite", |n| n.is_finite()),
        -1e6f64..1e6,
        (-1000i32..1000).prop_map(f64::from),
    ]
}

/// Text CSV reads back as text: not a number, TRUE/FALSE or a formula, and
/// not empty (an empty field is an empty cell).
fn csv_text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 ,;\"'\r\n\t=.+-]{1,12}",
        "\\PC{1,8}",
        Just("a,\"b\"\r\nc".to_string()),
    ]
    .prop_filter("reads back as text", |s| {
        !s.is_empty()
            && !s.starts_with('=')
            && s.parse::<f64>().is_err()
            && !s.eq_ignore_ascii_case("true")
            && !s.eq_ignore_ascii_case("false")
    })
}

fn grid(value: impl Strategy<Value = Value>) -> impl Strategy<Value = Vec<Vec<Option<Value>>>> {
    let cell = prop::option::weighted(0.8, value);
    prop::collection::vec(prop::collection::vec(cell, 1..6), 1..6)
}

fn fill(grid: &[Vec<Option<Value>>]) -> CalcEngine {
    let mut engine = CalcEngine::new();
    for (r, row) in grid.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if let Some(v) = v {
                engine.set_value(0, CellCoord::new(r as u32, c as u32), v.input());
            }
        }
    }
    engine
}

fn assert_same(grid: &[Vec<Option<Value>>], loaded: &CalcEngine) -> Result<(), TestCaseError> {
    for (r, row) in grid.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let coord = CellCoord::new(r as u32, c as u32);
            let want = v.as_ref().map_or(CellResult::Empty, Value::result);
            prop_assert_eq!(loaded.get_value(0, coord), want, "at {}", coord);
        }
    }
    Ok(())
}

#[cfg(feature = "csv")]
proptest! {
    #![proptest_config(config(256))]

    #[test]
    fn csv_round_trips_values_and_text(grid in grid(prop_oneof![
        finite().prop_map(Value::Number),
        csv_text().prop_map(Value::Text),
        any::<bool>().prop_map(Value::Bool),
    ])) {
        let engine = fill(&grid);
        let mut buf = Vec::new();
        rustsheet::csv_io::write_sheet(&engine, 0, &mut buf).unwrap();
        let mut loaded = CalcEngine::new();
        rustsheet::csv_io::read_sheet(&mut loaded, 0, buf.as_slice()).unwrap();
        assert_same(&grid, &loaded)?;
    }
}

#[cfg(feature = "xlsx")]
mod xlsx {
    use super::*;
    use rustsheet::format::{Borders, CellFormat, HAlign, Rgb, VAlign};
    use rustsheet::xlsx::{XlsxReader, XlsxWriter, read_formatting_from_path};

    /// Text that survives XML: no control characters but tab and newline.
    /// Not empty: rust_xlsxwriter writes "" as a blank cell, as Excel does.
    fn xml_text() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-zA-Z0-9 &<>\"'\t\n=.,;+-]{1,16}",
            "\\PC{1,8}",
            Just("  padded  ".to_string()),
        ]
    }

    /// Formats that read back as written: no font size, name or color that
    /// matches the workbook default (those read back as unset).
    fn format() -> impl Strategy<Value = CellFormat> {
        let flags = any::<[bool; 5]>();
        let number_format = prop::option::of(prop::sample::select(vec![
            "0.00",
            "0%",
            "#,##0",
            "yyyy-mm-dd",
            "\"$\"#,##0.00",
            "@",
        ]));
        let fill = prop::option::of(prop::sample::select(vec![
            Rgb(0xFF, 0xF2, 0xCC),
            Rgb(0x1F, 0x4E, 0x79),
            Rgb(0, 0xB0, 0x50),
        ]));
        let h_align = prop::sample::select(vec![
            HAlign::General,
            HAlign::Left,
            HAlign::Center,
            HAlign::Right,
        ]);
        let v_align = prop::sample::select(vec![VAlign::Bottom, VAlign::Center, VAlign::Top]);
        let borders = prop::sample::select(vec![Borders::NONE, Borders::ALL]);
        let size = prop::option::of(prop::sample::select(vec![8u8, 14, 20]));
        (flags, number_format, fill, h_align, v_align, borders, size).prop_map(
            |(f, number_format, fill, h_align, v_align, borders, font_size)| CellFormat {
                bold: f[0],
                italic: f[1],
                underline: f[2],
                strikethrough: f[3],
                wrap: f[4],
                number_format: number_format.map(str::to_string),
                fill,
                h_align,
                v_align,
                borders,
                font_size,
                ..Default::default()
            },
        )
    }

    fn temp_path() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "rustsheet_prop_{}_{}.xlsx",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    proptest! {
        // Each case writes and reads a file.
        #![proptest_config(config(48))]

        #[test]
        fn xlsx_round_trips_values_and_formats(
            grid in grid(prop_oneof![
                finite().prop_map(Value::Number),
                xml_text().prop_map(Value::Text),
                any::<bool>().prop_map(Value::Bool),
            ]),
            formats in prop::collection::vec((0u32..6, 0u32..6, format()), 0..4),
        ) {
            let mut engine = fill(&grid);
            for (row, col, f) in &formats {
                engine.set_cell_format(0, CellCoord::new(*row, *col), f.clone());
            }
            let path = temp_path();
            let mut writer = XlsxWriter::new();
            writer.add_engine_sheet("Sheet1", &engine, 0).unwrap();
            writer.save(&path).unwrap();

            let mut loaded = CalcEngine::new();
            let read = XlsxReader::open(&path)
                .and_then(|mut r| r.read_into_engine("Sheet1", &mut loaded, 0));
            let formatting = read_formatting_from_path(&path);
            let _ = std::fs::remove_file(&path);
            read.unwrap();
            assert_same(&grid, &loaded)?;

            let formatting = &formatting.unwrap()[0].1;
            for (row, col, _) in &formats {
                let coord = CellCoord::new(*row, *col);
                prop_assert_eq!(formatting.get(coord), engine.cell_format(0, coord), "at {}", coord);
            }
        }
    }
}
