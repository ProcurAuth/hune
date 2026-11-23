// SPDX-License-Identifier: MIT OR Apache-2.0
//! The frame of a MongoDB result (7.16c, 7.16d, 8b.6).
//!
//! The result of a query is a stream of documents. The server needs all the documents to know the
//! type of each column. A [`MongoFrameBuilder`] therefore takes the documents one at a time, keeps
//! each value in a compact form, counts the rows and the bytes, and makes the frame in
//! [`MongoFrameBuilder::finish`].
//!
//! # The rules of 7.16c
//!
//! - The columns are the top-level fields of the documents, in the order of their first
//!   appearance. A document that lacks a field gives `null` for it.
//! - A nested document or array is one text: the compact Relaxed Extended JSON v2 text of the
//!   value ([`write_relaxed_json`]).
//! - A column that holds more than one BSON type follows the table of 7.16c ([`resolve`]). Int32
//!   with Int64 gives `long`. Int32, Int64 and Double give `double`, when no Int64 value has an
//!   absolute value above 2^53. A Decimal128 with an integer gives `decimal`. A Document with an
//!   Array gives `json`. Each other mix is a [`Fault::Query`].
//! - A value of a type that the frame cannot show is a [`Fault::Query`], also inside a nested value.
//! - A result with no document gives a frame with no column.
//!
//! # The count of bytes (8b.6)
//!
//! The count adds, for each value, the length in bytes of its JSON text in the frame, plus 1. A
//! field that a document lacks counts as `null`. The count uses the form that the value has when it
//! arrives. An integer that a later document widens to a text or a double can then add up to two
//! bytes that the count has not seen. The check runs after each value, so a document that brings
//! the count above the cap stops the query at the first value that does it.
//!
//! The module does no I/O. It reads no clock.

use std::collections::{HashMap, HashSet};

use crate::frame::{Frame, FrameBuilder, FrameStats, write_json_string};
use crate::mongo::value::{Document, MongoValue, OtherBsonType};

/// The most columns of a frame (7.16c).
pub const MAX_FRAME_COLUMNS: usize = 256;

/// The deepest nesting that the text of a nested value takes. The server limit is 100 levels.
const MAX_NESTING: usize = 200;

/// The longest plain text of a Decimal128, in characters. A longer value is a [`Fault::Query`].
const MAX_DECIMAL_CHARS: usize = 400;

/// The largest integer that a double holds exactly: 2^53.
const TWO_POW_53: u64 = 1 << 53;

/// The caps of one query (8b.6), from the effective limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCaps {
    /// The effective `max_result_rows`.
    pub max_rows: u64,
    /// The key `max_result_bytes`.
    pub max_bytes: u64,
}

/// The reason why a document cannot join the frame, or why the frame cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The document is above `max_result_rows`: `422 query_limit`, or `403 ceiling_exceeded`.
    Rows,
    /// The count of bytes is above `max_result_bytes`: `422 query_limit`.
    Bytes,
    /// A value does not decode, or a document has a repeated field name: `502 source_bad_response`.
    /// The text never holds a value of a row.
    BadResponse(String),
    /// A type that 7.16c does not allow, a mix of types, or too many columns: `422 query_error`
    /// with `mongo_code` `null`. The text names the column and the types, never a value.
    Query(String),
}

/// The type of a column (7.16c). It is the member `type` of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnType {
    /// Only `null`.
    Null,
    /// Int32.
    Int,
    /// Int64, with the Int32 values written as texts.
    Long,
    /// Double.
    Double,
    /// Decimal128.
    Decimal,
    /// String.
    String,
    /// Boolean.
    Bool,
    /// Date.
    Date,
    /// ObjectId.
    ObjectId,
    /// Binary of the subtype 4.
    Uuid,
    /// Document.
    Object,
    /// Array.
    Array,
    /// Document and Array in one column.
    Json,
}

impl ColumnType {
    /// Gives the member `type` of the column.
    pub fn name(self) -> &'static str {
        match self {
            ColumnType::Null => "null",
            ColumnType::Int => "int",
            ColumnType::Long => "long",
            ColumnType::Double => "double",
            ColumnType::Decimal => "decimal",
            ColumnType::String => "string",
            ColumnType::Bool => "bool",
            ColumnType::Date => "date",
            ColumnType::ObjectId => "objectId",
            ColumnType::Uuid => "uuid",
            ColumnType::Object => "object",
            ColumnType::Array => "array",
            ColumnType::Json => "json",
        }
    }

    /// Gives the ClickHouse type text that selects the kind and the value checks of 7.16. The
    /// builder of `hune-core` needs it. The member `type` is set to [`ColumnType::name`] after.
    fn proxy(self) -> &'static str {
        match self {
            ColumnType::Null => "Nothing",
            ColumnType::Int => "Int32",
            ColumnType::Double => "Float64",
            ColumnType::Long => "Int64",
            ColumnType::Decimal => "Decimal(76, 0)",
            ColumnType::Bool => "Bool",
            ColumnType::Date => "DateTime64(3)",
            ColumnType::String
            | ColumnType::ObjectId
            | ColumnType::Uuid
            | ColumnType::Object
            | ColumnType::Array
            | ColumnType::Json => "String",
        }
    }
}

// The bits of the BSON types that a column holds. `null` has no bit.
const T_INT: u16 = 1;
const T_LONG: u16 = 2;
const T_DOUBLE: u16 = 4;
const T_DECIMAL: u16 = 8;
const T_STRING: u16 = 16;
const T_BOOL: u16 = 32;
const T_DATE: u16 = 64;
const T_OID: u16 = 128;
const T_UUID: u16 = 256;
const T_OBJECT: u16 = 512;
const T_ARRAY: u16 = 1024;

/// The names of the bits, in the order of the messages.
const TYPE_NAMES: [(u16, &str); 11] = [
    (T_INT, "int"),
    (T_LONG, "long"),
    (T_DOUBLE, "double"),
    (T_DECIMAL, "decimal"),
    (T_STRING, "string"),
    (T_BOOL, "bool"),
    (T_DATE, "date"),
    (T_OID, "objectId"),
    (T_UUID, "uuid"),
    (T_OBJECT, "object"),
    (T_ARRAY, "array"),
];

/// Why the types of a column do not give one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixError {
    /// The set of types is not in the table of 7.16c.
    Mix,
    /// An Int64 value above 2^53 meets a Double value.
    BigLong,
}

/// Gives the type of a column from the set of the BSON types of its values (7.16c).
///
/// `types` has one bit for each BSON type that the column holds. `null` has no bit. `big_long` is
/// true when an Int64 value of the column has an absolute value above 2^53.
///
/// # Errors
///
/// The set is not in the table of 7.16c, or an Int64 above 2^53 meets a Double.
fn resolve_bits(types: u16, big_long: bool) -> Result<ColumnType, MixError> {
    match types {
        0 => return Ok(ColumnType::Null),
        T_INT => return Ok(ColumnType::Int),
        T_LONG => return Ok(ColumnType::Long),
        T_DOUBLE => return Ok(ColumnType::Double),
        T_DECIMAL => return Ok(ColumnType::Decimal),
        T_STRING => return Ok(ColumnType::String),
        T_BOOL => return Ok(ColumnType::Bool),
        T_DATE => return Ok(ColumnType::Date),
        T_OID => return Ok(ColumnType::ObjectId),
        T_UUID => return Ok(ColumnType::Uuid),
        T_OBJECT => return Ok(ColumnType::Object),
        T_ARRAY => return Ok(ColumnType::Array),
        _ => {}
    }
    if types & !(T_INT | T_LONG) == 0 {
        Ok(ColumnType::Long)
    } else if types & !(T_INT | T_LONG | T_DOUBLE) == 0 {
        if big_long { Err(MixError::BigLong) } else { Ok(ColumnType::Double) }
    } else if types & !(T_DECIMAL | T_INT | T_LONG) == 0 {
        Ok(ColumnType::Decimal)
    } else if types == T_OBJECT | T_ARRAY {
        Ok(ColumnType::Json)
    } else {
        Err(MixError::Mix)
    }
}

/// Gives the type of a column from the BSON type names of its values (7.16c): `int`, `long`,
/// `double`, `decimal`, `string`, `bool`, `date`, `objectId`, `uuid`, `object` and `array`. The
/// name `null` and each repeated name are allowed. `big_long` is true when an Int64 value has an
/// absolute value above 2^53.
///
/// # Errors
///
/// A name that the table does not know, a set that the table does not allow, or an Int64 above
/// 2^53 with a Double.
pub fn resolve(types: &[&str], big_long: bool) -> Result<ColumnType, MixError> {
    let mut bits = 0u16;
    for t in types {
        if *t == "null" {
            continue;
        }
        let Some((bit, _)) = TYPE_NAMES.iter().find(|(_, n)| n == t) else {
            return Err(MixError::Mix);
        };
        bits |= bit;
    }
    resolve_bits(bits, big_long)
}

/// The text of a set of types, for a message.
fn names_of(types: u16) -> String {
    let mut out = String::new();
    for (bit, name) in TYPE_NAMES {
        if types & bit != 0 {
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(name);
        }
    }
    out
}

/// Cuts a column name for a message.
fn short(name: &str) -> &str {
    const MAX: usize = 100;
    if name.len() <= MAX {
        return name;
    }
    let mut end = MAX;
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

fn mix_fault(name: &str, types: u16, error: MixError) -> Fault {
    let advice = "Cast the value in the pipeline, for example with $toString, $toDouble or $toDate.";
    Fault::Query(match error {
        MixError::Mix => format!(
            "The column \"{}\" holds values of these types, and the frame cannot mix them: {}. {advice}",
            short(name),
            names_of(types)
        ),
        MixError::BigLong => format!(
            "The column \"{}\" holds long and double values, and a long value is above 2^53. The frame cannot mix them. {advice}",
            short(name)
        ),
    })
}

fn unsupported_fault(name: &str, t: OtherBsonType) -> Fault {
    Fault::Query(format!(
        "The field \"{}\" holds a value of the BSON type {}, and the frame cannot show it. Cast the value in the pipeline, for example with $toString, $toDouble or $toDate.",
        short(name),
        t.as_str()
    ))
}

// ---------------------------------------------------------------------------------------------
// The text of a number, a date and a nested value.
// ---------------------------------------------------------------------------------------------

/// Gives the number of characters of an integer in decimal, with the sign.
fn int_len(n: i64) -> usize {
    let mut v = n.unsigned_abs();
    let mut len = usize::from(n < 0);
    loop {
        len += 1;
        v /= 10;
        if v == 0 {
            return len;
        }
    }
}

/// Writes a finite double as `serde_json` does. A double that is not finite is the text `nan`,
/// `inf` or `-inf` in a JSON string (7.16c).
fn write_double(out: &mut String, x: f64) {
    if x.is_nan() {
        out.push_str("\"nan\"");
    } else if x.is_infinite() {
        out.push_str(if x > 0.0 { "\"inf\"" } else { "\"-inf\"" });
    } else {
        out.push_str(ryu::Buffer::new().format_finite(x));
    }
}

fn double_len(x: f64) -> usize {
    if x.is_nan() || x == f64::INFINITY {
        5
    } else if x.is_infinite() {
        6
    } else {
        ryu::Buffer::new().format_finite(x).len()
    }
}

/// Gives the Unix seconds of a Date value, with 3 decimals at most and no trailing zero. The
/// Date 1794211200125 gives `1794211200.125`.
pub fn date_seconds_text(ms: i64) -> String {
    let abs = ms.unsigned_abs();
    let (secs, frac) = (abs / 1000, abs % 1000);
    let sign = if ms < 0 { "-" } else { "" };
    if frac == 0 {
        format!("{sign}{secs}")
    } else {
        let digits = format!("{frac:03}");
        format!("{sign}{secs}.{}", digits.trim_end_matches('0'))
    }
}

/// Gives the length in bytes of the JSON string of `s`, with its quotes. The escapes are those of
/// [`write_json_string`].
fn json_string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}

/// What the text of a nested value cannot show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaxedError {
    /// The value holds a BSON type that the model does not read.
    Unsupported(OtherBsonType),
    /// The nesting is deeper than 200 levels.
    TooDeep,
    /// A document holds a repeated field name.
    DuplicateKey,
}

/// Writes the compact Relaxed Extended JSON v2 text of a value, with the keys in document order.
///
/// An integer and a finite double are plain JSON numbers. A double that is not finite is
/// `{"$numberDouble":"NaN"}`, `Infinity` or `-Infinity`. A Decimal128 is `{"$numberDecimal":"..."}`,
/// an ObjectId is `{"$oid":"..."}`, a UUID is `{"$uuid":"..."}`, and a Date is
/// `{"$date":"YYYY-MM-DDTHH:MM:SS.mmmZ"}` for the years 1970 to 9999 and
/// `{"$date":{"$numberLong":"..."}}` outside them.
///
/// # Errors
///
/// The value holds a type that the model does not read, the nesting is too deep, or a document
/// repeats a field name.
pub fn write_relaxed_json(out: &mut String, value: &MongoValue) -> Result<(), RelaxedError> {
    write_relaxed(out, value, 0)
}

fn write_relaxed(out: &mut String, value: &MongoValue, depth: usize) -> Result<(), RelaxedError> {
    if depth > MAX_NESTING {
        return Err(RelaxedError::TooDeep);
    }
    match value {
        MongoValue::Null => out.push_str("null"),
        MongoValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        MongoValue::Int32(n) => out.push_str(&n.to_string()),
        MongoValue::Int64(n) => out.push_str(&n.to_string()),
        MongoValue::Double(x) => {
            if x.is_finite() {
                out.push_str(ryu::Buffer::new().format_finite(*x));
            } else {
                let text = if x.is_nan() {
                    "NaN"
                } else if *x > 0.0 {
                    "Infinity"
                } else {
                    "-Infinity"
                };
                out.push_str("{\"$numberDouble\":\"");
                out.push_str(text);
                out.push_str("\"}");
            }
        }
        MongoValue::Decimal128(d) => {
            out.push_str("{\"$numberDecimal\":");
            write_json_string(out, d.as_str());
            out.push('}');
        }
        MongoValue::String(s) => write_json_string(out, s),
        MongoValue::DateMs(ms) => {
            out.push_str("{\"$date\":");
            if (0..253_402_300_800_000).contains(ms) {
                out.push('"');
                out.push_str(&crate::time::UtcMillis::from_millis(*ms).to_rfc3339());
                out.push('"');
            } else {
                out.push_str("{\"$numberLong\":\"");
                out.push_str(&ms.to_string());
                out.push_str("\"}");
            }
            out.push('}');
        }
        MongoValue::ObjectId(b) => {
            out.push_str("{\"$oid\":\"");
            push_hex(out, b);
            out.push_str("\"}");
        }
        MongoValue::Uuid(b) => {
            out.push_str("{\"$uuid\":\"");
            push_uuid(out, b);
            out.push_str("\"}");
        }
        MongoValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_relaxed(out, item, depth + 1)?;
            }
            out.push(']');
        }
        MongoValue::Document(members) => {
            check_keys(members)?;
            out.push('{');
            for (i, (key, item)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_string(out, key);
                out.push(':');
                write_relaxed(out, item, depth + 1)?;
            }
            out.push('}');
        }
        MongoValue::Other(t) => return Err(RelaxedError::Unsupported(*t)),
    }
    Ok(())
}

/// Refuses a document that repeats a key.
fn check_keys(members: &Document) -> Result<(), RelaxedError> {
    if members.len() <= 16 {
        for (i, (k, _)) in members.iter().enumerate() {
            if members[..i].iter().any(|(p, _)| p == k) {
                return Err(RelaxedError::DuplicateKey);
            }
        }
    } else {
        let mut seen = HashSet::with_capacity(members.len());
        if members.iter().any(|(k, _)| !seen.insert(k.as_str())) {
            return Err(RelaxedError::DuplicateKey);
        }
    }
    Ok(())
}

fn push_hex(out: &mut String, bytes: &[u8]) {
    for b in bytes {
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 15), 16).unwrap_or('0'));
    }
}

fn push_uuid(out: &mut String, b: &[u8; 16]) {
    for (i, byte) in b.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        push_hex(out, std::slice::from_ref(byte));
    }
}

fn relaxed_fault(name: &str, e: RelaxedError) -> Fault {
    match e {
        RelaxedError::Unsupported(t) => unsupported_fault(name, t),
        RelaxedError::TooDeep => Fault::BadResponse("A nested value is deeper than 200 levels.".to_owned()),
        RelaxedError::DuplicateKey => Fault::BadResponse("A document has a repeated field name.".to_owned()),
    }
}

/// What a Decimal128 text gives for the frame.
enum Decimal {
    /// The plain decimal text, with the scale of the value.
    Plain(String),
    /// `NaN`, `Infinity` or `-Infinity`: the frame value is `null`.
    NotFinite,
}

/// Reads the text that the server prints for a Decimal128. A value with an exponent becomes a plain
/// decimal text with the same digits and the same scale: `1.20E+3` gives `1200`, and `5E-7` gives
/// `0.0000005`.
///
/// # Errors
///
/// The text is not a Decimal128 text, or its plain form is longer than 400 characters (`Err(true)`
/// for the second case).
fn plain_decimal(text: &str) -> Result<Decimal, bool> {
    let lower = text.to_ascii_lowercase();
    let unsigned = lower.strip_prefix(['-', '+']).unwrap_or(&lower);
    if matches!(unsigned, "nan" | "inf" | "infinity" | "snan") {
        return Ok(Decimal::NotFinite);
    }
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !all_digits(int) || !all_digits(frac) {
        return Err(false);
    }
    let exp: i64 = match exponent {
        None => 0,
        Some(e) => {
            let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
            if digits.is_empty() || !all_digits(digits) || digits.len() > 6 {
                return Err(false);
            }
            let n: i64 = digits.parse().map_err(|_| false)?;
            if e.starts_with('-') { -n } else { n }
        }
    };
    // The number is `digits` times 10 to the power `exp - frac.len()`.
    let digits = format!("{int}{frac}");
    let scale = frac.len() as i64 - exp;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if scale <= 0 {
        let zeros = usize::try_from(-scale).unwrap_or(usize::MAX);
        let trimmed = digits.trim_start_matches('0');
        if trimmed.is_empty() {
            out.push('0');
        } else {
            if trimmed.len().saturating_add(zeros) > MAX_DECIMAL_CHARS {
                return Err(true);
            }
            out.push_str(trimmed);
            out.extend(std::iter::repeat_n('0', zeros));
        }
    } else {
        let scale = usize::try_from(scale).unwrap_or(usize::MAX);
        if scale.saturating_add(digits.len()) > MAX_DECIMAL_CHARS {
            return Err(true);
        }
        if scale >= digits.len() {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', scale - digits.len()));
            out.push_str(&digits);
        } else {
            let split = digits.len() - scale;
            let int_part = digits[..split].trim_start_matches('0');
            out.push_str(if int_part.is_empty() { "0" } else { int_part });
            out.push('.');
            out.push_str(&digits[split..]);
        }
    }
    Ok(Decimal::Plain(out))
}

// ---------------------------------------------------------------------------------------------
// The compact cells of a column.
// ---------------------------------------------------------------------------------------------

const C_NULL: u8 = 0;
const C_I32: u8 = 1;
const C_I64: u8 = 2;
const C_F64: u8 = 3;
const C_FALSE: u8 = 4;
const C_TRUE: u8 = 5;
const C_DATE: u8 = 6;
const C_STR: u8 = 7;

/// One value of a column, read back.
#[derive(Debug, Clone, Copy)]
enum Cell<'a> {
    Null,
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    Date(i64),
    Str(&'a str),
}

/// A column while the documents arrive. The cells of the rows `0..n` are in `cells` and `text`. The
/// rows from `n` on are `null`.
struct Acc {
    name: String,
    types: u16,
    big_long: bool,
    /// The row (plus 1) of the last value that a document gave. It finds a repeated field name.
    last: u64,
    /// The number of cells.
    n: u64,
    cells: Vec<u8>,
    text: String,
}

impl Acc {
    fn new(name: &str) -> Acc {
        Acc { name: name.to_owned(), types: 0, big_long: false, last: 0, n: 0, cells: Vec::new(), text: String::new() }
    }

    /// Adds `null` cells up to `rows`.
    fn pad(&mut self, rows: u64) {
        if self.n < rows {
            let missing = usize::try_from(rows - self.n).unwrap_or(usize::MAX);
            self.cells.resize(self.cells.len().saturating_add(missing), C_NULL);
            self.n = rows;
        }
    }

    fn push_str_cell(&mut self, s: &str) -> Result<(), Fault> {
        let len = u32::try_from(s.len()).map_err(|_| Fault::BadResponse("A text is larger than 4 GiB.".to_owned()))?;
        self.cells.push(C_STR);
        self.cells.extend_from_slice(&len.to_le_bytes());
        self.text.push_str(s);
        Ok(())
    }

    /// Gives an iterator over the cells of the column, with `rows` cells in all.
    fn reader(&self) -> CellReader<'_> {
        CellReader { cells: &self.cells, text: &self.text, pos: 0, tpos: 0 }
    }
}

struct CellReader<'a> {
    cells: &'a [u8],
    text: &'a str,
    pos: usize,
    tpos: usize,
}

impl<'a> CellReader<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let bytes: [u8; N] = self.cells.get(self.pos..self.pos + N)?.try_into().ok()?;
        self.pos += N;
        Some(bytes)
    }

    /// Reads the next cell. A column that ended gives `null` (the rows after its last cell).
    fn next_cell(&mut self) -> Option<Cell<'a>> {
        let tag = *self.cells.get(self.pos)?;
        self.pos += 1;
        Some(match tag {
            C_I32 => Cell::I32(i32::from_le_bytes(self.take()?)),
            C_I64 => Cell::I64(i64::from_le_bytes(self.take()?)),
            C_F64 => Cell::F64(f64::from_bits(u64::from_le_bytes(self.take()?))),
            C_FALSE => Cell::Bool(false),
            C_TRUE => Cell::Bool(true),
            C_DATE => Cell::Date(i64::from_le_bytes(self.take()?)),
            C_STR => {
                let len = u32::from_le_bytes(self.take()?) as usize;
                let s = self.text.get(self.tpos..self.tpos + len)?;
                self.tpos += len;
                Cell::Str(s)
            }
            _ => Cell::Null,
        })
    }
}

/// Writes the frame value of a cell, for a column of type `t`.
fn write_cell(out: &mut String, t: ColumnType, cell: Cell<'_>) -> Result<(), Fault> {
    let bad = || Fault::BadResponse("A cell does not fit the type of its column.".to_owned());
    match (t, cell) {
        (_, Cell::Null) | (ColumnType::Null, _) => out.push_str("null"),
        (ColumnType::Int, Cell::I32(n)) => out.push_str(&n.to_string()),
        (ColumnType::Long | ColumnType::Decimal, Cell::I32(n)) => {
            out.push('"');
            out.push_str(&n.to_string());
            out.push('"');
        }
        (ColumnType::Long | ColumnType::Decimal, Cell::I64(n)) => {
            out.push('"');
            out.push_str(&n.to_string());
            out.push('"');
        }
        (ColumnType::Double, Cell::I32(n)) => write_double(out, f64::from(n)),
        (ColumnType::Double, Cell::I64(n)) => write_double(out, n as f64),
        (ColumnType::Double, Cell::F64(x)) => write_double(out, x),
        (ColumnType::Bool, Cell::Bool(b)) => out.push_str(if b { "true" } else { "false" }),
        (ColumnType::Date, Cell::Date(ms)) => out.push_str(&date_seconds_text(ms)),
        (
            ColumnType::Decimal
            | ColumnType::String
            | ColumnType::ObjectId
            | ColumnType::Uuid
            | ColumnType::Object
            | ColumnType::Array
            | ColumnType::Json,
            Cell::Str(s),
        ) => write_json_string(out, s),
        _ => return Err(bad()),
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The builder.
// ---------------------------------------------------------------------------------------------

/// Builds the frame of one query from its documents (7.16c, 8b.6).
pub struct MongoFrameBuilder {
    caps: FrameCaps,
    index: HashMap<String, usize>,
    cols: Vec<Acc>,
    rows: u64,
    bytes: u64,
}

impl std::fmt::Debug for MongoFrameBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The debug text has counts only: a document can hold data.
        f.debug_struct("MongoFrameBuilder")
            .field("columns", &self.cols.len())
            .field("rows", &self.rows)
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl MongoFrameBuilder {
    /// Makes an empty builder.
    pub fn new(caps: FrameCaps) -> MongoFrameBuilder {
        MongoFrameBuilder { caps, index: HashMap::new(), cols: Vec::new(), rows: 0, bytes: 0 }
    }

    /// Gives the number of documents that the builder holds.
    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// Gives the count of bytes of 8b.6.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Gives the number of columns that the documents made.
    pub fn column_count(&self) -> usize {
        self.cols.len()
    }

    /// Adds one document.
    ///
    /// The row cap is exact: the document after `max_rows` documents gives [`Fault::Rows`], before it
    /// is read. The count of bytes is checked after each value. A builder that gave a fault MUST NOT
    /// be used again.
    ///
    /// # Errors
    ///
    /// A cap, a value that does not decode, a repeated field name, a type that 7.16c does not allow,
    /// a mix that its table does not allow, or more than 256 columns.
    pub fn push_document(&mut self, doc: &Document) -> Result<(), Fault> {
        if self.rows >= self.caps.max_rows {
            return Err(Fault::Rows);
        }
        let row = self.rows;
        let mut bytes = self.bytes;
        for (key, value) in doc {
            let idx = match self.index.get(key.as_str()) {
                Some(i) => *i,
                None => {
                    if self.cols.len() >= MAX_FRAME_COLUMNS {
                        return Err(Fault::Query(format!(
                            "The result has more than {MAX_FRAME_COLUMNS} columns. Reduce the fields with $project."
                        )));
                    }
                    // The rows before this one gave `null` for the new column.
                    bytes = bytes.saturating_add(row.saturating_mul(5));
                    self.cols.push(Acc::new(key));
                    self.index.insert(key.clone(), self.cols.len() - 1);
                    self.cols.len() - 1
                }
            };
            let col = &mut self.cols[idx];
            if col.last == row + 1 {
                return Err(Fault::BadResponse("A document has a repeated field name.".to_owned()));
            }
            col.last = row + 1;
            let json_len = Self::append(col, row, value)?;
            bytes = bytes.saturating_add(json_len + 1);
            if bytes > self.caps.max_bytes {
                return Err(Fault::Bytes);
            }
        }
        // A field that the document lacks gives `null`: 4 bytes and 1.
        let missing = self.cols.iter().filter(|c| c.last != row + 1).count() as u64;
        bytes = bytes.saturating_add(missing * 5);
        if bytes > self.caps.max_bytes {
            return Err(Fault::Bytes);
        }
        self.bytes = bytes;
        self.rows += 1;
        Ok(())
    }

    /// Stores one value in its column. Gives the length of its JSON text in the frame.
    fn append(col: &mut Acc, row: u64, value: &MongoValue) -> Result<u64, Fault> {
        col.pad(row);
        let (bit, json_len): (u16, u64) = match value {
            MongoValue::Null => {
                col.cells.push(C_NULL);
                (0, 4)
            }
            MongoValue::Bool(b) => {
                col.cells.push(if *b { C_TRUE } else { C_FALSE });
                (T_BOOL, if *b { 4 } else { 5 })
            }
            MongoValue::Int32(n) => {
                col.cells.push(C_I32);
                col.cells.extend_from_slice(&n.to_le_bytes());
                (T_INT, int_len(i64::from(*n)) as u64)
            }
            MongoValue::Int64(n) => {
                col.cells.push(C_I64);
                col.cells.extend_from_slice(&n.to_le_bytes());
                if n.unsigned_abs() > TWO_POW_53 {
                    col.big_long = true;
                }
                (T_LONG, int_len(*n) as u64 + 2)
            }
            MongoValue::Double(x) => {
                col.cells.push(C_F64);
                col.cells.extend_from_slice(&x.to_bits().to_le_bytes());
                (T_DOUBLE, double_len(*x) as u64)
            }
            MongoValue::Decimal128(d) => match plain_decimal(d.as_str()) {
                Ok(Decimal::Plain(text)) => {
                    col.push_str_cell(&text)?;
                    (T_DECIMAL, text.len() as u64 + 2)
                }
                Ok(Decimal::NotFinite) => {
                    col.cells.push(C_NULL);
                    (T_DECIMAL, 4)
                }
                Err(false) => return Err(Fault::BadResponse("A Decimal128 value is not valid.".to_owned())),
                Err(true) => {
                    return Err(Fault::Query(format!(
                        "A Decimal128 value of the column \"{}\" has more than {MAX_DECIMAL_CHARS} digits in plain form. Cast the value in the pipeline, for example with $toString or $toDouble.",
                        short(&col.name)
                    )));
                }
            },
            MongoValue::String(s) => {
                col.push_str_cell(s)?;
                (T_STRING, json_string_len(s) as u64)
            }
            MongoValue::DateMs(ms) => {
                col.cells.push(C_DATE);
                col.cells.extend_from_slice(&ms.to_le_bytes());
                (T_DATE, date_seconds_text(*ms).len() as u64)
            }
            MongoValue::ObjectId(b) => {
                let mut hex = String::with_capacity(24);
                push_hex(&mut hex, b);
                col.push_str_cell(&hex)?;
                (T_OID, 26)
            }
            MongoValue::Uuid(b) => {
                let mut text = String::with_capacity(36);
                push_uuid(&mut text, b);
                col.push_str_cell(&text)?;
                (T_UUID, 38)
            }
            MongoValue::Document(_) | MongoValue::Array(_) => {
                let mut text = String::new();
                write_relaxed_json(&mut text, value).map_err(|e| relaxed_fault(&col.name, e))?;
                col.push_str_cell(&text)?;
                let bit = if matches!(value, MongoValue::Document(_)) { T_OBJECT } else { T_ARRAY };
                (bit, json_string_len(&text) as u64)
            }
            MongoValue::Other(t) => return Err(unsupported_fault(&col.name, *t)),
        };
        col.n = row + 1;
        col.types |= bit;
        // The mix is checked at each value, so the query fails at the first value that breaks it.
        if let Err(e) = resolve_bits(col.types, col.big_long) {
            return Err(mix_fault(&col.name, col.types, e));
        }
        Ok(json_len)
    }

    /// Makes the frame. A result with no document, or with documents that have no field, gives a
    /// frame with no column and no row. The member `type` of each column is the type name of 7.16c.
    /// The member `stats` has `read_rows` and `read_bytes` set to `null`. `result_bytes` is the count
    /// of 8b.6.
    ///
    /// # Errors
    ///
    /// A cell that does not fit the type of its column. This is a fault of the builder, and the
    /// caller reports it as `source_bad_response`.
    pub fn finish(mut self, query_id: String, elapsed_ms: u64) -> Result<Frame, Fault> {
        let stats = FrameStats { elapsed_ms, read_rows: None, read_bytes: None, result_bytes: Some(self.bytes) };
        if self.cols.is_empty() {
            return Ok(Frame { etag: None, query_id, interval_ms: None, columns: Vec::new(), rows: 0, stats });
        }
        let rows = self.rows;
        let mut types = Vec::with_capacity(self.cols.len());
        for c in &mut self.cols {
            c.pad(rows);
            types.push(resolve_bits(c.types, c.big_long).map_err(|e| mix_fault(&c.name, c.types, e))?);
        }
        let names = self.cols.iter().map(|c| c.name.clone()).collect();
        let proxies = types.iter().map(|t| t.proxy().to_owned()).collect();
        let mut builder = FrameBuilder::new(names, proxies).map_err(|e| Fault::BadResponse(e.message().to_owned()))?;
        let mut readers: Vec<CellReader<'_>> = self.cols.iter().map(Acc::reader).collect();
        let mut line = String::new();
        for _ in 0..rows {
            line.clear();
            line.push('[');
            for (i, (reader, t)) in readers.iter_mut().zip(&types).enumerate() {
                if i > 0 {
                    line.push(',');
                }
                write_cell(&mut line, *t, reader.next_cell().unwrap_or(Cell::Null))?;
            }
            line.push(']');
            builder.push_row(&line).map_err(|e| Fault::BadResponse(e.message().to_owned()))?;
        }
        let mut frame = builder.finish(query_id, stats);
        for (col, t) in frame.columns.iter_mut().zip(&types) {
            col.type_text = t.name().to_owned();
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Kind;
    use crate::mongo::value::Decimal128Text;

    fn decimal(text: &str) -> MongoValue {
        MongoValue::Decimal128(Decimal128Text::from_server(text).expect("a text that is not empty"))
    }

    fn caps(rows: u64, bytes: u64) -> FrameCaps {
        FrameCaps { max_rows: rows, max_bytes: bytes }
    }

    fn doc(text: &str) -> Document {
        MongoValue::document_from_extended_json(text).unwrap()
    }

    fn build(docs: &[&str]) -> Result<Frame, Fault> {
        let mut b = MongoFrameBuilder::new(caps(1000, 1 << 20));
        for d in docs {
            b.push_document(&doc(d))?;
        }
        b.finish("q1".to_owned(), 7)
    }

    fn json(f: &Frame) -> serde_json::Value {
        serde_json::from_str(&f.to_json()).unwrap()
    }

    #[test]
    fn each_type_gives_its_kind_and_its_value() {
        let f = build(&[
            r#"{"i":1,"l":{"$numberLong":"9223372036854775807"},"d":1.5,"dec":{"$numberDecimal":"12.500000000"},
               "s":"x","b":true,"t":{"$date":{"$numberLong":"1794211200125"}},
               "o":{"$oid":"507f1f77bcf86cd799439011"},"u":{"$uuid":"123e4567-e89b-12d3-a456-426614174000"},
               "j":{"a":1},"arr":[1,"x"],"n":null}"#,
        ])
        .unwrap();
        let cols: Vec<(&str, &str, &str)> =
            f.columns.iter().map(|c| (c.name.as_str(), c.type_text.as_str(), c.kind.as_str())).collect();
        assert_eq!(
            cols,
            [
                ("i", "int", "number"),
                ("l", "long", "number_text"),
                ("d", "double", "number"),
                ("dec", "decimal", "number_text"),
                ("s", "string", "text"),
                ("b", "bool", "bool"),
                ("t", "date", "time"),
                ("o", "objectId", "text"),
                ("u", "uuid", "text"),
                ("j", "object", "text"),
                ("arr", "array", "text"),
                ("n", "null", "null"),
            ]
        );
        assert_eq!(
            json(&f)["values"],
            serde_json::json!([
                [1],
                ["9223372036854775807"],
                [1.5],
                ["12.500000000"],
                ["x"],
                [true],
                [1794211200.125],
                ["507f1f77bcf86cd799439011"],
                ["123e4567-e89b-12d3-a456-426614174000"],
                ["{\"a\":1}"],
                ["[1,\"x\"]"],
                [null]
            ])
        );
        assert_eq!(f.rows, 1);
        assert_eq!(f.stats.read_rows, None);
        assert_eq!(f.stats.read_bytes, None);
        assert!(f.stats.result_bytes.is_some());
        assert_eq!(f.stats.elapsed_ms, 7);
    }

    #[test]
    fn a_date_has_three_decimals_at_most_and_may_precede_1970() {
        assert_eq!(date_seconds_text(1_794_211_200_125), "1794211200.125");
        assert_eq!(date_seconds_text(1_794_211_200_000), "1794211200");
        assert_eq!(date_seconds_text(1_794_211_200_120), "1794211200.12");
        assert_eq!(date_seconds_text(0), "0");
        assert_eq!(date_seconds_text(-500), "-0.5");
        assert_eq!(date_seconds_text(-1500), "-1.5");
        assert_eq!(date_seconds_text(-86_400_000), "-86400");
        assert_eq!(date_seconds_text(i64::MIN), "-9223372036854775.808");
        let f = build(&[r#"{"t":{"$date":{"$numberLong":"-1500"}}}"#, r#"{"t":{"$date":"2026-11-09T08:00:00.125Z"}}"#])
            .unwrap();
        assert_eq!(f.columns[0].kind, Kind::Time);
        assert_eq!(json(&f)["values"], serde_json::json!([[-1.5, 1794211200.125]]));
    }

    #[test]
    fn a_double_that_is_not_finite_is_a_text() {
        let mut b = MongoFrameBuilder::new(caps(10, 1000));
        for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.1, -0.0, 3.0, 1e21] {
            b.push_document(&vec![("d".to_owned(), MongoValue::Double(x))]).unwrap();
        }
        let f = b.finish("q".into(), 0).unwrap();
        assert_eq!(f.columns[0].type_text, "double");
        assert_eq!(json(&f)["values"], serde_json::json!([["nan", "inf", "-inf", 0.1, -0.0, 3.0, 1e21]]));
    }

    #[test]
    fn a_decimal_keeps_its_scale_and_a_special_value_is_null() {
        let mut b = MongoFrameBuilder::new(caps(10, 1000));
        for t in ["12.50", "NaN", "Infinity", "-Infinity", "-0.00", "1.20E+3", "5E-7", "0E+2", "-1.5E-1", "123"] {
            b.push_document(&vec![("d".to_owned(), decimal(t))]).unwrap();
        }
        let f = b.finish("q".into(), 0).unwrap();
        assert_eq!((f.columns[0].type_text.as_str(), f.columns[0].kind), ("decimal", Kind::NumberText));
        assert_eq!(
            json(&f)["values"],
            serde_json::json!([["12.50", null, null, null, "-0.00", "1200", "0.0000005", "0", "-0.15", "123"]])
        );
    }

    #[test]
    fn a_decimal_that_is_not_valid_is_a_bad_response_and_a_huge_one_is_a_query_error() {
        let push = |t: &str| MongoFrameBuilder::new(caps(10, 1000)).push_document(&vec![("d".to_owned(), decimal(t))]);
        assert!(matches!(push("12x").unwrap_err(), Fault::BadResponse(_)));
        assert!(matches!(push("1E+6000").unwrap_err(), Fault::Query(_)));
        assert!(matches!(push("1E-6000").unwrap_err(), Fault::Query(_)));
        assert!(push("1E+300").is_ok());
    }

    #[test]
    fn widening_follows_the_table() {
        // Int32 and Int64: long, each Int32 as a digit text.
        let f = build(&[r#"{"a":1}"#, r#"{"a":{"$numberLong":"5000000000"}}"#]).unwrap();
        assert_eq!((f.columns[0].type_text.as_str(), f.columns[0].kind), ("long", Kind::NumberText));
        assert_eq!(json(&f)["values"], serde_json::json!([["1", "5000000000"]]));
        // Int32 and Double: double.
        let f = build(&[r#"{"a":1}"#, r#"{"a":2.5}"#]).unwrap();
        assert_eq!((f.columns[0].type_text.as_str(), f.columns[0].kind), ("double", Kind::Number));
        assert_eq!(json(&f)["values"], serde_json::json!([[1.0, 2.5]]));
        // Int64 and Double: double, up to 2^53 in absolute value.
        let f = build(&[r#"{"a":{"$numberLong":"9007199254740992"}}"#, r#"{"a":2.5}"#]).unwrap();
        assert_eq!(f.columns[0].type_text, "double");
        let e = build(&[r#"{"a":{"$numberLong":"9007199254740993"}}"#, r#"{"a":2.5}"#]).unwrap_err();
        assert!(matches!(&e, Fault::Query(m) if m.contains("2^53") && m.contains("\"a\"")), "{e:?}");
        let e = build(&[r#"{"a":2.5}"#, r#"{"a":{"$numberLong":"-9007199254740993"}}"#]).unwrap_err();
        assert!(matches!(e, Fault::Query(_)));
        // Int32, Int64 and Double.
        let f = build(&[r#"{"a":1}"#, r#"{"a":{"$numberLong":"5"}}"#, r#"{"a":0.5}"#]).unwrap();
        assert_eq!(f.columns[0].type_text, "double");
        // Decimal128 with an integer: decimal.
        let f = build(&[r#"{"a":{"$numberDecimal":"1.50"}}"#, r#"{"a":2}"#, r#"{"a":{"$numberLong":"3"}}"#]).unwrap();
        assert_eq!(f.columns[0].type_text, "decimal");
        assert_eq!(json(&f)["values"], serde_json::json!([["1.50", "2", "3"]]));
        // Document and Array: json.
        let f = build(&[r#"{"a":{"x":1}}"#, r#"{"a":[1,2]}"#]).unwrap();
        assert_eq!((f.columns[0].type_text.as_str(), f.columns[0].kind), ("json", Kind::Text));
        assert_eq!(json(&f)["values"], serde_json::json!([["{\"x\":1}", "[1,2]"]]));
        // Null values do not change the type.
        let f = build(&[r#"{"a":null}"#, r#"{"a":1}"#, r#"{"a":null}"#]).unwrap();
        assert_eq!(f.columns[0].type_text, "int");
        assert_eq!(json(&f)["values"], serde_json::json!([[null, 1, null]]));
    }

    #[test]
    fn a_mix_that_the_table_refuses_is_a_query_error_that_names_the_column_and_the_types() {
        for pair in [
            (r#"{"a":1}"#, r#"{"a":"x"}"#),
            (r#"{"a":{"$numberDecimal":"1"}}"#, r#"{"a":1.5}"#),
            (r#"{"a":true}"#, r#"{"a":1}"#),
            (r#"{"a":{"$date":"2026-01-01T00:00:00.000Z"}}"#, r#"{"a":1}"#),
            (r#"{"a":"x"}"#, r#"{"a":{"$oid":"507f1f77bcf86cd799439011"}}"#),
            (r#"{"a":{"x":1}}"#, r#"{"a":"s"}"#),
        ] {
            let e = build(&[pair.0, pair.1]).unwrap_err();
            let Fault::Query(m) = e else { panic!("{pair:?}: {e:?}") };
            assert!(m.contains("\"a\"") && m.contains("$toString"), "{m}");
        }
        let Fault::Query(m) = build(&[r#"{"a":1}"#, r#"{"a":"x"}"#]).unwrap_err() else { panic!() };
        assert!(m.contains("int, string"), "{m}");
        // The message holds no value.
        let Fault::Query(m) = build(&[r#"{"a":1}"#, r#"{"a":"secret-value"}"#]).unwrap_err() else { panic!() };
        assert!(!m.contains("secret-value"));
    }

    #[test]
    fn a_column_of_each_unsupported_type_is_a_query_error() {
        for (form, name) in [
            (r#"{"$binary":{"base64":"AQID","subType":"00"}}"#, "binData"),
            (r#"{"$regularExpression":{"pattern":"a","options":""}}"#, "regex"),
            (r#"{"$timestamp":{"t":1,"i":1}}"#, "timestamp"),
            (r#"{"$code":"x"}"#, "javascript"),
            (r#"{"$symbol":"x"}"#, "symbol"),
            (r#"{"$minKey":1}"#, "minKey"),
            (r#"{"$maxKey":1}"#, "maxKey"),
            (r#"{"$undefined":true}"#, "undefined"),
        ] {
            let text = format!("{{\"a\":{form}}}");
            let e = build(&[&text]).unwrap_err();
            let Fault::Query(m) = e else { panic!("{name}: {e:?}") };
            assert!(m.contains(name) && m.contains("\"a\""), "{m}");
        }
        // Inside a nested value too.
        let e = build(&[r#"{"a":{"b":{"$minKey":1}}}"#]).unwrap_err();
        assert!(matches!(e, Fault::Query(m) if m.contains("minKey")));
    }

    #[test]
    fn the_columns_follow_the_first_appearance_and_a_missing_field_is_null() {
        let f = build(&[r#"{"a":1}"#, r#"{"b":"x","a":2}"#, r#"{"c":true}"#]).unwrap();
        let names: Vec<_> = f.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(json(&f)["values"], serde_json::json!([[1, 2, null], [null, "x", null], [null, null, true]]));
        assert_eq!(f.rows, 3);
    }

    #[test]
    fn a_nested_value_is_not_split_into_columns_and_keeps_the_document_order() {
        let f = build(&[r#"{"a":{"z":1,"b":{"$oid":"507f1f77bcf86cd799439011"},"m":[null,true,"s",1.5]}}"#]).unwrap();
        assert_eq!(f.columns.len(), 1);
        assert_eq!(
            json(&f)["values"][0][0],
            serde_json::json!(r#"{"z":1,"b":{"$oid":"507f1f77bcf86cd799439011"},"m":[null,true,"s",1.5]}"#)
        );
    }

    #[test]
    fn the_text_of_a_nested_value_is_relaxed_extended_json() {
        let v = MongoValue::Document(vec![
            ("i".to_owned(), MongoValue::Int32(-3)),
            ("l".to_owned(), MongoValue::Int64(9_223_372_036_854_775_807)),
            ("n".to_owned(), MongoValue::Double(f64::NAN)),
            ("p".to_owned(), MongoValue::Double(f64::INFINITY)),
            ("m".to_owned(), MongoValue::Double(f64::NEG_INFINITY)),
            ("w".to_owned(), MongoValue::Double(2.0)),
            ("dec".to_owned(), decimal("12.50")),
            ("d".to_owned(), MongoValue::DateMs(1_794_211_200_125)),
            ("old".to_owned(), MongoValue::DateMs(-1)),
            ("far".to_owned(), MongoValue::DateMs(253_402_300_800_000)),
            ("u".to_owned(), MongoValue::Uuid([0x12; 16])),
            ("q\"".to_owned(), MongoValue::String("a\"b\\c\n\u{1}é".to_owned())),
        ]);
        let mut out = String::new();
        write_relaxed_json(&mut out, &v).unwrap();
        assert_eq!(
            out,
            concat!(
                r#"{"i":-3,"l":9223372036854775807,"n":{"$numberDouble":"NaN"},"p":{"$numberDouble":"Infinity"},"#,
                r#""m":{"$numberDouble":"-Infinity"},"w":2.0,"dec":{"$numberDecimal":"12.50"},"#,
                r#""d":{"$date":"2026-11-09T08:00:00.125Z"},"old":{"$date":{"$numberLong":"-1"}},"#,
                r#""far":{"$date":{"$numberLong":"253402300800000"}},"#,
                r#""u":{"$uuid":"12121212-1212-1212-1212-121212121212"},"#,
                r#""q\"":"a\"b\\c\n\u0001é"}"#
            )
        );
        // The text reads back with the same values.
        let back = MongoValue::from_extended_json(&out).unwrap();
        assert_eq!(back.get("l"), Some(&MongoValue::Int64(i64::MAX)));
        assert_eq!(back.get("d"), Some(&MongoValue::DateMs(1_794_211_200_125)));
    }

    #[test]
    fn a_repeated_field_name_is_a_bad_response_at_any_depth() {
        let dup: Document = vec![("a".to_owned(), MongoValue::Int32(1)), ("a".to_owned(), MongoValue::Int32(2))];
        let mut b = MongoFrameBuilder::new(caps(10, 1000));
        assert!(matches!(b.push_document(&dup).unwrap_err(), Fault::BadResponse(_)));
        let nested: Document = vec![("x".to_owned(), MongoValue::Document(dup))];
        let mut b = MongoFrameBuilder::new(caps(10, 1000));
        assert!(matches!(b.push_document(&nested).unwrap_err(), Fault::BadResponse(_)));
        // More than 16 members use the set.
        let many: Document = (0..40).map(|i| (format!("k{}", i % 39), MongoValue::Null)).collect();
        let mut out = String::new();
        assert_eq!(write_relaxed_json(&mut out, &MongoValue::Document(many)), Err(RelaxedError::DuplicateKey));
    }

    #[test]
    fn a_very_deep_value_is_refused() {
        let mut v = MongoValue::Int32(1);
        for _ in 0..300 {
            v = MongoValue::Array(vec![v]);
        }
        let mut b = MongoFrameBuilder::new(caps(10, 1 << 20));
        let e = b.push_document(&vec![("a".to_owned(), v)]).unwrap_err();
        assert!(matches!(e, Fault::BadResponse(_)));
    }

    #[test]
    fn the_result_with_no_document_is_a_frame_with_no_column() {
        let f = MongoFrameBuilder::new(caps(10, 100)).finish("q9".to_owned(), 3).unwrap();
        assert!(f.columns.is_empty());
        assert_eq!(f.rows, 0);
        assert_eq!(f.stats.result_bytes, Some(0));
        let v = json(&f);
        assert_eq!(
            (&v["columns"], &v["values"], &v["rows"]),
            (&serde_json::json!([]), &serde_json::json!([]), &serde_json::json!(0))
        );
        assert_eq!(v["query_id"], "q9");
        // Documents with no field give the same frame.
        let mut b = MongoFrameBuilder::new(caps(10, 100));
        b.push_document(&Vec::new()).unwrap();
        let f = b.finish("q".into(), 0).unwrap();
        assert!(f.columns.is_empty());
        assert_eq!(f.rows, 0);
    }

    #[test]
    fn the_row_cap_is_exact() {
        let mut b = MongoFrameBuilder::new(caps(2, 1 << 20));
        b.push_document(&doc(r#"{"a":1}"#)).unwrap();
        b.push_document(&doc(r#"{"a":2}"#)).unwrap();
        assert_eq!(b.rows(), 2);
        assert_eq!(b.push_document(&doc(r#"{"a":3}"#)), Err(Fault::Rows));
        assert_eq!(b.rows(), 2);
    }

    #[test]
    fn the_byte_count_is_the_json_length_plus_one_for_each_value() {
        let mut b = MongoFrameBuilder::new(caps(10, 1 << 20));
        // 1 (int) + 1, "ab" (4) + 1, null (4) + 1, true (4) + 1.
        b.push_document(&doc(r#"{"i":1,"s":"ab","n":null,"b":true}"#)).unwrap();
        assert_eq!(b.bytes(), 2 + 5 + 5 + 5);
        // A missing field counts as null: 5 bytes.
        b.push_document(&doc(r#"{"i":22}"#)).unwrap();
        assert_eq!(b.bytes(), 17 + 3 + 15);
        // A new column counts null for the rows before it.
        b.push_document(&doc(r#"{"z":1}"#)).unwrap();
        assert_eq!(b.bytes(), 35 + 2 * 5 + 2 + 4 * 5);
        let f = b.finish("q".into(), 0).unwrap();
        assert_eq!(f.stats.result_bytes, Some(67));
    }

    #[test]
    fn the_byte_cap_stops_at_the_first_value_above_it() {
        let mut b = MongoFrameBuilder::new(caps(10, 20));
        let big = "x".repeat(10_000);
        let d: Document = vec![("a".to_owned(), MongoValue::Int32(1)), ("s".to_owned(), MongoValue::String(big))];
        assert_eq!(b.push_document(&d), Err(Fault::Bytes));
        // A count equal to the cap passes.
        let mut b = MongoFrameBuilder::new(caps(10, 2 + 5));
        b.push_document(&doc(r#"{"i":1,"s":"ab"}"#)).unwrap();
        assert_eq!(b.bytes(), 7);
        assert_eq!(b.push_document(&doc(r#"{"i":1}"#)), Err(Fault::Bytes));
    }

    #[test]
    fn the_column_limit_is_256() {
        let fields = |n: usize| -> Document { (0..n).map(|i| (format!("f{i}"), MongoValue::Int32(1))).collect() };
        let mut b = MongoFrameBuilder::new(caps(10, 1 << 20));
        b.push_document(&fields(256)).unwrap();
        assert_eq!(b.column_count(), 256);
        let f = b.finish("q".into(), 0).unwrap();
        assert_eq!(f.columns.len(), 256);
        let mut b = MongoFrameBuilder::new(caps(10, 1 << 20));
        let e = b.push_document(&fields(257)).unwrap_err();
        assert!(matches!(e, Fault::Query(m) if m.contains("256")));
        // A column that comes in a later document counts too.
        let mut b = MongoFrameBuilder::new(caps(10, 1 << 20));
        b.push_document(&fields(256)).unwrap();
        let e = b.push_document(&vec![("extra".to_owned(), MongoValue::Null)]).unwrap_err();
        assert!(matches!(e, Fault::Query(_)));
    }

    #[test]
    fn resolve_is_the_table_of_7_16c() {
        assert_eq!(resolve(&[], false), Ok(ColumnType::Null));
        assert_eq!(resolve(&["null", "null"], false), Ok(ColumnType::Null));
        assert_eq!(resolve(&["int", "long"], false), Ok(ColumnType::Long));
        assert_eq!(resolve(&["int", "double"], false), Ok(ColumnType::Double));
        assert_eq!(resolve(&["long", "double"], false), Ok(ColumnType::Double));
        assert_eq!(resolve(&["long", "double"], true), Err(MixError::BigLong));
        assert_eq!(resolve(&["decimal", "int"], false), Ok(ColumnType::Decimal));
        assert_eq!(resolve(&["decimal", "long", "null"], false), Ok(ColumnType::Decimal));
        assert_eq!(resolve(&["object", "array"], false), Ok(ColumnType::Json));
        assert_eq!(resolve(&["int", "string"], false), Err(MixError::Mix));
        assert_eq!(resolve(&["decimal", "double"], false), Err(MixError::Mix));
        assert_eq!(resolve(&["bool", "int"], false), Err(MixError::Mix));
        assert_eq!(resolve(&["nonsense"], false), Err(MixError::Mix));
        assert_eq!(resolve(&["objectId", "uuid"], false), Err(MixError::Mix));
    }

    #[test]
    fn a_large_result_keeps_the_values_in_order() {
        let mut b = MongoFrameBuilder::new(caps(10_000, 1 << 30));
        for i in 0..5000i32 {
            let d: Document = if i % 3 == 0 {
                vec![("n".to_owned(), MongoValue::Int32(i))]
            } else {
                vec![("n".to_owned(), MongoValue::Int32(i)), ("s".to_owned(), MongoValue::String(format!("v{i}")))]
            };
            b.push_document(&d).unwrap();
        }
        let f = b.finish("q".into(), 0).unwrap();
        assert_eq!(f.rows, 5000);
        let v = json(&f);
        assert_eq!(v["values"][0][4999], 4999);
        assert_eq!(v["values"][1][0], serde_json::Value::Null);
        assert_eq!(v["values"][1][4999], "v4999");
    }

    #[test]
    fn debug_has_counts_only() {
        let mut b = MongoFrameBuilder::new(caps(10, 1000));
        b.push_document(&doc(r#"{"secret":"hunter2"}"#)).unwrap();
        let t = format!("{b:?}");
        assert!(!t.contains("hunter2") && !t.contains("secret"));
    }
}
