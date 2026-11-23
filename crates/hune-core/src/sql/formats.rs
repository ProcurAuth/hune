//! The variable formats that are valid only inside a string literal (7.9, D7).
//!
//! Each format here copies the Grafana format of the same name (Grafana 12.4.2, the format registry of the
//! front end). The function [`render`] gives the text before the string escape. The caller applies the
//! string escape of 7.8 to the whole text, so the text cannot leave the literal. The text is free text.
//! Code and a quoted name refuse these formats (7.8), because the escape of a literal does not hold there.

use super::placeholders::Format;

/// What a placeholder with a format stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject<'a> {
    /// One value: the selection of a variable that is not `multi`, or the empty selection (`""`).
    One(&'a str),
    /// A list: a `multi` variable, or "All" without `all_value`. `all` tells that the viewer selected "All".
    List {
        /// The values, in order.
        values: &'a [String],
        /// True when the viewer selected "All".
        all: bool,
    },
    /// "All" with an `all_value`: the SQL text of the author.
    AllValue(&'a str),
}

/// Renders the format `format` of the variable `variable` for `subject`.
///
/// It gives `None` for a format that is not literal-only (`raw`, `csv`, `singlequote`, `sqlstring`).
pub fn render(format: Format, variable: &str, subject: Subject<'_>) -> Option<String> {
    if !format.literal_only() {
        return None;
    }
    Some(match subject {
        Subject::AllValue(text) => all_value(format, variable, text),
        Subject::One(v) => one(format, variable, v),
        Subject::List { values, all } => list(format, variable, values, all),
    })
}

/// The value `$__all` that the URL of Grafana holds for "All".
const ALL_URL_VALUE: &str = "$__all";

fn all_value(format: Format, variable: &str, text: &str) -> String {
    match format {
        Format::Text => "All".to_owned(),
        Format::PercentEncode => percent_encode(text),
        Format::QueryParam => format!("var-{variable}={ALL_URL_VALUE}"),
        // Grafana gives the text of the author without a change, whatever the other format.
        _ => text.to_owned(),
    }
}

fn one(format: Format, variable: &str, v: &str) -> String {
    match format {
        Format::Json | Format::Pipe | Format::Glob | Format::Distributed | Format::Text => v.to_owned(),
        Format::Regex => escape_regex(v),
        Format::Lucene => escape_lucene(v),
        Format::PercentEncode => percent_encode(v),
        Format::QueryParam => format!("var-{variable}={}", url_param_encode(v)),
        Format::DoubleQuote => double_quote(v),
        Format::Html => escape_html(v),
        Format::Raw | Format::Csv | Format::SingleQuote | Format::SqlString => v.to_owned(),
    }
}

fn list(format: Format, variable: &str, values: &[String], all: bool) -> String {
    let join = |sep: &str| values.join(sep);
    match format {
        Format::Json => json_array(values),
        Format::Pipe => join("|"),
        Format::Regex => {
            let x: Vec<String> = values.iter().map(|v| escape_regex(v)).collect();
            if x.len() == 1 { x[0].clone() } else { format!("({})", x.join("|")) }
        }
        Format::Glob => {
            if values.len() > 1 {
                format!("{{{}}}", join(","))
            } else {
                join(",")
            }
        }
        Format::Lucene => {
            if values.is_empty() {
                "__empty__".to_owned()
            } else {
                let x: Vec<String> = values.iter().map(|v| format!("\"{}\"", escape_lucene(v))).collect();
                format!("({})", x.join(" OR "))
            }
        }
        Format::PercentEncode => percent_encode(&format!("{{{}}}", join(","))),
        Format::QueryParam => {
            if all {
                format!("var-{variable}={ALL_URL_VALUE}")
            } else if values.is_empty() {
                format!("var-{variable}=")
            } else {
                let x: Vec<String> = values.iter().map(|v| format!("var-{variable}={}", url_param_encode(v))).collect();
                x.join("&")
            }
        }
        Format::Distributed => {
            let x: Vec<String> = values
                .iter()
                .enumerate()
                .map(|(i, v)| if i == 0 { v.clone() } else { format!("{variable}={v}") })
                .collect();
            x.join(",")
        }
        Format::DoubleQuote => {
            let x: Vec<String> = values.iter().map(|v| double_quote(v)).collect();
            x.join(",")
        }
        Format::Html => escape_html(&join(", ")),
        Format::Text => {
            if all {
                "All".to_owned()
            } else {
                join(" + ")
            }
        }
        Format::Raw | Format::Csv | Format::SingleQuote | Format::SqlString => join(","),
    }
}

/// The characters that JavaScript counts as white space in `\s` and in the trim of a number.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `escapeRegex` of Grafana: a backslash before each of `\ ^ $ * + ? . ( ) | [ ] { } /`.
fn escape_regex(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        if matches!(c, '\\' | '^' | '$' | '*' | '+' | '?' | '.' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '/') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Tells if the JavaScript call `Number(text)` gives a number and not `NaN`.
fn js_is_number(text: &str) -> bool {
    let t = text.trim_matches(is_js_space);
    if t.is_empty() {
        return true;
    }
    let radix = |digits: &str, r: u32| !digits.is_empty() && digits.chars().all(|c| c.is_digit(r));
    for (prefix, r) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return radix(rest, r);
        }
    }
    let t = t.strip_prefix(['+', '-']).unwrap_or(t);
    if t == "Infinity" {
        return true;
    }
    let (mantissa, exponent) = match t.find(['e', 'E']) {
        Some(i) => (&t[..i], Some(&t[i + 1..])),
        None => (t, None),
    };
    if let Some(e) = exponent {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        if e.is_empty() || !e.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    let (int, frac) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    (!int.is_empty() || !frac.is_empty())
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.bytes().all(|b| b.is_ascii_digit())
}

/// `luceneEscape` of Grafana: a text that is a number stays as it is. Else a backslash goes before each of
/// `! * + - = < > & | ( ) [ ] { } ^ ~ ? : \ / "` and before each white space character.
fn escape_lucene(v: &str) -> String {
    if js_is_number(v) {
        return v.to_owned();
    }
    let mut out = String::with_capacity(v.len() + 8);
    for c in v.chars() {
        if is_js_space(c) || "!*+-=<>&|()[]{}^~?:\\/\"".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Writes the UTF-8 bytes of `c` as `%XX` with capital hex digits.
fn push_percent(out: &mut String, c: char) {
    let mut buf = [0u8; 4];
    for b in c.encode_utf8(&mut buf).bytes() {
        out.push_str(&format!("%{b:02X}"));
    }
}

/// The percent encoding of Grafana (`encodeURIComponent`, then `! ' ( ) *` too): only `A-Z a-z 0-9 - _ . ~` stay.
fn percent_encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len() * 3);
    for c in v.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            push_percent(&mut out, c);
        }
    }
    out
}

/// The encoding of `toUrlParams` of Grafana: as [`percent_encode`], but `@ : $ , ;` stay and a space is `+`.
fn url_param_encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len() * 3);
    for c in v.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '@' | ':' | '$' | ',' | ';') {
            out.push(c);
        } else if c == ' ' {
            out.push('+');
        } else {
            push_percent(&mut out, c);
        }
    }
    out
}

/// `doublequote` of Grafana: `"` before and after, and `\` before each `"` of the value.
fn double_quote(v: &str) -> String {
    format!("\"{}\"", v.replace('"', "\\\""))
}

/// `escapeHtml` of Grafana: `& < > ' "` become entities.
fn escape_html(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 8);
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// `JSON.stringify` of a list of strings.
fn json_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in v.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::placeholders::LITERAL_ONLY_FORMATS;

    fn many(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn list_of(f: Format, items: &[&str]) -> String {
        let v = many(items);
        render(f, "env", Subject::List { values: &v, all: false }).unwrap()
    }

    fn one_of(f: Format, v: &str) -> String {
        render(f, "env", Subject::One(v)).unwrap()
    }

    #[test]
    fn the_vectors_of_grafana_for_two_values() {
        let two = ["prod", "dev"];
        for (f, want) in [
            (Format::Json, r#"["prod","dev"]"#),
            (Format::Pipe, "prod|dev"),
            (Format::Regex, "(prod|dev)"),
            (Format::Glob, "{prod,dev}"),
            (Format::Lucene, r#"("prod" OR "dev")"#),
            (Format::PercentEncode, "%7Bprod%2Cdev%7D"),
            (Format::QueryParam, "var-env=prod&var-env=dev"),
            (Format::Distributed, "prod,env=dev"),
            (Format::DoubleQuote, r#""prod","dev""#),
            (Format::Html, "prod, dev"),
            (Format::Text, "prod + dev"),
        ] {
            assert_eq!(list_of(f, &two), want, "{f:?}");
        }
    }

    #[test]
    fn the_vectors_of_grafana_for_one_value() {
        for (f, want) in [
            (Format::Json, "dev"),
            (Format::Pipe, "dev"),
            (Format::Regex, "dev"),
            (Format::Glob, "dev"),
            (Format::Lucene, "dev"),
            (Format::PercentEncode, "dev"),
            (Format::QueryParam, "var-env=dev"),
            (Format::Distributed, "dev"),
            (Format::DoubleQuote, r#""dev""#),
            (Format::Html, "dev"),
            (Format::Text, "dev"),
        ] {
            assert_eq!(one_of(f, "dev"), want, "{f:?}");
        }
    }

    #[test]
    fn a_list_of_one_value_and_an_empty_list() {
        assert_eq!(list_of(Format::Regex, &["a.b"]), r"a\.b");
        assert_eq!(list_of(Format::Glob, &["a"]), "a");
        assert_eq!(list_of(Format::Json, &["a"]), r#"["a"]"#);
        assert_eq!(list_of(Format::Lucene, &["a b"]), r#"("a\ b")"#);
        assert_eq!(list_of(Format::Text, &["a"]), "a");
        assert_eq!(list_of(Format::QueryParam, &["a"]), "var-env=a");
        let none: [&str; 0] = [];
        let want = [
            (Format::Json, "[]"),
            (Format::Pipe, ""),
            (Format::Regex, "()"),
            (Format::Glob, ""),
            (Format::Lucene, "__empty__"),
            (Format::PercentEncode, "%7B%7D"),
            (Format::QueryParam, "var-env="),
            (Format::Distributed, ""),
            (Format::DoubleQuote, ""),
            (Format::Html, ""),
            (Format::Text, ""),
        ];
        for (f, w) in want {
            assert_eq!(list_of(f, &none), w, "{f:?}");
        }
    }

    #[test]
    fn special_characters() {
        assert_eq!(one_of(Format::Regex, r"a.b*c(d)|e[f]{g}^$+?\/"), r"a\.b\*c\(d\)\|e\[f\]\{g\}\^\$\+\?\\\/");
        assert_eq!(one_of(Format::Lucene, r#"a-b c:"d"/\"#), r#"a\-b\ c\:\"d\"\/\\"#);
        assert_eq!(one_of(Format::Lucene, "12"), "12");
        assert_eq!(one_of(Format::Lucene, "-1.5e3"), "-1.5e3");
        assert_eq!(one_of(Format::Lucene, "0x1F"), "0x1F");
        assert_eq!(one_of(Format::Lucene, ""), "");
        assert_eq!(one_of(Format::Lucene, " "), " ");
        assert_eq!(one_of(Format::Lucene, "1 2"), r"1\ 2");
        assert_eq!(one_of(Format::Lucene, "1_0"), r"1_0");
        assert_eq!(one_of(Format::PercentEncode, "a b/c'd(e)!~é"), "a%20b%2Fc%27d%28e%29%21~%C3%A9");
        assert_eq!(one_of(Format::QueryParam, "a b/c@d:e$f,g;h'i"), "var-env=a+b%2Fc@d:e$f,g;h%27i");
        assert_eq!(one_of(Format::DoubleQuote, r#"a"b\c"#), r#""a\"b\c""#);
        assert_eq!(one_of(Format::Html, r#"<a href="x">&'"#), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
        assert_eq!(
            list_of(Format::Json, &["a\"b", "c\\d", "\u{7f}", "\u{e9}"]),
            "[\"a\\\"b\",\"c\\\\d\",\"\u{7f}\",\"\u{e9}\"]"
        );
        assert_eq!(list_of(Format::Html, &["a<b", "c"]), "a&lt;b, c");
    }

    #[test]
    fn the_numbers_of_javascript() {
        for t in [
            "",
            " ",
            "0",
            "-0",
            "+1",
            "1.",
            ".5",
            "1e5",
            "1E-5",
            "Infinity",
            "-Infinity",
            "0b101",
            "0o17",
            "0XaB",
            " 7 ",
        ] {
            assert!(js_is_number(t), "{t:?}");
        }
        for t in
            ["a", "1a", "1e", "e1", ".", "+", "-", "0x", "0xg", "0b2", "infinity", "1_0", "1,5", "--1", "1.2.3", "0x-1"]
        {
            assert!(!js_is_number(t), "{t:?}");
        }
    }

    #[test]
    fn all_selected() {
        let v = many(&["prod", "dev"]);
        let all = |f| render(f, "env", Subject::List { values: &v, all: true }).unwrap();
        assert_eq!(all(Format::Text), "All");
        assert_eq!(all(Format::QueryParam), "var-env=$__all");
        assert_eq!(all(Format::Regex), "(prod|dev)");
        let av = |f| render(f, "env", Subject::AllValue("a.b'")).unwrap();
        assert_eq!(av(Format::Text), "All");
        assert_eq!(av(Format::Regex), "a.b'");
        assert_eq!(av(Format::DoubleQuote), "a.b'");
        assert_eq!(av(Format::PercentEncode), "a.b%27");
        assert_eq!(av(Format::QueryParam), "var-env=$__all");
    }

    #[test]
    fn only_the_literal_formats_render() {
        for f in [Format::Raw, Format::Csv, Format::SingleQuote, Format::SqlString] {
            assert_eq!(render(f, "x", Subject::One("a")), None);
        }
        for f in LITERAL_ONLY_FORMATS {
            assert!(render(f, "x", Subject::One("a")).is_some());
        }
    }
}
