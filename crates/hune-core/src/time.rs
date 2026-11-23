//! UTC times with millisecond precision (5.2 `UtcMillis`). The text form is
//! `YYYY-MM-DDTHH:MM:SS.mmmZ` (RFC 3339 in UTC with milliseconds, 6.2).
//!
//! This module reads no clock (R1). The caller gives the time in epoch milliseconds.

use std::fmt;

/// A UTC time in milliseconds since 1970-01-01T00:00:00Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcMillis(i64);

impl UtcMillis {
    /// Makes a time from epoch milliseconds.
    pub fn from_millis(ms: i64) -> Self {
        UtcMillis(ms)
    }

    /// Gives the epoch milliseconds.
    pub fn as_millis(&self) -> i64 {
        self.0
    }

    /// Gives the text form `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    pub fn to_rfc3339(&self) -> String {
        let ms = self.0.rem_euclid(1000);
        let secs = self.0.div_euclid(1000);
        let days = secs.div_euclid(86_400);
        let sod = secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", y, m, d, sod / 3600, (sod % 3600) / 60, sod % 60, ms)
    }

    /// Reads the text form `YYYY-MM-DDTHH:MM:SS.mmmZ` exactly. Another form gives `None`.
    pub fn parse_rfc3339(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 24
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
            || b[19] != b'.'
            || b[23] != b'Z'
        {
            return None;
        }
        let num = |r: std::ops::Range<usize>| -> Option<i64> {
            let mut v: i64 = 0;
            for &c in &b[r] {
                if !c.is_ascii_digit() {
                    return None;
                }
                v = v * 10 + i64::from(c - b'0');
            }
            Some(v)
        };
        let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (h, mi, se, ms) = (num(11..13)?, num(14..16)?, num(17..19)?, num(20..23)?);
        if !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) || h > 23 || mi > 59 || se > 59 {
            return None;
        }
        let days = days_from_civil(y, mo, d);
        Some(UtcMillis((days * 86_400 + h * 3600 + mi * 60 + se) * 1000 + ms))
    }
}

impl fmt::Display for UtcMillis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl serde::Serialize for UtcMillis {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_rfc3339())
    }
}

/// Tells if `y` is a leap year of the proleptic Gregorian calendar.
pub fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Gives the number of days of month `m` (1 to 12) of year `y`.
pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Gives the days since 1970-01-01 of a civil date (algorithm of H. Hinnant).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Gives the civil date (year, month, day) of a count of days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let t = UtcMillis::from_millis(1_794_211_200_125);
        let s = t.to_rfc3339();
        assert_eq!(UtcMillis::parse_rfc3339(&s), Some(t));
        assert_eq!(UtcMillis::from_millis(0).to_rfc3339(), "1970-01-01T00:00:00.000Z");
        assert_eq!(days_from_civil(2026, 11, 9) * 86_400, 1_794_182_400);
        assert!(UtcMillis::parse_rfc3339("2026-02-30T00:00:00.000Z").is_none());
    }
}
