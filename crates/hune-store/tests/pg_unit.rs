//! Tests of the PostgreSQL backend that need no server: the options, the TLS settings and the
//! errors of a connection that cannot work.
#![cfg(feature = "conformance")]

mod pg_support;

use std::time::Duration;

use hune_store::{
    MigrateError, MigrateReport, PgOpenErrorKind, PgOptions, PgSslMode, PgStore, migrate_store, postgres,
};

fn options(port: u16, sslmode: PgSslMode) -> PgOptions {
    PgOptions {
        host: "127.0.0.1".to_owned(),
        port,
        database: "hune".to_owned(),
        user: "hune_app".to_owned(),
        password: Some("secret-value-123".to_owned()),
        sslmode,
        max_connections: 2,
        connect_timeout: Duration::from_secs(1),
        statement_timeout: Duration::from_secs(1),
        poll_interval: Duration::from_secs(1),
        history_keep: 100,
        check_role: false,
    }
}

#[test]
fn options_debug_hides_the_password() {
    let o = options(5432, PgSslMode::VerifyFull { ca_pem: b"pem".to_vec(), server_name: "db".to_owned() });
    let text = format!("{o:?}");
    assert!(!text.contains("secret-value-123"), "{text}");
    assert!(text.contains("<hidden>"), "{text}");
}

#[tokio::test]
async fn open_without_server_is_temporary() {
    let port = pg_support::free_port();
    let e = PgStore::open(options(port, PgSslMode::Disable)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Temporary, "{e}");
    assert!(e.is_temporary());
}

#[tokio::test]
async fn open_waiting_calls_back_on_each_temporary_error() {
    let port = pg_support::free_port();
    let mut seen = 0;
    let fut = PgStore::open_waiting(options(port, PgSslMode::Disable), |_| seen += 1);
    // The call never ends without a server: stop it after the second wait.
    assert!(tokio::time::timeout(Duration::from_millis(2500), fut).await.is_err());
    assert!(seen >= 2, "seen {seen}");
}

#[tokio::test]
async fn a_ca_file_without_certificate_is_permanent() {
    let mode = PgSslMode::VerifyFull { ca_pem: b"no certificate here".to_vec(), server_name: "db".to_owned() };
    let e = PgStore::open(options(1, mode)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent);
    assert!(e.message.contains("storage.postgres.tls.ca_file"), "{e}");
}

#[tokio::test]
async fn migrate_store_without_server_gives_a_connect_error() {
    let port = pg_support::free_port();
    let e = migrate_store(&options(port, PgSslMode::Disable)).await.unwrap_err();
    assert!(matches!(e, MigrateError::Connect(_)), "{e:?}");
}

#[test]
fn migration_texts() {
    assert_eq!(MigrateReport { from: 0, to: 1, applied: vec![1] }.to_string(), "schema version 1");
    assert_eq!(MigrateReport { from: 1, to: 1, applied: vec![] }.to_string(), "schema version 1, nothing to do");
    assert_eq!(MigrateError::LockHeld.to_string(), "another hune migrate-store holds the lock");
    assert_eq!(
        MigrateError::Newer { database: 3, binary: 1 }.to_string(),
        "database schema 3 is newer than this binary (1)"
    );
    assert_eq!(postgres::SCHEMA_VERSION, 1);
    assert_eq!(postgres::MIGRATIONS.len(), 1);
    assert!(postgres::MIGRATIONS[0].sql.contains("CREATE TABLE hune_versions"));
}
