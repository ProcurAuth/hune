// SPDX-License-Identifier: MIT OR Apache-2.0
//! `copy_store`, export, import and restore from volume to volume (5.37 to 5.40).
#![cfg(feature = "conformance")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hune_store::conformance::{doc, fid, meta, uid};
use hune_store::volume::testing::VolumeHooks;
use hune_store::volume::{Faults, VolumeOptions, VolumeStore};
use hune_store::{
    AnyStore, Etag, FolderTitle, Outcome, Store, StoreError, TransferError, VersionKind, copy_store, export_to_dir,
    import_from_dir, verify_equal,
};

struct Vol {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Vol {
    fn new() -> Vol {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        Vol { _dir: dir, root }
    }
    async fn open(&self, read_only: bool, keep: u32) -> AnyStore {
        let opts = VolumeOptions {
            root: self.root.clone(),
            read_only,
            rescan_interval: Duration::from_secs(1),
            history_keep: keep,
        };
        AnyStore::Volume(VolumeStore::open(opts).await.unwrap())
    }
    fn child(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).unwrap()
}

/// A volume with 2 folders, 3 dashboards, saves, a move, a delete (tombstone) and an outside change.
async fn populated(vol: &Vol) -> AnyStore {
    let s = vol.open(false, 100).await;
    s.create_folder(&fid("transit"), &title("Transit")).await.unwrap();
    s.create_folder(&fid("ops"), &title("Operations")).await.unwrap();
    let a = doc("alpha", "Alpha");
    s.create(&fid("transit"), &a, VersionKind::Create, &meta(1)).await.unwrap();
    let a2 = doc("alpha", "Alpha two");
    s.save(&a2, a.etag(), &fid("transit"), VersionKind::Save, &meta(2)).await.unwrap();
    s.move_to(&uid("alpha"), &fid("ops"), a2.etag(), &fid("transit"), &meta(3)).await.unwrap();
    let b = doc("beta", "Beta");
    s.create(&fid("ops"), &b, VersionKind::Create, &meta(4)).await.unwrap();
    s.delete(&uid("beta"), b.etag(), &fid("ops"), &meta(5)).await.unwrap();
    let c = doc("gamma", "Gamma");
    s.create(&fid("transit"), &c, VersionKind::Create, &meta(6)).await.unwrap();
    // An outside change, recorded as an external version by the next save.
    let outside = doc("gamma", "Gamma outside");
    VolumeHooks::new(&vol.root, Faults::new()).outside_write(&fid("transit"), &uid("gamma"), outside.bytes()).unwrap();
    let c3 = doc("gamma", "Gamma three");
    s.save(&c3, outside.etag(), &fid("transit"), VersionKind::Save, &meta(7)).await.unwrap();
    s
}

fn tree(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let rel = e.path().strip_prefix(base).unwrap().to_string_lossy().into_owned();
            if e.file_type().unwrap().is_dir() {
                out.insert(rel, None);
                walk(base, &e.path(), out);
            } else {
                out.insert(rel, Some(fs::read(e.path()).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn without_hune(mut t: BTreeMap<String, Option<Vec<u8>>>) -> BTreeMap<String, Option<Vec<u8>>> {
    t.retain(|k, _| !k.starts_with(".hune"));
    t
}

#[tokio::test]
async fn transfer_export_import_export_is_byte_equal() {
    let src = Vol::new();
    let a = populated(&src).await;
    let out = Vol::new();
    let d1 = out.child("d1");
    let r1 = export_to_dir(&a, &d1).await.expect("export");
    assert_eq!((r1.folders, r1.dashboards, r1.versions), (2, 3, 8), "{r1:?}");
    assert!(!d1.join(".hune").exists(), "the export writes no .hune");
    // The export is a valid volume root: a reader loads it.
    let reader = Vol { _dir: tempfile::tempdir().unwrap(), root: d1.clone() };
    let r = reader.open(true, 100).await;
    assert_eq!(r.load().await.unwrap().entries.len(), 2);
    // Import into an empty volume, then export again.
    let dst = Vol::new();
    let b = dst.open(false, 100).await;
    let r2 = import_from_dir(&d1, &b).await.expect("import");
    assert_eq!(r2, r1);
    let d2 = out.child("d2");
    export_to_dir(&b, &d2).await.expect("second export");
    assert_eq!(tree(&d1), tree(&d2), "SC-44 on volumes: byte-equal directories");
    // The files of the first export equal the files of the source volume.
    assert_eq!(tree(&d1), without_hune(tree(&src.root)).into_iter().filter(|(k, _)| !k.is_empty()).collect());
    verify_equal(&a, &b).await.expect("the stores are equal");
}

#[tokio::test]
async fn transfer_copy_store_volume_to_volume() {
    let src = Vol::new();
    let a = populated(&src).await;
    let dst = Vol::new();
    let b = dst.open(false, 100).await;
    let report = copy_store(&a, &b).await.expect("copy");
    assert_eq!((report.folders, report.dashboards, report.versions), (2, 3, 8));
    assert_eq!(without_hune(tree(&src.root)), without_hune(tree(&dst.root)), "byte-equal files");
    // The copy keeps the tombstone: the uid cannot be created again, and a restore works.
    let c = b.create(&fid("ops"), &doc("beta", "Beta"), VersionKind::Create, &meta(50)).await;
    assert!(matches!(c, Err(StoreError::Exists { .. })));
    let h = b.history(&uid("beta"), None, 10).await.unwrap();
    assert_eq!((h[0].kind, h[0].etag), (VersionKind::Delete, None));
}

#[tokio::test]
async fn transfer_copy_from_a_read_only_source_takes_no_lock() {
    let src = Vol::new();
    let a = populated(&src).await;
    let ro = src.open(true, 100).await;
    let dst = Vol::new();
    let b = dst.open(false, 100).await;
    copy_store(&ro, &b).await.expect("copy from a read-only source beside the writer");
    drop(a);
}

#[tokio::test]
async fn transfer_into_a_store_that_is_not_empty() {
    let src = Vol::new();
    let a = populated(&src).await;
    let dst = Vol::new();
    let b = dst.open(false, 100).await;
    b.create_folder(&fid("existing"), &title("Existing")).await.unwrap();
    let before = tree(&dst.root);
    let r = copy_store(&a, &b).await;
    assert!(matches!(r, Err(TransferError::Store(StoreError::NotEmpty))), "{r:?}");
    assert_eq!(tree(&dst.root), before, "SC-45: nothing written");
    // A target with only a history directory is not empty either.
    let dst2 = Vol::new();
    let b2 = dst2.open(false, 100).await;
    copy_store(&a, &b2).await.unwrap();
    let r = copy_store(&a, &b2).await;
    assert!(matches!(r, Err(TransferError::Store(StoreError::NotEmpty))));
}

#[tokio::test]
async fn transfer_import_of_an_invalid_layout_writes_nothing() {
    let src = Vol::new();
    let a = populated(&src).await;
    let out = Vol::new();
    let layout = out.child("layout");
    export_to_dir(&a, &layout).await.unwrap();
    let dst = Vol::new();
    let b = dst.open(false, 100).await;
    let empty = tree(&dst.root);
    // 1. An invalid current document.
    fs::write(layout.join("ops/alpha.json"), b"{ not json").unwrap();
    let r = import_from_dir(&layout, &b).await;
    assert!(matches!(r, Err(TransferError::InvalidSource(ref p)) if !p.is_empty()), "SC-46: {r:?}");
    assert_eq!(tree(&dst.root), empty, "SC-46: nothing written");
    export_to_dir(&a, &out.child("layout2")).await.unwrap();
    let layout = out.child("layout2");
    // 2. A history file whose SHA-256 differs from its etag.
    fs::write(layout.join(".history/alpha/1.json"), b"tampered").unwrap();
    let r = import_from_dir(&layout, &b).await;
    assert!(
        matches!(r, Err(TransferError::InvalidSource(ref p)) if p.iter().any(|x| x.place.contains("alpha"))),
        "{r:?}"
    );
    assert_eq!(tree(&dst.root), empty);
    export_to_dir(&a, &out.child("layout3")).await.unwrap();
    let layout = out.child("layout3");
    // 3. A metadata file that is not valid.
    fs::write(layout.join(".history/alpha/2.meta.json"), b"{}").unwrap();
    let r = import_from_dir(&layout, &b).await;
    assert!(matches!(r, Err(TransferError::InvalidSource(_))), "{r:?}");
    assert_eq!(tree(&dst.root), empty);
}

#[tokio::test]
async fn transfer_export_directory_rules() {
    let src = Vol::new();
    let a = populated(&src).await;
    let out = Vol::new();
    let store_err = |r: Result<_, TransferError>| match r {
        Err(TransferError::Store(e)) => e,
        other => panic!("expected a store error, found {other:?}"),
    };
    // A relative path, a final slash, a missing parent.
    assert!(matches!(store_err(export_to_dir(&a, Path::new("relative/dir")).await), StoreError::Unavailable(_)));
    let slash = PathBuf::from(format!("{}/", out.child("x").display()));
    assert!(matches!(store_err(export_to_dir(&a, &slash).await), StoreError::Unavailable(_)));
    assert!(matches!(store_err(export_to_dir(&a, &out.child("no/parent")).await), StoreError::Unavailable(_)));
    // A directory that is not empty.
    fs::create_dir(out.child("full")).unwrap();
    fs::write(out.child("full/x"), b"x").unwrap();
    assert_eq!(store_err(export_to_dir(&a, &out.child("full")).await), StoreError::NotEmpty);
    // An empty directory is permitted.
    fs::create_dir(out.child("empty")).unwrap();
    export_to_dir(&a, &out.child("empty")).await.unwrap();
    // A second export into the same directory is refused.
    assert_eq!(store_err(export_to_dir(&a, &out.child("empty")).await), StoreError::NotEmpty);
}

#[tokio::test]
async fn transfer_export_of_an_invalid_source_writes_nothing() {
    let src = Vol::new();
    let a = populated(&src).await;
    fs::write(src.child("transit/gamma.json"), b"{ broken").unwrap();
    let out = Vol::new();
    let target = out.child("export");
    let r = export_to_dir(&a, &target).await;
    assert!(matches!(r, Err(TransferError::InvalidSource(_))), "{r:?}");
    assert!(!target.exists(), "5.38 item 4: stops before it writes");
}

#[tokio::test]
async fn transfer_import_keeps_all_versions_and_the_retention_applies_later() {
    let src = Vol::new();
    let s = src.open(false, 100).await;
    s.create_folder(&fid("f"), &title("F")).await.unwrap();
    let mut d = doc("d1", "T1");
    s.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    for i in 2..=6u64 {
        let n = doc("d1", &format!("T{i}"));
        s.save(&n, d.etag(), &fid("f"), VersionKind::Save, &meta(i)).await.unwrap();
        d = n;
    }
    let dst = Vol::new();
    let t = dst.open(false, 2).await;
    copy_store(&s, &t).await.unwrap();
    assert_eq!(t.history(&uid("d1"), None, 100).await.unwrap().len(), 6, "the import keeps all versions");
    let next = doc("d1", "T7");
    let r = t.save(&next, d.etag(), &fid("f"), VersionKind::Save, &meta(7)).await.unwrap();
    assert_eq!((r.version, r.outcome), (7, Outcome::Written));
    let nums: Vec<u64> = t.history(&uid("d1"), None, 100).await.unwrap().iter().map(|v| v.version).collect();
    assert_eq!(nums, vec![7, 6], "the retention applies at the next write");
}

#[tokio::test]
async fn transfer_version_number_gaps_stay() {
    let src = Vol::new();
    let s = src.open(false, 2).await;
    s.create_folder(&fid("f"), &title("F")).await.unwrap();
    let mut d = doc("d1", "T1");
    s.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    for i in 2..=5u64 {
        let n = doc("d1", &format!("T{i}"));
        s.save(&n, d.etag(), &fid("f"), VersionKind::Save, &meta(i)).await.unwrap();
        d = n;
    }
    let dst = Vol::new();
    let t = dst.open(false, 2).await;
    copy_store(&s, &t).await.unwrap();
    let nums: Vec<u64> = t.history(&uid("d1"), None, 100).await.unwrap().iter().map(|v| v.version).collect();
    assert_eq!(nums, vec![5, 4]);
}

#[tokio::test]
async fn restore_of_a_deleted_dashboard_as_the_command_does() {
    // 5.40: n is the highest version of the uid that has a document; saved_by is `-`.
    let src = Vol::new();
    let s = populated(&src).await;
    let u = uid("beta");
    assert!(matches!(s.get(&u).await, Err(StoreError::NotFound)));
    let versions = s.history(&u, None, 100).await.unwrap();
    let n = versions.iter().find(|v| v.etag.is_some()).map(|v| v.version).expect("a version with a document");
    assert_eq!(n, 1);
    let m = hune_store::WriteMeta {
        saved_by: hune_store::Subject::anonymous(),
        saved_at: hune_store::UtcMillis::from_millis(1_794_300_000_000),
        message: hune_store::Message::parse(&format!("restore of version {n} by hune restore")).unwrap(),
        save_id: None,
    };
    let saved = s.restore(&u, n, None, None, &m).await.unwrap();
    assert_eq!((saved.version, saved.outcome, saved.folder.as_str()), (3, Outcome::Written, "ops"));
    let cur = s.get(&u).await.unwrap();
    assert_eq!(Some(cur.etag), saved.etag);
    assert_eq!(cur.saved_by.as_str(), "-");
    // A dashboard that exists: the command stops before the call, and a restore without if_match gives Conflict.
    let r = s.restore(&u, 1, None, None, &m).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))));
    let _ = Etag::of(b"");
}
