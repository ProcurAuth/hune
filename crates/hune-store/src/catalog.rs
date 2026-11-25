// SPDX-License-Identifier: MIT OR Apache-2.0
//! The catalog (5.8): the copy of the store in the memory of the instance.

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::store::AnyStore;
use crate::store::{Delta, EntryKey, FolderState, Marks, Snapshot, Store, StoreEvent};
use crate::stores::Stores;
use crate::types::{CatalogEntry, FolderId, FolderRecord, StoreError, Uid, ValidationError};

/// A change of the sync state, for the log lines `store.sync_failed` and `store.sync_recovered`
/// and the metric `hune_store_up` (5.8, item 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTransition {
    /// The first failed poll after a success: log `store.sync_failed`, set `hune_store_up` to 0.
    Failed,
    /// The first successful poll after a failure: log `store.sync_recovered`, set `hune_store_up` to 1.
    Recovered {
        /// The time since the first failed poll.
        down: Duration,
    },
}

/// The counts of the changes that one poll applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PollSummary {
    /// Folders new or changed.
    pub folders_changed: usize,
    /// Folders removed.
    pub folders_removed: usize,
    /// Entries new or changed.
    pub entries_changed: usize,
    /// Entries removed.
    pub entries_removed: usize,
}

/// The result of one poll.
#[derive(Debug)]
pub struct PollResult {
    /// The changes that the poll applied, or the error. The catalog keeps its content after an error.
    pub outcome: Result<PollSummary, StoreError>,
    /// The change of the sync state, when there is one.
    pub transition: Option<SyncTransition>,
    /// The events of the store for the log since the last poll.
    pub events: Vec<StoreEvent>,
}

/// The counts of the content of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CatalogStats {
    /// Valid folders.
    pub folders: usize,
    /// Invalid folders.
    pub invalid_folders: usize,
    /// Valid entries.
    pub dashboards: usize,
    /// Invalid entries.
    pub invalid: usize,
}

struct State {
    folders: BTreeMap<FolderId, FolderState>,
    entries: BTreeMap<EntryKey, Arc<CatalogEntry>>,
    marks: Arc<Marks>,
    up: bool,
    down_since: Option<Instant>,
}

/// The catalog. It uses interior mutability: no reader holds a lock across an `.await`.
pub struct Catalog {
    state: RwLock<State>,
    pending: Mutex<Vec<StoreEvent>>,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog").field("stats", &self.stats()).finish()
    }
}

fn first_key(uid: &Uid) -> Option<EntryKey> {
    // "0" is the smallest folder id.
    FolderId::parse("0").ok().map(|f| (uid.clone(), f))
}

impl State {
    fn remove_uid(&mut self, uid: &Uid) {
        let Some(start) = first_key(uid) else { return };
        let keys: Vec<EntryKey> = self
            .entries
            .range((Bound::Included(start), Bound::Unbounded))
            .map(|(k, _)| k)
            .take_while(|k| k.0 == *uid)
            .cloned()
            .collect();
        let marks = Arc::make_mut(&mut self.marks);
        for k in keys {
            self.entries.remove(&k);
            marks.entries.remove(&k);
        }
    }

    fn apply(&mut self, d: Delta) -> PollSummary {
        let summary = PollSummary {
            folders_changed: d.folders.len(),
            folders_removed: d.removed_folders.len(),
            entries_changed: d.entries.len(),
            entries_removed: d.removed_entries.len(),
        };
        for f in d.removed_folders {
            self.folders.remove(&f);
        }
        for f in d.folders {
            self.folders.insert(f.id().clone(), f);
        }
        for k in d.removed_entries {
            self.entries.remove(&k);
        }
        for e in d.entries {
            self.entries.insert((e.uid().clone(), e.folder().clone()), e);
        }
        self.marks = Arc::new(d.marks);
        summary
    }
}

/// A store that the catalog reads: the calls of `Store` and the events that it holds for the log.
pub trait Source: Store {
    /// Takes the events that the store holds for the log.
    fn drain_events(&self) -> Vec<StoreEvent>;
}

impl Source for AnyStore {
    fn drain_events(&self) -> Vec<StoreEvent> {
        AnyStore::drain_events(self)
    }
}

impl Source for Stores {
    fn drain_events(&self) -> Vec<StoreEvent> {
        Stores::drain_events(self)
    }
}

impl Catalog {
    fn read(&self) -> std::sync::RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(|e| e.into_inner())
    }

    fn keep_events<S: Source>(&self, store: &S) {
        let events = store.drain_events();
        if !events.is_empty() {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).extend(events);
        }
    }

    /// Reads the store one time with `Store::load` (5.8, item 1).
    pub async fn load<S: Source>(store: &S) -> Result<Catalog, StoreError> {
        let snap: Snapshot = store.load().await?;
        let mut st = State {
            folders: BTreeMap::new(),
            entries: BTreeMap::new(),
            marks: Arc::new(snap.marks),
            up: true,
            down_since: None,
        };
        for f in snap.folders {
            st.folders.insert(f.id().clone(), f);
        }
        for e in snap.entries {
            st.entries.insert((e.uid().clone(), e.folder().clone()), e);
        }
        let catalog = Catalog { state: RwLock::new(st), pending: Mutex::default() };
        catalog.keep_events(store);
        Ok(catalog)
    }

    /// Takes the events that came with the load and the refreshes.
    pub fn take_events(&self) -> Vec<StoreEvent> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Calls `Store::poll` with the marks and applies the changes (5.8, items 2 and 6).
    pub async fn poll<S: Source>(&self, store: &S) -> PollResult {
        let marks = self.read().marks.clone();
        let result = store.poll(&marks).await;
        self.keep_events(store);
        let (outcome, transition) = {
            let mut st = self.write();
            match result {
                Ok(delta) => {
                    let summary = st.apply(delta);
                    let transition = if st.up {
                        None
                    } else {
                        let down = st.down_since.take().map(|t| t.elapsed()).unwrap_or_default();
                        st.up = true;
                        Some(SyncTransition::Recovered { down })
                    };
                    (Ok(summary), transition)
                }
                Err(e) => {
                    let transition = if st.up {
                        st.up = false;
                        st.down_since = Some(Instant::now());
                        Some(SyncTransition::Failed)
                    } else {
                        None
                    };
                    (Err(e), transition)
                }
            }
        };
        let events = self.take_events();
        PollResult { outcome, transition, events }
    }

    /// Tells if the last poll succeeded (the metric `hune_store_up`).
    pub fn is_up(&self) -> bool {
        self.read().up
    }

    /// The valid folders, sorted by id.
    pub fn folders(&self) -> Vec<FolderRecord> {
        self.read()
            .folders
            .values()
            .filter_map(|f| match f {
                FolderState::Valid(r) => Some(r.clone()),
                FolderState::Invalid { .. } => None,
            })
            .collect()
    }

    /// The invalid folders with their errors, sorted by id.
    pub fn invalid_folders(&self) -> Vec<(FolderId, Vec<ValidationError>)> {
        self.read()
            .folders
            .values()
            .filter_map(|f| match f {
                FolderState::Invalid { id, errors } => Some((id.clone(), errors.clone())),
                FolderState::Valid(_) => None,
            })
            .collect()
    }

    /// All entries, sorted by uid and then by folder.
    pub fn entries(&self) -> Vec<Arc<CatalogEntry>> {
        self.read().entries.values().cloned().collect()
    }

    /// The entry of a uid. When two folders hold the uid, the entry of the first folder.
    pub fn get(&self, uid: &Uid) -> Option<Arc<CatalogEntry>> {
        let start = first_key(uid)?;
        let st = self.read();
        st.entries
            .range((Bound::Included(start), Bound::Unbounded))
            .next()
            .filter(|(k, _)| k.0 == *uid)
            .map(|(_, e)| e.clone())
    }

    /// The counts of the content.
    pub fn stats(&self) -> CatalogStats {
        let st = self.read();
        let mut s = CatalogStats::default();
        for f in st.folders.values() {
            match f {
                FolderState::Valid(_) => s.folders += 1,
                FolderState::Invalid { .. } => s.invalid_folders += 1,
            }
        }
        for e in st.entries.values() {
            match **e {
                CatalogEntry::Valid { .. } => s.dashboards += 1,
                CatalogEntry::Invalid { .. } => s.invalid += 1,
            }
        }
        s
    }

    /// Reads one uid again after an own write, so that the write is visible before the answer
    /// (5.8, item 3). Gives false when the read failed: the next poll then finds the change.
    pub async fn refresh_uid<S: Source>(&self, store: &S, uid: &Uid) -> bool {
        let probe = store.entry_probe(uid).await;
        self.keep_events(store);
        let Ok(probe) = probe else { return false };
        let mut guard = self.write();
        let st = &mut *guard;
        st.remove_uid(uid);
        let marks = Arc::make_mut(&mut st.marks);
        for (entry, mark) in probe.entries {
            let key = (entry.uid().clone(), entry.folder().clone());
            marks.entries.insert(key.clone(), mark);
            st.entries.insert(key, entry);
        }
        true
    }

    /// Reads all folders again after an own folder write. Gives false when the read failed.
    pub async fn refresh_folders<S: Source>(&self, store: &S) -> bool {
        let probe = store.folder_probe().await;
        self.keep_events(store);
        let Ok(probe) = probe else { return false };
        let mut guard = self.write();
        let st = &mut *guard;
        st.folders.clear();
        let marks = Arc::make_mut(&mut st.marks);
        marks.folders.clear();
        for (state, mark) in probe {
            marks.folders.insert(state.id().clone(), mark);
            st.folders.insert(state.id().clone(), state);
        }
        true
    }
}
