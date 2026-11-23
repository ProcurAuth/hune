//! The SERVER span of a request of `server.listen` (14.10).
//!
//! The span has the name `<METHOD> <route template>`. It is a child of a valid incoming
//! `traceparent`. Its attributes are the ones of the table of 14.10: no SQL, no value, no row
//! data, no subject and no secret (12.15). The requests of `server.ops_listen` make no span.

use super::route::{self, Matched, RouteId};
use super::state::AppState;
use crate::telemetry::{Span, Telemetry, TraceContext, Tracing};
use http::{HeaderMap, Method};
use std::net::SocketAddr;

/// The SERVER span of one request, with the context for its CLIENT spans.
pub struct RequestSpan {
    span: Span,
    /// The context that the handlers use to make CLIENT spans. It makes no span when the span is off.
    pub trace: Tracing,
}

/// Reads the header `traceparent`. Zero, two or more headers, or a value that is not valid, give `None`.
fn incoming_parent(headers: &HeaderMap) -> Option<TraceContext> {
    let mut values = headers.get_all("traceparent").iter();
    let (first, rest) = (values.next()?, values.next());
    if rest.is_some() {
        return None;
    }
    TraceContext::parse(first.to_str().ok()?)
}

impl RequestSpan {
    /// A span that makes nothing. The handlers make no CLIENT span.
    pub fn off() -> RequestSpan {
        RequestSpan { span: Telemetry::disabled().span_server("", None), trace: Tracing::default() }
    }

    /// Starts the span of a request. With a disabled telemetry, it costs one test.
    pub fn begin(
        state: &AppState,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
        peer: SocketAddr,
        request_id: &str,
    ) -> RequestSpan {
        if !state.telemetry.is_enabled() {
            return RequestSpan::off();
        }
        let rel = route::strip_base(&state.config.server.base_path, path);
        let matched = rel.map(|rel| route::route(method, rel));
        let (template, uid, panel) = match &matched {
            Some(Matched::Route(spec, params)) => {
                let template = spec.label.split_once(' ').map_or(spec.label, |(_, t)| t);
                let panel = if spec.id == RouteId::PanelQuery { params.get("id") } else { None };
                (Some(template), params.get("uid"), panel)
            }
            _ => (None, None, None),
        };
        let name = match template {
            Some(t) => format!("{method} {t}"),
            None => method.as_str().to_owned(),
        };
        let mut span = state.telemetry.span_server(&name, incoming_parent(headers).as_ref());
        span.set_str("http.request.method", method.as_str());
        if let Some(t) = template {
            span.set_str("http.route", t);
        }
        span.set_str("client.address", &peer.ip().to_string());
        span.set_str("hune.request_id", request_id);
        if let Some(uid) = uid {
            span.set_str("hune.dashboard.uid", uid);
        }
        if let Some(id) = panel.and_then(|p| p.parse::<i64>().ok()) {
            span.set_int("hune.panel.id", id);
        }
        let trace = Tracing::new(state.telemetry.clone(), span.context());
        RequestSpan { span, trace }
    }

    /// Ends the span with the status of the answer. `code` is the error code of the answer, when it
    /// has one. A status of 400 or more sets `error.type`, and a status of 500 or more sets the error status.
    pub fn end(mut self, status: u16, code: Option<&str>) {
        self.span.set_int("http.response.status_code", i64::from(status));
        if status >= 400 {
            match code {
                Some(code) => self.span.set_str("error.type", code),
                None => self.span.set_str("error.type", &status.to_string()),
            }
        }
        if status >= 500 {
            self.span.set_error();
        }
        self.span.end();
    }
}
