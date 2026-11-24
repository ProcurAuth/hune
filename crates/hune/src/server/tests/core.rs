// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the HTTP core: pages, static files, headers, limits, the write guard, the ops
//! listener, the connection rules and the graceful stop.

use super::harness::*;
use crate::server::limits::Timing;
use crate::server::state::Phase;
use crate::web_assets::{self, BUILD_HASH};
use std::time::Duration;

fn security_headers_ok(r: &Resp) {
    assert_eq!(r.header("content-security-policy"), Some(crate::server::headers::CSP));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(r.header("x-frame-options"), Some("DENY"));
    assert_eq!(r.header("referrer-policy"), Some("same-origin"));
    assert_eq!(r.header("cross-origin-resource-policy"), Some("same-origin"));
    assert_eq!(r.header("cross-origin-opener-policy"), Some("same-origin"));
    assert!(r.headers.iter().all(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-")));
    let id = r.header("x-request-id").expect("x-request-id");
    assert!(!id.is_empty());
    assert!(r.header("set-cookie").is_none());
}

#[tokio::test]
async fn session_in_mode_open() {
    let h = Harness::start(|_| {}).await;
    let r = get(h.addr, "/api/session").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.header("cache-control"), Some("no-store"));
    assert_eq!(r.header("x-hune-build"), Some(BUILD_HASH));
    security_headers_ok(&r);
    let j = r.json();
    assert_eq!(j["mode"], "open");
    assert_eq!(j["subject"], "-");
    assert_eq!(j["editing"], true);
    assert_eq!(j["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(j["build"], BUILD_HASH);
    assert_eq!(j["refresh_choices"], serde_json::json!(["10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"]));
    assert_eq!(j["limits"]["document_bytes"], 4_194_304);
    assert_eq!(j["limits"]["query_body_bytes"], 262_144);
    assert_eq!(j["limits"]["max_data_points"], 10_000);
    let keys: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys.len(), 11);
    // The block `geomap` is absent: the member is `null` (6.10, 13.12c).
    assert_eq!(j["geomap"], serde_json::Value::Null);
    assert_eq!(j["query_inspect"], false);
    // The repeat limits (6.10): the defaults of the three keys, a closed object of three members.
    assert_eq!(j["repeat_limits"], serde_json::json!({"panel": 24, "section": 12, "dashboard": 120}));
    // The cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
    assert_eq!(j["transform_max_cells"], 10_000_000);
}

#[tokio::test]
async fn session_gives_the_cell_cap_of_the_configuration() {
    // The value of the typed configuration goes to the page as it is: the bound, the lowest value and a value between.
    for cells in [100_000_000_u32, 1, 2_500_000] {
        let h = Harness::start(|c| c.server.transform_max_cells = cells).await;
        let j = get(h.addr, "/api/session").await.json();
        assert_eq!(j["transform_max_cells"], cells, "{cells}");
        assert!(j["transform_max_cells"].is_u64());
        assert!(j["limits"].get("transform_max_cells").is_none());
    }
}

#[tokio::test]
async fn session_gives_the_repeat_limits_of_the_configuration() {
    let h = Harness::start(|c| {
        c.server.repeat_max_per_panel = 7;
        c.server.repeat_max_per_section = 3;
        c.server.repeat_max_per_dashboard = 50;
    })
    .await;
    let j = get(h.addr, "/api/session").await.json();
    assert_eq!(j["repeat_limits"], serde_json::json!({"panel": 7, "section": 3, "dashboard": 50}));
    assert_eq!(j["repeat_limits"].as_object().unwrap().len(), 3);
}

#[tokio::test]
async fn session_editing_follows_the_configuration() {
    let h = Harness::start(|c| c.server.editing = false).await;
    assert_eq!(get(h.addr, "/api/session").await.json()["editing"], false);
    let h = Harness::start(|c| {
        if let crate::config::StorageConfig::Volume(v) = &mut c.storage {
            v.read_only = true;
        }
    })
    .await;
    assert_eq!(get(h.addr, "/api/session").await.json()["editing"], false);
}

#[tokio::test]
async fn request_id_is_kept_or_made() {
    let h = Harness::start(|_| {}).await;
    let r = call(h.addr, "GET", "/api/session", &[("X-Request-Id", "client-id.1")], b"").await;
    assert_eq!(r.header("x-request-id"), Some("client-id.1"));
    let r = call(h.addr, "GET", "/api/session", &[("X-Request-Id", "bad id!")], b"").await;
    let id = r.header("x-request-id").unwrap();
    assert_eq!(id.len(), 32);
    let r2 = get(h.addr, "/api/session").await;
    assert_ne!(r2.header("x-request-id"), r.header("x-request-id"));
}

#[tokio::test]
async fn shell_pages_have_markers_replaced() {
    let h = Harness::start(|_| {}).await;
    for path in ["/", "/d/nodes", "/d/nodes/my-slug"] {
        let r = get(h.addr, path).await;
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));
        assert_eq!(r.header("cache-control"), Some("no-cache"));
        security_headers_ok(&r);
        let text = r.text();
        assert!(!text.contains("{{base_path}}") && !text.contains("{{build}}"));
        assert!(text.contains(BUILD_HASH));
        assert!(text.contains("<base href=\"/\">"));
    }
    // HEAD gives the same headers and no body.
    let get_r = get(h.addr, "/").await;
    let head = call(h.addr, "HEAD", "/", &[], b"").await;
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    assert_eq!(head.header("content-length"), get_r.header("content-length"));
    assert_eq!(head.header("cache-control"), Some("no-cache"));
}

#[tokio::test]
async fn shell_with_a_base_path_and_escape() {
    let h = Harness::start(|c| c.server.base_path = "/dash&board/".to_owned()).await;
    let r = get(h.addr, "/dash&board/").await;
    assert_eq!(r.status, 200);
    assert!(r.text().contains("<base href=\"/dash&amp;board/\">"));
    assert_eq!(get(h.addr, "/dash&board/api/session").await.status, 200);
    assert_eq!(get(h.addr, "/dash&board/d/abc/slug").await.status, 200);
    // A path outside the base path is a JSON 404.
    for path in ["/", "/api/session", "/dash&board", "/other/"] {
        let r = get(h.addr, path).await;
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.code(), "not_found");
    }
}

#[tokio::test]
async fn unknown_paths_by_region() {
    let h = Harness::start(|_| {}).await;
    for path in ["/nothing", "/d/BAD_UID", "/d/abc/", "/d/abc/x/y", "/favicon.ico"] {
        let r = get(h.addr, path).await;
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"), "{path}");
        assert_eq!(r.header("cache-control"), Some("no-cache"));
        assert!(r.text().contains("<base href"));
    }
    let long_slug = format!("/d/abc/{}", "s".repeat(257));
    assert_eq!(get(h.addr, &long_slug).await.status, 404);
    assert_eq!(get(h.addr, &format!("/d/abc/{}", "s".repeat(256))).await.status, 200);
    for path in [
        "/api/nothing",
        "/api/dashboards/",
        "/static/abc",
        "/static/abcdefabcdefabcd/js/main.js",
        "/.well-known/other",
        "/.well-known/procurauth/actions",
        "/.well-known/procurauth/resources",
    ] {
        let r = get(h.addr, path).await;
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.header("content-type"), Some("application/json"), "{path}");
        assert_eq!(r.code(), "not_found");
        assert_eq!(r.header("cache-control"), Some("no-store"));
        security_headers_ok(&r);
    }
    // The ProcurAuth routes ignore the query parameters in mode open.
    assert_eq!(get(h.addr, "/.well-known/procurauth/resources?type=x").await.status, 404);
    // A POST to a path that no route names is a JSON 404.
    let r = call(h.addr, "POST", "/nothing", &[("Content-Type", "application/json")], b"{}").await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
}

#[tokio::test]
async fn static_files() {
    let h = Harness::start(|_| {}).await;
    let file = web_assets::FILES.iter().find(|f| f.path.ends_with(".js")).expect("a js file in web/");
    let r = get(h.addr, &format!("/static/{BUILD_HASH}/{}", file.path)).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some(file.content_type));
    assert_eq!(r.header("cache-control"), Some("private, max-age=31536000, immutable"));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(r.body, file.bytes);
    security_headers_ok(&r);
    // Every embedded file is served with its content type, except the shell template.
    for f in web_assets::FILES.iter().filter(|f| f.path != "shell.html") {
        let r = get(h.addr, &format!("/static/{BUILD_HASH}/{}", f.path)).await;
        assert_eq!(r.status, 200, "{}", f.path);
        assert_eq!(r.header("content-type"), Some(f.content_type), "{}", f.path);
        assert_eq!(r.body.len(), f.bytes.len());
    }
    let head = call(h.addr, "HEAD", &format!("/static/{BUILD_HASH}/{}", file.path), &[], b"").await;
    assert!(head.body.is_empty());
    assert_eq!(head.header("content-length"), Some(file.bytes.len().to_string().as_str()));
    // A wrong hash, the shell template and a path outside the table give 404.
    for path in [
        format!("/static/0000000000000000/{}", file.path),
        format!("/static/{BUILD_HASH}/shell.html"),
        format!("/static/{BUILD_HASH}/../Cargo.toml"),
        format!("/static/{BUILD_HASH}/%2e%2e/Cargo.toml"),
        format!("/static/{BUILD_HASH}/js/"),
        format!("/static/{BUILD_HASH}/absent.js"),
    ] {
        let r = get(h.addr, &path).await;
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.code(), "not_found");
    }
}

#[tokio::test]
async fn methods_405_head_options() {
    let h = Harness::start(|_| {}).await;
    let r = call(h.addr, "DELETE", "/api/session", &[], b"").await;
    assert_eq!(r.status, 405);
    assert_eq!(r.code(), "method_not_allowed");
    assert_eq!(r.header("allow"), Some("GET, HEAD"));
    security_headers_ok(&r);
    let r = call(
        h.addr,
        "OPTIONS",
        "/api/session",
        &[("Origin", "https://evil.example"), ("Access-Control-Request-Method", "GET")],
        b"",
    )
    .await;
    assert_eq!(r.status, 405);
    assert!(r.header("access-control-allow-origin").is_none());
    let r = call(h.addr, "OPTIONS", "/", &[], b"").await;
    assert_eq!(r.status, 405);
    let r = call(h.addr, "OPTIONS", "/api/nothing", &[], b"").await;
    assert_eq!(r.status, 405);
    // No route takes PATCH: 405 at the route step, before the write guard.
    let r = call(h.addr, "PATCH", "/api/folders", &[("Sec-Fetch-Site", "cross-site")], b"").await;
    assert_eq!(r.status, 405);
    let r = call(h.addr, "PATCH", "/api/session", &[], b"").await;
    assert_eq!((r.status, r.header("allow")), (405, Some("GET, HEAD")));
    let r = call(h.addr, "GET", "/api/sources/prod/preview", &[], b"").await;
    assert_eq!((r.status, r.header("allow")), (405, Some("POST")));
    let r = call(h.addr, "PUT", "/api/folders", &[], b"").await;
    assert_eq!((r.status, r.header("allow")), (405, Some("GET, HEAD, POST")));
    // HEAD of an API route: same status and headers, no body.
    let g = get(h.addr, "/api/session").await;
    let head = call(h.addr, "HEAD", "/api/session", &[], b"").await;
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    assert_eq!(head.header("content-length"), g.header("content-length"));
    assert_eq!(head.header("x-hune-build"), Some(BUILD_HASH));
    security_headers_ok(&head);
    // HEAD of an error has the status and no body.
    let head = call(h.addr, "HEAD", "/api/nothing", &[], b"").await;
    assert_eq!(head.status, 404);
    assert!(head.body.is_empty());
}

#[tokio::test]
async fn strict_query_parameters() {
    let h = Harness::start(|_| {}).await;
    for path in ["/api/session?x=1", "/api/session?a=1&a=2", "/api/folders?limit=1", "/api/session?%zz=1"] {
        let r = get(h.addr, path).await;
        assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{path}");
    }
    assert_eq!(get(h.addr, "/api/session?").await.status, 200);
    // The page routes ignore unknown parameters (6.2).
    assert_eq!(get(h.addr, "/?from=now-1h&x=1&x=2").await.status, 200);
    assert_eq!(get(h.addr, "/d/abc?var-a=1&var-a=2").await.status, 200);
}

#[tokio::test]
async fn write_guard_matrix_over_http() {
    let h = Harness::start(|_| {}).await;
    let json = ("Content-Type", "application/json");
    let body = br#"{"id":"x","title":"X"}"#;
    // Cross-site is refused before the content type is checked.
    let r =
        call(h.addr, "POST", "/api/folders", &[("Sec-Fetch-Site", "cross-site"), ("Content-Type", "text/plain")], body)
            .await;
    assert_eq!((r.status, r.code().as_str()), (403, "cross_site_request"));
    let r = call(h.addr, "POST", "/api/folders", &[("Origin", "https://evil.example"), json], body).await;
    assert_eq!((r.status, r.code().as_str()), (403, "cross_site_request"));
    let r = call(h.addr, "POST", "/api/folders", &[("Origin", "null"), json], body).await;
    assert_eq!(r.code(), "cross_site_request");
    // The guard also applies to the query routes (they use POST).
    let r = call(h.addr, "POST", "/api/dashboards/abc/panels/1/query", &[("Sec-Fetch-Site", "same-site"), json], b"{}")
        .await;
    assert_eq!(r.code(), "cross_site_request");
    let r = call(h.addr, "DELETE", "/api/folders/x", &[("Sec-Fetch-Site", "cross-site")], b"").await;
    assert_eq!(r.code(), "cross_site_request");
    // A wrong content type is 415.
    let r = call(h.addr, "POST", "/api/folders", &[("Content-Type", "text/plain")], body).await;
    assert_eq!((r.status, r.code().as_str()), (415, "unsupported_media_type"));
    let r = call(h.addr, "POST", "/api/folders", &[], body).await;
    assert_eq!(r.status, 415);
    let r = call(h.addr, "POST", "/api/folders", &[("Content-Type", "application/json; charset=latin1")], body).await;
    assert_eq!(r.status, 415);
    // Same-origin passes the guard: the Origin host equals the Host header.
    let r = call(h.addr, "POST", "/api/folders", &[("Origin", "http://localhost"), json], b"{").await;
    assert_ne!(r.code(), "cross_site_request");
    let r = call(h.addr, "POST", "/api/folders", &[("Sec-Fetch-Site", "same-origin"), json], b"{").await;
    assert_ne!(r.code(), "cross_site_request");
    // The metrics count the refusals.
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_http_refused_total{reason=\"cross_site_request\"} 5"), "{text}");
    assert!(text.contains("hune_http_refused_total{reason=\"unsupported_media_type\"} 3"), "{text}");
}

#[tokio::test]
async fn editing_disabled_on_write_routes_only() {
    let h = Harness::start(|c| c.server.editing = false).await;
    let json = ("Content-Type", "application/json");
    let writes: &[(&str, &str)] = &[
        ("POST", "/api/folders"),
        ("PUT", "/api/folders/x"),
        ("DELETE", "/api/folders/x"),
        ("POST", "/api/dashboards"),
        ("PUT", "/api/dashboards/abc"),
        ("DELETE", "/api/dashboards/abc"),
        ("POST", "/api/dashboards/abc/restore"),
        ("POST", "/api/dashboards/abc/move"),
        ("POST", "/api/sources/prod/preview"),
    ];
    for (m, p) in writes {
        let r = call(h.addr, m, p, &[json], if *m == "DELETE" { b"" } else { b"{}" }).await;
        assert_eq!((r.status, r.code().as_str()), (403, "editing_disabled"), "{m} {p}");
    }
    // The query and options routes use POST but are reads: the route is reached (500, or an error of the route).
    let r = call(h.addr, "POST", "/api/dashboards/abc/panels/1/query", &[json], b"{}").await;
    assert_ne!(r.code(), "editing_disabled");
    let r = call(h.addr, "POST", "/api/dashboards/abc/variables/v/options", &[json], b"{}").await;
    assert_ne!(r.code(), "editing_disabled");
    // The cross-site guard comes before the `server.editing` check (6.28 steps 4 and 6).
    let r = call(h.addr, "POST", "/api/folders", &[("Sec-Fetch-Site", "cross-site"), json], b"{}").await;
    assert_eq!(r.code(), "cross_site_request");
}

#[tokio::test]
async fn body_and_header_limits() {
    let h = Harness::start(|_| {}).await;
    let json = ("Content-Type", "application/json");
    // Content-Length above the limit of the route class (16 KiB for a folder route).
    let r = call(h.addr, "POST", "/api/folders", &[json, ("Content-Length", "16385")], b"").await;
    assert_eq!((r.status, r.code().as_str()), (413, "payload_too_large"));
    assert_eq!(r.json()["error"]["limit"], 16384);
    let r =
        call(h.addr, "POST", "/api/dashboards/abc/panels/1/query", &[json, ("Content-Length", "262145")], b"").await;
    assert_eq!((r.status, r.json()["error"]["limit"].as_u64()), (413, Some(262_144)));
    let r = call(h.addr, "POST", "/api/sources/prod/preview", &[json, ("Content-Length", "524289")], b"").await;
    assert_eq!((r.status, r.json()["error"]["limit"].as_u64()), (413, Some(524_288)));
    let r = call(h.addr, "POST", "/api/dashboards", &[json, ("Content-Length", "8388609")], b"").await;
    assert_eq!((r.status, r.json()["error"]["limit"].as_u64()), (413, Some(8_388_608)));
    // A chunked body is counted and stops at the first byte past the limit.
    let mut c = Client::connect(h.addr).await;
    let mut req = b"POST /api/folders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
    let chunk = vec![b'a'; 4096];
    for _ in 0..5 {
        req.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        req.extend_from_slice(&chunk);
        req.extend_from_slice(b"\r\n");
    }
    req.extend_from_slice(b"0\r\n\r\n");
    c.send(&req).await;
    let r = c.read(false).await;
    assert_eq!((r.status, r.code().as_str()), (413, "payload_too_large"));
    // A body on a GET or a DELETE is 400.
    let r = call(h.addr, "GET", "/api/session", &[("Content-Length", "2")], b"{}").await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    let r = call(h.addr, "DELETE", "/api/folders/x", &[json], b"{}").await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    // A request target of 8 KiB is the limit.
    let ok = format!("/api/session?{}", "a".repeat(8192 - "/api/session?".len()));
    let r = get(h.addr, &ok).await;
    assert_eq!(r.status, 400, "the parameter is unknown but the target passes the limit");
    let long = format!("/api/session?{}", "a".repeat(8192));
    let r = get(h.addr, &long).await;
    assert_eq!((r.status, r.code().as_str()), (414, "uri_too_long"));
    // More than 100 headers, and more than 16 KiB of headers.
    let many: Vec<(String, String)> = (0..101).map(|i| (format!("X-H{i}"), "v".to_owned())).collect();
    let refs: Vec<(&str, &str)> = many.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let r = call(h.addr, "GET", "/api/session", &refs, b"").await;
    assert_eq!((r.status, r.code().as_str()), (431, "headers_too_large"));
    let big = "v".repeat(17 * 1024);
    let r = call(h.addr, "GET", "/api/session", &[("X-Big", big.as_str())], b"").await;
    assert_eq!((r.status, r.code().as_str()), (431, "headers_too_large"));
    let few: Vec<(String, String)> = (0..95).map(|i| (format!("X-H{i}"), "v".to_owned())).collect();
    let refs: Vec<(&str, &str)> = few.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(call(h.addr, "GET", "/api/session", &refs, b"").await.status, 200);
    let text = get(h.ops, "/metrics").await.text();
    for reason in ["uri_too_long", "headers_too_large", "payload_too_large"] {
        assert!(!text.contains(&format!("hune_http_refused_total{{reason=\"{reason}\"}} 0")), "{reason}: {text}");
    }
}

#[tokio::test]
async fn body_timeout_gives_408_and_closes() {
    let timing = Timing { body_timeout: Duration::from_millis(300), ..test_timing() };
    let h = Harness::start_with(timing, |_| {}, |_| {}).await;
    let mut c = Client::connect(h.addr).await;
    c.send(b"POST /api/folders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{\"id\"").await;
    let r = c.read(false).await;
    assert_eq!((r.status, r.code().as_str()), (408, "request_timeout"));
    assert_eq!(r.header("connection"), Some("close"));
    assert!(c.closed_within(Duration::from_secs(2)).await);
}

#[tokio::test]
async fn idle_and_header_timeouts_close_the_connection() {
    let timing = Timing { header_timeout: Duration::from_millis(300), ..test_timing() };
    let h = Harness::start_with(timing, |_| {}, |c| c.server.idle_timeout = Duration::from_millis(400)).await;
    // A kept connection is closed after the idle time.
    let mut c = Client::connect(h.addr).await;
    c.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(c.read(false).await.status, 200);
    assert!(!c.closed_within(Duration::from_millis(100)).await, "still open before the idle time");
    assert!(c.closed_within(Duration::from_secs(3)).await, "closed after the idle time");
    // A head that does not end is closed after the header time. No answer comes.
    let mut c = Client::connect(h.addr).await;
    c.send(b"GET /api/session HTTP/1.1\r\nHost: loc").await;
    assert!(c.closed_within(Duration::from_secs(3)).await);
    // A connection that sends nothing is closed after the header time too.
    let mut c = Client::connect(h.addr).await;
    assert!(c.closed_within(Duration::from_secs(3)).await);
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_http_refused_total{reason=\"header_timeout\"} 2"), "{text}");
}

#[tokio::test]
async fn keep_alive_serves_several_requests() {
    let h = Harness::start(|_| {}).await;
    let mut c = Client::connect(h.addr).await;
    for _ in 0..3 {
        c.send(&request("GET", "/api/session", &[], b"")).await;
        assert_eq!(c.read(false).await.status, 200);
    }
    // Two requests in one write (a pipeline of requests).
    let mut two = request("GET", "/api/session", &[], b"");
    two.extend(request("HEAD", "/api/session", &[], b""));
    c.send(&two).await;
    assert_eq!(c.read(false).await.status, 200);
    assert_eq!(c.read(true).await.status, 200);
}

#[tokio::test]
async fn too_many_connections_get_server_busy() {
    let h = Harness::start(|c| c.server.max_connections = 2).await;
    let mut a = Client::connect(h.addr).await;
    let mut b = Client::connect(h.addr).await;
    a.send(&request("GET", "/api/session", &[], b"")).await;
    b.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(a.read(false).await.status, 200);
    assert_eq!(b.read(false).await.status, 200);
    let mut c = Client::connect(h.addr).await;
    c.send(&request("GET", "/api/session", &[], b"")).await;
    let r = c.read(false).await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_busy"));
    assert_eq!(r.header("retry-after"), Some("1"));
    assert_eq!(r.header("connection"), Some("close"));
    assert_eq!(r.json()["error"]["retryable"], true);
    security_headers_ok(&r);
    assert!(c.closed_within(Duration::from_secs(2)).await);
    // The two first connections still work, and a place frees when one closes.
    a.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(a.read(false).await.status, 200);
    drop(b);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(get(h.addr, "/api/session").await.status, 200);
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_http_refused_total{reason=\"max_connections\"} 1"), "{text}");
}

#[tokio::test]
async fn bodies_in_flight_are_bounded() {
    let h = Harness::start(|_| {}).await;
    // Take all the places for the small bodies, with requests whose body does not end.
    let mut held = Vec::new();
    for _ in 0..crate::server::limits::MAX_INFLIGHT_SMALL_BODIES {
        let mut c = Client::connect(h.addr).await;
        c.send(b"POST /api/folders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 10\r\n\r\n{").await;
        held.push(c);
    }
    // Wait until the server holds all the places.
    for _ in 0..200 {
        if h.state().small_bodies.available_permits() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let r = call(h.addr, "POST", "/api/folders", &[("Content-Type", "application/json")], b"{}").await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_busy"));
    assert_eq!(r.header("retry-after"), Some("1"));
    assert_eq!(r.header("connection"), Some("close"));
    // The document bodies have their own places.
    assert!(h.state().doc_bodies.available_permits() > 0);
    drop(held);
}

#[tokio::test]
async fn ops_listener_routes() {
    let h = Harness::start(|_| {}).await;
    let r = get(h.ops, "/livez").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.text(), "{\"status\":\"live\"}");
    assert_eq!(r.header("cache-control"), Some("no-store"));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    let r = get(h.ops, "/readyz").await;
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(
        (j["ready"].as_bool(), j["stopping"].as_bool(), j["catalog_loaded"].as_bool()),
        (Some(true), Some(false), Some(true))
    );
    assert_eq!(j["store"]["backend"], "volume");
    assert_eq!(j["store"]["state"], "up");
    assert!(j["store"]["catalog_age_seconds"].is_u64());
    assert_eq!(j["sources"]["prod"], "unknown");
    assert_eq!(j["pdp"], "none");
    let r = get(h.ops, "/metrics").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("text/plain; version=0.0.4; charset=utf-8"));
    let text = r.text();
    assert!(text.contains("hune_build_info{version=\"0.1.2\""));
    assert!(text.contains("hune_ready 1\n"));
    assert!(text.contains("hune_store_up 1\n"));
    // HEAD works; another method is 405; another path is 404; a body is 400.
    let head = call(h.ops, "HEAD", "/livez", &[], b"").await;
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    let r = call(h.ops, "POST", "/livez", &[], b"").await;
    assert_eq!((r.status, r.code().as_str(), r.header("allow")), (405, "method_not_allowed", Some("GET, HEAD")));
    let r = get(h.ops, "/api/session").await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    let r = get(h.ops, "/").await;
    assert_eq!(r.status, 404);
    let r = call(h.ops, "GET", "/livez", &[("Content-Length", "2")], b"{}").await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    // The ops listener has no route of the main listener, and the main one has none of the ops routes.
    assert_eq!(get(h.addr, "/livez").await.status, 404);
    assert_eq!(get(h.addr, "/metrics").await.status, 404);
    // Calls of the ops listener make no http.request line and no http metric.
    let text = get(h.ops, "/metrics").await.text();
    assert!(!text.contains("route=\"GET /livez\""));
}

#[tokio::test]
async fn http_metrics_and_log_line() {
    let h = Harness::start(|_| {}).await;
    let _ = call(h.addr, "GET", "/api/session", &[("X-Request-Id", "req-1")], b"").await;
    let _ = get(h.addr, "/api/nothing").await;
    let text = get(h.ops, "/metrics").await.text();
    assert!(text.contains("hune_http_requests_total{route=\"GET /api/session\",status=\"200\"} 1\n"), "{text}");
    assert!(text.contains("hune_http_requests_total{route=\"unmatched\",status=\"404\"} 1\n"));
    assert!(text.contains("hune_http_request_duration_seconds_count{route=\"GET /api/session\"} 1\n"));
    assert!(text.contains("hune_http_in_flight 0\n"));
    let line = h
        .logger
        .memory_lines()
        .into_iter()
        .map(|l| serde_json::from_str::<serde_json::Value>(&l).unwrap())
        .find(|v| v["event"] == "http.request" && v["request_id"] == "req-1")
        .expect("an http.request line");
    assert_eq!(line["level"], "info");
    assert_eq!(line["method"], "GET");
    assert_eq!(line["route"], "GET /api/session");
    assert_eq!(line["status"], 200);
    assert!(line["code"].is_null());
    assert!(line["duration_ms"].is_u64());
    assert_eq!(line["request_bytes"], 0);
    assert!(line["response_bytes"].as_u64().unwrap() > 100);
    assert_eq!(line["subject"], "-");
    assert!(line["client"].as_str().unwrap().starts_with("127.0.0.1:"));
    let err = h
        .logger
        .memory_lines()
        .into_iter()
        .map(|l| serde_json::from_str::<serde_json::Value>(&l).unwrap())
        .find(|v| v["event"] == "http.request" && v["status"] == 404)
        .unwrap();
    assert_eq!(err["code"], "not_found");
    assert_eq!(err["route"], "unmatched");
}

#[tokio::test]
async fn start_events_and_no_secret_in_the_log() {
    let mut h = Harness::start(|_| {}).await;
    let _ = get(h.addr, "/api/session").await;
    let events = h.events();
    let pos = |name: &str| events.iter().position(|e| e == name).unwrap_or_else(|| panic!("{name} in {events:?}"));
    assert!(pos("store.loaded") < pos("server.listening"));
    assert!(pos("server.listening") < pos("server.ready"));
    let listening = h
        .logger
        .memory_lines()
        .into_iter()
        .map(|l| serde_json::from_str::<serde_json::Value>(&l).unwrap())
        .find(|v| v["event"] == "server.listening")
        .unwrap();
    assert_eq!(listening["access_mode"], "open");
    assert_eq!(listening["storage_backend"], "volume");
    assert_eq!(listening["listen"], h.addr.to_string());
    assert_eq!(listening["ops_listen"], h.ops.to_string());
    assert_eq!(h.stop().await, 0);
    let all = h.logger.memory_lines().join("\n");
    assert!(!all.contains("secret-password"));
}

#[tokio::test]
async fn health_client_follows_livez() {
    let mut h = Harness::start(|_| {}).await;
    let ops = h.ops;
    let ok = tokio::task::spawn_blocking(move || crate::server::health_client::check(ops, Duration::from_secs(2)))
        .await
        .unwrap();
    assert!(ok.is_ok(), "{ok:?}");
    assert_eq!(h.stop().await, 0);
    let ops = h.ops;
    let down = tokio::task::spawn_blocking(move || crate::server::health_client::check(ops, Duration::from_secs(2)))
        .await
        .unwrap();
    assert!(down.is_err());
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

#[tokio::test]
async fn readyz_is_503_until_the_catalog_loads() {
    use crate::server::lifecycle;
    use std::sync::Arc;
    let ops_port = free_port();
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&std::fs::canonicalize(dir.path()).unwrap());
    config.server.ops_listen = format!("127.0.0.1:{ops_port}").parse().unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let release = Arc::new(tokio::sync::Notify::new());
    let release2 = release.clone();
    let hook = move |state: Arc<crate::server::state::AppState>| async move {
        release2.notified().await;
        crate::server::startup::open_store(state).await
    };
    let starting = tokio::spawn(lifecycle::start(Arc::new(config), logger, test_timing(), hook));
    let ops: std::net::SocketAddr = format!("127.0.0.1:{ops_port}").parse().unwrap();
    // The ops listener answers while the hook runs: live, and not ready.
    let mut live = None;
    for _ in 0..200 {
        if let Ok(s) = tokio::net::TcpStream::connect(ops).await {
            live = Some(s);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(live.expect("the ops listener binds first"));
    assert_eq!(get(ops, "/livez").await.status, 200);
    let r = get(ops, "/readyz").await;
    assert_eq!(r.status, 503);
    assert_eq!(r.json()["catalog_loaded"], false);
    assert_eq!(r.json()["ready"], false);
    assert!(get(ops, "/metrics").await.text().contains("hune_ready 0\n"));
    release.notify_one();
    let running = starting.await.unwrap().expect("the start ends");
    let r = get(ops, "/readyz").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["ready"], true);
    assert_eq!(get(running.listen_addr, "/api/session").await.status, 200);
    let mut rx = tokio::sync::mpsc::unbounded_channel().1;
    assert_eq!(running.stop("SIGTERM", &mut rx).await, 0);
}

#[tokio::test]
async fn failed_start_steps_log_start_failed() {
    use crate::server::lifecycle;
    // The main port is taken: exit with start_failed and the ops listener closes.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&std::fs::canonicalize(dir.path()).unwrap());
    config.server.listen = taken.local_addr().unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let r = lifecycle::start(
        std::sync::Arc::new(config),
        logger.clone(),
        test_timing(),
        crate::server::startup::open_store,
    )
    .await;
    let e = r.err().expect("the bind fails");
    assert_eq!(e.step, "bind listen");
    assert!(e.error.contains(&taken.local_addr().unwrap().to_string()), "{}", e.error);
    let line = logger.memory_lines().into_iter().find(|l| l.contains("server.start_failed")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!((v["level"].as_str(), v["step"].as_str()), (Some("error"), Some("bind listen")));
    // The ops port is taken.
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&std::fs::canonicalize(dir.path()).unwrap());
    config.server.ops_listen = taken.local_addr().unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let e = lifecycle::start(std::sync::Arc::new(config), logger, test_timing(), crate::server::startup::open_store)
        .await
        .err()
        .unwrap();
    assert_eq!(e.step, "bind ops_listen");
    // A volume that cannot open stops the start.
    let mut config = test_config(std::path::Path::new("/nonexistent/hune/volume"));
    config.server.listen = "127.0.0.1:0".parse().unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let e = lifecycle::start(std::sync::Arc::new(config), logger, test_timing(), crate::server::startup::open_store)
        .await
        .err()
        .unwrap();
    assert_eq!(e.step, "store.open");
    assert!(e.error.contains("/nonexistent/hune/volume"), "{}", e.error);
}

#[tokio::test]
async fn graceful_stop_lets_a_request_in_flight_end() {
    let mut h = Harness::start_with(test_timing(), |_| {}, |c| c.server.shutdown_grace = Duration::from_secs(30)).await;
    let addr = h.addr;
    let ops = h.ops;
    // A request in flight: its body is not complete.
    let mut busy = Client::connect(addr).await;
    busy.send(b"POST /api/folders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 4\r\n\r\n{").await;
    for _ in 0..200 {
        if h.state().metrics.http_in_flight() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // An idle kept connection.
    let mut idle = Client::connect(addr).await;
    idle.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(idle.read(false).await.status, 200);
    let state = h.state();
    let running = h.running.take().unwrap();
    let mut rx = h.signals_rx.take().unwrap();
    let stopping = tokio::spawn(async move { running.stop("SIGTERM", &mut rx).await });
    // At once: /readyz is 503 and the metric shows the stop. The listener still serves during the drain.
    for _ in 0..200 {
        if state.stopping() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let r = get(ops, "/readyz").await;
    assert_eq!(r.status, 503);
    assert_eq!(r.json()["stopping"], true);
    assert!(get(ops, "/metrics").await.text().contains("hune_stopping 1\n"));
    assert_eq!(get(ops, "/livez").await.status, 200);
    // After the drain the idle connection closes and the accept stops.
    assert!(idle.closed_within(Duration::from_secs(3)).await);
    for _ in 0..100 {
        if state.phase() == Phase::Refusing {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(tokio::net::TcpStream::connect(addr).await.is_err(), "the listener is closed");
    assert!(!stopping.is_finished(), "the stop waits for the request in flight");
    // The request in flight ends: its answer comes, the connection closes, the stop ends with 0.
    busy.send(b"1}}").await;
    let r = busy.read(false).await;
    assert_ne!(r.status, 503);
    assert!(busy.closed_within(Duration::from_secs(3)).await);
    assert_eq!(stopping.await.unwrap(), 0);
    let events = h.events();
    assert!(events.contains(&"server.stopping".to_owned()));
    assert_eq!(events.last().unwrap(), "server.stopped");
    // The ops listener is closed at the exit.
    assert!(tokio::net::TcpStream::connect(ops).await.is_err());
}

#[tokio::test]
async fn requests_after_the_drain_get_server_stopping() {
    let h = Harness::start(|_| {}).await;
    let mut c = Client::connect(h.addr).await;
    c.send(&request("GET", "/api/session", &[], b"")).await;
    assert_eq!(c.read(false).await.status, 200);
    h.state().set_phase(Phase::Refusing);
    c.send(&request("GET", "/api/session", &[], b"")).await;
    let r = c.read(false).await;
    assert_eq!((r.status, r.code().as_str()), (503, "server_stopping"));
    assert_eq!(r.header("connection"), Some("close"));
    assert_eq!(r.json()["error"]["retryable"], true);
    security_headers_ok(&r);
    assert!(c.closed_within(Duration::from_secs(2)).await);
    // During the drain the server serves as usual.
    h.state().set_phase(Phase::Draining);
    assert_eq!(get(h.addr, "/api/session").await.status, 200);
}

#[tokio::test]
async fn grace_that_ends_with_a_request_in_flight_exits_with_1() {
    let mut h = Harness::start_with(test_timing(), |_| {}, |c| c.server.shutdown_grace = Duration::from_secs(3)).await;
    let mut busy = Client::connect(h.addr).await;
    busy.send(b"POST /api/folders HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 40\r\n\r\n{").await;
    for _ in 0..200 {
        if h.state().metrics.http_in_flight() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let started = std::time::Instant::now();
    assert_eq!(h.stop().await, 1);
    assert!(started.elapsed() < Duration::from_secs(5));
    let events = h.events();
    assert!(events.contains(&"server.shutdown_timeout".to_owned()), "{events:?}");
    assert!(!events.contains(&"server.stopped".to_owned()));
    drop(busy);
}

#[tokio::test]
async fn second_signal_ends_the_stop_with_1() {
    let mut h = Harness::start_with(Timing { drain: Duration::from_secs(5), ..test_timing() }, |_| {}, |_| {}).await;
    h.signals_tx.send("SIGINT").unwrap();
    let started = std::time::Instant::now();
    assert_eq!(h.stop().await, 1);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(h.events().contains(&"server.signal_again".to_owned()));
}

#[tokio::test]
async fn a_second_request_in_flight_counter_returns_to_zero() {
    let h = Harness::start(|_| {}).await;
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let addr = h.addr;
        tasks.push(tokio::spawn(async move { get(addr, "/api/session").await.status }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), 200);
    }
    assert_eq!(h.state().metrics.http_in_flight(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn works_on_the_multi_thread_runtime() {
    let h = Harness::start(|_| {}).await;
    assert_eq!(get(h.addr, "/api/session").await.status, 200);
    assert_eq!(get(h.ops, "/livez").await.status, 200);
}

#[test]
fn serves_on_the_runtime_of_the_product() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let mut config = test_config(&std::fs::canonicalize(dir.path()).unwrap());
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let ops_addr = probe.local_addr().unwrap();
    drop(probe);
    config.server.ops_listen = ops_addr;
    let runtime = crate::server::build_runtime(&config).unwrap();
    let logger = crate::log::Logger::memory(crate::config::LogLevel::Debug);
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = std::thread::spawn(move || {
        runtime.block_on(crate::server::lifecycle::run(
            std::sync::Arc::new(config),
            logger,
            test_timing(),
            crate::server::startup::open_store,
            rx,
        ))
    });
    let mut answered = false;
    for _ in 0..100 {
        if let Ok(mut s) = std::net::TcpStream::connect(ops_addr) {
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            s.write_all(b"GET /livez HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
            let mut text = String::new();
            if s.read_to_string(&mut text).is_ok() && text.starts_with("HTTP/1.1 200") {
                answered = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    tx.send("SIGTERM").unwrap();
    assert_eq!(thread.join().unwrap(), 0);
    assert!(answered, "the ops listener answers on the multi-thread runtime of the product");
}
