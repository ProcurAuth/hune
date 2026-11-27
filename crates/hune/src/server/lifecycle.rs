// SPDX-License-Identifier: MIT OR Apache-2.0
//! The start sequence (14.12) and the graceful stop (14.13).

use super::conn;
use super::limits::Timing;
use super::ops;
use super::state::{AppState, Phase, Services, StopStage};
use crate::config::Config;
use crate::log::Logger;
use crate::metrics::Metrics;
use crate::telemetry::{ExportState, Telemetry};
use crate::web_assets;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// A start step that failed. `serve` writes `server.start_failed` and exits with 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartError {
    /// The step: `shell`, `bind ops_listen`, `store.open`, `catalog.load`, `bind listen`.
    pub step: &'static str,
    /// The message for the operator.
    pub error: String,
}

impl StartError {
    /// Makes a start error.
    pub fn new(step: &'static str, error: impl Into<String>) -> Self {
        StartError { step, error: error.into() }
    }
}

/// The exit code of a successful stop (14.12).
pub const EXIT_OK: u8 = 0;
/// The exit code of a failure (14.12).
pub const EXIT_FAILURE: u8 = 1;

/// A server that runs: the state, the bound addresses and the handles that the stop needs.
pub struct Running {
    /// The shared state.
    pub state: Arc<AppState>,
    /// The bound address of `server.listen`.
    pub listen_addr: SocketAddr,
    /// The bound address of `server.ops_listen`.
    pub ops_addr: SocketAddr,
    stop_accept: watch::Sender<bool>,
    close_connections: watch::Sender<bool>,
    ops_stop: watch::Sender<bool>,
    main_task: JoinHandle<()>,
    ops_task: JoinHandle<()>,
}

fn unix_seconds() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Builds the telemetry handle and the exporter task from the block `telemetry` (14.12 step 4).
/// The hook `on_dropped` adds to `hune_telemetry_dropped_spans_total`. The hook `on_state` writes
/// `telemetry.export_failed` and `telemetry.export_recovered`, one time for each change of state.
fn build_telemetry(
    config: &Config,
    logger: &Arc<Logger>,
    metrics: &Arc<Metrics>,
) -> Result<(Telemetry, crate::telemetry::ExporterTask), StartError> {
    // The spans that the exporter dropped since the last failure began: the value of `export_recovered`.
    let lost = Arc::new(AtomicU64::new(0));
    let (m, counted) = (metrics.clone(), lost.clone());
    let on_dropped: crate::telemetry::DroppedHook = Arc::new(move |n| {
        m.telemetry_dropped(n);
        counted.fetch_add(n, Ordering::Relaxed);
    });
    let log = logger.clone();
    let on_state: crate::telemetry::StateHook = Arc::new(move |state| match state {
        ExportState::Failed { error, dropped } => {
            lost.store(dropped, Ordering::Relaxed);
            log.telemetry_export_failed(&error, dropped);
        }
        ExportState::Recovered => log.telemetry_export_recovered(lost.swap(0, Ordering::Relaxed)),
    });
    Telemetry::start(&config.telemetry, on_dropped, on_state).map_err(|e| StartError::new("telemetry", e.to_string()))
}

fn fail(logger: &Logger, e: &StartError) {
    logger.server_start_failed(e.step, &e.error);
}

/// Does the steps 5 to 10 of 14.12. `hook` does the steps 6 to 9: it opens the store, loads the
/// catalog and starts the tasks, and gives the services. A failure writes `server.start_failed`.
pub async fn start<H, Fut>(
    config: Arc<Config>,
    logger: Arc<Logger>,
    timing: Timing,
    hook: H,
) -> Result<Running, StartError>
where
    H: FnOnce(Arc<AppState>) -> Fut,
    Fut: Future<Output = Result<Services, StartError>>,
{
    let result = start_inner(config, logger.clone(), timing, hook).await;
    if let Err(e) = &result {
        fail(&logger, e);
    }
    result
}

async fn start_inner<H, Fut>(
    config: Arc<Config>,
    logger: Arc<Logger>,
    timing: Timing,
    hook: H,
) -> Result<Running, StartError>
where
    H: FnOnce(Arc<AppState>) -> Fut,
    Fut: Future<Output = Result<Services, StartError>>,
{
    let shell = web_assets::build_shells(&config.server.base_path)
        .ok_or_else(|| StartError::new("shell", "the build has no web/shell.html"))?;
    // The gzip copies of the static files: one time at start, never for a request (9.3, 13.8).
    if config.server.compress_static {
        let _ = web_assets::gzip_copies();
    }
    let metrics = Arc::new(Metrics::new(super::build_info::VERSION, super::build_info::COMMIT, unix_seconds()));
    // Step 4: build the sources. This step sends nothing on the network.
    let sources = super::sources::build_sources(&config)?;
    // The PEP and its transport. This step sends nothing on the network (11.6).
    let access = crate::access::Access::from_config(&config, logger.clone(), metrics.clone())
        .map_err(|e| StartError::new("access", e))?;
    // The telemetry handle and the exporter task. This step sends nothing on the network (14.9).
    let (telemetry, exporter) = build_telemetry(&config, &logger, &metrics)?;
    let state = AppState::new(config.clone(), logger.clone(), metrics, shell, sources, access, telemetry, timing);

    // Step 5: bind the ops listener. It serves at once, and `/readyz` answers 503.
    let ops_listener = TcpListener::bind(config.server.ops_listen).await.map_err(|e| {
        StartError::new("bind ops_listen", format!("server.ops_listen {}: {e}", config.server.ops_listen))
    })?;
    let ops_addr = ops_listener.local_addr().map_err(|e| {
        StartError::new("bind ops_listen", format!("server.ops_listen {}: {e}", config.server.ops_listen))
    })?;
    let (ops_stop, ops_stop_rx) = watch::channel(false);
    let ops_task = tokio::spawn(ops::accept_loop(ops_listener, state.clone(), ops_stop_rx));

    // Steps 6 to 9: the store, the catalog and the tasks.
    let services = match hook(state.clone()).await {
        Ok(s) => s,
        Err(e) => {
            let _ = ops_stop.send(true);
            ops_task.abort();
            return Err(e);
        }
    };
    state.set_services(services);
    state.health.mark_catalog_loaded();
    // Step 9: the probes of the sources. The first probe does not delay the readiness.
    super::sources::spawn_source_tasks(&state);
    // Step 9: the OTLP exporter. With `telemetry.enabled = false`, the task ends at once.
    tokio::spawn(exporter.run());

    // Step 10: bind the main listener and accept.
    let main_listener = match TcpListener::bind(config.server.listen).await {
        Ok(l) => l,
        Err(e) => {
            let _ = ops_stop.send(true);
            ops_task.abort();
            state.set_stop_stage(StopStage::Stopped);
            state.telemetry.shutdown().await;
            return Err(StartError::new("bind listen", format!("server.listen {}: {e}", config.server.listen)));
        }
    };
    let listen_addr = main_listener
        .local_addr()
        .map_err(|e| StartError::new("bind listen", format!("server.listen {}: {e}", config.server.listen)))?;
    let (stop_accept, stop_accept_rx) = watch::channel(false);
    let (close_connections, close_rx) = watch::channel(false);
    let main_task = tokio::spawn(conn::accept_loop(main_listener, state.clone(), stop_accept_rx, close_rx));
    state.health.mark_ready();
    state.metrics.set_ready(true);
    logger.server_listening(&listen_addr.to_string(), &ops_addr.to_string(), state.access_mode(), state.backend_name());
    logger.server_ready(u64::try_from(state.started_at.elapsed().as_millis()).unwrap_or(u64::MAX));
    Ok(Running { state, listen_addr, ops_addr, stop_accept, close_connections, ops_stop, main_task, ops_task })
}

/// A signal during a stop. The first value starts the stop and the second one ends it with 1.
pub type Signals = mpsc::UnboundedReceiver<&'static str>;

impl Running {
    /// The graceful stop of 14.13. It gives the exit code: 0 for a stop in time, 1 for the end of
    /// the grace with requests in flight or for a second signal.
    pub async fn stop(mut self, signal: &str, again: &mut Signals) -> u8 {
        let started = Instant::now();
        let state = self.state.clone();
        let log = state.logger.clone();
        let grace = state.config.server.shutdown_grace;
        let in_flight = || u64::try_from(state.metrics.http_in_flight()).unwrap_or(0);

        // Step 1.
        log.server_stopping(signal, in_flight());
        state.set_phase(Phase::Draining);
        state.metrics.set_ready(false);

        // Step 2: the drain.
        if wait_or_signal(state.timing.drain, again, &log).await {
            return self.abort_now();
        }
        // Step 3: stop the accept, close the idle connections, refuse new requests.
        state.set_phase(Phase::Refusing);
        let _ = self.stop_accept.send(true);
        let _ = self.close_connections.send(true);
        let _ = (&mut self.main_task).await;

        // Step 4: let the requests in flight end, for G minus 3 s at most.
        let step4 = grace.saturating_sub(Duration::from_secs(3));
        if wait_idle(&in_flight, step4, again, &log).await {
            return self.abort_now();
        }
        // Step 5: cancel the work of the sources, for 1 s at most.
        if in_flight() > 0 {
            state.set_stop_stage(StopStage::Cancelling);
            if wait_idle(&in_flight, Duration::from_secs(1), again, &log).await {
                return self.abort_now();
            }
        }
        let remaining = in_flight();
        // Step 6: stop the tasks, then export the OTLP queue one last time, for 2 s at most.
        state.set_stop_stage(StopStage::Stopped);
        state.telemetry.shutdown().await;
        // Step 7: close the ops listener.
        let _ = self.ops_stop.send(true);
        let _ = (&mut self.ops_task).await;
        if remaining > 0 {
            log.server_shutdown_timeout(remaining);
            return EXIT_FAILURE;
        }
        log.server_stopped(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        EXIT_OK
    }

    fn abort_now(&self) -> u8 {
        self.state.set_stop_stage(StopStage::Stopped);
        self.ops_task.abort();
        self.main_task.abort();
        EXIT_FAILURE
    }
}

/// Waits for `d`. A second signal ends the wait: it writes `server.signal_again` and gives `true`.
async fn wait_or_signal(d: Duration, again: &mut Signals, log: &Logger) -> bool {
    tokio::select! {
        () = tokio::time::sleep(d) => false,
        s = again.recv() => {
            if let Some(name) = s {
                log.server_signal_again(name);
                true
            } else {
                // The channel is closed: no signal can come. Wait for the time.
                tokio::time::sleep(d).await;
                false
            }
        }
    }
}

/// Waits until `in_flight()` is 0 or `limit` ends. It gives `true` on a second signal.
async fn wait_idle<F: Fn() -> u64>(in_flight: &F, limit: Duration, again: &mut Signals, log: &Logger) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while in_flight() > 0 && tokio::time::Instant::now() < deadline {
        let step = Duration::from_millis(10);
        tokio::select! {
            () = tokio::time::sleep(step) => {}
            s = again.recv() => {
                if let Some(name) = s {
                    log.server_signal_again(name);
                    return true;
                }
                // The channel is closed: no signal can come.
                tokio::time::sleep(step).await;
            }
        }
    }
    false
}

/// Runs the server until the first signal, then stops it. It gives the exit code of `serve`.
pub async fn run<H, Fut>(config: Arc<Config>, logger: Arc<Logger>, timing: Timing, hook: H, mut signals: Signals) -> u8
where
    H: FnOnce(Arc<AppState>) -> Fut,
    Fut: Future<Output = Result<Services, StartError>>,
{
    let began = Instant::now();
    let running = tokio::select! {
        r = start(config, logger.clone(), timing, hook) => match r {
            Ok(running) => running,
            Err(_) => return EXIT_FAILURE,
        },
        s = signals.recv() => {
            // A signal during the start: nothing serves yet, so the stop is at once.
            let name = s.unwrap_or("SIGTERM");
            logger.server_stopping(name, 0);
            logger.server_stopped(u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX));
            return EXIT_OK;
        }
    };
    let first = signals.recv().await.unwrap_or("SIGTERM");
    running.stop(first, &mut signals).await
}
