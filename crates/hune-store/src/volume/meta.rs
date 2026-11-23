//! The history metadata file (5.11) and the names of the version files (5.9).

use hune_core::time::UtcMillis;
use serde::{Deserialize, Deserializer, Serialize};

use crate::types::{Etag, FolderId, Message, SaveId, Subject, Uid, VersionKind, VersionMeta};

/// The `format_version` of the metadata file.
const META_FORMAT: u64 = 1;

/// The file name of the metadata of version `n`.
pub(crate) fn meta_name(n: u64) -> String {
    format!("{n}.meta.json")
}

/// The file name of the bytes of version `n`.
pub(crate) fn json_name(n: u64) -> String {
    format!("{n}.json")
}

/// The kind of file that a name in `.history/<uid>` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VersionFile {
    Json,
    Meta,
}

/// Reads `<n>.json` or `<n>.meta.json`: `n` is a decimal integer from 1 without a zero
/// before the first other digit and at most 9223372036854775807 (5.9, item 2; 5.11).
pub(crate) fn parse_version_name(name: &str) -> Option<(u64, VersionFile)> {
    let (digits, kind) = match name.strip_suffix(".meta.json") {
        Some(d) => (d, VersionFile::Meta),
        None => (name.strip_suffix(".json")?, VersionFile::Json),
    };
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    (1..=i64::MAX as u64).contains(&n).then_some((n, kind))
}

fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaIn {
    format_version: u64,
    uid: String,
    version: u64,
    kind: String,
    folder: String,
    #[serde(deserialize_with = "present")]
    etag: Option<String>,
    saved_at: String,
    saved_by: String,
    message: String,
    #[serde(deserialize_with = "present")]
    save_id: Option<String>,
}

#[derive(Serialize)]
struct MetaOut<'a> {
    format_version: u64,
    uid: &'a str,
    version: u64,
    kind: &'a str,
    folder: &'a str,
    etag: Option<String>,
    saved_at: String,
    saved_by: &'a str,
    message: &'a str,
    save_id: Option<String>,
}

/// Writes the canonical bytes of a metadata file (5.11, item 1; 4.38).
pub(crate) fn encode_meta(m: &VersionMeta) -> Vec<u8> {
    let out = MetaOut {
        format_version: META_FORMAT,
        uid: m.uid.as_str(),
        version: m.version,
        kind: m.kind.as_str(),
        folder: m.folder.as_str(),
        etag: m.etag.map(|e| e.to_hex()),
        saved_at: m.saved_at.to_rfc3339(),
        saved_by: m.saved_by.as_str(),
        message: m.message.as_str(),
        save_id: m.save_id.map(|s| s.to_string()),
    };
    // A struct of text, integers and null never fails to serialise.
    let mut bytes = serde_json::to_vec_pretty(&out).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Reads and checks a metadata file. `dir_uid` and `n` come from the place of the file.
/// The error is a text for the log.
pub(crate) fn decode_meta(bytes: &[u8], dir_uid: &Uid, n: u64) -> Result<VersionMeta, String> {
    let m: MetaIn = serde_json::from_slice(bytes).map_err(|e| format!("metadata file not valid: {e}"))?;
    if m.format_version != META_FORMAT {
        return Err(format!("metadata format_version {} is not loadable", m.format_version));
    }
    if m.uid != dir_uid.as_str() {
        return Err(format!("member uid {:?} differs from the directory {dir_uid}", m.uid));
    }
    if m.version != n {
        return Err(format!("member version {} differs from the file name {n}", m.version));
    }
    let kind = VersionKind::parse(&m.kind).ok_or_else(|| format!("unknown kind {:?}", m.kind))?;
    let folder = FolderId::parse(&m.folder).map_err(|e| e.to_string())?;
    let etag = match (&m.etag, kind) {
        (None, VersionKind::Delete) => None,
        (Some(_), VersionKind::Delete) => return Err("a delete version has no etag".to_owned()),
        (None, _) => return Err("etag is null and the kind is not delete".to_owned()),
        (Some(e), _) => Some(Etag::parse_hex(e).map_err(|e| e.to_string())?),
    };
    let saved_at = UtcMillis::parse_rfc3339(&m.saved_at)
        .ok_or_else(|| format!("saved_at {:?} is not a UtcMillis text", m.saved_at))?;
    let saved_by = Subject::parse(&m.saved_by).map_err(|e| e.to_string())?;
    if kind == VersionKind::External && saved_by.as_str() != Subject::EXTERNAL {
        return Err("an external version has saved_by external".to_owned());
    }
    let message = Message::parse(&m.message).map_err(|e| e.to_string())?;
    let save_id = m.save_id.as_deref().map(SaveId::parse).transpose().map_err(|e| e.to_string())?;
    Ok(VersionMeta {
        uid: dir_uid.clone(),
        version: n,
        kind,
        folder,
        etag,
        saved_at,
        saved_by,
        message,
        save_id,
        size: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> VersionMeta {
        VersionMeta {
            uid: Uid::parse("nodes").unwrap(),
            version: 7,
            kind: VersionKind::Save,
            folder: FolderId::parse("transit").unwrap(),
            etag: Some(Etag::of(b"x")),
            saved_at: UtcMillis::from_millis(1_794_211_200_123),
            saved_by: Subject::anonymous(),
            message: Message::parse("Add \"the\" column\n").unwrap(),
            save_id: Some(SaveId::parse("0f8e6b2c-3c1d-4f7a-9b55-2d4c8e1a7f30").unwrap()),
            size: None,
        }
    }

    #[test]
    fn round_trip_and_form() {
        let m = sample();
        let bytes = encode_meta(&m);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("{\n  \"format_version\": 1,\n  \"uid\": \"nodes\",\n"), "{text}");
        assert!(text.ends_with("}\n"));
        assert!(text.contains("\\\"the\\\" column\\n"));
        assert_eq!(decode_meta(&bytes, &m.uid, 7).unwrap(), m);
        assert!(decode_meta(&bytes, &m.uid, 8).is_err());
        assert!(decode_meta(&bytes, &Uid::parse("other").unwrap(), 7).is_err());
    }

    #[test]
    fn closed_schema() {
        let m = sample();
        let text = String::from_utf8(encode_meta(&m)).unwrap();
        let extra = text.replacen("{\n", "{\n  \"extra\": 1,\n", 1);
        assert!(decode_meta(extra.as_bytes(), &m.uid, 7).is_err());
        let missing = text.replace("  \"save_id\": \"0f8e6b2c-3c1d-4f7a-9b55-2d4c8e1a7f30\"\n", "");
        let missing = missing.replace("\"message\": \"Add \\\"the\\\" column\\n\",", "\"message\": \"x\"");
        assert!(decode_meta(missing.as_bytes(), &m.uid, 7).is_err());
        let dup = text.replacen("{\n", "{\n  \"uid\": \"nodes\",\n", 1);
        assert!(decode_meta(dup.as_bytes(), &m.uid, 7).is_err());
    }

    #[test]
    fn names() {
        assert_eq!(parse_version_name("7.json"), Some((7, VersionFile::Json)));
        assert_eq!(parse_version_name("12.meta.json"), Some((12, VersionFile::Meta)));
        assert_eq!(parse_version_name("0.json"), None);
        assert_eq!(parse_version_name("07.json"), None);
        assert_eq!(parse_version_name("a.json"), None);
        assert_eq!(parse_version_name("9223372036854775808.json"), None);
        assert_eq!(parse_version_name("9223372036854775807.json").map(|x| x.0), Some(i64::MAX as u64));
    }
}
