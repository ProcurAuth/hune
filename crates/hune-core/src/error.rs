// SPDX-License-Identifier: MIT OR Apache-2.0
//! The form of a validation error (4.37). The format validation (4.36), the SQL checks at
//! save (7.18) and the API errors `invalid_document`, `stored_document_invalid` and
//! `bad_request` (6.8) use this one form.

use serde::Serialize;
use std::fmt;

/// The code of a validation error (4.37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The bytes are not UTF-8, or they start with a byte order mark.
    Encoding,
    /// A JSON syntax error.
    Syntax,
    /// A size or a count is above its limit.
    TooLarge,
    /// The depth is above `MAX_JSON_DEPTH`.
    TooDeep,
    /// A member name occurs two times in one object.
    DuplicateMember,
    /// The table does not list the member.
    UnknownMember,
    /// A member that MUST be in the object is absent.
    MissingMember,
    /// The JSON type is not the type of the table.
    WrongType,
    /// A number or a length is outside its range.
    OutOfRange,
    /// A text does not match its grammar.
    BadFormat,
    /// A value occurs two times where it MUST be unique.
    NotUnique,
    /// A `reuse` names a wrong panel, the `query` variables have a cycle, or a source is unknown.
    BadReference,
    /// Two panels of one section overlap.
    Overlap,
    /// The value is valid alone but not in this place.
    NotPermitted,
    /// The `uid` differs from the location.
    UidMismatch,
    /// The format version is not the version of this binary.
    FormatVersion,
    /// The list stopped at `MAX_ERRORS`.
    TooManyErrors,
}

impl ErrorCode {
    /// Gives the code as the text of 4.37.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::Encoding => "encoding",
            ErrorCode::Syntax => "syntax",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::TooDeep => "too_deep",
            ErrorCode::DuplicateMember => "duplicate_member",
            ErrorCode::UnknownMember => "unknown_member",
            ErrorCode::MissingMember => "missing_member",
            ErrorCode::WrongType => "wrong_type",
            ErrorCode::OutOfRange => "out_of_range",
            ErrorCode::BadFormat => "bad_format",
            ErrorCode::NotUnique => "not_unique",
            ErrorCode::BadReference => "bad_reference",
            ErrorCode::Overlap => "overlap",
            ErrorCode::NotPermitted => "not_permitted",
            ErrorCode::UidMismatch => "uid_mismatch",
            ErrorCode::FormatVersion => "format_version",
            ErrorCode::TooManyErrors => "too_many_errors",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One validation error (4.37).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    /// The JSON pointer (RFC 6901) of the member or the item. `""` for the whole document.
    pub pointer: String,
    /// The code of the error.
    pub code: ErrorCode,
    /// One English sentence: the member, the rule and the value.
    pub message: String,
    /// The line (from 1) for the codes `encoding` and `syntax`. Else `None`.
    pub line: Option<u32>,
    /// The column in characters (from 1) for the codes `encoding` and `syntax`. Else `None`.
    pub column: Option<u32>,
}

impl ValidationError {
    /// Makes an error without a position.
    pub fn new(pointer: impl Into<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        ValidationError { pointer: pointer.into(), code, message: message.into(), line: None, column: None }
    }

    /// Makes an error with a line and a column (codes `encoding` and `syntax`).
    pub fn at(code: ErrorCode, message: impl Into<String>, line: u32, column: u32) -> Self {
        ValidationError {
            pointer: String::new(),
            code,
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.pointer, self.code, self.message)?;
        if let (Some(l), Some(c)) = (self.line, self.column) {
            write!(f, " (line {l}, column {c})")?;
        }
        Ok(())
    }
}

/// Escapes one reference token of a JSON pointer (RFC 6901): `~` becomes `~0`, `/` becomes `~1`.
pub fn pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
