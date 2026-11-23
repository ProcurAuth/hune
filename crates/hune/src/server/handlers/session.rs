// SPDX-License-Identifier: MIT OR Apache-2.0
//! `GET /api/session` (6.10).

use crate::server::build_info;
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::limits::{
    QUERY_BODY_BYTES, SESSION_MAX_DATA_POINTS, SESSION_SQL_BYTES, SESSION_VALUE_BYTES, SESSION_VARIABLE_VALUES,
};
use crate::server::response::{Response, json_response};
use crate::web_assets::BUILD_HASH;
use http::StatusCode;
use hune_core::limits::{MAX_DOCUMENT_BYTES, REFRESH_CHOICES};
use serde::Serialize;

#[derive(Serialize)]
struct Limits {
    document_bytes: u64,
    sql_bytes: u64,
    variable_values: u64,
    value_bytes: u64,
    max_data_points: u64,
    query_body_bytes: u64,
}

/// The run time limits of a repeat (4.13a, 4.35): the values of `server.repeat_max_per_*`.
#[derive(Serialize)]
struct RepeatLimits {
    panel: u32,
    section: u32,
    dashboard: u32,
}

/// The tile source of the map (6.10, 13.12c): the three values of the block `geomap`. The values are public.
#[derive(Serialize)]
struct SessionGeomap<'a> {
    tile_url: &'a str,
    attribution: &'a str,
    max_zoom: u8,
}

#[derive(Serialize)]
struct Session<'a> {
    mode: &'a str,
    subject: &'a str,
    editing: bool,
    /// The value of `server.query_inspect` (6.10): the page shows the item Inspect only when it is true.
    query_inspect: bool,
    /// The values of the three keys `server.repeat_max_per_*` (6.10): the page checks a repeat with them.
    repeat_limits: RepeatLimits,
    /// The value of `server.transform_max_cells` (6.10): the page checks the cells of a step of a transform with it.
    transform_max_cells: u32,
    /// The block `geomap` (6.10): `null` without the block. The page checks the template again (9.19i).
    geomap: Option<SessionGeomap<'a>>,
    version: &'a str,
    build: &'a str,
    refresh_choices: &'a [&'a str],
    limits: Limits,
}

/// Gives the mode, the subject and the limits for the page.
pub async fn session(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let body = Session {
        mode: ctx.state.access_mode(),
        subject: &ctx.subject,
        editing: ctx.state.writes_enabled(),
        query_inspect: ctx.state.config.server.query_inspect,
        repeat_limits: RepeatLimits {
            panel: ctx.state.config.server.repeat_max_per_panel,
            section: ctx.state.config.server.repeat_max_per_section,
            dashboard: ctx.state.config.server.repeat_max_per_dashboard,
        },
        transform_max_cells: ctx.state.config.server.transform_max_cells,
        geomap: ctx.state.config.geomap.as_ref().map(|g| SessionGeomap {
            tile_url: &g.tile_url.text,
            attribution: &g.attribution,
            max_zoom: g.max_zoom,
        }),
        version: build_info::VERSION,
        build: BUILD_HASH,
        refresh_choices: &REFRESH_CHOICES,
        limits: Limits {
            document_bytes: MAX_DOCUMENT_BYTES as u64,
            sql_bytes: SESSION_SQL_BYTES,
            variable_values: SESSION_VARIABLE_VALUES,
            value_bytes: SESSION_VALUE_BYTES,
            max_data_points: SESSION_MAX_DATA_POINTS,
            query_body_bytes: QUERY_BODY_BYTES,
        },
    };
    Ok(json_response(StatusCode::OK, &body))
}
