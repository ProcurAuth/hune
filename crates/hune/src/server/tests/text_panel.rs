// SPDX-License-Identifier: MIT OR Apache-2.0
//! The text panel (4.33a, 6.20, 6.22) at the server: the save and the read keep the canonical bytes, the
//! routes of the query and of the preview refuse the panel, and the text never reaches a log. The server
//! reads the lines only: it never parses the Markdown. The route table (6.9) has no new route.

use super::api::{JSON, NODES, etag_of, fill, key, post_body, put};
use super::authzen_pdp::{Ans, HEADER, Pdp, authzen, token};
use super::harness::*;
use super::query::{panel_body, post_preview, preview_body};
use serde_json::{Value, json};
use std::time::Duration;

/// A text that no log line holds. The tests look for it in the log.
const MARKER: &str = "MARKER-7f3a9c-text-never-in-a-log";

fn text_panel(id: u32, y: u32, content: &[&str]) -> Value {
    json!({
        "id": id,
        "type": "text",
        "title": "Notes",
        "description": "",
        "transparent": false,
        "grid": {"x": 0, "y": y, "w": 12, "h": 8},
        "options": {"mode": "markdown", "content": content},
        "links": []
    })
}

/// The fixture `nodes` with a section that holds the text panel 99. `edit` changes the document.
fn text_doc(uid: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut doc: Value = serde_json::from_str(NODES).unwrap();
    doc["uid"] = json!(uid);
    doc["title"] = json!("Notes");
    doc["sections"].as_array_mut().unwrap().push(json!({
        "title": "Notes",
        "collapsed": false,
        "panels": [text_panel(99, 0, &["# Runbook", "", "Region: $env. See [the wiki](https://wiki.example/runbook)."])]
    }));
    edit(&mut doc);
    serde_json::to_string(&doc).unwrap()
}

/// The text panel of the document of the tests.
fn text_of(doc: &mut Value) -> &mut Value {
    let last = doc["sections"].as_array().unwrap().len() - 1;
    &mut doc["sections"][last]["panels"][0]
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
async fn a_save_with_a_text_panel_gives_the_canonical_bytes_and_a_read_gives_the_same_bytes() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    // The members of the text panel are not in the canonical order in the input.
    let doc = text_doc("notes1", |d| {
        *text_of(d) = json!({
            "links": [],
            "options": {"content": ["# Runbook", "", "Tab:\there"], "mode": "markdown"},
            "grid": {"h": 8, "w": 12, "y": 0, "x": 0},
            "transparent": false,
            "description": "",
            "title": "Notes",
            "type": "text",
            "id": 99
        });
    });
    let r = create(&h, &doc).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let canonical = hune_core::document::validate(doc.as_bytes(), None).unwrap();
    let bytes = stored(&h, "notes1");
    assert_eq!(bytes, canonical.bytes());
    assert_eq!(etag_of(&r), canonical.etag().to_hex());
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains(
            "\"type\": \"text\",\n          \"title\": \"Notes\",\n          \"description\": \"\",\n          \"transparent\": false,\n          \"grid\": {\n            \"x\": 0,\n            \"y\": 0,\n            \"w\": 12,\n            \"h\": 8\n          },\n          \"options\": {\n            \"mode\": \"markdown\",\n            \"content\": [\n              \"# Runbook\",\n              \"\",\n              \"Tab:\\there\"\n            ]\n          },\n          \"links\": []\n        }"
        ),
        "{text}"
    );
    // The read gives the same document and the same etag.
    let read = get(h.addr, "/api/dashboards/notes1").await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_eq!(etag_of(&read), canonical.etag().to_hex());
    let panel = read.json()["document"]["sections"][2]["panels"][0].clone();
    assert_eq!(panel["type"], "text");
    assert_eq!(panel["options"], json!({"mode": "markdown", "content": ["# Runbook", "", "Tab:\there"]}));
    assert!(panel.get("data").is_none() && panel.get("fields").is_none() && panel.get("transforms").is_none());
    // A document with no text panel keeps its bytes.
    let plain =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/nodes.json")).unwrap();
    assert_eq!(plain, NODES);
    // A save of an existing document with a text panel works too.
    let e = etag_of(&read);
    let again = text_doc("notes1", |d| text_of(d)["options"]["content"] = json!(["changed"]));
    let r = put(&h, "notes1", &e, &again, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn three_invalid_documents_give_422_with_the_codes_of_4_33a() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    type Case = (&'static str, Box<dyn FnOnce(&mut Value)>, &'static str, &'static str);
    let cases: [Case; 3] = [
        (
            "the mode html",
            Box::new(|d| text_of(d)["options"]["mode"] = json!("html")),
            "/sections/2/panels/0/options/mode",
            "bad_format",
        ),
        (
            "a line with a line end",
            Box::new(|d| text_of(d)["options"]["content"] = json!(["a\nb"])),
            "/sections/2/panels/0/options/content/0",
            "bad_format",
        ),
        (
            "a member data",
            Box::new(|d| text_of(d)["data"] = json!({"reuse": {"panel": 1, "with_transforms": false}})),
            "/sections/2/panels/0/data",
            "not_permitted",
        ),
    ];
    for (name, edit, pointer, code) in cases {
        let r = create(&h, &text_doc("notes-bad", edit)).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(pointers(&r), vec![(pointer.to_owned(), code.to_owned())], "{name}");
    }
    // A text above a limit of 4.35 is `too_large`.
    let r = create(&h, &text_doc("notes-bad", |d| text_of(d)["options"]["content"] = json!(vec!["x"; 2001]))).await;
    assert_eq!(pointers(&r), vec![("/sections/2/panels/0/options/content".to_owned(), "too_large".to_owned())]);
}

#[tokio::test]
async fn a_panel_query_and_a_preview_of_a_text_panel_give_400_bad_request() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let r = create(&h, &text_doc("notes2", |_| {})).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    // The query route of 6.20: the panel exists, so the answer is not 404.
    let r = call(h.addr, "POST", "/api/dashboards/notes2/panels/99/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This panel has no query.");
    // The panel 99 is not the only panel: a panel with a query still runs the usual checks.
    let r = call(h.addr, "POST", "/api/dashboards/notes2/panels/1000/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 404, "{}", r.text());
    // The preview of 6.22: a draft text panel, with the route of an existing source.
    let draft = text_panel(7, 0, &["# Title"]);
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": draft, "uid": "notes2"}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    assert_eq!(r.json()["error"]["message"], "This panel has no query.");
    // The refusal does not depend on the rest of the draft: a text panel with a wrong member is still 400.
    let mut bad = text_panel(7, 0, &["# Title"]);
    bad["options"]["mode"] = json!("html");
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": bad}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
    // The route table has no new route: a route for the text does not exist.
    let r = get(h.addr, "/api/dashboards/notes2/panels/99/text").await;
    assert_eq!(r.status, 404, "{}", r.text());
}

#[tokio::test]
async fn a_reuse_of_a_text_panel_gives_422_bad_reference() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let doc = text_doc("notes3", |d| {
        // The panel 3 of the fixture reuses the panel 2: it reuses the text panel 99 now.
        d["sections"][0]["panels"][1]["data"]["reuse"]["panel"] = json!(99);
    });
    let r = create(&h, &doc).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    let got = pointers(&r);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].1, "bad_reference");
    assert!(got[0].0.ends_with("/data/reuse/panel"), "{got:?}");
    let message = r.json()["error"]["errors"][0]["message"].as_str().unwrap().to_owned();
    assert!(message.contains("a text panel has no query"), "{message}");
}

#[tokio::test]
async fn a_subject_with_dashboard_read_and_no_source_access_reads_a_dashboard_of_text_panels_only() {
    // The PDP gives `dashboard.read` only: no `source.query` and no `dashboard.write`.
    let pdp = Pdp::start(|q| if q.action == "dashboard.read" { Ans::yes() } else { Ans::No(None) }).await;
    let text_only = {
        let mut doc: Value = serde_json::from_str(&text_doc("notes4", |_| {})).unwrap();
        doc["variables"] = json!([]);
        doc["sections"] = json!([{"title": null, "collapsed": false, "panels": [text_panel(1, 0, &["# Only text"])]}]);
        serde_json::to_string(&doc).unwrap()
    };
    let body = text_only.clone();
    let fill_text = move |root: &std::path::Path| {
        fill(root);
        std::fs::write(
            root.join("ops/notes4.json"),
            hune_core::document::validate(body.as_bytes(), None).unwrap().bytes(),
        )
        .unwrap();
    };
    let h = Harness::start_with(test_timing(), fill_text, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    let t = token("user:bob");
    let r = call(h.addr, "GET", "/api/dashboards/notes4", &[(HEADER, t.as_str())], b"").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["document"]["sections"][0]["panels"][0]["type"], "text");
    assert_eq!(r.json()["document"]["sections"][0]["panels"][0]["options"]["content"], json!(["# Only text"]));
    // The PDP got no question about a source: a text panel needs no source access. The page has no write
    // flag for this subject (the answer of the PDP to `dashboard.write` is no).
    let asked: Vec<String> = pdp.asked().iter().map(|q| q.action.clone()).collect();
    assert!(asked.iter().all(|a| a != "source.query"), "{asked:?}");
    assert!(asked.iter().any(|a| a == "dashboard.read"), "{asked:?}");
    assert_eq!(r.json()["access"]["write"], false, "{}", r.text());
}

#[tokio::test]
async fn the_logs_of_the_run_hold_no_line_of_the_content() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let marked = text_doc("notes5", |d| {
        text_of(d)["options"]["content"] = json!([MARKER, format!("[x](https://wiki.example/{MARKER})")]);
        text_of(d)["description"] = json!("a description");
    });
    let r = create(&h, &marked).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let e = etag_of(&r);
    assert_eq!(get(h.addr, "/api/dashboards/notes5").await.status, 200);
    // A save, a refused save (the marker is in a line with a line end), a query and a preview of the panel.
    let changed = text_doc("notes5", |d| text_of(d)["options"]["content"] = json!([MARKER, MARKER]));
    assert_eq!(put(&h, "notes5", &e, &changed, &key()).await.status, 200);
    let refused = text_doc("notes5", |d| text_of(d)["options"]["content"] = json!([format!("{MARKER}\n{MARKER}")]));
    let r = create(&h, &refused).await;
    assert_eq!(r.status, 422, "{}", r.text());
    let r = call(h.addr, "POST", "/api/dashboards/notes5/panels/99/query", &[JSON], &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 409, "the etag changed with the save: {}", r.text());
    let e2 = etag_of(&get(h.addr, "/api/dashboards/notes5").await);
    let r = call(h.addr, "POST", "/api/dashboards/notes5/panels/99/query", &[JSON], &panel_body(&e2, json!({}))).await;
    assert_eq!(r.status, 400, "{}", r.text());
    let draft = text_panel(7, 0, &[MARKER]);
    let r = post_preview(&h, "prod", &preview_body(json!({"panel": draft}))).await;
    assert_eq!(r.status, 400, "{}", r.text());
    let lines = h.logger.memory_lines();
    assert!(lines.len() > 5, "the run has log lines");
    assert!(lines.iter().any(|l| l.contains("notes5")), "the log names the dashboard");
    for l in &lines {
        assert!(!l.contains(MARKER), "the content is in a log line: {l}");
    }
}
