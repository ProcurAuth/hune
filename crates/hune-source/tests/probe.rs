// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the probe and the state of a source (8.11).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::*;
use hune_source::SourceState;

const GOOD: &[&str] = &["GRANT SELECT ON analytics.* TO hune"];

fn probe_server(version: &'static str, readonly: &'static str, grants: &'static [&'static str]) -> Handler {
    handler(move |rec| probe_answer(rec, version, readonly, grants).unwrap_or_else(|| ok_steps(rec, ONE_COLUMN)))
}

#[tokio::test]
async fn a_healthy_account_is_up() {
    let server = FakeServer::start(probe_server("26.8.34.13", "2", GOOD)).await;
    let src = source(config(server.addr));
    assert_eq!(src.state(), SourceState::Unknown);
    assert!(src.last_probe().is_none());
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Up);
    assert_eq!(p.previous, SourceState::Unknown);
    assert!(p.changed());
    assert_eq!(p.version.as_deref(), Some("26.8.34.13"));
    assert_eq!(p.version_supported, Some(true));
    assert_eq!(p.detail, None);
    assert_eq!(src.state(), SourceState::Up);
    assert_eq!(src.last_probe(), Some(p));
    // Two statements, in this order, with the parameters of 8.3 and the limits of the keys.
    assert_eq!(server.request_count(), 2);
    let (a, b) = (server.request(0), server.request(1));
    assert_eq!(a.sql(), "SELECT version(), getSetting('readonly')");
    assert_eq!(b.sql(), "SHOW GRANTS");
    for r in [&a, &b] {
        assert_eq!(r.param("log_comment").as_deref(), Some(r#"{"app":"hune","kind":"probe"}"#));
        assert_eq!(r.param("max_execution_time").as_deref(), Some("60"));
        assert_eq!(r.param("max_result_rows").as_deref(), Some("100000"));
        assert_eq!(r.param("max_result_bytes").as_deref(), Some("1048576"));
        assert_eq!(r.param("max_query_size").as_deref(), Some("1048576"));
        assert_eq!(r.param("result_overflow_mode").as_deref(), Some("throw"));
        assert_eq!(r.header("x-clickhouse-user"), Some("hune"));
        assert!(uuid_v7(&r.query_id()));
    }
    assert_ne!(a.query_id(), b.query_id());
    // The second probe does not change the state.
    let p2 = src.probe().await;
    assert!(!p2.changed());
}

fn uuid_v7(s: &str) -> bool {
    hune_source::Uuid::parse(s).is_some_and(|u| u.version() == 7)
}

#[tokio::test]
async fn an_old_version_is_up_with_a_warning_flag() {
    let server = FakeServer::start(probe_server("25.12.3.1", "2", GOOD)).await;
    let src = source(config(server.addr));
    let p = src.probe().await;
    assert_eq!((p.state, p.version_supported), (SourceState::Up, Some(false)));
    // The queries still run.
    src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
}

#[tokio::test]
async fn readonly_other_than_2_is_misconfigured() {
    for ro in ["0", "1"] {
        let server = FakeServer::start(probe_server("26.8.1.1", ro, GOOD)).await;
        let src = source(config(server.addr));
        let p = src.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured, "{ro}");
        assert!(p.detail.unwrap().contains(&format!("readonly = {ro}")));
        // The grants are not read after this failure.
        assert_eq!(server.request_count(), 1);
    }
}

#[tokio::test]
async fn a_grant_that_breaks_the_contract_is_misconfigured() {
    let bad: &[&[&str]] = &[
        &["GRANT SELECT, URL ON *.* TO hune"],
        &["GRANT SELECT ON analytics.* TO hune", "GRANT INSERT ON analytics.* TO hune"],
        &["GRANT SELECT ON analytics.* TO hune WITH GRANT OPTION"],
        &["GRANT admin TO hune"],
        &["GRANT KILL QUERY ON *.* TO hune"],
    ];
    for grants in bad {
        let server = FakeServer::start(handler({
            let g = *grants;
            move |rec| probe_answer(rec, "26.8.1.1", "2", g).unwrap()
        }))
        .await;
        let src = source(config(server.addr));
        let p = src.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured, "{grants:?}");
        assert!(p.detail.unwrap().contains("8.12"));
    }
}

#[tokio::test]
async fn a_failed_login_is_misconfigured() {
    let server =
        FakeServer::start(handler(|rec| error_steps(rec, 403, 516, "Code: 516. DB::Exception: Authentication failed")))
            .await;
    let src = source(config(server.addr));
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("login"));
}

#[tokio::test]
async fn a_setting_above_a_constraint_is_misconfigured() {
    let server = FakeServer::start(handler(|rec| {
        error_steps(rec, 500, 452, "Code: 452. DB::Exception: Setting max_result_rows should not be greater than 1000")
    }))
    .await;
    let src = source(config(server.addr));
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("constraint"));
}

#[tokio::test]
async fn another_failure_is_down() {
    let server = FakeServer::start(handler(|rec| error_steps(rec, 500, 999, "Code: 999. DB::Exception: boom"))).await;
    let src = source(config(server.addr));
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Down);
    assert!(p.detail.unwrap().contains("query_error"));
    // A bad answer is down too.
    let server = FakeServer::start(handler(|rec| ok_steps(rec, "[\"a\"]\n[\"String\"]\n[\"x\"]\n"))).await;
    let src = source(config(server.addr));
    assert_eq!(src.probe().await.state, SourceState::Down);
}

#[tokio::test]
async fn a_misconfigured_source_sends_nothing_until_a_probe_succeeds() {
    let ro = Arc::new(AtomicUsize::new(0));
    let r2 = Arc::clone(&ro);
    let server = FakeServer::start(handler(move |rec| {
        let readonly = if r2.load(Ordering::SeqCst) == 0 { "0" } else { "2" };
        probe_answer(rec, "26.8.1.1", readonly, GOOD).unwrap_or_else(|| ok_steps(rec, ONE_COLUMN))
    }))
    .await;
    let src = source(config(server.addr));
    assert_eq!(src.probe().await.state, SourceState::Misconfigured);
    let n = server.request_count();
    for _ in 0..3 {
        let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_misconfigured", 502, false));
        assert!(e.message().contains("readonly = 0"));
        assert_eq!(e.members().len(), 1);
    }
    assert_eq!(server.request_count(), n, "no query reached the server");
    // The operator fixes the account: the next probe succeeds.
    ro.store(1, Ordering::SeqCst);
    let p = src.probe().await;
    assert_eq!((p.state, p.previous), (SourceState::Up, SourceState::Misconfigured));
    src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
}

#[tokio::test]
async fn a_query_on_an_unknown_source_runs_the_first_probe() {
    let server = FakeServer::start(probe_server("26.8.1.1", "2", GOOD)).await;
    let src = source(config(server.addr));
    src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(src.state(), SourceState::Up);
    assert_eq!(server.request(0).sql(), "SELECT version(), getSetting('readonly')");
    assert_eq!(server.request(2).sql(), "SELECT 1");
}

#[tokio::test]
async fn concurrent_queries_on_an_unknown_source_share_one_probe() {
    let server = FakeServer::start(handler(|rec| {
        let mut v = vec![Step::Sleep(Duration::from_millis(100))];
        v.extend(probe_answer(rec, "26.8.1.1", "2", GOOD).unwrap_or_else(|| ok_steps(rec, ONE_COLUMN)));
        v
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.queue_wait = Duration::from_secs(3);
    let src = source(cfg);
    let mut hs = Vec::new();
    for _ in 0..5 {
        let s = src.clone();
        hs.push(tokio::spawn(async move { s.query(sql("SELECT 1"), limits(), ctx()).await }));
    }
    for h in hs {
        h.await.unwrap().unwrap();
    }
    let probes = server.requests.lock().unwrap().iter().filter(|r| r.sql().starts_with("SELECT version()")).count();
    assert_eq!(probes, 1);
}

#[tokio::test]
async fn concurrent_probes_give_one_result() {
    let server = FakeServer::start(handler(|rec| {
        let mut v = vec![Step::Sleep(Duration::from_millis(100))];
        v.extend(probe_answer(rec, "26.8.1.1", "2", GOOD).unwrap());
        v
    }))
    .await;
    let src = source(config(server.addr));
    let (a, b, c) = tokio::join!(src.probe(), src.probe(), src.probe());
    assert_eq!((a.state, b.state, c.state), (SourceState::Up, SourceState::Up, SourceState::Up));
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn the_wait_for_the_first_probe_is_bounded_by_queue_wait() {
    let server = FakeServer::start(handler(|rec| {
        let mut v = vec![Step::Sleep(Duration::from_millis(600))];
        v.extend(probe_answer(rec, "26.8.1.1", "2", GOOD).unwrap());
        v
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.queue_wait = Duration::from_millis(100);
    let src = source(cfg);
    let t = std::time::Instant::now();
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!(e.code(), "source_busy");
    assert!(t.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn the_probe_takes_no_permit() {
    let server =
        FakeServer::start(handler(|rec| probe_answer(rec, "26.8.1.1", "2", GOOD).unwrap_or_else(|| vec![Step::Hang])))
            .await;
    let mut cfg = config(server.addr);
    cfg.max_concurrent = 1;
    let src = source(cfg);
    src.probe().await;
    let s2 = src.clone();
    let q = tokio::spawn(async move { s2.query(sql("SELECT sleep(9)"), limits(), ctx()).await });
    let t = std::time::Instant::now();
    while src.admission_stats().in_flight < 1 {
        assert!(t.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Up);
    assert_eq!(src.admission_stats().in_flight, 1);
    q.abort();
}

#[tokio::test]
async fn the_state_watch_sees_each_change() {
    let ro = Arc::new(AtomicUsize::new(2));
    let r2 = Arc::clone(&ro);
    let server = FakeServer::start(handler(move |rec| {
        let readonly = if r2.load(Ordering::SeqCst) == 2 { "2" } else { "0" };
        probe_answer(rec, "26.8.1.1", readonly, GOOD).unwrap()
    }))
    .await;
    let src = source(config(server.addr));
    let mut rx = src.watch_state();
    assert_eq!(*rx.borrow(), SourceState::Unknown);
    src.probe().await;
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), SourceState::Up);
    ro.store(0, Ordering::SeqCst);
    src.probe().await;
    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), SourceState::Misconfigured);
    // No change, no event.
    src.probe().await;
    assert!(!rx.has_changed().unwrap());
}
