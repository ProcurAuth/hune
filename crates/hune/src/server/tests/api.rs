// SPDX-License-Identifier: MIT OR Apache-2.0
//! The API cases of 16.8 that need the store and no ClickHouse. Each test name starts with its case id.

use super::harness::*;
use crate::server::limits::Timing;
use crate::web_assets::BUILD_HASH;
use hune_core::document::folder_file_bytes;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub(super) const NODES: &str = include_str!("../../../../../tests/fixtures/dashboards/nodes.json");
pub(super) const JSON: (&str, &str) = ("Content-Type", "application/json");
pub(super) const ZERO_ETAG: &str = "\"0000000000000000000000000000000000000000000000000000000000000000\"";

static COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn key() -> String {
    format!("0194b3c6-6f1e-7d2a-9b4c-{:012x}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// The fixture dashboard with another uid and title.
pub(super) fn doc_text(uid: &str, title: &str) -> String {
    NODES
        .replace("\"uid\": \"nodes\"", &format!("\"uid\": \"{uid}\""))
        .replace("\"title\": \"Nodes\"", &format!("\"title\": \"{title}\""))
}

pub(super) fn put_body(doc: &str, message: &str) -> Vec<u8> {
    format!("{{\"message\":{},\"document\":{doc}}}", Value::String(message.to_owned())).into_bytes()
}

pub(super) fn post_body(folder: &str, doc: &str) -> Vec<u8> {
    format!("{{\"folder\":\"{folder}\",\"document\":{doc}}}").into_bytes()
}

pub(super) fn fill(root: &Path) {
    for (folder, title) in [("transit", "Transit desk"), ("ops", "Operations")] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("_folder.json"), folder_file_bytes(title)).unwrap();
    }
    std::fs::write(root.join("transit/nodes.json"), NODES).unwrap();
}

async fn start() -> Harness {
    Harness::start_with(Timing { drain: Duration::from_millis(50), ..test_timing() }, fill, |_| {}).await
}

pub(super) fn etag_of(r: &Resp) -> String {
    r.header("etag").expect("ETag header").trim_matches('"').to_owned()
}

pub(super) async fn put(h: &Harness, uid: &str, if_match: &str, doc: &str, id: &str) -> Resp {
    let ifm = format!("\"{if_match}\"");
    call(
        h.addr,
        "PUT",
        &format!("/api/dashboards/{uid}"),
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", id)],
        &put_body(doc, "m"),
    )
    .await
}

pub(super) async fn current(h: &Harness, uid: &str) -> Resp {
    get(h.addr, &format!("/api/dashboards/{uid}")).await
}

#[tokio::test]
async fn a01_list_shows_the_dashboards_with_their_store() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let r = get(h.addr, "/api/dashboards").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("cache-control"), Some("no-store"));
    let j = r.json();
    assert!(j["next"].is_null());
    let items = j["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["uid"], "nodes");
    assert_eq!(items[0]["state"], "valid");
    assert_eq!(items[0]["folder"], "transit");
    assert_eq!(items[0]["title"], "Nodes");
    assert_eq!(items[0]["tags"], json!(["chain"]));
    assert_eq!(items[0]["access"]["write"], true);
    // The store of the dashboard (5.42): the primary store, so it is not read-only and it has no warning.
    assert_eq!((items[0]["read_only"].as_bool(), items[0]["store"].as_str()), (Some(false), Some("main")));
    assert!(items[0].get("stale").is_none());
    assert!(items[0].get("errors").is_none());
    let etag = items[0]["etag"].as_str().unwrap();
    assert_eq!(etag.len(), 64);
    assert_eq!(items[0]["version"], 0);
    assert!(items[0]["updated_at"].as_str().unwrap().ends_with('Z'));
    // The filters and the search.
    let r = get(h.addr, "/api/dashboards?folder=ops").await;
    assert_eq!(r.json()["items"], json!([]));
    let r = get(h.addr, "/api/dashboards?folder=nothing").await;
    assert_eq!(r.json()["items"], json!([]));
    let r = get(h.addr, "/api/dashboards?q=NODE").await;
    assert_eq!(r.json()["items"].as_array().unwrap().len(), 1);
    let r = get(h.addr, "/api/dashboards?q=ZZZ").await;
    assert_eq!(r.json()["items"], json!([]));
    for bad in ["q=", "limit=0", "limit=501", "limit=x", "after=nonsense", "after=", "extra=1"] {
        let r = get(h.addr, &format!("/api/dashboards?{bad}")).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
    }
}

#[tokio::test]
async fn a01_list_pagination_follows_the_cursor() {
    let h = start().await;
    for (uid, title) in [("aaa", "Alpha"), ("bbb", "beta"), ("ccc", "Gamma")] {
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards",
            &[JSON, ("Idempotency-Key", &key())],
            &post_body("ops", &doc_text(uid, title)),
        )
        .await;
        assert_eq!(r.status, 201, "{}", r.text());
    }
    let mut seen = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..6 {
        let path = match &after {
            None => "/api/dashboards?limit=1".to_owned(),
            Some(a) => format!("/api/dashboards?limit=1&after={a}"),
        };
        let j = get(h.addr, &path).await.json();
        for i in j["items"].as_array().unwrap() {
            seen.push(i["uid"].as_str().unwrap().to_owned());
        }
        match j["next"].as_str() {
            Some(n) => after = Some(n.to_owned()),
            None => break,
        }
    }
    // Folder `ops` first, then `transit`; in a folder, the title in lower case.
    assert_eq!(seen, ["aaa", "bbb", "ccc", "nodes"]);
    assert_eq!(get(h.addr, "/api/dashboards?limit=4").await.json()["next"], Value::Null);
    assert!(get(h.addr, "/api/dashboards?limit=3").await.json()["next"].is_string());
}

#[tokio::test]
async fn a02_get_and_download() {
    let h = start().await;
    let dl = get(h.addr, "/api/dashboards/nodes?download=true").await;
    assert_eq!(dl.status, 200);
    assert_eq!(dl.header("content-type"), Some("application/json"));
    assert_eq!(dl.header("content-disposition"), Some("attachment; filename=\"nodes.json\""));
    assert_eq!(dl.body, NODES.as_bytes());
    let etag = etag_of(&dl);
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(&dl.body).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, etag);
    let r = current(&h, "nodes").await;
    assert_eq!(r.status, 200);
    assert_eq!(etag_of(&r), etag);
    let j = r.json();
    assert_eq!(j["uid"], "nodes");
    assert_eq!(j["folder"], "transit");
    assert_eq!(j["etag"], etag);
    assert_eq!(j["version"], 0);
    assert_eq!(j["access"]["write"], true);
    assert_eq!(j["document"]["uid"], "nodes");
    assert_eq!(j["document"]["format_version"], 3);
    // The names of the saved queries (6.14, 7.19): a panel with a query has a key, a text panel has none.
    assert_eq!(
        j["names"],
        json!({
            "panels": { "1": [["node"]], "2": [["account", "env", "node"]] },
            "variables": { "node": [] },
            "annotations": []
        })
    );
    assert!(dl.text().find("\"names\"").is_none(), "download=true has no names");
    // The member order of the document is the canonical order.
    let text = r.text();
    let text = &text[text.find("\"document\":").unwrap()..];
    let pos = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
    assert!(pos("format_version") < pos("uid") && pos("uid") < pos("title") && pos("title") < pos("variables"));
    // If-None-Match with the current etag: 304 with no body.
    let r = call(h.addr, "GET", "/api/dashboards/nodes", &[("If-None-Match", &format!("\"{etag}\""))], b"").await;
    assert_eq!(r.status, 304);
    assert!(r.body.is_empty());
    assert_eq!(etag_of(&r), etag);
    let r = call(h.addr, "GET", "/api/dashboards/nodes", &[("If-None-Match", ZERO_ETAG)], b"").await;
    assert_eq!(r.status, 200);
    for bad in ["*", "W/\"abc\"", "abc"] {
        let r = call(h.addr, "GET", "/api/dashboards/nodes", &[("If-None-Match", bad)], b"").await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
    }
    let r = get(h.addr, "/api/dashboards/nodes?download=maybe").await;
    assert_eq!(r.status, 400);
}

#[tokio::test]
async fn a03_unknown_uid_is_404() {
    let h = start().await;
    for path in [
        "/api/dashboards/unknown",
        "/api/dashboards/Bad_Uid",
        "/api/dashboards/unknown/history/1",
        "/api/dashboards/unknown/history",
    ] {
        let r = get(h.addr, path).await;
        assert_eq!((r.status, r.code().as_str()), (404, "not_found"), "{path}");
    }
}

#[tokio::test]
async fn a04_create_and_exists() {
    let h = start().await;
    let body = post_body("transit", &doc_text("created", "Created"));
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert_eq!(r.header("location"), Some("api/dashboards/created"));
    let j = r.json();
    assert_eq!(j["uid"], "created");
    assert_eq!(j["folder"], "transit");
    assert_eq!(j["version"], 1);
    assert_eq!(j["noop"], false);
    assert_eq!(j["warnings"], json!([]));
    assert_eq!(j["saved_by"], "-");
    assert!(j["saved_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(j["etag"], etag_of(&r));
    let got = current(&h, "created").await;
    assert_eq!(got.status, 200);
    assert_eq!(etag_of(&got), etag_of(&r));
    assert_eq!(got.json()["version"], 1);
    // The uid exists: 409. Also with a new key.
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    // A folder that does not exist: 404. A bad folder id: 400 with the pointer.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("nothing", &doc_text("other", "O")),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("Bad_Folder", &doc_text("other", "O")),
    )
    .await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/folder");
    // The headers: Idempotency-Key is necessary, and must be a UUID.
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON], &post_body("transit", &doc_text("o2", "O"))).await;
    assert_eq!((r.status, r.code().as_str()), (428, "precondition_required"));
    assert_eq!(r.json()["error"]["header"], "Idempotency-Key");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", "not-a-uuid")],
        &post_body("transit", &doc_text("o2", "O")),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    // The body is closed: an unknown member, an absent member and a wrong type give pointers.
    let r =
        call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], br#"{"folder":1,"extra":true}"#)
            .await;
    assert_eq!(r.status, 400);
    let errors = r.json()["error"]["errors"].clone();
    let pairs: Vec<(String, String)> = errors
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["pointer"].as_str().unwrap().into(), e["code"].as_str().unwrap().into()))
        .collect();
    assert!(pairs.contains(&("/folder".into(), "wrong_type".into())), "{pairs:?}");
    assert!(pairs.contains(&("/document".into(), "missing_member".into())), "{pairs:?}");
    assert!(pairs.contains(&("/extra".into(), "unknown_member".into())), "{pairs:?}");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        br#"{"folder":"transit","folder":"ops","document":{}}"#,
    )
    .await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error"]["errors"][0]["code"], "duplicate_member");
    // The log line holds no SQL and no body.
    let lines = h.logger.memory_lines();
    let changed = lines.iter().find(|l| l.contains("\"dashboard.changed\"")).expect("dashboard.changed");
    let v: Value = serde_json::from_str(changed).unwrap();
    assert_eq!(v["action"], "create");
    assert_eq!(v["uid"], "created");
    assert_eq!(v["version"], 1);
    assert_eq!(v["subject"], "-");
    assert!(v["etag_before"].is_null());
    assert_eq!(v["etag_after"], etag_of(&got));
}

#[tokio::test]
async fn a04_validate_only_writes_nothing() {
    let h = start().await;
    let doc = doc_text("probe", "Probe");
    let r = call(h.addr, "POST", "/api/dashboards?validate_only=true", &[JSON], &post_body("transit", &doc)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["valid"], true);
    assert_eq!(j["canonical"], doc);
    assert_eq!(j["etag"].as_str().unwrap().len(), 64);
    assert_eq!(j["warnings"], json!([]));
    assert_eq!(get(h.addr, "/api/dashboards/probe").await.status, 404);
    // A document that is not valid gives 422.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards?validate_only=true",
        &[JSON],
        &post_body("transit", "{\"format_version\":3}"),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    // validate_only on a save needs neither If-Match nor Idempotency-Key.
    let r = call(h.addr, "PUT", "/api/dashboards/nodes?validate_only=true", &[JSON], &put_body(NODES, "")).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["valid"], true);
}

#[tokio::test]
async fn a05_save_with_if_match() {
    let h = start().await;
    let before = current(&h, "nodes").await;
    let e0 = etag_of(&before);
    let new_doc = doc_text("nodes", "Nodes v2");
    let r = put(&h, "nodes", &e0, &new_doc, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_ne!(j["etag"], e0);
    assert_eq!(j["noop"], false);
    assert_eq!(j["version"], 2, "the volume writes an external version for the file that came from outside");
    assert_eq!(etag_of(&r), j["etag"].as_str().unwrap());
    let after = current(&h, "nodes").await;
    assert_eq!(after.json()["document"]["title"], "Nodes v2");
    assert_eq!(etag_of(&after), j["etag"].as_str().unwrap());
    // The history holds the versions.
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"][0]["kind"], "save");
    assert_eq!(hist["items"][0]["version"], 2);
    assert_eq!(hist["items"][0]["message"], "m");
    assert_eq!(hist["items"][0]["etag"], j["etag"]);
    // A save with the same document is a noop: no new version.
    let again = put(&h, "nodes", etag_of(&after).as_str(), &new_doc, &key()).await;
    assert_eq!(again.status, 200);
    assert_eq!(again.json()["noop"], true);
    assert_eq!(again.json()["version"], 2);
    assert_eq!(again.json()["etag"], j["etag"]);
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"].as_array().unwrap().len(), 2);
    // The log line has no document body.
    let lines = h.logger.memory_lines();
    // The volume can log an external change of its own for the file of the setup, before or after the save. The
    // time of that line depends on the scan, so only the lines of the save count here.
    let changed: Vec<&String> =
        lines.iter().filter(|l| l.contains("\"dashboard.changed\"") && l.contains("\"action\":\"save\"")).collect();
    assert_eq!(changed.len(), 1, "{lines:?}");
    let v: Value = serde_json::from_str(changed[0]).unwrap();
    assert_eq!(v["action"], "save");
    assert_eq!(v["etag_before"], e0);
    assert_eq!(v["etag_after"], j["etag"]);
    assert!(v["bytes"].as_u64().unwrap() > 1000);
    assert!(!changed[0].contains("SELECT"));
}

#[tokio::test]
async fn a06_save_without_if_match_is_refused() {
    let h = start().await;
    let before = etag_of(&current(&h, "nodes").await);
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[JSON, ("Idempotency-Key", &key())],
        &put_body(&doc_text("nodes", "X"), ""),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (428, "precondition_required"));
    assert_eq!(r.json()["error"]["header"], "If-Match");
    for bad in ["*", &format!("W/\"{before}\""), &before] {
        let r = call(
            h.addr,
            "PUT",
            "/api/dashboards/nodes",
            &[JSON, ("Idempotency-Key", &key()), ("If-Match", bad)],
            &put_body(&doc_text("nodes", "X"), ""),
        )
        .await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
    }
    let r = call(h.addr, "DELETE", "/api/dashboards/nodes", &[("Idempotency-Key", &key())], b"").await;
    assert_eq!((r.status, r.code().as_str()), (428, "precondition_required"));
    assert_eq!(etag_of(&current(&h, "nodes").await), before);
}

#[tokio::test]
async fn a07_old_if_match_gives_409_with_the_current_state() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r1 = put(&h, "nodes", &e0, &doc_text("nodes", "Two"), &key()).await;
    assert_eq!(r1.status, 200);
    let e1 = r1.json()["etag"].as_str().unwrap().to_owned();
    let r2 = put(&h, "nodes", &e0, &doc_text("nodes", "Three"), &key()).await;
    assert_eq!((r2.status, r2.code().as_str()), (409, "conflict"));
    let err = r2.json()["error"].clone();
    assert_eq!(err["current_etag"], e1);
    assert_eq!(err["current_version"], 2);
    assert_eq!(err["current_folder"], "transit");
    assert_eq!(err["saved_by"], "-");
    assert!(err["saved_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(err["retryable"], false);
    assert_eq!(current(&h, "nodes").await.json()["document"]["title"], "Two");
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_store_conflicts_total 1\n"), "{text}");
    assert!(text.contains("hune_store_operations_total{backend=\"volume\",op=\"save\",status=\"conflict\"} 1\n"));
}

#[tokio::test]
async fn a08_repeated_idempotency_key_replays_the_answer() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let id = key();
    let doc = doc_text("nodes", "Replay");
    let first = put(&h, "nodes", &e0, &doc, &id).await;
    assert_eq!(first.status, 200);
    assert!(first.header("idempotent-replayed").is_none());
    let second = put(&h, "nodes", &e0, &doc, &id).await;
    assert_eq!(second.status, 200, "{}", second.text());
    assert_eq!(second.header("idempotent-replayed"), Some("true"));
    assert_eq!(second.json(), first.json());
    assert_eq!(etag_of(&second), etag_of(&first));
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"].as_array().unwrap().len(), 2, "one external version and one save");
    // A repeated create gives the first answer with 201 and the same Location.
    let cid = key();
    let body = post_body("ops", &doc_text("replayed", "R"));
    let c1 = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &cid)], &body).await;
    let c2 = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &cid)], &body).await;
    assert_eq!((c1.status, c2.status), (201, 201));
    assert_eq!(c2.header("idempotent-replayed"), Some("true"));
    assert_eq!(c2.header("location"), Some("api/dashboards/replayed"));
    assert_eq!(c1.json()["saved_at"], c2.json()["saved_at"]);
}

#[tokio::test]
async fn a09_used_key_with_another_body_is_refused() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let id = key();
    let first = put(&h, "nodes", &e0, &doc_text("nodes", "One"), &id).await;
    assert_eq!(first.status, 200);
    let e1 = etag_of(&first);
    let second = put(&h, "nodes", &e0, &doc_text("nodes", "Other"), &id).await;
    assert_eq!((second.status, second.code().as_str()), (422, "idempotency_key_reused"));
    assert_eq!(etag_of(&current(&h, "nodes").await), e1);
}

#[tokio::test]
async fn a10_uid_that_differs_from_the_path_is_refused() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r = put(&h, "nodes", &e0, &doc_text("another", "X"), &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    let errors = r.json()["error"]["errors"].clone();
    assert_eq!(errors[0]["pointer"], "/uid");
    assert_eq!(errors[0]["code"], "uid_mismatch");
    assert_eq!(etag_of(&current(&h, "nodes").await), e0);
}

#[tokio::test]
async fn a11_a_document_with_three_errors_lists_them_all() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let bad = NODES
        .replace("\"title\": \"Nodes\"", "\"title\": \"\"")
        .replace("\"timezone\": \"browser\"", "\"timezone\": \"mars\"")
        .replace("\"cursor_sync\": \"crosshair\"", "\"cursor_sync\": \"nope\"");
    let r = put(&h, "nodes", &e0, &bad, &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    let errors = r.json()["error"]["errors"].clone();
    let pointers: Vec<&str> = errors.as_array().unwrap().iter().map(|e| e["pointer"].as_str().unwrap()).collect();
    assert_eq!(pointers.len(), 3, "{pointers:?}");
    for p in ["/title", "/time/timezone", "/cursor_sync"] {
        assert!(pointers.contains(&p), "{p} in {pointers:?}");
    }
    assert_eq!(etag_of(&current(&h, "nodes").await), e0);
}

#[tokio::test]
async fn a12_source_that_the_configuration_does_not_declare_is_refused() {
    // The SQL checks of 7.18 items 1 to 8 come with the query engine. This test covers item 9.
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let bad = NODES.replace("\"source\": \"prod\"", "\"source\": \"unknown\"");
    let r = put(&h, "nodes", &e0, &bad, &key()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"));
    let errors = r.json()["error"]["errors"].clone();
    let first = &errors[0];
    assert_eq!(first["code"], "bad_reference");
    assert!(first["pointer"].as_str().unwrap().ends_with("/source"));
    assert!(first["line"].is_null() && first["column"].is_null());
    // The same check runs for a create and for validate_only.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards?validate_only=true",
        &[JSON],
        &post_body("transit", &bad.replace("\"nodes\"", "\"zzz\"")),
    )
    .await;
    assert_eq!(r.code(), "invalid_document");
}

#[tokio::test]
async fn a13_delete_writes_a_tombstone() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r = call(
        h.addr,
        "DELETE",
        "/api/dashboards/nodes",
        &[("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        b"",
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["uid"], "nodes");
    assert_eq!(j["deleted"], true);
    assert_eq!(j["version"], 2, "the external version 1 and the tombstone 2");
    assert_eq!(j["saved_by"], "-");
    assert_eq!(get(h.addr, "/api/dashboards/nodes").await.status, 404);
    let list = get(h.addr, "/api/dashboards").await.json();
    assert_eq!(list["items"], json!([]));
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"][0]["kind"], "delete");
    assert!(hist["items"][0]["etag"].is_null());
    assert!(hist["items"][0]["size"].is_null());
    // A repeated delete answers as for an unknown uid.
    let again = call(
        h.addr,
        "DELETE",
        "/api/dashboards/nodes",
        &[("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        b"",
    )
    .await;
    assert_eq!((again.status, again.code().as_str()), (404, "not_found"));
    // A create of the uid gives already_exists: only `hune restore` brings it back.
    let c = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", NODES))
        .await;
    assert_eq!((c.status, c.code().as_str()), (409, "already_exists"));
    // A tombstone has no document and no bytes.
    let v = get(h.addr, "/api/dashboards/nodes/history/2").await;
    assert_eq!(v.status, 200);
    assert!(v.json()["document"].is_null());
    let d = get(h.addr, "/api/dashboards/nodes/history/2?download=true").await;
    assert_eq!((d.status, d.code().as_str()), (404, "not_found"));
    let log = h.logger.memory_lines().into_iter().find(|l| l.contains("\"action\":\"delete\"")).unwrap();
    let v: Value = serde_json::from_str(&log).unwrap();
    assert_eq!(v["event"], "dashboard.changed");
    assert!(v["etag_after"].is_null());
}

#[tokio::test]
async fn a14_history_lists_versions_newest_first() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r1 = put(&h, "nodes", &e0, &doc_text("nodes", "Two"), &key()).await;
    let e1 = etag_of(&r1);
    let r2 = put(&h, "nodes", &e1, &doc_text("nodes", "Three"), &key()).await;
    assert_eq!(r2.status, 200);
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    let items = hist["items"].as_array().unwrap();
    let versions: Vec<u64> = items.iter().map(|i| i["version"].as_u64().unwrap()).collect();
    assert_eq!(versions, [3, 2, 1]);
    assert_eq!(items[2]["kind"], "external");
    assert_eq!(items[2]["etag"], e0);
    assert!(items[0]["size"].as_u64().unwrap() > 0);
    assert_eq!(items[0]["folder"], "transit");
    assert!(hist["next"].is_null());
    // Pages of one item, with the cursor of 6.24.
    let p1 = get(h.addr, "/api/dashboards/nodes/history?limit=1").await.json();
    assert_eq!(p1["items"][0]["version"], 3);
    assert_eq!(p1["next"], "djMD".replace("djMD", &crate::server::handlers::history_cursor_for_test(3)));
    let p2 = get(h.addr, &format!("/api/dashboards/nodes/history?limit=1&after={}", p1["next"].as_str().unwrap()))
        .await
        .json();
    assert_eq!(p2["items"][0]["version"], 2);
    let p3 = get(h.addr, &format!("/api/dashboards/nodes/history?limit=1&after={}", p2["next"].as_str().unwrap()))
        .await
        .json();
    assert_eq!(p3["items"][0]["version"], 1);
    assert!(p3["next"].is_null());
    for bad in ["limit=101", "limit=0", "after=zzz"] {
        assert_eq!(get(h.addr, &format!("/api/dashboards/nodes/history?{bad}")).await.status, 400, "{bad}");
    }
    // One version, with its bytes.
    let v2 = get(h.addr, "/api/dashboards/nodes/history/2").await;
    assert_eq!(v2.status, 200);
    assert_eq!(v2.json()["document"]["title"], "Two");
    assert_eq!(v2.json()["kind"], "save");
    assert_eq!(etag_of(&v2), etag_of(&r1));
    let dl = get(h.addr, "/api/dashboards/nodes/history/2?download=true").await;
    assert_eq!(dl.status, 200);
    assert_eq!(dl.header("content-disposition"), Some("attachment; filename=\"nodes-v2.json\""));
    assert_eq!(dl.body, doc_text("nodes", "Two").as_bytes());
    for (path, status) in [
        ("/api/dashboards/nodes/history/9", 404),
        ("/api/dashboards/nodes/history/0", 400),
        ("/api/dashboards/nodes/history/x", 400),
        ("/api/dashboards/nodes/history/01", 400),
    ] {
        assert_eq!(get(h.addr, path).await.status, status, "{path}");
    }
}

#[tokio::test]
async fn a15_restore_makes_a_new_version() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r1 = put(&h, "nodes", &e0, &doc_text("nodes", "Two"), &key()).await;
    let e1 = etag_of(&r1);
    // Version 1 is the external version with the original bytes.
    let body = br#"{"version":1,"message":"Back"}"#;
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e1}\"")), ("Idempotency-Key", &key())],
        body,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["etag"], e0);
    assert_eq!(j["version"], 3);
    assert_eq!(j["noop"], false);
    let now = get(h.addr, "/api/dashboards/nodes?download=true").await;
    assert_eq!(now.body, NODES.as_bytes());
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"][0]["kind"], "restore");
    assert_eq!(hist["items"][0]["message"], "Back");
    // A restore of the current bytes writes nothing.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["noop"], true);
    // An unknown version: 404. A stale If-Match: 409. A bad body: 400.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"version":99}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e1}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (409, "conflict"));
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"version":0}"#,
    )
    .await;
    assert_eq!(r.status, 400);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/unknown/restore",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!(r.status, 404);
}

#[tokio::test]
async fn a16_move_changes_the_folder() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/move",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"folder":"ops","message":"Move"}"#,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["uid"], "nodes");
    assert_eq!(j["folder"], "ops");
    assert_eq!(j["etag"], e0, "a move does not change the etag");
    assert_eq!(j["noop"], false);
    let mut members: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
    members.sort_unstable();
    assert_eq!(members, ["etag", "folder", "noop", "saved_at", "saved_by", "uid", "version"], "6.19: no warnings");
    let got = current(&h, "nodes").await;
    assert_eq!(got.json()["folder"], "ops");
    assert_eq!(etag_of(&got), e0);
    let hist = get(h.addr, "/api/dashboards/nodes/history").await.json();
    assert_eq!(hist["items"][0]["kind"], "move");
    assert_eq!(hist["items"][0]["folder"], "ops");
    // A move to the current folder is a noop. An unknown folder is refused.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/move",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"folder":"ops"}"#,
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["noop"], true);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/move",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"folder":"nothing"}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/move",
        &[JSON, ("If-Match", &format!("\"{e0}\"")), ("Idempotency-Key", &key())],
        br#"{"folder":"Bad"}"#,
    )
    .await;
    assert_eq!(r.status, 400);
    let log = h.logger.memory_lines().into_iter().find(|l| l.contains("\"action\":\"move\"")).unwrap();
    let v: Value = serde_json::from_str(&log).unwrap();
    assert_eq!(v["from_folder"], "transit");
    assert_eq!(v["folder"], "ops");
}

#[tokio::test]
async fn a17_folders() {
    let h = start().await;
    let r = get(h.addr, "/api/folders").await;
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(j["access"]["create"], true);
    assert_eq!(
        j["items"][0],
        json!({"id": "ops", "title": "Operations", "access": {"write": true}, "read_only": false, "store": "main"})
    );
    assert_eq!(j["items"][1]["id"], "transit");
    assert_eq!(j["items"][1]["title"], "Transit desk");
    // Create.
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"research","title":"Research"}"#).await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert_eq!(r.json(), json!({"id": "research", "title": "Research"}));
    assert!(r.header("location").is_none());
    let ids: Vec<String> = get(h.addr, "/api/folders").await.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["ops", "research", "transit"]);
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"research","title":"Again"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    for (body, pointer) in [
        (&br#"{"id":"Bad_Id","title":"T"}"#[..], "/id"),
        (br#"{"id":"x","title":""}"#, "/title"),
        (br#"{"id":"x","title":"a\u0001b"}"#, "/title"),
        (br#"{"id":"x"}"#, "/title"),
        (br#"{"id":"x","title":"T","more":1}"#, "/more"),
    ] {
        let r = call(h.addr, "POST", "/api/folders", &[JSON], body).await;
        assert_eq!(r.status, 400, "{}", String::from_utf8_lossy(body));
        assert_eq!(r.json()["error"]["errors"][0]["pointer"], pointer);
    }
    // Rename: the last rename wins, a rename to the same title works.
    let r = call(h.addr, "PUT", "/api/folders/research", &[JSON], br#"{"title":"R&D"}"#).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json(), json!({"id": "research", "title": "R&D"}));
    assert_eq!(call(h.addr, "PUT", "/api/folders/research", &[JSON], br#"{"title":"R&D"}"#).await.status, 200);
    let r = call(h.addr, "PUT", "/api/folders/nothing", &[JSON], br#"{"title":"X"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    let titles = get(h.addr, "/api/folders").await.json();
    assert_eq!(titles["items"][1]["title"], "R&D");
    // Delete an empty folder; a folder with a document is refused.
    let r = call(h.addr, "DELETE", "/api/folders/research", &[], b"").await;
    assert_eq!(r.status, 204);
    assert!(r.body.is_empty());
    let r = call(h.addr, "DELETE", "/api/folders/transit", &[], b"").await;
    assert_eq!((r.status, r.code().as_str()), (409, "folder_not_empty"));
    let r = call(h.addr, "DELETE", "/api/folders/research", &[], b"").await;
    assert_eq!(r.status, 404);
    let r = call(h.addr, "DELETE", "/api/folders/Bad_Id", &[], b"").await;
    assert_eq!(r.status, 404);
    let events: Vec<Value> = h
        .logger
        .memory_lines()
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .filter(|v: &Value| v["event"] == "folder.changed")
        .collect();
    let actions: Vec<&str> = events.iter().map(|e| e["action"].as_str().unwrap()).collect();
    assert_eq!(actions, ["create", "rename", "rename", "delete"]);
    assert_eq!(events[0]["folder"], "research");
    assert_eq!(events[0]["subject"], "-");
}

/// The pointers and the codes of the error list of an answer.
fn faults(r: &Resp) -> Vec<(String, String)> {
    r.json()["error"]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["pointer"].as_str().unwrap().to_owned(), e["code"].as_str().unwrap().to_owned()))
        .collect()
}

#[tokio::test]
async fn a17_a_body_with_two_faults_lists_both() {
    let h = start().await;
    let both = [("/id".to_owned(), "bad_format".to_owned()), ("/title".to_owned(), "out_of_range".to_owned())];
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"BAD ID","title":""}"#).await;
    assert_eq!(r.status, 400);
    assert_eq!(faults(&r), both);
    // The same rule for the other bodies: a format fault and a shape fault together.
    let e0 = etag_of(&current(&h, "nodes").await);
    let ifm = format!("\"{e0}\"");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/move",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        br#"{"folder":"BAD ID","message":"a\u0001b","more":1}"#,
    )
    .await;
    assert_eq!(r.status, 400);
    let list = faults(&r);
    for want in [("/folder", "bad_format"), ("/message", "bad_format"), ("/more", "unknown_member")] {
        assert!(list.contains(&(want.0.to_owned(), want.1.to_owned())), "{want:?} in {list:?}");
    }
    let r = call(h.addr, "PUT", "/api/folders/transit", &[JSON], br#"{"title":"a\u0001b","more":1}"#).await;
    let list = faults(&r);
    assert!(list.contains(&("/title".to_owned(), "bad_format".to_owned())), "{list:?}");
    assert!(list.contains(&("/more".to_owned(), "unknown_member".to_owned())), "{list:?}");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        br#"{"version":0,"message":"a\u0001b"}"#,
    )
    .await;
    let list = faults(&r);
    assert!(list.contains(&("/version".to_owned(), "out_of_range".to_owned())), "{list:?}");
    assert!(list.contains(&("/message".to_owned(), "bad_format".to_owned())), "{list:?}");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        br#"{"folder":"BAD ID","message":"a\u0001b"}"#,
    )
    .await;
    let list = faults(&r);
    for want in [("/folder", "bad_format"), ("/message", "bad_format"), ("/document", "missing_member")] {
        assert!(list.contains(&(want.0.to_owned(), want.1.to_owned())), "{want:?} in {list:?}");
    }
    // A duplicate member and a shape fault together.
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"x","id":"y","title":1}"#).await;
    let list = faults(&r);
    assert!(list.contains(&("/id".to_owned(), "duplicate_member".to_owned())), "{list:?}");
    assert!(list.contains(&("/title".to_owned(), "wrong_type".to_owned())), "{list:?}");
}

#[tokio::test]
async fn a23_sources_list_has_ids_and_labels_only() {
    let h = start().await;
    let r = get(h.addr, "/api/sources").await;
    assert_eq!(r.status, 200);
    assert_eq!(
        r.json(),
        json!({"items": [{"id": "prod", "label": "Production", "kind": "clickhouse", "state": "unknown"}]})
    );
    let text = r.text();
    for secret in ["127.0.0.1", "8123", "hune", "secret-password", "analytics"] {
        assert!(!text.contains(secret), "{secret}");
    }
}

#[tokio::test]
async fn a25_session_has_mode_subject_and_editing() {
    let h = start().await;
    let j = get(h.addr, "/api/session").await.json();
    assert_eq!(
        (j["mode"].as_str(), j["subject"].as_str(), j["editing"].as_bool()),
        (Some("open"), Some("-"), Some(true))
    );
}

#[tokio::test]
async fn a26_each_write_into_a_read_only_store_is_refused_and_each_read_route_of_it_works() {
    let (h, _d, _r) = super::stores::start_two().await;
    let uid = super::stores::LAB_UID;
    let list = get(h.addr, "/api/dashboards").await.json();
    let lab = list["items"].as_array().unwrap().iter().find(|i| i["uid"] == uid).unwrap().clone();
    let ifm = format!("\"{}\"", lab["etag"].as_str().unwrap());
    type WriteCase = (&'static str, String, Vec<(&'static str, String)>, Vec<u8>);
    let writes: Vec<WriteCase> = vec![
        (
            "PUT",
            format!("/api/dashboards/{uid}"),
            vec![("Idempotency-Key", key()), ("If-Match", ifm.clone())],
            put_body(&doc_text(uid, "Changed"), ""),
        ),
        (
            "DELETE",
            format!("/api/dashboards/{uid}"),
            vec![("Idempotency-Key", key()), ("If-Match", ifm.clone())],
            vec![],
        ),
        (
            "POST",
            format!("/api/dashboards/{uid}/restore"),
            vec![("Idempotency-Key", key()), ("If-Match", ifm.clone())],
            br#"{"version":1}"#.to_vec(),
        ),
        (
            "POST",
            format!("/api/dashboards/{uid}/move"),
            vec![("Idempotency-Key", key()), ("If-Match", ifm.clone())],
            br#"{"folder":"ops"}"#.to_vec(),
        ),
        ("PUT", format!("/api/folders/{}", super::stores::LAB_FOLDER), vec![], br#"{"title":"X"}"#.to_vec()),
        ("DELETE", format!("/api/folders/{}", super::stores::LAB_FOLDER), vec![], vec![]),
    ];
    for (m, p, hs, body) in &writes {
        let mut headers: Vec<(&str, &str)> = vec![JSON];
        for (k, v) in hs {
            headers.push((k, v));
        }
        let r = call(h.addr, m, p, &headers, body).await;
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{m} {p}");
    }
    for path in ["/api/session", "/api/folders", "/api/dashboards", &format!("/api/dashboards/{uid}"), "/api/sources"] {
        assert_eq!(get(h.addr, path).await.status, 200, "{path}");
    }
    let list = get(h.addr, "/api/dashboards").await.json();
    let after = list["items"].as_array().unwrap().iter().find(|i| i["uid"] == uid).unwrap().clone();
    assert_eq!(after["etag"], lab["etag"], "no write happened");
}

#[tokio::test]
async fn a27_write_guard_on_the_api() {
    let h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let ifm = format!("\"{e0}\"");
    let hs = |extra: &'static [(&'static str, &'static str)]| -> Vec<(String, String)> {
        let mut v = vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("If-Match".to_owned(), ifm.clone()),
            ("Idempotency-Key".to_owned(), key()),
        ];
        v.extend(extra.iter().map(|(k, x)| ((*k).to_owned(), (*x).to_owned())));
        v
    };
    let doc = doc_text("nodes", "Guarded");
    for extra in [&[("Sec-Fetch-Site", "cross-site")][..], &[("Origin", "https://evil.example")][..]] {
        let hv = hs(match extra {
            [("Sec-Fetch-Site", "cross-site")] => &[("Sec-Fetch-Site", "cross-site")],
            _ => &[("Origin", "https://evil.example")],
        });
        let refs: Vec<(&str, &str)> = hv.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let r = call(h.addr, "PUT", "/api/dashboards/nodes", &refs, &put_body(&doc, "")).await;
        assert_eq!((r.status, r.code().as_str()), (403, "cross_site_request"));
    }
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[("Content-Type", "text/plain"), ("If-Match", &ifm), ("Idempotency-Key", &key())],
        &put_body(&doc, ""),
    )
    .await;
    assert_eq!(r.status, 415);
    assert_eq!(etag_of(&current(&h, "nodes").await), e0);
    let r = call(h.addr, "OPTIONS", "/api/dashboards/nodes", &[("Origin", "https://evil.example")], b"").await;
    assert_eq!(r.status, 405);
    assert!(r.headers.iter().all(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-")));
}

#[tokio::test]
async fn a29_each_class_of_answer_has_the_security_headers() {
    let h = start().await;
    let file = crate::web_assets::FILES.iter().find(|f| f.path.ends_with(".js")).unwrap();
    let answers = [
        get(h.addr, "/").await,
        get(h.addr, &format!("/static/{BUILD_HASH}/{}", file.path)).await,
        get(h.addr, "/api/dashboards/nodes").await,
        get(h.addr, "/api/dashboards/nodes?download=true").await,
        get(h.addr, "/api/dashboards/nothing").await,
        get(h.addr, "/nothing").await,
        call(h.addr, "DELETE", "/api/session", &[], b"").await,
    ];
    for r in &answers {
        assert_eq!(r.header("content-security-policy"), Some(crate::server::headers::CSP));
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(r.header("x-frame-options"), Some("DENY"));
        assert_eq!(r.header("referrer-policy"), Some("same-origin"));
        assert_eq!(r.header("cross-origin-resource-policy"), Some("same-origin"));
        assert_eq!(r.header("cross-origin-opener-policy"), Some("same-origin"));
        assert!(r.header("x-request-id").is_some());
    }
}

#[tokio::test]
async fn a30_cache_headers() {
    let h = start().await;
    let file = crate::web_assets::FILES.iter().find(|f| f.path.ends_with(".css")).unwrap();
    let r = get(h.addr, &format!("/static/{BUILD_HASH}/{}", file.path)).await;
    assert_eq!(r.header("cache-control"), Some("private, max-age=31536000, immutable"));
    assert_eq!(get(h.addr, &format!("/static/ffffffffffffffff/{}", file.path)).await.status, 404);
    assert_eq!(get(h.addr, "/").await.header("cache-control"), Some("no-cache"));
    assert_eq!(get(h.addr, "/api/folders").await.header("cache-control"), Some("no-store"));
    assert_eq!(get(h.addr, "/api/nothing").await.header("cache-control"), Some("no-store"));
}

#[tokio::test]
async fn a31_base_path_prefixes_every_route() {
    let h = Harness::start_with(test_timing(), fill, |c| c.server.base_path = "/dashboards/".to_owned()).await;
    for (path, status) in [
        ("/dashboards/", 200),
        ("/dashboards/d/nodes", 200),
        ("/dashboards/api/session", 200),
        ("/dashboards/api/folders", 200),
        ("/dashboards/api/dashboards", 200),
        ("/dashboards/api/dashboards/nodes", 200),
        ("/dashboards/api/dashboards/nodes/history", 200),
        ("/dashboards/api/sources", 200),
        ("/api/session", 404),
        ("/dashboards", 404),
    ] {
        let r = get(h.addr, path).await;
        assert_eq!(r.status, status, "{path}");
    }
    let static_path = format!("/dashboards/static/{BUILD_HASH}/js/main.js");
    assert_eq!(get(h.addr, &static_path).await.status, 200);
    // The shell holds relative links only, and the base element names the prefix.
    let shell = get(h.addr, "/dashboards/").await.text();
    assert!(shell.contains("<base href=\"/dashboards/\">"));
    let shell = shell.replace("<base href=\"/dashboards/\">", "");
    for attr in ["href=\"", "src=\""] {
        for (i, _) in shell.match_indices(attr) {
            let rest = &shell[i + attr.len()..];
            let value = &rest[..rest.find('"').unwrap()];
            assert!(!value.starts_with('/') && !value.starts_with("http"), "absolute link {value}");
        }
    }
    // A write under the prefix works, and the Location is relative.
    let r = call(
        h.addr,
        "POST",
        "/dashboards/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &doc_text("under-prefix", "P")),
    )
    .await;
    assert_eq!(r.status, 201);
    assert_eq!(r.header("location"), Some("api/dashboards/under-prefix"));
}

#[tokio::test]
async fn a36_the_log_holds_no_secret_and_no_sql() {
    let mut h = start().await;
    let e0 = etag_of(&current(&h, "nodes").await);
    let r = put(&h, "nodes", &e0, &doc_text("nodes", "Logged"), &key()).await;
    assert_eq!(r.status, 200);
    let _ = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &doc_text("n1", "N")),
    )
    .await;
    let _ = call(h.addr, "POST", "/api/dashboards/nodes/panels/1/query", &[JSON], b"{}").await;
    assert_eq!(h.stop().await, 0);
    let lines = h.logger.memory_lines();
    assert!(!lines.is_empty());
    for line in &lines {
        let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert!(v["event"].is_string(), "{line}");
        for needle in ["secret-password", "SELECT DISTINCT node", "analytics.nodes", "ORDER BY node"] {
            assert!(!line.contains(needle), "{needle} in {line}");
        }
    }
    let all = lines.join("\n");
    assert!(!all.contains("Bearer"));
}

#[tokio::test]
async fn a35_sigterm_serves_during_the_drain_then_refuses() {
    let timing = Timing { drain: Duration::from_millis(600), ..test_timing() };
    let mut h = Harness::start_with(timing, fill, |_| {}).await;
    let addr = h.addr;
    let ops = h.ops;
    let running = h.running.take().unwrap();
    let state = running.state.clone();
    let mut rx = h.signals_rx.take().unwrap();
    let stopping = tokio::spawn(async move { running.stop("SIGTERM", &mut rx).await });
    for _ in 0..100 {
        if state.stopping() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // At once: /readyz is 503. During the drain, new connections are served as usual.
    assert_eq!(get(ops, "/readyz").await.status, 503);
    assert_eq!(get(addr, "/api/dashboards/nodes").await.status, 200);
    assert_eq!(get(addr, "/api/session").await.status, 200);
    // After the drain, new connections are refused.
    assert_eq!(stopping.await.unwrap(), 0);
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}

#[tokio::test]
async fn external_changes_reach_the_catalog_by_the_poll() {
    let h = start().await;
    assert_eq!(get(h.addr, "/api/dashboards/outside").await.status, 404);
    let root = std::fs::canonicalize(h.dir.path()).unwrap();
    std::fs::write(root.join("ops/outside.json"), doc_text("outside", "Outside")).unwrap();
    let mut found = false;
    for _ in 0..100 {
        if get(h.addr, "/api/dashboards/outside").await.status == 200 {
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(found, "the poller finds the file");
    let events = h.events();
    assert!(events.contains(&"dashboard.changed".to_owned()), "{events:?}");
    let line = h.logger.memory_lines().into_iter().find(|l| l.contains("\"action\":\"external\"")).unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert!(v["request_id"].is_null());
    assert_eq!(v["subject"], "-");
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_documents{state=\"valid\"} 2\n"), "{text}");
    assert!(text.contains("hune_store_up 1\n"));
}

// ---- A60: the parameter `tag` of 6.12 -----------------------------------------------------------

/// The fixture dashboard with another uid, title and list of tags (the canonical layout of the fixture).
pub(super) fn doc_tags(uid: &str, title: &str, tags: &[&str]) -> String {
    let list = if tags.is_empty() {
        "\"tags\": []".to_owned()
    } else {
        let items: Vec<String> = tags.iter().map(|t| format!("    {}", Value::String((*t).to_owned()))).collect();
        format!("\"tags\": [\n{}\n  ]", items.join(",\n"))
    };
    let base = doc_text(uid, title);
    assert!(base.contains("\"tags\": [\n    \"chain\"\n  ]"), "the fixture layout changed");
    base.replace("\"tags\": [\n    \"chain\"\n  ]", &list)
}

/// A harness with the fixture `nodes` (tag `chain`, folder `transit`) and these dashboards:
/// `alpha` (`ops`, tags `chain` and `infra`), `beta` (`ops`, no tag), `gamma` (`ops`, tags `Chain` and
/// `ch`), and `broken` (`ops`, invalid, its bytes hold the text of a tag).
async fn start_tags() -> Harness {
    Harness::start_with(
        Timing { drain: Duration::from_millis(50), ..test_timing() },
        |root| {
            fill(root);
            std::fs::write(root.join("ops/alpha.json"), doc_tags("alpha", "Alpha", &["chain", "infra"])).unwrap();
            std::fs::write(root.join("ops/beta.json"), doc_tags("beta", "Beta", &[])).unwrap();
            std::fs::write(root.join("ops/gamma.json"), doc_tags("gamma", "Gamma", &["Chain", "ch"])).unwrap();
        },
        |_| {},
    )
    .await
}

pub(super) fn uids(r: &Resp) -> Vec<String> {
    r.json()["items"].as_array().unwrap().iter().map(|i| i["uid"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn a60_a_tag_lists_only_the_dashboards_that_have_it() {
    let h = start_tags().await;
    // The order is folder, title, uid (6.24): `ops` before `transit`.
    let r = get(h.addr, "/api/dashboards?tag=chain").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(uids(&r), ["alpha", "nodes"]);
    assert_eq!(r.json()["items"][0]["tags"], json!(["chain", "infra"]));
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=infra").await), ["alpha"]);
    // Without the parameter, each entry shows (4 valid).
    assert_eq!(get(h.addr, "/api/dashboards").await.json()["items"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn a60_a_tag_that_no_dashboard_has_gives_an_empty_list() {
    let h = start_tags().await;
    for tag in ["other", "nothing", "infra%20", "%20chain"] {
        let r = get(h.addr, &format!("/api/dashboards?tag={tag}")).await;
        assert_eq!(r.status, 200, "{tag}");
        assert_eq!(r.json()["items"], json!([]), "{tag}");
        assert!(r.json()["next"].is_null(), "{tag}");
    }
}

#[tokio::test]
async fn a60_the_comparison_is_exact() {
    let h = start_tags().await;
    // `Chain` is not `chain`, and `ch` is not a part match of `chain`.
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=Chain").await), ["gamma"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=ch").await), ["gamma"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=CHAIN").await), Vec::<String>::new());
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=olana").await), Vec::<String>::new());
}

#[tokio::test]
async fn a60_a_tag_works_with_q_and_folder() {
    let h = start_tags().await;
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=chain&q=ALPH").await), ["alpha"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=chain&q=node").await), ["nodes"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=chain&q=beta").await), Vec::<String>::new());
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=chain&folder=transit").await), ["nodes"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=chain&folder=ops").await), ["alpha"]);
    assert_eq!(uids(&get(h.addr, "/api/dashboards?tag=infra&folder=transit").await), Vec::<String>::new());
    assert_eq!(uids(&get(h.addr, "/api/dashboards?folder=ops&tag=chain&q=a&limit=5").await), ["alpha"]);
}

#[tokio::test]
async fn a60_the_tag_filters_before_the_pagination() {
    let h = start_tags().await;
    // With `limit=1`, each page holds one item that has the tag, and the pages cover the same list.
    let mut seen = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..5 {
        let path = match &after {
            None => "/api/dashboards?tag=chain&limit=1".to_owned(),
            Some(a) => format!("/api/dashboards?tag=chain&limit=1&after={a}"),
        };
        let r = get(h.addr, &path).await;
        assert_eq!(r.status, 200, "{}", r.text());
        assert_eq!(uids(&r).len(), 1);
        seen.extend(uids(&r));
        match r.json()["next"].as_str() {
            Some(n) => after = Some(n.to_owned()),
            None => break,
        }
    }
    assert_eq!(seen, ["alpha", "nodes"]);
    // The last page has no `next`: the items without the tag do not make a next page.
    assert!(get(h.addr, "/api/dashboards?tag=chain&limit=2").await.json()["next"].is_null());
    assert!(get(h.addr, "/api/dashboards?tag=infra&limit=1").await.json()["next"].is_null());
}

#[tokio::test]
async fn a60_a_bad_tag_gives_400_and_the_limits_are_in_bytes() {
    let h = start_tags().await;
    let long64 = "t".repeat(64);
    let long65 = "t".repeat(65);
    // 21 characters of 3 bytes each are 63 bytes: valid. 22 characters are 66 bytes: refused.
    let wide21 = "%E2%82%AC".repeat(21);
    let wide22 = "%E2%82%AC".repeat(22);
    for ok in [long64.as_str(), "x", wide21.as_str()] {
        let r = get(h.addr, &format!("/api/dashboards?tag={ok}")).await;
        assert_eq!(r.status, 200, "{ok}: {}", r.text());
    }
    for bad in [long65.as_str(), "", wide22.as_str()] {
        let r = get(h.addr, &format!("/api/dashboards?tag={bad}")).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
        assert_eq!(r.json()["error"]["message"], "The query parameter tag must have 1 to 64 bytes.", "{bad}");
    }
    // A bare `tag` with no `=` is an empty value. A bad escape is refused as for each parameter.
    for bad in ["tag", "tag=%ZZ", "tag=%FF"] {
        let r = get(h.addr, &format!("/api/dashboards?{bad}")).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
    }
    // The route stays closed: a repeated `tag` and an unknown name give 400 (6.2).
    for bad in ["tag=chain&tag=infra", "tag=chain&tag=chain", "tag=chain&tags=chain", "Tag=chain"] {
        let r = get(h.addr, &format!("/api/dashboards?{bad}")).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{bad}");
    }
}

#[tokio::test]
async fn a60_the_other_routes_do_not_accept_tag() {
    let h = start_tags().await;
    for path in ["/api/folders?tag=chain", "/api/dashboards/nodes?tag=chain"] {
        let r = get(h.addr, path).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{path}");
    }
}
