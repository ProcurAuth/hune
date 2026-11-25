// SPDX-License-Identifier: MIT OR Apache-2.0
//! The write side of the PostgreSQL backend: the transactions of 5.27.
//!
//! Each write is one transaction at the level `READ COMMITTED`. The lock of the row (P9) or the
//! lock of the creates (P12) serialises the writes. A write that returns without a change drops
//! its transaction, which sends `ROLLBACK`. An error at `COMMIT` gives `Unavailable`: the outcome
//! is then unknown, and the store never sends the write again (5.5, rule 11).

use std::time::SystemTime;

use deadpool_postgres::{Object, Transaction};
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;

use super::rows::{
    HeadRow, classify, corrupt, current_fields, folder_param, folder_text, now, parse_etag, parse_folder, to_system,
};
use super::{Fail, PgStore, R, sql, tx_opt, tx_x};
use crate::types::{
    CanonicalDocument, ConflictInfo, Etag, FolderId, FolderTitle, MAX_DASHBOARDS, MAX_FOLDERS, Outcome, Saved,
    StoreError, Uid, ValidVersion, VersionKind, WriteMeta,
};

/// The data of a new document that a write needs.
pub(super) struct NewDoc {
    uid: Uid,
    etag: Etag,
    hex: String,
    text: String,
    title: String,
    format_version: i32,
}

impl NewDoc {
    pub(super) fn of(doc: &CanonicalDocument) -> Result<NewDoc, StoreError> {
        let text = std::str::from_utf8(doc.bytes()).map_err(|_| corrupt("a document that is not UTF-8"))?.to_owned();
        Ok(NewDoc {
            uid: doc.uid().clone(),
            etag: *doc.etag(),
            hex: doc.etag().to_hex(),
            text,
            title: doc.title().to_owned(),
            format_version: i32::try_from(doc.format_version()).unwrap_or(i32::MAX),
        })
    }
}

/// The current row, as P9 gives it.
pub(super) struct CurRow {
    folder: String,
    etag: String,
    document: String,
    updated_at: SystemTime,
    updated_by: String,
}

/// The head version, as P11 gives it.
pub(super) struct Head {
    version: i64,
    etag: Option<String>,
    folder: String,
}

impl Head {
    fn row(&self) -> HeadRow {
        HeadRow { version: self.version, etag: self.etag.clone() }
    }
}

/// The request of a write, for the `save_id` check (5.5, rule 4).
enum Req<'a> {
    /// `create`, `save` and `restore`: the kind and the new etag.
    Save { kind: VersionKind, etag: &'a Etag },
    /// `move_to`: the target folder.
    Move { target: &'a FolderId },
    /// `delete`.
    Delete,
}

fn fk(e: tokio_postgres::Error) -> Fail {
    if e.code() == Some(&SqlState::FOREIGN_KEY_VIOLATION) {
        Fail::Store(StoreError::FolderNotFound)
    } else {
        Fail::Db(e)
    }
}

/// P9: lock of the current row.
async fn lock_cur(tx: &Transaction<'_>, uid: &Uid) -> R<Option<CurRow>> {
    let Some(r) = tx_opt(tx, sql::P9, &[&uid.as_str()]).await? else { return Ok(None) };
    Ok(Some(CurRow {
        folder: folder_text(&r, "folder_id")?,
        etag: r.try_get("etag")?,
        document: r.try_get("document")?,
        updated_at: r.try_get("updated_at")?,
        updated_by: r.try_get("updated_by")?,
    }))
}

/// P11: the head version.
async fn head_of(tx: &Transaction<'_>, uid: &Uid) -> R<Option<Head>> {
    let Some(r) = tx_opt(tx, sql::P11, &[&uid.as_str()]).await? else { return Ok(None) };
    Ok(Some(Head { version: r.try_get("version")?, etag: r.try_get("etag")?, folder: folder_text(&r, "folder_id")? }))
}

/// P12: the lock of the creates.
async fn lock_creates(tx: &Transaction<'_>) -> R<()> {
    tx_opt(tx, sql::P12, &[]).await?;
    Ok(())
}

/// P14: the folder exists. It also takes a share lock on the row.
async fn folder_exists(tx: &Transaction<'_>, folder: &FolderId) -> R<bool> {
    // The root is not a row: it always exists.
    if folder.is_root() {
        return Ok(true);
    }
    Ok(tx_opt(tx, sql::P14, &[&folder.as_str()]).await?.is_some())
}

/// P13: the number of dashboards and the number of folders.
async fn counts(tx: &Transaction<'_>) -> R<(i64, i64)> {
    match tx_opt(tx, sql::P13, &[]).await? {
        Some(r) => Ok((r.try_get(0)?, r.try_get(1)?)),
        None => Ok((0, 0)),
    }
}

/// P15: insert a version.
#[allow(clippy::too_many_arguments)]
async fn insert_version(
    tx: &Transaction<'_>,
    uid: &Uid,
    n: i64,
    kind: VersionKind,
    folder: &str,
    document: Option<&str>,
    etag: Option<&str>,
    saved_at: SystemTime,
    saved_by: &str,
    message: &str,
    save_id: Option<String>,
) -> R<()> {
    let folder = folder_param(folder);
    let p: [&(dyn ToSql + Sync); 10] =
        [&uid.as_str(), &n, &kind.as_str(), &folder, &document, &etag, &saved_at, &saved_by, &message, &save_id];
    tx_x(tx, sql::P15, &p).await?;
    Ok(())
}

/// P19: the retention. It removes the versions at or below `n - history_keep`.
async fn retention(tx: &Transaction<'_>, uid: &Uid, n: i64, keep: i64) -> R<()> {
    let limit = n.saturating_sub(keep);
    if limit >= 1 {
        tx_x(tx, sql::P19, &[&uid.as_str(), &limit]).await?;
    }
    Ok(())
}

fn saved(uid: &Uid, folder: &FolderId, version: i64, etag: Option<Etag>, outcome: Outcome) -> Saved {
    Saved { uid: uid.clone(), folder: folder.clone(), version: u64::try_from(version).unwrap_or(0), etag, outcome }
}

/// The check of the row of P21 in the transaction of a restore (5.27). A tombstone, a version without bytes, or a
/// version whose etag is not `want`, the etag of the valid document, gives `NotRestorable`. It parses no document:
/// the caller validated the bytes of the version before the transaction.
fn restorable(kind: &str, etag: Option<&str>, has_bytes: bool, want: &str) -> Result<(), StoreError> {
    if kind == VersionKind::Delete.as_str() || !has_bytes || etag != Some(want) {
        Err(StoreError::NotRestorable)
    } else {
        Ok(())
    }
}

impl PgStore {
    /// The `Conflict` error with the state of the current row.
    fn conflict(cur: &CurRow, head: Option<&Head>) -> Result<StoreError, StoreError> {
        let h = head.map(Head::row);
        let (version, saved_at, saved_by) = current_fields(&cur.etag, cur.updated_at, &cur.updated_by, h.as_ref())?;
        Ok(StoreError::Conflict(Box::new(ConflictInfo {
            etag: parse_etag(&cur.etag)?,
            version,
            folder: parse_folder(&cur.folder)?,
            saved_at,
            saved_by,
        })))
    }

    /// P10 and 5.5, rules 3 and 4: `Some` for a replay, an error for a save id of another request.
    async fn sid_check(tx: &Transaction<'_>, uid: &Uid, meta: &WriteMeta, req: &Req<'_>) -> R<Option<Saved>> {
        let Some(sid) = meta.save_id else { return Ok(None) };
        let text = sid.to_string();
        let Some(r) = tx_opt(tx, sql::P10, &[&uid.as_str(), &text]).await? else { return Ok(None) };
        let version: i64 = r.try_get("version")?;
        let kind: String = r.try_get("kind")?;
        let folder = folder_text(&r, "folder_id")?;
        let etag: Option<String> = r.try_get("etag")?;
        let kind = VersionKind::parse(&kind).ok_or_else(|| corrupt(format!("the kind {kind:?}")))?;
        let folder = parse_folder(&folder)?;
        let etag = etag.as_deref().map(parse_etag).transpose()?;
        let same = match req {
            Req::Save { kind: k, etag: e } => kind == *k && etag.as_ref() == Some(*e),
            Req::Move { target } => kind == VersionKind::Move && folder == **target,
            Req::Delete => kind == VersionKind::Delete,
        };
        if !same {
            return Err(Fail::Store(StoreError::SaveIdReused));
        }
        Ok(Some(saved(uid, &folder, version, etag, Outcome::Replayed)))
    }

    /// Step "P11 and the version `external` when needed" (5.27). It gives the number of the next version.
    async fn record_external(tx: &Transaction<'_>, uid: &Uid, cur: &CurRow, head: Option<&Head>) -> R<i64> {
        let n = head.map_or(0, |h| h.version) + 1;
        if head.is_some_and(|h| h.etag.as_deref() == Some(cur.etag.as_str())) {
            return Ok(n);
        }
        let valid = classify(uid, &cur.document, &cur.etag).is_ok();
        let document = valid.then_some(cur.document.as_str());
        insert_version(
            tx,
            uid,
            n,
            VersionKind::External,
            &cur.folder,
            document,
            Some(&cur.etag),
            cur.updated_at,
            "external",
            "",
            None,
        )
        .await?;
        Ok(n + 1)
    }

    /// The steps of `save` after P9 (5.27). `Ok((saved, true))` means that the caller commits.
    #[allow(clippy::too_many_arguments)]
    async fn save_core(
        &self,
        tx: &Transaction<'_>,
        uid: &Uid,
        cur: Option<CurRow>,
        nd: &NewDoc,
        e_if: Option<&Etag>,
        expected: Option<&FolderId>,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> R<(Saved, bool)> {
        if let Some(s) = Self::sid_check(tx, uid, meta, &Req::Save { kind, etag: &nd.etag }).await? {
            return Ok((s, false));
        }
        let Some(cur) = cur else { return Err(Fail::Store(StoreError::NotFound)) };
        let head = head_of(tx, uid).await?;
        let cur_folder = parse_folder(&cur.folder)?;
        if e_if.map(Etag::to_hex).as_deref() != Some(cur.etag.as_str()) || expected.is_some_and(|f| *f != cur_folder) {
            return Err(Fail::Store(Self::conflict(&cur, head.as_ref())?));
        }
        if nd.hex == cur.etag {
            let h = head.as_ref().map(Head::row);
            let (version, _, _) = current_fields(&cur.etag, cur.updated_at, &cur.updated_by, h.as_ref())?;
            let s = saved(uid, &cur_folder, i64::try_from(version).unwrap_or(0), Some(nd.etag), Outcome::Unchanged);
            return Ok((s, false));
        }
        let n = Self::record_external(tx, uid, &cur, head.as_ref()).await?;
        insert_version(
            tx,
            uid,
            n,
            kind,
            &cur.folder,
            Some(&nd.text),
            Some(&nd.hex),
            to_system(meta.saved_at),
            meta.saved_by.as_str(),
            meta.message.as_str(),
            meta.save_id.map(|s| s.to_string()),
        )
        .await?;
        let cur_folder_bind = folder_param(&cur.folder);
        let p: [&(dyn ToSql + Sync); 9] = [
            &uid.as_str(),
            &cur_folder_bind,
            &n,
            &nd.hex,
            &nd.format_version,
            &nd.title,
            &nd.text,
            &to_system(meta.saved_at),
            &meta.saved_by.as_str(),
        ];
        tx_x(tx, sql::P17, &p).await.map_err(fk)?;
        retention(tx, uid, n, self.history_keep()).await?;
        Ok((saved(uid, &cur_folder, n, Some(nd.etag), Outcome::Written), true))
    }

    pub(super) async fn save_impl(
        &self,
        doc: &CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let nd = NewDoc::of(doc)?;
        let mut obj = self.conn().await?;
        let res = self.save_tx(&mut obj, &nd, if_match, expected_folder, kind, meta).await;
        self.settle(obj, res)
    }

    async fn save_tx(
        &self,
        obj: &mut Object,
        nd: &NewDoc,
        if_match: &Etag,
        expected: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> R<Saved> {
        let tx = obj.transaction().await?;
        let cur = lock_cur(&tx, &nd.uid).await?;
        let (s, commit) = self.save_core(&tx, &nd.uid, cur, nd, Some(if_match), Some(expected), kind, meta).await?;
        if commit {
            self.commit(tx).await?;
        }
        Ok(s)
    }

    /// The steps of `create` after P12 (5.27). `restoring` is true for the restore of a deleted
    /// dashboard: the versions of the uid stay, and the number is the head plus 1.
    async fn create_core(
        &self,
        tx: &Transaction<'_>,
        folder: &FolderId,
        nd: &NewDoc,
        kind: VersionKind,
        meta: &WriteMeta,
        restoring: bool,
    ) -> R<Saved> {
        let uid = &nd.uid;
        if let Some(s) = Self::sid_check(tx, uid, meta, &Req::Save { kind, etag: &nd.etag }).await? {
            return Ok(s);
        }
        if !folder_exists(tx, folder).await? {
            return Err(Fail::Store(StoreError::FolderNotFound));
        }
        let (dashboards, _) = counts(tx).await?;
        if usize::try_from(dashboards).unwrap_or(usize::MAX) >= MAX_DASHBOARDS {
            return Err(Fail::Store(StoreError::Full));
        }
        let head = head_of(tx, uid).await?;
        if !restoring && let Some(h) = &head {
            return Err(Fail::Store(StoreError::Exists { folder: Some(parse_folder(&h.folder)?) }));
        }
        let n = head.as_ref().map_or(0, |h| h.version) + 1;
        let at = to_system(meta.saved_at);
        let folder_bind = folder.wire();
        let p: [&(dyn ToSql + Sync); 9] = [
            &uid.as_str(),
            &folder_bind,
            &n,
            &nd.hex,
            &nd.format_version,
            &nd.title,
            &nd.text,
            &at,
            &meta.saved_by.as_str(),
        ];
        if tx_x(tx, sql::P16, &p).await.map_err(fk)? == 0 {
            let existing = match tx_opt(tx, sql::P_FOLDER_OF, &[&uid.as_str()]).await? {
                Some(r) => Some(parse_folder(&folder_text(&r, 0)?)?),
                None => None,
            };
            return Err(Fail::Store(StoreError::Exists { folder: existing }));
        }
        insert_version(
            tx,
            uid,
            n,
            kind,
            folder.as_str(),
            Some(&nd.text),
            Some(&nd.hex),
            at,
            meta.saved_by.as_str(),
            meta.message.as_str(),
            meta.save_id.map(|s| s.to_string()),
        )
        .await
        .map_err(|f| match f {
            Fail::Db(e) => fk(e),
            other => other,
        })?;
        if restoring {
            retention(tx, uid, n, self.history_keep()).await?;
        }
        Ok(saved(uid, folder, n, Some(nd.etag), Outcome::Written))
    }

    pub(super) async fn create_impl(
        &self,
        folder: &FolderId,
        doc: &CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let nd = NewDoc::of(doc)?;
        let mut obj = self.conn().await?;
        let res = self.create_tx(&mut obj, folder, &nd, kind, meta).await;
        self.settle(obj, res)
    }

    async fn create_tx(
        &self,
        obj: &mut Object,
        folder: &FolderId,
        nd: &NewDoc,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> R<Saved> {
        let tx = obj.transaction().await?;
        lock_creates(&tx).await?;
        let s = self.create_core(&tx, folder, nd, kind, meta, false).await?;
        if s.outcome == Outcome::Written {
            self.commit(tx).await?;
        }
        Ok(s)
    }

    pub(super) async fn move_impl(
        &self,
        uid: &Uid,
        target: &FolderId,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let mut obj = self.conn().await?;
        let res = self.move_tx(&mut obj, uid, target, if_match, expected, meta).await;
        self.settle(obj, res)
    }

    async fn move_tx(
        &self,
        obj: &mut Object,
        uid: &Uid,
        target: &FolderId,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> R<Saved> {
        let tx = obj.transaction().await?;
        let cur = lock_cur(&tx, uid).await?;
        if let Some(s) = Self::sid_check(&tx, uid, meta, &Req::Move { target }).await? {
            return Ok(s);
        }
        let Some(cur) = cur else { return Err(Fail::Store(StoreError::NotFound)) };
        let head = head_of(&tx, uid).await?;
        let cur_folder = parse_folder(&cur.folder)?;
        if if_match.to_hex() != cur.etag || *expected != cur_folder {
            return Err(Fail::Store(Self::conflict(&cur, head.as_ref())?));
        }
        let (owned, document, etag) = (uid.clone(), cur.document.clone(), cur.etag.clone());
        let doc = self
            .off_thread(move || classify(&owned, &document, &etag))
            .await?
            .map_err(|e| Fail::Store(StoreError::Invalid(e)))?;
        if cur_folder == *target {
            let h = head.as_ref().map(Head::row);
            let (version, _, _) = current_fields(&cur.etag, cur.updated_at, &cur.updated_by, h.as_ref())?;
            return Ok(saved(uid, target, i64::try_from(version).unwrap_or(0), Some(*doc.etag()), Outcome::Unchanged));
        }
        if !folder_exists(&tx, target).await? {
            return Err(Fail::Store(StoreError::FolderNotFound));
        }
        let n = Self::record_external(&tx, uid, &cur, head.as_ref()).await?;
        let at = to_system(meta.saved_at);
        insert_version(
            &tx,
            uid,
            n,
            VersionKind::Move,
            target.as_str(),
            Some(&cur.document),
            Some(&cur.etag),
            at,
            meta.saved_by.as_str(),
            meta.message.as_str(),
            meta.save_id.map(|s| s.to_string()),
        )
        .await?;
        let format_version = i32::try_from(doc.format_version()).unwrap_or(i32::MAX);
        let target_bind = target.wire();
        let p: [&(dyn ToSql + Sync); 9] = [
            &uid.as_str(),
            &target_bind,
            &n,
            &cur.etag,
            &format_version,
            &doc.title(),
            &cur.document,
            &at,
            &meta.saved_by.as_str(),
        ];
        tx_x(&tx, sql::P17, &p).await.map_err(fk)?;
        retention(&tx, uid, n, self.history_keep()).await?;
        self.commit(tx).await?;
        Ok(saved(uid, target, n, Some(*doc.etag()), Outcome::Written))
    }

    pub(super) async fn delete_impl(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let mut obj = self.conn().await?;
        let res = self.delete_tx(&mut obj, uid, if_match, expected, meta).await;
        self.settle(obj, res)
    }

    async fn delete_tx(
        &self,
        obj: &mut Object,
        uid: &Uid,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> R<Saved> {
        let tx = obj.transaction().await?;
        let cur = lock_cur(&tx, uid).await?;
        if let Some(s) = Self::sid_check(&tx, uid, meta, &Req::Delete).await? {
            return Ok(s);
        }
        let Some(cur) = cur else { return Err(Fail::Store(StoreError::NotFound)) };
        let head = head_of(&tx, uid).await?;
        let cur_folder = parse_folder(&cur.folder)?;
        if if_match.to_hex() != cur.etag || *expected != cur_folder {
            return Err(Fail::Store(Self::conflict(&cur, head.as_ref())?));
        }
        let n = Self::record_external(&tx, uid, &cur, head.as_ref()).await?;
        insert_version(
            &tx,
            uid,
            n,
            VersionKind::Delete,
            &cur.folder,
            None,
            None,
            to_system(meta.saved_at),
            meta.saved_by.as_str(),
            meta.message.as_str(),
            meta.save_id.map(|s| s.to_string()),
        )
        .await?;
        tx_x(&tx, sql::P18, &[&uid.as_str()]).await?;
        retention(&tx, uid, n, self.history_keep()).await?;
        self.commit(tx).await?;
        Ok(saved(uid, &cur_folder, n, None, Outcome::Written))
    }

    /// `restore` with a version that the caller read and validated (5.27): the transaction never parses or validates
    /// the document.
    pub(super) async fn restore_impl(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let n = match i64::try_from(version.number()) {
            Ok(n) if n >= 1 => n,
            _ => return Err(StoreError::VersionNotFound),
        };
        let nd = NewDoc::of(version.document())?;
        let mut obj = self.conn().await?;
        let res = self.restore_tx(&mut obj, &nd, n, if_match, expected, meta).await;
        self.settle(obj, res)
    }

    async fn restore_tx(
        &self,
        obj: &mut Object,
        nd: &NewDoc,
        n: i64,
        if_match: Option<&Etag>,
        expected: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> R<Saved> {
        let uid = &nd.uid;
        let tx = obj.transaction().await?;
        let Some(v) = tx_opt(&tx, sql::P21, &[&uid.as_str(), &n]).await? else {
            return Err(Fail::Store(StoreError::VersionNotFound));
        };
        // The row tells that the version still has its bytes: the retention can remove it after the read of the
        // caller. The caller validated these bytes, so the row gives only the folder of the version.
        let kind: String = v.try_get("kind")?;
        let vfolder = folder_text(&v, "folder_id")?;
        let has_bytes = v.try_get::<_, Option<&str>>("document")?.is_some();
        restorable(&kind, v.try_get("etag")?, has_bytes, &nd.hex).map_err(Fail::Store)?;
        let cur = lock_cur(&tx, uid).await?;
        let (s, commit) = match cur {
            Some(c) => self.save_core(&tx, uid, Some(c), nd, if_match, expected, VersionKind::Restore, meta).await?,
            None => {
                if if_match.is_some() {
                    return Err(Fail::Store(StoreError::NotFound));
                }
                lock_creates(&tx).await?;
                match lock_cur(&tx, uid).await? {
                    Some(c) => {
                        self.save_core(&tx, uid, Some(c), nd, if_match, expected, VersionKind::Restore, meta).await?
                    }
                    None => {
                        let folder = parse_folder(&vfolder)?;
                        let s = self.create_core(&tx, &folder, nd, VersionKind::Restore, meta, true).await?;
                        let w = s.outcome == Outcome::Written;
                        (s, w)
                    }
                }
            }
        };
        if commit {
            self.commit(tx).await?;
        }
        Ok(s)
    }

    pub(super) async fn create_folder_impl(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        let mut obj = self.conn().await?;
        let res = self.create_folder_tx(&mut obj, id, title).await;
        self.settle(obj, res)
    }

    async fn create_folder_tx(&self, obj: &mut Object, id: &FolderId, title: &FolderTitle) -> R<()> {
        let tx = obj.transaction().await?;
        lock_creates(&tx).await?;
        let (_, folders) = counts(&tx).await?;
        if usize::try_from(folders).unwrap_or(usize::MAX) >= MAX_FOLDERS {
            return Err(Fail::Store(StoreError::Full));
        }
        let at = now();
        if tx_x(&tx, sql::P22, &[&id.as_str(), &title.as_str(), &at]).await? == 0 {
            return Err(Fail::Store(StoreError::Exists { folder: None }));
        }
        self.commit(tx).await
    }

    pub(super) async fn rename_folder_impl(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        let obj = self.conn().await?;
        let res = async {
            let at = now();
            let st = obj.prepare_cached(sql::P23).await?;
            let n = obj.execute(&st, &[&id.as_str(), &title.as_str(), &at]).await?;
            if n == 0 { Err(Fail::Store(StoreError::FolderNotFound)) } else { Ok(()) }
        }
        .await;
        self.settle(obj, res)
    }

    pub(super) async fn delete_folder_impl(&self, id: &FolderId) -> Result<(), StoreError> {
        let obj = self.conn().await?;
        let res = async {
            let st = obj.prepare_cached(sql::P24).await?;
            match obj.execute(&st, &[&id.as_str()]).await {
                Ok(0) => Err(Fail::Store(StoreError::FolderNotFound)),
                Ok(_) => Ok(()),
                // `ON DELETE RESTRICT` gives `23001`; `NO ACTION` would give `23503` (5.27).
                Err(e)
                    if e.code() == Some(&SqlState::RESTRICT_VIOLATION)
                        || e.code() == Some(&SqlState::FOREIGN_KEY_VIOLATION) =>
                {
                    Err(Fail::Store(StoreError::FolderNotEmpty))
                }
                Err(e) => Err(Fail::Db(e)),
            }
        }
        .await;
        self.settle(obj, res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transaction of a restore checks the kind of the row and the presence of its bytes, and nothing else: it
    /// never validates the document, because the caller validated it before the transaction (6.18).
    #[test]
    fn the_row_check_of_a_restore_validates_nothing() {
        let e = "ab".repeat(32);
        for kind in ["save", "external", "restore", "move", "create", "import", "migrate"] {
            assert_eq!(restorable(kind, Some(&e), true, &e), Ok(()), "{kind}");
        }
        assert_eq!(restorable("delete", None, false, &e), Err(StoreError::NotRestorable));
        assert_eq!(restorable("external", Some(&e), false, &e), Err(StoreError::NotRestorable));
        assert_eq!(restorable("save", Some(&"cd".repeat(32)), true, &e), Err(StoreError::NotRestorable));
    }
}
