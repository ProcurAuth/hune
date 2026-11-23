//! The write side of the volume backend (5.13 to 5.15, 5.18).

use hune_core::document::folder_file_bytes;
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::time::UtcMillis;
use rustix::io::Errno;

use super::dir::{Dir, FType};
use super::faults::FaultPoint;
use super::meta::{encode_meta, json_name, meta_name};
use super::read::{CurFile, Head};
use super::{FOLDER_FILE, HISTORY_DIR, Inner, io_err};
use crate::store::FolderState;
use crate::types::{
    CanonicalDocument, ConflictInfo, Etag, FolderId, FolderTitle, MAX_DASHBOARDS, MAX_FOLDERS, Message, Outcome,
    SaveId, Saved, StoreError, Subject, Uid, VersionKind, VersionMeta, WriteMeta,
};

/// The data of a document that a write needs. It is `'static`, so it moves into a task of the thread pool for file work.
pub(crate) struct NewDoc {
    pub uid: Uid,
    pub etag: Etag,
    pub bytes: Vec<u8>,
}

impl NewDoc {
    pub(crate) fn of(doc: &CanonicalDocument) -> NewDoc {
        NewDoc { uid: doc.uid().clone(), etag: *doc.etag(), bytes: doc.bytes().to_vec() }
    }
}

/// The request, for the `save_id` check (5.5, rule 4).
#[derive(Clone)]
enum Req {
    /// `create`, `save`, `restore`, `import` and `migrate`: the kind and the new etag.
    /// `creating` is true when no current document exists.
    Save { kind: VersionKind, etag: Etag, creating: bool },
    /// `move_to`: the target folder.
    Move { target: FolderId },
    /// `delete`.
    Delete,
}

/// What the effect of a version expects in the file it writes.
enum Expect {
    /// The current file has this etag. `mode` is its mode.
    Etag { etag: Etag, mode: u32 },
    /// No file exists.
    Absent,
}

enum Pre {
    Done(Saved),
    Go { cur: Box<CurFile>, head: Head },
}

fn saved(uid: &Uid, folder: &FolderId, version: u64, etag: Option<Etag>, outcome: Outcome) -> Saved {
    Saved { uid: uid.clone(), folder: folder.clone(), version, etag, outcome }
}

fn not_unique(uid: &Uid) -> StoreError {
    StoreError::Invalid(vec![ValidationError::new(
        "",
        ErrorCode::NotUnique,
        format!("the uid {uid} is in more than one folder"),
    )])
}

impl Inner {
    fn fault(&self, point: FaultPoint) -> Result<(), StoreError> {
        self.faults.check(point).map_err(|e| io_err(e, format!("injected fault at {point:?}")))
    }

    /// Opens `.history`, and makes it when it is absent.
    fn history_root(&self) -> Result<Dir, StoreError> {
        match self.root.open_dir(HISTORY_DIR) {
            Ok(d) => Ok(d),
            Err(Errno::NOENT) => {
                match self.root.mkdir(HISTORY_DIR) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(e) => return Err(io_err(e, "make .history")),
                }
                self.root.fsync().map_err(|e| io_err(e, "fsync the root"))?;
                self.root.open_dir(HISTORY_DIR).map_err(|e| io_err(e, "open .history"))
            }
            Err(e) => Err(io_err(e, "open .history")),
        }
    }

    /// Opens `.history/<uid>`, and makes it when it is absent (5.14, step 9).
    fn history_dir_for_write(&self, uid: &Uid) -> Result<Dir, StoreError> {
        let root = self.history_root()?;
        match root.open_dir(uid.as_str()) {
            Ok(d) => Ok(d),
            Err(Errno::NOENT) => {
                match root.mkdir(uid.as_str()) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(e) => return Err(io_err(e, format!("make .history/{uid}"))),
                }
                root.fsync().map_err(|e| io_err(e, "fsync .history"))?;
                root.open_dir(uid.as_str()).map_err(|e| io_err(e, format!("open .history/{uid}")))
            }
            Err(e) => Err(io_err(e, format!("open .history/{uid}"))),
        }
    }

    /// Writes the files of one version: `<n>.json` first, then `<n>.meta.json` (5.14, steps 9, 10).
    fn write_version(&self, vm: &VersionMeta, bytes: Option<&[u8]>) -> Result<(), StoreError> {
        let dir = self.history_dir_for_write(&vm.uid)?;
        if let Some(b) = bytes {
            dir.write_atomic(&json_name(vm.version), b, None)
                .map_err(|e| io_err(e, format!("write .history/{}/{}", vm.uid, json_name(vm.version))))?;
        }
        if vm.kind != VersionKind::External {
            self.fault(FaultPoint::HistoryJsonWritten)?;
        }
        dir.write_atomic(&meta_name(vm.version), &encode_meta(vm), None)
            .map_err(|e| io_err(e, format!("write .history/{}/{}", vm.uid, meta_name(vm.version))))?;
        if vm.kind != VersionKind::External {
            self.fault(FaultPoint::HistoryWritten)?;
        }
        Ok(())
    }

    /// Writes the metadata file of a version again with `save_id` null (5.14, steps 5 and 12).
    /// A failure is not reported: the caller returns its own error.
    fn demote(&self, vm: &VersionMeta) {
        let mut m = vm.clone();
        m.save_id = None;
        if let Ok(dir) = self.history_dir_for_write(&m.uid) {
            let _ = dir.write_atomic(&meta_name(m.version), &encode_meta(&m), None);
        }
    }

    /// 5.14 step 8: write the version `external` when the head differs from the current file.
    /// Gives the number of the next version.
    fn record_external(&self, uid: &Uid, cur: &CurFile, head: &Head) -> Result<u64, StoreError> {
        let n = head.max_n + 1;
        if head.meta.as_ref().is_some_and(|h| h.etag == Some(cur.etag)) {
            return Ok(n);
        }
        let bytes = cur.doc.as_ref().ok().map(|d| d.bytes());
        let vm = VersionMeta {
            uid: uid.clone(),
            version: n,
            kind: VersionKind::External,
            folder: cur.folder.clone(),
            etag: Some(cur.etag),
            saved_at: cur.stat.mtime,
            saved_by: Subject::external(),
            message: Message::default(),
            save_id: None,
            size: bytes.map(|b| b.len() as u64),
        };
        self.write_version(&vm, bytes)?;
        Ok(n + 1)
    }

    fn conflict_of(&self, cur: &CurFile, head: &Head) -> StoreError {
        let (version, saved_at, saved_by) = Self::current_fields(&cur.etag, cur.stat.mtime, head.meta.as_ref());
        StoreError::Conflict(Box::new(ConflictInfo {
            etag: cur.etag,
            version,
            folder: cur.folder.clone(),
            saved_at,
            saved_by,
        }))
    }

    /// 5.14 steps 11 to 13: prepare the temporary file, check the file again, commit.
    fn apply_file_effect(
        &self,
        folder: &FolderId,
        uid: &Uid,
        bytes: &[u8],
        expect: Expect,
        vm: &VersionMeta,
    ) -> Result<(), StoreError> {
        let name = format!("{uid}.json");
        let Some(dir) = self.open_folder(folder)? else { return Err(StoreError::FolderNotFound) };
        let mode = match &expect {
            Expect::Etag { mode, .. } => Some(*mode),
            Expect::Absent => None,
        };
        let tmp = dir.prepare(&name, bytes, mode).map_err(|e| io_err(e, format!("write {folder}/{name}")))?;
        if let Err(e) = self.fault(FaultPoint::BeforeEffect) {
            dir.unlink_quiet(&tmp);
            return Err(e);
        }
        match expect {
            Expect::Etag { etag, .. } => {
                let now = self.read_dashboard(folder, uid);
                match now {
                    Ok(Some(c)) if c.etag == etag => {}
                    Ok(Some(c)) => {
                        dir.unlink_quiet(&tmp);
                        self.demote(vm);
                        let head = self.head(uid)?;
                        return Err(self.conflict_of(&c, &head));
                    }
                    Ok(None) => {
                        dir.unlink_quiet(&tmp);
                        self.demote(vm);
                        return Err(StoreError::NotFound);
                    }
                    Err(e) => {
                        dir.unlink_quiet(&tmp);
                        return Err(e);
                    }
                }
            }
            Expect::Absent => match dir.exists(&name) {
                Ok(false) => {}
                Ok(true) => {
                    dir.unlink_quiet(&tmp);
                    self.demote(vm);
                    return Err(StoreError::Exists { folder: Some(folder.clone()) });
                }
                Err(e) => {
                    dir.unlink_quiet(&tmp);
                    return Err(io_err(e, format!("stat {folder}/{name}")));
                }
            },
        }
        dir.commit(&tmp, &name).map_err(|e| io_err(e, format!("commit {folder}/{name}")))
    }

    /// 5.15 `move_to`, steps 6 and 7.
    fn apply_move_effect(&self, from: &FolderId, to: &FolderId, uid: &Uid, vm: &VersionMeta) -> Result<(), StoreError> {
        let name = format!("{uid}.json");
        let Some(src) = self.open_folder(from)? else { return Err(StoreError::NotFound) };
        let Some(dst) = self.open_folder(to)? else { return Err(StoreError::FolderNotFound) };
        self.fault(FaultPoint::BeforeEffect)?;
        match src.rename_noreplace(&name, &dst, &name) {
            Ok(()) => {}
            Err(Errno::EXIST) => {
                self.demote(vm);
                return Err(StoreError::Exists { folder: Some(to.clone()) });
            }
            Err(e) => return Err(io_err(e, format!("move {from}/{name} to {to}"))),
        }
        dst.fsync().map_err(|e| io_err(e, format!("fsync {to}")))?;
        src.fsync().map_err(|e| io_err(e, format!("fsync {from}")))
    }

    /// 5.15 `delete`, step 4.
    fn apply_delete_effect(&self, folder: &FolderId, uid: &Uid) -> Result<(), StoreError> {
        let name = format!("{uid}.json");
        let Some(dir) = self.open_folder(folder)? else { return Ok(()) };
        match dir.unlink(&name) {
            Ok(()) | Err(Errno::NOENT) => {}
            Err(e) => return Err(io_err(e, format!("remove {folder}/{name}"))),
        }
        dir.fsync().map_err(|e| io_err(e, format!("fsync {folder}")))
    }

    /// 5.18 item 3: remove the oldest versions while the uid has more than `history_keep`.
    /// A failure is not reported: the version and its effect are durable.
    fn apply_retention(&self, uid: &Uid) {
        let Ok(Some(dir)) = self.hist_dir(uid) else { return };
        let Ok(metas) = self.list_versions(uid, &dir) else { return };
        let keep = self.opts.history_keep as usize;
        if metas.len() <= keep {
            return;
        }
        for n in &metas[..metas.len() - keep] {
            if dir.unlink(&meta_name(*n)).is_err() {
                return;
            }
            dir.unlink_quiet(&json_name(*n));
        }
        let _ = dir.fsync();
    }

    fn count_dashboards(&self) -> Result<usize, StoreError> {
        let mut n = 0;
        for f in self.folder_ids()? {
            let Some(dir) = self.open_folder(&f)? else { continue };
            for item in dir.list().map_err(|e| io_err(e, format!("list the folder {f}")))? {
                let is_dash = item.ftype == FType::File
                    && item.name != FOLDER_FILE
                    && item.name.strip_suffix(".json").is_some_and(|s| Uid::parse(s).is_ok());
                if is_dash {
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn require_valid_folder(&self, folder: &FolderId) -> Result<(), StoreError> {
        match self.read_folder_state(folder)? {
            Some((FolderState::Valid(_), _)) => Ok(()),
            _ => Err(StoreError::FolderNotFound),
        }
    }

    // The `save_id` check of 5.14 step 5.

    fn find_by_save_id(&self, uid: &Uid, sid: SaveId) -> Result<Option<VersionMeta>, StoreError> {
        let Some(dir) = self.hist_dir(uid)? else { return Ok(None) };
        for n in self.list_versions(uid, &dir)?.iter().rev() {
            if let Some(m) = self.read_meta(&dir, uid, *n)?
                && m.save_id == Some(sid)
            {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }

    fn same_request(v: &VersionMeta, req: &Req) -> bool {
        match req {
            Req::Save { kind, etag, .. } => v.kind == *kind && v.etag == Some(*etag),
            Req::Move { target } => v.kind == VersionKind::Move && v.folder == *target,
            Req::Delete => v.kind == VersionKind::Delete,
        }
    }

    fn is_applied(v: &VersionMeta, cur: Option<&CurFile>) -> bool {
        match v.kind {
            VersionKind::Delete => cur.is_none(),
            VersionKind::Move => cur.is_some_and(|c| c.folder == v.folder),
            _ => cur.is_some_and(|c| Some(c.etag) == v.etag),
        }
    }

    /// The state before version `v` (5.5, rule 14).
    fn state_before(
        &self,
        v: &VersionMeta,
        cur: Option<&CurFile>,
        e_if: Option<&Etag>,
        creating: bool,
    ) -> Result<bool, StoreError> {
        if creating {
            return Ok(cur.is_none());
        }
        let Some(c) = cur else { return Ok(false) };
        let prev = match self.hist_dir(&v.uid)? {
            Some(d) if v.version > 1 => self.read_meta(&d, &v.uid, v.version - 1)?,
            _ => None,
        };
        let etag_ok = prev.as_ref().and_then(|p| p.etag) == Some(c.etag) || e_if == Some(&c.etag);
        let folder_ok = v.kind != VersionKind::Move || prev.as_ref().is_none_or(|p| p.folder == c.folder);
        Ok(etag_ok && folder_ok)
    }

    /// 5.14 step 5. `Ok(Some)` is an answer: replayed, or rolled forward. `Ok(None)` means that
    /// the request is new. An error is `SaveIdReused` or `Conflict`.
    fn sid_check(
        &self,
        uid: &Uid,
        sid: SaveId,
        req: &Req,
        cur: Option<&CurFile>,
        e_if: Option<&Etag>,
    ) -> Result<Option<Saved>, StoreError> {
        let Some(v) = self.find_by_save_id(uid, sid)? else { return Ok(None) };
        if !Self::same_request(&v, req) {
            return Err(StoreError::SaveIdReused);
        }
        let head = self.head(uid)?;
        let is_head = head.meta.as_ref().is_some_and(|h| h.version == v.version);
        let replay = || saved(uid, &v.folder, v.version, v.etag, Outcome::Replayed);
        if !is_head || Self::is_applied(&v, cur) {
            return Ok(Some(replay()));
        }
        let creating = matches!(req, Req::Save { creating: true, .. });
        if !self.state_before(&v, cur, e_if, creating)? {
            self.demote(&v);
            return match cur {
                Some(c) => Err(self.conflict_of(c, &head)),
                None => Err(StoreError::NotFound),
            };
        }
        self.roll_forward(&v, cur, creating)?;
        self.note_own_write(uid, v.etag);
        self.apply_retention(uid);
        Ok(Some(saved(uid, &v.folder, v.version, v.etag, Outcome::Written)))
    }

    /// Applies the effect of a head version that is not applied (5.14, step 5).
    fn roll_forward(&self, v: &VersionMeta, cur: Option<&CurFile>, creating: bool) -> Result<(), StoreError> {
        match v.kind {
            VersionKind::Delete => match cur {
                Some(c) => self.apply_delete_effect(&c.folder, &v.uid),
                None => Ok(()),
            },
            VersionKind::Move => match cur {
                Some(c) => self.apply_move_effect(&c.folder, &v.folder, &v.uid, v),
                None => Err(StoreError::NotFound),
            },
            _ => {
                let dir = self.hist_dir(&v.uid)?.ok_or(StoreError::VersionNotFound)?;
                let (bytes, _) = dir
                    .read_file(&json_name(v.version), hune_core::limits::MAX_DOCUMENT_BYTES)
                    .map_err(|e| io_err(e, format!("read the bytes of version {}", v.version)))?;
                let expect = match (creating, cur) {
                    (false, Some(c)) => Expect::Etag { etag: c.etag, mode: c.stat.mode },
                    _ => Expect::Absent,
                };
                self.apply_file_effect(&v.folder, &v.uid, &bytes, expect, v)
            }
        }
    }

    // Steps 3 to 6 of 5.14 for the writes on a dashboard that exists.

    fn prologue(
        &self,
        uid: &Uid,
        req: &Req,
        save_id: Option<SaveId>,
        e_if: Option<&Etag>,
        expected: Option<&FolderId>,
    ) -> Result<Pre, StoreError> {
        let located = self.locate(uid)?;
        let cur = match located.as_slice() {
            [] => None,
            [f] => self.read_dashboard(f, uid)?,
            _ => return Err(not_unique(uid)),
        };
        if let Some(c) = &cur
            && let Some((FolderState::Invalid { errors, .. }, _)) = self.read_folder_state(&c.folder)?
        {
            let first = errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(StoreError::Invalid(vec![ValidationError::new(
                "",
                ErrorCode::NotPermitted,
                format!("the folder {} has no valid folder file: {first}", c.folder),
            )]));
        }
        if let Some(sid) = save_id
            && let Some(s) = self.sid_check(uid, sid, req, cur.as_ref(), e_if)?
        {
            return Ok(Pre::Done(s));
        }
        let Some(cur) = cur else { return Err(StoreError::NotFound) };
        let head = self.head(uid)?;
        if e_if != Some(&cur.etag) || expected.is_some_and(|f| *f != cur.folder) {
            return Err(self.conflict_of(&cur, &head));
        }
        Ok(Pre::Go { cur: Box::new(cur), head })
    }

    // The operations.

    pub(crate) fn save_sync(
        &self,
        doc: &NewDoc,
        if_match: &Etag,
        expected: &FolderId,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.save_core(doc, Some(if_match), Some(expected), kind, meta)
    }

    fn save_core(
        &self,
        doc: &NewDoc,
        e_if: Option<&Etag>,
        expected: Option<&FolderId>,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let uid = &doc.uid;
        let req = Req::Save { kind, etag: doc.etag, creating: false };
        let (cur, head) = match self.prologue(uid, &req, meta.save_id, e_if, expected)? {
            Pre::Done(s) => return Ok(s),
            Pre::Go { cur, head } => (cur, head),
        };
        // Step 7.
        if doc.etag == cur.etag {
            let (version, _, _) = Self::current_fields(&cur.etag, cur.stat.mtime, head.meta.as_ref());
            return Ok(saved(uid, &cur.folder, version, Some(cur.etag), Outcome::Unchanged));
        }
        // Steps 8 to 10.
        let n = self.record_external(uid, &cur, &head)?;
        let vm = new_version(uid, n, kind, &cur.folder, Some(doc.etag), Some(doc.bytes.len() as u64), meta);
        self.write_version(&vm, Some(&doc.bytes))?;
        // Steps 11 to 13.
        let expect = Expect::Etag { etag: cur.etag, mode: cur.stat.mode };
        self.apply_file_effect(&cur.folder, uid, &doc.bytes, expect, &vm)?;
        self.note_own_write(uid, Some(doc.etag));
        self.apply_retention(uid);
        Ok(saved(uid, &cur.folder, n, Some(doc.etag), Outcome::Written))
    }

    pub(crate) fn create_sync(
        &self,
        folder: &FolderId,
        doc: &NewDoc,
        kind: VersionKind,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.create_core(folder, doc, kind, meta, false)
    }

    /// 5.15 `create`. With `restoring` the versions of the uid stay and the number is the head plus 1.
    fn create_core(
        &self,
        folder: &FolderId,
        doc: &NewDoc,
        kind: VersionKind,
        meta: &WriteMeta,
        restoring: bool,
    ) -> Result<Saved, StoreError> {
        let uid = &doc.uid;
        self.require_valid_folder(folder)?;
        let req = Req::Save { kind, etag: doc.etag, creating: true };
        let located = self.locate(uid)?;
        if let Some(sid) = meta.save_id {
            let cur = match located.as_slice() {
                [] => None,
                [f] => self.read_dashboard(f, uid)?,
                _ => None,
            };
            if let Some(s) = self.sid_check(uid, sid, &req, cur.as_ref(), None)? {
                return Ok(s);
            }
        }
        if let Some(first) = located.first() {
            return Err(StoreError::Exists { folder: Some(first.clone()) });
        }
        let head = self.head(uid)?;
        if !restoring && head.max_n > 0 {
            return Err(StoreError::Exists { folder: head.meta.map(|m| m.folder) });
        }
        if self.count_dashboards()? >= MAX_DASHBOARDS {
            return Err(StoreError::Full);
        }
        let n = head.max_n + 1;
        let vm = new_version(uid, n, kind, folder, Some(doc.etag), Some(doc.bytes.len() as u64), meta);
        self.write_version(&vm, Some(&doc.bytes))?;
        self.apply_file_effect(folder, uid, &doc.bytes, Expect::Absent, &vm)?;
        self.note_own_write(uid, Some(doc.etag));
        self.apply_retention(uid);
        Ok(saved(uid, folder, n, Some(doc.etag), Outcome::Written))
    }

    pub(crate) fn move_sync(
        &self,
        uid: &Uid,
        target: &FolderId,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        self.require_valid_folder(target)?;
        let req = Req::Move { target: target.clone() };
        let (cur, head) = match self.prologue(uid, &req, meta.save_id, Some(if_match), Some(expected))? {
            Pre::Done(s) => return Ok(s),
            Pre::Go { cur, head } => (cur, head),
        };
        let doc = match &cur.doc {
            Ok(d) => d,
            Err(errors) => return Err(StoreError::Invalid(errors.clone())),
        };
        if cur.folder == *target {
            let (version, _, _) = Self::current_fields(&cur.etag, cur.stat.mtime, head.meta.as_ref());
            return Ok(saved(uid, target, version, Some(cur.etag), Outcome::Unchanged));
        }
        let Some(tdir) = self.open_folder(target)? else { return Err(StoreError::FolderNotFound) };
        match tdir.exists(&format!("{uid}.json")) {
            Ok(false) => {}
            Ok(true) => return Err(StoreError::Exists { folder: Some(target.clone()) }),
            Err(e) => return Err(io_err(e, format!("stat {target}/{uid}.json"))),
        }
        let n = self.record_external(uid, &cur, &head)?;
        let bytes = doc.bytes();
        let vm = new_version(uid, n, VersionKind::Move, target, Some(cur.etag), Some(bytes.len() as u64), meta);
        self.write_version(&vm, Some(bytes))?;
        self.apply_move_effect(&cur.folder, target, uid, &vm)?;
        self.note_own_write(uid, Some(cur.etag));
        self.apply_retention(uid);
        Ok(saved(uid, target, n, Some(cur.etag), Outcome::Written))
    }

    pub(crate) fn delete_sync(
        &self,
        uid: &Uid,
        if_match: &Etag,
        expected: &FolderId,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let (cur, head) = match self.prologue(uid, &Req::Delete, meta.save_id, Some(if_match), Some(expected))? {
            Pre::Done(s) => return Ok(s),
            Pre::Go { cur, head } => (cur, head),
        };
        let n = self.record_external(uid, &cur, &head)?;
        let vm = new_version(uid, n, VersionKind::Delete, &cur.folder, None, None, meta);
        self.write_version(&vm, None)?;
        self.apply_delete_effect(&cur.folder, uid)?;
        self.note_own_write(uid, None);
        self.apply_retention(uid);
        Ok(saved(uid, &cur.folder, n, None, Outcome::Written))
    }

    /// 5.14 `restore` with the document of version `version`, which the caller read and validated off the lock
    /// (`ValidVersion`). Under the lock, it reads only the metadata of the version: the retention can remove a version
    /// between the read of the caller and the lock. The etag of the metadata must be the etag of the document, because
    /// a version holds canonical bytes (5.12).
    pub(crate) fn restore_sync(
        &self,
        new: &NewDoc,
        version: u64,
        if_match: Option<&Etag>,
        expected: Option<&FolderId>,
        meta: &WriteMeta,
    ) -> Result<Saved, StoreError> {
        let uid = &new.uid;
        let folder = self.restorable_folder(uid, version, &new.etag)?;
        if self.locate(uid)?.is_empty() {
            if if_match.is_some() {
                return Err(StoreError::NotFound);
            }
            self.create_core(&folder, new, VersionKind::Restore, meta, true)
        } else {
            self.save_core(new, if_match, expected, VersionKind::Restore, meta)
        }
    }

    /// The folder of a version that has bytes (5.14, `restore` step 1), from its metadata file only. A version that
    /// does not exist gives `VersionNotFound`. A tombstone, an `external` version without bytes and a version whose
    /// etag is not the etag of the valid document give `NotRestorable`.
    fn restorable_folder(&self, uid: &Uid, version: u64, etag: &Etag) -> Result<FolderId, StoreError> {
        if version == 0 || version > i64::MAX as u64 {
            return Err(StoreError::VersionNotFound);
        }
        let Some(dir) = self.hist_dir(uid)? else { return Err(StoreError::VersionNotFound) };
        let Some(m) = self.read_meta(&dir, uid, version)? else { return Err(StoreError::VersionNotFound) };
        if m.etag != Some(*etag) {
            return Err(StoreError::NotRestorable);
        }
        match dir.exists(&json_name(version)) {
            Ok(true) => Ok(m.folder),
            Ok(false) => Err(StoreError::NotRestorable),
            Err(e) => Err(io_err(e, format!("stat .history/{uid}/{}", json_name(version)))),
        }
    }

    // The folder operations (5.15).

    pub(crate) fn create_folder_sync(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        if self.folder_ids()?.len() >= MAX_FOLDERS {
            return Err(StoreError::Full);
        }
        match self.root.mkdir(id.as_str()) {
            Ok(()) => {}
            Err(Errno::EXIST) => return Err(StoreError::Exists { folder: None }),
            Err(e) => return Err(io_err(e, format!("make the folder {id}"))),
        }
        let written = self
            .root
            .open_dir(id.as_str())
            .and_then(|d| d.write_atomic(FOLDER_FILE, &folder_file_bytes(title.as_str()), None).map(|()| d));
        match written {
            Ok(_) => self.root.fsync().map_err(|e| io_err(e, "fsync the root")),
            Err(e) => {
                if let Ok(d) = self.root.open_dir(id.as_str()) {
                    for i in d.list().unwrap_or_default() {
                        d.unlink_quiet(&i.name);
                    }
                }
                let _ = self.root.rmdir(id.as_str());
                Err(io_err(e, format!("write {id}/{FOLDER_FILE}")))
            }
        }
    }

    pub(crate) fn rename_folder_sync(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError> {
        let Some((FolderState::Valid(rec), _)) = self.read_folder_state(id)? else {
            return Err(StoreError::FolderNotFound);
        };
        if rec.title == *title {
            return Ok(());
        }
        let Some(dir) = self.open_folder(id)? else { return Err(StoreError::FolderNotFound) };
        let mode = dir.stat(FOLDER_FILE).ok().map(|s| s.mode);
        dir.write_atomic(FOLDER_FILE, &folder_file_bytes(title.as_str()), mode)
            .map_err(|e| io_err(e, format!("write {id}/{FOLDER_FILE}")))
    }

    pub(crate) fn delete_folder_sync(&self, id: &FolderId) -> Result<(), StoreError> {
        let Some(dir) = self.open_folder(id)? else { return Err(StoreError::FolderNotFound) };
        let mut items = dir.list().map_err(|e| io_err(e, format!("list the folder {id}")))?;
        items.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(first) = items.iter().find(|i| i.name != FOLDER_FILE) {
            self.push_event(crate::store::StoreEvent::FolderNotEmpty { folder: id.clone(), entry: first.name.clone() });
            return Err(StoreError::FolderNotEmpty);
        }
        match dir.unlink(FOLDER_FILE) {
            Ok(()) | Err(Errno::NOENT) => {}
            Err(e) => return Err(io_err(e, format!("remove {id}/{FOLDER_FILE}"))),
        }
        drop(dir);
        self.root.rmdir(id.as_str()).map_err(|e| io_err(e, format!("remove the folder {id}")))?;
        self.root.fsync().map_err(|e| io_err(e, "fsync the root"))
    }
}

fn new_version(
    uid: &Uid,
    n: u64,
    kind: VersionKind,
    folder: &FolderId,
    etag: Option<Etag>,
    size: Option<u64>,
    meta: &WriteMeta,
) -> VersionMeta {
    VersionMeta {
        uid: uid.clone(),
        version: n,
        kind,
        folder: folder.clone(),
        etag,
        saved_at: UtcMillis::from_millis(meta.saved_at.as_millis()),
        saved_by: meta.saved_by.clone(),
        message: meta.message.clone(),
        save_id: meta.save_id,
        size,
    }
}
