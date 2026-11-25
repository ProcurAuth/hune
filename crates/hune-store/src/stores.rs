// SPDX-License-Identifier: MIT OR Apache-2.0
//! The stores of the process (5.40): the primary store and the read-only stores behind one `Store`.
//!
//! One member of [`Stores`] is a volume or the PostgreSQL store. The primary member is the only one that accepts
//! writes: a write that targets another member gives `StoreError::ReadOnly`. The first load checks the start rules of
//! 5.41. After the start, the poll of a volume keeps the last valid state of a file that becomes invalid (5.42).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::store::{
    AnyStore, Delta, EntryKey, EntryProbe, Exporter, FolderMark, FolderState, Importer, Marks, Problem, Snapshot,
    Store, StoreEvent,
};
use crate::types::{
    CanonicalDocument, CatalogEntry, Current, Etag, FolderId, FolderTitle, Saved, StoreError, Uid, ValidVersion,
    Version, VersionKind, VersionMeta, WriteMeta,
};

/// The kind of a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    /// A volume store (5.9).
    Volume,
    /// The PostgreSQL store (5.23).
    Postgres,
}

impl StoreKind {
    /// The name for the log: `volume` or `postgres`.
    pub fn name(self) -> &'static str {
        match self {
            StoreKind::Volume => "volume",
            StoreKind::Postgres => "postgres",
        }
    }
}

/// What the process knows about one store, besides the store itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInfo {
    /// The id of the store: the label of the volume, or `postgres`.
    pub id: String,
    /// The kind.
    pub kind: StoreKind,
    /// The name of the root folder in the page. `None` for the primary store.
    pub title: Option<String>,
    /// True for the primary store.
    pub primary: bool,
}

impl StoreInfo {
    /// The name of the store in a message: the block of the configuration.
    pub fn shown(&self) -> String {
        match self.kind {
            StoreKind::Volume => format!("storage.volume \"{}\"", self.id),
            StoreKind::Postgres => "storage.postgres".to_owned(),
        }
    }
}

/// One store to put in [`Stores`].
#[derive(Debug, Clone)]
pub struct StoreSpec {
    /// The description.
    pub info: StoreInfo,
    /// The directory of a volume, for the messages.
    pub path: Option<PathBuf>,
    /// The period of the poll of this store.
    pub interval: Duration,
    /// The store. A store that is not the primary MUST be opened read-only when it is a volume.
    pub store: AnyStore,
}

/// The counts of one store after the first load, for the log line `store.loaded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberReport {
    /// The description of the store.
    pub info: StoreInfo,
    /// Valid folders.
    pub folders: usize,
    /// Valid dashboards.
    pub dashboards: usize,
    /// Invalid entries and invalid folders.
    pub invalid: usize,
}

/// A start rule that the first load found broken (5.41). The text names the store, the file and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartIssue(pub String);

impl std::fmt::Display for StartIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

struct Member {
    info: StoreInfo,
    path: Option<PathBuf>,
    interval: Duration,
    store: AnyStore,
}

#[derive(Default)]
struct State {
    folder_owner: BTreeMap<FolderId, usize>,
    uid_owner: BTreeMap<Uid, usize>,
    /// The snapshot of the checked first load. `Store::load` takes it one time.
    first: Option<Snapshot>,
    /// The next poll of each member.
    due: Vec<Option<Instant>>,
    /// The last valid entry of each file of a volume.
    known_entries: BTreeMap<EntryKey, Arc<CatalogEntry>>,
    /// The last valid state of each folder of a volume.
    known_folders: BTreeMap<FolderId, FolderState>,
    /// The entries of a volume that show their last valid state because the file is not valid now (5.42). The value is
    /// the etag of the invalid file that the log already reported.
    stale: BTreeMap<EntryKey, Etag>,
    /// The files of a volume that are not valid and have no last valid state: the catalog has no entry for them.
    rejected: BTreeMap<EntryKey, Etag>,
    /// The folders of a volume that show their last valid state, or that the catalog does not have.
    bad_folders: BTreeSet<FolderId>,
    events: Vec<StoreEvent>,
}

struct Inner {
    members: Vec<Member>,
    primary: Option<usize>,
    state: Mutex<State>,
}

/// The stores of the process. A clone shares the state.
#[derive(Clone)]
pub struct Stores {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Stores {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<&str> = self.inner.members.iter().map(|m| m.info.id.as_str()).collect();
        f.debug_struct("Stores").field("stores", &ids).field("primary", &self.primary_id()).finish()
    }
}

fn reasons(errors: &[crate::types::ValidationError]) -> String {
    errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

impl Stores {
    /// Makes the stores. It reads nothing. At most one spec is the primary.
    pub fn new(specs: Vec<StoreSpec>) -> Stores {
        let primary = specs.iter().position(|s| s.info.primary);
        let members: Vec<Member> = specs
            .into_iter()
            .map(|s| Member { info: s.info, path: s.path, interval: s.interval, store: s.store })
            .collect();
        let state = State { due: vec![None; members.len()], ..State::default() };
        Stores { inner: Arc::new(Inner { members, primary, state: Mutex::new(state) }) }
    }

    /// Makes the stores of one primary store: the process of a command line that works on one store.
    pub fn single(store: AnyStore, id: &str, kind: StoreKind) -> Stores {
        let interval = store.poll_interval();
        Stores::new(vec![StoreSpec {
            info: StoreInfo { id: id.to_owned(), kind, title: None, primary: true },
            path: None,
            interval,
            store,
        }])
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn member(&self, i: usize) -> &Member {
        &self.inner.members[i]
    }

    /// The description of each store, in the order of the configuration.
    pub fn infos(&self) -> Vec<&StoreInfo> {
        self.inner.members.iter().map(|m| &m.info).collect()
    }

    /// The id of the primary store, or `None` when no store accepts writes.
    pub fn primary_id(&self) -> Option<&str> {
        self.inner.primary.map(|i| self.inner.members[i].info.id.as_str())
    }

    /// The primary store, or `None`.
    pub fn primary(&self) -> Option<&AnyStore> {
        self.inner.primary.map(|i| &self.inner.members[i].store)
    }

    /// The titles of the read-only stores: the names of their root folders.
    pub fn read_only_titles(&self) -> Vec<&str> {
        self.inner.members.iter().filter(|m| !m.info.primary).filter_map(|m| m.info.title.as_deref()).collect()
    }

    /// The store that holds a folder, or `None` when the folder is not in the catalog.
    pub fn store_of_folder(&self, folder: &FolderId) -> Option<&StoreInfo> {
        let i = *self.state().folder_owner.get(folder)?;
        Some(&self.member(i).info)
    }

    /// The store that holds a dashboard, or `None` when the uid is not in the catalog.
    pub fn store_of_uid(&self, uid: &Uid) -> Option<&StoreInfo> {
        let i = *self.state().uid_owner.get(uid)?;
        Some(&self.member(i).info)
    }

    /// True when a write to this folder is refused because its store is read-only. A folder that is not in the
    /// catalog is not refused here: the write finds that it does not exist.
    pub fn folder_is_read_only(&self, folder: &FolderId) -> bool {
        self.store_of_folder(folder).is_some_and(|s| !s.primary)
    }

    /// True when a write to this dashboard is refused because its store is read-only.
    pub fn uid_is_read_only(&self, uid: &Uid) -> bool {
        self.store_of_uid(uid).is_some_and(|s| !s.primary)
    }

    /// True when the dashboard shows its last valid state because its file is not valid now (5.42).
    pub fn is_stale(&self, uid: &Uid) -> bool {
        let st = self.state();
        let Some(&owner) = st.uid_owner.get(uid) else { return false };
        let _ = owner;
        st.stale.keys().any(|(u, _)| u == uid)
    }

    /// For each volume, the number of files that are not valid now: the stale entries, the files with no last valid
    /// state and the folders. The ids come in the order of the configuration.
    pub fn invalid_files(&self) -> Vec<(String, usize)> {
        let st = self.state();
        let mut counts = vec![0usize; self.inner.members.len()];
        let owner_of = |folder: &FolderId| st.folder_owner.get(folder).copied();
        for (_, folder) in st.stale.keys().chain(st.rejected.keys()) {
            if let Some(i) = owner_of(folder) {
                counts[i] += 1;
            }
        }
        for folder in &st.bad_folders {
            if let Some(i) = owner_of(folder) {
                counts[i] += 1;
            }
        }
        self.inner
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.info.kind == StoreKind::Volume)
            .map(|(i, m)| (m.info.id.clone(), counts[i]))
            .collect()
    }

    /// Takes the events that the stores hold for the log.
    pub fn drain_events(&self) -> Vec<StoreEvent> {
        let mut out: Vec<StoreEvent> = Vec::new();
        for m in &self.inner.members {
            out.extend(m.store.drain_events());
        }
        out.extend(std::mem::take(&mut self.state().events));
        out
    }

    /// The name of the backend for the log: the name of the primary store, or `volume` and `postgres` joined when
    /// there is no primary store.
    pub fn backend_name(&self) -> &'static str {
        match self.inner.primary {
            Some(i) => self.inner.members[i].store.backend_name(),
            None => "none",
        }
    }

    /// The period of the poller: the shortest period of the stores.
    pub fn poll_interval(&self) -> Duration {
        self.inner.members.iter().map(|m| m.interval).min().unwrap_or(Duration::from_secs(5))
    }

    /// The number of versions that the primary store validated for a restore (tests only).
    #[doc(hidden)]
    pub fn restore_checks(&self) -> u64 {
        self.primary().map_or(0, AnyStore::restore_checks)
    }

    /// Reads each store one time and checks the start rules of 5.41. The result gives the counts of each store for
    /// the log. The next call of `Store::load` gives the same state without a second read.
    pub async fn open_checked(&self) -> Result<Vec<MemberReport>, StartIssue> {
        let mut snapshots = Vec::with_capacity(self.inner.members.len());
        for m in &self.inner.members {
            let snap = m.store.load().await.map_err(|e| StartIssue(format!("{}: {e}", m.info.shown())))?;
            snapshots.push(snap);
        }
        let mut reports = Vec::new();
        let mut st = self.state();
        let mut folder_owner: BTreeMap<FolderId, usize> = BTreeMap::new();
        let mut uid_owner: BTreeMap<Uid, (usize, FolderId)> = BTreeMap::new();
        for (i, (m, snap)) in self.inner.members.iter().zip(&snapshots).enumerate() {
            let shown = m.info.shown();
            let root = m.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            let mut folders = 0;
            let mut dashboards = 0;
            let mut invalid = 0;
            for f in &snap.folders {
                match f {
                    FolderState::Valid(_) => folders += 1,
                    FolderState::Invalid { id, errors } => {
                        invalid += 1;
                        if m.info.kind == StoreKind::Volume {
                            return Err(StartIssue(format!(
                                "{shown}: the folder file {root}/{id}/_folder.json is not valid: {}",
                                reasons(errors)
                            )));
                        }
                    }
                }
                if let Some(&other) = folder_owner.get(f.id()) {
                    return Err(StartIssue(format!(
                        "the folder id \"{}\" is in the store \"{}\" and in the store \"{}\"",
                        f.id(),
                        self.member(other).info.id,
                        m.info.id
                    )));
                }
                folder_owner.insert(f.id().clone(), i);
            }
            for e in &snap.entries {
                match &**e {
                    CatalogEntry::Valid { .. } => dashboards += 1,
                    CatalogEntry::Invalid { uid, folder, errors, .. } => {
                        invalid += 1;
                        if m.info.kind == StoreKind::Volume {
                            return Err(StartIssue(format!(
                                "{shown}: the file {root}/{folder}/{uid}.json is not valid: {}",
                                reasons(errors)
                            )));
                        }
                    }
                }
                let uid = e.uid();
                if let Some((other, other_folder)) = uid_owner.get(uid) {
                    // The same uid in two folders of one store is an error of the data of that store (5.10).
                    if *other != i {
                        return Err(StartIssue(format!(
                            "the dashboard uid \"{uid}\" is in the store \"{}\" (folder \"{other_folder}\") and in the store \"{}\" (folder \"{}\")",
                            self.member(*other).info.id,
                            m.info.id,
                            e.folder()
                        )));
                    }
                } else {
                    uid_owner.insert(uid.clone(), (i, e.folder().clone()));
                }
            }
            if !m.info.primary && dashboards + invalid == 0 {
                return Err(StartIssue(format!("{shown} is a read-only store and has no dashboard")));
            }
            reports.push(MemberReport { info: m.info.clone(), folders, dashboards, invalid });
        }
        // The title of a read-only store is the name of a root folder: no folder of the primary store has it.
        if let Some(p) = self.inner.primary {
            for f in &snapshots[p].folders {
                let FolderState::Valid(r) = f else { continue };
                for m in self.inner.members.iter().filter(|m| !m.info.primary) {
                    if m.info.title.as_deref() == Some(r.title.as_str()) {
                        return Err(StartIssue(format!(
                            "the folder \"{}\" of the primary store has the title of the read-only store \"{}\" ({}.title); give another name",
                            r.title.as_str(),
                            m.info.id,
                            m.info.shown()
                        )));
                    }
                }
            }
        }
        // Merge the snapshots.
        let mut merged = Snapshot { folders: Vec::new(), entries: Vec::new(), marks: Marks::empty() };
        for snap in snapshots {
            merged.folders.extend(snap.folders);
            merged.entries.extend(snap.entries);
            merged.marks.folders.extend(snap.marks.folders);
            merged.marks.entries.extend(snap.marks.entries);
        }
        merged.folders.sort_by(|a, b| a.id().cmp(b.id()));
        merged.entries.sort_by(|a, b| (a.uid(), a.folder()).cmp(&(b.uid(), b.folder())));
        for (k, v) in &folder_owner {
            st.folder_owner.insert(k.clone(), *v);
        }
        for (k, (v, _)) in &uid_owner {
            st.uid_owner.insert(k.clone(), *v);
        }
        for e in &merged.entries {
            let owner = st.uid_owner.get(e.uid()).copied();
            if owner.is_some_and(|o| self.member(o).info.kind == StoreKind::Volume) {
                st.known_entries.insert((e.uid().clone(), e.folder().clone()), e.clone());
            }
        }
        for f in &merged.folders {
            let owner = st.folder_owner.get(f.id()).copied();
            if owner.is_some_and(|o| self.member(o).info.kind == StoreKind::Volume) {
                st.known_folders.insert(f.id().clone(), f.clone());
            }
        }
        let now = Instant::now();
        for (i, m) in self.inner.members.iter().enumerate() {
            st.due[i] = Some(now + m.interval);
        }
        st.first = Some(merged);
        Ok(reports)
    }

    /// The marks of the items of one store.
    fn marks_of(&self, st: &State, marks: &Marks, i: usize) -> Marks {
        let mut sub = Marks::empty();
        for (id, mark) in &marks.folders {
            if st.folder_owner.get(id) == Some(&i) {
                sub.folders.insert(id.clone(), mark.clone());
            }
        }
        for (key, mark) in &marks.entries {
            if st.folder_owner.get(&key.1) == Some(&i) {
                sub.entries.insert(key.clone(), mark.clone());
            }
        }
        sub
    }

    /// The store of a uid for a read: the owner, or the primary store when the uid is not in the catalog (a deleted
    /// dashboard has history in the primary store).
    fn route_uid(&self, uid: &Uid) -> Option<usize> {
        let owner = self.state().uid_owner.get(uid).copied();
        owner.or(self.inner.primary)
    }

    /// The store of a uid for a write. `ReadOnly` when the store is not the primary store.
    fn write_uid(&self, uid: &Uid) -> Result<&AnyStore, StoreError> {
        let Some(p) = self.inner.primary else { return Err(StoreError::ReadOnly) };
        match self.state().uid_owner.get(uid) {
            Some(&o) if o != p => Err(StoreError::ReadOnly),
            _ => Ok(&self.member(p).store),
        }
    }

    /// The store of a folder for a write. `ReadOnly` when the store is not the primary store.
    fn write_folder(&self, folder: &FolderId) -> Result<&AnyStore, StoreError> {
        let Some(p) = self.inner.primary else { return Err(StoreError::ReadOnly) };
        match self.state().folder_owner.get(folder) {
            Some(&o) if o != p => Err(StoreError::ReadOnly),
            _ => Ok(&self.member(p).store),
        }
    }

    fn note_uid(&self, uid: &Uid) {
        if let Some(p) = self.inner.primary {
            self.state().uid_owner.entry(uid.clone()).or_insert(p);
        }
    }

    fn note_folder(&self, folder: &FolderId) {
        if let Some(p) = self.inner.primary {
            self.state().folder_owner.entry(folder.clone()).or_insert(p);
        }
    }

    /// Refuses a title that a read-only store uses as the name of its root folder.
    fn check_title(&self, title: &FolderTitle) -> Result<(), StoreError> {
        if self.read_only_titles().iter().any(|t| *t == title.as_str()) {
            return Err(StoreError::Exists { folder: None });
        }
        Ok(())
    }

    /// Keeps the last valid state of the files of a volume that are not valid now (5.42).
    fn filter_volume_delta(&self, st: &mut State, i: usize, d: &mut Delta) {
        let m = self.member(i);
        let root = m.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        let shown = m.info.shown();
        let mut entries = Vec::with_capacity(d.entries.len());
        for e in std::mem::take(&mut d.entries) {
            let key = (e.uid().clone(), e.folder().clone());
            match &*e {
                CatalogEntry::Valid { .. } => {
                    st.stale.remove(&key);
                    st.rejected.remove(&key);
                    st.known_entries.insert(key, e.clone());
                    entries.push(e);
                }
                CatalogEntry::Invalid { uid, folder, etag, errors } => {
                    let path = format!("{root}/{folder}/{uid}.json");
                    let reason = reasons(errors);
                    if let Some(good) = st.known_entries.get(&key).cloned() {
                        if st.stale.get(&key) != Some(etag) {
                            st.stale.insert(key.clone(), *etag);
                            st.events.push(StoreEvent::RescanRejected {
                                store: m.info.id.clone(),
                                path,
                                reason: format!("{reason}; the last valid state stays"),
                            });
                        }
                        entries.push(good);
                    } else if st.rejected.get(&key) != Some(etag) {
                        st.rejected.insert(key, *etag);
                        st.events.push(StoreEvent::RescanRejected {
                            store: m.info.id.clone(),
                            path,
                            reason: format!("{reason}; {shown} has no last valid state, so the file is ignored"),
                        });
                    }
                }
            }
        }
        d.entries = entries;
        for key in &d.removed_entries {
            st.stale.remove(key);
            st.rejected.remove(key);
            st.known_entries.remove(key);
        }
        let mut folders = Vec::with_capacity(d.folders.len());
        for f in std::mem::take(&mut d.folders) {
            match &f {
                FolderState::Valid(_) => {
                    st.bad_folders.remove(f.id());
                    st.known_folders.insert(f.id().clone(), f.clone());
                    folders.push(f);
                }
                FolderState::Invalid { id, errors } => {
                    let first = st.bad_folders.insert(id.clone());
                    if first {
                        st.events.push(StoreEvent::RescanRejected {
                            store: m.info.id.clone(),
                            path: format!("{root}/{id}/_folder.json"),
                            reason: reasons(errors),
                        });
                    }
                    if let Some(good) = st.known_folders.get(id).cloned() {
                        folders.push(good);
                    }
                }
            }
        }
        d.folders = folders;
        for id in &d.removed_folders {
            st.bad_folders.remove(id);
            st.known_folders.remove(id);
        }
    }

    /// Drops what a store adds after the start and another store already holds: a folder id or a uid that exists in
    /// two stores (5.41).
    fn reject_clashes(&self, st: &mut State, i: usize, d: &mut Delta) {
        let m = self.member(i);
        let mut dropped_folders: BTreeSet<FolderId> = BTreeSet::new();
        d.folders.retain(|f| match st.folder_owner.get(f.id()) {
            Some(&o) if o != i => {
                dropped_folders.insert(f.id().clone());
                st.events.push(StoreEvent::RescanRejected {
                    store: m.info.id.clone(),
                    path: f.id().to_string(),
                    reason: format!("the folder id is in the store \"{}\"", self.member(o).info.id),
                });
                false
            }
            _ => true,
        });
        d.entries.retain(|e| {
            if dropped_folders.contains(e.folder()) {
                return false;
            }
            match st.uid_owner.get(e.uid()) {
                Some(&o) if o != i => {
                    st.events.push(StoreEvent::RescanRejected {
                        store: m.info.id.clone(),
                        path: format!("{}/{}", e.folder(), e.uid()),
                        reason: format!("the uid is in the store \"{}\"", self.member(o).info.id),
                    });
                    false
                }
                _ => true,
            }
        });
        d.marks.folders.retain(|id, _| !dropped_folders.contains(id));
        d.marks.entries.retain(|key, _| {
            !dropped_folders.contains(&key.1) && st.uid_owner.get(&key.0).is_none_or(|o| *o == i)
        });
    }
}

impl Store for Stores {
    async fn load(&self) -> Result<Snapshot, StoreError> {
        let first = self.state().first.take();
        if let Some(snap) = first {
            return Ok(snap);
        }
        // No checked first load: read and merge, with no start rule.
        let mut merged = Snapshot { folders: Vec::new(), entries: Vec::new(), marks: Marks::empty() };
        let mut owners: Vec<(usize, Snapshot)> = Vec::new();
        for (i, m) in self.inner.members.iter().enumerate() {
            owners.push((i, m.store.load().await?));
        }
        {
            let mut st = self.state();
            for (i, snap) in &owners {
                for f in &snap.folders {
                    st.folder_owner.insert(f.id().clone(), *i);
                }
                for e in &snap.entries {
                    st.uid_owner.insert(e.uid().clone(), *i);
                }
            }
        }
        for (_, snap) in owners {
            merged.folders.extend(snap.folders);
            merged.entries.extend(snap.entries);
            merged.marks.folders.extend(snap.marks.folders);
            merged.marks.entries.extend(snap.marks.entries);
        }
        merged.folders.sort_by(|a, b| a.id().cmp(b.id()));
        merged.entries.sort_by(|a, b| (a.uid(), a.folder()).cmp(&(b.uid(), b.folder())));
        Ok(merged)
    }

    async fn poll(&self, marks: &Marks) -> Result<Delta, StoreError> {
        let now = Instant::now();
        let work: Vec<(usize, Marks)> = {
            let st = self.state();
            (0..self.inner.members.len())
                .filter(|i| st.due[*i].is_none_or(|t| now >= t))
                .map(|i| (i, self.marks_of(&st, marks, i)))
                .collect()
        };
        let mut results = Vec::with_capacity(work.len());
        for (i, sub) in &work {
            let d = self.member(*i).store.poll(sub).await?;
            results.push((*i, d));
        }
        let mut st = self.state();
        let mut out_marks = marks.clone();
        let mut out = Delta {
            folders: Vec::new(),
            removed_folders: Vec::new(),
            entries: Vec::new(),
            removed_entries: Vec::new(),
            marks: Marks::empty(),
        };
        for ((i, mut d), (_, old_sub)) in results.into_iter().zip(&work) {
            if self.member(i).info.kind == StoreKind::Volume {
                self.filter_volume_delta(&mut st, i, &mut d);
            }
            self.reject_clashes(&mut st, i, &mut d);
            for id in old_sub.folders.keys() {
                out_marks.folders.remove(id);
            }
            for key in old_sub.entries.keys() {
                out_marks.entries.remove(key);
            }
            for id in &d.removed_folders {
                st.folder_owner.remove(id);
            }
            for f in &d.folders {
                st.folder_owner.insert(f.id().clone(), i);
            }
            for key in &d.removed_entries {
                st.uid_owner.remove(&key.0);
            }
            for e in &d.entries {
                st.uid_owner.insert(e.uid().clone(), i);
            }
            out_marks.folders.extend(d.marks.folders);
            out_marks.entries.extend(d.marks.entries);
            out.folders.extend(d.folders);
            out.removed_folders.extend(d.removed_folders);
            out.entries.extend(d.entries);
            out.removed_entries.extend(d.removed_entries);
            st.due[i] = Some(now + self.member(i).interval);
        }
        out.marks = out_marks;
        Ok(out)
    }

    async fn get(&self, uid: &Uid) -> Result<Current, StoreError> {
        match self.route_uid(uid) {
            Some(i) => self.member(i).store.get(uid).await,
            None => Err(StoreError::NotFound),
        }
    }

    async fn raw_current(&self, uid: &Uid, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
        let owner = self.state().folder_owner.get(folder).copied();
        match owner.or_else(|| self.route_uid(uid)) {
            Some(i) => self.member(i).store.raw_current(uid, folder).await,
            None => Err(StoreError::NotFound),
        }
    }

    async fn create(
        &self,
        folder: &FolderId,
        doc: &CanonicalDocument,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let store = self.write_folder(folder)?;
        // A uid of another store is not free: the uid is unique across the stores (5.41).
        if self.state().uid_owner.get(doc.uid()).is_some_and(|o| Some(*o) != self.inner.primary) {
            return Err(StoreError::Exists { folder: None });
        }
        let saved = store.create(folder, doc, kind, meta).await?;
        self.note_uid(doc.uid());
        Ok(saved)
    }

    async fn save(
        &self,
        doc: &CanonicalDocument,
        if_match: &Etag,
        expected_folder: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.write_uid(doc.uid())?.save(doc, if_match, expected_folder, kind, meta).await
    }

    async fn move_to(
        &self,
        uid: &Uid,
        folder: &FolderId,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let store = self.write_uid(uid)?;
        self.write_folder(folder)?;
        store.move_to(uid, folder, if_match, expected_folder, meta).await
    }

    async fn delete(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected_folder: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.write_uid(uid)?.delete(uid, if_match, expected_folder, meta).await
    }

    async fn restore(
        &self,
        uid: &Uid,
        version: u64,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let saved = self.write_uid(uid)?.restore(uid, version, if_match, expected_folder, meta).await?;
        self.note_uid(uid);
        Ok(saved)
    }

    async fn history(&self, uid: &Uid, before: Option<u64>, limit: u32) -> Result<Vec<VersionMeta>, StoreError> {
        // Only the primary store has history (5.40).
        let Some(p) = self.inner.primary else { return Ok(Vec::new()) };
        if self.uid_is_read_only(uid) {
            return Ok(Vec::new());
        }
        self.member(p).store.history(uid, before, limit).await
    }

    async fn version(&self, uid: &Uid, version: u64) -> Result<Version, StoreError> {
        let Some(p) = self.inner.primary else { return Err(StoreError::VersionNotFound) };
        if self.uid_is_read_only(uid) {
            return Err(StoreError::VersionNotFound);
        }
        self.member(p).store.version(uid, version).await
    }

    async fn create_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        let Some(p) = self.inner.primary else { return Err(StoreError::ReadOnly) };
        // The folder id is unique across the stores (5.41).
        if self.state().folder_owner.get(id).is_some_and(|o| *o != p) {
            return Err(StoreError::Exists { folder: None });
        }
        self.check_title(title)?;
        self.member(p).store.create_folder(id, title).await?;
        self.note_folder(id);
        Ok(())
    }

    async fn rename_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        let store = self.write_folder(id)?;
        self.check_title(title)?;
        store.rename_folder(id, title).await
    }

    async fn delete_folder(&self, id: &FolderId) -> Result<(), StoreError> {
        self.write_folder(id)?.delete_folder(id).await
    }

    async fn export_begin(&self) -> Result<Exporter, StoreError> {
        match self.primary() {
            Some(s) => s.export_begin().await,
            None => Err(StoreError::ReadOnly),
        }
    }

    async fn import_begin(&self) -> Result<Importer, StoreError> {
        match self.primary() {
            Some(s) => s.import_begin().await,
            None => Err(StoreError::ReadOnly),
        }
    }

    async fn restore_valid(
        &self,
        version: &ValidVersion,
        if_match: Option<&Etag>,
        expected_folder: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let uid = version.document().uid().clone();
        let saved = self.write_uid(&uid)?.restore_valid(version, if_match, expected_folder, meta).await?;
        self.note_uid(&uid);
        Ok(saved)
    }

    async fn entry_probe(&self, uid: &Uid) -> Result<EntryProbe, StoreError> {
        match self.route_uid(uid) {
            Some(i) => self.member(i).store.entry_probe(uid).await,
            None => Ok(EntryProbe { entries: Vec::new() }),
        }
    }

    async fn folder_probe(&self) -> Result<Vec<(FolderState, FolderMark)>, StoreError> {
        // Only the primary store changes with an own write.
        match self.primary() {
            Some(s) => s.folder_probe().await,
            None => Ok(Vec::new()),
        }
    }

    async fn check_consistency(&self) -> Result<Vec<Problem>, StoreError> {
        match self.primary() {
            Some(s) => s.check_consistency().await,
            None => Ok(Vec::new()),
        }
    }
}

impl From<AnyStore> for Stores {
    fn from(store: AnyStore) -> Self {
        let kind = match store {
            AnyStore::Volume(_) => StoreKind::Volume,
            AnyStore::Postgres(_) => StoreKind::Postgres,
        };
        Stores::single(store, kind.name(), kind)
    }
}
