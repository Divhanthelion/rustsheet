//! Excel number format codes: `#,##0.00`, `0%`, `$#,##0_);[Red]($#,##0)`,
//! `yyyy-mm-dd`, `h:mm AM/PM`, `# ?/?` and so on.

use super::Rgb;
use crate::calc::functions::{apply_text_format, serial_to_date};

/// A number rendered with a format code.
#[derive(Debug, Clone, PartialEq)]
pub struct FormattedNumber {
    pub text: String,
    /// Color from a `[Red]`-style section prefix.
    pub color: Option<Rgb>,
}

/// Format code for a built-in `numFmtId` (ECMA-376 §18.8.30).
/// `None` for General (0) and ids Excel leaves to the locale.
pub fn builtin_number_format(id: u32) -> Option<&'static str> {
    Some(match id {
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "$#,##0_);($#,##0)",
        6 => "$#,##0_);[Red]($#,##0)",
        7 => "$#,##0.00_);($#,##0.00)",
        8 => "$#,##0.00_);[Red]($#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0_);(#,##0)",
        38 => "#,##0_);[Red](#,##0)",
        39 => "#,##0.00_);(#,##0.00)",
        40 => "#,##0.00_);[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mm:ss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// Render `n` with an Excel format code.
pub fn format_number(n: f64, code: &str) -> FormattedNumber {
    let sections = split_sections(code);
    // A dedicated negative or zero section gets the absolute value.
    let (section, value, signed_section) = if n < 0.0 && sections.len() >= 2 {
        (sections[1], -n, true)
    } else if n == 0.0 && sections.len() >= 3 {
        (sections[2], 0.0, true)
    } else {
        (sections[0], n, false)
    };

    let (body, color) = clean_section(section);
    let trimmed = body.trim();
    let datetime = is_datetime(&body);
    let needs_sign = !signed_section && !datetime && value < 0.0;
    let v = if needs_sign { -value } else { value };

    let text = if trimmed.is_empty() && signed_section {
        String::new()
    } else if trimmed.eq_ignore_ascii_case("general") || trimmed == "@" || trimmed.is_empty() {
        format_general(v, 11)
    } else if datetime {
        format_datetime(v, &body)
    } else if is_fraction(&body) {
        format_fraction(v, &body)
    } else if !unquoted(&body).contains(['0', '#', '?']) {
        // Literal-only section, e.g. "zero" in 0;-0;"zero".
        body.replace('"', "")
    } else {
        apply_text_format(v, &body)
    };

    let shows_nonzero = text.chars().any(|c| c.is_ascii_digit() && c != '0');
    let text = if needs_sign && shows_nonzero {
        format!("-{text}")
    } else {
        text
    };
    FormattedNumber { text, color }
}

/// Whether a format code shows dates or times.
pub fn is_date_format(code: &str) -> bool {
    let first = split_sections(code)[0];
    is_datetime(&clean_section(first).0)
}

/// Format a number like Excel's General format: as many decimals as fit in
/// `max_len` characters, switching to scientific notation when even the
/// integer part does not fit.
pub fn format_general(n: f64, max_len: usize) -> String {
    if n == 0.0 || !n.is_finite() {
        return if n.is_nan() {
            "#NUM!".into()
        } else {
            "0".into()
        };
    }
    let trim = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };

    // Plain notation, unless the number is too small to show any digit.
    if n.abs() >= 1e-9 || max_len >= 12 {
        for decimals in (0..=10).rev() {
            let s = trim(format!("{n:.decimals$}"));
            if s.len() <= max_len && s != "0" && s != "-0" {
                return s;
            }
        }
    }

    // Scientific, e.g. 1.23457E+15.
    let mut fallback = String::new();
    for digits in (0..=5).rev() {
        let raw = format!("{n:.digits$E}");
        let (mantissa, exp) = raw.split_once('E').unwrap_or((&raw, "0"));
        let exp: i32 = exp.parse().unwrap_or(0);
        let sign = if exp < 0 { '-' } else { '+' };
        let s = format!("{}E{sign}{:02}", trim(mantissa.to_string()), exp.abs());
        if s.len() <= max_len {
            return s;
        }
        fallback = s;
    }
    fallback
}

/// Split on `;` outside quotes and brackets.
fn split_sections(code: &str) -> Vec<&str> {
    let mut sections = Vec::new();
    let (mut start, mut in_quote, mut in_bracket, mut escaped) = (0, false, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !in_quote => escaped = true,
            '"' => in_quote = !in_quote,
            '[' if !in_quote => in_bracket = true,
            ']' if !in_quote => in_bracket = false,
            ';' if !in_quote && !in_bracket => {
                sections.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    sections.push(&code[start..]);
    sections
}

/// Reduce one section to what the formatters understand: drop padding (`_x`)
/// and fills (`*x`), turn escapes (`\x`) and currency tags (`[$€-407]`) into
/// quoted literals, keep elapsed-time brackets as plain tokens, and pull out a
/// `[Red]`-style color.
fn clean_section(section: &str) -> (String, Option<Rgb>) {
    let mut out = String::new();
    let mut color = None;
    let mut chars = section.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push('"');
                for q in chars.by_ref() {
                    out.push(q);
                    if q == '"' {
                        break;
                    }
                }
            }
            '\\' => {
                if let Some(lit) = chars.next() {
                    out.push('"');
                    out.push(lit);
                    out.push('"');
                }
            }
            '_' => {
                chars.next();
                out.push_str("\" \"");
            }
            '*' => {
                chars.next();
            }
            '[' => {
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == ']' {
                        break;
                    }
                    tag.push(t);
                }
                let lower = tag.to_ascii_lowercase();
                if let Some(rest) = tag.strip_prefix('$') {
                    let symbol = rest.split('-').next().unwrap_or("");
                    if !symbol.is_empty() {
                        out.push('"');
                        out.push_str(symbol);
                        out.push('"');
                    }
                } else if !lower.is_empty() && lower.chars().all(|c| matches!(c, 'h' | 'm' | 's')) {
                    // Elapsed time, e.g. [h]:mm; the tokenizer reads it back.
                    out.push('[');
                    out.push_str(&tag);
                    out.push(']');
                } else if let Some(rgb) = named_color(&lower) {
                    color = Some(rgb);
                }
                // Conditions like [>100] and locale tags are ignored.
            }
            _ => out.push(c),
        }
    }
    (out, color)
}

fn named_color(name: &str) -> Option<Rgb> {
    Some(match name {
        "black" => Rgb(0, 0, 0),
        "blue" => Rgb(0, 0, 255),
        "cyan" => Rgb(0, 255, 255),
        "green" => Rgb(0, 128, 0),
        "magenta" => Rgb(255, 0, 255),
        "red" => Rgb(255, 0, 0),
        "white" => Rgb(255, 255, 255),
        "yellow" => Rgb(255, 255, 0),
        _ => return None,
    })
}

fn unquoted(body: &str) -> String {
    let mut out = String::new();
    let mut in_quote = false;
    for c in body.chars() {
        if c == '"' {
            in_quote = !in_quote;
        } else if !in_quote {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

fn is_datetime(body: &str) -> bool {
    let b = unquoted(body);
    if b.contains("general") {
        return false;
    }
    b.chars().any(|c| matches!(c, 'y' | 'd' | 'h' | 's'))
        || (b.contains('m') && !b.contains('0') && !b.contains('#'))
}

fn is_fraction(body: &str) -> bool {
    let b = unquoted(body);
    b.contains('/') && b.contains('?')
}

/// `# ?/?`, `# ??/??`, `?/8`: a whole part (if the code has one) and a
/// fraction with the closest denominator that fits.
fn format_fraction(v: f64, body: &str) -> String {
    let b = unquoted(body);
    let (whole_spec, frac_spec) = match b.trim().rsplit_once(' ') {
        Some((w, f)) if w.contains('#') || w.contains('0') => (true, f.to_string()),
        _ => (false, b.trim().to_string()),
    };
    let den_spec = frac_spec.split('/').nth(1).unwrap_or("?");
    let fixed: Option<u32> = den_spec.trim().parse().ok();
    let max_den = fixed.unwrap_or_else(|| {
        let digits = den_spec
            .chars()
            .filter(|c| matches!(c, '?' | '#' | '0'))
            .count();
        10u32.pow(digits.clamp(1, 4) as u32) - 1
    });

    let sign = if v < 0.0 { "-" } else { "" };
    let v = v.abs();
    let mut whole = if whole_spec { v.trunc() } else { 0.0 };
    let frac = v - whole;
    let (mut num, mut den) = match fixed {
        Some(d) => ((frac * d as f64).round() as u64, d as u64),
        None => (1..=max_den as u64)
            .map(|d| ((frac * d as f64).round() as u64, d))
            .min_by(|a, b| {
                let ea = (frac - a.0 as f64 / a.1 as f64).abs();
                let eb = (frac - b.0 as f64 / b.1 as f64).abs();
                ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or((0, 1)),
    };
    if whole_spec && num == den {
        whole += 1.0;
        num = 0;
    }
    if num == 0 {
        den = 1;
    }
    match (whole_spec, whole as u64, num) {
        (true, w, 0) => format!("{sign}{w}"),
        (true, 0, n) => format!("{sign}{n}/{den}"),
        (true, w, n) => format!("{sign}{w} {n}/{den}"),
        (false, _, n) => format!("{sign}{n}/{den}"),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Lit(String),
    Year(usize),
    Month(usize),
    Minute(usize),
    Day(usize),
    Hour(usize),
    /// `[h]`, `[mm]`, `[ss]`: total hours, minutes or seconds.
    Elapsed {
        unit: char,
        len: usize,
    },
    Second(usize),
    FracSecond(usize),
    AmPm {
        short: bool,
    },
}

fn tokenize_datetime(body: &str) -> Vec<Tok> {
    let chars: Vec<char> = body.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let run = |i: usize, c: char| {
        chars[i..]
            .iter()
            .take_while(|x| x.eq_ignore_ascii_case(&c))
            .count()
    };
    while i < chars.len() {
        let c = chars[i];
        let lower = c.to_ascii_lowercase();
        let rest: String = chars[i..].iter().collect::<String>().to_ascii_lowercase();
        if c == '"' {
            let lit: String = chars[i + 1..].iter().take_while(|&&q| q != '"').collect();
            i += lit.chars().count() + 2;
            toks.push(Tok::Lit(lit));
        } else if c == '[' {
            let tag: String = chars[i + 1..].iter().take_while(|&&q| q != ']').collect();
            i += tag.chars().count() + 2;
            if let Some(unit) = tag.chars().next() {
                toks.push(Tok::Elapsed {
                    unit: unit.to_ascii_lowercase(),
                    len: tag.chars().count(),
                });
            }
        } else if rest.starts_with("am/pm") {
            toks.push(Tok::AmPm { short: false });
            i += 5;
        } else if rest.starts_with("a/p") {
            toks.push(Tok::AmPm { short: true });
            i += 3;
        } else if matches!(lower, 'y' | 'm' | 'd' | 'h' | 's') {
            let n = run(i, c);
            toks.push(match lower {
                'y' => Tok::Year(n),
                'm' => Tok::Month(n),
                'd' => Tok::Day(n),
                'h' => Tok::Hour(n),
                _ => Tok::Second(n),
            });
            i += n;
        } else if c == '.'
            && matches!(toks.last(), Some(Tok::Second(_)))
            && chars.get(i + 1) == Some(&'0')
        {
            let n = run(i + 1, '0');
            toks.push(Tok::FracSecond(n));
            i += n + 1;
        } else {
            toks.push(Tok::Lit(c.to_string()));
            i += 1;
        }
    }

    // "m" means minutes right after an hour or right before a second.
    let parts: Vec<usize> = (0..toks.len())
        .filter(|&k| !matches!(toks[k], Tok::Lit(_)))
        .collect();
    for (p, &k) in parts.iter().enumerate() {
        if let Tok::Month(n) = toks[k] {
            let after_hour = p > 0
                && matches!(
                    toks[parts[p - 1]],
                    Tok::Hour(_) | Tok::Elapsed { unit: 'h', .. }
                );
            let before_second = parts
                .get(p + 1)
                .is_some_and(|&next| matches!(toks[next], Tok::Second(_)));
            if (after_hour || before_second) && n <= 2 {
                toks[k] = Tok::Minute(n);
            }
        }
    }
    toks
}

fn format_datetime(serial: f64, body: &str) -> String {
    let toks = tokenize_datetime(body);

    let days = serial.floor();
    let mut secs_total = ((serial - days) * 86_400.0 * 1000.0).round() / 1000.0;
    let mut day_serial = days;
    if secs_total >= 86_400.0 {
        secs_total -= 86_400.0;
        day_serial += 1.0;
    }
    let (year, month, day) = serial_to_date(day_serial);
    let whole_secs = secs_total.floor() as u64;
    let (h, m, s) = (whole_secs / 3600, (whole_secs / 60) % 60, whole_secs % 60);
    let has_ampm = toks.iter().any(|t| matches!(t, Tok::AmPm { .. }));
    let weekday = ((day_serial as i64 - 1).rem_euclid(7)) as usize;
    const DAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_name = MONTHS[(month.clamp(1, 12) - 1) as usize];

    let mut out = String::new();
    for t in &toks {
        match t {
            Tok::Lit(s) => out.push_str(s),
            Tok::Year(n) if *n <= 2 => out.push_str(&format!("{:02}", year % 100)),
            Tok::Year(_) => out.push_str(&format!("{year:04}")),
            Tok::Month(1) => out.push_str(&month.to_string()),
            Tok::Month(2) => out.push_str(&format!("{month:02}")),
            Tok::Month(3) => out.push_str(&month_name[..3]),
            Tok::Month(5) => out.push_str(&month_name[..1]),
            Tok::Month(_) => out.push_str(month_name),
            Tok::Day(1) => out.push_str(&day.to_string()),
            Tok::Day(2) => out.push_str(&format!("{day:02}")),
            Tok::Day(3) => out.push_str(&DAYS[weekday][..3]),
            Tok::Day(_) => out.push_str(DAYS[weekday]),
            Tok::Elapsed { unit, len } => {
                let total = days as u64 * 86_400 + whole_secs;
                let value = match unit {
                    'h' => total / 3600,
                    'm' => total / 60,
                    _ => total,
                };
                out.push_str(&format!("{value:0len$}"));
            }
            Tok::Hour(len) => {
                let hours = if has_ampm {
                    match h % 12 {
                        0 => 12,
                        x => x,
                    }
                } else {
                    h
                };
                if *len >= 2 {
                    out.push_str(&format!("{hours:02}"));
                } else {
                    out.push_str(&hours.to_string());
                }
            }
            Tok::Minute(1) => out.push_str(&m.to_string()),
            Tok::Minute(_) => out.push_str(&format!("{m:02}")),
            Tok::Second(1) => out.push_str(&s.to_string()),
            Tok::Second(_) => out.push_str(&format!("{s:02}")),
            Tok::FracSecond(n) => {
                let frac = secs_total - secs_total.floor();
                let digits = format!("{:.*}", *n, frac);
                out.push_str(digits.trim_start_matches('0'));
            }
            Tok::AmPm { short } => {
                let pm = h >= 12;
                out.push_str(match (short, pm) {
                    (false, false) => "AM",
                    (false, true) => "PM",
                    (true, false) => "A",
                    (true, true) => "P",
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(n: f64, code: &str) -> String {
        format_number(n, code).text
    }

    #[test]
    fn numbers() {
        assert_eq!(f(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(f(-1234.5, "#,##0.00"), "-1,234.50");
        assert_eq!(f(0.256, "0.0%"), "25.6%");
        assert_eq!(f(1234.5, "$#,##0.00"), "$1,234.50");
        assert_eq!(f(-1234.5, "$#,##0.00"), "-$1,234.50");
        assert_eq!(f(12345.0, "0.00E+00"), "1.23E+04");
        assert_eq!(f(3.0, "0"), "3");
        assert_eq!(f(-0.001, "0.00"), "0.00");
        assert_eq!(f(42.0, "General"), "42");
        assert_eq!(f(5.0, "0 \"units\""), "5 units");
    }

    #[test]
    fn sections_and_colors() {
        let code = "$#,##0.00_);[Red]($#,##0.00)";
        assert_eq!(f(1234.5, code), "$1,234.50 ");
        let neg = format_number(-1234.5, code);
        assert_eq!(neg.text, "($1,234.50)");
        assert_eq!(neg.color, Some(Rgb(255, 0, 0)));
        assert_eq!(f(0.0, "0;-0;\"zero\""), "zero");
        assert_eq!(f(5.0, "[$€-407]#,##0.00"), "€5.00");
        assert_eq!(f(5.0, "[$-409]0.0"), "5.0");
    }

    #[test]
    fn dates_and_times() {
        // 45000 is 2023-03-15, a Wednesday.
        assert_eq!(f(45000.0, "yyyy-mm-dd"), "2023-03-15");
        assert_eq!(f(45000.0, "m/d/yyyy"), "3/15/2023");
        assert_eq!(f(45000.0, "d-mmm-yy"), "15-Mar-23");
        assert_eq!(f(45000.0, "dddd, mmmm d"), "Wednesday, March 15");
        assert_eq!(f(45000.75, "h:mm AM/PM"), "6:00 PM");
        assert_eq!(f(45000.5, "hh:mm:ss"), "12:00:00");
        assert_eq!(
            f(45000.0 + 1.0 / 24.0 + 5.0 / 1440.0, "m/d/yyyy h:mm"),
            "3/15/2023 1:05"
        );
        assert_eq!(f(1.5, "[h]:mm:ss"), "36:00:00");
        assert_eq!(f(0.25, "mm:ss"), "00:00");
    }

    #[test]
    fn fractions() {
        assert_eq!(f(1.5, "# ?/?"), "1 1/2");
        assert_eq!(f(0.75, "# ?/?"), "3/4");
        assert_eq!(f(2.0, "# ?/?"), "2");
        assert_eq!(f(0.3125, "# ??/??"), "5/16");
        assert_eq!(f(0.5, "?/8"), "4/8");
    }

    #[test]
    fn detects_date_formats() {
        assert!(is_date_format("yyyy-mm-dd"));
        assert!(is_date_format("[h]:mm:ss"));
        assert!(is_date_format("[$-409]mmmm d, yyyy;@"));
        assert!(!is_date_format("#,##0.00"));
        assert!(!is_date_format("0.0%"));
        assert!(!is_date_format("\"days\" 0"));
    }

    #[test]
    fn builtin_ids() {
        assert_eq!(builtin_number_format(0), None);
        assert_eq!(builtin_number_format(14), Some("m/d/yyyy"));
        assert_eq!(f(45000.0, builtin_number_format(14).unwrap()), "3/15/2023");
    }

    #[test]
    fn general_format_fits_width() {
        assert_eq!(format_general(1391.6666666666667, 11), "1391.666667");
        assert_eq!(format_general(0.26, 11), "0.26");
        assert_eq!(format_general(27400.0, 11), "27400");
        assert_eq!(format_general(-2.5, 11), "-2.5");
        assert_eq!(format_general(0.1 + 0.2, 11), "0.3");
        assert_eq!(format_general(0.0, 11), "0");
    }

    #[test]
    fn general_format_switches_to_scientific() {
        assert_eq!(format_general(1e15, 11), "1E+15");
        assert_eq!(format_general(123456789012345.0, 11), "1.23457E+14");
        assert_eq!(format_general(1.5e-12, 11), "1.5E-12");
        assert_eq!(format_general(12345678901.0, 11), "12345678901");
    }
}
