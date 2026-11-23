//! Export and import of the PostgreSQL backend (5.27, 5.37 to 5.39).
//!
//! The exporter reads in one transaction at `REPEATABLE READ`. The importer writes in one
//! transaction. Each one holds one connection. A connection that goes back to the pool in a
//! transaction would be wrong, so a drop without `end` or `commit` closes the connection.

use std::fmt;
use std::time::SystemTime;

use deadpool_postgres::Object;
use hune_core::document::validate;
use hune_core::error::{ErrorCode, ValidationError};
use tokio_postgres::types::ToSql;

use super::conn::{describe, is_broken};
use super::rows::{DashRow, classify, corrupt, now, parse_uid, to_system, version_meta};
use super::{PgStore, cl_opt, cl_q, sql};
use crate::store::{CurrentRecord, DashboardRecord, ImportReport};
use crate::types::{Etag, FolderRecord, StoreError, Uid, Version, VersionKind};

fn db(e: tokio_postgres::Error) -> StoreError {
    StoreError::Unavailable(describe(&e))
}

fn invalid_one(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(vec![ValidationError::new("", ErrorCode::NotPermitted, message)])
}

/// Closes a connection that is in a transaction: it leaves the pool and the server rolls back.
fn discard(obj: Object) {
    drop(Object::take(obj));
}

/// The exporter of the PostgreSQL backend (5.3): a consistent copy at one instant.
pub struct PgExporter {
    conn: Option<Object>,
    folders: Vec<FolderRecord>,
}

impl fmt::Debug for PgExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgExporter").field("folders", &self.folders.len()).finish()
    }
}

impl Drop for PgExporter {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            discard(c);
        }
    }
}

impl PgExporter {
    pub(crate) async fn begin(store: &PgStore) -> Result<Self, StoreError> {
        let obj = store.conn().await?;
        let mut me = PgExporter { conn: Some(obj), folders: Vec::new() };
        // The first statement fixes the snapshot, so P4 runs at once.
        let rows = {
            let c = me.conn()?;
            c.batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY").await.map_err(db)?;
            cl_q(c, sql::P4, &[]).await.map_err(db)?
        };
        for (state, _) in super::read::folders_of(&rows)? {
            match state {
                crate::store::FolderState::Valid(r) => me.folders.push(r),
                crate::store::FolderState::Invalid { errors, .. } => return Err(StoreError::Invalid(errors)),
            }
        }
        Ok(me)
    }

    fn conn(&self) -> Result<&Object, StoreError> {
        self.conn.as_ref().ok_or_else(|| StoreError::Unavailable("the export has ended".to_owned()))
    }

    pub(crate) async fn folders(&self) -> Result<Vec<FolderRecord>, StoreError> {
        Ok(self.folders.clone())
    }

    pub(crate) async fn uids(&self) -> Result<Vec<Uid>, StoreError> {
        let rows = cl_q(self.conn()?, sql::P25, &[]).await.map_err(db)?;
        rows.iter().map(|r| parse_uid(&r.try_get::<_, String>(0).map_err(db)?)).collect()
    }

    pub(crate) async fn dashboard(&self, uid: &Uid) -> Result<DashboardRecord, StoreError> {
        let c = self.conn()?;
        let rows = cl_q(c, sql::P26, &[&uid.as_str()]).await.map_err(db)?;
        let mut versions = Vec::with_capacity(rows.len());
        for r in &rows {
            let document: Option<String> = r.try_get("document").map_err(db)?;
            let etag: Option<String> = r.try_get("etag").map_err(db)?;
            if let (Some(d), Some(e)) = (&document, &etag)
                && Etag::of(d.as_bytes()).to_hex() != *e
            {
                let n: i64 = r.try_get("version").map_err(db)?;
                return Err(invalid_one(format!(
                    "the SHA-256 of the document of version {n} of {uid} differs from its etag"
                )));
            }
            let bytes = document.map(String::into_bytes);
            let meta = version_meta(
                uid,
                r.try_get("version").map_err(db)?,
                &r.try_get::<_, String>("kind").map_err(db)?,
                &r.try_get::<_, String>("folder_id").map_err(db)?,
                etag.as_deref(),
                r.try_get::<_, SystemTime>("saved_at").map_err(db)?,
                &r.try_get::<_, String>("saved_by").map_err(db)?,
                &r.try_get::<_, String>("message").map_err(db)?,
                r.try_get("save_id").map_err(db)?,
                bytes.as_ref().map(|b| b.len() as u64),
            )?;
            versions.push(Version { meta, bytes });
        }
        let current = match cl_opt(c, sql::P8, &[&uid.as_str()]).await.map_err(db)? {
            None => None,
            Some(r) => {
                let row = DashRow::of(&r).map_err(db)?;
                match classify(uid, &row.document, &row.etag) {
                    Err(errors) => return Err(StoreError::Invalid(errors)),
                    Ok(doc) => Some(CurrentRecord {
                        folder: super::rows::parse_folder(&row.folder)?,
                        bytes: doc.bytes().to_vec(),
                    }),
                }
            }
        };
        Ok(DashboardRecord { uid: uid.clone(), versions, current })
    }

    pub(crate) async fn end(mut self) -> Result<(), StoreError> {
        let Some(c) = self.conn.take() else { return Ok(()) };
        match c.batch_execute("COMMIT").await {
            Ok(()) => Ok(()),
            Err(e) => {
                if is_broken(&e) || c.is_closed() {
                    discard(c);
                }
                Err(db(e))
            }
        }
    }
}

/// The importer of the PostgreSQL backend (5.3): one transaction.
pub struct PgImporter {
    conn: Option<Object>,
    report: ImportReport,
}

impl fmt::Debug for PgImporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgImporter").field("report", &self.report).finish()
    }
}

impl Drop for PgImporter {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            discard(c);
        }
    }
}

impl PgImporter {
    pub(crate) async fn begin(store: &PgStore) -> Result<Self, StoreError> {
        let obj = store.conn().await?;
        let me = PgImporter { conn: Some(obj), report: ImportReport::default() };
        {
            let c = me.conn()?;
            c.batch_execute("BEGIN").await.map_err(db)?;
            cl_opt(c, sql::P12, &[]).await.map_err(db)?;
            let count: i64 = match cl_opt(c, sql::P27, &[]).await.map_err(db)? {
                Some(r) => r.try_get(0).map_err(db)?,
                None => 0,
            };
            if count != 0 {
                return Err(StoreError::NotEmpty);
            }
        }
        Ok(me)
    }

    fn conn(&self) -> Result<&Object, StoreError> {
        self.conn.as_ref().ok_or_else(|| StoreError::Unavailable("the import has ended".to_owned()))
    }

    pub(crate) async fn folder(&mut self, rec: &FolderRecord) -> Result<(), StoreError> {
        let at = now();
        let c = self.conn()?;
        let st = c.prepare_cached(sql::P22).await.map_err(db)?;
        let n = c.execute(&st, &[&rec.id.as_str(), &rec.title.as_str(), &at]).await.map_err(db)?;
        if n == 0 {
            return Err(StoreError::Exists { folder: None });
        }
        self.report.folders += 1;
        Ok(())
    }

    /// Checks one record before any statement (5.39, item 2).
    fn check_record(rec: &DashboardRecord) -> Result<(), StoreError> {
        let uid = &rec.uid;
        let mut last = 0;
        for v in &rec.versions {
            let m = &v.meta;
            if m.uid != *uid {
                return Err(invalid_one(format!("version {} of {uid} names the uid {}", m.version, m.uid)));
            }
            if m.version <= last || m.version > i64::MAX as u64 {
                return Err(invalid_one(format!("the versions of {uid} are not in ascending order at {}", m.version)));
            }
            last = m.version;
            match (&v.bytes, m.etag) {
                (Some(b), Some(e)) if Etag::of(b) == e => {
                    if std::str::from_utf8(b).is_err() {
                        return Err(invalid_one(format!("the bytes of version {} of {uid} are not UTF-8", m.version)));
                    }
                }
                (Some(_), _) => {
                    return Err(invalid_one(format!(
                        "the bytes of version {} of {uid} do not match its etag",
                        m.version
                    )));
                }
                (None, Some(_)) if m.kind == VersionKind::Delete => {
                    return Err(invalid_one(format!("version {} of {uid} is a delete and has an etag", m.version)));
                }
                (None, None) if m.kind != VersionKind::Delete => {
                    return Err(invalid_one(format!("version {} of {uid} has no etag", m.version)));
                }
                _ => {}
            }
        }
        if let Some(cur) = &rec.current {
            validate(&cur.bytes, Some(uid)).map_err(StoreError::Invalid)?;
        }
        Ok(())
    }

    pub(crate) async fn dashboard(&mut self, rec: &DashboardRecord) -> Result<(), StoreError> {
        Self::check_record(rec)?;
        let uid = &rec.uid;
        let c = self.conn()?;
        let insert = c.prepare_cached(sql::P15).await.map_err(db)?;
        for v in &rec.versions {
            let m = &v.meta;
            let n = i64::try_from(m.version).map_err(|_| corrupt("a version number above i64"))?;
            let document: Option<&str> = match &v.bytes {
                Some(b) => Some(std::str::from_utf8(b).map_err(|_| corrupt("bytes that are not UTF-8"))?),
                None => None,
            };
            let etag = m.etag.map(|e| e.to_hex());
            let at = to_system(m.saved_at);
            let sid = m.save_id.map(|s| s.to_string());
            let p: [&(dyn ToSql + Sync); 10] = [
                &uid.as_str(),
                &n,
                &m.kind.as_str(),
                &m.folder.as_str(),
                &document,
                &etag,
                &at,
                &m.saved_by.as_str(),
                &m.message.as_str(),
                &sid,
            ];
            c.execute(&insert, &p).await.map_err(db_fk)?;
        }
        if let Some(cur) = &rec.current {
            let doc = validate(&cur.bytes, Some(uid)).map_err(StoreError::Invalid)?;
            let text = std::str::from_utf8(doc.bytes()).map_err(|_| corrupt("bytes that are not UTF-8"))?;
            let hex = doc.etag().to_hex();
            // The row takes the number and the author of the head version when its etag is the
            // current etag. Else the current document is an outside change (5.2, `Current`).
            let head = rec.versions.last();
            let (version, at, by): (i64, SystemTime, String) = match head {
                Some(h) if h.meta.etag == Some(*doc.etag()) => (
                    i64::try_from(h.meta.version).map_err(|_| corrupt("a version number above i64"))?,
                    to_system(h.meta.saved_at),
                    h.meta.saved_by.as_str().to_owned(),
                ),
                Some(h) => (
                    i64::try_from(h.meta.version).map_err(|_| corrupt("a version number above i64"))?,
                    now(),
                    "external".to_owned(),
                ),
                None => (1, now(), "external".to_owned()),
            };
            let format_version = i32::try_from(doc.format_version()).unwrap_or(i32::MAX);
            let p: [&(dyn ToSql + Sync); 9] =
                [&uid.as_str(), &cur.folder.as_str(), &version, &hex, &format_version, &doc.title(), &text, &at, &by];
            let st = c.prepare_cached(sql::P16).await.map_err(db)?;
            if c.execute(&st, &p).await.map_err(db_fk)? == 0 {
                return Err(StoreError::Exists { folder: None });
            }
        }
        self.report.dashboards += 1;
        self.report.versions += rec.versions.len() as u64;
        Ok(())
    }

    pub(crate) async fn commit(mut self) -> Result<ImportReport, StoreError> {
        let Some(c) = self.conn.take() else { return Err(StoreError::Unavailable("the import has ended".to_owned())) };
        match c.batch_execute("COMMIT").await {
            Ok(()) => Ok(self.report),
            Err(e) => {
                discard(c);
                Err(db(e))
            }
        }
    }
}

fn db_fk(e: tokio_postgres::Error) -> StoreError {
    if e.code() == Some(&tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION) {
        StoreError::FolderNotFound
    } else {
        db(e)
    }
}
