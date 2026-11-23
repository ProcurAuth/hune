//! The MongoDB source in the API with the real driver and a real server: the cases MS13, MS16 and MS17 of
//! 16.9b.
//!
//! Each test is `#[ignore]`: `tests/ci/mongosource.sh` starts disposable servers and sets the variables
//! `HUNE_TEST_MGS_*`, and then it runs these tests with `cargo test -p hune --lib -- --ignored ::ms`. The
//! server under test is a `Harness` of this crate with the connector of the driver (`MongoDriver`) in place of
//! the fake client. Tests are not product code: they can read the environment and run `docker`.

use super::api::{JSON, etag_of, fill, key, post_body};
use super::harness::*;
use super::pg_source::doc_on;
use super::query::{draft_panel, panel_body, post_preview, preview_body};
use crate::server::limits::Timing;
use crate::server::sources::test_connectors;
use hune_core::ids::SourceId;
use hune_core::mongo::ReadPreference;
use hune_source::{MongoConnector, MongoDriver, MongoSourceConfig, MongoTlsMode, MongoTopology, SourceTls};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// The first instant of the data of `node_load` (2026-01-01T00:00:00Z).
const BASE_MS: i64 = 1_767_225_600_000;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: run tests/ci/mongosource.sh"))
}

/// The block of the user `hune_ro` on the server at `port`, with the test CA.
fn real_block(id: &str, port: u16) -> MongoSourceConfig {
    let ca = env("HUNE_TEST_MGS_CA_FILE");
    MongoSourceConfig {
        id: SourceId::parse(id).unwrap(),
        label: "Events".to_owned(),
        topology: MongoTopology::Direct { host: env("HUNE_TEST_MGS_HOST"), port },
        user: env("HUNE_TEST_MGS_RO_USER"),
        password: env("HUNE_TEST_MGS_RO_PASSWORD"),
        auth_database: "admin".to_owned(),
        databases: vec![env("HUNE_TEST_MGS_DATABASE")],
        // The golden pipelines of 16.9b join the collections `hosts` and `events`: the list names them, and the
        // probe gives the advice A1 for a name that no database holds.
        collections: ["node_load", "node_events", "types", "orders", "hosts", "events"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        read_preference: ReadPreference::Primary,
        tls_mode: MongoTlsMode::VerifyFull,
        tls: Some(SourceTls { ca_pem: std::fs::read(&ca).unwrap(), server_name: String::new() }),
        tls_ca_file: Some(PathBuf::from(&ca)),
        connect_timeout: Duration::from_secs(3),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 50_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(5),
        max_concurrent_per_subject: None,
    }
}

fn probing() -> Timing {
    Timing { probe_sources: true, ..test_timing() }
}

/// Starts a server with the real driver for the block `id`.
async fn start_real(id: &str, port: u16, edit: impl FnOnce(&mut crate::config::Config)) -> Harness {
    test_connectors::put(id, MongoConnector::of_type::<MongoDriver>());
    let block = real_block(id, port);
    let h = Harness::start_with(probing(), fill, |c| {
        c.mongo_sources.push(block);
        edit(c);
    })
    .await;
    wait_state(&h, id, "up", Duration::from_secs(30)).await;
    h
}

async fn state_of(h: &Harness, id: &str) -> (u16, String) {
    let r = get(h.ops, "/readyz").await;
    (r.status, r.json()["sources"][id].as_str().unwrap_or("").to_owned())
}

async fn wait_state(h: &Harness, id: &str, state: &str, within: Duration) {
    let start = Instant::now();
    loop {
        if state_of(h, id).await.1 == state {
            return;
        }
        assert!(start.elapsed() < within, "the source {id} did not get the state {state}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn runs(h: &Harness) -> Vec<Value> {
    h.logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["event"] == "query.run")
        .collect()
}

async fn create(h: &Harness, uid: &str, source: &str, sql: &[&str]) -> String {
    let doc = doc_on(uid, source, "table", sql);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{uid}: {}", r.text());
    etag_of(&r)
}

async fn run_panel(h: &Harness, uid: &str, etag: &str, range_ms: (i64, i64)) -> Resp {
    let url = format!("/api/dashboards/{uid}/panels/1/query");
    let body = panel_body(etag, json!({"variables": {}, "from_ms": range_ms.0, "to_ms": range_ms.1}));
    call(h.addr, "POST", &url, &[JSON], &body).await
}

/// The container of a compose service of the run.
fn container_of(service: &str) -> String {
    let out = Command::new(env("HUNE_TEST_MGS_DOCKER"))
        .args(["ps", "-aq", "--filter"])
        .arg(format!("label=com.docker.compose.project={}", env("HUNE_TEST_MGS_COMPOSE_PROJECT")))
        .arg("--filter")
        .arg(format!("label=com.docker.compose.service={service}"))
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).lines().next().expect("the container of the service").trim().to_owned()
}

fn docker(args: &[&str]) {
    let status = Command::new(env("HUNE_TEST_MGS_DOCKER")).args(args).status().unwrap();
    assert!(status.success(), "docker {args:?}");
}

fn rs_port() -> u16 {
    env("HUNE_TEST_MGS_RS_PORT").parse().unwrap()
}

const QUICK: &str = r#"{"collection":"node_load","aggregate":[{"$match":{"_id":{"$lt":3}}},{"$sort":{"_id":1}}]}"#;

/// MS13: a stopped MongoDB. The panel query fails with a retryable `source_unavailable`, `/readyz` stays 200, and the
/// state is `down`. After the restart, the state is `up`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms13_a_stopped_mongodb_gives_down_and_the_restart_gives_up() {
    let port: u16 = env("HUNE_TEST_MGS_JS_PORT").parse().unwrap();
    let container = container_of(&format!("{}-js", env("HUNE_TEST_MGS_SERVICE")));
    let h = start_real("ms13", port, |_| {}).await;
    let etag = create(&h, "ms13dash", "ms13", &[QUICK]).await;
    let range = (BASE_MS, BASE_MS + 1_000_000);
    let r = run_panel(&h, "ms13dash", &etag, range).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 3);

    // The server stops: the pooled connections die and no server answers.
    docker(&["stop", "-t", "3", &container]);
    let r = run_panel(&h, "ms13dash", &etag, range).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert_eq!(r.json()["error"]["retryable"], true);
    // The page of the server stays ready, and the state of the source is `down`.
    let (ready, state) = state_of(&h, "ms13").await;
    assert_eq!((ready, state.as_str()), (200, "down"));
    assert!(get(h.addr, "/api/sources").await.json()["items"].to_string().contains("\"down\""));

    // The server starts again. The first start of the fixture runs again: wait for the data.
    docker(&["start", &container]);
    let start = Instant::now();
    loop {
        let r = run_panel(&h, "ms13dash", &etag, range).await;
        if r.status == 200 {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(240), "the server did not come back: {}", r.text());
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    wait_state(&h, "ms13", "up", Duration::from_secs(60)).await;
    assert_eq!(state_of(&h, "ms13").await.0, 200);
}

/// MS16: an options query (7.14) and a preview run on a MongoDB source. With `server.query_inspect = true` the inspect
/// text is the compact Extended JSON of the expanded tree and holds no member that Hune adds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms16_an_options_query_and_a_preview_run_and_the_inspect_text_is_the_tree() {
    let h = start_real("ms16", rs_port(), |c| c.server.query_inspect = true).await;
    let range = (BASE_MS, BASE_MS + 1_800_000);

    // The options of a saved variable: the first column is the value. The light data has 20 nodes.
    let options_sql = concat!(
        r#"{"collection":"node_load","aggregate":["#,
        r#"{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"}}},"#,
        r#"{"$group":{"_id":"$node"}},{"$project":{"_id":0,"node":"$_id"}},{"$sort":{"node":1}}]}"#
    );
    let mut doc: Value = serde_json::from_str(&doc_on("ms16dash", "ms16", "table", &[QUICK])).unwrap();
    doc["variables"] = json!([{
        "kind": "query", "name": "which", "label": null, "hide": "none", "source": "ms16",
        "sql": [options_sql], "multi": false, "include_all": false, "all_value": null, "refresh": "load",
        "sort": "alpha_asc", "regex": null, "default": ["node-00"]
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
    let body = json!({
        "etag": etag, "from_ms": range.0, "to_ms": range.1,
        "variables": {"which": {"all": false, "values": ["node-00"]}}
    });
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/ms16dash/variables/which/options",
        &[JSON],
        &serde_json::to_vec(&body).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let options = r.json()["options"].clone();
    let values: Vec<&str> = options.as_array().unwrap().iter().map(|o| o["value"].as_str().unwrap()).collect();
    let expected: Vec<String> = (0..20).map(|n| format!("node-{n:02}")).collect();
    assert_eq!(values, expected.iter().map(String::as_str).collect::<Vec<_>>());
    let line = runs(&h).into_iter().find(|l| l["kind"] == "options").expect("the log line of the options query");
    assert_eq!(line["source_kind"], "mongodb");

    // The preview of a draft panel.
    let mut panel = draft_panel(QUICK);
    panel["data"]["query"]["source"] = json!("ms16");
    panel["data"]["query"]["format"] = json!("table");
    let body = preview_body(json!({"panel": panel, "from_ms": range.0, "to_ms": range.1}));
    let r = post_preview(&h, "ms16", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 3);
    let shown = r.json()["sql"].as_str().expect("the member sql").to_owned();

    // The inspect text is the compact Extended JSON of the expanded tree.
    assert!(shown.starts_with(r#"{"collection":"node_load","aggregate":["#), "{shown}");
    assert!(!shown.contains(char::is_whitespace), "{shown}");
    let tree: Value = serde_json::from_str(&shown).expect("the inspect text is JSON");
    let stages = tree["aggregate"].as_array().unwrap();
    // The tree has the two stages of the author and nothing that Hune adds: no final `$limit`.
    assert_eq!(stages.len(), 2, "{shown}");
    for added in ["cursor", "allowDiskUse", "maxTimeMS", "comment", "readConcern", "$db", "$readPreference"] {
        assert!(!shown.contains(&format!("\"{added}\"")), "{added} in {shown}");
    }
    // A macro is a Date in the text (Extended JSON), with the millisecond of the range.
    let macro_sql = r#"{"collection":"node_load","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom"}}},{"$limit":1}]}"#;
    let mut panel = draft_panel(macro_sql);
    panel["data"]["query"]["source"] = json!("ms16");
    panel["data"]["query"]["format"] = json!("table");
    let body = preview_body(json!({"panel": panel, "from_ms": range.0, "to_ms": range.1}));
    let r = post_preview(&h, "ms16", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let shown = r.json()["sql"].as_str().unwrap().to_owned();
    assert!(shown.contains("$date") && shown.contains("2026-01-01T00:00:00"), "{shown}");
    // The hash of the log line is the SHA-256 of this text, and no log line holds the text.
    let want: String = Sha256::digest(shown.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let line = runs(&h).into_iter().rev().find(|l| l["kind"] == "preview").expect("the log line of the preview");
    assert_eq!(line["sql_sha256"], want);
    assert!(!h.logger.memory_lines().join("\n").contains("\"$match\""));
}

/// The accepted texts of the golden files of 7.31 that only name collections of the test block.
fn golden_texts() -> Vec<(String, Vec<String>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/pipeline-mongo");
    let mut out = Vec::new();
    for file in [
        "stages.json",
        "query-operators.json",
        "expression-operators.json",
        "accumulators.json",
        "window-operators.json",
    ] {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(root.join(file)).unwrap()).unwrap();
        for case in doc["cases"].as_array().unwrap() {
            if case["expect"] != "accepted" {
                continue;
            }
            let lines: Vec<String> =
                case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_owned()).collect();
            // One text names the database `metrics`, which the test block does not list.
            if lines.iter().any(|l| l.contains("\"database\"")) {
                continue;
            }
            out.push((format!("{file}:{}", case["id"].as_str().unwrap()), lines));
        }
    }
    out
}

/// A frame without the members that change from run to run.
fn stable(mut frame: Value) -> Value {
    if let Some(m) = frame.as_object_mut() {
        m.remove("query_id");
        m.remove("etag");
        m.remove("stats");
    }
    frame
}

/// MS17: the golden pipelines give the same result at save and at run, on the same tree. The saved SHA-256 of the
/// expanded command is the value in the log line `query.run`. The preview runs the draft before the save, the panel
/// query runs the saved document: the two give the same frame and the same inspect text.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms17_the_golden_pipelines_give_the_same_result_at_save_and_at_run() {
    let h = start_real("ms17", rs_port(), |c| c.server.query_inspect = true).await;
    let range = (BASE_MS, BASE_MS + 600_000);
    let texts = golden_texts();
    assert!(texts.len() >= 200, "{} golden texts", texts.len());
    let mut checked = 0;
    for (n, (name, lines)) in texts.iter().enumerate() {
        let sql: Vec<&str> = lines.iter().map(String::as_str).collect();
        // The draft, before the save.
        let mut panel = draft_panel(&lines.join("\n"));
        panel["data"]["query"]["sql"] = json!(lines);
        panel["data"]["query"]["source"] = json!("ms17");
        panel["data"]["query"]["format"] = json!("table");
        let body = preview_body(json!({"panel": panel, "from_ms": range.0, "to_ms": range.1}));
        let preview = post_preview(&h, "ms17", &body).await;
        // The save: the checks of 7.18 pass for each golden text.
        let uid = format!("g{n}");
        let etag = create(&h, &uid, "ms17", &sql).await;
        let run = run_panel(&h, &uid, &etag, range).await;
        // The server may refuse a text because of the data (a type, an argument): the answer is the same one
        // at the preview and at the run.
        assert_eq!(preview.status, run.status, "{name}: {} / {}", preview.text(), run.text());
        assert!(matches!(run.status, 200 | 422), "{name}: {}", run.text());
        if run.status == 200 {
            assert_eq!(stable(preview.json()), stable(run.json()), "{name}");
            // The inspect text is the same tree.
            assert_eq!(preview.json()["sql"], run.json()["sql"], "{name}");
            let shown = run.json()["sql"].as_str().expect("the inspect text").to_owned();
            let want: String = Sha256::digest(shown.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
            let line = runs(&h).into_iter().rev().find(|l| l["kind"] == "panel").expect("the log line of the run");
            assert_eq!(line["sql_sha256"], want, "{name}");
            checked += 1;
        } else {
            assert_eq!(preview.code(), run.code(), "{name}");
            let error = run.json()["error"].clone();
            assert!(error.get("mongo_code").is_some(), "{name}: {}", run.text());
        }
    }
    assert!(checked >= 100, "only {checked} golden texts ran to a frame");
}

/// The annotations of the case MS20 on the source `ms20`, in this order (index 0 to 5).
fn ms20_annotations() -> Value {
    let one = |name: &str, stages: &str| {
        json!({"name": name, "source": "ms20", "color": "green",
            "sql": [format!(r#"{{"collection":"node_events","aggregate":{stages}}}"#)]})
    };
    json!([
        one(
            "Errors",
            r#"[{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"},"level":"error"}},{"$sort":{"ts":-1}},{"$skip":1},{"$project":{"_id":0,"time":"$ts","timeEnd":{"$add":["$ts",30000]},"title":"$node","text":"$message","tags":1}}]"#
        ),
        one(
            "Milliseconds",
            r#"[{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"},"level":"error"}},{"$sort":{"ts":1}},{"$limit":3},{"$project":{"_id":0,"time":{"$toLong":"$ts"}}}]"#
        ),
        one("Thousand", r#"[{"$sort":{"_id":1}},{"$limit":1000},{"$project":{"_id":0,"time":"$ts"}}]"#),
        one("Too many", r#"[{"$sort":{"_id":1}},{"$limit":1001},{"$project":{"_id":0,"time":"$ts"}}]"#),
        one("Empty", r#"[{"$match":{"level":"no-such-level"}},{"$project":{"_id":0,"time":"$ts"}}]"#),
        one("Html", r#"[{"$limit":1},{"$project":{"_id":0,"time":"$ts","text":{"$literal":"<b>SECRET-CELL</b>"}}}]"#),
    ])
}

/// MS20 (16.9b): annotations on a `mongodb` source through the server with the real driver (7.37; decisions
/// ID-K142 and ID-K333). The route, the frame rules, the cap of 1,000 rows and the log line are those of the
/// other kinds. An empty result has no column and gives no mark (rule 13).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms20_annotations_on_a_mongodb_source_give_the_marks_and_keep_the_caps() {
    let h = start_real("ms20", rs_port(), |_| {}).await;
    let mut doc: Value = serde_json::from_str(&doc_on("mgannot", "ms20", "table", &[QUICK])).unwrap();
    doc["annotations"] = ms20_annotations();
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
    let post = |index: usize| {
        let body = json!({"etag": etag, "from_ms": BASE_MS, "to_ms": BASE_MS + 600_000, "variables": {}});
        let url = format!("/api/dashboards/mgannot/annotations/{index}/query");
        let h = &h;
        async move { call(h.addr, "POST", &url, &[JSON], &serde_json::to_vec(&body).unwrap()).await }
    };
    // node_events has one document each 30 s and the level `error` for i % 5 = 4: 4 errors in the 10 minutes
    // (i = 4, 9, 14, 19). The descending sort and `$skip` 1 give i = 14, 9 and 4, and the marks come in the order of
    // the time.
    let r = post(0).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let ms = |i: i64| BASE_MS + i * 30_000;
    assert_eq!(
        r.json()["annotations"],
        json!([
            [ms(4), ms(4) + 30_000, "node-04", "synthetic event 4", ["t1", "t4"]],
            [ms(9), ms(9) + 30_000, "node-09", "synthetic event 9", ["t0", "t4"]],
            [ms(14), ms(14) + 30_000, "node-14", "synthetic event 14", ["t2", "t4"]]
        ])
    );
    // An Int64 of epoch milliseconds gives the same times.
    let r = post(1).await;
    let times: Vec<i64> = r.json()["annotations"].as_array().unwrap().iter().map(|m| m[0].as_i64().unwrap()).collect();
    assert_eq!(times, [ms(4), ms(9), ms(14)], "{}", r.text());
    // The cap of 7.37: 1,000 rows pass, the row after them gives `422 query_limit`.
    let r = post(2).await;
    assert_eq!((r.status, r.json()["rows"].as_u64()), (200, Some(1000)), "{}", r.text());
    let r = post(3).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_limit"), "{}", r.text());
    assert_eq!(r.json()["error"]["limit"], "max_result_rows");
    // An empty result has no column (7.16c) and gives no mark (rule 13). A hostile text gives no cell value.
    let r = post(4).await;
    assert_eq!((r.status, r.json()["annotations"].clone()), (200, json!([])), "{}", r.text());
    let r = post(5).await;
    assert_eq!((r.status, r.code().as_str()), (422, "annotation_invalid"), "{}", r.text());
    assert_eq!(r.json()["error"]["reason"], "bad_chars");
    assert!(!r.text().contains("SECRET-CELL"));
    let lines = runs(&h);
    assert_eq!(lines.len(), 6);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!((line["kind"].as_str(), line["annotation"].as_u64()), (Some("annotation"), Some(i as u64)));
        assert_eq!((line["source_kind"].as_str(), line["max_result_rows"].as_u64()), (Some("mongodb"), Some(1000)));
    }
    assert!(!h.logger.memory_lines().join("\n").contains("SECRET-CELL"));
}
