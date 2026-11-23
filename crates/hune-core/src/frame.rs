//! The frame of 7.16: the one columnar JSON answer of a query.
//!
//! A data source reads its rows and builds a [`Frame`] with a [`FrameBuilder`]. The builder
//! maps each ClickHouse type text to a [`Kind`] with [`map_type`]. It checks each value against
//! the table of 7.16 and stores it in a compact column. The frame writes the JSON of 7.16 with
//! [`Frame::write_json`].
//!
//! This module does no I/O. It does not read a clock or a random source.
//!
//! Storage. A column keeps its values as text in one buffer with one `u32` end offset for each
//! cell. A `bool` column keeps one byte for each cell. Thus a frame takes about the size of
//! the rows that a source reads, and the memory budget of 8.4 holds. A value of the kinds
//! `number` and `time` keeps its exact digits, and the JSON output writes them as they are.

use crate::time::{days_from_civil, days_in_month};
use std::borrow::Cow;
use std::fmt;

/// The kind of a column (7.16): the form that the browser reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Unix seconds, with a fraction for `DateTime64`.
    Time,
    /// A JSON number, or the text `nan`, `inf` or `-inf`.
    Number,
    /// A text with the exact integer or decimal.
    NumberText,
    /// A text.
    Text,
    /// `true` or `false`.
    Bool,
    /// Only `null`.
    Null,
}

impl Kind {
    /// Gives the kind as the text of the member `kind` of 7.16.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Time => "time",
            Kind::Number => "number",
            Kind::NumberText => "number_text",
            Kind::Text => "text",
            Kind::Bool => "bool",
            Kind::Null => "null",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The value check of a column: the kind and the accepted forms of 7.16 (second table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueForm {
    /// A JSON number, `"nan"`, `"inf"` or `"-inf"`. Kind `number`.
    Number,
    /// A text that matches `^-?[0-9]+$`. Kind `number_text`.
    Integer,
    /// A text that matches `^-?[0-9]+(\.[0-9]+)?$`. Kind `number_text`.
    Decimal,
    /// A text or a number that matches `^-?[0-9]+(\.[0-9]{1,9})?$`. Kind `time`.
    DateTime,
    /// A text `YYYY-MM-DD` that is a real date. Kind `time`.
    Date,
    /// `true` or `false`. Kind `bool`.
    Bool,
    /// Any JSON value. Kind `text`.
    Text,
    /// Only `null`. Kind `null`.
    Null,
}

impl ValueForm {
    /// Gives the kind of the columns that use this form.
    pub fn kind(self) -> Kind {
        match self {
            ValueForm::Number => Kind::Number,
            ValueForm::Integer | ValueForm::Decimal => Kind::NumberText,
            ValueForm::DateTime | ValueForm::Date => Kind::Time,
            ValueForm::Bool => Kind::Bool,
            ValueForm::Text => Kind::Text,
            ValueForm::Null => Kind::Null,
        }
    }
}

/// Tells if `s` has the form `name(...)`, with the last byte of `s` as the closing parenthesis.
/// Gives the text between the parentheses.
fn inner_of<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.strip_prefix('(')?;
    let inner = rest.strip_suffix(')')?;
    // The first parenthesis must close at the last byte: `inner` stays balanced.
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for &b in inner.as_bytes() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'`' => quote = Some(b),
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    (depth == 0 && quote.is_none()).then_some(inner)
}

/// Gives the text after the first comma of `s` that is outside parentheses and quotes.
fn after_first_top_level_comma(s: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'`' => quote = Some(b),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return Some(s[i + 1..].trim()),
            _ => {}
        }
    }
    None
}

/// Removes the wrappers `Nullable(...)`, `LowCardinality(...)` and `SimpleAggregateFunction(f, T)`
/// at any depth and in any order. Gives the type that remains.
pub fn strip_wrappers(type_text: &str) -> &str {
    let mut t = type_text.trim();
    loop {
        if let Some(inner) = inner_of(t, "Nullable").or_else(|| inner_of(t, "LowCardinality")) {
            t = inner.trim();
        } else if let Some(inner) = inner_of(t, "SimpleAggregateFunction") {
            match after_first_top_level_comma(inner) {
                Some(rest) => t = rest,
                None => return t,
            }
        } else {
            return t;
        }
    }
}

/// Maps a ClickHouse type text to its value form, with the table of 7.16.
///
/// The function never fails: a type that the table does not list maps to [`ValueForm::Text`].
pub fn map_type(type_text: &str) -> ValueForm {
    let t = strip_wrappers(type_text);
    let name = t.split('(').next().unwrap_or(t).trim();
    match name {
        "UInt8" | "UInt16" | "UInt32" | "Int8" | "Int16" | "Int32" | "Float32" | "Float64" | "BFloat16" => {
            ValueForm::Number
        }
        "UInt64" | "Int64" | "UInt128" | "Int128" | "UInt256" | "Int256" => ValueForm::Integer,
        "Decimal" | "Decimal32" | "Decimal64" | "Decimal128" | "Decimal256" => ValueForm::Decimal,
        "Bool" => ValueForm::Bool,
        "Date" | "Date32" => ValueForm::Date,
        "DateTime" | "DateTime64" => ValueForm::DateTime,
        "Nothing" => ValueForm::Null,
        // String, FixedString, UUID, IPv4, IPv6, Enum8, Enum16 and every other type.
        _ => ValueForm::Text,
    }
}

/// Gives the kind of a ClickHouse type text (7.16).
pub fn kind_of_type(type_text: &str) -> Kind {
    map_type(type_text).kind()
}

/// The error of a header, a row or a value that breaks 8.4 or 7.16.
///
/// The message names the column and the rule. It never holds a value of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameError {
    message: String,
}

impl FrameError {
    fn new(message: impl Into<String>) -> Self {
        FrameError { message: message.into() }
    }

    /// Gives the message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FrameError {}

/// A value of a frame cell. The text variants borrow from the column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameValue<'a> {
    /// A NULL.
    Null,
    /// A boolean.
    Bool(bool),
    /// A JSON number with its exact digits. The JSON output writes the digits as they are.
    Number(&'a str),
    /// A text. The JSON output writes it as a JSON string.
    Text(&'a str),
}

/// The values of one column.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Data {
    /// Text cells in one buffer. `ends[i]` is the end of cell `i`; `nulls[i]` marks a NULL.
    Text { buf: String, ends: Vec<u32>, nulls: Option<Vec<bool>> },
    /// One byte for each cell: 0 false, 1 true, 2 NULL.
    Bool(Vec<u8>),
    /// A column of the kind `null`: only the count of cells.
    Null(usize),
}

impl Data {
    fn len(&self) -> usize {
        match self {
            Data::Text { ends, .. } => ends.len(),
            Data::Bool(v) => v.len(),
            Data::Null(n) => *n,
        }
    }

    fn push_null(&mut self) {
        match self {
            Data::Text { buf, ends, nulls } => {
                let n = ends.len();
                nulls.get_or_insert_with(|| vec![false; n]).push(true);
                // A text column checks the size in `push_text`; a NULL adds no byte.
                ends.push(buf.len() as u32);
            }
            Data::Bool(v) => v.push(2),
            Data::Null(n) => *n += 1,
        }
    }

    fn push_text(&mut self, text: &str) -> Result<(), FrameError> {
        if let Data::Text { buf, ends, nulls } = self {
            if buf.len().saturating_add(text.len()) > u32::MAX as usize {
                return Err(FrameError::new("a column holds more than 4 GiB of text"));
            }
            buf.push_str(text);
            ends.push(buf.len() as u32);
            if let Some(n) = nulls {
                n.push(false);
            }
        }
        Ok(())
    }

    fn shrink(&mut self) {
        match self {
            Data::Text { buf, ends, nulls } => {
                buf.shrink_to_fit();
                ends.shrink_to_fit();
                if let Some(n) = nulls {
                    n.shrink_to_fit();
                }
            }
            Data::Bool(v) => v.shrink_to_fit(),
            Data::Null(_) => {}
        }
    }
}

/// One column of a frame: its name, its ClickHouse type text, its kind and its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column name, exactly as the source gives it. Two columns can have the same name.
    pub name: String,
    /// The ClickHouse type text, exactly as the source gives it.
    pub type_text: String,
    /// The kind of 7.16.
    pub kind: Kind,
    form: ValueForm,
    data: Data,
}

impl Column {
    /// Makes an empty column and finds its kind from the type text.
    pub fn new(name: impl Into<String>, type_text: impl Into<String>) -> Self {
        let type_text = type_text.into();
        let form = map_type(&type_text);
        let data = match form {
            ValueForm::Bool => Data::Bool(Vec::new()),
            ValueForm::Null => Data::Null(0),
            _ => Data::Text { buf: String::new(), ends: Vec::new(), nulls: None },
        };
        Column { name: name.into(), type_text, kind: form.kind(), form, data }
    }

    /// Gives the number of values of the column.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Tells if the column has no value.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gives the value at `row`, or `None` when `row` is outside the column.
    pub fn value(&self, row: usize) -> Option<FrameValue<'_>> {
        match &self.data {
            Data::Bool(v) => Some(match *v.get(row)? {
                0 => FrameValue::Bool(false),
                1 => FrameValue::Bool(true),
                _ => FrameValue::Null,
            }),
            Data::Null(n) => (row < *n).then_some(FrameValue::Null),
            Data::Text { buf, ends, nulls } => {
                let end = *ends.get(row)? as usize;
                if nulls.as_ref().is_some_and(|n| n[row]) {
                    return Some(FrameValue::Null);
                }
                let start = if row == 0 { 0 } else { ends[row - 1] as usize };
                let text = &buf[start..end];
                let raw_number = match self.kind {
                    Kind::Time => true,
                    // `nan`, `inf` and `-inf` are the only numbers that do not end with a digit.
                    Kind::Number => text.as_bytes().last().is_some_and(u8::is_ascii_digit),
                    _ => false,
                };
                Some(if raw_number { FrameValue::Number(text) } else { FrameValue::Text(text) })
            }
        }
    }

    /// Iterates over the values of the column.
    pub fn values(&self) -> impl Iterator<Item = FrameValue<'_>> + '_ {
        (0..self.len()).filter_map(|i| self.value(i))
    }

    /// Gives the heap memory of the values, in bytes.
    pub fn heap_bytes(&self) -> usize {
        match &self.data {
            Data::Text { buf, ends, nulls } => {
                buf.capacity() + ends.capacity() * 4 + nulls.as_ref().map_or(0, |n| n.capacity())
            }
            Data::Bool(v) => v.capacity(),
            Data::Null(_) => 0,
        }
    }

    /// Checks one raw JSON value against the form of the column and adds it.
    ///
    /// `raw` is one JSON value with no space around it. The error never holds the value.
    fn push_raw(&mut self, raw: &str) -> Result<(), String> {
        if raw == "null" {
            self.data.push_null();
            return Ok(());
        }
        let fail = |rule: &str| Err(format!("{rule}, found {}", json_type_name(raw)));
        match self.form {
            ValueForm::Null => fail("expected null"),
            ValueForm::Bool => match raw {
                "true" => self.push_bool(true),
                "false" => self.push_bool(false),
                _ => fail("expected true, false or null"),
            },
            ValueForm::Number => {
                if is_json_number(raw) {
                    return self.push_text_checked(raw);
                }
                match json_string_content(raw).as_deref() {
                    Some(s @ ("nan" | "inf" | "-inf")) => self.push_text_checked(s),
                    _ => fail("expected a number, \"nan\", \"inf\", \"-inf\" or null"),
                }
            }
            ValueForm::Integer => match json_string_content(raw) {
                Some(s) if is_integer_text(&s) => self.push_text_checked(&s),
                _ => fail("expected a text of digits or null"),
            },
            ValueForm::Decimal => match json_string_content(raw) {
                Some(s) if is_decimal_text(&s, usize::MAX) => self.push_text_checked(&s),
                _ => fail("expected a text of a decimal or null"),
            },
            ValueForm::DateTime => {
                let digits = if raw.starts_with('"') { json_string_content(raw) } else { Some(Cow::Borrowed(raw)) };
                match digits {
                    // The leading-zero rule keeps the JSON output valid: a JSON number has none.
                    Some(s) if is_decimal_text(&s, 9) && !has_leading_zero(&s) => self.push_text_checked(&s),
                    _ => fail("expected Unix seconds as a text or a number, or null"),
                }
            }
            ValueForm::Date => match json_string_content(raw).as_deref().and_then(parse_date) {
                Some(seconds) => self.push_text_checked(&seconds.to_string()),
                None => fail("expected a date YYYY-MM-DD or null"),
            },
            ValueForm::Text => {
                if raw.starts_with('"') {
                    match json_string_content(raw) {
                        Some(s) => self.push_text_checked(&s),
                        None => fail("expected valid JSON text"),
                    }
                } else {
                    self.push_text_checked(&compact_json(raw))
                }
            }
        }
    }

    fn push_bool(&mut self, v: bool) -> Result<(), String> {
        if let Data::Bool(bits) = &mut self.data {
            bits.push(u8::from(v));
        }
        Ok(())
    }

    fn push_text_checked(&mut self, s: &str) -> Result<(), String> {
        self.data.push_text(s).map_err(|e| e.message)
    }
}

/// Names the JSON type of a raw value, for an error message.
fn json_type_name(raw: &str) -> &'static str {
    match raw.as_bytes().first() {
        Some(b'"') => "a JSON string",
        Some(b'[') => "a JSON array",
        Some(b'{') => "a JSON object",
        Some(b't' | b'f') => "a JSON boolean",
        Some(b'n') => "null",
        _ => "a JSON number",
    }
}

/// Gives the content of a JSON string, or `None` when `raw` is not a JSON string.
fn json_string_content(raw: &str) -> Option<Cow<'_, str>> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    if !inner.contains('\\') {
        return Some(Cow::Borrowed(inner));
    }
    serde_json::from_str::<String>(raw).ok().map(Cow::Owned)
}

/// Tells if `s` is a JSON number (RFC 8259).
fn is_json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}

/// Tells if `s` matches `^-?[0-9]+$`.
fn is_integer_text(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())
}

/// Tells if `s` matches `^-?[0-9]+(\.[0-9]{1,max_fraction})?$`.
fn is_decimal_text(s: &str, max_fraction: usize) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
    digits(int) && frac.is_none_or(|f| digits(f) && f.len() <= max_fraction)
}

/// Tells if the integer part of a decimal text has a zero followed by another digit.
fn has_leading_zero(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let int = body.split('.').next().unwrap_or(body);
    int.len() > 1 && int.starts_with('0')
}

/// Reads `YYYY-MM-DD`. Gives the Unix seconds of 00:00:00 UTC of that day, or `None` when the
/// text is not a real date.
pub fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = &b[r];
        part.iter().all(u8::is_ascii_digit).then(|| part.iter().fold(0i64, |a, d| a * 10 + i64::from(d - b'0')))
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// Removes the insignificant white space of one valid JSON value.
fn compact_json(raw: &str) -> Cow<'_, str> {
    if !raw.bytes().any(|b| matches!(b, b' ' | b'\n' | b'\t' | b'\r')) {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;
    for c in raw.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if !matches!(c, ' ' | '\n' | '\t' | '\r') {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Gives the raw text of each item of a JSON array that serde has checked.
struct ArrayItems<'a> {
    text: &'a str,
    pos: usize,
    done: bool,
}

impl<'a> ArrayItems<'a> {
    /// `text` MUST be a valid JSON array.
    fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut pos = 0;
        while bytes.get(pos).is_some_and(|b| b.is_ascii_whitespace()) {
            pos += 1;
        }
        // Skip the opening bracket.
        pos += 1;
        ArrayItems { text, pos, done: false }
    }
}

impl<'a> Iterator for ArrayItems<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let b = self.text.as_bytes();
        let skip_ws = |mut p: usize| {
            while b.get(p).is_some_and(|c| c.is_ascii_whitespace()) {
                p += 1;
            }
            p
        };
        let start = skip_ws(self.pos);
        if b.get(start) == Some(&b']') {
            self.done = true;
            return None;
        }
        // Scan one value: a string, a nested array or object, or a scalar word.
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = start;
        while let Some(&c) = b.get(end) {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
            } else {
                match c {
                    b'"' => in_string = true,
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                        if depth == 0 {
                            end += 1;
                            break;
                        }
                    }
                    b',' | b' ' | b'\t' | b'\n' | b'\r' if depth == 0 => break,
                    _ => {}
                }
            }
            end += 1;
        }
        let item = &self.text[start..end];
        let after = skip_ws(end);
        match b.get(after) {
            Some(b',') => self.pos = after + 1,
            _ => {
                self.pos = after;
                self.done = true;
            }
        }
        Some(item)
    }
}

/// Reads the header lines of 8.4: a JSON array of texts.
pub fn parse_header_line(line: &str, what: &str) -> Result<Vec<String>, FrameError> {
    serde_json::from_str::<Vec<String>>(line)
        .map_err(|_| FrameError::new(format!("the line of the column {what} is not a JSON array of texts")))
}

/// Tells if `line` is one valid JSON array.
fn is_json_array(line: &str) -> bool {
    line.trim_start().starts_with('[') && serde_json::from_str::<serde::de::IgnoredAny>(line).is_ok()
}

/// The `stats` of a frame (7.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameStats {
    /// The time from the send of the request to the end of the answer, in milliseconds.
    pub elapsed_ms: u64,
    /// `read_rows` of `X-ClickHouse-Summary`, or `None`.
    pub read_rows: Option<u64>,
    /// `read_bytes` of `X-ClickHouse-Summary`, or `None`.
    pub read_bytes: Option<u64>,
    /// `result_bytes` of `X-ClickHouse-Summary`, or `None`.
    pub result_bytes: Option<u64>,
}

/// One frame (7.16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The etag of the request. The server sets it. `None` in a preview.
    pub etag: Option<String>,
    /// The query id of 8.10.
    pub query_id: String,
    /// `I` of 7.4. The server sets it. The JSON has no member when it is `None`.
    pub interval_ms: Option<u64>,
    /// The columns, in the order of the SQL.
    pub columns: Vec<Column>,
    /// The number of rows.
    pub rows: usize,
    /// The statistics.
    pub stats: FrameStats,
}

impl Frame {
    /// Gives the heap memory of the values and the names, in bytes.
    pub fn approx_bytes(&self) -> usize {
        self.columns.iter().map(|c| c.heap_bytes() + c.name.len() + c.type_text.len()).sum()
    }

    /// Writes the compact JSON of 7.16 at the end of `out`.
    pub fn write_json(&self, out: &mut String) {
        out.push('{');
        if let Some(etag) = &self.etag {
            out.push_str("\"etag\":");
            write_json_string(out, etag);
            out.push(',');
        }
        out.push_str("\"query_id\":");
        write_json_string(out, &self.query_id);
        if let Some(i) = self.interval_ms {
            out.push_str(",\"interval_ms\":");
            out.push_str(&i.to_string());
        }
        out.push_str(",\"columns\":[");
        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            write_json_string(out, &c.name);
            out.push_str(",\"type\":");
            write_json_string(out, &c.type_text);
            out.push_str(",\"kind\":\"");
            out.push_str(c.kind.as_str());
            out.push_str("\"}");
        }
        out.push_str("],\"values\":[");
        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('[');
            for (j, v) in c.values().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                match v {
                    FrameValue::Null => out.push_str("null"),
                    FrameValue::Bool(true) => out.push_str("true"),
                    FrameValue::Bool(false) => out.push_str("false"),
                    FrameValue::Number(digits) => out.push_str(digits),
                    FrameValue::Text(t) => write_json_string(out, t),
                }
            }
            out.push(']');
        }
        out.push_str("],\"rows\":");
        out.push_str(&self.rows.to_string());
        out.push_str(",\"stats\":{\"elapsed_ms\":");
        out.push_str(&self.stats.elapsed_ms.to_string());
        for (name, v) in [
            ("read_rows", self.stats.read_rows),
            ("read_bytes", self.stats.read_bytes),
            ("result_bytes", self.stats.result_bytes),
        ] {
            out.push_str(",\"");
            out.push_str(name);
            out.push_str("\":");
            match v {
                Some(n) => out.push_str(&n.to_string()),
                None => out.push_str("null"),
            }
        }
        out.push_str("}}");
    }

    /// Gives the compact JSON of 7.16.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(self.approx_bytes() + 256);
        self.write_json(&mut out);
        out
    }
}

/// Writes `s` as a JSON string. It escapes `"`, `\` and the control characters only.
pub fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u00");
                out.push(char::from_digit(c as u32 >> 4, 16).unwrap_or('0'));
                out.push(char::from_digit(c as u32 & 15, 16).unwrap_or('0'));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Builds a frame row by row from the lines of 8.4.
#[derive(Debug)]
pub struct FrameBuilder {
    columns: Vec<Column>,
    rows: usize,
}

impl FrameBuilder {
    /// Makes a builder from the two header lines of 8.4: the column names and the type texts.
    ///
    /// # Errors
    /// The lists have no column, or they differ in length.
    pub fn new(names: Vec<String>, types: Vec<String>) -> Result<Self, FrameError> {
        if names.is_empty() {
            return Err(FrameError::new("the answer has no column"));
        }
        if names.len() != types.len() {
            return Err(FrameError::new("the list of names and the list of types differ in length"));
        }
        let columns = names.into_iter().zip(types).map(|(n, t)| Column::new(n, t)).collect();
        Ok(FrameBuilder { columns, rows: 0 })
    }

    /// Makes a builder from the text of the first two lines of 8.4.
    pub fn from_header_lines(names_line: &str, types_line: &str) -> Result<Self, FrameError> {
        FrameBuilder::new(parse_header_line(names_line, "names")?, parse_header_line(types_line, "types")?)
    }

    /// Gives the number of columns.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Gives the number of rows that the builder holds.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Checks one row line of 8.4 and adds it.
    ///
    /// # Errors
    /// The line is not a JSON array, or it has a wrong number of values.
    /// A value does not fit its kind (7.16). A builder that gave an error MUST NOT be used again.
    pub fn push_row(&mut self, line: &str) -> Result<(), FrameError> {
        if !is_json_array(line) {
            return Err(FrameError::new("a row is not a JSON array"));
        }
        let n = self.columns.len();
        let mut count = 0usize;
        for raw in ArrayItems::new(line) {
            let Some(col) = self.columns.get_mut(count) else {
                return Err(FrameError::new(format!("a row has more than {n} values")));
            };
            col.push_raw(raw).map_err(|m| FrameError::new(format!("column {} ({}): {m}", count + 1, col.type_text)))?;
            count += 1;
        }
        if count != n {
            return Err(FrameError::new(format!("a row has {count} values and the header has {n}")));
        }
        self.rows += 1;
        Ok(())
    }

    /// Ends the frame.
    pub fn finish(mut self, query_id: String, stats: FrameStats) -> Frame {
        for c in &mut self.columns {
            c.data.shrink();
        }
        Frame { etag: None, query_id, interval_ms: None, columns: self.columns, rows: self.rows, stats }
    }
}
