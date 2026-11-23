//! The ops listener (14.2 to 14.4, 6.26): `/livez`, `/readyz` and `/metrics`.

use super::conn::{CloseReason, ConnShared, ConnTimes, run_connection};
use super::error::ApiError;
use super::limits::{OPS_HEADER_TIMEOUT, OPS_MAX_CONNECTIONS};
use super::response::{Response, bytes_response, error_response, json_response};
use super::state::AppState;
use bytes::Bytes;
use http::header::{CACHE_CONTROL, CONTENT_LENGTH, HeaderValue, TRANSFER_ENCODING};
use http::{Method, StatusCode};
use http_body_util::Full;
use hyper::body::Body;
use hyper::body::Incoming;
use serde::Serialize;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tokio::sync::watch;

#[derive(Serialize)]
struct StoreHealth<'a> {
    backend: &'a str,
    state: &'a str,
    catalog_age_seconds: u64,
}

#[derive(Serialize)]
struct Readyz<'a> {
    ready: bool,
    stopping: bool,
    catalog_loaded: bool,
    store: StoreHealth<'a>,
    sources: BTreeMap<String, &'static str>,
    pdp: &'a str,
}

/// Makes the answer of `/readyz` (14.4).
pub fn readyz(state: &AppState) -> Response {
    let ready = state.is_ready();
    let body = Readyz {
        ready,
        stopping: state.stopping(),
        catalog_loaded: state.health.catalog_loaded(),
        store: StoreHealth {
            backend: state.backend_name(),
            state: if state.health.store_up() { "up" } else { "down" },
            catalog_age_seconds: state.health.catalog_age_seconds(),
        },
        sources: state.health.sources(),
        // Mode `open` has no PDP. The mode `authzen` is not built yet.
        pdp: "none",
    };
    json_response(if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }, &body)
}

/// Answers one request of the ops listener.
pub fn ops_response(state: &AppState, method: &Method, path: &str, has_body: bool) -> Response {
    let known = matches!(path, "/livez" | "/readyz" | "/metrics");
    let mut response = if !known {
        error_response(&ApiError::not_found("No such path."))
    } else if *method != Method::GET && *method != Method::HEAD {
        error_response(&ApiError::method_not_allowed("GET, HEAD"))
    } else if has_body {
        error_response(&ApiError::bad_request("This route takes no body."))
    } else {
        match path {
            "/livez" => {
                bytes_response(StatusCode::OK, "application/json", Bytes::from_static(b"{\"status\":\"live\"}"))
            }
            "/readyz" => readyz(state),
            _ => {
                for source in &state.sources {
                    state.metrics.set_source_in_flight(source.id().as_str(), source.admission_stats().in_flight);
                }
                if let Some(first) = state.sources.first() {
                    let bytes = i64::try_from(first.admission_stats().memory_in_use).unwrap_or(i64::MAX);
                    state.metrics.result_memory_reserved(bytes);
                }
                if state.health.catalog_loaded() {
                    state.metrics.store_up(state.health.store_up());
                    state.metrics.store_catalog_age_seconds(state.health.catalog_age_seconds());
                }
                bytes_response(
                    StatusCode::OK,
                    "text/plain; version=0.0.4; charset=utf-8",
                    Bytes::from(state.metrics.render()),
                )
            }
        }
    };
    let h = response.headers_mut();
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(http::header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if *method == Method::HEAD {
        let len = response.body().size_hint().exact().unwrap_or(0);
        response.headers_mut().insert(CONTENT_LENGTH, HeaderValue::from(len));
        *response.body_mut() = Full::new(Bytes::new());
    }
    response
}

/// The accept loop of `server.ops_listen`. It ends when `stop` becomes `true`.
pub async fn accept_loop(listener: TcpListener, state: Arc<AppState>, mut stop: watch::Receiver<bool>) {
    let open = Arc::new(AtomicUsize::new(0));
    let times = ConnTimes { idle: state.config.server.idle_timeout, header: OPS_HEADER_TIMEOUT };
    loop {
        let accepted = tokio::select! {
            r = listener.accept() => r,
            _ = stop.changed() => {
                if *stop.borrow() { return; }
                continue;
            }
        };
        let Ok((stream, _peer)) = accepted else {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };
        if open.fetch_add(1, Ordering::SeqCst) >= OPS_MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            drop(stream);
            continue;
        }
        let open = open.clone();
        let state = state.clone();
        let close_rx = stop.clone();
        tokio::spawn(async move {
            let shared = ConnShared::new();
            let sh = shared.clone();
            let handler = move |req: hyper::Request<Incoming>| {
                let has_body = req.headers().contains_key(TRANSFER_ENCODING)
                    || req
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .is_some_and(|n| n > 0);
                sh.request_started();
                let response = ops_response(&state, req.method(), req.uri().path(), has_body);
                sh.request_finished();
                std::future::ready(Ok::<Response, Infallible>(response))
            };
            let _: CloseReason = run_connection(stream, shared, handler, times, close_rx).await;
            open.fetch_sub(1, Ordering::SeqCst);
        });
    }
}
