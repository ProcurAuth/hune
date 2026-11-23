//! The steps 6 to 9 of the start sequence (14.12): open the store (the volume or PostgreSQL),
//! load the catalog and start the poller. The hook `open_store` is the one that `serve` runs.

use super::lifecycle::StartError;
use super::state::{AppState, Services, StopStage};
use crate::config::{LogLevel, PostgresConfig, SslMode, StorageConfig};
use hune_store::{
    AnyStore, Catalog, PgOptions, PgSslMode, PgStore, StoreEvent, SyncTransition, VolumeOptions, VolumeStore,
};
use std::sync::Arc;
use std::time::Instant;

/// Builds the options of the PostgreSQL backend from the block `storage.postgres` (13.10, 5.23).
///
/// The password comes from the loaded configuration, so it already holds the value of
/// `password_env` or `password_file`. `check_role` is `true` for `serve` only (5.29, step 6).
/// The function changes no value: a configuration that cannot work gives an error.
pub(crate) fn pg_options(pg: &PostgresConfig, check_role: bool) -> Result<PgOptions, String> {
    let sslmode = match (pg.sslmode, &pg.tls) {
        (SslMode::Disable, _) => PgSslMode::Disable,
        (SslMode::VerifyFull, Some(tls)) => {
            PgSslMode::VerifyFull { ca_pem: tls.ca_pem.clone(), server_name: tls.server_name.clone() }
        }
        (SslMode::VerifyFull, None) => {
            return Err("storage.postgres.tls: the block is necessary when sslmode is \"verify-full\"".to_owned());
        }
    };
    Ok(PgOptions {
        host: pg.host.clone(),
        port: pg.port,
        database: pg.database.clone(),
        user: pg.user.clone(),
        password: Some(pg.password.expose().to_owned()),
        sslmode,
        max_connections: pg.max_connections as usize,
        connect_timeout: pg.connect_timeout,
        statement_timeout: pg.statement_timeout,
        poll_interval: pg.poll_interval,
        history_keep: pg.history_keep,
        check_role,
    })
}

/// Opens the PostgreSQL backend for `serve` (14.12 steps 6 and 7). A temporary error writes
/// `store.sync_failed` one time and the function tries again. A permanent error stops the start.
async fn open_postgres(state: &AppState, pg: &PostgresConfig) -> Result<AnyStore, StartError> {
    let options = pg_options(pg, true).map_err(|e| StartError::new("store.open", e))?;
    let log = state.logger.clone();
    let mut down_since: Option<Instant> = None;
    let result = PgStore::open_waiting(options, |e| {
        if down_since.is_none() {
            down_since = Some(Instant::now());
            log.store_sync_failed("postgres", &e.message);
            state.metrics.store_up(false);
        }
    })
    .await;
    match result {
        Ok(store) => {
            if let Some(since) = down_since {
                state.logger.store_sync_recovered("postgres", since.elapsed().as_secs());
            }
            Ok(AnyStore::Postgres(store))
        }
        Err(e) => Err(StartError::new("store.open", e.message)),
    }
}

/// Writes the log lines for the events that the store holds (14.7).
pub fn log_store_events(state: &AppState, events: Vec<StoreEvent>) {
    let log = &state.logger;
    for event in events {
        match event {
            StoreEvent::EntryIgnored { path, reason } => {
                log.event(LogLevel::Warn, "store.entry_ignored").field("path", path).field("reason", reason).emit();
            }
            StoreEvent::ExternalChange { uid, folder, etag_before, etag_after } => {
                log.event(LogLevel::Info, "dashboard.changed")
                    .opt_str("request_id", None)
                    .field("action", "external")
                    .field("uid", uid.as_str())
                    .field("folder", folder.as_str())
                    .opt_str("from_folder", None)
                    .opt_str("version", None)
                    .opt_str("etag_before", etag_before.map(|e| e.to_hex()).as_deref())
                    .opt_str("etag_after", etag_after.map(|e| e.to_hex()).as_deref())
                    .field("subject", "-")
                    .opt_str("save_id", None)
                    .opt_str("bytes", None)
                    .emit();
            }
            StoreEvent::LockUnsupported { path, errno } => {
                log.event(LogLevel::Warn, "store.lock_unsupported").field("path", path).field("errno", errno).emit();
            }
            // The store keeps the first entry that blocks a folder delete. The answer is 409.
            StoreEvent::FolderNotEmpty { .. } => {}
        }
    }
}

/// Sets the gauges `hune_documents` from the catalog.
pub fn update_document_metrics(state: &AppState, catalog: &Catalog) {
    let s = catalog.stats();
    state.metrics.documents("valid", s.dashboards as u64);
    state.metrics.documents("invalid", s.invalid as u64);
}

/// Opens the store, loads the catalog one time and starts the poller (14.12 steps 6 to 9).
pub async fn open_store(state: Arc<AppState>) -> Result<Services, StartError> {
    let store = match &state.config.storage {
        StorageConfig::Volume(v) => {
            let options = VolumeOptions {
                root: v.path.clone(),
                read_only: v.read_only,
                rescan_interval: v.rescan_interval,
                history_keep: v.history_keep,
            };
            let volume = VolumeStore::open(options)
                .await
                .map_err(|e| StartError::new("store.open", format!("storage.volume: {e}")))?;
            AnyStore::Volume(volume)
        }
        StorageConfig::Postgres(pg) => open_postgres(&state, pg).await?,
    };
    let begin = Instant::now();
    let catalog = Catalog::load(&store)
        .await
        .map_err(|e| StartError::new("catalog.load", format!("the first load of the catalog failed: {e}")))?;
    let catalog = Arc::new(catalog);
    let stats = catalog.stats();
    log_store_events(&state, catalog.take_events());
    state.logger.store_loaded(
        store.backend_name(),
        stats.folders as u64,
        stats.dashboards as u64,
        (stats.invalid + stats.invalid_folders) as u64,
        0,
        u64::try_from(begin.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    state.metrics.store_up(true);
    update_document_metrics(&state, &catalog);
    spawn_poller(state.clone(), store.clone(), catalog.clone());
    Ok(Services { store, catalog })
}

/// Starts the task that polls the catalog every `poll_interval` (14.12 step 9).
fn spawn_poller(state: Arc<AppState>, store: AnyStore, catalog: Arc<Catalog>) {
    let mut stop = state.stop_signal();
    tokio::spawn(async move {
        let interval = store.poll_interval();
        loop {
            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                _ = stop.changed() => {
                    if *stop.borrow() != StopStage::Running { return; }
                    continue;
                }
            }
            let result = catalog.poll(&store).await;
            log_store_events(&state, result.events);
            let backend = store.backend_name();
            let error = result.outcome.as_ref().err().map(ToString::to_string);
            match &result.outcome {
                Ok(summary) => {
                    let changed = summary.folders_changed
                        + summary.folders_removed
                        + summary.entries_changed
                        + summary.entries_removed;
                    state.metrics.store_poll_changes(changed as u64);
                    state.health.sync_ok();
                    state.metrics.store_up(true);
                    update_document_metrics(&state, &catalog);
                }
                Err(_) => {
                    state.health.sync_failed();
                    state.metrics.store_up(false);
                }
            }
            match result.transition {
                Some(SyncTransition::Failed) => {
                    state.logger.store_sync_failed(backend, error.as_deref().unwrap_or("unknown"));
                }
                Some(SyncTransition::Recovered { down }) => {
                    state.logger.store_sync_recovered(backend, down.as_secs());
                }
                None => {}
            }
        }
    });
}
