//! A gzip encoder (RFC 1952) with DEFLATE (RFC 1951), for the static files of the front end (9.3).
//!
//! The server compresses each static file of a text type one time at start, when `server.compress_static`
//! is true (13.8). The encoder never sees a request, an API answer or a secret. It has no `unsafe` code
//! and no dependency.
//!
//! - The output is deterministic: the same input gives the same bytes. The header has no time and no name.
//! - The memory is the output and two fixed tables of 32768 positions.
//! - The time for each input byte has a limit (see `lz77`).
//!
//! The tests decode the output with the system `gzip -d` and with the zlib of Node.js.

mod bits;
mod deflate;
mod huffman;
mod lz77;

/// The largest input that `gzip` accepts: 16 MiB. The largest static file is far below it.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// The gzip header (RFC 1952, 2.3): ID1, ID2, CM = 8 (deflate), FLG = 0, MTIME = 0, XFL = 0 and
/// OS = 255 (unknown). A zero MTIME and no file name keep the output the same on each machine.
const HEADER: [u8; 10] = [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];

/// The table of the CRC-32 of RFC 1952 (polynomial 0xEDB88320, bits in reverse order).
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// The CRC-32 of RFC 1952, 8.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = u32::MAX;
    for &b in data {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// Compresses an input into one gzip member. Gives `None` for an input above `MAX_INPUT_BYTES`.
pub fn gzip(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > MAX_INPUT_BYTES {
        return None;
    }
    let size = u32::try_from(data.len()).ok()?;
    let body = deflate::deflate(data);
    let mut out = Vec::with_capacity(HEADER.len() + body.len() + 8);
    out.extend_from_slice(&HEADER);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests;
