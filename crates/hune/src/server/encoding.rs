// SPDX-License-Identifier: MIT OR Apache-2.0
//! The header `Accept-Encoding` of a request (RFC 9110, 12.5.3) for the gzip copies of the static files (9.3).
//!
//! The rule is strict. A request accepts gzip only when the header names `gzip` or `x-gzip` with a weight
//! above 0, or names no gzip and has `*` with a weight above 0. A header that does not parse gives the
//! file with no content coding, which each client accepts.

use http::header::{ACCEPT_ENCODING, HeaderMap};

/// Tells if the request accepts the content coding gzip. An absent header gives false, so a client that
/// asks for nothing gets the file as it is.
pub fn accepts_gzip(headers: &HeaderMap) -> bool {
    // `Some(true)` when each item that names the coding has a weight above 0.
    let (mut gzip, mut star): (Option<bool>, Option<bool>) = (None, None);
    for value in headers.get_all(ACCEPT_ENCODING) {
        let Ok(text) = value.to_str() else { return false };
        for item in text.split(',') {
            let item = item.trim_matches(is_ows);
            if item.is_empty() {
                continue;
            }
            let Some((coding, weight)) = parse_item(item) else { return false };
            let accepted = weight > 0;
            if coding.eq_ignore_ascii_case("gzip") || coding.eq_ignore_ascii_case("x-gzip") {
                gzip = Some(gzip.unwrap_or(true) && accepted);
            } else if coding == "*" {
                star = Some(star.unwrap_or(true) && accepted);
            }
        }
    }
    gzip.or(star).unwrap_or(false)
}

fn is_ows(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// A `tchar` of RFC 9110, 5.6.2.
fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Reads one item: a coding and an optional weight `q`. Gives the coding and the weight in thousandths.
/// Another parameter, a second weight or a bad weight gives `None`.
fn parse_item(item: &str) -> Option<(&str, u16)> {
    let mut parts = item.split(';');
    let coding = parts.next()?.trim_matches(is_ows);
    if coding.is_empty() || !coding.bytes().all(is_tchar) {
        return None;
    }
    let mut weight = None;
    for param in parts {
        let (name, value) = param.trim_matches(is_ows).split_once('=')?;
        if !name.eq_ignore_ascii_case("q") || weight.is_some() {
            return None;
        }
        weight = Some(parse_qvalue(value)?);
    }
    Some((coding, weight.unwrap_or(1000)))
}

/// Reads a `qvalue` of RFC 9110, 12.4.2: `0` with at most three decimals, or `1` with at most three zeros.
fn parse_qvalue(text: &str) -> Option<u16> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match whole {
        "0" => {
            let mut thousandths = 0u16;
            for (i, b) in fraction.bytes().enumerate() {
                thousandths += u16::from(b - b'0') * [100, 10, 1][i];
            }
            Some(thousandths)
        }
        "1" if fraction.bytes().all(|b| b == b'0') => Some(1000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    fn accepts(values: &[&str]) -> bool {
        let mut h = HeaderMap::new();
        for v in values {
            h.append(ACCEPT_ENCODING, HeaderValue::from_str(v).unwrap());
        }
        accepts_gzip(&h)
    }

    #[test]
    fn the_codings_and_the_weights() {
        for yes in [
            "gzip",
            "GZIP",
            "x-gzip",
            "gzip, deflate, br, zstd",
            "br;q=1.0, gzip;q=0.8, *;q=0.1",
            "gzip;q=0.001",
            "gzip ; q=0.5",
            "gzip;Q=1",
            "gzip;q=1.000",
            "gzip;q=1.",
            "*",
            "*;q=0.5",
            "identity, *;q=1",
            "deflate,,gzip",
        ] {
            assert!(accepts(&[yes]), "{yes}");
        }
        for no in [
            "",
            "identity",
            "br, deflate",
            "gzip;q=0",
            "gzip;q=0.000",
            "*;q=0",
            "gzip;q=0, *",
            "*, gzip;q=0",
            "gzip, gzip;q=0",
            "gzip;q=1.001",
            "gzip;q=2",
            "gzip;q=0.0001",
            "gzip;q=",
            "gzip;q=.5",
            "gzip;level=9",
            "gzip;q=0.5;q=0.5",
            "gzip;q = 0.5",
            "gz ip",
            "gzip/1",
            "br, gzip;q=bad",
        ] {
            assert!(!accepts(&[no]), "{no}");
        }
        assert!(!accepts(&[]));
    }

    #[test]
    fn two_header_lines_join_into_one_list() {
        assert!(accepts(&["br", "gzip"]));
        assert!(!accepts(&["gzip", "gzip;q=0"]));
        assert!(!accepts(&["gzip", "x;q=bad"]));
    }

    #[test]
    fn a_header_that_is_not_text_gives_no_gzip() {
        let mut h = HeaderMap::new();
        h.insert(ACCEPT_ENCODING, HeaderValue::from_bytes(b"gzip\xff").unwrap());
        assert!(!accepts_gzip(&h));
    }
}
