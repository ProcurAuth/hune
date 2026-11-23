//! The JSON reader of a document (4.36, steps 1 and 2).
//!
//! The reader is strict. It refuses a duplicate member and a depth above `MAX_JSON_DEPTH`. It
//! keeps a number as its text, so that the validator can tell an integer from a number. It
//! gives the line and the column (in characters, from 1) of an `encoding` or `syntax` error.

use crate::error::{ErrorCode, ValidationError, pointer_token};
use crate::limits::{MAX_DOCUMENT_BYTES, MAX_JSON_DEPTH};
use std::collections::HashSet;

/// A JSON value. An object keeps its members in input order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(Num),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

/// The longest number text that a [`Num`] keeps with no heap memory.
const INLINE_NUM: usize = 22;

/// A JSON number. The text follows the grammar of RFC 8259. A text of at most 22 bytes stays in the value,
/// so an array of many numbers costs no heap memory for each number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Num {
    text: NumText,
}

/// The text of a [`Num`]. The bytes after `len` are 0, so two equal texts are equal values.
#[derive(Debug, Clone, PartialEq)]
enum NumText {
    Inline { len: u8, bytes: [u8; INLINE_NUM] },
    Heap(Box<str>),
}

impl Num {
    /// Makes the number of a text of the grammar of RFC 8259.
    fn of(text: &str) -> Num {
        let text = match u8::try_from(text.len()) {
            Ok(len) if text.len() <= INLINE_NUM => {
                let mut bytes = [0u8; INLINE_NUM];
                bytes[..text.len()].copy_from_slice(text.as_bytes());
                NumText::Inline { len, bytes }
            }
            _ => NumText::Heap(text.into()),
        };
        Num { text }
    }

    /// Makes the number of an integer.
    pub fn integer(value: i64) -> Num {
        Num::of(&value.to_string())
    }

    /// Gives the text of the number as it is in the input.
    pub fn text(&self) -> &str {
        match &self.text {
            NumText::Inline { len, bytes } => std::str::from_utf8(&bytes[..usize::from(*len)]).unwrap_or_default(),
            NumText::Heap(text) => text,
        }
    }

    /// Tells if the number has no fraction and no exponent (4.2).
    pub fn is_integer(&self) -> bool {
        !self.text().contains(['.', 'e', 'E'])
    }

    /// Gives the value of an integer. A number that is not an integer, or that is too large,
    /// gives `None`.
    pub fn as_i64(&self) -> Option<i64> {
        if self.is_integer() { self.text().parse::<i64>().ok() } else { None }
    }

    /// Gives the nearest double. A value that is not finite gives `None`.
    pub fn as_f64(&self) -> Option<f64> {
        self.text().parse::<f64>().ok().filter(|f| f.is_finite())
    }
}

/// Checks the size and the encoding of the input (4.36, step 1) and gives the text.
pub(crate) fn check_bytes(bytes: &[u8]) -> Result<&str, ValidationError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(ValidationError::new(
            "",
            ErrorCode::TooLarge,
            format!("document must have at most {MAX_DOCUMENT_BYTES} bytes; value {} bytes", bytes.len()),
        ));
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(ValidationError::at(ErrorCode::Encoding, "document must not start with a byte order mark", 1, 1));
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(e) => {
            let valid = bytes.get(..e.valid_up_to()).unwrap_or_default();
            let (line, column) = position(valid);
            Err(ValidationError::at(
                ErrorCode::Encoding,
                format!("document must be UTF-8; byte {} is not valid", e.valid_up_to()),
                line,
                column,
            ))
        }
    }
}

/// Gives the line and the column of the byte after `prefix`. The prefix is valid UTF-8.
fn position(prefix: &[u8]) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut column: u32 = 1;
    for &b in prefix {
        if b == b'\n' {
            line = line.saturating_add(1);
            column = 1;
        } else if b & 0xC0 != 0x80 {
            column = column.saturating_add(1);
        }
    }
    (line, column)
}

/// Parses one JSON text. Only white space can follow the value.
pub(crate) fn parse(text: &str) -> Result<Json, ValidationError> {
    let mut p = Parser { bytes: text.as_bytes(), text, pos: 0, line: 1, column: 1, path: Vec::new() };
    p.skip_ws();
    let value = p.value(1)?;
    p.skip_ws();
    if p.pos < p.bytes.len() {
        return Err(p.syntax("unexpected data after the end of the document"));
    }
    Ok(value)
}

enum Seg {
    Key(String),
    Index(usize),
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    path: Vec<Seg>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        if let Some(&b) = self.bytes.get(self.pos) {
            self.pos += 1;
            if b == b'\n' {
                self.line = self.line.saturating_add(1);
                self.column = 1;
            } else if b & 0xC0 != 0x80 {
                self.column = self.column.saturating_add(1);
            }
        }
    }

    fn syntax(&self, message: &str) -> ValidationError {
        ValidationError::at(ErrorCode::Syntax, message, self.line, self.column)
    }

    /// Describes the character at the cursor for a message.
    fn found(&self) -> String {
        match self.text.get(self.pos..).and_then(|s| s.chars().next()) {
            None => "the end of the document".to_owned(),
            Some(c) if c.is_control() => format!("U+{:04X}", c as u32),
            Some(c) => format!("'{c}'"),
        }
    }

    fn pointer(&self) -> String {
        let mut out = String::new();
        for seg in &self.path {
            out.push('/');
            match seg {
                Seg::Key(k) => out.push_str(&pointer_token(k)),
                Seg::Index(i) => out.push_str(&i.to_string()),
            }
        }
        out
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.bump();
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, ValidationError> {
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.syntax(&format!("expected a value; found {}", self.found()))),
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, ValidationError> {
        let ok = self.bytes.get(self.pos..).is_some_and(|rest| rest.starts_with(word.as_bytes()));
        if !ok {
            return Err(self.syntax(&format!("expected {word}; found {}", self.found())));
        }
        for _ in 0..word.len() {
            self.bump();
        }
        Ok(value)
    }

    fn digits(&mut self) -> usize {
        let mut n = 0;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
            n += 1;
        }
        n
    }

    fn number(&mut self) -> Result<Json, ValidationError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => self.bump(),
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.syntax(&format!("expected a digit in a number; found {}", self.found()))),
        }
        if self.peek() == Some(b'.') {
            self.bump();
            if self.digits() == 0 {
                return Err(self.syntax(&format!("expected a digit after the decimal point; found {}", self.found())));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            if self.digits() == 0 {
                return Err(self.syntax(&format!("expected a digit in the exponent; found {}", self.found())));
            }
        }
        Ok(Json::Num(Num::of(self.text.get(start..self.pos).unwrap_or_default())))
    }

    fn hex4(&mut self) -> Result<u32, ValidationError> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let d = match self.peek() {
                Some(b @ b'0'..=b'9') => u32::from(b - b'0'),
                Some(b @ b'a'..=b'f') => u32::from(b - b'a') + 10,
                Some(b @ b'A'..=b'F') => u32::from(b - b'A') + 10,
                _ => {
                    return Err(self.syntax(&format!("expected four hex digits after \\u; found {}", self.found())));
                }
            };
            self.bump();
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, ValidationError> {
        self.bump(); // the opening quote
        let mut out = String::new();
        loop {
            let run_start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.bump();
            }
            out.push_str(self.text.get(run_start..self.pos).unwrap_or_default());
            match self.peek() {
                None => return Err(self.syntax("the string has no closing quote")),
                Some(b'"') => {
                    self.bump();
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.bump();
                    self.escape(&mut out)?;
                }
                Some(_) => {
                    return Err(
                        self.syntax(&format!("a string must not contain {}; write it as an escape", self.found()))
                    );
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), ValidationError> {
        let c = match self.peek() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.bump();
                let first = self.hex4()?;
                let code = if (0xD800..0xDC00).contains(&first) {
                    if self.peek() != Some(b'\\') {
                        return Err(self.syntax("a high surrogate escape needs a low surrogate escape after it"));
                    }
                    self.bump();
                    if self.peek() != Some(b'u') {
                        return Err(self.syntax("a high surrogate escape needs a low surrogate escape after it"));
                    }
                    self.bump();
                    let second = self.hex4()?;
                    if !(0xDC00..0xE000).contains(&second) {
                        return Err(self.syntax("a high surrogate escape needs a low surrogate escape after it"));
                    }
                    0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                } else if (0xDC00..0xE000).contains(&first) {
                    return Err(self.syntax("a low surrogate escape has no high surrogate escape before it"));
                } else {
                    first
                };
                match char::from_u32(code) {
                    Some(c) => out.push(c),
                    None => return Err(self.syntax("the escape is not a Unicode scalar value")),
                }
                return Ok(());
            }
            _ => return Err(self.syntax(&format!("expected a valid escape after a backslash; found {}", self.found()))),
        };
        self.bump();
        out.push(c);
        Ok(())
    }

    fn enter(&mut self, depth: usize) -> Result<(), ValidationError> {
        if depth > MAX_JSON_DEPTH {
            return Err(ValidationError::new(
                self.pointer(),
                ErrorCode::TooDeep,
                format!("depth must be at most {MAX_JSON_DEPTH}; value {depth}"),
            ));
        }
        Ok(())
    }

    fn array(&mut self, depth: usize) -> Result<Json, ValidationError> {
        self.enter(depth)?;
        self.bump();
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.bump();
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            self.path.push(Seg::Index(items.len()));
            let item = self.value(depth + 1)?;
            self.path.pop();
            items.push(item);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b']') => {
                    self.bump();
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.syntax(&format!("expected ',' or ']' in an array; found {}", self.found()))),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, ValidationError> {
        self.enter(depth)?;
        self.bump();
        let mut members: Vec<(String, Json)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.bump();
            return Ok(Json::Obj(members));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.syntax(&format!("expected a member name (a string); found {}", self.found())));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.syntax(&format!("expected ':' after a member name; found {}", self.found())));
            }
            self.bump();
            self.skip_ws();
            if !seen.insert(key.clone()) {
                self.path.push(Seg::Key(key.clone()));
                let pointer = self.pointer();
                return Err(ValidationError::new(
                    pointer,
                    ErrorCode::DuplicateMember,
                    format!("member {} occurs two times in one object", crate::document::read::clip(&key)),
                ));
            }
            self.path.push(Seg::Key(key.clone()));
            let value = self.value(depth + 1)?;
            self.path.pop();
            members.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.bump(),
                Some(b'}') => {
                    self.bump();
                    return Ok(Json::Obj(members));
                }
                _ => return Err(self.syntax(&format!("expected ',' or '}}' in an object; found {}", self.found()))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> ValidationError {
        parse(text).unwrap_err()
    }

    #[test]
    fn parses_values() {
        let v = parse(" {\"a\": [1, 2.5e3, -0, true, null, \"x\\u00e9\"]} ").unwrap();
        let Json::Obj(m) = v else { panic!("object") };
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn position_counts_characters() {
        let e = err("{\n  \"é\": x}");
        assert_eq!(e.code, ErrorCode::Syntax);
        assert_eq!((e.line, e.column), (Some(2), Some(8)));
    }

    #[test]
    fn surrogate_pair() {
        assert_eq!(parse("\"\\ud83d\\ude00\"").unwrap(), Json::Str("\u{1F600}".to_owned()));
        assert_eq!(err("\"\\ud83d\"").code, ErrorCode::Syntax);
        assert_eq!(err("\"\\ude00\"").code, ErrorCode::Syntax);
    }

    #[test]
    fn refused_forms() {
        for bad in ["", "{", "{\"a\"}", "{\"a\":1,}", "[1,]", "01", "1.", "1e", "-", "+1", "\"a\nb\"", "tru", "{} x"] {
            assert_eq!(err(bad).code, ErrorCode::Syntax, "{bad:?}");
        }
    }

    #[test]
    fn depth_bound() {
        let ok = format!("{}{}", "[".repeat(32), "]".repeat(32));
        assert!(parse(&ok).is_ok());
        let deep = format!("{}{}", "[".repeat(33), "]".repeat(33));
        assert_eq!(err(&deep).code, ErrorCode::TooDeep);
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert_eq!(err(&deep).code, ErrorCode::TooDeep);
    }

    #[test]
    fn duplicate_member_pointer() {
        let e = err("{\"a\": {\"b\": 1, \"b\": 2}}");
        assert_eq!(e.code, ErrorCode::DuplicateMember);
        assert_eq!(e.pointer, "/a/b");
    }

    #[test]
    fn number_kinds() {
        // A number costs no more than a text in the tree.
        assert!(std::mem::size_of::<Num>() <= std::mem::size_of::<String>());
        assert!(std::mem::size_of::<Json>() <= 32);
        let Json::Num(n) = parse("12345678901234567890.5").unwrap() else { panic!() };
        assert_eq!(n.text(), "12345678901234567890.5");
        let long = format!("1{}", "0".repeat(40));
        let Json::Num(n) = parse(&long).unwrap() else { panic!() };
        assert_eq!(n.text(), long);
        assert_eq!(Num::integer(-42).text(), "-42");
        let Json::Num(n) = parse("12").unwrap() else { panic!() };
        assert!(n.is_integer());
        assert_eq!(n.as_i64(), Some(12));
        let Json::Num(n) = parse("1e999").unwrap() else { panic!() };
        assert_eq!(n.as_f64(), None);
        let Json::Num(n) = parse("99999999999999999999").unwrap() else { panic!() };
        assert_eq!(n.as_i64(), None);
        assert!(n.as_f64().is_some());
    }

    #[test]
    fn encoding_errors() {
        let e = check_bytes(b"\xEF\xBB\xBF{}").unwrap_err();
        assert_eq!((e.code, e.line, e.column), (ErrorCode::Encoding, Some(1), Some(1)));
        let e = check_bytes(b"{\n\"a\": \"\xFF\"}").unwrap_err();
        assert_eq!((e.code, e.line, e.column), (ErrorCode::Encoding, Some(2), Some(7)));
        let big = vec![b' '; MAX_DOCUMENT_BYTES + 1];
        assert_eq!(check_bytes(&big).unwrap_err().code, ErrorCode::TooLarge);
    }
}
