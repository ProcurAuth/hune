//! Tests of the expansion (7.6 to 7.13): macros, placeholders, contexts, formats, refusals and P.

use hune_core::sql::{
    ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, TextValueType, ValuesError, VariableDef,
    check_time_range, compute_interval, expand, resolve_values,
};

const FROM: i64 = 1_794_211_200_000;
const TO: i64 = 1_794_232_800_000;

struct Case {
    defs: Vec<VariableDef>,
    values: RequestValues,
}

impl Case {
    fn new(defs: Vec<VariableDef>) -> Case {
        Case { defs, values: RequestValues::new() }
    }

    fn set(mut self, name: &str, all: bool, values: &[&str]) -> Case {
        self.values.insert(name, all, values);
        self
    }

    fn run_with(&self, sql: &str, with_interval: bool, limit: usize) -> Result<String, ExpandError> {
        let resolved = resolve_values(&self.defs, &self.values).map_err(|e| match e {
            ValuesError::Refused(r) => ExpandError::ValueRefused(r),
            ValuesError::BadRequest(m) => panic!("bad request: {m}"),
        })?;
        let range = check_time_range(FROM, TO).unwrap();
        let interval = with_interval.then(|| compute_interval(&range, 1200, 1).unwrap());
        expand(&ExpandRequest { sql, values: &resolved, range, interval, max_query_size: limit })
    }

    fn run(&self, sql: &str) -> Result<String, ExpandError> {
        self.run_with(sql, true, 262_144)
    }

    fn ok(&self, sql: &str) -> String {
        self.run(sql).unwrap_or_else(|e| panic!("{sql:?}: {e:?}"))
    }

    fn refused(&self, sql: &str) -> String {
        match self.run(sql) {
            Err(ExpandError::ValueRefused(r)) => r.variable,
            other => panic!("{sql:?}: {other:?}"),
        }
    }

    fn invalid(&self, sql: &str) -> SqlInvalidKind {
        match self.run(sql) {
            Err(ExpandError::Invalid(i)) => i.kind,
            other => panic!("{sql:?}: {other:?}"),
        }
    }
}

fn none() -> Case {
    Case::new(vec![])
}

#[test]
fn worked_example_of_7_10() {
    let defs = vec![
        VariableDef::query("node", true, false, None),
        VariableDef::custom("env", &["prod", "dev"], false, false, None),
    ];
    let c = Case::new(defs).set("node", false, &["a1", "b'2"]).set("env", false, &["prod"]);
    let sql = "SELECT $__timeInterval(event_time) AS t, node, sum(weight) AS weight\n\
FROM analytics.weights\n\
WHERE $__timeFilter(event_time) AND node IN ($node) AND env = '$env'\n\
GROUP BY t, node\n\
ORDER BY t";
    let want = "SELECT toStartOfInterval(toDateTime(event_time), INTERVAL 20 second) AS t, node, sum(weight) AS weight\n\
FROM analytics.weights\n\
WHERE event_time >= toDateTime(1794211200) AND event_time <= toDateTime(1794232800) AND node IN ('a1','b\\'2') AND env = 'prod'\n\
GROUP BY t, node\n\
ORDER BY t";
    assert_eq!(c.ok(sql), want);
}

#[test]
fn the_five_macros() {
    let c = none();
    assert_eq!(
        c.ok("SELECT $__fromTime, $__toTime, $__interval_s"),
        "SELECT toDateTime(1794211200), toDateTime(1794232800), 20"
    );
    assert_eq!(
        c.ok("SELECT 1 WHERE $__timeFilter(t)"),
        "SELECT 1 WHERE t >= toDateTime(1794211200) AND t <= toDateTime(1794232800)"
    );
    assert_eq!(c.ok("SELECT $__timeInterval(t)"), "SELECT toStartOfInterval(toDateTime(t), INTERVAL 20 second)");
    // Blanks at the start and the end of the argument go. A comment stays.
    assert_eq!(
        c.ok("SELECT 1 WHERE $__timeFilter(  a.b\n )"),
        "SELECT 1 WHERE a.b >= toDateTime(1794211200) AND a.b <= toDateTime(1794232800)"
    );
    assert_eq!(
        c.ok("SELECT $__timeInterval( /* c */ t )"),
        "SELECT toStartOfInterval(toDateTime(/* c */ t), INTERVAL 20 second)"
    );
    // A macro with no argument followed by a blank and `(`.
    assert_eq!(c.ok("SELECT $__fromTime (1)"), "SELECT toDateTime(1794211200) (1)");
    // The argument keeps its own parentheses, a string with a parenthesis and a comma at depth 1.
    assert_eq!(
        c.ok("SELECT 1 WHERE $__timeFilter(toDateTime(f(a, ')'), 'UTC'))"),
        "SELECT 1 WHERE toDateTime(f(a, ')'), 'UTC') >= toDateTime(1794211200) AND toDateTime(f(a, ')'), 'UTC') <= toDateTime(1794232800)"
    );
    // The macros nest.
    assert_eq!(
        c.ok("SELECT $__timeInterval($__fromTime)"),
        "SELECT toStartOfInterval(toDateTime(toDateTime(1794211200)), INTERVAL 20 second)"
    );
}

/// D08: `$__dt` expands exactly as `$__dateTimeFilter` (7.10). The arguments and the property P are the same.
#[test]
fn the_alias_dt_is_date_time_filter() {
    let c = Case::new(vec![
        VariableDef::custom("col", &["ts", "t2"], false, false, None),
        VariableDef::text("t", TextValueType::Text),
        VariableDef::text("min", TextValueType::Number),
    ])
    .set("col", false, &["ts"])
    .set("t", false, &["'; DROP TABLE x; --"])
    .set("min", false, &["-5"]);
    let want = "SELECT 1 WHERE (toDate(ts) >= toDate('2026-11-09') AND toDate(ts) <= toDate('2026-11-09')) AND \
                (ts >= toDateTime(1794211200) AND ts <= toDateTime(1794232800))";
    for sql in [
        "SELECT 1 WHERE $__dt(toDate(ts), ts)",
        "SELECT 1 WHERE $__dateTimeFilter(toDate(ts), ts)",
        "SELECT 1 WHERE $__dt(  toDate($col) ,\n ${col} )",
    ] {
        assert_eq!(c.ok(sql), want, "{sql}");
    }
    // A variable query has no interval, and `$__dt` needs none.
    assert_eq!(c.run_with("SELECT 1 WHERE $__dt(toDate(ts), ts)", false, 1000).unwrap(), want);
    // P: a free text stays in its literal in each copy of the argument, and a number that joins its neighbours is refused.
    let out = c.ok("SELECT 1 WHERE $__dt(d, if(x = '$t', ts, ts))");
    let strings =
        hune_core::sql::lex(&out).unwrap().iter().filter(|k| k.kind == hune_core::sql::TokenKind::String).count();
    assert_eq!(strings, 4, "{out}");
    assert_eq!(c.refused("SELECT 1 WHERE $__dt(d, ts + 0-$min)"), "min");
    // The faults name `$__dt`, and the text stays outside code.
    for sql in
        ["SELECT $__dt", "SELECT $__dt()", "SELECT $__dt(a)", "SELECT $__dt(a, b, c)", "SELECT ${__dt:csv}(a, b)"]
    {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Macro, "{sql}");
    }
    assert_eq!(c.ok("SELECT '$__dt(a, b)' -- $__dt(a, b)"), "SELECT '$__dt(a, b)' -- $__dt(a, b)");
}

#[test]
fn macros_stay_text_outside_code() {
    let c = none();
    let sql = "SELECT '$__fromTime $__timeFilter(x)', \"$__toTime\", `$__interval_s` -- $__fromTime\n/* $__toTime */";
    assert_eq!(c.ok(sql), sql);
    // Not read in a heredoc either.
    assert_eq!(c.ok("SELECT $$ $__fromTime $$"), "SELECT $$ $__fromTime $$");
}

#[test]
fn the_interval_follows_the_request() {
    let range = check_time_range(0, 600_000).unwrap();
    let interval = compute_interval(&range, 1920, 1).unwrap();
    let resolved = resolve_values(&[], &RequestValues::new()).unwrap();
    let r = ExpandRequest {
        sql: "SELECT $__interval_s, $__timeInterval(t)",
        values: &resolved,
        range,
        interval: Some(interval),
        max_query_size: 1000,
    };
    assert_eq!(expand(&r).unwrap(), "SELECT 1, toStartOfInterval(toDateTime(t), INTERVAL 1 second)");
}

#[test]
fn macro_faults_are_sql_invalid() {
    let c = none();
    for sql in [
        "SELECT $__timeFilter",
        "SELECT $__timeFilter()",
        "SELECT $__timeFilter(a, b)",
        "SELECT $__timeFilter(a",
        "SELECT $__fromTime()",
        "SELECT $__toTime()",
        "SELECT $__timeFilter_ms",
        "SELECT $__from()",
        "SELECT $__range",
        "SELECT $__range_s",
        "SELECT $__range_ms",
        "SELECT $__nothing",
        "SELECT $__conditionalAll(a, $x)",
        "SELECT $__",
        "SELECT ${__fromTime:csv}",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Macro, "{sql}");
    }
    // The braced form of a macro is the macro.
    assert_eq!(c.ok("SELECT ${__fromTime}"), "SELECT toDateTime(1794211200)");
    // A variable query has no interval.
    for sql in
        ["SELECT $__interval_s", "SELECT $__timeInterval(t)", "SELECT $__interval_ms", "SELECT $__timeInterval_ms(t)"]
    {
        match c.run_with(sql, false, 1000) {
            Err(ExpandError::Invalid(i)) => assert_eq!(i.kind, SqlInvalidKind::Macro),
            other => panic!("{other:?}"),
        }
    }
    assert!(
        c.run_with("SELECT $__timeFilter(t), $__fromTime, $__toTime, $__from, $__to, $__dateFilter(d)", false, 1000)
            .is_ok()
    );
}

#[test]
fn macro_nesting_is_bounded() {
    let c = none();
    let deep = format!("SELECT {}t{}", "$__timeInterval(".repeat(40), ")".repeat(40));
    assert_eq!(c.invalid(&deep), SqlInvalidKind::Macro);
    let ok = format!("SELECT {}t{}", "$__timeInterval(".repeat(16), ")".repeat(16));
    assert!(c.run(&ok).is_ok());
    // A copy twice at each level cannot make a huge text: the limit stops it.
    let blow = format!("SELECT {}t{}", "$__timeFilter(".repeat(16), ")".repeat(16));
    assert!(matches!(c.run_with(&blow, true, 4096), Err(ExpandError::TooLarge { .. })));
}

#[test]
fn placeholders_in_the_argument_come_first() {
    let c = Case::new(vec![VariableDef::custom("col", &["ts", "t2"], false, false, None)]).set("col", false, &["ts"]);
    assert_eq!(
        c.ok("SELECT 1 WHERE $__timeFilter($col)"),
        "SELECT 1 WHERE ts >= toDateTime(1794211200) AND ts <= toDateTime(1794232800)"
    );
    assert_eq!(
        c.ok("SELECT 1 WHERE $__timeFilter(f('$col'))"),
        "SELECT 1 WHERE f('ts') >= toDateTime(1794211200) AND f('ts') <= toDateTime(1794232800)"
    );
}

#[test]
fn unknown_names_and_longest_run() {
    let c = Case::new(vec![
        VariableDef::custom("env", &["prod"], false, false, None),
        VariableDef::custom("env_name", &["N"], false, false, None),
    ])
    .set("env", false, &["prod"])
    .set("env_name", false, &["N"]);
    assert_eq!(
        c.ok("SELECT $env_name, ${env}_name, $other, ${other}, ${other:csv}, $1, $"),
        "SELECT N, prod_name, $other, ${other}, ${other:csv}, $1, $"
    );
    assert_eq!(c.ok("SELECT '$other ${other}' AS a"), "SELECT '$other ${other}' AS a");
    // [[name]] stays text.
    assert_eq!(c.ok("SELECT [[env]], '[[env]]'"), "SELECT [[env]], '[[env]]'");
}

#[test]
fn comments_are_not_read() {
    let c = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &["x"]);
    let sql = "SELECT 1 -- $t ${t:raw} ${\n/* $t /* ${ */ $t */ # $t\n, 2";
    assert_eq!(c.ok(sql), sql);
}

#[test]
fn inserted_text_is_never_read_again() {
    let c = Case::new(vec![
        VariableDef::custom("a", &["$__fromTime $b ${b}"], false, false, None),
        VariableDef::text("b", TextValueType::Text),
    ])
    .set("a", false, &["$__fromTime $b ${b}"])
    .set("b", false, &["$a"]);
    assert_eq!(c.ok("SELECT '$a', '$b'"), "SELECT '$__fromTime $b ${b}', '$a'");
    let c = Case::new(vec![VariableDef::query("q", true, false, None)]).set("q", false, &["$__fromTime", "${q}"]);
    assert_eq!(c.ok("SELECT $q"), "SELECT '$__fromTime','${q}'");
}

// ---- 7.8: the table, row by row ----

fn defs_for_contexts() -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::text("n", TextValueType::Number),
        VariableDef::query("q", false, true, None),
        VariableDef::query("qm", true, true, None),
        VariableDef::query("qa", true, true, Some("'all'")),
        VariableDef::custom("c", &["a", "b'c"], false, true, None),
        VariableDef::custom("cm", &["a", "b'c"], true, true, None),
        VariableDef::custom("ca", &["a", "b'c"], true, true, Some("x,y")),
        VariableDef::custom("cn", &["1", "2"], true, false, None),
    ]
}

fn ctx_case() -> Case {
    Case::new(defs_for_contexts())
        .set("t", false, &["o'r\\"])
        .set("n", false, &["-5"])
        .set("q", false, &["it's"])
        .set("qm", false, &["a", "b'c", "d\\"])
        .set("qa", true, &[])
        .set("c", false, &["b'c"])
        .set("cm", false, &["a", "b'c"])
        .set("ca", true, &[])
        .set("cn", false, &["1", "2"])
}

#[test]
fn string_context() {
    let c = ctx_case();
    // Single value or all_value: none, raw, csv.
    for f in ["", ":raw", ":csv"] {
        assert_eq!(c.ok(&format!("SELECT 'x${{t{f}}}y'")), "SELECT 'xo\\'r\\\\y'");
        assert_eq!(c.ok(&format!("SELECT 'x${{q{f}}}y'")), "SELECT 'xit\\'sy'");
        assert_eq!(c.ok(&format!("SELECT '${{c{f}}}'")), "SELECT 'b\\'c'");
        assert_eq!(c.ok(&format!("SELECT '${{ca{f}}}'")), "SELECT 'x,y'");
        assert_eq!(c.ok(&format!("SELECT '${{qa{f}}}'")), "SELECT '\\'all\\''");
    }
    // A list: csv only.
    assert_eq!(c.ok("SELECT '${qm:csv}'"), "SELECT 'a,b\\'c,d\\\\'");
    assert_eq!(c.ok("SELECT '${cm:csv}'"), "SELECT 'a,b\\'c'");
    assert_eq!(c.invalid("SELECT '$qm'"), SqlInvalidKind::Context);
    assert_eq!(c.invalid("SELECT '${qm:raw}'"), SqlInvalidKind::Context);
    assert_eq!(c.invalid("SELECT '$cm'"), SqlInvalidKind::Context);
    // singlequote and sqlstring: always an error.
    for v in ["t", "q", "c", "cm", "ca"] {
        for f in ["singlequote", "sqlstring"] {
            assert_eq!(c.invalid(&format!("SELECT '${{{v}:{f}}}'")), SqlInvalidKind::Context, "{v}:{f}");
        }
    }
    // The content of a string with the quote mark inside it, and two placeholders.
    assert_eq!(c.ok("SELECT 'a$t$c'"), "SELECT 'ao\\'r\\\\b\\'c'");
    // The empty query selection gives the empty content.
    let e = Case::new(defs_for_contexts()).set("q", false, &[]);
    assert_eq!(e.ok("SELECT '[$q]'"), "SELECT '[]'");
    // A list with 0 values and csv.
    let e = Case::new(defs_for_contexts()).set("qm", false, &[]);
    assert_eq!(e.ok("SELECT '[${qm:csv}]'"), "SELECT '[]'");
    // A `${` that does not match stays text in a string.
    assert_eq!(c.ok("SELECT '${ ${x:bad} ${t'"), "SELECT '${ ${x:bad} ${t'");
}

#[test]
fn quoted_name_context() {
    let c = ctx_case();
    for f in ["", ":raw", ":csv"] {
        assert_eq!(c.ok(&format!("SELECT `${{c{f}}}`, \"${{c{f}}}\"")), "SELECT `b'c`, \"b'c\"");
        assert_eq!(c.ok(&format!("SELECT `${{ca{f}}}`")), "SELECT `x,y`");
    }
    let c2 = Case::new(vec![VariableDef::custom("c", &["a`b"], false, false, None)]).set("c", false, &["a`b"]);
    assert_eq!(c2.ok("SELECT `x$c`"), "SELECT `xa\\`b`");
    // A `text` or `query` variable, or a list: an error.
    for sql in [
        "SELECT `$t`",
        "SELECT `$q`",
        "SELECT \"$qa\"",
        "SELECT `$cm`",
        "SELECT `${cm:csv}`",
        "SELECT `${cn:csv}`",
        "SELECT `${c:singlequote}`",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Context, "{sql}");
    }
}

#[test]
fn heredoc_context() {
    let c = ctx_case();
    assert_eq!(c.invalid("SELECT $$ $t $$"), SqlInvalidKind::Context);
    assert_eq!(c.invalid("SELECT $tag$ a ${c} b $tag$"), SqlInvalidKind::Context);
    // A name that is no variable stays text. The tag is not a placeholder.
    assert_eq!(c.ok("SELECT $x$ $y $x$"), "SELECT $x$ $y $x$");
    assert_eq!(c.ok("SELECT $t$ $y $t$"), "SELECT $t$ $y $t$");
}

#[test]
fn code_context_numbers() {
    let c = ctx_case();
    assert_eq!(c.ok("SELECT 1 + $n"), "SELECT 1 + -5");
    assert_eq!(c.ok("SELECT 1 + ${n:raw}, ${n:csv}"), "SELECT 1 + -5, -5");
    // A free value that matches the number grammar is permitted, also from a `text` variable.
    let t = Case::new(defs_for_contexts()).set("t", false, &["12.50"]);
    assert_eq!(t.ok("SELECT $t"), "SELECT 12.50");
    // A free value that does not match: refused.
    assert_eq!(c.refused("SELECT $t"), "t");
    assert_eq!(c.refused("SELECT $q"), "q");
    assert_eq!(c.refused("SELECT ${t:raw}"), "t");
    assert_eq!(c.refused("SELECT ${t:csv}"), "t");
    for bad in ["", "1e3", "1 OR 1=1", "0x10", ".5", "5.", "+1", "1,2"] {
        let t = Case::new(defs_for_contexts()).set("t", false, &[bad]);
        assert_eq!(t.refused("SELECT $t"), "t", "{bad:?}");
    }
    // An author value as it is.
    assert_eq!(c.invalid("SELECT $c"), SqlInvalidKind::Lexer);
    let a = Case::new(defs_for_contexts()).set("c", false, &["a"]);
    assert_eq!(a.ok("SELECT x FROM t WHERE y = $c"), "SELECT x FROM t WHERE y = a");
    // all_value as it is, with any format.
    assert_eq!(c.ok("SELECT ${ca}, ${ca:raw}, ${ca:singlequote}"), "SELECT x,y, x,y, x,y");
    assert_eq!(c.ok("SELECT $qa"), "SELECT 'all'");
}

#[test]
fn code_context_lists_and_literals() {
    let c = ctx_case();
    // none and singlequote: string literals joined by `,`.
    assert_eq!(c.ok("SELECT 1 WHERE x IN ($qm)"), "SELECT 1 WHERE x IN ('a','b\\'c','d\\\\')");
    assert_eq!(c.ok("SELECT 1 WHERE x IN (${qm:singlequote})"), "SELECT 1 WHERE x IN ('a','b\\'c','d\\\\')");
    // sqlstring: the quote mark doubled and the backslash doubled.
    assert_eq!(c.ok("SELECT 1 WHERE x IN (${qm:sqlstring})"), "SELECT 1 WHERE x IN ('a','b''c','d\\\\')");
    // A single value with singlequote or sqlstring.
    assert_eq!(c.ok("SELECT ${t:singlequote}, ${t:sqlstring}"), "SELECT 'o\\'r\\\\', 'o''r\\\\'");
    assert_eq!(c.ok("SELECT ${c:singlequote}"), "SELECT 'b\\'c'");
    assert_eq!(c.ok("SELECT ${n:singlequote}"), "SELECT '-5'");
    // A list of author values.
    assert_eq!(c.ok("SELECT $cm, ${cm:sqlstring}"), "SELECT 'a','b\\'c', 'a','b''c'");
    assert_eq!(c.ok("SELECT ${cm:raw}, ${cm:csv}"), "SELECT a,b'c, a,b'c");
    // The "All" of a custom variable without all_value: all options in document order.
    let all = Case::new(defs_for_contexts()).set("cm", true, &[]);
    assert_eq!(all.ok("SELECT $cm"), "SELECT 'a','b\\'c'");
    // The "All" of a query variable without all_value: the values that the page sends.
    let q = Case::new(defs_for_contexts()).set("qm", true, &["x", "y"]);
    assert_eq!(q.ok("SELECT ${qm}"), "SELECT 'x','y'");
    // A list with free values and raw or csv: each value must be a number.
    let n = Case::new(defs_for_contexts()).set("qm", false, &["1", "-2.5", "30"]);
    assert_eq!(n.ok("SELECT ${qm:csv}, ${qm:raw}"), "SELECT 1,-2.5,30, 1,-2.5,30");
    let m = Case::new(defs_for_contexts()).set("qm", false, &["1", "x"]);
    assert_eq!(m.refused("SELECT ${qm:csv}"), "qm");
    assert_eq!(m.refused("SELECT ${qm:raw}"), "qm");
    // The empty lists.
    let e = Case::new(defs_for_contexts()).set("qm", false, &[]).set("q", false, &[]).set("cm", false, &[]);
    for f in ["", ":singlequote", ":sqlstring"] {
        assert_eq!(e.ok(&format!("SELECT x IN (${{qm{f}}})")), "SELECT x IN ('')");
        assert_eq!(e.ok(&format!("SELECT x IN (${{q{f}}})")), "SELECT x IN ('')");
        assert_eq!(e.ok(&format!("SELECT x IN (${{cm{f}}})")), "SELECT x IN ('')");
    }
    for f in [":raw", ":csv"] {
        assert_eq!(e.ok(&format!("SELECT x IN (${{qm{f}}})")), "SELECT x IN ()");
        assert_eq!(e.ok(&format!("SELECT x IN (${{q{f}}})")), "SELECT x IN ()");
    }
}

#[test]
fn code_faults_in_braces() {
    let c = ctx_case();
    for sql in [
        "SELECT ${",
        "SELECT ${t",
        "SELECT ${}",
        "SELECT ${t:}",
        "SELECT ${t:date}",
        "SELECT ${t:none}",
        "SELECT ${t:CSV}",
        "SELECT ${t:csv",
        "SELECT ${ t}",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Placeholder, "{sql}");
    }
    // Unknown names with a bad format are also errors in code.
    assert_eq!(c.invalid("SELECT ${zzz:date}"), SqlInvalidKind::Placeholder);
}

// ---- P: the refusals ----

#[test]
fn property_p_refuses_a_structure_change() {
    // The example of 7.11.
    let c = Case::new(vec![VariableDef::text("min", TextValueType::Number)]).set("min", false, &["-5"]);
    assert_eq!(c.refused("SELECT 1 WHERE amount > 0-$min AND tenant = 'a'"), "min");
    assert_eq!(
        c.ok("SELECT 1 WHERE amount > 0 - $min AND tenant = 'a'"),
        "SELECT 1 WHERE amount > 0 - -5 AND tenant = 'a'"
    );
    // A number that joins its neighbours.
    let n = Case::new(vec![VariableDef::text("n", TextValueType::Number)]).set("n", false, &["5"]);
    for sql in ["SELECT x$n", "SELECT $n.5", "SELECT $n.x", "SELECT 1e$n", "SELECT 0x$n", "SELECT 1.$n", "SELECT $nabc"]
    {
        // `$nabc` names another variable, so it stays text; the others change the tokens.
        let r = n.run(sql);
        if sql == "SELECT $nabc" {
            assert_eq!(r.unwrap(), sql);
        } else {
            assert!(matches!(r, Err(ExpandError::ValueRefused(_))), "{sql}: {r:?}");
        }
    }
    assert_eq!(n.ok("SELECT 1+$n, 2-$n"), "SELECT 1+5, 2-5");
}

#[test]
fn property_p_in_strings() {
    let t = Case::new(vec![VariableDef::text("t", TextValueType::Text)]);
    for v in [
        "'",
        "\\",
        "\\'",
        "'; DROP TABLE x; --",
        "''",
        "\\\\'",
        "/*",
        "--",
        "$$",
        "`",
        "\"",
        "$x$",
        "\u{2028}",
        "\u{a0}",
        "a\u{1f600}",
    ] {
        let c = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &[v]);
        let out = c.ok("SELECT 'a$t' AS x, 1 -- c");
        let tokens = hune_core::sql::lex(&out).unwrap();
        assert!(tokens.iter().filter(|k| k.kind == hune_core::sql::TokenKind::String).count() == 1, "{v:?} {out:?}");
    }
    // A backslash before the placeholder joins with the escape of the value: P refuses.
    // The output has a string that never closes, so the final lexer refuses it (7.11, step 5).
    let c = t.set("t", false, &["'"]);
    assert_eq!(c.invalid("SELECT 'a\\$t'"), SqlInvalidKind::Lexer);
    let c = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &["x"]);
    assert_eq!(c.ok("SELECT 'a\\$t'"), "SELECT 'a\\x'");
}

#[test]
fn control_characters_are_refused() {
    let defs = vec![VariableDef::text("t", TextValueType::Text)];
    for v in ["a\0b", "a\nb", "a\rb", "a\u{7f}b", "\u{1}"] {
        let mut r = RequestValues::new();
        r.insert("t", false, &[v]);
        assert!(matches!(resolve_values(&defs, &r), Err(ValuesError::Refused(_))), "{v:?}");
    }
}

#[test]
fn author_text_is_not_checked_by_p() {
    let c = Case::new(vec![VariableDef::custom("a", &["1 OR 1=1", "x'y"], false, false, None)]).set(
        "a",
        false,
        &["1 OR 1=1"],
    );
    assert_eq!(c.ok("SELECT 1 WHERE $a"), "SELECT 1 WHERE 1 OR 1=1");
}

// ---- 7.2 steps 9 to 12 ----

#[test]
fn lexer_errors() {
    let c = none();
    for sql in ["SELECT 'a", "SELECT /* a", "SELECT `a", "SELECT $$a", "SELECT #x"] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Lexer, "{sql}");
    }
    match c.run("SELECT 1\nFROM 'abc") {
        Err(ExpandError::Invalid(i)) => assert_eq!((i.line, i.column), (Some(2), Some(6))),
        other => panic!("{other:?}"),
    }
}

#[test]
fn statement_check_on_the_final_text() {
    let c = none();
    for sql in [
        "SELECT 1; SELECT 2",
        "SELECT 1 FORMAT JSON",
        "SHOW TABLES",
        "SELECT 1 INTO OUTFILE 'x'",
        "-- c",
        "",
        "WITH 1 AS x INSERT INTO t SELECT x",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Statement, "{sql:?}");
    }
    assert_eq!(c.ok("WITH x AS (SELECT 1) SELECT * FROM x;"), "WITH x AS (SELECT 1) SELECT * FROM x;");
    // The statement check reads the final text: an all_value can complete a statement.
    let a = Case::new(vec![VariableDef::custom("v", &["a"], false, true, Some("SELECT 1"))]).set("v", true, &[]);
    assert_eq!(a.ok("$v"), "SELECT 1");
}

#[test]
fn size_limit() {
    let c = none();
    let sql = "SELECT 1";
    assert_eq!(c.run_with(sql, true, 8).unwrap(), sql);
    assert_eq!(c.run_with(sql, true, 7), Err(ExpandError::TooLarge { size: 8, limit: 7 }));
    // The expansion makes the text larger than the stored text.
    let q = Case::new(vec![VariableDef::query("q", true, false, None)]).set("q", false, &["x"; 100].map(|s| s));
    let r = q.run_with("SELECT $q", true, 100);
    match r {
        Err(ExpandError::TooLarge { size, limit }) => assert!(size > 100 && limit == 100),
        other => panic!("{other:?}"),
    }
    // A hostile request cannot use much memory: 10,000 values of 1,024 bytes, many placeholders.
    let big = "a'".repeat(512);
    let mut r = RequestValues::new();
    let vals: Vec<&str> = (0..10_000).map(|_| big.as_str()).collect();
    r.insert("q", false, &vals);
    let defs = vec![VariableDef::query("q", true, false, None)];
    let resolved = resolve_values(&defs, &r).unwrap();
    let sql = format!("SELECT {}", "$q,".repeat(80_000));
    let range = check_time_range(0, 1000).unwrap();
    let out = expand(&ExpandRequest { sql: &sql, values: &resolved, range, interval: None, max_query_size: 1_000_000 });
    assert!(matches!(out, Err(ExpandError::TooLarge { .. })));
}

#[test]
fn missing_variable_member() {
    let defs = vec![VariableDef::text("t", TextValueType::Text), VariableDef::text("u", TextValueType::Text)];
    let mut r = RequestValues::new();
    r.insert("t", false, &["1"]);
    let resolved = resolve_values(&defs, &r).unwrap();
    let range = check_time_range(0, 1000).unwrap();
    let ex = |sql: &str| expand(&ExpandRequest { sql, values: &resolved, range, interval: None, max_query_size: 1000 });
    assert_eq!(ex("SELECT $t"), Ok("SELECT 1".to_owned()));
    assert_eq!(ex("SELECT $u"), Err(ExpandError::MissingVariable("u".to_owned())));
    assert_eq!(ex("SELECT '$u'"), Err(ExpandError::MissingVariable("u".to_owned())));
    // A variable that the SQL does not name needs no member. A comment names nothing.
    assert_eq!(ex("SELECT $t -- $u"), Ok("SELECT 1 -- $u".to_owned()));
    assert_eq!(ExpandError::MissingVariable("u".into()).api_code(), "bad_request");
}

#[test]
fn api_codes() {
    let c = none();
    assert_eq!(c.run("SHOW").unwrap_err().api_code(), "sql_invalid");
    assert_eq!(c.run_with("SELECT 1", true, 1).unwrap_err().api_code(), "sql_too_large");
    let t = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &["x"]);
    assert_eq!(t.run("SELECT $t").unwrap_err().api_code(), "variable_value_refused");
}

#[test]
fn an_empty_selection_joins_no_tokens() {
    // An empty list gives the empty text with raw and csv. The empty text MUST NOT merge its
    // neighbours: `0-` and `-` would make a comment. Section 7.11 says an empty range has no check;
    // the engine checks that a token starts at the place, which is stricter.
    let c = Case::new(vec![
        VariableDef::query("m", true, false, None),
        VariableDef::custom("cm", &["a"], true, false, None),
    ])
    .set("m", false, &[])
    .set("cm", false, &[]);
    assert_eq!(c.refused("SELECT 0-${m:csv}- 1"), "m");
    assert_eq!(c.refused("SELECT 1${m:raw}e5"), "m");
    assert_eq!(c.refused("SELECT 0-${cm:csv}- 1"), "cm");
    assert_eq!(c.ok("SELECT x IN (${m:csv}) AND y IN (${cm:raw})"), "SELECT x IN () AND y IN ()");
    assert_eq!(c.ok("SELECT 1 ${m:csv} 2"), "SELECT 1  2");
    // In a string, an empty content must not move the closing quote mark.
    let q = Case::new(vec![
        VariableDef::query("q", false, false, None),
        VariableDef::custom("cm", &["a"], true, false, None),
    ])
    .set("q", false, &[])
    .set("cm", false, &[]);
    assert_eq!(q.ok("SELECT 'a${q}b'"), "SELECT 'ab'");
    assert!(q.run("SELECT 'a\\${q}' AND y = 'b'").is_err());
    assert!(q.run("SELECT 'a\\${cm:csv}' AND y = 'b' AND z = ''").is_err());
}
