// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the variable table of 15.6 (I01) for the v1 form.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn query_var(extra: Value) -> Value {
    let mut v = json!({
        "type": "query", "allowCustomValue": false, "name": "v", "label": "", "hide": 0, "datasource": ch(), "query": "SELECT DISTINCT a FROM t",
        "multi": false, "includeAll": false, "allValue": "", "refresh": 1, "sort": 0, "regex": "", "current": {}, "options": []
    });
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

fn var_out(v: Value) -> (DashboardImport, Value) {
    let r = run(&dash(vec![panel(1, "stat", 0)], vec![v]));
    let out = doc(&r);
    let var = out["variables"][0].clone();
    (r, var)
}

#[test]
fn query_variable_members() {
    let (r, v) = var_out(query_var(json!({
        "label": "Node", "hide": 1, "multi": true, "includeAll": true, "allValue": "'all'", "refresh": 2, "sort": 6,
        "regex": "/^v-(.*)/i", "description": "", "skipUrlSync": false, "current": {"text": ["All"], "value": ["$__all"]}
    })));
    no_items(&r);
    assert_eq!(
        v,
        json!({
            "kind": "query", "name": "v", "label": "Node", "hide": "label", "source": "prod",
            "sql": ["SELECT DISTINCT a FROM t"], "multi": true, "include_all": true, "all_value": "'all'",
            "refresh": "time_change", "sort": "alpha_ci_desc", "regex": "/^v-(.*)/i", "default": ["$__all"]
        })
    );
}

#[test]
fn hide_sort_refresh_tables() {
    for (g, v) in [(0, "none"), (1, "label"), (2, "variable")] {
        let (r, var) = var_out(query_var(json!({"hide": g})));
        no_items(&r);
        assert_eq!(var["hide"], v);
    }
    let names = [
        "none",
        "alpha_asc",
        "alpha_desc",
        "num_asc",
        "num_desc",
        "alpha_ci_asc",
        "alpha_ci_desc",
        "natural_asc",
        "natural_desc",
    ];
    for (g, v) in names.iter().enumerate() {
        let (r, var) = var_out(query_var(json!({"sort": g})));
        no_items(&r);
        assert_eq!(var["sort"], *v);
    }
    let (r, var) = var_out(query_var(json!({"sort": 9})));
    assert_eq!(var["sort"], "none");
    assert_eq!(r.items[0].class, Class::Unsupported);
    for (g, v, listed) in [(1, "load", false), (2, "time_change", false), (0, "load", true)] {
        let (r, var) = var_out(query_var(json!({"refresh": g})));
        assert_eq!(var["refresh"], v);
        assert_eq!(!r.items.is_empty(), listed);
        if listed {
            assert_eq!(r.items[0].class, Class::Changed);
        }
    }
}

#[test]
fn all_value_and_regex_rules() {
    let (r, var) = var_out(query_var(json!({"includeAll": false, "allValue": "x"})));
    no_items(&r);
    assert_eq!(var["all_value"], Value::Null);
    let (_, var) = var_out(query_var(json!({"includeAll": true, "allValue": ""})));
    assert_eq!(var["all_value"], Value::Null);
    let (r, var) = var_out(query_var(json!({"regex": "(?i)x"})));
    assert_eq!(var["regex"], Value::Null);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("null")));
    let (r, var) = var_out(query_var(json!({"regex": "/x/z"})));
    assert_eq!(var["regex"], Value::Null);
    assert_eq!(r.items.len(), 1);
    let (_, var) = var_out(query_var(json!({"regex": "plain"})));
    assert_eq!(var["regex"], "plain");
}

#[test]
fn defaults_of_query_and_custom_variables() {
    let case = |extra: Value, expected: Value, listed: bool| {
        let (r, var) = var_out(query_var(extra.clone()));
        assert_eq!(var["default"], expected, "{extra}");
        assert_eq!(!r.items.is_empty(), listed, "{extra}: {:#?}", r.items);
    };
    case(json!({"current": {"value": "a"}}), json!(["a"]), false);
    case(json!({"current": {"value": ""}}), json!([]), false);
    case(json!({"current": {}}), json!([]), false);
    case(json!({"multi": true, "current": {"value": ["a", "b"]}}), json!(["a", "b"]), false);
    case(json!({"current": {"value": ["a", "b"]}}), json!([]), true);
    case(json!({"includeAll": true, "current": {"value": "$__all"}}), json!(["$__all"]), false);
    case(json!({"includeAll": false, "current": {"value": "$__all"}}), json!([]), true);
    case(json!({"includeAll": true, "multi": true, "current": {"value": ["$__all", "a"]}}), json!(["$__all"]), true);
    // A value of a query variable can be absent from the options.
    case(json!({"current": {"value": "not-listed"}}), json!(["not-listed"]), false);
}

fn custom_var(extra: Value) -> Value {
    let mut v = json!({"type": "custom", "allowCustomValue": false, "name": "c", "query": "a,b", "multi": false, "includeAll": false, "current": {"value": "a"}, "options": []});
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

#[test]
fn custom_variables() {
    let (r, v) = var_out(custom_var(
        json!({"query": "Production : prod,Preproduction : preprod, x\\,y ,z", "current": {"value": "preprod"},
        "options": [{"text": "Production", "value": "prod"}]}),
    ));
    no_items(&r);
    assert_eq!(
        v["options"],
        json!([
        {"text": "Production", "value": "prod"}, {"text": "Preproduction", "value": "preprod"},
        {"text": "x,y", "value": "x,y"}, {"text": "z", "value": "z"}])
    );
    assert_eq!(v["default"], json!(["preprod"]));
    // A default that is not an option value gives an empty array.
    let (r, v) = var_out(custom_var(json!({"current": {"value": "zzz"}})));
    assert_eq!(v["default"], json!([]));
    assert_eq!(r.items[0].fallback, Some(Fallback::other("an empty array")));
    // With All.
    let (r, v) = var_out(custom_var(
        json!({"includeAll": true, "allValue": "1", "multi": true, "current": {"value": ["$__all"]}}),
    ));
    no_items(&r);
    assert_eq!(v["all_value"], "1");
    assert_eq!(v["default"], json!(["$__all"]));
    // Two options with one value, or no option, stop the dashboard.
    for q in ["a,a", "", " , "] {
        let r = run(&dash(vec![panel(1, "stat", 0)], vec![custom_var(json!({"query": q}))]));
        assert!(r.document_bytes.is_none(), "{q:?}");
    }
}

fn textbox(name: &str, default: &str) -> Value {
    json!({"type": "textbox", "name": name, "label": "T", "query": default, "current": {"value": default}, "options": [{"value": default}]})
}

#[test]
fn text_variables() {
    let (r, v) = var_out(textbox("t", "hello"));
    no_items(&r);
    assert_eq!(
        v,
        json!({"kind": "text", "name": "t", "label": "T", "hide": "none", "value_type": "text", "default": "hello"})
    );
    // Used in a string only: text. Used in code: number, when the default is a decimal number.
    let mut p = panel(1, "table", 0);
    p["targets"][0]["rawSql"] = json!("SELECT '${t}' FROM x LIMIT ${n}");
    let r = run(&dash(vec![p.clone()], vec![textbox("t", "abc"), textbox("n", "-12.5")]));
    let out = doc(&r);
    assert_eq!(out["variables"][0]["value_type"], "text");
    assert_eq!(out["variables"][1]["value_type"], "number");
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
    // A default that is not a number, for a variable in code, stops the dashboard (19.11).
    let r = run(&dash(vec![p.clone()], vec![textbox("t", "abc"), textbox("n", "ten")]));
    assert!(r.document_bytes.is_none());
    assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    // A removed panel does not count as a use.
    let mut gone = panel(2, "news", 4);
    gone["targets"][0]["rawSql"] = json!("SELECT ${n}");
    let r = run(&dash(vec![panel(1, "stat", 0), gone], vec![textbox("n", "ten")]));
    assert_eq!(doc(&r)["variables"][0]["value_type"], "text");
}

#[test]
fn variable_types_and_names() {
    // The kinds `constant` and `interval` map now (15.6): the file f27_interval_constant.rs holds their tests.
    // The kind `adhoc` maps now (I39): the file adhoc_import.rs holds its tests.
    for t in ["datasource", "groupby", "whatever"] {
        let r = run(&dash(vec![panel(1, "stat", 0)], vec![json!({"type": t, "name": "x"})]));
        assert!(r.document_bytes.is_none(), "{t}");
        assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    }
    for name in ["__reserved", "1abc", "has space", ""] {
        let r = run(&dash(vec![panel(1, "stat", 0)], vec![textbox(name, "x")]));
        assert!(r.document_bytes.is_none(), "{name}");
    }
    // The query variable needs a mapped data source and an SQL text.
    let mut v = query_var(json!({}));
    v["datasource"] = json!({"type": "grafana-clickhouse-datasource", "uid": "elsewhere"});
    assert!(run(&dash(vec![panel(1, "stat", 0)], vec![v])).document_bytes.is_none());
    // The SQL can come from `definition` or from an object.
    let mut v = query_var(json!({"query": "", "definition": "SELECT 7"}));
    let (_, var) = var_out(v.clone());
    assert_eq!(var["sql"], json!(["SELECT 7"]));
    v["definition"] = json!("");
    v["query"] = json!({"rawSql": "SELECT 8", "editorType": "sql", "queryType": "table", "format": 1});
    let (_, var) = var_out(v);
    assert_eq!(var["sql"], json!(["SELECT 8"]));
}

#[test]
fn descriptions_and_skip_url_sync() {
    let mut v = textbox("t", "x");
    v["description"] = json!("Some text");
    v["skipUrlSync"] = json!(true);
    let (r, _) = var_out(v);
    assert_eq!(r.items.len(), 2);
    assert!(r.items.iter().all(|i| i.class == Class::Dropped));
}

#[test]
fn variable_sql_checks() {
    // $__timeInterval is not permitted in a variable query (7.18, item 5).
    let v = query_var(json!({"query": "SELECT $__timeInterval(ts) FROM t"}));
    let r = run(&dash(vec![panel(1, "stat", 0)], vec![v]));
    assert!(r.document_bytes.is_none());
    assert!(r.items.iter().any(|i| i.reason.contains("not permitted")), "{:#?}", r.items);
    // $__timeFilter is permitted.
    let v = query_var(json!({"query": "SELECT a FROM t WHERE $__timeFilter(ts)"}));
    no_items(&run(&dash(vec![panel(1, "stat", 0)], vec![v])));
}
