// SPDX-License-Identifier: MIT OR Apache-2.0
//! The type `MongoSource` (8b.2, 8b.9, 8b.10, 8b.12, 8.11).
//!
//! A `MongoSource` is a cheap handle (`Clone`) to the shared state of one `mongodb "<id>"` block: its
//! connector, its client, its admission, its state and its last probe. The methods have the same
//! names and the same types as the methods of the other sources, so `AnySource` dispatches to it in
//! the same way.
//!
//! # Two ways to make a source
//!
//! - [`MongoSource::new`] makes a source with no client. A build without the driver (8b.2)
//!   has no network client. The source then stays `misconfigured` with [`NO_DRIVER_MESSAGE`]. A query
//!   and a probe tell why. It sends nothing on the network.
//! - [`MongoSource::with_connector`] and [`MongoSource::with_driver`] make a source whose client comes
//!   from a [`MongoConnector`]. The logic is complete and generic over the trait [`MongoClient`]. The
//!   tests use the fake client of the module `fake`. The driver module will use `with_driver`.
//!
//! # A query
//!
//! 1. The state gates the query as for the other sources (8b.12): `misconfigured` fails at once, and
//!    `unknown` waits for the first probe within `queue_wait`. A source that has passed no probe since
//!    start, or since its last `misconfigured` verdict, sends nothing to the server and gives
//!    `503 source_unavailable` (the finding M2 of the PostgreSQL review).
//! 2. The admission of 8.8 gives the permits (`max_concurrent`, `queue_wait`, the limit of one subject,
//!    the memory budget of `2 x max_result_bytes`). The wait is the value of `QueryCtx::queue_wait`.
//! 3. `exec::run` does the steps of 8b.5 to 8b.7 on the client, and ends the session.
//!
//! The future of [`MongoSource::query`] is safe to drop. A drop while the query runs means that the
//! client left: a task kills the cursor, ends the session and gives the permits back (8b.7).
//!
//! # The probe and the notices
//!
//! [`MongoSource::probe`] makes a new client with the connector, runs `probe::run_probe` on it within
//! `connect_timeout` plus 10 s (8b.12), and keeps the client when the probe passes. The client of a
//! probe that fails a mandatory check is dropped, so the connections close at the change to
//! `misconfigured`. A fault of the probe never moves a `misconfigured` source out of this state: only a
//! probe that passes does. The caller runs the probe at start and every `PROBE_INTERVAL`.
//!
//! The source keeps the advice of the last probe that ran to the end. The advice that is new or has
//! other names is queued as an [`AdviceChange`], and a version below 7.0 is queued as a version text. The
//! server calls [`MongoSource::take_advice_changes`] and [`MongoSource::take_version_unsupported`] after
//! each probe and writes one log line `source.advice` or `source.version_unsupported` for each value.
//! The source writes no log line itself.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hune_core::frame::Frame;
use hune_core::ids::SourceId;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::{Instant, timeout, timeout_at};

use crate::admission::{Admission, AdmissionStats, Busy};
use crate::budget::{MAX_RESERVATION, ResultMemoryBudget};
use crate::config::MongoSourceConfig;
use crate::error::{Ceiling, SourceError, SourceSetupError};
use crate::mongo::erased::{ErasedClient, MongoConnector};
use crate::mongo::exec::{self, Killer, Lists, Run};
use crate::mongo::iface::{ConnectOptions, MongoClient};
use crate::mongo::probe::{self, Outcome as ProbeOutcome};
use crate::postgres::AdviceChange;
use crate::types::{EffectiveLimits, PreparedQuery, Probe, QueryCtx, SourceKind, SourceState};

/// The text that a source with no driver gives as the reason of the state `misconfigured`.
pub const NO_DRIVER_MESSAGE: &str = "The MongoDB driver is not in this build.";

/// The most advice changes, and the most versions, that the source queues between two takes.
const MAX_NOTICES: usize = 64;

/// The text that a viewer gets with `source_misconfigured` after a probe verdict. The reason, with the
/// names of the account, goes to the log line `source.misconfigured` only.
const VIEWER_MISCONFIGURED: &str =
    "The source is misconfigured. The log line source.misconfigured of the server holds the reason.";

/// The text of `source_unavailable` for a source that passed no probe yet.
const NOT_VERIFIED: &str = "The source passed no probe yet: the server did not answer, or the account is not checked.";

/// The advice state that the probe keeps from one run to the next.
#[derive(Default)]
struct Notes {
    /// The advice of the last probe that read it. A probe that failed does not change it.
    advice: Vec<probe::Advice>,
    /// The version that gave the last version notice.
    unsupported: Option<String>,
    /// The advice changes that nobody took yet.
    queue: Vec<AdviceChange>,
    /// The versions below the oldest supported one that nobody took yet.
    versions: Vec<String>,
}

struct Inner {
    config: MongoSourceConfig,
    /// `None` for a build with no driver.
    connector: Option<MongoConnector>,
    /// The client of the last probe that passed.
    client: Mutex<Option<Arc<dyn ErasedClient>>>,
    killer: Arc<Killer>,
    admission: Admission,
    state: AtomicU8,
    state_tx: watch::Sender<SourceState>,
    /// True after a probe that passed, until the next `misconfigured` verdict. A query reaches the
    /// server only when it is true.
    verified: AtomicBool,
    probe_lock: AsyncMutex<()>,
    probe_seq: AtomicU64,
    last_probe: Mutex<Option<Probe>>,
    notes: Mutex<Notes>,
    /// The time from the start of a query to its client deadline. `None` means the value of 8b.7. The
    /// tests set a short one.
    deadline: Option<Duration>,
}

/// One MongoDB source (8b). A clone shares the client, the admission and the state.
#[derive(Clone)]
pub struct MongoSource {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for MongoSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MongoSource").field("id", &self.inner.config.id).field("state", &self.state()).finish()
    }
}

fn check(cond: bool, message: &str) -> Result<(), SourceSetupError> {
    if cond { Ok(()) } else { Err(SourceSetupError::new(message)) }
}

impl MongoSource {
    /// Makes the source from its settings and the shared memory budget of the results in flight
    /// (`server.max_result_memory_bytes`, 8b.6, 8.8). It opens no connection and sends nothing on the
    /// network.
    ///
    /// The source has no client: its state is `misconfigured` and its reason is [`NO_DRIVER_MESSAGE`]
    /// (8b.2). [`MongoSource::with_connector`] makes a source with a client.
    ///
    /// # Errors
    ///
    /// A value is out of range.
    pub fn new(config: MongoSourceConfig, budget: ResultMemoryBudget) -> Result<MongoSource, SourceSetupError> {
        MongoSource::build(config, budget, None, None)
    }

    /// Makes the source with a connector. The state is `unknown` until the first probe. It opens no
    /// connection and sends nothing on the network.
    ///
    /// # Errors
    ///
    /// A value is out of range.
    pub fn with_connector(
        config: MongoSourceConfig,
        budget: ResultMemoryBudget,
        connector: MongoConnector,
    ) -> Result<MongoSource, SourceSetupError> {
        MongoSource::build(config, budget, Some(connector), None)
    }

    /// Makes the source with the client type `C`, which the driver module gives (8b.2).
    ///
    /// # Errors
    ///
    /// A value is out of range.
    pub fn with_driver<C: MongoClient + 'static>(
        config: MongoSourceConfig,
        budget: ResultMemoryBudget,
    ) -> Result<MongoSource, SourceSetupError> {
        MongoSource::with_connector(config, budget, MongoConnector::of_type::<C>())
    }

    fn build(
        config: MongoSourceConfig,
        budget: ResultMemoryBudget,
        connector: Option<MongoConnector>,
        deadline: Option<Duration>,
    ) -> Result<MongoSource, SourceSetupError> {
        check(!config.user.is_empty(), "user: the value is empty")?;
        check(!config.auth_database.is_empty(), "auth_database: the value is empty")?;
        check(!config.databases.is_empty(), "databases: the list is empty")?;
        check(!config.collections.is_empty(), "collections: the list is empty")?;
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
        let admission = Admission::new(
            config.max_concurrent,
            config.max_concurrent_per_subject,
            config.queue_wait,
            budget,
            config.max_result_bytes * 2,
        );
        // A source with no connector can never pass a probe: it starts `misconfigured`.
        let first = if connector.is_some() { SourceState::Unknown } else { SourceState::Misconfigured };
        let (state_tx, _) = watch::channel(first);
        Ok(MongoSource {
            inner: Arc::new(Inner {
                killer: Arc::new(Killer::new(config.connect_timeout)),
                config,
                connector,
                client: Mutex::new(None),
                admission,
                state: AtomicU8::new(first.to_u8()),
                state_tx,
                verified: AtomicBool::new(false),
                probe_lock: AsyncMutex::new(()),
                probe_seq: AtomicU64::new(0),
                last_probe: Mutex::new(None),
                notes: Mutex::new(Notes::default()),
                deadline,
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
        SourceKind::MongoDb
    }

    /// Gives the state of 8b.12.
    pub fn state(&self) -> SourceState {
        SourceState::from_u8(self.inner.state.load(Ordering::Acquire))
    }

    /// Gives a receiver that sees each change of the state. The server uses it for the log lines
    /// `source.up`, `source.down` and `source.misconfigured` and for the metric `hune_source_up`.
    pub fn watch_state(&self) -> watch::Receiver<SourceState> {
        self.inner.state_tx.subscribe()
    }

    /// Gives the counters of the admission (8b.10), for the metrics of section 14.
    pub fn admission_stats(&self) -> AdmissionStats {
        self.inner.admission.stats()
    }

    /// Gives the result of the last probe, or `None` before the first one.
    pub fn last_probe(&self) -> Option<Probe> {
        self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Tells if the source holds a client, that is, if a probe passed and no later verdict dropped it.
    pub fn has_client(&self) -> bool {
        self.client().is_some()
    }

    /// Gives the advice changes that the probes found since the last call, oldest first. The server
    /// writes one `source.advice` line for each change (see the module comment). At most 64 changes
    /// wait: a source that nobody drains drops the oldest ones.
    pub fn take_advice_changes(&self) -> Vec<AdviceChange> {
        std::mem::take(&mut self.inner.notes.lock().unwrap_or_else(PoisonError::into_inner).queue)
    }

    /// Gives the versions below 7.0 that the probes found since the last call, one for each change of
    /// the version. The server writes one `source.version_unsupported` line for each of them.
    pub fn take_version_unsupported(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.notes.lock().unwrap_or_else(PoisonError::into_inner).versions)
    }

    fn client(&self) -> Option<Arc<dyn ErasedClient>> {
        self.inner.client.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set_client(&self, client: Option<Arc<dyn ErasedClient>>) {
        *self.inner.client.lock().unwrap_or_else(PoisonError::into_inner) = client;
    }

    fn set_state(&self, new: SourceState) -> SourceState {
        let prev = SourceState::from_u8(self.inner.state.swap(new.to_u8(), Ordering::AcqRel));
        if prev != new {
            self.inner.state_tx.send_replace(new);
        }
        if new == SourceState::Misconfigured {
            self.inner.verified.store(false, Ordering::Release);
            // The connections close at the change (8b.12): the client drops, and a query in flight keeps
            // its own pointer until it ends.
            self.set_client(None);
        }
        prev
    }

    /// Changes the state from `from` to `to` only when the state is still `from`. A verdict of a probe
    /// that came in the meantime wins.
    fn set_state_if(&self, from: SourceState, to: SourceState) {
        let swapped =
            self.inner.state.compare_exchange(from.to_u8(), to.to_u8(), Ordering::AcqRel, Ordering::Acquire).is_ok();
        if swapped && from != to {
            self.inner.state_tx.send_replace(to);
        }
    }

    /// Updates the state after a query (8b.12). A query never changes a `misconfigured` state to another
    /// one: only a probe that passes does.
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
            Err(SourceError::SourceMisconfigured { message, .. }) => {
                let previous = self.set_state(SourceState::Misconfigured);
                // The reason goes in the last probe, so the server writes it in the log line
                // `source.misconfigured`. The viewer gets the text of the error, which is fixed.
                *self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner) = Some(Probe {
                    state: SourceState::Misconfigured,
                    previous,
                    version: None,
                    version_supported: None,
                    detail: Some(message.clone()),
                });
            }
            Err(_) => {}
        }
    }

    fn busy(&self, why: Busy) -> SourceError {
        SourceError::SourceBusy { source: self.inner.config.id.clone(), message: why.message().to_owned() }
    }

    /// The error of a source that is `misconfigured`. The text is fixed. A source with no driver says
    /// that the driver is not in the build.
    fn misconfigured_error(&self) -> SourceError {
        let message = if self.inner.connector.is_none() { NO_DRIVER_MESSAGE } else { VIEWER_MISCONFIGURED };
        SourceError::SourceMisconfigured { source: self.inner.config.id.clone(), message: message.to_owned() }
    }

    /// The error of a source that passed no probe.
    fn not_verified_error(&self) -> SourceError {
        SourceError::SourceUnavailable { source: self.inner.config.id.clone(), message: NOT_VERIFIED.to_owned() }
    }

    fn client_deadline(&self, limits: &EffectiveLimits) -> Duration {
        self.inner.deadline.unwrap_or_else(|| exec::client_deadline(limits))
    }

    /// Runs one query within the limits and gives one frame (7.16c).
    ///
    /// The future is safe to drop. A drop while the query runs kills the cursor, ends the session and
    /// gives the permits back (8b.7).
    pub async fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> Result<Frame, SourceError> {
        let id = &self.inner.config.id;
        let PreparedQuery::MongoQuery { command } = query else {
            // The server sends a query of the kind of the source. A mismatch is a fault of the caller.
            return Err(SourceError::SourceBadResponse {
                source: id.clone(),
                query_id: ctx.query_id,
                message: "The prepared query is not a MongoDB query.".to_owned(),
            });
        };
        // A ceiling of 0 gives an error before the send (8b.8, 8.5).
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
        // The wait in the queue starts at the arrival and ends when the query gets its permits or the
        // admission refuses it (8b.10). The wait for the first probe of an unknown source counts.
        let arrival = Instant::now();
        let deadline = self.inner.admission.deadline();
        match self.state() {
            SourceState::Misconfigured => return Err(self.misconfigured_error()),
            SourceState::Unknown => {
                // A query on an unknown source waits for the first probe, within `queue_wait` (8b.12).
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
        // No probe passed since the start or since the last `misconfigured` verdict: nothing goes to
        // the server (8b.12, finding M2 of the PostgreSQL review).
        if !self.inner.verified.load(Ordering::Acquire) {
            ctx.queue_wait.set(arrival.elapsed());
            return Err(self.not_verified_error());
        }
        let permits = self.inner.admission.acquire(ctx.subject.as_str(), deadline).await;
        ctx.queue_wait.set(arrival.elapsed());
        let permits = permits.map_err(|why| self.busy(why))?;
        let Some(client) = self.client() else {
            // A change to `misconfigured` came between the check and the permits.
            return Err(self.not_verified_error());
        };
        let run = Run {
            source: id,
            query: &command,
            limits: &limits,
            query_id: ctx.query_id,
            log_comment: &ctx.log_comment,
            lists: Lists {
                databases: &self.inner.config.databases,
                collections: &self.inner.config.collections,
                read_preference: self.inner.config.read_preference,
            },
            deadline: self.client_deadline(&limits),
        };
        let result = exec::run(client, Arc::clone(&self.inner.killer), permits, &run).await;
        self.note_result(&result);
        result
    }

    /// Runs the first probe when no result exists. One probe runs at a time.
    async fn first_probe(&self) {
        let _guard = self.inner.probe_lock.lock().await;
        if self.state() == SourceState::Unknown {
            self.run_probe().await;
        }
    }

    /// Runs the probe of 8b.12 and gives its result.
    ///
    /// The probe takes no permit of the admission. One probe runs at a time: a call that waits for
    /// another probe gives the result of that probe. The deadline of the whole probe is
    /// `connect_timeout` plus 10 s. The caller runs it at start and every `PROBE_INTERVAL`. A source with
    /// no driver gives the state `misconfigured` and the reason [`NO_DRIVER_MESSAGE`], and sends nothing.
    pub async fn probe(&self) -> Probe {
        if self.inner.connector.is_none() {
            return Probe {
                state: SourceState::Misconfigured,
                previous: SourceState::Misconfigured,
                version: None,
                version_supported: None,
                detail: Some(NO_DRIVER_MESSAGE.to_owned()),
            };
        }
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
        let (probe, report, client) = match timeout(limit, self.probe_steps(previous)).await {
            Ok(x) => x,
            Err(_) => {
                let timeout =
                    crate::mongo::MongoError::new(crate::mongo::MongoErrorClass::Timeout, "the probe timed out");
                (probe::finish(previous, &ProbeOutcome::from_error(&timeout)), None, None)
            }
        };
        if probe.state == SourceState::Up {
            // The client and the flag come before the state: a query that sees `up` finds both.
            self.set_client(client);
            self.inner.verified.store(true, Ordering::Release);
        }
        self.set_state(probe.state);
        if let Some(report) = report {
            self.keep_notes(&report);
        }
        *self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner) = Some(probe.clone());
        self.inner.probe_seq.fetch_add(1, Ordering::AcqRel);
        probe
    }

    /// Makes a client and runs the checks. It gives the probe, the report when the checks ran to the
    /// end, and the client for a probe that passes.
    async fn probe_steps(
        &self,
        previous: SourceState,
    ) -> (Probe, Option<probe::Report>, Option<Arc<dyn ErasedClient>>) {
        let Some(connector) = &self.inner.connector else {
            // `probe` answers a source with no driver before this point.
            return (
                probe::finish(
                    previous,
                    &ProbeOutcome::Fault { group: crate::mongo::errors::Group::Misconfigured, code: None },
                ),
                None,
                None,
            );
        };
        let options = ConnectOptions::from_config(&self.inner.config);
        // Each probe makes a new client: it checks TLS, the login and the checks again (8b.12).
        let client = match connector.connect(options).await {
            Ok(c) => c,
            Err(e) => return (probe::finish(previous, &ProbeOutcome::from_error(&e)), None, None),
        };
        match probe::run_probe(&*client, &self.inner.config).await {
            Ok(report) => {
                let passed = report.failure.is_none();
                let probe = probe::finish(previous, &ProbeOutcome::Checked(report.clone()));
                (probe, Some(report), passed.then_some(client))
            }
            Err(e) => (probe::finish(previous, &ProbeOutcome::from_error(&e)), None, None),
        }
    }

    /// Keeps the advice of a probe that ran to the end, and queues the notices (8b.12).
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
        if let Some(version) = report.version {
            if probe::version_supported(version) {
                notes.unsupported = None;
            } else if notes.unsupported.as_deref() != Some(report.version_text.as_str()) {
                notes.unsupported = Some(report.version_text.clone());
                notes.versions.push(report.version_text.clone());
            }
        }
        let excess = notes.queue.len().saturating_sub(MAX_NOTICES);
        notes.queue.drain(..excess);
        let excess = notes.versions.len().saturating_sub(MAX_NOTICES);
        notes.versions.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use hune_core::mongo::{MongoQuery, MongoValue, QueryBody, ReadPreference};

    use super::*;
    use crate::config::{MongoHost, MongoTlsMode, MongoTopology};
    use crate::mongo::fake::{FakeMongo, Reply};
    use crate::mongo::iface::{AuthRole, AuthUser, MongoError, MongoErrorClass, Privilege, PrivilegeResource};
    use crate::{AnySource, LogComment, QueryKind, QueueWait, SubjectId, Uuid};

    fn config() -> MongoSourceConfig {
        MongoSourceConfig {
            id: SourceId::parse("events").unwrap(),
            label: "Events".to_owned(),
            topology: MongoTopology::Direct { host: "h".to_owned(), port: 27017 },
            user: "hune_ro".to_owned(),
            password: "secret-password".to_owned(),
            auth_database: "admin".to_owned(),
            databases: vec!["analytics".to_owned()],
            collections: vec!["c".to_owned()],
            read_preference: ReadPreference::Primary,
            tls_mode: MongoTlsMode::Disable,
            tls: None,
            tls_ca_file: None,
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100,
            max_result_bytes: 1 << 20,
            max_query_size: 1 << 20,
            max_concurrent: 4,
            queue_wait: Duration::from_secs(1),
            max_concurrent_per_subject: None,
        }
    }

    fn driverless() -> MongoSource {
        MongoSource::new(config(), ResultMemoryBudget::new(1 << 30)).unwrap()
    }

    fn with_fake(config: MongoSourceConfig) -> (MongoSource, FakeMongo) {
        let fake = FakeMongo::for_config(&config);
        let source = MongoSource::with_connector(config, ResultMemoryBudget::new(1 << 30), fake.connector()).unwrap();
        (source, fake)
    }

    fn query() -> PreparedQuery {
        PreparedQuery::MongoQuery {
            command: MongoQuery {
                collection: "c".to_owned(),
                database: None,
                body: QueryBody::Aggregate(vec![MongoValue::from_extended_json(r#"{"$match":{}}"#).unwrap()]),
            },
        }
    }

    fn limits() -> EffectiveLimits {
        EffectiveLimits {
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100,
            max_result_bytes: 1 << 20,
            max_query_size: 1 << 20,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        }
    }

    fn ctx() -> QueryCtx {
        QueryCtx {
            kind: QueryKind::Panel,
            query_id: Uuid::now_v7(),
            log_comment: LogComment::probe(),
            subject: SubjectId::open(),
            traceparent: None,
            queue_wait: QueueWait::new(),
        }
    }

    // ---- no driver ----

    #[test]
    fn a_source_with_no_driver_is_a_misconfigured_mongodb_source() {
        let s = driverless();
        assert_eq!(s.kind(), SourceKind::MongoDb);
        assert_eq!(s.kind().as_str(), "mongodb");
        assert_eq!(s.state(), SourceState::Misconfigured);
        assert_eq!(*s.watch_state().borrow(), SourceState::Misconfigured);
        assert_eq!(s.label(), "Events");
        assert!(!s.has_client());
        let any = AnySource::from(s);
        assert_eq!(any.kind(), SourceKind::MongoDb);
        assert_eq!(any.id().as_str(), "events");
        assert!(any.last_probe().is_none());
        assert_eq!(any.admission_stats().in_flight, 0);
        assert!(any.take_advice_changes().is_empty());
        assert!(any.take_version_unsupported().is_empty());
    }

    #[tokio::test]
    async fn a_source_with_no_driver_says_that_the_driver_is_not_in_the_build() {
        let any = AnySource::from(driverless());
        let p = any.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(!p.changed());
        assert_eq!(p.detail.as_deref(), Some(NO_DRIVER_MESSAGE));
        match any.query(query(), limits(), ctx()).await {
            Err(SourceError::SourceMisconfigured { message, .. }) => {
                assert!(message.contains("driver is not in this build"))
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(any.state(), SourceState::Misconfigured);
    }

    #[test]
    fn a_value_out_of_range_is_a_setup_error() {
        type Edit = fn(&mut MongoSourceConfig);
        let cases: [(Edit, &str); 11] = [
            (|c| c.user.clear(), "user"),
            (|c| c.databases.clear(), "databases"),
            (|c| c.collections.clear(), "collections"),
            (|c| c.max_concurrent = 0, "max_concurrent"),
            (|c| c.max_concurrent_per_subject = Some(0), "max_concurrent_per_subject"),
            (|c| c.max_result_rows = 0, "max_result_rows"),
            (|c| c.max_result_bytes = 0, "max_result_bytes"),
            (|c| c.max_result_bytes = u64::MAX, "max_result_bytes"),
            (|c| c.max_query_size = 0, "max_query_size"),
            (|c| c.max_execution_time = Duration::from_millis(500), "max_execution_time"),
            (|c| c.connect_timeout = Duration::ZERO, "connect_timeout"),
        ];
        for (edit, key) in cases {
            let mut c = config();
            edit(&mut c);
            let e = MongoSource::new(c, ResultMemoryBudget::new(1 << 30)).unwrap_err();
            assert!(e.message().starts_with(key), "{key}: {}", e.message());
        }
    }

    #[test]
    fn debug_hides_the_password() {
        let (s, _) = with_fake(config());
        let text = format!("{s:?} {:?}", config());
        assert!(!text.contains("secret-password"));
    }

    // ---- the probe (MS11) ----

    #[tokio::test]
    async fn the_account_of_8b13_gives_up_and_a_client() {
        let (s, fake) = with_fake(config());
        assert_eq!(s.state(), SourceState::Unknown);
        let p = s.probe().await;
        assert_eq!(
            (p.state, p.previous, p.version.as_deref(), p.version_supported),
            (SourceState::Up, SourceState::Unknown, Some("7.0.14"), Some(true))
        );
        assert!(p.detail.is_none());
        assert_eq!(s.state(), SourceState::Up);
        assert!(s.has_client());
        assert_eq!(s.last_probe(), Some(p));
        assert_eq!(fake.command_names(), ["hello", "buildInfo", "connectionStatus", "listCollections"]);
        // The options of the connect come from the block, and the password does not reach a log.
        let o = fake.last_connect_options().unwrap();
        assert_eq!((o.user.as_str(), o.auth_database.as_str()), ("hune_ro", "admin"));
        assert!(!format!("{o:?}").contains("secret-password"));
        assert_eq!(*s.watch_state().borrow(), SourceState::Up);
    }

    #[tokio::test]
    async fn each_deviant_user_gives_misconfigured_and_the_check_names_the_reason() {
        type Edit = fn(&FakeMongo);
        let cases: [(Edit, &str); 6] = [
            (|f| f.edit_auth(|a| a.users[0].db = "analytics".to_owned()), "M1: hune_ro@analytics"),
            (
                |f| f.edit_auth(|a| a.users.push(AuthUser { user: "x".to_owned(), db: "admin".to_owned() })),
                "M1: hune_ro@admin, x@admin",
            ),
            (
                |f| {
                    f.edit_auth(|a| a.roles.push(AuthRole { role: "readWrite".to_owned(), db: "analytics".to_owned() }))
                },
                "M2: readWrite@analytics",
            ),
            (|f| f.edit_auth(|a| a.roles.clear()), "M2: missing read@analytics"),
            (
                |f| {
                    f.edit_auth(|a| {
                        a.privileges
                            .push(Privilege { resource: PrivilegeResource::Cluster, actions: vec!["find".to_owned()] })
                    })
                },
                "M3: cluster",
            ),
            (|f| f.edit_auth(|a| a.privileges[0].actions.push("insert".to_owned())), "M3: analytics.*:insert"),
        ];
        for (edit, reason) in cases {
            let (s, fake) = with_fake(config());
            edit(&fake);
            let p = s.probe().await;
            assert_eq!(p.state, SourceState::Misconfigured, "{reason}");
            assert_eq!(p.detail.as_deref(), Some(reason));
            assert_eq!(s.state(), SourceState::Misconfigured);
            assert!(!s.has_client(), "the connections close at the change");
            assert!(s.take_advice_changes().is_empty());
        }
    }

    #[tokio::test]
    async fn the_replica_set_checks_m4() {
        let mut c = config();
        c.topology = MongoTopology::ReplicaSet {
            hosts: vec![
                MongoHost { host: "m1".to_owned(), port: 27017 },
                MongoHost { host: "m2".to_owned(), port: 27017 },
            ],
            replica_set: "rs0".to_owned(),
        };
        let (s, _) = with_fake(c.clone());
        assert_eq!(s.probe().await.state, SourceState::Up);
        let (s, fake) = with_fake(c.clone());
        fake.edit_server_info(|i| i.hosts.push("m3:27017".to_owned()));
        let p = s.probe().await;
        assert_eq!((p.state, p.detail.as_deref()), (SourceState::Misconfigured, Some("M4: m3:27017")));
        let (s, fake) = with_fake(c);
        fake.edit_server_info(|i| i.set_name = Some("other".to_owned()));
        assert_eq!(s.probe().await.detail.as_deref(), Some("M4: set name other"));
    }

    #[tokio::test]
    async fn m0_failures_give_misconfigured_without_the_account_commands() {
        let (s, fake) = with_fake(config());
        fake.edit_server_info(|i| i.max_bson_object_size = 1000);
        let p = s.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(p.detail.as_deref().unwrap().starts_with("M0: "));
        assert_eq!(fake.command_names(), ["hello", "buildInfo"]);
    }

    #[tokio::test]
    async fn advice_writes_one_notice_for_each_change_and_never_changes_the_state() {
        let (s, fake) = with_fake(config());
        fake.set_collections("analytics", &["other", "secret"]);
        let p = s.probe().await;
        assert_eq!(p.state, SourceState::Up);
        let changes = s.take_advice_changes();
        assert_eq!(changes.len(), 2);
        assert_eq!((changes[0].check, changes[0].names.clone()), ("A1", vec!["c".to_owned()]));
        assert_eq!(
            (changes[1].check, changes[1].names.clone()),
            ("A2", vec!["analytics.other".to_owned(), "analytics.secret".to_owned()])
        );
        // The same result gives no notice.
        s.probe().await;
        assert!(s.take_advice_changes().is_empty());
        // A new name is a change. One check only.
        fake.set_collections("analytics", &["c", "other", "secret", "third"]);
        s.probe().await;
        let changes = s.take_advice_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].check, "A2");
        assert_eq!(changes[0].names.len(), 3);
        // A probe that failed a mandatory check does not reset the advice.
        fake.edit_auth(|a| a.roles.clear());
        assert_eq!(s.probe().await.state, SourceState::Misconfigured);
        fake.edit_auth(|a| a.roles.push(AuthRole { role: "read".to_owned(), db: "analytics".to_owned() }));
        s.probe().await;
        assert!(s.take_advice_changes().is_empty());
        // The advice that closes and comes back is a change.
        fake.set_collections("analytics", &["c"]);
        s.probe().await;
        fake.set_collections("analytics", &["c", "zzz"]);
        s.probe().await;
        assert_eq!(s.take_advice_changes().len(), 1);
        assert_eq!(s.state(), SourceState::Up);
    }

    #[tokio::test]
    async fn an_old_version_gives_one_notice_for_each_change_and_the_queries_still_run() {
        let (s, fake) = with_fake(config());
        fake.edit_server_info(|i| i.version = "6.0.14".to_owned());
        let p = s.probe().await;
        assert_eq!((p.state, p.version_supported), (SourceState::Up, Some(false)));
        assert_eq!(s.take_version_unsupported(), vec!["6.0.14".to_owned()]);
        s.probe().await;
        assert!(s.take_version_unsupported().is_empty());
        fake.edit_server_info(|i| i.version = "5.0.1".to_owned());
        s.probe().await;
        assert_eq!(s.take_version_unsupported(), vec!["5.0.1".to_owned()]);
        // A supported version resets: a later downgrade is a change again.
        fake.edit_server_info(|i| i.version = "8.0.1".to_owned());
        s.probe().await;
        fake.edit_server_info(|i| i.version = "6.0.14".to_owned());
        s.probe().await;
        assert_eq!(s.take_version_unsupported().len(), 1);
        // The queries still run.
        fake.push_read(Reply::docs(&[r#"{"n":1}"#]));
        assert_eq!(s.query(query(), limits(), ctx()).await.unwrap().rows, 1);
    }

    #[tokio::test]
    async fn the_notice_queues_are_bounded() {
        let (s, fake) = with_fake(config());
        for i in 0..200 {
            fake.edit_server_info(|info| info.version = format!("4.{i}.0"));
            s.probe().await;
        }
        assert_eq!(s.take_version_unsupported().len(), MAX_NOTICES);
    }

    #[tokio::test]
    async fn a_source_that_cannot_connect_is_down_and_a_query_gets_a_retryable_error() {
        let (s, fake) = with_fake(config());
        fake.fail_every_connect(Some(MongoError::new(MongoErrorClass::Network, "connection refused by 10.1.2.3")));
        let p = s.probe().await;
        assert_eq!((p.state, p.previous), (SourceState::Down, SourceState::Unknown));
        assert!(!p.detail.as_deref().unwrap().contains("10.1.2.3"));
        assert_eq!(s.state(), SourceState::Down);
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
        // Nothing went to the server.
        assert!(fake.commands().is_empty());
        // The server comes back: the next probe gives `up`.
        fake.fail_every_connect(None);
        assert_eq!(s.probe().await.state, SourceState::Up);
    }

    #[tokio::test]
    async fn a_login_fault_or_a_failed_certificate_gives_misconfigured_and_a_link_fault_gives_down() {
        let (s, fake) = with_fake(config());
        fake.fail_every_connect(Some(MongoError::server(18, "AuthenticationFailed", "bad credentials for hune_ro")));
        let p = s.probe().await;
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(!p.detail.as_deref().unwrap().contains("hune_ro"));
        let (s, fake) = with_fake(config());
        let mut cert = MongoError::new(MongoErrorClass::Tls, "certificate verify failed");
        cert.code_name = Some(crate::mongo::iface::CERTIFICATE_CODE_NAME.to_owned());
        fake.fail_every_connect(Some(cert));
        assert_eq!(s.probe().await.state, SourceState::Misconfigured);
        let (s, fake) = with_fake(config());
        fake.fail_every_connect(Some(MongoError::new(MongoErrorClass::Tls, "handshake eof")));
        assert_eq!(s.probe().await.state, SourceState::Down);
        let (s, fake) = with_fake(config());
        fake.fail_auth(MongoError::server(13, "Unauthorized", "not authorized on admin"));
        assert_eq!(s.probe().await.state, SourceState::Misconfigured);
        let (s, fake) = with_fake(config());
        fake.fail_server_info(MongoError::new(MongoErrorClass::Timeout, "slow"));
        assert_eq!(s.probe().await.state, SourceState::Down);
    }

    #[tokio::test]
    async fn a_fault_of_the_probe_never_moves_a_misconfigured_source_out_of_the_state() {
        let (s, fake) = with_fake(config());
        fake.edit_auth(|a| a.roles.clear());
        assert_eq!(s.probe().await.state, SourceState::Misconfigured);
        fake.fail_every_connect(Some(MongoError::new(MongoErrorClass::Network, "refused")));
        let p = s.probe().await;
        assert_eq!((p.state, p.changed()), (SourceState::Misconfigured, false));
        assert_eq!(s.state(), SourceState::Misconfigured);
        fake.fail_every_connect(None);
        // Fixed account: only a probe that passes gives `up`.
        fake.edit_auth(|a| a.roles.push(AuthRole { role: "read".to_owned(), db: "analytics".to_owned() }));
        assert_eq!(s.probe().await.state, SourceState::Up);
        assert!(s.has_client());
    }

    #[tokio::test]
    async fn the_probe_that_runs_at_the_same_time_shares_its_result() {
        let (s, fake) = with_fake(config());
        fake.delay_server_info(Duration::from_millis(60));
        let (a, b) = tokio::join!(s.probe(), s.probe());
        assert_eq!(a.state, SourceState::Up);
        assert_eq!(a, b);
        assert_eq!(fake.connects(), 1, "one probe ran");
    }

    // ---- the state gates the query (8b.12) ----

    #[tokio::test]
    async fn a_query_on_an_unknown_source_runs_one_probe_and_then_goes_to_the_server() {
        let (s, fake) = with_fake(config());
        fake.push_read(Reply::docs(&[r#"{"n":1}"#]));
        let c = ctx();
        let wait = c.queue_wait.clone();
        let frame = s.query(query(), limits(), c).await.unwrap();
        assert_eq!(frame.rows, 1);
        assert_eq!(s.state(), SourceState::Up);
        assert_eq!(fake.connects(), 1);
        assert!(wait.get().is_some());
        let names = fake.command_names();
        assert_eq!(names.first(), Some(&"hello"));
        assert!(names.contains(&"aggregate"));
        // A second query runs no probe.
        fake.push_read(Reply::docs(&[]));
        s.query(query(), limits(), ctx()).await.unwrap();
        assert_eq!(fake.connects(), 1);
    }

    #[tokio::test]
    async fn a_misconfigured_source_sends_nothing_until_a_probe_passes() {
        let (s, fake) = with_fake(config());
        fake.edit_auth(|a| a.users[0].user = "root".to_owned());
        assert_eq!(s.probe().await.state, SourceState::Misconfigured);
        let sent = fake.commands().len();
        for _ in 0..3 {
            let e = s.query(query(), limits(), ctx()).await.unwrap_err();
            assert_eq!((e.code(), e.http_status()), ("source_misconfigured", 502));
            // The text is fixed: the reason is in the probe only.
            assert!(!e.message().contains("root"), "{}", e.message());
            assert_eq!(e.members().len(), 1);
        }
        assert_eq!(fake.commands().len(), sent, "no command after the verdict");
        assert!(s.last_probe().unwrap().detail.unwrap().starts_with("M1"));
        fake.edit_auth(|a| a.users[0].user = "hune_ro".to_owned());
        assert_eq!(s.probe().await.state, SourceState::Up);
        fake.push_read(Reply::docs(&[]));
        assert!(s.query(query(), limits(), ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn a_source_that_passed_no_probe_sends_nothing_to_the_server() {
        // The first probe cannot connect: the source is down and has never been verified.
        let (s, fake) = with_fake(config());
        fake.fail_every_connect(Some(MongoError::new(MongoErrorClass::Network, "x")));
        s.probe().await;
        fake.fail_every_connect(None);
        let before = fake.commands().len();
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert_eq!(e.code(), "source_unavailable");
        assert_eq!(fake.commands().len(), before);
        // The next probe that passes opens the gate.
        assert_eq!(s.probe().await.state, SourceState::Up);
        fake.push_read(Reply::docs(&[]));
        assert!(s.query(query(), limits(), ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn a_query_never_moves_a_misconfigured_source_to_another_state() {
        let (s, fake) = with_fake(config());
        assert_eq!(s.probe().await.state, SourceState::Up);
        // The server answers a login code: the source becomes misconfigured and drops its client.
        fake.push_read(Reply::error(MongoError::server(18, "AuthenticationFailed", "x")));
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert_eq!(e.code(), "source_misconfigured");
        assert_eq!(s.state(), SourceState::Misconfigured);
        assert!(!s.has_client());
        // The reason stays in the last probe, with the code and no text of the server.
        let last = s.last_probe().unwrap();
        assert_eq!((last.state, last.previous), (SourceState::Misconfigured, SourceState::Up));
        assert!(last.detail.unwrap().contains("code 18"));
        // A later query fails at once: no success of a query can restore the state.
        let before = fake.commands().len();
        assert_eq!(s.query(query(), limits(), ctx()).await.unwrap_err().code(), "source_misconfigured");
        assert_eq!(fake.commands().len(), before);
        s.set_state_if(SourceState::Down, SourceState::Up);
        assert_eq!(s.state(), SourceState::Misconfigured);
    }

    #[tokio::test]
    async fn an_unavailable_answer_sets_down_and_a_success_sets_up() {
        let (s, fake) = with_fake(config());
        s.probe().await;
        fake.push_read(Reply::error(MongoError::new(MongoErrorClass::Network, "reset")));
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
        assert_eq!(s.state(), SourceState::Down);
        // A query on a down source still goes to the server (8b.12).
        fake.push_read(Reply::docs(&[r#"{"n":1}"#]));
        assert!(s.query(query(), limits(), ctx()).await.is_ok());
        assert_eq!(s.state(), SourceState::Up);
        // Another error does not change the state.
        fake.push_read(Reply::error(MongoError::server(168, "InvalidPipelineOperator", "x")));
        assert_eq!(s.query(query(), limits(), ctx()).await.unwrap_err().code(), "query_error");
        assert_eq!(s.state(), SourceState::Up);
    }

    #[tokio::test]
    async fn a_query_of_another_kind_is_a_bad_response() {
        let (s, _) = with_fake(config());
        let e = s.query(PreparedQuery::PostgresSql { sql: "select 1".to_owned() }, limits(), ctx()).await.unwrap_err();
        assert_eq!(e.code(), "source_bad_response");
    }

    #[tokio::test]
    async fn a_ceiling_of_zero_is_refused_before_anything_is_sent() {
        let (s, fake) = with_fake(config());
        let mut l = limits();
        l.max_result_rows = 0;
        let e = s.query(query(), l, ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_rows"), Some(0)));
        let mut l = limits();
        l.max_execution_time = Duration::ZERO;
        let e = s.query(query(), l, ctx()).await.unwrap_err();
        assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_duration"), Some(0)));
        assert_eq!(fake.connects(), 0);
        assert!(fake.commands().is_empty());
    }

    // ---- admission (8b.10) ----

    async fn up_source(config: MongoSourceConfig) -> (MongoSource, FakeMongo) {
        let (s, fake) = with_fake(config);
        assert_eq!(s.probe().await.state, SourceState::Up);
        (s, fake)
    }

    #[tokio::test]
    async fn the_queue_wait_applies_and_a_query_over_the_limit_gets_source_busy() {
        let mut c = config();
        c.max_concurrent = 1;
        c.queue_wait = Duration::from_millis(80);
        let (s, fake) = up_source(c).await;
        fake.push_read(Reply::docs(&[]).after(Duration::from_millis(400)));
        let first = {
            let s = s.clone();
            tokio::spawn(async move { s.query(query(), limits(), ctx()).await })
        };
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(s.admission_stats().in_flight, 1);
        let c2 = ctx();
        let wait = c2.queue_wait.clone();
        let e = s.query(query(), limits(), c2).await.unwrap_err();
        assert_eq!(
            (e.code(), e.http_status(), e.retryable(), e.retry_after_secs()),
            ("source_busy", 503, true, Some(1))
        );
        let waited = wait.get().unwrap();
        assert!(waited >= Duration::from_millis(60) && waited < Duration::from_millis(350), "{waited:?}");
        assert!(first.await.unwrap().is_ok());
        assert_eq!(s.admission_stats().in_flight, 0);
        // The refused query sent nothing: one aggregate only.
        assert_eq!(fake.command_names().iter().filter(|n| **n == "aggregate").count(), 1);
    }

    #[tokio::test]
    async fn the_limit_of_one_subject_is_separate_from_the_limit_of_the_source() {
        let mut c = config();
        c.max_concurrent = 4;
        c.max_concurrent_per_subject = Some(1);
        c.queue_wait = Duration::from_millis(60);
        let (s, fake) = up_source(c).await;
        fake.push_read(Reply::docs(&[]).after(Duration::from_millis(300)));
        let alice = |s: MongoSource| async move {
            let mut c = ctx();
            c.subject = SubjectId::new("alice");
            s.query(query(), limits(), c).await
        };
        let first = tokio::spawn(alice(s.clone()));
        tokio::time::sleep(Duration::from_millis(40)).await;
        // The same subject waits and gets `source_busy`.
        assert_eq!(alice(s.clone()).await.unwrap_err().code(), "source_busy");
        // Another subject passes.
        let mut c = ctx();
        c.subject = SubjectId::new("bob");
        assert!(s.query(query(), limits(), c).await.is_ok());
        assert!(first.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn the_memory_budget_holds_two_times_max_result_bytes_for_each_query() {
        let mut c = config();
        c.max_result_bytes = 1000;
        c.queue_wait = Duration::from_millis(60);
        let fake = FakeMongo::for_config(&c);
        // The budget holds one reservation of 2000 bytes.
        let s = MongoSource::with_connector(c, ResultMemoryBudget::new(2000), fake.connector()).unwrap();
        s.probe().await;
        fake.push_read(Reply::docs(&[]).after(Duration::from_millis(300)));
        let first = {
            let s = s.clone();
            tokio::spawn(async move { s.query(query(), limits(), ctx()).await })
        };
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(s.admission_stats().memory_in_use, 2000);
        assert_eq!(s.query(query(), limits(), ctx()).await.unwrap_err().code(), "source_busy");
        assert!(first.await.unwrap().is_ok());
        assert_eq!(s.admission_stats().memory_in_use, 0);
        // A budget below one reservation can never hold a query.
        let mut c = config();
        c.max_result_bytes = 1000;
        let fake = FakeMongo::for_config(&c);
        let s = MongoSource::with_connector(c, ResultMemoryBudget::new(1999), fake.connector()).unwrap();
        s.probe().await;
        assert_eq!(s.query(query(), limits(), ctx()).await.unwrap_err().code(), "source_busy");
    }

    // ---- the client leaves, the deadline, no log (8b.7) ----

    #[tokio::test]
    async fn the_client_deadline_of_the_source_is_the_limit_plus_five_seconds() {
        let (s, _) = up_source(config()).await;
        assert_eq!(s.client_deadline(&limits()), Duration::from_secs(65));
        let mut l = limits();
        l.max_execution_time = Duration::from_millis(2500);
        assert_eq!(s.client_deadline(&l), Duration::from_secs(8));
    }

    #[tokio::test]
    async fn a_dropped_query_kills_its_cursor_and_gives_its_permits_back() {
        let (s, fake) = up_source(config()).await;
        fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 61));
        fake.push_get_more(Reply::docs(&[]).after(Duration::from_secs(30)));
        let task = {
            let s = s.clone();
            tokio::spawn(async move { s.query(query(), limits(), ctx()).await })
        };
        for _ in 0..100 {
            if s.admission_stats().in_flight == 1 && fake.command_names().contains(&"getMore") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        task.abort();
        let _ = task.await;
        for _ in 0..200 {
            if s.admission_stats().in_flight == 0 && !fake.finished().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(fake.killed(), vec![61]);
        assert_eq!(s.admission_stats().in_flight, 0);
        // The drop is not an error of the source: the state stays `up`.
        assert_eq!(s.state(), SourceState::Up);
    }

    #[tokio::test]
    async fn the_client_deadline_of_the_source_gives_query_timeout() {
        let fake = FakeMongo::for_config(&config());
        let budget = ResultMemoryBudget::new(1 << 30);
        let s = MongoSource::build(config(), budget, Some(fake.connector()), Some(Duration::from_millis(120))).unwrap();
        s.probe().await;
        fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 71));
        fake.push_get_more(Reply::docs(&[]).after(Duration::from_secs(30)));
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
        assert_eq!(fake.killed(), vec![71]);
        assert_eq!(s.admission_stats().in_flight, 0);
        // A timeout is not a change of state.
        assert_eq!(s.state(), SourceState::Up);
    }

    #[tokio::test]
    async fn a_guard_refusal_is_an_internal_error_and_sends_nothing() {
        let (s, fake) = up_source(config()).await;
        let before = fake.commands().len();
        let bad = PreparedQuery::MongoQuery {
            command: MongoQuery {
                collection: "c".to_owned(),
                database: None,
                body: QueryBody::Aggregate(vec![MongoValue::from_extended_json(r#"{"$out":"x"}"#).unwrap()]),
            },
        };
        let e = s.query(bad, limits(), ctx()).await.unwrap_err();
        assert_eq!((e.code(), e.http_status()), ("internal_error", 500));
        assert_eq!(fake.commands().len(), before);
        let other = PreparedQuery::MongoQuery {
            command: MongoQuery { collection: "secret".to_owned(), database: None, body: QueryBody::Aggregate(vec![]) },
        };
        assert_eq!(s.query(other, limits(), ctx()).await.unwrap_err().code(), "internal_error");
        assert_eq!(fake.commands().len(), before);
        // An internal error does not change the state.
        assert_eq!(s.state(), SourceState::Up);
    }

    #[tokio::test]
    async fn the_attribution_comment_and_the_options_reach_the_read() {
        let (s, fake) = up_source(config()).await;
        fake.push_read(Reply::docs(&[]));
        let mut c = ctx();
        c.log_comment = LogComment {
            kind: crate::types::LogKind::Panel,
            uid: Some("node-load".to_owned()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r1".to_owned()),
            subject: Some("alice".to_owned()),
        };
        let qid = c.query_id;
        s.query(query(), limits(), c).await.unwrap();
        let req = fake.requests().pop().unwrap();
        assert_eq!(req.comment.get("query_id"), Some(&MongoValue::String(qid.to_string())));
        assert_eq!(req.comment.get("uid"), Some(&MongoValue::String("node-load".to_owned())));
        assert_eq!(req.comment.get("subject"), Some(&MongoValue::String("alice".to_owned())));
        assert_eq!((req.batch_size, req.max_time_ms, req.read_preference), (101, 60_000, ReadPreference::Primary));
    }

    #[tokio::test]
    async fn the_source_keeps_the_words_of_the_server_out_of_its_errors() {
        let (s, fake) = up_source(config()).await;
        fake.push_read(Reply::error(MongoError::new(MongoErrorClass::Network, "reset by 10.9.9.9 token=abc")));
        let e = s.query(query(), limits(), ctx()).await.unwrap_err();
        assert!(!e.message().contains("abc") && !e.message().contains("10.9.9.9"));
    }
}
