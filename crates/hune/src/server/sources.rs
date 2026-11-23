// SPDX-License-Identifier: MIT OR Apache-2.0
//! The data sources at start (14.12 steps 4 and 9): the construction from the configuration, the
//! probe tasks and the watchers of the state (8.11).

use super::lifecycle::StartError;
use super::source_blocks::{mongo_blocks, postgres_blocks};
use super::state::{AppState, StopStage};
use crate::config::{ClickHouseConfig, Config};
use hune_core::ids::SourceId;
use hune_source::{
    AnySource, ClickHouseSource, ClickHouseSourceConfig, MongoSource, PostgresSource, ResultMemoryBudget, SourceKind,
    SourceState, SourceTls,
};
use std::sync::Arc;
use std::time::Instant;

/// Maps the block of the configuration to the settings of the source.
pub fn source_config(c: &ClickHouseConfig) -> Result<ClickHouseSourceConfig, StartError> {
    let id = SourceId::parse(&c.id).map_err(|e| StartError::new("sources", format!("clickhouse \"{}\": {e}", c.id)))?;
    Ok(ClickHouseSourceConfig {
        id,
        label: c.label.clone(),
        https: c.url.https,
        host: c.url.host.clone(),
        port: c.url.port,
        user: c.user.clone(),
        password: c.password.expose().to_owned(),
        database: c.database.clone(),
        tls: c.tls.as_ref().map(|t| SourceTls { ca_pem: t.ca_pem.clone(), server_name: t.server_name.clone() }),
        connect_timeout: c.connect_timeout,
        max_execution_time: c.max_execution_time,
        max_result_rows: c.max_result_rows,
        max_result_bytes: c.max_result_bytes,
        max_query_size: c.max_query_size,
        max_concurrent: c.max_concurrent,
        queue_wait: c.queue_wait,
        max_concurrent_per_subject: c.max_concurrent_per_subject,
    })
}

/// Builds one source for each `clickhouse` block, then one for each `postgres` block, then one for
/// each `mongodb` block, with one shared memory budget. It sends nothing on the network (14.12
/// step 4). The order is the order of the file in each kind.
///
/// A build with the cargo feature `mongo-driver` (the default) gives each `mongodb` source the
/// client of the driver module (8b.2). A build without it gives a source with no client, which stays
/// `misconfigured` with the fixed text of the source. The configuration refuses such a block at stage 2
/// before this call, so that case is only a guard. A unit-test build can give a fake client for an id
/// with `test_connectors::put`, and has no driver for another id, so that no unit test opens a
/// connection.
pub fn build_sources(config: &Config) -> Result<Vec<AnySource>, StartError> {
    let budget = ResultMemoryBudget::new(config.server.max_result_memory_bytes);
    let postgres = postgres_blocks(config);
    let mongo = mongo_blocks(config);
    let mut out = Vec::with_capacity(config.clickhouse.len() + postgres.len() + mongo.len());
    for block in &config.clickhouse {
        let cfg = source_config(block)?;
        let source = ClickHouseSource::new(cfg, budget.clone())
            .map_err(|e| StartError::new("sources", format!("clickhouse \"{}\": {e}", block.id)))?;
        out.push(AnySource::ClickHouse(source));
    }
    for cfg in postgres {
        let id = cfg.id.as_str().to_owned();
        let source = PostgresSource::new(cfg.clone(), budget.clone())
            .map_err(|e| StartError::new("sources", format!("postgres \"{id}\": {e}")))?;
        out.push(AnySource::Postgres(source));
    }
    for cfg in mongo {
        let id = cfg.id.as_str().to_owned();
        let source =
            mongo_source(cfg, &budget).map_err(|e| StartError::new("sources", format!("mongodb \"{id}\": {e}")))?;
        out.push(AnySource::Mongo(source));
    }
    Ok(out)
}

/// Builds the source of one `mongodb` block. A build with no driver gives a source with no client.
#[cfg_attr(all(feature = "mongo-driver", not(test)), allow(clippy::unnecessary_wraps))]
fn mongo_source(
    cfg: &hune_source::MongoSourceConfig,
    budget: &ResultMemoryBudget,
) -> Result<MongoSource, hune_source::SourceSetupError> {
    #[cfg(test)]
    if let Some(connector) = test_connectors::take(cfg.id.as_str()) {
        return MongoSource::with_connector(cfg.clone(), budget.clone(), connector);
    }
    #[cfg(all(feature = "mongo-driver", not(test)))]
    {
        MongoSource::with_driver::<hune_source::MongoDriver>(cfg.clone(), budget.clone())
    }
    #[cfg(not(all(feature = "mongo-driver", not(test))))]
    {
        MongoSource::new(cfg.clone(), budget.clone())
    }
}

/// The fake clients of the tests. A test puts the connector of an id before it starts the server.
#[cfg(test)]
pub(crate) mod test_connectors {
    use hune_source::MongoConnector;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static CONNECTORS: Mutex<Option<HashMap<String, MongoConnector>>> = Mutex::new(None);

    /// Keeps the connector for the next source with this id. Use an id that no other test uses.
    pub(crate) fn put(id: &str, connector: MongoConnector) {
        CONNECTORS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id.to_owned(), connector);
    }

    pub(super) fn take(id: &str) -> Option<MongoConnector> {
        CONNECTORS.lock().unwrap().as_mut()?.remove(id)
    }
}

/// Starts, for each source, the task that probes it and the task that watches its state
/// (14.12 step 9). The first probe runs at once and does not delay the readiness.
pub fn spawn_source_tasks(state: &Arc<AppState>) {
    for source in &state.sources {
        spawn_watcher(state.clone(), source.clone());
        if state.timing.probe_sources {
            spawn_prober(state.clone(), source.clone());
        }
    }
}

fn spawn_prober(state: Arc<AppState>, source: AnySource) {
    let mut stop = state.stop_signal();
    tokio::spawn(async move {
        let mut warned_version: Option<String> = None;
        loop {
            let probe = source.probe().await;
            // A PostgreSQL or MongoDB source queues its advice changes and its unsupported versions (8a.12,
            // 8a.14, 8b.12). One log line for each. They hold the names of the check and no SQL text.
            for change in source.take_advice_changes() {
                state.logger.source_advice(source.id().as_str(), change.check, &change.names);
            }
            for version in source.take_version_unsupported() {
                state.logger.source_version_unsupported(source.id().as_str(), source.kind().as_str(), &version);
            }
            // A PostgreSQL or MongoDB source reports its version by `take_version_unsupported` above: a
            // second line from `Probe` would repeat it.
            let own_version = if source.kind() == SourceKind::ClickHouse { probe.version_supported } else { None };
            match (&probe.version, own_version) {
                (Some(v), Some(false)) if warned_version.as_deref() != Some(v.as_str()) => {
                    state.logger.source_version_unsupported(source.id().as_str(), source.kind().as_str(), v);
                    warned_version = Some(v.clone());
                }
                (_, Some(true)) => warned_version = None,
                _ => {}
            }
            tokio::select! {
                () = tokio::time::sleep(state.timing.probe_interval) => {}
                _ = stop.changed() => {
                    if *stop.borrow() != StopStage::Running { return; }
                }
            }
        }
    });
}

/// Follows the state of a source: the health, the metric and one log line for each change.
fn spawn_watcher(state: Arc<AppState>, source: AnySource) {
    let mut stop = state.stop_signal();
    let mut rx = source.watch_state();
    let id = source.id().as_str().to_owned();
    let kind = source.kind().as_str();
    let mut last = *rx.borrow();
    state.health.set_source(&id, last.as_str());
    state.metrics.source_up(&id, last == SourceState::Up);
    tokio::spawn(async move {
        let mut down_since: Option<Instant> = None;
        loop {
            tokio::select! {
                changed = rx.changed() => {
                    if changed.is_err() { return; }
                }
                _ = stop.changed() => {
                    if *stop.borrow() != StopStage::Running { return; }
                    continue;
                }
            }
            let now = *rx.borrow_and_update();
            if now == last {
                continue;
            }
            let detail = source.last_probe().filter(|p| p.state == now).and_then(|p| p.detail);
            match now {
                SourceState::Up => {
                    let down = down_since.take().map_or(0, |t| t.elapsed().as_secs());
                    state.logger.source_up(&id, kind, down);
                }
                SourceState::Down => {
                    down_since.get_or_insert_with(Instant::now);
                    let why = detail.unwrap_or_else(|| "a query could not reach the source".to_owned());
                    state.logger.source_down(&id, kind, &why);
                }
                SourceState::Misconfigured => {
                    down_since.get_or_insert_with(Instant::now);
                    let why = detail.unwrap_or_else(|| "the account or the certificate breaks the contract".to_owned());
                    state.logger.source_misconfigured(&id, kind, &why, None);
                }
                SourceState::Unknown => {}
            }
            state.health.set_source(&id, now.as_str());
            state.metrics.source_up(&id, now == SourceState::Up);
            last = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::tests::harness::test_config;

    #[test]
    fn maps_the_block_and_hides_the_password_in_debug() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let cfg = source_config(&config.clickhouse[0]).unwrap();
        assert_eq!(cfg.id.as_str(), "prod");
        assert_eq!((cfg.host.as_str(), cfg.port, cfg.https), ("127.0.0.1", 8123, false));
        assert_eq!(cfg.password, "secret-password");
        assert!(!format!("{cfg:?}").contains("secret-password"));
        let sources = build_sources(&config).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].state(), SourceState::Unknown);
    }
}
