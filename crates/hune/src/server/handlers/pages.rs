//! The page routes and the static files (6.25).

use super::shell_response;
use crate::server::ctx::{Ctx, Request};
use crate::server::encoding::accepts_gzip;
use crate::server::error::ApiError;
use crate::server::response::{Response, bytes_response};
use crate::server::route::RouteId;
use crate::web_assets;
use bytes::Bytes;
use http::StatusCode;
use http::header::{CACHE_CONTROL, CONTENT_ENCODING, HeaderValue, VARY};
use hune_core::ids::is_valid_id;

/// The longest `{slug}` in bytes (6.25).
const MAX_SLUG_BYTES: usize = 256;

/// `GET /`, `GET /d/{uid}` and `GET /d/{uid}/{slug}`: the shell. A uid that breaks the rule of
/// 4.3, or a slug that is not 1 to 256 bytes, gets the shell with the status 404.
pub async fn page(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    if ctx.route.id == RouteId::PageDashboard {
        let uid_ok = ctx.params.get("uid").is_some_and(is_valid_id);
        let slug_ok = ctx.params.get("slug").is_none_or(|s| !s.is_empty() && s.len() <= MAX_SLUG_BYTES);
        if !(uid_ok && slug_ok) {
            return Ok(shell_response(&ctx, StatusCode::NOT_FOUND));
        }
    }
    Ok(shell_response(&ctx, StatusCode::OK))
}

/// `GET /static/{build-hash}/{path}`: an embedded file. A wrong hash or an unknown path gives `404 not_found`.
///
/// With `server.compress_static`, a file of a text type has `Vary: Accept-Encoding`, and a request that
/// accepts gzip gets the gzip copy of start with `Content-Encoding: gzip` (9.3). The answer is always the
/// whole file or the whole copy: the server ignores `Range`. A `HEAD` gets the headers of the `GET`.
/// The `Cache-Control` of a static file (6.25, 12): never `public` (decision ID-K315).
pub const STATIC_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

pub async fn static_file(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let build = ctx.params.get("build").unwrap_or("");
    let path = ctx.params.get("path").unwrap_or("");
    let Some(file) = web_assets::find_static(build, path) else {
        return Err(ApiError::not_found("No such file."));
    };
    let varies = ctx.state.config.server.compress_static && web_assets::is_compressible(file.content_type);
    let copy = if varies && accepts_gzip(&req.headers) { web_assets::gzip_copy(file) } else { None };
    let gzip = copy.is_some();
    let body = copy.unwrap_or_else(|| Bytes::from_static(file.bytes));
    let mut r = bytes_response(StatusCode::OK, file.content_type, body);
    let headers = r.headers_mut();
    // `private` (decision ID-K315): the browser keeps the file for a year, and a shared cache in front of the edge never gives it
    // to a client that has no token (mode `authzen`, 11.29). The file and its gzip copy have the same value.
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(STATIC_CACHE_CONTROL));
    if varies {
        headers.insert(VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    if gzip {
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    Ok(r)
}
