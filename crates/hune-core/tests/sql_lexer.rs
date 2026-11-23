// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the ClickHouse lexer (7.5) on the shared vectors of ClickHouse Dash.
//!
//! The vectors are `tests/frontend/model/sql-lex-vectors.json` of ClickHouse Dash at the commit
//! `e970694e5e0b17b07cf187f3128af93fa1a82b76`, copied without change. The rules of `test_lexer_rules`
//! of `tests/native/sql_test.cpp` of the same commit are in `sql_lexer_rules.rs`.
//!
//! The file gives, for each text, the answers of the lexer of the page. The lexer of the backend
//! gives the same answers, except for the texts that the file marks `"differs": true`. Hune
//! follows the backend rules, so for these texts the answers MUST differ. This is the test of the
//! C++ file: `same != differs`.
//!
//! Hune does not follow the backend in one rule: a line comment ends at LF only, as in ClickHouse
//! 26.8.34.13 (7.5). The list `HUNE_OVERRIDES` gives the vectors that this rule changes, with the
//! reason. For them, Hune gives the answer of the page.
//!
//! Not run: the members `statements` and `mask`. They test the cursor functions of the editor
//! (the statement around a cursor, the mask of strings and comments), which Hune does not have.

use hune_core::sql::{Token, TokenKind, lex_all, utf16_offset};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/fixtures/sql-lex-vectors.json");

/// One class for each UTF-16 unit: `S` string or double-quoted name, `C` comment, `?` LF, `.` else.
fn classes_of_lexer(src: &str) -> String {
    let units: usize = src.chars().map(char::len_utf16).sum();
    let mut out = vec![b'.'; units];
    for t in lex_all(src) {
        let class = match t.kind {
            TokenKind::String => b'S',
            TokenKind::QuotedName if src.as_bytes()[t.start] == b'"' => b'S',
            TokenKind::LineComment | TokenKind::BlockComment => b'C',
            _ => continue,
        };
        for slot in &mut out[utf16_offset(src, t.start)..utf16_offset(src, t.end)] {
            *slot = class;
        }
    }
    mark_line_feeds(src, &mut out);
    String::from_utf8(out).unwrap()
}

fn mark_line_feeds(src: &str, out: &mut [u8]) {
    let mut unit = 0;
    for c in src.chars() {
        if c == '\n' {
            out[unit] = b'?';
        }
        unit += c.len_utf16();
    }
}

fn expected_classes(vector: &Value, src: &str) -> String {
    let units: usize = src.chars().map(char::len_utf16).sum();
    let mut out = vec![b'.'; units];
    for span in vector["spans"].as_array().unwrap() {
        let class = match span[2].as_str().unwrap() {
            "string" | "dquote" => b'S',
            "line" | "block" => b'C',
            _ => continue,
        };
        let (a, b) = (span[0].as_u64().unwrap() as usize, span[1].as_u64().unwrap() as usize);
        for slot in &mut out[a..b] {
            *slot = class;
        }
    }
    mark_line_feeds(src, &mut out);
    String::from_utf8(out).unwrap()
}

/// The pieces of a script between its `;` as UTF-16 `[start, end)` pairs.
fn split_of_lexer(src: &str) -> Vec<usize> {
    let units: usize = src.chars().map(char::len_utf16).sum();
    let mut out = vec![];
    let mut start = 0;
    for t in lex_all(src) {
        if t.kind == TokenKind::Punct && src.as_bytes()[t.start] == b';' {
            let at = utf16_offset(src, t.start);
            out.extend([start, at]);
            start = at + 1;
        }
    }
    out.extend([start, units]);
    out
}

fn expected_split(vector: &Value) -> Vec<usize> {
    vector["split"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| [p[0].as_u64().unwrap() as usize, p[1].as_u64().unwrap() as usize])
        .collect()
}

/// The vectors that the file marks `"differs": true` and where Hune gives the answer of the page: the text and the
/// reason. The backend of ClickHouse Dash ends a line comment at CR, and ClickHouse 26.8.34.13 does not (7.5).
const HUNE_OVERRIDES: [(&str, &str); 2] = [
    (
        "select 1 -- a\r\n; x",
        "ClickHouse 26.8.34.13 ends a line comment at LF only: the CR stays in the comment, as the page reads it",
    ),
    (
        "select 1 # a\r; b\n; c",
        "ClickHouse 26.8.34.13 ends a line comment at LF only: the CR and the `;` stay in the comment, as the page \
         reads it",
    ),
];

#[test]
fn vectors_of_clickhouse_dash() {
    let doc: Value = serde_json::from_str(VECTORS).unwrap();
    let vectors = doc.as_array().unwrap();
    assert!(vectors.len() >= 35);
    let (mut agreeing, mut differing, mut overridden) = (0, 0, 0);
    for v in vectors {
        let src = v["sql"].as_str().unwrap();
        let marked = v.get("differs").and_then(Value::as_bool).unwrap_or(false);
        let over = HUNE_OVERRIDES.iter().find(|(text, _)| *text == src);
        if let Some((_, reason)) = over {
            assert!(marked, "an override names a vector that the file does not mark: {src:?} ({reason})");
            overridden += 1;
        }
        let differs = marked && over.is_none();
        let same = classes_of_lexer(src) == expected_classes(v, src) && split_of_lexer(src) == expected_split(v);
        assert_ne!(same, differs, "vector {src:?} ({})", v["note"]);
        if differs {
            differing += 1;
        } else {
            agreeing += 1;
        }
    }
    assert_eq!(overridden, HUNE_OVERRIDES.len(), "each override names a vector of the file");
    assert!(agreeing >= 25 && differing >= 4, "{agreeing} agree, {differing} differ");
}

/// For the vectors that differ, the lexer of the backend gives the answer that the note of the
/// vector names. Each case here is the backend rule that differs from the page.
#[test]
fn backend_rules_where_the_vectors_differ() {
    let kinds = |s: &str| -> Vec<(TokenKind, bool)> { lex_all(s).iter().map(|t| (t.kind, t.closed)).collect() };
    // A # is a comment only before a blank or `!`.
    assert!(kinds("select 1 #a; b\n; select 2").iter().any(|k| k.0 == TokenKind::Unknown));
    // A block comment nests.
    let t = lex_all("/* a /* b */ ; c */ select 1");
    assert_eq!((t[0].kind, t[0].end), (TokenKind::BlockComment, 19));
    // $$ ... $$ is a heredoc.
    let t: Vec<Token> = lex_all("select $$a;'b$$; select 2");
    assert_eq!(t[2].kind, TokenKind::Heredoc);
    // A backslash escapes in a name in backticks.
    let t = lex_all("select `a\\`; select 2");
    assert_eq!((t[2].kind, t[2].closed), (TokenKind::QuotedName, false));
    // The overrides of Hune (`HUNE_OVERRIDES`): a CR does not end a line comment, as in ClickHouse 26.8.34.13 (7.5).
    let t = lex_all("select 1 -- a\r\n; x");
    assert_eq!((t[4].kind, t[4].end), (TokenKind::LineComment, 14));
    let t = lex_all("select 1 # a\r; b\n; c");
    assert_eq!((t[4].kind, t[4].end), (TokenKind::LineComment, 16));
    // `//` starts a line comment of ClickHouse 26.8.34.13. The vectors of the file do not test it.
    let t = lex_all("select 1 // a;\n; x");
    assert_eq!((t[4].kind, t[4].end), (TokenKind::LineComment, 14));
}

#[test]
fn the_vector_file_is_the_unchanged_copy() {
    // The policy check of 16.18 compares the SHA-256 with `tests/images.lock`. Here: the file is JSON
    // with the 39 vectors of the commit e970694e.
    let doc: Value = serde_json::from_str(VECTORS).unwrap();
    assert_eq!(doc.as_array().unwrap().len(), 39);
}
