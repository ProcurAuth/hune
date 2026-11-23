//! Durations and time texts (4.4).

/// The largest absolute time in epoch milliseconds (4.4).
pub const MAX_ABSOLUTE_MS: u64 = 4_294_967_295_999;

/// The unit of a relative time (4.4, `tunit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    /// `s`
    Second,
    /// `m`: minute
    Minute,
    /// `h`
    Hour,
    /// `d`
    Day,
    /// `w`
    Week,
    /// `M`: month
    Month,
    /// `y`
    Year,
}

impl TimeUnit {
    fn parse(c: u8) -> Option<Self> {
        match c {
            b's' => Some(TimeUnit::Second),
            b'm' => Some(TimeUnit::Minute),
            b'h' => Some(TimeUnit::Hour),
            b'd' => Some(TimeUnit::Day),
            b'w' => Some(TimeUnit::Week),
            b'M' => Some(TimeUnit::Month),
            b'y' => Some(TimeUnit::Year),
            _ => None,
        }
    }
}

/// The offset of a relative time: `now-<count><unit>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset {
    /// 1 to 999999999.
    pub count: u32,
    /// The unit.
    pub unit: TimeUnit,
}

/// A time text (4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeText {
    /// `now`, with an optional offset and an optional unit to round down to.
    Relative {
        /// The part `-<count><unit>`.
        offset: Option<Offset>,
        /// The part `/<unit>`: the time rounds down to the start of this unit.
        round: Option<TimeUnit>,
    },
    /// Epoch milliseconds, at most `MAX_ABSOLUTE_MS`.
    Absolute(u64),
}

/// Reads the `count` of 4.4: 1 to 999999999, with no zero as first digit. Gives the value and the
/// number of bytes read.
fn read_count(b: &[u8]) -> Option<(u32, usize)> {
    let first = *b.first()?;
    if !(b'1'..=b'9').contains(&first) {
        return None;
    }
    let mut n = 0usize;
    let mut v: u32 = 0;
    while let Some(&d) = b.get(n) {
        if !d.is_ascii_digit() {
            break;
        }
        if n >= 9 {
            return None;
        }
        v = v * 10 + u32::from(d - b'0');
        n += 1;
    }
    Some((v, n))
}

/// Reads a duration of 4.4 (`count` and one unit: `ms`, `s`, `m`, `h`, `d`) in milliseconds.
/// A text that does not match the grammar gives `None`.
pub fn parse_duration_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    let (count, n) = read_count(b)?;
    let factor: u64 = match b.get(n..)? {
        b"ms" => 1,
        b"s" => 1000,
        b"m" => 60_000,
        b"h" => 3_600_000,
        b"d" => 86_400_000,
        _ => return None,
    };
    Some(u64::from(count) * factor)
}

/// Reads a `time_from` of 4.15: `now-<count><unit>[/<unit>]` or `now/<unit>`. The text `now`
/// alone, an absolute time and any other text give `None`.
pub fn parse_time_from(s: &str) -> Option<TimeText> {
    match parse_time_text(s)? {
        t @ TimeText::Relative { offset, round } if offset.is_some() || round.is_some() => Some(t),
        _ => None,
    }
}

/// Reads a `time_shift` of 4.15: `<count><unit>`, with a unit of `tunit` (4.4). Any other text
/// gives `None`.
pub fn parse_time_shift(s: &str) -> Option<Offset> {
    let b = s.as_bytes();
    let (count, n) = read_count(b)?;
    let unit = TimeUnit::parse(*b.get(n)?)?;
    (b.len() == n + 1).then_some(Offset { count, unit })
}

/// Reads a time text of 4.4. A text that does not match the grammar gives `None`.
pub fn parse_time_text(s: &str) -> Option<TimeText> {
    let b = s.as_bytes();
    if let Some(rest) = b.strip_prefix(b"now") {
        let (offset, rest) = match rest.strip_prefix(b"-") {
            Some(after) => {
                let (count, n) = read_count(after)?;
                let unit = TimeUnit::parse(*after.get(n)?)?;
                (Some(Offset { count, unit }), after.get(n + 1..)?)
            }
            None => (None, rest),
        };
        let round = match rest {
            [] => None,
            [b'/', u] => Some(TimeUnit::parse(*u)?),
            _ => return None,
        };
        return Some(TimeText::Relative { offset, round });
    }
    if b == b"0" {
        return Some(TimeText::Absolute(0));
    }
    let first = *b.first()?;
    if !(b'1'..=b'9').contains(&first) || b.len() > 13 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let v: u64 = s.parse().ok()?;
    (v <= MAX_ABSOLUTE_MS).then_some(TimeText::Absolute(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_of_4_4() {
        let valid = ["now", "now-6h", "now/d", "now-1d/d", "1731100000000", "0", "now-999999999y/M"];
        for t in valid {
            assert!(parse_time_text(t).is_some(), "{t}");
        }
        let invalid = [
            "now-0h",
            "now+1h",
            "now-6H",
            "01731100000000",
            "4294967296000",
            "",
            "now-",
            "now-6",
            "now/",
            "now/x",
            "now-6h/dd",
            "now-1000000000h",
            "-1",
            "1.5",
            " now",
            "nowx",
            "Now",
        ];
        for t in invalid {
            assert!(parse_time_text(t).is_none(), "{t}");
        }
        assert_eq!(parse_time_text("4294967295999"), Some(TimeText::Absolute(MAX_ABSOLUTE_MS)));
        assert_eq!(
            parse_time_text("now-6h/d"),
            Some(TimeText::Relative {
                offset: Some(Offset { count: 6, unit: TimeUnit::Hour }),
                round: Some(TimeUnit::Day)
            })
        );
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration_ms("90s"), Some(90_000));
        assert_eq!(parse_duration_ms("1ms"), Some(1));
        assert_eq!(parse_duration_ms("5m"), Some(300_000));
        assert_eq!(parse_duration_ms("2h"), Some(7_200_000));
        assert_eq!(parse_duration_ms("1d"), Some(86_400_000));
        assert_eq!(parse_duration_ms("999999999d"), Some(999_999_999 * 86_400_000));
        for bad in ["1.5m", "1m30s", "0s", "01s", "s", "", "5", "5w", "5M", "5 s", "1000000000s", "-1s", "5ss"] {
            assert_eq!(parse_duration_ms(bad), None, "{bad}");
        }
    }
}
