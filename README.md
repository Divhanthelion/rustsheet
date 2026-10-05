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

- **220+ Excel functions** across math, statistics, finance, text, logic, lookup and reference, dates and times, and information: `SUM`, `IF`, `VLOOKUP`, `INDEX`/`MATCH`, `SUMIF`/`COUNTIF` with wildcards, `MAXIFS`, `PMT`, `NPV`/`IRR`, `XIRR`, `NETWORKDAYS`, `DATEDIF`, `TEXTJOIN`, `TEXTBEFORE`, `INDIRECT`, `OFFSET`, `PERCENTILE`, `FORECAST`, and more. Press **F1** for the full list with examples.
- **Live recalculation** with dependency tracking and cycle detection (`#CIRC!`).
- **Multiple sheets** with cross-sheet references (`Sheet2!A1`, `SUM(Sheet2!A1:A10)`). Double-click a tab to rename it; the formulas that use it follow.
- **Formatting**: bold, italic, underline, strikethrough, font size and color, fills, borders, alignment, and Excel number formats (currency, percent, dates, times, fractions, custom codes like `#,##0.00_);[Red](#,##0.00)`). Resize columns and rows by dragging, or double-click a column border to fit it.
- **Smart entry**: typing `12%`, `$1,234.50`, `2026-10-03` or `2:30 PM` stores a number and picks the matching format.
- **Charts**: line, bar, scatter, area, pie and doughnut, saved into the workbook.
- **PivotTables**: summarize a range by rows, columns and report filters with sum, count, average, min or max, subtotals and grand totals, and hide items you don't want. Refresh after the data changes (Alt+F5, or Ctrl+Alt+F5 for all). PivotTables made in Excel open ready to refresh.
- **Conditional formatting**: highlight cells by value, text, rank, average, duplicates, blanks, errors or a formula, plus data bars and 2- and 3-color scales.
- **Data validation**: whole numbers, decimals, dates, times, text length, custom formulas, or a list with an in-cell drop-down, with an input message and a stop, warning or information alert.
- **Notes** on cells (Shift+F2), shown when you point at the red corner.
- **Pictures**: insert PNG, JPEG, GIF or BMP files or paste a screenshot; move, resize, reorder and add alt text. They print and save with the workbook.
- **Fonts**: any font installed on your PC, by name.
- **Excel files**: read and write values, formulas, formatting, column widths, row heights, charts, conditional formatting, data validation, notes and pictures in `.xlsx`.
- **CSV**: import and export, formulas included.
- **Copy and paste** within RustSheet (formulas follow their new position, formats come along) and with Excel, Google Sheets or any app that copies tab-separated text. Cut and paste moves cells.
- **Full-size sheets**: Excel's 1,048,576 rows by 16,384 columns, with scrollbars and Excel-style navigation (Ctrl+Arrow, Ctrl+End, Ctrl+A).
- **Rows and columns**: insert, delete, hide, resize and freeze. Formulas, formats, merges, charts and filters follow; references to deleted cells become `#REF!`. Click or drag headers to select whole rows or columns, and format them in one go.
- **Sort and filter**: sort by one column or several, and AutoFilter with value lists and search.
- **Find and Replace** with `*` and `?` wildcards, in formulas or shown values, on one sheet or all of them.
- **Fill**: Ctrl+D and Ctrl+R, or drag the fill handle to continue a series (numbers, dates, "Item 1", months, weekdays).
- **Merged cells and wrapped text**, vertical alignment, and long text that spills into empty neighbors.
- **Print and PDF**: print through the Windows print dialog, or export a PDF, with gridlines, fit to width, or the selection only.
- **Crash safe**: unsaved work is autosaved every minute and offered back if RustSheet ever closes unexpectedly; saves replace files atomically.
- **Undo/redo**, formula autocomplete, light and dark themes (or follow Windows), recent files, and screen reader support.

### Excel compatibility notes

- Aggregates (`AVERAGE`, `COUNT`, `PRODUCT`, `MIN`, `MAX`, `SUMIF`, `COUNTIF`) skip blanks and text, as Excel does.
- `MOD`, `CEILING` and `FLOOR` follow Excel's sign rules.
- `INDIRECT` and `OFFSET` work wherever a range does (`SUM(OFFSET(A1,0,0,5))`) and recalculate after every edit, as in Excel. `INDIRECT` reads A1-style references only.
- Dynamic-array functions that spill (`FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`) are not supported.
- Array constants such as `{1,2;3,4}` work wherever a function reads a range (`SUM({1,2,3})`, `VLOOKUP(2,{1,"a";2,"b"},2)`); arithmetic on a whole array (`{1,2}*2`) is not supported.
- Arguments can be left empty, as in `PMT(5%/12,360,,100000)` or `IF(A1>0,,"none")`.
- Dates follow Excel's 1900 date system, including its February 29th, 1900 (serial 60).
- Numbers display in Excel's General format: as many decimals as fit the column, then scientific notation.
- `TEXT` uses the same formatter as cells, so it accepts the same format codes.

## File formats

| Format | Open | Save |
|--------|------|------|
| `.xlsx` | Workbook, formulas, formatting, charts, pictures, PivotTables | Workbook, formulas, formatting, charts, pictures, PivotTables (see below) |
| `.csv` | One sheet | Current sheet only |

`.xls` and `.ods` are not supported. From `.xlsx`, RustSheet keeps fonts (name, bold, italic, underline, strikethrough, size, color), solid fills, borders (drawn as thin lines), alignment, wrapped text, number formats, row and column formats, column widths, row heights, hidden rows and columns, merged cells, frozen panes, AutoFilters, conditional formatting, data validation, notes and pictures. It skips icon sets, sparklines, shapes and text boxes.

PivotTables are saved as their values and formats, plus a definition RustSheet uses to refresh them. Excel shows them as ordinary cells (rust_xlsxwriter, which writes the file, can't write Excel's PivotTable parts). PivotTables made in Excel open in RustSheet ready to refresh, with their rows, columns, values, filters and hidden items.

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
.\packaging\windows\build-msix.ps1                  # x64
.\packaging\windows\build-msix.ps1 -Arch x64,arm64  # both, plus a .msixbundle
```

## Keyboard shortcuts

| Keys | Action |
|------|--------|
| Ctrl+N / Ctrl+O / Ctrl+S / Ctrl+P | New / Open / Save / Print |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / Cut / Paste |
| Ctrl+F / Ctrl+H | Find / Replace |
| Ctrl+D / Ctrl+R | Fill down / right |
| Delete | Clear the selected cells, or delete the selected picture |
| Shift+F2 | Add or edit a note |
| Alt+Down | Open a cell's drop-down list |
| Alt+F5 / Ctrl+Alt+F5 | Refresh a PivotTable / all PivotTables |
| Ctrl+B / Ctrl+I / Ctrl+U | Bold / Italic / Underline |
| F2 or type | Edit the active cell |
| Enter / Tab | Move down / right (Shift goes back) |
| Ctrl+Arrow, Ctrl+End | Jump to the edge of the data, the last used cell |
| Shift+Arrow | Extend the selection |
| Ctrl+A, Ctrl+Space, Shift+Space | Select the data, a column, a row |
| Ctrl+Shift+= / Ctrl+- | Insert / delete rows or columns |
| Ctrl+Shift+L | Filter on or off |
| F4 | Toggle absolute/relative reference |
| F1 | Help and function reference |

## Architecture

| Module | Role |
|--------|------|
| `cell/` | Coordinates, values, string interning |
| `grid/` | Sparse sheet storage |
| `format/` | Cell formats, number format codes, typed-input parsing, conditional formats, validation, pictures |
| `pivot` | PivotTable definitions and layout |
| `formula/` | pest grammar and Pratt parser |
| `calc/` | `CalcEngine`, function library, dependency graph |
| `chart/` | Chart definitions, rendering, LTTB downsampling |
| `xlsx/`, `csv_io/` | File I/O |
| `gui/` | egui application |

## Privacy

RustSheet collects no data and makes no network connections. See [PRIVACY.md](PRIVACY.md).

## License

[MIT](LICENSE)
