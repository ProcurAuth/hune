//! `hune copy-store`, `hune export` and `hune import` (5.37 to 5.39).
//!
//! The functions read through the `Store` trait and write through the importer of the target.
//! The volume layout writer is in `volume/` (R4).

use std::fmt;
use std::path::Path;

use crate::store::{AnyStore, DashboardRecord, ImportReport, Importer, Problem, Store};
use crate::types::{FolderRecord, StoreError};
use crate::volume::{DirTarget, VolumeOptions, VolumeStore};

/// The error of a copy, an export or an import.
#[derive(Debug, Clone)]
pub enum TransferError {
    /// A store error.
    Store(StoreError),
    /// The source holds invalid folders, invalid documents or invalid metadata files.
    /// Nothing is written (5.37 item 4, 5.38 item 4, 5.39 item 3).
    InvalidSource(Vec<Problem>),
    /// The verification found a difference (5.37, item 8).
    Verify(String),
    /// An error after the first write: the target can hold a partial copy.
    Partial(Box<TransferError>),
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferError::Store(e) => write!(f, "{e}"),
            TransferError::InvalidSource(list) => {
                write!(f, "the source has {} problem(s):", list.len())?;
                for p in list {
                    write!(f, "\n{p}")?;
                }
                Ok(())
            }
            TransferError::Verify(m) => write!(f, "verification failed: {m}"),
            TransferError::Partial(e) => {
                write!(f, "{e}; the target can hold a partial copy: remove the target before the next attempt")
            }
        }
    }
}

impl std::error::Error for TransferError {}

impl From<StoreError> for TransferError {
    fn from(e: StoreError) -> Self {
        TransferError::Store(e)
    }
}

enum Sink {
    Store(Importer),
    Dir(DirTarget),
}

impl Sink {
    async fn folder(&mut self, rec: &FolderRecord) -> Result<(), StoreError> {
        match self {
            Sink::Store(i) => i.folder(rec).await,
            Sink::Dir(d) => d.folder(rec).await,
        }
    }
    async fn dashboard(&mut self, rec: &DashboardRecord) -> Result<(), StoreError> {
        match self {
            Sink::Store(i) => i.dashboard(rec).await,
            Sink::Dir(d) => d.dashboard(rec).await,
        }
    }
    async fn finish(self) -> Result<ImportReport, StoreError> {
        match self {
            Sink::Store(i) => i.commit().await,
            Sink::Dir(d) => d.finish().await,
        }
    }
}

async fn check_source(from: &AnyStore) -> Result<(), TransferError> {
    let problems = from.check_consistency().await?;
    if problems.is_empty() { Ok(()) } else { Err(TransferError::InvalidSource(problems)) }
}

/// Reads the folders, then each uid with its versions from the lowest number to the highest, and writes them.
async fn pump(from: &AnyStore, sink: &mut Sink) -> Result<(), StoreError> {
    let exporter = from.export_begin().await?;
    for f in exporter.folders().await? {
        sink.folder(&f).await?;
    }
    for uid in exporter.uids().await? {
        let rec = exporter.dashboard(&uid).await?;
        sink.dashboard(&rec).await?;
    }
    exporter.end().await
}

async fn run(from: &AnyStore, mut sink: Sink) -> Result<ImportReport, TransferError> {
    pump(from, &mut sink).await.map_err(|e| TransferError::Partial(Box::new(e.into())))?;
    sink.finish().await.map_err(|e| TransferError::Partial(Box::new(e.into())))
}

/// Compares two stores: each folder, each version (metadata and bytes) and each current
/// document (5.37, item 8).
pub async fn verify_equal(a: &AnyStore, b: &AnyStore) -> Result<(), TransferError> {
    let (ea, eb) = (a.export_begin().await?, b.export_begin().await?);
    let (fa, fb) = (ea.folders().await?, eb.folders().await?);
    if fa != fb {
        return Err(TransferError::Verify("the folders differ".to_owned()));
    }
    let (ua, ub) = (ea.uids().await?, eb.uids().await?);
    if ua != ub {
        return Err(TransferError::Verify("the lists of uids differ".to_owned()));
    }
    for uid in &ua {
        if ea.dashboard(uid).await? != eb.dashboard(uid).await? {
            return Err(TransferError::Verify(format!("the versions or the current document of {uid} differ")));
        }
    }
    ea.end().await?;
    eb.end().await?;
    Ok(())
}

/// Copies a store into an empty store, then verifies the copy (5.37, steps 4 to 8).
/// A volume source MUST be open with `read_only = true`, so it takes no lock (5.37, item 2).
pub async fn copy_store(from: &AnyStore, to: &AnyStore) -> Result<ImportReport, TransferError> {
    check_source(from).await?;
    let importer = to.import_begin().await?;
    let report = run(from, Sink::Store(importer)).await?;
    verify_equal(from, to).await.map_err(|e| TransferError::Partial(Box::new(e)))?;
    Ok(report)
}

/// Writes the whole store as a volume layout into a new or empty directory (5.38).
pub async fn export_to_dir(from: &AnyStore, dir: &Path) -> Result<ImportReport, TransferError> {
    check_source(from).await?;
    let target = DirTarget::create(dir).await?;
    run(from, Sink::Dir(target)).await
}

/// Reads a volume layout into the empty store `to`, then verifies it (5.39).
/// The directory is opened as a read-only volume: no lock, no write, no link followed.
pub async fn import_from_dir(dir: &Path, to: &AnyStore) -> Result<ImportReport, TransferError> {
    let opts = VolumeOptions {
        root: dir.to_owned(),
        read_only: true,
        rescan_interval: std::time::Duration::from_secs(5),
        history_keep: 100,
    };
    let source: AnyStore = VolumeStore::open(opts).await.map_err(StoreError::from)?.into();
    check_source(&source).await?;
    let importer = to.import_begin().await?;
    let report = run(&source, Sink::Store(importer)).await?;
    verify_equal(&source, to).await.map_err(|e| TransferError::Partial(Box::new(e)))?;
    Ok(report)
}
