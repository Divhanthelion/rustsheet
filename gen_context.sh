#!/bin/bash
# Generate contextprompt.md - concatenates all Rust source files with project context

OUTPUT="contextprompt.md"

cat > "$OUTPUT" << 'HEADER'
# RustSheet - Complete Project Context

## Overview
RustSheet is a high-performance, Excel-compatible spreadsheet engine written in Rust with a native GUI via egui.

**Key Features:**
- Formula parsing with 70+ Excel functions (SUM, VLOOKUP, IF, etc.)
- Dependency tracking and cycle detection
- Excel .xlsx file I/O
- Multi-sheet support with tabs
- Undo/redo system
- Native GUI with cell grid, selection, formula bar with autocomplete

**Architecture:**
- `cell/` - Core types (CellValue, CellCoord, StringPool)
- `grid/` - Sparse storage (HashMap + RoaringBitmap)
- `formula/` - pest grammar + Pratt parser for formulas
- `calc/` - CalcEngine with RefCell for recursive evaluation
- `xlsx/` - calamine reader, rust_xlsxwriter writer
- `gui/` - egui-based spreadsheet UI

**Current Issue:** Cell editing not smooth - focus management prevents multiple keystrokes

---

## File Tree
```
HEADER

# Add file tree
find . -type f \( -name "*.rs" -o -name "*.toml" -o -name "*.pest" \) \
    ! -path "./target/*" | sort | sed 's|^./||' >> "$OUTPUT"

cat >> "$OUTPUT" << 'MIDDLE'
```

---

## Cargo.toml
```toml
MIDDLE

cat Cargo.toml >> "$OUTPUT"

cat >> "$OUTPUT" << 'DIVIDER'
```

---

## Source Files

DIVIDER

# Concatenate all Rust files
find . -type f -name "*.rs" ! -path "./target/*" | sort | while read -r file; do
    relpath="${file#./}"
    echo "### $relpath" >> "$OUTPUT"
    echo '```rust' >> "$OUTPUT"
    cat "$file" >> "$OUTPUT"
    echo '```' >> "$OUTPUT"
    echo "" >> "$OUTPUT"
done

# Add pest grammar
if [ -f "src/formula/formula.pest" ]; then
    echo "### src/formula/formula.pest" >> "$OUTPUT"
    echo '```pest' >> "$OUTPUT"
    cat "src/formula/formula.pest" >> "$OUTPUT"
    echo '```' >> "$OUTPUT"
fi

echo "Generated $OUTPUT ($(wc -l < "$OUTPUT") lines)"
