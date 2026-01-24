use crate::cell::{CellCoord, CellError};
use crate::formula::{BinaryOp, Expr, UnaryOp, FormulaParser};
use crate::calc::functions::BuiltinFunctions;
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
        }
    }

    /// Set a cell's value (not a formula)
    pub fn set_value(&mut self, sheet: u32, coord: CellCoord, value: CellValueInput) {
        self.clear_cell_deps(sheet, coord);
        self.inputs.insert((sheet, coord), CellInput::Value(value));
        self.invalidate(sheet, coord);
    }

    /// Set a cell's formula
    pub fn set_formula(&mut self, sheet: u32, coord: CellCoord, formula: &str) -> Result<(), String> {
        // Parse and validate formula
        let expr = self.parser.parse(formula).map_err(|e| e.to_string())?;

        // Clear old dependencies
        self.clear_cell_deps(sheet, coord);

        // Store formula
        self.formulas.insert(formula.to_string(), Arc::new(expr.clone()));
        self.inputs.insert((sheet, coord), CellInput::Formula(formula.to_string()));

        // Build new dependencies
        let mut deps = Vec::new();
        expr.collect_dependencies(&mut deps);
        for dep in deps {
            let dep_coord = dep.coord;
            let dep_sheet = 0u32; // TODO: resolve sheet name
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
        self.cache.borrow_mut().insert((sheet, coord), result.clone());
        result
    }

    /// Get the formula string for a cell, if it has one
    pub fn get_formula(&self, sheet: u32, coord: CellCoord) -> Option<String> {
        match self.inputs.get(&(sheet, coord)) {
            Some(CellInput::Formula(f)) => Some(f.clone()),
            _ => None,
        }
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

            Expr::CellRef(r) => {
                let ref_sheet = 0u32; // TODO: resolve sheet name
                self.get_value(ref_sheet, r.coord)
            }

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

            Expr::Function(func) => {
                self.evaluate_function(func, sheet)
            }
        }
    }

    fn evaluate_binary_op(&self, op: BinaryOp, left: CellResult, right: CellResult) -> CellResult {
        match op {
            BinaryOp::Add => {
                match (left.as_number(), right.as_number()) {
                    (Some(l), Some(r)) => CellResult::Value(l + r),
                    _ => CellResult::Error(CellError::Value),
                }
            }
            BinaryOp::Sub => {
                match (left.as_number(), right.as_number()) {
                    (Some(l), Some(r)) => CellResult::Value(l - r),
                    _ => CellResult::Error(CellError::Value),
                }
            }
            BinaryOp::Mul => {
                match (left.as_number(), right.as_number()) {
                    (Some(l), Some(r)) => CellResult::Value(l * r),
                    _ => CellResult::Error(CellError::Value),
                }
            }
            BinaryOp::Div => {
                match (left.as_number(), right.as_number()) {
                    (Some(_), Some(r)) if r == 0.0 => CellResult::Error(CellError::DivZero),
                    (Some(l), Some(r)) => CellResult::Value(l / r),
                    _ => CellResult::Error(CellError::Value),
                }
            }
            BinaryOp::Pow => {
                match (left.as_number(), right.as_number()) {
                    (Some(l), Some(r)) => CellResult::Value(l.powf(r)),
                    _ => CellResult::Error(CellError::Value),
                }
            }
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
    pub fn collect_range_values(&self, sheet: u32, range: &crate::cell::CellRange) -> Vec<CellResult> {
        range.iter().map(|coord| self.get_value(sheet, coord)).collect()
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
        assert_eq!(engine.get_value(0, CellCoord::new(0, 0)), CellResult::Value(42.0));
    }

    #[test]
    fn test_simple_formula() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_formula(0, CellCoord::new(0, 1), "=A1*2").unwrap();
        assert_eq!(engine.get_value(0, CellCoord::new(0, 1)), CellResult::Value(20.0));
    }

    #[test]
    fn test_dependency_update() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_formula(0, CellCoord::new(0, 1), "=A1+5").unwrap();

        assert_eq!(engine.get_value(0, CellCoord::new(0, 1)), CellResult::Value(15.0));

        // Update A1
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(20.0));
        assert_eq!(engine.get_value(0, CellCoord::new(0, 1)), CellResult::Value(25.0));
    }

    #[test]
    fn test_cycle_detection() {
        let mut engine = CalcEngine::new();
        engine.set_formula(0, CellCoord::new(0, 0), "=B1").unwrap();
        engine.set_formula(0, CellCoord::new(0, 1), "=A1").unwrap();

        assert_eq!(engine.get_value(0, CellCoord::new(0, 0)), CellResult::Error(CellError::Circular));
    }

    #[test]
    fn test_div_zero() {
        let mut engine = CalcEngine::new();
        engine.set_formula(0, CellCoord::new(0, 0), "=1/0").unwrap();
        assert_eq!(engine.get_value(0, CellCoord::new(0, 0)), CellResult::Error(CellError::DivZero));
    }
}
