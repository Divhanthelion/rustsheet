# RustSheet

A high-performance, Excel-compatible spreadsheet engine written in Rust with a native GUI.

## Features

- **Formula Engine**: 70+ Excel functions (SUM, VLOOKUP, IF, AVERAGE, etc.)
- **Dependency Tracking**: Automatic recalculation with cycle detection
- **Excel I/O**: Read and write .xlsx files via calamine/rust_xlsxwriter
- **Multi-Sheet Support**: Workbook with multiple sheets and tab navigation
- **Native GUI**: egui-based interface with cell grid, formula bar with autocomplete
- **Charting**: Line, Bar, Scatter, Area, Pie, and Doughnut charts with drag-to-select
- **Undo/Redo**: Full edit history support

## Build

```bash
cargo build --features gui      # Build with GUI
cargo run --features gui        # Run the application
cargo test                      # Run tests
```

## Architecture

- `cell/` - Core types (CellValue, CellCoord, StringPool)
- `grid/` - Sparse storage using HashMap + RoaringBitmap
- `formula/` - pest grammar + Pratt parser for Excel-compatible formulas
- `calc/` - CalcEngine with dependency tracking and caching
- `chart/` - Chart definitions, rendering (egui_plot + custom mesh), and LTTB downsampling
- `xlsx/` - Excel file I/O
- `gui/` - egui-based spreadsheet UI

## License

MIT
