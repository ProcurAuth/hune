//! Container tests of the PostgreSQL client of the source (16.9a: PS04, PS05, PS09, PS12 to PS15).
//!
//! The tests are `#[ignore]`. The script `tests/ci/pgsource.sh` starts a disposable PostgreSQL on
//! 127.0.0.1 with synthetic data, sets the variables `HUNE_TEST_PGS_*` and runs them with
//! `-- --include-ignored`. The client of `hune-source` runs no check of the statement (that check is in
//! `hune-core`), so a test sends any text. A test name starts with its case id.
//!
//! PS07 (the connection side with the role `hune_rw`) is a library test: the test sets the state of the
//! source by hand, and only the crate can do it (see `src/postgres/source/tests/container_tests.rs`).
//! PS13 uses the proxy to stop and to start the server: a container that stops loses its tmpfs data.

mod pgs;

use std::time::{Duration, Instant};

use hune_source::{EffectiveLimits, PreparedQuery, SourceError, SourceState};
use pgs::{
    Proxy, Xorshift, admin, app_name, cell, config_for, ctx, ctx_for, env, limits, make_source, ro_config, run,
    sessions_named, tls_config, wait_sessions, wide,
};

/// The config of the role `hune_ro` through a proxy.
fn via(proxy: &Proxy) -> hune_source::PostgresSourceConfig {
    config_for(&env("HUNE_TEST_PGS_RO_USER"), &env("HUNE_TEST_PGS_RO_PASSWORD"), ("127.0.0.1", proxy.port()))
}

/// A query of many Execute messages. One row in 20 sleeps about 1 ms, so each Execute of 1000 rows takes
/// about 60 ms. The server timeout of 1 s never fires, and only the client deadline stops the query (8a.7).
const MANY_EXECUTES: &str = "SELECT g, (CASE WHEN g % 20 = 0 THEN pg_sleep(0.001)::text ELSE '' END) AS s \
FROM generate_series(1, 1000000) AS g";

fn expect_err(r: Result<hune_core::frame::Frame, SourceError>) -> SourceError {
    match r {
        Ok(f) => panic!("the query gave a frame of {} rows, an error was expected", f.rows),
        Err(e) => e,
    }
}

// PS04: limits.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_row_limit_is_exact() {
    let source = make_source(ro_config());
    for (max, n) in [(1000u64, 1000u64), (1000, 1001), (2500, 2500), (2500, 2501), (1, 1), (1, 2), (3, 0)] {
        let sql = format!("SELECT i FROM generate_series(1, {n}) AS i");
        let r = run(&source, &sql, limits(max, 10_000_000, 30)).await;
        if n <= max {
            let f = r.unwrap_or_else(|e| panic!("max {max}, rows {n}: {e}"));
            assert_eq!(f.rows as u64, n, "max {max}");
            assert_eq!(source.idle_connections(), 1, "a clean session goes back to the pool");
        } else {
            let e = expect_err(r);
            assert_eq!((e.code(), e.http_status()), ("query_limit", 422), "max {max}, rows {n}");
            assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result_rows"), Some(max)));
            assert_eq!(source.idle_connections(), 0, "a cap closes the session");
        }
    }
    // The ceiling `max_rows` gives its own code.
    let ceiling = EffectiveLimits { rows_from_ceiling: true, ..limits(5, 10_000_000, 30) };
    let e = expect_err(run(&source, "SELECT i FROM generate_series(1, 6) AS i", ceiling).await);
    assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_rows"), Some(5)));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_row_cap_stops_the_work_of_the_server() {
    // A server that sent each row would need 100 s. The portal asks for `R + 1` rows: the server stops at 4.
    let source = make_source(ro_config());
    let start = Instant::now();
    let sql = "SELECT i, pg_sleep(0.001)::text FROM generate_series(1, 100000) AS i";
    let e = expect_err(run(&source, sql, limits(3, 10_000_000, 30)).await);
    assert_eq!(e.code(), "query_limit");
    assert!(start.elapsed() < Duration::from_secs(15), "{:?}", start.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_byte_limit_is_exact() {
    let source = make_source(ro_config());
    // Each value of 1,000 bytes is 1,002 bytes of JSON, and the count adds 1.
    let sql = |n: u32| format!("SELECT repeat('a', 1000) AS v FROM generate_series(1, {n})");
    let exact = 3 * 1003;
    let f = run(&source, &sql(3), limits(1000, exact, 30)).await.expect("exactly the limit passes");
    assert_eq!((f.rows, f.stats.result_bytes), (3, Some(exact)));
    assert_eq!(source.idle_connections(), 1);
    let e = expect_err(run(&source, &sql(4), limits(1000, exact, 30)).await);
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422));
    assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result_bytes"), Some(exact)));
    assert_eq!(source.idle_connections(), 0, "a cap closes the session");
    // One byte less than the exact count fails.
    let e = expect_err(run(&source, &sql(3), limits(1000, exact - 1, 30)).await);
    assert_eq!(e.limit_name(), Some("max_result_bytes"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_time_limit_with_one_execute() {
    let source = make_source(ro_config());
    let start = Instant::now();
    let e = expect_err(run(&source, "SELECT pg_sleep(20)::text", limits(1000, 1_000_000, 1)).await);
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    assert!(start.elapsed() < Duration::from_secs(5), "the server stops first: {:?}", start.elapsed());
    // The member of the ceiling.
    let ceiling = EffectiveLimits { time_from_ceiling: true, ..limits(1000, 1_000_000, 1) };
    let e = expect_err(run(&source, "SELECT pg_sleep(20)::text", ceiling).await);
    assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_duration"), Some(1)));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_time_limit_with_many_executes() {
    let source = make_source(ro_config());
    let c = ctx();
    let name = app_name(&c.query_id);
    let start = Instant::now();
    let e = expect_err(
        source
            .query(PreparedQuery::PostgresSql { sql: MANY_EXECUTES.to_owned() }, limits(1_000_000, 100_000_000, 1), c)
            .await,
    );
    let took = start.elapsed();
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    // The server value restarts at each Execute: the client deadline (1 s + 5 s) stopped the query.
    assert!(took >= Duration::from_millis(5500) && took < Duration::from_secs(10), "{took:?}");
    assert_eq!(source.idle_connections(), 0, "a deadline closes the session");
    let a = admin().await;
    wait_sessions(&a, &name, 0, Duration::from_secs(2)).await.expect("the query left pg_stat_activity");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_sql_text_above_max_query_size_never_reaches_the_server() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let sql = "SELECT 'a text of more than 20 bytes'";
    let small = EffectiveLimits { max_query_size: 20, ..wide() };
    let e = expect_err(source.query(PreparedQuery::PostgresSql { sql: sql.to_owned() }, small, ctx()).await);
    // `422 sql_too_large` is the code of `hune-core` (7.13). This client refuses the text as a second barrier.
    assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(None)));
    assert_eq!(proxy.connections(), 0, "no connection opened");
    assert_eq!(source.state(), SourceState::Unknown);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps04_a_row_of_50_mb_gives_query_limit_and_a_closed_session() {
    let source = make_source(ro_config());
    let start = Instant::now();
    let one = "SELECT v FROM metrics.big_value";
    let e = expect_err(run(&source, one, limits(1000, 1 << 20, 60)).await);
    assert_eq!((e.code(), e.limit_name(), e.limit_value()), ("query_limit", Some("max_result_bytes"), Some(1 << 20)));
    assert_eq!(source.idle_connections(), 0);
    // The second row is never read: the first row stops the query.
    let two = "SELECT v FROM metrics.big_value UNION ALL SELECT v FROM metrics.big_value";
    let e = expect_err(run(&source, two, limits(1000, 1 << 20, 60)).await);
    assert_eq!(e.limit_name(), Some("max_result_bytes"));
    assert_eq!(source.idle_connections(), 0);
    assert!(start.elapsed() < Duration::from_secs(50), "{:?}", start.elapsed());
    // The messages never hold the value: the text is short.
    assert!(e.message().len() < 200, "{}", e.message());
    // The next query works: the source opens a new session.
    let f = run(&source, "SELECT length(v) FROM metrics.big_value", wide()).await.expect("works");
    assert_eq!(cell(&f, 0, 0), "50000000");
}

// PS05: cancellation.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps05_a_client_that_leaves_stops_the_query_within_2_s() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let a = admin().await;
    let c = ctx();
    let name = app_name(&c.query_id);
    let task = {
        let source = source.clone();
        tokio::spawn(async move {
            source.query(PreparedQuery::PostgresSql { sql: "SELECT pg_sleep(30)::text".to_owned() }, wide(), c).await
        })
    };
    wait_sessions(&a, &name, 1, Duration::from_secs(10)).await.expect("the query runs");
    // The client leaves: the future is dropped.
    task.abort();
    let _ = task.await;
    let gone = wait_sessions(&a, &name, 0, Duration::from_secs(2)).await.expect("the query left pg_stat_activity");
    assert!(gone < Duration::from_secs(2), "{gone:?}");
    assert!(proxy.cancels() >= 1, "a cancel request was sent");
    assert_eq!(source.idle_connections(), 0, "a cancelled session never goes back to the pool");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps05_the_client_deadline_stops_the_query_within_2_s() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let a = admin().await;
    let c = ctx();
    let name = app_name(&c.query_id);
    let e = expect_err(
        source
            .query(PreparedQuery::PostgresSql { sql: MANY_EXECUTES.to_owned() }, limits(1_000_000, 100_000_000, 1), c)
            .await,
    );
    assert_eq!(e.code(), "query_timeout");
    wait_sessions(&a, &name, 0, Duration::from_secs(2)).await.expect("the query left pg_stat_activity");
    assert!(proxy.cancels() >= 1, "a cancel request was sent");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps05_a_lost_cancel_request_still_stops_the_query_within_3_s() {
    let proxy = Proxy::start().await;
    proxy.drop_cancels(true);
    let source = make_source(via(&proxy));
    let a = admin().await;
    let c = ctx();
    let name = app_name(&c.query_id);
    let task = {
        let source = source.clone();
        tokio::spawn(async move {
            source.query(PreparedQuery::PostgresSql { sql: "SELECT pg_sleep(30)::text".to_owned() }, wide(), c).await
        })
    };
    wait_sessions(&a, &name, 1, Duration::from_secs(10)).await.expect("the query runs");
    task.abort();
    let _ = task.await;
    // The proxy drops the cancel request. The close of the connection and
    // `client_connection_check_interval = 1000` stop the query.
    let gone = wait_sessions(&a, &name, 0, Duration::from_secs(3)).await.expect("the query left pg_stat_activity");
    assert!(gone < Duration::from_secs(3), "{gone:?}");
    assert!(proxy.cancels() >= 1, "the cancel request reached the proxy and was dropped");
}

// PS09: one statement.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps09_a_second_statement_is_impossible() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let a = admin().await;
    for sql in [
        "SELECT 1; SELECT 2",
        "SELECT 1 /* x */ ; DROP TABLE metrics.write_target",
        "SELECT 1;\nINSERT INTO metrics.write_target DEFAULT VALUES",
    ] {
        let e = expect_err(run(&source, sql, wide()).await);
        assert_eq!((e.code(), e.http_status(), e.sqlstate()), ("query_error", 422, Some(Some("42601"))), "{sql}");
    }
    // A text that is one statement with a `;` at the end is one statement.
    let f = run(&source, "SELECT 1 AS one;", wide()).await.expect("one statement");
    assert_eq!(cell(&f, 0, 0), "1");
    // The session stays usable after the error of the server.
    assert_eq!(source.idle_connections(), 1);
    // The table of the second text still exists and is empty of the second text's effect.
    let exists: bool =
        a.query_one("SELECT to_regclass('metrics.write_target') IS NOT NULL", &[]).await.expect("catalog").get(0);
    assert!(exists);
    // The proxy: no `Q` message holds a text that is not a constant of 8a.5.
    let constants = ["START TRANSACTION READ ONLY", "BEGIN READ ONLY", "ROLLBACK", "DISCARD ALL"];
    let simple = proxy.simple_queries();
    assert!(!simple.is_empty());
    for q in &simple {
        assert!(constants.contains(&q.as_str()), "a message Query holds {q:?}");
    }
    assert!(simple.iter().any(|q| q == "START TRANSACTION READ ONLY"));
    assert!(simple.iter().any(|q| q == "DISCARD ALL"));
    // The caller text goes in a Parse message, with the comment of 8a.11 in front.
    assert!(proxy.parses().iter().any(|p| p.starts_with("/*hune {") && p.ends_with("SELECT 1; SELECT 2")));
}

// PS12: TLS.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_the_test_ca_with_channel_binding_succeeds() {
    let source = make_source(tls_config(&env("HUNE_TEST_PGS_CA_FILE"), &env("HUNE_TEST_PGS_SERVER_NAME")));
    let p = source.probe().await;
    assert_eq!((p.state, p.detail.as_deref()), (SourceState::Up, None), "{p:?}");
    assert_eq!(p.version_supported, Some(true));
    // The session uses TLS and the server says so.
    let f = run(&source, "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", wide()).await.expect("query");
    assert_eq!((f.rows, cell(&f, 0, 0).as_str()), (1, "true"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_a_wrong_server_name_stops_the_connection() {
    let source = make_source(tls_config(&env("HUNE_TEST_PGS_CA_FILE"), "wrong-name.example"));
    let p = source.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured, "{p:?}");
    let detail = p.detail.expect("a detail");
    assert!(detail.contains("tls.server_name"), "{detail}");
    assert!(!detail.contains(&env("HUNE_TEST_PGS_RO_PASSWORD")));
    let e = expect_err(run(&source, "SELECT 1", wide()).await);
    assert_eq!((e.code(), e.http_status()), ("source_misconfigured", 502));
    assert_eq!(source.state(), SourceState::Misconfigured);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_a_certificate_from_another_ca_stops_the_connection() {
    let source = make_source(tls_config(&env("HUNE_TEST_PGS_WRONG_CA_FILE"), &env("HUNE_TEST_PGS_SERVER_NAME")));
    let p = source.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured, "{p:?}");
    let detail = p.detail.expect("a detail");
    assert!(detail.contains("tls.ca_file"), "{detail}");
    // No system store is read: the wrong CA is the only root, so the check fails.
    let e = expect_err(run(&source, "SELECT 1", wide()).await);
    assert_eq!(e.code(), "source_misconfigured");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_the_methods_md5_and_password_stop_the_connection() {
    for user in [env("HUNE_TEST_PGS_MD5_USER"), env("HUNE_TEST_PGS_CLEAR_USER")] {
        let mut config = tls_config(&env("HUNE_TEST_PGS_CA_FILE"), &env("HUNE_TEST_PGS_SERVER_NAME"));
        config.user = user.clone();
        config.password = env("HUNE_TEST_PGS_LEGACY_PASSWORD");
        let source = make_source(config);
        let p = source.probe().await;
        assert_ne!(p.state, SourceState::Up, "{user}: {p:?}");
        let detail = p.detail.expect("a detail");
        assert!(detail.contains("channel binding"), "{user}: {detail}");
        assert!(!detail.contains(&env("HUNE_TEST_PGS_LEGACY_PASSWORD")));
        let e = expect_err(run(&source, "SELECT 1", wide()).await);
        assert_eq!(e.code(), "source_misconfigured", "{user}");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_sslmode_disable_works_on_the_loopback_interface() {
    let source = make_source(ro_config());
    let p = source.probe().await;
    assert_eq!(p.state, SourceState::Up, "{p:?}");
    let f = run(&source, "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()", wide()).await.expect("query");
    assert_eq!(cell(&f, 0, 0), "false");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_a_wrong_password_is_misconfigured_and_the_text_has_no_secret() {
    let mut config = ro_config();
    config.password = "not-the-password".to_owned();
    let source = make_source(config);
    let p = source.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured, "{p:?}");
    let detail = p.detail.expect("a detail");
    assert!(detail.contains("28P01"), "{detail}");
    assert!(!detail.contains("not-the-password") && !detail.contains(&env("HUNE_TEST_PGS_RO_PASSWORD")));
}

// PS13: a stopped server.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps13_a_stopped_server_gives_source_unavailable_then_up_again() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let f = run(&source, "SELECT 1 AS one", wide()).await.expect("the first query");
    assert_eq!(cell(&f, 0, 0), "1");
    assert_eq!(source.state(), SourceState::Up);
    // The server stops: the port refuses the connections.
    proxy.stop();
    let secret_sql = "SELECT 'a-value-that-must-not-leak' AS v";
    let e = expect_err(run(&source, secret_sql, wide()).await);
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
    assert!(!e.message().contains("a-value-that-must-not-leak"), "{}", e.message());
    assert_eq!(source.state(), SourceState::Down);
    let p = source.probe().await;
    assert_eq!(p.state, SourceState::Down, "{p:?}");
    assert!(!p.detail.expect("detail").contains("secret"));
    // The server starts again.
    proxy.restart().await;
    let p = source.probe().await;
    assert_eq!((p.state, p.previous), (SourceState::Up, SourceState::Down), "{p:?}");
    let f = run(&source, "SELECT 2 AS two", wide()).await.expect("after the restart");
    assert_eq!(cell(&f, 0, 0), "2");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps13_a_query_on_a_down_source_goes_to_the_server_and_a_success_sets_up() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    run(&source, "SELECT 1", wide()).await.expect("the first query");
    proxy.stop();
    let _ = expect_err(run(&source, "SELECT 1", wide()).await);
    assert_eq!(source.state(), SourceState::Down);
    proxy.restart().await;
    // No probe runs: the query itself goes to the server (8a.12).
    run(&source, "SELECT 1", wide()).await.expect("the query goes to the server");
    assert_eq!(source.state(), SourceState::Up);
}

// PS14: the pool.

/// Waits until the server runs the text of the caller: the session has the name and the query holds `needle`.
async fn wait_running(a: &tokio_postgres::Client, name: &str, needle: &str) {
    let pattern = format!("%{needle}%");
    let start = Instant::now();
    loop {
        let n: i64 = a
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE application_name = $1 AND state = 'active' AND query LIKE $2",
                &[&name, &pattern],
            )
            .await
            .expect("pg_stat_activity")
            .get(0);
        if n == 1 {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "the query does not run");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn terminate(a: &tokio_postgres::Client, pids: &[i32]) {
    a.execute("SELECT pg_terminate_backend(pid) FROM unnest($1::int4[]) AS pid", &[&pids]).await.expect("terminate");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps14_the_next_query_after_70_seconds_succeeds_on_the_first_attempt() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    run(&source, "SELECT 1", wide()).await.expect("the first query");
    assert_eq!(source.idle_connections(), 1);
    let before = proxy.connections();
    // The pool closes an idle session after 60 s.
    tokio::time::sleep(Duration::from_secs(66)).await;
    assert_eq!(source.idle_connections(), 0, "the session idled for more than 60 s");
    tokio::time::sleep(Duration::from_secs(4)).await;
    let f = run(&source, "SELECT 2 AS two", wide()).await.expect("the first attempt succeeds");
    assert_eq!(cell(&f, 0, 0), "2");
    assert_eq!(proxy.connections(), before + 1, "one new session, no retry");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps14_a_session_that_ends_before_begin_gives_one_retry_and_a_success() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let a = admin().await;
    run(&source, "SELECT 1", wide()).await.expect("the first query");
    assert_eq!(source.idle_connections(), 1);
    // The admin session ends the idle session. The client crate may see it before or at the BEGIN.
    terminate(&a, &proxy.pids()).await;
    let f = run(&source, "SELECT 2 AS two", wide()).await.expect("one retry and a success");
    assert_eq!(cell(&f, 0, 0), "2");
    assert_eq!(source.state(), SourceState::Up);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps14_a_reused_session_that_breaks_at_begin_gives_one_retry_and_a_success() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    run(&source, "SELECT 1", wide()).await.expect("the first query");
    let before = proxy.connections();
    // The next BEGIN ends its connection before the server sees it: the reused session is dead.
    proxy.kill_next_simple_query();
    let f = run(&source, "SELECT 2 AS two", wide()).await.expect("one retry and a success");
    assert_eq!(cell(&f, 0, 0), "2");
    assert_eq!(proxy.connections(), before + 1, "exactly one new session");
    // The caller text ran one time.
    assert_eq!(proxy.parses().iter().filter(|p| p.ends_with("SELECT 2 AS two")).count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps14_a_session_that_ends_after_begin_gives_503_and_no_second_run() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let a = admin().await;
    run(&source, "SELECT 1", wide()).await.expect("the first query");
    let c = ctx();
    let name = app_name(&c.query_id);
    let task = {
        let source = source.clone();
        tokio::spawn(async move {
            source
                .query(PreparedQuery::PostgresSql { sql: "SELECT pg_sleep(20)::text AS slept".to_owned() }, wide(), c)
                .await
        })
    };
    wait_running(&a, &name, "pg_sleep(20)").await;
    // The admin session ends the session after BEGIN READ ONLY.
    let pid: i32 = a
        .query_one("SELECT pid FROM pg_stat_activity WHERE application_name = $1", &[&name])
        .await
        .expect("the session of the query")
        .get(0);
    terminate(&a, &[pid]).await;
    let e = expect_err(task.await.expect("the task ends"));
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
    assert_eq!(source.state(), SourceState::Down);
    let runs = proxy.parses().iter().filter(|p| p.ends_with("SELECT pg_sleep(20)::text AS slept")).count();
    assert_eq!(runs, 1, "the proxy shows no second run");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps14_admission_queue_wait_and_subject_limit() {
    // One place: the second query waits for `queue_wait` and gets `source_busy`.
    let mut config = ro_config();
    config.max_concurrent = 1;
    config.queue_wait = Duration::from_millis(400);
    let source = make_source(config);
    run(&source, "SELECT 1", wide()).await.expect("warm up: the first probe and a session");
    let slow = {
        let source = source.clone();
        tokio::spawn(async move { run(&source, "SELECT pg_sleep(2)::text", wide()).await })
    };
    let a = admin().await;
    // Wait until the first query holds its place.
    let start = Instant::now();
    while source.admission_stats().in_flight == 0 && start.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(source.admission_stats().in_flight, 1);
    let c = ctx();
    let wait = c.queue_wait.clone();
    let e = expect_err(source.query(PreparedQuery::PostgresSql { sql: "SELECT 2".to_owned() }, wide(), c).await);
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_busy", 503, true));
    let waited = wait.get().expect("the queue wait is set");
    assert!(waited >= Duration::from_millis(350) && waited < Duration::from_secs(2), "{waited:?}");
    slow.await.expect("task").expect("the first query ends well");
    assert_eq!(source.admission_stats().in_flight, 0);
    drop(a);

    // One place for each subject: the same subject waits, another subject runs.
    let mut config = ro_config();
    config.max_concurrent = 3;
    config.queue_wait = Duration::from_millis(300);
    config.max_concurrent_per_subject = Some(1);
    let source = make_source(config);
    run(&source, "SELECT 1", wide()).await.expect("warm up");
    let alice = {
        let source = source.clone();
        tokio::spawn(async move {
            source
                .query(
                    PreparedQuery::PostgresSql { sql: "SELECT pg_sleep(1.5)::text".to_owned() },
                    wide(),
                    ctx_for("user:alice"),
                )
                .await
        })
    };
    let start = Instant::now();
    while source.admission_stats().in_flight == 0 && start.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let e = expect_err(
        source.query(PreparedQuery::PostgresSql { sql: "SELECT 2".to_owned() }, wide(), ctx_for("user:alice")).await,
    );
    assert_eq!(e.code(), "source_busy");
    let bob = source
        .query(PreparedQuery::PostgresSql { sql: "SELECT 3 AS three".to_owned() }, wide(), ctx_for("user:bob"))
        .await
        .expect("another subject runs");
    assert_eq!(cell(&bob, 0, 0), "3");
    alice.await.expect("task").expect("alice ends well");
}

// PS15: attribution.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps15_pg_stat_activity_holds_the_name_and_the_comment() {
    let source = make_source(ro_config());
    let a = admin().await;
    let c = ctx();
    let id = c.query_id;
    let name = app_name(&id);
    assert!(name.len() <= 63);
    let task = {
        let source = source.clone();
        tokio::spawn(async move {
            source
                .query(PreparedQuery::PostgresSql { sql: "SELECT pg_sleep(10)::text AS slept".to_owned() }, wide(), c)
                .await
        })
    };
    wait_sessions(&a, &name, 1, Duration::from_secs(10)).await.expect("the query runs");
    let row = a
        .query_one("SELECT application_name, query FROM pg_stat_activity WHERE application_name = $1", &[&name])
        .await
        .expect("pg_stat_activity");
    let (app, query): (String, String) = (row.get(0), row.get(1));
    assert_eq!(app, name);
    let expected_start = format!(
        "/*hune {{\"app\":\"hune\",\"kind\":\"panel\",\"uid\":\"nodes\",\"panel\":4,\"query_id\":\"{id}\",\"request_id\":\"r-5f2c\",\"subject\":\"user%3Aalice\"}}*/\nSELECT pg_sleep(10)"
    );
    assert!(query.starts_with(&expected_start), "{query}");
    task.abort();
    let _ = task.await;
    wait_sessions(&a, &name, 0, Duration::from_secs(3)).await.expect("the query left");
    assert_eq!(sessions_named(&a, "hune/probe").await, 0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps15_for_1000_random_subjects_the_comment_never_closes_before_its_end() {
    let proxy = Proxy::start().await;
    let source = make_source(via(&proxy));
    let mut rng = Xorshift(0x9e37_79b9_7f4a_7c15);
    let pool: Vec<char> = "*/*/'\"\\-;%_ \n\t:@u\u{0}\u{7f}é日\u{1f600}abcXYZ019".chars().collect();
    for _ in 0..1000 {
        let len = (rng.next() % 300) as usize;
        let subject: String = (0..len).map(|_| pool[(rng.next() % pool.len() as u64) as usize]).collect();
        let c = ctx_for(&subject);
        let f = source
            .query(PreparedQuery::PostgresSql { sql: "SELECT 1 AS one".to_owned() }, wide(), c)
            .await
            .unwrap_or_else(|e| panic!("subject {subject:?}: {e}"));
        assert_eq!(cell(&f, 0, 0), "1");
    }
    let texts: Vec<String> = proxy.parses().into_iter().filter(|p| p.starts_with("/*hune ")).collect();
    assert_eq!(texts.len(), 1000);
    for t in &texts {
        // The first `*/` is the end of the comment, and the SQL follows on the next line.
        let end = t.find("*/").expect("the comment closes");
        assert_eq!(&t[end + 2..], "\nSELECT 1 AS one", "{t:?}");
        assert!(t[..end + 2].len() <= 1024 + 2, "{}", t.len());
        assert!(!t[8..end].contains("/*"), "a comment never opens inside: {t:?}");
    }
}

// Security: the source never holds a secret in an error.

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a PostgreSQL container"]
async fn ps12_the_errors_of_a_source_hold_no_secret_and_no_text_of_the_query() {
    let source = make_source(ro_config());
    let sql = "SELECT * FROM metrics.no_such_table_zz";
    let e = expect_err(run(&source, sql, wide()).await);
    assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(Some("42P01"))));
    let text = format!("{e} {e:?}");
    assert!(!text.contains(&env("HUNE_TEST_PGS_RO_PASSWORD")));
    // A session that got an ErrorResponse and cleaned up goes back to the pool.
    assert_eq!(source.idle_connections(), 1);
    // The next query on the same session runs.
    let again = run(&source, "SELECT 5 AS five", wide()).await.expect("works");
    assert_eq!(cell(&again, 0, 0), "5");
}
