// SPDX-License-Identifier: MIT OR Apache-2.0
//! The heatmap and the histogram (4.33c, 4.33d, 6.20) at the server: the save and the read keep the canonical
//! bytes, the validation gives the codes of 4.33c and 4.33d, the panel query is the ordinary panel query, and a
//! `reuse` of such a panel works as for a time series. A `repeat` and `extra_queries` follow 4.13a and 4.15a.
//! The route table (6.9) has no new route: the server never buckets.

use super::api::{JSON, NODES, etag_of, fill, key, post_body};
use super::fake_ch::{FRAME_BODY, Fake};
use super::harness::*;
use super::query::{panel_body, queries};
use serde_json::{Value, json};

fn options_of(kind: &str) -> Value {
    if kind == "heatmap" {
        json!({"calculate": true, "x_buckets": {"mode": "size", "value": 600000},
            "y_buckets": {"mode": "size", "value": 2, "scale": "log2"}, "y_axis": "left", "y_reverse": false,
            "y_min": null, "y_max": null,
            "color": {"mode": "scheme", "scheme": "viridis", "fill": "dark-orange", "scale": "exponential",
                "exponent": 0.5, "steps": 64, "reverse": false, "min": null, "max": 10},
            "cell_gap": 1, "hide_le": 0, "hide_ge": null, "show_legend": true, "tooltip": {"mode": "single"}})
    } else {
        json!({"bucket_count": 30, "bucket_size": null, "bucket_offset": 0, "combine": false, "stacking": "none",
            "line_width": 1, "fill_opacity": 80,
            "legend": {"show": true, "placement": "bottom", "mode": "list", "values": []},
            "tooltip": {"mode": "single"}})
    }
}

fn bucket_panel(id: u32, kind: &str, y: u32) -> Value {
    json!({
        "id": id,
        "type": kind,
        "title": "Latency",
        "description": "",
        "transparent": false,
        "grid": {"x": 0, "y": y, "w": 12, "h": 8},
        "data": {"query": {
            "source": "prod",
            "format": "table",
            "sql": ["SELECT toDateTime(1) AS t, 12.5 AS latency WHERE $__timeFilter(t)"],
            "min_interval": null,
            "max_data_points": null
        }},
        "transforms": [],
        "fields": {"defaults": {"unit": "ms"}, "overrides": []},
        "options": options_of(kind),
        "links": []
    })
}

/// The fixture `nodes` with a last section that holds the panels 91 (heatmap), 92 (histogram) and 93 (a
/// histogram that reuses the panel 91). `edit` changes the document.
fn bucket_doc(uid: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("Buckets");
    let mut reuse = bucket_panel(93, "histogram", 16);
    reuse["data"] = json!({"reuse": {"panel": 91, "with_transforms": false}});
    doc["sections"].as_array_mut().unwrap().push(json!({
        "title": "Buckets",
        "collapsed": false,
        "panels": [bucket_panel(91, "heatmap", 0), bucket_panel(92, "histogram", 8), reuse]
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
    let doc = bucket_doc("buckets1", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[0]["options"] = json!({
            "tooltip": {"mode": "single", "sort": "none", "hide_zeros": false},
            "show_legend": true, "hide_ge": null, "hide_le": 0, "cell_gap": 1,
            "color": {"max": 10, "min": null, "reverse": false, "steps": 64, "exponent": 0.5, "scale": "exponential",
                "fill": "dark-orange", "scheme": "viridis", "mode": "scheme"},
            "y_max": null, "y_min": null, "y_reverse": false, "y_axis": "left",
            "y_buckets": {"scale": "log2", "value": 2, "mode": "size"},
            "x_buckets": {"value": 600000, "mode": "size"}, "calculate": true
        });
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let canonical = hune_core::document::validate(doc.as_bytes(), None).unwrap();
    let bytes = stored(&h, "buckets1");
    assert_eq!(bytes, canonical.bytes());
    assert_eq!(etag_of(&r), canonical.etag().to_hex());
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains("\"options\": {\n            \"calculate\": true,\n            \"x_buckets\": {\n              \"mode\": \"size\",\n              \"value\": 600000\n            },"),
        "{text}"
    );
    assert!(
        text.contains("\"options\": {\n            \"bucket_count\": 30,\n            \"bucket_size\": null,\n            \"bucket_offset\": 0,"),
        "{text}"
    );
    // The read gives the same document and the same etag.
    let read = get(h.addr, "/api/dashboards/buckets1").await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_eq!(etag_of(&read), canonical.etag().to_hex());
    let panels = read.json()["document"]["sections"][2]["panels"].clone();
    assert_eq!((panels[0]["type"].clone(), panels[1]["type"].clone()), (json!("heatmap"), json!("histogram")));
    assert_eq!(panels[0]["options"], options_of("heatmap"));
    assert_eq!(panels[1]["options"], options_of("histogram"));
    // A document with no such panel keeps its bytes.
    let plain =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/nodes.json")).unwrap();
    assert_eq!(plain, NODES);
}

#[tokio::test]
async fn four_invalid_documents_give_422_with_the_codes_of_4_33c_and_4_33d() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    type Case = (&'static str, Box<dyn FnOnce(&mut Value)>, &'static str, &'static str);
    let cases: [Case; 4] = [
        (
            "a log scale with the mode count",
            Box::new(|d| last_section(d)["panels"][0]["options"]["y_buckets"]["mode"] = json!("count")),
            "/sections/2/panels/0/options/y_buckets/mode",
            "not_permitted",
        ),
        (
            "a colour range with min equal to max",
            Box::new(|d| last_section(d)["panels"][0]["options"]["color"]["min"] = json!(10)),
            "/sections/2/panels/0/options/color/max",
            "out_of_range",
        ),
        (
            "a bucket count of 1001",
            Box::new(|d| last_section(d)["panels"][1]["options"]["bucket_count"] = json!(1001)),
            "/sections/2/panels/1/options/bucket_count",
            "out_of_range",
        ),
        (
            "a member of the other type",
            Box::new(|d| last_section(d)["panels"][1]["options"]["calculate"] = json!(true)),
            "/sections/2/panels/1/options/calculate",
            "unknown_member",
        ),
    ];
    for (name, edit, pointer, code) in cases {
        let r = create(&h, &bucket_doc("buckets-bad", edit)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(pointers(&r), vec![(pointer.to_owned(), code.to_owned())], "{name}");
    }
    // A binary that does not know the type refuses it at `type`: a type with a wrong name gives `bad_format`.
    let r = create(&h, &bucket_doc("buckets-bad", |d| last_section(d)["panels"][0]["type"] = json!("heatmaps"))).await;
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
    let r = create(&h, &bucket_doc("buckets2", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    let table =
        call(h.addr, "POST", "/api/dashboards/buckets2/panels/2/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(table.status, 200, "{}", table.text());
    for id in [91, 92] {
        let path = format!("/api/dashboards/buckets2/panels/{id}/query");
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
    // The panels have the query that the document gives, with the macros expanded. The server never buckets.
    let q = fake.queries();
    assert_eq!(q.len(), 3);
    for rec in &q[1..] {
        let sql = rec.sql();
        assert!(!sql.contains("$__") && sql.contains("12.5 AS latency"), "{sql}");
        let comment: Value = serde_json::from_str(&rec.param("log_comment").unwrap()).unwrap();
        assert_eq!(comment["kind"], "panel");
    }
    // The route table has no new route: a route for the buckets does not exist.
    assert_eq!(get(h.addr, "/api/dashboards/buckets2/panels/91/buckets").await.status, 404);
}

#[tokio::test]
async fn a_reuse_of_such_a_panel_works_as_for_a_time_series() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    let r = create(&h, &bucket_doc("buckets3", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    // The panel 93 reuses the frame of the panel 91: it has no query of its own, and the route says so.
    let r = call(h.addr, "POST", "/api/dashboards/buckets3/panels/93/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This panel uses the frame of another panel and has no query.");
    // The source panel answers.
    let r = call(h.addr, "POST", "/api/dashboards/buckets3/panels/91/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    // A reuse of a reuse is invalid, and a reuse of the other type is valid.
    let doc = bucket_doc("buckets4", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[1]["data"] = json!({"reuse": {"panel": 93, "with_transforms": false}});
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r)[0].1, "bad_reference");
    let doc = bucket_doc("buckets5", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[2]["type"] = json!("heatmap");
        panels[2]["options"] = options_of("heatmap");
        panels[2]["data"] = json!({"reuse": {"panel": 92, "with_transforms": true}});
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
}

#[tokio::test]
async fn a_repeat_and_extra_queries_of_these_types_follow_4_13a_and_4_15a() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    // A heatmap and a histogram can repeat for the variable `node` and have extra queries.
    let doc = bucket_doc("buckets6", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[2].as_object_mut().unwrap().remove("data");
        panels[2]["data"] = json!({"query": {"source": "prod", "format": "table", "sql": ["SELECT 1"],
            "min_interval": null, "max_data_points": null}});
        panels[0]["repeat"] = json!({"variable": "node", "direction": "h", "max_per_row": 3});
        panels[1]["data"]["extra_queries"] =
            json!([{"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 2 AS v"]}]);
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let text = String::from_utf8(stored(&h, "buckets6")).unwrap();
    assert!(text.contains("\"repeat\": {\n            \"variable\": \"node\",\n            \"direction\": \"h\",\n            \"max_per_row\": 3"), "{text}");
    assert!(text.contains("\"extra_queries\": ["), "{text}");
    // A panel with a repeat and extra queries is refused, as for a time series (4.15a, rule 8).
    let doc = bucket_doc("buckets9", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[2].as_object_mut().unwrap().remove("data");
        panels[2]["data"] = json!({"query": {"source": "prod", "format": "table", "sql": ["SELECT 1"],
            "min_interval": null, "max_data_points": null}});
        panels[0]["repeat"] = json!({"variable": "node", "direction": "h"});
        panels[0]["data"]["extra_queries"] =
            json!([{"ref": "B", "source": "prod", "format": "time_series", "sql": ["SELECT 3 AS v"]}]);
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r), vec![("/sections/2/panels/0/data/extra_queries".to_owned(), "not_permitted".to_owned())]);
    // A reuse of a panel with a repeat is refused, as for a time series (4.13a, rule 8).
    let doc = bucket_doc("buckets7", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[0]["repeat"] = json!({"variable": "node", "direction": "h"});
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r), vec![("/sections/2/panels/2/data/reuse/panel".to_owned(), "bad_reference".to_owned())]);
    // A reuse with extra queries is refused (4.15a).
    let doc = bucket_doc("buckets8", |d| {
        let panels = last_section(d)["panels"].as_array_mut().unwrap();
        panels[2]["data"]["extra_queries"] =
            json!([{"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 2"]}]);
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(pointers(&r), vec![("/sections/2/panels/2/data/extra_queries".to_owned(), "not_permitted".to_owned())]);
}
