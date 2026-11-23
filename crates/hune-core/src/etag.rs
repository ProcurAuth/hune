// SPDX-License-Identifier: MIT OR Apache-2.0
//! The etag of 4.39: the SHA-256 of the canonical bytes, as 64 lower-case hex characters.

use sha2::{Digest, Sha256};
use std::fmt;

/// An etag: 32 bytes. The text form has 64 lower-case hex characters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Etag([u8; 32]);

/// The error of a text that is not 64 lower-case hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtagError;

impl fmt::Display for EtagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected 64 lower-case hexadecimal characters")
    }
}

impl std::error::Error for EtagError {}

impl Etag {
    /// Computes the SHA-256 of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Etag(out)
    }

    /// Makes an etag from 32 raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Etag(bytes)
    }

    /// Gives the 32 raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Reads the text form: exactly 64 lower-case hex characters.
    pub fn parse_hex(s: &str) -> Result<Self, EtagError> {
        let b = s.as_bytes();
        if b.len() != 64 {
            return Err(EtagError);
        }
        let mut out = [0u8; 32];
        for (i, chunk) in b.chunks(2).enumerate() {
            let hi = hex_val(chunk[0]).ok_or(EtagError)?;
            let lo = hex_val(chunk[1]).ok_or(EtagError)?;
            out[i] = (hi << 4) | lo;
        }
        Ok(Etag(out))
    }

    /// Gives the text form: 64 lower-case hex characters.
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(64);
        for byte in self.0 {
            s.push(HEX[(byte >> 4) as usize] as char);
            s.push(HEX[(byte & 0x0f) as usize] as char);
        }
        s
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

impl fmt::Display for Etag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Etag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Etag({})", self.to_hex())
    }
}

impl serde::Serialize for Etag {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let e = Etag::of(b"");
        assert_eq!(e.to_hex(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(Etag::parse_hex(&e.to_hex()).unwrap(), e);
    }

    #[test]
    fn refused_forms() {
        assert!(Etag::parse_hex("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855").is_err());
        assert!(Etag::parse_hex("abc").is_err());
        assert!(Etag::parse_hex(&"g".repeat(64)).is_err());
    }
}
