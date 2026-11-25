// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `Store` trait (5.3), the enum `AnyStore` (R8) and the types that the trait needs.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use crate::postgres::{PgExporter, PgImporter, PgStore};
use crate::types::{
    CatalogEntry, Current, Etag, FolderId, FolderRecord, Saved, StoreError, Uid, ValidVersion, ValidationError,
    Version, VersionKind, VersionMeta, WriteMeta,
};
use crate::volume::{VolumeExporter, VolumeImporter, VolumeStore};

/// The signature of a file at a poll (5.17): the device, the inode, the size and the two
/// change times. It is a hint only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSig {
    pub(crate) dev: u64,
    pub(crate) ino: u64,
    pub(crate) size: u64,
    pub(crate) mtime_ns: i128,
    pub(crate) ctime_ns: i128,
}

/// The state of one folder: a valid folder with its title, or an invalid folder (5.10).
#[derive(Debug, Clone, PartialEq)]
pub enum FolderState {
    /// A folder with a valid folder file.
    Valid(FolderRecord),
    /// A folder directory without a valid folder file.
    Invalid {
        /// The folder id (the directory name).
        id: FolderId,
        /// The errors of the folder file.
        errors: Vec<ValidationError>,
    },
}

impl FolderState {
    /// The folder id.
    pub fn id(&self) -> &FolderId {
        match self {
            FolderState::Valid(r) => &r.id,
            FolderState::Invalid { id, .. } => id,
        }
    }
}

/// The mark of one folder in the catalog.
#[derive(Debug, Clone)]
pub struct FolderMark {
    pub(crate) sig: Option<FileSig>,
    pub(crate) state: FolderState,
}

/// The mark of one catalog entry.
#[derive(Debug, Clone)]
pub struct EntryMark {
    pub(crate) etag: Etag,
    pub(crate) sig: Option<FileSig>,
    /// The context of the entry: bit 0 is "the folder is invalid", bit 1 is "the uid is not unique".
    pub(crate) ctx: u8,
}

/// The key of a catalog entry: the uid and the folder. Two folders can hold one uid (5.10, item 3).
pub type EntryKey = (Uid, FolderId);

/// The marks of the catalog: one item for each folder and each entry (5.3).
#[derive(Debug, Clone, Default)]
pub struct Marks {
    pub(crate) folders: BTreeMap<FolderId, FolderMark>,
    pub(crate) entries: BTreeMap<EntryKey, EntryMark>,
}

impl Marks {
    /// Makes marks with no item.
    pub fn empty() -> Self {
        Marks::default()
    }
    /// The number of items.
    pub fn len(&self) -> usize {
        self.folders.len() + self.entries.len()
    }
    /// Tells if the marks have no item.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.entries.is_empty()
    }
}

/// All folders and all catalog entries of a store (5.3).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The folders, sorted by id.
    pub folders: Vec<FolderState>,
    /// The entries, sorted by uid and folder.
    pub entries: Vec<Arc<CatalogEntry>>,
    /// The marks of this state.
    pub marks: Marks,
}

/// The changes since the marks (5.3).
#[derive(Debug, Clone)]
pub struct Delta {
    /// The folders that are new or changed.
    pub folders: Vec<FolderState>,
    /// The folders that are gone.
    pub removed_folders: Vec<FolderId>,
    /// The entries that are new or changed.
    pub entries: Vec<Arc<CatalogEntry>>,
    /// The entries that are gone.
    pub removed_entries: Vec<EntryKey>,
    /// The complete new marks.
    pub marks: Marks,
}

/// The state of one uid, read for the catalog after an own write.
#[derive(Debug, Clone)]
pub struct EntryProbe {
    /// The entries of the uid: none, one, or one for each folder that holds the uid.
    pub entries: Vec<(Arc<CatalogEntry>, EntryMark)>,
}

/// An event that the store gives to the server for its log (section 14).
#[derive(Debug, Clone, PartialEq)]
pub enum StoreEvent {
    /// `store.entry_ignored`: the scan ignores an entry.
    EntryIgnored {
        /// The path relative to the root.
        path: String,
        /// The reason.
        reason: String,
    },
    /// `dashboard.changed` with the action `external`.
    ExternalChange {
        /// The uid.
        uid: Uid,
        /// The folder.
        folder: FolderId,
        /// The etag in the catalog before the change.
        etag_before: Option<Etag>,
        /// The etag after the change.
        etag_after: Option<Etag>,
    },
    /// `store.lock_unsupported`: the file system has no `flock`.
    LockUnsupported {
        /// The path of the lock file.
        path: String,
        /// The error number.
        errno: i32,
    },
    /// `store.rescan_rejected`: a poll of a store did not take a change: a file of a volume that is not valid, or a
    /// folder id or a uid that another store holds (5.44).
    RescanRejected {
        /// The id of the store.
        store: String,
        /// The path of the file, or the folder and the uid.
        path: String,
        /// The reason. It never goes to a client.
        reason: String,
    },
    /// A `delete_folder` found an entry in the folder (5.15, SV-23).
    FolderNotEmpty {
        /// The folder.
        folder: FolderId,
        /// The name of the first entry that is not `_folder.json`.
        entry: String,
    },
}

/// The current document of a uid with its folder, for an export or an import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentRecord {
    /// The folder.
    pub folder: FolderId,
    /// The canonical bytes.
    pub bytes: Vec<u8>,
}

/// One uid with all its versions, for an export or an import (5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardRecord {
    /// The uid.
    pub uid: Uid,
    /// All versions, from the lowest number to the highest.
    pub versions: Vec<Version>,
    /// The current document, or `None` after a delete.
    pub current: Option<CurrentRecord>,
}

/// The counts of an import or a copy (5.37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// The number of folders.
    pub folders: u64,
    /// The number of uids with a current document or a version.
    pub dashboards: u64,
    /// The number of versions.
    pub versions: u64,
}

/// The reader of a consistent copy of a store (5.3).
#[derive(Debug)]
pub enum Exporter {
    /// The volume backend.
    Volume(VolumeExporter),
    /// The PostgreSQL backend.
    Postgres(Box<PgExporter>),
}

impl Exporter {
    /// The folders, sorted by id.
    pub async fn folders(&self) -> Result<Vec<FolderRecord>, StoreError> {
        match self {
            Exporter::Volume(e) => e.folders().await,
            Exporter::Postgres(e) => e.folders().await,
        }
    }
    /// The uids that have a current document or a version, in byte order.
    pub async fn uids(&self) -> Result<Vec<Uid>, StoreError> {
        match self {
            Exporter::Volume(e) => e.uids().await,
            Exporter::Postgres(e) => e.uids().await,
        }
    }
    /// One uid with all its versions and its current document.
    pub async fn dashboard(&self, uid: &Uid) -> Result<DashboardRecord, StoreError> {
        match self {
            Exporter::Volume(e) => e.dashboard(uid).await,
            Exporter::Postgres(e) => e.dashboard(uid).await,
        }
    }
    /// Ends the export.
    pub async fn end(self) -> Result<(), StoreError> {
        match self {
            Exporter::Volume(e) => e.end().await,
            Exporter::Postgres(e) => e.end().await,
        }
    }
}

/// The writer of an import into an empty store (5.3).
#[derive(Debug)]
pub enum Importer {
    /// The volume backend.
    Volume(VolumeImporter),
    /// The PostgreSQL backend.
    Postgres(Box<PgImporter>),
}

impl Importer {
    /// Writes one folder.
    pub async fn folder(&mut self, record: &FolderRecord) -> Result<(), StoreError> {
        match self {
            Importer::Volume(i) => i.folder(record).await,
            Importer::Postgres(i) => i.folder(record).await,
        }
    }
    /// Writes one uid: its versions from the lowest number to the highest, then its current document.
    pub async fn dashboard(&mut self, record: &DashboardRecord) -> Result<(), StoreError> {
        match self {
            Importer::Volume(i) => i.dashboard(record).await,
            Importer::Postgres(i) => i.dashboard(record).await,
        }
    }
    /// Ends the import and gives the counts.
    pub async fn commit(self) -> Result<ImportReport, StoreError> {
        match self {
            Importer::Volume(i) => i.commit().await,
            Importer::Postgres(i) => i.commit().await,
        }
    }
}

/// The contract of a storage backend (5.3). Each method is async. The futures are `Send`.
pub trait Store: Send + Sync {
    /// Reads all folders and all catalog entries, valid and invalid.
    fn load(&self) -> impl Future<Output = Result<Snapshot, StoreError>> + Send;
    /// Reads the changes since the marks.
    fn poll(&self, marks: &Marks) -> impl Future<Output = Result<Delta, StoreError>> + Send;
    /// Reads the current document of a uid from the backend.
    fn get(&self, uid: &Uid) -> impl Future<Output = Result<Current, StoreError>> + Send;
    /// Reads the stored bytes of the current document of a uid in a folder, without the
    /// validation, with their etag (the SHA-256 of the bytes, 4.39). It serves the download of an
    /// invalid entry (6.14). `NotFound` when the uid is not in the folder.
    fn raw_current(
        &self,
        uid: &Uid,
        folder: &FolderId,
    ) -> impl Future<Output = Result<(Vec<u8>, Etag), StoreError>> + Send;
    /// Makes a new dashboard (version 1, or the next version of a deleted uid for a restore).
    fn create(
        &self,
        folder: &FolderId,
        doc: &crate::types::CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Saves a new version of a dashboard.
    fn save(
        &self,
        doc: &crate::types::CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Moves a dashboard to another folder.
    fn move_to(
        &self,
        uid: &Uid,
        folder: &FolderId,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Deletes a dashboard and writes a tombstone version.
    fn delete(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Restores a version as the current document. It reads the version and validates it before it takes a lock of
    /// the store, then it calls `restore_valid`.
    fn restore(
        &self,
        uid: &Uid,
        version: u64,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Lists the version metadata of a uid, newest first.
    fn history(
        &self,
        uid: &Uid,
        before: Option<u64>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<VersionMeta>, StoreError>> + Send;
    /// Reads one version with its bytes.
    fn version(&self, uid: &Uid, version: u64) -> impl Future<Output = Result<Version, StoreError>> + Send;
    /// Makes a folder.
    fn create_folder(
        &self,
        id: &FolderId,
        title: &crate::types::FolderTitle,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Changes the title of a folder.
    fn rename_folder(
        &self,
        id: &FolderId,
        title: &crate::types::FolderTitle,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Deletes an empty folder.
    fn delete_folder(&self, id: &FolderId) -> impl Future<Output = Result<(), StoreError>> + Send;
    /// Begins the export of a consistent copy.
    fn export_begin(&self) -> impl Future<Output = Result<Exporter, StoreError>> + Send;
    /// Begins the import into an empty store.
    fn import_begin(&self) -> impl Future<Output = Result<Importer, StoreError>> + Send;

    // Extension methods of this crate (not in the 15 methods of 5.3).

    /// Restores a version that the caller read and validated (5.4, item 4). Under its lock, the store reads the
    /// metadata of the version again (`VersionNotFound`, `NotRestorable`), then does the save or the create of a
    /// restore with the `If-Match` and folder checks. It never parses or validates the document.
    fn restore_valid(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;
    /// Reads the state of one uid for the catalog, with the marks of its files.
    fn entry_probe(&self, uid: &Uid) -> impl Future<Output = Result<EntryProbe, StoreError>> + Send;
    /// Reads all folders with their marks.
    fn folder_probe(&self) -> impl Future<Output = Result<Vec<(FolderState, FolderMark)>, StoreError>> + Send;
    /// Finds the problems that stop a copy, an export or an import (5.37 item 4, 5.38 item 4,
    /// 5.39 item 2). An empty list means no problem.
    fn check_consistency(&self) -> impl Future<Output = Result<Vec<Problem>, StoreError>> + Send;
}

/// One problem that stops a copy, an export or an import.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// The folder, when the problem has one.
    pub folder: Option<FolderId>,
    /// The uid, when the problem has one.
    pub uid: Option<Uid>,
    /// A text that names the path or the version.
    pub place: String,
    /// The errors, or one error with the text of the problem.
    pub errors: Vec<ValidationError>,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.place)?;
        for e in &self.errors {
            write!(f, "\n  {e}")?;
        }
        Ok(())
    }
}

/// The store of the process (R8: an enum, no `dyn`).
#[derive(Debug, Clone)]
pub enum AnyStore {
    /// The volume backend.
    Volume(VolumeStore),
    /// The PostgreSQL backend.
    Postgres(PgStore),
}

impl AnyStore {
    /// Takes the events that the store holds for the log.
    pub fn drain_events(&self) -> Vec<StoreEvent> {
        match self {
            AnyStore::Volume(s) => s.drain_events(),
            AnyStore::Postgres(s) => s.drain_events(),
        }
    }
    /// The name of the backend for the log: `volume` or `postgres`.
    pub fn backend_name(&self) -> &'static str {
        match self {
            AnyStore::Volume(_) => "volume",
            AnyStore::Postgres(_) => "postgres",
        }
    }
    /// The period of the poll that the options of the backend give.
    pub fn poll_interval(&self) -> std::time::Duration {
        match self {
            AnyStore::Volume(s) => s.rescan_interval(),
            AnyStore::Postgres(s) => s.poll_interval(),
        }
    }
    /// The number of versions that the store validated for a restore since it opened (tests only, 6.18).
    #[doc(hidden)]
    pub fn restore_checks(&self) -> u64 {
        match self {
            AnyStore::Volume(s) => s.restore_checks(),
            AnyStore::Postgres(s) => s.restore_checks(),
        }
    }
}

impl From<VolumeStore> for AnyStore {
    fn from(s: VolumeStore) -> Self {
        AnyStore::Volume(s)
    }
}

impl From<PgStore> for AnyStore {
    fn from(s: PgStore) -> Self {
        AnyStore::Postgres(s)
    }
}

impl Store for AnyStore {
    async fn load(&self) -> Result<Snapshot, StoreError> {
        match self {
            AnyStore::Volume(s) => s.load().await,
            AnyStore::Postgres(s) => s.load().await,
        }
    }
    async fn poll(&self, marks: &Marks) -> Result<Delta, StoreError> {
        match self {
            AnyStore::Volume(s) => s.poll(marks).await,
            AnyStore::Postgres(s) => s.poll(marks).await,
        }
    }
    async fn get(&self, uid: &Uid) -> Result<Current, StoreError> {
        match self {
            AnyStore::Volume(s) => s.get(uid).await,
            AnyStore::Postgres(s) => s.get(uid).await,
        }
    }
    async fn raw_current(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        match self {
            AnyStore::Volume(s) => s.raw_current(uid, folder).await,
            AnyStore::Postgres(s) => s.raw_current(uid, folder).await,
        }
    }
    async fn create(
        &self,
        folder: &FolderId,
        doc: &crate::types::CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.create(folder, doc, kind, meta).await,
            AnyStore::Postgres(s) => s.create(folder, doc, kind, meta).await,
        }
    }
    async fn save(
        &self,
        doc: &crate::types::CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.save(doc, if_match, expected_folder, kind, meta).await,
            AnyStore::Postgres(s) => s.save(doc, if_match, expected_folder, kind, meta).await,
        }
    }
    async fn move_to(
        &self,
        uid: &Uid,
        folder: &FolderId,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.move_to(uid, folder, if_match, expected_folder, meta).await,
            AnyStore::Postgres(s) => s.move_to(uid, folder, if_match, expected_folder, meta).await,
        }
    }
    async fn delete(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.delete(uid, if_match, expected_folder, meta).await,
            AnyStore::Postgres(s) => s.delete(uid, if_match, expected_folder, meta).await,
        }
    }
    async fn restore(
        &self,
        uid: &Uid,
        version: u64,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.restore(uid, version, if_match, expected_folder, meta).await,
            AnyStore::Postgres(s) => s.restore(uid, version, if_match, expected_folder, meta).await,
        }
    }
    async fn history(&self, uid: &Uid, before: Option<u64>, limit: u32) -> Result<Vec<VersionMeta>, StoreError> {
        match self {
            AnyStore::Volume(s) => s.history(uid, before, limit).await,
            AnyStore::Postgres(s) => s.history(uid, before, limit).await,
        }
    }
    async fn version(&self, uid: &Uid, version: u64) -> Result<Version, StoreError> {
        match self {
            AnyStore::Volume(s) => s.version(uid, version).await,
            AnyStore::Postgres(s) => s.version(uid, version).await,
        }
    }
    async fn create_folder(&self, id: &FolderId, title: &crate::types::FolderTitle) -> Result<(), StoreError> {
        match self {
            AnyStore::Volume(s) => s.create_folder(id, title).await,
            AnyStore::Postgres(s) => s.create_folder(id, title).await,
        }
    }
    async fn rename_folder(&self, id: &FolderId, title: &crate::types::FolderTitle) -> Result<(), StoreError> {
        match self {
            AnyStore::Volume(s) => s.rename_folder(id, title).await,
            AnyStore::Postgres(s) => s.rename_folder(id, title).await,
        }
    }
    async fn delete_folder(&self, id: &FolderId) -> Result<(), StoreError> {
        match self {
            AnyStore::Volume(s) => s.delete_folder(id).await,
            AnyStore::Postgres(s) => s.delete_folder(id).await,
        }
    }
    async fn export_begin(&self) -> Result<Exporter, StoreError> {
        match self {
            AnyStore::Volume(s) => s.export_begin().await,
            AnyStore::Postgres(s) => s.export_begin().await,
        }
    }
    async fn import_begin(&self) -> Result<Importer, StoreError> {
        match self {
            AnyStore::Volume(s) => s.import_begin().await,
            AnyStore::Postgres(s) => s.import_begin().await,
        }
    }
    async fn restore_valid(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        match self {
            AnyStore::Volume(s) => s.restore_valid(version, if_match, expected_folder, meta).await,
            AnyStore::Postgres(s) => s.restore_valid(version, if_match, expected_folder, meta).await,
        }
    }
    async fn entry_probe(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        match self {
            AnyStore::Volume(s) => s.entry_probe(uid).await,
            AnyStore::Postgres(s) => s.entry_probe(uid).await,
        }
    }
    async fn folder_probe(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        match self {
            AnyStore::Volume(s) => s.folder_probe().await,
            AnyStore::Postgres(s) => s.folder_probe().await,
        }
    }
    async fn check_consistency(&self) -> Result<Vec<Problem>, StoreError> {
        match self {
            AnyStore::Volume(s) => s.check_consistency().await,
            AnyStore::Postgres(s) => s.check_consistency().await,
        }
    }
}
