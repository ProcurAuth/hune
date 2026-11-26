// SPDX-License-Identifier: MIT OR Apache-2.0
//! The dashboard routes: list, read, create, save and delete (6.12 to 6.16).

use super::gate;
use super::paging::{DashboardKey, PageCursor, parse_limit};
use super::support::{
    SaveWarning, StoreOp, declared_sources, folder_json, folder_sort_key, format_faults, log_dashboard_changed,
    map_store_error, read_folder_member, read_message, services, validate_document, write_meta,
};
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::headers::etag_header_value;
use crate::server::json::{Member, Shape, parse_and_check_with};
use crate::server::response::{Response, bytes_response, empty_response, json_response};
use bytes::Bytes;
use http::StatusCode;
use http::header::{CONTENT_DISPOSITION, ETAG, HeaderValue, LOCATION};
use hune_core::document::CanonicalDocument;
use hune_core::error::ValidationError;
use hune_core::ids::Uid;
use hune_store::{CatalogEntry, Etag, FolderId, Outcome, Saved, Store, VersionKind, WriteMeta};
use serde::Serialize;
use serde_json::Value;

static FOLDER_SHAPE: Shape = Shape::Str { min_chars: 1, max_chars: 40 };
static MEMBERS: [Member; 3] = [
    Member { name: "folder", shape: Shape::Nullable(&FOLDER_SHAPE), required: true },
    Member { name: "message", shape: Shape::Str { min_chars: 0, max_chars: 500 }, required: false },
    Member { name: "document", shape: Shape::Any, required: true },
];
static CREATE_SHAPE: Shape = Shape::Object(&MEMBERS);
static SAVE_MEMBERS: [Member; 2] = [
    Member { name: "message", shape: Shape::Str { min_chars: 0, max_chars: 500 }, required: false },
    Member { name: "document", shape: Shape::Any, required: true },
];
static SAVE_SHAPE: Shape = Shape::Object(&SAVE_MEMBERS);

#[derive(Serialize)]
struct Access {
    write: bool,
    history: bool,
    export: bool,
}

impl From<gate::Flags> for Access {
    fn from(f: gate::Flags) -> Access {
        Access { write: f.write, history: f.history, export: f.export }
    }
}

#[derive(Serialize)]
struct ListItem {
    uid: String,
    folder: Option<String>,
    title: String,
    tags: Vec<String>,
    etag: String,
    version: u64,
    updated_at: Option<String>,
    updated_by: String,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<ValidationError>>,
    access: Access,
    /// True when the store of the dashboard is read-only (5.42). Then `access.write` and `access.history` are false.
    read_only: bool,
    /// The id of the store of the dashboard.
    store: String,
    /// `file_invalid` when the dashboard shows its last valid state because its file is not valid now (5.44).
    #[serde(skip_serializing_if = "Option::is_none")]
    stale: Option<&'static str>,
}

/// The code of the warning of a dashboard that shows its last valid state (5.44).
const STALE_CODE: &str = "file_invalid";

/// The store of a dashboard for the page (5.42): its id, and true when it is read-only.
fn store_facts(services: &crate::server::state::Services, uid: &Uid) -> (String, bool) {
    match services.store.store_of_uid(uid) {
        Some(info) => (info.id.clone(), !info.primary),
        None => (services.store.primary_id().unwrap_or_default().to_owned(), false),
    }
}

/// The refusal of a write on a dashboard of a read-only store, or `None` (5.42, 11.35).
async fn read_only_dashboard(
    ctx: &Ctx,
    services: &crate::server::state::Services,
    entry: &CatalogEntry,
) -> Option<ApiError> {
    if !services.store.uid_is_read_only(entry.uid()) {
        return None;
    }
    Some(gate::read_only_refusal(ctx, entry.folder().as_str(), Some(entry.uid().as_str())).await)
}

/// The refusal of a write into a folder of a read-only store, or `None` (5.42, 11.35).
pub(super) async fn read_only_folder(
    ctx: &Ctx,
    services: &crate::server::state::Services,
    folder: &FolderId,
) -> Option<ApiError> {
    if !services.store.folder_is_read_only(folder) {
        return None;
    }
    Some(gate::read_only_refusal(ctx, folder.as_str(), None).await)
}

#[derive(Serialize)]
struct DashboardList {
    items: Vec<ListItem>,
    next: Option<String>,
}

/// The folded text for the search `q`: ASCII letters in lower case (6.12).
fn fold(text: &str) -> String {
    text.to_ascii_lowercase()
}

struct Row {
    key: DashboardKey,
    entry: std::sync::Arc<CatalogEntry>,
}

/// `GET /api/dashboards`: the list with the pagination of 6.24. Each valid and each invalid entry shows.
pub async fn list(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let limit = parse_limit(ctx.query.get("limit"), 500)? as usize;
    let after = ctx.query.get("after").map(PageCursor::parse).transpose()?.unwrap_or_default();
    let q = match ctx.query.get("q") {
        None => None,
        Some(q) if (1..=100).contains(&q.len()) => Some(fold(q)),
        Some(_) => return Err(ApiError::bad_request("The query parameter q must have 1 to 100 bytes.")),
    };
    // The tag of 6.12: one exact tag, 1 to 64 bytes. The repeat of the parameter gives 400 in `Query::parse` (6.2).
    let tag = match ctx.query.get("tag") {
        None => None,
        Some(t) if (1..=64).contains(&t.len()) => Some(t),
        Some(_) => return Err(ApiError::bad_request("The query parameter tag must have 1 to 64 bytes.")),
    };
    let folder_filter = ctx.query.get("folder");
    let services = services(&ctx)?;
    let mut rows: Vec<Row> = services
        .catalog
        .entries()
        .into_iter()
        .filter(|e| folder_filter.is_none_or(|f| folder_sort_key(e.folder()) == f))
        .map(|entry| {
            let title = match &*entry {
                CatalogEntry::Valid { doc, .. } => doc.title().to_owned(),
                CatalogEntry::Invalid { uid, .. } => uid.as_str().to_owned(),
            };
            let key = DashboardKey::new(folder_sort_key(entry.folder()), &title, entry.uid().as_str());
            Row { key, entry }
        })
        .filter(|row| match &q {
            None => true,
            Some(q) => {
                let title = match &*row.entry {
                    CatalogEntry::Valid { doc, .. } => fold(doc.title()),
                    CatalogEntry::Invalid { uid, .. } => fold(uid.as_str()),
                };
                title.contains(q.as_str()) || fold(row.entry.uid().as_str()).contains(q.as_str())
            }
        })
        .filter(|row| match (tag, &*row.entry) {
            (None, _) => true,
            // The comparison is exact. An invalid entry has no tag and never matches.
            (Some(t), CatalogEntry::Valid { doc, .. }) => doc.document().tags.iter().any(|x| x == t),
            (Some(_), CatalogEntry::Invalid { .. }) => false,
        })
        .filter(|row| after.key.as_ref().is_none_or(|a| row.key > *a))
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    // The rows that the previous page examined after its last visible item. The cursor counts them and holds
    // no key of them (6.24).
    rows.drain(..after.skip.min(rows.len()));
    // The display filter of 11.25 (mode `authzen`). In mode `open`, each row shows.
    let keys: Vec<(String, String)> =
        rows.iter().map(|r| (r.entry.folder().as_str().to_owned(), r.entry.uid().as_str().to_owned())).collect();
    let in_read_only_store = |folder: &str| services.store.folder_text_is_read_only(folder);
    let (rows, next, writes) = match gate::filter_dashboards(&ctx, &keys, limit, &in_read_only_store).await? {
        None => {
            let more = rows.len() > limit;
            rows.truncate(limit);
            let next = if more {
                rows.last().map(|r| PageCursor { key: Some(r.key.clone()), skip: 0 }.encode())
            } else {
                None
            };
            let server = &ctx.state.config.server;
            let open = gate::Flags { write: true, history: server.history, export: server.export };
            let writes = vec![open; rows.len()];
            (rows, next, writes)
        }
        Some(filtered) => {
            // The cursor holds the key of the last row that the subject sees, and the count of the hidden rows
            // after it. With no visible row on the page, it holds the key of the request and a longer count.
            let next = if filtered.examined < rows.len() && filtered.examined > 0 {
                let cursor = match filtered.picked.last() {
                    Some(&(last, _)) => {
                        PageCursor { key: rows.get(last).map(|r| r.key.clone()), skip: filtered.examined - (last + 1) }
                    }
                    None => PageCursor { key: after.key.clone(), skip: after.skip + filtered.examined },
                };
                Some(cursor.encode())
            } else {
                None
            };
            let mut slots: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
            let mut picked = Vec::new();
            let mut writes = Vec::new();
            for (i, w) in filtered.picked {
                if let Some(row) = slots.get_mut(i).and_then(Option::take) {
                    picked.push(row);
                    writes.push(w);
                }
            }
            (picked, next, writes)
        }
    };
    let mut items = Vec::with_capacity(rows.len());
    for (row, flags) in rows.iter().zip(writes.iter().copied()) {
        let (store, read_only) = store_facts(services, row.entry.uid());
        let flags = if read_only { flags.of_read_only_store() } else { flags };
        let stale = services.store.is_stale(row.entry.uid()).then_some(STALE_CODE);
        items.push(match &*row.entry {
            CatalogEntry::Valid { current, doc } => ListItem {
                uid: current.uid.as_str().to_owned(),
                folder: current.folder.wire().map(str::to_owned),
                title: doc.title().to_owned(),
                tags: doc.document().tags.clone(),
                etag: current.etag.to_hex(),
                version: current.version,
                updated_at: Some(current.saved_at.to_rfc3339()),
                updated_by: current.saved_by.as_str().to_owned(),
                state: "valid",
                errors: None,
                access: flags.into(),
                read_only,
                store,
                stale,
            },
            CatalogEntry::Invalid { uid, folder, etag, errors } => {
                // The head version of the history gives the version and the author, when it exists.
                let head = services.store.history(uid, None, 1).await.ok().and_then(|v| v.into_iter().next());
                ListItem {
                    uid: uid.as_str().to_owned(),
                    folder: folder.wire().map(str::to_owned),
                    title: uid.as_str().to_owned(),
                    tags: Vec::new(),
                    etag: etag.to_hex(),
                    version: head.as_ref().map_or(0, |h| h.version),
                    updated_at: head.as_ref().map(|h| h.saved_at.to_rfc3339()),
                    updated_by: head.as_ref().map_or_else(|| "external".to_owned(), |h| h.saved_by.as_str().to_owned()),
                    state: "invalid",
                    errors: Some(errors.iter().take(20).cloned().collect()),
                    access: flags.into(),
                    read_only,
                    store,
                    stale: None,
                }
            }
        });
    }
    Ok(json_response(StatusCode::OK, &DashboardList { items, next }))
}

/// Cuts the final line end of canonical bytes, so that they embed in a JSON answer.
fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[..end]
}

/// Reads the uid of the path. A text that is not a uid names no dashboard: `404 not_found`.
pub(super) fn path_uid(ctx: &Ctx) -> Result<Uid, ApiError> {
    Uid::parse(ctx.params.get("uid").unwrap_or("")).map_err(|_| ApiError::not_found("No such dashboard."))
}

/// The answer of `download=true`: the bytes, the `ETag` and the attachment name (6.14).
fn download_response(uid: &Uid, bytes: Bytes, etag: &Etag) -> Response {
    let mut r = bytes_response(StatusCode::OK, "application/json", bytes);
    r.headers_mut().insert(ETAG, etag_header_value(etag));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}.json\"", uid.as_str())) {
        r.headers_mut().insert(CONTENT_DISPOSITION, v);
    }
    r
}

/// `GET /api/dashboards/{uid}` (6.14).
pub async fn get(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let download = ctx.query.flag("download")?;
    let services = services(&ctx)?;
    let Some(entry) = services.catalog.get(&uid) else {
        return Err(gate::unknown(&ctx, "No such dashboard.").await);
    };
    if download {
        gate::dashboard_export(&ctx, entry.folder().as_str(), uid.as_str()).await?;
    } else {
        gate::dashboard_read(&ctx, entry.folder().as_str(), uid.as_str()).await?;
    }
    let CatalogEntry::Valid { current, doc } = &*entry else {
        let CatalogEntry::Invalid { errors, folder, .. } = &*entry else { return Err(ApiError::internal()) };
        if !download {
            return Err(ApiError::stored_document_invalid(errors));
        }
        // The download of an invalid entry gives the stored bytes as they are (6.14, 4.39).
        let op = StoreOp::begin(&ctx, "raw_current");
        let read = services.store.raw_current(&uid, folder).await;
        op.end(&ctx.state, "raw_current", &read);
        let (bytes, etag) = read.map_err(|e| map_store_error(&e))?;
        return Ok(download_response(&uid, Bytes::from(bytes), &etag));
    };
    if !download && ctx.if_none_match.as_ref() == Some(&current.etag) {
        let mut r = empty_response(StatusCode::NOT_MODIFIED);
        r.headers_mut().insert(ETAG, etag_header_value(&current.etag));
        return Ok(r);
    }
    if download {
        return Ok(download_response(&uid, Bytes::copy_from_slice(doc.bytes()), &current.etag));
    }
    let (store, read_only) = store_facts(services, &uid);
    let flags = gate::flags(&ctx, current.folder.as_str(), uid.as_str(), read_only).await?;
    let stale = if services.store.is_stale(&uid) { format!(",\"stale\":\"{STALE_CODE}\"") } else { String::new() };
    let q = |s: &str| Value::String(s.to_owned()).to_string();
    let head = format!(
        "{{\"uid\":{},\"folder\":{},\"etag\":{},\"version\":{},\"updated_at\":{},\"updated_by\":{},\"access\":{{\"write\":{},\"history\":{},\"export\":{}}},\"read_only\":{},\"store\":{}{},\"document\":",
        q(current.uid.as_str()),
        folder_json(&current.folder),
        q(&current.etag.to_hex()),
        current.version,
        q(&current.saved_at.to_rfc3339()),
        q(current.saved_by.as_str()),
        flags.write,
        flags.history,
        flags.export,
        read_only,
        q(&store),
        stale
    );
    // The names that each saved query names (6.14, 7.19): a pure function of the document, with no query.
    let names = doc.names_json();
    let mut body = Vec::with_capacity(head.len() + doc.bytes().len() + names.len() + 10);
    body.extend_from_slice(head.as_bytes());
    body.extend_from_slice(trim_end(doc.bytes()));
    body.extend_from_slice(b",\"names\":");
    body.extend_from_slice(names.as_bytes());
    // The permitted keys of each `adhoc` variable (6.14, 4.10c): the keys of the document that the configured list of
    // its source holds now. The member is present only when the document has an `adhoc` variable.
    if let Some(adhoc) = adhoc_member(&ctx.state, doc.document()) {
        body.extend_from_slice(b",\"adhoc\":");
        body.extend_from_slice(adhoc.as_bytes());
    }
    body.push(b'}');
    let mut r = bytes_response(StatusCode::OK, "application/json", Bytes::from(body));
    r.headers_mut().insert(ETAG, etag_header_value(&current.etag));
    Ok(r)
}

/// The member `adhoc` of the answer of 6.14: an object with one member for each `adhoc` variable, the array of its
/// permitted keys in the order of the document. `None` when the document has no `adhoc` variable.
fn adhoc_member(state: &crate::server::state::AppState, doc: &hune_core::document::Document) -> Option<String> {
    let mut members = serde_json::Map::new();
    for v in doc.variables() {
        if let hune_core::document::VariableKind::Adhoc { source, keys } = &v.kind {
            let configured =
                crate::server::source_blocks::find(&state.config, source.as_str()).and_then(|b| b.adhoc_columns);
            let permitted = hune_core::sql::adhoc::permitted_keys(keys, configured.unwrap_or(&[]));
            members.insert(v.name.clone(), Value::from(permitted));
        }
    }
    (!members.is_empty()).then(|| Value::Object(members).to_string())
}

#[derive(Serialize)]
struct WriteAnswer<'a> {
    uid: &'a str,
    folder: Option<&'a str>,
    etag: String,
    version: u64,
    saved_at: String,
    saved_by: String,
    noop: bool,
    /// Absent in the answer of a move (6.19). The answers of a create, a save and a restore hold it.
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<&'a [SaveWarning]>,
}

/// Gives the time and the author of the version that an answer names (6.13).
pub(super) async fn stamp(
    services: &crate::server::state::Services,
    saved: &Saved,
    meta: &WriteMeta,
) -> Result<(String, String), ApiError> {
    let own = || (meta.saved_at.to_rfc3339(), meta.saved_by.as_str().to_owned());
    let stamped = match saved.outcome {
        Outcome::Written => own(),
        Outcome::Replayed => match services.store.version(&saved.uid, saved.version).await {
            Ok(v) => (v.meta.saved_at.to_rfc3339(), v.meta.saved_by.as_str().to_owned()),
            Err(e) => return Err(map_store_error(&e)),
        },
        Outcome::Unchanged => match services.store.get(&saved.uid).await {
            Ok(c) => (c.saved_at.to_rfc3339(), c.saved_by.as_str().to_owned()),
            Err(_) => own(),
        },
    };
    Ok(stamped)
}

/// Refreshes the catalog entry of the uid after an own write (5.8, item 3) and writes the events.
pub(super) async fn refresh(ctx: &Ctx, services: &crate::server::state::Services, uid: &Uid) {
    services.catalog.refresh_uid(&services.store, uid).await;
    crate::server::startup::log_store_events(&ctx.state, services.catalog.take_events());
    crate::server::startup::update_document_metrics(&ctx.state, &services.catalog);
}

/// Makes the answer of a create, a save, a restore or a move: `201` for a create, else `200`.
/// A move gives no `warnings` (6.19): its caller passes `None`.
pub(super) async fn write_answer(
    services: &crate::server::state::Services,
    saved: &Saved,
    meta: &WriteMeta,
    warnings: Option<&[SaveWarning]>,
    created: bool,
    doc_etag: &Etag,
) -> Result<Response, ApiError> {
    let (saved_at, saved_by) = stamp(services, saved, meta).await?;
    let etag = saved.etag.unwrap_or(*doc_etag);
    let body = WriteAnswer {
        uid: saved.uid.as_str(),
        folder: saved.folder.wire(),
        etag: etag.to_hex(),
        version: saved.version,
        saved_at,
        saved_by,
        noop: saved.outcome == Outcome::Unchanged,
        warnings,
    };
    let status = if created { StatusCode::CREATED } else { StatusCode::OK };
    let mut r = json_response(status, &body);
    let h = r.headers_mut();
    h.insert(ETAG, etag_header_value(&etag));
    if created && let Ok(v) = HeaderValue::from_str(&format!("api/dashboards/{}", saved.uid.as_str())) {
        h.insert(LOCATION, v);
    }
    if saved.outcome == Outcome::Replayed {
        h.insert(crate::server::headers::IDEMPOTENT_REPLAYED, HeaderValue::from_static("true"));
    }
    Ok(r)
}

/// Validates the member `document` of a body: 4.36 from its own bytes, then 7.18. A fault gives
/// `422 invalid_document` with the pointers of the document. The work runs in `spawn_blocking` with the place of the
/// body and the permits of 6.27 and 12.16, and it keeps them until it ends.
async fn check_document(
    ctx: &Ctx,
    req: &Request,
    bytes: &[u8],
    expected_uid: Option<&Uid>,
) -> Result<(CanonicalDocument, Vec<SaveWarning>), ApiError> {
    validate_document(ctx, req.place.take(), bytes.to_vec(), expected_uid.cloned(), true)
        .await?
        .map_err(|errors| ApiError::invalid_document(&errors))
}

/// The sources of the decision of a create or of a save (11.21): the pre-pass reads them from the bytes of the
/// document, before the validation. `None`: the pre-pass cannot read the document, so Hune asks the use alone. The
/// mode `open` asks nothing and reads nothing.
fn asked_sources(ctx: &Ctx, document: &[u8]) -> Option<Vec<String>> {
    if ctx.state.access.is_none() {
        return Some(Vec::new());
    }
    let blocks = declared_sources(&ctx.state);
    let ids: Vec<&str> = blocks.iter().map(|b| b.id).collect();
    super::prepass::declared_sources(document, &ids)
}

/// The check after the validation (11.21): each source of the valid document must be in the decision. A document that
/// the pre-pass could not read must not pass the validation. Else the answer is `500`, with the log line
/// `access.prepass_mismatch`, and Hune writes nothing: a fault of the pre-pass never skips a question.
fn after_check(ctx: &Ctx, asked: Option<&[String]>, doc: &CanonicalDocument) -> Result<(), ApiError> {
    if ctx.state.access.is_none() {
        return Ok(());
    }
    let named = gate::document_sources(doc.document());
    let missing: Vec<&str> = match asked {
        Some(list) => named.iter().filter(|s| !list.contains(s)).map(String::as_str).collect(),
        None => named.iter().map(String::as_str).collect(),
    };
    if asked.is_some() && missing.is_empty() {
        return Ok(());
    }
    ctx.state
        .logger
        .event(crate::config::LogLevel::Error, "access.prepass_mismatch")
        .request_id(&ctx.request_id)
        .field("route", ctx.route.label)
        .field("uid", doc.uid().as_str())
        .field("readable", asked.is_some())
        .field("missing", Value::from(missing))
        .emit();
    Err(ApiError::internal())
}

#[derive(Serialize)]
struct ValidateAnswer<'a> {
    valid: bool,
    etag: String,
    canonical: String,
    warnings: &'a [SaveWarning],
}

fn validate_answer(doc: &CanonicalDocument, warnings: &[SaveWarning]) -> Response {
    let answer = ValidateAnswer {
        valid: true,
        etag: doc.etag().to_hex(),
        canonical: String::from_utf8_lossy(doc.bytes()).into_owned(),
        warnings,
    };
    json_response(StatusCode::OK, &answer)
}

/// `POST /api/dashboards` (6.13).
///
/// In the mode `authzen`, the decision comes before the validation (11.20): the folder (11.23), the pre-pass of the
/// sources, the questions of 11.21, then the validation and the check after it.
pub async fn create(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let body =
        parse_and_check_with(&req.body, &CREATE_SHAPE, &["document"], |v| format_faults(v, &["folder"], false, true))?;
    let folder = read_folder_member(&body.value["folder"], "/folder")?;
    let message = read_message(&body.value)?;
    let validate_only = ctx.query.flag("validate_only")?;
    let bytes = body.member_bytes(&req.body, "document").ok_or_else(ApiError::internal)?;
    let mut asked: Option<Vec<String>> = Some(Vec::new());
    if ctx.state.access.is_some() {
        // Mode `authzen` (11.21, 11.23): the pre-pass, then the folder must exist, then the questions. The pre-pass
        // comes before the lookup, so a folder that exists and one that does not get the same work (11.23).
        if !validate_only {
            asked = asked_sources(&ctx, bytes);
        }
        let services = services(&ctx)?;
        let sources = asked.as_deref().unwrap_or(&[]);
        if !folder.is_root() && !services.catalog.folders().iter().any(|f| f.id == folder) {
            // The same others as for a folder that exists, then the question of 11.23 (11.23, rule 1).
            return Err(gate::unknown_with(&ctx, "No such folder.", gate::Others::Sources(sources)).await);
        }
        if let Some(refusal) = read_only_folder(&ctx, services, &folder).await {
            return Err(refusal);
        }
        gate::folder_write_with_sources(&ctx, folder.as_str(), sources).await?;
    } else if let Some(refusal) = read_only_folder(&ctx, services(&ctx)?, &folder).await {
        // The mode `open`: a draft or a create into a read-only store gets the same refusal (5.42).
        return Err(refusal);
    }
    let (doc, warnings) = check_document(&ctx, &req, bytes, None).await?;
    if validate_only {
        return Ok(validate_answer(&doc, &warnings));
    }
    after_check(&ctx, asked.as_deref(), &doc)?;
    let services = services(&ctx)?;
    if ctx.state.access.is_some() {
        let held = match services.catalog.get(doc.uid()) {
            Some(entry) => Some(entry.folder().clone()),
            None => services
                .store
                .history(doc.uid(), None, 1)
                .await
                .ok()
                .and_then(|versions| versions.into_iter().next())
                .map(|v| v.folder),
        };
        if let Some(held_folder) = held {
            return Err(gate::already_exists(&ctx, held_folder.as_str(), doc.uid().as_str()).await?);
        }
    }
    let meta = write_meta(&ctx, message);
    let op = StoreOp::begin(&ctx, "create");
    let result = services.store.create(&folder, &doc, VersionKind::Create, &meta).await;
    op.end(&ctx.state, "create", &result);
    let saved = result.map_err(|e| map_store_error(&e))?;
    refresh(&ctx, services, doc.uid()).await;
    if saved.outcome == Outcome::Written {
        log_dashboard_changed(
            &ctx,
            "create",
            saved.uid.as_str(),
            saved.folder.as_str(),
            None,
            saved.version,
            None,
            saved.etag.as_ref().map(Etag::to_hex).as_deref(),
            Some(doc.bytes().len() as u64),
        );
    }
    write_answer(services, &saved, &meta, Some(&warnings), true, doc.etag()).await
}

/// `PUT /api/dashboards/{uid}` (6.15).
///
/// In the mode `authzen`, the catalog lookup and the decision come before the validation (11.20). In the mode `open`,
/// the validation comes first, as before: a path that is not a uid gives the same answer as an unknown uid, after
/// the checks of the body.
pub async fn save(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx);
    let body = parse_and_check_with(&req.body, &SAVE_SHAPE, &["document"], |v| format_faults(v, &[], false, true))?;
    let message = read_message(&body.value)?;
    let validate_only = ctx.query.flag("validate_only")?;
    let bytes = body.member_bytes(&req.body, "document").ok_or_else(ApiError::internal)?;
    let mut asked: Option<Vec<String>> = Some(Vec::new());
    let mut decided: Option<std::sync::Arc<CatalogEntry>> = None;
    if ctx.state.access.is_some() {
        let uid = uid.clone()?;
        // The pre-pass comes before the lookup, so a uid that exists and one that does not get the same work (11.23).
        // `validate_only` asks the use question only (11.21) and needs no pre-pass.
        if !validate_only {
            asked = asked_sources(&ctx, bytes);
        }
        let services = services(&ctx)?;
        let sources = asked.as_deref().unwrap_or(&[]);
        let Some(entry) = services.catalog.get(&uid) else {
            // The same others as for a uid that exists, then the question of 11.23 (11.23, rule 1).
            return Err(gate::unknown_with(&ctx, "No such dashboard.", gate::Others::Sources(sources)).await);
        };
        if let Some(refusal) = read_only_dashboard(&ctx, services, &entry).await {
            return Err(refusal);
        }
        gate::dashboard_write(&ctx, entry.folder().as_str(), uid.as_str(), sources).await?;
        decided = Some(entry);
    } else if let Ok(uid) = &uid {
        // The mode `open`: a save or a validation of a dashboard of a read-only store gets the refusal (5.42).
        let services = services(&ctx)?;
        if let Some(entry) = services.catalog.get(uid)
            && let Some(refusal) = read_only_dashboard(&ctx, services, &entry).await
        {
            return Err(refusal);
        }
    }
    // The member uid of the document must equal the uid of the path: a fault gives `/uid` (6.15).
    let expected = uid.as_ref().ok();
    let (doc, warnings) = check_document(&ctx, &req, bytes, expected).await?;
    if validate_only {
        return Ok(validate_answer(&doc, &warnings));
    }
    after_check(&ctx, asked.as_deref(), &doc)?;
    let uid = uid?;
    let services = services(&ctx)?;
    // The folder of the decision is the folder that the store compares (6.15, step 5).
    let entry = match decided {
        Some(entry) => entry,
        None => match services.catalog.get(&uid) {
            Some(entry) => entry,
            None => return Err(gate::unknown(&ctx, "No such dashboard.").await),
        },
    };
    let if_match = ctx.if_match.ok_or_else(ApiError::internal)?;
    let folder = entry.folder().clone();
    let etag_before = entry.etag().to_hex();
    let meta = write_meta(&ctx, message);
    let op = StoreOp::begin(&ctx, "save");
    let result = services.store.save(&doc, &if_match, &folder, VersionKind::Save, &meta).await;
    op.end(&ctx.state, "save", &result);
    let saved = match result {
        Ok(s) => s,
        Err(e) => {
            if matches!(e, hune_store::StoreError::Conflict(_)) {
                // The catalog may be behind (7.15): read the uid again, so that the next save can pass.
                refresh(&ctx, services, &uid).await;
            }
            return Err(map_store_error(&e));
        }
    };
    refresh(&ctx, services, &uid).await;
    if saved.outcome == Outcome::Written {
        log_dashboard_changed(
            &ctx,
            "save",
            saved.uid.as_str(),
            saved.folder.as_str(),
            None,
            saved.version,
            Some(&etag_before),
            saved.etag.as_ref().map(Etag::to_hex).as_deref(),
            Some(doc.bytes().len() as u64),
        );
    }
    write_answer(services, &saved, &meta, Some(&warnings), false, doc.etag()).await
}

#[derive(Serialize)]
struct DeleteAnswer<'a> {
    uid: &'a str,
    version: u64,
    deleted: bool,
    saved_at: String,
    saved_by: String,
}

/// `DELETE /api/dashboards/{uid}` (6.16).
pub async fn delete(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let services = services(&ctx)?;
    let Some(entry) = services.catalog.get(&uid) else {
        return Err(gate::unknown(&ctx, "No such dashboard.").await);
    };
    if let Some(refusal) = read_only_dashboard(&ctx, services, &entry).await {
        return Err(refusal);
    }
    gate::dashboard_write_use(&ctx, entry.folder().as_str(), uid.as_str()).await?;
    let if_match = ctx.if_match.ok_or_else(ApiError::internal)?;
    let folder: FolderId = entry.folder().clone();
    let etag_before = entry.etag().to_hex();
    let meta = write_meta(&ctx, hune_store::Message::default());
    let op = StoreOp::begin(&ctx, "delete");
    let result = services.store.delete(&uid, &if_match, &folder, &meta).await;
    op.end(&ctx.state, "delete", &result);
    let saved = match result {
        Ok(s) => s,
        Err(e) => {
            if matches!(e, hune_store::StoreError::Conflict(_)) {
                refresh(&ctx, services, &uid).await;
            }
            return Err(map_store_error(&e));
        }
    };
    refresh(&ctx, services, &uid).await;
    if saved.outcome == Outcome::Written {
        log_dashboard_changed(
            &ctx,
            "delete",
            uid.as_str(),
            folder.as_str(),
            None,
            saved.version,
            Some(&etag_before),
            None,
            None,
        );
    }
    let (saved_at, saved_by) = stamp(services, &saved, &meta).await?;
    let body = DeleteAnswer { uid: uid.as_str(), version: saved.version, deleted: true, saved_at, saved_by };
    let mut r = json_response(StatusCode::OK, &body);
    if saved.outcome == Outcome::Replayed {
        r.headers_mut().insert(crate::server::headers::IDEMPOTENT_REPLAYED, HeaderValue::from_static("true"));
    }
    Ok(r)
}
