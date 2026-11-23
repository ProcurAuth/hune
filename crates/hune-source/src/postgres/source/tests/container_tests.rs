//! Container tests of the source (16.9a): PS07 (the connection side of the read-only rule, alone), and the
//! regression tests of the security review: the functions that run text (H1), the login class codes of a
//! statement (M1), the verdict of the probe (M2, M3), the size of a message (M4), and a session in use at the
//! change to `misconfigured` (L2).
//!
//! The test is `#[ignore]`. The script `tests/ci/pgsource.sh` starts a disposable PostgreSQL on 127.0.0.1
//! and sets `HUNE_TEST_PGS_*`. The client of the crate connects with the role `hune_rw`, which has all
//! rights on the schema `metrics`. The probe refuses this role, so the test sets the state of the source
//! to `up` by hand: only a test inside the crate can do it. Each write must fail with the SQLSTATE 25006,
//! because the transaction is `READ ONLY` and the session starts with `default_transaction_read_only = on`.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use hune_core::frame::Frame;
use tokio_postgres::NoTls;

use super::*;
use crate::budget::ResultMemoryBudget;
use crate::config::PostgresSslMode;
use crate::types::{LogComment, QueryKind, QueueWait, SubjectId, Uuid};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: needs a PostgreSQL container"))
}

fn rw_config() -> PostgresSourceConfig {
    PostgresSourceConfig {
        id: SourceId::parse("rw").expect("id"),
        label: "Rw".to_owned(),
        host: env("HUNE_TEST_PGS_HOST"),
        port: env("HUNE_TEST_PGS_PORT").parse().expect("port"),
        database: env("HUNE_TEST_PGS_DATABASE"),
        user: env("HUNE_TEST_PGS_RW_USER"),
        password: env("HUNE_TEST_PGS_RW_PASSWORD"),
        search_path: vec![env("HUNE_TEST_PGS_SCHEMA")],
        sslmode: PostgresSslMode::Disable,
        tls: None,
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 1000,
        max_result_bytes: 1_000_000,
        max_query_size: 100_000,
        max_concurrent: 1,
        queue_wait: Duration::from_secs(15),
        max_concurrent_per_subject: None,
    }
}

async fn run(source: &PostgresSource, sql: &str) -> Result<Frame, SourceError> {
    let limits = EffectiveLimits {
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 1000,
        max_result_bytes: 1_000_000,
        max_query_size: 100_000,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    };
    let ctx = QueryCtx {
        kind: QueryKind::Panel,
        query_id: Uuid::now_v7(),
        log_comment: LogComment::probe(),
        subject: SubjectId::open(),
        traceparent: None,
        queue_wait: QueueWait::new(),
    };
    source.query(PreparedQuery::PostgresSql { sql: sql.to_owned() }, limits, ctx).await
}

fn sqlstate(r: Result<Frame, SourceError>) -> Option<String> {
    match r {
        Ok(f) => panic!("a write gave a frame of {} rows", f.rows),
        Err(e) => {
            assert_eq!(e.code(), "query_error", "{e}");
            e.sqlstate().and_then(|s| s.map(str::to_owned))
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps07_a_write_fails_on_the_read_only_transaction_with_a_role_that_can_write() {
    // The role can write: a plain client with no startup parameter does it (in a transaction that rolls back).
    let mut c = tokio_postgres::Config::new();
    c.host(env("HUNE_TEST_PGS_HOST"))
        .port(env("HUNE_TEST_PGS_PORT").parse().expect("port"))
        .user(env("HUNE_TEST_PGS_RW_USER"))
        .password(env("HUNE_TEST_PGS_RW_PASSWORD"))
        .dbname(env("HUNE_TEST_PGS_DATABASE"));
    let (mut plain, connection) = c.connect(NoTls).await.expect("the plain client connects");
    tokio::spawn(async move { drop(connection.await) });
    let tx = plain.transaction().await.expect("begin");
    tx.execute("INSERT INTO metrics.write_target DEFAULT VALUES", &[]).await.expect("the role can write");
    tx.rollback().await.expect("rollback");

    let source = PostgresSource::new(rw_config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
    // The test sets the verdict by hand: a source that passed no probe sends nothing (finding M2).
    source.set_state(SourceState::Up);
    source.inner.verified.store(true, Ordering::Release);
    let writes = [
        "INSERT INTO metrics.write_target DEFAULT VALUES",
        "INSERT INTO metrics.write_target (note) VALUES ('x') RETURNING id",
        "UPDATE metrics.col_target SET note = 'changed'",
        "DELETE FROM metrics.col_target",
        "CREATE TABLE metrics.ps07_new (a int)",
        "SELECT id INTO metrics.ps07_copy FROM metrics.col_target",
        "WITH d AS (DELETE FROM metrics.col_target RETURNING id) SELECT id FROM d",
        "TRUNCATE metrics.col_target",
    ];
    for sql in writes {
        assert_eq!(sqlstate(run(&source, sql).await).as_deref(), Some("25006"), "{sql}");
    }
    // Each write failed: the data is as the fixture made it, and no table was made.
    let (admin, connection) = {
        let mut a = tokio_postgres::Config::new();
        a.host(env("HUNE_TEST_PGS_HOST"))
            .port(env("HUNE_TEST_PGS_PORT").parse().expect("port"))
            .user(env("HUNE_TEST_PGS_ADMIN_USER"))
            .password(env("HUNE_TEST_PGS_ADMIN_PASSWORD"))
            .dbname(env("HUNE_TEST_PGS_DATABASE"));
        a.connect(NoTls).await.expect("admin")
    };
    tokio::spawn(async move { drop(connection.await) });
    let row = admin
        .query_one(
            "SELECT (SELECT count(*) FROM metrics.col_target), (SELECT count(*) FROM metrics.write_target), \
             to_regclass('metrics.ps07_new') IS NOT NULL, to_regclass('metrics.ps07_copy') IS NOT NULL",
            &[],
        )
        .await
        .expect("catalog");
    assert_eq!(
        (row.get::<_, i64>(0), row.get::<_, i64>(1), row.get::<_, bool>(2), row.get::<_, bool>(3)),
        (1, 0, false, false)
    );

    // A change of the setting does not last, and a write still fails.
    run(&source, "SELECT set_config('default_transaction_read_only', 'off', false)")
        .await
        .expect("set_config is a read");
    assert_eq!(
        sqlstate(run(&source, "INSERT INTO metrics.write_target DEFAULT VALUES").await).as_deref(),
        Some("25006")
    );
    // `SET TRANSACTION READ WRITE` fails, or has no effect: a write after it fails too.
    let set = sqlstate(run(&source, "SET TRANSACTION READ WRITE").await);
    assert!(matches!(set.as_deref(), Some("25001" | "25006") | None), "{set:?}");
    assert_eq!(
        sqlstate(run(&source, "INSERT INTO metrics.write_target DEFAULT VALUES").await).as_deref(),
        Some("25006")
    );
    let _ = run(&source, "SET default_transaction_read_only = off").await;
    assert_eq!(
        sqlstate(run(&source, "INSERT INTO metrics.write_target DEFAULT VALUES").await).as_deref(),
        Some("25006")
    );
    // The next query on the pool shows the startup value.
    let f = run(&source, "SELECT current_setting('default_transaction_read_only') AS ro").await.expect("works");
    assert_eq!(f.columns[0].value(0), Some(hune_core::frame::FrameValue::Text("on")));
    assert_eq!(source.idle_connections(), 1, "the session of the pool served each query");
}

// ---- The regression tests of the security review ----

fn config_for(user: &str, password: &str) -> PostgresSourceConfig {
    PostgresSourceConfig { user: user.to_owned(), password: password.to_owned(), ..rw_config() }
}

fn ro_config() -> PostgresSourceConfig {
    config_for(&env("HUNE_TEST_PGS_RO_USER"), &env("HUNE_TEST_PGS_RO_PASSWORD"))
}

fn limits_of(rows: u64, bytes: u64) -> EffectiveLimits {
    EffectiveLimits {
        max_execution_time: Duration::from_secs(60),
        max_result_rows: rows,
        max_result_bytes: bytes,
        max_query_size: 100_000,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    }
}

async fn run_with(source: &PostgresSource, sql: &str, limits: EffectiveLimits) -> Result<Frame, SourceError> {
    let ctx = QueryCtx {
        kind: QueryKind::Panel,
        query_id: Uuid::now_v7(),
        log_comment: LogComment::probe(),
        subject: SubjectId::open(),
        traceparent: None,
        queue_wait: QueueWait::new(),
    };
    source.query(PreparedQuery::PostgresSql { sql: sql.to_owned() }, limits, ctx).await
}

/// The error of a query that must fail.
fn error_of(r: Result<Frame, SourceError>) -> SourceError {
    match r {
        Ok(f) => panic!("the query gave a frame of {} rows, an error was expected", f.rows),
        Err(e) => e,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps11_the_probe_passes_hune_ro_and_refuses_each_deviant_role_with_its_check() {
    // The role of the source passes R0 to R9, with the contract of 8a.13 (the revoke of the functions included).
    let source = PostgresSource::new(ro_config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
    let p = source.probe().await;
    assert_eq!((p.state, p.detail.as_deref()), (SourceState::Up, None), "{p:?}");
    // Each deviant role gives `misconfigured`, and the detail names its check (findings H1, M3).
    let roles = env("HUNE_TEST_PGS_BAD_ROLES");
    let mut seen = Vec::new();
    for item in roles.split_whitespace() {
        let (role, check) = item.split_once(':').expect("role:check");
        let source =
            PostgresSource::new(config_for(role, &env("HUNE_TEST_PGS_BAD_PASSWORD")), ResultMemoryBudget::new(1 << 30))
                .expect("builds");
        let p = source.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured, "{role}: {p:?}");
        let detail = p.detail.expect("a detail");
        assert!(detail.starts_with(&format!("{check}: ")), "{role} gives {detail}");
        // The query of a viewer gets a fixed text and no name of the catalog (finding L1).
        let e = error_of(run_with(&source, "SELECT 1", limits_of(10, 1000)).await);
        assert_eq!(e.code(), "source_misconfigured", "{role}");
        assert!(!e.message().contains(check) && !e.message().contains("metrics"), "{role}: {}", e.message());
        seen.push(role.to_owned());
    }
    for role in ["hune_bad_signal", "hune_bad_emit", "hune_bad_ts", "hune_bad_noinh", "hune_bad_owner"] {
        assert!(seen.iter().any(|r| r == role), "{role} is not in HUNE_TEST_PGS_BAD_ROLES");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps11_the_server_refuses_the_functions_that_run_text_for_hune_ro() {
    // Finding H1. The client check is a defence in depth: this test sends the text through the source, which
    // runs no check of the statement. The revoke of 8a.13 makes the server refuse each call with 42501.
    let source = PostgresSource::new(ro_config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
    assert_eq!(source.probe().await.state, SourceState::Up);
    for sql in [
        "SELECT * FROM ts_stat('SELECT pg_sleep(0)::text::tsvector')",
        "SELECT * FROM ts_stat('SELECT 1::text::tsvector', 'a')",
        "SELECT ts_rewrite('a & b'::tsquery, 'SELECT ''a''::tsquery, ''c''::tsquery')::text AS q",
        "SELECT pg_logical_emit_message(false, 'hune', 'x'::text)::text AS lsn",
        "SELECT pg_terminate_backend(0)",
        "SELECT pg_cancel_backend(0)",
    ] {
        let e = error_of(run_with(&source, sql, limits_of(10, 1000)).await);
        assert_eq!((e.code(), e.http_status()), ("source_access_denied", 502), "{sql}");
    }
    assert_eq!(source.state(), SourceState::Up, "a refused call does not change the state");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_a_login_class_error_of_a_statement_is_a_query_error_and_the_source_stays_up() {
    // Finding M1: `pg_database_size('no_such_db')` raises 3D000 on an open session.
    let source = PostgresSource::new(ro_config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
    assert_eq!(source.probe().await.state, SourceState::Up);
    let e = error_of(run_with(&source, "SELECT pg_database_size('no_such_db')", limits_of(10, 1000)).await);
    assert_eq!((e.code(), e.http_status(), e.sqlstate()), ("query_error", 422, Some(Some("3D000"))), "{e}");
    assert_eq!(source.state(), SourceState::Up);
    // The next query runs: the source is not misconfigured.
    let f = run_with(&source, "SELECT 1 AS one", limits_of(10, 1000)).await.expect("works");
    assert_eq!(f.rows, 1);
    assert_eq!(source.state(), SourceState::Up);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_a_session_in_use_at_the_change_to_misconfigured_does_not_go_back_to_the_pool() {
    // Finding L2. The query runs a few seconds. The change comes while it runs.
    let source = PostgresSource::new(ro_config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
    assert_eq!(source.probe().await.state, SourceState::Up);
    let runner = source.clone();
    let task = tokio::spawn(async move {
        run_with(&runner, "SELECT count(*) AS n FROM generate_series(1, 40000000)", limits_of(10, 1000)).await
    });
    let start = std::time::Instant::now();
    while source.admission_stats().in_flight == 0 {
        assert!(start.elapsed() < Duration::from_secs(10), "the query does not start");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    source.set_state(SourceState::Misconfigured);
    let f = task.await.expect("joins").expect("the query in flight ends well");
    assert_eq!(f.rows, 1);
    assert_eq!(source.idle_connections(), 0, "the session of a source that changed state is closed");
    // A control: after a probe that passes, a clean session goes back.
    assert_eq!(source.probe().await.state, SourceState::Up);
    run_with(&source, "SELECT 1 AS one", limits_of(10, 1000)).await.expect("works");
    assert_eq!(source.idle_connections(), 1);
}

/// The resident memory of this process, in bytes (Linux).
fn resident_bytes() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").expect("/proc/self/statm");
    let pages: u64 = statm.split_whitespace().nth(1).expect("resident pages").parse().expect("number");
    pages * 4096
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_four_huge_values_do_not_fill_the_memory_and_give_query_limit() {
    // Finding M4. The fixture holds one row with a text of 50,000,000 bytes. Without the limit on the message,
    // the client buffers the whole row for each query: four queries cost about 200 MB.
    let mut config = ro_config();
    config.max_concurrent = 4;
    config.max_result_bytes = 1 << 20;
    let source = PostgresSource::new(config, ResultMemoryBudget::new(1 << 30)).expect("builds");
    assert_eq!(source.probe().await.state, SourceState::Up);
    let before = resident_bytes();
    let peak = Arc::new(AtomicU64::new(before));
    let watch = {
        let peak = Arc::clone(&peak);
        tokio::spawn(async move {
            loop {
                peak.fetch_max(resident_bytes(), Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    };
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let source = source.clone();
        tasks.push(tokio::spawn(async move {
            run_with(&source, "SELECT v FROM metrics.big_value", limits_of(1000, 1 << 20)).await
        }));
    }
    for t in tasks {
        let e = error_of(t.await.expect("joins"));
        assert_eq!(
            (e.code(), e.limit_name(), e.limit_value()),
            ("query_limit", Some("max_result_bytes"), Some(1 << 20)),
            "{e}"
        );
    }
    watch.abort();
    assert_eq!(source.idle_connections(), 0, "a failed stream closes the session");
    let growth = peak.load(Ordering::Relaxed).saturating_sub(before);
    assert!(growth < 80 * 1024 * 1024, "the memory grew by {growth} bytes: the rows were buffered");
    // The source still works.
    let f = run_with(&source, "SELECT 1 AS one", limits_of(10, 1000)).await.expect("works");
    assert_eq!(f.rows, 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_a_huge_value_over_tls_gives_query_limit() {
    // Finding M4 on the TLS path: the wrapper sits above the TLS stream.
    let mut config = ro_config();
    config.sslmode = PostgresSslMode::VerifyFull;
    config.tls = Some(crate::config::SourceTls {
        ca_pem: std::fs::read(env("HUNE_TEST_PGS_CA_FILE")).expect("ca file"),
        server_name: env("HUNE_TEST_PGS_SERVER_NAME"),
    });
    config.max_result_bytes = 1 << 20;
    let source = PostgresSource::new(config, ResultMemoryBudget::new(1 << 30)).expect("builds");
    assert_eq!(source.probe().await.state, SourceState::Up);
    let e = error_of(run_with(&source, "SELECT v FROM metrics.big_value", limits_of(1000, 1 << 20)).await);
    assert_eq!((e.code(), e.limit_name()), ("query_limit", Some("max_result_bytes")), "{e}");
    assert_eq!(source.idle_connections(), 0);
    let f = run_with(&source, "SELECT 1 AS one", limits_of(10, 1000)).await.expect("works");
    assert_eq!(f.rows, 1);
    // A value below the limit still passes over TLS.
    let f = run_with(&source, "SELECT repeat('x', 500000) AS v", limits_of(10, 1 << 20)).await.expect("works");
    assert_eq!(f.rows, 1);
}
