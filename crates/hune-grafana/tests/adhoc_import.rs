// SPDX-License-Identifier: MIT OR Apache-2.0
//! The importer and the ad hoc variable (I39; 15.6, 4.10c): the variable stays as the kind `adhoc`, and
//! each query on its source gets a `changed` row that asks for `AND $__adhocFilters`.

mod common;

use common::*;
use hune_grafana::{Class, Fallback, ImportOptions, import_dashboard, parse_acceptance_file};
use serde_json::{Value, json};

fn adhoc(extra: Value) -> Value {
    let mut v =
        json!({"type": "adhoc", "name": "filters", "datasource": ch(), "filters": [], "hide": 0, "skipUrlSync": false});
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

/// Runs the import with the configured lists of `columns`, and accepts every item.
fn run_with(d: &Value, columns: Option<&[&str]>) -> hune_grafana::DashboardImport {
    let opts = || {
        let mut o: ImportOptions = base_options();
        if let Some(c) = columns {
            o.adhoc_columns.insert("prod".to_owned(), c.iter().map(|s| (*s).to_owned()).collect());
        }
        o
    };
    let first = import_dashboard(d, &opts());
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut o = opts();
    o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    import_dashboard(d, &o)
}

fn with_query(sql: &str) -> Vec<Value> {
    let mut p = panel(1, "table", 0);
    p["targets"] = json!([target(sql)]);
    vec![p]
}

#[test]
fn the_variable_keeps_its_kind_and_each_query_on_its_source_gets_a_changed_row() {
    let v = adhoc(
        json!({"label": "Filters", "defaultKeys": [{"text": "host", "value": "host"}], "filters": [{"key": "env", "operator": "=", "value": "prod", "condition": ""}]}),
    );
    let d = dash(with_query("SELECT count() FROM t WHERE x = 1"), vec![v]);
    let r = run_with(&d, Some(&["env", "host", "zone"]));
    let out = doc(&r);
    assert_eq!(
        out["variables"][0],
        json!({"kind": "adhoc", "name": "filters", "label": "Filters", "hide": "none", "source": "prod", "keys": ["host", "env"]})
    );
    let rows: Vec<&hune_grafana::ReportItem> =
        r.items.iter().filter(|i| i.reason.contains("$__adhocFilters")).collect();
    assert!(rows.iter().any(|i| i.class == Class::Changed && i.pointer.contains("/targets/0")), "{:#?}", r.items);
    // The stored filter of Grafana does not apply in Hune: its key joins `keys`, and a row says so.
    assert!(r.items.iter().any(|i| i.class == Class::Changed && i.pointer.ends_with("/filters")), "{:#?}", r.items);
    // The SQL is not changed: no automatic wrap.
    assert_eq!(out["sections"][0]["panels"][0]["data"]["query"]["sql"], json!(["SELECT count() FROM t WHERE x = 1"]));
}

#[test]
fn the_keys_come_from_the_configured_list_when_grafana_has_none() {
    // The Cluster Analysis example of the plugin: no key, no filter.
    let d = dash(with_query("SELECT 1"), vec![adhoc(json!({}))]);
    let r = run_with(&d, Some(&["service", "host"]));
    assert_eq!(doc(&r)["variables"][0]["keys"], json!(["service", "host"]));
    assert!(r.items.iter().any(|i| i.class == Class::Changed && i.reason.contains("adhoc_columns")), "{:#?}", r.items);
    // No list: the variable leaves the document, with the fallback of its row.
    let r = run_with(&d, None);
    assert!(r.document_bytes.is_some());
    assert!(doc(&r)["variables"].as_array().unwrap().is_empty());
    let item = r.items.iter().find(|i| i.class == Class::Unsupported).expect("an unsupported item");
    assert_eq!(item.fallback, Some(Fallback::other("the variable is removed")));
}

#[test]
fn a_key_outside_the_list_or_the_grammar_is_removed() {
    let v = adhoc(json!({"defaultKeys": [{"text": "host"}, {"text": "a.b"}, {"text": "secret"}, {"text": "host"}]}));
    let d = dash(with_query("SELECT 1"), vec![v]);
    let r = run_with(&d, Some(&["host", "a"]));
    assert_eq!(doc(&r)["variables"][0]["keys"], json!(["host"]));
    let removed: Vec<&hune_grafana::ReportItem> =
        r.items.iter().filter(|i| i.fallback == Some(Fallback::other("the key is removed"))).collect();
    assert_eq!(removed.len(), 2, "{:#?}", r.items);
}

#[test]
fn an_unmapped_source_or_another_kind_removes_the_variable() {
    let mut v = adhoc(json!({}));
    v["datasource"] = json!({"type": "grafana-clickhouse-datasource", "uid": "elsewhere"});
    let r = run_with(&dash(with_query("SELECT 1"), vec![v]), Some(&["host"]));
    assert!(doc(&r)["variables"].as_array().unwrap().is_empty());
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::other("the variable is removed"))));
    // One row for each query on the source of the variable, and one row for the variable.
    let r = run_with(
        &dash(with_query("SELECT 1"), vec![adhoc(json!({"defaultKeys": [{"text": "host"}]}))]),
        Some(&["host"]),
    );
    let rows = |r: &hune_grafana::DashboardImport| {
        r.items
            .iter()
            .filter(|i| i.reason.contains("add AND $__adhocFilters") && i.pointer.contains("/targets/"))
            .count()
    };
    assert_eq!(rows(&r), 1, "{:#?}", r.items);
    let mut two = with_query("SELECT 1");
    let mut second = panel(2, "table", 9);
    second["targets"] = json!([target("SELECT 2"), target("SELECT 3")]);
    two.push(second);
    let r = run_with(&dash(two, vec![adhoc(json!({"defaultKeys": [{"text": "host"}]}))]), Some(&["host"]));
    assert_eq!(rows(&r), 3, "{:#?}", r.items);
    // No adhoc variable: no row.
    let r = run_with(&dash(with_query("SELECT 1"), vec![]), Some(&["host"]));
    assert_eq!(rows(&r), 0);
}

#[test]
fn the_v2_forms_map_the_same_way() {
    // v2beta1: the data source is at the level of the item, with `group`.
    let v2 = json!({
        "kind": "AdhocVariable", "group": "grafana-clickhouse-datasource", "datasource": {"name": "clickhouse"},
        "spec": {"name": "filters", "baseFilters": [], "filters": [], "defaultKeys": [{"text": "host"}], "hide": "dontHide", "skipUrlSync": false, "allowCustomValue": true}
    });
    let d = json!({
        "apiVersion": "dashboard.grafana.app/v2beta1", "kind": "Dashboard", "metadata": {"name": "x"},
        "spec": {"title": "X", "variables": [v2], "elements": {}, "layout": {"kind": "GridLayout", "spec": {"items": []}}}
    });
    let r = run_with(&d, Some(&["host"]));
    if let Some(bytes) = &r.document_bytes {
        let out: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(out["variables"][0]["kind"], "adhoc");
        assert_eq!(out["variables"][0]["keys"], json!(["host"]));
    } else {
        panic!("{:#?}", r.items);
    }
}

/// Review 3, finding 2: a hostile list of 200,000 distinct keys costs one linear pass and one counted item. The read of
/// keys stops at the distinct key 65 (`MAX_ADHOC_KEYS + 1`). Before the fix, the search for a key that came before read
/// the whole list for each key, and each key above the limit gave one item.
#[test]
fn a_list_of_200000_keys_costs_one_pass_and_one_counted_item() {
    let keys: Vec<Value> = (0..200_000).map(|i| json!({"text": format!("k{i}")})).collect();
    let d = dash(with_query("SELECT 1"), vec![adhoc(json!({"defaultKeys": keys}))]);
    let opts = base_options();
    let mut best = std::time::Duration::MAX;
    let mut last = None;
    for i in 0..9 {
        let t = std::time::Instant::now();
        let r = import_dashboard(&d, &opts);
        if i >= 2 {
            best = best.min(t.elapsed());
        }
        last = Some(r);
    }
    let r = last.unwrap();
    println!("200,000 keys: the import takes {best:?} (minimum of 7 after 2 warm-ups), {} items", r.items.len());
    let counted: Vec<&hune_grafana::ReportItem> =
        r.items.iter().filter(|i| i.fallback == Some(Fallback::other("the keys are removed"))).collect();
    assert_eq!(counted.len(), 1, "{:#?}", counted);
    assert!(counted[0].reason.contains("the 199935 items after it"), "{}", counted[0].reason);
    assert_eq!(counted[0].pointer, "/templating/list/0/defaultKeys/64");
    assert!(r.items.len() <= 65, "{} items", r.items.len());
    let accepted = run_with(&d, None);
    let kept = doc(&accepted)["variables"][0]["keys"].as_array().unwrap().len();
    assert_eq!(kept, 64);
    assert!(best < std::time::Duration::from_millis(100), "{best:?}");
}

/// The read stops at the distinct key 65, and a key that came before does not count: 64 distinct keys with repeats keep
/// their order and give no counted item.
#[test]
fn repeated_keys_do_not_count_toward_the_limit() {
    let mut keys: Vec<Value> = (0..64).map(|i| json!({"text": format!("k{i}")})).collect();
    keys.extend((0..1000).map(|i| json!({"text": format!("k{}", i % 64)})));
    let d = dash(
        with_query("SELECT 1"),
        vec![adhoc(json!({"defaultKeys": keys, "filters": [{"key": "k3", "operator": "=", "value": "a"}]}))],
    );
    let r = run_with(&d, None);
    let kept: Vec<String> =
        doc(&r)["variables"][0]["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap().to_owned()).collect();
    assert_eq!(kept, (0..64).map(|i| format!("k{i}")).collect::<Vec<_>>());
    assert!(!r.items.iter().any(|i| i.fallback == Some(Fallback::other("the keys are removed"))), "{:#?}", r.items);
}
