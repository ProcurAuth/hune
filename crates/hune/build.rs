// SPDX-License-Identifier: MIT OR Apache-2.0
//! Build script of the crate `hune`.
//!
//! It embeds each file of the directory `web/` (9.3). It makes the build list, the build hash and a
//! Rust table of the files. It also makes the preload lists of the shell (9.6) and gives the commit and
//! the target to `hune version` (17.2).
//! This script reads the environment: a build script is not product code (R2).

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// The extensions that 9.3 permits, with their `Content-Type`.
const CONTENT_TYPES: [(&str, &str); 7] = [
    ("html", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("json", "application/json"),
    ("woff2", "font/woff2"),
    ("png", "image/png"),
];

/// A file with the name `LICENSE` has no extension. 9.2 and 9.24 name one such file for the copied
/// chart engine, so this one name is permitted as plain text.
const LICENSE_NAME: &str = "LICENSE";
const LICENSE_TYPE: &str = "text/plain; charset=utf-8";

struct Entry {
    /// The path relative to `web/`, with `/` as separator.
    path: String,
    absolute: PathBuf,
    content_type: &'static str,
    sha256: String,
}

fn fail(message: &str) -> ! {
    println!("cargo:warning=hune build: {message}");
    eprintln!("error: hune build: {message}");
    std::process::exit(1);
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn content_type_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == LICENSE_NAME {
        return Some(LICENSE_TYPE);
    }
    let (_, ext) = name.rsplit_once('.')?;
    CONTENT_TYPES.iter().find(|(e, _)| *e == ext).map(|(_, t)| *t)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) {
    let read = fs::read_dir(dir).unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", dir.display())));
    for item in read {
        let item = item.unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", dir.display())));
        let path = item.path();
        let meta = fs::symlink_metadata(&path)
            .unwrap_or_else(|e| fail(&format!("cannot read the metadata of {}: {e}", path.display())));
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_text = rel
            .components()
            .map(|c| c.as_os_str().to_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_else(|| fail(&format!("the path {} is not UTF-8", path.display())))
            .join("/");
        println!("cargo:rerun-if-changed={}", path.display());
        if meta.file_type().is_symlink() {
            fail(&format!("web/{rel_text} is a symbolic link; 9.3 refuses symbolic links"));
        } else if meta.is_dir() {
            walk(root, &path, out);
        } else if meta.is_file() {
            let content_type = content_type_of(&rel_text).unwrap_or_else(|| {
                fail(&format!(
                    "web/{rel_text} has an extension that 9.3 does not permit; \
                     permitted: .html .js .css .svg .json .woff2 .png"
                ))
            });
            let data = fs::read(&path).unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", path.display())));
            let absolute =
                fs::canonicalize(&path).unwrap_or_else(|e| fail(&format!("cannot resolve {}: {e}", path.display())));
            out.push(Entry { path: rel_text, absolute, content_type, sha256: hex(&Sha256::digest(&data)) });
        } else {
            fail(&format!("web/{rel_text} is not a regular file"));
        }
    }
}

/// The relative modules that a JavaScript source imports with a static import: `import ... from "./x.js"`,
/// `export ... from "./x.js"` and `import "./x.js"`. A dynamic `import()` is not a static import. The rule is
/// the one of the front unit test `tests/frontend/shell_preload.test.mjs`, which has the same regular expression:
/// the word `from`, then a quote, then a text that starts with `.` and has no quote, then a quote.
fn static_imports(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let is_quote = |b: u8| b == b'"' || b == b'\'';
    // After the position `at`, skips white space, then reads a quoted text that starts with a dot.
    let quoted_path = |mut at: usize| -> Option<String> {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= bytes.len() || !is_quote(bytes[at]) || bytes.get(at + 1) != Some(&b'.') {
            return None;
        }
        let start = at + 1;
        let mut end = start;
        while end < bytes.len() && !is_quote(bytes[end]) {
            end += 1;
        }
        if end >= bytes.len() || end == start + 1 {
            return None;
        }
        source.get(start..end).map(str::to_owned)
    };
    let mut found: Vec<(usize, String)> = Vec::new();
    for (i, _) in source.match_indices("from") {
        let word_before = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if !word_before && let Some(path) = quoted_path(i + 4) {
            found.push((i, path));
        }
    }
    for (i, _) in source.match_indices("import") {
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        if line_start && let Some(path) = quoted_path(i + 6) {
            found.push((i, path));
        }
    }
    found.sort_by_key(|(i, _)| *i);
    found.into_iter().map(|(_, p)| p).collect()
}

/// Joins a relative module path to the folder of a file and removes `.` and `..`. Both paths use `/`.
fn join_module(from_file: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = from_file.split('/').collect();
    parts.pop();
    for part in relative.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// The static closure of a module, in the order in which a browser finds the files: breadth first, and the
/// imports of a file in source order. The entry is the first item. A missing file stops the build.
fn static_closure(entry: &str, entries: &[Entry]) -> Vec<String> {
    let mut seen = vec![entry.to_owned()];
    let mut next = 0;
    while next < seen.len() {
        let file = seen[next].clone();
        next += 1;
        let item = entries
            .iter()
            .find(|e| e.path == file)
            .unwrap_or_else(|| fail(&format!("web/{file} is imported, but it is not a file of web/")));
        let data =
            fs::read(&item.absolute).unwrap_or_else(|e| fail(&format!("cannot read {}: {e}", item.absolute.display())));
        let source = String::from_utf8(data).unwrap_or_else(|_| fail(&format!("web/{file} is not UTF-8")));
        for relative in static_imports(&source) {
            let target = join_module(&file, &relative)
                .unwrap_or_else(|| fail(&format!("web/{file} imports {relative}, which is outside web/")));
            if !seen.contains(&target) {
                seen.push(target);
            }
        }
    }
    seen
}

/// A Rust list of paths for the generated table.
fn rust_list(name: &str, doc: &str, paths: &[String]) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "/// {doc}");
    let _ = writeln!(text, "pub static {name}: &[&str] = &[");
    for p in paths {
        let _ = writeln!(text, "    {p:?},");
    }
    let _ = writeln!(text, "];");
    text
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=HUNE_BUILD_COMMIT");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| fail("no CARGO_MANIFEST_DIR")));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| fail("no OUT_DIR")));
    let web = manifest.join("../../web");
    if !web.is_dir() {
        fail(&format!("the directory {} does not exist", web.display()));
    }
    println!("cargo:rerun-if-changed={}", web.display());

    let mut entries = Vec::new();
    walk(&web, &web, &mut entries);
    entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));

    // The build list (9.3): one line "<path> <sha256>" for each file, each line ends with LF.
    let mut list = String::new();
    for e in &entries {
        let _ = writeln!(list, "{} {}", e.path, e.sha256);
    }
    let build_hash = hex(&Sha256::digest(list.as_bytes()))[..16].to_owned();

    let mut table = String::new();
    let _ = writeln!(table, "/// The build hash of 9.3.");
    let _ = writeln!(table, "pub const BUILD_HASH: &str = {build_hash:?};");
    let _ = writeln!(table, "/// The embedded files, in byte order of the path.");
    let _ = writeln!(table, "pub static FILES: &[WebFile] = &[");
    for e in &entries {
        let _ = writeln!(
            table,
            "    WebFile {{ path: {:?}, content_type: {:?}, bytes: include_bytes!({:?}), sha256: {:?} }},",
            e.path,
            e.content_type,
            e.absolute.to_string_lossy(),
            e.sha256
        );
    }
    let _ = writeln!(table, "];");
    // The preload lists of the shell (9.6). The shell template holds the closure of `main.js` as links. The list of a
    // route is the closure of its module, without the files that every shell preloads.
    let main_closure = static_closure("js/main.js", &entries);
    let main_files: Vec<String> = main_closure[1..].to_vec();
    let route_files = |entry: &str| -> Vec<String> {
        static_closure(entry, &entries).into_iter().filter(|p| !main_closure.contains(p)).collect()
    };
    table.push_str(&rust_list(
        "PRELOAD_MAIN",
        "The static closure of `js/main.js` without `js/main.js`, in the order of discovery (9.6).",
        &main_files,
    ));
    table.push_str(&rust_list(
        "PRELOAD_LIST",
        "The modules that the list page needs and that `PRELOAD_MAIN` does not have (9.6).",
        &route_files("js/list.js"),
    ));
    table.push_str(&rust_list(
        "PRELOAD_DASHBOARD",
        "The modules that the dashboard page needs and that `PRELOAD_MAIN` does not have (9.6).",
        &route_files("js/dashboard.js"),
    ));
    fs::write(out_dir.join("web_table.rs"), table).unwrap_or_else(|e| fail(&format!("cannot write the table: {e}")));

    let commit = std::env::var("HUNE_BUILD_COMMIT").unwrap_or_else(|_| "unknown".to_owned());
    let commit = if commit.trim().is_empty() { "unknown".to_owned() } else { commit.trim().to_owned() };
    println!("cargo:rustc-env=HUNE_BUILD_COMMIT={commit}");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned());
    println!("cargo:rustc-env=HUNE_TARGET={target}");
}
