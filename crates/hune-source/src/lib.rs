// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hune data sources (section 8). The crate has the `DataSource` trait and the enum `AnySource`.
//! It has the ClickHouse HTTP client, the limits, the admission control and the cancellation.
//!
//! The frame types (7.16) live in `hune_core::frame`. This crate does not depend on the
//! configuration types of the binary: the binary builds a [`ClickHouseSourceConfig`].
//!
//! The PostgreSQL source (section 8a) lives in the module `postgres`. Each submodule names the
//! sections that it implements. The source writes no log line: it queues the advice changes of the
//! probe ([`AdviceChange`]) for the server (`PostgresSource::take_advice_changes`).
//!
//! The MongoDB source (section 8b) lives in the module `mongo`: the narrow client trait `MongoClient`,
//! the logic of the source generic over it, and a fake client for the tests. The build has no MongoDB
//! driver (8b.2): `MongoSource::new` makes a source that stays `misconfigured`.
//!
//! Cancellation. A caller that drops the future of [`AnySource::query`] closes the connection
//! to ClickHouse at once. ClickHouse then stops the query. The crate never sends `KILL QUERY`.
#![forbid(unsafe_code)]

mod admission;
mod answer;
mod budget;
mod clickhouse;
mod config;
mod conn;
mod error;
mod grants;
mod mongo;
mod postgres;
mod request;
mod source;
mod types;

pub use admission::AdmissionStats;
pub use budget::{MemoryReservation, ResultMemoryBudget};
pub use clickhouse::{ClickHouseSource, OLDEST_SUPPORTED_VERSION, PROBE_INTERVAL, parse_version};
pub use config::{
    ClickHouseSourceConfig, MongoHost, MongoSourceConfig, MongoTlsMode, MongoTopology, PostgresSourceConfig,
    PostgresSslMode, SourceTls,
};
pub use error::{Ceiling, ErrorCtx, LimitName, MESSAGE_MAX_BYTES, Member, SourceError, SourceSetupError, first_line};
pub use mongo::{
    APP_NAME as MONGO_APP_NAME, AuthInfo, AuthRole, AuthUser, Batch, CERTIFICATE_CODE_NAME, ConnectOptions,
    CursorHandle, FakeMongo, MongoClient, MongoConnector, MongoError, MongoErrorClass, MongoSource, NO_DRIVER_MESSAGE,
    Privilege, PrivilegeResource, ReadOp, ReadRequest, Reply as FakeMongoReply, ServerInfo,
};
#[cfg(feature = "mongo-driver")]
pub use mongo::{MongoDriver, decode_document as decode_mongo_document};
pub use postgres::{AdviceChange, PostgresSource, apply_time_series_rule};
pub use source::{AnySource, DataSource};
pub use types::{
    EffectiveLimits, LOG_COMMENT_MAX_BYTES, LOG_COMMENT_SUBJECT_MAX_BYTES, LogComment, LogKind, PreparedQuery, Probe,
    QueryCtx, QueryKind, QueueWait, SourceKind, SourceState, SubjectId, TraceParent, Uuid,
};

/// The version text that the client sends in `User-Agent`.
pub const USER_AGENT: &str = concat!("hune/", env!("CARGO_PKG_VERSION"));
