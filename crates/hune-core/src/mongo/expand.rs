// SPDX-License-Identifier: MIT OR Apache-2.0
//! The expansion of a MongoDB query and the property PM (7.34, 7.35).
//!
//! The engine never writes a value in the text of the query. It parses the text one time (7.28). It
//! checks the grammar of the stored tree T0. Then it replaces each string leaf that is a macro or a
//! placeholder by a typed value, and so it makes the tree T1. A free value stays inside its leaf:
//!
//! - PM1: T1 and T0 have the same keys, stage names, operator names and structure. The only change is a
//!   leaf that becomes a typed value. The array of a format `json`, `json_number` or `json_oid`
//!   replaces one leaf.
//! - PM2: a string leaf that holds a free value does not start with `$`.
//! - PM3: a free value is never in a name place. The check of the grammar refuses the place.
//! - PM4: a value of a format that gives a number, a date, an ObjectId or a boolean follows the grammar of
//!   its format.
//!
//! The steps of 7.35 map to functions: [`parse`] (steps 1 and 2), [`expand`] and [`prepare`] (steps 3 to 7).
//! [`prepare`] runs all of them on the SQL lines. A fault of the text, of the grammar or of the lists gives
//! `422 sql_invalid` ([`MongoExpandError::Invalid`]). A value that breaks PM or its format gives
//! `422 variable_value_refused`. A variable that the request does not give gives `400 bad_request`.

use std::fmt;

use crate::mongo::grammar::{
    Check, Class, EmbFormat, Fault, Item, Macro, Part, Phase, ResolvedNames, WholeFormat, check_top, classify,
    shape_top,
};
use crate::mongo::json::{JsonError, JsonErrorKind, Seg, Span, parse_text, pointer};
use crate::mongo::limits::{MAX_QUERY_ARRAY_ITEMS, MAX_QUERY_DEPTH, MAX_QUERY_NODES, MAX_QUERY_STRING_BYTES};
use crate::mongo::tree::MongoQuery;
use crate::mongo::value::{Document, MongoValue};
use crate::sql::formats::{self, Subject};
use crate::sql::values::{Resolved, ResolvedValues, Selection, ValueRefused, is_number_text};
use crate::sql::{Interval, SqlUse, Stage, TimeRange};

/// The cause of a `sql_invalid` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidKind {
    /// A fault of the profile J (7.28).
    Json,
    /// A limit of 7.28.
    Limit,
    /// The top object breaks 7.29.
    Top,
    /// A stage, an operator or a shape that 7.30 to 7.32 refuse.
    Grammar,
    /// A macro that is not valid (7.34).
    Macro,
    /// A placeholder in a place or with a format that 7.34 refuses, or a list with no format.
    Placeholder,
    /// A collection or a database that the source does not list (7.33).
    Reference,
    /// The check of the expanded tree failed (step 6 of 7.35).
    Guard,
}

/// The error `422 sql_invalid` of the language (7.28, 7.35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MongoInvalid {
    /// The cause.
    pub kind: InvalidKind,
    /// The English message. It holds no value of the query.
    pub message: String,
    /// The JSON pointer inside the query text, for example `/aggregate/2/$out`. It is empty when the
    /// fault has no place in the tree.
    pub pointer: String,
    /// In the stored text, or in the expanded tree (which has no position).
    pub stage: Stage,
    /// The index of the SQL line, from 1. The member `line` of the API error.
    pub line: Option<u32>,
    /// The column in the line, in characters, from 1.
    pub column: Option<u32>,
}

impl fmt::Display for MongoInvalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pointer.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.pointer, self.message)
        }
    }
}

/// The error of [`expand`] and [`prepare`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MongoExpandError {
    /// `422 sql_invalid`.
    Invalid(MongoInvalid),
    /// `400 bad_request`: the query names a variable of the document, and the request has no member for it.
    MissingVariable(String),
    /// `422 variable_value_refused`.
    ValueRefused(ValueRefused),
    /// `422 sql_too_large`: the BSON size of the query and the limit.
    TooLarge {
        /// The size in bytes.
        size: usize,
        /// `max_query_size` of the source.
        limit: usize,
    },
}

impl MongoExpandError {
    /// Gives the API error code of 6.8.
    pub fn api_code(&self) -> &'static str {
        match self {
            MongoExpandError::Invalid(_) => "sql_invalid",
            MongoExpandError::MissingVariable(_) => "bad_request",
            MongoExpandError::ValueRefused(_) => "variable_value_refused",
            MongoExpandError::TooLarge { .. } => "sql_too_large",
        }
    }
}

impl fmt::Display for MongoExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MongoExpandError::Invalid(e) => e.fmt(f),
            MongoExpandError::MissingVariable(n) => write!(f, "the request has no value for the variable {n}"),
            MongoExpandError::ValueRefused(r) => r.fmt(f),
            MongoExpandError::TooLarge { size, limit } => write!(f, "the query has {size} bytes, the limit is {limit}"),
        }
    }
}

impl std::error::Error for MongoExpandError {}

/// The input of [`expand`] and [`prepare`].
#[derive(Debug, Clone)]
pub struct MongoExpandRequest<'a> {
    /// The variables of the document and the checked values of the request (7.7).
    pub values: &'a ResolvedValues,
    /// The checked time range (7.3).
    pub range: TimeRange,
    /// The interval of 7.4. A variable query has none: `$__interval_ms` and `$__interval_s` then give an error.
    pub interval: Option<Interval>,
    /// `max_query_size` of the source, in bytes.
    pub max_query_size: usize,
    /// The key `collections` of the source (7.33).
    pub collections: &'a [String],
    /// The key `databases` of the source (7.33).
    pub databases: &'a [String],
}

/// Joins the SQL lines of a panel with one LF between two lines (7.28, 7.35 step 1).
pub fn join_lines<S: AsRef<str>>(lines: &[S]) -> String {
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(l.as_ref());
    }
    out
}

/// Gives the line (from 1) and the column in characters (from 1) of a byte offset.
pub(crate) fn line_column(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let (mut line, mut column) = (1u32, 1u32);
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
    (line, column)
}

fn invalid_kind(item: Item) -> InvalidKind {
    match item {
        Item::L1 => InvalidKind::Limit,
        Item::L2 => InvalidKind::Top,
        Item::L3 | Item::L4 => InvalidKind::Grammar,
        Item::L5 | Item::L9 => InvalidKind::Macro,
        Item::L6 | Item::L7 => InvalidKind::Placeholder,
        Item::L8 => InvalidKind::Reference,
    }
}

/// Where a fault is in the stored text.
struct Locator<'a> {
    span: &'a Span,
    text: &'a str,
}

fn invalid_from_fault(f: &Fault, top: &MongoValue, loc: Option<&Locator<'_>>, stage: Stage) -> MongoInvalid {
    let (line, column) = match loc {
        Some(l) => {
            let (a, b) = line_column(l.text, l.span.locate(top, &f.path, f.at_key));
            (Some(a), Some(b))
        }
        None => (None, None),
    };
    MongoInvalid { kind: invalid_kind(f.item), message: f.message.clone(), pointer: f.pointer(), stage, line, column }
}

fn json_invalid(text: &str, e: &JsonError) -> MongoInvalid {
    let (line, column) = line_column(text, e.offset);
    let kind = match e.kind {
        JsonErrorKind::BadFormat => InvalidKind::Json,
        JsonErrorKind::TooDeep | JsonErrorKind::TooLarge => InvalidKind::Limit,
    };
    MongoInvalid {
        kind,
        message: e.message.clone(),
        pointer: String::new(),
        stage: Stage::Stored,
        line: Some(line),
        column: Some(column),
    }
}

/// The result of [`parse_lines`]: the query and the data that a later check needs to name positions.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedQuery {
    /// The stored query T0.
    pub query: MongoQuery,
    /// The top object, in the form of the text.
    pub top: MongoValue,
    /// The positions in the text.
    pub span: Span,
    /// The joined text.
    pub text: String,
}

/// Steps 1 and 2 of 7.35 with the shape of the top object (7.29): joins the lines, parses the text with the
/// profile J and makes the [`MongoQuery`]. It does not check the grammar below the members and the lists.
///
/// # Errors
///
/// A fault of the profile J, a limit of 7.28 or a top object that breaks 7.29 gives [`MongoInvalid`].
pub fn parse_lines<S: AsRef<str>>(lines: &[S]) -> Result<ParsedQuery, MongoInvalid> {
    let text = join_lines(lines);
    let parsed = parse_text(&text).map_err(|e| json_invalid(&text, &e))?;
    match shape_top(&parsed.value) {
        Ok(query) => Ok(ParsedQuery { query, top: parsed.value, span: parsed.span, text }),
        Err(faults) => {
            let loc = Locator { span: &parsed.span, text: &text };
            Err(invalid_from_fault(&faults[0], &parsed.value, Some(&loc), Stage::Stored))
        }
    }
}

/// Parses the SQL lines of a panel into the stored query T0. See [`parse_lines`].
///
/// # Errors
///
/// See [`parse_lines`].
pub fn parse<S: AsRef<str>>(lines: &[S]) -> Result<MongoQuery, MongoInvalid> {
    parse_lines(lines).map(|p| p.query)
}

/// Steps 1 to 7 of 7.35 on the SQL lines of a panel: parse, check the grammar of T0, expand, check PM,
/// check the grammar of T1 and check the size. It gives the expanded query T1.
///
/// # Errors
///
/// See [`MongoExpandError`].
pub fn prepare<S: AsRef<str>>(lines: &[S], req: &MongoExpandRequest<'_>) -> Result<MongoQuery, MongoExpandError> {
    let text = join_lines(lines);
    let parsed = parse_text(&text).map_err(|e| MongoExpandError::Invalid(json_invalid(&text, &e)))?;
    let loc = Locator { span: &parsed.span, text: &text };
    run(&parsed.value, Some(&loc), req)
}

/// Steps 3 to 7 of 7.35 on a stored query T0. The error has no line and no column, because the text is
/// not known. It gives the expanded query T1.
///
/// # Errors
///
/// See [`MongoExpandError`].
pub fn expand(query: &MongoQuery, req: &MongoExpandRequest<'_>) -> Result<MongoQuery, MongoExpandError> {
    run(&query.to_value(), None, req)
}

/// Gives the database of a query: its member `database`, or the first name of the key `databases` (7.29).
pub fn effective_database<'a>(query: &'a MongoQuery, databases: &'a [String]) -> Option<&'a str> {
    query.database.as_deref().or_else(|| databases.first().map(String::as_str))
}

fn run(
    top: &MongoValue,
    loc: Option<&Locator<'_>>,
    req: &MongoExpandRequest<'_>,
) -> Result<MongoQuery, MongoExpandError> {
    let invalid = |f: &Fault| MongoExpandError::Invalid(invalid_from_fault(f, top, loc, Stage::Stored));
    // A tree from the parser holds these limits. A tree that another caller builds can be deeper.
    for (too_big, message) in [
        (top.depth() > MAX_QUERY_DEPTH, "the depth of objects and arrays is above 32"),
        (top.node_count() > MAX_QUERY_NODES, "the tree has more than 100000 values"),
    ] {
        if too_big {
            return Err(MongoExpandError::Invalid(MongoInvalid {
                kind: InvalidKind::Limit,
                message: message.to_owned(),
                pointer: String::new(),
                stage: Stage::Stored,
                line: None,
                column: None,
            }));
        }
    }
    if let Err(faults) = shape_top(top) {
        return Err(invalid(&faults[0]));
    }
    let vars = ResolvedNames(req.values);
    let usage = if req.interval.is_some() { SqlUse::Panel } else { SqlUse::Variable };
    let check =
        Check { phase: Phase::Stored, vars: &vars, collections: req.collections, databases: req.databases, usage };
    let report = check_top(top, &check);
    if let Some(f) = report.faults.first() {
        return Err(invalid(f));
    }
    // Step 4 and 5: the expansion and the property PM for each inserted value.
    let base = bson_size(top);
    let mut ex = Ex { req, path: Vec::new(), inserted: Vec::new(), nodes: 0, added: 0, base };
    let t1 = ex.walk(top)?;
    let guard = |message: String, pointer: String| {
        MongoExpandError::Invalid(MongoInvalid {
            kind: InvalidKind::Guard,
            message,
            pointer,
            stage: Stage::Expanded,
            line: None,
            column: None,
        })
    };
    // Step 6: the check of the expanded tree. PM1 first.
    if !same_structure(top, &t1) {
        return Err(guard("the expansion changed the structure of the query".to_owned(), String::new()));
    }
    if t1.depth() > MAX_QUERY_DEPTH {
        return Err(guard("the depth of objects and arrays is above 32".to_owned(), String::new()));
    }
    let check1 = Check {
        phase: Phase::Expanded,
        vars: &crate::mongo::grammar::NoVariables,
        collections: req.collections,
        databases: req.databases,
        usage: SqlUse::Panel,
    };
    if let Some(f) = check_top(&t1, &check1).faults.first() {
        // A fault at an inserted leaf is a fault of the value, and not of the document.
        if let Some((_, variable)) = ex.inserted.iter().find(|(p, _)| f.path.starts_with(p)) {
            return Err(MongoExpandError::ValueRefused(ValueRefused {
                variable: variable.clone(),
                reason: format!("The value does not fit its place: {}.", f.message),
            }));
        }
        return Err(guard(f.message.clone(), f.pointer()));
    }
    let query = shape_top(&t1).map_err(|faults| guard(faults[0].message.clone(), faults[0].pointer()))?;
    // Step 7: the size.
    let size = bson_size(&query.to_value());
    if size > req.max_query_size {
        return Err(MongoExpandError::TooLarge { size, limit: req.max_query_size });
    }
    Ok(query)
}

/// Checks PM1: the same keys, the same structure, and a change only where a string leaf became a typed value.
fn same_structure(a: &MongoValue, b: &MongoValue) -> bool {
    let mut stack = vec![(a, b)];
    while let Some((x, y)) = stack.pop() {
        match (x, y) {
            (MongoValue::Document(p), MongoValue::Document(q)) => {
                if p.len() != q.len() || p.iter().zip(q).any(|((k1, _), (k2, _))| k1 != k2) {
                    return false;
                }
                stack.extend(p.iter().zip(q).map(|((_, v1), (_, v2))| (v1, v2)));
            }
            (MongoValue::Array(p), MongoValue::Array(q)) => {
                if p.len() != q.len() {
                    return false;
                }
                stack.extend(p.iter().zip(q));
            }
            (MongoValue::String(_), MongoValue::Document(_)) => return false,
            (MongoValue::String(_), _) => {}
            (MongoValue::Document(_) | MongoValue::Array(_), _)
            | (_, MongoValue::Document(_) | MongoValue::Array(_)) => {
                return false;
            }
            (p, q) => {
                if p != q {
                    return false;
                }
            }
        }
    }
    true
}

// ---- The BSON size ----

/// Gives the size in bytes of the BSON encoding of a value, without its element header. A document or an
/// array has its length prefix and its end byte. An array key is the decimal index.
pub fn bson_size(v: &MongoValue) -> usize {
    match v {
        MongoValue::Null | MongoValue::Other(_) => 0,
        MongoValue::Bool(_) => 1,
        MongoValue::Int32(_) => 4,
        MongoValue::Int64(_) | MongoValue::Double(_) | MongoValue::DateMs(_) => 8,
        MongoValue::Decimal128(_) => 16,
        MongoValue::ObjectId(_) => 12,
        MongoValue::Uuid(_) => 4 + 1 + 16,
        MongoValue::String(s) => 4 + s.len() + 1,
        MongoValue::Document(d) => 4 + d.iter().map(|(k, v)| 1 + k.len() + 1 + bson_size(v)).sum::<usize>() + 1,
        MongoValue::Array(a) => {
            4 + a.iter().enumerate().map(|(i, v)| 1 + decimal_len(i) + 1 + bson_size(v)).sum::<usize>() + 1
        }
    }
}

fn decimal_len(mut n: usize) -> usize {
    let mut len = 1;
    while n >= 10 {
        n /= 10;
        len += 1;
    }
    len
}

// ---- The expansion ----

#[derive(Clone, Copy)]
enum Pr<'a> {
    K(&'a str),
    I(usize),
}

struct Ex<'a> {
    req: &'a MongoExpandRequest<'a>,
    path: Vec<Pr<'a>>,
    /// The path and the first variable of each leaf that the expansion replaced.
    inserted: Vec<(Vec<Seg>, String)>,
    nodes: usize,
    /// The bytes that insertions add. With `base`, a bound of the size of the tree.
    added: usize,
    base: usize,
}

fn refused(variable: &str, reason: &str) -> MongoExpandError {
    MongoExpandError::ValueRefused(ValueRefused { variable: variable.to_owned(), reason: reason.to_owned() })
}

fn pm_fail(variable: &str) -> MongoExpandError {
    refused(variable, "The value changes the structure of the query.")
}

fn placeholder_error(message: &str, pointer: String) -> MongoExpandError {
    MongoExpandError::Invalid(MongoInvalid {
        kind: InvalidKind::Placeholder,
        message: message.to_owned(),
        pointer,
        stage: Stage::Stored,
        line: None,
        column: None,
    })
}

impl<'a> Ex<'a> {
    fn owned_path(&self) -> Vec<Seg> {
        self.path
            .iter()
            .map(|p| match p {
                Pr::K(k) => Seg::Key((*k).to_owned()),
                Pr::I(i) => Seg::Index(*i),
            })
            .collect()
    }

    fn walk(&mut self, v: &'a MongoValue) -> Result<MongoValue, MongoExpandError> {
        self.nodes += 1;
        Ok(match v {
            MongoValue::Document(d) => {
                let mut out: Document = Vec::with_capacity(d.len());
                for (k, val) in d {
                    self.path.push(Pr::K(k));
                    let nv = self.walk(val)?;
                    self.path.pop();
                    out.push((k.clone(), nv));
                }
                MongoValue::Document(out)
            }
            MongoValue::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, val) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    let nv = self.walk(val)?;
                    self.path.pop();
                    out.push(nv);
                }
                MongoValue::Array(out)
            }
            MongoValue::String(s) => self.string(s)?,
            other => other.clone(),
        })
    }

    /// Counts the values and the bytes that an insertion adds, and checks the bounds.
    fn account(&mut self, variable: &str, value: &MongoValue) -> Result<(), MongoExpandError> {
        let extra = value.node_count().saturating_sub(1);
        self.nodes += extra;
        if self.nodes > MAX_QUERY_NODES {
            return Err(refused(variable, "The values make the query have more than 100000 values."));
        }
        self.added = self.added.saturating_add(bson_size(value));
        let size = self.base.saturating_add(self.added);
        if size > self.req.max_query_size {
            return Err(MongoExpandError::TooLarge { size, limit: self.req.max_query_size });
        }
        Ok(())
    }

    fn record(&mut self, variable: &str) {
        let p = self.owned_path();
        self.inserted.push((p, variable.to_owned()));
    }

    fn resolved(&self, name: &str) -> Result<&'a Resolved, MongoExpandError> {
        self.req.values.get(name).ok_or_else(|| MongoExpandError::MissingVariable(name.to_owned()))
    }

    fn string(&mut self, s: &'a str) -> Result<MongoValue, MongoExpandError> {
        let vars = ResolvedNames(self.req.values);
        let mut warns = Vec::new();
        match classify(s, &vars, true, &mut warns) {
            Class::Text | Class::Path | Class::VarRef => Ok(MongoValue::String(s.to_owned())),
            Class::Macro(m) => self.macro_value(m),
            Class::Whole { name, format } => {
                let v = self.whole(name, format)?;
                self.record(name);
                self.account(name, &v)?;
                Ok(v)
            }
            Class::Embedded(parts) => {
                let v = self.embedded(&parts)?;
                if let Some(Part::Var { name, .. }) = parts.iter().find(|p| matches!(p, Part::Var { .. })) {
                    self.record(name);
                    self.account(name, &v)?;
                }
                Ok(v)
            }
            Class::Bad(b) => Err(placeholder_error(&b.message, pointer(&self.owned_path()))),
        }
    }

    fn macro_value(&self, m: Macro) -> Result<MongoValue, MongoExpandError> {
        let interval = |m: Macro| {
            self.req.interval.ok_or_else(|| {
                placeholder_error(
                    &format!("{} is not permitted in the query of a variable", m.name()),
                    pointer(&self.owned_path()),
                )
            })
        };
        Ok(match m {
            Macro::TimeFrom => MongoValue::DateMs(self.req.range.from_ms()),
            Macro::TimeTo => MongoValue::DateMs(self.req.range.to_ms()),
            Macro::From => MongoValue::Int64(self.req.range.from_ms()),
            Macro::To => MongoValue::Int64(self.req.range.to_ms()),
            Macro::IntervalMs => MongoValue::Int64(i64::try_from(interval(m)?.ms).unwrap_or(i64::MAX)),
            Macro::IntervalS => MongoValue::Int64(i64::try_from(interval(m)?.seconds).unwrap_or(i64::MAX)),
        })
    }

    /// The one value of a selection: a value, the `all_value`, or the empty text. A list is an error.
    fn one(&self, name: &str, r: &'a Resolved) -> Result<(&'a str, bool), MongoExpandError> {
        match &r.selection {
            Selection::Single { value, author } => Ok((value.as_str(), *author)),
            Selection::AllValue(t) => Ok((t.as_str(), true)),
            Selection::Empty => Ok(("", false)),
            Selection::List { .. } => Err(placeholder_error(
                &format!("the variable {name} gives a list: use the format json, json_number or json_oid"),
                pointer(&self.owned_path()),
            )),
        }
    }

    /// The values of a selection, as a list.
    fn items(r: &'a Resolved) -> (Vec<&'a str>, bool) {
        match &r.selection {
            Selection::Single { value, author } => (vec![value.as_str()], *author),
            Selection::AllValue(t) => (vec![t.as_str()], true),
            Selection::Empty => (Vec::new(), false),
            Selection::List { values, author } => (values.iter().map(String::as_str).collect(), *author),
        }
    }

    fn whole(&self, name: &str, format: WholeFormat) -> Result<MongoValue, MongoExpandError> {
        let r = self.resolved(name)?;
        match format {
            WholeFormat::Text => {
                let (text, author) = self.one(name, r)?;
                text_leaf(name, text, author)
            }
            WholeFormat::Number => {
                let (text, _) = self.one(name, r)?;
                parse_number(text).ok_or_else(|| refused(name, "The value is not a number."))
            }
            WholeFormat::Date => {
                let (text, _) = self.one(name, r)?;
                parse_date(text).ok_or_else(|| refused(name, "The value is not a date."))
            }
            WholeFormat::Oid => {
                let (text, _) = self.one(name, r)?;
                MongoValue::object_id_from_hex(text).ok_or_else(|| refused(name, "The value is not an ObjectId."))
            }
            WholeFormat::Bool => match self.one(name, r)?.0 {
                "true" => Ok(MongoValue::Bool(true)),
                "false" => Ok(MongoValue::Bool(false)),
                _ => Err(refused(name, "The value is not true or false.")),
            },
            WholeFormat::Json => {
                let (items, author) = Self::items(r);
                let out: Result<Vec<MongoValue>, MongoExpandError> =
                    items.into_iter().map(|t| text_leaf(name, t, author)).collect();
                list_leaf(name, out?)
            }
            WholeFormat::JsonNumber => {
                let (items, _) = Self::items(r);
                let out: Result<Vec<MongoValue>, MongoExpandError> = items
                    .into_iter()
                    .map(|t| parse_number(t).ok_or_else(|| refused(name, "A value is not a number.")))
                    .collect();
                list_leaf(name, out?)
            }
            WholeFormat::JsonOid => {
                let (items, _) = Self::items(r);
                let out: Result<Vec<MongoValue>, MongoExpandError> = items
                    .into_iter()
                    .map(|t| {
                        MongoValue::object_id_from_hex(t).ok_or_else(|| refused(name, "A value is not an ObjectId."))
                    })
                    .collect();
                list_leaf(name, out?)
            }
        }
    }

    fn embedded(&self, parts: &[Part<'_>]) -> Result<MongoValue, MongoExpandError> {
        let mut out = String::new();
        let mut free = false;
        let mut first_var = "";
        for part in parts {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Var { name, format, .. } => {
                    if first_var.is_empty() {
                        first_var = name;
                    }
                    let r = self.resolved(name)?;
                    let (text, is_free) = self.render(name, *format, r)?;
                    free |= is_free;
                    out.push_str(&text);
                }
            }
            if out.len() > MAX_QUERY_STRING_BYTES {
                return Err(refused(first_var, "The text is longer than 16384 bytes."));
            }
        }
        // PM2. The stored string does not start with `$`, so this holds. The check stays as a guard.
        if free && out.starts_with('$') {
            return Err(pm_fail(first_var));
        }
        Ok(MongoValue::String(out))
    }

    /// Renders one embedded placeholder (7.34). It gives the text and whether the value is free.
    fn render(&self, name: &str, format: EmbFormat, r: &'a Resolved) -> Result<(String, bool), MongoExpandError> {
        let free = match &r.selection {
            Selection::Single { author, .. } | Selection::List { author, .. } => !*author,
            Selection::AllValue(_) | Selection::Empty => false,
        };
        let subject = match &r.selection {
            Selection::Single { value, .. } => Subject::One(value),
            Selection::Empty => Subject::One(""),
            Selection::AllValue(t) => Subject::AllValue(t),
            Selection::List { values, .. } => Subject::List { values, all: r.all },
        };
        let text = match format {
            EmbFormat::None | EmbFormat::Raw | EmbFormat::Csv => match subject {
                Subject::One(v) | Subject::AllValue(v) => v.to_owned(),
                Subject::List { values, .. } if format == EmbFormat::Csv => values.join(","),
                Subject::List { .. } => {
                    return Err(placeholder_error(
                        &format!("the variable {name} gives a list: use the format csv or another format of a list"),
                        pointer(&self.owned_path()),
                    ));
                }
            },
            EmbFormat::Std(f) => formats::render(f, name, subject).unwrap_or_default(),
        };
        Ok((text, free))
    }
}

/// A string leaf from a value. A free value does not start with `$` (PM2).
fn text_leaf(name: &str, text: &str, author: bool) -> Result<MongoValue, MongoExpandError> {
    if !author && text.starts_with('$') {
        return Err(pm_fail(name));
    }
    if text.len() > MAX_QUERY_STRING_BYTES {
        return Err(refused(name, "The text is longer than 16384 bytes."));
    }
    Ok(MongoValue::String(text.to_owned()))
}

fn list_leaf(name: &str, items: Vec<MongoValue>) -> Result<MongoValue, MongoExpandError> {
    if items.len() > MAX_QUERY_ARRAY_ITEMS {
        return Err(refused(name, "The list has more than 10000 items."));
    }
    Ok(MongoValue::Array(items))
}

/// The format `number` (7.34): the grammar `^-?[0-9]+(\.[0-9]+)?$`. An integer in the range of Int64 gives an
/// Int32 or an Int64. Else it is a Double that MUST be finite.
pub fn parse_number(text: &str) -> Option<MongoValue> {
    if !is_number_text(text) {
        return None;
    }
    if !text.contains('.')
        && let Ok(n) = text.parse::<i64>()
    {
        return Some(i32::try_from(n).map_or(MongoValue::Int64(n), MongoValue::Int32));
    }
    match text.parse::<f64>() {
        Ok(f) if f.is_finite() => Some(MongoValue::Double(f)),
        _ => None,
    }
}

/// The format `date` (7.34): `YYYY-MM-DDTHH:MM:SS[.fff]Z`, or 1 to 16 digits of epoch milliseconds.
pub fn parse_date(text: &str) -> Option<MongoValue> {
    if (1..=16).contains(&text.len()) && text.bytes().all(|c| c.is_ascii_digit()) {
        return text.parse::<i64>().ok().map(MongoValue::DateMs);
    }
    crate::mongo::json::parse_iso_date(text).map(MongoValue::DateMs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::{RequestValues, TextValueType, VariableDef, check_time_range, compute_interval, resolve_values};

    fn req_with<'a>(
        values: &'a ResolvedValues,
        cols: &'a [String],
        dbs: &'a [String],
        variable: bool,
    ) -> MongoExpandRequest<'a> {
        let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
        let interval = if variable { None } else { Some(compute_interval(&range, 1200, 1).unwrap()) };
        MongoExpandRequest { values, range, interval, max_query_size: 262_144, collections: cols, databases: dbs }
    }

    fn run_text(text: &str, defs: &[VariableDef], req: &RequestValues) -> Result<MongoQuery, MongoExpandError> {
        let values = resolve_values(defs, req).unwrap();
        let cols = vec!["c".to_owned()];
        let dbs = vec!["db".to_owned()];
        prepare(&[text], &req_with(&values, &cols, &dbs, false))
    }

    #[test]
    fn macros_become_typed_leaves() {
        let q = run_text(
            r#"{"collection":"c","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom","$lte":"${__timeTo}"},"f":"$__from"}}]}"#,
            &[],
            &RequestValues::new(),
        )
        .unwrap();
        let v = q.to_value();
        let m = v.get("aggregate").unwrap();
        let MongoValue::Array(stages) = m else { panic!() };
        let ts = stages[0].get("$match").unwrap().get("ts").unwrap();
        assert_eq!(ts.get("$gte"), Some(&MongoValue::DateMs(1_794_211_200_000)));
        assert_eq!(ts.get("$lte"), Some(&MongoValue::DateMs(1_794_232_800_000)));
        assert_eq!(stages[0].get("$match").unwrap().get("f"), Some(&MongoValue::Int64(1_794_211_200_000)));
    }

    #[test]
    fn hostile_value_stays_in_its_leaf() {
        let defs = [VariableDef::text("host", TextValueType::Text)];
        let mut r = RequestValues::new();
        r.insert("host", false, &["$secret"]);
        let e = run_text(r#"{"collection":"c","aggregate":[{"$match":{"h":"$host"}}]}"#, &defs, &r).unwrap_err();
        match e {
            MongoExpandError::ValueRefused(v) => {
                assert_eq!(v.variable, "host");
                assert_eq!(v.reason, "The value changes the structure of the query.");
            }
            other => panic!("{other:?}"),
        }
        let mut r = RequestValues::new();
        r.insert("host", false, &[r#"{"$out":"x"}"#]);
        let q = run_text(r#"{"collection":"c","aggregate":[{"$match":{"h":"$host"}}]}"#, &defs, &r).unwrap();
        assert_eq!(
            q.to_value().get("aggregate"),
            Some(&MongoValue::Array(vec![MongoValue::Document(vec![(
                "$match".into(),
                MongoValue::Document(vec![("h".into(), MongoValue::String(r#"{"$out":"x"}"#.into()))])
            )])]))
        );
    }

    #[test]
    fn bson_size_is_exact_for_a_small_document() {
        // {"a": 1} is 12 bytes in BSON.
        assert_eq!(bson_size(&MongoValue::Document(vec![("a".into(), MongoValue::Int32(1))])), 12);
        // {"a": [ "x" ]}: the array {"0": "x"} is 4 + (1 + 1 + 1 + 6) + 1 = 14, and the document is 4 + 1 + 2 + 14 + 1 = 22.
        let v = MongoValue::Document(vec![("a".into(), MongoValue::Array(vec![MongoValue::String("x".into())]))]);
        assert_eq!(bson_size(&v), 22);
    }

    #[test]
    fn number_and_date_formats() {
        assert_eq!(parse_number("12"), Some(MongoValue::Int32(12)));
        assert_eq!(parse_number("-0"), Some(MongoValue::Int32(0)));
        assert_eq!(parse_number("3000000000"), Some(MongoValue::Int64(3_000_000_000)));
        assert_eq!(parse_number("12.5"), Some(MongoValue::Double(12.5)));
        assert!(matches!(parse_number("99999999999999999999"), Some(MongoValue::Double(_))));
        assert_eq!(parse_number("12abc"), None);
        assert_eq!(parse_number(&format!("1{}", "0".repeat(400))), None);
        assert_eq!(parse_date("1794211200125"), Some(MongoValue::DateMs(1_794_211_200_125)));
        assert_eq!(parse_date("2026-11-09T14:00:00Z"), Some(MongoValue::DateMs(1_794_232_800_000)));
        assert_eq!(parse_date("2026-11-09 14:00:00"), None);
    }
}
