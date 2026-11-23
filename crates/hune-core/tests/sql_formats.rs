// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the variable formats that are valid only inside a string literal (7.9, 7.11, D7).
//!
//! The injection cases: a hostile value goes through each format inside a string, and the string
//! literal stays one token that holds the whole text. Each format fails closed in code, in a quoted
//! name and in a heredoc, for each class and each selection of a variable.

use hune_core::sql::formats::{Subject, render};
use hune_core::sql::placeholders::LITERAL_ONLY_FORMATS;
use hune_core::sql::{
    ExpandError, ExpandRequest, Format, RequestValues, SqlInvalidKind, TextValueType, Token, TokenKind, VariableDef,
    check_time_range, compute_interval, expand, lex_all, resolve_values,
};

const FROM: i64 = 1_794_211_200_000;
const TO: i64 = 1_794_232_800_000;

fn run(defs: &[VariableDef], values: &RequestValues, sql: &str) -> Result<String, ExpandError> {
    let resolved = resolve_values(defs, values).expect("values");
    let range = check_time_range(FROM, TO).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    expand(&ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 262_144 })
}

/// The variables of the tests. The name `v` is the name that the formats `distributed` and `queryparam` print.
struct Shape {
    label: &'static str,
    def: VariableDef,
    all: bool,
    values: Vec<String>,
}

fn shapes(hostile: &[String]) -> Vec<Shape> {
    let one = hostile.first().cloned().unwrap_or_default();
    let opts = ["a'b", "c\\d", "e f"];
    let s = |label, def, all, values: &[String]| Shape { label, def, all, values: values.to_vec() };
    vec![
        s("text", VariableDef::text("v", TextValueType::Text), false, std::slice::from_ref(&one)),
        s("query single", VariableDef::query("v", false, false, None), false, std::slice::from_ref(&one)),
        s("query multi", VariableDef::query("v", true, false, None), false, hostile),
        s("query empty list", VariableDef::query("v", true, false, None), false, &[]),
        s("query empty selection", VariableDef::query("v", false, false, None), false, &[]),
        s("query all", VariableDef::query("v", true, true, None), true, hostile),
        s("query all with all_value", VariableDef::query("v", true, true, Some("x'y")), true, &[]),
        s("custom single", VariableDef::custom("v", &opts, false, false, None), false, &["a'b".to_owned()]),
        s(
            "custom multi",
            VariableDef::custom("v", &opts, true, false, None),
            false,
            &["a'b".to_owned(), "e f".to_owned()],
        ),
        // The engine reads the options of a custom variable for "All", and it ignores the values of the request.
        s("custom all", VariableDef::custom("v", &opts, true, true, None), true, &opts.map(str::to_owned)),
        s("custom all with all_value", VariableDef::custom("v", &opts, false, true, Some("x'y")), true, &[]),
    ]
}

fn request(sh: &Shape) -> RequestValues {
    let mut r = RequestValues::new();
    let refs: Vec<&str> = sh.values.iter().map(String::as_str).collect();
    r.insert("v", sh.all, &refs);
    r
}

// ---- the seeded generator of hostile values ----

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
}

const PIECES: &[&str] = &[
    "'",
    "\"",
    "`",
    "\\",
    "\\'",
    "''",
    "$",
    "${",
    "${v}",
    "$v",
    "$__from",
    "$$",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ",",
    "|",
    "&",
    "<",
    ">",
    "%",
    "%27",
    "&#39;",
    "&quot;",
    "--",
    "/*",
    "*/",
    "#",
    "; DROP TABLE t; --",
    "' OR 1=1 --",
    "\\\\",
    "\u{2028}",
    "\u{a0}",
    "\u{feff}",
    "\u{e9}",
    "\u{1f600}",
    " ",
    "a",
    "b",
    "1",
    ".",
    "-",
    "e",
    "0x",
    "NaN",
    "Infinity",
    ":",
    "=",
    "+",
    "*",
    "?",
    "^",
    "~",
    "!",
    "/",
    "var-v=",
];

fn hostile(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(12) {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

/// Decodes the content of a string token: `\\` is `\`, `\'` is `'`.
fn decode(content: &str) -> String {
    let mut out = String::new();
    let mut chars = content.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next().expect("a backslash starts an escape pair"));
        } else {
            assert_ne!(c, '\'', "an unescaped quote mark inside the content: {content:?}");
            out.push(c);
        }
    }
    out
}

fn non_blank(sql: &str) -> Vec<Token> {
    lex_all(sql).into_iter().filter(|t| t.kind != TokenKind::Blank).collect()
}

/// The text that the format gives for a shape, before the string escape.
fn formatted(f: Format, sh: &Shape) -> String {
    let one = sh.values.first().map_or("", String::as_str);
    let subject = if !sh.def.may_be_list() && !sh.all {
        Subject::One(one)
    } else {
        Subject::List { values: &sh.values, all: sh.all }
    };
    render(f, "v", subject).expect("a literal-only format")
}

#[test]
fn a_hostile_value_stays_inside_the_string_literal_for_each_format() {
    let mut rng = Rng(0x5eed);
    for round in 0..300 {
        let list: Vec<String> = (0..1 + rng.below(4)).map(|_| hostile(&mut rng)).collect();
        for sh in shapes(&list) {
            for f in LITERAL_ONLY_FORMATS {
                let sql = format!("SELECT 1 WHERE s = 'a${{v:{}}}b' AND t = 2", f.name());
                let out = run(std::slice::from_ref(&sh.def), &request(&sh), &sql)
                    .unwrap_or_else(|e| panic!("round {round}, {} {f:?}: {e:?} for {list:?}", sh.label));
                // The tokens are the tokens of the template, and the string holds the whole text.
                let tokens = non_blank(&out);
                let strings: Vec<&Token> = tokens.iter().filter(|t| t.kind == TokenKind::String).collect();
                assert_eq!(strings.len(), 1, "{} {f:?}: {out:?}", sh.label);
                let t = strings[0];
                assert!(t.closed, "{} {f:?}: {out:?}", sh.label);
                let words: Vec<&str> =
                    tokens.iter().filter(|t| t.kind != TokenKind::String).map(|t| &out[t.start..t.end]).collect();
                assert_eq!(
                    words,
                    ["SELECT", "1", "WHERE", "s", "=", "AND", "t", "=", "2"],
                    "{} {f:?}: {out:?}",
                    sh.label
                );
                let content = &out[t.start + 1..t.end - 1];
                let want = match (&sh.def.list_options(), sh.all) {
                    (Some(l), true) if l.all_value.is_some() => {
                        let text = l.all_value.clone().unwrap();
                        render(f, "v", Subject::AllValue(&text)).unwrap()
                    }
                    _ => formatted(f, &sh),
                };
                assert_eq!(decode(content), format!("a{want}b"), "{} {f:?}: {out:?}", sh.label);
            }
        }
    }
}

#[test]
fn each_format_fails_closed_in_code_in_a_quoted_name_and_in_a_heredoc() {
    let mut rng = Rng(0xc0de);
    let list: Vec<String> = (0..3).map(|_| hostile(&mut rng)).collect();
    for sh in shapes(&list) {
        for f in LITERAL_ONLY_FORMATS {
            let name = f.name();
            for sql in [
                format!("SELECT ${{v:{name}}}"),
                format!("SELECT x IN (${{v:{name}}})"),
                format!("SELECT x = ${{v:{name}}} AND y = 1"),
                format!("SELECT 1 WHERE $__timeFilter(${{v:{name}}})"),
                format!("SELECT $__conditionalAll(a = ${{v:{name}}}, $v)"),
                format!("SELECT `${{v:{name}}}`"),
                format!("SELECT \"${{v:{name}}}\""),
                format!("SELECT $$ ${{v:{name}}} $$"),
                format!("SELECT $t$ ${{v:{name}}} $t$"),
            ] {
                // `$__conditionalAll` reads the variable first. A selection of All gives 1=1 and drops the text.
                if sql.contains("conditionalAll") && sh.all {
                    continue;
                }
                match run(std::slice::from_ref(&sh.def), &request(&sh), &sql) {
                    Err(ExpandError::Invalid(i)) => {
                        let kind_ok = i.kind == SqlInvalidKind::Context
                            || (sql.contains("conditionalAll") && i.kind == SqlInvalidKind::Macro);
                        assert!(kind_ok, "{} {f:?} {sql:?}: {i:?}", sh.label);
                    }
                    other => panic!("{} {f:?} {sql:?}: {other:?}", sh.label),
                }
            }
        }
    }
}

#[test]
fn the_formats_that_stay_in_code_still_work_in_code() {
    let def = VariableDef::query("v", true, false, None);
    let mut r = RequestValues::new();
    r.insert("v", false, &["a'b", "c"]);
    let defs = [def];
    assert_eq!(run(&defs, &r, "SELECT x IN (${v:singlequote})").unwrap(), "SELECT x IN ('a\\'b','c')");
    assert_eq!(run(&defs, &r, "SELECT x IN (${v:sqlstring})").unwrap(), "SELECT x IN ('a''b','c')");
    assert_eq!(run(&defs, &r, "SELECT '${v:csv}'").unwrap(), "SELECT 'a\\'b,c'");
}

#[test]
fn a_format_in_a_string_in_a_comment_or_in_a_heredoc_is_not_read() {
    let def = VariableDef::query("v", true, false, None);
    let mut r = RequestValues::new();
    r.insert("v", false, &["a"]);
    let defs = [def];
    let sql = "SELECT 1 -- ${v:json}\n/* ${v:regex} */";
    assert_eq!(run(&defs, &r, sql).unwrap(), sql);
}

#[test]
fn the_vectors_of_7_11_for_the_new_formats() {
    // A value with a quote mark cannot end the string, whatever the format makes of it.
    let def = VariableDef::query("v", false, false, None);
    let mut r = RequestValues::new();
    r.insert("v", false, &["x' OR 1=1 --"]);
    let defs = [def];
    for f in LITERAL_ONLY_FORMATS {
        let out = run(&defs, &r, &format!("SELECT 1 WHERE s = '${{v:{}}}'", f.name())).unwrap();
        assert!(out.ends_with("'"), "{f:?}: {out}");
        let t = non_blank(&out);
        assert_eq!(t.iter().filter(|t| t.kind == TokenKind::String).count(), 1, "{f:?}: {out}");
        assert!(
            t.iter().all(|t| t.kind != TokenKind::LineComment && t.kind != TokenKind::BlockComment),
            "{f:?}: {out}"
        );
    }
    // A value of a format that ends with a backslash cannot join the closing quote mark.
    let mut r = RequestValues::new();
    r.insert("v", false, &["a\\"]);
    for f in LITERAL_ONLY_FORMATS {
        let out = run(&defs, &r, &format!("SELECT '${{v:{}}}', 1", f.name())).unwrap();
        let t = non_blank(&out);
        assert_eq!(t.iter().filter(|t| t.kind == TokenKind::String).count(), 1, "{f:?}: {out}");
        assert!(out.ends_with(", 1"), "{f:?}: {out}");
    }
}

#[test]
fn a_hostile_value_stays_inside_its_literal_in_a_conditional_all() {
    let mut rng = Rng(0xa11);
    let def = VariableDef::query("h", true, true, None);
    let defs = [def];
    for round in 0..300 {
        let list: Vec<String> = (0..1 + rng.below(4)).map(|_| hostile(&mut rng)).collect();
        let mut r = RequestValues::new();
        let refs: Vec<&str> = list.iter().map(String::as_str).collect();
        r.insert("h", false, &refs);
        let sql = "SELECT 1 WHERE $__conditionalAll(h IN ($h) AND s = '${h:regex}', $h) AND t = 2";
        let out = run(&defs, &r, sql).unwrap_or_else(|e| panic!("round {round}: {e:?} for {list:?}"));
        let tokens = non_blank(&out);
        let strings: Vec<String> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::String)
            .map(|t| {
                assert!(t.closed, "{out:?}");
                decode(&out[t.start + 1..t.end - 1])
            })
            .collect();
        let regex = render(Format::Regex, "h", Subject::List { values: &list, all: false }).unwrap();
        let mut want = list.clone();
        want.push(regex);
        assert_eq!(strings, want, "round {round}: {out:?}");
        let words: Vec<&str> =
            tokens.iter().filter(|t| t.kind != TokenKind::String).map(|t| &out[t.start..t.end]).collect();
        let commas = vec![","; list.len() - 1];
        let mut expect_words = vec!["SELECT", "1", "WHERE", "h", "IN", "("];
        expect_words.extend(commas);
        expect_words.extend([")", "AND", "s", "=", "AND", "t", "=", "2"]);
        assert_eq!(words, expect_words, "round {round}: {out:?}");
        // All selected: the condition is 1=1 and the values do not appear.
        let mut all = RequestValues::new();
        all.insert("h", true, &refs);
        let out = run(&defs, &all, sql).unwrap();
        assert_eq!(out, "SELECT 1 WHERE 1=1 AND t = 2");
    }
}
