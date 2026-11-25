// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dashboards without a folder in a volume (5.9, 5.10 rules 5 and 6): a file `<uid>.json` at the root of the volume.
#![cfg(feature = "conformance")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hune_core::ids::FolderId;
use hune_store::conformance::{doc, doc_text, fid, meta, uid};
use hune_store::volume::{VolumeOptions, VolumeStore};
use hune_store::{AnyStore, CatalogEntry, FolderTitle, Store, StoreError, VersionKind};

fn dir() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(d.path()).unwrap();
    (d, root)
}

async fn open(root: &Path) -> AnyStore {
    let opts = VolumeOptions {
        root: root.to_path_buf(),
        read_only: false,
        rescan_interval: Duration::from_secs(1),
        history_keep: 100,
    };
    AnyStore::Volume(VolumeStore::open(opts).await.unwrap())
}

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).unwrap()
}

#[tokio::test]
async fn a_dashboard_without_a_folder_is_written_at_the_root_and_read_back() {
    let (_d, root) = dir();
    let s = open(&root).await;
    let saved =
        s.create(&FolderId::root(), &doc("rooted", "At the root"), VersionKind::Create, &meta(1)).await.unwrap();
    assert!(saved.folder.is_root());
    assert!(root.join("rooted.json").is_file(), "the file is at the root of the volume");
    let cur = s.get(&uid("rooted")).await.unwrap();
    assert!(cur.folder.is_root());
    assert_eq!(cur.folder.wire(), None);
    let snap = s.load().await.unwrap();
    assert_eq!(snap.folders.len(), 0, "the root is not a folder");
    assert_eq!(snap.entries.len(), 1);
    assert!(snap.entries[0].folder().is_root());
}

#[tokio::test]
async fn a_new_store_reads_the_files_at_the_root_as_dashboards() {
    let (_d, root) = dir();
    fs::write(root.join("cpu.json"), doc_text("cpu", "Cpu")).unwrap();
    let s = open(&root).await;
    let cur = s.get(&uid("cpu")).await.unwrap();
    assert!(cur.folder.is_root());
}

#[tokio::test]
async fn a_file_at_the_root_that_is_no_dashboard_is_an_invalid_entry_and_another_name_is_ignored() {
    let (_d, root) = dir();
    // `package.json` has a stem that fits the uid rule: it is read as a dashboard, and it is not valid (5.10, rule 5).
    fs::write(root.join("package.json"), "{\"name\": \"x\"}").unwrap();
    // A name that does not fit the uid rule, and a file that is not json, are ignored.
    fs::write(root.join("Notes.json"), "{}").unwrap();
    fs::write(root.join("README.md"), "text").unwrap();
    let s = open(&root).await;
    let snap = s.load().await.unwrap();
    let names: Vec<(String, bool)> =
        snap.entries.iter().map(|e| (e.uid().to_string(), matches!(&**e, CatalogEntry::Invalid { .. }))).collect();
    assert_eq!(names, vec![("package".to_owned(), true)]);
}

#[tokio::test]
async fn a_uid_at_the_root_and_in_a_folder_is_not_unique() {
    let (_d, root) = dir();
    let s = open(&root).await;
    s.create_folder(&fid("ops"), &title("Ops")).await.unwrap();
    s.create(&fid("ops"), &doc("twin", "In a folder"), VersionKind::Create, &meta(1)).await.unwrap();
    drop(s);
    fs::write(root.join("twin.json"), doc_text("twin", "At the root")).unwrap();
    let s = open(&root).await;
    let snap = s.load().await.unwrap();
    let twins: Vec<_> = snap.entries.iter().filter(|e| e.uid().as_str() == "twin").collect();
    assert_eq!(twins.len(), 2);
    for e in twins {
        match &**e {
            CatalogEntry::Invalid { errors, .. } => assert_eq!(errors[0].code.to_string(), "not_unique"),
            CatalogEntry::Valid { .. } => panic!("a uid in two places must be invalid"),
        }
    }
}

#[tokio::test]
async fn a_move_goes_from_the_root_to_a_folder_and_back() {
    let (_d, root) = dir();
    let s = open(&root).await;
    s.create_folder(&fid("ops"), &title("Ops")).await.unwrap();
    let saved = s.create(&FolderId::root(), &doc("mover", "Mover"), VersionKind::Create, &meta(1)).await.unwrap();
    let etag = saved.etag.unwrap();
    let moved = s.move_to(&uid("mover"), &fid("ops"), &etag, &FolderId::root(), &meta(2)).await.unwrap();
    assert_eq!(moved.folder, fid("ops"));
    assert!(root.join("ops/mover.json").is_file());
    assert!(!root.join("mover.json").exists());
    let back = s.move_to(&uid("mover"), &FolderId::root(), &etag, &fid("ops"), &meta(3)).await.unwrap();
    assert!(back.folder.is_root());
    assert!(root.join("mover.json").is_file());
    assert!(!root.join("ops/mover.json").exists());
    // A move to the place where the dashboard is writes nothing (6.19).
    let noop = s.move_to(&uid("mover"), &FolderId::root(), &etag, &FolderId::root(), &meta(4)).await.unwrap();
    assert_eq!(noop.outcome, hune_store::Outcome::Unchanged);
}

#[tokio::test]
async fn the_history_of_a_root_dashboard_has_a_null_folder_in_its_metadata_file() {
    let (_d, root) = dir();
    let s = open(&root).await;
    s.create(&FolderId::root(), &doc("hist", "History"), VersionKind::Create, &meta(1)).await.unwrap();
    let text = fs::read_to_string(root.join(".history/hist/1.meta.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(value["folder"].is_null(), "{text}");
    let h = s.history(&uid("hist"), None, 10).await.unwrap();
    assert_eq!(h.len(), 1);
    assert!(h[0].folder.is_root());
    // A new store reads the same history.
    drop(s);
    let s = open(&root).await;
    assert!(s.history(&uid("hist"), None, 10).await.unwrap()[0].folder.is_root());
}

#[tokio::test]
async fn a_delete_removes_the_file_at_the_root_and_a_restore_brings_it_back_to_the_root() {
    let (_d, root) = dir();
    let s = open(&root).await;
    let saved = s.create(&FolderId::root(), &doc("gone", "Gone"), VersionKind::Create, &meta(1)).await.unwrap();
    s.delete(&uid("gone"), &saved.etag.unwrap(), &FolderId::root(), &meta(2)).await.unwrap();
    assert!(!root.join("gone.json").exists());
    assert!(matches!(s.get(&uid("gone")).await, Err(StoreError::NotFound)));
    let back = s.restore(&uid("gone"), 1, None, None, &meta(3)).await.unwrap();
    assert!(back.folder.is_root());
    assert!(root.join("gone.json").is_file());
}

#[tokio::test]
async fn the_root_comes_first_in_the_order_of_places() {
    let (_d, root) = dir();
    let s = open(&root).await;
    s.create_folder(&fid("aaa"), &title("A")).await.unwrap();
    s.create(&fid("aaa"), &doc("in-a", "In A"), VersionKind::Create, &meta(1)).await.unwrap();
    s.create(&FolderId::root(), &doc("zz-root", "Root"), VersionKind::Create, &meta(2)).await.unwrap();
    let snap = s.load().await.unwrap();
    let mut places: Vec<FolderId> = snap.entries.iter().map(|e| e.folder().clone()).collect();
    places.sort();
    assert!(places[0].is_root());
}
