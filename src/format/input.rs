//! Recognize typed numbers the way Excel does: `12%`, `$1,234.50`, `(5)`,
//! `2026-10-03`, `10/3/2026`, `14:30`, `2:30 PM`.

use crate::calc::functions::date_to_serial;

/// Parse what a user typed into a cell. Returns the number and, for input
/// such as `12%` or a date, the format code that displays it the same way.
/// Plain numbers return no format. Anything else is `None` (text).
pub fn parse_typed_number(input: &str) -> Option<(f64, Option<&'static str>)> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<f64>() {
        // Rust also accepts "inf" and "NaN"; Excel treats those as text.
        return n.is_finite().then_some((n, None));
    }
    parse_decorated(s)
        .or_else(|| parse_date_time(s))
        .filter(|(n, _)| n.is_finite())
}

/// Percent, currency, thousands separators and accounting negatives.
fn parse_decorated(s: &str) -> Option<(f64, Option<&'static str>)> {
    let (negative, s) = if let Some(inner) = s.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        (true, inner.trim())
    } else if let Some(rest) = s.strip_prefix('-') {
        (true, rest.trim_start())
    } else {
        (false, s)
    };
    let (percent, s) = match s.strip_suffix('%') {
        Some(rest) => (true, rest.trim_end()),
        None => (false, s),
    };
    let (currency, s) = match s.strip_prefix('$') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };
    // "-$5" and "$-5" both mean minus five dollars.
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) if !negative => (true, rest),
        _ => (negative, s),
    };
    if s.is_empty() || (percent && currency) {
        return None;
    }

    let thousands = s.contains(',');
    if thousands && !valid_thousands(s) {
        return None;
    }
    let digits: String = s.chars().filter(|&c| c != ',').collect();
    if !digits.chars().all(|c| c.is_ascii_digit() || c == '.') || digits.matches('.').count() > 1 {
        return None;
    }
    let mut n: f64 = digits.parse().ok()?;
    if negative {
        n = -n;
    }
    let decimals = digits.contains('.');
    let format = if percent {
        n /= 100.0;
        Some(if decimals { "0.00%" } else { "0%" })
    } else if currency {
        Some(if decimals { "$#,##0.00" } else { "$#,##0" })
    } else if thousands {
        Some(if decimals { "#,##0.00" } else { "#,##0" })
    } else if negative {
        // "(5)" with no other decoration.
        None
    } else {
        return None;
    };
    Some((n, format))
}

/// Groups of three digits after the first comma, e.g. `1,234,567.8`.
fn valid_thousands(s: &str) -> bool {
    let int_part = s.split('.').next().unwrap_or(s);
    let mut groups = int_part.split(',');
    let first = groups.next().unwrap_or("");
    !first.is_empty() && first.len() <= 3 && groups.all(|g| g.len() == 3)
}

fn parse_date_time(s: &str) -> Option<(f64, Option<&'static str>)> {
    let lower = s.to_ascii_lowercase();
    let (date_part, time_part) = match lower.split_once(' ') {
        // "2:30 pm" is a time with a meridiem, not a date and a time.
        Some((a, b)) if !a.contains(':') => (Some(a), Some(b.trim())),
        _ if lower.contains(':') => (None, Some(lower.as_str())),
        _ => (Some(lower.as_str()), None),
    };

    let date = match date_part {
        Some(d) => Some(parse_date(d)?),
        None => None,
    };
    let time = match time_part {
        Some(t) => Some(parse_time(t)?),
        None => None,
    };

    Some(match (date, time) {
        (Some((serial, iso)), None) => (serial, Some(if iso { "yyyy-mm-dd" } else { "m/d/yyyy" })),
        (None, Some((frac, fmt))) => (frac, Some(fmt)),
        (Some((serial, iso)), Some((frac, _))) => (
            serial + frac,
            Some(if iso {
                "yyyy-mm-dd h:mm"
            } else {
                "m/d/yyyy h:mm"
            }),
        ),
        (None, None) => return None,
    })
}

/// `yyyy-mm-dd` (returns iso = true) or `m/d/yyyy` / `m/d/yy`.
fn parse_date(s: &str) -> Option<(f64, bool)> {
    let nums = |sep: char| -> Option<Vec<i32>> {
        let parts: Vec<&str> = s.split(sep).collect();
        (parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())))
        .then(|| parts.iter().filter_map(|p| p.parse().ok()).collect())
    };
    let (year, month, day, iso) = if let Some(v) = nums('-').filter(|v| v.len() == 3) {
        if s.split('-').next()?.len() != 4 {
            return None;
        }
        (v[0], v[1], v[2], true)
    } else {
        let v = nums('/').filter(|v| v.len() == 3)?;
        let year = if s.rsplit('/').next()?.len() <= 2 {
            2000 + v[2]
        } else {
            v[2]
        };
        (year, v[0], v[1], false)
    };
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // Excel's calendar has a February 29th, 1900, serial 60.
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 || year == 1900 => 29,
        2 => 28,
        _ => return None,
    };
    if !(1900..=9999).contains(&year) || day < 1 || day > days_in_month {
        return None;
    }
    Some((date_to_serial(year, month, day), iso))
}

/// `h:mm`, `h:mm:ss`, optionally followed by `am`/`pm`.
fn parse_time(s: &str) -> Option<(f64, &'static str)> {
    let (clock, meridiem) = if let Some(c) = s.strip_suffix("am") {
        (c.trim(), Some(false))
    } else if let Some(c) = s.strip_suffix("pm") {
        (c.trim(), Some(true))
    } else {
        (s, None)
    };
    let parts: Vec<&str> = clock.split(':').collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let mut h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let sec: u32 = parts.get(2).map_or(Some(0), |p| p.parse().ok())?;
    if m > 59 || sec > 59 {
        return None;
    }
    match meridiem {
        Some(pm) if (1..=12).contains(&h) => {
            h = h % 12 + if pm { 12 } else { 0 };
        }
        Some(_) => return None,
        None if h > 23 => return None,
        None => {}
    }
    let frac = (h * 3600 + m * 60 + sec) as f64 / 86_400.0;
    let fmt = match (meridiem.is_some(), parts.len() == 3) {
        (true, false) => "h:mm AM/PM",
        (true, true) => "h:mm:ss AM/PM",
        (false, false) => "h:mm",
        (false, true) => "h:mm:ss",
    };
    Some((frac, fmt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::format_number;

    fn show(input: &str) -> Option<String> {
        let (n, fmt) = parse_typed_number(input)?;
        Some(match fmt {
            Some(code) => format_number(n, code).text,
            None => n.to_string(),
        })
    }

    #[test]
    fn plain_numbers_have_no_format() {
        assert_eq!(parse_typed_number("42"), Some((42.0, None)));
        assert_eq!(parse_typed_number("-1.5"), Some((-1.5, None)));
        assert_eq!(parse_typed_number("(5)"), Some((-5.0, None)));
        assert_eq!(parse_typed_number("inf"), None);
        assert_eq!(parse_typed_number("hello"), None);
        assert_eq!(parse_typed_number("1,23"), None);
    }

    #[test]
    fn decorated_numbers_round_trip_their_display() {
        assert_eq!(parse_typed_number("12%"), Some((0.12, Some("0%"))));
        assert_eq!(show("12.5%").as_deref(), Some("12.50%"));
        assert_eq!(show("$1,234.50").as_deref(), Some("$1,234.50"));
        assert_eq!(show("-$5").as_deref(), Some("-$5"));
        assert_eq!(show("1,234,567").as_deref(), Some("1,234,567"));
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(show("2023-03-15").as_deref(), Some("2023-03-15"));
        assert_eq!(parse_typed_number("2023-03-15").map(|p| p.0), Some(45000.0));
        assert_eq!(show("3/15/2023").as_deref(), Some("3/15/2023"));
        assert_eq!(show("14:30").as_deref(), Some("14:30"));
        assert_eq!(show("2:30 PM").as_deref(), Some("2:30 PM"));
        assert_eq!(
            show("2023-03-15 18:00").as_deref(),
            Some("2023-03-15 18:00")
        );
        assert_eq!(parse_typed_number("2023-02-30"), None);
        assert_eq!(parse_typed_number("1/2"), None);
        assert_eq!(parse_typed_number("25:00"), None);
        // Excel's own February 29th, 1900, serial 60, shows as itself.
        assert_eq!(parse_typed_number("2/29/1900").map(|p| p.0), Some(60.0));
        assert_eq!(show("2/29/1900").as_deref(), Some("2/29/1900"));
        assert_eq!(show("3/1/1900").as_deref(), Some("3/1/1900"));
        assert_eq!(parse_typed_number("2/29/1901"), None);
    }
}
