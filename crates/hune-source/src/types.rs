//! The plain types of the `DataSource` contract (8.1): ids, limits, context and state.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hune_core::mongo::MongoQuery;

/// A UUID of version 7 (RFC 9562). It is the `query_id` of 8.10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid([u8; 16]);

impl Uuid {
    /// Makes a UUID version 7 from a time in Unix milliseconds and 10 random bytes.
    ///
    /// The UUID holds the 48 low bits of `unix_ms`. It uses 74 of the 80 random bits.
    pub fn new_v7(unix_ms: u64, random: [u8; 10]) -> Uuid {
        let mut b = [0u8; 16];
        b[..6].copy_from_slice(&unix_ms.to_be_bytes()[2..]);
        b[6..].copy_from_slice(&random);
        b[6] = (b[6] & 0x0f) | 0x70;
        b[8] = (b[8] & 0x3f) | 0x80;
        Uuid(b)
    }

    /// Makes a UUID version 7 from the system clock and the system random source.
    ///
    /// When the random source fails, the function mixes the clock and a counter instead.
    /// A query id is an attribution key, not a secret.
    pub fn now_v7() -> Uuid {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let mut random = [0u8; 10];
        if getrandom::fill(&mut random).is_err() {
            let mix = (now.as_nanos() as u64) ^ COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
            random[..8].copy_from_slice(&mix.to_le_bytes());
            random[8..].copy_from_slice(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes()[..2]);
        }
        Uuid::new_v7(now.as_millis() as u64, random)
    }

    /// Reads the canonical text form: 36 characters, lower-case or upper-case hexadecimal.
    pub fn parse(s: &str) -> Option<Uuid> {
        let b = s.as_bytes();
        if b.len() != 36 || [8, 13, 18, 23].iter().any(|&i| b[i] != b'-') {
            return None;
        }
        let mut out = [0u8; 16];
        let mut n = 0;
        let mut i = 0;
        while i < 36 {
            if matches!(i, 8 | 13 | 18 | 23) {
                i += 1;
                continue;
            }
            let hi = (b[i] as char).to_digit(16)?;
            let lo = (b[i + 1] as char).to_digit(16)?;
            out[n] = (hi * 16 + lo) as u8;
            n += 1;
            i += 2;
        }
        Some(Uuid(out))
    }

    /// Gives the 16 bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Gives the version number of the UUID.
    pub fn version(&self) -> u8 {
        self.0[6] >> 4
    }

    /// Gives the time of a version 7 UUID, in Unix milliseconds.
    pub fn unix_ms(&self) -> u64 {
        let mut b = [0u8; 8];
        b[2..].copy_from_slice(&self.0[..6]);
        u64::from_be_bytes(b)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// The kind of a data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// A ClickHouse server on its HTTP interface.
    ClickHouse,
    /// A PostgreSQL server on its native protocol (8a).
    Postgres,
    /// A MongoDB server or replica set (8b).
    MongoDb,
}

impl SourceKind {
    /// Gives the kind as the text of the configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::ClickHouse => "clickhouse",
            SourceKind::Postgres => "postgres",
            SourceKind::MongoDb => "mongodb",
        }
    }
}

/// The state of a source (8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceState {
    /// No probe finished yet.
    Unknown,
    /// The last probe succeeded.
    Up,
    /// The account or the certificate breaks the contract of 8.12 or 8.6.
    Misconfigured,
    /// The source did not answer.
    Down,
}

impl SourceState {
    /// Gives the state as text, for the log and the metrics.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceState::Unknown => "unknown",
            SourceState::Up => "up",
            SourceState::Misconfigured => "misconfigured",
            SourceState::Down => "down",
        }
    }

    pub(crate) fn to_u8(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_u8(v: u8) -> SourceState {
        match v {
            1 => SourceState::Up,
            2 => SourceState::Misconfigured,
            3 => SourceState::Down,
            _ => SourceState::Unknown,
        }
    }
}

impl fmt::Display for SourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The result of one probe (8.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The state after the probe.
    pub state: SourceState,
    /// The state before the probe.
    pub previous: SourceState,
    /// The ClickHouse version text, when the probe read it.
    pub version: Option<String>,
    /// `Some(false)` when the version is below the oldest supported release (8.13).
    pub version_supported: Option<bool>,
    /// One English sentence that tells the reason of a state other than `up`. It holds no secret.
    pub detail: Option<String>,
}

impl Probe {
    /// Tells if the probe changed the state.
    pub fn changed(&self) -> bool {
        self.state != self.previous
    }
}

/// The kind of a query that runs for a person (8.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryKind {
    /// The query of a panel.
    Panel,
    /// The options query of a variable.
    Options,
    /// The query of an annotation (7.37).
    Annotation,
    /// The preview of a draft.
    Preview,
}

impl QueryKind {
    /// Gives the text of the member `kind` of `log_comment` and of the log line.
    pub fn as_str(self) -> &'static str {
        match self {
            QueryKind::Panel => "panel",
            QueryKind::Options => "options",
            QueryKind::Annotation => "annotation",
            QueryKind::Preview => "preview",
        }
    }
}

/// The subject of a request (11): the admission key of 8.8. `-` in mode `open`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubjectId(String);

impl SubjectId {
    /// Makes a subject from its text.
    pub fn new(s: impl Into<String>) -> SubjectId {
        SubjectId(s.into())
    }

    /// Gives the subject of mode `open`: `-`.
    pub fn open() -> SubjectId {
        SubjectId("-".to_owned())
    }

    /// Gives the text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A W3C `traceparent` header value: `00-<32 hex>-<16 hex>-<2 hex>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceParent(String);

impl TraceParent {
    /// Checks the text and keeps it. Gives `None` for a text of another form.
    pub fn parse(s: &str) -> Option<TraceParent> {
        let parts: Vec<&str> = s.split('-').collect();
        let hex = |p: &str, n: usize| p.len() == n && p.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
        let ok = parts.len() == 4
            && parts[0] == "00"
            && hex(parts[1], 32)
            && hex(parts[2], 16)
            && hex(parts[3], 2)
            && parts[1].bytes().any(|c| c != b'0')
            && parts[2].bytes().any(|c| c != b'0');
        ok.then(|| TraceParent(s.to_owned()))
    }

    /// Gives the header value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The text that the engine sends to the source (7.11). A new kind of source adds a variant.
#[derive(Debug, Clone, PartialEq)]
pub enum PreparedQuery {
    /// One ClickHouse SQL statement.
    ClickHouseSql {
        /// The final SQL text. It holds the query parameters `{name:Type}` and no value of them (7.6a).
        sql: String,
        /// The values of the query parameters of the final text (7.6a). The client sends them as `param_<name>`.
        params: hune_core::sql::QueryParams,
    },
    /// One PostgreSQL SQL statement (8a.2, 8a.5). It has no parameter: the text is final.
    PostgresSql {
        /// The final SQL text, without the attribution comment of 8a.11.
        sql: String,
    },
    /// One MongoDB read (7.27 to 7.29, 8b.5). The tree is the expanded query: the typed values of
    /// the variables are in it already (7.35). The source adds the database default, the last
    /// stage `$limit` and the options of the command.
    MongoQuery {
        /// The parsed and expanded top object.
        command: MongoQuery,
    },
}

/// The effective limits of one query (7.2, 8.1, 11.24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveLimits {
    /// The time limit, in whole seconds. A fraction rounds up when the source sends it.
    pub max_execution_time: Duration,
    /// The row limit of the result.
    pub max_result_rows: u64,
    /// The byte limit of the result.
    pub max_result_bytes: u64,
    /// The size limit of the SQL.
    pub max_query_size: u64,
    /// True when the ceiling `max_rows` is the effective `max_result_rows` (11.24).
    pub rows_from_ceiling: bool,
    /// True when the ceiling `max_duration` is the effective `max_execution_time` (11.24).
    pub time_from_ceiling: bool,
}

/// The `kind` member of `log_comment` (8.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    /// A panel query.
    Panel,
    /// An options query.
    Options,
    /// An annotation query (7.37).
    Annotation,
    /// A preview.
    Preview,
    /// A probe of the source.
    Probe,
}

impl LogKind {
    fn as_str(self) -> &'static str {
        match self {
            LogKind::Panel => "panel",
            LogKind::Options => "options",
            LogKind::Annotation => "annotation",
            LogKind::Preview => "preview",
            LogKind::Probe => "probe",
        }
    }
}

impl From<QueryKind> for LogKind {
    fn from(k: QueryKind) -> LogKind {
        match k {
            QueryKind::Panel => LogKind::Panel,
            QueryKind::Options => LogKind::Options,
            QueryKind::Annotation => LogKind::Annotation,
            QueryKind::Preview => LogKind::Preview,
        }
    }
}

/// The attribution comment of 8.10. [`LogComment::to_json`] gives the compact JSON that
/// Hune sends as the setting `log_comment`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogComment {
    /// The member `kind`.
    pub kind: LogKind,
    /// The uid of the dashboard. Absent for a probe and for the preview of a new dashboard.
    pub uid: Option<String>,
    /// The folder of the body, for the preview of a new dashboard. It comes in place of `uid`.
    pub folder: Option<String>,
    /// The panel id. For the kind `annotation`, it is the index of the annotation, and the member has the
    /// name `annotation` (8.10).
    pub panel: Option<u32>,
    /// The ref of the query of a panel that has several queries (4.15a, 6.20). It follows the grammar of 4.3, so
    /// it needs no escape. Absent when the request has no ref.
    pub query_ref: Option<String>,
    /// The variable name. It comes in place of `panel`.
    pub variable: Option<String>,
    /// The request id of 6.3. Absent for a probe.
    pub request_id: Option<String>,
    /// The subject. Absent for a probe. The text is cut to 256 bytes.
    pub subject: Option<String>,
}

/// The size limit of `log_comment` in bytes (8.10).
pub const LOG_COMMENT_MAX_BYTES: usize = 1024;
/// The size limit of the member `subject` in bytes (8.10).
pub const LOG_COMMENT_SUBJECT_MAX_BYTES: usize = 256;

impl LogComment {
    /// Makes the comment of a probe: `{"app":"hune","kind":"probe"}`.
    pub fn probe() -> LogComment {
        LogComment {
            kind: LogKind::Probe,
            uid: None,
            folder: None,
            panel: None,
            query_ref: None,
            variable: None,
            request_id: None,
            subject: None,
        }
    }

    /// Writes the compact JSON of 8.10, with the members in this order: `app`, `kind`, `uid`,
    /// `folder`, `panel`, `ref`, `variable`, `request_id`, `subject`.
    ///
    /// The result has 1,024 bytes at most. When it is larger, the function shortens `subject`
    /// first, then it drops `subject`.
    pub fn to_json(&self) -> String {
        let mut limit = self.subject.as_deref().map_or(0, |s| cut_utf8(s, LOG_COMMENT_SUBJECT_MAX_BYTES).len());
        loop {
            let json = self.render(limit);
            if json.len() <= LOG_COMMENT_MAX_BYTES || limit == 0 {
                return json;
            }
            limit /= 2;
        }
    }

    fn render(&self, subject_limit: usize) -> String {
        fn text(s: &mut String, name: &str, v: &str) {
            s.push_str(",\"");
            s.push_str(name);
            s.push_str("\":");
            hune_core::frame::write_json_string(s, v);
        }
        let mut s = String::from(r#"{"app":"hune","kind":""#);
        s.push_str(self.kind.as_str());
        s.push('"');
        if let Some(v) = &self.uid {
            text(&mut s, "uid", v);
        }
        if let Some(v) = &self.folder {
            text(&mut s, "folder", v);
        }
        if let Some(p) = self.panel {
            let name = if self.kind == LogKind::Annotation { "annotation" } else { "panel" };
            s.push_str(&format!(",\"{name}\":{p}"));
        }
        if let Some(v) = &self.query_ref {
            text(&mut s, "ref", v);
        }
        if let Some(v) = &self.variable {
            text(&mut s, "variable", v);
        }
        if let Some(v) = &self.request_id {
            text(&mut s, "request_id", v);
        }
        if let (Some(v), true) = (&self.subject, subject_limit > 0) {
            text(&mut s, "subject", cut_utf8(v, subject_limit));
        }
        s.push('}');
        s
    }
}

/// Cuts `s` to at most `max` bytes at a character boundary.
pub(crate) fn cut_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// The time that one query waited in the admission queue of its source (8.8).
///
/// The clones of one value share the same cell. The caller keeps a clone in the [`QueryCtx`] and
/// reads it after the query ends, also when the query is refused or when the caller drops the
/// future. The source sets the value one time: when the query gets its permits, or when the
/// admission refuses it. A query that never reaches the admission leaves the value unset.
#[derive(Debug, Clone)]
pub struct QueueWait(Arc<AtomicU64>);

impl QueueWait {
    const UNSET: u64 = u64::MAX;

    /// Makes a cell with no value.
    pub fn new() -> QueueWait {
        QueueWait(Arc::new(AtomicU64::new(Self::UNSET)))
    }

    /// Gives the wait, or `None` when the query never reached the admission.
    pub fn get(&self) -> Option<Duration> {
        match self.0.load(Ordering::Acquire) {
            Self::UNSET => None,
            nanos => Some(Duration::from_nanos(nanos)),
        }
    }

    /// Sets the wait. A wait of more than 584 years is cut to the largest value that fits.
    pub(crate) fn set(&self, wait: Duration) {
        let nanos = u64::try_from(wait.as_nanos()).unwrap_or(Self::UNSET - 1).min(Self::UNSET - 1);
        self.0.store(nanos, Ordering::Release);
    }
}

impl Default for QueueWait {
    fn default() -> QueueWait {
        QueueWait::new()
    }
}

/// The context of one query (8.1).
#[derive(Debug, Clone)]
pub struct QueryCtx {
    /// The kind of the query.
    pub kind: QueryKind,
    /// The query id: a UUID version 7 (8.10).
    pub query_id: Uuid,
    /// The attribution comment (8.10).
    pub log_comment: LogComment,
    /// The subject: the admission key of 8.8.
    pub subject: SubjectId,
    /// The W3C trace context, when telemetry is on.
    pub traceparent: Option<TraceParent>,
    /// The cell where the source puts the time in the admission queue (8.8). Keep a clone to read it.
    pub queue_wait: QueueWait,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v7_layout() {
        let u = Uuid::new_v7(0x0123_4567_89ab, [0xff; 10]);
        assert_eq!(u.to_string(), "01234567-89ab-7fff-bfff-ffffffffffff");
        assert_eq!(u.version(), 7);
        assert_eq!(u.unix_ms(), 0x0123_4567_89ab);
        let z = Uuid::new_v7(1_794_211_200_000, [0; 10]);
        assert_eq!(&z.to_string()[14..], "7000-8000-000000000000");
    }

    #[test]
    fn uuid_text_round_trip() {
        let t = "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d";
        let u = Uuid::parse(t).unwrap();
        assert_eq!(u.to_string(), t);
        assert_eq!(Uuid::parse(&t.to_uppercase()).unwrap(), u);
        assert!(Uuid::parse("019a6b3c6f1e7d2a9b4c1e2f3a4b5c6d").is_none());
        assert!(Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6g").is_none());
        assert!(Uuid::parse("").is_none());
    }

    #[test]
    fn now_v7_is_v7_and_unique() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        assert_ne!(a, b);
        assert_eq!(a.version(), 7);
        assert_eq!(a.to_string().len(), 36);
        assert_eq!(a.as_bytes()[8] >> 6, 0b10);
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        assert!(ms.abs_diff(a.unix_ms()) < 5000);
        // A later query id sorts after an earlier one by its time.
        assert!(b.unix_ms() >= a.unix_ms());
    }

    #[test]
    fn traceparent_check() {
        assert!(TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").is_some());
        assert!(TraceParent::parse("00-00000000000000000000000000000000-00f067aa0ba902b7-01").is_none());
        assert!(TraceParent::parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").is_none());
        assert!(TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7").is_none());
        assert!(TraceParent::parse("00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01").is_none());
        assert!(TraceParent::parse("x\r\ny").is_none());
    }

    #[test]
    fn log_comment_of_8_10() {
        let c = LogComment {
            kind: LogKind::Panel,
            uid: Some("nodes".into()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r-5f2c".into()),
            subject: Some("user:alice".into()),
        };
        assert_eq!(
            c.to_json(),
            r#"{"app":"hune","kind":"panel","uid":"nodes","panel":4,"request_id":"r-5f2c","subject":"user:alice"}"#
        );
        assert_eq!(LogComment::probe().to_json(), r#"{"app":"hune","kind":"probe"}"#);
        let p = LogComment {
            kind: LogKind::Preview,
            uid: None,
            folder: Some("transit".into()),
            panel: None,
            query_ref: None,
            variable: Some("node".into()),
            request_id: Some("r-1".into()),
            subject: Some("-".into()),
        };
        assert_eq!(
            p.to_json(),
            r#"{"app":"hune","kind":"preview","folder":"transit","variable":"node","request_id":"r-1","subject":"-"}"#
        );
        assert_eq!(LogKind::from(QueryKind::Options), LogKind::Options);
        // The ref of a query of a panel with several queries comes after `panel` (8.10).
        let mut r = c.clone();
        r.query_ref = Some("B".into());
        assert_eq!(
            r.to_json(),
            r#"{"app":"hune","kind":"panel","uid":"nodes","panel":4,"ref":"B","request_id":"r-5f2c","subject":"user:alice"}"#
        );
    }

    #[test]
    fn log_comment_is_bounded() {
        let mut c = LogComment::probe();
        c.kind = LogKind::Panel;
        c.uid = Some("u".into());
        c.request_id = Some("r".into());
        // 300 bytes of two-byte characters: cut to 256 bytes at a character boundary.
        c.subject = Some("é".repeat(150));
        let json = c.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["subject"].as_str().unwrap().len(), 256);
        // A subject made of control characters grows 6 times when escaped: the comment stays small.
        c.subject = Some("\u{1}".repeat(256));
        let json = c.to_json();
        assert!(json.len() <= LOG_COMMENT_MAX_BYTES, "{}", json.len());
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
        // A long uid and variable leave no room: the subject goes away, the JSON stays valid.
        c.uid = Some("a".repeat(600));
        c.variable = Some("b".repeat(400));
        let json = c.to_json();
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
        assert!(!json.contains("subject"));
    }
}
