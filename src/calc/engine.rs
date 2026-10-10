use crate::calc::array::{Array, MAX_ARRAY_ITEMS, Operand};
use crate::calc::functions::BuiltinFunctions;
use crate::cell::{CellCoord, CellError, CellRange, MAX_COL, MAX_ROW};
use crate::format::{CellFormat, SheetFormatting};
use crate::formula::{BinaryOp, Expr, FormulaParser, RangeKind, UnaryOp};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// Input for a cell (either a value or formula string)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellInput {
    Empty,
    Value(CellValueInput),
    Formula(String),
}

/// A cell's stored constant.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValueInput {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
}

impl Eq for CellValueInput {}

impl std::hash::Hash for CellValueInput {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            CellValueInput::Number(n) => {
                0u8.hash(state);
                n.to_bits().hash(state);
            }
            CellValueInput::Text(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            CellValueInput::Bool(b) => {
                2u8.hash(state);
                b.hash(state);
            }
            CellValueInput::Error(e) => {
                3u8.hash(state);
                e.hash(state);
            }
        }
    }
}

/// Result of cell computation
#[derive(Debug, Clone, PartialEq)]
pub enum CellResult {
    Value(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
    Empty,
}

impl CellResult {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CellResult::Value(n) => Some(*n),
            CellResult::Bool(true) => Some(1.0),
            CellResult::Bool(false) => Some(0.0),
            CellResult::Empty => Some(0.0),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            CellResult::Bool(b) => Some(*b),
            CellResult::Value(n) => Some(*n != 0.0),
            CellResult::Empty => Some(false),
            _ => None,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, CellResult::Error(_))
    }

    /// The value as `&` and the text functions see it: numbers as an
    /// unformatted cell shows them (General). `None` for errors.
    pub fn to_text(&self) -> Option<String> {
        match self {
            CellResult::Error(_) => None,
            other => Some(crate::format::display_text(other, None)),
        }
    }
}

/// A cell on a sheet.
type Key = (u32, CellCoord);

/// Functions whose result can change while nothing they read does; cells
/// calling them recalculate after every edit. INDIRECT and OFFSET are here
/// because what they read is only known when they run, so no dependency
/// records it.
const VOLATILE_FUNCTIONS: &[&str] = &["RAND", "RANDBETWEEN", "NOW", "TODAY", "INDIRECT", "OFFSET"];

/// Deepest nesting of cell evaluations reached through references the
/// dependency graph can't see (INDIRECT, OFFSET): static chains evaluate
/// iteratively and stay shallow, and 250 native frames fit the GUI
/// thread's 1 MiB stack even in debug builds.
const MAX_EVAL_DEPTH: u32 = 250;

/// The calculation engine - manages formula evaluation with incremental computation
///
/// Uses interior mutability (RefCell) for cache and cycle detection state to allow
/// recursive evaluation through function calls without violating borrow rules.
///
/// Every cached value was computed from cached values only, so a cell that
/// is not cached has nothing cached downstream of it.
pub struct CalcEngine {
    /// Formula parser
    parser: FormulaParser,
    /// Parsed formula ASTs (indexed by formula string hash)
    formulas: HashMap<String, Arc<Expr>>,
    /// Cell inputs per sheet
    inputs: HashMap<Key, CellInput>,
    /// Computed values cache - uses RefCell for interior mutability during evaluation
    cache: RefCell<HashMap<Key, CellResult>>,
    /// Dependency graph: cell -> cells that depend on it
    dependents: HashMap<Key, HashSet<Key>>,
    /// Reverse dependency: cell -> cells it depends on
    dependencies: HashMap<Key, HashSet<Key>>,
    /// Ranges formulas read, unexpanded: the range's sheet -> reader ->
    /// ranges. A change invalidates every reader with a range around it.
    range_dependents: HashMap<u32, HashMap<Key, Vec<CellRange>>>,
    /// Formula cells by sheet as (column, row), to find the formulas inside
    /// a range without visiting its other cells
    formula_cells: HashMap<u32, BTreeSet<(u32, u32)>>,
    /// Formula cells that call a volatile function
    volatile: HashSet<Key>,
    /// Greatest row and column with input, per sheet; rebuilt when `None`
    extents: RefCell<Option<HashMap<u32, CellCoord>>>,
    /// Cells currently being evaluated (for cycle detection) - uses RefCell for interior mutability
    evaluating: RefCell<HashSet<Key>>,
    /// The cell whose formula is being evaluated, for ROW() and COLUMN()
    current: Cell<Option<Key>>,
    /// Nesting of `get_value` computations, capped at [`MAX_EVAL_DEPTH`]
    eval_depth: Cell<u32>,
    /// A depth overflow happened below: results are provisional, not cached
    eval_tainted: Cell<bool>,
    /// Built-in functions
    functions: BuiltinFunctions,
    /// Tab names, index-aligned with sheet keys
    sheet_names: Vec<String>,
    /// Cell formats and row/column sizes, by sheet
    formatting: HashMap<u32, SheetFormatting>,
    /// Bumped on every change to inputs or formatting
    revision: Cell<u64>,
    /// Conditional formatting statistics by (sheet, rule index)
    cf_cache: super::conditional::StatsCache,
    /// Minutes NOW() and TODAY() add to UTC
    clock_offset: i32,
    /// Seconds since 1970-01-01 UTC; tests pin it
    clock: fn() -> f64,
}

fn system_clock() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

impl CalcEngine {
    pub fn new() -> Self {
        Self {
            parser: FormulaParser::new(),
            formulas: HashMap::new(),
            inputs: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
            dependents: HashMap::new(),
            dependencies: HashMap::new(),
            range_dependents: HashMap::new(),
            formula_cells: HashMap::new(),
            volatile: HashSet::new(),
            extents: RefCell::new(None),
            evaluating: RefCell::new(HashSet::new()),
            current: Cell::new(None),
            eval_depth: Cell::new(0),
            eval_tainted: Cell::new(false),
            functions: BuiltinFunctions::new(),
            sheet_names: vec!["Sheet1".to_string()],
            formatting: HashMap::new(),
            revision: Cell::new(0),
            cf_cache: Default::default(),
            clock_offset: 0,
            clock: system_clock,
        }
    }

    /// Changes whenever any cell or format changes, for caches of derived
    /// state.
    pub fn revision(&self) -> u64 {
        self.revision.get()
    }

    fn touch(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
    }

    pub(super) fn cf_cache(&self) -> &super::conditional::StatsCache {
        &self.cf_cache
    }

    /// Minutes NOW() and TODAY() add to UTC; 0 unless set.
    pub fn clock_offset(&self) -> i32 {
        self.clock_offset
    }

    /// Set the minutes NOW() and TODAY() add to UTC. A change recalculates
    /// the volatile cells, as an edit does; setting the same offset again
    /// costs nothing.
    pub fn set_clock_offset(&mut self, minutes: i32) {
        if minutes != self.clock_offset {
            self.clock_offset = minutes;
            self.touch();
            self.drop_cached(None);
        }
    }

    /// Seconds since 1970-01-01 UTC, now.
    pub(super) fn utc_seconds(&self) -> f64 {
        (self.clock)()
    }

    #[cfg(test)]
    pub(crate) fn set_clock(&mut self, clock: fn() -> f64) {
        self.clock = clock;
        self.touch();
        self.drop_cached(None);
    }

    /// Set a cell's value (not a formula)
    pub fn set_value(&mut self, sheet: u32, coord: CellCoord, value: CellValueInput) {
        self.clear_cell_deps(sheet, coord);
        self.store_input((sheet, coord), CellInput::Value(value));
        self.invalidate(sheet, coord);
    }

    /// Set a cell's formula
    pub fn set_formula(
        &mut self,
        sheet: u32,
        coord: CellCoord,
        formula: &str,
    ) -> Result<(), String> {
        // Parse and validate formula
        let formula = crate::formula::normalize_formula(formula);
        let expr = self.parser.parse(&formula).map_err(|e| e.to_string())?;
        let key = (sheet, coord);

        // Clear old dependencies
        self.clear_cell_deps(sheet, coord);

        // Build new dependencies. Ranges stay whole: A:A is one entry.
        let mut cells = Vec::new();
        let mut ranges = Vec::new();
        expr.collect_references(&mut cells, &mut ranges);
        for dep in cells {
            let Ok(dep_sheet) = self.resolve_sheet(dep.sheet.as_deref(), sheet) else {
                continue;
            };
            let dep_key = (dep_sheet, dep.coord);
            self.dependencies.entry(key).or_default().insert(dep_key);
            self.dependents.entry(dep_key).or_default().insert(key);
        }
        for dep in ranges {
            let Ok(dep_sheet) = self.resolve_sheet(dep.sheet.as_deref(), sheet) else {
                continue;
            };
            self.range_dependents
                .entry(dep_sheet)
                .or_default()
                .entry(key)
                .or_default()
                .push(dep.range);
        }
        if expr.calls_any(VOLATILE_FUNCTIONS) {
            self.volatile.insert(key);
        }

        // Store formula
        self.formulas.insert(formula.clone(), Arc::new(expr));
        self.store_input(key, CellInput::Formula(formula));

        self.invalidate(sheet, coord);
        Ok(())
    }

    /// Clear a cell
    pub fn clear(&mut self, sheet: u32, coord: CellCoord) {
        self.clear_cell_deps(sheet, coord);
        self.remove_input((sheet, coord));
        self.invalidate(sheet, coord);
    }

    fn store_input(&mut self, key: Key, input: CellInput) {
        let (sheet, coord) = key;
        if let Some(extents) = self.extents.get_mut() {
            let max = extents.entry(sheet).or_insert(coord);
            *max = CellCoord::new(max.row.max(coord.row), max.col.max(coord.col));
        }
        let formulas = self.formula_cells.entry(sheet).or_default();
        if matches!(input, CellInput::Formula(_)) {
            formulas.insert((coord.col, coord.row));
        } else {
            formulas.remove(&(coord.col, coord.row));
        }
        self.inputs.insert(key, input);
    }

    fn remove_input(&mut self, key: Key) {
        let (sheet, coord) = key;
        if self.inputs.remove(&key).is_none() {
            return;
        }
        if let Some(formulas) = self.formula_cells.get_mut(&sheet) {
            formulas.remove(&(coord.col, coord.row));
        }
        // Only a cell on the far edge can shrink the extent.
        let extents = self.extents.get_mut();
        let on_edge = extents
            .as_ref()
            .and_then(|e| e.get(&sheet))
            .is_some_and(|max| max.row == coord.row || max.col == coord.col);
        if on_edge {
            *extents = None;
        }
    }

    /// Get computed value for a cell
    ///
    /// Uses interior mutability to allow recursive calls during function evaluation
    /// without requiring &mut self, which would conflict with the borrow of self.functions.
    pub fn get_value(&self, sheet: u32, coord: CellCoord) -> CellResult {
        let key = (sheet, coord);
        // Check cache first
        if let Some(cached) = self.cache.borrow().get(&key) {
            return cached.clone();
        }

        if self.is_formula(key) && !self.evaluating.borrow().contains(&key) {
            self.evaluate_precedents(key);
        }

        // Compute value. References the dependency graph can't see
        // (INDIRECT, OFFSET) recurse natively, so cap that depth at
        // #CALC! rather than a blown stack.
        let depth = self.eval_depth.get();
        if depth >= MAX_EVAL_DEPTH {
            self.eval_tainted.set(true);
            return CellResult::Error(CellError::Calc);
        }
        self.eval_depth.set(depth + 1);
        let result = self.compute(sheet, coord);
        self.eval_depth.set(depth);
        self.store_result(key, result)
    }

    /// Cache `result` for `key`, unless a depth overflow below made it
    /// provisional; the overflow is forgotten once evaluation unwinds, so
    /// the same cells compute for real when read less deeply.
    fn store_result(&self, key: Key, result: CellResult) -> CellResult {
        if self.eval_tainted.get() {
            if self.eval_depth.get() == 0 {
                self.eval_tainted.set(false);
            }
        } else {
            self.cache.borrow_mut().insert(key, result.clone());
        }
        result
    }

    fn is_formula(&self, key: Key) -> bool {
        matches!(self.inputs.get(&key), Some(CellInput::Formula(_)))
    }

    /// Evaluate the uncached formulas `root` reads, directly or through
    /// others, precedents first. Evaluating `root` afterwards only recurses
    /// into cached cells, so the stack grows with formula size rather than
    /// with the length of a dependency chain. Cycles are left for
    /// `evaluating` to report.
    fn evaluate_precedents(&self, root: Key) {
        // A cell is expanded once; while expanded and not yet done it is on
        // the current path, so meeting it again means a cycle.
        let mut expanded = HashSet::from([root]);
        let mut stack: Vec<(Key, bool)> = Vec::new();
        self.push_stale_precedents(root, &expanded, &mut stack);
        while let Some((key, done)) = stack.pop() {
            if self.cache.borrow().contains_key(&key) {
                continue;
            }
            if done {
                let result = self.compute(key.0, key.1);
                self.store_result(key, result);
            } else if expanded.insert(key) {
                stack.push((key, true));
                self.push_stale_precedents(key, &expanded, &mut stack);
            }
        }
    }

    /// Push the uncached formula cells that `key`'s formula refers to.
    fn push_stale_precedents(
        &self,
        key: Key,
        expanded: &HashSet<Key>,
        stack: &mut Vec<(Key, bool)>,
    ) {
        let cache = self.cache.borrow();
        let evaluating = self.evaluating.borrow();
        let mut push = |dep: Key| {
            if !expanded.contains(&dep)
                && !cache.contains_key(&dep)
                && !evaluating.contains(&dep)
                && self.is_formula(dep)
            {
                stack.push((dep, false));
            }
        };
        if let Some(deps) = self.dependencies.get(&key) {
            deps.iter().for_each(|&dep| push(dep));
        }
        for (&sheet, readers) in &self.range_dependents {
            for range in readers.get(&key).into_iter().flatten() {
                self.for_each_formula_in(sheet, range, |coord| push((sheet, coord)));
            }
        }
    }

    /// Call `f` with each formula cell inside `range` on `sheet`.
    fn for_each_formula_in(&self, sheet: u32, range: &CellRange, mut f: impl FnMut(CellCoord)) {
        let Some(formulas) = self.formula_cells.get(&sheet) else {
            return;
        };
        // Seek column by column, skipping columns without formulas.
        let mut col = range.start.col;
        while col <= range.end.col {
            let Some(&(found, _)) = formulas.range((col, range.start.row)..).next() else {
                break;
            };
            if found > range.end.col {
                break;
            }
            for &(_, row) in formulas.range((found, range.start.row)..=(found, range.end.row)) {
                f(CellCoord::new(row, found));
            }
            let Some(next) = found.checked_add(1) else {
                break;
            };
            col = next;
        }
    }

    /// Get the formula string for a cell, if it has one
    pub fn get_formula(&self, sheet: u32, coord: CellCoord) -> Option<String> {
        match self.inputs.get(&(sheet, coord)) {
            Some(CellInput::Formula(f)) => Some(f.clone()),
            _ => None,
        }
    }

    /// Iterate stored inputs for one sheet.
    pub fn iter_sheet_inputs(
        &self,
        sheet: u32,
    ) -> impl Iterator<Item = (CellCoord, &CellInput)> + '_ {
        self.inputs
            .iter()
            .filter_map(move |(&(s, coord), input)| (s == sheet).then_some((coord, input)))
    }

    /// Greatest row and column that have input on this sheet.
    pub fn sheet_max_coord(&self, sheet: u32) -> Option<CellCoord> {
        let mut extents = self.extents.borrow_mut();
        let extents = extents.get_or_insert_with(|| {
            let mut extents: HashMap<u32, CellCoord> = HashMap::new();
            for &(s, coord) in self.inputs.keys() {
                let max = extents.entry(s).or_insert(coord);
                *max = CellCoord::new(max.row.max(coord.row), max.col.max(coord.col));
            }
            extents
        });
        extents.get(&sheet).copied()
    }

    /// How many rows and columns, counted from each range's top-left
    /// corner, can hold input in any of `ranges`: past them every range is
    /// empty. `None` when all of them are.
    pub(crate) fn used_window(&self, ranges: &[(u32, CellRange)]) -> Option<(u32, u32)> {
        let mut window: Option<(u32, u32)> = None;
        for &(sheet, range) in ranges {
            let Some(max) = self.sheet_max_coord(sheet) else {
                continue;
            };
            let rows = (max.row.saturating_add(1))
                .saturating_sub(range.start.row)
                .min(range.height());
            let cols = (max.col.saturating_add(1))
                .saturating_sub(range.start.col)
                .min(range.width());
            if rows > 0 && cols > 0 {
                let (r, c) = window.unwrap_or((0, 0));
                window = Some((r.max(rows), c.max(cols)));
            }
        }
        window
    }

    /// The part of `range` on `sheet` that can hold input; the rest is
    /// empty. Keeps whole columns from walking a million rows.
    pub(crate) fn used_part(&self, sheet: u32, range: &CellRange) -> Option<CellRange> {
        self.used_window(&[(sheet, *range)])
            .map(|window| leading(range, window))
    }

    /// Compute a cell's value
    fn compute(&self, sheet: u32, coord: CellCoord) -> CellResult {
        let key = (sheet, coord);
        // Cycle detection - check if this cell is already being evaluated
        if self.evaluating.borrow().contains(&key) {
            return CellResult::Error(CellError::Circular);
        }

        match self.inputs.get(&key) {
            None | Some(CellInput::Empty) => CellResult::Empty,
            Some(CellInput::Value(v)) => match v {
                CellValueInput::Number(n) => CellResult::Value(*n),
                CellValueInput::Text(s) => CellResult::Text(s.clone()),
                CellValueInput::Bool(b) => CellResult::Bool(*b),
                CellValueInput::Error(e) => CellResult::Error(*e),
            },
            Some(CellInput::Formula(formula)) => {
                if let Some(expr) = self.formulas.get(formula).cloned() {
                    // Mark cell as being evaluated
                    self.evaluating.borrow_mut().insert(key);
                    let result = self.evaluate_expr_at(sheet, coord, &expr);
                    // Unmark cell after evaluation
                    self.evaluating.borrow_mut().remove(&key);
                    result
                } else {
                    CellResult::Error(CellError::Calc)
                }
            }
        }
    }

    /// Evaluate an expression as the formula of the cell at `coord`, which
    /// ROW() and COLUMN() report. Cells evaluated along the way get their
    /// own.
    pub fn evaluate_expr_at(&self, sheet: u32, coord: CellCoord, expr: &Expr) -> CellResult {
        let outer = self.current.replace(Some((sheet, coord)));
        let result = self.evaluate_expr(sheet, expr);
        self.current.set(outer);
        result
    }

    /// The cell whose formula is being evaluated, if any.
    pub(crate) fn current_cell(&self) -> Option<(u32, CellCoord)> {
        self.current.get()
    }

    /// Evaluate an expression
    pub fn evaluate_expr(&self, sheet: u32, expr: &Expr) -> CellResult {
        match expr {
            Expr::Number(n) => CellResult::Value(*n),
            Expr::Text(s) => CellResult::Text(s.clone()),
            Expr::Bool(b) => CellResult::Bool(*b),
            Expr::Error(e) => CellResult::Error(*e),
            // An empty argument reads like an empty cell.
            Expr::Missing => CellResult::Empty,
            // As one value, an array constant is its only item; a larger
            // one, like a larger range, needs a function that reads it whole.
            Expr::Array(rows) => match rows.as_slice() {
                [row] if row.len() == 1 => self.evaluate_expr(sheet, &row[0]),
                _ => CellResult::Error(CellError::Value),
            },

            Expr::CellRef(r) => match self.resolve_sheet(r.sheet.as_deref(), sheet) {
                Ok(ref_sheet) => self.get_value(ref_sheet, r.coord),
                Err(e) => CellResult::Error(e),
            },

            Expr::RangeRef(r) => match self.resolve_sheet(r.sheet.as_deref(), sheet) {
                // One cell is its value; a larger range has no single value
                // and needs a function that reads it whole.
                Ok(s) if r.range.cell_count() == 1 => self.get_value(s, r.range.start),
                Ok(_) => CellResult::Error(CellError::Value),
                Err(e) => CellResult::Error(e),
            },

            Expr::Unary { .. } | Expr::Binary { .. } | Expr::Chain { .. } => {
                self.evaluate_operand(sheet, expr).into_single()
            }

            Expr::Function(func) => self.evaluate_function(func, sheet),
        }
    }

    /// Evaluate `expr` with its operators working item by item over
    /// ranges and array constants. Whole columns and rows are read only as
    /// far down or across as their sheets are used (see [`Lines`]).
    pub(crate) fn evaluate_operand(&self, sheet: u32, expr: &Expr) -> Operand {
        let mut lines = Lines::default();
        self.lines_in(sheet, expr, &mut lines);
        self.operand(sheet, expr, lines)
    }

    /// `expr` as an array, for functions that read an argument whole; one
    /// value is a 1 x 1 array.
    pub(crate) fn evaluate_array(&self, sheet: u32, expr: &Expr) -> Result<Array, CellError> {
        match self.evaluate_operand(sheet, expr) {
            Operand::Array(array) => Ok(array),
            Operand::Single(CellResult::Error(e)) => Err(e),
            Operand::Single(v) => Ok(Array::new(1, 1, vec![v])),
        }
    }

    fn operand(&self, sheet: u32, expr: &Expr, lines: Lines) -> Operand {
        match expr {
            Expr::RangeRef(r) => match self.resolve_sheet(r.sheet.as_deref(), sheet) {
                Ok(s) => {
                    let range = match r.kind {
                        RangeKind::Cells => r.range,
                        RangeKind::Columns => leading(&r.range, (lines.rows, r.range.width())),
                        RangeKind::Rows => leading(&r.range, (r.range.height(), lines.cols)),
                    };
                    self.range_operand(s, &range)
                }
                Err(e) => Operand::Single(CellResult::Error(e)),
            },
            Expr::Array(rows) => Operand::Array(Array::from_literals(rows)),
            Expr::Function(f) if f.name == "INDIRECT" || f.name == "OFFSET" => {
                match self.functions.reference(expr, sheet, self) {
                    Some(Ok((s, range))) => {
                        // Known only now: trim whole lines with their own
                        // sheet's extent too.
                        let mut lines = lines;
                        let columns = range.start.row == 0 && range.end.row == MAX_ROW;
                        let rows = range.start.col == 0 && range.end.col == MAX_COL;
                        self.widen_lines(s, columns, rows, &mut lines);
                        let (height, width) = (range.height(), range.width());
                        let range = leading(
                            &range,
                            (
                                if columns { lines.rows } else { height },
                                if rows { lines.cols } else { width },
                            ),
                        );
                        self.range_operand(s, &range)
                    }
                    Some(Err(e)) => Operand::Single(CellResult::Error(e)),
                    None => Operand::Single(self.evaluate_expr(sheet, expr)),
                }
            }
            Expr::Unary { op, operand } => {
                let op = *op;
                self.operand(sheet, operand, lines).map(|v| unary_op(op, v))
            }
            Expr::Binary { op, left, right } => {
                let op = *op;
                let left = self.operand(sheet, left, lines);
                let right = self.operand(sheet, right, lines);
                left.combine(right, |l, r| binary_op(op, l, r))
            }
            // Left to right, as the Binary nodes it stands for would go.
            Expr::Chain { first, rest } => {
                let mut acc = self.operand(sheet, first, lines);
                for (op, operand) in rest {
                    let op = *op;
                    let right = self.operand(sheet, operand, lines);
                    acc = acc.combine(right, |l, r| binary_op(op, l, r));
                }
                acc
            }
            other => Operand::Single(self.evaluate_expr(sheet, other)),
        }
    }

    /// The cells of `range` on `sheet` as an array, reading only its used
    /// part: the rest is blank. #VALUE! past [`MAX_ARRAY_ITEMS`].
    fn range_operand(&self, sheet: u32, range: &CellRange) -> Operand {
        if range.cell_count() > MAX_ARRAY_ITEMS {
            return Operand::Single(CellResult::Error(CellError::Value));
        }
        let cols = range.width() as usize;
        let mut items = vec![CellResult::Empty; range.cell_count() as usize];
        for coord in self.used_part(sheet, range).iter().flat_map(|p| p.iter()) {
            let row = (coord.row - range.start.row) as usize;
            let col = (coord.col - range.start.col) as usize;
            items[row * cols + col] = self.get_value(sheet, coord);
        }
        Operand::Array(Array::new(range.height(), range.width(), items))
    }

    /// Widen `lines` to the whole columns and rows `expr`'s operators read.
    /// Function arguments are their own expressions and aren't looked into.
    fn lines_in(&self, sheet: u32, expr: &Expr, lines: &mut Lines) {
        match expr {
            Expr::RangeRef(r) if r.kind != RangeKind::Cells => {
                if let Ok(s) = self.resolve_sheet(r.sheet.as_deref(), sheet) {
                    let columns = r.kind == RangeKind::Columns;
                    self.widen_lines(s, columns, !columns, lines);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.lines_in(sheet, left, lines);
                self.lines_in(sheet, right, lines);
            }
            Expr::Unary { operand, .. } => self.lines_in(sheet, operand, lines),
            Expr::Chain { first, rest } => {
                for operand in Expr::chain_operands(first, rest) {
                    self.lines_in(sheet, operand, lines);
                }
            }
            _ => {}
        }
    }

    /// Reach `lines` to the last row (for whole `columns`) and the last
    /// column (for whole `rows`) with input on `sheet`.
    fn widen_lines(&self, sheet: u32, columns: bool, rows: bool, lines: &mut Lines) {
        let Some(max) = self.sheet_max_coord(sheet) else {
            return;
        };
        if columns {
            lines.rows = lines.rows.max(max.row + 1);
        }
        if rows {
            lines.cols = lines.cols.max(max.col + 1);
        }
    }

    /// Evaluate a function call
    ///
    /// Now takes &self instead of &mut self, enabled by interior mutability
    /// on cache and evaluating fields. This eliminates the borrow conflict
    /// where self.functions.evaluate() needed &mut self while self.functions
    /// was already borrowed.
    fn evaluate_function(&self, func: &crate::formula::FunctionCall, sheet: u32) -> CellResult {
        self.functions.evaluate(func, sheet, self)
    }

    /// Invalidate a cell and all its dependents, and every volatile cell
    /// with its dependents, since any edit recalculates those.
    fn invalidate(&mut self, sheet: u32, coord: CellCoord) {
        self.touch();
        self.drop_cached(Some((sheet, coord)));
    }

    /// Drop the cached values of `root`, of every volatile cell, and of
    /// everything downstream of them.
    fn drop_cached(&mut self, root: Option<Key>) {
        // Use a worklist algorithm to avoid stack overflow on cyclic dependencies
        let cache = self.cache.get_mut();
        // Nothing computed yet, as while loading: nothing to drop, and no
        // scan of the range readers.
        if cache.is_empty() {
            return;
        }
        let mut to_invalidate: Vec<Key> = root.into_iter().collect();
        to_invalidate.extend(self.volatile.iter().filter(|k| cache.contains_key(k)));
        let mut invalidated = HashSet::new();
        // Range readers still to scan, per sheet. A reader leaves its list
        // once queued or invalidated, so a cascade over many readers scans
        // each one about once, not once per invalidated cell.
        let mut candidates: HashMap<u32, Vec<(Key, &Vec<CellRange>)>> = HashMap::new();

        while let Some(key) = to_invalidate.pop() {
            // Skip if already invalidated (handles cycles)
            if !invalidated.insert(key) {
                continue;
            }
            // Nothing downstream of an uncached cell is cached.
            if cache.remove(&key).is_none() && Some(key) != root {
                continue;
            }

            // Queue dependents for invalidation
            if let Some(deps) = self.dependents.get(&key) {
                to_invalidate.extend(deps.iter().filter(|d| !invalidated.contains(d)));
            }
            let readers = candidates.entry(key.0).or_insert_with(|| {
                self.range_dependents
                    .get(&key.0)
                    .map(|readers| readers.iter().map(|(k, r)| (*k, r)).collect())
                    .unwrap_or_default()
            });
            readers.retain(|(reader, ranges)| {
                if invalidated.contains(reader) {
                    return false;
                }
                if ranges.iter().any(|r| r.contains(key.1)) {
                    to_invalidate.push(*reader);
                    return false;
                }
                true
            });
        }
    }

    /// Clear dependencies for a cell
    fn clear_cell_deps(&mut self, sheet: u32, coord: CellCoord) {
        let key = (sheet, coord);
        if let Some(deps) = self.dependencies.remove(&key) {
            for dep in deps {
                if let Some(dependents) = self.dependents.get_mut(&dep) {
                    dependents.remove(&key);
                }
            }
        }
        for readers in self.range_dependents.values_mut() {
            readers.remove(&key);
        }
        self.volatile.remove(&key);
    }

    /// Drop everything derived from the inputs.
    fn clear_derived(&mut self) {
        self.formulas.clear();
        self.dependents.clear();
        self.dependencies.clear();
        self.range_dependents.clear();
        self.formula_cells.clear();
        self.volatile.clear();
        *self.extents.get_mut() = None;
        self.cache.get_mut().clear();
        self.evaluating.get_mut().clear();
    }

    /// Collect values from a range for function evaluation
    pub fn collect_range_values(
        &self,
        sheet: u32,
        range: &crate::cell::CellRange,
    ) -> Vec<CellResult> {
        range
            .iter()
            .map(|coord| self.get_value(sheet, coord))
            .collect()
    }

    /// Resolve a sheet qualifier to an index. Unqualified refs use `current`.
    pub fn resolve_sheet(&self, qualifier: Option<&str>, current: u32) -> Result<u32, CellError> {
        let Some(name) = qualifier else {
            return Ok(current);
        };
        let name = name.trim_matches('\'');
        self.sheet_names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| i as u32)
            .ok_or(CellError::Ref)
    }

    pub fn set_sheet_names(&mut self, names: Vec<String>) {
        if self.sheet_names == names {
            return;
        }
        self.sheet_names = names;
        self.touch();
        self.rebind_formulas();
    }

    pub fn sheet_names(&self) -> &[String] {
        &self.sheet_names
    }

    /// Rewrite formula text after a tab rename, then rebind.
    pub fn rewrite_sheet_name(&mut self, old: &str, new: &str) {
        self.rename_pivot_sources(old, new);
        let items: Vec<(u32, CellCoord, String)> = self
            .inputs
            .iter()
            .filter_map(|(&(sheet, coord), input)| match input {
                CellInput::Formula(f) => Some((sheet, coord, f.clone())),
                _ => None,
            })
            .collect();

        // Every formula is set again: recomputing beats invalidating each.
        self.cache.get_mut().clear();
        for (sheet, coord, formula) in items {
            if let Ok(mut expr) = self.parser.parse(&formula) {
                expr.rename_sheet(old, new);
                let _ = self.set_formula(sheet, coord, &format!("={expr}"));
            }
        }
    }

    /// A sheet's formatting, if it has any.
    pub fn formatting(&self, sheet: u32) -> Option<&SheetFormatting> {
        self.formatting.get(&sheet)
    }

    pub fn formatting_mut(&mut self, sheet: u32) -> &mut SheetFormatting {
        self.touch();
        self.formatting.entry(sheet).or_default()
    }

    pub fn cell_format(&self, sheet: u32, coord: CellCoord) -> Option<&CellFormat> {
        self.formatting(sheet)?.get(coord)
    }

    /// Set a cell's format; the default format clears it.
    pub fn set_cell_format(&mut self, sheet: u32, coord: CellCoord, format: CellFormat) {
        self.formatting_mut(sheet).set(coord, format);
    }

    pub(crate) fn all_inputs(&self) -> impl Iterator<Item = ((u32, CellCoord), &CellInput)> {
        self.inputs.iter().map(|(&k, v)| (k, v))
    }

    pub(crate) fn all_formatting(&self) -> &HashMap<u32, SheetFormatting> {
        &self.formatting
    }

    pub(crate) fn all_formatting_mut(&mut self) -> &mut HashMap<u32, SheetFormatting> {
        self.touch();
        &mut self.formatting
    }

    /// A cell's stored input (value or formula), if any.
    pub fn get_input(&self, sheet: u32, coord: CellCoord) -> Option<&CellInput> {
        self.inputs.get(&(sheet, coord))
    }

    /// Remove every input and all derived state, for whole-workbook rewrites
    /// that put cells back afterwards.
    pub(crate) fn take_all_inputs(&mut self) -> HashMap<(u32, CellCoord), CellInput> {
        let inputs = std::mem::take(&mut self.inputs);
        self.touch();
        self.clear_derived();
        inputs
    }

    /// Drop one sheet's cells and shift higher sheet keys down by one.
    pub fn remove_sheet_and_shift(&mut self, index: u32) {
        self.touch();
        self.formatting = std::mem::take(&mut self.formatting)
            .into_iter()
            .filter(|&(sheet, _)| sheet != index)
            .map(|(sheet, f)| (if sheet > index { sheet - 1 } else { sheet }, f))
            .collect();

        let snapshot: Vec<((u32, CellCoord), CellInput)> =
            std::mem::take(&mut self.inputs).into_iter().collect();
        self.clear_derived();

        for ((sheet, coord), input) in snapshot {
            if sheet == index {
                continue;
            }
            let new_sheet = if sheet > index { sheet - 1 } else { sheet };
            match input {
                CellInput::Empty => {}
                CellInput::Value(v) => self.set_value(new_sheet, coord, v),
                CellInput::Formula(f) => {
                    let _ = self.set_formula(new_sheet, coord, &f);
                }
            }
        }
    }

    fn rebind_formulas(&mut self) {
        let items: Vec<(u32, CellCoord, String)> = self
            .inputs
            .iter()
            .filter_map(|(&(sheet, coord), input)| match input {
                CellInput::Formula(f) => Some((sheet, coord, f.clone())),
                _ => None,
            })
            .collect();
        // Every formula is set again: recomputing beats invalidating each.
        self.cache.get_mut().clear();
        for (sheet, coord, formula) in items {
            let _ = self.set_formula(sheet, coord, &formula);
        }
    }
}

impl Default for CalcEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// How far operators read whole columns and rows: down to the last row
/// (across to the last column) that any of their sheets uses, which keeps
/// `A:A*B:B` to the used rows with both sides one shape. Past that every
/// item is blank and is left out, so unlike Excel `SUMPRODUCT(--(A:A=""))`
/// counts only the blanks among the used rows.
#[derive(Clone, Copy, Default)]
struct Lines {
    /// Rows a whole column keeps
    rows: u32,
    /// Columns a whole row keeps
    cols: u32,
}

/// `op` on one value: errors pass through, and `-` and `%` convert as
/// arithmetic does.
fn unary_op(op: UnaryOp, value: CellResult) -> CellResult {
    let number = |f: fn(f64) -> f64| match &value {
        CellResult::Error(e) => CellResult::Error(*e),
        v => v
            .as_number()
            .map_or(CellResult::Error(CellError::Value), |n| {
                CellResult::Value(f(n))
            }),
    };
    match op {
        UnaryOp::Neg => number(|n| -n),
        UnaryOp::Percent => number(|n| n / 100.0),
        UnaryOp::Pos => value,
    }
}

/// `left op right` on single values; an error on either side is the
/// answer, the left one first.
fn binary_op(op: BinaryOp, left: CellResult, right: CellResult) -> CellResult {
    if let CellResult::Error(e) = left {
        return CellResult::Error(e);
    }
    if let CellResult::Error(e) = right {
        return CellResult::Error(e);
    }
    let arithmetic = |f: fn(f64, f64) -> CellResult| match (left.as_number(), right.as_number()) {
        (Some(l), Some(r)) => f(l, r),
        _ => CellResult::Error(CellError::Value),
    };
    match op {
        BinaryOp::Add => arithmetic(|l, r| CellResult::Value(l + r)),
        BinaryOp::Sub => arithmetic(|l, r| CellResult::Value(l - r)),
        BinaryOp::Mul => arithmetic(|l, r| CellResult::Value(l * r)),
        BinaryOp::Div if right.as_number() == Some(0.0) && left.as_number().is_some() => {
            CellResult::Error(CellError::DivZero)
        }
        BinaryOp::Div => arithmetic(|l, r| CellResult::Value(l / r)),
        BinaryOp::Pow => arithmetic(|l, r| CellResult::Value(l.powf(r))),
        BinaryOp::Concat => match (left.to_text(), right.to_text()) {
            (Some(l), Some(r)) => CellResult::Text(l + &r),
            _ => CellResult::Error(CellError::Value),
        },
        BinaryOp::Eq => CellResult::Bool(compare_values(&left, &right).is_eq()),
        BinaryOp::Neq => CellResult::Bool(compare_values(&left, &right).is_ne()),
        BinaryOp::Lt => CellResult::Bool(compare_values(&left, &right).is_lt()),
        BinaryOp::Lte => CellResult::Bool(compare_values(&left, &right).is_le()),
        BinaryOp::Gt => CellResult::Bool(compare_values(&left, &right).is_gt()),
        BinaryOp::Gte => CellResult::Bool(compare_values(&left, &right).is_ge()),
    }
}

/// The first `rows` x `cols` cells of `range`.
pub(crate) fn leading(range: &CellRange, (rows, cols): (u32, u32)) -> CellRange {
    let rows = rows.clamp(1, range.height());
    let cols = cols.clamp(1, range.width());
    CellRange::new(
        range.start,
        CellCoord::new(range.start.row + rows - 1, range.start.col + cols - 1),
    )
}

/// Excel's comparison: numbers < text < logicals, with no conversion
/// between them, and text ignoring case. A blank compares as the other
/// side's zero value: 0, "" or FALSE.
fn compare_values(left: &CellResult, right: &CellResult) -> Ordering {
    let rank = |v: &CellResult| match v {
        CellResult::Value(_) => 0,
        CellResult::Text(_) => 1,
        CellResult::Bool(_) => 2,
        _ => 3,
    };
    let blank_like = |other: &CellResult| match other {
        CellResult::Text(_) => CellResult::Text(String::new()),
        CellResult::Bool(_) => CellResult::Bool(false),
        _ => CellResult::Value(0.0),
    };
    match (left, right) {
        (CellResult::Empty, CellResult::Empty) => Ordering::Equal,
        (CellResult::Empty, other) => compare_values(&blank_like(other), other),
        (other, CellResult::Empty) => compare_values(other, &blank_like(other)),
        (CellResult::Value(a), CellResult::Value(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (CellResult::Text(a), CellResult::Text(b)) => a
            .chars()
            .flat_map(char::to_lowercase)
            .cmp(b.chars().flat_map(char::to_lowercase)),
        (CellResult::Bool(a), CellResult::Bool(b)) => a.cmp(b),
        (a, b) => rank(a).cmp(&rank(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_value() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(42.0));
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(42.0)
        );
    }

    #[test]
    fn test_simple_formula() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine
            .set_formula(0, CellCoord::new(0, 1), "=A1*2")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(20.0)
        );
    }

    #[test]
    fn test_dependency_update() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine
            .set_formula(0, CellCoord::new(0, 1), "=A1+5")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(15.0)
        );

        // Update A1
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(20.0));
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(25.0)
        );
    }

    #[test]
    fn test_cycle_detection() {
        let mut engine = CalcEngine::new();
        engine.set_formula(0, CellCoord::new(0, 0), "=B1").unwrap();
        engine.set_formula(0, CellCoord::new(0, 1), "=A1").unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Error(CellError::Circular)
        );
    }

    #[test]
    fn test_div_zero() {
        let mut engine = CalcEngine::new();
        engine.set_formula(0, CellCoord::new(0, 0), "=1/0").unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Error(CellError::DivZero)
        );
    }

    #[test]
    fn test_cross_sheet_ref() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(5.0));
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Sheet2!A1")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(5.0)
        );

        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(9.0));
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(9.0)
        );
    }

    #[test]
    fn test_missing_sheet_is_ref() {
        let mut engine = CalcEngine::new();
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Nope!A1")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Error(CellError::Ref)
        );
    }

    #[test]
    fn test_remove_sheet_and_shift() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["S1".into(), "S2".into()]);
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(2.0));
        engine.remove_sheet_and_shift(0);
        engine.set_sheet_names(vec!["S2".into()]);
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(2.0)
        );
    }

    #[test]
    fn test_rename_sheet_rewrites_formula() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Data".into()]);
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(3.0));
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Data!A1")
            .unwrap();
        engine.rewrite_sheet_name("Data", "Numbers");
        engine.set_sheet_names(vec!["Sheet1".into(), "Numbers".into()]);
        assert_eq!(
            engine.get_formula(0, CellCoord::new(0, 0)).as_deref(),
            Some("=Numbers!A1")
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(3.0)
        );
    }

    #[test]
    fn renaming_a_sheet_to_digits_keeps_its_references() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Data".into()]);
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(3.0));
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Data!A1*2")
            .unwrap();
        engine.rewrite_sheet_name("Data", "2024");
        engine.set_sheet_names(vec!["Sheet1".into(), "2024".into()]);
        assert_eq!(
            engine.get_formula(0, CellCoord::new(0, 0)).as_deref(),
            Some("=('2024'!A1*2)")
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(6.0)
        );
    }

    #[test]
    fn test_cross_sheet_dependency_invalidation() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);

        // Set value on Sheet2
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(10.0));

        // Formula on Sheet1 referencing Sheet2
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Sheet2!A1*2")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(20.0)
        );

        // Update value on Sheet2 - should invalidate and recalc Sheet1's formula
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(15.0));
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(30.0)
        );
    }

    #[test]
    fn test_delete_sheet_referenced_by_formula() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "ToDelete".into(), "Sheet3".into()]);

        // Values
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(5.0));
        engine.set_value(2, CellCoord::new(0, 0), CellValueInput::Number(10.0));

        // Formula referencing the sheet we'll delete
        engine
            .set_formula(0, CellCoord::new(0, 0), "=ToDelete!A1")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(5.0)
        );

        // Remove ToDelete sheet (index 1)
        engine.remove_sheet_and_shift(1);
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet3".into()]);

        // Formula now references "ToDelete" which no longer exists
        // This should produce a #REF! error
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Error(CellError::Ref)
        );
    }

    #[test]
    fn test_rename_with_special_characters() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "My Data".into()]);

        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(42.0));

        // Reference sheet with space in name (must be quoted in Excel)
        engine
            .set_formula(0, CellCoord::new(0, 0), "='My Data'!A1")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(42.0)
        );

        // Rename to another name with space
        engine.rewrite_sheet_name("My Data", "New Data");
        engine.set_sheet_names(vec!["Sheet1".into(), "New Data".into()]);

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(42.0)
        );
    }

    #[test]
    fn test_multi_sheet_chain() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["A".into(), "B".into(), "C".into()]);

        // Chain: C!A1 -> B!A1 -> A!A1
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(5.0));
        engine
            .set_formula(1, CellCoord::new(0, 0), "=A!A1*2")
            .unwrap();
        engine
            .set_formula(2, CellCoord::new(0, 0), "=B!A1+10")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(5.0)
        );
        assert_eq!(
            engine.get_value(1, CellCoord::new(0, 0)),
            CellResult::Value(10.0)
        );
        assert_eq!(
            engine.get_value(2, CellCoord::new(0, 0)),
            CellResult::Value(20.0)
        );

        // Update root value
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(7.0));
        assert_eq!(
            engine.get_value(1, CellCoord::new(0, 0)),
            CellResult::Value(14.0)
        );
        assert_eq!(
            engine.get_value(2, CellCoord::new(0, 0)),
            CellResult::Value(24.0)
        );
    }

    #[test]
    fn test_cross_sheet_circular_ref() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);

        // Create circular reference across sheets
        engine
            .set_formula(0, CellCoord::new(0, 0), "=Sheet2!A1")
            .unwrap();
        engine
            .set_formula(1, CellCoord::new(0, 0), "=Sheet1!A1")
            .unwrap();

        // Both should detect circular dependency
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Error(CellError::Circular)
        );
    }

    #[test]
    fn test_case_insensitive_sheet_names() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "DataSheet".into()]);

        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(100.0));

        // Reference with different casing
        engine
            .set_formula(0, CellCoord::new(0, 0), "=DATASHEET!A1")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=datasheet!A1")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(100.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(100.0)
        );
    }

    fn at(a1: &str) -> CellCoord {
        CellCoord::from_a1(a1).unwrap()
    }

    fn num(engine: &mut CalcEngine, a1: &str, n: f64) {
        engine.set_value(0, at(a1), CellValueInput::Number(n));
    }

    fn formula(engine: &mut CalcEngine, a1: &str, f: &str) {
        engine.set_formula(0, at(a1), f).unwrap();
    }

    fn value(engine: &CalcEngine, a1: &str) -> CellResult {
        engine.get_value(0, at(a1))
    }

    fn cached(engine: &CalcEngine, a1: &str) -> bool {
        engine.cache.borrow().contains_key(&(0, at(a1)))
    }

    /// Value of a one-off formula on a scratch cell.
    fn eval(engine: &mut CalcEngine, f: &str) -> CellResult {
        formula(engine, "Z1000", f);
        value(engine, "Z1000")
    }

    #[test]
    fn huge_ranges_are_tracked_whole() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 1.0);
        num(&mut engine, "A2", 2.0);
        formula(&mut engine, "B1", "=SUM(A1:A1048576)");
        formula(&mut engine, "D5", "=SUM(A:A)+COUNTA(1:1)");

        assert!(engine.dependents.is_empty());
        assert!(engine.dependencies.is_empty());
        assert_eq!(engine.range_dependents[&0].len(), 2);
        assert_eq!(engine.range_dependents[&0][&(0, at("D5"))].len(), 2);
        assert_eq!(value(&engine, "B1"), CellResult::Value(3.0));
        assert_eq!(value(&engine, "D5"), CellResult::Value(5.0));
        // Evaluating stops at the used rows.
        assert!(engine.cache.borrow().len() < 20);
    }

    #[test]
    fn edits_invalidate_range_readers_only_inside_their_ranges() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 1.0);
        num(&mut engine, "A2", 2.0);
        formula(&mut engine, "B1", "=SUM(A1:A100)");
        assert_eq!(value(&engine, "B1"), CellResult::Value(3.0));

        num(&mut engine, "A101", 50.0);
        num(&mut engine, "C5", 50.0);
        assert!(cached(&engine, "B1"));

        num(&mut engine, "A100", 10.0);
        assert!(!cached(&engine, "B1"));
        assert_eq!(value(&engine, "B1"), CellResult::Value(13.0));

        // Through a chain: C1 reads B1, which reads the range.
        formula(&mut engine, "C1", "=B1*2");
        assert_eq!(value(&engine, "C1"), CellResult::Value(26.0));
        num(&mut engine, "A50", 1.0);
        assert_eq!(value(&engine, "C1"), CellResult::Value(28.0));
    }

    #[test]
    fn replacing_a_formula_drops_its_ranges() {
        let mut engine = CalcEngine::new();
        formula(&mut engine, "B1", "=SUM(A:A)");
        assert_eq!(engine.range_dependents[&0].len(), 1);
        num(&mut engine, "B1", 5.0);
        assert!(engine.range_dependents[&0].is_empty());
        formula(&mut engine, "B2", "=MAX(A1:A3)");
        engine.clear(0, at("B2"));
        assert!(engine.range_dependents[&0].is_empty());
    }

    #[test]
    fn formula_inside_its_own_range_is_circular() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 1.0);
        formula(&mut engine, "A3", "=SUM(A1:A5)");
        assert_eq!(value(&engine, "A3"), CellResult::Error(CellError::Circular));
        formula(&mut engine, "B2", "=SUM(B:B)");
        assert_eq!(value(&engine, "B2"), CellResult::Error(CellError::Circular));
    }

    #[test]
    fn indirect_chains_past_the_depth_cap_are_calc_errors() {
        const N: u32 = 1_000;
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        for row in 1..N {
            engine
                .set_formula(
                    0,
                    CellCoord::new(row, 0),
                    &format!("=INDIRECT(\"A{row}\")+1"),
                )
                .unwrap();
        }
        // The graph can't see through INDIRECT, so this evaluates by
        // recursion: too deep is an error, not a blown stack.
        assert_eq!(
            engine.get_value(0, CellCoord::new(N - 1, 0)),
            CellResult::Error(CellError::Calc)
        );
        // A provisional answer is not cached: shallower reads compute for
        // real afterwards.
        assert_eq!(
            engine.get_value(0, CellCoord::new(99, 0)),
            CellResult::Value(100.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(N - 1, 0)),
            CellResult::Error(CellError::Calc)
        );
    }

    #[test]
    fn hand_built_ragged_arrays_do_not_panic() {
        // The parser rejects ragged rows; built by hand they must still
        // evaluate, short rows reading as #N/A.
        let engine = CalcEngine::new();
        let array = Expr::Array(vec![
            vec![Expr::Number(1.0), Expr::Number(2.0)],
            vec![Expr::Number(3.0)],
        ]);
        let sum = Expr::Function(crate::formula::FunctionCall {
            name: "SUM".into(),
            args: vec![array],
        });
        assert_eq!(
            engine.evaluate_expr(0, &sum),
            CellResult::Error(CellError::NA)
        );
    }

    #[test]
    fn long_chains_evaluate_without_deep_recursion() {
        const N: u32 = 50_000;
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        for row in 1..N {
            engine
                .set_formula(0, CellCoord::new(row, 0), &format!("=A{row}+1"))
                .unwrap();
        }
        assert_eq!(
            engine.get_value(0, CellCoord::new(N - 1, 0)),
            CellResult::Value(N as f64)
        );

        // Editing the root recomputes the whole chain the same way.
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(2.0));
        assert_eq!(
            engine.get_value(0, CellCoord::new(N - 1, 0)),
            CellResult::Value(N as f64 + 1.0)
        );
    }

    #[test]
    fn long_chains_through_ranges_evaluate_without_deep_recursion() {
        const N: u32 = 10_000;
        let mut engine = CalcEngine::new();
        // Each row sums the row below it; the last holds a number.
        engine.set_value(0, CellCoord::new(N - 1, 1), CellValueInput::Number(1.0));
        for row in 1..N {
            let below = row + 1;
            engine
                .set_formula(
                    0,
                    CellCoord::new(row - 1, 1),
                    &format!("=SUM(B{below}:B{below})+1"),
                )
                .unwrap();
        }
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(N as f64)
        );
    }

    #[test]
    fn whole_column_and_row_functions() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 1.0);
        num(&mut engine, "A3", 2.0);
        num(&mut engine, "A7", 4.0);
        engine.set_value(0, at("B2"), CellValueInput::Text("x".into()));
        engine.set_value(0, at("B5"), CellValueInput::Text("X".into()));
        num(&mut engine, "C3", 10.0);

        assert_eq!(eval(&mut engine, "=SUM(A:A)"), CellResult::Value(7.0));
        assert_eq!(eval(&mut engine, "=SUM($A:A,3:3)"), CellResult::Value(19.0));
        assert_eq!(
            eval(&mut engine, "=COUNTIF(B:B,\"x\")"),
            CellResult::Value(2.0)
        );
        assert_eq!(
            eval(&mut engine, "=SUMIF(B:B,\"x\",A:A)"),
            CellResult::Value(0.0)
        );
        assert_eq!(
            eval(&mut engine, "=SUMIF(A:A,\">1\",C:C)"),
            CellResult::Value(10.0)
        );
        assert_eq!(
            eval(&mut engine, "=COUNTIFS(A:A,\">0\",C:C,\">5\")"),
            CellResult::Value(1.0)
        );
        assert_eq!(
            eval(&mut engine, "=SUMPRODUCT(A:A,C:C)"),
            CellResult::Value(20.0)
        );
        assert_eq!(eval(&mut engine, "=MATCH(4,A:A,0)"), CellResult::Value(7.0));
        assert_eq!(
            eval(&mut engine, "=VLOOKUP(2,A:C,3,FALSE)"),
            CellResult::Value(10.0)
        );
        assert_eq!(
            eval(&mut engine, "=ROWS(A:B)*COLUMNS(A:B)"),
            CellResult::Value(2_097_152.0)
        );
        // Blanks past the used rows still count.
        assert_eq!(
            eval(&mut engine, "=COUNTBLANK(C:C)"),
            CellResult::Value(1_048_575.0)
        );
        assert_eq!(
            eval(&mut engine, "=COUNTIF(C:C,\"\")"),
            CellResult::Value(1_048_575.0)
        );
        assert_eq!(
            eval(&mut engine, "=COUNTIFS(A:A,\"\",C:C,\"\")"),
            CellResult::Value(1_048_573.0)
        );
        assert_eq!(eval(&mut engine, "=INDEX(A:A,7)"), CellResult::Value(4.0));
    }

    #[test]
    fn whole_columns_follow_edits() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 1.0);
        formula(&mut engine, "B1", "=SUM(A:A)");
        assert_eq!(value(&engine, "B1"), CellResult::Value(1.0));
        num(&mut engine, "A1048576", 5.0);
        assert_eq!(value(&engine, "B1"), CellResult::Value(6.0));
        engine.clear(0, at("A1048576"));
        assert_eq!(value(&engine, "B1"), CellResult::Value(1.0));
    }

    #[test]
    fn percent_and_leading_dot() {
        let mut engine = CalcEngine::new();
        num(&mut engine, "A1", 200.0);
        assert_eq!(eval(&mut engine, "=10%"), CellResult::Value(0.1));
        assert_eq!(eval(&mut engine, "=A1*5%"), CellResult::Value(10.0));
        assert_eq!(eval(&mut engine, "=-2%"), CellResult::Value(-0.02));
        assert_eq!(eval(&mut engine, "=.5+1"), CellResult::Value(1.5));
        assert_eq!(eval(&mut engine, "=2^300%"), CellResult::Value(8.0));
        assert_eq!(
            eval(&mut engine, "=(1/0)%"),
            CellResult::Error(CellError::DivZero)
        );
        assert_eq!(
            eval(&mut engine, "=\"a\"%"),
            CellResult::Error(CellError::Value)
        );
    }

    #[test]
    fn comparisons_follow_excel() {
        let mut engine = CalcEngine::new();
        let t = CellResult::Bool(true);
        let f = CellResult::Bool(false);
        // Text ignores case, for equality and order.
        assert_eq!(eval(&mut engine, "=\"ABC\"=\"abc\""), t);
        assert_eq!(eval(&mut engine, "=\"abc\"<>\"ABC\""), f);
        assert_eq!(eval(&mut engine, "=\"a\"<\"B\""), t);
        assert_eq!(eval(&mut engine, "=\"apple\">\"Apple pie\""), f);
        assert_eq!(eval(&mut engine, "=EXACT(\"ABC\",\"abc\")"), f);
        // No conversion between types: number < text < logical.
        assert_eq!(eval(&mut engine, "=1=\"1\""), f);
        assert_eq!(eval(&mut engine, "=TRUE=1"), f);
        assert_eq!(eval(&mut engine, "=99999<\"a\""), t);
        assert_eq!(eval(&mut engine, "=\"zzz\"<FALSE"), t);
        assert_eq!(eval(&mut engine, "=TRUE>FALSE"), t);
        assert_eq!(eval(&mut engine, "=2>=2"), t);
        // A blank is the other side's zero value.
        assert_eq!(eval(&mut engine, "=Q1=0"), t);
        assert_eq!(eval(&mut engine, "=Q1=\"\""), t);
        assert_eq!(eval(&mut engine, "=Q1=FALSE"), t);
        assert_eq!(eval(&mut engine, "=Q1<1"), t);
        assert_eq!(eval(&mut engine, "=Q1<\"a\""), t);
        assert_eq!(eval(&mut engine, "=Q1=Q2"), t);
        // Arithmetic still converts.
        assert_eq!(eval(&mut engine, "=TRUE+1"), CellResult::Value(2.0));
        assert_eq!(
            eval(&mut engine, "=\"a\"+1"),
            CellResult::Error(CellError::Value)
        );
    }

    #[test]
    fn concatenation_uses_general_format() {
        let mut engine = CalcEngine::new();
        let text = |s: &str| CellResult::Text(s.into());
        assert_eq!(eval(&mut engine, "=\"x\"&1E15"), text("x1E+15"));
        assert_eq!(eval(&mut engine, "=\"x\"&0.1"), text("x0.1"));
        assert_eq!(eval(&mut engine, "=\"x\"&(0.1+0.2)"), text("x0.3"));
        assert_eq!(eval(&mut engine, "=1&TRUE&Q1"), text("1TRUE"));
        assert_eq!(eval(&mut engine, "=CONCATENATE(\"x\",1/4)"), text("x0.25"));
    }

    #[test]
    fn long_chains_evaluate_and_follow_their_cells() {
        let mut engine = CalcEngine::new();
        for row in 0..1500 {
            engine.set_value(0, CellCoord::new(row, 0), CellValueInput::Number(1.0));
        }
        let terms: Vec<String> = (1..=1500).map(|r| format!("A{r}")).collect();
        formula(&mut engine, "B1", &format!("={}", terms.join("+")));
        assert_eq!(value(&engine, "B1"), CellResult::Value(1500.0));
        num(&mut engine, "A1500", 2.0);
        assert_eq!(value(&engine, "B1"), CellResult::Value(1501.0));
        assert_eq!(
            eval(&mut engine, &format!("=1{}", "+1".repeat(4000))),
            CellResult::Value(4001.0)
        );
        assert_eq!(
            eval(&mut engine, "=10-2-3*2/4+1"),
            CellResult::Value(10.0 - 2.0 - 1.5 + 1.0)
        );
        assert_eq!(
            eval(&mut engine, "=\"a\"&1&TRUE"),
            CellResult::Text("a1TRUE".into())
        );
        assert_eq!(
            eval(&mut engine, "=1+\"x\"+1/0"),
            CellResult::Error(CellError::Value)
        );
        // Renaming a sheet rewrites references inside a chain.
        engine.set_sheet_names(vec!["Sheet1".into(), "Data".into()]);
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(5.0));
        formula(&mut engine, "C1", "=Data!A1+Data!A1*2+1");
        engine.rewrite_sheet_name("Data", "2024");
        engine.set_sheet_names(vec!["Sheet1".into(), "2024".into()]);
        assert_eq!(
            engine.get_formula(0, at("C1")).as_deref(),
            Some("=('2024'!A1+('2024'!A1*2)+1)")
        );
        assert_eq!(value(&engine, "C1"), CellResult::Value(16.0));
    }

    #[test]
    fn the_deepest_accepted_formulas_evaluate_on_a_test_sized_stack() {
        // The parser's depth guard admits these. Release builds parse and
        // evaluate them in under 1 MiB, the Windows main thread's stack; a
        // debug build's parse of the ^ chain takes about 1.5 MiB.
        let deepest = [
            format!("={}1", "-".repeat(510)),
            format!("=2{}", "^1".repeat(510)),
            format!("=1{}", ">2".repeat(510)),
        ];
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || {
                let mut engine = CalcEngine::new();
                for (f, want) in deepest.iter().zip([1.0, 2.0]) {
                    assert_eq!(eval(&mut engine, f), CellResult::Value(want), "{f}");
                }
                assert_eq!(eval(&mut engine, &deepest[2]), CellResult::Bool(true));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn volatile_cells_recalculate_after_any_edit() {
        let mut engine = CalcEngine::new();
        formula(&mut engine, "A1", "=RAND()");
        formula(&mut engine, "B1", "=A1*2");
        formula(&mut engine, "C1", "=1+1");
        assert!(engine.volatile.contains(&(0, at("A1"))));
        assert!(!engine.volatile.contains(&(0, at("B1"))));
        let _ = value(&engine, "B1");
        let _ = value(&engine, "C1");
        assert!(cached(&engine, "A1") && cached(&engine, "B1"));

        num(&mut engine, "Z9", 1.0);
        assert!(!cached(&engine, "A1"));
        assert!(!cached(&engine, "B1"));
        assert!(cached(&engine, "C1"));
        let (CellResult::Value(a), CellResult::Value(b)) =
            (value(&engine, "A1"), value(&engine, "B1"))
        else {
            panic!("expected numbers");
        };
        assert_eq!(b, a * 2.0);

        // No longer volatile once replaced.
        num(&mut engine, "A1", 3.0);
        assert!(engine.volatile.is_empty());
    }

    #[test]
    fn now_and_today_follow_the_clock_offset() {
        use crate::calc::functions::date_to_serial;
        /// 2024-03-09 23:30 UTC, half an hour before midnight.
        fn late_evening() -> f64 {
            let days = date_to_serial(2024, 3, 9) - date_to_serial(1970, 1, 1);
            days * 86_400.0 + 23.5 * 3_600.0
        }
        let (march_9, march_10) = (date_to_serial(2024, 3, 9), date_to_serial(2024, 3, 10));
        let utc_now = march_9 + 23.5 / 24.0;
        let number = |engine: &CalcEngine, a1| match value(engine, a1) {
            CellResult::Value(n) => n,
            other => panic!("{a1} is {other:?}"),
        };

        let mut engine = CalcEngine::new();
        engine.set_clock(late_evening);
        formula(&mut engine, "A1", "=TODAY()");
        formula(&mut engine, "B1", "=NOW()");
        formula(&mut engine, "C1", "=B1-A1");
        formula(&mut engine, "D1", "=1+1");
        assert_eq!(engine.clock_offset(), 0, "UTC unless set");
        assert_eq!(value(&engine, "A1"), CellResult::Value(march_9));
        assert!((number(&engine, "B1") - utc_now).abs() < 1e-9);
        let _ = value(&engine, "C1");
        let _ = value(&engine, "D1");

        // An hour east of UTC it is already tomorrow.
        engine.set_clock_offset(60);
        assert!(!cached(&engine, "A1") && !cached(&engine, "B1") && !cached(&engine, "C1"));
        assert!(cached(&engine, "D1"), "only volatile cells recalculate");
        assert_eq!(value(&engine, "A1"), CellResult::Value(march_10));
        assert!((number(&engine, "B1") - (utc_now + 60.0 / 1440.0)).abs() < 1e-9);
        assert!((number(&engine, "C1") - 0.5 / 24.0).abs() < 1e-9);

        // The same offset again keeps what is cached.
        engine.set_clock_offset(60);
        assert!(cached(&engine, "A1") && cached(&engine, "C1"));

        // Exactly midnight belongs to the new day; a minute short does not.
        engine.set_clock_offset(30);
        assert_eq!(value(&engine, "A1"), CellResult::Value(march_10));
        assert_eq!(value(&engine, "B1"), CellResult::Value(march_10));
        engine.set_clock_offset(29);
        assert_eq!(value(&engine, "A1"), CellResult::Value(march_9));

        // West of UTC, the same instant is earlier the same day.
        engine.set_clock_offset(-5 * 60);
        assert_eq!(value(&engine, "A1"), CellResult::Value(march_9));
        assert!((number(&engine, "B1") - (utc_now - 300.0 / 1440.0)).abs() < 1e-9);
    }
}
