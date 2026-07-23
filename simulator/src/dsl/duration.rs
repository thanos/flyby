//! Human-friendly duration parsing (`"5s"`, `"1ms"`, `"500us"`, `"250ns"`).

use flyby_core::{Error, Result};

/// Parse a duration string into nanoseconds.
///
/// Accepts integer or decimal values with a unit suffix:
/// `ns`, `us`/`µs`, `ms`, `s`. Bare integers are treated as nanoseconds.
pub fn parse_duration_ns(raw: &str) -> Result<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(Error::config("empty duration"));
    }

    let (num_raw, unit) = split_num_unit(s)?;
    let num: String = num_raw.chars().filter(|c| *c != '_').collect();
    let factor: f64 = match unit {
        "" | "ns" => 1.0,
        "us" | "µs" | "usec" => 1_000.0,
        "ms" | "msec" => 1_000_000.0,
        "s" | "sec" | "secs" => 1_000_000_000.0,
        other => {
            return Err(Error::config(format!(
                "unknown duration unit '{other}' in '{raw}' (use ns/us/ms/s)"
            )));
        }
    };

    let value: f64 = num
        .parse()
        .map_err(|_| Error::config(format!("invalid duration number '{num}' in '{raw}'")))?;
    if !value.is_finite() || value < 0.0 {
        return Err(Error::config(format!(
            "duration must be finite and ≥ 0: '{raw}'"
        )));
    }
    Ok((value * factor).round() as u64)
}

fn split_num_unit(s: &str) -> Result<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.') {
        i += 1;
    }
    if i == start {
        return Err(Error::config(format!("duration missing number: '{s}'")));
    }
    let num = &s[start..i];
    let unit = s[i..].trim();
    Ok((num, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_units() {
        assert_eq!(parse_duration_ns("1s").unwrap(), 1_000_000_000);
        assert_eq!(parse_duration_ns("5ms").unwrap(), 5_000_000);
        assert_eq!(parse_duration_ns("500us").unwrap(), 500_000);
        assert_eq!(parse_duration_ns("250ns").unwrap(), 250);
        assert_eq!(parse_duration_ns("1.5ms").unwrap(), 1_500_000);
    }
}
