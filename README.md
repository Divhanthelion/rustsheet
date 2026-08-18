# RustSheet

A native spreadsheet with an Excel-compatible formula engine.

## Features

- **Formulas**: 100+ Excel functions, dependency tracking, and cycle detection
- **Cross-sheet**: `Sheet2!A1` and `SUM(Sheet2!A1:A2)`; rename rewrites qualifiers; delete remaps sheet keys
- **xlsx**: Read and write values, formulas, used cells, and charts
- **CSV**: Read and write the current sheet, including formulas
- **Charts**: Line, bar, scatter, area, pie, and doughnut
- **GUI**: egui grid, formula bar with autocomplete, undo/redo

Aggregates (`AVERAGE`, `COUNT`, `PRODUCT`, `MIN`, `MAX`, `SUMIF`/`COUNTIF`) skip blanks. `TEXT` supports number, percent, scientific, and date formats. `MOD`, `CEILING`, and `FLOOR` follow Excel sign rules.

## Formats

| Format | Open | Save |
|--------|------|------|
| `.xlsx` | Workbook, formulas, charts | Workbook, formulas, charts |
| `.csv` | One sheet | Current sheet only |

`.xls` / `.ods` are not supported.

## Build

Default features are `gui`, `xlsx`, and `csv`.

```bash
cargo run          # native GUI
cargo test         # library tests
```

## Architecture

- `cell/` — coordinates, values, string pool
- `grid/` — sparse sheet storage
- `formula/` — pest grammar + Pratt parser
- `calc/` — CalcEngine, functions, dependency graph
- `chart/` — definitions, rendering, LTTB downsampling
- `xlsx/` — Excel read/write
- `csv_io/` — CSV read/write
- `gui/` — egui UI

## License

MIT
