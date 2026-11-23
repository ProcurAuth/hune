//! The folder file (4.41).

use super::canonical;
use super::read::{Ctx, Obj, TextRule};
use crate::error::ValidationError;
use crate::limits::MAX_TITLE_CHARS;

/// A folder file (4.41): the title of one folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderFile {
    /// The title of the folder: 1 to 256 characters.
    pub title: String,
}

/// Validates the bytes of a folder file (4.41). The file is closed: it has `format_version`
/// and `title` only.
pub fn parse_folder_file(bytes: &[u8]) -> Result<FolderFile, Vec<ValidationError>> {
    let tree = super::parse_object(bytes, super::FOLDER_FORMAT_VERSION).map_err(|e| vec![e])?;
    let mut cx = Ctx::default();
    let title = Obj::root(&tree, "folder file").and_then(|mut obj| {
        obj.opt("format_version");
        let title = obj.text(&mut cx, "title", TextRule::chars(1, MAX_TITLE_CHARS));
        obj.finish(&mut cx, &["format_version", "title"]);
        title
    });
    match title {
        Some(title) if cx.errors.is_empty() => Ok(FolderFile { title }),
        _ => Err(cx.errors),
    }
}

/// Gives the canonical bytes of a folder file (4.41, 4.38).
pub fn folder_file_bytes(title: &str) -> Vec<u8> {
    canonical::write(&canonical::folder_out(title))
}
