// SPDX-License-Identifier: MIT OR Apache-2.0
//! The query routes through a fake ClickHouse: A18 to A24 of 16.8 and the error mapping of 6.8.

use super::api::{JSON, NODES, doc_text, etag_of, fill};
use super::fake_ch::{FRAME_BODY, Fake, Step, error, healthy, ok};
use super::harness::*;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

pub(super) fn queries(body: &'static str) -> super::fake_ch::Handler {
    healthy(move |rec| ok(rec, body))
}

pub(super) async fn etag(h: &Harness, uid: &str) -> String {
    etag_of(&get(h.addr, &format!("/api/dashboards/{uid}")).await)
}

pub(super) fn panel_body(etag: &str, extra: Value) -> Vec<u8> {
    let mut v = json!({
        "etag": etag,
        "from_ms": 1_794_211_200_000_i64,
        "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200,
        "variables": {
            "node": {"all": false, "values": ["a1", "b2"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
        }
    });
    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            if x.is_null() {
                m.remove(k);
            } else {
                m.insert(k.clone(), x.clone());
            }
        }
    }
    serde_json::to_vec(&v).unwrap()
}

pub(super) async fn post_panel(h: &Harness, panel: u32, body: &[u8]) -> Resp {
    call(h.addr, "POST", &format!("/api/dashboards/nodes/panels/{panel}/query"), &[JSON], body).await
}

#[tokio::test]
async fn a18_panel_query_gives_a_frame() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/panels/1/query",
        &[JSON, ("X-Request-Id", "req-q1")],
        &panel_body(&e, json!({})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.header("cache-control"), Some("no-store"));
    let j = r.json();
    assert_eq!(j["etag"], e);
    assert_eq!(j["rows"], 2);
    assert_eq!(j["interval_ms"], 60_000, "the panel has min_interval 1m");
    assert_eq!(j["columns"][0], json!({"name": "t", "type": "DateTime", "kind": "time"}));
    assert_eq!(j["columns"][1]["kind"], "text");
    assert_eq!(j["columns"][2], json!({"name": "weight", "type": "Decimal(38, 9)", "kind": "number_text"}));
    assert_eq!(j["values"][0], json!([1794211200, 1794211260]));
    assert_eq!(j["values"][1], json!(["a1", null]));
    assert_eq!(j["values"][2], json!(["12.5", "3"]));
    assert_eq!(j["stats"]["read_rows"], 10);
    // The query that ClickHouse got: expanded SQL, the settings of 8.3 and the attribution of 8.10.
    let q = fake.queries();
    assert_eq!(q.len(), 1);
    let sql = q[0].sql();
    assert!(!sql.contains("$__") && !sql.contains("${"), "{sql}");
    assert!(sql.contains("node IN ('a1','b2')"), "{sql}");
    assert!(sql.contains("toStartOfInterval"), "{sql}");
    assert_eq!(q[0].query_id(), j["query_id"].as_str().unwrap());
    assert_eq!(q[0].param("database").as_deref(), Some("analytics"));
    assert_eq!(q[0].param("max_result_rows").as_deref(), Some("100000"));
    assert_eq!(q[0].header("x-clickhouse-user"), Some("hune"));
    assert_eq!(q[0].header("x-clickhouse-key"), Some("secret-password"));
    let comment: Value = serde_json::from_str(&q[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(
        comment,
        json!({"app": "hune", "kind": "panel", "uid": "nodes", "panel": 1, "request_id": "req-q1", "subject": "-"})
    );
    // The log line has no SQL and gives its hash.
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "query.run")
        .expect("query.run");
    assert_eq!(line["kind"], "panel");
    assert_eq!(line["uid"], "nodes");
    assert_eq!(line["panel"], 1);
    assert_eq!(line["source"], "prod");
    assert_eq!(line["outcome"], "ok");
    assert_eq!(line["status"], 200);
    assert_eq!(line["rows"], 2);
    assert_eq!(line["request_id"], "req-q1");
    assert_eq!(line["query_id"], j["query_id"]);
    assert_eq!(line["sql_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(line["ceiling"], false);
    assert!(line["clickhouse_code"].is_null());
    // The query got its place at once: `queue_ms` is a number, and the histogram has one value (14.8).
    assert!(line["queue_ms"].as_u64().is_some_and(|q| q < 1000), "{line}");
    for l in h.logger.memory_lines() {
        assert!(
            !l.contains("secret-password") && !l.contains("toStartOfInterval") && !l.contains("analytics.node_weight"),
            "{l}"
        );
    }
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_query_duration_seconds_count{source=\"prod\",status=\"ok\"} 1"), "{m}");
    assert!(m.contains("hune_source_queue_wait_seconds_count{source=\"prod\"} 1"), "{m}");
    assert!(m.contains("hune_query_rows_total{source=\"prod\"} 2"));
    assert!(m.contains("hune_query_response_bytes_total{source=\"prod\"} 50"));
    assert!(m.contains("hune_source_up{source=\"prod\"} 1"));
    assert!(m.contains("hune_source_in_flight{source=\"prod\"} 0"));
    // The second panel (a table) uses the text variables.
    let r = post_panel(&h, 2, &panel_body(&e, json!({"max_data_points": 100}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sql = fake.queries()[1].sql();
    assert!(sql.contains("env = 'prod'"), "{sql}");
}

#[tokio::test]
async fn a19_stale_etag_gives_409_and_a_newer_store_is_read_once() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |c| c.storage.volumes[0].rescan_interval = Duration::from_secs(3600)).await;
    let e0 = etag(&h, "nodes").await;
    // An etag that nobody has: 409 with the current etag, and nothing runs.
    let bogus = "0".repeat(64);
    let r = post_panel(&h, 1, &panel_body(&bogus, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (409, "dashboard_changed"));
    assert_eq!(r.json()["error"]["current_etag"], e0);
    assert_eq!(r.json()["error"]["retryable"], false);
    assert!(fake.queries().is_empty());
    // A change outside Hune: the catalog is old. The request with the new etag runs after one read of the store.
    let root = std::fs::canonicalize(h.dir.path()).unwrap();
    let new_doc = doc_text("nodes", "Changed outside");
    std::fs::write(root.join("transit/nodes.json"), &new_doc).unwrap();
    let new_etag: String = {
        use sha2::{Digest, Sha256};
        Sha256::digest(new_doc.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    };
    let r = post_panel(&h, 1, &panel_body(&new_etag, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The catalog entry is updated now: the old etag gives 409 with the new one.
    let r = post_panel(&h, 1, &panel_body(&e0, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (409, "dashboard_changed"));
    assert_eq!(r.json()["error"]["current_etag"], new_etag);
    // The catalog entry is updated: the page gets the new document without a poll.
    assert_eq!(get(h.addr, "/api/dashboards/nodes").await.json()["document"]["title"], "Changed outside");
    assert_eq!(fake.queries().len(), 1);
}

#[tokio::test]
async fn a20_bad_requests_are_refused_before_clickhouse() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let cases: Vec<(u32, Value, u16, &str)> = vec![
        (1, json!({"from_ms": 5, "to_ms": 5}), 422, "invalid_time_range"),
        (1, json!({"from_ms": 10, "to_ms": 5}), 422, "invalid_time_range"),
        (1, json!({"from_ms": -1}), 422, "invalid_time_range"),
        (1, json!({"to_ms": 4_294_967_296_000_i64}), 422, "invalid_time_range"),
        (1, json!({"max_data_points": 0}), 400, "bad_request"),
        (1, json!({"max_data_points": 10_001}), 400, "bad_request"),
        (1, json!({"max_data_points": -4}), 400, "bad_request"),
        (99, json!({}), 404, "not_found"),
        (3, json!({}), 400, "bad_request"),
        (1, json!({"variables": {"nothing": {"all": false, "values": ["x"]}}}), 400, "bad_request"),
        (1, json!({"variables": []}), 400, "bad_request"),
        (1, json!({"etag": "ABC"}), 400, "bad_request"),
        (1, json!({"extra": 1}), 400, "bad_request"),
        (1, json!({"variables": null}), 400, "bad_request"),
        (1, json!({"etag": null}), 400, "bad_request"),
    ];
    for (panel, change, status, code) in cases {
        let r = post_panel(&h, panel, &panel_body(&e, change.clone())).await;
        assert_eq!((r.status, r.code().as_str()), (status, code), "{change} {}", r.text());
    }
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/x/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 404);
    let r = call(h.addr, "POST", "/api/dashboards/nothing/panels/1/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 404);
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn a21_refused_variable_values_send_no_query() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let cases = [
        // A text variable takes exactly one value.
        json!({"variables": {"node": {"all": false, "values": ["a"]}, "account": {"all": false, "values": ["a", "b"]}, "env": {"all": false, "values": ["prod"]}}}),
        // A value that is not an option of a custom variable.
        json!({"variables": {"node": {"all": false, "values": ["a"]}, "account": {"all": false, "values": [""]}, "env": {"all": false, "values": ["evil"]}}}),
        // A control character.
        json!({"variables": {"node": {"all": false, "values": ["a\u{1}"]}, "account": {"all": false, "values": [""]}, "env": {"all": false, "values": ["prod"]}}}),
        // A value that is not a string.
        json!({"variables": {"node": {"all": false, "values": [1]}, "account": {"all": false, "values": [""]}, "env": {"all": false, "values": ["prod"]}}}),
    ];
    for change in cases {
        let r = post_panel(&h, 1, &panel_body(&e, change.clone())).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{change} {}", r.text());
        let err = r.json()["error"].clone();
        assert!(err["variable"].is_string() && err["reason"].is_string());
        assert_eq!(err["retryable"], false);
    }
    // A variable that the SQL names and the request does not give.
    let r = post_panel(&h, 2, &panel_body(&e, json!({"variables": {"env": {"all": false, "values": ["prod"]}}}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn sql_that_the_engine_refuses_gives_422() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let bad_doc = |sql: &str| NODES.replacen("\"SELECT node, name, weight, ratio, status\"", &format!("\"{sql}\""), 1);
    let h = start_with_fake(
        &fake,
        |root| {
            fill(root);
            std::fs::write(root.join("transit/nodes.json"), bad_doc("DROP TABLE x")).unwrap();
            std::fs::write(
                root.join("transit/second.json"),
                bad_doc("SELECT 1 -- ok").replace("\"uid\": \"nodes\"", "\"uid\": \"second\""),
            )
            .unwrap();
        },
        |c| c.clickhouse[0].max_query_size = 400,
    )
    .await;
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 2, &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    // The size limit of the final SQL.
    let r = post_panel(
        &h,
        1,
        &panel_body(
            &e,
            json!({"variables": {
        "node": {"all": false, "values": (0..100).map(|i| format!("node-number-{i}")).collect::<Vec<_>>()},
        "account": {"all": false, "values": [""]}, "env": {"all": false, "values": ["prod"]}}}),
        ),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_too_large"), "{}", r.text());
    assert!(r.json()["error"]["size"].as_u64().unwrap() > 400);
    assert_eq!(r.json()["error"]["limit"], 400);
    assert!(fake.queries().is_empty());
    // An expansion error never reaches the source: no query.run line, no queue wait (14.7).
    assert_eq!(h.logger.memory_lines().iter().filter(|l| l.contains("\"query.run\"")).count(), 0);
    let m = get(h.ops, "/metrics").await.text();
    assert!(!m.contains("hune_source_queue_wait_seconds_count{"), "{m}");
    assert!(!m.contains("hune_query_duration_seconds_count{"), "{m}");
}

#[tokio::test]
async fn a22_options_of_a_variable() {
    let body =
        "[\"node\",\"name\"]\n[\"String\",\"String\"]\n[\"a1\",\"Node A\"]\n[null,\"skipped\"]\n[\"b2\",\"b2\"]\n";
    let fake = Fake::start(queries(body)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let req = json!({
        "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "variables": {"env": {"all": false, "values": ["prod"]}}
    });
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&req).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["etag"], e);
    assert_eq!(j["options"], json!([{"value": "a1", "text": "Node A"}, {"value": "b2", "text": "b2"}]));
    assert_eq!(j["query_id"].as_str().unwrap(), fake.queries()[0].query_id());
    assert_eq!(fake.queries()[0].param("max_result_rows").as_deref(), Some("10000"), "the cap of 7.14");
    let comment: Value = serde_json::from_str(&fake.queries()[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(comment["kind"], "options");
    assert_eq!(comment["variable"], "node");
    assert!(comment.get("panel").is_none());
    // A variable of another kind is 400; an unknown variable is 404; a stale etag is 409.
    let post = |name: &str, body: Value| {
        let name = name.to_owned();
        let addr = h.addr;
        async move {
            call(
                addr,
                "POST",
                &format!("/api/dashboards/nodes/variables/{name}/options"),
                &[JSON],
                &serde_json::to_vec(&body).unwrap(),
            )
            .await
        }
    };
    assert_eq!(post("env", req.clone()).await.status, 400);
    assert_eq!(post("account", req.clone()).await.status, 400);
    assert_eq!(post("nothing", req.clone()).await.status, 404);
    let mut stale = req.clone();
    stale["etag"] = json!("1".repeat(64));
    assert_eq!(post("node", stale).await.code(), "dashboard_changed");
    let mut bad = req.clone();
    bad["max_data_points"] = json!(5);
    assert_eq!(post("node", bad).await.status, 400, "the options body has no max_data_points");
    let mut range = req.clone();
    range["to_ms"] = json!(1);
    assert_eq!(post("node", range).await.code(), "invalid_time_range");
    assert_eq!(fake.queries().len(), 1);
}

#[tokio::test]
async fn options_convert_the_kinds_of_7_14() {
    let body = "[\"t\",\"n\",\"b\"]\n[\"DateTime\",\"UInt32\",\"Bool\"]\n[\"1794211200\",5,true]\n[\"1794211200.5\",6,false]\n";
    let fake = Fake::start(queries(body)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let req =
        json!({"etag": e, "from_ms": 1000, "to_ms": 2000, "variables": {"env": {"all": false, "values": ["prod"]}}});
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&req).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The first column is the value (a time in epoch milliseconds), the second is the text (a number).
    assert_eq!(
        r.json()["options"],
        json!([{"value": "1794211200000", "text": "5"}, {"value": "1794211200500", "text": "6"}])
    );
}

#[tokio::test]
async fn clickhouse_errors_map_to_the_codes_of_6_8() {
    let fake = Fake::start(healthy(|rec| {
        let sql = rec.sql();
        let code = |c: i64, status: u16| error(rec, status, c, "Code: x. DB::Exception: something\nstack trace line");
        if sql.contains("max_concurrent") {
            return vec![Step::Hang];
        }
        match rec.param("max_execution_time").as_deref() {
            Some("61") => code(159, 500),
            Some("62") => code(241, 500),
            Some("63") => code(497, 500),
            Some("64") => code(396, 500),
            Some("65") => code(62, 500),
            Some("66") => code(202, 500),
            Some("67") => ok(rec, "not a frame"),
            _ => ok(rec, FRAME_BODY),
        }
    }))
    .await;
    let expect: [(u64, u16, &str); 7] = [
        (61, 504, "query_timeout"),
        (62, 422, "query_memory_limit"),
        (63, 502, "source_access_denied"),
        (64, 422, "query_limit"),
        (65, 422, "query_error"),
        (66, 503, "source_unavailable"),
        (67, 502, "source_bad_response"),
    ];
    // One server for each time limit, so that the fake can tell the cases apart.
    for (seconds, status, code) in expect {
        let h =
            start_with_fake(&fake, fill, |c| c.clickhouse[0].max_execution_time = Duration::from_secs(seconds)).await;
        let e = etag(&h, "nodes").await;
        let r = post_panel(&h, 1, &panel_body(&e, json!({}))).await;
        assert_eq!((r.status, r.code().as_str()), (status, code), "{}", r.text());
        let err = r.json()["error"].clone();
        match code {
            "query_timeout" | "source_access_denied" | "source_bad_response" => {
                assert_eq!(err["source"], "prod");
                assert!(err["query_id"].is_string());
            }
            "query_error" => {
                assert_eq!(err["clickhouse_code"], 62);
                assert_eq!(err["source"], "prod");
                assert!(err["query_id"].is_string());
                // The message holds the first line of the ClickHouse message only.
                assert!(!err["message"].as_str().unwrap().contains("stack trace"));
            }
            "query_limit" => {
                assert_eq!(err["limit"], "max_result");
                assert!(err["value"].is_null());
            }
            "source_unavailable" => {
                assert_eq!(err["source"], "prod");
                assert_eq!(err["retryable"], true);
            }
            _ => {}
        }
        let line: Value = h
            .logger
            .memory_lines()
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .find(|v| v["event"] == "query.run")
            .expect("a query.run line");
        assert_eq!(line["code"], code);
        assert_eq!(line["status"], status);
        assert!(line["outcome"].is_string());
    }
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..300 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timeout: {what}");
}

#[tokio::test]
async fn c05_a_client_that_leaves_cancels_the_query() {
    let fake = Fake::start(healthy(|_| vec![Step::Hang])).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let mut c = Client::connect(h.addr).await;
    let body = panel_body(&e, json!({}));
    c.send(&request("POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &body)).await;
    wait_for("the query reaches ClickHouse", || fake.queries().len() == 1).await;
    assert_eq!(h.state().metrics.http_in_flight(), 1);
    let left = std::time::Instant::now();
    drop(c);
    // The connection to ClickHouse closes within 2 s (8.9).
    wait_for("the connection to ClickHouse closes", || fake.closed() == 1).await;
    assert!(left.elapsed() < Duration::from_secs(2), "{:?}", left.elapsed());
    wait_for("the request ends", || h.state().metrics.http_in_flight() == 0).await;
    // The log line of the query has the outcome cancelled and the status 499.
    wait_for("the log line", || h.logger.memory_lines().iter().any(|l| l.contains("\"query.run\""))).await;
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "query.run")
        .unwrap();
    assert_eq!((line["outcome"].as_str(), line["status"].as_u64()), (Some("cancelled"), Some(499)));
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_query_duration_seconds_count{source=\"prod\",status=\"cancelled\"} 1"), "{m}");
    assert!(m.contains("hune_source_in_flight{source=\"prod\"} 0"));
}

#[tokio::test]
async fn c05_the_same_on_a_kept_connection_and_the_source_stays_usable() {
    let fake = Fake::start(healthy(|rec| {
        if rec.sql().contains("FROM analytics.node_weight") && rec.param("max_result_rows").as_deref() == Some("100000")
        {
            vec![Step::Hang]
        } else {
            ok(rec, FRAME_BODY)
        }
    }))
    .await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let e = etag(&h, "nodes").await;
    let mut c = Client::connect(h.addr).await;
    c.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(c.read(false).await.status, 200);
    c.send(&request("POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &panel_body(&e, json!({})))).await;
    wait_for("the query reaches ClickHouse", || fake.queries().len() == 1).await;
    // Half of a second later the client leaves.
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(c);
    wait_for("the connection to ClickHouse closes", || fake.closed() == 1).await;
    wait_for("the request ends", || h.state().metrics.http_in_flight() == 0).await;
    // The second panel runs and gets its answer.
    let r = post_panel(&h, 2, &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn a35_a_stop_during_a_slow_query_ends_it_with_server_stopping() {
    let fake = Fake::start(healthy(|_| vec![Step::Hang])).await;
    let mut h = start_with_fake(&fake, fill, |c| c.server.shutdown_grace = Duration::from_secs(3)).await;
    let e = etag(&h, "nodes").await;
    let addr = h.addr;
    let body = panel_body(&e, json!({}));
    let slow =
        tokio::spawn(async move { call(addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &body).await });
    wait_for("the query reaches ClickHouse", || fake.queries().len() == 1).await;
    // The grace is 3 s: step 4 of 14.13 lasts 0 s, and step 5 cancels the query.
    assert_eq!(h.stop().await, 0);
    let r = slow.await.unwrap();
    assert_eq!((r.status, r.code().as_str()), (503, "server_stopping"), "{}", r.text());
    assert_eq!(r.header("connection"), Some("close"));
    wait_for("the connection to ClickHouse closes", || fake.closed() == 1).await;
}

#[tokio::test]
async fn source_busy_has_retry_after() {
    let fake = Fake::start(healthy(|_| vec![Step::Hang])).await;
    let h = start_with_fake(&fake, fill, |c| {
        c.clickhouse[0].max_concurrent = 1;
        c.clickhouse[0].queue_wait = Duration::from_millis(100);
    })
    .await;
    let e = etag(&h, "nodes").await;
    let addr = h.addr;
    let body = panel_body(&e, json!({}));
    let b2 = body.clone();
    let first =
        tokio::spawn(async move { call(addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &b2).await });
    wait_for("the first query reaches ClickHouse", || fake.queries().len() == 1).await;
    let r = post_panel(&h, 2, &body).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_busy"), "{}", r.text());
    assert_eq!(r.header("retry-after"), Some("1"));
    assert_eq!(r.json()["error"]["retryable"], true);
    assert_eq!(r.json()["error"]["source"], "prod");
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_source_refused_total{source=\"prod\",reason=\"queue_wait\"} 1"), "{m}");
    assert!(m.contains("hune_source_in_flight{source=\"prod\"} 1"), "{m}");
    // A refusal of the admission writes no query.run line (14.7).
    assert_eq!(h.logger.memory_lines().iter().filter(|l| l.contains("\"query.run\"")).count(), 0);
    // The refused query waited `queue_wait` (100 ms): the histogram has its wait (14.8).
    assert!(m.contains("hune_source_queue_wait_seconds_count{source=\"prod\"} 1"), "{m}");
    let sum: f64 = m
        .lines()
        .find_map(|l| l.strip_prefix("hune_source_queue_wait_seconds_sum{source=\"prod\"} "))
        .and_then(|v| v.parse().ok())
        .expect("the sum");
    assert!((0.08..2.0).contains(&sum), "{sum}");
    first.abort();
}

#[tokio::test]
async fn a_query_that_waits_for_a_place_writes_queue_ms_and_counts_in_the_histogram() {
    // The first query holds the only place until the client leaves. The second waits.
    let fake = Fake::start(healthy(|rec| {
        if rec.param("max_result_rows").as_deref() == Some("100000") && rec.sql().contains("node_weight") {
            vec![Step::Hang]
        } else {
            ok(rec, FRAME_BODY)
        }
    }))
    .await;
    let h = start_with_fake(&fake, fill, |c| {
        c.clickhouse[0].max_concurrent = 1;
        c.clickhouse[0].queue_wait = Duration::from_secs(10);
    })
    .await;
    let e = etag(&h, "nodes").await;
    let body = panel_body(&e, json!({}));
    let mut c = Client::connect(h.addr).await;
    c.send(&request("POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &body)).await;
    wait_for("the first query reaches ClickHouse", || fake.queries().len() == 1).await;
    let addr = h.addr;
    let b2 = panel_body(&e, json!({"max_data_points": 100}));
    let second =
        tokio::spawn(async move { call(addr, "POST", "/api/dashboards/nodes/panels/2/query", &[JSON], &b2).await });
    wait_for("the second query waits", || h.state().metrics.http_in_flight() == 2).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    drop(c);
    let r = second.await.unwrap();
    assert_eq!(r.status, 200, "{}", r.text());
    let lines: Vec<Value> = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["event"] == "query.run")
        .collect();
    let ok_line = lines.iter().find(|v| v["outcome"] == "ok").expect("the line of the second query");
    let q = ok_line["queue_ms"].as_u64().expect("queue_ms is a number");
    assert!((200..5000).contains(&q), "{ok_line}");
    // The first query left after its place: its line has the outcome cancelled and a `queue_ms`.
    wait_for("the cancelled line", || h.logger.memory_lines().iter().any(|l| l.contains("\"cancelled\""))).await;
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_source_queue_wait_seconds_count{source=\"prod\"} 2"), "{m}");
    let sum: f64 = m
        .lines()
        .find_map(|l| l.strip_prefix("hune_source_queue_wait_seconds_sum{source=\"prod\"} "))
        .and_then(|v| v.parse().ok())
        .expect("the sum");
    assert!((0.2..6.0).contains(&sum), "{sum}");
}

#[tokio::test]
async fn source_states_follow_the_probe() {
    // A healthy account: up, with the log line source.up.
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    wait_source_state(&h, "up").await;
    assert_eq!(get(h.addr, "/api/sources").await.json()["items"][0]["state"], "up");
    assert!(h.events().contains(&"source.up".to_owned()));
    assert!(get(h.ops, "/metrics").await.text().contains("hune_source_up{source=\"prod\"} 1"));
    // The readiness never depends on the source.
    assert_eq!(get(h.ops, "/readyz").await.status, 200);

    // readonly = 1: misconfigured; a query fails at once and sends nothing.
    let fake = Fake::start(Arc::new(|rec| {
        super::fake_ch::probe_answer(rec, "26.8.1.1", 1).unwrap_or_else(|| ok(rec, FRAME_BODY))
    }))
    .await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    wait_source_state(&h, "misconfigured").await;
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "source.misconfigured")
        .expect("source.misconfigured");
    assert_eq!(line["level"], "error");
    assert_eq!(line["source"], "prod");
    assert!(line["reason"].as_str().unwrap().contains("readonly"));
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (502, "source_misconfigured"));
    assert!(fake.queries().is_empty());

    // An old version: the warning source.version_unsupported, and the queries still run.
    let fake = Fake::start(Arc::new(|rec| {
        super::fake_ch::probe_answer(rec, "25.1.1.1", 2).unwrap_or_else(|| ok(rec, FRAME_BODY))
    }))
    .await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    wait_source_state(&h, "up").await;
    wait_for("the warning", || h.events().contains(&"source.version_unsupported".to_owned())).await;
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "source.version_unsupported")
        .unwrap();
    assert_eq!((line["level"].as_str(), line["version"].as_str()), (Some("warn"), Some("25.1.1.1")));

    // Nothing listens: down, with the log line source.down, and the readiness stays 200.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = closed.local_addr().unwrap();
    drop(closed);
    let dead_fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&dead_fake, fill, |c| {
        c.clickhouse[0].url.port = dead.port();
        c.clickhouse[0].url.host = dead.ip().to_string();
    })
    .await;
    wait_source_state(&h, "down").await;
    assert!(h.events().contains(&"source.down".to_owned()));
    assert_eq!(get(h.ops, "/readyz").await.status, 200);
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"));
    assert!(get(h.ops, "/metrics").await.text().contains("hune_source_up{source=\"prod\"} 0"));
}

#[tokio::test]
async fn a_source_that_comes_back_logs_source_up_with_the_down_time() {
    let fake = Fake::start(Arc::new({
        let n = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        move |rec| {
            if rec.sql().starts_with("SELECT version()") && n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                return error(rec, 500, 202, "too many connections");
            }
            super::fake_ch::probe_answer(rec, "26.8.1.1", 2).unwrap_or_else(|| ok(rec, FRAME_BODY))
        }
    }))
    .await;
    let addr = fake.addr;
    let timing = crate::server::limits::Timing {
        probe_sources: true,
        probe_interval: Duration::from_millis(150),
        ..test_timing()
    };
    let h = Harness::start_with(timing, fill, |c| {
        c.clickhouse[0].url.host = addr.ip().to_string();
        c.clickhouse[0].url.port = addr.port();
    })
    .await;
    wait_for("source.down", || h.events().contains(&"source.down".to_owned())).await;
    wait_for("source.up", || h.events().contains(&"source.up".to_owned())).await;
    let up: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "source.up")
        .unwrap();
    assert!(up["down_seconds"].is_u64());
    wait_source_state(&h, "up").await;
}

pub(super) fn draft_panel(sql: &str) -> Value {
    let doc: Value = serde_json::from_str(NODES).unwrap();
    let mut panel = doc["sections"][0]["panels"][0].clone();
    panel["id"] = json!(7);
    panel["data"]["query"]["sql"] = json!([sql]);
    panel["data"]["query"]["min_interval"] = Value::Null;
    panel
}

pub(super) fn preview_body(extra: Value) -> Vec<u8> {
    let mut v = json!({
        "uid": "nodes",
        "from_ms": 1_794_211_200_000_i64,
        "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200,
        "dashboard_variables": [],
        "variables": {},
        "panel": draft_panel("SELECT toDateTime(1) AS t, 1 AS n WHERE $__timeFilter(t)"),
    });
    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            if x.is_null() {
                m.remove(k);
            } else {
                m.insert(k.clone(), x.clone());
            }
        }
    }
    serde_json::to_vec(&v).unwrap()
}

pub(super) async fn post_preview(h: &Harness, source: &str, body: &[u8]) -> Resp {
    call(h.addr, "POST", &format!("/api/sources/{source}/preview"), &[JSON], body).await
}

#[tokio::test]
async fn a24_preview_runs_a_draft_panel_and_a_draft_variable() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = post_preview(&h, "prod", &preview_body(json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert!(j.get("etag").is_none(), "a preview has no etag");
    assert_eq!(j["rows"], 2);
    assert_eq!(j["interval_ms"], 20_000);
    let q = fake.queries();
    assert!(q[0].sql().starts_with("SELECT toDateTime(1) AS t"), "{}", q[0].sql());
    assert!(!q[0].sql().contains("$__"));
    let comment: Value = serde_json::from_str(&q[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(comment["kind"], "preview");
    assert_eq!(comment["uid"], "nodes");
    assert_eq!(comment["panel"], 7);
    // A new dashboard: the folder takes the place of the uid.
    let r = post_preview(&h, "prod", &preview_body(json!({"uid": null, "folder": "transit"}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let comment: Value = serde_json::from_str(&fake.queries()[1].param("log_comment").unwrap()).unwrap();
    assert_eq!(comment["folder"], "transit");
    assert!(comment.get("uid").is_none());
    // A draft variable with a parent variable.
    let variables: Value = serde_json::from_str::<Value>(NODES).unwrap()["variables"].clone();
    let body = preview_body(json!({
        "panel": null, "max_data_points": null, "variable": "node",
        "dashboard_variables": variables,
        "variables": {"env": {"all": false, "values": ["prod"]}},
    }));
    let r = post_preview(&h, "prod", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["options"][0]["value"], "1794211200000");
    assert!(r.json().get("etag").is_none());
    assert!(fake.queries()[2].sql().contains("SELECT DISTINCT node"));
    assert_eq!(fake.queries()[2].param("max_result_rows").as_deref(), Some("10000"));
    // The log line of the preview.
    let lines: Vec<Value> = h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.iter().filter(|v| v["event"] == "query.run" && v["kind"] == "preview").count(), 3);
}

#[tokio::test]
async fn a24_preview_ignores_the_placement_of_the_draft_variables() {
    // The draft document has one section. A placement of the real document names another section (4.7a).
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let mut variables: Value = serde_json::from_str::<Value>(NODES).unwrap()["variables"].clone();
    variables[2]["placement"] = json!({"section": 5, "x": 0, "y": 0, "w": 6, "h": 2});
    variables[0]["placement"] = json!({"section": 0, "x": 0, "y": 0, "w": 6, "h": 2, "title": "Pick"});
    let body = preview_body(json!({
        "panel": null, "max_data_points": null, "variable": "node",
        "dashboard_variables": variables,
        "variables": {"env": {"all": false, "values": ["prod"]}},
    }));
    let r = post_preview(&h, "prod", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The same body with a panel: a placement that overlaps the draft panel gives no error either.
    let body = preview_body(json!({"dashboard_variables": variables}));
    let r = post_preview(&h, "prod", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn a24_preview_refusals() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let cases: Vec<(&str, Value, u16, &str)> = vec![
        ("both uid and folder", json!({"folder": "transit"}), 400, "bad_request"),
        ("neither uid nor folder", json!({"uid": null}), 400, "bad_request"),
        ("both panel and variable", json!({"variable": "node"}), 400, "bad_request"),
        ("neither panel nor variable", json!({"panel": null, "max_data_points": null}), 400, "bad_request"),
        ("no max_data_points with a panel", json!({"max_data_points": null}), 400, "bad_request"),
        ("max_data_points with a variable", json!({"panel": null, "variable": "node"}), 400, "bad_request"),
        ("unknown uid", json!({"uid": "nothing"}), 404, "not_found"),
        ("unknown folder", json!({"uid": null, "folder": "nothing"}), 404, "not_found"),
        (
            "a variable that is not in the body",
            json!({"panel": null, "max_data_points": null, "variable": "nothing"}),
            400,
            "bad_request",
        ),
        ("a reuse panel", json!({"panel": {"id": 1, "data": {"reuse": {"panel": 2}}}}), 400, "bad_request"),
        ("bad range", json!({"from_ms": 10, "to_ms": 5}), 422, "invalid_time_range"),
        ("bad max_data_points", json!({"max_data_points": 0}), 400, "bad_request"),
        ("an unknown member", json!({"extra": 1}), 400, "bad_request"),
    ];
    for (name, change, status, code) in cases {
        let r = post_preview(&h, "prod", &preview_body(change)).await;
        assert_eq!((r.status, r.code().as_str()), (status, code), "{name}: {}", r.text());
    }
    // An unknown source in the path.
    assert_eq!(post_preview(&h, "nothing", &preview_body(json!({}))).await.status, 404);
    // A draft that breaks section 4 or 7.18: 422 with pointers relative to the body.
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": draft_panel("DROP TABLE x")}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    let mut panel = draft_panel("SELECT 1");
    panel["type"] = json!("candlestick");
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": panel}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/panel/type");
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": draft_panel("SELECT $__nothing(1)")}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/panel/data/query/sql/0");
    assert_eq!(r.json()["error"]["errors"][0]["code"], "bad_format");
    let mut other = draft_panel("SELECT 1");
    other["data"]["query"]["source"] = json!("elsewhere");
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": other}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/panel/data/query/source");
    assert_eq!(r.json()["error"]["errors"][0]["code"], "bad_reference");
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn a24_preview_is_a_write_route() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = call(
        h.addr,
        "POST",
        "/api/sources/prod/preview",
        &[JSON, ("Sec-Fetch-Site", "cross-site")],
        &preview_body(json!({})),
    )
    .await;
    assert_eq!(r.code(), "cross_site_request");
    assert!(fake.queries().is_empty());
    // The panel routes still work: they are reads.
    let e = etag(&h, "nodes").await;
    assert_eq!(post_panel(&h, 1, &panel_body(&e, json!({}))).await.status, 200);
}

#[tokio::test]
async fn a12_the_checks_of_7_18_run_at_save() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let sql_of = |sql: &str| NODES.replacen("\"SELECT node, name, weight, ratio, status\"", &format!("\"{sql}\""), 1);
    let validate = |doc: String| {
        let body = format!("{{\"folder\":\"transit\",\"document\":{doc}}}");
        let addr = h.addr;
        async move { call(addr, "POST", "/api/dashboards?validate_only=true", &[JSON], body.as_bytes()).await }
    };
    // Item 2: the statement check. The pointer names the line of the SQL.
    let r = validate(sql_of("DROP TABLE x")).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    let first = r.json()["error"]["errors"][0].clone();
    assert_eq!(first["code"], "bad_format");
    assert_eq!(first["pointer"], "/sections/1/panels/0/data/query/sql/0");
    assert!(first["line"].is_null() && first["column"].is_null());
    assert!(first["message"].as_str().unwrap().starts_with("column "));
    // Two statements.
    let r = validate(sql_of("SELECT 1; SELECT 2")).await;
    assert_eq!(r.code(), "invalid_document");
    // Item 3: an unknown macro.
    let r = validate(sql_of("SELECT $__nothing(x)")).await;
    assert_eq!(r.json()["error"]["errors"][0]["code"], "bad_format");
    // Item 5: `$__timeInterval` in the SQL of a variable.
    let bad_var = NODES.replacen("\"SELECT DISTINCT node\"", "\"SELECT $__timeInterval(ts) AS t\"", 1);
    let r = validate(bad_var).await;
    assert_eq!(r.json()["error"]["errors"][0]["code"], "not_permitted");
    assert!(r.json()["error"]["errors"][0]["pointer"].as_str().unwrap().starts_with("/variables/0/sql/"));
    // The warnings: a word FINAL, a name that is not a variable.
    let r = validate(sql_of("SELECT x FROM t FINAL WHERE a = $nothing")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let warnings = r.json()["warnings"].clone();
    assert!(warnings.as_array().unwrap().len() >= 2, "{warnings}");
    assert!(warnings[0]["pointer"].as_str().unwrap().starts_with("/sections/1/panels/0/data/query/sql/"));
    assert!(warnings[0]["message"].as_str().unwrap().starts_with("column "));
    // The same checks run at a create, with the warnings in the answer.
    let body = format!(
        "{{\"folder\":\"transit\",\"document\":{}}}",
        doc_text("warned", "W").replacen(
            "\"SELECT node, name, weight, ratio, status\"",
            "\"SELECT x FROM t FINAL\"",
            1
        )
    );
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &super::api::key())], body.as_bytes())
        .await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert!(!r.json()["warnings"].as_array().unwrap().is_empty());
    // A save and a restore refuse a document that breaks the checks.
    let e = etag(&h, "nodes").await;
    let doc = sql_of("DROP TABLE x");
    let r = super::api::put(&h, "nodes", &e, &doc, &super::api::key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
}
