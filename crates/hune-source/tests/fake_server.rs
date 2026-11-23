//! Tests of the client against a fake HTTP server: request, answer, errors, limits, pool, state.

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::*;
use hune_source::{AnySource, Member, QueueWait, SourceError, SourceState, TraceParent};

const SPEC_BODY: &str = "[\"t\",\"node\",\"weight\",\"blocks\",\"active\"]\n\
    [\"DateTime\",\"LowCardinality(String)\",\"Decimal(38, 9)\",\"UInt32\",\"Bool\"]\n\
    [\"1794211200\",\"a1\",\"12.500000000\",42,true]\n[\"1794211220\",null,\"3.000000000\",7,false]\n";

/// A handler that answers each statement with the body, and the probe statements with a healthy account.
fn healthy(body: &'static str) -> Handler {
    handler(move |rec| {
        probe_answer(rec, "26.8.34.13", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| ok_steps(rec, body))
    })
}

#[tokio::test]
async fn the_request_of_8_3() {
    let server = FakeServer::start(healthy(SPEC_BODY)).await;
    let src = source(config(server.addr));
    let mut c = ctx();
    c.traceparent = TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
    let frame = src.query(sql("SELECT 1 -- 71 KB later"), limits(), c.clone()).await.unwrap();
    // The first request is the probe of the unknown source; the query is the third one.
    let rec = server.request(server.request_count() - 1);
    assert_eq!(rec.method, "POST");
    assert_eq!(rec.path(), "/");
    assert_eq!(rec.sql(), "SELECT 1 -- 71 KB later");
    assert_eq!(rec.header("host").unwrap(), server.addr.to_string());
    assert_eq!(rec.header("x-clickhouse-user"), Some("hune"));
    assert_eq!(rec.header("x-clickhouse-key"), Some("p4ss-secret"));
    assert_eq!(rec.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(rec.header("content-length"), Some("23"));
    assert_eq!(rec.header("user-agent"), Some(concat!("hune/", env!("CARGO_PKG_VERSION"))));
    assert_eq!(rec.header("traceparent"), Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"));
    for absent in ["authorization", "accept-encoding", "cookie", "proxy-authorization"] {
        assert!(rec.header(absent).is_none(), "{absent}");
    }
    let expect = [
        ("database", "analytics"),
        ("default_format", "JSONCompactEachRowWithNamesAndTypes"),
        ("max_execution_time", "60"),
        ("timeout_overflow_mode", "throw"),
        ("max_result_rows", "1000"),
        ("max_result_bytes", "1048576"),
        ("result_overflow_mode", "throw"),
        ("max_query_size", "1048576"),
        ("wait_end_of_query", "1"),
        ("cancel_http_readonly_queries_on_client_close", "1"),
        ("output_format_json_quote_64bit_integers", "1"),
        ("output_format_json_quote_decimals", "1"),
        ("output_format_json_quote_denormals", "1"),
        ("output_format_json_validate_utf8", "1"),
        ("date_time_output_format", "unix_timestamp"),
    ];
    let params = rec.params();
    for (k, v) in expect {
        assert_eq!(rec.param(k).as_deref(), Some(v), "{k}");
    }
    // query_id and log_comment (8.10).
    assert_eq!(rec.query_id(), c.query_id.to_string());
    assert_eq!(rec.param("log_comment").unwrap(), c.log_comment.to_json());
    // The settings are exactly the 17 of 8.3.
    assert_eq!(params.len(), 17, "{params:?}");
    assert!(!rec.target.contains("p4ss-secret") && !rec.target.contains("SELECT"));
    // The frame.
    assert_eq!(frame.query_id, c.query_id.to_string());
    assert_eq!(frame.rows, 2);
    assert_eq!(frame.columns.len(), 5);
    let json = frame.to_json();
    assert!(
        json.contains(
            r#""values":[[1794211200,1794211220],["a1",null],["12.500000000","3.000000000"],[42,7],[true,false]]"#
        ),
        "{json}"
    );
}

#[tokio::test]
async fn a_large_sql_goes_in_the_body_not_in_the_url() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = source(config(server.addr));
    let big = format!("SELECT 1 /* {} */", "x".repeat(71_000));
    src.query(sql(&big), limits(), ctx()).await.unwrap();
    let rec = server.request(server.request_count() - 1);
    assert_eq!(rec.body.len(), big.len());
    assert!(rec.target.len() < 2000);
}

#[tokio::test]
async fn stats_come_from_the_summary_header() {
    let server = FakeServer::start(handler(|rec| {
        if let Some(p) = probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]) {
            return p;
        }
        let summary = r#"{"read_rows":"120000","read_bytes":"9000000","written_rows":"0","result_rows":"3","result_bytes":"512"}"#;
        ok_steps_with(rec, &[ONE_COLUMN.as_bytes()], &[("X-ClickHouse-Summary", summary.to_owned())])
    }))
    .await;
    let src = source(config(server.addr));
    let f = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(
        (f.stats.read_rows, f.stats.read_bytes, f.stats.result_bytes),
        (Some(120_000), Some(9_000_000), Some(512))
    );
    assert!(f.stats.elapsed_ms < 5000);
}

#[tokio::test]
async fn stats_are_null_without_the_header() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = source(config(server.addr));
    let f = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!((f.stats.read_rows, f.stats.read_bytes, f.stats.result_bytes), (None, None, None));
    assert!(f.to_json().contains(r#""read_rows":null,"read_bytes":null,"result_bytes":null"#));
}

#[tokio::test]
async fn the_query_id_header_is_checked() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            vec![
                Step::Send(head(200, &[("X-ClickHouse-Query-Id", "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d".to_owned())])),
                Step::Send(chunked(&[ONE_COLUMN.as_bytes()])),
            ]
        })
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("source_bad_response", 502));
    assert!(e.query_id().is_some() && e.source_id().is_some());
}

#[tokio::test]
async fn a_missing_query_id_header_is_a_bad_response() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| vec![Step::Send(head(200, &[])), Step::Send(chunked(&[ONE_COLUMN.as_bytes()]))])
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!(e.code(), "source_bad_response");
}

async fn error_for(status: u16, code: i64, text: &'static str) -> (SourceError, SourceState) {
    let server = FakeServer::start(handler(move |rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| error_steps(rec, status, code, text))
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    (e, src.state())
}

#[tokio::test]
async fn clickhouse_errors_map_as_8_5() {
    let (e, _) =
        error_for(500, 62, "Code: 62. DB::Exception: Syntax error: failed at position 8\nStack trace:\n0. foo").await;
    assert_eq!((e.code(), e.http_status(), e.clickhouse_code()), ("query_error", 422, Some(62)));
    assert_eq!(e.message(), "Code: 62. DB::Exception: Syntax error: failed at position 8");
    assert!(!e.retryable());

    let (e, _) = error_for(404, 60, "Code: 60. DB::Exception: Table analytics.nope does not exist.").await;
    assert_eq!(e.clickhouse_code(), Some(60));
    let (e, _) = error_for(500, 241, "Code: 241. DB::Exception: Memory limit exceeded").await;
    assert_eq!(e.code(), "query_memory_limit");
    let (e, _) = error_for(500, 497, "Code: 497. DB::Exception: hune: Not enough privileges").await;
    assert_eq!((e.code(), e.http_status()), ("source_access_denied", 502));
    let (e, _) = error_for(500, 159, "Code: 159. DB::Exception: Timeout exceeded").await;
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    let (e, _) = error_for(500, 160, "Code: 160. DB::Exception: Estimated query execution time is too long").await;
    assert_eq!(e.code(), "query_timeout");
    let (e, _) = error_for(500, 396, "Code: 396. DB::Exception: Limit for result exceeded").await;
    assert_eq!((e.code(), e.limit_name(), e.limit_value()), ("query_limit", Some("max_result"), None));
    let (e, _) = error_for(500, 158, "Code: 158. DB::Exception: Limit for rows to read exceeded").await;
    assert_eq!(e.limit_name(), Some("max_rows_to_read"));
    let (e, _) = error_for(500, 307, "Code: 307. DB::Exception: Limit for bytes to read exceeded").await;
    assert_eq!(e.limit_name(), Some("max_bytes_to_read"));
    let (e, _) = error_for(500, 394, "Code: 394. DB::Exception: Query was cancelled").await;
    assert_eq!(e, SourceError::Cancelled);
    let (e, _) = error_for(500, 735, "Code: 735. DB::Exception: cancelled by client").await;
    assert_eq!(e, SourceError::Cancelled);
    let (e, _) = error_for(500, 452, "Code: 452. DB::Exception: Setting is below constraint").await;
    assert_eq!((e.code(), e.clickhouse_code()), ("query_error", Some(452)));
}

#[tokio::test]
async fn login_failures_make_the_source_misconfigured() {
    for code in [192, 193, 194, 516] {
        let (e, state) = error_for(403, code, "Code: 516. DB::Exception: hune: Authentication failed").await;
        assert_eq!((e.code(), e.http_status()), ("source_misconfigured", 502), "{code}");
        assert_eq!(state, SourceState::Misconfigured);
    }
}

#[tokio::test]
async fn unavailable_codes_make_the_source_down() {
    for code in [202, 203, 209, 210, 279] {
        let (e, state) = error_for(500, code, "Code: 209. DB::Exception: Timeout").await;
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true), "{code}");
        assert_eq!(state, SourceState::Down);
    }
}

#[tokio::test]
async fn a_status_other_than_200_without_the_header_is_unavailable() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| vec![Step::Send(head(502, &[])), Step::Send(chunked(&[b"<html>Bad gateway</html>"]))])
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
    assert!(!e.message().contains("Bad gateway"));
    assert_eq!(src.state(), SourceState::Down);
}

#[tokio::test]
async fn a_success_after_down_sets_up() {
    let fail = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let f2 = std::sync::Arc::clone(&fail);
    let server = FakeServer::start(handler(move |rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            if f2.load(Ordering::SeqCst) { error_steps(rec, 500, 209, "Code: 209.") } else { ok_steps(rec, ONE_COLUMN) }
        })
    }))
    .await;
    let src = source(config(server.addr));
    let mut rx = src.watch_state();
    assert!(src.query(sql("SELECT 1"), limits(), ctx()).await.is_err());
    assert_eq!(src.state(), SourceState::Down);
    // A query on a down source still goes to the server.
    fail.store(false, Ordering::SeqCst);
    src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(src.state(), SourceState::Up);
    assert!(rx.has_changed().unwrap());
    assert_eq!(*rx.borrow_and_update(), SourceState::Up);
}

#[tokio::test]
async fn an_exception_in_the_middle_of_the_stream() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            ok_steps_with(
                rec,
                &[b"[\"n\"]\n[\"UInt32\"]\n[1]\n[2]\n", b"[3]\nCode: 241. DB::Exce", b"ption: Memory limit exceeded\n"],
                &[],
            )
        })
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!(e.code(), "query_memory_limit");
}

#[tokio::test]
async fn a_cut_stream_is_not_a_frame() {
    // The server stops in the middle of a chunked body.
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            vec![
                Step::Send(head(200, &[("X-ClickHouse-Query-Id", rec.query_id())])),
                Step::Send(b"20\r\n[\"n\"]\n[\"UInt32\"]\n[1]\n".to_vec()),
                Step::Close,
            ]
        })
    }))
    .await;
    let src = source(config(server.addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
}

#[tokio::test]
async fn the_row_limit_closes_the_connection() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            let mut first = b"[\"n\"]\n[\"UInt32\"]\n".to_vec();
            first.extend_from_slice(b"[1]\n".repeat(50).as_slice());
            let mut v = vec![Step::Send(head(200, &[("X-ClickHouse-Query-Id", rec.query_id())]))];
            v.push(Step::Send(format!("{:x}\r\n", first.len()).into_bytes()));
            v.push(Step::Send(first));
            v.push(Step::Send(b"\r\n".to_vec()));
            // Then rows without end, as chunks of 40 rows.
            let row = b"[1]\n".repeat(40);
            let mut chunk = format!("{:x}\r\n", row.len()).into_bytes();
            chunk.extend_from_slice(&row);
            chunk.extend_from_slice(b"\r\n");
            v.push(Step::Endless(chunk));
            v
        })
    }))
    .await;
    let src = source(config(server.addr));
    let mut l = limits();
    l.max_result_rows = 100;
    let e = src.query(sql("SELECT 1"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422));
    assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result_rows"), Some(100)));
    wait_for(|| server.counters.endless_write_failed.load(Ordering::SeqCst) == 1).await;
    // The connection did not go back to the pool.
    // The query took the connection of the probe and closed it: nothing is idle.
    assert_eq!(src.idle_connections(), 0);
}

#[tokio::test]
async fn the_row_limit_from_a_ceiling_is_a_ceiling_error() {
    let server = FakeServer::start(healthy("[\"n\"]\n[\"UInt32\"]\n[1]\n[2]\n[3]\n")).await;
    let src = source(config(server.addr));
    let mut l = limits();
    l.max_result_rows = 2;
    l.rows_from_ceiling = true;
    let e = src.query(sql("SELECT 1"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
    assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_rows"), Some(2)));
    assert_eq!(e.members(), vec![("ceiling", Member::Text("max_rows".into())), ("limit", Member::Uint(2))]);
}

#[tokio::test]
async fn the_byte_limit_closes_the_connection() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            let mut first = b"[\"s\"]\n[\"String\"]\n".to_vec();
            first.extend_from_slice(b"[\"abcdefghij\"]\n".repeat(10).as_slice());
            let mut v = vec![Step::Send(head(200, &[("X-ClickHouse-Query-Id", rec.query_id())]))];
            v.push(Step::Send(format!("{:x}\r\n", first.len()).into_bytes()));
            v.push(Step::Send(first));
            v.push(Step::Send(b"\r\n".to_vec()));
            let row = b"[\"abcdefghij\"]\n".repeat(100);
            let mut chunk = format!("{:x}\r\n", row.len()).into_bytes();
            chunk.extend_from_slice(&row);
            chunk.extend_from_slice(b"\r\n");
            v.push(Step::Endless(chunk));
            v
        })
    }))
    .await;
    let src = source(config(server.addr));
    let mut l = limits();
    l.max_result_bytes = 4096;
    let e = src.query(sql("SELECT 1"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422));
    assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result_bytes"), Some(4096)));
    wait_for(|| server.counters.endless_write_failed.load(Ordering::SeqCst) == 1).await;
}

#[tokio::test]
async fn a_ceiling_of_zero_sends_nothing() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = source(config(server.addr));
    let mut l = limits();
    l.max_result_rows = 0;
    l.rows_from_ceiling = true;
    let e = src.query(sql("SELECT 1"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_rows"), Some(0)));
    let mut l = limits();
    l.max_execution_time = Duration::ZERO;
    l.time_from_ceiling = true;
    let e = src.query(sql("SELECT 1"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_duration"), Some(0)));
    assert_eq!(server.request_count(), 0);
    assert_eq!(src.state(), SourceState::Unknown);
}

#[tokio::test]
async fn a_dropped_query_closes_the_connection() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    assert_eq!(src.admission_stats().in_flight, 0);
    let q = src.query(sql("SELECT sleep(30)"), limits(), ctx());
    // The client leaves after 150 ms.
    assert!(tokio::time::timeout(Duration::from_millis(150), q).await.is_err());
    let t = std::time::Instant::now();
    wait_for(|| server.counters.client_closed_during_wait.load(Ordering::SeqCst) == 1).await;
    assert!(t.elapsed() < Duration::from_secs(1), "the server saw the close after {:?}", t.elapsed());
    // The permits are back, and the connection is not in the pool.
    assert_eq!(src.admission_stats().in_flight, 0);
    assert_eq!(src.admission_stats().memory_in_use, 0);
    // The query took the idle connection of the probe and closed it: nothing is idle.
    assert_eq!(src.idle_connections(), 0);
    // The state does not change for a client that left.
    assert_eq!(src.state(), SourceState::Up);
}

#[tokio::test]
async fn an_aborted_task_closes_the_connection() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    let s2 = src.clone();
    let h = tokio::spawn(async move { s2.query(sql("SELECT sleep(30)"), limits(), ctx()).await.map(|_| ()) });
    wait_for(|| server.request_count() >= 3).await;
    h.abort();
    wait_for(|| server.counters.client_closed_during_wait.load(Ordering::SeqCst) == 1).await;
    assert_eq!(src.admission_stats().in_flight, 0);
}

#[tokio::test]
async fn the_client_deadline_is_the_time_limit_plus_5_seconds() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    let mut l = limits();
    l.max_execution_time = Duration::from_secs(1);
    let start = std::time::Instant::now();
    let e = src.query(sql("SELECT sleep(30)"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    let waited = start.elapsed();
    assert!(waited >= Duration::from_millis(5900) && waited < Duration::from_secs(8), "{waited:?}");
    // The deadline closed the connection.
    wait_for(|| server.counters.client_closed_during_wait.load(Ordering::SeqCst) == 1).await;
    assert_eq!(src.admission_stats().in_flight, 0);
    // A deadline is not a failure of the source.
    assert_eq!(src.state(), SourceState::Up);
}

#[tokio::test]
async fn the_client_deadline_from_a_ceiling_is_a_ceiling_error() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    let mut l = limits();
    l.max_execution_time = Duration::from_secs(1);
    l.time_from_ceiling = true;
    let e = src.query(sql("SELECT sleep(30)"), l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
    assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_duration"), Some(1)));
}

#[tokio::test]
async fn a_connection_is_reused() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = source(config(server.addr));
    for _ in 0..3 {
        src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    }
    assert_eq!(server.connections(), 1);
    // The probe and 3 queries: 2 probe statements, then 3 queries.
    assert_eq!(server.request_count(), 5);
}

#[tokio::test]
async fn concurrent_queries_use_one_connection_each() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            let mut v = vec![Step::Sleep(Duration::from_millis(150))];
            v.extend(ok_steps(rec, ONE_COLUMN));
            v
        })
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    let mut hs = Vec::new();
    for _ in 0..3 {
        let s = src.clone();
        hs.push(tokio::spawn(async move { s.query(sql("SELECT 1"), limits(), ctx()).await }));
    }
    for h in hs {
        h.await.unwrap().unwrap();
    }
    assert_eq!(server.connections(), 3);
    assert_eq!(src.idle_connections(), 3);
}

#[tokio::test]
async fn a_connection_that_the_server_closed_is_not_used_again() {
    // The server closes each connection after one answer, as ClickHouse does at `keep_alive_timeout`.
    let server = FakeServer::start(handler(|rec| {
        let mut v = probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| ok_steps(rec, ONE_COLUMN));
        v.push(Step::Close);
        v
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    }
    assert!(server.connections() >= 4);
}

#[tokio::test]
async fn a_reused_connection_that_failed_before_the_write_is_retried_once() {
    // The server closes the connection just after the answer, and the client sends at once, so
    // the stale connection can be found only at the send.
    let server = FakeServer::start(handler(|rec| {
        let mut v = probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| ok_steps(rec, ONE_COLUMN));
        v.push(Step::Close);
        v
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    for _ in 0..20 {
        src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    }
}

#[tokio::test]
async fn a_request_that_was_written_is_never_sent_again() {
    // The server reads the query and closes without an answer.
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| vec![Step::Close])
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    let before = server.request_count();
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
    assert_eq!(server.request_count(), before + 1, "no second send of the query");
    assert_eq!(src.state(), SourceState::Down);
}

#[tokio::test]
async fn a_connection_with_connection_close_is_not_pooled() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| {
            let mut v = ok_steps_with(rec, &[ONE_COLUMN.as_bytes()], &[("Connection", "close".to_owned())]);
            v.push(Step::Close);
            v
        })
    }))
    .await;
    let src = source(config(server.addr));
    src.probe().await;
    assert_eq!(src.idle_connections(), 1);
    src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(src.idle_connections(), 0, "the connection was taken and not given back");
}

#[tokio::test]
async fn a_refused_connection_is_unavailable_and_the_state_is_down() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    let src = source(config(addr));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    // The first query ran the probe, which failed: the state is down, and the query still went out.
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
    assert_eq!(src.state(), SourceState::Down);
    assert_eq!(e.source_id().unwrap().as_str(), "test");
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Down);
    assert!(p.detail.is_some());
}

#[tokio::test]
async fn busy_when_the_source_has_no_free_place() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.max_concurrent = 1;
    cfg.queue_wait = Duration::from_millis(100);
    let src = source(cfg);
    src.probe().await;
    let s2 = src.clone();
    let first = tokio::spawn(async move { s2.query(sql("SELECT sleep(3)"), limits(), ctx()).await });
    wait_for(|| src.admission_stats().in_flight == 1).await;
    let t = std::time::Instant::now();
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable(), e.retry_after_secs()), ("source_busy", 503, true, Some(1)));
    assert_eq!(e.source_id().unwrap().as_str(), "test");
    assert!(t.elapsed() >= Duration::from_millis(80));
    first.abort();
}

#[tokio::test]
async fn busy_per_subject_only_for_that_subject() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| if rec.sql().contains("slow") { vec![Step::Hang] } else { ok_steps(rec, ONE_COLUMN) })
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.max_concurrent = 4;
    cfg.max_concurrent_per_subject = Some(1);
    cfg.queue_wait = Duration::from_millis(100);
    let src = source(cfg);
    src.probe().await;
    let s2 = src.clone();
    let slow = tokio::spawn(async move { s2.query(sql("SELECT slow"), limits(), ctx_for("user:alice")).await });
    wait_for(|| src.admission_stats().in_flight == 1).await;
    let e = src.query(sql("SELECT 1"), limits(), ctx_for("user:alice")).await.unwrap_err();
    assert_eq!(e.code(), "source_busy");
    src.query(sql("SELECT 1"), limits(), ctx_for("user:bob")).await.unwrap();
    slow.abort();
}

#[tokio::test]
async fn busy_when_the_memory_budget_is_used() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.max_result_bytes = 1 << 20;
    cfg.queue_wait = Duration::from_millis(100);
    // One query takes 2 MiB: the budget holds one.
    let budget = hune_source::ResultMemoryBudget::new(3 << 20);
    let src = hune_source::ClickHouseSource::new(cfg, budget.clone()).unwrap();
    src.probe().await;
    let s2 = src.clone();
    let first = tokio::spawn(async move { s2.query(sql("SELECT sleep(3)"), limits(), ctx()).await });
    wait_for(|| budget.in_use() == 2 << 20).await;
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!(e.code(), "source_busy");
    first.abort();
    wait_for(|| budget.in_use() == 0).await;
}

#[tokio::test]
async fn any_source_dispatches() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = AnySource::from(source(config(server.addr)));
    assert_eq!(src.id().as_str(), "test");
    assert_eq!(src.kind().as_str(), "clickhouse");
    assert_eq!(src.state(), SourceState::Unknown);
    let f = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(f.rows, 3);
    assert_eq!(src.state(), SourceState::Up);
    // The trait gives the same.
    use hune_source::DataSource;
    let f = DataSource::query(&src, sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(f.rows, 3);
    assert_eq!(DataSource::probe(&src).await.state, SourceState::Up);
}

#[tokio::test]
async fn the_password_never_shows_in_a_debug_text() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let cfg = config(server.addr);
    assert!(!format!("{cfg:?}").contains("p4ss-secret"));
    let src = source(cfg);
    assert!(!format!("{src:?}").contains("p4ss-secret"));
}

/// Waits at most 3 s for a condition.
pub async fn wait_for(cond: impl Fn() -> bool) {
    let t = std::time::Instant::now();
    while !cond() {
        assert!(t.elapsed() < Duration::from_secs(3), "the condition did not become true");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn a_query_that_waits_for_the_only_place_reports_its_queue_time() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| if rec.sql().contains("slow") { vec![Step::Hang] } else { ok_steps(rec, ONE_COLUMN) })
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.max_concurrent = 1;
    cfg.queue_wait = Duration::from_secs(10);
    let src = source(cfg);
    src.probe().await;
    // The first query holds the only place. Its own queue time is near 0.
    let first_ctx = ctx();
    let first_wait: QueueWait = first_ctx.queue_wait.clone();
    let s1 = src.clone();
    let first = tokio::spawn(async move { s1.query(sql("SELECT slow"), limits(), first_ctx).await });
    wait_for(|| src.admission_stats().in_flight == 1).await;
    assert!(first_wait.get().unwrap() < Duration::from_millis(500), "{:?}", first_wait.get());
    // The second query waits. It gets the place when the first one ends.
    let second_ctx = ctx();
    let second_wait = second_ctx.queue_wait.clone();
    assert_eq!(second_wait.get(), None, "no value before the admission ends");
    let s2 = src.clone();
    let second = tokio::spawn(async move { s2.query(sql("SELECT 1"), limits(), second_ctx).await });
    wait_for(|| src.admission_stats().waiting == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(second_wait.get(), None, "the query still waits");
    first.abort();
    second.await.unwrap().unwrap();
    let waited = second_wait.get().expect("the wait is set");
    assert!(waited >= Duration::from_millis(180), "{waited:?}");
    assert!(waited < Duration::from_secs(5), "{waited:?}");
}

#[tokio::test]
async fn a_refused_query_reports_the_time_that_it_waited() {
    let server = FakeServer::start(handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"]).unwrap_or_else(|| vec![Step::Hang])
    }))
    .await;
    let mut cfg = config(server.addr);
    cfg.max_concurrent = 1;
    cfg.queue_wait = Duration::from_millis(150);
    let src = source(cfg);
    src.probe().await;
    let s2 = src.clone();
    let first = tokio::spawn(async move { s2.query(sql("SELECT sleep(3)"), limits(), ctx()).await });
    wait_for(|| src.admission_stats().in_flight == 1).await;
    let c = ctx();
    let wait = c.queue_wait.clone();
    let e = src.query(sql("SELECT 1"), limits(), c).await.unwrap_err();
    assert_eq!(e.code(), "source_busy");
    let waited = wait.get().expect("a refusal sets the wait");
    assert!(waited >= Duration::from_millis(130) && waited < Duration::from_secs(2), "{waited:?}");
    first.abort();
}

#[tokio::test]
async fn a_query_that_never_reaches_the_admission_has_no_queue_time() {
    let server = FakeServer::start(healthy(ONE_COLUMN)).await;
    let src = source(config(server.addr));
    src.probe().await;
    let mut l = limits();
    l.max_result_rows = 0;
    let c = ctx();
    let wait = c.queue_wait.clone();
    let e = src.query(sql("SELECT 1"), l, c).await.unwrap_err();
    assert_eq!(e.code(), "ceiling_exceeded");
    assert_eq!(wait.get(), None);
}
