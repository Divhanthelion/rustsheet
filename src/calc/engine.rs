use crate::calc::functions::BuiltinFunctions;
use crate::cell::{CellCoord, CellError};
use crate::formula::{BinaryOp, Expr, FormulaParser, UnaryOp};
use salsa;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Salsa database trait for incremental computation
#[salsa::db]
pub trait CalcDb: salsa::Database {
    /// Get raw cell input (user-entered value or formula)
    fn cell_input(&self, sheet: u32, coord: CellCoord) -> CellInput;

    /// Get computed cell value (with caching and dependency tracking)
    fn cell_value(&self, sheet: u32, coord: CellCoord) -> CellResult;
}

/// Input for a cell (either a value or formula string)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellInput {
    Empty,
    Value(CellValueInput),
    Formula(String),
}

/// Simplified cell value for salsa (no Box)
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
}

/// The calculation engine - manages formula evaluation with incremental computation
///
/// Uses interior mutability (RefCell) for cache and cycle detection state to allow
/// recursive evaluation through function calls without violating borrow rules.
pub struct CalcEngine {
    /// Formula parser
    parser: FormulaParser,
    /// Parsed formula ASTs (indexed by formula string hash)
    formulas: HashMap<String, Arc<Expr>>,
    /// Cell inputs per sheet
    inputs: HashMap<(u32, CellCoord), CellInput>,
    /// Computed values cache - uses RefCell for interior mutability during evaluation
    cache: RefCell<HashMap<(u32, CellCoord), CellResult>>,
    /// Dependency graph: cell -> cells that depend on it
    dependents: HashMap<(u32, CellCoord), HashSet<(u32, CellCoord)>>,
    /// Reverse dependency: cell -> cells it depends on
    dependencies: HashMap<(u32, CellCoord), HashSet<(u32, CellCoord)>>,
    /// Cells currently being evaluated (for cycle detection) - uses RefCell for interior mutability
    evaluating: RefCell<HashSet<(u32, CellCoord)>>,
    /// Built-in functions
    functions: BuiltinFunctions,
    /// Tab names, index-aligned with sheet keys
    sheet_names: Vec<String>,
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
            evaluating: RefCell::new(HashSet::new()),
            functions: BuiltinFunctions::new(),
            sheet_names: vec!["Sheet1".to_string()],
        }
    }

    /// Set a cell's value (not a formula)
    pub fn set_value(&mut self, sheet: u32, coord: CellCoord, value: CellValueInput) {
        self.clear_cell_deps(sheet, coord);
        self.inputs.insert((sheet, coord), CellInput::Value(value));
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

        // Clear old dependencies
        self.clear_cell_deps(sheet, coord);

        // Store formula
        self.formulas
            .insert(formula.to_string(), Arc::new(expr.clone()));
        self.inputs
            .insert((sheet, coord), CellInput::Formula(formula.to_string()));

        // Build new dependencies
        let mut deps = Vec::new();
        expr.collect_dependencies(&mut deps);
        for dep in deps {
            let Ok(dep_sheet) = self.resolve_sheet(dep.sheet.as_deref(), sheet) else {
                continue;
            };
            let dep_coord = dep.coord;
            self.dependencies
                .entry((sheet, coord))
                .or_default()
                .insert((dep_sheet, dep_coord));
            self.dependents
                .entry((dep_sheet, dep_coord))
                .or_default()
                .insert((sheet, coord));
        }

        self.invalidate(sheet, coord);
        Ok(())
    }

    /// Clear a cell
    pub fn clear(&mut self, sheet: u32, coord: CellCoord) {
        self.clear_cell_deps(sheet, coord);
        self.inputs.remove(&(sheet, coord));
        self.invalidate(sheet, coord);
    }

    /// Get computed value for a cell
    ///
    /// Uses interior mutability to allow recursive calls during function evaluation
    /// without requiring &mut self, which would conflict with the borrow of self.functions.
    pub fn get_value(&self, sheet: u32, coord: CellCoord) -> CellResult {
        // Check cache first
        if let Some(cached) = self.cache.borrow().get(&(sheet, coord)) {
            return cached.clone();
        }

        // Compute value
        let result = self.compute(sheet, coord);
        self.cache
            .borrow_mut()
            .insert((sheet, coord), result.clone());
        result
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
        self.iter_sheet_inputs(sheet)
            .map(|(coord, _)| coord)
            .reduce(|a, b| CellCoord::new(a.row.max(b.row), a.col.max(b.col)))
    }

    /// Compute a cell's value
    fn compute(&self, sheet: u32, coord: CellCoord) -> CellResult {
        // Cycle detection - check if this cell is already being evaluated
        let is_cycle = self.evaluating.borrow().contains(&(sheet, coord));
        if is_cycle {
            return CellResult::Error(CellError::Circular);
        }

        let input = self.inputs.get(&(sheet, coord)).cloned();

        match input {
            None | Some(CellInput::Empty) => CellResult::Empty,
            Some(CellInput::Value(v)) => match v {
                CellValueInput::Number(n) => CellResult::Value(n),
                CellValueInput::Text(s) => CellResult::Text(s),
                CellValueInput::Bool(b) => CellResult::Bool(b),
                CellValueInput::Error(e) => CellResult::Error(e),
            },
            Some(CellInput::Formula(formula)) => {
                if let Some(expr) = self.formulas.get(&formula).cloned() {
                    // Mark cell as being evaluated
                    self.evaluating.borrow_mut().insert((sheet, coord));
                    let result = self.evaluate_expr(sheet, &expr);
                    // Unmark cell after evaluation
                    self.evaluating.borrow_mut().remove(&(sheet, coord));
                    result
                } else {
                    CellResult::Error(CellError::Calc)
                }
            }
        }
    }

    /// Evaluate an expression
    pub fn evaluate_expr(&self, sheet: u32, expr: &Expr) -> CellResult {
        match expr {
            Expr::Number(n) => CellResult::Value(*n),
            Expr::Text(s) => CellResult::Text(s.clone()),
            Expr::Bool(b) => CellResult::Bool(*b),
            Expr::Error(e) => CellResult::Error(*e),

            Expr::CellRef(r) => match self.resolve_sheet(r.sheet.as_deref(), sheet) {
                Ok(ref_sheet) => self.get_value(ref_sheet, r.coord),
                Err(e) => CellResult::Error(e),
            },

            Expr::RangeRef(_) => {
                // Ranges can't be evaluated to a single value outside functions
                CellResult::Error(CellError::Value)
            }

            Expr::Unary { op, operand } => {
                let val = self.evaluate_expr(sheet, operand);
                match op {
                    UnaryOp::Neg => match val.as_number() {
                        Some(n) => CellResult::Value(-n),
                        None => CellResult::Error(CellError::Value),
                    },
                    UnaryOp::Pos => val,
                }
            }

            Expr::Binary { op, left, right } => {
                let lval = self.evaluate_expr(sheet, left);
                let rval = self.evaluate_expr(sheet, right);

                // Propagate errors
                if let CellResult::Error(e) = &lval {
                    return CellResult::Error(*e);
                }
                if let CellResult::Error(e) = &rval {
                    return CellResult::Error(*e);
                }

                self.evaluate_binary_op(*op, lval, rval)
            }

            Expr::Function(func) => self.evaluate_function(func, sheet),
        }
    }

    fn evaluate_binary_op(&self, op: BinaryOp, left: CellResult, right: CellResult) -> CellResult {
        match op {
            BinaryOp::Add => match (left.as_number(), right.as_number()) {
                (Some(l), Some(r)) => CellResult::Value(l + r),
                _ => CellResult::Error(CellError::Value),
            },
            BinaryOp::Sub => match (left.as_number(), right.as_number()) {
                (Some(l), Some(r)) => CellResult::Value(l - r),
                _ => CellResult::Error(CellError::Value),
            },
            BinaryOp::Mul => match (left.as_number(), right.as_number()) {
                (Some(l), Some(r)) => CellResult::Value(l * r),
                _ => CellResult::Error(CellError::Value),
            },
            BinaryOp::Div => match (left.as_number(), right.as_number()) {
                (Some(_), Some(0.0)) => CellResult::Error(CellError::DivZero),
                (Some(l), Some(r)) => CellResult::Value(l / r),
                _ => CellResult::Error(CellError::Value),
            },
            BinaryOp::Pow => match (left.as_number(), right.as_number()) {
                (Some(l), Some(r)) => CellResult::Value(l.powf(r)),
                _ => CellResult::Error(CellError::Value),
            },
            BinaryOp::Concat => {
                let l_str = match &left {
                    CellResult::Text(s) => s.clone(),
                    CellResult::Value(n) => n.to_string(),
                    CellResult::Bool(b) => b.to_string().to_uppercase(),
                    CellResult::Empty => String::new(),
                    _ => return CellResult::Error(CellError::Value),
                };
                let r_str = match &right {
                    CellResult::Text(s) => s.clone(),
                    CellResult::Value(n) => n.to_string(),
                    CellResult::Bool(b) => b.to_string().to_uppercase(),
                    CellResult::Empty => String::new(),
                    _ => return CellResult::Error(CellError::Value),
                };
                CellResult::Text(format!("{}{}", l_str, r_str))
            }
            BinaryOp::Eq => self.compare_values(&left, &right, |a, b| a == b),
            BinaryOp::Neq => self.compare_values(&left, &right, |a, b| a != b),
            BinaryOp::Lt => self.compare_numbers(&left, &right, |a, b| a < b),
            BinaryOp::Lte => self.compare_numbers(&left, &right, |a, b| a <= b),
            BinaryOp::Gt => self.compare_numbers(&left, &right, |a, b| a > b),
            BinaryOp::Gte => self.compare_numbers(&left, &right, |a, b| a >= b),
        }
    }

    fn compare_values<F>(&self, left: &CellResult, right: &CellResult, f: F) -> CellResult
    where
        F: Fn(&CellResult, &CellResult) -> bool,
    {
        CellResult::Bool(f(left, right))
    }

    fn compare_numbers<F>(&self, left: &CellResult, right: &CellResult, f: F) -> CellResult
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left.as_number(), right.as_number()) {
            (Some(l), Some(r)) => CellResult::Bool(f(l, r)),
            _ => CellResult::Error(CellError::Value),
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

    /// Invalidate a cell and all its dependents
    fn invalidate(&mut self, sheet: u32, coord: CellCoord) {
        // Use a worklist algorithm to avoid stack overflow on cyclic dependencies
        let mut to_invalidate = vec![(sheet, coord)];
        let mut invalidated = HashSet::new();

        while let Some((s, c)) = to_invalidate.pop() {
            // Skip if already invalidated (handles cycles)
            if !invalidated.insert((s, c)) {
                continue;
            }

            self.cache.borrow_mut().remove(&(s, c));

            // Queue dependents for invalidation
            if let Some(deps) = self.dependents.get(&(s, c)) {
                for &(dep_sheet, dep_coord) in deps {
                    if !invalidated.contains(&(dep_sheet, dep_coord)) {
                        to_invalidate.push((dep_sheet, dep_coord));
                    }
                }
            }
        }
    }

    /// Clear dependencies for a cell
    fn clear_cell_deps(&mut self, sheet: u32, coord: CellCoord) {
        if let Some(deps) = self.dependencies.remove(&(sheet, coord)) {
            for (dep_sheet, dep_coord) in deps {
                if let Some(dependents) = self.dependents.get_mut(&(dep_sheet, dep_coord)) {
                    dependents.remove(&(sheet, coord));
                }
            }
        }
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
        self.rebind_formulas();
    }

    pub fn sheet_names(&self) -> &[String] {
        &self.sheet_names
    }

    /// Rewrite formula text after a tab rename, then rebind.
    pub fn rewrite_sheet_name(&mut self, old: &str, new: &str) {
        let items: Vec<(u32, CellCoord, String)> = self
            .inputs
            .iter()
            .filter_map(|(&(sheet, coord), input)| match input {
                CellInput::Formula(f) => Some((sheet, coord, f.clone())),
                _ => None,
            })
            .collect();

        for (sheet, coord, formula) in items {
            if let Ok(mut expr) = self.parser.parse(&formula) {
                expr.rename_sheet(old, new);
                let _ = self.set_formula(sheet, coord, &format!("={expr}"));
            }
        }
    }

    /// Drop one sheet's cells and shift higher sheet keys down by one.
    pub fn remove_sheet_and_shift(&mut self, index: u32) {
        let snapshot: Vec<((u32, CellCoord), CellInput)> =
            std::mem::take(&mut self.inputs).into_iter().collect();
        self.formulas.clear();
        self.dependents.clear();
        self.dependencies.clear();
        self.cache.borrow_mut().clear();
        self.evaluating.borrow_mut().clear();

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
}
