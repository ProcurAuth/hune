// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ClickHouse source. It has the HTTP client of 8.3 and 8.4 and the pool of 8.7.
//! It has the admission of 8.8, the deadlines and the cancellation of 8.9, and the probe of 8.11.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use http::StatusCode;
use http::header::{CONNECTION, CONTENT_ENCODING};
use http_body_util::BodyExt;
use hune_core::frame::{Frame, FrameStats, FrameValue};
use hune_core::ids::SourceId;
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::{Instant, timeout_at};

use crate::admission::{Admission, AdmissionStats, Busy};
use crate::answer::{AnswerParser, ERROR_BODY_CAP};
use crate::budget::ResultMemoryBudget;
use crate::config::ClickHouseSourceConfig;
use crate::conn::{Conn, ConnectError, Connector, IDLE_TIMEOUT, Pool};
use crate::error::{Ceiling, ErrorCtx, SourceError, SourceSetupError, exec_seconds};
use crate::grants::check_grants;
use crate::request::{RequestTemplate, Settings, path_and_query};
use crate::types::{
    EffectiveLimits, LogComment, PreparedQuery, Probe, QueryCtx, SourceKind, SourceState, TraceParent, Uuid,
};

/// The time between two probes (8.11). The caller of [`ClickHouseSource::probe`] keeps this clock.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(30);

/// The oldest ClickHouse release that Hune supports (8.13): major and minor number.
pub const OLDEST_SUPPORTED_VERSION: (u32, u32) = (26, 3);

/// The wait after the effective `max_execution_time` before the client deadline (8.9).
const DEADLINE_MARGIN: Duration = Duration::from_secs(5);

/// The limit that the probe keeps for its two statements.
const PROBE_STATEMENTS: [&str; 2] = ["SELECT version(), getSetting('readonly')", "SHOW GRANTS"];

struct Inner {
    id: SourceId,
    label: String,
    user: String,
    database: String,
    max_execution_time: Duration,
    max_result_rows: u64,
    max_result_bytes: u64,
    max_query_size: u64,
    connect_timeout: Duration,
    template: RequestTemplate,
    connector: Connector,
    pool: Pool,
    admission: Admission,
    state: AtomicU8,
    state_tx: watch::Sender<SourceState>,
    probe_lock: AsyncMutex<()>,
    probe_seq: AtomicU64,
    last_probe: Mutex<Option<Probe>>,
}

/// One ClickHouse server on its HTTP interface. A clone shares the pool, the admission and the state.
#[derive(Clone)]
pub struct ClickHouseSource {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for ClickHouseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseSource").field("id", &self.inner.id).field("state", &self.state()).finish()
    }
}

fn check(cond: bool, message: &str) -> Result<(), SourceSetupError> {
    if cond { Ok(()) } else { Err(SourceSetupError::new(message)) }
}

impl ClickHouseSource {
    /// Builds the source. It does the CPU work only (the TLS configuration): it sends nothing on
    /// the network (3.5, step 5).
    ///
    /// # Errors
    /// A value is out of range, or `tls` does not match `https`. The CA text has no certificate,
    /// or a name or the password cannot go in an HTTP header.
    pub fn new(cfg: ClickHouseSourceConfig, budget: ResultMemoryBudget) -> Result<Self, SourceSetupError> {
        check(!cfg.host.is_empty(), "host: the value is empty")?;
        check(cfg.port != 0, "port: the value is 0")?;
        check(!cfg.user.is_empty(), "user: the value is empty")?;
        check(cfg.max_concurrent >= 1, "max_concurrent: the value is below 1")?;
        check(cfg.max_concurrent_per_subject != Some(0), "max_concurrent_per_subject: the value is 0")?;
        check(cfg.max_result_rows >= 1, "max_result_rows: the value is below 1")?;
        check(cfg.max_result_bytes >= 1, "max_result_bytes: the value is below 1")?;
        check(
            cfg.max_result_bytes.saturating_mul(2) <= crate::budget::MAX_RESERVATION,
            "max_result_bytes: twice the value is above 4294967295",
        )?;
        check(cfg.max_query_size >= 1, "max_query_size: the value is below 1")?;
        check(cfg.max_execution_time >= Duration::from_secs(1), "max_execution_time: the value is below 1s")?;
        check(!cfg.connect_timeout.is_zero(), "connect_timeout: the value is 0")?;
        let template = RequestTemplate::new(&cfg)?;
        let connector = Connector::new(&cfg)?;
        let admission = Admission::new(
            cfg.max_concurrent,
            cfg.max_concurrent_per_subject,
            cfg.queue_wait,
            budget,
            cfg.max_result_bytes * 2,
        );
        let (state_tx, _) = watch::channel(SourceState::Unknown);
        Ok(ClickHouseSource {
            inner: Arc::new(Inner {
                id: cfg.id,
                label: cfg.label,
                user: cfg.user,
                database: cfg.database,
                max_execution_time: cfg.max_execution_time,
                max_result_rows: cfg.max_result_rows,
                max_result_bytes: cfg.max_result_bytes,
                max_query_size: cfg.max_query_size,
                connect_timeout: cfg.connect_timeout,
                template,
                connector,
                pool: Pool::new(cfg.max_concurrent as usize, IDLE_TIMEOUT),
                admission,
                state: AtomicU8::new(SourceState::Unknown.to_u8()),
                state_tx,
                probe_lock: AsyncMutex::new(()),
                probe_seq: AtomicU64::new(0),
                last_probe: Mutex::new(None),
            }),
        })
    }

    /// Gives the id of the block.
    pub fn id(&self) -> &SourceId {
        &self.inner.id
    }

    /// Gives the label of the block.
    pub fn label(&self) -> &str {
        &self.inner.label
    }

    /// Gives the kind of the source.
    pub fn kind(&self) -> SourceKind {
        SourceKind::ClickHouse
    }

    /// Gives the state of 8.11.
    pub fn state(&self) -> SourceState {
        SourceState::from_u8(self.inner.state.load(Ordering::Acquire))
    }

    /// Gives a receiver that sees each change of the state. The server uses it for the log lines
    /// `source.up`, `source.down` and `source.misconfigured` and for the metric `hune_source_up`.
    pub fn watch_state(&self) -> watch::Receiver<SourceState> {
        self.inner.state_tx.subscribe()
    }

    /// Gives the counters of the admission, for the metrics of section 14.
    pub fn admission_stats(&self) -> AdmissionStats {
        self.inner.admission.stats()
    }

    /// Gives the result of the last probe, or `None` before the first one.
    pub fn last_probe(&self) -> Option<Probe> {
        self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Gives the number of idle connections in the pool.
    pub fn idle_connections(&self) -> usize {
        self.inner.pool.idle_count()
    }

    fn set_state(&self, new: SourceState) -> SourceState {
        let prev = SourceState::from_u8(self.inner.state.swap(new.to_u8(), Ordering::AcqRel));
        if prev != new {
            self.inner.state_tx.send_replace(new);
        }
        prev
    }

    /// Updates the state after a query (8.11).
    fn note_result(&self, result: &Result<Frame, SourceError>) {
        match result {
            Ok(_) => {
                if self.state() == SourceState::Down {
                    self.set_state(SourceState::Up);
                }
            }
            Err(SourceError::SourceUnavailable { .. }) => {
                if self.state() != SourceState::Misconfigured {
                    self.set_state(SourceState::Down);
                }
            }
            Err(SourceError::SourceMisconfigured { .. }) => {
                self.set_state(SourceState::Misconfigured);
            }
            Err(_) => {}
        }
    }

    fn busy(&self, why: Busy) -> SourceError {
        SourceError::SourceBusy { source: self.inner.id.clone(), message: why.message().to_owned() }
    }

    fn misconfigured_error(&self) -> SourceError {
        let detail = self
            .last_probe()
            .and_then(|p| p.detail)
            .unwrap_or_else(|| "The account or the certificate breaks the contract of the source.".to_owned());
        SourceError::SourceMisconfigured { source: self.inner.id.clone(), message: detail }
    }

    /// Runs one query within the limits and gives one frame (7.16).
    ///
    /// The future is safe to drop. A drop closes the connection to ClickHouse, and ClickHouse stops
    /// the query (8.9). The future never sends `KILL QUERY`.
    pub async fn query(
        &self,
        query: PreparedQuery,
        limits: EffectiveLimits,
        ctx: QueryCtx,
    ) -> Result<Frame, SourceError> {
        let PreparedQuery::ClickHouseSql { sql, params } = query else {
            // The server sends a query of the kind of the source. A mismatch is a fault of the caller.
            return Err(SourceError::SourceBadResponse {
                source: self.inner.id.clone(),
                query_id: Uuid::now_v7(),
                message: "The prepared query is not a ClickHouse query.".to_owned(),
            });
        };
        // A ceiling of 0 gives an error before the send, because ClickHouse reads 0 as "no limit" (8.5).
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
        // The wait in the queue starts at the arrival and ends when the query gets its permits or
        // the admission refuses it (8.8). The wait for the first probe of an unknown source counts.
        let arrival = Instant::now();
        let deadline = self.inner.admission.deadline();
        match self.state() {
            SourceState::Misconfigured => return Err(self.misconfigured_error()),
            SourceState::Unknown => {
                // A query on an unknown source waits for the first probe, within `queue_wait` (8.11).
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
        let permits = self.inner.admission.acquire(ctx.subject.as_str(), deadline).await;
        ctx.queue_wait.set(arrival.elapsed());
        let _permits = permits.map_err(|why| self.busy(why))?;
        let request_deadline = Duration::from_secs(exec_seconds(&limits)) + DEADLINE_MARGIN;
        let params = params.query_string();
        let result = self
            .execute(Run {
                sql: &sql,
                params: &params,
                limits: &limits,
                query_id: ctx.query_id,
                log_comment: ctx.log_comment.to_json(),
                traceparent: ctx.traceparent.as_ref(),
                deadline: request_deadline,
            })
            .await;
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

    /// Runs the probe of 8.11 and gives its result.
    ///
    /// The probe takes no permit of the admission. One probe runs at a time: a call that waits
    /// for another probe gives the result of that probe. The deadline of each statement is
    /// `connect_timeout` plus 5 s. The caller runs it at start and every [`PROBE_INTERVAL`].
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
        let outcome = self.probe_steps().await;
        self.set_state(outcome.state);
        let probe = Probe {
            state: outcome.state,
            previous,
            version: outcome.version,
            version_supported: outcome.supported,
            detail: outcome.detail,
        };
        *self.inner.last_probe.lock().unwrap_or_else(PoisonError::into_inner) = Some(probe.clone());
        self.inner.probe_seq.fetch_add(1, Ordering::AcqRel);
        probe
    }

    /// Runs one statement of the probe with the limits of the keys.
    async fn probe_statement(&self, sql: &str) -> Result<Frame, SourceError> {
        let limits = EffectiveLimits {
            max_execution_time: self.inner.max_execution_time,
            max_result_rows: self.inner.max_result_rows,
            max_result_bytes: self.inner.max_result_bytes,
            max_query_size: self.inner.max_query_size,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        };
        self.execute(Run {
            sql,
            params: "",
            limits: &limits,
            query_id: Uuid::now_v7(),
            log_comment: LogComment::probe().to_json(),
            traceparent: None,
            deadline: self.inner.connect_timeout + DEADLINE_MARGIN,
        })
        .await
    }

    async fn probe_steps(&self) -> ProbeOutcome {
        let fail = |e: SourceError, version: Option<String>, supported: Option<bool>| -> ProbeOutcome {
            let (state, detail) = match &e {
                SourceError::SourceMisconfigured { message, .. } => (SourceState::Misconfigured, message.clone()),
                SourceError::QueryError { clickhouse_code: 452, message, .. } => (
                    SourceState::Misconfigured,
                    format!("A setting that Hune sends is above a constraint of the settings profile: {message}"),
                ),
                // Code 164: `readonly = 1` refuses the settings of 8.3. The contract needs `readonly = 2`.
                SourceError::QueryError { clickhouse_code: 164, message, .. } => (
                    SourceState::Misconfigured,
                    format!("The account refuses the settings of Hune, so readonly is not 2: {message}"),
                ),
                other => (SourceState::Down, format!("{}: {}", other.code(), other.message())),
            };
            ProbeOutcome { state, version, supported, detail: Some(detail) }
        };
        let frame = match self.probe_statement(PROBE_STATEMENTS[0]).await {
            Ok(f) => f,
            Err(e) => return fail(e, None, None),
        };
        let (version, readonly) = match read_version_row(&frame) {
            Some(v) => v,
            None => {
                let e = SourceError::SourceBadResponse {
                    source: self.inner.id.clone(),
                    query_id: Uuid::parse(&frame.query_id).unwrap_or(Uuid::new_v7(0, [0; 10])),
                    message: "The answer of the version statement has an unexpected shape.".to_owned(),
                };
                return fail(e, None, None);
            }
        };
        let supported = parse_version(&version).map(|v| v >= OLDEST_SUPPORTED_VERSION);
        if readonly != 2 {
            return ProbeOutcome {
                state: SourceState::Misconfigured,
                version: Some(version),
                supported,
                detail: Some(format!(
                    "The account has readonly = {readonly}. The contract of 8.12 needs readonly = 2."
                )),
            };
        }
        let grants = match self.probe_statement(PROBE_STATEMENTS[1]).await {
            Ok(f) => f,
            Err(e) => return fail(e, Some(version), supported),
        };
        let lines: Vec<String> = grants
            .columns
            .first()
            .map(|c| {
                c.values()
                    .filter_map(|v| match v {
                        FrameValue::Text(t) => Some(t.to_owned()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Err(why) = check_grants(&lines, &self.inner.user) {
            return ProbeOutcome {
                state: SourceState::Misconfigured,
                version: Some(version),
                supported,
                detail: Some(format!("The account breaks the contract of 8.12: {why}")),
            };
        }
        ProbeOutcome { state: SourceState::Up, version: Some(version), supported, detail: None }
    }

    /// Takes an idle connection, or opens a new one.
    async fn checkout(&self, fresh_only: bool) -> Result<(Conn, bool), ConnectError> {
        if !fresh_only && let Some(c) = self.inner.pool.take() {
            return Ok((c, true));
        }
        Ok((self.inner.connector.connect().await?, false))
    }

    /// Sends one request and reads the answer into a frame (8.3, 8.4, 8.5, 8.9).
    async fn execute(&self, run: Run<'_>) -> Result<Frame, SourceError> {
        let inner = &*self.inner;
        let ectx = ErrorCtx { source: &inner.id, query_id: run.query_id, limits: run.limits };
        let path = path_and_query(&Settings {
            database: &inner.database,
            query_id: run.query_id,
            max_execution_time: exec_seconds(run.limits),
            max_result_rows: run.limits.max_result_rows,
            max_result_bytes: run.limits.max_result_bytes,
            max_query_size: run.limits.max_query_size,
            log_comment: &run.log_comment,
            params: run.params,
        });
        let connect_error = |e: ConnectError| match e {
            ConnectError::Unavailable(m) => ectx.unavailable(m),
            ConnectError::Misconfigured(m) => ectx.misconfigured(m),
        };
        // A request goes again one time only, and only when a reused connection failed before
        // Hune wrote the request (8.7).
        let mut retried = false;
        let mut fresh_only = false;
        let (conn, response, started, deadline) = loop {
            let (mut conn, reused) = self.checkout(fresh_only).await.map_err(connect_error)?;
            if reused && conn.sender.ready().await.is_err() {
                continue;
            }
            let request = inner
                .template
                .build(&path, run.sql, run.traceparent)
                .map_err(|_| ectx.bad_response("Hune could not build the request."))?;
            let started = Instant::now();
            let deadline = started + run.deadline;
            match timeout_at(deadline, conn.sender.try_send_request(request)).await {
                Err(_) => return Err(ectx.timeout()),
                Ok(Ok(response)) => break (conn, response, started, deadline),
                Ok(Err(e)) => {
                    if reused && !retried && e.message().is_some() {
                        retried = true;
                        fresh_only = true;
                        continue;
                    }
                    return Err(ectx.unavailable("The connection to the source broke before the answer."));
                }
            }
        };
        let (parts, mut body) = response.into_parts();
        let exception_header = parts.headers.get("x-clickhouse-exception-code");
        if parts.status != StatusCode::OK || exception_header.is_some() {
            let text = read_error_text(&mut body, deadline).await;
            let code = exception_header.and_then(|h| h.to_str().ok()).and_then(|s| s.trim().parse::<i64>().ok());
            return Err(match (code, exception_header.is_some()) {
                (Some(c), _) => ectx.map_clickhouse(c, &text),
                (None, true) => ectx.bad_response("The header X-ClickHouse-Exception-Code is not an integer."),
                // A proxy in front of ClickHouse refused a long request line or long headers: no change of the
                // state of the source (8.5, 7.6a).
                (None, false) if matches!(parts.status.as_u16(), 414 | 431) => {
                    ectx.proxy_refused(parts.status.as_u16())
                }
                (None, false) => {
                    ectx.unavailable(format!("The source answered with the HTTP status {}.", parts.status.as_u16()))
                }
            });
        }
        let id_text = run.query_id.to_string();
        let header_id = parts.headers.get("x-clickhouse-query-id").and_then(|h| h.to_str().ok());
        if header_id != Some(id_text.as_str()) {
            return Err(ectx.bad_response("The header X-ClickHouse-Query-Id differs from the query id that Hune sent."));
        }
        if let Some(enc) = parts.headers.get(CONTENT_ENCODING)
            && !enc.as_bytes().eq_ignore_ascii_case(b"identity")
        {
            return Err(ectx.bad_response("The answer has a content encoding that Hune did not ask for."));
        }
        let summary = parts.headers.get("x-clickhouse-summary").and_then(|h| h.to_str().ok()).map(parse_summary);
        let mut parser = AnswerParser::new(ectx);
        loop {
            match timeout_at(deadline, body.frame()).await {
                Err(_) => return Err(ectx.timeout()),
                Ok(None) => break,
                Ok(Some(Err(_))) => {
                    return Err(ectx.unavailable("The connection to the source broke while it sent the answer."));
                }
                Ok(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        parser.feed(&data)?;
                    }
                }
            }
        }
        let (read_rows, read_bytes, result_bytes) = summary.unwrap_or_default();
        let stats = FrameStats {
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            read_rows,
            read_bytes,
            result_bytes,
        };
        let frame = parser.finish(id_text, stats)?;
        let closes = parts
            .headers
            .get(CONNECTION)
            .is_some_and(|h| h.as_bytes().windows(5).any(|w| w.eq_ignore_ascii_case(b"close")));
        if !closes {
            inner.pool.put(conn);
        }
        Ok(frame)
    }
}

/// The parameters of one request.
struct Run<'a> {
    sql: &'a str,
    /// The pairs of the query parameters of 7.6a, percent-encoded, or the empty text.
    params: &'a str,
    limits: &'a EffectiveLimits,
    query_id: Uuid,
    log_comment: String,
    traceparent: Option<&'a TraceParent>,
    /// The time from the send to the end of the answer.
    deadline: Duration,
}

struct ProbeOutcome {
    state: SourceState,
    version: Option<String>,
    supported: Option<bool>,
    detail: Option<String>,
}

/// Reads the text of an error answer: at most 16 KiB, within the deadline.
async fn read_error_text(body: &mut hyper::body::Incoming, deadline: Instant) -> String {
    let mut buf: Vec<u8> = Vec::new();
    while buf.len() < ERROR_BODY_CAP {
        match timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Ok(data) = frame.into_data() {
                    let room = ERROR_BODY_CAP - buf.len();
                    buf.extend_from_slice(&data[..data.len().min(room)]);
                }
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Reads `read_rows`, `read_bytes` and `result_bytes` of `X-ClickHouse-Summary` (8.4).
pub(crate) fn parse_summary(text: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return (None, None, None);
    };
    let get = |name: &str| {
        let m = v.get(name)?;
        m.as_u64().or_else(|| m.as_str().and_then(|s| s.trim().parse().ok()))
    };
    (get("read_rows"), get("read_bytes"), get("result_bytes"))
}

/// Reads the answer of `SELECT version(), getSetting('readonly')`.
fn read_version_row(frame: &Frame) -> Option<(String, u64)> {
    if frame.rows != 1 || frame.columns.len() != 2 {
        return None;
    }
    let version = match frame.columns[0].value(0)? {
        FrameValue::Text(t) => t.to_owned(),
        _ => return None,
    };
    let readonly = match frame.columns[1].value(0)? {
        FrameValue::Number(s) | FrameValue::Text(s) => s.parse().ok()?,
        _ => return None,
    };
    Some((version, readonly))
}

/// Reads the major and the minor number of a ClickHouse version text such as `26.8.34.13`.
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_header() {
        let h = r#"{"read_rows":"120000","read_bytes":"9000000","written_rows":"0","result_rows":"2","result_bytes":"512","elapsed_ns":"81000000"}"#;
        assert_eq!(parse_summary(h), (Some(120_000), Some(9_000_000), Some(512)));
        assert_eq!(parse_summary(r#"{"read_rows":"1"}"#), (Some(1), None, None));
        assert_eq!(parse_summary(r#"{"read_rows":7,"result_bytes":"x"}"#), (Some(7), None, None));
        assert_eq!(parse_summary("not json"), (None, None, None));
        assert_eq!(parse_summary("[]"), (None, None, None));
    }

    #[test]
    fn version_texts() {
        assert_eq!(parse_version("26.8.34.13"), Some((26, 8)));
        assert_eq!(parse_version("25.3"), Some((25, 3)));
        assert_eq!(parse_version("x.y"), None);
        assert_eq!(parse_version(""), None);
        assert!(parse_version("26.3.1.1").unwrap() >= OLDEST_SUPPORTED_VERSION);
        assert!(parse_version("25.12.1.1").unwrap() < OLDEST_SUPPORTED_VERSION);
    }
}
