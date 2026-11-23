// SPDX-License-Identifier: MIT OR Apache-2.0
//! The read side of the volume backend: the scan (5.10, 5.17), `get`, `history`, `version`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use hune_core::document::{parse_folder_file, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::ids::is_valid_id;
use hune_core::limits::MAX_DOCUMENT_BYTES;
use hune_core::time::UtcMillis;
use rustix::io::Errno;

use super::dir::{Dir, FType, FileStat};
use super::meta::{VersionFile, decode_meta, json_name, meta_name, parse_version_name};
use super::{FOLDER_FILE, HISTORY_DIR, Inner, MAX_SMALL_FILE, io_err};
use crate::store::{Delta, EntryMark, EntryProbe, FileSig, FolderMark, FolderState, Marks, StoreEvent};
use crate::types::{
    CanonicalDocument, CatalogEntry, Current, Etag, FolderId, FolderRecord, FolderTitle, MAX_HISTORY_PAGE, StoreError,
    Subject, Uid, Version, VersionKind, VersionMeta,
};

/// Bit of `EntryMark::ctx`: the folder of the entry is invalid.
const CTX_FOLDER_INVALID: u8 = 1;
/// Bit of `EntryMark::ctx`: the uid is in more than one folder.
const CTX_DUPLICATE: u8 = 2;

/// The current file of a uid, read from its folder.
pub(crate) struct CurFile {
    pub folder: FolderId,
    pub stat: FileStat,
    /// The etag that a write compares (5.14, step 4): canonical when valid, else of the raw bytes.
    pub etag: Etag,
    /// The validation result.
    pub doc: Result<CanonicalDocument, Vec<ValidationError>>,
}

/// The head version of a uid and the highest version number in the directory.
pub(crate) struct Head {
    pub meta: Option<VersionMeta>,
    pub max_n: u64,
}

/// Validates the raw bytes of a dashboard file (4.36, 5.10 item 4) and gives its etag.
pub(crate) fn classify(raw: &[u8], uid: &Uid) -> (Etag, Result<CanonicalDocument, Vec<ValidationError>>) {
    if raw.len() > MAX_DOCUMENT_BYTES {
        let e =
            ValidationError::new("", ErrorCode::TooLarge, format!("the file has more than {MAX_DOCUMENT_BYTES} bytes"));
        return (Etag::of(raw), Err(vec![e]));
    }
    match validate(raw, Some(uid)) {
        Ok(doc) => (*doc.etag(), Ok(doc)),
        Err(errs) => (Etag::of(raw), Err(errs)),
    }
}

fn one_error(code: ErrorCode, message: impl Into<String>) -> Vec<ValidationError> {
    vec![ValidationError::new("", code, message)]
}

fn read_error_entry(folder: &FolderId, uid: &Uid, e: Errno) -> Arc<CatalogEntry> {
    Arc::new(CatalogEntry::Invalid {
        uid: uid.clone(),
        folder: folder.clone(),
        etag: Etag::of(b""),
        errors: one_error(ErrorCode::NotPermitted, format!("the file cannot be read: {e}")),
    })
}

impl Inner {
    /// The folder directories of the root.
    pub(crate) fn folder_ids(&self) -> Result<Vec<FolderId>, StoreError> {
        let mut out = Vec::new();
        for item in self.root.list().map_err(|e| io_err(e, "list the root"))? {
            if item.ftype == FType::Dir && item.name_ok {
                out.extend(FolderId::parse(&item.name));
            }
        }
        out.sort();
        Ok(out)
    }

    /// Opens the directory of a folder. `None` when it is absent or a link.
    pub(crate) fn open_folder(&self, folder: &FolderId) -> Result<Option<Dir>, StoreError> {
        match self.root.open_dir(folder.as_str()) {
            Ok(d) => Ok(Some(d)),
            Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => Ok(None),
            Err(e) => Err(io_err(e, format!("open the folder {folder}"))),
        }
    }

    /// Reads the folder file and gives the state of the folder. `None`: no such directory.
    pub(crate) fn read_folder_state(
        &self,
        folder: &FolderId,
    ) -> Result<Option<(FolderState, Option<FileSig>)>, StoreError> {
        let Some(dir) = self.open_folder(folder)? else { return Ok(None) };
        Ok(Some(self.folder_state_in(&dir, folder)?))
    }

    fn folder_state_in(&self, dir: &Dir, folder: &FolderId) -> Result<(FolderState, Option<FileSig>), StoreError> {
        let invalid = |errors| FolderState::Invalid { id: folder.clone(), errors };
        match dir.read_file(FOLDER_FILE, MAX_SMALL_FILE) {
            Err(Errno::NOENT) => Ok((
                invalid(one_error(ErrorCode::MissingMember, format!("the folder file {FOLDER_FILE} is absent"))),
                None,
            )),
            Err(e) => Err(io_err(e, format!("read {folder}/{FOLDER_FILE}"))),
            Ok((bytes, st)) => {
                let state = if bytes.len() > MAX_SMALL_FILE {
                    invalid(one_error(ErrorCode::TooLarge, "the folder file is too large"))
                } else {
                    match parse_folder_file(&bytes) {
                        Err(errs) => invalid(errs),
                        Ok(f) => match FolderTitle::parse(&f.title) {
                            Ok(title) => FolderState::Valid(FolderRecord { id: folder.clone(), title }),
                            Err(e) => {
                                invalid(vec![ValidationError::new("/title", ErrorCode::BadFormat, e.to_string())])
                            }
                        },
                    }
                };
                Ok((state, Some(st.sig)))
            }
        }
    }

    /// The folders that hold a file `<uid>.json`, in byte order.
    pub(crate) fn locate(&self, uid: &Uid) -> Result<Vec<FolderId>, StoreError> {
        let name = format!("{uid}.json");
        let mut out = Vec::new();
        for f in self.folder_ids()? {
            let Some(dir) = self.open_folder(&f)? else { continue };
            match dir.stat(&name) {
                Ok(st) if st.ftype == FType::File => out.push(f),
                Ok(_) | Err(Errno::NOENT) => {}
                Err(e) => return Err(io_err(e, format!("stat {f}/{name}"))),
            }
        }
        Ok(out)
    }

    /// Reads `<folder>/<uid>.json`. `None` when it is absent.
    pub(crate) fn read_dashboard(&self, folder: &FolderId, uid: &Uid) -> Result<Option<CurFile>, StoreError> {
        let Some(dir) = self.open_folder(folder)? else { return Ok(None) };
        let name = format!("{uid}.json");
        match dir.read_file(&name, MAX_DOCUMENT_BYTES) {
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(io_err(e, format!("read {folder}/{name}"))),
            Ok((raw, stat)) => {
                let (etag, doc) = classify(&raw, uid);
                Ok(Some(CurFile { folder: folder.clone(), stat, etag, doc }))
            }
        }
    }

    /// Opens `.history/<uid>`. `None` when it is absent, or a link.
    pub(crate) fn hist_dir(&self, uid: &Uid) -> Result<Option<Dir>, StoreError> {
        let history = match self.root.open_dir(HISTORY_DIR) {
            Ok(d) => d,
            Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return Ok(None),
            Err(e) => return Err(io_err(e, "open .history")),
        };
        match history.open_dir(uid.as_str()) {
            Ok(d) => Ok(Some(d)),
            Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => Ok(None),
            Err(e) => Err(io_err(e, format!("open .history/{uid}"))),
        }
    }

    /// The version numbers with a metadata file, from the lowest to the highest. Other entries that are not
    /// permitted give the event `store.entry_ignored`.
    pub(crate) fn list_versions(&self, uid: &Uid, dir: &Dir) -> Result<Vec<u64>, StoreError> {
        let mut metas = Vec::new();
        for item in dir.list().map_err(|e| io_err(e, format!("list .history/{uid}")))? {
            if item.name.starts_with('.') {
                continue;
            }
            match parse_version_name(&item.name) {
                Some((n, VersionFile::Meta)) if item.ftype == FType::File => metas.push(n),
                Some((_, VersionFile::Json)) if item.ftype == FType::File => {}
                _ => self.note_ignored(
                    format!("{HISTORY_DIR}/{uid}/{}", item.name),
                    &format!("not a version file ({})", item.ftype.describe()),
                    0,
                    false,
                ),
            }
        }
        metas.sort_unstable();
        Ok(metas)
    }

    /// Reads one metadata file. A file that breaks a rule is skipped with an event (5.11, item 4).
    pub(crate) fn read_meta(&self, dir: &Dir, uid: &Uid, n: u64) -> Result<Option<VersionMeta>, StoreError> {
        let name = meta_name(n);
        match dir.read_file(&name, MAX_SMALL_FILE) {
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(io_err(e, format!("read .history/{uid}/{name}"))),
            Ok((bytes, st)) => {
                let result = if bytes.len() > MAX_SMALL_FILE {
                    Err("metadata file too large".to_owned())
                } else {
                    decode_meta(&bytes, uid, n)
                };
                match result {
                    Ok(m) => Ok(Some(m)),
                    Err(reason) => {
                        self.note_ignored(
                            format!("{HISTORY_DIR}/{uid}/{name}"),
                            &reason,
                            st.sig.size ^ (st.sig.mtime_ns as u64),
                            false,
                        );
                        Ok(None)
                    }
                }
            }
        }
    }

    /// The head version: the valid metadata file with the highest number.
    pub(crate) fn head(&self, uid: &Uid) -> Result<Head, StoreError> {
        let Some(dir) = self.hist_dir(uid)? else { return Ok(Head { meta: None, max_n: 0 }) };
        let metas = self.list_versions(uid, &dir)?;
        let max_n = metas.last().copied().unwrap_or(0);
        for n in metas.iter().rev() {
            if let Some(m) = self.read_meta(&dir, uid, *n)? {
                return Ok(Head { meta: Some(m), max_n });
            }
        }
        Ok(Head { meta: None, max_n })
    }

    /// The `version`, `saved_at` and `saved_by` of a current document (5.2, `Current`).
    pub(crate) fn current_fields(
        etag: &Etag,
        mtime: UtcMillis,
        head: Option<&VersionMeta>,
    ) -> (u64, UtcMillis, Subject) {
        match head {
            Some(h) if h.etag.as_ref() == Some(etag) => (h.version, h.saved_at, h.saved_by.clone()),
            Some(h) => (h.version, mtime, Subject::external()),
            None => (0, mtime, Subject::external()),
        }
    }

    fn valid_entry(
        &self,
        cur_folder: &FolderId,
        uid: &Uid,
        stat: &FileStat,
        doc: CanonicalDocument,
    ) -> Arc<CatalogEntry> {
        let head = match self.head(uid) {
            Ok(h) => h,
            Err(e) => {
                return Arc::new(CatalogEntry::Invalid {
                    uid: uid.clone(),
                    folder: cur_folder.clone(),
                    etag: *doc.etag(),
                    errors: one_error(ErrorCode::NotPermitted, format!("the history cannot be read: {e}")),
                });
            }
        };
        let (version, saved_at, saved_by) = Self::current_fields(doc.etag(), stat.mtime, head.meta.as_ref());
        let current = Current {
            uid: uid.clone(),
            folder: cur_folder.clone(),
            version,
            etag: *doc.etag(),
            bytes: Arc::new(doc.bytes().to_vec()),
            saved_at,
            saved_by,
        };
        // The names of 6.14 are made here, in `spawn_blocking`, so that the first read never makes them on an
        // async worker (3.7).
        let _ = doc.names_json();
        Arc::new(CatalogEntry::Valid { current, doc: Arc::new(doc) })
    }

    /// Builds the catalog entry of one file, with the context of the folder and the uniqueness.
    fn build_entry(
        &self,
        folder: &FolderId,
        uid: &Uid,
        folder_state: &FolderState,
        raw: &[u8],
        stat: &FileStat,
        ctx: u8,
    ) -> Arc<CatalogEntry> {
        let invalid = |errors| {
            Arc::new(CatalogEntry::Invalid { uid: uid.clone(), folder: folder.clone(), etag: Etag::of(raw), errors })
        };
        if ctx & CTX_FOLDER_INVALID != 0 {
            let reason = match folder_state {
                FolderState::Invalid { errors, .. } => errors.first().map(|e| e.message.clone()).unwrap_or_default(),
                FolderState::Valid(_) => String::new(),
            };
            return invalid(one_error(
                ErrorCode::NotPermitted,
                format!("the folder {folder} has no valid folder file: {reason}"),
            ));
        }
        if ctx & CTX_DUPLICATE != 0 {
            return invalid(one_error(ErrorCode::NotUnique, format!("the uid {uid} is in more than one folder")));
        }
        match classify(raw, uid) {
            (_, Ok(doc)) => self.valid_entry(folder, uid, stat, doc),
            (_, Err(errors)) => invalid(errors),
        }
    }

    /// Writes the event `store.entry_ignored` for an entry of a scan. A change of the entry
    /// (its size, its times or its inode) gives the event again.
    fn ignore_entry(&self, dir: &Dir, path: String, name: &str, reason: &str) {
        let stamp = dir.stat(name).map_or(0, |s| (s.sig.mtime_ns as u64) ^ s.sig.size ^ s.sig.ino);
        self.note_ignored(path, reason, stamp, true);
    }

    /// The scan of 5.10 and 5.17. `emit_changes` makes the events `ExternalChange`.
    pub(crate) fn scan(&self, marks: &Marks, emit_changes: bool) -> Result<Delta, StoreError> {
        self.begin_scan();
        let out = self.scan_inner(marks, emit_changes);
        self.end_scan();
        out
    }

    fn scan_inner(&self, marks: &Marks, emit_changes: bool) -> Result<Delta, StoreError> {
        self.faults.check(super::faults::FaultPoint::Scan).map_err(|e| io_err(e, "scan the root"))?;
        // 1. The root.
        let mut folder_names = Vec::new();
        for item in self.root.list().map_err(|e| io_err(e, "list the root"))? {
            if item.name.starts_with('.') {
                continue;
            }
            if item.ftype == FType::Dir && item.name_ok && is_valid_id(&item.name) {
                if let Ok(id) = FolderId::parse(&item.name) {
                    folder_names.push(id);
                }
            } else {
                self.ignore_entry(
                    &self.root,
                    item.name.clone(),
                    &item.name,
                    &format!("{} with a name that is not a folder id", item.ftype.describe()),
                );
            }
        }
        folder_names.sort();
        // 2. The folders and their files.
        let mut new_marks = Marks::default();
        let mut changed_folders = Vec::new();
        let mut files: Vec<(FolderId, Uid, FileStat)> = Vec::new();
        for f in &folder_names {
            let Some(dir) = self.open_folder(f)? else { continue };
            let listing = match dir.list() {
                Ok(l) => l,
                Err(Errno::NOENT) => continue,
                Err(e) => return Err(io_err(e, format!("list the folder {f}"))),
            };
            let folder_stat = match dir.stat(FOLDER_FILE) {
                Ok(st) if st.ftype == FType::File => Some(st.sig),
                Ok(st) => {
                    self.ignore_entry(
                        &dir,
                        format!("{f}/{FOLDER_FILE}"),
                        FOLDER_FILE,
                        &format!("{} instead of a file", st.ftype.describe()),
                    );
                    None
                }
                Err(Errno::NOENT) => None,
                Err(e) => return Err(io_err(e, format!("stat {f}/{FOLDER_FILE}"))),
            };
            let (state, sig) = match marks.folders.get(f) {
                Some(m) if m.sig == folder_stat => (m.state.clone(), m.sig),
                _ => self.folder_state_in(&dir, f)?,
            };
            let changed = marks.folders.get(f).is_none_or(|m| m.state != state);
            if changed {
                changed_folders.push(state.clone());
            }
            new_marks.folders.insert(f.clone(), FolderMark { sig, state });
            for item in listing {
                if item.name.starts_with('.') || item.name == FOLDER_FILE {
                    continue;
                }
                let uid = item.name.strip_suffix(".json").and_then(|stem| Uid::parse(stem).ok());
                match uid {
                    Some(uid) if item.ftype == FType::File && item.name_ok => match dir.stat(&item.name) {
                        Ok(st) => files.push((f.clone(), uid, st)),
                        Err(Errno::NOENT) => {}
                        Err(e) => return Err(io_err(e, format!("stat {f}/{}", item.name))),
                    },
                    _ => {
                        let reason = if uid.is_some() {
                            format!("{} instead of a file", item.ftype.describe())
                        } else {
                            format!("{} that is not a dashboard file", item.ftype.describe())
                        };
                        self.ignore_entry(&dir, format!("{f}/{}", item.name), &item.name, &reason);
                    }
                }
            }
        }
        // 3. Duplicate uids (5.10, item 3).
        let mut counts: HashMap<&Uid, usize> = HashMap::new();
        for (_, uid, _) in &files {
            *counts.entry(uid).or_default() += 1;
        }
        let duplicates: BTreeSet<Uid> = counts.iter().filter(|(_, n)| **n > 1).map(|(u, _)| (*u).clone()).collect();
        // 4. The entries.
        let mut entries = Vec::new();
        files.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
        for (folder, uid, st) in files {
            let state = &new_marks.folders[&folder].state;
            let mut ctx = 0;
            if matches!(state, FolderState::Invalid { .. }) {
                ctx |= CTX_FOLDER_INVALID;
            }
            if duplicates.contains(&uid) {
                ctx |= CTX_DUPLICATE;
            }
            let key = (uid.clone(), folder.clone());
            let previous = marks.entries.get(&key);
            if let Some(m) = previous
                && m.sig == Some(st.sig)
                && m.ctx == ctx
            {
                new_marks.entries.insert(key, m.clone());
                continue;
            }
            let state = state.clone();
            let entry = match self.read_dashboard_raw(&folder, &uid) {
                Ok(Some((raw, stat))) => self.build_entry(&folder, &uid, &state, &raw, &stat, ctx),
                Ok(None) => continue,
                Err(e) => read_error_entry(&folder, &uid, e),
            };
            if emit_changes
                && previous.is_none_or(|m| m.etag != *entry.etag())
                && !self.is_own_write(&uid, entry.etag())
            {
                self.push_event(StoreEvent::ExternalChange {
                    uid: uid.clone(),
                    folder: folder.clone(),
                    etag_before: previous.map(|m| m.etag),
                    etag_after: Some(*entry.etag()),
                });
            }
            new_marks.entries.insert(key, EntryMark { etag: *entry.etag(), sig: Some(st.sig), ctx });
            entries.push(entry);
        }
        let removed_folders: Vec<FolderId> =
            marks.folders.keys().filter(|f| !new_marks.folders.contains_key(*f)).cloned().collect();
        let removed_entries: Vec<_> =
            marks.entries.keys().filter(|k| !new_marks.entries.contains_key(*k)).cloned().collect();
        Ok(Delta { folders: changed_folders, removed_folders, entries, removed_entries, marks: new_marks })
    }

    /// Reads the raw bytes of a dashboard file, without the validation.
    fn read_dashboard_raw(&self, folder: &FolderId, uid: &Uid) -> Result<Option<(Vec<u8>, FileStat)>, Errno> {
        let dir = match self.root.open_dir(folder.as_str()) {
            Ok(d) => d,
            Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return Ok(None),
            Err(e) => return Err(e),
        };
        match dir.read_file(&format!("{uid}.json"), MAX_DOCUMENT_BYTES) {
            Ok(r) => Ok(Some(r)),
            Err(Errno::NOENT) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The entries of one uid with their marks (the extension `entry_probe`).
    pub(crate) fn entry_probe_sync(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        let folders = self.locate(uid)?;
        let dup = folders.len() > 1;
        let mut out = Vec::new();
        for f in folders {
            let Some((state, _)) = self.read_folder_state(&f)? else { continue };
            let Some(dir) = self.open_folder(&f)? else { continue };
            let (raw, stat) = match dir.read_file(&format!("{uid}.json"), MAX_DOCUMENT_BYTES) {
                Ok(r) => r,
                Err(Errno::NOENT) => continue,
                Err(e) => return Err(io_err(e, format!("read {f}/{uid}.json"))),
            };
            let mut ctx = 0;
            if matches!(state, FolderState::Invalid { .. }) {
                ctx |= CTX_FOLDER_INVALID;
            }
            if dup {
                ctx |= CTX_DUPLICATE;
            }
            let entry = self.build_entry(&f, uid, &state, &raw, &stat, ctx);
            let mark = EntryMark { etag: *entry.etag(), sig: Some(stat.sig), ctx };
            out.push((entry, mark));
        }
        Ok(EntryProbe { entries: out })
    }

    /// All folders with their marks (the extension `folder_probe`).
    pub(crate) fn folder_probe_sync(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        let mut out = Vec::new();
        for f in self.folder_ids()? {
            if let Some((state, sig)) = self.read_folder_state(&f)? {
                out.push((state.clone(), FolderMark { sig, state }));
            }
        }
        Ok(out)
    }

    /// `Store::get` (5.4).
    pub(crate) fn get_current(&self, uid: &Uid) -> Result<Current, StoreError> {
        let folders = self.locate(uid)?;
        let folder = match folders.as_slice() {
            [] => return Err(StoreError::NotFound),
            [f] => f.clone(),
            _ => {
                return Err(StoreError::Invalid(one_error(
                    ErrorCode::NotUnique,
                    format!("the uid {uid} is in more than one folder"),
                )));
            }
        };
        if let Some((FolderState::Invalid { errors, .. }, _)) = self.read_folder_state(&folder)? {
            let first = errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(StoreError::Invalid(one_error(
                ErrorCode::NotPermitted,
                format!("the folder {folder} has no valid folder file: {first}"),
            )));
        }
        let Some(cur) = self.read_dashboard(&folder, uid)? else { return Err(StoreError::NotFound) };
        match cur.doc {
            Err(errors) => Err(StoreError::Invalid(errors)),
            Ok(doc) => {
                let head = self.head(uid)?;
                let (version, saved_at, saved_by) =
                    Self::current_fields(doc.etag(), cur.stat.mtime, head.meta.as_ref());
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

    /// `Store::raw_current`: the stored bytes of `<folder>/<uid>.json`, without the validation.
    pub(crate) fn raw_current_sync(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        let Some(dir) = self.open_folder(folder)? else { return Err(StoreError::NotFound) };
        let name = format!("{uid}.json");
        match dir.read_file(&name, MAX_DOCUMENT_BYTES) {
            Err(Errno::NOENT) => Err(StoreError::NotFound),
            Err(e) => Err(io_err(e, format!("read {folder}/{name}"))),
            Ok((raw, _)) => {
                let etag = Etag::of(&raw);
                Ok((raw, etag))
            }
        }
    }

    /// The size of `<n>.json`, when it exists.
    fn version_size(&self, dir: &Dir, n: u64) -> Option<u64> {
        dir.stat(&json_name(n)).ok().filter(|s| s.ftype == FType::File).map(|s| s.sig.size)
    }

    /// `Store::history` (5.4). A `limit` above 100 gives 100.
    pub(crate) fn history_sync(
        &self,
        uid: &Uid,
        before: Option<u64>,
        limit: u32,
    ) -> Result<Vec<VersionMeta>, StoreError> {
        let limit = limit.min(MAX_HISTORY_PAGE) as usize;
        let hist = self.hist_dir(uid)?;
        let metas = match &hist {
            Some(d) => self.list_versions(uid, d)?,
            None => Vec::new(),
        };
        if metas.is_empty() && self.locate(uid)?.is_empty() {
            return Err(StoreError::NotFound);
        }
        let mut out = Vec::new();
        let Some(dir) = hist else { return Ok(out) };
        for n in metas.iter().rev().filter(|n| before.is_none_or(|b| **n < b)) {
            if out.len() >= limit {
                break;
            }
            if let Some(mut m) = self.read_meta(&dir, uid, *n)? {
                m.size = if m.kind == VersionKind::Delete { None } else { self.version_size(&dir, *n) };
                out.push(m);
            }
        }
        Ok(out)
    }

    /// `Store::version` (5.4).
    pub(crate) fn version_sync(&self, uid: &Uid, n: u64) -> Result<Version, StoreError> {
        if n == 0 || n > i64::MAX as u64 {
            return Err(StoreError::VersionNotFound);
        }
        let Some(dir) = self.hist_dir(uid)? else { return Err(StoreError::VersionNotFound) };
        let Some(mut meta) = self.read_meta(&dir, uid, n)? else { return Err(StoreError::VersionNotFound) };
        let Some(etag) = meta.etag else { return Ok(Version { meta, bytes: None }) };
        match dir.read_file(&json_name(n), MAX_DOCUMENT_BYTES) {
            Err(Errno::NOENT) => Ok(Version { meta, bytes: None }),
            Err(e) => Err(io_err(e, format!("read .history/{uid}/{}", json_name(n)))),
            Ok((bytes, _)) => {
                if Etag::of(&bytes) != etag {
                    self.note_ignored(
                        format!("{HISTORY_DIR}/{uid}/{}", json_name(n)),
                        "the SHA-256 of the bytes differs from the etag",
                        bytes.len() as u64,
                        false,
                    );
                    return Err(StoreError::VersionNotFound);
                }
                meta.size = Some(bytes.len() as u64);
                Ok(Version { meta, bytes: Some(bytes) })
            }
        }
    }

    /// The versions of all uids in `.history`, as a map from uid to the version numbers. Used
    /// by the export and the checks.
    pub(crate) fn history_uids(&self) -> Result<BTreeMap<Uid, ()>, StoreError> {
        let mut out = BTreeMap::new();
        let history = match self.root.open_dir(HISTORY_DIR) {
            Ok(d) => d,
            Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return Ok(out),
            Err(e) => return Err(io_err(e, "open .history")),
        };
        for item in history.list().map_err(|e| io_err(e, "list .history"))? {
            if item.name.starts_with('.') {
                continue;
            }
            match Uid::parse(&item.name) {
                Ok(uid) if item.ftype == FType::Dir && item.name_ok => {
                    out.insert(uid, ());
                }
                _ => self.note_ignored(
                    format!("{HISTORY_DIR}/{}", item.name),
                    &format!("{} that is not a uid directory", item.ftype.describe()),
                    0,
                    false,
                ),
            }
        }
        Ok(out)
    }
}
