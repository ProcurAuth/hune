// SPDX-License-Identifier: MIT OR Apache-2.0
//! The rows of 15.6 for the MongoDB plugin (case I23): a fixture in each input form that holds the member.
//! The export is `tests/fixtures/grafana/export-mongo`.

mod common;

use common::*;
use hune_grafana::{
    Class, DashboardFile, DashboardImport, DashboardResult, Fallback, ImportInput, ImportOptions, ImportResult,
    MongoLists, ReportMeta, SourceKind, import_all, import_dashboard, parse_acceptance_file, render_report,
};
use serde_json::{Value, json};

fn mongo_dir() -> std::path::PathBuf {
    repo_root().join("tests/fixtures/grafana/export-mongo")
}

fn mongo_input() -> ImportInput {
    let dir = mongo_dir();
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

/// The options with the configuration (`-c`): the kinds and the lists of the block `events`.
fn with_config(accept: &str) -> ImportOptions {
    let mut o = without_config(accept);
    o.source_kinds.insert("events".to_owned(), SourceKind::MongoDb);
    o.source_kinds.insert("prod".to_owned(), SourceKind::ClickHouse);
    o.mongo_lists.insert(
        "events".to_owned(),
        MongoLists {
            databases: vec!["analytics".to_owned()],
            collections: vec!["orders".to_owned(), "node_load".to_owned()],
        },
    );
    o
}

/// The options without `-c`: the type of the Grafana data source gives the kind.
fn without_config(accept: &str) -> ImportOptions {
    let mut o = ImportOptions::with_source_map(&[("mongo", "events"), ("clickhouse", "prod")]);
    o.accept = parse_acceptance_file(accept).expect("acceptance");
    o
}

fn accept_all(r: &ImportResult) -> String {
    let mut ids: Vec<String> = r.all_items().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids.join("\n")
}

fn dry_run() -> ImportResult {
    import_all(&mongo_input(), &with_config("")).expect("import")
}

fn accepted_run() -> ImportResult {
    let accept = accept_all(&dry_run());
    import_all(&mongo_input(), &with_config(&accept)).expect("import")
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

/// The query text of a panel of the document, parsed: the object of 7.29.
fn query_of(doc: &Value, id: i64) -> Value {
    let lines = panel_of(doc, id).unwrap_or_else(|| panic!("panel {id} stays"))["data"]["query"]["sql"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&lines).unwrap_or_else(|e| panic!("panel {id}: {e}: {lines}"))
}

#[test]
fn each_mongodb_row_gives_its_item() {
    // I23: the items of the rows of 15.6, in the form v1 raw (the other forms follow in the next test).
    let r = dry_run();
    let d = by(&r, "mongo-main");
    let at = |panel: usize, member: &str| -> Vec<&hune_grafana::ReportItem> {
        d.items.iter().filter(|i| i.pointer == format!("/panels/{}/targets/0/{member}", panel - 1)).collect()
    };
    let none = |panel: usize| {
        let prefix = format!("/panels/{}/", panel - 1);
        assert!(d.items.iter().all(|i| !i.pointer.starts_with(&prefix)), "panel {panel}: {:#?}", d.items);
    };
    // Both queryType values and each macro of 7.34 map with no item.
    for panel in [1, 2, 3, 18] {
        none(panel);
    }
    // An absent queryType is changed to table.
    assert_eq!(at(4, "queryType")[0].class, Class::Changed);
    // The wrapper and the bare token are changed to the macros.
    let w = at(5, "queryText");
    assert_eq!((w.len(), w[0].class), (1, Class::Changed));
    assert!(w[0].reason.contains("wrapper"), "{}", w[0].reason);
    let b = at(6, "queryText");
    assert_eq!((b.len(), b[0].class), (1, Class::Changed));
    assert!(b[0].reason.contains("bare token"), "{}", b[0].reason);
    // A refused stage names the stage, and the panel is removed.
    let o = at(7, "queryText");
    assert_eq!((o[0].class, o[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    assert!(o[0].reason.contains("$out"), "{}", o[0].reason);
    // A $lookup outside the key `collections` (7.33).
    let l = at(8, "queryText");
    assert_eq!((l[0].class, l[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    assert!(l[0].reason.contains("collections"), "{}", l[0].reason);
    // The shell syntax: no JavaScript.
    let sh = at(9, "queryText");
    assert_eq!((sh[0].class, sh[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    assert!(sh[0].reason.contains("no shell syntax"), "{}", sh[0].reason);
    // An Extended JSON form that 7.28 refuses.
    let bin = at(10, "queryText");
    assert!(bin[0].reason.contains("$binary"), "{}", bin[0].reason);
    assert_eq!(bin[0].fallback, Some(Fallback::PanelRemoved));
    // A placeholder in a key.
    let k = at(11, "queryText");
    assert!(k[0].reason.contains("key"), "{}", k[0].reason);
    assert_eq!(k[0].fallback, Some(Fallback::PanelRemoved));
    // Another queryType.
    let q = at(12, "queryType");
    assert_eq!((q[0].class, q[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PanelRemoved)));
    // The old form [[name]] is changed.
    let o = at(13, "queryText");
    assert_eq!((o.len(), o[0].class), (1, Class::Changed));
    assert!(o[0].reason.contains("[[name]]"), "{}", o[0].reason);
    // A macro name in a longer string stays text: changed.
    let m = at(14, "queryText");
    assert_eq!((m.len(), m[0].class), (1, Class::Changed));
    // A MongoDB data source that the map does not name.
    assert_eq!(at(15, "datasource")[0].reason, "the data source unmapped-mongo is not in the source map");
    // A text that is not an array of stages.
    assert!(at(16, "queryText")[0].reason.contains("one JSON array of stages"));
    // A member of the SQL plugins is an unknown member of a MongoDB query.
    let m = at(17, "rawSql");
    assert_eq!((m[0].reason.as_str(), m[0].fallback.clone()), ("unknown member", Some(Fallback::MemberRemoved)));
    assert_eq!(r.exit_code(), 1);
}

#[test]
fn the_documents_hold_the_mongodb_queries() {
    let r = accepted_run();
    assert_eq!(r.exit_code(), 0, "{}", r.exit_reason());
    let doc = document(&r, "mongo-main");
    for id in [1, 2, 3, 4, 5, 6, 13, 14, 17, 18] {
        assert!(panel_of(&doc, id).is_some(), "panel {id} stays");
    }
    for id in [7, 8, 9, 10, 11, 12, 15, 16] {
        assert!(panel_of(&doc, id).is_none(), "panel {id} goes");
    }
    // The source ids and the formats.
    for id in [1, 2, 3, 4, 5, 6, 13, 14, 17] {
        assert_eq!(panel_of(&doc, id).unwrap()["data"]["query"]["source"], "events", "panel {id}");
    }
    assert_eq!(panel_of(&doc, 18).unwrap()["data"]["query"]["source"], "prod");
    let format = |id: i64| panel_of(&doc, id).unwrap()["data"]["query"]["format"].clone();
    assert_eq!((format(1), format(2), format(4)), (json!("table"), json!("time_series"), json!("table")));
    // The object of 7.29: the collection, the database when the plugin gives one, and the stages.
    let q1 = query_of(&doc, 1);
    assert_eq!(q1["collection"], "orders");
    assert!(q1.get("database").is_none());
    assert_eq!(q1["aggregate"][0]["$match"]["host"]["$in"], "${node:json}");
    assert_eq!(q1["aggregate"][0]["$match"]["ts"]["$gte"], "$__timeFrom");
    let q2 = query_of(&doc, 2);
    assert_eq!((q2["collection"].as_str(), q2["database"].as_str()), (Some("node_load"), Some("analytics")));
    assert_eq!(q2["aggregate"][1]["$group"]["_id"]["t"]["$dateTrunc"]["binSize"], "$__interval_s");
    assert_eq!(q2["aggregate"][1]["$group"]["_id"]["host"], "$host", "a field path stays");
    assert_eq!(query_of(&doc, 3)["aggregate"][1]["$limit"], "$__interval_ms");
    // The wrapper and the bare token give the macros.
    let q5 = query_of(&doc, 5);
    assert_eq!(q5["aggregate"][0]["$match"]["ts"], json!({"$gte": "$__timeFrom", "$lte": "$__timeTo"}));
    let q6 = query_of(&doc, 6);
    assert_eq!(q6["aggregate"][0]["$match"]["epoch_ms"], json!({"$gte": "$__from", "$lte": "$__to"}));
    // The old form is the new form.
    assert_eq!(query_of(&doc, 13)["aggregate"][0]["$match"]["host"], "${region}");
    // The lines: one for each key and one for each stage.
    let lines = panel_of(&doc, 1).unwrap()["data"]["query"]["sql"].as_array().unwrap().clone();
    assert_eq!(lines[0], "{");
    assert_eq!(lines[lines.len() - 1], "}");
    assert_eq!(lines.len(), 5 + 3, "one line for each key, one for each stage, and four marks");
    // The saved text passes the checks of the server for the lists of the block.
    for id in [1, 2, 3, 4, 5, 6, 13, 14, 17] {
        let text = panel_of(&doc, id).unwrap()["data"]["query"]["sql"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let defs = [
            hune_core::sql::VariableDef::custom("node", &["node-a", "node-b"], true, true, None),
            hune_core::sql::VariableDef::custom("region", &["eu", "us"], false, false, None),
        ];
        let ctx = hune_core::mongo::SaveContext {
            variables: &defs,
            collections: &["orders".to_owned(), "node_load".to_owned()],
            databases: &["analytics".to_owned()],
            usage: hune_core::sql::SqlUse::Panel,
        };
        let result = hune_core::mongo::save_checks(&text, &ctx);
        assert!(result.faults.is_empty(), "panel {id}: {:?}", result.faults);
    }
    // The document is canonical and the golden file agrees.
    let text = String::from_utf8(by(&r, "mongo-main").document_bytes.clone().unwrap()).unwrap();
    check_golden("mongo/documents/mongo-main.json", &text);
}

#[test]
fn the_four_forms_give_the_same_document_and_the_same_items() {
    // I23: a fixture in each input form that holds the member.
    let r = accepted_run();
    let raw = strip(document(&r, "mongo-main"), "mongo-main");
    for uid in ["mongo-wrapped", "mongo-v2a", "mongo-v2b"] {
        assert_eq!(strip(document(&r, uid), uid), raw, "{uid}");
    }
    let key = |d: &DashboardImport| {
        let mut v: Vec<(String, String, Option<i64>)> =
            d.items.iter().map(|i| (i.class.as_str().to_owned(), i.reason.clone(), i.panel_id)).collect();
        v.sort();
        v
    };
    let base = key(by(&r, "mongo-main"));
    assert!(base.len() >= 12, "{base:#?}");
    for uid in ["mongo-wrapped", "mongo-v2a", "mongo-v2b"] {
        assert_eq!(key(by(&r, uid)), base, "{uid}");
    }
}

#[test]
fn the_run_reports_the_kind_of_each_source_that_the_types_give() {
    // 15.9: the command compares these with the blocks of the configuration.
    let uses = dry_run().source_uses();
    assert!(uses.iter().any(|u| u.grafana == "mongo" && u.source == "events" && u.kind == SourceKind::MongoDb));
    assert!(uses.iter().any(|u| u.grafana == "clickhouse" && u.source == "prod" && u.kind == SourceKind::ClickHouse));
    assert_eq!(uses.len(), 2, "{uses:#?}");
}

#[test]
fn a_run_without_the_configuration_takes_the_kind_from_the_type_and_waits_for_the_lists() {
    let r = import_all(&mongo_input(), &without_config("")).expect("import");
    let d = by(&r, "mongo-main");
    // Without `-c` the importer knows no list: the $lookup of panel 8 is not a fault here (L8 waits for the save).
    assert!(d.items.iter().all(|i| !i.pointer.starts_with("/panels/7/")), "{:#?}", d.items);
    // The other faults stay.
    assert!(d.items.iter().any(|i| i.pointer == "/panels/6/targets/0/queryText" && i.reason.contains("$out")));
    let uses = r.source_uses();
    assert!(uses.iter().any(|u| u.source == "events" && u.kind == SourceKind::MongoDb), "{uses:#?}");
}

#[test]
fn a_reference_with_no_type_takes_the_kind_of_the_block() {
    let text = r#"[{"$match": {}}]"#;
    let mut p = json!({"id": 1, "type": "table", "title": "P", "datasource": "mongo",
        "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0},
        "targets": [{"datasource": "mongo", "refId": "A", "queryText": text, "collection": "orders", "queryType": "table"}],
        "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}});
    p["datasource"] = json!("mongo");
    let d = dash(vec![p], vec![]);
    // With the kind of the block, the query is a MongoDB query and the type gives no use to check.
    let mut o = with_config("");
    let with_kind = import_dashboard(&d, &o);
    assert_eq!(with_kind.items.len(), 0, "{:#?}", with_kind.items);
    assert!(with_kind.source_uses.is_empty(), "no type, no use to check");
    assert_eq!(first_panel(&doc_of(&with_kind))["data"]["query"]["source"], "events");
    // Without a kind the importer reads ClickHouse, as before: the MongoDB members are unknown members.
    o.source_kinds.clear();
    let without = import_dashboard(&d, &o);
    assert!(without.items.iter().any(|i| i.reason.contains("no rawSql")), "{:#?}", without.items);
}

fn doc_of(d: &DashboardImport) -> Value {
    serde_json::from_slice(d.document_bytes.as_ref().expect("a document")).expect("json")
}

#[test]
fn the_type_wins_over_the_kind_of_the_block() {
    // The type gives mongodb, the block is a ClickHouse block: the importer reads MongoDB, and the
    // command stops with exit code 2 because of `source_uses`.
    let panel = json!({"id": 1, "type": "table", "title": "P", "datasource": {"type": "haohanyang-mongodb-datasource", "uid": "mongo"},
        "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0},
        "targets": [{"datasource": {"type": "haohanyang-mongodb-datasource", "uid": "mongo"}, "refId": "A",
            "queryText": "[{\"$match\": {}}]", "collection": "orders", "queryType": "table"}],
        "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}});
    let mut o = ImportOptions::with_source_map(&[("mongo", "events")]);
    o.source_kinds.insert("events".to_owned(), SourceKind::ClickHouse);
    let d = import_dashboard(&dash(vec![panel], vec![]), &o);
    assert_eq!(d.source_uses[0].kind, SourceKind::MongoDb);
    assert_eq!(d.items.len(), 0, "{:#?}", d.items);
}

#[test]
fn a_query_variable_of_a_mongodb_source_is_not_imported() {
    let mut v = json!({"type": "query", "allowCustomValue": false, "name": "n", "label": "", "hide": 0, "refresh": 2, "sort": 1, "regex": "",
        "multi": false, "includeAll": false, "current": {}, "options": [], "skipUrlSync": false,
        "datasource": {"type": "haohanyang-mongodb-datasource", "uid": "mongo"},
        "definition": "[{\"$group\": {\"_id\": \"$host\"}}]", "query": "[{\"$group\": {\"_id\": \"$host\"}}]"});
    v["current"] = json!({"selected": false, "text": "a", "value": "a"});
    let d = import_dashboard(&dash(vec![], vec![v]), &with_config(""));
    assert_eq!(d.result, DashboardResult::Blocked, "{:#?}", d.items);
    let item = d.items.iter().find(|i| i.reason.contains("MongoDB data source")).expect("an item");
    assert_eq!(item.fallback, Some(Fallback::DashboardNotImported));
}

#[test]
fn the_report_equals_its_golden_file() {
    let meta = ReportMeta {
        hune_version: "0.1.3".to_owned(),
        created_at: "2026-11-20T09:12:44Z".to_owned(),
        input_dir: "/private/grafana-export-mongo".to_owned(),
        source_map: without_config("").source_map.clone(),
        link_prefix: None,
        folders_file: None,
        accept_file: None,
        write: false,
        replace: false,
    };
    let rendered = render_report(&dry_run(), &meta);
    check_golden("mongo/report-unaccepted.md", &rendered.markdown);
    assert_eq!(by(&dry_run(), "mongo-main").result, DashboardResult::Blocked);
}

#[test]
fn an_open_custom_variable_with_a_dollar_option_used_by_a_mongodb_query_stays_closed() {
    // 15.6: the engine refuses a free value that starts with `$` for MongoDB (PM2), so Grafana's
    // `allowCustomValue: true` is not kept for a `custom` variable that has such an option and that a MongoDB query uses.
    let mongo = json!({"type": "haohanyang-mongodb-datasource", "uid": "mongo"});
    let panel = |text: &str| {
        json!({"id": 1, "type": "table", "title": "P", "datasource": mongo,
            "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0},
            "targets": [{"datasource": mongo, "refId": "A", "queryText": text, "collection": "orders", "queryType": "table"}],
            "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}})
    };
    let var = |query: &str| {
        json!({"type": "custom", "name": "env", "query": query, "allowCustomValue": true,
            "current": {"value": "prod"}, "options": []})
    };
    let o = with_config("");
    let query = r#"[{"$match": {"env": "$env"}}]"#;
    // An option that starts with `$`: the variable is closed, with one `changed` item.
    let d = import_dashboard(&dash(vec![panel(query)], vec![var("prod,$dev")]), &o);
    let item = d.items.iter().find(|i| i.member.ends_with("allowCustomValue")).expect("an item");
    assert_eq!(item.class, Class::Changed);
    assert!(item.reason.contains("MongoDB"), "{}", item.reason);
    assert!(d.items.iter().all(|i| i.class != Class::Unsupported), "{:#?}", d.items);
    // No option that starts with `$`, or a query that does not name the variable: the variable stays open.
    for (text, options) in [(query, "prod,dev"), (r#"[{"$match": {"other": 1}}]"#, "prod,$dev")] {
        let d = import_dashboard(&dash(vec![panel(text)], vec![var(options)]), &o);
        assert!(d.items.iter().all(|i| !i.member.ends_with("allowCustomValue")), "{:#?}", d.items);
        let doc: Value = serde_json::from_slice(d.document_bytes.as_ref().expect("a document")).expect("json");
        assert_eq!(doc["variables"][0]["allow_custom_value"], json!(true));
    }
}

#[test]
fn d6_each_placeholder_form_of_a_mongodb_query_counts_as_a_use() {
    // D6 of the security review of 2026-11-16: the importer searched the text `$env`, so the forms `${env}` and
    // `${env:format}` (and `[[env]]`, which the importer rewrites to `${env}`) left the variable open. The search
    // also found `$envx`. The importer now reads the placeholders of 7.6 with the scan of `hune-core`.
    let mongo = json!({"type": "haohanyang-mongodb-datasource", "uid": "mongo"});
    let panel = |text: &str| {
        json!({"id": 1, "type": "table", "title": "P", "datasource": mongo,
            "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0},
            "targets": [{"datasource": mongo, "refId": "A", "queryText": text, "collection": "orders", "queryType": "table"}],
            "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}})
    };
    let vars = || {
        vec![
            json!({"type": "custom", "name": "env", "query": "prod,$dev", "allowCustomValue": true,
                "current": {"value": "prod"}, "options": []}),
            json!({"type": "custom", "name": "envx", "query": "a,b", "allowCustomValue": false,
                "current": {"value": "a"}, "options": []}),
        ]
    };
    let o = with_config("");
    let member = |d: &DashboardImport| -> Vec<(Class, String)> {
        d.items
            .iter()
            .filter(|i| i.member.ends_with("allowCustomValue") && i.pointer.starts_with("/templating/list/0/"))
            .map(|i| (i.class, i.reason.clone()))
            .collect()
    };
    for text in [
        r#"[{"$match": {"env": "$env"}}]"#,
        r#"[{"$match": {"env": "${env}"}}]"#,
        r#"[{"$match": {"env": "${env:text}"}}]"#,
        r#"[{"$match": {"n": "${env:number}"}}]"#,
        r#"[{"$match": {"env": "host-${env}"}}]"#,
        r#"[{"$match": {"env": "[[env]]"}}]"#,
    ] {
        let d = import_dashboard(&dash(vec![panel(text)], vars()), &o);
        let found = member(&d);
        assert_eq!(found.len(), 1, "{text}: {:#?}", d.items);
        assert_eq!(found[0].0, Class::Changed, "{text}");
        assert!(found[0].1.contains("MongoDB"), "{text}: {}", found[0].1);
    }
    // A longer name is another variable: `env` stays open.
    let d = import_dashboard(&dash(vec![panel(r#"[{"$match": {"env": "$envx"}}]"#)], vars()), &o);
    assert!(member(&d).is_empty(), "{:#?}", d.items);
    let doc: Value = serde_json::from_slice(d.document_bytes.as_ref().expect("a document")).expect("json");
    assert_eq!(doc["variables"][0]["allow_custom_value"], json!(true));
}
