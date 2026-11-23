//! The MongoDB source in the API: the cases A52 to A56 of 16.8, the dispatch by the kind of the
//! source, the errors of 6.8 and the log line of a query.
//!
//! The tests use the fake client of `hune-source` through `test_connectors`: the build has no driver
//! (8b.2) and they need no MongoDB server. The tests with a real server are in the ignored cases
//! of `tests/ci/mongosource.sh`.

use super::api::{JSON, NODES, etag_of, fill, key, post_body, put};
use super::authzen_pdp::*;
use super::harness::*;
use super::pg_source::doc_on;
use super::query::{draft_panel, panel_body, post_preview, preview_body};
use crate::server::limits::Timing;
use crate::server::sources::test_connectors;
use hune_core::ids::SourceId;
use hune_core::mongo::ReadPreference;
use hune_source::FakeMongoReply as Reply;
use hune_source::{FakeMongo, MongoError, MongoErrorClass, MongoSourceConfig, MongoTlsMode, MongoTopology};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

const ORDERS: &str = r#"{"collection":"orders","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom","$lte":"$__timeTo"}}},{"$project":{"_id":0,"host":1,"load":1}}]}"#;

/// The settings of a `mongodb` block that no server serves: the tests give it a fake client.
pub(super) fn mongo_block(id: &str, label: &str) -> MongoSourceConfig {
    MongoSourceConfig {
        id: SourceId::parse(id).unwrap(),
        label: label.to_owned(),
        topology: MongoTopology::Direct { host: "mongo-host-marker".to_owned(), port: 27017 },
        user: "mongo-user-marker".to_owned(),
        password: "mongo-password-marker".to_owned(),
        auth_database: "admin".to_owned(),
        databases: vec!["analytics-marker".to_owned()],
        collections: vec!["orders".to_owned(), "node_load".to_owned()],
        read_preference: ReadPreference::Primary,
        tls_mode: MongoTlsMode::Disable,
        tls: None,
        tls_ca_file: None,
        connect_timeout: Duration::from_secs(2),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 50_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(1),
        max_concurrent_per_subject: None,
    }
}

fn probing() -> Timing {
    Timing { probe_sources: true, ..test_timing() }
}

/// Starts a server with the block `id` and a fake client for it. The probe runs at once.
async fn start_mongo(id: &str, edit: impl FnOnce(&mut crate::config::Config)) -> (Harness, FakeMongo) {
    let block = mongo_block(id, "Events");
    let fake = FakeMongo::for_config(&block);
    test_connectors::put(id, fake.connector());
    let h = Harness::start_with(probing(), fill, |c| {
        c.mongo_sources.push(block);
        edit(c);
    })
    .await;
    wait_up(&h, id).await;
    (h, fake)
}

async fn wait_up(h: &Harness, id: &str) {
    for _ in 0..200 {
        if get(h.ops, "/readyz").await.json()["sources"][id] == "up" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the source {id} never got the state up");
}

fn log(h: &Harness) -> Vec<Value> {
    h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn runs(h: &Harness) -> Vec<Value> {
    log(h).into_iter().filter(|v| v["event"] == "query.run").collect()
}

/// Creates a dashboard on the source `events` and gives its etag.
async fn create(h: &Harness, uid: &str, source: &str, sql: &[&str]) -> String {
    let doc = doc_on(uid, source, "table", sql);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{uid}: {}", r.text());
    etag_of(&r)
}

/// Creates a dashboard whose only variable is the `text` variable `account`.
async fn create_with_account(h: &Harness, uid: &str, source: &str, sql: &str) -> String {
    let mut doc: Value = serde_json::from_str(&doc_on(uid, source, "table", &[sql])).unwrap();
    doc["variables"] = json!([{
        "kind": "text", "name": "account", "label": null, "hide": "none", "value_type": "text", "default": ""
    }]);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{uid}: {}", r.text());
    etag_of(&r)
}

async fn run_panel(h: &Harness, uid: &str, etag: &str) -> Resp {
    let url = format!("/api/dashboards/{uid}/panels/1/query");
    call(h.addr, "POST", &url, &[JSON], &panel_body(etag, json!({"variables": {}}))).await
}

fn pointers(r: &Resp) -> Vec<(String, String)> {
    r.json()["error"]["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| (e["pointer"].as_str().unwrap().to_owned(), e["code"].as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a52_a_panel_query_on_a_mongodb_source_gives_the_frame_of_7_16c() {
    let (h, fake) = start_mongo("events52", |_| {}).await;
    fake.push_read(Reply::docs(&[
        r#"{"host":"node-a","load":1.5,"n":{"$numberLong":"7"}}"#,
        r#"{"host":"node-b","load":2.5,"n":{"$numberLong":"8"}}"#,
    ]));
    let etag = create(&h, "mgtable", "events52", &[ORDERS]).await;
    let r = run_panel(&h, "mgtable", &etag).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["rows"], 2);
    let names: Vec<&str> = j["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["host", "load", "n"]);
    assert_eq!(j["columns"][0]["kind"], "text");
    assert_eq!(j["columns"][1]["kind"], "number");
    assert_eq!(j["values"][0], json!(["node-a", "node-b"]));
    // The command is the typed tree: the macros are Dates, and the text of the query holds no `$__`.
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    let sent = format!("{:?}", requests[0]);
    assert!(!sent.contains("$__"), "{sent}");
    let line = &runs(&h)[0];
    assert_eq!((line["source_kind"].as_str(), line["outcome"].as_str()), (Some("mongodb"), Some("ok")));
    assert!(line["clickhouse_code"].is_null() && line["sqlstate"].is_null() && line["mongo_code"].is_null());
    assert_eq!(line["sql_sha256"].as_str().map(str::len), Some(64));
}

#[tokio::test]
async fn a52_a_dashboard_with_a_panel_on_each_of_the_three_kinds_is_saved_and_read_back() {
    let (h, _fake) = start_mongo("events52b", |c| {
        c.postgres_sources.push(super::pg_source::pg_block("metrics", "Metrics"));
    })
    .await;
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!("threekinds");
    doc["title"] = json!("Three kinds");
    for (next_id, (source, sql)) in (90..).zip([("metrics", "SELECT 1 AS n"), ("events52b", ORDERS)]) {
        let raw: Value = serde_json::from_str(&doc_on("x", source, "table", &[sql])).unwrap();
        let mut panel = raw["sections"][0]["panels"][0].clone();
        panel["id"] = json!(next_id);
        panel["grid"]["y"] = json!(next_id * 10);
        doc["sections"][0]["panels"].as_array_mut().unwrap().push(panel);
    }
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let got = get(h.addr, "/api/dashboards/threekinds").await.json();
    let sources: Vec<&str> = got["document"]["sections"][0]["panels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["data"]["query"]["source"].as_str())
        .collect();
    for want in ["prod", "metrics", "events52b"] {
        assert!(sources.contains(&want), "{want}: {sources:?}");
    }
}

#[tokio::test]
async fn a53_the_sources_list_has_the_kind_mongodb_and_no_host_user_database_collection_or_secret() {
    let (h, _fake) = start_mongo("events53", |_| {}).await;
    let r = get(h.addr, "/api/sources").await;
    assert_eq!(r.status, 200);
    let items = r.json()["items"].clone();
    assert_eq!(items.as_array().unwrap().len(), 2);
    assert_eq!(items[0], json!({"id": "events53", "label": "Events", "kind": "mongodb", "state": "up"}));
    assert_eq!((items[1]["id"].as_str(), items[1]["kind"].as_str()), (Some("prod"), Some("clickhouse")));
    assert_eq!(items[0].as_object().unwrap().len(), 4);
    let text = r.text();
    for secret in [
        "mongo-host-marker",
        "mongo-user-marker",
        "mongo-password-marker",
        "analytics-marker",
        "node_load",
        "orders",
        "27017",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    let ready = get(h.ops, "/readyz").await.json();
    assert_eq!(ready["sources"]["events53"], "up");
}

#[tokio::test]
async fn a_mongodb_source_of_a_build_with_no_driver_stays_misconfigured_with_the_fixed_text() {
    // No connector is registered for this id: the build has no driver.
    let h = Harness::start_with(probing(), fill, |c| c.mongo_sources.push(mongo_block("nodriver", "No driver"))).await;
    let listed = get(h.addr, "/api/sources").await.json();
    let item = listed["items"].as_array().unwrap().iter().find(|i| i["id"] == "nodriver").unwrap().clone();
    assert_eq!((item["kind"].as_str(), item["state"].as_str()), (Some("mongodb"), Some("misconfigured")));
    let etag = create(&h, "nodrv", "nodriver", &[ORDERS]).await;
    let r = run_panel(&h, "nodrv", &etag).await;
    assert_eq!((r.status, r.code().as_str()), (502, "source_misconfigured"), "{}", r.text());
    assert_eq!(r.json()["error"]["source"], "nodriver");
    assert!(r.text().contains("driver"), "{}", r.text());
    // The query never reached the source: no `query.run` line.
    assert!(runs(&h).is_empty());
    let all = h.logger.memory_lines().join("\n");
    for needle in ["mongo-password-marker", "mongo-user-marker", "mongo-host-marker"] {
        assert!(!all.contains(needle), "{needle}");
    }
}

/// The set of 7.36 that every route refuses. Each text breaks the rules of 7.28 to 7.33.
fn refused_set() -> Vec<(&'static str, String)> {
    let top = |pipeline: &str| format!(r#"{{"collection":"orders","aggregate":{pipeline}}}"#);
    vec![
        ("$out", top(r#"[{"$out":"stolen"}]"#)),
        ("$merge", top(r#"[{"$merge":{"into":"stolen"}}]"#)),
        ("$where", top(r#"[{"$match":{"$where":"sleep(1000)"}}]"#)),
        (
            "$function",
            top(r#"[{"$addFields":{"x":{"$function":{"body":"function(){return 1}","args":[],"lang":"js"}}}}]"#),
        ),
        (
            "a $lookup outside the list",
            top(r#"[{"$lookup":{"from":"secrets","localField":"a","foreignField":"b","as":"c"}}]"#),
        ),
        ("a duplicate key", top(r#"[{"$match":{"a":1,"a":2}}]"#)),
        ("JavaScript text", "function () { return db.orders.find(); }".to_owned()),
        ("a placeholder in a key", top(r#"[{"$match":{"${host}":1}}]"#)),
    ]
}

#[tokio::test]
async fn a54_save_create_and_preview_refuse_the_queries_that_break_7_28_to_7_33() {
    let (h, _fake) = start_mongo("events54", |_| {}).await;
    let e0 = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    for (name, text) in refused_set() {
        // The save: 422 with the pointer of the line.
        let doc = doc_on("nodes", "events54", "table", &[text.as_str()]);
        let r = put(&h, "nodes", &e0, &doc, &key()).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        let list = pointers(&r);
        assert!(list.iter().any(|(p, _)| p == "/sections/0/panels/0/data/query/sql/0"), "{name}: {list:?}");
        // The create, with `validate_only`.
        let doc = doc_on("created", "events54", "table", &[text.as_str()]);
        let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &post_body("transit", &doc)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        // The preview: the pointer is in the body.
        let mut panel = draft_panel(&text);
        panel["data"]["query"]["source"] = json!("events54");
        panel["data"]["query"]["format"] = json!("table");
        let r = post_preview(&h, "events54", &preview_body(json!({"panel": panel}))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/panel/data/query/sql/0", "{name}");
        // The same text on a `clickhouse` source follows 7.12: it is no SELECT.
        let doc = doc_on("nodes", "prod", "table", &[text.as_str()]);
        let r = put(&h, "nodes", &e0, &doc, &key()).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name} on prod: {}", r.text());
    }
    // A text that follows the grammar is saved, with a variable placeholder in a value.
    let ok = doc_on("nodes", "events54", "table", &[ORDERS]);
    let r = put(&h, "nodes", &e0, &ok, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The collection of an unlisted name is the item L8.
    let e1 = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    let other = ORDERS.replace("orders", "unlisted");
    let r = put(&h, "nodes", &e1, &doc_on("nodes", "events54", "table", &[other.as_str()]), &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r)[0].1, "bad_reference");
}

#[tokio::test]
async fn a54_a_restore_refuses_a_version_that_breaks_7_28_to_7_33() {
    let bad = doc_on("restored", "events54r", "table", &[r#"{"collection":"orders","aggregate":[{"$out":"x"}]}"#]);
    let block = mongo_block("events54r", "Events");
    test_connectors::put("events54r", FakeMongo::for_config(&block).connector());
    let h = Harness::start_with(
        probing(),
        |root| {
            fill(root);
            std::fs::write(root.join("transit/restored.json"), &bad).unwrap();
        },
        |c| c.mongo_sources.push(block),
    )
    .await;
    // Version 1 is the file. A good save makes version 2.
    let e0 = etag_of(&get(h.addr, "/api/dashboards/restored").await);
    let good = doc_on("restored", "events54r", "table", &[ORDERS]);
    let r = put(&h, "restored", &e0, &good, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let e1 = etag_of(&r);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/restored/restore",
        &[JSON, ("If-Match", &format!("\"{e1}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r)[0].0, "/sections/0/panels/0/data/query/sql/0");
}

#[tokio::test]
async fn the_save_warnings_of_7_36_come_in_the_answer() {
    let (h, _fake) = start_mongo("events54w", |_| {}).await;
    // The warning 1: a `${name}` that is no variable of the document.
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"host":"${nope}"}}]}"#;
    let doc = doc_on("warned", "events54w", "table", &[text]);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let warnings = r.json()["warnings"].as_array().unwrap().clone();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0]["pointer"], "/sections/0/panels/0/data/query/sql/0");
}

#[tokio::test]
async fn the_expansion_and_the_size_limit_stop_a_mongodb_query_before_the_source() {
    let (h, fake) = start_mongo("events52s", |c| c.mongo_sources[0].max_query_size = 200).await;
    let long = format!(r#"{{"collection":"orders","aggregate":[{{"$match":{{"host":"{}"}}}}]}}"#, "x".repeat(300));
    let etag = create(&h, "mgsize", "events52s", &[long.as_str()]).await;
    let r = run_panel(&h, "mgsize", &etag).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_too_large"), "{}", r.text());
    assert_eq!(r.json()["error"]["limit"], 200);
    assert!(r.json()["error"]["size"].as_u64().unwrap() > 200);
    assert!(fake.requests().is_empty());
    assert!(runs(&h).is_empty());
}

#[tokio::test]
async fn a_variable_value_that_breaks_a_format_gives_variable_value_refused() {
    let (h, fake) = start_mongo("events52v", |_| {}).await;
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"n":"${account:number}"}}]}"#;
    let etag = create_with_account(&h, "mgvar", "events52v", text).await;
    let url = "/api/dashboards/mgvar/panels/1/query";
    let body = panel_body(&etag, json!({"variables": {"account": {"all": false, "values": ["12abc"]}}}));
    let r = call(h.addr, "POST", url, &[JSON], &body).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert_eq!(r.json()["error"]["variable"], "account");
    assert!(fake.requests().is_empty());
    // A field path from a free value is refused (PM2).
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"$expr":{"$eq":["$host","${account}"]}}}]}"#;
    let etag = create_with_account(&h, "mgpath", "events52v", text).await;
    let body = panel_body(&etag, json!({"variables": {"account": {"all": false, "values": ["$secret"]}}}));
    let r = call(h.addr, "POST", "/api/dashboards/mgpath/panels/1/query", &[JSON], &body).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert_eq!(r.json()["error"]["reason"], "The value changes the structure of the query.");
    // A good value runs.
    fake.push_read(Reply::docs(&[r#"{"host":"a"}"#]));
    let body = panel_body(&etag, json!({"variables": {"account": {"all": false, "values": ["node-a"]}}}));
    let r = call(h.addr, "POST", "/api/dashboards/mgpath/panels/1/query", &[JSON], &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn the_preview_of_a_panel_and_of_a_variable_on_a_mongodb_source_runs_the_expansion() {
    let (h, fake) = start_mongo("events52p", |_| {}).await;
    fake.push_read(Reply::docs(&[r#"{"host":"a","load":1}"#]));
    let mut panel = draft_panel(ORDERS);
    panel["data"]["query"]["source"] = json!("events52p");
    panel["data"]["query"]["format"] = json!("table");
    let r = post_preview(&h, "events52p", &preview_body(json!({"panel": panel}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 1);
    assert!(r.json().get("sql").is_none(), "server.query_inspect is false");
    // The options of a draft variable: the first column is the value.
    fake.push_read(Reply::docs(&[r#"{"host":"node-a"}"#, r#"{"host":"node-b"}"#]));
    let variable = json!({
        "kind": "query", "name": "node", "label": null, "hide": "none", "source": "events52p",
        "sql": [r#"{"collection":"orders","aggregate":[{"$group":{"_id":"$host"}},{"$project":{"_id":0,"host":"$_id"}}]}"#],
        "multi": false, "include_all": false, "all_value": null, "refresh": "load", "sort": "alpha_asc",
        "regex": null, "default": ["node-a"]
    });
    let body = json!({
        "uid": "nodes", "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "dashboard_variables": [variable], "variables": {"node": {"all": false, "values": ["node-a"]}},
        "variable": "node"
    });
    let r = post_preview(&h, "events52p", &serde_json::to_vec(&body).unwrap()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let answer = r.json();
    let values: Vec<&str> =
        answer["options"].as_array().unwrap().iter().map(|o| o["value"].as_str().unwrap()).collect();
    assert_eq!(values, ["node-a", "node-b"]);
    // A variable query cannot hold an interval macro (L9): the draft is refused.
    let mut bad = body.clone();
    bad["dashboard_variables"][0]["sql"] =
        json!([r#"{"collection":"orders","aggregate":[{"$limit":"$__interval_ms"}]}"#]);
    let r = post_preview(&h, "events52p", &serde_json::to_vec(&bad).unwrap()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
}

#[tokio::test]
async fn the_options_route_of_a_saved_variable_on_a_mongodb_source_works() {
    let (h, fake) = start_mongo("events52o", |_| {}).await;
    let mut doc: Value = serde_json::from_str(&doc_on("mgopts", "events52o", "table", &[ORDERS])).unwrap();
    doc["variables"] = json!([{
        "kind": "query", "name": "node", "label": null, "hide": "none", "source": "events52o",
        "sql": [r#"{"collection":"node_load","aggregate":[{"$group":{"_id":"$host"}},{"$project":{"_id":0,"host":"$_id"}}]}"#],
        "multi": false, "include_all": false, "all_value": null, "refresh": "load", "sort": "alpha_asc",
        "regex": null, "default": ["node-a"]
    }]);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let etag = etag_of(&r);
    fake.push_read(Reply::docs(&[r#"{"host":"node-a"}"#, r#"{"host":"node-b"}"#]));
    let body = json!({
        "etag": etag, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "variables": {"node": {"all": false, "values": ["node-a"]}}
    });
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/mgopts/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&body).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["options"].as_array().unwrap().len(), 2);
    let line = &runs(&h)[0];
    assert_eq!((line["kind"].as_str(), line["source_kind"].as_str()), (Some("options"), Some("mongodb")));
}

#[tokio::test]
async fn the_inspect_text_is_the_compact_json_of_the_expanded_tree_and_its_hash_is_in_the_log() {
    let (h, fake) = start_mongo("events52i", |c| c.server.query_inspect = true).await;
    fake.push_read(Reply::docs(&[r#"{"host":"a","load":1}"#]));
    let etag = create(&h, "mginspect", "events52i", &[ORDERS]).await;
    let r = run_panel(&h, "mginspect", &etag).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let shown = r.json()["sql"].as_str().expect("the member sql").to_owned();
    assert!(shown.starts_with(r#"{"collection":"orders","aggregate":["#), "{shown}");
    assert!(!shown.contains("$__timeFrom") && shown.contains("2026"), "{shown}");
    assert!(!shown.contains(char::is_whitespace), "{shown}");
    // The hash is the SHA-256 of this text. The text itself is in no log line.
    let want: String = Sha256::digest(shown.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(runs(&h)[0]["sql_sha256"], want);
    assert!(!h.logger.memory_lines().join("\n").contains("\"$match\""));
}

#[tokio::test]
async fn a_query_error_has_the_member_mongo_code_and_a_fault_of_hune_is_an_internal_error() {
    let (h, fake) = start_mongo("events56e", |_| {}).await;
    let etag = create(&h, "mgerr", "events56e", &[ORDERS]).await;
    // The server refuses the pipeline with the code 9 `FailedToParse`.
    fake.push_read(Reply::error(MongoError::server(9, "FailedToParse", "value-marker-4417 is not valid")));
    let r = run_panel(&h, "mgerr", &etag).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"), "{}", r.text());
    let err = r.json()["error"].clone();
    assert_eq!((err["mongo_code"].as_i64(), err["source"].as_str()), (Some(9), Some("events56e")));
    assert!(err.get("clickhouse_code").is_none() && err.get("sqlstate").is_none());
    assert!(err["query_id"].is_string());
    // A fault of Hune has no code of the server: `mongo_code` is null.
    fake.push_read(Reply::docs(&[r#"{"a":{"$binary":{"base64":"AA==","subType":"00"}},"b":1}"#, r#"{"a":"x","b":1}"#]));
    let r = run_panel(&h, "mgerr", &etag).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"), "{}", r.text());
    assert!(r.json()["error"]["mongo_code"].is_null());
    // The refusal of the wire guard is `500 internal_error` with the fixed text.
    fake.push_read(Reply::error(MongoError::new(MongoErrorClass::Guard, "a guard refused db marker-9921")));
    let r = run_panel(&h, "mgerr", &etag).await;
    assert_eq!((r.status, r.code().as_str()), (500, "internal_error"), "{}", r.text());
    assert!(!r.text().contains("marker-9921"));
    let lines = runs(&h);
    assert_eq!(lines.len(), 3);
    assert_eq!((lines[0]["mongo_code"].as_i64(), lines[0]["outcome"].as_str()), (Some(9), Some("sql_error")));
    assert!(lines[1]["mongo_code"].is_null());
    assert_eq!(lines[2]["status"], 500);
    // No value of the data and no query text in any line.
    let all = h.logger.memory_lines().join("\n");
    assert!(!all.contains("value-marker-4417") && !all.contains("marker-9921"));
}

async fn start_authzen(rules: impl Fn(&Q) -> Ans + Send + Sync + 'static, id: &str) -> (Pdp, Harness, FakeMongo) {
    let pdp = Pdp::start(rules).await;
    let url = pdp.url();
    let block = mongo_block(id, "Events");
    let fake = FakeMongo::for_config(&block);
    test_connectors::put(id, fake.connector());
    let h = Harness::start_with(
        probing(),
        |root| {
            fill(root);
            std::fs::write(root.join("transit/mgdash.json"), doc_on("mgdash", id, "table", &[ORDERS])).unwrap();
        },
        |c| {
            authzen(c, &url, Duration::ZERO);
            c.mongo_sources.push(block);
        },
    )
    .await;
    wait_up(&h, id).await;
    (pdp, h, fake)
}

async fn authed(h: &Harness, method: &str, path: &str, body: &[u8]) -> Resp {
    let t = token("user:alice");
    call(h.addr, method, path, &[JSON, (HEADER, t.as_str())], body).await
}

#[tokio::test]
async fn a55_the_ceilings_lower_the_limits_of_a_mongodb_query() {
    let (_pdp, h, fake) = start_authzen(
        |q| {
            if q.action == "dashboard.read" && q.short().ends_with("dashboard:mgdash") {
                Ans::Yes(vec![("max_rows", json!(2)), ("max_duration", json!(1))])
            } else {
                Ans::yes()
            }
        },
        "events55",
    )
    .await;
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/mgdash", b"").await);
    let body = panel_body(&e, json!({"variables": {}}));
    // The result has 5 rows and the ceiling is 2: 403 ceiling_exceeded, and nothing is cut.
    fake.push_read(Reply::docs(&[r#"{"a":1}"#, r#"{"a":2}"#, r#"{"a":3}"#, r#"{"a":4}"#, r#"{"a":5}"#]));
    let r = authed(&h, "POST", "/api/dashboards/mgdash/panels/1/query", &body).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(
        (err["action"].as_str(), err["ceiling"].as_str(), err["limit"].as_u64()),
        (Some("dashboard.read"), Some("max_rows"), Some(2))
    );
    let run = runs(&h).into_iter().next().unwrap();
    assert_eq!(
        (run["source_kind"].as_str(), run["outcome"].as_str(), run["ceiling"].as_bool()),
        (Some("mongodb"), Some("ceiling"), Some(true))
    );
    assert_eq!(run["max_result_rows"], 2);
    // The command carries the ceiling: `maxTimeMS` is 1000 and the pipeline stops at R + 1 rows.
    let sent = fake.requests().pop().unwrap();
    assert_eq!((sent.max_time_ms, sent.batch_size), (1000, 3));
    // The time ceiling of 1 s stops a slow query: 403 ceiling_exceeded with `max_duration`.
    fake.push_read(Reply::docs(&[r#"{"a":1}"#]).after(Duration::from_secs(10)));
    let started = std::time::Instant::now();
    let r = authed(&h, "POST", "/api/dashboards/mgdash/panels/1/query", &body).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    assert_eq!(r.json()["error"]["ceiling"], "max_duration");
    assert!(started.elapsed() < Duration::from_secs(8));
}

#[tokio::test]
async fn a56_the_log_holds_no_query_text_and_the_action_document_holds_the_mongodb_source() {
    let (_pdp, h, fake) = start_authzen(|_| Ans::yes(), "events56").await;
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/mgdash", b"").await);
    let body = panel_body(&e, json!({"variables": {}}));
    fake.push_read(Reply::docs(&[r#"{"host":"value-marker-7731"}"#]));
    let r = authed(&h, "POST", "/api/dashboards/mgdash/panels/1/query", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    fake.push_read(Reply::error(MongoError::server(26, "NamespaceNotFound", "ns not found")));
    let r = authed(&h, "POST", "/api/dashboards/mgdash/panels/1/query", &body).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"), "{}", r.text());
    let lines = runs(&h);
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|v| v["source_kind"] == "mongodb" && v["clickhouse_code"].is_null() && v["sqlstate"].is_null())
    );
    assert!(lines[0]["mongo_code"].is_null() && lines[0]["outcome"] == "ok");
    assert_eq!((lines[1]["mongo_code"].as_i64(), lines[1]["outcome"].as_str()), (Some(26), Some("sql_error")));
    let all = h.logger.memory_lines().join("\n");
    for needle in [
        "value-marker-7731",
        "\"$match\"",
        "$__timeFrom",
        "node_load",
        "orders",
        "analytics-marker",
        "mongo-password-marker",
        "mongo-user-marker",
        "mongo-host-marker",
    ] {
        assert!(!all.contains(needle), "{needle}");
    }
    let doc = get(h.addr, "/.well-known/procurauth/actions").await;
    assert_eq!(doc.status, 200);
    let doc = doc.json();
    let paths: Vec<&str> = doc["resources"].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"source:events56") && paths.contains(&"source:prod"), "{paths:?}");
    assert!(!doc.to_string().contains("mongodb"));
    // The log of the sources: the kind is in the line `source.up`.
    let up = log(&h).into_iter().find(|v| v["event"] == "source.up" && v["source"] == "events56");
    assert_eq!(up.unwrap()["source_kind"], "mongodb");
}

#[tokio::test]
async fn the_probe_lines_of_a_mongodb_source_have_its_kind() {
    let block = mongo_block("events56p", "Events");
    let fake = FakeMongo::for_config(&block);
    // A version below 7.0 gives the line `source.version_unsupported`.
    fake.edit_server_info(|i| i.version = "6.0.5".to_owned());
    test_connectors::put("events56p", fake.connector());
    let h = Harness::start_with(probing(), fill, |c| c.mongo_sources.push(block)).await;
    wait_up(&h, "events56p").await;
    let line = log(&h).into_iter().find(|v| v["event"] == "source.version_unsupported").expect("a version line");
    assert_eq!(
        (line["source_kind"].as_str(), line["source"].as_str(), line["version"].as_str()),
        (Some("mongodb"), Some("events56p"), Some("6.0.5"))
    );
}

/// Creates a dashboard whose only variable is a `custom` variable `env`, closed or open.
async fn create_with_env(h: &Harness, uid: &str, source: &str, sql: &str, open: bool) -> String {
    let mut doc: Value = serde_json::from_str(&doc_on(uid, source, "table", &[sql])).unwrap();
    let mut var = json!({
        "kind": "custom", "name": "env", "label": null, "hide": "none",
        "options": [{"text": "Prod", "value": "prod"}, {"text": "Dev", "value": "$dev"}],
        "multi": false, "include_all": false, "all_value": null, "default": ["prod"]
    });
    if open {
        var["allow_custom_value"] = json!(true);
    }
    doc["variables"] = json!([var]);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{uid}: {}", r.text());
    etag_of(&r)
}

async fn run_env(h: &Harness, uid: &str, etag: &str, value: &str) -> Resp {
    let url = format!("/api/dashboards/{uid}/panels/1/query");
    let variables = json!({"env": {"all": false, "values": [value]}});
    call(h.addr, "POST", &url, &[JSON], &panel_body(etag, json!({"variables": variables}))).await
}

#[tokio::test]
async fn a95_an_open_custom_variable_on_a_mongodb_source_refuses_a_value_that_starts_with_a_dollar() {
    let (h, fake) = start_mongo("events95", |_| {}).await;
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"env":"$env"}}]}"#;
    let open = create_with_env(&h, "mgopen", "events95", text, true).await;
    let closed = create_with_env(&h, "mgclosed", "events95", text, false).await;
    // An open variable: `$secret` and the option `$dev` are free text, and PM2 refuses a value that starts with `$`.
    for bad in ["$secret", "$dev", "$$ROOT"] {
        let r = run_env(&h, "mgopen", &open, bad).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{bad}: {}", r.text());
    }
    assert!(fake.requests().is_empty(), "no command reached MongoDB");
    // The value `secret` runs.
    fake.push_read(Reply::docs(&[r#"{"host":"node-a","load":1.5}"#]));
    let r = run_env(&h, "mgopen", &open, "secret").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(fake.requests().len(), 1);
    // A closed variable: a value outside the options is refused at the check of the values, before any command.
    let r = run_env(&h, "mgclosed", &closed, "$secret").await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert_eq!(fake.requests().len(), 1);
    // The option `$dev` of the closed variable is author text: it runs.
    fake.push_read(Reply::docs(&[r#"{"host":"node-a","load":1.5}"#]));
    let r = run_env(&h, "mgclosed", &closed, "$dev").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(fake.requests().len(), 2);
}
