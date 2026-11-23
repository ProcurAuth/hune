// SPDX-License-Identifier: MIT OR Apache-2.0
//! The query parameters of ClickHouse (7.6a): the form `{name:Type}` in code.
//!
//! ClickHouse puts the value of the URL parameter `param_<name>` in place of each form with that name. The value
//! never becomes SQL text, so the final SQL is the same for any value (the property PQ). This module finds the
//! forms of a text, checks them against the variables of the document, checks the values with the grammar of their
//! type and makes the text of each value. It does no I/O.
//!
//! The candidate rule. ClickHouse skips blanks and comments, and it reads a quoted name as a name. Thus a code `{`
//! that a word or a quoted name and `:` follow, across blanks and comments, is a query parameter for ClickHouse.
//! The byte before the `{` is not `$`: `${name:format}` is a placeholder of 7.6. Each candidate MUST be the strict
//! form, because a `param_x` of the request applies to each candidate with the name `x`.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use super::dialect::Dialect;
use super::lexer::{Token, TokenKind};
use super::placeholders::{self, Found};
use super::values::{ParamShape, ResolvedValues, Selection, ValueRefused, is_number_text};

/// The most query parameters in one final text (7.6a, 7.13).
pub const MAX_QUERY_PARAMS: usize = 32;
/// The most items of one Array value (7.6a, 7.13).
pub const MAX_PARAM_ITEMS: usize = 1_000;
/// The most bytes of all the `&param_<name>=<value>` pairs of one request, after the percent encoding (7.6a, 7.13).
pub const MAX_PARAM_BYTES: usize = 4_096;

/// The end of the reason of a refusal at a cap (7.6a).
const LONG_LISTS: &str = "use the text form for long lists.";

/// A type of the closed list of 7.6a. The spelling and the case are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamType {
    /// `String`: any value of 7.7.
    String,
    /// `Int64`: an integer from -2^63 to 2^63 - 1, with no leading zero.
    Int64,
    /// `UInt64`: an integer from 0 to 2^64 - 1, with no leading zero.
    UInt64,
    /// `Float64`: the number grammar of 7.7, finite.
    Float64,
    /// `Date`: a calendar date `YYYY-MM-DD` from 1970-01-01 to 2149-06-06.
    Date,
    /// `Array(String)`.
    ArrayString,
    /// `Array(Int64)`.
    ArrayInt64,
    /// `Array(UInt64)`.
    ArrayUInt64,
}

impl ParamType {
    /// The eight types of 7.6a.
    pub const ALL: [ParamType; 8] = [
        ParamType::String,
        ParamType::Int64,
        ParamType::UInt64,
        ParamType::Float64,
        ParamType::Date,
        ParamType::ArrayString,
        ParamType::ArrayInt64,
        ParamType::ArrayUInt64,
    ];

    /// Gives the spelling of the type.
    pub fn name(self) -> &'static str {
        match self {
            ParamType::String => "String",
            ParamType::Int64 => "Int64",
            ParamType::UInt64 => "UInt64",
            ParamType::Float64 => "Float64",
            ParamType::Date => "Date",
            ParamType::ArrayString => "Array(String)",
            ParamType::ArrayInt64 => "Array(Int64)",
            ParamType::ArrayUInt64 => "Array(UInt64)",
        }
    }

    /// Reads the exact spelling of a type of the list. Another text gives `None`.
    pub fn parse(text: &str) -> Option<ParamType> {
        ParamType::ALL.into_iter().find(|t| t.name() == text)
    }

    /// Tells if the type is an Array type.
    pub fn is_array(self) -> bool {
        matches!(self, ParamType::ArrayString | ParamType::ArrayInt64 | ParamType::ArrayUInt64)
    }

    /// Gives the type of one item: the type itself for a scalar type.
    pub fn item(self) -> ParamType {
        match self {
            ParamType::ArrayString => ParamType::String,
            ParamType::ArrayInt64 => ParamType::Int64,
            ParamType::ArrayUInt64 => ParamType::UInt64,
            t => t,
        }
    }

    /// Tells if one value of the request matches the grammar of the type of one item (7.6a).
    pub fn accepts(self, value: &str) -> bool {
        match self.item() {
            ParamType::Int64 => int_in_range(value, true),
            ParamType::UInt64 => int_in_range(value, false),
            ParamType::Float64 => is_number_text(value) && value.parse::<f64>().is_ok_and(f64::is_finite),
            ParamType::Date => is_date(value),
            _ => true,
        }
    }
}

impl fmt::Display for ParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Tells if the text is `^-?(0|[1-9][0-9]*)$` (with `signed`) or `^(0|[1-9][0-9]*)$`, in the range of `Int64` or
/// `UInt64`. ClickHouse 26.8 refuses a leading zero with the code 457 (130 in an array), and it wraps a value out of
/// range with no error: both checks are necessary.
fn int_in_range(value: &str, signed: bool) -> bool {
    let (negative, digits) = match value.strip_prefix('-') {
        Some(rest) if signed => (true, rest),
        _ => (false, value),
    };
    let canonical = digits == "0" || digits.bytes().next().is_some_and(|b| (b'1'..=b'9').contains(&b));
    if !canonical || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 20 {
        return false;
    }
    let Ok(n) = digits.parse::<u128>() else { return false };
    match (signed, negative) {
        (true, true) => n <= 1u128 << 63,
        (true, false) => n < 1u128 << 63,
        _ => n <= u128::from(u64::MAX),
    }
}

/// Tells if the text is a real calendar date `YYYY-MM-DD` from 1970-01-01 to 2149-06-06, the range of `Date`.
fn is_date(value: &str) -> bool {
    let b = value.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    let number = |r: std::ops::Range<usize>| b[r].iter().fold(0u32, |n, d| n * 10 + u32::from(d - b'0'));
    let (y, m, d) = (number(0..4), number(5..7), number(8..10));
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    d >= 1 && d <= days && (1970, 1, 1) <= (y, m, d) && (y, m, d) <= (2149, 6, 6)
}

/// A candidate of 7.6a: a code `{` that ClickHouse reads as a query parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate<'a> {
    /// The strict form `{name:Type}` with a type of the list.
    Strict {
        /// The name.
        name: &'a str,
        /// The type.
        ty: ParamType,
        /// The offset of the `{`.
        start: usize,
        /// The offset after the `}`.
        end: usize,
    },
    /// The strict syntax with a type outside the list, for example `{x:Identifier}`.
    OtherType {
        /// The name.
        name: &'a str,
        /// The text of the type.
        ty: &'a str,
        /// The offset of the `{`.
        start: usize,
    },
    /// A loose form: a blank, a comment, a quoted name or another text between the braces.
    Loose {
        /// The offset of the `{`.
        start: usize,
    },
}

impl Candidate<'_> {
    /// The offset of the `{`.
    pub fn start(&self) -> usize {
        match *self {
            Candidate::Strict { start, .. } | Candidate::OtherType { start, .. } | Candidate::Loose { start } => start,
        }
    }
}

/// Tells if the text is a name of the strict form: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_param_name(text: &str) -> bool {
    let b = text.as_bytes();
    !b.is_empty() && (b[0].is_ascii_alphabetic() || b[0] == b'_') && b.iter().all(|c| placeholders::is_name_byte(*c))
}

/// Finds the candidates of 7.6a in a stored text, in the order of the text. `tokens` are all the tokens of `src`.
///
/// A `{` right after `$` is not a candidate here: `${name:format}` is a placeholder of 7.6. The scan reads code tokens
/// only, so a form in a string, a quoted name, a comment or a heredoc stays text. It is linear: the trivia that the
/// scan of one `{` skips come before the next `{`.
pub fn candidates<'a>(src: &'a str, tokens: &[Token]) -> Vec<Candidate<'a>> {
    scan(src, tokens, |_| false)
}

/// Finds the candidates of 7.6a in a final text (run time, step 3), in the order of the text. `stored` are the pairs
/// of the stored text.
///
/// Here a `{` right after `$` is a candidate too when its name has a pair in `stored`. ClickHouse 26.8 reads
/// `a${t:Identifier}` as the name `a$` with an alias that is a query parameter, so author text could forge a form
/// there. A `${name:format}` whose name is not a variable stays text (7.6), and the request has no pair for it.
pub fn final_candidates<'a>(
    src: &'a str,
    tokens: &[Token],
    stored: &BTreeMap<String, ParamType>,
) -> Vec<Candidate<'a>> {
    scan(src, tokens, |name| stored.contains_key(name))
}

/// The scan of [`candidates`] and [`final_candidates`]. `after_dollar` tells if a `{` right after `$`, with the given
/// text of the name, is a candidate.
fn scan<'a>(src: &'a str, tokens: &[Token], after_dollar: impl Fn(&str) -> bool) -> Vec<Candidate<'a>> {
    let b = src.as_bytes();
    let text = |t: &Token| src.get(t.start..t.end).unwrap_or("");
    let next_solid = |from: usize| (from..tokens.len()).find(|&k| !tokens[k].kind.is_trivia());
    let mut out = Vec::new();
    for (i, open) in tokens.iter().enumerate() {
        if !open.kind.is_code() || text(open) != "{" {
            continue;
        }
        let Some(j) = next_solid(i + 1) else { continue };
        let word = &tokens[j];
        if !matches!(word.kind, TokenKind::Word | TokenKind::QuotedName) {
            continue;
        }
        let Some(k) = next_solid(j + 1) else { continue };
        let colon = &tokens[k];
        if !colon.kind.is_code() || text(colon) != ":" {
            continue;
        }
        let name = text(word);
        if open.start > 0 && b[open.start - 1] == b'$' && !after_dollar(name) {
            continue;
        }
        let tight = word.kind == TokenKind::Word && word.start == open.end && colon.start == word.end;
        if !tight || !is_param_name(name) {
            out.push(Candidate::Loose { start: open.start });
            continue;
        }
        let after = colon.end;
        let rest = src.get(after..).unwrap_or("");
        let typed = ParamType::ALL
            .into_iter()
            .find(|t| rest.starts_with(t.name()) && b.get(after + t.name().len()) == Some(&b'}'));
        if let Some(ty) = typed {
            out.push(Candidate::Strict { name, ty, start: open.start, end: after + ty.name().len() + 1 });
            continue;
        }
        let run = rest.bytes().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'(' | b')')).count();
        if run > 0 && b.get(after + run) == Some(&b'}') {
            out.push(Candidate::OtherType { name, ty: &rest[..run], start: open.start });
        } else {
            out.push(Candidate::Loose { start: open.start });
        }
    }
    out
}

/// Finds the names of the placeholders of 7.6 that the expansion reads: in code, in a string and in a quoted name.
/// A name that starts with `__` is a macro and is not in the set. A comment holds no placeholder, and a placeholder
/// in a heredoc is a document error of its own (7.18, item 6).
pub fn text_form_names<'a>(dialect: Dialect, src: &'a str, tokens: &[Token]) -> HashSet<&'a str> {
    let mut out = HashSet::new();
    let mut read = |lo: usize, hi: usize| {
        let mut p = lo;
        while let Some(found) = placeholders::next_for(dialect, src, p, hi) {
            p = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.name.starts_with("__")
            {
                out.insert(ph.name);
            }
        }
    };
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        if t.kind.is_code() {
            let mut j = i;
            while j < tokens.len() && tokens[j].kind.is_code() {
                j += 1;
            }
            read(t.start, tokens[j - 1].end);
            i = j;
            continue;
        }
        match t.kind {
            TokenKind::String | TokenKind::QuotedName | TokenKind::EString => {
                if let Some((lo, hi)) = t.content_range(src) {
                    read(lo, hi);
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The code of a fault of a candidate. The checks at save map it to the codes of 4.37.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultCode {
    /// A loose form (7.18, item 10).
    BadFormat,
    /// A refused type, variable, shape, pair or dialect (7.18, items 11 and 13 to 16).
    NotPermitted,
    /// A name that is not a variable of the document (7.18, item 12).
    BadReference,
}

/// One fault of the candidates of a stored text (7.6a, 7.18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamFault {
    /// The code.
    pub code: FaultCode,
    /// The offset of the `{`.
    pub offset: usize,
    /// The English message.
    pub message: String,
}

/// The result of [`check_stored`]: the faults in the order of the text, and the pairs of the strict forms.
#[derive(Debug, Clone, Default)]
pub struct StoredParams {
    /// The faults, in the order of the text.
    pub faults: Vec<ParamFault>,
    /// The name and the type of each strict form, with the first type of a name.
    pub pairs: BTreeMap<String, ParamType>,
}

/// Checks the candidates of a stored text with the rules of 7.6a (7.18, items 10 to 16).
///
/// `shape_of` gives the shape of a variable of the document, or `None` for a name that is not a variable.
/// `text_names` are the names of the placeholders of 7.6 in the same text ([`text_form_names`]). The check stops
/// after `max_faults` faults, so a text with many faults makes a bounded list: the run reads the first fault only,
/// and the save keeps the faults of the lowest positions (4.35). The pairs are complete only with no fault.
pub fn check_stored(
    dialect: Dialect,
    found: &[Candidate<'_>],
    shape_of: impl Fn(&str) -> Option<ParamShape>,
    text_names: &HashSet<&str>,
    max_faults: usize,
) -> StoredParams {
    let mut out = StoredParams::default();
    for c in found {
        if out.faults.len() >= max_faults {
            break;
        }
        if let Err((code, message)) = check_one(dialect, c, &shape_of, text_names, &mut out.pairs) {
            out.faults.push(ParamFault { code, offset: c.start(), message });
        }
    }
    out
}

/// Checks one candidate of a stored text (7.18, items 10 to 16). A strict form with no fault adds its pair.
fn check_one(
    dialect: Dialect,
    c: &Candidate<'_>,
    shape_of: &impl Fn(&str) -> Option<ParamShape>,
    text_names: &HashSet<&str>,
    pairs: &mut BTreeMap<String, ParamType>,
) -> Result<(), (FaultCode, String)> {
    let not_permitted = |message: String| Err((FaultCode::NotPermitted, message));
    if dialect != Dialect::ClickHouse {
        return not_permitted("a query parameter {name:Type} is permitted only on a ClickHouse source".to_owned());
    }
    let (name, ty) = match *c {
        Candidate::Loose { .. } => {
            let message = "a { followed by a name and : is a query parameter of ClickHouse: write exactly \
                           {name:Type}, with no blank, comment or quoted name";
            return Err((FaultCode::BadFormat, message.to_owned()));
        }
        Candidate::OtherType { name, ty, .. } => {
            return not_permitted(format!(
                "the type {} of the query parameter {} is not permitted: use String, Int64, UInt64, Float64, Date, \
                 Array(String), Array(Int64) or Array(UInt64)",
                crate::report::shown(ty),
                crate::report::shown(name)
            ));
        }
        Candidate::Strict { name, ty, .. } => (name, ty),
    };
    if name.starts_with("__") {
        return not_permitted(format!("the query parameter {name} names a macro, not a variable"));
    }
    let Some(shape) = shape_of(name) else {
        let message = format!("the query parameter {name} is not a variable of the document");
        return Err((FaultCode::BadReference, message));
    };
    match shape {
        ParamShape::Refused(why) => {
            return not_permitted(format!("the variable {name} cannot be a query parameter: {why}"));
        }
        ParamShape::Scalar if ty.is_array() => {
            return not_permitted(format!("the variable {name} gives one value: use a type that is not an Array type"));
        }
        ParamShape::List if !ty.is_array() => {
            return not_permitted(format!("the variable {name} can give a list: use an Array type"));
        }
        ParamShape::Scalar | ParamShape::List => {}
    }
    if text_names.contains(name) {
        return not_permitted(format!(
            "the variable {name} is a query parameter and a placeholder in the same text: use one form"
        ));
    }
    match pairs.get(name) {
        Some(first) if *first != ty => {
            not_permitted(format!("the query parameter {name} has two types: {first} and {ty}"))
        }
        Some(_) => Ok(()),
        None if pairs.len() >= MAX_QUERY_PARAMS => {
            not_permitted(format!("a text has {MAX_QUERY_PARAMS} query parameters at most: {LONG_LISTS}"))
        }
        None => {
            pairs.insert(name.to_owned(), ty);
            Ok(())
        }
    }
}

/// Checks the candidates of the final text (7.6a, run time, step 3). Each candidate MUST be a strict form whose
/// name and type are a pair of the stored text. It gives the pairs of the final text, or `None` when a candidate
/// breaks the rule: author text in code can make a new form, for example `{x:Identifier}`.
pub fn final_pairs(
    found: &[Candidate<'_>],
    stored: &BTreeMap<String, ParamType>,
) -> Option<BTreeMap<String, ParamType>> {
    let mut out = BTreeMap::new();
    for c in found {
        match *c {
            Candidate::Strict { name, ty, .. } if stored.get(name) == Some(&ty) => {
                out.insert(name.to_owned(), ty);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The query parameters of one final text (7.6a): the name and the text of each value, in the byte order of the
/// names. The text is the escaped text of ClickHouse, before the percent encoding of 8.3.
///
/// The `Debug` form shows the names and the sizes only, never a value (12.15).
#[derive(Clone, PartialEq, Eq, Default)]
pub struct QueryParams {
    pairs: Vec<(String, String)>,
}

impl fmt::Debug for QueryParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.pairs.iter().map(|(n, v)| format!("{n}: {} bytes", v.len()))).finish()
    }
}

impl QueryParams {
    /// Tells if the text has no query parameter.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Gives the count of the parameters.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Gives the name and the escaped text of each parameter, in the byte order of the names.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }

    /// Gives the text that the client adds to the request target after the parameters of 8.3:
    /// `&param_<name>=<value>` for each parameter, with the value percent-encoded by the rule of 8.3.
    pub fn query_string(&self) -> String {
        let mut out = String::new();
        for (name, text) in &self.pairs {
            push_pair(&mut out, name, text);
        }
        out
    }
}

/// Adds `&param_<name>=<value>` to `out`, with the value percent-encoded: each byte outside `A-Z a-z 0-9 - . _ ~`
/// becomes `%XX` (8.3).
fn push_pair(out: &mut String, name: &str, text: &str) {
    out.push_str("&param_");
    out.push_str(name);
    out.push('=');
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
        }
    }
}

/// Writes the escapes of ClickHouse that a value needs: `\` becomes `\\`, and a TAB, a line feed, a carriage return
/// and a zero byte get their escape sequence. With `quote`, `'` becomes `\'` too. A value of 7.7 has no control
/// character, so the escapes of the control characters are a second guard only.
fn escape_into(value: &str, quote: bool, out: &mut String) {
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' if quote => out.push_str("\\'"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
}

/// Makes the escaped text of ClickHouse for the values of one parameter (7.6a). The values match their type.
///
/// A scalar `String` is the value with its escapes. A number or a date is the value as it is. An Array is `[`, the
/// items joined by `,`, and `]`. A `String` item is in `'...'`, with `\` and `'` escaped.
pub fn escaped_text(ty: ParamType, values: &[&str]) -> String {
    let mut out = String::new();
    if !ty.is_array() {
        match values.first() {
            Some(v) if ty == ParamType::String => escape_into(v, false, &mut out),
            Some(v) => out.push_str(v),
            None => {}
        }
        return out;
    }
    out.push('[');
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if ty == ParamType::ArrayString {
            out.push('\'');
            escape_into(v, true, &mut out);
            out.push('\'');
        } else {
            out.push_str(v);
        }
    }
    out.push(']');
    out
}

fn refused(variable: &str, reason: String) -> ValueRefused {
    ValueRefused { variable: variable.to_owned(), reason }
}

/// The reason of a refusal of the cap [`MAX_PARAM_BYTES`].
fn too_many_bytes() -> String {
    format!("The query parameters take {MAX_PARAM_BYTES} bytes at most in the request: {LONG_LISTS}")
}

/// The error of [`bind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    /// The request has no member for the variable: `400 bad_request` (7.7).
    Missing(String),
    /// A value breaks the grammar of its type, or a cap: `422 variable_value_refused`.
    Refused(ValueRefused),
}

/// Checks the values of the parameters of the final text and makes their texts (7.6a, run time, steps 4 and 5).
///
/// The names come in byte order. A scalar type takes exactly one value. An Array type takes 0 to
/// [`MAX_PARAM_ITEMS`] values. Each value matches the grammar of its type. The final text has
/// [`MAX_QUERY_PARAMS`] names at most, and the pairs take [`MAX_PARAM_BYTES`] at most after the percent
/// encoding. A reason names the type and never holds the value.
pub fn bind(pairs: &BTreeMap<String, ParamType>, values: &ResolvedValues) -> Result<QueryParams, BindError> {
    let mut out = QueryParams::default();
    let mut encoded = String::new();
    for (count, (name, &ty)) in pairs.iter().enumerate() {
        if count >= MAX_QUERY_PARAMS {
            let reason = format!("A query takes {MAX_QUERY_PARAMS} query parameters at most: {LONG_LISTS}");
            return Err(BindError::Refused(refused(name, reason)));
        }
        let r = values.get(name).ok_or_else(|| BindError::Missing(name.clone()))?;
        let items: Vec<&str> = match &r.selection {
            Selection::Single { value, .. } => vec![value.as_str()],
            Selection::Empty => Vec::new(),
            Selection::List { values, .. } => values.iter().map(String::as_str).collect(),
            Selection::AllValue(_) => {
                let reason = "A variable with an all_value cannot be a query parameter.".to_owned();
                return Err(BindError::Refused(refused(name, reason)));
            }
        };
        if !ty.is_array() && items.len() != 1 {
            let reason = format!("The type {ty} takes exactly one value.");
            return Err(BindError::Refused(refused(name, reason)));
        }
        if items.len() > MAX_PARAM_ITEMS {
            let reason = format!("The type {ty} takes {MAX_PARAM_ITEMS} values at most: {LONG_LISTS}");
            return Err(BindError::Refused(refused(name, reason)));
        }
        if items.iter().any(|v| !ty.accepts(v)) {
            let what = if ty.is_array() { "An item" } else { "The value" };
            let reason = format!("{what} is not a valid {}.", ty.item());
            return Err(BindError::Refused(refused(name, reason)));
        }
        // The pair takes at least `&param_`, the name, `=` and each byte of each value, because an escape and the
        // percent encoding only add bytes. So a running total above the cap refuses the pair before its escaped text
        // and its encoding take memory (review 3, finding 3).
        let least = items.iter().fold(encoded.len() + "&param_=".len() + name.len(), |n, v| n.saturating_add(v.len()));
        if least > MAX_PARAM_BYTES {
            return Err(BindError::Refused(refused(name, too_many_bytes())));
        }
        let text = escaped_text(ty, &items);
        push_pair(&mut encoded, name, &text);
        if encoded.len() > MAX_PARAM_BYTES {
            return Err(BindError::Refused(refused(name, too_many_bytes())));
        }
        out.pairs.push((name.clone(), text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::lexer::lex_all_for;

    fn found(sql: &str) -> Vec<Candidate<'_>> {
        candidates(sql, &lex_all_for(Dialect::ClickHouse, sql))
    }

    #[test]
    fn the_type_list_is_closed_and_exact() {
        for t in ParamType::ALL {
            assert_eq!(ParamType::parse(t.name()), Some(t));
        }
        for bad in ["string", "STRING", "Int", "Int32", "Identifier", "DateTime", "Nullable(String)", "Array(Date)", ""]
        {
            assert_eq!(ParamType::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn integer_grammar_and_ranges() {
        let i = |v: &str| ParamType::Int64.accepts(v);
        let u = |v: &str| ParamType::UInt64.accepts(v);
        for ok in ["0", "-0", "7", "-7", "-9223372036854775808", "9223372036854775807"] {
            assert!(i(ok), "{ok}");
        }
        for bad in [
            "",
            "-",
            "+1",
            "1.0",
            "1e3",
            " 1",
            "1 ",
            "0x1F",
            "9223372036854775808",
            "-9223372036854775809",
            "\u{661}",
            "00",
            "007",
            "-01",
            "--1",
        ] {
            assert!(!i(bad), "{bad:?}");
        }
        for ok in ["0", "1", "18446744073709551615"] {
            assert!(u(ok), "{ok}");
        }
        for bad in [
            "-0",
            "-1",
            "18446744073709551616",
            "1.5",
            "",
            "99999999999999999999999",
            "00",
            "01",
            "00018446744073709551615",
        ] {
            assert!(!u(bad), "{bad:?}");
        }
        // A long run of digits is refused without a big integer: the length decides first.
        assert!(!i(&"9".repeat(1024)));
        assert!(!i(&format!("{}1", "0".repeat(1023))));
    }

    #[test]
    fn float_and_date_grammar() {
        let f = |v: &str| ParamType::Float64.accepts(v);
        for ok in ["0", "-1.5", "12.50", "007.0"] {
            assert!(f(ok), "{ok}");
        }
        for bad in ["1e3", "inf", "NaN", ".5", "5.", "+1", &"9".repeat(400), "0x10", ""] {
            assert!(!f(bad), "{bad:?}");
        }
        let d = |v: &str| ParamType::Date.accepts(v);
        for ok in ["1970-01-01", "2149-06-06", "2000-02-29", "2024-02-29", "2026-11-22"] {
            assert!(d(ok), "{ok}");
        }
        for bad in [
            "1969-12-31",
            "2149-06-07",
            "1900-02-29",
            "2023-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-04-31",
            "2026-1-01",
            "2026-01-01 ",
            "2026/01/01",
            "20260101",
            "",
            "２０２６-01-01",
        ] {
            assert!(!d(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_strict_form_and_each_loose_form() {
        assert!(matches!(
            found("SELECT {x:String}")[..],
            [Candidate::Strict { name: "x", ty: ParamType::String, start: 7, end: 17 }]
        ));
        assert!(matches!(
            found("SELECT {_a1:Array(UInt64)}")[..],
            [Candidate::Strict { name: "_a1", ty: ParamType::ArrayUInt64, .. }]
        ));
        for loose in [
            "SELECT { x:String}",
            "SELECT {x :String}",
            "SELECT {x: String}",
            "SELECT {x:String }",
            "SELECT {x:Array( String)}",
            "SELECT {/* c */x:String}",
            "SELECT {x/* c */:String}",
            "SELECT {x:/* c */String}",
            "SELECT {-- c\nx:String}",
            "SELECT {x # c\n:String}",
            "SELECT {`x`:String}",
            "SELECT {\"x\":String}",
            "SELECT {`x`:Identifier}",
            "SELECT {\u{a0}x:String}",
            "SELECT {x\u{2003}:String}",
            "SELECT {x$y:String}",
            "SELECT {$x:String}",
            "SELECT {é:String}",
            "SELECT {x:String",
            "SELECT {x:}",
            "SELECT {x:Decimal(10, 2)}",
            "SELECT {x:'String'}",
        ] {
            assert!(matches!(found(loose)[..], [Candidate::Loose { start: 7 }]), "{loose:?}: {:?}", found(loose));
        }
        for other in
            ["SELECT {x:Identifier}", "SELECT {x:string}", "SELECT {x:Nullable(String)}", "SELECT {x:DateTime64(3)}"]
        {
            assert!(matches!(found(other)[..], [Candidate::OtherType { name: "x", start: 7, .. }]), "{other:?}");
        }
    }

    #[test]
    fn literals_comments_heredocs_and_placeholders_stay_text() {
        for text in [
            "SELECT '{x:String}'",
            "SELECT `{x:String}`",
            "SELECT \"{x:String}\"",
            "SELECT 1 -- {x:String}",
            "SELECT 1 /* {x:String} */",
            "SELECT $h${x:String}$h$",
            "SELECT $${x:String}$$",
            "SELECT ${x:csv}",
            "SELECT ${x:String}",
            "SELECT a${x:String}",
            "SELECT {'a': 1}",
            "SELECT {1: 2}",
            "SELECT format('{} {}', a, b)",
            "SELECT map('a', 1)",
            "SELECT {x.y:String}",
            // A number is not a name for ClickHouse: `{1x:String}` is no query parameter.
            "SELECT {1x:String}",
        ] {
            assert!(found(text).is_empty(), "{text:?}: {:?}", found(text));
        }
    }

    #[test]
    fn the_final_scan_also_reads_a_brace_after_a_dollar() {
        // ClickHouse 26.8 reads `a${t:Identifier}` as the name `a$` with the alias `{t:Identifier}`, so the scan of
        // the final text takes this `{` when `t` has a pair. The stored text has no such `{`: there it starts a
        // placeholder of 7.6.
        fn fin(sql: &str) -> Vec<Candidate<'_>> {
            let stored = BTreeMap::from([("t".to_owned(), ParamType::String)]);
            final_candidates(sql, &lex_all_for(Dialect::ClickHouse, sql), &stored)
        }
        assert!(found("SELECT a${t:Identifier}").is_empty());
        assert!(matches!(
            fin("SELECT a${t:Identifier}")[..],
            [Candidate::OtherType { name: "t", ty: "Identifier", start: 9 }]
        ));
        assert!(matches!(fin("SELECT ${t:String}")[..], [Candidate::Strict { name: "t", start: 8, .. }]));
        assert!(matches!(fin("SELECT a$ {t:String}")[..], [Candidate::Strict { name: "t", start: 10, .. }]));
        assert!(matches!(fin("SELECT a${ t:String}")[..], [Candidate::Loose { start: 9 }]));
        // A name with no pair gets no value, so its form after `$` stays text: `${other:csv}` of a name that is not a
        // variable stays text in the expansion (7.6). The other rules are the same in both scans.
        for text in [
            "SELECT 'a${t:String}'",
            "SELECT 1 -- a${t:String}",
            "SELECT $h$a${t:String}$h$",
            "SELECT ${1:2}",
            "SELECT ${other:csv}",
            "SELECT a${other:Identifier}",
            "SELECT a${`t`:String}",
        ] {
            assert!(fin(text).is_empty(), "{text:?}: {:?}", fin(text));
        }
    }

    #[test]
    fn the_scan_is_linear() {
        // The minimum of 7 runs after 2 warm-ups (the policy of the timing tests), with a generous limit for a debug
        // build. A scan that went back over the trivia of each `{` would take seconds here.
        fn min_of_7(f: impl Fn() -> usize) -> (std::time::Duration, usize) {
            let mut best = std::time::Duration::MAX;
            let mut last = f();
            for i in 0..9 {
                let t = std::time::Instant::now();
                last = f();
                if i >= 2 {
                    best = best.min(t.elapsed());
                }
            }
            (best, last)
        }
        let limit = std::time::Duration::from_millis(500);
        let trivia = format!("SELECT {}", "{/* c */ ".repeat(50_000));
        let tokens = lex_all_for(Dialect::ClickHouse, &trivia);
        let stored = BTreeMap::new();
        let (t, n) =
            min_of_7(|| candidates(&trivia, &tokens).len() + final_candidates(&trivia, &tokens, &stored).len());
        assert!(n == 0 && t < limit, "{n} candidates in {t:?}");
        let forms = format!("SELECT {}", "{x:String}+".repeat(50_000));
        let tokens = lex_all_for(Dialect::ClickHouse, &forms);
        let (t, n) = min_of_7(|| candidates(&forms, &tokens).len());
        assert!(n == 50_000 && t < limit, "{n} candidates in {t:?}");
    }

    #[test]
    fn the_check_of_a_stored_text_stops_at_its_fault_cap() {
        let text = format!("SELECT {}", "{ x:String}+".repeat(10_000));
        let found = found(&text);
        assert_eq!(found.len(), 10_000);
        let checked = check_stored(Dialect::ClickHouse, &found, |_| Some(ParamShape::Scalar), &HashSet::new(), 201);
        assert_eq!(checked.faults.len(), 201);
        assert!(checked.faults.windows(2).all(|w| w[0].offset < w[1].offset), "the faults of the lowest positions");
        let one = check_stored(Dialect::ClickHouse, &found, |_| Some(ParamShape::Scalar), &HashSet::new(), 1);
        assert_eq!((one.faults.len(), one.faults[0].offset, one.faults[0].code), (1, 7, FaultCode::BadFormat));
    }

    #[test]
    fn the_escaped_text_of_each_type() {
        assert_eq!(escaped_text(ParamType::String, &["a\\b'c"]), "a\\\\b'c");
        assert_eq!(escaped_text(ParamType::String, &["t\tn\nr\r"]), "t\\tn\\nr\\r");
        assert_eq!(escaped_text(ParamType::Int64, &["-5"]), "-5");
        assert_eq!(escaped_text(ParamType::ArrayString, &["a'b", "c\\d", "], ["]), "['a\\'b','c\\\\d','], [']");
        assert_eq!(escaped_text(ParamType::ArrayUInt64, &["1", "2"]), "[1,2]");
        assert_eq!(escaped_text(ParamType::ArrayInt64, &[]), "[]");
    }

    #[test]
    fn the_pairs_are_percent_encoded_with_the_rule_of_8_3() {
        let p = QueryParams { pairs: vec![("a".into(), "x y&z=é~".into()), ("b".into(), "[1,2]".into())] };
        assert_eq!(p.query_string(), "&param_a=x%20y%26z%3D%C3%A9~&param_b=%5B1%2C2%5D");
        // The Debug form holds no value.
        assert_eq!(format!("{p:?}"), r#"["a: 9 bytes", "b: 5 bytes"]"#);
    }
}
