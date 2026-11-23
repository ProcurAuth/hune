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
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    }
}
