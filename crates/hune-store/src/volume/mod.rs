//! The volume backend (5.9 to 5.22): dashboards as files in a directory tree.
//!
//! Only this module creates, writes, renames or removes files and directories (R4).

mod dir;
mod faults;
mod meta;
mod read;
#[cfg(test)]
mod restore_tests;
mod transfer;
mod write;

#[cfg(any(test, feature = "conformance"))]
pub mod testing;

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hune_core::time::UtcMillis;
use rustix::fd::OwnedFd;
use rustix::fs::{FlockOperation, flock, ftruncate};
use rustix::io::Errno;
use tokio::sync::{OwnedMutexGuard, Semaphore};

use self::dir::{Dir, FType, is_temp_name};
pub use self::faults::FaultPoint;
use self::faults::Faults as FaultsHandle;
#[cfg(any(test, feature = "conformance"))]
pub use self::faults::{FaultAction, Faults};
use self::meta::{VersionFile, parse_version_name};
pub use self::transfer::{DirTarget, VolumeExporter, VolumeImporter};
use crate::store::{
    Delta, EntryProbe, Exporter, FolderMark, FolderState, Importer, Marks, Problem, Snapshot, StoreEvent,
};
use crate::types::{
    CanonicalDocument, Current, Etag, FolderId, FolderTitle, Saved, StoreError, Uid, ValidVersion, Version,
    VersionKind, VersionMeta, WriteMeta,
};

/// The number of permits for the file work that blocks a thread (3.7).
const IO_PERMITS: usize = 8;
/// The largest number of events that the store holds before the server takes them.
const MAX_EVENTS: usize = 10_000;
/// The largest size of a folder file or a metadata file that the backend reads.
pub(crate) const MAX_SMALL_FILE: usize = 65_536;
/// The names of the two reserved directories (5.9).
pub(crate) const HUNE_DIR: &str = ".hune";
pub(crate) const HISTORY_DIR: &str = ".history";
pub(crate) const FOLDER_FILE: &str = "_folder.json";

/// The options of the volume backend (13.10).
#[derive(Debug, Clone)]
pub struct VolumeOptions {
    /// `storage.volume.path`: absolute and canonical.
    pub root: PathBuf,
    /// `storage.volume.read_only`.
    pub read_only: bool,
    /// `storage.volume.rescan_interval`.
    pub rescan_interval: Duration,
    /// `storage.volume.history_keep`.
    pub history_keep: u32,
}

/// The cause of a start error of the volume backend (5.21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenErrorKind {
    /// The path breaks a rule of 5.12, item 1.
    PathRule,
    /// The root does not exist.
    RootMissing,
    /// The root is not a directory.
    RootNotDirectory,
    /// The process cannot read the root.
    RootNotReadable,
    /// Others can write the root (5.20, item 3).
    RootWritableByOthers,
    /// `.hune` or `.history` exists and is not a directory.
    ReservedNotDirectory,
    /// A writer cannot write the root.
    RootNotWritable,
    /// Another process holds the lock (5.16).
    LockHeld,
    /// Another I/O error.
    Io,
}

/// A start error of the volume backend. The message names the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenError {
    /// The cause.
    pub kind: OpenErrorKind,
    /// The path that the message names.
    pub path: PathBuf,
    /// The message for the operator.
    pub message: String,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OpenError {}

impl From<OpenError> for StoreError {
    fn from(e: OpenError) -> Self {
        StoreError::Unavailable(e.message)
    }
}

fn open_error(kind: OpenErrorKind, path: &Path, what: impl fmt::Display) -> OpenError {
    OpenError { kind, path: path.to_owned(), message: format!("{}: {what}", path.display()) }
}

/// The volume backend. A clone shares the state.
#[derive(Clone)]
pub struct VolumeStore {
    pub(crate) inner: Arc<Inner>,
}

impl fmt::Debug for VolumeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VolumeStore")
            .field("root", &self.inner.root_path)
            .field("read_only", &self.inner.opts.read_only)
            .finish()
    }
}

struct IgnoredState {
    stamp: u64,
    scan: bool,
    seen: bool,
}

/// The shared state of one volume store.
pub(crate) struct Inner {
    pub root: Dir,
    pub root_path: PathBuf,
    pub opts: VolumeOptions,
    _lock: Option<OwnedFd>,
    pub store_lock: Arc<tokio::sync::RwLock<()>>,
    uid_locks: Mutex<HashMap<Uid, Arc<tokio::sync::Mutex<()>>>>,
    io: Arc<Semaphore>,
    events: Mutex<VecDeque<StoreEvent>>,
    ignored: Mutex<HashMap<String, IgnoredState>>,
    recent: Mutex<HashMap<Uid, Etag>>,
    /// The number of versions that this store validated for a restore (see `VolumeStore::restore_checks`).
    restore_checks: std::sync::atomic::AtomicU64,
    pub faults: FaultsHandle,
}

impl Inner {
    /// Counts one validation of a version for a restore.
    pub(crate) fn note_restore_check(&self) {
        self.restore_checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn push_event(&self, e: StoreEvent) {
        if let Ok(mut q) = self.events.lock() {
            if q.len() >= MAX_EVENTS {
                q.pop_front();
            }
            q.push_back(e);
        }
    }

    /// Writes the event `store.entry_ignored` one time for each entry, and again after the
    /// entry changes (`stamp` changes). `scan` entries leave the table when a scan no longer
    /// sees them.
    pub(crate) fn note_ignored(&self, path: String, reason: &str, stamp: u64, scan: bool) {
        let fire = {
            let Ok(mut map) = self.ignored.lock() else { return };
            if map.len() > 100_000 {
                map.clear();
            }
            match map.get_mut(&path) {
                Some(st) if st.stamp == stamp => {
                    st.seen = true;
                    false
                }
                _ => {
                    map.insert(path.clone(), IgnoredState { stamp, scan, seen: true });
                    true
                }
            }
        };
        if fire {
            self.push_event(StoreEvent::EntryIgnored { path, reason: reason.to_owned() });
        }
    }

    pub(crate) fn begin_scan(&self) {
        if let Ok(mut map) = self.ignored.lock() {
            for st in map.values_mut().filter(|s| s.scan) {
                st.seen = false;
            }
        }
    }

    pub(crate) fn end_scan(&self) {
        if let Ok(mut map) = self.ignored.lock() {
            map.retain(|_, st| !st.scan || st.seen);
        }
    }

    /// Remembers the etag of an own write, so that a scan does not call it an outside change.
    pub(crate) fn note_own_write(&self, uid: &Uid, etag: Option<Etag>) {
        if let Ok(mut m) = self.recent.lock() {
            if m.len() > MAX_EVENTS {
                m.clear();
            }
            match etag {
                Some(e) => {
                    m.insert(uid.clone(), e);
                }
                None => {
                    m.remove(uid);
                }
            }
        }
    }

    pub(crate) fn is_own_write(&self, uid: &Uid, etag: &Etag) -> bool {
        self.recent.lock().map(|m| m.get(uid) == Some(etag)).unwrap_or(false)
    }

    fn uid_lock(&self, uid: &Uid) -> Arc<tokio::sync::Mutex<()>> {
        match self.uid_locks.lock() {
            Ok(mut m) => m.entry(uid.clone()).or_default().clone(),
            Err(_) => Arc::default(),
        }
    }

    /// Drops the locks of the uids that nobody holds or waits for.
    fn tidy_uid_locks(&self) {
        if let Ok(mut m) = self.uid_locks.lock()
            && m.len() > 1024
        {
            m.retain(|_, l| Arc::strong_count(l) > 1);
        }
    }

    pub(crate) fn check_writable(&self) -> Result<(), StoreError> {
        if self.opts.read_only { Err(StoreError::ReadOnly) } else { Ok(()) }
    }
}

/// Maps an error number to a store error (5.13): `ENOSPC` and `EDQUOT` give `Full`.
pub(crate) fn io_err(e: Errno, what: impl fmt::Display) -> StoreError {
    if e == Errno::NOSPC || e == Errno::DQUOT {
        StoreError::Full
    } else {
        StoreError::Unavailable(format!("{what}: {e}"))
    }
}

impl VolumeStore {
    /// Opens the volume: the start checks of 5.21 and, for a writer, the lock of 5.16.
    pub async fn open(opts: VolumeOptions) -> Result<VolumeStore, OpenError> {
        Self::open_inner(opts, FaultsHandle::new()).await
    }

    /// Opens the volume with a fault handle (tests only).
    #[cfg(any(test, feature = "conformance"))]
    pub async fn open_with_faults(opts: VolumeOptions, faults: Faults) -> Result<VolumeStore, OpenError> {
        Self::open_inner(opts, faults).await
    }

    async fn open_inner(opts: VolumeOptions, faults: FaultsHandle) -> Result<VolumeStore, OpenError> {
        let path = opts.root.clone();
        let inner = tokio::task::spawn_blocking(move || open_sync(opts, faults))
            .await
            .map_err(|e| open_error(OpenErrorKind::Io, &path, format!("start task failed: {e}")))??;
        Ok(VolumeStore { inner: Arc::new(inner) })
    }

    /// The fault handle of this store (tests only).
    #[cfg(any(test, feature = "conformance"))]
    pub fn faults(&self) -> Faults {
        self.inner.faults.clone()
    }

    /// The root path.
    pub fn root(&self) -> &Path {
        &self.inner.root_path
    }

    /// The options of the store.
    pub fn options(&self) -> &VolumeOptions {
        &self.inner.opts
    }

    /// The period of the poll.
    pub fn rescan_interval(&self) -> Duration {
        self.inner.opts.rescan_interval
    }

    /// The number of versions that this store validated for a restore since it opened. The tests read it: the
    /// restore of the API validates its version one time, before the store (6.18).
    #[doc(hidden)]
    pub fn restore_checks(&self) -> u64 {
        self.inner.restore_checks.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Takes the events that the store holds for the log.
    pub fn drain_events(&self) -> Vec<StoreEvent> {
        match self.inner.events.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Runs file work that blocks a thread. It uses `spawn_blocking` behind the semaphore of 8
    /// permits (3.7). The guard (store lock and uid lock) moves into the task. Thus the locks
    /// stay held when the caller drops the future.
    pub(crate) async fn run<T, G>(
        &self,
        guard: G,
        f: impl FnOnce(&Inner) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError>
    where
        T: Send + 'static,
        G: Send + 'static,
    {
        let permit = self
            .inner
            .io
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| StoreError::Unavailable("file work semaphore closed".to_owned()))?;
        let inner = self.inner.clone();
        let out = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _guard = guard;
            f(&inner)
        })
        .await
        .map_err(|e| StoreError::Unavailable(format!("file work task failed: {e}")))?;
        self.inner.tidy_uid_locks();
        out
    }

    /// Takes the store lock in shared mode, then the lock of the uid (5.16).
    pub(crate) async fn lock_shared(&self, uid: &Uid) -> (tokio::sync::OwnedRwLockReadGuard<()>, OwnedMutexGuard<()>) {
        let s = self.inner.store_lock.clone().read_owned().await;
        let u = self.inner.uid_lock(uid).lock_owned().await;
        (s, u)
    }

    /// Takes the store lock in exclusive mode, then the lock of the uid (5.16).
    pub(crate) async fn lock_exclusive(
        &self,
        uid: &Uid,
    ) -> (tokio::sync::OwnedRwLockWriteGuard<()>, OwnedMutexGuard<()>) {
        let s = self.inner.store_lock.clone().write_owned().await;
        let u = self.inner.uid_lock(uid).lock_owned().await;
        (s, u)
    }

    pub(crate) async fn lock_store_exclusive(&self) -> tokio::sync::OwnedRwLockWriteGuard<()> {
        self.inner.store_lock.clone().write_owned().await
    }

    pub(crate) async fn lock_store_shared(&self) -> tokio::sync::OwnedRwLockReadGuard<()> {
        self.inner.store_lock.clone().read_owned().await
    }

    // The methods of the trait `Store`.

    pub(crate) async fn load(&self) -> Result<Snapshot, StoreError> {
        let g = self.lock_store_shared().await;
        let delta = self.run(g, |i| i.scan(&Marks::empty(), false)).await?;
        let mut folders = delta.folders;
        folders.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(Snapshot { folders, entries: delta.entries, marks: delta.marks })
    }

    pub(crate) async fn poll(&self, marks: &Marks) -> Result<Delta, StoreError> {
        let g = self.lock_store_shared().await;
        let marks = marks.clone();
        self.run(g, move |i| i.scan(&marks, true)).await
    }

    pub(crate) async fn get(&self, uid: &Uid) -> Result<Current, StoreError> {
        let uid = uid.clone();
        self.run((), move |i| i.get_current(&uid)).await
    }

    pub(crate) async fn raw_current(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        let (uid, folder) = (uid.clone(), folder.clone());
        self.run((), move |i| i.raw_current_sync(&uid, &folder)).await
    }

    pub(crate) async fn create(
        &self,
        folder: &FolderId,
        doc: &CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        let uid = doc.uid().clone();
        let g = self.lock_exclusive(&uid).await;
        let req = write::NewDoc::of(doc);
        let (folder, meta) = (folder.clone(), meta.clone());
        self.run(g, move |i| i.create_sync(&folder, &req, kind, &meta)).await
    }

    pub(crate) async fn save(
        &self,
        doc: &CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        let uid = doc.uid().clone();
        let g = self.lock_shared(&uid).await;
        let req = write::NewDoc::of(doc);
        let (if_match, expected, meta) = (*if_match, expected_folder.clone(), meta.clone());
        self.run(g, move |i| i.save_sync(&req, &if_match, &expected, kind, &meta)).await
    }

    pub(crate) async fn move_to(
        &self,
        uid: &Uid,
        folder: &FolderId,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        let g = self.lock_shared(uid).await;
        let (uid, target, if_match, expected, meta) =
            (uid.clone(), folder.clone(), *if_match, expected_folder.clone(), meta.clone());
        self.run(g, move |i| i.move_sync(&uid, &target, &if_match, &expected, &meta)).await
    }

    pub(crate) async fn delete(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        let g = self.lock_shared(uid).await;
        let (uid, if_match, expected, meta) = (uid.clone(), *if_match, expected_folder.clone(), meta.clone());
        self.run(g, move |i| i.delete_sync(&uid, &if_match, &expected, &meta)).await
    }

    pub(crate) async fn restore(
        &self,
        uid: &Uid,
        version: u64,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        // The read and the validation take no lock of the store: only `restore_valid` takes it (5.14).
        let read = self.version(uid, version).await?;
        let owned = uid.clone();
        let checked = self
            .run((), move |i| {
                i.note_restore_check();
                ValidVersion::check(&owned, &read)
            })
            .await?;
        self.restore_valid(&checked, if_match, expected_folder, meta).await
    }

    pub(crate) async fn restore_valid(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.inner.check_writable()?;
        let req = write::NewDoc::of(version.document());
        // A restore of a deleted dashboard is a create: it takes the exclusive store lock. The work under the lock
        // reads the metadata of the version and the current file. It does not validate the version (6.18).
        let g = self.lock_exclusive(&req.uid).await;
        let (n, if_match, expected, meta) =
            (version.number(), if_match.copied(), expected_folder.cloned(), meta.clone());
        self.run(g, move |i| i.restore_sync(&req, n, if_match.as_ref(), expected.as_ref(), &meta)).await
    }

    pub(crate) async fn history(
        &self,
        uid: &Uid,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<VersionMeta>, StoreError> {
        let uid = uid.clone();
        self.run((), move |i| i.history_sync(&uid, before, limit)).await
    }

    pub(crate) async fn version(&self, uid: &Uid, version: u64) -> Result<Version, StoreError> {
        let uid = uid.clone();
        self.run((), move |i| i.version_sync(&uid, version)).await
    }

    pub(crate) async fn create_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        self.inner.check_writable()?;
        let g = self.lock_store_exclusive().await;
        let (id, title) = (id.clone(), title.clone());
        self.run(g, move |i| i.create_folder_sync(&id, &title)).await
    }

    pub(crate) async fn rename_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        self.inner.check_writable()?;
        let g = self.lock_store_exclusive().await;
        let (id, title) = (id.clone(), title.clone());
        self.run(g, move |i| i.rename_folder_sync(&id, &title)).await
    }

    pub(crate) async fn delete_folder(&self, id: &FolderId) -> Result<(), StoreError> {
        self.inner.check_writable()?;
        let g = self.lock_store_exclusive().await;
        let id = id.clone();
        self.run(g, move |i| i.delete_folder_sync(&id)).await
    }

    pub(crate) async fn export_begin(&self) -> Result<Exporter, StoreError> {
        Ok(Exporter::Volume(VolumeExporter::begin(self).await?))
    }

    pub(crate) async fn import_begin(&self) -> Result<Importer, StoreError> {
        Ok(Importer::Volume(VolumeImporter::begin(self).await?))
    }

    pub(crate) async fn entry_probe(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        let g = self.lock_store_shared().await;
        let uid = uid.clone();
        self.run(g, move |i| i.entry_probe_sync(&uid)).await
    }

    pub(crate) async fn folder_probe(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        let g = self.lock_store_shared().await;
        self.run(g, |i| i.folder_probe_sync()).await
    }

    pub(crate) async fn check_consistency(&self) -> Result<Vec<Problem>, StoreError> {
        let g = self.lock_store_shared().await;
        self.run(g, |i| i.check_sync()).await
    }
}

// The start of the volume.

/// Checks the rules of 5.12, item 1 on the text of the path.
fn check_path_rules(path: &Path) -> Result<(), (OpenErrorKind, String)> {
    let rule = |m: String| (OpenErrorKind::PathRule, m);
    check_path_text(path).map_err(rule)?;
    match std::fs::canonicalize(path) {
        Ok(real) if real == path => Ok(()),
        Ok(real) => Err(rule(format!("the canonical path is {}: a component is a symbolic link", real.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err((OpenErrorKind::RootMissing, "the root does not exist".to_owned()))
        }
        Err(e) => Err(rule(format!("the path cannot be resolved: {e}"))),
    }
}

fn check_path_text(path: &Path) -> Result<(), String> {
    let text = path.to_str().ok_or("the path is not UTF-8")?;
    if !text.starts_with('/') {
        return Err("the path MUST be absolute".to_owned());
    }
    if text.len() > 4096 {
        return Err("the path has more than 4096 bytes".to_owned());
    }
    if text == "/" {
        return Err("the root of the file system cannot be a volume root".to_owned());
    }
    if text.ends_with('/') {
        return Err("the path MUST NOT end with /".to_owned());
    }
    for comp in text[1..].split('/') {
        if comp.is_empty() || comp == "." || comp == ".." {
            return Err(format!("the path has the component {comp:?}"));
        }
    }
    Ok(())
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn now_millis() -> UtcMillis {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    UtcMillis::from_millis(ms)
}

fn check_reserved(root: &Dir, path: &Path, name: &str) -> Result<(), OpenError> {
    match root.stat(name) {
        Ok(st) if st.ftype != FType::Dir => Err(open_error(
            OpenErrorKind::ReservedNotDirectory,
            &path.join(name),
            format!("{name} MUST be a directory, found a {}", st.ftype.describe()),
        )),
        Ok(_) | Err(Errno::NOENT) => Ok(()),
        Err(e) => Err(open_error(OpenErrorKind::Io, &path.join(name), e)),
    }
}

fn open_sync(opts: VolumeOptions, faults: FaultsHandle) -> Result<Inner, OpenError> {
    use OpenErrorKind as K;
    let path = opts.root.clone();
    check_path_rules(&path).map_err(|(kind, m)| open_error(kind, &path, m))?;
    let root = Dir::open_root(&path, faults.clone()).map_err(|e| {
        let kind = match e {
            Errno::NOENT => K::RootMissing,
            Errno::NOTDIR | Errno::LOOP => K::RootNotDirectory,
            Errno::ACCESS | Errno::PERM => K::RootNotReadable,
            _ => K::Io,
        };
        open_error(kind, &path, format!("the root cannot be opened: {e}"))
    })?;
    let st = root.stat_self().map_err(|e| open_error(K::Io, &path, e))?;
    if st.mode & 0o002 != 0 {
        return Err(open_error(
            K::RootWritableByOthers,
            &path,
            format!("others can write the root (mode {:04o})", st.mode),
        ));
    }
    check_reserved(&root, &path, HUNE_DIR)?;
    check_reserved(&root, &path, HISTORY_DIR)?;
    let mut lock_fd = None;
    let mut early_events = Vec::new();
    if !opts.read_only {
        let hune_path = path.join(HUNE_DIR);
        match root.mkdir(HUNE_DIR) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(e) => {
                return Err(open_error(K::RootNotWritable, &hune_path, format!("cannot make the directory: {e}")));
            }
        }
        let hune = root.open_dir(HUNE_DIR).map_err(|e| open_error(K::Io, &hune_path, e))?;
        lock_fd = take_lock(&hune, &path, &faults, &mut early_events)?;
        hune.probe().map_err(|e| open_error(K::RootNotWritable, &hune_path, format!("the write probe failed: {e}")))?;
        match root.mkdir(HISTORY_DIR) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(e) => {
                return Err(open_error(
                    K::RootNotWritable,
                    &path.join(HISTORY_DIR),
                    format!("cannot make the directory: {e}"),
                ));
            }
        }
        check_reserved(&root, &path, HISTORY_DIR)?;
        cleanup(&root, &hune).map_err(|e| open_error(K::Io, &path, format!("the start clean-up failed: {e}")))?;
    }
    let inner = Inner {
        root,
        root_path: path,
        opts,
        _lock: lock_fd,
        store_lock: Arc::new(tokio::sync::RwLock::new(())),
        uid_locks: Mutex::default(),
        io: Arc::new(Semaphore::new(IO_PERMITS)),
        events: Mutex::default(),
        ignored: Mutex::default(),
        recent: Mutex::default(),
        restore_checks: std::sync::atomic::AtomicU64::new(0),
        faults,
    };
    for e in early_events {
        inner.push_event(e);
    }
    Ok(inner)
}

/// Takes the lock of 5.16. Gives the descriptor that holds it, or `None` when the file
/// system has no `flock`.
fn take_lock(
    hune: &Dir,
    root: &Path,
    faults: &FaultsHandle,
    events: &mut Vec<StoreEvent>,
) -> Result<Option<OwnedFd>, OpenError> {
    use OpenErrorKind as K;
    let lock_path = root.join(HUNE_DIR).join("lock");
    let fd = hune
        .open_lock_file("lock")
        .map_err(|e| open_error(K::Io, &lock_path, format!("cannot open the lock file: {e}")))?;
    let result = faults
        .check(self::faults::FaultPoint::Flock)
        .and_then(|()| flock(&fd, FlockOperation::NonBlockingLockExclusive));
    match result {
        Ok(()) => {}
        Err(Errno::WOULDBLOCK) => {
            let holder = hune
                .read_file("lock", 4096)
                .map(|(b, _)| String::from_utf8_lossy(&b).trim().to_owned())
                .unwrap_or_default();
            return Err(open_error(
                K::LockHeld,
                &lock_path,
                format!("another process holds the volume lock; the lock file says: {holder:?}"),
            ));
        }
        Err(e @ (Errno::NOLCK | Errno::OPNOTSUPP | Errno::INVAL)) => {
            events.push(StoreEvent::LockUnsupported { path: lock_path.display().to_string(), errno: e.raw_os_error() });
            return Ok(None);
        }
        Err(e) => return Err(open_error(K::Io, &lock_path, format!("flock failed: {e}"))),
    }
    let holder = format!("{} {} {}\n", hostname(), std::process::id(), now_millis().as_millis());
    let write = ftruncate(&fd, 0)
        .and_then(|()| rustix::io::write(&fd, holder.as_bytes()).map(|_| ()))
        .and_then(|()| rustix::fs::fsync(&fd));
    write.map_err(|e| open_error(K::Io, &lock_path, format!("cannot write the holder text: {e}")))?;
    Ok(Some(fd))
}

/// The clean-up after the lock (5.21): temporary files, and each `<n>.json` without its
/// metadata file.
fn cleanup(root: &Dir, hune: &Dir) -> Result<(), Errno> {
    for item in hune.list()? {
        if item.ftype == FType::File && item.name.starts_with("probe-") {
            hune.unlink_quiet(&item.name);
        }
    }
    for item in root.list()? {
        if item.ftype == FType::File && is_temp_name(&item.name) {
            root.unlink_quiet(&item.name);
        }
        if item.ftype == FType::Dir && item.name_ok && hune_core::ids::is_valid_id(&item.name) {
            let Ok(d) = root.open_dir(&item.name) else { continue };
            for f in d.list()? {
                if f.ftype == FType::File && is_temp_name(&f.name) {
                    d.unlink_quiet(&f.name);
                }
            }
        }
    }
    let Ok(history) = root.open_dir(HISTORY_DIR) else { return Ok(()) };
    for item in history.list()? {
        if item.ftype == FType::File && is_temp_name(&item.name) {
            history.unlink_quiet(&item.name);
        }
        if item.ftype != FType::Dir || !item.name_ok || !hune_core::ids::is_valid_id(&item.name) {
            continue;
        }
        let Ok(d) = history.open_dir(&item.name) else { continue };
        let names = d.list()?;
        let metas: std::collections::HashSet<u64> = names
            .iter()
            .filter_map(|f| match parse_version_name(&f.name) {
                Some((n, VersionFile::Meta)) => Some(n),
                _ => None,
            })
            .collect();
        for f in names {
            if f.ftype != FType::File {
                continue;
            }
            if is_temp_name(&f.name) {
                d.unlink_quiet(&f.name);
            } else if let Some((n, VersionFile::Json)) = parse_version_name(&f.name)
                && !metas.contains(&n)
            {
                d.unlink_quiet(&f.name);
            }
        }
    }
    Ok(())
}
