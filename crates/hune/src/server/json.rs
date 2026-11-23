//! The check of a JSON request body (6.2).
//!
//! `serde_json` accepts a duplicate member and a deep value without a report. This module adds
//! a scan for the duplicate members and for the depth limit of 64. It also checks a closed schema
//! and gives the JSON pointers of 4.37.

use super::error::ApiError;
use super::limits::MAX_BODY_JSON_DEPTH;
use hune_core::error::{ErrorCode, ValidationError, pointer_token};
use hune_core::limits::MAX_ERRORS;
use serde_json::Value;
use std::collections::HashSet;
use std::ops::Range;

/// The result of the scan of a body.
#[derive(Debug, Default)]
pub struct Scan {
    /// The faults that the scan found: duplicate members and a depth above the limit.
    pub errors: Vec<ValidationError>,
    /// The byte range of each member of the top object, in input order.
    pub members: Vec<(String, Range<usize>)>,
}

struct Frame {
    object: bool,
    seen: HashSet<String>,
    key: Option<String>,
    index: usize,
    expect_key: bool,
}

fn pointer_of(stack: &[Frame], upto: usize) -> String {
    let mut out = String::new();
    for f in &stack[..upto] {
        out.push('/');
        if f.object {
            out.push_str(&pointer_token(f.key.as_deref().unwrap_or("")));
        } else {
            out.push_str(&f.index.to_string());
        }
    }
    out
}

fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn push_error(errors: &mut Vec<ValidationError>, e: ValidationError) -> bool {
    if errors.last().is_some_and(|l| l.code == ErrorCode::TooManyErrors) {
        return false;
    }
    if errors.len() + 1 >= MAX_ERRORS {
        errors.push(ValidationError::new("", ErrorCode::TooManyErrors, "Too many errors; the list stops here."));
        return false;
    }
    errors.push(e);
    true
}

/// Scans a body that is valid JSON. The members named in `raw_members` are members of the top
/// object that other rules check (for example `document`): the scan does not look inside them.
pub fn scan(bytes: &[u8], raw_members: &[&str]) -> Scan {
    let mut out = Scan::default();
    let mut stack: Vec<Frame> = Vec::new();
    let mut skip_len: Option<usize> = None;
    // The start of the value of the current member of the top object.
    let mut member_start: Option<usize> = None;
    let mut i = 0;
    let n = bytes.len();

    // Marks the start of a value when the value is a member of the top object.
    macro_rules! value_start {
        ($pos:expr) => {
            if stack.len() == 1 && stack[0].object && !stack[0].expect_key {
                member_start = Some($pos);
            }
        };
    }
    // Records the end of a value when the value is a member of the top object.
    macro_rules! value_end {
        ($pos:expr) => {
            if stack.len() == 1 && stack[0].object {
                if let (Some(start), Some(key)) = (member_start.take(), stack[0].key.clone()) {
                    out.members.push((key, start..$pos));
                }
            }
        };
    }

    while i < n {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' | b':' => i += 1,
            b',' => {
                if let Some(f) = stack.last_mut() {
                    if f.object {
                        f.expect_key = true;
                    } else {
                        f.index += 1;
                    }
                }
                i += 1;
            }
            b'{' | b'[' => {
                value_start!(i);
                let skipping = skip_len.is_some();
                if !skipping
                    && stack.len() == 1
                    && stack[0].object
                    && let Some(k) = &stack[0].key
                    && raw_members.contains(&k.as_str())
                {
                    skip_len = Some(stack.len());
                }
                stack.push(Frame {
                    object: c == b'{',
                    seen: HashSet::new(),
                    key: None,
                    index: 0,
                    expect_key: c == b'{',
                });
                if skip_len.is_none() && stack.len() > MAX_BODY_JSON_DEPTH {
                    push_error(
                        &mut out.errors,
                        ValidationError::new(
                            pointer_of(&stack, stack.len() - 1),
                            ErrorCode::TooDeep,
                            format!("The depth of the body is above {MAX_BODY_JSON_DEPTH}."),
                        ),
                    );
                    return out;
                }
                i += 1;
            }
            b'}' | b']' => {
                stack.pop();
                i += 1;
                if skip_len == Some(stack.len()) {
                    skip_len = None;
                }
                value_end!(i);
            }
            b'"' => {
                let end = string_end(bytes, i);
                let is_key = stack.last().is_some_and(|f| f.object && f.expect_key);
                if is_key {
                    if skip_len.is_none() {
                        let key = std::str::from_utf8(&bytes[i..end])
                            .ok()
                            .and_then(|s| serde_json::from_str::<String>(s).ok())
                            .unwrap_or_default();
                        let depth = stack.len();
                        let duplicate = {
                            let f = &mut stack[depth - 1];
                            let dup = !f.seen.insert(key.clone());
                            f.key = Some(key.clone());
                            f.expect_key = false;
                            dup
                        };
                        if duplicate {
                            let pointer = format!("{}/{}", pointer_of(&stack, depth - 1), pointer_token(&key));
                            let ok = push_error(
                                &mut out.errors,
                                ValidationError::new(
                                    pointer,
                                    ErrorCode::DuplicateMember,
                                    format!("The member {key:?} occurs more than one time."),
                                ),
                            );
                            if !ok {
                                return out;
                            }
                        }
                    } else if let Some(f) = stack.last_mut() {
                        f.expect_key = false;
                    }
                } else {
                    value_start!(i);
                    value_end!(end);
                }
                i = end;
            }
            _ => {
                // A number or a literal.
                let start = i;
                while i < n && !matches!(bytes[i], b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n') {
                    i += 1;
                }
                value_start!(start);
                value_end!(i);
            }
        }
    }
    out
}

/// A body that passed the checks.
#[derive(Debug)]
pub struct Body {
    /// The JSON value.
    pub value: Value,
    /// The byte range of each member of the top object, for the members that the caller reads
    /// from their own bytes.
    pub members: Vec<(String, Range<usize>)>,
}

impl Body {
    /// Gives the bytes of one member of the top object, as they are in the request body.
    pub fn member_bytes<'a>(&self, body: &'a [u8], name: &str) -> Option<&'a [u8]> {
        self.members.iter().find(|(k, _)| k == name).map(|(_, r)| &body[r.clone()])
    }
}

fn bad(errors: Vec<ValidationError>) -> ApiError {
    ApiError::bad_request_errors("The body does not follow the schema.", &errors)
}

/// Parses a body: the syntax, the duplicate members and the depth (6.2). The members that
/// `raw_members` names are not scanned. A scan fault stops the parse.
pub fn parse_body(bytes: &[u8], raw_members: &[&str]) -> Result<Body, ApiError> {
    let (body, scan_errors) = parse_scanned(bytes, raw_members)?;
    if scan_errors.is_empty() { Ok(body) } else { Err(bad(scan_errors)) }
}

/// Parses a body and gives the faults of the scan with it, so that the caller can list them with
/// the faults of the schema.
fn parse_scanned(bytes: &[u8], raw_members: &[&str]) -> Result<(Body, Vec<ValidationError>), ApiError> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            return Err(bad(vec![ValidationError::new(
                "",
                ErrorCode::Encoding,
                format!("The body is not UTF-8: invalid byte at offset {}.", e.valid_up_to()),
            )]));
        }
    };
    let parsed = if raw_members.is_empty() { serde_json::from_str(text) } else { parse_top(text, raw_members) };
    let value: Value = match parsed {
        Ok(v) => v,
        Err(e) => {
            let message = e.to_string();
            let error = if message.starts_with("recursion limit exceeded") {
                ValidationError::new(
                    "",
                    ErrorCode::TooDeep,
                    format!("The depth of the body is above {MAX_BODY_JSON_DEPTH}."),
                )
            } else {
                let line = u32::try_from(e.line()).unwrap_or(u32::MAX).max(1);
                let column = u32::try_from(e.column()).unwrap_or(u32::MAX).max(1);
                ValidationError::at(ErrorCode::Syntax, format!("The body is not valid JSON: {message}."), line, column)
            };
            return Err(bad(vec![error]));
        }
    };
    let scanned = scan(bytes, raw_members);
    Ok((Body { value, members: scanned.members }, scanned.errors))
}

/// Parses a body whose members of the top object that `raw` names stay bytes: the parse checks their syntax
/// and builds no tree of them. Each of them becomes `null` in the value. The caller reads their bytes with
/// `Body::member_bytes` (6.2). Thus the document of a create or of a save stays a range of the body until the
/// decision (11.20).
fn parse_top(text: &str, raw: &[&str]) -> Result<Value, serde_json::Error> {
    use serde::de::DeserializeSeed;
    let mut de = serde_json::Deserializer::from_str(text);
    let value = TopSeed { raw }.deserialize(&mut de)?;
    de.end()?;
    Ok(value)
}

/// The top value of a body, with the members that `raw` names as `null`.
struct TopSeed<'r> {
    raw: &'r [&'r str],
}

impl<'de> serde::de::DeserializeSeed<'de> for TopSeed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for TopSeed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if self.raw.contains(&key.as_str()) {
                map.next_value::<serde::de::IgnoredAny>()?;
                out.insert(key, Value::Null);
            } else {
                let v: Value = map.next_value()?;
                out.insert(key, v);
            }
        }
        Ok(Value::Object(out))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element::<Value>()? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(Value::from(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
}

/// The expected shape of a JSON value.
#[derive(Debug, Clone, Copy)]
pub enum Shape {
    /// Any JSON value.
    Any,
    /// A boolean.
    Bool,
    /// A whole number in a range.
    Int { min: i64, max: i64 },
    /// A text with a length in characters.
    Str { min_chars: usize, max_chars: usize },
    /// A list.
    Array { item: &'static Shape, max: usize },
    /// An object with a closed list of members.
    Object(&'static [Member]),
    /// An object with free member names and one shape of value.
    Map { value: &'static Shape, max: usize },
    /// `null` or the inner shape.
    Nullable(&'static Shape),
}

/// One member of an object shape.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    /// The member name.
    pub name: &'static str,
    /// The shape of the value.
    pub shape: Shape,
    /// Tells if the member must be present.
    pub required: bool,
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

fn expected_of(shape: &Shape) -> &'static str {
    match shape {
        Shape::Any => "any value",
        Shape::Bool => "a boolean",
        Shape::Int { .. } => "a whole number",
        Shape::Str { .. } => "a text",
        Shape::Array { .. } => "a list",
        Shape::Object(_) | Shape::Map { .. } => "an object",
        Shape::Nullable(inner) => expected_of(inner),
    }
}

/// Checks a value against a shape and gives every fault with its pointer (4.37).
pub fn check_shape(value: &Value, shape: &Shape) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    walk(value, shape, &mut String::new(), &mut errors);
    errors
}

fn walk(value: &Value, shape: &Shape, path: &mut String, errors: &mut Vec<ValidationError>) {
    if errors.len() >= MAX_ERRORS - 1 {
        return;
    }
    let wrong = |path: &str, errors: &mut Vec<ValidationError>| {
        push_error(
            errors,
            ValidationError::new(
                path,
                ErrorCode::WrongType,
                format!("Expected {}, found {}.", expected_of(shape), kind_of(value)),
            ),
        );
    };
    match shape {
        Shape::Any => {}
        Shape::Nullable(inner) => {
            if !value.is_null() {
                walk(value, inner, path, errors);
            }
        }
        Shape::Bool => {
            if !value.is_boolean() {
                wrong(path, errors);
            }
        }
        Shape::Int { min, max } => match value {
            Value::Number(num) => match num.as_i64() {
                Some(v) if v >= *min && v <= *max => {}
                Some(v) => {
                    push_error(
                        errors,
                        ValidationError::new(
                            path.as_str(),
                            ErrorCode::OutOfRange,
                            format!("Expected a whole number from {min} to {max}, found {v}."),
                        ),
                    );
                }
                None => wrong(path, errors),
            },
            _ => wrong(path, errors),
        },
        Shape::Str { min_chars, max_chars } => match value {
            Value::String(s) => {
                let n = s.chars().count();
                if n < *min_chars || n > *max_chars {
                    push_error(
                        errors,
                        ValidationError::new(
                            path.as_str(),
                            ErrorCode::OutOfRange,
                            format!("Expected a text of {min_chars} to {max_chars} characters, found {n}."),
                        ),
                    );
                }
            }
            _ => wrong(path, errors),
        },
        Shape::Array { item, max } => match value {
            Value::Array(items) => {
                if items.len() > *max {
                    push_error(
                        errors,
                        ValidationError::new(
                            path.as_str(),
                            ErrorCode::TooLarge,
                            format!("Expected at most {max} items, found {}.", items.len()),
                        ),
                    );
                    return;
                }
                for (i, v) in items.iter().enumerate() {
                    let len = path.len();
                    path.push('/');
                    path.push_str(&i.to_string());
                    walk(v, item, path, errors);
                    path.truncate(len);
                }
            }
            _ => wrong(path, errors),
        },
        Shape::Map { value: inner, max } => match value {
            Value::Object(map) => {
                if map.len() > *max {
                    push_error(
                        errors,
                        ValidationError::new(
                            path.as_str(),
                            ErrorCode::TooLarge,
                            format!("Expected at most {max} members, found {}.", map.len()),
                        ),
                    );
                    return;
                }
                for (k, v) in map {
                    let len = path.len();
                    path.push('/');
                    path.push_str(&pointer_token(k));
                    walk(v, inner, path, errors);
                    path.truncate(len);
                }
            }
            _ => wrong(path, errors),
        },
        Shape::Object(members) => match value {
            Value::Object(map) => {
                for m in *members {
                    let len = path.len();
                    path.push('/');
                    path.push_str(&pointer_token(m.name));
                    match map.get(m.name) {
                        Some(v) => walk(v, &m.shape, path, errors),
                        None if m.required => {
                            push_error(
                                errors,
                                ValidationError::new(
                                    path.as_str(),
                                    ErrorCode::MissingMember,
                                    format!("The member {} is necessary.", m.name),
                                ),
                            );
                        }
                        None => {}
                    }
                    path.truncate(len);
                }
                for k in map.keys() {
                    if !members.iter().any(|m| m.name == k) {
                        push_error(
                            errors,
                            ValidationError::new(
                                format!("{path}/{}", pointer_token(k)),
                                ErrorCode::UnknownMember,
                                format!("The member {k:?} is not in the schema."),
                            ),
                        );
                    }
                }
            }
            _ => wrong(path, errors),
        },
    }
}

/// Parses a body and checks it against a shape. A fault gives `400 bad_request` with the list
/// of errors (6.2).
pub fn parse_and_check(bytes: &[u8], shape: &Shape, raw_members: &[&str]) -> Result<Body, ApiError> {
    parse_and_check_with(bytes, shape, raw_members, |_| Vec::new())
}

/// Like `parse_and_check`, with the format checks of the members. `formats` reads the parsed value
/// and gives the faults of the members that have the right type but a wrong format. The answer lists
/// the faults of the scan, of the shape and of the formats together, each pointer one time (6.2).
pub fn parse_and_check_with(
    bytes: &[u8],
    shape: &Shape,
    raw_members: &[&str],
    formats: impl FnOnce(&Value) -> Vec<ValidationError>,
) -> Result<Body, ApiError> {
    let (body, mut errors) = parse_scanned(bytes, raw_members)?;
    errors.extend(check_shape(&body.value, shape));
    for fault in formats(&body.value) {
        if !errors.iter().any(|e| e.pointer == fault.pointer) {
            errors.push(fault);
        }
    }
    // The order of the schema, so that the list does not depend on the check that found a fault.
    if let Shape::Object(members) = shape {
        let rank = |pointer: &str| {
            let first = pointer.strip_prefix('/').map(|p| p.split('/').next().unwrap_or(""));
            first.and_then(|token| members.iter().position(|m| pointer_token(m.name) == token)).unwrap_or(usize::MAX)
        };
        errors.sort_by_key(|e| rank(&e.pointer));
    }
    errors.truncate(MAX_ERRORS);
    if errors.is_empty() { Ok(body) } else { Err(bad(errors)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::error::Code;

    fn errors_of(e: &ApiError) -> Vec<(String, String)> {
        e.body()["error"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x["pointer"].as_str().unwrap().to_owned(), x["code"].as_str().unwrap().to_owned()))
            .collect()
    }

    #[test]
    fn duplicate_member_at_top_and_nested() {
        let e = parse_body(br#"{"a":1,"b":{"c":1,"c":2},"a":3}"#, &[]).unwrap_err();
        assert_eq!(e.code, Code::BadRequest);
        let list = errors_of(&e);
        assert!(list.contains(&("/b/c".into(), "duplicate_member".into())), "{list:?}");
        assert!(list.contains(&("/a".into(), "duplicate_member".into())), "{list:?}");
    }

    #[test]
    fn duplicate_with_escaped_name_and_in_array() {
        let e = parse_body(br#"{"l":[{"x":1},{"y":1,"y":2}],"a/b":1,"a~1b":2}"#, &[]).unwrap_err();
        let list = errors_of(&e);
        assert!(list.contains(&("/l/1/y".into(), "duplicate_member".into())), "{list:?}");
        // "a/b" and "a~1b" are two different names. No duplicate between them.
        assert_eq!(list.len(), 1, "{list:?}");
    }

    #[test]
    fn same_name_in_two_objects_is_not_a_duplicate() {
        assert!(parse_body(br#"{"a":{"x":1},"b":{"x":2},"c":[{"x":1},{"x":2}]}"#, &[]).is_ok());
    }

    #[test]
    fn depth_limit_is_64() {
        let ok = format!("{}1{}", "[".repeat(64), "]".repeat(64));
        assert!(parse_body(ok.as_bytes(), &[]).is_ok());
        let deep = format!("{}1{}", "[".repeat(65), "]".repeat(65));
        let e = parse_body(deep.as_bytes(), &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0].1, "too_deep");
        let very_deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
        let e = parse_body(very_deep.as_bytes(), &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0].1, "too_deep");
    }

    #[test]
    fn syntax_and_encoding_errors() {
        let e = parse_body(b"{\"a\": }", &[]).unwrap_err();
        let body = e.body();
        assert_eq!(body["error"]["errors"][0]["code"], "syntax");
        assert_eq!(body["error"]["errors"][0]["line"], 1);
        assert!(body["error"]["errors"][0]["column"].as_u64().unwrap() >= 1);
        let e = parse_body(b"", &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0].1, "syntax");
        let e = parse_body(&[b'{', 0xff, b'}'], &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0].1, "encoding");
    }

    #[test]
    fn raw_member_is_not_scanned_and_gives_its_bytes() {
        let body = br#"{ "message" : "m", "document" : { "a": 1, "a": 2, "b": [ 1, 2 ] } , "folder":"x"}"#;
        let parsed = parse_body(body, &["document"]).unwrap();
        assert_eq!(parsed.member_bytes(body, "document").unwrap(), br#"{ "a": 1, "a": 2, "b": [ 1, 2 ] }"#);
        assert_eq!(parsed.member_bytes(body, "message").unwrap(), br#""m""#);
        assert_eq!(parsed.member_bytes(body, "folder").unwrap(), br#""x""#);
        assert!(parsed.member_bytes(body, "absent").is_none());
        // The same duplicates outside a raw member are reported.
        assert!(parse_body(body, &[]).is_err());
    }

    #[test]
    fn member_spans_of_scalars() {
        let body = br#"{"n": 12 , "t": true,"s":"a,}b" }"#;
        let parsed = parse_body(body, &[]).unwrap();
        assert_eq!(parsed.member_bytes(body, "n").unwrap(), b"12");
        assert_eq!(parsed.member_bytes(body, "t").unwrap(), b"true");
        assert_eq!(parsed.member_bytes(body, "s").unwrap(), br#""a,}b""#);
    }

    static ITEM: Shape = Shape::Str { min_chars: 1, max_chars: 3 };
    static NAMES: Shape = Shape::Array { item: &ITEM, max: 2 };
    static VALUE: Shape = Shape::Bool;
    static MEMBERS: [Member; 5] = [
        Member { name: "id", shape: Shape::Str { min_chars: 1, max_chars: 40 }, required: true },
        Member { name: "n", shape: Shape::Int { min: 1, max: 10 }, required: true },
        Member { name: "names", shape: NAMES, required: false },
        Member { name: "flags", shape: Shape::Map { value: &VALUE, max: 2 }, required: false },
        Member { name: "opt", shape: Shape::Nullable(&ITEM), required: false },
    ];
    static ROOT: Shape = Shape::Object(&MEMBERS);

    #[test]
    fn shape_accepts_a_good_body() {
        let body = br#"{"id":"a","n":3,"names":["x","yy"],"flags":{"a":true},"opt":null}"#;
        assert!(parse_and_check(body, &ROOT, &[]).is_ok());
    }

    #[test]
    fn shape_reports_every_fault_with_pointers() {
        let body = br#"{"n":"3","names":["", 5, "x", "y"],"flags":{"a/b":1},"extra":1,"opt":"long"}"#;
        let e = parse_and_check(body, &ROOT, &[]).unwrap_err();
        assert_eq!(e.code, Code::BadRequest);
        let list = errors_of(&e);
        let has = |p: &str, c: &str| list.contains(&(p.to_owned(), c.to_owned()));
        assert!(has("/id", "missing_member"), "{list:?}");
        assert!(has("/n", "wrong_type"), "{list:?}");
        assert!(has("/names", "too_large"), "{list:?}");
        assert!(has("/flags/a~1b", "wrong_type"), "{list:?}");
        assert!(has("/extra", "unknown_member"), "{list:?}");
        assert!(has("/opt", "out_of_range"), "{list:?}");
    }

    #[test]
    fn shape_item_pointers_and_ranges() {
        let e = parse_and_check(br#"{"id":"a","n":11,"names":["", 5]}"#, &ROOT, &[]).unwrap_err();
        let list = errors_of(&e);
        assert!(list.contains(&("/n".into(), "out_of_range".into())), "{list:?}");
        assert!(list.contains(&("/names/0".into(), "out_of_range".into())), "{list:?}");
        assert!(list.contains(&("/names/1".into(), "wrong_type".into())), "{list:?}");
        let e = parse_and_check(b"[]", &ROOT, &[]).unwrap_err();
        assert_eq!(errors_of(&e), [("".to_owned(), "wrong_type".to_owned())]);
    }

    #[test]
    fn int_rejects_fractions_and_big_numbers() {
        let e = parse_and_check(br#"{"id":"a","n":1.5}"#, &ROOT, &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0], ("/n".to_owned(), "wrong_type".to_owned()));
        let e = parse_and_check(br#"{"id":"a","n":18446744073709551615}"#, &ROOT, &[]).unwrap_err();
        assert_eq!(errors_of(&e)[0], ("/n".to_owned(), "wrong_type".to_owned()));
    }

    #[test]
    fn error_list_is_bounded() {
        let mut text = String::from("{");
        for i in 0..400 {
            if i > 0 {
                text.push(',');
            }
            text.push_str(&format!("\"k{i}\":1"));
        }
        text.push('}');
        let e = parse_and_check(text.as_bytes(), &ROOT, &[]).unwrap_err();
        let list = errors_of(&e);
        assert!(list.len() <= MAX_ERRORS);
        assert_eq!(list.last().unwrap().1, "too_many_errors");
    }
}
