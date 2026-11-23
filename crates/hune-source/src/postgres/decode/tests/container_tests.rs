//! Container tests of the binary decode (7.16a, PS06).
//!
//! The tests are `#[ignore]`. They need a disposable PostgreSQL on 127.0.0.1 and the variables
//! `HUNE_TEST_PGS_HOST`, `_PORT`, `_DATABASE`, `_ADMIN_USER` and `_ADMIN_PASSWORD` (the harness
//! `tests/ci/pgsource.sh` sets them). The server holds synthetic data only. The tests send SQL through `prepare` and
//! `query` only. They compare the decode with the text that PostgreSQL prints for the same value,
//! and the frame with the examples of 7.16a.

use std::time::Duration;

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Config, NoTls};

use super::*;
use crate::postgres::frame_build::{Caps, PgFrameBuilder, classify_columns};

const REASON: &str = "needs a PostgreSQL container: run tests/ci/pgsource.sh";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: {REASON}"))
}

async fn connect() -> Client {
    let mut config = Config::new();
    config
        .host(env("HUNE_TEST_PGS_HOST"))
        .port(env("HUNE_TEST_PGS_PORT").parse().expect("port"))
        .user(env("HUNE_TEST_PGS_ADMIN_USER"))
        .password(env("HUNE_TEST_PGS_ADMIN_PASSWORD"))
        .dbname(env("HUNE_TEST_PGS_DATABASE"))
        .options("-c TimeZone=UTC -c DateStyle=ISO,YMD -c standard_conforming_strings=on")
        .connect_timeout(Duration::from_secs(10));
    let (client, connection) = config.connect(NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

/// Runs a query of one row and decodes each column.
async fn one(client: &Client, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Vec<Value> {
    let stmt = client.prepare(sql).await.expect("prepare");
    let columns = classify_columns(stmt.columns()).expect("listed types");
    let row = client.query_one(&stmt, params).await.expect("query");
    let raw = raw_values(&row).expect("raw");
    columns.iter().zip(&raw).map(|((_, ty), r)| decode(ty, *r).expect("decode")).collect()
}

/// The text of the server cast. A cast of `inet` to `text` keeps `/32` and `/128`, but the output
/// function of `inet` leaves them out (7.16a): the decode follows the output function.
fn server_text<'a>(ty: &str, t: &'a str) -> &'a str {
    if ty != "inet" {
        return t;
    }
    let host = if t.contains(':') { "/128" } else { "/32" };
    t.strip_suffix(host).unwrap_or(t)
}

fn text(v: &Value) -> &str {
    match v {
        Value::Text(s) | Value::Number(s) => s,
        other => panic!("not a text: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_scalars_match_the_examples_of_7_16a() {
    let c = connect().await;
    let v = one(
        &c,
        "SELECT true, 1::int2, -2::int4, 3::oid, 9223372036854775807::int8, 0.1::float4, 'NaN'::float8, \
         '-Infinity'::float4, 12.5::numeric(38,9), 'NaN'::numeric, 'a b'::text, 'x'::varchar, 'ab'::char(5), \
         'n'::name, '{\"a\":  1}'::json, '{\"a\":  1}'::jsonb, '2026-11-09'::date, 'infinity'::date, \
         '2026-11-09 08:00:00.125+00'::timestamptz, '-infinity'::timestamptz, '2026-11-09 08:00:00.5'::timestamp, \
         '13:05:07.5'::time, '1969-12-31 23:59:58.5+00'::timestamptz",
        &[],
    )
    .await;
    let expect: Vec<Value> = vec![
        Value::Bool(true),
        Value::Number("1".into()),
        Value::Number("-2".into()),
        Value::Number("3".into()),
        Value::Text("9223372036854775807".into()),
        Value::Number("0.1".into()),
        Value::Text("nan".into()),
        Value::Text("-inf".into()),
        Value::Text("12.500000000".into()),
        Value::Null,
        Value::Text("a b".into()),
        Value::Text("x".into()),
        Value::Text("ab   ".into()),
        Value::Text("n".into()),
        Value::Text("{\"a\":  1}".into()),
        Value::Text("{\"a\": 1}".into()),
        Value::Number("1794182400".into()),
        Value::Null,
        Value::Number("1794211200.125".into()),
        Value::Null,
        Value::Number("1794211200.5".into()),
        Value::Text("13:05:07.5".into()),
        Value::Number("-1.5".into()),
    ];
    assert_eq!(v, expect);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_text_forms_equal_the_server_text() {
    let c = connect().await;
    for (lit, ty) in [
        ("10.0.0.1", "inet"),
        ("10.0.0.1/24", "inet"),
        ("10.0.0.1/32", "inet"),
        ("10.0.0.0/8", "cidr"),
        ("10.1.2.3/32", "cidr"),
        ("::1", "inet"),
        ("::", "inet"),
        ("::/0", "cidr"),
        ("2001:db8::1", "inet"),
        ("2001:db8::/32", "cidr"),
        ("::ffff:1.2.3.4", "inet"),
        ("::1.2.3.4", "inet"),
        ("::2", "inet"),
        ("1::", "inet"),
        ("1:0:0:2:0:0:0:3", "inet"),
        ("0.0.0.0/0", "cidr"),
        ("550e8400-e29b-41d4-a716-446655440000", "uuid"),
        ("{\"b\":[1, 2,{\"c\":null}],\"a\":\"é\"}", "jsonb"),
        ("00:00:00", "time"),
        ("24:00:00", "time"),
        ("23:59:59.999999", "time"),
        ("01:02:03.120", "time"),
        ("0", "numeric"),
        ("-0.000", "numeric"),
        ("123456789012345678901234567890.123456789", "numeric"),
        ("0.00000000000000000001", "numeric"),
        ("100000000", "numeric"),
        ("-5e-9", "numeric(30,12)"),
        ("1e+30", "numeric"),
    ] {
        let sql = format!("SELECT $1::text::{ty}, ($1::text::{ty})::text");
        let v = one(&c, &sql, &[&lit]).await;
        assert_eq!(text(&v[0]), server_text(ty, text(&v[1])), "{lit} as {ty}");
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_random_ipv6_and_numeric_equal_the_server_text() {
    let c = connect().await;
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..400 {
        // Words with many zeros, so that the runs vary.
        let words: Vec<String> =
            (0..8).map(|_| if next() % 3 == 0 { format!("{:x}", next() % 0x10000) } else { "0".into() }).collect();
        let lit = format!("{}/{}", words.join(":"), next() % 129);
        for ty in ["inet", "cidr"] {
            // A cidr with host bits is an error: use `inet` then `cidr(inet)` through `set_masklen`.
            let sql = if ty == "inet" {
                "SELECT $1::text::inet, ($1::text::inet)::text".to_owned()
            } else {
                "SELECT network($1::text::inet)::cidr, (network($1::text::inet)::cidr)::text".to_owned()
            };
            let v = one(&c, &sql, &[&lit]).await;
            assert_eq!(text(&v[0]), server_text(ty, text(&v[1])), "{lit} as {ty}");
        }
    }
    for _ in 0..400 {
        let int_digits = next() % 12;
        let frac_digits = next() % 14;
        let mut s = String::new();
        if next() % 2 == 0 {
            s.push('-');
        }
        for i in 0..=int_digits {
            let d = if i == 0 { 1 + next() % 9 } else { next() % 10 };
            s.push(char::from(b'0' + d as u8));
        }
        if frac_digits > 0 {
            s.push('.');
            for _ in 0..frac_digits {
                s.push(char::from(b'0' + (next() % 10) as u8));
            }
        }
        let v = one(&c, "SELECT $1::text::numeric, ($1::text::numeric)::text", &[&s]).await;
        assert_eq!(text(&v[0]), text(&v[1]), "{s}");
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_random_timestamps_equal_the_server_epoch() {
    let c = connect().await;
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..300 {
        let secs = (next() % 8_000_000_000) as i64 - 2_000_000_000;
        let us = next() % 1_000_000;
        let micros = secs * 1_000_000 + if secs < 0 { -(us as i64) } else { us as i64 };
        // Exact: an integer count of microseconds added to the epoch.
        let v = one(
            &c,
            "SELECT timestamptz 'epoch' + $1::text::bigint * interval '1 microsecond', \
             (timestamptz 'epoch' + $1::text::bigint * interval '1 microsecond')::timestamp",
            &[&micros.to_string()],
        )
        .await;
        let sign = if micros < 0 { "-" } else { "" };
        let abs = micros.unsigned_abs();
        let mut want = format!("{sign}{}.{:06}", abs / 1_000_000, abs % 1_000_000);
        want = want.trim_end_matches('0').trim_end_matches('.').to_owned();
        assert_eq!(text(&v[0]), want, "{micros}");
        assert_eq!(text(&v[1]), want, "{micros} as timestamp");
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_arrays_enum_and_domain() {
    let c = connect().await;
    c.batch_execute(
        "DROP TYPE IF EXISTS pg_temp.color CASCADE; CREATE TYPE pg_temp.color AS ENUM ('red', 'dark blue'); \
         DROP DOMAIN IF EXISTS pg_temp.posint CASCADE; CREATE DOMAIN pg_temp.posint AS int4 CHECK (VALUE > 0);",
    )
    .await
    .expect("setup");
    let v = one(
        &c,
        "SELECT ARRAY[1,NULL,3], ARRAY['a','b\"c', NULL]::text[], ARRAY[[1,2],[3,4]], '{}'::int4[], \
         ARRAY['2026-11-09'::date, 'infinity'], ARRAY['2026-11-09 08:00:00.125+00'::timestamptz, '2026-11-09 08:00:00+00'], \
         ARRAY[1.5::float8,'NaN','Infinity'], ARRAY[12.50::numeric(10,2),'NaN'], ARRAY['red'::pg_temp.color], \
         'dark blue'::pg_temp.color, 5::pg_temp.posint, ARRAY[7::pg_temp.posint], ARRAY[9223372036854775807::int8], \
         ARRAY[true,false,NULL], ARRAY['10.0.0.1'::inet], ARRAY['550e8400-e29b-41d4-a716-446655440000'::uuid], \
         ARRAY['13:05:07.5'::time], '[1:2]={5,6}'::int4[], NULL::int4[]",
        &[],
    )
    .await;
    let want = [
        "[1,null,3]",
        r#"["a","b\"c",null]"#,
        "[[1,2],[3,4]]",
        "[]",
        r#"["2026-11-09",null]"#,
        r#"["2026-11-09T08:00:00.125Z","2026-11-09T08:00:00Z"]"#,
        r#"[1.5,"NaN","Infinity"]"#,
        r#"["12.50","NaN"]"#,
        r#"["red"]"#,
        "dark blue",
        "5",
        "[7]",
        r#"["9223372036854775807"]"#,
        "[true,false,null]",
        r#"["10.0.0.1"]"#,
        r#"["550e8400-e29b-41d4-a716-446655440000"]"#,
        r#"["13:05:07.5"]"#,
        "[5,6]",
    ];
    for (i, w) in want.iter().enumerate() {
        assert_eq!(text(&v[i]), *w, "column {}", i + 1);
    }
    assert_eq!(v[18], Value::Null);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_unsupported_types_are_found_by_describe() {
    let c = connect().await;
    for (expr, name) in [
        ("'\\x01'::bytea", "bytea"),
        ("interval '1 day'", "interval"),
        ("1::money", "money"),
        ("'<a/>'::xml", "xml"),
        ("int4range(1, 2)", "int4range"),
        ("point(1, 2)", "point"),
        ("ROW(1, 2)", "record"),
        ("'{}'::jsonb[]", "jsonb[]"),
        ("'a'::\"char\"", "char"),
        ("'12:00+01'::timetz", "timetz"),
        ("'1'::bit", "bit"),
    ] {
        let stmt = c.prepare(&format!("SELECT {expr} AS col")).await.expect("prepare");
        let err = classify_columns(stmt.columns()).expect_err(expr);
        assert_eq!(err.column, "col");
        assert_eq!(err.type_name, name, "{expr}");
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps06_decode_frame_of_the_spec_example() {
    let c = connect().await;
    let sql = "SELECT t AS time, label, weight, blocks, active FROM (VALUES \
               ('2026-11-09 08:00:00.125+00'::timestamptz, 'a1', 12.5::numeric(38,9), 42::int4, true), \
               ('2026-11-09 08:00:20+00', NULL, 3::numeric(38,9), 7, false)) AS v(t, label, weight, blocks, active)";
    let stmt = c.prepare(sql).await.expect("prepare");
    let columns = classify_columns(stmt.columns()).expect("listed");
    let mut b = PgFrameBuilder::new(columns, Caps { max_rows: 10, max_bytes: 10_000 }).expect("builder");
    for row in c.query(&stmt, &[]).await.expect("query") {
        b.push_pg_row(&row).expect("row");
    }
    let frame = b.finish("q".into(), 1);
    let v: serde_json::Value = serde_json::from_str(&frame.to_json()).unwrap();
    assert_eq!(
        v["values"],
        serde_json::json!([
            [1794211200.125, 1794211220],
            ["a1", null],
            ["12.500000000", "3.000000000"],
            [42, 7],
            [true, false]
        ])
    );
    let types: Vec<_> = frame.columns.iter().map(|c| c.type_text.as_str()).collect();
    assert_eq!(types, ["timestamptz", "text", "numeric", "int4", "bool"]);
}
