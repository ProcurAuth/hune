//! The vectors of `test_lexer_rules` and `test_literals` of `tests/native/sql_test.cpp` of
//! ClickHouse Dash at the commit `e970694e5e0b17b07cf187f3128af93fa1a82b76`, and the table of 7.5.
//!
//! Skipped from `test_literals`: the cases of `quote_string` and `quote_ident` that write a control
//! character as an escape (`\n`, `\r`, `\t`, `\0`, `\x01`). Hune refuses a control character in a
//! value (7.7) and writes only the escapes of 7.8. The cases of `hex_id_array`, `quote_list`,
//! `like_escape` and `is_hex_trace_id` are functions that Hune does not have. `quote_qualified` too.

use hune_core::sql::escape::{quote_name, quote_string};
use hune_core::sql::{TokenKind, lex_all, utf16_offset};

fn kinds(sql: &str) -> String {
    lex_all(sql)
        .iter()
        .map(|t| match (t.kind, t.closed) {
            (TokenKind::Blank, _) => '_',
            (TokenKind::LineComment, _) => 'L',
            (TokenKind::BlockComment, true) => 'B',
            (TokenKind::BlockComment, false) => 'b',
            (TokenKind::Word, _) => 'W',
            (TokenKind::QuotedName, true) => 'Q',
            (TokenKind::QuotedName, false) => 'q',
            (TokenKind::String, true) => 'S',
            (TokenKind::String, false) => 's',
            (TokenKind::Heredoc, true) => 'H',
            (TokenKind::Heredoc, false) => 'h',
            (TokenKind::Number, _) => 'N',
            (TokenKind::Punct, _) => 'P',
            (TokenKind::Unknown, _) => 'U',
            (TokenKind::EString, true) => 'E',
            (TokenKind::EString, false) => 'e',
            (TokenKind::PrefixedString, true) => 'S',
            (TokenKind::PrefixedString, false) => 's',
            (TokenKind::Operator, _) => 'O',
            (TokenKind::Parameter, _) => 'R',
        })
        .collect()
}

#[test]
fn lexer_rules() {
    let cases: &[(&str, &str)] = &[
        ("SELECT 1", "W_N"),
        ("a.b", "WPW"),
        ("f(x, 'y')", "WPWP_SP"),
        ("1--2", "NL"),
        ("1 -- x\n2", "N_L_N"),
        ("1 # x\n2", "N_L_N"),
        ("1 #! x", "N_L"),
        ("1 #x", "N_UW"),
        ("1 /* a /* b */ c */ 2", "N_B_N"),
        ("1 /* a /* b */", "N_b"),
        ("/**/", "B"),
        ("'a\\'b'", "S"),
        ("'a''b'", "S"),
        ("'abc", "s"),
        ("`a\\`b`", "Q"),
        ("\"a\"\"b\"", "Q"),
        ("`a", "q"),
        ("'a\\", "s"),
        ("$$a'b$$", "H"),
        ("$tag$a$$b$tag$", "H"),
        ("$$abc", "h"),
        ("a$b", "W"),
        ("$1", "W"),
        ("0x1F 1e3 1_000 .5 1.e3 0b101 1e-5 0x1p-3", "N_N_N_N_N_N_N_N"),
        ("1-2", "NPN"),
        ("KILL\u{a0}QUERY", "W_W"),
        ("KILL\u{200b}QUERY", "W_W"),
        ("KILL\u{feff}QUERY", "W_W"),
        ("KILL\u{3000}QUERY", "W_W"),
        ("\u{feff}SELECT", "_W"),
    ];
    for (sql, want) in cases {
        assert_eq!(&kinds(sql), want, "{sql:?}");
    }
}

#[test]
fn tokens_joined_in_order_are_the_text() {
    for sql in ["SELECT 'a' /* c */ FROM `t` -- x\nWHERE a = $$b$$ #! y", "'", "\\", "/*", "$", "\u{ff}\u{fe}", ""] {
        let mut joined = String::new();
        let mut at = 0;
        for t in lex_all(sql) {
            assert_eq!(t.start, at);
            assert!(t.end > t.start);
            joined.push_str(t.text(sql));
            at = t.end;
        }
        assert_eq!(joined, sql);
    }
    assert_eq!(utf16_offset("abc", 2), 2);
    assert_eq!(utf16_offset("a\u{e9}b", 3), 2);
    assert_eq!(utf16_offset("a\u{1f600}b", 5), 3);
}

#[test]
fn literals() {
    assert_eq!(quote_string("abc"), "'abc'");
    assert_eq!(quote_string(""), "''");
    assert_eq!(quote_string("o'rei\\lly"), "'o\\'rei\\\\lly'");
    assert_eq!(quote_string("' OR 1=1 --"), "'\\' OR 1=1 --'");
    assert_eq!(quote_string("abc\\"), "'abc\\\\'");
    assert_eq!(quote_string("caf\u{e9}"), "'caf\u{e9}'");
    assert_eq!(quote_name("a", '`'), "`a`");
    assert_eq!(quote_name("a`b", '`'), "`a\\`b`");
    assert_eq!(quote_name("a\\b", '`'), "`a\\\\b`");
    assert_eq!(quote_name("x\\` UNION SELECT 1 --", '`'), "`x\\\\\\` UNION SELECT 1 --`");
    assert_eq!(quote_name("", '`'), "``");
    for text in ["a'b", "\\", "\\'", "x\\\\'y", "--", "/*", "//", "#!", "$1$", "`", "\""] {
        let q = lex_all(&quote_string(text));
        assert!(q.len() == 1 && q[0].kind == TokenKind::String && q[0].closed);
        let n = lex_all(&quote_name(text, '`'));
        assert!(n.len() == 1 && n[0].kind == TokenKind::QuotedName && n[0].closed);
    }
}
