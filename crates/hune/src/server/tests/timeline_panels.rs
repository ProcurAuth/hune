// SPDX-License-Identifier: MIT OR Apache-2.0
//! The state timeline and the status history (4.33b, 6.20) at the server: the save and the read keep the
//! canonical bytes, the validation gives the codes of 4.33b, the panel query is the ordinary panel query, and a
//! `reuse` of such a panel works as for a time series. The route table (6.9) has no new route.

use super::api::{JSON, NODES, etag_of, fill, key, post_body};
use super::fake_ch::{FRAME_BODY, Fake};
use super::harness::*;
use super::query::{panel_body, queries};
use serde_json::{Value, json};

fn options_of(kind: &str) -> Value {
    let legend = json!({"show": true, "placement": "bottom", "mode": "list", "values": []});
    let tooltip = json!({"mode": "single"});
    if kind == "state_timeline" {
        json!({"merge_values": true, "show_values": "auto", "align_values": "left", "row_height": 90, "line_width": 0,
            "fill_opacity": 70, "span_nulls": false, "show_labels": true, "legend": legend, "tooltip": tooltip})
    } else {
        json!({"show_values": "auto", "row_height": 90, "col_width": 90, "line_width": 1, "fill_opacity": 70,
            "show_labels": true, "legend": legend, "tooltip": tooltip})
    }
}

fn timeline_panel(id: u32, kind: &str, y: u32) -> Value {
    json!({
        "id": id,
        "type": kind,
        "title": "States",
        "description": "",
        "transparent": false,
        "grid": {"x": 0, "y": y, "w": 12, "h": 8},
        "data": {"query": {
            "source": "prod",
            "format": "table",
            "sql": ["SELECT toDateTime(1) AS t, 'up' AS state WHERE $__timeFilter(t)"],
            "min_interval": null,
            "max_data_points": null
        }},
        "transforms": [],
        "fields": {"defaults": {"mappings": [{"kind": "value", "value": "up", "text": "UP", "color": "green"}]}, "overrides": []},
        "options": options_of(kind),
        "links": []
    })
}

/// The fixture `nodes` with a last section that holds the panels 91 (state timeline), 92 (status history)
/// and 93 (a status history that reuses the panel 91). `edit` changes the document.
fn timeline_doc(uid: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("States");
    let mut reuse = timeline_panel(93, "status_history", 16);
    reuse["data"] = json!({"reuse": {"panel": 91, "with_transforms": false}});
    doc["sections"].as_array_mut().unwrap().push(json!({
        "title": "States",
        "collapsed": false,
        "panels": [timeline_panel(91, "state_timeline", 0), timeline_panel(92, "status_history", 8), reuse]
    }));
    edit(&mut doc);
    serde_json::to_string(&doc).unwrap()
}

fn last_section(doc: &mut Value) -> &mut Value {
    let last = doc["sections"].as_array().unwrap().len() - 1;
    &mut doc["sections"][last]
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

fn stored(h: &Harness, uid: &str) -> Vec<u8> {
    std::fs::read(std::fs::canonicalize(h.dir.path()).unwrap().join(format!("transit/{uid}.json")))
        .expect("the file of the document")
}

#[tokio::test]
async fn a_save_with_each_type_gives_the_canonical_bytes_and_a_read_gives_the_same_bytes() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    // The members of `options` are not in the canonical order in the input, and the tooltip has its defaults.
    let doc = timeline_doc("states1", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[0]["options"] = json!({
            "tooltip": {"mode": "single", "sort": "none", "hide_zeros": false},
            "legend": {"values": [], "mode": "list", "placement": "bottom", "show": true},
            "show_labels": true, "span_nulls": false, "fill_opacity": 70, "line_width": 0, "row_height": 90,
            "align_values": "left", "show_values": "auto", "merge_values": true
        });
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let canonical = hune_core::document::validate(doc.as_bytes(), None).unwrap();
    let bytes = stored(&h, "states1");
    assert_eq!(bytes, canonical.bytes());
    assert_eq!(etag_of(&r), canonical.etag().to_hex());
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains("\"options\": {\n            \"merge_values\": true,\n            \"show_values\": \"auto\",\n            \"align_values\": \"left\",\n            \"row_height\": 90,"),
        "{text}"
    );
    assert!(
        text.contains("\"options\": {\n            \"show_values\": \"auto\",\n            \"row_height\": 90,\n            \"col_width\": 90,"),
        "{text}"
    );
    // The read gives the same document and the same etag.
    let read = get(h.addr, "/api/dashboards/states1").await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_eq!(etag_of(&read), canonical.etag().to_hex());
    let panels = read.json()["document"]["sections"][2]["panels"].clone();
    assert_eq!(
        (panels[0]["type"].clone(), panels[1]["type"].clone()),
        (json!("state_timeline"), json!("status_history"))
    );
    assert_eq!(panels[0]["options"], options_of("state_timeline"));
    assert_eq!(panels[1]["options"], options_of("status_history"));
    // A document with no such panel keeps its bytes.
    let plain =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/nodes.json")).unwrap();
    assert_eq!(plain, NODES);
}

#[tokio::test]
async fn three_invalid_documents_give_422_with_the_codes_of_4_33b() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    type Case = (&'static str, Box<dyn FnOnce(&mut Value)>, &'static str, &'static str);
    let cases: [Case; 3] = [
        (
            "a legend in the table mode",
            Box::new(|d| last_section(d)["panels"][0]["options"]["legend"]["mode"] = json!("table")),
            "/sections/2/panels/0/options/legend/mode",
            "not_permitted",
        ),
        (
            "a row height of 101",
            Box::new(|d| last_section(d)["panels"][1]["options"]["row_height"] = json!(101)),
            "/sections/2/panels/1/options/row_height",
            "out_of_range",
        ),
        (
            "a member of the other type",
            Box::new(|d| last_section(d)["panels"][1]["options"]["merge_values"] = json!(true)),
            "/sections/2/panels/1/options/merge_values",
            "unknown_member",
        ),
    ];
    for (name, edit, pointer, code) in cases {
        let r = create(&h, &timeline_doc("states-bad", edit)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(pointers(&r), vec![(pointer.to_owned(), code.to_owned())], "{name}");
    }
    // A binary that does not know the type refuses it at `type`: a type with a wrong name gives `bad_format`.
    let r =
        create(&h, &timeline_doc("states-bad", |d| last_section(d)["panels"][0]["type"] = json!("state_timelines")))
            .await;
    assert_eq!(r.status, 422, "{}", r.text());
    assert!(
        pointers(&r).contains(&("/sections/2/panels/0/type".to_owned(), "bad_format".to_owned())),
        "{:?}",
        pointers(&r)
    );
}

#[tokio::test]
async fn a_panel_query_of_each_type_gives_200_as_for_a_table() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = create(&h, &timeline_doc("states2", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let table =
        call(h.addr, "POST", "/api/dashboards/states2/panels/2/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(table.status, 200, "{}", table.text());
    for id in [91, 92] {
        let path = format!("/api/dashboards/states2/panels/{id}/query");
        let r = call(h.addr, "POST", &path, &[JSON], &panel_body(&e, json!({}))).await;
        assert_eq!(r.status, 200, "{id}: {}", r.text());
        assert_eq!(r.header("cache-control"), Some("no-store"));
        let j = r.json();
        assert_eq!(j["etag"], e);
        assert_eq!(j["rows"], 2);
        assert_eq!(j["columns"][0], json!({"name": "t", "type": "DateTime", "kind": "time"}));
        let mut keys: Vec<String> = j.as_object().unwrap().keys().cloned().collect();
        let mut table_keys: Vec<String> = table.json().as_object().unwrap().keys().cloned().collect();
        keys.sort();
        table_keys.sort();
        assert_eq!(keys, table_keys, "{id}: the same answer as for a table");
    }
    // The panel 91 has the query that the document gives, with the macros expanded.
    let q = fake.queries();
    assert_eq!(q.len(), 3);
    for rec in &q[1..] {
        let sql = rec.sql();
        assert!(!sql.contains("$__") && sql.contains("'up' AS state"), "{sql}");
        let comment: Value = serde_json::from_str(&rec.param("log_comment").unwrap()).unwrap();
        assert_eq!(comment["kind"], "panel");
    }
    // The route table has no new route: a route for the picture does not exist.
    assert_eq!(get(h.addr, "/api/dashboards/states2/panels/91/timeline").await.status, 404);
}

#[tokio::test]
async fn a_reuse_of_such_a_panel_works_as_for_a_time_series() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = create(&h, &timeline_doc("states3", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    // The panel 93 reuses the frame of the panel 91: it has no query of its own, and the route says so.
    let r = call(h.addr, "POST", "/api/dashboards/states3/panels/93/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This panel uses the frame of another panel and has no query.");
    // The source panel answers, and the reuse target is the same for a time series as for these types.
    let r = call(h.addr, "POST", "/api/dashboards/states3/panels/91/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // A reuse of a reuse is invalid, and a reuse of the other type is valid.
    let doc = timeline_doc("states4", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[1]["data"] = json!({"reuse": {"panel": 93, "with_transforms": false}});
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r)[0].1, "bad_reference");
    let doc = timeline_doc("states5", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[2]["type"] = json!("state_timeline");
        panels[2]["options"] = options_of("state_timeline");
        panels[2]["data"] = json!({"reuse": {"panel": 92, "with_transforms": true}});
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
}
