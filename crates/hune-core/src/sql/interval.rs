//! The time model (7.3), the interval rule (7.4) and the interval argument of a group macro (7.23).

use std::fmt;

/// The last millisecond of the ClickHouse type `DateTime` (7.3).
pub const MAX_TIME_MS: i64 = 4_294_967_295_999;
/// The smallest value of `max_data_points` (7.4).
pub const MIN_DATA_POINTS: u32 = 1;
/// The largest value of `max_data_points` (7.4).
pub const MAX_DATA_POINTS: u32 = 10_000;

/// A checked time range in epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    from_ms: i64,
    to_ms: i64,
}

/// The error of a time range that breaks 7.3. The API code is `invalid_time_range`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRangeError {
    /// The English message.
    pub message: String,
}

impl fmt::Display for TimeRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TimeRangeError {}

/// Checks `0 <= from_ms < to_ms <= 4294967295999` (7.3).
pub fn check_time_range(from_ms: i64, to_ms: i64) -> Result<TimeRange, TimeRangeError> {
    if from_ms < 0 || to_ms > MAX_TIME_MS || from_ms >= to_ms {
        return Err(TimeRangeError {
            message: format!(
                "expected 0 <= from_ms < to_ms <= {MAX_TIME_MS}, found from_ms {from_ms} and to_ms {to_ms}"
            ),
        });
    }
    Ok(TimeRange { from_ms, to_ms })
}

impl TimeRange {
    /// The start of the range in epoch milliseconds.
    pub fn from_ms(&self) -> i64 {
        self.from_ms
    }

    /// The end of the range in epoch milliseconds.
    pub fn to_ms(&self) -> i64 {
        self.to_ms
    }

    /// The length of the range in milliseconds.
    pub fn length_ms(&self) -> i64 {
        self.to_ms - self.from_ms
    }

    /// `F` of 7.10: the start in Unix seconds, rounded down.
    pub fn from_seconds(&self) -> i64 {
        self.from_ms / 1000
    }

    /// `T` of 7.10: the end in Unix seconds, rounded down.
    pub fn to_seconds(&self) -> i64 {
        self.to_ms / 1000
    }
}

/// The error of a `max_data_points` outside 1 to 10,000. The API code is `bad_request`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPointsError {
    /// The value that failed the check.
    pub value: u32,
}

impl fmt::Display for DataPointsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "max_data_points must be {MIN_DATA_POINTS} to {MAX_DATA_POINTS}, found {}", self.value)
    }
}

impl std::error::Error for DataPointsError {}

/// The interval of 7.4: `I` in milliseconds and `S` in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    /// `I`: the interval in milliseconds. The frame gives it as `interval_ms` (7.16).
    pub ms: u64,
    /// `S`: `max(1, floor(I / 1000))`, the interval in seconds for the macros (7.10).
    pub seconds: u64,
}

/// The rows of the table of 7.4: `R` below the bound gives the value, in milliseconds. The left
/// column comes first, then the right column.
const ROUND_TABLE: [(u64, u64); 29] = [
    (10, 1),
    (15, 10),
    (35, 20),
    (75, 50),
    (150, 100),
    (350, 200),
    (750, 500),
    (1_500, 1_000),
    (3_500, 2_000),
    (7_500, 5_000),
    (12_500, 10_000),
    (17_500, 15_000),
    (25_000, 20_000),
    (45_000, 30_000),
    (90_000, 60_000),
    (210_000, 120_000),
    (450_000, 300_000),
    (750_000, 600_000),
    (1_050_000, 900_000),
    (1_500_000, 1_200_000),
    (2_700_000, 1_800_000),
    (5_400_000, 3_600_000),
    (9_000_000, 7_200_000),
    (16_200_000, 10_800_000),
    (32_400_000, 21_600_000),
    (86_400_000, 43_200_000),
    (604_800_000, 86_400_000),
    (1_814_400_000, 604_800_000),
    (3_628_800_000, 2_592_000_000),
];
/// The value of the last row of the table, which has no bound.
const ROUND_LAST: u64 = 31_536_000_000;

/// `round_interval(R)` of 7.4 for `R = length_ms / data_points`. The function does not round `R`.
///
/// The comparison `R < bound` is the integer comparison `length_ms < bound * data_points`.
pub(super) fn round_interval(length_ms: u64, data_points: u64) -> u64 {
    for (bound, value) in ROUND_TABLE {
        if length_ms < bound * data_points {
            return value;
        }
    }
    ROUND_LAST
}

/// Computes the interval of 7.4. `max_data_points` MUST be 1 to 10,000, else the result is an
/// error. `min_interval_ms` is `L`, and 1 when the panel has no `min_interval`.
pub fn compute_interval(
    range: &TimeRange,
    max_data_points: u32,
    min_interval_ms: u64,
) -> Result<Interval, DataPointsError> {
    if !(MIN_DATA_POINTS..=MAX_DATA_POINTS).contains(&max_data_points) {
        return Err(DataPointsError { value: max_data_points });
    }
    let rounded = round_interval(range.length_ms().unsigned_abs(), u64::from(max_data_points));
    let ms = min_interval_ms.max(rounded);
    Ok(Interval { ms, seconds: (ms / 1000).max(1) })
}

/// Computes `I` of 7.4 in milliseconds. See [`compute_interval`].
pub fn interval_ms(range: &TimeRange, max_data_points: u32, min_interval_ms: u64) -> Result<u64, DataPointsError> {
    compute_interval(range, max_data_points, min_interval_ms).map(|i| i.ms)
}

/// The smallest interval of a group macro, in milliseconds (7.23).
pub const MIN_GROUP_INTERVAL_MS: u64 = 1;
/// The largest interval of a group macro: 365 days, in milliseconds (7.23).
pub const MAX_GROUP_INTERVAL_MS: u64 = 365 * 86_400_000;
/// The most digits that the number of an interval string can have. A longer number is over the limit.
const MAX_INTERVAL_DIGITS: usize = 12;

/// The cause of a fault in the interval argument of a group macro (7.23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupIntervalFault {
    /// The argument is not one string token, and it is not `$__interval` or `${__interval}`.
    Shape,
    /// The string does not match `<n><unit>` of 7.23.
    Grammar,
    /// The interval is under 1 ms or over 365 days.
    Range,
    /// The argument is the built-in `$__interval`, and the request has no interval (a variable query).
    NoInterval,
}

/// The error of the interval argument of a group macro. At run time, a fault in a text that a
/// variable gave is `422 variable_value_refused` with the reason "The value is not an interval of
/// 7.23". Another fault is `422 sql_invalid` (7.23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupIntervalError {
    /// The cause.
    pub fault: GroupIntervalFault,
    /// The English message.
    pub message: String,
}

impl fmt::Display for GroupIntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GroupIntervalError {}

fn group_error(fault: GroupIntervalFault, message: &str) -> GroupIntervalError {
    GroupIntervalError { fault, message: message.to_owned() }
}

/// The interval argument after the check of its form (7.23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupInterval {
    /// The built-in `$__interval`: `I` of 7.4. The form 2 and the form 3 of 7.23.
    Builtin,
    /// A string `'<n><unit>'`, in milliseconds. The form 1 of 7.23.
    Millis(u64),
}

/// The text of a built-in `$__interval` as the argument names it, without the quote marks.
fn is_builtin_text(text: &str) -> bool {
    text == "$__interval" || text == "${__interval}"
}

/// Reads the interval argument of a group macro (7.23). `arg` is the text of the second argument
/// after the expansion of its placeholders, without the blank tokens at both ends.
///
/// The text MUST be exactly one of: a string `'<n><unit>'`; `$__interval` or `${__interval}`; the
/// string `'$__interval'`. The number is a whole number of 1 or more, with no sign and no zero at its
/// start. The unit is `ms`, `s`, `m`, `h`, `d`, `w`, `M` (30 days) or `y` (365 days). The interval
/// MUST be from 1 ms to 365 days. The function never copies `arg` to its result.
pub fn parse_group_interval(arg: &str) -> Result<GroupInterval, GroupIntervalError> {
    if is_builtin_text(arg) {
        return Ok(GroupInterval::Builtin);
    }
    let Some(inner) = arg.strip_prefix('\'').and_then(|a| a.strip_suffix('\'')) else {
        return Err(group_error(
            GroupIntervalFault::Shape,
            "the interval must be one string like '5m', or $__interval",
        ));
    };
    if inner == "$__interval" {
        return Ok(GroupInterval::Builtin);
    }
    parse_interval_string(inner).map(GroupInterval::Millis)
}

/// Reads the content of an interval string: `<n><unit>`.
fn parse_interval_string(inner: &str) -> Result<u64, GroupIntervalError> {
    let grammar = || {
        group_error(
            GroupIntervalFault::Grammar,
            "the interval string must be a whole number and one unit: ms, s, m, h, d, w, M or y",
        )
    };
    let digits = inner.bytes().take_while(u8::is_ascii_digit).count();
    let (number, unit) = inner.split_at(digits);
    if number.is_empty() || number.starts_with('0') {
        return Err(grammar());
    }
    let unit_ms: u64 = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 7 * 86_400_000,
        "M" => 30 * 86_400_000,
        "y" => 365 * 86_400_000,
        _ => return Err(grammar()),
    };
    let over = || group_error(GroupIntervalFault::Range, "the interval must be from 1 ms to 365 days");
    if number.len() > MAX_INTERVAL_DIGITS {
        return Err(over());
    }
    let n: u64 = number.parse().map_err(|_| grammar())?;
    let ms = n.checked_mul(unit_ms).ok_or_else(over)?;
    if !(MIN_GROUP_INTERVAL_MS..=MAX_GROUP_INTERVAL_MS).contains(&ms) {
        return Err(over());
    }
    Ok(ms)
}

/// The check at save of the stored interval argument of a group macro (7.23). It checks the shape:
/// one string token, or `$__interval` or `${__interval}`. It checks the grammar of a string only when
/// the string has no placeholder: a `$` or a `[[` in the string defers the check to run time.
pub fn check_stored_group_interval(arg: &str) -> Result<(), GroupIntervalError> {
    if is_builtin_text(arg) {
        return Ok(());
    }
    let Some(inner) = arg.strip_prefix('\'').and_then(|a| a.strip_suffix('\'')) else {
        return Err(group_error(
            GroupIntervalFault::Shape,
            "the interval must be one string like '5m', or $__interval",
        ));
    };
    if inner.contains('$') || inner.contains("[[") {
        return Ok(());
    }
    parse_interval_string(inner).map(|_| ())
}

/// Reads the interval argument and gives the interval in milliseconds. `builtin_ms` is `I` of 7.4:
/// the value of the built-in `$__interval`. It is `None` in a variable query, where the built-in is
/// a fault. `I` must be from 1 ms to 365 days too.
pub fn resolve_group_interval(arg: &str, builtin_ms: Option<u64>) -> Result<u64, GroupIntervalError> {
    match parse_group_interval(arg)? {
        GroupInterval::Millis(ms) => Ok(ms),
        GroupInterval::Builtin => {
            let ms = builtin_ms.ok_or_else(|| {
                group_error(
                    GroupIntervalFault::NoInterval,
                    "$__interval needs the interval, which a variable query has not",
                )
            })?;
            if !(MIN_GROUP_INTERVAL_MS..=MAX_GROUP_INTERVAL_MS).contains(&ms) {
                return Err(group_error(GroupIntervalFault::Range, "the interval must be from 1 ms to 365 days"));
            }
            Ok(ms)
        }
    }
}

/// `G` of 7.23: the interval in seconds, in decimal. A whole number of seconds gives the integer.
/// A fraction gives the shortest text with 3 decimals at most. The text never has an exponent.
pub fn group_seconds_text(ms: u64) -> String {
    let (whole, millis) = (ms / 1000, ms % 1000);
    if millis == 0 {
        return whole.to_string();
    }
    let fraction = format!("{millis:03}");
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}

/// The text `G` for the interval argument of a group macro: [`resolve_group_interval`] and then
/// [`group_seconds_text`]. This is the only thing that a group macro writes of its second argument.
pub fn group_seconds(arg: &str, builtin_ms: Option<u64>) -> Result<String, GroupIntervalError> {
    resolve_group_interval(arg, builtin_ms).map(group_seconds_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_rules() {
        assert!(check_time_range(0, 1).is_ok());
        assert!(check_time_range(0, MAX_TIME_MS).is_ok());
        assert!(check_time_range(-1, 5).is_err());
        assert!(check_time_range(5, 5).is_err());
        assert!(check_time_range(6, 5).is_err());
        assert!(check_time_range(0, MAX_TIME_MS + 1).is_err());
        let r = check_time_range(1_794_211_200_999, 1_794_232_800_001).unwrap();
        assert_eq!((r.from_seconds(), r.to_seconds()), (1_794_211_200, 1_794_232_800));
    }

    fn run(len: i64, d: u32, l: u64) -> (u64, u64) {
        let r = check_time_range(1000, 1000 + len).unwrap();
        let i = compute_interval(&r, d, l).unwrap();
        (i.ms, i.seconds)
    }

    #[test]
    fn worked_examples_of_7_4() {
        assert_eq!(run(21_600_000, 1200, 1), (20_000, 20));
        assert_eq!(run(604_800_000, 800, 1), (900_000, 900));
        assert_eq!(run(3_600_000, 1200, 60_000), (60_000, 60));
        assert_eq!(run(600_000, 1920, 1), (200, 1));
        assert_eq!(run(2_592_000_000, 1000, 1), (1_800_000, 1_800));
    }

    #[test]
    fn every_row_of_the_table() {
        // For each bound B with D = 1: R = B - 1 gives the row, and R = B gives the next row.
        let mut rows: Vec<(u64, u64)> = ROUND_TABLE.to_vec();
        rows.push((u64::MAX, ROUND_LAST));
        for (k, (bound, value)) in rows.iter().enumerate() {
            if *bound != u64::MAX {
                assert_eq!(run(*bound as i64 - 1, 1, 1).0, *value, "below {bound}");
                assert_eq!(run(*bound as i64, 1, 1).0, rows[k + 1].1, "at {bound}");
            }
        }
        assert_eq!(run(4_000_000_000, 1, 1).0, 31_536_000_000);
        // The table of 7.4, spot checks of the text.
        assert_eq!(run(9, 1, 1).0, 1);
        assert_eq!(run(10, 1, 1).0, 10);
        assert_eq!(run(14, 1, 1).0, 10);
        assert_eq!(run(90_000, 1, 1).0, 120_000);
        assert_eq!(run(89_999, 1, 1).0, 60_000);
        assert_eq!(run(3_628_799_999, 1, 1).0, 2_592_000_000);
    }

    #[test]
    fn real_r_is_not_rounded() {
        // 10 ms over 3 points: R = 3.33 < 10, so 1 ms. 31 ms over 2 points: R = 15.5, not below 15.
        assert_eq!(run(10, 3, 1).0, 1);
        assert_eq!(run(31, 2, 1).0, 20);
        assert_eq!(run(30, 2, 1).0, 20);
        assert_eq!(run(29, 2, 1).0, 10);
    }

    #[test]
    fn seconds_and_floor() {
        assert_eq!(run(10, 10, 1), (1, 1));
        assert_eq!(run(1_000_000, 1, 1_500_000), (1_500_000, 1_500));
        assert_eq!(run(1_000_000, 1, 1), (900_000, 900));
        assert_eq!(run(10, 10, 2_999), (2_999, 2));
        assert_eq!(run(10, 10, 999), (999, 1));
    }

    #[test]
    fn data_points_limits() {
        let r = check_time_range(0, 1000).unwrap();
        assert!(compute_interval(&r, 0, 1).is_err());
        assert!(compute_interval(&r, 10_001, 1).is_err());
        assert!(compute_interval(&r, 1, 1).is_ok());
        assert!(compute_interval(&r, 10_000, 1).is_ok());
        assert_eq!(interval_ms(&r, 1000, 1), Ok(1));
    }

    #[test]
    fn group_interval_forms() {
        let ms = |a: &str| resolve_group_interval(a, Some(20_000));
        assert_eq!(ms("'5m'"), Ok(300_000));
        assert_eq!(ms("'1ms'"), Ok(1));
        assert_eq!(ms("'1s'"), Ok(1_000));
        assert_eq!(ms("'2h'"), Ok(7_200_000));
        assert_eq!(ms("'1d'"), Ok(86_400_000));
        assert_eq!(ms("'1w'"), Ok(604_800_000));
        assert_eq!(ms("'1M'"), Ok(2_592_000_000));
        assert_eq!(ms("'1y'"), Ok(31_536_000_000));
        assert_eq!(ms("'365d'"), Ok(MAX_GROUP_INTERVAL_MS));
        assert_eq!(ms("'52w'"), Ok(52 * 604_800_000));
        assert_eq!(ms("'12M'"), Ok(12 * 2_592_000_000));
        assert_eq!(ms("'1000ms'"), Ok(1_000));
        assert_eq!(ms("$__interval"), Ok(20_000));
        assert_eq!(ms("${__interval}"), Ok(20_000));
        assert_eq!(ms("'$__interval'"), Ok(20_000));
        assert_eq!(parse_group_interval("'$__interval'"), Ok(GroupInterval::Builtin));
    }

    #[test]
    fn group_interval_refusals() {
        use GroupIntervalFault::{Grammar, NoInterval, Range, Shape};
        let fault = |a: &str| resolve_group_interval(a, Some(20_000)).unwrap_err().fault;
        for shape in [
            "",
            "5m",
            "300",
            "\"5m\"",
            "'5m",
            "5m'",
            "'5m' ",
            " '5m'",
            "'5m' /* c */",
            "$__interval ",
            "$__interval_s",
            "$__interval_ms",
            "$__intervals",
            "${__interval:raw}",
            "$step",
            "E'5m'",
            "N'5m'",
            "$$5m$$",
            "'a''b'x",
            "$__interval, 0",
        ] {
            assert_eq!(fault(shape), Shape, "{shape:?}");
        }
        for grammar in [
            "''",
            "'5m' 'x'",
            "' 5m'",
            "'5m '",
            "'1h30m'",
            "'1.5s'",
            "'05m'",
            "'0s'",
            "'0ms'",
            "'-5m'",
            "'+5m'",
            "'5'",
            "'m'",
            "'5 m'",
            "'5mm'",
            "'5x'",
            "'5S'",
            "'5H'",
            "'5D'",
            "'5Y'",
            "'5min'",
            "'5m;'",
            "'5m\\'",
            "'5m'x'",
            "'$__interval '",
            "'${__interval}'",
            "'$__Interval'",
            "'1e3s'",
            "'0x5s'",
            "'５m'",
            "'5\u{0}m'",
            "'5\nm'",
            "'\u{a0}5m'",
        ] {
            assert_eq!(fault(grammar), Grammar, "{grammar:?}");
        }
        for range in [
            "'366d'",
            "'2y'",
            "'53w'",
            "'13M'",
            "'8761h'",
            "'999999999999y'",
            "'9999999999999999999999d'",
            "'31536000001ms'",
        ] {
            assert_eq!(fault(range), Range, "{range:?}");
        }
        assert_eq!(resolve_group_interval("$__interval", None).unwrap_err().fault, NoInterval);
        assert_eq!(resolve_group_interval("'$__interval'", None).unwrap_err().fault, NoInterval);
        assert_eq!(resolve_group_interval("'5m'", None), Ok(300_000));
        // The interval I of 7.4 has the same limits.
        assert_eq!(resolve_group_interval("$__interval", Some(0)).unwrap_err().fault, Range);
        assert_eq!(resolve_group_interval("$__interval", Some(MAX_GROUP_INTERVAL_MS + 1)).unwrap_err().fault, Range);
        assert_eq!(resolve_group_interval("$__interval", Some(MAX_GROUP_INTERVAL_MS)), Ok(MAX_GROUP_INTERVAL_MS));
        assert_eq!(resolve_group_interval("$__interval", Some(1)), Ok(1));
    }

    #[test]
    fn group_interval_save_check() {
        use GroupIntervalFault::{Grammar, Range, Shape};
        assert!(check_stored_group_interval("'5m'").is_ok());
        assert!(check_stored_group_interval("$__interval").is_ok());
        assert!(check_stored_group_interval("${__interval}").is_ok());
        assert!(check_stored_group_interval("'$__interval'").is_ok());
        // A placeholder defers the grammar to run time.
        assert!(check_stored_group_interval("'$step'").is_ok());
        assert!(check_stored_group_interval("'${n}m'").is_ok());
        assert!(check_stored_group_interval("'[[n]]m'").is_ok());
        // The shape is checked at save, also with a placeholder.
        assert_eq!(check_stored_group_interval("$step").unwrap_err().fault, Shape);
        assert_eq!(check_stored_group_interval("${step:singlequote}").unwrap_err().fault, Shape);
        assert_eq!(check_stored_group_interval("5m").unwrap_err().fault, Shape);
        assert_eq!(check_stored_group_interval("").unwrap_err().fault, Shape);
        // The grammar of a string without a placeholder is checked at save.
        assert_eq!(check_stored_group_interval("'1h30m'").unwrap_err().fault, Grammar);
        assert_eq!(check_stored_group_interval("'0s'").unwrap_err().fault, Grammar);
        assert_eq!(check_stored_group_interval("'2y'").unwrap_err().fault, Range);
    }

    #[test]
    fn group_seconds_texts() {
        for (ms, text) in [
            (1, "0.001"),
            (10, "0.01"),
            (20, "0.02"),
            (100, "0.1"),
            (500, "0.5"),
            (999, "0.999"),
            (1_000, "1"),
            (1_001, "1.001"),
            (1_500, "1.5"),
            (20_000, "20"),
            (300_000, "300"),
            (3_600_000, "3600"),
            (86_400_000, "86400"),
            (2_592_000_000, "2592000"),
            (31_536_000_000, "31536000"),
        ] {
            assert_eq!(group_seconds_text(ms), text, "{ms}");
        }
        assert_eq!(group_seconds("'5m'", None).unwrap(), "300");
        assert_eq!(group_seconds("'500ms'", None).unwrap(), "0.5");
        assert_eq!(group_seconds("$__interval", Some(20_000)).unwrap(), "20");
        for text in ["e", "E"] {
            assert!(!group_seconds_text(31_536_000_000).contains(text));
        }
    }
}
