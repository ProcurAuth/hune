//! Decision ID-K321, point 2 (owner answer of 2026-11-21): a restore validates its version one time (6.18).
//!
//! The handler reads the version and validates it in `spawn_blocking`, off the store lock. The store gets the valid
//! document and only checks `If-Match` and the folder under its lock. Before the fix, the store validated the version
//! a second time under its exclusive lock: the review measured 11.6 s for the slowest of 30 parallel `409` restores.

use super::api::{JSON, ZERO_ETAG, doc_text, etag_of, fill, key, post_body, put_body};
use super::bounded_checks::{heavy_doc, min_of_7};
use super::harness::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The validations of the handler and the validations of versions in the store.
fn counts(h: &Harness) -> (u64, u64) {
    let state = h.state();
    let store = state.services().expect("the store is open").store.restore_checks();
    (state.validations_run.load(Ordering::SeqCst), store)
}

async fn restore(h: &Harness, uid: &str, version: u64, if_match: &str) -> Resp {
    call(
        h.addr,
        "POST",
        &format!("/api/dashboards/{uid}/restore"),
        &[JSON, ("If-Match", if_match), ("Idempotency-Key", &key())],
        format!("{{\"version\":{version}}}").as_bytes(),
    )
    .await
}

/// One restore gives one validation of the version, with a good `If-Match` and with a wrong one. Before the fix: 2.
#[tokio::test]
async fn k321_a_restore_validates_its_version_one_time() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let e0 = etag_of(&get(h.addr, "/api/dashboards/nodes").await);
    let ifm = format!("\"{e0}\"");
    let r = call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        &put_body(&doc_text("nodes", "Two"), "m"),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let e1 = format!("\"{}\"", etag_of(&r));
    // A restore with a wrong `If-Match`: the validation, then `409` from the store.
    let before = counts(&h);
    let r = restore(&h, "nodes", 1, ZERO_ETAG).await;
    assert_eq!((r.status, r.code().as_str()), (409, "conflict"), "{}", r.text());
    let after = counts(&h);
    assert_eq!((after.0 - before.0, after.1 - before.1), (1, 0), "validations of the handler and of the store");
    // A restore that writes: one validation too, and the store writes the canonical bytes of version 1.
    let before = counts(&h);
    let r = restore(&h, "nodes", 1, &e1).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let after = counts(&h);
    assert_eq!((after.0 - before.0, after.1 - before.1), (1, 0), "validations of the handler and of the store");
    assert_eq!(etag_of(&r), e0, "the etag of version 1");
    assert_eq!(etag_of(&get(h.addr, "/api/dashboards/nodes").await), e0);
}

/// A `POST` with a `std::net` socket: the status and the time to the end of the answer.
fn blocking_post(addr: std::net::SocketAddr, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, Duration) {
    use std::io::{Read, Write};
    let t = Instant::now();
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    s.write_all(&request("POST", path, &[headers, &[("Connection", "close")]].concat(), body)).unwrap();
    let mut answer = Vec::new();
    s.read_to_end(&mut answer).unwrap();
    let took = t.elapsed();
    let text = String::from_utf8_lossy(&answer);
    let status = text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, took)
}

/// A `POST` that waits 60 s at most for its answer, for a client of the attack. It gives the status, or 0.
async fn slow_post(addr: std::net::SocketAddr, path: &str, headers: &[(&str, &str)], body: &[u8]) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Ok(mut s) = tokio::net::TcpStream::connect(addr).await else { return 0 };
    if s.write_all(&request("POST", path, &[headers, &[("Connection", "close")]].concat(), body)).await.is_err() {
        return 0;
    }
    let mut answer = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(60), s.read_to_end(&mut answer)).await;
    String::from_utf8_lossy(&answer).split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
}

/// 16 clients restore a heavy version with a wrong `If-Match` again and again (each answer is `409`). A move of a
/// small dashboard, which takes the store lock and no validation permit, still ends within 200 ms. A save is not the
/// probe: it needs a validation permit, and the restores hold the 4 permits for their one validation (6.27). Before
/// the fix, each restore held the exclusive store lock for a second validation of the heavy version.
#[test]
fn k321_a_restore_with_a_wrong_if_match_does_not_hold_the_store_lock_for_a_validation() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(16)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let h = Harness::start_with(test_timing(), fill, |_| {}).await;
        // Version 1 is the heavy document. Version 2 is small, so that the store reads a small current document.
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards",
            &[JSON, ("Idempotency-Key", &key())],
            &post_body("transit", &heavy_doc("heavy", 4)),
        )
        .await;
        assert_eq!(r.status, 201, "{}", r.text());
        let ifm = format!("\"{}\"", etag_of(&r));
        let r = call(
            h.addr,
            "PUT",
            "/api/dashboards/heavy",
            &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
            &put_body(&doc_text("heavy", "Small"), "m"),
        )
        .await;
        assert_eq!(r.status, 200, "{}", r.text());
        let probe = format!("\"{}\"", etag_of(&get(h.addr, "/api/dashboards/nodes").await));
        let stop = Arc::new(AtomicBool::new(false));
        let mut attackers = Vec::new();
        for _ in 0..16 {
            let (stop, addr) = (stop.clone(), h.addr);
            attackers.push(tokio::spawn(async move {
                let mut statuses = Vec::new();
                while !stop.load(Ordering::SeqCst) {
                    let id = key();
                    let headers = [JSON, ("If-Match", ZERO_ETAG), ("Idempotency-Key", id.as_str())];
                    statuses
                        .push(slow_post(addr, "/api/dashboards/heavy/restore", &headers, br#"{"version":1}"#).await);
                }
                statuses
            }));
        }
        // The restores reach the store before the first probe.
        std::thread::sleep(Duration::from_millis(1500));
        let mut runs = Vec::new();
        for i in 0..9 {
            let body = if i % 2 == 0 { br#"{"folder":"ops"}"# as &[u8] } else { br#"{"folder":"transit"}"# };
            let id = key();
            let headers = [JSON, ("If-Match", probe.as_str()), ("Idempotency-Key", id.as_str())];
            let (status, took) = blocking_post(h.addr, "/api/dashboards/nodes/move", &headers, body);
            assert_eq!(status, 200);
            runs.push(took);
            std::thread::sleep(Duration::from_millis(100));
        }
        stop.store(true, Ordering::SeqCst);
        let mut statuses = Vec::new();
        for a in attackers {
            statuses.extend(a.await.unwrap());
        }
        let worst = runs[2..].iter().max().copied().unwrap_or_default();
        eprintln!("move min of 7 {:?}, max {worst:?}; restores {}", min_of_7(&runs), statuses.len());
        // Each restore is a conflict, or a `503` when it waits too long for a validation permit.
        assert!(statuses.iter().all(|s| [409, 503].contains(s)), "{statuses:?}");
        assert!(statuses.contains(&409), "{statuses:?}");
        assert!(min_of_7(&runs) < Duration::from_millis(200), "{runs:?}");
    });
}
