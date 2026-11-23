// SPDX-License-Identifier: MIT OR Apache-2.0
//! The repeat of a panel (4.13a, 9.8c) at the server: no code runs for it. These tests check that a copy is
//! the ordinary panel query of 6.20 with one value for the repeat variable. The answers are those of 7.7, the
//! properties P, PG and PM, and the admission of 8.8. The route table (6.9) has no new route.

use super::api::{JSON, NODES, etag_of, fill, key, post_body, put};
use super::fake_ch::{FRAME_BODY, Fake, Step, healthy, ok};
use super::harness::*;
use super::mongo_source::mongo_block;
use super::pg_source::{doc_on, pg_block};
use super::query::{panel_body, queries};
use crate::server::sources::test_connectors;
use hune_source::FakeMongo;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The fixture `nodes` with a `repeat` on the panel 2 (the table of the section "Details"): the
/// variable `node` (`query`, `multi`), the direction `h` and `max_per_row` 3. `edit` changes the document.
fn repeat_doc(uid: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("Repeats");
    // The panel 3 reuses the panel 2, and a panel with a repeat has no reuse (4.13a, rule 8): remove it.
    doc["sections"][0]["panels"].as_array_mut().unwrap().truncate(1);
    doc["sections"][1]["panels"][0]["repeat"] = json!({"variable": "node", "direction": "h", "max_per_row": 3});
    edit(&mut doc);
    serde_json::to_string(&doc).unwrap()
}

/// The panel 2 of the document of the tests.
fn panel_of(doc: &mut Value) -> &mut Value {
    &mut doc["sections"][1]["panels"][0]
}

async fn create(h: &Harness, doc: &str) -> Resp {
    call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", doc)).await
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

/// The request of one copy: the repeat variable has `all` equal to `false` and one value.
fn copy_body(etag: &str, variable: &str, value: &str) -> Vec<u8> {
    panel_body(
        etag,
        json!({"variables": {
            "node": {"all": false, "values": ["a1", "b2"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
            variable: {"all": false, "values": [value]},
        }}),
    )
}

// ------------------------------------------------------------------ the save

#[tokio::test]
async fn a_save_with_a_repeat_gives_the_canonical_bytes_with_the_member_last() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    // The members of the input are not in the canonical order, and the section repeats too.
    let doc = repeat_doc("repeats1", |d| {
        panel_of(d)["repeat"] = json!({"max_per_row": 3, "direction": "h", "variable": "node"});
        d["sections"][0]["panels"][0]["transparent"] = json!(false);
        let panel = panel_of(d).clone();
        d["sections"][1]["panels"] = json!([]);
        d["sections"][1]["repeat"] = json!({"variable": "node"});
        d["sections"][1]["panels"].as_array_mut().unwrap().push({
            let mut p = panel;
            p.as_object_mut().unwrap().remove("repeat");
            p
        });
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    // The stored bytes are the canonical bytes: `repeat` is the last member of the section.
    let stored = std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/repeats1.json"))
        .expect("the file of the document");
    let canonical = hune_core::document::validate(doc.as_bytes(), None).unwrap();
    assert_eq!(stored.as_bytes(), canonical.bytes());
    assert_eq!(etag_of(&r), canonical.etag().to_hex());
    assert!(stored.contains("      ],\n      \"repeat\": {\n        \"variable\": \"node\"\n      }\n"), "{stored}");
    // The answer of the read gives the member in the document.
    let read = get(h.addr, "/api/dashboards/repeats1").await;
    assert_eq!(read.status, 200);
    assert_eq!(read.json()["document"]["sections"][1]["repeat"], json!({"variable": "node"}));
    // A panel with a repeat: the member is last, and `max_per_row` is last in it.
    let doc = repeat_doc("repeats2", |_| {});
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let stored =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/repeats2.json")).unwrap();
    assert!(
        stored.contains(
            "          \"links\": [],\n          \"repeat\": {\n            \"variable\": \"node\",\n            \"direction\": \"h\",\n            \"max_per_row\": 3\n          }\n        }"
        ),
        "{stored}"
    );
    // A document with no repeat keeps its bytes.
    let plain =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/nodes.json")).unwrap();
    assert_eq!(plain, NODES);
}

#[tokio::test]
async fn three_invalid_documents_give_422_with_the_codes_of_4_34() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    type Case = (&'static str, Box<dyn FnOnce(&mut Value)>, &'static str, &'static str);
    let cases: [Case; 3] = [
        (
            "unknown variable",
            Box::new(|d| panel_of(d)["repeat"]["variable"] = json!("nothing")),
            "/sections/1/panels/0/repeat/variable",
            "bad_reference",
        ),
        (
            "a variable with multi false",
            Box::new(|d| panel_of(d)["repeat"]["variable"] = json!("env")),
            "/sections/1/panels/0/repeat/variable",
            "not_permitted",
        ),
        (
            "max_per_row with v",
            Box::new(|d| panel_of(d)["repeat"]["direction"] = json!("v")),
            "/sections/1/panels/0/repeat/max_per_row",
            "not_permitted",
        ),
    ];
    for (name, edit, pointer, code) in cases {
        let doc = repeat_doc("repeats-bad", edit);
        let r = create(&h, &doc).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(pointers(&r), vec![(pointer.to_owned(), code.to_owned())], "{name}");
    }
    // A save of an existing document gives the same refusal.
    let e = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    let bad = repeat_doc("nodes", |d| panel_of(d)["repeat"]["variable"] = json!("nothing"));
    let r = put(&h, "nodes", &e, &bad, &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
}

// ------------------------------------------------------------- the query of a copy

#[tokio::test]
async fn a_copy_query_has_the_repeat_variable_fixed_and_the_expanded_sql_holds_that_one_value() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |c| c.server.query_inspect = true).await;
    let r = create(&h, &repeat_doc("repeats3", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    // The copies of the values `a1`, `b2` and `c3`: the same panel, one value each.
    for (i, value) in ["a1", "b2", "c3"].into_iter().enumerate() {
        let r = call(h.addr, "POST", "/api/dashboards/repeats3/panels/2/query", &[JSON], &copy_body(&e, "node", value))
            .await;
        assert_eq!(r.status, 200, "{value}: {}", r.text());
        let sent = fake.queries()[i].sql();
        assert!(sent.contains(&format!("node IN ('{value}')")), "{sent}");
        for other in ["a1", "b2", "c3"].into_iter().filter(|o| *o != value) {
            assert!(!sent.contains(other), "{other} in {sent}");
        }
        assert_eq!(r.json()["sql"], sent.as_str());
        assert_eq!(r.json()["etag"], e);
    }
    assert_eq!(fake.queries().len(), 3, "one query for each copy");
    // The route is the route of the stored panel: a key of a copy is not a panel id.
    let r =
        call(h.addr, "POST", "/api/dashboards/repeats3/panels/2-r2/query", &[JSON], &copy_body(&e, "node", "b2")).await;
    assert_eq!(r.status, 404, "{}", r.text());
    assert_eq!(fake.queries().len(), 3);
}

#[tokio::test]
async fn a_value_that_is_not_an_option_of_a_custom_variable_gives_variable_value_refused() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    // The repeat variable `host` is a `custom` variable with `multi`.
    let doc = repeat_doc("repeats4", |d| {
        d["variables"].as_array_mut().unwrap().push(json!({
            "kind": "custom", "name": "host", "label": null, "hide": "none",
            "options": [{"text": "Alpha", "value": "a"}, {"text": "Bravo", "value": "b"}],
            "multi": true, "include_all": false, "all_value": null, "default": ["a"]
        }));
        let p = panel_of(d);
        p["repeat"] = json!({"variable": "host", "direction": "h"});
        p["data"]["query"]["sql"].as_array_mut().unwrap().push(json!("  AND host IN (${host})"));
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let url = "/api/dashboards/repeats4/panels/2/query";
    for bad in ["evil", "A", "a,b", ""] {
        let r = call(h.addr, "POST", url, &[JSON], &copy_body(&e, "host", bad)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{bad:?}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "host");
    }
    assert!(fake.queries().is_empty(), "no query for a refused value");
    // An option of the variable gives 200.
    let r = call(h.addr, "POST", url, &[JSON], &copy_body(&e, "host", "b")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(fake.queries()[0].sql().contains("host IN ('b')"), "{}", fake.queries()[0].sql());
}

// ------------------------------------------------------- the hostile values (P, PG, PM)

#[tokio::test]
async fn a_hostile_free_value_of_a_copy_on_a_clickhouse_source_is_refused_and_no_query_runs() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    // A free value in code context MUST be a number (7.8): the property P.
    let doc = repeat_doc("repeats5", |d| {
        let sql = panel_of(d)["data"]["query"]["sql"].as_array_mut().unwrap();
        *sql.last_mut().unwrap() = json!("  AND weight IN (${node:raw})");
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let url = "/api/dashboards/repeats5/panels/2/query";
    for hostile in ["1) OR (1=1", "1; DROP TABLE x", "x' OR '1'='1", "1 -- ", "$__all"] {
        let r = call(h.addr, "POST", url, &[JSON], &copy_body(&e, "node", hostile)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "node");
    }
    assert!(fake.queries().is_empty(), "the source saw no query");
    let r = call(h.addr, "POST", url, &[JSON], &copy_body(&e, "node", "12")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(fake.queries()[0].sql().contains("weight IN (12)"), "{}", fake.queries()[0].sql());
}

#[tokio::test]
async fn a_hostile_free_value_of_a_copy_on_a_postgres_source_is_refused_and_no_query_runs() {
    // The source has no server: a value that passes the checks gives `503 source_unavailable`.
    let h = Harness::start_with(test_timing(), fill, |c| c.postgres_sources.push(pg_block("metrics", "Metrics"))).await;
    let mut doc: Value =
        serde_json::from_str(&doc_on("repeats6", "metrics", "table", &["SELECT n FROM t WHERE n IN (${host:raw})"]))
            .unwrap();
    doc["variables"] = json!([{
        "kind": "query", "name": "host", "label": null, "hide": "none", "source": "metrics",
        "sql": ["SELECT DISTINCT host FROM t"], "multi": true, "include_all": true, "all_value": null,
        "refresh": "load", "sort": "alpha_asc", "regex": null, "default": ["$__all"]
    }]);
    doc["sections"][0]["panels"][0]["repeat"] = json!({"variable": "host", "direction": "v"});
    let r = create(&h, &serde_json::to_string(&doc).unwrap()).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let url = "/api/dashboards/repeats6/panels/1/query";
    let body = |value: &str| panel_body(&e, json!({"variables": {"host": {"all": false, "values": [value]}}}));
    for hostile in ["1) OR (1=1", "1; DROP TABLE x", "E'\\", "1 -- "] {
        let r = call(h.addr, "POST", url, &[JSON], &body(hostile)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
    }
    // No query reached the source: the log has no `query.run` line.
    assert!(!h.events().contains(&"query.run".to_owned()));
    let r = call(h.addr, "POST", url, &[JSON], &body("12")).await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert!(h.events().contains(&"query.run".to_owned()));
}

#[tokio::test]
async fn a_hostile_free_value_of_a_copy_on_a_mongodb_source_is_refused_and_no_query_runs() {
    let block = mongo_block("events7", "Events");
    let fake = FakeMongo::for_config(&block);
    test_connectors::put("events7", fake.connector());
    let timing = crate::server::limits::Timing { probe_sources: true, ..test_timing() };
    let h = Harness::start_with(timing, fill, |c| c.mongo_sources.push(block)).await;
    for _ in 0..200 {
        if get(h.ops, "/readyz").await.json()["sources"]["events7"] == "up" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"n":{"$in":"${host:json_number}"}}}]}"#;
    let mut doc: Value = serde_json::from_str(&doc_on("repeats7", "events7", "table", &[text])).unwrap();
    doc["variables"] = json!([{
        "kind": "query", "name": "host", "label": null, "hide": "none", "source": "events7",
        "sql": [r#"{"collection":"orders","aggregate":[{"$group":{"_id":"$name"}},{"$project":{"_id":0,"host":"$_id"}}]}"#],
        "multi": true, "include_all": true, "all_value": null, "refresh": "load", "sort": "alpha_asc",
        "regex": null, "default": ["$__all"]
    }]);
    doc["sections"][0]["panels"][0]["repeat"] = json!({"variable": "host", "direction": "h", "max_per_row": 2});
    let r = create(&h, &serde_json::to_string(&doc).unwrap()).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let url = "/api/dashboards/repeats7/panels/1/query";
    let body = |value: &str| panel_body(&e, json!({"variables": {"host": {"all": false, "values": [value]}}}));
    for hostile in ["12abc", "1) OR (1=1", "{\"$out\":\"x\"}", "$secret"] {
        let r = call(h.addr, "POST", url, &[JSON], &body(hostile)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "host");
    }
    assert!(fake.requests().is_empty(), "the source saw no command");
    fake.push_read(hune_source::FakeMongoReply::docs(&[r#"{"host":"a"}"#]));
    let r = call(h.addr, "POST", url, &[JSON], &body("12")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(fake.requests().len(), 1);
}

// ---------------------------------------------------------------- admission (8.8)

#[tokio::test]
async fn twenty_four_copies_at_the_same_time_take_permits_queue_and_get_source_busy_as_8_8_says() {
    // Each query takes 400 ms at the source. The subject has 3 permits and a wait of 600 ms.
    let arrivals: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = arrivals.clone();
    let fake = Fake::start(healthy(move |rec| {
        seen.lock().unwrap().push(Instant::now());
        let mut steps = vec![Step::Sleep(Duration::from_millis(400))];
        steps.extend(ok(rec, FRAME_BODY));
        steps
    }))
    .await;
    let h = start_with_fake(&fake, fill, |c| {
        c.clickhouse[0].max_concurrent_per_subject = Some(3);
        c.clickhouse[0].queue_wait = Duration::from_millis(600);
    })
    .await;
    // The document has the repeat; the page sends one request for each of the 24 values.
    let r = create(&h, &repeat_doc("repeats8", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e8 = etag_of(&r);
    let addr = h.addr;
    let tasks: Vec<_> = (0..24)
        .map(|i| {
            let body = copy_body(&e8, "node", &format!("v{i:02}"));
            tokio::spawn(
                async move { call(addr, "POST", "/api/dashboards/repeats8/panels/2/query", &[JSON], &body).await },
            )
        })
        .collect();
    let mut done = Vec::new();
    for t in tasks {
        done.push(t.await.unwrap());
    }
    let oks = done.iter().filter(|r| r.status == 200).count();
    let busy: Vec<&Resp> = done.iter().filter(|r| r.status == 503).collect();
    assert_eq!(oks + busy.len(), 24, "only 200 and 503: {:?}", done.iter().map(|r| r.status).collect::<Vec<_>>());
    assert!((3..=9).contains(&oks), "the permits give 3 queries at a time: {oks} succeeded");
    assert!(busy.len() >= 12, "{} refused", busy.len());
    for r in &busy {
        assert_eq!(r.code().as_str(), "source_busy", "{}", r.text());
        assert_eq!(r.header("retry-after"), Some("1"));
        assert_eq!(r.json()["error"]["retryable"], true);
    }
    // The source got one query for each success, and never more than 3 at the same time.
    let times = arrivals.lock().unwrap().clone();
    assert_eq!(times.len(), oks, "a refused copy never reached the source");
    for t in &times {
        let within = times.iter().filter(|u| **u >= *t && u.duration_since(*t) < Duration::from_millis(350)).count();
        assert!(within <= 3, "{within} queries began within 350 ms: the permits are 3");
    }
    let m = get(h.ops, "/metrics").await.text();
    let refused: usize = m
        .lines()
        .find_map(|l| l.strip_prefix("hune_source_refused_total{source=\"prod\",reason=\"queue_wait\"} "))
        .and_then(|v| v.parse().ok())
        .expect("the counter of the refusals");
    assert_eq!(refused, busy.len(), "{m}");
}

// ------------------------------------------------------------------- log and routes

#[tokio::test]
async fn the_query_run_line_has_the_panel_id_and_no_value_and_the_route_table_has_no_new_route() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = create(&h, &repeat_doc("repeats9", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let value = "value-marker-4417";
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/repeats9/panels/2/query",
        &[JSON, ("X-Request-Id", "req-r9")],
        &copy_body(&e, "node", value),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let line: Value = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|v| v["event"] == "query.run")
        .expect("query.run");
    assert_eq!(line["kind"], "panel");
    assert_eq!(line["uid"], "repeats9");
    assert_eq!(line["panel"], 2, "the stored id of the panel, never a key");
    assert_eq!(line["outcome"], "ok");
    for l in h.logger.memory_lines() {
        assert!(!l.contains(value), "the value shows in a log line: {l}");
    }
    // The attribution comment of the source holds the id of the panel and no value.
    let comment = fake.queries()[0].param("log_comment").unwrap();
    assert!(!comment.contains(value), "{comment}");
    assert!(comment.contains("\"panel\":2"), "{comment}");
    // Repeats add no route: the 24 routes of before and the route of 6.21a make 25. None is for a repeat.
    assert_eq!(crate::server::route::ROUTES.len(), 25);
    assert!(crate::server::route::ROUTES.iter().all(|r| !r.label.contains("repeat") && !r.label.contains("copy")));
    let r = call(h.addr, "POST", "/api/dashboards/repeats9/repeat/2/query", &[JSON], &copy_body(&e, "node", "a")).await;
    assert_eq!(r.status, 404);
}
