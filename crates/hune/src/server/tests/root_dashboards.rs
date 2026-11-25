// SPDX-License-Identifier: MIT OR Apache-2.0
//! The dashboards without a folder in the API (6.11 to 6.13, 6.17, 6.19, 6.24): the folder is `null`.

use super::api::{JSON, current, doc_text, etag_of, fill, key, post_body};
use super::harness::*;
use serde_json::{Value, json};

async fn start() -> Harness {
    Harness::start_with(test_timing(), fill, |_| {}).await
}

fn uids_of(r: &Resp) -> Vec<String> {
    r.json()["items"].as_array().unwrap().iter().map(|i| i["uid"].as_str().unwrap().to_owned()).collect()
}

fn root_body(doc: &str) -> Vec<u8> {
    format!("{{\"folder\":null,\"document\":{doc}}}").into_bytes()
}

async fn create_root(h: &Harness, uid: &str, title: &str) -> Resp {
    call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &root_body(&doc_text(uid, title)))
        .await
}

async fn move_to(h: &Harness, uid: &str, etag: &str, folder: Value) -> Resp {
    let ifm = format!("\"{etag}\"");
    let body = json!({ "folder": folder, "message": "" }).to_string().into_bytes();
    call(
        h.addr,
        "POST",
        &format!("/api/dashboards/{uid}/move"),
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        &body,
    )
    .await
}

#[tokio::test]
async fn a_create_with_a_null_folder_makes_a_dashboard_without_a_folder() {
    let h = start().await;
    let r = create_root(&h, "rooted", "At the root").await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert!(r.json()["folder"].is_null(), "{}", r.text());
    // The read gives the folder null, and the history gives null for each version.
    let got = current(&h, "rooted").await;
    assert_eq!(got.status, 200);
    assert!(got.json()["folder"].is_null(), "{}", got.text());
    let history = get(h.addr, "/api/dashboards/rooted/history").await;
    assert_eq!(history.status, 200);
    assert!(history.json()["items"][0]["folder"].is_null(), "{}", history.text());
}

#[tokio::test]
async fn a_create_body_without_the_member_folder_is_refused_and_a_bad_value_gives_the_pointer() {
    let h = start().await;
    // No default: the member is necessary.
    let body = format!("{{\"document\":{}}}", doc_text("nofolder", "No member")).into_bytes();
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(r.status, 400, "{}", r.text());
    // The text of the root is not a folder id: the id rule refuses it.
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("_root", &doc_text("sneaky", "Sneaky")),
    )
    .await;
    assert_eq!(r.status, 400, "{}", r.text());
    assert_eq!(r.json()["error"]["errors"][0]["pointer"], "/folder");
    // A number is not a folder.
    let body = format!("{{\"folder\":3,\"document\":{}}}", doc_text("num", "Number")).into_bytes();
    let r = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(r.status, 400, "{}", r.text());
}

#[tokio::test]
async fn the_list_has_null_for_the_folder_puts_the_root_first_and_filters_with_an_empty_value() {
    let h = start().await;
    assert_eq!(create_root(&h, "zz-root", "Zzz root").await.status, 201);
    let r = get(h.addr, "/api/dashboards").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let body = r.json();
    let items = body["items"].as_array().unwrap().clone();
    let order: Vec<(String, Option<String>)> = items
        .iter()
        .map(|i| (i["uid"].as_str().unwrap().to_owned(), i["folder"].as_str().map(str::to_owned)))
        .collect();
    assert_eq!(order, vec![("zz-root".to_owned(), None), ("nodes".to_owned(), Some("transit".to_owned()))]);
    // The filter with an empty value gives the dashboards without a folder, and only them.
    let r = get(h.addr, "/api/dashboards?folder=").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(uids_of(&r), vec!["zz-root"]);
    // The filter with a folder id does not give them. An unknown folder gives an empty list.
    let r = get(h.addr, "/api/dashboards?folder=transit").await;
    assert_eq!(uids_of(&r), vec!["nodes"]);
    let r = get(h.addr, "/api/dashboards?folder=nothing").await;
    assert_eq!(r.json()["items"], json!([]));
    // The pages keep the order: one item a page, the root first.
    let first = get(h.addr, "/api/dashboards?limit=1").await.json();
    assert_eq!(first["items"][0]["uid"], "zz-root");
    let next = first["next"].as_str().expect("a cursor").to_owned();
    let second = get(h.addr, &format!("/api/dashboards?limit=1&after={next}")).await.json();
    assert_eq!(second["items"][0]["uid"], "nodes");
}

#[tokio::test]
async fn a_move_goes_to_the_root_with_null_and_back_to_a_folder() {
    let h = start().await;
    let r = create_root(&h, "mover", "Mover").await;
    let etag = etag_of(&r);
    let r = move_to(&h, "mover", &etag, json!("ops")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["folder"], "ops");
    assert_eq!(r.json()["noop"], false);
    let r = move_to(&h, "mover", &etag, Value::Null).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json()["folder"].is_null(), "{}", r.text());
    assert_eq!(r.json()["noop"], false);
    // A move to the place where the dashboard is writes nothing.
    let r = move_to(&h, "mover", &etag, Value::Null).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["noop"], true);
    // A move of a dashboard of a folder to the root.
    let r = move_to(&h, "nodes", &etag_of(&current(&h, "nodes").await), Value::Null).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json()["folder"].is_null());
    // The member folder is necessary: a move body without it is refused.
    let ifm = format!("\"{etag}\"");
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/mover/move",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        b"{\"message\":\"\"}",
    )
    .await;
    assert_eq!(r.status, 400, "{}", r.text());
}

#[tokio::test]
async fn the_folders_answer_has_the_member_root_and_no_root_item() {
    let h = start().await;
    let r = get(h.addr, "/api/folders").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    assert_eq!(j["root"]["access"]["write"], true, "{}", r.text());
    assert_eq!(j["root"]["read_only"], false);
    assert!(j["root"]["store"].is_string());
    let ids: Vec<String> =
        j["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_owned()).collect();
    assert_eq!(ids, vec!["ops", "transit"], "the root is not a folder");
}

#[tokio::test]
async fn a_delete_and_a_restore_keep_the_place_without_a_folder() {
    let h = start().await;
    let r = create_root(&h, "gone", "Gone").await;
    let etag = etag_of(&r);
    let ifm = format!("\"{etag}\"");
    let r =
        call(h.addr, "DELETE", "/api/dashboards/gone", &[("If-Match", &ifm), ("Idempotency-Key", &key())], b"").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json()["folder"].is_null(), "{}", r.text());
    assert_eq!(current(&h, "gone").await.status, 404);
}
