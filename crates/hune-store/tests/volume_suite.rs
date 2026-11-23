// SPDX-License-Identifier: MIT OR Apache-2.0
//! The volume tests of 5.35 (SV-01 to SV-29) and the case SX-01 of 16.7.
#![cfg(feature = "conformance")]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

use hune_core::document::validate;
use hune_core::error::ErrorCode;
use hune_core::limits::MAX_DOCUMENT_BYTES;
use hune_store::conformance::{doc, doc_text, fid, meta, meta_no_sid, uid};
use hune_store::volume::testing::VolumeHooks;
use hune_store::volume::{FaultAction, FaultPoint, Faults, VolumeOptions, VolumeStore};
use hune_store::{
    AnyStore, Catalog, CatalogEntry, Etag, FolderTitle, OpenErrorKind, Outcome, Store, StoreError, StoreEvent,
    VersionKind,
};

struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    faults: Faults,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        Env { _dir: dir, root, faults: Faults::new() }
    }

    fn opts(&self, read_only: bool, keep: u32) -> VolumeOptions {
        VolumeOptions {
            root: self.root.clone(),
            read_only,
            rescan_interval: Duration::from_secs(1),
            history_keep: keep,
        }
    }

    async fn writer(&self) -> VolumeStore {
        VolumeStore::open_with_faults(self.opts(false, 100), self.faults.clone()).await.expect("open the writer")
    }

    async fn reader(&self) -> VolumeStore {
        VolumeStore::open(self.opts(true, 100)).await.expect("open the reader")
    }

    fn hooks(&self) -> VolumeHooks {
        VolumeHooks::new(&self.root, self.faults.clone())
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).unwrap()
}

async fn mkfolder(s: &AnyStore, id: &str) {
    s.create_folder(&fid(id), &title(id)).await.unwrap();
}

/// A writer with the folder `f` and the dashboard `d1`.
async fn with_dashboard(env: &Env) -> (AnyStore, hune_store::CanonicalDocument) {
    let s = AnyStore::Volume(env.writer().await);
    mkfolder(&s, "f").await;
    let d = doc("d1", "First");
    s.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    (s, d)
}

async fn save(
    s: &AnyStore,
    d: &hune_store::CanonicalDocument,
    if_match: &Etag,
    n: u64,
) -> Result<hune_store::Saved, StoreError> {
    s.save(d, if_match, &fid("f"), VersionKind::Save, &meta(n)).await
}

async fn nums(s: &AnyStore, u: &str) -> Vec<u64> {
    s.history(&uid(u), None, 100).await.unwrap().iter().map(|v| v.version).collect()
}

fn events(s: &AnyStore) -> Vec<StoreEvent> {
    s.drain_events()
}

fn ignored_paths(ev: &[StoreEvent]) -> Vec<String> {
    let mut v: Vec<String> = ev
        .iter()
        .filter_map(|e| match e {
            StoreEvent::EntryIgnored { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect();
    v.sort();
    v
}

fn temp_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains(".tmp-"))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn sv_01_path_rules() {
    let env = Env::new();
    let open = |p: PathBuf| async move {
        VolumeStore::open(VolumeOptions {
            root: p,
            read_only: true,
            rescan_interval: Duration::from_secs(1),
            history_keep: 100,
        })
        .await
    };
    fs::create_dir_all(env.path("real/sub")).unwrap();
    let cases = [
        PathBuf::from("relative/dir"),
        PathBuf::from(format!("{}/real/../real", env.root.display())),
        PathBuf::from(format!("{}/real/", env.root.display())),
        PathBuf::from(format!("{}/./real", env.root.display())),
        PathBuf::from("/"),
    ];
    for p in cases {
        let e = open(p.clone()).await.unwrap_err();
        assert_eq!(e.kind, OpenErrorKind::PathRule, "SV-01 {p:?}: {e}");
        assert!(e.message.contains(&p.display().to_string()), "SV-01 the message names the path");
    }
    symlink(env.path("real"), env.path("link")).unwrap();
    assert_eq!(open(env.path("link")).await.unwrap_err().kind, OpenErrorKind::PathRule, "SV-01 link");
    symlink(env.path("real"), env.path("link2")).unwrap();
    assert_eq!(open(env.path("link2/sub")).await.unwrap_err().kind, OpenErrorKind::PathRule, "SV-01 link in path");
    assert!(open(env.path("real")).await.is_ok());
}

#[tokio::test]
async fn sv_02_root_absent_file_unreadable() {
    let env = Env::new();
    let open = |p: PathBuf, ro: bool| async move {
        VolumeStore::open(VolumeOptions {
            root: p,
            read_only: ro,
            rescan_interval: Duration::from_secs(1),
            history_keep: 100,
        })
        .await
    };
    assert_eq!(open(env.path("absent"), false).await.unwrap_err().kind, OpenErrorKind::RootMissing);
    fs::write(env.path("file"), b"x").unwrap();
    assert_eq!(open(env.path("file"), false).await.unwrap_err().kind, OpenErrorKind::RootNotDirectory);
    fs::create_dir(env.path("locked")).unwrap();
    fs::set_permissions(env.path("locked"), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(env.path("locked")).is_err() {
        assert_eq!(open(env.path("locked"), true).await.unwrap_err().kind, OpenErrorKind::RootNotReadable);
    }
    fs::set_permissions(env.path("locked"), fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test]
async fn sv_03_root_writable_by_others() {
    let env = Env::new();
    fs::set_permissions(&env.root, fs::Permissions::from_mode(0o757)).unwrap();
    let e = VolumeStore::open(env.opts(false, 100)).await.unwrap_err();
    assert_eq!(e.kind, OpenErrorKind::RootWritableByOthers, "{e}");
    fs::set_permissions(&env.root, fs::Permissions::from_mode(0o750)).unwrap();
    assert!(VolumeStore::open(env.opts(false, 100)).await.is_ok());
}

#[tokio::test]
async fn sv_04_reserved_names() {
    for name in [".hune", ".history"] {
        for as_link in [true, false] {
            let env = Env::new();
            if as_link {
                fs::create_dir(env.path("elsewhere")).unwrap();
                symlink(env.path("elsewhere"), env.path(name)).unwrap();
            } else {
                fs::write(env.path(name), b"x").unwrap();
            }
            for ro in [false, true] {
                let e = VolumeStore::open(env.opts(ro, 100)).await.unwrap_err();
                assert_eq!(e.kind, OpenErrorKind::ReservedNotDirectory, "SV-04 {name} link={as_link}: {e}");
            }
        }
    }
}

#[tokio::test]
async fn sv_05_second_writer() {
    let env = Env::new();
    let first = env.writer().await;
    let e = VolumeStore::open(env.opts(false, 100)).await.unwrap_err();
    assert_eq!(e.kind, OpenErrorKind::LockHeld);
    let holder = fs::read_to_string(env.path(".hune/lock")).unwrap();
    assert!(holder.contains(&std::process::id().to_string()), "the lock file holds the pid: {holder}");
    assert!(e.message.contains(holder.trim()), "SV-05 the message gives the content of the lock file: {e}");
    assert!(e.message.contains(".hune/lock"));
    drop(first);
    assert!(VolumeStore::open(env.opts(false, 100)).await.is_ok(), "the lock is free after the drop");
}

#[tokio::test]
async fn sv_06_read_only_beside_a_writer() {
    let env = Env::new();
    let fresh = Env::new();
    let _ro_only = fresh.reader().await;
    assert!(!fresh.path(".hune").exists() && !fresh.path(".history").exists(), "SV-06 a reader makes nothing");
    let (w, d) = with_dashboard(&env).await;
    let r = AnyStore::Volume(env.reader().await);
    assert_eq!(r.get(&uid("d1")).await.unwrap().bytes.as_slice(), d.bytes());
    assert_eq!(nums(&r, "d1").await, vec![1]);
    let ro = StoreError::ReadOnly;
    let d2 = doc("d1", "Second");
    assert_eq!(r.create_folder(&fid("g"), &title("G")).await, Err(ro.clone()));
    assert_eq!(r.rename_folder(&fid("f"), &title("G")).await, Err(ro.clone()));
    assert_eq!(r.delete_folder(&fid("f")).await, Err(ro.clone()));
    assert_eq!(r.create(&fid("f"), &doc("d2", "X"), VersionKind::Create, &meta(2)).await, Err(ro.clone()));
    assert_eq!(save(&r, &d2, d.etag(), 3).await, Err(ro.clone()));
    assert_eq!(r.move_to(&uid("d1"), &fid("f"), d.etag(), &fid("f"), &meta(4)).await, Err(ro.clone()));
    assert_eq!(r.delete(&uid("d1"), d.etag(), &fid("f"), &meta(5)).await, Err(ro.clone()));
    assert_eq!(r.restore(&uid("d1"), 1, None, None, &meta(6)).await, Err(ro.clone()));
    assert!(matches!(r.import_begin().await, Err(StoreError::ReadOnly)));
    assert!(w.get(&uid("d1")).await.is_ok());
}

#[tokio::test]
async fn sv_07_symbolic_links() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    // A folder that is a link.
    mkfolder(&w, "real").await;
    symlink(env.path("real"), env.path("linkdir")).unwrap();
    // A dashboard file that is a link, and a `_folder.json` that is a link.
    fs::write(env.path("outside.json"), doc("d9", "Out").bytes()).unwrap();
    symlink(env.path("outside.json"), env.path("f/d9.json")).unwrap();
    fs::create_dir(env.path("nofile")).unwrap();
    symlink(env.path("real/_folder.json"), env.path("nofile/_folder.json")).unwrap();
    let snap = w.load().await.unwrap();
    let names: Vec<String> = snap.folders.iter().map(|f| f.id().to_string()).collect();
    assert_eq!(names, vec!["f", "nofile", "real"], "SV-07: the link directory is not a folder");
    assert!(
        matches!(snap.folders[1], hune_store::FolderState::Invalid { .. }),
        "SV-07: a link as `_folder.json` is not read"
    );
    assert_eq!(snap.entries.len(), 1, "SV-07: the dashboard link is not an entry");
    let ign = ignored_paths(&events(&w));
    assert!(ign.contains(&"linkdir".to_owned()) && ign.contains(&"f/d9.json".to_owned()), "SV-07 {ign:?}");
    // No write through a link.
    let before = fs::read_dir(env.path("real")).unwrap().count();
    assert_eq!(
        w.create(&fid("linkdir"), &doc("d2", "X"), VersionKind::Create, &meta(2)).await,
        Err(StoreError::FolderNotFound)
    );
    assert!(matches!(w.create_folder(&fid("linkdir"), &title("X")).await, Err(StoreError::Exists { .. })));
    assert_eq!(fs::read_dir(env.path("real")).unwrap().count(), before, "SV-07: nothing written through the link");
    assert_eq!(w.get(&uid("d9")).await, Err(StoreError::NotFound), "SV-07: the link is not followed by get");
    // A history directory that is a link.
    fs::create_dir(env.path("hist-copy")).unwrap();
    for e in fs::read_dir(env.path(".history/d1")).unwrap() {
        let e = e.unwrap();
        fs::copy(e.path(), env.path("hist-copy").join(e.file_name())).unwrap();
    }
    fs::remove_dir_all(env.path(".history/d1")).unwrap();
    symlink(env.path("hist-copy"), env.path(".history/d1")).unwrap();
    assert_eq!(w.version(&uid("d1"), 1).await, Err(StoreError::VersionNotFound), "SV-07 history link");
    let r = save(&w, &doc("d1", "Second"), d.etag(), 5).await;
    assert!(r.is_err(), "SV-07: no write through the history link: {r:?}");
    assert_eq!(fs::read_dir(env.path("hist-copy")).unwrap().count(), 2, "SV-07: the link target is unchanged");
}

#[tokio::test]
async fn sv_08_foreign_entries() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    fs::write(env.path("notes.txt"), b"x").unwrap();
    fs::create_dir(env.path("Bad_Name")).unwrap();
    fs::create_dir(env.path(".git")).unwrap();
    fs::write(env.path(".gitignore"), b"x").unwrap();
    fs::write(env.path("f/readme.md"), b"x").unwrap();
    fs::write(env.path("f/.swp"), b"x").unwrap();
    let catalog = Catalog::load(&w).await.unwrap();
    let ign = ignored_paths(&catalog.take_events());
    assert_eq!(ign, vec!["Bad_Name", "f/readme.md", "notes.txt"], "SV-08 one line for each foreign entry");
    let polled = catalog.poll(&w).await;
    assert!(ignored_paths(&polled.events).is_empty(), "SV-08: one line only, no repeat");
    fs::write(env.path("notes.txt"), b"a longer text now").unwrap();
    let polled = catalog.poll(&w).await;
    assert_eq!(ignored_paths(&polled.events), vec!["notes.txt"], "SV-08: again after the entry changes");
}

#[tokio::test]
async fn sv_09_folder_without_folder_file() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    fs::create_dir(env.path("nofolder")).unwrap();
    let d = doc("d1", "First");
    fs::write(env.path("nofolder/d1.json"), d.bytes()).unwrap();
    let catalog = Catalog::load(&w).await.unwrap();
    assert!(catalog.folders().is_empty());
    assert_eq!(catalog.invalid_folders().len(), 1, "SV-09");
    match &*catalog.get(&uid("d1")).unwrap() {
        CatalogEntry::Invalid { errors, .. } => assert_eq!(errors[0].code, ErrorCode::NotPermitted, "SV-09"),
        other => panic!("SV-09: {other:?}"),
    }
    assert!(matches!(w.get(&uid("d1")).await, Err(StoreError::Invalid(_))), "SV-09: not served");
}

#[tokio::test]
async fn sv_10_one_uid_in_two_folders() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    mkfolder(&w, "g").await;
    let d = doc("d1", "First");
    fs::write(env.path("f/d1.json"), d.bytes()).unwrap();
    fs::write(env.path("g/d1.json"), d.bytes()).unwrap();
    let catalog = Catalog::load(&w).await.unwrap();
    let entries = catalog.entries();
    assert_eq!(entries.len(), 2, "SV-10");
    for e in entries {
        match &*e {
            CatalogEntry::Invalid { errors, .. } => assert_eq!(errors[0].code, ErrorCode::NotUnique, "SV-10"),
            other => panic!("SV-10: {other:?}"),
        }
    }
    assert!(matches!(w.get(&uid("d1")).await, Err(StoreError::Invalid(_))));
    // After the removal of one file, the other entry is valid again.
    fs::remove_file(env.path("g/d1.json")).unwrap();
    let r = catalog.poll(&w).await;
    assert!(r.outcome.is_ok());
    assert!(matches!(&*catalog.get(&uid("d1")).unwrap(), CatalogEntry::Valid { .. }), "SV-10 recovers");
}

#[tokio::test]
async fn sv_11_file_too_large() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    let mut big = doc_text("d1", "Big").into_bytes();
    big.resize(MAX_DOCUMENT_BYTES + 1, b' ');
    fs::write(env.path("f/d1.json"), &big).unwrap();
    let snap = w.load().await.unwrap();
    match &*snap.entries[0] {
        CatalogEntry::Invalid { errors, etag, .. } => {
            assert_eq!(errors[0].code, ErrorCode::TooLarge, "SV-11");
            assert_eq!(*etag, Etag::of(&big), "SV-11");
        }
        other => panic!("SV-11: {other:?}"),
    }
    // The scan reads at most MAX plus 1 bytes: the etag of a longer file covers the first MAX + 1 bytes.
    let mut longer = big.clone();
    longer.extend_from_slice(&[b' '; 1000]);
    fs::write(env.path("f/d1.json"), &longer).unwrap();
    let snap = w.load().await.unwrap();
    assert_eq!(*snap.entries[0].etag(), Etag::of(&big), "SV-11: at most MAX_DOCUMENT_BYTES plus 1 bytes are read");
}

#[tokio::test]
async fn sv_12_valid_file_that_is_not_canonical() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    let raw = format!("   {}", doc_text("d1", "Plain"));
    fs::write(env.path("f/d1.json"), raw.as_bytes()).unwrap();
    let canon = validate(raw.as_bytes(), Some(&uid("d1"))).unwrap();
    assert_ne!(canon.bytes(), raw.as_bytes());
    let cur = w.get(&uid("d1")).await.unwrap();
    assert_eq!((cur.bytes.as_slice(), cur.etag), (canon.bytes(), *canon.etag()), "SV-12 canonical form and etag");
    assert_eq!(fs::read(env.path("f/d1.json")).unwrap(), raw.as_bytes(), "SV-12: the file stays");
    let d2 = doc("d1", "Next");
    save(&w, &d2, canon.etag(), 2).await.unwrap();
    assert_eq!(fs::read(env.path("f/d1.json")).unwrap(), d2.bytes(), "SV-12: canonical after a save");
    let ext = w.version(&uid("d1"), 1).await.unwrap();
    assert_eq!(ext.meta.kind, VersionKind::External);
    assert_eq!(ext.bytes.as_deref(), Some(canon.bytes()), "SV-12: the external version holds the canonical bytes");
}

#[tokio::test]
async fn sv_13_truncated_json_then_full_file() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    let catalog = Catalog::load(&w).await.unwrap();
    let hooks = env.hooks();
    let d = doc("d1", "First");
    hooks.outside_write_in_place(&fid("f"), &uid("d1"), &d.bytes()[..100]).unwrap();
    catalog.poll(&w).await.outcome.unwrap();
    assert!(matches!(&*catalog.get(&uid("d1")).unwrap(), CatalogEntry::Invalid { .. }), "SV-13 invalid");
    hooks.outside_write_in_place(&fid("f"), &uid("d1"), d.bytes()).unwrap();
    catalog.poll(&w).await.outcome.unwrap();
    assert!(matches!(&*catalog.get(&uid("d1")).unwrap(), CatalogEntry::Valid { .. }), "SV-13 valid");
}

#[tokio::test]
async fn sv_14_rename_over_the_old_file() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let catalog = Catalog::load(&w).await.unwrap();
    catalog.take_events();
    let outside = doc("d1", "Outside");
    env.hooks().outside_write(&fid("f"), &uid("d1"), outside.bytes()).unwrap();
    let r = catalog.poll(&w).await;
    assert!(r.outcome.is_ok());
    assert_eq!(catalog.get(&uid("d1")).unwrap().etag(), outside.etag(), "SV-14 visible after one rescan");
    let ev: Vec<_> = r.events.iter().filter(|e| matches!(e, StoreEvent::ExternalChange { .. })).collect();
    assert_eq!(
        ev,
        vec![&StoreEvent::ExternalChange {
            uid: uid("d1"),
            folder: fid("f"),
            etag_before: Some(*d.etag()),
            etag_after: Some(*outside.etag())
        }],
        "SV-14 the event dashboard.changed with the action external"
    );
    assert_eq!(nums(&w, "d1").await, vec![1], "SV-14: the poll writes no version");
    let next = doc("d1", "Next");
    save(&w, &next, outside.etag(), 2).await.unwrap();
    let kinds: Vec<_> = w.history(&uid("d1"), None, 10).await.unwrap().iter().map(|v| v.kind).collect();
    assert_eq!(kinds, vec![VersionKind::Save, VersionKind::External, VersionKind::Create], "SV-14");
}

#[tokio::test]
async fn sv_15_no_space() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let d2 = doc("d1", "Second");
    let before = fs::read(env.path("f/d1.json")).unwrap();
    // The error in the first write (the history bytes).
    env.faults.inject(FaultPoint::Write, FaultAction::Errno(28));
    assert_eq!(save(&w, &d2, d.etag(), 2).await, Err(StoreError::Full), "SV-15 history");
    // The error in the third write (the dashboard file), after the two history files.
    let noop = FaultAction::Run(std::sync::Arc::new(|| {}));
    env.faults.inject(FaultPoint::Write, noop.clone());
    env.faults.inject(FaultPoint::Write, noop);
    env.faults.inject(FaultPoint::Write, FaultAction::Errno(28));
    assert_eq!(save(&w, &d2, d.etag(), 3).await, Err(StoreError::Full), "SV-15 dashboard file");
    assert_eq!(fs::read(env.path("f/d1.json")).unwrap(), before, "SV-15: the current file does not change");
    for dir in [env.path("f"), env.path(".history/d1"), env.root.clone()] {
        assert!(temp_files(&dir).is_empty(), "SV-15: no temporary file in {dir:?}");
    }
    assert_eq!(w.get(&uid("d1")).await.unwrap().bytes.as_slice(), d.bytes());
}

#[tokio::test]
async fn sv_16_crash_after_the_history_files_then_retry() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let d2 = doc("d1", "Second");
    env.hooks().break_next_write();
    let r = save(&w, &d2, d.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "SV-16 {r:?}");
    assert_eq!(fs::read(env.path("f/d1.json")).unwrap(), d.bytes(), "SV-16: the current file does not change");
    let s = save(&w, &d2, d.etag(), 2).await.unwrap();
    assert_eq!((s.outcome, s.version), (Outcome::Written, 2), "SV-16 the retry rolls forward");
    assert_eq!(w.get(&uid("d1")).await.unwrap().bytes.as_slice(), d2.bytes());
    assert_eq!(nums(&w, "d1").await, vec![2, 1], "SV-16: one version for this save");
    let again = save(&w, &d2, d.etag(), 2).await.unwrap();
    assert_eq!(again.outcome, Outcome::Replayed);
}

#[tokio::test]
async fn sv_16a_crash_then_another_save() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let d2 = doc("d1", "Second");
    env.hooks().break_next_write();
    assert!(save(&w, &d2, d.etag(), 2).await.is_err());
    let d3 = doc("d1", "Third");
    let s = save(&w, &d3, d.etag(), 3).await.unwrap();
    assert_eq!(s.version, 4, "SV-16a");
    let h = w.history(&uid("d1"), None, 10).await.unwrap();
    assert_eq!(
        h.iter().map(|v| (v.version, v.kind)).collect::<Vec<_>>(),
        vec![(4, VersionKind::Save), (3, VersionKind::External), (2, VersionKind::Save), (1, VersionKind::Create)]
    );
    let ext = w.version(&uid("d1"), 3).await.unwrap();
    assert_eq!(ext.bytes.as_deref(), Some(d.bytes()), "SV-16a: the external version holds the current bytes");
}

#[tokio::test]
async fn sv_17_crash_before_the_metadata_file() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    env.faults.inject(FaultPoint::HistoryJsonWritten, FaultAction::Errno(5));
    assert!(save(&w, &doc("d1", "Second"), d.etag(), 2).await.is_err());
    assert!(env.path(".history/d1/2.json").exists() && !env.path(".history/d1/2.meta.json").exists());
    drop(w);
    let w = AnyStore::Volume(env.writer().await);
    assert!(!env.path(".history/d1/2.json").exists(), "SV-17: removed at the next start");
    assert_eq!(nums(&w, "d1").await, vec![1], "SV-17: version 2 does not exist");
}

#[tokio::test]
async fn sv_18_temporary_files_of_a_crash() {
    let env = Env::new();
    let (w, _) = with_dashboard(&env).await;
    drop(w);
    let tmp = ".d1.json.tmp-0123456789abcdef";
    for dir in ["", "f", ".history", ".history/d1"] {
        fs::write(env.path(dir).join(tmp), b"x").unwrap();
    }
    fs::write(env.path("f/.keep-me"), b"x").unwrap();
    fs::write(env.path("f/d1.json.tmp-0123456789abcdef"), b"x").unwrap();
    let _w = env.writer().await;
    for dir in ["", "f", ".history", ".history/d1"] {
        assert!(!env.path(dir).join(tmp).exists(), "SV-18 {dir}");
    }
    assert!(env.path("f/.keep-me").exists(), "other dot files stay");
    assert!(
        env.path("f/d1.json.tmp-0123456789abcdef").exists(),
        "a name without the leading dot is not a temporary file"
    );
}

#[tokio::test]
async fn sv_19_file_changes_before_the_check() {
    let env = Env::new();
    let (w, _d) = with_dashboard(&env).await;
    let x = doc("d1", "X");
    env.hooks().outside_write(&fid("f"), &uid("d1"), x.bytes()).unwrap();
    let y = doc("d1", "Y");
    let hooks = env.hooks();
    let (yb, f, u) = (y.bytes().to_vec(), fid("f"), uid("d1"));
    env.faults.inject(
        FaultPoint::BeforeEffect,
        FaultAction::Run(std::sync::Arc::new(move || hooks.outside_write(&f, &u, &yb).unwrap())),
    );
    let s = doc("d1", "S");
    let r = save(&w, &s, x.etag(), 2).await;
    match r {
        Err(StoreError::Conflict(c)) => assert_eq!(c.etag, *y.etag(), "SV-19"),
        other => panic!("SV-19: {other:?}"),
    }
    let h = w.history(&uid("d1"), None, 10).await.unwrap();
    assert_eq!(
        h.iter().map(|v| (v.version, v.kind)).collect::<Vec<_>>(),
        vec![(3, VersionKind::Save), (2, VersionKind::External), (1, VersionKind::Create),],
        "SV-19"
    );
    assert!(h[0].save_id.is_none(), "SV-19: save_id null");
    let t = doc("d1", "T");
    let s2 = save(&w, &t, y.etag(), 3).await.unwrap();
    assert_eq!(s2.version, 5, "SV-19: the next versions: external 4, save 5");
    let r = save(&w, &s, x.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SV-19: a retry gives Conflict, not Replayed: {r:?}");
}

#[tokio::test]
async fn sv_20_modes() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let mode = |p: &str| fs::metadata(env.path(p)).unwrap().permissions().mode() & 0o7777;
    for p in ["f/d1.json", "f/_folder.json", ".history/d1/1.json", ".history/d1/1.meta.json", ".hune/lock"] {
        assert_eq!(mode(p) & !0o640, 0, "SV-20 file {p}: {:o}", mode(p));
    }
    for p in ["f", ".history", ".history/d1", ".hune"] {
        assert_eq!(mode(p) & !0o750, 0, "SV-20 directory {p}: {:o}", mode(p));
    }
    fs::set_permissions(env.path("f/d1.json"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(env.path("f/_folder.json"), fs::Permissions::from_mode(0o664)).unwrap();
    save(&w, &doc("d1", "Second"), d.etag(), 2).await.unwrap();
    w.rename_folder(&fid("f"), &title("Renamed")).await.unwrap();
    assert_eq!(mode("f/d1.json"), 0o644, "SV-20: the mode of a file that exists does not change");
    assert_eq!(mode("f/_folder.json"), 0o664, "SV-20: also for the folder file");
}

#[tokio::test]
async fn sv_21_no_flock() {
    let env = Env::new();
    env.faults.inject(FaultPoint::Flock, FaultAction::Errno(37));
    let w = env.writer().await;
    let ev = w.drain_events();
    assert!(
        ev.iter().any(|e| matches!(e, StoreEvent::LockUnsupported { errno: 37, path } if path.ends_with(".hune/lock"))),
        "SV-21 {ev:?}"
    );
    let s = AnyStore::Volume(w);
    mkfolder(&s, "f").await;
}

#[tokio::test]
async fn sv_22_commands_beside_serve() {
    // `import`, `restore` and `migrate-documents --write` open the volume as a writer: they stop on the lock.
    let env = Env::new();
    let _serve = env.writer().await;
    for _ in 0..3 {
        let e = VolumeStore::open(env.opts(false, 100)).await.unwrap_err();
        assert_eq!(e.kind, OpenErrorKind::LockHeld, "SV-22");
    }
}

#[tokio::test]
async fn sv_23_delete_folder_with_a_foreign_entry() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    fs::write(env.path("f/notes.txt"), b"x").unwrap();
    assert_eq!(w.delete_folder(&fid("f")).await, Err(StoreError::FolderNotEmpty), "SV-23");
    let ev = events(&w);
    assert!(
        ev.contains(&StoreEvent::FolderNotEmpty { folder: fid("f"), entry: "notes.txt".to_owned() }),
        "SV-23 the message names the entry: {ev:?}"
    );
    assert!(env.path("f/_folder.json").exists());
}

#[tokio::test]
async fn sv_24_bad_metadata_files() {
    let env = Env::new();
    let (w, _) = with_dashboard(&env).await;
    let text = fs::read_to_string(env.path(".history/d1/1.meta.json")).unwrap();
    fs::write(env.path(".history/d1/7.meta.json"), text.replace("\"uid\": \"d1\"", "\"uid\": \"zz\"")).unwrap();
    fs::write(env.path(".history/d1/8.meta.json"), &text).unwrap();
    fs::write(
        env.path(".history/d1/9.meta.json"),
        text.replace("\"version\": 1", "\"version\": 9").replace("\"kind\": \"create\"", "\"kind\": \"nope\""),
    )
    .unwrap();
    assert_eq!(nums(&w, "d1").await, vec![1], "SV-24: skipped");
    let ign = ignored_paths(&events(&w));
    assert_eq!(ign, vec![".history/d1/7.meta.json", ".history/d1/8.meta.json", ".history/d1/9.meta.json"], "SV-24");
    assert_eq!(w.version(&uid("d1"), 7).await, Err(StoreError::VersionNotFound));
}

#[tokio::test]
async fn sv_25_crash_after_the_history_files_of_a_create() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    let d = doc("d1", "First");
    env.hooks().break_next_write();
    let r = w.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))));
    assert_eq!(w.get(&uid("d1")).await, Err(StoreError::NotFound));
    let s = w.create(&fid("f"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    assert_eq!((s.outcome, s.version), (Outcome::Written, 1), "SV-25");
    assert_eq!(w.get(&uid("d1")).await.unwrap().bytes.as_slice(), d.bytes());
    assert_eq!(nums(&w, "d1").await, vec![1]);
}

#[tokio::test]
async fn sv_26_reader_polls_while_a_writer_saves() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let r = AnyStore::Volume(env.reader().await);
    let catalog = Catalog::load(&r).await.unwrap();
    save(&w, &doc("d1", "Second"), d.etag(), 2).await.unwrap();
    catalog.poll(&r).await.outcome.unwrap();
    match &*catalog.get(&uid("d1")).unwrap() {
        CatalogEntry::Valid { current, .. } => {
            assert_eq!((current.version, current.saved_by.as_str()), (2, "alice"), "SV-26");
        }
        other => panic!("SV-26: {other:?}"),
    }
}

#[tokio::test]
async fn sv_27_file_appears_in_the_target_folder() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    mkfolder(&w, "g").await;
    let other = doc("d1", "Appeared");
    let hooks = env.hooks();
    let (ob, g, u) = (other.bytes().to_vec(), fid("g"), uid("d1"));
    env.faults.inject(
        FaultPoint::BeforeEffect,
        FaultAction::Run(std::sync::Arc::new(move || hooks.outside_write(&g, &u, &ob).unwrap())),
    );
    let r = w.move_to(&uid("d1"), &fid("g"), d.etag(), &fid("f"), &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Exists { .. })), "SV-27 {r:?}");
    assert_eq!(
        fs::read(env.path("g/d1.json")).unwrap(),
        other.bytes(),
        "SV-27: the file of the target does not change"
    );
    assert!(env.path("f/d1.json").exists());
    let h = w.history(&uid("d1"), None, 10).await.unwrap();
    assert_eq!((h[0].version, h[0].kind, h[0].save_id), (2, VersionKind::Move, None), "SV-27");
}

#[tokio::test]
async fn sv_28_crash_after_the_history_files_of_a_move() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    mkfolder(&w, "g").await;
    env.hooks().break_next_write();
    let r = w.move_to(&uid("d1"), &fid("g"), d.etag(), &fid("f"), &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))));
    assert!(env.path("f/d1.json").exists() && !env.path("g/d1.json").exists());
    let s = w.move_to(&uid("d1"), &fid("g"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    assert_eq!((s.outcome, s.version), (Outcome::Written, 2), "SV-28: not replayed");
    assert!(env.path("g/d1.json").exists() && !env.path("f/d1.json").exists(), "SV-28");
}

#[tokio::test]
async fn sv_29_conflict_at_the_check_then_a_retry() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let y = doc("d1", "Y");
    let hooks = env.hooks();
    let (yb, f, u) = (y.bytes().to_vec(), fid("f"), uid("d1"));
    env.faults.inject(
        FaultPoint::BeforeEffect,
        FaultAction::Run(std::sync::Arc::new(move || hooks.outside_write(&f, &u, &yb).unwrap())),
    );
    let s = doc("d1", "S");
    assert!(matches!(save(&w, &s, d.etag(), 2).await, Err(StoreError::Conflict(_))));
    let t = doc("d1", "T");
    save(&w, &t, y.etag(), 3).await.unwrap();
    let r = save(&w, &s, d.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SV-29: the normal path gives Conflict: {r:?}");
    assert_eq!(
        w.get(&uid("d1")).await.unwrap().bytes.as_slice(),
        t.bytes(),
        "SV-29: the content of S never becomes current"
    );
}

#[tokio::test]
async fn sx_01_fsync_error() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let before = env.faults.fsync_calls();
    env.faults.inject(FaultPoint::Fsync, FaultAction::Errno(5));
    let r = save(&w, &doc("d1", "Second"), d.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "SX-01 {r:?}");
    assert_eq!(env.faults.fsync_calls(), before + 1, "SX-01: no second fsync after the error");
    for dir in [env.path("f"), env.path(".history/d1")] {
        assert!(temp_files(&dir).is_empty(), "SX-01: the temporary file is removed");
    }
    assert_eq!(w.get(&uid("d1")).await.unwrap().bytes.as_slice(), d.bytes());
    assert_eq!(nums(&w, "d1").await, vec![1]);
}

#[tokio::test]
async fn catalog_sync_failure_and_recovery() {
    use hune_store::SyncTransition;
    let env = Env::new();
    let (w, _) = with_dashboard(&env).await;
    let catalog = Catalog::load(&w).await.unwrap();
    assert!(catalog.is_up());
    env.faults.inject(FaultPoint::Scan, FaultAction::Errno(5));
    let r = catalog.poll(&w).await;
    assert!(r.outcome.is_err());
    assert_eq!(r.transition, Some(SyncTransition::Failed));
    assert!(!catalog.is_up());
    assert_eq!(catalog.entries().len(), 1, "the catalog keeps its content");
    env.faults.inject(FaultPoint::Scan, FaultAction::Errno(5));
    let r = catalog.poll(&w).await;
    assert!(r.outcome.is_err() && r.transition.is_none(), "one transition only");
    let r = catalog.poll(&w).await;
    assert!(r.outcome.is_ok());
    assert!(matches!(r.transition, Some(SyncTransition::Recovered { .. })));
    assert!(catalog.is_up());
}

#[tokio::test]
async fn catalog_refresh_after_own_writes() {
    let env = Env::new();
    let (w, d) = with_dashboard(&env).await;
    let catalog = Catalog::load(&w).await.unwrap();
    let d2 = doc("d1", "Second");
    save(&w, &d2, d.etag(), 2).await.unwrap();
    assert!(catalog.refresh_uid(&w, &uid("d1")).await);
    assert_eq!(catalog.get(&uid("d1")).unwrap().etag(), d2.etag(), "visible before the next poll");
    mkfolder(&w, "g").await;
    assert!(catalog.refresh_folders(&w).await);
    assert_eq!(catalog.folders().len(), 2);
    w.move_to(&uid("d1"), &fid("g"), d2.etag(), &fid("f"), &meta(3)).await.unwrap();
    catalog.refresh_uid(&w, &uid("d1")).await;
    assert_eq!(catalog.get(&uid("d1")).unwrap().folder(), &fid("g"));
    w.delete(&uid("d1"), d2.etag(), &fid("g"), &meta(4)).await.unwrap();
    catalog.refresh_uid(&w, &uid("d1")).await;
    assert!(catalog.get(&uid("d1")).is_none());
    // The next poll finds nothing to change and no external event.
    let r = catalog.poll(&w).await;
    let s = r.outcome.unwrap();
    assert_eq!((s.entries_changed, s.entries_removed), (0, 0), "{s:?}");
    assert!(!r.events.iter().any(|e| matches!(e, StoreEvent::ExternalChange { .. })), "{:?}", r.events);
    assert_eq!(catalog.stats().dashboards, 0);
    let _ = meta_no_sid(0);
}

#[tokio::test]
async fn raw_current_reads_the_bytes_of_an_invalid_entry() {
    let env = Env::new();
    let w = AnyStore::Volume(env.writer().await);
    mkfolder(&w, "f").await;
    let catalog = Catalog::load(&w).await.unwrap();
    let full = doc("d1", "First");
    let truncated = &full.bytes()[..100];
    env.hooks().outside_write_in_place(&fid("f"), &uid("d1"), truncated).unwrap();
    catalog.poll(&w).await.outcome.unwrap();
    let CatalogEntry::Invalid { etag, .. } = &*catalog.get(&uid("d1")).unwrap() else { panic!("not invalid") };
    let (bytes, raw_etag) = w.raw_current(&uid("d1"), &fid("f")).await.unwrap();
    assert_eq!(bytes, truncated, "the bytes are as stored");
    assert_eq!(raw_etag, *etag, "the etag is the one of the catalog entry");
    assert_eq!(raw_etag, Etag::of(truncated));
    assert!(matches!(w.raw_current(&uid("d1"), &fid("g")).await, Err(StoreError::NotFound)), "other folder");
    assert!(matches!(w.raw_current(&uid("zz"), &fid("f")).await, Err(StoreError::NotFound)), "unknown uid");
}
