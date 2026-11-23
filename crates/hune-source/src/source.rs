// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `DataSource` trait (8.1) and the enum `AnySource` (R8: enum dispatch, no `dyn`).

use std::future::Future;

use hune_core::frame::Frame;
use hune_core::ids::SourceId;

use crate::clickhouse::ClickHouseSource;
use crate::error::SourceError;
use crate::mongo::MongoSource;
use crate::postgres::{AdviceChange, PostgresSource};
use crate::types::{EffectiveLimits, PreparedQuery, Probe, QueryCtx, SourceKind, SourceState};

/// One data source of the configuration.
///
/// The trait has no method that writes data.
pub trait DataSource: Send + Sync {
    /// The id of the block, for example `prod` for `clickhouse "prod" { ... }`.
    fn id(&self) -> &SourceId;
    /// The kind of the source.
    fn kind(&self) -> SourceKind;
    /// The state of 8.11.
    fn state(&self) -> SourceState;
    /// Runs one prepared query within the limits, and gives one frame (7.16).
    fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> impl Future<Output = Result<Frame, SourceError>> + Send;
    /// Runs the probe of 8.11.
    fn probe(&self) -> impl Future<Output = Probe> + Send;
}

impl DataSource for ClickHouseSource {
    fn id(&self) -> &SourceId {
        ClickHouseSource::id(self)
    }

    fn kind(&self) -> SourceKind {
        ClickHouseSource::kind(self)
    }

    fn state(&self) -> SourceState {
        ClickHouseSource::state(self)
    }

    fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> impl Future<Output = Result<Frame, SourceError>> + Send {
        ClickHouseSource::query(self, query, limits, ctx)
    }

    fn probe(&self) -> impl Future<Output = Probe> + Send {
        ClickHouseSource::probe(self)
    }
}

/// Enum dispatch: no `dyn`. A new kind adds a variant (8.14).
#[derive(Debug, Clone)]
pub enum AnySource {
    /// A ClickHouse server.
    ClickHouse(ClickHouseSource),
    /// A PostgreSQL server (8a).
    Postgres(PostgresSource),
    /// A MongoDB server or replica set (8b).
    Mongo(MongoSource),
}

impl AnySource {
    /// Gives the id of the block.
    pub fn id(&self) -> &SourceId {
        match self {
            AnySource::ClickHouse(s) => s.id(),
            AnySource::Postgres(s) => s.id(),
            AnySource::Mongo(s) => s.id(),
        }
    }

    /// Gives the label of the block.
    pub fn label(&self) -> &str {
        match self {
            AnySource::ClickHouse(s) => s.label(),
            AnySource::Postgres(s) => s.label(),
            AnySource::Mongo(s) => s.label(),
        }
    }

    /// Gives the kind of the source.
    pub fn kind(&self) -> SourceKind {
        match self {
            AnySource::ClickHouse(s) => s.kind(),
            AnySource::Postgres(s) => s.kind(),
            AnySource::Mongo(s) => s.kind(),
        }
    }

    /// Gives the state of 8.11.
    pub fn state(&self) -> SourceState {
        match self {
            AnySource::ClickHouse(s) => s.state(),
            AnySource::Postgres(s) => s.state(),
            AnySource::Mongo(s) => s.state(),
        }
    }

    /// Gives a receiver that sees each change of the state.
    pub fn watch_state(&self) -> tokio::sync::watch::Receiver<SourceState> {
        match self {
            AnySource::ClickHouse(s) => s.watch_state(),
            AnySource::Postgres(s) => s.watch_state(),
            AnySource::Mongo(s) => s.watch_state(),
        }
    }

    /// Gives the result of the last probe, or `None` before the first one.
    pub fn last_probe(&self) -> Option<Probe> {
        match self {
            AnySource::ClickHouse(s) => s.last_probe(),
            AnySource::Postgres(s) => s.last_probe(),
            AnySource::Mongo(s) => s.last_probe(),
        }
    }

    /// Gives the counters of the admission.
    pub fn admission_stats(&self) -> crate::admission::AdmissionStats {
        match self {
            AnySource::ClickHouse(s) => s.admission_stats(),
            AnySource::Postgres(s) => s.admission_stats(),
            AnySource::Mongo(s) => s.admission_stats(),
        }
    }

    /// Gives the advice changes of the probes since the last call (8a.12). The server writes one
    /// `source.advice` line for each. A ClickHouse source has none.
    pub fn take_advice_changes(&self) -> Vec<AdviceChange> {
        match self {
            AnySource::ClickHouse(_) => Vec::new(),
            AnySource::Postgres(s) => s.take_advice_changes(),
            AnySource::Mongo(s) => s.take_advice_changes(),
        }
    }

    /// Gives the versions below the oldest supported release that the probes found since the last
    /// call (8a.14). The server writes one `source.version_unsupported` line for each. A ClickHouse
    /// source has none: its probe gives `Probe::version_supported`.
    pub fn take_version_unsupported(&self) -> Vec<String> {
        match self {
            AnySource::ClickHouse(_) => Vec::new(),
            AnySource::Postgres(s) => s.take_version_unsupported(),
            AnySource::Mongo(s) => s.take_version_unsupported(),
        }
    }

    /// Runs one prepared query. A `PreparedQuery` of another kind than the source gives
    /// `source_bad_response`, but v1 has one kind only.
    pub async fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> Result<Frame, SourceError> {
        match self {
            AnySource::ClickHouse(s) => s.query(query, limits, ctx).await,
            AnySource::Postgres(s) => s.query(query, limits, ctx).await,
            AnySource::Mongo(s) => s.query(query, limits, ctx).await,
        }
    }

    /// Runs the probe of 8.11.
    pub async fn probe(&self) -> Probe {
        match self {
            AnySource::ClickHouse(s) => s.probe().await,
            AnySource::Postgres(s) => s.probe().await,
            AnySource::Mongo(s) => s.probe().await,
        }
    }
}

impl DataSource for AnySource {
    fn id(&self) -> &SourceId {
        AnySource::id(self)
    }

    fn kind(&self) -> SourceKind {
        AnySource::kind(self)
    }

    fn state(&self) -> SourceState {
        AnySource::state(self)
    }

    fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> impl Future<Output = Result<Frame, SourceError>> + Send {
        AnySource::query(self, query, limits, ctx)
    }

    fn probe(&self) -> impl Future<Output = Probe> + Send {
        AnySource::probe(self)
    }
}

impl From<ClickHouseSource> for AnySource {
    fn from(s: ClickHouseSource) -> AnySource {
        AnySource::ClickHouse(s)
    }
}

impl From<MongoSource> for AnySource {
    fn from(s: MongoSource) -> AnySource {
        AnySource::Mongo(s)
    }
}

impl From<PostgresSource> for AnySource {
    fn from(s: PostgresSource) -> AnySource {
        AnySource::Postgres(s)
    }
}
