// SPDX-License-Identifier: MIT OR Apache-2.0
//! The type `PostgresSource` (8a.2, 8a.9, 8a.10, 8a.12, 8.11).
//!
//! A `PostgresSource` is a cheap handle (`Clone`) to the shared state of one `postgres "<id>"`
//! block: its connector, its pool, its admission, its state and its last probe. The methods have
//! the same names and the same types as the methods of `ClickHouseSource`, so `AnySource`
//! dispatches to both in the same way.
//!
//! # A query
//!
//! 1. The state gates the query as for ClickHouse (8a.12): `misconfigured` fails at once, and
//!    `unknown` waits for the first probe within `queue_wait`. A source that has passed no probe
//!    since start, or since its last `misconfigured` verdict, sends nothing to the server and gives
//!    `503 source_unavailable` (finding M2). The text of `source_misconfigured` for a viewer is
//!    fixed: the reason is in the log line `source.misconfigured` (finding L1).
//! 2. The admission of 8.8 gives the permits (`max_concurrent`, `queue_wait`, the limit of one
//!    subject, the memory budget). The wait is the value of `QueryCtx::queue_wait`.
//! 3. A session comes from the pool, or the connector opens one.
//! 4. `exec::run` does the steps of 8a.5. At the start of the transaction, a transport fault on a
//!    reused session makes the source take a new session and start again, one time.
//! 5. A clean session goes back to the pool. Each other session is closed. A session that was in
//!    use when the source became `misconfigured` is closed too: the source counts its changes to
//!    `misconfigured` and the session carries the count of its start (finding L2).
//!
//! The future of [`PostgresSource::query`] is safe to drop. A drop while the query runs means that
//! the client left: a task sends the cancel request and closes the session (8a.7).
//!
//! # The probe and the notices
//!
//! [`PostgresSource::probe`] runs `probe::run_probe` on its own session, within `connect_timeout`
//! plus 10 s (8a.12). The caller runs it at start and every `PROBE_INTERVAL`, as for ClickHouse. The
//! source keeps the advice of the last probe that ran to the end. The advice that is new or has other
//! names is queued as an [`AdviceChange`], and a version below the oldest supported one is queued as a
//! version text. A server below the oldest supported version fails R0 at the setup of the session, and the
//! source is `misconfigured` (finding I7), so a version text is rare. The server calls [`PostgresSource::take_advice_changes`] and
//! [`PostgresSource::take_version_unsupported`] after each probe and writes one log line
//! `source.advice` or `source.version_unsupported` for each value. The source writes no log line itself.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hune_core::frame::Frame;
use hune_core::ids::SourceId;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::{Instant, timeout, timeout_at};

use crate::admission::{Admission, AdmissionStats, Busy};
use crate::budget::{MAX_RESERVATION, ResultMemoryBudget};
use crate::config::PostgresSourceConfig;
use crate::error::{Ceiling, ErrorCtx, SourceError, SourceSetupError};
use crate::postgres::exec::{self, InFlight, Run};
use crate::postgres::pool::Pool;
use crate::postgres::probe::{self, Advice, Outcome as ProbeOutcome};
use crate::postgres::session::{Connector, OpenError, Session};
use crate::types::{EffectiveLimits, LogKind, PreparedQuery, Probe, QueryCtx, SourceKind, SourceState};

/// The most advice changes, and the most versions, that the source queues between two takes.
const MAX_NOTICES: usize = 64;

/// An advice check (A1 to A4) that fails with names that the last probe did not have (8a.12). The
/// server writes one `source.advice` line for each change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdviceChange {
    /// The check: `A1`, `A2`, `A3` or `A4`.
    pub check: &'static str,
    /// The first names (at most five), cleaned.
    pub names: Vec<String>,
}

/// The advice state that the probe keeps from one run to the next.
#[derive(Default)]
struct Notes {
    /// The advice of the last probe that read it. A probe that failed does not change it.
    advice: Vec<Advice>,
    /// The version that gave the last version notice.
    unsupported: Option<String>,
    /// The advice changes that nobody took yet.
    queue: Vec<AdviceChange>,
    /// The versions below the oldest supported one that nobody took yet.
    versions: Vec<String>,
}

/// The text that a viewer gets with `source_misconfigured`. The reason, with the names of the catalog,
/// goes to the log line `source.misconfigured` only (finding L1).
const VIEWER_MISCONFIGURED: &str =
    "The source is misconfigured. The log line source.misconfigured of the server holds the reason.";

/// The text of `source_unavailable` for a source that passed no probe yet (finding M2).
const NOT_VERIFIED: &str = "The source passed no probe yet: the server did not answer, or the account is not checked.";

struct Inner {
    config: PostgresSourceConfig,
    connector: Connector,
    pool: Pool,
    admission: Admission,
    state: AtomicU8,
    state_tx: watch::Sender<SourceState>,
    /// True after a probe that passed, until the next `misconfigured` verdict. A query reaches the server
    /// only when it is true (finding M2).
    verified: AtomicBool,
    /// The number of changes to `misconfigured`. A session goes back to the pool only when this number is the
    /// one of its start (finding L2).
    generation: AtomicU64,
    probe_lock: AsyncMutex<()>,
    probe_seq: AtomicU64,
    last_probe: Mutex<Option<Probe>>,
    notes: Mutex<Notes>,
}

/// One PostgreSQL source (8a). A clone shares the pool, the admission and the state.
#[derive(Clone)]
pub struct PostgresSource {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for PostgresSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresSource").field("id", &self.inner.config.id).field("state", &self.state()).finish()
    }
}

fn check(cond: bool, message: &str) -> Result<(), SourceSetupError> {
    if cond { Ok(()) } else { Err(SourceSetupError::new(message)) }
}

impl PostgresSource {
    /// Makes the source from its settings and the shared memory budget of the results in flight
    /// (`server.max_result_memory_bytes`, 8a.6, 8.8). It opens no connection and sends nothing on
    /// the network.
    ///
    /// # Errors
    ///
    /// A value is out of range, a key cannot go in the startup packet, or the TLS settings do not
    /// match the `sslmode` or do not build.
    pub fn new(config: PostgresSourceConfig, budget: ResultMemoryBudget) -> Result<PostgresSource, SourceSetupError> {
        check(!config.host.is_empty(), "host: the value is empty")?;
        check(config.port != 0, "port: the value is 0")?;
        check(!config.user.is_empty(), "user: the value is empty")?;
        check(!config.database.is_empty(), "database: the value is empty")?;
        check(config.max_concurrent >= 1, "max_concurrent: the value is below 1")?;
        check(config.max_concurrent_per_subject != Some(0), "max_concurrent_per_subject: the value is 0")?;
        check(config.max_result_rows >= 1, "max_result_rows: the value is below 1")?;
        check(config.max_result_bytes >= 1, "max_result_bytes: the value is below 1")?;
        check(
            config.max_result_bytes.saturating_mul(2) <= MAX_RESERVATION,
            "max_result_bytes: twice the value is above 4294967295",
        )?;
        check(config.max_query_size >= 1, "max_query_size: the value is below 1")?;
        check(config.max_execution_time >= Duration::from_secs(1), "max_execution_time: the value is below 1s")?;
        check(!config.connect_timeout.is_zero(), "connect_timeout: the value is 0")?;
        let connector = Connector::new(&config)?;
        let admission = Admission::new(
            config.max_concurrent,
            config.max_concurrent_per_subject,
            config.queue_wait,
            budget,
            config.max_result_bytes * 2,
        );
        let (state_tx, _) = watch::channel(SourceState::Unknown);
        Ok(PostgresSource {
            inner: Arc::new(Inner {
                pool: Pool::new(config.max_concurrent as usize),
                config,
                connector,
                admission,
                state: AtomicU8::new(SourceState::Unknown.to_u8()),
                state_tx,
                verified: AtomicBool::new(false),
                generation: AtomicU64::new(0),
                probe_lock: AsyncMutex::new(()),
                probe_seq: AtomicU64::new(0),
                last_probe: Mutex::new(None),
                notes: Mutex::new(Notes::default()),
            }),
        })
    }

    /// Gives the id of the block.
    pub fn id(&self) -> &SourceId {
        &self.inner.config.id
    }

    /// Gives the label of the block.
    pub fn label(&self) -> &str {
        &self.inner.config.label
    }

    /// Gives the kind of the source.
    pub fn kind(&self) -> SourceKind {
        SourceKind::Postgres
    }

    /// Gives the state of 8a.12.
    pub fn state(&self) -> SourceState {
        SourceState::from_u8(self.inner.state.load(Ordering::Acquire))
    }

    /// Gives a receiver that sees each change of the state. The server uses it for the log lines
    /// `source.up`, `source.down` and `source.misconfigured` and for the metric `hune_source_up`.
    pub fn watch_state(&self) -> watch::Receiver<SourceState> {
        self.inner.state_tx.subscribe()
    }

    /// Gives the counters of the admission (8a.10), for the metrics of section 14.
    pub fn admission_stats(&self) -> AdmissionStats {
        self.inner.admission.stats()
    }

    /// Gives the result of the last probe, or `None` before the first one.
    pub fn last_probe(&self) -> Option<Probe> {
        self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Gives the number of idle sessions in the pool.
    pub fn idle_connections(&self) -> usize {
        self.inner.pool.idle_count()
    }

    /// Gives the advice changes that the probes found since the last call, oldest first. The server
    /// writes one `source.advice` line for each change (see the module comment). At most 64 changes
    /// wait: a source that nobody drains drops the oldest ones.
    pub fn take_advice_changes(&self) -> Vec<AdviceChange> {
        std::mem::take(&mut self.inner.notes.lock().unwrap_or_else(PoisonError::into_inner).queue)
    }

    /// Gives the versions below the oldest supported release that the probes found since the last
    /// call, one for each change of the version. The server writes one `source.version_unsupported`
    /// line for each of them.
    pub fn take_version_unsupported(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.notes.lock().unwrap_or_else(PoisonError::into_inner).versions)
    }

    fn set_state(&self, new: SourceState) -> SourceState {
        let prev = SourceState::from_u8(self.inner.state.swap(new.to_u8(), Ordering::AcqRel));
        if prev != new {
            self.inner.state_tx.send_replace(new);
        }
        if new == SourceState::Misconfigured {
            self.inner.verified.store(false, Ordering::Release);
            if prev != SourceState::Misconfigured {
                // The count changes before the pool closes its idle sessions, so a session that goes back
                // later sees the change (finding L2).
                self.inner.generation.fetch_add(1, Ordering::AcqRel);
            }
            // The pool closes its idle sessions at the change (8a.12).
            self.inner.pool.clear();
        }
        prev
    }

    /// Changes the state from `from` to `to` only when the state is still `from`. A verdict of a probe that
    /// came in the meantime wins (finding M2).
    fn set_state_if(&self, from: SourceState, to: SourceState) {
        let swapped =
            self.inner.state.compare_exchange(from.to_u8(), to.to_u8(), Ordering::AcqRel, Ordering::Acquire).is_ok();
        if swapped && from != to {
            self.inner.state_tx.send_replace(to);
        }
    }

    /// Updates the state after a query (8a.12). A query never changes a `misconfigured` state to another one:
    /// only a probe that passes does (finding M2).
    fn note_result(&self, result: &Result<Frame, SourceError>) {
        match result {
            Ok(_) => {
                if self.inner.verified.load(Ordering::Acquire) {
                    self.set_state_if(SourceState::Down, SourceState::Up);
                }
            }
            Err(SourceError::SourceUnavailable { .. }) => {
                self.set_state_if(SourceState::Up, SourceState::Down);
                self.set_state_if(SourceState::Unknown, SourceState::Down);
            }
            Err(SourceError::SourceMisconfigured { .. }) => {
                self.set_state(SourceState::Misconfigured);
            }
            Err(_) => {}
        }
    }

    fn busy(&self, why: Busy) -> SourceError {
        SourceError::SourceBusy { source: self.inner.config.id.clone(), message: why.message().to_owned() }
    }

    /// The error of a source that is `misconfigured`. The text is fixed (finding L1).
    fn misconfigured_error(&self) -> SourceError {
        SourceError::SourceMisconfigured {
            source: self.inner.config.id.clone(),
            message: VIEWER_MISCONFIGURED.to_owned(),
        }
    }

    /// The error of a source that passed no probe (finding M2).
    fn not_verified_error(&self) -> SourceError {
        SourceError::SourceUnavailable { source: self.inner.config.id.clone(), message: NOT_VERIFIED.to_owned() }
    }

    /// Keeps the reason of a refused setup of a session in the last probe result, so the server writes it in the
    /// log line `source.misconfigured`. The viewer never gets it (finding L1).
    fn record_setup_refusal(&self, detail: &str) {
        let previous = self.state();
        *self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner) = Some(Probe {
            state: SourceState::Misconfigured,
            previous,
            version: None,
            version_supported: None,
            detail: Some(detail.to_owned()),
        });
    }

    /// Runs one query within the limits and gives one frame (7.16a).
    ///
    /// The future is safe to drop. A drop while the query runs sends a cancel request and closes
    /// the session (8a.7).
    pub async fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> Result<Frame, SourceError> {
        let id = &self.inner.config.id;
        let PreparedQuery::PostgresSql { sql } = query else {
            // The server sends a query of the kind of the source. A mismatch is a fault of the caller.
            return Err(SourceError::SourceBadResponse {
                source: id.clone(),
                query_id: ctx.query_id,
                message: "The prepared query is not a PostgreSQL query.".to_owned(),
            });
        };
        // A ceiling of 0 gives an error before the send (8a.8, 8.5).
        if limits.max_result_rows == 0 {
            return Err(SourceError::CeilingExceeded {
                ceiling: Ceiling::MaxRows,
                limit: 0,
                message: "The ceiling max_rows is 0: no query can run.".to_owned(),
            });
        }
        if limits.max_execution_time.is_zero() {
            return Err(SourceError::CeilingExceeded {
                ceiling: Ceiling::MaxDuration,
                limit: 0,
                message: "The ceiling max_duration is 0: no query can run.".to_owned(),
            });
        }
        // The check of 7.13 is in `hune-core`. This is a second barrier: the text never reaches the server.
        if sql.len() as u64 > limits.max_query_size {
            return Err(SourceError::QuerySqlError {
                source: id.clone(),
                sqlstate: None,
                query_id: ctx.query_id,
                message: format!("The SQL text is larger than the limit of {} bytes.", limits.max_query_size),
            });
        }
        // The wait in the queue starts at the arrival and ends when the query gets its permits or
        // the admission refuses it (8a.10). The wait for the first probe of an unknown source counts.
        let arrival = Instant::now();
        let deadline = self.inner.admission.deadline();
        match self.state() {
            SourceState::Misconfigured => return Err(self.misconfigured_error()),
            SourceState::Unknown => {
                // A query on an unknown source waits for the first probe, within `queue_wait` (8a.12).
                if timeout_at(deadline, self.first_probe()).await.is_err() {
                    ctx.queue_wait.set(arrival.elapsed());
                    return Err(self.busy(Busy::Timeout));
                }
                if self.state() == SourceState::Misconfigured {
                    ctx.queue_wait.set(arrival.elapsed());
                    return Err(self.misconfigured_error());
                }
            }
            SourceState::Up | SourceState::Down => {}
        }
        // No probe passed since the start or since the last `misconfigured` verdict: nothing goes to the
        // server (8a.12, finding M2).
        if !self.inner.verified.load(Ordering::Acquire) {
            ctx.queue_wait.set(arrival.elapsed());
            return Err(self.not_verified_error());
        }
        let permits = self.inner.admission.acquire(ctx.subject.as_str(), deadline).await;
        ctx.queue_wait.set(arrival.elapsed());
        let permits = permits.map_err(|why| self.busy(why))?;
        let log_comment = ctx.log_comment;
        let run = Run {
            source: id,
            sql: &sql,
            limits: &limits,
            kind: LogKind::from(ctx.kind),
            query_id: ctx.query_id,
            log_comment: &log_comment,
        };
        let result = self.execute(&run, permits).await;
        self.note_result(&result);
        result
    }

    /// Takes an idle session, or opens a new one.
    async fn checkout(&self, fresh_only: bool) -> Result<(Session, bool), OpenError> {
        if !fresh_only && let Some(s) = self.inner.pool.take() {
            return Ok((s, true));
        }
        Ok((self.inner.connector.open().await?, false))
    }

    /// Runs the steps 2 to 11 of 8a.5 with the permits of the admission.
    async fn execute(&self, run: &Run<'_>, permits: crate::admission::Permits) -> Result<Frame, SourceError> {
        let ectx = ErrorCtx { source: run.source, query_id: run.query_id, limits: run.limits };
        let mut permits = Some(permits);
        // A query goes again one time only, and only when a reused session failed at the start of the
        // transaction (8a.5, step 4). After this step Hune never sends a query again.
        let mut retried = false;
        let mut fresh_only = false;
        // The count of the changes to `misconfigured` at the start. A session that outlives a change does not
        // go back to the pool (finding L2).
        let generation = self.inner.generation.load(Ordering::Acquire);
        loop {
            let (session, reused) = match self.checkout(fresh_only).await {
                Ok(x) => x,
                Err(OpenError::Unavailable(m)) => return Err(ectx.unavailable(m)),
                Err(OpenError::Misconfigured(m)) => {
                    self.record_setup_refusal(&m);
                    return Err(ectx.misconfigured(VIEWER_MISCONFIGURED));
                }
            };
            let held = permits.take().expect("the permits are held at each turn of the loop");
            let mut flight = InFlight::new(session, held);
            let outcome = exec::run(flight.session_mut(), run).await;
            let (session, held) = flight.finish();
            if outcome.begin_fault && reused && !retried {
                retried = true;
                fresh_only = true;
                drop(session);
                permits = Some(held);
                continue;
            }
            if outcome.keep && self.inner.generation.load(Ordering::Acquire) == generation {
                self.inner.pool.put(session);
                // The check after the put closes the gap with a change that came between the two steps: the
                // change counts before it clears the pool, so one of the two steps sees the other.
                if self.inner.generation.load(Ordering::Acquire) != generation {
                    self.inner.pool.clear();
                }
            } else {
                drop(session);
            }
            drop(held);
            return outcome.result;
        }
    }

    /// Runs the first probe when no result exists. One probe runs at a time.
    async fn first_probe(&self) {
        let _guard = self.inner.probe_lock.lock().await;
        if self.state() == SourceState::Unknown {
            self.run_probe().await;
        }
    }

    /// Runs the probe of 8a.12 and gives its result.
    ///
    /// The probe takes no permit of the admission. One probe runs at a time: a call that waits for
    /// another probe gives the result of that probe. The deadline of the whole probe is
    /// `connect_timeout` plus 10 s. The caller runs it at start and every `PROBE_INTERVAL`.
    pub async fn probe(&self) -> Probe {
        let seen = self.inner.probe_seq.load(Ordering::Acquire);
        let _guard = self.inner.probe_lock.lock().await;
        if self.inner.probe_seq.load(Ordering::Acquire) != seen
            && let Some(p) = self.last_probe()
        {
            return p;
        }
        self.run_probe().await
    }

    async fn run_probe(&self) -> Probe {
        let previous = self.state();
        let limit = probe::probe_deadline(self.inner.config.connect_timeout);
        let (probe, report) = match timeout(limit, self.probe_steps(previous)).await {
            Ok(x) => x,
            Err(_) => (probe::finish(previous, &ProbeOutcome::Fault { sqlstate: None }), None),
        };
        self.set_state(probe.state);
        if probe.state == SourceState::Up {
            self.inner.verified.store(true, Ordering::Release);
        }
        if let Some(report) = report {
            self.keep_notes(&report);
        }
        *self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner) = Some(probe.clone());
        self.inner.probe_seq.fetch_add(1, Ordering::AcqRel);
        probe
    }

    /// Opens the session of the probe and runs the checks. It gives the probe, and the report when
    /// the checks ran to the end.
    async fn probe_steps(&self, previous: SourceState) -> (Probe, Option<probe::Report>) {
        // A fault of the setup never moves a `misconfigured` source to `down` (finding M2).
        let fault = |state: SourceState, message: String| Probe {
            state: probe::sticky(previous, state),
            previous,
            version: None,
            version_supported: None,
            detail: Some(message),
        };
        let mut session = match self.inner.connector.open().await {
            Ok(s) => s,
            Err(OpenError::Misconfigured(m)) => return (fault(SourceState::Misconfigured, m), None),
            Err(OpenError::Unavailable(m)) => return (fault(SourceState::Down, m), None),
        };
        // The probe session is closed at the end: each probe checks the TLS, the login and R0 again.
        match probe::run_probe(&mut session.client, &self.inner.config.search_path).await {
            Ok(report) => {
                let outcome = ProbeOutcome::Checked(report.clone());
                (probe::finish(previous, &outcome), Some(report))
            }
            Err(e) => (probe::finish(previous, &ProbeOutcome::from_error(&e)), None),
        }
    }

    /// Keeps the advice of a probe that ran to the end, and queues the notices (8a.12).
    fn keep_notes(&self, report: &probe::Report) {
        let mut notes = self.inner.notes.lock().unwrap_or_else(PoisonError::into_inner);
        if report.failure.is_none() {
            let changes: Vec<AdviceChange> = probe::advice_changes(&notes.advice, &report.advice)
                .into_iter()
                .map(|a| AdviceChange { check: a.check(), names: a.names.clone() })
                .collect();
            notes.queue.extend(changes);
            notes.advice.clone_from(&report.advice);
        }
        if report.version_num < probe::OLDEST_SUPPORTED_VERSION_NUM {
            let version = probe::version_text(report.version_num);
            if notes.unsupported.as_deref() != Some(version.as_str()) {
                notes.unsupported = Some(version.clone());
                notes.versions.push(version);
            }
        } else {
            notes.unsupported = None;
        }
        let excess = notes.queue.len().saturating_sub(MAX_NOTICES);
        notes.queue.drain(..excess);
        let excess = notes.versions.len().saturating_sub(MAX_NOTICES);
        notes.versions.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PostgresSslMode;
    use crate::types::Uuid;

    fn config() -> PostgresSourceConfig {
        PostgresSourceConfig {
            id: SourceId::parse("metrics").expect("valid id"),
            label: "Metrics".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 1,
            database: "analytics".to_owned(),
            user: "hune_ro".to_owned(),
            password: "secret-password".to_owned(),
            search_path: vec!["metrics".to_owned()],
            sslmode: PostgresSslMode::Disable,
            tls: None,
            connect_timeout: Duration::from_secs(2),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100_000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            max_concurrent: 16,
            queue_wait: Duration::from_secs(5),
            max_concurrent_per_subject: None,
        }
    }

    #[test]
    fn a_new_source_has_the_postgres_kind_and_an_unknown_state() {
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        assert_eq!(source.kind(), SourceKind::Postgres);
        assert_eq!(source.kind().as_str(), "postgres");
        assert_eq!(source.state(), SourceState::Unknown);
        assert_eq!(source.label(), "Metrics");
        assert!(source.last_probe().is_none());
        assert!(source.take_advice_changes().is_empty());
        assert!(source.take_version_unsupported().is_empty());
        assert_eq!(source.idle_connections(), 0);
        let stats = source.admission_stats();
        assert_eq!((stats.in_flight, stats.waiting), (0, 0));
    }

    #[test]
    fn config_debug_hides_the_password() {
        let text = format!("{:?}", config());
        assert!(!text.contains("secret-password"));
        assert!(text.contains("(hidden)"));
        let source = format!("{:?}", PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds"));
        assert!(!source.contains("secret-password"));
    }

    #[test]
    fn a_value_out_of_range_is_a_setup_error() {
        type Edit = fn(&mut PostgresSourceConfig);
        let cases: [(Edit, &str); 8] = [
            (|c| c.max_concurrent = 0, "max_concurrent"),
            (|c| c.max_concurrent_per_subject = Some(0), "max_concurrent_per_subject"),
            (|c| c.max_result_rows = 0, "max_result_rows"),
            (|c| c.max_result_bytes = 0, "max_result_bytes"),
            (|c| c.max_result_bytes = u64::MAX, "max_result_bytes"),
            (|c| c.max_execution_time = Duration::from_millis(500), "max_execution_time"),
            (|c| c.connect_timeout = Duration::ZERO, "connect_timeout"),
            (|c| c.port = 0, "port"),
        ];
        for (edit, key) in cases {
            let mut c = config();
            edit(&mut c);
            let e = PostgresSource::new(c, ResultMemoryBudget::new(1 << 30)).unwrap_err();
            assert!(e.message().starts_with(key), "{key}: {}", e.message());
        }
    }

    fn advice(check: crate::postgres::probe::AdviceCheck, names: &[&str]) -> Advice {
        Advice { check, names: names.iter().map(|n| (*n).to_owned()).collect() }
    }

    fn report(version_num: i32, failure: bool, advice: Vec<Advice>) -> probe::Report {
        let failure = failure.then(|| probe::check_names(probe::Check::R4, &["metrics.t".to_owned()]).unwrap_err());
        probe::Report { version_num, failure, advice }
    }

    #[test]
    fn advice_comes_one_time_for_each_change_and_a_failed_probe_keeps_it() {
        use crate::postgres::probe::AdviceCheck::{A1, A2};
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        let a1 = advice(A1, &["plpgsql"]);
        let a2 = advice(A2, &["temp"]);
        source.keep_notes(&report(180_001, false, vec![a1.clone(), a2.clone()]));
        let changes = source.take_advice_changes();
        assert_eq!(changes.len(), 2);
        assert_eq!((changes[0].check, changes[0].names.as_slice()), ("A1", &["plpgsql".to_owned()][..]));
        // The same result gives no notice.
        source.keep_notes(&report(180_001, false, vec![a1.clone(), a2.clone()]));
        assert!(source.take_advice_changes().is_empty());
        // A probe that failed a mandatory check does not reset the advice.
        source.keep_notes(&report(180_001, true, Vec::new()));
        source.keep_notes(&report(180_001, false, vec![a1.clone(), a2]));
        assert!(source.take_advice_changes().is_empty());
        // A new name is a change.
        source.keep_notes(&report(180_001, false, vec![advice(A1, &["plpgsql", "plperl"])]));
        let changes = source.take_advice_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!((changes[0].check, changes[0].names.len()), ("A1", 2));
        // The advice that closed and comes back is a change.
        source.keep_notes(&report(180_001, false, Vec::new()));
        source.keep_notes(&report(180_001, false, vec![a1]));
        assert_eq!(source.take_advice_changes().len(), 1);
    }

    #[test]
    fn an_old_version_gives_one_notice_for_each_change_of_the_version() {
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        source.keep_notes(&report(140_012, false, Vec::new()));
        assert_eq!(source.take_version_unsupported(), vec!["14.12".to_owned()]);
        source.keep_notes(&report(140_012, false, Vec::new()));
        assert!(source.take_version_unsupported().is_empty());
        source.keep_notes(&report(130_001, false, Vec::new()));
        assert_eq!(source.take_version_unsupported().len(), 1);
        // A supported version resets: a later downgrade is a change again.
        source.keep_notes(&report(180_001, false, Vec::new()));
        source.keep_notes(&report(140_012, false, Vec::new()));
        assert_eq!(source.take_version_unsupported().len(), 1);
    }

    #[test]
    fn the_queue_of_advice_changes_is_bounded() {
        use crate::postgres::probe::AdviceCheck::A3;
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        for i in 0..200 {
            let name = format!("pg_sleep({i})");
            source.keep_notes(&report(180_001, false, vec![advice(A3, &[name.as_str()])]));
        }
        assert_eq!(source.take_advice_changes().len(), MAX_NOTICES);
    }

    #[tokio::test]
    async fn a_query_of_another_kind_is_a_bad_response() {
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        let ctx = crate::types::QueryCtx {
            kind: crate::types::QueryKind::Panel,
            query_id: Uuid::now_v7(),
            log_comment: crate::types::LogComment::probe(),
            subject: crate::types::SubjectId::open(),
            traceparent: None,
            queue_wait: crate::types::QueueWait::new(),
        };
        let limits = EffectiveLimits {
            max_execution_time: Duration::from_secs(5),
            max_result_rows: 10,
            max_result_bytes: 1000,
            max_query_size: 1000,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        };
        let e = source
            .query(
                PreparedQuery::ClickHouseSql { sql: "SELECT 1".to_owned(), params: Default::default() },
                limits,
                ctx.clone(),
            )
            .await
            .unwrap_err();
        assert_eq!(e.code(), "source_bad_response");
        // A ceiling of 0 and a text above the size limit never reach the server.
        let zero_rows = EffectiveLimits { max_result_rows: 0, ..limits };
        let e = source
            .query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, zero_rows, ctx.clone())
            .await
            .unwrap_err();
        assert_eq!((e.code(), e.ceiling()), ("ceiling_exceeded", Some("max_rows")));
        let zero_time = EffectiveLimits { max_execution_time: Duration::ZERO, ..limits };
        let e = source
            .query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, zero_time, ctx.clone())
            .await
            .unwrap_err();
        assert_eq!((e.code(), e.ceiling()), ("ceiling_exceeded", Some("max_duration")));
        let small = EffectiveLimits { max_query_size: 5, ..limits };
        let e = source.query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, small, ctx).await.unwrap_err();
        assert_eq!((e.code(), e.sqlstate()), ("query_error", Some(None)));
        assert_eq!(source.state(), SourceState::Unknown, "no probe ran");
    }

    fn empty_frame() -> Frame {
        Frame {
            etag: None,
            query_id: String::new(),
            interval_ms: None,
            columns: Vec::new(),
            rows: 0,
            stats: hune_core::frame::FrameStats {
                elapsed_ms: 0,
                read_rows: None,
                read_bytes: None,
                result_bytes: None,
            },
        }
    }

    fn unavailable() -> SourceError {
        SourceError::SourceUnavailable { source: SourceId::parse("metrics").unwrap(), message: "x".to_owned() }
    }

    #[test]
    fn a_query_never_moves_a_misconfigured_source_to_another_state() {
        // Finding M2: a query that ends after the verdict of a probe cannot undo the verdict.
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        source.inner.verified.store(true, Ordering::Release);
        source.set_state(SourceState::Misconfigured);
        assert!(!source.inner.verified.load(Ordering::Acquire), "the verdict closes the flag");
        source.inner.verified.store(true, Ordering::Release);
        source.note_result(&Ok(empty_frame()));
        assert_eq!(source.state(), SourceState::Misconfigured);
        source.note_result(&Err(unavailable()));
        assert_eq!(source.state(), SourceState::Misconfigured);
        // The normal path: Up, a transport fault, Down, a good query, Up.
        source.set_state(SourceState::Up);
        source.inner.verified.store(true, Ordering::Release);
        source.note_result(&Err(unavailable()));
        assert_eq!(source.state(), SourceState::Down);
        source.note_result(&Ok(empty_frame()));
        assert_eq!(source.state(), SourceState::Up);
        // A source that passed no probe is not set to `up` by a query.
        source.set_state(SourceState::Down);
        source.inner.verified.store(false, Ordering::Release);
        source.note_result(&Ok(empty_frame()));
        assert_eq!(source.state(), SourceState::Down);
    }

    #[test]
    fn the_misconfigured_error_has_a_fixed_text_and_the_reason_stays_in_the_probe() {
        // Finding L1: the viewer gets no name of the catalog. The log line gets the detail of the last probe.
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        source.record_setup_refusal("R4: the role can change the table or view secret_schema.payroll.");
        let e = source.misconfigured_error();
        assert_eq!(e.code(), "source_misconfigured");
        assert_eq!(e.message(), VIEWER_MISCONFIGURED);
        assert!(!e.message().contains("payroll") && !e.message().contains("R4"));
        let detail = source.last_probe().and_then(|p| p.detail).unwrap();
        assert!(detail.contains("payroll"));
    }

    #[test]
    fn a_change_to_misconfigured_counts_one_time_and_clears_the_pool() {
        // Finding L2: the count is what a session in use compares when it goes back.
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        let count = || source.inner.generation.load(Ordering::Acquire);
        assert_eq!(count(), 0);
        source.set_state(SourceState::Up);
        assert_eq!(count(), 0);
        source.set_state(SourceState::Misconfigured);
        assert_eq!(count(), 1);
        source.set_state(SourceState::Misconfigured);
        assert_eq!(count(), 1, "the same state again is no change");
        source.set_state(SourceState::Up);
        source.set_state(SourceState::Misconfigured);
        assert_eq!(count(), 2);
    }

    /// A TCP server that counts the connections and closes each one at once.
    fn counting_listener() -> (u16, Arc<AtomicU64>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let accepted = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&accepted);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        (port, accepted)
    }

    fn panel_ctx() -> (crate::types::QueryCtx, EffectiveLimits) {
        let ctx = crate::types::QueryCtx {
            kind: crate::types::QueryKind::Panel,
            query_id: Uuid::now_v7(),
            log_comment: crate::types::LogComment::probe(),
            subject: crate::types::SubjectId::open(),
            traceparent: None,
            queue_wait: crate::types::QueueWait::new(),
        };
        let limits = EffectiveLimits {
            max_execution_time: Duration::from_secs(5),
            max_result_rows: 10,
            max_result_bytes: 1000,
            max_query_size: 1000,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        };
        (ctx, limits)
    }

    #[tokio::test]
    async fn a_source_that_passed_no_probe_sends_nothing_to_the_server() {
        // Finding M2: a probe that fails before any check passes leaves the state `down`. A query then does
        // not reach the server. The listener counts the connections: the probe opens one, the queries none.
        let (port, accepted) = counting_listener();
        let mut c = config();
        c.port = port;
        let source = PostgresSource::new(c, ResultMemoryBudget::new(1 << 30)).expect("builds");
        let p = source.probe().await;
        assert_eq!(p.state, SourceState::Down);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after_probe = accepted.load(Ordering::SeqCst);
        assert_eq!(after_probe, 1);
        for _ in 0..3 {
            let (ctx, limits) = panel_ctx();
            let e =
                source.query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, limits, ctx).await.unwrap_err();
            assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(accepted.load(Ordering::SeqCst), after_probe, "no query reached the server");
        assert_eq!(source.state(), SourceState::Down);
    }

    #[tokio::test]
    async fn a_fault_of_the_probe_never_moves_a_misconfigured_source_out_of_the_state() {
        // Finding M2 (flip.out of the review): the probe 1 gave `misconfigured`, the probe 2 met a fault and
        // gave `down`, and the queries then reached the server with a role that failed a check. Now the
        // verdict stays until a probe passes. The listener closes each connection: each probe has a fault.
        let (port, accepted) = counting_listener();
        let mut c = config();
        c.port = port;
        let source = PostgresSource::new(c, ResultMemoryBudget::new(1 << 30)).expect("builds");
        source.set_state(SourceState::Misconfigured);
        let p = source.probe().await;
        assert_eq!((p.state, p.previous), (SourceState::Misconfigured, SourceState::Misconfigured), "{p:?}");
        assert_eq!(source.state(), SourceState::Misconfigured);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after_probe = accepted.load(Ordering::SeqCst);
        assert_eq!(after_probe, 1);
        for _ in 0..3 {
            let (ctx, limits) = panel_ctx();
            let e =
                source.query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, limits, ctx).await.unwrap_err();
            assert_eq!(e.code(), "source_misconfigured");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(accepted.load(Ordering::SeqCst), after_probe, "no query reached the server");
        // A second fault changes nothing.
        let p = source.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured);
    }

    #[tokio::test]
    async fn a_query_on_an_unknown_source_runs_one_probe_and_then_the_gate() {
        let (port, accepted) = counting_listener();
        let mut c = config();
        c.port = port;
        let source = PostgresSource::new(c, ResultMemoryBudget::new(1 << 30)).expect("builds");
        let (ctx, limits) = panel_ctx();
        let e = source.query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, limits, ctx).await.unwrap_err();
        assert_eq!(e.code(), "source_unavailable");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(accepted.load(Ordering::SeqCst), 1, "the first probe only");
    }

    #[tokio::test]
    async fn a_source_that_cannot_connect_is_down_and_the_error_is_retryable() {
        // Port 1 on the loopback interface has no listener.
        let source = PostgresSource::new(config(), ResultMemoryBudget::new(1 << 30)).expect("builds");
        let p = source.probe().await;
        assert_eq!(p.state, SourceState::Down);
        assert!(p.detail.as_deref().is_some_and(|d| d.contains("connection refused")), "{p:?}");
        assert_eq!(source.state(), SourceState::Down);
        assert!(source.last_probe().is_some());
        let ctx = crate::types::QueryCtx {
            kind: crate::types::QueryKind::Panel,
            query_id: Uuid::now_v7(),
            log_comment: crate::types::LogComment::probe(),
            subject: crate::types::SubjectId::open(),
            traceparent: None,
            queue_wait: crate::types::QueueWait::new(),
        };
        let limits = EffectiveLimits {
            max_execution_time: Duration::from_secs(5),
            max_result_rows: 10,
            max_result_bytes: 1000,
            max_query_size: 1000,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        };
        let wait = ctx.queue_wait.clone();
        let e = source.query(PreparedQuery::PostgresSql { sql: "SELECT 1".to_owned() }, limits, ctx).await.unwrap_err();
        assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
        assert!(wait.get().is_some(), "the queue wait is set");
        assert!(!e.message().contains("secret-password"));
    }
}

#[cfg(test)]
#[path = "source/tests/container_tests.rs"]
mod container_tests;
