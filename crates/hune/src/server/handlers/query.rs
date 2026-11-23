// SPDX-License-Identifier: MIT OR Apache-2.0
//! The query routes: a panel query (6.20, 7.2), the options of a variable (6.21, 7.14) and the
//! parts that the preview shares (6.22, 7.17).

use super::dashboards::path_uid;
use super::gate;
use super::support::{StoreOp, map_store_error, services};
use crate::config::LogLevel;
use crate::server::ctx::{Ctx, Request};
use crate::server::error::{ApiError, Code};
use crate::server::json::{Member, Shape, parse_and_check_with};
use crate::server::response::{Response, bytes_response, empty_response, json_response};
use crate::server::source_blocks::{self, SourceBlock};
use crate::server::state::AppState;
use crate::telemetry::{ClientSpan, Span};
use bytes::Bytes;
use http::StatusCode;
use hune_core::document::{
    CanonicalDocument, Document, PanelData, QueryFormat, ValueType, VariableKind as DocVariableKind,
};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::frame::{Frame, FrameValue, Kind};
use hune_core::ids::Uid;
use hune_core::mongo::{MongoExpandError, MongoExpandRequest};
use hune_core::sql::adhoc::{AdhocError, AdhocRun, RequestFilter};
use hune_core::sql::{
    Dialect, ExpandError, ExpandRequest, Interval, Prepared, QueryParams, RequestValues, ResolvedValues, TextValueType,
    TimeRange, ValuesError, VariableDef, check_time_range, compute_interval, expand_prepared_adhoc, resolve_values_at,
};
use hune_source::{
    AnySource, EffectiveLimits, LogComment, PreparedQuery, QueryCtx, QueryKind, QueueWait, SourceError, SourceKind,
    SubjectId, TraceParent, Uuid, apply_time_series_rule,
};
use hune_store::{CatalogEntry, Etag, Store};
use procurauth_pep::ceilings::Ceilings;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;

/// The most rows of an options query (7.14).
pub const OPTIONS_MAX_ROWS: u64 = 10_000;

/// The status of a query that the client left: no answer (8.5, 8.9). The log line has this status.
const STATUS_CLIENT_LEFT: u16 = 499;

const ANY: Shape = Shape::Any;
const NUMBER: Shape = Shape::Int { min: i64::MIN, max: i64::MAX };

static PANEL_MEMBERS: [Member; 6] = [
    Member { name: "etag", shape: Shape::Str { min_chars: 1, max_chars: 128 }, required: true },
    Member { name: "from_ms", shape: NUMBER, required: true },
    Member { name: "to_ms", shape: NUMBER, required: true },
    Member { name: "max_data_points", shape: NUMBER, required: true },
    Member { name: "variables", shape: ANY, required: true },
    // The ref of one query of a panel with several queries (6.20, 4.15a).
    Member { name: "ref", shape: Shape::Str { min_chars: 1, max_chars: 64 }, required: false },
];
static PANEL_SHAPE: Shape = Shape::Object(&PANEL_MEMBERS);
static OPTIONS_MEMBERS: [Member; 4] = [
    Member { name: "etag", shape: Shape::Str { min_chars: 1, max_chars: 128 }, required: true },
    Member { name: "from_ms", shape: NUMBER, required: true },
    Member { name: "to_ms", shape: NUMBER, required: true },
    Member { name: "variables", shape: ANY, required: true },
];
static OPTIONS_SHAPE: Shape = Shape::Object(&OPTIONS_MEMBERS);

/// Maps the variables of a document to the definitions of the engine (7.7).
pub fn variable_defs(doc: &Document) -> Vec<VariableDef> {
    doc.variables()
        .iter()
        .filter_map(|v| {
            Some(match &v.kind {
                DocVariableKind::Query { multi, include_all, all_value, allow_custom_value, .. } => {
                    VariableDef::query(&v.name, *multi, *include_all, all_value.as_deref())
                        .with_open(*allow_custom_value)
                }
                DocVariableKind::Custom { options, multi, include_all, all_value, allow_custom_value, .. } => {
                    let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                    VariableDef::custom(&v.name, &values, *multi, *include_all, all_value.as_deref())
                        .with_open(*allow_custom_value)
                }
                DocVariableKind::Text { value_type, .. } => VariableDef::text(
                    &v.name,
                    match value_type {
                        ValueType::Text => TextValueType::Text,
                        ValueType::Number => TextValueType::Number,
                    },
                ),
                DocVariableKind::Interval { intervals, auto, auto_count, auto_min, .. } => {
                    let items: Vec<&str> = intervals.iter().map(String::as_str).collect();
                    VariableDef::interval(&v.name, &items, auto.then_some((*auto_count, auto_min.as_str())))
                }
                DocVariableKind::Constant { value } => VariableDef::constant(&v.name, value),
                // An `adhoc` variable has no value of 7.7: only `$__adhocFilters` reads its filters (7.7a, 7.10a).
                DocVariableKind::Adhoc { .. } => return None,
            })
        })
        .collect()
}

/// The filters of the `adhoc` variables of one request (7.7a): for each variable that the request names, its name and
/// its filters. Each filter passed the checks against the keys of the document and the configured list.
pub type AdhocInput = Vec<(String, Vec<RequestFilter>)>;

/// The configured list `adhoc_columns` of a source, or an empty list (no key permitted) when it is absent.
fn adhoc_columns<'a>(state: &'a AppState, source: &str) -> &'a [String] {
    block(state, source).and_then(|b| b.adhoc_columns).unwrap_or(&[])
}

fn adhoc_refused(r: hune_core::sql::ValueRefused) -> ApiError {
    ApiError::new(Code::VariableValueRefused, r.to_string()).with("variable", r.variable).with("reason", r.reason)
}

/// Takes the members of the `adhoc` variables out of `variables` of a request body (7.7a). It reads each member with
/// its closed shape (`400 bad_request`), and binds its filters to the keys of the variable and to the configured list of
/// its source (`422 variable_value_refused`). Thus a filter is never ignored, also on a route where no query has the
/// macro. It gives the body for the check of 7.7, with the other variables only, and the filters. Each query route calls
/// it before any source query.
pub fn adhoc_split(value: &Value, doc: &Document, state: &AppState) -> Result<(Value, AdhocInput), ApiError> {
    let mut variables = value.get("variables").cloned().unwrap_or(Value::Null);
    let mut input = AdhocInput::new();
    for v in doc.variables() {
        let DocVariableKind::Adhoc { source, keys } = &v.kind else { continue };
        let Some(member) = variables.as_object_mut().and_then(|o| o.remove(&v.name)) else { continue };
        let filters = hune_core::sql::adhoc::read_member(&v.name, &member).map_err(|e| match e {
            AdhocError::BadRequest(m) => ApiError::bad_request(m),
            AdhocError::Refused(r) => adhoc_refused(r),
        })?;
        hune_core::sql::adhoc::bind(&v.name, keys, adhoc_columns(state, source.as_str()), &filters)
            .map_err(adhoc_refused)?;
        input.push((v.name.clone(), filters));
    }
    Ok((serde_json::json!({ "variables": variables }), input))
}

/// The filters of the `adhoc` variable of the source of a panel query (7.10a): `None` when the document has no `adhoc`
/// variable on this source. The identifiers of the run point into the configured list (7.7a).
pub fn adhoc_run<'a>(
    doc: &'a Document,
    state: &'a AppState,
    source: &str,
    input: &AdhocInput,
) -> Result<Option<AdhocRun<'a>>, ApiError> {
    let Some((name, keys)) = doc.variables().iter().find_map(|v| match &v.kind {
        DocVariableKind::Adhoc { source: s, keys } if s.as_str() == source => Some((v.name.as_str(), keys)),
        _ => None,
    }) else {
        return Ok(None);
    };
    let filters = input.iter().find(|(n, _)| n == name).map_or(&[][..], |(_, f)| f.as_slice());
    hune_core::sql::adhoc::bind(name, keys, adhoc_columns(state, source), filters).map(Some).map_err(adhoc_refused)
}

/// Writes the log line `query.adhoc` (14.7): the count and the configured columns of the filters of a panel query.
/// It never holds a value or an operator.
fn log_adhoc(ctx: &Ctx, uid: &str, panel: u32, query_ref: Option<&str>, source: &str, run: &AdhocRun<'_>) {
    ctx.state
        .logger
        .event(LogLevel::Info, "query.adhoc")
        .request_id(&ctx.request_id)
        .field("uid", uid)
        .field("panel", panel)
        .opt_str("ref", query_ref)
        .field("source", source)
        .field("variable", run.variable())
        .field("count", run.len() as u64)
        .field("keys", Value::from(run.columns()))
        .emit();
}

fn bad_member(pointer: &str, code: ErrorCode, message: impl Into<String>) -> ApiError {
    ApiError::bad_request_errors(
        "The body does not follow the schema.",
        &[ValidationError::new(pointer, code, message)],
    )
}

/// The format fault of the member `etag` of a body. A member that is absent or not a text gives no
/// fault here: the shape check reports it.
pub(super) fn etag_faults(value: &Value) -> Vec<ValidationError> {
    match value.get("etag").and_then(Value::as_str) {
        Some(text) if Etag::parse_hex(text).is_err() => vec![ValidationError::new(
            "/etag",
            ErrorCode::BadFormat,
            "The etag must be 64 lower-case hexadecimal characters.",
        )],
        _ => Vec::new(),
    }
}

/// The format faults of the body of a panel query: the etag and the member `ref` (6.20). A `ref` that is
/// not a text gives no fault here: the shape check reports it.
fn panel_body_faults(value: &Value) -> Vec<ValidationError> {
    let mut faults = etag_faults(value);
    if let Some(text) = value.get("ref").and_then(Value::as_str)
        && !hune_core::document::is_valid_query_ref(text)
    {
        faults.push(ValidationError::new(
            "/ref",
            ErrorCode::BadFormat,
            "The ref must match ^[A-Za-z][A-Za-z0-9_]{0,15}$.",
        ));
    }
    faults
}

/// Reads the etag of a request body: 64 lower-case hexadecimal characters.
pub fn read_etag(value: &Value) -> Result<Etag, ApiError> {
    let text = value["etag"].as_str().unwrap_or("");
    Etag::parse_hex(text).map_err(|_| {
        bad_member("/etag", ErrorCode::BadFormat, "The etag must be 64 lower-case hexadecimal characters.")
    })
}

/// Reads `from_ms` and `to_ms` and checks the range (7.3). A fault gives `422 invalid_time_range`.
pub fn read_range(value: &Value) -> Result<TimeRange, ApiError> {
    let from = value["from_ms"].as_i64().unwrap_or(0);
    let to = value["to_ms"].as_i64().unwrap_or(0);
    check_time_range(from, to).map_err(|e| ApiError::new(Code::InvalidTimeRange, e.message))
}

/// Computes the interval of 7.4. A value outside 1 to 10,000 gives `400 bad_request`.
pub fn read_interval(value: &Value, range: &TimeRange, min_interval_ms: Option<u64>) -> Result<Interval, ApiError> {
    let raw = value["max_data_points"].as_i64().unwrap_or(0);
    let points = u32::try_from(raw).unwrap_or(0);
    compute_interval(range, points, min_interval_ms.unwrap_or(1).max(1))
        .map_err(|e| bad_member("/max_data_points", ErrorCode::OutOfRange, e.to_string()))
}

/// Checks the variable values of a request (7.7). `range` is the checked time range: the text of "Auto" of an
/// `interval` variable comes from it (7.4a). An `interval` value that is not an item, and a `constant` value
/// that differs from the document, give `422 variable_value_refused` here, before any source query.
pub fn read_values(value: &Value, defs: &[VariableDef], range: &TimeRange) -> Result<ResolvedValues, ApiError> {
    let request = RequestValues::from_json(&value["variables"]).map_err(values_error)?;
    resolve_values_at(defs, &request, range).map_err(values_error)
}

/// Maps the error of the check of the variable values.
pub fn values_error(e: ValuesError) -> ApiError {
    match e {
        ValuesError::BadRequest(m) => ApiError::bad_request(m),
        ValuesError::Refused(r) => ApiError::new(Code::VariableValueRefused, r.to_string())
            .with("variable", r.variable)
            .with("reason", r.reason),
    }
}

/// Maps the error of the expansion to the API error of 6.8.
pub fn expand_error(e: &ExpandError) -> ApiError {
    match e {
        ExpandError::Invalid(i) => {
            let mut err = ApiError::new(Code::SqlInvalid, i.message.clone());
            if let (Some(line), Some(column)) = (i.line, i.column) {
                err = err.with("line", line).with("column", column);
            }
            err
        }
        ExpandError::MissingVariable(_) => ApiError::bad_request(e.to_string()),
        ExpandError::ValueRefused(r) => ApiError::new(Code::VariableValueRefused, r.to_string())
            .with("variable", r.variable.clone())
            .with("reason", r.reason.clone()),
        ExpandError::TooLarge { size, limit } => {
            ApiError::new(Code::SqlTooLarge, e.to_string()).with("size", *size as u64).with("limit", *limit as u64)
        }
    }
}

fn code_of(name: &str) -> Code {
    Code::ALL.iter().copied().find(|c| c.as_str() == name).unwrap_or(Code::InternalError)
}

/// Maps the error of a source to the API error of 6.8. `action` is the PDP action of a
/// `ceiling_exceeded` error (11.24). A `query_error` of a PostgreSQL source has the member `sqlstate`,
/// a `query_error` of a MongoDB source has the member `mongo_code` and a `query_error` of a ClickHouse
/// source has the member `clickhouse_code`: the error of the source gives its own members.
pub fn source_error(e: &SourceError, action: &'static str) -> ApiError {
    // A fault of Hune (for example a refusal of the wire guard, 8b.5) has the fixed text of
    // `internal_error`: the message of the source is for the log and never goes to the client.
    if matches!(e, SourceError::Internal { .. }) {
        return ApiError::internal();
    }
    let code = code_of(e.code());
    let mut err = ApiError::new(code, e.message().to_owned());
    if code == Code::CeilingExceeded {
        err = err.with("action", action);
    }
    for (key, member) in e.members() {
        let value = match member {
            hune_source::Member::Text(t) => Value::String(t),
            hune_source::Member::Uint(n) => Value::from(n),
            hune_source::Member::Int(n) => Value::from(n),
            hune_source::Member::Null => Value::Null,
        };
        err = err.with(key, value);
    }
    err
}

/// Finds the block of the configuration of a source, of any kind.
pub fn block<'a>(state: &'a AppState, id: &str) -> Option<SourceBlock<'a>> {
    source_blocks::find(&state.config, id)
}

/// The limits of a query: the keys of the source, lowered by the ceilings of the answer (11.24).
/// Mode `open` has no ceiling (11.3). The lower value applies. When a ceiling equals the value of the
/// source, the ceiling is the cause, so a limit error is `ceiling_exceeded`.
pub fn effective_limits(block: &SourceBlock<'_>, options: bool, ceilings: &Ceilings) -> EffectiveLimits {
    let cap = if options { Some(OPTIONS_MAX_ROWS) } else { None };
    effective_limits_capped(block, cap, ceilings)
}

/// Like `effective_limits`, with a cap on the rows: the lowest of the key of the source, `cap` and the ceiling
/// applies. The options query has the cap of 7.14 and the annotation query has the cap of 7.37.
pub fn effective_limits_capped(block: &SourceBlock<'_>, cap: Option<u64>, ceilings: &Ceilings) -> EffectiveLimits {
    let mut rows = cap.map_or(block.max_result_rows, |c| block.max_result_rows.min(c));
    let mut time = block.max_execution_time;
    let mut rows_from_ceiling = false;
    let mut time_from_ceiling = false;
    if let Some(c) = ceilings.get("max_rows")
        && c <= rows
    {
        rows = c;
        rows_from_ceiling = true;
    }
    if let Some(c) = ceilings.get("max_duration")
        && c <= time.as_secs()
    {
        time = std::time::Duration::from_secs(c);
        time_from_ceiling = true;
    }
    EffectiveLimits {
        max_execution_time: time,
        max_result_rows: rows,
        max_result_bytes: block.max_result_bytes,
        max_query_size: block.max_query_size,
        rows_from_ceiling,
        time_from_ceiling,
    }
}

/// Finds the document of the request: the catalog entry, then the check of the etag (7.15).
///
/// An unknown uid gives `404`. An invalid entry gives `422 stored_document_invalid`. An etag that
/// differs from the catalog gives one read of the store. When the store has the etag of the
/// request, the catalog entry is updated. Else the answer is `409 dashboard_changed`.
pub async fn checked_document(ctx: &Ctx, uid: &Uid, etag: &Etag) -> Result<Arc<CanonicalDocument>, ApiError> {
    let services = services(ctx)?;
    let Some(entry) = services.catalog.get(uid) else {
        return Err(ApiError::not_found("No such dashboard."));
    };
    match &*entry {
        CatalogEntry::Invalid { errors, .. } => return Err(ApiError::stored_document_invalid(errors)),
        CatalogEntry::Valid { current, doc } if current.etag == *etag => return Ok(doc.clone()),
        CatalogEntry::Valid { .. } => {}
    }
    let op = StoreOp::begin(ctx, "get");
    let read = services.store.get(uid).await;
    op.end(&ctx.state, "get", &read);
    let current = read.map_err(|e| map_store_error(&e))?;
    if current.etag != *etag {
        return Err(ApiError::new(Code::DashboardChanged, "The dashboard changed after the page loaded it.")
            .with("current_etag", current.etag.to_hex()));
    }
    services.catalog.refresh_uid(&services.store, uid).await;
    match services.catalog.get(uid).as_deref() {
        Some(CatalogEntry::Valid { current, doc }) if current.etag == *etag => Ok(doc.clone()),
        _ => Err(ApiError::new(Code::DashboardChanged, "The dashboard changed after the page loaded it.")
            .with("current_etag", current.etag.to_hex())),
    }
}

/// The use question of a query route: `dashboard.read` on `N(uid)` (11.21). An unknown uid follows the
/// rule of 11.23. The decision comes before the check of the etag, so the answer tells nothing to a
/// subject without the right.
pub(super) async fn read_grant(ctx: &Ctx, uid: &Uid) -> Result<gate::Grant, ApiError> {
    let services = services(ctx)?;
    let Some(entry) = services.catalog.get(uid) else {
        return Err(gate::unknown(ctx, "No such dashboard.").await);
    };
    gate::dashboard_read(ctx, entry.folder().as_str(), uid.as_str()).await
}

/// What a query that ran gives the log line `query.run` (14.7).
pub struct RunInfo<'a> {
    pub kind: QueryKind,
    pub uid: Option<&'a str>,
    /// The folder of a preview of a dashboard that is not saved yet (8.10).
    pub folder: Option<&'a str>,
    pub panel: Option<u32>,
    /// The ref of the query, when the request of a panel has a `ref` (6.20, 8.10).
    pub query_ref: Option<&'a str>,
    pub variable: Option<&'a str>,
    /// The index of the annotation of an annotation query (8.10). It comes in place of `panel`.
    pub annotation: Option<u32>,
    pub source: &'a str,
    /// True for the query of a panel whose format is `time_series`. A `postgres` source then applies
    /// the rule of 7.16b to its frame (7.2, step 15a). The other queries set it to false.
    pub time_series: bool,
    /// The PDP action of a `ceiling_exceeded` error (11.24).
    pub action: &'static str,
}

/// The result of a query that did not give a frame.
pub enum Failure {
    /// An API error.
    Api(ApiError),
    /// The client left: there is no answer (8.5, 8.9).
    ClientLeft,
}

fn outcome_of(e: &SourceError) -> Option<&'static str> {
    Some(match e {
        SourceError::QueryTimeout { .. } => "timeout",
        SourceError::QueryLimit { .. } | SourceError::QueryMemoryLimit { .. } => "limit",
        SourceError::CeilingExceeded { .. } => "ceiling",
        SourceError::QueryError { .. }
        | SourceError::QuerySqlError { .. }
        | SourceError::QueryMongoError { .. }
        | SourceError::ProxyRefused { .. }
        | SourceError::SourceAccessDenied { .. } => "sql_error",
        SourceError::SourceUnavailable { .. }
        | SourceError::SourceBadResponse { .. }
        | SourceError::Internal { .. } => "unavailable",
        SourceError::Cancelled => "cancelled",
        // A query that never reaches the source writes no `query.run` line (14.7).
        SourceError::SourceBusy { .. } | SourceError::SourceMisconfigured { .. } => return None,
    })
}

/// The query after the expansion (7.2, steps 8 to 12, or 7.35 for a `mongodb` source).
pub struct Expanded {
    /// The query that the source runs.
    pub query: PreparedQuery,
    /// The text of the expanded query: the SQL, or the compact Extended JSON of the expanded tree
    /// (7.35, step 7). It is the text of the member `sql` (6.20) and the input of `sql_sha256` (14.7).
    /// It never goes to a log, a span or a metric.
    pub text: String,
}

/// Makes the prepared query of the kind of the source from the expanded SQL. The text of the member `sql`
/// (6.20) is the final SQL with the forms `{name:Type}` and no value of a query parameter (7.6a).
fn prepared(kind: SourceKind, sql: String, params: QueryParams) -> Expanded {
    let text = sql.clone();
    let query = match kind {
        // The engine refuses a query parameter on a `postgres` source (7.26), so `params` is empty here.
        SourceKind::Postgres => PreparedQuery::PostgresSql { sql },
        // A `mongodb` source never gets SQL: `expand_query` makes its query. The arm keeps the match
        // closed and gives the ClickHouse form, which the source refuses as a wrong kind.
        SourceKind::ClickHouse | SourceKind::MongoDb => PreparedQuery::ClickHouseSql { sql, params },
    };
    Expanded { query, text }
}

/// Maps the error of the expansion of a MongoDB query to the API error of 6.8 (7.35).
pub fn mongo_expand_error(e: &MongoExpandError) -> ApiError {
    match e {
        MongoExpandError::Invalid(i) => {
            let mut err = ApiError::new(Code::SqlInvalid, i.to_string());
            if let (Some(line), Some(column)) = (i.line, i.column) {
                err = err.with("line", line).with("column", column);
            }
            err
        }
        MongoExpandError::MissingVariable(_) => ApiError::bad_request(e.to_string()),
        MongoExpandError::ValueRefused(r) => ApiError::new(Code::VariableValueRefused, r.to_string())
            .with("variable", r.variable.clone())
            .with("reason", r.reason.clone()),
        MongoExpandError::TooLarge { size, limit } => {
            ApiError::new(Code::SqlTooLarge, e.to_string()).with("size", *size as u64).with("limit", *limit as u64)
        }
    }
}

/// Expands the query lines of a panel or a variable for the kind of the source (7.2, steps 8 to 12).
/// A `clickhouse` or `postgres` source follows 7.5 to 7.13 with its dialect. A `mongodb` source
/// follows 7.35: the parse, the grammar, the typed insertion of the values and the check of the size.
pub fn expand_query(
    cfg: &SourceBlock<'_>,
    lines: &[String],
    values: &ResolvedValues,
    range: TimeRange,
    interval: Option<Interval>,
) -> Result<Expanded, ApiError> {
    expand_query_adhoc(cfg, lines, values, range, interval, None)
}

/// Like `expand_query`, with the filters of the `adhoc` variable of the source of a panel query (7.10a).
pub fn expand_query_adhoc(
    cfg: &SourceBlock<'_>,
    lines: &[String],
    values: &ResolvedValues,
    range: TimeRange,
    interval: Option<Interval>,
    adhoc: Option<&AdhocRun<'_>>,
) -> Result<Expanded, ApiError> {
    if let Some(dialect) = cfg.dialect() {
        let Prepared { sql, params } =
            expand_sql_adhoc(dialect, &lines.join("\n"), values, range, interval, cfg.max_query_size, adhoc)?;
        return Ok(prepared(cfg.kind, sql, params));
    }
    let lists = cfg.mongo.ok_or_else(ApiError::internal)?;
    let request = MongoExpandRequest {
        values,
        range,
        interval,
        max_query_size: usize::try_from(cfg.max_query_size).unwrap_or(usize::MAX),
        collections: lists.collections,
        databases: lists.databases,
    };
    let command = hune_core::mongo::prepare(lines, &request).map_err(|e| mongo_expand_error(&e))?;
    let text = hune_core::mongo::to_query_json(&command.to_value());
    Ok(Expanded { query: PreparedQuery::MongoQuery { command }, text })
}

/// Sends one query to a source and records it in the log and the metrics (7.2, steps 14 to 16).
///
/// The future of this function holds the future of the source. When the connection of the client
/// closes, the server drops the future of the request and the connection to the source closes
/// (8.9).
pub async fn run_query(
    ctx: &Ctx,
    source: &AnySource,
    info: &RunInfo<'_>,
    expanded: Expanded,
    limits: EffectiveLimits,
) -> Result<Frame, Failure> {
    let query_id = Uuid::now_v7();
    let source_kind = source.kind();
    let span = query_span(ctx, info.source, source_kind, query_id);
    let comment = LogComment {
        kind: info.kind.into(),
        uid: info.uid.map(str::to_owned),
        folder: info.folder.map(str::to_owned),
        // The comment of an annotation query has the index in place of the panel (8.10).
        panel: info.panel.or(info.annotation),
        query_ref: info.query_ref.map(str::to_owned),
        variable: info.variable.map(str::to_owned),
        request_id: Some(ctx.request_id.clone()),
        subject: Some(ctx.subject.clone()),
    };
    let qctx = QueryCtx {
        kind: info.kind,
        query_id,
        log_comment: comment,
        subject: SubjectId::new(ctx.subject.clone()),
        traceparent: span.traceparent().and_then(|t| TraceParent::parse(&t)),
        queue_wait: QueueWait::new(),
    };
    let queue_wait = qctx.queue_wait.clone();
    // The hash of the text of the expanded query. The text itself stays here and goes nowhere else.
    let sha: String = Sha256::digest(expanded.text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let started = Instant::now();
    let guard = CancelGuard {
        ctx,
        info,
        source_kind,
        query_id,
        sha: &sha,
        limits: &limits,
        started,
        queue_wait: &queue_wait,
        done: false,
        span: Some(span),
    };
    let mut guard = guard;
    // At step 5 of the stop (14.13), the query ends: its connection to the source closes and the
    // request gets `503 server_stopping`.
    let result = tokio::select! {
        r = source.query(expanded.query, limits, qctx) => Some(r),
        () = stop_reached(ctx.state.stop_signal()) => None,
    };
    guard.done = true;
    let mut span = guard.span.take();
    let Some(result) = result else {
        end_query_span(span.take(), source_kind, Err(None));
        observe_queue_wait(ctx, info, &queue_wait);
        metrics_stopped(ctx, info, started.elapsed());
        return Err(Failure::Api(ApiError::new(Code::ServerStopping, "The server is stopping.")));
    };
    let elapsed = started.elapsed();
    let metrics = &ctx.state.metrics;
    // The wait in the admission queue (8.8): also for a refused query, which waited `queue_wait`.
    let queue = observe_queue_wait(ctx, info, &queue_wait);
    match result {
        Ok(mut frame) => {
            // Step 15a (7.2): the rule of 7.16b, for the format `time_series` of a PostgreSQL source.
            // A frame that does not pass the rule stays as it is: the page shows the error of 9.22.
            if source_kind == SourceKind::Postgres && info.time_series && info.kind != QueryKind::Options {
                let _ = apply_time_series_rule(&mut frame);
            }
            end_query_span(span.take(), source_kind, Ok(frame.rows as i64));
            metrics.observe_query(info.source, "ok", elapsed);
            metrics.query_rows(info.source, frame.rows as u64);
            let bytes = frame.stats.result_bytes.unwrap_or(0);
            metrics.query_response_bytes(info.source, bytes);
            write_run_line(
                ctx,
                info,
                source_kind,
                query_id,
                &sha,
                &limits,
                elapsed,
                queue,
                Outcome::Ok { rows: frame.rows as u64, bytes },
            );
            Ok(frame)
        }
        Err(e) => {
            end_query_span(span.take(), source_kind, Err(Some(&e)));
            if let SourceError::SourceBusy { .. } = &e {
                let reason = if e.message().contains("memory budget") { "memory" } else { "queue_wait" };
                metrics.source_refused(info.source, reason);
            }
            if let SourceError::CeilingExceeded { ceiling, limit, .. } = &e {
                ctx.state
                    .logger
                    .event(LogLevel::Info, "access.ceiling_refused")
                    .request_id(&ctx.request_id)
                    .field("subject", ctx.subject.as_str())
                    .field("action", info.action)
                    .field("ceiling", ceiling.as_str())
                    .field("limit", *limit)
                    .opt_str("uid", info.uid)
                    .field("source", info.source)
                    .emit();
            }
            if let Some(outcome) = outcome_of(&e) {
                metrics.observe_query(info.source, outcome, elapsed);
                write_run_line(
                    ctx,
                    info,
                    source_kind,
                    query_id,
                    &sha,
                    &limits,
                    elapsed,
                    queue,
                    Outcome::Failed { error: &e, outcome },
                );
            }
            if matches!(e, SourceError::Cancelled) {
                Err(Failure::ClientLeft)
            } else {
                Err(Failure::Api(source_error(&e, info.action)))
            }
        }
    }
}

/// Starts the CLIENT span of a query (14.10): `clickhouse.query`, `postgres.query` or `mongodb.query`.
/// It holds the source, the address of its server and the query id. It holds no query text and no value.
fn query_span(ctx: &Ctx, source: &str, kind: SourceKind, query_id: Uuid) -> Span {
    let (which, system) = match kind {
        SourceKind::ClickHouse => (ClientSpan::ClickhouseQuery, "clickhouse"),
        SourceKind::Postgres => (ClientSpan::PostgresQuery, "postgresql"),
        SourceKind::MongoDb => (ClientSpan::MongoQuery, "mongodb"),
    };
    let mut span = ctx.trace.client(which);
    span.set_str("db.system.name", system);
    if let Some(cfg) = block(&ctx.state, source) {
        span.set_str("server.address", cfg.host);
        span.set_int("server.port", i64::from(cfg.port));
    }
    span.set_str("hune.source", source);
    span.set_str("hune.query_id", &query_id.to_string());
    span
}

/// Ends the span of a query. `Ok(rows)` is a frame. `Err` is a failure: the error of the source, or
/// `None` for a query that ended with no result. `error.type` is the SQLSTATE for a PostgreSQL
/// source, the server code for a MongoDB source and the ClickHouse error code for a ClickHouse
/// source, or the code of the API error when the source gave none.
fn end_query_span(span: Option<Span>, kind: SourceKind, outcome: Result<i64, Option<&SourceError>>) {
    let Some(mut span) = span else { return };
    match outcome {
        Ok(rows) => span.set_int("hune.rows", rows),
        Err(error) => {
            span.set_error();
            if let Some(e) = error {
                match (kind, e.clickhouse_code(), e.sqlstate().flatten(), e.mongo_code().flatten()) {
                    (SourceKind::Postgres, _, Some(state), _) => span.set_str("error.type", state),
                    (SourceKind::ClickHouse, Some(code), _, _) => span.set_str("error.type", &code.to_string()),
                    (SourceKind::MongoDb, _, _, Some(code)) => span.set_str("error.type", &code.to_string()),
                    _ => span.set_str("error.type", e.code()),
                }
            }
        }
    }
    span.end();
}

/// Waits until the stop reaches the step that cancels the work in flight (14.13, step 5).
async fn stop_reached(mut rx: tokio::sync::watch::Receiver<crate::server::state::StopStage>) {
    loop {
        if *rx.borrow() != crate::server::state::StopStage::Running {
            return;
        }
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Adds the wait of a query in the admission queue to the histogram of the source (14.8).
/// A query that never reached the admission has no wait: nothing is recorded.
fn observe_queue_wait(ctx: &Ctx, info: &RunInfo<'_>, wait: &QueueWait) -> Option<std::time::Duration> {
    let wait = wait.get()?;
    ctx.state.metrics.observe_queue_wait(info.source, wait);
    Some(wait)
}

fn metrics_stopped(ctx: &Ctx, info: &RunInfo<'_>, elapsed: std::time::Duration) {
    ctx.state.metrics.observe_query(info.source, "cancelled", elapsed);
}

enum Outcome<'a> {
    Ok { rows: u64, bytes: u64 },
    Failed { error: &'a SourceError, outcome: &'static str },
}

#[allow(clippy::too_many_arguments)]
fn write_run_line(
    ctx: &Ctx,
    info: &RunInfo<'_>,
    source_kind: SourceKind,
    query_id: Uuid,
    sha: &str,
    limits: &EffectiveLimits,
    elapsed: std::time::Duration,
    queue: Option<std::time::Duration>,
    outcome: Outcome<'_>,
) {
    let (status, name, code, ch_code, sqlstate, mongo_code, rows, bytes) = match outcome {
        Outcome::Ok { rows, bytes } => (200u16, "ok", None, None, None, None, Some(rows), Some(bytes)),
        Outcome::Failed { error, outcome } => (
            error.http_status(),
            outcome,
            Some(error.code()),
            error.clickhouse_code(),
            // `sqlstate` is `null` for a ClickHouse source, and for an error with no SQLSTATE.
            error.sqlstate().flatten(),
            // `mongo_code` is `null` for a ClickHouse or PostgreSQL source, and for an error of Hune.
            error.mongo_code().flatten(),
            None,
            None,
        ),
    };
    ctx.state
        .logger
        .event(LogLevel::Info, "query.run")
        .request_id(&ctx.request_id)
        .field("kind", info.kind.as_str())
        .field("subject", ctx.subject.as_str())
        .opt_str("uid", info.uid)
        .field("panel", info.panel.map_or(Value::Null, Value::from))
        .opt_str("ref", info.query_ref)
        .opt_str("variable", info.variable)
        .field("annotation", info.annotation.map_or(Value::Null, Value::from))
        .field("source", info.source)
        .field("source_kind", source_kind.as_str())
        .field("query_id", query_id.to_string())
        .field("sql_sha256", sha)
        .field("queue_ms", queue.map_or(Value::Null, |q| Value::from(u64::try_from(q.as_millis()).unwrap_or(u64::MAX))))
        .field("duration_ms", u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .field("rows", rows.map_or(Value::Null, Value::from))
        .field("bytes", bytes.map_or(Value::Null, Value::from))
        .field("status", status)
        .field("outcome", name)
        .opt_str("code", code)
        .field("clickhouse_code", ch_code.map_or(Value::Null, Value::from))
        .opt_str("sqlstate", sqlstate)
        .field("mongo_code", mongo_code.map_or(Value::Null, Value::from))
        .field("max_result_rows", limits.max_result_rows)
        .field("max_execution_time_s", limits.max_execution_time.as_secs())
        .field("ceiling", limits.rows_from_ceiling || limits.time_from_ceiling)
        .emit();
}

/// Writes the log line of a query whose future the server dropped: the client left (14.7, 8.9).
struct CancelGuard<'a> {
    ctx: &'a Ctx,
    info: &'a RunInfo<'a>,
    source_kind: SourceKind,
    query_id: Uuid,
    sha: &'a str,
    limits: &'a EffectiveLimits,
    started: Instant,
    queue_wait: &'a QueueWait,
    done: bool,
    /// The CLIENT span of the query. The drop of a query that did not end ends it with the error status.
    span: Option<Span>,
}

impl Drop for CancelGuard<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        end_query_span(self.span.take(), self.source_kind, Err(None));
        let elapsed = self.started.elapsed();
        let queue = observe_queue_wait(self.ctx, self.info, self.queue_wait);
        self.ctx.state.metrics.observe_query(self.info.source, "cancelled", elapsed);
        write_run_line(
            self.ctx,
            self.info,
            self.source_kind,
            self.query_id,
            self.sha,
            self.limits,
            elapsed,
            queue,
            Outcome::Failed { error: &SourceError::Cancelled, outcome: "cancelled" },
        );
    }
}

/// The answer to a query that the client left: the status 499 and no body.
pub fn client_left_response() -> Response {
    empty_response(StatusCode::from_u16(STATUS_CLIENT_LEFT).unwrap_or(StatusCode::BAD_REQUEST))
}

/// Turns a failure into the result of a handler.
pub fn failure(f: Failure) -> Result<Response, ApiError> {
    match f {
        Failure::Api(e) => Err(e),
        Failure::ClientLeft => Ok(client_left_response()),
    }
}

/// Expands the SQL of a panel or a variable (7.2 steps 8 to 12) with the dialect of its source. It gives the final
/// SQL and the values of its query parameters (7.6a): every check of the values comes here, before any byte to the
/// source.
pub fn expand_sql(
    dialect: Dialect,
    sql: &str,
    values: &ResolvedValues,
    range: TimeRange,
    interval: Option<Interval>,
    max_query_size: u64,
) -> Result<Prepared, ApiError> {
    expand_sql_adhoc(dialect, sql, values, range, interval, max_query_size, None)
}

/// Like `expand_sql`, with the filters of the `adhoc` variable of the source of a panel query (7.10a).
pub fn expand_sql_adhoc(
    dialect: Dialect,
    sql: &str,
    values: &ResolvedValues,
    range: TimeRange,
    interval: Option<Interval>,
    max_query_size: u64,
    adhoc: Option<&AdhocRun<'_>>,
) -> Result<Prepared, ApiError> {
    let request = ExpandRequest {
        sql,
        values,
        range,
        interval,
        max_query_size: usize::try_from(max_query_size).unwrap_or(usize::MAX),
    };
    expand_prepared_adhoc(dialect, &request, adhoc).map_err(|e| expand_error(&e))
}

/// Makes the answer of a frame: the JSON of 7.16 with `Cache-Control: no-store`.
pub fn frame_response(frame: Frame, etag: Option<String>, interval_ms: Option<u64>) -> Response {
    frame_response_with_sql(frame, etag, interval_ms, None)
}

/// Like `frame_response`. With `sql`, the answer gets the member `sql` (6.20): the expanded SQL text.
/// The caller passes `Some` only when `server.query_inspect` is true and the subject may see the SQL.
/// The member holds the SQL text that Hune sent and nothing else: no setting, no header and no
/// connection value. It is the last member of the object.
pub fn frame_response_with_sql(
    frame: Frame,
    etag: Option<String>,
    interval_ms: Option<u64>,
    sql: Option<&str>,
) -> Response {
    frame_response_for_query(frame, etag, interval_ms, sql, None)
}

/// Like `frame_response_with_sql`. With `query_ref`, the answer gets the member `ref` (6.20): the ref of the
/// request. It comes before the member `sql`, which stays the last member.
pub fn frame_response_for_query(
    mut frame: Frame,
    etag: Option<String>,
    interval_ms: Option<u64>,
    sql: Option<&str>,
    query_ref: Option<&str>,
) -> Response {
    frame.etag = etag;
    frame.interval_ms = interval_ms;
    let mut json = frame.to_json();
    if let Some(r) = query_ref {
        add_text_member(&mut json, "ref", r);
    }
    if let Some(sql) = sql {
        add_sql_member(&mut json, sql);
    }
    bytes_response(StatusCode::OK, "application/json", Bytes::from(json.into_bytes()))
}

/// Adds `,"sql":"..."` before the last `}` of a JSON object.
fn add_sql_member(json: &mut String, sql: &str) {
    add_text_member(json, "sql", sql);
}

/// Adds `,"<name>":"<value>"` before the last `}` of a JSON object. The name is a fixed text.
fn add_text_member(json: &mut String, name: &str, value: &str) {
    // The JSON of a frame is an object. A text that is not an object stays as it is.
    if !json.ends_with('}') {
        return;
    }
    json.pop();
    json.push_str(",\"");
    json.push_str(name);
    json.push_str("\":");
    hune_core::frame::write_json_string(json, value);
    json.push('}');
}

/// `POST /api/dashboards/{uid}/panels/{id}/query` (6.20, 7.2).
pub async fn panel_query(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    // The id is a canonical decimal text: `+5` and `05` name no panel, as in the annotation route.
    let panel_id = ctx
        .params
        .get("id")
        .filter(|t| t.bytes().all(|b| b.is_ascii_digit()) && !t.is_empty() && (*t == "0" || !t.starts_with('0')))
        .and_then(|t| t.parse::<u32>().ok());
    let body = parse_and_check_with(&req.body, &PANEL_SHAPE, &[], panel_body_faults)?;
    let etag = read_etag(&body.value)?;
    let grant = read_grant(&ctx, &uid).await?;
    let doc = checked_document(&ctx, &uid, &etag).await?;
    let panel =
        panel_id.and_then(|id| doc.document().panel(id)).ok_or_else(|| ApiError::not_found("No such panel."))?;
    let panel_data = match &panel.data {
        PanelData::Query(query) => query,
        // A text panel has no query (4.33a, 6.20).
        PanelData::None => return Err(ApiError::bad_request("This panel has no query.")),
        PanelData::Reuse { .. } => {
            return Err(ApiError::bad_request("This panel uses the frame of another panel and has no query."));
        }
    };
    // The query that the ref names (6.20, 7.2 step 4). No ref: the first query. An unknown ref gives 404 and a
    // hidden query gives 400: both come after `dashboard.read` and before any byte to a source.
    let query_ref = body.value.get("ref").and_then(Value::as_str);
    let query = panel_data.find_query(query_ref).ok_or_else(|| ApiError::not_found("This panel has no such query."))?;
    if query.hide {
        return Err(ApiError::bad_request("This query is hidden."));
    }
    let source = ctx
        .state
        .source(query.source.as_str())
        .ok_or_else(|| ApiError::not_found("The source of the panel is not in the configuration."))?;
    // The kind of the source gives the dialect of the next steps (7.2, step 5).
    let cfg = block(&ctx.state, query.source.as_str()).ok_or_else(ApiError::internal)?;
    let range = read_range(&body.value)?;
    // The members of the panel come from `data.query` for each query (4.15a, rule 4).
    let interval = read_interval(&body.value, &range, panel_data.min_interval_ms())?;
    // The filters of the `adhoc` variables (7.7a): checked here, before any source query, and given to the macro of
    // the query when the document has an `adhoc` variable on its source (7.10a).
    let (plain, adhoc_input) = adhoc_split(&body.value, doc.document(), &ctx.state)?;
    let values = read_values(&plain, &variable_defs(doc.document()), &range)?;
    let adhoc = adhoc_run(doc.document(), &ctx.state, query.source.as_str(), &adhoc_input)?;
    let expanded = expand_query_adhoc(&cfg, query.sql, &values, range, Some(interval), adhoc.as_ref())?;
    if let Some(run) = adhoc.as_ref().filter(|r| !r.is_empty()) {
        log_adhoc(&ctx, uid.as_str(), panel.id, query_ref, query.source.as_str(), run);
    }
    let info = RunInfo {
        kind: QueryKind::Panel,
        uid: Some(uid.as_str()),
        folder: None,
        panel: Some(panel.id),
        query_ref,
        variable: None,
        annotation: None,
        source: query.source.as_str(),
        time_series: query.format == QueryFormat::TimeSeries,
        action: "dashboard.read",
    };
    // The expanded SQL goes into the answer only when the key is true and the subject may see it.
    // The copy stays in this function: it never goes to a log, a span or a metric.
    let shown = if ctx.state.config.server.query_inspect {
        let folder = services(&ctx)?.catalog.get(&uid).map(|e| e.folder().as_str().to_owned());
        match folder {
            Some(folder) if gate::query_inspect(&ctx, &folder, uid.as_str(), query.source.as_str()).await => {
                Some(expanded.text.clone())
            }
            _ => None,
        }
    } else {
        None
    };
    match run_query(&ctx, source, &info, expanded, effective_limits(&cfg, false, &grant.ceilings)).await {
        Ok(frame) => {
            Ok(frame_response_for_query(frame, Some(etag.to_hex()), Some(interval.ms), shown.as_deref(), query_ref))
        }
        Err(f) => failure(f),
    }
}

#[derive(Serialize)]
pub struct OptionItem {
    value: String,
    text: String,
}

#[derive(Serialize)]
struct OptionsAnswer<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<&'a str>,
    query_id: &'a str,
    options: Vec<OptionItem>,
}

/// The text of a time value in epoch milliseconds, from the Unix seconds with a fraction (7.14).
fn time_ms_text(seconds: &str) -> Option<String> {
    let (negative, digits) = match seconds.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, seconds),
    };
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let int: i128 = int.parse().ok()?;
    let mut millis = String::from(frac);
    millis.truncate(3);
    while millis.len() < 3 {
        millis.push('0');
    }
    let total = int.checked_mul(1000)?.checked_add(millis.parse::<i128>().ok()?)?;
    Some(if negative { format!("{}", -total) } else { total.to_string() })
}

fn option_text(kind: Kind, v: FrameValue<'_>) -> Option<String> {
    match v {
        FrameValue::Null => None,
        FrameValue::Bool(b) => Some(b.to_string()),
        FrameValue::Text(t) => Some(t.to_owned()),
        FrameValue::Number(n) => Some(if kind == Kind::Time { time_ms_text(n)? } else { n.to_owned() }),
    }
}

/// Makes the options of a variable from the frame (7.14). The first column is the value. The
/// second column, when it exists, is the text. A row with a `null` value is left out.
pub fn options_from_frame(frame: &Frame) -> Vec<OptionItem> {
    let Some(first) = frame.columns.first() else { return Vec::new() };
    let second = frame.columns.get(1);
    let mut out = Vec::with_capacity(frame.rows);
    for row in 0..frame.rows {
        let Some(value) = first.value(row).and_then(|v| option_text(first.kind, v)) else { continue };
        let text =
            second.and_then(|c| c.value(row).and_then(|v| option_text(c.kind, v))).unwrap_or_else(|| value.clone());
        out.push(OptionItem { value, text });
    }
    out
}

/// Makes the answer of an options query (6.21).
pub fn options_response(frame: &Frame, etag: Option<&str>) -> Response {
    let body = OptionsAnswer { etag, query_id: &frame.query_id, options: options_from_frame(frame) };
    json_response(StatusCode::OK, &body)
}

/// `POST /api/dashboards/{uid}/variables/{name}/options` (6.21, 7.14).
pub async fn variable_options(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let name = ctx.params.get("name").unwrap_or("").to_owned();
    let body = parse_and_check_with(&req.body, &OPTIONS_SHAPE, &[], etag_faults)?;
    let etag = read_etag(&body.value)?;
    let grant = read_grant(&ctx, &uid).await?;
    let doc = checked_document(&ctx, &uid, &etag).await?;
    let variable = doc.document().variable(&name).ok_or_else(|| ApiError::not_found("No such variable."))?;
    let DocVariableKind::Query { source: source_id, sql, .. } = &variable.kind else {
        return Err(ApiError::bad_request("The options route needs a variable of the kind query."));
    };
    let source = ctx
        .state
        .source(source_id.as_str())
        .ok_or_else(|| ApiError::not_found("The source of the variable is not in the configuration."))?;
    let cfg = block(&ctx.state, source_id.as_str()).ok_or_else(ApiError::internal)?;
    let range = read_range(&body.value)?;
    // The filters of an `adhoc` variable are checked on this route too (7.7a); a variable query has no macro.
    let (plain, _) = adhoc_split(&body.value, doc.document(), &ctx.state)?;
    let values = read_values(&plain, &variable_defs(doc.document()), &range)?;
    let expanded = expand_query(&cfg, sql, &values, range, None)?;
    let info = RunInfo {
        kind: QueryKind::Options,
        uid: Some(uid.as_str()),
        folder: None,
        panel: None,
        query_ref: None,
        variable: Some(&name),
        annotation: None,
        source: source_id.as_str(),
        // The options query never applies the rule of 7.16b (7.16b).
        time_series: false,
        action: "dashboard.read",
    };
    match run_query(&ctx, source, &info, expanded, effective_limits(&cfg, true, &grant.ceilings)).await {
        Ok(frame) => Ok(options_response(&frame, Some(&etag.to_hex()))),
        Err(f) => failure(f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sql_member_is_added_as_the_last_member_and_escaped() {
        let mut json = String::from("{\"a\":1,\"stats\":{\"b\":2}}");
        add_sql_member(&mut json, "SELECT 'x'\n\t\"q\" \\ \u{1}");
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["sql"], "SELECT 'x'\n\t\"q\" \\ \u{1}");
        assert_eq!(value["a"], 1);
        assert_eq!(value["stats"]["b"], 2);
        assert!(json.ends_with("\"}"));
        // A text that is not an object stays as it is.
        let mut other = String::from("[1]");
        add_sql_member(&mut other, "x");
        assert_eq!(other, "[1]");
    }

    #[test]
    fn time_values_become_epoch_milliseconds() {
        assert_eq!(time_ms_text("1794211200").as_deref(), Some("1794211200000"));
        assert_eq!(time_ms_text("1794211200.125").as_deref(), Some("1794211200125"));
        assert_eq!(time_ms_text("1794211200.5").as_deref(), Some("1794211200500"));
        assert_eq!(time_ms_text("1794211200.123456").as_deref(), Some("1794211200123"));
        assert_eq!(time_ms_text("0").as_deref(), Some("0"));
        assert_eq!(time_ms_text("x"), None);
    }

    #[test]
    fn source_errors_map_to_the_codes_of_6_8() {
        use hune_core::ids::SourceId;
        use hune_source::{Ceiling, LimitName};
        let source = SourceId::parse("prod").unwrap();
        let query_id = Uuid::new_v7(0, [0; 10]);
        let cases: Vec<(SourceError, Code)> = vec![
            (SourceError::QueryTimeout { source: source.clone(), query_id, message: "m".into() }, Code::QueryTimeout),
            (
                SourceError::CeilingExceeded { ceiling: Ceiling::MaxRows, limit: 5, message: "m".into() },
                Code::CeilingExceeded,
            ),
            (
                SourceError::QueryLimit { limit: LimitName::MaxResultRows, value: Some(3), message: "m".into() },
                Code::QueryLimit,
            ),
            (SourceError::QueryMemoryLimit { message: "m".into() }, Code::QueryMemoryLimit),
            (
                SourceError::SourceAccessDenied { source: source.clone(), query_id, message: "m".into() },
                Code::SourceAccessDenied,
            ),
            (
                SourceError::SourceMisconfigured { source: source.clone(), message: "m".into() },
                Code::SourceMisconfigured,
            ),
            (SourceError::SourceUnavailable { source: source.clone(), message: "m".into() }, Code::SourceUnavailable),
            (SourceError::SourceBusy { source: source.clone(), message: "m".into() }, Code::SourceBusy),
            (
                SourceError::SourceBadResponse { source: source.clone(), query_id, message: "m".into() },
                Code::SourceBadResponse,
            ),
            (
                SourceError::QueryError { source: source.clone(), clickhouse_code: 62, query_id, message: "m".into() },
                Code::QueryError,
            ),
        ];
        for (e, code) in cases {
            let api = source_error(&e, "dashboard.read");
            assert_eq!(api.code, code);
            assert_eq!(api.status().as_u16(), e.http_status());
            let body = api.body();
            let inner = body["error"].as_object().unwrap();
            let mut want: Vec<&str> = vec!["code", "message", "retryable"];
            want.extend(code.extra_members());
            let mut got: Vec<&str> = inner.keys().map(String::as_str).collect();
            want.sort_unstable();
            got.sort_unstable();
            assert_eq!(got, want, "{}", code.as_str());
            assert_eq!(inner["retryable"], e.retryable());
        }
        let ceiling = source_error(
            &SourceError::CeilingExceeded { ceiling: Ceiling::MaxDuration, limit: 7, message: "m".into() },
            "source.query",
        );
        assert_eq!(ceiling.body()["error"]["action"], "source.query");
        assert_eq!(ceiling.body()["error"]["ceiling"], "max_duration");
        assert_eq!(ceiling.body()["error"]["limit"], 7);
        let limit = source_error(
            &SourceError::QueryLimit { limit: LimitName::MaxResult, value: None, message: "m".into() },
            "dashboard.read",
        );
        assert_eq!(limit.body()["error"]["limit"], "max_result");
        assert!(limit.body()["error"]["value"].is_null());
    }

    #[test]
    fn a_query_error_of_a_postgres_source_has_sqlstate_and_no_clickhouse_code() {
        use hune_core::ids::SourceId;
        let source = SourceId::parse("metrics").unwrap();
        let query_id = Uuid::new_v7(0, [0; 10]);
        for sqlstate in [Some("42P01".to_owned()), None] {
            let e = SourceError::QuerySqlError {
                source: source.clone(),
                sqlstate: sqlstate.clone(),
                query_id,
                message: "m".into(),
            };
            let api = source_error(&e, "dashboard.read");
            assert_eq!((api.code, api.status().as_u16()), (Code::QueryError, 422));
            let body = api.body();
            let inner = body["error"].as_object().unwrap();
            let mut got: Vec<&str> = inner.keys().map(String::as_str).collect();
            got.sort_unstable();
            let mut want: Vec<&str> = vec!["code", "message", "retryable"];
            want.extend(Code::QueryError.extra_members_for(SourceKind::Postgres));
            want.sort_unstable();
            assert_eq!(got, want);
            assert!(inner.get("clickhouse_code").is_none());
            assert_eq!(inner["source"], "metrics");
            assert_eq!(inner["sqlstate"], sqlstate.map_or(Value::Null, Value::from));
        }
    }

    #[test]
    fn expand_errors_map_to_the_codes_of_6_8() {
        use hune_core::sql::{SqlInvalid, SqlInvalidKind, Stage, ValueRefused};
        let invalid = ExpandError::Invalid(SqlInvalid {
            kind: SqlInvalidKind::Lexer,
            message: "m".into(),
            stage: Stage::Stored,
            line: Some(2),
            column: Some(5),
        });
        let api = expand_error(&invalid);
        assert_eq!(api.code, Code::SqlInvalid);
        assert_eq!((api.body()["error"]["line"].as_u64(), api.body()["error"]["column"].as_u64()), (Some(2), Some(5)));
        let expanded = ExpandError::Invalid(SqlInvalid {
            kind: SqlInvalidKind::Statement,
            message: "m".into(),
            stage: Stage::Expanded,
            line: None,
            column: None,
        });
        assert!(expand_error(&expanded).body()["error"].get("line").is_none());
        let refused = ExpandError::ValueRefused(ValueRefused { variable: "v".into(), reason: "r".into() });
        let api = expand_error(&refused);
        assert_eq!(
            (api.code, api.body()["error"]["variable"].as_str(), api.body()["error"]["reason"].as_str()),
            (Code::VariableValueRefused, Some("v"), Some("r"))
        );
        let large = expand_error(&ExpandError::TooLarge { size: 10, limit: 5 });
        assert_eq!(
            (large.code, large.body()["error"]["size"].as_u64(), large.body()["error"]["limit"].as_u64()),
            (Code::SqlTooLarge, Some(10), Some(5))
        );
        assert_eq!(expand_error(&ExpandError::MissingVariable("v".into())).code, Code::BadRequest);
    }
}
