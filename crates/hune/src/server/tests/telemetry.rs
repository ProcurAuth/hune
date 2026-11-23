//! The OTLP exporter in the server (M7, 14.9, 14.10): a fake collector on `127.0.0.1`, a server with
//! `telemetry.enabled = true`, and the JSON that the collector received.
//! Tests are not product code: they can use trait objects.

use super::api::{JSON, doc_text, fill, key, post_body};
use super::authzen_pdp::*;
use super::fake_ch::{FRAME_BODY, Fake, error, healthy, ok};
use super::harness::*;
use crate::config::{Config, TelemetryConfig, TelemetryEnabled};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const TRACE_ID: &str = "0af7651916cd43dd8448eb211c80319c";
const PARENT_ID: &str = "b7ad6b7169203331";

/// A fake OTLP collector: it keeps each body and answers with a status that the test sets.
struct Collector {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    status: Arc<AtomicU16>,
    task: JoinHandle<()>,
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Collector {
    async fn start() -> Collector {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(AtomicU16::new(200));
        let (b, s) = (bodies.clone(), status.clone());
        let task = tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else { return };
                let (b, s) = (b.clone(), s.clone());
                tokio::spawn(async move {
                    let service = service_fn(move |req: http::Request<hyper::body::Incoming>| {
                        let (b, s) = (b.clone(), s.clone());
                        async move {
                            assert_eq!(req.uri().path(), "/v1/traces");
                            assert_eq!(req.headers()["content-type"], "application/json");
                            let body = req.into_body().collect().await.map(|x| x.to_bytes()).unwrap_or_default();
                            b.lock().unwrap().push(serde_json::from_slice(&body).unwrap_or_default());
                            let mut resp = http::Response::new(Full::new(Bytes::from_static(b"{}")));
                            *resp.status_mut() = http::StatusCode::from_u16(s.load(Ordering::SeqCst)).unwrap();
                            Ok::<_, std::convert::Infallible>(resp)
                        }
                    });
                    let _ =
                        hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tcp), service).await;
                });
            }
        });
        Collector { port, bodies, status, task }
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The spans of all exports, in the order of arrival.
    fn spans(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for body in self.bodies.lock().unwrap().iter() {
            if let Some(list) = body["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array() {
                out.extend(list.iter().cloned());
            }
        }
        out
    }

    fn exports(&self) -> usize {
        self.bodies.lock().unwrap().len()
    }

    fn resource(&self) -> Value {
        self.bodies.lock().unwrap()[0]["resourceSpans"][0]["resource"].clone()
    }
}

fn enabled(c: &mut Config, collector: &Collector, interval: Duration) {
    c.telemetry = TelemetryConfig::Enabled(TelemetryEnabled {
        otlp_endpoint: collector.endpoint(),
        ca_file: None,
        ca_pem: None,
        service_name: "hune-test".to_owned(),
        export_interval: interval,
    });
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timeout while waiting for: {what}");
}

fn named<'a>(spans: &'a [Value], name: &str) -> Vec<&'a Value> {
    spans.iter().filter(|s| s["name"] == name).collect()
}

fn one<'a>(spans: &'a [Value], name: &str) -> &'a Value {
    let found = named(spans, name);
    assert_eq!(found.len(), 1, "{name}: {spans:#?}");
    found[0]
}

/// The value of an attribute of an exported span: a text, an integer or a boolean.
fn attr(span: &Value, key: &str) -> Option<Value> {
    let list = span["attributes"].as_array()?;
    let value = &list.iter().find(|a| a["key"] == key)?["value"];
    if let Some(s) = value.get("stringValue") {
        return Some(s.clone());
    }
    if let Some(i) = value.get("intValue") {
        return Some(json!(i.as_str().and_then(|t| t.parse::<i64>().ok())));
    }
    value.get("boolValue").cloned()
}

fn status_code(span: &Value) -> i64 {
    span["status"]["code"].as_i64().unwrap_or(0)
}

/// The keys that 14.10 allows, over the four span types.
const ALLOWED_KEYS: [&str; 17] = [
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "client.address",
    "hune.request_id",
    "hune.dashboard.uid",
    "hune.panel.id",
    "error.type",
    "db.system.name",
    "server.address",
    "server.port",
    "hune.source",
    "hune.query_id",
    "hune.rows",
    "hune.store.backend",
    "hune.questions",
    "other",
];

fn panel_body(etag: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "etag": etag,
        "from_ms": 1_794_211_200_000_i64,
        "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200,
        "variables": {
            "node": {"all": false, "values": ["a1", "b2"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
        }
    }))
    .unwrap()
}

async fn etag(h: &Harness) -> String {
    super::api::etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

async fn post_panel(h: &Harness, extra: &[(&str, &str)], body: &[u8]) -> Resp {
    let mut headers = vec![JSON];
    headers.extend_from_slice(extra);
    call(h.addr, "POST", "/api/dashboards/nodes/panels/1/query", &headers, body).await
}

#[tokio::test]
async fn spans_follow_14_10_and_the_failures_show() {
    // 0: the query works. 1: ClickHouse code 202 (503). 2: ClickHouse code 62 (422).
    let mode = Arc::new(AtomicU8::new(0));
    let m = mode.clone();
    let fake = Fake::start(healthy(move |rec| match m.load(Ordering::SeqCst) {
        1 => error(rec, 500, 202, "Code: 202. DB::Exception: too many"),
        2 => error(rec, 500, 62, "Code: 62. DB::Exception: syntax"),
        _ => ok(rec, FRAME_BODY),
    }))
    .await;
    let collector = Collector::start().await;
    let h = start_with_fake(&fake, fill, |c| enabled(c, &collector, Duration::from_millis(100))).await;
    // The ops listener makes no span.
    assert_eq!(get(h.ops, "/livez").await.status, 200);
    assert_eq!(get(h.ops, "/metrics").await.status, 200);
    // A: a read with an incoming `traceparent`.
    let parent = format!("00-{TRACE_ID}-{PARENT_ID}-01");
    let r = call(h.addr, "GET", "/api/dashboards/nodes", &[("traceparent", &parent), ("X-Request-Id", "req-t1")], b"")
        .await;
    assert_eq!(r.status, 200);
    let e = super::api::etag_of(&r);
    // B: a create, with a store call.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &doc_text("tele1", "Tele")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    // C: a panel query that works.
    let r = post_panel(&h, &[], &panel_body(&e)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let query_id = r.json()["query_id"].as_str().unwrap().to_owned();
    // D: an unknown dashboard.
    assert_eq!(get(h.addr, "/api/dashboards/unknown").await.status, 404);
    // E: a panel query that fails with a ClickHouse error.
    mode.store(1, Ordering::SeqCst);
    let r = post_panel(&h, &[], &panel_body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"));
    // F: a ClickHouse error with a code in the answer of the source.
    mode.store(2, Ordering::SeqCst);
    let r = post_panel(&h, &[], &panel_body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"));
    wait_for("the spans", || {
        let spans = collector.spans();
        named(&spans, "POST /api/dashboards/{uid}/panels/{id}/query").len() == 3
            && named(&spans, "clickhouse.query").len() == 3
            && !named(&spans, "GET /api/dashboards/{uid}").is_empty()
            && !named(&spans, "store.create").is_empty()
    })
    .await;
    let spans = collector.spans();

    // The resource.
    let resource = collector.resource();
    let service = |k: &str| {
        resource["attributes"].as_array().unwrap().iter().find(|a| a["key"] == k).unwrap()["value"]["stringValue"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(service("service.name"), "hune-test");
    assert_eq!(service("service.version"), env!("CARGO_PKG_VERSION"));

    // A: SERVER span, child of the incoming trace.
    let read = named(&spans, "GET /api/dashboards/{uid}")[0];
    assert_eq!(read["kind"], 2);
    assert_eq!(read["traceId"], TRACE_ID);
    assert_eq!(read["parentSpanId"], PARENT_ID);
    assert_eq!(attr(read, "http.request.method"), Some(json!("GET")));
    assert_eq!(attr(read, "http.route"), Some(json!("/api/dashboards/{uid}")));
    assert_eq!(attr(read, "http.response.status_code"), Some(json!(200)));
    assert_eq!(attr(read, "client.address"), Some(json!("127.0.0.1")));
    assert_eq!(attr(read, "hune.request_id"), Some(json!("req-t1")));
    assert_eq!(attr(read, "hune.dashboard.uid"), Some(json!("nodes")));
    assert_eq!(attr(read, "error.type"), None);
    assert_eq!(status_code(read), 0);

    // B: the store span is a child of the SERVER span.
    let create = one(&spans, "POST /api/dashboards");
    assert_eq!(create["parentSpanId"], Value::Null, "no incoming traceparent: a new trace");
    assert_eq!(attr(create, "http.response.status_code"), Some(json!(201)));
    let store = one(&spans, "store.create");
    assert_eq!(store["kind"], 3);
    assert_eq!(store["traceId"], create["traceId"]);
    assert_eq!(store["parentSpanId"], create["spanId"]);
    assert_eq!(attr(store, "hune.store.backend"), Some(json!("volume")));
    assert_eq!(attr(store, "db.system.name"), None, "PostgreSQL only");
    assert_eq!(status_code(store), 0);

    // C and E: the SERVER spans of the panel query, with the CLIENT spans as children.
    let servers = named(&spans, "POST /api/dashboards/{uid}/panels/{id}/query");
    let (good, bad, rejected) = (servers[0], servers[1], servers[2]);
    assert_eq!(attr(good, "hune.panel.id"), Some(json!(1)));
    assert_eq!(attr(good, "hune.dashboard.uid"), Some(json!("nodes")));
    let queries = named(&spans, "clickhouse.query");
    let (q_good, q_bad, q_rejected) = (queries[0], queries[1], queries[2]);
    assert_eq!(q_good["kind"], 3);
    assert_eq!(q_good["traceId"], good["traceId"]);
    assert_eq!(q_good["parentSpanId"], good["spanId"]);
    assert_eq!(attr(q_good, "db.system.name"), Some(json!("clickhouse")));
    assert_eq!(attr(q_good, "server.address"), Some(json!("127.0.0.1")));
    assert_eq!(attr(q_good, "server.port"), Some(json!(fake.addr.port())));
    assert_eq!(attr(q_good, "hune.source"), Some(json!("prod")));
    assert_eq!(attr(q_good, "hune.query_id"), Some(json!(query_id)));
    assert_eq!(attr(q_good, "hune.rows"), Some(json!(2)));
    assert_eq!(status_code(q_good), 0);
    // ClickHouse got the `traceparent` of the CLIENT span.
    let sent = fake
        .queries()
        .into_iter()
        .find(|rec| rec.query_id() == query_id)
        .and_then(|rec| rec.header("traceparent").map(str::to_owned))
        .expect("a traceparent to ClickHouse");
    let span_id = q_good["spanId"].as_str().unwrap();
    assert_eq!(sent, format!("00-{}-{span_id}-01", q_good["traceId"].as_str().unwrap()));
    // E: the status of the failure.
    assert_eq!(attr(bad, "http.response.status_code"), Some(json!(503)));
    assert_eq!(attr(bad, "error.type"), Some(json!("source_unavailable")));
    assert_eq!(status_code(bad), 2, "a status of 500 or more is an error");
    assert_eq!(q_bad["parentSpanId"], bad["spanId"]);
    assert_eq!(status_code(q_bad), 2);
    assert_eq!(attr(q_bad, "error.type"), Some(json!("source_unavailable")));
    assert_eq!(attr(q_bad, "hune.rows"), None);
    // F: the ClickHouse code is the `error.type` of the CLIENT span. A status below 500 is not an error
    // for the SERVER span.
    assert_eq!(attr(rejected, "error.type"), Some(json!("query_error")));
    assert_eq!(status_code(rejected), 0);
    assert_eq!(q_rejected["parentSpanId"], rejected["spanId"]);
    assert_eq!(status_code(q_rejected), 2, "a CLIENT span that failed is an error");
    assert_eq!(attr(q_rejected, "error.type"), Some(json!("62")), "the ClickHouse error code");

    // D: a 404 has `error.type`, and the status stays unset (below 500).
    let unknown = spans
        .iter()
        .find(|s| s["name"] == "GET /api/dashboards/{uid}" && attr(s, "hune.dashboard.uid") == Some(json!("unknown")))
        .unwrap();
    assert_eq!(attr(unknown, "error.type"), Some(json!("not_found")));
    assert_eq!(status_code(unknown), 0);

    // No span for the ops listener.
    assert!(spans.iter().all(|s| !s["name"].as_str().unwrap().contains("livez")));
    assert!(spans.iter().all(|s| !s["name"].as_str().unwrap().contains("metrics")));
    // No forbidden attribute: only the keys of 14.10, and no SQL, no secret, no row data.
    let text = serde_json::to_string(&spans).unwrap();
    for forbidden in ["SELECT", "secret-password", "toStartOfInterval", "weight"] {
        assert!(!text.contains(forbidden), "{forbidden} in the spans");
    }
    for s in &spans {
        for a in s["attributes"].as_array().unwrap() {
            let k = a["key"].as_str().unwrap();
            assert!(ALLOWED_KEYS.contains(&k), "the key {k} is not in 14.10");
        }
    }
}

#[tokio::test]
async fn a_request_with_an_invalid_traceparent_starts_a_new_trace() {
    let collector = Collector::start().await;
    let h = Harness::start_with(test_timing(), fill, |c| enabled(c, &collector, Duration::from_millis(100))).await;
    for bad in ["00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331", "garbage", ""] {
        let r = call(h.addr, "GET", "/api/session", &[("traceparent", bad)], b"").await;
        assert_eq!(r.status, 200);
    }
    wait_for("three spans", || collector.spans().len() == 3).await;
    for s in collector.spans() {
        assert_eq!(s["name"], "GET /api/session");
        assert_eq!(s["parentSpanId"], Value::Null);
    }
}

#[tokio::test]
async fn a_failing_collector_is_counted_and_logged_one_time_for_each_change() {
    let collector = Collector::start().await;
    collector.status.store(500, Ordering::SeqCst);
    let h = Harness::start_with(test_timing(), fill, |c| enabled(c, &collector, Duration::from_millis(50))).await;
    let failed = |h: &Harness| -> Vec<Value> {
        h.logger
            .memory_lines()
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|v| v["event"] == "telemetry.export_failed")
            .collect()
    };
    // Two exports fail: the line comes one time.
    for _ in 0..2 {
        assert_eq!(get(h.addr, "/api/session").await.status, 200);
        let seen = collector.exports();
        wait_for("an export", || collector.exports() > seen).await;
    }
    let lines = failed(&h);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert_eq!(lines[0]["level"], "warn");
    assert!(lines[0]["error"].is_string());
    assert!(lines[0]["dropped"].as_u64().unwrap() >= 1);
    // The counter of 14.8 holds the dropped spans.
    let mut count = 0;
    for _ in 0..200 {
        count = dropped(&h).await;
        if count >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(count >= 2, "the counter is {count}");
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("# TYPE hune_telemetry_dropped_spans_total counter"), "{text}");
    // The collector comes back: one line `telemetry.export_recovered`.
    collector.status.store(200, Ordering::SeqCst);
    assert_eq!(get(h.addr, "/api/session").await.status, 200);
    wait_for("the recovery", || {
        h.logger.memory_lines().iter().any(|l| l.contains("\"event\":\"telemetry.export_recovered\""))
    })
    .await;
    assert_eq!(failed(&h).len(), 1);
}

/// The value of `hune_telemetry_dropped_spans_total` in the metrics of the ops listener.
async fn dropped(h: &Harness) -> u64 {
    let text = get(h.ops, "/metrics").await.text();
    text.lines()
        .find_map(|l| l.strip_prefix("hune_telemetry_dropped_spans_total "))
        .and_then(|v| v.trim().parse().ok())
        .expect("the counter is in the metrics")
}

#[tokio::test]
async fn the_stop_exports_the_queue_one_last_time() {
    let collector = Collector::start().await;
    // An interval of one hour: only the final export of 14.13 can send the spans.
    let mut h = Harness::start_with(test_timing(), fill, |c| enabled(c, &collector, Duration::from_secs(3600))).await;
    for _ in 0..3 {
        assert_eq!(get(h.addr, "/api/session").await.status, 200);
    }
    assert_eq!(collector.exports(), 0);
    assert_eq!(h.stop().await, 0);
    let spans = collector.spans();
    assert_eq!(named(&spans, "GET /api/session").len(), 3, "{spans:#?}");
    // The span of a request that comes after the stop is not exported, and the stop order is kept.
    let events = h.events();
    let stopping = events.iter().position(|e| e == "server.stopping").unwrap();
    let stopped = events.iter().position(|e| e == "server.stopped").unwrap();
    assert!(stopping < stopped);
}

#[tokio::test]
async fn a_disabled_telemetry_sends_nothing() {
    let collector = Collector::start().await;
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    assert_eq!(get(h.addr, "/api/session").await.status, 200);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(collector.exports(), 0);
    assert!(!h.state().telemetry.is_enabled());
}

#[tokio::test]
async fn the_pdp_call_has_a_span_and_the_pdp_gets_the_traceparent() {
    let pdp = Pdp::start(|_| Ans::yes()).await;
    let collector = Collector::start().await;
    let h = Harness::start_with(test_timing(), fill, |c| {
        authzen(c, &pdp.url(), Duration::ZERO);
        enabled(c, &collector, Duration::from_millis(100));
    })
    .await;
    let t = token("user:alice");
    let r = call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await;
    assert_eq!(r.status, 200, "{}", r.text());
    wait_for("the pdp span", || !named(&collector.spans(), "pdp.evaluate").is_empty()).await;
    let spans = collector.spans();
    let server = one(&spans, "GET /api/dashboards/{uid}");
    let pdp_spans = named(&spans, "pdp.evaluate");
    let evaluate = pdp_spans[0];
    assert_eq!(evaluate["kind"], 3);
    assert_eq!(evaluate["traceId"], server["traceId"]);
    assert_eq!(evaluate["parentSpanId"], server["spanId"]);
    assert_eq!(attr(evaluate, "server.address"), Some(json!("127.0.0.1")));
    assert!(attr(evaluate, "hune.questions").and_then(|v| v.as_i64()).unwrap_or(0) >= 1);
    assert_eq!(status_code(evaluate), 0);
    // The PDP got the `traceparent` of the CLIENT span. No attribute holds the subject.
    let seen = pdp.traceparents.lock().unwrap().clone();
    let expected: Vec<String> = pdp_spans
        .iter()
        .map(|s| format!("00-{}-{}-01", s["traceId"].as_str().unwrap(), s["spanId"].as_str().unwrap()))
        .collect();
    assert!(!seen.is_empty() && seen.iter().all(|t| expected.contains(t)), "{seen:?} {expected:?}");
    let text = serde_json::to_string(&spans).unwrap();
    assert!(!text.contains("alice") && !text.contains(PDP_TOKEN));
}

#[tokio::test]
async fn a_mongodb_query_has_the_span_mongodb_query_with_the_server_code_and_no_text() {
    use super::mongo_source::mongo_block;
    use hune_source::{FakeMongo, FakeMongoReply, MongoError};
    let collector = Collector::start().await;
    let block = mongo_block("events14", "Events");
    let fake = FakeMongo::for_config(&block);
    fake.push_read(FakeMongoReply::docs(&[r#"{"host":"span-value-marker"}"#]));
    fake.push_read(FakeMongoReply::error(MongoError::server(9, "FailedToParse", "bad")));
    crate::server::sources::test_connectors::put("events14", fake.connector());
    let h = Harness::start_with(crate::server::limits::Timing { probe_sources: true, ..test_timing() }, fill, |c| {
        enabled(c, &collector, Duration::from_millis(100));
        c.mongo_sources.push(block);
    })
    .await;
    for _ in 0..200 {
        if get(h.ops, "/readyz").await.json()["sources"]["events14"] == "up" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"secret_field_marker":1}}]}"#;
    let doc = super::pg_source::doc_on("mgspan", "events14", "table", &[text]);
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", &doc))
        .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = super::api::etag_of(&r);
    for want in [200, 422] {
        let body = super::query::panel_body(&e, json!({"variables": {}}));
        let r = call(h.addr, "POST", "/api/dashboards/mgspan/panels/1/query", &[JSON], &body).await;
        assert_eq!(r.status, want, "{}", r.text());
    }
    wait_for("the spans", || named(&collector.spans(), "mongodb.query").len() == 2).await;
    let spans = collector.spans();
    let queries = named(&spans, "mongodb.query");
    let (good, bad) = (queries[0], queries[1]);
    assert_eq!(good["kind"], 3);
    assert_eq!(attr(good, "db.system.name"), Some(json!("mongodb")));
    assert_eq!(attr(good, "server.address"), Some(json!("mongo-host-marker")));
    assert_eq!(attr(good, "server.port"), Some(json!(27017)));
    assert_eq!(attr(good, "hune.source"), Some(json!("events14")));
    assert_eq!(attr(good, "hune.rows"), Some(json!(1)));
    assert_eq!(status_code(good), 0);
    // The server code is the `error.type` of the CLIENT span.
    assert_eq!(status_code(bad), 2);
    assert_eq!(attr(bad, "error.type"), Some(json!("9")));
    let all = serde_json::to_string(&spans).unwrap();
    for forbidden in ["span-value-marker", "secret_field_marker", "$match", "mongo-password-marker"] {
        assert!(!all.contains(forbidden), "{forbidden} in the spans");
    }
    for s in &spans {
        for a in s["attributes"].as_array().unwrap() {
            let k = a["key"].as_str().unwrap();
            assert!(ALLOWED_KEYS.contains(&k), "the key {k} is not in 14.10");
        }
    }
}
