// SPDX-License-Identifier: MIT OR Apache-2.0
//! The logger of `serve` (14.6, 14.7).
//!
//! The logger writes one JSON object for each line on the standard output. Each line starts with
//! the members `ts`, `level` and `event`, in this order. A line has 16 KiB at most. The logger
//! never holds a secret, a token or the data of a row: the callers give only the fields of 14.7.
#![allow(dead_code)]

use crate::config::{EffectiveEntry, LogLevel, Warning};
use hune_core::time::UtcMillis;
use serde_json::{Map, Value};
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// The size limit of one line, in bytes (14.6).
pub const MAX_LINE_BYTES: usize = 16 * 1024;

/// Gives the text of a level (14.6).
pub fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
    }
}

/// The rank of a level: the higher the rank, the more lines.
fn rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Error => 0,
        LogLevel::Warn => 1,
        LogLevel::Info => 2,
        LogLevel::Debug => 3,
    }
}

enum Sink {
    Stdout,
    /// A buffer in memory, for the tests.
    Memory(Mutex<Vec<u8>>),
}

/// The counts of the line `store.loaded` (14.7).
#[derive(Debug, Clone, Copy)]
pub struct StoreCounts {
    /// Valid folders.
    pub folders: u64,
    /// Valid dashboards.
    pub dashboards: u64,
    /// Invalid entries and invalid folders.
    pub invalid: u64,
    /// Entries that the scan ignores.
    pub ignored: u64,
}

/// The JSON line logger.
pub struct Logger {
    level: LogLevel,
    sink: Sink,
}

/// One log line under construction. Call `emit` to write it.
#[must_use = "an event is written only by emit()"]
pub struct Event<'a> {
    logger: &'a Logger,
    level: LogLevel,
    name: &'static str,
    fields: Vec<(&'static str, Value)>,
}

impl Logger {
    /// Makes a logger that writes on the standard output.
    pub fn stdout(level: LogLevel) -> Arc<Logger> {
        Arc::new(Logger { level, sink: Sink::Stdout })
    }

    /// Makes a logger that keeps its lines in memory. Read them with `memory_lines`.
    pub fn memory(level: LogLevel) -> Arc<Logger> {
        Arc::new(Logger { level, sink: Sink::Memory(Mutex::new(Vec::new())) })
    }

    /// Gives the lines of a memory logger. A logger on the standard output gives an empty list.
    pub fn memory_lines(&self) -> Vec<String> {
        match &self.sink {
            Sink::Stdout => Vec::new(),
            Sink::Memory(buffer) => {
                let guard = buffer.lock().unwrap_or_else(PoisonError::into_inner);
                String::from_utf8_lossy(&guard).lines().map(str::to_owned).collect()
            }
        }
    }

    /// Tells if a line of this level is written.
    pub fn enabled(&self, level: LogLevel) -> bool {
        rank(level) <= rank(self.level)
    }

    /// Starts a line of the event `name`.
    pub fn event(&self, level: LogLevel, name: &'static str) -> Event<'_> {
        Event { logger: self, level, name, fields: Vec::new() }
    }

    fn write_line(&self, line: &str) {
        match &self.sink {
            Sink::Stdout => {
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                // A closed output is not a reason to stop the server.
                let _ = lock.write_all(line.as_bytes());
                let _ = lock.write_all(b"\n");
                let _ = lock.flush();
            }
            Sink::Memory(buffer) => {
                let mut guard = buffer.lock().unwrap_or_else(PoisonError::into_inner);
                guard.extend_from_slice(line.as_bytes());
                guard.push(b'\n');
            }
        }
    }
}

impl Event<'_> {
    /// Adds a field with a JSON value.
    pub fn field(mut self, key: &'static str, value: impl Into<Value>) -> Self {
        self.fields.push((key, value.into()));
        self
    }

    /// Adds a text field, or `null` when the value is absent (14.6).
    pub fn opt_str(self, key: &'static str, value: Option<&str>) -> Self {
        self.field(key, value.map_or(Value::Null, |v| Value::String(v.to_owned())))
    }

    /// Adds the field `request_id` (14.6).
    pub fn request_id(self, id: &str) -> Self {
        self.field("request_id", id)
    }

    /// Writes the line when the level passes the filter of `log.level`.
    pub fn emit(self) {
        if !self.logger.enabled(self.level) {
            return;
        }
        let line = format_line(now_millis(), self.level, self.name, self.fields);
        self.logger.write_line(&line);
    }
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Gives the current time as RFC 3339 text in UTC with milliseconds.
pub fn now_rfc3339() -> String {
    UtcMillis::from_millis(now_millis()).to_rfc3339()
}

fn push_member(out: &mut String, key: &str, value: &Value) {
    out.push(',');
    out.push_str(&Value::String(key.to_owned()).to_string());
    out.push(':');
    out.push_str(&value.to_string());
}

fn render(ts_ms: i64, level: LogLevel, event: &str, fields: &[(&'static str, Value)], truncated: bool) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\"ts\":");
    out.push_str(&Value::String(UtcMillis::from_millis(ts_ms).to_rfc3339()).to_string());
    out.push_str(",\"level\":\"");
    out.push_str(level_name(level));
    out.push_str("\",\"event\":");
    out.push_str(&Value::String(event.to_owned()).to_string());
    for (key, value) in fields {
        push_member(&mut out, key, value);
    }
    if truncated {
        out.push_str(",\"truncated\":true");
    }
    out.push('}');
    out
}

/// Cuts a text to at most `max` bytes at a character boundary and adds `...`.
fn cut_text(text: &mut String, max: usize) {
    let mut end = max.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str("...");
}

/// Makes one line. A line above `MAX_LINE_BYTES` gets its longest text field cut (14.6).
fn format_line(ts_ms: i64, level: LogLevel, event: &str, mut fields: Vec<(&'static str, Value)>) -> String {
    let mut line = render(ts_ms, level, event, &fields, false);
    if line.len() <= MAX_LINE_BYTES {
        return line;
    }
    // Make room for the member `truncated` and for the three characters `...`.
    for _ in 0..64 {
        line = render(ts_ms, level, event, &fields, true);
        if line.len() <= MAX_LINE_BYTES {
            return line;
        }
        let excess = line.len() - MAX_LINE_BYTES;
        let longest = fields
            .iter_mut()
            .filter_map(|(_, v)| if let Value::String(s) = v { Some(s) } else { None })
            .max_by_key(|s| s.len());
        match longest {
            Some(text) if text.len() > 3 => {
                let keep = text.len().saturating_sub(excess + 3);
                let keep = if text.ends_with("...") { keep.min(text.len() - 3) } else { keep };
                cut_text(text, keep);
            }
            _ => break,
        }
    }
    // No text field can be cut: write the bare line.
    render(ts_ms, level, event, &[], true)
}

/// Reads the JSON text of an effective value (13.22). A text that is not JSON stays a JSON string.
pub fn json_value(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// The fields of the line `http.request` (14.7).
pub struct HttpRequestLog<'a> {
    pub request_id: &'a str,
    pub method: &'a str,
    pub route: &'a str,
    pub status: u16,
    pub code: Option<&'a str>,
    pub duration_ms: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub subject: &'a str,
    pub client: &'a str,
}

impl Logger {
    /// Writes the start log of 13.22: `server.config`, `server.config.sources` and one
    /// `config.warning` for each warning.
    pub fn start_log(&self, effective: &[EffectiveEntry], warnings: &[Warning]) {
        let mut config = Map::new();
        let mut sources = Map::new();
        for entry in effective {
            config.insert(entry.key.clone(), json_value(&entry.json));
            if entry.source != "file" {
                sources.insert(entry.key.clone(), Value::String(entry.source.clone()));
            }
        }
        self.event(LogLevel::Info, "server.config").field("config", Value::Object(config)).emit();
        self.event(LogLevel::Info, "server.config.sources").field("sources", Value::Object(sources)).emit();
        for w in warnings {
            self.event(LogLevel::Warn, "config.warning")
                .field("key", w.key.as_str())
                .field("message", w.message.as_str())
                .emit();
        }
    }

    /// Writes `http.request` (14.7).
    pub fn http_request(&self, r: &HttpRequestLog<'_>) {
        self.event(LogLevel::Info, "http.request")
            .request_id(r.request_id)
            .field("method", r.method)
            .field("route", r.route)
            .field("status", r.status)
            .opt_str("code", r.code)
            .field("duration_ms", r.duration_ms)
            .field("request_bytes", r.request_bytes)
            .field("response_bytes", r.response_bytes)
            .field("subject", r.subject)
            .field("client", r.client)
            .emit();
    }

    /// Writes `server.listening` (14.7).
    pub fn server_listening(&self, listen: &str, ops_listen: &str, access_mode: &str, storage_backend: &str) {
        self.event(LogLevel::Info, "server.listening")
            .field("listen", listen)
            .field("ops_listen", ops_listen)
            .field("version", crate::server::build_info::VERSION)
            .field("commit", crate::server::build_info::COMMIT)
            .field("access_mode", access_mode)
            .field("storage_backend", storage_backend)
            .emit();
    }

    /// Writes `server.ready` (14.7).
    pub fn server_ready(&self, elapsed_ms: u64) {
        self.event(LogLevel::Info, "server.ready").field("elapsed_ms", elapsed_ms).emit();
    }

    /// Writes `server.start_failed` (14.7).
    pub fn server_start_failed(&self, step: &str, error: &str) {
        self.event(LogLevel::Error, "server.start_failed").field("step", step).field("error", error).emit();
    }

    /// Writes `server.stopping` (14.7).
    pub fn server_stopping(&self, signal: &str, in_flight: u64) {
        self.event(LogLevel::Info, "server.stopping").field("signal", signal).field("in_flight", in_flight).emit();
    }

    /// Writes `server.stopped` (14.7).
    pub fn server_stopped(&self, elapsed_ms: u64) {
        self.event(LogLevel::Info, "server.stopped").field("elapsed_ms", elapsed_ms).emit();
    }

    /// Writes `server.shutdown_timeout` (14.7).
    pub fn server_shutdown_timeout(&self, in_flight: u64) {
        self.event(LogLevel::Error, "server.shutdown_timeout").field("in_flight", in_flight).emit();
    }

    /// Writes `server.signal_again` (14.7).
    pub fn server_signal_again(&self, signal: &str) {
        self.event(LogLevel::Error, "server.signal_again").field("signal", signal).emit();
    }

    /// Writes `store.loaded` (14.7).
    pub fn store_loaded(&self, store: &str, backend: &str, counts: &StoreCounts, elapsed_ms: u64) {
        self.event(LogLevel::Info, "store.loaded")
            .field("store", store)
            .field("backend", backend)
            .field("folders", counts.folders)
            .field("dashboards", counts.dashboards)
            .field("invalid", counts.invalid)
            .field("ignored", counts.ignored)
            .field("elapsed_ms", elapsed_ms)
            .emit();
    }

    /// Writes `store.rescan_rejected` (14.7): a poll did not take a change of a store (5.44).
    pub fn store_rescan_rejected(&self, store: &str, path: &str, reason: &str) {
        self.event(LogLevel::Error, "store.rescan_rejected")
            .field("store", store)
            .field("path", path)
            .field("reason", reason)
            .emit();
    }

    /// Writes `store.sync_failed` (14.7). Call it one time for each change of state.
    pub fn store_sync_failed(&self, backend: &str, error: &str) {
        self.event(LogLevel::Warn, "store.sync_failed").field("backend", backend).field("error", error).emit();
    }

    /// Writes `telemetry.export_failed` (14.7). Call it one time for each change of state.
    pub fn telemetry_export_failed(&self, error: &str, dropped: u64) {
        self.event(LogLevel::Warn, "telemetry.export_failed").field("error", error).field("dropped", dropped).emit();
    }

    /// Writes `telemetry.export_recovered` (14.7): the export works again.
    pub fn telemetry_export_recovered(&self, dropped: u64) {
        self.event(LogLevel::Info, "telemetry.export_recovered").field("dropped", dropped).emit();
    }

    /// Writes `store.sync_recovered` (14.7).
    pub fn store_sync_recovered(&self, backend: &str, down_seconds: u64) {
        self.event(LogLevel::Info, "store.sync_recovered")
            .field("backend", backend)
            .field("down_seconds", down_seconds)
            .emit();
    }

    /// Writes `source.down` (14.7). Call it one time for each change of state. `kind` is
    /// `clickhouse`, `postgres` or `mongodb`.
    pub fn source_down(&self, source: &str, kind: &str, error: &str) {
        self.event(LogLevel::Warn, "source.down")
            .field("source", source)
            .field("source_kind", kind)
            .field("error", error)
            .emit();
    }

    /// Writes `source.up` (14.7).
    pub fn source_up(&self, source: &str, kind: &str, down_seconds: u64) {
        self.event(LogLevel::Info, "source.up")
            .field("source", source)
            .field("source_kind", kind)
            .field("down_seconds", down_seconds)
            .emit();
    }

    /// Writes `source.misconfigured` (14.7). `clickhouse_code` is `null` for a PostgreSQL source and a
    /// MongoDB source. `sqlstate` is `null` for a ClickHouse source, a MongoDB source, and a PostgreSQL
    /// source that gave none. `mongo_code` is `null`: the probe of a MongoDB source gives no code.
    pub fn source_misconfigured(&self, source: &str, kind: &str, reason: &str, sqlstate: Option<&str>) {
        self.event(LogLevel::Error, "source.misconfigured")
            .field("source", source)
            .field("source_kind", kind)
            .field("reason", reason)
            .field("clickhouse_code", Value::Null)
            .opt_str("sqlstate", sqlstate)
            .field("mongo_code", Value::Null)
            .emit();
    }

    /// Writes `source.advice` (14.7): one advice check of 8a.12 or 8b.12 has changed
    /// to a failure. `names` are the first names of the check (5 at most). The line holds no SQL text
    /// and no value of a query.
    pub fn source_advice(&self, source: &str, check: &str, names: &[String]) {
        let names: Vec<&str> = names.iter().take(5).map(String::as_str).collect();
        self.event(LogLevel::Warn, "source.advice")
            .field("source", source)
            .field("check", check)
            .field("names", names)
            .emit();
    }

    /// Writes `source.version_unsupported` (14.7). Call it one time for each change.
    pub fn source_version_unsupported(&self, source: &str, kind: &str, version: &str) {
        self.event(LogLevel::Warn, "source.version_unsupported")
            .field("source", source)
            .field("source_kind", kind)
            .field("version", version)
            .emit();
    }

    /// Writes `folder.changed` (14.7). `action` is `create`, `rename` or `delete`.
    pub fn folder_changed(&self, request_id: Option<&str>, action: &str, folder: &str, subject: &str) {
        self.event(LogLevel::Info, "folder.changed")
            .opt_str("request_id", request_id)
            .field("action", action)
            .field("folder", folder)
            .field("subject", subject)
            .emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_with_ts_level_event() {
        let log = Logger::memory(LogLevel::Debug);
        log.event(LogLevel::Info, "x.y").field("b", 1).field("a", "t").emit();
        let lines = log.memory_lines();
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert!(line.starts_with("{\"ts\":\""), "{line}");
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["level"], "info");
        assert_eq!(v["event"], "x.y");
        let ts = v["ts"].as_str().unwrap();
        assert!(UtcMillis::parse_rfc3339(ts).is_some(), "{ts}");
        let pos = |k: &str| line.find(&format!("\"{k}\":")).unwrap();
        assert!(
            pos("ts") < pos("level") && pos("level") < pos("event") && pos("event") < pos("b") && pos("b") < pos("a")
        );
    }

    #[test]
    fn the_source_lines_have_the_kind_and_the_advice_has_at_most_five_names() {
        let log = Logger::memory(LogLevel::Debug);
        log.source_down("metrics", "postgres", "The source did not answer.");
        log.source_up("metrics", "postgres", 4);
        log.source_misconfigured("metrics", "postgres", "R4: metrics.t", Some("42501"));
        log.source_misconfigured("prod", "clickhouse", "readonly is 0", None);
        log.source_version_unsupported("metrics", "postgres", "14.9");
        let names: Vec<String> = (0..8).map(|n| format!("n{n}")).collect();
        log.source_advice("metrics", "A3", &names);
        let lines: Vec<Value> = log.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 6);
        for v in &lines[..5] {
            assert!(v["source_kind"] == "postgres" || v["source_kind"] == "clickhouse", "{v}");
        }
        assert_eq!((lines[2]["sqlstate"].as_str(), lines[2]["clickhouse_code"].is_null()), (Some("42501"), true));
        assert!(lines[3]["sqlstate"].is_null() && lines[3]["clickhouse_code"].is_null());
        let advice = &lines[5];
        assert_eq!((advice["event"].as_str(), advice["level"].as_str()), (Some("source.advice"), Some("warn")));
        assert_eq!((advice["source"].as_str(), advice["check"].as_str()), (Some("metrics"), Some("A3")));
        assert_eq!(advice["names"].as_array().map(Vec::len), Some(5));
        assert!(advice.get("source_kind").is_none());
    }

    #[test]
    fn level_filter() {
        let log = Logger::memory(LogLevel::Warn);
        log.event(LogLevel::Debug, "d").emit();
        log.event(LogLevel::Info, "i").emit();
        log.event(LogLevel::Warn, "w").emit();
        log.event(LogLevel::Error, "e").emit();
        let events: Vec<String> = log
            .memory_lines()
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["event"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(events, ["w", "e"]);
    }

    #[test]
    fn missing_value_is_null() {
        let log = Logger::memory(LogLevel::Info);
        log.event(LogLevel::Info, "x").opt_str("code", None).emit();
        let v: Value = serde_json::from_str(&log.memory_lines()[0]).unwrap();
        assert!(v["code"].is_null());
    }

    #[test]
    fn long_text_is_cut_and_marked() {
        let log = Logger::memory(LogLevel::Info);
        let big = "é".repeat(20_000);
        log.event(LogLevel::Info, "x").field("small", "ok").field("big", big).emit();
        let line = &log.memory_lines()[0];
        assert!(line.len() <= MAX_LINE_BYTES, "{}", line.len());
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["truncated"], true);
        assert_eq!(v["small"], "ok");
        assert!(v["big"].as_str().unwrap().ends_with("..."));
    }

    #[test]
    fn escaped_text_still_fits() {
        let log = Logger::memory(LogLevel::Info);
        log.event(LogLevel::Info, "x").field("big", "\u{1}".repeat(10_000)).emit();
        let line = &log.memory_lines()[0];
        assert!(line.len() <= MAX_LINE_BYTES);
        assert!(serde_json::from_str::<Value>(line).is_ok());
    }

    #[test]
    fn json_value_forms() {
        assert_eq!(json_value("\"0.0.0.0:8080\""), Value::String("0.0.0.0:8080".into()));
        assert_eq!(json_value("1024"), Value::from(1024));
        assert_eq!(json_value("true"), Value::Bool(true));
        assert_eq!(json_value("<hidden>"), Value::String("<hidden>".into()));
        assert_eq!(json_value("[\"a\"]"), serde_json::json!(["a"]));
    }

    #[test]
    fn start_log_has_three_kinds_of_line() {
        let log = Logger::memory(LogLevel::Info);
        let effective = vec![
            EffectiveEntry {
                key: "server.editing".into(),
                value: "true".into(),
                json: "true".into(),
                source: "file".into(),
            },
            EffectiveEntry {
                key: "a.password".into(),
                value: "<hidden>".into(),
                json: "\"<hidden>\"".into(),
                source: "file:/run/secret".into(),
            },
        ];
        let warnings = vec![Warning { key: "access.mode".into(), message: "open".into() }];
        log.start_log(&effective, &warnings);
        let lines = log.memory_lines();
        assert_eq!(lines.len(), 3);
        let first: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(first["event"], "server.config");
        assert_eq!(first["config"]["server.editing"], true);
        assert_eq!(first["config"]["a.password"], "<hidden>");
        let second: Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(second["sources"]["a.password"], "file:/run/secret");
        assert!(second["sources"].get("server.editing").is_none());
        let third: Value = serde_json::from_str(&lines[2]).unwrap();
        assert_eq!(third["event"], "config.warning");
        assert_eq!(third["level"], "warn");
    }
}
