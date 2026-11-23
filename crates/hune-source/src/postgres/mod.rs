// SPDX-License-Identifier: MIT OR Apache-2.0
//! The PostgreSQL data source (section 8a). It reads data for panel queries and never writes.
//!
//! The dialect (7.19 to 7.26) lives in `hune_core::sql`. The frame types (7.16a, 7.16b) are
//! decoded here (`decode`, `frame_build`). The crate has no new dependency: `tokio-postgres` and
//! `tokio-postgres-rustls` are the crates of the store (8a.2).
//!
//! # Modules and the sections that each one implements
//!
//! | Module | Sections | Job |
//! |---|---|---|
//! | `source` | 8a.2, 8a.10 | The type [`PostgresSource`], its state and its admission. |
//! | `startup` | 8a.4 | The startup packet and the TLS setup. |
//! | `session` | 8a.4, 8a.5 | One session and its fixed statements. |
//! | `pool` | 8a.9 | The small pool of sessions. |
//! | `exec` | 8a.5 to 8a.7 | The run of one query. |
//! | `cancel` | 8a.7 | The CancelRequest. |
//! | `bounded` | 8a.6, 8a.15 | The limit on the length of each message of the server. |
//! | `decode` | 7.16a | The binary decode of a value. |
//! | `frame_build` | 7.16, 7.16a, 8a.6 | The frame and the caps. |
//! | `attrib` | 8a.11 | The comment and `application_name`. |
//! | `fixed` | 8a.3, 8a.5 | The only file that holds the fixed statements and the simple protocol. |
//! | `errors` | 8a.8 | The map from an error to a `SourceError`. |
//! | `probe` | 8a.12 | The probe and the state. |
//! | `probe_sql` | 8a.12, 8a.13 | The statements of the probe. |
//!
//! # Status
//!
//! The client is complete: `session`, `pool`, `exec`, `cancel` and `source`
//! run the steps of 8a.4 to 8a.10 and 8a.12. The container tests are in
//! `tests/pgsource_client.rs` and in `tests/pgsource_down.rs` (16.9a: PS04 to PS07, PS09, PS12 to PS15).

pub(crate) mod attrib;
pub(crate) mod bounded;
pub(crate) mod cancel;
pub(crate) mod decode;
pub(crate) mod errors;
pub(crate) mod exec;
pub(crate) mod fixed;
pub(crate) mod frame_build;
pub(crate) mod pool;
pub(crate) mod probe;
pub(crate) mod probe_sql;
pub(crate) mod session;
mod source;
pub(crate) mod startup;

pub use frame_build::apply_time_series_rule;
pub use source::{AdviceChange, PostgresSource};
