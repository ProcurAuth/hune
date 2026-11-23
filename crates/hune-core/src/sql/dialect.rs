// SPDX-License-Identifier: MIT OR Apache-2.0
//! The SQL dialects of the query engine (7.19).
//!
//! A source has a kind, and the kind gives the dialect of its SQL. The engine is one program with
//! one dialect for each kind. The sections 7.5 to 7.18 describe the dialect `clickhouse`. The
//! sections 7.20 to 7.26 describe the dialect `postgres`.
//!
//! This module holds only the choice of the dialect. The lexer, the escape rules, the macros, the
//! statement check and the save checks of a dialect live in their own modules. Each of them takes
//! a [`Dialect`] when it has more than one behaviour.
//!
//! The table of 7.19 lists the items that a dialect changes. The new [`TokenKind`](super::TokenKind)
//! values `EString`, `PrefixedString`, `Operator` and `Parameter` belong to the lexer of `postgres`
//! (7.20). The lexer of `clickhouse` never makes them.

/// The dialect of the SQL text of a source (7.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// The dialect of a `clickhouse` source (7.5 to 7.18).
    ClickHouse,
    /// The dialect of a `postgres` source (7.20 to 7.26).
    Postgres,
}

impl Dialect {
    /// Gives the name of the dialect. It is the kind of the source as the configuration writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Dialect::ClickHouse => "clickhouse",
            Dialect::Postgres => "postgres",
        }
    }

    /// Reads the name of a dialect. An unknown name gives `None`.
    pub fn parse(name: &str) -> Option<Dialect> {
        match name {
            "clickhouse" => Some(Dialect::ClickHouse),
            "postgres" => Some(Dialect::Postgres),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Dialect;

    #[test]
    fn names_round_trip() {
        for d in [Dialect::ClickHouse, Dialect::Postgres] {
            assert_eq!(Dialect::parse(d.as_str()), Some(d));
        }
        assert_eq!(Dialect::parse("mysql"), None);
        assert_eq!(Dialect::parse(""), None);
    }
}
