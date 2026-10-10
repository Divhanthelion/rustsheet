//! Throughput of the grid, the formula parser and recalculation.
//!
//! cargo bench --bench grid_benchmark [-- <filter>]

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use rustsheet::calc::{CalcEngine, CellResult, CellValueInput};
use rustsheet::cell::{CellCoord, CellValue};
use rustsheet::formula::FormulaParser;
use rustsheet::grid::SparseGrid;
use std::hint::black_box;

const GRID_CELLS: u32 = 100_000;

/// A block 100 columns wide, filled row by row like typed or imported data.
fn block(i: u32) -> CellCoord {
    CellCoord::new(i / 100, i % 100)
}

/// Coordinates spread over the whole sheet, the same on every run.
fn scattered(n: u32) -> Vec<CellCoord> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            CellCoord::new((x % 1_048_576) as u32, ((x >> 20) % 16_384) as u32)
        })
        .collect()
}

fn sparse_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse_grid");
    group.throughput(Throughput::Elements(u64::from(GRID_CELLS)));

    group.bench_function("set_block_100k", |b| {
        b.iter(|| {
            let mut grid = SparseGrid::new();
            for i in 0..GRID_CELLS {
                grid.set(block(i), CellValue::Number(f64::from(i)));
            }
            grid
        })
    });

    let spread = scattered(GRID_CELLS);
    group.bench_function("set_scattered_100k", |b| {
        b.iter(|| {
            let mut grid = SparseGrid::new();
            for (i, &coord) in spread.iter().enumerate() {
                grid.set(coord, CellValue::Number(i as f64));
            }
            grid
        })
    });

    let mut grid = SparseGrid::new();
    for i in 0..GRID_CELLS {
        grid.set(block(i), CellValue::Number(f64::from(i)));
    }
    group.bench_function("get_block_100k", |b| {
        b.iter(|| {
            let mut sum = 0.0;
            for i in 0..GRID_CELLS {
                if let Some(CellValue::Number(n)) = grid.get(black_box(block(i))) {
                    sum += n;
                }
            }
            sum
        })
    });
    group.bench_function("get_missing_100k", |b| {
        b.iter(|| {
            spread
                .iter()
                .filter(|&&coord| grid.get(black_box(coord)).is_some())
                .count()
        })
    });
    group.finish();
}

/// Formulas of the kinds a working sheet holds.
const FORMULAS: &[&str] = &[
    "=A1+B1",
    "=SUM(A1:A100)",
    "=SUM($B$2:$B$1000)/COUNT(B2:B1000)",
    "=IF(A2>100,\"High\",IF(A2>50,\"Medium\",\"Low\"))",
    "=VLOOKUP(D2,Prices!$A$2:$C$500,3,FALSE)",
    "=SUMIFS(Sales!C:C,Sales!A:A,\"East\",Sales!B:B,\">=\"&DATE(2024,1,1))",
    "=ROUND(AVERAGE(C2:C31)*(1+$F$1),2)",
    "=IFERROR(INDEX(B:B,MATCH(E2,A:A,0)),\"\")",
    "=-A1^2+3*B1%-C1/4",
    "='Q1 Report'!D7&\" - \"&TEXT(NOW(),\"yyyy-mm-dd\")",
    "=AND(A1<>\"\",OR(B1>=0.5,C1<=.25))",
    "=MAX(A1:A10)-MIN(A1:A10)",
];

fn formula_parse(c: &mut Criterion) {
    let parser = FormulaParser::new();
    let mut group = c.benchmark_group("formula_parse");
    group.throughput(Throughput::Elements(FORMULAS.len() as u64));
    group.bench_function("mix", |b| {
        b.iter(|| {
            for f in FORMULAS {
                black_box(parser.parse(black_box(f)).unwrap());
            }
        })
    });
    group.finish();
}

fn at(row: u32, col: u32) -> CellCoord {
    CellCoord::new(row, col)
}

/// Change `input` and read `output`, which depends on it: the cost of one
/// edit and its recalculation. The value changes every time, so nothing is
/// served from a cache.
fn bench_edit(
    c: &mut Criterion,
    name: &str,
    mut engine: CalcEngine,
    input: CellCoord,
    output: CellCoord,
) {
    let mut n = 0.0;
    c.bench_function(name, |b| {
        b.iter(|| {
            n += 1.0;
            engine.set_value(0, input, CellValueInput::Number(n));
            let result = engine.get_value(0, output);
            debug_assert!(matches!(result, CellResult::Value(_)));
            result
        })
    });
}

fn recalc(c: &mut Criterion) {
    // A1 = 1, A2 = A1+1, ..., A10000 = A9999+1
    const CHAIN: u32 = 10_000;
    let mut engine = CalcEngine::new();
    engine.set_value(0, at(0, 0), CellValueInput::Number(1.0));
    for row in 1..CHAIN {
        engine
            .set_formula(0, at(row, 0), &format!("=A{}+1", row))
            .unwrap();
    }
    bench_edit(c, "recalc/chain_10k", engine, at(0, 0), at(CHAIN - 1, 0));

    // Building the chain, then evaluating it from scratch.
    c.bench_function("recalc/build_and_evaluate_chain_10k", |b| {
        b.iter_batched(
            || {
                (1..CHAIN)
                    .map(|row| format!("=A{}+1", row))
                    .collect::<Vec<_>>()
            },
            |formulas| {
                let mut engine = CalcEngine::new();
                engine.set_value(0, at(0, 0), CellValueInput::Number(1.0));
                for (row, f) in (1..CHAIN).zip(&formulas) {
                    engine.set_formula(0, at(row, 0), f).unwrap();
                }
                engine.get_value(0, at(CHAIN - 1, 0))
            },
            BatchSize::LargeInput,
        )
    });
}

fn aggregates(c: &mut Criterion) {
    // SUM over a 100k-cell column.
    const ROWS: u32 = 100_000;
    let mut engine = CalcEngine::new();
    for row in 0..ROWS {
        engine.set_value(0, at(row, 0), CellValueInput::Number(f64::from(row % 97)));
    }
    engine
        .set_formula(0, at(0, 2), &format!("=SUM(A1:A{ROWS})"))
        .unwrap();
    bench_edit(c, "aggregate/sum_100k", engine, at(ROWS / 2, 0), at(0, 2));

    // SUMIFS over 10k rows: a region in A, an amount in B.
    const RECORDS: u32 = 10_000;
    let regions = ["North", "South", "East", "West", "Central"];
    let mut engine = CalcEngine::new();
    for row in 0..RECORDS {
        let region = regions[(row % 5) as usize].to_string();
        engine.set_value(0, at(row, 0), CellValueInput::Text(region));
        engine.set_value(0, at(row, 1), CellValueInput::Number(f64::from(row % 250)));
    }
    engine
        .set_formula(
            0,
            at(0, 3),
            &format!("=SUMIFS(B1:B{RECORDS},A1:A{RECORDS},\"East\",B1:B{RECORDS},\">100\")"),
        )
        .unwrap();
    bench_edit(
        c,
        "aggregate/sumifs_10k",
        engine,
        at(RECORDS / 2, 1),
        at(0, 3),
    );
}

criterion_group!(benches, sparse_grid, formula_parse, recalc, aggregates);
criterion_main!(benches);
