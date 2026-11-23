//! Decoding of rows into the types of the store, and the check of a stored document (5.24, item 8).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hune_core::document::validate;
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::MAX_DOCUMENT_BYTES;
use tokio_postgres::Row;

use crate::store::{EntryMark, FolderMark, FolderState};
use crate::types::{
    CanonicalDocument, CatalogEntry, Current, Etag, FolderId, FolderRecord, FolderTitle, Message, SaveId, StoreError,
    Subject, Uid, UtcMillis, VersionKind, VersionMeta,
};

/// The answer for a row that the backend cannot decode. A row that breaks a `CHECK` of 5.24
/// cannot exist, so this means a change outside Hune.
pub(super) fn corrupt(what: impl std::fmt::Display) -> StoreError {
    StoreError::Unavailable(format!("a row cannot be decoded: {what}"))
}

/// The `SystemTime` of a `UtcMillis`.
pub(super) fn to_system(t: UtcMillis) -> SystemTime {
    let ms = t.as_millis();
    if ms >= 0 {
        UNIX_EPOCH + Duration::from_millis(ms.unsigned_abs())
    } else {
        UNIX_EPOCH - Duration::from_millis(ms.unsigned_abs())
    }
}

/// The `UtcMillis` of a `SystemTime`, cut to the millisecond.
pub(super) fn from_system(t: SystemTime) -> UtcMillis {
    let ms = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    };
    UtcMillis::from_millis(ms)
}

/// The instance clock, cut to the millisecond.
pub(super) fn now() -> SystemTime {
    to_system(from_system(SystemTime::now()))
}

pub(super) fn parse_etag(s: &str) -> Result<Etag, StoreError> {
    Etag::parse_hex(s).map_err(|_| corrupt(format!("the etag {s:?} is not 64 hex characters")))
}

pub(super) fn parse_uid(s: &str) -> Result<Uid, StoreError> {
    Uid::parse(s).map_err(|e| corrupt(format!("the uid {s:?}: {e}")))
}

pub(super) fn parse_folder(s: &str) -> Result<FolderId, StoreError> {
    FolderId::parse(s).map_err(|e| corrupt(format!("the folder id {s:?}: {e}")))
}

fn parse_subject(s: &str) -> Result<Subject, StoreError> {
    Subject::parse(s).map_err(corrupt)
}

pub(super) fn parse_save_id(s: Option<String>) -> Result<Option<SaveId>, StoreError> {
    match s {
        None => Ok(None),
        Some(t) => SaveId::parse(&t).map(Some).map_err(corrupt),
    }
}

fn one(code: ErrorCode, message: &str) -> Vec<ValidationError> {
    vec![ValidationError::new("", code, message)]
}

/// Checks a stored document with its column `etag` (5.24, item 8). The document is valid when
/// the core accepts it, its bytes are the canonical bytes, and the column etag is the etag of
/// these bytes. The error of a column that is wrong has the code `not_permitted` and names the column.
pub(super) fn classify(
    uid: &Uid,
    document: &str,
    column_etag: &str,
) -> Result<CanonicalDocument, Vec<ValidationError>> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(one(ErrorCode::TooLarge, &format!("the column document has more than {MAX_DOCUMENT_BYTES} bytes")));
    }
    let doc = validate(document.as_bytes(), Some(uid))?;
    if doc.bytes() != document.as_bytes() {
        return Err(one(ErrorCode::NotPermitted, "the column document is not in canonical form"));
    }
    if doc.etag().to_hex() != column_etag {
        return Err(one(ErrorCode::NotPermitted, "the column etag differs from the SHA-256 of the column document"));
    }
    Ok(doc)
}

/// The head version of a uid, as P11 and the join of P5, P7 and P8 give it.
#[derive(Debug, Clone)]
pub(super) struct HeadRow {
    pub version: i64,
    pub etag: Option<String>,
}

/// The fields `version`, `saved_at` and `saved_by` of a `Current` (5.2). The head version gives
/// the number. An outside change (the head etag differs, or no head exists) gives `external`.
pub(super) fn current_fields(
    column_etag: &str,
    updated_at: SystemTime,
    updated_by: &str,
    head: Option<&HeadRow>,
) -> Result<(u64, UtcMillis, Subject), StoreError> {
    let at = from_system(updated_at);
    let n = |v: i64| u64::try_from(v).unwrap_or(0);
    match head {
        Some(h) if h.etag.as_deref() == Some(column_etag) => Ok((n(h.version), at, parse_subject(updated_by)?)),
        Some(h) => Ok((n(h.version), at, Subject::external())),
        None => Ok((0, at, Subject::external())),
    }
}

/// One row of P5, P7 or P8.
pub(super) struct DashRow {
    pub uid: String,
    pub folder: String,
    pub etag: String,
    pub document: String,
    pub updated_at: SystemTime,
    pub updated_by: String,
    pub head: Option<HeadRow>,
}

impl DashRow {
    pub(super) fn of(row: &Row) -> Result<DashRow, tokio_postgres::Error> {
        let h_version: Option<i64> = row.try_get("h_version")?;
        let h_etag: Option<String> = row.try_get("h_etag")?;
        Ok(DashRow {
            uid: row.try_get("uid")?,
            folder: row.try_get("folder_id")?,
            etag: row.try_get("etag")?,
            document: row.try_get("document")?,
            updated_at: row.try_get("updated_at")?,
            updated_by: row.try_get("updated_by")?,
            head: h_version.map(|version| HeadRow { version, etag: h_etag }),
        })
    }

    /// The catalog entry of the row and its mark. The mark holds the column etag, because the
    /// poll compares it with P6.
    pub(super) fn entry(&self) -> Result<(Arc<CatalogEntry>, EntryMark), StoreError> {
        let uid = parse_uid(&self.uid)?;
        let folder = parse_folder(&self.folder)?;
        let mark_etag = parse_etag(&self.etag)?;
        let mark = EntryMark { etag: mark_etag, sig: None, ctx: 0 };
        let entry = match classify(&uid, &self.document, &self.etag) {
            Ok(doc) => {
                let (version, saved_at, saved_by) =
                    current_fields(&self.etag, self.updated_at, &self.updated_by, self.head.as_ref())?;
                let current = Current {
                    uid,
                    folder,
                    version,
                    etag: *doc.etag(),
                    bytes: Arc::new(doc.bytes().to_vec()),
                    saved_at,
                    saved_by,
                };
                // The names of 6.14 are made here, with the check, so that the first read never makes them on an
                // async worker (3.7).
                let _ = doc.names_json();
                CatalogEntry::Valid { current, doc: Arc::new(doc) }
            }
            Err(errors) => CatalogEntry::Invalid { uid, folder, etag: Etag::of(self.document.as_bytes()), errors },
        };
        Ok((Arc::new(entry), mark))
    }
}

/// The stored bytes of a row and their etag (the SHA-256 of the bytes, 4.39). The row must
/// be in the folder: else the answer is `NotFound`.
pub(super) fn raw_of_row(row: &DashRow, folder: &FolderId) -> Result<(Vec<u8>, Etag), StoreError> {
    if parse_folder(&row.folder)? != *folder {
        return Err(StoreError::NotFound);
    }
    let bytes = row.document.clone().into_bytes();
    let etag = Etag::of(&bytes);
    Ok((bytes, etag))
}

/// The state of a folder from a row of P4.
pub(super) fn folder_state(row: &Row) -> Result<(FolderState, FolderMark), StoreError> {
    let id: String = row.try_get("id").map_err(corrupt)?;
    let title: String = row.try_get("title").map_err(corrupt)?;
    let id = parse_folder(&id)?;
    let state = match FolderTitle::parse(&title) {
        Ok(t) => FolderState::Valid(FolderRecord { id, title: t }),
        Err(e) => FolderState::Invalid {
            id,
            errors: vec![ValidationError::new("/title", ErrorCode::BadFormat, e.to_string())],
        },
    };
    Ok((state.clone(), FolderMark { sig: None, state }))
}

/// The metadata of a version from the columns of P20, P21 or P26. The caller gives the uid,
/// the number and the size.
#[allow(clippy::too_many_arguments)]
pub(super) fn version_meta(
    uid: &Uid,
    version: i64,
    kind: &str,
    folder: &str,
    etag: Option<&str>,
    saved_at: SystemTime,
    saved_by: &str,
    message: &str,
    save_id: Option<String>,
    size: Option<u64>,
) -> Result<VersionMeta, StoreError> {
    Ok(VersionMeta {
        uid: uid.clone(),
        version: u64::try_from(version).map_err(|_| corrupt("a negative version number"))?,
        kind: VersionKind::parse(kind).ok_or_else(|| corrupt(format!("the kind {kind:?}")))?,
        folder: parse_folder(folder)?,
        etag: etag.map(parse_etag).transpose()?,
        saved_at: from_system(saved_at),
        saved_by: parse_subject(saved_by)?,
        message: Message::parse(message).map_err(corrupt)?,
        save_id: parse_save_id(save_id)?,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(folder: &str, document: &str) -> DashRow {
        DashRow {
            uid: "d1".to_owned(),
            folder: folder.to_owned(),
            etag: "0".repeat(64),
            document: document.to_owned(),
            updated_at: UNIX_EPOCH,
            updated_by: "user:a".to_owned(),
            head: None,
        }
    }

    #[test]
    fn raw_of_row_gives_the_stored_bytes_and_their_digest() {
        let folder = parse_folder("f").unwrap();
        let (bytes, etag) = raw_of_row(&row("f", "{ not a document"), &folder).unwrap();
        assert_eq!(bytes, b"{ not a document");
        assert_eq!(etag, Etag::of(b"{ not a document"));
    }

    #[test]
    fn raw_of_row_in_another_folder_is_not_found() {
        let other = parse_folder("g").unwrap();
        assert!(matches!(raw_of_row(&row("f", "x"), &other), Err(StoreError::NotFound)));
    }
}
