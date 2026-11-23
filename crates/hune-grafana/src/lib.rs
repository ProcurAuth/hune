// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hune importer (section 15): Grafana v1 and v2 dashboard JSON to format 1, with the loss
//! report. Pure functions: no I/O.
//!
//! The crate reads values, not files. The command `hune import-grafana` reads the files,
//! checks the JSON rules of 15.4, calls these functions, and writes the documents and the
//! report.
//!
//! - [`import_dashboard`] converts one dashboard.
//! - [`import_all`] converts the input directory and gives the report data, the exit code and
//!   the redirects.
//! - [`render_report`] and [`render_nginx_map`] write the report (15.11) and the nginx map (15.17).
//! - [`parse_acceptance_file`] and [`parse_folder_map`] read the two option files.
#![forbid(unsafe_code)]

mod annotations;
mod colour;
mod dashboard;
mod fields;
mod item;
mod mongoprep;
mod options;
mod out;
mod panel;
mod panelopts;
mod query;
mod repeat;
mod sqlprep;
mod text;
mod transforms;
mod transforms_more;
mod util;
mod variables;
mod walk;

pub use dashboard::{DashboardImport, DashboardResult, Form, import_dashboard};
pub use item::{Class, Fallback, ReportItem, SourceUse};
pub use options::{
    Acceptance, FolderInfo, FolderMapEntry, FolderResolution, FolderTable, GrafanaFolder, ImportError, ImportOptions,
    LinkPrefix, MongoLists, SourceKind, assign_uids, parse_acceptance_file, parse_folder_map, uid_rule,
};
mod all;
mod render;
pub use all::{
    DashboardFile, FolderReport, FolderResult, ImportInput, ImportResult, InputFileInfo, NotImported, Redirect, Totals,
    import_all,
};
pub use render::{RenderedReport, ReportMeta, render_nginx_map, render_report};
