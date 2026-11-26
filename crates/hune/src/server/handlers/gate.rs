// SPDX-License-Identifier: MIT OR Apache-2.0
//! The questions of each route in the mode `authzen` (11.21, 11.23, 11.25).
//!
//! Each function asks the PDP through `crate::access::Access` and gives the grant or the error of
//! 11.28. In the mode `open`, each function answers at once and asks nothing (11.22). A handler calls
//! a function of this module at step 8 of 6.28, after the checks of the shape of the request and
//! before the first write to the store and the first byte to a source. The decision of a write route and the question
//! of 11.23 never use a kept answer (11.27).

use crate::access::{Access, Who};
use crate::server::ctx::Ctx;
use crate::server::error::{ApiError, Code};
use crate::server::route::RouteId;
use hune_core::document::Document;
use procurauth_pep::Allowed;
use procurauth_pep::ceilings::Ceilings;
use procurauth_pep::operation::{Operation, Permission};
use std::ops::Range;
use std::sync::Arc;

const READ: &str = "dashboard.read";
const WRITE: &str = "dashboard.write";
const HISTORY: &str = "dashboard.history";
const EXPORT: &str = "dashboard.export";
const QUERY: &str = "source.query";

/// The most questions of one page of a list (11.25).
const PAGE_QUESTIONS: usize = 200;
/// The most folders that one page of `GET /api/dashboards` examines. Each folder costs 5 questions
/// at most before its dashboards, so the page stays within `PAGE_QUESTIONS`.
const FOLDERS_PER_PAGE: usize = 36;

/// The ceilings of the use of a granted operation (11.24). They are empty in the mode `open`.
#[derive(Debug, Clone, Default)]
pub struct Grant {
    /// The ceilings that the `true` answer of the use gave.
    pub ceilings: Ceilings,
}

/// The access layer and the subject of the request, or `None` in the mode `open`.
fn parts(ctx: &Ctx) -> Result<Option<(Arc<Access>, Who)>, ApiError> {
    let Some(access) = ctx.state.access.clone() else { return Ok(None) };
    let identity = ctx.identity.clone().ok_or_else(ApiError::internal)?;
    Ok(Some((access, Who { identity, request_id: ctx.request_id.clone(), trace: ctx.trace.clone() })))
}

/// The action of the use of a route of 11.21, or `None` for a route that asks no decision of its own. A read with
/// `download=true` has the use `dashboard.export`.
fn route_use(id: RouteId, download: bool) -> Option<&'static str> {
    match id {
        RouteId::DashboardGet if download => Some(EXPORT),
        RouteId::History | RouteId::HistoryVersion => Some(HISTORY),
        RouteId::DashboardGet
        | RouteId::PanelQuery
        | RouteId::VariableOptions
        | RouteId::AnnotationQuery => Some(READ),
        RouteId::FolderCreate
        | RouteId::FolderRename
        | RouteId::FolderDelete
        | RouteId::DashboardCreate
        | RouteId::DashboardSave
        | RouteId::DashboardDelete
        | RouteId::Restore
        | RouteId::Move => Some(WRITE),
        RouteId::SourcePreview => Some(QUERY),
        RouteId::Session
        | RouteId::FoldersList
        | RouteId::DashboardsList
        | RouteId::SourcesList
        | RouteId::WellKnownActions
        | RouteId::WellKnownResources
        | RouteId::PageList
        | RouteId::PageDashboard
        | RouteId::Static => None,
    }
}

/// Gives each `403 forbidden` of a route one body: the member `action` is the use of the route (11.30). The refused
/// question, and the existence of the uid or of the folder, do not change the body (11.23). A route that asks no
/// decision of its own keeps the error as it is. The refusal of 17.7a (`approval_cannot_wait`) keeps its own fixed
/// body: it has no `action` and it does not depend on the route.
fn same_refusal(ctx: &Ctx, e: ApiError) -> ApiError {
    if e.plain.is_some() {
        return e;
    }
    let download = ctx.route.id == RouteId::DashboardGet && ctx.query.flag("download").unwrap_or(false);
    match (e.code, route_use(ctx.route.id, download)) {
        (Code::Forbidden, Some(action)) => crate::access::forbidden(action),
        _ => e,
    }
}

/// True for a route whose questions never use a kept answer: each write route, the preview included (11.27). Thus a
/// uid or a folder that exists and one that does not exist cost the same PDP requests (11.23, rule 6).
fn fresh_route(id: RouteId) -> bool {
    matches!(route_use(id, false), Some(WRITE | QUERY))
}

/// Asks for one operation of a route (11.21), with the refusal of `same_refusal`. `fresh` asks with no kept answer
/// on each route: the question of 11.23.
async fn decide_as(
    ctx: &Ctx,
    access: &Arc<Access>,
    who: &Who,
    op: Operation,
    fresh: bool,
) -> Result<Allowed, ApiError> {
    let answer = if fresh || fresh_route(ctx.route.id) {
        access.decide_fresh(who, op).await
    } else {
        access.decide(who, op).await
    };
    answer.map_err(|e| same_refusal(ctx, e))
}

/// Asks for one operation of a route (11.21). A write route asks with no kept answer.
async fn decide(ctx: &Ctx, access: &Arc<Access>, who: &Who, op: Operation) -> Result<Allowed, ApiError> {
    decide_as(ctx, access, who, op, false).await
}

/// The ids of the sources that a document names, without duplicates (11.21).
pub fn document_sources(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |id: &str| {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_owned());
        }
    };
    // The variables first, in document order: a `query` variable and an `adhoc` variable name a source (4.10c). An
    // `adhoc` variable has no SQL text, and its source counts as the pre-pass reads it (11.21).
    for v in doc.variables() {
        match &v.kind {
            hune_core::document::VariableKind::Query { source, .. }
            | hune_core::document::VariableKind::Adhoc { source, .. } => push(source.as_str()),
            _ => {}
        }
    }
    for text in doc.sql_texts() {
        push(text.source.as_str());
    }
    out
}

fn with_sources(mut op: Operation, access: &Access, sources: &[String]) -> Operation {
    for s in sources {
        op = op.also(QUERY, access.tree().source(s));
    }
    op
}

/// The other permissions of a write route that name only items that exist (11.23, rule 1). The question of 11.23
/// asks them first, as the route asks them for an item that exists.
pub enum Others<'a> {
    /// The route has no other permission, or its other permission names the unknown item.
    None,
    /// `source.query` on each source of the pre-pass: a create and a save.
    Sources(&'a [String]),
    /// `dashboard.write` on N(uid): a move to a folder that does not exist.
    Dashboard { folder: &'a str, uid: &'a str },
}

/// The error for a dashboard uid or a folder id that the catalog does not hold (11.23), with no other permission.
pub async fn unknown(ctx: &Ctx, message: &str) -> ApiError {
    unknown_with(ctx, message, Others::None).await
}

/// The error for a dashboard uid or a folder id that the catalog does not hold (11.23). In the mode `open`, it is
/// `404`. In the mode `authzen`, one operation decides: the `others` first, then `dashboard.read` on the service as
/// the use. A `false` answer gives `403`, `true` for all gives `404`, and a failure gives `503`. The operation never
/// uses a kept answer (11.27). Thus it sends the PDP requests that the route sends for an item that exists. The `403`
/// has the body of each other refusal of the route (11.30).
pub async fn unknown_with(ctx: &Ctx, message: &str, others: Others<'_>) -> ApiError {
    let found = ApiError::not_found(message.to_owned());
    let (access, who) = match parts(ctx) {
        Err(e) => return e,
        Ok(None) => return found,
        Ok(Some(p)) => p,
    };
    let mut op = Operation::new(READ, access.tree().service());
    match others {
        Others::None => {}
        Others::Sources(list) => op = with_sources(op, &access, list),
        Others::Dashboard { folder, uid } => op = op.also(WRITE, access.tree().dashboard(folder, uid)),
    }
    match decide_as(ctx, &access, &who, op, true).await {
        Ok(_) => found,
        Err(e) => e,
    }
}

/// The refusal of a write whose target is in a read-only store (5.42, 11.35). Hune never asks `dashboard.write`
/// for such a target: a grant of it changes nothing. In the mode `authzen`, one question comes first: `dashboard.read`
/// on the dashboard `N(uid)`, or on the folder `F(folder)` when `uid` is `None`. A subject that cannot read the
/// target gets the refusal of any other write (11.30), byte for byte, so the answer does not tell that the target
/// exists. A subject that can read it gets `403 read_only_store`. The mode `open` answers `read_only_store` at once.
pub async fn read_only_refusal(ctx: &Ctx, folder: &str, uid: Option<&str>) -> ApiError {
    let refusal = ApiError::new(Code::ReadOnlyStore, "The store of this item is read-only.");
    let (access, who) = match parts(ctx) {
        Err(e) => return e,
        Ok(None) => return refusal,
        Ok(Some(p)) => p,
    };
    let node = match uid {
        Some(u) => access.tree().dashboard(folder, u),
        None => access.tree().folder(folder),
    };
    match decide_as(ctx, &access, &who, Operation::new(READ, node), true).await {
        Ok(_) => refusal,
        Err(e) => e,
    }
}

/// `dashboard.read` on `N(uid)`: a read route and the routes of the queries (11.21).
pub async fn dashboard_read(ctx: &Ctx, folder: &str, uid: &str) -> Result<Grant, ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(Grant::default()) };
    let allowed = decide(ctx, &access, &who, Operation::new(READ, access.tree().dashboard(folder, uid))).await?;
    Ok(Grant { ceilings: allowed.ceilings().clone() })
}

/// `dashboard.export` on `N(uid)` as the use, with `dashboard.read` as the other permission: a read with
/// `download=true` (11.21).
pub async fn dashboard_export(ctx: &Ctx, folder: &str, uid: &str) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    let node = access.tree().dashboard(folder, uid);
    decide(ctx, &access, &who, Operation::new(EXPORT, node.clone()).also(READ, node)).await.map(|_| ())
}

/// The questions of the two history routes (11.21, 6.17). The key `server.history` is checked before this call. A
/// dashboard of a read-only store has no history: after `dashboard.read` alone, the answer is
/// `404 history_unavailable`. Else `dashboard.history` is the use and `dashboard.read` the other permission.
pub async fn history_access(ctx: &Ctx, folder: &str, uid: &str, read_only: bool) -> Result<(), ApiError> {
    let unavailable = || ApiError::new(Code::HistoryUnavailable, "This dashboard has no history.");
    let Some((access, who)) = parts(ctx)? else {
        return if read_only { Err(unavailable()) } else { Ok(()) };
    };
    let node = access.tree().dashboard(folder, uid);
    if read_only {
        decide(ctx, &access, &who, Operation::new(READ, node)).await?;
        return Err(unavailable());
    }
    decide(ctx, &access, &who, Operation::new(HISTORY, node.clone()).also(READ, node)).await.map(|_| ())
}

/// `dashboard.write` on `N(uid)`, with `source.query` on each source of the new document (11.21).
pub async fn dashboard_write(ctx: &Ctx, folder: &str, uid: &str, sources: &[String]) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    let op = with_sources(Operation::new(WRITE, access.tree().dashboard(folder, uid)), &access, sources);
    decide(ctx, &access, &who, op).await.map(|_| ())
}

/// The use question only, for the cases that need no other permission: a delete, a validation, and
/// an error that comes before the sources are known.
pub async fn dashboard_write_use(ctx: &Ctx, folder: &str, uid: &str) -> Result<(), ApiError> {
    dashboard_write(ctx, folder, uid, &[]).await
}

/// A restore: `dashboard.write` on `N(uid)` as the use, with `dashboard.history` on `N(uid)` as another permission and
/// `source.query` on each source of the restored version (11.21). The first call has no source: Hune has not read the
/// version yet.
pub async fn dashboard_restore(ctx: &Ctx, folder: &str, uid: &str, sources: &[String]) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    let node = access.tree().dashboard(folder, uid);
    let op = with_sources(Operation::new(WRITE, node.clone()).also(HISTORY, node), &access, sources);
    decide(ctx, &access, &who, op).await.map(|_| ())
}

/// `dashboard.write` on `F(folder)`, with `source.query` on each source: a create (11.21).
pub async fn folder_write_with_sources(ctx: &Ctx, folder: &str, sources: &[String]) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    let op = with_sources(Operation::new(WRITE, access.tree().folder(folder)), &access, sources);
    decide(ctx, &access, &who, op).await.map(|_| ())
}

/// `dashboard.write` on the service: the creation of a folder (11.21).
pub async fn service_write(ctx: &Ctx) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    decide(ctx, &access, &who, Operation::new(WRITE, access.tree().service())).await.map(|_| ())
}

/// A move: `dashboard.write` on `N(uid)` as another permission, then on `F(target)` as the use (11.21).
pub async fn move_dashboard(ctx: &Ctx, folder: &str, uid: &str, target: &str) -> Result<(), ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(()) };
    let op = Operation::new(WRITE, access.tree().folder(target)).also(WRITE, access.tree().dashboard(folder, uid));
    decide(ctx, &access, &who, op).await.map(|_| ())
}

/// The create of a uid that the store holds: `dashboard.read` on its node decides between
/// `403` and `409 already_exists` (11.23, rule 3).
pub async fn already_exists(ctx: &Ctx, folder: &str, uid: &str) -> Result<ApiError, ApiError> {
    let exists = ApiError::new(Code::AlreadyExists, "This id exists already.");
    let Some((access, who)) = parts(ctx)? else { return Ok(exists) };
    decide(ctx, &access, &who, Operation::new(READ, access.tree().dashboard(folder, uid))).await?;
    Ok(exists)
}

/// The preview: `source.query` on `S(source)` as the use, and `dashboard.write` on the dashboard or on
/// the folder of a draft that is not saved as the other permission (11.21).
pub async fn preview(ctx: &Ctx, source: &str, draft: Draft<'_>) -> Result<Grant, ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(Grant::default()) };
    let other = match draft {
        Draft::Saved { folder, uid } => access.tree().dashboard(folder, uid),
        Draft::New { folder } => access.tree().folder(folder),
    };
    let op = Operation::new(QUERY, access.tree().source(source)).also(WRITE, other);
    let allowed = decide(ctx, &access, &who, op).await?;
    Ok(Grant { ceilings: allowed.ceilings().clone() })
}

/// The right to see the expanded SQL of a query (`server.query_inspect`). The mode
/// `open` asks nothing: the key is the control. In the mode `authzen`, the subject needs the same two
/// permissions as the preview of a saved dashboard: `source.query` on `S(source)` and
/// `dashboard.write` on `N(uid)`. This is stricter than `dashboard.read`: a viewer gets the data, and
/// an author gets the SQL too. A `false` answer, a failure of the PDP and a failure of a ceiling check all give `false`:
/// the SQL is left out and the query answer stays valid.
pub async fn query_inspect(ctx: &Ctx, folder: &str, uid: &str, source: &str) -> bool {
    if !ctx.state.config.server.query_inspect {
        return false;
    }
    let Ok(parts) = parts(ctx) else { return false };
    let Some((access, who)) = parts else { return true };
    // A dashboard of a read-only store has no `dashboard.write` question (5.42), so nobody gets its SQL here.
    let in_read_only_store = ctx
        .state
        .services()
        .is_some_and(|s| hune_core::ids::Uid::parse(uid).is_ok_and(|u| s.store.uid_is_read_only(&u)));
    if in_read_only_store {
        return false;
    }
    let op = Operation::new(QUERY, access.tree().source(source)).also(WRITE, access.tree().dashboard(folder, uid));
    access.check(&who, op).await.unwrap_or(false)
}

/// The dashboard of a preview.
pub enum Draft<'a> {
    /// A saved dashboard under edit.
    Saved { folder: &'a str, uid: &'a str },
    /// A new dashboard, in this folder.
    New { folder: &'a str },
}

/// The three flags of the member `access` of a dashboard (6.12, 11.25).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// `dashboard.write`.
    pub write: bool,
    /// `dashboard.history`.
    pub history: bool,
    /// `dashboard.export`.
    pub export: bool,
}

impl Flags {
    /// The flags of an item of a read-only store: no write and no history (5.42, 5.18).
    pub fn of_read_only_store(self) -> Flags {
        Flags { write: false, history: false, export: self.export }
    }
}

/// The flags of one dashboard (11.25): `dashboard.write`, `dashboard.history` and `dashboard.export` on `N(uid)`,
/// each with one call of `Pep::check`, in one batch. A question that the keys or the store exclude is not asked and
/// its flag is `false`. In the mode `open`, the flags are the process ceilings.
pub async fn flags(ctx: &Ctx, folder: &str, uid: &str, read_only: bool) -> Result<Flags, ApiError> {
    let server = &ctx.state.config.server;
    let (history, export) = (server.history && !read_only, server.export);
    let Some((access, who)) = parts(ctx)? else { return Ok(Flags { write: !read_only, history, export }) };
    let node = access.tree().dashboard(folder, uid);
    let mut ops = Vec::new();
    if !read_only {
        ops.push(Operation::new(WRITE, node.clone()));
    }
    if history {
        ops.push(Operation::new(HISTORY, node.clone()));
    }
    if export {
        ops.push(Operation::new(EXPORT, node));
    }
    let mut answers = access.check_many(&who, ops).await?.into_iter();
    let mut next = |asked: bool| asked && answers.next().unwrap_or(false);
    Ok(Flags { write: next(!read_only), history: next(history), export: next(export) })
}

/// The sources of the list: `source.query` on `S(s)` in one display call (11.25).
pub async fn visible_sources(ctx: &Ctx, ids: &[String]) -> Result<Option<Vec<bool>>, ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(None) };
    let perms = ids.iter().map(|s| Permission::new(QUERY, access.tree().source(s))).collect();
    access.display(&who, perms).await.map(Some)
}

/// The display filter of the list of folders (11.25).
pub struct FolderFilter {
    /// For each folder, in the order given: `None` when hidden, else the flag `access.write`.
    pub folders: Vec<Option<bool>>,
    /// The flag `access.create` at the top level.
    pub create: bool,
}

/// Steps 1 and 2 of 11.25, then the flags: at most 2 questions for each folder and one for the service. A folder of
/// a read-only store (`read_only(id)`) has no question for `dashboard.write` and the flag `false` (5.42).
pub async fn filter_folders(
    ctx: &Ctx,
    ids: &[String],
    read_only: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Option<FolderFilter>, ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(None) };
    let perms = ids.iter().map(|f| Permission::new(READ, access.tree().folder(f))).collect();
    let seen = access.display(&who, perms).await?;
    let mut folders: Vec<Option<bool>> = seen.iter().map(|v| v.then_some(false)).collect();
    let visible: Vec<usize> = (0..ids.len()).filter(|&i| seen[i] && !read_only(&ids[i])).collect();
    let mut ops: Vec<Operation> =
        visible.iter().map(|&i| Operation::new(WRITE, access.tree().folder(&ids[i]))).collect();
    ops.push(Operation::new(WRITE, access.tree().service()));
    let mut answers = access.check_many(&who, ops).await?;
    let create = answers.pop().unwrap_or(false);
    for (i, a) in visible.into_iter().zip(answers) {
        folders[i] = Some(a);
    }
    Ok(Some(FolderFilter { folders, create }))
}

/// The result of the filter of the list of dashboards.
pub struct DashboardFilter {
    /// The index of each visible row, with the flags of its folder.
    pub picked: Vec<(usize, Flags)>,
    /// The number of rows that the filter examined, from the start of the rows.
    pub examined: usize,
}

enum FolderState {
    Hidden,
    /// The grant on the folder covers each dashboard below it (P 4.3).
    All {
        flags: Flags,
    },
    /// Only some dashboards show: each one needs its own question.
    Some {
        flags: Flags,
    },
}

/// The display filter of one page of dashboards (11.25). `rows` are the `(folder, uid)` of the rows
/// in the order of the list. The filter asks at most 200 questions and stops when it holds `limit`
/// items. A folder of a read-only store (`read_only(folder)`) has no question for `dashboard.write` and
/// `dashboard.history` (5.42). A key that is `false` removes the question of its action (11.25).
pub async fn filter_dashboards(
    ctx: &Ctx,
    rows: &[(String, String)],
    limit: usize,
    read_only: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Option<DashboardFilter>, ApiError> {
    let Some((access, who)) = parts(ctx)? else { return Ok(None) };
    let mut groups: Vec<(String, Range<usize>)> = Vec::new();
    for (i, (folder, _)) in rows.iter().enumerate() {
        match groups.last_mut() {
            Some((f, range)) if f == folder => range.end = i + 1,
            _ => groups.push((folder.clone(), i..i + 1)),
        }
    }
    groups.truncate(FOLDERS_PER_PAGE);
    let mut used = 0usize;
    // Step 1: one display call for the folders.
    let perms = groups.iter().map(|(f, _)| Permission::new(READ, access.tree().folder(f))).collect();
    let seen = access.display(&who, perms).await?;
    used += groups.len();
    // Step 3: `dashboard.read`, `dashboard.write`, `dashboard.history` and `dashboard.export` on each visible
    // folder, with `check`.
    let visible: Vec<usize> = (0..groups.len()).filter(|&i| seen[i]).collect();
    let mut ops = Vec::new();
    let server = &ctx.state.config.server;
    let asks_write = |g: usize| !read_only(&groups[g].0);
    let asks_history = |g: usize| server.history && !read_only(&groups[g].0);
    let asks_export = server.export;
    for &g in &visible {
        let folder = access.tree().folder(&groups[g].0);
        ops.push(Operation::new(READ, folder.clone()));
        if asks_write(g) {
            ops.push(Operation::new(WRITE, folder.clone()));
        }
        if asks_history(g) {
            ops.push(Operation::new(HISTORY, folder.clone()));
        }
        if asks_export {
            ops.push(Operation::new(EXPORT, folder));
        }
    }
    let answers = access.check_many(&who, ops).await?;
    used += answers.len();
    let mut states: Vec<FolderState> = groups.iter().map(|_| FolderState::Hidden).collect();
    let mut it = answers.into_iter();
    for &g in &visible {
        let read = it.next().unwrap_or(false);
        let write = asks_write(g) && it.next().unwrap_or(false);
        let history = asks_history(g) && it.next().unwrap_or(false);
        let export = asks_export && it.next().unwrap_or(false);
        let flags = Flags { write, history, export };
        states[g] = if read { FolderState::All { flags } } else { FolderState::Some { flags } };
    }
    // Steps 4 and 5: the rows, in order.
    let mut picked: Vec<(usize, Flags)> = Vec::new();
    let mut examined = 0usize;
    'groups: for (g, (folder, range)) in groups.iter().enumerate() {
        if picked.len() >= limit {
            break;
        }
        match &states[g] {
            FolderState::Hidden => examined = range.end,
            FolderState::All { flags } => {
                for i in range.clone() {
                    if picked.len() >= limit {
                        break 'groups;
                    }
                    picked.push((i, *flags));
                    examined = i + 1;
                }
            }
            FolderState::Some { flags } => {
                let mut i = range.start;
                while i < range.end && picked.len() < limit && used < PAGE_QUESTIONS {
                    let take = (range.end - i).min(limit - picked.len()).min(PAGE_QUESTIONS - used);
                    let perms = (i..i + take)
                        .map(|k| Permission::new(READ, access.tree().dashboard(folder, &rows[k].1)))
                        .collect();
                    let answers = access.display(&who, perms).await?;
                    used += take;
                    for (k, seen) in (i..i + take).zip(answers) {
                        if seen {
                            picked.push((k, *flags));
                        }
                        examined = k + 1;
                    }
                    i += take;
                }
                if i < range.end {
                    break;
                }
            }
        }
    }
    Ok(Some(DashboardFilter { picked, examined }))
}
