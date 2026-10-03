<p align="center">
  <img src="assets/icon-256.png" width="96" alt="RustSheet icon">
</p>

<h1 align="center">RustSheet</h1>

<p align="center">
  A fast, native spreadsheet with an Excel-compatible formula engine.<br>
  Opens and saves <code>.xlsx</code> and <code>.csv</code>. No account, no cloud, no telemetry.
</p>

<p align="center">
  <a href="https://github.com/Divhanthelion/rustsheet/actions/workflows/ci.yml"><img src="https://github.com/Divhanthelion/rustsheet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

![RustSheet with a budget workbook and a bar chart](assets/screenshot.png)

## Features

- **100+ Excel functions** across math, statistics, text, logic, lookup and dates: `SUM`, `AVERAGE`, `IF`, `VLOOKUP`, `INDEX`/`MATCH`, `SUMIF`/`COUNTIF` with wildcards, `TEXT`, `ROUND`, and more. Press **F1** for the full list with examples.
- **Live recalculation** with dependency tracking and cycle detection (`#CIRC!`).
- **Multiple sheets** with cross-sheet references (`Sheet2!A1`, `SUM(Sheet2!A1:A10)`). Renaming a sheet rewrites the formulas that use it.
- **Formatting**: bold, italic, underline, strikethrough, font size and color, fills, borders, alignment, and Excel number formats (currency, percent, dates, times, fractions, custom codes like `#,##0.00_);[Red](#,##0.00)`). Resize columns and rows by dragging, or double-click a column border to fit it.
- **Smart entry**: typing `12%`, `$1,234.50`, `2026-10-03` or `2:30 PM` stores a number and picks the matching format.
- **Charts**: line, bar, scatter, area, pie and doughnut, saved into the workbook.
- **Excel files**: read and write values, formulas, formatting, column widths, row heights and charts in `.xlsx`.
- **CSV**: import and export, formulas included.
- **Undo/redo**, formula autocomplete, light and dark themes, keyboard navigation that works like Excel's.

### Excel compatibility notes

- Aggregates (`AVERAGE`, `COUNT`, `PRODUCT`, `MIN`, `MAX`, `SUMIF`, `COUNTIF`) skip blanks and text, as Excel does.
- `MOD`, `CEILING` and `FLOOR` follow Excel's sign rules.
- Numbers display in Excel's General format: as many decimals as fit the column, then scientific notation.
- `TEXT` uses the same formatter as cells, so it accepts the same format codes.

## File formats

| Format | Open | Save |
|--------|------|------|
| `.xlsx` | Workbook, formulas, formatting, charts | Workbook, formulas, formatting, charts |
| `.csv` | One sheet | Current sheet only |

`.xls` and `.ods` are not supported. From `.xlsx` formatting, RustSheet keeps fonts (bold, italic, underline, strikethrough, size, color), solid fills, borders (drawn as thin lines), horizontal alignment, number formats, column widths and row heights. It does not keep font names, merged cells, wrapped text, vertical alignment, conditional formatting or column-wide styles.

## Install

**Windows**: the Microsoft Store listing is coming soon. Until then, build from source.

## Build from source

Requires Rust 1.87 or newer.

```bash
cargo run --release          # the app
cargo test                   # engine, file I/O and GUI unit tests
cargo run --release -- file.xlsx   # open a file at startup
```

Default features are `gui`, `xlsx` and `csv`. For the engine alone (no GUI):

```bash
cargo test --no-default-features --features xlsx,csv
```

### Windows package (MSIX)

Needs the Windows 10/11 SDK. See [packaging/windows/STORE.md](packaging/windows/STORE.md) for the Store submission steps.

```powershell
.\packaging\windows\build-msix.ps1
```

## Keyboard shortcuts

| Keys | Action |
|------|--------|
| Ctrl+N / Ctrl+O / Ctrl+S | New / Open / Save |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+B / Ctrl+I / Ctrl+U | Bold / Italic / Underline |
| F2 or type | Edit the active cell |
| Enter / Tab | Confirm and move down / right |
| Ctrl+Arrow | Jump to the edge of the data |
| Shift+Arrow | Extend the selection |
| F4 | Toggle absolute/relative reference |
| F1 | Help and function reference |

## Architecture

| Module | Role |
|--------|------|
| `cell/` | Coordinates, values, string interning |
| `grid/` | Sparse sheet storage |
| `format/` | Cell formats, number format codes, typed-input parsing |
| `formula/` | pest grammar and Pratt parser |
| `calc/` | `CalcEngine`, function library, dependency graph |
| `chart/` | Chart definitions, rendering, LTTB downsampling |
| `xlsx/`, `csv_io/` | File I/O |
| `gui/` | egui application |

## Privacy

RustSheet collects no data and makes no network connections. See [PRIVACY.md](PRIVACY.md).

## License

[MIT](LICENSE)
