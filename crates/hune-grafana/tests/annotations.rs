//! The rows of 15.6 for the annotations (4.5b, ID-K153): the form v1 follows the exports of the reference
//! Grafana and the measure of 19.20e. The form v2 is synthetic: it follows the schema, and no real export of
//! Grafana confirms its paths (open question Q1).

mod common;

use common::*;
use hune_grafana::{Class, Fallback};
use serde_json::{Value, json};

fn ds() -> Value {
    ch()
}

fn query_item(name: &str, sql: &str) -> Value {
    json!({
        "datasource": ds(), "enable": true, "iconColor": "red", "name": name,
        "target": {"datasource": ds(), "editorType": "sql", "format": 1, "queryType": "table", "rawSql": sql, "refId": "Anno"}
    })
}

fn builtin() -> Value {
    json!({"builtIn": 1, "datasource": {"type": "grafana", "uid": "-- Grafana --"}, "enable": true, "hide": true,
        "iconColor": "rgba(0, 211, 255, 1)", "name": "Annotations & Alerts", "type": "dashboard"})
}

/// A dashboard with two time series (ids 1 and 2), a stat (id 3) and the annotation items.
fn with_items(items: Vec<Value>) -> Value {
    let mut d = dash(vec![panel(1, "timeseries", 0), panel(2, "timeseries", 4), panel(3, "stat", 8)], vec![]);
    d["annotations"] = json!({"list": items});
    d
}

fn annotations_of(d: &hune_grafana::DashboardImport) -> Value {
    doc(d)["annotations"].clone()
}

const SQL: &str = "SELECT t AS time, 'x' AS text FROM ops.deploys WHERE $__timeFilter(t)";

#[test]
fn the_builtin_item_is_metadata_and_the_document_has_no_member() {
    let r = run_plain(&with_items(vec![builtin()]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    assert!(doc(&r).get("annotations").is_none());
    // The bytes of a document with no annotation are as before: no member `annotations`.
    let r = run_plain(&with_items(vec![]));
    assert!(doc(&r).get("annotations").is_none());
}

#[test]
fn a_clickhouse_item_maps_to_an_annotation_with_the_report_warning() {
    let mut item = query_item("Deploys", SQL);
    item["enable"] = json!(false);
    let r = run(&with_items(vec![builtin(), item]));
    let a = annotations_of(&r);
    assert_eq!(a, json!([{"name": "Deploys", "source": "prod", "sql": [SQL], "color": "red", "enabled": false}]));
    // The warning of 15.6 is a `changed` item at the SQL.
    let w = one_item(&r, "annotations.list.1.target.rawSql");
    assert_eq!(w.class, Class::Changed);
    assert!(w.reason.contains("is not checked against the rules of 7.37"), "{}", w.reason);
}

#[test]
fn eight_items_map_and_the_ninth_is_removed() {
    let items: Vec<Value> = (0..9).map(|i| query_item(&format!("A{i}"), SQL)).collect();
    let r = run(&with_items(items));
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 8);
    let ninth = one_item(&r, "annotations.list.8");
    assert_eq!((ninth.class, ninth.fallback.clone()), (Class::Unsupported, Some(Fallback::other("item removed"))));
}

#[test]
fn a_name_that_repeats_is_changed_to_a_new_name() {
    let r = run(&with_items(vec![query_item("Same", SQL), query_item("Same", SQL), query_item("Same", SQL)]));
    let names: Vec<String> =
        annotations_of(&r).as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, ["Same", "Same 2", "Same 3"]);
    assert_eq!(one_item(&r, "annotations.list.1.name").class, Class::Changed);
}

#[test]
fn the_colour_maps_or_becomes_the_grey_of_grafana() {
    for (input, want, changed) in [
        (json!("red"), "red", false),
        (json!("#73bf69"), "#73bf69", false),
        (json!("rgb(1, 2, 3)"), "rgb(1, 2, 3)", false),
        (json!("semi-dark-blue"), "semi-dark-blue", false),
        (json!("not-a-colour"), "#808080", true),
        (json!(""), "#808080", true),
        (json!(5), "#808080", true),
    ] {
        let mut item = query_item("C", SQL);
        item["iconColor"] = input.clone();
        let r = run(&with_items(vec![item]));
        assert_eq!(annotations_of(&r)[0]["color"], want, "{input}");
        assert_eq!(!items_with(&r, "annotations.list.0.iconColor").is_empty(), changed, "{input}");
    }
    let mut item = query_item("C", SQL);
    item.as_object_mut().unwrap().remove("iconColor");
    let r = run(&with_items(vec![item]));
    assert_eq!(annotations_of(&r)[0]["color"], "#808080");
    assert_eq!(one_item(&r, "annotations.list.0.iconColor").class, Class::Changed);
}

#[test]
fn the_filter_maps_to_the_time_series_of_the_document() {
    // exclude false: the ids of time series stay, the others leave the list as changed items.
    let mut item = query_item("F", SQL);
    item["filter"] = json!({"exclude": false, "ids": [2, 3, 99]});
    let r = run(&with_items(vec![item]));
    assert_eq!(annotations_of(&r)[0]["panels"], json!([2]));
    assert_eq!(items_with(&r, "annotations.list.0.filter.ids").len(), 2);
    // exclude true: the list names each time series that the filter does not exclude.
    let mut item = query_item("F", SQL);
    item["filter"] = json!({"exclude": true, "ids": [1]});
    let r = run(&with_items(vec![item]));
    assert_eq!(annotations_of(&r)[0]["panels"], json!([2]));
    assert_eq!(one_item(&r, "annotations.list.0.filter").class, Class::Changed);
    // An empty list is unsupported: the item is removed.
    for filter in [
        json!({"exclude": false, "ids": []}),
        json!({"exclude": false, "ids": [3]}),
        json!({"exclude": true, "ids": [1, 2]}),
    ] {
        let mut item = query_item("F", SQL);
        item["filter"] = filter.clone();
        let r = run(&with_items(vec![item]));
        assert!(r.document_bytes.as_ref().is_some_and(|_| doc(&r).get("annotations").is_none()), "{filter}");
        let unsupported: Vec<_> =
            items_with(&r, "annotations.list.0.filter").into_iter().filter(|i| i.class == Class::Unsupported).collect();
        assert_eq!(unsupported.len(), 1, "{filter}");
    }
    // An absent filter: the member `panels` is absent.
    let r = run(&with_items(vec![query_item("F", SQL)]));
    assert!(annotations_of(&r)[0].get("panels").is_none());
}

#[test]
fn hide_and_mappings_and_other_members() {
    let mut item = query_item("H", SQL);
    item["hide"] = json!(true);
    let r = run(&with_items(vec![item]));
    assert!(items_with(&r, "annotations.list.0.hide").is_empty(), "hide true is metadata");
    let mut item = query_item("H", SQL);
    item["hide"] = json!(false);
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.hide").class, Class::Dropped);
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 1);
    // `mappings` with a member: unsupported, item removed. An empty list is metadata.
    let mut item = query_item("M", SQL);
    item["mappings"] = json!({"text": {"source": "field", "value": "note"}});
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.mappings").class, Class::Unsupported);
    assert!(doc(&r).get("annotations").is_none());
    let mut item = query_item("M", SQL);
    item["mappings"] = json!({});
    assert_eq!(annotations_of(&run(&with_items(vec![item]))).as_array().unwrap().len(), 1);
    // Another member: unsupported, member removed in the report. A trivial value is metadata.
    let mut item = query_item("O", SQL);
    item["showIn"] = json!(1);
    item["limit"] = json!(0);
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.showIn").class, Class::Unsupported);
    assert!(items_with(&r, "annotations.list.0.limit").is_empty());
}

#[test]
fn an_item_that_hune_cannot_hold_is_removed() {
    let macros = [
        "SELECT $__timeInterval(t) AS time FROM x",
        "SELECT $__timeInterval_ms(t) AS time FROM x",
        "SELECT t AS time FROM x WHERE a > $__interval_s",
        "SELECT $__interval_ms AS time",
    ];
    for sql in macros {
        let r = run(&with_items(vec![query_item("I", sql)]));
        let i = one_item(&r, "annotations.list.0.target.rawSql");
        assert_eq!((i.class, i.fallback.clone()), (Class::Unsupported, Some(Fallback::other("item removed"))), "{sql}");
        assert!(i.reason.contains("not permitted"), "{}", i.reason);
        assert!(doc(&r).get("annotations").is_none(), "{sql}");
    }
    // A data source that is not in the map, and a data source that is not a ClickHouse source.
    let mut item = query_item("P", SQL);
    item["datasource"] = json!({"type": "prometheus", "uid": "prom"});
    item["target"]["datasource"] = json!({"type": "prometheus", "uid": "prom"});
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.datasource").class, Class::Unsupported);
    let mut item = query_item("G", SQL);
    item["datasource"] = json!({"type": "grafana", "uid": "-- Grafana --"});
    item.as_object_mut().unwrap().remove("target");
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.datasource").class, Class::Unsupported);
    // A PostgreSQL data source that the map does not name.
    let mut item = query_item("Pg", SQL);
    item["datasource"] = json!({"type": "grafana-postgresql-datasource", "uid": "pg-not-in-map"});
    item["target"]["datasource"] = json!({"type": "grafana-postgresql-datasource", "uid": "pg-not-in-map"});
    let r = run(&with_items(vec![item]));
    let i = one_item(&r, "annotations.list.0.datasource");
    assert!(i.reason.contains("not in the source map"), "{}", i.reason);
    // No SQL, an empty name and a name of 257 characters.
    let mut item = query_item("N", SQL);
    item["target"] = json!({});
    assert_eq!(one_item(&run(&with_items(vec![item])), "annotations.list.0.target").class, Class::Unsupported);
    for name in [json!(""), json!("x".repeat(257))] {
        let mut item = query_item("N", SQL);
        item["name"] = name;
        assert_eq!(one_item(&run(&with_items(vec![item])), "annotations.list.0.name").class, Class::Unsupported);
    }
    // The other items of the list stay.
    let r = run(&with_items(vec![query_item("Bad", "SELECT $__interval_ms"), query_item("Good", SQL)]));
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 1);
    assert_eq!(annotations_of(&r)[0]["name"], "Good");
}

// ---------------------------------------------------------------- PostgreSQL and MongoDB (ID-K142, ID-K333)

fn pg_ds() -> Value {
    json!({"type": "grafana-postgresql-datasource", "uid": "old-name"})
}

fn mongo_ds() -> Value {
    json!({"type": "haohanyang-mongodb-datasource", "uid": "mongo"})
}

/// An item of the PostgreSQL plugin, as Grafana 12 writes it (15.6).
fn pg_item(name: &str, sql: &str) -> Value {
    json!({
        "datasource": pg_ds(), "enable": true, "iconColor": "blue", "name": name,
        "target": {"datasource": pg_ds(), "editorMode": "code", "format": "table", "rawQuery": true, "rawSql": sql,
            "refId": "Anno", "sql": {"columns": [], "limit": 50}, "table": "deploys"}
    })
}

/// An item of the MongoDB plugin: the stages in `queryText`, the collection and the format in `queryType`.
fn mongo_item(name: &str, stages: &str) -> Value {
    json!({
        "datasource": mongo_ds(), "enable": true, "iconColor": "green", "name": name,
        "target": {"datasource": mongo_ds(), "queryText": stages, "collection": "node_events", "database": "analytics",
            "queryType": "table", "refId": "Anno"}
    })
}

fn mongo_options() -> hune_grafana::ImportOptions {
    hune_grafana::ImportOptions::with_source_map(&[("clickhouse", "prod"), ("old-name", "legacy"), ("mongo", "events")])
}

/// As `run`, with the source map of `mongo_options`.
fn run_mongo(d: &Value) -> hune_grafana::DashboardImport {
    let first = hune_grafana::import_dashboard(d, &mongo_options());
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    let mut o = mongo_options();
    o.accept = hune_grafana::parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    hune_grafana::import_dashboard(d, &o)
}

const PG_SQL: &str = "SELECT deployed_at AS time, version::text AS text, ARRAY[env, host] AS tags FROM deploys WHERE $__timeFilter(deployed_at)";

const STAGES: &str = r#"[{"$match": {"ts": {"$gte": "$__timeFrom", "$lt": "$__timeTo"}, "level": "error"}}, {"$project": {"_id": 0, "time": "$ts", "text": "$message", "tags": 1}}]"#;

#[test]
fn a_postgres_item_maps_with_the_dialect_of_its_source() {
    let r = run(&with_items(vec![builtin(), pg_item("Deploys", PG_SQL)]));
    assert_eq!(annotations_of(&r), json!([{"name": "Deploys", "source": "legacy", "sql": [PG_SQL], "color": "blue"}]));
    // The members of a code query are metadata. The 15.6 warning is at the SQL.
    assert_eq!(one_item(&r, "annotations.list.1.target.rawSql").class, Class::Changed);
    assert!(items_with(&r, "annotations.list.1.target.editorMode").is_empty());
    // The checks of 7.26 and 7.25: an interval macro and a refused function remove the item.
    for sql in [
        "SELECT $__timeGroup(t, $__interval) AS time FROM x GROUP BY 1",
        "SELECT pg_sleep(1) AS time",
        "SELECT t AS time FROM x WHERE n > $__interval_ms",
    ] {
        let r = run(&with_items(vec![pg_item("P", sql)]));
        let i = one_item(&r, "annotations.list.0.target.rawSql");
        assert_eq!((i.class, i.fallback.clone()), (Class::Unsupported, Some(Fallback::other("item removed"))), "{sql}");
        assert!(doc(&r).get("annotations").is_none(), "{sql}");
    }
    // The visual builder with a SQL text: the text stays and the builder goes.
    let mut item = pg_item("B", PG_SQL);
    item["target"]["editorMode"] = json!("builder");
    let r = run(&with_items(vec![item]));
    let i = one_item(&r, "annotations.list.0.target.editorMode");
    assert_eq!(i.class, Class::Unsupported);
    assert_eq!(annotations_of(&r)[0]["sql"], json!([PG_SQL]));
    // The format `time_series` is changed to `table`. A member of the MongoDB plugin is unknown here.
    let mut item = pg_item("F", PG_SQL);
    item["target"]["format"] = json!("time_series");
    item["target"]["queryText"] = json!("[]");
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.target.format").class, Class::Changed);
    assert_eq!(one_item(&r, "annotations.list.0.target.queryText").class, Class::Unsupported);
}

#[test]
fn a_mongodb_item_maps_to_the_object_of_7_29() {
    let r = run_mongo(&with_items(vec![builtin(), mongo_item("Errors", STAGES)]));
    let a = annotations_of(&r);
    assert_eq!(
        (a[0]["name"].as_str(), a[0]["source"].as_str(), a[0]["color"].as_str()),
        (Some("Errors"), Some("events"), Some("green"))
    );
    let text = a[0]["sql"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect::<Vec<_>>().join("\n");
    let tree: Value = serde_json::from_str(&text).expect("the lines are one JSON object");
    assert_eq!(tree["collection"], "node_events");
    assert_eq!(tree["database"], "analytics");
    assert_eq!(tree["aggregate"].as_array().unwrap().len(), 2);
    assert_eq!(one_item(&r, "annotations.list.1.target.queryText").class, Class::Changed);
    // The format `timeseries` is changed to `table`. A member of a SQL plugin is unknown here.
    let mut item = mongo_item("T", STAGES);
    item["target"]["queryType"] = json!("timeseries");
    item["target"]["rawSql"] = json!("SELECT 1");
    let r = run_mongo(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.target.queryType").class, Class::Changed);
    assert_eq!(one_item(&r, "annotations.list.0.target.rawSql").class, Class::Unsupported);
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 1);
}

#[test]
fn a_mongodb_item_that_hune_cannot_hold_is_removed() {
    // An interval macro (7.36, L9), a refused stage (L3), a text that is not JSON, and no queryText.
    for stages in [
        r#"[{"$match": {"ts": {"$gte": "$__timeFrom"}}}, {"$limit": "$__interval_ms"}]"#,
        r#"[{"$match": {"ts": {"$gte": "$__timeFrom"}}}, {"$out": "x"}]"#,
        r#"db.node_events.find({})"#,
        "",
    ] {
        let r = run_mongo(&with_items(vec![mongo_item("M", stages)]));
        let i = one_item(&r, "annotations.list.0.target.queryText");
        assert_eq!(
            (i.class, i.fallback.clone()),
            (Class::Unsupported, Some(Fallback::other("item removed"))),
            "{stages}"
        );
        assert!(doc(&r).get("annotations").is_none(), "{stages}");
    }
    let r = run_mongo(&with_items(vec![mongo_item("M", r#"[{"$limit": "$__interval_ms"}]"#)]));
    let i = one_item(&r, "annotations.list.0.target.queryText");
    assert!(i.reason.contains("in the query of an annotation"), "{}", i.reason);
    let mut item = mongo_item("N", STAGES);
    item["target"].as_object_mut().unwrap().remove("collection");
    let r = run_mongo(&with_items(vec![item, mongo_item("Good", STAGES)]));
    assert_eq!(one_item(&r, "annotations.list.0.target").class, Class::Unsupported);
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 1);
    assert_eq!(annotations_of(&r)[0]["name"], "Good");
}

#[test]
fn the_format_other_than_table_is_changed() {
    let mut item = query_item("T", SQL);
    item["target"]["format"] = json!(0);
    let r = run(&with_items(vec![item]));
    assert_eq!(one_item(&r, "annotations.list.0.target.format").class, Class::Changed);
    assert_eq!(annotations_of(&r).as_array().unwrap().len(), 1);
}

#[test]
fn the_text_variable_in_code_of_an_annotation_follows_the_number_rule() {
    let mut d = with_items(vec![query_item("V", "SELECT t AS time FROM x WHERE n > $threshold")]);
    d["templating"] =
        json!({"list": [{"type": "textbox", "name": "threshold", "query": "5", "current": {"value": "5"}}]});
    let r = run(&d);
    assert_eq!(doc(&r)["variables"][0]["value_type"], "number");
}

// ---------------------------------------------------------------- the form v2 (synthetic)

fn v2_doc(items: Vec<Value>) -> Value {
    json!({
        "apiVersion": "dashboard.grafana.app/v2beta1", "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1"},
        "spec": {
            "title": "V2 annotations", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": [], "elements": {}, "layout": {"kind": "RowsLayout", "spec": {"rows": []}},
            "annotations": items
        }
    })
}

#[test]
fn the_v2_form_has_the_same_items_below_spec() {
    let builtin = json!({"kind": "AnnotationQuery", "spec": {"builtIn": true, "name": "Annotations & Alerts", "enable": true, "hide": true, "iconColor": "rgba(0, 211, 255, 1)"}});
    let item = json!({"kind": "AnnotationQuery", "spec": {
        "name": "Deploys", "enable": true, "hide": false, "iconColor": "blue",
        "query": {"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0",
            "datasource": {"name": "clickhouse"},
            "spec": {"rawSql": SQL, "format": 1, "editorType": "sql", "queryType": "table", "refId": "Anno"}}
    }});
    let r = run(&v2_doc(vec![builtin, item]));
    assert_eq!(annotations_of(&r), json!([{"name": "Deploys", "source": "prod", "sql": [SQL], "color": "blue"}]));
    assert_eq!(one_item(&r, "spec.annotations.1.spec.hide").class, Class::Dropped);
    // The real export of the reference Grafana has an item with no query: it is removed.
    let bare = json!({"kind": "AnnotationQuery", "spec": {"name": "Deploys", "enable": true, "iconColor": "red"}});
    let r = run(&v2_doc(vec![bare]));
    assert!(doc(&r).get("annotations").is_none());
    assert!(r.items.iter().all(|i| i.class == Class::Unsupported));
}

#[test]
fn a_filter_with_200000_ids_is_refused_at_once() {
    // Review of 2026-11-14, finding F3: the list was read with a quadratic loop.
    let ids: Vec<i64> = (0..200_000).collect();
    for exclude in [false, true] {
        let mut item = query_item("F", SQL);
        item["filter"] = json!({"exclude": exclude, "ids": ids});
        let t = std::time::Instant::now();
        let r = run_plain(&with_items(vec![item]));
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "{:?}", t.elapsed());
        let unsupported: Vec<_> =
            items_with(&r, "annotations.list.0.filter").into_iter().filter(|i| i.class == Class::Unsupported).collect();
        assert_eq!(unsupported.len(), 1, "exclude {exclude}");
        // The list leaves no item for each id.
        assert!(items_with(&r, "annotations.list.0.filter.ids").is_empty());
    }
}
