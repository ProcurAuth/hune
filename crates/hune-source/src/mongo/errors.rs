//! The error map of a MongoDB source (8b.8, ID-MG12).
//!
//! Hune maps an error by the code of the server, never by the message. A [`MongoError`] has a class
//! and a code. [`map_error`] gives the [`SourceError`] of the table of 8b.8. The messages of
//! `source_misconfigured` and `source_unavailable` are fixed English texts with the code when there
//! is one. They never hold text of the server, because the log lines `source.down` and
//! `source.misconfigured` carry them. The messages of `query_error`, `source_access_denied` and
//! `query_memory_limit` hold the first line of the server message, cut to 1,000 bytes.
//!
//! [`classify`] gives the same grouping to the probe: the group says if a fault leaves the state
//! `down` or `misconfigured` (8b.12).

use hune_core::mongo::frame::Fault;

use crate::error::{ErrorCtx, SourceError, first_line};
use crate::mongo::iface::{CERTIFICATE_CODE_NAME, MongoError, MongoErrorClass};

/// The codes that give `503 source_unavailable` (8b.8).
pub const UNAVAILABLE_CODES: [i32; 13] = [6, 7, 89, 91, 133, 189, 202, 9001, 10107, 11600, 11602, 13435, 13436];

/// The codes of the login that give `502 source_misconfigured` (8b.8).
pub const LOGIN_CODES: [i32; 4] = [18, 11, 334, 31];

/// The codes of a time limit that the server enforces: `MaxTimeMSExpired` and `ExceededTimeLimit`.
pub const TIMEOUT_CODES: [i32; 2] = [50, 262];

/// The codes of a memory limit of the server: `QueryExceededMemoryLimitNoDiskUseAllowed` and
/// `ExceededMemoryLimit`.
pub const MEMORY_CODES: [i32; 2] = [292, 146];

/// The codes that mean that the cursor stopped because of a kill: `Interrupted` and `CursorKilled`.
pub const KILL_CODES: [i32; 2] = [11601, 237];

/// The code `Unauthorized`.
pub const UNAUTHORIZED: i32 = 13;

/// The group of an error: what it means for the state of the source and for the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The deadline of the client, or a time limit of the server.
    Timeout,
    /// A memory limit of the server.
    Memory,
    /// `Unauthorized`.
    AccessDenied,
    /// The login, the certificate, or the role fails. The source is misconfigured.
    Misconfigured,
    /// The link or the server does not answer. The source is down.
    Unavailable,
    /// The stop of a cursor by a kill.
    Killed,
    /// A reply that does not decode.
    BadResponse,
    /// The wire guard refused the request.
    Guard,
    /// Another server error: `query_error`.
    Other,
}

/// Puts an error in its group, by its class and its code.
pub fn classify(error: &MongoError) -> Group {
    match error.class {
        MongoErrorClass::Network => Group::Unavailable,
        MongoErrorClass::Tls => {
            if error.code_name.as_deref() == Some(CERTIFICATE_CODE_NAME) {
                Group::Misconfigured
            } else {
                Group::Unavailable
            }
        }
        MongoErrorClass::Auth => Group::Misconfigured,
        MongoErrorClass::Timeout => Group::Timeout,
        MongoErrorClass::Decode => Group::BadResponse,
        MongoErrorClass::Guard => Group::Guard,
        MongoErrorClass::Server => match error.code {
            Some(c) if TIMEOUT_CODES.contains(&c) => Group::Timeout,
            Some(c) if MEMORY_CODES.contains(&c) => Group::Memory,
            Some(UNAUTHORIZED) => Group::AccessDenied,
            Some(c) if LOGIN_CODES.contains(&c) => Group::Misconfigured,
            Some(c) if UNAVAILABLE_CODES.contains(&c) => Group::Unavailable,
            Some(c) if KILL_CODES.contains(&c) => Group::Killed,
            _ => Group::Other,
        },
    }
}

/// The fixed text of an error of the group `Unavailable` or `Misconfigured`, with the code.
fn fixed(error: &MongoError, base: &str) -> String {
    match error.code {
        Some(code) => format!("{base} (code {code})."),
        None => format!("{base}."),
    }
}

/// The text of `source_unavailable` for an error of the link.
pub fn unavailable_text(error: &MongoError) -> String {
    let base = match error.class {
        MongoErrorClass::Tls => "The TLS link to the MongoDB server failed",
        MongoErrorClass::Server => "The MongoDB server cannot serve the query now",
        _ => "The MongoDB server did not answer",
    };
    fixed(error, base)
}

/// The text of `source_misconfigured` for a login or a certificate fault.
pub fn misconfigured_text(error: &MongoError) -> String {
    let base = match error.class {
        MongoErrorClass::Tls => "The certificate of the MongoDB server failed the check",
        _ => "The login of the account failed",
    };
    fixed(error, base)
}

/// Maps an error of the client interface to the error of 8b.8.
///
/// `client_left` is true when Hune sent the kill for a client that left. The codes 11601 and 237
/// then give `Cancelled`, and the server sends no answer (8b.7, 8b.8). The same codes from another
/// cause give `query_error`.
pub fn map_error(ectx: &ErrorCtx<'_>, error: &MongoError, client_left: bool) -> SourceError {
    match classify(error) {
        Group::Timeout => ectx.timeout(),
        Group::Memory => SourceError::QueryMemoryLimit { message: first_line(&error.message) },
        Group::AccessDenied => SourceError::SourceAccessDenied {
            source: ectx.source.clone(),
            query_id: ectx.query_id,
            message: first_line(&error.message),
        },
        Group::Misconfigured => ectx.misconfigured(misconfigured_text(error)),
        Group::Unavailable => ectx.unavailable(unavailable_text(error)),
        Group::Killed if client_left => SourceError::Cancelled,
        Group::Killed | Group::Other => query_error(ectx, error.code.map(i64::from), &error.message),
        Group::BadResponse => ectx.bad_response("A reply of the MongoDB server did not decode."),
        Group::Guard => internal("The request guard refused the command of the query."),
    }
}

/// A `query_error` with the member `mongo_code`.
pub fn query_error(ectx: &ErrorCtx<'_>, mongo_code: Option<i64>, message: &str) -> SourceError {
    SourceError::QueryMongoError {
        source: ectx.source.clone(),
        mongo_code,
        query_id: ectx.query_id,
        message: first_line(message),
    }
}

/// The fault of Hune: `500 internal_error`. The text is fixed.
pub fn internal(message: &str) -> SourceError {
    SourceError::Internal { message: message.to_owned() }
}

/// Maps a fault of the frame builder (7.16c, 8b.6).
pub fn map_frame_fault(ectx: &ErrorCtx<'_>, fault: Fault) -> SourceError {
    match fault {
        Fault::Rows => ectx.rows(),
        Fault::Bytes => ectx.bytes(),
        Fault::BadResponse(message) => ectx.bad_response(message),
        // A type that 7.16c does not allow, a mix, or too many columns: `mongo_code` is `null`.
        Fault::Query(message) => query_error(ectx, None, &message),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use hune_core::ids::SourceId;

    use super::*;
    use crate::error::{Ceiling, LimitName};
    use crate::types::{EffectiveLimits, Uuid};

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

    fn map_with(error: &MongoError, rows_ceiling: bool, time_ceiling: bool, left: bool) -> SourceError {
        let id = SourceId::parse("events").unwrap();
        let l = limits(rows_ceiling, time_ceiling);
        let q = Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap();
        map_error(&ErrorCtx { source: &id, query_id: q, limits: &l }, error, left)
    }

    fn server(code: i32) -> SourceError {
        map_with(&MongoError::server(code, "Name", "first line\nsecond line"), false, false, false)
    }

    #[test]
    fn the_timeout_codes_and_the_client_deadline_give_query_timeout() {
        for code in [50, 262] {
            let e = server(code);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("query_timeout", 504, false), "{code}");
            assert_eq!(e.source_id().unwrap().as_str(), "events");
            assert!(e.query_id().is_some());
        }
        let e = map_with(&MongoError::new(MongoErrorClass::Timeout, "x"), false, false, false);
        assert_eq!(e.code(), "query_timeout");
        // With a ceiling, the code is `ceiling_exceeded` with the effective seconds.
        let e = map_with(&MongoError::server(50, "MaxTimeMSExpired", "x"), false, true, false);
        assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
        assert_eq!((e.ceiling(), e.ceiling_limit()), (Some(Ceiling::MaxDuration.as_str()), Some(30)));
    }

    #[test]
    fn the_memory_codes_give_query_memory_limit_with_the_first_line() {
        for code in [292, 146] {
            let e = server(code);
            assert_eq!((e.code(), e.http_status()), ("query_memory_limit", 422), "{code}");
            assert_eq!(e.message(), "first line");
            assert!(e.members().is_empty());
        }
    }

    #[test]
    fn unauthorized_gives_source_access_denied() {
        let e = server(13);
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_access_denied", 502, false));
        let names: Vec<_> = e.members().into_iter().map(|m| m.0).collect();
        assert_eq!(names, ["source", "query_id"]);
        assert_eq!(e.message(), "first line");
    }

    #[test]
    fn the_login_codes_give_source_misconfigured_with_a_fixed_text() {
        for code in [18, 11, 334, 31] {
            let e = server(code);
            assert_eq!((e.code(), e.http_status()), ("source_misconfigured", 502), "{code}");
            assert_eq!(e.members().len(), 1);
            // The text of the server never goes in.
            assert!(!e.message().contains("first line"));
            assert!(e.message().contains(&code.to_string()));
        }
        let e = map_with(&MongoError::new(MongoErrorClass::Auth, "bad password for hunter2"), false, false, false);
        assert_eq!(e.code(), "source_misconfigured");
        assert!(!e.message().contains("hunter2"));
    }

    #[test]
    fn a_failed_certificate_is_misconfigured_and_another_tls_fault_is_unavailable() {
        let mut cert = MongoError::new(MongoErrorClass::Tls, "bad certificate for host");
        cert.code_name = Some(CERTIFICATE_CODE_NAME.to_owned());
        let e = map_with(&cert, false, false, false);
        assert_eq!(e.code(), "source_misconfigured");
        assert!(!e.message().contains("host"));
        let handshake = MongoError::new(MongoErrorClass::Tls, "handshake eof");
        let e = map_with(&handshake, false, false, false);
        assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true));
        assert!(!e.message().contains("eof"));
    }

    #[test]
    fn the_network_codes_and_the_link_faults_give_source_unavailable() {
        for code in [6, 7, 89, 91, 133, 189, 202, 9001, 10107, 11600, 11602, 13435, 13436] {
            let e = server(code);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true), "{code}");
            assert!(!e.message().contains("first line"));
            assert!(e.message().contains(&code.to_string()));
        }
        let e =
            map_with(&MongoError::new(MongoErrorClass::Network, "connection reset by 10.0.0.1"), false, false, false);
        assert_eq!(e.code(), "source_unavailable");
        assert!(!e.message().contains("10.0.0.1"));
        assert!(!e.message().contains("code"));
    }

    #[test]
    fn a_kill_for_a_client_that_left_gives_no_answer_and_another_kill_is_a_query_error() {
        for code in [11601, 237] {
            let e = MongoError::server(code, "Interrupted", "operation was interrupted");
            assert_eq!(map_with(&e, false, false, true), SourceError::Cancelled);
            let other = map_with(&e, false, false, false);
            assert_eq!((other.code(), other.mongo_code()), ("query_error", Some(Some(i64::from(code)))));
        }
    }

    #[test]
    fn any_other_code_is_a_query_error_with_mongo_code_and_the_first_line_only() {
        for code in [2, 9, 40324, 168, 26, 43, 1, 99999] {
            let e = server(code);
            assert_eq!((e.code(), e.http_status(), e.retryable()), ("query_error", 422, false), "{code}");
            assert_eq!(e.mongo_code(), Some(Some(i64::from(code))));
            assert_eq!(e.clickhouse_code(), None);
            assert_eq!(e.sqlstate(), None);
            assert_eq!(e.message(), "first line");
            let names: Vec<_> = e.members().into_iter().map(|m| m.0).collect();
            assert_eq!(names, ["source", "mongo_code", "query_id"]);
        }
        // A rare case: a server error with no code.
        let e = map_with(&MongoError::new(MongoErrorClass::Server, "odd"), false, false, false);
        assert_eq!(e.mongo_code(), Some(None));
    }

    #[test]
    fn a_decode_fault_is_a_bad_response_and_a_guard_refusal_is_internal() {
        let e = map_with(&MongoError::new(MongoErrorClass::Decode, "bad bson with secret"), false, false, false);
        assert_eq!((e.code(), e.http_status()), ("source_bad_response", 502));
        assert!(!e.message().contains("secret"));
        let e =
            map_with(&MongoError::new(MongoErrorClass::Guard, "The command holds the key $out."), false, false, false);
        assert_eq!((e.code(), e.http_status()), ("internal_error", 500));
        assert!(!e.message().contains("$out"));
        assert!(e.members().is_empty());
    }

    #[test]
    fn the_message_is_cut_to_one_line_of_1000_bytes() {
        let long = format!("{}\nsecond", "é".repeat(800));
        let e = map_with(&MongoError::server(2, "BadValue", &long), false, false, false);
        assert!(e.message().len() <= 1000);
        assert!(!e.message().contains('\n'));
    }

    #[test]
    fn the_frame_faults_map_as_8b8_says() {
        let id = SourceId::parse("events").unwrap();
        let l = limits(false, false);
        let q = Uuid::now_v7();
        let ectx = ErrorCtx { source: &id, query_id: q, limits: &l };
        let e = map_frame_fault(&ectx, Fault::Rows);
        assert_eq!((e.code(), e.limit_name(), e.limit_value()), ("query_limit", Some("max_result_rows"), Some(1000)));
        let e = map_frame_fault(&ectx, Fault::Bytes);
        assert_eq!((e.limit_name(), e.limit_value()), (Some(LimitName::MaxResultBytes.as_str()), Some(33_554_432)));
        let e = map_frame_fault(&ectx, Fault::BadResponse("m".to_owned()));
        assert_eq!(e.code(), "source_bad_response");
        let e = map_frame_fault(&ectx, Fault::Query("The column \"a\" holds int, string.".to_owned()));
        assert_eq!((e.code(), e.http_status(), e.mongo_code()), ("query_error", 422, Some(None)));
        let l = limits(true, false);
        let e = map_frame_fault(&ErrorCtx { source: &id, query_id: q, limits: &l }, Fault::Rows);
        assert_eq!((e.code(), e.ceiling()), ("ceiling_exceeded", Some("max_rows")));
        assert_eq!(
            map_frame_fault(&ErrorCtx { source: &id, query_id: q, limits: &l }, Fault::Bytes).code(),
            "query_limit"
        );
    }

    #[test]
    fn the_groups_are_stable() {
        assert_eq!(classify(&MongoError::server(50, "MaxTimeMSExpired", "")), Group::Timeout);
        assert_eq!(classify(&MongoError::server(13, "Unauthorized", "")), Group::AccessDenied);
        assert_eq!(classify(&MongoError::new(MongoErrorClass::Network, "")), Group::Unavailable);
        assert_eq!(classify(&MongoError::server(2, "BadValue", "")), Group::Other);
    }
}
