// SPDX-License-Identifier: MIT OR Apache-2.0
//! The escape rules of 7.8 (ClickHouse) and 7.21 (PostgreSQL).
//!
//! The functions with `pg` in their name follow 7.21. A standard string doubles `'` and keeps a
//! backslash. An `E` string doubles `'` and writes `\` as `\\`. A quoted name doubles `"`. The session has
//! `standard_conforming_strings = on` (8a.4).
//!
//! A string escape writes `\` as `\\` and `'` as `\'`. A name escape writes `\` as `\\` and puts a
//! `\` before the quote mark of the name. A value has no control character (7.7), so no other
//! escape applies. These functions are the only place that writes a value into a literal.

/// Writes `value` with the string escapes of 7.8, with no quote marks around it.
pub fn escape_string_content(value: &str, out: &mut String) {
    for c in value.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
}

/// Writes `value` with the name escapes of 7.8 for the quote mark `quote`, with no quote marks.
pub fn escape_name_content(value: &str, quote: char, out: &mut String) {
    for c in value.chars() {
        if c == '\\' || c == quote {
            out.push('\\');
        }
        out.push(c);
    }
}

/// Gives `value` as a string literal: `'` and the string escapes of 7.8.
pub fn quote_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    escape_string_content(value, &mut out);
    out.push('\'');
    out
}

/// Gives `value` as a literal of the format `sqlstring`: `'` is written `''` and `\` is written `\\`.
pub fn quote_sqlstring(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Gives `value` as a quoted name with the quote mark `quote` (`` ` `` or `"`).
pub fn quote_name(value: &str, quote: char) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    escape_name_content(value, quote, &mut out);
    out.push(quote);
    out
}

/// Writes `value` with the escape of a PostgreSQL standard string (7.21), with no quote marks:
/// `'` becomes `''`, and a backslash stays.
pub fn escape_pg_string_content(value: &str, out: &mut String) {
    for c in value.chars() {
        if c == '\'' {
            out.push('\'');
        }
        out.push(c);
    }
}

/// Writes `value` with the escape of a PostgreSQL `E` string (7.21), with no quote marks: `\` becomes
/// `\\`, and `'` becomes `''`.
pub fn escape_pg_estring_content(value: &str, out: &mut String) {
    for c in value.chars() {
        if c == '\\' || c == '\'' {
            out.push(c);
        }
        out.push(c);
    }
}

/// Writes `value` with the escape of a PostgreSQL quoted name (7.21), with no quote marks: `"` becomes `""`.
pub fn escape_pg_name_content(value: &str, out: &mut String) {
    for c in value.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
}

/// Gives `value` as a PostgreSQL standard string literal (7.21).
pub fn quote_pg_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    escape_pg_string_content(value, &mut out);
    out.push('\'');
    out
}

/// Gives `value` as a PostgreSQL quoted name (7.21).
pub fn quote_pg_name(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    escape_pg_name_content(value, &mut out);
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::dialect::Dialect;
    use crate::sql::lexer::{TokenKind, lex_all, lex_all_for};

    #[test]
    fn string_escapes() {
        assert_eq!(quote_string("abc"), "'abc'");
        assert_eq!(quote_string(""), "''");
        assert_eq!(quote_string("o'rei\\lly"), "'o\\'rei\\\\lly'");
        assert_eq!(quote_string("' OR 1=1 --"), "'\\' OR 1=1 --'");
        assert_eq!(quote_string("abc\\"), "'abc\\\\'");
        assert_eq!(quote_string("caf\u{e9}"), "'caf\u{e9}'");
    }

    #[test]
    fn name_escapes() {
        assert_eq!(quote_name("a", '`'), "`a`");
        assert_eq!(quote_name("a`b", '`'), "`a\\`b`");
        assert_eq!(quote_name("a\\b", '`'), "`a\\\\b`");
        assert_eq!(quote_name("x\\` UNION SELECT 1 --", '`'), "`x\\\\\\` UNION SELECT 1 --`");
        assert_eq!(quote_name("", '`'), "``");
        assert_eq!(quote_name("a\"b", '"'), "\"a\\\"b\"");
        assert_eq!(quote_name("a`b", '"'), "\"a`b\"");
    }

    #[test]
    fn sqlstring_escapes() {
        assert_eq!(quote_sqlstring("a'b\\c"), "'a''b\\\\c'");
        assert_eq!(quote_sqlstring(""), "''");
    }

    #[test]
    fn one_token_whatever_the_text_holds() {
        for text in ["a'b", "\\", "\\'", "x\\\\'y", "--", "/*", "`", "\"", "'", "''", "\\\\\\"] {
            for quoted in [quote_string(text), quote_sqlstring(text)] {
                let t = lex_all(&quoted);
                assert_eq!(t.len(), 1, "{quoted:?}");
                assert!(t[0].kind == TokenKind::String && t[0].closed);
            }
            for q in ['`', '"'] {
                let t = lex_all(&quote_name(text, q));
                assert_eq!(t.len(), 1);
                assert!(t[0].kind == TokenKind::QuotedName && t[0].closed);
            }
        }
    }

    #[test]
    fn pg_escapes() {
        assert_eq!(quote_pg_string("abc"), "'abc'");
        assert_eq!(quote_pg_string(""), "''");
        assert_eq!(quote_pg_string("o'rei\\lly"), "'o''rei\\lly'");
        assert_eq!(quote_pg_name("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_pg_name("a\\b"), "\"a\\b\"");
        let mut e = String::new();
        escape_pg_estring_content("a\\'b", &mut e);
        assert_eq!(e, "a\\\\''b");
    }

    #[test]
    fn pg_one_token_whatever_the_text_holds() {
        let texts = [
            "a'b",
            "\\",
            "\\'",
            "x\\\\'y",
            "--",
            "/*",
            "`",
            "\"",
            "'",
            "''",
            "\\\\\\",
            "$$",
            "$a$",
            "U&",
            "\u{a0}",
            "\\'' OR 1=1 --",
        ];
        for text in texts {
            let q = quote_pg_string(text);
            let t = lex_all_for(Dialect::Postgres, &q);
            assert!(t.len() == 1 && t[0].kind == TokenKind::String && t[0].closed, "{q:?}");
            let mut inner = String::new();
            escape_pg_estring_content(text, &mut inner);
            let e = format!("E'{inner}'");
            let t = lex_all_for(Dialect::Postgres, &e);
            assert!(t.len() == 1 && t[0].kind == TokenKind::EString && t[0].closed, "{e:?}");
            let n = quote_pg_name(text);
            let t = lex_all_for(Dialect::Postgres, &n);
            assert!(t.len() == 1 && t[0].kind == TokenKind::QuotedName && t[0].closed, "{n:?}");
        }
    }
}
