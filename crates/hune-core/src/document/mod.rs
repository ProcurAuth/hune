//! Document format 3 (section 4): types, closed parse, validation, canonical bytes and etag.
//!
//! [`validate`], [`migrate_2_to_3`] and [`migrate_1_to_3`] are the only functions that make a
//! [`CanonicalDocument`]. [`migrate_1_to_2`] keeps the first change of the format (4.42).
//! The rest of the code reads the typed [`Document`].

mod canonical;
mod cross;
mod folder;
mod geomap;
mod grammar;
mod json;
mod logs;
mod migrate;
mod model;
mod names;
mod pattern;
mod read;
mod text_warnings;
mod time_text;
mod walk;
mod walk_transforms;

use crate::error::{ErrorCode, ValidationError};
use crate::etag::Etag;
use crate::ids::Uid;
use crate::limits::MAX_DOCUMENT_BYTES;

pub use cross::{dependency_names, referenced_names};
pub use folder::{FolderFile, folder_file_bytes, parse_folder_file};
pub use geomap::{
    GEOMAP_MAX_MARKER_SIZE, GEOMAP_MAX_ZOOM, GeomapLayer, GeomapOptions, GeomapTooltip, GeomapView, GeomapViewMode,
};
pub use grammar::{
    is_decimal_number, is_valid_colour, is_valid_query_ref, is_valid_unit, is_valid_url_prefix, is_valid_variable_name,
    is_valid_zone_name,
};
pub use migrate::{migrate_1_to_2, migrate_1_to_3, migrate_2_to_3};
pub use model::*;
pub use names::DocumentNames;
pub use pattern::{PatternFault, PatternForm, check_pattern};
pub use text_warnings::{TextWarning, text_warnings, text_warnings_into};
pub use time_text::{
    MAX_ABSOLUTE_MS, Offset, TimeText, TimeUnit, parse_duration_ms, parse_time_from, parse_time_shift, parse_time_text,
};

/// The format version of the document of this binary (4.42).
pub const FORMAT_VERSION: u32 = 3;

/// The format version of the folder file (4.41). It changes only when the schema of the folder
/// file changes (4.42, item 11), so it stays 1 when the document format changes.
pub const FOLDER_FORMAT_VERSION: u32 = 1;

/// A valid document: the canonical bytes, the etag, the uid, the title and the typed
/// document (5.2). Only [`validate`] makes it.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalDocument {
    bytes: Vec<u8>,
    etag: Etag,
    uid: Uid,
    title: String,
    document: Document,
    names: NamesCache,
}

/// The text of the member `names` (6.14), made one time for a document. It is a pure function of the
/// document, so a document in the catalog has no stale copy. Two documents are equal with or without it.
#[derive(Debug, Clone, Default)]
struct NamesCache(std::sync::OnceLock<String>);

impl PartialEq for NamesCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl CanonicalDocument {
    /// The JSON text of the member `names` of the answer of 6.14 (7.19). The first call reads the queries of
    /// the document. The next calls give the same text with no scan.
    pub fn names_json(&self) -> &str {
        self.names.0.get_or_init(|| DocumentNames::of(&self.document).to_json())
    }

    /// The canonical bytes (4.38).
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The etag of the canonical bytes (4.39).
    pub fn etag(&self) -> &Etag {
        &self.etag
    }

    /// The member `uid`.
    pub fn uid(&self) -> &Uid {
        &self.uid
    }

    /// The member `title`.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The typed document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The format version of the document. Always [`FORMAT_VERSION`] in this binary.
    pub fn format_version(&self) -> u32 {
        FORMAT_VERSION
    }
}

/// Checks the bytes, parses the JSON and reads `format_version` (4.36, steps 1 to 4).
/// `format` is the one format version that the caller accepts. Gives the JSON tree, or the one
/// error that stops the validation.
pub(crate) fn parse_object(bytes: &[u8], format: u32) -> Result<json::Json, ValidationError> {
    let text = json::check_bytes(bytes)?;
    let tree = json::parse(text)?;
    if !matches!(tree, json::Json::Obj(_)) {
        return Err(ValidationError::new(
            "",
            ErrorCode::WrongType,
            format!("document must be an object; value {}", read::show(&tree)),
        ));
    }
    check_format_version(&tree, format)?;
    Ok(tree)
}

fn check_format_version(tree: &json::Json, format: u32) -> Result<(), ValidationError> {
    let json::Json::Obj(members) = tree else { return Ok(()) };
    let fail = |what: String| {
        ValidationError::new("/format_version", ErrorCode::FormatVersion, format!("not loadable: {what}"))
    };
    match members.iter().find(|(k, _)| k == "format_version") {
        None => Err(fail("format_version is absent".to_owned())),
        Some((_, json::Json::Num(n))) if n.is_integer() => match n.as_i64() {
            Some(v) if v == i64::from(format) => Ok(()),
            _ => Err(fail(format!("format {}", read::clip(n.text())))),
        },
        Some((_, other)) => Err(fail(format!("format {}", read::show(other)))),
    }
}

/// Validates the bytes of one document (4.36). `expected_uid` is the location of the
/// document (4.5): a difference gives `uid_mismatch` at `/uid`. `None` skips that check.
///
/// The result is the canonical document, or the list of errors (at most `MAX_ERRORS`).
pub fn validate(bytes: &[u8], expected_uid: Option<&Uid>) -> Result<CanonicalDocument, Vec<ValidationError>> {
    let tree = parse_object(bytes, FORMAT_VERSION).map_err(|e| vec![e])?;
    validate_tree(tree, expected_uid)
}

/// Validates a JSON tree of format [`FORMAT_VERSION`] (4.36, steps 5 to 7). `validate` and the
/// migration use it, so a migrated document gets the same checks as any other document.
///
/// The tree is dropped after the walk, before the canonical text is made, so the two never use memory at
/// the same time.
pub(crate) fn validate_tree(
    tree: json::Json,
    expected_uid: Option<&Uid>,
) -> Result<CanonicalDocument, Vec<ValidationError>> {
    let mut cx = read::Ctx::default();
    let document = read::Obj::root(&tree, "document").and_then(|root| walk::document(&mut cx, root, expected_uid));
    if !cx.stopped() {
        cross::check(&mut cx);
    }
    drop(tree);
    let Some(document) = document.filter(|_| cx.errors.is_empty()) else {
        if cx.errors.is_empty() {
            // The walk gave no document and no error: this does not occur. Report it as an error.
            cx.errors.push(ValidationError::new("", ErrorCode::BadFormat, "the document is not valid"));
        }
        return Err(cx.errors);
    };
    drop(cx);
    let out = canonical::document_out(&document);
    // The length comes before the text: a canonical form above the limit gives `too_large`, and the writer
    // allocates no buffer for it. A form within the limit gets one buffer of its exact length.
    let len = canonical::length(&out);
    if len > MAX_DOCUMENT_BYTES {
        return Err(vec![ValidationError::new(
            "",
            ErrorCode::TooLarge,
            format!("canonical document must have at most {MAX_DOCUMENT_BYTES} bytes; value {len} bytes"),
        )]);
    }
    let canonical_bytes = canonical::write_sized(&out, len);
    drop(out);
    let etag = Etag::of(&canonical_bytes);
    Ok(CanonicalDocument {
        bytes: canonical_bytes,
        etag,
        uid: document.uid.clone(),
        title: document.title.clone(),
        document,
        names: NamesCache::default(),
    })
}
