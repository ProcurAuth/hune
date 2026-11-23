// SPDX-License-Identifier: MIT OR Apache-2.0
//! Base64 for the public keys (11.9): the standard form with padding (PEM) and the URL form
//! with no padding (JWK, RFC 7515).

const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn value_of(alphabet: &[u8; 64], c: u8) -> Option<u32> {
    alphabet.iter().position(|a| *a == c).map(|p| p as u32)
}

fn decode(text: &str, alphabet: &[u8; 64], padded: bool) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let body_len = if padded {
        if !bytes.len().is_multiple_of(4) {
            return None;
        }
        let pad = bytes.iter().rev().take_while(|b| **b == b'=').count();
        if pad > 2 {
            return None;
        }
        bytes.len() - pad
    } else {
        bytes.len()
    };
    if body_len % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body_len / 4 * 3 + 2);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &c in &bytes[..body_len] {
        acc = (acc << 6) | value_of(alphabet, c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // The unused bits must be zero: one text for one value.
    if acc != 0 {
        return None;
    }
    Some(out)
}

/// Decode the standard form, with the padding that its length needs.
pub fn decode_standard(text: &str) -> Option<Vec<u8>> {
    decode(text, STD, true)
}

/// Decode the URL form with no padding.
pub fn decode_url(text: &str) -> Option<Vec<u8>> {
    decode(text, URL, false)
}

/// Encode in the standard form, with the padding that the length needs.
pub fn encode_standard(data: &[u8]) -> String {
    let mut out = encode_url(data).replace('-', "+").replace('_', "/");
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

/// Encode in the URL form with no padding.
pub fn encode_url(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            3 => (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]),
            2 => (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8),
            _ => u32::from(chunk[0]) << 16,
        };
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(URL[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = encode_url(&data);
            assert_eq!(decode_url(&encoded).unwrap(), data, "{len}");
        }
    }

    #[test]
    fn standard_form_needs_padding_and_url_form_has_none() {
        assert_eq!(decode_standard("TWFu").unwrap(), b"Man");
        assert_eq!(decode_standard("TWE=").unwrap(), b"Ma");
        assert_eq!(decode_standard("TQ==").unwrap(), b"M");
        assert!(decode_standard("TWE").is_none());
        assert!(decode_standard("TQ=").is_none());
        assert!(decode_standard("T===").is_none());
        assert_eq!(decode_url("TWE").unwrap(), b"Ma");
        assert!(decode_url("TWE=").is_none());
        assert!(decode_url("A").is_none());
        assert!(decode_url("a+b").is_none());
        assert!(decode_standard("a-b=").is_none());
    }

    #[test]
    fn non_zero_trailing_bits_are_refused() {
        // "TR" would decode to 'M' with trailing bits that are not zero.
        assert!(decode_url("TR").is_none());
        assert_eq!(decode_url("TQ").unwrap(), b"M");
    }
}
