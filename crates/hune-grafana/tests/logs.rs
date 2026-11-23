//! The logs panel of Grafana 12.4.2 (case I40 of 16.13; 15.6, 4.33f; decision ID-K326, items P05 and D06).
//!
//! The four options that Hune has map. An option of a new Grafana panel is metadata. Another value of an option
//! that Hune does not have is `unsupported`. The format 2 of the ClickHouse plugin becomes `logs` on a logs panel,
//! and `table` on another panel (D07). The logs panel of the plugin example "Simple ClickHouse OTel Dashboard"
//! imports. The forms v1, v2alpha1 and v2beta1.

mod common;

use common::*;
use hune_grafana::{Class, DashboardResult, Fallback, ImportOptions, import_dashboard, parse_acceptance_file};
use serde_json::{Value, json};

/// The logs panel of the plugin example (4.5.1, `opentelemetry-clickhouse.json`), with the data source of the tests.
fn otel_panel() -> Value {
    json!({
        "datasource": ch(),
        "gridPos": {"h": 8, "w": 24, "x": 0, "y": 44},
        "id": 11,
        "options": {"dedupStrategy": "none", "enableLogDetails": true, "prettifyLogMessage": false, "showCommonLabels": false,
            "showLabels": false, "showTime": true, "sortOrder": "Descending", "wrapLogMessage": false},
        "targets": [{
            "builderOptions": {"columns": [{"hint": "time", "name": "Timestamp", "type": "DateTime64(9)"},
                {"hint": "log_level", "name": "SeverityText", "type": "LowCardinality(String)"},
                {"hint": "log_message", "name": "Body", "type": "String"}],
                "database": "default", "filters": [], "limit": 1000, "meta": {"logMessageLike": "", "otelVersion": "latest"},
                "mode": "list", "orderBy": [], "queryType": "logs", "table": "otel_logs"},
            "datasource": ch(),
            "editorType": "builder",
            "format": 2,
            "pluginVersion": "4.0.6",
            "rawSql": "SELECT Timestamp as timestamp, Body as body, SeverityText as level FROM \"default\".\"otel_logs\" LIMIT 1000",
            "refId": "A"
        }],
        "title": "Trace Logs",
        "type": "logs"
    })
}

/// A logs panel with the options `options` and one query of the format 2.
fn logs(options: Value) -> Value {
    let mut p = panel(1, "logs", 0);
    p["options"] = options;
    p["targets"][0]["format"] = json!(2);
    p
}

#[test]
fn the_logs_panel_of_the_otel_example_imports() {
    let first = run_plain(&dash(vec![otel_panel()], vec![]));
    // One item: the builder form leaves, and its SQL stays (15.6). Each other member maps or is metadata.
    assert_eq!(first.items.len(), 1, "{:#?}", first.items);
    assert_eq!((first.items[0].member.as_str(), first.items[0].class), ("targets.0.editorType", Class::Changed));
    let r = run(&dash(vec![otel_panel()], vec![]));
    assert_eq!(r.result, DashboardResult::WouldWrite, "{:#?}", r.items);
    let d = doc(&r);
    let p = first_panel(&d);
    assert_eq!(p["type"], "logs");
    assert_eq!(p["title"], "Trace Logs");
    assert_eq!(p["options"], json!({"show_time": true}));
    assert_eq!(p["data"]["query"]["format"], "logs");
    assert_eq!(p["data"]["query"]["source"], "prod");
    assert_eq!(
        p["data"]["query"]["sql"],
        json!([
            "SELECT Timestamp as timestamp, Body as body, SeverityText as level FROM \"default\".\"otel_logs\" LIMIT 1000"
        ])
    );
}

#[test]
fn the_four_options_map_and_a_default_leaves_the_canonical_form() {
    let cases = [
        (json!({}), json!({})),
        (
            json!({"showTime": false, "wrapLogMessage": false, "enableLogDetails": true, "sortOrder": "Descending"}),
            json!({}),
        ),
        (
            json!({"showTime": true, "wrapLogMessage": true, "enableLogDetails": false, "sortOrder": "Ascending"}),
            json!({"show_time": true, "wrap": true, "details": false, "order": "oldest_first"}),
        ),
        (json!({"sortOrder": "Ascending"}), json!({"order": "oldest_first"})),
    ];
    for (input, want) in cases {
        let r = run_plain(&dash(vec![logs(input.clone())], vec![]));
        no_items(&r);
        assert_eq!(first_panel(&doc(&r))["options"], want, "{input}");
    }
}

#[test]
fn a_bad_value_of_a_mapped_option_is_unsupported_with_the_default() {
    for (input, member, fallback) in [
        (json!({"sortOrder": "Newest"}), "options.sortOrder", "the Grafana default of that option"),
        (json!({"showTime": "yes"}), "options.showTime", "the Grafana default of that option"),
        (json!({"enableLogDetails": 3}), "options.enableLogDetails", "the Grafana default of that option"),
    ] {
        let r = run(&dash(vec![logs(input.clone())], vec![]));
        let item = one_item(&r, member);
        assert_eq!(
            (item.class, item.fallback.clone()),
            (Class::Unsupported, Some(Fallback::other(fallback))),
            "{input}"
        );
        assert_eq!(first_panel(&doc(&r))["options"], json!({}), "{input}");
    }
}

#[test]
fn the_options_that_hune_does_not_have_are_metadata_at_the_grafana_default_and_unsupported_else() {
    let defaults = json!({"dedupStrategy": "none", "fontSize": "default", "detailsMode": "inline", "timestampResolution": "ms",
        "showLabels": false, "showCommonLabels": false, "prettifyLogMessage": false, "enableInfiniteScrolling": false,
        "showControls": false, "unwrappedColumns": false, "syntaxHighlighting": false});
    no_items(&run_plain(&dash(vec![logs(defaults)], vec![])));
    let others = [
        ("dedupStrategy", json!("exact"), "the panel shows each line: Hune has no deduplication"),
        ("dedupStrategy", json!("signature"), "the panel shows each line: Hune has no deduplication"),
        ("fontSize", json!("small"), "the panel shows the text at the default size"),
        ("detailsMode", json!("sidebar"), "the details row opens under the line (inline)"),
        ("timestampResolution", json!("ns"), "the time shows milliseconds"),
        ("showLabels", json!(true), "the panel shows no unique labels"),
        ("showCommonLabels", json!(true), "the panel shows no common labels"),
        ("prettifyLogMessage", json!(true), "the panel shows the JSON text as it is"),
        ("enableInfiniteScrolling", json!(true), "the panel shows the lines of one query"),
        ("showControls", json!(true), "the panel shows no controls"),
        ("unwrappedColumns", json!(true), "the panel shows no columns of displayed fields"),
        ("syntaxHighlighting", json!(true), "the panel shows the body in one colour"),
    ];
    for (key, value, fallback) in others {
        let r = run(&dash(vec![logs(json!({ key: value }))], vec![]));
        let item = one_item(&r, &format!("options.{key}"));
        assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::other(fallback))), "{key}");
        // The fallback keeps the panel.
        assert_eq!(first_panel(&doc(&r))["type"], "logs", "{key}");
    }
    // A member that no row names stays an unknown member.
    let r = run_plain(&dash(vec![logs(json!({"isFilterLabelActive": "x"}))], vec![]));
    assert_eq!(one_item(&r, "options.isFilterLabelActive").reason, "unknown member");
}

#[test]
fn the_format_of_a_query_of_a_logs_panel() {
    for (format, want) in [(2, "logs"), (1, "table"), (0, "time_series")] {
        let mut p = logs(json!({}));
        p["targets"][0]["format"] = json!(format);
        let r = run_plain(&dash(vec![p], vec![]));
        no_items(&r);
        assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], want, "{format}");
    }
    // The format 3 (traces) is still unsupported: the panel is removed.
    let mut p = logs(json!({}));
    p["targets"][0]["format"] = json!(3);
    let r = run_plain(&dash(vec![p], vec![]));
    let item = one_item(&r, "targets.0.format");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    // The format 2 on another panel stays `table` (D07).
    let mut t = panel(1, "table", 0);
    t["targets"][0]["format"] = json!(2);
    let r = run(&dash(vec![t], vec![]));
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table");
    assert_eq!(one_item(&r, "targets.0.format").class, Class::Changed);
}

#[test]
fn a_logs_panel_keeps_its_first_query() {
    let mut p = logs(json!({}));
    let mut second = p["targets"][0].clone();
    second["refId"] = json!("B");
    p["targets"].as_array_mut().unwrap().push(second);
    let r = run(&dash(vec![p], vec![]));
    let item = one_item(&r, "targets.1");
    assert_eq!(item.class, Class::Unsupported);
    let d = doc(&r);
    assert!(first_panel(&d)["data"].get("extra_queries").is_none());
    assert_eq!(first_panel(&d)["data"]["query"]["format"], "logs");
}

/// A v2 dashboard with one logs panel of the format 2.
fn v2_logs(form: &str) -> Value {
    let beta = form == "beta1";
    let spec = json!({"rawSql": "SELECT t, body, level FROM app.logs", "editorType": "sql", "format": 2});
    let query = if beta {
        json!({"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0", "datasource": {"name": "clickhouse"}, "spec": spec})
    } else {
        json!({"kind": "grafana-clickhouse-datasource", "spec": spec})
    };
    let mut panel_query = json!({"kind": "PanelQuery", "spec": {"query": query, "refId": "A", "hidden": false}});
    if !beta {
        panel_query["spec"]["datasource"] = ch();
    }
    let viz = if beta {
        json!({"kind": "VizConfig", "group": "logs", "version": "12.4.2", "spec": {"options": {"showTime": true, "sortOrder": "Ascending"}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    } else {
        json!({"kind": "logs", "spec": {"pluginVersion": "12.4.2", "options": {"showTime": true, "sortOrder": "Ascending"}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    };
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{form}"),
        "kind": "Dashboard",
        "metadata": {"name": "logsv2", "namespace": "default", "resourceVersion": "1"},
        "spec": {"title": "V2 logs", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": [],
            "elements": {"panel-1": {"kind": "Panel", "spec": {"id": 1, "title": "Logs", "description": "", "links": [],
                "data": {"kind": "QueryGroup", "spec": {"queries": [panel_query], "transformations": [], "queryOptions": {}}},
                "vizConfig": viz}}},
            "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {"x": 0, "y": 0,
                "width": 24, "height": 8, "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}
        }
    })
}

#[test]
fn the_v2_forms_of_a_logs_panel() {
    for form in ["alpha1", "beta1"] {
        let d = v2_logs(form);
        let first = import_dashboard(&d, &ImportOptions::with_source_map(&[("clickhouse", "prod")]));
        let mut o = ImportOptions::with_source_map(&[("clickhouse", "prod")]);
        let ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
        o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
        let r = import_dashboard(&d, &o);
        assert_eq!(r.result, DashboardResult::WouldWrite, "{form}: {:#?}", r.items);
        assert!(r.items.iter().all(|i| i.class != Class::Unsupported), "{form}: {:#?}", r.items);
        let p = first_panel(&doc(&r)).clone();
        assert_eq!(p["type"], "logs", "{form}");
        assert_eq!(p["options"], json!({"show_time": true, "order": "oldest_first"}), "{form}");
        assert_eq!(p["data"]["query"]["format"], "logs", "{form}");
    }
}
