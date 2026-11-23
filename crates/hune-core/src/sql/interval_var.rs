//! The `interval` variable (4.10a): the grammar of an item, the table of the texts of "Auto" and the text of
//! "Auto" for a request (7.4a).
//!
//! The functions are pure. The server computes the text of "Auto". The page never sends it.

use super::interval::{TimeRange, round_interval};

/// The most digits of the number of an interval item (4.10a).
const MAX_ITEM_DIGITS: usize = 9;
/// The largest interval of an item: 365 days, in milliseconds (4.10a).
pub const MAX_ITEM_MS: u64 = 365 * 86_400_000;
/// The value of the option "Auto" (4.10a).
pub const AUTO_VALUE: &str = "$__auto";
/// The smallest `auto_count` (4.10a).
pub const MIN_AUTO_COUNT: u32 = 1;
/// The largest `auto_count` (4.10a).
pub const MAX_AUTO_COUNT: u32 = 1_000;

/// The 30 rows of the table of 7.4a: the interval in milliseconds and its text, in the order of the table.
/// A text of `auto_min` is one of these texts (4.10a).
pub const AUTO_TABLE: [(u64, &str); 30] = [
    (1, "1ms"),
    (10, "10ms"),
    (20, "20ms"),
    (50, "50ms"),
    (100, "100ms"),
    (200, "200ms"),
    (500, "500ms"),
    (1_000, "1s"),
    (2_000, "2s"),
    (5_000, "5s"),
    (10_000, "10s"),
    (15_000, "15s"),
    (20_000, "20s"),
    (30_000, "30s"),
    (60_000, "1m"),
    (120_000, "2m"),
    (300_000, "5m"),
    (600_000, "10m"),
    (900_000, "15m"),
    (1_200_000, "20m"),
    (1_800_000, "30m"),
    (3_600_000, "1h"),
    (7_200_000, "2h"),
    (10_800_000, "3h"),
    (21_600_000, "6h"),
    (43_200_000, "12h"),
    (86_400_000, "1d"),
    (604_800_000, "7d"),
    (2_592_000_000, "30d"),
    (31_536_000_000, "1y"),
];

/// The cause of a fault in an interval item (4.10a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemFault {
    /// The text does not match `^[1-9][0-9]{0,8}(ms|s|m|h|d|w|y)$`. The API code is `bad_format`.
    Format,
    /// The interval is more than 365 days. The API code is `out_of_range`.
    Range,
}

/// Reads an interval item (4.10a) and gives its value in milliseconds.
///
/// The text MUST match `^[1-9][0-9]{0,8}(ms|s|m|h|d|w|y)$` and MUST be 365 days or less. The unit `M` of
/// Grafana, a sign, a space and a decimal point are not permitted. The units are milliseconds, seconds,
/// minutes, hours, days, weeks and years of 365 days.
pub fn parse_interval_item(text: &str) -> Result<u64, ItemFault> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let (number, unit) = text.split_at(digits);
    if number.is_empty() || number.len() > MAX_ITEM_DIGITS || number.starts_with('0') {
        return Err(ItemFault::Format);
    }
    let unit_ms: u64 = match unit {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 7 * 86_400_000,
        "y" => 365 * 86_400_000,
        _ => return Err(ItemFault::Format),
    };
    // At most 9 digits: the number fits in `u64`. The product can overflow for a unit of 365 days: that is a range fault.
    let n: u64 = number.parse().map_err(|_| ItemFault::Format)?;
    match n.checked_mul(unit_ms) {
        Some(ms) if ms <= MAX_ITEM_MS => Ok(ms),
        _ => Err(ItemFault::Range),
    }
}

/// Gives the interval in milliseconds of a text of the table of 7.4a, or `None` for another text.
pub fn auto_min_ms(text: &str) -> Option<u64> {
    AUTO_TABLE.iter().find(|(_, t)| *t == text).map(|(ms, _)| *ms)
}

/// Gives the largest row of the table of 7.4a that is not above `ms`. A value under 1 ms gives the first row.
/// The importer uses it to map an `auto_min` of Grafana (15.6).
pub fn largest_table_text_at_most(ms: u64) -> &'static str {
    AUTO_TABLE.iter().rev().find(|(v, _)| *v <= ms).map_or(AUTO_TABLE[0].1, |(_, t)| *t)
}

/// Gives the text of the row of `ms` in the table of 7.4a. A value that is not a row gives `None`.
fn table_text(ms: u64) -> Option<&'static str> {
    AUTO_TABLE.iter().find(|(v, _)| *v == ms).map(|(_, t)| *t)
}

/// Computes the text of "Auto" (7.4a): `max(M, round_interval(R))` with `R = length / auto_count`.
///
/// `auto_count` MUST be 1 to 1,000. `auto_min` MUST be a text of the table. The result is always a text of
/// the table. A call with other arguments gives `None`: the document check makes this case not possible.
pub fn auto_text(range: &TimeRange, auto_count: u32, auto_min: &str) -> Option<&'static str> {
    if !(MIN_AUTO_COUNT..=MAX_AUTO_COUNT).contains(&auto_count) {
        return None;
    }
    let minimum = auto_min_ms(auto_min)?;
    let rounded = round_interval(range.length_ms().unsigned_abs(), u64::from(auto_count));
    table_text(minimum.max(rounded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::check_time_range;

    /// The 59 ranges of the measure on Grafana 12.4.2 (7.4a): a step count of 1 and a minimum of 1 ms.
    const MEASURED: [(i64, &str); 59] = [
        (9, "1ms"),
        (10, "10ms"),
        (14, "10ms"),
        (15, "20ms"),
        (34, "20ms"),
        (35, "50ms"),
        (74, "50ms"),
        (75, "100ms"),
        (149, "100ms"),
        (150, "200ms"),
        (349, "200ms"),
        (350, "500ms"),
        (749, "500ms"),
        (750, "1s"),
        (1499, "1s"),
        (1500, "2s"),
        (3499, "2s"),
        (3500, "5s"),
        (7499, "5s"),
        (7500, "10s"),
        (12499, "10s"),
        (12500, "15s"),
        (17499, "15s"),
        (17500, "20s"),
        (24999, "20s"),
        (25000, "30s"),
        (44999, "30s"),
        (45000, "1m"),
        (89999, "1m"),
        (90000, "2m"),
        (209999, "2m"),
        (210000, "5m"),
        (449999, "5m"),
        (450000, "10m"),
        (749999, "10m"),
        (750000, "15m"),
        (1049999, "15m"),
        (1050000, "20m"),
        (1499999, "20m"),
        (1500000, "30m"),
        (2699999, "30m"),
        (2700000, "1h"),
        (5399999, "1h"),
        (5400000, "2h"),
        (8999999, "2h"),
        (9000000, "3h"),
        (16199999, "3h"),
        (16200000, "6h"),
        (32399999, "6h"),
        (32400000, "12h"),
        (86399999, "12h"),
        (86400000, "1d"),
        (604799999, "1d"),
        (604800000, "7d"),
        (1814399999, "7d"),
        (1814400000, "30d"),
        (3628799999, "30d"),
        (3628800000, "1y"),
        (4000000000, "1y"),
    ];

    fn auto(len: i64, count: u32, min: &str) -> Option<&'static str> {
        auto_text(&check_time_range(1000, 1000 + len).unwrap(), count, min)
    }

    #[test]
    fn the_59_bound_cases_of_the_measure() {
        for (len, text) in MEASURED {
            assert_eq!(auto(len, 1, "1ms"), Some(text), "range {len} ms");
        }
    }

    #[test]
    fn the_table_has_30_rows_in_order_with_distinct_texts() {
        assert_eq!(AUTO_TABLE.len(), 30);
        for pair in AUTO_TABLE.windows(2) {
            assert!(pair[0].0 < pair[1].0 && pair[0].1 != pair[1].1);
        }
        // Each text of the table is a valid item that gives the same interval, except `1y` and `30d` (30 days
        // is an item, and `1y` is 365 days).
        for (ms, text) in AUTO_TABLE {
            assert_eq!(parse_interval_item(text), Ok(ms), "{text}");
        }
    }

    #[test]
    fn the_worked_examples_of_7_4a() {
        // 6 hours, 30 steps, 10 s: R = 720,000 ms, rounds to 600,000 ms.
        assert_eq!(auto(21_600_000, 30, "10s"), Some("10m"));
        // 5 minutes: R = 10,000 ms.
        assert_eq!(auto(300_000, 30, "10s"), Some("10s"));
        assert_eq!(auto(30 * 86_400_000, 30, "10s"), Some("1d"));
        // Other measured values: 6 h, 10 steps, 1 minute; 5 minutes, 10 steps, 1 minute.
        assert_eq!(auto(21_600_000, 10, "1m"), Some("30m"));
        assert_eq!(auto(300_000, 10, "1m"), Some("1m"));
        assert_eq!(auto(30 * 86_400_000, 30, "1ms"), Some("1d"));
        assert_eq!(auto(365 * 86_400_000, 10, "1ms"), Some("30d"));
        assert_eq!(auto(365 * 86_400_000, 30, "1ms"), Some("7d"));
        assert_eq!(auto(21_600_000, 1, "1h"), Some("6h"));
        assert_eq!(auto(60_000, 1, "1h"), Some("1h"));
    }

    #[test]
    fn the_minimum_and_the_extremes() {
        // The minimum wins when it is above the rounded interval.
        assert_eq!(auto(1, 1, "1y"), Some("1y"));
        // The largest range: 4,294,967,295,999 ms with 1 step, and with 1,000 steps, gives 1 year.
        let max = check_time_range(0, crate::sql::MAX_TIME_MS).unwrap();
        assert_eq!(auto_text(&max, 1, "1ms"), Some("1y"));
        assert_eq!(auto_text(&max, 1_000, "1ms"), Some("1y"));
        // The smallest range with the largest count.
        assert_eq!(auto(1, 1_000, "1ms"), Some("1ms"));
        // A count or a minimum outside the rules gives nothing.
        assert_eq!(auto(1_000, 0, "1s"), None);
        assert_eq!(auto(1_000, 1_001, "1s"), None);
        assert_eq!(auto(1_000, 1, "90s"), None);
        assert_eq!(auto(1_000, 1, "1M"), None);
    }

    #[test]
    fn every_text_of_the_result_is_a_row_of_the_table() {
        let texts: Vec<&str> = AUTO_TABLE.iter().map(|(_, t)| *t).collect();
        for len in [1_i64, 7, 999, 60_000, 3_600_000, 86_400_000 * 11, 4_000_000_000] {
            for count in [1, 2, 30, 999, 1_000] {
                for (_, min) in AUTO_TABLE {
                    let t = auto(len, count, min).unwrap();
                    assert!(texts.contains(&t));
                    assert!(auto_min_ms(t).unwrap() >= auto_min_ms(min).unwrap());
                }
            }
        }
    }

    #[test]
    fn item_grammar() {
        for (text, ms) in [
            ("1ms", 1),
            ("999999999ms", 999_999_999),
            ("1s", 1_000),
            ("90s", 90_000),
            ("1m", 60_000),
            ("1h", 3_600_000),
            ("7d", 604_800_000),
            ("1w", 604_800_000),
            ("52w", 364 * 86_400_000),
            ("365d", MAX_ITEM_MS),
            ("1y", MAX_ITEM_MS),
            ("8760h", MAX_ITEM_MS),
        ] {
            assert_eq!(parse_interval_item(text), Ok(ms), "{text}");
        }
        for text in [
            "",
            "1",
            "m",
            "0m",
            "01m",
            "-1m",
            "+1m",
            "1.5h",
            "1 m",
            " 1m",
            "1m ",
            "1M",
            "1H",
            "1D",
            "1Y",
            "1min",
            "1mm",
            "1ms2",
            "1e3s",
            "1\u{661}m",
            "1m\n",
            "1000000000ms",
            "abc",
            "5x",
            "$__auto",
            "1m;2m",
            "1m'",
        ] {
            assert_eq!(parse_interval_item(text), Err(ItemFault::Format), "{text:?}");
        }
        for text in ["366d", "53w", "2y", "8761h", "525601m", "99999999d", "999999999y", "31536001s"] {
            assert_eq!(parse_interval_item(text), Err(ItemFault::Range), "{text:?}");
        }
        // Not a hostile case: the first digit rule and the digit count come before the range.
        assert_eq!(parse_interval_item("0000000001m"), Err(ItemFault::Format));
    }

    #[test]
    fn the_map_down_of_the_minimum() {
        assert_eq!(largest_table_text_at_most(90_000), "1m");
        assert_eq!(largest_table_text_at_most(1), "1ms");
        assert_eq!(largest_table_text_at_most(0), "1ms");
        assert_eq!(largest_table_text_at_most(3_600_000), "1h");
        assert_eq!(largest_table_text_at_most(3_599_999), "30m");
        assert_eq!(largest_table_text_at_most(u64::MAX), "1y");
        assert_eq!(auto_min_ms("10s"), Some(10_000));
        assert_eq!(auto_min_ms("1w"), None);
    }
}
