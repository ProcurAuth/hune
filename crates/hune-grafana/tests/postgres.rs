// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the PostgreSQL rows of 15.6 (case I22). The fixture `tests/fixtures/grafana/export-pg` holds one
//! dashboard with a panel for each row, in the four input forms.

mod common;

use common::*;
use hune_core::sql::Dialect;
use hune_grafana::{
    Class, DashboardFile, DashboardImport, DashboardResult, Fallback, ImportInput, ImportOptions, ImportResult,
    ReportMeta, SourceKind, import_all, import_dashboard, parse_acceptance_file, render_report,
};
use serde_json::{Value, json};

fn pg_dir() -> std::path::PathBuf {
    repo_root().join("tests/fixtures/grafana/export-pg")
}

fn pg_input() -> ImportInput {
    let dir = pg_dir();
    let mut names: Vec<_> = std::fs::read_dir(dir.join("dashboards"))
        .expect("dashboards dir")
        .map(|e| e.expect("entry").file_name().into_string().expect("name"))
        .collect();
    names.sort();
    ImportInput {
        dashboards: names
            .iter()
            .map(|n| DashboardFile {
                name: format!("dashboards/{n}"),
                stem: n.trim_end_matches(".json").to_owned(),
                sha256: None,
                value: read_json(&dir.join("dashboards").join(n)),
            })
            .collect(),
        search: Some(read_json(&dir.join("search.json"))),
        ..ImportInput::default()
    }
}

fn pg_options(accept: &str) -> ImportOptions {
    let mut o = ImportOptions::with_source_map(&[("pg", "analytics"), ("clickhouse", "prod")]);
    o.accept = parse_acceptance_file(accept).expect("acceptance");
    o
}

/// The text of an acceptance file for each item of a run without acceptance.
fn accept_all(r: &ImportResult) -> String {
    let mut ids: Vec<String> = r.all_items().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids.join("\n")
}

fn dry_run() -> ImportResult {
    import_all(&pg_input(), &pg_options("")).expect("import")
}

fn accepted_run() -> ImportResult {
    let accept = accept_all(&dry_run());
    import_all(&pg_input(), &pg_options(&accept)).expect("import")
}

fn by<'a>(r: &'a ImportResult, uid: &str) -> &'a DashboardImport {
    r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap_or_else(|| panic!("no dashboard {uid}"))
}

fn document(r: &ImportResult, uid: &str) -> Value {
    let d = by(r, uid);
    serde_json::from_slice(d.document_bytes.as_ref().unwrap_or_else(|| panic!("no document for {uid}: {:#?}", d.items)))
        .expect("json")
}

fn panel_of(doc: &Value, id: i64) -> Option<&Value> {
    doc["sections"]
        .as_array()?
        .iter()
        .flat_map(|s| s["panels"].as_array().into_iter().flatten())
        .find(|p| p["id"] == id)
}

fn strip(mut v: Value, uid: &str) -> Value {
    assert_eq!(v["uid"], uid);
    v["uid"] = json!("x");
    v
}

#[test]
fn each_postgres_row_gives_its_item() {
    // I22: the items of the rows of 15.6, in the form v1 raw (the others follow in the next test).
    let r = dry_run();
    let d = by(&r, "pg-main");
    let at = |panel: usize, member: &str| -> Vec<&hune_grafana::ReportItem> {
        d.items.iter().filter(|i| i.pointer == format!("/panels/{panel}/targets/0/{member}")).collect()
    };
    // Panel 1 (a code query with the members of the plugin): metadata only, no item.
    assert!(d.items.iter().all(|i| !i.pointer.starts_with("/panels/0/")), "{:#?}", d.items);
    assert!(d.items.iter().all(|i| !i.pointer.starts_with("/panels/1/")), "{:#?}", d.items);
    // An absent format is changed to table.
    let f = at(2, "format");
    assert_eq!((f.len(), f[0].class), (1, Class::Changed));
    // Another format text: the panel is removed.
    let f = at(3, "format");
    assert_eq!((f[0].class, f[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    // The builder with a rawSql: the rawSql stays. Without a rawSql the panel is removed.
    let e = at(4, "editorMode");
    assert_eq!(e[0].class, Class::Unsupported);
    assert_eq!(
        e[0].fallback,
        Some(Fallback::other("the rawSql stays as the SQL text, and the builder members are removed"))
    );
    assert!(e[0].reason.contains("no query builder for PostgreSQL"));
    assert_eq!(at(5, "editorMode")[0].fallback, Some(Fallback::PanelRemoved));
    // The third argument of a group macro is removed.
    let t = at(6, "rawSql");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].fallback, Some(Fallback::other("the third argument is removed, and the panel shows the gaps")));
    // A macro of the dialect clickhouse: the panel is removed.
    let m = at(7, "rawSql");
    assert!(m.len() >= 2 && m.iter().all(|i| i.fallback == Some(Fallback::PanelRemoved)));
    assert!(m.iter().any(|i| i.reason.contains("$__timeFilter_ms is not a macro of Hune")), "{m:#?}");
    // An interval of 7.23 with two units: the panel is removed.
    let m = at(8, "rawSql");
    assert_eq!((m.len(), m[0].fallback.clone()), (1, Some(Fallback::PanelRemoved)));
    // A macro in a string stays text, and the item is changed.
    let m = at(9, "rawSql");
    assert_eq!((m.len(), m[0].class), (1, Class::Changed));
    // A PostgreSQL data source that the map does not name: the panel is removed.
    let m = at(12, "datasource");
    assert_eq!(m[0].reason, "the data source unmapped-pg is not in the source map");
    // The panels with no fault (the group macros with a literal and with $__interval, the ClickHouse panel) have no item.
    assert!(d.items.iter().all(|i| !i.pointer.starts_with("/panels/10/") && !i.pointer.starts_with("/panels/11/")));
    assert_eq!(r.exit_code(), 1);
}

#[test]
fn the_documents_hold_the_postgres_queries() {
    let r = accepted_run();
    assert_eq!(r.exit_code(), 0, "{}", r.exit_reason());
    let doc = document(&r, "pg-main");
    let query = |id: i64| panel_of(&doc, id).map(|p| p["data"]["query"].clone());
    // The panels that stay and the panels that go.
    for id in [1, 2, 3, 5, 7, 10, 11, 12] {
        assert!(panel_of(&doc, id).is_some(), "panel {id} stays");
    }
    for id in [4, 6, 8, 9, 13] {
        assert!(panel_of(&doc, id).is_none(), "panel {id} goes");
    }
    // The source ids: PostgreSQL panels to the block `analytics`, the ClickHouse panel to `prod`.
    for id in [1, 2, 3, 5, 7, 10, 11] {
        assert_eq!(query(id).unwrap()["source"], "analytics", "panel {id}");
    }
    assert_eq!(query(12).unwrap()["source"], "prod");
    // The format texts.
    assert_eq!(query(1).unwrap()["format"], "table");
    assert_eq!(query(2).unwrap()["format"], "time_series");
    assert_eq!(query(3).unwrap()["format"], "table", "an absent format gives table");
    assert_eq!(query(5).unwrap()["format"], "table");
    assert_eq!(query(11).unwrap()["format"], "time_series");
    // The macros stay as written.
    let sql = |id: i64| {
        query(id).unwrap()["sql"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect::<Vec<_>>().join("\n")
    };
    assert!(sql(2).contains("$__timeGroupAlias(ts, $__interval)"));
    assert!(sql(1).contains("$__timeFilter(ts) AND host IN ($host)"));
    assert!(sql(11).contains("$__unixEpochGroupAlias(epoch_s, '1h')"));
    assert!(sql(10).contains("'$__timeFrom()'"));
    // The third argument is gone.
    assert!(sql(7).contains("$__timeGroupAlias(ts, '5m')"), "{}", sql(7));
    assert!(!sql(7).contains(", 0"));
    // The builder panel keeps the SQL that the builder made, and no member of the builder.
    assert_eq!(sql(5), "SELECT host, avg(load) FROM metrics.node_load GROUP BY host");
    // The variable queries the PostgreSQL source.
    assert_eq!(doc["variables"][0]["source"], "analytics");
    // The document is canonical and the golden file agrees.
    let text = String::from_utf8(by(&r, "pg-main").document_bytes.clone().unwrap()).unwrap();
    check_golden("pg/documents/pg-main.json", &text);
}

#[test]
fn the_four_forms_give_the_same_document_and_the_same_items() {
    // I22: a fixture in each input form that holds the member.
    let r = accepted_run();
    let raw = strip(document(&r, "pg-main"), "pg-main");
    for uid in ["pg-wrapped", "pg-v2a", "pg-v2b"] {
        assert_eq!(strip(document(&r, uid), uid), raw, "{uid}");
    }
    let key = |d: &DashboardImport| {
        let mut v: Vec<(String, String, Option<i64>)> =
            d.items.iter().map(|i| (i.class.as_str().to_owned(), i.reason.clone(), i.panel_id)).collect();
        v.sort();
        v
    };
    let base = key(by(&r, "pg-main"));
    assert!(base.len() >= 10, "{base:#?}");
    for uid in ["pg-wrapped", "pg-v2a", "pg-v2b"] {
        assert_eq!(key(by(&r, uid)), base, "{uid}");
    }
}

#[test]
fn the_run_reports_the_kind_of_each_source_that_the_types_give() {
    // 15.9: the command compares these with the blocks of the configuration.
    let r = dry_run();
    let uses = r.source_uses();
    assert!(uses.iter().any(|u| u.grafana == "pg" && u.source == "analytics" && u.kind == SourceKind::Postgres));
    assert!(uses.iter().any(|u| u.grafana == "clickhouse" && u.source == "prod" && u.kind == SourceKind::ClickHouse));
    assert_eq!(uses.len(), 2, "{uses:#?}");
}

#[test]
fn the_report_equals_its_golden_file() {
    let meta = ReportMeta {
        hune_version: "0.3.2".to_owned(),
        created_at: "2026-11-20T09:12:44Z".to_owned(),
        input_dir: "/private/grafana-export-pg".to_owned(),
        source_map: pg_options("").source_map.clone(),
        link_prefix: None,
        folders_file: None,
        accept_file: None,
        write: false,
        replace: false,
    };
    let rendered = render_report(&dry_run(), &meta);
    check_golden("pg/report-unaccepted.md", &rendered.markdown);
    assert_eq!(by(&dry_run(), "pg-main").result, DashboardResult::Blocked);
}

// ---------------------------------------------------------------- small dashboards

fn pg_target(extra: Value) -> Value {
    let mut t = json!({"datasource": {"type": "grafana-postgresql-datasource", "uid": "pg"}, "refId": "A", "editorMode": "code",
        "rawQuery": true, "format": "table", "rawSql": "SELECT 1 AS a"});
    for (k, v) in extra.as_object().unwrap() {
        if v.is_null() {
            t.as_object_mut().unwrap().remove(k);
        } else {
            t[k] = v.clone();
        }
    }
    t
}

fn pg_panel(target: Value) -> Value {
    json!({"id": 1, "type": "table", "title": "P", "datasource": target["datasource"].clone(),
        "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0}, "targets": [target],
        "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}})
}

fn run_pg(panel: Value, kinds: &[(&str, Dialect)]) -> DashboardImport {
    let d = dash(vec![panel], vec![]);
    let mut o = ImportOptions::with_source_map(&[("pg", "analytics"), ("clickhouse", "prod"), ("old-name", "legacy")]);
    o.source_kinds = kinds.iter().map(|(k, v)| ((*k).to_owned(), (*v).into())).collect();
    let first = import_dashboard(&d, &o);
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    import_dashboard(&d, &o)
}

#[test]
fn the_old_type_postgres_gives_a_postgres_source() {
    let t = pg_target(json!({"datasource": {"type": "postgres", "uid": "pg"}}));
    let d = run_pg(pg_panel(t), &[]);
    no_items(&d);
    assert_eq!(first_panel(&doc(&d))["data"]["query"]["source"], "analytics");
    assert_eq!(d.source_uses.len(), 1);
    assert_eq!(d.source_uses[0].kind, SourceKind::Postgres);
}

#[test]
fn a_reference_with_no_type_takes_the_dialect_of_the_block() {
    // An old v1 reference is a data source name (a text). The configuration gives the kind.
    let sql = "SELECT $__timeGroupAlias(ts, '1h', 0) FROM t";
    let t = pg_target(json!({"datasource": "pg", "rawSql": sql, "format": null}));
    let mut p = pg_panel(t);
    p["datasource"] = json!("pg");
    let with_kind = run_pg(p.clone(), &[("analytics", Dialect::Postgres)]);
    assert_eq!(
        first_panel(&doc(&with_kind))["data"]["query"]["sql"],
        json!(["SELECT $__timeGroupAlias(ts, '1h') FROM t"])
    );
    assert!(with_kind.source_uses.is_empty(), "no type, no use to check");
    // Without a kind the importer reads ClickHouse, as before, and a PostgreSQL macro is no macro there.
    let without = import_dashboard(&dash(vec![p], vec![]), &ImportOptions::with_source_map(&[("pg", "analytics")]));
    assert!(without.items.iter().any(|i| i.reason.contains("not a macro")), "{:#?}", without.items);
}

#[test]
fn the_type_wins_over_the_kind_of_the_block() {
    // The type gives postgres, the block is a ClickHouse block: the importer reads PostgreSQL, and the
    // command stops with exit code 2 because of `source_uses`.
    let t = pg_target(json!({"rawSql": "SELECT $__timeGroup(ts, '1h', 0) FROM t"}));
    let d = run_pg(pg_panel(t), &[("analytics", Dialect::ClickHouse)]);
    assert_eq!(d.source_uses[0].kind, SourceKind::Postgres);
    assert_eq!(first_panel(&doc(&d))["data"]["query"]["sql"], json!(["SELECT $__timeGroup(ts, '1h') FROM t"]));
}

#[test]
fn a_format_that_is_not_a_text_is_unsupported() {
    for bad in [json!(1), json!(0), json!("logs"), json!("trace"), json!("")] {
        let d = run_pg_plain(pg_panel(pg_target(json!({"format": bad.clone()}))));
        let item = one_item(&d, "targets.0.format");
        assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)), "{bad}");
    }
}

fn run_pg_plain(panel: Value) -> DashboardImport {
    import_dashboard(&dash(vec![panel], vec![]), &ImportOptions::with_source_map(&[("pg", "analytics")]))
}

#[test]
fn the_members_of_the_other_plugin_are_unknown_members() {
    // A ClickHouse member in a PostgreSQL query, and a PostgreSQL member in a ClickHouse query.
    let d = run_pg_plain(pg_panel(pg_target(json!({"editorType": "sql", "builderOptions": {"mode": "list"}}))));
    assert!(
        d.items.iter().any(|i| i.reason == "unknown member" && i.pointer.ends_with("/editorType")),
        "{:#?}",
        d.items
    );
    let mut t = target("SELECT 1 AS a");
    t["editorMode"] = json!("code");
    t["table"] = json!("t");
    let d = run_plain(&dash(
        vec![{
            let mut p = panel(1, "table", 0);
            p["targets"] = json!([t]);
            p
        }],
        vec![],
    ));
    assert!(
        d.items.iter().any(|i| i.reason == "unknown member" && i.pointer.ends_with("/editorMode")),
        "{:#?}",
        d.items
    );
    assert!(d.items.iter().any(|i| i.reason == "unknown member" && i.pointer.ends_with("/table")), "{:#?}", d.items);
}

#[test]
fn a_builder_with_a_blank_sql_removes_the_panel() {
    for sql in [json!(""), json!("  \n "), json!(null)] {
        let d = run_pg_plain(pg_panel(pg_target(
            json!({"editorMode": "builder", "rawQuery": false, "rawSql": sql.clone()}),
        )));
        let item = one_item(&d, "targets.0.editorMode");
        assert_eq!(item.fallback, Some(Fallback::PanelRemoved), "{sql}");
    }
    let d = run_pg_plain(pg_panel(pg_target(json!({"editorMode": "other"}))));
    assert_eq!(one_item(&d, "targets.0.editorMode").fallback, Some(Fallback::PanelRemoved));
}

#[test]
fn a_third_argument_is_removed_in_code_only_and_in_each_group_macro() {
    let sql = "SELECT $__timeGroup(a, '1m', NULL), $__timeGroupAlias(b, $__interval, 0), $__unixEpochGroup(c, '1h', 1),\n       $__unixEpochGroupAlias(d, '1d', previous), '$__timeGroup(x, y, z)' AS s -- $__timeGroup(x, y, z)\nFROM t";
    let d = run_pg(pg_panel(pg_target(json!({"rawSql": sql}))), &[]);
    assert_eq!(
        first_panel(&doc(&d))["data"]["query"]["sql"],
        json!([
            "SELECT $__timeGroup(a, '1m'), $__timeGroupAlias(b, $__interval), $__unixEpochGroup(c, '1h'),",
            "       $__unixEpochGroupAlias(d, '1d'), '$__timeGroup(x, y, z)' AS s -- $__timeGroup(x, y, z)",
            "FROM t"
        ])
    );
}

#[test]
fn the_old_double_bracket_form_is_rewritten_in_a_postgres_query() {
    let mut vars = json!({"type": "custom", "allowCustomValue": false, "name": "host", "query": "a,b", "current": {"text": "a", "value": "a"}, "options": []});
    vars["multi"] = json!(false);
    let d = dash(
        vec![pg_panel(pg_target(json!({"rawSql": "SELECT '[[host]]' AS h, [[host]] AS g, E'[[host]]' AS e"})))],
        vec![vars],
    );
    let o = ImportOptions::with_source_map(&[("pg", "analytics")]);
    let first = import_dashboard(&d, &o);
    assert!(first.items.iter().any(|i| i.reason.contains("[[name]] is rewritten to ${name}")), "{:#?}", first.items);
}
