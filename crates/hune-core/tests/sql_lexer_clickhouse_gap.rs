// SPDX-License-Identifier: MIT OR Apache-2.0
//! The lexer gap of review 3 (finding 1): ClickHouse 26.8.34.13 reads `//` as a line comment, ends a line comment at LF
//! only, and reads `$1$ ... $1$` as a heredoc (7.5). Before the fix, the lexer of Hune did not, so a value of a
//! viewer could leave its literal in the four texts below. Each text now gets the error that 7.11 and 7.6a give, and a
//! value never reaches the SQL as code.
//!
//! The texts are the end-to-end texts of the verifier, with the tables of `tests/fixtures/clickhouse/init.sql`. The
//! ClickHouse suite sends the same texts through the query route (`real_ch.rs`) and the probe table of the lexer
//! (`crates/hune-source/tests/clickhouse.rs`).

use hune_core::error::ErrorCode;
use hune_core::sql::adhoc::{RequestFilter, bind};
use hune_core::sql::{
    AdhocSave, Dialect, ExpandError, ExpandRequest, RequestValues, SqlUse, TextValueType, VariableDef,
    check_time_range, expand_prepared, expand_prepared_adhoc, resolve_values_at, save_checks_adhoc, save_checks_for,
};

/// A text variable in code, between two `//` comments that each hold one `'`.
pub const SLASHES: &str = "SELECT node, weight FROM analytics.seed   // the team's main table\n\
WHERE blocks > $min                         // don't show small values\nORDER BY weight DESC LIMIT 3";
/// A heredoc with a tag that starts with a digit hides a `'`, and a `--` comment holds the other `'`.
pub const HEREDOC: &str =
    "SELECT node, weight, $1$'$1$ AS h FROM analytics.seed\nWHERE blocks > $min -- '\nORDER BY weight DESC LIMIT 3";
/// A query parameter `{x:Identifier}` between two `//` comments that each hold one `'`.
pub const QPARAM: &str =
    "SELECT {x:String} AS t, count() AS n // the user's table:\nFROM analytics.{x:Identifier} // that's all";
/// `$__adhocFilters` after a `//` comment that holds one `'`, inside the string that ClickHouse reads.
pub const ADHOC: &str = "SELECT node, weight FROM analytics.seed // it's\nWHERE node = ' AND $__adhocFilters -- '\nORDER BY weight DESC LIMIT 3";
/// The value of the viewer: it tries to add a second SELECT on another table.
pub const HOSTILE: &str = "1000 UNION ALL SELECT toString(x), toDecimal128(4242, 9) FROM analytics.secret --";

const NOT_A_NUMBER: &str = "Outside a string, only a number is permitted.";

fn text_vars(names: &[&str]) -> Vec<VariableDef> {
    names.iter().map(|n| VariableDef::text(n, TextValueType::Text)).collect()
}

fn expand_one(sql: &str, defs: &[VariableDef], name: &str, value: &str) -> Result<String, ExpandError> {
    let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
    let mut req = RequestValues::new();
    req.insert(name, false, &[value]);
    let values = resolve_values_at(defs, &req, &range).unwrap();
    let r = ExpandRequest { sql, values: &values, range, interval: None, max_query_size: 262_144 };
    expand_prepared(Dialect::ClickHouse, &r).map(|p| p.sql)
}

#[test]
fn a_text_variable_between_two_slash_comments_is_refused() {
    let defs = text_vars(&["min"]);
    assert!(save_checks_for(Dialect::ClickHouse, SLASHES, SqlUse::Panel, &defs).faults.is_empty());
    match expand_one(SLASHES, &defs, "min", HOSTILE) {
        Err(ExpandError::ValueRefused(r)) => {
            assert_eq!((r.variable.as_str(), r.reason.as_str()), ("min", NOT_A_NUMBER))
        }
        other => panic!("the value must be refused: {other:?}"),
    }
    let honest = expand_one(SLASHES, &defs, "min", "50").unwrap();
    assert!(honest.contains("WHERE blocks > 50 "), "{honest}");
}

#[test]
fn a_text_variable_after_a_digit_heredoc_is_refused() {
    let defs = text_vars(&["min"]);
    assert!(save_checks_for(Dialect::ClickHouse, HEREDOC, SqlUse::Panel, &defs).faults.is_empty());
    match expand_one(HEREDOC, &defs, "min", HOSTILE) {
        Err(ExpandError::ValueRefused(r)) => {
            assert_eq!((r.variable.as_str(), r.reason.as_str()), ("min", NOT_A_NUMBER))
        }
        other => panic!("the value must be refused: {other:?}"),
    }
    assert!(expand_one(HEREDOC, &defs, "min", "50").unwrap().contains("$1$'$1$ AS h"));
}

#[test]
fn a_query_parameter_between_two_slash_comments_is_an_identifier() {
    let defs = text_vars(&["x"]);
    let save = save_checks_for(Dialect::ClickHouse, QPARAM, SqlUse::Panel, &defs);
    assert!(
        save.faults.iter().any(|f| f.code == ErrorCode::NotPermitted && f.message.contains("Identifier")),
        "the save refuses {{x:Identifier}} for a text variable: {:?}",
        save.faults
    );
    match expand_one(QPARAM, &defs, "x", "secret") {
        Err(e) => assert_eq!(e.api_code(), "sql_invalid", "{e:?}"),
        Ok(sql) => panic!("the run must refuse the text: {sql}"),
    }
}

#[test]
fn an_adhoc_value_stays_out_of_the_sql() {
    let none: Vec<VariableDef> = Vec::new();
    let save = save_checks_adhoc(
        Dialect::ClickHouse,
        ADHOC,
        SqlUse::Panel,
        &none,
        AdhocSave { names: &["f"], on_source: true },
    );
    assert!(save.faults.is_empty(), "{:?}", save.faults);
    let cols = vec!["node".to_owned()];
    let f = [RequestFilter { key: "node".into(), op: "=".into(), value: HOSTILE.into() }];
    let ad = bind("f", &cols, &cols, &f).unwrap();
    let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
    let values = resolve_values_at(&none, &RequestValues::new(), &range).unwrap();
    let r = ExpandRequest { sql: ADHOC, values: &values, range, interval: None, max_query_size: 262_144 };
    // The macro is inside the string that ClickHouse reads, so its text stays (7.10a, rule 3): no value is in the SQL.
    let sql = expand_prepared_adhoc(Dialect::ClickHouse, &r, Some(&ad)).unwrap().sql;
    assert_eq!(sql, ADHOC);
    assert!(!sql.contains("analytics.secret"));
}
