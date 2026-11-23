// SPDX-License-Identifier: MIT OR Apache-2.0
//! The block `geomap` on the main listener (9.4, 12.8, 6.10): without the block, the header of 12.8 byte for byte
//! and the session member `null`; with the block, one more source in `img-src` only, and the session member with the three values.

use super::harness::*;
use crate::config::{GeomapConfig, parse_tile_url};
use crate::web_assets::BUILD_HASH;

/// The header of 12.8 and 9.4, as the specification writes it.
const CSP_12_8: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

fn block(tile_url: &str) -> GeomapConfig {
    GeomapConfig {
        tile_url: parse_tile_url(tile_url).unwrap(),
        attribution: "Tiles: example.org".to_owned(),
        max_zoom: 12,
    }
}

/// The paths of the kinds of answers: the shell, a page, an API answer, a static file, an error.
fn paths() -> Vec<String> {
    let file = crate::web_assets::FILES.first().map(|f| f.path).unwrap_or("js/main.js");
    vec![
        "/".to_owned(),
        "/d/abc".to_owned(),
        "/api/session".to_owned(),
        format!("/static/{BUILD_HASH}/{file}"),
        "/api/nothing".to_owned(),
    ]
}

#[tokio::test]
async fn without_the_block_the_header_is_the_header_of_12_8_and_the_session_member_is_null() {
    assert_eq!(crate::server::headers::CSP, CSP_12_8);
    let h = Harness::start(|_| {}).await;
    for path in paths() {
        let r = get(h.addr, &path).await;
        assert_eq!(r.header("content-security-policy"), Some(CSP_12_8), "{path}");
    }
    let j = get(h.addr, "/api/session").await.json();
    assert!(j.as_object().unwrap().contains_key("geomap"));
    assert_eq!(j["geomap"], serde_json::Value::Null);
}

#[tokio::test]
async fn with_the_block_only_img_src_gains_the_tile_source() {
    let tile = "https://tiles.example.org:8443/osm/{z}/{x}/{y}.png?key=public-key";
    let h = Harness::start(|c| c.geomap = Some(block(tile))).await;
    let want = CSP_12_8.replace("img-src 'self' data:;", "img-src 'self' data: https://tiles.example.org:8443/osm/;");
    for path in paths() {
        let r = get(h.addr, &path).await;
        assert_eq!(r.header("content-security-policy"), Some(want.as_str()), "{path}");
    }
    // Each other directive is the same, connect-src included: the page fetches a tile as an image only.
    let others = |csp: &str| -> Vec<String> {
        csp.split("; ").filter(|d| !d.starts_with("img-src")).map(str::to_owned).collect()
    };
    assert_eq!(others(&want), others(CSP_12_8));
    assert!(want.contains("connect-src 'self';"));
    assert!(!want.contains("key=public-key") && !want.contains("{z}"));
    let j = get(h.addr, "/api/session").await.json();
    assert_eq!(j["geomap"], serde_json::json!({"tile_url": tile, "attribution": "Tiles: example.org", "max_zoom": 12}));
}

#[test]
fn the_header_comes_from_the_parts_so_no_policy_character_reaches_it() {
    // Each accepted template of the shared list gives a header with ten directives and one more source in img-src.
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../tests/frontend/vectors/FE-85-geomap.json")).unwrap();
    for item in vectors["tile_url"]["valid"].as_array().unwrap() {
        let g = block(item[0].as_str().unwrap());
        let csp = crate::server::headers::csp_header(Some(&g));
        assert_eq!(csp.split("; ").count(), 10, "{csp}");
        let img = csp.split("; ").find(|d| d.starts_with("img-src ")).unwrap();
        assert_eq!(img, format!("img-src 'self' data: {}", item[1].as_str().unwrap()));
        assert_eq!(csp.matches(',').count(), 0);
        assert_eq!(csp.matches('"').count(), 0);
        assert!(http::HeaderValue::from_str(&csp).is_ok());
    }
    assert_eq!(crate::server::headers::csp_header(None), CSP_12_8);
}
