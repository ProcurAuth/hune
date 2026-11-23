//! Build script of the crate `hune`.
//!
//! It embeds each file of the directory `web/` (9.3). It makes the build list, the build hash and a
//! Rust table of the files. It also gives the commit and the target to `hune version` (17.2).
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
    fs::write(out_dir.join("web_table.rs"), table).unwrap_or_else(|e| fail(&format!("cannot write the table: {e}")));

    let commit = std::env::var("HUNE_BUILD_COMMIT").unwrap_or_else(|_| "unknown".to_owned());
    let commit = if commit.trim().is_empty() { "unknown".to_owned() } else { commit.trim().to_owned() };
    println!("cargo:rustc-env=HUNE_BUILD_COMMIT={commit}");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned());
    println!("cargo:rustc-env=HUNE_TARGET={target}");
}
