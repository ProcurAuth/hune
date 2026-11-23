// SPDX-License-Identifier: MIT OR Apache-2.0
//! The strict query string of an API route (6.2): an unknown or repeated parameter gives
//! `400 bad_request`.

use super::error::ApiError;
use std::collections::BTreeMap;

/// The parameters of one request.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query(BTreeMap<String, String>);

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes a component of a query string: `%XX` and `+` as a space. A bad escape or a result
/// that is not UTF-8 gives `None`.
pub fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hi = hex(*bytes.get(i + 1)?)?;
                let lo = hex(*bytes.get(i + 2)?)?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

impl Query {
    /// Parses the raw query string against the list of accepted names.
    pub fn parse(raw: Option<&str>, allowed: &[&str]) -> Result<Query, ApiError> {
        let mut map = BTreeMap::new();
        let Some(raw) = raw else { return Ok(Query(map)) };
        for pair in raw.split('&').filter(|p| !p.is_empty()) {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let bad = || ApiError::bad_request(format!("The query parameter {name:?} is not valid."));
            let name = percent_decode(name).ok_or_else(bad)?;
            let value = percent_decode(value).ok_or_else(bad)?;
            if !allowed.contains(&name.as_str()) {
                return Err(ApiError::bad_request(format!("The query parameter {name:?} is not known on this route.")));
            }
            if map.insert(name.clone(), value).is_some() {
                return Err(ApiError::bad_request(format!("The query parameter {name:?} occurs more than one time.")));
            }
        }
        Ok(Query(map))
    }

    /// Gives the value of a parameter.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// Reads a flag: absent or `false` is false, `true` is true. Another value gives `400`.
    pub fn flag(&self, name: &str) -> Result<bool, ApiError> {
        match self.get(name) {
            None | Some("false") => Ok(false),
            Some("true") => Ok(true),
            Some(other) => Err(ApiError::bad_request(format!(
                "The query parameter {name} must be true or false; value {other:?}."
            ))),
        }
    }

    /// Tells if the request has no parameter.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::error::Code;

    #[test]
    fn parses_known_parameters() {
        let q = Query::parse(Some("folder=transit&q=a%20b+c&limit=5"), &["folder", "q", "limit", "after"]).unwrap();
        assert_eq!(q.get("folder"), Some("transit"));
        assert_eq!(q.get("q"), Some("a b c"));
        assert_eq!(q.get("limit"), Some("5"));
        assert_eq!(q.get("after"), None);
        assert!(Query::parse(None, &[]).unwrap().is_empty());
        assert!(Query::parse(Some(""), &[]).unwrap().is_empty());
    }

    #[test]
    fn unknown_and_repeated_parameters_are_refused() {
        let e = Query::parse(Some("x=1"), &["a"]).unwrap_err();
        assert_eq!(e.code, Code::BadRequest);
        let e = Query::parse(Some("a=1&a=2"), &["a"]).unwrap_err();
        assert_eq!(e.code, Code::BadRequest);
        let e = Query::parse(Some("a=1"), &[]).unwrap_err();
        assert_eq!(e.code, Code::BadRequest);
        // A name that is the same after the decoding is a repeat.
        assert!(Query::parse(Some("a=1&%61=2"), &["a"]).is_err());
    }

    #[test]
    fn bad_escapes_are_refused() {
        assert!(Query::parse(Some("a=%zz"), &["a"]).is_err());
        assert!(Query::parse(Some("a=%2"), &["a"]).is_err());
        assert!(Query::parse(Some("a=%ff"), &["a"]).is_err());
    }

    #[test]
    fn flags() {
        let q = Query::parse(Some("download=true&validate_only=maybe"), &["download", "validate_only", "x"]).unwrap();
        assert!(q.flag("download").unwrap());
        assert!(q.flag("validate_only").is_err());
        assert!(!q.flag("x").unwrap());
        let q = Query::parse(Some("download=false"), &["download"]).unwrap();
        assert!(!q.flag("download").unwrap());
    }
}
