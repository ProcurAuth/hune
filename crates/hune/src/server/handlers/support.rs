// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers of the store-backed handlers. They give the services, the error mapping of 5.6 and the
//! write metadata. They also hold the log line of a change and the save checks of 7.18.

use crate::config::LogLevel;
use crate::server::ctx::{BodyPlace, Ctx};
use crate::server::error::{ApiError, Code};
use crate::server::limits::VALIDATION_WAIT;
use crate::server::source_blocks::SourceBlock;
use crate::server::state::{AppState, Services, SubjectSlot};
use hune_core::document::{CanonicalDocument, Document};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::ids::Uid;
use hune_store::{FolderId, Message, StoreError, Subject, UtcMillis, WriteMeta};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::OwnedSemaphorePermit;

/// The result of the validation of a document (4.36) and of its save checks (7.18): the document and its
/// warnings, or the list of errors.
pub type Checked = Result<(CanonicalDocument, Vec<SaveWarning>), Vec<ValidationError>>;

/// The permits of one validation in `spawn_blocking`: a permit of `MAX_INFLIGHT_VALIDATIONS` and, in the mode
/// `authzen`, a place of the subject (6.27, 12.16). They are free again when the value drops.
pub struct CheckPermit {
    _permit: OwnedSemaphorePermit,
    _subject: Option<SubjectSlot>,
}

fn validations_busy() -> ApiError {
    ApiError::new(Code::ServerBusy, "Too many validations of a document are in flight.")
}

/// Takes the permits of one validation (6.27, 12.16). In the mode `authzen`, a subject that holds
/// `MAX_INFLIGHT_PER_SUBJECT` validations already gets `503 server_busy` at once. Then the request waits for a
/// permit for `VALIDATION_WAIT` at most. No permit in this time gives `503 server_busy`.
pub async fn check_permit(state: &AppState, subject: &str) -> Result<CheckPermit, ApiError> {
    let slot = if state.access.is_some() {
        Some(state.subject_validations.try_take(subject).ok_or_else(validations_busy)?)
    } else {
        None
    };
    match tokio::time::timeout(VALIDATION_WAIT, state.validations.clone().acquire_owned()).await {
        Ok(Ok(permit)) => Ok(CheckPermit { _permit: permit, _subject: slot }),
        Ok(Err(_)) | Err(_) => Err(validations_busy()),
    }
}

/// Runs the CPU work on a document in `spawn_blocking`, never on an async worker (3.7, 6.27). The permits and
/// the place of the body move into the task, as the guard of the volume store does: they stay taken until the work
/// ends, also when the client closes the connection first. A failed task gives `500`.
pub async fn run_check<T: Send + 'static>(
    state: &Arc<AppState>,
    permit: CheckPermit,
    place: Option<BodyPlace>,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    #[cfg(test)]
    let state = Arc::clone(state);
    #[cfg(not(test))]
    let _ = state;
    tokio::task::spawn_blocking(move || {
        let _held = (permit, place);
        #[cfg(test)]
        state.validations_run.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        work()
    })
    .await
    .map_err(|_| ApiError::internal())
}

/// Validates the bytes of a document (4.36) and runs its save checks (7.18). It runs in `spawn_blocking`.
fn check_bytes(state: &AppState, bytes: &[u8], expected: Option<&Uid>) -> Checked {
    let doc = hune_core::document::validate(bytes, expected)?;
    let (errors, warnings) = sql_save_checks(doc.document(), &declared_sources(state));
    if errors.is_empty() { Ok((doc, warnings)) } else { Err(errors) }
}

/// Validates a document and runs its save checks in `spawn_blocking`, with the permits of 6.27 and 12.16.
///
/// The work never runs on an async worker, so a slow document cannot block the other requests. With `detach`, the
/// work is a task of its own: when the client closes the connection, the task still waits for its permit and runs
/// to its end, and it keeps the place of the body until then. A create and a save use it, so the 8 places of the
/// document bodies bound the work that waits or runs (6.27). Without `detach`, a request that the client leaves
/// before its permit comes starts no work.
pub async fn validate_document(
    ctx: &Ctx,
    place: Option<BodyPlace>,
    bytes: Vec<u8>,
    expected: Option<Uid>,
    detach: bool,
) -> Result<Checked, ApiError> {
    let state = Arc::clone(&ctx.state);
    let subject = ctx.subject.clone();
    let job = async move {
        let permit = check_permit(&state, &subject).await?;
        let st = Arc::clone(&state);
        run_check(&state, permit, place, move || check_bytes(&st, &bytes, expected.as_ref())).await
    };
    if detach { tokio::spawn(job).await.map_err(|_| ApiError::internal())? } else { job.await }
}

/// Gives the services of the store, or `503 storage_unavailable` before they exist.
pub fn services(ctx: &Ctx) -> Result<&Services, ApiError> {
    ctx.state.services().ok_or_else(|| ApiError::new(Code::StorageUnavailable, "The store is not ready."))
}

/// The time now, cut to the millisecond.
pub fn now() -> UtcMillis {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    UtcMillis::from_millis(ms)
}

/// Makes the metadata of a write (5.2): the subject of the request, the time now and the message.
pub fn write_meta(ctx: &Ctx, message: Message) -> WriteMeta {
    WriteMeta {
        saved_by: Subject::parse(&ctx.subject).unwrap_or_else(|_| Subject::anonymous()),
        saved_at: now(),
        message,
        save_id: ctx.save_id,
    }
}

/// Maps an error of the store to the answer of 5.6.
pub fn map_store_error(e: &StoreError) -> ApiError {
    match e {
        StoreError::NotFound => ApiError::not_found("No such dashboard."),
        StoreError::FolderNotFound => ApiError::not_found("No such folder."),
        StoreError::VersionNotFound => ApiError::not_found("No such version."),
        StoreError::Exists { .. } => ApiError::new(Code::AlreadyExists, "This id exists already."),
        StoreError::Conflict(c) => ApiError::conflict(
            &c.etag.to_hex(),
            c.version,
            c.folder.as_str(),
            &c.saved_at.to_rfc3339(),
            c.saved_by.as_str(),
        ),
        StoreError::FolderNotEmpty => ApiError::new(Code::FolderNotEmpty, "The folder holds a dashboard."),
        StoreError::ReadOnly => ApiError::new(Code::EditingDisabled, "The store is read-only."),
        StoreError::Invalid(errors) => ApiError::stored_document_invalid(errors),
        StoreError::NotRestorable => ApiError::new(Code::NotRestorable, "This version cannot be restored."),
        StoreError::SaveIdReused => {
            ApiError::new(Code::IdempotencyKeyReused, "The Idempotency-Key belongs to a different request.")
        }
        StoreError::Full => ApiError::new(Code::StorageFull, "The store has no space left."),
        StoreError::Unavailable(_) => {
            ApiError::new(Code::StorageUnavailable, "The store cannot do this operation now.")
        }
        StoreError::NotEmpty => ApiError::internal(),
    }
}

/// The label `status` of `hune_store_operations_total` (14.8) for a result.
fn status_label(e: Option<&StoreError>) -> &'static str {
    match e {
        None => "ok",
        Some(StoreError::Conflict(_)) => "conflict",
        Some(StoreError::Unavailable(_)) => "unavailable",
        Some(StoreError::Full) => "full",
        Some(StoreError::Invalid(_) | StoreError::NotRestorable) => "invalid",
        Some(_) => "ok",
    }
}

/// One call to the store: the start time and the CLIENT span `store.<op>` (14.10). Call `begin` before
/// the call and `end` after it. `end` records the metrics and ends the span.
pub struct StoreOp {
    started: Instant,
    span: crate::telemetry::Span,
}

impl StoreOp {
    /// Starts the call. The span is a child of the SERVER span of the request.
    pub fn begin(ctx: &Ctx, op: &str) -> StoreOp {
        let mut span = ctx.trace.client(crate::telemetry::ClientSpan::StoreOperation(op));
        span.set_str("hune.store.backend", ctx.state.backend_name());
        if ctx.state.backend_name() == "postgres" {
            span.set_str("db.system.name", "postgresql");
        }
        StoreOp { started: Instant::now(), span }
    }

    /// Ends the call: the metrics of `record_op`, then the span. An error sets the error status.
    pub fn end<T>(mut self, state: &AppState, op: &str, result: &Result<T, StoreError>) {
        record_op(state, op, self.started, result);
        if result.is_err() {
            self.span.set_error();
        }
        self.span.end();
    }
}

/// Records one operation of the store in the metrics, and the conflicts.
pub fn record_op<T>(state: &AppState, op: &str, started: Instant, result: &Result<T, StoreError>) {
    let err = result.as_ref().err();
    if matches!(err, Some(StoreError::Conflict(_))) {
        state.metrics.store_conflict();
    }
    state.metrics.observe_store_op(state.backend_name(), op, status_label(err), started.elapsed());
}

/// Checks the message of a write: no control character (6.13). The length is checked by the shape.
pub fn read_message(body: &Value) -> Result<Message, ApiError> {
    let text = body.get("message").and_then(Value::as_str).unwrap_or("");
    if text.chars().any(char::is_control) {
        return Err(ApiError::bad_request_errors(
            "The body does not follow the schema.",
            &[ValidationError::new("/message", ErrorCode::BadFormat, "The message must hold no control character.")],
        ));
    }
    Message::parse(text).map_err(|e| {
        ApiError::bad_request_errors(
            "The body does not follow the schema.",
            &[ValidationError::new("/message", ErrorCode::OutOfRange, e.0)],
        )
    })
}

/// The format faults of the members `folder` or `id` (a folder id), `title` and `message` of a body.
/// A member that is absent or has another type gives no fault here: the shape check reports it.
/// `folder_members` names the members that hold a folder id.
pub fn format_faults(
    body: &Value,
    folder_members: &[&str],
    with_title: bool,
    with_message: bool,
) -> Vec<ValidationError> {
    let mut out = Vec::new();
    for name in folder_members {
        if let Some(text) = body.get(*name).and_then(Value::as_str)
            && let Err(e) = FolderId::parse(text)
        {
            out.push(ValidationError::new(format!("/{name}"), ErrorCode::BadFormat, e.to_string()));
        }
    }
    if with_title
        && let Some(text) = body.get("title").and_then(Value::as_str)
        && let Err(e) = hune_store::FolderTitle::parse(text)
    {
        out.push(ValidationError::new("/title", ErrorCode::BadFormat, e.0));
    }
    if with_message
        && let Some(text) = body.get("message").and_then(Value::as_str)
        && text.chars().any(char::is_control)
    {
        out.push(ValidationError::new("/message", ErrorCode::BadFormat, "The message must hold no control character."));
    }
    out
}

/// Reads a folder id from a text of the body. A fault is a `bad_request` with the pointer.
pub fn read_folder_id(text: &str, pointer: &str) -> Result<FolderId, ApiError> {
    FolderId::parse(text).map_err(|e| {
        ApiError::bad_request_errors(
            "The body does not follow the schema.",
            &[ValidationError::new(pointer, ErrorCode::BadFormat, e.to_string())],
        )
    })
}

/// Writes the log line `dashboard.changed` (14.7). It holds no SQL and no document body.
#[allow(clippy::too_many_arguments)]
pub fn log_dashboard_changed(
    ctx: &Ctx,
    action: &str,
    uid: &str,
    folder: &str,
    from_folder: Option<&str>,
    version: u64,
    etag_before: Option<&str>,
    etag_after: Option<&str>,
    bytes: Option<u64>,
) {
    ctx.state
        .logger
        .event(LogLevel::Info, "dashboard.changed")
        .request_id(&ctx.request_id)
        .field("action", action)
        .field("uid", uid)
        .field("folder", folder)
        .opt_str("from_folder", from_folder)
        .field("version", version)
        .opt_str("etag_before", etag_before)
        .opt_str("etag_after", etag_after)
        .field("subject", ctx.subject.as_str())
        .opt_str("save_id", ctx.save_id.map(|s| s.to_string()).as_deref())
        .field("bytes", bytes.map_or(Value::Null, Value::from))
        .emit();
}

/// A warning of 7.18: a pointer and a message. A warning never stops a write. It is the warning of the bounded
/// report of the core (4.35, 6.13).
pub type SaveWarning = hune_core::report::Warning;

/// The checks at save of 7.18 on the document that passed 4.36. They run the checks 1 to 8 and
/// the warnings 1 to 5 on each SQL text, then the warning 6 on the display texts. They also run the check 9 (the source id) at the member
/// `source`. Each text is checked with the language of its source (7.18, 7.26, 7.36): the kind of the
/// source gives it. A `mongodb` source runs the checks L1 to L9 and the warnings 1 to 3 of 7.36 on its
/// texts. `declared` lists the blocks of the sources of the configuration.
///
/// The items go into one bounded report of the core: each list holds at most its cap, and its last item says that
/// the list stops (4.35, 4.36, 6.13). A check never makes an item past the cap. The checks stop when the list of
/// errors stopped, because no other check can change the answer then.
pub fn sql_save_checks(doc: &Document, declared: &[SourceBlock<'_>]) -> (Vec<ValidationError>, Vec<SaveWarning>) {
    use hune_core::document::SqlOwner;
    use hune_core::mongo::SaveContext;
    use hune_core::report::SaveReport;
    use hune_core::sql::{AdhocSave, SqlUse, check_source, save_checks_into_adhoc};
    let declared_sources: Vec<&str> = declared.iter().map(|d| d.id).collect();
    let defs = super::query::variable_defs(doc);
    let mut report = SaveReport::new();
    adhoc_save_checks(doc, declared, &mut report);
    let adhoc_names: Vec<&str> = doc
        .variables()
        .iter()
        .filter(|v| matches!(v.kind, hune_core::document::VariableKind::Adhoc { .. }))
        .map(|v| v.name.as_str())
        .collect();
    for text in doc.sql_texts() {
        if report.errors_stopped() {
            break;
        }
        if let Err(e) = check_source(text.source.as_str(), &declared_sources) {
            report.error_with(|| {
                let pointer = match text.pointer.strip_suffix("/sql") {
                    Some(base) => format!("{base}/source"),
                    None => text.pointer.clone(),
                };
                ValidationError::new(
                    pointer,
                    e.code,
                    format!("source must be a source id of the configuration; value {}", text.source),
                )
            });
        }
        let usage = match text.owner {
            SqlOwner::Variable { .. } => SqlUse::Variable,
            SqlOwner::Panel { .. } => SqlUse::Panel,
            SqlOwner::Annotation { .. } => SqlUse::Annotation,
        };
        let block = declared.iter().find(|d| d.id == text.source.as_str());
        // An annotation runs on a `clickhouse`, a `postgres` or a `mongodb` source (4.5b): its text gets the checks of the kind below. A kind that 8.2 adds later is refused, and the
        // text of the annotation then gets no other check.
        if let SqlOwner::Annotation { index } = text.owner
            && let Some(b) = block
            && !super::annotation::annotation_kind(b.kind)
        {
            report.error_with(|| {
                ValidationError::new(
                    format!("/annotations/{index}/source"),
                    ErrorCode::NotPermitted,
                    format!(
                        "source must be a source of the kind clickhouse, postgres or mongodb for an annotation; value {} of the kind {}",
                        text.source,
                        b.kind.as_str()
                    ),
                )
            });
            continue;
        }
        if let Some(lists) = block.and_then(|b| b.mongo) {
            // The language of a `mongodb` source: 7.28 to 7.36. An unknown source id gives item 9 only.
            let ctx =
                SaveContext { variables: &defs, collections: lists.collections, databases: lists.databases, usage };
            hune_core::mongo::save_checks_into(text.lines, &ctx, &text.pointer, &mut report);
            continue;
        }
        let dialect = block.and_then(SourceBlock::dialect).unwrap_or(hune_core::sql::Dialect::ClickHouse);
        let on_source = doc.variables().iter().any(
            |v| matches!(&v.kind, hune_core::document::VariableKind::Adhoc { source, .. } if source == text.source),
        );
        let adhoc = AdhocSave { names: &adhoc_names, on_source };
        save_checks_into_adhoc(dialect, &text.text(), usage, &defs, adhoc, &text.pointer, &mut report);
    }
    hune_core::document::text_warnings_into(doc, &mut report);
    report.into_parts()
}

/// The checks at save of each `adhoc` variable (7.18, items 17 and 18): its source is a declared `clickhouse` source
/// with the key `adhoc_columns`, and each key is in that list. A change of the list later fails closed at run time
/// (7.7a), and the page offers only the keys of the list (6.14).
fn adhoc_save_checks(doc: &Document, declared: &[SourceBlock<'_>], report: &mut hune_core::report::SaveReport) {
    for (i, v) in doc.variables().iter().enumerate() {
        let hune_core::document::VariableKind::Adhoc { source, keys } = &v.kind else { continue };
        let pointer = format!("/variables/{i}/source");
        let Some(block) = declared.iter().find(|d| d.id == source.as_str()) else {
            report.error_with(|| {
                ValidationError::new(
                    pointer,
                    ErrorCode::BadReference,
                    format!("source must be a source id of the configuration; value {source}"),
                )
            });
            continue;
        };
        if block.kind != hune_source::SourceKind::ClickHouse {
            report.error_with(|| {
                ValidationError::new(
                    pointer,
                    ErrorCode::NotPermitted,
                    format!(
                        "source must be a source of the kind clickhouse for an adhoc variable; value {source} of the kind {}",
                        block.kind.as_str()
                    ),
                )
            });
            continue;
        }
        let Some(columns) = block.adhoc_columns else {
            report.error_with(|| {
                ValidationError::new(
                    pointer,
                    ErrorCode::NotPermitted,
                    format!("source must have the key adhoc_columns in the configuration for an adhoc variable; value {source}"),
                )
            });
            continue;
        };
        for k in hune_core::sql::adhoc::keys_outside(keys, columns) {
            report.error_with(|| {
                ValidationError::new(
                    format!("/variables/{i}/keys/{k}"),
                    ErrorCode::NotPermitted,
                    format!("key must be an item of clickhouse \"{source}\".adhoc_columns; value \"{}\"", keys[k]),
                )
            });
        }
    }
}

/// The blocks of the sources that the configuration declares, of the three kinds.
pub fn declared_sources(state: &AppState) -> Vec<SourceBlock<'_>> {
    crate::server::source_blocks::blocks(&state.config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hune_core::ids::Uid;
    use hune_source::SourceKind;
    use hune_store::ConflictInfo;

    #[test]
    fn store_error_mapping_5_6() {
        use Code::*;
        let cases: Vec<(StoreError, Code)> = vec![
            (StoreError::NotFound, NotFound),
            (StoreError::FolderNotFound, NotFound),
            (StoreError::VersionNotFound, NotFound),
            (StoreError::Exists { folder: None }, AlreadyExists),
            (StoreError::FolderNotEmpty, Code::FolderNotEmpty),
            (StoreError::ReadOnly, EditingDisabled),
            (StoreError::Invalid(vec![]), StoredDocumentInvalid),
            (StoreError::NotRestorable, Code::NotRestorable),
            (StoreError::SaveIdReused, IdempotencyKeyReused),
            (StoreError::Full, StorageFull),
            (StoreError::Unavailable("io error at /secret/path".into()), StorageUnavailable),
            (StoreError::NotEmpty, InternalError),
        ];
        for (e, code) in cases {
            let api = map_store_error(&e);
            assert_eq!(api.code, code, "{e}");
            assert!(!api.message.contains("/secret/path"));
        }
        let info = ConflictInfo {
            etag: hune_core::etag::Etag::of(b"x"),
            version: 8,
            folder: FolderId::parse("transit").unwrap(),
            saved_at: UtcMillis::from_millis(0),
            saved_by: Subject::anonymous(),
        };
        let api = map_store_error(&StoreError::Conflict(Box::new(info)));
        assert_eq!(api.status().as_u16(), 409);
        assert_eq!(api.body()["error"]["current_version"], 8);
        assert_eq!(api.body()["error"]["current_folder"], "transit");
        assert_eq!(api.body()["error"]["saved_at"], "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn status_labels_are_in_the_closed_list() {
        for e in [
            None,
            Some(StoreError::Full),
            Some(StoreError::NotFound),
            Some(StoreError::Unavailable(String::new())),
            Some(StoreError::NotRestorable),
        ] {
            assert!(["ok", "conflict", "unavailable", "full", "invalid"].contains(&status_label(e.as_ref())));
        }
    }

    #[test]
    fn message_rules() {
        let ok = serde_json::json!({"message": "Fix the query"});
        assert_eq!(read_message(&ok).unwrap().as_str(), "Fix the query");
        assert_eq!(read_message(&serde_json::json!({})).unwrap().as_str(), "");
        let bad = serde_json::json!({"message": "a\nb"});
        let e = read_message(&bad).unwrap_err();
        assert_eq!(e.body()["error"]["errors"][0]["pointer"], "/message");
    }

    #[test]
    fn folder_id_rule() {
        assert!(read_folder_id("transit", "/folder").is_ok());
        let e = read_folder_id("Bad_Id", "/folder").unwrap_err();
        assert_eq!(e.body()["error"]["errors"][0]["code"], "bad_format");
    }

    fn doc_with_source(source: &str) -> hune_core::document::CanonicalDocument {
        let text =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/dashboards/nodes.json"))
                .unwrap();
        let text = text.replace("\"source\": \"prod\"", &format!("\"source\": \"{source}\""));
        hune_core::document::validate(text.as_bytes(), Some(&Uid::parse("nodes").unwrap())).expect("fixture is valid")
    }

    #[test]
    fn source_reference_check_item_9() {
        let ok = doc_with_source("prod");
        let (errors, warnings) = sql_save_checks(ok.document(), &[SourceBlock::stub("prod", SourceKind::ClickHouse)]);
        assert!(errors.is_empty() && warnings.is_empty());
        let bad = doc_with_source("unknown");
        let (errors, _) = sql_save_checks(bad.document(), &[SourceBlock::stub("prod", SourceKind::ClickHouse)]);
        assert!(!errors.is_empty());
        for e in &errors {
            assert_eq!(e.code, ErrorCode::BadReference);
            assert!(e.pointer.ends_with("/source"), "{}", e.pointer);
            assert!(e.line.is_none() && e.column.is_none());
        }
    }

    #[test]
    fn warning_6_display_text_of_an_unknown_variable() {
        let text =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/dashboards/nodes.json"))
                .unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
        json["sections"][0]["panels"][0]["title"] = serde_json::json!("Latency of $nope");
        let doc =
            hune_core::document::validate(&serde_json::to_vec(&json).unwrap(), Some(&Uid::parse("nodes").unwrap()))
                .expect("valid");
        let (errors, warnings) = sql_save_checks(doc.document(), &[SourceBlock::stub("prod", SourceKind::ClickHouse)]);
        assert!(errors.is_empty());
        let last = warnings.last().expect("a warning");
        assert_eq!(last.pointer, "/sections/0/panels/0/title");
        assert_eq!(last.message, "$nope is not a variable of the document");
    }
}
