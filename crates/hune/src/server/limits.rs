//! The limits of the HTTP server (6.27, 12.16, 14.2, 14.14). They are constants, not keys.

use std::time::Duration;

/// The longest request target (6.27): 8 KiB.
pub const MAX_TARGET_BYTES: usize = 8 * 1024;
/// The most headers in one request (6.27).
pub const MAX_HEADERS: usize = 100;
/// The most bytes in all headers of one request (6.27): 16 KiB.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
/// The buffer limit of the HTTP parser. A head above the limits of 6.27 but below this value
/// gets a JSON answer. A larger head gets the bare answer of the parser.
pub const PARSER_HEAD_BYTES: usize = 64 * 1024;
/// The most headers that the parser reads. One more than `MAX_HEADERS` is enough to detect the fault.
pub const PARSER_MAX_HEADERS: usize = 256;
/// The time to read the request line and the headers (6.27).
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
/// The time to read a body (6.27).
pub const BODY_TIMEOUT: Duration = Duration::from_secs(30);
/// The body limit of a panel query and of an options query (6.27).
pub const QUERY_BODY_BYTES: u64 = 262_144;
/// The body limit of a preview (6.27).
pub const PREVIEW_BODY_BYTES: u64 = 524_288;
/// The body limit of a restore, a move and a folder route (6.27).
pub const SMALL_BODY_BYTES: u64 = 16_384;
/// The most bodies of a create, a save or an import in flight (6.27).
pub const MAX_INFLIGHT_DOC_BODIES: usize = 8;
/// The most other bodies in flight (6.27).
pub const MAX_INFLIGHT_SMALL_BODIES: usize = 256;
/// The most validations of a document that run or wait at the same time in `spawn_blocking` (6.27, 12.16).
pub const MAX_INFLIGHT_VALIDATIONS: usize = 4;
/// The longest wait of a validation for a permit (6.27). Past it, the request gets `503 server_busy`.
pub const VALIDATION_WAIT: Duration = Duration::from_secs(5);
/// The most document bodies, and the most validations, of one subject in flight in the mode `authzen` (12.16).
pub const MAX_INFLIGHT_PER_SUBJECT: usize = 2;
/// The depth limit of a JSON request body (6.2).
pub const MAX_BODY_JSON_DEPTH: usize = 64;
/// The time during which `/readyz` answers 503 and the listener still serves (6.28, 14.13).
pub const SHUTDOWN_DRAIN: Duration = Duration::from_secs(5);
/// The connections of the ops listener (14.2).
pub const OPS_MAX_CONNECTIONS: usize = 64;
/// The time to read the headers on the ops listener (14.2).
pub const OPS_HEADER_TIMEOUT: Duration = Duration::from_secs(10);
/// The number of refusal answers that Hune writes at the same time above `max_connections`.
pub const MAX_REFUSAL_TASKS: usize = 64;
/// The time to write one refusal answer.
pub const REFUSAL_WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// The limits that `GET /api/session` gives (6.10): the SQL text (4.35).
pub const SESSION_SQL_BYTES: u64 = 262_144;
/// The limits that `GET /api/session` gives (6.10): the variable values (4.35).
pub const SESSION_VARIABLE_VALUES: u64 = 10_000;
/// The limits that `GET /api/session` gives (6.10): one value (4.35).
pub const SESSION_VALUE_BYTES: u64 = 1024;
/// The limits that `GET /api/session` gives (6.10): `max_data_points` (7.4).
pub const SESSION_MAX_DATA_POINTS: u64 = 10_000;

/// The times that a test can shorten. The product uses `Timing::default()`.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// The time to read the request line and the headers (6.27).
    pub header_timeout: Duration,
    /// The time to read a body (6.27).
    pub body_timeout: Duration,
    /// The drain of a stop (6.28, 14.13).
    pub drain: Duration,
    /// Tells if the probe tasks of the sources run (8.11). A test that has no source switches it off.
    pub probe_sources: bool,
    /// The time between two probes of a source (8.11).
    pub probe_interval: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            header_timeout: HEADER_TIMEOUT,
            body_timeout: BODY_TIMEOUT,
            drain: SHUTDOWN_DRAIN,
            probe_sources: true,
            probe_interval: hune_source::PROBE_INTERVAL,
        }
    }
}
