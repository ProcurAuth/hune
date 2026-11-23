// SPDX-License-Identifier: MIT OR Apache-2.0
//! The types of the store contract (5.2) and the errors (5.6).

use std::fmt;
use std::sync::Arc;

pub use hune_core::document::CanonicalDocument;
pub use hune_core::error::ValidationError;
pub use hune_core::etag::Etag;
pub use hune_core::ids::{FolderId, Uid};
pub use hune_core::time::UtcMillis;

/// The largest number of current dashboards in one store (5.7).
pub const MAX_DASHBOARDS: usize = 10_000;
/// The largest number of folders in one store (5.7).
pub const MAX_FOLDERS: usize = 1_000;
/// The largest number of characters in one history message (5.7).
pub const MAX_MESSAGE_CHARS: usize = 500;
/// The largest number of characters in `saved_by` (5.7).
pub const MAX_SUBJECT_CHARS: usize = 256;
/// The largest number of versions in one answer of `history` (5.7).
pub const MAX_HISTORY_PAGE: u32 = 100;

/// The error of a value that does not fit its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TypeError {}

fn has_control(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

/// A folder title: a text of 1 to 256 characters with no control character.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderTitle(String);

impl FolderTitle {
    /// Checks the title.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        let n = s.chars().count();
        if n == 0 || n > 256 || has_control(s) {
            return Err(TypeError(format!("expected a title of 1 to 256 characters, found {n} characters")));
        }
        Ok(FolderTitle(s.to_owned()))
    }
    /// The title as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The UUID of one save attempt (6.6). Text form: `8-4-4-4-12` lower-case hex.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SaveId([u8; 16]);

impl SaveId {
    /// Makes a save id from 16 raw bytes.
    pub fn from_bytes(b: [u8; 16]) -> Self {
        SaveId(b)
    }
    /// The 16 raw bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    /// Reads the canonical text form: 36 characters, lower-case hex, with four `-`.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        let b = s.as_bytes();
        let err = || TypeError("expected a UUID in the form 8-4-4-4-12 with lower-case hex".to_owned());
        if b.len() != 36 {
            return Err(err());
        }
        let mut out = [0u8; 16];
        let mut n = 0;
        let mut i = 0;
        while i < 36 {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                if b[i] != b'-' {
                    return Err(err());
                }
                i += 1;
                continue;
            }
            let hv = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            let hi = hv(b[i]).ok_or_else(err)?;
            let lo = hv(b[i + 1]).ok_or_else(err)?;
            out[n] = (hi << 4) | lo;
            n += 1;
            i += 2;
        }
        Ok(SaveId(out))
    }
}

impl fmt::Display for SaveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
        )
    }
}

impl fmt::Debug for SaveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SaveId({self})")
    }
}

/// The author of a write: a ProcurAuth subject, or one of the reserved texts `-`,
/// `external` and `import-grafana` (5.2). A text of 1 to 256 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(String);

impl Subject {
    /// The subject of the mode `open` and of the command line.
    pub const ANONYMOUS: &'static str = "-";
    /// The subject of an outside change (5.5, rule 7).
    pub const EXTERNAL: &'static str = "external";
    /// The subject of the Grafana importer (section 15).
    pub const IMPORT_GRAFANA: &'static str = "import-grafana";

    /// Checks the subject: 1 to 256 characters, no control character.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        let n = s.chars().count();
        if n == 0 || n > MAX_SUBJECT_CHARS || has_control(s) {
            return Err(TypeError(format!("expected a subject of 1 to 256 characters, found {n} characters")));
        }
        Ok(Subject(s.to_owned()))
    }
    /// The subject `-`.
    pub fn anonymous() -> Self {
        Subject(Self::ANONYMOUS.to_owned())
    }
    /// The subject `external`.
    pub fn external() -> Self {
        Subject(Self::EXTERNAL.to_owned())
    }
    /// The subject `import-grafana`.
    pub fn import_grafana() -> Self {
        Subject(Self::IMPORT_GRAFANA.to_owned())
    }
    /// The subject as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A history message: a text of 0 to 500 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Message(String);

impl Message {
    /// Checks the message: 0 to 500 characters.
    pub fn parse(s: &str) -> Result<Self, TypeError> {
        let n = s.chars().count();
        if n > MAX_MESSAGE_CHARS {
            return Err(TypeError(format!("expected a message of 0 to 500 characters, found {n} characters")));
        }
        Ok(Message(s.to_owned()))
    }
    /// The message as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The metadata of one write (5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteMeta {
    /// The author.
    pub saved_by: Subject,
    /// The time of the write, from the clock of the instance, cut to the millisecond.
    pub saved_at: UtcMillis,
    /// The history message.
    pub message: Message,
    /// The id of the save attempt, from `Idempotency-Key`.
    pub save_id: Option<SaveId>,
}

/// The kind of a history version (5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VersionKind {
    /// `Store::create` from the API.
    Create,
    /// `Store::save` from the API.
    Save,
    /// `Store::restore`.
    Restore,
    /// `Store::move_to`.
    Move,
    /// `Store::delete`: a tombstone with no document and no etag.
    Delete,
    /// `hune import-grafana --write`.
    Import,
    /// An outside change that the store found (5.5, rule 7).
    External,
    /// `hune migrate-documents --write`.
    Migrate,
}

impl VersionKind {
    /// The text of the kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            VersionKind::Create => "create",
            VersionKind::Save => "save",
            VersionKind::Restore => "restore",
            VersionKind::Move => "move",
            VersionKind::Delete => "delete",
            VersionKind::Import => "import",
            VersionKind::External => "external",
            VersionKind::Migrate => "migrate",
        }
    }
    /// Reads the text of a kind.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "create" => VersionKind::Create,
            "save" => VersionKind::Save,
            "restore" => VersionKind::Restore,
            "move" => VersionKind::Move,
            "delete" => VersionKind::Delete,
            "import" => VersionKind::Import,
            "external" => VersionKind::External,
            "migrate" => VersionKind::Migrate,
            _ => return None,
        })
    }
}

/// The metadata of one history version (5.2, 5.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionMeta {
    /// The uid of the dashboard.
    pub uid: Uid,
    /// The version number, from 1.
    pub version: u64,
    /// The kind of the version.
    pub kind: VersionKind,
    /// The folder of the dashboard at this version.
    pub folder: FolderId,
    /// The etag of the document. Absent for `delete`.
    pub etag: Option<Etag>,
    /// The time of the write.
    pub saved_at: UtcMillis,
    /// The author.
    pub saved_by: Subject,
    /// The history message.
    pub message: Message,
    /// The id of the save attempt.
    pub save_id: Option<SaveId>,
    /// The size of the bytes. Absent for `delete` and for an `external` version without bytes.
    pub size: Option<u64>,
}

/// One history version with its bytes (5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The metadata.
    pub meta: VersionMeta,
    /// The bytes of the document. Absent for `delete` and for an `external` version without bytes.
    pub bytes: Option<Vec<u8>>,
}

/// A version that passed the validation of 4.36 for a restore (5.4, item 4): its number and its valid document.
///
/// Only [`ValidVersion::check`] makes it, from a version that `Store::version` gave. Thus the document is the
/// document of that version. `Store::restore_valid` writes it and never parses or validates it again.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidVersion {
    number: u64,
    doc: CanonicalDocument,
}

impl ValidVersion {
    /// Validates the bytes of a version of `uid` with the expected uid `uid` (4.36). A version without bytes, or
    /// with bytes that are not valid in the current format, gives `NotRestorable`. The work is linear in the bytes
    /// and uses the CPU: run it in `spawn_blocking`, never on an async worker and never under a lock of the store.
    pub fn check(uid: &Uid, version: &Version) -> Result<ValidVersion, StoreError> {
        let Some(bytes) = &version.bytes else { return Err(StoreError::NotRestorable) };
        let doc = hune_core::document::validate(bytes, Some(uid)).map_err(|_| StoreError::NotRestorable)?;
        Ok(ValidVersion { number: version.meta.version, doc })
    }

    /// The number of the version.
    pub fn number(&self) -> u64 {
        self.number
    }

    /// The valid document of the version.
    pub fn document(&self) -> &CanonicalDocument {
        &self.doc
    }
}

/// The current document of a uid, read from the backend (5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    /// The uid.
    pub uid: Uid,
    /// The folder.
    pub folder: FolderId,
    /// The version number of the head version, or 0 without a head version.
    pub version: u64,
    /// The etag of the canonical bytes.
    pub etag: Etag,
    /// The canonical bytes.
    pub bytes: Arc<Vec<u8>>,
    /// The time of the current version.
    pub saved_at: UtcMillis,
    /// The author of the current version, or `external` after an outside change.
    pub saved_by: Subject,
}

/// The outcome of a write (5.2, 5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The store wrote a new version.
    Written,
    /// The new etag equals the current etag: no version (5.5, rule 5).
    Unchanged,
    /// The `save_id` names an applied version of the same request (5.5, rule 3).
    Replayed,
}

/// The result of a write (5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// The uid.
    pub uid: Uid,
    /// The folder after the write.
    pub folder: FolderId,
    /// The version number after the write.
    pub version: u64,
    /// The etag after the write. Absent after a delete.
    pub etag: Option<Etag>,
    /// The outcome.
    pub outcome: Outcome,
}

/// The state of a dashboard that a `Conflict` carries (5.6, 6.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictInfo {
    /// The current etag.
    pub etag: Etag,
    /// The current version number.
    pub version: u64,
    /// The current folder.
    pub folder: FolderId,
    /// The time of the current version.
    pub saved_at: UtcMillis,
    /// The author of the current version.
    pub saved_by: Subject,
}

/// One folder of a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRecord {
    /// The folder id.
    pub id: FolderId,
    /// The folder title.
    pub title: FolderTitle,
}

/// One entry of the catalog (5.2, 5.8).
#[derive(Debug, Clone)]
pub enum CatalogEntry {
    /// A valid current document.
    Valid {
        /// The current document as the backend gives it.
        current: Current,
        /// The typed document.
        doc: Arc<CanonicalDocument>,
    },
    /// A stored document that is not valid. The server does not serve it.
    Invalid {
        /// The uid (the file stem or the row key).
        uid: Uid,
        /// The folder.
        folder: FolderId,
        /// The SHA-256 of the raw bytes (4.39, item 3).
        etag: Etag,
        /// The errors of 4.37.
        errors: Vec<ValidationError>,
    },
}

impl CatalogEntry {
    /// The uid of the entry.
    pub fn uid(&self) -> &Uid {
        match self {
            CatalogEntry::Valid { current, .. } => &current.uid,
            CatalogEntry::Invalid { uid, .. } => uid,
        }
    }
    /// The folder of the entry.
    pub fn folder(&self) -> &FolderId {
        match self {
            CatalogEntry::Valid { current, .. } => &current.folder,
            CatalogEntry::Invalid { folder, .. } => folder,
        }
    }
    /// The etag of the entry.
    pub fn etag(&self) -> &Etag {
        match self {
            CatalogEntry::Valid { current, .. } => &current.etag,
            CatalogEntry::Invalid { etag, .. } => etag,
        }
    }
}

/// The errors of a store (5.6).
#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    /// No current dashboard with this uid. 404 `not_found`.
    NotFound,
    /// No valid folder with this id. 404 `not_found`.
    FolderNotFound,
    /// No version at this number. 404 `not_found`.
    VersionNotFound,
    /// The uid or the folder id exists. For a uid, the folder of the current dashboard or of
    /// the head version of a deleted dashboard. 409 `already_exists`.
    Exists {
        /// The folder of the uid, when the id is a uid.
        folder: Option<FolderId>,
    },
    /// `if_match` or `expected_folder` differs. 409 `conflict`.
    Conflict(Box<ConflictInfo>),
    /// The folder holds a dashboard or another entry. 409 `folder_not_empty`.
    FolderNotEmpty,
    /// The volume has `read_only = true`. 403 `editing_disabled`.
    ReadOnly,
    /// A stored document is not valid in the current format. 422 `stored_document_invalid`.
    Invalid(Vec<ValidationError>),
    /// The version cannot be restored. 422 `not_restorable`.
    NotRestorable,
    /// The `save_id` belongs to a version of another request. 422 `idempotency_key_reused`.
    SaveIdReused,
    /// No space, or a limit of 5.7. 507 `storage_full`.
    Full,
    /// The backend does not answer, or an I/O error. 503 `storage_unavailable`. The text is
    /// for the log only and never goes to a client.
    Unavailable(String),
    /// The target of an import is not empty. Command line only.
    NotEmpty,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound => f.write_str("not found"),
            StoreError::FolderNotFound => f.write_str("folder not found"),
            StoreError::VersionNotFound => f.write_str("version not found"),
            StoreError::Exists { .. } => f.write_str("already exists"),
            StoreError::Conflict(c) => write!(f, "conflict: current etag {}", c.etag),
            StoreError::FolderNotEmpty => f.write_str("folder not empty"),
            StoreError::ReadOnly => f.write_str("read-only store"),
            StoreError::Invalid(e) => write!(f, "stored document not valid ({} errors)", e.len()),
            StoreError::NotRestorable => f.write_str("version not restorable"),
            StoreError::SaveIdReused => f.write_str("save id reused by another request"),
            StoreError::Full => f.write_str("storage full"),
            StoreError::Unavailable(s) => write!(f, "storage unavailable: {s}"),
            StoreError::NotEmpty => f.write_str("target store not empty"),
        }
    }
}

impl std::error::Error for StoreError {}
