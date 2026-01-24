use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rustsheet::prelude::*;

fn bench_sparse_grid_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse_grid_insert");

    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let mut grid = SparseGrid::new();
                for i in 0..size {
                    grid.set(
                        CellCoord::new(i, i % 100),
                        CellValue::Number(i as f64),
                    );
                }
                black_box(grid)
            });
        });
    }

    group.finish();
}

fn bench_sparse_grid_lookup(c: &mut Criterion) {
    // Pre-populate grid
    let mut grid = SparseGrid::new();
    for i in 0..10000 {
        grid.set(
            CellCoord::new(i, i % 100),
            CellValue::Number(i as f64),
        );
    }

    c.bench_function("sparse_grid_lookup", |b| {
        b.iter(|| {
            let mut sum = 0.0;
            for i in 0..1000 {
                if let Some(CellValue::Number(n)) = grid.get(CellCoord::new(i, i % 100)) {
                    sum += n;
                }
            }
            black_box(sum)
        });
    });
}

fn bench_formula_parsing(c: &mut Criterion) {
    let parser = FormulaParser::new();

    let formulas = [
        "=1+2",
        "=A1+B2*C3",
        "=SUM(A1:Z100)",
        "=IF(A1>10,\"big\",\"small\")",
        "=VLOOKUP(A1,B1:D100,2,FALSE)",
    ];

    let mut group = c.benchmark_group("formula_parsing");

    for formula in formulas.iter() {
        group.bench_with_input(BenchmarkId::from_parameter(formula), formula, |b, formula| {
            b.iter(|| {
                black_box(parser.parse(formula).unwrap())
            });
        });
    }

    group.finish();
}

fn bench_calculation_engine(c: &mut Criterion) {
    c.bench_function("calc_engine_chain", |b| {
        b.iter(|| {
            let mut engine = CalcEngine::new();

            // Set up a chain of dependent cells
            engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));

            for i in 1..100 {
                let prev = CellCoord::new(i - 1, 0);
                let curr = CellCoord::new(i, 0);
                let formula = format!("={}+1", prev.to_a1());
                engine.set_formula(0, curr, &formula).unwrap();
            }

            // Force evaluation of last cell
            black_box(engine.get_value(0, CellCoord::new(99, 0)))
        });
    });
}

fn bench_sum_large_range(c: &mut Criterion) {
    let mut engine = CalcEngine::new();

    // Set up 10000 cells
    for i in 0..10000 {
        engine.set_value(0, CellCoord::new(i, 0), CellValueInput::Number(1.0));
    }

    engine.set_formula(0, CellCoord::new(0, 1), "=SUM(A1:A10000)").unwrap();

    c.bench_function("sum_10000_cells", |b| {
        b.iter(|| {
            // Invalidate and recalculate
            engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(2.0));
            black_box(engine.get_value(0, CellCoord::new(0, 1)))
        });
    });
}

criterion_group!(
    benches,
    bench_sparse_grid_insert,
    bench_sparse_grid_lookup,
    bench_formula_parsing,
    bench_calculation_engine,
    bench_sum_large_range,
);

criterion_main!(benches);
