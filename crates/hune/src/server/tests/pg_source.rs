//! The PostgreSQL source in the API: A48 and A49 of 16.8, the dialect that the kind of the source
//! chooses, the error answer, and the log line of a query that reaches no server.
//!
//! These tests need no PostgreSQL server: the source `metrics` points to a port that nobody
//! listens on. A panel query of a valid text then reaches the source and gives `503
//! source_unavailable`, which shows the full path of 7.2 up to step 15. The tests with a real
//! server are in `crates/hune/tests/pgsource_api.rs`.

use super::api::{JSON, NODES, etag_of, fill, key, post_body, put};
use super::harness::*;
use super::query::{draft_panel, panel_body, post_preview, preview_body};
use hune_core::ids::SourceId;
use hune_source::{PostgresSourceConfig, PostgresSslMode};
use serde_json::{Value, json};
use std::time::Duration;

/// The settings of a `postgres` block that nobody serves.
pub(super) fn pg_block(id: &str, label: &str) -> PostgresSourceConfig {
    PostgresSourceConfig {
        id: SourceId::parse(id).unwrap(),
        label: label.to_owned(),
        host: "127.0.0.1".to_owned(),
        port: 1,
        database: "pg-database-marker".to_owned(),
        user: "pg-user-marker".to_owned(),
        password: "pg-password-marker".to_owned(),
        search_path: vec!["metrics".to_owned()],
        sslmode: PostgresSslMode::Disable,
        tls: None,
        connect_timeout: Duration::from_secs(1),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 50_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(1),
        max_concurrent_per_subject: None,
    }
}

async fn start_pg() -> Harness {
    Harness::start_with(test_timing(), fill, |c| c.postgres_sources.push(pg_block("metrics", "Metrics"))).await
}

/// A document with one panel on the source and the SQL lines. It has no variable.
pub(super) fn doc_on(uid: &str, source: &str, format: &str, sql: &[&str]) -> String {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("Postgres panel");
    doc["variables"] = json!([]);
    let mut panel = doc["sections"][0]["panels"][0].clone();
    panel["data"]["query"]["source"] = json!(source);
    panel["data"]["query"]["format"] = json!(format);
    panel["data"]["query"]["sql"] = json!(sql);
    panel["data"]["query"]["min_interval"] = Value::Null;
    doc["sections"] = json!([{"title": null, "collapsed": false, "panels": [panel]}]);
    serde_json::to_string_pretty(&doc).unwrap()
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
async fn a48_sources_list_has_both_kinds_and_no_host_user_or_secret() {
    let h = start_pg().await;
    let r = get(h.addr, "/api/sources").await;
    assert_eq!(r.status, 200);
    assert_eq!(
        r.json(),
        json!({"items": [
            {"id": "metrics", "label": "Metrics", "kind": "postgres", "state": "unknown"},
            {"id": "prod", "label": "Production", "kind": "clickhouse", "state": "unknown"},
        ]})
    );
    let text = r.text();
    for secret in ["127.0.0.1", "pg-database-marker", "pg-user-marker", "pg-password-marker", "secret-password"] {
        assert!(!text.contains(secret), "{secret}");
    }
    // The state of both sources is in /readyz.
    let ready = get(h.ops, "/readyz").await.json();
    assert!(ready["sources"].get("metrics").is_some() && ready["sources"].get("prod").is_some(), "{ready}");
}

#[tokio::test]
async fn a49_save_refuses_the_sql_that_breaks_7_25_on_a_postgres_source() {
    let h = start_pg().await;
    let e0 = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    // Each text breaks 7.25. The pointer is the SQL line of the panel.
    let refused: [(&str, &[&str]); 7] = [
        ("a DELETE", &["DELETE FROM t"]),
        ("two statements", &["SELECT 1;", "SELECT 2"]),
        ("a refused function", &["SELECT pg_sleep(1)"]),
        ("a parameter", &["SELECT $1"]),
        ("a locking clause", &["SELECT a FROM t FOR UPDATE"]),
        ("a data-modifying CTE", &["WITH x AS (DELETE FROM t RETURNING 1) SELECT * FROM x"]),
        ("a function that writes a file", &["SELECT pg_read_file('/etc/passwd')"]),
    ];
    for (name, sql) in refused {
        let doc = doc_on("nodes", "metrics", "table", sql);
        let r = put(&h, "nodes", &e0, &doc, &key()).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        let list = pointers(&r);
        assert!(list.iter().any(|(p, _)| p.starts_with("/sections/0/panels/0/data/query/sql/")), "{name}: {list:?}");
    }
    // The same save is refused at the creation and by validate_only.
    let doc = doc_on("created", "metrics", "table", &["SELECT pg_sleep(1)"]);
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &post_body("transit", &doc)).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    // A text that follows 7.25 is saved.
    let ok = doc_on(
        "nodes",
        "metrics",
        "time_series",
        &[
            "SELECT $__timeGroupAlias(ts, $__interval), avg(v) AS v FROM t WHERE $__timeFilter(ts) GROUP BY 1 ORDER BY 1",
        ],
    );
    let r = put(&h, "nodes", &e0, &ok, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The macros of the dialect `postgres` are refused for a ClickHouse source, and the reverse.
    let e1 = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    let on_ch = doc_on("nodes", "prod", "table", &["SELECT $__timeGroupAlias(ts, 1) FROM t"]);
    let r = put(&h, "nodes", &e1, &on_ch, &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    // A text that 7.12 accepts is saved on a ClickHouse source: the dialect follows the source.
    let on_ch = doc_on("nodes", "prod", "table", &["SELECT pg_sleep(1)"]);
    let r = put(&h, "nodes", &e1, &on_ch, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn a49_preview_refuses_the_sql_that_breaks_7_25_and_uses_the_dialect_of_the_source() {
    let h = start_pg().await;
    let panel = |sql: &str| {
        let mut p = draft_panel(sql);
        p["data"]["query"]["source"] = json!("metrics");
        p["data"]["query"]["format"] = json!("table");
        p
    };
    for sql in ["DELETE FROM t", "SELECT 1; SELECT 2", "SELECT pg_sleep(1)", "SELECT $1", "SELECT a FROM t FOR UPDATE"]
    {
        let r = post_preview(&h, "metrics", &preview_body(json!({"panel": panel(sql)}))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{sql}: {}", r.text());
        assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/panel/data/query/sql/0", "{sql}");
    }
    // A macro of the dialect `clickhouse` is not a macro of the dialect `postgres`.
    let r = post_preview(&h, "metrics", &preview_body(json!({"panel": panel("SELECT $__timeInterval(ts)")}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    // A draft whose source is the other kind is refused (the source of the route).
    let mut other = panel("SELECT 1");
    other["data"]["query"]["source"] = json!("prod");
    let r = post_preview(&h, "metrics", &preview_body(json!({"panel": other}))).await;
    assert_eq!(r.json()["error"]["errors"][0]["code"], "bad_reference");
    // A valid text reaches the source, which has no server: 503 source_unavailable, with no SQL.
    let good = panel("SELECT 1 AS n");
    let r = post_preview(&h, "metrics", &preview_body(json!({"panel": good}))).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert_eq!(r.json()["error"]["source"], "metrics");
}

#[tokio::test]
async fn a_panel_query_on_a_postgres_source_runs_the_pipeline_and_logs_the_kind() {
    let h = start_pg().await;
    let sql = [
        "SELECT $__timeGroupAlias(ts, $__interval), avg(v) AS secret_column_marker FROM t WHERE $__timeFilter(ts) GROUP BY 1",
    ];
    let doc = doc_on("pgdash", "metrics", "time_series", &sql);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let etag = etag_of(&r);
    let url = "/api/dashboards/pgdash/panels/1/query";
    let r = call(h.addr, "POST", url, &[JSON], &panel_body(&etag, json!({"variables": {}}))).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert!(r.json()["error"].get("clickhouse_code").is_none() && r.json()["error"].get("sqlstate").is_none());
    // The line `query.run` has the kind, `clickhouse_code` null and `sqlstate` null. No SQL, no value.
    let lines: Vec<Value> = h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    let run: Vec<&Value> = lines.iter().filter(|v| v["event"] == "query.run").collect();
    assert_eq!(run.len(), 1, "{lines:?}");
    let run = run[0];
    assert_eq!((run["source"].as_str(), run["source_kind"].as_str()), (Some("metrics"), Some("postgres")));
    assert_eq!((run["outcome"].as_str(), run["status"].as_u64()), (Some("unavailable"), Some(503)));
    assert!(run["clickhouse_code"].is_null() && run["sqlstate"].is_null());
    assert_eq!(run["sql_sha256"].as_str().map(str::len), Some(64));
    let all = h.logger.memory_lines().join("\n");
    for needle in ["secret_column_marker", "pg-password-marker", "pg-user-marker", "pg-database-marker", "FROM t"] {
        assert!(!all.contains(needle), "{needle}");
    }
}

#[tokio::test]
async fn the_size_limit_and_the_expansion_stop_a_postgres_query_before_the_source() {
    let h = Harness::start_with(test_timing(), fill, |c| {
        let mut block = pg_block("metrics", "Metrics");
        block.max_query_size = 200;
        c.postgres_sources.push(block);
    })
    .await;
    let long = format!("SELECT '{}' AS n", "x".repeat(300));
    let doc = doc_on("pgsize", "metrics", "table", &[long.as_str()]);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let etag = etag_of(&r);
    let url = "/api/dashboards/pgsize/panels/1/query";
    let r = call(h.addr, "POST", url, &[JSON], &panel_body(&etag, json!({"variables": {}}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_too_large"), "{}", r.text());
    assert_eq!(r.json()["error"]["limit"], 200);
    // No `query.run` line: the query never reached the source.
    assert!(!h.events().contains(&"query.run".to_owned()));
}

#[tokio::test]
async fn the_query_run_line_of_a_clickhouse_source_has_its_kind_and_a_null_sqlstate() {
    use super::fake_ch::{FRAME_BODY, Fake};
    use super::query::queries;
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |c| c.postgres_sources.push(pg_block("metrics", "Metrics"))).await;
    let e = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "query.run")
        .unwrap();
    assert_eq!(line["source_kind"], "clickhouse");
    assert!(line["sqlstate"].is_null() && line["clickhouse_code"].is_null());
}

/// Creates a dashboard on the source `metrics` with one `custom` variable `env`, closed or open.
async fn create_with_env(h: &Harness, uid: &str, sql: &str, open: bool) -> Resp {
    let mut doc: Value = serde_json::from_str(&doc_on(uid, "metrics", "table", &[sql])).unwrap();
    let mut var = json!({
        "kind": "custom", "name": "env", "label": null, "hide": "none",
        "options": [{"text": "Prod", "value": "prod"}], "multi": false, "include_all": false,
        "all_value": null, "default": ["prod"]
    });
    if open {
        var["allow_custom_value"] = json!(true);
    }
    doc["variables"] = json!([var]);
    call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(&doc).unwrap()),
    )
    .await
}

#[tokio::test]
async fn ps16_a_custom_variable_on_a_postgres_source_follows_its_member() {
    let h = start_pg().await;
    let run = |uid: &'static str, etag: String, value: &'static str| {
        let addr = h.addr;
        async move {
            let variables = json!({"env": {"all": false, "values": [value]}});
            let body = panel_body(&etag, json!({"variables": variables}));
            call(addr, "POST", &format!("/api/dashboards/{uid}/panels/1/query"), &[JSON], &body).await
        }
    };
    // A string: a closed variable refuses a value outside the options, an open one passes the expansion and
    // reaches the source (the source is down, so 503 shows the path of 7.2 up to step 15).
    let sql = "SELECT 1 AS a WHERE x = '$env'";
    let closed = etag_of(&create_with_env(&h, "pgclosed", sql, false).await);
    let open = etag_of(&create_with_env(&h, "pgopen", sql, true).await);
    let r = run("pgclosed", closed.clone(), "x' OR 1=1 --").await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    let r = run("pgopen", open.clone(), "x' OR 1=1 --").await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    // A value in code is a number only.
    let code = "SELECT 1 AS a WHERE x = $env";
    let open_code = etag_of(&create_with_env(&h, "pgcode", code, true).await);
    let r = run("pgcode", open_code.clone(), "1 OR 1=1").await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    let r = run("pgcode", open_code, "42").await;
    assert_eq!(r.status, 503, "{}", r.text());
    // A quoted name: valid for the closed variable, `not_permitted` at save for the open one.
    let name = "SELECT 1 AS a FROM \"$env\"";
    assert_eq!(create_with_env(&h, "pgnameclosed", name, false).await.status, 201);
    let r = create_with_env(&h, "pgnameopen", name, true).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r)[0].1, "not_permitted");
}
