// SPDX-License-Identifier: MIT OR Apache-2.0
//! Export, import and the checks of a layout (5.37 to 5.39). This module writes the volume
//! layout of 5.9 to 5.11 into an empty directory (R4).

use std::collections::BTreeSet;
use std::path::Path;

use hune_core::document::{folder_file_bytes, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::MAX_DOCUMENT_BYTES;
use rustix::fs::{self, Mode, OFlags};
use rustix::io::Errno;
use tokio::sync::OwnedRwLockWriteGuard;

use super::dir::{Dir, FILE_MODE, FType};
use super::meta::{VersionFile, decode_meta, encode_meta, json_name, meta_name, parse_version_name};
use super::{FOLDER_FILE, HISTORY_DIR, Inner, MAX_SMALL_FILE, VolumeStore, io_err};
use crate::store::{CurrentRecord, DashboardRecord, FolderState, ImportReport, Marks, Problem};
use crate::types::{CatalogEntry, Etag, FolderRecord, StoreError, Uid, Version, VersionKind};

fn invalid_one(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(vec![ValidationError::new("", ErrorCode::NotPermitted, message)])
}

impl Dir {
    /// Creates a file with `O_EXCL` and writes the bytes. No temporary file, no `fsync`: the
    /// caller calls `sync_all` at the end of a bulk write.
    pub(crate) fn create_new(&self, name: &str, bytes: &[u8]) -> Result<(), Errno> {
        let fd = fs::openat(
            self.raw_fd(),
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(FILE_MODE),
        )?;
        let mut off = 0;
        while off < bytes.len() {
            match rustix::io::write(&fd, &bytes[off..]) {
                Ok(0) => return Err(Errno::IO),
                Ok(n) => off += n,
                Err(Errno::INTR) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Writes the layout of 5.9 to 5.11 into a directory, one folder or one uid at a time.
#[derive(Clone)]
pub(crate) struct LayoutWriter {
    root: Dir,
}

impl LayoutWriter {
    pub(crate) fn new(root: Dir) -> Self {
        LayoutWriter { root }
    }

    pub(crate) fn write_folder(&self, rec: &FolderRecord) -> Result<(), StoreError> {
        match self.root.mkdir(rec.id.as_str()) {
            Ok(()) => {}
            Err(Errno::EXIST) => return Err(StoreError::Exists { folder: None }),
            Err(e) => return Err(io_err(e, format!("make the folder {}", rec.id))),
        }
        let dir = self.root.open_dir(rec.id.as_str()).map_err(|e| io_err(e, format!("open the folder {}", rec.id)))?;
        dir.create_new(FOLDER_FILE, &folder_file_bytes(rec.title.as_str()))
            .map_err(|e| io_err(e, format!("write {}/{FOLDER_FILE}", rec.id)))
    }

    /// Checks one record before any byte goes out (5.39, item 2).
    fn check_record(&self, rec: &DashboardRecord) -> Result<(), StoreError> {
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
                (Some(b), Some(e)) if Etag::of(b) == e => {}
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

    /// Writes the versions from the lowest number to the highest, then the current document (5.39, item 6).
    pub(crate) fn write_dashboard(&self, rec: &DashboardRecord) -> Result<u64, StoreError> {
        self.check_record(rec)?;
        let uid = &rec.uid;
        if !rec.versions.is_empty() {
            match self.root.mkdir(HISTORY_DIR) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(e) => return Err(io_err(e, "make .history")),
            }
            let history = self.root.open_dir(HISTORY_DIR).map_err(|e| io_err(e, "open .history"))?;
            match history.mkdir(uid.as_str()) {
                Ok(()) => {}
                Err(Errno::EXIST) => return Err(StoreError::Exists { folder: None }),
                Err(e) => return Err(io_err(e, format!("make .history/{uid}"))),
            }
            let dir = history.open_dir(uid.as_str()).map_err(|e| io_err(e, format!("open .history/{uid}")))?;
            for v in &rec.versions {
                if let Some(b) = &v.bytes {
                    dir.create_new(&json_name(v.meta.version), b)
                        .map_err(|e| io_err(e, format!("write .history/{uid}/{}", json_name(v.meta.version))))?;
                }
                dir.create_new(&meta_name(v.meta.version), &encode_meta(&v.meta))
                    .map_err(|e| io_err(e, format!("write .history/{uid}/{}", meta_name(v.meta.version))))?;
            }
        }
        if let Some(cur) = &rec.current {
            let dir = match self.root.open_dir(cur.folder.as_str()) {
                Ok(d) => d,
                Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return Err(StoreError::FolderNotFound),
                Err(e) => return Err(io_err(e, format!("open the folder {}", cur.folder))),
            };
            dir.create_new(&format!("{uid}.json"), &cur.bytes)
                .map_err(|e| io_err(e, format!("write {}/{uid}.json", cur.folder)))?;
        }
        Ok(rec.versions.len() as u64)
    }

    pub(crate) fn sync_all(&self) {
        fs::sync();
    }
}

/// The importer of the volume backend (5.3). It holds the store lock in exclusive mode.
#[derive(Debug)]
pub struct VolumeImporter {
    store: VolumeStore,
    _guard: OwnedRwLockWriteGuard<()>,
    report: ImportReport,
}

impl VolumeImporter {
    pub(crate) async fn begin(store: &VolumeStore) -> Result<Self, StoreError> {
        store.inner.check_writable()?;
        let guard = store.lock_store_exclusive().await;
        store
            .run((), |i| {
                if !i.folder_ids()?.is_empty() || !i.history_uids()?.is_empty() {
                    return Err(StoreError::NotEmpty);
                }
                Ok(())
            })
            .await?;
        Ok(VolumeImporter { store: store.clone(), _guard: guard, report: ImportReport::default() })
    }

    pub(crate) async fn folder(&mut self, rec: &FolderRecord) -> Result<(), StoreError> {
        let rec = rec.clone();
        self.store.run((), move |i| LayoutWriter::new(i.root.clone()).write_folder(&rec)).await?;
        self.report.folders += 1;
        Ok(())
    }

    pub(crate) async fn dashboard(&mut self, rec: &DashboardRecord) -> Result<(), StoreError> {
        let rec = rec.clone();
        let n = self.store.run((), move |i| LayoutWriter::new(i.root.clone()).write_dashboard(&rec)).await?;
        self.report.dashboards += 1;
        self.report.versions += n;
        Ok(())
    }

    pub(crate) async fn commit(self) -> Result<ImportReport, StoreError> {
        self.store
            .run((), |i| {
                LayoutWriter::new(i.root.clone()).sync_all();
                Ok(())
            })
            .await?;
        Ok(self.report)
    }
}

impl std::fmt::Debug for LayoutWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LayoutWriter")
    }
}

/// The target of `hune export`: a new or empty directory that becomes a volume layout (5.38).
#[derive(Debug)]
pub struct DirTarget {
    writer: LayoutWriter,
    report: ImportReport,
}

impl DirTarget {
    /// Opens the target. `path` MUST be absolute, absent or an empty directory, and its parent
    /// MUST exist (5.38, item 1).
    pub async fn create(path: &Path) -> Result<DirTarget, StoreError> {
        let path = path.to_owned();
        let root = tokio::task::spawn_blocking(move || create_target_dir(&path))
            .await
            .map_err(|e| StoreError::Unavailable(format!("export task failed: {e}")))??;
        Ok(DirTarget { writer: LayoutWriter::new(root), report: ImportReport::default() })
    }

    /// Writes one folder.
    pub async fn folder(&mut self, rec: &FolderRecord) -> Result<(), StoreError> {
        let (w, rec) = (self.writer.clone(), rec.clone());
        tokio::task::spawn_blocking(move || w.write_folder(&rec))
            .await
            .map_err(|e| StoreError::Unavailable(format!("export task failed: {e}")))??;
        self.report.folders += 1;
        Ok(())
    }

    /// Writes one uid with its versions and its current document.
    pub async fn dashboard(&mut self, rec: &DashboardRecord) -> Result<(), StoreError> {
        let (w, rec) = (self.writer.clone(), rec.clone());
        let n = tokio::task::spawn_blocking(move || w.write_dashboard(&rec))
            .await
            .map_err(|e| StoreError::Unavailable(format!("export task failed: {e}")))??;
        self.report.dashboards += 1;
        self.report.versions += n;
        Ok(())
    }

    /// Ends the export and gives the counts.
    pub async fn finish(self) -> Result<ImportReport, StoreError> {
        let w = self.writer.clone();
        tokio::task::spawn_blocking(move || w.sync_all())
            .await
            .map_err(|e| StoreError::Unavailable(format!("export task failed: {e}")))?;
        Ok(self.report)
    }
}

fn create_target_dir(path: &Path) -> Result<Dir, StoreError> {
    let text = path.to_str().ok_or_else(|| StoreError::Unavailable("the export path is not UTF-8".to_owned()))?;
    let bad = |m: &str| StoreError::Unavailable(format!("{text}: {m}"));
    if !text.starts_with('/') || text.len() < 2 || text.ends_with('/') {
        return Err(bad("the export directory MUST be an absolute path without a final /"));
    }
    if text[1..].split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        return Err(bad("the export path has an empty, . or .. component"));
    }
    let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| bad("the path has no name"))?;
    let parent = path.parent().ok_or_else(|| bad("the path has no parent"))?;
    let parent_dir =
        Dir::open_root(parent, Default::default()).map_err(|e| bad(&format!("the parent cannot be opened: {e}")))?;
    match parent_dir.mkdir(name) {
        Ok(()) => {}
        Err(Errno::EXIST) => {
            let dir = parent_dir.open_dir(name).map_err(|_| bad("the path exists and is not a directory"))?;
            let empty = dir.list().map_err(|e| io_err(e, "list the export directory"))?.is_empty();
            if !empty {
                return Err(StoreError::NotEmpty);
            }
            return Ok(dir);
        }
        Err(e) => return Err(io_err(e, format!("make {text}"))),
    }
    parent_dir.open_dir(name).map_err(|e| io_err(e, format!("open {text}")))
}

/// The exporter of the volume backend (5.3): a read without a lock.
#[derive(Debug)]
pub struct VolumeExporter {
    store: VolumeStore,
    folders: Vec<FolderRecord>,
}

impl VolumeExporter {
    pub(crate) async fn begin(store: &VolumeStore) -> Result<Self, StoreError> {
        let g = store.lock_store_shared().await;
        let folders = store
            .run(g, |i| {
                let problems = i.scan_problems()?;
                if !problems.is_empty() {
                    let errors = problems
                        .into_iter()
                        .flat_map(|p| {
                            let place = p.place;
                            p.errors.into_iter().map(move |mut e| {
                                e.message = format!("{place}: {}", e.message);
                                e
                            })
                        })
                        .collect();
                    return Err(StoreError::Invalid(errors));
                }
                let mut out = Vec::new();
                for f in i.folder_ids()? {
                    if let Some((FolderState::Valid(r), _)) = i.read_folder_state(&f)? {
                        out.push(r);
                    }
                }
                Ok(out)
            })
            .await?;
        Ok(VolumeExporter { store: store.clone(), folders })
    }

    pub(crate) async fn folders(&self) -> Result<Vec<FolderRecord>, StoreError> {
        Ok(self.folders.clone())
    }

    pub(crate) async fn uids(&self) -> Result<Vec<Uid>, StoreError> {
        self.store.run((), |i| i.export_uids()).await
    }

    pub(crate) async fn dashboard(&self, uid: &Uid) -> Result<DashboardRecord, StoreError> {
        let uid = uid.clone();
        self.store.run((), move |i| i.export_dashboard(&uid)).await
    }

    pub(crate) async fn end(self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl Inner {
    /// Each uid that has a current document or a version, in byte order (5.4, item 6).
    pub(crate) fn export_uids(&self) -> Result<Vec<Uid>, StoreError> {
        let mut set: BTreeSet<Uid> = self.history_uids()?.into_keys().collect();
        for f in self.folder_ids()? {
            let Some(dir) = self.open_folder(&f)? else { continue };
            for item in dir.list().map_err(|e| io_err(e, format!("list the folder {f}")))? {
                if item.ftype != FType::File || !item.name_ok || item.name.starts_with('.') {
                    continue;
                }
                set.extend(item.name.strip_suffix(".json").and_then(|s| Uid::parse(s).ok()));
            }
        }
        // `Uid` sorts by its text, which is the byte order.
        Ok(set.into_iter().collect())
    }

    /// One uid with its versions and its current document. The export lists the versions, reads
    /// them, reads the current file, then lists the versions again. A difference gives a new
    /// attempt, at most 3 times (5.38, item 3).
    pub(crate) fn export_dashboard(&self, uid: &Uid) -> Result<DashboardRecord, StoreError> {
        for _ in 0..3 {
            let before = self.version_numbers(uid)?;
            let mut versions: Vec<Version> = Vec::new();
            let mut stable = true;
            let dir = self.hist_dir(uid)?;
            for n in &before {
                let Some(d) = &dir else { break };
                if self.read_meta(d, uid, *n)?.is_none() {
                    continue;
                }
                match self.version_sync(uid, *n) {
                    Ok(v) => versions.push(v),
                    Err(StoreError::VersionNotFound) => stable = false,
                    Err(e) => return Err(e),
                }
            }
            let current = match self.locate(uid)?.as_slice() {
                [] => None,
                [f] => match self.read_dashboard(f, uid)? {
                    None => None,
                    Some(c) => match c.doc {
                        Ok(d) => Some(CurrentRecord { folder: f.clone(), bytes: d.bytes().to_vec() }),
                        Err(e) => return Err(StoreError::Invalid(e)),
                    },
                },
                _ => return Err(invalid_one(format!("the uid {uid} is in more than one folder"))),
            };
            if stable && before == self.version_numbers(uid)? {
                return Ok(DashboardRecord { uid: uid.clone(), versions, current });
            }
        }
        Err(StoreError::Unavailable(format!("the versions of {uid} change during the export")))
    }

    fn version_numbers(&self, uid: &Uid) -> Result<Vec<u64>, StoreError> {
        match self.hist_dir(uid)? {
            Some(d) => self.list_versions(uid, &d),
            None => Ok(Vec::new()),
        }
    }

    /// The invalid folders and the invalid entries (5.37 item 4, 5.38 item 4).
    pub(crate) fn scan_problems(&self) -> Result<Vec<Problem>, StoreError> {
        let delta = self.scan(&Marks::empty(), false)?;
        let mut out = Vec::new();
        for f in &delta.folders {
            if let FolderState::Invalid { id, errors } = f {
                out.push(Problem {
                    folder: Some(id.clone()),
                    uid: None,
                    place: format!("{id}/{FOLDER_FILE}"),
                    errors: errors.clone(),
                });
            }
        }
        for e in &delta.entries {
            if let CatalogEntry::Invalid { uid, folder, errors, .. } = &**e {
                out.push(Problem {
                    folder: Some(folder.clone()),
                    uid: Some(uid.clone()),
                    place: format!("{folder}/{uid}.json"),
                    errors: errors.clone(),
                });
            }
        }
        Ok(out)
    }

    /// The strict check of a layout (5.39, item 2). It gives the scan problems. It also checks
    /// each metadata file and each `<n>.json` of `.history`.
    pub(crate) fn check_sync(&self) -> Result<Vec<Problem>, StoreError> {
        let mut out = self.scan_problems()?;
        for uid in self.history_uids()?.into_keys() {
            let Some(dir) = self.hist_dir(&uid)? else { continue };
            let mut items = dir.list().map_err(|e| io_err(e, format!("list .history/{uid}")))?;
            items.sort_by(|a, b| a.name.cmp(&b.name));
            for item in items {
                let Some((n, VersionFile::Meta)) = parse_version_name(&item.name) else { continue };
                let place = format!("{HISTORY_DIR}/{uid}/{}", item.name);
                let problem = |msg: String| Problem {
                    folder: None,
                    uid: Some(uid.clone()),
                    place: place.clone(),
                    errors: vec![ValidationError::new("", ErrorCode::NotPermitted, msg)],
                };
                let bytes = match dir.read_file(&item.name, MAX_SMALL_FILE) {
                    Ok((b, _)) if b.len() <= MAX_SMALL_FILE => b,
                    Ok(_) => {
                        out.push(problem("metadata file too large".to_owned()));
                        continue;
                    }
                    Err(e) => {
                        out.push(problem(format!("metadata file cannot be read: {e}")));
                        continue;
                    }
                };
                let m = match decode_meta(&bytes, &uid, n) {
                    Ok(m) => m,
                    Err(msg) => {
                        out.push(problem(msg));
                        continue;
                    }
                };
                let Some(etag) = m.etag else { continue };
                match dir.read_file(&json_name(n), MAX_DOCUMENT_BYTES) {
                    Ok((b, _)) if Etag::of(&b) == etag => {}
                    Ok(_) => out.push(problem(format!("the SHA-256 of {} differs from the etag", json_name(n)))),
                    Err(Errno::NOENT) if m.kind == VersionKind::External => {}
                    Err(e) => out.push(problem(format!("{} cannot be read: {e}", json_name(n)))),
                }
            }
        }
        Ok(out)
    }
}
