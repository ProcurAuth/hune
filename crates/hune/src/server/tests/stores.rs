// SPDX-License-Identifier: MIT OR Apache-2.0
//! The primary store and the read-only stores (5.42 to 5.44, 6.10, 6.8, 11.35): the fields of the page, the refusal
//! of a write, the start rules and the warning of a file that is not valid after the start.

use super::api::{JSON, doc_text, fill, post_body, put_body};
use super::authzen_pdp::*;
use super::harness::*;
use super::same_refusal::{PAIRS, ask};
use crate::server::lifecycle;
use crate::server::startup;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(super) const LAB_UID: &str = "lab-one";
pub(super) const LAB_FOLDER: &str = "labops";

/// The folder file of a volume (5.10).
fn folder_file(title: &str) -> String {
    format!("{{\n  \"format_version\": 1,\n  \"title\": \"{title}\"\n}}\n")
}

/// A volume with the folder `labops` and the dashboard `lab-one`: the content of the read-only store `lab`.
fn lab_volume() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir_all(root.join(LAB_FOLDER)).unwrap();
    std::fs::write(root.join(LAB_FOLDER).join("_folder.json"), folder_file("Lab operations")).unwrap();
    std::fs::write(root.join(LAB_FOLDER).join(format!("{LAB_UID}.json")), doc_text(LAB_UID, "Lab one")).unwrap();
    (dir, root)
}

/// A server with the primary store `main` (the folders `transit` and `ops`, the dashboard `nodes`) and the read-only
/// store `lab`.
pub(super) async fn start_two() -> (Harness, tempfile::TempDir, PathBuf) {
    let (dir, root) = lab_volume();
    let r = root.clone();
    let h = Harness::start_with(test_timing(), fill, move |c| add_read_only_volume(c, &r)).await;
    (h, dir, root)
}

async fn get_json(h: &Harness, path: &str) -> Value {
    let r = super::harness::get(h.addr, path).await;
    assert_eq!(r.status, 200, "{path}: {}", r.text());
    r.json()
}

fn item<'a>(list: &'a Value, uid: &str) -> &'a Value {
    list["items"].as_array().unwrap().iter().find(|i| i["uid"] == uid).unwrap_or_else(|| panic!("{uid}"))
}

#[tokio::test]
async fn the_session_lists_the_stores_with_their_titles() {
    let (h, _d, _r) = start_two().await;
    let j = get_json(&h, "/api/session").await;
    assert_eq!(j["editing"], true);
    assert_eq!(
        j["stores"],
        json!([
            {"id": "main", "kind": "volume", "title": null, "primary": true, "read_only": false},
            {"id": "lab", "kind": "volume", "title": "Lab", "primary": false, "read_only": true},
        ])
    );
}

#[tokio::test]
async fn the_list_the_folders_and_the_read_tell_the_store_and_the_right_to_write() {
    let (h, _d, _r) = start_two().await;
    let list = get_json(&h, "/api/dashboards").await;
    let nodes = item(&list, "nodes");
    assert_eq!(
        (nodes["store"].as_str(), nodes["read_only"].as_bool(), nodes["access"]["write"].as_bool()),
        (Some("main"), Some(false), Some(true))
    );
    let lab = item(&list, LAB_UID);
    assert_eq!(
        (lab["store"].as_str(), lab["read_only"].as_bool(), lab["access"]["write"].as_bool()),
        (Some("lab"), Some(true), Some(false))
    );
    assert_eq!(lab["folder"], LAB_FOLDER);
    assert!(lab.get("stale").is_none());
    // The history is for the primary store only. The export is the same for both stores.
    assert_eq!(nodes["access"], json!({"write": true, "history": true, "export": true}));
    assert_eq!(lab["access"], json!({"write": false, "history": false, "export": true}));
    let folders = get_json(&h, "/api/folders").await;
    let f = |id: &str| folders["items"].as_array().unwrap().iter().find(|i| i["id"] == id).cloned().unwrap();
    assert_eq!(
        f("transit"),
        json!({"id": "transit", "title": "Transit desk", "access": {"write": true}, "read_only": false, "store": "main"})
    );
    assert_eq!(
        f(LAB_FOLDER),
        json!({"id": LAB_FOLDER, "title": "Lab operations", "access": {"write": false}, "read_only": true, "store": "lab"})
    );
    let one = get_json(&h, &format!("/api/dashboards/{LAB_UID}")).await;
    assert_eq!(
        (one["store"].as_str(), one["read_only"].as_bool(), one["access"]["write"].as_bool()),
        (Some("lab"), Some(true), Some(false))
    );
    assert_eq!(one["document"]["uid"], LAB_UID);
    let nodes = get_json(&h, "/api/dashboards/nodes").await;
    assert_eq!(
        (nodes["store"].as_str(), nodes["read_only"].as_bool(), nodes["access"]["write"].as_bool()),
        (Some("main"), Some(false), Some(true))
    );
    // The download of a dashboard of a read-only store works.
    let dl = super::harness::get(h.addr, &format!("/api/dashboards/{LAB_UID}?download=true")).await;
    assert_eq!(dl.status, 200);
    assert_eq!(dl.body, doc_text(LAB_UID, "Lab one").as_bytes());
}

/// A143: a dashboard of a read-only store has no history.
#[tokio::test]
async fn a143_a_read_only_dashboard_has_no_history() {
    let (h, _d, _r) = start_two().await;
    for path in [
        format!("/api/dashboards/{LAB_UID}/history"),
        format!("/api/dashboards/{LAB_UID}/history/1"),
        format!("/api/dashboards/{LAB_UID}/history/1?download=true"),
    ] {
        let r = super::harness::get(h.addr, &path).await;
        assert_eq!((r.status, r.code().as_str()), (404, "history_unavailable"), "{path}");
    }
    // The primary store keeps its history, and the same route works there.
    let r = super::harness::get(h.addr, "/api/dashboards/nodes/history").await;
    assert_eq!(r.status, 200);
    // A restore in a read-only store is a write into it: `read_only_store`.
    let list = get_json(&h, "/api/dashboards").await;
    let etag = item(&list, LAB_UID)["etag"].as_str().unwrap().to_owned();
    let r = call(
        h.addr,
        "POST",
        &format!("/api/dashboards/{LAB_UID}/restore"),
        &[JSON, ("If-Match", &format!("\"{etag}\"")), ("Idempotency-Key", "0194b3c6-6f1e-7d2a-9b4c-00000000a143")],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"));
}

#[tokio::test]
async fn each_write_into_a_read_only_store_gives_403_read_only_store_and_changes_nothing() {
    let (h, _d, root) = start_two().await;
    let before = get_json(&h, "/api/dashboards").await;
    let lab_etag = item(&before, LAB_UID)["etag"].as_str().unwrap().to_owned();
    let ifm = format!("\"{lab_etag}\"");
    let key = |n: u32| format!("0194b3c6-6f1e-7d2a-9b4c-00000000b{n:03}");
    let refused = |r: super::harness::Resp, what: &str| {
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{what}: {}", r.text());
    };
    let changed = put_body(&doc_text(LAB_UID, "Changed"), "m");
    // Save and the validation of a save.
    for path in [format!("/api/dashboards/{LAB_UID}"), format!("/api/dashboards/{LAB_UID}?validate_only=true")] {
        let k = key(1);
        let r = call(h.addr, "PUT", &path, &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &k)], &changed).await;
        refused(r, &path);
    }
    // Delete, restore, move out of the store and move into the store.
    let k = key(2);
    refused(
        call(
            h.addr,
            "DELETE",
            &format!("/api/dashboards/{LAB_UID}"),
            &[("If-Match", &ifm), ("Idempotency-Key", &k)],
            b"",
        )
        .await,
        "delete",
    );
    let k = key(3);
    refused(
        call(
            h.addr,
            "POST",
            &format!("/api/dashboards/{LAB_UID}/restore"),
            &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &k)],
            br#"{"version":1}"#,
        )
        .await,
        "restore",
    );
    let k = key(4);
    refused(
        call(
            h.addr,
            "POST",
            &format!("/api/dashboards/{LAB_UID}/move"),
            &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &k)],
            br#"{"folder":"ops"}"#,
        )
        .await,
        "move out",
    );
    let nodes_etag = item(&before, "nodes")["etag"].as_str().unwrap().to_owned();
    let nifm = format!("\"{nodes_etag}\"");
    let k = key(5);
    refused(
        call(
            h.addr,
            "POST",
            "/api/dashboards/nodes/move",
            &[JSON, ("If-Match", &nifm), ("Idempotency-Key", &k)],
            format!("{{\"folder\":\"{LAB_FOLDER}\"}}").as_bytes(),
        )
        .await,
        "move in",
    );
    // Create into a folder of the read-only store, and the validation of a create.
    for path in ["/api/dashboards", "/api/dashboards?validate_only=true"] {
        let k = key(6);
        refused(
            call(
                h.addr,
                "POST",
                path,
                &[JSON, ("Idempotency-Key", &k)],
                &post_body(LAB_FOLDER, &doc_text("lab-new", "New")),
            )
            .await,
            path,
        );
    }
    // The routes of the folders.
    refused(
        call(h.addr, "PUT", &format!("/api/folders/{LAB_FOLDER}"), &[JSON], br#"{"title":"Other"}"#).await,
        "rename folder",
    );
    refused(call(h.addr, "DELETE", &format!("/api/folders/{LAB_FOLDER}"), &[], b"").await, "delete folder");
    // The preview of a draft of a dashboard, or in a folder, of the read-only store.
    let draft = |member: &str, value: &str| {
        let doc: Value = serde_json::from_str(&doc_text("nodes", "Nodes")).unwrap();
        let panel = doc["sections"][0]["panels"][0].clone();
        serde_json::to_vec(&json!({member: value, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
            "max_data_points": 1200, "dashboard_variables": [], "variables": {}, "panel": panel}))
        .unwrap()
    };
    refused(call(h.addr, "POST", "/api/sources/prod/preview", &[JSON], &draft("uid", LAB_UID)).await, "preview uid");
    refused(
        call(h.addr, "POST", "/api/sources/prod/preview", &[JSON], &draft("folder", LAB_FOLDER)).await,
        "preview folder",
    );
    // The files and the catalog did not change.
    assert_eq!(get_json(&h, "/api/dashboards").await, before);
    let names: Vec<String> = std::fs::read_dir(root.join(LAB_FOLDER))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let mut names = names;
    names.sort();
    assert_eq!(names, ["_folder.json", "lab-one.json"]);
    assert!(!root.join(".hune").exists() && !root.join(".history").exists(), "a read-only store writes no file");
}

#[tokio::test]
async fn a_folder_id_a_folder_title_and_a_uid_of_another_store_are_refused_in_the_primary_store() {
    let (h, _d, _r) = start_two().await;
    // The folder id `labops` is in the store `lab`.
    let r = call(
        h.addr,
        "POST",
        "/api/folders",
        &[JSON],
        format!("{{\"id\":\"{LAB_FOLDER}\",\"title\":\"X\"}}").as_bytes(),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    // The title `Lab` is the name of the root folder of the store `lab`.
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"research","title":"Lab"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    let r = call(h.addr, "PUT", "/api/folders/ops", &[JSON], br#"{"title":"Lab"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    // A uid of the store `lab`.
    let k = "0194b3c6-6f1e-7d2a-9b4c-00000000b100";
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", k)],
        &post_body("ops", &doc_text(LAB_UID, "Same uid")),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    // The primary store still works.
    let k = "0194b3c6-6f1e-7d2a-9b4c-00000000b101";
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", k)],
        &post_body("ops", &doc_text("fresh", "Fresh")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    let one = get_json(&h, "/api/dashboards/fresh").await;
    assert_eq!((one["store"].as_str(), one["read_only"].as_bool()), (Some("main"), Some(false)));
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"research","title":"Research"}"#).await;
    assert_eq!(r.status, 201);
    let folders = get_json(&h, "/api/folders").await;
    let research = folders["items"].as_array().unwrap().iter().find(|i| i["id"] == "research").unwrap();
    assert_eq!((research["store"].as_str(), research["read_only"].as_bool()), (Some("main"), Some(false)));
}

// ---- The start rules (5.43) ----

async fn try_start(edit: impl FnOnce(&mut crate::config::Config)) -> lifecycle::StartError {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    fill(&root);
    let mut config = test_config(&root);
    edit(&mut config);
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    match lifecycle::start(Arc::new(config), logger, test_timing(), startup::open_store).await {
        Ok(_) => panic!("the start must stop"),
        Err(e) => e,
    }
}

#[tokio::test]
async fn the_start_stops_for_each_broken_start_rule_and_names_the_store_the_file_and_the_reason() {
    // A read-only store with no dashboard.
    let empty = tempfile::tempdir().unwrap();
    let empty_root = std::fs::canonicalize(empty.path()).unwrap();
    let e = try_start(|c| add_read_only_volume(c, &empty_root)).await;
    assert_eq!(
        (e.step, e.error.as_str()),
        ("store.open", "storage.volume \"lab\" is a read-only store and has no dashboard")
    );
    // A file that is not valid in a read-only store.
    let (_d, root) = lab_volume();
    std::fs::write(root.join(LAB_FOLDER).join("bad.json"), b"{ not json").unwrap();
    let r = root.clone();
    let e = try_start(move |c| add_read_only_volume(c, &r)).await;
    assert_eq!(e.step, "store.open");
    assert!(
        e.error.starts_with(&format!(
            "storage.volume \"lab\": the file {}/{LAB_FOLDER}/bad.json is not valid: ",
            root.display()
        )),
        "{}",
        e.error
    );
    // A uid in two stores.
    let (_d2, root2) = lab_volume();
    std::fs::write(root2.join(LAB_FOLDER).join("nodes.json"), doc_text("nodes", "Nodes")).unwrap();
    let r = root2.clone();
    let e = try_start(move |c| add_read_only_volume(c, &r)).await;
    assert_eq!(
        e.error,
        format!(
            "the dashboard uid \"nodes\" is in the store \"main\" (folder \"transit\") and in the store \"lab\" (folder \"{LAB_FOLDER}\")"
        )
    );
    // A folder id in two stores.
    let (_d3, root3) = lab_volume();
    std::fs::create_dir_all(root3.join("ops")).unwrap();
    std::fs::write(root3.join("ops/_folder.json"), folder_file("Other ops")).unwrap();
    std::fs::write(root3.join("ops/other.json"), doc_text("other", "Other")).unwrap();
    let r = root3.clone();
    let e = try_start(move |c| add_read_only_volume(c, &r)).await;
    assert_eq!(e.error, "the folder id \"ops\" is in the store \"main\" and in the store \"lab\"");
    // A folder of the primary store that has the title of the store `lab`.
    let (_d4, root4) = lab_volume();
    let r = root4.clone();
    let e = try_start(move |c| {
        add_read_only_volume(c, &r);
        c.storage.volumes[1].title = Some("Operations".to_owned());
    })
    .await;
    assert_eq!(
        e.error,
        "the folder \"Operations\" of the primary store has the title of the read-only store \"lab\" (storage.volume \"lab\".title); give another name"
    );
}

#[tokio::test]
async fn a_file_that_is_not_valid_in_the_primary_volume_stops_the_start_too() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    fill(&root);
    std::fs::write(root.join("transit/broken.json"), b"{ not json").unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let e =
        lifecycle::start(Arc::new(test_config(&root)), logger, test_timing(), startup::open_store).await.err().unwrap();
    assert_eq!(e.step, "store.open");
    assert!(
        e.error.starts_with(&format!(
            "storage.volume \"main\": the file {}/transit/broken.json is not valid: ",
            root.display()
        )),
        "{}",
        e.error
    );
}

#[tokio::test]
async fn the_primary_store_can_be_empty_and_the_first_dashboard_can_be_created() {
    let h = Harness::start(|_| {}).await;
    assert_eq!(get_json(&h, "/api/dashboards").await["items"], json!([]));
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"first","title":"First"}"#).await;
    assert_eq!(r.status, 201);
    let k = "0194b3c6-6f1e-7d2a-9b4c-00000000b200";
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", k)],
        &post_body("first", &doc_text("one", "One")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
}

#[tokio::test]
async fn the_store_loaded_line_comes_once_for_each_store() {
    let (h, _d, _r) = start_two().await;
    let lines: Vec<Value> = h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    let loaded: Vec<(&str, &str, u64, u64)> = lines
        .iter()
        .filter(|l| l["event"] == "store.loaded")
        .map(|l| {
            (
                l["store"].as_str().unwrap(),
                l["backend"].as_str().unwrap(),
                l["folders"].as_u64().unwrap(),
                l["dashboards"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(loaded, vec![("main", "volume", 2, 1), ("lab", "volume", 1, 1)]);
}

// ---- A file that is not valid after the start (5.44) ----

async fn wait_for(h: &Harness, what: &str, check: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..100 {
        let list = get_json(h, "/api/dashboards").await;
        if check(&list) {
            return list;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{what}");
}

#[tokio::test]
async fn a_file_that_becomes_not_valid_keeps_its_last_valid_state_and_shows_a_warning() {
    let (h, _d, root) = start_two().await;
    let file = root.join(LAB_FOLDER).join(format!("{LAB_UID}.json"));
    let good = std::fs::read(&file).unwrap();
    std::fs::write(&file, b"{ not json").unwrap();
    let list = wait_for(&h, "the warning", |l| item(l, LAB_UID).get("stale").is_some()).await;
    let lab = item(&list, LAB_UID);
    assert_eq!(
        (lab["state"].as_str(), lab["stale"].as_str(), lab["title"].as_str()),
        (Some("valid"), Some("file_invalid"), Some("Lab one"))
    );
    // The page reads the last valid state, with the warning, and never the path or the parser message.
    let r = super::harness::get(h.addr, &format!("/api/dashboards/{LAB_UID}")).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["stale"], "file_invalid");
    assert_eq!(r.json()["document"]["title"], "Lab one");
    assert!(!r.text().contains(&root.display().to_string()) && !r.text().contains("not json"));
    let dl = super::harness::get(h.addr, &format!("/api/dashboards/{LAB_UID}?download=true")).await;
    assert_eq!(dl.body, good, "the download is the last valid state");
    // The log line and the metric.
    let rejected = h.logger.memory_lines().into_iter().filter(|l| l.contains("\"store.rescan_rejected\"")).count();
    assert_eq!(rejected, 1, "one line for one change of the file");
    let metrics = super::harness::get(h.ops, "/metrics").await.text();
    assert!(metrics.contains("hune_store_invalid_files{store=\"lab\"} 1"), "{metrics}");
    // The file becomes valid again: the warning goes.
    std::fs::write(&file, &good).unwrap();
    let list = wait_for(&h, "the end of the warning", |l| item(l, LAB_UID).get("stale").is_none()).await;
    assert_eq!(item(&list, LAB_UID)["state"], "valid");
    let metrics = super::harness::get(h.ops, "/metrics").await.text();
    assert!(metrics.contains("hune_store_invalid_files{store=\"lab\"} 0"), "{metrics}");
}

#[tokio::test]
async fn a_new_file_that_is_not_valid_after_the_start_has_no_entry() {
    let (h, _d, root) = start_two().await;
    std::fs::write(root.join(LAB_FOLDER).join("fresh-bad.json"), b"nope").unwrap();
    for _ in 0..100 {
        let metrics = super::harness::get(h.ops, "/metrics").await.text();
        if metrics.contains("hune_store_invalid_files{store=\"lab\"} 1") {
            let list = get_json(&h, "/api/dashboards").await;
            assert!(list["items"].as_array().unwrap().iter().all(|i| i["uid"] != "fresh-bad"));
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the metric");
}

// ---- The mode authzen (11.35) ----

fn lab_uid_questions(pdp: &Pdp, action: &str) -> usize {
    pdp.asked().iter().filter(|q| q.action == action && q.resource.contains(LAB_UID)).count()
}

/// A subject that reads and writes everything, `user:alice`. A subject `user:bob` reads and cannot write. No other subject has a right.
fn rules(q: &Q) -> Ans {
    match (q.sub.as_str(), q.action.as_str()) {
        ("user:alice", _) => Ans::yes(),
        ("user:bob", "dashboard.read") => Ans::yes(),
        _ => Ans::No(None),
    }
}

async fn start_authzen(pdp: &Pdp) -> (Harness, tempfile::TempDir) {
    let (dir, root) = lab_volume();
    let r = root.clone();
    let url = pdp.url();
    let h = Harness::start_with(test_timing(), fill, move |c| {
        authzen(c, &url, Duration::ZERO);
        add_read_only_volume(c, &r);
    })
    .await;
    (h, dir)
}

#[tokio::test]
async fn in_the_mode_authzen_a_subject_that_can_read_gets_read_only_store_and_hune_never_asks_to_write() {
    let _pairs = PAIRS.lock().await;
    let pdp = Pdp::start(rules).await;
    let (h, _d) = start_authzen(&pdp).await;
    for sub in ["user:alice", "user:bob"] {
        pdp.clear();
        let save = put_body(&doc_text(LAB_UID, "Changed"), "m");
        let r = ask(&h, sub, "PUT", &format!("/api/dashboards/{LAB_UID}"), &save).await;
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{sub}: {}", r.text());
        let r = ask(&h, sub, "DELETE", &format!("/api/dashboards/{LAB_UID}"), b"").await;
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{sub}");
        let r = ask(&h, sub, "PUT", &format!("/api/folders/{LAB_FOLDER}"), br#"{"title":"T"}"#).await;
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{sub}");
        let r = ask(&h, sub, "POST", "/api/dashboards", &post_body(LAB_FOLDER, &doc_text("lab-new", "New"))).await;
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{sub}");
        // A grant of `dashboard.write` changes nothing: Hune never asked for it on a resource of the read-only store.
        assert_eq!(lab_uid_questions(&pdp, "dashboard.write"), 0, "{sub}");
        assert!(
            pdp.asked().iter().all(|q| !(q.action == "dashboard.write" && q.resource.contains(LAB_FOLDER))),
            "{sub}: {:?}",
            pdp.asked()
        );
    }
}

#[tokio::test]
async fn in_the_mode_authzen_a_subject_that_cannot_read_gets_the_refusal_of_any_other_write_byte_for_byte() {
    let _pairs = PAIRS.lock().await;
    let pdp = Pdp::start(rules).await;
    let (h, _d) = start_authzen(&pdp).await;
    let save = |uid: &str| put_body(&doc_text(uid, "Changed"), "m");
    // For each write route: a target of the read-only store, and a target that does not exist.
    type Request = (String, Vec<u8>);
    let pairs: Vec<(&str, &str, Request, Request)> = vec![
        (
            "save",
            "PUT",
            (format!("/api/dashboards/{LAB_UID}"), save(LAB_UID)),
            ("/api/dashboards/nobody".to_owned(), save("nobody")),
        ),
        (
            "delete",
            "DELETE",
            (format!("/api/dashboards/{LAB_UID}"), Vec::new()),
            ("/api/dashboards/nobody".to_owned(), Vec::new()),
        ),
        (
            "restore",
            "POST",
            (format!("/api/dashboards/{LAB_UID}/restore"), br#"{"version":1}"#.to_vec()),
            ("/api/dashboards/nobody/restore".to_owned(), br#"{"version":1}"#.to_vec()),
        ),
        (
            "move",
            "POST",
            (format!("/api/dashboards/{LAB_UID}/move"), br#"{"folder":"ops"}"#.to_vec()),
            ("/api/dashboards/nobody/move".to_owned(), br#"{"folder":"ops"}"#.to_vec()),
        ),
        (
            "create in a folder",
            "POST",
            ("/api/dashboards".to_owned(), post_body(LAB_FOLDER, &doc_text("n1", "N"))),
            ("/api/dashboards".to_owned(), post_body("nofolder", &doc_text("n2", "N"))),
        ),
        (
            "rename of a folder",
            "PUT",
            (format!("/api/folders/{LAB_FOLDER}"), br#"{"title":"T"}"#.to_vec()),
            ("/api/folders/nofolder".to_owned(), br#"{"title":"T"}"#.to_vec()),
        ),
        (
            "delete of a folder",
            "DELETE",
            (format!("/api/folders/{LAB_FOLDER}"), Vec::new()),
            ("/api/folders/nofolder".to_owned(), Vec::new()),
        ),
    ];
    for (what, method, known, unknown) in pairs {
        pdp.clear();
        let a = ask(&h, "user:eve", method, &known.0, &known.1).await;
        let asked_known = pdp.requests();
        pdp.clear();
        let b = ask(&h, "user:eve", method, &unknown.0, &unknown.1).await;
        let asked_unknown = pdp.requests();
        assert_eq!((a.status, a.code().as_str()), (403, "forbidden"), "{what}: {}", a.text());
        assert_eq!(b.status, a.status, "{what}");
        let strip = |r: &super::harness::Resp| -> Vec<(String, String)> {
            r.headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("date")).cloned().collect()
        };
        assert_eq!(strip(&b), strip(&a), "{what}");
        assert_eq!(b.text(), a.text(), "{what}");
        assert_eq!(a.json()["error"]["action"], "dashboard.write", "{what}");
        assert_eq!((asked_known, asked_unknown), (1, 1), "{what}");
    }
}

#[tokio::test]
async fn in_the_mode_authzen_the_flags_of_a_read_only_item_are_false_without_a_question_to_write() {
    let _pairs = PAIRS.lock().await;
    let pdp = Pdp::start(rules).await;
    let (h, _d) = start_authzen(&pdp).await;
    pdp.clear();
    let t = token("user:alice");
    let r = call(h.addr, "GET", "/api/dashboards", &[(HEADER, t.as_str())], b"").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let list = r.json();
    assert_eq!(item(&list, LAB_UID)["access"]["write"], false);
    assert_eq!(item(&list, "nodes")["access"]["write"], true);
    assert_eq!(lab_uid_questions(&pdp, "dashboard.write"), 0);
    assert!(
        pdp.asked()
            .iter()
            .all(|q| !(q.action == "dashboard.write" && q.resource.ends_with(&format!("folder:{LAB_FOLDER}")))),
        "{:?}",
        pdp.asked()
    );
    pdp.clear();
    let r = call(h.addr, "GET", "/api/folders", &[(HEADER, t.as_str())], b"").await;
    let folders = r.json();
    let lab = folders["items"].as_array().unwrap().iter().find(|i| i["id"] == LAB_FOLDER).unwrap();
    assert_eq!(
        (lab["access"]["write"].as_bool(), lab["read_only"].as_bool(), lab["store"].as_str()),
        (Some(false), Some(true), Some("lab"))
    );
    assert!(
        pdp.asked()
            .iter()
            .all(|q| !(q.action == "dashboard.write" && q.resource.ends_with(&format!("folder:{LAB_FOLDER}")))),
        "{:?}",
        pdp.asked()
    );
}

// A dashboard without a folder in a read-only store (5.9, 5.42): it shows with a null folder, it is read-only, and each
// write to it gives `read_only_store`. A create at the root goes to the primary store.
#[tokio::test]
async fn a_dashboard_without_a_folder_in_a_read_only_store_is_read_only_and_a_root_create_goes_to_the_primary() {
    let (dir, root) = lab_volume();
    std::fs::write(root.join("lab-root.json"), doc_text("lab-root", "Lab root")).unwrap();
    let r = root.clone();
    let h = Harness::start_with(test_timing(), fill, move |c| add_read_only_volume(c, &r)).await;
    let _keep = dir;
    let list = get_json(&h, "/api/dashboards").await;
    let lab = item(&list, "lab-root");
    assert!(lab["folder"].is_null(), "{lab}");
    assert_eq!(
        (lab["store"].as_str(), lab["read_only"].as_bool(), lab["access"]["write"].as_bool()),
        (Some("lab"), Some(true), Some(false))
    );
    let one = get_json(&h, "/api/dashboards/lab-root").await;
    assert!(one["folder"].is_null());
    assert_eq!(one["read_only"], true);
    // The root dashboards come first in the list.
    let order: Vec<String> =
        list["items"].as_array().unwrap().iter().map(|i| i["uid"].as_str().unwrap().to_owned()).collect();
    assert_eq!(order[0], "lab-root", "{order:?}");
    // A save, a move to the root of the primary and a delete give `read_only_store` and change nothing.
    let etag = one["etag"].as_str().unwrap().to_owned();
    let ifm = format!("\"{etag}\"");
    let doc = doc_text("lab-root", "Changed");
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/lab-root",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &super::api::key())],
        &put_body(&doc, ""),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{}", r.text());
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/lab-root/move",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &super::api::key())],
        b"{\"folder\":\"transit\"}",
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{}", r.text());
    let r = call(
        h.addr,
        "DELETE",
        "/api/dashboards/lab-root",
        &[("If-Match", &ifm), ("Idempotency-Key", &super::api::key())],
        b"",
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{}", r.text());
    assert!(root.join("lab-root.json").is_file());
    // A create without a folder goes to the primary store, never to the read-only one. A uid of the read-only store is taken.
    let body = format!("{{\"folder\":null,\"document\":{}}}", doc_text("main-root", "Main root")).into_bytes();
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &super::api::key())], &body).await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert!(r.json()["folder"].is_null());
    let one = get_json(&h, "/api/dashboards/main-root").await;
    assert_eq!((one["store"].as_str(), one["read_only"].as_bool()), (Some("main"), Some(false)));
    assert!(!root.join("main-root.json").exists(), "the root of the read-only store is not written");
    let twin = format!("{{\"folder\":null,\"document\":{}}}", doc_text("lab-root", "Twin")).into_bytes();
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &super::api::key())], &twin).await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"), "{}", r.text());
}

// A file at the root of a volume that is no valid dashboard stops the start and names the file (5.10, rule 5); a name that
// is no uid is ignored.
#[tokio::test]
async fn a_file_at_the_root_of_a_volume_that_is_no_dashboard_stops_the_start_and_another_name_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    fill(&root);
    std::fs::write(root.join("package.json"), b"{\"name\": \"x\"}").unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let e =
        lifecycle::start(Arc::new(test_config(&root)), logger, test_timing(), startup::open_store).await.err().unwrap();
    assert_eq!(e.step, "store.open");
    assert!(
        e.error
            .starts_with(&format!("storage.volume \"main\": the file {}/package.json is not valid: ", root.display())),
        "{}",
        e.error
    );
    // The same volume with names that are no uid: the start works.
    std::fs::remove_file(root.join("package.json")).unwrap();
    std::fs::write(root.join("Notes.json"), b"{}").unwrap();
    std::fs::write(root.join("README.md"), b"text").unwrap();
    let r = root.clone();
    let h = Harness::start_with(
        test_timing(),
        move |dst| {
            for e in std::fs::read_dir(&r).unwrap() {
                let e = e.unwrap();
                if e.path().is_file() {
                    std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
                }
            }
            fill(dst);
        },
        |_| {},
    )
    .await;
    let list = get_json(&h, "/api/dashboards").await;
    let uids: Vec<String> =
        list["items"].as_array().unwrap().iter().map(|i| i["uid"].as_str().unwrap().to_owned()).collect();
    assert_eq!(uids, vec!["nodes".to_owned()]);
}

/// A server with `storage.read_only = true`: one read-only store `lab`, no primary store, no history.
async fn start_read_only() -> (Harness, tempfile::TempDir) {
    let (dir, root) = lab_volume();
    let h = Harness::start_with(
        test_timing(),
        |_| {},
        move |c| {
            c.storage.read_only = true;
            c.storage.volumes.clear();
            add_read_only_volume(c, &root);
            c.server.history = false;
        },
    )
    .await;
    (h, dir)
}

#[tokio::test]
async fn a_file_with_no_primary_store_has_no_editing_and_every_write_is_refused() {
    let (h, _d) = start_read_only().await;
    let session = get_json(&h, "/api/session").await;
    assert_eq!(session["editing"], false);
    assert_eq!(
        session["stores"],
        json!([{"id": "lab", "kind": "volume", "title": "Lab", "primary": false, "read_only": true}])
    );
    let list = get_json(&h, "/api/dashboards").await;
    assert_eq!(item(&list, LAB_UID)["access"]["write"], false);
    let folders = get_json(&h, "/api/folders").await;
    assert_eq!(folders["access"]["create"], false);
    assert_eq!(folders["root"]["access"]["write"], false);
    let refused = |r: super::harness::Resp, what: &str| {
        assert_eq!((r.status, r.code().as_str()), (403, "read_only_store"), "{what}: {}", r.text());
    };
    let key = "0194b3c6-6f1e-7d2a-9b4c-00000000c001";
    refused(
        call(
            h.addr,
            "POST",
            "/api/dashboards",
            &[JSON, ("Idempotency-Key", key)],
            &post_body("", &doc_text("new-one", "New")),
        )
        .await,
        "create",
    );
    refused(call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"research","title":"Research"}"#).await, "folder");
    // The history is off by the file, and the read still works.
    let r = get(h.addr, &format!("/api/dashboards/{LAB_UID}/history")).await;
    assert_eq!((r.status, r.code().as_str()), (403, "history_disabled"));
    assert_eq!(get(h.addr, &format!("/api/dashboards/{LAB_UID}")).await.status, 200);
}
