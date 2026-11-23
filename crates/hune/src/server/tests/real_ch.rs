//! The cases A18 to A24 and C05 of 16.8 against a real ClickHouse, through the server.
//!
//! The tests are ignored. Start a ClickHouse as `tests/ci/source.sh` does (the database
//! `analytics` of `tests/fixtures/clickhouse/init.sql`), then run
//! `cargo test -p hune -- --ignored real_ch`. The address comes from `HUNE_TEST_CLICKHOUSE`
//! (default `127.0.0.1:18123`).

use super::api::{JSON, NODES, etag_of};
use super::harness::*;
use hune_core::document::folder_file_bytes;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

const USER: &str = "hune";
const PASSWORD: &str = "hune-test-password";
const ADMIN: (&str, &str) = ("hune_admin", "hune-test-admin-password");

fn ch_addr() -> SocketAddr {
    std::env::var("HUNE_TEST_CLICKHOUSE").unwrap_or_else(|_| "127.0.0.1:18123".to_owned()).parse().unwrap()
}

/// Runs a statement as the administrator and gives the text of the answer.
async fn admin(sql: &str) -> String {
    let sql = sql.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut s = TcpStream::connect_timeout(&ch_addr(), Duration::from_secs(3)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let req = format!(
            "POST /?default_format=TabSeparated HTTP/1.1\r\nHost: x\r\nX-ClickHouse-User: {}\r\nX-ClickHouse-Key: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sql}",
            ADMIN.0,
            ADMIN.1,
            sql.len()
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut text = String::new();
        s.read_to_string(&mut text).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("HTTP/1.1 200"), "{head}\n{body}");
        // A chunked body: drop the sizes.
        if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
            let mut out = String::new();
            let mut rest = body;
            while let Some((size, tail)) = rest.split_once("\r\n") {
                let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
                if n == 0 {
                    break;
                }
                out.push_str(&tail[..n]);
                rest = &tail[n + 2..];
            }
            return out;
        }
        body.to_owned()
    })
    .await
    .unwrap()
}

/// The dashboard of the fixture, with its SQL on the table `analytics.seed`.
fn seed_dashboard() -> String {
    NODES
        .replace("analytics.node_weight", "analytics.seed")
        .replace("$__timeInterval(ts)", "$__timeInterval(event_time)")
        .replace("$__timeFilter(ts)", "$__timeFilter(event_time)")
        .replace("\"SELECT node, name, weight, ratio, status\"", "\"SELECT node, weight, blocks, active\"")
        .replace("\"FROM analytics.nodes\"", "\"FROM analytics.seed\"")
        .replace("OR account = '${account}'", "OR node = '${account}'")
        .replace("AND env = '${env}'", "AND '${env}' = 'prod'")
}

fn fill(root: &Path) {
    std::fs::create_dir_all(root.join("transit")).unwrap();
    std::fs::write(root.join("transit/_folder.json"), folder_file_bytes("Transit desk")).unwrap();
    std::fs::write(root.join("transit/nodes.json"), seed_dashboard()).unwrap();
}

async fn start(edit: impl FnOnce(&mut crate::config::Config)) -> Harness {
    let addr = ch_addr();
    let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
    let h = Harness::start_with(timing, fill, |c| {
        let b = &mut c.clickhouse[0];
        b.url.host = addr.ip().to_string();
        b.url.port = addr.port();
        b.url.text = format!("http://{addr}");
        b.user = USER.to_owned();
        b.password = crate::config::Secret::new(PASSWORD.to_owned());
        edit(c);
    })
    .await;
    wait_source_state(&h, "up").await;
    h
}

fn panel_request(etag: &str, extra: Value) -> Vec<u8> {
    let mut v = json!({
        "etag": etag,
        "from_ms": 1_794_211_200_000_i64,
        "to_ms": 1_794_211_800_000_i64,
        "max_data_points": 30,
        "variables": {
            "node": {"all": false, "values": ["v0", "v1", "v2"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
        }
    });
    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            m.insert(k.clone(), x.clone());
        }
    }
    serde_json::to_vec(&v).unwrap()
}

fn preview_request(sql: &str) -> Vec<u8> {
    let doc: Value = serde_json::from_str(&seed_dashboard()).unwrap();
    let mut panel = doc["sections"][0]["panels"][0].clone();
    panel["id"] = json!(9);
    panel["data"]["query"]["sql"] = json!([sql]);
    panel["data"]["query"]["min_interval"] = Value::Null;
    serde_json::to_vec(&json!({
        "uid": "nodes", "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_211_800_000_i64,
        "max_data_points": 100, "dashboard_variables": [], "variables": {}, "panel": panel,
    }))
    .unwrap()
}

async fn preview(h: &Harness, sql: &str) -> Resp {
    call(h.addr, "POST", "/api/sources/prod/preview", &[JSON], &preview_request(sql)).await
}

async fn seed_etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

/// The number of queries of Hune in the query log, for a kind.
async fn logged_queries(kind: &str) -> u64 {
    admin("SYSTEM FLUSH LOGS").await;
    admin(&format!(
        "SELECT count() FROM system.query_log WHERE type = 'QueryFinish' AND JSONExtractString(log_comment, 'app') = 'hune' AND JSONExtractString(log_comment, 'kind') = '{kind}'"
    ))
    .await
    .trim()
    .parse()
    .unwrap()
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a18_panel_query_gives_the_expected_frame() {
    let h = start(|_| {}).await;
    let e = seed_etag(&h).await;
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &panel_request(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["etag"], e);
    assert_eq!(j["interval_ms"], 60_000);
    let names: Vec<&str> = j["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["t", "node", "weight"]);
    let kinds: Vec<&str> = j["columns"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["time", "text", "number_text"]);
    assert_eq!(j["columns"][0]["type"], "DateTime");
    assert_eq!(j["columns"][1]["type"], "LowCardinality(String)");
    assert!(j["columns"][2]["type"].as_str().unwrap().starts_with("Decimal"));
    // 30 seed rows in 10 minutes, 3 nodes, an interval of 1 minute.
    assert!(j["rows"].as_u64().unwrap() >= 3);
    let first_t = j["values"][0][0].as_u64().unwrap();
    assert_eq!(first_t % 60, 0);
    assert!(j["values"][2][0].as_str().unwrap().contains('.'), "an exact decimal text");
    assert!(j["query_id"].as_str().unwrap().len() == 36);
    // The query log of ClickHouse holds the attribution of 8.10.
    admin("SYSTEM FLUSH LOGS").await;
    let comment = admin(&format!(
        "SELECT log_comment FROM system.query_log WHERE query_id = '{}' AND type = 'QueryFinish'",
        j["query_id"].as_str().unwrap()
    ))
    .await;
    let comment: Value = serde_json::from_str(comment.trim()).unwrap();
    assert_eq!(comment["app"], "hune");
    assert_eq!(comment["kind"], "panel");
    assert_eq!(comment["uid"], "nodes");
    assert_eq!(comment["panel"], 1);
    assert_eq!(comment["subject"], "-");
    // The second panel (a table with text variables) runs too.
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/2/query", &[JSON], &panel_request(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["columns"].as_array().unwrap().len(), 4);
    assert_eq!(r.json()["columns"][3]["kind"], "bool");
    assert_eq!(r.json()["rows"], 30);
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a21_a_refused_value_sends_no_query() {
    let h = start(|_| {}).await;
    let e = seed_etag(&h).await;
    let before = logged_queries("panel").await;
    let bad = json!({"variables": {
        "node": {"all": false, "values": ["v0"]},
        "account": {"all": false, "values": ["a", "b"]},
        "env": {"all": false, "values": ["prod"]},
    }});
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/2/query", &[JSON], &panel_request(&e, bad)).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"));
    // A value that tries to leave its literal is refused or stays inside it. Either way, no second statement runs.
    let tricky = json!({"variables": {
        "node": {"all": false, "values": ["v0"]},
        "account": {"all": false, "values": ["x' OR 1=1 --"]},
        "env": {"all": false, "values": ["prod"]},
    }});
    let r = call(h.addr, "POST", "/api/dashboards/nodes/panels/2/query", &[JSON], &panel_request(&e, tricky)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 0, "the quote is escaped, so the text matches no node");
    assert_eq!(logged_queries("panel").await, before + 1, "only the second request reached ClickHouse");
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a22_options_of_a_variable() {
    let h = start(|_| {}).await;
    let e = seed_etag(&h).await;
    let req = json!({"etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_211_800_000_i64, "variables": {}});
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&req).unwrap(),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(
        r.json()["options"],
        json!([{"value": "v0", "text": "v0"}, {"value": "v1", "text": "v1"}, {"value": "v2", "text": "v2"}])
    );
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a23_sources_list_shows_no_secret() {
    let h = start(|_| {}).await;
    let r = get(h.addr, "/api/sources").await;
    assert_eq!(r.json()["items"][0], json!({"id": "prod", "label": "Production", "kind": "clickhouse", "state": "up"}));
    for needle in [PASSWORD, USER, "18123", "127.0.0.1"] {
        assert!(!r.text().contains(needle), "{needle}");
    }
    let log = h.logger.memory_lines().join("\n");
    assert!(!log.contains(PASSWORD));
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a24_preview_and_the_errors_of_clickhouse() {
    let h = start(|_| {}).await;
    let r = preview(&h, "SELECT count() AS n FROM analytics.seed").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["columns"][0], json!({"name": "n", "type": "UInt64", "kind": "number_text"}));
    assert_eq!(r.json()["values"][0], json!(["30"]));
    assert!(r.json().get("etag").is_none());
    // The codes of 8.5, from the real ClickHouse.
    let cases: [(&str, u16, &str); 4] = [
        ("SELECT * FROM analytics.no_such_table", 422, "query_error"),
        ("SELECT * FROM system.users", 502, "source_access_denied"),
        ("SELECT number FROM numbers(200000)", 422, "query_limit"),
        ("SELECT secret FROM analytics.seed", 422, "query_error"),
    ];
    for (sql, status, code) in cases {
        let r = preview(&h, sql).await;
        assert_eq!((r.status, r.code().as_str()), (status, code), "{sql}: {}", r.text());
        assert!(r.json()["error"]["message"].as_str().unwrap().lines().count() <= 1);
    }
    let r = preview(&h, "SELECT * FROM analytics.no_such_table").await;
    assert_eq!(r.json()["error"]["clickhouse_code"], 60);
    // The writes and the other statements never run: the statement check refuses them before the source.
    for sql in ["INSERT INTO analytics.seed VALUES (1)", "DROP TABLE analytics.seed", "SELECT 1; SELECT 2"] {
        let r = preview(&h, sql).await;
        assert_eq!(r.status, 422, "{sql}");
    }
    assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a_time_limit_gives_query_timeout() {
    let h = start(|c| c.clickhouse[0].max_execution_time = Duration::from_secs(1)).await;
    let r = preview(&h, "SELECT sleep(3) AS s").await;
    assert_eq!((r.status, r.code().as_str()), (504, "query_timeout"), "{}", r.text());
    assert_eq!(r.json()["error"]["source"], "prod");
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_c05_a_client_that_leaves_cancels_the_query_in_clickhouse() {
    let h = start(|_| {}).await;
    let running = || async {
        admin("SELECT count() FROM system.processes WHERE query LIKE 'SELECT sleepEachRow%' AND query NOT LIKE '%system.processes%'")
            .await
            .trim()
            .parse::<u64>()
            .unwrap()
    };
    assert_eq!(running().await, 0);
    let mut c = Client::connect(h.addr).await;
    c.send(&request(
        "POST",
        "/api/sources/prod/preview",
        &[JSON],
        &preview_request("SELECT sleepEachRow(0.1) AS s FROM numbers(300) SETTINGS max_block_size = 1"),
    ))
    .await;
    let mut seen = false;
    for _ in 0..100 {
        if running().await == 1 {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(seen, "the query runs in ClickHouse");
    let left = std::time::Instant::now();
    drop(c);
    loop {
        if running().await == 0 {
            break;
        }
        assert!(left.elapsed() < Duration::from_secs(2), "the query must leave system.processes within 2 s");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!("C05: the query left system.processes {} ms after the client left", left.elapsed().as_millis());
    // The query log shows a cancelled query, not a finished one.
    admin("SYSTEM FLUSH LOGS").await;
    let finished = admin("SELECT count() FROM system.query_log WHERE type = 'QueryFinish' AND query LIKE 'SELECT sleepEachRow%' AND JSONExtractString(log_comment, 'app') = 'hune'").await;
    assert_eq!(finished.trim(), "0");
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_a_misconfigured_account_is_reported() {
    for (user, state) in [("hune_ro0", "misconfigured"), ("hune_ro1", "misconfigured"), ("hune_extra", "misconfigured")]
    {
        let addr = ch_addr();
        let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
        let h = Harness::start_with(timing, fill, |c| {
            let b = &mut c.clickhouse[0];
            b.url.host = addr.ip().to_string();
            b.url.port = addr.port();
            b.user = user.to_owned();
            b.password = crate::config::Secret::new(PASSWORD.to_owned());
        })
        .await;
        wait_source_state(&h, state).await;
        let e = seed_etag(&h).await;
        let r =
            call(h.addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], &panel_request(&e, json!({}))).await;
        assert_eq!((r.status, r.code().as_str()), (502, "source_misconfigured"), "{user}: {}", r.text());
    }
}

// ---- F27: the variables `interval` and `constant` (C33, C34 of 16.9) ----------------------------------------------

/// The text of the constant of the document: it has the hostile characters of C32.
const F27_CONST: &str = "a'b\"c$d\\e;--x";

/// The seed dashboard with an `interval` variable `bucket` and a `constant` variable `k`. The SQL of panel 1 names
/// both. For the range of the requests (10 minutes, 30 steps, 10 s minimum) the text of "Auto" is `20s` (R = 20 s).
fn f27_dashboard() -> String {
    let mut d: Value = serde_json::from_str(&seed_dashboard()).unwrap();
    let vars = d["variables"].as_array_mut().unwrap();
    vars.push(json!({
        "kind": "interval", "name": "bucket", "label": "Bucket", "hide": "none",
        "intervals": ["1m", "10m", "1h", "1d"], "auto": true, "auto_count": 30, "auto_min": "10s",
        "default": "$__auto"
    }));
    vars.push(json!({"kind": "constant", "name": "k", "label": "A note", "hide": "variable", "value": F27_CONST}));
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("$__timeFilter")).unwrap();
    sql.insert(at + 1, json!("  AND parseTimeDelta('${bucket}') > 0 AND '${k}' != ''"));
    serde_json::to_string_pretty(&d).unwrap()
}

async fn start_f27() -> Harness {
    let addr = ch_addr();
    let text = f27_dashboard();
    let fill = move |root: &Path| {
        std::fs::create_dir_all(root.join("transit")).unwrap();
        std::fs::write(root.join("transit/_folder.json"), folder_file_bytes("Transit desk")).unwrap();
        std::fs::write(root.join("transit/nodes.json"), text).unwrap();
    };
    let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
    let h = Harness::start_with(timing, fill, |c| {
        let b = &mut c.clickhouse[0];
        b.url.host = addr.ip().to_string();
        b.url.port = addr.port();
        b.url.text = format!("http://{addr}");
        b.user = USER.to_owned();
        b.password = crate::config::Secret::new(PASSWORD.to_owned());
    })
    .await;
    wait_source_state(&h, "up").await;
    h
}

fn f27_variables(bucket: &str, k: Option<Value>) -> Value {
    let mut v = json!({
        "node": {"all": false, "values": ["v0", "v1", "v2"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
        "bucket": {"all": false, "values": [bucket]},
    });
    if let Some(k) = k {
        v["k"] = k;
    }
    v
}

/// Tells whether the query text of a finished query in the log of ClickHouse holds `needle` (a plain text).
async fn query_has(query_id: &str, needle: &str) -> bool {
    let literal = format!("'{}'", needle.replace('\\', "\\\\").replace('\'', "\\'"));
    admin(&format!(
        "SELECT count() FROM system.query_log WHERE query_id = '{query_id}' AND type = 'QueryFinish' AND position(query, {literal}) > 0"
    ))
    .await
    .trim()
        == "1"
}

/// Sends the panel request (6.20) and the options request (6.21) with the same values, and gives the two answers.
async fn f27_two_routes(h: &Harness, etag: &str, variables: Value) -> [Resp; 2] {
    let panel = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/panels/1/query",
        &[JSON],
        &panel_request(etag, json!({"variables": variables.clone()})),
    )
    .await;
    let body = json!({"etag": etag, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_211_800_000_i64,
        "variables": variables});
    let options = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&body).unwrap(),
    )
    .await;
    [panel, options]
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_c33_a_hostile_interval_request_gives_422_and_no_query() {
    let h = start_f27().await;
    let e = seed_etag(&h).await;
    // The control: an item and "Auto" run on the real ClickHouse. The query of Hune holds the text of the item.
    let mut ran = Vec::new();
    for (value, text) in [("1m", "1m"), ("1d", "1d"), ("$__auto", "20s")] {
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards/nodes/panels/1/query",
            &[JSON],
            &panel_request(&e, json!({"variables": f27_variables(value, None)})),
        )
        .await;
        assert_eq!(r.status, 200, "{value}: {}", r.text());
        ran.push((r.json()["query_id"].as_str().unwrap().to_owned(), text));
    }
    admin("SYSTEM FLUSH LOGS").await;
    for (id, text) in &ran {
        assert!(query_has(id, &format!("parseTimeDelta('{text}')")).await, "{text}");
    }
    // The refusals. The count of the queries of Hune in the log of ClickHouse does not move.
    let before = (logged_queries("panel").await, logged_queries("variable").await);
    let mut hostile: Vec<String> = [
        "7m",
        "1m ",
        " 1m",
        "1M",
        "1H",
        "5m",
        "",
        "$__AUTO",
        "1m'; DROP TABLE analytics.seed; --",
        "1m\" OR \"1\"=\"1",
        "1m`",
        "1m\\",
        "1m\u{0}",
        "${bucket}",
        "$__from",
        "1m,10m",
        "'",
        "\\",
        "1m) OR (1=1",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    hostile.extend((0..20).map(|i| format!("{}m{}", i + 100, "'".repeat(i % 3))));
    for bad in &hostile {
        for r in f27_two_routes(&h, &e, f27_variables(bad, None)).await {
            assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{bad:?}: {}", r.text());
            assert_eq!(r.json()["error"]["variable"], "bucket");
        }
    }
    // Two values, no value and "All".
    for (all, values) in [(false, json!(["1m", "10m"])), (false, json!([])), (true, json!([]))] {
        let mut v = f27_variables("1m", None);
        v["bucket"] = json!({"all": all, "values": values});
        for r in f27_two_routes(&h, &e, v).await {
            assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{all} {values}: {}", r.text());
        }
    }
    let after = (logged_queries("panel").await, logged_queries("variable").await);
    assert_eq!(after, before, "a refused request reached ClickHouse");
    assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
    println!(
        "C33: {} hostile values and 3 forms on 2 routes: all 422, no query in the log of ClickHouse",
        hostile.len()
    );
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_c34_a_constant_request_with_a_different_value_gives_422_and_no_query() {
    let h = start_f27().await;
    let e = seed_etag(&h).await;
    // The control: no member, and the equal member, run with the value of the document in its literal and its name.
    let mut ids = Vec::new();
    for member in [None, Some(json!({"all": false, "values": [F27_CONST]}))] {
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards/nodes/panels/1/query",
            &[JSON],
            &panel_request(&e, json!({"variables": f27_variables("1m", member)})),
        )
        .await;
        assert_eq!(r.status, 200, "{}", r.text());
        ids.push(r.json()["query_id"].as_str().unwrap().to_owned());
    }
    admin("SYSTEM FLUSH LOGS").await;
    // The escapes of 7.8: the quote and the backslash stay inside one literal.
    for id in &ids {
        assert!(query_has(id, "'a\\'b\"c$d\\\\e;--x'").await, "the escaped value of the document is in the SQL");
        assert!(query_has(id, "parseTimeDelta('1m')").await);
    }
    let same =
        |id: &str| format!("SELECT query FROM system.query_log WHERE query_id = '{id}' AND type = 'QueryFinish'");
    assert_eq!(admin(&same(&ids[0])).await, admin(&same(&ids[1])).await, "the equal member gives the same SQL");
    let before = (logged_queries("panel").await, logged_queries("variable").await);
    for member in [
        json!({"all": false, "values": ["zzz"]}),
        json!({"all": false, "values": [""]}),
        json!({"all": false, "values": []}),
        json!({"all": false, "values": [F27_CONST, F27_CONST]}),
        json!({"all": false, "values": ["x'; DROP TABLE analytics.seed; --"]}),
        json!({"all": true, "values": []}),
        json!({"all": true, "values": [F27_CONST]}),
    ] {
        for r in f27_two_routes(&h, &e, f27_variables("1m", Some(member.clone()))).await {
            assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{member}: {}", r.text());
            assert_eq!(r.json()["error"]["variable"], "k");
            assert_eq!(r.json()["error"]["reason"], "A constant takes its value from the document.");
        }
    }
    let after = (logged_queries("panel").await, logged_queries("variable").await);
    assert_eq!(after, before, "a refused request reached ClickHouse");
    assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
    println!("C34: 7 different members on 2 routes: all 422, no query in the log of ClickHouse");
}

// ---- the lexer gap of review 3 (finding 1; 7.5) ----

/// The texts of `crates/hune-core/tests/sql_lexer_clickhouse_gap.rs`, as SQL lines (4.16).
const GAP_SLASHES: [&str; 3] = [
    "SELECT node, weight FROM analytics.seed   // the team's main table",
    "WHERE blocks > $min                         // don't show small values",
    "ORDER BY weight DESC LIMIT 3",
];
const GAP_HEREDOC: [&str; 3] = [
    "SELECT node, weight, $1$'$1$ AS h FROM analytics.seed",
    "WHERE blocks > $min -- '",
    "ORDER BY weight DESC LIMIT 3",
];
const GAP_QPARAM: [&str; 2] =
    ["SELECT {x:String} AS t, count() AS n // the user's table:", "FROM analytics.{x:Identifier} // that's all"];
const GAP_ADHOC: [&str; 3] = [
    "SELECT node, weight FROM analytics.seed // it's",
    "WHERE node = ' AND $__adhocFilters -- '",
    "ORDER BY weight DESC LIMIT 3",
];
/// The value of the viewer: before the fix, ClickHouse read it as a second SELECT on `analytics.secret`.
const GAP_HOSTILE: &str = "1000 UNION ALL SELECT toString(x), toDecimal128(4242, 9) FROM analytics.secret --";

/// The seed dashboard with a text variable `min`, an `adhoc` variable `f`, and three table panels: 2 (two `//`
/// comments), 4 (a heredoc with the tag `1`) and 5 (the macro after a `//` comment).
fn gap_dashboard() -> String {
    let mut d: Value = serde_json::from_str(&seed_dashboard()).unwrap();
    let vars = d["variables"].as_array_mut().unwrap();
    vars.push(
        json!({"kind": "text", "name": "min", "label": null, "hide": "none", "value_type": "text", "default": "0"}),
    );
    vars.push(json!({
        "kind": "adhoc", "name": "f", "label": "Filters", "hide": "none", "source": "prod", "keys": ["node"]
    }));
    // Panel 2 is the table of the section "Details".
    let panels = d["sections"][1]["panels"].as_array_mut().unwrap();
    let at = panels.iter().position(|p| p["id"] == 2).unwrap();
    panels[at]["data"]["query"]["sql"] = json!(GAP_SLASHES);
    for (id, y, sql) in [(4, 10, &GAP_HEREDOC[..]), (5, 18, &GAP_ADHOC[..])] {
        let mut p = panels[at].clone();
        p["id"] = json!(id);
        p["title"] = json!(format!("Gap {id}"));
        p["grid"] = json!({"x": 0, "y": y, "w": 24, "h": 8});
        p["data"]["query"]["sql"] = json!(sql);
        panels.push(p);
    }
    serde_json::to_string_pretty(&d).unwrap()
}

fn gap_variables(min: &str, filters: Value) -> Value {
    json!({
        "node": {"all": false, "values": ["v0", "v1", "v2"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
        "min": {"all": false, "values": [min]},
        "f": {"filters": filters},
    })
}

#[tokio::test]
#[ignore = "needs a ClickHouse: see the header of this file"]
async fn real_the_texts_of_the_lexer_gap_send_no_value_as_code() {
    let addr = ch_addr();
    let text = gap_dashboard();
    let fill = move |root: &Path| {
        std::fs::create_dir_all(root.join("transit")).unwrap();
        std::fs::write(root.join("transit/_folder.json"), folder_file_bytes("Transit desk")).unwrap();
        std::fs::write(root.join("transit/nodes.json"), text).unwrap();
    };
    let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
    let h = Harness::start_with(timing, fill, |c| {
        let b = &mut c.clickhouse[0];
        b.url.host = addr.ip().to_string();
        b.url.port = addr.port();
        b.url.text = format!("http://{addr}");
        b.user = USER.to_owned();
        b.password = crate::config::Secret::new(PASSWORD.to_owned());
        b.adhoc_columns = Some(vec!["node".to_owned()]);
    })
    .await;
    wait_source_state(&h, "up").await;
    let e = seed_etag(&h).await;
    let server = h.addr;
    let panel = |id: u32, variables: Value| {
        let body = panel_request(&e, json!({ "variables": variables }));
        let path = format!("/api/dashboards/nodes/panels/{id}/query");
        async move { call(server, "POST", &path, &[JSON], &body).await }
    };
    let before = (logged_queries("panel").await, logged_queries("preview").await);
    // A text variable between two `//` comments, and after a heredoc with the tag `1`: 422 before any query.
    for id in [2, 4] {
        let r = panel(id, gap_variables(GAP_HOSTILE, json!([]))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "panel {id}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "min");
        assert_eq!(r.json()["error"]["reason"], "Outside a string, only a number is permitted.");
    }
    // A query parameter between two `//` comments: the save refuses `{x:Identifier}` for a text variable, and so does
    // the preview.
    let mut doc: Value = serde_json::from_str(&gap_dashboard()).unwrap();
    doc["variables"].as_array_mut().unwrap().push(
        json!({"kind": "text", "name": "x", "label": null, "hide": "none", "value_type": "text", "default": "seed"}),
    );
    let panels = doc["sections"][1]["panels"].as_array_mut().unwrap();
    let at = panels.iter().position(|p| p["id"] == 2).unwrap();
    panels[at]["data"]["query"]["sql"] = json!(GAP_QPARAM);
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes?validate_only=true",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &super::api::key())],
        &super::api::put_body(&doc.to_string(), "m"),
    )
    .await;
    assert_eq!(r.status, 422, "{}", r.text());
    assert!(r.text().contains("Identifier"), "{}", r.text());
    let mut draft: Value = serde_json::from_slice(&preview_request("SELECT 1")).unwrap();
    draft["panel"]["data"]["query"]["sql"] = json!(GAP_QPARAM);
    draft["dashboard_variables"] = json!([
        {"kind": "text", "name": "x", "label": null, "hide": "none", "value_type": "text", "default": "seed"}
    ]);
    draft["variables"] = json!({"x": {"all": false, "values": ["secret"]}});
    let r = call(h.addr, "POST", "/api/sources/prod/preview", &[JSON], &serde_json::to_vec(&draft).unwrap()).await;
    assert_eq!(r.status, 422, "{}", r.text());
    let after = (logged_queries("panel").await, logged_queries("preview").await);
    assert_eq!(after, before, "a refused text reached ClickHouse");
    // The controls: an honest value runs, and the `//` comments are comments for ClickHouse.
    let r = panel(2, gap_variables("5", json!([]))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 3);
    // The macro after a `//` comment is inside the string that ClickHouse reads: its text stays, and no value of the
    // filter reaches the SQL.
    let r = panel(5, gap_variables("0", json!([{"key": "node", "op": "=", "value": GAP_HOSTILE}]))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["rows"], 0);
    let id = r.json()["query_id"].as_str().unwrap().to_owned();
    admin("SYSTEM FLUSH LOGS").await;
    assert!(query_has(&id, "' AND $__adhocFilters -- '").await, "the macro stays text in the string");
    assert!(!query_has(&id, "analytics.secret").await, "no value of the filter is in the SQL");
    assert_eq!(logged_queries("panel").await, before.0 + 2, "only the two controls reached ClickHouse");
    assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
}
