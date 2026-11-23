//! A JSON value with members in a fixed order, and the writer of the canonical layout (4.38).
//!
//! `serde_json` without the feature `preserve_order` sorts the members of an object. The
//! importer needs the order of the tables of section 4. So it builds this tree and writes it.

/// A JSON value ready to write. An object keeps its members in the order to write.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    Null,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Arr(Vec<Out>),
    Obj(Vec<(String, Out)>),
}

impl Out {
    /// Makes a string value.
    pub(crate) fn s(text: &str) -> Out {
        Out::Str(text.to_owned())
    }

    /// Makes a string value or `null`.
    pub(crate) fn opt_s(text: Option<&str>) -> Out {
        text.map_or(Out::Null, Out::s)
    }

    /// Makes an object from members.
    pub(crate) fn obj<const N: usize>(members: [(&str, Out); N]) -> Out {
        Out::Obj(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// Makes an object from a list of members. Use it when the number of members is not fixed.
    pub(crate) fn members(members: Vec<(&str, Out)>) -> Out {
        Out::Obj(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    /// Makes an array of strings.
    pub(crate) fn strings<S: AsRef<str>>(items: &[S]) -> Out {
        Out::Arr(items.iter().map(|s| Out::s(s.as_ref())).collect())
    }

    /// Writes the value in the canonical layout with one line end at the end (4.38).
    pub(crate) fn write(&self) -> String {
        let mut buf = String::new();
        write_value(self, 0, &mut buf);
        buf.push('\n');
        buf
    }
}

fn indent(level: usize, buf: &mut String) {
    for _ in 0..level {
        buf.push_str("  ");
    }
}

fn write_value(out: &Out, level: usize, buf: &mut String) {
    match out {
        Out::Null => buf.push_str("null"),
        Out::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        Out::Int(i) => buf.push_str(&i.to_string()),
        Out::Num(f) => buf.push_str(&number_text(*f)),
        Out::Str(s) => write_string(s, buf),
        Out::Arr(items) if items.is_empty() => buf.push_str("[]"),
        Out::Arr(items) => {
            buf.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                indent(level + 1, buf);
                write_value(item, level + 1, buf);
                buf.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            indent(level, buf);
            buf.push(']');
        }
        Out::Obj(members) if members.is_empty() => buf.push_str("{}"),
        Out::Obj(members) => {
            buf.push_str("{\n");
            for (i, (name, value)) in members.iter().enumerate() {
                indent(level + 1, buf);
                write_string(name, buf);
                buf.push_str(": ");
                write_value(value, level + 1, buf);
                buf.push_str(if i + 1 < members.len() { ",\n" } else { "\n" });
            }
            indent(level, buf);
            buf.push('}');
        }
    }
}

fn write_string(s: &str, buf: &mut String) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\u{8}' => buf.push_str("\\b"),
            '\u{c}' => buf.push_str("\\f"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if c <= '\u{1f}' => buf.push_str(&format!("\\u{:04x}", c as u32)),
            c => buf.push(c),
        }
    }
    buf.push('"');
}

/// Gives the text of a finite double in the form of RFC 8785, section 3.2.2.3 (4.38, rule 9).
pub(crate) fn number_text(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return "0".to_owned();
    }
    // `{:e}` gives the shortest digits that read back to the same double.
    let raw = format!("{:e}", value.abs());
    let (mantissa, exponent) = raw.split_once('e').unwrap_or((raw.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let n = exponent + 1;
    let k = i32::try_from(digits.len()).unwrap_or(0);
    let sign = if value < 0.0 { "-" } else { "" };
    let zeros = |count: i32| "0".repeat(usize::try_from(count).unwrap_or(0));
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", zeros(n - k))
    } else if 0 < n && n <= 21 {
        let (a, b) = digits.split_at(usize::try_from(n).unwrap_or(0));
        format!("{a}.{b}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", zeros(-n))
    } else {
        let e = n - 1;
        let esign = if e < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        let mant = if rest.is_empty() { first.to_owned() } else { format!("{first}.{rest}") };
        format!("{mant}e{esign}{}", e.abs())
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_forms_of_rfc_8785() {
        assert_eq!(number_text(1.5), "1.5");
        assert_eq!(number_text(1000.0), "1000");
        assert_eq!(number_text(-0.0), "0");
        assert_eq!(number_text(0.000001), "0.000001");
        assert_eq!(number_text(1e-7), "1e-7");
        assert_eq!(number_text(1e21), "1e+21");
        assert_eq!(number_text(1.234_567_890_123_456_8e29), "1.2345678901234568e+29");
        assert_eq!(number_text(-12.25), "-12.25");
        assert_eq!(number_text(123456.0), "123456");
    }

    #[test]
    fn canonical_layout() {
        let v = Out::obj([("a", Out::Arr(vec![Out::Int(1), Out::s("x\ny")])), ("b", Out::Obj(vec![]))]);
        assert_eq!(v.write(), "{\n  \"a\": [\n    1,\n    \"x\\ny\"\n  ],\n  \"b\": {}\n}\n");
    }
}
