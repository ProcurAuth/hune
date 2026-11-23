// SPDX-License-Identifier: MIT OR Apache-2.0
//! `GET /api/sources` (6.22): the ids, the labels and the kinds from the configuration.

use super::gate;
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::response::{Response, json_response};
use http::StatusCode;
use serde::Serialize;

#[derive(Serialize)]
struct Item<'a> {
    id: &'a str,
    label: &'a str,
    kind: &'static str,
    state: &'static str,
}

#[derive(Serialize)]
struct SourceList<'a> {
    items: Vec<Item<'a>>,
}

/// Lists the sources in byte order of the id. The list is empty when `server.editing` is false, because
/// the editor is off then (6.22). It holds no host, no URL, no user and no secret.
pub async fn list(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let state = &ctx.state;
    let mut items = Vec::new();
    if state.writes_enabled() {
        let health = state.health.sources();
        let mut sources = crate::server::source_blocks::blocks(&state.config);
        sources.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        // Mode `authzen`: `source.query` on each source, in one display call (11.25).
        let ids: Vec<String> = sources.iter().map(|s| s.id.to_owned()).collect();
        if let Some(seen) = gate::visible_sources(&ctx, &ids).await? {
            let mut keep = seen.into_iter();
            sources.retain(|_| keep.next().unwrap_or(false));
        }
        for s in sources {
            items.push(Item {
                id: s.id,
                label: s.label,
                kind: s.kind.as_str(),
                state: health.get(s.id).copied().unwrap_or("unknown"),
            });
        }
    }
    Ok(json_response(StatusCode::OK, &SourceList { items }))
}
