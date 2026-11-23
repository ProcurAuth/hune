//! The annotation route (6.21a, 7.37) through a fake ClickHouse: the answer, the refusals of the request,
//! the limits, the seven reasons of `annotation_invalid`, the access rules and the log. The save checks of
//! 7.18 for an annotation are here too. The tests k15 and k17 to k20 run the same route on a `postgres`
//! source with no server and on a `mongodb` source with the fake client of 8b.16 (decisions ID-K142 and
//! ID-K333). The tests with real servers are in `pg_real.rs` (PS17) and `mongo_real.rs` (MS20).

use super::api::{JSON, NODES, etag_of, fill, key, put_body};
use super::authzen_pdp::*;
use super::fake_ch::{Fake, Step, error, healthy, ok};
use super::harness::*;
use super::pg_source::pg_block;
use hune_source::{FakeMongo, FakeMongoReply as MongoReply};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

/// The body of a fake ClickHouse answer: the two header lines and the rows.
fn frame_body(columns: &[(&str, &str)], rows: &[Value]) -> String {
    let names: Vec<&str> = columns.iter().map(|c| c.0).collect();
    let types: Vec<&str> = columns.iter().map(|c| c.1).collect();
    let mut out = format!("{}\n{}\n", json!(names), json!(types));
    for r in rows {
        out.push_str(&r.to_string());
        out.push('\n');
    }
    out
}

const MARK_COLUMNS: [(&str, &str); 5] = [
    ("time", "DateTime64(3)"),
    ("timeEnd", "Nullable(DateTime64(3))"),
    ("title", "String"),
    ("text", "String"),
    ("tags", "String"),
];

/// The answer with a point and a region, in the order of the time.
fn two_marks() -> String {
    frame_body(
        &MARK_COLUMNS,
        &[
            json!(["1794214800.000", "1794216600.500", "Maintenance", "Disk change", ""]),
            json!(["1794211800.123", null, "", "Deploy v1.2", "prod, deploy"]),
        ],
    )
}

/// The fixture dashboard with the two annotations that the tests use. The annotation 0 is on the source
/// `prod`, with a variable and the range. The annotation 1 is switched off.
fn annotation_doc(uid: &str) -> Value {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["annotations"] = json!([
        {
            "name": "Deploys", "source": "prod", "color": "red", "panels": [1],
            "sql": [
                "SELECT deployed_at AS time, end_at AS timeEnd, title, text, tags",
                "FROM ops.deploys",
                "WHERE $__timeFilter(deployed_at) AND env = '${env}'"
            ]
        },
        {"name": "Off", "source": "prod", "color": "blue", "enabled": false, "sql": ["SELECT 1 AS time"]}
    ]);
    doc
}

fn fill_annotations(root: &Path) {
    fill(root);
    std::fs::write(root.join("transit/nodes.json"), serde_json::to_string_pretty(&annotation_doc("nodes")).unwrap())
        .unwrap();
}

fn body(etag: &str, extra: Value) -> Vec<u8> {
    let mut v = json!({
        "etag": etag,
        "from_ms": 1_794_211_200_000_i64,
        "to_ms": 1_794_232_800_000_i64,
        "variables": {
            "node": {"all": false, "values": ["a1"]},
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

async fn etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

async fn post(h: &Harness, index: &str, body: &[u8]) -> Resp {
    call(h.addr, "POST", &format!("/api/dashboards/nodes/annotations/{index}/query"), &[JSON], body).await
}

async fn start(fake: &Fake) -> Harness {
    start_with_fake(fake, fill_annotations, |_| {}).await
}

fn run_lines(h: &Harness) -> Vec<Value> {
    h.logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["event"] == "query.run")
        .collect()
}

#[tokio::test]
async fn k01_the_answer_has_the_marks_in_the_order_of_the_time() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/annotations/0/query",
        &[JSON, ("X-Request-Id", "req-k1")],
        &body(&e, json!({})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.header("cache-control"), Some("no-store"));
    let j = r.json();
    assert_eq!(j["etag"], e);
    assert_eq!(j["rows"], 2);
    assert_eq!(
        j["annotations"],
        json!([
            [1794211800123_i64, null, "", "Deploy v1.2", ["prod", "deploy"]],
            [1794214800000_i64, 1794216600500_i64, "Maintenance", "Disk change", []]
        ])
    );
    let q = fake.queries();
    assert_eq!(q.len(), 1);
    assert_eq!(j["query_id"].as_str().unwrap(), q[0].query_id());
    // The answer has no SQL, no column list and no interval: the member list is closed (6.21a).
    let mut keys: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["annotations", "etag", "query_id", "rows"]);
    // The SQL of the stored annotation, expanded with the range and the variable. The limit is 1,000.
    let sql = q[0].sql();
    assert!(!sql.contains("$__") && !sql.contains("${"), "{sql}");
    assert!(sql.contains("env = 'prod'") && sql.contains("FROM ops.deploys"), "{sql}");
    assert_eq!(q[0].param("max_result_rows").as_deref(), Some("1000"));
    let comment: Value = serde_json::from_str(&q[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(
        comment,
        json!({"app": "hune", "kind": "annotation", "uid": "nodes", "annotation": 0, "request_id": "req-k1", "subject": "-"})
    );
    let runs = run_lines(&h);
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0]["kind"].as_str(), runs[0]["annotation"].as_u64()), (Some("annotation"), Some(0)));
    assert!(runs[0]["panel"].is_null() && runs[0]["variable"].is_null());
    assert_eq!((runs[0]["rows"].as_u64(), runs[0]["max_result_rows"].as_u64()), (Some(2), Some(1000)));
}

#[tokio::test]
async fn k02_an_index_that_names_no_annotation_gives_404() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    for index in ["2", "7", "8", "-1", "x", "01", "1.0", "+1", "99999999999999999999", "0x1"] {
        let r = post(&h, index, &body(&e, json!({}))).await;
        assert_eq!((r.status, r.code().as_str()), (404, "not_found"), "{index}");
        assert_eq!(r.json()["error"]["message"], "No such annotation.", "{index}");
    }
    let r = call(h.addr, "POST", "/api/dashboards/nothing/annotations/0/query", &[JSON], &body(&e, json!({}))).await;
    assert_eq!(r.status, 404);
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn k03_a_disabled_annotation_gives_400_and_runs_no_query() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let r = post(&h, "1", &body(&etag(&h).await, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    assert_eq!(r.json()["error"]["message"], "This annotation is disabled.");
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn k04_a_body_with_max_data_points_or_sql_or_a_missing_member_gives_400() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    let cases = [
        json!({"max_data_points": 1200}),
        json!({"sql": "SELECT 1"}),
        json!({"sql": ["SELECT 1"], "etag": e}),
        json!({"variables": null}),
        json!({"from_ms": null}),
        json!({"etag": "ABC"}),
        json!({"etag": null}),
        json!({"variables": []}),
    ];
    for change in cases {
        let r = post(&h, "0", &body(&e, change.clone())).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{change} {}", r.text());
    }
    let r = post(&h, "0", &body(&e, json!({"from_ms": 5, "to_ms": 5}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_time_range"));
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn k05_a_stale_etag_gives_409_with_the_current_etag() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    let r = post(&h, "0", &body(&"0".repeat(64), json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (409, "dashboard_changed"));
    assert_eq!(r.json()["error"]["current_etag"], e);
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn k06_a_variable_value_that_breaks_the_property_p_gives_422_and_runs_no_query() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    let evil = |env: &str| {
        json!({"variables": {
            "node": {"all": false, "values": ["a1"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": [env]}}})
    };
    for value in ["evil", "prod' OR '1'='1", "a\u{1}"] {
        let r = post(&h, "0", &body(&e, evil(value))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{value}");
        assert!(r.json()["error"]["variable"].is_string() && r.json()["error"]["reason"].is_string());
    }
    assert!(fake.queries().is_empty());
}

/// A fake that keeps the setting `max_result_rows` as ClickHouse does: a result above it is an error.
fn honest_fake(rows: usize) -> super::fake_ch::Handler {
    healthy(move |rec| {
        let max: usize = rec.param("max_result_rows").and_then(|m| m.parse().ok()).unwrap_or(usize::MAX);
        if rows > max {
            return error(rec, 500, 396, "Code: 396. DB::Exception: Limit for result exceeded");
        }
        let marks: Vec<Value> =
            (0..rows).map(|i| json!([format!("{}", 1_794_211_200 + i), null, "", "", ""])).collect();
        ok(rec, &frame_body(&MARK_COLUMNS, &marks))
    })
}

#[tokio::test]
async fn k07_1000_rows_give_1000_marks_and_1001_rows_give_query_limit() {
    for (rows, status) in [(1000usize, 200u16), (1001, 422)] {
        let fake = Fake::start(honest_fake(rows)).await;
        let h = start(&fake).await;
        let r = post(&h, "0", &body(&etag(&h).await, json!({}))).await;
        assert_eq!(r.status, status, "{rows}: {}", r.text());
        if status == 200 {
            assert_eq!(r.json()["rows"], 1000);
            assert_eq!(r.json()["annotations"].as_array().unwrap().len(), 1000);
        } else {
            assert_eq!(r.code(), "query_limit");
        }
    }
    // A source that ignores the setting: the route keeps the cap of 1,000 rows.
    let marks: Vec<Value> = (0..1001).map(|i| json!([format!("{}", 1_794_211_200 + i), null, "", "", ""])).collect();
    let body_text = frame_body(&MARK_COLUMNS, &marks);
    let fake = Fake::start(healthy(move |rec| ok(rec, &body_text))).await;
    let h = start(&fake).await;
    let r = post(&h, "0", &body(&etag(&h).await, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_limit"), "{}", r.text());
    assert_eq!(r.json()["error"]["limit"], "max_result_rows");
    assert_eq!(r.json()["error"]["value"], 1000);
}

#[tokio::test]
async fn k08_a_ceiling_max_rows_of_10_gives_403_ceiling_exceeded() {
    let fake = Fake::start(honest_fake(20)).await;
    let pdp = Pdp::start(|q| {
        if q.action == "dashboard.read" && q.short().ends_with("dashboard:nodes") {
            Ans::Yes(vec![("max_rows", json!(10))])
        } else {
            Ans::yes()
        }
    })
    .await;
    let h = start_with_fake(&fake, fill_annotations, |c| authzen(c, &pdp.url(), Duration::from_secs(30))).await;
    let t = token("user:alice");
    let hs = [(HEADER, t.as_str()), JSON];
    let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await);
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &hs, &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(
        (err["action"].as_str(), err["ceiling"].as_str(), err["limit"].as_u64()),
        (Some("dashboard.read"), Some("max_rows"), Some(10))
    );
    assert_eq!(fake.queries()[0].param("max_result_rows").as_deref(), Some("10"));
}

#[tokio::test]
async fn k09_each_reason_of_annotation_invalid_gives_422_with_no_cell_value() {
    let secret = "SECRET-CELL-VALUE";
    type Case<'a> = (&'a str, Vec<(&'a str, &'a str)>, Vec<Value>, Option<u64>);
    let cases: Vec<Case<'_>> = vec![
        ("no_time", vec![("text", "String")], vec![json!([secret])], None),
        ("bad_time", vec![("time", "Nullable(DateTime)")], vec![json!(["1794211800"]), json!([null])], Some(2)),
        (
            "bad_end",
            vec![("time", "DateTime"), ("timeEnd", "DateTime")],
            vec![json!(["1794211800", "1794211700"])],
            Some(1),
        ),
        ("bad_text", vec![("time", "DateTime"), ("title", "UInt8")], vec![json!(["1794211800", 5])], None),
        (
            "bad_chars",
            vec![("time", "DateTime"), ("text", "String")],
            vec![json!(["1794211800", "ok"]), json!(["1794211800", format!("<b>{secret}</b>")])],
            Some(2),
        ),
        (
            "too_long",
            vec![("time", "DateTime"), ("title", "String")],
            vec![json!(["1794211800", format!("{secret}{}", "x".repeat(300))])],
            Some(1),
        ),
        (
            "duplicate_column",
            vec![("time", "DateTime"), ("Title", "String"), ("title", "String")],
            vec![json!(["1794211800", secret, secret])],
            None,
        ),
    ];
    for (reason, columns, rows, row) in cases {
        let text = frame_body(&columns, &rows);
        let fake = Fake::start(healthy(move |rec| ok(rec, &text))).await;
        let h = start(&fake).await;
        let r = post(&h, "0", &body(&etag(&h).await, json!({}))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "annotation_invalid"), "{reason}: {}", r.text());
        let err = &r.json()["error"];
        assert_eq!(err["reason"], reason);
        assert_eq!(err["row"], row.map_or(Value::Null, Value::from), "{reason}");
        assert_eq!(err["retryable"], false);
        assert!(!r.text().contains("SECRET") && !r.text().contains("<b>"), "{reason}: {}", r.text());
        let mut keys: Vec<&str> = err.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "message", "reason", "retryable", "row"]);
        // The query ran and the answer holds no mark: the error is the only member.
        assert!(r.json().get("annotations").is_none());
    }
}

#[tokio::test]
async fn k10_a_viewer_with_dashboard_read_only_gets_the_marks_and_one_question_is_asked() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let pdp = Pdp::start(|q| if q.action == "dashboard.read" { Ans::yes() } else { Ans::No(None) }).await;
    let h = start_with_fake(&fake, fill_annotations, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    let t = token("user:viewer");
    let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await);
    pdp.clear();
    let hs = [(HEADER, t.as_str()), JSON];
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &hs, &body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let asked = pdp.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(
        (asked[0].action.as_str(), asked[0].short().as_str()),
        ("dashboard.read", "folder:transit/dashboard:nodes")
    );
    // Without the right: 403 and no query. The page gets no mark.
    pdp.set_rules(|_| Ans::No(None));
    let before = fake.queries().len();
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &hs, &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(r.json()["error"]["action"], "dashboard.read");
    assert_eq!(fake.queries().len(), before);
    // A token is necessary.
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &[JSON], &body(&e, json!({}))).await;
    assert_eq!(r.status, 401);
}

#[tokio::test]
async fn k11_no_log_line_span_or_metric_holds_a_title_a_text_or_a_tag() {
    let text = frame_body(
        &MARK_COLUMNS,
        &[json!(["1794211800.000", null, "TITLE-MARKER", "TEXT-MARKER", "tag-marker-a, tag-marker-b"])],
    );
    let fake = Fake::start(healthy(move |rec| ok(rec, &text))).await;
    let h = start(&fake).await;
    let r = post(&h, "0", &body(&etag(&h).await, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.text().contains("TITLE-MARKER"), "the answer holds the marks");
    for l in h.logger.memory_lines() {
        assert!(!l.contains("MARKER") && !l.contains("marker"), "{l}");
    }
    let m = get(h.ops, "/metrics").await.text();
    assert!(!m.contains("MARKER") && !m.contains("marker"), "{m}");
    // A fault of the text gives a log line with no cell value too.
    let bad = frame_body(&MARK_COLUMNS, &[json!(["1794211800.000", null, "<i>TITLE-MARKER", "", ""])]);
    let fake = Fake::start(healthy(move |rec| ok(rec, &bad))).await;
    let h = start(&fake).await;
    let r = post(&h, "0", &body(&etag(&h).await, json!({}))).await;
    assert_eq!(r.code(), "annotation_invalid");
    for l in h.logger.memory_lines() {
        assert!(!l.contains("MARKER"), "{l}");
    }
}

#[tokio::test]
async fn k12_the_route_is_in_the_table_of_11_21_and_in_the_route_table() {
    const ACCESS: &str = include_str!("../../../../../docs/05-access-security.md");
    const API: &str = include_str!("../../../../../docs/03-api-query-sources.md");
    assert!(
        ACCESS
            .contains("| `POST /api/dashboards/{uid}/annotations/{index}/query` | none | `dashboard.read` on N(uid) |")
    );
    assert!(API.contains("| `POST` | `/api/dashboards/{uid}/annotations/{index}/query` |"));
    assert!(
        crate::server::route::ROUTES
            .iter()
            .any(|r| r.label == "POST /api/dashboards/{uid}/annotations/{index}/query" && !r.write)
    );
}

// ---- The save (7.18, 11.21) --------------------------------------------------------------------

async fn save(h: &Harness, doc: &Value) -> Resp {
    let e = etag(h).await;
    call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&serde_json::to_string(doc).unwrap(), "m"),
    )
    .await
}

fn errors_of(r: &Resp) -> Vec<(String, String)> {
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
async fn k13_a_save_with_an_interval_macro_is_refused_and_a_postgres_source_gets_its_checks() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start_with_fake(&fake, fill_annotations, |c| c.postgres_sources.push(pg_block("metrics", "Metrics"))).await;
    // A valid document is saved.
    let r = save(&h, &annotation_doc("nodes")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    for (sql, pointer) in [
        (vec!["SELECT $__timeInterval(t) AS time", "FROM x"], "/annotations/0/sql/0"),
        (vec!["SELECT 1 AS time", "FROM x GROUP BY $__timeInterval_ms(t)"], "/annotations/0/sql/1"),
        (vec!["SELECT $__interval_s AS time"], "/annotations/0/sql/0"),
        (vec!["SELECT $__interval_ms AS time"], "/annotations/0/sql/0"),
    ] {
        let mut doc = annotation_doc("nodes");
        doc["annotations"][0]["sql"] = json!(sql);
        let r = save(&h, &doc).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{sql:?} {}", r.text());
        assert_eq!(errors_of(&r), vec![(pointer.to_owned(), "not_permitted".to_owned())], "{sql:?}");
    }
    // A `postgres` source (ID-K142, ID-K333): the text gets the checks of its dialect (7.26) and is saved.
    let mut doc = annotation_doc("nodes");
    doc["annotations"][0]["source"] = json!("metrics");
    doc["annotations"][0]["sql"] = json!([
        "SELECT deployed_at AS time, version::text AS text, ARRAY[env, host] AS tags",
        "FROM deploys WHERE $__timeFilter(deployed_at) AND env = '${env}'"
    ]);
    let r = save(&h, &doc).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The checks of the dialect `postgres` apply (7.26): an interval in a group macro (item 5), a refused function
    // (item 2) and a macro of the dialect `clickhouse` (item 3) each give one fault at the line.
    for (sql, code) in [
        ("SELECT $__timeGroup(t, $__interval) AS time FROM x GROUP BY 1", "not_permitted"),
        ("SELECT pg_sleep(1) AS time", "bad_format"),
        ("SELECT $__timeInterval(t) AS time FROM x", "bad_format"),
    ] {
        let mut doc = annotation_doc("nodes");
        doc["annotations"][0]["source"] = json!("metrics");
        doc["annotations"][0]["sql"] = json!([sql]);
        let r = save(&h, &doc).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{sql}: {}", r.text());
        assert_eq!(errors_of(&r), vec![("/annotations/0/sql/0".to_owned(), code.to_owned())], "{sql}");
    }
    // An unknown source: `bad_reference` at the member `source`.
    let mut doc = annotation_doc("nodes");
    doc["annotations"][1]["source"] = json!("nowhere");
    let r = save(&h, &doc).await;
    assert_eq!(errors_of(&r), vec![("/annotations/1/source".to_owned(), "bad_reference".to_owned())]);
    // The format errors of 4.5b come with their pointers.
    let mut doc = annotation_doc("nodes");
    doc["annotations"][0]["panels"] = json!([1, 99]);
    let r = save(&h, &doc).await;
    assert_eq!(errors_of(&r), vec![("/annotations/0/panels/1".to_owned(), "bad_reference".to_owned())]);
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn k14_a_save_with_an_annotation_asks_source_query_for_its_source() {
    let pdp = Pdp::start(|_| Ans::yes()).await;
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start_with_fake(&fake, fill, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    let t = token("user:alice");
    // The panels use `prod` only. The annotation names `conf`: the save asks `source.query` on both.
    let mut doc = annotation_doc("nodes");
    doc["annotations"][0]["source"] = json!("conf");
    doc["annotations"][1]["source"] = json!("conf");
    let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await);
    pdp.clear();
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[(HEADER, t.as_str()), JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&serde_json::to_string(&doc).unwrap(), "m"),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let mut sources: Vec<String> =
        pdp.asked().iter().filter(|q| q.action == "source.query").map(|q| q.short()).collect();
    sources.sort();
    assert_eq!(sources, ["source:conf", "source:prod"]);
    // A subject with no `source.query` on `conf` cannot save the annotation.
    pdp.set_rules(
        |q| if q.action == "source.query" && q.short() == "source:conf" { Ans::No(None) } else { Ans::yes() },
    );
    let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await);
    let mut other = doc.clone();
    other["title"] = json!("Other title");
    pdp.clear();
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[(HEADER, t.as_str()), JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&serde_json::to_string(&other).unwrap(), "m"),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{}", r.text());
    // The body names the use of the save (11.30), whatever question the PDP refused. The refused question is
    // `source.query`: the use is not asked.
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    assert!(pdp.asked().iter().all(|q| q.action == "source.query"), "{:?}", pdp.asked());
}

#[tokio::test]
async fn k15_a_stored_annotation_on_postgres_reaches_its_source_and_an_interval_macro_runs_no_query() {
    let fake = Fake::start(healthy(|rec| ok(rec, &two_marks()))).await;
    let h = start_with_fake(
        &fake,
        |root| {
            fill(root);
            let mut doc = annotation_doc("nodes");
            doc["annotations"] = json!([
                {"name": "Pg", "source": "metrics", "color": "red", "sql": ["SELECT 1 AS time"]},
                {"name": "Macro", "source": "prod", "color": "red", "sql": ["SELECT $__interval_ms AS time"]},
                {"name": "Gone", "source": "gone", "color": "red", "sql": ["SELECT 1 AS time"]}
            ]);
            std::fs::write(root.join("transit/nodes.json"), serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        },
        |c| c.postgres_sources.push(pg_block("metrics", "Metrics")),
    )
    .await;
    let e = etag(&h).await;
    // The `postgres` source `metrics` has no server: the query takes the pipeline of 7.2 up to the source, and the
    // answer is the error of the source (8a.8). The log line has the kind `annotation` and the kind of the source.
    let r = post(&h, "0", &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert_eq!(r.json()["error"]["source"], "metrics");
    let runs = run_lines(&h);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(
        (runs[0]["kind"].as_str(), runs[0]["annotation"].as_u64(), runs[0]["source_kind"].as_str()),
        (Some("annotation"), Some(0), Some("postgres"))
    );
    assert_eq!(runs[0]["max_result_rows"], 1000);
    let r = post(&h, "1", &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    let r = post(&h, "2", &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"), "{}", r.text());
    assert!(fake.queries().is_empty());
}

// ---- The `mongodb` and `postgres` sources (decisions ID-K142 and ID-K333) ------------------------------

/// The annotation 0 of the MongoDB tests: the range, the `custom` variable `env`, the `text` variable `account`
/// (a free value) and the five roles.
const MG_ANNOTATION: &str = r#"{"collection":"node_load","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"},"env":"${env}","node":"${account}"}},{"$project":{"_id":0,"time":"$ts","timeEnd":1,"title":1,"text":1,"tags":1}}]}"#;

/// Starts a server with the `mongodb` source `id` and a fake client for it (8b.16). The dashboard `nodes`
/// has the annotation 0 on this source, with the text `sql`. `edit` changes the configuration.
async fn start_mongo(id: &str, sql: &str, edit: impl FnOnce(&mut crate::config::Config)) -> (Harness, FakeMongo) {
    let block = super::mongo_source::mongo_block(id, "Events");
    let fake = FakeMongo::for_config(&block);
    crate::server::sources::test_connectors::put(id, fake.connector());
    let (source, text) = (id.to_owned(), sql.to_owned());
    let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
    let h = Harness::start_with(
        timing,
        move |root| {
            fill(root);
            let mut doc = annotation_doc("nodes");
            doc["annotations"][0]["source"] = json!(source);
            doc["annotations"][0]["sql"] = json!([text]);
            std::fs::write(root.join("transit/nodes.json"), serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        },
        |c| {
            c.mongo_sources.push(block);
            edit(c);
        },
    )
    .await;
    for _ in 0..200 {
        if get(h.ops, "/readyz").await.json()["sources"][id] == "up" {
            return (h, fake);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the source {id} never got the state up");
}

#[tokio::test]
async fn k17_a_mongodb_annotation_gives_the_marks_with_the_same_route_and_log() {
    let (h, fake) = start_mongo("anmg17", MG_ANNOTATION, |_| {}).await;
    fake.push_read(MongoReply::docs(&[
        r#"{"time":{"$date":{"$numberLong":"1794214800000"}},"timeEnd":{"$date":{"$numberLong":"1794216600500"}},"title":"Maintenance","text":"Disk change","tags":["ops"]}"#,
        r#"{"time":{"$date":{"$numberLong":"1794211800123"}},"text":"Deploy v1.2","tags":["prod","deploy","prod"]}"#,
    ]));
    let e = etag(&h).await;
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/annotations/0/query",
        &[JSON, ("X-Request-Id", "req-k17")],
        &body(&e, json!({})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["rows"], 2);
    assert_eq!(
        j["annotations"],
        json!([
            [1794211800123_i64, null, "", "Deploy v1.2", ["prod", "deploy"]],
            [1794214800000_i64, 1794216600500_i64, "Maintenance", "Disk change", ["ops"]]
        ])
    );
    let mut keys: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["annotations", "etag", "query_id", "rows"]);
    // The command is the typed tree (7.35): the macros are Dates, the variable is a typed leaf, and the first
    // batch stops at the cap of 1,000 rows plus 1. The comment names the annotation (8b.11).
    let sent = fake.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].collection(), "node_load");
    assert_eq!(sent[0].batch_size, 1000);
    let command = format!("{:?}", sent[0].op);
    assert!(!command.contains("$__") && !command.contains("${") && command.contains("prod"), "{command}");
    let comment = format!("{:?}", sent[0].comment);
    assert!(comment.contains("annotation") && comment.contains("req-k17"), "{comment}");
    let runs = run_lines(&h);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0]["kind"].as_str(), runs[0]["annotation"].as_u64(), runs[0]["source_kind"].as_str()),
        (Some("annotation"), Some(0), Some("mongodb"))
    );
    assert!(runs[0]["panel"].is_null() && runs[0]["variable"].is_null());
    assert_eq!((runs[0]["rows"].as_u64(), runs[0]["max_result_rows"].as_u64()), (Some(2), Some(1000)));
    assert_eq!(j["query_id"], runs[0]["query_id"]);
    // An empty result has no column (7.16c): no mark and no fault (7.37, rule 13).
    fake.push_read(MongoReply::docs(&[]));
    let r = post(&h, "0", &body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!((r.json()["rows"].as_u64(), r.json()["annotations"].clone()), (Some(0), json!([])));
}

#[tokio::test]
async fn k18_a_mongodb_annotation_keeps_the_caps_the_property_pm_and_the_text_rules() {
    let (h, fake) = start_mongo("anmg18", MG_ANNOTATION, |_| {}).await;
    let e = etag(&h).await;
    let docs = |n: usize| -> Vec<String> {
        (0..n)
            .map(|i| format!(r#"{{"time":{{"$date":{{"$numberLong":"{}"}}}}}}"#, 1_794_211_200_000_i64 + i as i64))
            .collect()
    };
    // 1,000 rows give 1,000 marks. 1,001 rows give `422 query_limit`: the client stops at the row after the cap.
    for (n, status) in [(1000usize, 200u16), (1001, 422)] {
        let texts = docs(n);
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        fake.push_read(MongoReply::docs(&refs));
        let r = post(&h, "0", &body(&e, json!({}))).await;
        assert_eq!(r.status, status, "{n}: {}", r.text());
        if status == 200 {
            assert_eq!(r.json()["annotations"].as_array().unwrap().len(), 1000);
        } else {
            assert_eq!(
                (r.code().as_str(), r.json()["error"]["limit"].as_str()),
                ("query_limit", Some("max_result_rows"))
            );
        }
    }
    // A free value that changes the structure of the query (7.35, PM2: a leaf that starts with `$`) and a value
    // that is not an option of the closed variable `env` are refused before the source.
    let before = fake.requests().len();
    for (env, account, variable) in [("prod", "$secret", "account"), ("evil", "", "env")] {
        let evil = json!({"variables": {
            "node": {"all": false, "values": ["a1"]},
            "account": {"all": false, "values": [account]},
            "env": {"all": false, "values": [env]}}});
        let r = post(&h, "0", &body(&e, evil)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
        assert_eq!(r.json()["error"]["variable"], variable);
    }
    assert_eq!(fake.requests().len(), before);
    // A hostile text and a time of a wrong type give `annotation_invalid` with no cell value.
    for (doc, reason, row) in [
        (r#"{"time":5,"text":"<script>SECRET-CELL</script>"}"#, "bad_chars", Some(1)),
        (r#"{"time":"SECRET-CELL"}"#, "bad_time", None),
        (r#"{"time":5,"title":true}"#, "bad_text", None),
    ] {
        fake.push_read(MongoReply::docs(&[doc]));
        let r = post(&h, "0", &body(&e, json!({}))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "annotation_invalid"), "{doc}: {}", r.text());
        assert_eq!(r.json()["error"]["reason"], reason, "{doc}");
        assert_eq!(r.json()["error"]["row"], row.map_or(Value::Null, Value::from), "{doc}");
        assert!(!r.text().contains("SECRET"), "{}", r.text());
    }
    for l in h.logger.memory_lines() {
        assert!(!l.contains("SECRET") && !l.contains("\"$match\""), "{l}");
    }
}

#[tokio::test]
async fn k19_a_mongodb_annotation_asks_dashboard_read_and_takes_its_ceilings() {
    let pdp = Pdp::start(|q| {
        if q.action == "dashboard.read" && q.short().ends_with("dashboard:nodes") {
            Ans::Yes(vec![("max_rows", json!(2))])
        } else {
            Ans::yes()
        }
    })
    .await;
    let url = pdp.url();
    let (h, fake) = start_mongo("anmg19", MG_ANNOTATION, |c| authzen(c, &url, Duration::ZERO)).await;
    let t = token("user:alice");
    let hs = [(HEADER, t.as_str()), JSON];
    let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await);
    pdp.clear();
    fake.push_read(MongoReply::docs(&[r#"{"time":1}"#, r#"{"time":2}"#, r#"{"time":3}"#]));
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &hs, &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(
        (err["action"].as_str(), err["ceiling"].as_str(), err["limit"].as_u64()),
        (Some("dashboard.read"), Some("max_rows"), Some(2))
    );
    let asked: Vec<String> = pdp.asked().iter().map(|q| format!("{} {}", q.action, q.short())).collect();
    assert_eq!(asked, ["dashboard.read folder:transit/dashboard:nodes"]);
    assert_eq!(fake.requests().pop().unwrap().batch_size, 3);
    // Without the right: 403 and no query.
    pdp.set_rules(|_| Ans::No(None));
    let before = fake.requests().len();
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &hs, &body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(fake.requests().len(), before);
}

#[tokio::test]
async fn k20_a_save_checks_a_mongodb_annotation_with_the_rules_of_7_36() {
    let (h, fake) = start_mongo("anmg20", MG_ANNOTATION, |_| {}).await;
    let mut doc = annotation_doc("nodes");
    doc["annotations"][0]["source"] = json!("anmg20");
    doc["annotations"][0]["sql"] = json!([MG_ANNOTATION]);
    let r = save(&h, &doc).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // An interval macro (L9), a refused stage (L3), a collection that the block does not list (L8) and a SQL text.
    for (text, code) in [
        (r#"{"collection":"node_load","aggregate":[{"$limit":"$__interval_ms"}]}"#, "not_permitted"),
        (r#"{"collection":"node_load","aggregate":[{"$out":"x"}]}"#, "not_permitted"),
        (r#"{"collection":"secret","aggregate":[{"$limit":1}]}"#, "bad_reference"),
        ("SELECT now() AS time", "bad_format"),
    ] {
        let mut doc = annotation_doc("nodes");
        doc["annotations"][0]["source"] = json!("anmg20");
        doc["annotations"][0]["sql"] = json!([text]);
        let r = save(&h, &doc).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{text}: {}", r.text());
        let errors = errors_of(&r);
        assert_eq!(errors.len(), 1, "{text}: {errors:?}");
        assert!(errors[0].0.starts_with("/annotations/0/sql/") && errors[0].1 == code, "{text}: {errors:?}");
        if text.contains("interval") {
            assert!(r.text().contains("in the query of an annotation"), "{}", r.text());
        }
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn k16_the_route_takes_the_same_pipeline_after_a_client_leaves_and_a_hang_is_cancelled() {
    // A query that never answers: the client leaves, and the server drops the query (8.9). The route has
    // the same behaviour as a panel query: no mark, and the line `query.run` has the outcome `cancelled`.
    let fake = Fake::start(healthy(|_| vec![Step::Hang])).await;
    let h = start(&fake).await;
    let e = etag(&h).await;
    let addr = h.addr;
    let request = request("POST", "/api/dashboards/nodes/annotations/0/query", &[JSON], &body(&e, json!({})));
    let mut c = Client::connect(addr).await;
    c.send(&request).await;
    for _ in 0..200 {
        if !fake.queries().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(fake.queries().len(), 1);
    drop(c);
    for _ in 0..200 {
        if fake.closed() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(fake.closed(), 1, "the connection to ClickHouse closed");
    for _ in 0..200 {
        if !run_lines(&h).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let runs = run_lines(&h);
    assert_eq!(runs[0]["outcome"], "cancelled");
    assert_eq!((runs[0]["kind"].as_str(), runs[0]["annotation"].as_u64()), (Some("annotation"), Some(0)));
}
