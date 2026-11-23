// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the rows of 15.6, one table at a time (I01, I02, I10, I12). Each case builds a small
//! v1 dashboard. The v2 forms use the same code: see `export.rs` for the parity of the four forms.

mod common;

use common::*;
use hune_grafana::{Class, DashboardResult, Fallback};
use serde_json::{Value, json};

fn with(mut p: Value, key: &str, v: Value) -> Value {
    p[key] = v;
    p
}

fn stat(options: Value) -> Value {
    with(panel(1, "stat", 0), "options", options)
}

fn import_panel(p: Value) -> hune_grafana::DashboardImport {
    run(&dash(vec![p], vec![]))
}

// ------------------------------------------------------------------ dashboard members

#[test]
fn refresh_maps_to_the_next_larger_choice() {
    let cases: [(Value, Option<&str>, bool); 11] = [
        (json!(""), None, false),
        (json!(false), None, false),
        (json!("10s"), Some("10s"), false),
        (json!("1d"), Some("1d"), false),
        (json!("5s"), Some("10s"), true),
        (json!("2m"), Some("5m"), true),
        (json!("45m"), Some("1h"), true),
        (json!("500ms"), Some("10s"), true),
        (json!("3d"), Some("1d"), true),
        (json!("soon"), None, true),
        (json!("1w"), None, true),
    ];
    for (input, expected, listed) in cases {
        let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
        d["refresh"] = input.clone();
        let r = run(&d);
        let out = doc(&r);
        assert_eq!(out["time"]["refresh"], json!(expected), "refresh {input}");
        assert_eq!(!r.items.is_empty(), listed, "refresh {input}: {:#?}", r.items);
        if listed {
            let item = one_item(&r, "refresh");
            let class = if expected.is_some() { Class::Changed } else { Class::Unsupported };
            assert_eq!(item.class, class, "refresh {input}");
        }
    }
}

#[test]
fn time_texts_and_zone() {
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["time"] = json!({"from": "now-6H", "to": "2024-01-01T00:00:00Z"});
    d["timezone"] = json!("Europe/Paris");
    let r = run(&d);
    let out = doc(&r);
    // A zone name maps as it is (4.6).
    assert_eq!(out["time"], json!({"from": "now-6h", "to": "now", "timezone": "Europe/Paris", "refresh": null}));
    assert_eq!(items_with(&r, "time.from").len(), 1);
    assert_eq!(items_with(&r, "time.to").len(), 1);
    assert!(items_with(&r, "timezone").is_empty());
    d["timezone"] = json!("GMT");
    let r = run(&d);
    assert_eq!(doc(&r)["time"]["timezone"], "browser");
    assert_eq!(one_item(&r, "timezone").fallback, Some(Fallback::other("browser")));
    // Two absolute times in the wrong order.
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["time"] = json!({"from": "1731100000000", "to": "1731000000000"});
    d["timezone"] = json!("utc");
    let r = run(&d);
    assert_eq!(doc(&r)["time"]["from"], "now-6h");
    assert_eq!(doc(&r)["time"]["timezone"], "utc");
    assert_eq!(r.items.len(), 1);
    // Relative forms of the grammar and an absolute time map as they are.
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["time"] = json!({"from": "now-1d/d", "to": "now/d"});
    no_items(&run(&d));
}

#[test]
fn cursor_sync_values() {
    for (input, expected, listed) in [
        (json!(0), "off", false),
        (json!(1), "crosshair", false),
        (json!(2), "crosshair", true),
        (json!(7), "off", true),
    ] {
        let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
        d["graphTooltip"] = input.clone();
        let r = run(&d);
        assert_eq!(doc(&r)["cursor_sync"], expected, "{input}");
        assert_eq!(!r.items.is_empty(), listed, "{input}");
    }
}

#[test]
fn tags_are_unique_and_the_header_is_checked() {
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["tags"] = json!(["a", "b", "a"]);
    d["description"] = json!("text");
    let r = run(&d);
    assert_eq!(doc(&r)["tags"], json!(["a", "b"]));
    assert_eq!(one_item(&r, "tags.2").class, Class::Changed);
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["title"] = json!("");
    let r = run(&d);
    assert_eq!(r.result, DashboardResult::NotImported);
    assert!(r.document_bytes.is_none());
    let r = run_plain(&d);
    assert_eq!(r.result, DashboardResult::Blocked);
}

#[test]
fn dashboard_members_dropped_or_unsupported() {
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["timepicker"] = json!({"hidden": true});
    d["fiscalYearStartMonth"] = json!(3);
    d["weekStart"] = json!("monday");
    d["links"] = json!([{"title": "x", "url": "/x", "type": "other"}]);
    d["annotations"] = json!({"list": [{"builtIn": 1, "name": "Annotations"}, {"name": "Own"}]});
    d["editable"] = json!(true);
    d["id"] = json!(9);
    d["version"] = json!(3);
    d["schemaVersion"] = json!(41);
    d["gnetId"] = json!(null);
    d["__inputs"] = json!([]);
    let r = run(&d);
    assert_eq!(one_item(&r, "timepicker").class, Class::Dropped);
    assert_eq!(one_item(&r, "fiscalYearStartMonth").class, Class::Dropped);
    assert_eq!(one_item(&r, "weekStart").class, Class::Dropped);
    assert_eq!(one_item(&r, "links.0").class, Class::Unsupported);
    let own = items_with(&r, "annotations.list.1");
    assert!(own.len() == 1 && own.iter().all(|i| i.class == Class::Unsupported), "no data source (15.6) ");
    assert_eq!(r.items.len(), 5);
    assert!(r.metadata >= 6);
    // The Grafana defaults are metadata.
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["timepicker"] =
        json!({"refresh_intervals": ["5s", "10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"], "hidden": false});
    d["fiscalYearStartMonth"] = json!(0);
    d["weekStart"] = json!("");
    no_items(&run(&d));
}

#[test]
fn unknown_members_are_listed_and_trivial_ones_are_metadata() {
    // I02
    let mut d = dash(vec![panel(1, "stat", 0)], vec![]);
    d["zzz"] = json!(1);
    d["zzz0"] = json!(false);
    d["zzz1"] = json!({"a": [], "b": ""});
    let r = run_plain(&d);
    assert_eq!(r.items.len(), 1);
    let item = &r.items[0];
    assert_eq!(item.id, "t1:/zzz");
    assert_eq!(item.class, Class::Unsupported);
    assert_eq!(item.reason, "unknown member");
    assert_eq!(item.fallback, Some(Fallback::MemberRemoved));
    assert_eq!(item.value.as_deref(), Some("1"));
    assert!(!item.accepted);
    assert_eq!(r.result, DashboardResult::Blocked);
    assert!(r.document_bytes.is_none());
    let r = run(&d);
    assert_eq!(r.result, DashboardResult::WouldWrite);
    assert!(r.document_bytes.is_some());
}

// ------------------------------------------------------------------ layout

#[test]
fn rows_give_sections_and_the_y_rule() {
    let row = |id: i64, title: &str, y: i64, collapsed: bool, inner: Vec<Value>| json!({"id": id, "type": "row", "title": title, "collapsed": collapsed, "gridPos": {"h": 1, "w": 24, "x": 0, "y": y}, "panels": inner});
    let panels = vec![
        panel(1, "stat", 2),
        row(10, "First", 5, false, vec![]),
        panel(2, "stat", 6),
        panel(3, "stat", 11),
        row(11, "Folded", 20, true, vec![panel(4, "stat", 21), panel(5, "stat", 26)]),
        row(12, "Last", 30, false, vec![]),
        panel(6, "stat", 31),
    ];
    let r = run(&dash(panels, vec![]));
    no_items(&r);
    let out = doc(&r);
    let sections = out["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 4);
    assert_eq!(sections[0]["title"], Value::Null);
    assert_eq!(sections[0]["panels"][0]["grid"]["y"], 2);
    assert_eq!(sections[1]["title"], "First");
    assert_eq!(sections[1]["panels"][0]["grid"]["y"], 0);
    assert_eq!(sections[1]["panels"][1]["grid"]["y"], 5);
    assert_eq!(sections[2]["title"], "Folded");
    assert_eq!(sections[2]["collapsed"], true);
    assert_eq!(sections[2]["panels"][0]["grid"]["y"], 0);
    assert_eq!(sections[2]["panels"][1]["grid"]["y"], 5);
    assert_eq!(sections[3]["panels"][0]["grid"]["y"], 0);
    assert_eq!(r.panels, 6);
}

#[test]
fn a_dashboard_that_starts_with_a_row_has_no_section_without_header() {
    let row = json!({"id": 10, "type": "row", "title": "", "collapsed": true, "gridPos": {"h": 1, "w": 24, "x": 0, "y": 0}, "panels": []});
    let r = run(&dash(vec![row, panel(1, "stat", 1)], vec![]));
    let out = doc(&r);
    assert_eq!(out["sections"].as_array().unwrap().len(), 1);
    assert_eq!(out["sections"][0]["title"], Value::Null);
    assert_eq!(out["sections"][0]["collapsed"], false);
    // The empty title of the first row is the null title. The collapsed flag is changed.
    assert_eq!(one_item(&r, "panels.0.collapsed").class, Class::Changed);
}

#[test]
fn an_empty_dashboard_has_one_section() {
    let r = run(&dash(vec![], vec![]));
    let out = doc(&r);
    assert_eq!(out["sections"], json!([{"title": null, "collapsed": false, "panels": []}]));
}

#[test]
fn grid_members_are_checked() {
    let mut p = panel(1, "stat", 0);
    p["gridPos"] = json!({"h": 4, "w": 20, "x": 10, "y": 0});
    let r = run_plain(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
}

#[test]
fn panel_ids_are_unique() {
    let r = run(&dash(vec![panel(1, "stat", 0), panel(1, "stat", 4)], vec![]));
    assert_eq!(r.result, DashboardResult::NotImported);
    assert!(r.items.iter().any(|i| i.reason.contains("same id")));
}

// ------------------------------------------------------------------ panel members

#[test]
fn panel_types() {
    for t in ["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge"] {
        let r = import_panel(panel(1, t, 0));
        no_items(&r);
        assert_eq!(first_panel(&doc(&r))["type"], t);
    }
    // The news panel of Grafana stays out: the panel is removed.
    let r = import_panel(panel(1, "news", 0));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PanelRemoved));
    assert_eq!(doc(&r)["sections"][0]["panels"], json!([]));
}

#[test]
fn panel_members_dropped_and_unsupported() {
    let mut p = panel(1, "stat", 0);
    p["transparent"] = json!(true);
    p["cacheTimeout"] = json!("60");
    p["queryCachingTTL"] = json!(10);
    p["timeShift"] = json!("1h");
    p["timeFrom"] = json!("now/d");
    p["hideTimeOverride"] = json!(false);
    p["pluginVersion"] = json!("12.2.1");
    p["repeat"] = json!("env");
    p["repeatDirection"] = json!("h");
    p["maxPerRow"] = json!(2);
    p["interval"] = json!("30s");
    p["maxDataPoints"] = json!(300);
    p["description"] = json!("A panel.");
    let r = import_panel(p);
    // The member `transparent` is mapped (format 2, 4.13): it has no item.
    assert!(items_with(&r, "transparent").is_empty(), "{:#?}", r.items);
    // The time override is mapped (4.15): it has no item.
    assert!(items_with(&r, "timeShift").is_empty() && items_with(&r, "timeFrom").is_empty());
    // The variable `env` does not exist in this dashboard: the repeat is removed (15.6, 4.13a). The other two
    // repeat members have no effect then. see `tests/repeat.rs` for the rows of the repeat.
    assert_eq!(one_item(&r, "repeat").fallback, Some(Fallback::other("no repeat")));
    assert!(items_with(&r, "maxPerRow").is_empty() && items_with(&r, "repeatDirection").is_empty());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    let out = doc(&r);
    assert_eq!(first_panel(&out)["transparent"], true);
    let q = &first_panel(&out)["data"]["query"];
    assert_eq!((q["time_from"].as_str(), q["time_shift"].as_str()), (Some("now/d"), Some("1h")));
    assert!(q.get("hide_time_override").is_none());
    assert_eq!(q["min_interval"], "30s");
    assert_eq!(q["max_data_points"], 300);
    assert_eq!(first_panel(&out)["description"], "A panel.");
}

#[test]
fn transparent_forms() {
    // An absent or null member gives the Grafana default `false`. Grafana writes a boolean (or 1 and 0).
    for (input, expected) in [
        (None, false),
        (Some(json!(null)), false),
        (Some(json!(false)), false),
        (Some(json!(true)), true),
        (Some(json!(0)), false),
        (Some(json!(1)), true),
    ] {
        let mut p = panel(1, "stat", 0);
        if let Some(v) = &input {
            p["transparent"] = v.clone();
        }
        let r = import_panel(p);
        no_items(&r);
        assert_eq!(first_panel(&doc(&r))["transparent"], expected, "{input:?}");
    }
    // Another value is unsupported: the fallback is the Grafana default.
    for bad in [json!("yes"), json!(2), json!({}), json!([])] {
        let mut p = panel(1, "stat", 0);
        p["transparent"] = bad.clone();
        let r = import_panel(p);
        let item = one_item(&r, "transparent");
        assert_eq!((item.class, &item.fallback), (Class::Unsupported, &Some(Fallback::other("false"))), "{bad}");
        assert_eq!(first_panel(&doc(&r))["transparent"], false, "{bad}");
    }
}

#[test]
fn interval_forms() {
    for (input, expected, listed) in [
        (json!(""), json!(null), false),
        (json!(">1m"), json!("1m"), false),
        (json!("500ms"), json!("500ms"), false),
        (json!("1d"), json!("1d"), false),
        (json!("2d"), json!(null), true),
        (json!("1m30s"), json!(null), true),
        (json!("fast"), json!(null), true),
    ] {
        let mut p = panel(1, "stat", 0);
        p["interval"] = input.clone();
        let r = import_panel(p);
        assert_eq!(first_panel(&doc(&r))["data"]["query"]["min_interval"], expected, "{input}");
        assert_eq!(!r.items.is_empty(), listed, "{input}");
    }
}

#[test]
fn links_of_a_panel() {
    let mut p = panel(1, "stat", 0);
    p["links"] = json!([
        {"title": "Ok", "url": "https://docs.example.org/x?y=${__url_time_range}", "targetBlank": true},
        {"title": "Relative", "url": "/dashboards/d/abc?var-a=${a}"},
        {"title": "Scheme", "url": "javascript:alert(1)"},
        {"title": "Detail", "url": "/x/${__bogus}"},
        {"title": "Empty", "url": ""}
    ]);
    let r = import_panel(p);
    let out = doc(&r);
    let links = &first_panel(&out)["links"];
    assert_eq!(links.as_array().unwrap().len(), 2);
    assert_eq!(links[0]["target_blank"], true);
    assert_eq!(links[1]["target_blank"], false);
    assert_eq!(r.items.len(), 3);
    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::LinkRemoved)));
}

#[test]
fn library_mongo_and_unmapped_panels_are_removed() {
    let lib = json!({"id": 1, "gridPos": {"h": 2, "w": 4, "x": 0, "y": 0}, "libraryPanel": {"uid": "u", "name": "n"}, "title": "Lib"});
    let mongo = json!({"id": 2, "type": "table", "title": "Docs", "datasource": {"type": "haohanyang-mongodb-datasource", "uid": "m"},
        "gridPos": {"h": 2, "w": 4, "x": 4, "y": 0}, "targets": [{"datasource": {"type": "haohanyang-mongodb-datasource", "uid": "m"}, "queryText": "db.x.find()"}]});
    let unmapped =
        with(panel(3, "table", 2), "datasource", json!({"type": "grafana-clickhouse-datasource", "uid": "elsewhere"}));
    let mut unmapped = unmapped;
    unmapped["targets"][0]["datasource"] = json!({"type": "grafana-clickhouse-datasource", "uid": "elsewhere"});
    let mut prom = panel(4, "table", 4);
    prom["targets"][0]["datasource"] = json!({"type": "prometheus", "uid": "prom"});
    let r = run(&dash(vec![lib, mongo.clone(), unmapped, prom, panel(5, "stat", 6)], vec![]));
    assert_eq!(r.items.len(), 4, "{:#?}", r.items);
    let by_id = |id: i64| r.items.iter().find(|i| i.panel_id == Some(id)).unwrap();
    assert_eq!(by_id(1).class, Class::Unsupported);
    // A MongoDB data source that the source map does not name is `unsupported` (15.6).
    assert_eq!(by_id(2).class, Class::Unsupported);
    assert_eq!(by_id(2).fallback, Some(Fallback::PanelRemoved));
    assert!(by_id(2).reason.contains("source map"), "{}", by_id(2).reason);
    assert_eq!(by_id(3).class, Class::Unsupported);
    assert_eq!(by_id(4).class, Class::Unsupported);
    assert_eq!(doc(&r)["sections"][0]["panels"].as_array().unwrap().len(), 1);
    // I10: the report holds the MongoDB query text cut to 200 characters.
    let mut long = mongo;
    long["targets"][0]["queryText"] = json!("x".repeat(500));
    long["targets"][0]["collection"] = json!("docs");
    let mut options = hune_grafana::ImportOptions::with_source_map(&[("m", "events")]);
    options.source_kinds.insert("events".to_owned(), hune_grafana::SourceKind::MongoDb);
    let r = hune_grafana::import_dashboard(&dash(vec![long], vec![]), &options);
    let item = r.items.iter().find(|i| i.class == Class::Unsupported).expect("an unsupported item");
    assert!(item.truncated);
    assert_eq!(item.value.as_deref().unwrap().chars().count(), 200);
}

// ------------------------------------------------------------------ queries

#[test]
fn query_formats_and_flags() {
    let mut p = panel(1, "table", 0);
    p["targets"][0]["format"] = json!(0);
    assert_eq!(first_panel(&doc(&import_panel(p)))["data"]["query"]["format"], "time_series");
    // The format 2 (logs) becomes table with a `changed` item. The format 3 (traces) removes the panel (15.6).
    let mut p = panel(1, "table", 0);
    p["targets"][0]["format"] = json!(2);
    let r = import_panel(p);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table");
    assert_eq!(r.items[0].class, Class::Changed);
    let mut p = panel(1, "table", 0);
    p["targets"][0]["format"] = json!(3);
    let r = import_panel(p);
    assert_eq!(r.items[0].fallback, Some(Fallback::PanelRemoved));
    let mut p = panel(1, "table", 0);
    p["targets"][0].as_object_mut().unwrap().remove("format");
    let r = import_panel(p);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table");
    assert_eq!(r.items[0].class, Class::Changed);
    let mut p = panel(1, "table", 0);
    p["targets"][0]["hide"] = json!(true);
    let r = import_panel(p);
    // `hide` maps to `hide` of the query (4.15a, 15.6): no item, and the query stays in the document.
    no_items(&r);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["hide"], true);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["sql"], json!(["SELECT 1 AS a"]));
}

#[test]
fn builder_queries_keep_their_sql() {
    let mut p = panel(1, "table", 0);
    p["targets"][0]["editorType"] = json!("builder");
    p["targets"][0]["builderOptions"] = json!({"table": "t"});
    let r = import_panel(p);
    assert_eq!(one_item(&r, "targets.0.editorType").class, Class::Changed);
    assert_eq!(r.items.len(), 1);
}

#[test]
fn data_source_references() {
    // An old v1 reference is the name of the data source.
    let mut p = panel(1, "table", 0);
    p["datasource"] = json!("old-name");
    p["targets"][0]["datasource"] = json!("old-name");
    let r = import_panel(p);
    no_items(&r);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["source"], "legacy");
    // A reference to a data source variable.
    let mut p = panel(1, "table", 0);
    p["targets"][0]["datasource"] = json!({"type": "grafana-clickhouse-datasource", "uid": "${DS_CLICKHOUSE}"});
    let r = import_panel(p);
    assert_eq!(r.items.len(), 1);
    assert!(r.items[0].reason.contains("variable"));
    // The panel data source stands when the target has none.
    let mut p = panel(1, "table", 0);
    p["targets"][0].as_object_mut().unwrap().remove("datasource");
    no_items(&import_panel(p));
}

#[test]
fn placeholders_and_macros() {
    let var = json!({"type": "textbox", "name": "n", "query": "5", "current": {"value": "5"}});
    let custom =
        json!({"type": "custom", "allowCustomValue": false, "name": "env", "query": "a,b", "current": {"value": "a"}});
    let sqls = [
        (
            "SELECT $__timeInterval(ts), $__interval_s FROM t WHERE $__timeFilter(ts) AND ts > $__fromTime AND ts < $__toTime",
            true,
        ),
        ("SELECT * FROM t WHERE x = ${n} AND e = '${env}' AND y = $unknown", true),
        // The macros of F05 to F08 (the engine of section 7 reads them).
        ("SELECT * FROM t WHERE $__timeFilter_ms(ts)", true),
        ("SELECT $__timeInterval_ms(ts), $__interval_ms FROM t WHERE $__timeFilter_ms(ts)", true),
        ("SELECT * FROM t WHERE ts > $__fromTime_ms AND ts < $__toTime_ms", true),
        ("SELECT * FROM t WHERE ts > $__from AND ts < $__to", true),
        ("SELECT * FROM t WHERE $__dateFilter(d) AND $__dateTimeFilter(d, ts)", true),
        // D08: the second name of `$__dateTimeFilter` in the plugin.
        ("SELECT * FROM t WHERE $__dt(toDate(ts), ts)", true),
        ("SELECT * FROM t WHERE $__dt(ts)", false),
        ("SELECT * FROM t WHERE $__conditionalAll(e IN ($env), $env)", true),
        ("SELECT * FROM t WHERE $__range > 1", false),
        ("SELECT * FROM t WHERE x IN (${n:json})", false),
        ("SELECT * FROM t WHERE x = ${", false),
        ("SELECT * FROM t WHERE $__timeFilter()", false),
        ("SELECT * FROM t WHERE $__fromTime()", false),
        ("SELECT * FROM t; SELECT 2", false),
        ("DROP TABLE t", false),
        ("SELECT '${n:csv}', ${env:singlequote}", true),
        ("SELECT $__timeFilter(ts) -- $__bogus ${x:y} /* $__bogus */", true),
        ("", false),
    ];
    for (sql, ok) in sqls {
        let mut p = panel(1, "table", 0);
        p["targets"][0]["rawSql"] = json!(sql);
        let r = run(&dash(vec![p], vec![var.clone(), custom.clone()]));
        let kept =
            r.document_bytes.is_some() && doc(&r)["sections"][0]["panels"].as_array().is_some_and(|a| !a.is_empty());
        assert_eq!(kept, ok, "sql {sql:?}: {:#?}", r.items);
        if !ok {
            assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved)), "sql {sql:?}");
        }
    }
}

/// F09: a macro or a format that Grafana replaces inside a string, and that Hune keeps as text, is a `changed` item.
#[test]
fn text_that_grafana_replaces_in_a_string_is_listed() {
    let custom =
        json!({"type": "custom", "allowCustomValue": false, "name": "env", "query": "a,b", "current": {"value": "a"}});
    for (sql, member_count) in [
        ("SELECT '$__interval' AS i FROM t", 1),
        ("SELECT * FROM t WHERE e = '${env:date}'", 1),
        ("SELECT * FROM t WHERE e = '${env:csv}' AND ts > $__fromTime", 0),
    ] {
        let mut p = panel(1, "table", 0);
        p["targets"][0]["rawSql"] = json!(sql);
        let r = run(&dash(vec![p], vec![custom.clone()]));
        let changed: Vec<_> = r.items.iter().filter(|i| i.class == Class::Changed).collect();
        assert_eq!(changed.len(), member_count, "sql {sql:?}: {:#?}", r.items);
        // The panel stays: the text is only a difference of behaviour.
        assert!(doc(&r)["sections"][0]["panels"].as_array().is_some_and(|a| a.len() == 1), "sql {sql:?}");
        for item in changed {
            assert_eq!(item.fallback, None);
            assert!(item.reason.starts_with("SQL: line 1, column "), "{}", item.reason);
        }
    }
}

#[test]
fn sql_lines_and_carriage_returns() {
    let mut p = panel(1, "table", 0);
    p["targets"][0]["rawSql"] = json!("SELECT 1\r\n  FROM t\rWHERE 1\n\tAND 2");
    let r = import_panel(p);
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["sql"], json!(["SELECT 1", "  FROM t", "WHERE 1", "\tAND 2"]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
}

#[test]
fn double_brackets_become_braces() {
    let var =
        json!({"type": "custom", "allowCustomValue": false, "name": "env", "query": "a,b", "current": {"value": "a"}});
    let mut p = panel(1, "table", 0);
    p["targets"][0]["rawSql"] = json!("SELECT '[[env]]', [[other]] FROM t");
    let r = run(&dash(vec![p], vec![var]));
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["sql"], json!(["SELECT '${env}', [[other]] FROM t"]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
}

#[test]
fn panel_reuse() {
    let reuse = |id: i64, source: i64, with: bool, y: i64| {
        let mut p = panel(id, "stat", y);
        p["datasource"] = json!({"type": "datasource", "uid": "-- Dashboard --"});
        p["targets"] = json!([{"datasource": {"type": "datasource", "uid": "-- Dashboard --"}, "panelId": source, "withTransforms": with, "refId": "A"}]);
        p
    };
    let r = run(&dash(vec![panel(1, "timeseries", 0), reuse(2, 1, true, 4), reuse(3, 1, false, 8)], vec![]));
    no_items(&r);
    let out = doc(&r);
    assert_eq!(out["sections"][0]["panels"][1]["data"], json!({"reuse": {"panel": 1, "with_transforms": true}}));
    assert_eq!(out["sections"][0]["panels"][2]["data"], json!({"reuse": {"panel": 1, "with_transforms": false}}));
    // A reuse of a reuse, a reuse of a removed panel and a reuse of an absent panel.
    let r = run(&dash(
        vec![
            panel(1, "news", 0),
            reuse(2, 1, true, 4),
            reuse(3, 2, true, 8),
            reuse(4, 99, true, 12),
            panel(5, "stat", 16),
        ],
        vec![],
    ));
    assert_eq!(r.items.len(), 4, "{:#?}", r.items);
    assert_eq!(doc(&r)["sections"][0]["panels"].as_array().unwrap().len(), 1);
}

// ------------------------------------------------------------------ options

#[track_caller]
fn option_case(ptype: &str, options: Value, custom: Value, pointer: &str, expected: Value, class: Option<Class>) {
    let mut p = panel(1, ptype, 0);
    p["options"] = options.clone();
    p["fieldConfig"] = json!({"defaults": {"custom": custom}, "overrides": []});
    let r = import_panel(p);
    let out = doc(&r);
    assert_eq!(
        first_panel(&out)["options"].pointer(pointer),
        Some(&expected),
        "{ptype} {options} {pointer}: {:#?}",
        r.items
    );
    match class {
        None => assert!(r.items.is_empty(), "{ptype} {options}: {:#?}", r.items),
        Some(c) => assert!(r.items.iter().any(|i| i.class == c), "{ptype} {options}: {:#?}", r.items),
    }
}

#[test]
fn legend_value_tables() {
    let ts = |legend: Value| json!({"legend": legend});
    option_case(
        "timeseries",
        ts(json!({"showLegend": true, "displayMode": "table"})),
        json!({}),
        "/legend/mode",
        json!("table"),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"showLegend": true, "displayMode": "list"})),
        json!({}),
        "/legend/show",
        json!(true),
        None,
    );
    option_case("timeseries", ts(json!({"displayMode": "hidden"})), json!({}), "/legend/show", json!(false), None);
    option_case("timeseries", ts(json!({"displayMode": "hidden"})), json!({}), "/legend/mode", json!("list"), None);
    option_case(
        "timeseries",
        ts(json!({"showLegend": false, "displayMode": "table"})),
        json!({}),
        "/legend/show",
        json!(false),
        None,
    );
    option_case("timeseries", ts(json!({"placement": "right"})), json!({}), "/legend/placement", json!("right"), None);
    option_case(
        "timeseries",
        ts(json!({"calcs": ["sum", "min", "max", "mean", "last"]})),
        json!({}),
        "/legend/values",
        json!(["total", "min", "max", "mean", "last"]),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"calcs": ["lastNotNull"]})),
        json!({}),
        "/legend/values",
        json!(["last_not_null"]),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"calcs": ["last", "lastNotNull"]})),
        json!({}),
        "/legend/values",
        json!(["last", "last_not_null"]),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"calcs": ["lastNotNull", "mean", "max", "min", "range"]})),
        json!({}),
        "/legend/values",
        json!(["last_not_null", "mean", "max", "min", "range"]),
        None,
    );
    // The calculations that the Grafana-parity work adds (F20).
    option_case(
        "timeseries",
        ts(json!({"calcs": ["median", "stdDev", "variance", "range", "diff"]})),
        json!({}),
        "/legend/values",
        json!(["median", "std_dev", "variance", "range", "diff"]),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"calcs": ["diffperc", "delta", "distinctCount", "changeCount", "count"]})),
        json!({}),
        "/legend/values",
        json!(["diff_percent", "delta", "distinct_count", "change_count", "count"]),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"calcs": ["allIsZero", "max"]})),
        json!({}),
        "/legend/values",
        json!(["max"]),
        Some(Class::Unsupported),
    );
    // Two items that give one value: the second is removed.
    option_case(
        "timeseries",
        ts(json!({"calcs": ["max", "max"]})),
        json!({}),
        "/legend/values",
        json!(["max"]),
        Some(Class::Changed),
    );
    // More than 5 values: the first 5 stay.
    option_case(
        "timeseries",
        ts(json!({"calcs": ["min", "max", "mean", "median", "range", "diff", "delta"]})),
        json!({}),
        "/legend/values",
        json!(["min", "max", "mean", "median", "range"]),
        Some(Class::Unsupported),
    );
    // The sort of the legend table (F20): the title of the column, `sortDesc`.
    for (title, to) in [
        ("Name", "name"),
        ("Last *", "last_not_null"),
        ("Last", "last"),
        ("First *", "first_not_null"),
        ("First", "first"),
        ("Min", "min"),
        ("Max", "max"),
        ("Mean", "mean"),
        ("Median", "median"),
        ("Variance", "variance"),
        ("StdDev", "std_dev"),
        ("Total", "total"),
        ("Count", "count"),
        ("Range", "range"),
        ("Delta", "delta"),
        ("Difference", "diff"),
        ("Difference percent", "diff_percent"),
        ("Distinct Count", "distinct_count"),
        ("Change Count", "change_count"),
    ] {
        option_case(
            "timeseries",
            ts(json!({"displayMode": "table", "calcs": ["max"], "sortBy": title})),
            json!({}),
            "/legend/sort_by",
            json!(to),
            None,
        );
    }
    option_case(
        "timeseries",
        ts(json!({"displayMode": "table", "sortBy": "Max", "sortDesc": true})),
        json!({}),
        "/legend/sort_desc",
        json!(true),
        None,
    );
    option_case(
        "timeseries",
        ts(json!({"displayMode": "table", "sortBy": "Max", "sortDesc": false})),
        json!({}),
        "/legend/sort_by",
        json!("max"),
        None,
    );
    option_case(
        "piechart",
        json!({"legend": {"displayMode": "table", "values": ["value"], "sortBy": "Value", "sortDesc": true}}),
        json!({}),
        "/legend/sort_by",
        json!("value"),
        None,
    );
    option_case(
        "piechart",
        json!({"legend": {"sortBy": "Percent"}}),
        json!({}),
        "/legend/sort_by",
        json!("percent"),
        None,
    );
    // A column that the legend of Hune has not: the item says so, and the legend has no sort.
    for (ptype, legend) in [
        ("timeseries", json!({"sortBy": "Step"})),
        ("timeseries", json!({"sortBy": "Value"})),
        ("piechart", json!({"sortBy": "Max"})),
        ("timeseries", json!({"sortBy": 3})),
    ] {
        let mut p = panel(1, ptype, 0);
        p["options"] = json!({"legend": legend});
        let r = import_panel(p);
        let out = doc(&r);
        assert_eq!(first_panel(&out)["options"].pointer("/legend/sort_by"), None);
        assert_eq!(r.items.len(), 1, "{legend}: {:#?}", r.items);
        assert_eq!(r.items[0].member, "options.legend.sortBy");
        assert_eq!(r.items[0].fallback, Some(Fallback::MemberRemoved));
    }
    let mut p = panel(1, "timeseries", 0);
    p["options"] = json!({"legend": {"sortBy": "Max", "sortDesc": "yes"}});
    let r = import_panel(p);
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "options.legend.sortDesc");
    assert_eq!(r.items[0].fallback, Some(Fallback::MemberRemoved));
    option_case(
        "timeseries",
        ts(json!({"placement": "top"})),
        json!({}),
        "/legend/placement",
        json!("bottom"),
        Some(Class::Unsupported),
    );
    option_case(
        "piechart",
        json!({"legend": {"values": ["value", "percent"]}}),
        json!({}),
        "/legend/values",
        json!(["value", "percent"]),
        None,
    );
    option_case(
        "piechart",
        json!({"legend": {"values": ["value", "other"]}}),
        json!({}),
        "/legend/values",
        json!(["value"]),
        Some(Class::Unsupported),
    );
    option_case("piechart", json!({}), json!({}), "/legend/placement", json!("right"), None);
}

#[test]
fn tooltip_and_reduce_value_tables() {
    for (g, v) in [("single", "single"), ("multi", "multi"), ("none", "hidden")] {
        option_case("timeseries", json!({"tooltip": {"mode": g}}), json!({}), "/tooltip/mode", json!(v), None);
    }
    option_case(
        "timeseries",
        json!({"tooltip": {"mode": "weird"}}),
        json!({}),
        "/tooltip/mode",
        json!("single"),
        Some(Class::Unsupported),
    );
    for (g, v) in [
        ("lastNotNull", "last_not_null"),
        ("last", "last"),
        ("firstNotNull", "first_not_null"),
        ("first", "first"),
        ("min", "min"),
        ("max", "max"),
        ("mean", "mean"),
        ("sum", "total"),
        ("count", "count"),
    ] {
        option_case("stat", json!({"reduceOptions": {"calcs": [g]}}), json!({}), "/reduce/calc", json!(v), None);
    }
    option_case(
        "stat",
        json!({"reduceOptions": {"calcs": []}}),
        json!({}),
        "/reduce/calc",
        json!("last_not_null"),
        Some(Class::Changed),
    );
    option_case(
        "stat",
        json!({"reduceOptions": {"calcs": ["sum", "max"]}}),
        json!({}),
        "/reduce/calc",
        json!("total"),
        Some(Class::Unsupported),
    );
    // The calculations that the Grafana-parity work adds (F20).
    for (g, v) in [
        ("median", "median"),
        ("stdDev", "std_dev"),
        ("variance", "variance"),
        ("range", "range"),
        ("diff", "diff"),
        ("diffperc", "diff_percent"),
        ("delta", "delta"),
        ("distinctCount", "distinct_count"),
        ("changeCount", "change_count"),
    ] {
        option_case("stat", json!({"reduceOptions": {"calcs": [g]}}), json!({}), "/reduce/calc", json!(v), None);
    }
    option_case(
        "stat",
        json!({"reduceOptions": {"calcs": ["allValues"]}}),
        json!({}),
        "/reduce/calc",
        json!("last_not_null"),
        Some(Class::Unsupported),
    );
    // The order and the zeros of the tooltip (F20).
    for sort in ["asc", "desc"] {
        option_case(
            "timeseries",
            json!({"tooltip": {"mode": "multi", "sort": sort}}),
            json!({}),
            "/tooltip/sort",
            json!(sort),
            None,
        );
    }
    option_case(
        "barchart",
        json!({"tooltip": {"mode": "multi", "sort": "desc", "hideZeros": true}}),
        json!({}),
        "/tooltip/hide_zeros",
        json!(true),
        None,
    );
    // The default values are mapped, and the canonical form leaves them out.
    for (tooltip, class) in [
        (json!({"mode": "multi", "sort": "none", "hideZeros": false}), None),
        (json!({"mode": "multi", "sort": "up"}), Some(Class::Unsupported)),
        (json!({"mode": "multi", "hideZeros": "yes"}), Some(Class::Unsupported)),
    ] {
        let mut p = panel(1, "timeseries", 0);
        p["options"] = json!({"tooltip": tooltip});
        let r = import_panel(p);
        let out = doc(&r);
        assert_eq!(first_panel(&out)["options"]["tooltip"], json!({"mode": "multi"}), "{tooltip}");
        match class {
            None => assert!(r.items.is_empty(), "{tooltip}: {:#?}", r.items),
            Some(c) => assert!(r.items.iter().any(|i| i.class == c), "{tooltip}: {:#?}", r.items),
        }
    }
    option_case("stat", json!({"reduceOptions": {"fields": ""}}), json!({}), "/reduce/fields", json!(null), None);
    option_case(
        "stat",
        json!({"reduceOptions": {"fields": "/^Total$/"}}),
        json!({}),
        "/reduce/fields",
        json!("/^Total$/"),
        None,
    );
    option_case(
        "stat",
        json!({"reduceOptions": {"fields": "Total"}}),
        json!({}),
        "/reduce/fields",
        json!("Total"),
        None,
    );
    option_case(
        "stat",
        json!({"reduceOptions": {"fields": "(?i)x"}}),
        json!({}),
        "/reduce/fields",
        json!(null),
        Some(Class::Unsupported),
    );
    option_case("stat", json!({"reduceOptions": {"values": true}}), json!({}), "/reduce/values", json!(true), None);
    option_case(
        "bargauge",
        json!({"reduceOptions": {"values": false, "limit": 5}}),
        json!({}),
        "/reduce/values",
        json!(false),
        Some(Class::Unsupported),
    );
}

#[test]
fn timeseries_custom_value_tables() {
    for (g, v) in
        [("linear", "linear"), ("smooth", "smooth"), ("stepBefore", "step_before"), ("stepAfter", "step_after")]
    {
        option_case("timeseries", json!({}), json!({"lineInterpolation": g}), "/interpolation", json!(v), None);
    }
    option_case(
        "timeseries",
        json!({}),
        json!({"lineInterpolation": "spline"}),
        "/interpolation",
        json!("linear"),
        Some(Class::Unsupported),
    );
    option_case("timeseries", json!({}), json!({"spanNulls": true}), "/span_nulls", json!(true), None);
    option_case("timeseries", json!({}), json!({"spanNulls": false}), "/span_nulls", json!(false), None);
    // A number is the null threshold `span_nulls_ms`: `span_nulls` stays `false`.
    option_case("timeseries", json!({}), json!({"spanNulls": 3600000}), "/span_nulls", json!(false), None);
    option_case("timeseries", json!({}), json!({"spanNulls": 3600000}), "/span_nulls_ms", json!(3600000), None);
    for d in ["line", "bars", "points"] {
        option_case("timeseries", json!({}), json!({"drawStyle": d}), "/draw", json!(d), None);
    }
    option_case(
        "timeseries",
        json!({}),
        json!({"lineWidth": 3, "fillOpacity": 40, "pointSize": 9, "showPoints": "always"}),
        "/point_size",
        json!(9),
        None,
    );
    option_case("timeseries", json!({}), json!({"lineWidth": 11}), "/line_width", json!(1), Some(Class::Unsupported));
    option_case("timeseries", json!({}), json!({"stacking": {"mode": "percent"}}), "/stacking", json!("percent"), None);
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "log", "log": 10}}),
        "/y_scale",
        json!("log"),
        None,
    );
    // The base 2 maps to `log_base`.
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "log", "log": 2}}),
        "/y_scale",
        json!("log"),
        None,
    );
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "log", "log": 2}}),
        "/log_base",
        json!(2),
        None,
    );
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "log", "log": 3}}),
        "/y_scale",
        json!("log"),
        Some(Class::Unsupported),
    );
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "linear", "log": 2}}),
        "/y_scale",
        json!("linear"),
        None,
    );
    option_case(
        "timeseries",
        json!({}),
        json!({"scaleDistribution": {"type": "symlog"}}),
        "/y_scale",
        json!("linear"),
        Some(Class::Unsupported),
    );
}

#[test]
fn other_panel_option_tables() {
    option_case("stat", json!({"colorMode": "background"}), json!({}), "/color_mode", json!("background"), None);
    option_case(
        "stat",
        json!({"colorMode": "background_solid"}),
        json!({}),
        "/color_mode",
        json!("background"),
        Some(Class::Changed),
    );
    option_case(
        "stat",
        json!({"colorMode": "none", "graphMode": "none", "textMode": "value_and_name", "orientation": "vertical"}),
        json!({}),
        "/text_mode",
        json!("value_and_name"),
        None,
    );
    option_case(
        "stat",
        json!({"graphMode": "line"}),
        json!({}),
        "/graph_mode",
        json!("area"),
        Some(Class::Unsupported),
    );
    option_case("barchart", json!({"orientation": "horizontal"}), json!({}), "/orientation", json!("horizontal"), None);
    option_case("barchart", json!({"orientation": "auto"}), json!({}), "/orientation", json!("auto"), None);
    option_case("bargauge", json!({"orientation": "vertical"}), json!({}), "/orientation", json!("vertical"), None);
    option_case(
        "bargauge",
        json!({"orientation": "auto"}),
        json!({}),
        "/orientation",
        json!("horizontal"),
        Some(Class::Changed),
    );
    option_case("barchart", json!({"barWidth": 0.97, "groupWidth": 0.7}), json!({}), "/bar_width", json!(97), None);
    option_case("barchart", json!({"barWidth": 0.97, "groupWidth": 0.7}), json!({}), "/group_width", json!(70), None);
    option_case("barchart", json!({"barWidth": 1, "groupWidth": 0.1}), json!({}), "/group_width", json!(10), None);
    option_case("barchart", json!({"barWidth": 0.05}), json!({}), "/bar_width", json!(97), Some(Class::Unsupported));
    option_case(
        "barchart",
        json!({"xTickLabelRotation": -45, "showValue": "never", "stacking": "normal", "xField": "day"}),
        json!({}),
        "/x_tick_rotation",
        json!(-45),
        None,
    );
    option_case("barchart", json!({"xField": "day"}), json!({}), "/x_field", json!("day"), None);
    option_case(
        "table",
        json!({"sortBy": [{"displayName": "a", "desc": true}, {"displayName": "b"}]}),
        json!({}),
        "/sort",
        json!([{"field": "a", "desc": true}, {"field": "b", "desc": false}]),
        None,
    );
    option_case("table", json!({"showHeader": false}), json!({}), "/show_header", json!(false), None);
    option_case(
        "piechart",
        json!({"pieType": "donut", "displayLabels": ["name", "value", "percent", "name"]}),
        json!({}),
        "/labels",
        json!(["name", "value", "percent"]),
        None,
    );
    option_case(
        "xychart",
        json!({"series": [{"x": "a", "y": "b", "show": "points+lines"}]}),
        json!({"pointSize": 7, "lineWidth": 2}),
        "/draw",
        json!("points_and_lines"),
        None,
    );
    // The form of Grafana 12.4.2: a mapping, matchers byName, `custom.show`, a `pointSize` object.
    let xy12 = |mapping: &str| {
        json!({"mapping": mapping, "legend": {"showLegend": true, "displayMode": "list", "placement": "bottom"},
            "series": [{"frame": {"matcher": {"id": "byIndex", "options": 0}},
                "x": {"matcher": {"id": "byName", "options": "cpu"}},
                "y": {"matcher": {"id": "byName", "options": "requests"}}}]})
    };
    let xy12_custom = json!({"lineWidth": 2, "show": "points+lines", "pointSize": {"fixed": 7, "min": 1, "max": 100}});
    option_case("xychart", xy12("manual"), xy12_custom.clone(), "/x_field", json!("cpu"), None);
    option_case("xychart", xy12("manual"), xy12_custom.clone(), "/y_fields", json!(["requests"]), None);
    option_case("xychart", xy12("manual"), xy12_custom.clone(), "/draw", json!("points_and_lines"), None);
    option_case("xychart", xy12("manual"), xy12_custom.clone(), "/point_size", json!(7), None);
    option_case("xychart", xy12("manual"), xy12_custom.clone(), "/line_width", json!(2), None);
    // `auto`: Grafana chooses the fields and ignores the series.
    option_case("xychart", xy12("auto"), xy12_custom.clone(), "/x_field", json!(null), None);
    option_case("xychart", xy12("auto"), xy12_custom, "/y_fields", json!([]), None);
    option_case("xychart", json!({"mapping": "auto", "series": [{}]}), json!({}), "/draw", json!("points"), None);
    option_case("xychart", json!({"mapping": "other"}), json!({}), "/x_field", json!(null), Some(Class::Unsupported));
    option_case(
        "xychart",
        json!({"series": [{"x": {"matcher": {"id": "byRegexp", "options": "c.*"}}, "y": "b"}]}),
        json!({}),
        "/y_fields",
        json!(["b"]),
        Some(Class::Unsupported),
    );
    option_case(
        "xychart",
        json!({"series": [{"frame": {"matcher": {"id": "byIndex", "options": 1}}, "x": "a", "y": "b"}]}),
        json!({}),
        "/x_field",
        json!("a"),
        Some(Class::Unsupported),
    );
    option_case(
        "xychart",
        json!({}),
        json!({"pointSize": {"fixed": 5, "min": 2, "max": 100}}),
        "/point_size",
        json!(5),
        Some(Class::Unsupported),
    );
    // An xychart has no `y_scale` in format 2: a log scale is an unsupported value.
    option_case(
        "xychart",
        json!({}),
        json!({"scaleDistribution": {"type": "log", "log": 10}}),
        "/point_size",
        json!(5),
        Some(Class::Unsupported),
    );
    option_case(
        "xychart",
        json!({"series": [{"x": "a", "y": "b"}, {"x": "a", "y": "c"}]}),
        json!({}),
        "/y_fields",
        json!(["b", "c"]),
        None,
    );
    option_case(
        "bargauge",
        json!({"displayMode": "lcd", "showUnfilled": false}),
        json!({}),
        "/display_mode",
        json!("lcd"),
        None,
    );
}

#[test]
fn unknown_option_members_are_listed() {
    // `showPercentChange` = `false` is the Grafana default: metadata. (`wideLayout` = `false` turns a default off:
    // it is an item since P04 of the later options.)
    let r = import_panel(stat(json!({"colorMode": "value", "frobnicate": "on", "showPercentChange": false})));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "options.frobnicate");
    assert_eq!(r.items[0].fallback, Some(Fallback::MemberRemoved));
    // A legend member that no table names.
    let r = import_panel(with(panel(1, "timeseries", 0), "options", json!({"legend": {"sortKey": "Max"}})));
    assert_eq!(r.items[0].member, "options.legend.sortKey");
}

#[test]
fn a_hostile_grid_position_at_the_end_of_the_integers_is_unsupported_and_never_overflows() {
    // Review of 2026-11-14, finding F4: `y + 1` and `v - base` overflowed.
    let row = |y: i64| json!({"id": 10, "type": "row", "title": "R", "collapsed": false, "gridPos": {"h": 1, "w": 24, "x": 0, "y": y}, "panels": []});
    let r = run_plain(&dash(vec![row(i64::MAX), panel(1, "stat", 1)], vec![]));
    assert!(r.document_bytes.is_none());
    assert!(r.items.iter().any(|i| i.class == Class::Unsupported));
    // A row at the lowest value: the base is near the lowest integer, and a panel at the highest overflows `v - base`.
    let r = run_plain(&dash(vec![row(i64::MIN), panel(1, "stat", i64::MAX)], vec![]));
    assert!(r.items.iter().any(|i| i.class == Class::Unsupported));
    let mut inner = panel(2, "stat", i64::MAX);
    inner["gridPos"]["y"] = json!(i64::MAX);
    let folded = json!({"id": 11, "type": "row", "title": "F", "collapsed": true, "gridPos": {"h": 1, "w": 24, "x": 0, "y": i64::MIN}, "panels": [inner]});
    let r = run_plain(&dash(vec![folded], vec![]));
    assert!(r.items.iter().any(|i| i.class == Class::Unsupported));
}
