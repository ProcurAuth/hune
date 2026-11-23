// SPDX-License-Identifier: MIT OR Apache-2.0
//! The PostgreSQL tests of 5.36 (SP-01 to SP-19), the start checks of 5.29 and `migrate-store`
//! of 5.28. Each test makes its own databases on the server of the test run.
//!
//! The tests are ignored without a server: run `tests/ci/store-pg.sh`.
#![cfg(feature = "conformance")]

mod pg_support;

use std::time::{Duration, Instant};

use hune_store::conformance::{doc, fid, meta, uid};
use hune_store::postgres::{MIGRATIONS, MigrateError, Migration, migrate_store_with};
use hune_store::{
    AnyStore, Catalog, CatalogEntry, FolderTitle, Outcome, PgOpenErrorKind, PgSslMode, PgStore, Store, StoreError,
    VersionKind, migrate_store,
};
use pg_support::{Proxy, backend, ca_pem, config, server_name, wrong_ca_pem};

const IGNORE: &str = "needs PostgreSQL: tests/ci/store-pg.sh";

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).expect("title")
}

fn code_of(e: &tokio_postgres::Error) -> String {
    e.code().map(|c| c.code().to_owned()).unwrap_or_else(|| format!("no SQLSTATE: {e}"))
}

/// A store with the folder `f` and the dashboard `d1`.
async fn one_dashboard(store: &PgStore) -> hune_store::CanonicalDocument {
    let s = AnyStore::Postgres(store.clone());
    s.create_folder(&fid("f"), &title("Folder f")).await.unwrap();
    let d = doc("d1", "First");
    s.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    d
}

async fn numbers(s: &AnyStore, u: &str) -> Vec<u64> {
    s.history(&uid(u), None, 100).await.unwrap().iter().map(|v| v.version).collect()
}

/// SP-01: `serve` on an empty database stops with the message that names `hune migrate-store`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_01_empty_database() {
    let _ = IGNORE;
    let cfg = config();
    let db = backend().fresh_name();
    cfg.empty_database(&db).await;
    let e = PgStore::open(cfg.app_options(&db, 100)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::NoSchema, "{e}");
    assert!(e.message.contains("hune migrate-store"), "{e}");
    assert!(e.message.contains("the database has no Hune schema"), "{e}");
    let e = e.with_config_file("/etc/hune/migrate.hcl");
    assert!(e.message.contains("-c /etc/hune/migrate.hcl"), "{e}");
}

/// SP-02: a schema version above the binary, and a binary of version 2 on a database at 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_02_schema_version() {
    let cfg = config();
    let db = backend().new_database().await;
    let e = PgStore::open_expecting(cfg.app_options(&db, 100), 2).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::SchemaOlder, "{e}");
    assert!(e.message.contains("older") && e.message.contains("hune migrate-store"), "{e}");
    let admin = cfg.admin(&db).await;
    admin.client.execute("UPDATE hune_schema SET version = 7 WHERE id", &[]).await.unwrap();
    let e = PgStore::open(cfg.app_options(&db, 100)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::SchemaNewer, "{e}");
    assert!(e.message.contains("newer") && e.message.contains("schema version 7"), "{e}");
    let e = migrate_store(&cfg.owner_options(&db)).await.unwrap_err();
    assert_eq!(e, MigrateError::Newer { database: 7, binary: 1 });
}

/// SP-03: a database with the encoding `SQL_ASCII`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_03_encoding() {
    let cfg = config();
    let db = backend().fresh_name();
    cfg.ascii_database(&db).await;
    let e = PgStore::open(cfg.app_options(&db, 100)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Encoding, "{e}");
    assert!(e.message.contains("database encoding is SQL_ASCII; Hune needs UTF8"), "{e}");
    let e = migrate_store(&cfg.owner_options(&db)).await.unwrap_err();
    assert!(matches!(e, MigrateError::Encoding(ref x) if x == "SQL_ASCII"), "{e:?}");
}

/// SP-04: `migrate-store` on an empty database, two times. The grants of 5.25 hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_04_migrate_twice() {
    let cfg = config();
    let db = backend().fresh_name();
    cfg.empty_database(&db).await;
    let first = migrate_store(&cfg.owner_options(&db)).await.unwrap();
    assert_eq!((first.from, first.to, first.applied.clone()), (0, 1, vec![1]));
    assert_eq!(first.to_string(), "schema version 1");
    let second = migrate_store(&cfg.owner_options(&db)).await.unwrap();
    assert_eq!((second.from, second.to, second.applied.clone()), (1, 1, vec![]));
    assert_eq!(second.to_string(), "schema version 1, nothing to do");
    // The application role reads hune_schema only, and has the four rights on the other tables.
    let admin = cfg.admin(&db).await;
    let app = &cfg.app_user;
    let has = |table: &'static str, right: &'static str| {
        let app = app.clone();
        let admin = &admin;
        async move {
            let r = admin
                .client
                .query_one("SELECT has_table_privilege($1, $2, $3)", &[&app, &table, &right])
                .await
                .unwrap();
            r.get::<_, bool>(0)
        }
    };
    assert!(has("hune_schema", "SELECT").await);
    for r in ["INSERT", "UPDATE", "DELETE"] {
        assert!(!has("hune_schema", r).await, "hune_schema {r}");
    }
    for t in ["hune_folders", "hune_dashboards", "hune_versions"] {
        for r in ["SELECT", "INSERT", "UPDATE", "DELETE"] {
            assert!(has(t, r).await, "{t} {r}");
        }
    }
    // The owner role owns the tables, and the application role starts as `serve` needs.
    let store = PgStore::open(cfg.app_options(&db, 100)).await.unwrap();
    drop(store);
}

const SLOW: Migration = Migration {
    version: 2,
    sql: "SELECT pg_sleep(2); UPDATE hune_schema SET version = 2, updated_at = now() WHERE id",
};

/// SP-05: two `migrate-store` at the same time. One applies, the other stops on the lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_05_two_migrations_at_once() {
    let cfg = config();
    let db = backend().fresh_name();
    cfg.empty_database(&db).await;
    let list = [MIGRATIONS[0], SLOW];
    let opts = cfg.owner_options(&db);
    let first = {
        let opts = opts.clone();
        tokio::spawn(async move { migrate_store_with(&opts, &list).await })
    };
    tokio::time::sleep(Duration::from_millis(700)).await;
    let second = migrate_store_with(&opts, &list).await;
    assert_eq!(second, Err(MigrateError::LockHeld), "SP-05 second run");
    assert_eq!(second.unwrap_err().to_string(), "another hune migrate-store holds the lock");
    let first = first.await.unwrap().expect("SP-05 first run");
    assert_eq!((first.from, first.to, first.applied), (0, 2, vec![1, 2]));
    // The lock is free again.
    let third = migrate_store_with(&opts, &list).await.unwrap();
    assert!(third.applied.is_empty());
}

/// SP-05, second form: a session that holds the advisory lock stops `migrate-store`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_05_lock_held_by_a_session() {
    let cfg = config();
    let db = backend().fresh_name();
    cfg.empty_database(&db).await;
    let admin = cfg.admin(&db).await;
    admin.client.query_one("SELECT pg_advisory_lock(8532464671393841153)", &[]).await.unwrap();
    assert_eq!(migrate_store(&cfg.owner_options(&db)).await, Err(MigrateError::LockHeld));
    admin.client.query_one("SELECT pg_advisory_unlock(8532464671393841153)", &[]).await.unwrap();
    assert!(migrate_store(&cfg.owner_options(&db)).await.is_ok());
}

/// SP-06: a step that fails is rolled back. The version does not change.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_06_failing_step() {
    let cfg = config();
    let db = backend().fresh_name();
    cfg.empty_database(&db).await;
    let bad = Migration {
        version: 2,
        sql: "CREATE TABLE hune_sp06 (id int); SELECT 1 / 0; UPDATE hune_schema SET version = 2 WHERE id",
    };
    let r = migrate_store_with(&cfg.owner_options(&db), &[MIGRATIONS[0], bad]).await;
    match r {
        Err(MigrateError::Step { version: 2, ref message }) => assert!(message.contains("division by zero"), "{r:?}"),
        other => panic!("SP-06: {other:?}"),
    }
    let admin = cfg.admin(&db).await;
    let v: i32 = admin.client.query_one("SELECT version FROM hune_schema WHERE id", &[]).await.unwrap().get(0);
    assert_eq!(v, 1, "SP-06 version");
    let t: bool =
        admin.client.query_one("SELECT to_regclass('public.hune_sp06') IS NOT NULL", &[]).await.unwrap().get(0);
    assert!(!t, "SP-06 the table of the failed step must not exist");
    // The lock is free: the next run applies the good step 2 only.
    let ok = Migration {
        version: 2,
        sql: "CREATE TABLE hune_sp06 (id int); UPDATE hune_schema SET version = 2, updated_at = now() WHERE id",
    };
    let r = migrate_store_with(&cfg.owner_options(&db), &[MIGRATIONS[0], ok]).await.unwrap();
    assert_eq!((r.from, r.to, r.applied), (1, 2, vec![2]));
}

/// SP-07: direct inserts that break a `CHECK` are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_07_checks() {
    let cfg = config();
    let db = backend().new_database().await;
    let admin = cfg.admin(&db).await;
    let c = &admin.client;
    c.execute("INSERT INTO hune_folders VALUES ('f', 'F', now(), now())", &[]).await.unwrap();
    let good = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let dash = "INSERT INTO hune_dashboards (uid, folder_id, version, etag, format_version, title, document, \
                updated_at, updated_by) VALUES ($1, 'f', 1, $2, 1, 't', $3, now(), 'x')";
    let big = "a".repeat(4_194_304 + 1);
    let cases: Vec<(&str, String, String, String)> = vec![
        ("bad uid", "Bad_Uid".to_owned(), good.to_owned(), "{}".to_owned()),
        ("bad etag", "ok-uid".to_owned(), "xyz".to_owned(), "{}".to_owned()),
        ("document of 4 MiB plus 1 byte", "ok-uid".to_owned(), good.to_owned(), big),
    ];
    for (what, u, e, d) in cases {
        let r = c.execute(dash, &[&u, &e, &d]).await;
        let err = r.expect_err(what);
        assert_eq!(code_of(&err), "23514", "SP-07 {what}");
    }
    // A document of exactly 4 MiB is accepted.
    c.execute(dash, &[&"ok-uid", &good, &"a".repeat(4_194_304)]).await.unwrap();
    let ver = "INSERT INTO hune_versions (uid, version, kind, folder_id, document, etag, saved_at, saved_by, message) \
               VALUES ($1, $2, $3, 'f', $4, $5, now(), 'x', '')";
    // A delete with a document, a save without a document, a delete with an etag, a bad kind.
    let bad: Vec<(&str, &str, Option<&str>, Option<&str>)> = vec![
        ("delete with a document", "delete", Some("{}"), None),
        ("save without a document", "save", None, Some(good)),
        ("delete with an etag", "delete", None, Some(good)),
        ("bad kind", "nope", Some("{}"), Some(good)),
        ("bad etag", "save", Some("{}"), Some("not-hex")),
    ];
    for (what, kind, document, etag) in bad {
        let r = c.execute(ver, &[&"ok-uid", &1_i64, &kind, &document, &etag]).await;
        let err = r.expect_err(what);
        assert_eq!(code_of(&err), "23514", "SP-07 {what}");
    }
    // A message of 501 characters.
    let msg = "m".repeat(501);
    let r = c
        .execute(
            "INSERT INTO hune_versions (uid, version, kind, folder_id, document, etag, saved_at, saved_by, message) \
             VALUES ('ok-uid', 1, 'save', 'f', '{}', $1, now(), 'x', $2)",
            &[&good, &msg],
        )
        .await;
    assert_eq!(code_of(&r.unwrap_err()), "23514", "SP-07 message");
}

/// SP-08: a folder with a dashboard cannot be deleted. The foreign key gives `FolderNotEmpty`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_08_folder_not_empty() {
    let cfg = config();
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    one_dashboard(&store).await;
    let s = AnyStore::Postgres(store);
    assert_eq!(s.delete_folder(&fid("f")).await, Err(StoreError::FolderNotEmpty));
    let admin = cfg.admin(&db).await;
    let e = admin.client.execute("DELETE FROM hune_folders WHERE id = 'f'", &[]).await.unwrap_err();
    assert!(code_of(&e).starts_with("23"), "{e}");
}

/// SP-09: PostgreSQL is down at start, then available. The caller waits, then checks and loads.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_09_down_at_start() {
    let cfg = config();
    let db = backend().new_database().await;
    let port = pg_support::free_port();
    let mut opts = cfg.app_options(&db, 100);
    opts.port = port;
    opts.connect_timeout = Duration::from_secs(1);
    let waits = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let w = waits.clone();
    let opener = tokio::spawn(async move {
        PgStore::open_waiting(opts, move |e| {
            assert!(e.is_temporary());
            w.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(!opener.is_finished(), "SP-09: the opener must wait");
    assert!(waits.load(std::sync::atomic::Ordering::SeqCst) >= 2, "SP-09: not ready, tries again");
    let proxy = Proxy::start(&cfg.host, cfg.port, port).await;
    let store = tokio::time::timeout(Duration::from_secs(10), opener).await.expect("SP-09 start").unwrap().unwrap();
    let any = AnyStore::Postgres(store);
    one_dashboard_any(&any).await;
    let catalog = Catalog::load(&any).await.expect("SP-09: load the catalog");
    assert_eq!(catalog.stats().dashboards, 1);
    proxy.stop();
}

async fn one_dashboard_any(s: &AnyStore) {
    s.create_folder(&fid("f"), &title("Folder f")).await.unwrap();
    s.create(&fid("f"), &doc("d1", "First"), VersionKind::Create, &meta(1)).await.unwrap();
}

/// A store behind a proxy, with a dashboard.
async fn proxied(keep: u32) -> (AnyStore, PgStore, Proxy, String) {
    let cfg = config();
    let db = backend().new_database().await;
    let proxy = Proxy::start(&cfg.host, cfg.port, 0).await;
    let mut opts = cfg.app_options(&db, keep);
    opts.host = "127.0.0.1".to_owned();
    opts.port = proxy.port();
    let store = PgStore::open(opts).await.unwrap();
    one_dashboard(&store).await;
    (AnyStore::Postgres(store.clone()), store, proxy, db)
}

/// SP-10: the proxy closes the connection during a save. `Unavailable`; the connection leaves the
/// pool; the save with the same `save_id` gives one version.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_10_proxy_cuts_a_save() {
    let (s, pg, proxy, db) = proxied(100).await;
    let d1 = doc("d1", "First");
    let d2 = doc("d1", "Second");
    let admin = config().admin(&db).await;
    admin.client.batch_execute("BEGIN; SELECT 1 FROM hune_dashboards WHERE uid = 'd1' FOR UPDATE").await.unwrap();
    let size_before = pg.pool_status().0;
    let task = {
        let (s, d1, d2) = (s.clone(), d1.clone(), d2.clone());
        tokio::spawn(async move { s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    proxy.cut();
    let r = task.await.unwrap();
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "SP-10: {r:?}");
    assert!(pg.pool_status().0 < size_before, "SP-10: the connection must leave the pool");
    admin.client.batch_execute("ROLLBACK").await.unwrap();
    let r = s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.expect("SP-10 retry");
    assert!(matches!(r.outcome, Outcome::Written | Outcome::Replayed), "SP-10: {r:?}");
    assert_eq!(numbers(&s, "d1").await, vec![2, 1], "SP-10: one version for the save");
    proxy.stop();
}

/// The server confirms the commit, but the answer is lost (a fault after `COMMIT`). The same
/// save gives `replayed`, never a second version.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn lost_answer_after_commit_is_replayed() {
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    let d1 = one_dashboard(&store).await;
    let s = AnyStore::Postgres(store.clone());
    let d2 = doc("d1", "Second");
    store.faults().inject(hune_store::postgres::testing::PgFaultPoint::AfterCommit);
    let r = s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "{r:?}");
    let r = s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Replayed);
    assert_eq!(numbers(&s, "d1").await, vec![2, 1]);
}

/// The commit that fails before it reaches the server: the same save gives `written`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn failure_before_commit_is_written_on_retry() {
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    let d1 = one_dashboard(&store).await;
    let s = AnyStore::Postgres(store.clone());
    let d2 = doc("d1", "Second");
    store.faults().inject(hune_store::postgres::testing::PgFaultPoint::BeforeCommit);
    let r = s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "{r:?}");
    assert_eq!(numbers(&s, "d1").await, vec![1], "nothing is written");
    let r = s.save(&d2, d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Written);
    assert_eq!(numbers(&s, "d1").await, vec![2, 1]);
}

/// SP-11: `get` on a pooled connection that is broken is sent again on a new connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_11_read_retry() {
    let cfg = config();
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    one_dashboard(&store).await;
    let s = AnyStore::Postgres(store);
    for round in 0..5 {
        s.get(&uid("d1")).await.unwrap();
        let admin = cfg.admin(&db).await;
        let n = admin
            .client
            .execute(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                 WHERE datname = $1 AND application_name = 'hune' AND pid <> pg_backend_pid()",
                &[&db],
            )
            .await
            .unwrap();
        assert!(n >= 1, "SP-11 round {round}: no connection to break");
        let cur = s.get(&uid("d1")).await.expect("SP-11: the read must succeed on a new connection");
        assert_eq!(cur.version, 1);
    }
}

/// SP-12: a write on a node in recovery (`25006`): `Unavailable`; the connection leaves the pool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_12_read_only_session() {
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    let d1 = one_dashboard(&store).await;
    let s = AnyStore::Postgres(store.clone());
    let size_before = store.pool_status().0;
    assert!(size_before >= 1);
    store.faults().run_on_next_connection("SET default_transaction_read_only = on");
    let r = s.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await;
    match r {
        Err(StoreError::Unavailable(ref m)) => assert!(m.contains("25006"), "{m}"),
        other => panic!("SP-12: {other:?}"),
    }
    assert_eq!(store.pool_status().0, size_before - 1, "SP-12: the connection must leave the pool");
    // A new connection works.
    let r = s.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Written);
}

/// SP-13: a save while another session holds the row: `Unavailable` after `statement_timeout`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_13_statement_timeout() {
    let cfg = config();
    let db = backend().new_database().await;
    let mut opts = cfg.app_options(&db, 100);
    opts.statement_timeout = Duration::from_secs(1);
    let store = PgStore::open(opts).await.unwrap();
    let d1 = one_dashboard(&store).await;
    let s = AnyStore::Postgres(store.clone());
    let admin = cfg.admin(&db).await;
    admin.client.batch_execute("BEGIN; SELECT 1 FROM hune_dashboards WHERE uid = 'd1' FOR UPDATE").await.unwrap();
    let size_before = store.pool_status().0;
    let t = Instant::now();
    let r = s.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await;
    let took = t.elapsed();
    match r {
        Err(StoreError::Unavailable(ref m)) => assert!(m.contains("57014"), "{m}"),
        other => panic!("SP-13: {other:?}"),
    }
    assert!(took >= Duration::from_millis(900) && took < Duration::from_secs(8), "SP-13: took {took:?}");
    assert_eq!(store.pool_status().0, size_before, "SP-13: the connection stays in the pool");
    admin.client.batch_execute("ROLLBACK").await.unwrap();
    let r = s.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(3)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Written);
}

/// SP-14: two instances. A save on A is visible on B after at most `poll_interval` plus 1 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_14_two_instances() {
    let cfg = config();
    let db = backend().new_database().await;
    let a = AnyStore::Postgres(backend().open(&db, 100).await);
    let b = AnyStore::Postgres(PgStore::open(cfg.app_options(&db, 100)).await.unwrap());
    let catalog = std::sync::Arc::new(Catalog::load(&b).await.unwrap());
    let poller = {
        let (catalog, b) = (catalog.clone(), b.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(b.poll_interval()).await;
                catalog.poll(&b).await;
            }
        })
    };
    one_dashboard_any(&a).await;
    let t = Instant::now();
    let limit = b.poll_interval() + Duration::from_secs(1);
    loop {
        if catalog.get(&uid("d1")).is_some() {
            break;
        }
        assert!(t.elapsed() <= limit, "SP-14: not visible after {:?}", t.elapsed());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let d1 = doc("d1", "First");
    a.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.unwrap();
    let t = Instant::now();
    loop {
        if let Some(e) = catalog.get(&uid("d1"))
            && matches!(&*e, CatalogEntry::Valid { current, .. } if current.version == 2)
        {
            break;
        }
        assert!(t.elapsed() <= limit, "SP-14: the save is not visible after {:?}", t.elapsed());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    poller.abort();
}

/// SP-15: TLS with the right CA, with a wrong CA and with a wrong `server_name`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_15_tls() {
    let cfg = config();
    let db = backend().new_database().await;
    let tls = |ca: Vec<u8>, name: String| {
        let mut o = cfg.app_options(&db, 100);
        o.sslmode = PgSslMode::VerifyFull { ca_pem: ca, server_name: name };
        o
    };
    let store = PgStore::open(tls(ca_pem(), server_name())).await.expect("SP-15 right CA");
    one_dashboard(&store).await;
    let admin = cfg.admin(&db).await;
    let rows = admin
        .client
        .query(
            "SELECT s.ssl, s.version FROM pg_stat_ssl s JOIN pg_stat_activity a USING (pid) \
             WHERE a.application_name = 'hune' AND a.datname = $1",
            &[&db],
        )
        .await
        .unwrap();
    assert!(!rows.is_empty(), "SP-15: no connection of Hune");
    for r in &rows {
        assert!(r.get::<_, bool>(0), "SP-15: a connection without TLS");
        let v: String = r.get(1);
        assert!(v == "TLSv1.2" || v == "TLSv1.3", "{v}");
    }
    let e = PgStore::open(tls(wrong_ca_pem(), server_name())).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent, "SP-15 wrong CA: {e}");
    let e = PgStore::open(tls(ca_pem(), "other.example".to_owned())).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent, "SP-15 wrong server name: {e}");
    // The migration also uses TLS with the same rules.
    let mut o = cfg.owner_options(&db);
    o.sslmode = PgSslMode::VerifyFull { ca_pem: wrong_ca_pem(), server_name: server_name() };
    assert!(matches!(migrate_store(&o).await, Err(MigrateError::Connect(_))));
}

/// SP-16: the order of the uids is the byte order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_16_byte_order() {
    let db = backend().new_database().await;
    let s = AnyStore::Postgres(backend().open(&db, 100).await);
    s.create_folder(&fid("f"), &title("F")).await.unwrap();
    for (i, u) in ["ab", "a0", "a-b"].iter().enumerate() {
        s.create(&fid("f"), &doc(u, "T"), VersionKind::Create, &meta(i as u64 + 1)).await.unwrap();
    }
    let snap = s.load().await.unwrap();
    let order: Vec<_> = snap.entries.iter().map(|e| e.uid().to_string()).collect();
    assert_eq!(order, vec!["a-b", "a0", "ab"], "SP-16 load");
    let ex = s.export_begin().await.unwrap();
    let uids: Vec<_> = ex.uids().await.unwrap().iter().map(|u| u.to_string()).collect();
    assert_eq!(uids, vec!["a-b", "a0", "ab"], "SP-16 export");
    ex.end().await.unwrap();
}

/// SP-17: `serve` with the owner role, a member of it, a `NOINHERIT` member, a superuser: refused.
/// With the application role: it starts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_17_roles() {
    let cfg = config();
    let db = backend().new_database().await;
    let pid = std::process::id();
    let (member, noinherit) = (format!("vt{pid}_member"), format!("vt{pid}_noinherit"));
    let admin = cfg.admin("postgres").await;
    for (name, extra) in [(&member, "INHERIT"), (&noinherit, "NOINHERIT")] {
        admin.client.batch_execute(&format!("DROP ROLE IF EXISTS \"{name}\"")).await.unwrap();
        let sql = format!("CREATE ROLE \"{name}\" LOGIN {extra} PASSWORD 'memberpw' IN ROLE \"{}\"", cfg.owner_user);
        admin.client.batch_execute(&sql).await.unwrap();
    }
    let mut results = Vec::new();
    let opts_of = |user: &str, password: &str| {
        let mut o = cfg.app_options(&db, 100);
        o.user = user.to_owned();
        o.password = Some(password.to_owned());
        o
    };
    results.push(("owner", opts_of(&cfg.owner_user, &cfg.owner_password)));
    results.push(("member", opts_of(&member, "memberpw")));
    results.push(("noinherit member", opts_of(&noinherit, "memberpw")));
    results.push(("superuser", opts_of(&cfg.admin_user, &cfg.admin_password)));
    for (what, o) in results {
        let e = PgStore::open(o).await.expect_err(what);
        assert_eq!(e.kind, PgOpenErrorKind::RoleRefused, "SP-17 {what}: {e}");
        assert!(e.message.contains("is a superuser or owns the Hune tables"), "{e}");
        assert!(e.message.contains("(5.25, 12.13)"), "{e}");
    }
    PgStore::open(cfg.app_options(&db, 100)).await.expect("SP-17: the application role starts");
    // The other commands do not check the role: the owner role opens without `check_role`.
    let mut o = cfg.owner_options(&db);
    o.check_role = false;
    PgStore::open(o).await.expect("SP-17: no role check for a command that is not serve");
    for name in [&member, &noinherit] {
        admin.client.batch_execute(&format!("DROP ROLE \"{name}\"")).await.unwrap();
    }
}

/// SP-18: a wrong password, an absent database and a wrong CA stop the start (permanent). A node in
/// recovery (a read-only session) is temporary, and the caller starts after it becomes a primary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_18_start_failures() {
    let cfg = config();
    let db = backend().new_database().await;
    let mut o = cfg.app_options(&db, 100);
    o.password = Some("wrong password".to_owned());
    let e = PgStore::open(o).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent, "SP-18 wrong password: {e}");
    let e = PgStore::open(cfg.app_options("vt-no-such-database", 100)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent, "SP-18 absent database: {e}");
    let mut o = cfg.app_options(&db, 100);
    o.sslmode = PgSslMode::VerifyFull { ca_pem: wrong_ca_pem(), server_name: server_name() };
    let e = PgStore::open(o).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Permanent, "SP-18 wrong CA: {e}");
    // A session that is read-only is a node in recovery for `target_session_attrs=read-write`.
    let admin = cfg.admin("postgres").await;
    admin
        .client
        .batch_execute(&format!("ALTER DATABASE \"{db}\" SET default_transaction_read_only = on"))
        .await
        .unwrap();
    let e = PgStore::open(cfg.app_options(&db, 100)).await.unwrap_err();
    assert_eq!(e.kind, PgOpenErrorKind::Temporary, "SP-18 in recovery: {e}");
    let mut o = cfg.app_options(&db, 100);
    o.connect_timeout = Duration::from_secs(1);
    let opener = tokio::spawn(PgStore::open_waiting(o, |_| {}));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!opener.is_finished(), "SP-18: serve waits while the node is in recovery");
    admin.client.batch_execute(&format!("ALTER DATABASE \"{db}\" RESET default_transaction_read_only")).await.unwrap();
    let store = tokio::time::timeout(Duration::from_secs(10), opener).await.expect("SP-18 start").unwrap();
    store.expect("SP-18: serve starts when the node is a primary");
}

/// SP-19: a row whose `etag` differs from the SHA-256 of `document`: an invalid entry with one
/// error `not_permitted`, in `load`, in `poll` and in `get`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sp_19_etag_mismatch() {
    let cfg = config();
    let db = backend().new_database().await;
    let store = backend().open(&db, 100).await;
    one_dashboard(&store).await;
    let s = AnyStore::Postgres(store);
    let catalog = Catalog::load(&s).await.unwrap();
    assert!(matches!(&*catalog.get(&uid("d1")).unwrap(), CatalogEntry::Valid { .. }));
    let wrong = "f".repeat(64);
    let admin = cfg.admin(&db).await;
    admin.client.execute("UPDATE hune_dashboards SET etag = $1 WHERE uid = 'd1'", &[&wrong]).await.unwrap();
    let check = |errors: &[hune_store::ValidationError], what: &str| {
        assert_eq!(errors.len(), 1, "SP-19 {what}");
        assert_eq!(errors[0].code.as_str(), "not_permitted", "SP-19 {what}");
        assert!(errors[0].message.contains("column"), "SP-19 {what}: {}", errors[0].message);
    };
    let snap = s.load().await.unwrap();
    match &*snap.entries[0] {
        CatalogEntry::Invalid { errors, .. } => check(errors, "load"),
        other => panic!("SP-19 load: {other:?}"),
    }
    catalog.poll(&s).await.outcome.unwrap();
    match &*catalog.get(&uid("d1")).unwrap() {
        CatalogEntry::Invalid { errors, .. } => check(errors, "poll"),
        other => panic!("SP-19 poll: {other:?}"),
    }
    match s.get(&uid("d1")).await {
        Err(StoreError::Invalid(errors)) => check(&errors, "get"),
        other => panic!("SP-19 get: {other:?}"),
    }
    // A write on the row compares the column etag (5.27, item 7).
    let r = s.save(&doc("d1", "X"), &hune_store::Etag::of(b"x"), &fid("f"), VersionKind::Save, &meta(5)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "{r:?}");
}

/// The password never reaches an error text of the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn errors_do_not_hold_the_password() {
    let cfg = config();
    let db = backend().new_database().await;
    let mut o = cfg.app_options(&db, 100);
    o.password = Some("hunter2-secret".to_owned());
    let e = PgStore::open(o).await.unwrap_err();
    assert!(!e.message.contains("hunter2-secret"), "{e}");
}

/// A pool of one connection serves the writes one after the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn pool_of_one_serves_sequential_writes() {
    let cfg = config();
    let db = backend().new_database().await;
    let mut o = cfg.app_options(&db, 100);
    o.max_connections = 1;
    let store = PgStore::open(o).await.unwrap();
    let d1 = one_dashboard(&store).await;
    let s = AnyStore::Postgres(store);
    let r = s.save(&doc("d1", "Second"), d1.etag(), &fid("f"), VersionKind::Save, &meta(2)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Written);
    assert_eq!(numbers(&s, "d1").await, vec![2, 1]);
}
