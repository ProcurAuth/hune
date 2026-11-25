// SPDX-License-Identifier: MIT OR Apache-2.0
//! Seeded random tests of the lexer and the substitution (16.4).
//!
//! The generator makes SQL templates from keywords, identifiers, quoted names, numbers, strings,
//! comments, heredocs and macros, with variable references in code, strings, quoted names,
//! comments and heredocs. It makes hostile values. A seeded generator (splitmix64) gives the same
//! cases on each platform.
//!
//! Run one seed again: `HUNE_TEST_SEED=4242 cargo test -p hune-core --locked random_`.
//! The environment variables `HUNE_TEST_SEEDS` (the number of seeds, from 1) and `HUNE_TEST_CASES`
//! (the cases of each seed) set a longer run for a nightly job. The default is the seeds 1 to 64.
//!
//! Properties (16.4): R1 a value never leaves its literal and decodes to itself. R2 the tokens outside
//! the places of references are the tokens of the template. R3 a refused value gives an error and
//! no SQL. R4 a reference in a comment stays text. R5 the same inputs give the same bytes. R6 the
//! statement check of the output agrees with the template. R7 the lexer never panics and the token
//! texts joined give the input. Also: a document fault of the expansion is a fault of the checks at save.

mod seed_threads;

use hune_core::sql::{
    ExpandError, ExpandRequest, RequestValues, SaveCheckResult, SqlInvalidKind, SqlUse, TextValueType, Token,
    TokenKind, ValuesError, VariableDef, check_time_range, compute_interval, expand, lex, lex_all, resolve_values,
    save_checks,
};

// ---- the seeded generator ----

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

    fn percent(&mut self, p: usize) -> bool {
        self.below(100) < p
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const SAFE_ALPHABET: &[&str] = &[
    "'",
    "\"",
    "`",
    "\\",
    "$",
    "{",
    "}",
    "-",
    "/",
    "*",
    "#",
    ";",
    "\t",
    "\u{2028}",
    "\u{1f600}",
    "\u{e9}",
    "a",
    "b",
    "1",
    "2",
    ".",
    " ",
    "(",
    ")",
    ",",
    "_",
    "e",
    "x",
    "%",
    "\u{a0}",
    "\u{feff}",
    "[",
    "]",
    ":",
    "=",
];
const PIECES: &[&str] = &[
    "--",
    "/*",
    "*/",
    "$$",
    "$a$",
    "${x}",
    "$__fromTime",
    "# ",
    "#!",
    "''",
    "\\'",
    "\\\\",
    "' OR 1=1 --",
    "${t}",
    "$t",
    "$$ $$",
];

fn hostile_value(rng: &mut Rng) -> String {
    let mut s = String::new();
    let target = if rng.percent(2) { 600 + rng.below(420) } else { rng.below(40) };
    while s.len() < target {
        if rng.percent(25) {
            s.push_str(rng.pick(PIECES));
        } else {
            s.push_str(rng.pick(SAFE_ALPHABET));
        }
    }
    // Keep the value within 1,024 bytes without cutting a character.
    while s.len() > 1024 {
        s.pop();
    }
    s
}

fn number_value(rng: &mut Rng) -> String {
    let mut s = String::new();
    if rng.percent(30) {
        s.push('-');
    }
    for _ in 0..1 + rng.below(6) {
        s.push(char::from(b'0' + rng.below(10) as u8));
    }
    if rng.percent(30) {
        s.push('.');
        for _ in 0..1 + rng.below(3) {
            s.push(char::from(b'0' + rng.below(10) as u8));
        }
    }
    s
}

fn free_value(rng: &mut Rng) -> String {
    if rng.percent(45) { number_value(rng) } else { hostile_value(rng) }
}

fn has_control(s: &str) -> bool {
    s.chars().any(|c| c < ' ' || c == '\u{7f}')
}

fn with_control(rng: &mut Rng) -> String {
    let mut s = free_value(rng);
    let c = *rng.pick(&['\0', '\n', '\r', '\u{1}', '\u{7f}', '\u{1f}']);
    let at = rng.below(s.len() + 1);
    let at = (0..=at).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
    s.insert(at, c);
    s
}

// ---- variables ----

const OPTIONS: [&str; 3] = ["a1", "b2", "c d"];

/// The variables of the test. `open_query` and `open_custom` set the member `allow_custom_value` (4.7).
fn defs_with(open_query: bool, open_custom: bool) -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::text("n", TextValueType::Number),
        VariableDef::query("q", false, true, None).with_open(open_query),
        VariableDef::query("qm", true, true, None).with_open(open_query),
        VariableDef::query("qa", true, true, Some("7")).with_open(open_query),
        VariableDef::custom("c", &OPTIONS, false, false, None).with_open(open_custom),
        VariableDef::custom("cm", &OPTIONS, true, true, None).with_open(open_custom),
        VariableDef::custom("ca", &OPTIONS, true, true, Some("x1")).with_open(open_custom),
    ]
}

const VARS: [&str; 8] = ["t", "n", "q", "qm", "qa", "c", "cm", "ca"];

#[derive(Clone, Debug)]
enum Sel {
    Single(String),
    List(Vec<String>),
    AllText(String),
    Empty,
}

#[derive(Clone, Debug)]
struct VarCase {
    free: bool,
    custom: bool,
    sel: Sel,
    /// The selection is "All" of a `custom` variable: the list that the server builds from the options.
    all: bool,
}

impl VarCase {
    /// The case of the same values when the `custom` variable is open: each value of the
    /// request is free text and the variable stands in no quoted name. The list "All" and `all_value` stay author text.
    fn opened(&self) -> VarCase {
        if !self.custom {
            return self.clone();
        }
        let author = self.all || matches!(self.sel, Sel::AllText(_));
        VarCase { free: !author, custom: false, sel: self.sel.clone(), all: self.all }
    }
}

struct Request {
    values: RequestValues,
    vars: Vec<(&'static str, VarCase)>,
    has_control: bool,
}

fn make_request(rng: &mut Rng) -> Request {
    let mut values = RequestValues::new();
    let mut vars = vec![];
    let mut control = false;
    let maybe_bad = |rng: &mut Rng, control: &mut bool| {
        if rng.percent(2) {
            *control = true;
            with_control(rng)
        } else {
            free_value(rng)
        }
    };
    for name in VARS {
        let (all, strs, case): (bool, Vec<String>, VarCase) = match name {
            "t" => {
                let v = maybe_bad(rng, &mut control);
                (false, vec![v.clone()], VarCase { free: true, custom: false, all: false, sel: Sel::Single(v) })
            }
            "n" => {
                let v = number_value(rng);
                (false, vec![v.clone()], VarCase { free: true, custom: false, all: false, sel: Sel::Single(v) })
            }
            "q" => {
                if rng.percent(25) {
                    (false, vec![], VarCase { free: true, custom: false, all: false, sel: Sel::Empty })
                } else if rng.percent(10) {
                    // "All" of a non-multi query variable without all_value: the list of the page.
                    let l: Vec<String> = (0..rng.below(4)).map(|_| free_value(rng)).collect();
                    (true, l.clone(), VarCase { free: true, custom: false, all: false, sel: Sel::List(l) })
                } else {
                    let v = maybe_bad(rng, &mut control);
                    (false, vec![v.clone()], VarCase { free: true, custom: false, all: false, sel: Sel::Single(v) })
                }
            }
            "qm" => {
                let all = rng.percent(30);
                let long = rng.percent(5);
                let n = rng.below(if long { 51 } else { 6 });
                let mut l: Vec<String> = vec![];
                for _ in 0..n {
                    l.push(maybe_bad(rng, &mut control));
                }
                (all, l.clone(), VarCase { free: true, custom: false, all: false, sel: Sel::List(l) })
            }
            "qa" => {
                if rng.percent(40) {
                    (true, vec![], VarCase { free: false, custom: false, all: false, sel: Sel::AllText("7".into()) })
                } else {
                    let l: Vec<String> = (0..rng.below(4)).map(|_| free_value(rng)).collect();
                    (false, l.clone(), VarCase { free: true, custom: false, all: false, sel: Sel::List(l) })
                }
            }
            "c" => {
                let v = rng.pick(&OPTIONS).to_string();
                (false, vec![v.clone()], VarCase { free: false, custom: true, all: false, sel: Sel::Single(v) })
            }
            "cm" => {
                if rng.percent(30) {
                    let all: Vec<String> = OPTIONS.iter().map(|s| s.to_string()).collect();
                    (true, vec![], VarCase { free: false, custom: true, all: true, sel: Sel::List(all) })
                } else {
                    let l: Vec<String> = OPTIONS.iter().filter(|_| rng.percent(50)).map(|s| s.to_string()).collect();
                    (false, l.clone(), VarCase { free: false, custom: true, all: false, sel: Sel::List(l) })
                }
            }
            _ => {
                if rng.percent(40) {
                    (true, vec![], VarCase { free: false, custom: true, all: false, sel: Sel::AllText("x1".into()) })
                } else {
                    let l: Vec<String> = OPTIONS.iter().filter(|_| rng.percent(50)).map(|s| s.to_string()).collect();
                    (false, l.clone(), VarCase { free: false, custom: true, all: false, sel: Sel::List(l) })
                }
            }
        };
        control |= strs.iter().any(|v| has_control(v));
        let refs: Vec<&str> = strs.iter().map(String::as_str).collect();
        values.insert(name, all, &refs);
        vars.push((name, case));
    }
    Request { values, vars, has_control: control }
}

// ---- templates ----

#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    Bare,
    Brace,
    Fmt(&'static str),
}

const FORMATS: [&str; 4] = ["raw", "csv", "singlequote", "sqlstring"];

fn make_form(rng: &mut Rng) -> Form {
    match rng.below(7) {
        0 | 1 => Form::Bare,
        2 | 3 => Form::Brace,
        _ => Form::Fmt(rng.pick(&FORMATS)),
    }
}

fn reference(var: &str, form: Form) -> String {
    match form {
        Form::Bare => format!("${var}"),
        Form::Brace => format!("${{{var}}}"),
        Form::Fmt(f) => format!("${{{var}:{f}}}"),
    }
}

fn format_of(form: Form) -> Option<&'static str> {
    match form {
        Form::Fmt(f) => Some(f),
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum Piece {
    /// Text with no reference. The output segment MUST be equal to the text.
    Plain(String),
    /// A macro or a comment: the expected output text is not the template text.
    Expected(String),
    StrRef {
        pre: String,
        var: &'static str,
        form: Form,
        post: String,
    },
    NameRef {
        pre: String,
        var: &'static str,
        form: Form,
        post: String,
        quote: char,
    },
    CodeRef {
        var: &'static str,
        form: Form,
    },
    /// A reference in code with template text before and after it, with no blank.
    AdjCode {
        pre: &'static str,
        var: &'static str,
        form: Form,
        post: &'static str,
    },
    /// A reference in a string with template text that can hold a backslash or a doubled quote mark.
    AdjStr {
        pre: &'static str,
        var: &'static str,
        form: Form,
        post: &'static str,
    },
    HeredocRef {
        var: &'static str,
        form: Form,
    },
}

const F: i64 = 1_794_211_200;
const T: i64 = 1_794_232_800;

fn tf(e: &str) -> String {
    format!("{e} >= toDateTime({F}) AND {e} <= toDateTime({T})")
}

fn plain_piece(rng: &mut Rng) -> Piece {
    const KEYWORDS: &[&str] =
        &["FROM", "WHERE", "AND", "OR", "AS", "ON", "GROUP", "BY", "ORDER", "LIMIT", "JOIN", "x", "t1", "_a"];
    const NUMBERS: &[&str] = &["1", "0x1F", "1.5e-3", ".5", "1_000", "42"];
    const STRINGS: &[&str] = &["'a'", "'it''s'", "'a\\'b'", "'\\\\'", "''", "'a b ; c'", "'-- x'", "'/* y'", "'$$'"];
    const NAMES: &[&str] = &["`a\\`b`", "`x``y`", "\"a\\\"b\"", "\"a\"\"b\"", "`plain`", "`a--b`", "\"a/*b\""];
    const COMMENTS: &[&str] = &[
        "-- plain\n",
        "# plain\n",
        "#! plain\n",
        "/* a /* b */ c */",
        "/**/",
        "-- $t ${t:csv} $qm ${c}\n",
        "/* $t /* ${n} */ $c */",
        "# $qm ${ca:raw}\n",
    ];
    const HEREDOCS: &[&str] = &["$$a'b$$", "$tag$ x $$ y $tag$", "$$ -- /* $$", "$_1$ $zz $_1$"];
    match rng.below(10) {
        0 | 1 => Piece::Plain(rng.pick(KEYWORDS).to_string()),
        2 => Piece::Plain(rng.pick(NUMBERS).to_string()),
        3 => Piece::Plain(rng.pick(STRINGS).to_string()),
        4 => Piece::Plain(rng.pick(NAMES).to_string()),
        5 => Piece::Plain(rng.pick(COMMENTS).to_string()),
        6 => Piece::Plain(rng.pick(HEREDOCS).to_string()),
        7 => Piece::Plain(
            rng.pick(&["+", "-", "*", "/", "=", "<", ">", ",", "f(a, b)", "(x)", "[1, 2]", "$zz", "${zz}", "$1"])
                .to_string(),
        ),
        _ => match rng.below(6) {
            0 => Piece::Expected("$__fromTime".into()),
            1 => Piece::Expected("$__toTime".into()),
            2 => Piece::Expected("$__interval_s".into()),
            3 => Piece::Expected("$__timeFilter(col)".into()),
            4 => Piece::Expected("$__timeInterval(col)".into()),
            _ => Piece::Plain("'$__fromTime $__x'".into()),
        },
    }
}

fn expected_macro(template: &str) -> String {
    match template {
        "$__fromTime" => format!("toDateTime({F})"),
        "$__toTime" => format!("toDateTime({T})"),
        "$__interval_s" => "20".into(),
        "$__timeFilter(col)" => tf("col"),
        _ => "toStartOfInterval(toDateTime(col), INTERVAL 20 second)".into(),
    }
}

fn plain_text(rng: &mut Rng) -> String {
    const WORDS: &[&str] = &["a", "b ", "x y", "; ", "-", "\u{e9}", "[", "", "%"];
    let mut s = String::new();
    for _ in 0..rng.below(3) {
        s.push_str(rng.pick(WORDS));
    }
    s
}

/// A bare `$v` takes the longest run of name bytes, so a text that starts with one needs the braces.
fn safe_form(form: Form, post: &str) -> Form {
    let joins = post.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
    if form == Form::Bare && joins { Form::Brace } else { form }
}

fn make_piece(rng: &mut Rng) -> Piece {
    let var = *rng.pick(&VARS);
    let lists = matches!(var, "qm" | "cm" | "ca" | "qa");
    match rng.below(24) {
        0..=3 => {
            let (pre, post) = (plain_text(rng), plain_text(rng));
            let form = if lists && rng.percent(60) { Form::Fmt("csv") } else { make_form(rng) };
            Piece::StrRef { pre, var, form: safe_form(form, &post), post }
        }
        4 => {
            let (pre, post, form) = (plain_text(rng), plain_text(rng), make_form(rng));
            let var = if rng.percent(70) { *rng.pick(&["c", "ca"]) } else { *rng.pick(&["cm", "t", "q"]) };
            Piece::NameRef { pre, var, form: safe_form(form, &post), post, quote: *rng.pick(&['`', '"']) }
        }
        5..=7 => {
            let form = match var {
                "t" | "q" if rng.percent(60) => Form::Fmt(rng.pick(&["singlequote", "sqlstring"])),
                _ => make_form(rng),
            };
            Piece::CodeRef { var, form }
        }
        8 if rng.percent(40) => Piece::HeredocRef { var, form: make_form(rng) },
        9..=11 => {
            let pre = *rng.pick(&["", "0-", "1.", "x", "-", "0x", ".", "1e", "a.", "1 -", "1,", "1+"]);
            let post = *rng.pick(&["", "abc", "e5", ".5", "x", "-", "1", ",", "e+", "_", ")", " AND"]);
            let form = match var {
                "t" | "q" if rng.percent(50) => Form::Fmt(rng.pick(&["singlequote", "sqlstring"])),
                "n" | "t" | "q" => make_form(rng),
                _ => Form::Fmt(rng.pick(&FORMATS)),
            };
            Piece::AdjCode { pre, var, form: safe_form(form, post), post }
        }
        12 | 13 => {
            let pre = *rng.pick(&["", "\\", "a\\", "''", "\\\\", "\\'"]);
            let post = *rng.pick(&["", "b", "\\\\", "''", "\\'"]);
            let form = if lists && rng.percent(60) { Form::Fmt("csv") } else { make_form(rng) };
            Piece::AdjStr { pre, var, form: safe_form(form, post), post }
        }
        _ => plain_piece(rng),
    }
}

struct Template {
    pieces: Vec<Piece>,
    text: String,
    statement_ok: bool,
}

fn make_template(rng: &mut Rng) -> Template {
    let mut pieces = vec![];
    let bad_head = rng.percent(6);
    pieces.push(Piece::Plain(if bad_head { "SHOW".into() } else { "SELECT".into() }));
    for _ in 0..1 + rng.below(7) {
        pieces.push(make_piece(rng));
    }
    let bad_tail = rng.percent(6);
    if bad_tail {
        pieces.push(Piece::Plain(rng.pick(&["; SELECT 2", "FORMAT JSON", "INTO OUTFILE 'x'"]).to_string()));
    }
    let mut text = String::new();
    for (k, p) in pieces.iter().enumerate() {
        let t = match p {
            Piece::Plain(s) | Piece::Expected(s) => s.clone(),
            Piece::StrRef { pre, var, form, post } => format!("'{pre}{}{post}'", reference(var, *form)),
            Piece::NameRef { pre, var, form, post, quote } => {
                format!("{quote}{pre}{}{post}{quote}", reference(var, *form))
            }
            Piece::CodeRef { var, form } => reference(var, *form),
            Piece::AdjCode { pre, var, form, post } => format!("{pre}{}{post}", reference(var, *form)),
            Piece::AdjStr { pre, var, form, post } => format!("'{pre}{}{post}'", reference(var, *form)),
            Piece::HeredocRef { var, form } => format!("$hd$ x {} y $hd$", reference(var, *form)),
        };
        text.push_str(&t);
        text.push_str(&format!(" /*#{k}*/ "));
    }
    Template { pieces, text, statement_ok: !(bad_head || bad_tail) }
}

// ---- the oracle: it reads only the output tokens ----

/// Decodes the content of a string or a name: a backslash gives the next character, a doubled quote
/// mark gives one.
fn decode(content: &str, quote: char) -> String {
    let mut out = String::new();
    let mut it = content.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else if c == quote && it.peek() == Some(&quote) {
            it.next();
            out.push(quote);
        } else {
            out.push(c);
        }
    }
    out
}

fn non_blank<'a>(seg: &'a str, tokens: &'a [Token]) -> Vec<(TokenKind, &'a str)> {
    tokens.iter().filter(|t| t.kind != TokenKind::Blank).map(|t| (t.kind, t.text(seg))).collect()
}

fn single_literal(seg: &str, kind: TokenKind, quote: char) -> Option<String> {
    let tokens = lex_all(seg);
    let nb = non_blank(seg, &tokens);
    match nb.as_slice() {
        [(k, text)] if *k == kind && tokens.iter().any(|t| t.closed) && text.len() >= 2 => {
            Some(decode(&text[1..text.len() - 1], quote))
        }
        _ => None,
    }
}

/// Reads a list of string tokens separated by `,`. It gives the decoded items.
fn string_list(seg: &str) -> Option<Vec<String>> {
    let tokens = lex_all(seg);
    let nb = non_blank(seg, &tokens);
    let mut items = vec![];
    for (i, (k, text)) in nb.iter().enumerate() {
        if i % 2 == 0 {
            if *k != TokenKind::String
                || !tokens.iter().all(|t| t.kind != TokenKind::String || t.closed)
                || text.len() < 2
            {
                return None;
            }
            items.push(decode(&text[1..text.len() - 1], '\''));
        } else if *k != TokenKind::Punct || *text != "," {
            return None;
        }
    }
    if nb.len().is_multiple_of(2) { None } else { Some(items) }
}

/// Reads a list of numbers (with an optional `-`) separated by `,`.
fn number_list(seg: &str) -> Option<Vec<String>> {
    let tokens = lex_all(seg);
    let nb = non_blank(seg, &tokens);
    let mut items = vec![];
    let mut i = 0;
    while i < nb.len() {
        let mut item = String::new();
        if nb[i] == (TokenKind::Punct, "-") {
            item.push('-');
            i += 1;
        }
        match nb.get(i) {
            Some((TokenKind::Number, t)) => item.push_str(t),
            _ => return None,
        }
        items.push(item);
        i += 1;
        match nb.get(i) {
            None => break,
            Some((TokenKind::Punct, ",")) => i += 1,
            _ => return None,
        }
    }
    Some(items)
}

fn joined(sel: &Sel) -> String {
    match sel {
        Sel::Single(v) | Sel::AllText(v) => v.clone(),
        Sel::List(l) => l.join(","),
        Sel::Empty => String::new(),
    }
}

fn items(sel: &Sel) -> Vec<String> {
    match sel {
        Sel::Single(v) => vec![v.clone()],
        Sel::List(l) => l.clone(),
        _ => vec![],
    }
}

fn quote_with(v: &str, sql_string: bool) -> String {
    let mut out = String::from("'");
    for c in v.chars() {
        match (c, sql_string) {
            ('\\', _) => out.push_str("\\\\"),
            ('\'', true) => out.push_str("''"),
            ('\'', false) => out.push_str("\\'"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn is_number(v: &str) -> bool {
    let v = v.strip_prefix('-').unwrap_or(v);
    let (a, b) = v.split_once('.').map_or((v, None), |(a, b)| (a, Some(b)));
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
    digits(a) && b.is_none_or(digits)
}

/// The text that a reference gives in code, by the table of 7.8, and the shape of a free value
/// (`Some(true)` numbers, `Some(false)` string literals). `None` for the text means a refusal.
fn render_expected(c: &VarCase, fmt: Option<&str>) -> Option<(String, Option<bool>)> {
    let quoted = matches!(fmt, Some("singlequote" | "sqlstring"));
    let sqlstring = fmt == Some("sqlstring");
    let lit = |l: &[String]| -> String {
        if l.is_empty() {
            "''".to_owned()
        } else {
            l.iter().map(|v| quote_with(v, sqlstring)).collect::<Vec<_>>().join(",")
        }
    };
    let free = |shape: bool| c.free.then_some(shape);
    let empty_list = |l: &[String]| l.is_empty().then_some(true);
    match &c.sel {
        Sel::AllText(t) => Some((t.clone(), None)),
        Sel::Empty if matches!(fmt, Some("raw" | "csv")) => Some((String::new(), free(true))),
        Sel::Empty => Some((lit(&[]), free(false))),
        Sel::Single(v) if quoted => Some((lit(std::slice::from_ref(v)), free(false))),
        Sel::Single(v) if !c.free => Some((v.clone(), None)),
        Sel::Single(v) => is_number(v).then(|| (v.clone(), Some(true))),
        Sel::List(l) if matches!(fmt, Some("raw" | "csv")) => {
            if c.free && !l.iter().all(|v| is_number(v)) {
                None
            } else {
                Some((l.join(","), free(true).or(empty_list(l))))
            }
        }
        Sel::List(l) => Some((lit(l), free(false))),
    }
}

/// Tells if the string forms of 7.8 permit the reference in a string.
fn string_form_allowed(c: &VarCase, fmt: Option<&str>) -> bool {
    let quoted = matches!(fmt, Some("singlequote" | "sqlstring"));
    let list = matches!(c.sel, Sel::List(_));
    !quoted && !(list && matches!(fmt, None | Some("raw")))
}

/// Independent check of the property P for the range `[start, end)` of `seg`: a token starts at
/// `start` and a token ends at `end`, and the tokens inside are a list of numbers or of strings.
fn region_ok(seg: &str, start: usize, end: usize, numbers: bool) -> bool {
    let tokens = lex_all(seg);
    if start == end {
        return start == seg.len()
            || tokens.iter().any(|t| t.start == start)
            || tokens.iter().any(|t| t.kind == TokenKind::Blank && t.start < start && start < t.end);
    }
    if !tokens.iter().any(|t| t.start == start) || !tokens.iter().any(|t| t.end == end) {
        return false;
    }
    let inside: Vec<&Token> = tokens.iter().filter(|t| t.start >= start && t.end <= end).collect();
    let mut expect_item = true;
    let mut i = 0;
    while i < inside.len() {
        let t = inside[i];
        if expect_item {
            if numbers {
                if t.kind == TokenKind::Punct && t.text(seg) == "-" {
                    i += 1;
                }
                if !inside.get(i).is_some_and(|t| t.kind == TokenKind::Number) {
                    return false;
                }
            } else if t.kind != TokenKind::String || !t.closed {
                return false;
            }
        } else if t.kind != TokenKind::Punct || t.text(seg) != "," {
            return false;
        }
        expect_item = !expect_item;
        i += 1;
    }
    !expect_item
}

/// Checks one output segment against its piece. It gives a description of the failure.
fn check_segment(piece: &Piece, seg: &str, req: &Request) -> Result<(), String> {
    let var_case = |v: &str| req.vars.iter().find(|(n, _)| *n == v).map(|(_, c)| c.clone()).unwrap();
    match piece {
        Piece::Plain(t) => {
            if seg == t {
                Ok(())
            } else {
                Err(format!("plain piece changed: {t:?} became {seg:?}"))
            }
        }
        Piece::Expected(t) => {
            let want = expected_macro(t);
            if seg == want { Ok(()) } else { Err(format!("macro {t:?} gave {seg:?}")) }
        }
        Piece::HeredocRef { .. } => Err("a placeholder in a heredoc was expanded".into()),
        Piece::AdjCode { pre, var, form, post } => {
            let c = var_case(var);
            let (text, shape) =
                render_expected(&c, format_of(*form)).ok_or("a refused value was expanded".to_owned())?;
            let want = format!("{pre}{text}{post}");
            if seg != want {
                return Err(format!("code text {seg:?}, want {want:?}"));
            }
            match shape {
                Some(numbers) if !region_ok(seg, pre.len(), pre.len() + text.len(), numbers) => {
                    Err(format!("a free value changed the token structure: {seg:?}"))
                }
                _ => Ok(()),
            }
        }
        Piece::AdjStr { pre, var, form, post } => {
            let c = var_case(var);
            if !string_form_allowed(&c, format_of(*form)) {
                return Err("a string took a form that 7.8 refuses".into());
            }
            let mut esc = String::new();
            for ch in joined(&c.sel).chars() {
                if ch == '\\' || ch == '\'' {
                    esc.push('\\');
                }
                esc.push(ch);
            }
            let want = format!("'{pre}{esc}{post}'");
            let tokens = lex_all(seg);
            let one = tokens.len() == 1 && tokens[0].kind == TokenKind::String && tokens[0].closed;
            if seg == want && one { Ok(()) } else { Err(format!("string {seg:?}, want one token {want:?}")) }
        }
        Piece::StrRef { pre, var, post, .. } => {
            let c = var_case(var);
            let want = format!("{}{}{}", decode(pre, '\''), joined(&c.sel), decode(post, '\''));
            let got = single_literal(seg, TokenKind::String, '\'').ok_or(format!("not one string literal: {seg:?}"))?;
            let tokens = lex_all(seg);
            if tokens.iter().filter(|t| t.kind == TokenKind::String).count() != 1 {
                return Err(format!("{seg:?} is not one string"));
            }
            if got == want { Ok(()) } else { Err(format!("string content {got:?}, want {want:?}")) }
        }
        Piece::NameRef { pre, var, post, quote, .. } => {
            let c = var_case(var);
            if !c.custom || matches!(c.sel, Sel::List(_)) {
                return Err("a name took a free value or a list".into());
            }
            let want = format!("{pre}{}{post}", joined(&c.sel));
            let got =
                single_literal(seg, TokenKind::QuotedName, *quote).ok_or(format!("not one quoted name: {seg:?}"))?;
            if got == want { Ok(()) } else { Err(format!("name content {got:?}, want {want:?}")) }
        }
        Piece::CodeRef { var, form } => {
            let c = var_case(var);
            let fmt = format_of(*form);
            let quoted = matches!(fmt, Some("singlequote" | "sqlstring"));
            let want_items = items(&c.sel);
            match (&c.sel, c.free) {
                (Sel::AllText(t), _) => {
                    if seg == t {
                        Ok(())
                    } else {
                        Err(format!("all_value {t:?} became {seg:?}"))
                    }
                }
                (Sel::Single(v), true) if !quoted => {
                    let nl = number_list(seg).ok_or(format!("a free value is not a number: {seg:?}"))?;
                    if nl == [v.clone()] && seg == v { Ok(()) } else { Err(format!("number {v:?} became {seg:?}")) }
                }
                (Sel::Single(v), false) if !quoted => {
                    if seg == v {
                        Ok(())
                    } else {
                        Err(format!("author value {v:?} became {seg:?}"))
                    }
                }
                (Sel::Single(_) | Sel::List(_) | Sel::Empty, _)
                    if quoted || matches!(c.sel, Sel::Empty) && fmt.is_none() =>
                {
                    let got = string_list(seg).ok_or(format!("not a list of strings: {seg:?}"))?;
                    let want = if want_items.is_empty() { vec![String::new()] } else { want_items };
                    if got == want { Ok(()) } else { Err(format!("literals {got:?}, want {want:?}")) }
                }
                (Sel::Empty, _) => {
                    if matches!(fmt, Some("raw" | "csv")) {
                        if seg.is_empty() { Ok(()) } else { Err(format!("empty selection gave {seg:?}")) }
                    } else {
                        Err("unreachable".into())
                    }
                }
                (Sel::List(l), free) => match fmt {
                    None | Some("singlequote" | "sqlstring") => {
                        let got = string_list(seg).ok_or(format!("not a list of strings: {seg:?}"))?;
                        let want = if l.is_empty() { vec![String::new()] } else { l.clone() };
                        if got == want { Ok(()) } else { Err(format!("literals {got:?}, want {want:?}")) }
                    }
                    _ => {
                        if l.is_empty() {
                            return if seg.is_empty() { Ok(()) } else { Err(format!("empty list gave {seg:?}")) };
                        }
                        if free {
                            let got = number_list(seg).ok_or(format!("free values are not numbers: {seg:?}"))?;
                            if got == *l && seg == l.join(",") { Ok(()) } else { Err(format!("numbers {got:?}")) }
                        } else if seg == l.join(",") {
                            Ok(())
                        } else {
                            Err(format!("author list became {seg:?}"))
                        }
                    }
                },
                (Sel::Single(_), _) => Err("unreachable".into()),
            }
        }
    }
}

/// Finds the sentinel tokens `/*#k*/` in order and gives the text between them.
fn segments(out: &str, tokens: &[Token], count: usize) -> Option<Vec<String>> {
    let mut segs = vec![];
    let mut from = 0;
    let mut k = 0;
    for t in tokens {
        if t.kind == TokenKind::BlockComment && t.text(out) == format!("/*#{k}*/") {
            let raw = &out[from..t.start];
            let raw = if k == 0 { raw } else { raw.strip_prefix(' ')? };
            segs.push(raw.strip_suffix(' ')?.to_owned());
            from = t.end;
            k += 1;
            if k == count {
                break;
            }
        }
    }
    (k == count && out[from..] == *" ").then_some(segs)
}

// ---- one case ----

#[derive(Default)]
struct Stats {
    cases: usize,
    ok: usize,
    refused: usize,
    invalid: usize,
    structure: usize,
    open_ok: usize,
    open_runs: usize,
}

impl seed_threads::Merge for Stats {
    fn merge(&mut self, other: Self) {
        self.cases += other.cases;
        self.ok += other.ok;
        self.refused += other.refused;
        self.invalid += other.invalid;
        self.structure += other.structure;
        self.open_ok += other.open_ok;
        self.open_runs += other.open_runs;
    }
}

/// Runs one request on one template with the given variables and checks the properties R1 to R6. It gives the
/// outcome of the expansion, or `None` when the check of the values refuses the request.
fn check_run(
    defs: &[VariableDef],
    req: &Request,
    tpl: &Template,
    ctx: &dyn Fn() -> String,
    stats: &mut Stats,
) -> Option<Result<String, ExpandError>> {
    // R3: a control character is refused at the check of the values.
    let resolved = match resolve_values(defs, &req.values) {
        Ok(r) => {
            assert!(!req.has_control, "a control character was accepted\n{}", ctx());
            r
        }
        Err(ValuesError::Refused(_)) => {
            assert!(req.has_control, "values refused with no control character\n{}", ctx());
            stats.refused += 1;
            return None;
        }
        Err(e) => panic!("{e:?}\n{}", ctx()),
    };
    let range = check_time_range(F * 1000, T * 1000).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let er = ExpandRequest { sql: &tpl.text, values: &resolved, range, interval, max_query_size: 1 << 20 };
    let first = expand(&er);
    // R5: the same inputs give the same bytes.
    assert_eq!(first, expand(&er), "not deterministic\n{}", ctx());

    let has_heredoc_ref = tpl.pieces.iter().any(|p| matches!(p, Piece::HeredocRef { .. }));
    let saved: SaveCheckResult = save_checks(&tpl.text, SqlUse::Panel, defs);
    match &first {
        Err(e) => {
            match e {
                ExpandError::ValueRefused(r) => {
                    stats.refused += 1;
                    stats.structure += usize::from(r.reason.contains("structure"));
                }
                _ => stats.invalid += 1,
            }
            // R6: a template that passes the statement check never fails it after the expansion.
            if tpl.statement_ok {
                assert!(
                    !matches!(e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Statement),
                    "the expansion broke the statement: {e:?}\n{}",
                    ctx()
                );
            }
            // A fault of the document that the expansion finds is also a fault at save.
            if e.is_document_fault() {
                assert!(!saved.faults.is_empty(), "no fault at save for {e:?}\n{}", ctx());
            }
        }
        Ok(out) => {
            stats.ok += 1;
            assert!(!has_heredoc_ref, "a placeholder in a heredoc was expanded\n{}", ctx());
            assert!(tpl.statement_ok, "a template that fails the statement check gave SQL\n{}\n{out:?}", ctx());
            let tokens = lex(out).unwrap_or_else(|e| panic!("the output does not lex: {e}\n{}\n{out:?}", ctx()));
            let segs = segments(out, &tokens, tpl.pieces.len())
                .unwrap_or_else(|| panic!("the sentinels are not in order\n{}\n{out:?}", ctx()));
            for (piece, seg) in tpl.pieces.iter().zip(&segs) {
                if let Err(why) = check_segment(piece, seg, req) {
                    panic!("{why}\npiece: {piece:?}\nsegment: {seg:?}\n{}\noutput: {out:?}", ctx());
                }
            }
        }
    }
    Some(first)
}

/// The outcome of an expansion with no check: the text or the error, and the faults and warnings at save.
fn outcome(defs: &[VariableDef], req: &Request, tpl: &Template) -> (Result<String, ExpandError>, String) {
    let saved = format!("{:?}", save_checks(&tpl.text, SqlUse::Panel, defs));
    let resolved = match resolve_values(defs, &req.values) {
        Ok(r) => r,
        Err(e) => {
            return (
                Err(ExpandError::ValueRefused(match e {
                    ValuesError::Refused(r) => r,
                    other => panic!("{other:?}"),
                })),
                saved,
            );
        }
    };
    let range = check_time_range(F * 1000, T * 1000).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let er = ExpandRequest { sql: &tpl.text, values: &resolved, range, interval, max_query_size: 1 << 20 };
    (expand(&er), saved)
}

fn run_case(rng: &mut Rng, seed: u64, case: usize, stats: &mut Stats) {
    let req = make_request(rng);
    let tpl = make_template(rng);
    let ctx = || format!("seed {seed} case {case}\ntemplate: {:?}\nrequest: {:?}", tpl.text, req.values);
    stats.cases += 1;

    // The base run: each variable is closed.
    let closed = defs_with(false, false);
    let Some(base) = check_run(&closed, &req, &tpl, &ctx, stats) else { return };

    // 16.4: the flag of a `query` variable. The expansion and the checks at save give the same result.
    let query_open = defs_with(true, false);
    assert_eq!(
        outcome(&query_open, &req, &tpl),
        outcome(&closed, &req, &tpl),
        "allow_custom_value changed a query variable\n{}",
        ctx()
    );
    assert_eq!(outcome(&query_open, &req, &tpl).0, base, "{}", ctx());

    // 16.4: the same values with each `custom` variable open. Each value is free text: P holds.
    let custom_open = defs_with(false, true);
    let opened = Request {
        values: req.values.clone(),
        vars: req.vars.iter().map(|(n, c)| (*n, c.opened())).collect(),
        has_control: req.has_control,
    };
    let mut open_stats = Stats::default();
    let open_run = check_run(&custom_open, &opened, &tpl, &ctx, &mut open_stats);
    stats.open_ok += usize::from(matches!(open_run, Some(Ok(_))));
    stats.open_runs += 1;
}

fn run_seed(seed: u64, cases: usize, stats: &mut Stats) {
    let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
    for case in 0..cases {
        run_case(&mut rng, seed, case, stats);
    }
}

fn env_number(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

#[test]
fn random_substitution_seeds() {
    let cases = env_number("HUNE_TEST_CASES").unwrap_or(5000) as usize;
    let seeds: Vec<u64> = match env_number("HUNE_TEST_SEED") {
        Some(s) => vec![s],
        None => (1..=env_number("HUNE_TEST_SEEDS").unwrap_or(64)).collect(),
    };
    // The seeds run on all cores. Each seed has its own generator, so the cases are the same as on one thread.
    let stats: Stats = seed_threads::run_seeds(&seeds, |seed, stats| run_seed(seed, cases, stats));
    eprintln!(
        "random: {} cases, {} expanded, {} refused ({} by the property P), {} invalid; open custom: {} runs, {} expanded",
        stats.cases, stats.ok, stats.refused, stats.structure, stats.invalid, stats.open_runs, stats.open_ok
    );
    assert!(stats.open_ok * 100 >= stats.open_runs * 15, "too few expansions with open custom variables");
    // The test must exercise both outcomes.
    assert!(stats.ok * 100 >= stats.cases * 20, "too few expansions: {} of {}", stats.ok, stats.cases);
    if env_number("HUNE_TEST_SEED").is_none() {
        assert!(stats.structure > 0, "the property P never fired: the test does not reach it");
    }
    assert!(stats.refused * 100 >= stats.cases * 5, "too few refusals: {} of {}", stats.refused, stats.cases);
}

// ---- R7: the lexer ----

fn random_sql(rng: &mut Rng, pieces: usize) -> String {
    const P: &[&str] = &[
        "'",
        "\"",
        "`",
        "\\",
        "-",
        "--",
        "#",
        "# ",
        "#!",
        "/",
        "*",
        "/*",
        "*/",
        "$",
        "$$",
        "$a$",
        " ",
        "\n",
        "a",
        "KILL",
        "QUERY",
        "1",
        ".",
        "(",
        ")",
        ";",
        "\u{a0}",
        ",",
        "\r",
        "e",
        "x",
        "\u{2028}",
        "\u{1f600}",
        "\u{e9}",
        "\u{feff}",
        "$a",
        "${a}",
        "\0",
        "0x",
        "1e",
        "+",
    ];
    (0..pieces).map(|_| *rng.pick(P)).collect()
}

#[test]
fn random_lexer_never_panics_and_covers_the_text() {
    let seeds = env_number("HUNE_TEST_SEEDS").unwrap_or(64);
    for seed in 1..=seeds {
        let mut rng = Rng(seed ^ 0xABCD);
        for _ in 0..1000 {
            let n = rng.below(30);
            let sql = random_sql(&mut rng, n);
            let tokens = lex_all(&sql);
            let mut at = 0;
            let mut joined = String::new();
            for t in &tokens {
                assert_eq!(t.start, at, "seed {seed}: {sql:?}");
                assert!(t.end > t.start && sql.is_char_boundary(t.end), "seed {seed}: {sql:?}");
                joined.push_str(t.text(&sql));
                at = t.end;
            }
            assert_eq!(joined, sql, "seed {seed}");
            let _ = lex(&sql);
        }
    }
}

// ---- the other random properties ----

/// A free value that the lexer would read as a comment or a quote mark never changes the tokens
/// outside its string literal. This test uses one template and many hostile values.
#[test]
fn random_one_string_literal_holds_any_value() {
    let defs = vec![VariableDef::text("t", TextValueType::Text)];
    let range = check_time_range(0, 1000).unwrap();
    for seed in 1..=64u64 {
        let mut rng = Rng(seed);
        for _ in 0..200 {
            let v = hostile_value(&mut rng);
            if has_control(&v) {
                continue;
            }
            let mut r = RequestValues::new();
            r.insert("t", false, &[&v]);
            let resolved = resolve_values(&defs, &r).unwrap();
            let sql = "SELECT 'a${t}b' AS x, \"y\" -- $t\nFROM z WHERE k = '$t'";
            let er = ExpandRequest { sql, values: &resolved, range, interval: None, max_query_size: 1 << 16 };
            let out = expand(&er).unwrap_or_else(|e| panic!("seed {seed}: {v:?}: {e:?}"));
            let tokens = lex(&out).unwrap();
            let strings: Vec<String> = tokens
                .iter()
                .filter(|t| t.kind == TokenKind::String)
                .map(|t| decode(&t.text(&out)[1..t.text(&out).len() - 1], '\''))
                .collect();
            assert_eq!(strings, vec![format!("a{v}b"), v.clone()], "seed {seed}: {v:?}");
            let kinds: Vec<TokenKind> = tokens.iter().filter(|t| !t.kind.is_trivia()).map(|t| t.kind).collect();
            assert_eq!(kinds.len(), 12, "seed {seed}: {v:?}: {out:?}");
        }
    }
}

// ---- the dialect `postgres`: the property P (7.24, 16.4) ----
//
// The generator builds a template from hostile pieces of the dialect: `E'` strings, dollar quotes, `U&`
// forms, nested comments, `#`, a backslash before a quote mark, Unicode spaces and operator runs. It puts
// references in a string, an `E` string, a quoted name, and in code as a number and as a string literal.
// The oracle is the structure: a hostile value must give the token kinds that a harmless value gives, or an
// error. A free text in a literal must decode to the value.

mod pg {
    use super::{Rng, has_control, hostile_value, number_value};
    use hune_core::sql::{
        Dialect, ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, SqlUse, TextValueType, Token, TokenKind,
        VariableDef, check_time_range, expand_for, lex_all_for, resolve_values, save_checks_for,
    };

    const PG_PIECES: &[&str] = &[
        "E'",
        "e'",
        "U&\"",
        "U&'",
        "B'",
        "X'",
        "$$",
        "$q$",
        "$1",
        "/* /*",
        "*/",
        "--",
        "#",
        "\\",
        "\\'",
        "''",
        "\\\\",
        "@-",
        "-",
        "+-",
        "\u{a0}",
        "\u{3000}",
        "' OR 1=1 --",
        "$$ $$",
        "${x}",
        "$t",
        "[[t]]",
        "::",
        "~!",
        "<-",
    ];

    fn pg_value(rng: &mut Rng) -> String {
        let mut s = hostile_value(rng);
        if rng.percent(40) {
            s.push_str(rng.pick(PG_PIECES));
        }
        if rng.percent(40) {
            s.insert_str(0, rng.pick(PG_PIECES));
        }
        s.retain(|c| !has_control(&c.to_string()));
        while s.len() > 1024 {
            s.pop();
        }
        s
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Ctx {
        Std,
        EStr,
        Name,
        CodeNumber,
        CodeString,
        /// A reference where the dialect refuses it: dollar quote, prefixed string, `U&` name.
        Bad,
    }

    struct Piece {
        ctx: Ctx,
        var: String,
        before: String,
        after: String,
        glue_before: &'static str,
        glue_after: &'static str,
    }

    const GLUE: &[&str] = &[" ", " ", " ", "\n", "", "@", "x", "E", "-", "1", "/", "*", "$", "U&", "(", ")", ",", "+"];
    const SEPARATORS: &[&str] = &[" ", "\n", " /* c */ ", " -- c\n", "  ", " /* a /* b */ c */ "];

    fn reference(p: &Piece) -> String {
        match p.ctx {
            Ctx::CodeString => format!("${{{}:singlequote}}", p.var),
            _ => format!("${{{}}}", p.var),
        }
    }

    fn template_text(pieces: &[Piece], rng: &mut Rng) -> String {
        let mut t = String::from("SELECT 1");
        for p in pieces {
            t.push_str(rng.pick(SEPARATORS));
            t.push_str(p.glue_before);
            match p.ctx {
                Ctx::Std => t.push_str(&format!("'{}{}{}'", p.before, reference(p), p.after)),
                Ctx::EStr => t.push_str(&format!("E'{}{}{}'", p.before, reference(p), p.after)),
                Ctx::Name => t.push_str(&format!("\"{}{}{}\"", p.before, reference(p), p.after)),
                Ctx::CodeNumber | Ctx::CodeString => t.push_str(&reference(p)),
                Ctx::Bad => {
                    let (open, close) =
                        *rng_pick(rng, &[("$$", "$$"), ("$q$", "$q$"), ("B'", "'"), ("U&'", "'"), ("U&\"", "\"")]);
                    t.push_str(&format!("{open}{}{}{}{close}", p.before, reference(p), p.after));
                }
            }
            t.push_str(p.glue_after);
        }
        t
    }

    fn rng_pick<'a, T>(rng: &mut Rng, items: &'a [T]) -> &'a T {
        rng.pick(items)
    }

    /// The kind of the variables that are not in a quoted name (16.4): a `text` variable, or a `query`
    /// variable with `allow_custom_value` true or false. The three give the same result.
    #[derive(Clone, Copy)]
    enum Kind {
        Text,
        Query(bool),
    }

    fn defs_for(pieces: &[Piece], values: &[String]) -> Vec<VariableDef> {
        defs_of_kind(pieces, values, Kind::Text)
    }

    fn defs_of_kind(pieces: &[Piece], values: &[String], kind: Kind) -> Vec<VariableDef> {
        pieces
            .iter()
            .zip(values)
            .map(|(p, v)| match (p.ctx, kind) {
                (Ctx::Name, _) => VariableDef::custom(&p.var, &[v.as_str()], false, false, None),
                (_, Kind::Query(open)) => VariableDef::query(&p.var, false, false, None).with_open(open),
                _ => VariableDef::text(&p.var, TextValueType::Text),
            })
            .collect()
    }

    fn expand_with(sql: &str, pieces: &[Piece], values: &[String]) -> Result<String, ExpandError> {
        expand_kind(sql, pieces, values, Kind::Text)
    }

    fn expand_kind(sql: &str, pieces: &[Piece], values: &[String], kind: Kind) -> Result<String, ExpandError> {
        let defs = defs_of_kind(pieces, values, kind);
        let mut request = RequestValues::new();
        for (p, v) in pieces.iter().zip(values) {
            request.insert(&p.var, false, &[v.as_str()]);
        }
        let resolved = match resolve_values(&defs, &request) {
            Ok(r) => r,
            Err(hune_core::sql::ValuesError::Refused(r)) => return Err(ExpandError::ValueRefused(r)),
            Err(e) => panic!("{e}"),
        };
        let range = check_time_range(0, 1000).unwrap();
        let req = ExpandRequest { sql, values: &resolved, range, interval: None, max_query_size: 1 << 20 };
        expand_for(Dialect::Postgres, &req)
    }

    /// The units of an output: the kind, the text of a token that holds no value, and the index of the first
    /// token. A minus before a number joins the number.
    type Unit = (TokenKind, Option<String>, usize);

    fn skeleton(out: &str, tokens: &[Token]) -> Vec<Unit> {
        let mut v = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            let first = i;
            let mut kind = t.kind;
            let text = t.text(out);
            if kind == TokenKind::Operator
                && text == "-"
                && tokens.get(i + 1).is_some_and(|n| n.kind == TokenKind::Number)
            {
                i += 1;
                kind = TokenKind::Number;
            }
            let keep =
                !matches!(kind, TokenKind::Number | TokenKind::String | TokenKind::EString | TokenKind::QuotedName);
            v.push((kind, keep.then(|| text.to_owned()), first));
            i += 1;
        }
        v
    }

    fn same_structure(a: &[Unit], b: &[Unit]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0 == y.0 && x.1 == y.1)
    }

    fn decode_std(s: &str) -> String {
        s.replace("''", "'")
    }

    fn decode_name(s: &str) -> String {
        s.replace("\"\"", "\"")
    }

    fn decode_e(s: &str) -> Option<String> {
        let mut out = String::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '\\' => match it.next() {
                    Some('\\') => out.push('\\'),
                    _ => return None,
                },
                '\'' => match it.next() {
                    Some('\'') => out.push('\''),
                    _ => return None,
                },
                c => out.push(c),
            }
        }
        Some(out)
    }

    #[derive(Default)]
    struct Stats {
        cases: usize,
        skipped: usize,
        ok: usize,
        refused: usize,
        bad_refused: usize,
    }

    impl crate::seed_threads::Merge for Stats {
        fn merge(&mut self, other: Self) {
            self.cases += other.cases;
            self.skipped += other.skipped;
            self.ok += other.ok;
            self.refused += other.refused;
            self.bad_refused += other.bad_refused;
        }
    }

    fn run_case(rng: &mut Rng, seed: u64, case: usize, stats: &mut Stats) {
        stats.cases += 1;
        let count = 1 + rng.below(4);
        let mut pieces = Vec::new();
        for k in 0..count {
            let ctx = match rng.below(100) {
                0..=24 => Ctx::Std,
                25..=44 => Ctx::EStr,
                45..=54 => Ctx::Name,
                55..=74 => Ctx::CodeNumber,
                75..=94 => Ctx::CodeString,
                _ => Ctx::Bad,
            };
            let affix = |rng: &mut Rng| if rng.percent(50) { String::new() } else { "p".to_owned() };
            pieces.push(Piece {
                ctx,
                var: format!("v{k}"),
                before: affix(rng),
                after: affix(rng),
                glue_before: if ctx == Ctx::CodeNumber || ctx == Ctx::CodeString { rng.pick(GLUE) } else { " " },
                glue_after: if ctx == Ctx::CodeNumber || ctx == Ctx::CodeString { rng.pick(GLUE) } else { " " },
            });
        }
        let sql = template_text(&pieces, rng);
        // Harmless values.
        let neutral: Vec<String> = pieces
            .iter()
            .enumerate()
            .map(|(i, p)| if p.ctx == Ctx::CodeNumber { "7".to_owned() } else { format!("QQ{i}") })
            .collect();
        let neutral_out = expand_with(&sql, &pieces, &neutral);
        // A reference counts only when the lexer puts it in a dollar quote, a prefixed string or a `U&` name:
        // the glue of the generator can change the tokens.
        let has_bad = lex_all_for(Dialect::Postgres, &sql).iter().any(|t| {
            let text = t.text(&sql);
            let opaque = matches!(t.kind, TokenKind::Heredoc | TokenKind::PrefixedString)
                || (t.kind == TokenKind::QuotedName && text.len() > 2 && text[..2].eq_ignore_ascii_case("u&"));
            opaque && pieces.iter().any(|p| p.ctx == Ctx::Bad && text.contains(&format!("${{{}}}", p.var)))
        });
        // Hostile values.
        let hostile: Vec<String> = pieces
            .iter()
            .map(|p| if p.ctx == Ctx::CodeNumber && rng.percent(60) { number_value(rng) } else { pg_value(rng) })
            .collect();
        if hostile.iter().any(|v| has_control(v)) {
            stats.skipped += 1;
            return;
        }
        let result = expand_with(&sql, &pieces, &hostile);
        // The member `allow_custom_value` of a `query` variable changes nothing.
        for open in [false, true] {
            assert_eq!(
                expand_kind(&sql, &pieces, &hostile, Kind::Query(open)),
                result,
                "seed {seed} case {case}: a query variable with allow_custom_value {open} differs: {sql:?}"
            );
        }
        if has_bad {
            // A reference in a dollar quote, a prefixed string or a `U&` name is a document error, and the checks
            // at save find it too. Another error may come first, in the text order.
            assert!(result.is_err(), "seed {seed} case {case}: {sql:?} gave {result:?}");
            if let Err(ExpandError::Invalid(i)) = &result
                && i.kind == SqlInvalidKind::Context
            {
                let defs = defs_for(&pieces, &hostile);
                let saved = save_checks_for(Dialect::Postgres, &sql, SqlUse::Panel, &defs);
                assert!(!saved.faults.is_empty(), "seed {seed} case {case}: {sql:?}");
            }
            stats.bad_refused += 1;
            return;
        }
        let neutral_out = match neutral_out {
            Ok(o) => o,
            Err(_) => {
                // The template itself does not expand (for example a lexer error of the glue).
                stats.skipped += 1;
                return;
            }
        };
        let out = match result {
            Ok(o) => o,
            Err(ExpandError::Invalid(i)) => {
                panic!("seed {seed} case {case}: {sql:?} with {hostile:?}: invalid {i:?} while harmless values pass")
            }
            Err(_) => {
                stats.refused += 1;
                return;
            }
        };
        stats.ok += 1;
        let hostile_tokens = lex_all_for(Dialect::Postgres, &out);
        let neutral_tokens = lex_all_for(Dialect::Postgres, &neutral_out);
        let (hs, ns) = (skeleton(&out, &hostile_tokens), skeleton(&neutral_out, &neutral_tokens));
        assert!(
            same_structure(&hs, &ns),
            "seed {seed} case {case}: {sql:?} with {hostile:?} gave {out:?}, harmless {neutral_out:?}"
        );
        // A free text in a literal decodes to the value. The marker of the harmless output gives the unit.
        for (i, p) in pieces.iter().enumerate() {
            let marker = format!("QQ{i}");
            let Some(unit) = ns.iter().position(|u| neutral_tokens[u.2].text(&neutral_out).contains(&marker)) else {
                continue;
            };
            let t = &hostile_tokens[hs[unit].2];
            let Some((lo, hi)) = t.content_range(&out) else { continue };
            let content = &out[lo..hi];
            let want = format!("{}{}{}", p.before, hostile[i], p.after);
            // A string after an `E` string and a line end continues it, so the kind of the harmless token rules.
            let value_only = if p.ctx == Ctx::CodeString { hostile[i].clone() } else { want.clone() };
            let decoded = match (p.ctx, t.kind) {
                (Ctx::Name, TokenKind::QuotedName) => Some(decode_name(content)),
                (Ctx::Std | Ctx::CodeString | Ctx::EStr, TokenKind::String) => Some(decode_std(content)),
                (Ctx::Std | Ctx::CodeString | Ctx::EStr, TokenKind::EString) => decode_e(content),
                _ => None,
            };
            assert_eq!(decoded.as_deref(), Some(value_only.as_str()), "seed {seed} case {case}: {sql:?} gave {out:?}");
        }
    }

    #[test]
    fn random_postgres_property_p() {
        let cases = super::env_number("HUNE_TEST_CASES").unwrap_or(3000) as usize;
        let seeds: Vec<u64> = match super::env_number("HUNE_TEST_SEED") {
            Some(s) => vec![s],
            None => (1..=super::env_number("HUNE_TEST_SEEDS").unwrap_or(32)).collect(),
        };
        // The seeds run on all cores. Each seed has its own generator, so the cases are the same as on one thread.
        let stats: Stats = crate::seed_threads::run_seeds(&seeds, |seed, stats| {
            let mut rng = Rng(seed ^ 0x5047);
            for case in 0..cases {
                run_case(&mut rng, seed, case, stats);
            }
        });
        eprintln!(
            "random pg: {} cases, {} expanded, {} refused, {} bad context, {} skipped",
            stats.cases, stats.ok, stats.refused, stats.bad_refused, stats.skipped
        );
        assert!(stats.ok * 100 >= stats.cases * 15, "too few expansions: {} of {}", stats.ok, stats.cases);
        assert!(stats.refused > 0, "the property P never fired");
        assert!(stats.bad_refused > 0);
    }
}
