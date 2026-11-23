// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hooks for the conformance suite (5.33, item 3): changes of the volume outside the trait.
//! This module is compiled only for the tests and with the feature `conformance`.

use std::path::{Path, PathBuf};

use super::faults::{FaultPoint, Faults};
use super::meta::{encode_meta, json_name, meta_name};
use super::{FOLDER_FILE, HISTORY_DIR};
use crate::types::{FolderId, Uid, VersionMeta};
use hune_core::document::folder_file_bytes;

/// The hooks of one volume root.
#[derive(Debug, Clone)]
pub struct VolumeHooks {
    root: PathBuf,
    faults: Faults,
}

impl VolumeHooks {
    /// Makes the hooks of a root and of the faults of its store.
    pub fn new(root: &Path, faults: Faults) -> Self {
        VolumeHooks { root: root.to_owned(), faults }
    }

    /// The root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The fault handle.
    pub fn faults(&self) -> &Faults {
        &self.faults
    }

    /// Replaces `<folder>/<uid>.json` with a new file by a rename, as `git pull` does.
    pub fn outside_write(&self, folder: &FolderId, uid: &Uid, bytes: &[u8]) -> std::io::Result<()> {
        let dir = self.root.join(folder.as_str());
        let tmp = dir.join(format!(".{uid}.outside"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dir.join(format!("{uid}.json")))
    }

    /// Writes a file in place, without a rename (a truncated file during a write, SV-13).
    pub fn outside_write_in_place(&self, folder: &FolderId, uid: &Uid, bytes: &[u8]) -> std::io::Result<()> {
        std::fs::write(self.root.join(folder.as_str()).join(format!("{uid}.json")), bytes)
    }

    /// Removes `<folder>/<uid>.json`.
    pub fn outside_remove(&self, folder: &FolderId, uid: &Uid) -> std::io::Result<()> {
        std::fs::remove_file(self.root.join(folder.as_str()).join(format!("{uid}.json")))
    }

    /// Writes one history version, with its bytes when it has some.
    pub fn inject_version(&self, meta: &VersionMeta, bytes: Option<&[u8]>) -> std::io::Result<()> {
        let dir = self.root.join(HISTORY_DIR).join(meta.uid.as_str());
        std::fs::create_dir_all(&dir)?;
        if let Some(b) = bytes {
            std::fs::write(dir.join(json_name(meta.version)), b)?;
        }
        std::fs::write(dir.join(meta_name(meta.version)), encode_meta(meta))
    }

    /// Makes `n` folders named `fill-<number>`.
    pub fn fill_folders(&self, n: usize) -> std::io::Result<()> {
        for i in 0..n {
            let dir = self.root.join(format!("fill-{i}"));
            std::fs::create_dir(&dir)?;
            std::fs::write(dir.join(FOLDER_FILE), folder_file_bytes("Fill"))?;
        }
        Ok(())
    }

    /// Writes the dashboard files of a list of documents into a folder.
    pub fn fill_dashboards(
        &self,
        folder: &FolderId,
        docs: impl Iterator<Item = (Uid, Vec<u8>)>,
    ) -> std::io::Result<()> {
        let dir = self.root.join(folder.as_str());
        for (uid, bytes) in docs {
            std::fs::write(dir.join(format!("{uid}.json")), bytes)?;
        }
        Ok(())
    }

    /// Makes the next write fail after its version is durable. The effect is not visible. This
    /// is the state after a crash (SC-48, SV-16, SV-25, SV-28).
    pub fn break_next_write(&self) {
        self.faults.inject(FaultPoint::HistoryWritten, super::faults::FaultAction::Errno(5));
    }
}
