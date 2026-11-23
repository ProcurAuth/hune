// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hune store. It holds the `Store` trait (5.3), the volume backend (5.9 to 5.22) and the
//! catalog (5.8). It also holds the PostgreSQL backend (5.23 to 5.32), `copy_store` (5.37) and
//! the conformance suite (5.33 to 5.36, feature `conformance`).
//!
//! The enum [`AnyStore`] dispatches the calls (rule R8 of 3.4): no `dyn` trait object.
#![forbid(unsafe_code)]

pub mod catalog;
pub mod copy;
pub mod postgres;
pub mod store;
pub mod types;
pub mod volume;

#[cfg(feature = "conformance")]
pub mod conformance;

pub use catalog::{Catalog, CatalogStats, PollResult, PollSummary, SyncTransition};
pub use copy::{TransferError, copy_store, export_to_dir, import_from_dir, verify_equal};
pub use postgres::{
    MigrateError, MigrateReport, PgOpenError, PgOpenErrorKind, PgOptions, PgSslMode, PgStore, SCHEMA_VERSION,
    migrate_store,
};
pub use store::{
    AnyStore, CurrentRecord, DashboardRecord, Delta, EntryKey, EntryMark, EntryProbe, Exporter, FileSig, FolderMark,
    FolderState, ImportReport, Importer, Marks, Problem, Snapshot, Store, StoreEvent,
};
pub use types::*;
pub use volume::{OpenError, OpenErrorKind, VolumeOptions, VolumeStore};
