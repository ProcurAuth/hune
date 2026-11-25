// SPDX-License-Identifier: MIT OR Apache-2.0
//! Checked identifiers of 4.3: dashboard uid, folder id and source id.
//!
//! The three ids use the rule `^[a-z0-9][a-z0-9-]{0,39}$`. Only lower-case letters are
//! permitted, so a case-insensitive file system cannot merge two ids.

use std::fmt;

/// The error of an id that does not match `^[a-z0-9][a-z0-9-]{0,39}$`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdError {
    /// The text that failed the check.
    pub value: String,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected an id that matches ^[a-z0-9][a-z0-9-]{{0,39}}$, found {:?}", self.value)
    }
}

impl std::error::Error for IdError {}

/// Tells if `s` matches `^[a-z0-9][a-z0-9-]{0,39}$`.
pub fn is_valid_id(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 40 {
        return false;
    }
    if !(b[0].is_ascii_lowercase() || b[0].is_ascii_digit()) {
        return false;
    }
    b[1..].iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

macro_rules! checked_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Checks the text against the id rule of 4.3.
            pub fn parse(s: &str) -> Result<Self, IdError> {
                if is_valid_id(s) {
                    Ok(Self(s.to_owned()))
                } else {
                    Err(IdError { value: s.to_owned() })
                }
            }

            /// Gives the id as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> {
                Self::parse(s)
            }
        }

    };
}

// The plain order of an id and the plain text form: byte order, and the text of the id. `FolderId` has its own forms below.
macro_rules! plain_id_forms {
    ($name:ident) => {
        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $name {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }
    };
}

checked_id!(
    /// The uid of a dashboard. It is also the file stem (volume) or the row key (PostgreSQL).
    Uid
);
checked_id!(
    /// The id of a folder. It is the directory name (volume) or the column `id` (PostgreSQL).
    FolderId
);
checked_id!(
    /// The id of a data source: the label of a `clickhouse "<id>"` block.
    SourceId
);

plain_id_forms!(Uid);
plain_id_forms!(SourceId);

/// The text that stands for "no folder" inside the code and in the access tree (the node `folder:_root`). The id rule of 4.3 refuses it,
/// so no client can name it as a folder id: only `FolderId::root` makes it. The API never shows this text: it shows `null` (6.11).
pub const ROOT_FOLDER_TEXT: &str = "_root";

impl FolderId {
    /// The place of a dashboard that has no folder.
    pub fn root() -> Self {
        Self(ROOT_FOLDER_TEXT.to_owned())
    }

    /// True for the place of a dashboard that has no folder.
    pub fn is_root(&self) -> bool {
        self.0 == ROOT_FOLDER_TEXT
    }

    /// The value of the API: the id of a real folder, or `None` (the JSON `null`) for the root.
    pub fn wire(&self) -> Option<&str> {
        if self.is_root() { None } else { Some(&self.0) }
    }

    /// Reads a value of the API: `None` (the JSON `null`) is the root, a text is checked with the id rule of 4.3.
    pub fn from_wire(value: Option<&str>) -> Result<Self, IdError> {
        match value {
            None => Ok(Self::root()),
            Some(text) => Self::parse(text),
        }
    }
}

// The order of folders: the root first, then the real folders in byte order of the id (6.24).
impl PartialOrd for FolderId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FolderId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.is_root().cmp(&self.is_root()).then_with(|| self.0.cmp(&other.0))
    }
}

// The JSON form of a folder id: the text, and `null` for the root.
impl serde::Serialize for FolderId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.wire() {
            Some(text) => s.serialize_str(text),
            None => s.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_rule() {
        assert!(is_valid_id("a"));
        assert!(is_valid_id("0"));
        assert!(is_valid_id("nodes"));
        assert!(is_valid_id("a-b-c"));
        assert!(is_valid_id(&"a".repeat(40)));
        assert!(!is_valid_id(&"a".repeat(41)));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-a"));
        assert!(!is_valid_id("A"));
        assert!(!is_valid_id("a_b"));
        assert!(!is_valid_id("a.b"));
        assert!(Uid::parse("Prod").is_err());
        assert_eq!(FolderId::parse("transit").unwrap().as_str(), "transit");
        // The root is not a folder id: the rule refuses it, and only `root()` makes it.
        assert!(FolderId::parse("_root").is_err());
        assert!(FolderId::root().is_root());
        assert_eq!(FolderId::root().wire(), None);
        assert_eq!(FolderId::from_wire(None).unwrap(), FolderId::root());
        assert_eq!(FolderId::from_wire(Some("ops")).unwrap().wire(), Some("ops"));
        assert!(FolderId::from_wire(Some("_root")).is_err());
        assert_eq!(serde_json::to_string(&FolderId::root()).unwrap(), "null");
        assert_eq!(serde_json::to_string(&FolderId::parse("ops").unwrap()).unwrap(), "\"ops\"");
        // The root comes first, also before an id that starts with a number.
        let mut ids = [
            FolderId::parse("b").unwrap(),
            FolderId::parse("0a").unwrap(),
            FolderId::root(),
            FolderId::parse("a").unwrap(),
        ];
        ids.sort();
        assert_eq!(ids.iter().map(|f| f.to_string()).collect::<Vec<_>>(), ["_root", "0a", "a", "b"]);
    }
}
