// SPDX-License-Identifier: MIT OR Apache-2.0
//! Request and response headers. This module has the request id (6.3, 14.6), the entity tags (6.5)
//! and the `Idempotency-Key` (6.6). It also has the write guard (12.7) and the security headers
//! (12.8, 9.4).

use super::error::{ApiError, Code};
use crate::config::{GEOMAP_IMG_SRC_BASE as IMG_SRC_BASE, GeomapConfig};
use http::header::{HeaderMap, HeaderName, HeaderValue};
use hune_core::etag::Etag;
use hune_store::SaveId;

/// The content security policy of 9.4 and 12.8.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
                       font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; \
                       frame-ancestors 'none'";

pub const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
pub const X_HUNE_BUILD: HeaderName = HeaderName::from_static("x-hune-build");
pub const IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
pub const IDEMPOTENT_REPLAYED: HeaderName = HeaderName::from_static("idempotent-replayed");
pub const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
pub const X_FORWARDED_HOST: HeaderName = HeaderName::from_static("x-forwarded-host");

/// Tells if a request id from a client is valid: 1 to 128 characters of `A-Z a-z 0-9 . _ : -`.
pub fn valid_request_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// Makes a request id: 32 lower-case hexadecimal characters from the random source of the system.
pub fn new_request_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // The random source failed. A time-based value is still unique enough for a log id.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        bytes = t.to_le_bytes();
    }
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Gives the request id of a request: the valid `X-Request-Id`, else a new id (6.3).
pub fn request_id(headers: &HeaderMap) -> String {
    headers
        .get(X_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .filter(|v| valid_request_id(v))
        .map_or_else(new_request_id, str::to_owned)
}

/// Reads one header that must occur one time at most, as text. A repeated or non-text header
/// gives `400 bad_request`.
fn single(headers: &HeaderMap, name: &HeaderName) -> Result<Option<String>, ApiError> {
    let mut all = headers.get_all(name).iter();
    let Some(first) = all.next() else { return Ok(None) };
    if all.next().is_some() {
        return Err(ApiError::bad_request(format!("The header {name} occurs more than one time.")));
    }
    first
        .to_str()
        .map(|s| Some(s.to_owned()))
        .map_err(|_| ApiError::bad_request(format!("The header {name} is not text.")))
}

/// Reads one strong entity tag (6.5): 64 lower-case hexadecimal characters in double quotes.
/// The forms `*`, `W/"..."` and a list give `400 bad_request`.
pub fn parse_strong_etag(header: &str, value: &str) -> Result<Etag, ApiError> {
    let bad = || {
        ApiError::bad_request(format!(
            "The header {header} must hold one strong entity tag: 64 lower-case hexadecimal characters in double quotes."
        ))
    };
    let inner = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or_else(bad)?;
    Etag::parse_hex(inner).map_err(|_| bad())
}

/// Reads `If-Match`. An absent header gives `428 precondition_required` (6.3).
pub fn if_match(headers: &HeaderMap) -> Result<Etag, ApiError> {
    let name = HeaderName::from_static("if-match");
    match single(headers, &name)? {
        None => Err(ApiError::precondition_required("If-Match")),
        Some(v) => parse_strong_etag("If-Match", &v),
    }
}

/// Reads `If-None-Match`. The header is optional (6.3).
pub fn if_none_match(headers: &HeaderMap) -> Result<Option<Etag>, ApiError> {
    let name = HeaderName::from_static("if-none-match");
    single(headers, &name)?.map(|v| parse_strong_etag("If-None-Match", &v)).transpose()
}

/// Reads `Idempotency-Key`: a UUID in the canonical text form (6.6). An absent header gives
/// `428 precondition_required`.
pub fn idempotency_key(headers: &HeaderMap) -> Result<SaveId, ApiError> {
    match single(headers, &IDEMPOTENCY_KEY)? {
        None => Err(ApiError::precondition_required("Idempotency-Key")),
        Some(v) => SaveId::parse(&v).map_err(|_| {
            ApiError::bad_request(
                "The header Idempotency-Key must hold a UUID: 36 lower-case hexadecimal characters with four '-'.",
            )
        }),
    }
}

/// Makes the value of the header `ETag` from an etag: the hexadecimal text in double quotes (6.5).
pub fn etag_header_value(etag: &Etag) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{}\"", etag.to_hex())).unwrap_or_else(|_| HeaderValue::from_static("\"\""))
}

/// Tells if a `Content-Type` is `application/json` with an optional `charset=utf-8` (6.3, 12.7).
pub fn is_json_content_type(value: &str) -> bool {
    let mut parts = value.split(';');
    let media = parts.next().unwrap_or("").trim();
    if !media.eq_ignore_ascii_case("application/json") {
        return false;
    }
    let mut charset_seen = false;
    for param in parts {
        let param = param.trim();
        let Some((name, val)) = param.split_once('=') else { return false };
        let val = val.trim().trim_matches('"');
        if charset_seen || !name.trim().eq_ignore_ascii_case("charset") || !val.eq_ignore_ascii_case("utf-8") {
            return false;
        }
        charset_seen = true;
    }
    true
}

/// Checks the `Content-Type` of a request with a body: `415 unsupported_media_type` when it is
/// absent or not JSON.
pub fn check_content_type(headers: &HeaderMap) -> Result<(), ApiError> {
    let ok = headers.get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(is_json_content_type);
    if ok {
        Ok(())
    } else {
        Err(ApiError::new(Code::UnsupportedMediaType, "The body must have the media type application/json."))
    }
}

fn split_host_port(text: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = text.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port = match tail {
            "" => None,
            t => Some(t.strip_prefix(':')?.parse::<u16>().ok()?),
        };
        return Some((format!("[{}]", host.to_ascii_lowercase()), port));
    }
    match text.rsplit_once(':') {
        Some((host, port)) => {
            if host.contains(':') || host.is_empty() {
                return None;
            }
            Some((host.to_ascii_lowercase(), Some(port.parse::<u16>().ok()?)))
        }
        None if text.is_empty() => None,
        None => Some((text.to_ascii_lowercase(), None)),
    }
}

/// Compares the host and port of `Origin` with the host header (12.7, rule 2 and 3).
fn origin_matches_host(origin: &str, host: &str) -> bool {
    let (scheme_port, rest) = if let Some(r) = origin.strip_prefix("http://") {
        (80u16, r)
    } else if let Some(r) = origin.strip_prefix("https://") {
        (443u16, r)
    } else {
        return false;
    };
    if rest.contains(['/', '?', '#', '@']) {
        return false;
    }
    let (Some((oh, op)), Some((hh, hp))) = (split_host_port(rest), split_host_port(host.trim())) else {
        return false;
    };
    oh == hh && op.unwrap_or(scheme_port) == hp.unwrap_or(scheme_port)
}

/// The cross-site guard of 12.7 for `POST`, `PUT`, `PATCH` and `DELETE`: `403 cross_site_request`
/// when the request comes from another site.
pub fn cross_site_guard(headers: &HeaderMap) -> Result<(), ApiError> {
    let refuse = || ApiError::new(Code::CrossSiteRequest, "The request comes from another site.");
    if let Some(value) = headers.get(SEC_FETCH_SITE) {
        let ok = value
            .to_str()
            .is_ok_and(|v| v.trim().eq_ignore_ascii_case("same-origin") || v.trim().eq_ignore_ascii_case("none"));
        return if ok { Ok(()) } else { Err(refuse()) };
    }
    let mut origins = headers.get_all(http::header::ORIGIN).iter();
    let Some(origin) = origins.next() else { return Ok(()) };
    if origins.next().is_some() {
        return Err(refuse());
    }
    let Ok(origin) = origin.to_str() else { return Err(refuse()) };
    let host = headers
        .get(X_FORWARDED_HOST)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or("").to_owned())
        .or_else(|| headers.get(http::header::HOST).and_then(|v| v.to_str().ok()).map(str::to_owned));
    match host {
        Some(host) if origin_matches_host(origin.trim(), &host) => Ok(()),
        _ => Err(refuse()),
    }
}

/// The content security policy with the tile source of the block `geomap` (12.8, 13.12c).
///
/// The source comes from the parsed parts of the template only, after `'self' data:` of `img-src`. No other directive
/// changes: `connect-src` stays `'self'`, because the page loads a tile as an image. Without the block, the result is
/// [`CSP`], byte for byte.
pub fn csp_header(geomap: Option<&GeomapConfig>) -> String {
    match geomap {
        None => CSP.to_owned(),
        Some(g) => CSP.replacen(&format!("img-src {IMG_SRC_BASE};"), &format!("img-src {};", g.img_src()), 1),
    }
}

/// The value of the header of [`csp_header`]. The server builds it one time at start (`AppState::new`). A value that
/// is not a valid header, which the grammar of 13.13 excludes, gives [`CSP`]: no tile host (fail closed).
pub fn csp_header_value(geomap: Option<&GeomapConfig>) -> HeaderValue {
    HeaderValue::from_str(&csp_header(geomap)).unwrap_or_else(|_| HeaderValue::from_static(CSP))
}

/// Adds the security headers of 12.8 to a header map, with the policy of 12.8 with no tile source. The refusal of 6.27
/// uses it, because Hune writes that answer before it reads a request.
pub fn add_security_headers(headers: &mut HeaderMap) {
    add_security_headers_with(headers, &HeaderValue::from_static(CSP));
}

/// Adds the security headers of 12.8 to a header map, with the policy that the server built at start.
pub fn add_security_headers_with(headers: &mut HeaderMap, csp: &HeaderValue) {
    headers.insert(HeaderName::from_static("content-security-policy"), csp.clone());
    let pairs: [(&'static str, &'static str); 5] = [
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("cross-origin-opener-policy", "same-origin"),
    ];
    for (name, value) in pairs {
        headers.insert(HeaderName::from_static(name), HeaderValue::from_static(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.append(HeaderName::from_bytes(k.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
        }
        m
    }

    const E: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn request_id_rule() {
        assert!(valid_request_id("abc.DEF_1:2-3"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id(&"a".repeat(129)));
        assert!(valid_request_id(&"a".repeat(128)));
        assert!(!valid_request_id("a b"));
        assert_eq!(request_id(&map(&[("x-request-id", "my-id")])), "my-id");
        let fresh = request_id(&map(&[("x-request-id", "bad id!")]));
        assert_eq!(fresh.len(), 32);
        assert!(fresh.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert_ne!(new_request_id(), new_request_id());
    }

    #[test]
    fn if_match_forms() {
        assert_eq!(if_match(&map(&[("if-match", &format!("\"{E}\""))])).unwrap().to_hex(), E);
        let absent = if_match(&map(&[])).unwrap_err();
        assert_eq!(absent.code, Code::PreconditionRequired);
        assert_eq!(absent.body()["error"]["header"], "If-Match");
        for bad in [
            "*".to_owned(),
            format!("W/\"{E}\""),
            format!("\"{E}\", \"{E}\""),
            E.to_owned(),
            format!("\"{}\"", E.to_uppercase()),
            "\"abc\"".to_owned(),
            String::new(),
        ] {
            let err = if_match(&map(&[("if-match", &bad)])).unwrap_err();
            assert_eq!(err.code, Code::BadRequest, "{bad}");
        }
        let twice = map(&[("if-match", &format!("\"{E}\"")), ("if-match", &format!("\"{E}\""))]);
        assert_eq!(if_match(&twice).unwrap_err().code, Code::BadRequest);
    }

    #[test]
    fn if_none_match_is_optional() {
        assert!(if_none_match(&map(&[])).unwrap().is_none());
        assert!(if_none_match(&map(&[("if-none-match", &format!("\"{E}\""))])).unwrap().is_some());
        assert_eq!(if_none_match(&map(&[("if-none-match", "*")])).unwrap_err().code, Code::BadRequest);
    }

    #[test]
    fn idempotency_key_forms() {
        let ok = "0194b3c6-6f1e-7d2a-9b4c-1e2f3a4b5c6d";
        assert_eq!(idempotency_key(&map(&[("idempotency-key", ok)])).unwrap().to_string(), ok);
        assert_eq!(idempotency_key(&map(&[])).unwrap_err().code, Code::PreconditionRequired);
        for bad in ["0194B3C6-6F1E-7D2A-9B4C-1E2F3A4B5C6D", "abc", "0194b3c66f1e7d2a9b4c1e2f3a4b5c6d", ""] {
            assert_eq!(idempotency_key(&map(&[("idempotency-key", bad)])).unwrap_err().code, Code::BadRequest, "{bad}");
        }
    }

    #[test]
    fn content_type_forms() {
        for ok in [
            "application/json",
            "application/json; charset=utf-8",
            "Application/JSON;charset=UTF-8",
            "application/json; charset=\"utf-8\"",
        ] {
            assert!(is_json_content_type(ok), "{ok}");
        }
        for bad in [
            "text/plain",
            "application/json; charset=latin1",
            "application/json; boundary=x",
            "application/jsonx",
            "application/json; charset=utf-8; charset=utf-8",
            "",
        ] {
            assert!(!is_json_content_type(bad), "{bad}");
        }
        assert_eq!(check_content_type(&map(&[])).unwrap_err().code, Code::UnsupportedMediaType);
        assert!(check_content_type(&map(&[("content-type", "application/json")])).is_ok());
    }

    #[test]
    fn write_guard_matrix() {
        let cases: &[(&[(&str, &str)], bool)] = &[
            (&[], true),
            (&[("sec-fetch-site", "same-origin")], true),
            (&[("sec-fetch-site", "none")], true),
            (&[("sec-fetch-site", "cross-site")], false),
            (&[("sec-fetch-site", "same-site")], false),
            // Sec-Fetch-Site decides when it is present.
            (&[("sec-fetch-site", "same-origin"), ("origin", "https://evil.example")], true),
            (&[("sec-fetch-site", "cross-site"), ("origin", "http://hune.example"), ("host", "hune.example")], false),
            (&[("origin", "http://hune.example"), ("host", "hune.example")], true),
            (&[("origin", "http://hune.example:8080"), ("host", "hune.example:8080")], true),
            (&[("origin", "http://hune.example:8080"), ("host", "hune.example")], false),
            (&[("origin", "https://hune.example"), ("host", "hune.example:443")], true),
            (&[("origin", "http://hune.example"), ("host", "hune.example:80")], true),
            (&[("origin", "https://hune.example"), ("host", "hune.example")], true),
            (&[("origin", "https://hune.example"), ("host", "hune.example:80")], false),
            (&[("origin", "null"), ("host", "hune.example")], false),
            (&[("origin", "https://evil.example"), ("host", "hune.example")], false),
            (&[("origin", "https://Hune.Example"), ("host", "hune.example")], true),
            (&[("origin", "http://[::1]:8080"), ("host", "[::1]:8080")], true),
            (&[("origin", "http://hune.example")], false),
            (&[("origin", "https://hune.example/path"), ("host", "hune.example")], false),
            (&[("origin", "ftp://hune.example"), ("host", "hune.example")], false),
            (
                &[
                    ("origin", "https://public.example"),
                    ("host", "internal:8080"),
                    ("x-forwarded-host", "public.example"),
                ],
                true,
            ),
            (
                &[
                    ("origin", "https://public.example"),
                    ("host", "public.example"),
                    ("x-forwarded-host", "other.example, public.example"),
                ],
                false,
            ),
            (&[("origin", "https://a.example"), ("origin", "https://a.example"), ("host", "a.example")], false),
        ];
        for (pairs, ok) in cases {
            let result = cross_site_guard(&map(pairs));
            assert_eq!(result.is_ok(), *ok, "{pairs:?}");
            if let Err(e) = result {
                assert_eq!(e.code, Code::CrossSiteRequest);
            }
        }
    }

    #[test]
    fn the_policy_without_the_block_is_the_policy_of_12_8_byte_for_byte() {
        // The text of 9.4 and 12.8, as the specification writes it.
        let spec = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";
        assert_eq!(CSP, spec);
        assert_eq!(csp_header(None), spec);
        assert_eq!(csp_header_value(None), spec);
        assert!(CSP.contains(&format!("img-src {IMG_SRC_BASE};")));
    }

    #[test]
    fn the_block_adds_one_source_to_img_src_only() {
        let g = GeomapConfig {
            tile_url: crate::config::parse_tile_url("https://tiles.example.org:8443/osm/{z}/{x}/{y}.png?key=a&b=c")
                .unwrap(),
            attribution: "x".to_owned(),
            max_zoom: 3,
        };
        let csp = csp_header(Some(&g));
        assert_eq!(
            csp,
            CSP.replace("img-src 'self' data:;", "img-src 'self' data: https://tiles.example.org:8443/osm/;")
        );
        let mut h = HeaderMap::new();
        add_security_headers_with(&mut h, &csp_header_value(Some(&g)));
        assert_eq!(h["content-security-policy"], csp.as_str());
        assert_eq!(h["x-frame-options"], "DENY");
        assert_eq!(h.len(), 6);
    }

    #[test]
    fn security_headers_are_complete() {
        let mut h = HeaderMap::new();
        add_security_headers(&mut h);
        assert_eq!(h["content-security-policy"], CSP);
        assert!(!CSP.contains("wasm-unsafe-eval") && !CSP.contains("script-src 'self' 'unsafe-inline'"));
        assert_eq!(h["x-content-type-options"], "nosniff");
        assert_eq!(h["x-frame-options"], "DENY");
        assert_eq!(h["referrer-policy"], "same-origin");
        assert_eq!(h["cross-origin-resource-policy"], "same-origin");
        assert_eq!(h["cross-origin-opener-policy"], "same-origin");
        assert!(h.keys().all(|k| !k.as_str().starts_with("access-control-")));
    }

    #[test]
    fn etag_header_is_quoted() {
        let e = Etag::parse_hex(E).unwrap();
        assert_eq!(etag_header_value(&e), format!("\"{E}\""));
    }
}
