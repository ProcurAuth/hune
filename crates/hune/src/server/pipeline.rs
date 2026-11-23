// SPDX-License-Identifier: MIT OR Apache-2.0
//! The steps of a request of the main listener, in the order of 6.28.

use super::conn::ConnShared;
use super::ctx::{BodyPlace, Ctx, Place, Request};
use super::error::{ApiError, Code};
use super::handlers;
use super::headers;
use super::limits::{
    MAX_HEADER_BYTES, MAX_HEADERS, MAX_TARGET_BYTES, PREVIEW_BODY_BYTES, QUERY_BODY_BYTES, SMALL_BODY_BYTES,
};
use super::query::Query;
use super::response::{Response, bytes_response, error_response};
use super::route::{self, BodyClass, Matched, Region, RouteId, Verb};
use super::spans::RequestSpan;
use super::state::{AppState, Phase};
use crate::log::HttpRequestLog;
use crate::telemetry::Tracing;
use crate::web_assets::BUILD_HASH;
use bytes::{Bytes, BytesMut};
use http::header::{CACHE_CONTROL, CONTENT_LENGTH, HeaderMap, HeaderValue, TRANSFER_ENCODING};
use http::{Method, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Body as _;
use hyper::body::Incoming;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, TryAcquireError};

/// What the steps give back: the route label, the answer and the size of the request.
struct Handled {
    label: &'static str,
    result: Result<Response, ApiError>,
    request_bytes: u64,
    /// The region of the path, for the headers of the answer. `None` before the route match.
    region: Option<Region>,
    /// The subject for the log line: `-` before the token check (14.7).
    subject: String,
}

impl Handled {
    fn error(label: &'static str, e: ApiError) -> Self {
        Handled { label, result: Err(e), request_bytes: 0, region: None, subject: "-".to_owned() }
    }
}

/// The body limit of a route (6.27).
pub fn body_limit(class: BodyClass, max_request_bytes: u64) -> u64 {
    match class {
        BodyClass::None => 0,
        BodyClass::Query => QUERY_BODY_BYTES,
        BodyClass::Preview => PREVIEW_BODY_BYTES,
        BodyClass::Doc => max_request_bytes,
        BodyClass::Small => SMALL_BODY_BYTES,
    }
}

/// Checks the size of the request target and of the headers (6.27, step 3 of 6.28).
pub fn check_head_limits(uri: &http::Uri, headers: &HeaderMap) -> Result<(), ApiError> {
    let target = uri.path_and_query().map_or(uri.path().len(), |p| p.as_str().len());
    if target > MAX_TARGET_BYTES {
        return Err(ApiError::new(Code::UriTooLong, "The request target is longer than 8 KiB."));
    }
    let count = headers.len();
    let bytes: usize = headers.iter().map(|(n, v)| n.as_str().len() + v.as_bytes().len() + 4).sum();
    if count > MAX_HEADERS || bytes > MAX_HEADER_BYTES {
        return Err(ApiError::new(
            Code::HeadersTooLarge,
            "The request has more than 100 headers, or its headers are larger than 16 KiB.",
        ));
    }
    Ok(())
}

fn has_body(headers: &HeaderMap) -> bool {
    headers.contains_key(TRANSFER_ENCODING)
        || headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|n| n > 0)
}

fn declared_length(headers: &HeaderMap) -> Option<u64> {
    headers.get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok())
}

/// The label `reason` of `hune_http_refused_total` for a code (14.8).
fn refused_reason(code: Code) -> Option<&'static str> {
    Some(match code {
        Code::UriTooLong => "uri_too_long",
        Code::HeadersTooLarge => "headers_too_large",
        Code::PayloadTooLarge => "payload_too_large",
        Code::CrossSiteRequest => "cross_site_request",
        Code::UnsupportedMediaType => "unsupported_media_type",
        Code::RequestTimeout => "body_timeout",
        _ => return None,
    })
}

/// Reads a body within the limit and the time of 6.27.
async fn read_body(mut body: Incoming, limit: u64, timeout: Duration) -> Result<Bytes, ApiError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut buf = BytesMut::new();
    loop {
        let frame = match tokio::time::timeout_at(deadline, body.frame()).await {
            Err(_) => {
                return Err(ApiError::new(Code::RequestTimeout, "The body did not arrive within 30 seconds."));
            }
            Ok(None) => break,
            Ok(Some(Err(_))) => return Err(ApiError::bad_request("The body could not be read.")),
            Ok(Some(Ok(frame))) => frame,
        };
        if let Ok(data) = frame.into_data() {
            if buf.len() as u64 + data.len() as u64 > limit {
                return Err(ApiError::payload_too_large(limit));
            }
            buf.extend_from_slice(&data);
        }
    }
    Ok(buf.freeze())
}

/// Takes the place of a body (6.27). In the mode `authzen`, a document body also takes a place of its subject
/// first (12.16): a subject that holds `MAX_INFLIGHT_PER_SUBJECT` document bodies gets `503 server_busy`.
fn take_place(state: &AppState, class: BodyClass, subject: &str) -> Result<BodyPlace, ApiError> {
    let busy = || ApiError::new(Code::ServerBusy, "Too many request bodies are in flight.");
    let subject_place = if class == BodyClass::Doc && state.access.is_some() {
        Some(state.subject_bodies.try_take(subject).ok_or_else(busy)?)
    } else {
        None
    };
    let sem = if class == BodyClass::Doc { &state.doc_bodies } else { &state.small_bodies };
    let body: OwnedSemaphorePermit = match sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(TryAcquireError::NoPermits | TryAcquireError::Closed) => return Err(busy()),
    };
    Ok(BodyPlace { body, subject: subject_place })
}

/// The steps 2 to 8 of 6.28 for one request.
async fn process(
    state: &Arc<AppState>,
    peer: SocketAddr,
    request_id: &str,
    trace: &Tracing,
    req: hyper::Request<Incoming>,
) -> Handled {
    let (parts, body) = req.into_parts();
    // Step 2: after the drain, a request gets `503 server_stopping`.
    if state.phase() == Phase::Refusing {
        return Handled::error("unmatched", ApiError::new(Code::ServerStopping, "The server is stopping."));
    }
    // Step 3: limits, base path, route.
    if let Err(e) = check_head_limits(&parts.uri, &parts.headers) {
        return Handled::error("unmatched", e);
    }
    let Some(rel) = route::strip_base(&state.config.server.base_path, parts.uri.path()) else {
        return Handled {
            label: "unmatched",
            result: Err(ApiError::not_found("No such path.")),
            request_bytes: 0,
            region: Some(Region::Api),
            subject: "-".to_owned(),
        };
    };
    let (spec, params) = match route::route(&parts.method, rel) {
        Matched::MethodNotAllowed(allow) => {
            return Handled {
                label: "unmatched",
                result: Err(ApiError::method_not_allowed(allow)),
                request_bytes: 0,
                region: Some(Region::of(rel)),
                subject: "-".to_owned(),
            };
        }
        Matched::NotFound(region) => {
            let is_get = parts.method == Method::GET || parts.method == Method::HEAD;
            let result = if region == Region::Page && is_get {
                // A path that no route names: the shell with the status 404 (6.25).
                Ok(shell_not_found(state))
            } else {
                Err(ApiError::not_found("No such path."))
            };
            return Handled {
                label: "unmatched",
                result,
                request_bytes: 0,
                region: Some(region),
                subject: "-".to_owned(),
            };
        }
        Matched::Route(spec, params) => (spec, params),
    };
    let region = Some(Region::of(rel));
    // The subject of the log line: `-` until the token check gives the claim `sub` (14.7).
    let subject_cell = std::sync::Mutex::new("-".to_owned());
    let fail = |e: ApiError| Handled {
        label: spec.label,
        result: Err(e),
        request_bytes: 0,
        region,
        subject: subject_cell.lock().map_or_else(|_| "-".to_owned(), |g| g.clone()),
    };
    let body_present = has_body(&parts.headers);
    if spec.body == BodyClass::None && body_present {
        return fail(ApiError::bad_request("This route takes no body."));
    }
    let limit = body_limit(spec.body, state.config.server.max_request_bytes);
    if spec.body != BodyClass::None
        && let Some(n) = declared_length(&parts.headers)
        && n > limit
    {
        return fail(ApiError::payload_too_large(limit));
    }
    let query = match spec.query {
        Some(allowed) => match Query::parse(parts.uri.query(), allowed) {
            Ok(q) => q,
            Err(e) => return fail(e),
        },
        None => Query::default(),
    };
    // Step 4: the write guard, then the content type.
    let writes = matches!(spec.verb, Verb::Post | Verb::Put | Verb::Delete);
    if writes {
        if let Err(e) = headers::cross_site_guard(&parts.headers) {
            return fail(e);
        }
        if body_present && let Err(e) = headers::check_content_type(&parts.headers) {
            return fail(e);
        }
    }
    // Step 5: the identity token (11.8). Mode `open` has no token and the subject is `-`.
    let mut identity = None;
    if let Some(access) = &state.access {
        let rule = crate::access::rule_of(spec.id);
        if rule != Some(crate::access::Rule::NoToken) {
            match access.authenticate(&parts.headers) {
                Ok(id) => {
                    if let Ok(mut g) = subject_cell.lock() {
                        *g = id.sub().to_owned();
                    }
                    identity = Some(id);
                }
                Err(u) => {
                    state
                        .logger
                        .event(crate::config::LogLevel::Info, "access.unauthenticated")
                        .request_id(request_id)
                        .field("method", parts.method.as_str())
                        .field("route", spec.label)
                        .field("reason", u.reason)
                        .emit();
                    return fail(ApiError::new(Code::Unauthenticated, "A valid identity token is necessary."));
                }
            }
        }
        if rule.is_none() {
            state
                .logger
                .event(crate::config::LogLevel::Error, "access.no_rule")
                .request_id(request_id)
                .field("method", parts.method.as_str())
                .field("route", spec.label)
                .emit();
            return fail(
                ApiError::new(Code::Forbidden, "You have no right for this operation.").with("action", "unknown"),
            );
        }
    }
    let subject = subject_cell.lock().map_or_else(|_| "-".to_owned(), |g| g.clone());
    // Step 6: the process ceiling.
    if spec.write && !state.writes_enabled() {
        return fail(ApiError::new(Code::EditingDisabled, "Editing is off on this server."));
    }
    // The headers of a write (6.3). `validate_only=true` needs none of them (6.15).
    let validate_only = matches!(spec.id, RouteId::DashboardCreate | RouteId::DashboardSave)
        && match query.flag("validate_only") {
            Ok(v) => v,
            Err(e) => return fail(e),
        };
    let mut if_match = None;
    let mut save_id = None;
    if !validate_only {
        if spec.needs_if_match {
            match headers::if_match(&parts.headers) {
                Ok(e) => if_match = Some(e),
                Err(e) => return fail(e),
            }
        }
        if spec.needs_idempotency {
            match headers::idempotency_key(&parts.headers) {
                Ok(k) => save_id = Some(k),
                Err(e) => return fail(e),
            }
        }
    }
    let if_none_match = if spec.id == RouteId::DashboardGet {
        match headers::if_none_match(&parts.headers) {
            Ok(v) => v,
            Err(e) => return fail(e),
        }
    } else {
        None
    };
    // Step 7: a place for the body, then the body. The handler can move the place into the work on the body,
    // so that the place stays taken until that work ends (6.27).
    let mut place = Place::default();
    let mut bytes = Bytes::new();
    if spec.body != BodyClass::None {
        match take_place(state, spec.body, &subject) {
            Ok(p) => place = Place::new(p),
            Err(e) => return fail(e),
        }
        match read_body(body, limit, state.timing.body_timeout).await {
            Ok(b) => bytes = b,
            Err(e) => return fail(e),
        }
    }
    let request_bytes = bytes.len() as u64;
    // Step 8: the handler.
    let ctx = Ctx {
        state: state.clone(),
        request_id: request_id.to_owned(),
        subject: subject.clone(),
        identity,
        raw_query: parts.uri.query().map(str::to_owned),
        route: spec,
        params,
        query,
        if_match,
        if_none_match,
        save_id,
        peer,
        trace: trace.clone(),
    };
    let request = Request { method: parts.method.clone(), headers: parts.headers, body: bytes, place };
    let result = handlers::dispatch(ctx, request).await;
    Handled { label: spec.label, result, request_bytes, region, subject }
}

fn shell_not_found(state: &AppState) -> Response {
    let mut r = bytes_response(StatusCode::NOT_FOUND, "text/html; charset=utf-8", state.shell.clone());
    r.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    r
}

/// Adds the headers that each answer of the main listener has (6.4, 12.8). `csp` is the policy that the server built at
/// start (12.8, 13.12c).
fn finish(
    mut response: Response,
    request_id: &str,
    is_head: bool,
    add_build: bool,
    csp: &HeaderValue,
) -> (Response, u64) {
    let h = response.headers_mut();
    headers::add_security_headers_with(h, csp);
    if let Ok(v) = HeaderValue::from_str(request_id) {
        h.insert(headers::X_REQUEST_ID, v);
    }
    if add_build {
        h.insert(headers::X_HUNE_BUILD, HeaderValue::from_static(BUILD_HASH));
    }
    if !h.contains_key(CACHE_CONTROL) {
        h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    let len = response.body().size_hint().exact().unwrap_or(0);
    if is_head {
        // The same headers as the `GET`, with the length of its body, and no body.
        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(len));
        *response.body_mut() = Full::new(Bytes::new());
        return (response, 0);
    }
    (response, len)
}

/// Runs one request of the main listener through the steps of 6.28. It never fails: each fault is an answer.
pub async fn serve_request(
    state: Arc<AppState>,
    conn: Arc<ConnShared>,
    peer: SocketAddr,
    req: hyper::Request<Incoming>,
) -> Result<Response, Infallible> {
    conn.request_started();
    let _in_flight = state.metrics.http_in_flight_guard();
    let started = Instant::now();
    let method = req.method().clone();
    let request_id = headers::request_id(req.headers());
    let content_length = declared_length(req.headers()).unwrap_or(0);
    let had_body = has_body(req.headers());

    let span = RequestSpan::begin(&state, &method, req.uri().path(), req.headers(), peer, &request_id);
    let handled = process(&state, peer, &request_id, &span.trace, req).await;

    let (response, code) = match handled.result {
        Ok(r) => (r, None),
        Err(e) => {
            if let Some(reason) = refused_reason(e.code) {
                state.metrics.http_refused(reason);
            }
            let mut response = error_response(&e);
            // Hune does not read the body of a request that has no valid token: close (11.8).
            if e.code == Code::Unauthenticated && had_body {
                response.headers_mut().insert(http::header::CONNECTION, HeaderValue::from_static("close"));
            }
            (response, Some(e.code))
        }
    };
    let add_build = code.is_some() || handled.region == Some(Region::Api);
    let (response, response_bytes) = finish(response, &request_id, method == Method::HEAD, add_build, &state.csp);
    let status = response.status().as_u16();
    let label = handled.label;
    let elapsed = started.elapsed();
    state.metrics.http_request(label, status, elapsed);
    span.end(status, code.map(Code::as_str));
    let request_bytes = if handled.request_bytes == 0 { content_length } else { handled.request_bytes };
    state.logger.http_request(&HttpRequestLog {
        request_id: &request_id,
        method: method.as_str(),
        route: label,
        status,
        code: code.map(Code::as_str),
        duration_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        request_bytes,
        response_bytes,
        subject: &handled.subject,
        client: &peer.to_string(),
    });
    conn.request_finished();
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_limits_of_6_27() {
        assert_eq!(body_limit(BodyClass::None, 9), 0);
        assert_eq!(body_limit(BodyClass::Query, 9), 262_144);
        assert_eq!(body_limit(BodyClass::Preview, 9), 524_288);
        assert_eq!(body_limit(BodyClass::Small, 9), 16_384);
        assert_eq!(body_limit(BodyClass::Doc, 8_388_608), 8_388_608);
    }

    #[test]
    fn head_limits() {
        let uri: http::Uri = "/api/session".parse().unwrap();
        let mut h = HeaderMap::new();
        assert!(check_head_limits(&uri, &h).is_ok());
        let long = format!("/{}", "a".repeat(MAX_TARGET_BYTES));
        let uri_long: http::Uri = long.parse().unwrap();
        assert_eq!(check_head_limits(&uri_long, &HeaderMap::new()).unwrap_err().code, Code::UriTooLong);
        let ok = format!("/{}", "a".repeat(MAX_TARGET_BYTES - 1));
        assert!(check_head_limits(&ok.parse().unwrap(), &HeaderMap::new()).is_ok());
        for i in 0..MAX_HEADERS {
            h.insert(
                http::HeaderName::from_bytes(format!("x-h{i}").as_bytes()).unwrap(),
                HeaderValue::from_static("v"),
            );
        }
        assert!(check_head_limits(&uri, &h).is_ok());
        h.insert(http::HeaderName::from_static("x-extra"), HeaderValue::from_static("v"));
        assert_eq!(check_head_limits(&uri, &h).unwrap_err().code, Code::HeadersTooLarge);
        let mut big = HeaderMap::new();
        big.insert(
            http::HeaderName::from_static("x-big"),
            HeaderValue::from_str(&"v".repeat(MAX_HEADER_BYTES)).unwrap(),
        );
        assert_eq!(check_head_limits(&uri, &big).unwrap_err().code, Code::HeadersTooLarge);
    }

    #[test]
    fn refusal_reasons_are_in_the_metric_list() {
        for code in Code::ALL {
            if let Some(r) = refused_reason(code) {
                assert!(crate::metrics::REFUSED_REASONS.contains(&r));
            }
        }
    }
}
