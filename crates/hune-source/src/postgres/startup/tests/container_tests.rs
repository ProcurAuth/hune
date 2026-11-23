//! Container tests of the startup packet and the fixed texts (8a.4, 8a.5).
//!
//! The tests are `#[ignore]`. They need a disposable PostgreSQL on 127.0.0.1 and the variables
//! `HUNE_TEST_PGS_HOST`, `_PORT`, `_DATABASE`, `_ADMIN_USER` and `_ADMIN_PASSWORD` (the harness
//! `tests/ci/pgsource.sh` sets them; no TLS). They check that the server accepts each startup parameter of the table, that
//! the values read back are the table values, and that the fixed texts do their work.

use std::time::Duration;

use hune_core::ids::SourceId;
use tokio_postgres::{Client, NoTls};

use super::*;
use crate::postgres::fixed;
use crate::types::{LogKind, Uuid};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: needs a PostgreSQL container"))
}

fn source_config() -> PostgresSourceConfig {
    PostgresSourceConfig {
        id: SourceId::parse("test").expect("id"),
        label: "Test".to_owned(),
        host: env("HUNE_TEST_PGS_HOST"),
        port: env("HUNE_TEST_PGS_PORT").parse().expect("port"),
        database: env("HUNE_TEST_PGS_DATABASE"),
        user: env("HUNE_TEST_PGS_ADMIN_USER"),
        password: env("HUNE_TEST_PGS_ADMIN_PASSWORD"),
        search_path: vec!["public".to_owned(), "pg_catalog".to_owned()],
        sslmode: PostgresSslMode::Disable,
        tls: None,
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 1000,
        max_result_bytes: 1_000_000,
        max_query_size: 100_000,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(1),
        max_concurrent_per_subject: None,
    }
}

async fn connect() -> Client {
    let config = connection_config(&source_config()).expect("config");
    let (client, connection) = config.connect(NoTls).await.expect("the server accepts the startup parameters");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

async fn show(client: &Client, name: &str) -> String {
    // The name comes from a constant list of the test.
    let row = client.query_one("SELECT current_setting($1)", &[&name]).await.expect("setting");
    row.get::<_, String>(0)
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps_startup_values_read_back_as_the_table_of_8a4() {
    let c = connect().await;
    for (name, want) in [
        ("default_transaction_read_only", "on"),
        ("standard_conforming_strings", "on"),
        ("client_encoding", "UTF8"),
        ("TimeZone", "UTC"),
        ("DateStyle", "ISO, YMD"),
        ("search_path", "public,pg_catalog"),
        ("statement_timeout", "1min"),
        ("lock_timeout", "5s"),
        ("idle_in_transaction_session_timeout", "15s"),
        ("client_connection_check_interval", "1s"),
        ("application_name", "hune"),
    ] {
        assert_eq!(show(&c, name).await, want, "{name}");
    }
    // The expected values of R0 are the values that the server reads back.
    for (name, want) in expected_session_values(&source_config()).expect("values") {
        assert_eq!(show(&c, name).await, want, "{name}");
    }
}

#[tokio::test]
#[ignore = "needs a PostgreSQL container"]
async fn ps_fixed_texts_run_one_query_in_a_read_only_transaction() {
    let c = connect().await;
    let id = Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").expect("uuid");
    fixed::begin_read_only(&c).await.expect("begin");
    fixed::set_query_values(&c, Duration::from_secs(7), LogKind::Panel, &id).await.expect("set_config");
    assert_eq!(show(&c, "statement_timeout").await, "7s");
    assert_eq!(show(&c, "lock_timeout").await, "5s");
    assert_eq!(show(&c, "application_name").await, "hune/panel/019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d");
    assert_eq!(show(&c, "transaction_read_only").await, "on");
    // The transaction cannot write, even for a role that can.
    let e = c.query("CREATE TABLE hune_must_not_exist (a int)", &[]).await.expect_err("read only");
    assert_eq!(e.code().map(|s| s.code()), Some("25006"));
    fixed::rollback(&c).await.expect("rollback");
    // The values were local to the transaction.
    assert_eq!(show(&c, "statement_timeout").await, "1min");
    assert_eq!(show(&c, "application_name").await, "hune");
    fixed::discard_all(&c).await.expect("discard");
    // DISCARD ALL resets each value to the value of the startup packet, not to the server default:
    // the connection stays read-only and keeps its startup values.
    assert_eq!(show(&c, "default_transaction_read_only").await, "on");
    assert_eq!(show(&c, "statement_timeout").await, "1min");
    assert_eq!(show(&c, "application_name").await, "hune");
}
