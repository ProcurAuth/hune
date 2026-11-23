// SPDX-License-Identifier: MIT OR Apache-2.0
//! The lexers of the two dialects: ClickHouse (7.5) and PostgreSQL (7.20).
//!
//! The rules of ClickHouse are the rules of `src/sql_lexer.hpp` of ClickHouse Dash at the commit
//! `e970694e5e0b17b07cf187f3128af93fa1a82b76`, with three changes that follow the measure of
//! ClickHouse 26.8.34.13 (7.5): `//` starts a line comment, a line comment ends at LF only, and a
//! heredoc tag can start with a digit. [`lex_all`] and [`lex`] read this dialect. The rules of
//! PostgreSQL follow `scan.l` of PostgreSQL 15 to 18. [`lex_all_for`] and [`lex_for`] pick the
//! dialect. A position is a byte offset. A lexer never panics. The tokens cover the text with no
//! gap, so the token texts joined in order give the text again.

use super::dialect::Dialect;
use std::fmt;

/// The kind of a token (7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A run of blanks.
    Blank,
    /// A line comment. The `clickhouse` lexer ends it before the next LF: a CR, FF, VT or Unicode line
    /// separator does not end it (7.5). The `postgres` lexer ends it before the next LF or CR (7.20).
    LineComment,
    /// A block comment. It nests.
    BlockComment,
    /// A keyword, a function, a bare name, or a word that starts with `$`.
    Word,
    /// A name in double quotes or in backquotes.
    QuotedName,
    /// A string in single quotes.
    String,
    /// A heredoc: `$tag$ ... $tag$` or `$$ ... $$`. The dialect `postgres` calls it a dollar quote.
    Heredoc,
    /// A number.
    Number,
    /// One other byte.
    Punct,
    /// A `#` that starts no comment.
    Unknown,
    /// An `E'...'` string of the dialect `postgres`, where a backslash escapes the next byte (7.20).
    /// The `clickhouse` lexer never makes this token.
    EString,
    /// A `B'...'`, `X'...'`, `N'...'` or `U&'...'` string of the dialect `postgres`. The prefix is
    /// part of the token (7.20). A `U&"..."` name is a `QuotedName`. The `clickhouse` lexer never
    /// makes this token.
    PrefixedString,
    /// A run of operator bytes of the dialect `postgres` (7.20). The `clickhouse` lexer never makes
    /// this token.
    Operator,
    /// A parameter `$` and digits of the dialect `postgres` (7.20). The `clickhouse` lexer never
    /// makes this token.
    Parameter,
}

impl TokenKind {
    /// Tells if the token is a code token: blank, word, number, punctuation or unknown (7.6).
    /// The tokens `Operator` and `Parameter` are code tokens too (7.20).
    pub fn is_code(self) -> bool {
        matches!(
            self,
            TokenKind::Blank
                | TokenKind::Word
                | TokenKind::Number
                | TokenKind::Punct
                | TokenKind::Unknown
                | TokenKind::Operator
                | TokenKind::Parameter
        )
    }

    /// Tells if the token is a comment.
    pub fn is_comment(self) -> bool {
        matches!(self, TokenKind::LineComment | TokenKind::BlockComment)
    }

    /// Tells if the token is a blank or a comment.
    pub fn is_trivia(self) -> bool {
        self == TokenKind::Blank || self.is_comment()
    }
}

/// One token. `start` and `end` are byte offsets in the text, and `end` is not in the token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// The kind of the token.
    pub kind: TokenKind,
    /// The offset of the first byte.
    pub start: usize,
    /// The offset after the last byte.
    pub end: usize,
    /// False when the text ends before a block comment, a string, a quoted name or a heredoc closes.
    pub closed: bool,
}

impl Token {
    /// Gives the text of the token. `src` MUST be the text that the lexer read.
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        src.get(self.start..self.end).unwrap_or("")
    }

    /// Gives the byte range of the content of a string or of a quoted name: the bytes between the
    /// quote marks, with the prefix (`E`, `B`, `U&` and the others) left out. An open token has its
    /// content to the end of the token. Another kind of token gives `None`. `src` MUST be the text
    /// that the lexer read.
    ///
    /// A part of a string after a line end (7.20) is a token of its own that starts at the quote
    /// mark, so it has no prefix.
    pub fn content_range(&self, src: &str) -> Option<(usize, usize)> {
        let first = *src.as_bytes().get(self.start)?;
        let quote_first = matches!(first, b'\'' | b'"' | b'`');
        let open = match self.kind {
            TokenKind::String => 1,
            TokenKind::QuotedName if quote_first => 1,
            TokenKind::QuotedName => 3,
            TokenKind::EString | TokenKind::PrefixedString if quote_first => 1,
            TokenKind::EString => 2,
            TokenKind::PrefixedString if matches!(first, b'U' | b'u') => 3,
            TokenKind::PrefixedString => 2,
            _ => return None,
        };
        let from = self.start + open;
        let to = if self.closed { self.end.saturating_sub(1) } else { self.end };
        (from <= to).then_some((from, to))
    }
}

/// A position in a text: the byte offset, the line (from 1) and the column (from 1).
///
/// The lines end at LF. The column counts Unicode characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The byte offset.
    pub offset: usize,
    /// The line, from 1.
    pub line: u32,
    /// The column in characters, from 1.
    pub column: u32,
}

impl Position {
    /// Computes the position of a byte offset. An offset past the end gives the end.
    pub fn at(text: &str, offset: usize) -> Position {
        let offset = offset.min(text.len());
        let mut line = 1u32;
        let mut column = 1u32;
        for (i, c) in text.char_indices() {
            if i >= offset {
                break;
            }
            if c == '\n' {
                line = line.saturating_add(1);
                column = 1;
            } else {
                column = column.saturating_add(1);
            }
        }
        Position { offset, line, column }
    }
}

/// Reads the positions of many offsets of one text in one pass. The offsets come in increasing order, so the
/// cost of all the positions is the length of the text. A smaller offset starts the pass again from the start.
///
/// [`PositionCursor::at`] gives the same value as [`Position::at`].
#[derive(Debug, Clone)]
pub struct PositionCursor<'a> {
    text: &'a str,
    /// The byte offset of the next character to read. It is on a character boundary.
    next: usize,
    line: u32,
    column: u32,
}

impl<'a> PositionCursor<'a> {
    /// Starts at the start of the text.
    pub fn new(text: &'a str) -> PositionCursor<'a> {
        PositionCursor { text, next: 0, line: 1, column: 1 }
    }

    /// Gives the position of a byte offset. An offset past the end gives the end.
    pub fn at(&mut self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        if offset < self.next {
            *self = PositionCursor::new(self.text);
        }
        let rest = self.text.get(self.next..).unwrap_or_default();
        let base = self.next;
        let mut next = self.text.len();
        for (i, c) in rest.char_indices() {
            if base + i >= offset {
                next = base + i;
                break;
            }
            if c == '\n' {
                self.line = self.line.saturating_add(1);
                self.column = 1;
            } else {
                self.column = self.column.saturating_add(1);
            }
        }
        self.next = next;
        Position { offset, line: self.line, column: self.column }
    }
}

/// The cause of a lexer error (7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexErrorKind {
    /// A `#` that starts no comment.
    UnknownToken,
    /// A block comment that the text ends before it closes.
    UnclosedBlockComment,
    /// A string that the text ends before it closes.
    UnclosedString,
    /// A quoted name that the text ends before it closes.
    UnclosedQuotedName,
    /// A heredoc that the text ends before it closes.
    UnclosedHeredoc,
}

impl LexErrorKind {
    /// Gives a short English message.
    pub fn message(self) -> &'static str {
        match self {
            LexErrorKind::UnknownToken => "a # that starts no comment is not a token",
            LexErrorKind::UnclosedBlockComment => "a block comment is not closed",
            LexErrorKind::UnclosedString => "a string is not closed",
            LexErrorKind::UnclosedQuotedName => "a quoted name is not closed",
            LexErrorKind::UnclosedHeredoc => "a heredoc is not closed",
        }
    }
}

/// A lexer error with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// The cause.
    pub kind: LexErrorKind,
    /// The position of the first byte of the token.
    pub position: Position,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (line {}, column {})", self.kind.message(), self.position.line, self.position.column)
    }
}

impl std::error::Error for LexError {}

/// Gives the lexer error of a token, or `None` when the token is valid.
pub fn token_fault(token: &Token) -> Option<LexErrorKind> {
    match token.kind {
        TokenKind::Unknown => Some(LexErrorKind::UnknownToken),
        TokenKind::BlockComment if !token.closed => Some(LexErrorKind::UnclosedBlockComment),
        TokenKind::String | TokenKind::EString | TokenKind::PrefixedString if !token.closed => {
            Some(LexErrorKind::UnclosedString)
        }
        TokenKind::QuotedName if !token.closed => Some(LexErrorKind::UnclosedQuotedName),
        TokenKind::Heredoc if !token.closed => Some(LexErrorKind::UnclosedHeredoc),
        _ => None,
    }
}

/// Reads the text with the lexer of ClickHouse (7.5) and gives all tokens, with no error check.
///
/// The tokens cover the text with no gap. Use [`lex`] to refuse an unknown or unclosed token.
pub fn lex_all(src: &str) -> Vec<Token> {
    lex_all_for(Dialect::ClickHouse, src)
}

/// Reads the text with the lexer of the dialect and gives all tokens, with no error check.
///
/// The tokens cover the text with no gap. Use [`lex_for`] to refuse an unknown or unclosed token.
pub fn lex_all_for(dialect: Dialect, src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    if dialect == Dialect::Postgres {
        return lex_all_pg(b);
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < b.len() {
        let t = next_token(b, pos);
        pos = t.end;
        out.push(t);
    }
    out
}

/// Reads the text with the lexer of ClickHouse and gives the tokens. The first unknown or unclosed
/// token is an error.
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    lex_for(Dialect::ClickHouse, src)
}

/// Reads the text with the lexer of the dialect and gives the tokens. The first unknown or unclosed
/// token is an error.
pub fn lex_for(dialect: Dialect, src: &str) -> Result<Vec<Token>, LexError> {
    let tokens = lex_all_for(dialect, src);
    for t in &tokens {
        if let Some(kind) = token_fault(t) {
            return Err(LexError { kind, position: Position::at(src, t.start) });
        }
    }
    Ok(tokens)
}

/// Gives the UTF-16 unit that matches a byte offset. The editor of the page counts these units (10.3).
pub fn utf16_offset(src: &str, byte: usize) -> usize {
    let mut units = 0;
    for (i, c) in src.char_indices() {
        if i >= byte {
            break;
        }
        units += c.len_utf16();
    }
    units
}

fn unicode_space_length(b: &[u8], pos: usize) -> usize {
    let at = |i: usize| b.get(pos + i).copied().unwrap_or(0);
    let (c0, c1, c2) = (at(0), at(1), at(2));
    if c0 == 0xC2 && (c1 == 0x85 || c1 == 0xA0) {
        return 2;
    }
    if c0 == 0xE1 && c1 == 0xA0 && c2 == 0x8E {
        return 3;
    }
    if c0 == 0xE2 && c1 == 0x80 && ((0x80..=0x8D).contains(&c2) || c2 == 0xA8 || c2 == 0xA9 || c2 == 0xAF) {
        return 3;
    }
    if c0 == 0xE2 && c1 == 0x81 && (c2 == 0x9F || c2 == 0xA0) {
        return 3;
    }
    if (c0 == 0xE3 && c1 == 0x80 && c2 == 0x80) || (c0 == 0xEF && c1 == 0xBB && c2 == 0xBF) {
        return 3;
    }
    0
}

fn blank_length(b: &[u8], pos: usize) -> usize {
    match b[pos] {
        b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B => 1,
        c if c >= 0x80 => unicode_space_length(b, pos),
        _ => 0,
    }
}

/// Tells if a line comment of `clickhouse` starts at `pos` (7.5): `--`, `//` (so `///` too), or `#` followed by a
/// space or `!`.
fn starts_line_comment(b: &[u8], pos: usize) -> bool {
    match b.get(pos) {
        Some(b'-') => b.get(pos + 1) == Some(&b'-'),
        Some(b'/') => b.get(pos + 1) == Some(&b'/'),
        Some(b'#') => matches!(b.get(pos + 1), Some(b' ') | Some(b'!')),
        _ => false,
    }
}

fn starts_block_comment(b: &[u8], pos: usize) -> bool {
    b.get(pos) == Some(&b'/') && b.get(pos + 1) == Some(&b'*')
}

/// Gives the end of a line comment of `clickhouse` (7.5): the offset of the next LF, or the end of the text.
/// ClickHouse 26.8.34.13 ends the comment at LF only: a CR, FF, VT, NUL, U+0085, U+2028 or U+2029 stays in it.
fn line_comment_end(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && b[pos] != b'\n' {
        pos += 1;
    }
    pos
}

/// Gives the end of a line comment of `postgres` (7.20): the offset of the next LF or CR, or the end of the text.
fn line_comment_end_pg(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && b[pos] != b'\n' && b[pos] != b'\r' {
        pos += 1;
    }
    pos
}

fn block_comment_end(b: &[u8], mut pos: usize) -> (usize, bool) {
    let mut depth = 0usize;
    while pos < b.len() {
        if starts_block_comment(b, pos) {
            depth += 1;
            pos += 2;
        } else if b[pos] == b'*' && b.get(pos + 1) == Some(&b'/') {
            pos += 2;
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return (pos, true);
            }
        } else {
            pos += 1;
        }
    }
    (b.len(), false)
}

fn quoted_end(b: &[u8], pos: usize) -> (usize, bool) {
    let quote = b[pos];
    let mut p = pos + 1;
    while p < b.len() {
        if b[p] == b'\\' {
            p += 2;
        } else if b[p] == quote {
            if b.get(p + 1) == Some(&quote) {
                p += 2;
            } else {
                return (p + 1, true);
            }
        } else {
            p += 1;
        }
    }
    (b.len(), false)
}

fn is_word_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn is_word_char(c: u8) -> bool {
    is_word_start(c) || c.is_ascii_digit() || c == b'$'
}

/// Gives the length of the heredoc tag (`$$` or `$tag$`) that starts at `pos`, or 0. The tag has ASCII letters,
/// digits and `_`, and it can start with a digit: ClickHouse 26.8.34.13 reads `$1$ ... $1$` as a heredoc (7.5).
pub(crate) fn heredoc_tag_length(b: &[u8], pos: usize) -> usize {
    if b.get(pos) != Some(&b'$') {
        return 0;
    }
    let mut i = pos + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    if b.get(i) == Some(&b'$') { i - pos + 1 } else { 0 }
}

fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn heredoc_end(b: &[u8], pos: usize, tag: usize) -> (usize, bool) {
    match find_from(b, &b[pos..pos + tag], pos + tag) {
        Some(found) => (found + tag, true),
        None => (b.len(), false),
    }
}

fn number_end(b: &[u8], mut pos: usize) -> usize {
    let hex = b.get(pos) == Some(&b'0') && matches!(b.get(pos + 1), Some(b'x') | Some(b'X'));
    while pos < b.len() {
        let c = b[pos];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
            let exponent = if hex { c == b'p' || c == b'P' } else { c == b'e' || c == b'E' };
            pos += 1;
            if exponent && matches!(b.get(pos), Some(b'+') | Some(b'-')) {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

fn word_end(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && is_word_char(b[pos]) {
        if b[pos] >= 0x80 && unicode_space_length(b, pos) > 0 {
            break;
        }
        pos += 1;
    }
    pos
}

fn next_token(b: &[u8], pos: usize) -> Token {
    let c = b[pos];
    let mut closed = true;
    let blank = blank_length(b, pos);
    let (kind, mut end) = if blank > 0 {
        let mut i = pos + blank;
        while i < b.len() {
            let more = blank_length(b, i);
            if more == 0 {
                break;
            }
            i += more;
        }
        (TokenKind::Blank, i)
    } else if starts_line_comment(b, pos) {
        (TokenKind::LineComment, line_comment_end(b, pos))
    } else if starts_block_comment(b, pos) {
        let (e, cl) = block_comment_end(b, pos);
        closed = cl;
        (TokenKind::BlockComment, e)
    } else if c == b'\'' {
        let (e, cl) = quoted_end(b, pos);
        closed = cl;
        (TokenKind::String, e)
    } else if c == b'"' || c == b'`' {
        let (e, cl) = quoted_end(b, pos);
        closed = cl;
        (TokenKind::QuotedName, e)
    } else if c == b'$' && heredoc_tag_length(b, pos) > 0 {
        let (e, cl) = heredoc_end(b, pos, heredoc_tag_length(b, pos));
        closed = cl;
        (TokenKind::Heredoc, e)
    } else if c.is_ascii_digit() || (c == b'.' && b.get(pos + 1).is_some_and(u8::is_ascii_digit)) {
        (TokenKind::Number, number_end(b, pos))
    } else if is_word_start(c) || c == b'$' {
        (TokenKind::Word, word_end(b, pos))
    } else if c == b'#' {
        (TokenKind::Unknown, pos + 1)
    } else {
        (TokenKind::Punct, pos + 1)
    };
    if end <= pos {
        end = pos + 1;
    }
    Token { kind, start: pos, end: end.min(b.len()), closed }
}

// ---- the dialect `postgres` (7.20) ----

/// Tells if a byte is a blank of PostgreSQL: space, TAB, LF, CR, FF or VT. A byte from 0x80 is never
/// a blank, so a Unicode space is a word byte.
fn is_pg_blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B)
}

/// Tells if a byte can be in a run of operator bytes (7.20).
fn is_pg_operator_byte(c: u8) -> bool {
    matches!(
        c,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'<'
            | b'>'
            | b'='
            | b'~'
            | b'!'
            | b'@'
            | b'#'
            | b'%'
            | b'^'
            | b'&'
            | b'|'
            | b'`'
            | b'?'
    )
}

/// Tells if an operator byte forbids the removal of a trailing `+` or `-` (7.20).
fn is_pg_special_operator_byte(c: u8) -> bool {
    matches!(c, b'~' | b'!' | b'@' | b'#' | b'%' | b'^' | b'&' | b'|' | b'`' | b'?')
}

/// Gives the end of the operator token that starts at `pos` (7.20). `pos` is on an operator byte
/// that starts no comment.
fn pg_operator_end(b: &[u8], pos: usize) -> usize {
    let mut end = pos;
    while end < b.len() && is_pg_operator_byte(b[end]) {
        if end > pos && (starts_line_comment_pg(b, end) || starts_block_comment(b, end)) {
            break;
        }
        end += 1;
    }
    let last_sign = |e: usize| matches!(b[e - 1], b'+' | b'-');
    if end - pos > 1 && last_sign(end) && !b[pos..end - 1].iter().any(|&c| is_pg_special_operator_byte(c)) {
        end -= 1;
        while end - pos > 1 && last_sign(end) {
            end -= 1;
        }
    }
    end
}

fn starts_line_comment_pg(b: &[u8], pos: usize) -> bool {
    b.get(pos) == Some(&b'-') && b.get(pos + 1) == Some(&b'-')
}

/// Finds the opening quote of a continuation of a string, as `scan.l` does (7.20).
///
/// `from` is the offset after a closing quote. A continuation is: blanks, TABs, VTs and FFs, or line
/// comments, then one LF or CR, then blanks or line comments that end in a line end, then a quote.
/// The string goes on in the same mode: a continuation of an `E` string keeps its backslash escape.
/// The function gives the offset of the opening quote, or `None`.
fn pg_continuation(b: &[u8], from: usize) -> Option<usize> {
    let mut p = from;
    loop {
        match b.get(p) {
            Some(b' ' | b'\t' | 0x0B | 0x0C) => p += 1,
            Some(b'-') if starts_line_comment_pg(b, p) => p = line_comment_end_pg(b, p),
            _ => break,
        }
    }
    if !matches!(b.get(p), Some(b'\n' | b'\r')) {
        return None;
    }
    p += 1;
    loop {
        match b.get(p) {
            Some(c) if is_pg_blank(*c) => p += 1,
            Some(b'-') if starts_line_comment_pg(b, p) => {
                let e = line_comment_end_pg(b, p);
                if !matches!(b.get(e), Some(b'\n' | b'\r')) {
                    return None;
                }
                p = e + 1;
            }
            _ => break,
        }
    }
    (b.get(p) == Some(&b'\'')).then_some(p)
}

/// Gives the end of one part of a string that has its opening quote at `open` (7.20). `escapes` is
/// true for an `E` string. A doubled quote is one quote. A part that follows a line end is another
/// part: [`lex_all_pg`] makes it a token of its own, with the same mode.
fn pg_string_part(b: &[u8], open: usize, escapes: bool) -> (usize, bool) {
    let mut p = open + 1;
    while p < b.len() {
        if escapes && b[p] == b'\\' {
            p += 2;
        } else if b[p] == b'\'' {
            if b.get(p + 1) == Some(&b'\'') {
                p += 2;
            } else {
                return (p + 1, true);
            }
        } else {
            p += 1;
        }
    }
    (b.len(), false)
}

/// Reads the text with the lexer of `postgres`. A string that a line end and a quote mark continue is
/// one literal for PostgreSQL, in the mode of its first part (7.20). The tokens are: the first part, the
/// blanks and the line comments of the gap, and the next part, as a token of the same kind. The next
/// part starts at its quote mark, and it is an `E` string when the first part is.
fn lex_all_pg(b: &[u8]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < b.len() {
        let t = next_token_pg(b, pos);
        pos = t.end;
        out.push(t);
        if !t.closed || !matches!(t.kind, TokenKind::String | TokenKind::EString | TokenKind::PrefixedString) {
            continue;
        }
        while let Some(open) = pg_continuation(b, pos) {
            while pos < open {
                let gap = next_token_pg(b, pos);
                pos = gap.end;
                out.push(gap);
            }
            let (end, closed) = pg_string_part(b, open, t.kind == TokenKind::EString);
            out.push(Token { kind: t.kind, start: open, end, closed });
            pos = end;
            if !closed {
                break;
            }
        }
    }
    out
}

/// Gives the end of a quoted name that has its opening quote at `open` (7.20). A doubled quote is
/// one quote. A backslash is an ordinary byte.
fn pg_quoted_name_end(b: &[u8], open: usize) -> (usize, bool) {
    let mut p = open + 1;
    while p < b.len() {
        if b[p] == b'"' {
            if b.get(p + 1) == Some(&b'"') {
                p += 2;
            } else {
                return (p + 1, true);
            }
        } else {
            p += 1;
        }
    }
    (b.len(), false)
}

/// Gives the length of the dollar quote tag (`$$` or `$tag$`) that starts at `pos`, or 0 (7.20).
/// The tag starts with an ASCII letter, `_` or a byte from 0x80. It goes on with these bytes and
/// digits. It has no `$`.
pub(crate) fn pg_dollar_tag_length(b: &[u8], pos: usize) -> usize {
    if b.get(pos) != Some(&b'$') {
        return 0;
    }
    let mut i = pos + 1;
    if b.get(i).is_some_and(|&c| is_word_start(c)) {
        while b.get(i).is_some_and(|&c| is_word_start(c) || c.is_ascii_digit()) {
            i += 1;
        }
    }
    if b.get(i) == Some(&b'$') { i - pos + 1 } else { 0 }
}

fn pg_word_end(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && is_word_char(b[pos]) {
        pos += 1;
    }
    pos
}

/// Reads the prefix of a string or of a quoted name at `pos`. It gives the token kind and the
/// offset of the opening quote. The prefix is read only at the start of a token.
fn pg_prefix(b: &[u8], pos: usize) -> Option<(TokenKind, usize)> {
    let at = |i: usize| b.get(pos + i).copied();
    match (b[pos], at(1), at(2)) {
        (b'E' | b'e', Some(b'\''), _) => Some((TokenKind::EString, pos + 1)),
        (b'B' | b'b' | b'X' | b'x' | b'N' | b'n', Some(b'\''), _) => Some((TokenKind::PrefixedString, pos + 1)),
        (b'U' | b'u', Some(b'&'), Some(b'\'')) => Some((TokenKind::PrefixedString, pos + 2)),
        (b'U' | b'u', Some(b'&'), Some(b'"')) => Some((TokenKind::QuotedName, pos + 2)),
        _ => None,
    }
}

fn next_token_pg(b: &[u8], pos: usize) -> Token {
    let c = b[pos];
    let mut closed = true;
    let (kind, mut end) = if is_pg_blank(c) {
        let mut i = pos + 1;
        while i < b.len() && is_pg_blank(b[i]) {
            i += 1;
        }
        (TokenKind::Blank, i)
    } else if starts_line_comment_pg(b, pos) {
        (TokenKind::LineComment, line_comment_end_pg(b, pos))
    } else if starts_block_comment(b, pos) {
        let (e, cl) = block_comment_end(b, pos);
        closed = cl;
        (TokenKind::BlockComment, e)
    } else if c == b'\'' {
        let (e, cl) = pg_string_part(b, pos, false);
        closed = cl;
        (TokenKind::String, e)
    } else if c == b'"' {
        let (e, cl) = pg_quoted_name_end(b, pos);
        closed = cl;
        (TokenKind::QuotedName, e)
    } else if let Some((kind, open)) = pg_prefix(b, pos) {
        let (e, cl) = if kind == TokenKind::QuotedName {
            pg_quoted_name_end(b, open)
        } else {
            pg_string_part(b, open, kind == TokenKind::EString)
        };
        closed = cl;
        (kind, e)
    } else if c == b'$' {
        let tag = pg_dollar_tag_length(b, pos);
        if tag > 0 {
            let (e, cl) = heredoc_end(b, pos, tag);
            closed = cl;
            (TokenKind::Heredoc, e)
        } else if b.get(pos + 1).is_some_and(u8::is_ascii_digit) {
            let mut i = pos + 1;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            (TokenKind::Parameter, i)
        } else {
            (TokenKind::Word, pg_word_end(b, pos + 1))
        }
    } else if c.is_ascii_digit() || (c == b'.' && b.get(pos + 1).is_some_and(u8::is_ascii_digit)) {
        (TokenKind::Number, number_end(b, pos))
    } else if is_word_start(c) {
        (TokenKind::Word, pg_word_end(b, pos))
    } else if is_pg_operator_byte(c) {
        (TokenKind::Operator, pg_operator_end(b, pos))
    } else {
        (TokenKind::Punct, pos + 1)
    };
    if end <= pos {
        end = pos + 1;
    }
    Token { kind, start: pos, end: end.min(b.len()), closed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_gives_the_positions_of_position_at() {
        let text = "SELECT 'é' -- ü\n\n  x𝄞y\n$b";
        let offsets: Vec<usize> = (0..=text.len() + 2).collect();
        let mut cursor = PositionCursor::new(text);
        for &o in &offsets {
            assert_eq!(cursor.at(o), Position::at(text, o), "offset {o}");
        }
        // A smaller offset starts again from the start.
        for &o in offsets.iter().rev() {
            assert_eq!(cursor.at(o), Position::at(text, o), "offset {o}");
        }
        for o in [3, 9, 9, 1, 20, 8] {
            assert_eq!(cursor.at(o), Position::at(text, o), "offset {o}");
        }
    }

    fn kinds(src: &str) -> String {
        lex_all(src)
            .iter()
            .map(|t| match t.kind {
                TokenKind::Blank => '_',
                TokenKind::LineComment => 'L',
                TokenKind::BlockComment => {
                    if t.closed {
                        'B'
                    } else {
                        'b'
                    }
                }
                TokenKind::Word => 'W',
                TokenKind::QuotedName => {
                    if t.closed {
                        'Q'
                    } else {
                        'q'
                    }
                }
                TokenKind::String => {
                    if t.closed {
                        'S'
                    } else {
                        's'
                    }
                }
                TokenKind::Heredoc => {
                    if t.closed {
                        'H'
                    } else {
                        'h'
                    }
                }
                TokenKind::Number => 'N',
                TokenKind::Punct => 'P',
                TokenKind::Unknown => 'U',
                // The `postgres` kinds (7.20): the `clickhouse` lexer never makes them.
                TokenKind::EString => {
                    if t.closed {
                        'E'
                    } else {
                        'e'
                    }
                }
                TokenKind::PrefixedString => {
                    if t.closed {
                        'S'
                    } else {
                        's'
                    }
                }
                TokenKind::Operator => 'O',
                TokenKind::Parameter => 'R',
            })
            .collect()
    }

    #[test]
    fn table_of_7_5() {
        let cases: &[(&str, &str)] = &[
            ("SELECT 1", "W_N"),
            ("a.b", "WPW"),
            ("1--2", "NL"),
            ("1 # x\n2", "N_L_N"),
            ("1 #! x", "N_L"),
            ("1 #x", "N_UW"),
            // The measure of ClickHouse 26.8.34.13 (7.5): `//` and `///` start a line comment, `/ /` is two
            // punctuation tokens, and a CR, FF or U+2028 does not end a line comment.
            ("1 // x\n2", "N_L_N"),
            ("1 /// x\n2", "N_L_N"),
            ("1 / / x", "N_P_P_W"),
            ("1 -- x\r2", "N_L"),
            ("1 # x\r2", "N_L"),
            ("1 #! x\r2", "N_L"),
            ("1 // x\r2", "N_L"),
            ("1 -- x\r\n2", "N_L_N"),
            ("1 -- x\x0c2", "N_L"),
            ("1 -- x\u{2028}2", "N_L"),
            ("1 -- /*\n2", "N_L_N"),
            ("1 /* // */ 2", "N_B_N"),
            ("1 /* a /* b */ c */ 2", "N_B_N"),
            ("1 /* a /* b */", "N_b"),
            ("'a\\'b'", "S"),
            ("'a''b'", "S"),
            ("'abc", "s"),
            ("`a\\`b`", "Q"),
            ("$$a'b$$", "H"),
            ("$tag$a$$b$tag$", "H"),
            // A heredoc tag can start with a digit (ClickHouse 26.8.34.13).
            ("$1$a'b$1$", "H"),
            ("$1a$a'b$1a$", "H"),
            ("$9_$a'b$9_$", "H"),
            // ClickHouse reads an open `$1$x` as a name. Hune reads an open heredoc: a lexer error, so it refuses more.
            ("$1$x", "h"),
            ("a$b", "W"),
            ("$1", "W"),
            ("1-2", "NPN"),
            ("KILL\u{a0}QUERY", "W_W"),
            ("0x1F 1e3 1_000 .5 1.e3 0b101 1e-5 0x1p-3", "N_N_N_N_N_N_N_N"),
        ];
        for (src, want) in cases {
            assert_eq!(&kinds(src), want, "{src:?}");
        }
    }

    #[test]
    fn unicode_spaces_are_blanks() {
        for c in [
            '\u{85}', '\u{a0}', '\u{180e}', '\u{200b}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{2060}',
            '\u{3000}', '\u{feff}',
        ] {
            let s = format!("KILL{c}QUERY");
            assert_eq!(kinds(&s), "W_W", "U+{:04X}", c as u32);
        }
        assert_eq!(kinds("\u{feff}SELECT"), "_W");
    }

    #[test]
    fn tokens_cover_the_text() {
        for src in [
            "SELECT 'a' /* c */ FROM `t` -- x\nWHERE a = $$b$$ #! y",
            "'",
            "\\",
            "/*",
            "$",
            "\u{ff}",
            "'a\\",
            "é'\\é",
            "",
        ] {
            let mut at = 0;
            for t in lex_all(src) {
                assert_eq!(t.start, at);
                assert!(t.end > t.start);
                assert!(src.is_char_boundary(t.start) && src.is_char_boundary(t.end), "{src:?}");
                at = t.end;
            }
            assert_eq!(at, src.len());
        }
    }

    #[test]
    fn errors_have_positions() {
        let e = lex("SELECT 1\n  FROM 'abc").unwrap_err();
        assert_eq!(e.kind, LexErrorKind::UnclosedString);
        assert_eq!((e.position.line, e.position.column, e.position.offset), (2, 8, 16));
        assert_eq!(lex("a #x").unwrap_err().kind, LexErrorKind::UnknownToken);
        assert_eq!(lex("/* a").unwrap_err().kind, LexErrorKind::UnclosedBlockComment);
        assert_eq!(lex("`a").unwrap_err().kind, LexErrorKind::UnclosedQuotedName);
        assert_eq!(lex("$$a").unwrap_err().kind, LexErrorKind::UnclosedHeredoc);
        assert!(lex("-- only a comment").is_ok());
    }

    #[test]
    fn utf16_units() {
        assert_eq!(utf16_offset("abc", 2), 2);
        assert_eq!(utf16_offset("a\u{e9}b", 3), 2);
        assert_eq!(utf16_offset("a\u{1f600}b", 5), 3);
    }
}
