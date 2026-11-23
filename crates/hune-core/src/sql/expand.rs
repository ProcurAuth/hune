// SPDX-License-Identifier: MIT OR Apache-2.0
//! The expansion (7.11) in one pass: macros, placeholders and the escape rules. Then the checks
//! of the property P, of the statement and of the size (steps 9 to 12 of 7.2).

use std::fmt;

use super::adhoc::{self, AdhocRun};
use super::dialect::Dialect;
use super::escape::{
    escape_name_content, escape_pg_estring_content, escape_pg_name_content, escape_pg_string_content,
    escape_string_content, quote_pg_string, quote_sqlstring, quote_string,
};
use super::formats::{self, Subject};
use super::interval::{GroupIntervalFault, Interval, TimeRange, group_seconds};
use super::lexer::{Position, Token, TokenKind, heredoc_tag_length, lex_for};
use super::macros::{self, AnyMacro, Macro, PgMacro, parse_call, parse_pg_call};
use super::params::{self, BindError, ParamType, QueryParams};
use super::placeholders::{self, Format, Found};
use super::rules::{Place, context_fault, interval_argument_shape};
use super::statement::check_statement_for;
use super::values::{Resolved, ResolvedValues, Selection, ValueRefused, VariableClass, is_number_text};

/// The most levels of macros inside the argument of a macro.
const MAX_MACRO_DEPTH: usize = 16;

/// The input of [`expand`].
#[derive(Debug, Clone)]
pub struct ExpandRequest<'a> {
    /// The stored SQL text: the lines joined with LF (7.2, step 8).
    pub sql: &'a str,
    /// The variables of the document and the checked values of the request (7.7).
    pub values: &'a ResolvedValues,
    /// The checked time range (7.3).
    pub range: TimeRange,
    /// The interval of 7.4. A variable query has none: `$__timeInterval` and `$__interval_s`
    /// then give a document error.
    pub interval: Option<Interval>,
    /// `max_query_size` of the source, in bytes (7.13).
    pub max_query_size: usize,
}

/// The cause of a `sql_invalid` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlInvalidKind {
    /// A lexer error (7.5).
    Lexer,
    /// A failure of the statement check (7.12).
    Statement,
    /// A macro that breaks 7.10.
    Macro,
    /// A `${` that does not match the grammar of 7.6.
    Placeholder,
    /// A placeholder in a context that 7.8 marks as a document error.
    Context,
    /// A query parameter that breaks 7.6a: a loose form, a refused type or variable, or a form of the final text that
    /// the stored text has not.
    Parameter,
}

/// Where a `sql_invalid` error is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// In the stored text. `line` and `column` refer to it.
    Stored,
    /// In the final text. The viewer does not see it, so the error has no position.
    Expanded,
}

/// The error `422 sql_invalid` (7.5, 7.12, 7.18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlInvalid {
    /// The cause.
    pub kind: SqlInvalidKind,
    /// The English message.
    pub message: String,
    /// The text that has the fault.
    pub stage: Stage,
    /// The line in the stored text, from 1. The member `line` of the API error.
    pub line: Option<u32>,
    /// The column in characters in the stored text, from 1. The member `column` of the API error.
    pub column: Option<u32>,
}

/// The error of [`expand`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// `422 sql_invalid`.
    Invalid(SqlInvalid),
    /// `400 bad_request`: the SQL names a variable of the document, and the request has no member for it.
    MissingVariable(String),
    /// `422 variable_value_refused`.
    ValueRefused(ValueRefused),
    /// `422 sql_too_large`: the size that the text reaches and the limit.
    TooLarge {
        /// The size in bytes. When the expansion stops early, it is the size that the text reached.
        size: usize,
        /// `max_query_size`.
        limit: usize,
    },
}

impl ExpandError {
    /// Gives the API error code of 6.8.
    pub fn api_code(&self) -> &'static str {
        match self {
            ExpandError::Invalid(_) => "sql_invalid",
            ExpandError::MissingVariable(_) => "bad_request",
            ExpandError::ValueRefused(_) => "variable_value_refused",
            ExpandError::TooLarge { .. } => "sql_too_large",
        }
    }

    /// Tells if the error is a fault of the stored document: a macro, a placeholder, a context, or a query parameter
    /// of the stored text (7.6a). The checks at save (7.18) also find these faults.
    pub fn is_document_fault(&self) -> bool {
        matches!(
            self,
            ExpandError::Invalid(SqlInvalid {
                kind: SqlInvalidKind::Macro | SqlInvalidKind::Placeholder | SqlInvalidKind::Context,
                ..
            }) | ExpandError::Invalid(SqlInvalid { kind: SqlInvalidKind::Parameter, stage: Stage::Stored, .. })
        )
    }
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::Invalid(e) => f.write_str(&e.message),
            ExpandError::MissingVariable(n) => write!(f, "the request has no value for the variable {n}"),
            ExpandError::ValueRefused(r) => r.fmt(f),
            ExpandError::TooLarge { size, limit } => write!(f, "the SQL has {size} bytes, the limit is {limit}"),
        }
    }
}

impl std::error::Error for ExpandError {}

/// Joins the SQL lines of a panel with one LF between two lines (7.2, step 8).
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

/// Tells if the text of a quoted-name token is a `U&"..."` name of the dialect `postgres` (7.20).
fn is_unicode_name(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 3 && b[0].eq_ignore_ascii_case(&b'u') && b[1] == b'&' && b[2] == b'"'
}

/// Gives the length of the tag of a dollar quote (`$$` or `$tag$`) in the token `t`. The tag has no `$`
/// inside (7.20), so it ends at the second `$`.
fn dollar_tag_length(src: &str, t: &Token) -> usize {
    let b = src.as_bytes();
    let mut i = t.start + 1;
    while i < t.end && b[i] != b'$' {
        i += 1;
    }
    (i + 1).min(t.end) - t.start
}

/// What an inserted free value must be in the output (the property P of 7.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Inside the string token of this kind that starts and ends at these output offsets.
    InString { tok_start: usize, tok_end: usize, kind: TokenKind },
    /// A sequence of numbers with one `,` between two numbers.
    Numbers,
    /// A sequence of string tokens with one `,` between two strings.
    Strings,
    /// An empty text: a token starts at this offset (or two blanks merge), so the empty text joins no two tokens.
    Junction,
}

#[derive(Debug, Clone)]
struct Insertion {
    start: usize,
    end: usize,
    shape: Shape,
    variable: String,
}

impl Insertion {
    fn shifted(&self, base: usize) -> Insertion {
        let mut this = self.clone();
        this.shift(base);
        this
    }

    fn shift(&mut self, base: usize) {
        self.start += base;
        self.end += base;
        if let Shape::InString { tok_start, tok_end, .. } = &mut self.shape {
            *tok_start += base;
            *tok_end += base;
        }
    }
}

#[derive(Default)]
struct Frag {
    text: String,
    ins: Vec<Insertion>,
}

struct Expander<'a> {
    dialect: Dialect,
    req: &'a ExpandRequest<'a>,
    tokens: &'a [Token],
    /// The filters of the `adhoc` variable of the source of a panel query (7.10a). `None`: the macro
    /// `$__adhocFilters` is a document error (a variable query, an annotation, or no `adhoc` variable on the source).
    adhoc: Option<&'a AdhocRun<'a>>,
    /// The permitted places of `$__adhocFilters` in the stored text, read one time when the first macro needs them.
    adhoc_places: std::cell::OnceCell<adhoc::AdhocPlaces>,
}

fn invalid(kind: SqlInvalidKind, message: String, stage: Stage, at: Option<Position>) -> ExpandError {
    ExpandError::Invalid(SqlInvalid { kind, message, stage, line: at.map(|p| p.line), column: at.map(|p| p.column) })
}

impl Expander<'_> {
    fn src(&self) -> &str {
        self.req.sql
    }

    fn stored(&self, kind: SqlInvalidKind, message: String, at: usize) -> ExpandError {
        invalid(kind, message, Stage::Stored, Some(Position::at(self.src(), at)))
    }

    fn slice(&self, a: usize, b: usize) -> &str {
        self.src().get(a..b).unwrap_or("")
    }

    fn check_size(&self, current: usize, extra: usize) -> Result<(), ExpandError> {
        let size = current.saturating_add(extra);
        if size > self.req.max_query_size {
            return Err(ExpandError::TooLarge { size, limit: self.req.max_query_size });
        }
        Ok(())
    }

    fn push(&self, f: &mut Frag, s: &str) -> Result<(), ExpandError> {
        self.check_size(f.text.len(), s.len())?;
        f.text.push_str(s);
        Ok(())
    }

    fn append(&self, f: &mut Frag, a: &Frag) -> Result<(), ExpandError> {
        self.check_size(f.text.len(), a.text.len())?;
        let base = f.text.len();
        f.text.push_str(&a.text);
        f.ins.extend(a.ins.iter().map(|i| i.shifted(base)));
        Ok(())
    }

    fn expand_tokens(&self, lo: usize, hi: usize, out: &mut Frag, depth: usize) -> Result<(), ExpandError> {
        self.expand_range(lo, hi, out, depth, false)
    }

    /// Expands the tokens `lo..hi`. With `interval_argument`, the text is the second argument of a group
    /// macro (7.23): `$__interval` stays as it is, and no macro is permitted.
    fn expand_range(
        &self,
        lo: usize,
        hi: usize,
        out: &mut Frag,
        depth: usize,
        interval_argument: bool,
    ) -> Result<(), ExpandError> {
        let postgres = self.dialect == Dialect::Postgres;
        let mut i = lo;
        while i < hi {
            let t = self.tokens[i];
            match t.kind {
                k if k.is_code() => {
                    let mut j = i;
                    while j < hi && self.tokens[j].kind.is_code() {
                        j += 1;
                    }
                    i = self.code_run(i, j, hi, out, depth, interval_argument)?;
                }
                TokenKind::String => {
                    self.literal_token(&t, Place::String, out)?;
                    i += 1;
                }
                TokenKind::EString => {
                    self.literal_token(&t, Place::EString, out)?;
                    i += 1;
                }
                TokenKind::QuotedName => {
                    if postgres && is_unicode_name(self.slice(t.start, t.end)) {
                        self.opaque_token(&t, Place::Prefixed)?;
                        self.push(out, self.slice(t.start, t.end))?;
                    } else {
                        self.literal_token(&t, Place::QuotedName, out)?;
                    }
                    i += 1;
                }
                TokenKind::PrefixedString => {
                    self.opaque_token(&t, Place::Prefixed)?;
                    self.push(out, self.slice(t.start, t.end))?;
                    i += 1;
                }
                TokenKind::Heredoc => {
                    let place = if postgres { Place::DollarQuote } else { Place::Heredoc };
                    self.opaque_token(&t, place)?;
                    self.push(out, self.slice(t.start, t.end))?;
                    i += 1;
                }
                _ => {
                    self.push(out, self.slice(t.start, t.end))?;
                    i += 1;
                }
            }
        }
        Ok(())
    }

    /// Expands the code tokens `i0..j`. It gives the index of the token where the caller continues.
    fn code_run(
        &self,
        i0: usize,
        j: usize,
        hi: usize,
        out: &mut Frag,
        depth: usize,
        interval_argument: bool,
    ) -> Result<usize, ExpandError> {
        let src = self.src();
        let run_hi = self.tokens[j - 1].end;
        let mut p = self.tokens[i0].start;
        loop {
            let Some(found) = placeholders::next_for(self.dialect, src, p, run_hi) else {
                self.push(out, self.slice(p, run_hi))?;
                return Ok(j);
            };
            self.push(out, self.slice(p, found.start()))?;
            let ph = match found {
                Found::BadBrace { start, fault, .. } => {
                    let what = match fault {
                        placeholders::BraceFault::Malformed => {
                            "a ${ does not match ${name} or ${name:format}".to_owned()
                        }
                        placeholders::BraceFault::UnknownFormat(f) => {
                            format!("the format {f} is not in the list of formats")
                        }
                    };
                    return Err(self.stored(SqlInvalidKind::Placeholder, what, start));
                }
                Found::Placeholder(ph) => ph,
            };
            p = ph.end;
            if ph.name.starts_with("__") && self.dialect == Dialect::Postgres {
                match self.pg_macro(&ph, hi, out, depth, interval_argument)? {
                    Some(next) => return Ok(next),
                    None => continue,
                }
            }
            if ph.name.starts_with("__") {
                let m = Macro::from_name(ph.name).ok_or_else(|| {
                    self.stored(SqlInvalidKind::Macro, format!("${} is not a macro of Hune", ph.name), ph.start)
                })?;
                if ph.format.is_some() {
                    return Err(self.stored(SqlInvalidKind::Macro, format!("{} takes no format", m.name()), ph.start));
                }
                let interval = match (m.needs_interval(), self.req.interval) {
                    (true, None) => {
                        let msg = format!("{} needs the interval, which a variable query has not", m.name());
                        return Err(self.stored(SqlInvalidKind::Macro, msg, ph.start));
                    }
                    (_, i) => i,
                };
                let call = parse_call(m, src, self.tokens, ph.end, hi)
                    .map_err(|f| self.stored(SqlInvalidKind::Macro, f.message(m), ph.start))?;
                let range = &self.req.range;
                let seconds = interval.map_or(0, |i| i.seconds);
                let interval_ms = interval.map_or(0, |i| i.ms);
                match (m, call) {
                    (Macro::FromTime, _) => self.push(out, &macros::from_time_text(range))?,
                    (Macro::ToTime, _) => self.push(out, &macros::to_time_text(range))?,
                    (Macro::FromTimeMs, _) => self.push(out, &macros::from_time_ms_text(range))?,
                    (Macro::ToTimeMs, _) => self.push(out, &macros::to_time_ms_text(range))?,
                    (Macro::IntervalS, _) => self.push(out, &seconds.to_string())?,
                    (Macro::IntervalMs, _) => self.push(out, &interval_ms.to_string())?,
                    (Macro::From, _) => self.push(out, &range.from_ms().to_string())?,
                    (Macro::To, _) => self.push(out, &range.to_ms().to_string())?,
                    (Macro::AdhocFilters, _) => self.adhoc_filters(&ph, depth, out)?,
                    (_, Some(c)) => {
                        if depth >= MAX_MACRO_DEPTH {
                            let msg = format!("macros nest more than {MAX_MACRO_DEPTH} levels");
                            return Err(self.stored(SqlInvalidKind::Macro, msg, ph.start));
                        }
                        let arg_of = |lo: usize, hi: usize| -> Result<Frag, ExpandError> {
                            let mut arg = Frag::default();
                            self.expand_tokens(lo, hi, &mut arg, depth + 1)?;
                            Ok(arg)
                        };
                        match m {
                            Macro::ConditionalAll => {
                                let (lo, hi) = c.second.unwrap_or((c.arg_end, c.arg_end));
                                let r = self.conditional_variable(lo, hi, ph.start)?;
                                if r.all {
                                    self.push(out, "1=1")?;
                                } else {
                                    self.append(out, &arg_of(c.arg_start, c.arg_end)?)?;
                                }
                            }
                            Macro::TimeFilter | Macro::TimeFilterMs | Macro::DateFilter => {
                                let arg = arg_of(c.arg_start, c.arg_end)?;
                                let (mid, tail) = match m {
                                    Macro::TimeFilter => (
                                        format!(" >= toDateTime({}) AND ", range.from_seconds()),
                                        format!(" <= toDateTime({})", range.to_seconds()),
                                    ),
                                    Macro::TimeFilterMs => (
                                        format!(" >= fromUnixTimestamp64Milli({}) AND ", range.from_ms()),
                                        format!(" <= fromUnixTimestamp64Milli({})", range.to_ms()),
                                    ),
                                    _ => (
                                        format!(" >= toDate('{}') AND ", macros::utc_date(range.from_ms())),
                                        format!(" <= toDate('{}')", macros::utc_date(range.to_ms())),
                                    ),
                                };
                                self.append(out, &arg)?;
                                self.push(out, &mid)?;
                                self.append(out, &arg)?;
                                self.push(out, &tail)?;
                            }
                            Macro::DateTimeFilter | Macro::Dt => {
                                let (lo, hi) = c.second.unwrap_or((c.arg_end, c.arg_end));
                                let date = arg_of(c.arg_start, c.arg_end)?;
                                let time = arg_of(lo, hi)?;
                                self.push(out, "(")?;
                                self.append(out, &date)?;
                                self.push(out, &format!(" >= toDate('{}') AND ", macros::utc_date(range.from_ms())))?;
                                self.append(out, &date)?;
                                self.push(out, &format!(" <= toDate('{}')) AND (", macros::utc_date(range.to_ms())))?;
                                self.append(out, &time)?;
                                self.push(out, &format!(" >= toDateTime({}) AND ", range.from_seconds()))?;
                                self.append(out, &time)?;
                                self.push(out, &format!(" <= toDateTime({}))", range.to_seconds()))?;
                            }
                            Macro::TimeIntervalMs => {
                                self.push(out, "toStartOfInterval(toDateTime64(")?;
                                self.append(out, &arg_of(c.arg_start, c.arg_end)?)?;
                                self.push(out, &format!(", 3), INTERVAL {interval_ms} millisecond)"))?;
                            }
                            _ => {
                                self.push(out, "toStartOfInterval(toDateTime(")?;
                                self.append(out, &arg_of(c.arg_start, c.arg_end)?)?;
                                self.push(out, &format!("), INTERVAL {seconds} second)"))?;
                            }
                        }
                        return Ok(c.close + 1);
                    }
                    (_, None) => {}
                }
                continue;
            }
            if !self.req.values.is_declared(ph.name) {
                self.push(out, self.slice(ph.start, ph.end))?;
                continue;
            }
            let resolved =
                self.req.values.get(ph.name).ok_or_else(|| ExpandError::MissingVariable(ph.name.to_owned()))?;
            if let Some(fault) = context_fault(Place::Code, false, false, ph.format) {
                return Err(self.stored(SqlInvalidKind::Context, fault.message(ph.name), ph.start));
            }
            let (text, shape) = render_code(self.dialect, ph.name, resolved, ph.format)?;
            let start = out.text.len();
            self.push(out, &text)?;
            if let Some(shape) = shape {
                out.ins.push(Insertion { start, end: out.text.len(), shape, variable: ph.name.to_owned() });
            }
        }
    }

    /// Expands `$__adhocFilters` (7.10a). The macro needs the filters of an `adhoc` variable on the source of the query,
    /// a place outside another macro, and a place of [`adhoc::AdhocPlaces`] (one term of the top-level conjunction of
    /// a `WHERE` or `PREWHERE` clause). The text is `1=1` or the filters in parentheses. Each value literal is a free
    /// range of the code context: the property P checks it as one string token (7.11).
    fn adhoc_filters(
        &self,
        ph: &placeholders::Placeholder<'_>,
        depth: usize,
        out: &mut Frag,
    ) -> Result<(), ExpandError> {
        let fault = |m: &str| self.stored(SqlInvalidKind::Macro, m.to_owned(), ph.start);
        let Some(run) = self.adhoc else {
            return Err(fault(
                "$__adhocFilters needs an adhoc variable whose source is the source of the query (7.10a)",
            ));
        };
        if depth > 0 {
            return Err(fault("$__adhocFilters is not permitted inside another macro (7.10a)"));
        }
        let places = self.adhoc_places.get_or_init(|| adhoc::AdhocPlaces::of(self.src(), self.tokens));
        if !places.permits(ph.start, ph.end) {
            return Err(fault(adhoc::PLACE_MESSAGE));
        }
        let (text, ranges) = run.render();
        let base = out.text.len();
        self.push(out, &text)?;
        out.ins.extend(ranges.into_iter().map(|(start, end)| Insertion {
            start: base + start,
            end: base + end,
            shape: Shape::Strings,
            variable: run.variable().to_owned(),
        }));
        Ok(())
    }

    /// Expands a string or a quoted name. The quote marks stay.
    fn literal_token(&self, t: &Token, place: Place, out: &mut Frag) -> Result<(), ExpandError> {
        let src = self.src();
        let (content_lo, content_hi) =
            t.content_range(src).unwrap_or((t.start + 1, t.end.saturating_sub(1).max(t.start + 1)));
        let quote = self.slice(t.start, t.start + 1).chars().next().unwrap_or('\'');
        let tok_start = out.text.len();
        self.push(out, self.slice(t.start, content_lo))?;
        let mut local = Vec::new();
        let mut p = content_lo;
        loop {
            let Some(found) = placeholders::next_for(self.dialect, src, p, content_hi) else {
                self.push(out, self.slice(p, content_hi))?;
                break;
            };
            self.push(out, self.slice(p, found.start()))?;
            p = found.end();
            let ph = match found {
                Found::BadBrace { start, end, .. } => {
                    self.push(out, self.slice(start, end))?;
                    continue;
                }
                Found::Placeholder(ph) => ph,
            };
            if ph.name.starts_with("__") || !self.req.values.is_declared(ph.name) {
                self.push(out, self.slice(ph.start, ph.end))?;
                continue;
            }
            let r = self.req.values.get(ph.name).ok_or_else(|| ExpandError::MissingVariable(ph.name.to_owned()))?;
            let is_list = matches!(r.selection, Selection::List { .. });
            // An open `custom` variable has the rows of a `query` variable (7.8): the member decides, not the kind.
            // An `interval` and a `constant` variable have the rows of a closed `custom` variable (7.8).
            let is_custom =
                matches!(r.class, VariableClass::Custom | VariableClass::Interval | VariableClass::Constant) && !r.open;
            if let Some(fault) = context_fault(place, is_custom, is_list, ph.format) {
                return Err(self.stored(SqlInvalidKind::Context, fault.message(ph.name), ph.start));
            }
            let (text, free) = render_literal(self.dialect, ph.name, r, ph.format, place, quote);
            let start = out.text.len();
            self.push(out, &text)?;
            if free {
                local.push((start, out.text.len(), ph.name));
            }
        }
        self.push(out, self.slice(content_hi, t.end))?;
        let tok_end = out.text.len();
        if matches!(place, Place::String | Place::EString) {
            out.ins.extend(local.into_iter().map(|(start, end, name)| Insertion {
                start,
                end,
                shape: Shape::InString { tok_start, tok_end, kind: t.kind },
                variable: name.to_owned(),
            }));
        }
        Ok(())
    }

    /// Expands the macro of a placeholder that starts with `__` in the dialect `postgres` (7.22, 7.23).
    /// It gives the index of the token where the caller continues after a call. `None` means that the
    /// caller continues after the name.
    fn pg_macro(
        &self,
        ph: &placeholders::Placeholder<'_>,
        hi: usize,
        out: &mut Frag,
        depth: usize,
        interval_argument: bool,
    ) -> Result<Option<usize>, ExpandError> {
        let fault = |kind: SqlInvalidKind, message: String| self.stored(kind, message, ph.start);
        if ph.name == "__interval" {
            if interval_argument && ph.format.is_none() {
                self.push(out, self.slice(ph.start, ph.end))?;
                return Ok(None);
            }
            let msg = "$__interval is valid only as the interval argument of a group macro (7.23)".to_owned();
            return Err(fault(SqlInvalidKind::Macro, msg));
        }
        if interval_argument {
            let msg = format!("${} is not permitted in the interval argument of a group macro (7.23)", ph.name);
            return Err(fault(SqlInvalidKind::Macro, msg));
        }
        if ph.name == adhoc::MACRO_NAME {
            let msg = "$__adhocFilters is permitted only on a clickhouse source (7.10a)".to_owned();
            return Err(fault(SqlInvalidKind::Macro, msg));
        }
        let Some(AnyMacro::Postgres(m)) = macros::lookup(self.dialect, ph.name) else {
            return Err(fault(SqlInvalidKind::Macro, format!("${} is not a macro of Hune", ph.name)));
        };
        if ph.format.is_some() {
            return Err(fault(SqlInvalidKind::Macro, format!("{} takes no format", m.name())));
        }
        if m.needs_interval() && self.req.interval.is_none() {
            let msg = format!("{} needs the interval, which a variable query has not", m.name());
            return Err(fault(SqlInvalidKind::Macro, msg));
        }
        let call = parse_pg_call(m, self.src(), self.tokens, ph.end, hi)
            .map_err(|f| fault(SqlInvalidKind::Macro, f.message_for(m.name(), m.arity())))?;
        let range = &self.req.range;
        let Some(c) = call else {
            let text = m
                .expand_value(range, self.req.interval)
                .ok_or_else(|| fault(SqlInvalidKind::Macro, format!("{} needs an argument", m.name())))?;
            self.push(out, &text)?;
            return Ok(None);
        };
        if !m.takes_argument() {
            let text = m
                .expand_value(range, self.req.interval)
                .ok_or_else(|| fault(SqlInvalidKind::Macro, format!("{} needs an argument", m.name())))?;
            self.push(out, &text)?;
            return Ok(Some(c.close + 1));
        }
        if depth >= MAX_MACRO_DEPTH {
            let msg = format!("macros nest more than {MAX_MACRO_DEPTH} levels");
            return Err(fault(SqlInvalidKind::Macro, msg));
        }
        let arg_of = |lo: usize, hi: usize| -> Result<Frag, ExpandError> {
            let mut arg = Frag::default();
            self.expand_tokens(lo, hi, &mut arg, depth + 1)?;
            Ok(arg)
        };
        if m == PgMacro::ConditionalAll {
            let (lo, hi) = c.second.unwrap_or((c.arg_end, c.arg_end));
            let r = self.conditional_variable(lo, hi, ph.start)?;
            if r.all {
                self.push(out, "1=1")?;
            } else {
                self.append(out, &arg_of(c.arg_start, c.arg_end)?)?;
            }
            return Ok(Some(c.close + 1));
        }
        let group = if m.is_group() {
            let (lo, hi) = c.second.unwrap_or((c.arg_end, c.arg_end));
            Some(self.group_text(lo, hi, ph.start, depth)?)
        } else {
            None
        };
        // The macro writes its first argument where the marker is. A marker is a control byte that no
        // template of a macro holds, so the text splits at it without a doubt.
        const MARK: &str = "\u{1}";
        let template = m
            .expand_call(range, MARK, group.as_deref())
            .ok_or_else(|| fault(SqlInvalidKind::Macro, format!("{} needs an argument", m.name())))?;
        let arg = arg_of(c.arg_start, c.arg_end)?;
        let mut parts = template.split(MARK);
        if let Some(first) = parts.next() {
            self.push(out, first)?;
        }
        for part in parts {
            self.append(out, &arg)?;
            self.push(out, part)?;
        }
        Ok(Some(c.close + 1))
    }

    /// Reads the second argument of a group macro and gives `G` (7.23). The argument is never copied to
    /// the output.
    fn group_text(&self, lo: usize, hi: usize, macro_at: usize, depth: usize) -> Result<String, ExpandError> {
        let stored = |message: String| self.stored(SqlInvalidKind::Macro, message, macro_at);
        interval_argument_shape(self.src(), self.tokens, lo, hi).map_err(|e| stored(e.message))?;
        let (start, end) = (self.tokens[lo].start, self.tokens[hi - 1].end);
        let mut arg = Frag::default();
        self.expand_range(lo, hi, &mut arg, depth + 1, true)?;
        match group_seconds(&arg.text, self.req.interval.map(|i| i.ms)) {
            Ok(g) => Ok(g),
            Err(e) => {
                // A fault in a text that a variable gave is a refused value. Another fault is a fault of the SQL.
                let variable = self.first_variable(start, end);
                match (variable, e.fault) {
                    (
                        Some(name),
                        GroupIntervalFault::Grammar | GroupIntervalFault::Shape | GroupIntervalFault::Range,
                    ) => Err(ExpandError::ValueRefused(ValueRefused {
                        variable: name,
                        reason: "The value is not an interval of 7.23".to_owned(),
                    })),
                    _ => Err(stored(e.message)),
                }
            }
        }
    }

    /// Gives the name of the first declared variable that the stored text `start..end` names.
    fn first_variable(&self, start: usize, end: usize) -> Option<String> {
        let mut p = start;
        while let Some(found) = placeholders::next_for(self.dialect, self.src(), p, end) {
            p = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.name.starts_with("__")
                && self.req.values.is_declared(ph.name)
            {
                return Some(ph.name.to_owned());
            }
        }
        None
    }

    /// Reads the second argument of `$__conditionalAll` (7.10). It MUST be exactly one placeholder without a
    /// format, and it MUST name a `query` or a `custom` variable of the document. It gives the value
    /// of the variable. A variable that the request does not hold gives `bad_request`.
    fn conditional_variable(&self, lo: usize, hi: usize, macro_at: usize) -> Result<&Resolved, ExpandError> {
        let src = self.src();
        let (start, end) = (self.tokens[lo].start, self.tokens[hi - 1].end);
        let fault = |m: &str| self.stored(SqlInvalidKind::Macro, format!("$__conditionalAll: {m}"), macro_at);
        match placeholders::next_for(self.dialect, src, start, end) {
            Some(Found::Placeholder(ph)) if ph.start == start && ph.end == end && !ph.name.starts_with("__") => {
                if ph.format.is_some() {
                    return Err(fault("the second argument has no format"));
                }
                if !self.req.values.is_declared(ph.name) {
                    return Err(fault("the second argument is not a variable of the document"));
                }
                let r = self.req.values.get(ph.name).ok_or_else(|| ExpandError::MissingVariable(ph.name.to_owned()))?;
                match r.class {
                    VariableClass::Query | VariableClass::Custom => Ok(r),
                    VariableClass::Text | VariableClass::Interval | VariableClass::Constant => {
                        Err(fault("the second argument must be a query variable or a custom variable"))
                    }
                }
            }
            _ => Err(fault("the second argument must be exactly one placeholder: $name or ${name}")),
        }
    }

    /// A placeholder of a declared variable in a heredoc, a dollar quote, a prefixed string or a `U&` name
    /// is a document error (7.8, 7.21).
    fn opaque_token(&self, t: &Token, place: Place) -> Result<(), ExpandError> {
        let src = self.src();
        let (lo, hi) = match place {
            Place::Prefixed => t.content_range(src).unwrap_or((t.start, t.end)),
            Place::DollarQuote => {
                let tag = dollar_tag_length(src, t);
                (t.start + tag, t.end.saturating_sub(tag).max(t.start + tag))
            }
            _ => {
                let tag = heredoc_tag_length(src.as_bytes(), t.start);
                (t.start + tag, t.end.saturating_sub(tag))
            }
        };
        let mut p = lo;
        while p < hi {
            let Some(found) = placeholders::next_for(self.dialect, src, p, hi) else { break };
            p = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.name.starts_with("__")
                && self.req.values.is_declared(ph.name)
            {
                let fault = context_fault(place, false, false, ph.format);
                let msg = fault.map(|f| f.message(ph.name)).unwrap_or_default();
                return Err(self.stored(SqlInvalidKind::Context, msg, ph.start));
            }
        }
        Ok(())
    }
}

/// Renders the value of a placeholder in code (7.8). It gives the text and the shape that P checks
/// for a free value.
fn render_code(
    dialect: Dialect,
    name: &str,
    r: &Resolved,
    format: Option<Format>,
) -> Result<(String, Option<Shape>), ExpandError> {
    let not_number = || {
        ExpandError::ValueRefused(ValueRefused {
            variable: name.to_owned(),
            reason: "Outside a string, only a number is permitted.".to_owned(),
        })
    };
    let literal = |values: &[String], sql_string: bool, free: bool| {
        // The dialect `postgres` writes `singlequote` and `sqlstring` the same way: `'` doubled, a backslash kept (7.21).
        let quoted: Vec<String> = values
            .iter()
            .map(|v| match (dialect, sql_string) {
                (Dialect::Postgres, _) => quote_pg_string(v),
                (_, true) => quote_sqlstring(v),
                (_, false) => quote_string(v),
            })
            .collect();
        let text = if quoted.is_empty() { "''".to_owned() } else { quoted.join(",") };
        (text, free.then_some(Shape::Strings))
    };
    // Defence in depth: the context check refuses these formats before this point. A free text never reaches code.
    if format.is_some_and(Format::literal_only) {
        return Err(ExpandError::ValueRefused(ValueRefused {
            variable: name.to_owned(),
            reason: "This format is permitted only inside a string.".to_owned(),
        }));
    }
    let sql_string = format == Some(Format::SqlString);
    let quoted_format = matches!(format, Some(Format::SingleQuote | Format::SqlString));
    match &r.selection {
        Selection::AllValue(t) => Ok((t.clone(), None)),
        Selection::Empty => {
            if matches!(format, Some(Format::Raw | Format::Csv)) {
                Ok((String::new(), Some(Shape::Junction)))
            } else {
                Ok(literal(&[], sql_string, true))
            }
        }
        Selection::Single { value, author } => {
            if quoted_format {
                Ok(literal(std::slice::from_ref(value), sql_string, !author))
            } else if *author {
                Ok((value.clone(), None))
            } else if is_number_text(value) {
                Ok((value.clone(), Some(Shape::Numbers)))
            } else {
                Err(not_number())
            }
        }
        Selection::List { values, author } => match format {
            None | Some(Format::SingleQuote | Format::SqlString) => Ok(literal(values, sql_string, !author)),
            Some(_) => {
                if !author && values.iter().any(|v| !is_number_text(v)) {
                    return Err(not_number());
                }
                // An empty list comes from the choice of the viewer, also for author text: it must join no tokens.
                let shape = match (values.is_empty(), author) {
                    (true, _) => Some(Shape::Junction),
                    (false, false) => Some(Shape::Numbers),
                    (false, true) => None,
                };
                Ok((values.join(","), shape))
            }
        },
    }
}

/// Renders the value of a placeholder in a string or a quoted name (7.8). The context check has
/// passed. It gives the text and tells if the text holds a free value.
///
/// A format that is valid only inside a string (7.9, D7) makes its text first. Then the string escape
/// applies to the whole text, so the text stays inside the literal.
fn render_literal(
    dialect: Dialect,
    name: &str,
    r: &Resolved,
    format: Option<Format>,
    place: Place,
    quote: char,
) -> (String, bool) {
    let escape = |s: &str| {
        let mut out = String::with_capacity(s.len());
        match (dialect, place) {
            (Dialect::Postgres, Place::QuotedName) => escape_pg_name_content(s, &mut out),
            (Dialect::Postgres, Place::EString) => escape_pg_estring_content(s, &mut out),
            (Dialect::Postgres, _) => escape_pg_string_content(s, &mut out),
            (_, Place::QuotedName) => escape_name_content(s, quote, &mut out),
            _ => escape_string_content(s, &mut out),
        }
        out
    };
    let sel = &r.selection;
    let formatted = format.and_then(|f| {
        let subject = match sel {
            Selection::AllValue(t) => Subject::AllValue(t),
            Selection::Single { value, .. } => Subject::One(value),
            Selection::Empty => Subject::One(""),
            Selection::List { values, .. } => Subject::List { values, all: r.all },
        };
        formats::render(f, name, subject)
    });
    let text = |plain: String| formatted.clone().map_or_else(|| escape(&plain), |t| escape(&t));
    match sel {
        Selection::AllValue(t) => (text(t.clone()), false),
        Selection::Empty => (text(String::new()), true),
        Selection::Single { value, author } => (text(value.clone()), !author),
        // An empty list comes from the choice of the viewer: P checks it, also for author text.
        Selection::List { values, author } => (text(values.join(",")), !author || values.is_empty()),
    }
}

/// Checks the property P for one inserted range on the tokens of the output.
fn property_holds(dialect: Dialect, tokens: &[Token], text: &str, ins: &Insertion) -> bool {
    let is_minus = |t: &Token| match dialect {
        Dialect::ClickHouse => t.kind == TokenKind::Punct && text.as_bytes()[t.start] == b'-',
        // The sign is an operator token whose text is exactly `-` (7.24).
        Dialect::Postgres => t.kind == TokenKind::Operator && t.end == t.start + 1 && text.as_bytes()[t.start] == b'-',
    };
    let at = |start: usize| {
        let i = tokens.partition_point(|t| t.start < start);
        (i < tokens.len() && tokens[i].start == start).then_some(i)
    };
    match ins.shape {
        Shape::InString { tok_start, tok_end, kind } => at(tok_start).is_some_and(|i| {
            let t = &tokens[i];
            t.kind == kind && t.closed && t.end == tok_end && ins.start > tok_start && ins.end < tok_end
        }),
        Shape::Junction => {
            // A token starts here, or the text ends here, or two blanks merge into one blank token.
            ins.start == text.len()
                || at(ins.start).is_some()
                || tokens.iter().any(|t| t.kind == TokenKind::Blank && t.start < ins.start && ins.start < t.end)
        }
        Shape::Numbers | Shape::Strings => {
            let numbers = ins.shape == Shape::Numbers;
            let Some(mut i) = at(ins.start) else { return false };
            loop {
                if numbers && tokens.get(i).is_some_and(is_minus) {
                    i += 1;
                }
                let want = if numbers { TokenKind::Number } else { TokenKind::String };
                match tokens.get(i) {
                    Some(t) if t.kind == want && t.closed => i += 1,
                    _ => return false,
                }
                let end = tokens[i - 1].end;
                if end == ins.end {
                    return true;
                }
                if end > ins.end {
                    return false;
                }
                match tokens.get(i) {
                    Some(t) if t.kind == TokenKind::Punct && text.as_bytes()[t.start] == b',' => i += 1,
                    _ => return false,
                }
            }
        }
    }
}

/// Expands the stored SQL text (7.2, steps 9 to 12). The result is the final SQL text.
///
/// The function reads the text with the lexer and replaces the macros and the placeholders. It
/// reads the result again and checks the property P for each free value. Then it checks the
/// statement (7.12) and the size (7.13).
pub fn expand(req: &ExpandRequest<'_>) -> Result<String, ExpandError> {
    expand_for(Dialect::ClickHouse, req)
}

/// Expands the stored SQL text of a source of the given dialect (7.2, steps 9 to 12).
///
/// For `clickhouse` it is [`expand`]. For `postgres` it uses the lexer of 7.20, the macros of 7.22 and
/// 7.23, the escape rules of 7.21, the property P of 7.24 and the statement check of 7.25.
///
/// It gives the SQL text only. A text with query parameters (7.6a) also needs their values in the request:
/// the server calls [`expand_prepared`].
pub fn expand_for(dialect: Dialect, req: &ExpandRequest<'_>) -> Result<String, ExpandError> {
    expand_prepared(dialect, req).map(|p| p.sql)
}

/// The final text of a query and the values of its query parameters (7.6a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The final SQL text. It holds the forms `{name:Type}` and no value of them.
    pub sql: String,
    /// The values of the query parameters of the final text, in the byte order of the names.
    pub params: QueryParams,
}

/// Checks the query parameters of the stored text before the expansion (7.6a, run time, step 1). A fault gives
/// `sql_invalid` at the position of the `{`. It gives the pairs of the strict forms.
fn stored_params(
    dialect: Dialect,
    req: &ExpandRequest<'_>,
    tokens: &[Token],
) -> Result<std::collections::BTreeMap<String, ParamType>, ExpandError> {
    let found = params::candidates(req.sql, tokens);
    if found.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let names = params::text_form_names(dialect, req.sql, tokens);
    let checked = params::check_stored(dialect, &found, |n| req.values.param_shape(n), &names, 1);
    if let Some(f) = checked.faults.first() {
        return Err(invalid(
            SqlInvalidKind::Parameter,
            f.message.clone(),
            Stage::Stored,
            Some(Position::at(req.sql, f.offset)),
        ));
    }
    Ok(checked.pairs)
}

/// Expands the stored SQL text of a source of the given dialect (7.2, steps 9 to 12), and makes the values of
/// the query parameters of a ClickHouse text (7.6a).
///
/// Before the expansion, it checks the forms `{name:Type}` of the stored text. After the checks of [`expand_for`],
/// it reads the candidates of the final text again: each one MUST be a pair of the stored text, else the answer is
/// `sql_invalid`. Then it checks the value of each parameter of the final text with the grammar of its type, and the
/// caps. Nothing of this needs I/O, so each refusal comes before any byte to the source.
pub fn expand_prepared(dialect: Dialect, req: &ExpandRequest<'_>) -> Result<Prepared, ExpandError> {
    expand_prepared_adhoc(dialect, req, None)
}

/// Expands the stored SQL text as [`expand_prepared`] does, with the filters of the `adhoc` variable of the source of a
/// panel query (7.10a). The server binds the filters first ([`adhoc::bind`]), so each refusal of a filter comes before.
/// `None` makes `$__adhocFilters` a document error, as for a variable query and an annotation.
pub fn expand_prepared_adhoc(
    dialect: Dialect,
    req: &ExpandRequest<'_>,
    adhoc: Option<&AdhocRun<'_>>,
) -> Result<Prepared, ExpandError> {
    let tokens = lex_for(dialect, req.sql)
        .map_err(|e| invalid(SqlInvalidKind::Lexer, e.to_string(), Stage::Stored, Some(e.position)))?;
    let stored = stored_params(dialect, req, &tokens)?;
    let ex = Expander { dialect, req, tokens: &tokens, adhoc, adhoc_places: std::cell::OnceCell::new() };
    let mut out = Frag::default();
    ex.expand_tokens(0, tokens.len(), &mut out, 0)?;
    let final_tokens = lex_for(dialect, &out.text).map_err(|e| {
        // In the dialect `postgres`, a final text that does not lex, while the stored text lexes, comes from a
        // free value that joins or opens a token (7.24, `E$name` with `\`). The value is refused. The error can
        // be after the range of the value: a value that opens a token leaves it open at a later offset. Then the value
        // that the error follows most closely is the one that the message names (finding I2).
        if dialect == Dialect::Postgres
            && let Some(ins) = out
                .ins
                .iter()
                .find(|i| i.end >= e.position.offset)
                .or_else(|| out.ins.iter().rev().find(|i| i.start <= e.position.offset))
        {
            return ExpandError::ValueRefused(ValueRefused {
                variable: ins.variable.clone(),
                reason: "The value changes the structure of the statement.".to_owned(),
            });
        }
        invalid(
            SqlInvalidKind::Lexer,
            format!("the final SQL has a lexer error: {}", e.kind.message()),
            Stage::Expanded,
            None,
        )
    })?;
    for ins in &out.ins {
        if !property_holds(dialect, &final_tokens, &out.text, ins) {
            return Err(ExpandError::ValueRefused(ValueRefused {
                variable: ins.variable.clone(),
                reason: "The value changes the structure of the statement.".to_owned(),
            }));
        }
    }
    check_statement_for(dialect, &out.text, &final_tokens)
        .map_err(|e| invalid(SqlInvalidKind::Statement, e.fault.message().to_owned(), Stage::Expanded, None))?;
    if out.text.len() > req.max_query_size {
        return Err(ExpandError::TooLarge { size: out.text.len(), limit: req.max_query_size });
    }
    // The second scan (7.6a, run time, step 3): author text in code can make a new form, also after a `$`.
    let found = params::final_candidates(&out.text, &final_tokens, &stored);
    let pairs = params::final_pairs(&found, &stored).ok_or_else(|| {
        let message = "the final SQL has a query parameter that the stored SQL has not".to_owned();
        invalid(SqlInvalidKind::Parameter, message, Stage::Expanded, None)
    })?;
    let params = params::bind(&pairs, req.values).map_err(|e| match e {
        BindError::Missing(name) => ExpandError::MissingVariable(name),
        BindError::Refused(r) => ExpandError::ValueRefused(r),
    })?;
    Ok(Prepared { sql: out.text, params })
}
