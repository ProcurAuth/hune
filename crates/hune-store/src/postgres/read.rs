// SPDX-License-Identifier: MIT OR Apache-2.0
//! The read side of the PostgreSQL backend: `load`, `poll`, `get`, `history`, `version` and the
//! probes (5.27, 5.30).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use deadpool_postgres::Object;
use tokio_postgres::IsolationLevel;
use tokio_postgres::types::ToSql;

use super::rows::{DashRow, classify, current_fields, folder_state, folder_text, parse_etag, parse_folder, parse_uid};
use super::{Fail, PgStore, R, cl_opt, cl_q, sql, tx_q};
use crate::store::{Delta, EntryKey, EntryMark, EntryProbe, FolderMark, FolderState, Marks, Problem, Snapshot};
use crate::types::{CatalogEntry, Current, Etag, FolderId, MAX_HISTORY_PAGE, StoreError, Uid, Version, VersionMeta};

/// The size of a group of uids that P7 reads (5.30, item 3).
const POLL_GROUP: usize = 1000;

/// Opens the transaction of a snapshot read: `REPEATABLE READ READ ONLY` (5.27).
async fn begin_snapshot(obj: &mut Object) -> R<deadpool_postgres::Transaction<'_>> {
    let tx = obj.build_transaction().isolation_level(IsolationLevel::RepeatableRead).read_only(true).start().await?;
    Ok(tx)
}

/// The catalog entries of rows and their marks. The check of each document runs in `spawn_blocking` (3.7).
async fn entries_off_thread(
    store: &PgStore,
    rows: Vec<DashRow>,
) -> Result<Vec<(Arc<CatalogEntry>, EntryMark)>, StoreError> {
    store.off_thread(move || rows.iter().map(DashRow::entry).collect::<Result<Vec<_>, _>>()).await?
}

/// The folders of P4, as states and marks.
pub(super) fn folders_of(rows: &[tokio_postgres::Row]) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
    rows.iter().map(folder_state).collect()
}

impl PgStore {
    async fn load_inner(c: &mut Object) -> R<(Vec<(FolderState, FolderMark)>, Vec<DashRow>)> {
        let tx = begin_snapshot(c).await?;
        let folders = tx_q(&tx, sql::P4, &[]).await?;
        let dashes = tx_q(&tx, sql::P5, &[]).await?;
        tx.commit().await?;
        let folders = folders_of(&folders)?;
        let dashes = dashes.iter().map(DashRow::of).collect::<Result<Vec<_>, _>>()?;
        Ok((folders, dashes))
    }

    pub(super) async fn load_impl(&self) -> Result<Snapshot, StoreError> {
        let (folders, dashes) = with_read!(self, |c| Self::load_inner(c))?;
        let mut marks = Marks::default();
        let mut states = Vec::new();
        for (state, mark) in folders {
            marks.folders.insert(state.id().clone(), mark);
            states.push(state);
        }
        let mut entries = Vec::new();
        for (entry, mark) in entries_off_thread(self, dashes).await? {
            marks.entries.insert((entry.uid().clone(), entry.folder().clone()), mark);
            entries.push(entry);
        }
        Ok(Snapshot { folders: states, entries, marks })
    }

    async fn poll_inner(c: &mut Object) -> R<(Vec<(FolderState, FolderMark)>, Vec<(String, String, String)>)> {
        let tx = begin_snapshot(c).await?;
        let folders = tx_q(&tx, sql::P4, &[]).await?;
        let heads = tx_q(&tx, sql::P6, &[]).await?;
        tx.commit().await?;
        let folders = folders_of(&folders)?;
        let mut out = Vec::with_capacity(heads.len());
        for r in &heads {
            out.push((
                r.try_get::<_, String>("uid")?,
                folder_text(&r, "folder_id")?,
                r.try_get::<_, String>("etag")?,
            ));
        }
        Ok((folders, out))
    }

    async fn fetch_rows(c: &mut Object, uids: Vec<String>) -> R<Vec<DashRow>> {
        let rows = cl_q(c, sql::P7, &[&uids as &(dyn ToSql + Sync)]).await?;
        Ok(rows.iter().map(DashRow::of).collect::<Result<Vec<_>, _>>()?)
    }

    pub(super) async fn poll_impl(&self, marks: &Marks) -> Result<Delta, StoreError> {
        let (folders, heads) = with_read!(self, |c| Self::poll_inner(c))?;
        // The folders: a new, a changed (title) and a removed folder.
        let mut new_marks = Marks::default();
        let mut changed_folders = Vec::new();
        for (state, mark) in folders {
            let id = state.id().clone();
            if marks.folders.get(&id).is_none_or(|m| m.state != state) {
                changed_folders.push(state);
            }
            new_marks.folders.insert(id, mark);
        }
        let removed_folders: Vec<FolderId> =
            marks.folders.keys().filter(|f| !new_marks.folders.contains_key(*f)).cloned().collect();
        // The entries: compare P6 with the marks.
        let mut current: BTreeMap<EntryKey, Etag> = BTreeMap::new();
        for (uid, folder, etag) in &heads {
            current.insert((parse_uid(uid)?, parse_folder(folder)?), parse_etag(etag)?);
        }
        let mut changed_uids: BTreeSet<Uid> = BTreeSet::new();
        for (key, etag) in &current {
            if marks.entries.get(key).is_none_or(|m| m.etag != *etag) {
                changed_uids.insert(key.0.clone());
            }
        }
        for (key, etag) in &current {
            if !changed_uids.contains(&key.0) {
                new_marks.entries.insert(key.clone(), EntryMark { etag: *etag, sig: None, ctx: 0 });
            }
        }
        let list: Vec<String> = changed_uids.iter().map(|u| u.as_str().to_owned()).collect();
        let mut entries = Vec::new();
        for group in list.chunks(POLL_GROUP) {
            let group = group.to_vec();
            let rows = with_read!(self, |c| Self::fetch_rows(c, group.clone()))?;
            for (entry, mark) in entries_off_thread(self, rows).await? {
                new_marks.entries.insert((entry.uid().clone(), entry.folder().clone()), mark);
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| (a.uid(), a.folder()).cmp(&(b.uid(), b.folder())));
        let removed_entries: Vec<EntryKey> =
            marks.entries.keys().filter(|k| !new_marks.entries.contains_key(*k)).cloned().collect();
        Ok(Delta { folders: changed_folders, removed_folders, entries, removed_entries, marks: new_marks })
    }

    async fn dash_row(c: &Object, uid: &Uid) -> R<Option<DashRow>> {
        match cl_opt(c, sql::P8, &[&uid.as_str()]).await? {
            Some(r) => Ok(Some(DashRow::of(&r)?)),
            None => Ok(None),
        }
    }

    pub(super) async fn get_impl(&self, uid: &Uid) -> Result<Current, StoreError> {
        let row = with_read!(self, |c| Self::dash_row(c, uid))?.ok_or(StoreError::NotFound)?;
        let owned = uid.clone();
        let (row, checked) = self
            .off_thread(move || {
                let checked = classify(&owned, &row.document, &row.etag);
                (row, checked)
            })
            .await?;
        match checked {
            Err(errors) => Err(StoreError::Invalid(errors)),
            Ok(doc) => {
                let folder = parse_folder(&row.folder)?;
                let (version, saved_at, saved_by) =
                    current_fields(&row.etag, row.updated_at, &row.updated_by, row.head.as_ref())?;
                Ok(Current {
                    uid: uid.clone(),
                    folder,
                    version,
                    etag: *doc.etag(),
                    bytes: Arc::new(doc.bytes().to_vec()),
                    saved_at,
                    saved_by,
                })
            }
        }
    }

    /// `Store::raw_current`: the column `document` of the row, without the validation.
    pub(super) async fn raw_current_impl(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        let row = with_read!(self, |c| Self::dash_row(c, uid))?.ok_or(StoreError::NotFound)?;
        super::rows::raw_of_row(&row, folder)
    }

    async fn history_inner(c: &Object, uid: &Uid, before: i64, limit: i64) -> R<Vec<VersionMeta>> {
        let rows = cl_q(c, sql::P20, &[&uid.as_str(), &before, &limit]).await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let size: Option<i32> = r.try_get(8)?;
            out.push(super::rows::version_meta(
                uid,
                r.try_get(0)?,
                &r.try_get::<_, String>(1)?,
                &folder_text(&r, 2)?,
                r.try_get::<_, Option<String>>(3)?.as_deref(),
                r.try_get(4)?,
                &r.try_get::<_, String>(5)?,
                &r.try_get::<_, String>(6)?,
                r.try_get(7)?,
                size.and_then(|s| u64::try_from(s).ok()),
            )?);
        }
        if out.is_empty() {
            let known: bool = match cl_opt(c, sql::P_UID_KNOWN, &[&uid.as_str()]).await? {
                Some(r) => r.try_get(0)?,
                None => false,
            };
            if !known {
                return Err(Fail::Store(StoreError::NotFound));
            }
        }
        Ok(out)
    }

    pub(super) async fn history_impl(
        &self,
        uid: &Uid,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<VersionMeta>, StoreError> {
        let limit = i64::from(limit.min(MAX_HISTORY_PAGE));
        let before = before.map_or(i64::MAX, |b| i64::try_from(b).unwrap_or(i64::MAX));
        with_read!(self, |c| Self::history_inner(c, uid, before, limit))
    }

    async fn version_inner(c: &Object, uid: &Uid, n: i64) -> R<Version> {
        let Some(r) = cl_opt(c, sql::P21, &[&uid.as_str(), &n]).await? else {
            return Err(Fail::Store(StoreError::VersionNotFound));
        };
        let document: Option<String> = r.try_get(2)?;
        let bytes = document.map(String::into_bytes);
        let meta = super::rows::version_meta(
            uid,
            n,
            &r.try_get::<_, String>(0)?,
            &folder_text(&r, 1)?,
            r.try_get::<_, Option<String>>(3)?.as_deref(),
            r.try_get(4)?,
            &r.try_get::<_, String>(5)?,
            &r.try_get::<_, String>(6)?,
            r.try_get(7)?,
            bytes.as_ref().map(|b| b.len() as u64),
        )?;
        Ok(Version { meta, bytes })
    }

    pub(super) async fn version_impl(&self, uid: &Uid, version: u64) -> Result<Version, StoreError> {
        let Ok(n) = i64::try_from(version) else { return Err(StoreError::VersionNotFound) };
        if n == 0 {
            return Err(StoreError::VersionNotFound);
        }
        with_read!(self, |c| Self::version_inner(c, uid, n))
    }

    pub(super) async fn entry_probe_impl(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        let row = with_read!(self, |c| Self::dash_row(c, uid))?;
        let entries = match row {
            Some(row) => entries_off_thread(self, vec![row]).await?,
            None => Vec::new(),
        };
        Ok(EntryProbe { entries })
    }

    async fn folders_inner(c: &Object) -> R<Vec<tokio_postgres::Row>> {
        Ok(cl_q(c, sql::P4, &[]).await?)
    }

    pub(super) async fn folder_probe_impl(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        let rows = with_read!(self, |c| Self::folders_inner(c))?;
        folders_of(&rows)
    }

    /// The invalid current documents (5.37, item 4). The versions are checked when the export reads them.
    pub(super) async fn check_impl(&self) -> Result<Vec<Problem>, StoreError> {
        let (folders, dashes) = with_read!(self, |c| Self::load_inner(c))?;
        let mut out = Vec::new();
        for (state, _) in &folders {
            if let FolderState::Invalid { id, errors } = state {
                out.push(Problem {
                    folder: Some(id.clone()),
                    uid: None,
                    place: format!("hune_folders.id = {id}"),
                    errors: errors.clone(),
                });
            }
        }
        for (entry, _) in entries_off_thread(self, dashes).await? {
            if let CatalogEntry::Invalid { uid, folder, errors, .. } = &*entry {
                out.push(Problem {
                    folder: Some(folder.clone()),
                    uid: Some(uid.clone()),
                    place: format!("hune_dashboards.uid = {uid} (folder {folder})"),
                    errors: errors.clone(),
                });
            }
        }
        Ok(out)
    }
}
