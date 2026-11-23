//! The response type and its builders.

use super::error::ApiError;
use bytes::Bytes;
use http::header::{ALLOW, CACHE_CONTROL, CONNECTION, CONTENT_TYPE, HeaderName, HeaderValue, RETRY_AFTER};
use http::{Response as HttpResponse, StatusCode};
use http_body_util::Full;
use serde::Serialize;

/// The response of a handler: the whole body is in memory.
pub type Response = HttpResponse<Full<Bytes>>;

/// Makes a response with a status, a content type and a body.
pub fn bytes_response(status: StatusCode, content_type: &'static str, body: Bytes) -> Response {
    let mut r = HttpResponse::new(Full::new(body));
    *r.status_mut() = status;
    r.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    r
}

/// Makes a JSON response from a value that serialises.
pub fn json_response<T: Serialize>(status: StatusCode, value: &T) -> Response {
    match serde_json::to_vec(value) {
        Ok(bytes) => bytes_response(status, "application/json", Bytes::from(bytes)),
        Err(_) => error_response(&ApiError::internal()),
    }
}

/// Makes a response with a status and no body.
pub fn empty_response(status: StatusCode) -> Response {
    let mut r = HttpResponse::new(Full::new(Bytes::new()));
    *r.status_mut() = status;
    r
}

/// Makes the answer of an error (6.7), with the headers that its code needs (6.4).
pub fn error_response(e: &ApiError) -> Response {
    let body = serde_json::to_vec(&e.body()).unwrap_or_else(|_| b"{}".to_vec());
    let mut r = bytes_response(e.status(), "application/json", Bytes::from(body));
    let h = r.headers_mut();
    h.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(allow) = &e.allow
        && let Ok(v) = HeaderValue::from_str(allow)
    {
        h.insert(ALLOW, v);
    }
    if let Some(secs) = e.code.retry_after() {
        h.insert(RETRY_AFTER, HeaderValue::from(secs));
    }
    if e.code.closes_connection() {
        h.insert(CONNECTION, HeaderValue::from_static("close"));
    }
    r
}

/// Sets a header whose name is a constant.
pub fn set_header(r: &mut Response, name: HeaderName, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        r.headers_mut().insert(name, v);
    }
}
