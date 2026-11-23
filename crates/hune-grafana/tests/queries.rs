//! The rows of 15.6 for several queries in a panel (4.15a, decisions ID-K165, ID-K169 and ID-K171): one test
//! for each row in the four input forms (v1 raw, v1 wrapped, v2alpha1 and v2beta1), and separate tests for a
//! hidden first target, a refId that breaks the grammar, two targets with one refId, nine targets, a Mixed
//! panel with a target of each kind of source, a target of `-- Dashboard --`, a repeat with two targets and the
//! matcher `byFrameRefID`. The v2 paths follow the schema: no real v2 export has several queries (open point).

mod common;

use common::*;
use hune_grafana::{
    Class, DashboardImport, Fallback, ImportOptions, MongoLists, import_dashboard, parse_acceptance_file,
};
use serde_json::{Value, json};

const FORMS: [&str; 4] = ["raw", "wrapped", "alpha1", "beta1"];

const MONGO_TEXT: &str =
    r#"[{"$match":{"ts":{"$gte":"$__timeFrom","$lte":"$__timeTo"}}},{"$project":{"_id":0,"host":1}}]"#;

fn pg() -> Value {
    json!({"type": "grafana-postgresql-datasource", "uid": "pg"})
}

fn mongo() -> Value {
    json!({"type": "haohanyang-mongodb-datasource", "uid": "mongo"})
}

/// The kinds of target of the tests.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Ch,
    Pg,
    Mongo,
}

/// One target of a panel, before the forms write it.
#[derive(Clone)]
struct T {
    kind: Kind,
    /// The data source of the target. `Null`: the target names none.
    ds: Value,
    /// The `refId`. `Null`: absent.
    ref_id: Value,
    /// The `hide` (`hidden` in v2). `Null`: absent.
    hide: Value,
    text: String,
}

fn t(ref_id: &str, text: &str) -> T {
    T { kind: Kind::Ch, ds: ch(), ref_id: json!(ref_id), hide: Value::Null, text: text.to_owned() }
}

fn hidden(mut x: T) -> T {
    x.hide = json!(true);
    x
}

fn with_ds(mut x: T, kind: Kind, ds: Value) -> T {
    x.kind = kind;
    x.ds = ds;
    if kind == Kind::Mongo {
        x.text = MONGO_TEXT.to_owned();
    }
    x
}

fn plugin(x: &T) -> Value {
    match x.kind {
        Kind::Ch => json!({"rawSql": x.text, "format": 1, "editorType": "sql", "queryType": "table"}),
        Kind::Pg => json!({"rawSql": x.text, "format": "table", "editorMode": "code", "rawQuery": true}),
        Kind::Mongo => json!({"queryText": x.text, "collection": "orders", "queryType": "table"}),
    }
}

fn v1_target(x: &T) -> Value {
    let mut o = plugin(x);
    if !x.ds.is_null() {
        o["datasource"] = x.ds.clone();
    }
    if !x.ref_id.is_null() {
        o["refId"] = x.ref_id.clone();
    }
    if !x.hide.is_null() {
        o["hide"] = x.hide.clone();
    }
    o
}

fn v2_query(form: &str, x: &T) -> Value {
    let group = x.ds["type"].as_str().unwrap_or("grafana-clickhouse-datasource").to_owned();
    let mut spec = if form == "alpha1" {
        json!({"query": {"kind": group, "spec": plugin(x)}})
    } else {
        let mut query = json!({"kind": "DataQuery", "group": group, "version": "v0", "spec": plugin(x)});
        if let Some(name) = x.ds["uid"].as_str() {
            query["datasource"] = json!({"name": name});
        }
        json!({"query": query})
    };
    if form == "alpha1" && !x.ds.is_null() {
        spec["datasource"] = x.ds.clone();
    }
    if !x.ref_id.is_null() {
        spec["refId"] = x.ref_id.clone();
    }
    if !x.hide.is_null() {
        spec["hidden"] = x.hide.clone();
    }
    json!({"kind": "PanelQuery", "spec": spec})
}

/// What a test changes in a panel.
struct P {
    ptype: &'static str,
    targets: Vec<T>,
    /// The data source of the panel in v1: `-- Mixed --` for a Mixed panel.
    mixed: bool,
    /// The v1 members of the panel, and the v2 members of the grid item, for example a repeat.
    repeat: bool,
    overrides: Value,
}

fn p(ptype: &'static str, targets: Vec<T>) -> P {
    P { ptype, targets, mixed: false, repeat: false, overrides: json!([]) }
}

fn host_var_v1() -> Value {
    json!({"type": "custom", "allowCustomValue": false, "name": "host", "query": "a,b,c", "multi": true, "includeAll": true, "current": {"value": ["a"]}, "options": []})
}

fn host_var_v2() -> Value {
    json!({"kind": "CustomVariable", "spec": {"allowCustomValue": false, "name": "host", "query": "a,b,c", "multi": true, "includeAll": true,
        "current": {"text": "a", "value": "a"}, "options": [], "hide": "dontHide", "skipUrlSync": false}})
}

fn v1(panels: &[P]) -> Value {
    let list: Vec<Value> = panels
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let ds =
                if x.mixed { json!({"type": "datasource", "uid": "-- Mixed --"}) } else { x.targets[0].ds.clone() };
            let mut o = json!({
                "id": i as i64 + 1, "type": x.ptype, "title": format!("Panel {}", i + 1),
                "gridPos": {"h": 4, "w": 12, "x": 0, "y": 4 * i as i64},
                "targets": x.targets.iter().map(v1_target).collect::<Vec<_>>(),
                "fieldConfig": {"defaults": {}, "overrides": x.overrides}, "options": {}
            });
            if !ds.is_null() {
                o["datasource"] = ds;
            }
            if x.repeat {
                o["repeat"] = json!("host");
                o["repeatDirection"] = json!("v");
            }
            o
        })
        .collect();
    let vars = if panels.iter().any(|x| x.repeat) { vec![host_var_v1()] } else { vec![] };
    dash(list, vars)
}

fn v2(form: &str, panels: &[P]) -> Value {
    let mut elements = serde_json::Map::new();
    let mut items = Vec::new();
    for (i, x) in panels.iter().enumerate() {
        let viz = if form == "alpha1" {
            json!({"kind": x.ptype, "spec": {"pluginVersion": "12.2.1", "options": {},
                "fieldConfig": {"defaults": {}, "overrides": x.overrides}}})
        } else {
            json!({"kind": "VizConfig", "group": x.ptype, "version": "12.2.1", "spec": {"options": {},
                "fieldConfig": {"defaults": {}, "overrides": x.overrides}}})
        };
        let queries: Vec<Value> = x.targets.iter().map(|q| v2_query(form, q)).collect();
        elements.insert(
            format!("panel-{}", i + 1),
            json!({"kind": "Panel", "spec": {"id": i + 1, "title": format!("Panel {}", i + 1), "description": "",
                "links": [], "vizConfig": viz,
                "data": {"kind": "QueryGroup", "spec": {"queries": queries, "transformations": [], "queryOptions": {}}}}}),
        );
        let mut spec = json!({"x": 0, "y": 4 * i as i64, "width": 12, "height": 4,
            "element": {"kind": "ElementReference", "name": format!("panel-{}", i + 1)}});
        if x.repeat {
            spec["repeat"] = json!({"mode": "variable", "value": "host", "direction": "v"});
        }
        items.push(json!({"kind": "GridLayoutItem", "spec": spec}));
    }
    let variables = if panels.iter().any(|x| x.repeat) { vec![host_var_v2()] } else { vec![] };
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{}", if form == "alpha1" { "alpha1" } else { "beta1" }),
        "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "Queries", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": variables, "elements": elements, "layout": {"kind": "GridLayout", "spec": {"items": items}}}
    })
}

fn in_form(form: &str, panels: &[P]) -> Value {
    match form {
        "raw" => v1(panels),
        "wrapped" => json!({"dashboard": v1(panels), "meta": {"slug": "t1"}}),
        other => v2(other, panels),
    }
}

fn options() -> ImportOptions {
    let mut o = ImportOptions::with_source_map(&[("clickhouse", "prod"), ("pg", "analytics"), ("mongo", "events")]);
    o.mongo_lists.insert(
        "events".to_owned(),
        MongoLists {
            databases: vec!["analytics".to_owned()],
            collections: vec!["orders".to_owned(), "node_load".to_owned()],
        },
    );
    o
}

/// Imports twice: the second run accepts each item of the first (see `run` of the common helpers).
fn import(form: &str, panels: &[P]) -> DashboardImport {
    let input = in_form(form, panels);
    let first = import_dashboard(&input, &options());
    let mut o = options();
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    import_dashboard(&input, &o)
}

fn every_form(panels: &[P], check: impl Fn(&str, &DashboardImport)) {
    for form in FORMS {
        check(form, &import(form, panels));
    }
}

fn data(d: &Value, panel: usize) -> &Value {
    &d["sections"][0]["panels"][panel]["data"]
}

fn refs_of(d: &Value, panel: usize) -> Vec<String> {
    let data = data(d, panel);
    let mut out = vec![data["query"]["ref"].as_str().unwrap_or("A").to_owned()];
    for e in data["extra_queries"].as_array().into_iter().flatten() {
        out.push(e["ref"].as_str().unwrap().to_owned());
    }
    out
}

// ------------------------------------------------------------------ rows

#[test]
fn two_to_eight_items_map_to_query_and_extra_queries_in_each_form() {
    for count in [2usize, 3, 8] {
        let targets: Vec<T> =
            (0..count).map(|i| t(&((b'A' + i as u8) as char).to_string(), &format!("SELECT {i} AS n"))).collect();
        every_form(&[p("table", targets)], |form, r| {
            no_items(r);
            let d = doc(r);
            assert_eq!(data(&d, 0)["query"]["sql"], json!(["SELECT 0 AS n"]), "{form}");
            let extra = data(&d, 0)["extra_queries"].as_array().unwrap();
            assert_eq!(extra.len(), count - 1, "{form}");
            assert_eq!(extra[0]["sql"], json!(["SELECT 1 AS n"]), "{form}");
            assert_eq!(extra[0]["ref"], "B");
            assert_eq!(extra[0]["source"], "prod");
            assert_eq!(extra[0]["format"], "table");
            assert!(data(&d, 0)["query"].get("ref").is_none(), "the ref A of the first query gives no member");
            assert!(extra[0].get("hide").is_none());
        });
    }
}

#[test]
fn the_ninth_item_is_unsupported_and_removed() {
    let targets: Vec<T> = (0..9).map(|i| t(&((b'A' + i as u8) as char).to_string(), "SELECT 1")).collect();
    every_form(&[p("table", targets)], |form, r| {
        let d = doc(r);
        assert_eq!(refs_of(&d, 0).len(), 8, "{form}");
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(r.items[0].class, Class::Unsupported);
        assert_eq!(r.items[0].fallback, Some(Fallback::other("the item is removed")));
        assert!(r.items[0].pointer.ends_with("/8"), "{}", r.items[0].pointer);
    });
}

#[test]
fn hide_maps_to_hide_and_a_hidden_first_target_stays() {
    let mut targets = vec![t("A", "SELECT 1"), t("B", "SELECT 2"), t("C", "SELECT 3")];
    targets[1].hide = json!(true);
    targets[2].hide = json!(false);
    every_form(&[p("table", targets)], |form, r| {
        no_items(r);
        let d = doc(r);
        let extra = data(&d, 0)["extra_queries"].as_array().unwrap();
        assert_eq!(extra[0]["hide"], true, "{form}");
        assert!(extra[1].get("hide").is_none(), "hide false is metadata: {form}");
    });
    // A hidden first target: `hide` = `true` in `data.query`, and the other queries stay.
    let targets = vec![hidden(t("A", "SELECT 1")), t("B", "SELECT 2")];
    every_form(&[p("table", targets)], |form, r| {
        no_items(r);
        let d = doc(r);
        assert_eq!(data(&d, 0)["query"]["hide"], true, "{form}");
        assert_eq!(data(&d, 0)["extra_queries"].as_array().unwrap().len(), 1);
    });
    // A panel with one hidden query is valid too.
    every_form(&[p("table", vec![hidden(t("A", "SELECT 1"))])], |form, r| {
        no_items(r);
        assert_eq!(data(&doc(r), 0)["query"]["hide"], true, "{form}");
    });
}

#[test]
fn a_hide_that_is_not_a_flag_is_unsupported_and_the_query_stays_hidden() {
    // Finding D5: the text "true" and the number 2 gave metadata and a query that runs. Now each is listed as
    // `unsupported`, and the query keeps `hide` = `true`, as Grafana hides it.
    for odd in [json!("true"), json!(2), json!("false"), json!([true])] {
        let mut targets = vec![t("A", "SELECT 1"), t("B", "SELECT 2")];
        targets[1].hide = odd.clone();
        every_form(&[p("table", targets)], |form, r| {
            assert_eq!(r.items.len(), 1, "{form} {odd}: {:#?}", r.items);
            assert_eq!(r.items[0].class, Class::Unsupported, "{form} {odd}");
            assert_eq!(r.items[0].fallback, Some(Fallback::other("the query is hidden: hide = true")));
            assert!(r.items[0].reason.contains("hide must be true, false, 0 or 1"), "{}", r.items[0].reason);
            let d = doc(r);
            assert_eq!(data(&d, 0)["extra_queries"][0]["hide"], true, "{form} {odd}");
            assert!(data(&d, 0)["query"].get("hide").is_none(), "{form} {odd}");
        });
    }
    // The flags 1 and 0 and the value null keep their meaning, with no item.
    let mut targets = vec![t("A", "SELECT 1"), t("B", "SELECT 2"), t("C", "SELECT 3")];
    targets[1].hide = json!(1);
    targets[2].hide = json!(0);
    every_form(&[p("table", targets)], |form, r| {
        no_items(r);
        let d = doc(r);
        assert_eq!(data(&d, 0)["extra_queries"][0]["hide"], true, "{form}");
        assert!(data(&d, 0)["extra_queries"][1].get("hide").is_none(), "{form}");
    });
}

#[test]
fn a_ref_id_of_the_grammar_maps_as_it_is_and_the_first_ref_a_gives_no_member() {
    let targets = vec![t("Z", "SELECT 1"), t("A", "SELECT 2"), t("m_2", "SELECT 3")];
    every_form(&[p("table", targets)], |form, r| {
        no_items(r);
        let d = doc(r);
        assert_eq!(refs_of(&d, 0), ["Z", "A", "m_2"], "{form}");
        assert_eq!(data(&d, 0)["query"]["ref"], "Z");
    });
}

#[test]
fn a_ref_id_that_breaks_the_grammar_is_changed_to_the_first_free_letter() {
    // A digit first, 17 characters, an empty text, a text with a blank, and an absent member.
    for bad in [json!("1x"), json!("A".repeat(17)), json!(""), json!("a b"), Value::Null] {
        let mut second = t("x", "SELECT 2");
        second.ref_id = bad.clone();
        every_form(&[p("table", vec![t("A", "SELECT 1"), second.clone()])], |form, r| {
            let d = doc(r);
            assert_eq!(refs_of(&d, 0), ["A", "B"], "{form} {bad}");
            let changed: Vec<_> = r.items.iter().filter(|i| i.class == Class::Changed).collect();
            assert_eq!(changed.len(), 1, "{form} {bad}: {:#?}", r.items);
        });
    }
}

#[test]
fn two_targets_with_one_ref_id_give_a_new_letter_to_the_second() {
    let targets = vec![t("A", "SELECT 1"), t("A", "SELECT 2"), t("B", "SELECT 3")];
    every_form(&[p("table", targets)], |form, r| {
        let d = doc(r);
        // The first `A` stays. `B` belongs to the third target, so the second takes the next free letter, `C`.
        assert_eq!(refs_of(&d, 0), ["A", "C", "B"], "{form}");
        assert_eq!(r.items.iter().filter(|i| i.class == Class::Changed).count(), 1, "{form}: {:#?}", r.items);
    });
}

#[test]
fn a_panel_with_one_query_has_no_ref_member_and_keeps_its_bytes() {
    // The refId `B` of a panel with one query is metadata: the document has no `ref` (4.42).
    let one_b = p("table", vec![t("B", "SELECT 1")]);
    let one_a = p("table", vec![t("A", "SELECT 1")]);
    for form in FORMS {
        let b = import(form, std::slice::from_ref(&one_b));
        let a = import(form, std::slice::from_ref(&one_a));
        no_items(&b);
        assert!(!b.document_bytes.as_ref().unwrap().windows(5).any(|w| w == b"\"ref\""), "{form}");
        assert_eq!(b.document_bytes, a.document_bytes, "{form}");
    }
}

#[test]
fn the_matcher_by_frame_ref_id_maps_to_query() {
    let overrides = json!([{"matcher": {"id": "byFrameRefID", "options": "B"},
        "properties": [{"id": "unit", "value": "percent"}]}]);
    let mut x = p("timeseries", vec![t("A", "SELECT 1"), t("B", "SELECT 2")]);
    x.overrides = overrides;
    every_form(&[x], |form, r| {
        no_items(r);
        let d = doc(r);
        let o = &d["sections"][0]["panels"][0]["fields"]["overrides"][0];
        assert_eq!(o["match"], json!({"by": "query", "value": "B"}), "{form}");
        assert_eq!(o["set"], json!({"unit": "percent"}));
    });
    // A refId that the importer changed is changed in the matcher too.
    let mut x = p("timeseries", vec![t("A", "SELECT 1"), t("1x", "SELECT 2")]);
    x.overrides = json!([{"matcher": {"id": "byFrameRefID", "options": "1x"},
        "properties": [{"id": "unit", "value": "percent"}]}]);
    every_form(&[x], |form, r| {
        let d = doc(r);
        assert_eq!(d["sections"][0]["panels"][0]["fields"]["overrides"][0]["match"]["value"], "B", "{form}");
        let matcher: Vec<_> = r.items.iter().filter(|i| i.member.contains("matcher")).collect();
        assert_eq!(matcher.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(matcher[0].class, Class::Changed);
    });
    // A text that breaks the grammar of 4.3: the override is removed.
    let mut x = p("timeseries", vec![t("A", "SELECT 1")]);
    x.overrides = json!([{"matcher": {"id": "byFrameRefID", "options": "a b"},
        "properties": [{"id": "unit", "value": "percent"}]}]);
    every_form(&[x], |form, r| {
        let d = doc(r);
        assert_eq!(d["sections"][0]["panels"][0]["fields"]["overrides"], json!([]), "{form}");
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(r.items[0].fallback, Some(Fallback::other("override removed")));
    });
    // A ref that no query has is valid, and it matches no field.
    let mut x = p("timeseries", vec![t("A", "SELECT 1")]);
    x.overrides = json!([{"matcher": {"id": "byFrameRefID", "options": "Q"},
        "properties": [{"id": "unit", "value": "percent"}]}]);
    every_form(&[x], |form, r| {
        no_items(r);
        let d = doc(r);
        assert_eq!(d["sections"][0]["panels"][0]["fields"]["overrides"][0]["match"]["value"], "Q", "{form}");
    });
    // The matchers `byType` and `byValue` stay unsupported.
    for id in ["byType", "byValue"] {
        let mut x = p("timeseries", vec![t("A", "SELECT 1")]);
        x.overrides =
            json!([{"matcher": {"id": id, "options": "A"}, "properties": [{"id": "unit", "value": "percent"}]}]);
        let r = import("raw", &[x]);
        assert_eq!(r.items.len(), 1, "{id}");
        assert_eq!(r.items[0].fallback, Some(Fallback::other("override removed")));
    }
}

#[test]
fn a_mixed_panel_maps_each_target_by_its_own_data_source() {
    let mut x = p(
        "table",
        vec![
            t("A", "SELECT 1 AS n"),
            with_ds(t("B", "SELECT 2 AS n"), Kind::Pg, pg()),
            with_ds(t("C", ""), Kind::Mongo, mongo()),
        ],
    );
    x.mixed = true;
    every_form(&[x], |form, r| {
        let d = doc(r);
        let extra = data(&d, 0)["extra_queries"].as_array().unwrap();
        assert_eq!(data(&d, 0)["query"]["source"], "prod", "{form}");
        assert_eq!((extra[0]["source"].as_str(), extra[1]["source"].as_str()), (Some("analytics"), Some("events")));
        assert!(!extra[1]["sql"].as_array().unwrap().is_empty());
        // The items of the report: the format of a PostgreSQL target is a text, the other members map.
        assert!(r.items.iter().all(|i| i.class != Class::Unsupported), "{form}: {:#?}", r.items);
    });
}

#[test]
fn a_mixed_panel_with_one_target_maps_to_data_query() {
    let mut x = p("table", vec![with_ds(t("A", "SELECT 2 AS n"), Kind::Pg, pg())]);
    x.mixed = true;
    every_form(&[x], |form, r| {
        no_items(r);
        let d = doc(r);
        assert_eq!(data(&d, 0)["query"]["source"], "analytics", "{form}");
        assert!(data(&d, 0).get("extra_queries").is_none());
    });
}

#[test]
fn a_target_with_no_data_source_or_a_data_source_that_no_row_maps_is_removed_in_a_panel_with_several_targets() {
    let mut no_ds = t("B", "SELECT 2");
    no_ds.ds = Value::Null;
    let unknown =
        with_ds(t("C", "SELECT 3"), Kind::Ch, json!({"type": "grafana-clickhouse-datasource", "uid": "unknown"}));
    let mut dash_ds = t("D", "SELECT 4");
    dash_ds.ds = json!({"type": "datasource", "uid": "-- Dashboard --"});
    let grafana_ds = with_ds(t("E", "SELECT 5"), Kind::Ch, json!({"type": "datasource", "uid": "-- Grafana --"}));
    let mut x = p("table", vec![t("A", "SELECT 1"), no_ds, unknown, dash_ds, grafana_ds]);
    x.mixed = true;
    every_form(&[x], |form, r| {
        let d = doc(r);
        assert_eq!(refs_of(&d, 0), ["A"], "{form}");
        let removed: Vec<_> = r
            .items
            .iter()
            .filter(|i| i.class == Class::Unsupported && i.fallback == Some(Fallback::other("the target is removed")))
            .collect();
        assert_eq!(removed.len(), 4, "{form}: {:#?}", r.items);
    });
}

#[test]
fn a_mixed_panel_with_no_usable_target_is_removed() {
    let mut no_ds = t("A", "SELECT 1");
    no_ds.ds = Value::Null;
    let mut x = p("table", vec![no_ds]);
    x.mixed = true;
    let r = import("raw", &[x]);
    assert!(
        r.document_bytes.is_none() || !String::from_utf8_lossy(r.document_bytes.as_ref().unwrap()).contains("Panel 1")
    );
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved)), "{:#?}", r.items);
    // Two targets that no row maps: each is removed, and the panel is removed.
    let bad = |id: &str| with_ds(t(id, "SELECT 1"), Kind::Ch, json!({"type": "datasource", "uid": "-- Grafana --"}));
    let mut x = p("table", vec![bad("A"), bad("B")]);
    x.mixed = true;
    let r = import("raw", &[x]);
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved)), "{:#?}", r.items);
}

#[test]
fn a_panel_of_dashboard_with_other_targets_keeps_the_first_target() {
    let mut reuse = t("A", "");
    reuse.ds = json!({"type": "datasource", "uid": "-- Dashboard --"});
    let first = p("table", vec![t("A", "SELECT 1")]);
    let mut second = p("stat", vec![reuse, t("B", "SELECT 2")]);
    // The v1 form needs `panelId` in the reuse target. The v2 forms read it from the plugin spec.
    second.targets[0].text = String::new();
    for form in FORMS {
        let mut input = in_form(form, &[first_clone(&first), second_clone(&second)]);
        add_panel_id(form, &mut input);
        let r0 = import_dashboard(&input, &options());
        let mut o = options();
        let ids: Vec<String> = r0.items.iter().map(|i| i.id.clone()).collect();
        o.accept = parse_acceptance_file(&ids.join("\n")).unwrap();
        let r = import_dashboard(&input, &o);
        let d = doc(&r);
        assert_eq!(
            d["sections"][0]["panels"][1]["data"],
            json!({"reuse": {"panel": 1, "with_transforms": false}}),
            "{form}"
        );
        let removed: Vec<_> = r
            .items
            .iter()
            .filter(|i| i.fallback == Some(Fallback::other("the targets other than the first are removed")))
            .collect();
        assert_eq!(removed.len(), 1, "{form}: {:#?}", r.items);
    }
}

fn first_clone(x: &P) -> P {
    P { ptype: x.ptype, targets: x.targets.clone(), mixed: x.mixed, repeat: x.repeat, overrides: x.overrides.clone() }
}

fn second_clone(x: &P) -> P {
    first_clone(x)
}

/// Writes `panelId` 1 in the first target of the second panel, in the place of each form.
fn add_panel_id(form: &str, input: &mut Value) {
    match form {
        "raw" => input["panels"][1]["targets"][0]["panelId"] = json!(1),
        "wrapped" => input["dashboard"]["panels"][1]["targets"][0]["panelId"] = json!(1),
        "alpha1" => {
            input["spec"]["elements"]["panel-2"]["spec"]["data"]["spec"]["queries"][0]["spec"]["query"]["spec"]["panelId"] =
                json!(1);
        }
        _ => {
            input["spec"]["elements"]["panel-2"]["spec"]["data"]["spec"]["queries"][0]["spec"]["query"]["spec"]["panelId"] =
                json!(1);
        }
    }
}

#[test]
fn a_panel_with_repeat_and_two_targets_loses_the_repeat_and_keeps_the_queries() {
    let mut x = p("table", vec![t("A", "SELECT 1"), t("B", "SELECT 2")]);
    x.repeat = true;
    every_form(&[x], |form, r| {
        let d = doc(r);
        let panel = &d["sections"][0]["panels"][0];
        assert!(panel.get("repeat").is_none(), "{form}");
        assert_eq!(refs_of(&d, 0), ["A", "B"], "{form}");
        let item = r.items.iter().find(|i| i.fallback == Some(Fallback::other("no repeat on the panel"))).unwrap();
        assert_eq!(item.class, Class::Unsupported, "{form}");
        assert!(item.reason.contains("several queries"), "{}", item.reason);
    });
    // A panel with one query keeps its repeat.
    let mut x = p("table", vec![t("A", "SELECT 1")]);
    x.repeat = true;
    every_form(&[x], |form, r| {
        no_items(r);
        assert_eq!(doc(r)["sections"][0]["panels"][0]["repeat"]["variable"], "host", "{form}");
    });
}

#[test]
fn a_bar_chart_and_an_xy_chart_keep_the_first_query() {
    for ptype in ["barchart", "xychart"] {
        let x = p(ptype, vec![t("A", "SELECT 1 AS a, 2 AS b"), t("B", "SELECT 2"), t("C", "SELECT 3")]);
        every_form(&[x], |form, r| {
            let d = doc(r);
            assert!(data(&d, 0).get("extra_queries").is_none(), "{ptype} {form}");
            let removed: Vec<_> =
                r.items.iter().filter(|i| i.fallback == Some(Fallback::other("the item is removed"))).collect();
            assert_eq!(removed.len(), 2, "{ptype} {form}: {:#?}", r.items);
            assert!(removed.iter().all(|i| i.class == Class::Unsupported));
        });
    }
}

#[test]
fn a_bar_chart_that_reuses_a_panel_with_several_queries_is_removed() {
    let source = p("table", vec![t("A", "SELECT 1"), t("B", "SELECT 2")]);
    let mut reuse = t("A", "");
    reuse.ds = json!({"type": "datasource", "uid": "-- Dashboard --"});
    let bar = p("barchart", vec![reuse]);
    for form in FORMS {
        let mut input = in_form(form, &[first_clone(&source), first_clone(&bar)]);
        add_panel_id(form, &mut input);
        let r = import_dashboard(&input, &options());
        let removed =
            r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved) && i.reason.contains("several queries"));
        assert!(removed, "{form}: {:#?}", r.items);
    }
}

#[test]
fn an_unknown_member_of_a_v2_query_is_listed_and_a_member_of_a_second_query_is_read() {
    for form in ["alpha1", "beta1"] {
        let mut input = in_form(form, &[p("table", vec![t("A", "SELECT 1"), t("B", "SELECT 2")])]);
        input["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][1]["spec"]["mystery"] = json!("x");
        let r = import_dashboard(&input, &options());
        let found: Vec<_> = r.items.iter().filter(|i| i.pointer.ends_with("/queries/1/spec/mystery")).collect();
        assert_eq!(found.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(found[0].class, Class::Unsupported);
        assert_eq!(found[0].reason, "unknown member");
    }
}

#[test]
fn a_section_with_repeat_and_a_panel_with_several_queries_loses_the_section_repeat() {
    // The row panel of v1 with `repeat` holds the panel that follows it.
    let row = json!({"id": 100, "type": "row", "title": "Host ${host}", "collapsed": false, "repeat": "host",
        "gridPos": {"h": 1, "w": 24, "x": 0, "y": 0}, "panels": []});
    let mut x = v1(&[p("table", vec![t("A", "SELECT 1"), t("B", "SELECT 2")])]);
    x["panels"][0]["gridPos"]["y"] = json!(1);
    x["panels"] = json!([row, x["panels"][0].clone()]);
    x["templating"]["list"] = json!([host_var_v1()]);
    let r = import_dashboard(&x, &options());
    let mut o = options();
    let ids: Vec<String> = r.items.iter().map(|i| i.id.clone()).collect();
    o.accept = parse_acceptance_file(&ids.join("\n")).unwrap();
    let r = import_dashboard(&x, &o);
    let d = doc(&r);
    assert!(d["sections"][0].get("repeat").is_none());
    assert_eq!(refs_of(&d, 0), ["A", "B"]);
    assert!(r.items.iter().any(|i| i.reason.contains("several queries")), "{:#?}", r.items);
}

// ------------------------------------------------------------------ the fixtures of 15.14c

/// The six dashboards `tests/fixtures/grafana/multi_query_*.json`: the conversion of the parity documents. The
/// data source `conf-ds` is the source `conf`.
#[test]
fn the_six_fixtures_of_15_14c_import_with_the_classes_of_15_6() {
    let mut o = ImportOptions::with_source_map(&[("clickhouse", "prod"), ("conf-ds", "conf")]);
    let names = ["timeseries", "table", "stat_hidden", "mixed", "error", "all_hidden"];
    let counts = [3usize, 2, 2, 2, 2, 2];
    for (name, count) in names.iter().zip(counts) {
        let path = repo_root().join(format!("tests/fixtures/grafana/multi_query_{name}.json"));
        let input = read_json(&path);
        let r = import_dashboard(&input, &o);
        let d = doc(&r);
        assert_eq!(refs_of(&d, 0).len(), count, "{name}");
        // Every member maps: the converter writes the inverse of the importer.
        no_items(&r);
        // The parity document is the same document.
        let parity =
            read_json(&repo_root().join(format!("tests/fixtures/parity/multi-query-{}.json", name.replace('_', "-"))));
        assert_eq!(d["sections"], parity["sections"], "{name}");
    }
    o.accept = parse_acceptance_file("").unwrap();
    // The Mixed fixture needs the source map: without `conf-ds`, the target is removed and listed.
    let bare = ImportOptions::with_source_map(&[("clickhouse", "prod")]);
    let r = import_dashboard(&read_json(&repo_root().join("tests/fixtures/grafana/multi_query_mixed.json")), &bare);
    assert!(r.items.iter().any(|i| i.reason.contains("not in the source map")), "{:#?}", r.items);
}
