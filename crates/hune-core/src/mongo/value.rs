// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Hune value model of MongoDB (8b.16) and the typed leaves of the query tree (7.28).
//!
//! The driver module converts each BSON value to [`MongoValue`], one document at a time. This
//! crate has the type and needs no MongoDB crate. A document keeps its keys in the order of the
//! server. The model has a constructor from Extended JSON v2 text ([`MongoValue::from_extended_json`]).
//! The golden vectors of 16.9b and the fake client of the unit tests use it.

use std::collections::HashSet;
use std::fmt;

use crate::mongo::limits::MAX_QUERY_DEPTH;
use crate::time::UtcMillis;

/// A document: the keys in order. Two members can never have the same key.
pub type Document = Vec<(String, MongoValue)>;

/// The text of a Decimal128 value.
///
/// Hune never does arithmetic on it. A value from the server can be `NaN` or `Infinity`. A value
/// that an author writes in a query follows the rule of 7.28 ([`Decimal128Text::parse_query`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decimal128Text(String);

impl Decimal128Text {
    /// Keeps the text of a value that the server sent. The text is not empty.
    pub fn from_server(text: impl Into<String>) -> Option<Decimal128Text> {
        let text = text.into();
        if text.is_empty() { None } else { Some(Decimal128Text(text)) }
    }

    /// Reads the text of a Decimal128 of a query (7.28): `^-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?$`
    /// with at most 34 digits in the coefficient. `NaN` and `Infinity` give `None`.
    pub fn parse_query(text: &str) -> Option<Decimal128Text> {
        let body = text.strip_prefix('-').unwrap_or(text);
        let (mantissa, exponent) = match body.find(['e', 'E']) {
            Some(i) => (&body[..i], Some(&body[i + 1..])),
            None => (body, None),
        };
        let (int, frac) = match mantissa.split_once('.') {
            Some((i, f)) => (i, Some(f)),
            None => (mantissa, None),
        };
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(int) || frac.is_some_and(|f| !digits(f)) {
            return None;
        }
        if let Some(e) = exponent {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            if !digits(e) {
                return None;
            }
        }
        if int.len() + frac.map_or(0, str::len) > 34 {
            return None;
        }
        Some(Decimal128Text(text.to_owned()))
    }

    /// Gives the text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Decimal128Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A BSON type that the model does not read. The value is lost: only the type stays (8b.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OtherBsonType {
    /// A binary value with a subtype other than 4.
    Binary,
    /// A regular expression.
    Regex,
    /// A BSON timestamp.
    Timestamp,
    /// JavaScript code, with or without a scope.
    Code,
    /// A symbol.
    Symbol,
    /// The lowest value of the order.
    MinKey,
    /// The highest value of the order.
    MaxKey,
    /// A deprecated database pointer.
    DbPointer,
    /// The deprecated type `undefined`.
    Undefined,
}

impl OtherBsonType {
    /// Gives the BSON alias of the type.
    pub fn as_str(self) -> &'static str {
        match self {
            OtherBsonType::Binary => "binData",
            OtherBsonType::Regex => "regex",
            OtherBsonType::Timestamp => "timestamp",
            OtherBsonType::Code => "javascript",
            OtherBsonType::Symbol => "symbol",
            OtherBsonType::MinKey => "minKey",
            OtherBsonType::MaxKey => "maxKey",
            OtherBsonType::DbPointer => "dbPointer",
            OtherBsonType::Undefined => "undefined",
        }
    }
}

/// A value of the Hune model (8b.16).
#[derive(Debug, Clone, PartialEq)]
pub enum MongoValue {
    /// BSON null.
    Null,
    /// A boolean.
    Bool(bool),
    /// A 32-bit integer.
    Int32(i32),
    /// A 64-bit integer.
    Int64(i64),
    /// A double.
    Double(f64),
    /// A Decimal128, as text.
    Decimal128(Decimal128Text),
    /// A string.
    String(String),
    /// A date, in milliseconds since the Unix epoch.
    DateMs(i64),
    /// An ObjectId: 12 bytes.
    ObjectId([u8; 12]),
    /// A binary value of subtype 4.
    Uuid([u8; 16]),
    /// An array.
    Array(Vec<MongoValue>),
    /// A document with ordered keys.
    Document(Document),
    /// A type that the model does not read.
    Other(OtherBsonType),
}

impl MongoValue {
    /// Gives the BSON type alias of the value, for the frame type of 7.16c and for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            MongoValue::Null => "null",
            MongoValue::Bool(_) => "bool",
            MongoValue::Int32(_) => "int",
            MongoValue::Int64(_) => "long",
            MongoValue::Double(_) => "double",
            MongoValue::Decimal128(_) => "decimal",
            MongoValue::String(_) => "string",
            MongoValue::DateMs(_) => "date",
            MongoValue::ObjectId(_) => "objectId",
            MongoValue::Uuid(_) => "uuid",
            MongoValue::Array(_) => "array",
            MongoValue::Document(_) => "object",
            MongoValue::Other(t) => t.as_str(),
        }
    }

    /// Makes an ObjectId from 24 hexadecimal digits, in any case. Another text gives `None`.
    pub fn object_id_from_hex(hex: &str) -> Option<MongoValue> {
        let b = hex.as_bytes();
        if b.len() != 24 {
            return None;
        }
        let mut out = [0u8; 12];
        for (i, pair) in b.chunks(2).enumerate() {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            out[i] = (hi * 16 + lo) as u8;
        }
        Some(MongoValue::ObjectId(out))
    }

    /// Gives the 24 lower-case hexadecimal digits of an ObjectId, or `None` for another type.
    pub fn object_id_hex(&self) -> Option<String> {
        match self {
            MongoValue::ObjectId(b) => Some(b.iter().map(|x| format!("{x:02x}")).collect()),
            _ => None,
        }
    }

    /// Gives the members of a document, or `None` for another type.
    pub fn as_document(&self) -> Option<&Document> {
        match self {
            MongoValue::Document(d) => Some(d),
            _ => None,
        }
    }

    /// Gives the first member of a document with this key, or `None`.
    pub fn get(&self, key: &str) -> Option<&MongoValue> {
        self.as_document()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Gives the number of values of the tree, the root included. An array or a document counts
    /// one for itself and one for each item (7.28, `MAX_QUERY_NODES`). A key is not a value.
    pub fn node_count(&self) -> usize {
        let mut count = 0usize;
        let mut stack = vec![self];
        while let Some(v) = stack.pop() {
            count += 1;
            match v {
                MongoValue::Array(items) => stack.extend(items.iter()),
                MongoValue::Document(members) => stack.extend(members.iter().map(|(_, v)| v)),
                _ => {}
            }
        }
        count
    }

    /// Gives the depth of objects and arrays (7.28, `MAX_QUERY_DEPTH`). A scalar has depth 0 and
    /// the top object has depth 1.
    pub fn depth(&self) -> usize {
        let mut max = 0usize;
        let mut stack = vec![(self, 0usize)];
        while let Some((v, d)) = stack.pop() {
            match v {
                MongoValue::Array(items) => {
                    max = max.max(d + 1);
                    stack.extend(items.iter().map(|i| (i, d + 1)));
                }
                MongoValue::Document(members) => {
                    max = max.max(d + 1);
                    stack.extend(members.iter().map(|(_, i)| (i, d + 1)));
                }
                _ => {}
            }
        }
        max
    }

    /// Tells if the value uses only the eleven types of the query tree (7.28): null, boolean,
    /// Int32, Int64, Double, Decimal128, string, Date, ObjectId, array and object.
    pub fn is_query_value(&self) -> bool {
        let mut stack = vec![self];
        while let Some(v) = stack.pop() {
            match v {
                MongoValue::Uuid(_) | MongoValue::Other(_) => return false,
                MongoValue::Array(items) => stack.extend(items.iter()),
                MongoValue::Document(members) => stack.extend(members.iter().map(|(_, v)| v)),
                _ => {}
            }
        }
        true
    }

    /// Reads one value from Extended JSON v2 text, in the canonical or the relaxed form.
    ///
    /// An object keeps its key order. A duplicate key is an error. The depth is at most 32. A plain
    /// integer is an Int32 when it fits, else an Int64. The forms `$oid`, `$date`, `$numberInt`,
    /// `$numberLong`, `$numberDouble`, `$numberDecimal` and `$uuid` give the matching type. The
    /// forms `$binary`, `$regularExpression`, `$timestamp`, `$code`, `$symbol`, `$minKey`,
    /// `$maxKey`, `$undefined` and `$dbPointer` give [`MongoValue::Other`].
    ///
    /// # Errors
    ///
    /// The text is not valid JSON, or a typed form is wrong.
    pub fn from_extended_json(text: &str) -> Result<MongoValue, EjsonError> {
        let mut p = Reader { b: text.as_bytes(), pos: 0 };
        p.skip_blank();
        let v = p.value(0)?;
        p.skip_blank();
        if p.pos != p.b.len() {
            return Err(p.err("a value after the first one"));
        }
        Ok(v)
    }

    /// Reads one document from Extended JSON v2 text.
    ///
    /// # Errors
    ///
    /// The text is not valid, or its value is not an object.
    pub fn document_from_extended_json(text: &str) -> Result<Document, EjsonError> {
        match MongoValue::from_extended_json(text)? {
            MongoValue::Document(d) => Ok(d),
            other => Err(EjsonError { offset: 0, message: format!("expected an object; found {}", other.type_name()) }),
        }
    }
}

/// A fault of the Extended JSON reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EjsonError {
    /// The byte offset in the text.
    pub offset: usize,
    /// The English message.
    pub message: String,
}

impl fmt::Display for EjsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for EjsonError {}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn err(&self, message: impl Into<String>) -> EjsonError {
        EjsonError { offset: self.pos, message: message.into() }
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

    fn word(&mut self, w: &str, v: MongoValue) -> Result<MongoValue, EjsonError> {
        if self.b[self.pos..].starts_with(w.as_bytes()) {
            self.pos += w.len();
            Ok(v)
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<MongoValue, EjsonError> {
        match self.b.get(self.pos) {
            None => Err(self.err("unexpected end of text")),
            Some(b'n') => self.word("null", MongoValue::Null),
            Some(b't') => self.word("true", MongoValue::Bool(true)),
            Some(b'f') => self.word("false", MongoValue::Bool(false)),
            Some(b'"') => Ok(MongoValue::String(self.string()?)),
            Some(b'[') => self.array(depth + 1),
            Some(b'{') => self.object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<MongoValue, EjsonError> {
        if depth > MAX_QUERY_DEPTH {
            return Err(self.err("the depth is above 32"));
        }
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_blank();
        if self.eat(b']') {
            return Ok(MongoValue::Array(items));
        }
        loop {
            self.skip_blank();
            items.push(self.value(depth)?);
            self.skip_blank();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(MongoValue::Array(items));
            }
            return Err(self.err("expected `,` or `]`"));
        }
    }

    fn object(&mut self, depth: usize) -> Result<MongoValue, EjsonError> {
        if depth > MAX_QUERY_DEPTH {
            return Err(self.err("the depth is above 32"));
        }
        let start = self.pos;
        self.pos += 1;
        let mut members: Document = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.skip_blank();
        if self.eat(b'}') {
            return Ok(MongoValue::Document(members));
        }
        loop {
            self.skip_blank();
            if self.b.get(self.pos) != Some(&b'"') {
                return Err(self.err("expected a key"));
            }
            let key = self.string()?;
            if !seen.insert(key.clone()) {
                return Err(self.err(format!("duplicate key `{key}`")));
            }
            self.skip_blank();
            if !self.eat(b':') {
                return Err(self.err("expected `:`"));
            }
            self.skip_blank();
            let v = self.value(depth)?;
            members.push((key, v));
            self.skip_blank();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                break;
            }
            return Err(self.err("expected `,` or `}`"));
        }
        typed(members).map_err(|message| EjsonError { offset: start, message })
    }

    fn number(&mut self) -> Result<MongoValue, EjsonError> {
        let start = self.pos;
        self.eat(b'-');
        let int_start = self.pos;
        while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
        let int = &self.b[int_start..self.pos];
        if int.is_empty() || (int.len() > 1 && int[0] == b'0') {
            return Err(self.err("invalid number"));
        }
        let mut float = false;
        if self.eat(b'.') {
            float = true;
            let s = self.pos;
            while self.b.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if s == self.pos {
                return Err(self.err("invalid number"));
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
                return Err(self.err("invalid number"));
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.pos]).map_err(|_| self.err("invalid number"))?;
        if float {
            return match text.parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(MongoValue::Double(f)),
                _ => Err(self.err("the number is not finite")),
            };
        }
        match text.parse::<i64>() {
            Ok(n) => Ok(i32::try_from(n).map_or(MongoValue::Int64(n), MongoValue::Int32)),
            Err(_) => Err(self.err("the integer is above the range of Int64")),
        }
    }

    fn hex4(&mut self) -> Result<u32, EjsonError> {
        let s = self.b.get(self.pos..self.pos + 4).ok_or_else(|| self.err("invalid escape"))?;
        let mut v = 0u32;
        for &c in s {
            v = v * 16 + (c as char).to_digit(16).ok_or_else(|| self.err("invalid escape"))?;
        }
        self.pos += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, EjsonError> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.pos) else {
                return Err(self.err("the string has no end"));
            };
            self.pos += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.b.get(self.pos) else {
                        return Err(self.err("invalid escape"));
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
                                    return Err(self.err("invalid surrogate pair"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("invalid surrogate pair"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(cp).ok_or_else(|| self.err("invalid escape"))?
                        }
                        _ => return Err(self.err("invalid escape")),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                0x00..=0x1f => return Err(self.err("a control character in a string")),
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("the text is not UTF-8"))
    }
}

/// Reads an unsigned decimal of a typed form.
fn digits_i64(s: &str) -> Result<i64, String> {
    let body = s.strip_prefix('-').unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("`{s}` is not a whole number"));
    }
    s.parse::<i64>().map_err(|_| format!("`{s}` is out of range"))
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn uuid_from_text(s: &str) -> Option<[u8; 16]> {
    let b = s.as_bytes();
    if b.len() != 36 || [8, 13, 18, 23].iter().any(|&i| b[i] != b'-') {
        return None;
    }
    let mut out = [0u8; 16];
    let (mut n, mut i) = (0, 0);
    while i < 36 {
        if matches!(i, 8 | 13 | 18 | 23) {
            i += 1;
            continue;
        }
        out[n] = ((b[i] as char).to_digit(16)? * 16 + (b[i + 1] as char).to_digit(16)?) as u8;
        n += 1;
        i += 2;
    }
    Some(out)
}

/// Date text: `YYYY-MM-DDTHH:MM:SS` with 0 to 3 decimals and the suffix `Z`.
fn date_from_text(s: &str) -> Option<i64> {
    let body = s.strip_suffix('Z')?;
    let (head, frac) = match body.split_once('.') {
        Some((h, f)) if (1..=3).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()) => (h, f),
        Some(_) => return None,
        None => (body, ""),
    };
    let full = format!("{head}.{frac:0<3}Z");
    UtcMillis::parse_rfc3339(&full).map(|t| t.as_millis())
}

/// Turns an object into a typed value when its keys are one of the typed forms.
fn typed(members: Document) -> Result<MongoValue, String> {
    let Some((first, _)) = members.first() else {
        return Ok(MongoValue::Document(members));
    };
    if !first.starts_with('$') {
        return Ok(MongoValue::Document(members));
    }
    let single = members.len() == 1;
    let name = first.clone();
    let one = |what: &str| -> Result<(), String> {
        if single { Ok(()) } else { Err(format!("the form `{what}` has other members")) }
    };
    let only_text = |v: &MongoValue, what: &str| -> Result<String, String> {
        match v {
            MongoValue::String(s) => Ok(s.clone()),
            other => Err(format!("`{what}` needs a string; found {}", other.type_name())),
        }
    };
    let value = &members[0].1;
    match name.as_str() {
        "$oid" => {
            one("$oid")?;
            MongoValue::object_id_from_hex(&only_text(value, "$oid")?)
                .ok_or_else(|| "`$oid` needs 24 hexadecimal digits".to_owned())
        }
        "$numberInt" => {
            one("$numberInt")?;
            let n = digits_i64(&only_text(value, "$numberInt")?)?;
            i32::try_from(n).map(MongoValue::Int32).map_err(|_| "`$numberInt` is out of range".to_owned())
        }
        "$numberLong" => {
            one("$numberLong")?;
            digits_i64(&only_text(value, "$numberLong")?).map(MongoValue::Int64)
        }
        "$numberDouble" => {
            one("$numberDouble")?;
            let t = only_text(value, "$numberDouble")?;
            match t.as_str() {
                "NaN" => Ok(MongoValue::Double(f64::NAN)),
                "Infinity" => Ok(MongoValue::Double(f64::INFINITY)),
                "-Infinity" => Ok(MongoValue::Double(f64::NEG_INFINITY)),
                _ => t.parse::<f64>().map(MongoValue::Double).map_err(|_| "`$numberDouble` is not a number".to_owned()),
            }
        }
        "$numberDecimal" => {
            one("$numberDecimal")?;
            Decimal128Text::from_server(only_text(value, "$numberDecimal")?)
                .map(MongoValue::Decimal128)
                .ok_or_else(|| "`$numberDecimal` is empty".to_owned())
        }
        "$date" => {
            one("$date")?;
            match value {
                MongoValue::String(s) => {
                    date_from_text(s).map(MongoValue::DateMs).ok_or_else(|| "`$date` has a wrong date text".to_owned())
                }
                MongoValue::Int64(n) => Ok(MongoValue::DateMs(*n)),
                MongoValue::Int32(n) => Ok(MongoValue::DateMs(i64::from(*n))),
                other => Err(format!("`$date` needs a string or `$numberLong`; found {}", other.type_name())),
            }
        }
        "$uuid" => {
            one("$uuid")?;
            uuid_from_text(&only_text(value, "$uuid")?)
                .map(MongoValue::Uuid)
                .ok_or_else(|| "`$uuid` needs the text form of a UUID".to_owned())
        }
        "$binary" => {
            one("$binary")?;
            let (Some(MongoValue::String(data)), Some(MongoValue::String(sub))) =
                (value.get("base64"), value.get("subType"))
            else {
                return Err("`$binary` needs `base64` and `subType`".to_owned());
            };
            let bytes = base64(data).ok_or_else(|| "`$binary` has a wrong base64 text".to_owned())?;
            if sub.trim_start_matches('0') == "4" {
                let arr: [u8; 16] = bytes.try_into().map_err(|_| "`$binary` of subtype 4 needs 16 bytes".to_owned())?;
                Ok(MongoValue::Uuid(arr))
            } else {
                Ok(MongoValue::Other(OtherBsonType::Binary))
            }
        }
        "$regularExpression" => one(&name).map(|()| MongoValue::Other(OtherBsonType::Regex)),
        "$timestamp" => one(&name).map(|()| MongoValue::Other(OtherBsonType::Timestamp)),
        "$symbol" => one(&name).map(|()| MongoValue::Other(OtherBsonType::Symbol)),
        "$minKey" => one(&name).map(|()| MongoValue::Other(OtherBsonType::MinKey)),
        "$maxKey" => one(&name).map(|()| MongoValue::Other(OtherBsonType::MaxKey)),
        "$undefined" => one(&name).map(|()| MongoValue::Other(OtherBsonType::Undefined)),
        "$dbPointer" => one(&name).map(|()| MongoValue::Other(OtherBsonType::DbPointer)),
        "$code" => {
            if members.iter().all(|(k, _)| k == "$code" || k == "$scope") {
                Ok(MongoValue::Other(OtherBsonType::Code))
            } else {
                Err("the form `$code` has other members".to_owned())
            }
        }
        _ => Ok(MongoValue::Document(members)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ej(text: &str) -> MongoValue {
        MongoValue::from_extended_json(text).unwrap()
    }

    fn ej_err(text: &str) -> String {
        MongoValue::from_extended_json(text).unwrap_err().message
    }

    #[test]
    fn plain_numbers_follow_rule_j4_and_j5() {
        assert_eq!(ej("5"), MongoValue::Int32(5));
        assert_eq!(ej("-0"), MongoValue::Int32(0));
        assert_eq!(ej("2147483647"), MongoValue::Int32(i32::MAX));
        assert_eq!(ej("2147483648"), MongoValue::Int64(2_147_483_648));
        assert_eq!(ej("-9223372036854775808"), MongoValue::Int64(i64::MIN));
        assert!(ej_err("9223372036854775808").contains("Int64"));
        assert_eq!(ej("1.5"), MongoValue::Double(1.5));
        assert_eq!(ej("1e2"), MongoValue::Double(100.0));
        assert!(ej_err("1e999").contains("finite"));
    }

    #[test]
    fn typed_forms_give_the_matching_type() {
        assert_eq!(ej(r#"{"$numberLong":"9007199254740993"}"#), MongoValue::Int64(9_007_199_254_740_993));
        assert_eq!(ej(r#"{"$numberInt":"-7"}"#), MongoValue::Int32(-7));
        assert_eq!(ej(r#"{"$numberDouble":"NaN"}"#).type_name(), "double");
        assert_eq!(
            ej(r#"{"$numberDecimal":"1.50"}"#),
            MongoValue::Decimal128(Decimal128Text::from_server("1.50").unwrap())
        );
        assert_eq!(ej(r#"{"$date":"1970-01-01T00:00:01.5Z"}"#), MongoValue::DateMs(1500));
        assert_eq!(ej(r#"{"$date":"2024-02-29T12:00:00Z"}"#), MongoValue::DateMs(1_709_208_000_000));
        assert_eq!(ej(r#"{"$date":{"$numberLong":"-86400000"}}"#), MongoValue::DateMs(-86_400_000));
        assert_eq!(
            ej(r#"{"$uuid":"01020304-0506-0708-090a-0b0c0d0e0f10"}"#),
            MongoValue::Uuid([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
        );
        assert_eq!(ej(r#"{"$binary":{"base64":"AQID","subType":"00"}}"#), MongoValue::Other(OtherBsonType::Binary));
        assert_eq!(ej(r#"{"$minKey":1}"#), MongoValue::Other(OtherBsonType::MinKey));
    }

    #[test]
    fn binary_subtype_4_is_a_uuid() {
        let v = ej(r#"{"$binary":{"base64":"AQIDBAUGBwgJCgsMDQ4PEA==","subType":"04"}}"#);
        assert_eq!(v, MongoValue::Uuid([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]));
    }

    #[test]
    fn object_id_keeps_12_bytes_and_no_case() {
        let v = ej(r#"{"$oid":"507F1F77BCF86CD799439011"}"#);
        assert_eq!(v.object_id_hex().as_deref(), Some("507f1f77bcf86cd799439011"));
        let MongoValue::ObjectId(b) = v else { panic!("not an ObjectId") };
        assert_eq!(b[0], 0x50);
        assert_eq!(b[11], 0x11);
        assert!(MongoValue::object_id_from_hex("507f1f77bcf86cd79943901").is_none());
        assert!(MongoValue::object_id_from_hex("zz7f1f77bcf86cd799439011").is_none());
        assert!(ej_err(r#"{"$oid":"12"}"#).contains("24"));
    }

    #[test]
    fn wrong_typed_forms_are_errors() {
        assert!(ej_err(r#"{"$numberLong":5}"#).contains("string"));
        assert!(ej_err(r#"{"$numberLong":"1.5"}"#).contains("whole"));
        assert!(ej_err(r#"{"$date":"2024-13-01T00:00:00Z"}"#).contains("$date"));
        assert!(ej_err(r#"{"$date":"2024-01-01T00:00:00.1234Z"}"#).contains("$date"));
        assert!(ej_err(r#"{"$oid":"507f1f77bcf86cd799439011","x":1}"#).contains("other members"));
    }

    #[test]
    fn document_keeps_the_order_of_the_keys() {
        let d = MongoValue::document_from_extended_json(r#"{"z":1,"a":{"y":[1,2],"b":null},"m":"x"}"#).unwrap();
        let keys: Vec<&str> = d.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["z", "a", "m"]);
        let a = &d[1].1;
        assert_eq!(a.get("y"), Some(&MongoValue::Array(vec![MongoValue::Int32(1), MongoValue::Int32(2)])));
        assert_eq!(a.get("b"), Some(&MongoValue::Null));
        assert_eq!(a.get("none"), None);
    }

    #[test]
    fn reader_refuses_bad_text() {
        assert!(ej_err(r#"{"a":1,"a":2}"#).contains("duplicate"));
        assert!(ej_err("[1,]").contains("unexpected"));
        assert!(ej_err("{} {}").contains("after the first"));
        assert!(ej_err(r#"{"a":1,}"#).contains("key"));
        assert!(ej_err("01").contains("invalid number"));
        assert!(ej_err("\"a\u{1}\"").contains("control"));
        assert!(ej_err("").contains("end"));
        assert!(MongoValue::document_from_extended_json("[1]").is_err());
    }

    #[test]
    fn strings_decode_escapes_and_pairs() {
        assert_eq!(ej(r#""a\né😀""#), MongoValue::String("a\n\u{e9}\u{1F600}".to_owned()));
        assert!(ej_err(r#""\ud83d""#).contains("surrogate"));
        assert!(ej_err(r#""\q""#).contains("escape"));
    }

    #[test]
    fn depth_is_limited_to_32() {
        let ok = format!("{}1{}", "[".repeat(32), "]".repeat(32));
        assert_eq!(ej(&ok).depth(), 32);
        let deep = format!("{}1{}", "[".repeat(33), "]".repeat(33));
        assert!(ej_err(&deep).contains("depth"));
    }

    #[test]
    fn counts_and_depth_of_a_tree() {
        let v = ej(r#"{"a":[1,2,{"b":3}],"c":"x"}"#);
        // root, array, 1, 2, object, 3, "x"
        assert_eq!(v.node_count(), 7);
        assert_eq!(v.depth(), 3);
        assert_eq!(MongoValue::Int32(1).depth(), 0);
        assert_eq!(MongoValue::Int32(1).node_count(), 1);
    }

    #[test]
    fn query_values_exclude_uuid_and_other() {
        assert!(ej(r#"{"a":[{"$oid":"507f1f77bcf86cd799439011"},{"$date":"1970-01-01T00:00:00Z"}]}"#).is_query_value());
        assert!(!ej(r#"{"a":[{"$minKey":1}]}"#).is_query_value());
        assert!(!MongoValue::Uuid([0; 16]).is_query_value());
    }

    #[test]
    fn type_names_are_bson_aliases() {
        let names: Vec<&str> = [
            MongoValue::Null,
            MongoValue::Bool(true),
            MongoValue::Int32(1),
            MongoValue::Int64(1),
            MongoValue::Double(1.0),
            MongoValue::String(String::new()),
            MongoValue::DateMs(0),
            MongoValue::ObjectId([0; 12]),
            MongoValue::Array(vec![]),
            MongoValue::Document(vec![]),
        ]
        .iter()
        .map(MongoValue::type_name)
        .collect();
        assert_eq!(names, ["null", "bool", "int", "long", "double", "string", "date", "objectId", "array", "object"]);
    }

    #[test]
    fn decimal_text_of_a_query_follows_7_28() {
        for ok in ["0", "-1", "1.5", "1.5e10", "-0.1E-3", "12345678901234567890123456789012.34"] {
            assert!(Decimal128Text::parse_query(ok).is_some(), "{ok}");
        }
        let too_long = "1".repeat(35);
        for bad in ["", "-", "NaN", "Infinity", ".5", "5.", "1e", "1e+", "+1", "1 ", too_long.as_str()] {
            assert!(Decimal128Text::parse_query(bad).is_none(), "{bad}");
        }
        assert!(Decimal128Text::from_server("NaN").is_some());
        assert!(Decimal128Text::from_server("").is_none());
    }
}
