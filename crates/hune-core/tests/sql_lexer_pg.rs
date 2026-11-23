// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the PostgreSQL lexer (7.20): the vectors, the rules one by one, the seeded random
//! round trips and the unclosed forms.
//!
//! The vectors are `tests/fixtures/sql-lex-vectors-pg.json`. Each vector has the text `sql`, the
//! letters `tokens` in the notation of 7.5 and 7.20, the first lexer fault `error` (or null) and a
//! `note`. The letters are written by hand from the rules of 7.20 and from `scan.l` of PostgreSQL
//! 15 to 18. They are not the output of the lexer. One line of `tests/images.lock` holds the
//! SHA-256 of the file, so a silent edit of a vector fails the test `the_vector_file_is_pinned`.
//!
//! Run one seed again: `HUNE_TEST_SEED=4242 cargo test -p hune-core --locked sql_lexer_pg`.
//! The variables `HUNE_TEST_SEEDS` and `HUNE_TEST_CASES` set a longer run.

use hune_core::sql::{Dialect, LexErrorKind, Token, TokenKind, lex_all, lex_all_for, lex_for};
use serde_json::Value;
use sha2::{Digest, Sha256};

const VECTORS: &str = include_str!("../../../tests/fixtures/sql-lex-vectors-pg.json");
const LOCK: &str = include_str!("../../../tests/images.lock");

fn pg(src: &str) -> Vec<Token> {
    lex_all_for(Dialect::Postgres, src)
}

/// The letters of 7.5 and 7.20 for a token.
fn letter(t: &Token) -> char {
    let (open, shut) = match t.kind {
        TokenKind::Blank => return '_',
        TokenKind::LineComment => return 'L',
        TokenKind::BlockComment => ('b', 'B'),
        TokenKind::Word => return 'W',
        TokenKind::QuotedName => ('q', 'Q'),
        TokenKind::String | TokenKind::PrefixedString => ('s', 'S'),
        TokenKind::Heredoc => ('h', 'H'),
        TokenKind::Number => return 'N',
        TokenKind::Punct => return 'P',
        TokenKind::Unknown => return 'U',
        TokenKind::EString => ('e', 'E'),
        TokenKind::Operator => return 'O',
        TokenKind::Parameter => return 'R',
    };
    if t.closed { shut } else { open }
}

fn letters(src: &str) -> String {
    pg(src).iter().map(letter).collect()
}

fn assert_cover(src: &str, tokens: &[Token]) {
    let mut at = 0;
    for t in tokens {
        assert_eq!(t.start, at, "gap in {src:?}");
        assert!(t.end > t.start, "empty token in {src:?}");
        assert!(src.is_char_boundary(t.start) && src.is_char_boundary(t.end), "split character in {src:?}");
        at = t.end;
    }
    assert_eq!(at, src.len(), "the tokens do not reach the end of {src:?}");
    let joined: String = tokens.iter().map(|t| t.text(src)).collect();
    assert_eq!(joined, src);
}

fn error_name(kind: Option<LexErrorKind>) -> Option<&'static str> {
    kind.map(|k| match k {
        LexErrorKind::UnknownToken => "UnknownToken",
        LexErrorKind::UnclosedBlockComment => "UnclosedBlockComment",
        LexErrorKind::UnclosedString => "UnclosedString",
        LexErrorKind::UnclosedQuotedName => "UnclosedQuotedName",
        LexErrorKind::UnclosedHeredoc => "UnclosedHeredoc",
    })
}

// ---- the vectors ----

#[test]
fn the_vectors_of_7_20() {
    let doc: Value = serde_json::from_str(VECTORS).unwrap();
    let list = doc.as_array().unwrap();
    assert!(list.len() >= 150, "{} vectors", list.len());
    for v in list {
        let sql = v["sql"].as_str().unwrap();
        let want = v["tokens"].as_str().unwrap();
        let note = v["note"].as_str().unwrap();
        let tokens = pg(sql);
        assert_cover(sql, &tokens);
        assert_eq!(letters(sql), want, "{sql:?}: {note}");
        let got = lex_for(Dialect::Postgres, sql).err().map(|e| e.kind);
        assert_eq!(error_name(got), v["error"].as_str(), "error of {sql:?}: {note}");
        if let Some(e) = lex_for(Dialect::Postgres, sql).err() {
            // The position is the start of the first faulty token.
            let first = tokens.iter().find(|t| !t.closed).unwrap();
            assert_eq!(e.position.offset, first.start, "{sql:?}");
        }
    }
}

#[test]
fn the_table_of_7_20() {
    let cases: &[(&str, &str)] = &[
        ("SELECT 1", "W_N"),
        ("'a''b'", "S"),
        ("1 # x", "N_O_W"),
        ("$1", "R"),
        ("$tag$a$$b$tag$", "H"),
        ("$$a'b$$", "H"),
        ("a$b", "W"),
        ("U&\"a\"", "Q"),
        ("x@-1", "WON"),
        ("a<-b", "WOOW"),
        ("'a' 'b'", "S_S"),
        ("1\u{a0}2", "NW"),
        // The table of 7.20 has `SWs` here. The rule of the prefix and `scan.l` give `Ss`: `b'` starts a bit string.
        ("'a\\'b'", "Ss"),
        ("E'a\\'b'", "E"),
        ("1 --x", "N_L"),
        ("/* a /* b */ c */", "B"),
    ];
    for (src, want) in cases {
        assert_eq!(&letters(src), want, "{src:?}");
    }
}

#[test]
fn the_vector_file_is_pinned() {
    let line = LOCK
        .lines()
        .find(|l| l.starts_with("sql-lex-vectors-pg "))
        .expect("images.lock has no line sql-lex-vectors-pg");
    let want = line.split("@sha256:").nth(1).and_then(|r| r.split_whitespace().next()).expect("no SHA-256");
    let got: String = Sha256::digest(VECTORS.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(want, got, "tests/fixtures/sql-lex-vectors-pg.json changed: update its line in tests/images.lock");
}

// ---- the rules, one by one ----

fn kinds(src: &str) -> Vec<(TokenKind, bool)> {
    pg(src).iter().map(|t| (t.kind, t.closed)).collect()
}

#[test]
fn token_kinds_of_the_dialect() {
    assert_eq!(kinds("E'a'"), [(TokenKind::EString, true)]);
    assert_eq!(kinds("e'a"), [(TokenKind::EString, false)]);
    for s in ["B'1'", "b'1'", "X'1'", "x'1'", "N'a'", "n'a'", "U&'a'", "u&'a'"] {
        assert_eq!(kinds(s), [(TokenKind::PrefixedString, true)], "{s}");
    }
    // `U&"..."` is a quoted name, and its prefix is part of the token.
    assert_eq!(kinds("U&\"a\""), [(TokenKind::QuotedName, true)]);
    assert_eq!(pg("U&\"a\"")[0].end, 5);
    // A dollar quote reuses `Heredoc`.
    assert_eq!(kinds("$a$x$a$"), [(TokenKind::Heredoc, true)]);
    assert_eq!(kinds("$1"), [(TokenKind::Parameter, true)]);
    assert_eq!(kinds("+"), [(TokenKind::Operator, true)]);
    // The prefix is part of the token: the token starts at the prefix letter.
    let t = pg("x U&'a'");
    assert_eq!((t[2].kind, t[2].start, t[2].end), (TokenKind::PrefixedString, 2, 7));
}

#[test]
fn clickhouse_path_is_not_changed() {
    // The same texts give the old answers with the ClickHouse lexer (7.5) and the new kinds never occur.
    let ch = |s: &str| -> Vec<TokenKind> { lex_all(s).iter().map(|t| t.kind).collect() };
    assert_eq!(ch("$1"), [TokenKind::Word]);
    assert_eq!(ch("a#b"), [TokenKind::Word, TokenKind::Unknown, TokenKind::Word]);
    assert_eq!(ch("E'a'"), [TokenKind::Word, TokenKind::String]);
    assert_eq!(ch("a+b"), [TokenKind::Word, TokenKind::Punct, TokenKind::Word]);
    assert_eq!(ch("'a\\'b'"), [TokenKind::String]);
    assert_eq!(ch("KILL\u{a0}QUERY").len(), 3);
    for s in ["E'a' $1 + U&'x' B'1' -- c\n 'a'\n'b'", "a#b", "$$x$$", "\"a\\\"b\""] {
        let a: Vec<_> = lex_all(s).iter().map(|t| (t.kind, t.start, t.end, t.closed)).collect();
        let b: Vec<_> =
            lex_all_for(Dialect::ClickHouse, s).iter().map(|t| (t.kind, t.start, t.end, t.closed)).collect();
        assert_eq!(a, b);
        for t in lex_all(s) {
            assert!(!matches!(
                t.kind,
                TokenKind::EString | TokenKind::PrefixedString | TokenKind::Operator | TokenKind::Parameter
            ));
        }
    }
}

#[test]
fn a_hash_starts_no_comment() {
    assert!(lex_for(Dialect::Postgres, "select 1 # x").is_ok());
    assert_eq!(letters("a #! b"), "W_O_W");
    assert_eq!(letters("a #x"), "W_OW");
    assert!(lex_for(Dialect::ClickHouse, "select 1 #x").is_err());
}

#[test]
fn unicode_spaces_are_word_bytes() {
    for c in [
        '\u{85}', '\u{a0}', '\u{180e}', '\u{200b}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{2060}',
        '\u{3000}', '\u{feff}',
    ] {
        assert_eq!(letters(&format!("KILL{c}QUERY")), "W", "U+{:04X}", c as u32);
        assert_eq!(letters(&format!("{c}SELECT")), "W");
    }
    for c in [' ', '\t', '\n', '\r', '\u{c}', '\u{b}'] {
        assert_eq!(letters(&format!("a{c}b")), "W_W", "U+{:04X}", c as u32);
    }
}

#[test]
fn string_escape_rules() {
    // Standard strings: the backslash is ordinary, the quote is doubled.
    assert_eq!(letters(r"'\'"), "S");
    assert_eq!(letters(r"'\\'"), "S");
    assert_eq!(letters(r"'\'';"), "s");
    assert_eq!(letters(r"'\' y'"), "S_Ws");
    assert_eq!(letters(r"'a\' b"), "S_W");
    // E strings: a backslash escapes the next byte.
    assert_eq!(letters(r"E'\''"), "E");
    assert_eq!(letters(r"E'\\'"), "E");
    assert_eq!(letters(r"E'\\\''"), "E");
    assert_eq!(letters(r"E'\'"), "e");
    assert_eq!(letters(r"e'\\\'"), "e");
    // A name: no backslash escape for any prefix.
    assert_eq!(letters(r#""\""#), "Q");
    assert_eq!(letters(r#"U&"\""#), "Q");
}

#[test]
fn dollar_quote_tag_rules() {
    assert_eq!(letters("$$x$$"), "H");
    assert_eq!(letters("$a$x$a$"), "H");
    assert_eq!(letters("$a_1$x$a_1$"), "H");
    // The tag has no `$` and does not start with a digit.
    assert_eq!(letters("$a$b$ x $a$b$"), "HW");
    assert_eq!(letters("$1$x$1$"), "Rh");
    assert_eq!(letters("$1a$x$1a$"), "RW");
    assert_eq!(letters("$-$x$-$"), "WOh");
    assert_eq!(letters("$ $x$ $"), "W_h");
    // The closing tag is the same bytes, in the same case.
    assert_eq!(letters("$a$x$A$"), "h");
    assert_eq!(letters("$a$x$a"), "h");
    // The first closing delimiter ends the quote. Another delimiter inside is text.
    assert_eq!(letters("$a$ $b$ $a$ $a$"), "H_h");
    assert_eq!(letters("$a$ $b$ $a$"), "H");
    // A dollar quote does not start inside a word.
    assert_eq!(letters("x$$ y $$"), "W_W_h");
    assert_eq!(letters("$$ 'unclosed"), "h");
}

#[test]
fn comments_nest_and_hide_everything() {
    assert_eq!(letters("/* a /* b */ c */"), "B");
    assert_eq!(letters("/* a /* b */ c"), "b");
    assert_eq!(letters("/* ' \" $$ -- */"), "B");
    assert_eq!(letters("-- ' \" $$ /*\nx"), "L_W");
    assert_eq!(letters("-- a\r\nb"), "L_W");
    // A block comment mark inside a string or a name is text.
    assert_eq!(letters("'/*' */"), "S_O");
    assert_eq!(letters("\"/*\" */"), "Q_O");
}

#[test]
fn operator_runs() {
    let cases: &[(&str, &[&str])] = &[
        ("a<=b", &["a", "<=", "b"]),
        ("a->>b", &["a", "->>", "b"]),
        ("a<-b", &["a", "<", "-", "b"]),
        ("a=-1", &["a", "=", "-", "1"]),
        ("a@-1", &["a", "@-", "1"]),
        ("a|-1", &["a", "|-", "1"]),
        ("a+/*x*/b", &["a", "+", "/*x*/", "b"]),
        ("a+--x", &["a", "+", "--x"]),
        ("a*/b", &["a", "*/", "b"]),
        ("a+-+b", &["a", "+", "-", "+", "b"]),
        ("a!-b", &["a", "!-", "b"]),
        ("a#-b", &["a", "#-", "b"]),
        ("a?-b", &["a", "?-", "b"]),
        ("a`-b", &["a", "`-", "b"]),
        ("a%+b", &["a", "%+", "b"]),
        ("a<>b", &["a", "<>", "b"]),
        ("a&&b", &["a", "&&", "b"]),
    ];
    for (src, want) in cases {
        let got: Vec<&str> = pg(src).iter().map(|t| t.text(src)).collect();
        assert_eq!(&got, want, "{src:?}");
    }
}

#[test]
fn string_continuation_follows_scan_l() {
    // A line end between two quotes makes one literal for PostgreSQL. The lexer gives one token for
    // each part, and the second part has the kind and the mode of the first part.
    assert_eq!(letters("'a'\n'b'"), "S_S");
    assert_eq!(letters("E'a'\n'\\''"), "E_E");
    let src = "select 'a'\n  'b' from t";
    let t = pg(src);
    assert_eq!((t[2].text(src), t[4].text(src)), ("'a'", "'b'"));
    // The part after the line end starts at its quote mark: it has no prefix.
    assert_eq!(t[4].content_range(src), Some((14, 15)));
    // The desync that the rule closes: PostgreSQL reads `'\''` as part of the E string, so the text
    // after it is code. A lexer that read the second part as a standard string would see a string
    // that PostgreSQL does not see.
    let src = "E'a'\n'\\'' ; select 1 ; --";
    let t = pg(src);
    assert_eq!((t[0].kind, t[2].kind), (TokenKind::EString, TokenKind::EString));
    assert_eq!(t[2].text(src), "'\\''");
    assert_eq!(letters(src), "E_E_P_W_N_P_L");
    // Without a line end there is no continuation: the second part is a standard string, and the
    // doubled quote at its end leaves it open.
    assert_eq!(letters("E'a' '\\''"), "E_s");
    // A comment of the gap is a comment token: a placeholder in it is not read.
    let src = "'a' -- $x\n'b'";
    let t = pg(src);
    assert_eq!(t[2].kind, TokenKind::LineComment);
    assert_eq!(t[2].text(src), "-- $x");
}

#[test]
fn content_ranges() {
    let cases: &[(&str, Dialect, &str)] = &[
        ("'ab'", Dialect::Postgres, "ab"),
        ("E'a\\'b'", Dialect::Postgres, "a\\'b"),
        ("e'ab", Dialect::Postgres, "ab"),
        ("B'01'", Dialect::Postgres, "01"),
        ("x'1F'", Dialect::Postgres, "1F"),
        ("N'a'", Dialect::Postgres, "a"),
        ("U&'a'", Dialect::Postgres, "a"),
        ("u&'\\0041'", Dialect::Postgres, "\\0041"),
        ("\"a\"\"b\"", Dialect::Postgres, "a\"\"b"),
        ("U&\"a\"", Dialect::Postgres, "a"),
        ("u&\"a", Dialect::Postgres, "a"),
        ("''", Dialect::Postgres, ""),
        ("'ab'", Dialect::ClickHouse, "ab"),
        ("`a`", Dialect::ClickHouse, "a"),
        ("\"a\"", Dialect::ClickHouse, "a"),
    ];
    for (src, dialect, want) in cases {
        let t = lex_all_for(*dialect, src);
        assert_eq!(t.len(), 1, "{src:?}");
        let (from, to) = t[0].content_range(src).unwrap();
        assert_eq!(&src[from..to], *want, "{src:?}");
    }
    for src in ["a", "1", "$$a$$", "-- c", " "] {
        assert_eq!(pg(src)[0].content_range(src), None, "{src:?}");
    }
}

#[test]
fn unclosed_forms() {
    let cases: &[(&str, LexErrorKind)] = &[
        ("'abc", LexErrorKind::UnclosedString),
        ("select '", LexErrorKind::UnclosedString),
        ("E'abc", LexErrorKind::UnclosedString),
        ("E'abc\\", LexErrorKind::UnclosedString),
        ("e'abc\\'", LexErrorKind::UnclosedString),
        ("B'1", LexErrorKind::UnclosedString),
        ("X'1F", LexErrorKind::UnclosedString),
        ("N'a", LexErrorKind::UnclosedString),
        ("U&'a", LexErrorKind::UnclosedString),
        ("'a'\n'b", LexErrorKind::UnclosedString),
        ("E'a'\n'b\\'", LexErrorKind::UnclosedString),
        ("\"abc", LexErrorKind::UnclosedQuotedName),
        ("U&\"abc", LexErrorKind::UnclosedQuotedName),
        ("\"a\"\"", LexErrorKind::UnclosedQuotedName),
        ("/* a", LexErrorKind::UnclosedBlockComment),
        ("/* a /* b */", LexErrorKind::UnclosedBlockComment),
        ("/*", LexErrorKind::UnclosedBlockComment),
        ("/*/", LexErrorKind::UnclosedBlockComment),
        ("$$", LexErrorKind::UnclosedHeredoc),
        ("$$ a", LexErrorKind::UnclosedHeredoc),
        ("$a$ b $a", LexErrorKind::UnclosedHeredoc),
        ("$a$ b $A$", LexErrorKind::UnclosedHeredoc),
    ];
    for (src, kind) in cases {
        let e = lex_for(Dialect::Postgres, src).unwrap_err();
        assert_eq!(e.kind, *kind, "{src:?}");
        let tokens = pg(src);
        assert!(tokens.iter().any(|t| !t.closed), "{src:?}");
        assert_cover(src, &tokens);
        // An open token runs to the end of the text.
        assert_eq!(tokens.last().unwrap().end, src.len());
    }
    // These closed forms are valid.
    for src in ["''", "\"\"", "$$$$", "/**/", "-- a", "'a'\n-- c\n'b'", "E''", "U&''", "U&\"\""] {
        assert!(lex_for(Dialect::Postgres, src).is_ok(), "{src:?}");
    }
    // The position of an error is the first byte of the token, in lines and columns.
    let e = lex_for(Dialect::Postgres, "select 1\n  from E'abc").unwrap_err();
    assert_eq!((e.position.line, e.position.column, e.position.offset), (2, 8, 16));
}

#[test]
fn every_prefix_in_every_case() {
    for p in ["b", "B", "x", "X", "n", "N", "u&", "U&"] {
        let src = format!("{p}'a''b'");
        let t = pg(&src);
        assert_eq!(t.len(), 1, "{src}");
        assert_eq!(t[0].kind, TokenKind::PrefixedString);
    }
    for p in ["e", "E"] {
        let t = pg(&format!("{p}'a\\'b'"));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].kind, TokenKind::EString);
    }
    // After a word byte, a digit, a `$` or a `.` that ends a number, the prefix is part of the word or the number.
    for src in ["aE'x'", "_E'x'", "1E'x'", "$E'x'", "\u{e9}E'x'", "0xE'x'"] {
        let t = pg(src);
        assert_eq!(t.len(), 2, "{src:?}");
        assert_eq!(t[1].kind, TokenKind::String, "{src:?}");
    }
    // After an operator byte or a punctuation, a prefix starts a token.
    for src in ["(E'x'", "=E'x'", ",E'x'", ";E'x'", "-E'x'"] {
        let t = pg(src);
        assert_eq!(t.last().unwrap().kind, TokenKind::EString, "{src:?}");
    }
}

// ---- seeded random tests ----

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn env_number(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn seeds() -> Vec<u64> {
    match env_number("HUNE_TEST_SEED") {
        Some(s) => vec![s],
        None => (1..=env_number("HUNE_TEST_SEEDS").unwrap_or(32)).collect(),
    }
}

fn cases() -> usize {
    env_number("HUNE_TEST_CASES").unwrap_or(2000) as usize
}

/// Hostile bytes of the dialect: every quote, escape, comment and dollar mark, blanks of every
/// kind, Unicode spaces, multibyte characters and a NUL byte.
const SOUP: &[&str] = &[
    "'",
    "''",
    "\"",
    "\"\"",
    "`",
    "\\",
    "$",
    "$$",
    "$a$",
    "$1",
    "$a",
    "${a}",
    "-",
    "--",
    "#",
    "/",
    "*",
    "/*",
    "*/",
    "+",
    "<",
    ">",
    "=",
    "!",
    "@",
    "%",
    "^",
    "&",
    "|",
    "?",
    "~",
    ":",
    ";",
    ",",
    ".",
    "(",
    ")",
    "[",
    "]",
    " ",
    "\n",
    "\r",
    "\t",
    "\u{b}",
    "\u{c}",
    "\u{a0}",
    "\u{3000}",
    "\u{e9}",
    "\u{1f600}",
    "\0",
    "a",
    "E",
    "e",
    "B",
    "X",
    "N",
    "U",
    "u",
    "U&",
    "1",
    "0x",
    "1e",
    "e'",
    "E'",
    "b'",
    "U&'",
    "U&\"",
    "x",
    "_",
    "0",
];

fn soup(rng: &mut Rng) -> String {
    let n = 1 + rng.below(24);
    (0..n).map(|_| *rng.pick(SOUP)).collect()
}

#[test]
fn random_soup_round_trips() {
    for seed in seeds() {
        let mut rng = Rng(seed);
        for case in 0..cases() {
            let src = soup(&mut rng);
            let tokens = pg(&src);
            assert_cover(&src, &tokens);
            // Each token read alone is the same token. The lexer has no memory between tokens,
            // except in a continuation, which is inside one token.
            for t in &tokens {
                let text = t.text(&src);
                // A part of a string after a line end has the mode of the first part: alone it is a standard string.
                if matches!(t.kind, TokenKind::EString | TokenKind::PrefixedString) && text.starts_with('\'') {
                    continue;
                }
                let alone = pg(text);
                assert_eq!(alone.len(), 1, "seed {seed} case {case}: {src:?} token {text:?} alone gives {alone:?}");
                assert_eq!((alone[0].kind, alone[0].closed), (t.kind, t.closed), "seed {seed} case {case}: {text:?}");
            }
            // `lex_for` agrees with the closed flags, and the ClickHouse path never gives a Postgres kind.
            let open = tokens.iter().any(|t| !t.closed);
            assert_eq!(lex_for(Dialect::Postgres, &src).is_err(), open, "{src:?}");
            for t in lex_all(&src) {
                assert!(!matches!(
                    t.kind,
                    TokenKind::EString | TokenKind::PrefixedString | TokenKind::Operator | TokenKind::Parameter
                ));
            }
            // Only the last token can be open, and nothing follows an open token.
            if let Some(i) = tokens.iter().position(|t| !t.closed) {
                assert_eq!(i, tokens.len() - 1, "{src:?}");
            }
        }
    }
}

/// A piece of a generated text, with the letter that the lexer must give.
struct Piece {
    text: String,
    letter: char,
}

fn text_of(rng: &mut Rng, alphabet: &[&str], max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(alphabet)).collect()
}

const CONTENT: &[&str] = &[
    "a",
    "b",
    " ",
    "\n",
    "\r",
    "\t",
    "\\",
    "--",
    "/*",
    "*/",
    "$$",
    "$a$",
    "$",
    "{",
    "}",
    "\u{e9}",
    "\u{1f600}",
    "\u{a0}",
    ";",
    "#",
    "`",
    "E'",
    "U&",
    "1",
    "&",
];

fn make_piece(rng: &mut Rng) -> Piece {
    match rng.below(14) {
        0 => {
            // A standard string: the quote is doubled, the backslash is ordinary.
            let body = text_of(rng, CONTENT, 8).replace('\'', "''");
            Piece { text: format!("'{body}'"), letter: 'S' }
        }
        1 => {
            // An E string: the backslash and the quote are escaped.
            let body = text_of(rng, CONTENT, 8);
            let body = match rng.below(3) {
                0 => body.replace('\\', "\\\\").replace('\'', "\\'"),
                1 => body.replace('\\', "\\\\").replace('\'', "''"),
                _ => body.replace('\\', "\\\\").replace('\'', "\\'") + "\\\\",
            };
            let e = *rng.pick(&["E", "e"]);
            Piece { text: format!("{e}'{body}'"), letter: 'E' }
        }
        2 => {
            let p = *rng.pick(&["B", "b", "X", "x", "N", "n", "U&", "u&"]);
            let body = text_of(rng, CONTENT, 8).replace('\'', "''");
            Piece { text: format!("{p}'{body}'"), letter: 'S' }
        }
        3 => {
            let body = text_of(rng, CONTENT, 8).replace('"', "\"\"");
            let p = *rng.pick(&["", "", "U&", "u&"]);
            Piece { text: format!("{p}\"{body}\""), letter: 'Q' }
        }
        4 => {
            // A dollar quote. The body holds no closing delimiter.
            let tag = rng.pick(&["", "a", "tag", "_x1", "\u{e9}", "A"]).to_string();
            let delim = format!("${tag}$");
            let mut body = text_of(rng, CONTENT, 10);
            while body.contains(&delim) || body.ends_with(&format!("${tag}")) && !tag.is_empty() {
                body.pop();
            }
            // A body that ends with `$` and a prefix of the delimiter can join the closing delimiter: avoid it.
            if body.ends_with('$') {
                body.push(' ');
            }
            Piece { text: format!("{delim}{body}{delim}"), letter: 'H' }
        }
        5 => {
            // A block comment with a nested comment.
            let inner = text_of(rng, &["a", " ", "'", "\"", "--", "$$", "\n", "\u{e9}"], 5);
            let text = match rng.below(3) {
                0 => format!("/*{inner}*/"),
                1 => format!("/*{inner}/*{inner}*/{inner}*/"),
                _ => format!("/*/*{inner}*/*/"),
            };
            Piece { text, letter: 'B' }
        }
        6 => {
            let body = text_of(rng, &["a", " ", "'", "\"", "$$", "/*", "\t", "\u{e9}", "\\", "--"], 6);
            Piece { text: format!("--{body}\n"), letter: 'L' }
        }
        7 => Piece { text: format!("${}", rng.below(100000)), letter: 'R' },
        8 => {
            let w = *rng.pick(&[
                "select",
                "a",
                "_x",
                "x1",
                "a$b",
                "\u{e9}t\u{e9}",
                "KILL\u{a0}QUERY",
                "E",
                "x$$y",
                "U",
                "\u{3000}",
            ]);
            Piece { text: w.to_owned(), letter: 'W' }
        }
        9 => {
            let n = *rng.pick(&["1", "42", "0x1F", "1e3", "1_000", ".5", "1.e3", "0b101", "1e-5", "0x1p-3", "1.5.6"]);
            Piece { text: n.to_owned(), letter: 'N' }
        }
        10 => {
            let o = *rng.pick(&["+", "<=", "->>", "@-", "|-", "!~*", "#", "||", "%", "^", "?", "`", "<>", "::"]);
            // `::` is two punctuation tokens: it is not used here. Give an operator instead.
            let o = if o == "::" { "=" } else { o };
            Piece { text: o.to_owned(), letter: 'O' }
        }
        11 => Piece { text: "(".to_owned(), letter: 'P' },
        12 => Piece { text: ".".to_owned(), letter: 'P' },
        _ => Piece { text: ",".to_owned(), letter: 'P' },
    }
}

/// The separators hold a semicolon, so a string never follows a string after a line end: that
/// would be a continuation. They give a blank before and after the piece.
const SEPARATORS: &[&str] = &[";", " ; ", "\n;\n", "\t;", ";\r\n", " ;\u{c}", ";\u{b}"];

#[test]
fn random_texts_from_pieces_give_the_pieces() {
    for seed in seeds() {
        let mut rng = Rng(seed ^ 0x5eed);
        for case in 0..cases() {
            let n = 1 + rng.below(6);
            let pieces: Vec<Piece> = (0..n).map(|_| make_piece(&mut rng)).collect();
            let mut src = String::new();
            for (i, p) in pieces.iter().enumerate() {
                if i > 0 {
                    src.push_str(rng.pick(SEPARATORS));
                }
                src.push_str(&p.text);
            }
            let tokens = pg(&src);
            assert_cover(&src, &tokens);
            // Remove the blanks and the separators: the pieces remain, in order.
            let mut at = 0;
            let mut found: Vec<(char, &str)> = Vec::new();
            while at < tokens.len() {
                let t = &tokens[at];
                let text = t.text(&src);
                let is_separator = match t.kind {
                    TokenKind::Blank => true,
                    TokenKind::Punct => text == ";",
                    _ => false,
                };
                if !is_separator {
                    found.push((letter(t), text));
                }
                at += 1;
            }
            // A line comment holds its own line end? No: the lexer ends it before the line end.
            let want: Vec<(char, String)> = pieces
                .iter()
                .map(|p| {
                    (p.letter, if p.letter == 'L' { p.text.trim_end_matches('\n').to_owned() } else { p.text.clone() })
                })
                .collect();
            // A word `W` piece next to nothing else is one token. A `(`, `.` or `,` is one `P` token.
            let got: Vec<(char, String)> = found.iter().map(|(l, t)| (*l, (*t).to_owned())).collect();
            assert_eq!(got, want, "seed {seed} case {case}: {src:?}");
        }
    }
}

#[test]
fn random_continuations() {
    // A string, horizontal blanks and comments, one line end, blanks and comment lines, a string.
    // PostgreSQL joins the parts into one literal, and the lexer gives one token. The second part
    // keeps the mode of the first part.
    for seed in seeds() {
        let mut rng = Rng(seed ^ 0xc0de);
        for case in 0..cases() / 4 {
            let escapes = rng.below(2) == 0;
            let body: &[&str] = if escapes { &["a", " ", "\\\\", "\\'", "''"] } else { &["a", " ", "\\", "''"] };
            let first = text_of(&mut rng, body, 3);
            let second = text_of(&mut rng, body, 3);
            let open = if escapes { *rng.pick(&["E", "e"]) } else { *rng.pick(&["", "B", "N", "x"]) };
            let horiz = text_of(&mut rng, &[" ", "\t", "\u{c}", "-- c"], 3);
            let newline = *rng.pick(&["\n", "\r", "\r\n", "\n\n"]);
            let after = text_of(&mut rng, &[" ", "\t", "\u{b}", "-- c\n", "\n"], 3);
            let src = format!("{open}'{first}'{horiz}{newline}{after}'{second}'");
            let tokens = pg(&src);
            assert_cover(&src, &tokens);
            let kind = if escapes {
                TokenKind::EString
            } else if open.is_empty() {
                TokenKind::String
            } else {
                TokenKind::PrefixedString
            };
            // Two parts of the same kind, and the gap is made of blanks and line comments.
            let parts: Vec<&Token> = tokens.iter().filter(|t| !t.kind.is_trivia()).collect();
            assert_eq!(parts.len(), 2, "seed {seed} case {case}: {src:?}: {tokens:?}");
            assert!(parts.iter().all(|t| t.closed && t.kind == kind), "{src:?}");
            assert_eq!(parts[1].text(&src), format!("'{second}'"), "{src:?}");
            assert!(
                tokens
                    .iter()
                    .all(|t| matches!(t.kind, TokenKind::Blank | TokenKind::LineComment) || parts.contains(&t))
            );
            // With no line end the parts are two tokens, with a blank between them.
            let ws = *rng.pick(&[" ", "\t", "\u{c}", "\u{b}", "  "]);
            if !escapes && !first.contains('\\') && !second.contains('\\') {
                let src = format!("'{first}'{ws}'{second}'");
                assert_eq!(letters(&src), "S_S", "{src:?}");
            }
        }
    }
}

#[test]
fn the_text_of_a_closed_string_decodes_to_the_content() {
    // The decoder here is independent of the lexer: it follows the rules of 7.20 on the content.
    fn decode(token: &str, escapes: bool) -> String {
        let inner = &token[token.find('\'').unwrap() + 1..token.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if escapes && c == '\\' {
                out.push(chars.next().unwrap());
            } else if c == '\'' {
                assert_eq!(chars.next(), Some('\''));
                out.push('\'');
            } else {
                out.push(c);
            }
        }
        out
    }
    for seed in seeds() {
        let mut rng = Rng(seed ^ 0xdec0de);
        for _ in 0..cases() / 4 {
            let content = text_of(&mut rng, CONTENT, 10).replace(['\r', '\n'], " ");
            let std = format!("'{}'", content.replace('\'', "''"));
            let t = pg(&std);
            assert_eq!(t.len(), 1);
            assert_eq!(decode(t[0].text(&std), false), content);
            let es = format!("E'{}'", content.replace('\\', "\\\\").replace('\'', "\\'"));
            let t = pg(&es);
            assert_eq!(t.len(), 1);
            assert_eq!(decode(t[0].text(&es), true), content);
        }
    }
}
