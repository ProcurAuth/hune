// SPDX-License-Identifier: MIT OR Apache-2.0
//! The state that the handlers share: the configuration, the logger, the metrics, the shell,
//! the stop phase, the health and the services of the store.

use crate::access::Access;
use crate::config::{AccessConfig, Config, PrimaryRef};
use crate::log::Logger;
use crate::metrics::Metrics;
use crate::telemetry::Telemetry;
use hune_source::AnySource;
use hune_store::{Catalog, Stores};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;
use tokio::sync::{Semaphore, watch};

/// The phase of the process (6.28 step 2, 14.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Normal service.
    Running = 0,
    /// A stop signal came: `/readyz` answers 503 and the listener still serves (the drain).
    Draining = 1,
    /// The accept stopped: a request gets `503 server_stopping`.
    Refusing = 2,
}

/// The stop steps that the background tasks can wait for (14.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopStage {
    /// The process runs.
    Running,
    /// Step 5: the work in flight must end (the sources close their connections).
    Cancelling,
    /// Step 6: the tasks stop.
    Stopped,
}

/// The services that need the store. The start hook makes them (14.12 steps 6 to 9).
pub struct Services {
    /// The stores of the process: the primary store and the read-only stores (5.42).
    pub store: Stores,
    /// The catalog in memory.
    pub catalog: Arc<Catalog>,
}

/// The facts that `/readyz` shows (14.4).
pub struct Health {
    catalog_loaded: AtomicBool,
    ready: AtomicBool,
    last_sync_ok: Mutex<Option<Instant>>,
    store_up: AtomicBool,
    sources: Mutex<BTreeMap<String, &'static str>>,
}

impl Health {
    fn new(source_ids: impl IntoIterator<Item = String>) -> Self {
        Health {
            catalog_loaded: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            last_sync_ok: Mutex::new(None),
            store_up: AtomicBool::new(false),
            sources: Mutex::new(source_ids.into_iter().map(|id| (id, "unknown")).collect()),
        }
    }
    /// Marks the first complete load of the catalog (14.4 rule 1).
    pub fn mark_catalog_loaded(&self) {
        self.catalog_loaded.store(true, Ordering::SeqCst);
        self.sync_ok();
    }
    /// Tells if the catalog loaded one time.
    pub fn catalog_loaded(&self) -> bool {
        self.catalog_loaded.load(Ordering::SeqCst)
    }
    /// Marks that the main listener is bound and `/readyz` can answer 200 (14.12 step 10).
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }
    /// Tells if the start finished.
    pub fn started(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
    /// Records a poll or a scan that succeeded.
    pub fn sync_ok(&self) {
        *self.last_sync_ok.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        self.store_up.store(true, Ordering::SeqCst);
    }
    /// Records a poll or a scan that failed.
    pub fn sync_failed(&self) {
        self.store_up.store(false, Ordering::SeqCst);
    }
    /// Tells if the last poll or scan succeeded.
    pub fn store_up(&self) -> bool {
        self.store_up.load(Ordering::SeqCst)
    }
    /// The whole seconds since the last poll or scan that succeeded.
    pub fn catalog_age_seconds(&self) -> u64 {
        self.last_sync_ok.lock().unwrap_or_else(PoisonError::into_inner).map_or(0, |t| t.elapsed().as_secs())
    }
    /// Sets the state of a source: `up`, `down` or `unknown`.
    pub fn set_source(&self, id: &str, state: &'static str) {
        self.sources.lock().unwrap_or_else(PoisonError::into_inner).insert(id.to_owned(), state);
    }
    /// The state of each source, in byte order of the id.
    pub fn sources(&self) -> BTreeMap<String, &'static str> {
        self.sources.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

/// The places of each subject (12.16): a count for each subject that holds one place or more.
pub struct SubjectCaps {
    held: Mutex<HashMap<String, usize>>,
    max: usize,
}

/// One place of one subject. The place is free again when the value drops.
pub struct SubjectSlot {
    caps: Arc<SubjectCaps>,
    subject: String,
}

impl SubjectCaps {
    /// Makes the counts, with at most `max` places for each subject.
    pub fn new(max: usize) -> Self {
        SubjectCaps { held: Mutex::new(HashMap::new()), max }
    }

    /// Takes a place for the subject. It gives `None` when the subject holds `max` places already.
    pub fn try_take(self: &Arc<Self>, subject: &str) -> Option<SubjectSlot> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let count = held.entry(subject.to_owned()).or_insert(0);
        if *count >= self.max {
            return None;
        }
        *count += 1;
        Some(SubjectSlot { caps: Arc::clone(self), subject: subject.to_owned() })
    }

    /// The number of places that the subject holds.
    pub fn held(&self, subject: &str) -> usize {
        self.held.lock().unwrap_or_else(PoisonError::into_inner).get(subject).copied().unwrap_or(0)
    }
}

impl Drop for SubjectSlot {
    fn drop(&mut self) {
        let mut held = self.caps.held.lock().unwrap_or_else(PoisonError::into_inner);
        // The map keeps no entry with the count 0, so its size stays the number of subjects in flight.
        if let Some(count) = held.get_mut(&self.subject) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                held.remove(&self.subject);
            }
        }
    }
}

/// The shared state of the server. Handlers get it as `Arc<AppState>`.
pub struct AppState {
    /// The configuration, read one time at start.
    pub config: Arc<Config>,
    /// The JSON logger.
    pub logger: Arc<Logger>,
    /// The metrics.
    pub metrics: Arc<Metrics>,
    /// The shell pages, made one time at start (9.3, 9.6).
    pub shells: crate::web_assets::Shells,
    /// The value of `Content-Security-Policy` of each answer of the main listener (12.8), built one time at start from the
    /// block `geomap` (13.12c). Without the block it is the policy of 12.8, byte for byte.
    pub csp: http::HeaderValue,
    /// The data sources, one for each `clickhouse` block, in the order of the file (14.12 step 4).
    pub sources: Vec<AnySource>,
    /// The access layer of the mode `authzen`. It is `None` in the mode `open`.
    pub access: Option<Arc<Access>>,
    /// The telemetry handle (14.9). A disabled handle makes no span.
    pub telemetry: Telemetry,
    /// The facts of `/readyz`.
    pub health: Health,
    /// The instant of the start of the process.
    pub started_at: Instant,
    /// The times of 6.27 and 14.13.
    pub timing: super::limits::Timing,
    /// The places for the bodies of a create, a save or an import (6.27).
    pub doc_bodies: Arc<Semaphore>,
    /// The places for the other bodies (6.27).
    pub small_bodies: Arc<Semaphore>,
    /// The permits of the validations of a document in `spawn_blocking` (6.27, `MAX_INFLIGHT_VALIDATIONS`).
    pub validations: Arc<Semaphore>,
    /// The document bodies of each subject in flight, in the mode `authzen` (12.16).
    pub subject_bodies: Arc<SubjectCaps>,
    /// The validations of each subject that run or wait, in the mode `authzen` (12.16).
    pub subject_validations: Arc<SubjectCaps>,
    /// The number of validations that started in `spawn_blocking` (tests only).
    #[cfg(test)]
    pub validations_run: AtomicU64,
    phase: AtomicU8,
    stop_tx: watch::Sender<StopStage>,
    services: OnceLock<Services>,
    open_connections: AtomicU64,
}

impl AppState {
    /// Makes the state. The services come later with `set_services`.
    // One argument for each part of the state, as the route table does.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<Config>,
        logger: Arc<Logger>,
        metrics: Arc<Metrics>,
        shells: crate::web_assets::Shells,
        sources: Vec<AnySource>,
        access: Option<Arc<Access>>,
        telemetry: Telemetry,
        timing: super::limits::Timing,
    ) -> Arc<AppState> {
        let (stop_tx, _) = watch::channel(StopStage::Running);
        let source_ids: Vec<String> = super::source_blocks::blocks(&config).iter().map(|b| b.id.to_owned()).collect();
        let csp = super::headers::csp_header_value(config.geomap.as_ref());
        Arc::new(AppState {
            config,
            logger,
            metrics,
            shells,
            csp,
            sources,
            access,
            telemetry,
            health: Health::new(source_ids),
            started_at: Instant::now(),
            timing,
            doc_bodies: Arc::new(Semaphore::new(super::limits::MAX_INFLIGHT_DOC_BODIES)),
            small_bodies: Arc::new(Semaphore::new(super::limits::MAX_INFLIGHT_SMALL_BODIES)),
            validations: Arc::new(Semaphore::new(super::limits::MAX_INFLIGHT_VALIDATIONS)),
            subject_bodies: Arc::new(SubjectCaps::new(super::limits::MAX_INFLIGHT_PER_SUBJECT)),
            subject_validations: Arc::new(SubjectCaps::new(super::limits::MAX_INFLIGHT_PER_SUBJECT)),
            #[cfg(test)]
            validations_run: AtomicU64::new(0),
            phase: AtomicU8::new(Phase::Running as u8),
            stop_tx,
            services: OnceLock::new(),
            open_connections: AtomicU64::new(0),
        })
    }

    /// Gives the services. It does nothing when they are set already.
    pub fn set_services(&self, services: Services) {
        let _ = self.services.set(services);
    }

    /// The services of the store, or `None` before the start hook ends.
    pub fn services(&self) -> Option<&Services> {
        self.services.get()
    }

    /// The current phase.
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::SeqCst) {
            0 => Phase::Running,
            1 => Phase::Draining,
            _ => Phase::Refusing,
        }
    }

    /// Changes the phase.
    pub fn set_phase(&self, phase: Phase) {
        self.phase.store(phase as u8, Ordering::SeqCst);
        self.metrics.set_stopping(phase != Phase::Running);
    }

    /// Tells if a stop started.
    pub fn stopping(&self) -> bool {
        self.phase() != Phase::Running
    }

    /// Gives a receiver for the stop stages. A background task waits on it.
    pub fn stop_signal(&self) -> watch::Receiver<StopStage> {
        self.stop_tx.subscribe()
    }

    /// Moves to a stop stage.
    pub fn set_stop_stage(&self, stage: StopStage) {
        self.stop_tx.send_replace(stage);
    }

    /// Finds a source by its id.
    pub fn source(&self, id: &str) -> Option<&AnySource> {
        self.sources.iter().find(|s| s.id().as_str() == id)
    }

    /// The number of connections on the main listener.
    pub fn open_connections(&self) -> u64 {
        self.open_connections.load(Ordering::SeqCst)
    }

    pub(crate) fn connection_opened(&self) -> u64 {
        self.open_connections.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn connection_closed(&self) {
        self.open_connections.fetch_sub(1, Ordering::SeqCst);
    }

    /// True when `/readyz` answers 200 (14.4).
    pub fn is_ready(&self) -> bool {
        self.health.catalog_loaded() && self.health.started() && !self.stopping()
    }

    /// The text of `access.mode`.
    pub fn access_mode(&self) -> &'static str {
        match self.config.access {
            AccessConfig::Open => "open",
            AccessConfig::Authzen(_) => "authzen",
        }
    }

    /// The name of the backend of the primary store: `volume` or `postgres`. It is `none` when no store is the
    /// primary store.
    pub fn backend_name(&self) -> &'static str {
        match self.config.storage.primary() {
            Some(PrimaryRef::Volume(_)) => "volume",
            Some(PrimaryRef::Postgres(_)) => "postgres",
            None => "none",
        }
    }
}
