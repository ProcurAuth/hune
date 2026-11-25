// SPDX-License-Identifier: MIT OR Apache-2.0
//! The stores of the process (5.40 to 5.42): one primary store and read-only stores behind one `Store`.
#![cfg(feature = "conformance")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hune_store::conformance::{doc, fid, meta, uid};
use hune_store::volume::{VolumeOptions, VolumeStore};
use hune_store::{
    AnyStore, Catalog, CatalogEntry, FolderTitle, Source, Store, StoreError, StoreEvent, StoreInfo, StoreKind,
    StoreSpec, Stores, VersionKind,
};

fn dir() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(d.path()).unwrap();
    (d, root)
}

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).unwrap()
}

async fn open(root: &Path, read_only: bool) -> AnyStore {
    let opts = VolumeOptions {
        root: root.to_path_buf(),
        read_only,
        rescan_interval: Duration::from_secs(1),
        history_keep: 100,
    };
    AnyStore::Volume(VolumeStore::open(opts).await.unwrap())
}

/// Writes a folder with dashboards in a volume, through a writable store that closes at the end.
async fn seed(root: &Path, folder: &str, folder_title: &str, dashboards: &[(&str, &str)]) {
    let s = open(root, false).await;
    s.create_folder(&fid(folder), &title(folder_title)).await.unwrap();
    for (n, (u, t)) in dashboards.iter().enumerate() {
        s.create(&fid(folder), &doc(u, t), VersionKind::Create, &meta(n as u64 + 1)).await.unwrap();
    }
}

fn spec(
    id: &str,
    store: AnyStore,
    root: &Path,
    primary: bool,
    store_title: Option<&str>,
    interval: Duration,
) -> StoreSpec {
    StoreSpec {
        info: StoreInfo { id: id.to_owned(), kind: StoreKind::Volume, title: store_title.map(str::to_owned), primary },
        path: Some(root.to_path_buf()),
        interval,
        store,
    }
}

/// A primary volume and a read-only volume "lab" with the folder `ops` and the dashboard `lab-one`.
async fn two(interval: Duration) -> (Stores, Vec<tempfile::TempDir>, PathBuf, PathBuf) {
    let (d1, main_root) = dir();
    let (d2, lab_root) = dir();
    seed(&lab_root, "ops", "Operations", &[("lab-one", "Lab one")]).await;
    let main = open(&main_root, false).await;
    main.create_folder(&fid("home"), &title("Home")).await.unwrap();
    main.create(&fid("home"), &doc("main-one", "Main one"), VersionKind::Create, &meta(1)).await.unwrap();
    let lab = open(&lab_root, true).await;
    let stores = Stores::new(vec![
        spec("main", main, &main_root, true, None, interval),
        spec("lab", lab, &lab_root, false, Some("Lab"), interval),
    ]);
    (stores, vec![d1, d2], main_root, lab_root)
}

#[tokio::test]
async fn the_first_load_counts_each_store_and_merges_them() {
    let (stores, _dirs, _m, _l) = two(Duration::from_secs(1)).await;
    let reports = stores.open_checked().await.unwrap();
    let counts: Vec<(String, usize, usize, usize)> =
        reports.iter().map(|r| (r.info.id.clone(), r.folders, r.dashboards, r.invalid)).collect();
    assert_eq!(counts, vec![("main".into(), 1, 1, 0), ("lab".into(), 1, 1, 0)]);
    let catalog = Catalog::load(&stores).await.unwrap();
    let uids: Vec<String> = catalog.entries().iter().map(|e| e.uid().as_str().to_owned()).collect();
    assert_eq!(uids, vec!["lab-one", "main-one"]);
    assert_eq!(catalog.folders().len(), 2);
    assert_eq!(stores.store_of_uid(&uid("lab-one")).unwrap().id, "lab");
    assert_eq!(stores.store_of_folder(&fid("home")).unwrap().id, "main");
    assert_eq!(stores.primary_id(), Some("main"));
    assert!(stores.uid_is_read_only(&uid("lab-one")));
    assert!(!stores.uid_is_read_only(&uid("main-one")));
    assert!(stores.folder_is_read_only(&fid("ops")));
}

#[tokio::test]
async fn a_write_in_a_read_only_store_is_refused_and_a_write_in_the_primary_works() {
    let (stores, _dirs, _m, _l) = two(Duration::from_secs(1)).await;
    stores.open_checked().await.unwrap();
    let _ = stores.load().await.unwrap();
    // The read-only dashboard.
    let current = stores.get(&uid("lab-one")).await.unwrap();
    let etag = current.etag;
    let changed = doc("lab-one", "Changed");
    let ro = |r: Result<_, StoreError>| matches!(r, Err(StoreError::ReadOnly));
    assert!(ro(stores.save(&changed, &etag, &fid("ops"), VersionKind::Save, &meta(9)).await.map(|_| ())));
    assert!(ro(stores.delete(&uid("lab-one"), &etag, &fid("ops"), &meta(9)).await.map(|_| ())));
    assert!(ro(stores.move_to(&uid("lab-one"), &fid("home"), &etag, &fid("ops"), &meta(9)).await.map(|_| ())));
    assert!(ro(stores.restore(&uid("lab-one"), 1, None, None, &meta(9)).await.map(|_| ())));
    assert!(ro(stores.create(&fid("ops"), &doc("new-one", "New"), VersionKind::Create, &meta(9)).await.map(|_| ())));
    assert!(ro(stores.rename_folder(&fid("ops"), &title("Renamed")).await));
    assert!(ro(stores.delete_folder(&fid("ops")).await));
    // A move of a dashboard of the primary store into a folder of a read-only store.
    let main_etag = stores.get(&uid("main-one")).await.unwrap().etag;
    assert!(ro(stores.move_to(&uid("main-one"), &fid("ops"), &main_etag, &fid("home"), &meta(9)).await.map(|_| ())));
    // The primary store works.
    stores.create(&fid("home"), &doc("main-two", "Main two"), VersionKind::Create, &meta(10)).await.unwrap();
    assert_eq!(stores.store_of_uid(&uid("main-two")).unwrap().id, "main");
    let c = stores.get(&uid("main-two")).await.unwrap();
    stores.save(&doc("main-two", "Main two b"), &c.etag, &fid("home"), VersionKind::Save, &meta(11)).await.unwrap();
    stores.create_folder(&fid("more"), &title("More")).await.unwrap();
    assert_eq!(stores.store_of_folder(&fid("more")).unwrap().id, "main");
    // The history exists for the primary store only.
    assert_eq!(stores.history(&uid("main-two"), None, 10).await.unwrap().len(), 2);
    assert!(stores.history(&uid("lab-one"), None, 10).await.unwrap().is_empty());
    assert!(matches!(stores.version(&uid("lab-one"), 1).await, Err(StoreError::VersionNotFound)));
}

#[tokio::test]
async fn a_folder_id_or_a_title_of_a_read_only_store_cannot_be_used_in_the_primary() {
    let (stores, _dirs, _m, _l) = two(Duration::from_secs(1)).await;
    stores.open_checked().await.unwrap();
    let _ = stores.load().await.unwrap();
    // The folder id `ops` is in the store "lab".
    let r = stores.create_folder(&fid("ops"), &title("Other")).await;
    assert!(matches!(r, Err(StoreError::Exists { .. })), "{r:?}");
    // The title `Lab` is the name of the root folder of the store "lab".
    let r = stores.create_folder(&fid("lab2"), &title("Lab")).await;
    assert!(matches!(r, Err(StoreError::Exists { .. })), "{r:?}");
    let r = stores.rename_folder(&fid("home"), &title("Lab")).await;
    assert!(matches!(r, Err(StoreError::Exists { .. })), "{r:?}");
    // A uid of another store is not free.
    let r = stores.create(&fid("home"), &doc("lab-one", "Same uid"), VersionKind::Create, &meta(9)).await;
    assert!(matches!(r, Err(StoreError::Exists { .. })), "{r:?}");
}

#[tokio::test]
async fn without_a_primary_store_every_write_is_refused() {
    let (d, root) = dir();
    seed(&root, "ops", "Operations", &[("lab-one", "Lab one")]).await;
    let stores =
        Stores::new(vec![spec("lab", open(&root, true).await, &root, false, Some("Lab"), Duration::from_secs(1))]);
    stores.open_checked().await.unwrap();
    assert_eq!(stores.primary_id(), None);
    let r = stores.create_folder(&fid("new"), &title("New")).await;
    assert!(matches!(r, Err(StoreError::ReadOnly)), "{r:?}");
    let etag = stores.get(&uid("lab-one")).await.unwrap().etag;
    let r = stores.delete(&uid("lab-one"), &etag, &fid("ops"), &meta(9)).await;
    assert!(matches!(r, Err(StoreError::ReadOnly)), "{r:?}");
    drop(d);
}

#[tokio::test]
async fn a_read_only_store_with_no_dashboard_stops_the_start() {
    let (d1, main_root) = dir();
    let (d2, lab_root) = dir();
    let stores = Stores::new(vec![
        spec("main", open(&main_root, false).await, &main_root, true, None, Duration::from_secs(1)),
        spec("lab", open(&lab_root, true).await, &lab_root, false, Some("Lab"), Duration::from_secs(1)),
    ]);
    let issue = stores.open_checked().await.unwrap_err();
    assert_eq!(issue.0, "storage.volume \"lab\" is a read-only store and has no dashboard");
    drop((d1, d2));
}

#[tokio::test]
async fn the_primary_store_can_be_empty() {
    let (d1, main_root) = dir();
    let (d2, lab_root) = dir();
    seed(&lab_root, "ops", "Operations", &[("lab-one", "Lab one")]).await;
    let stores = Stores::new(vec![
        spec("main", open(&main_root, false).await, &main_root, true, None, Duration::from_secs(1)),
        spec("lab", open(&lab_root, true).await, &lab_root, false, Some("Lab"), Duration::from_secs(1)),
    ]);
    let reports = stores.open_checked().await.unwrap();
    assert_eq!((reports[0].folders, reports[0].dashboards), (0, 0));
    drop((d1, d2));
}

#[tokio::test]
async fn a_file_that_is_not_valid_stops_the_start_for_a_volume_with_its_label_file_and_reason() {
    for primary in [false, true] {
        let (d1, main_root) = dir();
        let (d2, other_root) = dir();
        let bad_root = if primary { &main_root } else { &other_root };
        seed(bad_root, "ops", "Operations", &[("good", "Good")]).await;
        fs::write(bad_root.join("ops/bad.json"), b"{").unwrap();
        let (m, o);
        if primary {
            seed(&other_root, "x", "X", &[("other", "Other")]).await;
            m = spec("main", open(&main_root, false).await, &main_root, true, None, Duration::from_secs(1));
            o = spec("other", open(&other_root, true).await, &other_root, false, Some("Other"), Duration::from_secs(1));
        } else {
            m = spec("main", open(&main_root, false).await, &main_root, true, None, Duration::from_secs(1));
            o = spec("other", open(&other_root, true).await, &other_root, false, Some("Other"), Duration::from_secs(1));
        }
        let issue = Stores::new(vec![m, o]).open_checked().await.unwrap_err().0;
        let label = if primary { "main" } else { "other" };
        let file = bad_root.join("ops/bad.json");
        assert!(
            issue.starts_with(&format!("storage.volume \"{label}\": the file {} is not valid: ", file.display())),
            "{issue}"
        );
        drop((d1, d2));
    }
}

#[tokio::test]
async fn a_uid_or_a_folder_id_in_two_stores_stops_the_start() {
    let (d1, a_root) = dir();
    let (d2, b_root) = dir();
    seed(&a_root, "ops", "Operations", &[("same", "A")]).await;
    seed(&b_root, "lab", "Lab folder", &[("same", "B")]).await;
    let stores = Stores::new(vec![
        spec("a", open(&a_root, false).await, &a_root, true, None, Duration::from_secs(1)),
        spec("b", open(&b_root, true).await, &b_root, false, Some("B"), Duration::from_secs(1)),
    ]);
    let issue = stores.open_checked().await.unwrap_err().0;
    assert_eq!(
        issue,
        "the dashboard uid \"same\" is in the store \"a\" (folder \"ops\") and in the store \"b\" (folder \"lab\")"
    );
    drop((d1, d2));

    let (d1, a_root) = dir();
    let (d2, b_root) = dir();
    seed(&a_root, "ops", "Operations", &[("one", "A")]).await;
    seed(&b_root, "ops", "Other", &[("two", "B")]).await;
    let stores = Stores::new(vec![
        spec("a", open(&a_root, false).await, &a_root, true, None, Duration::from_secs(1)),
        spec("b", open(&b_root, true).await, &b_root, false, Some("B"), Duration::from_secs(1)),
    ]);
    let issue = stores.open_checked().await.unwrap_err().0;
    assert_eq!(issue, "the folder id \"ops\" is in the store \"a\" and in the store \"b\"");
    drop((d1, d2));
}

#[tokio::test]
async fn a_folder_of_the_primary_store_with_the_title_of_a_read_only_store_stops_the_start() {
    let (d1, a_root) = dir();
    let (d2, b_root) = dir();
    seed(&a_root, "ops", "Lab", &[("one", "A")]).await;
    seed(&b_root, "other", "Other", &[("two", "B")]).await;
    let stores = Stores::new(vec![
        spec("a", open(&a_root, false).await, &a_root, true, None, Duration::from_secs(1)),
        spec("lab", open(&b_root, true).await, &b_root, false, Some("Lab"), Duration::from_secs(1)),
    ]);
    let issue = stores.open_checked().await.unwrap_err().0;
    assert_eq!(
        issue,
        "the folder \"Lab\" of the primary store has the title of the read-only store \"lab\" (storage.volume \"lab\".title); give another name"
    );
    drop((d1, d2));
}

#[tokio::test]
async fn the_poll_keeps_the_last_valid_state_of_a_file_that_becomes_not_valid() {
    let (stores, _dirs, _m, lab_root) = two(Duration::ZERO).await;
    stores.open_checked().await.unwrap();
    let catalog = Catalog::load(&stores).await.unwrap();
    let file = lab_root.join("ops/lab-one.json");
    let good = fs::read(&file).unwrap();
    assert!(!stores.is_stale(&uid("lab-one")));

    // The file becomes not valid: the dashboard stays valid and shows the warning.
    fs::write(&file, b"{ not json").unwrap();
    let result = catalog.poll(&stores).await;
    assert!(result.outcome.is_ok(), "{:?}", result.outcome);
    let entry = catalog.get(&uid("lab-one")).unwrap();
    assert!(matches!(*entry, CatalogEntry::Valid { .. }));
    assert!(stores.is_stale(&uid("lab-one")));
    assert_eq!(stores.invalid_files(), vec![("main".to_owned(), 0), ("lab".to_owned(), 1)]);
    let rejected: Vec<&StoreEvent> =
        result.events.iter().filter(|e| matches!(e, StoreEvent::RescanRejected { .. })).collect();
    assert_eq!(rejected.len(), 1, "{:?}", result.events);
    match rejected[0] {
        StoreEvent::RescanRejected { store, path, reason } => {
            assert_eq!(store, "lab");
            assert_eq!(path, &file.display().to_string());
            assert!(reason.ends_with("the last valid state stays"), "{reason}");
        }
        _ => unreachable!(),
    }
    // The same invalid file does not give a second event.
    let again = catalog.poll(&stores).await;
    assert!(again.events.iter().all(|e| !matches!(e, StoreEvent::RescanRejected { .. })), "{:?}", again.events);
    assert!(stores.is_stale(&uid("lab-one")));

    // The file becomes valid again: the warning goes.
    fs::write(&file, &good).unwrap();
    let result = catalog.poll(&stores).await;
    assert!(result.outcome.is_ok());
    assert!(!stores.is_stale(&uid("lab-one")));
    assert_eq!(stores.invalid_files(), vec![("main".to_owned(), 0), ("lab".to_owned(), 0)]);

    // A file that is deleted leaves the catalog.
    fs::remove_file(&file).unwrap();
    catalog.poll(&stores).await.outcome.unwrap();
    assert!(catalog.get(&uid("lab-one")).is_none());
}

#[tokio::test]
async fn a_new_file_that_is_not_valid_has_no_entry_and_is_counted() {
    let (stores, _dirs, _m, lab_root) = two(Duration::ZERO).await;
    stores.open_checked().await.unwrap();
    let catalog = Catalog::load(&stores).await.unwrap();
    fs::write(lab_root.join("ops/broken.json"), b"nope").unwrap();
    let result = catalog.poll(&stores).await;
    assert!(result.outcome.is_ok());
    assert!(catalog.get(&uid("broken")).is_none());
    assert_eq!(stores.invalid_files(), vec![("main".to_owned(), 0), ("lab".to_owned(), 1)]);
    assert!(!stores.is_stale(&uid("broken")));
    assert_eq!(result.events.iter().filter(|e| matches!(e, StoreEvent::RescanRejected { .. })).count(), 1);
}

#[tokio::test]
async fn a_store_is_polled_after_its_own_period() {
    // The read-only store has a long period: a change of its files waits. The primary store has no wait.
    let (d1, main_root) = dir();
    let (d2, lab_root) = dir();
    seed(&lab_root, "ops", "Operations", &[("lab-one", "Lab one")]).await;
    let main = open(&main_root, false).await;
    main.create_folder(&fid("home"), &title("Home")).await.unwrap();
    let stores = Stores::new(vec![
        spec("main", main, &main_root, true, None, Duration::ZERO),
        spec("lab", open(&lab_root, true).await, &lab_root, false, Some("Lab"), Duration::from_secs(3600)),
    ]);
    stores.open_checked().await.unwrap();
    let catalog = Catalog::load(&stores).await.unwrap();
    assert_eq!(stores.poll_interval(), Duration::ZERO);
    seed_more(&lab_root, "lab-two").await;
    catalog.poll(&stores).await.outcome.unwrap();
    assert!(catalog.get(&uid("lab-two")).is_none(), "the store lab is not due");
    drop((d1, d2));
}

async fn seed_more(root: &Path, uid_text: &str) {
    let s = open(root, false).await;
    s.create(&fid("ops"), &doc(uid_text, "More"), VersionKind::Create, &meta(50)).await.unwrap();
}

#[tokio::test]
async fn a_poll_applies_the_change_of_a_store_that_is_due() {
    let (stores, _dirs, _m, lab_root) = two(Duration::ZERO).await;
    stores.open_checked().await.unwrap();
    let catalog = Catalog::load(&stores).await.unwrap();
    seed_more(&lab_root, "lab-two").await;
    catalog.poll(&stores).await.outcome.unwrap();
    assert!(catalog.get(&uid("lab-two")).is_some());
    assert_eq!(stores.store_of_uid(&uid("lab-two")).unwrap().id, "lab");
}

#[tokio::test]
async fn the_catalog_sees_a_write_in_the_primary_store_at_once() {
    let (stores, _dirs, _m, _l) = two(Duration::from_secs(1)).await;
    stores.open_checked().await.unwrap();
    let catalog = Catalog::load(&stores).await.unwrap();
    stores.create(&fid("home"), &doc("main-two", "Two"), VersionKind::Create, &meta(5)).await.unwrap();
    assert!(catalog.refresh_uid(&stores, &uid("main-two")).await);
    assert!(catalog.get(&uid("main-two")).is_some());
    let events = Source::drain_events(&stores);
    assert!(events.iter().all(|e| !matches!(e, StoreEvent::RescanRejected { .. })));
}
