//! Binary decode of the values of a row (7.16a).
//!
//! The client asks for the binary form of each column (8a.5). This module gives the closed list
//! of the column types ([`ColumnType::from_pg`]) and decodes one value by its type
//! ([`decode`]). The result is a [`Value`]: the frame value of the table of 7.16a.
//!
//! The list: `bool`, `int2`, `int4`, `oid`, `int8`, `float4`, `float8`, `numeric` (with its
//! `dscale`), the text types, `uuid`, `json`, `jsonb`, `inet`, `cidr`, enum labels, `date`,
//! `timestamp`, `timestamptz`, `time` and the arrays of these types, except `json` and `jsonb`.
//! A domain has the kind of its base type. A type outside the list gives [`UnsupportedType`].
//!
//! A value that does not decode gives [`DecodeError`]. The error text names the type and the
//! rule. It never holds a value of a row. The module does no I/O and holds no state.
//!
//! Choices where 7.16a is silent (see the report of the milestone):
//!
//! - An array element `numeric` NaN, `Infinity` or `-Infinity` is the JSON string `"NaN"`,
//!   `"Infinity"` or `"-Infinity"`, as for a `float` element.
//! - An array element `date`, `timestamp` or `timestamptz` with the value `infinity` or
//!   `-infinity` is `null`, as for the scalar.
//! - The lower bounds of an array are not part of the JSON text.

use std::fmt::Write as _;

use hune_core::frame::write_json_string;
use hune_core::time::civil_from_days;
use tokio_postgres::types::{FromSql, Kind, Type};

/// The maximum number of dimensions of a PostgreSQL array (`MAXDIM`).
const MAX_DIMS: usize = 6;
/// Microseconds in one second.
const US_PER_SEC: i64 = 1_000_000;
/// Microseconds in one day.
const US_PER_DAY: i64 = 86_400 * US_PER_SEC;
/// Days from 1970-01-01 to 2000-01-01, the epoch of the binary `date` and `timestamp`.
const DAYS_1970_TO_2000: i64 = 10_957;
/// The largest `dscale` of a `numeric` (`NUMERIC_MAX_DISPLAY_SCALE` of the wire format).
const NUMERIC_MAX_DSCALE: usize = 16_383;

/// The scalar types of the closed list of 7.16a.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scalar {
    /// `bool`.
    Bool,
    /// `int2`.
    Int2,
    /// `int4`.
    Int4,
    /// `oid`.
    Oid,
    /// `int8`.
    Int8,
    /// `float4`.
    Float4,
    /// `float8`.
    Float8,
    /// `numeric`.
    Numeric,
    /// `text`, `varchar`, `bpchar` and `name`.
    Text,
    /// `uuid`.
    Uuid,
    /// `json`.
    Json,
    /// `jsonb`.
    Jsonb,
    /// `inet`.
    Inet,
    /// `cidr`.
    Cidr,
    /// An enum type: the wire value is the label.
    Enum,
    /// `date`.
    Date,
    /// `timestamp`: read as UTC.
    Timestamp,
    /// `timestamptz`.
    Timestamptz,
    /// `time`.
    Time,
}

/// A column type that the closed list of 7.16a accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColumnType {
    /// The member `type` of the frame: `pg_type.typname`, or the element name and `[]`.
    pub name: String,
    /// The scalar type, or the element type of an array.
    pub scalar: Scalar,
    /// True for an array.
    pub array: bool,
}

/// A column type that 7.16a does not list (ID-PG12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsupportedType {
    /// The name of the type: `pg_type.typname`, or the element name and `[]`.
    pub name: String,
}

/// A value that does not decode (8a.8: `502 source_bad_response`).
///
/// The text never holds a value of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodeError {
    message: String,
}

impl DecodeError {
    fn new(ty: &str, rule: &str) -> DecodeError {
        DecodeError { message: format!("A value of the type {ty} does not decode: {rule}.") }
    }

    /// Gives the message.
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl ColumnType {
    /// Finds the entry of the closed list for a PostgreSQL type, or refuses the type.
    ///
    /// A domain gives the entry of its base type, with the name of the domain. A built-in type is
    /// found by its oid: a user type that has the name of a built-in type is not a built-in type.
    pub(crate) fn from_pg(ty: &Type) -> Result<ColumnType, UnsupportedType> {
        match resolve(ty) {
            Resolved::Scalar(scalar) => Ok(ColumnType { name: ty.name().to_owned(), scalar, array: false }),
            Resolved::Array(elem_name, scalar) => {
                Ok(ColumnType { name: format!("{elem_name}[]"), scalar, array: true })
            }
            Resolved::Unsupported(name) => Err(UnsupportedType { name }),
        }
    }
}

enum Resolved {
    Scalar(Scalar),
    Array(String, Scalar),
    Unsupported(String),
}

fn builtin_scalar(ty: &Type) -> Option<Scalar> {
    // Only a built-in type has an oid below 16384.
    if ty.oid() >= 16_384 {
        return None;
    }
    Some(if *ty == Type::BOOL {
        Scalar::Bool
    } else if *ty == Type::INT2 {
        Scalar::Int2
    } else if *ty == Type::INT4 {
        Scalar::Int4
    } else if *ty == Type::OID {
        Scalar::Oid
    } else if *ty == Type::INT8 {
        Scalar::Int8
    } else if *ty == Type::FLOAT4 {
        Scalar::Float4
    } else if *ty == Type::FLOAT8 {
        Scalar::Float8
    } else if *ty == Type::NUMERIC {
        Scalar::Numeric
    } else if *ty == Type::TEXT || *ty == Type::VARCHAR || *ty == Type::BPCHAR || *ty == Type::NAME {
        Scalar::Text
    } else if *ty == Type::UUID {
        Scalar::Uuid
    } else if *ty == Type::JSON {
        Scalar::Json
    } else if *ty == Type::JSONB {
        Scalar::Jsonb
    } else if *ty == Type::INET {
        Scalar::Inet
    } else if *ty == Type::CIDR {
        Scalar::Cidr
    } else if *ty == Type::DATE {
        Scalar::Date
    } else if *ty == Type::TIMESTAMP {
        Scalar::Timestamp
    } else if *ty == Type::TIMESTAMPTZ {
        Scalar::Timestamptz
    } else if *ty == Type::TIME {
        Scalar::Time
    } else {
        return None;
    })
}

/// Finds the scalar of a type that is not an array: a built-in type, an enum or a domain of them.
fn scalar_of(ty: &Type) -> Option<Scalar> {
    match ty.kind() {
        Kind::Enum(_) => Some(Scalar::Enum),
        Kind::Domain(base) => match base.kind() {
            Kind::Array(_) => None,
            _ => scalar_of(base),
        },
        Kind::Simple => builtin_scalar(ty),
        _ => None,
    }
}

fn resolve(ty: &Type) -> Resolved {
    match ty.kind() {
        Kind::Array(elem) => array_of(elem),
        Kind::Domain(base) => match base.kind() {
            // A domain over an array has the array rule of its base type.
            Kind::Array(elem) => array_of(elem),
            _ => match scalar_of(ty) {
                Some(s) => Resolved::Scalar(s),
                None => Resolved::Unsupported(ty.name().to_owned()),
            },
        },
        _ => match scalar_of(ty) {
            Some(s) => Resolved::Scalar(s),
            None => Resolved::Unsupported(ty.name().to_owned()),
        },
    }
}

fn array_of(elem: &Type) -> Resolved {
    match scalar_of(elem) {
        // The array of `json` or `jsonb` is not in the list (7.16a).
        Some(Scalar::Json | Scalar::Jsonb) | None => Resolved::Unsupported(format!("{}[]", elem.name())),
        Some(s) => Resolved::Array(elem.name().to_owned(), s),
    }
}

/// The frame value of 7.16a, before the checks of the frame builder.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// A NULL.
    Null,
    /// A boolean.
    Bool(bool),
    /// A JSON number: the exact digits, written as they are.
    Number(String),
    /// A text: written as a JSON string.
    Text(String),
}

impl Value {
    /// Gives the length in bytes of the compact JSON text of the value (7.16, 8a.6).
    pub(crate) fn json_len(&self) -> usize {
        match self {
            Value::Null => 4,
            Value::Bool(true) => 4,
            Value::Bool(false) => 5,
            Value::Number(s) => s.len(),
            Value::Text(s) => json_string_len(s),
        }
    }

    /// Appends the compact JSON text of the value.
    pub(crate) fn write_json(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Number(s) => out.push_str(s),
            Value::Text(s) => write_json_string(out, s),
        }
    }
}

/// Gives the length of `s` as `write_json_string` writes it, with the quote marks.
pub(crate) fn json_string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}

/// Decodes one value of the binary form (7.16a). `raw` is `None` for a NULL.
///
/// # Errors
///
/// The bytes do not match the wire format of the type.
pub(crate) fn decode(ty: &ColumnType, raw: Option<&[u8]>) -> Result<Value, DecodeError> {
    let Some(raw) = raw else {
        return Ok(Value::Null);
    };
    if ty.array {
        return decode_array(ty, raw).map(Value::Text);
    }
    let parsed = parse_scalar(ty.scalar, raw).map_err(|r| DecodeError::new(&ty.name, r))?;
    Ok(parsed.into_value())
}

/// A scalar after the parse of its wire bytes.
enum Parsed {
    Bool(bool),
    /// `int2`, `int4`, `oid`.
    Int(i64),
    /// `int8`.
    BigInt(i64),
    Float(FloatOut),
    Numeric(NumericOut),
    Text(String),
    /// Days since 1970-01-01. `None` for an infinity.
    Date(Option<i64>),
    /// Microseconds since 1970-01-01 00:00:00 UTC. `None` for an infinity.
    Timestamp(Option<i64>),
}

enum FloatOut {
    Finite(String),
    Nan,
    PosInf,
    NegInf,
}

enum NumericOut {
    Finite(String),
    Nan,
    PosInf,
    NegInf,
}

impl Parsed {
    /// The frame value of a column of this type (first table of 7.16a).
    fn into_value(self) -> Value {
        match self {
            Parsed::Bool(b) => Value::Bool(b),
            Parsed::Int(i) => Value::Number(i.to_string()),
            Parsed::BigInt(i) => Value::Text(i.to_string()),
            Parsed::Float(FloatOut::Finite(s)) => Value::Number(s),
            Parsed::Float(FloatOut::Nan) => Value::Text("nan".into()),
            Parsed::Float(FloatOut::PosInf) => Value::Text("inf".into()),
            Parsed::Float(FloatOut::NegInf) => Value::Text("-inf".into()),
            Parsed::Numeric(NumericOut::Finite(s)) => Value::Text(s),
            Parsed::Numeric(_) => Value::Null,
            Parsed::Text(s) => Value::Text(s),
            Parsed::Date(Some(days)) => Value::Number((days * 86_400).to_string()),
            Parsed::Date(None) | Parsed::Timestamp(None) => Value::Null,
            Parsed::Timestamp(Some(us)) => Value::Number(unix_seconds_text(us)),
        }
    }

    /// Writes the element of an array (the array rule of 7.16a).
    fn write_element(&self, out: &mut String) {
        match self {
            Parsed::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Parsed::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Parsed::BigInt(i) => {
                let _ = write!(out, "\"{i}\"");
            }
            Parsed::Float(FloatOut::Finite(s)) => out.push_str(s),
            Parsed::Float(FloatOut::Nan) | Parsed::Numeric(NumericOut::Nan) => out.push_str("\"NaN\""),
            Parsed::Float(FloatOut::PosInf) | Parsed::Numeric(NumericOut::PosInf) => out.push_str("\"Infinity\""),
            Parsed::Float(FloatOut::NegInf) | Parsed::Numeric(NumericOut::NegInf) => out.push_str("\"-Infinity\""),
            Parsed::Numeric(NumericOut::Finite(s)) | Parsed::Text(s) => write_json_string(out, s),
            Parsed::Date(Some(days)) => {
                let mut s = String::new();
                push_date(&mut s, *days);
                write_json_string(out, &s);
            }
            Parsed::Timestamp(Some(us)) => {
                let mut s = String::new();
                push_timestamp(&mut s, *us);
                s.push('Z');
                write_json_string(out, &s);
            }
            Parsed::Date(None) | Parsed::Timestamp(None) => out.push_str("null"),
        }
    }
}

fn be_i16(b: &[u8]) -> i64 {
    i64::from(i16::from_be_bytes([b[0], b[1]]))
}

fn be_u16(b: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([b[0], b[1]]))
}

fn utf8(raw: &[u8]) -> Result<&str, &'static str> {
    std::str::from_utf8(raw).map_err(|_| "the text is not UTF-8")
}

/// Parses the wire bytes of one scalar. The error is a rule, without any value.
fn parse_scalar(scalar: Scalar, raw: &[u8]) -> Result<Parsed, &'static str> {
    const BAD_LEN: &str = "the length is wrong";
    match scalar {
        Scalar::Bool => match raw {
            [0] => Ok(Parsed::Bool(false)),
            [1] => Ok(Parsed::Bool(true)),
            _ => Err("the byte is not 0 or 1"),
        },
        Scalar::Int2 => {
            let b: [u8; 2] = raw.try_into().map_err(|_| BAD_LEN)?;
            Ok(Parsed::Int(i64::from(i16::from_be_bytes(b))))
        }
        Scalar::Int4 => {
            let b: [u8; 4] = raw.try_into().map_err(|_| BAD_LEN)?;
            Ok(Parsed::Int(i64::from(i32::from_be_bytes(b))))
        }
        Scalar::Oid => {
            let b: [u8; 4] = raw.try_into().map_err(|_| BAD_LEN)?;
            Ok(Parsed::Int(i64::from(u32::from_be_bytes(b))))
        }
        Scalar::Int8 => {
            let b: [u8; 8] = raw.try_into().map_err(|_| BAD_LEN)?;
            Ok(Parsed::BigInt(i64::from_be_bytes(b)))
        }
        Scalar::Float4 => {
            let b: [u8; 4] = raw.try_into().map_err(|_| BAD_LEN)?;
            let f = f32::from_be_bytes(b);
            Ok(Parsed::Float(if f.is_nan() {
                FloatOut::Nan
            } else if f.is_infinite() {
                if f > 0.0 { FloatOut::PosInf } else { FloatOut::NegInf }
            } else {
                // `serde_json` writes an `f32` as an `f32`: 0.1 stays 0.1.
                FloatOut::Finite(serde_json::to_string(&f).map_err(|_| "the number does not print")?)
            }))
        }
        Scalar::Float8 => {
            let b: [u8; 8] = raw.try_into().map_err(|_| BAD_LEN)?;
            let f = f64::from_be_bytes(b);
            Ok(Parsed::Float(if f.is_nan() {
                FloatOut::Nan
            } else if f.is_infinite() {
                if f > 0.0 { FloatOut::PosInf } else { FloatOut::NegInf }
            } else {
                FloatOut::Finite(serde_json::to_string(&f).map_err(|_| "the number does not print")?)
            }))
        }
        Scalar::Numeric => parse_numeric(raw).map(Parsed::Numeric),
        Scalar::Text | Scalar::Json | Scalar::Enum => Ok(Parsed::Text(utf8(raw)?.to_owned())),
        Scalar::Jsonb => match raw.split_first() {
            Some((1, text)) => Ok(Parsed::Text(utf8(text)?.to_owned())),
            _ => Err("the version byte is not 1"),
        },
        Scalar::Uuid => {
            let b: [u8; 16] = raw.try_into().map_err(|_| BAD_LEN)?;
            let mut s = String::with_capacity(36);
            for (i, byte) in b.iter().enumerate() {
                if matches!(i, 4 | 6 | 8 | 10) {
                    s.push('-');
                }
                let _ = write!(s, "{byte:02x}");
            }
            Ok(Parsed::Text(s))
        }
        Scalar::Inet => parse_inet(raw, false).map(Parsed::Text),
        Scalar::Cidr => parse_inet(raw, true).map(Parsed::Text),
        Scalar::Date => {
            let b: [u8; 4] = raw.try_into().map_err(|_| BAD_LEN)?;
            let d = i32::from_be_bytes(b);
            Ok(Parsed::Date(if d == i32::MAX || d == i32::MIN { None } else { Some(i64::from(d) + DAYS_1970_TO_2000) }))
        }
        Scalar::Timestamp | Scalar::Timestamptz => {
            let b: [u8; 8] = raw.try_into().map_err(|_| BAD_LEN)?;
            let us = i64::from_be_bytes(b);
            if us == i64::MAX || us == i64::MIN {
                return Ok(Parsed::Timestamp(None));
            }
            let unix = us.checked_add(DAYS_1970_TO_2000 * US_PER_DAY).ok_or("the time is out of range")?;
            Ok(Parsed::Timestamp(Some(unix)))
        }
        Scalar::Time => {
            let b: [u8; 8] = raw.try_into().map_err(|_| BAD_LEN)?;
            let us = i64::from_be_bytes(b);
            if !(0..=US_PER_DAY).contains(&us) {
                return Err("the time is out of range");
            }
            let mut s = String::with_capacity(15);
            push_clock(&mut s, us);
            Ok(Parsed::Text(s))
        }
    }
}

/// Writes `HH:MM:SS[.ffffff]` for a count of microseconds in `0..=86_400_000_000`.
fn push_clock(s: &mut String, us: i64) {
    let secs = us / US_PER_SEC;
    let frac = us % US_PER_SEC;
    let _ = write!(s, "{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    push_fraction(s, frac);
}

/// Writes `.ffffff` without trailing zero. It writes nothing for a zero fraction.
fn push_fraction(s: &mut String, frac_us: i64) {
    if frac_us == 0 {
        return;
    }
    let digits = format!("{frac_us:06}");
    s.push('.');
    s.push_str(digits.trim_end_matches('0'));
}

/// Writes the year, the month and the day of a count of days since 1970-01-01.
fn push_date(s: &mut String, days: i64) {
    let (y, m, d) = civil_from_days(days);
    if (0..=9999).contains(&y) {
        let _ = write!(s, "{y:04}-{m:02}-{d:02}");
    } else if y < 0 {
        let _ = write!(s, "-{:04}-{m:02}-{d:02}", -y);
    } else {
        let _ = write!(s, "+{y}-{m:02}-{d:02}");
    }
}

/// Writes `YYYY-MM-DDTHH:MM:SS[.ffffff]` for microseconds since 1970-01-01 00:00:00 UTC.
fn push_timestamp(s: &mut String, unix_us: i64) {
    let days = unix_us.div_euclid(US_PER_DAY);
    let rem = unix_us.rem_euclid(US_PER_DAY);
    push_date(s, days);
    s.push('T');
    push_clock(s, rem);
}

/// Gives Unix seconds with up to 6 decimals and no trailing zero (7.16a, `timestamptz`).
fn unix_seconds_text(unix_us: i64) -> String {
    let abs = unix_us.unsigned_abs();
    let secs = abs / US_PER_SEC as u64;
    let frac = (abs % US_PER_SEC as u64) as i64;
    let mut s = String::with_capacity(24);
    if unix_us < 0 {
        s.push('-');
    }
    let _ = write!(s, "{secs}");
    push_fraction(&mut s, frac);
    s
}

/// Decodes the wire form of a `numeric`: base 10,000 digits, `weight`, sign and `dscale`.
fn parse_numeric(raw: &[u8]) -> Result<NumericOut, &'static str> {
    if raw.len() < 8 {
        return Err("the header is too short");
    }
    let ndigits = be_u16(&raw[0..2]);
    let weight = be_i16(&raw[2..4]);
    let sign = be_u16(&raw[4..6]);
    let dscale = be_u16(&raw[6..8]);
    if raw.len() != 8 + 2 * ndigits {
        return Err("the length does not match the number of digits");
    }
    match sign {
        0xC000 | 0xD000 | 0xF000 => {
            if ndigits != 0 {
                return Err("a special value has digits");
            }
            return Ok(match sign {
                0xC000 => NumericOut::Nan,
                0xD000 => NumericOut::PosInf,
                _ => NumericOut::NegInf,
            });
        }
        0x0000 | 0x4000 => {}
        _ => return Err("the sign is not known"),
    }
    if dscale > NUMERIC_MAX_DSCALE {
        return Err("the scale is too large");
    }
    let mut groups = Vec::with_capacity(ndigits);
    for chunk in raw[8..].as_chunks::<2>().0 {
        let g = usize::from(u16::from_be_bytes(*chunk));
        if g > 9999 {
            return Err("a digit group is above 9999");
        }
        groups.push(g);
    }
    if ndigits > 0 && (weight < -(dscale as i64 / 4 + ndigits as i64 + 1) || weight > 32_767) {
        return Err("the weight is out of range");
    }
    // The group that has the weight `w` is `groups[weight - w]`.
    let group_at = |k: i64| -> usize { usize::try_from(k).ok().and_then(|k| groups.get(k)).copied().unwrap_or(0) };
    let mut text = String::new();
    let mut nonzero = false;
    // Integer part.
    let mut int = String::new();
    if weight >= 0 {
        for k in 0..=weight {
            let g = group_at(k);
            nonzero |= g != 0;
            let _ = write!(int, "{g:04}");
        }
    }
    let trimmed = int.trim_start_matches('0');
    if trimmed.is_empty() {
        int = "0".to_owned();
    } else {
        int = trimmed.to_owned();
    }
    text.push_str(&int);
    // Fraction: `dscale` digits, cut (never rounded) or padded with zeros.
    if dscale > 0 {
        text.push('.');
        let mut digit_pos = 0usize;
        let mut g = 0i64;
        while digit_pos < dscale {
            let group = group_at(weight + 1 + g);
            let four = format!("{group:04}");
            for ch in four.chars() {
                if digit_pos == dscale {
                    break;
                }
                nonzero |= ch != '0';
                text.push(ch);
                digit_pos += 1;
            }
            g += 1;
        }
    }
    if sign == 0x4000 && nonzero {
        text.insert(0, '-');
    }
    Ok(NumericOut::Finite(text))
}

/// Prints an `inet` or `cidr` as PostgreSQL does. `inet` leaves out `/32` and `/128`.
fn parse_inet(raw: &[u8], cidr: bool) -> Result<String, &'static str> {
    if raw.len() < 4 {
        return Err("the header is too short");
    }
    let (family, bits, nb) = (raw[0], usize::from(raw[1]), usize::from(raw[3]));
    let addr = &raw[4..];
    if addr.len() != nb {
        return Err("the length does not match the address");
    }
    let (max_bits, text) = match (family, nb) {
        (2, 4) => (32, format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])),
        (3, 16) => (128, ipv6_text(addr)),
        _ => return Err("the family or the address size is not known"),
    };
    if bits > max_bits {
        return Err("the mask is too large");
    }
    if cidr || bits != max_bits { Ok(format!("{text}/{bits}")) } else { Ok(text) }
}

/// Prints an IPv6 address with the algorithm of PostgreSQL (`inet_net_ntop_ipv6`): the longest
/// run of zero words becomes `::` (the first run when two have the same length).
fn ipv6_text(src: &[u8]) -> String {
    let mut words = [0u16; 8];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u16::from_be_bytes([src[2 * i], src[2 * i + 1]]);
    }
    // The longest run of zero words; the first one wins a tie.
    let (mut best_base, mut best_len) = (None::<usize>, 0usize);
    let mut i = 0;
    while i < 8 {
        if words[i] != 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < 8 && words[i] == 0 {
            i += 1;
        }
        if best_base.is_none() || i - start > best_len {
            best_base = Some(start);
            best_len = i - start;
        }
    }
    if best_len < 2 {
        best_base = None;
    }
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if let Some(b) = best_base
            && i >= b
            && i < b + best_len
        {
            if i == b {
                out.push(':');
            }
            continue;
        }
        if i != 0 {
            out.push(':');
        }
        if i == 6 && best_base == Some(0) && (best_len == 6 || (best_len == 5 && words[5] == 0xffff)) {
            let _ = write!(out, "{}.{}.{}.{}", src[12], src[13], src[14], src[15]);
            return out;
        }
        let _ = write!(out, "{w:x}");
    }
    if let Some(b) = best_base
        && b + best_len == 8
    {
        out.push(':');
    }
    out
}

/// Reads the binary array of PostgreSQL and writes its compact JSON text (the array rule).
fn decode_array(ty: &ColumnType, raw: &[u8]) -> Result<String, DecodeError> {
    let fail = |rule: &str| DecodeError::new(&ty.name, rule);
    if raw.len() < 12 {
        return Err(fail("the header is too short"));
    }
    let ndim = i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let ndim = usize::try_from(ndim).map_err(|_| fail("the number of dimensions is negative"))?;
    if ndim > MAX_DIMS {
        return Err(fail("the number of dimensions is above 6"));
    }
    if ndim == 0 {
        return if raw.len() == 12 { Ok("[]".to_owned()) } else { Err(fail("an empty array has data")) };
    }
    let head = 12 + 8 * ndim;
    if raw.len() < head {
        return Err(fail("the dimensions are cut"));
    }
    let mut dims = Vec::with_capacity(ndim);
    let mut total: usize = 1;
    for d in 0..ndim {
        let o = 12 + 8 * d;
        let len = i32::from_be_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
        let len = usize::try_from(len).map_err(|_| fail("a dimension is negative"))?;
        total = total.checked_mul(len).ok_or_else(|| fail("the number of elements is too large"))?;
        dims.push(len);
    }
    let body = &raw[head..];
    // Each element has a length word of 4 bytes at least, so a bad count cannot make a large loop.
    if total > body.len() / 4 {
        return Err(fail("the number of elements does not fit the data"));
    }
    if total == 0 {
        return if body.is_empty() { Ok("[]".to_owned()) } else { Err(fail("an empty array has data")) };
    }
    let mut out = String::with_capacity(body.len() + total);
    let mut pos = 0usize;
    write_dim(&mut out, &dims, ty, body, &mut pos).map_err(fail)?;
    if pos != body.len() {
        return Err(fail("the data is longer than the elements"));
    }
    Ok(out)
}

fn write_dim(
    out: &mut String,
    dims: &[usize],
    ty: &ColumnType,
    body: &[u8],
    pos: &mut usize,
) -> Result<(), &'static str> {
    out.push('[');
    for i in 0..dims[0] {
        if i > 0 {
            out.push(',');
        }
        if dims.len() > 1 {
            write_dim(out, &dims[1..], ty, body, pos)?;
        } else {
            let head = body.get(*pos..*pos + 4).ok_or("an element is cut")?;
            *pos += 4;
            let len = i32::from_be_bytes([head[0], head[1], head[2], head[3]]);
            if len == -1 {
                out.push_str("null");
                continue;
            }
            let len = usize::try_from(len).map_err(|_| "an element length is negative")?;
            let end = pos.checked_add(len).ok_or("an element length is too large")?;
            let data = body.get(*pos..end).ok_or("an element is cut")?;
            *pos = end;
            parse_scalar(ty.scalar, data)?.write_element(out);
        }
    }
    out.push(']');
    Ok(())
}

/// The raw bytes of one column of a row, in the binary form.
///
/// It accepts each type, so [`raw_values`] can read the bytes before [`decode`] checks the type.
struct Raw<'a>(&'a [u8]);

impl<'a> FromSql<'a> for Raw<'a> {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Raw<'a>, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Raw(raw))
    }

    fn accepts(_: &Type) -> bool {
        true
    }
}

/// Gives the raw bytes of each column of a `tokio-postgres` row. A NULL is `None`.
///
/// # Errors
///
/// The row has a column that it cannot read. The text names the column number only.
pub(crate) fn raw_values(row: &tokio_postgres::Row) -> Result<Vec<Option<&[u8]>>, DecodeError> {
    (0..row.len())
        .map(|i| {
            row.try_get::<_, Option<Raw<'_>>>(i)
                .map(|v| v.map(|r| r.0))
                .map_err(|_| DecodeError { message: format!("The column {} of a row cannot be read.", i + 1) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ct(ty: &Type) -> ColumnType {
        ColumnType::from_pg(ty).expect("listed type")
    }

    fn scalar(ty: &Type, raw: &[u8]) -> Value {
        decode(&ct(ty), Some(raw)).expect("decodes")
    }

    fn numeric(ndigits: u16, weight: i16, sign: u16, dscale: u16, groups: &[u16]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&ndigits.to_be_bytes());
        v.extend_from_slice(&weight.to_be_bytes());
        v.extend_from_slice(&sign.to_be_bytes());
        v.extend_from_slice(&dscale.to_be_bytes());
        for g in groups {
            v.extend_from_slice(&g.to_be_bytes());
        }
        v
    }

    fn text(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    fn number(s: &str) -> Value {
        Value::Number(s.to_owned())
    }

    #[test]
    fn closed_list_and_names() {
        for (ty, name) in [
            (Type::BOOL, "bool"),
            (Type::INT2, "int2"),
            (Type::INT4, "int4"),
            (Type::OID, "oid"),
            (Type::INT8, "int8"),
            (Type::FLOAT4, "float4"),
            (Type::FLOAT8, "float8"),
            (Type::NUMERIC, "numeric"),
            (Type::TEXT, "text"),
            (Type::VARCHAR, "varchar"),
            (Type::BPCHAR, "bpchar"),
            (Type::NAME, "name"),
            (Type::UUID, "uuid"),
            (Type::JSON, "json"),
            (Type::JSONB, "jsonb"),
            (Type::INET, "inet"),
            (Type::CIDR, "cidr"),
            (Type::DATE, "date"),
            (Type::TIMESTAMP, "timestamp"),
            (Type::TIMESTAMPTZ, "timestamptz"),
            (Type::TIME, "time"),
        ] {
            let c = ct(&ty);
            assert_eq!(c.name, name);
            assert!(!c.array);
        }
        let a = ct(&Type::INT4_ARRAY);
        assert_eq!((a.name.as_str(), a.array), ("int4[]", true));
        assert_eq!(ct(&Type::TEXT_ARRAY).name, "text[]");
        assert_eq!(ct(&Type::TIMESTAMPTZ_ARRAY).scalar, Scalar::Timestamptz);
    }

    #[test]
    fn unsupported_types_are_refused() {
        for ty in [
            Type::BYTEA,
            Type::INTERVAL,
            Type::MONEY,
            Type::XML,
            Type::INT4_RANGE,
            Type::POINT,
            Type::TIMETZ,
            Type::CHAR,
            Type::JSON_ARRAY,
            Type::JSONB_ARRAY,
            Type::BYTEA_ARRAY,
            Type::TS_VECTOR,
            Type::VOID,
        ] {
            let err = ColumnType::from_pg(&ty).unwrap_err();
            assert!(!err.name.is_empty());
        }
        assert_eq!(ColumnType::from_pg(&Type::BYTEA).unwrap_err().name, "bytea");
        assert_eq!(ColumnType::from_pg(&Type::JSONB_ARRAY).unwrap_err().name, "jsonb[]");
    }

    #[test]
    fn user_types() {
        let color = Type::new("color".into(), 20_000, Kind::Enum(vec!["red".into(), "blue".into()]), "public".into());
        let c = ct(&color);
        assert_eq!((c.name.as_str(), c.scalar, c.array), ("color", Scalar::Enum, false));
        assert_eq!(decode(&c, Some(b"red")).unwrap(), text("red"));
        // A domain has the kind of its base type and keeps its own name.
        let dom = Type::new("money_t".into(), 20_001, Kind::Domain(Type::NUMERIC), "public".into());
        let c = ct(&dom);
        assert_eq!((c.name.as_str(), c.scalar), ("money_t", Scalar::Numeric));
        let dom2 = Type::new("d2".into(), 20_002, Kind::Domain(dom.clone()), "public".into());
        assert_eq!(ct(&dom2).scalar, Scalar::Numeric);
        let bad = Type::new("raw_t".into(), 20_003, Kind::Domain(Type::BYTEA), "public".into());
        assert!(ColumnType::from_pg(&bad).is_err());
        // An array of an enum and an array domain.
        let arr = Type::new("_color".into(), 20_004, Kind::Array(color.clone()), "public".into());
        let c = ct(&arr);
        assert_eq!((c.name.as_str(), c.scalar, c.array), ("color[]", Scalar::Enum, true));
        let darr = Type::new("ints".into(), 20_005, Kind::Domain(Type::INT4_ARRAY), "public".into());
        assert!(ct(&darr).array);
        // A composite and a user type with a built-in name are refused.
        let comp = Type::new("pair".into(), 20_006, Kind::Composite(vec![]), "public".into());
        assert!(ColumnType::from_pg(&comp).is_err());
        let citext = Type::new("citext".into(), 20_007, Kind::Simple, "public".into());
        assert!(ColumnType::from_pg(&citext).is_err());
        let fake_text = Type::new("text".into(), 20_008, Kind::Simple, "evil".into());
        assert!(ColumnType::from_pg(&fake_text).is_err());
    }

    #[test]
    fn integers_and_bool() {
        assert_eq!(scalar(&Type::BOOL, &[1]), Value::Bool(true));
        assert_eq!(scalar(&Type::BOOL, &[0]), Value::Bool(false));
        assert!(decode(&ct(&Type::BOOL), Some(&[2])).is_err());
        assert_eq!(scalar(&Type::INT2, &(-5i16).to_be_bytes()), number("-5"));
        assert_eq!(scalar(&Type::INT4, &i32::MIN.to_be_bytes()), number("-2147483648"));
        assert_eq!(scalar(&Type::OID, &u32::MAX.to_be_bytes()), number("4294967295"));
        assert_eq!(scalar(&Type::INT8, &i64::MAX.to_be_bytes()), text("9223372036854775807"));
        assert_eq!(scalar(&Type::INT8, &i64::MIN.to_be_bytes()), text("-9223372036854775808"));
        assert!(decode(&ct(&Type::INT4), Some(&[0, 0, 1])).is_err());
        assert_eq!(decode(&ct(&Type::INT4), None).unwrap(), Value::Null);
    }

    #[test]
    fn floats() {
        assert_eq!(scalar(&Type::FLOAT8, &1.5f64.to_be_bytes()), number("1.5"));
        assert_eq!(scalar(&Type::FLOAT8, &1.0f64.to_be_bytes()), number("1.0"));
        assert_eq!(scalar(&Type::FLOAT4, &0.1f32.to_be_bytes()), number("0.1"));
        assert_eq!(scalar(&Type::FLOAT8, &f64::NAN.to_be_bytes()), text("nan"));
        assert_eq!(scalar(&Type::FLOAT8, &f64::INFINITY.to_be_bytes()), text("inf"));
        assert_eq!(scalar(&Type::FLOAT4, &f32::NEG_INFINITY.to_be_bytes()), text("-inf"));
        assert_eq!(scalar(&Type::FLOAT8, &(-0.0f64).to_be_bytes()), number("-0.0"));
        assert_eq!(scalar(&Type::FLOAT8, &1e300f64.to_be_bytes()), number("1e+300"));
    }

    #[test]
    fn numerics() {
        // 12.5 with dscale 9: 12.500000000.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(2, 0, 0, 9, &[12, 5000])), text("12.500000000"));
        assert_eq!(scalar(&Type::NUMERIC, &numeric(2, 0, 0, 2, &[12, 5000])), text("12.50"));
        assert_eq!(scalar(&Type::NUMERIC, &numeric(2, 0, 0x4000, 2, &[12, 5000])), text("-12.50"));
        // An integer above 10^8, and a weight above the digits (trailing zero groups are not stored).
        assert_eq!(scalar(&Type::NUMERIC, &numeric(1, 2, 0, 0, &[7])), text("700000000"));
        assert_eq!(scalar(&Type::NUMERIC, &numeric(3, 2, 0, 0, &[1, 2345, 6789])), text("123456789"));
        // 0.00001: weight -2.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(1, -2, 0, 5, &[1000])), text("0.00001"));
        // Zero, with and without scale.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0, 0, &[])), text("0"));
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0, 3, &[])), text("0.000"));
        // A negative zero has no sign.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0x4000, 1, &[])), text("0.0"));
        // A fraction of 0.5 with dscale 1, a leading zero group is trimmed.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(1, -1, 0, 4, &[5000])), text("0.5000"));
        // NaN and the infinities give null.
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0xC000, 0, &[])), Value::Null);
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0xD000, 0, &[])), Value::Null);
        assert_eq!(scalar(&Type::NUMERIC, &numeric(0, 0, 0xF000, 0, &[])), Value::Null);
    }

    #[test]
    fn numeric_faults() {
        let ty = ct(&Type::NUMERIC);
        for raw in [
            numeric(1, 0, 0, 0, &[]),        // short body
            numeric(1, 0, 0, 0, &[10_000]),  // group above 9999
            numeric(1, 0, 0x1234, 0, &[1]),  // unknown sign
            numeric(1, 0, 0, 16_384, &[1]),  // dscale too large
            numeric(1, 0, 0xC000, 0, &[1]),  // NaN with digits
            numeric(1, -30_000, 0, 2, &[1]), // weight far below the scale
            vec![0, 0, 0],                   // short header
        ] {
            let err = decode(&ty, Some(&raw)).unwrap_err();
            assert!(err.message().starts_with("A value of the type numeric does not decode"));
        }
    }

    #[test]
    fn texts_uuid_json() {
        assert_eq!(scalar(&Type::TEXT, "héllo".as_bytes()), text("héllo"));
        assert_eq!(scalar(&Type::BPCHAR, b"ab   "), text("ab   "));
        assert_eq!(scalar(&Type::NAME, b"n"), text("n"));
        assert!(decode(&ct(&Type::TEXT), Some(&[0xff, 0xfe])).is_err());
        let u = [0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89];
        assert_eq!(scalar(&Type::UUID, &u), text("abcdef01-2345-6789-abcd-ef0123456789"));
        assert!(decode(&ct(&Type::UUID), Some(&u[..15])).is_err());
        assert_eq!(scalar(&Type::JSON, br#"{"a" :  1}"#), text(r#"{"a" :  1}"#));
        let mut jb = vec![1u8];
        jb.extend_from_slice(br#"{"a": 1}"#);
        assert_eq!(scalar(&Type::JSONB, &jb), text(r#"{"a": 1}"#));
        jb[0] = 2;
        assert!(decode(&ct(&Type::JSONB), Some(&jb)).is_err());
    }

    #[test]
    fn inet_and_cidr() {
        let v4 = |bits: u8, a: [u8; 4]| [&[2, bits, 0, 4][..], &a].concat();
        assert_eq!(scalar(&Type::INET, &v4(32, [10, 0, 0, 1])), text("10.0.0.1"));
        assert_eq!(scalar(&Type::INET, &v4(24, [10, 0, 0, 1])), text("10.0.0.1/24"));
        assert_eq!(scalar(&Type::CIDR, &[&[2, 8, 1, 4][..], &[10, 0, 0, 0]].concat()), text("10.0.0.0/8"));
        assert_eq!(scalar(&Type::CIDR, &[&[2, 32, 1, 4][..], &[10, 0, 0, 1]].concat()), text("10.0.0.1/32"));
        let v6 = |bits: u8, words: [u16; 8]| {
            let mut v = vec![3, bits, 0, 16];
            for w in words {
                v.extend_from_slice(&w.to_be_bytes());
            }
            v
        };
        assert_eq!(scalar(&Type::INET, &v6(128, [0, 0, 0, 0, 0, 0, 0, 1])), text("::1"));
        assert_eq!(scalar(&Type::INET, &v6(128, [0; 8])), text("::"));
        assert_eq!(scalar(&Type::INET, &v6(64, [0x2001, 0xdb8, 0, 0, 0, 0, 0, 1])), text("2001:db8::1/64"));
        assert_eq!(scalar(&Type::INET, &v6(128, [0x2001, 0xdb8, 0, 1, 0, 0, 0, 1])), text("2001:db8:0:1::1"));
        assert_eq!(scalar(&Type::INET, &v6(128, [0x2001, 0, 1, 0, 1, 0, 1, 0])), text("2001:0:1:0:1:0:1:0"));
        assert_eq!(scalar(&Type::INET, &v6(128, [1, 0, 0, 0, 0, 0, 0, 0])), text("1::"));
        assert_eq!(scalar(&Type::INET, &v6(128, [0, 0, 0, 0, 0, 0xffff, 0x0102, 0x0304])), text("::ffff:1.2.3.4"));
        assert_eq!(scalar(&Type::INET, &v6(128, [1, 2, 3, 4, 5, 6, 7, 8])), text("1:2:3:4:5:6:7:8"));
        assert!(decode(&ct(&Type::INET), Some(&v4(33, [1, 2, 3, 4]))).is_err());
        assert!(decode(&ct(&Type::INET), Some(&[2, 32, 0, 4, 1, 2])).is_err());
        assert!(decode(&ct(&Type::INET), Some(&[9, 32, 0, 4, 1, 2, 3, 4])).is_err());
    }

    #[test]
    fn dates_and_times() {
        // 2026-11-09 is 9,809 days after 2000-01-01.
        let days = (1_794_182_400i64 / 86_400 - DAYS_1970_TO_2000) as i32;
        assert_eq!(scalar(&Type::DATE, &days.to_be_bytes()), number("1794182400"));
        assert_eq!(scalar(&Type::DATE, &0i32.to_be_bytes()), number("946684800"));
        assert_eq!(scalar(&Type::DATE, &(-1i32).to_be_bytes()), number("946598400"));
        assert_eq!(scalar(&Type::DATE, &i32::MAX.to_be_bytes()), Value::Null);
        assert_eq!(scalar(&Type::DATE, &i32::MIN.to_be_bytes()), Value::Null);
        // 2026-11-09 08:00:00.125+00 is 1794211200.125.
        let us = (1_794_211_200_125_000i64) - DAYS_1970_TO_2000 * US_PER_DAY;
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &us.to_be_bytes()), number("1794211200.125"));
        assert_eq!(scalar(&Type::TIMESTAMP, &us.to_be_bytes()), number("1794211200.125"));
        let whole = (1_794_211_200_000_000i64) - DAYS_1970_TO_2000 * US_PER_DAY;
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &whole.to_be_bytes()), number("1794211200"));
        let micro = whole + 1;
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &micro.to_be_bytes()), number("1794211200.000001"));
        // Before 1970, with a fraction.
        let before = -500_000i64 - DAYS_1970_TO_2000 * US_PER_DAY;
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &before.to_be_bytes()), number("-0.5"));
        let before = -1_500_000i64 - DAYS_1970_TO_2000 * US_PER_DAY;
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &before.to_be_bytes()), number("-1.5"));
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &i64::MAX.to_be_bytes()), Value::Null);
        assert_eq!(scalar(&Type::TIMESTAMPTZ, &i64::MIN.to_be_bytes()), Value::Null);
        assert_eq!(scalar(&Type::TIMESTAMP, &i64::MAX.to_be_bytes()), Value::Null);
        // Near the end of the range: no overflow, an error.
        assert!(decode(&ct(&Type::TIMESTAMPTZ), Some(&(i64::MAX - 1).to_be_bytes())).is_err());
        // time
        assert_eq!(scalar(&Type::TIME, &0i64.to_be_bytes()), text("00:00:00"));
        assert_eq!(
            scalar(&Type::TIME, &(13 * 3600 * US_PER_SEC + 5 * 60 * US_PER_SEC + 7_500_000).to_be_bytes()),
            text("13:05:07.5")
        );
        assert_eq!(scalar(&Type::TIME, &US_PER_DAY.to_be_bytes()), text("24:00:00"));
        assert!(decode(&ct(&Type::TIME), Some(&(US_PER_DAY + 1).to_be_bytes())).is_err());
        assert!(decode(&ct(&Type::TIME), Some(&(-1i64).to_be_bytes())).is_err());
    }

    /// Builds the wire form of a one-dimensional array.
    fn array(elem_oid: u32, items: &[Option<Vec<u8>>]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1i32.to_be_bytes());
        v.extend_from_slice(&i32::from(items.iter().any(Option::is_none)).to_be_bytes());
        v.extend_from_slice(&elem_oid.to_be_bytes());
        v.extend_from_slice(&(items.len() as i32).to_be_bytes());
        v.extend_from_slice(&1i32.to_be_bytes());
        for it in items {
            match it {
                None => v.extend_from_slice(&(-1i32).to_be_bytes()),
                Some(b) => {
                    v.extend_from_slice(&(b.len() as i32).to_be_bytes());
                    v.extend_from_slice(b);
                }
            }
        }
        v
    }

    #[test]
    fn arrays() {
        let c = ct(&Type::INT4_ARRAY);
        let raw = array(23, &[Some(1i32.to_be_bytes().to_vec()), None, Some((-3i32).to_be_bytes().to_vec())]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text("[1,null,-3]"));
        // int8 and numeric elements are strings.
        let c = ct(&Type::INT8_ARRAY);
        let raw = array(20, &[Some(i64::MAX.to_be_bytes().to_vec())]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["9223372036854775807"]"#));
        let c = ct(&Type::NUMERIC_ARRAY);
        let raw = array(1700, &[Some(numeric(2, 0, 0, 2, &[12, 5000])), Some(numeric(0, 0, 0xC000, 0, &[]))]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["12.50","NaN"]"#));
        // float elements: numbers, and the strings for the special values.
        let c = ct(&Type::FLOAT8_ARRAY);
        let raw = array(
            701,
            &[
                Some(2.5f64.to_be_bytes().to_vec()),
                Some(f64::NAN.to_be_bytes().to_vec()),
                Some(f64::INFINITY.to_be_bytes().to_vec()),
                Some(f64::NEG_INFINITY.to_be_bytes().to_vec()),
            ],
        );
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"[2.5,"NaN","Infinity","-Infinity"]"#));
        // bool, text with escapes
        let c = ct(&Type::BOOL_ARRAY);
        assert_eq!(decode(&c, Some(&array(16, &[Some(vec![1]), Some(vec![0])]))).unwrap(), text("[true,false]"));
        let c = ct(&Type::TEXT_ARRAY);
        let raw = array(25, &[Some(b"a\"b\\c\n".to_vec()), None]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["a\"b\\c\n",null]"#));
        // date, timestamptz
        let c = ct(&Type::DATE_ARRAY);
        let d = ((1_794_182_400i64 / 86_400 - DAYS_1970_TO_2000) as i32).to_be_bytes().to_vec();
        let raw = array(1082, &[Some(d), Some(i32::MAX.to_be_bytes().to_vec())]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["2026-11-09",null]"#));
        let c = ct(&Type::TIMESTAMPTZ_ARRAY);
        let us = (1_794_211_200_125_000i64 - DAYS_1970_TO_2000 * US_PER_DAY).to_be_bytes().to_vec();
        let whole = (1_794_211_200_000_000i64 - DAYS_1970_TO_2000 * US_PER_DAY).to_be_bytes().to_vec();
        let raw = array(1184, &[Some(us), Some(whole)]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["2026-11-09T08:00:00.125Z","2026-11-09T08:00:00Z"]"#));
        // time, uuid
        let c = ct(&Type::TIME_ARRAY);
        let raw = array(1083, &[Some(3_600_000_000i64.to_be_bytes().to_vec())]);
        assert_eq!(decode(&c, Some(&raw)).unwrap(), text(r#"["01:00:00"]"#));
    }

    #[test]
    fn arrays_dimensions_and_faults() {
        let c = ct(&Type::INT4_ARRAY);
        // Empty array.
        let mut empty = Vec::new();
        empty.extend_from_slice(&0i32.to_be_bytes());
        empty.extend_from_slice(&0i32.to_be_bytes());
        empty.extend_from_slice(&23u32.to_be_bytes());
        assert_eq!(decode(&c, Some(&empty)).unwrap(), text("[]"));
        // Two dimensions: 2 x 2.
        let mut v = Vec::new();
        v.extend_from_slice(&2i32.to_be_bytes());
        v.extend_from_slice(&0i32.to_be_bytes());
        v.extend_from_slice(&23u32.to_be_bytes());
        for (len, lb) in [(2i32, 1i32), (2, 1)] {
            v.extend_from_slice(&len.to_be_bytes());
            v.extend_from_slice(&lb.to_be_bytes());
        }
        for i in 1..=4i32 {
            v.extend_from_slice(&4i32.to_be_bytes());
            v.extend_from_slice(&i.to_be_bytes());
        }
        assert_eq!(decode(&c, Some(&v)).unwrap(), text("[[1,2],[3,4]]"));
        // Cut data, extra data, bad counts.
        let cut = &v[..v.len() - 1];
        assert!(decode(&c, Some(cut)).is_err());
        let mut extra = v.clone();
        extra.push(0);
        assert!(decode(&c, Some(&extra)).is_err());
        let mut huge = v.clone();
        huge[12..16].copy_from_slice(&i32::MAX.to_be_bytes());
        huge[20..24].copy_from_slice(&i32::MAX.to_be_bytes());
        assert!(decode(&c, Some(&huge)).is_err());
        let mut neg = v.clone();
        neg[0..4].copy_from_slice(&(-1i32).to_be_bytes());
        assert!(decode(&c, Some(&neg)).is_err());
        let mut seven = v.clone();
        seven[0..4].copy_from_slice(&7i32.to_be_bytes());
        assert!(decode(&c, Some(&seven)).is_err());
        assert!(decode(&c, Some(&[0u8; 5])).is_err());
        // An element with a bad length.
        let raw = array(23, &[Some(vec![1, 2, 3])]);
        assert!(decode(&c, Some(&raw)).is_err());
        // A NULL array.
        assert_eq!(decode(&c, None).unwrap(), Value::Null);
    }

    #[test]
    fn json_len_equals_the_writer() {
        for v in [
            Value::Null,
            Value::Bool(true),
            Value::Bool(false),
            Value::Number("12.5".into()),
            text(""),
            text("plain"),
            text("quote\" back\\ nl\n cr\r tab\t bell\u{7} é 漢 😀"),
        ] {
            let mut s = String::new();
            v.write_json(&mut s);
            assert_eq!(v.json_len(), s.len(), "{v:?}");
        }
    }

    #[test]
    fn errors_hold_no_value() {
        let err = decode(&ct(&Type::INT4), Some(&[9, 9, 9])).unwrap_err();
        assert_eq!(err.message(), "A value of the type int4 does not decode: the length is wrong.");
    }
}

#[cfg(test)]
#[path = "decode/tests/container_tests.rs"]
mod container_tests;
