// SPDX-License-Identifier: MIT OR Apache-2.0
//! The statement check (7.12 for `clickhouse`, 7.25 for `postgres`).
//!
//! [`check_statement`] is the check of 7.12. It does not change. [`check_statement_postgres`] is
//! the check of 7.25. [`check_statement_for`] picks the check from a [`Dialect`]. The check of
//! 7.25 is a defence in depth: the read-only transaction and the role of 8a.13 are the guard.

use super::dialect::Dialect;
use super::lexer::{Position, Token, TokenKind};

/// The cause of a failure of the statement check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementFault {
    /// The text has no word before the end (a text with blanks and comments only).
    NoFirstWord,
    /// The first word is not `SELECT` or `WITH`.
    FirstWord,
    /// A `;` at depth 0 that is not the last token that is not a blank or a comment.
    SecondStatement,
    /// A word `FORMAT` at depth 0 followed by a word.
    Format,
    /// A word `INTO` at depth 0 followed by the word `OUTFILE`.
    IntoOutfile,
    /// A word `INSERT` at depth 0 followed by the word `INTO`.
    InsertInto,
    /// `postgres`: the first word is not `SELECT`, `WITH`, `VALUES` or `TABLE` (7.25, rule 1).
    FirstWordPostgres,
    /// `postgres`: one of the words `INTO`, `INSERT`, `UPDATE`, `DELETE` and `MERGE` at any depth
    /// (7.25, rule 3). The value names the word.
    RefusedWord(&'static Refusal),
    /// `postgres`: a word `FOR` followed by `UPDATE`, `SHARE`, `NO` or `KEY` (7.25, rule 4).
    LockingClause,
    /// `postgres`: a parameter token such as `$1` (7.25, rule 5).
    Parameter,
    /// `postgres`: a `U&` prefix before a quote mark or a double quote (7.25, rule 6).
    UnicodeEscape,
    /// `postgres`: a call of a function of the closed list (7.25, rule 7). The value names the entry.
    RefusedFunction(&'static Refusal),
    /// `postgres`: a string, a quoted name, a dollar quote or a block comment that does not close.
    /// The lexer refuses it first. This check is a second guard.
    UnclosedToken,
}

/// An entry of a closed list of 7.25: a refused word or a refused function name.
#[derive(Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The name, in lower case. For a prefix entry it is the prefix.
    pub name: &'static str,
    /// True when each name that starts with `name` is refused.
    pub prefix: bool,
    /// The message of the failure. It names the entry.
    pub message: &'static str,
}

macro_rules! refusal {
    (word $n:literal) => {
        Refusal { name: $n, prefix: false, message: concat!("the word ", $n, " is not permitted") }
    };
    (fun $n:literal) => {
        Refusal { name: $n, prefix: false, message: concat!("the function ", $n, " is not permitted") }
    };
    (pre $n:literal) => {
        Refusal {
            name: $n,
            prefix: true,
            message: concat!("the functions with a name that starts with ", $n, " are not permitted"),
        }
    };
}

/// The words that 7.25 refuses at any depth (rule 3).
pub const REFUSED_WORDS: &[Refusal] = &[
    refusal!(word "into"),
    refusal!(word "insert"),
    refusal!(word "update"),
    refusal!(word "delete"),
    refusal!(word "merge"),
];

/// The closed list of the refused functions of 7.25 (rule 7), in the order of the table.
pub const REFUSED_FUNCTIONS: &[Refusal] = &[
    refusal!(fun "pg_sleep"),
    refusal!(fun "pg_sleep_for"),
    refusal!(fun "pg_sleep_until"),
    refusal!(fun "set_config"),
    refusal!(fun "pg_cancel_backend"),
    refusal!(fun "pg_terminate_backend"),
    refusal!(fun "pg_notify"),
    refusal!(pre "pg_advisory_"),
    refusal!(pre "pg_try_advisory_"),
    refusal!(fun "pg_read_file"),
    refusal!(fun "pg_read_binary_file"),
    refusal!(fun "pg_ls_dir"),
    refusal!(fun "pg_stat_file"),
    refusal!(fun "lo_import"),
    refusal!(fun "lo_export"),
    refusal!(pre "dblink"),
    refusal!(fun "query_to_xml"),
    refusal!(fun "query_to_xml_and_xmlschema"),
    refusal!(fun "query_to_xmlschema"),
    refusal!(fun "cursor_to_xml"),
    refusal!(fun "cursor_to_xmlschema"),
    refusal!(fun "pg_reload_conf"),
    refusal!(fun "pg_rotate_logfile"),
    refusal!(fun "pg_switch_wal"),
    refusal!(fun "pg_promote"),
    refusal!(fun "pg_create_restore_point"),
    refusal!(fun "pg_logical_emit_message"),
    refusal!(fun "ts_stat"),
    refusal!(fun "ts_rewrite"),
    refusal!(pre "crosstab"),
    refusal!(fun "connectby"),
    refusal!(fun "xpath_table"),
];

impl StatementFault {
    /// Gives a short English message.
    pub fn message(self) -> &'static str {
        match self {
            StatementFault::NoFirstWord => "the statement has no first word",
            StatementFault::FirstWord => "the first word must be SELECT or WITH",
            StatementFault::SecondStatement => "the text holds more than one statement",
            StatementFault::Format => "the clause FORMAT is not permitted: the server sets the format",
            StatementFault::IntoOutfile => "INTO OUTFILE is not permitted",
            StatementFault::InsertInto => "INSERT INTO is not permitted",
            StatementFault::FirstWordPostgres => "the first word must be SELECT, WITH, VALUES or TABLE",
            StatementFault::RefusedWord(r) | StatementFault::RefusedFunction(r) => r.message,
            StatementFault::LockingClause => "a locking clause (FOR UPDATE, FOR SHARE) is not permitted",
            StatementFault::Parameter => "a parameter ($1) is not permitted",
            StatementFault::UnicodeEscape => "the U& form is not permitted",
            StatementFault::UnclosedToken => "a string, a name, a dollar quote or a comment does not close",
        }
    }
}

/// A failure of the statement check, with the position of the token that fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementError {
    /// The cause.
    pub fault: StatementFault,
    /// The position of the token that fails, or the end of the text.
    pub position: Position,
}

impl std::fmt::Display for StatementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {}, column {})", self.fault.message(), self.position.line, self.position.column)
    }
}

impl std::error::Error for StatementError {}

fn is_punct(src: &str, t: &Token, c: u8) -> bool {
    t.kind == TokenKind::Punct && src.as_bytes().get(t.start) == Some(&c)
}

fn is_word(src: &str, t: &Token, word: &str) -> bool {
    t.kind == TokenKind::Word && t.text(src).eq_ignore_ascii_case(word)
}

/// Checks the statement (7.12). `tokens` MUST be the tokens of `src`.
///
/// The depth counts `(`, `[` and `{` punctuation tokens. The check skips the `(` tokens at the start
/// before it reads the first word, and it does not count them.
pub fn check_statement(src: &str, tokens: &[Token]) -> Result<(), StatementError> {
    let fail = |fault: StatementFault, at: usize| Err(StatementError { fault, position: Position::at(src, at) });
    let sig: Vec<&Token> = tokens.iter().filter(|t| !t.kind.is_trivia()).collect();
    let first = sig.iter().position(|t| !is_punct(src, t, b'(')).unwrap_or(sig.len());
    let Some(head) = sig.get(first) else {
        return fail(StatementFault::NoFirstWord, src.len());
    };
    if !(is_word(src, head, "SELECT") || is_word(src, head, "WITH")) {
        return fail(StatementFault::FirstWord, head.start);
    }
    let mut depth = 0usize;
    for (k, t) in sig.iter().enumerate().skip(first) {
        let next = sig.get(k + 1);
        match t.kind {
            TokenKind::Punct => match src.as_bytes().get(t.start) {
                Some(b'(' | b'[' | b'{') => depth += 1,
                Some(b')' | b']' | b'}') => depth = depth.saturating_sub(1),
                Some(b';') if depth == 0 && next.is_some() => return fail(StatementFault::SecondStatement, t.start),
                _ => {}
            },
            TokenKind::Word if depth == 0 => {
                let after_is = |word: Option<&str>| match (next, word) {
                    (Some(n), Some(w)) => is_word(src, n, w),
                    (Some(n), None) => n.kind == TokenKind::Word,
                    (None, _) => false,
                };
                if is_word(src, t, "FORMAT") && after_is(None) {
                    return fail(StatementFault::Format, t.start);
                }
                if is_word(src, t, "INTO") && after_is(Some("OUTFILE")) {
                    return fail(StatementFault::IntoOutfile, t.start);
                }
                if is_word(src, t, "INSERT") && after_is(Some("INTO")) {
                    return fail(StatementFault::InsertInto, t.start);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Checks the statement with the rules of a dialect: 7.12 for `clickhouse`, 7.25 for `postgres`.
/// `tokens` MUST be the tokens of `src` from the lexer of the same dialect.
pub fn check_statement_for(dialect: Dialect, src: &str, tokens: &[Token]) -> Result<(), StatementError> {
    match dialect {
        Dialect::ClickHouse => check_statement(src, tokens),
        Dialect::Postgres => check_statement_postgres(src, tokens),
    }
}

fn is_open(src: &str, t: &Token) -> bool {
    t.kind == TokenKind::Punct && matches!(src.as_bytes().get(t.start), Some(b'(' | b'[' | b'{'))
}

fn is_close(src: &str, t: &Token) -> bool {
    t.kind == TokenKind::Punct && matches!(src.as_bytes().get(t.start), Some(b')' | b']' | b'}'))
}

/// Gives the name of a word or of a quoted name, in lower case. A quoted name loses its quote marks,
/// and a doubled `""` is one quote mark. Any other token gives `None`.
fn name_of(src: &str, t: &Token) -> Option<String> {
    let text = t.text(src);
    match t.kind {
        TokenKind::Word => Some(text.to_ascii_lowercase()),
        TokenKind::QuotedName => {
            let inner = text.strip_prefix('"')?.strip_suffix('"')?;
            Some(inner.replace("\"\"", "\"").to_ascii_lowercase())
        }
        _ => None,
    }
}

fn is_unicode_prefix(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 3 && b[0].eq_ignore_ascii_case(&b'u') && b[1] == b'&' && (b[2] == b'\'' || b[2] == b'"')
}

/// Checks the statement of the dialect `postgres` (7.25). `tokens` MUST be the tokens of `src` from
/// the lexer of 7.20.
///
/// The rules are: the first word (1), one statement (2), the refused words (3), the locking clauses
/// (4), the parameters (5), the `U&` forms (6) and the refused function calls (7). The depth counts
/// `(`, `[` and `{` tokens. Words and names compare with ASCII letters folded.
///
/// Two rules are stricter than the text of 7.25, because the server refuses the same texts. A `;` at
/// depth 1 or more fails, and a token that does not close fails. A quoted name matches a refused
/// function in any case.
pub fn check_statement_postgres(src: &str, tokens: &[Token]) -> Result<(), StatementError> {
    let fail = |fault: StatementFault, at: usize| Err(StatementError { fault, position: Position::at(src, at) });
    if let Some(t) = tokens.iter().find(|t| !t.closed) {
        return fail(StatementFault::UnclosedToken, t.start);
    }
    let sig: Vec<&Token> = tokens.iter().filter(|t| !t.kind.is_trivia()).collect();
    let first = sig.iter().position(|t| !is_punct(src, t, b'(')).unwrap_or(sig.len());
    let Some(head) = sig.get(first) else {
        return fail(StatementFault::NoFirstWord, src.len());
    };
    if !["SELECT", "WITH", "VALUES", "TABLE"].iter().any(|w| is_word(src, head, w)) {
        return fail(StatementFault::FirstWordPostgres, head.start);
    }
    let mut depth = 0usize;
    for (k, t) in sig.iter().enumerate().skip(first) {
        let next = sig.get(k + 1);
        if is_open(src, t) {
            depth += 1;
        } else if is_close(src, t) {
            depth = depth.saturating_sub(1);
        } else if is_punct(src, t, b';') && (depth > 0 || next.is_some()) {
            return fail(StatementFault::SecondStatement, t.start);
        }
        match t.kind {
            TokenKind::Parameter => return fail(StatementFault::Parameter, t.start),
            TokenKind::PrefixedString | TokenKind::QuotedName if is_unicode_prefix(t.text(src)) => {
                return fail(StatementFault::UnicodeEscape, t.start);
            }
            // A `U` that a gap-free `&` and a quote follow. The lexer joins them, so this is a guard.
            TokenKind::Word if t.text(src).eq_ignore_ascii_case("u") => {
                if let (Some(amp), Some(quote)) = (sig.get(k + 1), sig.get(k + 2)) {
                    let joined = t.end == amp.start && amp.end == quote.start;
                    if joined && amp.text(src) == "&" && quote.text(src).starts_with(['\'', '"']) {
                        return fail(StatementFault::UnicodeEscape, t.start);
                    }
                }
            }
            _ => {}
        }
        if t.kind == TokenKind::Word {
            let word = t.text(src);
            if let Some(r) = REFUSED_WORDS.iter().find(|r| word.eq_ignore_ascii_case(r.name)) {
                return fail(StatementFault::RefusedWord(r), t.start);
            }
            // `FOR UPDATE` fails at the word `UPDATE` next, as the table of 7.25 shows.
            if word.eq_ignore_ascii_case("FOR")
                && next.is_some_and(|n| ["SHARE", "NO", "KEY"].iter().any(|w| is_word(src, n, w)))
            {
                return fail(StatementFault::LockingClause, t.start);
            }
        }
        if next.is_some_and(|n| is_punct(src, n, b'('))
            && let Some(name) = name_of(src, t)
            && let Some(r) =
                REFUSED_FUNCTIONS.iter().find(|r| if r.prefix { name.starts_with(r.name) } else { name == r.name })
        {
            return fail(StatementFault::RefusedFunction(r), t.start);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::lexer::lex_all;

    fn check(src: &str) -> Result<(), StatementFault> {
        check_statement(src, &lex_all(src)).map_err(|e| e.fault)
    }

    #[test]
    fn table_of_7_12() {
        assert_eq!(check("SELECT 1"), Ok(()));
        assert_eq!(check("/* c */ (SELECT 1) UNION ALL (SELECT 2)"), Ok(()));
        assert_eq!(check("WITH x AS (SELECT 1) SELECT * FROM x;"), Ok(()));
        assert_eq!(check("SELECT format('{} {}', a, b) FROM t"), Ok(()));
        assert_eq!(check("SELECT 1; SELECT 2"), Err(StatementFault::SecondStatement));
        assert_eq!(check("SELECT 1 FORMAT JSON"), Err(StatementFault::Format));
        assert_eq!(check("SHOW TABLES"), Err(StatementFault::FirstWord));
        assert_eq!(check("WITH 1 AS x INSERT INTO t SELECT x"), Err(StatementFault::InsertInto));
        assert_eq!(check("SELECT 1 INTO OUTFILE 'x'"), Err(StatementFault::IntoOutfile));
        assert_eq!(check("-- only a comment"), Err(StatementFault::NoFirstWord));
    }

    #[test]
    fn more_cases() {
        assert_eq!(check(""), Err(StatementFault::NoFirstWord));
        assert_eq!(check("  ((  "), Err(StatementFault::NoFirstWord));
        assert_eq!(check("select 1"), Ok(()));
        assert_eq!(check("SeLeCt 1 ; -- c\n /* d */"), Ok(()));
        assert_eq!(check("SELECT 1;;"), Err(StatementFault::SecondStatement));
        assert_eq!(check("SELECT 1; ;"), Err(StatementFault::SecondStatement));
        assert_eq!(check("SELECT ';' FROM `a;b`"), Ok(()));
        assert_eq!(check("SELECT (1; 2)"), Ok(()));
        assert_eq!(check("SELECT 1 format json"), Err(StatementFault::Format));
        assert_eq!(check("SELECT 1 FORMAT"), Ok(()));
        assert_eq!(check("SELECT 1 FORMAT /* c */ JSON"), Err(StatementFault::Format));
        assert_eq!(check("SELECT f(1 FORMAT JSON)"), Ok(()));
        assert_eq!(check("SELECT [1 FORMAT JSON]"), Ok(()));
        assert_eq!(check("SELECT 1 INTO x"), Ok(()));
        assert_eq!(check("SELECT 'a' AS insert, 1 AS into"), Ok(()));
        assert_eq!(check("(SELECT 1; SELECT 2)"), Err(StatementFault::SecondStatement));
        assert_eq!(check("SELECT 1 /* ; */ -- ;\n"), Ok(()));
        assert_eq!(check("\u{a0}SELECT 1"), Ok(()));
        assert_eq!(check("SELECT 1 )) ; SELECT 2"), Err(StatementFault::SecondStatement));
        assert_eq!(check("$__x SELECT 1"), Err(StatementFault::FirstWord));
    }

    #[test]
    fn positions() {
        let src = "SELECT 1\nFROM t FORMAT JSON";
        let e = check_statement(src, &lex_all(src)).unwrap_err();
        assert_eq!((e.position.line, e.position.column), (2, 8));
    }
}
