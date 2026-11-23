//! The rows of 15.6 for `allowCustomValue` (F41, ID-K200, ID-K206): the forms v1 and v2beta1.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn query_var(extra: Value) -> Value {
    let mut v = json!({
        "type": "query", "name": "q", "label": "", "hide": 0, "datasource": ch(), "query": "SELECT DISTINCT a FROM t",
        "multi": false, "includeAll": false, "allValue": "", "refresh": 1, "sort": 0, "regex": "", "current": {}, "options": []
    });
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

fn custom_var(extra: Value) -> Value {
    let mut v = json!({"type": "custom", "name": "c", "query": "a,b", "multi": false, "includeAll": false,
        "current": {"value": "a"}, "options": []});
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

/// The items that name the member `allowCustomValue`, in any form.
fn member_items(r: &DashboardImport) -> Vec<&hune_grafana::ReportItem> {
    r.items.iter().filter(|i| i.member.ends_with("allowCustomValue")).collect()
}

fn one_member_item(r: &DashboardImport) -> &hune_grafana::ReportItem {
    let found = member_items(r);
    assert_eq!(found.len(), 1, "{:#?}", r.items);
    found[0]
}

fn is_open(v: &Value) -> bool {
    v.get("allow_custom_value") == Some(&json!(true))
}

fn import(vars: Vec<Value>, panel_sql: &str) -> DashboardImport {
    let mut p = panel(1, "stat", 0);
    p["targets"] = json!([target(panel_sql)]);
    run(&dash(vec![p], vars))
}

#[test]
fn a_query_variable_follows_the_member_and_an_absent_member_is_true() {
    for (member, open) in
        [(Some(json!(true)), true), (None, true), (Some(Value::Null), true), (Some(json!(false)), false)]
    {
        let mut v = query_var(json!({}));
        if let Some(m) = &member {
            v["allowCustomValue"] = m.clone();
        }
        let r = import(vec![v], "SELECT 1 AS a");
        let var = &doc(&r)["variables"][0];
        assert_eq!(is_open(var), open, "{member:?}");
        // `false` has no member in the document (4.38).
        assert!(var.get("allow_custom_value").is_none() || open);
        assert!(member_items(&r).is_empty(), "{member:?}: {:#?}", r.items);
    }
}

#[test]
fn a_member_that_is_not_a_boolean_is_unsupported_and_the_variable_stays_closed() {
    for bad in [json!("yes"), json!({}), json!([true])] {
        let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![query_var(json!({"allowCustomValue": bad.clone()}))]));
        let item = one_member_item(&r);
        assert_eq!(item.class, Class::Unsupported, "{bad}");
        assert_eq!(item.fallback, Some(Fallback::other("no member")));
        let r = import(vec![query_var(json!({"allowCustomValue": bad}))], "SELECT 1 AS a");
        assert!(!is_open(&doc(&r)["variables"][0]));
    }
}

#[test]
fn a_custom_variable_with_an_absent_member_stays_closed_with_a_changed_item() {
    // ID-K206: a closed `custom` variable is a check of the server. Grafana reads an absent member as true.
    for member in [None, Some(Value::Null)] {
        let mut v = custom_var(json!({}));
        if let Some(m) = member {
            v["allowCustomValue"] = m;
        }
        let r = import(vec![v], "SELECT 1 AS a");
        assert!(!is_open(&doc(&r)["variables"][0]));
        let item = one_member_item(&r);
        assert_eq!(item.class, Class::Changed);
        assert!(item.reason.contains("keeps the variable closed"), "{}", item.reason);
        assert!(item.value.is_none());
    }
}

#[test]
fn a_custom_variable_with_true_is_open_and_keeps_a_typed_default() {
    let r = import(
        vec![custom_var(json!({"allowCustomValue": true, "current": {"value": "typed value"}}))],
        "SELECT 1 AS a",
    );
    no_items(&r);
    let var = &doc(&r)["variables"][0];
    assert!(is_open(var));
    assert_eq!(var["default"], json!(["typed value"]));
    // The same default is refused when the variable is closed (4.11).
    let r = import(
        vec![custom_var(json!({"allowCustomValue": false, "current": {"value": "typed value"}}))],
        "SELECT 1 AS a",
    );
    assert_eq!(doc(&r)["variables"][0]["default"], json!([]));
    assert_eq!(r.items[0].fallback, Some(Fallback::other("an empty array")));
    // A control character in a typed default gives an empty array.
    let r = import(vec![custom_var(json!({"allowCustomValue": true, "current": {"value": "a\nb"}}))], "SELECT 1 AS a");
    assert_eq!(doc(&r)["variables"][0]["default"], json!([]));
    // `false` gives no member.
    let r = import(vec![custom_var(json!({"allowCustomValue": false}))], "SELECT 1 AS a");
    no_items(&r);
    assert!(doc(&r)["variables"][0].get("allow_custom_value").is_none());
}

#[test]
fn the_member_of_a_text_variable_is_metadata() {
    let text = json!({"type": "textbox", "name": "t", "label": "", "query": "x", "current": {"value": "x"},
        "options": [{"value": "x"}], "allowCustomValue": true});
    let r = import(vec![text], "SELECT 1 AS a");
    let var = &doc(&r)["variables"][0];
    assert!(var.get("allow_custom_value").is_none());
    assert!(member_items(&r).is_empty());
}

/// A custom variable that Grafana opens, and a panel query that names it. It gives the variable of the document.
fn open_custom_in(sql: &str) -> (DashboardImport, Value) {
    let r = import(vec![custom_var(json!({"allowCustomValue": true, "current": {"value": "typed"}}))], sql);
    let var = doc(&r)["variables"][0].clone();
    (r, var)
}

#[test]
fn a_placeholder_in_a_quoted_name_keeps_the_custom_variable_closed() {
    for sql in ["SELECT a FROM \"$c\"", "SELECT a FROM `${c}`", "SELECT a FROM `${c:csv}`"] {
        let (r, var) = open_custom_in(sql);
        assert!(!is_open(&var), "{sql}");
        let item = member_items(&r);
        assert_eq!(item.len(), 1, "{sql}: {:#?}", r.items);
        assert_eq!(item[0].class, Class::Changed);
        assert!(item[0].reason.contains("quoted name"), "{}", item[0].reason);
        assert_eq!(item[0].value.as_deref(), Some("true"));
        // The default is not an option: the closed rule of 4.11 gives an empty array.
        assert_eq!(var["default"], json!([]));
    }
}

#[test]
fn a_placeholder_in_code_with_the_format_none_raw_or_csv_keeps_the_variable_closed() {
    for sql in
        ["SELECT a FROM t WHERE x = $c", "SELECT a FROM t WHERE x = ${c:raw}", "SELECT a FROM t WHERE x IN (${c:csv})"]
    {
        let (r, var) = open_custom_in(sql);
        assert!(!is_open(&var), "{sql}");
        let item = member_items(&r);
        assert_eq!(item.len(), 1, "{sql}: {:#?}", r.items);
        assert!(item[0].reason.contains("code"), "{}", item[0].reason);
    }
}

#[test]
fn a_string_or_a_quoted_format_in_code_keeps_the_variable_open() {
    for sql in [
        "SELECT a FROM t WHERE x = '$c'",
        "SELECT a FROM t WHERE x = ${c:singlequote}",
        "SELECT a FROM t WHERE x = ${c:sqlstring}",
        "SELECT a FROM t WHERE x = '${c:csv}'",
        "SELECT a FROM t",
    ] {
        let (r, var) = open_custom_in(sql);
        assert!(is_open(&var), "{sql}: {:#?}", r.items);
        no_items(&r);
        assert_eq!(var["default"], json!(["typed"]));
    }
}

#[test]
fn d6_each_placeholder_form_in_sql_counts_as_a_use() {
    // D6 of the security review of 2026-11-16: each form of 7.6 counts, and `[[c]]` too (the importer rewrites it
    // to `${c}` in code and in strings). A longer name is another variable.
    for (sql, place) in [
        ("SELECT a FROM t WHERE x = $c", "code"),
        ("SELECT a FROM t WHERE x = ${c}", "code"),
        ("SELECT a FROM t WHERE x = ${c:raw}", "code"),
        ("SELECT a FROM t WHERE x IN (${c:csv})", "code"),
        ("SELECT a FROM t WHERE x = [[c]]", "code"),
        ("SELECT a FROM \"$c\"", "quoted name"),
        ("SELECT a FROM \"${c}\"", "quoted name"),
        ("SELECT a FROM `${c:raw}`", "quoted name"),
        ("SELECT a FROM `${c:csv}`", "quoted name"),
    ] {
        let (r, var) = open_custom_in(sql);
        assert!(!is_open(&var), "{sql}: {:#?}", r.items);
        let item = member_items(&r);
        assert_eq!(item.len(), 1, "{sql}: {:#?}", r.items);
        assert!(item[0].reason.contains(place), "{sql}: {}", item[0].reason);
    }
    for sql in ["SELECT a FROM t WHERE x = '[[c]]'", "SELECT a FROM t WHERE x = '${c}' -- $c [[c]]"] {
        let (r, var) = open_custom_in(sql);
        assert!(is_open(&var), "{sql}: {:#?}", r.items);
        assert!(member_items(&r).is_empty(), "{sql}: {:#?}", r.items);
    }
    // `$cc` names the variable `cc`, not `c`.
    let cc = json!({"type": "custom", "allowCustomValue": false, "name": "cc", "query": "1,2", "multi": false,
        "includeAll": false, "current": {"value": "1"}, "options": []});
    let r = import(
        vec![custom_var(json!({"allowCustomValue": true, "current": {"value": "typed"}})), cc],
        "SELECT a FROM t WHERE x = $cc",
    );
    assert!(is_open(&doc(&r)["variables"][0]), "{:#?}", r.items);
    assert!(member_items(&r).is_empty(), "{:#?}", r.items);
}

#[test]
fn the_variable_sql_and_the_annotation_sql_count_as_places() {
    // The SQL of a query variable.
    let q = query_var(json!({"allowCustomValue": true, "query": "SELECT a FROM t WHERE x = $c"}));
    let r = import(vec![custom_var(json!({"allowCustomValue": true})), q], "SELECT 1 AS a");
    assert!(!is_open(&doc(&r)["variables"][0]));
    assert!(is_open(&doc(&r)["variables"][1]));
    // The SQL of an annotation.
    let mut d = dash(vec![panel(1, "timeseries", 0)], vec![custom_var(json!({"allowCustomValue": true}))]);
    d["annotations"] = json!({"list": [{
        "datasource": ch(), "enable": true, "iconColor": "red", "name": "A",
        "target": {"datasource": ch(), "editorType": "sql", "format": 1, "queryType": "table",
                   "rawSql": "SELECT t AS time, 'x' AS text FROM \"$c\" WHERE $__timeFilter(t)", "refId": "Anno"}}]});
    let r = run(&d);
    assert!(!is_open(&doc(&r)["variables"][0]), "{:#?}", r.items);
    assert_eq!(member_items(&r).len(), 1);
}

#[test]
fn the_parity_fixture_imports_back_to_the_variables_of_the_hune_fixture() {
    // The converter of 15.14b writes `allowCustomValue` for each variable of the two kinds. The importer reads it
    // back to the same variables: an open variable stays open, and a closed variable has no member.
    let grafana = read_json(&repo_root().join("tests/fixtures/grafana/open_variables.json"));
    let hune = read_json(&repo_root().join("tests/fixtures/dashboards/open_variables.json"));
    let r = run(&grafana);
    let out = doc(&r);
    assert_eq!(out["variables"], hune["variables"]);
    assert!(member_items(&r).is_empty(), "{:#?}", r.items);
}
