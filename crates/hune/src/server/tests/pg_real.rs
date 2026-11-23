// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cases A47, A50 and A51 of 16.8 against a real PostgreSQL, through the server.
//!
//! The tests are ignored. `tests/ci/pgsource.sh` starts PostgreSQL 15 and 18 with the fixture of
//! `tests/fixtures/postgres-source` and sets the variables `HUNE_TEST_PGS_*`. Then run
//! `cargo test -p hune --bin hune -- --ignored pg_real --test-threads=1`.

use super::api::{JSON, NODES, etag_of, fill, key, post_body};
use super::authzen_pdp::*;
use super::harness::*;
use super::pg_source::{doc_on, pg_block};
use super::query::panel_body;
use hune_source::PostgresSourceConfig;
use serde_json::{Value, json};
use std::time::Duration;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("the variable {name} is not set: run tests/ci/pgsource.sh"))
}

/// The block `metrics` for the container of the harness: the read-only role of 8a.13.
fn real_block() -> PostgresSourceConfig {
    let mut b = pg_block("metrics", "Metrics");
    b.host = var("HUNE_TEST_PGS_HOST");
    b.port = var("HUNE_TEST_PGS_PORT").parse().expect("HUNE_TEST_PGS_PORT");
    b.database = var("HUNE_TEST_PGS_DATABASE");
    b.user = var("HUNE_TEST_PGS_RO_USER");
    b.password = var("HUNE_TEST_PGS_RO_PASSWORD");
    b.search_path = vec![var("HUNE_TEST_PGS_SCHEMA")];
    b.connect_timeout = Duration::from_secs(5);
    b
}

/// The range of the fixture: 2026-01-01T00:00:00Z plus 30 minutes.
fn body(etag: &str) -> Vec<u8> {
    let mut v: Value = serde_json::from_slice(&panel_body(etag, json!({"variables": {}}))).unwrap();
    v["from_ms"] = json!(1_767_225_600_000_i64);
    v["to_ms"] = json!(1_767_227_400_000_i64);
    serde_json::to_vec(&v).unwrap()
}

async fn create(h: &Harness, uid: &str, doc: &str, hdr: &[(&str, &str)]) -> String {
    let mut headers: Vec<(&str, &str)> = vec![JSON];
    let k = key();
    headers.push(("Idempotency-Key", &k));
    headers.extend_from_slice(hdr);
    let r = call(h.addr, "POST", "/api/dashboards", &headers, &post_body("transit", doc)).await;
    assert_eq!(r.status, 201, "{uid}: {}", r.text());
    etag_of(&r)
}

async fn query(h: &Harness, uid: &str, etag: &str, hdr: &[(&str, &str)]) -> Resp {
    let mut headers: Vec<(&str, &str)> = vec![JSON];
    headers.extend_from_slice(hdr);
    call(h.addr, "POST", &format!("/api/dashboards/{uid}/panels/1/query"), &headers, &body(etag)).await
}

fn log(h: &Harness) -> Vec<Value> {
    h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgsource.sh"]
async fn a47_a_panel_query_on_a_postgres_source_gives_the_frame_of_7_16a() {
    let h = Harness::start_with(test_timing(), fill, |c| c.postgres_sources.push(real_block())).await;
    let sql = ["SELECT created_at, host, load, epoch_s FROM node_load ORDER BY created_at, host LIMIT 3"];
    let etag = create(&h, "pgtable", &doc_on("pgtable", "metrics", "table", &sql), &[]).await;
    let r = query(&h, "pgtable", &etag, &[]).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["rows"], 3);
    assert_eq!(j["columns"][0], json!({"name": "created_at", "type": "timestamptz", "kind": "time"}));
    assert_eq!(j["columns"][1], json!({"name": "host", "type": "text", "kind": "text"}));
    assert_eq!(j["columns"][2], json!({"name": "load", "type": "float8", "kind": "number"}));
    assert_eq!(j["columns"][3], json!({"name": "epoch_s", "type": "int8", "kind": "number_text"}));
    assert_eq!(j["values"][0][0].as_f64(), Some(1_767_225_600.0));
    assert_eq!(j["values"][1], json!(["node-a", "node-b", "node-c"]));
    assert_eq!(j["values"][3], json!(["1767225600", "1767225600", "1767225600"]));
    let line = log(&h).into_iter().find(|v| v["event"] == "query.run").unwrap();
    assert_eq!((line["source_kind"].as_str(), line["outcome"].as_str()), (Some("postgres"), Some("ok")));

    // The format `time_series` applies the rule of 7.16b: the column `time` of a macro gets the kind `time`.
    let group = ["SELECT $__timeGroupAlias(created_at, $__interval), avg(load) AS load FROM node_load \
         WHERE $__timeFilter(created_at) GROUP BY 1 ORDER BY 1"];
    let etag = create(&h, "pgseries", &doc_on("pgseries", "metrics", "time_series", &group), &[]).await;
    let r = query(&h, "pgseries", &etag, &[]).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["columns"][0]["name"], "time");
    assert_eq!(r.json()["columns"][0]["kind"], "time", "{}", r.text());
    // The same text with the format `table` keeps the kind of the type.
    let etag = create(&h, "pgplain", &doc_on("pgplain", "metrics", "table", &group), &[]).await;
    let r = query(&h, "pgplain", &etag, &[]).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_ne!(r.json()["columns"][0]["kind"], "time");

    // A dashboard with a panel on each kind of source is saved and read back.
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!("both");
    doc["title"] = json!("Both kinds");
    let pg: Value = serde_json::from_str(&doc_on("x", "metrics", "table", &sql)).unwrap();
    let mut panel = pg["sections"][0]["panels"][0].clone();
    panel["id"] = json!(99);
    panel["grid"]["y"] = json!(100);
    doc["sections"][0]["panels"].as_array_mut().unwrap().push(panel);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let got = get(h.addr, "/api/dashboards/both").await.json();
    let panels = got["document"]["sections"][0]["panels"].as_array().unwrap();
    assert!(panels.iter().any(|p| p["data"]["query"]["source"] == "metrics"));
    assert!(panels.iter().any(|p| p["data"]["query"]["source"] == "prod"));

    // The probe changes the state (8a.12). `GET /api/sources` shows the kind and the state.
    let listed = get(h.addr, "/api/sources").await.json();
    assert_eq!(listed["items"][0]["kind"], "postgres");
}

async fn start_authzen_rules(rules: impl Fn(&Q) -> Ans + Send + Sync + 'static) -> (Pdp, Harness) {
    {
        let pdp = Pdp::start(rules).await;
        let url = pdp.url();
        let h = Harness::start_with(
            probing(),
            |root| {
                fill(root);
                let sql = ["SELECT host FROM node_load ORDER BY created_at LIMIT 10"];
                std::fs::write(root.join("transit/pgdash.json"), doc_on("pgdash", "metrics", "table", &sql)).unwrap();
                let slow = ["SELECT count(*) AS n FROM big_rows a CROSS JOIN big_rows b"];
                std::fs::write(root.join("transit/pgslow.json"), doc_on("pgslow", "metrics", "table", &slow)).unwrap();
            },
            |c| {
                authzen(c, &url, Duration::ZERO);
                let mut block = real_block();
                block.max_concurrent_per_subject = Some(4);
                c.postgres_sources.push(block);
            },
        )
        .await;
        (pdp, h)
    }
}

fn probing() -> crate::server::limits::Timing {
    crate::server::limits::Timing { probe_sources: true, ..test_timing() }
}

async fn authed(h: &Harness, method: &str, path: &str, body: &[u8]) -> Resp {
    let t = token("user:alice");
    call(h.addr, method, path, &[JSON, (HEADER, t.as_str())], body).await
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgsource.sh"]
async fn a50_the_ceilings_lower_the_limits_of_a_postgres_query() {
    let (_pdp, h) = start_authzen_rules(|q| {
        if q.action == "dashboard.read" && q.short().ends_with("dashboard:pgdash") {
            Ans::Yes(vec![("max_rows", json!(2))])
        } else if q.action == "dashboard.read" && q.short().ends_with("dashboard:pgslow") {
            Ans::Yes(vec![("max_duration", json!(1))])
        } else {
            Ans::yes()
        }
    })
    .await;
    // The result has 10 rows and the ceiling is 2: 403 ceiling_exceeded, and nothing is cut.
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/pgdash", b"").await);
    let r = authed(&h, "POST", "/api/dashboards/pgdash/panels/1/query", &body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(
        (err["action"].as_str(), err["ceiling"].as_str(), err["limit"].as_u64()),
        (Some("dashboard.read"), Some("max_rows"), Some(2))
    );
    let run = log(&h).into_iter().find(|v| v["event"] == "query.run").unwrap();
    assert_eq!(
        (run["source_kind"].as_str(), run["outcome"].as_str(), run["ceiling"].as_bool()),
        (Some("postgres"), Some("ceiling"), Some(true))
    );
    assert_eq!(run["max_result_rows"], 2);
    // The time ceiling of 1 s stops a long query: 403 ceiling_exceeded with `max_duration`.
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/pgslow", b"").await);
    let started = std::time::Instant::now();
    let r = authed(&h, "POST", "/api/dashboards/pgslow/panels/1/query", &body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    assert_eq!(r.json()["error"]["ceiling"], "max_duration");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgsource.sh"]
async fn a51_the_log_holds_no_sql_and_no_value_and_the_action_document_holds_the_source() {
    let (_pdp, h) = start_authzen_rules(|_| Ans::yes()).await;
    let marker_sql = ["SELECT 'value-marker-7731' AS m, host FROM node_load LIMIT 1"];
    let bad_sql = ["SELECT sql_marker_column FROM metrics.no_such_table_7731"];
    for (uid, sql) in [("pgok", &marker_sql[..]), ("pgbad", &bad_sql[..])] {
        let doc = doc_on(uid, "metrics", "table", sql);
        let t = token("user:alice");
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards",
            &[JSON, (HEADER, t.as_str()), ("Idempotency-Key", &key())],
            &post_body("transit", &doc),
        )
        .await;
        assert_eq!(r.status, 201, "{}", r.text());
    }
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/pgok", b"").await);
    let r = authed(&h, "POST", "/api/dashboards/pgok/panels/1/query", &body(&e)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let e = etag_of(&authed(&h, "GET", "/api/dashboards/pgbad", b"").await);
    let r = authed(&h, "POST", "/api/dashboards/pgbad/panels/1/query", &body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(err["sqlstate"], "42P01");
    assert!(err.get("clickhouse_code").is_none());
    assert_eq!(err["source"], "metrics");
    // `query.run`: the kind, the SQLSTATE for the failure, `clickhouse_code` null.
    let runs: Vec<Value> = log(&h).into_iter().filter(|v| v["event"] == "query.run").collect();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|v| v["source_kind"] == "postgres" && v["clickhouse_code"].is_null()));
    assert!(runs[0]["sqlstate"].is_null() && runs[0]["outcome"] == "ok");
    assert_eq!((runs[1]["sqlstate"].as_str(), runs[1]["outcome"].as_str()), (Some("42P01"), Some("sql_error")));
    // No SQL text, no value, no password in any line.
    let all = h.logger.memory_lines().join("\n");
    for needle in
        ["value-marker-7731", "sql_marker_column", "no_such_table_7731", "node_load", &var("HUNE_TEST_PGS_RO_PASSWORD")]
    {
        assert!(!all.contains(needle), "{needle}");
    }
    // The action document holds the source of the new kind, with no kind in its text.
    let doc = get(h.addr, "/.well-known/procurauth/actions").await;
    assert_eq!(doc.status, 200);
    let doc = doc.json();
    let paths: Vec<&str> = doc["resources"].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"source:metrics") && paths.contains(&"source:prod"), "{paths:?}");
    assert!(!doc.to_string().contains("postgres"));
}

/// The annotations of the case PS17 on the source `metrics`, in this order (index 0 to 6).
fn ps17_annotations() -> Value {
    let one = |name: &str, sql: &[&str]| json!({"name": name, "source": "metrics", "color": "red", "sql": sql});
    json!([
        one(
            "Load",
            &[
                "SELECT $__time(created_at), created_at + interval '30 seconds' AS \"timeEnd\", host AS title,",
                "  'load ' || load::text AS text, ARRAY[env, host] AS tags",
                "FROM node_load WHERE $__timeFilter(created_at) AND host = 'node-a' AND env = '${env}'",
                "ORDER BY created_at DESC LIMIT 3 OFFSET 28"
            ]
        ),
        one("Milliseconds", &["SELECT epoch_s * 1000 AS time FROM node_load WHERE host = 'node-a' ORDER BY 1 LIMIT 3"]),
        one("Thousand", &["SELECT created_at AS time FROM node_load ORDER BY created_at LIMIT 1000"]),
        one("Too many", &["SELECT created_at AS time FROM node_load ORDER BY created_at LIMIT 1001"]),
        one("Empty", &["SELECT created_at AS time, host AS title FROM node_load WHERE false"]),
        one("Html", &["SELECT created_at AS time, '<b>SECRET-CELL</b>' AS text FROM node_load LIMIT 1"]),
        one("Seconds", &["SELECT $__timeEpoch(created_at) FROM node_load ORDER BY created_at LIMIT 1"]),
    ])
}

/// PS17 (16.17): annotations on a `postgres` source through the server (7.37). The
/// route, the frame rules, the cap of 1,000 rows and the log line are those of a ClickHouse source.
#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgsource.sh"]
async fn ps17_annotations_on_a_postgres_source_give_the_marks_and_keep_the_caps() {
    let h = Harness::start_with(test_timing(), fill, |c| c.postgres_sources.push(real_block())).await;
    let mut doc: Value = serde_json::from_str(&doc_on("pgannot", "metrics", "table", &["SELECT 1 AS n"])).unwrap();
    let env: Value = serde_json::from_str::<Value>(NODES).unwrap()["variables"][2].clone();
    assert_eq!(env["name"], "env");
    doc["variables"] = json!([env]);
    doc["annotations"] = ps17_annotations();
    let etag = create(&h, "pgannot", &serde_json::to_string(&doc).unwrap(), &[]).await;
    let post = |index: usize| {
        let body = json!({
            "etag": etag, "from_ms": 1_767_225_600_000_i64, "to_ms": 1_767_227_400_000_i64,
            "variables": {"env": {"all": false, "values": ["prod"]}}
        });
        let url = format!("/api/dashboards/pgannot/annotations/{index}/query");
        let h = &h;
        async move { call(h.addr, "POST", &url, &[JSON], &serde_json::to_vec(&body).unwrap()).await }
    };
    // The range is 30 minutes: the rows of node-a at 00:00 to 00:30 are 31, so OFFSET 28 of the descending
    // order gives 00:00, 00:01 and 00:02. The marks come in the order of the time.
    let r = post(0).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(
        r.json()["annotations"],
        json!([
            [1_767_225_600_000_i64, 1_767_225_630_000_i64, "node-a", "load 0", ["prod", "node-a"]],
            [1_767_225_660_000_i64, 1_767_225_690_000_i64, "node-a", "load 1", ["prod", "node-a"]],
            [1_767_225_720_000_i64, 1_767_225_750_000_i64, "node-a", "load 2", ["prod", "node-a"]]
        ])
    );
    // An `int8` of epoch milliseconds gives the same times.
    let r = post(1).await;
    let times: Vec<i64> = r.json()["annotations"].as_array().unwrap().iter().map(|m| m[0].as_i64().unwrap()).collect();
    assert_eq!(times, [1_767_225_600_000, 1_767_225_660_000, 1_767_225_720_000], "{}", r.text());
    // The cap of 7.37: 1,000 rows pass, the row after them gives `422 query_limit`.
    let r = post(2).await;
    assert_eq!((r.status, r.json()["rows"].as_u64()), (200, Some(1000)), "{}", r.text());
    let r = post(3).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_limit"), "{}", r.text());
    assert_eq!(
        (r.json()["error"]["limit"].as_str(), r.json()["error"]["value"].as_u64()),
        (Some("max_result_rows"), Some(1000))
    );
    // An empty result gives no mark. A hostile text gives `annotation_invalid` with no cell value.
    let r = post(4).await;
    assert_eq!((r.status, r.json()["annotations"].clone()), (200, json!([])), "{}", r.text());
    let r = post(5).await;
    assert_eq!((r.status, r.code().as_str()), (422, "annotation_invalid"), "{}", r.text());
    assert_eq!(r.json()["error"]["reason"], "bad_chars");
    assert!(!r.text().contains("SECRET-CELL"));
    // `$__timeEpoch` gives Unix seconds: they are read as milliseconds, so the mark is in January 1970 (7.37).
    let r = post(6).await;
    assert_eq!(r.json()["annotations"][0][0], 1_767_225_600_i64, "{}", r.text());
    // The log lines: the kind `annotation`, the index, the kind of the source and the cap. No cell value.
    let runs: Vec<Value> = log(&h).into_iter().filter(|v| v["event"] == "query.run").collect();
    assert_eq!(runs.len(), 7);
    for (i, run) in runs.iter().enumerate() {
        assert_eq!((run["kind"].as_str(), run["annotation"].as_u64()), (Some("annotation"), Some(i as u64)));
        assert_eq!((run["source_kind"].as_str(), run["max_result_rows"].as_u64()), (Some("postgres"), Some(1000)));
    }
    assert!(!h.logger.memory_lines().join("\n").contains("SECRET-CELL"));
}
