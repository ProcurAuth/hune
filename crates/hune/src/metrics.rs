// SPDX-License-Identifier: MIT OR Apache-2.0
//! The metrics of 14.8 and the Prometheus text writer (format 0.0.4).
//!
//! The metrics are counters, gauges and histograms with a fixed list of label names. The label
//! values come from closed lists (routes, status codes, source ids, store operations), so the
//! number of series is bounded. No label holds a uid, a subject, SQL or an error message.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// The buckets of the HTTP, query and store histograms (14.8), in seconds.
pub const DURATION_BUCKETS: [f64; 14] =
    [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0];
/// The buckets of the queue wait and PDP histograms (14.8), in seconds.
pub const WAIT_BUCKETS: [f64; 14] =
    [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 60.0];

/// The values of the label `reason` of `hune_http_refused_total` (14.8).
pub const REFUSED_REASONS: [&str; 8] = [
    "max_connections",
    "header_timeout",
    "body_timeout",
    "uri_too_long",
    "headers_too_large",
    "payload_too_large",
    "cross_site_request",
    "unsupported_media_type",
];

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn labels_vec(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

/// A counter without a label.
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    /// Adds 1.
    pub fn inc(&self) {
        self.add(1);
    }
    /// Adds a value.
    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }
    /// Reads the value.
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// A gauge without a label.
#[derive(Debug, Default)]
pub struct Gauge(AtomicI64);

impl Gauge {
    /// Sets the value.
    pub fn set(&self, v: i64) {
        self.0.store(v, Ordering::Relaxed);
    }
    /// Adds a value, which can be negative.
    pub fn add(&self, v: i64) {
        self.0.fetch_add(v, Ordering::Relaxed);
    }
    /// Reads the value.
    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// A counter with labels.
#[derive(Debug)]
pub struct CounterVec {
    cells: Mutex<BTreeMap<Vec<String>, u64>>,
}

impl CounterVec {
    fn new(seed: &[&[&str]]) -> Self {
        CounterVec { cells: Mutex::new(seed.iter().map(|s| (labels_vec(s), 0)).collect()) }
    }
    /// Adds a value to the series of the label values.
    pub fn add(&self, values: &[&str], n: u64) {
        *lock(&self.cells).entry(labels_vec(values)).or_insert(0) += n;
    }
    /// Adds 1.
    pub fn inc(&self, values: &[&str]) {
        self.add(values, 1);
    }
    /// Reads one series. A series that does not exist is 0.
    pub fn get(&self, values: &[&str]) -> u64 {
        lock(&self.cells).get(&labels_vec(values)).copied().unwrap_or(0)
    }
}

/// A gauge with labels.
#[derive(Debug, Default)]
pub struct GaugeVec {
    cells: Mutex<BTreeMap<Vec<String>, i64>>,
}

impl GaugeVec {
    /// Sets one series.
    pub fn set(&self, values: &[&str], v: i64) {
        lock(&self.cells).insert(labels_vec(values), v);
    }
    /// Adds to one series.
    pub fn add(&self, values: &[&str], v: i64) {
        *lock(&self.cells).entry(labels_vec(values)).or_insert(0) += v;
    }
    /// Reads one series. A series that does not exist is 0.
    pub fn get(&self, values: &[&str]) -> i64 {
        lock(&self.cells).get(&labels_vec(values)).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
struct HistCell {
    counts: Vec<u64>,
    sum: f64,
    count: u64,
}

/// A histogram with labels.
#[derive(Debug)]
pub struct HistogramVec {
    buckets: &'static [f64],
    cells: Mutex<BTreeMap<Vec<String>, HistCell>>,
}

impl HistogramVec {
    fn new(buckets: &'static [f64], seed: &[&[&str]]) -> Self {
        let cells = seed
            .iter()
            .map(|s| (labels_vec(s), HistCell { counts: vec![0; buckets.len()], sum: 0.0, count: 0 }))
            .collect();
        HistogramVec { buckets, cells: Mutex::new(cells) }
    }
    /// Records one value in seconds.
    pub fn observe(&self, values: &[&str], seconds: f64) {
        let mut cells = lock(&self.cells);
        let cell = cells.entry(labels_vec(values)).or_insert_with(|| HistCell {
            counts: vec![0; self.buckets.len()],
            sum: 0.0,
            count: 0,
        });
        for (i, upper) in self.buckets.iter().enumerate() {
            if seconds <= *upper {
                cell.counts[i] += 1;
            }
        }
        cell.sum += seconds;
        cell.count += 1;
    }
    /// Records a duration.
    pub fn observe_duration(&self, values: &[&str], d: Duration) {
        self.observe(values, d.as_secs_f64());
    }
    /// The number of values of one series.
    pub fn count(&self, values: &[&str]) -> u64 {
        lock(&self.cells).get(&labels_vec(values)).map_or(0, |c| c.count)
    }
}

/// Escapes a label value (text format 0.0.4).
fn escape_label(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

fn label_text(names: &[&str], values: &[String], extra: Option<(&str, &str)>) -> String {
    let mut parts: Vec<String> =
        names.iter().zip(values).map(|(n, v)| format!("{n}=\"{}\"", escape_label(v))).collect();
    if let Some((n, v)) = extra {
        parts.push(format!("{n}=\"{v}\""));
    }
    if parts.is_empty() { String::new() } else { format!("{{{}}}", parts.join(",")) }
}

fn header(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

fn write_counter(out: &mut String, name: &str, help: &str, v: &Counter) {
    header(out, name, "counter", help);
    let _ = writeln!(out, "{name} {}", v.get());
}

fn write_gauge(out: &mut String, name: &str, help: &str, v: &Gauge) {
    header(out, name, "gauge", help);
    let _ = writeln!(out, "{name} {}", v.get());
}

fn write_counter_vec(out: &mut String, name: &str, help: &str, names: &[&str], v: &CounterVec) {
    header(out, name, "counter", help);
    for (values, n) in lock(&v.cells).iter() {
        let _ = writeln!(out, "{name}{} {n}", label_text(names, values, None));
    }
}

fn write_gauge_vec(out: &mut String, name: &str, help: &str, names: &[&str], v: &GaugeVec) {
    header(out, name, "gauge", help);
    for (values, n) in lock(&v.cells).iter() {
        let _ = writeln!(out, "{name}{} {n}", label_text(names, values, None));
    }
}

fn write_histogram(out: &mut String, name: &str, help: &str, names: &[&str], v: &HistogramVec) {
    header(out, name, "histogram", help);
    for (values, cell) in lock(&v.cells).iter() {
        for (i, upper) in v.buckets.iter().enumerate() {
            let le = format!("{upper}");
            let _ = writeln!(out, "{name}_bucket{} {}", label_text(names, values, Some(("le", &le))), cell.counts[i]);
        }
        let _ = writeln!(out, "{name}_bucket{} {}", label_text(names, values, Some(("le", "+Inf"))), cell.count);
        let _ = writeln!(out, "{name}_sum{} {}", label_text(names, values, None), cell.sum);
        let _ = writeln!(out, "{name}_count{} {}", label_text(names, values, None), cell.count);
    }
}

/// All metrics of the process (14.8).
#[derive(Debug)]
pub struct Metrics {
    start_time_seconds: i64,
    ready: Gauge,
    stopping: Gauge,
    http_requests: CounterVec,
    http_duration: HistogramVec,
    http_in_flight: Gauge,
    http_refused: CounterVec,
    query_duration: HistogramVec,
    query_rows: CounterVec,
    query_bytes: CounterVec,
    source_in_flight: GaugeVec,
    source_queue_wait: HistogramVec,
    source_refused: CounterVec,
    source_up: GaugeVec,
    result_memory: Gauge,
    store_duration: HistogramVec,
    store_operations: CounterVec,
    store_conflicts: Counter,
    store_poll_changes: Counter,
    store_up: Gauge,
    store_catalog_age: Gauge,
    documents: GaugeVec,
    store_invalid_files: GaugeVec,
    pdp_requests: CounterVec,
    pdp_duration: HistogramVec,
    pdp_questions: CounterVec,
    pdp_cache_hits: Counter,
    pdp_pool_in_use: Gauge,
    telemetry_dropped: Counter,
    version: &'static str,
    commit: &'static str,
}

/// The values of the label `result` of `hune_pdp_requests_total` (14.8).
const PDP_RESULTS: [&str; 6] = ["ok", "timeout", "transport", "status", "invalid", "pool_busy"];

impl Metrics {
    /// Makes the metrics. `start_unix_seconds` is the Unix time of the start.
    pub fn new(version: &'static str, commit: &'static str, start_unix_seconds: i64) -> Self {
        let refused: Vec<[&str; 1]> = REFUSED_REASONS.iter().map(|r| [*r]).collect();
        let refused_seed: Vec<&[&str]> = refused.iter().map(|r| &r[..]).collect();
        let pdp: Vec<[&str; 1]> = PDP_RESULTS.iter().map(|r| [*r]).collect();
        let pdp_seed: Vec<&[&str]> = pdp.iter().map(|r| &r[..]).collect();
        let docs: [[&str; 1]; 3] = [["valid"], ["invalid"], ["ignored"]];
        let docs_seed: Vec<&[&str]> = docs.iter().map(|r| &r[..]).collect();
        let m = Metrics {
            start_time_seconds: start_unix_seconds,
            ready: Gauge::default(),
            stopping: Gauge::default(),
            http_requests: CounterVec::new(&[]),
            http_duration: HistogramVec::new(&DURATION_BUCKETS, &[]),
            http_in_flight: Gauge::default(),
            http_refused: CounterVec::new(&refused_seed),
            query_duration: HistogramVec::new(&DURATION_BUCKETS, &[]),
            query_rows: CounterVec::new(&[]),
            query_bytes: CounterVec::new(&[]),
            source_in_flight: GaugeVec::default(),
            source_queue_wait: HistogramVec::new(&WAIT_BUCKETS, &[]),
            source_refused: CounterVec::new(&[]),
            source_up: GaugeVec::default(),
            result_memory: Gauge::default(),
            store_duration: HistogramVec::new(&DURATION_BUCKETS, &[]),
            store_operations: CounterVec::new(&[]),
            store_conflicts: Counter::default(),
            store_poll_changes: Counter::default(),
            store_up: Gauge::default(),
            store_catalog_age: Gauge::default(),
            documents: GaugeVec::default(),
            store_invalid_files: GaugeVec::default(),
            pdp_requests: CounterVec::new(&pdp_seed),
            pdp_duration: HistogramVec::new(&WAIT_BUCKETS, &[&[]]),
            pdp_questions: CounterVec::new(&[&["true"], &["false"]]),
            pdp_cache_hits: Counter::default(),
            pdp_pool_in_use: Gauge::default(),
            telemetry_dropped: Counter::default(),
            version,
            commit,
        };
        for s in docs_seed {
            m.documents.set(s, 0);
        }
        m
    }

    /// Sets `hune_ready`.
    pub fn set_ready(&self, ready: bool) {
        self.ready.set(i64::from(ready));
    }
    /// Sets `hune_stopping`.
    pub fn set_stopping(&self, stopping: bool) {
        self.stopping.set(i64::from(stopping));
    }
    /// Records one answer of the main listener: the counter and the histogram of 14.8.
    pub fn http_request(&self, route: &str, status: u16, duration: Duration) {
        self.http_requests.inc(&[route, &status.to_string()]);
        self.http_duration.observe_duration(&[route], duration);
    }
    /// Adds a request in flight. Drop the guard at the end of the request.
    pub fn http_in_flight_guard(&self) -> InFlightGuard<'_> {
        self.http_in_flight.add(1);
        InFlightGuard { gauge: &self.http_in_flight }
    }
    /// The requests in flight now.
    pub fn http_in_flight(&self) -> i64 {
        self.http_in_flight.get()
    }
    /// Counts a refusal before the route. `reason` is one of `REFUSED_REASONS`.
    pub fn http_refused(&self, reason: &str) {
        self.http_refused.inc(&[reason]);
    }
    /// Records one query to a source (14.8). `status` is `ok`, `timeout`, `limit`, `ceiling`,
    /// `sql_error`, `unavailable` or `cancelled`.
    pub fn observe_query(&self, source: &str, status: &str, duration: Duration) {
        self.query_duration.observe_duration(&[source, status], duration);
    }
    /// Counts the rows of a result.
    pub fn query_rows(&self, source: &str, rows: u64) {
        self.query_rows.add(&[source], rows);
    }
    /// Counts the bytes that ClickHouse sent.
    pub fn query_response_bytes(&self, source: &str, bytes: u64) {
        self.query_bytes.add(&[source], bytes);
    }
    /// Sets the queries that run now on a source.
    pub fn set_source_in_flight(&self, source: &str, n: usize) {
        self.source_in_flight.set(&[source], i64::try_from(n).unwrap_or(i64::MAX));
    }
    /// Changes the queries that run now on a source.
    pub fn source_in_flight(&self, source: &str, delta: i64) {
        self.source_in_flight.add(&[source], delta);
    }
    /// Records the wait for a place of a source.
    pub fn observe_queue_wait(&self, source: &str, wait: Duration) {
        self.source_queue_wait.observe_duration(&[source], wait);
    }
    /// Counts a query that got `source_busy`. `reason` is `queue_wait`, `per_subject` or `memory`.
    pub fn source_refused(&self, source: &str, reason: &str) {
        self.source_refused.inc(&[source, reason]);
    }
    /// Sets `hune_source_up` of a source.
    pub fn source_up(&self, source: &str, up: bool) {
        self.source_up.set(&[source], i64::from(up));
    }
    /// Sets the bytes that the queries in flight reserve.
    pub fn result_memory_reserved(&self, bytes: i64) {
        self.result_memory.set(bytes);
    }
    /// Records one operation of the store. `status` is `ok`, `conflict`, `unavailable`, `full` or `invalid`.
    pub fn observe_store_op(&self, backend: &str, op: &str, status: &str, duration: Duration) {
        self.store_duration.observe_duration(&[backend, op], duration);
        self.store_operations.inc(&[backend, op, status]);
    }
    /// Counts a save that got `409 conflict`.
    pub fn store_conflict(&self) {
        self.store_conflicts.inc();
    }
    /// Counts the documents that a poll found changed.
    pub fn store_poll_changes(&self, n: u64) {
        self.store_poll_changes.add(n);
    }
    /// Sets `hune_store_up`.
    pub fn store_up(&self, up: bool) {
        self.store_up.set(i64::from(up));
    }
    /// Sets `hune_store_catalog_age_seconds`.
    pub fn store_catalog_age_seconds(&self, seconds: u64) {
        self.store_catalog_age.set(i64::try_from(seconds).unwrap_or(i64::MAX));
    }
    /// Sets the documents of the catalog. `state` is `valid`, `invalid` or `ignored`.
    pub fn documents(&self, state: &str, n: u64) {
        self.documents.set(&[state], i64::try_from(n).unwrap_or(i64::MAX));
    }
    /// Sets `hune_store_invalid_files` for one volume: the files that are not valid now (5.44).
    pub fn store_invalid_files(&self, store: &str, n: u64) {
        self.store_invalid_files.set(&[store], i64::try_from(n).unwrap_or(i64::MAX));
    }
    /// Counts a request to the PDP. `result` is `ok`, `timeout`, `transport`, `status`, `invalid` or `pool_busy`.
    pub fn pdp_request(&self, result: &str) {
        self.pdp_requests.inc(&[result]);
    }
    /// Records the time of one request to the PDP.
    pub fn observe_pdp(&self, d: Duration) {
        self.pdp_duration.observe_duration(&[], d);
    }
    /// Counts a question that the PDP answered. `decision` is `true` or `false`.
    pub fn pdp_question(&self, decision: bool) {
        self.pdp_questions.inc(&[if decision { "true" } else { "false" }]);
    }
    /// Counts a question that a kept answer served.
    pub fn pdp_cache_hit(&self) {
        self.pdp_cache_hits.inc();
    }
    /// Sets the permits of the PDP pool in use.
    pub fn pdp_pool_in_use(&self, n: i64) {
        self.pdp_pool_in_use.set(n);
    }
    /// Counts spans that the exporter dropped.
    pub fn telemetry_dropped(&self, n: u64) {
        self.telemetry_dropped.add(n);
    }

    /// Writes all metrics in the Prometheus text format 0.0.4, in the order of 14.8.
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(8192);
        header(&mut out, "hune_build_info", "gauge", "The build.");
        let _ = writeln!(
            out,
            "hune_build_info{{version=\"{}\",commit=\"{}\"}} 1",
            escape_label(self.version),
            escape_label(self.commit)
        );
        header(&mut out, "hune_start_time_seconds", "gauge", "The Unix time of the start.");
        let _ = writeln!(out, "hune_start_time_seconds {}", self.start_time_seconds);
        write_gauge(&mut out, "hune_ready", "1 when /readyz answers 200.", &self.ready);
        write_gauge(&mut out, "hune_stopping", "1 from the signal of a stop.", &self.stopping);
        write_counter_vec(
            &mut out,
            "hune_http_requests_total",
            "The answers of the main listener.",
            &["route", "status"],
            &self.http_requests,
        );
        write_histogram(
            &mut out,
            "hune_http_request_duration_seconds",
            "The time from the first byte of the request to the last byte of the answer.",
            &["route"],
            &self.http_duration,
        );
        write_gauge(&mut out, "hune_http_in_flight", "The requests that Hune works on now.", &self.http_in_flight);
        write_counter_vec(
            &mut out,
            "hune_http_refused_total",
            "The refusals before the route.",
            &["reason"],
            &self.http_refused,
        );
        write_histogram(
            &mut out,
            "hune_query_duration_seconds",
            "The time of a query.",
            &["source", "status"],
            &self.query_duration,
        );
        write_counter_vec(&mut out, "hune_query_rows_total", "The rows of the results.", &["source"], &self.query_rows);
        write_counter_vec(
            &mut out,
            "hune_query_response_bytes_total",
            "The bytes that ClickHouse sent.",
            &["source"],
            &self.query_bytes,
        );
        write_gauge_vec(
            &mut out,
            "hune_source_in_flight",
            "The queries that run now on the source.",
            &["source"],
            &self.source_in_flight,
        );
        write_histogram(
            &mut out,
            "hune_source_queue_wait_seconds",
            "The wait for a place.",
            &["source"],
            &self.source_queue_wait,
        );
        write_counter_vec(
            &mut out,
            "hune_source_refused_total",
            "The queries that got source_busy.",
            &["source", "reason"],
            &self.source_refused,
        );
        write_gauge_vec(&mut out, "hune_source_up", "1 when the last ping succeeded.", &["source"], &self.source_up);
        write_gauge(
            &mut out,
            "hune_result_memory_reserved_bytes",
            "The bytes that the queries in flight reserve.",
            &self.result_memory,
        );
        write_histogram(
            &mut out,
            "hune_store_operation_duration_seconds",
            "The time of an operation of the store.",
            &["backend", "op"],
            &self.store_duration,
        );
        write_counter_vec(
            &mut out,
            "hune_store_operations_total",
            "The operations of the store.",
            &["backend", "op", "status"],
            &self.store_operations,
        );
        write_counter(
            &mut out,
            "hune_store_conflicts_total",
            "The saves that got 409 conflict.",
            &self.store_conflicts,
        );
        write_counter(
            &mut out,
            "hune_store_poll_changes_total",
            "The documents that a poll or a scan found changed.",
            &self.store_poll_changes,
        );
        write_gauge(&mut out, "hune_store_up", "1 when the last poll or scan succeeded.", &self.store_up);
        write_gauge(
            &mut out,
            "hune_store_catalog_age_seconds",
            "The seconds since the last poll or scan that succeeded.",
            &self.store_catalog_age,
        );
        write_gauge_vec(&mut out, "hune_documents", "The documents of the catalog.", &["state"], &self.documents);
        write_gauge_vec(
            &mut out,
            "hune_store_invalid_files",
            "The files of a volume that are not valid now. The catalog keeps the last valid state of each of them.",
            &["store"],
            &self.store_invalid_files,
        );
        write_counter_vec(
            &mut out,
            "hune_pdp_requests_total",
            "The requests to the PDP.",
            &["result"],
            &self.pdp_requests,
        );
        write_histogram(
            &mut out,
            "hune_pdp_duration_seconds",
            "The time of one request to the PDP.",
            &[],
            &self.pdp_duration,
        );
        write_counter_vec(
            &mut out,
            "hune_pdp_questions_total",
            "The questions that the PDP answered.",
            &["decision"],
            &self.pdp_questions,
        );
        write_counter(
            &mut out,
            "hune_pdp_cache_hits_total",
            "The questions that a kept answer served.",
            &self.pdp_cache_hits,
        );
        write_gauge(&mut out, "hune_pdp_pool_in_use", "The permits of the pool in use.", &self.pdp_pool_in_use);
        write_counter(
            &mut out,
            "hune_telemetry_dropped_spans_total",
            "The spans that the exporter dropped.",
            &self.telemetry_dropped,
        );
        out
    }
}

/// Removes a request from `hune_http_in_flight` when dropped.
pub struct InFlightGuard<'a> {
    gauge: &'a Gauge,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.gauge.add(-1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// The label names of each metric (14.8) and its type.
    const TABLE: &[(&str, &str, &[&str])] = &[
        ("hune_build_info", "gauge", &["version", "commit"]),
        ("hune_start_time_seconds", "gauge", &[]),
        ("hune_ready", "gauge", &[]),
        ("hune_stopping", "gauge", &[]),
        ("hune_http_requests_total", "counter", &["route", "status"]),
        ("hune_http_request_duration_seconds", "histogram", &["route"]),
        ("hune_http_in_flight", "gauge", &[]),
        ("hune_http_refused_total", "counter", &["reason"]),
        ("hune_query_duration_seconds", "histogram", &["source", "status"]),
        ("hune_query_rows_total", "counter", &["source"]),
        ("hune_query_response_bytes_total", "counter", &["source"]),
        ("hune_source_in_flight", "gauge", &["source"]),
        ("hune_source_queue_wait_seconds", "histogram", &["source"]),
        ("hune_source_refused_total", "counter", &["source", "reason"]),
        ("hune_source_up", "gauge", &["source"]),
        ("hune_result_memory_reserved_bytes", "gauge", &[]),
        ("hune_store_operation_duration_seconds", "histogram", &["backend", "op"]),
        ("hune_store_operations_total", "counter", &["backend", "op", "status"]),
        ("hune_store_conflicts_total", "counter", &[]),
        ("hune_store_poll_changes_total", "counter", &[]),
        ("hune_store_up", "gauge", &[]),
        ("hune_store_catalog_age_seconds", "gauge", &[]),
        ("hune_documents", "gauge", &["state"]),
        ("hune_store_invalid_files", "gauge", &["store"]),
        ("hune_pdp_requests_total", "counter", &["result"]),
        ("hune_pdp_duration_seconds", "histogram", &[]),
        ("hune_pdp_questions_total", "counter", &["decision"]),
        ("hune_pdp_cache_hits_total", "counter", &[]),
        ("hune_pdp_pool_in_use", "gauge", &[]),
        ("hune_telemetry_dropped_spans_total", "counter", &[]),
    ];

    fn exercised() -> Metrics {
        let m = Metrics::new("0.1.0", "abc", 1_794_000_000);
        m.set_ready(true);
        m.http_request("GET /api/session", 200, Duration::from_millis(3));
        m.http_refused("max_connections");
        m.observe_query("prod", "ok", Duration::from_millis(30));
        m.query_rows("prod", 5);
        m.query_response_bytes("prod", 500);
        m.source_in_flight("prod", 1);
        m.observe_queue_wait("prod", Duration::from_millis(2));
        m.source_refused("prod", "queue_wait");
        m.source_up("prod", true);
        m.observe_store_op("volume", "get", "ok", Duration::from_millis(1));
        m.store_up(true);
        m.documents("valid", 3);
        m.store_invalid_files("lab", 1);
        m.pdp_request("ok");
        m.observe_pdp(Duration::from_millis(1));
        m
    }

    /// Reads the text and gives, for each metric name, its type and the label names of its samples.
    fn parse(text: &str) -> BTreeMap<String, (String, BTreeSet<Vec<String>>)> {
        let mut types: BTreeMap<String, String> = BTreeMap::new();
        let mut out: BTreeMap<String, (String, BTreeSet<Vec<String>>)> = BTreeMap::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("# TYPE ") {
                let (name, kind) = rest.split_once(' ').unwrap();
                types.insert(name.to_owned(), kind.to_owned());
                out.insert(name.to_owned(), (kind.to_owned(), BTreeSet::new()));
            } else if line.starts_with('#') {
                continue;
            } else {
                let (series, _value) = line.rsplit_once(' ').unwrap();
                let (name, labels) = match series.split_once('{') {
                    Some((n, l)) => (n, l.trim_end_matches('}')),
                    None => (series, ""),
                };
                let base = ["_bucket", "_sum", "_count"]
                    .iter()
                    .find_map(|s| name.strip_suffix(s).filter(|b| types.get(*b).is_some_and(|t| t == "histogram")))
                    .unwrap_or(name);
                let mut names: Vec<String> = labels
                    .split("\",")
                    .filter(|p| !p.is_empty())
                    .map(|p| p.split('=').next().unwrap().to_owned())
                    .filter(|n| !(base != name && n == "le"))
                    .collect();
                names.sort();
                out.get_mut(base).unwrap_or_else(|| panic!("no TYPE for {base}")).1.insert(names);
            }
        }
        out
    }

    #[test]
    fn every_metric_of_14_8_is_present_with_its_label_set() {
        let text = exercised().render();
        let parsed = parse(&text);
        assert_eq!(parsed.len(), TABLE.len(), "{:?}", parsed.keys());
        for (name, kind, labels) in TABLE {
            let (got_kind, sets) = parsed.get(*name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(got_kind, kind, "{name}");
            let mut want: Vec<String> = labels.iter().map(|s| (*s).to_owned()).collect();
            want.sort();
            for set in sets {
                assert_eq!(*set, want, "{name}");
            }
        }
        // The metrics are in the order of the table.
        let order: Vec<&str> =
            text.lines().filter_map(|l| l.strip_prefix("# TYPE ")).map(|l| l.split(' ').next().unwrap()).collect();
        let want: Vec<&str> = TABLE.iter().map(|t| t.0).collect();
        assert_eq!(order, want);
    }

    #[test]
    fn histogram_text_is_cumulative_with_the_buckets_of_14_8() {
        let m = Metrics::new("0.1.0", "abc", 0);
        m.http_request("GET /", 200, Duration::from_millis(7));
        m.http_request("GET /", 200, Duration::from_secs(200));
        let text = m.render();
        assert!(text.contains("hune_http_request_duration_seconds_bucket{route=\"GET /\",le=\"0.005\"} 0\n"), "{text}");
        assert!(text.contains("hune_http_request_duration_seconds_bucket{route=\"GET /\",le=\"0.01\"} 1\n"));
        assert!(text.contains("hune_http_request_duration_seconds_bucket{route=\"GET /\",le=\"120\"} 1\n"));
        assert!(text.contains("hune_http_request_duration_seconds_bucket{route=\"GET /\",le=\"+Inf\"} 2\n"));
        assert!(text.contains("hune_http_request_duration_seconds_count{route=\"GET /\"} 2\n"));
        let n = text.lines().filter(|l| l.starts_with("hune_http_request_duration_seconds_bucket")).count();
        assert_eq!(n, 15);
        assert!(text.contains("hune_http_requests_total{route=\"GET /\",status=\"200\"} 2\n"));
    }

    #[test]
    fn wait_histograms_use_their_own_buckets() {
        let m = Metrics::new("0.1.0", "abc", 0);
        m.observe_queue_wait("s", Duration::from_millis(1));
        let text = m.render();
        assert!(text.contains("hune_source_queue_wait_seconds_bucket{source=\"s\",le=\"0.001\"} 1\n"), "{text}");
        assert!(text.contains("hune_source_queue_wait_seconds_bucket{source=\"s\",le=\"60\"} 1\n"));
        // The PDP histogram has a zero series in mode open.
        assert!(text.contains("hune_pdp_duration_seconds_bucket{le=\"0.001\"} 0\n"));
        assert!(text.contains("hune_pdp_requests_total{result=\"ok\"} 0\n"));
        assert!(text.contains("hune_pdp_questions_total{decision=\"true\"} 0\n"));
    }

    #[test]
    fn gauges_and_in_flight_guard() {
        let m = Metrics::new("0.1.0", "abc", 42);
        {
            let _a = m.http_in_flight_guard();
            let _b = m.http_in_flight_guard();
            assert_eq!(m.http_in_flight(), 2);
        }
        assert_eq!(m.http_in_flight(), 0);
        m.set_stopping(true);
        let text = m.render();
        assert!(text.contains("hune_stopping 1\n"));
        assert!(text.contains("hune_ready 0\n"));
        assert!(text.contains("hune_start_time_seconds 42\n"));
        assert!(text.contains("hune_build_info{version=\"0.1.0\",commit=\"abc\"} 1\n"));
    }

    #[test]
    fn label_values_are_escaped() {
        let m = Metrics::new("0.1.0", "abc", 0);
        m.source_up("a\"b\\c\nd", true);
        assert!(m.render().contains("hune_source_up{source=\"a\\\"b\\\\c\\nd\"} 1\n"));
    }

    #[test]
    fn refused_reasons_are_seeded() {
        let text = Metrics::new("0.1.0", "abc", 0).render();
        for r in REFUSED_REASONS {
            assert!(text.contains(&format!("hune_http_refused_total{{reason=\"{r}\"}} 0\n")), "{r}");
        }
    }
}
