//! A restore validates its version one time, never under the store lock (6.18, decision ID-K321).

use super::{VolumeOptions, VolumeStore};
use crate::types::{
    CanonicalDocument, FolderId, FolderTitle, Message, StoreError, Subject, Uid, UtcMillis, ValidVersion, VersionKind,
    WriteMeta,
};
use std::time::Duration;

const NODES: &str = include_str!("../../../../tests/fixtures/dashboards/nodes.json");

fn doc(title: &str) -> CanonicalDocument {
    let text = NODES.replacen("\"title\": \"Nodes\"", &format!("\"title\": \"{title}\""), 1);
    hune_core::document::validate(text.as_bytes(), None).expect("the fixture is valid")
}

fn meta() -> WriteMeta {
    WriteMeta {
        saved_by: Subject::anonymous(),
        saved_at: UtcMillis::from_millis(1_794_211_200_000),
        message: Message::default(),
        save_id: None,
    }
}

async fn store(dir: &tempfile::TempDir, history_keep: u32) -> VolumeStore {
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let opts = VolumeOptions { root, read_only: false, rescan_interval: Duration::from_secs(5), history_keep };
    let s = VolumeStore::open(opts).await.expect("open");
    s.create_folder(&FolderId::parse("f").unwrap(), &FolderTitle::parse("F").unwrap()).await.unwrap();
    s
}

/// `Store::restore` validates the version one time, before the lock. `restore_valid` takes the valid document of the
/// caller and validates nothing: the API restore thus validates its version one time. Before the fix, the store
/// validated the version under its exclusive lock, also after the validation of the caller.
#[tokio::test]
async fn a_restore_validates_its_version_one_time() {
    let dir = tempfile::tempdir().unwrap();
    let s = store(&dir, 100).await;
    let uid = Uid::parse("nodes").unwrap();
    let (d1, d2) = (doc("First"), doc("Second"));
    s.create(&FolderId::parse("f").unwrap(), &d1, VersionKind::Create, &meta()).await.unwrap();
    s.save(&d2, d1.etag(), &FolderId::parse("f").unwrap(), VersionKind::Save, &meta()).await.unwrap();
    // The path of `hune restore`: one validation in the store.
    let saved = s.restore(&uid, 1, Some(d2.etag()), None, &meta()).await.unwrap();
    assert_eq!((saved.version, saved.etag), (3, Some(*d1.etag())));
    assert_eq!(s.restore_checks(), 1);
    // The path of the API: the caller validates, the store validates nothing.
    let valid = ValidVersion::check(&uid, &s.version(&uid, 2).await.unwrap()).unwrap();
    let saved = s.restore_valid(&valid, Some(d1.etag()), None, &meta()).await.unwrap();
    assert_eq!((saved.version, saved.etag), (4, Some(*d2.etag())));
    assert_eq!(s.restore_checks(), 1, "restore_valid validated the version");
    // A wrong `If-Match`: the store gives the conflict and validates nothing.
    let r = s.restore_valid(&valid, Some(d1.etag()), None, &meta()).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "{r:?}");
    assert_eq!(s.restore_checks(), 1);
}

/// Under its lock, the store reads the metadata of the version again: a version that the retention removed after the
/// read of the caller gives `VersionNotFound`, and a tombstone gives `NotRestorable` (5.4, item 4).
#[tokio::test]
async fn the_store_checks_the_version_again_under_its_lock() {
    let dir = tempfile::tempdir().unwrap();
    let s = store(&dir, 2).await;
    let uid = Uid::parse("nodes").unwrap();
    let f = FolderId::parse("f").unwrap();
    let (d1, d2, d3) = (doc("First"), doc("Second"), doc("Third"));
    s.create(&f, &d1, VersionKind::Create, &meta()).await.unwrap();
    let valid = ValidVersion::check(&uid, &s.version(&uid, 1).await.unwrap()).unwrap();
    s.save(&d2, d1.etag(), &f, VersionKind::Save, &meta()).await.unwrap();
    // Keep 2: this write removes version 1.
    s.save(&d3, d2.etag(), &f, VersionKind::Save, &meta()).await.unwrap();
    let r = s.restore_valid(&valid, Some(d3.etag()), Some(&f), &meta()).await;
    assert_eq!(r, Err(StoreError::VersionNotFound));
    // A tombstone has no bytes: `ValidVersion::check` refuses it, and so does the store under its lock.
    s.delete(&uid, d3.etag(), &f, &meta()).await.unwrap();
    let tomb = s.version(&uid, 4).await.unwrap();
    assert_eq!(ValidVersion::check(&uid, &tomb), Err(StoreError::NotRestorable));
    let r = s.restore(&uid, 4, None, None, &meta()).await;
    assert_eq!(r, Err(StoreError::NotRestorable));
    assert_eq!(s.restore_checks(), 1);
}
