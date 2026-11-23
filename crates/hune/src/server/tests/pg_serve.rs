//! `serve` on the PostgreSQL backend (5.23 to 5.31, 14.12 steps 6 to 8).
//!
//! The tests of the option mapping run always. The tests with a server are ignored. Start them
//! with `tests/ci/pgwire.sh`, which runs PostgreSQL 15 and 18 in containers and sets the variables
//! that `pg_env` reads.

use super::api::{JSON, doc_text, etag_of, key, post_body, put};
use super::harness::*;
use super::pg_env::{PgEnv, Role};
use crate::config::{Config, PostgresConfig, Secret, SslMode, StorageConfig, TlsConfig};
use crate::log::Logger;
use crate::server::lifecycle::{self, Running};
use crate::server::startup::{self, pg_options};
use hune_store::{PgSslMode, migrate_store};
use serde_json::{Value, json};
use std::net::TcpListener as StdListener;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// A configuration of `storage.postgres` for a test. The poll is fast.
fn pg_config(env: &PgEnv, database: &str, role: Role, tls: bool) -> PostgresConfig {
    let (user, password) = env.credentials(role);
    PostgresConfig {
        host: env.host.clone(),
        port: env.port,
        database: database.to_owned(),
        user: user.to_owned(),
        password: Secret::new(password.to_owned()),
        sslmode: if tls { SslMode::VerifyFull } else { SslMode::Disable },
        tls: tls.then(|| TlsConfig {
            ca_file: env.ca_file.clone(),
            ca_pem: std::fs::read(&env.ca_file).unwrap(),
            server_name: env.server_name.clone(),
        }),
        max_connections: 4,
        connect_timeout: Duration::from_secs(2),
        statement_timeout: Duration::from_secs(10),
        poll_interval: Duration::from_millis(200),
        history_keep: 100,
    }
}

/// Applies the migrations with the owner role, through the library, as `migrate-store` does.
async fn migrate(env: &PgEnv, database: &str) {
    let options = pg_options(&pg_config(env, database, Role::Owner, false), false).unwrap();
    let report = migrate_store(&options).await.expect("migrate-store");
    assert_eq!(report.to, 1);
}

/// The configuration of a server on PostgreSQL.
fn config_for(pg: PostgresConfig) -> Config {
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(dir.path());
    config.storage = StorageConfig::Postgres(pg);
    config
}

/// Starts the server and gives the error of the start, if any.
async fn try_start(pg: PostgresConfig) -> (Result<Running, lifecycle::StartError>, Arc<Logger>) {
    let logger = Logger::memory(crate::config::LogLevel::Debug);
    let result = lifecycle::start(Arc::new(config_for(pg)), logger.clone(), test_timing(), startup::open_store).await;
    (result, logger)
}

async fn start_pg(pg: PostgresConfig) -> Harness {
    Harness::start_with(test_timing(), |_| {}, |c| c.storage = StorageConfig::Postgres(pg)).await
}

fn events_of(logger: &Logger) -> Vec<Value> {
    logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn count(events: &[Value], name: &str) -> usize {
    events.iter().filter(|e| e["event"] == name).count()
}

// ---- The option mapping (always run) ----

fn sample(sslmode: SslMode, tls: Option<TlsConfig>) -> PostgresConfig {
    PostgresConfig {
        host: "pg.example.internal".to_owned(),
        port: 5433,
        database: "hune_db".to_owned(),
        user: "hune_app".to_owned(),
        password: Secret::new("s3cret".to_owned()),
        sslmode,
        tls,
        max_connections: 7,
        connect_timeout: Duration::from_secs(3),
        statement_timeout: Duration::from_secs(11),
        poll_interval: Duration::from_secs(4),
        history_keep: 42,
    }
}

#[test]
fn pg_options_maps_each_key_without_a_change() {
    let o = pg_options(&sample(SslMode::Disable, None), true).unwrap();
    assert_eq!(
        (o.host.as_str(), o.port, o.database.as_str(), o.user.as_str()),
        ("pg.example.internal", 5433, "hune_db", "hune_app")
    );
    assert_eq!(o.password.as_deref(), Some("s3cret"));
    assert_eq!(o.sslmode, PgSslMode::Disable);
    assert_eq!(o.max_connections, 7);
    assert_eq!(o.connect_timeout, Duration::from_secs(3));
    assert_eq!(o.statement_timeout, Duration::from_secs(11));
    assert_eq!(o.poll_interval, Duration::from_secs(4));
    assert_eq!(o.history_keep, 42);
    assert!(o.check_role);
    assert!(!pg_options(&sample(SslMode::Disable, None), false).unwrap().check_role);
    // The debug text hides the password.
    assert!(!format!("{o:?}").contains("s3cret"));
}

#[test]
fn pg_options_verify_full_carries_the_ca_bytes_and_the_name() {
    let tls = TlsConfig {
        ca_file: "/nonexistent/ca.pem".into(),
        ca_pem: b"-----BEGIN CERTIFICATE-----\n".to_vec(),
        server_name: "pg.example.internal".to_owned(),
    };
    let o = pg_options(&sample(SslMode::VerifyFull, Some(tls)), false).unwrap();
    assert_eq!(
        o.sslmode,
        PgSslMode::VerifyFull {
            ca_pem: b"-----BEGIN CERTIFICATE-----\n".to_vec(),
            server_name: "pg.example.internal".to_owned()
        }
    );
}

#[test]
fn pg_options_verify_full_needs_the_tls_block() {
    let e = pg_options(&sample(SslMode::VerifyFull, None), false).unwrap_err();
    assert!(e.starts_with("storage.postgres.tls:"), "{e}");
}

#[test]
fn pg_options_ignore_the_tls_block_with_sslmode_disable() {
    let tls = TlsConfig { ca_file: "/x".into(), ca_pem: b"x".to_vec(), server_name: "n".to_owned() };
    let o = pg_options(&sample(SslMode::Disable, Some(tls)), false).unwrap();
    assert_eq!(o.sslmode, PgSslMode::Disable);
}

// ---- A TCP forward (ignored tests) ----

/// Forwards each connection on `127.0.0.1:listen` to `127.0.0.1:target`.
async fn forward(listen: u16, target: u16) -> JoinHandle<()> {
    let listener = TcpListener::bind(("127.0.0.1", listen)).await.expect("bind the forward");
    tokio::spawn(async move {
        loop {
            let Ok((mut inbound, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                if let Ok(mut outbound) = TcpStream::connect(("127.0.0.1", target)).await {
                    drop(copy_bidirectional(&mut inbound, &mut outbound).await);
                }
            });
        }
    })
}

fn free_port() -> u16 {
    StdListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

// ---- The tests with a server (ignored) ----

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_serve_over_tls_runs_the_api_on_postgres() {
    let env = PgEnv::from_env();
    let db = env.fresh_database("serve_api");
    migrate(&env, &db).await;
    let mut h = start_pg(pg_config(&env, &db, Role::App, true)).await;
    let ready = get(h.ops, "/readyz").await;
    assert_eq!(ready.status, 200, "{}", ready.text());
    let events = events_of(&h.logger);
    let listening = events.iter().find(|e| e["event"] == "server.listening").expect("server.listening");
    assert_eq!(listening["storage_backend"], "postgres");
    let loaded = events.iter().find(|e| e["event"] == "store.loaded").expect("store.loaded");
    assert_eq!(
        (loaded["backend"].as_str(), loaded["dashboards"].as_u64(), loaded["folders"].as_u64()),
        (Some("postgres"), Some(0), Some(0))
    );
    assert_eq!(count(&events, "store.sync_failed"), 0);
    // A folder, then a dashboard in it.
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"transit","title":"Transit desk"}"#).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let body = post_body("transit", &doc_text("pgdash", "First"));
    let created = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(created.status, 201, "{}", created.text());
    assert_eq!(created.json()["version"], 1);
    let e1 = etag_of(&created);
    // The save with the current etag gives version 2.
    let saved = put(&h, "pgdash", &e1, &doc_text("pgdash", "Second"), &key()).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    assert_eq!(saved.json()["version"], 2);
    let e2 = etag_of(&saved);
    assert_ne!(e1, e2);
    // A save with the old etag is a conflict that carries the current state.
    let stale = put(&h, "pgdash", &e1, &doc_text("pgdash", "Third"), &key()).await;
    assert_eq!((stale.status, stale.code().as_str()), (409, "conflict"));
    let err = stale.json()["error"].clone();
    assert_eq!((err["current_etag"].as_str(), err["current_version"].as_u64()), (Some(e2.as_str()), Some(2)));
    // The history lists both versions, newest first.
    let hist = get(h.addr, "/api/dashboards/pgdash/history").await.json();
    let versions: Vec<u64> = hist["items"].as_array().unwrap().iter().map(|i| i["version"].as_u64().unwrap()).collect();
    assert_eq!(versions, [2, 1]);
    assert_eq!(hist["items"][0]["kind"], "save");
    assert_eq!(hist["items"][1]["kind"], "create");
    let v1 = get(h.addr, "/api/dashboards/pgdash/history/1").await;
    assert_eq!(v1.json()["document"]["title"], "First");
    // The delete writes a tombstone. The dashboard is gone from the list.
    let del = call(
        h.addr,
        "DELETE",
        "/api/dashboards/pgdash",
        &[("If-Match", &format!("\"{e2}\"")), ("Idempotency-Key", &key())],
        b"",
    )
    .await;
    assert_eq!(del.status, 200, "{}", del.text());
    assert_eq!(get(h.addr, "/api/dashboards/pgdash").await.status, 404);
    assert_eq!(get(h.addr, "/api/dashboards").await.json()["items"], json!([]));
    let metrics = get(h.ops, "/metrics").await.text();
    assert!(metrics.contains("hune_store_up 1\n"), "{metrics}");
    assert!(metrics.contains("backend=\"postgres\""), "{metrics}");
    assert!(metrics.contains("hune_store_conflicts_total 1\n"), "{metrics}");
    let lines = h.logger.memory_lines().join("\n");
    assert!(!lines.contains("apppw"), "the log holds no password");
    assert_eq!(h.stop().await, 0);
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_serve_refuses_the_owner_role() {
    let env = PgEnv::from_env();
    let db = env.fresh_database("serve_owner");
    migrate(&env, &db).await;
    let (result, logger) = try_start(pg_config(&env, &db, Role::Owner, false)).await;
    let e = result.err().expect("the start fails");
    assert_eq!(e.step, "store.open");
    assert_eq!(
        e.error,
        format!(
            "storage.postgres: the role {} is a superuser or owns the Hune tables; serve needs a role with only \
             SELECT, INSERT, UPDATE and DELETE (5.25, 12.13)",
            env.owner_user
        )
    );
    let events = events_of(&logger);
    let failed = events.iter().find(|e| e["event"] == "server.start_failed").expect("server.start_failed");
    assert_eq!(failed["step"], "store.open");
    assert_eq!(failed["error"], e.error.as_str());
    assert_eq!(count(&events, "store.sync_failed"), 0, "a refused role is permanent: no retry");
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_serve_stops_at_once_on_a_permanent_error() {
    let env = PgEnv::from_env();
    // No schema: the message of 5.29 step 3 keeps the text `<file>`.
    let empty = env.fresh_database("serve_empty");
    let (result, logger) = try_start(pg_config(&env, &empty, Role::App, false)).await;
    let e = result.err().expect("no schema");
    assert_eq!(e.step, "store.open");
    assert_eq!(e.error, "storage.postgres: the database has no Hune schema; run: hune migrate-store -c <file>");
    assert_eq!(count(&events_of(&logger), "server.start_failed"), 1);
    // A wrong password: SQLSTATE class 28.
    let mut wrong = pg_config(&env, &empty, Role::App, false);
    wrong.password = Secret::new("not-the-password".to_owned());
    let (result, logger) = try_start(wrong).await;
    let e = result.err().expect("wrong password");
    assert!(e.error.starts_with("storage.postgres: cannot connect to "), "{}", e.error);
    assert!(!e.error.contains("not-the-password"));
    assert_eq!(count(&events_of(&logger), "store.sync_failed"), 0);
    // No such database: SQLSTATE 3D000.
    let (result, _) = try_start(pg_config(&env, "vgw_no_such_database", Role::App, false)).await;
    assert!(result.err().expect("no database").error.contains("3D000"));
    // A CA that did not sign the certificate.
    let mut bad_ca = pg_config(&env, &empty, Role::App, true);
    let wrong_ca = std::env::var("HUNE_PGW_WRONG_CA_FILE").expect("HUNE_PGW_WRONG_CA_FILE");
    bad_ca.tls.as_mut().unwrap().ca_pem = std::fs::read(wrong_ca).unwrap();
    let (result, logger) = try_start(bad_ca).await;
    result.err().expect("the certificate check fails");
    assert_eq!(count(&events_of(&logger), "store.sync_failed"), 0, "a failed certificate check is permanent");
    // The old schema and the new one: the version check.
    migrate(&env, &empty).await;
    env.admin(&empty, "UPDATE hune_schema SET version = 2");
    let (result, _) = try_start(pg_config(&env, &empty, Role::App, false)).await;
    let e = result.err().expect("newer schema");
    assert_eq!(
        e.error,
        "storage.postgres: database schema version 2 is newer than this binary (1); use a binary of schema version 2"
    );
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_serve_waits_for_postgres_and_logs_one_sync_failed() {
    let env = PgEnv::from_env();
    let db = env.fresh_database("serve_wait");
    migrate(&env, &db).await;
    // Nothing listens on the port that the configuration names.
    let port = free_port();
    let mut pg = pg_config(&env, &db, Role::App, false);
    pg.port = port;
    pg.connect_timeout = Duration::from_millis(300);
    let logger = Logger::memory(crate::config::LogLevel::Debug);
    let config = Arc::new(config_for(pg));
    let task: JoinHandle<Result<Running, lifecycle::StartError>> =
        tokio::spawn(lifecycle::start(config, logger.clone(), test_timing(), startup::open_store));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!task.is_finished(), "the start waits and does not stop");
    let events = events_of(&logger);
    assert_eq!(count(&events, "store.sync_failed"), 1, "one line for one change of state: {events:?}");
    let failed = events.iter().find(|e| e["event"] == "store.sync_failed").unwrap();
    assert_eq!(failed["backend"], "postgres");
    assert_eq!(count(&events, "server.start_failed"), 0);
    assert_eq!(count(&events, "server.ready"), 0);
    // PostgreSQL comes back: the start goes on to `/readyz` 200.
    let relay = forward(port, env.port).await;
    let running = tokio::time::timeout(Duration::from_secs(15), task).await.expect("the start ends").unwrap().unwrap();
    let ready = get(running.ops_addr, "/readyz").await;
    assert_eq!(ready.status, 200, "{}", ready.text());
    let events = events_of(&logger);
    assert_eq!(count(&events, "store.sync_failed"), 1);
    assert_eq!(count(&events, "store.sync_recovered"), 1);
    assert_eq!(count(&events, "server.ready"), 1);
    let (_tx, mut rx) = mpsc::unbounded_channel();
    assert_eq!(running.stop("SIGTERM", &mut rx).await, 0);
    relay.abort();
}

#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_two_instances_share_the_store_through_the_poll() {
    let env = PgEnv::from_env();
    let db = env.fresh_database("serve_two");
    migrate(&env, &db).await;
    let mut a = start_pg(pg_config(&env, &db, Role::App, false)).await;
    let mut b = start_pg(pg_config(&env, &db, Role::App, false)).await;
    let r = call(a.addr, "POST", "/api/folders", &[JSON], br#"{"id":"shared","title":"Shared"}"#).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let body = post_body("shared", &doc_text("seen-by-b", "Seen"));
    let r = call(a.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(r.status, 201, "{}", r.text());
    // Instance B has no `dashboard.changed` for it: it learns the change by the poll (5.30).
    let mut seen = false;
    for _ in 0..100 {
        if get(b.addr, "/api/dashboards/seen-by-b").await.status == 200 {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(seen, "B sees the dashboard of A after the poll");
    assert_eq!(count(&events_of(&b.logger), "dashboard.changed"), 0);
    // B saves with the etag that A made: the store accepts it, and A sees the new etag later.
    let etag = etag_of(&get(b.addr, "/api/dashboards/seen-by-b").await);
    let saved = put(&b, "seen-by-b", &etag, &doc_text("seen-by-b", "Seen twice"), &key()).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    let mut synced = false;
    for _ in 0..100 {
        let got = get(a.addr, "/api/dashboards/seen-by-b").await;
        if got.status == 200 && got.json()["version"] == 2 {
            synced = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(synced, "A sees the version of B after the poll");
    assert_eq!(a.stop().await, 0);
    assert_eq!(b.stop().await, 0);
}

/// One restore of the API validates its version one time on PostgreSQL too (6.18, decision ID-K321). The handler
/// validates the version, and the transaction of the store validates nothing. Before the fix: 2 validations.
#[tokio::test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
async fn pg_serve_a_restore_validates_its_version_one_time() {
    let env = PgEnv::from_env();
    let db = env.fresh_database("serve_restore_once");
    migrate(&env, &db).await;
    let mut h = start_pg(pg_config(&env, &db, Role::App, false)).await;
    let r = call(h.addr, "POST", "/api/folders", &[JSON], br#"{"id":"transit","title":"Transit desk"}"#).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let body = post_body("transit", &doc_text("pgrestore", "First"));
    let created = call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &body).await;
    assert_eq!(created.status, 201, "{}", created.text());
    let e1 = etag_of(&created);
    let saved = put(&h, "pgrestore", &e1, &doc_text("pgrestore", "Second"), &key()).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    let e2 = format!("\"{}\"", etag_of(&saved));
    let counts = |h: &Harness| {
        let state = h.state();
        let store = state.services().expect("the store is open").store.restore_checks();
        (state.validations_run.load(std::sync::atomic::Ordering::SeqCst), store)
    };
    for (if_match, status) in [(super::api::ZERO_ETAG, 409), (e2.as_str(), 200)] {
        let before = counts(&h);
        let r = call(
            h.addr,
            "POST",
            "/api/dashboards/pgrestore/restore",
            &[JSON, ("If-Match", if_match), ("Idempotency-Key", &key())],
            br#"{"version":1}"#,
        )
        .await;
        assert_eq!(r.status, status, "{}", r.text());
        let after = counts(&h);
        assert_eq!((after.0 - before.0, after.1 - before.1), (1, 0), "validations of the handler and of the store");
    }
    assert_eq!(etag_of(&get(h.addr, "/api/dashboards/pgrestore").await), e1);
    assert_eq!(h.stop().await, 0);
}
