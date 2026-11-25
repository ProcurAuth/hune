// SPDX-License-Identifier: MIT OR Apache-2.0
//! The dashboards without a folder in the stores of the process (5.9, 5.10 rule 5, 5.42 to 5.44): the root of a store
//! belongs to the store that holds the uid.
#![cfg(feature = "conformance")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hune_core::ids::FolderId;
use hune_store::conformance::{doc, doc_text, fid, meta, uid};
use hune_store::volume::{VolumeOptions, VolumeStore};
use hune_store::{AnyStore, Catalog, FolderTitle, Store, StoreError, StoreInfo, StoreKind, StoreSpec, Stores, VersionKind};

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

fn spec(id: &str, store: AnyStore, root: &Path, primary: bool, store_title: Option<&str>) -> StoreSpec {
    StoreSpec {
        info: StoreInfo { id: id.to_owned(), kind: StoreKind::Volume, title: store_title.map(str::to_owned), primary },
        path: Some(root.to_path_buf()),
        interval: Duration::from_secs(1),
        store,
    }
}

#[tokio::test]
async fn a_dashboard_without_a_folder_belongs_to_the_store_that_holds_its_uid() {
    let (_d1, main_root) = dir();
    let (_d2, lab_root) = dir();
    // The primary has the folder `home` with `main-one`. The read-only store has one folder and a file at its root.
    let main = open(&main_root, false).await;
    main.create_folder(&fid("home"), &title("Home")).await.unwrap();
    main.create(&fid("home"), &doc("main-one", "Main one"), VersionKind::Create, &meta(1)).await.unwrap();
    let seed = open(&lab_root, false).await;
    seed.create_folder(&fid("ops"), &title("Operations")).await.unwrap();
    seed.create(&fid("ops"), &doc("lab-one", "Lab one"), VersionKind::Create, &meta(1)).await.unwrap();
    drop(seed);
    fs::write(lab_root.join("lab-root.json"), doc_text("lab-root", "Lab root")).unwrap();
    fs::write(main_root.join("main-root.json"), doc_text("main-root", "Main root")).unwrap();
    let lab = open(&lab_root, true).await;
    let stores = Stores::new(vec![
        spec("main", main, &main_root, true, None),
        spec("lab", lab, &lab_root, false, Some("Lab")),
    ]);
    let reports = stores.open_checked().await.unwrap();
    let counts: Vec<(String, usize, usize)> = reports.iter().map(|r| (r.info.id.clone(), r.folders, r.dashboards)).collect();
    assert_eq!(counts, vec![("main".into(), 1, 2), ("lab".into(), 1, 2)]);
    let _ = stores.load().await.unwrap();
    assert_eq!(stores.store_of_uid(&uid("lab-root")).unwrap().id, "lab");
    assert_eq!(stores.store_of_uid(&uid("main-root")).unwrap().id, "main");
    assert!(stores.uid_is_read_only(&uid("lab-root")));
    assert!(!stores.uid_is_read_only(&uid("main-root")));
    // The root is not a folder, so no store owns it as a folder.
    assert!(stores.store_of_folder(&FolderId::root()).is_none());
    assert!(!stores.folder_is_read_only(&FolderId::root()));
    // A write to the root dashboard of the read-only store is refused.
    let ro = |r: Result<_, StoreError>| matches!(r, Err(StoreError::ReadOnly));
    let lab_etag = stores.get(&uid("lab-root")).await.unwrap().etag;
    let changed = doc("lab-root", "Changed");
    assert!(ro(stores.save(&changed, &lab_etag, &FolderId::root(), VersionKind::Save, &meta(9)).await.map(|_| ())));
    assert!(ro(stores.move_to(&uid("lab-root"), &fid("home"), &lab_etag, &FolderId::root(), &meta(9)).await.map(|_| ())));
    // A write to the root of the primary works, and the file is at the root of the primary volume.
    let created = stores.create(&FolderId::root(), &doc("main-new", "New"), VersionKind::Create, &meta(10)).await.unwrap();
    assert!(created.folder.is_root());
    assert_eq!(stores.store_of_uid(&uid("main-new")).unwrap().id, "main");
    assert!(main_root.join("main-new.json").is_file());
    // A move of a primary dashboard to the root works.
    let etag = stores.get(&uid("main-one")).await.unwrap().etag;
    let moved = stores.move_to(&uid("main-one"), &FolderId::root(), &etag, &fid("home"), &meta(11)).await.unwrap();
    assert!(moved.folder.is_root());
    // A uid that the read-only store holds is not free at the root of the primary.
    let twin = stores.create(&FolderId::root(), &doc("lab-root", "Twin"), VersionKind::Create, &meta(12)).await;
    assert!(matches!(twin, Err(StoreError::Exists { .. })), "{twin:?}");
    let catalog = Catalog::load(&stores).await.unwrap();
    let roots: Vec<String> =
        catalog.entries().iter().filter(|e| e.folder().is_root()).map(|e| e.uid().as_str().to_owned()).collect();
    assert_eq!(roots, vec!["lab-root", "main-new", "main-one", "main-root"]);
}
