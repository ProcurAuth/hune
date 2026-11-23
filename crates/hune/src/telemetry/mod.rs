// SPDX-License-Identifier: MIT OR Apache-2.0
//! The OTLP exporter of 14.9 and the span model of 14.10 (milestone M7).
//!
//! The module stands alone. The server makes spans with a `Telemetry` handle. A disabled handle
//! makes no span and costs nothing. An enabled handle puts each finished span in a bounded
//! queue. One task, the `ExporterTask`, sends the queue to the collector with OTLP/HTTP and the
//! JSON encoding. The telemetry is best effort and never delays a request (14.9).
//!
//! The module reads no environment variable and uses no OpenTelemetry SDK (R2, R7). The TLS
//! stack is `rustls` with the CA bytes of the configuration only (R6).
//!
//! Use:
//!
//! 1. Call `start(cfg, on_dropped, on_state)`. Spawn `ExporterTask::run` on the runtime.
//! 2. Call `span_server` or `span_client` and `Span::end` in the request path.
//! 3. At a stop, call `Telemetry::shutdown`. It exports the queue one last time, for 2 s at most.

mod client;
mod otlp;
mod span;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{Notify, watch};
use tokio::time::Instant;

use crate::config::{TelemetryConfig, TelemetryEnabled};

#[allow(unused_imports)]
pub use span::{
    Attr, AttrValue, SpanData, SpanId, SpanKind, SpanStatus, SpanType, TraceContext, TraceId, now_unix_nanos,
};

/// The size of the queue, in spans (14.9).
pub const QUEUE_CAPACITY: usize = 2048;
/// The number of waiting spans that starts an export at once, and the size limit of one export.
pub const BATCH_SIZE: usize = 512;
/// The upper limit of the deadline of one export.
pub const MAX_EXPORT_DEADLINE: Duration = Duration::from_secs(10);
/// The time that the last export at a stop can take (14.13).
pub const FINAL_EXPORT_BUDGET: Duration = Duration::from_secs(2);

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The error of the start of the exporter. The text names the key and the fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryError(String);

impl TelemetryError {
    pub(crate) fn new(text: impl Into<String>) -> TelemetryError {
        TelemetryError(text.into())
    }
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TelemetryError {}

/// A change of state of the export. The log hook receives one value for each change (14.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportState {
    /// The export changed to a failure. The log event is `telemetry.export_failed` with the
    /// fields `error` and `dropped`.
    Failed {
        /// The class of the error, as text. It holds no span data.
        error: String,
        /// The number of spans that this export dropped.
        dropped: u64,
    },
    /// The export succeeded after a failure. The specification gives no event for it.
    Recovered,
}

/// The metrics hook: it receives the number of spans that Hune dropped, at the moment of the
/// drop. The caller adds it to `hune_telemetry_dropped_spans_total`.
pub type DroppedHook = Arc<dyn Fn(u64) + Send + Sync>;
/// The log hook: it receives each change of state of the export.
pub type StateHook = Arc<dyn Fn(ExportState) + Send + Sync>;

struct Shared {
    queue: Mutex<VecDeque<SpanData>>,
    notify: Notify,
    dropped: AtomicU64,
    on_dropped: DroppedHook,
    stop: watch::Sender<bool>,
    done: watch::Sender<bool>,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<SpanData>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn count_dropped(&self, n: u64) {
        if n > 0 {
            self.dropped.fetch_add(n, Ordering::Relaxed);
            (self.on_dropped)(n);
        }
    }

    fn push(&self, span: SpanData) {
        if *self.stop.borrow() {
            self.count_dropped(1);
            return;
        }
        let len = {
            let mut q = self.lock();
            if q.len() >= QUEUE_CAPACITY {
                drop(q);
                self.count_dropped(1);
                return;
            }
            q.push_back(span);
            q.len()
        };
        if len >= BATCH_SIZE {
            self.notify.notify_one();
        }
    }

    fn len(&self) -> usize {
        self.lock().len()
    }

    fn take_batch(&self) -> Vec<SpanData> {
        let mut q = self.lock();
        let n = q.len().min(BATCH_SIZE);
        q.drain(..n).collect()
    }

    fn requeue_front(&self, batch: Vec<SpanData>) {
        let mut q = self.lock();
        for s in batch.into_iter().rev() {
            q.push_front(s);
        }
    }

    fn drain_and_count(&self) {
        let n = {
            let mut q = self.lock();
            let n = q.len();
            q.clear();
            n
        };
        self.count_dropped(n as u64);
    }
}

/// The handle of the telemetry. A clone is cheap. A disabled handle makes no span.
#[derive(Clone)]
pub struct Telemetry {
    shared: Option<Arc<Shared>>,
}

impl fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Telemetry").field("enabled", &self.shared.is_some()).finish()
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Telemetry::disabled()
    }
}

/// The tracing context of one request: the handle and the context of the SERVER span of the request.
/// A CLIENT span that `client` makes is a child of the SERVER span. The default value makes no span.
#[derive(Debug, Clone, Default)]
pub struct Tracing {
    telemetry: Telemetry,
    parent: Option<TraceContext>,
}

impl Tracing {
    /// Makes the context of a request. `parent` is the context of its SERVER span.
    pub fn new(telemetry: Telemetry, parent: Option<TraceContext>) -> Tracing {
        Tracing { telemetry, parent }
    }

    /// Starts a CLIENT span that is a child of the SERVER span.
    pub fn client(&self, which: ClientSpan<'_>) -> Span {
        self.telemetry.span_client(which, self.parent.as_ref())
    }
}

/// The type of a CLIENT span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientSpan<'a> {
    /// A query to ClickHouse. The name is `clickhouse.query`.
    ClickhouseQuery,
    /// A query to PostgreSQL. The name is `postgres.query`.
    PostgresQuery,
    /// A query to MongoDB. The name is `mongodb.query`.
    MongoQuery,
    /// An operation of the store. The name is `store.<op>`.
    StoreOperation(&'a str),
    /// A request to the PDP. The name is `pdp.evaluate`.
    PdpEvaluate,
}

impl Telemetry {
    /// Makes a handle that makes no span and costs nothing.
    pub fn disabled() -> Telemetry {
        Telemetry { shared: None }
    }

    /// Gives true when the handle makes spans.
    pub fn is_enabled(&self) -> bool {
        self.shared.is_some()
    }

    /// Starts the telemetry from the typed configuration. With `Disabled`, it gives a disabled
    /// handle and a task that ends at once. The function sends nothing on the network.
    ///
    /// The caller MUST spawn `ExporterTask::run` on the runtime.
    pub fn start(
        cfg: &TelemetryConfig,
        on_dropped: DroppedHook,
        on_state: StateHook,
    ) -> Result<(Telemetry, ExporterTask), TelemetryError> {
        match cfg {
            TelemetryConfig::Disabled => Ok((Telemetry::disabled(), ExporterTask { inner: None })),
            TelemetryConfig::Enabled(enabled) => start(enabled, on_dropped, on_state),
        }
    }

    /// Gives the number of spans that Hune dropped: a full queue and the failed exports. The
    /// metrics code reads this value for `hune_telemetry_dropped_spans_total`.
    #[allow(dead_code)] // Part of the span API.
    pub fn dropped_spans(&self) -> u64 {
        self.shared.as_ref().map_or(0, |s| s.dropped.load(Ordering::Relaxed))
    }

    /// Gives the number of spans in the queue.
    #[allow(dead_code)] // Part of the span API.
    pub fn queued_spans(&self) -> usize {
        self.shared.as_ref().map_or(0, |s| s.len())
    }

    /// Starts a SERVER span for a request of `server.listen` (14.10). The name is
    /// `<METHOD> <route template>`. With a valid incoming `traceparent`, the span is a child of
    /// that trace. Else the span starts a new trace.
    pub fn span_server(&self, name: &str, parent: Option<&TraceContext>) -> Span {
        self.make(SpanType::Request, name, parent)
    }

    /// Starts a CLIENT span (14.10). `parent` is the context of the SERVER span of the request,
    /// or `None` for a call that no request started.
    pub fn span_client(&self, which: ClientSpan<'_>, parent: Option<&TraceContext>) -> Span {
        if self.shared.is_none() {
            return Span { live: None };
        }
        match which {
            ClientSpan::ClickhouseQuery => self.make(SpanType::ClickhouseQuery, "clickhouse.query", parent),
            ClientSpan::PostgresQuery => self.make(SpanType::PostgresQuery, "postgres.query", parent),
            ClientSpan::MongoQuery => self.make(SpanType::MongoQuery, "mongodb.query", parent),
            ClientSpan::StoreOperation(op) => {
                self.make(SpanType::StoreOperation, &format!("store.{}", span::clip(op)), parent)
            }
            ClientSpan::PdpEvaluate => self.make(SpanType::PdpEvaluate, "pdp.evaluate", parent),
        }
    }

    fn make(&self, ty: SpanType, name: &str, parent: Option<&TraceContext>) -> Span {
        let Some(shared) = &self.shared else {
            return Span { live: None };
        };
        let (trace_id, span_id, parent_id, flags) = match parent {
            Some(p) => {
                let Some(id) = span::new_span_id() else {
                    return Span { live: None };
                };
                (p.trace_id, id, Some(p.span_id), p.flags)
            }
            None => {
                let Some((t, s)) = span::new_root_ids() else {
                    return Span { live: None };
                };
                (t, s, None, 0x01)
            }
        };
        Span {
            live: Some(Box::new(Live {
                shared: Arc::clone(shared),
                ty,
                flags,
                data: SpanData {
                    trace_id,
                    span_id,
                    parent: parent_id,
                    kind: ty.kind(),
                    name: span::clip(name),
                    start_unix_nanos: now_unix_nanos(),
                    end_unix_nanos: 0,
                    attributes: Vec::new(),
                    status: SpanStatus::Unset,
                },
            })),
        }
    }

    /// Stops the telemetry at step 6 of 14.13. It asks the exporter task to export the queue one
    /// last time, and it waits for the end, for 2 s at most. A span that ends after the call is
    /// dropped. A disabled handle returns at once.
    pub async fn shutdown(&self) {
        let Some(shared) = &self.shared else {
            return;
        };
        shared.stop.send_replace(true);
        let mut done = shared.done.subscribe();
        let _ = tokio::time::timeout(FINAL_EXPORT_BUDGET + Duration::from_millis(250), done.wait_for(|d| *d)).await;
    }
}

struct Live {
    shared: Arc<Shared>,
    ty: SpanType,
    flags: u8,
    data: SpanData,
}

/// A running span. Call `end` to finish it. A span that drops without `end` is not exported.
/// A span of a disabled handle does nothing.
pub struct Span {
    live: Option<Box<Live>>,
}

impl Span {
    /// Gives true when the span is recorded.
    #[allow(dead_code)] // Part of the span API.
    pub fn is_recording(&self) -> bool {
        self.live.is_some()
    }

    /// Gives the context of this span, for a child span or for a remote call.
    pub fn context(&self) -> Option<TraceContext> {
        self.live.as_ref().map(|l| TraceContext { trace_id: l.data.trace_id, span_id: l.data.span_id, flags: l.flags })
    }

    /// Gives the value of the `traceparent` header that Hune sends to ClickHouse and to the PDP.
    /// A disabled span gives `None`.
    pub fn traceparent(&self) -> Option<String> {
        self.context().map(|c| c.format())
    }

    fn set(&mut self, key: &'static str, value: impl FnOnce() -> AttrValue) {
        let Some(live) = &mut self.live else {
            return;
        };
        // A key outside the closed list of the span type is dropped (14.10, 12.15).
        if !live.ty.allowed_keys().contains(&key) {
            return;
        }
        let value = value();
        match live.data.attributes.iter_mut().find(|a| a.key == key) {
            Some(a) => a.value = value,
            None => live.data.attributes.push(Attr { key, value }),
        }
    }

    /// Sets a text attribute. The text is cut at 256 bytes.
    pub fn set_str(&mut self, key: &'static str, value: &str) {
        self.set(key, || AttrValue::Str(span::clip(value)));
    }

    /// Sets an integer attribute.
    pub fn set_int(&mut self, key: &'static str, value: i64) {
        self.set(key, || AttrValue::Int(value));
    }

    /// Sets a boolean attribute.
    #[allow(dead_code)] // Part of the span API.
    pub fn set_bool(&mut self, key: &'static str, value: bool) {
        self.set(key, || AttrValue::Bool(value));
    }

    /// Sets the status to error (14.10).
    pub fn set_error(&mut self) {
        if let Some(live) = &mut self.live {
            live.data.status = SpanStatus::Error;
        }
    }

    /// Ends the span now and puts it in the queue. A full queue drops it and counts it.
    pub fn end(self) {
        self.end_at(now_unix_nanos());
    }

    /// Ends the span at a given time, in Unix nanoseconds. A time before the start gives a span
    /// of zero length.
    pub fn end_at(self, end_unix_nanos: u64) {
        if let Some(mut live) = self.live {
            live.data.end_unix_nanos = end_unix_nanos.max(live.data.start_unix_nanos);
            live.shared.push(live.data);
        }
    }
}

fn start(
    cfg: &TelemetryEnabled,
    on_dropped: DroppedHook,
    on_state: StateHook,
) -> Result<(Telemetry, ExporterTask), TelemetryError> {
    let endpoint = client::Endpoint::parse(&cfg.otlp_endpoint)?;
    let http = client::Client::new(endpoint, cfg.ca_pem.as_deref(), format!("hune/{VERSION}"))?;
    let instance_id =
        span::new_uuid().ok_or_else(|| TelemetryError::new("telemetry: the system random source gave no bytes"))?;
    let encoder = otlp::Encoder::new(&cfg.service_name, VERSION, &instance_id);
    let (stop, _) = watch::channel(false);
    let (done, _) = watch::channel(false);
    let shared = Arc::new(Shared {
        queue: Mutex::new(VecDeque::new()),
        notify: Notify::new(),
        dropped: AtomicU64::new(0),
        on_dropped,
        stop,
        done,
    });
    let task = ExporterTask {
        inner: Some(TaskInner {
            shared: Arc::clone(&shared),
            http,
            encoder,
            interval: cfg.export_interval,
            deadline: cfg.export_interval.min(MAX_EXPORT_DEADLINE),
            on_state,
            failing: false,
        }),
    };
    Ok((Telemetry { shared: Some(shared) }, task))
}

/// The exporter task. Spawn `run` one time on the runtime.
pub struct ExporterTask {
    inner: Option<TaskInner>,
}

struct TaskInner {
    shared: Arc<Shared>,
    http: client::Client,
    encoder: otlp::Encoder,
    interval: Duration,
    deadline: Duration,
    on_state: StateHook,
    failing: bool,
}

impl ExporterTask {
    /// Runs the exporter until `Telemetry::shutdown`. It makes one export at a time: each
    /// `export_interval`, or at once when 512 spans wait. At the end, it exports the queue one
    /// last time, for 2 s at most.
    pub async fn run(self) {
        if let Some(inner) = self.inner {
            inner.run().await;
        }
    }
}

impl TaskInner {
    async fn run(mut self) {
        let mut stop = self.shared.stop.subscribe();
        let mut ticker = tokio::time::interval_at(Instant::now() + self.interval, self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            if *stop.borrow() {
                break;
            }
            if self.shared.len() >= BATCH_SIZE {
                if self.export_cycle(&mut stop).await {
                    break;
                }
                continue;
            }
            let tick = tokio::select! {
                _ = ticker.tick() => true,
                () = self.shared.notify.notified() => false,
                _ = stop.wait_for(|s| *s) => break,
            };
            if tick && self.export_cycle(&mut stop).await {
                break;
            }
        }
        self.final_export().await;
        self.shared.done.send_replace(true);
    }

    /// Exports one batch. It gives true when a stop interrupted the export.
    async fn export_cycle(&mut self, stop: &mut watch::Receiver<bool>) -> bool {
        let batch = self.shared.take_batch();
        if batch.is_empty() {
            return false;
        }
        let outcome = {
            let attempt = self.attempt(&batch, Instant::now() + self.deadline);
            tokio::select! {
                r = attempt => Some(r),
                _ = stop.wait_for(|s| *s) => None,
            }
        };
        match outcome {
            Some(result) => {
                self.finish(result, batch.len());
                false
            }
            None => {
                // The stop interrupts the export. The final export sends the batch.
                self.shared.requeue_front(batch);
                true
            }
        }
    }

    async fn final_export(&mut self) {
        let end = Instant::now() + FINAL_EXPORT_BUDGET;
        loop {
            let batch = self.shared.take_batch();
            if batch.is_empty() {
                return;
            }
            let limit = (Instant::now() + self.deadline).min(end);
            let result = self.attempt(&batch, limit).await;
            let failed = result.is_err();
            self.finish(result, batch.len());
            if failed || Instant::now() >= end {
                self.shared.drain_and_count();
                return;
            }
        }
    }

    async fn attempt(&self, batch: &[SpanData], limit: Instant) -> Result<(), String> {
        let body = self.encoder.encode(batch);
        match tokio::time::timeout_at(limit, self.http.post(body)).await {
            Ok(r) => r,
            Err(_) => Err("deadline exceeded".to_owned()),
        }
    }

    fn finish(&mut self, result: Result<(), String>, spans: usize) {
        match result {
            Ok(()) => {
                if self.failing {
                    self.failing = false;
                    (self.on_state)(ExportState::Recovered);
                }
            }
            Err(error) => {
                let dropped = spans as u64;
                self.shared.count_dropped(dropped);
                if !self.failing {
                    self.failing = true;
                    (self.on_state)(ExportState::Failed { error, dropped });
                }
            }
        }
    }
}
