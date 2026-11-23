// SPDX-License-Identifier: MIT OR Apache-2.0
//! The common tests that need two backends: SC-44 and SC-47 (5.34). Each function takes two
//! factories and a scratch directory. The tests call it in both directions, for example
//! volume to PostgreSQL and PostgreSQL to volume.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{Backend, Fixture, doc, fid, meta, uid};
use crate::copy::{copy_store, export_to_dir, import_from_dir};
use crate::store::Store;
use crate::types::{Etag, FolderTitle, VersionKind};

/// Fills an empty store with the cases that 5.34 names for SC-44. The store gets three folders
/// (one is empty), saves, a move, a delete (a tombstone) and a restore of a deleted dashboard.
/// It also gets a restore of a version and an outside change. At last, an invalid outside file
/// is replaced by a save: this gives an `external` version without bytes.
pub async fn populate(fx: &Fixture) {
    let s = &fx.store;
    for (id, title) in [("f", "Folder f"), ("g", "Folder g"), ("h", "Folder h")] {
        s.create_folder(&fid(id), &FolderTitle::parse(title).expect("title")).await.expect("create folder");
    }
    let f = fid("f");
    // d1: three versions.
    let d1 = doc("d1", "First");
    s.create(&f, &d1, VersionKind::Create, &meta(1)).await.expect("create d1");
    let d1b = doc("d1", "Second");
    s.save(&d1b, d1.etag(), &f, VersionKind::Save, &meta(2)).await.expect("save d1");
    let d1c = doc("d1", "Third");
    s.save(&d1c, d1b.etag(), &f, VersionKind::Save, &meta(3)).await.expect("save d1");
    // d2: a move.
    let d2 = doc("d2", "Moved");
    s.create(&f, &d2, VersionKind::Create, &meta(4)).await.expect("create d2");
    s.move_to(&uid("d2"), &fid("g"), d2.etag(), &f, &meta(5)).await.expect("move d2");
    // d3: a tombstone.
    let d3 = doc("d3", "Deleted");
    s.create(&f, &d3, VersionKind::Create, &meta(6)).await.expect("create d3");
    s.delete(&uid("d3"), d3.etag(), &f, &meta(7)).await.expect("delete d3");
    // d4: an outside change.
    let d4 = doc("d4", "Before");
    s.create(&f, &d4, VersionKind::Create, &meta(8)).await.expect("create d4");
    let outside = doc("d4", "Outside");
    fx.hooks.outside_write(&f, &uid("d4"), outside.bytes()).await;
    // d5: an invalid outside file, then a save.
    let d5 = doc("d5", "Fifth");
    s.create(&f, &d5, VersionKind::Create, &meta(9)).await.expect("create d5");
    let garbage = b"{ this is not a dashboard";
    fx.hooks.outside_write(&f, &uid("d5"), garbage).await;
    let d5b = doc("d5", "Fifth again");
    s.save(&d5b, &Etag::of(garbage), &f, VersionKind::Save, &meta(10)).await.expect("save d5");
    // d6: a restore of a deleted dashboard.
    let d6 = doc("d6", "Sixth");
    s.create(&f, &d6, VersionKind::Create, &meta(20)).await.expect("create d6");
    s.delete(&uid("d6"), d6.etag(), &f, &meta(21)).await.expect("delete d6");
    s.restore(&uid("d6"), 1, None, None, &meta(22)).await.expect("restore d6");
    // d7: a restore of a version of a dashboard that exists.
    let d7 = doc("d7", "Seventh");
    s.create(&f, &d7, VersionKind::Create, &meta(30)).await.expect("create d7");
    let d7b = doc("d7", "Seventh again");
    s.save(&d7b, d7.etag(), &f, VersionKind::Save, &meta(31)).await.expect("save d7");
    s.restore(&uid("d7"), 1, Some(d7b.etag()), Some(&f), &meta(32)).await.expect("restore d7");
}

/// The files of a directory tree: the relative path of each file and its bytes, and each directory.
fn tree(root: &Path) -> (BTreeMap<PathBuf, Vec<u8>>, Vec<PathBuf>) {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>, dirs: &mut Vec<PathBuf>) {
        let mut names: Vec<_> = std::fs::read_dir(dir).expect("read dir").map(|e| e.expect("entry").path()).collect();
        names.sort();
        for p in names {
            let rel = p.strip_prefix(root).expect("prefix").to_owned();
            if p.is_dir() {
                dirs.push(rel);
                walk(root, &p, files, dirs);
            } else {
                files.insert(rel, std::fs::read(&p).expect("read file"));
            }
        }
    }
    let (mut files, mut dirs) = (BTreeMap::new(), Vec::new());
    walk(root, root, &mut files, &mut dirs);
    (files, dirs)
}

/// Asserts that two directories hold the same directories and the same files with the same bytes.
pub fn assert_dirs_equal(a: &Path, b: &Path, id: &str) {
    let ((fa, da), (fb, db)) = (tree(a), tree(b));
    assert_eq!(da, db, "{id}: the directories differ");
    assert_eq!(fa.keys().collect::<Vec<_>>(), fb.keys().collect::<Vec<_>>(), "{id}: the files differ");
    for (p, bytes) in &fa {
        assert!(bytes == &fb[p], "{id}: the bytes of {} differ", p.display());
    }
    assert!(!fa.is_empty(), "{id}: the export is empty");
}

/// SC-44: export of a store of backend `a`, import into an empty store of backend `b`, export again.
/// The two directories are byte-equal.
pub async fn sc_44(a: &impl Backend, b: &impl Backend, scratch: &Path) {
    let src = a.new_store(100).await;
    populate(&src).await;
    let one = scratch.join("one");
    export_to_dir(&src.store, &one).await.expect("SC-44 first export");
    let dst = b.new_store(100).await;
    import_from_dir(&one, &dst.store).await.expect("SC-44 import");
    let two = scratch.join("two");
    export_to_dir(&dst.store, &two).await.expect("SC-44 second export");
    assert_dirs_equal(&one, &two, "SC-44");
}

/// SC-47: `copy_store` from backend `a` to backend `b` and back into a new store of backend `a`.
/// The files are byte-equal, and `copy_store` itself compares the folders, the versions and the
/// current documents.
pub async fn sc_47(a: &impl Backend, b: &impl Backend, scratch: &Path) {
    let src = a.new_store(100).await;
    populate(&src).await;
    let mid = b.new_store(100).await;
    let r1 = copy_store(&src.store, &mid.store).await.expect("SC-47 copy there");
    let back = a.new_store(100).await;
    let r2 = copy_store(&mid.store, &back.store).await.expect("SC-47 copy back");
    assert_eq!(r1, r2, "SC-47: the counts differ");
    let (one, two) = (scratch.join("src"), scratch.join("back"));
    export_to_dir(&src.store, &one).await.expect("SC-47 export of the source");
    export_to_dir(&back.store, &two).await.expect("SC-47 export of the copy");
    assert_dirs_equal(&one, &two, "SC-47");
    // The etags of each current document agree on the three stores.
    for u in ["d1", "d2", "d4", "d5", "d6", "d7"] {
        let (x, y, z) = (
            src.store.get(&uid(u)).await.expect("get"),
            mid.store.get(&uid(u)).await.expect("get"),
            back.store.get(&uid(u)).await.expect("get"),
        );
        assert_eq!((x.etag, x.folder.clone()), (y.etag, y.folder.clone()), "SC-47 {u}");
        assert_eq!((y.etag, y.folder), (z.etag, z.folder), "SC-47 {u}");
    }
}
