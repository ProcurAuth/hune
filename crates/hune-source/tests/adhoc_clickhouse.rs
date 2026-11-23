// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ad hoc filters on a real ClickHouse (F36; 7.10a, 16.9).
//!
//! The values of the filters are always string literals. ClickHouse converts the literal to the type of the column for
//! the comparison. This test measures that comparison for numeric, date, `Decimal`, `Enum`, `LowCardinality` and
//! `Nullable` columns, with hostile values, through the expansion of `$__adhocFilters` and the source client.
//!
//! The test is `#[ignore]`. The script `tests/ci/source.sh` starts a container on 127.0.0.1 and runs it with
//! `-- --ignored`. The address comes from `HUNE_TEST_CLICKHOUSE` (default `127.0.0.1:18123`).

use std::net::SocketAddr;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hune_core::sql::adhoc::{RequestFilter, bind};
use hune_core::sql::{
    Dialect, ExpandRequest, RequestValues, check_time_range, compute_interval, expand_prepared_adhoc, resolve_values,
};
use hune_source::{
    ClickHouseSource, ClickHouseSourceConfig, EffectiveLimits, LogComment, LogKind, PreparedQuery, QueryCtx, QueryKind,
    QueueWait, ResultMemoryBudget, SourceError, SubjectId, Uuid,
};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde_json::Value;

const REASON: &str = "needs a ClickHouse container: run tests/ci/source.sh";

fn http_addr() -> SocketAddr {
    std::env::var("HUNE_TEST_CLICKHOUSE").unwrap_or_else(|_| "127.0.0.1:18123".to_owned()).parse().unwrap()
}

fn source() -> ClickHouseSource {
    let a = http_addr();
    let cfg = ClickHouseSourceConfig {
        id: hune_core::ids::SourceId::parse("ch").unwrap(),
        label: "Test".into(),
        https: false,
        host: a.ip().to_string(),
        port: a.port(),
        user: "hune".into(),
        password: "hune-test-password".into(),
        database: "analytics".into(),
        tls: None,
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 100_000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(2),
        max_concurrent_per_subject: None,
    };
    ClickHouseSource::new(cfg, ResultMemoryBudget::new(1 << 30)).unwrap()
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
            uid: Some("adhoc".into()),
            folder: None,
            panel: Some(1),
            query_ref: None,
            variable: None,
            request_id: Some("r-adhoc".into()),
            subject: Some("-".into()),
        },
        subject: SubjectId::open(),
        traceparent: None,
        queue_wait: QueueWait::new(),
    }
}

/// Runs one statement as the administrator of the container.
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

/// One case: the column, the operator, the value and the count of rows, or the ClickHouse code of the error.
type Case = (&'static str, &'static str, &'static str, Result<u64, Option<i64>>);

const COLUMNS: [&str; 10] = ["i", "u", "f", "d", "dt", "dec", "e", "lc", "n", "ns"];

/// The count of rows that one filter keeps, or the ClickHouse code of the error.
async fn count(src: &ClickHouseSource, key: &str, op: &str, value: &str) -> Result<u64, Option<i64>> {
    let keys: Vec<String> = COLUMNS.iter().map(|s| (*s).to_owned()).collect();
    let filters = [RequestFilter { key: key.into(), op: op.into(), value: value.into() }];
    let run = bind("filters", &keys, &keys, &filters).unwrap();
    let resolved = resolve_values(&[], &RequestValues::new()).unwrap();
    let range = check_time_range(0, 1000).unwrap();
    let sql = "SELECT count() AS rows_kept FROM analytics.adhoc_types WHERE $__adhocFilters";
    let req = ExpandRequest {
        sql,
        values: &resolved,
        range,
        interval: Some(compute_interval(&range, 100, 1).unwrap()),
        max_query_size: 1_048_576,
    };
    let prepared = expand_prepared_adhoc(Dialect::ClickHouse, &req, Some(&run)).unwrap();
    let q = PreparedQuery::ClickHouseSql { sql: prepared.sql, params: prepared.params };
    match src.query(q, limits(), ctx()).await {
        Ok(frame) => {
            let j: Value = serde_json::from_str(&frame.to_json()).unwrap();
            Ok(j["values"][0][0].as_u64().or_else(|| j["values"][0][0].as_str().and_then(|s| s.parse().ok())).unwrap())
        }
        Err(e @ SourceError::QueryError { .. }) => Err(e.clickhouse_code()),
        Err(e) => panic!("{key} {op} {value:?}: {e:?}"),
    }
}

#[tokio::test]
#[ignore = "needs a ClickHouse container: run tests/ci/source.sh"]
async fn adhoc_a_string_literal_compares_with_each_column_type() {
    let _ = REASON;
    admin(
        "CREATE TABLE IF NOT EXISTS analytics.adhoc_types (i Int32, u UInt64, f Float64, d Date, dt DateTime('UTC'), \
         dec Decimal(18, 4), e Enum8('a' = 1, 'b''c' = 2), lc LowCardinality(String), n Nullable(Int32), \
         ns Nullable(String)) ENGINE = Memory",
    )
    .await;
    admin("TRUNCATE TABLE analytics.adhoc_types").await;
    admin(
        "INSERT INTO analytics.adhoc_types VALUES \
         (-5, 18446744073709551615, 1.5, '2026-11-22', '2026-11-22 10:00:00', 12.3400, 'a', 'web-1', NULL, NULL), \
         (7, 0, -0.25, '1970-01-01', '1970-01-01 00:00:00', -0.0001, 'b''c', 'it''s', 3, 'x\\\\y'), \
         (7, 42, 1e300, '2149-06-06', '2106-02-07 06:28:15', 99999999999999.9999, 'a', '', 0, '')",
    )
    .await;
    let src = source();
    // (column, operator, value, expected): a count, or Err with the ClickHouse code (no conversion of the literal).
    let cases: &[Case] = &[
        ("i", "=", "7", Ok(2)),
        ("i", "!=", "7", Ok(1)),
        ("i", "=", "-5", Ok(1)),
        // A zero before the first digit is not an integer for ClickHouse 26.8: the code 53 (TYPE_MISMATCH).
        ("i", "=", "07", Err(Some(53))),
        ("u", "=", "18446744073709551615", Ok(1)),
        ("u", "=", "42", Ok(1)),
        ("f", "=", "1.5", Ok(1)),
        ("f", "=", "-0.25", Ok(1)),
        ("d", "=", "2026-11-22", Ok(1)),
        ("d", "!=", "1970-01-01", Ok(2)),
        ("dt", "=", "2026-11-22 10:00:00", Ok(1)),
        ("dec", "=", "12.34", Ok(1)),
        ("dec", "=", "-0.0001", Ok(1)),
        ("e", "=", "a", Ok(2)),
        ("e", "=", "b'c", Ok(1)),
        ("lc", "=", "web-1", Ok(1)),
        ("lc", "=", "it's", Ok(1)),
        ("lc", "=", "", Ok(1)),
        ("lc", "!=", "web-1", Ok(2)),
        // NULL never compares: `=` and `!=` both leave the NULL row out.
        ("n", "=", "3", Ok(1)),
        ("n", "!=", "3", Ok(1)),
        ("ns", "=", "x\\y", Ok(1)),
        ("ns", "!=", "x\\y", Ok(1)),
        // Hostile texts stay values: they compare and match no row.
        ("lc", "=", "' OR 1=1 --", Ok(0)),
        ("lc", "!=", "web-1' OR '1'='1", Ok(3)),
        ("ns", "=", "\\' OR 1 /*", Ok(0)),
        ("lc", "=", "$__fromTime ${x} `a` \"b\" \u{a0}\u{2028}é", Ok(0)),
    ];
    let mut failures = Vec::new();
    for (key, op, value, expected) in cases {
        let got = count(&src, key, op, value).await;
        if &got != expected {
            failures.push(format!("{key} {op} {value:?}: got {got:?}, expected {expected:?}"));
        }
    }
    // A literal that ClickHouse cannot convert gives an error of the source: a 422 query_error, no row. The code is
    // recorded, not fixed by the test (it differs between versions).
    for (key, value) in [("i", "abc"), ("d", "2026-13-45"), ("dec", "x"), ("e", "z"), ("u", "-1"), ("n", "1.5")] {
        match count(&src, key, "=", value).await {
            Err(code) => eprintln!("{key} = {value:?}: ClickHouse code {code:?}"),
            Ok(n) => eprintln!("{key} = {value:?}: ClickHouse converts it, {n} rows"),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
