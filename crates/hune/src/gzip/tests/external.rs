// SPDX-License-Identifier: MIT OR Apache-2.0
//! Round trips through two independent decoders: the system `gzip -d` and `zlib.gunzipSync` of Node.js.
//!
//! The inputs are the edge cases, seeded random inputs and each embedded file of a text type, with the
//! largest one. The encoder is deterministic and the files are fixed at build time, so the bytes that these
//! tests decode are the bytes that the server sends. The test needs `gzip`. When `node` is not on the path
//! (the builder image of 17.2 has no Node.js), the Node.js part writes a note and does nothing.

use super::*;
use crate::web_assets::{FILES, is_compressible};
use sha2::{Digest, Sha256};
use std::io::{ErrorKind, Read, Write};
use std::process::{Command, Stdio};

/// Runs a program with `input` on its standard input and gives its standard output, or `None` when the
/// program does not exist.
fn run(program: &str, args: &[&str], input: Vec<u8>) -> Option<Vec<u8>> {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => panic!("{program}: {e}"),
    };
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    writer.join().unwrap().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "{program} failed: {status}: {err}");
    Some(out)
}

/// The inputs of the round trips: the edge cases, 4 seeds of random inputs and the embedded text files.
fn all_inputs() -> Vec<(String, Vec<u8>)> {
    let mut inputs: Vec<(String, Vec<u8>)> =
        edge_inputs().into_iter().enumerate().map(|(i, d)| (format!("edge {i}"), d)).collect();
    for seed in 1..=4 {
        for (i, d) in random_inputs(seed).into_iter().enumerate() {
            inputs.push((format!("random seed {seed} kind {i}"), d));
        }
    }
    for f in FILES.iter().filter(|f| is_compressible(f.content_type)) {
        inputs.push((f.path.to_owned(), f.bytes.to_vec()));
    }
    inputs
}

#[test]
fn system_gzip_decodes_each_output() {
    let inputs = all_inputs();
    // One run of `gzip -d` reads the members one after the other. It checks the CRC and the size of each.
    let mut members = Vec::new();
    let mut want = Vec::new();
    for (_, data) in &inputs {
        members.extend_from_slice(&gzip(data).unwrap());
        want.extend_from_slice(data);
    }
    let out = run("gzip", &["-d", "-c"], members).expect("the system gzip is necessary for this test");
    assert!(out == want, "gzip -d gives other bytes ({} bytes, want {})", out.len(), want.len());
    // Each member alone, so a failure names its input.
    for (name, data) in inputs.iter().filter(|(n, _)| n.starts_with("edge")) {
        let out = run("gzip", &["-d", "-c"], gzip(data).unwrap()).unwrap();
        assert!(&out == data, "{name}");
    }
}

/// The Node.js program: frames of a 4-byte little-endian length and a gzip member on the standard input,
/// and one line "<size> <sha256>" for each frame on the standard output.
const NODE_DECODER: &str = r#"
const zlib = require("node:zlib");
const crypto = require("node:crypto");
const parts = [];
process.stdin.on("data", (c) => parts.push(c));
process.stdin.on("end", () => {
  const all = Buffer.concat(parts);
  const lines = [];
  let at = 0;
  while (at < all.length) {
    const n = all.readUInt32LE(at);
    at += 4;
    const member = all.subarray(at, at + n);
    at += n;
    try {
      const raw = zlib.gunzipSync(member);
      lines.push(raw.length + " " + crypto.createHash("sha256").update(raw).digest("hex"));
    } catch (e) {
      lines.push("error " + e.code);
    }
  }
  process.stdout.write(lines.join("\n") + "\n");
});
"#;

#[test]
fn node_zlib_decodes_each_output() {
    let inputs = all_inputs();
    let mut frames = Vec::new();
    for (_, data) in &inputs {
        let gz = gzip(data).unwrap();
        frames.extend_from_slice(&(gz.len() as u32).to_le_bytes());
        frames.extend_from_slice(&gz);
    }
    let Some(out) = run("node", &["-e", NODE_DECODER], frames) else {
        eprintln!("note: node is not on the path; the Node.js round trip did not run");
        return;
    };
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), inputs.len());
    for ((name, data), line) in inputs.iter().zip(lines) {
        let hex: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(line, format!("{} {hex}", data.len()), "{name}");
    }
}

#[test]
fn the_largest_embedded_file_makes_a_round_trip_and_gets_smaller() {
    let largest = FILES.iter().filter(|f| is_compressible(f.content_type)).max_by_key(|f| f.bytes.len()).unwrap();
    let gz = gzip(largest.bytes).unwrap();
    assert!(gz.len() * 2 < largest.bytes.len(), "{}: {} -> {}", largest.path, largest.bytes.len(), gz.len());
    let out = run("gzip", &["-d", "-c"], gz).expect("the system gzip is necessary for this test");
    assert!(out == largest.bytes, "{}", largest.path);
}
