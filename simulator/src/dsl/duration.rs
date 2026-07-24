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
    let start = 0; // keep optional leading sign in the numeric token
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.') {
        i += 1;
    }
    if i == start || (i == 1 && (bytes[0] == b'+' || bytes[0] == b'-')) {
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

    #[test]
    fn parses_aliases_underscores_and_bare_ns() {
        assert_eq!(parse_duration_ns(" 2_000 ").unwrap(), 2_000);
        assert_eq!(parse_duration_ns("1sec").unwrap(), 1_000_000_000);
        assert_eq!(parse_duration_ns("2secs").unwrap(), 2_000_000_000);
        assert_eq!(parse_duration_ns("3msec").unwrap(), 3_000_000);
        assert_eq!(parse_duration_ns("4usec").unwrap(), 4_000);
        assert_eq!(parse_duration_ns("1.5s").unwrap(), 1_500_000_000);
        assert_eq!(parse_duration_ns("+10ms").unwrap(), 10_000_000);
        assert_eq!(parse_duration_ns("1µs").unwrap(), 1_000);
    }

    #[test]
    fn rejects_invalid_durations() {
        assert!(parse_duration_ns("").is_err());
        assert!(parse_duration_ns("   ").is_err());
        assert!(parse_duration_ns("ms").is_err());
        assert!(parse_duration_ns("1x").is_err());
        assert!(parse_duration_ns("-1ms").is_err());
        assert!(parse_duration_ns("nanms").is_err());
        assert!(parse_duration_ns("1.2.3ms").is_err());
    }
}
