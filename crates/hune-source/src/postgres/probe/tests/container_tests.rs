//! Container tests of the probe and of the error map (8a.8, 8a.12, 16.9a).
//!
//! The tests are `#[ignore]`. The script `tests/ci/pg-probe.sh` starts a disposable PostgreSQL on
//! 127.0.0.1, loads `tests/fixtures/postgres-probe` and runs them with `-- --ignored`. The script
//! gives `HUNE_TEST_PG_PORT`, `HUNE_TEST_PG_PASSWORD` (the password of the `probe_*` roles),
//! `HUNE_TEST_PG_ADMIN_PASSWORD` and `HUNE_TEST_PG_VERSION`.
//!
//! The tests send SQL only through `query` and `execute`, the extended protocol. They check the
//! verdict of the probe for a read-only role, for a hardened role, and for one role with each
//! forbidden right. They also map real server errors by their SQLSTATE.

use std::time::Duration;

use hune_core::ids::SourceId;
use tokio_postgres::{Client, Config, NoTls};

use super::*;
use crate::error::{ErrorCtx, SourceError};
use crate::postgres::errors::{MapInput, map_error, map_sqlstate};
use crate::types::{EffectiveLimits, Uuid};

const REASON: &str = "needs a PostgreSQL container: run tests/ci/pg-probe.sh";

/// The startup parameters of 8a.4 that R0 reads back.
const OPTIONS: &str = "-c default_transaction_read_only=on -c standard_conforming_strings=on \
-c client_encoding=UTF8 -c TimeZone=UTC -c search_path=metrics -c lock_timeout=5000";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: {REASON}"))
}

fn port() -> u16 {
    env("HUNE_TEST_PG_PORT").parse().expect("port")
}

async fn connect_as(
    user: &str,
    password: &str,
    database: &str,
    options: Option<&str>,
) -> Result<Client, tokio_postgres::Error> {
    let mut config = Config::new();
    config.host("127.0.0.1").port(port()).user(user).password(password).dbname(database);
    config.application_name("hune/probe");
    config.connect_timeout(Duration::from_secs(10));
    if let Some(options) = options {
        config.options(options);
    }
    let (client, connection) = config.connect(NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}

async fn connect(user: &str, database: &str) -> Client {
    connect_as(user, &env("HUNE_TEST_PG_PASSWORD"), database, Some(OPTIONS)).await.expect("connect")
}

async fn probe_of(user: &str, database: &str) -> Report {
    let mut client = connect(user, database).await;
    run_probe(&mut client, &["metrics".to_owned()]).await.expect("probe runs")
}

fn expect_fail(report: &Report, check: Check, needle: &str) {
    let failure = report.failure.as_ref().unwrap_or_else(|| panic!("the probe passed, {check:?} expected"));
    assert_eq!(failure.check, check, "{}", failure.detail);
    assert!(failure.detail.starts_with(&format!("{}: ", check.as_str())), "{}", failure.detail);
    assert!(failure.detail.contains(needle), "{:?} not in {}", needle, failure.detail);
    assert!(report.advice.is_empty(), "no advice after a failed check");
    let probe = finish(SourceState::Up, &Outcome::Checked(report.clone()));
    assert_eq!(probe.state, SourceState::Misconfigured);
    assert_eq!(probe.detail.as_deref(), Some(failure.detail.as_str()));
}

fn names(report: &Report) -> Vec<&'static str> {
    report.advice.iter().map(Advice::check).collect()
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_a_read_only_role_is_up_and_the_default_install_has_all_advice() {
    let report = probe_of("probe_ok", "probe_db").await;
    assert_eq!(report.failure, None, "a read-only role must pass R0 to R8");
    let expected_major = env("HUNE_TEST_PG_VERSION");
    let p = finish(SourceState::Unknown, &Outcome::Checked(report.clone()));
    assert_eq!(p.state, SourceState::Up);
    assert_eq!(p.version_supported, Some(true));
    assert!(p.version.as_deref().unwrap().starts_with(&format!("{expected_major}.")), "{:?}", p.version);
    // A default server: DO on plpgsql, TEMPORARY, pg_sleep and the like, and no temp_file_limit.
    assert_eq!(names(&report), ["A1", "A2", "A3", "A4"]);
    let a3 = &report.advice[2];
    assert!(a3.names.len() <= 5 && a3.names.iter().all(|n| n.starts_with("pg_")), "{:?}", a3.names);
    assert_eq!(report.advice[3].names, ["temp_file_limit=-1"]);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_a_hardened_role_has_no_advice() {
    let report = probe_of("probe_hard", "probe_hard").await;
    assert_eq!(report.failure, None);
    assert_eq!(report.advice, [], "the closure of A1 to A4 must remove each advice");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_each_forbidden_attribute_fails_r1() {
    for (role, attribute) in [
        ("probe_super", "SUPERUSER"),
        ("probe_createrole", "CREATEROLE"),
        ("probe_createdb", "CREATEDB"),
        ("probe_replication", "REPLICATION"),
        ("probe_bypassrls", "BYPASSRLS"),
    ] {
        let report = probe_of(role, "probe_db").await;
        expect_fail(&report, Check::R1, attribute);
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_dangerous_memberships_fail_r2() {
    for (role, group) in [
        ("probe_member_pg_read_all_data", "pg_read_all_data"),
        ("probe_member_pg_execute", "pg_execute_server_program"),
        // A NOINHERIT member can still use `SET ROLE` (ID-PG3).
        ("probe_member_noinherit", "pg_monitor"),
        // A role with a write attribute.
        ("probe_member_attr", "grp_createdb"),
    ] {
        let report = probe_of(role, "probe_db").await;
        expect_fail(&report, Check::R2, group);
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_create_rights_fail_r3() {
    expect_fail(&probe_of("probe_schema_create", "probe_db").await, Check::R3, "schema metrics");
    expect_fail(&probe_of("probe_db_create", "probe_db").await, Check::R3, "the database");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_table_and_view_write_rights_fail_r4() {
    for role in [
        "probe_t_insert",
        "probe_t_update",
        "probe_t_delete",
        "probe_t_truncate",
        "probe_t_references",
        "probe_t_trigger",
    ] {
        expect_fail(&probe_of(role, "probe_db").await, Check::R4, "metrics.t");
    }
    expect_fail(&probe_of("probe_view_update", "probe_db").await, Check::R4, "metrics.v");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_sequence_and_column_rights_fail_r5_and_r6() {
    expect_fail(&probe_of("probe_seq_update", "probe_db").await, Check::R5, "metrics.seq");
    expect_fail(&probe_of("probe_col_insert", "probe_db").await, Check::R6, "metrics.t.v");
    expect_fail(&probe_of("probe_col_update", "probe_db").await, Check::R6, "metrics.t.v");
    expect_fail(&probe_of("probe_col_references", "probe_db").await, Check::R6, "metrics.t.id");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_dblink_and_file_functions_fail_r7() {
    // The extension gives EXECUTE on its functions to PUBLIC: a plain read-only role fails.
    expect_fail(&probe_of("probe_ok", "probe_dblink").await, Check::R7, "dblink");
    // One explicit grant on a file function.
    expect_fail(&probe_of("probe_readfile", "probe_db").await, Check::R7, "pg_read_file");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_the_functions_that_run_text_or_signal_fail_r7_also_through_public() {
    // Finding H1. `probe_db` has the revoke of 8a.13: the role with an explicit grant fails, the others pass.
    for (role, function) in [
        ("probe_exec_term", "pg_terminate_backend"),
        ("probe_exec_cancel", "pg_cancel_backend"),
        ("probe_exec_emit", "pg_logical_emit_message"),
        ("probe_exec_tsstat", "ts_stat"),
        ("probe_exec_tsrewrite", "ts_rewrite"),
    ] {
        expect_fail(&probe_of(role, "probe_db").await, Check::R7, function);
    }
    // `probe_public` is a default install: PUBLIC runs the five functions, so a plain read-only role fails.
    let report = probe_of("probe_ok", "probe_public").await;
    expect_fail(&report, Check::R7, "(");
    // The detail lists the first names: at most five of the functions.
    let detail = &report.failure.as_ref().unwrap().detail;
    assert!(
        ["pg_terminate_backend", "pg_cancel_backend", "pg_logical_emit_message", "ts_stat", "ts_rewrite"]
            .iter()
            .any(|f| detail.contains(f)),
        "{detail}"
    );
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_a_membership_without_inherit_is_checked_as_the_role_itself() {
    // Finding M3 (a): the group holds the right. A member with NOINHERIT does not use it at once, but it can
    // run `SET ROLE`. R3 to R8 look at each role that the account is a member of.
    expect_fail(&probe_of("probe_noinh_r3", "probe_db").await, Check::R3, "schema metrics");
    expect_fail(&probe_of("probe_noinh_r4", "probe_db").await, Check::R4, "metrics.t");
    expect_fail(&probe_of("probe_noinh_r5", "probe_db").await, Check::R5, "metrics.seq");
    expect_fail(&probe_of("probe_noinh_r6", "probe_db").await, Check::R6, "metrics.t.v");
    expect_fail(&probe_of("probe_noinh_r7", "probe_db").await, Check::R7, "pg_read_file");
    expect_fail(&probe_of("probe_noinh_r8", "probe_fdw").await, Check::R8, "probe_remote");
    // An inherited membership, and a membership through a group of a group.
    expect_fail(&probe_of("probe_inherit_r4", "probe_db").await, Check::R4, "metrics.t");
    expect_fail(&probe_of("probe_indirect_r4", "probe_db").await, Check::R4, "metrics.t");
    // The same groups in a database where they hold nothing: the role passes.
    assert_eq!(probe_of("probe_noinh_r4", "probe_hard").await.failure, None);
    // A plain group that holds only SELECT does not fail a member.
    assert_eq!(probe_of("probe_ok", "probe_db").await.failure, None);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_an_owner_fails_r9() {
    // Finding M3 (b): an owner can grant itself each right again, so the probe refuses it at once, also when its
    // own rights are revoked (R3 to R8 see nothing).
    expect_fail(&probe_of("probe_owner_table", "probe_db").await, Check::R9, "table metrics.owned");
    // The owner is a group of the account, with NOINHERIT.
    expect_fail(&probe_of("probe_owner_member", "probe_db").await, Check::R9, "table metrics.owned_group");
    expect_fail(&probe_of("probe_owner_schema", "probe_db").await, Check::R9, "schema own_schema");
    expect_fail(&probe_of("probe_owner_function", "probe_db").await, Check::R9, "function own_fn()");
    expect_fail(&probe_of("probe_owner_type", "probe_db").await, Check::R9, "type metrics.own_type");
    expect_fail(&probe_of("probe_owner_lo", "probe_db").await, Check::R9, "large object 424242");
    // A role that owns nothing passes, and the objects of the others do not count.
    assert_eq!(probe_of("probe_ok", "probe_db").await.failure, None);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_wrappers_and_servers_fail_r8() {
    expect_fail(&probe_of("probe_fdw", "probe_fdw").await, Check::R8, "postgres_fdw");
    expect_fail(&probe_of("probe_srv", "probe_fdw").await, Check::R8, "probe_remote");
    // The same database, a role without USAGE: the objects exist and the role passes.
    assert_eq!(probe_of("probe_ok", "probe_fdw").await.failure, None);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_wrong_session_parameters_fail_r0() {
    // No startup parameters: the session is read-write and its search_path is the default.
    let mut client = connect_as("probe_ok", &env("HUNE_TEST_PG_PASSWORD"), "probe_db", None).await.unwrap();
    let report = run_probe(&mut client, &["metrics".to_owned()]).await.unwrap();
    expect_fail(&report, Check::R0, "default_transaction_read_only");
    // The parameters are right and the key differs.
    let mut client = connect("probe_ok", "probe_db").await;
    let report = run_probe(&mut client, &["other".to_owned()]).await.unwrap();
    expect_fail(&report, Check::R0, "search_path");
    // Blanks in the key do not matter.
    let mut client = connect("probe_ok", "probe_db").await;
    let report = run_probe(&mut client, &[" metrics ".to_owned()]).await.unwrap();
    assert_eq!(report.failure, None);
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_leaves_nothing_open_and_changes_nothing() {
    let mut client = connect("probe_ok", "probe_db").await;
    let first = run_probe(&mut client, &["metrics".to_owned()]).await.unwrap();
    // The connection is out of the transaction after ROLLBACK: the probe runs again on it.
    let second = run_probe(&mut client, &["metrics".to_owned()]).await.unwrap();
    assert_eq!(first, second);
    let row = client.query_one("SELECT txid_current_if_assigned() IS NULL", &[]).await.unwrap();
    assert!(row.get::<_, bool>(0), "the probe assigned no transaction id");
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_a_login_fault_makes_the_source_misconfigured() {
    let wrong = connect_as("probe_ok", "not-the-password", "probe_db", Some(OPTIONS)).await.unwrap_err();
    assert_eq!(errors::sqlstate_of(&wrong).as_deref(), Some("28P01"));
    let p = finish(SourceState::Up, &Outcome::from_error(&wrong));
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(!p.detail.unwrap().contains("not-the-password"));
    let missing = connect_as("probe_ok", &env("HUNE_TEST_PG_PASSWORD"), "no_such_db", Some(OPTIONS)).await.unwrap_err();
    assert_eq!(errors::sqlstate_of(&missing).as_deref(), Some("3D000"));
    assert_eq!(finish(SourceState::Up, &Outcome::from_error(&missing)).state, SourceState::Misconfigured);
}

// The error map on real server errors (8a.8).

fn limits() -> EffectiveLimits {
    EffectiveLimits {
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 1000,
        max_result_bytes: 33_554_432,
        max_query_size: 1_048_576,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    }
}

fn id() -> SourceId {
    SourceId::parse("metrics").unwrap()
}

fn query_id() -> Uuid {
    Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap()
}

/// Runs `sql` in a read-only transaction as `probe_ok` and maps the error.
async fn mapped(sql: &str) -> SourceError {
    mapped_with(sql, "SELECT 1").await
}

async fn mapped_with(sql: &str, before: &str) -> SourceError {
    let mut client = connect("probe_ok", "probe_db").await;
    let tx = client.build_transaction().read_only(true).start().await.unwrap();
    tx.query(before, &[]).await.unwrap();
    let error = tx.query(sql, &[]).await.expect_err("the statement must fail");
    let (id, l) = (id(), limits());
    map_error(&ErrorCtx { source: &id, query_id: query_id(), limits: &l }, &error, false)
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_real_errors_map_by_sqlstate() {
    let e = mapped("SELECT 1/0").await;
    assert_eq!((e.code(), e.http_status(), e.sqlstate()), ("query_error", 422, Some(Some("22012"))));
    assert_eq!(e.message(), "division by zero");

    let e = mapped("SELECT * FROM metrics.nope").await;
    assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(Some("42P01"))));

    let e = mapped("SELEC 1").await;
    assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(Some("42601"))));

    // The read-only transaction refuses a write before the privilege check.
    let e = mapped("CREATE TABLE metrics.zz (a int)").await;
    assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(Some("25006"))));

    // A table without a grant.
    let e = mapped("SELECT * FROM metrics.hidden").await;
    assert_eq!((e.code(), e.http_status()), ("source_access_denied", 502));
    assert_eq!(e.query_id(), Some(query_id()));

    // The server stops the statement at its timeout.
    let e = mapped_with("SELECT pg_sleep(5)", "SELECT set_config('statement_timeout', '200', true)").await;
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_a_lock_timeout_maps_to_a_timeout() {
    // The administrator holds an exclusive lock. The reader waits for `lock_timeout` and fails.
    let mut admin = connect_as("postgres", &env("HUNE_TEST_PG_ADMIN_PASSWORD"), "probe_db", None).await.unwrap();
    let locker = admin.build_transaction().start().await.unwrap();
    locker.execute("LOCK TABLE metrics.t IN ACCESS EXCLUSIVE MODE", &[]).await.unwrap();
    let e = mapped_with("SELECT * FROM metrics.t", "SELECT set_config('lock_timeout', '200', true)").await;
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
    locker.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container: run tests/ci/pg-probe.sh"]
async fn pg_probe_the_message_is_the_first_line_of_the_server_message() {
    // `RAISE`-free way to get a message with data: a cast error holds the value.
    let e = mapped("SELECT 'secret-value'::int").await;
    assert_eq!(e.sqlstate(), Some(Some("22P02")));
    assert!(e.message().contains("secret-value"), "the page gets the message: {}", e.message());
    assert!(!e.message().contains('\n'));
    // The SQLSTATE alone is what a log line holds.
    let (id, l) = (id(), limits());
    let ctx = ErrorCtx { source: &id, query_id: query_id(), limits: &l };
    let unavailable = map_sqlstate(
        &ctx,
        MapInput { sqlstate: Some("57P01"), message: "secret-value", cancel_sent_for_client_left: false },
    );
    assert!(!unavailable.message().contains("secret-value"));
}
