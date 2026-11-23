//! Integration tests against a real ClickHouse (16.9, cases C01 to C13 that a local run can do).
//!
//! The tests are `#[ignore]`. The script `tests/ci/source.sh` starts a container on 127.0.0.1 and
//! runs them with `-- --ignored`. The address comes from `HUNE_TEST_CLICKHOUSE` (default
//! `127.0.0.1:18123`) and `HUNE_TEST_CLICKHOUSE_TLS` (default `127.0.0.1:18443`).

mod common;

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hune_core::frame::Frame;
use hune_source::{
    ClickHouseSource, ClickHouseSourceConfig, EffectiveLimits, LogComment, LogKind, PreparedQuery, QueryCtx, QueryKind,
    QueueWait, ResultMemoryBudget, SourceError, SourceState, SourceTls, SubjectId, Uuid,
};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};

const REASON: &str = "needs a ClickHouse container: run tests/ci/source.sh";
const PASSWORD: &str = "hune-test-password";
const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const OTHER_CA: &[u8] = include_bytes!("fixtures/tls/other-ca.pem");

fn addr_from(var: &str, default: &str) -> SocketAddr {
    std::env::var(var).unwrap_or_else(|_| default.to_owned()).parse().expect("a socket address")
}

fn http_addr() -> SocketAddr {
    addr_from("HUNE_TEST_CLICKHOUSE", "127.0.0.1:18123")
}

fn tls_addr() -> SocketAddr {
    addr_from("HUNE_TEST_CLICKHOUSE_TLS", "127.0.0.1:18443")
}

fn cfg(user: &str) -> ClickHouseSourceConfig {
    let a = http_addr();
    ClickHouseSourceConfig {
        id: hune_core::ids::SourceId::parse("ch").unwrap(),
        label: "Test".into(),
        https: false,
        host: a.ip().to_string(),
        port: a.port(),
        user: user.into(),
        password: PASSWORD.into(),
        database: "analytics".into(),
        tls: None,
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 100_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        max_concurrent: 8,
        queue_wait: Duration::from_secs(2),
        max_concurrent_per_subject: None,
    }
}

fn source_for(user: &str) -> ClickHouseSource {
    ClickHouseSource::new(cfg(user), ResultMemoryBudget::new(1 << 31)).unwrap()
}

fn limits() -> EffectiveLimits {
    EffectiveLimits {
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 100_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    }
}

fn ctx() -> QueryCtx {
    QueryCtx {
        kind: QueryKind::Panel,
        query_id: Uuid::now_v7(),
        log_comment: LogComment {
            kind: LogKind::Panel,
            uid: Some("nodes".into()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r-5f2c".into()),
            subject: Some("-".into()),
        },
        subject: SubjectId::open(),
        traceparent: None,
        queue_wait: QueueWait::new(),
    }
}

async fn run(src: &ClickHouseSource, sql: &str) -> Result<Frame, SourceError> {
    src.query(PreparedQuery::ClickHouseSql { sql: sql.to_owned(), params: Default::default() }, limits(), ctx()).await
}

async fn run_with(src: &ClickHouseSource, sql: &str, l: EffectiveLimits, c: QueryCtx) -> Result<Frame, SourceError> {
    src.query(PreparedQuery::ClickHouseSql { sql: sql.to_owned(), params: Default::default() }, l, c).await
}

/// Runs one statement as the administrator on the HTTP interface and gives the text of the answer.
async fn admin(sql: &str) -> String {
    let stream = tokio::net::TcpStream::connect(http_addr()).await.expect("ClickHouse is not reachable");
    let (mut sender, conn) = http1::handshake::<_, Full<Bytes>>(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = hyper::Request::post("/")
        .header("host", http_addr().to_string())
        .header("x-clickhouse-user", "hune_admin")
        .header("x-clickhouse-key", "hune-test-admin-password")
        .body(Full::new(Bytes::from(sql.to_owned())))
        .unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body).into_owned();
    assert!(status.is_success(), "admin statement failed: {text}");
    text
}

async fn wait_until(what: &str, secs: u64, mut cond: impl AsyncFnMut() -> bool) -> Duration {
    let t = Instant::now();
    while !cond().await {
        assert!(t.elapsed() < Duration::from_secs(secs), "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    t.elapsed()
}

async fn in_processes(query_id: &str) -> bool {
    admin(&format!("SELECT count() FROM system.processes WHERE query_id = '{query_id}'")).await.trim() != "0"
}

fn frame_json(f: &Frame) -> Value {
    serde_json::from_str(&f.to_json()).unwrap()
}

// ---------------------------------------------------------------------------------------------
// C01: a seed table with known times gives the expected rows.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c01_seed_rows() {
    let _ = REASON;
    let src = source_for("hune");
    let f =
        run(&src, "SELECT event_time, node, weight, blocks, active FROM analytics.seed ORDER BY event_time LIMIT 3")
            .await
            .unwrap();
    let j = frame_json(&f);
    assert_eq!(j["rows"], 3);
    assert_eq!(
        j["values"],
        json!([
            [1794211200, 1794211220, 1794211240],
            ["v0", "v1", "v2"],
            ["0.5", "1.5", "2.5"],
            [0, 1, 2],
            [true, false, true]
        ])
    );
    let kinds: Vec<&str> = j["columns"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["time", "text", "number_text", "number", "bool"]);
    assert_eq!(j["columns"][1]["type"], "LowCardinality(String)");
    assert_eq!(j["columns"][2]["type"], "Decimal(38, 9)");
    assert_eq!(src.state(), SourceState::Up);
}

// ---------------------------------------------------------------------------------------------
// C07: each type of 7.16 gives its kind and value.

const TYPES_SQL: &str = "SELECT toUInt8(255) AS u8, toInt32(-5) AS i32, toUInt32(4294967295) AS u32,
    toUInt64('18446744073709551615') AS u64max, toInt64('-9223372036854775808') AS i64min,
    toInt128('-170141183460469231731687303715884105728') AS i128min,
    toUInt256('115792089237316195423570985008687907853269984665640564039457584007913129639935') AS u256max,
    toDecimal128('12.5', 9) AS dec, toDecimal32('-1.25', 2) AS dec32, toDecimal256('1', 3) AS dec256,
    toFloat64(1.5) AS f64, toFloat32(0.1) AS f32, nan AS fnan, inf AS finf, -inf AS fninf, toBFloat16(1.5) AS bf,
    true AS b, false AS bf2,
    toDate('2026-11-09') AS d, toDate32('1900-01-01') AS d32, toDateTime(1794211200, 'UTC') AS dt,
    toDateTime64('2026-11-09 00:00:00.125', 3, 'UTC') AS dt64,
    toDateTime('2026-11-09 00:00:00', 'Europe/Paris') AS dtp,
    'text' AS s, toFixedString('ab', 4) AS fs, toUUID('019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d') AS u,
    toIPv4('1.2.3.4') AS ip4, toIPv6('::1') AS ip6, CAST('a' AS Enum8('a' = 1, 'b' = 2)) AS en,
    CAST(NULL AS Nullable(UInt64)) AS nn, CAST('x' AS LowCardinality(String)) AS lc,
    CAST('y' AS LowCardinality(Nullable(String))) AS lcn, [1, 2, 3] AS arr, map('a', 1) AS m, (1, 'x') AS t,
    NULL AS nothing, [toDate('2026-11-09')] AS arrd, INTERVAL 1 DAY AS iv";

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c07_every_type_gives_its_kind_and_value() {
    let src = source_for("hune");
    let f = run(&src, TYPES_SQL).await.unwrap();
    let j = frame_json(&f);
    // (name, type, kind, value)
    let expect: Vec<(&str, &str, &str, Value)> = vec![
        ("u8", "UInt8", "number", json!(255)),
        ("i32", "Int32", "number", json!(-5)),
        ("u32", "UInt32", "number", json!(4294967295u64)),
        ("u64max", "UInt64", "number_text", json!("18446744073709551615")),
        ("i64min", "Int64", "number_text", json!("-9223372036854775808")),
        ("i128min", "Int128", "number_text", json!("-170141183460469231731687303715884105728")),
        (
            "u256max",
            "UInt256",
            "number_text",
            json!("115792089237316195423570985008687907853269984665640564039457584007913129639935"),
        ),
        ("dec", "Decimal(38, 9)", "number_text", json!("12.5")),
        ("dec32", "Decimal(9, 2)", "number_text", json!("-1.25")),
        ("dec256", "Decimal(76, 3)", "number_text", json!("1")),
        ("f64", "Float64", "number", json!(1.5)),
        ("f32", "Float32", "number", json!(0.1)),
        ("fnan", "Float64", "number", json!("nan")),
        ("finf", "Float64", "number", json!("inf")),
        ("fninf", "Float64", "number", json!("-inf")),
        ("bf", "BFloat16", "number", json!(1.5)),
        ("b", "Bool", "bool", json!(true)),
        ("bf2", "Bool", "bool", json!(false)),
        ("d", "Date", "time", json!(1794182400)),
        ("d32", "Date32", "time", json!(-2208988800i64)),
        ("dt", "DateTime('UTC')", "time", json!(1794211200)),
        ("dt64", "DateTime64(3, 'UTC')", "time", json!(1794182400.125)),
        ("dtp", "DateTime('Europe/Paris')", "time", json!(1794178800)),
        ("s", "String", "text", json!("text")),
        ("fs", "FixedString(4)", "text", json!("ab\u{0}\u{0}")),
        ("u", "UUID", "text", json!("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d")),
        ("ip4", "IPv4", "text", json!("1.2.3.4")),
        ("ip6", "IPv6", "text", json!("::1")),
        ("en", "Enum8('a' = 1, 'b' = 2)", "text", json!("a")),
        ("nn", "Nullable(UInt64)", "number_text", Value::Null),
        ("lc", "LowCardinality(String)", "text", json!("x")),
        ("lcn", "LowCardinality(Nullable(String))", "text", json!("y")),
        ("arr", "Array(UInt8)", "text", json!("[1,2,3]")),
        ("m", "Map(String, UInt8)", "text", json!("{\"a\":1}")),
        ("t", "Tuple(UInt8, String)", "text", json!("[1,\"x\"]")),
        ("nothing", "Nullable(Nothing)", "null", Value::Null),
        ("arrd", "Array(Date)", "text", json!("[\"2026-11-09\"]")),
        ("iv", "IntervalDay", "text", json!("1")),
    ];
    let cols = j["columns"].as_array().unwrap();
    assert_eq!(cols.len(), expect.len());
    for (i, (name, ty, kind, value)) in expect.iter().enumerate() {
        assert_eq!(cols[i]["name"], *name);
        assert_eq!(cols[i]["type"], *ty, "{name}");
        assert_eq!(cols[i]["kind"], *kind, "{name}");
        assert_eq!(j["values"][i][0], *value, "{name}");
    }
    // The 64-bit and larger integers are exact in the JSON text, not as numbers.
    let text = f.to_json();
    assert!(text.contains("\"18446744073709551615\""));
    assert!(text.contains("1794182400.125"), "the digits of a DateTime64 stay as they are");
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c07_simple_aggregate_function_and_nulls_in_a_table() {
    let src = source_for("hune");
    let f = run(&src, "SELECT k, total, peak FROM analytics.types_extra ORDER BY k").await.unwrap();
    let j = frame_json(&f);
    assert_eq!(j["columns"][1]["kind"], "number_text");
    assert_eq!(j["columns"][2]["kind"], "number");
    assert_eq!(j["values"], json!([[1, 2], ["18446744073709551615", "7"], [2.5, null]]));
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn the_stats_come_from_the_summary_header() {
    let src = source_for("hune");
    let f = run(&src, "SELECT * FROM analytics.seed").await.unwrap();
    assert_eq!(f.rows, 30);
    assert_eq!(f.stats.read_rows, Some(30));
    assert!(f.stats.read_bytes.unwrap() > 0);
    assert!(f.stats.result_bytes.is_some());
    assert!(f.stats.elapsed_ms < 10_000);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn a_result_of_100000_rows_streams_into_a_frame() {
    let src = source_for("hune");
    let f = run(&src, "SELECT number, toString(number) AS s, number / 3 AS x FROM numbers(100000)").await.unwrap();
    assert_eq!(f.rows, 100_000);
    assert_eq!(f.columns[0].value(99_999), Some(hune_core::frame::FrameValue::Text("99999")));
    assert!(f.approx_bytes() < 8 << 20, "{}", f.approx_bytes());
}

// ---------------------------------------------------------------------------------------------
// C04: limits.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c04_result_rows() {
    let src = source_for("hune");
    let mut l = limits();
    l.max_result_rows = 10;
    let e = run_with(&src, "SELECT number FROM numbers(100)", l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422));
    assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result"), None));
    // The same, when a ceiling lowered the limit.
    l.rows_from_ceiling = true;
    let e = run_with(&src, "SELECT number FROM numbers(100)", l, ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
    assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_rows"), Some(10)));
    // A result at the limit passes.
    let mut l = limits();
    l.max_result_rows = 10;
    assert_eq!(run_with(&src, "SELECT number FROM numbers(10)", l, ctx()).await.unwrap().rows, 10);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c04_result_bytes() {
    let src = source_for("hune");
    let mut l = limits();
    l.max_result_bytes = 1000;
    let e = run_with(&src, "SELECT toString(number) FROM numbers(10000)", l, ctx()).await.unwrap_err();
    assert_eq!(e.code(), "query_limit");
    // ClickHouse (code 396) or the byte check of Hune: both are query_limit and never a cut result.
    assert!(matches!(e.limit_name(), Some("max_result" | "max_result_bytes")), "{:?}", e.limit_name());
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c04_execution_time() {
    let src = source_for("hune");
    let mut l = limits();
    l.max_execution_time = Duration::from_secs(1);
    let t = Instant::now();
    let e = run_with(&src, "SELECT sleepEachRow(0.3) FROM numbers(20) SETTINGS max_block_size = 1", l, ctx())
        .await
        .unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    assert!(t.elapsed() < Duration::from_secs(5));
    l.time_from_ceiling = true;
    let e = run_with(&src, "SELECT sleepEachRow(0.3) FROM numbers(20) SETTINGS max_block_size = 1", l, ctx())
        .await
        .unwrap_err();
    assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_duration"), Some(1)));
    // A query that ends in time passes.
    assert!(run(&src, "SELECT sleepEachRow(0.1) FROM numbers(3) SETTINGS max_block_size = 1").await.is_ok());
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c04_query_size() {
    let src = source_for("hune");
    let mut l = limits();
    l.max_query_size = 262_144;
    let big = format!("SELECT 1 /* {} */", "x".repeat(300_000));
    let e = run_with(&src, &big, l, ctx()).await.unwrap_err();
    // Hune checks the size first (`sql_too_large`, 7.13). The setting is the second guard.
    assert_eq!((e.code(), e.http_status()), ("query_error", 422));
    assert!(e.message().to_lowercase().contains("max query size"), "{}", e.message());
    // A text of 71 KB passes: the SQL is in the body.
    let ok = format!("SELECT 1 /* {} */", "x".repeat(71_000));
    assert_eq!(run(&src, &ok).await.unwrap().rows, 1);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn rows_and_bytes_to_read_and_memory() {
    let src = source_for("hune");
    let e = run(&src, "SELECT * FROM analytics.seed SETTINGS max_rows_to_read = 5").await.unwrap_err();
    assert_eq!((e.code(), e.limit_name(), e.limit_value()), ("query_limit", Some("max_rows_to_read"), None));
    let e = run(&src, "SELECT * FROM analytics.seed SETTINGS max_bytes_to_read = 10").await.unwrap_err();
    assert_eq!((e.code(), e.limit_name()), ("query_limit", Some("max_bytes_to_read")));
    let e = run(&src, "SELECT groupArray(number) FROM numbers(10000000) SETTINGS max_memory_usage = 1000000")
        .await
        .unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_memory_limit", 422));
    assert!(e.members().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Error mapping against the real server.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn clickhouse_errors_map_as_8_5() {
    let src = source_for("hune");
    let e = run(&src, "SELEC 1").await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.clickhouse_code()), ("query_error", 422, Some(62)));
    assert!(e.message().starts_with("Code: 62."), "{}", e.message());
    assert!(!e.message().contains('\n'));
    assert!(e.message().len() <= 1000);
    let e = run(&src, "SELECT * FROM analytics.no_such_table").await.unwrap_err();
    assert_eq!(e.clickhouse_code(), Some(60));
    let e = run(&src, "SELECT throwIf(1, 'boom')").await.unwrap_err();
    assert_eq!(e.code(), "query_error");
    // The members of 6.8 for query_error.
    let names: Vec<_> = e.members().into_iter().map(|m| m.0).collect();
    assert_eq!(names, ["source", "clickhouse_code", "query_id"]);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn access_denied_is_a_source_error() {
    let src = source_for("hune_narrow");
    let e = run(&src, "SELECT count() FROM analytics.secret").await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_access_denied", 502, false));
    assert!(e.query_id().is_some());
    // The account can still read the table that it owns.
    assert_eq!(run(&src, "SELECT count() FROM analytics.seed").await.unwrap().rows, 1);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn a_wrong_password_or_user_makes_the_source_misconfigured() {
    let mut c = cfg("hune");
    c.password = "wrong".into();
    let src = ClickHouseSource::new(c, ResultMemoryBudget::new(1 << 31)).unwrap();
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    let e = run(&src, "SELECT 1").await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_misconfigured", 502, false));
    assert!(!e.message().contains("wrong"));
    let src = source_for("nobody");
    assert_eq!(src.probe().await.state, SourceState::Misconfigured);
}

// ---------------------------------------------------------------------------------------------
// C05: cancellation.

async fn cancel_case(warm_up: bool) {
    let src = source_for("hune");
    assert_eq!(src.probe().await.state, SourceState::Up);
    if warm_up {
        // A kept HTTP/1.1 connection: the long query goes on a connection that served a query.
        assert_eq!(run(&src, "SELECT 1").await.unwrap().rows, 1);
        assert!(src.idle_connections() >= 1);
    }
    let c = ctx();
    let id = c.query_id.to_string();
    let s2 = src.clone();
    let task = tokio::spawn(async move {
        run_with(&s2, "SELECT sleepEachRow(0.1) FROM numbers(300) SETTINGS max_block_size = 1", limits(), c)
            .await
            .map(|_| ())
    });
    wait_until("the query shows in system.processes", 10, async || in_processes(&id).await).await;
    // The client leaves: the future of the query is dropped.
    task.abort();
    let t = Instant::now();
    while in_processes(&id).await {
        assert!(t.elapsed() < Duration::from_secs(2), "the query is still in system.processes after 2 s");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    println!("C05 (warm_up = {warm_up}): the query left system.processes after {:?}", t.elapsed());
    assert_eq!(src.admission_stats().in_flight, 0);
    assert_eq!(src.admission_stats().memory_in_use, 0);
    // The source still works.
    assert_eq!(run(&src, "SELECT 1").await.unwrap().rows, 1);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c05_cancellation_within_2_seconds() {
    cancel_case(false).await;
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c05_cancellation_on_a_kept_connection() {
    cancel_case(true).await;
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c05_cancellation_by_the_timeout_of_the_caller() {
    let src = source_for("hune");
    src.probe().await;
    let c = ctx();
    let id = c.query_id.to_string();
    let q = run_with(&src, "SELECT sleepEachRow(0.1) FROM numbers(300) SETTINGS max_block_size = 1", limits(), c);
    assert!(tokio::time::timeout(Duration::from_millis(700), q).await.is_err());
    let t = Instant::now();
    while in_processes(&id).await {
        assert!(t.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// ---------------------------------------------------------------------------------------------
// C06: the read-only guard: readonly = 0, 1 and 2.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c06_readonly_0_1_2() {
    // readonly = 2: up.
    let src = source_for("hune");
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Up, "{:?}", p.detail);
    assert!(p.version.as_deref().is_some_and(|v| v.contains('.')));
    // readonly = 0: the account can write. The probe finds `readonly` is not 2.
    let src = source_for("hune_ro0");
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("readonly = 0"));
    let e = run(&src, "SELECT 1").await.unwrap_err();
    assert_eq!(e.code(), "source_misconfigured");
    // readonly = 1: ClickHouse refuses the settings of 8.3 (code 164).
    let src = source_for("hune_ro1");
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("readonly"));
}

// ---------------------------------------------------------------------------------------------
// C08: system.query_log holds the query_id and the log_comment.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c08_query_log_has_the_query_id_and_the_log_comment() {
    let src = source_for("hune");
    let c = ctx();
    let expect_comment = c.log_comment.to_json();
    let id = c.query_id.to_string();
    run_with(&src, "SELECT 42 AS answer", limits(), c).await.unwrap();
    let probe = src.last_probe().unwrap();
    assert_eq!(probe.state, SourceState::Up);
    admin("SYSTEM FLUSH LOGS").await;
    let got = admin(&format!(
        "SELECT log_comment, JSONExtractString(log_comment, 'app'), JSONExtractString(log_comment, 'uid') FROM system.query_log WHERE query_id = '{id}' AND type = 'QueryFinish' FORMAT TSV"
    ))
    .await;
    let fields: Vec<&str> = got.trim_end().split('\t').collect();
    assert_eq!(fields[0], expect_comment.replace('\\', "\\\\"), "{got}");
    assert_eq!((fields[1], fields[2]), ("hune", "nodes"));
    // The probe statements carry the comment of a probe.
    let n = admin("SELECT count() FROM system.query_log WHERE type = 'QueryFinish' AND log_comment = '{\"app\":\"hune\",\"kind\":\"probe\"}' AND query LIKE 'SHOW GRANTS%'").await;
    assert!(n.trim().parse::<u64>().unwrap() >= 1, "{n}");
    // The query id is a UUID v7 that joins the log.
    assert_eq!(Uuid::parse(&id).unwrap().version(), 7);
}

// ---------------------------------------------------------------------------------------------
// C09: the account contract of 8.12.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c09_account_contract() {
    let src = source_for("hune");
    let one = |f: &Frame| frame_json(f)["values"][0][0].clone();
    assert_eq!(one(&run(&src, "SELECT getSetting('readonly')").await.unwrap()), json!(2));
    for (sql, expect) in [
        ("CHECK GRANT SELECT ON analytics.*", 1),
        ("CHECK GRANT INSERT ON analytics.*", 0),
        ("CHECK GRANT ALTER ON analytics.*", 0),
        ("CHECK GRANT CREATE ON *.*", 0),
        ("CHECK GRANT DROP ON *.*", 0),
        ("CHECK GRANT SOURCES ON *.*", 0),
        ("CHECK GRANT URL ON *.*", 0),
        ("CHECK GRANT S3 ON *.*", 0),
        ("CHECK GRANT REMOTE ON *.*", 0),
        ("CHECK GRANT FILE ON *.*", 0),
        ("CHECK GRANT MONGO ON *.*", 0),
        ("CHECK GRANT KILL QUERY ON *.*", 0),
        ("CHECK GRANT ACCESS MANAGEMENT ON *.*", 0),
        ("CHECK GRANT SYSTEM ON *.*", 0),
    ] {
        assert_eq!(one(&run(&src, sql).await.unwrap()), json!(expect), "{sql}");
    }
    let f = run(&src, "SHOW GRANTS").await.unwrap();
    let j = frame_json(&f);
    assert_eq!(j["values"], json!([["GRANT SELECT ON analytics.* TO hune"]]));
    // The probe passes the contract for this account, and fails it for an account with `GRANT URL`.
    assert_eq!(src.probe().await.state, SourceState::Up);
    let extra = source_for("hune_extra");
    let p = extra.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("URL"));
    // An account that reads one table passes the contract too.
    assert_eq!(source_for("hune_narrow").probe().await.state, SourceState::Up);
}

// ---------------------------------------------------------------------------------------------
// C12: a SETTINGS clause cannot break the CONST constraints or go above a MAX.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c12_settings_clause_is_refused() {
    let src = source_for("hune");
    for sql in [
        "SELECT 1 SETTINGS max_result_rows = 100001",
        "SELECT 1 SETTINGS max_execution_time = 61",
        "SELECT 1 SETTINGS max_result_bytes = 33554433",
        "SELECT 1 SETTINGS max_query_size = 1048577",
        "SELECT 1 SETTINGS result_overflow_mode = 'break'",
        "SELECT 1 SETTINGS timeout_overflow_mode = 'break'",
        "SELECT 1 SETTINGS read_overflow_mode = 'break'",
    ] {
        let e = run(&src, sql).await.unwrap_err();
        assert_eq!((e.code(), e.clickhouse_code()), ("query_error", Some(452)), "{sql}: {e}");
    }
    // A change of `readonly` itself is refused with code 164 (READONLY).
    let e = run(&src, "SELECT 1 SETTINGS readonly = 0").await.unwrap_err();
    assert_eq!((e.code(), e.clickhouse_code()), ("query_error", Some(164)));
    // A value inside the constraints is fine.
    assert!(run(&src, "SELECT 1 SETTINGS max_result_rows = 10").await.is_ok());
    // An account whose profile has a lower MAX than the key of the source: the probe says so.
    let low = source_for("hune_lowmax");
    let p = low.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured, "{:?}", p.detail);
    assert!(p.detail.unwrap().contains("constraint"));
}

// ---------------------------------------------------------------------------------------------
// C13: the pool never sends on a connection that ClickHouse closed.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c13_query_after_15_seconds_of_silence() {
    let src = source_for("hune");
    assert_eq!(run(&src, "SELECT 1").await.unwrap().rows, 1);
    assert!(src.idle_connections() >= 1);
    // Hune closes an idle connection after 5 s, before ClickHouse does (keep_alive_timeout 10 s).
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(src.idle_connections(), 0, "the reaper closed the idle connection");
    tokio::time::sleep(Duration::from_secs(9)).await;
    // The first attempt succeeds.
    assert_eq!(run(&src, "SELECT 2").await.unwrap().rows, 1);
    assert_eq!(src.state(), SourceState::Up);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c13_a_connection_that_clickhouse_closed_is_not_used() {
    // Without the reaper, a connection of 12 s of age would be closed by the server. Here the pool
    // is used right after 10.5 s: the query must still succeed on the first attempt.
    let src = source_for("hune");
    run(&src, "SELECT 1").await.unwrap();
    tokio::time::sleep(Duration::from_millis(10_500)).await;
    let t = Instant::now();
    run(&src, "SELECT 2").await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(5));
}

// ---------------------------------------------------------------------------------------------
// C10: TLS with the test CA.

fn tls_cfg(ca: &[u8], name: &str) -> ClickHouseSourceConfig {
    let a = tls_addr();
    let mut c = cfg("hune");
    c.https = true;
    c.host = a.ip().to_string();
    c.port = a.port();
    c.tls = Some(SourceTls { ca_pem: ca.to_vec(), server_name: name.to_owned() });
    c
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c10_tls_with_the_test_ca() {
    let src = ClickHouseSource::new(tls_cfg(CA, "clickhouse-test"), ResultMemoryBudget::new(1 << 31)).unwrap();
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Up, "{:?}", p.detail);
    assert_eq!(run(&src, "SELECT count() FROM analytics.seed").await.unwrap().rows, 1);
    // The IP address is in the SAN too.
    let src = ClickHouseSource::new(tls_cfg(CA, "127.0.0.1"), ResultMemoryBudget::new(1 << 31)).unwrap();
    assert_eq!(src.probe().await.state, SourceState::Up);
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c10_a_wrong_server_name_or_ca_stops_the_connection() {
    let src = ClickHouseSource::new(tls_cfg(CA, "wrong-name"), ResultMemoryBudget::new(1 << 31)).unwrap();
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    let e = run(&src, "SELECT 1").await.unwrap_err();
    assert_eq!(e.code(), "source_misconfigured");
    // The CA of another authority: no fallback to a system store.
    let src = ClickHouseSource::new(tls_cfg(OTHER_CA, "clickhouse-test"), ResultMemoryBudget::new(1 << 31)).unwrap();
    assert_eq!(src.probe().await.state, SourceState::Misconfigured);
}

// ---------------------------------------------------------------------------------------------
// C11 (partly): a ClickHouse that is down.

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn c11_a_source_that_is_down_gives_a_retryable_error() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    drop(l);
    let mut c = cfg("hune");
    c.host = a.ip().to_string();
    c.port = a.port();
    let src = ClickHouseSource::new(c, ResultMemoryBudget::new(1 << 31)).unwrap();
    let e = run(&src, "SELECT 1").await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
    assert_eq!(src.state(), SourceState::Down);
}

// ---------------------------------------------------------------------------------------------
// C30 to C32 and C35 (F27): the variables `interval` and `constant` against the real ClickHouse.
// C33 and C34 go through the routes of the server: `crates/hune/src/server/tests/real_ch.rs`.

mod f27 {
    use super::*;
    use hune_core::sql::{
        AUTO_TABLE, ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, TextValueType, TimeRange, ValuesError,
        VariableDef, check_time_range, compute_interval, expand, resolve_values_at,
    };

    pub const FROM: i64 = 1_794_211_200_000;
    pub const HOSTILE: &str = "a'b\"c$d\\e;--x";

    pub fn range_of(len_ms: i64) -> TimeRange {
        check_time_range(FROM, FROM + len_ms).unwrap()
    }

    /// Expands `sql` for the definitions and the request, as the server does, over a range of `len_ms`.
    pub fn expand_for(
        defs: &[VariableDef],
        req: &RequestValues,
        sql: &str,
        len_ms: i64,
    ) -> Result<String, ExpandError> {
        let range = range_of(len_ms);
        let resolved = resolve_values_at(defs, req, &range).map_err(|e| match e {
            ValuesError::Refused(r) => ExpandError::ValueRefused(r),
            ValuesError::BadRequest(m) => panic!("bad request: {m}"),
        })?;
        let interval = Some(compute_interval(&range, 1200, 1).unwrap());
        expand(&ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 1 << 20 })
    }

    /// The first value of the column `n` of a frame, as the JSON of the frame gives it.
    pub fn cell(j: &Value, n: usize) -> &Value {
        &j["values"][n][0]
    }

    pub fn names(j: &Value) -> Vec<String> {
        j["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect()
    }

    // ---- C30 ----

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c30_parse_time_delta_of_each_item_form() {
        let src = source_for("hune");
        // The five items of the case, then the 30 texts of the table of 7.4a (the values of "Auto").
        let fixed: [(&str, f64); 5] =
            [("1m", 60.0), ("1d", 86_400.0), ("1w", 604_800.0), ("1y", 31_536_000.0), ("500ms", 0.5)];
        let mut all: Vec<(String, f64)> = fixed.iter().map(|(t, s)| ((*t).to_owned(), *s)).collect();
        all.extend(AUTO_TABLE.iter().map(|(ms, t)| ((*t).to_owned(), *ms as f64 / 1000.0)));
        let items: Vec<&str> = all.iter().map(|(t, _)| t.as_str()).collect();
        let defs = [VariableDef::interval("bucket", &items, None)];
        for (text, seconds) in &all {
            let mut req = RequestValues::new();
            req.insert("bucket", false, &[text.as_str()]);
            let sql = expand_for(&defs, &req, "SELECT parseTimeDelta('${bucket}') AS s, '${bucket}' AS t", 3_600_000)
                .unwrap();
            assert_eq!(sql, format!("SELECT parseTimeDelta('{text}') AS s, '{text}' AS t"));
            let j = frame_json(&run(&src, &sql).await.unwrap());
            assert_eq!(j["rows"], 1, "{text}");
            let got = cell(&j, 0).as_f64().unwrap_or_else(|| panic!("{text}: {j}"));
            assert!((got - seconds).abs() < 1e-9, "{text}: ClickHouse gave {got}, 7.4a gives {seconds}");
            assert_eq!(cell(&j, 1), text.as_str());
        }
        println!(
            "C30: {} item forms checked ({} in the case, {} of the table of 7.4a)",
            all.len(),
            fixed.len(),
            AUTO_TABLE.len()
        );
    }

    // ---- C31 ----

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c31_the_text_of_auto_on_a_real_range() {
        let src = source_for("hune");
        // `auto_count` 30 and `auto_min` `10s`. Each row: the range in ms, then the text of the table of 7.4a that
        // the rule gives (R = range / 30, I = max(10 s, round_interval(R))). The texts are computed by hand.
        let rows: [(i64, &str); 6] = [
            (1_000, "10s"),      // R = 33 ms, round_interval = 20 ms: the minimum gives 10 s
            (300_000, "10s"),    // R = 10 s
            (3_600_000, "2m"),   // R = 2 min
            (21_600_000, "10m"), // R = 12 min
            (86_400_000, "1h"),  // R = 48 min
            (604_800_000, "6h"), // R = 5 h 36 min
        ];
        let defs = [VariableDef::interval("bucket", &["1m", "10m"], Some((30, "10s")))];
        for (len, text) in rows {
            let mut req = RequestValues::new();
            req.insert("bucket", false, &["$__auto"]);
            let sql =
                expand_for(&defs, &req, "SELECT '${bucket}' AS t, parseTimeDelta('${bucket}') AS s", len).unwrap();
            let j = frame_json(&run(&src, &sql).await.unwrap());
            assert_eq!(cell(&j, 0), text, "range {len} ms");
            let table = AUTO_TABLE.iter().find(|(_, t)| *t == text).expect("a text of the table");
            let got = cell(&j, 1).as_f64().unwrap();
            assert!((got - table.0 as f64 / 1000.0).abs() < 1e-9, "range {len} ms: {j}");
        }
        println!("C31: 6 ranges, the statement returned the text of 7.4a for each");
    }

    // ---- C32 ----

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c32_a_hostile_constant() {
        let src = source_for("hune");
        let none = RequestValues::new();
        let defs = [VariableDef::constant("k", HOSTILE)];
        // In a string: escaped, one row, the text comes back unchanged.
        let sql =
            expand_for(&defs, &none, "SELECT '${k}' AS x, count() AS n FROM analytics.seed WHERE '${k}' != ''", 1000)
                .unwrap();
        let j = frame_json(&run(&src, &sql).await.unwrap());
        assert_eq!(j["rows"], 1, "{sql}");
        assert_eq!(cell(&j, 0), HOSTILE);
        assert_eq!(cell(&j, 1), "30");
        // In a quoted name (two quotes): the name escapes hold, the column has the text as its name.
        for (open, label) in [("\"", "double quote"), ("`", "backtick")] {
            let sql = expand_for(&defs, &none, &format!("SELECT 1 AS {open}${{k}}{open}"), 1000).unwrap();
            let j = frame_json(&run(&src, &sql).await.unwrap());
            assert_eq!(j["rows"], 1, "{label}: {sql}");
            assert_eq!(names(&j), [HOSTILE], "{label}: {sql}");
        }
        // In code: the text stays as it is, and the check of the final SQL refuses it because it opens a string.
        let e = expand_for(&defs, &none, "SELECT ${k} -- x", 1000).unwrap_err();
        assert!(matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Lexer), "{e:?}");
        // A constant that tries to end the statement: it stays in its literal, one row, no second statement.
        let attack = [VariableDef::constant("k", "x'; DROP TABLE analytics.seed; SELECT '1")];
        let sql = expand_for(&attack, &none, "SELECT '${k}' AS x", 1000).unwrap();
        let j = frame_json(&run(&src, &sql).await.unwrap());
        assert_eq!(
            (j["rows"].clone(), cell(&j, 0).clone()),
            (json!(1), json!("x'; DROP TABLE analytics.seed; SELECT '1"))
        );
        assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
        // The same text in a name.
        let sql = expand_for(&attack, &none, "SELECT 1 AS `${k}`", 1000).unwrap();
        let j = frame_json(&run(&src, &sql).await.unwrap());
        assert_eq!(names(&j), ["x'; DROP TABLE analytics.seed; SELECT '1"]);
        assert_eq!(admin("SELECT count() FROM analytics.seed").await.trim(), "30");
    }

    // ---- C35 ----

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    // The pieces of the random tests of 16.4 (no control character: the check of 7.7 refuses those).
    const PIECES: &[&str] = &[
        "'", "\"", "`", "\\", "$", "${", "$__from", "--", "/*", "*/", ";", "(", ")", " ", "a", "1", ",", "\u{e9}", "%",
    ];

    fn hostile_text(rng: &mut Rng) -> String {
        let n = 1 + rng.below(8);
        (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect()
    }

    fn env_number(name: &str) -> Option<u64> {
        std::env::var(name).ok()?.parse().ok()
    }

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c35_seeded_random_interval_and_constant_against_the_real_server() {
        let src = source_for("hune");
        let cases = env_number("HUNE_TEST_CASES").unwrap_or(500) as usize;
        let seeds: Vec<u64> = match env_number("HUNE_TEST_SEED") {
            Some(s) => vec![s],
            None => (1..=env_number("HUNE_TEST_SEEDS").unwrap_or(4)).collect(),
        };
        let items = ["1m", "10m", "1h", "1d"];
        let (mut sent, mut refused) = (0_usize, 0_usize);
        for seed in &seeds {
            let mut rng = Rng(*seed);
            for case in 0..cases {
                let doc_const = hostile_text(&mut rng);
                let defs = vec![
                    VariableDef::interval("iv", &items, Some((30, "10s"))),
                    VariableDef::constant("k", &doc_const),
                    VariableDef::text("t", TextValueType::Text),
                    VariableDef::query("q", false, false, None),
                ];
                let iv_value = match rng.below(4) {
                    0 => items[rng.below(items.len())].to_owned(),
                    1 => "$__auto".to_owned(),
                    _ => hostile_text(&mut rng),
                };
                let iv_ok = items.contains(&iv_value.as_str()) || iv_value == "$__auto";
                let k_member = match rng.below(3) {
                    0 => None,
                    1 => Some(doc_const.clone()),
                    _ => Some(hostile_text(&mut rng)),
                };
                let k_ok = k_member.as_ref().is_none_or(|m| *m == doc_const);
                let t_value = hostile_text(&mut rng);
                let q_value = hostile_text(&mut rng);
                let mut req = RequestValues::new();
                req.insert("iv", false, &[iv_value.as_str()]);
                if let Some(m) = &k_member {
                    req.insert("k", false, &[m.as_str()]);
                }
                req.insert("t", false, &[t_value.as_str()]);
                req.insert("q", false, &[q_value.as_str()]);
                let label = format!(
                    "seed {seed} case {case}: iv {iv_value:?} k {k_member:?} const {doc_const:?} t {t_value:?} q {q_value:?}"
                );
                // The six hours of the other random tests: "Auto" is `10m`.
                let sql = "SELECT '${iv}' AS ri, '${k}' AS rk, '${t}' AS rt, '${q}' AS rq, 1 AS `${k}`";
                match expand_for(&defs, &req, sql, 21_600_000) {
                    Err(ExpandError::ValueRefused(r)) => {
                        assert!(!(iv_ok && k_ok), "{label}: refused {r:?}");
                        refused += 1;
                    }
                    Err(other) => panic!("{label}: {other:?}"),
                    Ok(out) => {
                        assert!(iv_ok && k_ok, "{label}: accepted");
                        let j = frame_json(&run(&src, &out).await.unwrap_or_else(|e| panic!("{label}: {e:?}\n{out}")));
                        assert_eq!(j["rows"], 1, "{label}: {out}");
                        let expected_iv = if iv_value == "$__auto" { "10m" } else { iv_value.as_str() };
                        assert_eq!(cell(&j, 0), expected_iv, "{label}: {out}");
                        assert_eq!(cell(&j, 1), doc_const.as_str(), "{label}: {out}");
                        assert_eq!(cell(&j, 2), t_value.as_str(), "{label}: {out}");
                        assert_eq!(cell(&j, 3), q_value.as_str(), "{label}: {out}");
                        let n = names(&j);
                        // A column name that the name escapes hold: the text of the constant.
                        assert_eq!(n[4], doc_const, "{label}: {out}");
                        sent += 1;
                    }
                }
            }
        }
        println!(
            "C35: {} seeds x {cases} cases: {sent} statements ran on the real ClickHouse, {refused} refused before it",
            seeds.len()
        );
        assert!(sent > 0);
    }
}

// ---------------------------------------------------------------------------------------------
// C40 and C41 (F39, 7.6a, decision ID-K330): the query parameters of ClickHouse against the real server.

mod f39 {
    use super::*;
    use hune_core::sql::{
        Dialect, ExpandError, ExpandRequest, ParamType, Prepared, RequestValues, SqlInvalidKind, TextValueType,
        VariableDef, check_time_range, expand_prepared, params::escaped_text, resolve_values,
    };

    /// The hostile set of strings: quote marks, backslashes, brackets, commas, Unicode, the escapes of ClickHouse as
    /// text, and texts of SQL and of URLs.
    fn strings() -> Vec<String> {
        let mut out: Vec<String> = [
            "",
            "a",
            "'",
            "''",
            "\\",
            "\\\\",
            "\\'",
            "'\\",
            "]",
            "[",
            ",",
            "', '",
            "['a']",
            "}",
            "{x:String}",
            "$x",
            "${x}",
            "é",
            "😀",
            "\u{2028}",
            "\u{a0}",
            "\u{feff}",
            "a b",
            "%",
            "%41",
            "&param_x=1",
            "+",
            "=",
            "?",
            "#",
            "\\N",
            "\\t",
            "\\n",
            "\\0",
            "\\x41",
            "NULL",
            "--",
            "/*",
            "*/",
            ";",
            "\"",
            "`",
            "\u{80}\u{9f}",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        // Two long values: each one alone stays below the cap of 4,096 bytes of 7.6a after the encoding.
        out.push("\\'".repeat(300));
        out.push("é".repeat(512));
        out
    }

    fn upper_hex(s: &str) -> String {
        s.bytes().map(|b| format!("{b:02X}")).collect()
    }

    /// Expands `sql` with the values as the server does: the checks of 7.7, then 7.6a.
    fn prepare(defs: &[VariableDef], items: &[(&str, &[&str])], sql: &str) -> Prepared {
        let mut req = RequestValues::new();
        for (n, v) in items {
            req.insert(n, false, v);
        }
        let values = resolve_values(defs, &req).expect("the values of 7.7");
        let request = ExpandRequest {
            sql,
            values: &values,
            range: check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap(),
            interval: None,
            max_query_size: 1 << 20,
        };
        expand_prepared(Dialect::ClickHouse, &request).unwrap_or_else(|e| panic!("{sql} {items:?}: {e:?}"))
    }

    async fn first_text(src: &ClickHouseSource, p: Prepared) -> Vec<String> {
        let label = format!("{} {:?}", p.sql, p.params);
        let frame = src
            .query(PreparedQuery::ClickHouseSql { sql: p.sql, params: p.params }, limits(), ctx())
            .await
            .unwrap_or_else(|e| panic!("{label}: {e:?}"));
        let j = frame_json(&frame);
        assert_eq!(j["rows"], 1, "{label}: {j}");
        (0..j["columns"].as_array().unwrap().len())
            .map(|n| j["values"][n][0].as_str().unwrap_or("").to_owned())
            .collect()
    }

    // ---- C40 ----

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c40_each_type_round_trips_a_hostile_set() {
        let src = source_for("hune");
        let defs = [VariableDef::text("t", TextValueType::Text), VariableDef::query("m", true, false, None)];
        let corpus = strings();
        // String: the bytes come back unchanged, and the type is String.
        for v in &corpus {
            let p = prepare(&defs, &[("t", &[v])], "SELECT hex({t:String}) AS h, toTypeName({t:String}) AS ty");
            assert_eq!(first_text(&src, p).await, [upper_hex(v), "String".to_owned()], "{v:?}");
        }
        // Array(String): the short values as one list, then each value alone, then the empty list.
        let short = &corpus[..corpus.len() - 2];
        let refs: Vec<&str> = short.iter().map(String::as_str).collect();
        let sql = "SELECT arrayStringConcat(arrayMap(x -> hex(x), {m:Array(String)}), ',') AS h, \
                   toString(length({m:Array(String)})) AS n";
        let want: Vec<String> = short.iter().map(|v| upper_hex(v)).collect();
        let got = first_text(&src, prepare(&defs, &[("m", &refs)], sql)).await;
        assert_eq!(got, [want.join(","), short.len().to_string()]);
        for v in &corpus {
            let got = first_text(&src, prepare(&defs, &[("m", &[v])], sql)).await;
            assert_eq!(got, [upper_hex(v), "1".to_owned()], "{v:?}");
        }
        assert_eq!(first_text(&src, prepare(&defs, &[("m", &[])], sql)).await, ["", "0"]);
        // Int64 and UInt64: the bounds and the sign of zero give the decimal value.
        let ints: [(&str, &str, &str); 6] = [
            ("Int64", "-9223372036854775808", "-9223372036854775808"),
            ("Int64", "9223372036854775807", "9223372036854775807"),
            ("Int64", "-0", "0"),
            ("Int64", "0", "0"),
            ("UInt64", "18446744073709551615", "18446744073709551615"),
            ("UInt64", "0", "0"),
        ];
        for (ty, v, want) in ints {
            let p = prepare(&defs, &[("t", &[v])], &format!("SELECT toString({{t:{ty}}}) AS s"));
            assert_eq!(first_text(&src, p).await, [want], "{ty} {v}");
            let p = prepare(&defs, &[("m", &[v, v])], &format!("SELECT toString({{m:Array({ty})}}) AS s"));
            assert_eq!(first_text(&src, p).await, [format!("[{want},{want}]")], "{ty} {v}");
        }
        // Float64: the same 64 bits as the parse of Rust.
        for v in ["0", "-0", "1.5", "-1.5", "0.1", "123456789.000000001", "0.000000000000000000001", &"9".repeat(300)] {
            let p = prepare(&defs, &[("t", &[v])], "SELECT toString(reinterpretAsUInt64({t:Float64})) AS b");
            let bits = v.parse::<f64>().unwrap().to_bits().to_string();
            assert_eq!(first_text(&src, p).await, [bits], "{v}");
        }
        // Date: the range of 7.6a, and the leap days.
        for v in ["1970-01-01", "2149-06-06", "2000-02-29", "2024-02-29", "2026-11-22"] {
            let p = prepare(&defs, &[("t", &[v])], "SELECT toString({t:Date}) AS d, toTypeName({t:Date}) AS ty");
            assert_eq!(first_text(&src, p).await, [v, "Date"], "{v}");
        }
    }

    /// Sends one statement as the user `hune` with one pair `param_t`, made by the encoder of 7.6a and the percent
    /// encoding of 8.3, and gives the status and the text of the answer.
    async fn raw(text: &str, sql: &str) -> (u16, String) {
        let stream = tokio::net::TcpStream::connect(http_addr()).await.expect("ClickHouse is not reachable");
        let (mut sender, conn) = http1::handshake::<_, Full<Bytes>>(TokioIo::new(stream)).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let encoded: String = text
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let req = hyper::Request::post(format!("/?param_t={encoded}"))
            .header("host", http_addr().to_string())
            .header("x-clickhouse-user", "hune")
            .header("x-clickhouse-key", PASSWORD)
            .body(Full::new(Bytes::from(sql.to_owned())))
            .unwrap();
        let resp = sender.send_request(req).await.unwrap();
        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).trim_end().to_owned())
    }

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c40_the_escapes_of_the_control_characters_round_trip() {
        // 7.7 refuses these characters, so the engine never sends them. The encoder escapes them all the same.
        for v in ["\t", "\n", "\r", "\0", "a\tb\nc", "\\\t", "'\n'", "x\r\ny"] {
            let (status, text) = raw(&escaped_text(ParamType::String, &[v]), "SELECT hex({t:String})").await;
            assert_eq!((status, text.as_str()), (200, upper_hex(v).as_str()), "{v:?}");
            let (status, text) = raw(
                &escaped_text(ParamType::ArrayString, &[v, "]", v]),
                "SELECT arrayStringConcat(arrayMap(x -> hex(x), {t:Array(String)}), ',')",
            )
            .await;
            assert_eq!((status, text), (200, format!("{},5D,{}", upper_hex(v), upper_hex(v))), "{v:?}");
        }
    }

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c40_clickhouse_refuses_or_changes_the_values_that_the_grammar_of_7_6a_refuses() {
        // Each value of this list breaks the grammar of its type in 7.6a. The answer of ClickHouse 26.8 shows why:
        // a leading zero gives 457 or 130, and a value out of range or an impossible date changes with no error.
        let cases: [(&str, &str, &str, u16, &str); 6] = [
            ("007", "Int64", "SELECT toString({t:Int64})", 500, "Code: 457"),
            ("-0", "UInt64", "SELECT toString({t:UInt64})", 500, "Code: 457"),
            ("9223372036854775808", "Int64", "SELECT toString({t:Int64})", 200, "-9223372036854775808"),
            ("18446744073709551616", "UInt64", "SELECT toString({t:UInt64})", 200, "0"),
            ("2026-02-30", "Date", "SELECT toString({t:Date})", 200, "2026-03-02"),
            ("1969-12-31", "Date", "SELECT toString({t:Date})", 200, "1970-01-01"),
        ];
        for (value, ty, sql, status, start) in cases {
            assert!(!ParamType::parse(ty).unwrap().accepts(value), "7.6a refuses {value} for {ty}");
            let (got, text) = raw(value, sql).await;
            if status == 200 {
                assert_eq!((got, text.as_str()), (200, start), "{value} {ty}: ClickHouse changes the value");
            } else {
                // The status of a ClickHouse error depends on the code: the test reads the code in the text.
                assert!(got != 200 && text.starts_with(start), "{value} {ty}: {got} {text}");
            }
        }
        let (_, text) = raw("[007]", "SELECT toString({t:Array(Int64)})").await;
        assert!(text.starts_with("Code: 130"), "{text}");
    }

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c40_clickhouse_reads_a_brace_after_a_dollar_as_a_query_parameter() {
        // Measured on ClickHouse 26.8: `a$` is a name, and the `{t:Identifier}` after it is its alias. Thus the second
        // scan of 7.6a reads a `{` right after `$` in the final text, and the engine refuses the forged form.
        let (status, text) = raw("zz", "SELECT 7 AS a$, a${t:Identifier} FORMAT TSVWithNames").await;
        assert_eq!((status, text.as_str()), (200, "a$\tzz\n7\t7"));
        let defs = [
            VariableDef::text("t", TextValueType::Text),
            VariableDef::custom("k", &["a${t:Identifier}"], false, false, None),
        ];
        let mut req = RequestValues::new();
        req.insert("t", false, &["zz"]);
        req.insert("k", false, &["a${t:Identifier}"]);
        let values = resolve_values(&defs, &req).unwrap();
        let request = ExpandRequest {
            sql: "SELECT 7 AS a$, {t:String} AS s, $k",
            values: &values,
            range: check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap(),
            interval: None,
            max_query_size: 1 << 20,
        };
        let e = expand_prepared(Dialect::ClickHouse, &request).unwrap_err();
        assert!(matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Parameter), "{e:?}");
    }

    // ---- C41 ----

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn c41_the_query_log_holds_the_value_in_place_of_the_form_as_for_the_text_form() {
        let src = source_for("hune");
        let defs = [VariableDef::text("t", TextValueType::Text)];
        let value = "secret-value-c41";
        let p = prepare(&defs, &[("t", &[value])], "SELECT {t:String} AS v");
        let c = ctx();
        let id = c.query_id.to_string();
        let frame =
            src.query(PreparedQuery::ClickHouseSql { sql: p.sql, params: p.params }, limits(), c).await.unwrap();
        assert_eq!(frame_json(&frame)["values"][0][0], value);
        admin("SYSTEM FLUSH LOGS").await;
        let logged = admin(&format!(
            "SELECT query FROM system.query_log WHERE query_id = '{id}' AND type = 'QueryFinish' FORMAT TSVRaw"
        ))
        .await;
        // Measured on ClickHouse 26.8: the log holds the statement with the value in place of the form. The log thus
        // holds the values of the query parameters as it holds the values of the text form (8.3, 12.15).
        assert_eq!(logged.trim_end(), format!("SELECT '{value}' AS v"));
    }
}

// ---------------------------------------------------------------------------------------------
// The lexer of 7.5 against the lexer of the real server (review 3, finding 1). The tables give the answers of ClickHouse
// 26.8.34.13 to each probe, and the lexer of Hune MUST read each probe in the same way. A ClickHouse with another lexer
// gives another answer, so this test fails, and the rules of 7.5 must be measured again.

mod lexer_7_5 {
    use super::*;
    use hune_core::sql::{TokenKind, lex_all};

    /// The answer of ClickHouse: the names of the columns of the result, or the code of the error.
    type Answer = Result<Vec<String>, Option<i64>>;

    async fn answer(src: &ClickHouseSource, sql: &str) -> Answer {
        match run(src, sql).await {
            Ok(f) => {
                let j = frame_json(&f);
                Ok(j["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect())
            }
            Err(e @ SourceError::QueryError { .. }) => Err(e.clickhouse_code()),
            Err(e) => panic!("{sql:?}: {e:?}"),
        }
    }

    /// The openers of a line comment, and the answer of ClickHouse 26.8.34.13 to `SELECT 1 AS a <opener> z` LF
    /// `, 2 AS b`. `#<TAB>`, `#<LF>` and `/ /` open no comment.
    const OPENERS: [(&str, Result<&[&str], i64>); 14] = [
        ("--", Ok(&["a", "b"])),
        ("-- ", Ok(&["a", "b"])),
        ("--x", Ok(&["a", "b"])),
        ("-->", Ok(&["a", "b"])),
        ("#", Ok(&["a", "b"])),
        ("# ", Ok(&["a", "b"])),
        ("#!", Ok(&["a", "b"])),
        ("//", Ok(&["a", "b"])),
        ("// ", Ok(&["a", "b"])),
        ("///", Ok(&["a", "b"])),
        ("#\t", Err(62)),
        ("#\n", Err(62)),
        ("/ /", Err(62)),
        ("REM", Err(62)),
    ];

    /// The ends of a line comment, and the answer of ClickHouse 26.8.34.13 to `SELECT 1 AS a <opener> z<end>, 2 AS b`
    /// LF for each opener: only LF ends the comment.
    const ENDS: [(&str, Result<&[&str], i64>); 9] = [
        ("\n", Ok(&["a", "b"])),
        ("\r\n", Ok(&["a", "b"])),
        ("\r", Ok(&["a"])),
        ("\x0c", Ok(&["a"])),
        ("\x0b", Ok(&["a"])),
        ("\0", Ok(&["a"])),
        ("\u{85}", Ok(&["a"])),
        ("\u{2028}", Ok(&["a"])),
        ("\u{2029}", Ok(&["a"])),
    ];

    /// The tags of a heredoc, and the answer of ClickHouse 26.8.34.13 to `SELECT $<tag>$x'y$<tag>$ AS h`. A tag has
    /// ASCII letters, digits and `_`, and it can start with a digit.
    const TAGS: [(&str, Result<&[&str], i64>); 28] = [
        ("", Ok(&["h"])),
        ("a", Ok(&["h"])),
        ("_", Ok(&["h"])),
        ("A1", Ok(&["h"])),
        ("1", Ok(&["h"])),
        ("1a", Ok(&["h"])),
        ("9_", Ok(&["h"])),
        (" ", Err(62)),
        ("a b", Err(62)),
        ("'", Err(62)),
        ("\"", Err(62)),
        ("`", Err(62)),
        ("-", Err(62)),
        (".", Err(62)),
        ("\u{e9}", Err(62)),
        ("\t", Err(62)),
        ("\n", Err(62)),
        ("{", Err(62)),
        ("}", Err(62)),
        ("#", Err(62)),
        ("/*", Err(62)),
        ("--", Err(62)),
        ("//", Err(62)),
        ("\\", Err(62)),
        ("(", Err(62)),
        ("a-b", Err(62)),
        ("x'y", Err(62)),
        ("\u{a0}", Err(62)),
    ];

    /// Other probes: the text and the answer of ClickHouse 26.8.34.13.
    const OTHERS: [(&str, Result<&[&str], i64>); 9] = [
        ("SELECT 1 /* a /* b */ c */ AS a", Ok(&["a"])),
        ("SELECT 1 /* a /* b */ AS a", Err(62)),
        ("SELECT 1 /*/ AS a */ AS b", Ok(&["b"])),
        ("SELECT 1 /* // */ AS a", Ok(&["a"])),
        ("SELECT 1 AS a -- /* \n, 2 AS b -- */", Ok(&["a", "b"])),
        ("SELECT 'a\\'b' AS s", Ok(&["s"])),
        ("SELECT 'a''b' AS s", Ok(&["s"])),
        ("SELECT 1 AS `a\\`b`", Ok(&["a`b"])),
        ("SELECT 1 AS $1", Ok(&["$1"])),
    ];

    fn has_word(sql: &str, word: &str) -> bool {
        lex_all(sql).iter().any(|t| t.kind == TokenKind::Word && t.text(sql) == word)
    }

    fn has(sql: &str, kind: TokenKind) -> bool {
        lex_all(sql).iter().any(|t| t.kind == kind && t.closed)
    }

    fn want(w: &Result<&[&str], i64>) -> Answer {
        w.map(|c| c.iter().map(|s| (*s).to_owned()).collect()).map_err(Some)
    }

    #[tokio::test]
    #[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
    async fn the_lexer_of_7_5_reads_each_probe_as_clickhouse_does() {
        let src = source_for("hune");
        let mut bad = Vec::new();
        let mut count = 0;
        // Line comments: the opener, then the end. When ClickHouse answers, Hune reads `b` as code exactly when the
        // answer has the column b. When ClickHouse refuses the text, Hune reads no line comment in it.
        let mut texts: Vec<(String, Answer)> = Vec::new();
        for (opener, w) in &OPENERS {
            let rest = if opener.ends_with('\n') { "" } else { " z\n" };
            texts.push((format!("SELECT 1 AS a {opener}{rest}, 2 AS b"), want(w)));
        }
        for opener in ["--", "# ", "#!", "//"] {
            for (end, w) in &ENDS {
                texts.push((format!("SELECT 1 AS a {opener} z{end}, 2 AS b\n"), want(w)));
            }
        }
        for (sql, w) in &texts {
            let got = answer(&src, sql).await;
            count += 1;
            if got != *w {
                bad.push(format!("ClickHouse {sql:?}: {got:?}, measured {w:?}"));
            }
            let agrees = match w {
                Ok(columns) => {
                    let unknown = lex_all(sql).iter().any(|t| t.kind == TokenKind::Unknown);
                    !unknown && has_word(sql, "b") == (columns.len() == 2)
                }
                Err(_) => !has(sql, TokenKind::LineComment),
            };
            if !agrees {
                bad.push(format!("Hune {sql:?}: reads the text in another way than ClickHouse {w:?}"));
            }
        }
        // Heredocs: Hune reads one closed heredoc exactly when ClickHouse gives the column h.
        for (tag, w) in &TAGS {
            let sql = format!("SELECT ${tag}$x'y${tag}$ AS h");
            let got = answer(&src, &sql).await;
            count += 1;
            if got != want(w) {
                bad.push(format!("ClickHouse {sql:?}: {got:?}, measured {w:?}"));
            }
            if has(&sql, TokenKind::Heredoc) != w.is_ok() {
                bad.push(format!("Hune {sql:?}: heredoc {}, ClickHouse {w:?}", has(&sql, TokenKind::Heredoc)));
            }
        }
        for (sql, w) in &OTHERS {
            let got = answer(&src, sql).await;
            count += 1;
            if got != want(w) {
                bad.push(format!("ClickHouse {sql:?}: {got:?}, measured {w:?}"));
            }
        }
        assert!(bad.is_empty(), "{} of {count} probes differ:\n{}", bad.len(), bad.join("\n"));
        println!("the lexer of 7.5: {count} probes give the answers of ClickHouse 26.8.34.13");
    }
}
