// SPDX-License-Identifier: MIT OR Apache-2.0
//! The PostgreSQL side of the conformance suite (5.33 to 5.36): the factory of empty stores,
//! the hooks and the helpers of the tests of 5.36.
//!
//! The caller gives the connection data of a server in a [`PgTestConfig`]. The suite never reads
//! the environment. Each store gets its own database, copied from one template database that
//! holds the schema (migrated by the owner role) and the grants of 5.25.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::OnceCell;
use tokio_postgres::NoTls;
use tokio_postgres::types::ToSql;

use super::{Backend, Fixture, Hooks};
use crate::postgres::testing::{PgFaultPoint, PgFaults};
use crate::postgres::{MigrateReport, PgOptions, PgSslMode, PgStore, migrate_store};
use crate::store::AnyStore;
use crate::types::{Etag, FolderId, Uid, VersionMeta};

/// The connection data of a PostgreSQL server for the tests.
#[derive(Debug, Clone)]
pub struct PgTestConfig {
    /// The host.
    pub host: String,
    /// The port.
    pub port: u16,
    /// A superuser, for the hooks and for the databases of the tests.
    pub admin_user: String,
    /// The password of the superuser.
    pub admin_password: String,
    /// The owner role of 5.25 (`migrate-store`).
    pub owner_user: String,
    /// The password of the owner role.
    pub owner_password: String,
    /// The application role of 5.25.
    pub app_user: String,
    /// The password of the application role.
    pub app_password: String,
}

/// One admin session. The connection task stops on drop.
pub struct Admin {
    /// The client.
    pub client: tokio_postgres::Client,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Admin {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl PgTestConfig {
    /// Opens a superuser session on a database.
    pub async fn admin(&self, database: &str) -> Admin {
        let mut c = tokio_postgres::Config::new();
        c.host(&self.host).port(self.port).user(&self.admin_user).password(&self.admin_password).dbname(database);
        let (client, conn) = c.connect(NoTls).await.expect("admin connection");
        Admin { client, task: tokio::spawn(async move { drop(conn.await) }) }
    }

    fn options(&self, database: &str, user: &str, password: &str, history_keep: u32) -> PgOptions {
        PgOptions {
            host: self.host.clone(),
            port: self.port,
            database: database.to_owned(),
            user: user.to_owned(),
            password: Some(password.to_owned()),
            sslmode: PgSslMode::Disable,
            max_connections: 16,
            connect_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(10),
            poll_interval: Duration::from_secs(1),
            history_keep,
            check_role: false,
        }
    }

    /// The options of the application role on a database. `check_role` is true, as for `serve`.
    pub fn app_options(&self, database: &str, history_keep: u32) -> PgOptions {
        PgOptions { check_role: true, ..self.options(database, &self.app_user, &self.app_password, history_keep) }
    }

    /// The options of the owner role on a database (`migrate.hcl`).
    pub fn owner_options(&self, database: &str) -> PgOptions {
        self.options(database, &self.owner_user, &self.owner_password, 100)
    }

    /// Makes an empty database with the grants of 5.25 for the two roles and the default
    /// privileges of the owner role. The database has no Hune table.
    pub async fn empty_database(&self, name: &str) {
        let admin = self.admin("postgres").await;
        let q = format!("CREATE DATABASE \"{name}\"");
        admin.client.batch_execute(&q).await.expect("create database");
        drop(admin);
        self.grant(name).await;
    }

    /// Makes an empty database with the encoding `SQL_ASCII` (SP-03).
    pub async fn ascii_database(&self, name: &str) {
        let admin = self.admin("postgres").await;
        let q =
            format!("CREATE DATABASE \"{name}\" TEMPLATE template0 ENCODING 'SQL_ASCII' LC_COLLATE 'C' LC_CTYPE 'C'");
        admin.client.batch_execute(&q).await.expect("create database");
        drop(admin);
        self.grant(name).await;
    }

    async fn grant(&self, name: &str) {
        let a = self.admin(name).await;
        let (o, p) = (&self.owner_user, &self.app_user);
        let sql = format!(
            "GRANT CONNECT ON DATABASE \"{name}\" TO \"{o}\"; GRANT CREATE, USAGE ON SCHEMA public TO \"{o}\"; \
             GRANT CONNECT ON DATABASE \"{name}\" TO \"{p}\"; GRANT USAGE ON SCHEMA public TO \"{p}\"; \
             ALTER DEFAULT PRIVILEGES FOR ROLE \"{o}\" IN SCHEMA public \
             GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO \"{p}\""
        );
        a.client.batch_execute(&sql).await.expect("grants of 5.25");
    }

    /// Makes a database, with the grants, and applies `migrate-store` with the owner role.
    pub async fn migrated_database(&self, name: &str) -> MigrateReport {
        self.empty_database(name).await;
        migrate_store(&self.owner_options(name)).await.expect("migrate-store")
    }

    /// Makes a database as a copy of a template. It retries while the template is in use.
    async fn copy_database(&self, name: &str, template: &str) {
        let admin = self.admin("postgres").await;
        let q = format!("CREATE DATABASE \"{name}\" TEMPLATE \"{template}\"");
        for attempt in 0..100 {
            match admin.client.batch_execute(&q).await {
                Ok(()) => return,
                Err(e) if e.code().map(|c| c.code()) == Some("55006") && attempt < 99 => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => panic!("create database {name} from {template}: {e}"),
            }
        }
    }
}

/// The factory of PostgreSQL stores for the conformance suite.
pub struct PgBackend {
    cfg: PgTestConfig,
    prefix: String,
    counter: AtomicU64,
    template: OnceCell<String>,
}

impl PgBackend {
    /// Makes a factory. The names of its databases start with `prefix`.
    pub fn new(cfg: PgTestConfig, prefix: &str) -> PgBackend {
        PgBackend { cfg, prefix: prefix.to_owned(), counter: AtomicU64::new(0), template: OnceCell::new() }
    }

    /// The connection data.
    pub fn config(&self) -> &PgTestConfig {
        &self.cfg
    }

    /// The name of a new database that no one used.
    pub fn fresh_name(&self) -> String {
        format!("{}_{}", self.prefix, self.counter.fetch_add(1, Ordering::SeqCst))
    }

    async fn template(&self) -> &str {
        self.template
            .get_or_init(|| async {
                let name = format!("{}_tpl", self.prefix);
                self.cfg.migrated_database(&name).await;
                name
            })
            .await
    }

    /// Makes a database with the schema at the current version and returns its name.
    pub async fn new_database(&self) -> String {
        let tpl = self.template().await.to_owned();
        let name = self.fresh_name();
        self.cfg.copy_database(&name, &tpl).await;
        name
    }

    /// Opens a store on a database, with the options of the application role.
    pub async fn open(&self, database: &str, history_keep: u32) -> PgStore {
        PgStore::open(self.cfg.app_options(database, history_keep)).await.expect("open the PostgreSQL store")
    }
}

impl Backend for PgBackend {
    async fn new_store(&self, history_keep: u32) -> Fixture {
        let name = self.new_database().await;
        let store = self.open(&name, history_keep).await;
        let hooks = PgHooks { cfg: self.cfg.clone(), database: name, history_keep, faults: store.faults() };
        Fixture { store: AnyStore::Postgres(store), hooks: Hooks::Postgres(hooks) }
    }
}

/// The hooks of a PostgreSQL store (5.33, item 3). They change the database outside the trait,
/// with a superuser session.
pub struct PgHooks {
    cfg: PgTestConfig,
    database: String,
    history_keep: u32,
    faults: PgFaults,
}

impl PgHooks {
    /// The name of the database.
    pub fn database(&self) -> &str {
        &self.database
    }

    /// The connection data.
    pub fn config(&self) -> &PgTestConfig {
        &self.cfg
    }

    /// The faults of the store.
    pub fn faults(&self) -> &PgFaults {
        &self.faults
    }

    /// Opens a superuser session on the database of the store.
    pub async fn admin(&self) -> Admin {
        self.cfg.admin(&self.database).await
    }

    /// Replaces the current document of a uid with outside bytes, or makes the row. A tool
    /// that changes a row MUST write `etag` and `document` together (5.24, item 8): the hook does.
    pub async fn outside_write(&self, folder: &FolderId, uid: &Uid, bytes: &[u8]) {
        let text = std::str::from_utf8(bytes).expect("outside bytes are UTF-8");
        let etag = Etag::of(bytes).to_hex();
        let a = self.admin().await;
        let n = a
            .client
            .execute(
                "UPDATE hune_dashboards SET folder_id = $2, document = $3, etag = $4, updated_at = now() \
                 WHERE uid = $1",
                &[&uid.as_str(), &folder.as_str(), &text, &etag],
            )
            .await
            .expect("outside update");
        if n == 0 {
            a.client
                .execute(
                    "INSERT INTO hune_dashboards (uid, folder_id, version, etag, format_version, title, document, \
                     updated_at, updated_by) VALUES ($1, $2, 1, $4, 3, 'Outside', $3, now(), 'external')",
                    &[&uid.as_str(), &folder.as_str(), &text, &etag],
                )
                .await
                .expect("outside insert");
        }
    }

    /// Changes the column `etag` of a row only, so that it differs from the document (SP-19).
    pub async fn corrupt_etag(&self, uid: &Uid, etag: &str) {
        let a = self.admin().await;
        a.client
            .execute("UPDATE hune_dashboards SET etag = $2 WHERE uid = $1", &[&uid.as_str(), &etag])
            .await
            .expect("corrupt the etag");
    }

    /// Makes the next write fail with `Unavailable` after its transaction is committed: the
    /// answer is lost, and the version is durable.
    pub async fn break_next_write(&self) {
        self.faults.inject(PgFaultPoint::AfterCommit);
    }

    /// Writes one history version outside the trait.
    pub async fn inject_version(&self, meta: &VersionMeta, bytes: Option<&[u8]>) {
        let text = bytes.map(|b| std::str::from_utf8(b).expect("version bytes are UTF-8"));
        let etag = meta.etag.map(|e| e.to_hex());
        let version = i64::try_from(meta.version).expect("version");
        let at = crate::postgres::testing::system_time(meta.saved_at);
        let sid = meta.save_id.map(|s| s.to_string());
        let a = self.admin().await;
        let p: [&(dyn ToSql + Sync); 10] = [
            &meta.uid.as_str(),
            &version,
            &meta.kind.as_str(),
            &meta.folder.as_str(),
            &text,
            &etag,
            &at,
            &meta.saved_by.as_str(),
            &meta.message.as_str(),
            &sid,
        ];
        a.client
            .execute(
                "INSERT INTO hune_versions (uid, version, kind, folder_id, document, etag, saved_at, saved_by, \
                 message, save_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::text::uuid)",
                &p,
            )
            .await
            .expect("inject version");
    }

    /// Fills the store with `n` folders named `fill-<number>`.
    pub async fn fill_folders(&self, n: usize) {
        let n = i32::try_from(n).expect("n");
        let a = self.admin().await;
        a.client
            .execute(
                "INSERT INTO hune_folders (id, title, created_at, updated_at) \
                 SELECT 'fill-' || i, 'Fill', now(), now() FROM generate_series(0, $1::int - 1) AS i",
                &[&n],
            )
            .await
            .expect("fill folders");
    }

    /// Fills a folder with the dashboards of the list.
    pub async fn fill_dashboards(&self, folder: &FolderId, docs: impl Iterator<Item = (Uid, Vec<u8>)>) {
        let a = self.admin().await;
        let mut uids = Vec::new();
        let mut etags = Vec::new();
        let mut texts = Vec::new();
        let flush = |uids: &mut Vec<String>, etags: &mut Vec<String>, texts: &mut Vec<String>| {
            let (u, e, t) = (std::mem::take(uids), std::mem::take(etags), std::mem::take(texts));
            let client = &a.client;
            let folder = folder.as_str().to_owned();
            async move {
                client
                    .execute(
                        "INSERT INTO hune_dashboards (uid, folder_id, version, etag, format_version, title, \
                         document, updated_at, updated_by) \
                         SELECT t.u, $1, 1, t.e, 3, 'Fill', t.d, now(), 'external' \
                         FROM unnest($2::text[], $3::text[], $4::text[]) AS t(u, e, d)",
                        &[&folder, &u, &e, &t],
                    )
                    .await
                    .expect("fill dashboards");
            }
        };
        for (uid, bytes) in docs {
            etags.push(Etag::of(&bytes).to_hex());
            uids.push(uid.as_str().to_owned());
            texts.push(String::from_utf8(bytes).expect("document bytes are UTF-8"));
            if uids.len() >= 250 {
                flush(&mut uids, &mut etags, &mut texts).await;
            }
        }
        if !uids.is_empty() {
            flush(&mut uids, &mut etags, &mut texts).await;
        }
    }

    /// Opens a second store on the same database. It sees the changes of the first store after a poll.
    pub async fn second_store(&self) -> AnyStore {
        let store = PgStore::open(self.cfg.app_options(&self.database, self.history_keep))
            .await
            .expect("open the second store");
        AnyStore::Postgres(store)
    }
}
