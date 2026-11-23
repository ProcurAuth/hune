//! The error map of a PostgreSQL source (8a.8, ID-PG12).
//!
//! This module maps an error of the server to a [`SourceError`] by the SQLSTATE only, never by the
//! message. The table of 8a.8 is the contract:
//!
//! | SQLSTATE or event | Result |
//! |---|---|
//! | 57014, when Hune sent no cancel for a client that left | `504 query_timeout`, or `403 ceiling_exceeded` |
//! | 57014, when Hune sent the cancel for a client that left | `Cancelled` (no answer) |
//! | 55P03 | `504 query_timeout` |
//! | 53200, 53400 | `422 query_memory_limit` |
//! | 42501 | `502 source_access_denied` |
//! | class 28, 3D000, after the session is open | `422 query_error` with the member `sqlstate` |
//! | class 28, 3D000, in the connection setup | `502 source_misconfigured` (see `session::open_error`) |
//! | class 08, 53000, 53100, 53300, 57P01, 57P02, 57P03, 25P03, 40001, 40P01 | `503 source_unavailable` |
//! | 25006 and each other SQLSTATE | `422 query_error` with the member `sqlstate` |
//!
//! The member `message` of a `query_error`, of `source_access_denied` and of `query_memory_limit`
//! is the first line of the primary message of the server, cut to 1,000 bytes. This text can hold
//! a value of the data, as a ClickHouse message can. The messages of `source_misconfigured` and
//! `source_unavailable` are fixed English texts with the SQLSTATE. They never hold text of the
//! server, because the state log lines `source.down` and `source.misconfigured` carry them (12.15).
//! An error without a SQLSTATE (a transport fault) gives `503 source_unavailable`.
//!
//! A statement of a viewer can raise a login class code on an open session, for example
//! `SELECT pg_database_size('no_such_db')` raises 3D000. The code then says something about the text of the
//! query, not about the account, so [`map_sqlstate`] gives `query_error` and the state of the source does not
//! change (finding M1). Only the setup of a connection and the probe give `misconfigured` for these codes.
//!
//! The functions of this module write no log line. A caller that logs an error writes the
//! SQLSTATE (see [`sqlstate_of`]) and the SHA-256 of the SQL, and never the message.

use hune_core::ids::SourceId;

use crate::error::{ErrorCtx, SourceError, first_line};
use crate::types::Uuid;

/// The class of a SQLSTATE for the map of 8a.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    /// 57014: the query stopped (deadline, `statement_timeout` or a cancel request).
    Canceled,
    /// 55P03: a lock was not available within `lock_timeout`.
    LockNotAvailable,
    /// 53200, 53400.
    MemoryLimit,
    /// 42501.
    AccessDenied,
    /// Class 28 and 3D000: the login or the database was refused. At the setup of a connection this is
    /// `misconfigured`. On an open session it is a `query_error` (the statement raised it).
    Login,
    /// The faults that a later try can cure.
    Unavailable,
    /// Every other SQLSTATE, 25006 included.
    Other,
}

/// Gives the class of a SQLSTATE. The match is on the code only.
pub(crate) fn classify(sqlstate: &str) -> Class {
    match sqlstate {
        "57014" => Class::Canceled,
        "55P03" => Class::LockNotAvailable,
        "53200" | "53400" => Class::MemoryLimit,
        "42501" => Class::AccessDenied,
        "3D000" => Class::Login,
        "53000" | "53100" | "53300" | "57P01" | "57P02" | "57P03" | "25P03" | "40001" | "40P01" => Class::Unavailable,
        s if s.starts_with("28") => Class::Login,
        s if s.starts_with("08") => Class::Unavailable,
        _ => Class::Other,
    }
}

/// Tells if the text is a well-formed SQLSTATE: five characters, digits and upper-case letters.
pub(crate) fn is_sqlstate(text: &str) -> bool {
    text.len() == 5 && text.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

/// Gives the SQLSTATE of an error of `tokio-postgres`, or `None` for a fault that is not a server
/// error (transport, protocol). This is the only value that a log line holds about the error.
pub(crate) fn sqlstate_of(error: &tokio_postgres::Error) -> Option<String> {
    let code = error.code()?.code();
    is_sqlstate(code).then(|| code.to_owned())
}

/// What the map needs besides the ErrorResponse.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MapInput<'a> {
    /// The SQLSTATE of the ErrorResponse, or `None` when the fault is not a server error.
    pub sqlstate: Option<&'a str>,
    /// The primary message of the server. The map keeps its first line.
    pub message: &'a str,
    /// True when Hune sent a cancel request because the client left (8a.7).
    pub cancel_sent_for_client_left: bool,
}

/// Maps a SQLSTATE and a message to the error of the source (8a.8).
///
/// `ctx` gives the source, the query id and the effective limits. The function never reads the
/// message to choose the code.
pub(crate) fn map_sqlstate(ctx: &ErrorCtx<'_>, input: MapInput<'_>) -> SourceError {
    let Some(raw) = input.sqlstate else {
        return ctx.unavailable("The PostgreSQL server did not answer.");
    };
    // A value that is not a SQLSTATE is a protocol fault: it has no class and gives no member.
    let sqlstate = is_sqlstate(raw).then_some(raw);
    let Some(state) = sqlstate else {
        return query_error(ctx.source, None, ctx.query_id, input.message);
    };
    match classify(state) {
        Class::Canceled if input.cancel_sent_for_client_left => SourceError::Cancelled,
        Class::Canceled => ctx.timeout(),
        Class::LockNotAvailable => SourceError::QueryTimeout {
            source: ctx.source.clone(),
            query_id: ctx.query_id,
            message: "A lock was not available within the lock timeout.".to_owned(),
        },
        Class::MemoryLimit => SourceError::QueryMemoryLimit { message: first_line(input.message) },
        Class::AccessDenied => SourceError::SourceAccessDenied {
            source: ctx.source.clone(),
            query_id: ctx.query_id,
            message: first_line(input.message),
        },
        // The session is open: the account was accepted. A statement raised this code (finding M1).
        Class::Login => query_error(ctx.source, Some(state), ctx.query_id, input.message),
        Class::Unavailable => ctx.unavailable(format!("The PostgreSQL server is not available (SQLSTATE {state}).")),
        Class::Other => query_error(ctx.source, Some(state), ctx.query_id, input.message),
    }
}

fn query_error(source: &SourceId, sqlstate: Option<&str>, query_id: Uuid, message: &str) -> SourceError {
    SourceError::QuerySqlError {
        source: source.clone(),
        sqlstate: sqlstate.map(str::to_owned),
        query_id,
        message: first_line(message),
    }
}

/// Maps an error of `tokio-postgres` to the error of the source (8a.8).
///
/// An error that holds an ErrorResponse goes through [`map_sqlstate`]. Another error is a
/// transport or protocol fault: it gives `503 source_unavailable` with a fixed text.
pub(crate) fn map_error(
    ctx: &ErrorCtx<'_>,
    error: &tokio_postgres::Error,
    cancel_sent_for_client_left: bool,
) -> SourceError {
    match error.as_db_error() {
        Some(db) => map_sqlstate(
            ctx,
            MapInput { sqlstate: Some(db.code().code()), message: db.message(), cancel_sent_for_client_left },
        ),
        None => map_sqlstate(ctx, MapInput { sqlstate: None, message: "", cancel_sent_for_client_left }),
    }
}

/// The error of a column of a type that 7.16a does not list: `422 query_error` with
/// `sqlstate = null`, before the query runs (8a.8, ID-PG12).
pub(crate) fn unsupported_type(ctx: &ErrorCtx<'_>, column: &str, type_name: &str) -> SourceError {
    // The column name and the type name come from the server catalog and the query text. They are
    // cut so that one line stays short.
    let column = crate::types::cut_utf8(column, 100);
    let type_name = crate::types::cut_utf8(type_name, 100);
    query_error(
        ctx.source,
        None,
        ctx.query_id,
        &format!("The column \"{column}\" has the type \"{type_name}\", which the source does not support."),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::types::EffectiveLimits;

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

    fn id() -> SourceId {
        SourceId::parse("metrics").unwrap()
    }

    fn query_id() -> Uuid {
        Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap()
    }

    fn map_with(sqlstate: &str, message: &str, left: bool, time_ceiling: bool) -> SourceError {
        let (id, l) = (id(), limits(false, time_ceiling));
        let ctx = ErrorCtx { source: &id, query_id: query_id(), limits: &l };
        map_sqlstate(&ctx, MapInput { sqlstate: Some(sqlstate), message, cancel_sent_for_client_left: left })
    }

    fn map(sqlstate: &str) -> SourceError {
        map_with(sqlstate, "the message", false, false)
    }

    fn shape(e: &SourceError) -> (&'static str, u16, bool) {
        (e.code(), e.http_status(), e.retryable())
    }

    #[test]
    fn query_canceled_is_a_timeout() {
        let e = map("57014");
        assert_eq!(shape(&e), ("query_timeout", 504, false));
        assert_eq!(e.source_id().unwrap().as_str(), "metrics");
        assert_eq!(e.query_id(), Some(query_id()));
        assert!(e.message().contains("30 seconds"));
    }

    #[test]
    fn query_canceled_with_the_time_ceiling_is_a_ceiling_error() {
        let e = map_with("57014", "m", false, true);
        assert_eq!((e.code(), e.http_status()), ("ceiling_exceeded", 403));
        assert_eq!((e.ceiling(), e.ceiling_limit()), (Some("max_duration"), Some(30)));
    }

    #[test]
    fn query_canceled_after_a_cancel_for_a_client_that_left_gives_no_answer() {
        assert_eq!(map_with("57014", "m", true, false), SourceError::Cancelled);
        // The flag changes the code 57014 only.
        assert_eq!(map_with("42601", "m", true, false).code(), "query_error");
    }

    #[test]
    fn lock_not_available_is_a_timeout_and_never_a_ceiling() {
        let e = map_with("55P03", "m", false, true);
        assert_eq!(shape(&e), ("query_timeout", 504, false));
        assert_eq!(e.members().len(), 2);
    }

    #[test]
    fn memory_codes() {
        for s in ["53200", "53400"] {
            let e = map(s);
            assert_eq!(shape(&e), ("query_memory_limit", 422, false), "{s}");
            assert_eq!(e.message(), "the message");
            assert!(e.members().is_empty());
        }
    }

    #[test]
    fn insufficient_privilege_is_access_denied() {
        let e = map("42501");
        assert_eq!(shape(&e), ("source_access_denied", 502, false));
        let names: Vec<_> = e.members().iter().map(|m| m.0).collect();
        assert_eq!(names, ["source", "query_id"]);
    }

    #[test]
    fn login_codes_on_an_open_session_are_query_errors() {
        // Finding M1: `SELECT pg_database_size('no_such_db')` raises 3D000 on an open session. It must not
        // make the whole source misconfigured.
        for s in ["28000", "28P01", "28001", "3D000"] {
            let e = map(s);
            assert_eq!(shape(&e), ("query_error", 422, false), "{s}");
            assert_eq!(e.sqlstate(), Some(Some(s)));
            let names: Vec<_> = e.members().iter().map(|m| m.0).collect();
            assert_eq!(names, ["source", "sqlstate", "query_id"], "{s}");
        }
    }

    #[test]
    fn availability_codes_are_unavailable_and_retryable() {
        for s in [
            "08000", "08001", "08003", "08004", "08006", "08P01", "53000", "53100", "53300", "57P01", "57P02", "57P03",
            "25P03", "40001", "40P01",
        ] {
            let e = map(s);
            assert_eq!(shape(&e), ("source_unavailable", 503, true), "{s}");
            assert!(e.message().contains(s));
        }
    }

    #[test]
    fn read_only_violation_and_other_codes_are_query_errors_with_the_sqlstate() {
        for s in ["25006", "42601", "42P01", "22012", "42703", "0A000", "XX000", "P0001"] {
            let e = map(s);
            assert_eq!(shape(&e), ("query_error", 422, false), "{s}");
            assert_eq!(e.sqlstate(), Some(Some(s)));
            assert_eq!(e.clickhouse_code(), None);
            let names: Vec<_> = e.members().iter().map(|m| m.0).collect();
            assert_eq!(names, ["source", "sqlstate", "query_id"], "{s}");
        }
    }

    #[test]
    fn the_class_decides_not_the_message() {
        // A message that looks like another error does not change the code.
        let e = map_with("42601", "canceling statement due to statement timeout", false, false);
        assert_eq!(e.code(), "query_error");
        let e = map_with("57014", "syntax error at or near", false, false);
        assert_eq!(e.code(), "query_timeout");
    }

    #[test]
    fn the_message_is_the_first_line_cut_to_1000_bytes() {
        let e = map_with("22012", "division by zero\nDETAIL: a=1 secret-row", false, false);
        assert_eq!(e.message(), "division by zero");
        let long = format!("{}\nsecond", "é".repeat(800));
        let e = map_with("22012", &long, false, false);
        assert!(e.message().len() <= 1000);
        assert!(e.message().chars().all(|c| c == 'é'));
        assert!(!e.message().contains("second"));
    }

    #[test]
    fn state_messages_never_hold_text_of_the_server() {
        let secret = "role \"alice\" password 'hunter2' value 42";
        for s in ["08006", "57P01", "55P03"] {
            let e = map_with(s, secret, false, false);
            assert!(!e.message().contains("hunter2") && !e.message().contains("alice"), "{s}");
        }
    }

    #[test]
    fn a_fault_without_a_sqlstate_is_unavailable() {
        let (id, l) = (id(), limits(false, false));
        let ctx = ErrorCtx { source: &id, query_id: query_id(), limits: &l };
        let e = map_sqlstate(&ctx, MapInput { sqlstate: None, message: "x", cancel_sent_for_client_left: false });
        assert_eq!(shape(&e), ("source_unavailable", 503, true));
    }

    #[test]
    fn a_malformed_sqlstate_gives_a_null_member() {
        for s in ["", "4260", "42601X", "4a601", "42 01"] {
            let e = map(s);
            assert_eq!(e.code(), "query_error", "{s:?}");
            assert_eq!(e.sqlstate(), Some(None), "{s:?}");
        }
    }

    #[test]
    fn classify_covers_the_table() {
        assert_eq!(classify("57014"), Class::Canceled);
        assert_eq!(classify("57P01"), Class::Unavailable);
        assert_eq!(classify("57000"), Class::Other);
        assert_eq!(classify("53300"), Class::Unavailable);
        assert_eq!(classify("53200"), Class::MemoryLimit);
        assert_eq!(classify("53100"), Class::Unavailable);
        assert_eq!(classify("27000"), Class::Other);
        assert_eq!(
            classify("28P01"),
            Class::Login,
            "the class is kept for the setup of a connection (session::open_error)"
        );
        assert_eq!(classify("3D001"), Class::Other);
    }

    #[test]
    fn unsupported_type_has_a_null_sqlstate() {
        let (id, l) = (id(), limits(false, false));
        let ctx = ErrorCtx { source: &id, query_id: query_id(), limits: &l };
        let e = unsupported_type(&ctx, "shape", "geometry");
        assert_eq!(shape(&e), ("query_error", 422, false));
        assert_eq!(e.sqlstate(), Some(None));
        assert!(e.message().contains("shape") && e.message().contains("geometry"));
    }

    #[test]
    fn the_query_error_members_have_the_json_null() {
        let e = map("");
        let members = e.members();
        assert_eq!(members[1], ("sqlstate", crate::error::Member::Null));
    }

    #[test]
    fn is_sqlstate_accepts_only_five_upper_case_characters() {
        assert!(is_sqlstate("42P01") && is_sqlstate("00000"));
        assert!(!is_sqlstate("42p01") && !is_sqlstate("4201") && !is_sqlstate("é2601"));
    }
}
