// SPDX-License-Identifier: MIT OR Apache-2.0
//! The common tests of 5.34. Each function is one test and gets a new empty store from the
//! backend. A failed check panics with the id of the test.

use std::sync::Arc;

use hune_core::document::validate;

use super::{Backend, Fixture, NODES_JSON, doc, doc_text, fid, meta, meta_no_sid, sid, uid};
use crate::catalog::Catalog;
use crate::copy::{TransferError, copy_store};
use crate::store::{AnyStore, Store};
use crate::types::{
    CanonicalDocument, CatalogEntry, Etag, FolderTitle, MAX_DASHBOARDS, MAX_FOLDERS, Outcome, Saved, StoreError,
    Subject, Uid, UtcMillis, ValidVersion, VersionKind, VersionMeta,
};

fn title(s: &str) -> FolderTitle {
    FolderTitle::parse(s).expect("test title")
}

async fn mkfolder(s: &AnyStore, id: &str) {
    s.create_folder(&fid(id), &title(&format!("Folder {id}"))).await.expect("create folder");
}

/// Creates a dashboard with the kind `create` and the save id of `n`.
async fn put(s: &AnyStore, folder: &str, d: &CanonicalDocument, n: u64) -> Saved {
    s.create(&fid(folder), d, VersionKind::Create, &meta(n)).await.expect("create")
}

/// Saves a new content with the kind `save`.
async fn upd(s: &AnyStore, folder: &str, d: &CanonicalDocument, if_match: &Etag, n: u64) -> Result<Saved, StoreError> {
    s.save(d, if_match, &fid(folder), VersionKind::Save, &meta(n)).await
}

async fn versions(s: &AnyStore, u: &str) -> Vec<VersionMeta> {
    s.history(&uid(u), None, 100).await.expect("history")
}

async fn numbers(s: &AnyStore, u: &str) -> Vec<u64> {
    versions(s, u).await.iter().map(|v| v.version).collect()
}

fn conflict_etag(e: StoreError) -> Etag {
    match e {
        StoreError::Conflict(c) => c.etag,
        other => panic!("expected Conflict, found {other:?}"),
    }
}

/// A store with the folder `f` and the dashboard `d1` created at version 1.
async fn one_dashboard(b: &impl Backend, keep: u32) -> (Fixture, CanonicalDocument) {
    let fx = b.new_store(keep).await;
    mkfolder(&fx.store, "f").await;
    let d = doc("d1", "First");
    put(&fx.store, "f", &d, 1).await;
    (fx, d)
}

/// SC-01: an empty store.
pub async fn sc_01(b: &impl Backend) {
    let fx = b.new_store(100).await;
    let snap = fx.store.load().await.expect("load");
    assert!(snap.folders.is_empty() && snap.entries.is_empty(), "SC-01");
    let ex = fx.store.export_begin().await.expect("export");
    assert!(ex.folders().await.unwrap().is_empty() && ex.uids().await.unwrap().is_empty(), "SC-01");
}

/// SC-02: create a folder.
pub async fn sc_02(b: &impl Backend) {
    let fx = b.new_store(100).await;
    fx.store.create_folder(&fid("transit"), &title("Transit")).await.unwrap();
    let snap = fx.store.load().await.unwrap();
    assert_eq!(snap.folders.len(), 1, "SC-02");
    match &snap.folders[0] {
        crate::store::FolderState::Valid(r) => assert_eq!((r.id.as_str(), r.title.as_str()), ("transit", "Transit")),
        other => panic!("SC-02: {other:?}"),
    }
}

/// SC-03: create a folder that exists.
pub async fn sc_03(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let e = fx.store.create_folder(&fid("f"), &title("Other")).await.unwrap_err();
    assert!(matches!(e, StoreError::Exists { .. }), "SC-03: {e:?}");
}

/// SC-04: rename a folder.
pub async fn sc_04(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    fx.store.rename_folder(&fid("f"), &title("New title")).await.unwrap();
    fx.store.rename_folder(&fid("f"), &title("New title")).await.unwrap();
    let snap = fx.store.load().await.unwrap();
    assert_eq!(snap.folders.len(), 1, "SC-04");
    match &snap.folders[0] {
        crate::store::FolderState::Valid(r) => assert_eq!((r.id.as_str(), r.title.as_str()), ("f", "New title")),
        other => panic!("SC-04: {other:?}"),
    }
}

/// SC-05: rename or delete an absent folder.
pub async fn sc_05(b: &impl Backend) {
    let fx = b.new_store(100).await;
    assert_eq!(fx.store.rename_folder(&fid("nope"), &title("X")).await, Err(StoreError::FolderNotFound), "SC-05");
    assert_eq!(fx.store.delete_folder(&fid("nope")).await, Err(StoreError::FolderNotFound), "SC-05");
}

/// SC-06: delete an empty folder.
pub async fn sc_06(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    fx.store.delete_folder(&fid("f")).await.unwrap();
    assert!(fx.store.load().await.unwrap().folders.is_empty(), "SC-06");
}

/// SC-07: delete a folder with a dashboard.
pub async fn sc_07(b: &impl Backend) {
    let (fx, _) = one_dashboard(b, 100).await;
    assert_eq!(fx.store.delete_folder(&fid("f")).await, Err(StoreError::FolderNotEmpty), "SC-07");
}

/// SC-08: create a dashboard.
pub async fn sc_08(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let d = doc("d1", "First");
    let s = put(&fx.store, "f", &d, 1).await;
    assert_eq!((s.version, s.outcome, s.etag), (1, Outcome::Written, Some(*d.etag())), "SC-08");
    let cur = fx.store.get(&uid("d1")).await.unwrap();
    assert_eq!(cur.bytes.as_slice(), d.bytes(), "SC-08");
    assert_eq!(cur.etag, Etag::of(d.bytes()), "SC-08");
    let h = versions(&fx.store, "d1").await;
    assert_eq!(h.len(), 1, "SC-08");
    assert_eq!(h[0].kind, VersionKind::Create, "SC-08");
}

/// SC-09: create in an absent folder.
pub async fn sc_09(b: &impl Backend) {
    let fx = b.new_store(100).await;
    let e = fx.store.create(&fid("nope"), &doc("d1", "A"), VersionKind::Create, &meta(1)).await;
    assert_eq!(e, Err(StoreError::FolderNotFound), "SC-09");
    assert_eq!(fx.store.history(&uid("d1"), None, 10).await, Err(StoreError::NotFound), "SC-09");
}

/// SC-10: create a uid that exists, in the same folder and in another folder.
pub async fn sc_10(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    mkfolder(&fx.store, "g").await;
    for f in ["f", "g"] {
        let e = fx.store.create(&fid(f), &d, VersionKind::Create, &meta(2)).await.unwrap_err();
        assert!(matches!(e, StoreError::Exists { .. }), "SC-10: {e:?}");
    }
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-10");
}

/// SC-11: create a deleted uid.
pub async fn sc_11(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    fx.store.delete(&uid("d1"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    let e = fx.store.create(&fid("f"), &d, VersionKind::Create, &meta(3)).await.unwrap_err();
    assert!(matches!(e, StoreError::Exists { .. }), "SC-11: {e:?}");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-11");
}

/// SC-12: save with the current etag.
pub async fn sc_12(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    let s = upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    assert_eq!((s.version, s.outcome, s.etag), (2, Outcome::Written, Some(*d2.etag())), "SC-12");
    let h = versions(&fx.store, "d1").await;
    assert_eq!(h.len(), 2, "SC-12");
    let m = meta(2);
    assert_eq!(h[0].kind, VersionKind::Save, "SC-12");
    assert_eq!(h[0].message, m.message, "SC-12");
    assert_eq!(h[0].saved_by, m.saved_by, "SC-12");
    assert_eq!(h[0].saved_at, m.saved_at, "SC-12");
    assert_eq!(h[0].save_id, m.save_id, "SC-12");
    let cur = fx.store.get(&uid("d1")).await.unwrap();
    assert_eq!((cur.version, cur.saved_by), (2, m.saved_by), "SC-12");
}

/// SC-13: save with an old etag.
pub async fn sc_13(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let before = fx.store.get(&uid("d1")).await.unwrap();
    let e = upd(&fx.store, "f", &doc("d1", "Third"), d.etag(), 3).await.unwrap_err();
    assert_eq!(conflict_etag(e), *d2.etag(), "SC-13");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap(), before, "SC-13");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-13");
}

/// SC-14: save the same content.
pub async fn sc_14(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let s = upd(&fx.store, "f", &d, d.etag(), 2).await.unwrap();
    assert_eq!((s.outcome, s.version), (Outcome::Unchanged, 1), "SC-14");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-14");
}

/// SC-15: save an absent uid.
pub async fn sc_15(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let e = upd(&fx.store, "f", &doc("d1", "A"), &Etag::of(b"x"), 1).await;
    assert_eq!(e, Err(StoreError::NotFound), "SC-15");
}

/// SC-16: save two times with one save id.
pub async fn sc_16(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    let first = upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let second = upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    assert_eq!(first.outcome, Outcome::Written, "SC-16");
    assert_eq!(second.outcome, Outcome::Replayed, "SC-16");
    assert_eq!((second.version, second.etag), (first.version, first.etag), "SC-16");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-16");
}

/// SC-17: create two times with one save id.
pub async fn sc_17(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let d = doc("d1", "First");
    let first = put(&fx.store, "f", &d, 1).await;
    let second = put(&fx.store, "f", &d, 1).await;
    assert_eq!((first.outcome, second.outcome), (Outcome::Written, Outcome::Replayed), "SC-17");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-17");
}

/// SC-18: a save id of another request.
pub async fn sc_18(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    mkfolder(&fx.store, "g").await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let current = *d2.etag();
    let reused = |r: Result<Saved, StoreError>, what: &str| {
        assert_eq!(r, Err(StoreError::SaveIdReused), "SC-18 {what}");
    };
    // The same save id and another content.
    reused(upd(&fx.store, "f", &doc("d1", "Other"), &current, 2).await, "save");
    // A move with the save id of a save.
    reused(fx.store.move_to(&uid("d1"), &fid("g"), &current, &fid("f"), &meta(2)).await, "move");
    // A delete with the save id of a save.
    reused(fx.store.delete(&uid("d1"), &current, &fid("f"), &meta(2)).await, "delete");
    // A restore with the save id of a save of the same bytes: version 2 has the bytes of d2.
    let r = fx.store.restore(&uid("d1"), 2, Some(&current), Some(&fid("f")), &meta(2)).await;
    reused(r, "restore");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-18");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap().folder, fid("f"), "SC-18");
}

/// SC-19: 16 saves at the same time with one `if_match`.
pub async fn sc_19(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let store = Arc::new(fx.store.clone());
    let mut tasks = Vec::new();
    for i in 0..16u64 {
        let (store, etag) = (store.clone(), *d.etag());
        tasks.push(tokio::spawn(async move {
            let d = doc("d1", &format!("Content {i}"));
            upd(&store, "f", &d, &etag, 100 + i).await
        }));
    }
    let (mut written, mut conflicts) = (0, 0);
    for t in tasks {
        match t.await.unwrap() {
            Ok(s) if s.outcome == Outcome::Written => written += 1,
            Err(StoreError::Conflict(_)) => conflicts += 1,
            other => panic!("SC-19: {other:?}"),
        }
    }
    assert_eq!((written, conflicts), (1, 15), "SC-19");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-19");
}

/// SC-20: 16 creates of one uid at the same time.
pub async fn sc_20(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let store = Arc::new(fx.store.clone());
    let mut tasks = Vec::new();
    for i in 0..16u64 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            let d = doc("d1", &format!("Content {i}"));
            store.create(&fid("f"), &d, VersionKind::Create, &meta_no_sid(i)).await
        }));
    }
    let (mut written, mut exists) = (0, 0);
    for t in tasks {
        match t.await.unwrap() {
            Ok(s) if s.outcome == Outcome::Written => written += 1,
            Err(StoreError::Exists { .. }) => exists += 1,
            other => panic!("SC-20: {other:?}"),
        }
    }
    assert_eq!((written, exists), (1, 15), "SC-20");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-20");
}

/// SC-21: 16 saves at the same time on 16 uids.
pub async fn sc_21(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let mut docs = Vec::new();
    for i in 0..16u64 {
        let d = doc(&format!("d{i}"), "First");
        put(&fx.store, "f", &d, i).await;
        docs.push(d);
    }
    let store = Arc::new(fx.store.clone());
    let mut tasks = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let (store, etag) = (store.clone(), *d.etag());
        tasks.push(tokio::spawn(async move {
            let d2 = doc(&format!("d{i}"), "Second");
            upd(&store, "f", &d2, &etag, 100 + i as u64).await
        }));
    }
    for t in tasks {
        let s = t.await.unwrap().expect("SC-21");
        assert_eq!(s.outcome, Outcome::Written, "SC-21");
    }
}

/// SC-22: move.
pub async fn sc_22(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    mkfolder(&fx.store, "g").await;
    let s = fx.store.move_to(&uid("d1"), &fid("g"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    assert_eq!((s.version, s.outcome, s.etag, s.folder.as_str()), (2, Outcome::Written, Some(*d.etag()), "g"), "SC-22");
    let cur = fx.store.get(&uid("d1")).await.unwrap();
    assert_eq!((cur.folder, cur.etag), (fid("g"), *d.etag()), "SC-22");
    let h = versions(&fx.store, "d1").await;
    assert_eq!((h[0].kind, h[0].folder.clone()), (VersionKind::Move, fid("g")), "SC-22");
}

/// SC-23: move to an absent folder, with an old etag, to the same folder.
pub async fn sc_23(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let r = fx.store.move_to(&uid("d1"), &fid("nope"), d.etag(), &fid("f"), &meta(2)).await;
    assert_eq!(r, Err(StoreError::FolderNotFound), "SC-23");
    mkfolder(&fx.store, "g").await;
    let r = fx.store.move_to(&uid("d1"), &fid("g"), &Etag::of(b"old"), &fid("f"), &meta(3)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-23: {r:?}");
    let r = fx.store.move_to(&uid("d1"), &fid("f"), d.etag(), &fid("f"), &meta(4)).await.unwrap();
    assert_eq!(r.outcome, Outcome::Unchanged, "SC-23");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-23");
}

/// SC-24: delete.
pub async fn sc_24(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let s = fx.store.delete(&uid("d1"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    assert_eq!((s.version, s.etag, s.outcome), (2, None, Outcome::Written), "SC-24");
    assert_eq!(fx.store.get(&uid("d1")).await, Err(StoreError::NotFound), "SC-24");
    let h = versions(&fx.store, "d1").await;
    assert_eq!(h.iter().map(|v| v.version).collect::<Vec<_>>(), vec![2, 1], "SC-24");
    assert_eq!((h[0].kind, h[0].etag), (VersionKind::Delete, None), "SC-24");
    let v = fx.store.version(&uid("d1"), 2).await.unwrap();
    assert!(v.bytes.is_none() && v.meta.kind == VersionKind::Delete, "SC-24");
}

/// SC-25: delete with an old etag.
pub async fn sc_25(b: &impl Backend) {
    let (fx, _) = one_dashboard(b, 100).await;
    let r = fx.store.delete(&uid("d1"), &Etag::of(b"old"), &fid("f"), &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-25: {r:?}");
    assert!(fx.store.get(&uid("d1")).await.is_ok(), "SC-25");
}

/// SC-26: restore a version of a dashboard that exists.
pub async fn sc_26(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let s = fx.store.restore(&uid("d1"), 1, Some(d2.etag()), Some(&fid("f")), &meta(3)).await.unwrap();
    assert_eq!((s.version, s.etag, s.outcome), (3, Some(*d.etag()), Outcome::Written), "SC-26");
    let h = versions(&fx.store, "d1").await;
    assert_eq!((h[0].kind, h[0].etag), (VersionKind::Restore, Some(*d.etag())), "SC-26");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap().bytes.as_slice(), d.bytes(), "SC-26");
    // The restore of the API: the caller validates the version, and the store validates it no more (6.18).
    let checks = fx.store.restore_checks();
    let v2 = ValidVersion::check(&uid("d1"), &fx.store.version(&uid("d1"), 2).await.unwrap()).unwrap();
    let s = fx.store.restore_valid(&v2, Some(d.etag()), Some(&fid("f")), &meta(4)).await.unwrap();
    assert_eq!((s.version, s.etag, s.outcome), (4, Some(*d2.etag()), Outcome::Written), "SC-26");
    assert_eq!(fx.store.restore_checks(), checks, "SC-26: the store validated the version again");
}

/// SC-27: restore with an old or an absent `if_match`.
pub async fn sc_27(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let old = fx.store.restore(&uid("d1"), 1, Some(d.etag()), Some(&fid("f")), &meta(3)).await;
    assert!(matches!(old, Err(StoreError::Conflict(_))), "SC-27: {old:?}");
    let none = fx.store.restore(&uid("d1"), 1, None, Some(&fid("f")), &meta(4)).await;
    assert!(matches!(none, Err(StoreError::Conflict(_))), "SC-27: {none:?}");
}

/// SC-28: restore a tombstone, an absent version, a version removed by the retention.
pub async fn sc_28(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 3).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    fx.store.delete(&uid("d1"), d2.etag(), &fid("f"), &meta(3)).await.unwrap();
    let r = fx.store.restore(&uid("d1"), 3, None, None, &meta(4)).await;
    assert_eq!(r, Err(StoreError::NotRestorable), "SC-28");
    let r = fx.store.restore(&uid("d1"), 99, None, None, &meta(5)).await;
    assert_eq!(r, Err(StoreError::VersionNotFound), "SC-28");
    // Keep 3: the next write removes version 1.
    let d3 = doc("d1", "Third");
    let early = ValidVersion::check(&uid("d1"), &fx.store.version(&uid("d1"), 1).await.unwrap()).unwrap();
    fx.store.restore(&uid("d1"), 2, None, None, &meta(6)).await.unwrap();
    let r = fx.store.restore(&uid("d1"), 1, Some(d2.etag()), Some(&fid("f")), &meta(7)).await;
    assert_eq!(r, Err(StoreError::VersionNotFound), "SC-28");
    // A version that the retention removed after the read of the caller: the store reads it again under its lock.
    let r = fx.store.restore_valid(&early, Some(d2.etag()), Some(&fid("f")), &meta(8)).await;
    assert_eq!(r, Err(StoreError::VersionNotFound), "SC-28");
    let _ = d3;
}

/// SC-29: restore a deleted dashboard.
pub async fn sc_29(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    fx.store.delete(&uid("d1"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    let r = fx.store.restore(&uid("d1"), 1, Some(d.etag()), None, &meta(3)).await;
    assert_eq!(r, Err(StoreError::NotFound), "SC-29");
    let s = fx.store.restore(&uid("d1"), 1, None, None, &meta(4)).await.unwrap();
    assert_eq!((s.version, s.folder.as_str(), s.etag), (3, "f", Some(*d.etag())), "SC-29");
    let cur = fx.store.get(&uid("d1")).await.unwrap();
    assert_eq!((cur.folder, cur.version), (fid("f"), 3), "SC-29");
}

/// SC-30: restore a deleted dashboard whose folder is deleted.
pub async fn sc_30(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    fx.store.delete(&uid("d1"), d.etag(), &fid("f"), &meta(2)).await.unwrap();
    fx.store.delete_folder(&fid("f")).await.unwrap();
    let r = fx.store.restore(&uid("d1"), 1, None, None, &meta(3)).await;
    assert_eq!(r, Err(StoreError::FolderNotFound), "SC-30");
}

/// SC-31: restore a version of another format version (hook). Formats 1 and 2 are the formats
/// before the migrations of 4.42; format 4 is a later format.
pub async fn sc_31(b: &impl Backend) {
    let (fx, _) = one_dashboard(b, 100).await;
    for (n, from) in [(2u64, "\"format_version\": 1"), (3, "\"format_version\": 2"), (4, "\"format_version\": 4")] {
        let other = doc_text("d1", "Old format").replacen("\"format_version\": 3", from, 1);
        let m = VersionMeta {
            uid: uid("d1"),
            version: n,
            kind: VersionKind::Save,
            folder: fid("f"),
            etag: Some(Etag::of(other.as_bytes())),
            saved_at: UtcMillis::from_millis(1_794_211_200_000),
            saved_by: Subject::anonymous(),
            message: Default::default(),
            save_id: None,
            size: None,
        };
        fx.hooks.inject_version(&m, Some(other.as_bytes())).await;
        let cur = fx.store.get(&uid("d1")).await.unwrap();
        let r = fx.store.restore(&uid("d1"), n, Some(&cur.etag), Some(&fid("f")), &meta(3)).await;
        assert_eq!(r, Err(StoreError::NotRestorable), "SC-31 ({from})");
    }
}

/// SC-32: history pages.
pub async fn sc_32(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 200).await;
    let mut etag = *d.etag();
    for i in 2..=103u64 {
        let nd = doc("d1", &format!("Title {i}"));
        upd(&fx.store, "f", &nd, &etag, i).await.unwrap();
        etag = *nd.etag();
    }
    let u = uid("d1");
    let page = |h: Vec<VersionMeta>| h.iter().map(|v| v.version).collect::<Vec<_>>();
    assert_eq!(page(fx.store.history(&u, None, 4).await.unwrap()), vec![103, 102, 101, 100], "SC-32");
    assert_eq!(page(fx.store.history(&u, Some(100), 3).await.unwrap()), vec![99, 98, 97], "SC-32");
    assert_eq!(fx.store.history(&u, None, 1000).await.unwrap().len(), 100, "SC-32");
    assert_eq!(page(fx.store.history(&u, Some(3), 10).await.unwrap()), vec![2, 1], "SC-32");
}

/// SC-33: the history of a uid with no current dashboard and no version.
pub async fn sc_33(b: &impl Backend) {
    let fx = b.new_store(100).await;
    assert_eq!(fx.store.history(&uid("nope"), None, 10).await, Err(StoreError::NotFound), "SC-33");
}

/// SC-34: retention with `history_keep` = 3.
pub async fn sc_34(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 3).await;
    let mut etag = *d.etag();
    for i in 2..=5u64 {
        let nd = doc("d1", &format!("Title {i}"));
        upd(&fx.store, "f", &nd, &etag, i).await.unwrap();
        etag = *nd.etag();
    }
    assert_eq!(numbers(&fx.store, "d1").await, vec![5, 4, 3], "SC-34");
    let s = upd(&fx.store, "f", &doc("d1", "Title 6"), &etag, 6).await.unwrap();
    assert_eq!(s.version, 6, "SC-34");
}

/// SC-35: retention after a delete.
pub async fn sc_35(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 3).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    let d3 = doc("d1", "Third");
    upd(&fx.store, "f", &d3, d2.etag(), 3).await.unwrap();
    fx.store.delete(&uid("d1"), d3.etag(), &fid("f"), &meta(4)).await.unwrap();
    let h = versions(&fx.store, "d1").await;
    assert_eq!(h.iter().map(|v| v.version).collect::<Vec<_>>(), vec![4, 3, 2], "SC-35");
    assert_eq!(h[0].kind, VersionKind::Delete, "SC-35");
    assert_eq!(fx.store.version(&uid("d1"), 1).await, Err(StoreError::VersionNotFound), "SC-35");
}

/// SC-36: read each kept version.
pub async fn sc_36(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    upd(&fx.store, "f", &d2, d.etag(), 2).await.unwrap();
    for (n, dd, kind) in [(1u64, &d, VersionKind::Create), (2, &d2, VersionKind::Save)] {
        let v = fx.store.version(&uid("d1"), n).await.unwrap();
        assert_eq!(v.bytes.as_deref(), Some(dd.bytes()), "SC-36");
        assert_eq!((v.meta.version, v.meta.kind, v.meta.etag), (n, kind, Some(*dd.etag())), "SC-36");
        assert_eq!(v.meta.size, Some(dd.bytes().len() as u64), "SC-36");
        assert_eq!(v.meta.saved_by, meta(n).saved_by, "SC-36");
        assert_eq!(v.meta.message, meta(n).message, "SC-36");
    }
}

/// SC-37: an outside change, then a save with the new etag.
pub async fn sc_37(b: &impl Backend) {
    let (fx, _) = one_dashboard(b, 100).await;
    let outside = doc("d1", "Outside");
    fx.hooks.outside_write(&fid("f"), &uid("d1"), outside.bytes()).await;
    let d3 = doc("d1", "Third");
    let s = upd(&fx.store, "f", &d3, outside.etag(), 3).await.unwrap();
    assert_eq!((s.version, s.outcome), (3, Outcome::Written), "SC-37");
    let h = versions(&fx.store, "d1").await;
    assert_eq!(
        h.iter().map(|v| (v.version, v.kind)).collect::<Vec<_>>(),
        vec![(3, VersionKind::Save), (2, VersionKind::External), (1, VersionKind::Create),],
        "SC-37"
    );
    assert_eq!(h[1].saved_by.as_str(), Subject::EXTERNAL, "SC-37");
    let v = fx.store.version(&uid("d1"), 2).await.unwrap();
    assert_eq!(v.bytes.as_deref(), Some(outside.bytes()), "SC-37");
}

/// SC-38: an outside change, then a save with the old etag.
pub async fn sc_38(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let outside = doc("d1", "Outside");
    fx.hooks.outside_write(&fid("f"), &uid("d1"), outside.bytes()).await;
    let r = upd(&fx.store, "f", &doc("d1", "Third"), d.etag(), 3).await.unwrap_err();
    assert_eq!(conflict_etag(r), *outside.etag(), "SC-38");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-38");
}

/// SC-39: a second store on the same backend polls and sees each change.
pub async fn sc_39(b: &impl Backend) {
    let fx = b.new_store(100).await;
    let second = fx.hooks.second_store().await;
    let catalog = Catalog::load(&second).await.expect("load");
    let s = &fx.store;
    let ids =
        |c: &Catalog| c.entries().iter().map(|e| (e.uid().to_string(), e.folder().to_string())).collect::<Vec<_>>();
    // A folder create.
    mkfolder(s, "f").await;
    mkfolder(s, "g").await;
    catalog.poll(&second).await.outcome.expect("poll");
    assert_eq!(catalog.folders().len(), 2, "SC-39 folder create");
    // A create.
    let d = doc("d1", "First");
    put(s, "f", &d, 1).await;
    catalog.poll(&second).await.outcome.expect("poll");
    assert_eq!(ids(&catalog), vec![("d1".to_owned(), "f".to_owned())], "SC-39 create");
    // A save.
    let d2 = doc("d1", "Second");
    upd(s, "f", &d2, d.etag(), 2).await.unwrap();
    catalog.poll(&second).await.outcome.expect("poll");
    let e = catalog.get(&uid("d1")).expect("entry");
    assert_eq!(e.etag(), d2.etag(), "SC-39 save");
    if let CatalogEntry::Valid { current, .. } = &*e {
        assert_eq!(current.version, 2, "SC-39 save");
    }
    // A move.
    s.move_to(&uid("d1"), &fid("g"), d2.etag(), &fid("f"), &meta(3)).await.unwrap();
    catalog.poll(&second).await.outcome.expect("poll");
    assert_eq!(ids(&catalog), vec![("d1".to_owned(), "g".to_owned())], "SC-39 move");
    // A delete.
    s.delete(&uid("d1"), d2.etag(), &fid("g"), &meta(4)).await.unwrap();
    catalog.poll(&second).await.outcome.expect("poll");
    assert!(catalog.entries().is_empty(), "SC-39 delete");
    // A folder rename and delete.
    s.rename_folder(&fid("f"), &title("Renamed")).await.unwrap();
    catalog.poll(&second).await.outcome.expect("poll");
    assert!(catalog.folders().iter().any(|f| f.id == fid("f") && f.title.as_str() == "Renamed"), "SC-39 rename");
    s.delete_folder(&fid("f")).await.unwrap();
    catalog.poll(&second).await.outcome.expect("poll");
    assert_eq!(catalog.folders().len(), 1, "SC-39 folder delete");
}

/// SC-40: an invalid stored document (hook).
pub async fn sc_40(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let catalog = Catalog::load(&fx.store).await.unwrap();
    fx.hooks.outside_write(&fid("f"), &uid("bad"), b"{ \"not\": valid").await;
    let snap = fx.store.load().await.unwrap();
    assert!(matches!(&*snap.entries[0], CatalogEntry::Invalid { errors, .. } if !errors.is_empty()), "SC-40 load");
    catalog.poll(&fx.store).await.outcome.expect("poll");
    let e = catalog.get(&uid("bad")).expect("entry");
    assert!(matches!(&*e, CatalogEntry::Invalid { .. }), "SC-40 poll");
    assert!(matches!(fx.store.get(&uid("bad")).await, Err(StoreError::Invalid(_))), "SC-40 get");
}

/// SC-41: limits.
pub async fn sc_41(b: &impl Backend) {
    let fx = b.new_store(100).await;
    fx.hooks.fill_folders(MAX_FOLDERS).await;
    let r = fx.store.create_folder(&fid("one-more"), &title("X")).await;
    assert_eq!(r, Err(StoreError::Full), "SC-41 folders");
    drop(fx);
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "d").await;
    let docs = (0..MAX_DASHBOARDS - 1).map(|i| {
        let u = format!("fill-{i}");
        (uid(&u), doc(&u, "Fill").bytes().to_vec())
    });
    fx.hooks.fill_dashboards(&fid("d"), docs).await;
    let last = doc("last", "Last");
    let s = fx.store.create(&fid("d"), &last, VersionKind::Create, &meta(1)).await.expect("SC-41: the 10000th");
    assert_eq!(s.outcome, Outcome::Written, "SC-41");
    let over = doc("over", "Over");
    let r = fx.store.create(&fid("d"), &over, VersionKind::Create, &meta(2)).await;
    assert_eq!(r, Err(StoreError::Full), "SC-41 dashboards");
}

/// SC-42: time precision.
pub async fn sc_42(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let m = meta(2);
    assert_eq!(m.saved_at.as_millis() % 1000, 123);
    let d2 = doc("d1", "Second");
    fx.store.save(&d2, d.etag(), &fid("f"), VersionKind::Save, &m).await.unwrap();
    let v = fx.store.version(&uid("d1"), 2).await.unwrap();
    assert_eq!(v.meta.saved_at, m.saved_at, "SC-42");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap().saved_at, m.saved_at, "SC-42");
}

/// SC-43: the fixture of 4.40.
pub async fn sc_43(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "transit").await;
    let d = validate(NODES_JSON.as_bytes(), None).expect("the fixture is valid");
    fx.store.create(&fid("transit"), &d, VersionKind::Create, &meta(1)).await.unwrap();
    let cur = fx.store.get(&uid("nodes")).await.unwrap();
    assert_eq!(cur.bytes.as_slice(), d.bytes(), "SC-43");
    assert_eq!(cur.etag, *d.etag(), "SC-43");
    // When the core writes the canonical form, the fixture is its own canonical form.
    if d.bytes() == NODES_JSON.as_bytes() {
        assert_eq!(cur.bytes.len(), 8397, "SC-43");
        assert_eq!(cur.etag.to_hex(), "50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501", "SC-43");
    }
}

/// SC-45: copy into a store that is not empty.
pub async fn sc_45(b: &impl Backend) {
    let (from, _) = one_dashboard(b, 100).await;
    let (to, _) = one_dashboard(b, 100).await;
    let r = copy_store(&from.store, &to.store).await;
    assert!(matches!(r, Err(TransferError::Store(StoreError::NotEmpty))), "SC-45: {r:?}");
    assert_eq!(numbers(&to.store, "d1").await, vec![1], "SC-45");
}

/// SC-46: copy of a store with an invalid current document.
pub async fn sc_46(b: &impl Backend) {
    let from = b.new_store(100).await;
    mkfolder(&from.store, "f").await;
    from.hooks.outside_write(&fid("f"), &uid("bad"), b"not json").await;
    let to = b.new_store(100).await;
    let r = copy_store(&from.store, &to.store).await;
    assert!(matches!(r, Err(TransferError::InvalidSource(_))), "SC-46: {r:?}");
    assert!(to.store.load().await.unwrap().folders.is_empty(), "SC-46");
}

/// SC-48: the backend breaks during a save (hook), then works again.
pub async fn sc_48(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    let d2 = doc("d1", "Second");
    fx.hooks.break_next_write().await;
    let r = upd(&fx.store, "f", &d2, d.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Unavailable(_))), "SC-48: {r:?}");
    let r = upd(&fx.store, "f", &d2, d.etag(), 2).await.expect("SC-48 retry");
    assert!(matches!(r.outcome, Outcome::Written | Outcome::Replayed), "SC-48");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap().bytes.as_slice(), d2.bytes(), "SC-48");
    assert_eq!(numbers(&fx.store, "d1").await, vec![2, 1], "SC-48");
}

/// SC-49: a write with the current etag and an `expected_folder` that is not the current folder.
pub async fn sc_49(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 100).await;
    mkfolder(&fx.store, "g").await;
    let wrong = fid("g");
    let r = fx.store.save(&doc("d1", "Second"), d.etag(), &wrong, VersionKind::Save, &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-49 save: {r:?}");
    let r = fx.store.move_to(&uid("d1"), &fid("g"), d.etag(), &wrong, &meta(3)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-49 move: {r:?}");
    let r = fx.store.delete(&uid("d1"), d.etag(), &wrong, &meta(4)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-49 delete: {r:?}");
    let r = fx.store.restore(&uid("d1"), 1, Some(d.etag()), Some(&wrong), &meta(5)).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-49 restore: {r:?}");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-49");
    assert_eq!(fx.store.get(&uid("d1")).await.unwrap().folder, fid("f"), "SC-49");
}

/// SC-50: the current document is not valid (hook).
pub async fn sc_50(b: &impl Backend) {
    let (fx, _) = one_dashboard(b, 100).await;
    mkfolder(&fx.store, "g").await;
    let garbage = b"{ this is not a dashboard";
    fx.hooks.outside_write(&fid("f"), &uid("d1"), garbage).await;
    let raw = Etag::of(garbage);
    let r = fx.store.move_to(&uid("d1"), &fid("g"), &raw, &fid("f"), &meta(2)).await;
    assert!(matches!(r, Err(StoreError::Invalid(_))), "SC-50 move: {r:?}");
    assert_eq!(numbers(&fx.store, "d1").await, vec![1], "SC-50");
    let d3 = doc("d1", "Third");
    let s = upd(&fx.store, "f", &d3, &raw, 3).await.expect("SC-50 save");
    assert_eq!(s.version, 3, "SC-50");
    let h = versions(&fx.store, "d1").await;
    assert_eq!((h[1].kind, h[1].etag), (VersionKind::External, Some(raw)), "SC-50");
    let v = fx.store.version(&uid("d1"), 2).await.unwrap();
    assert!(v.bytes.is_none(), "SC-50");
    let r = fx.store.restore(&uid("d1"), 2, Some(d3.etag()), Some(&fid("f")), &meta(4)).await;
    assert_eq!(r, Err(StoreError::NotRestorable), "SC-50");
}

/// SC-51: a uid with a current document and no version (hook).
pub async fn sc_51(b: &impl Backend) {
    let fx = b.new_store(100).await;
    mkfolder(&fx.store, "f").await;
    let d = doc("d1", "Outside");
    fx.hooks.outside_write(&fid("f"), &uid("d1"), d.bytes()).await;
    let cur = fx.store.get(&uid("d1")).await.unwrap();
    assert_eq!((cur.version, cur.saved_by.as_str()), (0, "external"), "SC-51");
    assert!(fx.store.history(&uid("d1"), None, 10).await.unwrap().is_empty(), "SC-51");
}

/// SC-52: retention with `history_keep` = 2, then a retry of an old save.
pub async fn sc_52(b: &impl Backend) {
    let (fx, d) = one_dashboard(b, 2).await;
    let a = doc("d1", "A");
    upd(&fx.store, "f", &a, d.etag(), 2).await.unwrap();
    let bb = doc("d1", "B");
    upd(&fx.store, "f", &bb, a.etag(), 3).await.unwrap();
    let c = doc("d1", "C");
    upd(&fx.store, "f", &c, bb.etag(), 4).await.unwrap();
    let r = upd(&fx.store, "f", &a, d.etag(), 2).await;
    assert!(matches!(r, Err(StoreError::Conflict(_))), "SC-52: {r:?}");
    let _ = sid(0);
}

/// The ids of the common tests that this backend-agnostic suite implements.
pub const IMPLEMENTED: &[&str] = &[
    "SC-01", "SC-02", "SC-03", "SC-04", "SC-05", "SC-06", "SC-07", "SC-08", "SC-09", "SC-10", "SC-11", "SC-12",
    "SC-13", "SC-14", "SC-15", "SC-16", "SC-17", "SC-18", "SC-19", "SC-20", "SC-21", "SC-22", "SC-23", "SC-24",
    "SC-25", "SC-26", "SC-27", "SC-28", "SC-29", "SC-30", "SC-31", "SC-32", "SC-33", "SC-34", "SC-35", "SC-36",
    "SC-37", "SC-38", "SC-39", "SC-40", "SC-41", "SC-42", "SC-43", "SC-45", "SC-46", "SC-48", "SC-49", "SC-50",
    "SC-51", "SC-52",
];

#[allow(dead_code)]
fn _uses(_: &Uid) {}
