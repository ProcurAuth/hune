// SPDX-License-Identifier: MIT OR Apache-2.0
//! The work before and after the decision is bounded (6.27, 11.20, 12.16). Findings NEW-PLACE, N2, D3 and D4 of the
//! security review of 2026-11-16, and the rule that the validation never runs on an async worker,
//! a place stays taken until its work ends, the decision comes before the validation, and a subject has its own caps.

use super::api::{JSON, NODES, ZERO_ETAG, doc_text, etag_of, fill, key, post_body, put_body};
use super::authzen_pdp::*;
use super::harness::*;
use crate::server::handlers::prepass::tests_hook;
use crate::server::limits::{MAX_INFLIGHT_DOC_BODIES, MAX_INFLIGHT_PER_SUBJECT, MAX_INFLIGHT_VALIDATIONS};
use hune_core::limits::{MAX_ERRORS, MAX_WARNINGS};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A valid document whose validation is slow: `vars` query variables, each with one SQL line of about `sql_bytes`
/// bytes of `$a` in a string literal, and the text variable `a`. It is the generator `vars_valid` of the review,
/// with the source `prod` of the test configuration. In a debug build, 4 variables take about 0.7 s and 15
/// variables about 2.8 s.
pub(super) fn heavy_doc(uid: &str, vars: usize) -> String {
    let sql = format!("SELECT '{}'", "$a".repeat(131_000));
    let mut variables: Vec<Value> = (0..vars)
        .map(|i| {
            json!({"kind": "query", "name": format!("v{i}"), "label": null, "hide": "none", "source": "prod",
                   "sql": [sql], "multi": false, "include_all": false, "all_value": null, "refresh": "load",
                   "sort": "none", "regex": null, "default": []})
        })
        .collect();
    variables.push(text_variable("a"));
    document(uid, variables, vec![panel(1, "prod", "SELECT 1", "")], &[])
}

fn text_variable(name: &str) -> Value {
    json!({"kind": "text", "name": name, "label": null, "hide": "none", "value_type": "text", "default": ""})
}

fn panel(id: u32, source: &str, sql: &str, description: &str) -> Value {
    json!({"id": id, "type": "table", "title": "t", "description": description, "transparent": false,
        "grid": {"x": 0, "y": id - 1, "w": 24, "h": 1},
        "data": {"query": {"source": source, "format": "table", "sql": [sql], "min_interval": null,
                           "max_data_points": null}},
        "transforms": [], "fields": {"defaults": {}, "overrides": []},
        "options": {"show_header": true, "sort": []}, "links": []})
}

fn document(uid: &str, variables: Vec<Value>, panels: Vec<Value>, annotations: &[Value]) -> String {
    let doc = json!({"format_version": 3, "uid": uid, "title": "Bounded", "description": "", "tags": [],
        "time": {"from": "now-1h", "to": "now", "timezone": "utc", "refresh": null}, "cursor_sync": "off",
        "links": [], "variables": variables, "annotations": annotations,
        "sections": [{"title": null, "collapsed": false, "panels": panels}]});
    serde_json::to_string(&doc).unwrap()
}

/// The minimum of 7 runs after 2 warm-ups (the policy of the timing tests).
pub(super) fn min_of_7(runs: &[Duration]) -> Duration {
    runs[2..].iter().copied().min().unwrap_or(Duration::MAX)
}

/// Waits until `cond` is true, for 20 s at most.
async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..2000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("never true: {what}");
}

/// A request with a body whose head and body are sent, on a connection that the test keeps or drops.
async fn send_only(h: &Harness, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Client {
    let mut c = Client::connect(h.addr).await;
    c.send(&request(method, path, headers, body)).await;
    c
}

/// A `GET` with a `std::net` socket: the status and the time to the end of the answer.
fn blocking_get(addr: std::net::SocketAddr, path: &str) -> (u16, Duration) {
    use std::io::{Read, Write};
    let t = Instant::now();
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
    let mut answer = Vec::new();
    s.read_to_end(&mut answer).unwrap();
    let took = t.elapsed();
    let text = String::from_utf8_lossy(&answer);
    let status = text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, took)
}

// ---- N2 and D3: the validation of a version never runs on an async worker -------------------------------------

/// `/api/session` stays fast while 2 times the workers restore a heavy version and read it with
/// `GET history/{n}`. Before the fix, each of these requests validated the version on an async worker: the review
/// measured 35.1 s for `/api/session` (restore) and 745 ms (history).
#[test]
fn n2_d3_heavy_restores_and_version_reads_leave_the_async_workers_free() {
    let workers = 2;
    // The runtime of 3.6 with few workers, so that the test needs few attackers.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .max_blocking_threads(16)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let h = Harness::start_with(test_timing(), fill, |_| {}).await;
        // Version 1: the heavy document. Version 2: a small document, so that the store read of the current
        // document stays small.
        let heavy = heavy_doc("heavy", 4);
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards",
            &[JSON, ("Idempotency-Key", &key())],
            &post_body("transit", &heavy),
        )
        .await;
        assert_eq!(r.status, 201, "{}", r.text());
        let e1 = etag_of(&r);
        let ifm = format!("\"{e1}\"");
        let r = call(
            h.addr,
            "PUT",
            "/api/dashboards/heavy",
            &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
            &put_body(&doc_text("heavy", "Small"), "m"),
        )
        .await;
        assert_eq!(r.status, 200, "{}", r.text());
        let stop = Arc::new(AtomicBool::new(false));
        let answers = Arc::new(AtomicU64::new(0));
        let mut attackers = Vec::new();
        for i in 0..4 * workers {
            let (stop, answers, addr) = (stop.clone(), answers.clone(), h.addr);
            attackers.push(tokio::spawn(async move {
                let mut statuses = Vec::new();
                while !stop.load(Ordering::SeqCst) {
                    let r = if i % 2 == 0 {
                        // A restore with a wrong If-Match: the validation runs, then the store gives 409.
                        call(
                            addr,
                            "POST",
                            "/api/dashboards/heavy/restore",
                            &[JSON, ("If-Match", ZERO_ETAG), ("Idempotency-Key", &key())],
                            br#"{"version":1}"#,
                        )
                        .await
                    } else {
                        get(addr, "/api/dashboards/heavy/history/1").await
                    };
                    statuses.push(r.status);
                    answers.fetch_add(1, Ordering::SeqCst);
                }
                statuses
            }));
        }
        // The probes use a blocking client and blocking sleeps on the thread of the test, so that their time does
        // not depend on the workers of the runtime under test.
        std::thread::sleep(Duration::from_millis(300));
        let before = answers.load(Ordering::SeqCst);
        let mut runs = Vec::new();
        for _ in 0..9 {
            let (status, took) = blocking_get(h.addr, "/api/session");
            assert_eq!(status, 200);
            runs.push(took);
            std::thread::sleep(Duration::from_millis(100));
        }
        // The attack ran during the probes: it gave answers, and the validation permits are in use.
        let busy = h.state().validations.available_permits() < MAX_INFLIGHT_VALIDATIONS;
        stop.store(true, Ordering::SeqCst);
        let mut statuses = Vec::new();
        for a in attackers {
            statuses.extend(a.await.unwrap());
        }
        let worst = runs[2..].iter().max().copied().unwrap_or_default();
        eprintln!("session min {:?} max {worst:?}; attack answers {}", min_of_7(&runs), statuses.len());
        assert!(busy || answers.load(Ordering::SeqCst) > before, "the attack did not run during the probes");
        assert!(min_of_7(&runs) < Duration::from_millis(100), "{runs:?}");
        // Each attack answer is a normal answer: 409 for a restore, 200 for a read, 503 when the queue is full.
        assert!(statuses.iter().all(|s| [200, 409, 503].contains(s)), "{statuses:?}");
        assert!(statuses.contains(&409) && statuses.contains(&200), "{statuses:?}");
    });
}

// ---- NEW-PLACE: a place stays taken until the work on the body ends -------------------------------------------

/// 8 heavy `validate_only` requests whose clients leave: their work still waits or runs, and it keeps the 8 places
/// of the document bodies. A ninth request gets `503 server_busy`. Before the fix, the places were free at once and
/// the review measured 49.8 s for a small `validate_only` after 20 such requests.
#[tokio::test]
async fn new_place_a_ninth_document_while_eight_dropped_ones_still_run_gets_503() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let heavy = post_body("transit", &heavy_doc("heavy", 8));
    let mut clients = Vec::new();
    for _ in 0..MAX_INFLIGHT_DOC_BODIES {
        clients.push(send_only(&h, "POST", "/api/dashboards?validate_only=true", &[JSON], &heavy).await);
    }
    let state = h.state();
    // The server holds the 8 bodies. After 500 ms it runs 4 validations, and the 4 others wait for a permit.
    wait_for("8 places", || state.doc_bodies.available_permits() == 0).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(clients);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let small = post_body("transit", &doc_text("small", "Small"));
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &small).await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_busy"), "{}", r.text());
    assert_eq!(r.header("retry-after"), Some("1"));
    // The work of the 8 dropped requests ends, then the places are free.
    wait_for("the 8 places are free", || state.doc_bodies.available_permits() == MAX_INFLIGHT_DOC_BODIES).await;
    assert!(state.validations_run.load(Ordering::SeqCst) >= MAX_INFLIGHT_VALIDATIONS as u64);
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &small).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

// ---- D4: the names of 6.14 are made at the load of the catalog ------------------------------------------------

/// The first `GET` of a dashboard gives the names that the catalog made off the async workers. Before the fix, the
/// first `GET` made them on an async worker inside a `OnceLock` (608 ms in the review, in a release build).
#[tokio::test]
async fn d4_the_first_read_of_a_dashboard_does_not_make_the_names() {
    let sql = format!("SELECT {}", "$a,".repeat(65_000));
    let panels: Vec<Value> = (1..=4).map(|i| panel(i, "prod", &sql, "")).collect();
    let doc = document("names", vec![text_variable("a")], panels, &[]);
    let h = Harness::start_with(
        test_timing(),
        |root| {
            fill(root);
            std::fs::write(root.join("transit/names.json"), &doc).unwrap();
        },
        |_| {},
    )
    .await;
    // The cost of the names on this document, for the report.
    let parsed = hune_core::document::validate(doc.as_bytes(), None).unwrap();
    let t = Instant::now();
    let names = parsed.names_json().len();
    let cost = t.elapsed();
    let mut runs = Vec::new();
    for _ in 0..9 {
        let t = Instant::now();
        let r = get(h.addr, "/api/dashboards/names").await;
        runs.push(t.elapsed());
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(serde_json::to_string(&r.json()["names"]).unwrap().len(), names);
    }
    eprintln!("names {names} bytes, cost {cost:?}; first GET {:?}; min of 7 {:?}", runs[0], min_of_7(&runs));
    assert!(cost > Duration::from_millis(100), "the document is too light for the test: {cost:?}");
    assert!(runs[0] < cost / 2 + min_of_7(&runs), "first {:?}, cost {cost:?}", runs[0]);
}

// ---- The caps of the lists of 6.13 and 4.36 -------------------------------------------------------------------

/// A document with 214 errors of the save checks gives 200 items, and the last one is `too_many_errors` (4.36, item
/// 7). A document with 2,048 warnings gives 200 items, and the last one says that the list stops. Before the fix, the
/// errors were cut at 200 with no last item, and the warnings had no cap.
#[tokio::test]
async fn the_lists_of_errors_and_warnings_stop_at_max_errors_with_a_last_item() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let variables: Vec<Value> = (0..64)
        .map(|i| {
            json!({"kind": "query", "name": format!("v{i}"), "label": null, "hide": "none", "source": "nope",
                   "sql": ["SELECT 1"], "multi": false, "include_all": false, "all_value": null, "refresh": "load",
                   "sort": "none", "regex": null, "default": []})
        })
        .collect();
    let panels: Vec<Value> = (1..=150).map(|i| panel(i, "nope", "SELECT 1", "")).collect();
    let doc = document("errors", variables, panels, &[]);
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &post_body("transit", &doc)).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    let errors = r.json()["error"]["errors"].as_array().unwrap().clone();
    assert_eq!(errors.len(), MAX_ERRORS);
    let marker = json!({"pointer": "", "code": "too_many_errors", "message": "the list stops here; it has 200 errors at most",
                        "line": null, "column": null});
    assert_eq!(errors.last().unwrap(), &marker);
    assert!(errors[..MAX_ERRORS - 1].iter().all(|e| e["code"] == "bad_reference"));
    // Warning 6 for each `$b` of a description: 2,048 warnings.
    let description = "$b".repeat(2048);
    let doc = document("warnings", vec![], vec![panel(1, "prod", "SELECT 1", &description)], &[]);
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &post_body("transit", &doc)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let warnings = r.json()["warnings"].as_array().unwrap().clone();
    assert_eq!(warnings.len(), MAX_WARNINGS);
    let marker = json!({"pointer": "", "message": "the list stops here; it has 200 warnings at most"});
    assert_eq!(warnings.last().unwrap(), &marker);
    assert!(r.body.len() < 64 * 1024 + doc.len() * 2, "{}", r.body.len());
}

// ---- The decision comes before the validation (mode `authzen`) ---------------------------------------

/// The grants of a subject: `(action, node below the service)`. A grant covers each node below it.
fn grants(list: Vec<(&'static str, &'static str)>) -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    move |q| {
        let qp = q.short();
        let ok = list
            .iter()
            .any(|(action, g)| *action == q.action && (g.is_empty() || qp == *g || qp.starts_with(&format!("{g}/"))));
        if ok { Ans::yes() } else { Ans::No(None) }
    }
}

fn allow_all() -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])
}

async fn start_authzen(pdp: &Pdp) -> Harness {
    Harness::start_with(test_timing(), fill, |c| authzen(c, &pdp.url(), Duration::ZERO)).await
}

async fn areq(h: &Harness, method: &str, path: &str, sub: &str, extra: &[(&str, &str)], body: &[u8]) -> Resp {
    let t = token(sub);
    let mut hs: Vec<(&str, &str)> = vec![(HEADER, t.as_str())];
    hs.extend_from_slice(extra);
    call(h.addr, method, path, &hs, body).await
}

fn shorts(pdp: &Pdp) -> Vec<(String, String)> {
    pdp.asked().iter().map(|q| (q.action.clone(), q.short())).collect()
}

fn pair(a: &str, r: &str) -> (String, String) {
    (a.to_owned(), r.to_owned())
}

/// The count of the store reads of one operation, from `/metrics`.
async fn store_ops(h: &Harness, op: &str) -> u64 {
    let text = get(h.ops, "/metrics").await.text();
    let label = format!("op=\"{op}\"");
    text.lines()
        .filter(|l| l.starts_with("hune_store_operations_total{") && l.contains(&label))
        .filter_map(|l| l.rsplit(' ').next().and_then(|v| v.parse::<u64>().ok()))
        .sum()
}

/// A preview body for the dashboard `nodes`, with one panel.
fn preview_body(sql: &str) -> Vec<u8> {
    let doc: Value = serde_json::from_str(NODES).unwrap();
    let mut p = doc["sections"][0]["panels"][0].clone();
    p["data"]["query"]["sql"] = json!([sql]);
    serde_json::to_vec(&json!({
        "uid": "nodes", "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200, "dashboard_variables": [], "variables": {}, "panel": p
    }))
    .unwrap()
}

/// A subject with no grant sends an invalid document and a document of 4 MiB to the create, the save, the two
/// `validate_only` routes, the preview and the restore. Each answer is `403`, the validator never runs, and the
/// restore reads no version. Before this rule, the document was validated first (`422`, or seconds of work).
#[tokio::test]
async fn k312_a_subject_with_no_grant_gets_403_and_the_validator_never_runs() {
    let pdp = Pdp::start(grants(vec![("dashboard.read", "")])).await;
    let h = start_authzen(&pdp).await;
    let state = h.state();
    let invalid = doc_text("nodes", "X").replace("\"title\": \"X\"", "\"title\": 5");
    let big = heavy_doc("nodes", 15);
    assert!(big.len() > 3_900_000);
    let ifm = format!("\"{}\"", etag_of(&areq(&h, "GET", "/api/dashboards/nodes", "user:eve", &[], b"").await));
    let reads_before = store_ops(&h, "version").await;
    for doc in [&invalid, &big] {
        let creates = [
            ("POST", "/api/dashboards", post_body("transit", doc)),
            ("POST", "/api/dashboards?validate_only=true", post_body("transit", doc)),
            ("PUT", "/api/dashboards/nodes", put_body(doc, "m")),
            ("PUT", "/api/dashboards/nodes?validate_only=true", put_body(doc, "m")),
        ];
        for (method, path, body) in creates {
            let id = key();
            let r =
                areq(&h, method, path, "user:eve", &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &id)], &body).await;
            assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{method} {path}: {}", r.text());
        }
    }
    let r =
        areq(&h, "POST", "/api/sources/prod/preview", "user:eve", &[JSON], &preview_body(&"x".repeat(300_000))).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{}", r.text());
    let r = areq(
        &h,
        "POST",
        "/api/dashboards/nodes/restore",
        "user:eve",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{}", r.text());
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    assert_eq!(state.validations_run.load(Ordering::SeqCst), 0, "the validator ran");
    assert_eq!(store_ops(&h, "version").await, reads_before, "the restore read a version");
}

/// A subject with the grants sends an invalid document: the decision comes first, then the validation gives `422`.
/// A document that the pre-pass cannot read asks the use alone. A restore asks the use alone, reads the version,
/// validates it, then asks the others and the use: three PDP requests at `cache_ttl = 0` (11.13).
#[tokio::test]
async fn k312_a_granted_subject_gets_the_decision_then_422_and_a_restore_asks_three_times() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start_authzen(&pdp).await;
    let state = h.state();
    let invalid = doc_text("fresh", "X").replace("\"title\": \"X\"", "\"title\": 5");
    pdp.clear();
    let r = areq(
        &h,
        "POST",
        "/api/dashboards",
        "user:alice",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &invalid),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("source.query", "source:prod"), pair("dashboard.write", "folder:ops")]);
    assert_eq!(state.validations_run.load(Ordering::SeqCst), 1);
    // The pre-pass cannot read `variables: {}`: the use alone, then the 422 of the validation.
    let unreadable = doc_text("fresh", "X").replacen("\"variables\": [", "\"variables\": {}, \"x\": [", 1);
    pdp.clear();
    let r = areq(
        &h,
        "POST",
        "/api/dashboards",
        "user:alice",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &unreadable),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:ops")]);
    // A restore of the external version 1 of `nodes`.
    let e0 = etag_of(&areq(&h, "GET", "/api/dashboards/nodes", "user:alice", &[], b"").await);
    let r = areq(
        &h,
        "PUT",
        "/api/dashboards/nodes",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        &put_body(&doc_text("nodes", "Two"), "m"),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let e1 = etag_of(&r);
    pdp.clear();
    let r = areq(
        &h,
        "POST",
        "/api/dashboards/nodes/restore",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e1}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let n = "folder:transit/dashboard:nodes";
    assert_eq!(
        shorts(&pdp),
        vec![pair("dashboard.write", n), pair("source.query", "source:prod"), pair("dashboard.write", n)],
        "the use alone, then the others, then the use"
    );
    assert_eq!(pdp.requests(), 3);
}

/// The check after the validation (11.21): a pre-pass that misses a source gives `500`, the log line
/// `access.prepass_mismatch` and no write. A document that the pre-pass reports as unreadable and that passes the
/// validation gives the same answer.
#[tokio::test]
async fn k312_a_pre_pass_that_misses_a_source_gives_500_and_no_write() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start_authzen(&pdp).await;
    let two = |uid: &str, title: &str| doc_text(uid, title).replacen("\"source\": \"prod\"", "\"source\": \"conf\"", 1);
    // A create: the pre-pass misses `conf`.
    *tests_hook::MISS.lock().unwrap() = Some(("pmiss".to_owned(), "conf".to_owned()));
    pdp.clear();
    let r = areq(
        &h,
        "POST",
        "/api/dashboards",
        "user:alice",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &two("pmiss", "P")),
    )
    .await;
    *tests_hook::MISS.lock().unwrap() = None;
    assert_eq!((r.status, r.code().as_str()), (500, "internal_error"), "{}", r.text());
    assert!(shorts(&pdp).iter().all(|(_, n)| n != "source:conf"), "{:?}", shorts(&pdp));
    assert_eq!(areq(&h, "GET", "/api/dashboards/pmiss", "user:alice", &[], b"").await.status, 404);
    let lines: Vec<Value> = h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    let line = lines.iter().find(|l| l["event"] == "access.prepass_mismatch").expect("the log line");
    assert_eq!(line["level"], "error");
    assert_eq!(line["missing"], json!(["conf"]));
    // A save: create the dashboard, then the pre-pass misses `conf`.
    let r = areq(
        &h,
        "POST",
        "/api/dashboards",
        "user:alice",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &doc_text("pmiss2", "P")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    *tests_hook::MISS.lock().unwrap() = Some(("pmiss2".to_owned(), "conf".to_owned()));
    let r = areq(
        &h,
        "PUT",
        "/api/dashboards/pmiss2",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&two("pmiss2", "Q"), "m"),
    )
    .await;
    *tests_hook::MISS.lock().unwrap() = None;
    assert_eq!((r.status, r.code().as_str()), (500, "internal_error"), "{}", r.text());
    // An unreadable document that passes the validation.
    *tests_hook::UNREADABLE.lock().unwrap() = Some("pmiss2".to_owned());
    pdp.clear();
    let r = areq(
        &h,
        "PUT",
        "/api/dashboards/pmiss2",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&doc_text("pmiss2", "R"), "m"),
    )
    .await;
    *tests_hook::UNREADABLE.lock().unwrap() = None;
    assert_eq!((r.status, r.code().as_str()), (500, "internal_error"), "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:ops/dashboard:pmiss2")], "the use alone");
    // No write: the etag is the etag of the create.
    let now = areq(&h, "GET", "/api/dashboards/pmiss2", "user:alice", &[], b"").await;
    assert_eq!(etag_of(&now), e);
}

/// A subject holds at most 2 document bodies and 2 validations in flight in the mode `authzen` (12.16). A third gets
/// `503 server_busy`. Another subject still gets a place.
#[tokio::test]
async fn k312_a_subject_holds_two_document_bodies_and_two_validations_at_most() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start_authzen(&pdp).await;
    let state = h.state();
    let alice = token("user:alice");
    // Two bodies that do not end: they hold the two places of alice.
    let head = |len: usize| {
        format!(
            "POST /api/dashboards?validate_only=true HTTP/1.1\r\nHost: localhost\r\n{HEADER}: {alice}\r\n\
             Content-Type: application/json\r\nContent-Length: {len}\r\n\r\n{{"
        )
    };
    let mut held = Vec::new();
    for _ in 0..MAX_INFLIGHT_PER_SUBJECT {
        let mut c = Client::connect(h.addr).await;
        c.send(head(1000).as_bytes()).await;
        held.push(c);
    }
    wait_for("alice holds 2 places", || {
        state.doc_bodies.available_permits() == MAX_INFLIGHT_DOC_BODIES - MAX_INFLIGHT_PER_SUBJECT
    })
    .await;
    assert_eq!(state.subject_bodies.held("user:alice"), MAX_INFLIGHT_PER_SUBJECT);
    let small = post_body("transit", &doc_text("fresh", "F"));
    let r = areq(&h, "POST", "/api/dashboards?validate_only=true", "user:alice", &[JSON], &small).await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_busy"), "{}", r.text());
    let r = areq(&h, "POST", "/api/dashboards?validate_only=true", "user:bob", &[JSON], &small).await;
    assert_eq!(r.status, 200, "{}", r.text());
    drop(held);
    wait_for("alice holds no place", || state.subject_bodies.held("user:alice") == 0).await;
    // Two heavy validations of alice: a third validation of alice gets 503, and bob gets his answer.
    let heavy = post_body("transit", &heavy_doc("heavy", 15));
    let mut running = Vec::new();
    for _ in 0..MAX_INFLIGHT_PER_SUBJECT {
        let t = token("user:alice");
        running.push(
            send_only(&h, "POST", "/api/dashboards?validate_only=true", &[(HEADER, t.as_str()), JSON], &heavy).await,
        );
    }
    wait_for("alice runs 2 validations", || state.subject_validations.held("user:alice") == MAX_INFLIGHT_PER_SUBJECT)
        .await;
    let r = areq(&h, "GET", "/api/dashboards/nodes/history/1", "user:alice", &[], b"").await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_busy"), "{}", r.text());
    let r = areq(&h, "POST", "/api/dashboards?validate_only=true", "user:bob", &[JSON], &small).await;
    assert_eq!(r.status, 200, "{}", r.text());
    for mut c in running {
        assert_eq!(c.read(false).await.status, 200);
    }
}
