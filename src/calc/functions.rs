use crate::calc::array::{Array, computes_array, literal};
use crate::calc::engine::{CalcEngine, CellResult, leading};
use crate::cell::{CellCoord, CellError, CellRange, MAX_COL, MAX_ROW};
use crate::formula::{Expr, FunctionCall};
use std::collections::HashMap;

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
            "AVERAGE" => outcome(self.eval_average(&func.args, sheet, engine)),
            "MIN" => outcome(self.eval_extreme(&func.args, sheet, engine, false)),
            "MAX" => outcome(self.eval_extreme(&func.args, sheet, engine, true)),
            "COUNT" => self.eval_count(&func.args, sheet, engine),
            "COUNTA" => self.eval_counta(&func.args, sheet, engine),
            "ABS" => outcome(self.eval_math1(&func.args, sheet, engine, f64::abs)),
            "ROUND" => outcome(self.eval_round(&func.args, sheet, engine, Rounding::Nearest)),
            "ROUNDUP" => outcome(self.eval_round(&func.args, sheet, engine, Rounding::Up)),
            "ROUNDDOWN" | "TRUNC" => {
                outcome(self.eval_round(&func.args, sheet, engine, Rounding::Down))
            }
            "SQRT" => outcome(self.eval_math1(&func.args, sheet, engine, f64::sqrt)),
            "POWER" => outcome(self.eval_power(&func.args, sheet, engine)),
            "MOD" => outcome(self.eval_mod(&func.args, sheet, engine)),
            "INT" => outcome(self.eval_math1(&func.args, sheet, engine, f64::floor)),
            "CEILING" => outcome(self.eval_ceiling_floor(&func.args, sheet, engine, true)),
            "FLOOR" => outcome(self.eval_ceiling_floor(&func.args, sheet, engine, false)),
            "SIGN" => outcome(self.eval_math1(&func.args, sheet, engine, |n| {
                if n == 0.0 { 0.0 } else { n.signum() }
            })),
            "PI" => CellResult::Value(std::f64::consts::PI),
            "EXP" => outcome(self.eval_math1(&func.args, sheet, engine, f64::exp)),
            "LN" => outcome(self.eval_math1(&func.args, sheet, engine, f64::ln)),
            "LOG" => outcome(self.eval_log(&func.args, sheet, engine)),
            "LOG10" => outcome(self.eval_math1(&func.args, sheet, engine, f64::log10)),
            "SIN" => outcome(self.eval_math1(&func.args, sheet, engine, f64::sin)),
            "COS" => outcome(self.eval_math1(&func.args, sheet, engine, f64::cos)),
            "TAN" => outcome(self.eval_math1(&func.args, sheet, engine, f64::tan)),
            "ASIN" => outcome(self.eval_math1(&func.args, sheet, engine, f64::asin)),
            "ACOS" => outcome(self.eval_math1(&func.args, sheet, engine, f64::acos)),
            "ATAN" => outcome(self.eval_math1(&func.args, sheet, engine, f64::atan)),
            "RAND" => CellResult::Value(rand_simple()),
            "RANDBETWEEN" => outcome(self.eval_randbetween(&func.args, sheet, engine)),
            "PRODUCT" => self.eval_product(&func.args, sheet, engine),
            "SUMPRODUCT" => outcome(self.eval_sumproduct(&func.args, sheet, engine)),
            "MEDIAN" => outcome(self.eval_median(&func.args, sheet, engine)),
            "STDEV" | "STDEV.S" => self.eval_stdev(&func.args, sheet, engine, false),
            "STDEVP" | "STDEV.P" => self.eval_stdev(&func.args, sheet, engine, true),
            "VAR" | "VAR.S" => self.eval_var(&func.args, sheet, engine, false),
            "VARP" | "VAR.P" => self.eval_var(&func.args, sheet, engine, true),
            "LARGE" => outcome(self.eval_nth(&func.args, sheet, engine, true)),
            "SMALL" => outcome(self.eval_nth(&func.args, sheet, engine, false)),
            "SUMSQ" => outcome(self.eval_sumsq(&func.args, sheet, engine)),
            "ATAN2" => outcome(self.eval_atan2(&func.args, sheet, engine)),
            "SINH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::sinh)),
            "COSH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::cosh)),
            "TANH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::tanh)),
            "ASINH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::asinh)),
            "ACOSH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::acosh)),
            "ATANH" => outcome(self.eval_math1(&func.args, sheet, engine, f64::atanh)),
            "DEGREES" => outcome(self.eval_math1(&func.args, sheet, engine, f64::to_degrees)),
            "RADIANS" => outcome(self.eval_math1(&func.args, sheet, engine, f64::to_radians)),
            "SQRTPI" => outcome(self.eval_math1(&func.args, sheet, engine, |n| {
                (n * std::f64::consts::PI).sqrt()
            })),
            "FACT" => outcome(self.eval_fact(&func.args, sheet, engine, 1.0)),
            "FACTDOUBLE" => outcome(self.eval_fact(&func.args, sheet, engine, 2.0)),
            "COMBIN" => outcome(self.eval_combin(&func.args, sheet, engine, false)),
            "PERMUT" => outcome(self.eval_combin(&func.args, sheet, engine, true)),
            "GCD" => outcome(self.eval_gcd_lcm(&func.args, sheet, engine, false)),
            "LCM" => outcome(self.eval_gcd_lcm(&func.args, sheet, engine, true)),
            "QUOTIENT" => outcome(self.eval_quotient(&func.args, sheet, engine)),
            "MROUND" => outcome(self.eval_mround(&func.args, sheet, engine)),
            "EVEN" => outcome(self.eval_even_odd(&func.args, sheet, engine, false)),
            "ODD" => outcome(self.eval_even_odd(&func.args, sheet, engine, true)),
            "ROMAN" => outcome(self.eval_roman(&func.args, sheet, engine)),
            "ARABIC" => outcome(self.eval_arabic(&func.args, sheet, engine)),
            "BASE" => outcome(self.eval_base(&func.args, sheet, engine)),
            "DECIMAL" => outcome(self.eval_decimal(&func.args, sheet, engine)),
            "CEILING.MATH" => outcome(self.eval_round_math(&func.args, sheet, engine, true)),
            "FLOOR.MATH" => outcome(self.eval_round_math(&func.args, sheet, engine, false)),

            // ===== Statistical functions =====
            "MODE" | "MODE.SNGL" => outcome(self.eval_mode(&func.args, sheet, engine)),
            "PERCENTILE" | "PERCENTILE.INC" => {
                outcome(self.eval_percentile(&func.args, sheet, engine, false, false))
            }
            "PERCENTILE.EXC" => {
                outcome(self.eval_percentile(&func.args, sheet, engine, true, false))
            }
            "QUARTILE" | "QUARTILE.INC" => {
                outcome(self.eval_percentile(&func.args, sheet, engine, false, true))
            }
            "QUARTILE.EXC" => outcome(self.eval_percentile(&func.args, sheet, engine, true, true)),
            "RANK" | "RANK.EQ" => outcome(self.eval_rank(&func.args, sheet, engine, false)),
            "RANK.AVG" => outcome(self.eval_rank(&func.args, sheet, engine, true)),
            "GEOMEAN" => outcome(self.eval_mean_of(&func.args, sheet, engine, Mean::Geometric)),
            "HARMEAN" => outcome(self.eval_mean_of(&func.args, sheet, engine, Mean::Harmonic)),
            "AVEDEV" => outcome(self.eval_mean_of(&func.args, sheet, engine, Mean::AbsDeviation)),
            "DEVSQ" => {
                outcome(self.eval_mean_of(&func.args, sheet, engine, Mean::SquaredDeviation))
            }
            "TRIMMEAN" => outcome(self.eval_trimmean(&func.args, sheet, engine)),
            "STANDARDIZE" => outcome(self.eval_standardize(&func.args, sheet, engine)),
            "CORREL" | "PEARSON" => {
                outcome(self.eval_paired(&func.args, sheet, engine, Paired::Correl))
            }
            "RSQ" => outcome(self.eval_paired(&func.args, sheet, engine, Paired::Rsq)),
            "COVARIANCE.P" | "COVAR" => {
                outcome(self.eval_paired(&func.args, sheet, engine, Paired::CovarianceP))
            }
            "COVARIANCE.S" => {
                outcome(self.eval_paired(&func.args, sheet, engine, Paired::CovarianceS))
            }
            "SLOPE" => outcome(self.eval_paired(&func.args, sheet, engine, Paired::Slope)),
            "INTERCEPT" => outcome(self.eval_paired(&func.args, sheet, engine, Paired::Intercept)),
            "STEYX" => outcome(self.eval_paired(&func.args, sheet, engine, Paired::Steyx)),
            "FORECAST" | "FORECAST.LINEAR" => {
                outcome(self.eval_forecast(&func.args, sheet, engine))
            }
            "MAXIFS" => self.eval_extreme_ifs(&func.args, sheet, engine, true),
            "MINIFS" => self.eval_extreme_ifs(&func.args, sheet, engine, false),
            "AVERAGEA" => {
                outcome(self.eval_a_variant(&func.args, sheet, engine, AVariant::Average))
            }
            "MAXA" => outcome(self.eval_a_variant(&func.args, sheet, engine, AVariant::Max)),
            "MINA" => outcome(self.eval_a_variant(&func.args, sheet, engine, AVariant::Min)),

            // ===== Financial functions =====
            "PMT" => outcome(self.eval_annuity(&func.args, sheet, engine, Annuity::Pmt)),
            "FV" => outcome(self.eval_annuity(&func.args, sheet, engine, Annuity::Fv)),
            "PV" => outcome(self.eval_annuity(&func.args, sheet, engine, Annuity::Pv)),
            "NPER" => outcome(self.eval_annuity(&func.args, sheet, engine, Annuity::Nper)),
            "IPMT" => outcome(self.eval_ipmt(&func.args, sheet, engine, false)),
            "PPMT" => outcome(self.eval_ipmt(&func.args, sheet, engine, true)),
            "CUMIPMT" => outcome(self.eval_cumulative(&func.args, sheet, engine, false)),
            "CUMPRINC" => outcome(self.eval_cumulative(&func.args, sheet, engine, true)),
            "RATE" => outcome(self.eval_rate(&func.args, sheet, engine)),
            "NPV" => outcome(self.eval_npv(&func.args, sheet, engine)),
            "IRR" => outcome(self.eval_irr(&func.args, sheet, engine)),
            "MIRR" => outcome(self.eval_mirr(&func.args, sheet, engine)),
            "XNPV" => outcome(self.eval_xnpv(&func.args, sheet, engine)),
            "XIRR" => outcome(self.eval_xirr(&func.args, sheet, engine)),
            "SLN" => outcome(self.eval_sln(&func.args, sheet, engine)),
            "SYD" => outcome(self.eval_syd(&func.args, sheet, engine)),
            "DB" => outcome(self.eval_db(&func.args, sheet, engine)),
            "DDB" => outcome(self.eval_ddb(&func.args, sheet, engine)),

            // ===== Logical functions =====
            "IF" => outcome(self.eval_if(&func.args, sheet, engine)),
            "AND" => outcome(self.eval_and_or(&func.args, sheet, engine, Logic::And)),
            "OR" => outcome(self.eval_and_or(&func.args, sheet, engine, Logic::Or)),
            "NOT" => outcome(self.eval_not(&func.args, sheet, engine)),
            "XOR" => outcome(self.eval_and_or(&func.args, sheet, engine, Logic::Xor)),
            "TRUE" => CellResult::Bool(true),
            "FALSE" => CellResult::Bool(false),
            "IFERROR" => self.eval_iferror(&func.args, sheet, engine),
            "IFNA" => self.eval_ifna(&func.args, sheet, engine),
            "IFS" => outcome(self.eval_ifs(&func.args, sheet, engine)),
            "SWITCH" => outcome(self.eval_switch(&func.args, sheet, engine)),
            "CHOOSE" => outcome(self.eval_choose(&func.args, sheet, engine)),

            // ===== Text functions =====
            "LEN" => outcome(self.eval_text1(&func.args, sheet, engine, |s| {
                CellResult::Value(s.chars().count() as f64)
            })),
            "UPPER" => outcome(self.eval_text1(&func.args, sheet, engine, |s| {
                CellResult::Text(s.to_uppercase())
            })),
            "LOWER" => outcome(self.eval_text1(&func.args, sheet, engine, |s| {
                CellResult::Text(s.to_lowercase())
            })),
            // Only spaces go, and runs of them inside become one.
            "TRIM" => outcome(self.eval_text1(&func.args, sheet, engine, |s| {
                CellResult::Text(
                    s.split(' ')
                        .filter(|w| !w.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })),
            "CONCATENATE" => outcome(self.eval_concatenate(&func.args, sheet, engine, false)),
            "CONCAT" => outcome(self.eval_concatenate(&func.args, sheet, engine, true)),
            "LEFT" => outcome(self.eval_left_right(&func.args, sheet, engine, false)),
            "RIGHT" => outcome(self.eval_left_right(&func.args, sheet, engine, true)),
            "MID" => outcome(self.eval_mid(&func.args, sheet, engine)),
            "FIND" => outcome(self.eval_find(&func.args, sheet, engine, true)),
            "SEARCH" => outcome(self.eval_find(&func.args, sheet, engine, false)),
            "SUBSTITUTE" => outcome(self.eval_substitute(&func.args, sheet, engine)),
            "REPLACE" => outcome(self.eval_replace(&func.args, sheet, engine)),
            "REPT" => outcome(self.eval_rept(&func.args, sheet, engine)),
            "EXACT" => outcome(self.eval_exact(&func.args, sheet, engine)),
            "VALUE" => outcome(self.eval_value(&func.args, sheet, engine)),
            "TEXT" => outcome(self.eval_text(&func.args, sheet, engine)),
            "CHAR" => outcome(self.eval_char(&func.args, sheet, engine)),
            "CODE" => outcome(self.eval_text1(&func.args, sheet, engine, |s| {
                s.chars()
                    .next()
                    .map_or(CellResult::Error(CellError::Value), |c| {
                        CellResult::Value(c as u32 as f64)
                    })
            })),
            "PROPER" => outcome(self.eval_text1(&func.args, sheet, engine, proper)),
            "TEXTJOIN" => outcome(self.eval_textjoin(&func.args, sheet, engine)),
            "CLEAN" => outcome(self.eval_clean(&func.args, sheet, engine)),
            "T" => outcome(self.eval_t(&func.args, sheet, engine)),
            "DOLLAR" => outcome(self.eval_fixed(&func.args, sheet, engine, true)),
            "FIXED" => outcome(self.eval_fixed(&func.args, sheet, engine, false)),
            "UNICHAR" => outcome(self.eval_unichar(&func.args, sheet, engine)),
            "UNICODE" => outcome(self.eval_unicode(&func.args, sheet, engine)),
            "NUMBERVALUE" => outcome(self.eval_numbervalue(&func.args, sheet, engine)),
            "TEXTBEFORE" => outcome(self.eval_text_split(&func.args, sheet, engine, true)),
            "TEXTAFTER" => outcome(self.eval_text_split(&func.args, sheet, engine, false)),

            // ===== Lookup functions =====
            "VLOOKUP" => outcome(self.eval_table_lookup(&func.args, sheet, engine, true)),
            "HLOOKUP" => outcome(self.eval_table_lookup(&func.args, sheet, engine, false)),
            "INDEX" => outcome(self.eval_index(&func.args, sheet, engine)),
            "MATCH" => outcome(self.eval_match(&func.args, sheet, engine)),
            "ROW" => outcome(self.eval_row_column(&func.args, sheet, engine, true)),
            "COLUMN" => outcome(self.eval_row_column(&func.args, sheet, engine, false)),
            "ROWS" => outcome(self.eval_extent(&func.args, sheet, engine, true)),
            "COLUMNS" => outcome(self.eval_extent(&func.args, sheet, engine, false)),
            "LOOKUP" => outcome(self.eval_lookup(&func.args, sheet, engine)),
            "ADDRESS" => outcome(self.eval_address(&func.args, sheet, engine)),
            "INDIRECT" => deref(self.indirect_ref(&func.args, sheet, engine), engine),
            "OFFSET" => deref(self.offset_ref(&func.args, sheet, engine), engine),
            "FORMULATEXT" => outcome(self.eval_formulatext(&func.args, sheet, engine)),
            "HYPERLINK" => outcome(self.eval_hyperlink(&func.args, sheet, engine)),
            "SHEET" => outcome(self.eval_sheet(&func.args, sheet, engine)),
            "SHEETS" => outcome(self.eval_sheets(&func.args, sheet, engine)),

            // ===== Conditional aggregation =====
            "SUMIF" => self.eval_sumif(&func.args, sheet, engine),
            "COUNTIF" => self.eval_countif(&func.args, sheet, engine),
            "AVERAGEIF" => self.eval_averageif(&func.args, sheet, engine),
            "SUMIFS" => self.eval_sumifs(&func.args, sheet, engine),
            "COUNTIFS" => self.eval_countifs(&func.args, sheet, engine),
            "AVERAGEIFS" => self.eval_averageifs(&func.args, sheet, engine),
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
            "ISEVEN" => outcome(self.eval_parity(&func.args, sheet, engine, false)),
            "ISODD" => outcome(self.eval_parity(&func.args, sheet, engine, true)),
            "ISERR" => outcome(self.eval_iserr(&func.args, sheet, engine)),
            "ERROR.TYPE" => outcome(self.eval_error_type(&func.args, sheet, engine)),
            "ISFORMULA" => outcome(self.eval_isformula(&func.args, sheet, engine)),

            // ===== Date/Time functions =====
            "DATE" => outcome(self.eval_date(&func.args, sheet, engine)),
            "YEAR" => outcome(self.eval_date_part(&func.args, sheet, engine, |(y, _, _)| y)),
            "MONTH" => outcome(self.eval_date_part(&func.args, sheet, engine, |(_, m, _)| m)),
            "DAY" => outcome(self.eval_date_part(&func.args, sheet, engine, |(_, _, d)| d)),
            "TODAY" => CellResult::Value(now_serial(engine).floor()),
            "NOW" => CellResult::Value(now_serial(engine)),
            "TIME" => outcome(self.eval_time(&func.args, sheet, engine)),
            "HOUR" => outcome(self.eval_time_part(&func.args, sheet, engine, 3600, 24)),
            "MINUTE" => outcome(self.eval_time_part(&func.args, sheet, engine, 60, 60)),
            "SECOND" => outcome(self.eval_time_part(&func.args, sheet, engine, 1, 60)),
            "WEEKDAY" => outcome(self.eval_weekday(&func.args, sheet, engine)),
            "WEEKNUM" => outcome(self.eval_weeknum(&func.args, sheet, engine)),
            "EDATE" => outcome(self.eval_edate(&func.args, sheet, engine, false)),
            "EOMONTH" => outcome(self.eval_edate(&func.args, sheet, engine, true)),
            "DATEDIF" => outcome(self.eval_datedif(&func.args, sheet, engine)),
            "DATEVALUE" => outcome(self.eval_datevalue(&func.args, sheet, engine, true)),
            "TIMEVALUE" => outcome(self.eval_datevalue(&func.args, sheet, engine, false)),
            "DAYS" => outcome(self.eval_days(&func.args, sheet, engine)),
            "DAYS360" => outcome(self.eval_days360(&func.args, sheet, engine)),
            "NETWORKDAYS" => outcome(self.eval_networkdays(&func.args, sheet, engine)),
            "WORKDAY" => outcome(self.eval_workday(&func.args, sheet, engine)),
            "YEARFRAC" => outcome(self.eval_yearfrac(&func.args, sheet, engine)),

            _ => CellResult::Error(CellError::Name),
        }
    }

    // ========== Math Functions ==========

    fn eval_sum(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values = self.collect_numeric_values(args, sheet, engine);
        if let Some(e) = first_error(&values) {
            return CellResult::Error(e);
        }
        let sum: f64 = values.into_iter().filter_map(|v| v.ok()).sum();
        CellResult::Value(sum)
    }

    fn eval_average(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        let values = self.numbers(args, sheet, engine)?;
        if values.is_empty() {
            return Err(CellError::DivZero);
        }
        finite(values.iter().sum::<f64>() / values.len() as f64)
    }

    /// MAX, or MIN; 0 when there are no numbers.
    fn eval_extreme(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, max: bool) -> Outcome {
        let values = self.numbers(args, sheet, engine)?;
        let pick = if max { f64::max } else { f64::min };
        Ok(CellResult::Value(
            values.into_iter().reduce(pick).unwrap_or(0.0),
        ))
    }

    fn eval_count(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let count = self
            .collect_numeric_values(args, sheet, engine)
            .into_iter()
            .filter(|v| v.is_ok())
            .count();
        CellResult::Value(count as f64)
    }

    fn eval_counta(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let count = self
            .collect_all_values(args, sheet, engine)
            .into_iter()
            .filter(|v| !matches!(v, CellResult::Empty))
            .count();
        CellResult::Value(count as f64)
    }

    /// ROUND, ROUNDUP, ROUNDDOWN and TRUNC to `num_digits` decimals, or to
    /// tens, hundreds... when negative.
    fn eval_round(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        mode: Rounding,
    ) -> Outcome {
        arity(args, 1, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let digits = self.opt_number(args, 1, 0.0, sheet, engine)?.trunc();
        finite(round_to(n, digits.clamp(-400.0, 400.0) as i32, mode))
    }

    /// POWER: 0 to a negative power is #DIV/0!, and 0^0 and roots of
    /// negative numbers are #NUM!.
    fn eval_power(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let base = self.number(&args[0], sheet, engine)?;
        let exponent = self.number(&args[1], sheet, engine)?;
        match (base, exponent) {
            (0.0, e) if e < 0.0 => Err(CellError::DivZero),
            (0.0, 0.0) => Err(CellError::Num),
            _ => finite(base.powf(exponent)),
        }
    }

    /// MOD: the remainder takes the divisor's sign.
    fn eval_mod(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let d = self.number(&args[1], sheet, engine)?;
        if d == 0.0 {
            return Err(CellError::DivZero);
        }
        finite(n - d * (n / d).floor())
    }

    /// CEILING (`up`) and FLOOR to a multiple of the significance, as Excel
    /// 2010 on has them: CEILING takes the next multiple toward +infinity
    /// (away from zero when both are negative), FLOOR the next toward
    /// -infinity (toward zero when both are negative). A positive number
    /// with a negative significance is #NUM!. CEILING by 0 is 0, FLOOR by
    /// 0 #DIV/0!.
    fn eval_ceiling_floor(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        up: bool,
    ) -> Outcome {
        arity(args, 1, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let significance = self.opt_number(args, 1, 1.0, sheet, engine)?;
        if significance == 0.0 {
            return if up {
                Ok(CellResult::Value(0.0))
            } else {
                Err(CellError::DivZero)
            };
        }
        if n == 0.0 {
            return Ok(CellResult::Value(0.0));
        }
        if n > 0.0 && significance < 0.0 {
            return Err(CellError::Num);
        }
        // At 15 digits, so FLOOR(0.3, 0.1) is 0.3 although 0.3/0.1 is
        // 2.9999... A negative significance flips the steps' sign, which
        // turns ceil and floor around in the number's terms.
        let steps = snap(n / significance);
        let steps = if up { steps.ceil() } else { steps.floor() };
        // + 0.0: CEILING(-0.5, 1) is 0, not -0.
        finite(snap(steps * significance) + 0.0)
    }

    fn eval_log(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let base = self.opt_number(args, 1, 10.0, sheet, engine)?;
        if n <= 0.0 || base <= 0.0 {
            return Err(CellError::Num);
        }
        if base == 1.0 {
            return Err(CellError::DivZero);
        }
        finite(n.log(base))
    }

    /// RANDBETWEEN: a whole number from `bottom` up to `top`, both rounded
    /// inward to whole numbers.
    fn eval_randbetween(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let bottom = self.number(&args[0], sheet, engine)?.ceil();
        let top = self.number(&args[1], sheet, engine)?.floor();
        if bottom > top {
            return Err(CellError::Num);
        }
        finite((bottom + rand_simple() * (top - bottom + 1.0)).floor())
    }

    fn eval_product(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        let values = self.collect_numeric_values(args, sheet, engine);
        if let Some(e) = first_error(&values) {
            return CellResult::Error(e);
        }
        let nums: Vec<f64> = values.into_iter().filter_map(|v| v.ok()).collect();
        if nums.is_empty() {
            return CellResult::Value(0.0);
        }
        CellResult::Value(nums.into_iter().product())
    }

    fn eval_median(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        let mut values = self.numbers(args, sheet, engine)?;
        if values.is_empty() {
            return Err(CellError::Num);
        }
        values.sort_by(f64::total_cmp);
        let mid = values.len() / 2;
        Ok(CellResult::Value(if values.len().is_multiple_of(2) {
            (values[mid - 1] + values[mid]) / 2.0
        } else {
            values[mid]
        }))
    }

    /// SUMPRODUCT: the sum of the item-by-item products of equally sized
    /// ranges or arrays. Anything but a number counts as 0, logicals too,
    /// as in Excel.
    fn eval_sumproduct(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.is_empty() {
            return Err(CellError::Value);
        }
        let blocks = args
            .iter()
            .map(|arg| self.block(arg, sheet, engine))
            .collect::<Result<Vec<_>, _>>()?;
        let shape = |b: &Block| (b.height(), b.width());
        if blocks.iter().any(|b| shape(b) != shape(&blocks[0])) {
            return Err(CellError::Value);
        }
        // Every product past the used window is 0.
        let (rows, cols) = blocks
            .iter()
            .map(|b| b.used(engine))
            .fold((0, 0), |(r, c), (br, bc)| (r.max(br), c.max(bc)));
        let mut sum = 0.0;
        for row in 0..rows {
            for col in 0..cols {
                let mut product = 1.0;
                for block in &blocks {
                    match block.get(engine, row, col) {
                        CellResult::Error(e) => return Err(e),
                        CellResult::Value(n) => product *= n,
                        _ => product = 0.0,
                    }
                }
                sum += product;
            }
        }
        finite(sum)
    }

    fn eval_stdev(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        population: bool,
    ) -> CellResult {
        match self.variance(args, sheet, engine, population) {
            Ok(v) => CellResult::Value(v.sqrt()),
            Err(e) => e,
        }
    }

    fn eval_var(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        population: bool,
    ) -> CellResult {
        match self.variance(args, sheet, engine, population) {
            Ok(v) => CellResult::Value(v),
            Err(e) => e,
        }
    }

    fn variance(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        population: bool,
    ) -> Result<f64, CellResult> {
        let collected = self.collect_numeric_values(args, sheet, engine);
        if let Some(e) = first_error(&collected) {
            return Err(CellResult::Error(e));
        }
        let values: Vec<f64> = collected.into_iter().filter_map(|v| v.ok()).collect();
        let n = values.len();
        let denom = if population { n } else { n.saturating_sub(1) };
        if denom == 0 {
            return Err(CellResult::Error(CellError::DivZero));
        }
        let mean = values.iter().sum::<f64>() / n as f64;
        let sumsq = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        Ok(sumsq / denom as f64)
    }

    /// LARGE (`large`) or SMALL: the k-th largest or smallest number of the
    /// first argument, which must be a reference or an array.
    fn eval_nth(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, large: bool) -> Outcome {
        arity(args, 2, 2)?;
        let block = self.block(&args[0], sheet, engine)?;
        let mut values = block
            .used_values(engine)
            .filter_map(|v| numeric_for_aggregate(&v, true))
            .collect::<Result<Vec<f64>, _>>()?;
        let k = self.number(&args[1], sheet, engine)?.floor();
        if k < 1.0 || k > values.len() as f64 {
            return Err(CellError::Num);
        }
        let k = k as usize;
        values.sort_by(f64::total_cmp);
        Ok(CellResult::Value(if large {
            values[values.len() - k]
        } else {
            values[k - 1]
        }))
    }

    fn eval_sumsq(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        let values = self.numbers(args, sheet, engine)?;
        finite(values.iter().map(|x| x * x).sum())
    }

    /// One number in, one out; results outside the function's domain are
    /// NaN or infinite, which is #NUM!.
    fn eval_math1(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        f: fn(f64) -> f64,
    ) -> Outcome {
        arity(args, 1, 1)?;
        finite(f(self.number(&args[0], sheet, engine)?))
    }

    /// Excel's ATAN2 takes x first: ATAN2(x, y) is atan2(y, x).
    fn eval_atan2(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let x = self.number(&args[0], sheet, engine)?;
        let y = self.number(&args[1], sheet, engine)?;
        if x == 0.0 && y == 0.0 {
            return Err(CellError::DivZero);
        }
        finite(y.atan2(x))
    }

    /// FACT (`step` 1) and FACTDOUBLE (`step` 2): n * (n - step) * ... >= 1.
    fn eval_fact(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, step: f64) -> Outcome {
        arity(args, 1, 1)?;
        let n = self.number(&args[0], sheet, engine)?;
        if n < 0.0 {
            return Err(CellError::Num);
        }
        let mut product = 1.0_f64;
        let mut k = n.trunc();
        while k > 1.0 && product.is_finite() {
            product *= k;
            k -= step;
        }
        finite(product)
    }

    /// COMBIN, or PERMUT when `ordered`.
    fn eval_combin(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        ordered: bool,
    ) -> Outcome {
        arity(args, 2, 2)?;
        let n = self.number(&args[0], sheet, engine)?.trunc();
        let k = self.number(&args[1], sheet, engine)?.trunc();
        if n < 0.0 || k < 0.0 || n < k || (ordered && n == 0.0) {
            return Err(CellError::Num);
        }
        let mut result = 1.0_f64;
        if ordered {
            let mut i = 0.0;
            while i < k && result.is_finite() {
                result *= n - i;
                i += 1.0;
            }
        } else {
            let k = k.min(n - k);
            let mut i = 1.0;
            while i <= k && result.is_finite() {
                result = result * (n - k + i) / i;
                i += 1.0;
            }
            result = result.round();
        }
        finite(result)
    }

    /// GCD, or LCM when `lcm`, of the whole parts of non-negative numbers.
    fn eval_gcd_lcm(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, lcm: bool) -> Outcome {
        const LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53
        let values = self.numbers(args, sheet, engine)?;
        if values.is_empty() {
            return Err(CellError::Value);
        }
        if values.iter().any(|v| *v < 0.0 || *v >= LIMIT) {
            return Err(CellError::Num);
        }
        let gcd = |mut a: u64, mut b: u64| {
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a
        };
        let mut acc = if lcm { 1u64 } else { 0u64 };
        for v in values.into_iter().map(|v| v.trunc() as u64) {
            acc = if !lcm {
                gcd(acc, v)
            } else if v == 0 || acc == 0 {
                0
            } else {
                match (acc / gcd(acc, v)).checked_mul(v) {
                    Some(m) if (m as f64) < LIMIT => m,
                    _ => return Err(CellError::Num),
                }
            };
        }
        Ok(CellResult::Value(acc as f64))
    }

    fn eval_quotient(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let d = self.number(&args[1], sheet, engine)?;
        if d == 0.0 {
            return Err(CellError::DivZero);
        }
        finite((n / d).trunc())
    }

    /// Nearest multiple, halves away from zero; #NUM! when the signs differ.
    fn eval_mround(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let n = self.number(&args[0], sheet, engine)?;
        let multiple = self.number(&args[1], sheet, engine)?;
        if n == 0.0 || multiple == 0.0 {
            return Ok(CellResult::Value(0.0));
        }
        if (n < 0.0) != (multiple < 0.0) {
            return Err(CellError::Num);
        }
        finite(snap(snap(n / multiple).round() * multiple))
    }

    /// EVEN or ODD: away from zero to the next even or odd integer.
    fn eval_even_odd(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, odd: bool) -> Outcome {
        arity(args, 1, 1)?;
        let n = self.number(&args[0], sheet, engine)?;
        let mut whole = snap(n.abs()).ceil();
        if (whole % 2.0 == 1.0) != odd {
            whole += 1.0;
        }
        finite(if n < 0.0 { -whole } else { whole })
    }

    fn eval_roman(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let n = self.number(&args[0], sheet, engine)?.trunc();
        // TRUE is the classic form 0, FALSE the most concise, 4.
        let form = match args.get(1).map(|a| self.eval_arg(a, sheet, engine)) {
            None => 0.0,
            Some(CellResult::Bool(b)) => {
                if b {
                    0.0
                } else {
                    4.0
                }
            }
            Some(v) => to_number(&v)?.trunc(),
        };
        if !(0.0..4000.0).contains(&n) || !(0.0..=4.0).contains(&form) {
            return Err(CellError::Value);
        }
        Ok(CellResult::Text(roman(n as u32, form as u32)))
    }

    fn eval_arabic(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        if digits.chars().count() > 255 {
            return Err(CellError::Value);
        }
        // Each numeral adds, or subtracts when a larger one follows it.
        let mut total = 0i64;
        let mut next = 0i64;
        for c in digits.chars().rev() {
            let v = match c.to_ascii_uppercase() {
                'I' => 1,
                'V' => 5,
                'X' => 10,
                'L' => 50,
                'C' => 100,
                'D' => 500,
                'M' => 1000,
                _ => return Err(CellError::Value),
            };
            total += if v < next { -v } else { v };
            next = v;
        }
        let total = if negative { -total } else { total };
        Ok(CellResult::Value(total as f64))
    }

    fn eval_base(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let n = self.number(&args[0], sheet, engine)?.trunc();
        let radix = self.number(&args[1], sheet, engine)?.trunc();
        let min_len = self.number_or(args, 2, 0.0, sheet, engine)?.trunc();
        if !(0.0..9_007_199_254_740_992.0).contains(&n)
            || !(2.0..=36.0).contains(&radix)
            || !(0.0..=255.0).contains(&min_len)
        {
            return Err(CellError::Num);
        }
        let (mut n, radix) = (n as u64, radix as u64);
        let mut digits = Vec::new();
        loop {
            let d = (n % radix) as u32;
            digits.push(
                std::char::from_digit(d, 36)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
            n /= radix;
            if n == 0 {
                break;
            }
        }
        while digits.len() < min_len as usize {
            digits.push('0');
        }
        Ok(CellResult::Text(digits.iter().rev().collect()))
    }

    fn eval_decimal(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let radix = self.number(&args[1], sheet, engine)?.trunc();
        if !(2.0..=36.0).contains(&radix) || text.chars().count() > 255 {
            return Err(CellError::Num);
        }
        let mut total = 0.0;
        for c in text.trim().chars() {
            match c.to_digit(radix as u32) {
                Some(d) => total = total * radix + d as f64,
                None => return Err(CellError::Num),
            }
        }
        if total >= 9_007_199_254_740_992.0 {
            return Err(CellError::Num);
        }
        Ok(CellResult::Value(total))
    }

    /// CEILING.MATH (`up`) and FLOOR.MATH. The significance's sign is
    /// ignored; a nonzero mode rounds negative numbers the other way: away
    /// from zero for CEILING.MATH, toward it for FLOOR.MATH.
    fn eval_round_math(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, up: bool) -> Outcome {
        arity(args, 1, 3)?;
        let n = self.number(&args[0], sheet, engine)?;
        let significance = self.number_or(args, 1, 1.0, sheet, engine)?.abs();
        let mode = self.number_or(args, 2, 0.0, sheet, engine)?;
        if significance == 0.0 {
            return Ok(CellResult::Value(0.0));
        }
        let steps = snap(n.abs() / significance);
        // Positive numbers go up for CEILING.MATH; negative ones mirror that
        // unless mode flips them.
        let away_from_zero = if n >= 0.0 { up } else { (mode != 0.0) == up };
        let steps = if away_from_zero {
            steps.ceil()
        } else {
            steps.floor()
        };
        let magnitude = snap(steps * significance);
        finite(if n < 0.0 { -magnitude } else { magnitude })
    }

    // ========== Statistical Functions ==========

    /// MODE: the most frequent number, the first one met on a tie; #N/A
    /// when no number repeats.
    fn eval_mode(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        let values = self.numbers(args, sheet, engine)?;
        let mut counts: HashMap<u64, (usize, usize)> = HashMap::new();
        for (i, v) in values.iter().enumerate() {
            // + 0.0 makes -0 and 0 one key.
            counts.entry((v + 0.0).to_bits()).or_insert((0, i)).0 += 1;
        }
        let best = counts
            .values()
            .filter(|(count, _)| *count > 1)
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        match best {
            Some(&(_, first)) => Ok(CellResult::Value(values[first])),
            None => Err(CellError::NA),
        }
    }

    /// PERCENTILE and QUARTILE (`quartile`), inclusive or `exclusive`.
    fn eval_percentile(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        exclusive: bool,
        quartile: bool,
    ) -> Outcome {
        arity(args, 2, 2)?;
        let mut values = self.numbers(&args[..1], sheet, engine)?;
        let k = self.number(&args[1], sheet, engine)?;
        let k = if quartile {
            let q = k.trunc();
            let valid = if exclusive { 1.0..=3.0 } else { 0.0..=4.0 };
            if !valid.contains(&q) {
                return Err(CellError::Num);
            }
            q / 4.0
        } else {
            k
        };
        values.sort_by(f64::total_cmp);
        let result = if exclusive {
            percentile_exc(&values, k)
        } else {
            percentile_inc(&values, k)
        };
        result.map(CellResult::Value).ok_or(CellError::Num)
    }

    /// RANK.EQ, or RANK.AVG (`average`) giving ties their mean rank.
    fn eval_rank(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, average: bool) -> Outcome {
        arity(args, 2, 3)?;
        let n = self.number(&args[0], sheet, engine)?;
        if self.reference(&args[1], sheet, engine).is_none() {
            return Err(CellError::Value);
        }
        let values = self.numbers(&args[1..2], sheet, engine)?;
        let ascending = self.number_or(args, 2, 0.0, sheet, engine)? != 0.0;
        let ties = values.iter().filter(|v| **v == n).count();
        if ties == 0 {
            return Err(CellError::NA);
        }
        let ahead = values
            .iter()
            .filter(|v| if ascending { **v < n } else { **v > n })
            .count();
        let rank = ahead as f64 + 1.0;
        Ok(CellResult::Value(if average {
            rank + (ties - 1) as f64 / 2.0
        } else {
            rank
        }))
    }

    fn eval_mean_of(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, kind: Mean) -> Outcome {
        let values = self.numbers(args, sheet, engine)?;
        if values.is_empty() {
            return Err(CellError::Num);
        }
        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let result = match kind {
            Mean::Geometric | Mean::Harmonic if values.iter().any(|v| *v <= 0.0) => {
                return Err(CellError::Num);
            }
            Mean::Geometric => (values.iter().map(|v| v.ln()).sum::<f64>() / n).exp(),
            Mean::Harmonic => n / values.iter().map(|v| 1.0 / v).sum::<f64>(),
            Mean::AbsDeviation => values.iter().map(|v| (v - mean).abs()).sum::<f64>() / n,
            Mean::SquaredDeviation => values.iter().map(|v| (v - mean).powi(2)).sum(),
        };
        finite(result)
    }

    /// Mean after dropping `percent` of the points, half from each end,
    /// rounded down to an even count.
    fn eval_trimmean(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let mut values = self.numbers(&args[..1], sheet, engine)?;
        let percent = self.number(&args[1], sheet, engine)?;
        if !(0.0..1.0).contains(&percent) || values.is_empty() {
            return Err(CellError::Num);
        }
        values.sort_by(f64::total_cmp);
        let cut = (snap(values.len() as f64 * percent) / 2.0).floor() as usize;
        let kept = &values[cut..values.len() - cut];
        finite(kept.iter().sum::<f64>() / kept.len() as f64)
    }

    fn eval_standardize(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let [x, mean, sd] = self.numbers_from(args, 0, sheet, engine)?[..] else {
            return Err(CellError::Value);
        };
        if sd <= 0.0 {
            return Err(CellError::Num);
        }
        finite((x - mean) / sd)
    }

    /// The (y, x) points of two equally sized arguments where both are
    /// numbers; other pairs are skipped and errors returned, as in Excel.
    fn points(
        &self,
        ys: &Expr,
        xs: &Expr,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<(f64, f64)>, CellError> {
        let mut points = Vec::new();
        for pair in self.paired_cells(ys, xs, sheet, engine)? {
            match pair {
                (CellResult::Error(e), _) | (_, CellResult::Error(e)) => return Err(e),
                (CellResult::Value(y), CellResult::Value(x)) => points.push((y, x)),
                _ => {}
            }
        }
        Ok(points)
    }

    fn eval_paired(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, kind: Paired) -> Outcome {
        arity(args, 2, 2)?;
        let points = self.points(&args[0], &args[1], sheet, engine)?;
        finite(regression(&points, kind)?)
    }

    /// FORECAST(x, known_y's, known_x's): the regression line at x.
    fn eval_forecast(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let x = self.number(&args[0], sheet, engine)?;
        let points = self.points(&args[1], &args[2], sheet, engine)?;
        let slope = regression(&points, Paired::Slope)?;
        let intercept = regression(&points, Paired::Intercept)?;
        finite(intercept + slope * x)
    }

    /// MAXIFS (`max`) and MINIFS: the extreme of the numbers whose row meets
    /// every criterion, or 0 when none does.
    fn eval_extreme_ifs(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        max: bool,
    ) -> CellResult {
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return CellResult::Error(CellError::Value);
        }
        let (target, target_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let criteria = match self.bind_criteria(&args[1..], sheet, engine, &target) {
            Ok(c) => c,
            Err(e) => return e,
        };
        let Some((window, mask)) =
            self.criteria_mask(&criteria, Some((target_sheet, target)), engine)
        else {
            return CellResult::Value(0.0);
        };
        let mut best: Option<f64> = None;
        for (i, coord) in leading(&target, window).iter().enumerate() {
            if !mask[i] {
                continue;
            }
            match engine.get_value(target_sheet, coord) {
                CellResult::Value(n) => {
                    best = Some(match best {
                        Some(b) if (b >= n) == max => b,
                        _ => n,
                    })
                }
                CellResult::Error(e) => return CellResult::Error(e),
                _ => {}
            }
        }
        CellResult::Value(best.unwrap_or(0.0))
    }

    /// AVERAGEA, MAXA and MINA: text in references and arrays counts as 0
    /// and logicals as 1 or 0; blanks are skipped, while an empty slot is 0.
    fn eval_a_variant(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        kind: AVariant,
    ) -> Outcome {
        let mut values = Vec::new();
        let mut read = |value: CellResult| -> Result<(), CellError> {
            match value {
                CellResult::Value(n) => values.push(n),
                CellResult::Bool(b) => values.push(if b { 1.0 } else { 0.0 }),
                CellResult::Text(_) => values.push(0.0),
                CellResult::Empty => {}
                CellResult::Error(e) => return Err(e),
            }
            Ok(())
        };
        for arg in args {
            match self.source(arg, sheet, engine) {
                Some(block) => block?.used_values(engine).try_for_each(&mut read)?,
                None => match self.branch(arg, sheet, engine) {
                    CellResult::Empty => {}
                    val => read(CellResult::Value(to_number(&val)?))?,
                },
            }
        }
        let result = match kind {
            AVariant::Average if values.is_empty() => return Err(CellError::DivZero),
            AVariant::Average => values.iter().sum::<f64>() / values.len() as f64,
            _ if values.is_empty() => 0.0,
            AVariant::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            AVariant::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        };
        finite(result)
    }

    // ========== Financial Functions ==========

    /// PMT, FV, PV and NPER, from the annuity equation
    /// pv*(1+r)^n + pmt*(1+r*type)*((1+r)^n - 1)/r + fv = 0.
    fn eval_annuity(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        kind: Annuity,
    ) -> Outcome {
        arity(args, 3, 5)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        let (rate, a, b) = (v[0], v[1], v[2]);
        let c = v.get(3).copied().unwrap_or(0.0);
        let due = v.get(4).is_some_and(|t| *t != 0.0);
        let result = match kind {
            Annuity::Pmt if a == 0.0 => return Err(CellError::Num),
            Annuity::Pmt => annuity_pmt(rate, a, b, c, due),
            Annuity::Fv => annuity_fv(rate, a, b, c, due),
            Annuity::Pv => annuity_pv(rate, a, b, c, due),
            Annuity::Nper => annuity_nper(rate, a, b, c, due).ok_or(CellError::Num)?,
        };
        finite(result)
    }

    /// IPMT, or PPMT (`principal`): one period's interest or principal.
    fn eval_ipmt(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        principal: bool,
    ) -> Outcome {
        arity(args, 4, 6)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        let (rate, per, nper, pv) = (v[0], v[1], v[2], v[3]);
        let fv = v.get(4).copied().unwrap_or(0.0);
        let due = v.get(5).is_some_and(|t| *t != 0.0);
        if per < 1.0 || per > nper {
            return Err(CellError::Num);
        }
        let interest = annuity_ipmt(rate, per, nper, pv, fv, due);
        finite(if principal {
            annuity_pmt(rate, nper, pv, fv, due) - interest
        } else {
            interest
        })
    }

    /// CUMIPMT, or CUMPRINC (`principal`), over periods start..=end.
    fn eval_cumulative(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        principal: bool,
    ) -> Outcome {
        arity(args, 6, 6)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        let (rate, nper, pv, start, end, kind) =
            (v[0], v[1], v[2], v[3].trunc(), v[4].trunc(), v[5]);
        // Periods are summed one by one, so over a million is #NUM! rather
        // than a long wait.
        if rate <= 0.0
            || nper <= 0.0
            || pv <= 0.0
            || start < 1.0
            || end < start
            || end > nper
            || end - start >= 1e6
            || (kind != 0.0 && kind != 1.0)
        {
            return Err(CellError::Num);
        }
        let due = kind == 1.0;
        let payment = annuity_pmt(rate, nper, pv, 0.0, due);
        let mut total = 0.0;
        let mut per = start;
        while per <= end {
            let interest = annuity_ipmt(rate, per, nper, pv, 0.0, due);
            total += if principal {
                payment - interest
            } else {
                interest
            };
            per += 1.0;
        }
        finite(total)
    }

    fn eval_rate(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 6)?;
        let v = self.numbers_from(&args[..3], 0, sheet, engine)?;
        let (nper, payment, pv) = (v[0], v[1], v[2]);
        let fv = self.number_or(args, 3, 0.0, sheet, engine)?;
        let due = if self.number_or(args, 4, 0.0, sheet, engine)? != 0.0 {
            1.0
        } else {
            0.0
        };
        let guess = self.number_or(args, 5, 0.1, sheet, engine)?;
        if nper <= 0.0 {
            return Err(CellError::Num);
        }
        let balance = |r: f64| {
            if r.abs() < 1e-12 {
                pv + payment * nper + fv
            } else {
                let g = (1.0 + r).powf(nper);
                pv * g + payment * (1.0 + r * due) * (g - 1.0) / r + fv
            }
        };
        solve_rate(balance, guess)
            .map(CellResult::Value)
            .ok_or(CellError::Num)
    }

    /// NPV: the values discounted from the end of the first period on.
    fn eval_npv(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.len() < 2 {
            return Err(CellError::Value);
        }
        let rate = self.number(&args[0], sheet, engine)?;
        let values = self.numbers(&args[1..], sheet, engine)?;
        if rate == -1.0 {
            return Err(CellError::DivZero);
        }
        finite(npv(rate, &values, 1))
    }

    fn eval_irr(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let values = self.numbers(&args[..1], sheet, engine)?;
        let guess = self.number_or(args, 1, 0.1, sheet, engine)?;
        if !values.iter().any(|v| *v > 0.0) || !values.iter().any(|v| *v < 0.0) {
            return Err(CellError::Num);
        }
        solve_rate(|r| npv(r, &values, 0), guess)
            .map(CellResult::Value)
            .ok_or(CellError::Num)
    }

    /// MIRR: negative flows financed at one rate, positive ones reinvested
    /// at another.
    fn eval_mirr(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let values = self.numbers(&args[..1], sheet, engine)?;
        let finance = self.number(&args[1], sheet, engine)?;
        let reinvest = self.number(&args[2], sheet, engine)?;
        let n = values.len() as f64;
        let (mut gains, mut costs) = (0.0, 0.0);
        for (i, v) in values.iter().enumerate() {
            if *v > 0.0 {
                gains += v * (1.0 + reinvest).powf(n - 1.0 - i as f64);
            } else {
                costs += v / (1.0 + finance).powf(i as f64);
            }
        }
        if gains == 0.0 || costs == 0.0 || n < 2.0 {
            return Err(CellError::DivZero);
        }
        finite((gains / -costs).powf(1.0 / (n - 1.0)) - 1.0)
    }

    /// Cash flows and their dates for XNPV and XIRR: numbers in step, the
    /// dates whole and none before the first.
    fn dated_flows(
        &self,
        values: &Expr,
        dates: &Expr,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<(f64, f64)>, CellError> {
        let pairs = match self.paired_cells(values, dates, sheet, engine) {
            Err(CellError::NA) => return Err(CellError::Num),
            other => other?,
        };
        let mut flows = Vec::with_capacity(pairs.len());
        for pair in pairs {
            match pair {
                (CellResult::Error(e), _) | (_, CellResult::Error(e)) => return Err(e),
                (CellResult::Value(v), CellResult::Value(d)) => flows.push((v, d.trunc())),
                _ => return Err(CellError::Value),
            }
        }
        match flows.first() {
            Some(&(_, first)) if flows.iter().all(|(_, d)| *d >= first) => Ok(flows),
            _ => Err(CellError::Num),
        }
    }

    fn eval_xnpv(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let rate = self.number(&args[0], sheet, engine)?;
        let flows = self.dated_flows(&args[1], &args[2], sheet, engine)?;
        if rate <= -1.0 {
            return Err(CellError::Num);
        }
        finite(xnpv(rate, &flows))
    }

    fn eval_xirr(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let flows = self.dated_flows(&args[0], &args[1], sheet, engine)?;
        let guess = self.number_or(args, 2, 0.1, sheet, engine)?;
        if !flows.iter().any(|f| f.0 > 0.0) || !flows.iter().any(|f| f.0 < 0.0) {
            return Err(CellError::Num);
        }
        solve_rate(|r| xnpv(r, &flows), guess)
            .map(CellResult::Value)
            .ok_or(CellError::Num)
    }

    fn eval_sln(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        let (cost, salvage, life) = (v[0], v[1], v[2]);
        if life == 0.0 {
            return Err(CellError::DivZero);
        }
        finite((cost - salvage) / life)
    }

    fn eval_syd(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 4, 4)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        let (cost, salvage, life, per) = (v[0], v[1], v[2], v[3]);
        if life <= 0.0 || per <= 0.0 || per > life || salvage < 0.0 {
            return Err(CellError::Num);
        }
        finite((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
    }

    /// Fixed-declining balance at a rate rounded to three decimals, as
    /// Excel does; the first and last periods are prorated by `month`.
    fn eval_db(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 4, 5)?;
        let v = self.numbers_from(&args[..4], 0, sheet, engine)?;
        let (cost, salvage, life, period) = (v[0], v[1], v[2], v[3].trunc());
        let month = self.number_or(args, 4, 12.0, sheet, engine)?.trunc();
        let last = if month < 12.0 { life + 1.0 } else { life };
        if cost < 0.0
            || salvage < 0.0
            || life <= 0.0
            || period < 1.0
            || period > last
            || !(1.0..=12.0).contains(&month)
        {
            return Err(CellError::Num);
        }
        if cost == 0.0 {
            return Ok(CellResult::Value(0.0));
        }
        let rate = round_half_away(1.0 - (salvage / cost).powf(1.0 / life), 3);
        let first = cost * rate * month / 12.0;
        // Each later period takes `rate` of what the one before left.
        let left_after_first = cost - first;
        let depreciation = if period == 1.0 {
            first
        } else if period == life + 1.0 {
            left_after_first * (1.0 - rate).powf(life - 1.0) * rate * (12.0 - month) / 12.0
        } else {
            left_after_first * (1.0 - rate).powf(period - 2.0) * rate
        };
        finite(depreciation)
    }

    /// Double-declining balance (or another `factor`), never below salvage.
    fn eval_ddb(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 4, 5)?;
        let v = self.numbers_from(&args[..4], 0, sheet, engine)?;
        let (cost, salvage, life, period) = (v[0], v[1], v[2], v[3]);
        let factor = self.number_or(args, 4, 2.0, sheet, engine)?;
        if cost < 0.0
            || salvage < 0.0
            || life <= 0.0
            || period <= 0.0
            || period > life
            || factor <= 0.0
        {
            return Err(CellError::Num);
        }
        let rate = (factor / life).min(1.0);
        let before = if rate == 1.0 {
            if period == 1.0 { cost } else { 0.0 }
        } else {
            cost * (1.0 - rate).powf(period - 1.0)
        };
        let after = cost * (1.0 - rate).powf(period);
        let depreciation = if after < salvage {
            before - salvage
        } else {
            before - after
        };
        finite(depreciation.max(0.0))
    }

    // ========== Logical Functions ==========

    fn eval_if(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        if self.logical(&args[0], sheet, engine)? {
            Ok(self.branch(&args[1], sheet, engine))
        } else {
            Ok(args
                .get(2)
                .map_or(CellResult::Bool(false), |e| self.branch(e, sheet, engine)))
        }
    }

    /// AND, OR and XOR. Every argument is read, so an error anywhere is
    /// the answer, as in Excel.
    fn eval_and_or(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, kind: Logic) -> Outcome {
        let values = self.logicals(args, sheet, engine)?;
        let trues = values.iter().filter(|b| **b).count();
        Ok(CellResult::Bool(match kind {
            Logic::And => trues == values.len(),
            Logic::Or => trues > 0,
            Logic::Xor => trues % 2 == 1,
        }))
    }

    fn eval_not(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        Ok(CellResult::Bool(!self.logical(&args[0], sheet, engine)?))
    }

    fn eval_iferror(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.branch(&args[0], sheet, engine);
        if val.is_error() {
            self.branch(&args[1], sheet, engine)
        } else {
            val
        }
    }

    fn eval_ifna(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() != 2 {
            return CellResult::Error(CellError::Value);
        }

        let val = self.branch(&args[0], sheet, engine);
        if matches!(val, CellResult::Error(CellError::NA)) {
            self.branch(&args[1], sheet, engine)
        } else {
            val
        }
    }

    /// IFS(condition1, value1, ...): the value of the first true condition,
    /// #N/A when none is.
    fn eval_ifs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.len() < 2 || !args.len().is_multiple_of(2) {
            return Err(CellError::Value);
        }
        for pair in args.chunks(2) {
            if self.logical(&pair[0], sheet, engine)? {
                return Ok(self.branch(&pair[1], sheet, engine));
            }
        }
        Err(CellError::NA)
    }

    /// SWITCH(value, case1, result1, ..., [default]).
    fn eval_switch(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.len() < 3 {
            return Err(CellError::Value);
        }
        let value = self.eval_arg(&args[0], sheet, engine);
        if let CellResult::Error(e) = value {
            return Err(e);
        }
        for pair in args[1..].chunks_exact(2) {
            match self.eval_arg(&pair[0], sheet, engine) {
                CellResult::Error(e) => return Err(e),
                case if values_equal(&value, &case) => {
                    return Ok(self.branch(&pair[1], sheet, engine));
                }
                _ => {}
            }
        }
        // An even count leaves the default last.
        if args.len().is_multiple_of(2) {
            Ok(self.branch(&args[args.len() - 1], sheet, engine))
        } else {
            Err(CellError::NA)
        }
    }

    fn eval_choose(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.len() < 2 {
            return Err(CellError::Value);
        }
        let index = self.number(&args[0], sheet, engine)?.trunc();
        if index < 1.0 || index >= args.len() as f64 {
            return Err(CellError::Value);
        }
        Ok(self.branch(&args[index as usize], sheet, engine))
    }

    // ========== Text Functions ==========

    /// One text in, one value out. Numbers read as General shows them.
    fn eval_text1(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        f: fn(&str) -> CellResult,
    ) -> Outcome {
        arity(args, 1, 1)?;
        match f(&self.text_arg(&args[0], sheet, engine)?) {
            CellResult::Error(e) => Err(e),
            result => Ok(result),
        }
    }

    /// CONCATENATE, or CONCAT (`ranges`), which also joins every cell of a
    /// range or item of an array.
    fn eval_concatenate(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        ranges: bool,
    ) -> Outcome {
        let push = |out: &mut String, value: CellResult| match value {
            CellResult::Error(e) => Err(e),
            v => {
                out.push_str(&v.to_text().unwrap_or_default());
                Ok(())
            }
        };
        let mut out = String::new();
        for arg in args {
            // Blanks add nothing.
            match ranges.then(|| self.source(arg, sheet, engine)).flatten() {
                Some(block) => block?
                    .used_values(engine)
                    .try_for_each(|v| push(&mut out, v))?,
                None => out.push_str(&self.text_arg(arg, sheet, engine)?),
            }
            // Over-long for certain (no character is more than 4 bytes):
            // stop before a huge range builds a huge string.
            if out.len() > MAX_TEXT * 4 {
                return Err(CellError::Value);
            }
        }
        text_result(out)
    }

    /// LEFT, or RIGHT (`from_end`): the first or last characters, one when
    /// the count is left out.
    fn eval_left_right(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        from_end: bool,
    ) -> Outcome {
        arity(args, 1, 2)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let count = self.opt_number(args, 1, 1.0, sheet, engine)?.trunc();
        if count < 0.0 {
            return Err(CellError::Value);
        }
        // Casts saturate: a huge count takes everything.
        let count = count as usize;
        let chars = text.chars();
        Ok(CellResult::Text(if from_end {
            let skip = text.chars().count().saturating_sub(count);
            chars.skip(skip).collect()
        } else {
            chars.take(count).collect()
        }))
    }

    fn eval_mid(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let start = self.number(&args[1], sheet, engine)?.trunc();
        let count = self.number(&args[2], sheet, engine)?.trunc();
        if start < 1.0 || count < 0.0 {
            return Err(CellError::Value);
        }
        Ok(CellResult::Text(
            text.chars()
                .skip(start as usize - 1)
                .take(count as usize)
                .collect(),
        ))
    }

    /// FIND (`case_sensitive`) or SEARCH: where `find_text` first starts in
    /// `within_text` at or after `start_num`, counting characters from 1.
    /// Empty `find_text` is found at once. SEARCH takes wildcards, as in
    /// Excel; FIND reads every character literally.
    fn eval_find(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        case_sensitive: bool,
    ) -> Outcome {
        arity(args, 2, 3)?;
        let find_text = self.text_arg(&args[0], sheet, engine)?;
        let haystack: Vec<char> = self.text_arg(&args[1], sheet, engine)?.chars().collect();
        let start = self.opt_number(args, 2, 1.0, sheet, engine)?.trunc();
        if start < 1.0 || start > haystack.len().max(1) as f64 {
            return Err(CellError::Value);
        }
        let start = start as usize - 1;
        if !case_sensitive && has_wildcards(&find_text) {
            let folded: Vec<char> = haystack.iter().copied().map(fold).collect();
            return wildcard_find(&find_text, &folded, start)
                .map(|i| CellResult::Value((i + 1) as f64))
                .ok_or(CellError::Value);
        }
        let needle: Vec<char> = find_text.chars().collect();
        let same = |a: &char, b: &char| {
            a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
        };
        (start..=haystack.len().saturating_sub(needle.len()))
            .find(|&i| {
                haystack
                    .get(i..i + needle.len())
                    .is_some_and(|window| window.iter().zip(&needle).all(|(a, b)| same(a, b)))
            })
            .map(|i| CellResult::Value((i + 1) as f64))
            .ok_or(CellError::Value)
    }

    /// SUBSTITUTE(text, old_text, new_text, [instance_num]): every
    /// occurrence, or only the given one.
    fn eval_substitute(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 4)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let old = self.text_arg(&args[1], sheet, engine)?;
        let new = self.text_arg(&args[2], sheet, engine)?;
        let instance = match args.get(3) {
            Some(expr) => Some(self.number(expr, sheet, engine)?.trunc()),
            None => None,
        };
        if instance.is_some_and(|n| n < 1.0) {
            return Err(CellError::Value);
        }
        if old.is_empty() {
            return Ok(CellResult::Text(text));
        }
        let result = match instance {
            None => text.replace(&old, &new),
            Some(n) => match text.match_indices(&old).nth(n as usize - 1) {
                Some((at, _)) => format!("{}{new}{}", &text[..at], &text[at + old.len()..]),
                None => text,
            },
        };
        text_result(result)
    }

    /// REPLACE(old_text, start_num, num_chars, new_text), by characters.
    fn eval_replace(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 4, 4)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let start = self.number(&args[1], sheet, engine)?.trunc();
        let count = self.number(&args[2], sheet, engine)?.trunc();
        let new = self.text_arg(&args[3], sheet, engine)?;
        if start < 1.0 || count < 0.0 {
            return Err(CellError::Value);
        }
        let start = start as usize - 1;
        let before: String = text.chars().take(start).collect();
        let after: String = text
            .chars()
            .skip(start.saturating_add(count as usize))
            .collect();
        text_result(before + &new + &after)
    }

    /// REPT(text, number_times), up to Excel's 32,767 characters.
    fn eval_rept(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let times = self.number(&args[1], sheet, engine)?.trunc();
        if times < 0.0 || text.chars().count() as f64 * times > MAX_TEXT as f64 {
            return Err(CellError::Value);
        }
        Ok(CellResult::Text(text.repeat(times as usize)))
    }

    fn eval_exact(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let a = self.text_arg(&args[0], sheet, engine)?;
        let b = self.text_arg(&args[1], sheet, engine)?;
        Ok(CellResult::Bool(a == b))
    }

    /// VALUE: text that reads as a number, amount, percent, date or time.
    fn eval_value(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        match self.eval_arg(&args[0], sheet, engine) {
            CellResult::Value(n) => Ok(CellResult::Value(n)),
            CellResult::Error(e) => Err(e),
            CellResult::Text(s) => text_to_number(s.trim())
                .map(CellResult::Value)
                .ok_or(CellError::Value),
            CellResult::Bool(_) | CellResult::Empty => Err(CellError::Value),
        }
    }

    fn eval_text(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let val = self.eval_arg(&args[0], sheet, engine);
        if let CellResult::Error(e) = val {
            return Err(e);
        }
        let format = self.text_arg(&args[1], sheet, engine)?;
        Ok(CellResult::Text(match val {
            CellResult::Value(n) => crate::format::format_number(n, &format).text,
            other => other.to_text().unwrap_or_default(),
        }))
    }

    fn eval_char(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let n = self.number(&args[0], sheet, engine)?.trunc();
        if !(1.0..=255.0).contains(&n) {
            return Err(CellError::Value);
        }
        Ok(CellResult::Text(char::from(n as u8).to_string()))
    }

    /// TEXTJOIN(delimiter, ignore_empty, text1, ...). A range or array of
    /// delimiters is used in turn. Like Excel, results over 32,767
    /// characters are #VALUE!.
    fn eval_textjoin(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        if args.len() < 3 {
            return Err(CellError::Value);
        }
        let as_text = |v: CellResult| match v {
            CellResult::Error(e) => Err(e),
            v => Ok(v.to_text().unwrap_or_default()),
        };
        let delimiters: Vec<String> = match self.source(&args[0], sheet, engine) {
            Some(block) => {
                let block = block?;
                if block.height() as u64 * block.width() as u64 > MAX_TEXT as u64 {
                    return Err(CellError::Value);
                }
                block
                    .all_values(engine)
                    .map(as_text)
                    .collect::<Result<_, _>>()?
            }
            None => vec![self.text_arg(&args[0], sheet, engine)?],
        };
        let ignore_empty = self.logical(&args[1], sheet, engine)?;
        // With only empty delimiters, empty pieces change nothing.
        let skip_empty = ignore_empty || delimiters.iter().all(String::is_empty);

        let mut out = String::new();
        let (mut pieces, mut chars) = (0usize, 0usize);
        let mut push = |piece: &str| -> Result<(), CellError> {
            if piece.is_empty() && skip_empty {
                return Ok(());
            }
            if pieces > 0 {
                let delimiter = &delimiters[(pieces - 1) % delimiters.len()];
                out.push_str(delimiter);
                chars += delimiter.chars().count();
            }
            out.push_str(piece);
            chars += piece.chars().count();
            pieces += 1;
            if chars > MAX_TEXT {
                return Err(CellError::Value);
            }
            Ok(())
        };
        for arg in &args[2..] {
            match self.source(arg, sheet, engine) {
                // Skipped blanks past the used part need not be visited;
                // kept ones fill the output to its limit first.
                Some(block) => {
                    let block = block?;
                    let values: Box<dyn Iterator<Item = CellResult>> = if skip_empty {
                        Box::new(block.used_values(engine))
                    } else {
                        Box::new(block.all_values(engine))
                    };
                    for value in values {
                        push(&as_text(value)?)?;
                    }
                }
                None => push(&self.text_arg(arg, sheet, engine)?)?,
            }
        }
        Ok(CellResult::Text(out))
    }

    /// CLEAN: drops the control characters 0-31.
    fn eval_clean(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        Ok(CellResult::Text(
            text.chars().filter(|c| (*c as u32) >= 32).collect(),
        ))
    }

    /// T: text stays, anything else but an error becomes "".
    fn eval_t(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        match self.eval_arg(&args[0], sheet, engine) {
            CellResult::Text(s) => Ok(CellResult::Text(s)),
            CellResult::Error(e) => Err(e),
            _ => Ok(CellResult::Text(String::new())),
        }
    }

    /// DOLLAR (`currency`) and FIXED: a number rounded to `decimals` (left
    /// of the point when negative) as text with thousands separators.
    fn eval_fixed(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        currency: bool,
    ) -> Outcome {
        arity(args, 1, if currency { 2 } else { 3 })?;
        let n = self.number(&args[0], sheet, engine)?;
        let decimals = self.number_or(args, 1, 2.0, sheet, engine)?.trunc();
        let no_commas = !currency && self.logical_or(args, 2, false, sheet, engine)?;
        if decimals > 127.0 {
            return Err(CellError::Value);
        }
        // Past f64's range, rounding changes nothing.
        let rounded = round_half_away(n, decimals.max(-308.0) as i32);
        let rounded = if rounded.is_finite() { rounded } else { n };
        let body = format_fixed(rounded.abs(), decimals.max(0.0) as usize, !no_commas);
        let negative = rounded < 0.0;
        Ok(CellResult::Text(match (currency, negative) {
            (true, true) => format!("(${body})"),
            (true, false) => format!("${body}"),
            (false, true) => format!("-{body}"),
            (false, false) => body,
        }))
    }

    fn eval_unichar(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let n = self.number(&args[0], sheet, engine)?.trunc();
        if !(1.0..=1_114_111.0).contains(&n) {
            return Err(CellError::Value);
        }
        // Lone surrogates are not characters.
        char::from_u32(n as u32)
            .map(|c| CellResult::Text(c.to_string()))
            .ok_or(CellError::NA)
    }

    fn eval_unicode(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        text.chars()
            .next()
            .map(|c| CellResult::Value(c as u32 as f64))
            .ok_or(CellError::Value)
    }

    /// NUMBERVALUE(text, [decimal_separator], [group_separator]), locale
    /// independent: spaces are ignored and each trailing % divides by 100.
    fn eval_numbervalue(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 3)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        let separator = |i: usize, default: char| -> Result<char, CellError> {
            match given(args, i) {
                Some(expr) => self
                    .text_arg(expr, sheet, engine)?
                    .chars()
                    .next()
                    .ok_or(CellError::Value),
                None => Ok(default),
            }
        };
        let decimal = separator(1, '.')?;
        let group = separator(2, ',')?;
        if decimal == group {
            return Err(CellError::Value);
        }
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let number = compact.trim_end_matches('%');
        let percents = (compact.len() - number.len()) as i32;
        if number.is_empty() {
            return if percents == 0 {
                Ok(CellResult::Value(0.0))
            } else {
                Err(CellError::Value)
            };
        }
        // One decimal separator at most, and no grouping after it.
        if number.matches(decimal).count() > 1
            || number
                .find(decimal)
                .is_some_and(|i| number[i..].contains(group))
        {
            return Err(CellError::Value);
        }
        let plain: String = number
            .chars()
            .filter(|c| *c != group)
            .map(|c| if c == decimal { '.' } else { c })
            .collect();
        match plain.parse::<f64>() {
            Ok(n) if n.is_finite() => finite(n / 100f64.powi(percents)),
            _ => Err(CellError::Value),
        }
    }

    /// TEXTBEFORE (`before`) and TEXTAFTER(text, delimiter, [instance_num],
    /// [match_mode], [match_end], [if_not_found]). A negative instance
    /// counts from the end; a range of delimiters matches any of them.
    fn eval_text_split(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        before: bool,
    ) -> Outcome {
        arity(args, 2, 6)?;
        let text: Vec<char> = self.text_arg(&args[0], sheet, engine)?.chars().collect();
        let chars = |v: CellResult| match v {
            CellResult::Error(e) => Err(e),
            v => Ok(v.to_text().unwrap_or_default().chars().collect()),
        };
        let delimiters: Vec<Vec<char>> = match self.source(&args[1], sheet, engine) {
            Some(block) => {
                let block = block?;
                if block.used(engine) == (0, 0) {
                    return Err(CellError::Value);
                }
                block
                    .used_values(engine)
                    .map(chars)
                    .collect::<Result<_, _>>()?
            }
            None => vec![self.text_arg(&args[1], sheet, engine)?.chars().collect()],
        };
        let instance = self.number_or(args, 2, 1.0, sheet, engine)?.trunc();
        let ignore_case = self.number_or(args, 3, 0.0, sheet, engine)? != 0.0;
        let match_end = self.number_or(args, 4, 0.0, sheet, engine)? != 0.0;
        if instance == 0.0 || instance.abs() > text.len().max(1) as f64 {
            return Err(CellError::Value);
        }

        let same =
            |a: char, b: char| a == b || (ignore_case && a.to_lowercase().eq(b.to_lowercase()));
        let found_at = |i: usize| -> Option<usize> {
            delimiters
                .iter()
                .find(|d| {
                    i + d.len() <= text.len() && d.iter().zip(&text[i..]).all(|(x, y)| same(*x, *y))
                })
                .map(Vec::len)
        };
        // An empty delimiter matches at once: at the start, or the end when
        // counting back.
        let matches: Vec<(usize, usize)> = if delimiters.iter().any(Vec::is_empty) {
            let at = if instance > 0.0 { 0 } else { text.len() };
            vec![(at, at)]
        } else {
            let mut matches = Vec::new();
            let mut i = 0;
            while i < text.len() {
                match found_at(i) {
                    Some(len) => {
                        matches.push((i, i + len));
                        i += len;
                    }
                    None => i += 1,
                }
            }
            matches
        };

        let n = instance.abs() as usize;
        let hit = if n <= matches.len() {
            Some(if instance > 0.0 {
                matches[n - 1]
            } else {
                matches[matches.len() - n]
            })
        } else if match_end && n == matches.len() + 1 {
            // The text's end (or start, counting back) is one more delimiter.
            let edge = if instance > 0.0 { text.len() } else { 0 };
            Some((edge, edge))
        } else {
            None
        };
        match hit {
            Some((start, end)) => {
                let piece = if before { &text[..start] } else { &text[end..] };
                Ok(CellResult::Text(piece.iter().collect()))
            }
            None => match given(args, 5) {
                Some(expr) => Ok(self.eval_arg(expr, sheet, engine)),
                None => Err(CellError::NA),
            },
        }
    }

    // ========== Lookup Functions ==========

    /// VLOOKUP (`vertical`) or HLOOKUP(value, table, index, [approximate]):
    /// the entry of the table's first column (row) that matches, answered
    /// from column (row) `index` of its row (column). Approximate matching,
    /// the default, takes the last entry at or below the value, as Excel's
    /// search finds in sorted data.
    fn eval_table_lookup(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        vertical: bool,
    ) -> Outcome {
        arity(args, 3, 4)?;
        let needle = self.eval_arg(&args[0], sheet, engine);
        if let CellResult::Error(e) = needle {
            return Err(e);
        }
        let table = self.block(&args[1], sheet, engine)?;
        let index = self.number(&args[2], sheet, engine)?.trunc();
        // An empty slot is FALSE here, not the default: VLOOKUP(x, t, 2,) is
        // exact, as in Excel.
        let approximate = match args.get(3) {
            Some(expr) => self.logical(expr, sheet, engine)?,
            None => true,
        };
        let across = if vertical {
            table.width()
        } else {
            table.height()
        };
        if index < 1.0 {
            return Err(CellError::Value);
        }
        if index > across as f64 {
            return Err(CellError::Ref);
        }
        let entry = |i: u32, offset: u32| {
            if vertical {
                table.get(engine, i, offset)
            } else {
                table.get(engine, offset, i)
            }
        };
        // Entries past the used window are blank.
        let (rows, cols) = table.used(engine);
        let scan = if vertical { rows } else { cols };
        let exact = Exact::new(&needle);
        let mut found = None;
        for i in 0..scan {
            let value = entry(i, 0);
            if !approximate {
                if exact.matches(&value) {
                    found = Some(i);
                    break;
                }
            } else if lookup_order(&value, &needle).is_some_and(|o| o.is_le()) {
                found = Some(i);
            }
        }
        let i = found.ok_or(CellError::NA)?;
        Ok(entry(i, index as u32 - 1))
    }

    /// INDEX(range, row, [column]). A one-row range takes the column as its
    /// only number, and 0 picks the whole of a one-cell-wide dimension.
    fn eval_index(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let table = self.block(&args[0], sheet, engine)?;
        let first = self.number(&args[1], sheet, engine)?.trunc();
        let (row, col) = match args.get(2) {
            Some(expr) => (first, self.number(expr, sheet, engine)?.trunc()),
            None if table.height() == 1 => (1.0, first),
            None => (first, 1.0),
        };
        let pick = |n: f64, size: u32| -> Result<u32, CellError> {
            match n {
                n if n < 0.0 => Err(CellError::Value),
                // The whole row or column: one cell only when it is one wide.
                0.0 if size == 1 => Ok(0),
                0.0 => Err(CellError::Value),
                n if n > size as f64 => Err(CellError::Ref),
                n => Ok(n as u32 - 1),
            }
        };
        let (row, col) = (pick(row, table.height())?, pick(col, table.width())?);
        Ok(table.get(engine, row, col))
    }

    /// MATCH(value, row_or_column, [type]): the 1-based position of an equal
    /// entry (type 0); in ascending data of the last entry at or below the
    /// value (positive type, the default); in descending data of the last
    /// at or above it (negative type).
    fn eval_match(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let needle = self.eval_arg(&args[0], sheet, engine);
        if let CellResult::Error(e) = needle {
            return Err(e);
        }
        let list = self.block(&args[1], sheet, engine)?;
        let kind = match args.get(2) {
            Some(expr) => self.number(expr, sheet, engine)?,
            None => 1.0,
        };
        if list.height() > 1 && list.width() > 1 {
            return Err(CellError::NA);
        }
        let across = list.height() == 1;
        // Entries past the used window are blank.
        let (rows, cols) = list.used(engine);
        let count = if across { cols } else { rows };
        let exact = Exact::new(&needle);
        let mut found = None;
        for i in 0..count {
            let entry = if across {
                list.get(engine, 0, i)
            } else {
                list.get(engine, i, 0)
            };
            if kind == 0.0 {
                if exact.matches(&entry) {
                    found = Some(i);
                    break;
                }
            } else if lookup_order(&entry, &needle)
                .is_some_and(|o| if kind > 0.0 { o.is_le() } else { o.is_ge() })
            {
                found = Some(i);
            }
        }
        found
            .map(|i| CellResult::Value(i as f64 + 1.0))
            .ok_or(CellError::NA)
    }

    /// ROW (`row`) or COLUMN of a reference, or of the formula's own cell
    /// when there is none.
    fn eval_row_column(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        row: bool,
    ) -> Outcome {
        if args.is_empty() {
            let (_, coord) = engine.current_cell().ok_or(CellError::Value)?;
            let n = if row { coord.row } else { coord.col };
            return Ok(CellResult::Value(n as f64 + 1.0));
        }
        let measure: fn(&CellRange) -> u32 = if row {
            |r| r.start.row + 1
        } else {
            |r| r.start.col + 1
        };
        match self.reference_measure(args, sheet, engine, measure) {
            CellResult::Error(e) => Err(e),
            result => Ok(result),
        }
    }

    /// ROWS (`rows`) or COLUMNS of a reference or an array.
    fn eval_extent(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, rows: bool) -> Outcome {
        arity(args, 1, 1)?;
        let block = self.block(&args[0], sheet, engine)?;
        let n = if rows { block.height() } else { block.width() };
        Ok(CellResult::Value(n as f64))
    }

    /// A number read off the one reference argument.
    fn reference_measure(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        measure: fn(&CellRange) -> u32,
    ) -> CellResult {
        if args.len() != 1 {
            return CellResult::Error(CellError::Value);
        }
        match self.reference(&args[0], sheet, engine) {
            Some(Ok((_, range))) => CellResult::Value(measure(&range) as f64),
            Some(Err(e)) => CellResult::Error(e),
            None => CellResult::Error(CellError::Value),
        }
    }

    /// LOOKUP(value, lookup_vector, [result_vector]): the last entry of
    /// the same type at or below `value`, as Excel's search finds in
    /// sorted data. Without a result vector, a tall array is searched down
    /// its first column and answers from its last; a wide one across its
    /// first row, answering from its last.
    fn eval_lookup(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let needle = self.eval_arg(&args[0], sheet, engine);
        if let CellResult::Error(e) = needle {
            return Err(e);
        }
        let list = self.block(&args[1], sheet, engine)?;
        let down = list.height() >= list.width();
        // Blanks past the used part never match.
        let (rows, cols) = list.used(engine);
        let scan = if down { rows } else { cols };
        let mut found = None;
        for i in 0..scan {
            let entry = if down {
                list.get(engine, i, 0)
            } else {
                list.get(engine, 0, i)
            };
            if lookup_order(&entry, &needle).is_some_and(|o| o.is_le()) {
                found = Some(i);
            }
        }
        let i = found.ok_or(CellError::NA)?;
        let Some(result) = given(args, 2) else {
            return Ok(if down {
                list.get(engine, i, list.width() - 1)
            } else {
                list.get(engine, list.height() - 1, i)
            });
        };
        match self.block(result, sheet, engine)? {
            Block::Cells(result_sheet, r) => {
                // Excel reads on past a short result vector.
                let down = r.height() > 1 || r.width() == 1;
                let (offset, max) = if down {
                    (r.start.row, MAX_ROW)
                } else {
                    (r.start.col, MAX_COL)
                };
                if offset as u64 + i as u64 > max as u64 {
                    return Err(CellError::Ref);
                }
                let coord = if down {
                    CellCoord::new(r.start.row + i, r.start.col)
                } else {
                    CellCoord::new(r.start.row, r.start.col + i)
                };
                Ok(engine.get_value(result_sheet, coord))
            }
            // An array has nothing past its end.
            array => {
                let down = array.height() > 1 || array.width() == 1;
                let (row, col) = if down { (i, 0) } else { (0, i) };
                if row >= array.height() || col >= array.width() {
                    return Err(CellError::NA);
                }
                Ok(array.get(engine, row, col))
            }
        }
    }

    /// ADDRESS(row, column, [abs_num], [a1], [sheet_text]) as text.
    fn eval_address(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 5)?;
        let row = self.number(&args[0], sheet, engine)?.trunc();
        let col = self.number(&args[1], sheet, engine)?.trunc();
        let abs_num = self.number_or(args, 2, 1.0, sheet, engine)?.trunc();
        let a1 = self.logical_or(args, 3, true, sheet, engine)?;
        let sheet_text = match args.get(4) {
            Some(expr) => self.text_arg(expr, sheet, engine)?,
            None => String::new(),
        };
        if !(1.0..=(MAX_ROW + 1) as f64).contains(&row)
            || !(1.0..=(MAX_COL + 1) as f64).contains(&col)
            || !(1.0..=4.0).contains(&abs_num)
        {
            return Err(CellError::Value);
        }
        let (row, col) = (row as u32, col as u32);
        let row_abs = abs_num == 1.0 || abs_num == 2.0;
        let col_abs = abs_num == 1.0 || abs_num == 3.0;
        let address = if a1 {
            CellCoord::new(row - 1, col - 1).to_a1_abs(row_abs, col_abs)
        } else {
            let part = |axis: char, n: u32, absolute: bool| {
                if absolute {
                    format!("{axis}{n}")
                } else {
                    format!("{axis}[{n}]")
                }
            };
            part('R', row, row_abs) + &part('C', col, col_abs)
        };
        let prefix = if sheet_text.is_empty() {
            String::new()
        } else if sheet_text.starts_with(|c: char| c.is_ascii_digit())
            || sheet_text.chars().any(|c| !c.is_alphanumeric() && c != '_')
        {
            format!("'{}'!", sheet_text.replace('\'', "''"))
        } else {
            format!("{sheet_text}!")
        };
        Ok(CellResult::Text(prefix + &address))
    }

    /// INDIRECT(ref_text, [a1]): the reference an A1-style text names.
    /// R1C1 text (a1 FALSE) is not supported and gives #REF!.
    fn indirect_ref(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<(u32, CellRange), CellError> {
        arity(args, 1, 2)?;
        let text = self.text_arg(&args[0], sheet, engine)?;
        if !self.logical_or(args, 1, true, sheet, engine)? {
            return Err(CellError::Ref);
        }
        let (qualifier, range) = parse_reference_text(&text).ok_or(CellError::Ref)?;
        let data_sheet = engine.resolve_sheet(qualifier.as_deref(), sheet)?;
        Ok((data_sheet, range))
    }

    /// OFFSET(reference, rows, cols, [height], [width]): the reference moved
    /// and resized. A negative height or width reaches up or left; leaving
    /// the sheet is #REF!.
    fn offset_ref(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<(u32, CellRange), CellError> {
        arity(args, 3, 5)?;
        let (data_sheet, base) = self
            .reference(&args[0], sheet, engine)
            .ok_or(CellError::Value)??;
        let rows = self.number(&args[1], sheet, engine)?.trunc();
        let cols = self.number(&args[2], sheet, engine)?.trunc();
        let height = self
            .number_or(args, 3, base.height() as f64, sheet, engine)?
            .trunc();
        let width = self
            .number_or(args, 4, base.width() as f64, sheet, engine)?
            .trunc();
        let span = |start: f64, size: f64| -> Option<(f64, f64)> {
            match size {
                s if s > 0.0 => Some((start, start + s - 1.0)),
                s if s < 0.0 => Some((start + s + 1.0, start)),
                _ => None,
            }
        };
        let (top, bottom) = span(base.start.row as f64 + rows, height).ok_or(CellError::Ref)?;
        let (left, right) = span(base.start.col as f64 + cols, width).ok_or(CellError::Ref)?;
        if top < 0.0 || left < 0.0 || bottom > MAX_ROW as f64 || right > MAX_COL as f64 {
            return Err(CellError::Ref);
        }
        Ok((
            data_sheet,
            CellRange::new(
                CellCoord::new(top as u32, left as u32),
                CellCoord::new(bottom as u32, right as u32),
            ),
        ))
    }

    /// FORMULATEXT(reference): the formula in the reference's first cell,
    /// or #N/A when it holds none.
    fn eval_formulatext(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let (s, range) = self
            .reference(&args[0], sheet, engine)
            .ok_or(CellError::Value)??;
        engine
            .get_formula(s, range.start)
            .map(CellResult::Text)
            .ok_or(CellError::NA)
    }

    /// HYPERLINK(link_location, [friendly_name]) shows the friendly name,
    /// or the link itself without one.
    fn eval_hyperlink(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let link = self.text_arg(&args[0], sheet, engine)?;
        match given(args, 1) {
            Some(name) => match self.eval_arg(name, sheet, engine) {
                CellResult::Error(e) => Err(e),
                value => Ok(value),
            },
            None => Ok(CellResult::Text(link)),
        }
    }

    /// SHEET([value]): the sheet number of the calling sheet, a reference,
    /// or a sheet named by text.
    fn eval_sheet(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 0, 1)?;
        let Some(arg) = args.first() else {
            return Ok(CellResult::Value(sheet as f64 + 1.0));
        };
        if let Some(r) = self.reference(arg, sheet, engine) {
            return Ok(CellResult::Value(r?.0 as f64 + 1.0));
        }
        match self.eval_arg(arg, sheet, engine) {
            CellResult::Text(name) => engine
                .sheet_names()
                .iter()
                .position(|n| n.eq_ignore_ascii_case(&name))
                .map(|i| CellResult::Value(i as f64 + 1.0))
                .ok_or(CellError::NA),
            CellResult::Error(e) => Err(e),
            _ => Err(CellError::Value),
        }
    }

    /// SHEETS([reference]): every sheet, or the one a reference is on.
    fn eval_sheets(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 0, 1)?;
        match args.first() {
            None => Ok(CellResult::Value(engine.sheet_names().len() as f64)),
            Some(arg) => match self.reference(arg, sheet, engine) {
                Some(r) => r.map(|_| CellResult::Value(1.0)),
                None => Err(CellError::Value),
            },
        }
    }

    // ========== Conditional Aggregation ==========

    fn eval_sumif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let (range, criteria_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let (sum_range, sum_sheet) =
            match self.bind_sum_range(given(args, 2), (range, criteria_sheet), sheet, engine) {
                Ok(v) => v,
                Err(e) => return e,
            };

        // Past the used window both ranges are blank, adding nothing.
        let mut sum = 0.0;
        let window = engine.used_window(&[(criteria_sheet, range), (sum_sheet, sum_range)]);
        if let Some(window) = window {
            let (part, sum_part) = (leading(&range, window), leading(&sum_range, window));
            for (coord, sum_coord) in part.iter().zip(sum_part.iter()) {
                let cell_val = engine.get_value(criteria_sheet, coord);
                if self.matches_criteria(&cell_val, &criteria) {
                    // Only numbers add up; an error among them is the answer.
                    match engine.get_value(sum_sheet, sum_coord) {
                        CellResult::Value(n) => sum += n,
                        CellResult::Error(e) => return CellResult::Error(e),
                        _ => {}
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

        let (range, data_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let part = engine.used_part(data_sheet, &range);
        let mut count = 0;
        if let Some(part) = part {
            for coord in part.iter() {
                let cell_val = engine.get_value(data_sheet, coord);
                if self.matches_criteria(&cell_val, &criteria) {
                    count += 1;
                }
            }
        }
        // The rest of the range is blank.
        let blanks = range.cell_count() - part.map_or(0, |p| p.cell_count());
        if blanks > 0 && self.matches_criteria(&CellResult::Empty, &criteria) {
            count += blanks;
        }

        CellResult::Value(count as f64)
    }

    fn eval_averageif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || args.len() > 3 {
            return CellResult::Error(CellError::Value);
        }

        let (range, criteria_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };

        let criteria = self.eval_arg(&args[1], sheet, engine);

        let (avg_range, avg_sheet) =
            match self.bind_sum_range(given(args, 2), (range, criteria_sheet), sheet, engine) {
                Ok(v) => v,
                Err(e) => return e,
            };

        // Only numbers are averaged, as in Excel, so the blanks past the
        // used window count for nothing.
        let mut sum = 0.0;
        let mut count = 0;
        let window = engine.used_window(&[(criteria_sheet, range), (avg_sheet, avg_range)]);
        if let Some(window) = window {
            let (part, avg_part) = (leading(&range, window), leading(&avg_range, window));
            for (coord, avg_coord) in part.iter().zip(avg_part.iter()) {
                let cell_val = engine.get_value(criteria_sheet, coord);
                if self.matches_criteria(&cell_val, &criteria) {
                    match engine.get_value(avg_sheet, avg_coord) {
                        CellResult::Value(n) => {
                            sum += n;
                            count += 1;
                        }
                        CellResult::Error(e) => return CellResult::Error(e),
                        _ => {}
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

        let (range, data_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };

        let part = engine.used_part(data_sheet, &range);
        let mut count = 0;
        if let Some(part) = part {
            for coord in part.iter() {
                if matches!(engine.get_value(data_sheet, coord), CellResult::Empty) {
                    count += 1;
                }
            }
        }
        // The rest of the range is blank.
        count += range.cell_count() - part.map_or(0, |p| p.cell_count());

        CellResult::Value(count as f64)
    }

    fn eval_sumifs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return CellResult::Error(CellError::Value);
        }
        let (sum_range, sum_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let criteria = match self.bind_criteria(&args[1..], sheet, engine, &sum_range) {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Past the used window every range is blank, adding nothing.
        let Some((window, mask)) =
            self.criteria_mask(&criteria, Some((sum_sheet, sum_range)), engine)
        else {
            return CellResult::Value(0.0);
        };
        let mut sum = 0.0;
        for (i, coord) in leading(&sum_range, window).iter().enumerate() {
            if !mask[i] {
                continue;
            }
            match engine.get_value(sum_sheet, coord) {
                CellResult::Value(n) => sum += n,
                CellResult::Error(e) => return CellResult::Error(e),
                _ => {}
            }
        }
        CellResult::Value(sum)
    }

    fn eval_countifs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 2 || !args.len().is_multiple_of(2) {
            return CellResult::Error(CellError::Value);
        }
        let (first_range, _) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let criteria = match self.bind_criteria(args, sheet, engine, &first_range) {
            Ok(c) => c,
            Err(e) => return e,
        };

        let (matched, checked) = match self.criteria_mask(&criteria, None, engine) {
            Some(((rows, cols), mask)) => (
                mask.iter().filter(|m| **m).count() as u64,
                rows as u64 * cols as u64,
            ),
            None => (0, 0),
        };
        // The rest of each range is blank.
        let blanks = first_range.cell_count() - checked;
        let blanks_match = criteria
            .iter()
            .all(|(_, _, c)| self.matches_criteria(&CellResult::Empty, c));
        let count = matched + if blanks_match { blanks } else { 0 };
        CellResult::Value(count as f64)
    }

    fn eval_averageifs(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> CellResult {
        if args.len() < 3 || args.len().is_multiple_of(2) {
            return CellResult::Error(CellError::Value);
        }
        let (avg_range, avg_sheet) = match self.bind_range(&args[0], sheet, engine) {
            Ok(v) => v,
            Err(e) => return e,
        };
        let criteria = match self.bind_criteria(&args[1..], sheet, engine, &avg_range) {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Past the used window every range is blank, averaging nothing.
        let Some((window, mask)) =
            self.criteria_mask(&criteria, Some((avg_sheet, avg_range)), engine)
        else {
            return CellResult::Error(CellError::DivZero);
        };
        let mut sum = 0.0;
        let mut count = 0;
        for (i, coord) in leading(&avg_range, window).iter().enumerate() {
            if !mask[i] {
                continue;
            }
            match engine.get_value(avg_sheet, coord) {
                CellResult::Value(n) => {
                    sum += n;
                    count += 1;
                }
                CellResult::Error(e) => return CellResult::Error(e),
                _ => {}
            }
        }
        if count == 0 {
            CellResult::Error(CellError::DivZero)
        } else {
            CellResult::Value(sum / count as f64)
        }
    }

    /// Bind `range, criteria` pairs; each range must be shaped like `shape`.
    fn bind_criteria(
        &self,
        pairs: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        shape: &CellRange,
    ) -> Result<Vec<(u32, CellRange, CellResult)>, CellResult> {
        if pairs.len() < 2 || !pairs.len().is_multiple_of(2) {
            return Err(CellResult::Error(CellError::Value));
        }
        pairs
            .chunks(2)
            .map(|pair| {
                let (range, data_sheet) = self.bind_range(&pair[0], sheet, engine)?;
                if range.width() != shape.width() || range.height() != shape.height() {
                    return Err(CellResult::Error(CellError::Value));
                }
                let criteria = self.eval_arg(&pair[1], sheet, engine);
                Ok((data_sheet, range, criteria))
            })
            .collect()
    }

    /// Which cells of the used window (over the criteria ranges and
    /// `target`) meet every criterion, in row-major order. `None` when all
    /// the ranges are blank.
    fn criteria_mask(
        &self,
        criteria: &[(u32, CellRange, CellResult)],
        target: Option<(u32, CellRange)>,
        engine: &CalcEngine,
    ) -> Option<((u32, u32), Vec<bool>)> {
        let ranges: Vec<(u32, CellRange)> = criteria
            .iter()
            .map(|(data_sheet, range, _)| (*data_sheet, *range))
            .chain(target)
            .collect();
        let window = engine.used_window(&ranges)?;
        let mut mask = vec![true; window.0 as usize * window.1 as usize];
        for (data_sheet, range, criteria) in criteria {
            for (i, coord) in leading(range, window).iter().enumerate() {
                if mask[i]
                    && !self.matches_criteria(&engine.get_value(*data_sheet, coord), criteria)
                {
                    mask[i] = false;
                }
            }
        }
        Some((window, mask))
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

    /// ISEVEN, or ISODD, of the whole part; logicals are #VALUE! as in Excel.
    fn eval_parity(&self, args: &[Expr], sheet: u32, engine: &CalcEngine, odd: bool) -> Outcome {
        arity(args, 1, 1)?;
        let n = match self.eval_arg(&args[0], sheet, engine) {
            CellResult::Bool(_) => return Err(CellError::Value),
            val => to_number(&val)?,
        };
        Ok(CellResult::Bool((n.trunc() % 2.0 != 0.0) == odd))
    }

    /// ISERR: any error but #N/A.
    fn eval_iserr(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let val = self.eval_arg(&args[0], sheet, engine);
        Ok(CellResult::Bool(matches!(
            val,
            CellResult::Error(e) if e != CellError::NA
        )))
    }

    /// ERROR.TYPE: Excel's number for an error, #N/A for anything else.
    /// #CIRC! has no number and stays itself.
    fn eval_error_type(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let CellResult::Error(e) = self.eval_arg(&args[0], sheet, engine) else {
            return Err(CellError::NA);
        };
        let code = match e {
            CellError::Null => 1.0,
            CellError::DivZero => 2.0,
            CellError::Value => 3.0,
            CellError::Ref => 4.0,
            CellError::Name => 5.0,
            CellError::Num => 6.0,
            CellError::NA => 7.0,
            CellError::GettingData => 8.0,
            CellError::Spill => 9.0,
            CellError::Calc => 14.0,
            CellError::Circular => return Err(e),
        };
        Ok(CellResult::Value(code))
    }

    /// ISFORMULA(reference): whether its first cell holds a formula.
    fn eval_isformula(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 1)?;
        let (s, range) = self
            .reference(&args[0], sheet, engine)
            .ok_or(CellError::Value)??;
        Ok(CellResult::Bool(
            engine.get_formula(s, range.start).is_some(),
        ))
    }

    // ========== Date/Time Functions ==========

    fn eval_date(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let v = self.numbers_from(args, 0, sheet, engine)?;
        // Casts saturate; anything that far out ends up past 9999 below.
        let (year, month, day) = (v[0].trunc(), v[1].trunc() as i32, v[2].trunc() as i32);
        // Excel reads years 0-1899 as 1900-3799, and has no dates outside
        // 1900-9999.
        let year = if (0.0..1900.0).contains(&year) {
            year + 1900.0
        } else {
            year
        };
        if !(1900.0..=9999.0).contains(&year) {
            return Err(CellError::Num);
        }
        let serial = date_to_serial(year as i32, month, day);
        if !(0.0..=MAX_DATE_SERIAL).contains(&serial) {
            return Err(CellError::Num);
        }
        Ok(CellResult::Value(serial))
    }

    /// YEAR, MONTH or DAY of a date, picked by `part` from (y, m, d).
    fn eval_date_part(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        part: fn((i32, i32, i32)) -> i32,
    ) -> Outcome {
        arity(args, 1, 1)?;
        let serial = self.date_arg(&args[0], sheet, engine)?;
        Ok(CellResult::Value(part(serial_to_date(serial)) as f64))
    }

    /// TIME(hour, minute, second) as a fraction of a day; whole days wrap
    /// away, and a negative total is #NUM!.
    fn eval_time(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let mut seconds = 0.0;
        for (expr, scale) in args.iter().zip([3600.0, 60.0, 1.0]) {
            let n = self.number(expr, sheet, engine)?.trunc();
            if n > 32767.0 {
                return Err(CellError::Num);
            }
            seconds += n * scale;
        }
        if seconds < 0.0 {
            return Err(CellError::Num);
        }
        Ok(CellResult::Value(seconds.rem_euclid(86_400.0) / 86_400.0))
    }

    /// HOUR, MINUTE or SECOND of a serial, rounded to the nearest second
    /// as Excel does.
    fn eval_time_part(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        unit: u64,
        modulo: u64,
    ) -> Outcome {
        arity(args, 1, 1)?;
        let n = self.number(&args[0], sheet, engine)?;
        if !(0.0..MAX_DATE_SERIAL + 1.0).contains(&n) {
            return Err(CellError::Num);
        }
        let seconds = ((n - n.floor()) * 86_400.0).round() as u64 % 86_400;
        Ok(CellResult::Value((seconds / unit % modulo) as f64))
    }

    /// WEEKDAY(serial, [return_type]): 1 counts from Sunday, 2 from Monday,
    /// 3 from Monday as 0, and 11-17 from Monday through Sunday.
    fn eval_weekday(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let serial = self.date_arg(&args[0], sheet, engine)? as i64;
        let kind = self.number_or(args, 1, 1.0, sheet, engine)?.trunc();
        let day = weekday_index(serial);
        if kind == 3.0 {
            return Ok(CellResult::Value((day + 6).rem_euclid(7) as f64));
        }
        let first = week_start(kind).ok_or(CellError::Num)?;
        Ok(CellResult::Value(((day - first).rem_euclid(7) + 1) as f64))
    }

    /// WEEKNUM(serial, [return_type]): week 1 holds January 1st, and weeks
    /// start on the day `return_type` names (as for WEEKDAY); 21 is the
    /// ISO week.
    fn eval_weeknum(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 1, 2)?;
        let serial = self.date_arg(&args[0], sheet, engine)? as i64;
        let kind = self.number_or(args, 1, 1.0, sheet, engine)?.trunc();
        if kind == 21.0 {
            // The ISO week is the one its Thursday falls in.
            let thursday = serial - (weekday_index(serial) + 6).rem_euclid(7) + 3;
            let (year, _, _) = serial_to_date(thursday as f64);
            let jan1 = date_to_serial(year, 1, 1) as i64;
            return Ok(CellResult::Value(((thursday - jan1) / 7 + 1) as f64));
        }
        let first = week_start(kind).ok_or(CellError::Num)?;
        let (year, _, _) = serial_to_date(serial as f64);
        let jan1 = date_to_serial(year, 1, 1) as i64;
        let lead = (weekday_index(jan1) - first).rem_euclid(7);
        Ok(CellResult::Value(((serial - jan1 + lead) / 7 + 1) as f64))
    }

    /// EDATE, or EOMONTH (`month_end`): `months` months on, keeping the day
    /// where the month has it, or the month's last day.
    fn eval_edate(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        month_end: bool,
    ) -> Outcome {
        arity(args, 2, 2)?;
        let start = self.date_arg(&args[0], sheet, engine)?;
        let months = self.number(&args[1], sheet, engine)?.trunc();
        let (y, m, d) = serial_to_date(start);
        let total = y as f64 * 12.0 + (m - 1) as f64 + months;
        let year = (total / 12.0).floor();
        if !(1900.0..=9999.0).contains(&year) {
            return Err(CellError::Num);
        }
        let (year, month) = (year as i32, (total - year * 12.0) as i32 + 1);
        let last = days_in_month(year, month);
        let day = if month_end { last } else { d.min(last) };
        let serial = date_to_serial(year, month, day);
        if serial < 0.0 {
            return Err(CellError::Num);
        }
        Ok(CellResult::Value(serial))
    }

    /// DATEDIF(start, end, unit) in whole years ("Y"), months ("M"), days
    /// ("D"), or ignoring years ("YM", "YD") or months and years ("MD").
    fn eval_datedif(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 3, 3)?;
        let start = self.date_arg(&args[0], sheet, engine)?;
        let end = self.date_arg(&args[1], sheet, engine)?;
        let unit = self.text_arg(&args[2], sheet, engine)?.to_ascii_uppercase();
        if start > end {
            return Err(CellError::Num);
        }
        let (sy, sm, sd) = serial_to_date(start);
        let (ey, em, ed) = serial_to_date(end);
        let months = (ey - sy) * 12 + (em - sm) - i32::from(ed < sd);
        let result = match unit.as_str() {
            "Y" => (months / 12) as f64,
            "M" => months as f64,
            "D" => end - start,
            "YM" => (months % 12) as f64,
            // Excel's MD counts from the start day in the month before the
            // end, overflowing into the end month when that month is short,
            // which can give a negative count.
            "MD" if ed >= sd => (ed - sd) as f64,
            "MD" => end - date_to_serial(ey, em - 1, sd),
            "YD" => {
                let mut anniversary = date_to_serial(ey, sm, sd);
                if anniversary > end {
                    anniversary = date_to_serial(ey - 1, sm, sd);
                }
                end - anniversary
            }
            _ => return Err(CellError::Num),
        };
        Ok(CellResult::Value(result))
    }

    /// DATEVALUE (`date`) or TIMEVALUE of text: the whole serial, or the
    /// fraction of a day.
    fn eval_datevalue(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
        date: bool,
    ) -> Outcome {
        arity(args, 1, 1)?;
        let text = match self.eval_arg(&args[0], sheet, engine) {
            CellResult::Text(s) => s,
            CellResult::Error(e) => return Err(e),
            _ => return Err(CellError::Value),
        };
        let (day, time) = parse_date_time_text(&text).ok_or(CellError::Value)?;
        let result = if date { day } else { Some(time.unwrap_or(0.0)) };
        result.map(CellResult::Value).ok_or(CellError::Value)
    }

    /// DAYS(end, start): whole days between two dates.
    fn eval_days(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 2)?;
        let end = self.date_arg(&args[0], sheet, engine)?;
        let start = self.date_arg(&args[1], sheet, engine)?;
        Ok(CellResult::Value(end - start))
    }

    /// DAYS360(start, end, [method]) on a 360-day year. The US method
    /// treats a start on the month's last day (February's too) as the 30th,
    /// and an end on the 31st as the 30th only when the start is then the
    /// 30th; the European method (TRUE) makes every 31st a 30th.
    fn eval_days360(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let start = self.date_arg(&args[0], sheet, engine)?;
        let end = self.date_arg(&args[1], sheet, engine)?;
        let european = self.logical_or(args, 2, false, sheet, engine)?;
        let (sy, sm, mut sd) = serial_to_date(start);
        let (ey, em, mut ed) = serial_to_date(end);
        if european {
            sd = sd.min(30);
            ed = ed.min(30);
        } else {
            if sd == days_in_month(sy, sm) {
                sd = 30;
            }
            if ed == 31 && sd == 30 {
                ed = 30;
            }
        }
        Ok(CellResult::Value(
            ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64,
        ))
    }

    /// NETWORKDAYS(start, end, [holidays]): Monday-to-Friday days between
    /// the dates, both counted, less holidays; negative when end is first.
    fn eval_networkdays(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let start = self.date_arg(&args[0], sheet, engine)? as i64;
        let end = self.date_arg(&args[1], sheet, engine)? as i64;
        let holidays = self.date_list(given(args, 2), sheet, engine)?;
        let (lo, hi) = (start.min(end), start.max(end));
        let days = hi - lo + 1;
        let whole_weeks = days / 7;
        let rest = (lo + whole_weeks * 7..=hi)
            .filter(|d| is_workday(*d))
            .count() as i64;
        let off = holidays
            .iter()
            .filter(|h| (lo..=hi).contains(*h) && is_workday(**h))
            .count() as i64;
        let count = whole_weeks * 5 + rest - off;
        let count = if start > end { -count } else { count };
        Ok(CellResult::Value(count as f64))
    }

    /// WORKDAY(start, days, [holidays]): the date `days` working days away.
    fn eval_workday(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let mut date = self.date_arg(&args[0], sheet, engine)? as i64;
        let days = self.number(&args[1], sheet, engine)?.trunc();
        let holidays = self.date_list(given(args, 2), sheet, engine)?;
        if days.abs() > MAX_DATE_SERIAL {
            return Err(CellError::Num);
        }
        let step = if days < 0.0 { -1 } else { 1 };
        let mut left = days.abs() as i64;
        while left > 0 {
            date += step;
            if !(0..=MAX_DATE_SERIAL as i64).contains(&date) {
                return Err(CellError::Num);
            }
            if is_workday(date) && holidays.binary_search(&date).is_err() {
                left -= 1;
            }
        }
        Ok(CellResult::Value(date as f64))
    }

    /// YEARFRAC(start, end, [basis]) for bases 0 (US 30/360), 1
    /// (actual/actual), 2 (actual/360), 3 (actual/365) and 4 (European
    /// 30/360), following Excel's rules for each.
    fn eval_yearfrac(&self, args: &[Expr], sheet: u32, engine: &CalcEngine) -> Outcome {
        arity(args, 2, 3)?;
        let a = self.date_arg(&args[0], sheet, engine)?;
        let b = self.date_arg(&args[1], sheet, engine)?;
        let basis = self.number_or(args, 2, 0.0, sheet, engine)?.trunc();
        let (start, end) = (a.min(b), a.max(b));
        let (sy, sm, sd) = serial_to_date(start);
        let (ey, em, ed) = serial_to_date(end);
        let days = end - start;
        let days360 = |sd: i32, ed: i32| ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64;
        let result = match basis as i32 {
            _ if !(0.0..=4.0).contains(&basis) => return Err(CellError::Num),
            0 => {
                let last_of_feb = |y, m, d| m == 2 && d == days_in_month(y, 2);
                // A 31st end stays unless the start is the 30th or 31st; a
                // start on February's last day moves only an end on
                // February's last day.
                let (sd, ed) = if sd >= 30 && ed == 31 {
                    (30, 30)
                } else if sd == 31 {
                    (30, ed)
                } else if last_of_feb(sy, sm, sd) {
                    (30, if last_of_feb(ey, em, ed) { 30 } else { ed })
                } else {
                    (sd, ed)
                };
                days360(sd, ed) / 360.0
            }
            1 => {
                let within_a_year = sy == ey || (sy + 1 == ey && (sm, sd) >= (em, ed));
                if within_a_year {
                    // A year with a February 29th inside the span counts 366.
                    let spans_leap_day = |y: i32| {
                        is_leap_year(y) && {
                            let leap_day = date_to_serial(y, 2, 29);
                            (start..=end).contains(&leap_day)
                        }
                    };
                    let year_len = if (sy == ey && is_leap_year(sy))
                        || spans_leap_day(sy)
                        || spans_leap_day(ey)
                    {
                        366.0
                    } else {
                        365.0
                    };
                    days / year_len
                } else {
                    let years = (ey - sy + 1) as f64;
                    let span = date_to_serial(ey + 1, 1, 1) - date_to_serial(sy, 1, 1);
                    days / (span / years)
                }
            }
            2 => days / 360.0,
            3 => days / 365.0,
            _ => days360(sd.min(30), ed.min(30)) / 360.0,
        };
        finite(result)
    }

    // ========== Helpers ==========

    /// The block of values an argument gives; #VALUE! for a single value.
    fn block<'a>(
        &self,
        expr: &'a Expr,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Block<'a>, CellError> {
        self.source(expr, sheet, engine)
            .unwrap_or(Err(CellError::Value))
    }

    /// An argument a function can read whole: the cells a reference names,
    /// an array constant, or the array operators compute over ranges and
    /// arrays (`A1:A9*2`). `None` for a single value, which callers read as
    /// they always have.
    fn source<'a>(
        &self,
        expr: &'a Expr,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Option<Result<Block<'a>, CellError>> {
        match expr {
            Expr::Array(rows) => Some(Ok(Block::Array(rows))),
            _ if computes_array(expr) => {
                Some(engine.evaluate_array(sheet, expr).map(Block::Computed))
            }
            _ => self
                .reference(expr, sheet, engine)
                .map(|r| r.map(|(data_sheet, range)| Block::Cells(data_sheet, range))),
        }
    }

    fn bind_range(
        &self,
        expr: &Expr,
        current: u32,
        engine: &CalcEngine,
    ) -> Result<(CellRange, u32), CellResult> {
        match self.reference(expr, current, engine) {
            Some(Ok((sheet, range))) => Ok((range, sheet)),
            Some(Err(e)) => Err(CellResult::Error(e)),
            None => Err(CellResult::Error(CellError::Value)),
        }
    }

    /// The sheet and cells an argument refers to: a cell or range, or
    /// INDIRECT or OFFSET, which return references. `None` for any other
    /// expression, which is a value.
    pub(crate) fn reference(
        &self,
        expr: &Expr,
        current: u32,
        engine: &CalcEngine,
    ) -> Option<Result<(u32, CellRange), CellError>> {
        match expr {
            Expr::CellRef(r) => Some(
                engine
                    .resolve_sheet(r.sheet.as_deref(), current)
                    .map(|sheet| (sheet, CellRange::single(r.coord))),
            ),
            Expr::RangeRef(r) => Some(
                engine
                    .resolve_sheet(r.sheet.as_deref(), current)
                    .map(|sheet| (sheet, r.range)),
            ),
            Expr::Function(f) if f.name == "INDIRECT" => {
                Some(self.indirect_ref(&f.args, current, engine))
            }
            Expr::Function(f) if f.name == "OFFSET" => {
                Some(self.offset_ref(&f.args, current, engine))
            }
            _ => None,
        }
    }

    /// The cells SUMIF and AVERAGEIF add up: as in Excel, the criteria
    /// range's shape, starting at the given range's top-left cell.
    fn bind_sum_range(
        &self,
        arg: Option<&Expr>,
        (criteria, criteria_sheet): (CellRange, u32),
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<(CellRange, u32), CellResult> {
        let Some(arg) = arg else {
            return Ok((criteria, criteria_sheet));
        };
        let (range, data_sheet) = self.bind_range(arg, sheet, engine)?;
        let end = CellCoord::new(
            range.start.row + criteria.height() - 1,
            range.start.col + criteria.width() - 1,
        );
        Ok((CellRange::new(range.start, end), data_sheet))
    }

    fn eval_arg(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> CellResult {
        // Delegate to the engine's expression evaluator to handle all expression types
        // including binary operations, unary operations, and nested function calls
        engine.evaluate_expr(sheet, expr)
    }

    /// What IF, CHOOSE and the like return for a branch: its value, or 0
    /// for an empty one, as in Excel (IF(TRUE,,1) is 0).
    fn branch(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> CellResult {
        match expr {
            Expr::Missing => CellResult::Value(0.0),
            _ => self.eval_arg(expr, sheet, engine),
        }
    }

    /// Numbers to aggregate, with any errors met on the way. As in Excel,
    /// referenced cells and array items count only when they are numbers,
    /// while typed logicals count too, and so does an empty slot, as 0.
    /// Blank cells add nothing, so ranges stop at the sheet's used part.
    fn collect_numeric_values(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Vec<Result<f64, CellError>> {
        let mut values = Vec::new();
        for arg in args {
            if let Expr::Missing = arg {
                values.push(Ok(0.0));
                continue;
            }
            match self.source(arg, sheet, engine) {
                Some(Ok(block)) => values.extend(
                    block
                        .used_values(engine)
                        .filter_map(|v| numeric_for_aggregate(&v, true)),
                ),
                Some(Err(e)) => values.push(Err(e)),
                None => {
                    let val = self.eval_arg(arg, sheet, engine);
                    if let Some(v) = numeric_for_aggregate(&val, false) {
                        values.push(v);
                    }
                }
            }
        }
        values
    }

    fn collect_all_values(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Vec<CellResult> {
        let mut values = Vec::new();
        for arg in args {
            match self.source(arg, sheet, engine) {
                // Only for COUNTA, which skips blanks: stop at the used part.
                Some(Ok(block)) => values.extend(block.used_values(engine)),
                Some(Err(e)) => values.push(CellResult::Error(e)),
                // An empty slot is a value, 0, as for the numeric aggregates.
                None => values.push(self.branch(arg, sheet, engine)),
            }
        }
        values
    }

    /// The numbers an aggregate reads, or the first error among them.
    fn numbers(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<f64>, CellError> {
        self.collect_numeric_values(args, sheet, engine)
            .into_iter()
            .collect()
    }

    /// A scalar argument as a number, the way Excel converts one.
    fn number(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> Result<f64, CellError> {
        to_number(&self.eval_arg(expr, sheet, engine))
    }

    /// Optional numeric argument `i`, or `default` when it is left out or
    /// its slot is empty.
    fn number_or(
        &self,
        args: &[Expr],
        i: usize,
        default: f64,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<f64, CellError> {
        match given(args, i) {
            Some(expr) => self.number(expr, sheet, engine),
            None => Ok(default),
        }
    }

    /// Optional numeric argument `i` of the older functions: `absent` when
    /// left out, while an empty slot reads as a blank, 0, as Excel's own
    /// versions take it (LEFT("abc",) is "").
    fn opt_number(
        &self,
        args: &[Expr],
        i: usize,
        absent: f64,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<f64, CellError> {
        match args.get(i) {
            Some(expr) => self.number(expr, sheet, engine),
            None => Ok(absent),
        }
    }

    /// Every argument from `from` on as a number.
    fn numbers_from(
        &self,
        args: &[Expr],
        from: usize,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<f64>, CellError> {
        args[from..]
            .iter()
            .map(|a| self.number(a, sheet, engine))
            .collect()
    }

    /// A scalar argument as text: numbers as General shows them.
    fn text_arg(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> Result<String, CellError> {
        match self.eval_arg(expr, sheet, engine) {
            CellResult::Error(e) => Err(e),
            val => Ok(val.to_text().unwrap_or_default()),
        }
    }

    /// A scalar argument as a logical: numbers are TRUE unless 0, and text
    /// must spell TRUE or FALSE.
    fn logical(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> Result<bool, CellError> {
        match self.eval_arg(expr, sheet, engine) {
            CellResult::Bool(b) => Ok(b),
            CellResult::Value(n) => Ok(n != 0.0),
            CellResult::Empty => Ok(false),
            CellResult::Text(s) if s.eq_ignore_ascii_case("TRUE") => Ok(true),
            CellResult::Text(s) if s.eq_ignore_ascii_case("FALSE") => Ok(false),
            CellResult::Text(_) => Err(CellError::Value),
            CellResult::Error(e) => Err(e),
        }
    }

    fn logical_or(
        &self,
        args: &[Expr],
        i: usize,
        default: bool,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<bool, CellError> {
        match given(args, i) {
            Some(expr) => self.logical(expr, sheet, engine),
            None => Ok(default),
        }
    }

    /// The logicals AND, OR and XOR read: referenced cells holding numbers
    /// or logicals (text and blanks are skipped), and typed values that
    /// convert. #VALUE! when there are none.
    fn logicals(
        &self,
        args: &[Expr],
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<bool>, CellError> {
        let mut values = Vec::new();
        let mut read = |value: CellResult| -> Result<(), CellError> {
            match value {
                CellResult::Bool(b) => values.push(b),
                CellResult::Value(n) => values.push(n != 0.0),
                CellResult::Error(e) => return Err(e),
                CellResult::Text(_) | CellResult::Empty => {}
            }
            Ok(())
        };
        for arg in args {
            match self.source(arg, sheet, engine) {
                Some(block) => block?.used_values(engine).try_for_each(&mut read)?,
                None => {
                    let b = self.logical(arg, sheet, engine)?;
                    read(CellResult::Bool(b))?;
                }
            }
        }
        if values.is_empty() {
            Err(CellError::Value)
        } else {
            Ok(values)
        }
    }

    /// Two equally sized arguments read cell by cell, in step. Cells past
    /// the used part of both are blank and left out. #N/A when the sizes
    /// differ, as in Excel.
    fn paired_cells(
        &self,
        left: &Expr,
        right: &Expr,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<(CellResult, CellResult)>, CellError> {
        // An array pairs item by item, in reading order, with another array
        // or with an equally large reference.
        let array = |e: &Expr| matches!(e, Expr::Array(_)) || computes_array(e);
        if array(left) || array(right) {
            let a = self.block(left, sheet, engine)?;
            let b = self.block(right, sheet, engine)?;
            let size = |b: &Block| b.height() as u64 * b.width() as u64;
            if size(&a) != size(&b) {
                return Err(CellError::NA);
            }
            let items = |b: &Block| -> Vec<CellResult> {
                (0..b.height())
                    .flat_map(|row| (0..b.width()).map(move |col| b.get(engine, row, col)))
                    .collect()
            };
            return Ok(items(&a).into_iter().zip(items(&b)).collect());
        }
        let a = self.reference(left, sheet, engine).transpose()?;
        let b = self.reference(right, sheet, engine).transpose()?;
        let one = |expr: &Expr, r: Option<(u32, CellRange)>| -> Result<CellResult, CellError> {
            match r {
                Some((s, range)) if range.cell_count() == 1 => Ok(engine.get_value(s, range.start)),
                Some(_) => Err(CellError::NA),
                None => Ok(self.eval_arg(expr, sheet, engine)),
            }
        };
        let (Some((sa, ra)), Some((sb, rb))) = (a, b) else {
            return Ok(vec![(one(left, a)?, one(right, b)?)]);
        };
        if ra.cell_count() != rb.cell_count() {
            return Err(CellError::NA);
        }
        let (ra, rb) = if (ra.width(), ra.height()) == (rb.width(), rb.height()) {
            match engine.used_window(&[(sa, ra), (sb, rb)]) {
                Some(window) => (leading(&ra, window), leading(&rb, window)),
                None => return Ok(Vec::new()),
            }
        } else {
            (ra, rb)
        };
        Ok(ra
            .iter()
            .zip(rb.iter())
            .map(|(ca, cb)| (engine.get_value(sa, ca), engine.get_value(sb, cb)))
            .collect())
    }

    /// Dates from a holidays argument, sorted: a reference's numbers, blanks
    /// skipped, an array's, or one value.
    fn date_list(
        &self,
        expr: Option<&Expr>,
        sheet: u32,
        engine: &CalcEngine,
    ) -> Result<Vec<i64>, CellError> {
        let Some(expr) = expr else {
            return Ok(Vec::new());
        };
        match self.source(expr, sheet, engine) {
            Some(block) => dates_of(block?.used_values(engine)),
            None => dates_of([self.eval_arg(expr, sheet, engine)]),
        }
    }

    /// A date argument as a whole serial; #NUM! outside Excel's calendar.
    fn date_arg(&self, expr: &Expr, sheet: u32, engine: &CalcEngine) -> Result<f64, CellError> {
        let n = self.number(expr, sheet, engine)?;
        if !(0.0..MAX_DATE_SERIAL + 1.0).contains(&n) {
            return Err(CellError::Num);
        }
        Ok(n.floor())
    }

    /// Convert a CellResult to an Option<String> for text functions
    fn to_string_val(&self, val: &CellResult) -> Option<String> {
        val.to_text()
    }

    /// Check if a cell value matches a criteria (for SUMIF, COUNTIF, etc.)
    fn matches_criteria(&self, cell_val: &CellResult, criteria: &CellResult) -> bool {
        // Handle text criteria with operators
        if let CellResult::Text(crit_str) = criteria {
            let crit_str = crit_str.trim();

            // Check for comparison operators
            if let Some(rest) = crit_str.strip_prefix(">=") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val).is_some_and(|n| n >= crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix("<=") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val).is_some_and(|n| n <= crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix("<>") {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val)
                        .is_none_or(|n| (n - crit_num).abs() > f64::EPSILON);
                } else if has_wildcards(rest) {
                    return !text_matches(cell_val, rest.trim());
                } else {
                    // String comparison
                    return !self
                        .to_string_val(cell_val)
                        .is_some_and(|s| s.eq_ignore_ascii_case(rest.trim()));
                }
            } else if let Some(rest) = crit_str.strip_prefix('>') {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val).is_some_and(|n| n > crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix('<') {
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val).is_some_and(|n| n < crit_num);
                }
            } else if let Some(rest) = crit_str.strip_prefix('=') {
                // Explicit equality
                if let Ok(crit_num) = rest.trim().parse::<f64>() {
                    return criteria_number(cell_val)
                        .is_some_and(|n| (n - crit_num).abs() < f64::EPSILON);
                } else if has_wildcards(rest) {
                    return text_matches(cell_val, rest.trim());
                } else {
                    return self
                        .to_string_val(cell_val)
                        .is_some_and(|s| s.eq_ignore_ascii_case(rest.trim()));
                }
            }

            // No operator - try numeric comparison first, then string
            if let Ok(crit_num) = crit_str.parse::<f64>() {
                return criteria_number(cell_val)
                    .is_some_and(|n| (n - crit_num).abs() < f64::EPSILON);
            }

            if has_wildcards(crit_str) {
                return text_matches(cell_val, crit_str);
            }

            // Plain string comparison (case-insensitive)
            return self
                .to_string_val(cell_val)
                .is_some_and(|s| s.eq_ignore_ascii_case(crit_str));
        }

        // Non-text criteria: direct value comparison
        values_equal(cell_val, criteria)
    }
}

/// What the newer functions return; `Err` becomes the cell's error.
type Outcome = Result<CellResult, CellError>;

fn outcome(result: Outcome) -> CellResult {
    result.unwrap_or_else(CellResult::Error)
}

/// Wrong argument counts are #VALUE!, as elsewhere in this file. Empty
/// slots count: IF(A1,,0) has three arguments.
fn arity(args: &[Expr], min: usize, max: usize) -> Result<(), CellError> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(CellError::Value)
    }
}

/// Argument `i` unless it is left out or its slot is empty; optional
/// arguments with a default take the default either way.
fn given(args: &[Expr], i: usize) -> Option<&Expr> {
    args.get(i).filter(|a| !matches!(a, Expr::Missing))
}

/// The most characters a cell holds.
const MAX_TEXT: usize = 32_767;

/// Text a function returns; longer than a cell holds is #VALUE!, as in
/// Excel.
fn text_result(text: String) -> Outcome {
    if text.chars().count() > MAX_TEXT {
        Err(CellError::Value)
    } else {
        Ok(CellResult::Text(text))
    }
}

/// PROPER: a capital after anything but a letter, lower case elsewhere,
/// so "76BudGet" is "76Budget".
fn proper(text: &str) -> CellResult {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if after_letter {
            out.extend(c.to_lowercase());
        } else {
            out.extend(c.to_uppercase());
        }
        after_letter = c.is_alphabetic();
    }
    CellResult::Text(out)
}

/// What AND, OR and XOR make of their logicals.
#[derive(Clone, Copy)]
enum Logic {
    And,
    Or,
    Xor,
}

/// A numeric result; overflow and undefined results are #NUM!, as in Excel.
fn finite(n: f64) -> Outcome {
    if n.is_finite() {
        Ok(CellResult::Value(n))
    } else {
        Err(CellError::Num)
    }
}

/// The value of the one cell a reference names; a larger range has no
/// single value here.
fn deref(reference: Result<(u32, CellRange), CellError>, engine: &CalcEngine) -> CellResult {
    match reference {
        Ok((sheet, range)) if range.cell_count() == 1 => engine.get_value(sheet, range.start),
        Ok(_) => CellResult::Error(CellError::Value),
        Err(e) => CellResult::Error(e),
    }
}

/// Excel's conversion of a scalar to a number: errors pass through, blanks
/// are 0, logicals 1 or 0, and text must read as a number or date.
fn to_number(val: &CellResult) -> Result<f64, CellError> {
    match val {
        CellResult::Value(n) => Ok(*n),
        CellResult::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        CellResult::Empty => Ok(0.0),
        CellResult::Text(s) => text_to_number(s).ok_or(CellError::Value),
        CellResult::Error(e) => Err(*e),
    }
}

/// Text that reads as a number, percent, amount, date or time.
fn text_to_number(s: &str) -> Option<f64> {
    crate::format::parse_typed_number(s)
        .map(|(n, _)| n)
        .or_else(|| {
            let (date, time) = parse_date_time_text(s)?;
            Some(date.unwrap_or(0.0) + time.unwrap_or(0.0))
        })
}

/// `x` to the 15 significant digits Excel keeps, so 1.3/0.2 is 6.5 rather
/// than 6.4999...
fn snap(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.14e}").parse().unwrap_or(x)
}

/// ROUND's rounding: half away from zero, at 15 significant digits.
fn round_half_away(n: f64, digits: i32) -> f64 {
    round_to(n, digits, Rounding::Nearest)
}

/// Which way ROUND, ROUNDUP and ROUNDDOWN go.
#[derive(Clone, Copy)]
enum Rounding {
    /// Halves away from zero
    Nearest,
    /// Away from zero
    Up,
    /// Toward zero
    Down,
}

/// `n` to `digits` decimals, or to tens, hundreds... when negative, at the
/// 15 significant digits Excel keeps: ROUND(2.675, 2) is 2.68 although
/// 2.675 is stored a hair below it.
fn round_to(n: f64, digits: i32, mode: Rounding) -> f64 {
    if !n.is_finite() {
        return n;
    }
    // Past f64's decimal range nothing changes, and 10^300+ powers differ
    // by an ulp between platforms: answer without scaling.
    if digits > 308 {
        return n + 0.0;
    }
    if digits < -308 {
        return 0.0;
    }
    let m = 10f64.powi(digits.abs());
    let scaled = if digits >= 0 { n * m } else { n / m };
    // Past f64's precision there is nothing to round.
    if !scaled.is_finite() {
        return n;
    }
    let s = snap(scaled);
    let s = match mode {
        Rounding::Nearest => s.round(),
        Rounding::Up if s < 0.0 => s.floor(),
        Rounding::Up => s.ceil(),
        Rounding::Down => s.trunc(),
    };
    // + 0.0 turns -0 (ROUND(-0.4, 0)) into 0.
    (if digits >= 0 { s / m } else { s * m }) + 0.0
}

/// Numbers used by SUM/AVERAGE/COUNT/etc. Blanks and text are skipped.
/// Empty is 0 in operators (`as_number`), not here.
fn numeric_for_aggregate(val: &CellResult, in_range: bool) -> Option<Result<f64, CellError>> {
    match val {
        CellResult::Value(n) => Some(Ok(*n)),
        CellResult::Error(e) => Some(Err(*e)),
        CellResult::Empty | CellResult::Text(_) => None,
        CellResult::Bool(b) => {
            if in_range {
                None
            } else {
                Some(Ok(if *b { 1.0 } else { 0.0 }))
            }
        }
    }
}

/// The first error among aggregated values: SUM and friends return it.
fn first_error(values: &[Result<f64, CellError>]) -> Option<CellError> {
    values.iter().find_map(|v| v.err())
}

/// Numeric criteria must not treat a blank as 0.
fn criteria_number(val: &CellResult) -> Option<f64> {
    match val {
        CellResult::Value(n) => Some(*n),
        CellResult::Bool(true) => Some(1.0),
        CellResult::Bool(false) => Some(0.0),
        _ => None,
    }
}

pub(crate) fn apply_text_format(n: f64, format: &str) -> String {
    if is_date_format(format) {
        return format_date_serial(n, format);
    }

    let (prefix, body, suffix) = split_format_literals(format);
    let percent = body.contains('%');
    let scientific = body.to_ascii_uppercase().contains('E');
    let thousands = body.contains(',');
    let decimals = decimal_places(&body);
    let mut value = n;
    if percent {
        value *= 100.0;
    }

    let mut number = if scientific {
        format_scientific(value, decimals)
    } else {
        format_fixed(value, decimals, thousands)
    };
    if percent {
        number.push('%');
    }
    format!("{prefix}{number}{suffix}")
}

fn is_date_format(format: &str) -> bool {
    let body = strip_quoted(format).to_ascii_lowercase();
    body.contains("yy") || body.contains("dd") || body.contains("mmm") || body.contains("mm/dd")
}

fn strip_quoted(format: &str) -> String {
    let mut out = String::new();
    let mut in_quote = false;
    for c in format.chars() {
        if c == '"' {
            in_quote = !in_quote;
            continue;
        }
        if !in_quote {
            out.push(c);
        }
    }
    out
}

fn split_format_literals(format: &str) -> (String, String, String) {
    let mut prefix = String::new();
    let mut body = String::new();
    let mut suffix = String::new();
    let mut in_quote = false;
    let mut seen_num = false;
    let mut literal = String::new();

    for c in format.chars() {
        if c == '"' {
            in_quote = !in_quote;
            continue;
        }
        if in_quote {
            literal.push(c);
            continue;
        }
        if matches!(c, '0' | '#' | '.' | ',' | '%' | 'E' | 'e' | '+' | '-') {
            if !literal.is_empty() {
                if seen_num {
                    suffix.push_str(&literal);
                } else {
                    prefix.push_str(&literal);
                }
                literal.clear();
            }
            seen_num = true;
            body.push(c);
        } else {
            literal.push(c);
        }
    }
    if !literal.is_empty() {
        if seen_num {
            suffix.push_str(&literal);
        } else {
            prefix.push_str(&literal);
        }
    }
    (prefix, body, suffix)
}

fn decimal_places(body: &str) -> usize {
    let upper = body.to_ascii_uppercase();
    let number_part = upper.split('E').next().unwrap_or(&upper);
    match number_part.split_once('.') {
        Some((_, frac)) => frac.chars().filter(|c| *c == '0' || *c == '#').count(),
        None => 0,
    }
}

fn format_fixed(n: f64, decimals: usize, thousands: bool) -> String {
    let rounded = if decimals == 0 {
        n.round()
    } else {
        let m = 10f64.powi(decimals as i32);
        (n * m).round() / m
    };
    let negative = rounded.is_sign_negative() && rounded != 0.0;
    let formatted = format!("{:.*}", decimals, rounded.abs());
    let (int_part, frac) = match formatted.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (formatted, None),
    };
    let int_part = if thousands {
        add_thousands(&int_part)
    } else {
        int_part
    };
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&int_part);
    if let Some(frac) = frac {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

fn add_thousands(int_part: &str) -> String {
    let mut digits: Vec<char> = int_part.chars().collect();
    if digits.is_empty() {
        return "0".into();
    }
    let mut out = String::new();
    let mut count = 0;
    while let Some(c) = digits.pop() {
        if count > 0 && count % 3 == 0 {
            out.insert(0, ',');
        }
        out.insert(0, c);
        count += 1;
    }
    out
}

fn format_scientific(n: f64, decimals: usize) -> String {
    if n == 0.0 {
        let zeros = "0".repeat(decimals);
        return if decimals == 0 {
            "0E+00".into()
        } else {
            format!("0.{zeros}E+00")
        };
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let abs = n.abs();
    let exp = abs.log10().floor() as i32;
    let mantissa = abs / 10f64.powi(exp);
    format!("{sign}{:.*}E{exp:+03}", decimals, mantissa)
}

fn format_date_serial(serial: f64, format: &str) -> String {
    let (year, month, day) = serial_to_date(serial);
    let mut out = String::new();
    let mut in_quote = false;
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }
        if in_quote {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let lower = rest.to_ascii_lowercase();
        if lower.starts_with("yyyy") {
            out.push_str(&format!("{year:04}"));
            i += 4;
        } else if lower.starts_with("yy") {
            out.push_str(&format!("{:02}", year % 100));
            i += 2;
        } else if lower.starts_with("mmmm") {
            out.push_str(month_name(month));
            i += 4;
        } else if lower.starts_with("mmm") {
            out.push_str(&month_name(month)[..3]);
            i += 3;
        } else if lower.starts_with("mm") {
            out.push_str(&format!("{month:02}"));
            i += 2;
        } else if lower.starts_with('m') {
            out.push_str(&month.to_string());
            i += 1;
        } else if lower.starts_with("dd") {
            out.push_str(&format!("{day:02}"));
            i += 2;
        } else if lower.starts_with('d') {
            out.push_str(&day.to_string());
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn month_name(month: i32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Simple pseudo-random number generator (deterministic for reproducibility in tests)
/// Uses a simple linear congruential generator seeded from system time
fn rand_simple() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

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

/// Excel's serial for a date: 1900-01-01 is 1. Excel's calendar keeps
/// Lotus 1-2-3's February 29th, 1900 as serial 60, so every month from
/// March 1900 on starts a day later than the real calendar puts it. Days
/// outside the month carry into the next or previous one in that same
/// calendar: DATE(1900,2,29) is 60, DATE(1900,3,0) too.
pub(crate) fn date_to_serial(year: i32, month: i32, day: i32) -> f64 {
    // Months outside 1-12 carry into the year: month 0 is last December.
    let months = year as i64 * 12 + (month as i64 - 1);
    let (y, m) = (months.div_euclid(12), months.rem_euclid(12) + 1);
    let mut before_month = days_from_civil(y, m) - days_from_civil(1900, 1);
    if (y, m) > (1900, 2) {
        before_month += 1;
    }
    (before_month + day as i64) as f64
}

/// Days from 1970-01-01 to the first of the month, in the proleptic
/// Gregorian calendar.
fn days_from_civil(year: i64, month: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Last serial Excel accepts as a date: 9999-12-31.
const MAX_DATE_SERIAL: f64 = 2_958_465.0;

/// Serial of 1970-01-01, where Unix time starts.
const UNIX_EPOCH_SERIAL: f64 = 25_569.0;

/// The serial for now, for NOW() and TODAY(): the engine's clock moved
/// from UTC by its offset.
fn now_serial(engine: &CalcEngine) -> f64 {
    let seconds = engine.utc_seconds() + f64::from(engine.clock_offset()) * 60.0;
    seconds / 86_400.0 + UNIX_EPOCH_SERIAL
}

fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// Day of the week of a serial: 0 is Sunday. Serial 1, 1900-01-01, is a
/// Sunday to Excel.
fn weekday_index(serial: i64) -> i64 {
    (serial - 1).rem_euclid(7)
}

/// The date and time parts of text such as `2024-01-15`, `1/15/2024`,
/// `15-Jan-2024`, `January 15, 2024 6:30 PM` or `18:30`, as a whole serial
/// and a fraction of a day. `None` unless all of it reads.
fn parse_date_time_text(s: &str) -> Option<(Option<f64>, Option<f64>)> {
    let s = s.trim().to_ascii_lowercase();
    // The time starts at the word holding the first colon.
    let (date_part, time_part) = match s.find(':') {
        Some(colon) => {
            let start = s[..colon].rfind(' ').map_or(0, |i| i + 1);
            (s[..start].trim(), Some(&s[start..]))
        }
        None => (s.as_str(), None),
    };
    let time = match time_part {
        Some(t) => match crate::format::parse_typed_number(t) {
            Some((n, Some(_))) if (0.0..1.0).contains(&n) => Some(n),
            _ => return None,
        },
        None => None,
    };
    let date = if date_part.is_empty() {
        None
    } else {
        Some(parse_date_words(date_part)?)
    };
    (date.is_some() || time.is_some()).then_some((date, time))
}

/// A date written with numbers or a month name: `2024-01-15`, `2024/1/15`,
/// `1/15/2024`, `15-Jan-2024`, `Jan 15, 2024`. Two-digit years below 30
/// are 20xx, the rest 19xx, as in Excel.
fn parse_date_words(s: &str) -> Option<f64> {
    let words: Vec<&str> = s
        .split([' ', '-', '/', ','])
        .filter(|w| !w.is_empty())
        .collect();
    let [a, b, c] = words[..] else {
        return None;
    };
    let number = |w: &str| -> Option<i32> {
        (w.len() <= 4 && w.chars().all(|c| c.is_ascii_digit()))
            .then(|| w.parse().ok())
            .flatten()
    };
    let month_named = |w: &str| -> Option<i32> {
        (w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphabetic()))
            .then(|| (1..=12).find(|&m| month_name(m).to_ascii_lowercase().starts_with(w)))
            .flatten()
    };
    // Numbers alone need one separator throughout: 1/15/2024, not 1 15 2024.
    let numeric = |sep: char| s.chars().all(|c| c.is_ascii_digit() || c == sep);
    let (year_word, month, day) = if let Some(m) = month_named(a) {
        (c, m, number(b)?)
    } else if let Some(m) = month_named(b) {
        (c, m, number(a)?)
    } else if !numeric('-') && !numeric('/') {
        return None;
    } else if a.len() == 4 {
        (a, number(b)?, number(c)?)
    } else {
        (c, number(a)?, number(b)?)
    };
    let year = match (number(year_word)?, year_word.len()) {
        (y, 1 | 2) if y < 30 => 2000 + y,
        (y, 1 | 2) => 1900 + y,
        (y, _) => y,
    };
    // Excel's calendar has a February 29th, 1900.
    let leap_1900 = (year, month, day) == (1900, 2, 29);
    if !(1900..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || (day > days_in_month(year, month) && !leap_1900)
    {
        return None;
    }
    Some(date_to_serial(year, month, day))
}

/// The (year, month, day) of an Excel serial; 0 is January 0th, 1900 and
/// 60 the February 29th, 1900 Excel keeps (see `date_to_serial`).
pub(crate) fn serial_to_date(serial: f64) -> (i32, i32, i32) {
    let mut days = serial as i32;
    if days == 60 {
        return (1900, 2, 29);
    }
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

/// A piece of a wildcard pattern.
#[derive(Clone, Copy, PartialEq)]
enum Wild {
    /// One character, case folded
    Char(char),
    /// `?`: any one character
    One,
    /// `*`: any run of characters, none included
    Any,
}

/// Whether text has characters a wildcard pattern treats specially.
fn has_wildcards(s: &str) -> bool {
    s.contains(['*', '?', '~'])
}

/// A character compared ignoring case, one for one so positions hold.
fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// `pattern` as Excel's criteria, lookups and SEARCH read it: `*` and `?`
/// are wildcards, and `~` takes the `*`, `?` or `~` after it literally.
/// Before anything else, a `~` is itself.
fn wildcard_pattern(pattern: &str) -> Vec<Wild> {
    let mut out = Vec::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(match c {
            '~' => match chars.next_if(|n| matches!(n, '*' | '?' | '~')) {
                Some(escaped) => Wild::Char(escaped),
                None => Wild::Char('~'),
            },
            '*' => Wild::Any,
            '?' => Wild::One,
            c => Wild::Char(fold(c)),
        });
    }
    out
}

/// Whether `pattern` matches the whole of `text` (case folded). Greedy,
/// backing up only to the last `*`, so no input makes it recurse or
/// explode: at worst pattern length times text length steps.
fn pattern_matches(pattern: &[Wild], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    // After the last `*` seen: where the pattern resumes, and the text
    // position that `*` has absorbed up to.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(Wild::Any) => {
                star = Some((p + 1, t));
                p += 1;
            }
            Some(Wild::One) => (p, t) = (p + 1, t + 1),
            Some(Wild::Char(c)) if *c == text[t] => (p, t) = (p + 1, t + 1),
            _ => match star {
                Some((resume, absorbed)) => {
                    star = Some((resume, absorbed + 1));
                    (p, t) = (resume, absorbed + 1);
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|w| *w == Wild::Any)
}

/// Whether wildcard `pattern` matches all of `text`, ignoring case.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let text: Vec<char> = text.chars().map(fold).collect();
    pattern_matches(&wildcard_pattern(pattern), &text)
}

/// Whether a value is text that wildcard `pattern` matches whole. As in
/// Excel, wildcards never match numbers, logicals or blanks.
fn text_matches(value: &CellResult, pattern: &str) -> bool {
    matches!(value, CellResult::Text(s) if wildcard_match(pattern, s))
}

/// Equality for exact lookups, criteria and SWITCH: numbers, text ignoring
/// case, logicals, and blank with blank.
fn values_equal(a: &CellResult, b: &CellResult) -> bool {
    match (a, b) {
        (CellResult::Value(x), CellResult::Value(y)) => (x - y).abs() < f64::EPSILON,
        (CellResult::Text(x), CellResult::Text(y)) => x.eq_ignore_ascii_case(y),
        (CellResult::Bool(x), CellResult::Bool(y)) => x == y,
        (CellResult::Empty, CellResult::Empty) => true,
        _ => false,
    }
}

/// What an exact lookup (MATCH type 0, VLOOKUP and HLOOKUP with FALSE)
/// takes as the value sought: as in Excel, text with wildcards is a
/// pattern that only text entries can match.
enum Exact<'a> {
    Pattern(Vec<Wild>),
    Value(&'a CellResult),
}

impl<'a> Exact<'a> {
    fn new(needle: &'a CellResult) -> Self {
        match needle {
            CellResult::Text(s) if has_wildcards(s) => Exact::Pattern(wildcard_pattern(s)),
            other => Exact::Value(other),
        }
    }

    fn matches(&self, entry: &CellResult) -> bool {
        match (self, entry) {
            (Exact::Pattern(p), CellResult::Text(s)) => {
                pattern_matches(p, &s.chars().map(fold).collect::<Vec<_>>())
            }
            (Exact::Pattern(_), _) => false,
            (Exact::Value(needle), entry) => values_equal(needle, entry),
        }
    }
}

/// Where SEARCH finds wildcard `pattern` in `text` (case folded) at or
/// after `start`: the first position a match begins, whatever follows it.
fn wildcard_find(pattern: &str, text: &[char], start: usize) -> Option<usize> {
    let pattern = wildcard_pattern(pattern);
    // The part before the first `*` fixes where a match starts.
    let head_len = pattern
        .iter()
        .position(|w| *w == Wild::Any)
        .unwrap_or(pattern.len());
    let (head, tail) = pattern.split_at(head_len);
    let head_at = |i: usize| {
        text.get(i..i + head.len()).is_some_and(|window| {
            head.iter()
                .zip(window)
                .all(|(w, c)| *w == Wild::One || *w == Wild::Char(*c))
        })
    };
    let i = (start..=text.len()).find(|&i| head_at(i))?;
    if tail.is_empty() {
        return Some(i);
    }
    // The rest starts with `*`, so if it fails after the first place the
    // head fits it fails after every later one: one try settles it.
    let mut rest = tail.to_vec();
    rest.push(Wild::Any);
    pattern_matches(&rest, &text[i + head_len..]).then_some(i)
}

/// A block of values a function reads whole: cells of a sheet, an array
/// constant, or an array operators computed.
enum Block<'a> {
    Cells(u32, CellRange),
    /// Rows of literals, all the same length
    Array(&'a [Vec<Expr>]),
    Computed(Array),
}

impl Block<'_> {
    fn height(&self) -> u32 {
        match self {
            Block::Cells(_, range) => range.height(),
            Block::Array(rows) => rows.len() as u32,
            Block::Computed(array) => array.rows(),
        }
    }

    fn width(&self) -> u32 {
        match self {
            Block::Cells(_, range) => range.width(),
            Block::Array(rows) => rows.iter().map(|row| row.len() as u32).max().unwrap_or(0),
            Block::Computed(array) => array.cols(),
        }
    }

    /// The value `row` rows down and `col` columns across from the top left.
    fn get(&self, engine: &CalcEngine, row: u32, col: u32) -> CellResult {
        match self {
            Block::Cells(sheet, range) => engine.get_value(
                *sheet,
                CellCoord::new(range.start.row + row, range.start.col + col),
            ),
            // A hand-built ragged array reads #N/A past a short row.
            Block::Array(rows) => match rows.get(row as usize).and_then(|r| r.get(col as usize)) {
                Some(item) => literal(item),
                None => CellResult::Error(CellError::NA),
            },
            Block::Computed(array) => array.get(row, col).clone(),
        }
    }

    /// How many rows and columns, from the top left, can hold values: past
    /// them every cell is blank.
    fn used(&self, engine: &CalcEngine) -> (u32, u32) {
        match self {
            Block::Cells(sheet, range) => engine.used_window(&[(*sheet, *range)]).unwrap_or((0, 0)),
            Block::Array(_) | Block::Computed(_) => (self.height(), self.width()),
        }
    }

    /// Every value that can be other than blank, row by row: all of an
    /// array, the used part of cells.
    fn used_values<'b>(&'b self, engine: &'b CalcEngine) -> impl Iterator<Item = CellResult> + 'b {
        let (rows, cols) = self.used(engine);
        (0..rows).flat_map(move |row| (0..cols).map(move |col| self.get(engine, row, col)))
    }

    /// Every value, row by row.
    fn all_values<'b>(&'b self, engine: &'b CalcEngine) -> impl Iterator<Item = CellResult> + 'b {
        let (rows, cols) = (self.height(), self.width());
        (0..rows).flat_map(move |row| (0..cols).map(move |col| self.get(engine, row, col)))
    }
}

/// Holidays as whole serials, sorted and deduplicated; blanks are skipped.
fn dates_of(values: impl IntoIterator<Item = CellResult>) -> Result<Vec<i64>, CellError> {
    let mut dates = Vec::new();
    for v in values {
        match v {
            CellResult::Empty => {}
            CellResult::Value(n) if (0.0..=MAX_DATE_SERIAL).contains(&n) => {
                dates.push(n.floor() as i64)
            }
            CellResult::Value(_) => return Err(CellError::Num),
            CellResult::Error(e) => return Err(e),
            CellResult::Text(_) | CellResult::Bool(_) => return Err(CellError::Value),
        }
    }
    dates.sort_unstable();
    dates.dedup();
    Ok(dates)
}

/// The first day of the week for WEEKDAY and WEEKNUM return types, as a
/// `weekday_index`.
fn week_start(kind: f64) -> Option<i64> {
    match kind as i64 {
        1 | 17 => Some(0),
        2 | 11 => Some(1),
        k @ 12..=16 => Some(k - 10),
        _ => None,
    }
}

/// Monday to Friday.
fn is_workday(serial: i64) -> bool {
    !matches!(weekday_index(serial), 0 | 6)
}

/// An A1-style reference as INDIRECT reads it: `B2`, `$A$1:C3`, `A:C` or
/// `2:5`, with an optional `Sheet!` or `'My sheet'!` prefix. The sheet
/// comes back unquoted.
fn parse_reference_text(text: &str) -> Option<(Option<String>, CellRange)> {
    let text = text.trim();
    let (sheet, address) = match text.rfind('!') {
        Some(i) => {
            let name = &text[..i];
            let name = match name.strip_prefix('\'').and_then(|n| n.strip_suffix('\'')) {
                Some(quoted) => quoted.replace("''", "'"),
                None => name.to_string(),
            };
            if name.is_empty() {
                return None;
            }
            (Some(name), &text[i + 1..])
        }
        None => (None, text),
    };
    let cell = |s: &str| CellCoord::from_a1(s).filter(|c| c.row <= MAX_ROW && c.col <= MAX_COL);
    let column = |s: &str| {
        let bare = s.trim().trim_start_matches('$');
        (!bare.is_empty() && bare.chars().all(|c| c.is_ascii_alphabetic()))
            .then(|| cell(&format!("{bare}1")).map(|c| c.col))
            .flatten()
    };
    let row = |s: &str| {
        let bare = s.trim().trim_start_matches('$');
        bare.parse::<u32>()
            .ok()
            .filter(|r| (1..=MAX_ROW + 1).contains(r))
            .map(|r| r - 1)
    };
    let range = match address.split_once(':') {
        None => CellRange::single(cell(address)?),
        Some((a, b)) => {
            if let (Some(a), Some(b)) = (cell(a), cell(b)) {
                CellRange::new(a, b)
            } else if let (Some(a), Some(b)) = (column(a), column(b)) {
                CellRange::new(CellCoord::new(0, a), CellCoord::new(MAX_ROW, b))
            } else if let (Some(a), Some(b)) = (row(a), row(b)) {
                CellRange::new(CellCoord::new(a, 0), CellCoord::new(b, MAX_COL))
            } else {
                return None;
            }
        }
    };
    Some((sheet, range))
}

/// How a LOOKUP entry compares with the value sought; `None` when their
/// types differ, as those never match.
fn lookup_order(cell: &CellResult, needle: &CellResult) -> Option<std::cmp::Ordering> {
    match (cell, needle) {
        (CellResult::Value(a), CellResult::Value(b)) => a.partial_cmp(b),
        (CellResult::Text(a), CellResult::Text(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        (CellResult::Bool(a), CellResult::Bool(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Roman numerals in Excel's five forms: 0 is classic (499 is CDXCIX),
/// and each form up to 4 allows wider subtractive pairs (4 gives ID).
/// `n` is below 4000.
fn roman(mut n: u32, form: u32) -> String {
    const CHARS: [char; 7] = ['M', 'D', 'C', 'L', 'X', 'V', 'I'];
    const VALUES: [u32; 7] = [1000, 500, 100, 50, 10, 5, 1];
    let mut out = String::new();
    for power in 0..4 {
        let mut index = 2 * power;
        let digit = n / VALUES[index];
        if digit % 5 == 4 {
            // 4 pairs with the five above, 9 with the ten above; looser
            // forms step the smaller numeral down while it still fits.
            let upper = if digit == 4 { index - 1 } else { index - 2 };
            let mut steps = 0;
            while steps < form && index < 6 {
                steps += 1;
                if VALUES[upper] - VALUES[index + 1] <= n {
                    index += 1;
                } else {
                    steps = form;
                }
            }
            out.push(CHARS[index]);
            out.push(CHARS[upper]);
            n = n + VALUES[index] - VALUES[upper];
        } else {
            if digit > 4 {
                out.push(CHARS[index - 1]);
            }
            for _ in 0..digit % 5 {
                out.push(CHARS[index]);
            }
            n %= VALUES[index];
        }
    }
    out
}

/// PERCENTILE.INC of sorted values; `None` outside 0..=1 or when empty.
fn percentile_inc(sorted: &[f64], k: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=1.0).contains(&k) {
        return None;
    }
    interpolate(sorted, (sorted.len() - 1) as f64 * k)
}

/// PERCENTILE.EXC of sorted values: k must leave a point on each side.
fn percentile_exc(sorted: &[f64], k: f64) -> Option<f64> {
    let n = sorted.len() as f64;
    let rank = snap((n + 1.0) * k);
    if !(1.0..=n).contains(&rank) {
        return None;
    }
    interpolate(sorted, rank - 1.0)
}

/// The value at a fractional 0-based position, between its neighbors.
fn interpolate(sorted: &[f64], position: f64) -> Option<f64> {
    let position = snap(position);
    let lower = position.floor() as usize;
    let low = *sorted.get(lower)?;
    Some(match sorted.get(lower + 1) {
        Some(high) => low + (position - lower as f64) * (high - low),
        None => low,
    })
}

/// The arithmetic behind GEOMEAN, HARMEAN, AVEDEV and DEVSQ.
#[derive(Clone, Copy)]
enum Mean {
    Geometric,
    Harmonic,
    AbsDeviation,
    SquaredDeviation,
}

/// Statistics of (y, x) points.
#[derive(Clone, Copy, PartialEq)]
enum Paired {
    Correl,
    Rsq,
    CovarianceP,
    CovarianceS,
    Slope,
    Intercept,
    Steyx,
}

fn regression(points: &[(f64, f64)], kind: Paired) -> Result<f64, CellError> {
    let needed = match kind {
        Paired::CovarianceS => 2,
        Paired::Steyx => 3,
        _ => 1,
    };
    if points.len() < needed {
        return Err(CellError::DivZero);
    }
    let n = points.len() as f64;
    let mean_y = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_x = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (y, x) in points {
        let (dy, dx) = (y - mean_y, x - mean_x);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let flat = |s: f64| {
        if s == 0.0 {
            Err(CellError::DivZero)
        } else {
            Ok(s)
        }
    };
    Ok(match kind {
        Paired::CovarianceP => sxy / n,
        Paired::CovarianceS => sxy / (n - 1.0),
        Paired::Correl => sxy / (flat(sxx)? * flat(syy)?).sqrt(),
        Paired::Rsq => sxy * sxy / (flat(sxx)? * flat(syy)?),
        Paired::Slope => sxy / flat(sxx)?,
        Paired::Intercept => mean_y - sxy / flat(sxx)? * mean_x,
        Paired::Steyx => ((syy - sxy * sxy / flat(sxx)?) / (n - 2.0)).max(0.0).sqrt(),
    })
}

/// AVERAGEA, MAXA and MINA.
#[derive(Clone, Copy)]
enum AVariant {
    Average,
    Max,
    Min,
}

/// The annuity functions sharing PMT's argument checks.
#[derive(Clone, Copy)]
enum Annuity {
    Pmt,
    Fv,
    Pv,
    Nper,
}

/// 1 + rate for payments at the start of each period, else 1.
fn due_factor(rate: f64, due: bool) -> f64 {
    if due { 1.0 + rate } else { 1.0 }
}

fn annuity_pmt(rate: f64, nper: f64, pv: f64, fv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + fv) / nper;
    }
    let growth = (1.0 + rate).powf(nper);
    -rate * (pv * growth + fv) / (due_factor(rate, due) * (growth - 1.0))
}

fn annuity_fv(rate: f64, nper: f64, pmt: f64, pv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    let growth = (1.0 + rate).powf(nper);
    -(pv * growth + pmt * due_factor(rate, due) * (growth - 1.0) / rate)
}

fn annuity_pv(rate: f64, nper: f64, pmt: f64, fv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(fv + pmt * nper);
    }
    let growth = (1.0 + rate).powf(nper);
    -(fv + pmt * due_factor(rate, due) * (growth - 1.0) / rate) / growth
}

/// Periods to pay off; `None` when the payment never gets there.
fn annuity_nper(rate: f64, pmt: f64, pv: f64, fv: f64, due: bool) -> Option<f64> {
    if rate == 0.0 {
        return (pmt != 0.0).then(|| -(pv + fv) / pmt);
    }
    let payment = pmt * due_factor(rate, due);
    let ratio = (payment - fv * rate) / (payment + pv * rate);
    (ratio > 0.0).then(|| ratio.ln() / (1.0 + rate).ln())
}

/// Interest in period `per`: the rate on the balance the period starts
/// with. Payments at the start leave the first period no interest.
fn annuity_ipmt(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, due: bool) -> f64 {
    let payment = annuity_pmt(rate, nper, pv, fv, due);
    let balance = if per == 1.0 {
        if due { 0.0 } else { -pv }
    } else if due {
        annuity_fv(rate, per - 2.0, payment, pv, true) - payment
    } else {
        annuity_fv(rate, per - 1.0, payment, pv, false)
    };
    balance * rate
}

/// Values discounted at `rate`, the first `first` periods out.
fn npv(rate: f64, values: &[f64], first: i32) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| v / (1.0 + rate).powi(i as i32 + first))
        .sum()
}

/// Dated cash flows discounted to the first date on a 365-day year.
fn xnpv(rate: f64, flows: &[(f64, f64)]) -> f64 {
    let start = flows.first().map_or(0.0, |f| f.1);
    flows
        .iter()
        .map(|(v, date)| v / (1.0 + rate).powf((date - start) / 365.0))
        .sum()
}

/// A root of `f` near `guess`, for RATE, IRR and XIRR: Newton's method,
/// then bisection across the sign change nearest the guess when Newton
/// fails to settle. Rates stay above -100%.
fn solve_rate(f: impl Fn(f64) -> f64, guess: f64) -> Option<f64> {
    const TOLERANCE: f64 = 1e-10;
    let mut r = guess.max(-0.99);
    // A settled step must also leave little of the starting imbalance.
    let residual = 1e-9 * f(r).abs().max(1.0);
    for _ in 0..100 {
        let y = f(r);
        let h = 1e-6 * (1.0 + r.abs());
        let slope = (f(r + h) - f(r - h)) / (2.0 * h);
        if !y.is_finite() || !slope.is_finite() || slope == 0.0 {
            break;
        }
        let next = r - y / slope;
        if next <= -1.0 {
            r = (r - 1.0) / 2.0;
            continue;
        }
        if (next - r).abs() <= TOLERANCE * next.abs().max(1.0) {
            if f(next).abs() <= residual {
                return Some(next);
            }
            break;
        }
        r = next;
    }

    let mut points = vec![
        -0.999, -0.99, -0.9, -0.75, -0.5, -0.25, -0.1, -0.01, 0.0, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0,
        2.0, 5.0, 10.0, 100.0, 1000.0,
    ];
    if guess > -1.0 && guess.is_finite() {
        points.push(guess);
    }
    points.sort_by(f64::total_cmp);
    let distance = |(a, b): &(f64, f64)| (a.max(guess.min(*b)) - guess).abs();
    let (mut lo, mut hi) = points
        .windows(2)
        .map(|w| (w[0], w[1]))
        .filter(|(a, b)| {
            let (fa, fb) = (f(*a), f(*b));
            fa.is_finite() && fb.is_finite() && (fa <= 0.0) != (fb <= 0.0)
        })
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
    let rising = f(hi) > f(lo);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if (f(mid) > 0.0) == rising {
            hi = mid;
        } else {
            lo = mid;
        }
        if hi - lo <= TOLERANCE * mid.abs().max(1.0) {
            break;
        }
    }
    Some((lo + hi) / 2.0)
}

impl Default for BuiltinFunctions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::engine::CellValueInput;
    use crate::cell::{CellCoord, CellError};

    #[test]
    fn test_sum() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(2.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(3.0));
        engine
            .set_formula(0, CellCoord::new(3, 0), "=SUM(A1:A3)")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Value(6.0)
        );
    }

    #[test]
    fn test_if() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine
            .set_formula(0, CellCoord::new(0, 1), "=IF(A1>5,\"big\",\"small\")")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Text("big".to_string())
        );
    }

    #[test]
    fn test_average() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(20.0));
        engine
            .set_formula(0, CellCoord::new(2, 0), "=AVERAGE(A1:A2)")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(15.0)
        );
    }

    #[test]
    fn test_sum_cross_sheet_range() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);
        engine.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(4.0));
        engine.set_value(1, CellCoord::new(1, 0), CellValueInput::Number(6.0));
        engine
            .set_formula(0, CellCoord::new(0, 0), "=SUM(Sheet2!A1:A2)")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(10.0)
        );
    }

    #[test]
    fn blanks_are_skipped_in_aggregates() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(20.0));
        engine
            .set_formula(0, CellCoord::new(0, 1), "=AVERAGE(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 1), "=COUNT(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 1), "=PRODUCT(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 1), "=MIN(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 1), "=COUNTIF(A1:A3,\">=0\")")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(15.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 1)),
            CellResult::Value(2.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 1)),
            CellResult::Value(200.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 1)),
            CellResult::Value(10.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 1)),
            CellResult::Value(2.0)
        );
    }

    #[test]
    fn excel_mod_follows_divisor_sign() {
        let mut engine = CalcEngine::new();
        engine
            .set_formula(0, CellCoord::new(0, 0), "=MOD(-3,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=MOD(3,-2)")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(1.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(-1.0)
        );
    }

    #[test]
    fn ceiling_floor_follow_excel_2010_signs() {
        let mut e = CalcEngine::new();
        // Microsoft's examples for CEILING and FLOOR, then the rest of the
        // sign matrix. Excel 2007 gave #NUM! for a negative number with a
        // positive significance; 2010 on rounds it.
        check(
            &mut e,
            &[
                ("=CEILING(2.5,1)", num(3.0)),
                ("=CEILING(-2.5,-2)", num(-4.0)),
                ("=CEILING(-2.5,2)", num(-2.0)),
                ("=CEILING(1.5,0.1)", num(1.5)),
                ("=CEILING(0.234,0.01)", num(0.24)),
                ("=CEILING(2.5,-2)", err(CellError::Num)),
                ("=CEILING(-2.5,-1)", num(-3.0)),
                ("=CEILING(-0.5,1)", num(0.0)),
                ("=CEILING(-4,2)", num(-4.0)),
                ("=CEILING(2.5,0)", num(0.0)),
                ("=FLOOR(3.7,2)", num(2.0)),
                ("=FLOOR(-2.5,-2)", num(-2.0)),
                ("=FLOOR(2.5,-2)", err(CellError::Num)),
                ("=FLOOR(1.58,0.1)", num(1.5)),
                ("=FLOOR(0.234,0.01)", num(0.23)),
                ("=FLOOR(-2.5,2)", num(-4.0)),
                ("=FLOOR(-2.5,-1)", num(-2.0)),
                ("=FLOOR(-0.5,1)", num(-1.0)),
                ("=FLOOR(-4,2)", num(-4.0)),
                ("=FLOOR(2.5,0)", err(CellError::DivZero)),
                // .MATH ignores the significance's sign, as before.
                ("=CEILING.MATH(-2.5,-2)", num(-2.0)),
                ("=FLOOR.MATH(-2.5,-2)", num(-4.0)),
                ("=CEILING.MATH(2.5,-2)", num(4.0)),
                ("=FLOOR.MATH(2.5,-2)", num(2.0)),
            ],
        );
    }

    #[test]
    fn text_applies_number_and_date_formats() {
        let mut engine = CalcEngine::new();
        engine
            .set_formula(0, CellCoord::new(0, 0), "=TEXT(1234.5,\"0.00\")")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=TEXT(1234.5,\"#,##0.00\")")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=TEXT(0.5,\"0%\")")
            .unwrap();
        engine
            .set_formula(
                0,
                CellCoord::new(3, 0),
                "=TEXT(DATE(2024,8,18),\"yyyy-mm-dd\")",
            )
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Text("1234.50".into())
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Text("1,234.50".into())
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Text("50%".into())
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Text("2024-08-18".into())
        );
    }

    #[test]
    fn trunc_toward_zero() {
        let mut engine = CalcEngine::new();
        engine
            .set_formula(0, CellCoord::new(0, 0), "=TRUNC(-2.9)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=INT(-2.9)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=TRUNC(2.99,1)")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(-2.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(-3.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(2.9)
        );
    }

    #[test]
    fn sumifs_countifs_averageifs() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Text("a".into()));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Text("b".into()));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Text("a".into()));
        engine.set_value(0, CellCoord::new(0, 1), CellValueInput::Number(10.0));
        engine.set_value(0, CellCoord::new(1, 1), CellValueInput::Number(20.0));
        engine.set_value(0, CellCoord::new(2, 1), CellValueInput::Number(30.0));
        engine.set_value(0, CellCoord::new(0, 2), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(1, 2), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(2, 2), CellValueInput::Number(2.0));
        engine
            .set_formula(
                0,
                CellCoord::new(0, 3),
                "=SUMIFS(B1:B3,A1:A3,\"a\",C1:C3,1)",
            )
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 3), "=COUNTIFS(A1:A3,\"a\",C1:C3,1)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 3), "=AVERAGEIFS(B1:B3,A1:A3,\"a\")")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 3)),
            CellResult::Value(10.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 3)),
            CellResult::Value(1.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 3)),
            CellResult::Value(20.0)
        );
    }

    #[test]
    fn sumproduct_stdev_large_small() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(2.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(3.0));
        engine.set_value(0, CellCoord::new(0, 1), CellValueInput::Number(4.0));
        engine.set_value(0, CellCoord::new(1, 1), CellValueInput::Number(5.0));
        engine.set_value(0, CellCoord::new(2, 1), CellValueInput::Number(6.0));
        engine
            .set_formula(0, CellCoord::new(0, 2), "=SUMPRODUCT(A1:A3,B1:B3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 2), "=STDEV(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 2), "=VAR.S(A1:A3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 2), "=LARGE(A1:A3,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 2), "=SMALL(A1:A3,2)")
            .unwrap();
        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 2)),
            CellResult::Value(32.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 2)),
            CellResult::Value(1.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 2)),
            CellResult::Value(1.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 2)),
            CellResult::Value(2.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 2)),
            CellResult::Value(2.0)
        );
    }

    #[test]
    fn mod_edge_cases() {
        let mut engine = CalcEngine::new();
        // MOD(n, d) = n - d * FLOOR(n/d)
        // Excel: result has same sign as divisor
        engine
            .set_formula(0, CellCoord::new(0, 0), "=MOD(5,3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=MOD(-5,3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=MOD(5,-3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 0), "=MOD(-5,-3)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 0), "=MOD(0,5)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(5, 0), "=MOD(5,0)")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(2.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(1.0)
        ); // -5 - 3*floor(-5/3) = -5 - 3*(-2) = 1
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(-1.0)
        ); // 5 - (-3)*floor(5/-3) = 5 - (-3)*(-2) = -1
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Value(-2.0)
        ); // -5 - (-3)*floor(-5/-3) = -5 - (-3)*1 = -2
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 0)),
            CellResult::Value(0.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(5, 0)),
            CellResult::Error(CellError::DivZero)
        );
    }

    #[test]
    fn ceiling_edge_cases() {
        let mut engine = CalcEngine::new();
        // CEILING rounds away from zero to a multiple of significance
        engine
            .set_formula(0, CellCoord::new(0, 0), "=CEILING(4.2,1)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=CEILING(4.2,0.5)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=CEILING(-4.2,-1)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 0), "=CEILING(0,5)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 0), "=CEILING(5,0)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(5, 0), "=CEILING(4.2,-1)")
            .unwrap(); // Mixed signs = error

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(5.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(4.5)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(-5.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Value(0.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 0)),
            CellResult::Value(0.0)
        ); // CEILING(x, 0) = 0
        assert_eq!(
            engine.get_value(0, CellCoord::new(5, 0)),
            CellResult::Error(CellError::Num)
        );
    }

    #[test]
    fn floor_edge_cases() {
        let mut engine = CalcEngine::new();
        // FLOOR rounds toward zero to a multiple of significance
        engine
            .set_formula(0, CellCoord::new(0, 0), "=FLOOR(4.7,1)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=FLOOR(4.7,0.5)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=FLOOR(-4.7,-1)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 0), "=FLOOR(0,5)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 0), "=FLOOR(4.7,-1)")
            .unwrap(); // Mixed signs = error

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(4.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(4.5)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(-4.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Value(0.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 0)),
            CellResult::Error(CellError::Num)
        );
    }

    #[test]
    fn aggregates_skip_text_in_ranges() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(10.0));
        engine.set_value(
            0,
            CellCoord::new(1, 0),
            CellValueInput::Text("ignored".into()),
        );
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(20.0));
        engine.set_value(0, CellCoord::new(3, 0), CellValueInput::Bool(true)); // Bools in ranges are skipped too

        engine
            .set_formula(0, CellCoord::new(0, 1), "=SUM(A1:A4)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 1), "=AVERAGE(A1:A4)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 1), "=COUNT(A1:A4)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 1), "=COUNTA(A1:A4)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 1), "=MAX(A1:A4)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(5, 1), "=MIN(A1:A4)")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(30.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 1)),
            CellResult::Value(15.0)
        ); // 30/2 numbers
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 1)),
            CellResult::Value(2.0)
        ); // Only numbers
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 1)),
            CellResult::Value(4.0)
        ); // All non-empty
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 1)),
            CellResult::Value(20.0)
        );
        assert_eq!(
            engine.get_value(0, CellCoord::new(5, 1)),
            CellResult::Value(10.0)
        );
    }

    #[test]
    fn cross_sheet_range_aggregates() {
        let mut engine = CalcEngine::new();
        engine.set_sheet_names(vec!["Data".into(), "Summary".into()]);

        // Set up data on "Data" sheet (index 0)
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(100.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(200.0));
        engine.set_value(0, CellCoord::new(2, 0), CellValueInput::Number(300.0));

        // Formulas on "Summary" sheet (index 1) referencing "Data"
        engine
            .set_formula(1, CellCoord::new(0, 0), "=SUM(Data!A1:A3)")
            .unwrap();
        engine
            .set_formula(1, CellCoord::new(1, 0), "=AVERAGE(Data!A1:A3)")
            .unwrap();
        engine
            .set_formula(1, CellCoord::new(2, 0), "=MAX(Data!A1:A3)")
            .unwrap();

        assert_eq!(
            engine.get_value(1, CellCoord::new(0, 0)),
            CellResult::Value(600.0)
        );
        assert_eq!(
            engine.get_value(1, CellCoord::new(1, 0)),
            CellResult::Value(200.0)
        );
        assert_eq!(
            engine.get_value(1, CellCoord::new(2, 0)),
            CellResult::Value(300.0)
        );
    }

    #[test]
    fn wildcard_countif_sumif() {
        let mut engine = CalcEngine::new();
        engine.set_value(
            0,
            CellCoord::new(0, 0),
            CellValueInput::Text("Apple".into()),
        );
        engine.set_value(
            0,
            CellCoord::new(1, 0),
            CellValueInput::Text("Apricot".into()),
        );
        engine.set_value(
            0,
            CellCoord::new(2, 0),
            CellValueInput::Text("Banana".into()),
        );
        engine.set_value(0, CellCoord::new(3, 0), CellValueInput::Text("app".into()));
        engine.set_value(0, CellCoord::new(0, 1), CellValueInput::Number(1.0));
        engine.set_value(0, CellCoord::new(1, 1), CellValueInput::Number(2.0));
        engine.set_value(0, CellCoord::new(2, 1), CellValueInput::Number(3.0));
        engine.set_value(0, CellCoord::new(3, 1), CellValueInput::Number(4.0));

        // Wildcard * matches any characters
        engine
            .set_formula(0, CellCoord::new(0, 2), "=COUNTIF(A1:A4,\"A*\")")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 2), "=SUMIF(A1:A4,\"A*\",B1:B4)")
            .unwrap();
        // Wildcard ? matches single character
        engine
            .set_formula(0, CellCoord::new(2, 2), "=COUNTIF(A1:A4,\"App?e\")")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 2)),
            CellResult::Value(3.0)
        ); // Apple, Apricot, app
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 2)),
            CellResult::Value(7.0)
        ); // 1+2+4
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 2)),
            CellResult::Value(1.0)
        ); // Apple
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14 is a rounding result, not PI
    fn roundup_rounddown_negative() {
        let mut engine = CalcEngine::new();
        engine
            .set_formula(0, CellCoord::new(0, 0), "=ROUNDUP(3.14159,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(1, 0), "=ROUNDDOWN(3.14159,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(2, 0), "=ROUNDUP(-3.14159,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(3, 0), "=ROUNDDOWN(-3.14159,2)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(4, 0), "=ROUND(2.5,0)")
            .unwrap();
        engine
            .set_formula(0, CellCoord::new(5, 0), "=ROUND(-2.5,0)")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 0)),
            CellResult::Value(3.15)
        ); // Away from zero
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 0)),
            CellResult::Value(3.14)
        ); // Toward zero
        assert_eq!(
            engine.get_value(0, CellCoord::new(2, 0)),
            CellResult::Value(-3.15)
        ); // Away from zero (more negative)
        assert_eq!(
            engine.get_value(0, CellCoord::new(3, 0)),
            CellResult::Value(-3.14)
        ); // Toward zero
        assert_eq!(
            engine.get_value(0, CellCoord::new(4, 0)),
            CellResult::Value(3.0)
        ); // Standard rounding
        assert_eq!(
            engine.get_value(0, CellCoord::new(5, 0)),
            CellResult::Value(-3.0)
        ); // Standard rounding
    }

    #[test]
    fn nested_function_calls() {
        let mut engine = CalcEngine::new();
        engine.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(16.0));
        engine.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(9.0));

        // IF with nested function arguments
        engine
            .set_formula(
                0,
                CellCoord::new(0, 1),
                "=IF(SUM(A1:A2)>20,SQRT(A1),SQRT(A2))",
            )
            .unwrap();
        // Nested aggregates
        engine
            .set_formula(0, CellCoord::new(1, 1), "=SUM(SQRT(A1),SQRT(A2))")
            .unwrap();

        assert_eq!(
            engine.get_value(0, CellCoord::new(0, 1)),
            CellResult::Value(4.0)
        ); // 25>20, sqrt(16)
        assert_eq!(
            engine.get_value(0, CellCoord::new(1, 1)),
            CellResult::Value(7.0)
        ); // 4+3
    }

    // Expected values below are Excel's, most from Microsoft's own examples
    // for each function.

    /// Value of a one-off formula on a scratch cell, Z1000.
    fn eval(engine: &mut CalcEngine, formula: &str) -> CellResult {
        let at = CellCoord::new(999, 25);
        engine.set_formula(0, at, formula).unwrap();
        engine.get_value(0, at)
    }

    fn num(n: f64) -> CellResult {
        CellResult::Value(n)
    }

    fn text(s: &str) -> CellResult {
        CellResult::Text(s.into())
    }

    fn err(e: CellError) -> CellResult {
        CellResult::Error(e)
    }

    /// Put `values` down the column at `a1`.
    fn fill(engine: &mut CalcEngine, a1: &str, values: &[CellValueInput]) {
        let top = CellCoord::from_a1(a1).unwrap();
        for (i, v) in values.iter().enumerate() {
            engine.set_value(0, CellCoord::new(top.row + i as u32, top.col), v.clone());
        }
    }

    fn numbers(values: &[f64]) -> Vec<CellValueInput> {
        values.iter().map(|n| CellValueInput::Number(*n)).collect()
    }

    #[track_caller]
    fn check(engine: &mut CalcEngine, cases: &[(&str, CellResult)]) {
        for (formula, expected) in cases {
            assert_eq!(&eval(engine, formula), expected, "{formula}");
        }
    }

    #[track_caller]
    fn check_close(engine: &mut CalcEngine, tolerance: f64, cases: &[(&str, f64)]) {
        for (formula, expected) in cases {
            match eval(engine, formula) {
                CellResult::Value(n) => assert!(
                    (n - expected).abs() <= tolerance,
                    "{formula}: {n} is not within {tolerance} of {expected}"
                ),
                other => panic!("{formula}: expected {expected}, got {other:?}"),
            }
        }
    }

    #[test]
    fn date_carries_months_and_reads_short_years() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DATE(2024,0,1)", num(45261.0)),
                ("=DATE(2024,14,1)", num(45689.0)),
                ("=DATE(118,1,2)", num(43102.0)),
                ("=DATE(2024,2,29)", num(45351.0)),
                ("=DATE(10000,1,1)", err(CellError::Num)),
                ("=DATE(-1,1,1)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn time_and_its_parts() {
        let mut e = CalcEngine::new();
        check_close(
            &mut e,
            1e-12,
            &[
                ("=TIME(12,30,0)", 0.520_833_333_333_333_3),
                ("=TIME(27,0,0)", 0.125),
                ("=TIME(0,750,0)", 0.520_833_333_333_333_3),
                ("=TIME(1,-30,0)", 1.0 / 48.0),
            ],
        );
        check(
            &mut e,
            &[
                ("=TIME(0,-1,0)", err(CellError::Num)),
                ("=TIME(\"x\",0,0)", err(CellError::Value)),
                ("=HOUR(0.75)", num(18.0)),
                ("=HOUR(45000.75)", num(18.0)),
                ("=HOUR(\"6:45 PM\")", num(18.0)),
                ("=MINUTE(\"12:45:00 PM\")", num(45.0)),
                ("=SECOND(TIME(1,2,3))", num(3.0)),
                ("=MINUTE(TIME(10,30,15))", num(30.0)),
                ("=HOUR(-1)", err(CellError::Num)),
                ("=MINUTE(\"abc\")", err(CellError::Value)),
                ("=SECOND(1/0)", err(CellError::DivZero)),
            ],
        );
    }

    #[test]
    fn weekday_and_weeknum() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                // 2008-02-14 is a Thursday.
                ("=WEEKDAY(DATE(2008,2,14))", num(5.0)),
                ("=WEEKDAY(DATE(2008,2,14),2)", num(4.0)),
                ("=WEEKDAY(DATE(2008,2,14),3)", num(3.0)),
                ("=WEEKDAY(DATE(2008,2,14),16)", num(6.0)),
                ("=WEEKDAY(0)", num(7.0)),
                ("=WEEKDAY(DATE(2008,2,14),4)", err(CellError::Num)),
                ("=WEEKDAY(-1)", err(CellError::Num)),
                ("=WEEKNUM(DATE(2012,3,9))", num(10.0)),
                ("=WEEKNUM(DATE(2012,3,9),2)", num(11.0)),
                ("=WEEKNUM(DATE(2012,1,1),21)", num(52.0)),
                ("=WEEKNUM(DATE(2021,1,3),21)", num(53.0)),
                ("=WEEKNUM(DATE(2021,1,4),21)", num(1.0)),
                ("=WEEKNUM(DATE(2012,1,1),3)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn edate_and_eomonth() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=EDATE(DATE(2011,1,15),1)", num(40589.0)),
                ("=EDATE(DATE(2011,1,15),-1)", num(40527.0)),
                ("=EDATE(DATE(2011,1,15),2.9)", num(40617.0)),
                // Past the end of a short month: its last day.
                ("=EDATE(DATE(2024,1,31),1)", num(45351.0)),
                ("=EOMONTH(DATE(2011,1,1),1)", num(40602.0)),
                ("=EOMONTH(DATE(2011,1,1),-3)", num(40482.0)),
                ("=EDATE(-1,1)", err(CellError::Num)),
                ("=EOMONTH(DATE(9999,12,1),1)", err(CellError::Num)),
                ("=EDATE(\"x\",1)", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn datedif_units() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"Y\")", num(2.0)),
                ("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"D\")", num(440.0)),
                ("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"YD\")", num(75.0)),
                ("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"md\")", num(14.0)),
                ("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"M\")", num(14.0)),
                ("=DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"YM\")", num(2.0)),
                ("=DATEDIF(DATE(2020,2,29),DATE(2021,2,28),\"Y\")", num(0.0)),
                // Excel's MD quirk: counted from January 31st's overflow into
                // March, so negative.
                ("=DATEDIF(DATE(2015,1,31),DATE(2015,3,1),\"MD\")", num(-2.0)),
                (
                    "=DATEDIF(DATE(2003,1,1),DATE(2001,1,1),\"Y\")",
                    err(CellError::Num),
                ),
                (
                    "=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"W\")",
                    err(CellError::Num),
                ),
            ],
        );
    }

    #[test]
    fn datevalue_and_timevalue() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DATEVALUE(\"8/22/2011\")", num(40777.0)),
                ("=DATEVALUE(\"22-MAY-2011\")", num(40685.0)),
                ("=DATEVALUE(\"2011/02/23\")", num(40597.0)),
                ("=DATEVALUE(\"May 22, 2011\")", num(40685.0)),
                ("=DATEVALUE(\"2011-02-23 18:00\")", num(40597.0)),
                ("=TIMEVALUE(\"2011-02-23\")", num(0.0)),
                ("=DATEVALUE(40777)", err(CellError::Value)),
                ("=DATEVALUE(\"2011-02-30\")", err(CellError::Value)),
                ("=DATEVALUE(\"18:00\")", err(CellError::Value)),
                ("=TIMEVALUE(\"25:00\")", err(CellError::Value)),
            ],
        );
        check_close(
            &mut e,
            1e-9,
            &[
                ("=TIMEVALUE(\"2:24 AM\")", 0.1),
                ("=TIMEVALUE(\"22-Aug-2011 6:35 AM\")", 0.274_305_556),
            ],
        );
    }

    #[test]
    fn days_and_days360() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DAYS(\"15-MAR-2021\",\"1-FEB-2021\")", num(42.0)),
                ("=DAYS(DATE(2021,2,1),DATE(2021,3,15))", num(-42.0)),
                ("=DAYS(\"x\",1)", err(CellError::Value)),
                ("=DAYS360(DATE(2011,1,30),DATE(2011,12,31))", num(330.0)),
                ("=DAYS360(DATE(2011,1,15),DATE(2011,3,31))", num(76.0)),
                ("=DAYS360(DATE(2011,1,15),DATE(2011,3,31),TRUE)", num(75.0)),
                ("=DAYS360(DATE(2011,2,28),DATE(2011,3,31))", num(30.0)),
                ("=DAYS360(DATE(2011,3,31),DATE(2011,1,15))", num(-75.0)),
                ("=DAYS360(-1,1)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn networkdays_and_workday() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[41183.0, 41334.0]));
        fill(&mut e, "B1", &numbers(&[41235.0, 41247.0, 41295.0]));
        fill(&mut e, "C1", &numbers(&[39778.0, 39786.0, 39834.0]));
        check(
            &mut e,
            &[
                ("=NETWORKDAYS(A1,A2)", num(110.0)),
                ("=NETWORKDAYS(A1,A2,B1)", num(109.0)),
                ("=NETWORKDAYS(A1,A2,B1:B3)", num(107.0)),
                ("=NETWORKDAYS(A1,A2,B:B)", num(107.0)),
                ("=NETWORKDAYS(A2,A1,B1:B3)", num(-107.0)),
                ("=NETWORKDAYS(A1,A2,\"x\")", err(CellError::Value)),
                ("=WORKDAY(DATE(2008,10,1),151)", num(39933.0)),
                ("=WORKDAY(DATE(2008,10,1),151,C1:C3)", num(39938.0)),
                ("=WORKDAY(DATE(2008,10,1),0)", num(39722.0)),
                ("=WORKDAY(DATE(2024,1,8),-1)", num(45296.0)),
                ("=WORKDAY(DATE(9999,12,30),5)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn yearfrac_bases() {
        let mut e = CalcEngine::new();
        check_close(
            &mut e,
            1e-8,
            &[
                ("=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30))", 0.580_555_556),
                ("=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),1)", 0.576_502_732),
                ("=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),2)", 211.0 / 360.0),
                ("=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),3)", 0.578_082_192),
                ("=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),4)", 0.580_555_556),
                ("=YEARFRAC(DATE(2012,7,30),DATE(2012,1,1))", 0.580_555_556),
                // Spanning years, basis 1 averages their lengths.
                ("=YEARFRAC(DATE(2010,1,1),DATE(2012,7,1),1)", 2.496_350_365),
            ],
        );
        check(
            &mut e,
            &[(
                "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),5)",
                err(CellError::Num),
            )],
        );
    }

    #[test]
    fn annuity_functions() {
        let mut e = CalcEngine::new();
        check_close(
            &mut e,
            0.005,
            &[
                ("=PMT(0.05/12,360,200000)", -1073.64),
                ("=PMT(8%/12,10,10000)", -1037.03),
                ("=PMT(8%/12,10,10000,0,1)", -1030.16),
                ("=PMT(0,10,1000)", -100.0),
                ("=FV(6%/12,10,-200,-500,1)", 2581.40),
                ("=FV(12%/12,12,-1000)", 12682.50),
                ("=FV(0,12,-100,-1000)", 2200.0),
                ("=PV(8%/12,12*20,500,0,0)", -59777.15),
                ("=IPMT(10%/12,1,3*12,8000)", -66.67),
                ("=IPMT(10%,3,3,8000)", -292.45),
                ("=PPMT(10%/12,1,2*12,2000)", -75.62),
                ("=PPMT(8%,10,10,200000)", -27598.05),
                ("=CUMIPMT(9%/12,30*12,125000,1,1,0)", -937.50),
            ],
        );
        check_close(
            &mut e,
            1e-6,
            &[
                ("=NPER(12%/12,-100,-1000,10000,1)", 59.673_865_7),
                ("=NPER(1%,-100,-1000,10000)", 60.082_122_9),
                ("=NPER(1%,-100,-1000)", -9.578_594_04),
                ("=CUMIPMT(9%/12,30*12,125000,13,24,0)", -11_135.232_13),
                ("=CUMPRINC(9%/12,30*12,125000,13,24,0)", -934.107_123_4),
                ("=CUMPRINC(9%/12,30*12,125000,1,1,0)", -68.278_271_18),
            ],
        );
        check(
            &mut e,
            &[
                ("=PMT(0.1,0,1000)", err(CellError::Num)),
                ("=PMT(\"x\",10,1000)", err(CellError::Value)),
                ("=PMT(0.1,10)", err(CellError::Value)),
                ("=IPMT(0.1,0,3,8000)", err(CellError::Num)),
                ("=PPMT(0.1,4,3,8000)", err(CellError::Num)),
                ("=NPER(1%,0,1000)", err(CellError::Num)),
                ("=CUMIPMT(9%/12,360,125000,1,1,2)", err(CellError::Num)),
                ("=CUMPRINC(9%/12,360,125000,5,4,0)", err(CellError::Num)),
                // Payments at the start of each period earn no interest at first.
                ("=IPMT(0.1,1,3,8000,0,1)", num(0.0)),
            ],
        );
    }

    #[test]
    fn rate_irr_and_npv() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &numbers(&[-70000.0, 12000.0, 15000.0, 18000.0, 21000.0, 26000.0]),
        );
        fill(
            &mut e,
            "B1",
            &numbers(&[-120000.0, 39000.0, 30000.0, 21000.0, 37000.0, 46000.0]),
        );
        check_close(
            &mut e,
            1e-8,
            &[
                ("=RATE(4*12,-200,8000)", 0.007_701_472_49),
                ("=RATE(4*12,-200,8000)*12", 0.092_417_669_9),
                ("=IRR(A1:A5)", -0.021_244_848),
                ("=IRR(A1:A6)", 0.086_630_948),
                ("=IRR(A1:A3,-10%)", -0.443_506_941),
                ("=MIRR(B1:B6,10%,12%)", 0.126_094_130),
                ("=MIRR(B1:B4,10%,12%)", -0.048_044_655),
                ("=MIRR(B1:B6,10%,14%)", 0.134_759_111),
            ],
        );
        check_close(
            &mut e,
            0.005,
            &[
                ("=NPV(10%,-10000,3000,4200,6800)", 1188.44),
                ("=NPV(8%,8000,9200,10000,12000,14500)-40000", 1922.06),
            ],
        );
        check(
            &mut e,
            &[
                // All inflows: no rate balances them.
                ("=RATE(10,100,100)", err(CellError::Num)),
                ("=IRR(A2:A6)", err(CellError::Num)),
                ("=IRR(5)", err(CellError::Num)),
                ("=NPV(-1,100)", err(CellError::DivZero)),
                ("=NPV(10%,1/0)", err(CellError::DivZero)),
                ("=MIRR(A2:A6,10%,12%)", err(CellError::DivZero)),
            ],
        );
    }

    #[test]
    fn xnpv_and_xirr() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &numbers(&[-10000.0, 2750.0, 4250.0, 3250.0, 2750.0]),
        );
        fill(
            &mut e,
            "B1",
            &numbers(&[39448.0, 39508.0, 39751.0, 39859.0, 39904.0]),
        );
        check_close(
            &mut e,
            1e-6,
            &[
                ("=XNPV(0.09,A1:A5,B1:B5)", 2_086.647_602),
                ("=XIRR(A1:A5,B1:B5)", 0.373_362_535),
            ],
        );
        e.set_value(0, CellCoord::new(5, 1), CellValueInput::Number(39000.0));
        e.set_value(0, CellCoord::new(5, 0), CellValueInput::Number(1.0));
        check(
            &mut e,
            &[
                ("=XNPV(0.09,A1:A5,B1:B4)", err(CellError::Num)),
                // B6 comes before B1.
                ("=XNPV(0.09,A1:A6,B1:B6)", err(CellError::Num)),
                ("=XIRR(A2:A5,B2:B5)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn depreciation() {
        let mut e = CalcEngine::new();
        check_close(
            &mut e,
            0.005,
            &[
                ("=SLN(30000,7500,10)", 2250.0),
                ("=SYD(30000,7500,10,1)", 4090.91),
                ("=SYD(30000,7500,10,10)", 409.09),
                ("=DB(1000000,100000,6,1,7)", 186_083.33),
                ("=DB(1000000,100000,6,2,7)", 259_639.42),
                ("=DB(1000000,100000,6,6,7)", 55_841.76),
                ("=DB(1000000,100000,6,7,7)", 15_845.10),
                ("=DDB(2400,300,10*12,1,2)", 40.0),
                ("=DDB(2400,300,10,1,2)", 480.0),
                ("=DDB(2400,300,10,2,1.5)", 306.0),
                ("=DDB(2400,300,10,10)", 22.12),
            ],
        );
        check_close(&mut e, 1e-6, &[("=DDB(2400,300,10*365,1)", 1.315_068_493)]);
        check(
            &mut e,
            &[
                ("=SLN(30000,7500,0)", err(CellError::DivZero)),
                ("=SYD(30000,7500,10,11)", err(CellError::Num)),
                ("=DB(1000000,100000,6,8,7)", err(CellError::Num)),
                ("=DB(1000000,100000,6,1,13)", err(CellError::Num)),
                ("=DDB(2400,300,10,11)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn hyperbolic_and_angle_functions() {
        let mut e = CalcEngine::new();
        check_close(
            &mut e,
            1e-8,
            &[
                ("=ATAN2(1,1)", std::f64::consts::FRAC_PI_4),
                ("=ATAN2(-1,-1)", -2.356_194_490),
                ("=SINH(1)", 1.175_201_194),
                ("=COSH(4)", 27.308_232_836),
                ("=TANH(0.5)", 0.462_117_157),
                ("=ASINH(-2.5)", -1.647_231_146),
                ("=ACOSH(10)", 2.993_222_846),
                ("=ATANH(0.76159416)", 1.000_000_01),
                ("=DEGREES(PI())", 180.0),
                ("=RADIANS(270)", 4.712_388_980),
                ("=SQRTPI(1)", 1.772_453_851),
                ("=SQRTPI(2)", 2.506_628_275),
            ],
        );
        check(
            &mut e,
            &[
                ("=ATAN2(0,0)", err(CellError::DivZero)),
                ("=ACOSH(0.5)", err(CellError::Num)),
                ("=ATANH(1)", err(CellError::Num)),
                ("=SQRTPI(-1)", err(CellError::Num)),
                ("=SINH(1000)", err(CellError::Num)),
                ("=COSH(\"x\")", err(CellError::Value)),
                ("=TANH(1/0)", err(CellError::DivZero)),
            ],
        );
    }

    #[test]
    fn factorials_combinations_and_divisors() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[24.0, 36.0]));
        check(
            &mut e,
            &[
                ("=SUMSQ(3,4)", num(25.0)),
                ("=SUMSQ(A1:A2,1)", num(1873.0)),
                ("=SUMSQ(1,1/0)", err(CellError::DivZero)),
                ("=FACT(5)", num(120.0)),
                ("=FACT(1.9)", num(1.0)),
                ("=FACT(0)", num(1.0)),
                ("=FACT(-1)", err(CellError::Num)),
                ("=FACT(171)", err(CellError::Num)),
                ("=FACTDOUBLE(6)", num(48.0)),
                ("=FACTDOUBLE(7)", num(105.0)),
                ("=FACTDOUBLE(-1)", err(CellError::Num)),
                ("=COMBIN(8,2)", num(28.0)),
                ("=COMBIN(2,3)", err(CellError::Num)),
                ("=PERMUT(100,3)", num(970_200.0)),
                ("=PERMUT(3,2)", num(6.0)),
                ("=PERMUT(2,3)", err(CellError::Num)),
                ("=GCD(5,2)", num(1.0)),
                ("=GCD(A1:A2)", num(12.0)),
                ("=GCD(5,0)", num(5.0)),
                ("=GCD(-1,2)", err(CellError::Num)),
                ("=LCM(5,2)", num(10.0)),
                ("=LCM(A1:A2)", num(72.0)),
                ("=LCM(3,-1)", err(CellError::Num)),
                ("=QUOTIENT(5,2)", num(2.0)),
                ("=QUOTIENT(4.5,3.1)", num(1.0)),
                ("=QUOTIENT(-10,3)", num(-3.0)),
                ("=QUOTIENT(1,0)", err(CellError::DivZero)),
            ],
        );
    }

    #[test]
    fn rounding_to_multiples() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=MROUND(10,3)", num(9.0)),
                ("=MROUND(-10,-3)", num(-9.0)),
                ("=MROUND(1.3,0.2)", num(1.4)),
                ("=MROUND(5,2)", num(6.0)),
                ("=MROUND(5,-2)", err(CellError::Num)),
                ("=EVEN(1.5)", num(2.0)),
                ("=EVEN(3)", num(4.0)),
                ("=EVEN(2)", num(2.0)),
                ("=EVEN(-1)", num(-2.0)),
                ("=ODD(1.5)", num(3.0)),
                ("=ODD(3)", num(3.0)),
                ("=ODD(2)", num(3.0)),
                ("=ODD(-1)", num(-1.0)),
                ("=ODD(-2)", num(-3.0)),
                ("=ODD(0)", num(1.0)),
                ("=EVEN(\"x\")", err(CellError::Value)),
                ("=CEILING.MATH(24.3,5)", num(25.0)),
                ("=CEILING.MATH(6.7)", num(7.0)),
                ("=CEILING.MATH(-8.1,2)", num(-8.0)),
                ("=CEILING.MATH(-5.5,2,-1)", num(-6.0)),
                ("=CEILING.MATH(5,0)", num(0.0)),
                ("=FLOOR.MATH(24.3,5)", num(20.0)),
                ("=FLOOR.MATH(6.7)", num(6.0)),
                ("=FLOOR.MATH(-8.1,2)", num(-10.0)),
                ("=FLOOR.MATH(-5.5,2,-1)", num(-4.0)),
                ("=FLOOR.MATH(\"x\")", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn numerals_and_bases() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=ROMAN(499,0)", text("CDXCIX")),
                ("=ROMAN(499,1)", text("LDVLIV")),
                ("=ROMAN(499,2)", text("XDIX")),
                ("=ROMAN(499,3)", text("VDIV")),
                ("=ROMAN(499,4)", text("ID")),
                ("=ROMAN(499,FALSE)", text("ID")),
                ("=ROMAN(2013,0)", text("MMXIII")),
                ("=ROMAN(3999)", text("MMMCMXCIX")),
                ("=ROMAN(0)", text("")),
                ("=ROMAN(4000)", err(CellError::Value)),
                ("=ROMAN(1,5)", err(CellError::Value)),
                ("=ARABIC(\"LVII\")", num(57.0)),
                ("=ARABIC(\"mcmxii\")", num(1912.0)),
                ("=ARABIC(\"LDVLIV\")", num(499.0)),
                ("=ARABIC(\"-MMXI\")", num(-2011.0)),
                ("=ARABIC(\"\")", num(0.0)),
                ("=ARABIC(\"ABC\")", err(CellError::Value)),
                ("=BASE(7,2)", text("111")),
                ("=BASE(100,16)", text("64")),
                ("=BASE(15,2,10)", text("0000001111")),
                ("=BASE(-1,2)", err(CellError::Num)),
                ("=BASE(7,37)", err(CellError::Num)),
                ("=DECIMAL(\"FF\",16)", num(255.0)),
                ("=DECIMAL(111,2)", num(7.0)),
                ("=DECIMAL(\"zap\",36)", num(45745.0)),
                ("=DECIMAL(\"12\",2)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn mode_percentiles_and_quartiles() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[5.6, 4.0, 4.0, 3.0, 2.0, 4.0]));
        fill(&mut e, "B1", &numbers(&[1.0, 3.0, 2.0, 4.0]));
        fill(
            &mut e,
            "C1",
            &numbers(&[1.0, 2.0, 3.0, 6.0, 6.0, 6.0, 7.0, 8.0, 9.0]),
        );
        fill(
            &mut e,
            "D1",
            &numbers(&[1.0, 2.0, 4.0, 7.0, 8.0, 9.0, 10.0, 12.0]),
        );
        fill(
            &mut e,
            "E1",
            &numbers(&[
                6.0, 7.0, 15.0, 36.0, 39.0, 40.0, 41.0, 42.0, 43.0, 47.0, 49.0,
            ]),
        );
        check(
            &mut e,
            &[
                ("=MODE(A1:A6)", num(4.0)),
                ("=MODE.SNGL(1,2,2,1)", num(1.0)),
                ("=MODE(1,2,3)", err(CellError::NA)),
                ("=PERCENTILE.EXC(C1:C9,0.25)", num(2.5)),
                ("=PERCENTILE.EXC(C1:C9,0)", err(CellError::Num)),
                ("=PERCENTILE.EXC(C1:C9,0.01)", err(CellError::Num)),
                ("=PERCENTILE(B1:B4,1.5)", err(CellError::Num)),
                ("=PERCENTILE.INC(B1:B4,1)", num(4.0)),
                ("=QUARTILE(D1:D8,1)", num(3.5)),
                ("=QUARTILE.INC(D1:D8,4)", num(12.0)),
                ("=QUARTILE.EXC(E1:E11,1)", num(15.0)),
                ("=QUARTILE.EXC(E1:E11,3)", num(43.0)),
                ("=QUARTILE(D1:D8,5)", err(CellError::Num)),
                ("=QUARTILE.EXC(E1:E11,0)", err(CellError::Num)),
                ("=PERCENTILE(Q1:Q5,0.5)", err(CellError::Num)),
            ],
        );
        check_close(&mut e, 1e-12, &[("=PERCENTILE(B1:B4,0.3)", 1.9)]);
    }

    #[test]
    fn ranks() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[7.0, 3.5, 3.5, 1.0, 2.0]));
        check(
            &mut e,
            &[
                ("=RANK(A3,A1:A5,1)", num(3.0)),
                ("=RANK(A1,A1:A5,1)", num(5.0)),
                ("=RANK.EQ(7,A1:A5)", num(1.0)),
                ("=RANK.EQ(3.5,A1:A5)", num(2.0)),
                ("=RANK.AVG(3.5,A1:A5)", num(2.5)),
                ("=RANK.AVG(3.5,A1:A5,1)", num(3.5)),
                ("=RANK(9,A1:A5)", err(CellError::NA)),
                ("=RANK(1,5)", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn means_and_deviations() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &numbers(&[4.0, 5.0, 8.0, 7.0, 11.0, 4.0, 3.0]),
        );
        fill(&mut e, "B1", &numbers(&[4.0, 5.0, 6.0, 7.0, 5.0, 4.0, 3.0]));
        fill(
            &mut e,
            "C1",
            &numbers(&[4.0, 5.0, 6.0, 7.0, 2.0, 3.0, 4.0, 5.0, 1.0, 2.0, 3.0]),
        );
        check_close(
            &mut e,
            1e-8,
            &[
                ("=GEOMEAN(A1:A7)", 5.476_986_970),
                ("=HARMEAN(A1:A7)", 5.028_375_962),
                ("=AVEDEV(B1:B7)", 1.020_408_163),
                ("=DEVSQ(A1:A7)", 48.0),
                ("=TRIMMEAN(C1:C11,0.2)", 3.777_777_778),
                ("=STANDARDIZE(42,40,1.5)", 1.333_333_333),
            ],
        );
        check(
            &mut e,
            &[
                ("=GEOMEAN(1,-1)", err(CellError::Num)),
                ("=HARMEAN(0,1)", err(CellError::Num)),
                ("=AVEDEV(Q1:Q3)", err(CellError::Num)),
                ("=DEVSQ(1,1/0)", err(CellError::DivZero)),
                ("=TRIMMEAN(C1:C11,1)", err(CellError::Num)),
                ("=STANDARDIZE(42,40,0)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn paired_statistics() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[3.0, 2.0, 4.0, 5.0, 6.0]));
        fill(&mut e, "B1", &numbers(&[9.0, 7.0, 12.0, 15.0, 17.0]));
        fill(&mut e, "C1", &numbers(&[2.0, 3.0, 9.0, 1.0, 8.0, 7.0, 5.0]));
        fill(
            &mut e,
            "D1",
            &numbers(&[6.0, 5.0, 11.0, 7.0, 5.0, 4.0, 4.0]),
        );
        fill(&mut e, "E1", &numbers(&[6.0, 7.0, 9.0, 15.0, 21.0]));
        fill(&mut e, "F1", &numbers(&[20.0, 28.0, 31.0, 38.0, 40.0]));
        fill(&mut e, "G1", &numbers(&[2.0, 4.0, 8.0]));
        fill(&mut e, "H1", &numbers(&[5.0, 11.0, 12.0]));
        check_close(
            &mut e,
            1e-6,
            &[
                ("=CORREL(A1:A5,B1:B5)", 0.997_054_486),
                ("=PEARSON(B1:B5,A1:A5)", 0.997_054_486),
                ("=COVARIANCE.P(A1:A5,B1:B5)", 5.2),
                ("=COVAR(A1:A5,B1:B5)", 5.2),
                ("=COVARIANCE.S(G1:G3,H1:H3)", 9.666_666_667),
                ("=SLOPE(C1:C7,D1:D7)", 0.305_555_556),
                ("=INTERCEPT(C1:C5,D1:D5)", 0.048_387_097),
                ("=RSQ(C1:C7,D1:D7)", 0.057_950_192),
                ("=STEYX(C1:C7,D1:D7)", 3.305_718_950),
                ("=FORECAST(30,E1:E5,F1:F5)", 10.607_253),
                ("=FORECAST.LINEAR(30,E1:E5,F1:F5)", 10.607_253),
                // Whole columns pair up row by row.
                ("=SLOPE(C:C,D:D)", 0.305_555_556),
            ],
        );
        e.set_value(0, CellCoord::new(0, 0), CellValueInput::Text("n/a".into()));
        check_close(&mut e, 1e-9, &[("=COVARIANCE.P(A1:A5,B1:B5)", 5.5625)]);
        check(
            &mut e,
            &[
                ("=CORREL(A1:A5,B1:B4)", err(CellError::NA)),
                ("=SLOPE(A1:A3,G5:G7)", err(CellError::DivZero)),
                ("=STEYX(G1:G2,H1:H2)", err(CellError::DivZero)),
                ("=COVARIANCE.S(1,2)", err(CellError::DivZero)),
            ],
        );
    }

    #[test]
    fn maxifs_minifs_and_a_variants() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &numbers(&[89.0, 93.0, 96.0, 85.0, 91.0, 88.0]),
        );
        fill(&mut e, "B1", &numbers(&[1.0, 2.0, 2.0, 3.0, 1.0, 1.0]));
        fill(
            &mut e,
            "C1",
            &[
                CellValueInput::Number(10.0),
                CellValueInput::Number(7.0),
                CellValueInput::Number(9.0),
                CellValueInput::Number(2.0),
                CellValueInput::Text("Not available".into()),
            ],
        );
        fill(
            &mut e,
            "D1",
            &[
                CellValueInput::Bool(false),
                CellValueInput::Number(0.2),
                CellValueInput::Number(0.5),
                CellValueInput::Number(0.4),
                CellValueInput::Number(0.8),
            ],
        );
        check(
            &mut e,
            &[
                ("=MAXIFS(A1:A6,B1:B6,1)", num(91.0)),
                ("=MINIFS(A1:A6,B1:B6,1)", num(88.0)),
                ("=MAXIFS(A1:A6,B1:B6,\">1\",A1:A6,\"<95\")", num(93.0)),
                ("=MAXIFS(A:A,B:B,2)", num(96.0)),
                ("=MINIFS(A1:A6,B1:B6,9)", num(0.0)),
                ("=MAXIFS(A1:A6,B1:B5,1)", err(CellError::Value)),
                ("=AVERAGEA(C1:C5)", num(5.6)),
                ("=AVERAGE(C1:C5)", num(7.0)),
                ("=MAXA(D1:D5)", num(0.8)),
                ("=MINA(D1:D5)", num(0.0)),
                ("=MAXA(D1:D5,TRUE)", num(1.0)),
                ("=MINA(\"-1\",D2)", num(-1.0)),
                ("=MAXA(Q1:Q3)", num(0.0)),
                ("=AVERAGEA(Q1:Q3)", err(CellError::DivZero)),
                ("=MAXA(\"x\")", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn joining_and_cleaning_text() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &[
                CellValueInput::Text("a".into()),
                CellValueInput::Text(String::new()),
                CellValueInput::Text("c".into()),
            ],
        );
        e.set_value(0, CellCoord::new(4, 0), CellValueInput::Text("e".into()));
        fill(
            &mut e,
            "B1",
            &[
                CellValueInput::Text("-".into()),
                CellValueInput::Text("+".into()),
            ],
        );
        check(
            &mut e,
            &[
                ("=TEXTJOIN(\", \",TRUE,A1:A5)", text("a, c, e")),
                ("=TEXTJOIN(\", \",FALSE,A1:A5)", text("a, , c, , e")),
                (
                    "=TEXTJOIN(\"-\",TRUE,1,\"b\",TRUE,1.5)",
                    text("1-b-TRUE-1.5"),
                ),
                (
                    "=TEXTJOIN(B1:B2,TRUE,\"a\",\"b\",\"c\",\"d\")",
                    text("a-b+c-d"),
                ),
                ("=TEXTJOIN(\"\",FALSE,A:A)", text("ace")),
                ("=TEXTJOIN(\",\",FALSE,A:A)", err(CellError::Value)),
                ("=TEXTJOIN(\",\",TRUE,1/0)", err(CellError::DivZero)),
                ("=TEXTJOIN(\",\",TRUE)", err(CellError::Value)),
                (
                    "=CLEAN(CHAR(9)&\"Monthly report\"&CHAR(10))",
                    text("Monthly report"),
                ),
                ("=T(\"Rainfall\")", text("Rainfall")),
                ("=T(19)", text("")),
                ("=T(TRUE)", text("")),
                ("=T(1/0)", err(CellError::DivZero)),
                ("=UNICHAR(66)", text("B")),
                ("=UNICHAR(32)", text(" ")),
                ("=UNICHAR(0)", err(CellError::Value)),
                ("=UNICHAR(55296)", err(CellError::NA)),
                ("=UNICODE(\" \")", num(32.0)),
                ("=UNICODE(\"B\")", num(66.0)),
                ("=UNICODE(\"\")", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn number_text_conversions() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DOLLAR(1234.567,2)", text("$1,234.57")),
                ("=DOLLAR(1234.567,-2)", text("$1,200")),
                ("=DOLLAR(-1234.567,-2)", text("($1,200)")),
                ("=DOLLAR(-0.123,4)", text("($0.1230)")),
                ("=DOLLAR(99.888)", text("$99.89")),
                ("=DOLLAR(1,128)", err(CellError::Value)),
                ("=FIXED(1234.567,1)", text("1,234.6")),
                ("=FIXED(1234.567,-1)", text("1,230")),
                ("=FIXED(-1234.567,-1,TRUE)", text("-1230")),
                ("=FIXED(44.332)", text("44.33")),
                ("=FIXED(1234.565,2)", text("1,234.57")),
                ("=FIXED(\"x\")", err(CellError::Value)),
                ("=NUMBERVALUE(\"2.500,27\",\",\",\".\")", num(2500.27)),
                ("=NUMBERVALUE(\"1 234.5\")", num(1234.5)),
                ("=NUMBERVALUE(\"\")", num(0.0)),
                ("=NUMBERVALUE(\"1.2.3\")", err(CellError::Value)),
                ("=NUMBERVALUE(\"1.5,2\")", err(CellError::Value)),
                ("=NUMBERVALUE(\"1,5\",\",\",\",\")", err(CellError::Value)),
            ],
        );
        check_close(&mut e, 1e-15, &[("=NUMBERVALUE(\"3.5%\")", 0.035)]);
    }

    #[test]
    fn text_before_and_after() {
        let mut e = CalcEngine::new();
        let s = "\"Little Red Riding Hood\"";
        let f = |name: &str, rest: &str| format!("={name}({s},{rest})");
        let cases = [
            (f("TEXTBEFORE", "\" \""), text("Little")),
            (f("TEXTBEFORE", "\" \",2"), text("Little Red")),
            (f("TEXTBEFORE", "\" \",-1"), text("Little Red Riding")),
            (f("TEXTAFTER", "\" \""), text("Red Riding Hood")),
            (f("TEXTAFTER", "\" \",-1"), text("Hood")),
            (f("TEXTBEFORE", "\"red\""), err(CellError::NA)),
            (f("TEXTBEFORE", "\"red\",1,1"), text("Little ")),
            (f("TEXTAFTER", "\"x\",1,0,0,\"none\""), text("none")),
            (
                f("TEXTBEFORE", "\"x\",1,0,1"),
                text("Little Red Riding Hood"),
            ),
            (
                f("TEXTAFTER", "\"x\",-1,0,1"),
                text("Little Red Riding Hood"),
            ),
            (f("TEXTBEFORE", "\" \",5"), err(CellError::NA)),
            (f("TEXTBEFORE", "\" \",0"), err(CellError::Value)),
            (f("TEXTBEFORE", "\" \",100"), err(CellError::Value)),
            (f("TEXTBEFORE", "\"\""), text("")),
            (f("TEXTAFTER", "\"\""), text("Little Red Riding Hood")),
        ];
        for (formula, expected) in cases {
            assert_eq!(eval(&mut e, &formula), expected, "{formula}");
        }
    }

    #[test]
    fn lookup_vector_and_array_forms() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[4.14, 4.19, 5.17, 5.77, 6.39]));
        let colors = ["red", "orange", "yellow", "green", "blue"];
        fill(
            &mut e,
            "B1",
            &colors
                .iter()
                .map(|c| CellValueInput::Text(c.to_string()))
                .collect::<Vec<_>>(),
        );
        // A wide array: letters over numbers.
        for (i, (letter, n)) in [("a", 1.0), ("b", 2.0), ("c", 3.0), ("d", 4.0)]
            .into_iter()
            .enumerate()
        {
            let col = 3 + i as u32;
            e.set_value(
                0,
                CellCoord::new(0, col),
                CellValueInput::Text(letter.into()),
            );
            e.set_value(0, CellCoord::new(1, col), CellValueInput::Number(n));
        }
        check(
            &mut e,
            &[
                ("=LOOKUP(4.19,A1:A5,B1:B5)", text("orange")),
                ("=LOOKUP(5.75,A1:A5,B1:B5)", text("yellow")),
                ("=LOOKUP(7.66,A1:A5,B1:B5)", text("blue")),
                ("=LOOKUP(0,A1:A5,B1:B5)", err(CellError::NA)),
                ("=LOOKUP(\"C\",D1:G2)", num(3.0)),
                ("=LOOKUP(5.2,A1:B5)", text("yellow")),
                ("=LOOKUP(6,A:A,B:B)", text("green")),
                ("=LOOKUP(1/0,A1:A5)", err(CellError::DivZero)),
                ("=LOOKUP(1,2)", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn address_text() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=ADDRESS(2,3)", text("$C$2")),
                ("=ADDRESS(2,3,2)", text("C$2")),
                ("=ADDRESS(2,3,3)", text("$C2")),
                ("=ADDRESS(2,3,4)", text("C2")),
                ("=ADDRESS(2,3,2,FALSE)", text("R2C[3]")),
                (
                    "=ADDRESS(2,3,1,FALSE,\"[Book1]Sheet1\")",
                    text("'[Book1]Sheet1'!R2C3"),
                ),
                (
                    "=ADDRESS(2,3,1,FALSE,\"EXCEL SHEET\")",
                    text("'EXCEL SHEET'!R2C3"),
                ),
                ("=ADDRESS(1,1,1,TRUE,\"Data\")", text("Data!$A$1")),
                ("=ADDRESS(0,1)", err(CellError::Value)),
                ("=ADDRESS(1,1,5)", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn indirect_reads_and_follows_references() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Sheet1".into(), "My Data".into()]);
        e.set_value(0, CellCoord::new(0, 0), CellValueInput::Text("B2".into()));
        e.set_value(0, CellCoord::new(1, 1), CellValueInput::Number(1.333));
        fill(&mut e, "C1", &numbers(&[1.0, 2.0, 3.0]));
        e.set_value(1, CellCoord::new(0, 0), CellValueInput::Number(42.0));
        check(
            &mut e,
            &[
                ("=INDIRECT(A1)", num(1.333)),
                ("=INDIRECT(\"$B$\"&2)", num(1.333)),
                ("=SUM(INDIRECT(\"C1:C3\"))", num(6.0)),
                ("=SUM(INDIRECT(\"C:C\"))", num(6.0)),
                ("=COUNTIF(INDIRECT(\"C1:C3\"),\">1\")", num(2.0)),
                ("=ROWS(INDIRECT(\"C1:C3\"))", num(3.0)),
                ("=INDIRECT(\"'My Data'!A1\")", num(42.0)),
                ("=INDIRECT(\"nope\")", err(CellError::Ref)),
                ("=INDIRECT(\"Nope!A1\")", err(CellError::Ref)),
                ("=INDIRECT(\"A1\",FALSE)", err(CellError::Ref)),
                ("=INDIRECT(\"C1:C3\")", err(CellError::Value)),
            ],
        );

        // What INDIRECT reads is not a recorded dependency, so it recalculates
        // after any edit.
        e.set_formula(0, CellCoord::new(0, 4), "=INDIRECT(\"C\"&1)")
            .unwrap();
        e.set_formula(0, CellCoord::new(1, 4), "=SUM(INDIRECT(\"C1:C3\"))*2")
            .unwrap();
        assert_eq!(e.get_value(0, CellCoord::new(0, 4)), num(1.0));
        assert_eq!(e.get_value(0, CellCoord::new(1, 4)), num(12.0));
        e.set_value(0, CellCoord::new(0, 2), CellValueInput::Number(10.0));
        assert_eq!(e.get_value(0, CellCoord::new(0, 4)), num(10.0));
        assert_eq!(e.get_value(0, CellCoord::new(1, 4)), num(30.0));

        // A cell reading itself through INDIRECT is still a cycle.
        e.set_formula(0, CellCoord::new(5, 5), "=INDIRECT(\"F6\")")
            .unwrap();
        assert_eq!(
            e.get_value(0, CellCoord::new(5, 5)),
            err(CellError::Circular)
        );
    }

    #[test]
    fn offset_moves_and_resizes_references() {
        let mut e = CalcEngine::new();
        // C3:E5 holds 1..9 by rows; F5 holds 100.
        for (i, n) in (1..=9).enumerate() {
            let at = CellCoord::new(2 + i as u32 / 3, 2 + i as u32 % 3);
            e.set_value(0, at, CellValueInput::Number(n as f64));
        }
        e.set_value(0, CellCoord::new(4, 5), CellValueInput::Number(100.0));
        fill(&mut e, "A1", &numbers(&[1.0, 2.0, 3.0, 4.0, 5.0]));
        check(
            &mut e,
            &[
                ("=OFFSET(C3,2,3,1,1)", num(100.0)),
                ("=OFFSET(E5,-2,-2)", num(1.0)),
                ("=SUM(OFFSET(C3:E5,-1,0,3,3))", num(21.0)),
                ("=SUM(OFFSET(A1,1,0,3))", num(9.0)),
                // A negative height reaches up from the moved cell.
                ("=SUM(OFFSET(A5,0,0,-3))", num(12.0)),
                ("=ROWS(OFFSET(A1,0,0,4,2))", num(4.0)),
                ("=COLUMNS(OFFSET(A1,0,0,4,2))", num(2.0)),
                ("=AVERAGE(OFFSET(INDIRECT(\"A1\"),0,0,5))", num(3.0)),
                ("=OFFSET(C3:E5,0,-3,3,3)", err(CellError::Ref)),
                ("=OFFSET(A1,0,0,0)", err(CellError::Ref)),
                ("=OFFSET(A1,-1,0)", err(CellError::Ref)),
                ("=OFFSET(1,0,0)", err(CellError::Value)),
                ("=OFFSET(A1:A2,0,0)", err(CellError::Value)),
            ],
        );

        // Volatile, as what it reads is only known when it runs.
        e.set_formula(0, CellCoord::new(9, 6), "=SUM(OFFSET(A1,0,0,5))")
            .unwrap();
        assert_eq!(e.get_value(0, CellCoord::new(9, 6)), num(15.0));
        e.set_value(0, CellCoord::new(4, 0), CellValueInput::Number(50.0));
        assert_eq!(e.get_value(0, CellCoord::new(9, 6)), num(60.0));
    }

    #[test]
    fn formulatext_hyperlink_and_sheets() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Sheet1".into(), "Data".into()]);
        e.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(1.0));
        e.set_formula(0, CellCoord::new(0, 1), "=SUM(A1:A3)")
            .unwrap();
        e.set_formula(1, CellCoord::new(0, 0), "=SHEET()").unwrap();
        check(
            &mut e,
            &[
                ("=FORMULATEXT(B1)", text("=SUM(A1:A3)")),
                ("=FORMULATEXT(A1)", err(CellError::NA)),
                ("=FORMULATEXT(1)", err(CellError::Value)),
                ("=ISFORMULA(B1)", CellResult::Bool(true)),
                ("=ISFORMULA(A1)", CellResult::Bool(false)),
                ("=ISFORMULA(\"B1\")", err(CellError::Value)),
                (
                    "=HYPERLINK(\"https://example.com\",\"Report\")",
                    text("Report"),
                ),
                (
                    "=HYPERLINK(\"https://example.com\")",
                    text("https://example.com"),
                ),
                (
                    "=HYPERLINK(\"https://example.com\",1/0)",
                    err(CellError::DivZero),
                ),
                ("=SHEET()", num(1.0)),
                ("=SHEET(Data!A1)", num(2.0)),
                ("=SHEET(\"data\")", num(2.0)),
                ("=SHEET(\"Nope\")", err(CellError::NA)),
                ("=SHEETS()", num(2.0)),
                ("=SHEETS(Data!A1:B2)", num(1.0)),
                ("=SHEETS(1)", err(CellError::Value)),
            ],
        );
        assert_eq!(e.get_value(1, CellCoord::new(0, 0)), num(2.0));
    }

    #[test]
    fn parity_and_error_info() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=ISEVEN(-1)", CellResult::Bool(false)),
                ("=ISEVEN(2.5)", CellResult::Bool(true)),
                ("=ISEVEN(5)", CellResult::Bool(false)),
                ("=ISEVEN(Q1)", CellResult::Bool(true)),
                ("=ISODD(-1)", CellResult::Bool(true)),
                ("=ISODD(2.5)", CellResult::Bool(false)),
                ("=ISODD(5)", CellResult::Bool(true)),
                ("=ISEVEN(\"x\")", err(CellError::Value)),
                ("=ISODD(TRUE)", err(CellError::Value)),
                ("=ISEVEN(1/0)", err(CellError::DivZero)),
                ("=ISERR(NA())", CellResult::Bool(false)),
                ("=ISERR(1/0)", CellResult::Bool(true)),
                ("=ISERR(1)", CellResult::Bool(false)),
                ("=ERROR.TYPE(#NULL!)", num(1.0)),
                ("=ERROR.TYPE(1/0)", num(2.0)),
                ("=ERROR.TYPE(SQRT(-1))", num(6.0)),
                ("=ERROR.TYPE(NA())", num(7.0)),
                ("=ERROR.TYPE(1)", err(CellError::NA)),
            ],
        );
    }

    #[test]
    fn references_count_only_numbers() {
        let mut e = CalcEngine::new();
        e.set_value(0, CellCoord::new(0, 0), CellValueInput::Bool(true));
        e.set_value(0, CellCoord::new(1, 0), CellValueInput::Number(2.0));
        // As in Excel, a referenced logical is skipped while a typed one counts.
        check(
            &mut e,
            &[
                ("=SUM(A1,A2)", num(2.0)),
                ("=SUM(TRUE,A2)", num(3.0)),
                ("=COUNT(A1)", num(0.0)),
                ("=ROWS(A1)", num(1.0)),
            ],
        );
    }

    #[test]
    fn older_functions_pass_errors_through() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[1.0, 5.0, 3.0]));
        fill(&mut e, "B1", &numbers(&[10.0, 20.0, 30.0]));
        e.set_formula(0, CellCoord::new(3, 0), "=1/0").unwrap();
        e.set_formula(0, CellCoord::new(1, 1), "=NA()").unwrap();
        let div0 = || err(CellError::DivZero);
        let na = || err(CellError::NA);
        // The first error among the arguments is the answer, as in Excel;
        // one case per way the functions read their arguments.
        check(
            &mut e,
            &[
                // One number in: ABS, SQRT, EXP, LN, LOG10, INT, SIGN, trig
                ("=ABS(1/0)", div0()),
                ("=SQRT(NA())", na()),
                ("=SIN(A4)", div0()),
                ("=SIGN(\"x\")", err(CellError::Value)),
                ("=ASIN(2)", err(CellError::Num)),
                ("=EXP(1000)", err(CellError::Num)),
                // ROUND, ROUNDUP, ROUNDDOWN, TRUNC
                ("=ROUND(1/0,2)", div0()),
                ("=TRUNC(2.5,NA())", na()),
                // Two numbers: POWER, MOD, LOG, CEILING, FLOOR, RANDBETWEEN
                ("=MOD(NA(),2)", na()),
                ("=POWER(2,1/0)", div0()),
                ("=LOG(100,NA())", na()),
                ("=CEILING(A4,1)", div0()),
                ("=RANDBETWEEN(1,NA())", na()),
                // Aggregates that used to skip errors
                ("=AVERAGE(A1:A4)", div0()),
                ("=MAX(A1:A4)", div0()),
                ("=MIN(1,NA())", na()),
                ("=MEDIAN(A1:A4)", div0()),
                ("=LARGE(A1:A3,1/0)", div0()),
                ("=SUMIF(A1:A3,\">2\",B1:B3)", na()),
                ("=AVERAGEIF(A1:A3,\">2\",B1:B3)", na()),
                // Text
                ("=LEN(1/0)", div0()),
                ("=UPPER(NA())", na()),
                ("=LEFT(A4)", div0()),
                ("=MID(\"abc\",NA(),1)", na()),
                ("=FIND(\"a\",A4)", div0()),
                ("=SUBSTITUTE(\"a\",\"a\",NA())", na()),
                ("=REPT(\"a\",1/0)", div0()),
                ("=EXACT(NA(),\"a\")", na()),
                ("=VALUE(A4)", div0()),
                ("=TEXT(1,NA())", na()),
                ("=CHAR(1/0)", div0()),
                ("=CODE(NA())", na()),
                ("=CONCATENATE(\"a\",A4)", div0()),
                // Logical
                ("=IF(1/0,1,2)", div0()),
                ("=AND(TRUE,NA())", na()),
                ("=OR(A1:A4)", div0()),
                ("=NOT(NA())", na()),
                ("=IFS(A4,1)", div0()),
                ("=CHOOSE(NA(),1,2)", na()),
                ("=SWITCH(1/0,1,2)", div0()),
                // Lookup
                ("=VLOOKUP(NA(),A1:A3,1,FALSE)", na()),
                ("=HLOOKUP(1,A1:A3,NA())", na()),
                ("=INDEX(A1:A3,1/0)", div0()),
                ("=MATCH(1/0,A1:A3,0)", div0()),
                // Dates
                ("=DATE(2024,NA(),1)", na()),
                ("=YEAR(1/0)", div0()),
                ("=DAY(\"x\")", err(CellError::Value)),
            ],
        );
    }

    #[test]
    fn older_text_and_logical_functions_follow_excel() {
        let mut e = CalcEngine::new();
        fill(
            &mut e,
            "A1",
            &[
                CellValueInput::Bool(true),
                CellValueInput::Number(1.0),
                CellValueInput::Text("x".into()),
            ],
        );
        check(
            &mut e,
            &[
                (
                    "=TRIM(\" First   Quarter  Earnings \")",
                    text("First Quarter Earnings"),
                ),
                ("=LEN(\"héllo\")", num(5.0)),
                ("=LEN(Q1)", num(0.0)),
                ("=UPPER(12.5)", text("12.5")),
                ("=PROPER(\"this is a TITLE\")", text("This Is A Title")),
                ("=PROPER(\"2-way street\")", text("2-Way Street")),
                ("=PROPER(\"76BudGet\")", text("76Budget")),
                // Positions count characters, not bytes.
                ("=FIND(\"b\",\"ébb\",3)", num(3.0)),
                ("=SEARCH(\"B\",\"ébb\")", num(2.0)),
                ("=FIND(\"B\",\"abc\")", err(CellError::Value)),
                ("=FIND(\"\",\"abc\",2)", num(2.0)),
                ("=MID(\"Fluid Flow\",7,20)", text("Flow")),
                ("=MID(\"abc\",0,1)", err(CellError::Value)),
                ("=LEFT(\"Sale Price\",4)", text("Sale")),
                ("=RIGHT(\"Stock Number\")", text("r")),
                ("=LEFT(\"abc\",-1)", err(CellError::Value)),
                ("=REPLACE(\"abcdefghijk\",6,5,\"*\")", text("abcde*k")),
                (
                    "=SUBSTITUTE(\"Quarter 1, 2011\",\"1\",\"2\",3)",
                    text("Quarter 1, 2012"),
                ),
                ("=SUBSTITUTE(\"abc\",\"\",\"x\")", text("abc")),
                ("=SUBSTITUTE(\"abc\",\"b\",\"x\",0)", err(CellError::Value)),
                // Past what a cell holds.
                ("=REPT(\"ab\",100000)", err(CellError::Value)),
                ("=VALUE(\"$1,000\")", num(1000.0)),
                ("=CONCATENATE(\"x\",0.1+0.2)", text("x0.3")),
                ("=CONCAT(A1:A3,\"!\")", text("TRUE1x!")),
                // Referenced text is skipped; typed text must be a logical.
                ("=AND(A1:A3)", CellResult::Bool(true)),
                ("=XOR(TRUE,A1:A2)", CellResult::Bool(true)),
                ("=OR(A3)", err(CellError::Value)),
                ("=AND(FALSE,\"x\")", err(CellError::Value)),
                ("=IF(\"x\",1,2)", err(CellError::Value)),
                ("=IF(\"true\",1,2)", num(1.0)),
                ("=CHOOSE(2.9,\"a\",\"b\",\"c\")", text("b")),
                ("=POWER(0,-1)", err(CellError::DivZero)),
                ("=POWER(-8,1/3)", err(CellError::Num)),
                ("=LOG(8,2)", num(3.0)),
                ("=RANDBETWEEN(5,1)", err(CellError::Num)),
            ],
        );
    }

    #[test]
    fn lookups_match_like_excel() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[1.0, 3.0, 5.0, 7.0]));
        let texts = |items: &[&str]| -> Vec<CellValueInput> {
            items
                .iter()
                .map(|s| CellValueInput::Text(s.to_string()))
                .collect()
        };
        fill(&mut e, "B1", &texts(&["a", "b", "c", "d"]));
        fill(&mut e, "C1", &numbers(&[7.0, 5.0, 3.0, 1.0]));
        fill(&mut e, "D1", &texts(&["apple", "banana", "cherry"]));
        for (a1, v) in [("F1", 1.0), ("G1", 3.0), ("H1", 5.0)] {
            e.set_value(
                0,
                CellCoord::from_a1(a1).unwrap(),
                CellValueInput::Number(v),
            );
        }
        for (a1, v) in [("F2", "x"), ("G2", "y"), ("H2", "z")] {
            e.set_value(
                0,
                CellCoord::from_a1(a1).unwrap(),
                CellValueInput::Text(v.into()),
            );
        }
        check(
            &mut e,
            &[
                // Approximate: the last entry at or below the value.
                ("=VLOOKUP(4,A1:B4,2)", text("b")),
                ("=VLOOKUP(4,A1:B4,2,TRUE)", text("b")),
                ("=VLOOKUP(7,A1:B4,2)", text("d")),
                ("=VLOOKUP(100,A1:B4,2)", text("d")),
                ("=VLOOKUP(0,A1:B4,2)", err(CellError::NA)),
                ("=VLOOKUP(4,A1:B4,2,FALSE)", err(CellError::NA)),
                ("=VLOOKUP(3,A1:B4,0)", err(CellError::Value)),
                ("=VLOOKUP(3,A1:B4,3)", err(CellError::Ref)),
                ("=HLOOKUP(4,F1:H2,2)", text("y")),
                ("=HLOOKUP(4,F1:H2,2,FALSE)", err(CellError::NA)),
                ("=MATCH(4,A1:A4)", num(2.0)),
                ("=MATCH(4,C1:C4,-1)", num(2.0)),
                ("=MATCH(\"blueberry\",D1:D3)", num(2.0)),
                ("=MATCH(\"CHERRY\",D1:D3,0)", num(3.0)),
                ("=MATCH(5,F1:H1,0)", num(3.0)),
                ("=MATCH(1,A1:B4,0)", err(CellError::NA)),
                // A one-row range takes its only number as the column.
                ("=INDEX(F1:H1,2)", num(3.0)),
                ("=INDEX(A1:A4,2,0)", num(3.0)),
                ("=INDEX(A1:B4,0,1)", err(CellError::Value)),
                ("=INDEX(A1:A4,5)", err(CellError::Ref)),
            ],
        );
    }

    #[test]
    fn wildcards_in_search_and_exact_lookups() {
        let mut e = CalcEngine::new();
        let t = |s: &str| CellValueInput::Text(s.into());
        fill(&mut e, "D1", &[t("apple"), t("banana"), t("cherry")]);
        fill(
            &mut e,
            "J1",
            &[t("a*c"), t("abc"), t("*"), CellValueInput::Number(5.0)],
        );
        check(
            &mut e,
            &[
                // SEARCH: * any run, ? one character, ~ escapes; the answer
                // is where the match starts.
                ("=SEARCH(\"e\",\"Statements\",6)", num(7.0)),
                ("=SEARCH(\"margin\",\"Profit Margin\")", num(8.0)),
                ("=SEARCH(\"b*d\",\"abcde\")", num(2.0)),
                ("=SEARCH(\"B*E\",\"abcde\")", num(2.0)),
                ("=SEARCH(\"?c\",\"abcde\")", num(2.0)),
                ("=SEARCH(\"c?e\",\"abcde\")", num(3.0)),
                ("=SEARCH(\"*\",\"abc\")", num(1.0)),
                ("=SEARCH(\"*c\",\"abcabc\",2)", num(2.0)),
                ("=SEARCH(\"~*\",\"a*b\")", num(2.0)),
                ("=SEARCH(\"~?\",\"what?\")", num(5.0)),
                ("=SEARCH(\"x*\",\"abc\")", err(CellError::Value)),
                ("=SEARCH(\"b?\",\"ab\")", err(CellError::Value)),
                // FIND reads them literally.
                ("=FIND(\"*\",\"a*b\")", num(2.0)),
                ("=FIND(\"?\",\"a?b\")", num(2.0)),
                ("=FIND(\"b*\",\"abc\")", err(CellError::Value)),
                // MATCH type 0, VLOOKUP and HLOOKUP with FALSE: text only.
                ("=MATCH(\"b*\",D1:D3,0)", num(2.0)),
                ("=MATCH(\"?a*\",D1:D3,0)", num(2.0)),
                ("=MATCH(\"*rr*\",D1:D3,0)", num(3.0)),
                ("=MATCH(\"CH*\",D1:D3,0)", num(3.0)),
                ("=MATCH(\"z*\",D1:D3,0)", err(CellError::NA)),
                ("=MATCH(\"*\",{\"a\",\"*\",\"b\"},0)", num(1.0)),
                ("=MATCH(\"~*\",{\"a\",\"*\",\"b\"},0)", num(2.0)),
                ("=MATCH(\"a~*b\",{\"axb\",\"a*b\"},0)", num(2.0)),
                ("=MATCH(\"1*\",{1,\"1a\"},0)", num(2.0)),
                (
                    "=VLOOKUP(\"ban*\",{\"apple\",1;\"banana\",2;\"cherry\",3},2,FALSE)",
                    num(2.0),
                ),
                (
                    "=VLOOKUP(\"*rr?\",{\"apple\",1;\"banana\",2;\"cherry\",3},2,FALSE)",
                    num(3.0),
                ),
                ("=VLOOKUP(\"~*\",{\"x\",20;\"*\",10},2,FALSE)", num(10.0)),
                ("=VLOOKUP(\"*\",{1,20;\"*\",10},2,FALSE)", num(10.0)),
                ("=VLOOKUP(\"q*\",{\"apple\",1},2,FALSE)", err(CellError::NA)),
                (
                    "=HLOOKUP(\"b*\",{\"apple\",\"banana\";1,2},2,FALSE)",
                    num(2.0),
                ),
                // Criteria: the same matcher, now with ~, and only text
                // matches a pattern.
                ("=COUNTIF(J1:J5,\"~*\")", num(1.0)),
                ("=COUNTIF(J1:J5,\"a~*c\")", num(1.0)),
                ("=COUNTIF(J1:J5,\"a*c\")", num(2.0)),
                ("=COUNTIF(J1:J5,\"*\")", num(3.0)),
                ("=COUNTIF(J:J,\"*\")", num(3.0)),
                ("=COUNTIF(J1:J5,\"=a?c\")", num(2.0)),
                ("=COUNTIF(J1:J5,\"<>a*\")", num(3.0)),
            ],
        );
    }

    #[test]
    fn wildcard_patterns_take_hostile_input_quickly() {
        // Backtracking over every * would take ages here.
        let text = "a".repeat(30_000);
        let pattern = format!("{}b", "*a".repeat(40));
        assert!(!wildcard_match(&pattern, &text));
        let folded: Vec<char> = text.chars().collect();
        assert_eq!(wildcard_find(&format!("a{pattern}"), &folded, 0), None);
        assert_eq!(wildcard_find("a*a*a", &folded, 0), Some(0));
        assert!(wildcard_match("~", "~") && wildcard_match("a~b", "a~b"));
        assert!(wildcard_match("~~", "~") && !wildcard_match("~~", "~~"));
    }

    #[test]
    fn empty_argument_slots() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[1.0, 3.0, 5.0]));
        fill(
            &mut e,
            "B1",
            &[
                CellValueInput::Text("a".into()),
                CellValueInput::Text("b".into()),
                CellValueInput::Text("c".into()),
            ],
        );
        // Financial functions take their documented defaults: fv 0, type
        // 0, guess 0.1.
        check_close(
            &mut e,
            0.005,
            &[
                ("=PMT(8%/12,10,10000,,1)", -1030.16),
                ("=PMT(5%/12,360,,100000)", -120.155),
                ("=FV(12%/12,12,-1000,,)", 12682.50),
            ],
        );
        check_close(&mut e, 1e-9, &[("=RATE(48,-200,8000,,,)", 0.007_701_472_5)]);
        check(
            &mut e,
            &[
                // An empty branch is 0.
                ("=IF(TRUE,,5)", num(0.0)),
                ("=IF(FALSE,5,)", num(0.0)),
                ("=IF(FALSE,5)", CellResult::Bool(false)),
                ("=IF(A1>0,,\"no\")", num(0.0)),
                ("=CHOOSE(2,1,)", num(0.0)),
                ("=IFERROR(1/0,)", num(0.0)),
                // In a list of numbers an empty slot is 0 and counts.
                ("=SUM(1,,2)", num(3.0)),
                ("=COUNT(1,,2)", num(3.0)),
                ("=COUNTA(1,,2)", num(3.0)),
                ("=AVERAGE(1,,2)", num(1.0)),
                ("=MIN(5,,3)", num(0.0)),
                // Elsewhere it reads as a blank cell: "", 0 or FALSE...
                ("=CONCATENATE(\"a\",,\"b\")", text("ab")),
                ("=ROUND(2.5,)", num(3.0)),
                ("=LEFT(\"abc\",)", text("")),
                ("=VLOOKUP(3,A1:B3,2,)", text("b")),
                ("=VLOOKUP(4,A1:B3,2,)", err(CellError::NA)),
                ("=MATCH(4,A1:A3,)", err(CellError::NA)),
                // ...unless a newer function names a default for it.
                ("=ADDRESS(1,1,,FALSE)", text("R1C1")),
                ("=WEEKDAY(DATE(2008,2,14),)", num(5.0)),
                ("=DB(10000,1000,5,1,)", num(3690.0)),
                // Empty slots still count as arguments.
                ("=ABS(,)", err(CellError::Value)),
                ("=IF(TRUE,1,2,)", err(CellError::Value)),
            ],
        );
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.142 is a rounding result, not PI
    fn rounding_works_at_fifteen_digits() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                // 2.675 and 1.005 are stored a hair below the half.
                ("=ROUND(2.675,2)", num(2.68)),
                ("=ROUND(1.005,2)", num(1.01)),
                ("=ROUND(2.15,1)", num(2.2)),
                ("=ROUND(2.149,1)", num(2.1)),
                ("=ROUND(-1.475,2)", num(-1.48)),
                ("=ROUND(21.5,-1)", num(20.0)),
                ("=ROUND(626.3,-3)", num(1000.0)),
                ("=ROUND(1.98,-1)", num(0.0)),
                ("=ROUND(-50.55,-2)", num(-100.0)),
                ("=ROUNDUP(3.2,0)", num(4.0)),
                ("=ROUNDUP(76.9,0)", num(77.0)),
                ("=ROUNDUP(3.14159,3)", num(3.142)),
                ("=ROUNDUP(-3.14159,1)", num(-3.2)),
                ("=ROUNDUP(31415.92654,-2)", num(31500.0)),
                ("=ROUNDDOWN(3.2,0)", num(3.0)),
                ("=ROUNDDOWN(76.9,0)", num(76.0)),
                ("=ROUNDDOWN(3.14159,3)", num(3.141)),
                ("=ROUNDDOWN(-3.14159,1)", num(-3.1)),
                ("=ROUNDDOWN(31415.92654,-2)", num(31400.0)),
                ("=TRUNC(8.9)", num(8.0)),
                ("=TRUNC(-8.9)", num(-8.0)),
                ("=TRUNC(0.45)", num(0.0)),
                // More digits than an f64 has change nothing.
                ("=ROUND(1.5,400)", num(1.5)),
                ("=ROUND(123,-400)", num(0.0)),
                ("=CEILING(1.5,0.1)", num(1.5)),
                ("=CEILING(0.234,0.01)", num(0.24)),
                ("=FLOOR(1.58,0.1)", num(1.5)),
                ("=FLOOR(0.234,0.01)", num(0.23)),
                // 0.3/0.1 is 2.9999...; at 15 digits it is 3.
                ("=FLOOR(0.3,0.1)", num(0.3)),
                ("=CEILING(0.3,0.1)", num(0.3)),
            ],
        );
        let CellResult::Value(zero) = eval(&mut e, "=ROUND(-0.4,0)") else {
            panic!("expected a number");
        };
        assert!(zero.is_sign_positive(), "no negative zero");
    }

    #[test]
    fn serial_60_is_excels_february_29th_1900() {
        let mut e = CalcEngine::new();
        check(
            &mut e,
            &[
                ("=DATE(1900,2,28)", num(59.0)),
                ("=DATE(1900,2,29)", num(60.0)),
                ("=DATE(1900,3,1)", num(61.0)),
                ("=DATE(1900,1,60)", num(60.0)),
                ("=DATE(1900,3,0)", num(60.0)),
                ("=DATE(1901,1,1)", num(367.0)),
                ("=YEAR(60)", num(1900.0)),
                ("=MONTH(60)", num(2.0)),
                ("=DAY(60)", num(29.0)),
                ("=DAY(59)", num(28.0)),
                ("=MONTH(61)", num(3.0)),
                ("=DAY(61)", num(1.0)),
                ("=YEAR(0)", num(1900.0)),
                ("=MONTH(0)", num(1.0)),
                ("=DAY(0)", num(0.0)),
                ("=YEAR(-1)", err(CellError::Num)),
                // WEEKDAY counts serials modulo 7 from serial 1 as a Sunday,
                // so it is right from March 1900 (1900-03-01 was a
                // Thursday) and a day off the real calendar before, as in
                // Excel.
                ("=WEEKDAY(1)", num(1.0)),
                ("=WEEKDAY(59)", num(3.0)),
                ("=WEEKDAY(60)", num(4.0)),
                ("=WEEKDAY(60,2)", num(3.0)),
                ("=WEEKDAY(61)", num(5.0)),
                ("=DATEVALUE(\"2/29/1900\")", num(60.0)),
                ("=DATEVALUE(\"1900-02-29\")", num(60.0)),
                ("=DATEVALUE(\"2/29/1901\")", err(CellError::Value)),
                ("=TEXT(60,\"yyyy-mm-dd\")", text("1900-02-29")),
                ("=TEXT(61,\"mmm d\")", text("Mar 1")),
            ],
        );
        // Every date reads back as its serial.
        for serial in (0..=1500).chain([MAX_DATE_SERIAL as i32]) {
            let (y, m, d) = serial_to_date(serial as f64);
            assert_eq!(date_to_serial(y, m, d), serial as f64, "{y}-{m}-{d}");
        }
        assert_eq!(serial_to_date(MAX_DATE_SERIAL), (9999, 12, 31));
    }

    #[test]
    fn row_and_column_without_a_reference_are_the_formula_cell() {
        let mut e = CalcEngine::new();
        let at = |a1: &str| CellCoord::from_a1(a1).unwrap();
        e.set_formula(0, at("B3"), "=ROW()").unwrap();
        e.set_formula(0, at("C3"), "=COLUMN()").unwrap();
        // One formula filled down: each cell reads its own row.
        for row in 0..4 {
            e.set_formula(0, CellCoord::new(row, 3), "=ROW()*10+COLUMN()")
                .unwrap();
        }
        e.set_formula(0, at("E1"), "=SUM(D1:D4)+B3").unwrap();
        e.set_formula(0, at("F1"), "=ROW(C7)+COLUMN(C7)").unwrap();
        // E1 first: its precedents are evaluated for it, each as itself.
        assert_eq!(
            e.get_value(0, at("E1")),
            num(14.0 + 24.0 + 34.0 + 44.0 + 3.0)
        );
        assert_eq!(e.get_value(0, at("B3")), num(3.0));
        assert_eq!(e.get_value(0, at("C3")), num(3.0));
        assert_eq!(e.get_value(0, at("D4")), num(44.0));
        assert_eq!(e.get_value(0, at("F1")), num(10.0));
        // Outside any cell there is no row to give.
        let expr = crate::formula::FormulaParser::new()
            .parse("=ROW()")
            .unwrap();
        assert_eq!(e.evaluate_expr(0, &expr), err(CellError::Value));
        assert_eq!(e.evaluate_expr_at(0, at("Z9"), &expr), num(9.0));
    }

    #[test]
    fn array_constants_where_ranges_are_read() {
        let mut e = CalcEngine::new();
        fill(&mut e, "A1", &numbers(&[1.0, 2.0, 3.0]));
        let t = CellResult::Bool(true);
        let f = CellResult::Bool(false);
        check(
            &mut e,
            &[
                // Aggregates read only an array's numbers, as a reference's.
                ("=SUM({1,2,3})", num(6.0)),
                ("=SUM({1,\"a\",TRUE})", num(1.0)),
                ("=AVERAGE({1,2;3,4})", num(2.5)),
                ("=COUNT({1,\"a\",TRUE,2})", num(2.0)),
                ("=COUNTA({1,\"a\"})", num(2.0)),
                ("=MAX({1,#N/A})", err(CellError::NA)),
                ("=LARGE({5,1,3},1)", num(5.0)),
                ("=SMALL({5,1,3},1)", num(1.0)),
                ("=SUMPRODUCT({1,2,3},{4,5,6})", num(32.0)),
                ("=SUMPRODUCT({1;1;1},A1:A3)", num(6.0)),
                ("=SUMPRODUCT({1,2},{1;2})", err(CellError::Value)),
                ("=AND({TRUE,FALSE})", f.clone()),
                ("=OR({0,1})", t.clone()),
                // Lookups
                ("=VLOOKUP(2,{1,\"a\";2,\"b\"},2)", text("b")),
                ("=VLOOKUP(2.5,{1,\"a\";2,\"b\"},2)", text("b")),
                ("=VLOOKUP(2,{1,\"a\";2,\"b\"},2,FALSE)", text("b")),
                ("=VLOOKUP(2,{1,\"a\";2,\"b\"},3)", err(CellError::Ref)),
                ("=HLOOKUP(\"b\",{\"a\",\"b\";1,2},2,FALSE)", num(2.0)),
                ("=LOOKUP(2.5,{1,2,3},{\"x\",\"y\",\"z\"})", text("y")),
                ("=LOOKUP(2,{1,\"a\";2,\"b\"})", text("b")),
                ("=LOOKUP(\"c\",{\"a\",\"b\",\"c\";1,2,3})", num(3.0)),
                ("=MATCH(3,{1,3,5},0)", num(2.0)),
                ("=MATCH(4,{1,3,5})", num(2.0)),
                ("=INDEX({1,2;3,4},2,1)", num(3.0)),
                ("=INDEX({\"a\",\"b\",\"c\"},2)", text("b")),
                ("=ROWS({1,2;3,4;5,6})", num(3.0)),
                ("=COLUMNS({1,2,3})", num(3.0)),
                // Text and dates
                ("=CONCAT({\"a\",\"b\"},\"c\")", text("abc")),
                ("=TEXTJOIN(\"-\",TRUE,{\"a\",\"\",\"b\"})", text("a-b")),
                (
                    "=TEXTJOIN({\"-\",\"+\"},TRUE,\"a\",\"b\",\"c\")",
                    text("a-b+c"),
                ),
                (
                    "=NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,5),{45293})",
                    num(4.0),
                ),
                ("=CORREL({1,2,3},{2,4,6})", num(1.0)),
                ("=CORREL({1,2,3},{2,4})", err(CellError::NA)),
                // As one value, only a one-item array has one.
                ("={5}", num(5.0)),
                ("={1,2,3}", err(CellError::Value)),
                ("={1,2}+1", err(CellError::Value)),
                ("=ABS({-1,2})", err(CellError::Value)),
            ],
        );
        check_close(&mut e, 1e-12, &[("=AVERAGEA({1,\"a\",TRUE})", 2.0 / 3.0)]);
    }

    #[test]
    fn operators_work_item_by_item_over_ranges_and_arrays() {
        let mut e = CalcEngine::new();
        let t = |s: &str| CellValueInput::Text(s.into());
        let n = CellValueInput::Number;
        fill(
            &mut e,
            "A1",
            &[
                n(1.0),
                n(2.0),
                n(3.0),
                n(4.0),
                n(5.0),
                t("x"),
                // A7 blank
            ],
        );
        fill(
            &mut e,
            "B1",
            &numbers(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0]),
        );
        fill(&mut e, "C1", &[t("x"), t("y"), t("x"), t("z"), t("x")]);
        // D2 is blank.
        fill(&mut e, "D1", &[n(1.0)]);
        fill(&mut e, "D3", &[t("t")]);
        let (yes, no) = (CellResult::Bool(true), CellResult::Bool(false));
        check(
            &mut e,
            &[
                // The classic patterns
                ("=SUMPRODUCT(A1:A5*B1:B5)", num(550.0)),
                ("=SUMPRODUCT((C1:C5=\"x\")*B1:B5)", num(90.0)),
                ("=SUMPRODUCT(--(A1:A5>2))", num(3.0)),
                ("=SUMPRODUCT(-(C1:C5=\"x\"))", num(-3.0)),
                ("=SUMPRODUCT((A1:A5>1)*(A1:A5<5))", num(3.0)),
                ("=SUMPRODUCT(A1:A3,B1:B3*1)", num(140.0)),
                ("=SUM({1,2,3}+1)", num(9.0)),
                ("=SUM(A1:A3*2)", num(12.0)),
                ("=MAX(A1:A5*-1)", num(-1.0)),
                ("=LARGE(A1:A5*2,1)", num(10.0)),
                ("=SUM({1,2,3}*{4,5,6})", num(32.0)),
                ("=SUMPRODUCT({1,2}*{3,4})", num(11.0)),
                // Logicals in an array are skipped, as in a range, until
                // - or * turns them into 1 and 0.
                ("=SUM(A1:A3>1)", num(0.0)),
                ("=SUM(--(A1:A3>1))", num(2.0)),
                ("=SUMPRODUCT(A1:A5>2)", num(0.0)),
                ("=AND(A1:A3>0)", yes.clone()),
                ("=AND(A1:A3>1)", no.clone()),
                ("=OR(A1:A3>2)", yes.clone()),
                ("=INDEX(A1:A3>1,2)", yes.clone()),
                ("=MATCH(TRUE,A1:A5>2,0)", num(3.0)),
                (
                    "=INDEX(B1:B5,MATCH(1,(C1:C5=\"z\")*(A1:A5>3),0))",
                    num(40.0),
                ),
                ("=VLOOKUP(3,A1:B5*1,2,FALSE)", num(30.0)),
                ("=CONCAT(C1:C3&\"!\")", text("x!y!x!")),
                ("=ROWS(A1:A3*2)", num(3.0)),
                ("=COLUMNS({1,2,3}*{1;2})", num(3.0)),
                // A row against a column gives every pairing; other shapes
                // that differ fill with #N/A.
                ("=SUM({1,2,3}*{10;20})", num(180.0)),
                ("=SUM({1,2,3}+{1,2})", err(CellError::NA)),
                ("=INDEX({1,2,3}+{1,2},1,2)", num(4.0)),
                ("=INDEX({1,2,3}+{1,2},1,3)", err(CellError::NA)),
                // Blanks are 0 and text is #VALUE!, item by item; an
                // aggregate gives the first error it meets.
                ("=INDEX(D1:D3*2,1)", num(2.0)),
                ("=INDEX(D1:D3*2,2)", num(0.0)),
                ("=INDEX(D1:D3*2,3)", err(CellError::Value)),
                ("=SUM(D1:D2*2)", num(2.0)),
                ("=COUNT(D1:D2*2)", num(2.0)),
                ("=SUM(D1:D3*2)", err(CellError::Value)),
                ("=SUMPRODUCT(A1:A6*B1:B6)", err(CellError::Value)),
                ("=SUM(A1:A3/(A1:A3-2))", err(CellError::DivZero)),
                // In a cell, one item is its value and more are #VALUE!.
                ("=A1:A1*2", num(2.0)),
                ("=A2:A2", num(2.0)),
                ("={5}+1", num(6.0)),
                ("=-{3}", num(-3.0)),
                ("=A1:A3*2", err(CellError::Value)),
                ("=A1:A3=B1:B3", err(CellError::Value)),
                ("=Nope!A1:A3*2", err(CellError::Ref)),
            ],
        );
        check_close(&mut e, 1e-15, &[("=SUM(A1:A2%)", 0.03)]);
        // Ranges inside operators are dependencies like any other.
        e.set_formula(0, CellCoord::new(0, 25), "=SUM(A1:A3*2)")
            .unwrap();
        e.set_value(0, CellCoord::new(0, 0), CellValueInput::Number(11.0));
        assert_eq!(e.get_value(0, CellCoord::new(0, 25)), num(32.0));
    }

    #[test]
    fn whole_columns_in_operators_stop_at_the_used_rows() {
        let mut e = CalcEngine::new();
        e.set_sheet_names(vec!["Sheet1".into(), "Sheet2".into()]);
        fill(&mut e, "B1", &numbers(&[10.0, 20.0, 30.0]));
        let t = |s: &str| CellValueInput::Text(s.into());
        fill(&mut e, "C1", &[t("x"), t("y"), t("x")]);
        fill(&mut e, "E1", &numbers(&[1.0, 1.0, 1.0]));
        // Sheet2 is used further down than Sheet1: both sides keep its five
        // rows, where each sheet's own extent would differ in shape.
        for (row, v) in [(0, 2.0), (4, 7.0)] {
            e.set_value(1, CellCoord::new(row, 0), CellValueInput::Number(v));
        }
        let d1 = CellCoord::new(0, 3);
        e.set_formula(0, d1, "=SUMPRODUCT(B:B*Sheet2!A:A)").unwrap();
        assert_eq!(e.get_value(0, d1), num(20.0));
        check(
            &mut e,
            &[
                ("=SUMPRODUCT(B:B*E:E)", num(60.0)),
                ("=SUMPRODUCT((C:C=\"x\")*B:B)", num(40.0)),
                ("=SUM(B:B*2)", num(120.0)),
                ("=SUMPRODUCT((1:1=\"x\")*1)", num(1.0)),
                ("=SUMPRODUCT(INDIRECT(\"B:B\")*E:E)", num(60.0)),
                // Down to the used rows only (Z1000 holds this formula),
                // where Excel reads all 1,048,576.
                ("=ROWS(B:B*1)", num(1000.0)),
                // Past the cap on items: #VALUE!, before anything is built.
                ("=SUMPRODUCT(A1:B1048576*1)", err(CellError::Value)),
                ("=SUM(A1:A1048576*0+1)", num(1_048_576.0)),
            ],
        );
    }
}
