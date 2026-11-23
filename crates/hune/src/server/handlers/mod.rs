// SPDX-License-Identifier: MIT OR Apache-2.0
//! The handlers of the routes. One function of the form `async fn(Ctx, Request) -> Result<Response, ApiError>`
//! for each route, and `dispatch` that matches the route identity.

mod annotation;
mod dashboards;
mod folders;
pub(crate) mod gate;
mod history;
mod pages;
mod paging;
pub(crate) mod prepass;
mod preview;
mod query;
mod session;
mod sources;
mod stubs;
pub(crate) mod support;

use super::ctx::{Ctx, Request};
use super::error::ApiError;
use super::response::Response;
use super::route::RouteId;

/// The errors of the save checks of 7.18 for a document, with the source blocks of the configuration. The server
/// runs these checks on each save. The command `import-grafana` runs the same function before it writes, so the
/// two paths cannot differ.
pub(super) fn save_check_errors(
    doc: &hune_core::document::Document,
    config: &crate::config::Config,
) -> Vec<hune_core::error::ValidationError> {
    support::sql_save_checks(doc, &super::source_blocks::blocks(config)).0
}

/// Calls the handler of the route of the context.
pub async fn dispatch(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    match ctx.route.id {
        RouteId::Session => session::session(ctx, req).await,
        RouteId::PageList | RouteId::PageDashboard => pages::page(ctx, req).await,
        RouteId::Static => pages::static_file(ctx, req).await,
        RouteId::WellKnownActions | RouteId::WellKnownResources => stubs::well_known(ctx, req).await,
        RouteId::FoldersList => folders::list(ctx, req).await,
        RouteId::FolderCreate => folders::create(ctx, req).await,
        RouteId::FolderRename => folders::rename(ctx, req).await,
        RouteId::FolderDelete => folders::delete(ctx, req).await,
        RouteId::DashboardsList => dashboards::list(ctx, req).await,
        RouteId::DashboardCreate => dashboards::create(ctx, req).await,
        RouteId::DashboardGet => dashboards::get(ctx, req).await,
        RouteId::DashboardSave => dashboards::save(ctx, req).await,
        RouteId::DashboardDelete => dashboards::delete(ctx, req).await,
        RouteId::History => history::history(ctx, req).await,
        RouteId::HistoryVersion => history::version(ctx, req).await,
        RouteId::Restore => history::restore(ctx, req).await,
        RouteId::Move => history::move_to(ctx, req).await,
        RouteId::SourcesList => sources::list(ctx, req).await,
        RouteId::PanelQuery => query::panel_query(ctx, req).await,
        RouteId::VariableOptions => query::variable_options(ctx, req).await,
        RouteId::AnnotationQuery => annotation::annotation_query(ctx, req).await,
        RouteId::SourcePreview => preview::preview(ctx, req).await,
    }
}

/// Makes the shell answer with a status. The shell is the same for each page route (6.25).
pub(super) fn shell_response(ctx: &Ctx, status: http::StatusCode) -> Response {
    let mut r = super::response::bytes_response(status, "text/html; charset=utf-8", ctx.state.shell.clone());
    r.headers_mut().insert(http::header::CACHE_CONTROL, http::HeaderValue::from_static("no-cache"));
    r
}

/// The history cursor of a version, for the tests of the routes.
#[cfg(test)]
pub fn history_cursor_for_test(version: u64) -> String {
    paging::history_cursor(version)
}
