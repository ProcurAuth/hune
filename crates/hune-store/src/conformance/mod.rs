//! The conformance suite (5.33 to 5.36). The common tests of 5.34 are functions in
//! [`common`]. Each one gets a new empty store from a [`Backend`], so the PostgreSQL backend
//! can run the same functions (M5). The volume tests of 5.35 are in `tests/volume_suite.rs`.
//!
//! The suite compares documents with the result of `hune_core::document::validate`. SC-43
//! is the one test that compares bytes with the fixture file. It does so only when the core
//! writes the canonical form of the fixture.

pub mod common;
pub mod pg;
pub mod transfer;

use std::future::Future;
use std::path::Path;

use hune_core::document::{CanonicalDocument, validate};

use crate::store::AnyStore;
use crate::types::{FolderId, SaveId, Subject, Uid, UtcMillis, VersionMeta, WriteMeta};
use crate::volume::testing::VolumeHooks;
use crate::volume::{VolumeOptions, VolumeStore};

/// The fixture of 4.40: the document `nodes` of the folder `transit`.
pub const NODES_JSON: &str = include_str!("../../../../tests/fixtures/dashboards/nodes.json");

/// A new empty store with its test hooks (5.33, items 2 and 3).
pub struct Fixture {
    /// The store.
    pub store: AnyStore,
    /// The hooks.
    pub hooks: Hooks,
}

/// A factory of empty stores. The tests call it once for each store they need.
pub trait Backend: Send + Sync {
    /// Makes a new empty store with the retention `history_keep`.
    fn new_store(&self, history_keep: u32) -> impl Future<Output = Fixture> + Send;
}

/// The test hooks of a backend (5.33, item 3). A hook changes the backend outside the trait.
pub enum Hooks {
    /// The volume backend.
    Volume(VolumeHooks),
    /// The PostgreSQL backend.
    Postgres(pg::PgHooks),
}

impl Hooks {
    /// Replaces the current document of a uid with outside bytes, as `git pull` does.
    pub async fn outside_write(&self, folder: &FolderId, uid: &Uid, bytes: &[u8]) {
        match self {
            Hooks::Volume(h) => h.outside_write(folder, uid, bytes).expect("outside write"),
            Hooks::Postgres(h) => h.outside_write(folder, uid, bytes).await,
        }
    }

    /// Makes the next write fail with `Unavailable` after its version is durable and before its
    /// effect is visible.
    pub async fn break_next_write(&self) {
        match self {
            Hooks::Volume(h) => h.break_next_write(),
            Hooks::Postgres(h) => h.break_next_write().await,
        }
    }

    /// Writes one history version outside the trait.
    pub async fn inject_version(&self, meta: &VersionMeta, bytes: Option<&[u8]>) {
        match self {
            Hooks::Volume(h) => h.inject_version(meta, bytes).expect("inject version"),
            Hooks::Postgres(h) => h.inject_version(meta, bytes).await,
        }
    }

    /// Fills the store with `n` folders, outside the trait.
    pub async fn fill_folders(&self, n: usize) {
        match self {
            Hooks::Volume(h) => h.fill_folders(n).expect("fill folders"),
            Hooks::Postgres(h) => h.fill_folders(n).await,
        }
    }

    /// Fills a folder with the dashboards of the list, outside the trait.
    pub async fn fill_dashboards(&self, folder: &FolderId, docs: impl Iterator<Item = (Uid, Vec<u8>)>) {
        match self {
            Hooks::Volume(h) => h.fill_dashboards(folder, docs).expect("fill dashboards"),
            Hooks::Postgres(h) => h.fill_dashboards(folder, docs).await,
        }
    }

    /// Opens a second store on the same data. It sees the changes of the first store after a poll.
    pub async fn second_store(&self) -> AnyStore {
        match self {
            Hooks::Volume(h) => open_reader(h.root()).await,
            Hooks::Postgres(h) => h.second_store().await,
        }
    }
}

/// Opens a read-only volume store on a root.
pub async fn open_reader(root: &Path) -> AnyStore {
    let opts = VolumeOptions {
        root: root.to_owned(),
        read_only: true,
        rescan_interval: std::time::Duration::from_secs(1),
        history_keep: 100,
    };
    AnyStore::Volume(VolumeStore::open(opts).await.expect("open the second store"))
}

// Helpers for the tests.

/// A checked uid.
pub fn uid(s: &str) -> Uid {
    Uid::parse(s).expect("test uid")
}

/// A checked folder id.
pub fn fid(s: &str) -> FolderId {
    FolderId::parse(s).expect("test folder id")
}

/// A save id made from a number.
pub fn sid(n: u64) -> SaveId {
    let mut b = [0u8; 16];
    b[8..].copy_from_slice(&n.to_be_bytes());
    b[6] = 0x40;
    b[8] |= 0x80;
    SaveId::from_bytes(b)
}

/// The text of the fixture with another uid and another title.
pub fn doc_text(uid: &str, title: &str) -> String {
    let t = NODES_JSON.replacen("  \"uid\": \"nodes\",", &format!("  \"uid\": \"{uid}\","), 1).replacen(
        "  \"title\": \"Nodes\",",
        &format!("  \"title\": \"{title}\","),
        1,
    );
    assert!(t.contains(&format!("\"uid\": \"{uid}\"")), "the fixture has no uid line");
    t
}

/// A valid document built from the fixture.
pub fn doc(uid: &str, title: &str) -> CanonicalDocument {
    validate(doc_text(uid, title).as_bytes(), None).expect("the fixture is valid")
}

/// The metadata of a write. The author is `alice`. The time has milliseconds. The save id
/// comes from `n`.
pub fn meta(n: u64) -> WriteMeta {
    WriteMeta {
        saved_by: Subject::parse("alice").expect("subject"),
        saved_at: UtcMillis::from_millis(1_794_211_200_000 + i64::try_from(n).unwrap_or(0) * 1000 + 123),
        message: crate::types::Message::parse(&format!("write {n}")).expect("message"),
        save_id: Some(sid(n)),
    }
}

/// The metadata of a write without a save id.
pub fn meta_no_sid(n: u64) -> WriteMeta {
    WriteMeta { save_id: None, ..meta(n) }
}
