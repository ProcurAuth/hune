//! The measure of the encoder for the size analysis of the embedded files. It does not
//! run by default. Run it in the release profile:
//!
//! `cargo test --release -p hune measure_ -- --ignored --nocapture`
//!
//! It writes one line "<path> <raw bytes> <gzip bytes> <microseconds>" for each embedded file of a text
//! type, then the totals and the time of the whole table, which is the work of the start.

use super::*;
use crate::web_assets::{FILES, gzip_copies, is_compressible};
use std::time::Instant;

#[test]
#[ignore = "a measure for the analysis; run it in the release profile"]
fn measure_the_encoder_on_the_embedded_files() {
    let (mut raw, mut packed) = (0usize, 0usize);
    let start = Instant::now();
    for f in FILES.iter().filter(|f| is_compressible(f.content_type)) {
        let t = Instant::now();
        let gz = gzip(f.bytes).unwrap();
        println!("{}\t{}\t{}\t{}", f.path, f.bytes.len(), gz.len(), t.elapsed().as_micros());
        raw += f.bytes.len();
        packed += gz.len();
    }
    let all = start.elapsed();
    println!("total\t{raw}\t{packed}\t{}", all.as_micros());
    // The table of the server: the same work, through the cache of the module web_assets.
    let t = Instant::now();
    let copies = gzip_copies();
    let kept = copies.iter().flatten().count();
    let bytes: usize = copies.iter().flatten().map(|c| c.len()).sum();
    println!("table\t{kept} copies\t{bytes} bytes\t{} us", t.elapsed().as_micros());
}

/// The same measure on a copy of `web/` in another directory, so that the system gzip and this encoder read
/// the same bytes. The directory is in the variable `HUNE_GZIP_MEASURE_DIR` (a test may read the environment).
#[test]
#[ignore = "a measure for the analysis; run it in the release profile"]
fn measure_the_encoder_on_a_directory() {
    let Ok(root) = std::env::var("HUNE_GZIP_MEASURE_DIR") else {
        eprintln!("note: HUNE_GZIP_MEASURE_DIR is not set");
        return;
    };
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() { walk(&p, out) } else { out.push(p) }
        }
    }
    let root = std::path::PathBuf::from(root);
    let mut files = Vec::new();
    walk(&root, &mut files);
    let start = Instant::now();
    for p in files {
        let name = p.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
        if name.ends_with(".woff2") || name.ends_with(".png") {
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        let t = Instant::now();
        let gz = gzip(&data).unwrap();
        println!("{name}\t{}\t{}\t{}", data.len(), gz.len(), t.elapsed().as_micros());
    }
    println!("time\t{} us", start.elapsed().as_micros());
}
