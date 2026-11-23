// SPDX-License-Identifier: MIT OR Apache-2.0
//! The PostgreSQL backend (5.23 to 5.32).
//!
//! The backend sends the statements of 5.26 in the transactions of 5.27. It never migrates the
//! schema at start (D20): only [`migrate_store`] changes the schema. It never sends a write
//! again after an error (5.5, rule 11).

macro_rules! with_read {
    ($self:expr, |$c:ident| $body:expr) => {{
        let mut attempt = 0u8;
        loop {
            let mut obj = $self.conn().await?;
            let res = {
                let $c = &mut obj;
                $body.await
            };
            match $self.settle_read(obj, res, attempt == 0) {
                $crate::postgres::Settled::Done(r) => break r,
                $crate::postgres::Settled::Retry => attempt += 1,
            }
        }
    }};
}

mod conn;
mod migrate;
mod read;
mod rows;
mod sql;
#[cfg(any(test, feature = "conformance"))]
pub mod testing;
mod transfer;
mod write;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Object, Pool};
use tokio_postgres::Row;
use tokio_postgres::types::ToSql;

pub use self::conn::{PgOptions, PgSslMode};
pub use self::migrate::{
    MIGRATIONS, MigrateError, MigrateReport, Migration, SCHEMA_VERSION, migrate_store, migrate_store_with,
};
pub use self::transfer::{PgExporter, PgImporter};
use crate::store::{Delta, EntryProbe, Exporter, FolderMark, FolderState, Importer, Marks, Problem, Snapshot};
use crate::types::{
    CanonicalDocument, Current, Etag, FolderId, FolderTitle, Saved, StoreError, Uid, ValidVersion, Version,
    VersionKind, VersionMeta, WriteMeta,
};

/// The cause of a start error of the PostgreSQL backend (5.29).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgOpenErrorKind {
    /// A network failure, a timeout, SQLSTATE `57P03`, or a node in recovery. `serve` waits
    /// and tries again.
    Temporary,
    /// A wrong role or password (class `28`), no such database (`3D000`), a failed certificate
    /// check, or a configuration that cannot work. The process stops.
    Permanent,
    /// The database encoding is not `UTF8`.
    Encoding,
    /// The database has no Hune schema.
    NoSchema,
    /// The schema version is older than the binary.
    SchemaOlder,
    /// The schema version is newer than the binary.
    SchemaNewer,
    /// The role of `serve` is a superuser or owns the Hune tables (D76).
    RoleRefused,
}

/// A start error of the PostgreSQL backend. The message is the text of 5.29.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgOpenError {
    /// The cause.
    pub kind: PgOpenErrorKind,
    /// The message for the operator. It can hold the text `<file>`: see [`PgOpenError::with_config_file`].
    pub message: String,
}

impl PgOpenError {
    /// Tells if the caller can wait and try again (5.29, step 1).
    pub fn is_temporary(&self) -> bool {
        self.kind == PgOpenErrorKind::Temporary
    }

    /// Replaces the text `<file>` of the message with the path of the configuration file.
    pub fn with_config_file(mut self, path: &str) -> Self {
        self.message = self.message.replace("<file>", path);
        self
    }
}

impl fmt::Display for PgOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PgOpenError {}

impl From<PgOpenError> for StoreError {
    fn from(e: PgOpenError) -> Self {
        StoreError::Unavailable(e.message)
    }
}

/// The failure of a step in a transaction: a store error, or a database error to map.
pub(crate) enum Fail {
    Store(StoreError),
    Db(tokio_postgres::Error),
}

impl From<StoreError> for Fail {
    fn from(e: StoreError) -> Self {
        Fail::Store(e)
    }
}

impl From<tokio_postgres::Error> for Fail {
    fn from(e: tokio_postgres::Error) -> Self {
        Fail::Db(e)
    }
}

pub(crate) type R<T> = Result<T, Fail>;

/// The result of a read: an answer, or a request to send the read again on a new connection.
pub(crate) enum Settled<T> {
    Done(Result<T, StoreError>),
    Retry,
}

type Params<'a> = &'a [&'a (dyn ToSql + Sync)];

/// Runs a statement of 5.26 inside a transaction and gives the rows.
pub(crate) async fn tx_q(
    tx: &deadpool_postgres::Transaction<'_>,
    sql: &str,
    p: Params<'_>,
) -> Result<Vec<Row>, tokio_postgres::Error> {
    let st = tx.prepare_cached(sql).await?;
    tx.query(&st, p).await
}

/// Runs a statement inside a transaction and gives the first row, if any.
pub(crate) async fn tx_opt(
    tx: &deadpool_postgres::Transaction<'_>,
    sql: &str,
    p: Params<'_>,
) -> Result<Option<Row>, tokio_postgres::Error> {
    let st = tx.prepare_cached(sql).await?;
    tx.query_opt(&st, p).await
}

/// Runs a statement inside a transaction and gives the number of rows that it changed.
pub(crate) async fn tx_x(
    tx: &deadpool_postgres::Transaction<'_>,
    sql: &str,
    p: Params<'_>,
) -> Result<u64, tokio_postgres::Error> {
    let st = tx.prepare_cached(sql).await?;
    tx.execute(&st, p).await
}

/// Runs a statement on a connection and gives the rows.
pub(crate) async fn cl_q(c: &Object, sql: &str, p: Params<'_>) -> Result<Vec<Row>, tokio_postgres::Error> {
    let st = c.prepare_cached(sql).await?;
    c.query(&st, p).await
}

/// Runs a statement on a connection and gives the first row, if any.
pub(crate) async fn cl_opt(c: &Object, sql: &str, p: Params<'_>) -> Result<Option<Row>, tokio_postgres::Error> {
    let st = c.prepare_cached(sql).await?;
    c.query_opt(&st, p).await
}

/// The permits of the checks of stored documents in `spawn_blocking` (3.7). One backend runs in a process, so these
/// permits take the place of the 8 permits of the file work of the volume backend.
const CHECK_PERMITS: usize = 8;

struct Inner {
    pool: Pool,
    opts: PgOptions,
    /// The semaphore of the checks of stored documents (`CHECK_PERMITS`).
    checks: Arc<tokio::sync::Semaphore>,
    /// The number of versions that this store validated for a restore (see `PgStore::restore_checks`).
    restore_checks: std::sync::atomic::AtomicU64,
    #[cfg(any(test, feature = "conformance"))]
    faults: testing::PgFaults,
}

/// The PostgreSQL backend. A clone shares the pool.
#[derive(Clone)]
pub struct PgStore {
    inner: Arc<Inner>,
}

impl fmt::Debug for PgStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgStore").field("opts", &self.inner.opts).finish()
    }
}

impl PgStore {
    /// Opens the backend with the start checks of 5.29. The checks cover the connection, the
    /// encoding, the schema table and the schema version. With `check_role` they also cover the
    /// rights of the role. It never changes the schema (D20).
    pub async fn open(opts: PgOptions) -> Result<PgStore, PgOpenError> {
        Self::open_expecting(opts, SCHEMA_VERSION).await
    }

    /// Like [`PgStore::open`] with another schema version for the binary. For the tests of 5.36 (SP-02).
    #[doc(hidden)]
    pub async fn open_expecting(opts: PgOptions, expected: i32) -> Result<PgStore, PgOpenError> {
        let tls = conn::build_tls(&opts.sslmode)?;
        let pool = conn::build_pool(&opts, &tls)?;
        let store = PgStore {
            inner: Arc::new(Inner {
                pool,
                opts,
                checks: Arc::new(tokio::sync::Semaphore::new(CHECK_PERMITS)),
                restore_checks: std::sync::atomic::AtomicU64::new(0),
                #[cfg(any(test, feature = "conformance"))]
                faults: testing::PgFaults::default(),
            }),
        };
        store.check_start(expected).await?;
        Ok(store)
    }

    /// Opens the backend and waits while the failure is temporary (5.29, step 1). It calls
    /// `on_wait` with each temporary error, then sleeps `connect_timeout`. A permanent error, or
    /// a failed check, returns at once.
    pub async fn open_waiting(
        opts: PgOptions,
        mut on_wait: impl FnMut(&PgOpenError) + Send,
    ) -> Result<PgStore, PgOpenError> {
        let pause = opts.connect_timeout;
        loop {
            match Self::open(opts.clone()).await {
                Ok(s) => return Ok(s),
                Err(e) if e.is_temporary() => {
                    on_wait(&e);
                    tokio::time::sleep(pause).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// The options of the store.
    pub fn options(&self) -> &PgOptions {
        &self.inner.opts
    }

    /// Runs the check of stored documents in `spawn_blocking`, behind the semaphore of `CHECK_PERMITS` (3.7). A
    /// check validates whole documents (4.36), so it never runs on an async worker. The permit moves into the task,
    /// so it stays taken until the check ends.
    pub(crate) async fn off_thread<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, StoreError> {
        let permit = self
            .inner
            .checks
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| StoreError::Unavailable("check semaphore closed".to_owned()))?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            f()
        })
        .await
        .map_err(|e| StoreError::Unavailable(format!("check task failed: {e}")))
    }

    /// The period of the poll of the catalog.
    pub fn poll_interval(&self) -> Duration {
        self.inner.opts.poll_interval
    }

    /// The number of versions that this store validated for a restore since it opened. The tests read it: the
    /// restore of the API validates its version one time, before the store (6.18).
    #[doc(hidden)]
    pub fn restore_checks(&self) -> u64 {
        self.inner.restore_checks.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Counts one validation of a version for a restore.
    pub(crate) fn note_restore_check(&self) {
        self.inner.restore_checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// The backend gives no event for the log: the poll of PostgreSQL writes no `dashboard.changed` (5.30).
    pub fn drain_events(&self) -> Vec<crate::store::StoreEvent> {
        Vec::new()
    }

    /// The start checks of 5.29, steps 2 to 6, on one pooled connection.
    async fn check_start(&self, expected: i32) -> Result<(), PgOpenError> {
        let o = &self.inner.opts;
        let obj = self.inner.pool.get().await.map_err(|e| PgOpenError {
            kind: conn::classify_pool(&e),
            message: format!("storage.postgres: cannot connect to {}:{}: {}", o.host, o.port, conn::describe_pool(&e)),
        })?;
        let fail = |e: tokio_postgres::Error| PgOpenError {
            kind: if conn::is_broken(&e) { PgOpenErrorKind::Temporary } else { PgOpenErrorKind::Permanent },
            message: format!("storage.postgres: start check failed: {}", conn::describe(&e)),
        };
        let encoding: String =
            cl_opt(&obj, sql::P1, &[]).await.map_err(fail)?.and_then(|r| r.try_get(0).ok()).unwrap_or_default();
        if !encoding.eq_ignore_ascii_case("UTF8") {
            return Err(PgOpenError {
                kind: PgOpenErrorKind::Encoding,
                message: format!("storage.postgres: database encoding is {encoding}; Hune needs UTF8"),
            });
        }
        let has_table: bool =
            cl_opt(&obj, sql::P2, &[]).await.map_err(fail)?.and_then(|r| r.try_get(0).ok()).unwrap_or(false);
        if !has_table {
            return Err(PgOpenError {
                kind: PgOpenErrorKind::NoSchema,
                message: "storage.postgres: the database has no Hune schema; run: hune migrate-store -c <file>"
                    .to_owned(),
            });
        }
        // A role without the right to read `hune_schema` can be a member of the owner role
        // without `INHERIT` (SP-17): `serve` then gives the message of step 6.
        let found: i32 = match cl_opt(&obj, sql::P3, &[]).await {
            Ok(r) => r.and_then(|r| r.try_get(0).ok()).unwrap_or(0),
            Err(e) => {
                if o.check_role && e.code() == Some(&tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE) {
                    self.check_role_rights(&obj).await?;
                }
                return Err(fail(e));
            }
        };
        if found < expected {
            return Err(PgOpenError {
                kind: PgOpenErrorKind::SchemaOlder,
                message: format!(
                    "storage.postgres: database schema version {found} is older than this binary ({expected}); \
                     run: hune migrate-store -c <file>"
                ),
            });
        }
        if found > expected {
            return Err(PgOpenError {
                kind: PgOpenErrorKind::SchemaNewer,
                message: format!(
                    "storage.postgres: database schema version {found} is newer than this binary ({expected}); \
                     use a binary of schema version {found}"
                ),
            });
        }
        if o.check_role {
            self.check_role_rights(&obj).await?;
        }
        Ok(())
    }

    /// Step 6 of 5.29: the role is not a superuser and owns no Hune table (D76, P30).
    async fn check_role_rights(&self, obj: &Object) -> Result<(), PgOpenError> {
        let o = &self.inner.opts;
        let fail = |e: tokio_postgres::Error| PgOpenError {
            kind: if conn::is_broken(&e) { PgOpenErrorKind::Temporary } else { PgOpenErrorKind::Permanent },
            message: format!("storage.postgres: start check failed: {}", conn::describe(&e)),
        };
        let row = cl_opt(obj, sql::P30, &[]).await.map_err(fail)?;
        let (superuser, owner) = match row {
            Some(r) => (r.try_get::<_, bool>(0).unwrap_or(true), r.try_get::<_, bool>(1).unwrap_or(true)),
            None => (true, true),
        };
        if superuser || owner {
            return Err(PgOpenError {
                kind: PgOpenErrorKind::RoleRefused,
                message: format!(
                    "storage.postgres: the role {} is a superuser or owns the Hune tables; serve needs a role \
                     with only SELECT, INSERT, UPDATE and DELETE (5.25, 12.13)",
                    o.user
                ),
            });
        }
        Ok(())
    }

    /// Takes a connection from the pool. A failure gives `Unavailable`.
    pub(crate) async fn conn(&self) -> Result<Object, StoreError> {
        let obj = self
            .inner
            .pool
            .get()
            .await
            .map_err(|e| StoreError::Unavailable(format!("no connection: {}", conn::describe_pool(&e))))?;
        #[cfg(any(test, feature = "conformance"))]
        if let Some(sql) = self.inner.faults.take_session_sql() {
            obj.batch_execute(&sql)
                .await
                .map_err(|e| StoreError::Unavailable(format!("test session statement: {}", conn::describe(&e))))?;
        }
        Ok(obj)
    }

    /// Ends the use of a connection after a write. A connection error removes the connection
    /// from the pool (5.23). The answer is `Unavailable`, and the write is never sent again.
    pub(crate) fn settle<T>(&self, obj: Object, res: R<T>) -> Result<T, StoreError> {
        match res {
            Ok(v) => Ok(v),
            Err(Fail::Store(e)) => Err(e),
            Err(Fail::Db(e)) => {
                let text = conn::describe(&e);
                if conn::is_broken(&e) || obj.is_closed() {
                    drop(Object::take(obj));
                }
                Err(StoreError::Unavailable(text))
            }
        }
    }

    /// Ends the use of a connection after a read. A connection error gives one new try (5.23).
    pub(crate) fn settle_read<T>(&self, obj: Object, res: R<T>, can_retry: bool) -> Settled<T> {
        match res {
            Ok(v) => Settled::Done(Ok(v)),
            Err(Fail::Store(e)) => Settled::Done(Err(e)),
            Err(Fail::Db(e)) => {
                let broken = conn::is_broken(&e);
                let text = conn::describe(&e);
                if broken || obj.is_closed() {
                    drop(Object::take(obj));
                }
                if broken && can_retry { Settled::Retry } else { Settled::Done(Err(StoreError::Unavailable(text))) }
            }
        }
    }

    /// Runs the commit of a transaction. A fault of the tests can stop it before or after.
    pub(crate) async fn commit(&self, tx: deadpool_postgres::Transaction<'_>) -> R<()> {
        #[cfg(any(test, feature = "conformance"))]
        if self.inner.faults.take(testing::PgFaultPoint::BeforeCommit) {
            return Err(Fail::Store(StoreError::Unavailable("injected fault before the commit".to_owned())));
        }
        tx.commit().await?;
        #[cfg(any(test, feature = "conformance"))]
        if self.inner.faults.take(testing::PgFaultPoint::AfterCommit) {
            return Err(Fail::Store(StoreError::Unavailable("injected fault after the commit".to_owned())));
        }
        Ok(())
    }

    pub(crate) fn history_keep(&self) -> i64 {
        i64::from(self.inner.opts.history_keep)
    }

    // The methods of the trait `Store`.

    pub(crate) async fn load(&self) -> Result<Snapshot, StoreError> {
        self.load_impl().await
    }

    pub(crate) async fn poll(&self, marks: &Marks) -> Result<Delta, StoreError> {
        self.poll_impl(marks).await
    }

    pub(crate) async fn get(&self, uid: &Uid) -> Result<Current, StoreError> {
        self.get_impl(uid).await
    }

    pub(crate) async fn raw_current(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        self.raw_current_impl(uid, folder).await
    }

    pub(crate) async fn create(
        &self,
        folder: &FolderId,
        doc: &CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.create_impl(folder, doc, kind, meta).await
    }

    pub(crate) async fn save(
        &self,
        doc: &CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.save_impl(doc, if_match, expected_folder, kind, meta).await
    }

    pub(crate) async fn move_to(
        &self,
        uid: &Uid,
        folder: &FolderId,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.move_impl(uid, folder, if_match, expected_folder, meta).await
    }

    pub(crate) async fn delete(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.delete_impl(uid, if_match, expected_folder, meta).await
    }

    pub(crate) async fn restore(
        &self,
        uid: &Uid,
        version: u64,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        // The read and the validation come before the transaction of the write, behind the permits of the checks.
        let read = self.version(uid, version).await?;
        let owned = uid.clone();
        self.note_restore_check();
        let checked = self.off_thread(move || ValidVersion::check(&owned, &read)).await??;
        self.restore_valid(&checked, if_match, expected_folder, meta).await
    }

    pub(crate) async fn restore_valid(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.restore_impl(version, if_match, expected_folder, meta).await
    }

    pub(crate) async fn history(
        &self,
        uid: &Uid,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<VersionMeta>, StoreError> {
        self.history_impl(uid, before, limit).await
    }

    pub(crate) async fn version(&self, uid: &Uid, version: u64) -> Result<Version, StoreError> {
        self.version_impl(uid, version).await
    }

    pub(crate) async fn create_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        self.create_folder_impl(id, title).await
    }

    pub(crate) async fn rename_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        self.rename_folder_impl(id, title).await
    }

    pub(crate) async fn delete_folder(&self, id: &FolderId) -> Result<(), StoreError> {
        self.delete_folder_impl(id).await
    }

    pub(crate) async fn export_begin(&self) -> Result<Exporter, StoreError> {
        Ok(Exporter::Postgres(Box::new(PgExporter::begin(self).await?)))
    }

    pub(crate) async fn import_begin(&self) -> Result<Importer, StoreError> {
        Ok(Importer::Postgres(Box::new(PgImporter::begin(self).await?)))
    }

    pub(crate) async fn entry_probe(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        self.entry_probe_impl(uid).await
    }

    pub(crate) async fn folder_probe(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        self.folder_probe_impl().await
    }

    pub(crate) async fn check_consistency(&self) -> Result<Vec<Problem>, StoreError> {
        self.check_impl().await
    }
}
