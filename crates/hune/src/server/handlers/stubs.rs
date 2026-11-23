// SPDX-License-Identifier: MIT OR Apache-2.0
//! The routes that other parts of the product fill. Each gives `500 internal_error` for now.

use crate::access::{Access, OptionChild, Who, cut_label, parse_options_query, walk_options};
use crate::config::LogLevel;
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::response::{Response, bytes_response, json_response};
use crate::server::route::RouteId;
use bytes::Bytes;
use http::StatusCode;
use hune_store::CatalogEntry;
use std::sync::Arc;

/// The two ProcurAuth routes (11.18, 11.26). Mode `open` answers `404 not_found` (6.23).
pub async fn well_known(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let Some(access) = ctx.state.access.clone() else {
        return Err(ApiError::not_found("No such path."));
    };
    match ctx.route.id {
        RouteId::WellKnownActions => {
            // The same bytes at each read, with no token and no question (11.18, 11.29).
            Ok(bytes_response(StatusCode::OK, "application/json", Bytes::from(access.actions_body())))
        }
        _ => options_route(&ctx, &access).await,
    }
}

/// `GET /.well-known/procurauth/resources` (11.26): the children of a node that the subject can see.
async fn options_route(ctx: &Ctx, access: &Arc<Access>) -> Result<Response, ApiError> {
    let identity = ctx.identity.clone().ok_or_else(ApiError::internal)?;
    let who = Who { identity, request_id: ctx.request_id.clone(), trace: ctx.trace.clone() };
    let parsed = parse_options_query(ctx.raw_query.as_deref(), access)?;
    let services = super::support::services(ctx)?;
    let mut children: Vec<OptionChild> = match parsed.query.type_name.as_str() {
        "folder" => services
            .catalog
            .folders()
            .iter()
            .map(|f| OptionChild {
                id: f.id.as_str().to_owned(),
                label: cut_label(f.title.as_str()),
                has_children: true,
            })
            .collect(),
        "dashboard" => {
            let folder = parsed.query.parent.node().id.clone();
            services
                .catalog
                .entries()
                .into_iter()
                .filter(|e| e.folder().as_str() == folder)
                .map(|e| {
                    let label = match &*e {
                        CatalogEntry::Valid { doc, .. } => doc.title().to_owned(),
                        CatalogEntry::Invalid { uid, .. } => uid.as_str().to_owned(),
                    };
                    OptionChild { id: e.uid().as_str().to_owned(), label: cut_label(&label), has_children: false }
                })
                .collect()
        }
        _ => crate::server::source_blocks::blocks(&ctx.state.config)
            .iter()
            .map(|c| OptionChild { id: c.id.to_owned(), label: cut_label(c.label), has_children: false })
            .collect(),
    };
    children.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
    let walk = walk_options(access, &who, &parsed, children).await?;
    ctx.state
        .logger
        .event(LogLevel::Info, "access.options")
        .request_id(&ctx.request_id)
        .field("subject", ctx.subject.as_str())
        .field("type", parsed.query.type_name.as_str())
        .field("parent", parsed.parent_text.as_str())
        .field("items", walk.items.len() as u64)
        .field("questions", walk.questions as u64)
        .emit();
    let items: Vec<serde_json::Value> = walk
        .items
        .iter()
        .map(|i| serde_json::json!({"id": i.id, "label": i.label, "has_children": i.has_children}))
        .collect();
    Ok(json_response(StatusCode::OK, &serde_json::json!({"items": items, "next": walk.next})))
}

/// A route that is not built yet.
pub async fn todo(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    // TODO(phase 2): this route needs the store or a source.
    ctx.state
        .logger
        .event(crate::config::LogLevel::Error, "http.not_implemented")
        .request_id(&ctx.request_id)
        .field("route", ctx.route.label)
        .emit();
    Err(ApiError::internal())
}
