//! The context of one request and the request seen by a handler.

use super::query::Query;
use super::route::{Params, RouteSpec};
use super::state::AppState;
use crate::telemetry::Tracing;
use bytes::Bytes;
use http::{HeaderMap, Method};
use hune_core::etag::Etag;
use hune_store::SaveId;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::OwnedSemaphorePermit;

/// What the pipeline learned about the request before the handler runs (6.28, steps 1 to 6).
pub struct Ctx {
    /// The shared state.
    pub state: Arc<AppState>,
    /// The request id (6.3).
    pub request_id: String,
    /// The subject: `-` in mode `open`, and the claim `sub` in mode `authzen`.
    pub subject: String,
    /// The identity of the token in mode `authzen`. It is `None` in mode `open`, and on the action document.
    pub identity: Option<procurauth_pep::Identity>,
    /// The raw query string of the request, for the routes that read it themselves (11.26).
    pub raw_query: Option<String>,
    /// The route that matched.
    pub route: &'static RouteSpec,
    /// The path parameters, as the raw text of the path.
    pub params: Params,
    /// The query parameters, checked against the list of the route.
    pub query: Query,
    /// `If-Match`, when the route needs it and the request is not `validate_only`.
    pub if_match: Option<Etag>,
    /// `If-None-Match`, when the request has it.
    pub if_none_match: Option<Etag>,
    /// `Idempotency-Key`, when the route needs it and the request is not `validate_only`.
    pub save_id: Option<SaveId>,
    /// The address of the TCP peer.
    pub peer: SocketAddr,
    /// The tracing context of the request: it makes the CLIENT spans of 14.10.
    pub trace: Tracing,
}

/// The request as a handler sees it: the body is read already and is within the limit of the route.
pub struct Request {
    /// The method. `HEAD` stays `HEAD`: the pipeline removes the body of the answer.
    pub method: Method,
    /// The headers.
    pub headers: HeaderMap,
    /// The body. It is empty for a route without a body.
    pub body: Bytes,
    /// The place of the body in flight (6.27). The handler can move it into the work on the body.
    pub place: Place,
}

/// The places of a body in flight: the place of 6.27 and, in the mode `authzen`, the place of the subject (12.16).
/// They stay taken while this value or the work that took them lives.
#[derive(Default)]
pub struct Place(Mutex<Option<BodyPlace>>);

/// The permits of one body in flight.
pub struct BodyPlace {
    /// The permit of `MAX_INFLIGHT_DOC_BODIES` or of `MAX_INFLIGHT_SMALL_BODIES`.
    pub body: OwnedSemaphorePermit,
    /// The place of the subject for a document body in the mode `authzen`.
    pub subject: Option<super::state::SubjectSlot>,
}

impl Place {
    /// Holds the permits of a body.
    pub fn new(place: BodyPlace) -> Self {
        Place(Mutex::new(Some(place)))
    }

    /// Takes the permits out, so that the work on the body holds them until it ends.
    pub fn take(&self) -> Option<BodyPlace> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}
