// SPDX-License-Identifier: MIT OR Apache-2.0
//! The rows of the repeat in 15.6 (4.13a): one test for each row in the v1 form and in the v2 forms, and
//! separate tests for the default direction, `maxPerRow` with `v`, the shared row, the nested repeat and the
//! stored copy with `repeatPanelId`.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, DashboardResult, Fallback};
use serde_json::{Value, json};

// ------------------------------------------------------------------ helpers

fn host_var(multi: bool) -> Value {
    json!({"type": "custom", "allowCustomValue": false, "name": "host", "query": "a,b,c", "multi": multi, "includeAll": true, "current": {"value": ["a"]}, "options": []})
}

fn region_var() -> Value {
    json!({
        "type": "query", "allowCustomValue": false, "name": "region", "label": "", "hide": 0, "datasource": ch(), "query": "SELECT DISTINCT r FROM t",
        "multi": true, "includeAll": true, "allValue": "", "refresh": 1, "sort": 0, "regex": "", "current": {}, "options": []
    })
}

fn account_var() -> Value {
    json!({"type": "textbox", "name": "account", "query": "", "current": {"value": ""}})
}

fn with(mut p: Value, members: Value) -> Value {
    for (k, v) in members.as_object().unwrap() {
        p[k] = v.clone();
    }
    p
}

fn panel_at(id: i64, x: i64, y: i64, w: i64, h: i64) -> Value {
    let mut p = panel(id, "stat", y);
    p["gridPos"] = json!({"x": x, "y": y, "w": w, "h": h});
    p
}

fn row_v1(id: i64, title: &str, y: i64, members: Value) -> Value {
    with(
        json!({"id": id, "type": "row", "title": title, "collapsed": false, "gridPos": {"h": 1, "w": 24, "x": 0, "y": y}, "panels": []}),
        members,
    )
}

fn run_with(panels: Vec<Value>, variables: Vec<Value>) -> (DashboardImport, Value) {
    let r = run(&dash(panels, variables));
    let d = doc(&r);
    (r, d)
}

fn fallback_of<'a>(r: &'a DashboardImport, member: &str) -> Option<&'a Fallback> {
    items_with(r, member).first().and_then(|i| i.fallback.as_ref())
}

fn no_repeat() -> Fallback {
    Fallback::other("no repeat")
}

// the v2 forms

fn element(id: i64) -> Value {
    json!({"kind": "Panel", "spec": {"id": id, "title": format!("P{id}"), "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [{"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery",
            "group": "grafana-clickhouse-datasource", "version": "v0", "datasource": {"name": "clickhouse"},
            "spec": {"rawSql": "SELECT 1", "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"}},
            "refId": "A", "hidden": false}}], "transformations": [], "queryOptions": {}}},
        "vizConfig": {"kind": "VizConfig", "group": "stat", "version": "12.2.1", "spec": {"options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}}}})
}

fn grid_item(name: &str, x: i64, y: i64, w: i64, h: i64, repeat: Option<Value>) -> Value {
    let mut spec =
        json!({"x": x, "y": y, "width": w, "height": h, "element": {"kind": "ElementReference", "name": name}});
    if let Some(r) = repeat {
        spec["repeat"] = r;
    }
    json!({"kind": "GridLayoutItem", "spec": spec})
}

fn v2_row(title: &str, hide_header: bool, repeat: Option<Value>, items: Vec<Value>) -> Value {
    let mut spec = json!({"title": title, "collapse": false, "hideHeader": hide_header,
        "layout": {"kind": "GridLayout", "spec": {"items": items}}});
    if let Some(r) = repeat {
        spec["repeat"] = r;
    }
    json!({"kind": "RowsLayoutRow", "spec": spec})
}

fn v2_dash(form: &str, panels: &[i64], rows: Vec<Value>, variables: Vec<Value>) -> Value {
    let mut elements = serde_json::Map::new();
    for id in panels {
        elements.insert(format!("panel-{id}"), element(*id));
    }
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{form}"), "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "V2 test", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": variables, "elements": Value::Object(elements),
            "layout": {"kind": "RowsLayout", "spec": {"rows": rows}}}
    })
}

fn v2_custom(name: &str, multi: bool) -> Value {
    json!({"kind": "CustomVariable", "spec": {"allowCustomValue": false, "name": name, "multi": multi, "includeAll": true, "query": "a,b,c",
        "current": {"text": "a", "value": "a"}, "options": [{"text": "a", "value": "a"}, {"text": "b", "value": "b"}]}})
}

fn run_v2(d: &Value) -> (DashboardImport, Value) {
    let r = run(d);
    let out = doc(&r);
    (r, out)
}

const V2_FORMS: [&str; 2] = ["alpha1", "beta1"];

// ------------------------------------------------------- row: RowsLayoutRow.spec.repeat

#[test]
fn a_row_repeat_maps_to_the_repeat_of_the_section_in_v1() {
    let panels = vec![row_v1(10, "Host ${host}", 0, json!({"repeat": "host"})), panel_at(1, 0, 1, 12, 4)];
    let (r, d) = run_with(panels, vec![host_var(true)]);
    no_items(&r);
    // The first row is the first section.
    assert_eq!(d["sections"][0]["repeat"], json!({"variable": "host"}));
    assert_eq!(d["sections"][0]["title"], "Host ${host}");
    assert_eq!(d["sections"].as_array().unwrap().len(), 1);
}

#[test]
fn a_row_repeat_maps_to_the_repeat_of_the_section_in_v2() {
    for f in V2_FORMS {
        let rows = vec![
            v2_row("First", false, None, vec![grid_item("panel-1", 0, 0, 12, 4, None)]),
            v2_row(
                "Host",
                false,
                Some(json!({"mode": "variable", "value": "host"})),
                vec![grid_item("panel-2", 0, 0, 12, 4, None)],
            ),
        ];
        let (r, d) = run_v2(&v2_dash(f, &[1, 2], rows, vec![v2_custom("host", true)]));
        no_items(&r);
        assert_eq!(d["sections"][1]["repeat"], json!({"variable": "host"}), "{f}");
        assert!(d["sections"][0].get("repeat").is_none(), "{f}");
    }
}

#[test]
fn a_row_repeat_needs_the_mode_variable_in_v2() {
    let rows = vec![
        v2_row("First", false, None, vec![grid_item("panel-1", 0, 0, 12, 4, None)]),
        v2_row(
            "Host",
            false,
            Some(json!({"mode": "other", "value": "host"})),
            vec![grid_item("panel-2", 0, 0, 12, 4, None)],
        ),
    ];
    let (r, d) = run_v2(&v2_dash("beta1", &[1, 2], rows, vec![v2_custom("host", true)]));
    assert!(d["sections"][1].get("repeat").is_none());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!((r.items[0].class, r.items[0].fallback.clone()), (Class::Unsupported, Some(no_repeat())));
}

#[test]
fn the_first_section_with_no_header_cannot_repeat() {
    // v2: hideHeader on the first row.
    for f in V2_FORMS {
        let rows = vec![v2_row(
            "",
            true,
            Some(json!({"mode": "variable", "value": "host"})),
            vec![grid_item("panel-1", 0, 0, 12, 4, None)],
        )];
        let (r, d) = run_v2(&v2_dash(f, &[1], rows, vec![v2_custom("host", true)]));
        assert!(d["sections"][0].get("repeat").is_none(), "{f}");
        assert_eq!(d["sections"][0]["title"], Value::Null);
        assert_eq!(r.items.len(), 1, "{f}: {:#?}", r.items);
        assert_eq!(r.items[0].fallback, Some(no_repeat()));
    }
    // v1: the first row with an empty title has no header.
    let panels = vec![row_v1(10, "", 0, json!({"repeat": "host"})), panel_at(1, 0, 1, 12, 4)];
    let (r, d) = run_with(panels, vec![host_var(true)]);
    assert!(d["sections"][0].get("repeat").is_none());
    let item = r.items.iter().find(|i| i.pointer.ends_with("/repeat")).expect("an item for the repeat");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(no_repeat())));
}

#[test]
fn the_direction_and_max_per_row_of_a_row_are_metadata() {
    let panels = vec![
        row_v1(10, "Host", 0, json!({"repeat": "host", "repeatDirection": "v", "maxPerRow": 30})),
        panel_at(1, 0, 1, 12, 4),
    ];
    let (r, d) = run_with(panels, vec![host_var(true)]);
    no_items(&r);
    assert_eq!(d["sections"][0]["repeat"], json!({"variable": "host"}));
}

// ------------------------------------------------ panel: GridLayoutItem.spec.repeat

#[test]
fn a_panel_repeat_maps_to_the_repeat_of_the_panel_in_v1() {
    let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": "h", "maxPerRow": 3}));
    let (r, d) = run_with(vec![p], vec![host_var(true)]);
    no_items(&r);
    assert_eq!(first_panel(&d)["repeat"], json!({"variable": "host", "direction": "h", "max_per_row": 3}));
    let p = with(panel_at(1, 0, 0, 8, 4), json!({"repeat": "region", "repeatDirection": "v"}));
    let (r, d) = run_with(vec![p], vec![region_var()]);
    no_items(&r);
    assert_eq!(first_panel(&d)["repeat"], json!({"variable": "region", "direction": "v"}));
}

#[test]
fn a_panel_repeat_maps_to_the_repeat_of_the_panel_in_v2() {
    for f in V2_FORMS {
        let items = vec![
            grid_item(
                "panel-1",
                0,
                0,
                24,
                4,
                Some(json!({"mode": "variable", "value": "host", "direction": "h", "maxPerRow": 3})),
            ),
            grid_item("panel-2", 0, 4, 8, 4, Some(json!({"mode": "variable", "value": "host", "direction": "v"}))),
        ];
        let (r, d) = run_v2(&v2_dash(f, &[1, 2], vec![v2_row("A", false, None, items)], vec![v2_custom("host", true)]));
        no_items(&r);
        assert_eq!(
            d["sections"][0]["panels"][0]["repeat"],
            json!({"variable": "host", "direction": "h", "max_per_row": 3}),
            "{f}"
        );
        assert_eq!(d["sections"][0]["panels"][1]["repeat"], json!({"variable": "host", "direction": "v"}), "{f}");
    }
}

#[test]
fn the_direction_values_h_and_v_map_and_another_value_removes_the_repeat() {
    for (value, expect) in [("h", Some("h")), ("v", Some("v")), ("x", None)] {
        let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": value}));
        let (r, d) = run_with(vec![p], vec![host_var(true)]);
        match expect {
            Some(dir) => {
                no_items(&r);
                assert_eq!(first_panel(&d)["repeat"]["direction"], dir);
            }
            None => {
                assert!(first_panel(&d).get("repeat").is_none(), "{value}");
                assert_eq!(fallback_of(&r, "repeatDirection"), Some(&no_repeat()), "{value}");
            }
        }
    }
}

#[test]
fn an_absent_or_null_direction_is_changed_to_h() {
    for p in [
        with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host"})),
        with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": null})),
    ] {
        let (r, d) = run_with(vec![p], vec![host_var(true)]);
        assert_eq!(first_panel(&d)["repeat"], json!({"variable": "host", "direction": "h"}));
        assert_eq!(r.items.len(), 1, "{:#?}", r.items);
        assert_eq!(r.items[0].class, Class::Changed);
        assert!(r.items[0].reason.contains("direction"), "{}", r.items[0].reason);
    }
    // The same in v2.
    let items = vec![grid_item("panel-1", 0, 0, 24, 4, Some(json!({"mode": "variable", "value": "host"})))];
    let (r, d) = run_v2(&v2_dash("beta1", &[1], vec![v2_row("A", false, None, items)], vec![v2_custom("host", true)]));
    assert_eq!(d["sections"][0]["panels"][0]["repeat"]["direction"], "h");
    assert_eq!((r.items.len(), r.items[0].class), (1, Class::Changed));
}

#[test]
fn max_per_row_maps_with_h_from_1_to_24() {
    for n in [1, 4, 24] {
        let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": "h", "maxPerRow": n}));
        let (r, d) = run_with(vec![p], vec![host_var(true)]);
        no_items(&r);
        assert_eq!(first_panel(&d)["repeat"]["max_per_row"], n);
    }
    for bad in [json!(0), json!(25), json!(-1), json!("3"), json!(2.5)] {
        let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": "h", "maxPerRow": bad}));
        let (r, d) = run_with(vec![p], vec![host_var(true)]);
        assert!(first_panel(&d)["repeat"].get("max_per_row").is_none(), "{bad}");
        assert_eq!(first_panel(&d)["repeat"]["direction"], "h");
        assert_eq!(r.items.len(), 1, "{bad}: {:#?}", r.items);
        assert_eq!(
            (r.items[0].class, r.items[0].fallback.clone()),
            (Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{bad}"
        );
    }
}

#[test]
fn max_per_row_with_v_is_metadata() {
    for value in [json!(3), json!(30), json!(0)] {
        let p = with(panel_at(1, 0, 0, 8, 4), json!({"repeat": "host", "repeatDirection": "v", "maxPerRow": value}));
        let (r, d) = run_with(vec![p], vec![host_var(true)]);
        no_items(&r);
        assert_eq!(first_panel(&d)["repeat"], json!({"variable": "host", "direction": "v"}));
    }
}

#[test]
fn a_panel_with_no_repeat_ignores_the_two_other_members() {
    let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeatDirection": "h", "maxPerRow": 3, "repeat": ""}));
    let (r, d) = run_with(vec![p], vec![host_var(true)]);
    no_items(&r);
    assert!(first_panel(&d).get("repeat").is_none());
}

// ----------------------------------------------------------- the variable

#[test]
fn the_variable_must_be_a_query_or_custom_variable_with_multi() {
    for (var, why) in [(host_var(false), "multi"), (account_var(), "multi")] {
        let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": var["name"], "repeatDirection": "h"}));
        let (r, d) = run_with(vec![p], vec![var]);
        assert!(first_panel(&d).get("repeat").is_none());
        assert_eq!(r.items.len(), 1, "{:#?}", r.items);
        assert_eq!((r.items[0].class, r.items[0].fallback.clone()), (Class::Unsupported, Some(no_repeat())));
        assert!(r.items[0].reason.contains(why), "{}", r.items[0].reason);
    }
    // An unknown name.
    let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "nothing", "repeatDirection": "h"}));
    let (r, d) = run_with(vec![p], vec![host_var(true)]);
    assert!(first_panel(&d).get("repeat").is_none());
    assert!(r.items[0].reason.contains("not a variable"), "{}", r.items[0].reason);
    // A name that is not a text.
    let p = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": 5}));
    let (r, d) = run_with(vec![p], vec![host_var(true)]);
    assert!(first_panel(&d).get("repeat").is_none());
    assert_eq!(r.items[0].fallback, Some(no_repeat()));
}

#[test]
fn a_row_with_a_variable_that_cannot_repeat_keeps_the_section_and_removes_the_repeat() {
    let panels = vec![row_v1(10, "Host", 0, json!({"repeat": "host"})), panel_at(1, 0, 1, 12, 4)];
    let (r, d) = run_with(panels, vec![host_var(false)]);
    assert!(d["sections"][0].get("repeat").is_none());
    assert_eq!(d["sections"][0]["panels"].as_array().unwrap().len(), 1);
    let item = r.items.iter().find(|i| i.pointer.ends_with("/repeat")).expect("an item for the repeat");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(no_repeat())));
}

// -------------------------------------------------- a panel with a shared row unit

#[test]
fn a_panel_with_h_that_shares_a_row_unit_loses_its_repeat() {
    let a = with(panel_at(1, 0, 0, 12, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let b = panel_at(2, 12, 0, 12, 4);
    let (r, d) = run_with(vec![a, b], vec![host_var(true)]);
    assert!(first_panel(&d).get("repeat").is_none());
    assert_eq!(d["sections"][0]["panels"].as_array().unwrap().len(), 2);
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("no repeat on the panel")));
    assert_eq!(r.items[0].panel_id, Some(1));
    assert!(r.items[0].reason.contains("shares a row unit"), "{}", r.items[0].reason);
}

#[test]
fn d7_above_200_panels_the_cap_comes_before_the_checks_across_panels() {
    // D7 (a) of the security review of 2026-11-16: the cap of 4.13 (rule 2) applies before the checks across panels.
    // Panel 1 has the direction h and shares its row unit with panel 2. The other panels have their own rows.
    let panels = |count: i64| -> Vec<Value> {
        let mut out = vec![
            with(panel_at(1, 0, 0, 12, 1), json!({"repeat": "host", "repeatDirection": "h"})),
            panel_at(2, 12, 0, 12, 1),
        ];
        out.extend((3..=count).map(|id| panel_at(id, 0, id, 24, 1)));
        out
    };
    // 200 panels: the check of the row unit runs, and the document is valid.
    let (r, d) = run_with(panels(200), vec![host_var(true)]);
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert!(r.items[0].reason.contains("shares a row unit"), "{}", r.items[0].reason);
    assert_eq!(d["sections"][0]["panels"].as_array().unwrap().len(), 200);
    // 201 panels: one item for the whole dashboard, and no check across panels.
    let r = run_plain(&dash(panels(201), vec![host_var(true)]));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    let item = &r.items[0];
    assert_eq!((item.id.as_str(), item.pointer.as_str(), item.class), ("t1:", "", Class::Unsupported));
    assert_eq!(item.reason, "the dashboard has 201 panels: a document has 200 panels at most (4.13)");
    assert_eq!(item.fallback, Some(Fallback::DashboardNotImported));
    assert_eq!(r.result, DashboardResult::Blocked);
    assert!(r.document_bytes.is_none() && r.validation_errors.is_empty());
}

#[test]
fn a_panel_with_v_can_share_rows_and_a_panel_with_h_can_touch_other_rows() {
    let a = with(panel_at(1, 0, 0, 8, 4), json!({"repeat": "host", "repeatDirection": "v"}));
    let b = panel_at(2, 8, 0, 8, 4);
    let (r, d) = run_with(vec![a, b], vec![host_var(true)]);
    no_items(&r);
    assert_eq!(first_panel(&d)["repeat"]["direction"], "v");
    // The panel with h ends at row 4 and the other panel starts at row 4.
    let a = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let b = panel_at(2, 0, 4, 12, 4);
    let (r, d) = run_with(vec![a, b], vec![host_var(true)]);
    no_items(&r);
    assert_eq!(first_panel(&d)["repeat"]["direction"], "h");
}

// ---------------------------------------------------------------- nested repeat

#[test]
fn a_panel_with_repeat_in_a_row_with_repeat_loses_its_repeat_and_the_row_keeps_it() {
    let inner = with(panel_at(1, 0, 1, 24, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let panels = vec![row_v1(10, "Host", 0, json!({"repeat": "region"})), inner];
    let (r, d) = run_with(panels, vec![host_var(true), region_var()]);
    assert_eq!(d["sections"][0]["repeat"], json!({"variable": "region"}));
    assert!(d["sections"][0]["panels"][0].get("repeat").is_none());
    let lost: Vec<_> =
        r.items.iter().filter(|i| i.fallback == Some(Fallback::other("no repeat on the panel"))).collect();
    assert_eq!(lost.len(), 1, "{:#?}", r.items);
    assert_eq!(lost[0].panel_id, Some(1));
}

#[test]
fn the_same_nested_repeat_in_v2() {
    let items =
        vec![grid_item("panel-1", 0, 0, 24, 4, Some(json!({"mode": "variable", "value": "host", "direction": "h"})))];
    let rows = vec![
        v2_row("First", false, None, vec![grid_item("panel-2", 0, 0, 12, 4, None)]),
        v2_row("Row", false, Some(json!({"mode": "variable", "value": "host"})), items),
    ];
    let (r, d) = run_v2(&v2_dash("beta1", &[1, 2], rows, vec![v2_custom("host", true)]));
    assert_eq!(d["sections"][1]["repeat"], json!({"variable": "host"}));
    assert!(d["sections"][1]["panels"][0].get("repeat").is_none());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
}

// ----------------------------------------------------------------- reuse panels

fn reuse_of(id: i64, source: i64, y: i64) -> Value {
    let mut p = panel_at(id, 0, y, 12, 4);
    p["datasource"] = json!({"type": "datasource", "uid": "-- Dashboard --"});
    p["targets"] = json!([{"datasource": {"type": "datasource", "uid": "-- Dashboard --"}, "panelId": source, "withTransforms": true, "refId": "A"}]);
    p
}

#[test]
fn a_reuse_panel_cannot_repeat() {
    let source = panel_at(1, 0, 0, 12, 4);
    let reuse = with(reuse_of(2, 1, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let (r, d) = run_with(vec![source, reuse], vec![host_var(true)]);
    assert!(d["sections"][0]["panels"][1].get("repeat").is_none());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("no repeat on the panel")));
    assert_eq!(r.items[0].panel_id, Some(2));
}

#[test]
fn a_repeated_panel_cannot_be_the_source_of_a_reuse() {
    let source = with(panel_at(1, 0, 0, 24, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let reuse = reuse_of(2, 1, 4);
    let (r, d) = run_with(vec![source, reuse], vec![host_var(true)]);
    assert!(first_panel(&d).get("repeat").is_none());
    assert_eq!(d["sections"][0]["panels"].as_array().unwrap().len(), 2);
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(r.items[0].panel_id, Some(1));
}

#[test]
fn a_repeated_row_with_a_reuse_panel_or_with_a_reuse_source_loses_its_repeat() {
    // The row holds a reuse panel.
    let panels = vec![panel_at(1, 0, 0, 12, 4), row_v1(10, "Host", 4, json!({"repeat": "host"})), reuse_of(2, 1, 5)];
    let (r, d) = run_with(panels, vec![host_var(true)]);
    assert!(d["sections"][1].get("repeat").is_none());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(r.items[0].fallback, Some(no_repeat()));
    // A reuse panel outside the row names a panel of the row.
    let panels = vec![
        row_v1(10, "Host", 0, json!({"repeat": "host"})),
        panel_at(1, 0, 1, 12, 4),
        row_v1(11, "Other", 5, json!({})),
        reuse_of(2, 1, 6),
    ];
    let (r, d) = run_with(panels, vec![host_var(true)]);
    assert!(d["sections"][1].get("repeat").is_none());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
}

// ------------------------------------------------------------------ stored copies

#[test]
fn a_panel_with_repeat_panel_id_is_a_stored_copy_and_is_removed() {
    let original = with(panel_at(1, 0, 0, 12, 4), json!({"repeat": "host", "repeatDirection": "h"}));
    let copy = with(
        panel_at(7, 12, 0, 12, 4),
        json!({"repeatPanelId": 1, "repeatIteration": 1731000000000_i64, "scopedVars": {"host": {"text": "b", "value": "b"}}}),
    );
    let (r, d) = run_with(vec![original, copy], vec![host_var(true)]);
    let panels = d["sections"][0]["panels"].as_array().unwrap();
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0]["id"], 1);
    assert_eq!(panels[0]["repeat"]["variable"], "host");
    let listed = items_with(&r, "repeatPanelId");
    assert_eq!(listed.len(), 1, "{:#?}", r.items);
    assert_eq!(listed[0].class, Class::Dropped);
    assert_eq!(listed[0].panel_id, Some(7));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
}

#[test]
fn the_metadata_members_of_a_copy_have_no_item() {
    // `repeatPanelId` null and `repeatIteration`, `repeatedByRow`, `scopedVars` on a panel with no copy.
    let p = with(
        panel_at(1, 0, 0, 12, 4),
        json!({"repeatPanelId": null, "repeatIteration": 1731000000000_i64, "repeatedByRow": true, "scopedVars": {"host": {"text": "b"}}}),
    );
    let (r, _) = run_with(vec![p], vec![host_var(true)]);
    no_items(&r);
}
