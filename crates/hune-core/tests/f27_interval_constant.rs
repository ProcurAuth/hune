// SPDX-License-Identifier: MIT OR Apache-2.0
//! The variable kinds `interval` and `constant` in the engine (F27, 4.10a, 4.10b, 7.4a, 7.7, 7.8, 7.10):
//! the set check of an `interval` value, the text of "Auto", the value of a `constant`, the contexts, the
//! hostile sets and the seeded random tests of 16.4.
//!
//! Run one seed again: `HUNE_TEST_SEED=4242 cargo test -p hune-core --locked f27_random`.

use hune_core::sql::{
    ExpandError, ExpandRequest, RequestValues, Selection, SqlInvalidKind, SqlUse, TextValueType, TimeRange,
    ValuesError, VariableClass, VariableDef, check_time_range, compute_interval, expand, lex_all, resolve_values,
    resolve_values_at, save_checks,
};

const FROM: i64 = 1_794_211_200_000;
/// 6 hours.
const TO: i64 = 1_794_232_800_000;
const HOSTILE: &str = "a'b\"c$d\\e;--x";

fn range_of(len_ms: i64) -> TimeRange {
    check_time_range(FROM, FROM + len_ms).unwrap()
}

fn defs() -> Vec<VariableDef> {
    vec![
        VariableDef::interval("bucket", &["1m", "10m", "1h", "1d"], Some((30, "10s"))),
        VariableDef::interval("fixed", &["5m", "1w"], None),
        VariableDef::constant("k", HOSTILE),
        VariableDef::constant("empty", ""),
        VariableDef::text("t", TextValueType::Text),
        VariableDef::query("q", false, false, None),
    ]
}

fn request(items: &[(&str, bool, &[&str])]) -> RequestValues {
    let mut r = RequestValues::new();
    for (n, a, v) in items {
        r.insert(n, *a, v);
    }
    r
}

fn refusal(r: Result<hune_core::sql::ResolvedValues, ValuesError>) -> (String, String) {
    match r {
        Err(ValuesError::Refused(x)) => (x.variable, x.reason),
        other => panic!("{other:?}"),
    }
}

fn expand_with(sql: &str, req: &RequestValues, len_ms: i64) -> Result<String, ExpandError> {
    let range = range_of(len_ms);
    let resolved = resolve_values_at(&defs(), req, &range).map_err(|e| match e {
        ValuesError::Refused(r) => ExpandError::ValueRefused(r),
        ValuesError::BadRequest(m) => panic!("bad request: {m}"),
    })?;
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    expand(&ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 262_144 })
}

fn six_hours() -> i64 {
    TO - FROM
}

// ---------------------------------------------------------------- the set check of an interval

#[test]
fn an_item_gives_one_author_value() {
    let range = range_of(six_hours());
    for item in ["1m", "10m", "1h", "1d"] {
        let r = resolve_values_at(&defs(), &request(&[("bucket", false, &[item])]), &range).unwrap();
        let got = r.get("bucket").unwrap();
        assert_eq!(got.class, VariableClass::Interval);
        assert_eq!(got.selection, Selection::Single { value: item.to_owned(), author: true });
        assert!(!got.all && !got.open);
    }
}

#[test]
fn a_value_that_is_not_an_item_is_refused() {
    let range = range_of(six_hours());
    // In range of the grammar, out of the list, and the case variants of an item.
    for value in [
        "7m",
        "2m",
        "1M",
        "1H",
        "1D",
        "10M",
        "1h ",
        " 1h",
        "01m",
        "+1m",
        "1.5h",
        "60m",
        "3600s",
        "1w",
        "1y",
        "",
        "$__auto_interval_bucket",
        "$__AUTO",
        "$__auto ",
        "$__auto\n",
        "1m\u{0}",
    ] {
        let (var, reason) = refusal(resolve_values_at(&defs(), &request(&[("bucket", false, &[value])]), &range));
        assert_eq!(var, "bucket", "{value:?}");
        assert!(!reason.is_empty(), "{value:?}");
    }
    // The shape: 0 values, 2 values, "All".
    for (all, values) in [(false, &[][..]), (false, &["1m", "10m"][..]), (true, &[][..]), (true, &["1m"][..])] {
        let (var, _) = refusal(resolve_values_at(&defs(), &request(&[("bucket", all, values)]), &range));
        assert_eq!(var, "bucket");
    }
}

#[test]
fn auto_needs_auto_true() {
    let range = range_of(six_hours());
    let ok = resolve_values_at(&defs(), &request(&[("bucket", false, &["$__auto"])]), &range).unwrap();
    assert_eq!(ok.get("bucket").unwrap().selection, Selection::Single { value: "10m".into(), author: true });
    // The variable `fixed` has `auto` false: `$__auto` is not an item.
    let (var, reason) = refusal(resolve_values_at(&defs(), &request(&[("fixed", false, &["$__auto"])]), &range));
    assert_eq!((var.as_str(), reason.as_str()), ("fixed", "The value is not an item of the variable."));
    // With no time range the value cannot get its text: the plain function refuses it.
    let (var, _) = refusal(resolve_values(&defs(), &request(&[("bucket", false, &["$__auto"])])));
    assert_eq!(var, "bucket");
    // An item does not need the range.
    assert!(resolve_values(&defs(), &request(&[("bucket", false, &["1h"])])).is_ok());
}

#[test]
fn a_hostile_request_never_reaches_the_sql() {
    let hostile = [
        "1m'; DROP TABLE x; --",
        "1m' OR '1'='1",
        "1m\"",
        "1m`",
        "1m)",
        "1m\\",
        "1m;",
        "1m--",
        "1m/*",
        "1m\u{0}",
        "1m\r\n",
        "$__interval",
        "${bucket}",
        "$bucket",
        "$__from",
        "$__auto'",
        "{}",
        "../1m",
        "1m\t",
        "\u{ff11}m",
        "1\u{ff4d}",
        "1m\u{200b}",
        "1e3m",
        "0x10m",
        "1 m",
        "m",
        "1",
        "-1m",
        "9999999999999999999999m",
        "1m,10m",
        "[1m]",
        "'1m'",
        "\"1m\"",
        "NULL",
        "1m OR 1=1",
        "toDateTime(1)",
        "1s'; SELECT 1 --",
        "$__timeFilter(ts)",
        "1m UNION SELECT 1",
        "\u{202e}m1",
    ];
    assert_eq!(hostile.len(), 40);
    let range = range_of(six_hours());
    for value in hostile {
        let req = request(&[("bucket", false, &[value])]);
        let (var, _) = refusal(resolve_values_at(&defs(), &req, &range));
        assert_eq!(var, "bucket", "{value:?}");
        // The expansion never starts: the request stops at the set check.
        let sql = "SELECT parseTimeDelta('${bucket}')";
        assert!(matches!(expand_with(sql, &req, six_hours()), Err(ExpandError::ValueRefused(_))), "{value:?}");
    }
}

#[test]
fn the_set_check_with_50_items_and_10000_values() {
    let items: Vec<String> = (1..=50).map(|i| format!("{i}m")).collect();
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let defs = [VariableDef::interval("iv", &refs, Some((1_000, "1ms")))];
    let range = range_of(six_hours());
    for item in &items {
        assert!(resolve_values_at(&defs, &request(&[("iv", false, &[item])]), &range).is_ok(), "{item}");
    }
    assert!(resolve_values_at(&defs, &request(&[("iv", false, &["51m"])]), &range).is_err());
    // 10,000 values: the check refuses the shape. 10,001 values: the check refuses the count.
    for count in [10_000, 10_001] {
        let many = vec!["1m"; count];
        let mut req = RequestValues::new();
        req.insert("iv", false, &many);
        assert!(refusal(resolve_values_at(&defs, &req, &range)).1.len() > 3, "{count}");
    }
}

// ----------------------------------------------------------------------- the text of "Auto"

#[test]
fn auto_gives_the_table_text_for_the_range() {
    let req = request(&[("bucket", false, &["$__auto"])]);
    let sql = "SELECT '${bucket}', ${bucket}";
    // 6 h, 5 min and 30 d with 30 steps and a minimum of 10 s (7.4a).
    for (len, text) in [(six_hours(), "10m"), (300_000, "10s"), (30 * 86_400_000, "1d")] {
        assert_eq!(expand_with(sql, &req, len).unwrap(), format!("SELECT '{text}', {text}"), "{len}");
    }
    // The same text goes to each query of the request, with any max_data_points.
    let range = range_of(six_hours());
    let resolved = resolve_values_at(&defs(), &req, &range).unwrap();
    for points in [1, 100, 1200, 10_000] {
        let interval = Some(compute_interval(&range, points, 1).unwrap());
        let er =
            ExpandRequest { sql: "SELECT '${bucket}'", values: &resolved, range, interval, max_query_size: 1 << 20 };
        assert_eq!(expand(&er).unwrap(), "SELECT '10m'");
    }
    // The text is author text: it stays whole in a quoted name, and the macro `$__interval_s` is not changed.
    assert_eq!(expand_with("SELECT `${bucket}`", &req, six_hours()).unwrap(), "SELECT `10m`");
    let s = expand_with("SELECT $__interval_s, '${bucket}'", &req, six_hours()).unwrap();
    assert_eq!(s, "SELECT 20, '10m'");
}

#[test]
fn the_text_of_auto_in_every_format() {
    let req = request(&[("bucket", false, &["$__auto"])]);
    assert_eq!(expand_with("SELECT '${bucket:raw}'", &req, six_hours()).unwrap(), "SELECT '10m'");
    assert_eq!(expand_with("SELECT ${bucket:singlequote}", &req, six_hours()).unwrap(), "SELECT '10m'");
    assert_eq!(expand_with("SELECT ${bucket:sqlstring}", &req, six_hours()).unwrap(), "SELECT '10m'");
    assert_eq!(expand_with("SELECT '${bucket:csv}'", &req, six_hours()).unwrap(), "SELECT '10m'");
    assert_eq!(expand_with("SELECT '${bucket:text}'", &req, six_hours()).unwrap(), "SELECT '10m'");
    assert_eq!(expand_with("SELECT '[[bucket]]'", &req, six_hours()).unwrap(), "SELECT '[[bucket]]'");
}

// ---------------------------------------------------------------------------- a constant

#[test]
fn a_constant_takes_the_value_of_the_document() {
    let range = range_of(six_hours());
    // No member: the document gives the value.
    let r = resolve_values_at(&defs(), &RequestValues::new(), &range).unwrap();
    let got = r.get("k").unwrap();
    assert_eq!(got.class, VariableClass::Constant);
    assert_eq!(got.selection, Selection::Single { value: HOSTILE.to_owned(), author: true });
    assert_eq!(r.get("empty").unwrap().selection, Selection::Single { value: String::new(), author: true });
    // The equal member: the same result.
    let same = resolve_values_at(&defs(), &request(&[("k", false, &[HOSTILE])]), &range).unwrap();
    assert_eq!(same.get("k"), r.get("k"));
    // A different member, "All", no value, two values: refused with the reason of 7.7.
    for (all, values) in [
        (false, &["zzz"][..]),
        (false, &[""][..]),
        (false, &[][..]),
        (false, &[HOSTILE, HOSTILE][..]),
        (true, &[][..]),
        (true, &[HOSTILE][..]),
    ] {
        let (var, reason) = refusal(resolve_values_at(&defs(), &request(&[("k", all, values)]), &range));
        assert_eq!((var.as_str(), reason.as_str()), ("k", "A constant takes its value from the document."));
    }
    let (var, _) = refusal(resolve_values_at(&defs(), &request(&[("empty", false, &["x"])]), &range));
    assert_eq!(var, "empty");
    // The URL value of a constant never reaches the SQL: the page does not send it, and a request that does is refused.
    assert!(matches!(
        expand_with("SELECT '${k}'", &request(&[("k", false, &["zzz"])]), six_hours()),
        Err(ExpandError::ValueRefused(_))
    ));
}

#[test]
fn a_constant_has_the_escapes_of_its_context() {
    let none = RequestValues::new();
    // A string: the escapes hold the text in its literal.
    let s = expand_with("SELECT '${k}' AS x", &none, six_hours()).unwrap();
    assert_eq!(s, "SELECT 'a\\'b\"c$d\\\\e;--x' AS x");
    let tokens = lex_all(&s);
    assert_eq!(tokens.iter().filter(|t| t.kind == hune_core::sql::TokenKind::String).count(), 1);
    // A quoted name: the name escapes apply.
    let n = expand_with("SELECT 1 FROM \"${k}\"", &none, six_hours()).unwrap();
    assert_eq!(n, "SELECT 1 FROM \"a'b\\\"c$d\\\\e;--x\"");
    let b = expand_with("SELECT 1 FROM `${k}`", &none, six_hours()).unwrap();
    assert_eq!(b, "SELECT 1 FROM `a'b\"c$d\\\\e;--x`");
    // Code: the text stays as it is. This is an author choice (7.8). The check of the final SQL still stops
    // a text that opens a string and never closes it.
    let e = expand_with("SELECT ${k} -- x", &none, six_hours()).unwrap_err();
    assert!(matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Lexer), "{e:?}");
    let plain = [VariableDef::constant("n", "42 + 1")];
    let range = range_of(1000);
    let resolved = resolve_values_at(&plain, &RequestValues::new(), &range).unwrap();
    let er =
        ExpandRequest { sql: "SELECT ${n}, $n", values: &resolved, range, interval: None, max_query_size: 1 << 20 };
    assert_eq!(expand(&er).unwrap(), "SELECT 42 + 1, 42 + 1");
    // An empty constant gives an empty text.
    assert_eq!(expand_with("SELECT '${empty}'", &none, six_hours()).unwrap(), "SELECT ''");
}

#[test]
fn the_value_is_not_read_again() {
    let defs = [VariableDef::constant("c", "$t ${t} $__from '$c'"), VariableDef::text("t", TextValueType::Text)];
    let range = range_of(1000);
    let mut req = RequestValues::new();
    req.insert("t", false, &["X"]);
    let resolved = resolve_values_at(&defs, &req, &range).unwrap();
    let er = ExpandRequest {
        sql: "SELECT '${c}', '${t}'",
        values: &resolved,
        range,
        interval: None,
        max_query_size: 1 << 20,
    };
    assert_eq!(expand(&er).unwrap(), "SELECT '$t ${t} $__from \\'$c\\'', 'X'");
}

// --------------------------------------------------------------------------- the contexts

#[test]
fn a_list_macro_or_a_heredoc_refuses_the_new_kinds() {
    let req = request(&[("bucket", false, &["1h"])]);
    // `$__conditionalAll` names a `query` or a `custom` variable only (7.10).
    for sql in ["SELECT $__conditionalAll(a = 1, $bucket)", "SELECT $__conditionalAll(a = 1, ${k})"] {
        let e = expand_with(sql, &req, six_hours()).unwrap_err();
        assert!(matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Macro), "{sql}: {e:?}");
        let saved = save_checks(sql, SqlUse::Panel, &defs());
        assert_eq!(saved.faults.len(), 1, "{sql}: {saved:?}");
    }
    // A heredoc and a dollar quote: a document error, as for a `custom` variable.
    for sql in ["SELECT $$ ${bucket} $$", "SELECT $tag$ ${k} $tag$"] {
        let e = expand_with(sql, &req, six_hours()).unwrap_err();
        assert!(matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Context), "{sql}: {e:?}");
    }
    // A missing member of an `interval` is a `bad_request`, as for any variable. A `constant` needs none.
    let range = range_of(six_hours());
    let resolved = resolve_values_at(&defs(), &RequestValues::new(), &range).unwrap();
    let er =
        ExpandRequest { sql: "SELECT '${bucket}'", values: &resolved, range, interval: None, max_query_size: 1 << 20 };
    assert!(matches!(expand(&er), Err(ExpandError::MissingVariable(n)) if n == "bucket"));
    let er = ExpandRequest { sql: "SELECT '${k}'", values: &resolved, range, interval: None, max_query_size: 1 << 20 };
    assert!(expand(&er).is_ok());
}

#[test]
fn the_save_checks_accept_the_new_kinds_where_a_closed_custom_variable_stands() {
    for sql in [
        "SELECT toStartOfInterval(ts, toIntervalSecond(parseTimeDelta('${bucket}'))) FROM t",
        "SELECT 1 FROM \"${k}\"",
        "SELECT '${k}', '${fixed}', ${bucket:singlequote}",
        "SELECT ${bucket:csv}",
    ] {
        let saved = save_checks(sql, SqlUse::Panel, &defs());
        assert!(saved.faults.is_empty(), "{sql}: {saved:?}");
    }
    // A `text` variable still cannot stand in a quoted name.
    let saved = save_checks("SELECT 1 FROM \"${t}\"", SqlUse::Panel, &defs());
    assert_eq!(saved.faults.len(), 1);
}

// ------------------------------------------------------------------ the seeded random tests

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

const PIECES: &[&str] =
    &["'", "\"", "`", "\\", "$", "${", "$__from", "--", "/*", "*/", ";", "(", ")", " ", "a", "1", ",", "\u{e9}", "%"];

fn hostile_text(rng: &mut Rng) -> String {
    let n = 1 + rng.below(8);
    (0..n).map(|_| *rng.pick(PIECES)).collect()
}

/// Decodes the content of a string token: a backslash gives the next character, a doubled quote gives one.
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

/// Reads the output `SELECT <literal> AS x` and gives the decoded content of the literal.
fn literal_of(output: &str, quote: char) -> Option<String> {
    let rest = output.strip_prefix("SELECT ")?.strip_suffix(" AS x")?;
    let kind = if quote == '\'' { hune_core::sql::TokenKind::String } else { hune_core::sql::TokenKind::QuotedName };
    let tokens = lex_all(rest);
    if tokens.len() != 1 || tokens[0].kind != kind || !tokens[0].closed || rest.len() < 2 {
        return None;
    }
    Some(decode(&rest[1..rest.len() - 1], quote))
}

fn run_case(rng: &mut Rng, seed: u64, case: usize) {
    let doc_const = hostile_text(rng);
    let items = ["1m", "10m", "1h", "1d"];
    let defs = vec![
        VariableDef::interval("iv", &items, Some((30, "10s"))),
        VariableDef::constant("k", &doc_const),
        VariableDef::text("t", TextValueType::Text),
        VariableDef::query("q", false, false, None),
    ];
    let range = range_of(six_hours());
    // The request: an item, `$__auto`, or a hostile text.
    let iv_value = match rng.below(4) {
        0 => rng.pick(&items).to_string(),
        1 => "$__auto".to_owned(),
        _ => hostile_text(rng),
    };
    let iv_ok = items.contains(&iv_value.as_str()) || iv_value == "$__auto";
    // The member of the constant: absent, equal or hostile.
    let k_member = match rng.below(3) {
        0 => None,
        1 => Some(doc_const.clone()),
        _ => Some(hostile_text(rng)),
    };
    let k_ok = k_member.as_ref().is_none_or(|m| *m == doc_const);
    let t_value = hostile_text(rng);
    let q_value = hostile_text(rng);
    let mut req = RequestValues::new();
    req.insert("iv", false, &[iv_value.as_str()]);
    if let Some(m) = &k_member {
        req.insert("k", false, &[m.as_str()]);
    }
    req.insert("t", false, &[t_value.as_str()]);
    req.insert("q", false, &[q_value.as_str()]);
    let label = format!("seed {seed} case {case}: iv {iv_value:?} k {k_member:?} const {doc_const:?}");
    let resolved = resolve_values_at(&defs, &req, &range);
    // R3: a refused value gives an error and no SQL. A value that is allowed gives a value.
    let has_control = |s: &str| s.chars().any(|c| c < '\u{20}' || c == '\u{7f}');
    let expect_ok = iv_ok && k_ok && !has_control(&iv_value) && !has_control(&t_value) && !has_control(&q_value);
    let resolved = match resolved {
        Ok(r) => {
            assert!(expect_ok, "{label}: accepted");
            r
        }
        Err(ValuesError::Refused(r)) => {
            assert!(!expect_ok, "{label}: refused {r:?}");
            let expected = if !iv_ok { "iv" } else { "k" };
            if !has_control(&iv_value) && !has_control(&t_value) && !has_control(&q_value) {
                assert_eq!(r.variable, expected, "{label}");
            }
            return;
        }
        Err(other) => panic!("{label}: {other:?}"),
    };
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let go = |sql: &str| expand(&ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 1 << 20 });
    let auto_text = "10m";
    let expected_iv = if iv_value == "$__auto" { auto_text } else { iv_value.as_str() };
    // R1: a value stays in its literal and decodes to itself. A string and a quoted name, for each kind.
    for (quote, open) in [('\'', "'"), ('"', "\"")] {
        let wrap = |name: &str| format!("SELECT {open}${{{name}}}{open} AS x");
        assert_eq!(literal_of(&go(&wrap("iv")).unwrap(), quote).as_deref(), Some(expected_iv), "{label}");
        assert_eq!(literal_of(&go(&wrap("k")).unwrap(), quote).as_deref(), Some(doc_const.as_str()), "{label}");
    }
    // P holds for the `query` and `text` variables, also with the new kinds in the same statement.
    for name in ["t", "q"] {
        let value = if name == "t" { &t_value } else { &q_value };
        let sql = format!("SELECT '${{iv}}', '${{{name}}}', '${{k}}' AS x");
        match go(&sql) {
            Ok(out) => {
                let tokens = lex_all(&out);
                let strings: Vec<_> = tokens.iter().filter(|t| t.kind == hune_core::sql::TokenKind::String).collect();
                assert_eq!(strings.len(), 3, "{label}: {out}");
                assert_eq!(decode(&strings[1].text(&out)[1..strings[1].text(&out).len() - 1], '\''), *value, "{label}");
                assert_eq!(
                    decode(&strings[2].text(&out)[1..strings[2].text(&out).len() - 1], '\''),
                    doc_const,
                    "{label}"
                );
            }
            Err(ExpandError::ValueRefused(_)) => {}
            Err(other) => panic!("{label}: {other:?}"),
        }
    }
    // R5: the same inputs give the same bytes.
    assert_eq!(go("SELECT '${iv}', '${k}'"), go("SELECT '${iv}', '${k}'"), "{label}");
}

fn env_number(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

#[test]
fn f27_random_interval_and_constant_seeds() {
    let cases = env_number("HUNE_TEST_CASES").unwrap_or(200) as usize;
    let seeds: Vec<u64> = match env_number("HUNE_TEST_SEED") {
        Some(s) => vec![s],
        None => (1..=env_number("HUNE_TEST_SEEDS").unwrap_or(32)).collect(),
    };
    for seed in seeds {
        let mut rng = Rng(seed);
        for case in 0..cases {
            run_case(&mut rng, seed, case);
        }
    }
}
