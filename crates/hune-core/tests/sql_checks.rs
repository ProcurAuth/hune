// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the checks at save (7.18): the nine faults and the three warnings.

use hune_core::error::ErrorCode;
use hune_core::sql::{
    SaveCheckResult, SqlUse, TextValueType, VariableDef, WarningKind, check_source, join_lines, save_checks,
};

fn vars() -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::query("q", false, false, None),
        VariableDef::query("qm", true, true, None),
        VariableDef::custom("c", &["a", "b"], false, false, None),
        VariableDef::custom("cm", &["a", "b"], true, false, None),
        VariableDef::custom("ci", &["a", "b"], false, true, None),
        VariableDef::custom("ca", &["a", "b"], false, true, Some("x")),
    ]
}

fn check(sql: &str) -> SaveCheckResult {
    save_checks(sql, SqlUse::Panel, &vars())
}

fn codes(r: &SaveCheckResult) -> Vec<ErrorCode> {
    r.faults.iter().map(|f| f.code).collect()
}

fn clean(sql: &str) {
    let r = check(sql);
    assert!(r.faults.is_empty(), "{sql:?}: {:?}", r.faults);
}

fn fault(sql: &str, code: ErrorCode) {
    let r = check(sql);
    assert_eq!(codes(&r), vec![code], "{sql:?}: {:?}", r.faults);
}

#[test]
fn a_good_text_has_no_fault() {
    clean(
        "SELECT $__timeInterval(ts) AS t, count() FROM x WHERE $__timeFilter(ts) AND a IN ($qm) AND b = '$t' AND c = '${c}' GROUP BY t",
    );
    clean("SELECT `${c}` FROM t WHERE x = ${t} AND y = '${qm:csv}' AND z IN (${cm:sqlstring})");
    clean("SELECT $__fromTime, $__toTime, $__interval_s -- $__bad $t ${\n/* $__bad */");
    clean("WITH x AS (SELECT 1) SELECT * FROM x;");
}

#[test]
fn item_1_lexer_errors() {
    for sql in ["SELECT 'a", "SELECT /* a", "SELECT `a", "SELECT $$a", "SELECT 1 #x"] {
        let r = check(sql);
        assert!(r.faults.iter().any(|f| f.code == ErrorCode::BadFormat), "{sql}");
    }
    let r = check("SELECT 1\nFROM 'abc");
    let f = &r.faults[0];
    assert_eq!((f.code, f.line_index, f.column), (ErrorCode::BadFormat, 1, 6));
}

#[test]
fn item_2_statement_check() {
    for sql in [
        "SELECT 1; SELECT 2",
        "SELECT 1 FORMAT JSON",
        "SHOW TABLES",
        "SELECT 1 INTO OUTFILE 'x'",
        "-- c",
        "WITH 1 AS x INSERT INTO t SELECT x",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
}

#[test]
fn item_3_unknown_macro() {
    for sql in [
        "SELECT $__",
        "SELECT $__range",
        "SELECT $__range_s",
        "SELECT $__range_ms",
        "SELECT $__timeFilter_s(x)",
        "SELECT $__conditionalTest(a, $q)",
        "SELECT $__timezone",
        "SELECT $__interval",
        "SELECT $__nothing(1)",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    // The macros and the built-in variables of 7.10 are known.
    clean(
        "SELECT $__from, $__to, $__interval_ms, $__fromTime_ms, $__toTime_ms, $__timeFilter_ms(x), $__timeInterval_ms(x), \
         $__dateFilter(d), $__dateTimeFilter(d, t), $__conditionalAll(a IN ($qm), $qm)",
    );
    // In a string, a quoted name, a comment: no fault.
    clean("SELECT '$__from', \"$__x\" -- $__y");
}

#[test]
fn item_4_conditional_all_second_argument() {
    for sql in [
        "SELECT $__conditionalAll(a = 1, 1)",
        "SELECT $__conditionalAll(a = 1, $t)",
        "SELECT $__conditionalAll(a = 1, ${t})",
        "SELECT $__conditionalAll(a = 1, $nobody)",
        "SELECT $__conditionalAll(a = 1, ${qm:csv})",
        "SELECT $__conditionalAll(a = 1, $qm $qm)",
        "SELECT $__conditionalAll(a = 1, $qm + 1)",
        "SELECT $__conditionalAll(a = 1, '${qm:csv}')",
        "SELECT $__conditionalAll(a = 1, $__from)",
        "SELECT $__conditionalAll(a = 1, $qm /* c */)",
        "SELECT $__conditionalAll(a = 1)",
        "SELECT $__conditionalAll(a = 1, $qm, $qm)",
        "SELECT $__conditionalAll($qm)",
        "SELECT $__conditionalAll",
        "SELECT $__dateTimeFilter(a)",
        "SELECT $__dateTimeFilter(a, b, c)",
        "SELECT $__dateFilter(a, b)",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    clean("SELECT $__conditionalAll(a IN ($qm), $qm), $__conditionalAll(b = '$c', ${c}), $__conditionalAll(x, $ci)");
    // A blank around the second argument is allowed.
    clean("SELECT $__conditionalAll( a , \n $qm \n )");
}

/// D08: `$__dt` is the second name of `$__dateTimeFilter` (7.10). The checks at save are the same, and a message
/// names the name of the text.
#[test]
fn item_4_the_alias_dt() {
    clean("SELECT count() FROM t WHERE $__dt(toDate(ts), ts) AND $__dt( d , f(a, b) )");
    for sql in ["SELECT $__dt", "SELECT $__dt()", "SELECT $__dt(a)", "SELECT $__dt(a, b, c)", "SELECT $__dt(a, )"] {
        fault(sql, ErrorCode::BadFormat);
    }
    let r = save_checks("SELECT $__dt(a)", SqlUse::Panel, &vars());
    assert!(r.faults[0].message.starts_with("$__dt needs exactly two arguments"), "{:?}", r.faults);
    let r = save_checks("SELECT ${__dt:csv}(a, b)", SqlUse::Panel, &vars());
    assert_eq!(r.faults[0].message, "$__dt takes no format");
    // It needs no interval: a variable query and an annotation can use it.
    assert!(save_checks("SELECT $__dt(d, t)", SqlUse::Variable, &vars()).faults.is_empty());
    // The name is exact: `$__DT` and `$__dts` are not macros.
    fault("SELECT $__DT(a, b)", ErrorCode::BadFormat);
    fault("SELECT $__dts(a, b)", ErrorCode::BadFormat);
    clean("SELECT '$__dt(a)' -- $__dt");
}

#[test]
fn item_4_macro_arguments() {
    for sql in [
        "SELECT $__timeFilter",
        "SELECT $__timeFilter()",
        "SELECT $__timeFilter( )",
        "SELECT $__timeFilter(a, b)",
        "SELECT $__timeFilter(a",
        "SELECT $__timeInterval",
        "SELECT $__fromTime()",
        "SELECT $__toTime()",
        "SELECT $__interval_s()",
        "SELECT $__from()",
        "SELECT $__to()",
        "SELECT $__interval_ms()",
        "SELECT $__fromTime_ms()",
        "SELECT $__toTime_ms()",
        "SELECT $__timeFilter_ms",
        "SELECT $__timeInterval_ms()",
        "SELECT $__dateFilter",
        "SELECT $__dateFilter()",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    clean("SELECT $__fromTime (1), $__timeFilter(f(a, b))");
}

#[test]
fn item_5_interval_macros_in_a_variable() {
    for sql in
        ["SELECT $__interval_s", "SELECT $__timeInterval(t)", "SELECT $__interval_ms", "SELECT $__timeInterval_ms(t)"]
    {
        let r = save_checks(sql, SqlUse::Variable, &vars());
        assert_eq!(codes(&r), vec![ErrorCode::NotPermitted], "{sql}");
        assert!(save_checks(sql, SqlUse::Panel, &vars()).faults.is_empty());
    }
    assert!(
        save_checks(
            "SELECT $__timeFilter(t), $__fromTime, $__toTime, $__from, $__to, $__timeFilter_ms(t), $__fromTime_ms, $__dateFilter(d)",
            SqlUse::Variable,
            &vars()
        )
        .faults
        .is_empty()
    );
}

#[test]
fn item_6_placeholder_in_a_heredoc() {
    fault("SELECT $$ $t $$", ErrorCode::NotPermitted);
    fault("SELECT $tag$ ${c} $tag$", ErrorCode::NotPermitted);
    clean("SELECT $$ plain $$");
}

#[test]
fn item_7_braces_and_formats_in_code() {
    for sql in [
        "SELECT ${",
        "SELECT ${t",
        "SELECT ${}",
        "SELECT ${t:date}",
        "SELECT ${t:}",
        "SELECT ${zzz:bad}",
        "SELECT ${__fromTime:csv}",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    // In a string, a `${` that does not match stays text.
    clean("SELECT '${', '${t:date}', '${'");
}

#[test]
fn item_7_formats_that_are_valid_only_inside_a_string() {
    let names = [
        "json",
        "pipe",
        "regex",
        "glob",
        "lucene",
        "percentencode",
        "queryparam",
        "distributed",
        "doublequote",
        "html",
        "text",
    ];
    for f in names {
        // In a string: no fault, for a text, a query and a custom variable, single or list.
        for v in ["t", "q", "qm", "c", "cm", "ci", "ca"] {
            clean(&format!("SELECT '${{{v}:{f}}}', 'a ${{{v}:{f}}} b'"));
        }
        // In code and in a quoted name: a fault, whatever the variable.
        for v in ["t", "q", "qm", "c", "cm", "ci", "ca"] {
            fault(&format!("SELECT ${{{v}:{f}}}"), ErrorCode::NotPermitted);
            fault(&format!("SELECT x IN (${{{v}:{f}}})"), ErrorCode::NotPermitted);
            fault(&format!("SELECT `${{{v}:{f}}}`"), ErrorCode::NotPermitted);
            fault(&format!("SELECT \"${{{v}:{f}}}\""), ErrorCode::NotPermitted);
            fault(&format!("SELECT $$ ${{{v}:{f}}} $$"), ErrorCode::NotPermitted);
        }
        // In a macro argument: code.
        fault(&format!("SELECT $__timeFilter(${{c:{f}}})"), ErrorCode::NotPermitted);
        fault(&format!("SELECT $__conditionalAll(a = ${{c:{f}}}, $c)"), ErrorCode::NotPermitted);
        // A comment holds no placeholder.
        clean(&format!("SELECT 1 -- ${{t:{f}}}\n/* ${{t:{f}}} */"));
    }
    let r = check("SELECT ${t:json}");
    assert!(r.faults[0].message.contains("only inside a string"), "{:?}", r.faults);
    // The format `date` and the other unknown formats of Grafana stay a fault everywhere in code.
    for f in ["date", "join", "uriencode", "customqueryparam"] {
        fault(&format!("SELECT ${{t:{f}}}"), ErrorCode::BadFormat);
    }
}

#[test]
fn item_8_contexts() {
    // String: a list with the format none or raw, singlequote, sqlstring.
    for sql in [
        "SELECT '$qm'",
        "SELECT '${qm:raw}'",
        "SELECT '$cm'",
        "SELECT '$ci'",
        "SELECT '${t:singlequote}'",
        "SELECT '${c:sqlstring}'",
    ] {
        fault(sql, ErrorCode::NotPermitted);
    }
    clean("SELECT '${qm:csv}', '${ci:csv}', '$ca', '${ca:raw}', '$c', '$q', '$t'");
    // Quoted name: only a `custom` variable that is not a list.
    for sql in
        ["SELECT `$t`", "SELECT `$q`", "SELECT `$qm`", "SELECT `$cm`", "SELECT `$ci`", "SELECT \"${c:singlequote}\""]
    {
        fault(sql, ErrorCode::NotPermitted);
    }
    clean("SELECT `$c`, \"$ca\", `x${c:csv}`");
}

#[test]
fn item_9_source() {
    assert!(check_source("main", &["main", "logs"]).is_ok());
    let e = check_source("zzz", &["main"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::BadReference);
    assert!(e.message.contains("zzz"));
    assert!(check_source("a", &[]).is_err());
}

#[test]
fn warnings() {
    let r = check("SELECT $nope, ${nope2}, '$nope3' FROM t FINAL WHERE [[t]] = 1 AND [[nobody]] = 2 SETTINGS x = 1");
    let kinds: Vec<WarningKind> = r.warnings.iter().map(|w| w.kind).collect();
    assert_eq!(
        kinds,
        vec![
            WarningKind::UnknownVariable,
            WarningKind::UnknownVariable,
            WarningKind::UnknownVariable,
            WarningKind::FinalOrSettings,
            WarningKind::DoubleBracket,
            WarningKind::FinalOrSettings
        ]
    );
    assert!(r.faults.is_empty(), "{:?}", r.faults);
    // FINAL at any depth, SETTINGS at depth 0 only.
    let r = check("SELECT * FROM (SELECT 1 FROM t FINAL SETTINGS x = 1)");
    assert_eq!(r.warnings.iter().filter(|w| w.kind == WarningKind::FinalOrSettings).count(), 1);
    // Names that start with `__` in a string give no warning. Comments give none.
    assert!(check("SELECT '$__x', 1 -- $nope [[t]] FINAL").warnings.is_empty());
    assert!(check("SELECT $t").warnings.is_empty());
}

#[test]
fn items_make_validation_errors() {
    let sql = join_lines(&["SELECT 1", "FROM x", "WHERE a = 1 AND $__timeFilter"]);
    let r = check(&sql);
    assert_eq!(r.faults.len(), 1);
    let e = r.faults[0].to_validation_error("/sections/0/panels/2/data/query/sql");
    assert_eq!(e.pointer, "/sections/0/panels/2/data/query/sql/2");
    assert_eq!(e.code, ErrorCode::BadFormat);
    assert_eq!((e.line, e.column), (None, None));
    assert!(e.message.starts_with("column 17: "), "{}", e.message);
    let w = check("SELECT 1\nFROM x FINAL");
    let (p, m) = w.warnings[0].to_pointer_message("/sql");
    assert_eq!(p, "/sql/1");
    assert!(m.starts_with("column 8: "));
}

#[test]
fn several_faults_come_in_text_order() {
    let r = check("SELECT $__bad,\n  '$qm', $__timeFilter");
    let at: Vec<(usize, usize)> = r.faults.iter().map(|f| (f.line_index, f.column)).collect();
    assert_eq!(at, vec![(0, 8), (1, 4), (1, 10)]);
    assert_eq!(codes(&r), vec![ErrorCode::BadFormat, ErrorCode::NotPermitted, ErrorCode::BadFormat]);
}

#[test]
fn utf8_columns_count_characters() {
    let r = check("SELECT '\u{e9}\u{1f600}', $__bad");
    assert_eq!(r.faults[0].column, 14);
}

#[test]
fn warning_4_macro_in_a_string() {
    let r = check("SELECT '$__interval', \"$__from\", '$__timeFilter(x)', '$__range_s' -- $__to\n/* $__interval */");
    let kinds: Vec<WarningKind> = r.warnings.iter().map(|w| w.kind).collect();
    assert_eq!(kinds, vec![WarningKind::MacroInLiteral; 4]);
    assert!(r.faults.is_empty());
    assert!(r.warnings[0].message.contains("$__interval stays text"), "{:?}", r.warnings);
    // An unknown name, a heredoc and code give no such warning.
    let r = check("SELECT '$__nothing', $$ $__from $$, $__from");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn warning_5_unknown_format_in_a_string() {
    let r = check("SELECT '${t:date}', '${qm:join}', `${c:uriencode}`, '${nobody:date}', '${t:json}', '${'");
    let kinds: Vec<WarningKind> = r.warnings.iter().map(|w| w.kind).collect();
    assert_eq!(kinds, vec![WarningKind::UnknownFormatInString; 3], "{:?}", r.warnings);
    assert!(r.warnings[0].message.contains("date of ${t}"), "{:?}", r.warnings);
    assert!(r.faults.is_empty() || r.faults.iter().all(|f| f.code == ErrorCode::NotPermitted), "{:?}", r.faults);
}
