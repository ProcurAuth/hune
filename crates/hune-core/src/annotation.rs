// SPDX-License-Identifier: MIT OR Apache-2.0
//! The marks of an annotation query (7.37): a pure function from a frame to a list of marks.
//!
//! The function reads the columns by their role (`time`, `timeend`, `title`, `text` and `tags`),
//! checks each value and gives the marks in the order of the time. It refuses what it cannot
//! draw safely. It gives one fault and no mark for a frame that breaks a rule (fail closed).
//! The fault never holds the value of a cell.
//!
//! The function reads the kinds of 7.16 and not the types of a source, so the same rules apply to
//! the frame of a `clickhouse`, a `postgres` and a `mongodb` source (7.16a, 7.16c). The table
//! "The roles for each kind of source" of 7.37 gives the types that each role accepts.
//!
//! This module does no I/O. It does not read a clock or a random source.

use crate::frame::{Column, Frame, FrameValue, Kind};
use crate::limits::{
    MAX_ANNOTATION_TAG_CHARS, MAX_ANNOTATION_TAGS, MAX_ANNOTATION_TEXT_BYTES, MAX_ANNOTATION_TITLE_CHARS,
};

/// The largest time of a mark, in milliseconds: 4,294,967,295 seconds and 999 milliseconds (7.37, rule 4).
pub const MAX_MARK_MS: i64 = 4_294_967_295_999;

/// The reason of an `annotation_invalid` error: the closed list of 7.37.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnnotationReason {
    /// No column named `time`, and no column of the kind `time`.
    NoTime,
    /// The time is `null`, is not a whole number of milliseconds, is out of range, or has a wrong kind.
    BadTime,
    /// The end has a wrong kind, is out of range, or is below the time.
    BadEnd,
    /// The title, the text or the tags have a kind other than `text`.
    BadText,
    /// A control character, a bidirectional control, or the form of an HTML tag.
    BadChars,
    /// A title, a text, a tag or the count of tags is above its limit.
    TooLong,
    /// Two columns have the same role.
    DuplicateColumn,
}

impl AnnotationReason {
    /// Every reason, in the order of 7.37.
    pub const ALL: [AnnotationReason; 7] = [
        AnnotationReason::NoTime,
        AnnotationReason::BadTime,
        AnnotationReason::BadEnd,
        AnnotationReason::BadText,
        AnnotationReason::BadChars,
        AnnotationReason::TooLong,
        AnnotationReason::DuplicateColumn,
    ];

    /// Gives the text of the member `reason` of the error (6.8).
    pub fn as_str(self) -> &'static str {
        match self {
            AnnotationReason::NoTime => "no_time",
            AnnotationReason::BadTime => "bad_time",
            AnnotationReason::BadEnd => "bad_end",
            AnnotationReason::BadText => "bad_text",
            AnnotationReason::BadChars => "bad_chars",
            AnnotationReason::TooLong => "too_long",
            AnnotationReason::DuplicateColumn => "duplicate_column",
        }
    }

    /// Gives the fixed English sentence of the reason. It never holds a value of a cell.
    pub fn message(self) -> &'static str {
        match self {
            AnnotationReason::NoTime => "The result has no column named time and no column of the kind time.",
            AnnotationReason::BadTime => {
                "A time is null, is not a whole number of milliseconds, is out of range, or has a wrong kind."
            }
            AnnotationReason::BadEnd => "An end time has a wrong kind, is out of range, or is below its time.",
            AnnotationReason::BadText => "A title, a text or a tags value has a kind other than text.",
            AnnotationReason::BadChars => {
                "A title, a text or a tag has a control character, a bidirectional control or the form of an HTML tag."
            }
            AnnotationReason::TooLong => "A title, a text, a tag or the count of tags is above its limit.",
            AnnotationReason::DuplicateColumn => "Two columns of the result have the same role.",
        }
    }
}

/// The fault of a frame (7.37, rule 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnotationFault {
    /// The reason.
    pub reason: AnnotationReason,
    /// The first row that breaks a rule, counted from 1. `None` for a fault of a column.
    pub row: Option<usize>,
}

/// One mark: a point or a region (7.37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// The time in milliseconds.
    pub time_ms: i64,
    /// The end in milliseconds, for a region. `None` for a point. It is above `time_ms`.
    pub end_ms: Option<i64>,
    /// The title: 256 characters at most. It can be empty.
    pub title: String,
    /// The text: 2,048 bytes at most. It can be empty.
    pub text: String,
    /// The tags: 8 at most, unique, with 64 characters at most each.
    pub tags: Vec<String>,
}

/// The role of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Time,
    TimeEnd,
    Title,
    Text,
    Tags,
}

fn role_of(name: &str) -> Option<Role> {
    match name.to_ascii_lowercase().as_str() {
        "time" => Some(Role::Time),
        "timeend" => Some(Role::TimeEnd),
        "title" => Some(Role::Title),
        "text" => Some(Role::Text),
        "tags" => Some(Role::Tags),
        _ => None,
    }
}

fn fault(reason: AnnotationReason, row: Option<usize>) -> AnnotationFault {
    AnnotationFault { reason, row }
}

/// Makes the marks of a frame (7.37). The function is pure.
///
/// The marks come in the order of `time`, then of the row. The function drops nothing and merges
/// nothing. A frame with no column and no row gives no mark (7.37, rule 13): it is the empty result
/// of a `mongodb` source (7.16c), which has no column to name.
///
/// # Errors
/// A frame that breaks a rule of 7.37 gives one fault with the reason and the first row.
pub fn annotations_from_frame(frame: &Frame) -> Result<Vec<Mark>, AnnotationFault> {
    if frame.columns.is_empty() && frame.rows == 0 {
        return Ok(Vec::new());
    }
    let cols = Columns::resolve(frame)?;
    let mut marks = Vec::with_capacity(frame.rows);
    for row in 0..frame.rows {
        let at = Some(row + 1);
        let time_ms = time_value(cols.time, row).ok_or_else(|| fault(AnnotationReason::BadTime, at))?;
        let end_ms = match cols.end {
            None => None,
            Some(c) => end_value(c, row, time_ms).ok_or_else(|| fault(AnnotationReason::BadEnd, at))?,
        };
        let title = match cols.title {
            None => String::new(),
            Some(c) => single_text(c, row, MAX_ANNOTATION_TITLE_CHARS, true).map_err(|r| fault(r, at))?,
        };
        let text = match cols.text {
            None => String::new(),
            Some(c) => single_text(c, row, MAX_ANNOTATION_TEXT_BYTES, false).map_err(|r| fault(r, at))?,
        };
        let tags = match cols.tags {
            None => Vec::new(),
            Some(c) => tags_value(c, row).map_err(|r| fault(r, at))?,
        };
        marks.push(Mark { time_ms, end_ms, title, text, tags });
    }
    // A stable sort keeps the order of the rows for equal times.
    marks.sort_by_key(|m| m.time_ms);
    Ok(marks)
}

/// The columns of the roles, after the checks of the columns.
struct Columns<'a> {
    time: &'a Column,
    end: Option<&'a Column>,
    title: Option<&'a Column>,
    text: Option<&'a Column>,
    tags: Option<&'a Column>,
}

impl<'a> Columns<'a> {
    /// Finds the column of each role (rules 1 to 3) and checks the kind of each one.
    fn resolve(frame: &'a Frame) -> Result<Columns<'a>, AnnotationFault> {
        let mut found: [Option<&Column>; 5] = [None; 5];
        for c in &frame.columns {
            let Some(role) = role_of(&c.name) else { continue };
            let slot = &mut found[role as usize];
            if slot.is_some() {
                return Err(fault(AnnotationReason::DuplicateColumn, None));
            }
            *slot = Some(c);
        }
        let [time, end, title, text, tags] = found;
        let time = time
            .or_else(|| frame.columns.iter().find(|c| c.kind == Kind::Time))
            .ok_or_else(|| fault(AnnotationReason::NoTime, None))?;
        let number = |c: &Column| matches!(c.kind, Kind::Time | Kind::Number | Kind::NumberText | Kind::Null);
        let words = |c: &Column| matches!(c.kind, Kind::Text | Kind::Null);
        if !number(time) {
            return Err(fault(AnnotationReason::BadTime, None));
        }
        if end.is_some_and(|c| !number(c)) {
            return Err(fault(AnnotationReason::BadEnd, None));
        }
        if [title, text, tags].into_iter().flatten().any(|c| !words(c)) {
            return Err(fault(AnnotationReason::BadText, None));
        }
        Ok(Columns { time, end, title, text, tags })
    }
}

/// Reads the milliseconds of a time cell, or `None` when the value breaks a rule (7.37, rule 4).
fn time_value(c: &Column, row: usize) -> Option<i64> {
    let ms = whole_ms(c.kind, c.value(row)?)?;
    (0..=MAX_MARK_MS).contains(&ms).then_some(ms)
}

/// Reads an end cell (rule 5). It gives `Some(None)` for a point, `Some(Some(ms))` for a region and `None`
/// for a fault.
fn end_value(c: &Column, row: usize, time_ms: i64) -> Option<Option<i64>> {
    let v = c.value(row)?;
    if v == FrameValue::Null {
        return Some(None);
    }
    let ms = whole_ms(c.kind, v)?;
    if !(0..=MAX_MARK_MS).contains(&ms) {
        return None;
    }
    if ms == 0 || ms == time_ms {
        return Some(None);
    }
    (ms > time_ms).then_some(Some(ms))
}

/// Reads the milliseconds of a cell. A `time` value is Unix seconds with a fraction: the digits below the
/// millisecond are cut. A `number` or `number_text` value is whole epoch milliseconds. A negative value, a
/// `null` and a value of another form give `None`. The caller checks the upper bound.
fn whole_ms(kind: Kind, v: FrameValue<'_>) -> Option<i64> {
    match (kind, v) {
        (Kind::Time, FrameValue::Number(s)) => decimal(s, true),
        (Kind::Number, FrameValue::Number(s)) | (Kind::NumberText, FrameValue::Text(s)) => decimal(s, false),
        _ => None,
    }
}

/// Parses `^-?[0-9]+(\.[0-9]+)?$`. With `seconds`, the value is in seconds and the result in milliseconds with
/// the cut of the digits below the millisecond. Without it, the value MUST be a whole number: the fraction
/// MUST be zeros. A negative value gives `None`, and so does `-0` followed by a digit that is not zero.
fn decimal(s: &str, seconds: bool) -> Option<i64> {
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) if !f.is_empty() => (i, f),
        Some(_) => return None,
        None => (body, ""),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let zero_frac = frac.bytes().all(|b| b == b'0');
    let int_value: Option<i64> =
        int.trim_start_matches('0').parse().ok().or_else(|| int.bytes().all(|b| b == b'0').then_some(0));
    let int_value = int_value?;
    if negative {
        // `-0` and `-0.000` are zero. Any other negative value is below 0.
        return (int_value == 0 && zero_frac).then_some(0);
    }
    if seconds {
        let mut ms_digits: String = frac.chars().take(3).collect();
        while ms_digits.len() < 3 {
            ms_digits.push('0');
        }
        let ms: i64 = ms_digits.parse().ok()?;
        int_value.checked_mul(1000)?.checked_add(ms)
    } else if zero_frac {
        Some(int_value)
    } else {
        None
    }
}

/// Reads a title or a text. A `null` is the empty text. `chars`: the limit counts characters, else bytes.
fn single_text(c: &Column, row: usize, limit: usize, chars: bool) -> Result<String, AnnotationReason> {
    let s = match c.value(row) {
        Some(FrameValue::Text(t)) => t,
        Some(FrameValue::Null) | None => return Ok(String::new()),
        Some(_) => return Err(AnnotationReason::BadText),
    };
    let size = if chars { s.chars().count() } else { s.len() };
    if size > limit {
        return Err(AnnotationReason::TooLong);
    }
    check_chars(s)?;
    Ok(s.to_owned())
}

/// Reads the tags of a row (rule 7). A JSON array of texts gives its items. Another text is split at each
/// comma. The function trims each tag, drops the empty tags and keeps the first of equal tags.
///
/// The function reads one item at a time and never holds the list of items. It keeps at most one more
/// tag than the limit, so a large cell costs no memory beyond the cell.
fn tags_value(c: &Column, row: usize) -> Result<Vec<String>, AnnotationReason> {
    let s = match c.value(row) {
        Some(FrameValue::Text(t)) => t,
        Some(FrameValue::Null) | None => return Ok(Vec::new()),
        Some(_) => return Err(AnnotationReason::BadText),
    };
    let mut sink = TagSink::default();
    if !json_texts(s, &mut sink) {
        sink = TagSink::default();
        for item in s.split(',') {
            if sink.fault.is_some() {
                break;
            }
            sink.feed(item);
        }
    }
    match sink.fault {
        Some(reason) => Err(reason),
        None => Ok(sink.tags),
    }
}

/// The tags of one row, as the items arrive. The first fault stays and later items are ignored.
#[derive(Default)]
struct TagSink {
    tags: Vec<String>,
    fault: Option<AnnotationReason>,
}

impl TagSink {
    /// Reads one item. The check comes before the trim: a control character at the end of an item is a
    /// fault, and the trim never hides it.
    fn feed(&mut self, item: &str) {
        if self.fault.is_some() {
            return;
        }
        if let Err(reason) = check_chars(item) {
            self.fault = Some(reason);
            return;
        }
        let tag = item.trim();
        if tag.is_empty() || self.tags.iter().any(|t| t == tag) {
            return;
        }
        if tag.chars().count() > MAX_ANNOTATION_TAG_CHARS {
            self.fault = Some(AnnotationReason::TooLong);
            return;
        }
        self.tags.push(tag.to_owned());
        if self.tags.len() > MAX_ANNOTATION_TAGS {
            self.fault = Some(AnnotationReason::TooLong);
        }
    }
}

/// Reads a JSON array of texts into `sink`, one item at a time. It gives `false` for any other text, and
/// the caller reads the text as a list with commas. The reader checks the whole text, so a text with an
/// error after the limit is not a JSON array.
fn json_texts(s: &str, sink: &mut TagSink) -> bool {
    use serde::de::{self, Deserializer, Visitor};
    use std::fmt;

    struct Item<'a>(&'a mut TagSink);
    struct ItemText<'a>(&'a mut TagSink);
    struct Items<'a>(&'a mut TagSink);

    impl<'de> Visitor<'de> for ItemText<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a text")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<(), E> {
            self.0.feed(v);
            Ok(())
        }
    }

    impl<'de> de::DeserializeSeed<'de> for Item<'_> {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_str(ItemText(self.0))
        }
    }

    impl<'de> Visitor<'de> for Items<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an array of texts")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while seq.next_element_seed(Item(&mut *self.0))?.is_some() {}
            Ok(())
        }
    }

    if !s.trim_start().starts_with('[') {
        return false;
    }
    let mut de = serde_json::Deserializer::from_str(s);
    de.deserialize_seq(Items(sink)).is_ok() && de.end().is_ok()
}

/// Checks the characters of a title, a text or a tag (7.37, rule 9).
fn check_chars(s: &str) -> Result<(), AnnotationReason> {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let refused = match c {
            '\t' | '\n' => false,
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => true,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => true,
            // Zero-width and mark characters, the line and paragraph separators and the byte order mark.
            '\u{200b}'..='\u{200f}' | '\u{61c}' | '\u{2028}' | '\u{2029}' | '\u{feff}' => true,
            '<' => chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')),
            _ => false,
        };
        if refused {
            return Err(AnnotationReason::BadChars);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FrameBuilder, FrameStats};

    fn frame(names: &[&str], types: &[&str], rows: &[&str]) -> Frame {
        let mut b = FrameBuilder::new(
            names.iter().map(|s| (*s).to_owned()).collect(),
            types.iter().map(|s| (*s).to_owned()).collect(),
        )
        .unwrap();
        for r in rows {
            b.push_row(r).unwrap();
        }
        b.finish("q".into(), FrameStats::default())
    }

    #[test]
    fn decimal_forms() {
        assert_eq!(decimal("1794211200", true), Some(1_794_211_200_000));
        assert_eq!(decimal("1794211200.123456789", true), Some(1_794_211_200_123));
        assert_eq!(decimal("1.5", true), Some(1_500));
        assert_eq!(decimal("0", true), Some(0));
        assert_eq!(decimal("-0", true), Some(0));
        assert_eq!(decimal("-0.0001", true), None);
        assert_eq!(decimal("-1", false), None);
        assert_eq!(decimal("1500", false), Some(1_500));
        assert_eq!(decimal("1500.000", false), Some(1_500));
        assert_eq!(decimal("1500.5", false), None);
        assert_eq!(decimal("1e3", false), None);
        assert_eq!(decimal("", false), None);
        assert_eq!(decimal("-", false), None);
        assert_eq!(decimal("1.", false), None);
        assert_eq!(decimal("99999999999999999999999", false), None);
        assert_eq!(decimal("000000000000000000000000000005", false), Some(5));
    }

    #[test]
    fn a_point_a_region_and_the_order() {
        let f = frame(
            &["time", "timeEnd", "title", "text", "tags"],
            &["DateTime64(3)", "Nullable(UInt64)", "String", "String", "Array(String)"],
            &[
                "[\"5.5\", \"8000\", \"b\", \"x\", \"[\\\"p\\\",\\\"q\\\"]\"]",
                "[\"1\", null, \"a\", \"\", \"p, q,,p\"]",
                "[\"1\", \"0\", \"c\", \"\", \"\"]",
            ],
        );
        let marks = annotations_from_frame(&f).unwrap();
        assert_eq!(marks.len(), 3);
        assert_eq!((marks[0].time_ms, marks[0].end_ms, marks[0].title.as_str()), (1_000, None, "a"));
        assert_eq!(marks[0].tags, ["p", "q"]);
        assert_eq!((marks[1].time_ms, marks[1].title.as_str()), (1_000, "c"));
        assert_eq!((marks[2].time_ms, marks[2].end_ms), (5_500, Some(8_000)));
        assert_eq!(marks[2].tags, ["p", "q"]);
    }

    #[test]
    fn faults_hold_no_cell_value() {
        for r in AnnotationReason::ALL {
            assert!(!r.message().contains('<'));
        }
    }

    /// The frame that the client of 8b makes from documents in Extended JSON (7.16c).
    fn mongo_frame(docs: &[&str]) -> Frame {
        use crate::mongo::{FrameCaps, MongoFrameBuilder, MongoValue};
        let mut b = MongoFrameBuilder::new(FrameCaps { max_rows: 1_000, max_bytes: 1 << 20 });
        for d in docs {
            b.push_document(&MongoValue::document_from_extended_json(d).unwrap()).unwrap();
        }
        b.finish("q".into(), 0).unwrap()
    }

    fn reason_of(f: &Frame) -> (AnnotationReason, Option<usize>) {
        let e = annotations_from_frame(f).unwrap_err();
        (e.reason, e.row)
    }

    #[test]
    fn rule_13_a_frame_with_no_column_and_no_row_gives_no_mark() {
        // The empty result of a `mongodb` source has no column (7.16c). It is no fault of the result.
        let f = mongo_frame(&[]);
        assert!(f.columns.is_empty() && f.rows == 0);
        assert_eq!(annotations_from_frame(&f), Ok(Vec::new()));
        // A frame with columns and no row also gives no mark. A frame with a column and no time stays a fault.
        let f = frame(&["time", "title"], &["DateTime", "String"], &[]);
        assert_eq!(annotations_from_frame(&f), Ok(Vec::new()));
        let f = frame(&["title"], &["String"], &[]);
        assert_eq!(reason_of(&f), (AnnotationReason::NoTime, None));
    }

    #[test]
    fn mongodb_dates_strings_and_arrays_give_marks() {
        let f = mongo_frame(&[
            r#"{"_id": {"$oid": "65f1a2b3c4d5e6f708192a3b"}, "time": {"$date": {"$numberLong": "1794214800500"}},
                "timeEnd": {"$date": {"$numberLong": "1794216600000"}}, "title": "Maintenance", "text": "Disk change",
                "tags": ["ops", "disk", "ops"]}"#,
            r#"{"_id": {"$oid": "65f1a2b3c4d5e6f708192a3c"}, "time": {"$date": {"$numberLong": "1794211800123"}},
                "text": "Deploy v1.2", "tags": ["prod", " deploy "]}"#,
        ]);
        // `_id` has no role. The second document lacks `timeEnd` and `title`: `null`, so a point and an empty title.
        let kinds: Vec<&str> = f.columns.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["text", "time", "time", "text", "text", "text"]);
        let marks = annotations_from_frame(&f).unwrap();
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].time_ms, marks[0].end_ms, marks[0].title.as_str()), (1_794_211_800_123, None, ""));
        assert_eq!(
            (marks[0].text.as_str(), marks[0].tags.clone()),
            ("Deploy v1.2", vec!["prod".into(), "deploy".into()])
        );
        assert_eq!((marks[1].time_ms, marks[1].end_ms), (1_794_214_800_500, Some(1_794_216_600_000)));
        assert_eq!(marks[1].tags, ["ops", "disk"]);
        // A string with commas gives the tags too. An array with an item that is not a string is not an array
        // of texts: its JSON text is split at each comma (rule 7), as for ClickHouse.
        let f = mongo_frame(&[r#"{"time": 1, "tags": "a, b,,a"}"#]);
        assert_eq!(annotations_from_frame(&f).unwrap()[0].tags, ["a", "b"]);
        let f = mongo_frame(&[r#"{"time": 1, "tags": ["a", 7]}"#]);
        assert_eq!(annotations_from_frame(&f).unwrap()[0].tags, ["[\"a\"", "7]"]);
        // Two types in one column that 7.16c refuses stop the query before the rules of 7.37.
        use crate::mongo::{FrameCaps, MongoFrameBuilder, MongoValue};
        let mut b = MongoFrameBuilder::new(FrameCaps { max_rows: 10, max_bytes: 1 << 20 });
        b.push_document(&MongoValue::document_from_extended_json(r#"{"time": 1, "tags": ["a"]}"#).unwrap()).unwrap();
        let mixed = MongoValue::document_from_extended_json(r#"{"time": 2, "tags": "b"}"#).unwrap();
        assert!(matches!(b.push_document(&mixed), Err(crate::mongo::FrameFault::Query(_))));
    }

    #[test]
    fn mongodb_numbers_are_whole_epoch_milliseconds() {
        // Int32, Int64, Double and Decimal128 give `number` or `number_text`: whole epoch milliseconds (rule 4).
        for (value, want) in [
            (r#"5000"#, Some(5_000)),
            (r#"{"$numberLong": "1794211200125"}"#, Some(1_794_211_200_125)),
            (r#"{"$numberDouble": "1794211200125.0"}"#, Some(1_794_211_200_125)),
            (r#"{"$numberDecimal": "1794211200125.000"}"#, Some(1_794_211_200_125)),
            (r#"{"$numberDouble": "1.5"}"#, None),
            (r#"{"$numberDecimal": "12.5"}"#, None),
            (r#"{"$numberLong": "-1"}"#, None),
            (r#"{"$numberDouble": "NaN"}"#, None),
            (r#"{"$numberLong": "4294967296000"}"#, None),
        ] {
            let f = mongo_frame(&[&format!(r#"{{"time": {value}}}"#)]);
            match want {
                Some(ms) => assert_eq!(annotations_from_frame(&f).unwrap()[0].time_ms, ms, "{value}"),
                None => assert_eq!(reason_of(&f), (AnnotationReason::BadTime, Some(1)), "{value}"),
            }
        }
        // A missing time is `null`: `bad_time` at its row.
        let f = mongo_frame(&[r#"{"time": {"$date": "2026-11-09T08:00:00Z"}}"#, r#"{"title": "x"}"#]);
        assert_eq!(reason_of(&f), (AnnotationReason::BadTime, Some(2)));
    }

    #[test]
    fn mongodb_types_that_a_role_refuses() {
        // A string, a boolean, an object and an array are not a time.
        for value in [r#""2026-11-09""#, "true", r#"{"a": 1}"#, r#"[1]"#] {
            let f = mongo_frame(&[&format!(r#"{{"time": {value}}}"#)]);
            assert_eq!(reason_of(&f), (AnnotationReason::BadTime, None), "{value}");
        }
        // A boolean and a number are not a text. An ObjectId and an object are texts (7.16c).
        for value in ["true", "5", r#"{"$numberLong": "5"}"#] {
            let f = mongo_frame(&[&format!(r#"{{"time": 5, "title": {value}}}"#)]);
            assert_eq!(reason_of(&f), (AnnotationReason::BadText, None), "{value}");
        }
        let f = mongo_frame(&[r#"{"time": 5, "title": {"$oid": "65f1a2b3c4d5e6f708192a3b"}, "text": {"a": 1}}"#]);
        let m = &annotations_from_frame(&f).unwrap()[0];
        assert_eq!((m.title.as_str(), m.text.as_str()), ("65f1a2b3c4d5e6f708192a3b", r#"{"a":1}"#));
        // An end below the time, and a time and a `Time` in one document: two columns with one role.
        let f = mongo_frame(&[r#"{"time": 9000, "timeEnd": 8000}"#]);
        assert_eq!(reason_of(&f), (AnnotationReason::BadEnd, Some(1)));
        let f = mongo_frame(&[r#"{"time": 9000, "Time": 8000}"#]);
        assert_eq!(reason_of(&f), (AnnotationReason::DuplicateColumn, None));
    }

    #[test]
    fn mongodb_hostile_texts_are_refused() {
        for (field, value, reason) in [
            ("text", r#""<b>x</b>""#, AnnotationReason::BadChars),
            ("title", "\"a\\u202eb\"", AnnotationReason::BadChars),
            ("tags", r#"["ok", "x\u0000"]"#, AnnotationReason::BadChars),
            ("tags", r#"["a","b","c","d","e","f","g","h","i"]"#, AnnotationReason::TooLong),
            ("title", &format!(r#""{}""#, "x".repeat(257)), AnnotationReason::TooLong),
        ] {
            let f = mongo_frame(&[r#"{"time": 1}"#, &format!(r#"{{"time": 2, "{field}": {value}}}"#)]);
            assert_eq!(reason_of(&f), (reason, Some(2)), "{field} {value}");
        }
    }
}
