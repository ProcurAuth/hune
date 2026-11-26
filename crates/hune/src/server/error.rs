// SPDX-License-Identifier: MIT OR Apache-2.0
//! The error model of the API: the closed code table of 6.8 and the error body of 6.7.

use http::StatusCode;
use hune_core::error::ValidationError;
use hune_source::SourceKind;
use serde_json::{Map, Value};

/// One code of the closed table of 6.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    BadRequest,
    InvalidOptionsRequest,
    Unauthenticated,
    Forbidden,
    CeilingExceeded,
    HistoryDisabled,
    ExportDisabled,
    ReadOnlyStore,
    CrossSiteRequest,
    NotFound,
    HistoryUnavailable,
    MethodNotAllowed,
    RequestTimeout,
    Conflict,
    AlreadyExists,
    FolderNotEmpty,
    DashboardChanged,
    PayloadTooLarge,
    UriTooLong,
    UnsupportedMediaType,
    InvalidDocument,
    StoredDocumentInvalid,
    NotRestorable,
    IdempotencyKeyReused,
    InvalidTimeRange,
    VariableValueRefused,
    SqlInvalid,
    SqlTooLarge,
    QueryLimit,
    AnnotationInvalid,
    QueryMemoryLimit,
    QueryError,
    PreconditionRequired,
    HeadersTooLarge,
    InternalError,
    SourceAccessDenied,
    SourceMisconfigured,
    SourceBadResponse,
    PdpUnavailable,
    ServerBusy,
    ServerStopping,
    StorageUnavailable,
    SourceUnavailable,
    SourceBusy,
    QueryTimeout,
    StorageFull,
}

impl Code {
    /// Every code, in the order of the table of 6.8.
    pub const ALL: [Code; 46] = [
        Code::BadRequest,
        Code::InvalidOptionsRequest,
        Code::Unauthenticated,
        Code::Forbidden,
        Code::CeilingExceeded,
        Code::HistoryDisabled,
        Code::ExportDisabled,
        Code::ReadOnlyStore,
        Code::CrossSiteRequest,
        Code::NotFound,
        Code::HistoryUnavailable,
        Code::MethodNotAllowed,
        Code::RequestTimeout,
        Code::Conflict,
        Code::AlreadyExists,
        Code::FolderNotEmpty,
        Code::DashboardChanged,
        Code::PayloadTooLarge,
        Code::UriTooLong,
        Code::UnsupportedMediaType,
        Code::InvalidDocument,
        Code::StoredDocumentInvalid,
        Code::NotRestorable,
        Code::IdempotencyKeyReused,
        Code::InvalidTimeRange,
        Code::VariableValueRefused,
        Code::SqlInvalid,
        Code::SqlTooLarge,
        Code::QueryLimit,
        Code::AnnotationInvalid,
        Code::QueryMemoryLimit,
        Code::QueryError,
        Code::PreconditionRequired,
        Code::HeadersTooLarge,
        Code::InternalError,
        Code::SourceAccessDenied,
        Code::SourceMisconfigured,
        Code::SourceBadResponse,
        Code::PdpUnavailable,
        Code::ServerBusy,
        Code::ServerStopping,
        Code::StorageUnavailable,
        Code::SourceUnavailable,
        Code::SourceBusy,
        Code::QueryTimeout,
        Code::StorageFull,
    ];

    /// The text of the code.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BadRequest => "bad_request",
            Code::InvalidOptionsRequest => "invalid_options_request",
            Code::Unauthenticated => "unauthenticated",
            Code::Forbidden => "forbidden",
            Code::CeilingExceeded => "ceiling_exceeded",
            Code::HistoryDisabled => "history_disabled",
            Code::ExportDisabled => "export_disabled",
            Code::ReadOnlyStore => "read_only_store",
            Code::CrossSiteRequest => "cross_site_request",
            Code::NotFound => "not_found",
            Code::HistoryUnavailable => "history_unavailable",
            Code::MethodNotAllowed => "method_not_allowed",
            Code::RequestTimeout => "request_timeout",
            Code::Conflict => "conflict",
            Code::AlreadyExists => "already_exists",
            Code::FolderNotEmpty => "folder_not_empty",
            Code::DashboardChanged => "dashboard_changed",
            Code::PayloadTooLarge => "payload_too_large",
            Code::UriTooLong => "uri_too_long",
            Code::UnsupportedMediaType => "unsupported_media_type",
            Code::InvalidDocument => "invalid_document",
            Code::StoredDocumentInvalid => "stored_document_invalid",
            Code::NotRestorable => "not_restorable",
            Code::IdempotencyKeyReused => "idempotency_key_reused",
            Code::InvalidTimeRange => "invalid_time_range",
            Code::VariableValueRefused => "variable_value_refused",
            Code::SqlInvalid => "sql_invalid",
            Code::SqlTooLarge => "sql_too_large",
            Code::QueryLimit => "query_limit",
            Code::AnnotationInvalid => "annotation_invalid",
            Code::QueryMemoryLimit => "query_memory_limit",
            Code::QueryError => "query_error",
            Code::PreconditionRequired => "precondition_required",
            Code::HeadersTooLarge => "headers_too_large",
            Code::InternalError => "internal_error",
            Code::SourceAccessDenied => "source_access_denied",
            Code::SourceMisconfigured => "source_misconfigured",
            Code::SourceBadResponse => "source_bad_response",
            Code::PdpUnavailable => "pdp_unavailable",
            Code::ServerBusy => "server_busy",
            Code::ServerStopping => "server_stopping",
            Code::StorageUnavailable => "storage_unavailable",
            Code::SourceUnavailable => "source_unavailable",
            Code::SourceBusy => "source_busy",
            Code::QueryTimeout => "query_timeout",
            Code::StorageFull => "storage_full",
        }
    }

    /// The HTTP status of the code.
    pub fn status(self) -> StatusCode {
        let n = match self {
            Code::BadRequest | Code::InvalidOptionsRequest => 400,
            Code::Unauthenticated => 401,
            Code::Forbidden
            | Code::CeilingExceeded
            | Code::HistoryDisabled
            | Code::ExportDisabled
            | Code::ReadOnlyStore
            | Code::CrossSiteRequest => 403,
            Code::NotFound | Code::HistoryUnavailable => 404,
            Code::MethodNotAllowed => 405,
            Code::RequestTimeout => 408,
            Code::Conflict | Code::AlreadyExists | Code::FolderNotEmpty | Code::DashboardChanged => 409,
            Code::PayloadTooLarge => 413,
            Code::UriTooLong => 414,
            Code::UnsupportedMediaType => 415,
            Code::InvalidDocument
            | Code::StoredDocumentInvalid
            | Code::NotRestorable
            | Code::IdempotencyKeyReused
            | Code::InvalidTimeRange
            | Code::VariableValueRefused
            | Code::SqlInvalid
            | Code::SqlTooLarge
            | Code::QueryLimit
            | Code::AnnotationInvalid
            | Code::QueryMemoryLimit
            | Code::QueryError => 422,
            Code::PreconditionRequired => 428,
            Code::HeadersTooLarge => 431,
            Code::InternalError => 500,
            Code::SourceAccessDenied | Code::SourceMisconfigured | Code::SourceBadResponse => 502,
            Code::PdpUnavailable
            | Code::ServerBusy
            | Code::ServerStopping
            | Code::StorageUnavailable
            | Code::SourceUnavailable
            | Code::SourceBusy => 503,
            Code::QueryTimeout => 504,
            Code::StorageFull => 507,
        };
        StatusCode::from_u16(n).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// The value of the member `retryable` (6.8).
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Code::PdpUnavailable
                | Code::ServerBusy
                | Code::ServerStopping
                | Code::StorageUnavailable
                | Code::SourceUnavailable
                | Code::SourceBusy
        )
    }

    /// The extra members of the code, in the order of the table of 6.8. The members that the
    /// table marks as optional are in `optional_members`. For `query_error` this is the list of a
    /// ClickHouse source: `extra_members_for` gives the list by the kind of the source.
    pub fn extra_members(self) -> &'static [&'static str] {
        self.extra_members_for(SourceKind::ClickHouse)
    }

    /// The extra members of the code for the kind of the source that caused the error (6.8). Only
    /// `query_error` depends on the kind: it has `clickhouse_code` for a ClickHouse source and
    /// `sqlstate` for a PostgreSQL source and `mongo_code` for a MongoDB source. The three members never
    /// appear together.
    pub fn extra_members_for(self, kind: SourceKind) -> &'static [&'static str] {
        match self {
            Code::QueryError if kind == SourceKind::Postgres => &["source", "sqlstate", "query_id"],
            Code::QueryError if kind == SourceKind::MongoDb => &["source", "mongo_code", "query_id"],
            Code::BadRequest => &["errors"],
            Code::Forbidden => &["action"],
            Code::CeilingExceeded => &["action", "ceiling", "limit"],
            Code::Conflict => &["current_etag", "current_version", "current_folder", "saved_at", "saved_by"],
            Code::DashboardChanged => &["current_etag"],
            Code::PayloadTooLarge => &["limit"],
            Code::InvalidDocument | Code::StoredDocumentInvalid => &["errors"],
            Code::VariableValueRefused => &["variable", "reason"],
            Code::SqlInvalid => &["line", "column"],
            Code::SqlTooLarge => &["size", "limit"],
            Code::QueryLimit => &["limit", "value"],
            Code::AnnotationInvalid => &["reason", "row"],
            Code::QueryError => &["source", "clickhouse_code", "query_id"],
            Code::PreconditionRequired => &["header"],
            Code::SourceAccessDenied | Code::SourceBadResponse => &["source", "query_id"],
            Code::SourceMisconfigured | Code::SourceUnavailable | Code::SourceBusy => &["source"],
            Code::QueryTimeout => &["source", "query_id"],
            _ => &[],
        }
    }

    /// The extra members that 6.8 marks as optional.
    pub fn optional_members(self) -> &'static [&'static str] {
        match self {
            Code::BadRequest => &["errors"],
            Code::SqlInvalid => &["line", "column"],
            _ => &[],
        }
    }

    /// The value of the header `Retry-After` in seconds, for the codes that need it (6.4).
    pub fn retry_after(self) -> Option<u32> {
        match self {
            Code::ServerBusy | Code::SourceBusy => Some(1),
            Code::StorageUnavailable => Some(5),
            _ => None,
        }
    }

    /// Tells if the answer closes the connection (6.8, 12.16).
    pub fn closes_connection(self) -> bool {
        matches!(self, Code::ServerBusy | Code::ServerStopping | Code::RequestTimeout)
    }
}

/// The longest `message` in bytes (6.7).
pub const MAX_MESSAGE_BYTES: usize = 1000;

/// An error answer: a code, a message and the extra members of the code.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// The code.
    pub code: Code,
    /// The English text for a person.
    pub message: String,
    /// The extra members of the code, in the order of the table.
    pub extras: Vec<(&'static str, Value)>,
    /// The value of the header `Allow` for `405`.
    pub allow: Option<String>,
    /// A fixed word that is the whole body: `{"error": "<word>"}`. Only the refusal of 17.7a uses it.
    pub plain: Option<&'static str>,
}

fn cut_message(message: String) -> String {
    if message.len() <= MAX_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_owned()
}

impl ApiError {
    /// Makes an error with no extra member.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        ApiError { code, message: cut_message(message.into()), extras: Vec::new(), allow: None, plain: None }
    }

    /// Adds one extra member. Use the members of `Code::extra_members` only.
    pub fn with(mut self, key: &'static str, value: impl Into<Value>) -> Self {
        debug_assert!(
            self.code.extra_members().contains(&key)
                || self.code.extra_members_for(SourceKind::Postgres).contains(&key)
                || self.code.extra_members_for(SourceKind::MongoDb).contains(&key),
            "{key} is not a member of {}",
            self.code.as_str()
        );
        self.extras.push((key, value.into()));
        self
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.code.status()
    }

    /// Makes the body of 6.7.
    pub fn body(&self) -> Value {
        if let Some(word) = self.plain {
            return serde_json::json!({ "error": word });
        }
        let mut inner = Map::new();
        inner.insert("code".to_owned(), Value::from(self.code.as_str()));
        inner.insert("message".to_owned(), Value::from(self.message.as_str()));
        inner.insert("retryable".to_owned(), Value::from(self.code.retryable()));
        for (k, v) in &self.extras {
            inner.insert((*k).to_owned(), v.clone());
        }
        let mut outer = Map::new();
        outer.insert("error".to_owned(), Value::Object(inner));
        Value::Object(outer)
    }

    /// `400 bad_request` with a text.
    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::new(Code::BadRequest, message)
    }

    /// `400 bad_request` with a list of errors of 4.37.
    pub fn bad_request_errors(message: impl Into<String>, errors: &[ValidationError]) -> Self {
        ApiError::new(Code::BadRequest, message).with("errors", errors_value(errors))
    }

    /// `422 invalid_document` with a list of errors of 4.37.
    pub fn invalid_document(errors: &[ValidationError]) -> Self {
        ApiError::new(Code::InvalidDocument, "The document breaks a rule of the document format.")
            .with("errors", errors_value(errors))
    }

    /// `422 stored_document_invalid` with a list of errors of 4.37.
    pub fn stored_document_invalid(errors: &[ValidationError]) -> Self {
        ApiError::new(Code::StoredDocumentInvalid, "The stored document is not valid.")
            .with("errors", errors_value(errors))
    }

    /// `404 not_found`.
    pub fn not_found(message: impl Into<String>) -> Self {
        ApiError::new(Code::NotFound, message)
    }

    /// `405 method_not_allowed` with the header `Allow`.
    pub fn method_not_allowed(allow: impl Into<String>) -> Self {
        let mut e = ApiError::new(Code::MethodNotAllowed, "The path does not take this method.");
        e.allow = Some(allow.into());
        e
    }

    /// `500 internal_error`. The message never holds a detail: the log has it.
    pub fn internal() -> Self {
        ApiError::new(Code::InternalError, "Hune has an internal fault.")
    }

    /// `413 payload_too_large` with the limit.
    pub fn payload_too_large(limit: u64) -> Self {
        ApiError::new(Code::PayloadTooLarge, format!("The body is larger than {limit} bytes.")).with("limit", limit)
    }

    /// `428 precondition_required` with the name of the header.
    pub fn precondition_required(header: &'static str) -> Self {
        ApiError::new(Code::PreconditionRequired, format!("The header {header} is necessary.")).with("header", header)
    }

    /// `409 conflict` with the state of the dashboard.
    pub fn conflict(
        current_etag: &str,
        current_version: u64,
        current_folder: Option<&str>,
        saved_at: &str,
        saved_by: &str,
    ) -> Self {
        ApiError::new(Code::Conflict, "The dashboard changed after you opened it.")
            .with("current_etag", current_etag)
            .with("current_version", current_version)
            .with("current_folder", current_folder)
            .with("saved_at", saved_at)
            .with("saved_by", saved_by)
    }
}

/// Converts validation errors to the JSON list of 4.37.
pub fn errors_value(errors: &[ValidationError]) -> Value {
    serde_json::to_value(errors).unwrap_or_else(|_| Value::Array(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The table of 6.8: code, status, retryable, extra members.
    const TABLE: &[(&str, u16, bool, &[&str])] = &[
        ("bad_request", 400, false, &["errors"]),
        ("invalid_options_request", 400, false, &[]),
        ("unauthenticated", 401, false, &[]),
        ("forbidden", 403, false, &["action"]),
        ("ceiling_exceeded", 403, false, &["action", "ceiling", "limit"]),
        ("history_disabled", 403, false, &[]),
        ("export_disabled", 403, false, &[]),
        ("read_only_store", 403, false, &[]),
        ("cross_site_request", 403, false, &[]),
        ("not_found", 404, false, &[]),
        ("history_unavailable", 404, false, &[]),
        ("method_not_allowed", 405, false, &[]),
        ("request_timeout", 408, false, &[]),
        ("conflict", 409, false, &["current_etag", "current_version", "current_folder", "saved_at", "saved_by"]),
        ("already_exists", 409, false, &[]),
        ("folder_not_empty", 409, false, &[]),
        ("dashboard_changed", 409, false, &["current_etag"]),
        ("payload_too_large", 413, false, &["limit"]),
        ("uri_too_long", 414, false, &[]),
        ("unsupported_media_type", 415, false, &[]),
        ("invalid_document", 422, false, &["errors"]),
        ("stored_document_invalid", 422, false, &["errors"]),
        ("not_restorable", 422, false, &[]),
        ("idempotency_key_reused", 422, false, &[]),
        ("invalid_time_range", 422, false, &[]),
        ("variable_value_refused", 422, false, &["variable", "reason"]),
        ("sql_invalid", 422, false, &["line", "column"]),
        ("sql_too_large", 422, false, &["size", "limit"]),
        ("query_limit", 422, false, &["limit", "value"]),
        ("annotation_invalid", 422, false, &["reason", "row"]),
        ("query_memory_limit", 422, false, &[]),
        ("query_error", 422, false, &["source", "clickhouse_code", "query_id"]),
        ("precondition_required", 428, false, &["header"]),
        ("headers_too_large", 431, false, &[]),
        ("internal_error", 500, false, &[]),
        ("source_access_denied", 502, false, &["source", "query_id"]),
        ("source_misconfigured", 502, false, &["source"]),
        ("source_bad_response", 502, false, &["source", "query_id"]),
        ("pdp_unavailable", 503, true, &[]),
        ("server_busy", 503, true, &[]),
        ("server_stopping", 503, true, &[]),
        ("storage_unavailable", 503, true, &[]),
        ("source_unavailable", 503, true, &["source"]),
        ("source_busy", 503, true, &["source"]),
        ("query_timeout", 504, false, &["source", "query_id"]),
        ("storage_full", 507, false, &[]),
    ];

    #[test]
    fn code_table_matches_6_8() {
        assert_eq!(Code::ALL.len(), TABLE.len());
        let mut seen = HashSet::new();
        for (code, (name, status, retryable, extras)) in Code::ALL.iter().zip(TABLE) {
            assert_eq!(code.as_str(), *name);
            assert_eq!(code.status().as_u16(), *status, "{name}");
            assert_eq!(code.retryable(), *retryable, "{name}");
            assert_eq!(code.extra_members(), *extras, "{name}");
            assert!(seen.insert(*name), "duplicate {name}");
        }
    }

    #[test]
    fn body_of_every_code_has_exactly_its_members() {
        for code in Code::ALL {
            let mut e = ApiError::new(code, "Text.");
            for m in code.extra_members() {
                e = e.with(m, 1);
            }
            let body = e.body();
            let inner = body["error"].as_object().unwrap();
            assert_eq!(body.as_object().unwrap().len(), 1);
            assert_eq!(inner["code"], code.as_str());
            assert_eq!(inner["message"], "Text.");
            assert_eq!(inner["retryable"], code.retryable());
            let keys: HashSet<&str> = inner.keys().map(String::as_str).collect();
            let mut want: HashSet<&str> = ["code", "message", "retryable"].into_iter().collect();
            want.extend(code.extra_members());
            assert_eq!(keys, want, "{}", code.as_str());
        }
    }

    #[test]
    fn query_error_members_follow_the_kind_of_the_source() {
        assert_eq!(
            Code::QueryError.extra_members_for(SourceKind::ClickHouse),
            ["source", "clickhouse_code", "query_id"]
        );
        assert_eq!(Code::QueryError.extra_members_for(SourceKind::Postgres), ["source", "sqlstate", "query_id"]);
        assert_eq!(Code::QueryError.extra_members_for(SourceKind::MongoDb), ["source", "mongo_code", "query_id"]);
        for code in Code::ALL.into_iter().filter(|c| *c != Code::QueryError) {
            assert_eq!(code.extra_members_for(SourceKind::Postgres), code.extra_members(), "{}", code.as_str());
            assert_eq!(code.extra_members_for(SourceKind::MongoDb), code.extra_members(), "{}", code.as_str());
        }
        let e =
            ApiError::new(Code::QueryError, "m").with("source", "m").with("sqlstate", "42P01").with("query_id", "q");
        let body = e.body();
        assert!(body["error"].get("clickhouse_code").is_none());
        assert_eq!(body["error"]["sqlstate"], "42P01");
    }

    #[test]
    fn optional_members_are_extra_members() {
        for code in Code::ALL {
            for m in code.optional_members() {
                assert!(code.extra_members().contains(m));
            }
        }
    }

    #[test]
    fn retry_after_and_close_rules() {
        assert_eq!(Code::ServerBusy.retry_after(), Some(1));
        assert_eq!(Code::SourceBusy.retry_after(), Some(1));
        assert_eq!(Code::StorageUnavailable.retry_after(), Some(5));
        assert_eq!(Code::NotFound.retry_after(), None);
        assert!(Code::ServerStopping.closes_connection());
        assert!(Code::RequestTimeout.closes_connection());
        assert!(!Code::BadRequest.closes_connection());
    }

    #[test]
    fn message_is_cut_at_1000_bytes() {
        let e = ApiError::new(Code::BadRequest, "é".repeat(800));
        assert!(e.message.len() <= MAX_MESSAGE_BYTES);
        assert!(e.message.is_char_boundary(e.message.len()));
    }

    #[test]
    fn typed_constructors() {
        let e = ApiError::conflict("e", 8, Some("transit"), "2026-11-09T10:15:03.120Z", "-");
        assert_eq!(e.status().as_u16(), 409);
        assert_eq!(e.body()["error"]["current_version"], 8);
        let e = ApiError::payload_too_large(16384);
        assert_eq!(e.body()["error"]["limit"], 16384);
        let e = ApiError::precondition_required("If-Match");
        assert_eq!(e.body()["error"]["header"], "If-Match");
        let e = ApiError::method_not_allowed("GET, HEAD");
        assert_eq!(e.allow.as_deref(), Some("GET, HEAD"));
        let e = ApiError::bad_request_errors(
            "Bad body.",
            &[ValidationError::new("/a", hune_core::error::ErrorCode::WrongType, "x")],
        );
        assert_eq!(e.body()["error"]["errors"][0]["pointer"], "/a");
        assert!(e.body()["error"]["errors"][0]["line"].is_null());
    }
}
