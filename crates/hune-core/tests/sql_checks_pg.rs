//! Tests of the checks at save for the dialect `postgres` (7.26): the changes of the items and of the
//! warnings, and the agreement of the checks with the expansion.

use hune_core::error::ErrorCode;
use hune_core::sql::{
    Dialect, ExpandError, ExpandRequest, RequestValues, SaveCheckResult, SqlInvalidKind, SqlUse, TextValueType,
    VariableDef, WarningKind, check_time_range, compute_interval, expand_for, resolve_values, save_checks,
    save_checks_for,
};

fn vars() -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::query("q", false, false, None),
        VariableDef::query("qm", true, true, None),
        VariableDef::custom("c", &["a", "b"], false, false, None),
        VariableDef::custom("cm", &["a", "b"], true, false, None),
    ]
}

fn check_use(sql: &str, usage: SqlUse) -> SaveCheckResult {
    save_checks_for(Dialect::Postgres, sql, usage, &vars())
}

fn check(sql: &str) -> SaveCheckResult {
    check_use(sql, SqlUse::Panel)
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
        "SELECT $__timeGroupAlias(ts, $__interval), count(*) FROM x WHERE $__timeFilter(ts) AND a IN ($qm) \
         AND b = '$t' AND c = '${c}' GROUP BY 1",
    );
    clean("SELECT \"${c}\" FROM t WHERE x = ${t} AND y = '${qm:csv}' AND z = E'${t}' AND w ~ '${t:regex}'");
    clean("SELECT $__timeFrom, $__timeTo(), $__unixEpochFrom(), $__interval_s -- $__bad $t ${\n/* $__bad /* $t */ */");
    clean("SELECT $__timeGroup(ts, '5m'), $__unixEpochGroupAlias(n, '$q'), $__timeGroup(ts, '$__interval')");
    clean("WITH x AS (SELECT 1) SELECT * FROM x;");
    clean("SELECT 1 # 2");
}

#[test]
fn clickhouse_text_checks_stay_as_they_were() {
    // The old entry point is the dialect `clickhouse`.
    let r = save_checks("SELECT $__timeInterval(ts) FROM t", SqlUse::Panel, &vars());
    assert!(r.faults.is_empty());
    let r = save_checks_for(Dialect::ClickHouse, "SELECT $__timeInterval(ts) FROM t", SqlUse::Panel, &vars());
    assert!(r.faults.is_empty());
    // The same text is a fault for `postgres`.
    fault("SELECT $__timeInterval(ts) FROM t", ErrorCode::BadFormat);
}

#[test]
fn item_1_lexer_errors() {
    for sql in
        ["SELECT 'a", "SELECT E'a\\'", "SELECT /* a /* b */", "SELECT \"a", "SELECT $$a", "SELECT U&\"a", "SELECT B'1"]
    {
        let r = check(sql);
        assert!(r.faults.iter().any(|f| f.code == ErrorCode::BadFormat), "{sql}: {:?}", r.faults);
    }
    // A backquote and `\` are not quote marks.
    clean("SELECT 'a\\' || 'b'");
    let r = check("SELECT 1\nFROM 'abc");
    let f = &r.faults[0];
    assert_eq!((f.code, f.line_index, f.column), (ErrorCode::BadFormat, 1, 6));
}

#[test]
fn item_2_statement_check_of_7_25() {
    for sql in [
        "SHOW server_version",
        "SELECT 1; SELECT 2",
        "SELECT * INTO t2 FROM t",
        "SELECT * FROM t FOR UPDATE",
        "SELECT pg_sleep(1)",
        "SELECT pg_catalog.\"pg_sleep\" (1)",
        "SELECT $1",
        "SELECT U&\"d\\0061t\\0061\"",
        "SELECT set_config('a', 'b', false)",
        "-- only a comment",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    clean("(SELECT 1) UNION ALL (SELECT 2)");
    clean("TABLE metrics.node_load");
    clean("VALUES (1), (2)");
    clean("SELECT 1 AS \"update\"");
}

#[test]
fn item_3_a_name_that_the_table_does_not_list() {
    for sql in [
        "SELECT $__timeFilter_ms(ts)",
        "SELECT $__timeInterval(ts)",
        "SELECT $__fromTime",
        "SELECT $__dateFilter(ts)",
        "SELECT $__nothing",
        "SELECT $__interval",
        "SELECT x > $__interval",
        "SELECT ${__from:csv}",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    let r = check("SELECT $__fromTime");
    assert_eq!(r.faults[0].message, "$__fromTime is not a macro of Hune");
}

#[test]
fn item_4_the_arguments() {
    for sql in [
        "SELECT $__timeFilter()",
        "SELECT $__timeFilter(a, b)",
        "SELECT $__timeFilter",
        "SELECT $__timeGroup(ts)",
        "SELECT $__timeGroup(ts, '5m', 0)",
        "SELECT $__unixEpochGroup(ts, '5m', NULL)",
        "SELECT $__timeFrom(1)",
        "SELECT $__interval_s()",
        "SELECT $__from()",
        "SELECT $__timeGroup(ts, 5m)",
        "SELECT $__timeGroup(ts, '1h30m')",
        "SELECT $__timeGroup(ts, '1.5s')",
        "SELECT $__timeGroup(ts, '0s')",
        "SELECT $__timeGroup(ts, '2y')",
        "SELECT $__timeGroup(ts, 'x')",
        "SELECT $__timeGroup(ts, '5m' || '1')",
        "SELECT $__timeGroup(ts, E'5m')",
        "SELECT $__timeGroup(ts, ${__interval:csv})",
        "SELECT $__timeGroup(ts, $q)",
        "SELECT $__conditionalAll(a, $t)",
    ] {
        fault(sql, ErrorCode::BadFormat);
    }
    clean("SELECT $__timeFrom ()");
    clean("SELECT $__timeFrom( )");
}

#[test]
fn item_5_the_interval_in_the_sql_of_a_variable() {
    for sql in [
        "SELECT $__interval_s",
        "SELECT $__interval_ms",
        "SELECT $__timeGroup(ts, $__interval)",
        "SELECT $__timeGroup(ts, ${__interval})",
        "SELECT $__unixEpochGroupAlias(ts, '$__interval')",
    ] {
        let r = check_use(sql, SqlUse::Variable);
        assert_eq!(codes(&r), vec![ErrorCode::NotPermitted], "{sql}: {:?}", r.faults);
        // In a panel the same text is good.
        clean(sql);
    }
    let r = check_use("SELECT $__timeGroup(ts, '5m'), $__timeFrom, $__from", SqlUse::Variable);
    assert!(r.faults.is_empty(), "{:?}", r.faults);
}

#[test]
fn item_6_and_8_places_with_no_placeholder() {
    for sql in [
        "SELECT $$ $t $$",
        "SELECT $tag$ ${q} $tag$",
        "SELECT $\u{e9}$ $t $\u{e9}$",
        "SELECT B'$t'",
        "SELECT X'${t}'",
        "SELECT N'$t'",
        "SELECT U&'$t'",
        "SELECT U&\"$t\"",
        "SELECT u&\"${c}\"",
    ] {
        let r = check(sql);
        assert!(r.faults.iter().any(|f| f.code == ErrorCode::NotPermitted), "{sql}: {:?}", r.faults);
    }
    // A name that is not a variable of the document stays text.
    clean("SELECT $$ $nothing $$");
}

#[test]
fn item_8_the_table_of_7_8_in_a_string_and_a_name() {
    fault("SELECT '$qm'", ErrorCode::NotPermitted);
    fault("SELECT E'$qm'", ErrorCode::NotPermitted);
    clean("SELECT E'${qm:csv}'");
    fault("SELECT '${t:singlequote}'", ErrorCode::NotPermitted);
    fault("SELECT \"$t\"", ErrorCode::NotPermitted);
    fault("SELECT ${t:regex}", ErrorCode::NotPermitted);
    fault("SELECT \"${c:json}\"", ErrorCode::NotPermitted);
    clean("SELECT E'${t:json}', '${t:pipe}'");
}

#[test]
fn warnings() {
    let r =
        check("SELECT '$__timeFrom', \"$__from\", E'$__interval' -- x\n, '${nope}' /* $z */, '[[t]]' , '${t:date}'");
    let kinds: Vec<WarningKind> = r.warnings.iter().map(|w| w.kind).collect();
    assert!(r.faults.is_empty(), "{:?}", r.faults);
    assert_eq!(
        kinds,
        vec![
            WarningKind::MacroInLiteral,
            WarningKind::MacroInLiteral,
            WarningKind::MacroInLiteral,
            WarningKind::UnknownVariable,
            WarningKind::DoubleBracket,
            WarningKind::UnknownFormatInString,
        ]
    );
    // The interval of a group macro is no text: no warning.
    let r = check("SELECT $__timeGroup(ts, '$__interval'), $__timeGroup(ts, $__interval)");
    assert!(r.faults.is_empty() && r.warnings.is_empty(), "{:?} {:?}", r.faults, r.warnings);
    // Warning 3 of 7.18 does not apply.
    let r = check("SELECT 1 FROM t FINAL SETTINGS x");
    assert!(r.warnings.iter().all(|w| w.kind != WarningKind::FinalOrSettings));
    let r = save_checks("SELECT 1 FROM t FINAL", SqlUse::Panel, &vars());
    assert!(r.warnings.iter().any(|w| w.kind == WarningKind::FinalOrSettings));
}

/// A document fault of the expansion is a fault of the checks at save.
#[test]
fn the_expansion_and_the_checks_agree() {
    let defs = vars();
    let mut request = RequestValues::new();
    request.insert("t", false, &["x"]);
    request.insert("q", false, &["x"]);
    request.insert("qm", false, &["x"]);
    request.insert("c", false, &["a"]);
    request.insert("cm", false, &["a"]);
    let resolved = resolve_values(&defs, &request).unwrap();
    let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
    let interval = compute_interval(&range, 1200, 1).unwrap();
    for sql in [
        "SELECT $$ $t $$",
        "SELECT B'$t'",
        "SELECT U&'$t'",
        "SELECT \"$t\"",
        "SELECT '${t:singlequote}'",
        "SELECT ${t:json}",
        "SELECT $__nothing",
        "SELECT $__timeInterval(ts)",
        "SELECT $__timeFilter()",
        "SELECT $__timeGroup(ts, 5m)",
        "SELECT $__timeGroup(ts, '2y')",
        "SELECT $__timeGroup(ts, '5m', 0)",
        "SELECT $__interval",
        "SELECT ${nope:zz}",
        "SELECT '$qm'",
    ] {
        let req = ExpandRequest { sql, values: &resolved, range, interval: Some(interval), max_query_size: 10_000 };
        match expand_for(Dialect::Postgres, &req) {
            Err(ExpandError::Invalid(i)) if i.kind != SqlInvalidKind::Statement && i.kind != SqlInvalidKind::Lexer => {
                assert!(!check(sql).faults.is_empty(), "{sql}");
            }
            other => panic!("{sql}: {other:?}"),
        }
    }
}
