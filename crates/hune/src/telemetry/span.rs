//! The span model (14.10): ids, the W3C `traceparent` header, attributes and the span data.
//!
//! The model holds no I/O. A caller gives the times, or the telemetry module reads the clock.

use std::time::{SystemTime, UNIX_EPOCH};

/// The longest text of a string attribute or of a span name, in bytes.
pub const MAX_TEXT_BYTES: usize = 256;

/// A trace id of 16 bytes. It is never all zero.
pub type TraceId = [u8; 16];
/// A span id of 8 bytes. It is never all zero.
pub type SpanId = [u8; 8];

/// The kind of a span. The numbers are the values of the OTLP enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// A request that Hune serves.
    Server,
    /// A call that Hune makes.
    Client,
}

impl SpanKind {
    /// Gives the number of the OTLP enum `SpanKind`.
    pub fn otlp_number(self) -> u8 {
        match self {
            SpanKind::Server => 2,
            SpanKind::Client => 3,
        }
    }
}

/// The status of a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStatus {
    /// The span has no status.
    Unset,
    /// The span failed.
    Error,
}

impl SpanStatus {
    /// Gives the number of the OTLP enum `StatusCode`.
    pub fn otlp_number(self) -> u8 {
        match self {
            SpanStatus::Unset => 0,
            SpanStatus::Error => 2,
        }
    }
}

/// The value of an attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    /// A text.
    Str(String),
    /// An integer.
    Int(i64),
    /// A boolean.
    #[allow(dead_code)] // Part of the span API.
    Bool(bool),
}

/// One attribute of a span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// The key. It is in the closed list of the span type.
    pub key: &'static str,
    /// The value.
    pub value: AttrValue,
}

/// The type of a span in the table of 14.10. It fixes the closed list of attribute keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanType {
    /// A request of `server.listen`.
    Request,
    /// A query to ClickHouse.
    ClickhouseQuery,
    /// A query to PostgreSQL. It has the same attribute keys as a query to ClickHouse (14.10).
    PostgresQuery,
    /// A query to MongoDB. It has the same attribute keys as a query to ClickHouse (14.10).
    MongoQuery,
    /// An operation of the store.
    StoreOperation,
    /// A request to the PDP.
    PdpEvaluate,
}

const REQUEST_KEYS: &[&str] = &[
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "client.address",
    "hune.request_id",
    "hune.dashboard.uid",
    "hune.panel.id",
    "error.type",
];
const CLICKHOUSE_KEYS: &[&str] =
    &["db.system.name", "server.address", "server.port", "hune.source", "hune.query_id", "hune.rows", "error.type"];
const STORE_KEYS: &[&str] = &["hune.store.backend", "db.system.name"];
const PDP_KEYS: &[&str] = &["server.address", "hune.questions"];

impl SpanType {
    /// Gives the attribute keys that a span of this type can hold (14.10, 12.15).
    pub fn allowed_keys(self) -> &'static [&'static str] {
        match self {
            SpanType::Request => REQUEST_KEYS,
            SpanType::ClickhouseQuery | SpanType::PostgresQuery | SpanType::MongoQuery => CLICKHOUSE_KEYS,
            SpanType::StoreOperation => STORE_KEYS,
            SpanType::PdpEvaluate => PDP_KEYS,
        }
    }

    /// Gives the kind of a span of this type.
    pub fn kind(self) -> SpanKind {
        match self {
            SpanType::Request => SpanKind::Server,
            _ => SpanKind::Client,
        }
    }
}

/// A finished or running span, as the exporter holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanData {
    /// The trace id.
    pub trace_id: TraceId,
    /// The span id.
    pub span_id: SpanId,
    /// The id of the parent span, when the span has a parent.
    pub parent: Option<SpanId>,
    /// The kind.
    pub kind: SpanKind,
    /// The name.
    pub name: String,
    /// The start time, in Unix nanoseconds.
    pub start_unix_nanos: u64,
    /// The end time, in Unix nanoseconds.
    pub end_unix_nanos: u64,
    /// The attributes, in the order of the first set.
    pub attributes: Vec<Attr>,
    /// The status.
    pub status: SpanStatus,
}

/// The trace context that a span gives to a child or to a remote call: the W3C `traceparent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceContext {
    /// The trace id.
    pub trace_id: TraceId,
    /// The id of the span that the context names.
    pub span_id: SpanId,
    /// The trace flags. Bit 0 is the sampled flag.
    pub flags: u8,
}

const TRACEPARENT_LEN: usize = 55;

impl TraceContext {
    /// Parses a `traceparent` value. The parse is strict: version `00`, lower-case hex, the exact
    /// length, and no all-zero id. A value that does not pass gives `None`.
    pub fn parse(text: &str) -> Option<TraceContext> {
        let b = text.as_bytes();
        if b.len() != TRACEPARENT_LEN || b[2] != b'-' || b[35] != b'-' || b[52] != b'-' {
            return None;
        }
        if &b[0..2] != b"00" {
            return None;
        }
        let mut trace_id = [0u8; 16];
        let mut span_id = [0u8; 8];
        let mut flags = [0u8; 1];
        decode_hex(&b[3..35], &mut trace_id)?;
        decode_hex(&b[36..52], &mut span_id)?;
        decode_hex(&b[53..55], &mut flags)?;
        if trace_id == [0u8; 16] || span_id == [0u8; 8] {
            return None;
        }
        Some(TraceContext { trace_id, span_id, flags: flags[0] })
    }

    /// Writes the `traceparent` value: version `00`, lower-case hex.
    pub fn format(&self) -> String {
        format!("00-{}-{}-{:02x}", hex(&self.trace_id), hex(&self.span_id), self.flags)
    }
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Decodes lower-case hex. The input has exactly two bytes for each output byte.
fn decode_hex(input: &[u8], out: &mut [u8]) -> Option<()> {
    if input.len() != out.len() * 2 {
        return None;
    }
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = (nibble(input[2 * i])? << 4) | nibble(input[2 * i + 1])?;
    }
    Some(())
}

/// Writes bytes as lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(DIGITS[usize::from(b >> 4)]));
        s.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    s
}

/// Fills a buffer with random bytes. A failure of the system source gives `None`.
pub(crate) fn random_bytes<const N: usize>() -> Option<[u8; N]> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).ok()?;
    Some(out)
}

/// Makes a random trace id and span id, none of them all zero.
pub(crate) fn new_root_ids() -> Option<(TraceId, SpanId)> {
    loop {
        let bytes = random_bytes::<24>()?;
        let mut trace = [0u8; 16];
        let mut span = [0u8; 8];
        trace.copy_from_slice(&bytes[..16]);
        span.copy_from_slice(&bytes[16..]);
        if trace != [0u8; 16] && span != [0u8; 8] {
            return Some((trace, span));
        }
    }
}

/// Makes a random span id that is not all zero.
pub(crate) fn new_span_id() -> Option<SpanId> {
    loop {
        let id = random_bytes::<8>()?;
        if id != [0u8; 8] {
            return Some(id);
        }
    }
}

/// Makes a random version 4 UUID in its text form.
pub(crate) fn new_uuid() -> Option<String> {
    let mut b = random_bytes::<16>()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex(&b);
    Some(format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]))
}

/// Gives the Unix time in nanoseconds. A clock before 1970 gives 0.
pub fn now_unix_nanos() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_nanos()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

/// Cuts a text to `MAX_TEXT_BYTES` bytes at a character boundary.
pub(crate) fn clip(text: &str) -> String {
    if text.len() <= MAX_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
