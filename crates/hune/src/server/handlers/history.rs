// SPDX-License-Identifier: MIT OR Apache-2.0
//! The history, restore and move routes (6.17 to 6.19).

use super::dashboards::{path_uid, refresh, write_answer};
use super::gate;
use super::paging::{history_cursor, parse_history_cursor, parse_limit};
use super::support::{
    SaveWarning, StoreOp, check_permit, declared_sources, format_faults, log_dashboard_changed, map_store_error,
    read_folder_id, read_message, run_check, services, sql_save_checks, write_meta,
};
use crate::server::ctx::{Ctx, Request};
use crate::server::error::{ApiError, Code};
use crate::server::headers::etag_header_value;
use crate::server::json::{Member, Shape, parse_and_check_with};
use crate::server::response::{Response, bytes_response, json_response};
use bytes::Bytes;
use http::StatusCode;
use http::header::{CONTENT_DISPOSITION, ETAG, HeaderValue};
use hune_core::document::validate;
use hune_store::{Etag, MAX_HISTORY_PAGE, Outcome, Store, ValidVersion, VersionKind, VersionMeta};
use serde::Serialize;

#[derive(Serialize)]
struct HistoryItem {
    version: u64,
    kind: &'static str,
    etag: Option<String>,
    saved_at: String,
    saved_by: String,
    message: String,
    folder: String,
    size: Option<u64>,
}

fn item(m: &VersionMeta) -> HistoryItem {
    HistoryItem {
        version: m.version,
        kind: m.kind.as_str(),
        etag: m.etag.as_ref().map(Etag::to_hex),
        saved_at: m.saved_at.to_rfc3339(),
        saved_by: m.saved_by.as_str().to_owned(),
        message: m.message.as_str().to_owned(),
        folder: m.folder.as_str().to_owned(),
        size: m.size,
    }
}

#[derive(Serialize)]
struct HistoryPage {
    items: Vec<HistoryItem>,
    next: Option<String>,
}

/// `GET /api/dashboards/{uid}/history`: the versions, the newest first (6.17). It reads the store.
pub async fn history(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let limit = parse_limit(ctx.query.get("limit"), MAX_HISTORY_PAGE)?;
    let before = ctx.query.get("after").map(parse_history_cursor).transpose()?;
    let services = services(&ctx)?;
    read_gate(&ctx, services, &uid).await?;
    let op = StoreOp::begin(&ctx, "history");
    let result = services.store.history(&uid, before, limit).await;
    op.end(&ctx.state, "history", &result);
    let versions = result.map_err(|e| map_store_error(&e))?;
    let mut next = None;
    if versions.len() == limit as usize
        && let Some(last) = versions.last()
    {
        // The page is full: a next page exists when one more version is older than the last one.
        let more = services.store.history(&uid, Some(last.version), 1).await.map_err(|e| map_store_error(&e))?;
        if !more.is_empty() {
            next = Some(history_cursor(last.version));
        }
    }
    Ok(json_response(StatusCode::OK, &HistoryPage { items: versions.iter().map(item).collect(), next }))
}

/// The question of a read route on the history (11.21). A uid that the catalog does not hold follows
/// the rule of 11.23. In the mode `open`, it asks nothing and the store answers as before.
async fn read_gate(
    ctx: &Ctx,
    services: &crate::server::state::Services,
    uid: &hune_core::ids::Uid,
) -> Result<(), ApiError> {
    if ctx.state.access.is_none() {
        return Ok(());
    }
    let Some(entry) = services.catalog.get(uid) else {
        return Err(gate::unknown(ctx, "No such dashboard.").await);
    };
    gate::dashboard_read(ctx, entry.folder().as_str(), uid.as_str()).await.map(|_| ())
}

fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[..end]
}

/// `GET /api/dashboards/{uid}/history/{n}`: one version (6.17).
pub async fn version(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let n_text = ctx.params.get("n").unwrap_or("");
    let n = match n_text.parse::<u64>() {
        Ok(n) if n >= 1 && n_text.bytes().all(|b| b.is_ascii_digit()) && !n_text.starts_with('0') => n,
        _ => return Err(ApiError::bad_request("The version must be a whole number from 1.")),
    };
    let download = ctx.query.flag("download")?;
    let services = services(&ctx)?;
    read_gate(&ctx, services, &uid).await?;
    // The plain answer validates the version (6.17). The permit comes before the store read, so that a request that
    // waits holds no bytes of a version (6.27).
    let permit = if download { None } else { Some(check_permit(&ctx.state, &ctx.subject).await?) };
    let op = StoreOp::begin(&ctx, "version");
    let result = services.store.version(&uid, n).await;
    op.end(&ctx.state, "version", &result);
    let v = result.map_err(|e| map_store_error(&e))?;
    if download {
        let Some(bytes) = v.bytes else {
            return Err(ApiError::not_found("The store holds no bytes for this version."));
        };
        let mut r = bytes_response(StatusCode::OK, "application/json", Bytes::from(bytes));
        if let Some(etag) = &v.meta.etag {
            r.headers_mut().insert(ETAG, etag_header_value(etag));
        }
        if let Ok(h) = HeaderValue::from_str(&format!("attachment; filename=\"{}-v{n}.json\"", uid.as_str())) {
            r.headers_mut().insert(CONTENT_DISPOSITION, h);
        }
        return Ok(r);
    }
    let q = |s: &str| serde_json::Value::String(s.to_owned()).to_string();
    let m = &v.meta;
    let document: Vec<u8> = match (v.bytes, permit) {
        (None, _) => b"null".to_vec(),
        (Some(bytes), Some(permit)) => {
            // The current rules of section 4 must read the version (6.17): the validation runs in `spawn_blocking`,
            // never on an async worker (6.27). The etag of the bytes does not replace it, because a version of
            // format 2 has a matching etag and gives 422.
            let (bytes, read) = run_check(&ctx.state, permit, None, move || {
                let read = validate(&bytes, None).map(|_| ());
                (bytes, read)
            })
            .await?;
            if let Err(errors) = read {
                return Err(ApiError::stored_document_invalid(&errors));
            }
            let mut bytes = bytes;
            bytes.truncate(trim_end(&bytes).len());
            bytes
        }
        (Some(_), None) => return Err(ApiError::internal()),
    };
    let head = format!(
        "{{\"version\":{},\"kind\":{},\"etag\":{},\"saved_at\":{},\"saved_by\":{},\"message\":{},\"folder\":{},\"document\":",
        m.version,
        q(m.kind.as_str()),
        m.etag.as_ref().map_or_else(|| "null".to_owned(), |e| q(&e.to_hex())),
        q(&m.saved_at.to_rfc3339()),
        q(m.saved_by.as_str()),
        q(m.message.as_str()),
        q(m.folder.as_str()),
    );
    let mut body = head.into_bytes();
    body.extend_from_slice(&document);
    body.push(b'}');
    let mut r = bytes_response(StatusCode::OK, "application/json", Bytes::from(body));
    if let Some(etag) = &m.etag {
        r.headers_mut().insert(ETAG, etag_header_value(etag));
    }
    Ok(r)
}

static RESTORE_MEMBERS: [Member; 2] = [
    Member { name: "version", shape: Shape::Int { min: 1, max: i64::MAX }, required: true },
    Member { name: "message", shape: Shape::Str { min_chars: 0, max_chars: 500 }, required: false },
];
static RESTORE_SHAPE: Shape = Shape::Object(&RESTORE_MEMBERS);

/// What the validation of a version gives for a restore (6.18).
enum Restorable {
    /// The version passes 4.36 and 7.18: the version with its document, its warnings and its sources (11.21).
    Yes { version: Box<ValidVersion>, warnings: Vec<SaveWarning>, sources: Vec<String> },
    /// The version fails 4.36: `422 not_restorable`.
    No,
    /// The version fails 7.18: `422 invalid_document`.
    Invalid(Vec<hune_core::error::ValidationError>),
}

/// `POST /api/dashboards/{uid}/restore`: a new version with the document of an earlier one (6.18).
///
/// The order is the order of 11.21 for a restore: the catalog (11.23), the use alone, the store read, the validation
/// in `spawn_blocking`, then the others and the use. A subject without the use gets `403` and Hune reads nothing.
/// The validation of the handler is the one validation of the version: the store gets the valid document, and under
/// its lock it checks only the version, `If-Match` and the folder (6.18).
pub async fn restore(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let body = parse_and_check_with(&req.body, &RESTORE_SHAPE, &[], |v| format_faults(v, &[], false, true))?;
    let message = read_message(&body.value)?;
    let number = body.value["version"].as_u64().ok_or_else(ApiError::internal)?;
    let services = services(&ctx)?;
    let Some(entry) = services.catalog.get(&uid) else {
        return Err(gate::unknown(&ctx, "No such dashboard.").await);
    };
    let folder_text = entry.folder().as_str().to_owned();
    if services.store.uid_is_read_only(&uid) {
        return Err(gate::read_only_refusal(&ctx, &folder_text, Some(uid.as_str())).await);
    }
    // The use alone first: the others name the sources of the version, which Hune has not read yet (11.21).
    gate::dashboard_write_use(&ctx, &folder_text, uid.as_str()).await?;
    // The permit comes before the store read, so that a restore that waits holds no bytes of a version (6.27).
    let permit = check_permit(&ctx.state, &ctx.subject).await?;
    let op = StoreOp::begin(&ctx, "version");
    let read = services.store.version(&uid, number).await;
    op.end(&ctx.state, "version", &read);
    let version = read.map_err(|e| map_store_error(&e))?;
    let not_restorable = || ApiError::new(Code::NotRestorable, "This version cannot be restored.");
    if version.bytes.is_none() {
        return Err(not_restorable());
    }
    // A version that fails 4.36 is not restorable, and one that fails 7.18 is an invalid document (6.18). The
    // work runs in `spawn_blocking` with the permit and the place of the body (6.27).
    let state = std::sync::Arc::clone(&ctx.state);
    let expected = uid.clone();
    let checked = run_check(&ctx.state, permit, req.place.take(), move || {
        let Ok(valid) = ValidVersion::check(&expected, &version) else { return Restorable::No };
        let (errors, warnings) = sql_save_checks(valid.document().document(), &declared_sources(&state));
        if !errors.is_empty() {
            return Restorable::Invalid(errors);
        }
        let sources = gate::document_sources(valid.document().document());
        Restorable::Yes { version: Box::new(valid), warnings, sources }
    })
    .await?;
    let (valid, warnings) = match checked {
        Restorable::Yes { version, warnings, sources } => {
            // The others in one batch, then the use (11.21). The use goes to the PDP a second time: a write route
            // never uses a kept answer (11.27).
            gate::dashboard_write(&ctx, &folder_text, uid.as_str(), &sources).await?;
            (version, warnings)
        }
        Restorable::No => return Err(not_restorable()),
        Restorable::Invalid(errors) => return Err(ApiError::invalid_document(&errors)),
    };
    let if_match = ctx.if_match.ok_or_else(ApiError::internal)?;
    let folder = entry.folder().clone();
    let etag_before = entry.etag().to_hex();
    let meta = write_meta(&ctx, message);
    let op = StoreOp::begin(&ctx, "restore");
    let result = services.store.restore_valid(&valid, Some(&if_match), Some(&folder), &meta).await;
    op.end(&ctx.state, "restore", &result);
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
            "restore",
            uid.as_str(),
            saved.folder.as_str(),
            None,
            saved.version,
            Some(&etag_before),
            saved.etag.as_ref().map(Etag::to_hex).as_deref(),
            Some(valid.document().bytes().len() as u64),
        );
    }
    write_answer(services, &saved, &meta, Some(&warnings), false, valid.document().etag()).await
}

static MOVE_MEMBERS: [Member; 2] = [
    Member { name: "folder", shape: Shape::Str { min_chars: 1, max_chars: 40 }, required: true },
    Member { name: "message", shape: Shape::Str { min_chars: 0, max_chars: 500 }, required: false },
];
static MOVE_SHAPE: Shape = Shape::Object(&MOVE_MEMBERS);

/// `POST /api/dashboards/{uid}/move`: moves a dashboard to another folder (6.19).
pub async fn move_to(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let body = parse_and_check_with(&req.body, &MOVE_SHAPE, &[], |v| format_faults(v, &["folder"], false, true))?;
    let target = read_folder_id(body.value["folder"].as_str().unwrap_or(""), "/folder")?;
    let message = read_message(&body.value)?;
    let services = services(&ctx)?;
    let Some(entry) = services.catalog.get(&uid) else {
        return Err(gate::unknown(&ctx, "No such dashboard.").await);
    };
    if ctx.state.access.is_some() && !services.catalog.folders().iter().any(|f| f.id == target) {
        // The other permission names the dashboard, which exists: it comes first, as for a folder that exists (11.23).
        let others = gate::Others::Dashboard { folder: entry.folder().as_str(), uid: uid.as_str() };
        return Err(gate::unknown_with(&ctx, "No such folder.", others).await);
    }
    // A move out of a read-only store, or into a folder of a read-only store, is refused (5.42).
    if services.store.uid_is_read_only(&uid) {
        return Err(gate::read_only_refusal(&ctx, entry.folder().as_str(), Some(uid.as_str())).await);
    }
    if services.store.folder_is_read_only(&target) {
        return Err(gate::read_only_refusal(&ctx, target.as_str(), None).await);
    }
    gate::move_dashboard(&ctx, entry.folder().as_str(), uid.as_str(), target.as_str()).await?;
    let if_match = ctx.if_match.ok_or_else(ApiError::internal)?;
    let from = entry.folder().clone();
    let etag = *entry.etag();
    let meta = write_meta(&ctx, message);
    let op = StoreOp::begin(&ctx, "move_to");
    let result = services.store.move_to(&uid, &target, &if_match, &from, &meta).await;
    op.end(&ctx.state, "move_to", &result);
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
            VersionKind::Move.as_str(),
            uid.as_str(),
            saved.folder.as_str(),
            Some(from.as_str()),
            saved.version,
            Some(&etag.to_hex()),
            Some(&etag.to_hex()),
            None,
        );
    }
    write_answer(services, &saved, &meta, None, false, &etag).await
}
