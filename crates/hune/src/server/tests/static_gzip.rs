//! The gzip copies of the static files (9.3, 13.8 `server.compress_static`, decision ID-K304): the headers
//! `Content-Encoding`, `Vary` and `Content-Length`, `HEAD`, `Range`, and the answers that stay as they are.

use super::harness::*;
use crate::web_assets::{self, BUILD_HASH, FILES, WebFile};
use std::io::{Read, Write};
use std::process::{Command, Stdio};

const GZIP_FIRST: &str = "gzip, deflate, br, zstd";

fn url(file: &WebFile) -> String {
    format!("/static/{BUILD_HASH}/{}", file.path)
}

/// The largest JavaScript file: the copy of the chart engine.
fn largest_js() -> &'static WebFile {
    FILES.iter().filter(|f| f.path.ends_with(".js")).max_by_key(|f| f.bytes.len()).unwrap()
}

/// Decodes a gzip body with the system `gzip -d`.
fn gunzip(body: &[u8]) -> Vec<u8> {
    let mut child = Command::new("gzip")
        .args(["-d", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the system gzip is necessary for this test");
    let mut stdin = child.stdin.take().unwrap();
    let input = body.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    writer.join().unwrap().unwrap();
    assert!(child.wait().unwrap().success());
    out
}

#[tokio::test]
async fn a_request_that_accepts_gzip_gets_the_copy_with_the_right_headers() {
    let h = Harness::start(|_| {}).await;
    let file = largest_js();
    let copy = web_assets::gzip_copy(file).expect("the largest JavaScript file has a gzip copy");
    let r = call(h.addr, "GET", &url(file), &[("Accept-Encoding", GZIP_FIRST)], b"").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-encoding"), Some("gzip"));
    assert_eq!(r.header("vary"), Some("Accept-Encoding"));
    assert_eq!(r.header("content-type"), Some(file.content_type));
    assert_eq!(r.header("cache-control"), Some("private, max-age=31536000, immutable"));
    assert_eq!(r.header("content-length"), Some(copy.len().to_string().as_str()));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    assert!(r.header("content-security-policy").is_some());
    // The static answers have no entity tag (6.25), so the copy cannot share one with the file.
    assert_eq!(r.header("etag"), None);
    assert_eq!(r.body, copy.as_ref());
    assert!(r.body.len() * 2 < file.bytes.len(), "{} -> {}", file.bytes.len(), r.body.len());
    assert!(gunzip(&r.body) == file.bytes, "the copy does not decode to the file");
}

#[tokio::test]
async fn a_request_that_does_not_accept_gzip_gets_the_file() {
    let h = Harness::start(|_| {}).await;
    let file = largest_js();
    for headers in [
        &[][..],
        &[("Accept-Encoding", "gzip;q=0")][..],
        &[("Accept-Encoding", "br, deflate")][..],
        &[("Accept-Encoding", "identity")][..],
        &[("Accept-Encoding", "gzip;q=bad")][..],
        &[("Accept-Encoding", "*;q=0")][..],
    ] {
        let r = call(h.addr, "GET", &url(file), headers, b"").await;
        assert_eq!(r.status, 200, "{headers:?}");
        assert_eq!(r.header("content-encoding"), None, "{headers:?}");
        assert_eq!(r.header("vary"), Some("Accept-Encoding"), "{headers:?}");
        assert_eq!(r.header("cache-control"), Some("private, max-age=31536000, immutable"), "{headers:?}");
        assert_eq!(r.header("content-length"), Some(file.bytes.len().to_string().as_str()));
        assert_eq!(r.body, file.bytes, "{headers:?}");
    }
}

#[tokio::test]
async fn head_has_the_headers_of_get_for_each_variant() {
    let h = Harness::start(|_| {}).await;
    let file = largest_js();
    let copy = web_assets::gzip_copy(file).unwrap();
    for (headers, encoding, len) in
        [(&[("Accept-Encoding", "gzip")][..], Some("gzip"), copy.len()), (&[][..], None, file.bytes.len())]
    {
        let get = call(h.addr, "GET", &url(file), headers, b"").await;
        let head = call(h.addr, "HEAD", &url(file), headers, b"").await;
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert_eq!(head.header("content-length"), Some(len.to_string().as_str()));
        for name in ["content-type", "content-encoding", "vary", "cache-control", "content-length", "etag"] {
            assert_eq!(head.header(name), get.header(name), "{name}");
        }
        assert_eq!(head.header("content-encoding"), encoding);
    }
}

#[tokio::test]
async fn range_is_ignored_and_the_whole_body_comes() {
    let h = Harness::start(|_| {}).await;
    let file = largest_js();
    let copy = web_assets::gzip_copy(file).unwrap();
    let r = call(h.addr, "GET", &url(file), &[("Accept-Encoding", "gzip"), ("Range", "bytes=0-9")], b"").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-range"), None);
    assert_eq!(r.header("accept-ranges"), None);
    assert_eq!(r.body, copy.as_ref());
    let raw = call(h.addr, "GET", &url(file), &[("Range", "bytes=0-9")], b"").await;
    assert_eq!((raw.status, raw.body.len()), (200, file.bytes.len()));
    assert_eq!(raw.header("content-range"), None);
}

#[tokio::test]
async fn each_text_file_has_a_smaller_copy_and_a_font_has_none() {
    let h = Harness::start(|_| {}).await;
    for f in FILES.iter().filter(|f| f.path != web_assets::SHELL_PATH) {
        let r = call(h.addr, "GET", &url(f), &[("Accept-Encoding", "gzip")], b"").await;
        assert_eq!(r.status, 200, "{}", f.path);
        match web_assets::gzip_copy(f) {
            Some(copy) => {
                assert!(web_assets::is_compressible(f.content_type), "{}", f.path);
                assert!(copy.len() < f.bytes.len(), "{}", f.path);
                assert_eq!(r.header("content-encoding"), Some("gzip"), "{}", f.path);
                assert_eq!(r.body, copy.as_ref(), "{}", f.path);
            }
            None => {
                assert_eq!(r.header("content-encoding"), None, "{}", f.path);
                assert_eq!(r.body, f.bytes, "{}", f.path);
            }
        }
        let text = web_assets::is_compressible(f.content_type);
        assert_eq!(r.header("vary"), text.then_some("Accept-Encoding"), "{}", f.path);
        // A text file of more than 1 KiB always has a copy.
        if text && f.bytes.len() > 1024 {
            assert!(web_assets::gzip_copy(f).is_some(), "{}", f.path);
        }
    }
    let font = FILES.iter().find(|f| f.path.ends_with(".woff2")).expect("the font is in web/");
    assert!(web_assets::gzip_copy(font).is_none());
}

#[tokio::test]
async fn the_shell_the_api_and_the_errors_are_never_compressed() {
    let h = Harness::start(|_| {}).await;
    let accept = [("Accept-Encoding", "gzip")];
    for path in ["/", "/d/abc", "/no/such/page", "/api/session", "/api/dashboards", "/api/sources"] {
        let r = call(h.addr, "GET", path, &accept, b"").await;
        assert_eq!(r.header("content-encoding"), None, "{path}");
        assert_eq!(r.header("vary"), None, "{path}");
    }
    let missing = call(h.addr, "GET", &format!("/static/{BUILD_HASH}/absent.js"), &accept, b"").await;
    assert_eq!((missing.status, missing.header("content-encoding")), (404, None));
    let shell = call(h.addr, "GET", &format!("/static/{BUILD_HASH}/shell.html"), &accept, b"").await;
    assert_eq!((shell.status, shell.header("content-encoding")), (404, None));
}

#[tokio::test]
async fn the_key_false_sends_each_file_as_it_is() {
    let h = Harness::start(|c| c.server.compress_static = false).await;
    let file = largest_js();
    let r = call(h.addr, "GET", &url(file), &[("Accept-Encoding", GZIP_FIRST)], b"").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-encoding"), None);
    assert_eq!(r.header("vary"), None);
    assert_eq!(r.body, file.bytes);
}
