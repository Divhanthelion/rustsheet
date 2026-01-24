use crate::cell::CellError;
use crate::calc::engine::{CalcEngine, CellResult};
use crate::formula::{Expr, FunctionCall};

/// Built-in spreadsheet functions
pub struct BuiltinFunctions {
    // Could store function metadata here
}

impl BuiltinFunctions {
    pub fn new() -> Self {
        Self {}
    }

    /// Evaluate a function call
    ///
    /// Takes &CalcEngine instead of &mut CalcEngine. The CalcEngine uses interior
    /// mutability (RefCell) for its cache and cycle-detection state, allowing
    /// get_value() to be called with only a shared reference.
    pub fn evaluate(&self, func: &FunctionCall, sheet: u32, engine: &CalcEngine) -> CellResult {
        match func.name.as_str() {
            // ===== Math functions =====
            "SUM" => self.eval_sum(&func.args, sheet, engine),
            "AVERAGE" => self.eval_average(&func.args, sheet, engine),
            "MIN" => self.eval_min(&func.args, sheet, engine),
            "MAX" => self.eval_max(&func.args, sheet, engine),
            "COUNT" => self.eval_count(&func.args, sheet, engine),
            "COUNTA" => self.eval_counta(&func.args, sheet, engine),
            "ABS" => self.eval_abs(&func.args, sheet, engine),
            "ROUND" => self.eval_round(&func.args, sheet, engine),
            "ROUNDUP" => self.eval_roundup(&func.args, sheet, engine),
            "ROUNDDOWN" => self.eval_rounddown(&func.args, sheet, engine),
            "SQRT" => self.eval_sqrt(&func.args, sheet, engine),
            "POWER" => self.eval_power(&func.args, sheet, engine),
            "MOD" => self.eval_mod(&func.args, sheet, engine),
            "INT" => self.eval_int(&func.args, sheet, engine),
            "CEILING" => self.eval_ceiling(&func.args, sheet, engine),
            "FLOOR" => self.eval_floor(&func.args, sheet, engine),
            "SIGN" => self.eval_sign(&func.args, sheet, engine),
            "PI" => CellResult::Value(std::f64::consts::PI),
            "EXP" => self.eval_exp(&func.args, sheet, engine),
            "LN" => self.eval_ln(&func.args, sheet, engine),
            "LOG" => self.eval_log(&func.args, sheet, engine),
            "LOG10" => self.eval_log10(&func.args, sheet, engine),
            "SIN" => self.eval_trig(&func.args, sheet, engine, f64::sin),
            "COS" => self.eval_trig(&func.args, sheet, engine, f64::cos),
            "TAN" => self.eval_trig(&func.args, sheet, engine, f64::tan),
            "ASIN" => self.eval_trig(&func.args, sheet, engine, f64::asin),
            "ACOS" => self.eval_trig(&func.args, sheet, engine, f64::acos),
            "ATAN" => self.eval_trig(&func.args, sheet, engine, f64::atan),
            "RAND" => CellResult::Value(rand_simple()),
            "RANDBETWEEN" => self.eval_randbetween(&func.args, sheet, engine),
            "PRODUCT" => self.eval_product(&func.args, sheet, engine),
            "MEDIAN" => self.eval_median(&func.args, sheet, engine),

            // ===== Logical functions =====
            "IF" => self.eval_if(&func.args, sheet, engine),
            "AND" => self.eval_and(&func.args, sheet, engine),
            "OR" => self.eval_or(&func.args, sheet, engine),
            "NOT" => self.eval_not(&func.args, sheet, engine),
            "XOR" => self.eval_xor(&func.args, sheet, engine),
            "TRUE" => CellResult::Bool(true),
            "FALSE" => CellResult::Bool(false),
            "IFERROR" => self.eval_iferror(&func.args, sheet, engine),
            "IFNA" => self.eval_ifna(&func.args, sheet, engine),
            "IFS" => self.eval_ifs(&func.args, sheet, engine),
            "SWITCH" => self.eval_switch(&func.args, sheet, engine),
            "CHOOSE" => self.eval_choose(&func.args, sheet, engine),

            // ===== Text functions =====
            "LEN" => self.eval_len(&func.args, sheet, engine),
            "UPPER" => self.eval_upper(&func.args, sheet, engine),
            "LOWER" => self.eval_lower(&func.args, sheet, engine),
            "TRIM" => self.eval_trim(&func.args, sheet, engine),
            "CONCATENATE" | "CONCAT" => self.eval_concatenate(&func.args, sheet, engine),
            "LEFT" => self.eval_left(&func.args, sheet, engine),
            "RIGHT" => self.eval_right(&func.args, sheet, engine),
            "MID" => self.eval_mid(&func.args, sheet, engine),
            "FIND" => self.eval_find(&func.args, sheet, engine),
            "SEARCH" => self.eval_search(&func.args, sheet, engine),
            "SUBSTITUTE" => self.eval_substitute(&func.args, sheet, engine),
            "REPLACE" => self.eval_replace(&func.args, sheet, engine),
            "REPT" => self.eval_rept(&func.args, sheet, engine),
            "EXACT" => self.eval_exact(&func.args, sheet, engine),
            "VALUE" => self.eval_value(&func.args, sheet, engine),
            "TEXT" => self.eval_text(&func.args, sheet, engine),
            "CHAR" => self.eval_char(&func.args, sheet, engine),
            "CODE" => self.eval_code(&func.args, sheet, engine),
            "PROPER" => self.eval_proper(&func.args, sheet, engine),

            // ===== Lookup functions =====
            "VLOOKUP" => self.eval_vlookup(&func.args, sheet, engine),
            "HLOOKUP" => self.eval_hlookup(&func.args, sheet, engine),
            "INDEX" => self.eval_index(&func.args, sheet, engine),
            "MATCH" => self.eval_match(&func.args, sheet, engine),
            "ROW" => self.eval_row(&func.args, sheet, engine),
            "COLUMN" => self.eval_column(&func.args, sheet, engine),
            "ROWS" => self.eval_rows(&func.args),
            "COLUMNS" => self.eval_columns(&func.args),

            // ===== Conditional aggregation =====
            "SUMIF" => self.eval_sumif(&func.args, sheet, engine),
            "COUNTIF" => self.eval_countif(&func.args, sheet, engine),
            "AVERAGEIF" => self.eval_averageif(&func.args, sheet, engine),
            "COUNTBLANK" => self.eval_countblank(&func.args, sheet, engine),

            // ===== Info functions =====
            "ISBLANK" => self.eval_isblank(&func.args, sheet, engine),
            "ISERROR" => self.eval_iserror(&func.args, sheet, engine),
            "ISNUMBER" => self.eval_isnumber(&func.args, sheet, engine),
            "ISTEXT" => self.eval_istext(&func.args, sheet, engine),
            "ISLOGICAL" => self.eval_islogical(&func.args, sheet, engine),
            "ISNA" => self.eval_isna(&func.args, sheet, engine),
            "NA" => CellResult::Error(CellError::NA),
            "TYPE" => self.eval_type(&func.args, sheet, engine),
            "N" => self.eval_n(&func.args, sheet, engine),

            // ===== Date/Time functions =====
            "DATE" => self.eval_date(&func.args, sheet, engine),
            "YEAR" => self.eval_year(&func.args, sheet, engine),
            "MONTH" => self.eval_month(&func.args, sheet, engine),
            "DAY" => self.eval_day(&func.args, sheet, engine),
            "TODAY" => self.eval_today(),
            "NOW" => self.eval_now(),

            _ => CellResult::Error(CellError::Name),
        }
    }

    // ========== Math Functions ==========

    fn eval_sum(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values = self.collect_numeric_values(args, sheet, engine);
        if values.iter().any(|v| v.is_err()) {
            return CellResult::Error(CellError::Value);
        }
        let sum: f64 = values.into_iter().filter_map(|v| v.ok()).sum();
        CellResult::Value(sum)
    }

    fn eval_average(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values: Vec<f64> = self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter_map(|v| v.ok())
            .collect();

        if values.is_empty() {
            return CellResult::Error(CellError::DivZero);
        }

        let sum: f64 = values.iter().sum();
        CellResult::Value(sum / values.len() as f64)
    }

    fn eval_min(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values: Vec<f64> = self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter_map(|v| v.ok())
            .collect();

        if values.is_empty() {
            return CellResult::Value(0.0);
        }

        CellResult::Value(values.into_iter().fold(f64::INFINITY, f64::min))
    }

    fn eval_max(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values: Vec<f64> = self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter_map(|v| v.ok())
            .collect();

        if values.is_empty() {
            return CellResult::Value(0.0);
        }

        CellResult::Value(values.into_iter().fold(f64::NEG_INFINITY, f64::max))
    }

    fn eval_count(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let count = self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter(|v| v.is_ok())
            .count();
        CellResult::Value(count as f64)
    }

    fn eval_counta(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let count = self.collect_all_values(args, sheet, engine)
            .into_iter()
            .filter(|v| !matches!(v, CellResult::Empty))
            .count();
        CellResult::Value(count as f64)
    }

    fn eval_abs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) => CellResult::Value(n.abs()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_round(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 1 || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        let digits = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        match val.as_number() {
            Some(n) => {
                let multiplier = 10f64.powi(digits);
                CellResult::Value((n * multiplier).round() / multiplier)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_sqrt(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) if n < 0.0 => CellResult::Error(CellError::Num),
            Some(n) => CellResult::Value(n.sqrt()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_power(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }
        let base = self.eval_arg(&args[0], sheet, engine);
        let exp = self.eval_arg(&args[1], sheet, engine);

        match (base.as_number(), exp.as_number()) {
            (Some(b), Some(e)) => CellResult::Value(b.powf(e)),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_mod(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }
        let num = self.eval_arg(&args[0], sheet, engine);
        let divisor = self.eval_arg(&args[1], sheet, engine);

        match (num.as_number(), divisor.as_number()) {
            (Some(_), Some(d)) if d == 0.0 => CellResult::Error(CellError::DivZero),
            (Some(n), Some(d)) => CellResult::Value(n % d),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_int(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) => CellResult::Value(n.floor()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_ceiling(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let significance = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(1.0)
        } else {
            1.0
        };

        match val.as_number() {
            Some(n) if significance == 0.0 => CellResult::Value(0.0),
            Some(n) => CellResult::Value((n / significance).ceil() * significance),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_floor(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let significance = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(1.0)
        } else {
            1.0
        };

        match val.as_number() {
            Some(n) if significance == 0.0 => CellResult::Value(0.0),
            Some(n) => CellResult::Value((n / significance).floor() * significance),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_roundup(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let digits = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        match val.as_number() {
            Some(n) => {
                let multiplier = 10f64.powi(digits);
                let sign = if n >= 0.0 { 1.0 } else { -1.0 };
                CellResult::Value(sign * (n.abs() * multiplier).ceil() / multiplier)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_rounddown(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let digits = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as i32
        } else {
            0
        };

        match val.as_number() {
            Some(n) => {
                let multiplier = 10f64.powi(digits);
                let sign = if n >= 0.0 { 1.0 } else { -1.0 };
                CellResult::Value(sign * (n.abs() * multiplier).floor() / multiplier)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_sign(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) if n > 0.0 => CellResult::Value(1.0),
            Some(n) if n < 0.0 => CellResult::Value(-1.0),
            Some(_) => CellResult::Value(0.0),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_exp(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) => CellResult::Value(n.exp()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_ln(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) if n <= 0.0 => CellResult::Error(CellError::Num),
            Some(n) => CellResult::Value(n.ln()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_log(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let base = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(10.0)
        } else {
            10.0
        };

        match val.as_number() {
            Some(n) if n <= 0.0 || base <= 0.0 || base == 1.0 => CellResult::Error(CellError::Num),
            Some(n) => CellResult::Value(n.log(base)),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_log10(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) if n <= 0.0 => CellResult::Error(CellError::Num),
            Some(n) => CellResult::Value(n.log10()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_trig(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, f: fn(f64) -> f64) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) => CellResult::Value(f(n)),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_randbetween(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }
        let bottom = self.eval_arg(&args[0], sheet, engine);
        let top = self.eval_arg(&args[1], sheet, engine);

        match (bottom.as_number(), top.as_number()) {
            (Some(b), Some(t)) if b > t => CellResult::Error(CellError::Value),
            (Some(b), Some(t)) => {
                let range = t - b + 1.0;
                CellResult::Value((b + rand_simple() * range).floor())
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_product(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values = self.collect_numeric_values(args, sheet, engine);
        if values.iter().any(|v| v.is_err()) {
            return CellResult::Error(CellError::Value);
        }
        let product: f64 = values.into_iter().filter_map(|v| v.ok()).product();
        CellResult::Value(product)
    }

    fn eval_median(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let mut values: Vec<f64> = self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter_map(|v| v.ok())
            .collect();

        if values.is_empty() {
            return CellResult::Error(CellError::Num);
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = values.len() / 2;

        if values.len() % 2 == 0 {
            CellResult::Value((values[mid - 1] + values[mid]) / 2.0)
        } else {
            CellResult::Value(values[mid])
        }
    }

    // ========== Logical Functions ==========

    fn eval_if(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let condition = self.eval_arg(&args[0], sheet, engine);
        let cond_bool = condition.as_bool().unwrap_or(false);

        if cond_bool {
            self.eval_arg(&args[1], sheet, engine)
        } else if args.len() == 3 {
            self.eval_arg(&args[2], sheet, engine)
        } else {
            CellResult::Bool(false)
        }
    }

    fn eval_and(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() {
            return CellResult::Error(CellError::Value);
        }

        for arg in args {
            let val = self.eval_arg(arg, sheet, engine);
            match val.as_bool() {
                Some(false) => return CellResult::Bool(false),
                None => return CellResult::Error(CellError::Value),
                _ => {}
            }
        }
        CellResult::Bool(true)
    }

    fn eval_or(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() {
            return CellResult::Error(CellError::Value);
        }

        for arg in args {
            let val = self.eval_arg(arg, sheet, engine);
            match val.as_bool() {
                Some(true) => return CellResult::Bool(true),
                None => return CellResult::Error(CellError::Value),
                _ => {}
            }
        }
        CellResult::Bool(false)
    }

    fn eval_not(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_bool() {
            Some(b) => CellResult::Bool(!b),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_xor(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() {
            return CellResult::Error(CellError::Value);
        }

        let mut true_count = 0;
        for arg in args {
            let val = self.eval_arg(arg, sheet, engine);
            match val.as_bool() {
                Some(true) => true_count += 1,
                Some(false) => {}
                None => return CellResult::Error(CellError::Value),
            }
        }
        CellResult::Bool(true_count % 2 == 1)
    }

    fn eval_iferror(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        if val.is_error() {
            self.eval_arg(&args[1], sheet, engine)
        } else {
            val
        }
    }

    fn eval_ifna(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        if matches!(val, CellResult::Error(CellError::NA)) {
            self.eval_arg(&args[1], sheet, engine)
        } else {
            val
        }
    }

    fn eval_ifs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() % 2 != 0 {
            return CellResult::Error(CellError::Value);
        }

        for i in (0..args.len()).step_by(2) {
            let condition = self.eval_arg(&args[i], sheet, engine);
            if condition.as_bool().unwrap_or(false) {
                return self.eval_arg(&args[i + 1], sheet, engine);
            }
        }

        CellResult::Error(CellError::NA)
    }

    fn eval_switch(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 {
            return CellResult::Error(CellError::Value);
        }

        let expr_val = self.eval_arg(&args[0], sheet, engine);
        let has_default = args.len() % 2 == 0;
        let pairs_end = if has_default { args.len() - 1 } else { args.len() };

        for i in (1..pairs_end).step_by(2) {
            let case_val = self.eval_arg(&args[i], sheet, engine);
            if self.values_equal(&expr_val, &case_val) {
                return self.eval_arg(&args[i + 1], sheet, engine);
            }
        }

        if has_default {
            self.eval_arg(&args[args.len() - 1], sheet, engine)
        } else {
            CellResult::Error(CellError::NA)
        }
    }

    fn eval_choose(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 {
            return CellResult::Error(CellError::Value);
        }

        let index = self.eval_arg(&args[0], sheet, engine);
        match index.as_number() {
            Some(n) => {
                let idx = n as usize;
                if idx < 1 || idx >= args.len() {
                    CellResult::Error(CellError::Value)
                } else {
                    self.eval_arg(&args[idx], sheet, engine)
                }
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    // ========== Text Functions ==========

    fn eval_len(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match &val {
            CellResult::Text(s) => CellResult::Value(s.len() as f64),
            CellResult::Value(n) => CellResult::Value(n.to_string().len() as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_upper(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match &val {
            CellResult::Text(s) => CellResult::Text(s.to_uppercase()),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_lower(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match &val {
            CellResult::Text(s) => CellResult::Text(s.to_lowercase()),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_trim(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match &val {
            CellResult::Text(s) => CellResult::Text(s.trim().to_string()),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_concatenate(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let mut result = String::new();
        for arg in args {
            let val = self.eval_arg(arg, sheet, engine);
            let s = match &val {
                CellResult::Text(s) => s.clone(),
                CellResult::Value(n) => n.to_string(),
                CellResult::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
                CellResult::Empty => String::new(),
                CellResult::Error(e) => return CellResult::Error(*e),
            };
            result.push_str(&s);
        }
        CellResult::Text(result)
    }

    fn eval_left(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let num_chars = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(1.0) as usize
        } else {
            1
        };

        match text {
            Some(s) => CellResult::Text(s.chars().take(num_chars).collect()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_right(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.is_empty() || args.len() > 2 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let num_chars = if args.len() == 2 {
            self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(1.0) as usize
        } else {
            1
        };

        match text {
            Some(s) => {
                let len = s.chars().count();
                let skip = len.saturating_sub(num_chars);
                CellResult::Text(s.chars().skip(skip).collect())
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_mid(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 3 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let start = self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as usize;
        let num_chars = self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(0.0) as usize;

        if start < 1 {
            return CellResult::Error(CellError::Value);
        }

        match text {
            Some(s) => CellResult::Text(s.chars().skip(start - 1).take(num_chars).collect()),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_find(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let find_text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let within_text = self.to_string_val(&self.eval_arg(&args[1], sheet, engine));
        let start_num = if args.len() == 3 {
            self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(1.0) as usize
        } else {
            1
        };

        match (find_text, within_text) {
            (Some(find), Some(within)) => {
                if start_num < 1 || start_num > within.len() {
                    return CellResult::Error(CellError::Value);
                }
                let search_in = &within[(start_num - 1)..];
                match search_in.find(&find) {
                    Some(pos) => CellResult::Value((pos + start_num) as f64),
                    None => CellResult::Error(CellError::Value),
                }
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_search(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let find_text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let within_text = self.to_string_val(&self.eval_arg(&args[1], sheet, engine));
        let start_num = if args.len() == 3 {
            self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(1.0) as usize
        } else {
            1
        };

        match (find_text, within_text) {
            (Some(find), Some(within)) => {
                if start_num < 1 || start_num > within.len() + 1 {
                    return CellResult::Error(CellError::Value);
                }
                let search_in = within[(start_num - 1)..].to_lowercase();
                let find_lower = find.to_lowercase();
                match search_in.find(&find_lower) {
                    Some(pos) => CellResult::Value((pos + start_num) as f64),
                    None => CellResult::Error(CellError::Value),
                }
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_substitute(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 || args.len() > 4 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let old_text = self.to_string_val(&self.eval_arg(&args[1], sheet, engine));
        let new_text = self.to_string_val(&self.eval_arg(&args[2], sheet, engine));
        let instance_num = if args.len() == 4 {
            Some(self.eval_arg(&args[3], sheet, engine).as_number().unwrap_or(0.0) as usize)
        } else {
            None
        };

        match (text, old_text, new_text) {
            (Some(t), Some(old), Some(new)) => {
                if let Some(n) = instance_num {
                    // Replace nth occurrence
                    let mut result = t.clone();
                    let mut count = 0;
                    let mut search_start = 0;
                    while let Some(pos) = result[search_start..].find(&old) {
                        count += 1;
                        if count == n {
                            let abs_pos = search_start + pos;
                            result = format!("{}{}{}", &result[..abs_pos], new, &result[abs_pos + old.len()..]);
                            break;
                        }
                        search_start += pos + old.len();
                    }
                    CellResult::Text(result)
                } else {
                    CellResult::Text(t.replace(&old, &new))
                }
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_replace(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 4 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let start = self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as usize;
        let num_chars = self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(0.0) as usize;
        let new_text = self.to_string_val(&self.eval_arg(&args[3], sheet, engine));

        match (text, new_text) {
            (Some(t), Some(new)) if start >= 1 => {
                let chars: Vec<char> = t.chars().collect();
                let before: String = chars.iter().take(start - 1).collect();
                let after: String = chars.iter().skip(start - 1 + num_chars).collect();
                CellResult::Text(format!("{}{}{}", before, new, after))
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_rept(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let times = self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as usize;

        match text {
            Some(t) => CellResult::Text(t.repeat(times)),
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_exact(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let text1 = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        let text2 = self.to_string_val(&self.eval_arg(&args[1], sheet, engine));

        match (text1, text2) {
            (Some(t1), Some(t2)) => CellResult::Bool(t1 == t2),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_value(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        match text {
            Some(t) => match t.trim().parse::<f64>() {
                Ok(n) => CellResult::Value(n),
                Err(_) => CellResult::Error(CellError::Value),
            },
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_text(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        let _format = self.to_string_val(&self.eval_arg(&args[1], sheet, engine));

        // Simplified TEXT - just convert to string (full format support would be complex)
        match val {
            CellResult::Value(n) => CellResult::Text(n.to_string()),
            CellResult::Text(s) => CellResult::Text(s),
            CellResult::Bool(b) => CellResult::Text(if b { "TRUE" } else { "FALSE" }.to_string()),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_char(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(n) if n >= 1.0 && n <= 255.0 => {
                CellResult::Text(char::from(n as u8).to_string())
            }
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_code(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        match text {
            Some(t) if !t.is_empty() => CellResult::Value(t.chars().next().unwrap() as u32 as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_proper(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let text = self.to_string_val(&self.eval_arg(&args[0], sheet, engine));
        match text {
            Some(t) => {
                let mut result = String::new();
                let mut capitalize_next = true;
                for c in t.chars() {
                    if c.is_alphabetic() {
                        if capitalize_next {
                            result.extend(c.to_uppercase());
                            capitalize_next = false;
                        } else {
                            result.extend(c.to_lowercase());
                        }
                    } else {
                        result.push(c);
                        capitalize_next = !c.is_alphanumeric();
                    }
                }
                CellResult::Text(result)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    // ========== Lookup Functions ==========

    fn eval_vlookup(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 || args.len() > 4 {
            return CellResult::Error(CellError::Value);
        }

        let lookup_value = self.eval_arg(&args[0], sheet, engine);

        // Get range
        let range = match &args[1] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let col_index = match self.eval_arg(&args[2], sheet, engine).as_number() {
            Some(n) => n as u32,
            None => return CellResult::Error(CellError::Value),
        };

        if col_index < 1 || col_index > range.width() {
            return CellResult::Error(CellError::Ref);
        }

        let exact_match = if args.len() == 4 {
            !self.eval_arg(&args[3], sheet, engine).as_bool().unwrap_or(true)
        } else {
            false
        };

        // Search first column
        for row in range.start.row..=range.end.row {
            let cell_coord = crate::cell::CellCoord::new(row, range.start.col);
            let cell_val = engine.get_value(sheet, cell_coord);

            let matches = if exact_match {
                self.values_equal(&lookup_value, &cell_val)
            } else {
                // Approximate match - find largest value <= lookup_value
                match (lookup_value.as_number(), cell_val.as_number()) {
                    (Some(l), Some(c)) => c <= l,
                    _ => self.values_equal(&lookup_value, &cell_val),
                }
            };

            if matches {
                let result_coord = crate::cell::CellCoord::new(row, range.start.col + col_index - 1);
                return engine.get_value(sheet, result_coord);
            }
        }

        CellResult::Error(CellError::NA)
    }

    fn eval_hlookup(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 || args.len() > 4 {
            return CellResult::Error(CellError::Value);
        }

        let lookup_value = self.eval_arg(&args[0], sheet, engine);

        let range = match &args[1] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let row_index = match self.eval_arg(&args[2], sheet, engine).as_number() {
            Some(n) => n as u32,
            None => return CellResult::Error(CellError::Value),
        };

        if row_index < 1 || row_index > range.height() {
            return CellResult::Error(CellError::Ref);
        }

        let exact_match = if args.len() == 4 {
            !self.eval_arg(&args[3], sheet, engine).as_bool().unwrap_or(true)
        } else {
            false
        };

        // Search first row
        for col in range.start.col..=range.end.col {
            let cell_coord = crate::cell::CellCoord::new(range.start.row, col);
            let cell_val = engine.get_value(sheet, cell_coord);

            let matches = if exact_match {
                self.values_equal(&lookup_value, &cell_val)
            } else {
                match (lookup_value.as_number(), cell_val.as_number()) {
                    (Some(l), Some(c)) => c <= l,
                    _ => self.values_equal(&lookup_value, &cell_val),
                }
            };

            if matches {
                let result_coord = crate::cell::CellCoord::new(range.start.row + row_index - 1, col);
                return engine.get_value(sheet, result_coord);
            }
        }

        CellResult::Error(CellError::NA)
    }

    fn eval_index(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let range = match &args[0] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let row_num = self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as u32;
        let col_num = if args.len() == 3 {
            self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(1.0) as u32
        } else {
            1
        };

        if row_num < 1 || row_num > range.height() || col_num < 1 || col_num > range.width() {
            return CellResult::Error(CellError::Ref);
        }

        let coord = crate::cell::CellCoord::new(
            range.start.row + row_num - 1,
            range.start.col + col_num - 1,
        );
        engine.get_value(sheet, coord)
    }

    fn eval_match(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let lookup_value = self.eval_arg(&args[0], sheet, engine);

        let range = match &args[1] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let match_type = if args.len() == 3 {
            self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(1.0) as i32
        } else {
            1
        };

        // Determine if horizontal or vertical
        let is_horizontal = range.height() == 1;
        let count = if is_horizontal { range.width() } else { range.height() };

        for i in 0..count {
            let coord = if is_horizontal {
                crate::cell::CellCoord::new(range.start.row, range.start.col + i)
            } else {
                crate::cell::CellCoord::new(range.start.row + i, range.start.col)
            };

            let cell_val = engine.get_value(sheet, coord);

            let matches = match match_type {
                0 => self.values_equal(&lookup_value, &cell_val), // Exact match
                1 => {
                    // Largest value <= lookup_value (assumes sorted ascending)
                    match (lookup_value.as_number(), cell_val.as_number()) {
                        (Some(l), Some(c)) => c <= l,
                        _ => self.values_equal(&lookup_value, &cell_val),
                    }
                }
                -1 => {
                    // Smallest value >= lookup_value (assumes sorted descending)
                    match (lookup_value.as_number(), cell_val.as_number()) {
                        (Some(l), Some(c)) => c >= l,
                        _ => self.values_equal(&lookup_value, &cell_val),
                    }
                }
                _ => false,
            };

            if matches && match_type == 0 {
                return CellResult::Value((i + 1) as f64);
            }
        }

        // For approximate matches, return the last matching position
        if match_type != 0 {
            // Re-scan for the best match (simplified)
            for i in (0..count).rev() {
                let coord = if is_horizontal {
                    crate::cell::CellCoord::new(range.start.row, range.start.col + i)
                } else {
                    crate::cell::CellCoord::new(range.start.row + i, range.start.col)
                };

                let cell_val = engine.get_value(sheet, coord);
                if match_type == 1 {
                    if let (Some(l), Some(c)) = (lookup_value.as_number(), cell_val.as_number()) {
                        if c <= l {
                            return CellResult::Value((i + 1) as f64);
                        }
                    }
                } else if match_type == -1 {
                    if let (Some(l), Some(c)) = (lookup_value.as_number(), cell_val.as_number()) {
                        if c >= l {
                            return CellResult::Value((i + 1) as f64);
                        }
                    }
                }
            }
        }

        CellResult::Error(CellError::NA)
    }

    fn eval_row(&self, args: &[Expr], _sheet: u32, _engine: &CalcEngine) -> CellResult {
        if args.is_empty() {
            return CellResult::Error(CellError::Value); // Would need current cell context
        }
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        match &args[0] {
            Expr::CellRef(r) => CellResult::Value((r.coord.row + 1) as f64),
            Expr::RangeRef(r) => CellResult::Value((r.range.start.row + 1) as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_column(&self, args: &[Expr], _sheet: u32, _engine: &CalcEngine) -> CellResult {
        if args.is_empty() {
            return CellResult::Error(CellError::Value);
        }
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        match &args[0] {
            Expr::CellRef(r) => CellResult::Value((r.coord.col + 1) as f64),
            Expr::RangeRef(r) => CellResult::Value((r.range.start.col + 1) as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_rows(&self, args: &[Expr]) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        match &args[0] {
            Expr::RangeRef(r) => CellResult::Value(r.range.height() as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    fn eval_columns(&self, args: &[Expr]) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        match &args[0] {
            Expr::RangeRef(r) => CellResult::Value(r.range.width() as f64),
            _ => CellResult::Error(CellError::Value),
        }
    }

    // ========== Conditional Aggregation ==========

    fn eval_sumif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let range = match &args[0] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let sum_range = if args.len() == 3 {
            match &args[2] {
                Expr::RangeRef(r) => r.range,
                _ => return CellResult::Error(CellError::Value),
            }
        } else {
            range
        };

        let mut sum = 0.0;
        let coords: Vec<_> = range.iter().collect();
        let sum_coords: Vec<_> = sum_range.iter().collect();

        for (i, coord) in coords.iter().enumerate() {
            let cell_val = engine.get_value(sheet, *coord);
            if self.matches_criteria(&cell_val, &criteria) {
                if let Some(sum_coord) = sum_coords.get(i) {
                    if let Some(n) = engine.get_value(sheet, *sum_coord).as_number() {
                        sum += n;
                    }
                }
            }
        }

        CellResult::Value(sum)
    }

    fn eval_countif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let range = match &args[0] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let mut count = 0;
        for coord in range.iter() {
            let cell_val = engine.get_value(sheet, coord);
            if self.matches_criteria(&cell_val, &criteria) {
                count += 1;
            }
        }

        CellResult::Value(count as f64)
    }

    fn eval_averageif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let range = match &args[0] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let avg_range = if args.len() == 3 {
            match &args[2] {
                Expr::RangeRef(r) => r.range,
                _ => return CellResult::Error(CellError::Value),
            }
        } else {
            range
        };

        let mut sum = 0.0;
        let mut count = 0;
        let coords: Vec<_> = range.iter().collect();
        let avg_coords: Vec<_> = avg_range.iter().collect();

        for (i, coord) in coords.iter().enumerate() {
            let cell_val = engine.get_value(sheet, *coord);
            if self.matches_criteria(&cell_val, &criteria) {
                if let Some(avg_coord) = avg_coords.get(i) {
                    if let Some(n) = engine.get_value(sheet, *avg_coord).as_number() {
                        sum += n;
                        count += 1;
                    }
                }
            }
        }

        if count == 0 {
            CellResult::Error(CellError::DivZero)
        } else {
            CellResult::Value(sum / count as f64)
        }
    }

    fn eval_countblank(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }

        let range = match &args[0] {
            Expr::RangeRef(r) => r.range,
            _ => return CellResult::Error(CellError::Value),
        };

        let mut count = 0;
        for coord in range.iter() {
            let cell_val = engine.get_value(sheet, coord);
            if matches!(cell_val, CellResult::Empty) {
                count += 1;
            }
        }

        CellResult::Value(count as f64)
    }

    // ========== Info Functions ==========

    fn eval_isblank(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(matches!(val, CellResult::Empty))
    }

    fn eval_iserror(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(val.is_error())
    }

    fn eval_isnumber(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(matches!(val, CellResult::Value(_)))
    }

    fn eval_istext(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(matches!(val, CellResult::Text(_)))
    }

    fn eval_islogical(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(matches!(val, CellResult::Bool(_)))
    }

    fn eval_isna(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        CellResult::Bool(matches!(val, CellResult::Error(CellError::NA)))
    }

    fn eval_type(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        let type_num = match val {
            CellResult::Value(_) => 1.0,
            CellResult::Text(_) => 2.0,
            CellResult::Bool(_) => 4.0,
            CellResult::Error(_) => 16.0,
            CellResult::Empty => 1.0, // Excel treats blank as number
        };
        CellResult::Value(type_num)
    }

    fn eval_n(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val {
            CellResult::Value(n) => CellResult::Value(n),
            CellResult::Bool(true) => CellResult::Value(1.0),
            CellResult::Bool(false) => CellResult::Value(0.0),
            CellResult::Error(e) => CellResult::Error(e),
            _ => CellResult::Value(0.0),
        }
    }

    // ========== Date/Time Functions ==========

    fn eval_date(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 3 {
            return CellResult::Error(CellError::Value);
        }

        let year = self.eval_arg(&args[0], sheet, engine).as_number().unwrap_or(0.0) as i32;
        let month = self.eval_arg(&args[1], sheet, engine).as_number().unwrap_or(0.0) as i32;
        let day = self.eval_arg(&args[2], sheet, engine).as_number().unwrap_or(0.0) as i32;

        // Simplified date serial number calculation (Excel epoch: 1900-01-01 = 1)
        // This is a basic implementation - full date handling would need a proper library
        let serial = date_to_serial(year, month, day);
        if serial < 0.0 {
            CellResult::Error(CellError::Value)
        } else {
            CellResult::Value(serial)
        }
    }

    fn eval_year(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(serial) => {
                let (year, _, _) = serial_to_date(serial);
                CellResult::Value(year as f64)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_month(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(serial) => {
                let (_, month, _) = serial_to_date(serial);
                CellResult::Value(month as f64)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_day(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        let val = self.eval_arg(&args[0], sheet, engine);
        match val.as_number() {
            Some(serial) => {
                let (_, _, day) = serial_to_date(serial);
                CellResult::Value(day as f64)
            }
            None => CellResult::Error(CellError::Value),
        }
    }

    fn eval_today(&self) -> CellResult {
        // Return current date as serial number
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Days since Unix epoch + offset to Excel epoch (25569 days from 1900-01-01 to 1970-01-01)
        let days = (secs / 86400) as f64 + 25569.0;
        CellResult::Value(days)
    }

    fn eval_now(&self) -> CellResult {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        // Days since Unix epoch + offset to Excel epoch
        let days = secs / 86400.0 + 25569.0;
        CellResult::Value(days)
    }

    // ========== Helpers ==========

    fn eval_arg(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> CellResult {
        // Delegate to the engine's expression evaluator to handle all expression types
        // including binary operations, unary operations, and nested function calls
        engine.evaluate_expr(sheet, expr)
    }

    fn collect_numeric_values(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Vec<Result<f64, ()>> {
        let mut values = Vec::new();
        for arg in args {
            match arg {
                Expr::RangeRef(r) => {
                    for coord in r.range.iter() {
                        let val = engine.get_value(sheet, coord);
                        if let Some(n) = val.as_number() {
                            values.push(Ok(n));
                        } else if val.is_error() {
                            values.push(Err(()));
                        }
                        // Skip non-numeric, non-error values in ranges
                    }
                }
                _ => {
                    let val = self.eval_arg(arg, sheet, engine);
                    if let Some(n) = val.as_number() {
                        values.push(Ok(n));
                    } else if val.is_error() {
                        values.push(Err(()));
                    }
                }
            }
        }
        values
    }

    fn collect_all_values(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Vec<CellResult> {
        let mut values = Vec::new();
        for arg in args {
            match arg {
                Expr::RangeRef(r) => {
                    for coord in r.range.iter() {
                        values.push(engine.get_value(sheet, coord));
                    }
                }
                _ => {
                    values.push(self.eval_arg(arg, sheet, engine));
                }
            }
        }
        values
    }

    fn values_equal(&self, a: &CellResult, b: &CellResult) -> bool {
        match (a, b) {
            (CellResult::Value(x), CellResult::Value(y)) => (x - y).abs() < f64::EPSILON,
            (CellResult::Text(x), CellResult::Text(y)) => x.eq_ignore_ascii_case(y),
            (CellResult::Bool(x), CellResult::Bool(y)) => x == y,
            (CellResult::Empty, CellResult::Empty) => true,
            _ => false,
        }
    }

    /// Convert a CellResult to an Option<String> for text functions
    fn to_string_val(&self, val: &CellResult) -> Option<String> {
        match val {
            CellResult::Text(s) => Some(s.clone()),
            CellResult::Value(n) => Some(n.to_string()),
            CellResult::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.to_string()),
            CellResult::Empty => Some(String::new()),
            CellResult::Error(_) => None,
        }
    }

    /// Check if a cell value matches a criteria (for SUMIF, COUNTIF, etc.)
    fn matches_criteria(&self, cell_val: &CellResult, criteria: &CellResult) -> bool {
        // Handle text criteria with operators
        if let CellResult::Text(crit_str) = criteria {
            let crit_str = crit_str.trim();

            // Check for comparison operators
            if let Some(rest) = crit_str.strip_prefix(">=") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(false, |n| n >= crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix("<=") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(false, |n| n <= crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix("<>") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(true, |n| (n - crit_num).abs() > f64::EPSILON);
                } else {
                    // String comparison
                    return !self.to_string_val(cell_val)
                        .map_or(false, |s| s.eq_ignore_ascii_case(rest.trim()));
                }
            } else if let Some(rest) = crit_str.strip_prefix('>') {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(false, |n| n > crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix('<') {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(false, |n| n < crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix('=') {
                // Explicit equality
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return cell_val.as_number().map_or(false, |n| (n - crit_num).abs() < f64::EPSILON);
                } else {
                    return self.to_string_val(cell_val)
                        .map_or(false, |s| s.eq_ignore_ascii_case(rest.trim()));
                }
            }

            // No operator - try numeric comparison first, then string
            if let Ok(crit_num) = crit_str.parse::<f64>() {
                return cell_val.as_number().map_or(false, |n| (n - crit_num).abs() < f64::EPSILON);
            }

            // Wildcard matching (* and ?)
            if crit_str.contains('*') || crit_str.contains('?') {
                if let Some(cell_str) = self.to_string_val(cell_val) {
                    return wildcard_match(&crit_str.to_lowercase(), &cell_str.to_lowercase());
                }
                return false;
            }

            // Plain string comparison (case-insensitive)
            return self.to_string_val(cell_val)
                .map_or(false, |s| s.eq_ignore_ascii_case(crit_str));
        }

        // Non-text criteria: direct value comparison
        self.values_equal(cell_val, criteria)
    }
}

/// Simple pseudo-random number generator (deterministic for reproducibility in tests)
/// Uses a simple linear congruential generator seeded from system time
fn rand_simple() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEED: AtomicU64 = AtomicU64::new(0);

    // Initialize seed from time if zero
    let mut seed = SEED.load(Ordering::Relaxed);
    if seed == 0 {
        seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        SEED.store(seed, Ordering::Relaxed);
    }

    // LCG parameters (same as glibc)
    let new_seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    SEED.store(new_seed, Ordering::Relaxed);

    // Convert to [0, 1) range
    ((new_seed >> 16) & 0x7fff) as f64 / 32768.0
}

/// Convert year, month, day to Excel serial date number
/// Excel's epoch is 1900-01-01 = 1 (with the infamous 1900 leap year bug)
fn date_to_serial(year: i32, month: i32, day: i32) -> f64 {
    // Adjust for months outside 1-12
    let mut y = year;
    let mut m = month;

    if m < 1 {
        y -= (1 - m) / 12 + 1;
        m = 12 - (1 - m) % 12;
    } else if m > 12 {
        y += (m - 1) / 12;
        m = (m - 1) % 12 + 1;
    }

    // Basic date calculation (simplified, doesn't handle all edge cases)
    // Days from 1900-01-01
    let mut days: i32 = 0;

    // Years contribution
    for yr in 1900..y {
        days += if is_leap_year(yr) { 366 } else { 365 };
    }

    // Months contribution
    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for mon in 1..m {
        days += days_in_month[mon as usize];
        if mon == 2 && is_leap_year(y) {
            days += 1;
        }
    }

    // Days
    days += day;

    // Excel has a bug where 1900 is treated as a leap year (it isn't)
    // So dates >= March 1, 1900 are off by 1
    if days >= 60 {
        days += 1;
    }

    days as f64
}

/// Convert Excel serial date number to (year, month, day)
fn serial_to_date(serial: f64) -> (i32, i32, i32) {
    let mut days = serial as i32;

    // Account for Excel's 1900 leap year bug
    if days > 60 {
        days -= 1;
    }

    let mut year = 1900;
    let mut remaining = days;

    // Find year
    loop {
        let year_days = if is_leap_year(year) { 366 } else { 365 };
        if remaining <= year_days {
            break;
        }
        remaining -= year_days;
        year += 1;
    }

    // Find month
    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;

    loop {
        let mut month_days = days_in_month[month as usize];
        if month == 2 && is_leap_year(year) {
            month_days += 1;
        }
        if remaining <= month_days {
            break;
        }
        remaining -= month_days;
        month += 1;
    }

    (year, month, remaining)
}

/// Check if a year is a leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Simple wildcard matching for SUMIF/COUNTIF criteria
/// Supports * (any characters) and ? (single character)
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<_> = pattern.chars().collect();
    let text_chars: Vec<_> = text.chars().collect();

    wildcard_match_helper(&pattern_chars, &text_chars, 0, 0)
}

fn wildcard_match_helper(pattern: &[char], text: &[char], mut pi: usize, mut ti: usize) -> bool {
    while pi < pattern.len() {
        if pattern[pi] == '*' {
            // Skip consecutive *
            while pi < pattern.len() && pattern[pi] == '*' {
                pi += 1;
            }
            if pi == pattern.len() {
                return true; // Trailing * matches everything
            }
            // Try matching * with 0 to n characters
            while ti <= text.len() {
                if wildcard_match_helper(pattern, text, pi, ti) {
                    return true;
                }
                ti += 1;
            }
            return false;
        } else if ti >= text.len() {
            return false;
        } else if pattern[pi] == '?' || pattern[pi] == text[ti] {
            pi += 1;
            ti += 1;
        } else {
            return false;
        }
    }
    ti == text.len()
}

impl Default for BuiltinFunctions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::CellCoord;
    use crate::calc::engine::CellValueInput;

    #[test]
    fn test_sum() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(2.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(3.0));
        engine.set_formula(0, CellCoord::new(3, 0), "=SUM(A1:A3)").unwrap();

        assert_eq!(engine.get_value(0, CellCoord::new(3, 0)), CellResult::Value(6.0));
    }

    #[test]
    fn test_if() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_formula(0, CellCoord::new(0, 1), "=IF(A1>5,\"big\",\"small\")").unwrap();

        assert_eq!(engine.get_value(0, CellCoord::new(0, 1)), CellResult::Text("big".to_string()));
    }

    #[test]
    fn test_average() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(20.0));
        engine.set_formula(0, CellCoord::new(2, 0), "=AVERAGE(A1:A2)").unwrap();

        assert_eq!(engine.get_value(0, CellCoord::new(2, 0)), CellResult::Value(15.0));
    }
}
