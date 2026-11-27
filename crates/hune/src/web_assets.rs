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

/// The shell pages of this build (6.25, 9.6). All have the same body, except the modules that the shell preloads.
pub struct Shells {
    /// The shell with no route list: the page of the status 404, and the base of the two others.
    pub base: Bytes,
    /// The shell of `GET /`. It also preloads the modules of the list page.
    pub list: Bytes,
    /// The shell of `GET /d/{uid}[/{slug}]` with a valid uid. It also preloads the modules of the dashboard page.
    pub dashboard: Bytes,
}

/// The links `modulepreload` of a list of paths of `web/`, one on each line, for the marker `{{preload}}` (9.6).
fn preload_links(build: &str, paths: &[&str]) -> String {
    let mut out = String::new();
    for path in paths {
        out.push_str(&format!("  <link rel=\"modulepreload\" href=\"static/{build}/{}\">\n", html_escape(path)));
    }
    out
}

/// The path of a style sheet that a line of the shell template links, or `None` for another line.
fn sheet_of_line(line: &str) -> Option<&str> {
    line.trim().strip_prefix("<link rel=\"stylesheet\" href=\"static/{{build}}/")?.strip_suffix("\">")
}

/// Replaces the links of the style sheets that `PAGE_CSS` joins by one link to `PAGE_CSS`, at the place of the first one (9.6).
fn join_sheets(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut done = false;
    for line in template.split_inclusive('\n') {
        if sheet_of_line(line).is_some_and(|p| SHELL_SHEETS.contains(&p)) {
            if !done {
                out.push_str(&format!("  <link rel=\"stylesheet\" href=\"static/{{{{build}}}}/{PAGE_CSS}\">\n"));
                done = true;
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Makes the shell page from a template (9.3, 9.6).
///
/// The marker `{{base_path}}` becomes the HTML-escaped base path. The marker `{{build}}` becomes
/// the build hash. The markers `{{version_text}}` and `{{version_title}}` become the version line of
/// the header and its title (9.6). Their characters are safe in a JSON string, so no escape applies.
/// The links of the style sheets of `SHELL_SHEETS` become one link to `PAGE_CSS`.
/// The marker `{{preload}}` is a line of its own. It becomes one link `modulepreload` for each path of
/// `preload`, or nothing when the list is empty.
pub fn render_shell(template: &str, base_path: &str, build: &str, version: &UiVersion, preload: &[&str]) -> Bytes {
    let text = join_sheets(template)
        .replace("{{preload}}\n", &preload_links(build, preload))
        .replace("{{preload}}", "")
        .replace("{{base_path}}", &html_escape(base_path))
        .replace("{{build}}", build)
        .replace("{{version_text}}", &version.text)
        .replace("{{version_title}}", &version.title);
    Bytes::from(text.into_bytes())
}

/// Makes the shell pages of this build, or gives `None` when the build holds no `shell.html`.
pub fn build_shells(base_path: &str) -> Option<Shells> {
    let file = find(SHELL_PATH)?;
    let template = std::str::from_utf8(file.bytes).ok()?;
    let version = current_ui_version();
    let make = |preload: &[&str]| render_shell(template, base_path, BUILD_HASH, &version, preload);
    Some(Shells { base: make(&[]), list: make(PRELOAD_LIST), dashboard: make(PRELOAD_DASHBOARD) })
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
            &[],
        );
        let text = std::str::from_utf8(&page).unwrap();
        assert_eq!(text, "<base href=\"/a&amp;b&quot;&lt;x&gt;/\"> abcd abcd 0.1.0:012345678 Hune 0.1.0");
        assert!(!text.contains("{{"));
    }

    #[test]
    fn preload_marker_gives_one_link_for_each_path_or_nothing() {
        let version = UiVersion { text: "t".to_owned(), title: "t".to_owned() };
        let template = "<a>\n{{preload}}\n<b>\n";
        let none = render_shell(template, "/", "abcd", &version, &[]);
        assert_eq!(std::str::from_utf8(&none).unwrap(), "<a>\n<b>\n");
        let two = render_shell(template, "/", "abcd", &version, &["js/x.js", "js/a&b.js"]);
        assert_eq!(
            std::str::from_utf8(&two).unwrap(),
            "<a>\n  <link rel=\"modulepreload\" href=\"static/abcd/js/x.js\">\n  <link rel=\"modulepreload\" href=\"static/abcd/js/a&amp;b.js\">\n<b>\n"
        );
    }

    #[test]
    fn shell_of_this_build_has_no_marker_left() {
        let shells = build_shells("/dashboards/").expect("shell.html is in web/");
        for page in [&shells.base, &shells.list, &shells.dashboard] {
            let text = std::str::from_utf8(page).unwrap();
            assert!(!text.contains("{{"), "a marker is left in the shell");
            assert!(text.contains("<base href=\"/dashboards/\">"));
            assert!(text.contains(BUILD_HASH));
            let version = current_ui_version();
            assert!(text.contains(&format!("\"version\":\"{}\"", version.text)));
            assert!(text.contains(&format!("\"version_title\":\"{}\"", version.title)));
        }
    }

    /// The paths of the links `modulepreload` of a shell, in order.
    fn preloads(page: &Bytes) -> Vec<String> {
        let text = std::str::from_utf8(page).unwrap();
        let prefix = format!("<link rel=\"modulepreload\" href=\"static/{BUILD_HASH}/");
        text.lines().filter_map(|l| l.trim().strip_prefix(&prefix)?.strip_suffix("\">").map(str::to_owned)).collect()
    }

    /// The modules that a source imports with a static import, found with a simple scan of the lines that the front
    /// end writes. `build.rs` has its own scanner: the two must give the same result on the files of `web/`.
    fn imports_of(path: &str) -> Vec<String> {
        let source = std::str::from_utf8(find(path).unwrap().bytes).unwrap();
        let mut out = Vec::new();
        let mut rest = source;
        while let Some(i) = rest.find("from") {
            let before_ok = rest[..i].chars().next_back().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
            let after = rest[i + 4..].trim_start();
            if before_ok && after.starts_with(['"', '\'']) && after[1..].starts_with('.') {
                let end = after[1..].find(['"', '\'']).unwrap();
                let target = &after[1..1 + end];
                let mut parts: Vec<&str> = path.split('/').collect();
                parts.pop();
                for part in target.split('/') {
                    match part {
                        "." => {}
                        ".." => {
                            parts.pop();
                        }
                        other => parts.push(other),
                    }
                }
                out.push(parts.join("/"));
            }
            rest = &rest[i + 4..];
        }
        out
    }

    fn closure_of(entry: &str) -> Vec<String> {
        let mut seen = vec![entry.to_owned()];
        let mut at = 0;
        while at < seen.len() {
            for dep in imports_of(&seen[at].clone()) {
                if !seen.contains(&dep) {
                    seen.push(dep);
                }
            }
            at += 1;
        }
        seen
    }

    #[test]
    fn preload_lists_are_the_static_closures_and_the_files_exist() {
        let main = closure_of("js/main.js");
        assert_eq!(PRELOAD_MAIN, &main[1..]);
        for (list, entry) in [(PRELOAD_LIST, "js/list.js"), (PRELOAD_DASHBOARD, "js/dashboard.js")] {
            let want: Vec<String> = closure_of(entry).into_iter().filter(|p| !main.contains(p)).collect();
            assert_eq!(list, want, "{entry}");
            assert_eq!(list[0], entry);
        }
        for path in PRELOAD_MAIN.iter().chain(PRELOAD_LIST).chain(PRELOAD_DASHBOARD) {
            assert!(find_static(BUILD_HASH, path).is_some(), "{path}");
            assert!(path.starts_with("js/") && path.ends_with(".js"), "{path}");
            assert!(!path.starts_with("js/editor/"), "the editor loads on demand: {path}");
        }
        // The routes share nothing that main.js has, and the dashboard page does not load the module of the list page.
        assert!(PRELOAD_LIST.iter().all(|p| !PRELOAD_MAIN.contains(p)));
        assert!(PRELOAD_DASHBOARD.iter().all(|p| !PRELOAD_MAIN.contains(p)));
        assert!(!PRELOAD_DASHBOARD.contains(&"js/list.js"));
    }

    #[test]
    fn shells_preload_main_for_all_routes_and_the_route_modules_for_one() {
        let shells = build_shells("/").expect("shell.html is in web/");
        let main: Vec<String> = PRELOAD_MAIN.iter().map(|p| (*p).to_owned()).collect();
        let with_main = |extra: &[&str]| -> Vec<String> {
            main.iter().cloned().chain(extra.iter().map(|p| (*p).to_owned())).collect()
        };
        assert_eq!(preloads(&shells.base), with_main(&[]));
        assert_eq!(preloads(&shells.list), with_main(PRELOAD_LIST));
        assert_eq!(preloads(&shells.dashboard), with_main(PRELOAD_DASHBOARD));
        // The shell has no other link of this kind: the module of main.js is a script, and a panel module or the editor is not preloaded.
        for page in [&shells.base, &shells.list, &shells.dashboard] {
            let text = std::str::from_utf8(page).unwrap();
            assert_eq!(text.matches("rel=\"modulepreload\"").count(), preloads(page).len());
            assert!(!text.contains("/editor/"), "the editor loads on demand");
            assert!(
                text.contains(&format!("<script type=\"module\" src=\"static/{BUILD_HASH}/js/main.js\"></script>"))
            );
        }
        // The links come after the script of `main.js`, so the request for `main.js` is not queued behind them.
        for page in [&shells.base, &shells.list, &shells.dashboard] {
            let text = std::str::from_utf8(page).unwrap();
            let script = text.find("js/main.js\"></script>").unwrap();
            assert!(text.find("rel=\"modulepreload\"").unwrap() > script);
            assert!(text.find("js/boot_theme.js").unwrap() < script);
        }
        // The bodies differ by the links only.
        let strip = |page: &Bytes| -> String {
            std::str::from_utf8(page)
                .unwrap()
                .lines()
                .filter(|l| !l.contains("modulepreload"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(strip(&shells.base), strip(&shells.list));
        assert_eq!(strip(&shells.base), strip(&shells.dashboard));
    }

    #[test]
    fn the_template_links_the_closure_of_main_in_the_order_of_discovery() {
        let template = std::str::from_utf8(find(SHELL_PATH).unwrap().bytes).unwrap();
        let links: Vec<&str> = template
            .lines()
            .filter_map(|l| {
                l.trim().strip_prefix("<link rel=\"modulepreload\" href=\"static/{{build}}/")?.strip_suffix("\">")
            })
            .collect();
        assert_eq!(links, PRELOAD_MAIN);
        assert_eq!(template.matches("{{preload}}").count(), 1);
    }

    /// The paths of the links of style sheets in a text, in order.
    fn sheet_links(text: &str) -> Vec<String> {
        text.lines().filter_map(|l| sheet_of_line(l).map(str::to_owned)).collect()
    }

    #[test]
    fn the_shell_has_one_style_sheet_and_the_template_keeps_the_sources_in_order() {
        let template = std::str::from_utf8(find(SHELL_PATH).unwrap().bytes).unwrap();
        let sources = sheet_links(template);
        assert_eq!(sources, SHELL_SHEETS);
        assert!(sources.len() > 20, "the template links the style sheets one by one");
        let shells = build_shells("/").expect("shell.html is in web/");
        for page in [&shells.base, &shells.list, &shells.dashboard] {
            let text = std::str::from_utf8(page).unwrap();
            let rendered = text.lines().filter(|l| l.contains("rel=\"stylesheet\"")).count();
            assert_eq!(rendered, 1);
            assert!(text.contains(&format!("<link rel=\"stylesheet\" href=\"static/{BUILD_HASH}/{PAGE_CSS}\">")));
            // The font preload and the first script keep their order around the one link.
            let link = text.find(PAGE_CSS).unwrap();
            assert!(text.find("rel=\"preload\" href=\"static/").unwrap() < link);
            assert!(link < text.find("js/boot_theme.js").unwrap());
        }
    }

    #[test]
    fn page_css_is_a_file_of_the_build_that_joins_the_sheets_in_order() {
        let file = find_static(BUILD_HASH, PAGE_CSS).expect("the build makes the style sheet of the shell");
        assert_eq!(file.content_type, "text/css; charset=utf-8");
        assert!(gzip_copy(file).is_some(), "the style sheet has a gzip copy");
        let css = std::str::from_utf8(file.bytes).unwrap();
        assert!(!css.contains("@import") && !css.contains("@charset"));
        // Each source sheet is in the file, after the comment with its path, in the order of the template.
        let mut at = 0;
        for path in SHELL_SHEETS {
            let source = std::str::from_utf8(find(path).unwrap().bytes).unwrap();
            let marker = format!("/* {path} */\n");
            let start = css[at..].find(&marker).unwrap_or_else(|| panic!("{path} is missing or out of order"))
                + at
                + marker.len();
            // The sheets without a url() are copied byte for byte.
            if !source.contains("url(") {
                assert!(css[start..].starts_with(source), "{path} is not copied as it is");
            }
            at = start + source.len().min(css.len() - start);
        }
        // Each url() names the file that it named in its own sheet: the sheets of the folder css/vendor/chdash/ get a longer path.
        assert!(css.contains("src: url(\"../fonts/inter/InterVariable-main.woff2\")"));
        assert!(css.contains("src: url(\"../fonts/inter/InterVariable-rare.woff2\")"));
        assert!(css.contains("src: url(\"vendor/chdash/fonts/IBMPlexSans-Regular-Latin1.woff2\")"));
        assert!(!css.contains("url(\"fonts/IBMPlex"));
        // The sources stay in the build: the tests and the editor read them.
        assert!(find("css/dashboard.css").is_some() && find("css/vendor/chdash/00-tokens.css").is_some());
    }
}
