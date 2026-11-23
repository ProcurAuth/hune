// SPDX-License-Identifier: MIT OR Apache-2.0
//! The static files of the front end, embedded at build time (9.3).
//!
//! `build.rs` makes the table `FILES` and the constant `BUILD_HASH`. This module gives the lookup
//! and the shell page.
#![allow(dead_code)]

use crate::server::build_info::{UiVersion, current_ui_version};
use bytes::Bytes;

/// One embedded file.
pub struct WebFile {
    /// The path relative to `web/`, with `/` as separator.
    pub path: &'static str,
    /// The `Content-Type` of the extension (9.3).
    pub content_type: &'static str,
    /// The bytes of the file.
    pub bytes: &'static [u8],
    /// The SHA-256 of the bytes, in lower-case hexadecimal.
    pub sha256: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/web_table.rs"));

/// The path of the shell template in the table.
pub const SHELL_PATH: &str = "shell.html";

/// Finds an embedded file by its exact path. The table is in byte order of the path.
pub fn find(path: &str) -> Option<&'static WebFile> {
    FILES.binary_search_by(|f| f.path.as_bytes().cmp(path.as_bytes())).ok().map(|i| &FILES[i])
}

/// Finds the file that `GET /static/{build-hash}/{path}` serves (6.25).
///
/// The build hash must equal the build hash of the binary. The file `shell.html` is never served.
pub fn find_static(build_hash: &str, path: &str) -> Option<&'static WebFile> {
    if build_hash != BUILD_HASH || path == SHELL_PATH {
        return None;
    }
    find(path)
}

/// Tells if a file of this `Content-Type` gets a gzip copy (9.3): HTML, JavaScript, CSS, SVG, JSON and plain
/// text. A font and a PNG image are compressed already, so they get none.
pub fn is_compressible(content_type: &str) -> bool {
    content_type.starts_with("text/") || content_type == "image/svg+xml" || content_type == "application/json"
}

/// The gzip copies of the embedded files, in the order of `FILES` (9.3, 13.8 `server.compress_static`).
///
/// The first call makes the table, and each later call gives the same table. A file has a copy when its type
/// is compressible, when it is not the shell template, and when the copy is smaller than the file. The
/// server calls this function at start when `server.compress_static` is true. A request never compresses.
pub fn gzip_copies() -> &'static [Option<Bytes>] {
    static COPIES: std::sync::OnceLock<Vec<Option<Bytes>>> = std::sync::OnceLock::new();
    COPIES.get_or_init(|| {
        FILES
            .iter()
            .map(|f| {
                if f.path == SHELL_PATH || !is_compressible(f.content_type) {
                    return None;
                }
                crate::gzip::gzip(f.bytes).filter(|gz| gz.len() < f.bytes.len()).map(Bytes::from)
            })
            .collect()
    })
}

/// The gzip copy of an embedded file, when it has one.
pub fn gzip_copy(file: &WebFile) -> Option<Bytes> {
    let index = FILES.binary_search_by(|f| f.path.as_bytes().cmp(file.path.as_bytes())).ok()?;
    gzip_copies().get(index)?.clone()
}

/// Escapes a text for use in an HTML attribute or in an HTML text.
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Makes the shell page from a template (9.3, 9.6).
///
/// The marker `{{base_path}}` becomes the HTML-escaped base path. The marker `{{build}}` becomes
/// the build hash. The markers `{{version_text}}` and `{{version_title}}` become the version line of
/// the header and its title (9.6). Their characters are safe in a JSON string, so no escape applies.
pub fn render_shell(template: &str, base_path: &str, build: &str, version: &UiVersion) -> Bytes {
    let text = template
        .replace("{{base_path}}", &html_escape(base_path))
        .replace("{{build}}", build)
        .replace("{{version_text}}", &version.text)
        .replace("{{version_title}}", &version.title);
    Bytes::from(text.into_bytes())
}

/// Makes the shell page of this build, or gives `None` when the build holds no `shell.html`.
pub fn build_shell(base_path: &str) -> Option<Bytes> {
    let file = find(SHELL_PATH)?;
    let template = std::str::from_utf8(file.bytes).ok()?;
    Some(render_shell(template, base_path, BUILD_HASH, &current_ui_version()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_hashes_are_hex() {
        for pair in FILES.windows(2) {
            assert!(pair[0].path.as_bytes() < pair[1].path.as_bytes());
        }
        for f in FILES {
            assert_eq!(f.sha256.len(), 64);
            assert!(f.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }
        assert_eq!(BUILD_HASH.len(), 16);
    }

    #[test]
    fn build_hash_is_the_hash_of_the_build_list() {
        use sha2::{Digest, Sha256};
        let mut list = String::new();
        for f in FILES {
            list.push_str(&format!("{} {}\n", f.path, f.sha256));
        }
        let digest = Sha256::digest(list.as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(&hex[..16], BUILD_HASH);
    }

    #[test]
    fn each_file_hash_matches_its_bytes() {
        use sha2::{Digest, Sha256};
        for f in FILES {
            let digest = Sha256::digest(f.bytes);
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, f.sha256, "{}", f.path);
        }
    }

    #[test]
    fn static_lookup_rules() {
        assert!(find_static(BUILD_HASH, "js/main.js").is_some());
        assert!(find_static("0000000000000000", "js/main.js").is_none());
        assert!(find_static(BUILD_HASH, SHELL_PATH).is_none());
        assert!(find_static(BUILD_HASH, "../Cargo.toml").is_none());
        assert!(find_static(BUILD_HASH, "js/").is_none());
    }

    #[test]
    fn shell_markers_are_replaced_and_escaped() {
        let version = UiVersion { text: "0.1.0:012345678".to_owned(), title: "Hune 0.1.0".to_owned() };
        let page = render_shell(
            "<base href=\"{{base_path}}\"> {{build}} {{build}} {{version_text}} {{version_title}}",
            "/a&b\"<x>/",
            "abcd",
            &version,
        );
        let text = std::str::from_utf8(&page).unwrap();
        assert_eq!(text, "<base href=\"/a&amp;b&quot;&lt;x&gt;/\"> abcd abcd 0.1.0:012345678 Hune 0.1.0");
        assert!(!text.contains("{{"));
    }

    #[test]
    fn shell_of_this_build_has_no_marker_left() {
        let page = build_shell("/dashboards/").expect("shell.html is in web/");
        let text = std::str::from_utf8(&page).unwrap();
        assert!(!text.contains("{{"), "a marker is left in the shell");
        assert!(text.contains("<base href=\"/dashboards/\">"));
        assert!(text.contains(BUILD_HASH));
        let version = current_ui_version();
        assert!(text.contains(&format!("\"version\":\"{}\"", version.text)));
        assert!(text.contains(&format!("\"version_title\":\"{}\"", version.title)));
    }
}
