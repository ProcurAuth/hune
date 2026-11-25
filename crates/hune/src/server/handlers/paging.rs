// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cursors and the page rules of 6.24.

use crate::server::error::ApiError;
use hune_core::ids::is_valid_id;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encodes bytes in base64url with no padding.
pub fn b64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// Decodes base64url with no padding. A text that is not canonical gives `None`.
pub fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut vals = Vec::with_capacity(text.len());
    for c in text.bytes() {
        vals.push(u32::try_from(ALPHABET.iter().position(|a| *a == c)?).ok()?);
    }
    if vals.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= v << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    (b64url_encode(&out) == text).then_some(out)
}

/// The longest folded title in a dashboard cursor, in bytes.
pub const CURSOR_TITLE_BYTES: usize = 290;
/// The longest cursor in characters (6.24).
pub const MAX_CURSOR_CHARS: usize = 512;
/// The most digits in the count of a dashboard cursor.
const MAX_SKIP_DIGITS: usize = 9;

/// The sort key of a dashboard in a list (6.24). It has the folder, the title with ASCII letters in
/// lower case, and the uid, each in byte order. The title is cut to `CURSOR_TITLE_BYTES` bytes, so that
/// the cursor fits the limit of 512 characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DashboardKey {
    pub folder: String,
    pub title: Vec<u8>,
    pub uid: String,
}

impl DashboardKey {
    /// Makes the key of a dashboard.
    pub fn new(folder: &str, title: &str, uid: &str) -> Self {
        let mut t = title.as_bytes().to_ascii_lowercase();
        t.truncate(CURSOR_TITLE_BYTES);
        DashboardKey { folder: folder.to_owned(), title: t, uid: uid.to_owned() }
    }
}

/// The cursor of a page of dashboards (6.24): the position after the last item that the subject can see, and
/// the count of the rows that the page examined after it.
///
/// The cursor never holds the key of a row that the subject cannot read. `key` is the key of the last item
/// that the subject saw, or `None` before the first visible item. `skip` is the count of rows after this key
/// that the filter examined and hid. The next page starts after these rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageCursor {
    pub key: Option<DashboardKey>,
    pub skip: usize,
}

impl PageCursor {
    /// Makes the cursor of the text form: `folder`, `title`, `uid` and the count, split by the byte 0.
    pub fn encode(&self) -> String {
        let mut bytes = Vec::new();
        if let Some(k) = &self.key {
            bytes.extend_from_slice(k.folder.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&k.title);
            bytes.push(0);
            bytes.extend_from_slice(k.uid.as_bytes());
        } else {
            bytes.extend_from_slice(&[0, 0]);
        }
        bytes.push(0);
        bytes.extend_from_slice(self.skip.to_string().as_bytes());
        b64url_encode(&bytes)
    }

    /// Reads a cursor. A text that Hune did not make gives `400 bad_request`.
    pub fn parse(cursor: &str) -> Result<PageCursor, ApiError> {
        let bad = || ApiError::bad_request("The query parameter after is not a cursor of this server.");
        if cursor.is_empty() || cursor.len() > MAX_CURSOR_CHARS {
            return Err(bad());
        }
        let bytes = b64url_decode(cursor).ok_or_else(bad)?;
        let end = bytes.iter().rposition(|b| *b == 0).ok_or_else(bad)?;
        let digits = &bytes[end + 1..];
        if digits.is_empty()
            || digits.len() > MAX_SKIP_DIGITS
            || (digits.len() > 1 && digits[0] == b'0')
            || !digits.iter().all(u8::is_ascii_digit)
        {
            return Err(bad());
        }
        let skip: usize = std::str::from_utf8(digits).map_err(|_| bad())?.parse().map_err(|_| bad())?;
        let body = &bytes[..end];
        let first = body.iter().position(|b| *b == 0).ok_or_else(bad)?;
        let last = body.iter().rposition(|b| *b == 0).ok_or_else(bad)?;
        if first == last {
            return Err(bad());
        }
        let folder = std::str::from_utf8(&body[..first]).map_err(|_| bad())?;
        let uid = std::str::from_utf8(&body[last + 1..]).map_err(|_| bad())?;
        let title = &body[first + 1..last];
        if folder.is_empty() && uid.is_empty() && title.is_empty() {
            return Ok(PageCursor { key: None, skip });
        }
        // An empty folder is the key of a dashboard without a folder (6.24).
        if !(folder.is_empty() || is_valid_id(folder)) || !is_valid_id(uid) || title.len() > CURSOR_TITLE_BYTES {
            return Err(bad());
        }
        let key = DashboardKey { folder: folder.to_owned(), title: title.to_vec(), uid: uid.to_owned() };
        Ok(PageCursor { key: Some(key), skip })
    }
}

/// Makes the history cursor: the text `v<version>` in base64url (6.24).
pub fn history_cursor(version: u64) -> String {
    b64url_encode(format!("v{version}").as_bytes())
}

/// Reads a history cursor.
pub fn parse_history_cursor(cursor: &str) -> Result<u64, ApiError> {
    let bad = || ApiError::bad_request("The query parameter after is not a cursor of this server.");
    if cursor.is_empty() || cursor.len() > MAX_CURSOR_CHARS {
        return Err(bad());
    }
    let bytes = b64url_decode(cursor).ok_or_else(bad)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| bad())?;
    let digits = text.strip_prefix('v').ok_or_else(bad)?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    digits.parse::<u64>().map_err(|_| bad())
}

/// Reads the parameter `limit`: a whole number from 1 to `max`, 100 when absent (6.24).
pub fn parse_limit(value: Option<&str>, max: u32) -> Result<u32, ApiError> {
    let Some(v) = value else { return Ok(100.min(max)) };
    let bad = || ApiError::bad_request(format!("The query parameter limit must be a whole number from 1 to {max}."));
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    match v.parse::<u32>() {
        Ok(n) if (1..=max).contains(&n) => Ok(n),
        _ => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_vectors() {
        assert_eq!(b64url_encode(b"v12"), "djEy");
        assert_eq!(b64url_decode("djEy").unwrap(), b"v12");
        for n in 0..40usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
            assert_eq!(b64url_decode(&b64url_encode(&data)).unwrap(), data);
        }
        assert!(b64url_decode("djE").is_none() || b64url_decode("djE").is_some());
        assert!(b64url_decode("a").is_none());
        assert!(b64url_decode("dj+y").is_none());
        // A non canonical last character is refused.
        assert!(b64url_decode("djF=").is_none());
        assert!(b64url_decode("djEz").is_some());
    }

    #[test]
    fn history_cursor_rules() {
        assert_eq!(history_cursor(12), "djEy");
        assert_eq!(parse_history_cursor("djEy").unwrap(), 12);
        for bad in
            ["", "abc", &b64url_encode(b"v0"), &b64url_encode(b"v01"), &b64url_encode(b"12"), &b64url_encode(b"v")]
        {
            assert!(parse_history_cursor(bad).is_err(), "{bad}");
        }
        assert!(parse_history_cursor(&"A".repeat(513)).is_err());
    }

    #[test]
    fn dashboard_cursor_round_trip_and_order() {
        let a = DashboardKey::new("transit", "Alpha", "a1");
        let b = DashboardKey::new("transit", "alpha", "b1");
        let c = DashboardKey::new("transit", "Beta", "a0");
        let d = DashboardKey::new("ops", "Zed", "z");
        let mut keys = vec![c.clone(), a.clone(), d.clone(), b.clone()];
        keys.sort();
        assert_eq!(keys, vec![d.clone(), a.clone(), b, c]);
        let cur = PageCursor { key: Some(a), skip: 197 };
        assert_eq!(PageCursor::parse(&cur.encode()).unwrap(), cur);
        let none = PageCursor { key: None, skip: 5 };
        assert_eq!(PageCursor::parse(&none.encode()).unwrap(), none);
        // The longest cursor: an id of 40 characters, the longest title, an id of 40 characters, 9 digits.
        let id = "a".repeat(40);
        let long = PageCursor { key: Some(DashboardKey::new(&id, &"é".repeat(256), &id)), skip: 999_999_999 };
        assert!(long.encode().len() <= MAX_CURSOR_CHARS, "{}", long.encode().len());
        assert_eq!(PageCursor::parse(&long.encode()).unwrap(), long);
        for bad in [
            "",
            "abc",
            "djEy",
            &b64url_encode(b"x\0y"),
            &b64url_encode(b"Bad\0t\0u\x000"),
            &b64url_encode(b"a\0t\0u"),
            &b64url_encode(b"a\0t\0u\0"),
            &b64url_encode(b"a\0t\0u\x0001"),
            &b64url_encode(b"a\0t\0u\0-1"),
            &b64url_encode(b"a\0t\0u\x001234567890"),
            &b64url_encode(b"\0t\0\x000"),
            &"A".repeat(513),
        ] {
            assert!(PageCursor::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn limit_rules() {
        assert_eq!(parse_limit(None, 500).unwrap(), 100);
        assert_eq!(parse_limit(None, 100).unwrap(), 100);
        assert_eq!(parse_limit(Some("1"), 100).unwrap(), 1);
        assert_eq!(parse_limit(Some("500"), 500).unwrap(), 500);
        for bad in ["0", "501", "-1", "1.5", "", "abc", "99999999999"] {
            assert!(parse_limit(Some(bad), 500).is_err(), "{bad}");
        }
        assert!(parse_limit(Some("101"), 100).is_err());
    }
}
