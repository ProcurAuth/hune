//! Several queries in one panel (4.15a, 6.20): the cases A70 to A75 of 16.8 and the rules of decisions
//! ID-K160 to ID-K175 at the server. A request runs one query. The tests check the save of each query, the
//! answer for each ref, the refusal of a hidden query, the panel members, the properties P, PG and PM, the
//! mixed sources, the admission, the preview, the questions of the PDP, the log and the route table.

use super::api::{JSON, NODES, etag_of, fill, key, post_body, put_body};
use super::authzen_pdp::*;
use super::fake_ch::{FRAME_BODY, Fake, Step, healthy, ok};
use super::harness::*;
use super::mongo_source::mongo_block;
use super::pg_source::pg_block;
use super::query::{draft_panel, panel_body, post_preview, preview_body, queries};
use crate::config::Config;
use crate::server::sources::test_connectors;
use hune_source::FakeMongo;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const B_SQL: &str = "SELECT $__timeInterval(ts) AS t, avg(v) AS mem FROM analytics.mem_marker WHERE $__timeFilter(ts) AND node IN (${node}) GROUP BY t ORDER BY t";
const C_SQL: &str = "SELECT count() AS errors FROM analytics.errors_marker WHERE env = '${env}'";

/// The fixture `nodes` with another uid. The panel 1 has three queries: `A` (the query of the fixture, on
/// `prod`, with `min_interval` 1m), `B` on `prod` and `C` on `conf`. `edit` changes the document.
fn multi_doc(uid: &str, edit: impl FnOnce(&mut Value)) -> Value {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("Several queries");
    doc["sections"][0]["panels"][0]["data"]["extra_queries"] = json!([
        {"ref": "B", "source": "prod", "format": "time_series", "sql": [B_SQL]},
        {"ref": "C", "source": "conf", "format": "table", "sql": [C_SQL]},
    ]);
    edit(&mut doc);
    doc
}

/// Adds the source `conf` to the configuration. It is a copy of `prod` that connects to `fake`.
fn with_conf(fake: &Fake) -> impl FnOnce(&mut Config) + '_ {
    move |c| {
        let mut conf = c.clickhouse[0].clone();
        conf.id = "conf".to_owned();
        conf.label = "Conformance".to_owned();
        conf.url.host = fake.addr.ip().to_string();
        conf.url.port = fake.addr.port();
        conf.url.text = format!("http://{}", fake.addr);
        c.clickhouse.push(conf);
    }
}

async fn create(h: &Harness, doc: &Value) -> Resp {
    call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("transit", &serde_json::to_string(doc).unwrap()),
    )
    .await
}

/// Creates the dashboard and gives its etag.
async fn created(h: &Harness, doc: &Value) -> String {
    let r = create(h, doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    etag_of(&r)
}

fn uid_of(doc: &Value) -> String {
    doc["uid"].as_str().unwrap().to_owned()
}

async fn run(h: &Harness, uid: &str, panel: u32, etag: &str, extra: Value) -> Resp {
    let url = format!("/api/dashboards/{uid}/panels/{panel}/query");
    call(h.addr, "POST", &url, &[JSON], &panel_body(etag, extra)).await
}

async fn run_ref(h: &Harness, uid: &str, etag: &str, query_ref: &str) -> Resp {
    run(h, uid, 1, etag, json!({"ref": query_ref})).await
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

fn run_lines(h: &Harness) -> Vec<Value> {
    h.logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["event"] == "query.run")
        .collect()
}

/// A panel of the type `stat` with `extra` items of `extra_queries`, at the row `y`.
fn stat_panel(id: u32, y: u32, extra: usize) -> Value {
    let mut data = json!({"query": {"source": "prod", "format": "table", "sql": ["SELECT 1"],
        "min_interval": null, "max_data_points": null}});
    if extra > 0 {
        let items: Vec<Value> = (0..extra)
            .map(|i| json!({"ref": format!("R{i}"), "source": "prod", "format": "table", "sql": ["SELECT 1"]}))
            .collect();
        data["extra_queries"] = json!(items);
    }
    json!({
        "id": id, "type": "stat", "title": "P", "description": "", "transparent": false,
        "grid": {"x": 0, "y": y, "w": 24, "h": 1}, "data": data, "transforms": [],
        "fields": {"defaults": {}, "overrides": []},
        "options": {"reduce": {"calc": "last_not_null", "fields": null, "values": false}, "color_mode": "value",
            "graph_mode": "none", "text_mode": "auto", "orientation": "auto"},
        "links": []
    })
}

/// Validates a document with `validate_only` and gives the answer.
async fn validate_only(h: &Harness, doc: &Value) -> Resp {
    let uid = uid_of(doc);
    let e = etag_of(&get(h.addr, &format!("/api/dashboards/{uid}")).await);
    call(
        h.addr,
        "PUT",
        &format!("/api/dashboards/{uid}?validate_only=true"),
        &[JSON, ("If-Match", &format!("\"{e}\""))],
        &put_body(&serde_json::to_string(doc).unwrap(), ""),
    )
    .await
}

// ------------------------------------------------------------------------------------------- A72: the save

#[tokio::test]
async fn a72_a_save_checks_each_query_and_keeps_the_canonical_bytes() {
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&conf, fill, with_conf(&conf)).await;
    // A save with `extra_queries` gives the canonical bytes: `extra_queries` after `query`, `ref` first in an item.
    let doc = multi_doc("multi72", |d| {
        // Members in another order, the default ref `A` and `hide` false: the canonical form leaves them out.
        d["sections"][0]["panels"][0]["data"]["query"]["ref"] = json!("A");
        d["sections"][0]["panels"][0]["data"]["query"]["hide"] = json!(false);
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0] =
            json!({"sql": [B_SQL], "format": "time_series", "source": "prod", "ref": "B", "hide": false});
    });
    let e = created(&h, &doc).await;
    let stored = get(h.addr, "/api/dashboards/multi72?download=true").await;
    assert_eq!(stored.status, 200);
    let canonical = hune_core::document::validate(&serde_json::to_vec(&doc).unwrap(), None).unwrap();
    assert_eq!(stored.body, canonical.bytes());
    assert_eq!(e, canonical.etag().to_hex());
    let text = stored.text();
    assert!(text.find("\"extra_queries\"").unwrap() > text.find("\"query\": {").unwrap());
    assert!(!text.contains("\"hide\": false") && !text.contains("\"ref\": \"A\""), "{text}");
    // A fault in the second extra query has the pointer of its SQL line.
    let bad = multi_doc("multi72", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"][1]["sql"] = json!(["DROP TABLE x"]);
    });
    let r = validate_only(&h, &bad).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert!(
        pointers(&r).iter().any(|(p, _)| p == "/sections/0/panels/0/data/extra_queries/1/sql/0"),
        "{:?}",
        pointers(&r)
    );
    // A source that the configuration does not declare gives `bad_reference` at the member `source`.
    let bad = multi_doc("multi72", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0]["source"] = json!("nowhere");
    });
    let r = validate_only(&h, &bad).await;
    assert_eq!(
        pointers(&r),
        vec![("/sections/0/panels/0/data/extra_queries/0/source".to_owned(), "bad_reference".to_owned())]
    );
    // 8 queries are valid, and 9 give `out_of_range`.
    let eight = multi_doc("multi72", |d| {
        d["sections"][0]["panels"] = json!([stat_panel(1, 0, 7)]);
        d["variables"] = json!([]);
        d["sections"][1]["panels"] = json!([]);
    });
    assert_eq!(validate_only(&h, &eight).await.status, 200);
    let nine = multi_doc("multi72", |d| {
        d["sections"][0]["panels"] = json!([stat_panel(1, 0, 8)]);
        d["variables"] = json!([]);
        d["sections"][1]["panels"] = json!([]);
    });
    let r = validate_only(&h, &nine).await;
    assert_eq!(pointers(&r), vec![("/sections/0/panels/0/data/extra_queries".to_owned(), "out_of_range".to_owned())]);
    // 400 queries are valid, and 401 give `too_large`.
    let queries_of = |count: usize| {
        multi_doc("multi72", move |d| {
            let mut panels: Vec<Value> = (0..count / 8).map(|i| stat_panel(i as u32 + 1, i as u32, 7)).collect();
            if !count.is_multiple_of(8) {
                panels.push(stat_panel(100, 60, count % 8 - 1));
            }
            d["variables"] = json!([]);
            d["sections"][0]["panels"] = json!(panels);
            d["sections"][1]["panels"] = json!([]);
        })
    };
    assert_eq!(validate_only(&h, &queries_of(400)).await.status, 200);
    let r = validate_only(&h, &queries_of(401)).await;
    assert_eq!(pointers(&r), vec![("/sections".to_owned(), "too_large".to_owned())]);
    // A panel with `repeat` and `extra_queries` gives `not_permitted`.
    let repeat = multi_doc("multi72", |d| {
        d["sections"][0]["panels"][0]["repeat"] = json!({"variable": "node", "direction": "v"});
        d["sections"][0]["panels"].as_array_mut().unwrap().truncate(1);
    });
    let r = validate_only(&h, &repeat).await;
    assert_eq!(pointers(&r), vec![("/sections/0/panels/0/data/extra_queries".to_owned(), "not_permitted".to_owned())]);
    // A panel with all its queries hidden is a valid document.
    let hidden = multi_doc("multi72", |d| {
        let data = &mut d["sections"][0]["panels"][0]["data"];
        data["query"]["hide"] = json!(true);
        data["extra_queries"][0]["hide"] = json!(true);
        data["extra_queries"][1]["hide"] = json!(true);
    });
    assert_eq!(validate_only(&h, &hidden).await.status, 200);
}

// ----------------------------------------------------------------------------- A70: a request for each query

#[tokio::test]
async fn a70_a_request_with_a_ref_for_each_query_gives_three_answers_with_their_own_source_and_sql() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, |c| {
        with_conf(&conf)(c);
        c.server.query_inspect = true;
    })
    .await;
    let e = created(&h, &multi_doc("multi70", |_| {})).await;
    let mut ids = Vec::new();
    let mut shown = Vec::new();
    for query_ref in ["A", "B", "C"] {
        let r = run_ref(&h, "multi70", &e, query_ref).await;
        assert_eq!(r.status, 200, "{query_ref}: {}", r.text());
        let j = r.json();
        assert_eq!(j["ref"], query_ref);
        assert_eq!(j["etag"], e);
        ids.push(j["query_id"].as_str().unwrap().to_owned());
        shown.push(j["sql"].as_str().unwrap().to_owned());
        // `sql` stays the last member, and `ref` comes before it.
        let text = r.text();
        assert!(text.rfind("\"ref\":").unwrap() < text.rfind("\"sql\":").unwrap());
        assert!(text.ends_with('}') && text[text.rfind("\"sql\":").unwrap()..].matches("\":").count() == 1);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3, "each answer has its own query_id");
    assert!(shown[0].contains("node_weight"), "{}", shown[0]);
    assert!(shown[1].contains("mem_marker"), "{}", shown[1]);
    assert!(shown[2].contains("errors_marker") && shown[2].contains("env = 'prod'"), "{}", shown[2]);
    // Each source got its own queries: A and B on `prod`, C on `conf`.
    let on_prod: Vec<String> = prod.queries().iter().map(|q| q.sql()).collect();
    assert_eq!(on_prod.len(), 2, "{on_prod:?}");
    assert!(on_prod[0].contains("node_weight") && on_prod[1].contains("mem_marker"));
    let on_conf: Vec<String> = conf.queries().iter().map(|q| q.sql()).collect();
    assert_eq!(on_conf.len(), 1, "{on_conf:?}");
    assert!(on_conf[0].contains("errors_marker"));
    // A request with no `ref` runs the first query, and its answer has no `ref`.
    let r = run(&h, "multi70", 1, &e, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("ref").is_none(), "{}", r.text());
    assert!(r.json()["sql"].as_str().unwrap().contains("node_weight"));
    // A panel with one query: a request with no `ref` has the answer of today.
    let r = run(&h, "multi70", 2, &e, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("ref").is_none());
    // The first query can have another ref: the ref `A` then names no query.
    let doc = multi_doc("multi70b", |d| {
        d["sections"][0]["panels"][0]["data"]["query"]["ref"] = json!("Z");
    });
    let e2 = created(&h, &doc).await;
    assert_eq!(run_ref(&h, "multi70b", &e2, "Z").await.json()["ref"], "Z");
    assert_eq!(run_ref(&h, "multi70b", &e2, "A").await.status, 404);
}

#[tokio::test]
async fn a70_a_ref_that_no_query_has_gives_404_and_a_wrong_body_gives_400_with_no_byte_to_a_source() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, with_conf(&conf)).await;
    let e = created(&h, &multi_doc("multi70c", |_| {})).await;
    let r = run_ref(&h, "multi70c", &e, "Z").await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This panel has no such query.");
    // The refs are case sensitive. A panel with one query has the ref `A` only.
    assert_eq!(run_ref(&h, "multi70c", &e, "b").await.status, 404);
    assert_eq!(run(&h, "multi70c", 2, &e, json!({"ref": "B"})).await.status, 404);
    assert_eq!(run(&h, "multi70c", 2, &e, json!({"ref": "A"})).await.status, 200);
    let before = prod.queries().len();
    // A `ref` that breaks the grammar, a `ref` of a wrong type, an SQL member and a `source` member.
    for extra in [
        json!({"ref": "1B"}),
        json!({"ref": "A".repeat(17)}),
        json!({"ref": 5}),
        json!({"sql": "SELECT 1"}),
        json!({"ref": "B", "source": "conf"}),
        json!({"ref": "B", "sql": ["SELECT 1"]}),
    ] {
        let r = run(&h, "multi70c", 1, &e, extra.clone()).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{extra}: {}", r.text());
    }
    assert_eq!(prod.queries().len(), before, "no byte to the source");
    assert!(conf.queries().is_empty());
}

// ------------------------------------------------------------------------------------- A71: hidden queries

#[tokio::test]
async fn a71_a_hidden_query_gives_400_with_or_without_ref_and_no_byte_reaches_the_source() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, with_conf(&conf)).await;
    let doc = multi_doc("multi71", |d| {
        // The query `B` is hidden. The first query of the panel 2 is hidden: it has no ref.
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0]["hide"] = json!(true);
        d["sections"][1]["panels"][0]["data"]["query"]["hide"] = json!(true);
    });
    let e = created(&h, &doc).await;
    let r = run_ref(&h, "multi71", &e, "B").await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This query is hidden.");
    let r = run(&h, "multi71", 2, &e, json!({})).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    let r = run(&h, "multi71", 2, &e, json!({"ref": "A"})).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert!(prod.queries().is_empty() && conf.queries().is_empty(), "no byte to a source");
    assert!(run_lines(&h).is_empty(), "the log has no query.run line");
    // The other queries of the panel run.
    assert_eq!(run_ref(&h, "multi71", &e, "A").await.status, 200);
    assert_eq!(run_ref(&h, "multi71", &e, "C").await.status, 200);
    assert_eq!(prod.queries().len(), 1);
    assert_eq!(conf.queries().len(), 1);
    assert_eq!(run_lines(&h).len(), 2);
}

// --------------------------------------------------------------------------------- the panel members

#[tokio::test]
async fn the_panel_members_of_data_query_apply_to_each_query() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, |c| {
        with_conf(&conf)(c);
        c.server.query_inspect = true;
    })
    .await;
    let e = created(&h, &multi_doc("multi73", |_| {})).await;
    // The first query has `min_interval` 1m. With 1200 points on a 6 hours range, the interval is 18 s: the
    // minimum of the first query gives 60 s to each query of the panel.
    for query_ref in ["A", "B", "C"] {
        let r = run_ref(&h, "multi73", &e, query_ref).await;
        assert_eq!(r.status, 200, "{query_ref}: {}", r.text());
        assert_eq!(r.json()["interval_ms"], 60_000, "{query_ref}");
    }
    // The expanded SQL of the second query holds the interval of the first.
    let sql = prod.queries()[1].sql();
    assert!(sql.contains("mem_marker") && sql.contains("INTERVAL 60 second"), "{sql}");
    // `max_data_points` of the body applies to each query: 2 points on a 6 hours range give 10800 s.
    let r = run(&h, "multi73", 1, &e, json!({"ref": "B", "max_data_points": 2})).await;
    assert_eq!(r.json()["interval_ms"], 10_800_000);
    // A change of the first query: the panel member goes with it. A panel with no `min_interval` gives 18 s.
    let doc = multi_doc("multi73b", |d| {
        d["sections"][0]["panels"][0]["data"]["query"]["min_interval"] = Value::Null;
    });
    let e2 = created(&h, &doc).await;
    let r = run_ref(&h, "multi73b", &e2, "B").await;
    assert_eq!(r.json()["interval_ms"], 20_000, "{}", r.text());
}

// ------------------------------------------------------------------ T66: the properties P, PG and PM

#[tokio::test]
async fn t66_a_hostile_free_value_in_the_second_query_on_clickhouse_gives_422_and_no_query_at_the_source() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, with_conf(&conf)).await;
    // A free value in code context MUST be a number (7.8): the property P.
    let doc = multi_doc("multi66", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0]["sql"] = json!(["SELECT 1 WHERE n IN (${node:raw})"]);
        d["sections"][0]["panels"][0]["data"]["extra_queries"][1]["sql"] = json!(["SELECT 2 WHERE n IN (${node:raw})"]);
    });
    let e = created(&h, &doc).await;
    for query_ref in ["B", "C"] {
        for hostile in ["1) OR (1=1", "1; DROP TABLE x", "x' OR '1'='1", "1 -- "] {
            let r = run_free(&h, "multi66", &e, query_ref, hostile).await;
            assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
            assert_eq!(r.json()["error"]["variable"], "node");
        }
    }
    assert!(prod.queries().is_empty() && conf.queries().is_empty(), "the sources saw no query");
    assert_eq!(run_free(&h, "multi66", &e, "B", "12").await.status, 200);
    assert!(prod.queries()[0].sql().contains("n IN (12)"));
    assert_eq!(run_free(&h, "multi66", &e, "C", "13").await.status, 200);
    assert!(conf.queries()[0].sql().contains("n IN (13)"));
}

/// A request for the query `ref` with one value of the variable `node`.
async fn run_free(h: &Harness, uid: &str, etag: &str, query_ref: &str, value: &str) -> Resp {
    let variables = json!({
        "node": {"all": false, "values": [value]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
    });
    run(h, uid, 1, etag, json!({"ref": query_ref, "variables": variables})).await
}

#[tokio::test]
async fn t66_a_hostile_free_value_in_the_second_query_on_postgres_gives_422_and_no_query_at_the_source() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    // The source `metrics` has no server: a value that passes the checks gives `503 source_unavailable`.
    let h = start_with_fake(&prod, fill, |c| c.postgres_sources.push(pg_block("metrics", "Metrics"))).await;
    let doc = multi_doc("multi66p", |d| {
        let extra = &mut d["sections"][0]["panels"][0]["data"]["extra_queries"];
        *extra = json!([{"ref": "B", "source": "metrics", "format": "table",
            "sql": ["SELECT n FROM t WHERE n IN (${node:raw})"]}]);
    });
    let e = created(&h, &doc).await;
    for hostile in ["1) OR (1=1", "1; DROP TABLE x", "E'\\", "1 -- "] {
        let r = run_free(&h, "multi66p", &e, "B", hostile).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
    }
    assert!(run_lines(&h).is_empty(), "no query reached a source");
    let r = run_free(&h, "multi66p", &e, "B", "12").await;
    assert_eq!((r.status, r.code().as_str()), (503, "source_unavailable"), "{}", r.text());
    assert_eq!(run_lines(&h).len(), 1);
    // The first query of the panel stays on ClickHouse and answers.
    assert_eq!(run_ref(&h, "multi66p", &e, "A").await.status, 200);
}

#[tokio::test]
async fn t66_a_hostile_free_value_in_the_second_query_on_mongodb_gives_422_and_no_command_at_the_source() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let block = mongo_block("events66", "Events");
    let fake = FakeMongo::for_config(&block);
    test_connectors::put("events66", fake.connector());
    let h = start_with_fake(&prod, fill, |c| c.mongo_sources.push(block)).await;
    for _ in 0..200 {
        if get(h.ops, "/readyz").await.json()["sources"]["events66"] == "up" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let text = r#"{"collection":"orders","aggregate":[{"$match":{"n":{"$in":"${node:json_number}"}}}]}"#;
    let doc = multi_doc("multi66m", |d| {
        let extra = &mut d["sections"][0]["panels"][0]["data"]["extra_queries"];
        *extra = json!([{"ref": "B", "source": "events66", "format": "table", "sql": [text]}]);
    });
    let e = created(&h, &doc).await;
    for hostile in ["12abc", "1) OR (1=1", "{\"$out\":\"x\"}", "$secret"] {
        let r = run_free(&h, "multi66m", &e, "B", hostile).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{hostile}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "node");
    }
    assert!(fake.requests().is_empty(), "the source saw no command");
    fake.push_read(hune_source::FakeMongoReply::docs(&[r#"{"host":"a"}"#]));
    let r = run_free(&h, "multi66m", &e, "B", "12").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["ref"], "B");
    assert_eq!(fake.requests().len(), 1);
    // The first query of the panel runs on ClickHouse with the rules of its own kind.
    assert_eq!(run_ref(&h, "multi66m", &e, "A").await.status, 200);
    assert_eq!(prod.queries().len(), 1);
}

// ---------------------------------------------------------------------------------- A74, A75: the sources

#[tokio::test]
async fn a74_a_refused_statement_of_one_query_gives_422_and_the_other_queries_answer() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    // The volume holds the document with no save check: the query `B` has a statement that 7.25 refuses.
    let doc = multi_doc("multi74", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0]["sql"] = json!(["DROP TABLE x"]);
    });
    let h = start_with_fake(
        &prod,
        |root| {
            fill(root);
            std::fs::write(root.join("transit/multi74.json"), serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        },
        |c| {
            with_conf(&conf)(c);
            // The limits of a source apply to its own queries only.
            c.clickhouse[1].max_query_size = 40;
        },
    )
    .await;
    let e = etag_of(&get(h.addr, "/api/dashboards/multi74").await);
    let r = run_ref(&h, "multi74", &e, "B").await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    assert!(prod.queries().is_empty(), "the source saw no byte");
    // The other queries of the panel answer, each on its own source, with the frame of 7.16.
    let r = run_ref(&h, "multi74", &e, "A").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["columns"][0]["kind"], "time");
    // The query `C` is above `max_query_size` of its own source, and only that query is refused.
    let r = run_ref(&h, "multi74", &e, "C").await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_too_large"), "{}", r.text());
    assert_eq!(r.json()["error"]["limit"], 40);
    assert_eq!((prod.queries().len(), conf.queries().len()), (1, 0));
}

#[tokio::test]
async fn a75_each_source_counts_its_own_queries_and_a_busy_query_fails_alone() {
    // Each query takes 700 ms at its source. The subject has 1 permit on each source and waits 250 ms.
    let slow = |tag: &'static str| {
        Fake::start(healthy(move |rec| {
            let _ = tag;
            let mut steps = vec![Step::Sleep(Duration::from_millis(700))];
            steps.extend(ok(rec, FRAME_BODY));
            steps
        }))
    };
    let (prod, conf) = (slow("prod").await, slow("conf").await);
    let h = start_with_fake(&prod, fill, |c| {
        with_conf(&conf)(c);
        for b in &mut c.clickhouse {
            b.max_concurrent_per_subject = Some(1);
            b.queue_wait = Duration::from_millis(250);
        }
    })
    .await;
    let e = created(&h, &multi_doc("multi75", |_| {})).await;
    let addr = h.addr;
    let tasks: Vec<_> = ["A", "B", "C"]
        .iter()
        .map(|r| {
            let body = panel_body(&e, json!({"ref": r}));
            tokio::spawn(
                async move { call(addr, "POST", "/api/dashboards/multi75/panels/1/query", &[JSON], &body).await },
            )
        })
        .collect();
    let mut done = Vec::new();
    for t in tasks {
        done.push(t.await.unwrap());
    }
    let status: Vec<u16> = done.iter().map(|r| r.status).collect();
    // `A` and `B` are on `prod`: one has the permit and one waits too long. `C` is on `conf` and has its own permit.
    assert_eq!(status[2], 200, "{status:?}: {}", done[2].text());
    assert_eq!(status.iter().filter(|s| **s == 200).count(), 2, "{status:?}");
    let busy = done.iter().find(|r| r.status == 503).expect("one query is refused");
    assert_eq!(busy.code().as_str(), "source_busy", "{}", busy.text());
    assert_eq!(prod.queries().len(), 1, "the refused query never reached its source");
    assert_eq!(conf.queries().len(), 1);
}

#[tokio::test]
async fn a75_eight_requests_of_one_panel_at_the_same_time_take_permits_queue_and_get_source_busy() {
    let arrivals: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = arrivals.clone();
    let prod = Fake::start(healthy(move |rec| {
        seen.lock().unwrap().push(Instant::now());
        let mut steps = vec![Step::Sleep(Duration::from_millis(500))];
        steps.extend(ok(rec, FRAME_BODY));
        steps
    }))
    .await;
    let h = start_with_fake(&prod, fill, |c| {
        c.clickhouse[0].max_concurrent_per_subject = Some(2);
        c.clickhouse[0].queue_wait = Duration::from_millis(300);
    })
    .await;
    let doc = multi_doc("multi75b", |d| {
        d["variables"] = json!([]);
        d["sections"][0]["panels"] = json!([stat_panel(1, 0, 7)]);
        d["sections"][1]["panels"] = json!([]);
    });
    let e = created(&h, &doc).await;
    let addr = h.addr;
    let refs = ["A", "R0", "R1", "R2", "R3", "R4", "R5", "R6"];
    let tasks: Vec<_> = refs
        .iter()
        .map(|r| {
            let body = serde_json::to_vec(&json!({"etag": e, "from_ms": 1_794_211_200_000_i64,
                "to_ms": 1_794_232_800_000_i64, "max_data_points": 1200, "variables": {}, "ref": r}))
            .unwrap();
            tokio::spawn(
                async move { call(addr, "POST", "/api/dashboards/multi75b/panels/1/query", &[JSON], &body).await },
            )
        })
        .collect();
    let mut done = Vec::new();
    for t in tasks {
        done.push(t.await.unwrap());
    }
    let oks = done.iter().filter(|r| r.status == 200).count();
    let busy: Vec<&Resp> = done.iter().filter(|r| r.status == 503).collect();
    assert_eq!(oks + busy.len(), 8, "only 200 and 503: {:?}", done.iter().map(|r| r.status).collect::<Vec<_>>());
    assert_eq!(oks, 2, "the two permits give two queries at a time");
    for r in &busy {
        assert_eq!(r.code().as_str(), "source_busy", "{}", r.text());
        assert_eq!(r.header("retry-after"), Some("1"));
    }
    // A refused query never reached the source. A failed query does not change the answer of the others.
    assert_eq!(arrivals.lock().unwrap().len(), oks);
    for r in done.iter().filter(|r| r.status == 200) {
        assert_eq!(r.json()["rows"], 2);
    }
}

// --------------------------------------------------------------------------------------------- the preview

#[tokio::test]
async fn the_preview_gives_400_for_a_draft_with_extra_queries_and_runs_the_selected_query() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, |_| {}).await;
    let mut panel = draft_panel("SELECT 1 AS n");
    panel["data"]["extra_queries"] =
        json!([{"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 2 AS n"]}]);
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": panel}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "A preview runs one query.");
    assert!(prod.queries().is_empty());
    // The editor sends the selected query as `data.query`, with the members of the panel: it runs.
    let mut panel = draft_panel("SELECT 2 AS n");
    panel["data"]["query"]["ref"] = json!("B");
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": panel}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("ref").is_none(), "the preview is unchanged");
    assert_eq!(prod.queries().len(), 1);
}

// ------------------------------------------------------------------------------------- A73: the PDP questions

fn allow(list: Vec<(&'static str, &'static str)>) -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    move |q| {
        let at = q.short();
        for (action, g) in &list {
            if *action == q.action && (g.is_empty() || at == *g || at.starts_with(&format!("{g}/"))) {
                return Ans::yes();
            }
        }
        Ans::No(None)
    }
}

fn pair(a: &str, r: &str) -> (String, String) {
    (a.to_owned(), r.to_owned())
}

#[tokio::test]
async fn a73_the_save_asks_source_query_on_each_source_hidden_included_and_each_request_asks_one_question() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let pdp = Pdp::start(allow(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])).await;
    let (pa, ca) = (prod.addr, conf.addr);
    let mut h = Harness::start_with(test_timing(), fill, |c| {
        authzen(c, &pdp.url(), Duration::from_secs(30));
        for (b, a) in c.clickhouse.iter_mut().zip([pa, ca]) {
            b.url.host = a.ip().to_string();
            b.url.port = a.port();
            b.url.text = format!("http://{a}");
        }
    })
    .await;
    let req = |h: &Harness,
               method: &'static str,
               path: String,
               sub: &'static str,
               extra: Vec<(String, String)>,
               body: Vec<u8>| {
        let addr = h.addr;
        async move {
            let t = token(sub);
            let mut hs: Vec<(&str, &str)> = vec![(HEADER, t.as_str())];
            for (k, v) in &extra {
                hs.push((k.as_str(), v.as_str()));
            }
            call(addr, method, &path, &hs, &body).await
        }
    };
    // A save of a panel whose second query names `conf`, hidden: `source.query` on `conf` is asked.
    let doc = multi_doc("multi73p", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"] = json!([
            {"ref": "B", "source": "conf", "format": "table", "sql": ["SELECT 1"], "hide": true}
        ]);
    });
    let text = serde_json::to_string(&doc).unwrap();
    pdp.clear();
    let r = req(
        &h,
        "POST",
        "/api/dashboards".to_owned(),
        "user:alice",
        vec![("Idempotency-Key".into(), key()), ("Content-Type".into(), "application/json".into())],
        post_body("transit", &text),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let asked: Vec<(String, String)> = pdp.asked().iter().map(|q| (q.action.clone(), q.short())).collect();
    let mut sources: Vec<_> = asked.iter().filter(|(a, _)| a == "source.query").cloned().collect();
    sources.sort();
    assert_eq!(sources, vec![pair("source.query", "source:conf"), pair("source.query", "source:prod")]);
    // A subject with `source.query` on `prod` only gets 403 for the save, also with the hidden query. The library
    // keeps the answers of a subject for `cache_ttl`, so this subject is another one.
    pdp.set_rules(allow(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "source:prod")]));
    let e2 = etag_of(&req(&h, "GET", "/api/dashboards/multi73p".into(), "user:dave", vec![], vec![]).await);
    pdp.clear();
    let r = req(
        &h,
        "PUT",
        "/api/dashboards/multi73p".into(),
        "user:dave",
        vec![
            ("If-Match".into(), format!("\"{e2}\"")),
            ("Idempotency-Key".into(), key()),
            ("Content-Type".into(), "application/json".into()),
        ],
        put_body(&text, "m"),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{}", r.text());
    // The body names the use of the save (11.30), whatever question the PDP refused. The refused question is
    // `source.query`: the use is not asked.
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    assert!(pdp.asked().iter().all(|q| q.action == "source.query"), "{:?}", pdp.asked());
    // A viewer with `dashboard.read` runs each query: each request asks `dashboard.read` on the node. The create of the
    // author asks its questions again, because a write route uses no kept answer (11.27): give the grants back.
    pdp.set_rules(allow(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")]));
    let doc2 = multi_doc("multi73q", |_| {});
    let r = req(
        &h,
        "POST",
        "/api/dashboards".into(),
        "user:alice",
        vec![("Idempotency-Key".into(), key()), ("Content-Type".into(), "application/json".into())],
        post_body("transit", &serde_json::to_string(&doc2).unwrap()),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e3 = etag_of(&r);
    pdp.set_rules(allow(vec![("dashboard.read", "")]));
    pdp.clear();
    for query_ref in ["A", "B", "C"] {
        let r = req(
            &h,
            "POST",
            "/api/dashboards/multi73q/panels/1/query".into(),
            "user:bob",
            vec![("Content-Type".into(), "application/json".into())],
            panel_body(&e3, json!({"ref": query_ref})),
        )
        .await;
        assert_eq!(r.status, 200, "{query_ref}: {}", r.text());
    }
    // The three requests ask the same question: with `cache_ttl` 30 s the library makes one call.
    let reads: Vec<_> = pdp.asked().iter().filter(|q| q.action == "dashboard.read").map(|q| q.short()).collect();
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert_eq!(pdp.requests(), 1);
    // The question never names the ref or the source of the query.
    assert!(reads[0].starts_with("folder:transit/dashboard:multi73q"), "{reads:?}");
    // A subject without `dashboard.read` gets 403 for a hidden ref and learns nothing about the queries.
    let doc3 = multi_doc("multi73r", |d| {
        d["sections"][0]["panels"][0]["data"]["extra_queries"][0]["hide"] = json!(true);
    });
    pdp.set_rules(allow(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")]));
    let r = req(
        &h,
        "POST",
        "/api/dashboards".into(),
        "user:alice",
        vec![("Idempotency-Key".into(), key()), ("Content-Type".into(), "application/json".into())],
        post_body("transit", &serde_json::to_string(&doc3).unwrap()),
    )
    .await;
    let e4 = etag_of(&r);
    pdp.set_rules(allow(vec![]));
    for query_ref in ["B", "Z"] {
        let r = req(
            &h,
            "POST",
            "/api/dashboards/multi73r/panels/1/query".into(),
            "user:carol",
            vec![("Content-Type".into(), "application/json".into())],
            panel_body(&e4, json!({"ref": query_ref})),
        )
        .await;
        assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{query_ref}: {}", r.text());
    }
    let _ = h.stop().await;
    assert_eq!(prod.queries().len() + conf.queries().len(), 3);
    // With `cache_ttl` 0 each request asks its own question: three requests, three questions.
    let h0 = Harness::start_with(test_timing(), fill, |c| {
        authzen(c, &pdp.url(), Duration::ZERO);
        for (b, a) in c.clickhouse.iter_mut().zip([pa, ca]) {
            b.url.host = a.ip().to_string();
            b.url.port = a.port();
            b.url.text = format!("http://{a}");
        }
    })
    .await;
    pdp.set_rules(allow(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")]));
    let r = req(
        &h0,
        "POST",
        "/api/dashboards".into(),
        "user:alice",
        vec![("Idempotency-Key".into(), key()), ("Content-Type".into(), "application/json".into())],
        post_body("transit", &serde_json::to_string(&multi_doc("multi73z", |_| {})).unwrap()),
    )
    .await;
    let e5 = etag_of(&r);
    pdp.set_rules(allow(vec![("dashboard.read", "")]));
    pdp.clear();
    for query_ref in ["A", "B", "C"] {
        let r = req(
            &h0,
            "POST",
            "/api/dashboards/multi73z/panels/1/query".into(),
            "user:bob",
            vec![("Content-Type".into(), "application/json".into())],
            panel_body(&e5, json!({"ref": query_ref})),
        )
        .await;
        assert_eq!(r.status, 200, "{query_ref}: {}", r.text());
    }
    let reads = pdp.asked().iter().filter(|q| q.action == "dashboard.read").count();
    assert_eq!(reads, 3, "each request asks its own question");
}

// ------------------------------------------------------------------------------------- log and the routes

#[tokio::test]
async fn the_log_line_and_the_comment_have_the_ref_and_the_route_table_has_no_new_route() {
    let prod = Fake::start(queries(FRAME_BODY)).await;
    let conf = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&prod, fill, with_conf(&conf)).await;
    let e = created(&h, &multi_doc("multi76", |_| {})).await;
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/multi76/panels/1/query",
        &[JSON, ("X-Request-Id", "req-q76")],
        &panel_body(&e, json!({"ref": "B"})),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let comment: Value = serde_json::from_str(&prod.queries()[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(
        comment,
        json!({"app": "hune", "kind": "panel", "uid": "multi76", "panel": 1, "ref": "B", "request_id": "req-q76",
            "subject": "-"})
    );
    let lines = run_lines(&h);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["ref"], "B");
    assert_eq!(lines[0]["panel"], 1);
    // A request with no `ref` has no `ref` in the comment.
    let r = run(&h, "multi76", 1, &e, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let comment: Value = serde_json::from_str(&prod.queries()[1].param("log_comment").unwrap()).unwrap();
    assert!(comment.get("ref").is_none(), "{comment}");
    // The field is `null` when the request has no `ref` (14.6: a field without a value is `null`).
    assert!(run_lines(&h)[1]["ref"].is_null());
    // The line and the comment never hold the SQL.
    for l in h.logger.memory_lines() {
        assert!(!l.contains("mem_marker"), "{l}");
    }
    // The feature adds no route: the 25 routes of before stay (6.9).
    assert_eq!(crate::server::route::ROUTES.len(), 25);
}
