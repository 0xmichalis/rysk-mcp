//! Exact conversions between Rysk's integer base units and human-readable numbers,
//! plus the UTC calendar helpers the tools need for expiries.
//!
//! Rysk scales most option fields (strike, quantity, price, premium, asset price)
//! to 18 decimals, RFQ strikes and settlement prices to 8, and token payouts to the
//! token's own decimals. Conversions go through decimal strings, so values far
//! beyond `u128` never overflow and inputs like `0.1` stay exact.

/// Decimals of option quantities, prices, premiums and position strikes.
pub const WAD: u32 = 18;
/// Decimals of RFQ strikes and settlement prices.
pub const PRICE_DECIMALS: u32 = 8;

/// Interpret an integer string as a fixed-point number with `decimals` places.
/// Returns `None` when `raw` is not a non-negative integer.
pub fn from_base_units(raw: &str, decimals: u32) -> Option<f64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let decimals = decimals as usize;
    let padded = format!("{raw:0>width$}", width = decimals + 1);
    let (int, frac) = padded.split_at(padded.len() - decimals);
    format!("{int}.{frac}").parse().ok()
}

/// Scale a positive number to an integer string with `decimals` places, exactly as
/// written: `75.5` with 8 decimals is `"7550000000"`. Rejects values that need more
/// precision than `decimals` allows rather than rounding them.
pub fn to_base_units(value: f64, decimals: u32) -> Result<String, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{value} must be a positive number"));
    }
    // f64's Display is the shortest round-trip form and never uses an exponent.
    let text = value.to_string();
    let (int, frac) = text.split_once('.').unwrap_or((&text, ""));
    if frac.len() > decimals as usize {
        return Err(format!("{value} has more than {decimals} decimal places"));
    }
    let digits = format!("{int}{frac:0<width$}", width = decimals as usize);
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() { "0" } else { trimmed }.to_string())
}

/// Format a unix timestamp (seconds) as a UTC date, `YYYY-MM-DD`.
pub fn utc_date(timestamp: i64) -> String {
    let (y, m, d) = civil_from_days(timestamp.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Format a unix timestamp (seconds) as an RFC 3339 UTC datetime.
pub fn utc_datetime(timestamp: i64) -> String {
    let secs = timestamp.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        utc_date(timestamp),
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Parse a `YYYY-MM-DD` date into the unix timestamp of its UTC midnight.
pub fn parse_utc_date(date: &str) -> Option<i64> {
    let mut parts = date.trim().splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's proleptic Gregorian algorithms:
// https://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_units_scale_down_exactly() {
        assert_eq!(
            from_base_units("542508476000000000000", WAD),
            Some(542.508476)
        );
        assert_eq!(
            from_base_units("7841003462", PRICE_DECIMALS),
            Some(78.41003462)
        );
        assert_eq!(from_base_units("100000", 6), Some(0.1));
        assert_eq!(from_base_units("5", 18), Some(5e-18));
        assert_eq!(from_base_units("0", 18), Some(0.0));
        assert_eq!(from_base_units("42", 0), Some(42.0));
    }

    #[test]
    fn base_units_reject_non_integers() {
        assert_eq!(from_base_units("", 18), None);
        assert_eq!(from_base_units("-1", 18), None);
        assert_eq!(from_base_units("1.5", 18), None);
        assert_eq!(from_base_units("0x10", 18), None);
    }

    #[test]
    fn values_scale_up_exactly() {
        assert_eq!(to_base_units(75.0, 8).unwrap(), "7500000000");
        assert_eq!(to_base_units(75.5, 8).unwrap(), "7550000000");
        assert_eq!(to_base_units(0.85, 8).unwrap(), "85000000");
        assert_eq!(to_base_units(0.1, 18).unwrap(), "100000000000000000");
        assert_eq!(to_base_units(25.0, 18).unwrap(), "25000000000000000000");
        assert_eq!(to_base_units(0.00000001, 8).unwrap(), "1");
        assert_eq!(to_base_units(1e21, 0).unwrap(), "1000000000000000000000");
    }

    #[test]
    fn values_needing_more_precision_or_non_positive_are_rejected() {
        assert!(
            to_base_units(0.000000001, 8)
                .unwrap_err()
                .contains("8 decimal places")
        );
        assert!(to_base_units(0.0, 8).is_err());
        assert!(to_base_units(-1.0, 8).is_err());
        assert!(to_base_units(f64::NAN, 8).is_err());
        assert!(to_base_units(f64::INFINITY, 8).is_err());
    }

    #[test]
    fn timestamps_format_as_utc() {
        assert_eq!(utc_date(0), "1970-01-01");
        // A Rysk expiry: 2026-11-13 08:00 UTC.
        assert_eq!(utc_date(1_794_556_800), "2026-11-13");
        assert_eq!(utc_datetime(1_794_556_800), "2026-11-13T08:00:00Z");
        assert_eq!(utc_date(951_782_400), "2000-02-29");
        assert_eq!(utc_datetime(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn dates_parse_to_utc_midnight() {
        assert_eq!(parse_utc_date("1970-01-01"), Some(0));
        assert_eq!(parse_utc_date("2026-11-13"), Some(1_794_528_000));
        assert_eq!(parse_utc_date("2000-02-29"), Some(951_782_400));
        assert_eq!(parse_utc_date("2026-02-29"), None);
        assert_eq!(parse_utc_date("2026-13-01"), None);
        assert_eq!(parse_utc_date("2026-11"), None);
        assert_eq!(parse_utc_date("13NOV26"), None);
    }

    #[test]
    fn date_round_trips_over_a_wide_range() {
        // Years 1 through 9999; negative years are out of scope for expiries.
        for days in (-719_162..2_932_896).step_by(997) {
            let ts = days * 86_400;
            assert_eq!(parse_utc_date(&utc_date(ts)), Some(ts), "days {days}");
        }
    }
}
