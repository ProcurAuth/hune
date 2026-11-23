//! Wave H, step 0 (decision ID-K326, case I36 of 16.13): the data source variable and `__inputs` by the source map
//! (D01, P00), a panel that has the `y` of its row (D03), and the format 2 of the plugin on a panel that is not a
//! logs panel (D07). The forms v1, v2alpha1 and v2beta1.

mod common;

use common::*;
use hune_grafana::{
    Class, DashboardImport, DashboardResult, Fallback, ImportOptions, SourceKind, SourceUse, import_dashboard,
    parse_acceptance_file,
};
use serde_json::{Value, json};

/// The reason of a reference to a data source variable that the source map does not name (15.9).
const UNMAPPED: &str = "the data source is a variable that the source map does not name";

fn fixture() -> Value {
    read_json(&repo_root().join("tests/fixtures/grafana/datasource_variable.json"))
}

/// Imports with a source map. `accept_all` gives a second run that accepts each item of the first.
fn import_with(d: &Value, map: &[(&str, &str)], accept_all: bool) -> DashboardImport {
    let first = import_dashboard(d, &ImportOptions::with_source_map(map));
    if !accept_all {
        return first;
    }
    let mut o = ImportOptions::with_source_map(map);
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    import_dashboard(d, &o)
}

const MAP: [(&str, &str); 2] = [("datasource", "prod"), ("DS_CLICKHOUSE", "prod")];

/// The text of a golden file: the document, then one line for each item.
fn golden_text(r: &DashboardImport) -> String {
    let mut s = serde_json::to_string_pretty(&doc(r)).expect("json");
    s.push('\n');
    for i in &r.items {
        let fallback = i.fallback.as_ref().map_or("-", |f| f.text());
        s.push_str(&format!("{} [{}] {} | {}\n", i.id, i.class.as_str(), i.reason, fallback));
    }
    s
}

// ------------------------------------------------------------------ D01: the data source variable and __inputs

#[test]
fn a_mapped_data_source_variable_and_input_leave_the_document_and_each_reference_gets_the_source() {
    let r = import_with(&fixture(), &MAP, true);
    assert_eq!(r.result, DashboardResult::WouldWrite, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.class == Class::Changed), "{:#?}", r.items);
    let d = doc(&r);
    // The data source variable is not in the document. The query variable gets the mapped source.
    let vars = d["variables"].as_array().unwrap();
    assert_eq!(vars.len(), 1, "{vars:#?}");
    assert_eq!((vars[0]["name"].clone(), vars[0]["source"].clone()), (json!("database"), json!("prod")));
    // Each panel gets the mapped source: `${name}`, `$name`, `[[name]]` and the reference to the item of `__inputs`.
    let panels = d["sections"][0]["panels"].as_array().unwrap();
    assert_eq!(panels.len(), 4);
    for p in panels {
        assert_eq!(p["data"]["query"]["source"], "prod", "{p}");
    }
    // One `changed` item for the variable and one for the item of `__inputs`. The constant input is metadata.
    let at = |ptr: &str| r.items.iter().filter(|i| i.pointer == ptr).collect::<Vec<_>>();
    let var = at("/templating/list/0");
    assert_eq!(var.len(), 1);
    assert!(var[0].reason.contains("source prod") && var[0].reason.contains("regex \"\""), "{}", var[0].reason);
    assert!(var[0].reason.contains("multi false") && var[0].reason.contains("includeAll false"), "{}", var[0].reason);
    assert_eq!(at("/__inputs/0").len(), 1);
    assert!(at("/__inputs/1").is_empty());
    // The kind check of 15.9 sees the key of the map, not the reference text.
    let uses: Vec<&SourceUse> = r.source_uses.iter().collect();
    for key in ["datasource", "DS_CLICKHOUSE"] {
        assert!(
            uses.iter().any(|u| u.grafana == key && u.source == "prod" && u.kind == SourceKind::ClickHouse),
            "{uses:?}"
        );
    }
    check_golden("datasource_variable.txt", &golden_text(&r));
}

#[test]
fn a_data_source_variable_that_the_map_does_not_name_stops_the_dashboard() {
    let r = import_with(&fixture(), &[("DS_CLICKHOUSE", "prod")], true);
    assert_eq!(r.result, DashboardResult::NotImported);
    let var = r.items.iter().find(|i| i.pointer == "/templating/list/0").expect("the item of the variable");
    assert_eq!((var.class, var.fallback.clone()), (Class::Unsupported, Some(Fallback::DashboardNotImported)));
    assert!(var.reason.contains("does not name the data source variable \"datasource\""), "{}", var.reason);
    // The references to the variable: the panels and the query variable.
    let refs: Vec<_> = r.items.iter().filter(|i| i.reason == UNMAPPED).collect();
    assert!(refs.iter().any(|i| i.pointer == "/panels/0/targets/0/datasource"), "{refs:#?}");
    assert!(refs.iter().any(|i| i.pointer == "/panels/1/datasource"), "{refs:#?}");
    assert!(refs.iter().any(|i| i.pointer == "/templating/list/1/datasource"), "{refs:#?}");
    // The input that the map names still maps: its panel stays.
    assert!(!r.items.iter().any(|i| i.pointer.starts_with("/panels/3")), "{:#?}", r.items);
}

#[test]
fn an_unmapped_input_is_metadata_and_its_references_are_unsupported_with_the_new_reason() {
    let r = import_with(&fixture(), &[("datasource", "prod")], false);
    assert!(!r.items.iter().any(|i| i.pointer.starts_with("/__inputs")), "{:#?}", r.items);
    let item = r.items.iter().find(|i| i.pointer == "/panels/3/targets/0/datasource").expect("the panel item");
    // P00: the reason names the current state, not format 1.
    assert_eq!((item.reason.as_str(), item.fallback.clone()), (UNMAPPED, Some(Fallback::PanelRemoved)));
    // A reference to a name that is no data source variable and no item of `__inputs` is refused too.
    let mut d = fixture();
    d["panels"][0]["targets"][0]["datasource"]["uid"] = json!("${database}");
    let r = import_with(&d, &[("datasource", "prod"), ("DS_CLICKHOUSE", "prod"), ("database", "prod")], false);
    let item = r.items.iter().find(|i| i.pointer == "/panels/0/targets/0/datasource").expect("item");
    assert_eq!(item.reason, UNMAPPED);
}

#[test]
fn a_sql_text_that_names_the_data_source_variable_is_unsupported() {
    // In a panel: the panel is removed.
    let mut d = fixture();
    d["panels"][0]["targets"][0]["rawSql"] = json!("SELECT '$datasource' AS ds, count() AS c FROM system.tables");
    let r = import_with(&d, &MAP, false);
    let item = r.items.iter().find(|i| i.pointer == "/panels/0/targets/0/rawSql").expect("the SQL item");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    assert!(item.reason.contains("names the data source variable datasource"), "{}", item.reason);
    // The old form `[[name]]` and the item of `__inputs` too.
    d["panels"][0]["targets"][0]["rawSql"] = json!("SELECT 1 FROM t WHERE x = '${DS_CLICKHOUSE}'");
    let r = import_with(&d, &MAP, false);
    assert!(r.items.iter().any(|i| i.reason.contains("variable DS_CLICKHOUSE")), "{:#?}", r.items);
    d["panels"][0]["targets"][0]["rawSql"] = json!("SELECT 1 FROM [[datasource]].t");
    let r = import_with(&d, &MAP, false);
    assert!(r.items.iter().any(|i| i.reason.contains("variable datasource")), "{:#?}", r.items);
    // In a query variable: the dashboard is not imported.
    let mut d = fixture();
    d["templating"]["list"][1]["query"] = json!("SELECT name FROM system.databases WHERE '${datasource}' != ''");
    let r = import_with(&d, &MAP, true);
    assert_eq!(r.result, DashboardResult::NotImported, "{:#?}", r.items);
    let item = r.items.iter().find(|i| i.reason.contains("names the data source variable")).expect("item");
    assert_eq!(item.fallback, Some(Fallback::DashboardNotImported));
    // A variable whose name has the name of the data source variable as a prefix is another variable.
    let mut d = fixture();
    d["panels"][0]["targets"][0]["rawSql"] = json!("SELECT '$datasources' AS ds");
    assert!(!import_with(&d, &MAP, false).items.iter().any(|i| i.reason.contains("names the data source")));
}

#[test]
fn a_reference_with_no_type_takes_the_plugin_of_the_variable() {
    // The variable is a PostgreSQL data source variable: the old text reference gets the dialect `postgres`.
    let mut d = fixture();
    d["templating"]["list"][0]["query"] = json!("grafana-postgresql-datasource");
    d["templating"]["list"].as_array_mut().unwrap().truncate(1);
    d["panels"] = json!([d["panels"][2].clone()]);
    let r = import_with(&d, &MAP, false);
    assert!(
        r.source_uses.iter().any(|u| u.grafana == "datasource" && u.kind == SourceKind::Postgres),
        "{:?}",
        r.source_uses
    );
}

#[test]
fn the_annotations_take_the_mapped_source_and_refuse_the_variable_in_their_sql() {
    let annotation = |sql: &str| {
        json!({"datasource": {"type": "grafana-clickhouse-datasource", "uid": "${datasource}"}, "enable": true,
            "iconColor": "red", "name": "Deploys", "target": {"rawSql": sql, "format": 1}})
    };
    let mut d = fixture();
    d["annotations"] =
        json!({"list": [annotation("SELECT t AS time, 'x' AS text FROM deploys WHERE $__timeFilter(t)")]});
    let r = import_with(&d, &MAP, true);
    assert_eq!(doc(&r)["annotations"][0]["source"], "prod", "{:#?}", r.items);
    d["annotations"] = json!({"list": [annotation("SELECT t AS time, '$datasource' AS text FROM deploys")]});
    let r = import_with(&d, &MAP, true);
    assert!(doc(&r).get("annotations").is_none());
    let item = r.items.iter().find(|i| i.reason.contains("names the data source variable")).expect("item");
    assert_eq!(item.fallback, Some(Fallback::other("item removed")));
}

/// A v2 dashboard with a data source variable, a query variable and one panel that name it.
fn v2_dashboard(form: &str) -> Value {
    let beta = form == "beta1";
    let ds = if beta {
        json!({"name": "${datasource}"})
    } else {
        json!({"type": "grafana-clickhouse-datasource", "uid": "${datasource}"})
    };
    let query = |sql: &str, format: i64| {
        let spec = json!({"rawSql": sql, "editorType": "sql", "format": format});
        if beta {
            json!({"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0", "datasource": ds, "spec": spec})
        } else {
            json!({"kind": "grafana-clickhouse-datasource", "spec": spec})
        }
    };
    let mut variable = json!({"kind": "QueryVariable", "spec": {"name": "database", "label": "Database", "hide": "dontHide",
        "skipUrlSync": false, "multi": true, "includeAll": true, "allValue": "", "refresh": "onDashboardLoad",
        "sort": "disabled", "regex": "", "current": {}, "options": [], "allowCustomValue": false,
        "definition": "SELECT name FROM system.databases", "query": query("SELECT name FROM system.databases", 1)}});
    let mut panel_query = json!({"kind": "PanelQuery", "spec": {"query": query("SELECT count() AS c FROM t", 1), "refId": "A", "hidden": false}});
    if !beta {
        variable["spec"]["datasource"] = ds.clone();
        panel_query["spec"]["datasource"] = ds.clone();
    }
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{form}"),
        "kind": "Dashboard",
        "metadata": {"name": "dsv2", "namespace": "default", "resourceVersion": "1"},
        "spec": {"title": "V2 data source variable", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": [
                {"kind": "DatasourceVariable", "spec": {"name": "datasource", "pluginId": "grafana-clickhouse-datasource",
                    "refresh": "onDashboardLoad", "regex": "", "current": {"text": "", "value": ""}, "options": [],
                    "multi": false, "includeAll": false, "hide": "dontHide", "skipUrlSync": false,
                    "allowCustomValue": true, "label": "ClickHouse instance"}},
                variable
            ],
            "elements": {"panel-1": {"kind": "Panel", "spec": {"id": 1, "title": "Count", "description": "", "links": [],
                "data": {"kind": "QueryGroup", "spec": {"queries": [panel_query], "transformations": [], "queryOptions": {}}},
                "vizConfig": {"kind": "VizConfig", "group": "stat", "version": "12.4.2",
                    "spec": {"options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}}}}},
            "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {"x": 0, "y": 0,
                "width": 12, "height": 4, "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}
        }
    })
}

#[test]
fn the_v2_forms_of_the_data_source_variable() {
    for form in ["alpha1", "beta1"] {
        let r = import_with(&v2_dashboard(form), &[("datasource", "prod")], true);
        assert_eq!(r.result, DashboardResult::WouldWrite, "{form}: {:#?}", r.items);
        let d = doc(&r);
        assert_eq!(d["variables"].as_array().unwrap().len(), 1, "{form}");
        assert_eq!(d["variables"][0]["source"], "prod", "{form}");
        assert_eq!(first_panel(&d)["data"]["query"]["source"], "prod", "{form}");
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!((r.items[0].pointer.as_str(), r.items[0].class), ("/spec/variables/0", Class::Changed), "{form}");
        // Without the key, the variable stops the dashboard and the references are refused.
        let r = import_with(&v2_dashboard(form), &[("clickhouse", "prod")], true);
        assert_eq!(r.result, DashboardResult::NotImported, "{form}");
        assert!(r.items.iter().any(|i| i.reason == UNMAPPED), "{form}: {:#?}", r.items);
    }
}

// ------------------------------------------------------------------ D07: the format 2 of the plugin

#[test]
fn the_format_2_on_a_panel_that_is_not_a_logs_panel_becomes_table() {
    for ptype in ["timeseries", "table", "stat"] {
        let mut p = panel(1, ptype, 0);
        p["targets"][0]["format"] = json!(2);
        let r = run(&dash(vec![p], vec![]));
        assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table", "{ptype}");
        let item = one_item(&r, "targets.0.format");
        assert_eq!((item.class, item.value.as_deref()), (Class::Changed, Some("2")), "{ptype}");
    }
    // The v2 forms.
    for form in ["alpha1", "beta1"] {
        let mut d = v2_dashboard(form);
        d["spec"]["variables"] = json!([]);
        let q = &mut d["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][0]["spec"];
        if form == "beta1" {
            q["query"]["datasource"] = json!({"name": "clickhouse"});
        } else {
            q["datasource"] = json!({"type": "grafana-clickhouse-datasource", "uid": "clickhouse"});
        }
        q["query"]["spec"]["format"] = json!(2);
        let r = import_with(&d, &[("clickhouse", "prod")], true);
        assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table", "{form}: {:#?}", r.items);
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(r.items[0].class, Class::Changed, "{form}");
    }
    // The format 3 (traces) and 4 stay unsupported: the panel is removed.
    for format in [3, 4] {
        let mut p = panel(1, "table", 0);
        p["targets"][0]["format"] = json!(format);
        let r = run_plain(&dash(vec![p], vec![]));
        let item = one_item(&r, "targets.0.format");
        assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    }
}

// ------------------------------------------------------------------ D03: a panel that has the y of its row

fn row(id: i64, y: i64, collapsed: bool, inner: Vec<Value>) -> Value {
    json!({"id": id, "type": "row", "title": format!("Row {id}"), "collapsed": collapsed,
        "gridPos": {"h": 1, "w": 24, "x": 0, "y": y}, "panels": inner})
}

fn at_x(mut p: Value, x: i64) -> Value {
    p["gridPos"]["x"] = json!(x);
    p
}

fn grid_ys(d: &Value, section: usize) -> Vec<i64> {
    d["sections"][section]["panels"].as_array().unwrap().iter().map(|p| p["grid"]["y"].as_i64().unwrap()).collect()
}

fn row_items(r: &DashboardImport) -> Vec<&hune_grafana::ReportItem> {
    r.items.iter().filter(|i| i.reason.contains("the first panels of the row have the y of the row")).collect()
}

#[test]
fn an_expanded_row_with_panels_at_its_own_y_draws_them_on_the_first_line() {
    // The shape of the Advanced ClickHouse Monitoring Dashboard: a row at 0 and two panels at 0, then a panel at 4.
    let panels =
        vec![row(10, 0, false, vec![]), panel(1, "stat", 0), at_x(panel(2, "stat", 0), 12), panel(3, "stat", 4)];
    let r = run(&dash(panels, vec![]));
    let d = doc(&r);
    assert_eq!(grid_ys(&d, 0), vec![0, 0, 4]);
    let items = row_items(&r);
    assert_eq!(items.len(), 1, "{:#?}", r.items);
    assert_eq!((items[0].pointer.as_str(), items[0].class), ("/panels/0/gridPos/y", Class::Changed));
    assert!(r.items.iter().all(|i| i.class != Class::Unsupported), "{:#?}", r.items);
}

#[test]
fn a_collapsed_row_and_two_rows_follow_the_same_rule() {
    // A collapsed row at 0 holds a panel at 0. The second row at 72 has the old form: its first panel is at 73.
    let panels = vec![
        row(10, 0, true, vec![panel(1, "stat", 0), panel(2, "stat", 4)]),
        row(11, 72, false, vec![]),
        panel(3, "stat", 73),
        row(12, 80, false, vec![]),
        panel(4, "stat", 80),
        panel(5, "stat", 84),
    ];
    let r = run(&dash(panels, vec![]));
    let d = doc(&r);
    assert_eq!(grid_ys(&d, 0), vec![0, 4]);
    assert_eq!(grid_ys(&d, 1), vec![0]);
    assert_eq!(grid_ys(&d, 2), vec![0, 4]);
    let pointers: Vec<&str> = row_items(&r).iter().map(|i| i.pointer.as_str()).collect();
    assert_eq!(pointers, vec!["/panels/0/gridPos/y", "/panels/3/gridPos/y"]);
}

#[test]
fn a_panel_below_its_row_stays_unsupported() {
    let panels = vec![panel(1, "stat", 0), row(10, 5, false, vec![]), panel(2, "stat", 3), panel(3, "stat", 5)];
    let r = run_plain(&dash(panels, vec![]));
    assert!(row_items(&r).is_empty(), "{:#?}", r.items);
    let item = r.items.iter().find(|i| i.pointer == "/panels/2/gridPos/y").expect("the item of the panel");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::DashboardNotImported)));
    assert!(!r.importable);
}

// ------------------------------------------------------------------ bounded work

/// A dashboard with `n` data source variables and one panel whose SQL holds `n` placeholders of other names.
fn many_names(n: usize) -> Value {
    let vars: Vec<Value> = (0..n)
        .map(|i| json!({"type": "datasource", "name": format!("ds{i}"), "query": "grafana-clickhouse-datasource"}))
        .collect();
    let sql = format!(
        "SELECT 1 AS a FROM t WHERE x IN ('{}')",
        (0..n).map(|i| format!("$v{i}")).collect::<Vec<_>>().join("','")
    );
    let mut p = panel(1, "stat", 0);
    p["targets"][0]["rawSql"] = json!(sql);
    dash(vec![p], vars)
}

/// The minimum of 7 runs after 2 warm-ups (16.1), in seconds.
fn min_time(d: &Value) -> f64 {
    let o = ImportOptions::with_source_map(&[("clickhouse", "prod")]);
    (0..9)
        .map(|_| {
            let t = std::time::Instant::now();
            std::hint::black_box(import_dashboard(d, &o));
            t.elapsed().as_secs_f64()
        })
        .skip(2)
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn the_check_of_the_sql_against_the_data_source_names_is_linear() {
    // Linear work gives a ratio near 4 for 4 times the names and the placeholders. A loop over the names for each
    // placeholder scan gives a ratio near 16.
    let (small, large) = (min_time(&many_names(2_000)), min_time(&many_names(8_000)));
    assert!(large / small < 9.0, "small {small:.4}s, large {large:.4}s, ratio {:.1}", large / small);
}
