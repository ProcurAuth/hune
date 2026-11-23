// SPDX-License-Identifier: MIT OR Apache-2.0
//! The strict JSON parser of the profile J (7.28, rules J1 to J6) for the text of an author query.
//!
//! The text is the SQL lines of a panel, joined with one LF between two lines. The parser needs no
//! MongoDB crate. It is not the lenient reader of [`MongoValue::from_extended_json`], which reads
//! data of the server. This parser refuses what the profile refuses:
//!
//! - J1: a byte order mark, a comment, a trailing comma, a value after the first one, and a blank
//!   byte other than space, TAB, LF and CR.
//! - J2: a duplicate member name in one object.
//! - J3: U+0000 in a string or in a key, also when it comes from the escape `\u0000`.
//! - J4: an integer literal is an Int32, else an Int64, else an error. `-0` is the Int32 0.
//! - J5: a literal with a fraction or an exponent is a finite Double.
//! - J6: the typed objects `$date`, `$oid`, `$numberLong` and `$numberDecimal` are one typed value.
//!   Another form of Extended JSON stays an object. The grammar of 7.30 refuses its `$` key.
//!
//! The limits of 7.28 (depth, values, key size, string size, array size) apply while the parser
//! reads. [`parse_text`] also gives a [`Span`] tree, so that a later check can name the position of
//! a fault in the text. [`to_query_json`] writes a tree as compact text that the parser reads back.

use std::collections::HashSet;

use crate::mongo::limits::{
    MAX_QUERY_ARRAY_ITEMS, MAX_QUERY_DEPTH, MAX_QUERY_KEY_BYTES, MAX_QUERY_NODES, MAX_QUERY_STRING_BYTES,
};
use crate::mongo::value::{Decimal128Text, Document, MongoValue};
use crate::time::{days_from_civil, days_in_month};

/// One step of the path from the top object to a value: a key or an array index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Seg {
    /// A member name.
    Key(String),
    /// An array index, from 0.
    Index(usize),
}

/// Writes a path as a JSON pointer (RFC 6901), for example `/aggregate/2/$out`. The empty path is
/// the empty text.
pub fn pointer(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        out.push('/');
        match seg {
            Seg::Key(k) => {
                for c in k.chars() {
                    match c {
                        '~' => out.push_str("~0"),
                        '/' => out.push_str("~1"),
                        c => out.push(c),
                    }
                }
            }
            Seg::Index(i) => out.push_str(&i.to_string()),
        }
    }
    out
}

/// The cause of a fault of the parser, as the code of 4.37 that the save uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorKind {
    /// A fault of the profile J: `bad_format`.
    BadFormat,
    /// The depth is above `MAX_QUERY_DEPTH`: `too_deep`.
    TooDeep,
    /// A count or a size is above a limit of 7.28: `too_large`.
    TooLarge,
}

/// A fault of the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// The cause.
    pub kind: JsonErrorKind,
    /// The byte offset in the text.
    pub offset: usize,
    /// The English message. It holds no part of the text.
    pub message: String,
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for JsonError {}

/// The positions of a value in the text. The tree has the shape of the value tree: one child for
/// each item of an array and one for each member of an object, in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    /// The byte offset of the key of this member. For an array item and for the root, it is `start`.
    pub key_start: usize,
    /// The byte offset of the first byte of the value.
    pub start: usize,
    /// The spans of the items or of the members.
    pub children: Vec<Span>,
}

impl Span {
    /// Gives the byte offset of the value at `path` in `root`. With `at_key`, the offset of the last
    /// key is given. A path that the tree does not hold gives the offset of the last value found.
    pub fn locate(&self, root: &MongoValue, path: &[Seg], at_key: bool) -> usize {
        let mut span = self;
        let mut value = root;
        let mut offset = self.start;
        for (n, seg) in path.iter().enumerate() {
            let step = match (seg, value) {
                (Seg::Key(k), MongoValue::Document(d)) => d.iter().position(|(dk, _)| dk == k).map(|i| (i, &d[i].1)),
                (Seg::Index(i), MongoValue::Array(a)) => a.get(*i).map(|v| (*i, v)),
                _ => None,
            };
            let Some((i, v)) = step else { return offset };
            let Some(child) = span.children.get(i) else { return offset };
            let last = n + 1 == path.len();
            offset = if last && at_key { child.key_start } else { child.start };
            span = child;
            value = v;
        }
        offset
    }
}

/// The result of [`parse_text`].
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The value.
    pub value: MongoValue,
    /// Its positions.
    pub span: Span,
}

/// The names of the typed objects (J6).
const TYPED_KEYS: [&str; 4] = ["$date", "$oid", "$numberLong", "$numberDecimal"];

/// Reads a query text with the profile J.
///
/// # Errors
///
/// A fault of J1 to J6 or a limit of 7.28 gives a [`JsonError`] with the byte offset.
pub fn parse_text(text: &str) -> Result<Parsed, JsonError> {
    let mut p = Reader { b: text.as_bytes(), pos: 0, nodes: 0, long_digits: 0 };
    if text.starts_with('\u{feff}') {
        return Err(p.err(JsonErrorKind::BadFormat, "the text starts with a byte order mark"));
    }
    p.skip_blank();
    let (value, span) = p.value(0)?;
    p.skip_blank();
    if p.pos != p.b.len() {
        return Err(p.err(JsonErrorKind::BadFormat, "a value after the first one"));
    }
    Ok(Parsed { value, span })
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    nodes: usize,
    /// The count of digits of the last `$numberLong` object that the reader made.
    long_digits: usize,
}

impl Reader<'_> {
    fn err(&self, kind: JsonErrorKind, message: impl Into<String>) -> JsonError {
        JsonError { kind, offset: self.pos, message: message.into() }
    }

    fn bad(&self, message: impl Into<String>) -> JsonError {
        self.err(JsonErrorKind::BadFormat, message)
    }

    fn skip_blank(&mut self) {
        while matches!(self.b.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.b.get(self.pos) == Some(&c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn count_node(&mut self) -> Result<(), JsonError> {
        self.nodes += 1;
        if self.nodes > MAX_QUERY_NODES {
            return Err(self.err(JsonErrorKind::TooLarge, "the tree has more than 100000 values"));
        }
        Ok(())
    }

    fn word(&mut self, w: &str, v: MongoValue) -> Result<MongoValue, JsonError> {
        if self.b[self.pos..].starts_with(w.as_bytes()) {
            self.pos += w.len();
            Ok(v)
        } else {
            Err(self.bad("an invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<(MongoValue, Span), JsonError> {
        self.count_node()?;
        let start = self.pos;
        let v = match self.b.get(self.pos) {
            None => return Err(self.bad("the text ends before a value")),
            Some(b'n') => self.word("null", MongoValue::Null)?,
            Some(b't') => self.word("true", MongoValue::Bool(true))?,
            Some(b'f') => self.word("false", MongoValue::Bool(false))?,
            Some(b'"') => MongoValue::String(self.string(MAX_QUERY_STRING_BYTES, "a string")?),
            Some(b'[') => return self.array(depth + 1),
            Some(b'{') => return self.object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.number()?,
            Some(_) => return Err(self.bad("an unexpected byte")),
        };
        Ok((v, Span { key_start: start, start, children: Vec::new() }))
    }

    fn array(&mut self, depth: usize) -> Result<(MongoValue, Span), JsonError> {
        if depth > MAX_QUERY_DEPTH {
            return Err(self.err(JsonErrorKind::TooDeep, "the depth of objects and arrays is above 32"));
        }
        let start = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        let mut children = Vec::new();
        self.skip_blank();
        if self.eat(b']') {
            return Ok((MongoValue::Array(items), Span { key_start: start, start, children }));
        }
        loop {
            self.skip_blank();
            if items.len() >= MAX_QUERY_ARRAY_ITEMS {
                return Err(self.err(JsonErrorKind::TooLarge, "an array has more than 10000 items"));
            }
            let (v, s) = self.value(depth)?;
            items.push(v);
            children.push(s);
            self.skip_blank();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok((MongoValue::Array(items), Span { key_start: start, start, children }));
            }
            return Err(self.bad("expected a comma or the end of the array"));
        }
    }

    fn object(&mut self, depth: usize) -> Result<(MongoValue, Span), JsonError> {
        if depth > MAX_QUERY_DEPTH {
            return Err(self.err(JsonErrorKind::TooDeep, "the depth of objects and arrays is above 32"));
        }
        let start = self.pos;
        self.pos += 1;
        let mut members: Document = Vec::new();
        let mut children: Vec<Span> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.skip_blank();
        if self.eat(b'}') {
            return Ok((MongoValue::Document(members), Span { key_start: start, start, children }));
        }
        loop {
            self.skip_blank();
            let key_start = self.pos;
            if self.b.get(self.pos) != Some(&b'"') {
                return Err(self.bad("expected a key"));
            }
            let key = self.string(MAX_QUERY_KEY_BYTES, "a key")?;
            if !seen.insert(key.clone()) {
                self.pos = key_start;
                return Err(self.bad("a duplicate member name in one object"));
            }
            self.skip_blank();
            if !self.eat(b':') {
                return Err(self.bad("expected a colon"));
            }
            self.skip_blank();
            let (v, mut s) = if key == "$date" && self.b.get(self.pos) == Some(&b'{') {
                self.date_millis_object(depth)?
            } else {
                self.value(depth)?
            };
            s.key_start = key_start;
            members.push((key, v));
            children.push(s);
            self.skip_blank();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                break;
            }
            return Err(self.bad("expected a comma or the end of the object"));
        }
        if members.iter().any(|(k, _)| TYPED_KEYS.contains(&k.as_str())) {
            let end = self.pos;
            self.pos = start;
            let v = self.typed(&members)?;
            self.pos = end;
            return Ok((v, Span { key_start: start, start, children: Vec::new() }));
        }
        Ok((MongoValue::Document(members), Span { key_start: start, start, children }))
    }

    /// Reads the value of `$date` when it is an object. It MUST be `{"$numberLong": "<digits>"}` with 16
    /// digits at most (7.28).
    fn date_millis_object(&mut self, depth: usize) -> Result<(MongoValue, Span), JsonError> {
        let at = self.pos;
        self.long_digits = 0;
        let (v, s) = self.value(depth)?;
        if !matches!(v, MongoValue::Int64(_)) || self.long_digits == 0 || self.long_digits > 16 {
            self.pos = at;
            return Err(self.bad("the object of $date must be {\"$numberLong\": \"<1 to 16 digits>\"}"));
        }
        match v {
            MongoValue::Int64(n) => Ok((MongoValue::DateMs(n), s)),
            _ => Err(self.bad("the object of $date must be {\"$numberLong\": \"<1 to 16 digits>\"}")),
        }
    }

    /// Makes the typed value of an object that has a key of J6. The object has one member.
    fn typed(&mut self, members: &Document) -> Result<MongoValue, JsonError> {
        if members.len() != 1 {
            return Err(self.bad("a typed object has exactly one member"));
        }
        let (key, v) = (&members[0].0, &members[0].1);
        match (key.as_str(), v) {
            ("$date", MongoValue::String(t)) => match parse_iso_date(t) {
                Some(ms) => Ok(MongoValue::DateMs(ms)),
                None => Err(self.bad("$date must be YYYY-MM-DDTHH:MM:SS, 0 to 3 decimals and Z, a real UTC date")),
            },
            ("$date", MongoValue::DateMs(n)) => Ok(MongoValue::DateMs(*n)),
            ("$date", _) => Err(self.bad("$date must be a text or a $numberLong object")),
            ("$oid", MongoValue::String(t)) => MongoValue::object_id_from_hex(t)
                .ok_or_else(|| self.bad("$oid must be a text of 24 hexadecimal digits")),
            ("$oid", _) => Err(self.bad("$oid must be a text of 24 hexadecimal digits")),
            ("$numberLong", MongoValue::String(t)) => {
                let digits = t.strip_prefix('-').unwrap_or(t);
                let ok = (1..=19).contains(&digits.len()) && digits.bytes().all(|c| c.is_ascii_digit());
                match t.parse::<i64>() {
                    Ok(n) if ok => {
                        self.long_digits = digits.len();
                        Ok(MongoValue::Int64(n))
                    }
                    _ => Err(self.bad("$numberLong must be a text of 1 to 19 digits that fits in an Int64")),
                }
            }
            ("$numberLong", _) => Err(self.bad("$numberLong must be a text of digits")),
            ("$numberDecimal", MongoValue::String(t)) => Decimal128Text::parse_query(t)
                .map(MongoValue::Decimal128)
                .ok_or_else(|| self.bad("$numberDecimal must be a decimal text of 34 digits at most")),
            ("$numberDecimal", _) => Err(self.bad("$numberDecimal must be a text")),
            _ => Err(self.bad("a typed object has exactly one member")),
        }
    }

    fn number(&mut self) -> Result<MongoValue, JsonError> {
        let start = self.pos;
        self.eat(b'-');
        let int_start = self.pos;
        while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        let int = &self.b[int_start..self.pos];
        if int.is_empty() || (int.len() > 1 && int[0] == b'0') {
            return Err(self.bad("an invalid number"));
        }
        let mut float = false;
        if self.eat(b'.') {
            float = true;
            let s = self.pos;
            while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if s == self.pos {
                return Err(self.bad("an invalid number"));
            }
        }
        if matches!(self.b.get(self.pos), Some(b'e' | b'E')) {
            float = true;
            self.pos += 1;
            if matches!(self.b.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let s = self.pos;
            while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if s == self.pos {
                return Err(self.bad("an invalid number"));
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.pos]).map_err(|_| self.bad("an invalid number"))?;
        if float {
            return match text.parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(MongoValue::Double(f)),
                _ => Err(self.bad("the number is not finite")),
            };
        }
        match text.parse::<i64>() {
            Ok(n) => Ok(i32::try_from(n).map_or(MongoValue::Int64(n), MongoValue::Int32)),
            Err(_) => Err(self.bad("the integer is above the range of Int64")),
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let s = self.b.get(self.pos..self.pos + 4).ok_or_else(|| self.bad("an invalid escape"))?;
        let mut v = 0u32;
        for &c in s {
            v = v * 16 + (c as char).to_digit(16).ok_or_else(|| self.bad("an invalid escape"))?;
        }
        self.pos += 4;
        Ok(v)
    }

    /// Reads a string. `limit` is the largest size in bytes (a key or a string value).
    fn string(&mut self, limit: usize, what: &str) -> Result<String, JsonError> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.pos) else {
                return Err(self.bad("the string has no end"));
            };
            self.pos += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.b.get(self.pos) else {
                        return Err(self.bad("an invalid escape"));
                    };
                    self.pos += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.bad("an invalid surrogate pair"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.bad("an invalid surrogate pair"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(cp).ok_or_else(|| self.bad("an invalid escape"))?
                        }
                        _ => return Err(self.bad("an invalid escape")),
                    };
                    if ch == '\0' {
                        return Err(self.bad(format!("{what} holds U+0000 (J3)")));
                    }
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                0x00..=0x1f => return Err(self.bad("a control character in a string")),
                _ => out.push(c),
            }
            if out.len() > limit {
                return Err(self.err(JsonErrorKind::TooLarge, format!("{what} has more than {limit} bytes")));
            }
        }
        // The input is a `str` and the escapes give whole characters, so the bytes are UTF-8.
        String::from_utf8(out).map_err(|_| self.bad("the string is not UTF-8"))
    }
}

/// Reads `YYYY-MM-DDTHH:MM:SS` with 0 to 3 decimals and `Z`. The date MUST be real, from 0001-01-01 to
/// 9999-12-31 (7.28). It gives the epoch milliseconds.
pub fn parse_iso_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || !s.is_ascii() {
        return None;
    }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[b.len() - 1] != b'Z' {
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
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let ms = match b.len() {
        20 => 0,
        22..=24 => {
            if b[19] != b'.' {
                return None;
            }
            let digits = &s[20..b.len() - 1];
            let n = num(20..b.len() - 1)?;
            n * 10i64.pow(3 - digits.len() as u32)
        }
        _ => return None,
    };
    if y < 1 || !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) || h > 23 || mi > 59 || se > 59 {
        return None;
    }
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se) * 1000 + ms)
}

/// The first and the last millisecond of the years 0001 to 9999.
const MIN_ISO_MS: i64 = -62_135_596_800_000;
const MAX_ISO_MS: i64 = 253_402_300_799_999;

/// Writes a tree as compact JSON in the subset of 7.28, in document order. The parser of this file
/// reads the text back to the same tree. An Int64 and a Decimal128 use their typed object. A Date
/// from 0001 to 9999 uses the ISO text and another Date uses `$numberLong`. A UUID and an unread
/// type are not in a query tree: they are written as null.
pub fn to_query_json(v: &MongoValue) -> String {
    let mut out = String::new();
    write_value(v, &mut out);
    out
}

fn write_string(s: &str, out: &mut String) {
    // `serde_json` escapes the quote, the backslash and the control characters.
    out.push_str(&serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned()));
}

fn write_value(v: &MongoValue, out: &mut String) {
    match v {
        MongoValue::Null | MongoValue::Uuid(_) | MongoValue::Other(_) => out.push_str("null"),
        MongoValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        MongoValue::Int32(n) => out.push_str(&n.to_string()),
        MongoValue::Int64(n) => {
            out.push_str("{\"$numberLong\":\"");
            out.push_str(&n.to_string());
            out.push_str("\"}");
        }
        MongoValue::Double(f) => match serde_json::Number::from_f64(*f) {
            Some(n) => out.push_str(&n.to_string()),
            None => out.push_str("null"),
        },
        MongoValue::Decimal128(d) => {
            out.push_str("{\"$numberDecimal\":");
            write_string(d.as_str(), out);
            out.push('}');
        }
        MongoValue::String(s) => write_string(s, out),
        MongoValue::DateMs(ms) => {
            if (MIN_ISO_MS..=MAX_ISO_MS).contains(ms) {
                out.push_str("{\"$date\":\"");
                out.push_str(&crate::time::UtcMillis::from_millis(*ms).to_rfc3339());
                out.push_str("\"}");
            } else {
                out.push_str("{\"$date\":{\"$numberLong\":\"");
                out.push_str(&ms.to_string());
                out.push_str("\"}}");
            }
        }
        MongoValue::ObjectId(_) => {
            out.push_str("{\"$oid\":\"");
            out.push_str(&v.object_id_hex().unwrap_or_default());
            out.push_str("\"}");
        }
        MongoValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        MongoValue::Document(members) => {
            out.push('{');
            for (i, (k, item)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(item, out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> MongoValue {
        parse_text(text).unwrap_or_else(|e| panic!("{text}: {e}")).value
    }

    #[test]
    fn numbers_follow_j4_and_j5() {
        assert_eq!(ok("5"), MongoValue::Int32(5));
        assert_eq!(ok("-0"), MongoValue::Int32(0));
        assert_eq!(ok("2147483647"), MongoValue::Int32(i32::MAX));
        assert_eq!(ok("2147483648"), MongoValue::Int64(2_147_483_648));
        assert_eq!(ok("-2147483649"), MongoValue::Int64(-2_147_483_649));
        assert_eq!(ok("9223372036854775807"), MongoValue::Int64(i64::MAX));
        assert!(parse_text("9223372036854775808").is_err());
        assert_eq!(ok("1.0"), MongoValue::Double(1.0));
        assert_eq!(ok("1e2"), MongoValue::Double(100.0));
        assert!(parse_text("1e999").is_err());
        assert!(parse_text("01").is_err());
        assert!(parse_text("+1").is_err());
        assert!(parse_text(".5").is_err());
    }

    #[test]
    fn typed_objects() {
        assert_eq!(
            ok(r#"{"$oid":"0123456789ABCDEF01234567"}"#),
            MongoValue::object_id_from_hex("0123456789abcdef01234567").unwrap()
        );
        assert_eq!(ok(r#"{"$numberLong":"-5"}"#), MongoValue::Int64(-5));
        assert_eq!(ok(r#"{"$date":"2026-11-09T14:00:00.125Z"}"#), MongoValue::DateMs(1_794_232_800_125));
        assert_eq!(ok(r#"{"$date":{"$numberLong":"-1"}}"#), MongoValue::DateMs(-1));
        assert!(parse_text(r#"{"$date":"2026-02-30T00:00:00Z"}"#).is_err());
        assert!(parse_text(r#"{"$date":{"$numberLong":"12345678901234567"}}"#).is_err());
        assert!(parse_text(r#"{"$date":12}"#).is_err());
        assert!(parse_text(r#"{"$date":12345678901}"#).is_err());
        assert!(parse_text(r#"{"$oid":"12"}"#).is_err());
        assert!(parse_text(r#"{"$numberLong":5}"#).is_err());
        assert!(parse_text(r#"{"$numberLong":"+5"}"#).is_err());
        assert!(parse_text(r#"{"$numberDecimal":"NaN"}"#).is_err());
        assert!(parse_text(r#"{"$oid":"0123456789abcdef01234567","x":1}"#).is_err());
    }

    #[test]
    fn round_trip() {
        let t = r#"{"a":[1,{"$numberLong":"9"},1.5,{"$date":"2026-11-09T14:00:00.000Z"},{"$numberDecimal":"1.50"},"xé\n"],"b":{}}"#;
        let v = ok(t);
        assert_eq!(ok(&to_query_json(&v)), v);
        let d = MongoValue::DateMs(-62_135_596_800_001);
        assert_eq!(ok(&to_query_json(&d)), d);
    }

    #[test]
    fn spans_locate_values_and_keys() {
        let t = "{\"a\": [1,\n  {\"b\": 2}]}";
        let p = parse_text(t).unwrap();
        let path = [Seg::Key("a".into()), Seg::Index(1), Seg::Key("b".into())];
        assert_eq!(p.span.locate(&p.value, &path, true), t.find("\"b\"").unwrap());
        assert_eq!(p.span.locate(&p.value, &path, false), t.find('2').unwrap());
        assert_eq!(pointer(&path), "/a/1/b");
        assert_eq!(pointer(&[Seg::Key("a/b~".into())]), "/a~1b~0");
    }
}
