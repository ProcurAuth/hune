// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the members that only the v2 form has (I01): layouts, elements, variable names,
//! data source references and the transformation items.

mod common;

use common::*;
use hune_grafana::{Class, Fallback, Form, ImportOptions, import_all, import_dashboard};
use serde_json::{Value, json};

fn query(form: &str, sql: &str) -> Value {
    let spec = json!({"rawSql": sql, "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"});
    if form == "alpha1" {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "grafana-clickhouse-datasource", "spec": spec},
            "datasource": {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"}, "refId": "A", "hidden": false}})
    } else {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0",
            "datasource": {"name": "clickhouse"}, "spec": spec}, "refId": "A", "hidden": false}})
    }
}

fn element(form: &str, id: i64, vtype: &str) -> Value {
    let viz = if form == "alpha1" {
        json!({"kind": vtype, "spec": {"pluginVersion": "12.2.1", "options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    } else {
        json!({"kind": "VizConfig", "group": vtype, "version": "12.2.1", "spec": {"options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    };
    json!({"kind": "Panel", "spec": {"id": id, "title": format!("P{id}"), "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [query(form, "SELECT 1")], "transformations": [], "queryOptions": {}}}, "vizConfig": viz}})
}

fn item(name: &str, y: i64) -> Value {
    json!({"kind": "GridLayoutItem", "spec": {"x": 0, "y": y, "width": 12, "height": 4, "element": {"kind": "ElementReference", "name": name}}})
}

fn row(title: &str, hide_header: bool, collapse: bool, items: Vec<Value>) -> Value {
    json!({"kind": "RowsLayoutRow", "spec": {"title": title, "collapse": collapse, "hideHeader": hide_header,
        "layout": {"kind": "GridLayout", "spec": {"items": items}}}})
}

fn v2(form: &str, elements: Value, layout: Value, variables: Vec<Value>) -> Value {
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{}", if form == "alpha1" { "alpha1" } else { "beta1" }),
        "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "V2 test", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": variables, "elements": elements, "layout": layout}
    })
}

fn rows(rows: Vec<Value>) -> Value {
    json!({"kind": "RowsLayout", "spec": {"rows": rows}})
}

const FORMS: [&str; 2] = ["alpha1", "beta1"];

#[test]
fn rows_and_headers() {
    for f in FORMS {
        let els = json!({"panel-1": element(f, 1, "stat"), "panel-2": element(f, 2, "stat"), "panel-3": element(f, 3, "stat")});
        let layout = rows(vec![
            row("", true, false, vec![item("panel-1", 3)]),
            row("Two", false, true, vec![item("panel-2", 0)]),
            row("", false, false, vec![item("panel-3", 2)]),
        ]);
        let r = run(&v2(f, els, layout, vec![]));
        let out = doc(&r);
        let s = out["sections"].as_array().unwrap();
        assert_eq!(s[0]["title"], Value::Null, "{f}");
        assert_eq!(s[0]["panels"][0]["grid"]["y"], 3);
        assert_eq!(s[1]["title"], "Two");
        assert_eq!(s[1]["collapsed"], true);
        assert_eq!(s[2]["title"], "Row");
        assert_eq!(r.items.len(), 1, "{f}: {:#?}", r.items);
        assert_eq!(r.items[0].class, Class::Changed);
        assert_eq!(r.form, Some(if f == "alpha1" { Form::V2Alpha1 } else { Form::V2Beta1 }));
    }
}

#[test]
fn hide_header_on_a_later_row_is_unsupported() {
    let f = "beta1";
    let els = json!({"panel-1": element(f, 1, "stat"), "panel-2": element(f, 2, "stat")});
    let layout =
        rows(vec![row("A", false, false, vec![item("panel-1", 0)]), row("B", true, false, vec![item("panel-2", 0)])]);
    let r = run(&v2(f, els, layout, vec![]));
    assert_eq!(doc(&r)["sections"][1]["title"], "B");
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("the header shows")));
    assert_eq!(r.items[0].pointer, "/spec/layout/spec/rows/1/spec/hideHeader");
}

#[test]
fn grid_layout_at_the_top() {
    let f = "alpha1";
    let layout = json!({"kind": "GridLayout", "spec": {"items": [item("panel-1", 0)]}});
    let r = run(&v2(f, json!({"panel-1": element(f, 1, "table")}), layout, vec![]));
    no_items(&r);
    assert_eq!(doc(&r)["sections"][0]["title"], Value::Null);
}

#[test]
fn other_layouts_and_missing_elements() {
    let f = "beta1";
    for kind in ["TabsLayout", "AutoGridLayout"] {
        let layout = json!({"kind": kind, "spec": {}});
        let r = run(&v2(f, json!({}), layout, vec![]));
        assert!(r.document_bytes.is_none(), "{kind}");
        assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    }
    // A reference to an element that is absent.
    let layout = rows(vec![row("A", false, false, vec![item("panel-9", 0)])]);
    let r = run(&v2(f, json!({}), layout, vec![]));
    assert!(r.document_bytes.is_none());
    // An element that no item names is listed.
    let layout = rows(vec![row("A", false, false, vec![item("panel-1", 0)])]);
    let r = run(&v2(f, json!({"panel-1": element(f, 1, "stat"), "panel-2": element(f, 2, "stat")}), layout, vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].pointer, "/spec/elements/panel-2");
    assert_eq!(r.items[0].fallback, Some(Fallback::other("element removed")));
}

#[test]
fn row_and_item_repeat_and_fill_screen() {
    let f = "beta1";
    let mut r1 = row("A", false, false, vec![item("panel-1", 0)]);
    r1["spec"]["repeat"] = json!({"mode": "variable", "value": "env"});
    r1["spec"]["fillScreen"] = json!(false);
    let mut r2 = row("B", false, false, vec![item("panel-2", 0)]);
    r2["spec"]["fillScreen"] = json!(true);
    let els = json!({"panel-1": element(f, 1, "stat"), "panel-2": element(f, 2, "stat")});
    let r = run(&v2(f, els, rows(vec![r1, r2]), vec![]));
    // The variable `env` does not exist: the repeat of the row is removed (15.6, 4.13a).
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::other("no repeat"))));
    assert!(r.items.iter().any(|i| i.reason == "unknown member" && i.pointer.ends_with("fillScreen")));
}

#[test]
fn library_panel_and_unknown_viz() {
    let f = "alpha1";
    let els = json!({
        "panel-1": {"kind": "LibraryPanel", "spec": {"id": 1, "title": "Shared", "libraryPanel": {"uid": "u", "name": "n"}}},
        "panel-2": element(f, 2, "traces"),
        "panel-3": element(f, 3, "stat")});
    let layout = rows(vec![row("A", false, false, vec![item("panel-1", 0), item("panel-2", 4), item("panel-3", 8)])]);
    let r = run(&v2(f, els, layout, vec![]));
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PanelRemoved)));
    assert_eq!(doc(&r)["sections"][0]["panels"].as_array().unwrap().len(), 1);
}

fn qvar(form: &str, extra: Value) -> Value {
    let sql = json!({"rawSql": "SELECT a FROM t", "editorType": "sql", "queryType": "table", "format": 1});
    let mut spec = json!({"allowCustomValue": false, "name": "v", "label": "", "description": "", "hide": "dontHide", "skipUrlSync": false, "multi": false, "includeAll": false,
        "allValue": "", "refresh": "onDashboardLoad", "sort": "disabled", "regex": "", "current": {"text": "", "value": ""}, "options": []});
    if form == "alpha1" {
        spec["query"] = json!({"kind": "grafana-clickhouse-datasource", "spec": sql});
        spec["datasource"] = ch();
    } else {
        spec["query"] = json!({"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0", "datasource": {"name": "clickhouse"}, "spec": sql});
    }
    for (k, v) in extra.as_object().unwrap() {
        spec[k] = v.clone();
    }
    json!({"kind": "QueryVariable", "spec": spec})
}

#[test]
fn v2_variable_names() {
    for f in FORMS {
        let names = [
            ("disabled", "none"),
            ("alphabeticalAsc", "alpha_asc"),
            ("alphabeticalDesc", "alpha_desc"),
            ("numericalAsc", "num_asc"),
            ("numericalDesc", "num_desc"),
            ("alphabeticalCaseInsensitiveAsc", "alpha_ci_asc"),
            ("alphabeticalCaseInsensitiveDesc", "alpha_ci_desc"),
            ("naturalAsc", "natural_asc"),
            ("naturalDesc", "natural_desc"),
        ];
        for (g, v) in names {
            let r = run(&v2(f, json!({}), rows(vec![]), vec![qvar(f, json!({"sort": g}))]));
            no_items(&r);
            assert_eq!(doc(&r)["variables"][0]["sort"], v, "{f} {g}");
        }
        for (g, v, listed) in
            [("onDashboardLoad", "load", false), ("onTimeRangeChanged", "time_change", false), ("never", "load", true)]
        {
            let r = run(&v2(f, json!({}), rows(vec![]), vec![qvar(f, json!({"refresh": g}))]));
            assert_eq!(doc(&r)["variables"][0]["refresh"], v);
            assert_eq!(!r.items.is_empty(), listed);
        }
        for (g, v) in [("dontHide", "none"), ("hideLabel", "label"), ("hideVariable", "variable")] {
            let r = run(&v2(f, json!({}), rows(vec![]), vec![qvar(f, json!({"hide": g}))]));
            assert_eq!(doc(&r)["variables"][0]["hide"], v);
        }
        let r = run(&v2(f, json!({}), rows(vec![]), vec![qvar(f, json!({"sort": "weird"}))]));
        assert_eq!(r.items[0].class, Class::Unsupported);
        assert_eq!(doc(&r)["variables"][0]["sort"], "none");
    }
}

#[test]
fn v2_custom_and_text_variables() {
    let f = "beta1";
    let custom = json!({"kind": "CustomVariable", "spec": {"allowCustomValue": false, "name": "env", "label": "Env", "hide": "dontHide", "multi": false, "includeAll": false,
        "query": "Production : prod,Preproduction : preprod", "current": {"text": "Production", "value": "prod"},
        "options": [{"text": "Production", "value": "prod"}, {"text": "Preproduction", "value": "preprod"}]}});
    let text = json!({"kind": "TextVariable", "spec": {"name": "w", "hide": "hideLabel", "query": "abc", "current": {"text": "abc", "value": "abc"}}});
    let r = run(&v2(f, json!({}), rows(vec![]), vec![custom, text]));
    no_items(&r);
    let out = doc(&r);
    assert_eq!(out["variables"][0]["options"].as_array().unwrap().len(), 2);
    assert_eq!(out["variables"][0]["default"], json!(["prod"]));
    assert_eq!(
        out["variables"][1],
        json!({"kind": "text", "name": "w", "label": null, "hide": "label", "value_type": "text", "default": "abc"})
    );
    // A v2 custom variable with options and no query text.
    let custom = json!({"kind": "CustomVariable", "spec": {"allowCustomValue": false, "name": "env", "options": [{"text": "A", "value": "a"}], "current": {"value": "a"}}});
    let r = run(&v2(f, json!({}), rows(vec![]), vec![custom]));
    assert_eq!(doc(&r)["variables"][0]["options"], json!([{"text": "A", "value": "a"}]));
    // Other kinds.
    for kind in ["DatasourceVariable", "AdHocVariable", "GroupByVariable"] {
        let r = run(&v2(f, json!({}), rows(vec![]), vec![json!({"kind": kind, "spec": {"name": "x"}})]));
        assert!(r.document_bytes.is_none(), "{kind}");
    }
}

#[test]
fn v2_transformation_items_and_options() {
    for f in FORMS {
        let mut el = element(f, 1, "table");
        el["spec"]["data"]["spec"]["transformations"] = json!([
            {"kind": "filterByValue", "spec": {"id": "filterByValue", "options": {"type": "exclude", "match": "any", "filters": [
                {"fieldName": "a", "config": {"id": "isNull", "options": {}}}]}}},
            {"kind": "groupingToMatrix", "spec": {"id": "groupingToMatrix", "disabled": true, "options": {"columnField": "c", "rowField": "r", "valueField": "v"}}},
            {"kind": "seriesToRows", "spec": {"id": "seriesToRows", "options": {}}}]);
        el["spec"]["data"]["spec"]["queryOptions"] =
            json!({"interval": "30s", "maxDataPoints": 200, "cacheTimeout": "5", "timeFrom": "now"});
        el["spec"]["transparent"] = json!(true);
        let layout = rows(vec![row("A", false, false, vec![item("panel-1", 0)])]);
        let r = run(&v2(f, json!({"panel-1": el}), layout, vec![]));
        let out = doc(&r);
        let p = first_panel(&out);
        assert_eq!(p["transforms"].as_array().unwrap().len(), 1);
        assert_eq!(p["data"]["query"]["min_interval"], "30s");
        assert_eq!(p["data"]["query"]["max_data_points"], 200);
        assert_eq!(r.items.len(), 3, "{f}: {:#?}", r.items);
        assert_eq!(p["transparent"], true, "{f}");
        assert!(r.items.iter().all(|i| i.member != "transparent"), "{f}");
        assert!(r.items.iter().any(|i| i.member == "queryOptions.timeFrom"));
        assert!(r.items.iter().any(|i| i.member == "transformations.1.spec.disabled"));
    }
}

#[test]
fn v2_reuse_and_mongo() {
    let f = "beta1";
    let mut reuse = element(f, 2, "stat");
    reuse["spec"]["data"]["spec"]["queries"] = json!([{"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "datasource", "version": "v0",
        "datasource": {"name": "-- Dashboard --"}, "spec": {"panelId": 1, "withTransforms": true}}, "refId": "A", "hidden": false}}]);
    let mut mongo = element(f, 3, "table");
    mongo["spec"]["data"]["spec"]["queries"] = json!([{"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "haohanyang-mongodb-datasource",
        "version": "v0", "datasource": {"name": "mongo1"}, "spec": {"queryText": "db.x.find()"}}, "refId": "A", "hidden": true}}]);
    let layout = rows(vec![row("A", false, false, vec![item("panel-1", 0), item("panel-2", 4), item("panel-3", 8)])]);
    let r = run(&v2(
        f,
        json!({"panel-1": element(f, 1, "timeseries"), "panel-2": reuse, "panel-3": mongo}),
        layout,
        vec![],
    ));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    // The MongoDB data source `mongo1` is not in the source map: `unsupported`, panel removed (15.6).
    assert_eq!(r.items[0].class, Class::Unsupported);
    let out = doc(&r);
    assert_eq!(out["sections"][0]["panels"][1]["data"], json!({"reuse": {"panel": 1, "with_transforms": true}}));
}

#[test]
fn v2_header_members() {
    let f = "alpha1";
    let mut d = v2(f, json!({}), rows(vec![]), vec![]);
    d["spec"]["timeSettings"] = json!({"from": "now-1h", "to": "now", "timezone": "browser", "autoRefresh": "2m",
        "autoRefreshIntervals": ["5s"], "hideTimepicker": true, "fiscalYearStartMonth": 2, "weekStart": "", "nowDelay": "1m"});
    d["spec"]["cursorSync"] = json!("Crosshair");
    d["spec"]["links"] = json!([{"title": "x", "url": "/x"}]);
    d["spec"]["annotations"] = json!([{"kind": "AnnotationQuery", "spec": {"builtIn": true}}, {"kind": "AnnotationQuery", "spec": {"name": "own"}}]);
    d["spec"]["editable"] = json!(true);
    d["spec"]["liveNow"] = json!(false);
    d["spec"]["preload"] = json!(false);
    d["status"] = json!({});
    d["metadata"]["labels"] = json!({"a": "b"});
    let r = run(&d);
    let out = doc(&r);
    assert_eq!(out["time"], json!({"from": "now-1h", "to": "now", "timezone": "browser", "refresh": "5m"}));
    assert_eq!(out["cursor_sync"], "crosshair");
    // autoRefreshIntervals, hideTimepicker, fiscalYearStartMonth, nowDelay, links, annotation, refresh.
    assert_eq!(r.items.len(), 7, "{:#?}", r.items);
}

#[test]
fn input_errors_of_the_forms() {
    use common::*;
    let input = |value: Value, stem: &str| {
        let mut i = load_input();
        i.dashboards = vec![hune_grafana::DashboardFile {
            name: format!("dashboards/{stem}.json"),
            stem: stem.to_owned(),
            sha256: None,
            value,
        }];
        i
    };
    let o = options("");
    let good = v2("beta1", json!({}), rows(vec![]), vec![]);
    assert!(import_all(&input(good.clone(), "t2"), &o).is_ok());
    // The stem differs from the uid.
    assert!(import_all(&input(good.clone(), "other"), &o).unwrap_err().message.contains("stem"));
    // The stable `v2` of Grafana 13 is read as `v2beta1`, with the form name `v2beta1` in the report.
    let mut stable = good.clone();
    stable["apiVersion"] = json!("dashboard.grafana.app/v2");
    let r = import_all(&input(stable, "t2"), &o).expect("v2 is read");
    assert_eq!(r.files[0].form, "v2beta1");
    // Another apiVersion.
    let mut bad = good.clone();
    bad["apiVersion"] = json!("dashboard.grafana.app/v0alpha1");
    assert!(import_all(&input(bad, "t2"), &o).is_err());
    // A file that fits no row.
    assert!(import_all(&input(json!({"hello": 1}), "t2"), &o).is_err());
    // A raw v1 file needs search.json.
    let mut i = input(dash(vec![], vec![]), "t1");
    i.search = None;
    assert!(import_all(&i, &o).unwrap_err().message.contains("search.json"));
    // No dashboard.
    let mut i = load_input();
    i.dashboards.clear();
    assert!(import_all(&i, &o).is_err());
    // Two files with one uid.
    let mut i = input(good.clone(), "t2");
    let twin = hune_grafana::DashboardFile {
        name: "dashboards/t2b.json".into(),
        stem: "t2".into(),
        sha256: None,
        value: good,
    };
    i.dashboards.push(twin);
    assert!(import_all(&i, &o).is_err());
}

/// The panels of a synthetic v2 export (the `Stream` fixture, with the structure of a real export): the base step
/// of the thresholds has the value 0, `justifyMode` and the matcher `scope` are mapped, the `hideSeriesFrom`
/// override has the matcher `byNames`, and two panels are transparent. No item stays: the export imports with no
/// acceptance.
#[test]
fn stream_sample_v2() {
    let path = repo_root().join("tests/fixtures/grafana/stream/stream-sample.json");
    let input = read_json(&path);
    let r = import_dashboard(&input, &ImportOptions::with_source_map(&[("ClickHouse", "prod")]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    let out = doc(&r);
    let panels = &out["sections"][0]["panels"];
    assert_eq!(panels.as_array().map(Vec::len), Some(3));
    for p in panels.as_array().unwrap() {
        assert_eq!(p["fields"]["defaults"]["thresholds"]["steps"][0]["value"], Value::Null, "panel {}", p["id"]);
    }
    let panel = |id: i64| panels.as_array().unwrap().iter().find(|p| p["id"] == id).unwrap();
    // The stat is centred.
    assert_eq!(panel(23)["options"]["justify"], "center");
    // The matcher `scope` series gives no member, and the width stays.
    let widths = panel(10)["fields"]["overrides"].as_array().unwrap();
    assert_eq!(widths[1], json!({"match": {"by": "name", "value": "users"}, "set": {"width": 80}}));
    // The override `hideSeriesFrom` gives the matcher `names` and the property `hide_from`.
    assert_eq!(
        panel(40)["fields"]["overrides"][0],
        json!({"match": {"by": "names", "names": ["eu p99", "eu p50"], "mode": "exclude"},
               "set": {"hide_from": {"legend": false, "tooltip": true, "viz": true}}})
    );
    // The transparent flag maps to the member `transparent` on each of the two transparent panels (15.6): no item.
    let flags: Vec<&Value> = panels.as_array().unwrap().iter().map(|p| &p["transparent"]).collect();
    assert_eq!(flags.iter().filter(|v| **v == &Value::Bool(true)).count(), 2, "{flags:?}");
}

#[test]
fn the_member_allow_custom_value_is_read_in_the_spec_of_a_variable() {
    // 15.6: the member of a `QueryVariable` and of a `CustomVariable`, in the two v2 forms.
    let custom = |name: &str, member: Option<bool>| {
        let mut spec = json!({"name": name, "query": "a,b", "multi": false, "includeAll": false, "current": {"value": "a"},
            "options": [{"text": "a", "value": "a"}, {"text": "b", "value": "b"}]});
        if let Some(m) = member {
            spec["allowCustomValue"] = json!(m);
        }
        json!({"kind": "CustomVariable", "spec": spec})
    };
    for f in FORMS {
        let mut q_true = qvar(f, json!({"allowCustomValue": true}));
        q_true["spec"]["name"] = json!("q1");
        let mut q_false = qvar(f, json!({"allowCustomValue": false}));
        q_false["spec"]["name"] = json!("q2");
        let mut q_absent = qvar(f, json!({}));
        q_absent["spec"].as_object_mut().unwrap().remove("allowCustomValue");
        q_absent["spec"]["name"] = json!("q3");
        let text = json!({"kind": "TextVariable", "spec": {"name": "w", "query": "x", "current": {"text": "x", "value": "x"},
            "allowCustomValue": true}});
        let vars = vec![
            q_true,
            q_false,
            q_absent,
            custom("c1", Some(true)),
            custom("c2", Some(false)),
            custom("c3", None),
            text,
        ];
        let els = json!({"panel-1": element(f, 1, "stat")});
        let layout = rows(vec![row("A", false, false, vec![item("panel-1", 0)])]);
        let d = v2(f, els, layout, vars);
        let r = run(&d);
        let out = doc(&r);
        let open: Vec<bool> = out["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.get("allow_custom_value") == Some(&json!(true)))
            .collect();
        assert_eq!(open, [true, false, true, true, false, false, false], "{f}");
        // One `changed` item for the absent member of `c3`. The text variable and the others have none.
        let items: Vec<_> = r.items.iter().filter(|i| i.member.ends_with("allowCustomValue")).collect();
        assert_eq!(items.len(), 1, "{f}: {:#?}", r.items);
        assert_eq!(items[0].class, Class::Changed);
        assert_eq!(items[0].pointer, "/spec/variables/5/spec/allowCustomValue");
    }
}
