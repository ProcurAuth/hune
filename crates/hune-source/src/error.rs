//! The errors of a source: one variant for each Hune code of 8.5, and the setup error.

use std::fmt;

use hune_core::ids::SourceId;

use crate::types::{EffectiveLimits, Uuid, cut_utf8};

/// The size limit of the message of a ClickHouse refusal, in bytes (6.7, 8.4).
pub const MESSAGE_MAX_BYTES: usize = 1000;

/// The value of the member `limit` of `query_limit` (8.5): a closed list of five values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitName {
    /// ClickHouse code 396: the rows or the bytes of the result.
    MaxResult,
    /// The row check of 8.4.
    MaxResultRows,
    /// The byte check of 8.4.
    MaxResultBytes,
    /// ClickHouse code 158.
    MaxRowsToRead,
    /// ClickHouse code 307.
    MaxBytesToRead,
}

impl LimitName {
    /// Gives the text of the member `limit`.
    pub fn as_str(self) -> &'static str {
        match self {
            LimitName::MaxResult => "max_result",
            LimitName::MaxResultRows => "max_result_rows",
            LimitName::MaxResultBytes => "max_result_bytes",
            LimitName::MaxRowsToRead => "max_rows_to_read",
            LimitName::MaxBytesToRead => "max_bytes_to_read",
        }
    }
}

/// The ceiling of a `ceiling_exceeded` error (6.8, 11.24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ceiling {
    /// The ceiling `max_rows` lowered `max_result_rows`.
    MaxRows,
    /// The ceiling `max_duration` lowered `max_execution_time`.
    MaxDuration,
}

impl Ceiling {
    /// Gives the text of the member `ceiling`.
    pub fn as_str(self) -> &'static str {
        match self {
            Ceiling::MaxRows => "max_rows",
            Ceiling::MaxDuration => "max_duration",
        }
    }
}

/// One extra member of an error body (6.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    /// A JSON string.
    Text(String),
    /// A non-negative JSON number.
    Uint(u64),
    /// A JSON number.
    Int(i64),
    /// The JSON value `null`.
    Null,
}

/// The error of a query or a probe. One variant for each Hune code of 8.5.
///
/// The messages hold no secret and no row of data. A ClickHouse refusal keeps the first line of
/// the ClickHouse message, cut to 1,000 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// `504 query_timeout`.
    QueryTimeout {
        /// The source id.
        source: SourceId,
        /// The query id.
        query_id: Uuid,
        /// The English message.
        message: String,
    },
    /// `403 ceiling_exceeded`. The server adds the member `action`.
    CeilingExceeded {
        /// The ceiling that was the cause.
        ceiling: Ceiling,
        /// The effective value: the rows or the seconds.
        limit: u64,
        /// The English message.
        message: String,
    },
    /// `422 query_limit`.
    QueryLimit {
        /// The member `limit`.
        limit: LimitName,
        /// The member `value`. `None` gives `null`.
        value: Option<u64>,
        /// The English message.
        message: String,
    },
    /// `422 query_memory_limit` (ClickHouse code 241).
    QueryMemoryLimit {
        /// The first line of the ClickHouse message.
        message: String,
    },
    /// `502 source_access_denied` (ClickHouse code 497).
    SourceAccessDenied {
        /// The source id.
        source: SourceId,
        /// The query id.
        query_id: Uuid,
        /// The first line of the ClickHouse message.
        message: String,
    },
    /// `502 source_misconfigured`.
    SourceMisconfigured {
        /// The source id.
        source: SourceId,
        /// The English message.
        message: String,
    },
    /// `503 source_unavailable`.
    SourceUnavailable {
        /// The source id.
        source: SourceId,
        /// The English message.
        message: String,
    },
    /// `503 source_busy`. The answer has `Retry-After: 1`.
    SourceBusy {
        /// The source id.
        source: SourceId,
        /// The English message.
        message: String,
    },
    /// `502 source_bad_response`.
    SourceBadResponse {
        /// The source id.
        source: SourceId,
        /// The query id.
        query_id: Uuid,
        /// The English message. It never holds a value of a row.
        message: String,
    },
    /// `422 query_error`.
    QueryError {
        /// The source id.
        source: SourceId,
        /// The ClickHouse error code.
        clickhouse_code: i64,
        /// The query id.
        query_id: Uuid,
        /// The first line of the ClickHouse message.
        message: String,
    },
    /// `422 query_error` of a PostgreSQL source (8a.8, ID-PG12). The member `sqlstate` takes the
    /// place of `clickhouse_code`. It is `None` (JSON `null`) when the error has no SQLSTATE.
    QuerySqlError {
        /// The source id.
        source: SourceId,
        /// The SQLSTATE of the server error, five characters.
        sqlstate: Option<String>,
        /// The query id.
        query_id: Uuid,
        /// The first line of the primary message of the server error.
        message: String,
    },
    /// `422 query_error` of a MongoDB source (8b.8, ID-MG12). The member `mongo_code` takes the place of
    /// `clickhouse_code` and `sqlstate`. It is `None` (JSON `null`) when the error comes from Hune and
    /// not from the server: a type that 7.16c does not allow, a mix of types, too many columns.
    QueryMongoError {
        /// The source id.
        source: SourceId,
        /// The server error code.
        mongo_code: Option<i64>,
        /// The query id.
        query_id: Uuid,
        /// The first line of the server message, or the English message of Hune. It never holds a value
        /// of a row when Hune wrote it.
        message: String,
    },
    /// `422 query_error` of a ClickHouse source with `clickhouse_code` = `null` (8.5): a proxy in front of
    /// ClickHouse answered the HTTP status 414 or 431 with no ClickHouse code. The state of the source does not
    /// change, so long values cannot mark a source down (7.6a, ID-K330).
    ProxyRefused {
        /// The source id.
        source: SourceId,
        /// The HTTP status: 414 or 431.
        status: u16,
        /// The query id.
        query_id: Uuid,
        /// The English message.
        message: String,
    },
    /// `500 internal_error`: a fault of Hune, for example a refusal of the wire guard (8b.5). The
    /// message is fixed and holds no query text.
    Internal {
        /// The English message.
        message: String,
    },
    /// ClickHouse codes 394 and 735: the client left. The server sends no answer (8.5, 8.9).
    /// The log line has the outcome `cancelled` and the status 499.
    Cancelled,
}

impl SourceError {
    /// Gives the Hune code of 6.8. A client that left gives `cancelled`, which is not an API code.
    pub fn code(&self) -> &'static str {
        match self {
            SourceError::QueryTimeout { .. } => "query_timeout",
            SourceError::CeilingExceeded { .. } => "ceiling_exceeded",
            SourceError::QueryLimit { .. } => "query_limit",
            SourceError::QueryMemoryLimit { .. } => "query_memory_limit",
            SourceError::SourceAccessDenied { .. } => "source_access_denied",
            SourceError::SourceMisconfigured { .. } => "source_misconfigured",
            SourceError::SourceUnavailable { .. } => "source_unavailable",
            SourceError::SourceBusy { .. } => "source_busy",
            SourceError::SourceBadResponse { .. } => "source_bad_response",
            SourceError::QueryError { .. }
            | SourceError::QuerySqlError { .. }
            | SourceError::QueryMongoError { .. }
            | SourceError::ProxyRefused { .. } => "query_error",
            SourceError::Internal { .. } => "internal_error",
            SourceError::Cancelled => "cancelled",
        }
    }

    /// Gives the HTTP status of the code in 6.8. `Cancelled` gives 499, the status of the log line.
    pub fn http_status(&self) -> u16 {
        match self {
            SourceError::QueryTimeout { .. } => 504,
            SourceError::CeilingExceeded { .. } => 403,
            SourceError::QueryLimit { .. }
            | SourceError::QueryMemoryLimit { .. }
            | SourceError::QueryError { .. }
            | SourceError::QuerySqlError { .. }
            | SourceError::QueryMongoError { .. }
            | SourceError::ProxyRefused { .. } => 422,
            SourceError::SourceAccessDenied { .. }
            | SourceError::SourceMisconfigured { .. }
            | SourceError::SourceBadResponse { .. } => 502,
            SourceError::SourceUnavailable { .. } | SourceError::SourceBusy { .. } => 503,
            SourceError::Internal { .. } => 500,
            SourceError::Cancelled => 499,
        }
    }

    /// Gives the member `retryable` of 6.7.
    pub fn retryable(&self) -> bool {
        matches!(self, SourceError::SourceUnavailable { .. } | SourceError::SourceBusy { .. })
    }

    /// Gives the value of the header `Retry-After` in seconds, for `source_busy`.
    pub fn retry_after_secs(&self) -> Option<u32> {
        matches!(self, SourceError::SourceBusy { .. }).then_some(1)
    }

    /// Gives the English message of the member `message` (6.7). It is empty for `Cancelled`.
    pub fn message(&self) -> &str {
        match self {
            SourceError::QueryTimeout { message, .. }
            | SourceError::CeilingExceeded { message, .. }
            | SourceError::QueryLimit { message, .. }
            | SourceError::QueryMemoryLimit { message }
            | SourceError::SourceAccessDenied { message, .. }
            | SourceError::SourceMisconfigured { message, .. }
            | SourceError::SourceUnavailable { message, .. }
            | SourceError::SourceBusy { message, .. }
            | SourceError::SourceBadResponse { message, .. }
            | SourceError::QueryError { message, .. }
            | SourceError::QuerySqlError { message, .. }
            | SourceError::QueryMongoError { message, .. }
            | SourceError::ProxyRefused { message, .. }
            | SourceError::Internal { message } => message,
            SourceError::Cancelled => "",
        }
    }

    /// Gives the member `source`, when the code has it.
    pub fn source_id(&self) -> Option<&SourceId> {
        match self {
            SourceError::QueryTimeout { source, .. }
            | SourceError::SourceAccessDenied { source, .. }
            | SourceError::SourceMisconfigured { source, .. }
            | SourceError::SourceUnavailable { source, .. }
            | SourceError::SourceBusy { source, .. }
            | SourceError::SourceBadResponse { source, .. }
            | SourceError::QueryError { source, .. }
            | SourceError::QuerySqlError { source, .. }
            | SourceError::QueryMongoError { source, .. }
            | SourceError::ProxyRefused { source, .. } => Some(source),
            _ => None,
        }
    }

    /// Gives the member `query_id`, when the code has it.
    pub fn query_id(&self) -> Option<Uuid> {
        match self {
            SourceError::QueryTimeout { query_id, .. }
            | SourceError::SourceAccessDenied { query_id, .. }
            | SourceError::SourceBadResponse { query_id, .. }
            | SourceError::QueryError { query_id, .. }
            | SourceError::QuerySqlError { query_id, .. }
            | SourceError::QueryMongoError { query_id, .. }
            | SourceError::ProxyRefused { query_id, .. } => Some(*query_id),
            _ => None,
        }
    }

    /// Gives the member `clickhouse_code` of `query_error`.
    pub fn clickhouse_code(&self) -> Option<i64> {
        match self {
            SourceError::QueryError { clickhouse_code, .. } => Some(*clickhouse_code),
            _ => None,
        }
    }

    /// Gives the member `sqlstate` of the `query_error` of a PostgreSQL source. The outer `Option`
    /// tells if the error has the member. The inner `Option` is the value: `None` is JSON `null`.
    pub fn sqlstate(&self) -> Option<Option<&str>> {
        match self {
            SourceError::QuerySqlError { sqlstate, .. } => Some(sqlstate.as_deref()),
            _ => None,
        }
    }

    /// Gives the member `mongo_code` of the `query_error` of a MongoDB source. The outer `Option` tells if
    /// the error has the member. The inner `Option` is the value: `None` is JSON `null`.
    pub fn mongo_code(&self) -> Option<Option<i64>> {
        match self {
            SourceError::QueryMongoError { mongo_code, .. } => Some(*mongo_code),
            _ => None,
        }
    }

    /// Gives the member `limit` of `query_limit` as text.
    pub fn limit_name(&self) -> Option<&'static str> {
        match self {
            SourceError::QueryLimit { limit, .. } => Some(limit.as_str()),
            _ => None,
        }
    }

    /// Gives the member `limit` of `ceiling_exceeded`: the effective rows or seconds.
    pub fn ceiling_limit(&self) -> Option<u64> {
        match self {
            SourceError::CeilingExceeded { limit, .. } => Some(*limit),
            _ => None,
        }
    }

    /// Gives the member `value` of `query_limit`. `None` is the JSON value `null`.
    pub fn limit_value(&self) -> Option<u64> {
        match self {
            SourceError::QueryLimit { value, .. } => *value,
            _ => None,
        }
    }

    /// Gives the member `ceiling` of `ceiling_exceeded`.
    pub fn ceiling(&self) -> Option<&'static str> {
        match self {
            SourceError::CeilingExceeded { ceiling, .. } => Some(ceiling.as_str()),
            _ => None,
        }
    }

    /// Gives the extra members of 6.8 for this code, in the order of the table. The server
    /// adds `action` to `ceiling_exceeded`.
    pub fn members(&self) -> Vec<(&'static str, Member)> {
        let mut out = Vec::new();
        if let Some(s) = self.source_id() {
            out.push(("source", Member::Text(s.as_str().to_owned())));
        }
        if let Some(c) = self.clickhouse_code() {
            out.push(("clickhouse_code", Member::Int(c)));
        }
        if matches!(self, SourceError::ProxyRefused { .. }) {
            out.push(("clickhouse_code", Member::Null));
        }
        if let Some(state) = self.sqlstate() {
            out.push(("sqlstate", state.map_or(Member::Null, |s| Member::Text(s.to_owned()))));
        }
        if let Some(code) = self.mongo_code() {
            out.push(("mongo_code", code.map_or(Member::Null, Member::Int)));
        }
        if let Some(q) = self.query_id() {
            out.push(("query_id", Member::Text(q.to_string())));
        }
        match self {
            SourceError::QueryLimit { limit, value, .. } => {
                out.push(("limit", Member::Text(limit.as_str().to_owned())));
                out.push(("value", value.map_or(Member::Null, Member::Uint)));
            }
            SourceError::CeilingExceeded { ceiling, limit, .. } => {
                out.push(("ceiling", Member::Text(ceiling.as_str().to_owned())));
                out.push(("limit", Member::Uint(*limit)));
            }
            _ => {}
        }
        out
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for SourceError {}

/// The error of the construction of a source. It happens before the process serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSetupError {
    message: String,
}

impl SourceSetupError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        SourceSetupError { message: message.into() }
    }

    /// Gives the message. It never holds the password.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SourceSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceSetupError {}

/// Gives the first line of a ClickHouse message, cut to 1,000 bytes.
pub fn first_line(message: &str) -> String {
    let line = message.lines().next().unwrap_or("").trim_end();
    cut_utf8(line, MESSAGE_MAX_BYTES).to_owned()
}

/// What the error mapping needs to know about one query.
#[derive(Debug, Clone, Copy)]
pub struct ErrorCtx<'a> {
    /// The source id.
    pub source: &'a SourceId,
    /// The query id.
    pub query_id: Uuid,
    /// The effective limits of the query.
    pub limits: &'a EffectiveLimits,
}

/// The whole seconds that Hune sends as `max_execution_time`.
pub(crate) fn exec_seconds(limits: &EffectiveLimits) -> u64 {
    let d = limits.max_execution_time;
    d.as_secs() + u64::from(d.subsec_nanos() > 0)
}

impl ErrorCtx<'_> {
    /// The error of the client deadline and of the ClickHouse codes 159 and 160 (8.5).
    pub fn timeout(&self) -> SourceError {
        let secs = exec_seconds(self.limits);
        if self.limits.time_from_ceiling {
            SourceError::CeilingExceeded {
                ceiling: Ceiling::MaxDuration,
                limit: secs,
                message: format!("The query ran longer than the limit of {secs} seconds."),
            }
        } else {
            SourceError::QueryTimeout {
                source: self.source.clone(),
                query_id: self.query_id,
                message: format!("The query ran longer than the limit of {secs} seconds."),
            }
        }
    }

    fn rows_ceiling(&self) -> SourceError {
        SourceError::CeilingExceeded {
            ceiling: Ceiling::MaxRows,
            limit: self.limits.max_result_rows,
            message: format!(
                "The result is larger than a limit: {} rows or {} bytes.",
                self.limits.max_result_rows, self.limits.max_result_bytes
            ),
        }
    }

    /// The error of the row check of 8.4.
    pub fn rows(&self) -> SourceError {
        if self.limits.rows_from_ceiling {
            self.rows_ceiling()
        } else {
            SourceError::QueryLimit {
                limit: LimitName::MaxResultRows,
                value: Some(self.limits.max_result_rows),
                message: format!("The result has more than the limit of {} rows.", self.limits.max_result_rows),
            }
        }
    }

    /// The error of the byte check of 8.4.
    pub fn bytes(&self) -> SourceError {
        SourceError::QueryLimit {
            limit: LimitName::MaxResultBytes,
            value: Some(self.limits.max_result_bytes),
            message: format!("The result is larger than the limit of {} bytes.", self.limits.max_result_bytes),
        }
    }

    /// The error of a response that breaks 8.4.
    pub fn bad_response(&self, message: impl Into<String>) -> SourceError {
        SourceError::SourceBadResponse { source: self.source.clone(), query_id: self.query_id, message: message.into() }
    }

    /// The error of a source that does not answer.
    pub fn unavailable(&self, message: impl Into<String>) -> SourceError {
        SourceError::SourceUnavailable { source: self.source.clone(), message: message.into() }
    }

    /// The error of the HTTP status 414 or 431 with no ClickHouse code (8.5): a proxy in front of ClickHouse refused
    /// a long request line or long headers. It is a `query_error`, and it changes no state of the source.
    pub fn proxy_refused(&self, status: u16) -> SourceError {
        SourceError::ProxyRefused {
            source: self.source.clone(),
            status,
            query_id: self.query_id,
            message: format!(
                "A proxy in front of the source refused the request with the HTTP status {status}: the request is too \
                 long. Use the text form for long lists."
            ),
        }
    }

    /// The error of a source that Hune must not use.
    pub fn misconfigured(&self, message: impl Into<String>) -> SourceError {
        SourceError::SourceMisconfigured { source: self.source.clone(), message: message.into() }
    }

    /// Maps a ClickHouse error code to the Hune code, with the table of 8.5.
    ///
    /// The mapping uses the code only, never the message. `message` gives the text of the
    /// ClickHouse answer: the function keeps its first line.
    pub fn map_clickhouse(&self, code: i64, message: &str) -> SourceError {
        let msg = first_line(message);
        match code {
            159 | 160 => self.timeout(),
            396 if self.limits.rows_from_ceiling => self.rows_ceiling(),
            396 => SourceError::QueryLimit { limit: LimitName::MaxResult, value: None, message: msg },
            158 => SourceError::QueryLimit { limit: LimitName::MaxRowsToRead, value: None, message: msg },
            307 => SourceError::QueryLimit { limit: LimitName::MaxBytesToRead, value: None, message: msg },
            241 => SourceError::QueryMemoryLimit { message: msg },
            497 => {
                SourceError::SourceAccessDenied { source: self.source.clone(), query_id: self.query_id, message: msg }
            }
            192 | 193 | 194 | 516 => self.misconfigured(format!("The login of the account failed: {msg}")),
            202 | 203 | 209 | 210 | 279 => self.unavailable(msg),
            394 | 735 => SourceError::Cancelled,
            _ => SourceError::QueryError {
                source: self.source.clone(),
                clickhouse_code: code,
                query_id: self.query_id,
                message: msg,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn limits(rows_ceiling: bool, time_ceiling: bool) -> EffectiveLimits {
        EffectiveLimits {
            max_execution_time: Duration::from_secs(30),
            max_result_rows: 1000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            rows_from_ceiling: rows_ceiling,
            time_from_ceiling: time_ceiling,
        }
    }

    fn map(code: i64, rows_ceiling: bool, time_ceiling: bool) -> SourceError {
        let id = SourceId::parse("prod").unwrap();
        let l = limits(rows_ceiling, time_ceiling);
        let q = Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap();
        ErrorCtx { source: &id, query_id: q, limits: &l }
            .map_clickhouse(code, "Code: x. DB::Exception: first\nsecond line")
    }

    #[test]
    fn timeouts() {
        for code in [159, 160] {
            let e = map(code, false, false);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("query_timeout", 504, false));
            assert_eq!(e.source_id().unwrap().as_str(), "prod");
            assert!(e.query_id().is_some());
        }
        let e = map(159, false, true);
        assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
        assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_duration"), Some(30)));
    }

    #[test]
    fn result_limits() {
        let e = map(396, false, false);
        assert_eq!((e.code(), e.http_status()), ("query_limit", 422));
        assert_eq!(e.limit_name(), Some("max_result"));
        assert_eq!(e.limit_value(), None);
        let e = map(396, true, false);
        assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
        assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_rows"), Some(1000)));
        assert!(e.message().contains("1000 rows") && e.message().contains("33554432 bytes"));
        assert_eq!(map(158, false, false).limit_name(), Some("max_rows_to_read"));
        assert_eq!(map(307, false, false).limit_name(), Some("max_bytes_to_read"));
        // A ceiling changes the code 396 only.
        assert_eq!(map(158, true, false).code(), "query_limit");
    }

    #[test]
    fn the_checks_of_8_4() {
        let id = SourceId::parse("prod").unwrap();
        let q = Uuid::now_v7();
        let l = limits(false, false);
        let c = ErrorCtx { source: &id, query_id: q, limits: &l };
        let e = c.rows();
        assert_eq!((e.limit_name(), e.limit_value(), e.http_status()), (Some("max_result_rows"), Some(1000), 422));
        let e = c.bytes();
        assert_eq!((e.limit_name(), e.limit_value()), (Some("max_result_bytes"), Some(33_554_432)));
        let l = limits(true, false);
        let e = ErrorCtx { source: &id, query_id: q, limits: &l }.rows();
        assert_eq!((e.code(), e.ceiling()), ("ceiling_exceeded", Some("max_rows")));
        // The byte check never becomes a ceiling.
        assert_eq!(ErrorCtx { source: &id, query_id: q, limits: &l }.bytes().code(), "query_limit");
    }

    #[test]
    fn the_codes_of_the_query_parameters_and_the_statuses_of_a_proxy() {
        // 456 UNKNOWN_QUERY_PARAMETER and 457 BAD_QUERY_PARAMETER are a `query_error` (8.5, 7.6a).
        for code in [456, 457] {
            let e = map(code, false, false);
            assert_eq!(
                (e.code(), e.http_status(), e.retryable(), e.clickhouse_code()),
                ("query_error", 422, false, Some(code))
            );
        }
        // 414 and 431 of a proxy: a `query_error` with `clickhouse_code` null, not `source_unavailable`.
        let id = SourceId::parse("prod").unwrap();
        let l = limits(false, false);
        let q = Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap();
        for status in [414, 431] {
            let e = ErrorCtx { source: &id, query_id: q, limits: &l }.proxy_refused(status);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("query_error", 422, false));
            assert_eq!(e.clickhouse_code(), None);
            assert_eq!(
                e.members(),
                vec![
                    ("source", Member::Text("prod".into())),
                    ("clickhouse_code", Member::Null),
                    ("query_id", Member::Text(q.to_string())),
                ]
            );
            assert!(e.message().contains(&status.to_string()) && e.message().contains("text form"));
        }
    }

    #[test]
    fn memory_access_and_login() {
        let e = map(241, false, false);
        assert_eq!((e.code(), e.http_status()), ("query_memory_limit", 422));
        assert!(e.members().is_empty());
        let e = map(497, false, false);
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_access_denied", 502, false));
        let names: Vec<_> = e.members().into_iter().map(|m| m.0).collect();
        assert_eq!(names, ["source", "query_id"]);
        for code in [192, 193, 194, 516] {
            let e = map(code, false, false);
            assert_eq!((e.code(), e.http_status()), ("source_misconfigured", 502), "{code}");
            assert_eq!(e.members().len(), 1);
        }
    }

    #[test]
    fn unavailable_and_cancelled() {
        for code in [202, 203, 209, 210, 279] {
            let e = map(code, false, false);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true), "{code}");
        }
        for code in [394, 735] {
            assert_eq!(map(code, false, false), SourceError::Cancelled);
        }
        assert_eq!(SourceError::Cancelled.http_status(), 499);
    }

    #[test]
    fn any_other_code_is_a_query_error_with_the_first_line_only() {
        for code in [62, 60, 164, 452, 1, 9999] {
            let e = map(code, true, true);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("query_error", 422, false), "{code}");
            assert_eq!(e.clickhouse_code(), Some(code));
            assert_eq!(e.message(), "Code: x. DB::Exception: first");
            let names: Vec<_> = e.members().into_iter().map(|m| m.0).collect();
            assert_eq!(names, ["source", "clickhouse_code", "query_id"]);
        }
    }

    #[test]
    fn members_of_the_limit_codes() {
        let e = map(396, false, false);
        assert_eq!(e.members(), vec![("limit", Member::Text("max_result".into())), ("value", Member::Null)]);
        let e = map(396, true, false);
        assert_eq!(e.members(), vec![("ceiling", Member::Text("max_rows".into())), ("limit", Member::Uint(1000))]);
    }

    #[test]
    fn busy_has_a_retry_after() {
        let e = SourceError::SourceBusy { source: SourceId::parse("p").unwrap(), message: "m".into() };
        assert_eq!(
            (e.code(), e.http_status(), e.retryable(), e.retry_after_secs()),
            ("source_busy", 503, true, Some(1))
        );
        assert_eq!(map(62, false, false).retry_after_secs(), None);
    }

    #[test]
    fn the_message_is_one_line_of_1000_bytes() {
        let long = format!("{}\nsecond", "é".repeat(800));
        let m = first_line(&long);
        assert!(m.len() <= MESSAGE_MAX_BYTES);
        assert!(!m.contains('\n'));
        assert_eq!(first_line("a\r\nb"), "a");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn whole_seconds_round_up() {
        let mut l = limits(false, false);
        l.max_execution_time = Duration::from_millis(1500);
        assert_eq!(exec_seconds(&l), 2);
        l.max_execution_time = Duration::from_secs(60);
        assert_eq!(exec_seconds(&l), 60);
    }
}
