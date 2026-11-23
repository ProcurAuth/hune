// SPDX-License-Identifier: MIT OR Apache-2.0
//! The security tests of the member `allow_custom_value` (7.7, 7.8, 7.35, 12.4 T75 to T78).
//!
//! The engine is the same for 6.20, 6.21 (also as a parent), 6.21a and the preview, so the cases here run the
//! engine for ClickHouse, PostgreSQL and MongoDB. The server tests run the four routes.

mod mongo_common;

use hune_core::error::ErrorCode;
use hune_core::mongo::MongoExpandError;
use hune_core::sql::{
    Dialect, ExpandError, ExpandRequest, RequestValues, SaveCheckResult, SqlUse, TextValueType, ValuesError,
    VariableDef, check_time_range, compute_interval, expand_for, resolve_values, save_checks_for,
};

const FROM: i64 = 1_794_211_200_000;
const TO: i64 = 1_794_232_800_000;

/// The result of the whole chain for a SQL text: the check of the values, then the expansion.
#[derive(Debug, PartialEq, Eq)]
enum Out {
    /// `422 variable_value_refused` at the check of the values or at the expansion.
    Refused(String),
    /// A document error of the expansion.
    Invalid(String),
    /// The expanded text.
    Sql(String),
}

fn expand(dialect: Dialect, sql: &str, defs: &[VariableDef], values: &RequestValues) -> Out {
    let resolved = match resolve_values(defs, values) {
        Ok(r) => r,
        Err(ValuesError::Refused(r)) => return Out::Refused(format!("{}: {}", r.variable, r.reason)),
        Err(e) => panic!("{e:?}"),
    };
    let range = check_time_range(FROM, TO).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let req = ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 262_144 };
    match expand_for(dialect, &req) {
        Ok(s) => Out::Sql(s),
        Err(ExpandError::ValueRefused(r)) => Out::Refused(format!("{}: {}", r.variable, r.reason)),
        Err(e) => Out::Invalid(format!("{e:?}")),
    }
}

fn save(dialect: Dialect, sql: &str, defs: &[VariableDef]) -> SaveCheckResult {
    save_checks_for(dialect, sql, SqlUse::Panel, defs)
}

fn values(items: &[(&str, bool, &[&str])]) -> RequestValues {
    let mut r = RequestValues::new();
    for (n, a, v) in items {
        r.insert(n, *a, v);
    }
    r
}

/// Values that a hostile viewer sends: quote marks, backslashes, comment marks, `$`, Unicode spaces and `$__all`.
const HOSTILE: &[&str] = &[
    "x' OR 1=1 --",
    "x\\' OR 1=1 --",
    "'; DROP TABLE t; --",
    "\\",
    "\\\\'",
    "x /* y */",
    "x -- y",
    "$secret",
    "$$ROOT",
    "${v}",
    "$__all",
    "a\u{a0}b",
    "a\u{2028}b",
    "a\u{3000}b",
    "\"",
    "`",
    "1; SELECT 1",
    "1 OR 1",
    "-1",
    "12.5",
    "0",
    "e5",
    "",
    "a",
];

const SINGLE_TEMPLATES: &[&str] = &[
    "SELECT * FROM t WHERE a = '$v'",
    "SELECT * FROM t WHERE a = '${v}'",
    "SELECT * FROM t WHERE a = ${v:singlequote}",
    "SELECT * FROM t WHERE a = ${v:sqlstring}",
    "SELECT * FROM t WHERE a = $v",
    "SELECT * FROM t WHERE a = ${v:raw}",
    "SELECT * FROM t WHERE a = ${v:csv}",
    "SELECT * FROM t WHERE a = '${v:csv}'",
    "SELECT * FROM `$v`",
    "SELECT * FROM \"${v}\"",
    "SELECT * FROM t WHERE a = $v /* $v */ AND b = '$__timeFrom'",
];

const LIST_TEMPLATES: &[&str] = &[
    "SELECT * FROM t WHERE a IN ($v)",
    "SELECT * FROM t WHERE a IN (${v:singlequote})",
    "SELECT * FROM t WHERE a IN (${v:csv})",
    "SELECT * FROM t WHERE a = '${v:csv}'",
    "SELECT * FROM t WHERE a = '$v'",
    "SELECT * FROM `${v}`",
];

fn kinds(open: bool) -> [VariableDef; 3] {
    [
        VariableDef::query("v", false, false, None).with_open(open),
        VariableDef::query("v", false, false, None),
        VariableDef::text("v", TextValueType::Text),
    ]
}

#[test]
fn a_query_variable_has_the_same_expansion_with_the_member_true_and_false_and_as_a_text_variable() {
    let mut compared = 0;
    for dialect in [Dialect::ClickHouse, Dialect::Postgres] {
        for template in SINGLE_TEMPLATES {
            for value in HOSTILE {
                let req = values(&[("v", false, &[value])]);
                let [open, closed, text] = kinds(true);
                let a = expand(dialect, template, &[open], &req);
                let b = expand(dialect, template, &[closed], &req);
                assert_eq!(a, b, "{dialect:?} {template:?} {value:?}: the member true and false");
                // The `text` variable has the same rows, except that a quoted name takes only a `custom` variable
                // and the empty value: a `text` variable takes 0 bytes as a `query` variable does.
                let c = expand(dialect, template, &[text], &req);
                assert_eq!(a, c, "{dialect:?} {template:?} {value:?}: as a text variable");
                // The checks at save are the same too.
                let [open, closed, text] = kinds(true);
                assert_eq!(save(dialect, template, &[open]), save(dialect, template, &[closed]));
                assert_eq!(save(dialect, template, &[VariableDef::query("v", false, false, None)]).faults.len(), {
                    save(dialect, template, &[text]).faults.len()
                });
                compared += 1;
            }
        }
        for template in LIST_TEMPLATES {
            for case in [
                (false, vec!["a", "b'"]),
                (false, vec![]),
                (true, vec![]),
                (true, vec!["x", "y z"]),
                (false, vec!["$secret", "1", "-2.5"]),
            ] {
                let req = values(&[("v", case.0, &case.1)]);
                let defs = |open: bool| [VariableDef::query("v", true, true, None).with_open(open)];
                assert_eq!(
                    expand(dialect, template, &defs(true), &req),
                    expand(dialect, template, &defs(false), &req),
                    "{dialect:?} {template:?} {case:?}"
                );
                assert_eq!(save(dialect, template, &defs(true)), save(dialect, template, &defs(false)));
                compared += 1;
            }
        }
    }
    assert!(compared > 500, "{compared}");
}

#[test]
fn a_closed_custom_variable_refuses_each_value_that_is_not_an_option_before_any_expansion() {
    let options = ["prod", "stage", "Prod Env", "\u{e9}"];
    for member in [None, Some(false)] {
        let def =
            |multi: bool| VariableDef::custom("v", &options, multi, false, None).with_open(member.unwrap_or(false));
        let outside = [
            "dev",
            "PROD",
            "prod ",
            " prod",
            "prod\u{a0}",
            "e\u{301}",
            "\u{c9}",
            "Production",
            "Prod Env ",
            "prod,stage",
            "$__all",
            "'",
            "",
        ];
        for bad in outside {
            // A single value, a list with one item outside the options, and the text of an option instead of its value.
            let single = expand(Dialect::ClickHouse, "SELECT '$v'", &[def(false)], &values(&[("v", false, &[bad])]));
            assert!(matches!(&single, Out::Refused(m) if m.contains("not an option")), "{bad:?}: {single:?}");
            let list = expand(
                Dialect::ClickHouse,
                "SELECT '${v:csv}'",
                &[def(true)],
                &values(&[("v", false, &["prod", bad])]),
            );
            assert!(matches!(&list, Out::Refused(m) if m.contains("not an option")), "{bad:?}: {list:?}");
            let pg = expand(Dialect::Postgres, "SELECT 1 FROM \"$v\"", &[def(false)], &values(&[("v", false, &[bad])]));
            assert!(matches!(&pg, Out::Refused(m) if m.contains("not an option")), "{bad:?}: {pg:?}");
        }
        // The value of an option is accepted, and a closed `custom` variable keeps its place in a quoted name.
        let ok =
            expand(Dialect::ClickHouse, "SELECT 1 FROM `$v`", &[def(false)], &values(&[("v", false, &["Prod Env"])]));
        assert_eq!(ok, Out::Sql("SELECT 1 FROM `Prod Env`".into()));
    }
}

#[test]
fn an_open_custom_variable_never_gives_raw_text_for_a_hostile_value() {
    let def = |multi: bool| VariableDef::custom("v", &["a", "b"], multi, true, None).with_open(true);
    for dialect in [Dialect::ClickHouse, Dialect::Postgres] {
        for value in HOSTILE.iter().filter(|v| !v.is_empty() && **v != "$__all") {
            // In a string and with a quoted format: one literal, whatever the value.
            for template in [
                "SELECT * FROM t WHERE a = '$v'",
                "SELECT * FROM t WHERE a = ${v:singlequote}",
                "SELECT * FROM t WHERE a = ${v:sqlstring}",
            ] {
                let out = expand(dialect, template, &[def(false)], &values(&[("v", false, &[value])]));
                match out {
                    Out::Sql(sql) => {
                        let lexed = hune_core::sql::lex_all_for(dialect, &sql);
                        let strings = lexed.iter().filter(|t| t.kind == hune_core::sql::TokenKind::String).count();
                        assert!(strings >= 1, "{dialect:?} {template:?} {value:?}: {sql:?}");
                    }
                    Out::Refused(_) => {}
                    other => panic!("{dialect:?} {template:?} {value:?}: {other:?}"),
                }
            }
            // In code with the format none: a number or a refusal, never raw text.
            let out =
                expand(dialect, "SELECT * FROM t WHERE a = $v", &[def(false)], &values(&[("v", false, &[value])]));
            match (hune_core::sql::is_number_text(value), out) {
                (true, Out::Sql(sql)) => assert_eq!(sql, format!("SELECT * FROM t WHERE a = {value}")),
                (false, Out::Refused(m)) => assert!(m.contains("v"), "{m}"),
                (_, other) => panic!("{dialect:?} {value:?}: {other:?}"),
            }
            // In a list in code with the format csv: numbers only.
            let out =
                expand(dialect, "SELECT 1 WHERE a IN (${v:csv})", &[def(true)], &values(&[("v", false, &[value])]));
            assert_eq!(matches!(out, Out::Sql(_)), hune_core::sql::is_number_text(value), "{value:?}");
        }
    }
}

#[test]
fn an_open_custom_variable_in_a_quoted_name_is_refused_at_save_and_at_run() {
    for dialect in [Dialect::ClickHouse, Dialect::Postgres] {
        let templates: &[&str] = match dialect {
            Dialect::ClickHouse => &["SELECT * FROM `$v`", "SELECT * FROM \"${v}\"", "SELECT * FROM \"${v:csv}\""],
            Dialect::Postgres => &["SELECT * FROM \"$v\"", "SELECT * FROM \"${v}\"", "SELECT * FROM \"${v:csv}\""],
        };
        for template in templates {
            let open = [VariableDef::custom("v", &["a"], false, false, None).with_open(true)];
            let closed = [VariableDef::custom("v", &["a"], false, false, None)];
            // At save: `not_permitted` for the open variable, nothing for the closed one.
            let saved = save(dialect, template, &open);
            assert_eq!(saved.faults.len(), 1, "{dialect:?} {template:?}");
            assert_eq!(saved.faults[0].code, ErrorCode::NotPermitted);
            assert!(save(dialect, template, &closed).faults.is_empty(), "{dialect:?} {template:?}");
            // At run: a document error, also for a value that is an option.
            let out = expand(dialect, template, &open, &values(&[("v", false, &["a"])]));
            assert!(matches!(out, Out::Invalid(_)), "{dialect:?} {template:?}: {out:?}");
            assert!(matches!(expand(dialect, template, &closed, &values(&[("v", false, &["a"])])), Out::Sql(_)));
        }
    }
}

#[test]
fn the_list_all_of_an_open_custom_variable_stays_author_text() {
    let options = ["a b", "c'd", "7"];
    let open = [VariableDef::custom("v", &options, true, true, None).with_open(true)];
    let closed = [VariableDef::custom("v", &options, true, true, None)];
    let req = values(&[("v", true, &[])]);
    for template in
        ["SELECT 1 WHERE a IN ($v)", "SELECT 1 WHERE a IN (${v:csv})", "SELECT 1 WHERE a IN (${v:singlequote})"]
    {
        let (a, b) =
            (expand(Dialect::ClickHouse, template, &open, &req), expand(Dialect::ClickHouse, template, &closed, &req));
        assert_eq!(a, b, "{template}");
    }
    // The `all_value` is author text too.
    let with_all_value = [VariableDef::custom("v", &options, true, true, Some("1, 2")).with_open(true)];
    let out = expand(Dialect::ClickHouse, "SELECT 1 WHERE a IN ($v)", &with_all_value, &req);
    assert_eq!(out, Out::Sql("SELECT 1 WHERE a IN (1, 2)".into()));
    // A selection of values is free text: the same list in code needs numbers.
    let out = expand(Dialect::ClickHouse, "SELECT 1 WHERE a IN (${v:csv})", &open, &values(&[("v", false, &["a b"])]));
    assert!(matches!(out, Out::Refused(_)), "{out:?}");
}

#[test]
fn the_bounds_of_a_typed_value_of_an_open_custom_variable() {
    let def = [VariableDef::custom("v", &["a"], false, false, None).with_open(true)];
    let run = |value: &str| expand(Dialect::ClickHouse, "SELECT '$v'", &def, &values(&[("v", false, &[value])]));
    assert!(matches!(run(""), Out::Refused(_)), "0 bytes");
    assert_eq!(run("x"), Out::Sql("SELECT 'x'".into()), "1 byte");
    assert!(matches!(run(&"a".repeat(1024)), Out::Sql(_)), "1,024 bytes");
    assert!(matches!(run(&"a".repeat(1025)), Out::Refused(_)), "1,025 bytes");
    assert!(matches!(run("a\u{0}b"), Out::Refused(_)), "a control character");
    assert!(matches!(run("a\u{7f}b"), Out::Refused(_)), "DEL");
    assert!(matches!(run("$__all"), Out::Refused(_)), "the reserved value");
}

#[test]
fn a_mongodb_open_custom_variable_refuses_a_value_that_starts_with_a_dollar() {
    use mongo_common::*;
    let text = r#"{"collection":"events","aggregate":[{"$match":{"env":"$v"}}]}"#;
    let open = vec![VariableDef::custom("v", &["prod", "$dev"], false, false, None).with_open(true)];
    let closed = vec![VariableDef::custom("v", &["prod", "$dev"], false, false, None)];
    let run_defs = |defs: &[VariableDef], v: &str| run_with(text, defs, &request(&[("v", false, &[v])]), false);
    // A closed variable: the option `$dev` is author text and runs. Any other value is refused (7.7).
    assert!(run_defs(&closed, "$dev").is_ok());
    // The closed variable refuses a value outside the options at the check of the values, before the expansion.
    let refused = resolve_values(&closed, &request(&[("v", false, &["$secret"])]));
    assert!(matches!(refused, Err(ValuesError::Refused(_))));
    // An open variable: PM2 refuses `$secret` and also the option `$dev`. The value `secret` runs.
    for bad in ["$secret", "$dev", "$", "$$ROOT"] {
        assert!(matches!(run_defs(&open, bad), Err(MongoExpandError::ValueRefused(_))), "{bad}");
    }
    let ok = run_defs(&open, "secret").unwrap();
    assert!(format!("{:?}", ok.to_value()).contains("secret"));
    // The list "All" of an open variable is author text: an option that starts with `$` runs.
    let all = vec![VariableDef::custom("v", &["prod", "$dev"], true, true, None).with_open(true)];
    let text_list = r#"{"collection":"events","aggregate":[{"$match":{"env":{"$in":"${v:json}"}}}]}"#;
    let out = run_with(text_list, &all, &request(&[("v", true, &[])]), false);
    assert!(out.is_ok(), "{out:?}");
    // A placeholder of an open `custom` variable in a key place is refused at save, as for each variable.
    let key = r#"{"collection":"events","aggregate":[{"$match":{"$v":1}}]}"#;
    assert!(!save_with(key, &open, SqlUse::Panel).faults.is_empty());
}

#[test]
fn a_mongodb_query_variable_has_the_same_result_with_the_member_true_and_false() {
    use mongo_common::*;
    let templates = [
        r#"{"collection":"events","aggregate":[{"$match":{"h":"$v"}}]}"#,
        r#"{"collection":"events","aggregate":[{"$match":{"h":"x-$v-y"}}]}"#,
        r#"{"collection":"events","aggregate":[{"$match":{"h":{"$in":"$v"}}}]}"#,
        r#"{"collection":"events","aggregate":[{"$match":{"$v":1}}]}"#,
    ];
    let hostile = ["$secret", "$$ROOT", "a.b", "{\"$out\":\"x\"}", "12", "", "text", "\u{2028}"];
    for template in templates {
        for value in hostile {
            let defs = |open: bool| {
                vec![
                    VariableDef::query("v", false, false, None).with_open(open),
                    VariableDef::text("t", TextValueType::Text),
                ]
            };
            let req = request(&[("v", false, &[value])]);
            let a = run_with(template, &defs(true), &req, false);
            let b = run_with(template, &defs(false), &req, false);
            assert_eq!(a, b, "{template} {value:?}");
            assert_eq!(
                save_with(template, &defs(true), SqlUse::Panel),
                save_with(template, &defs(false), SqlUse::Panel)
            );
            // As a `text` variable the result is the same.
            let text_defs = vec![VariableDef::text("v", TextValueType::Text)];
            assert_eq!(a, run_with(template, &text_defs, &req, false), "{template} {value:?}: as text");
        }
    }
}

#[test]
fn the_check_of_a_request_reads_only_the_variables_of_the_request() {
    // The options of a `query` variable never reach the engine: the member has no effect on the request check.
    let defs = [VariableDef::query("q", true, false, None).with_open(false)];
    let many: Vec<String> = (0..10_000).map(|i| format!("value-{i}")).collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    let mut r = RequestValues::new();
    r.insert("q", false, &refs);
    assert!(resolve_values(&defs, &r).is_ok());
}
