//! The checks at save (7.18): faults and warnings on the stored SQL of a panel or a variable.
//!
//! The functions give data. The caller builds the [`ValidationError`] items of `invalid_document`
//! and the `warnings` of 6.13 from it. The pointer of an item names the SQL line.

use super::adhoc;
use super::dialect::Dialect;
use super::interval::{GroupInterval, parse_group_interval};
use super::lexer::{PositionCursor, Token, TokenKind, heredoc_tag_length, lex_all_for, token_fault};
use super::macros::{AnyMacro, Call, Macro, Parens, PgMacro, lookup, parse_call_matched, parse_pg_call_matched};
use super::params::{self, FaultCode};
use super::placeholders::{self, BraceFault, Found};
use super::rules::{Place, context_fault, interval_argument_shape};
use super::statement::check_statement_for;
use super::values::{ParamShape, VariableDef};
use crate::error::{ErrorCode, ValidationError};
use crate::limits::{MAX_ERRORS, MAX_WARNINGS};
use crate::report::{SaveReport, Warning, shown};

/// Where the SQL is used. A variable query has no interval (7.10, 7.18 item 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlUse {
    /// The SQL of a panel.
    Panel,
    /// The SQL of a `query` variable.
    Variable,
    /// The SQL of an annotation (4.5b). It has no interval, as the SQL of a variable.
    Annotation,
}

impl SqlUse {
    /// Tells if the SQL has no interval: a macro that needs the interval is `not_permitted` (7.18 item 5).
    fn has_no_interval(self) -> bool {
        matches!(self, SqlUse::Variable | SqlUse::Annotation)
    }

    /// Gives the words of the messages: `a variable` or `an annotation`.
    fn owner(self) -> &'static str {
        match self {
            SqlUse::Annotation => "an annotation",
            _ => "a variable",
        }
    }
}

/// The ad hoc context of one SQL text at save (7.10a, 7.18 items 19 to 21). The default refuses the macro
/// `$__adhocFilters`: a caller that does not know the `adhoc` variables of the document never accepts it.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdhocSave<'a> {
    /// The names of the `adhoc` variables of the document. A placeholder of one of them is a fault (item 21).
    pub names: &'a [&'a str],
    /// True when the document has an `adhoc` variable whose source is the source of the text.
    pub on_source: bool,
}

/// One fault of 7.18. The caller makes one `invalid_document` item from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFault {
    /// The code of 4.37: `BadFormat` or `NotPermitted`.
    pub code: ErrorCode,
    /// The index of the SQL line in the array of lines, from 0.
    pub line_index: usize,
    /// The column in the line, in characters, from 1.
    pub column: usize,
    /// The English message, without the column.
    pub message: String,
}

impl SaveFault {
    /// Makes the validation error. `sql_pointer` is the pointer of the member `sql`, for example
    /// `/sections/0/panels/2/data/query/sql`. The item has `line` and `column` null (4.37), and
    /// its message gives the column in the line.
    pub fn to_validation_error(&self, sql_pointer: &str) -> ValidationError {
        ValidationError::new(
            format!("{sql_pointer}/{}", self.line_index),
            self.code,
            format!("column {}: {}", self.column, self.message),
        )
    }
}

/// The kind of a warning of 7.18.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// Warning 1: a `$name` or `${name}` whose name is not a variable of the document.
    UnknownVariable,
    /// Warning 2: a `[[name]]` whose name is a variable of the document.
    DoubleBracket,
    /// Warning 3: a word `FINAL` at any depth, or a word `SETTINGS` at depth 0.
    FinalOrSettings,
    /// Warning 4: a macro or a built-in variable of Grafana (`$__from`, `$__interval`, ...) inside a string
    /// or a quoted name. Grafana replaces it. Hune keeps it as text.
    MacroInLiteral,
    /// Warning 5: `${name:format}` inside a string, where `name` is a variable of the document and the
    /// format is not a format of Hune (for example `date`). Grafana formats it. Hune keeps it as text.
    UnknownFormatInString,
}

/// The names that Grafana replaces inside a string, and that Hune does not (warning 4). The macros of 7.10 come too.
const GRAFANA_BUILTINS: [&str; 4] = ["__interval", "__range", "__range_s", "__range_ms"];

/// One warning of 7.18. A warning never stops a save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveWarning {
    /// The kind.
    pub kind: WarningKind,
    /// The index of the SQL line, from 0.
    pub line_index: usize,
    /// The column in the line, in characters, from 1.
    pub column: usize,
    /// The English message, without the column.
    pub message: String,
}

impl SaveWarning {
    /// Gives the pointer and the message of the member `warnings` (6.13).
    pub fn to_pointer_message(&self, sql_pointer: &str) -> (String, String) {
        (format!("{sql_pointer}/{}", self.line_index), format!("column {}: {}", self.column, self.message))
    }
}

/// The result of [`save_checks`]. Each list holds at most `MAX_ERRORS` minus 1 or `MAX_WARNINGS` minus 1 items
/// (4.35): the items of the lowest positions in the text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveCheckResult {
    /// The faults, in the order of their position in the text.
    pub faults: Vec<SaveFault>,
    /// The warnings, in the order of their position in the text.
    pub warnings: Vec<SaveWarning>,
    /// True when the text has more faults than the list holds.
    pub more_faults: bool,
    /// True when the text has more warnings than the list holds.
    pub more_warnings: bool,
}

/// One item that a check keeps, with its byte offset and the order in which the check made it.
struct Kept<T> {
    offset: usize,
    seq: usize,
    item: T,
}

impl<T> PartialEq for Kept<T> {
    fn eq(&self, other: &Self) -> bool {
        (self.offset, self.seq) == (other.offset, other.seq)
    }
}

impl<T> Eq for Kept<T> {}

impl<T> PartialOrd for Kept<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Kept<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.offset, self.seq).cmp(&(other.offset, other.seq))
    }
}

/// The items of the `limit` lowest positions, in a heap of at most `limit` items. An item of a higher position
/// than all kept items is not kept, so the memory stays bounded for any number of items. The order of two
/// items at one position is the order in which the check made them, as a stable sort gives.
struct Lowest<T> {
    limit: usize,
    seq: usize,
    more: bool,
    heap: std::collections::BinaryHeap<Kept<T>>,
}

impl<T> Lowest<T> {
    fn new(limit: usize) -> Lowest<T> {
        Lowest { limit, seq: 0, more: false, heap: std::collections::BinaryHeap::new() }
    }

    /// Offers the item that `make` gives. `make` runs only when the item is kept, so an item that the list
    /// does not keep costs no text.
    fn offer_with(&mut self, offset: usize, make: impl FnOnce() -> T) {
        let seq = self.seq;
        self.seq += 1;
        if self.heap.len() < self.limit {
            self.heap.push(Kept { offset, seq, item: make() });
            return;
        }
        self.more = true;
        // `seq` grows, so a new item is lower than the highest kept item only at a lower offset.
        if self.heap.peek().is_some_and(|top| offset < top.offset) {
            self.heap.pop();
            self.heap.push(Kept { offset, seq, item: make() });
        }
    }

    /// Gives the kept items in the order of their position.
    fn into_sorted(self) -> Vec<Kept<T>> {
        self.heap.into_sorted_vec()
    }
}

struct Checker<'a> {
    dialect: Dialect,
    /// The offsets of the `$__interval` that stand for `I` as the interval argument of a group macro (7.23).
    interval_arguments: std::collections::HashSet<usize>,
    /// The end of the last argument range whose `$__interval` are in `interval_arguments`.
    interval_marked_to: usize,
    src: &'a str,
    tokens: &'a [Token],
    usage: SqlUse,
    vars: &'a [VariableDef],
    faults: Lowest<(ErrorCode, String)>,
    warnings: Lowest<(WarningKind, String)>,
    /// The parentheses of the text, matched one time when the first macro call needs them.
    parens: Option<Parens>,
    /// The permitted places of `$__adhocFilters`, read one time when the first macro needs them (7.10a).
    adhoc_places: Option<adhoc::AdhocPlaces>,
    /// The ad hoc context of the text (7.10a).
    adhoc: AdhocSave<'a>,
    /// The highest end offset of the argument of a macro call so far. A placeholder before it is inside an argument.
    macro_args_to: usize,
}

impl Checker<'_> {
    fn fault(&mut self, code: ErrorCode, offset: usize, message: String) {
        self.faults.offer_with(offset, || (code, message));
    }

    /// Adds a fault whose message `message` makes only when the list keeps it.
    fn fault_with(&mut self, code: ErrorCode, offset: usize, message: impl FnOnce() -> String) {
        self.faults.offer_with(offset, || (code, message()));
    }

    fn warn(&mut self, kind: WarningKind, offset: usize, message: String) {
        self.warnings.offer_with(offset, || (kind, message));
    }

    /// Adds a warning whose message `message` makes only when the list keeps it.
    fn warn_with(&mut self, kind: WarningKind, offset: usize, message: impl FnOnce() -> String) {
        self.warnings.offer_with(offset, || (kind, message()));
    }

    fn var(&self, name: &str) -> Option<&VariableDef> {
        self.vars.iter().find(|v| v.name == name)
    }

    /// Reads the placeholders of a byte range in a context, and the `[[name]]` forms.
    fn scan(&mut self, lo: usize, hi: usize, place: Place) {
        let mut p = lo;
        while let Some(found) = placeholders::next_for(self.dialect, self.src, p, hi) {
            p = found.end();
            match found {
                Found::BadBrace { start, fault, .. } => {
                    if place == Place::Code {
                        let what = match fault {
                            BraceFault::Malformed => "a ${ does not match ${name} or ${name:format}".to_owned(),
                            BraceFault::UnknownFormat(f) => {
                                format!("the format {} is not in the list of formats", shown(&f))
                            }
                        };
                        self.fault(ErrorCode::BadFormat, start, what);
                    } else if let BraceFault::UnknownFormat(f) = fault
                        && matches!(place, Place::String | Place::QuotedName)
                    {
                        let name: String = self.src[start + 2..hi]
                            .bytes()
                            .take_while(|b| placeholders::is_name_byte(*b))
                            .map(char::from)
                            .collect();
                        if self.var(&name).is_some() {
                            let (f, name) = (shown(&f), shown(&name));
                            let msg = format!(
                                "the format {f} of ${{{name}}} is not a format of Hune: the text stays as it is"
                            );
                            self.warn(WarningKind::UnknownFormatInString, start, msg);
                        }
                    }
                }
                Found::Placeholder(ph) => self.placeholder(&ph, place),
            }
        }
        let mut p = lo;
        while let Some((at, name)) = placeholders::next_double_bracket(self.src, p, hi) {
            p = at + 2;
            if place != Place::Heredoc && self.var(name).is_some() {
                let msg = format!("[[{name}]] is not a placeholder: write ${{{name}}}");
                self.warn(WarningKind::DoubleBracket, at, msg);
            }
        }
    }

    fn placeholder(&mut self, ph: &placeholders::Placeholder<'_>, place: Place) {
        if ph.name.starts_with("__") {
            if self.interval_arguments.contains(&ph.start) {
                return;
            }
            if place == Place::Code {
                if self.dialect == Dialect::Postgres {
                    self.pg_macro_call(ph);
                } else {
                    self.macro_call(ph);
                }
            } else if matches!(place, Place::String | Place::EString | Place::QuotedName)
                && (lookup(self.dialect, ph.name).is_some() || GRAFANA_BUILTINS.contains(&ph.name))
            {
                let msg = format!("${} stays text inside a string or a quoted name: Grafana replaces it", ph.name);
                self.warn(WarningKind::MacroInLiteral, ph.start, msg);
            }
            return;
        }
        let src = self.src;
        if self.adhoc.names.contains(&ph.name) {
            let msg = || {
                format!(
                    "{} names the adhoc variable {}: only $__adhocFilters reads its filters (7.10a)",
                    shown(&src[ph.start..ph.end]),
                    shown(ph.name)
                )
            };
            self.fault_with(ErrorCode::NotPermitted, ph.start, msg);
            return;
        }
        let Some(def) = self.var(ph.name) else {
            let msg = || format!("{} is not a variable of the document", shown(&src[ph.start..ph.end]));
            self.warn_with(WarningKind::UnknownVariable, ph.start, msg);
            return;
        };
        let fault = context_fault(place, def.is_closed_custom(), def.may_be_list(), ph.format);
        if let Some(f) = fault {
            self.fault_with(ErrorCode::NotPermitted, ph.start, || f.message(ph.name));
        }
    }

    fn macro_call(&mut self, ph: &placeholders::Placeholder<'_>) {
        let Some(m) = Macro::from_name(ph.name) else {
            self.fault(ErrorCode::BadFormat, ph.start, format!("${} is not a macro of Hune", shown(ph.name)));
            return;
        };
        if ph.format.is_some() {
            self.fault(ErrorCode::BadFormat, ph.start, format!("{} takes no format", m.name()));
            return;
        }
        if self.usage.has_no_interval() && m.needs_interval() {
            let msg = format!("{} is not permitted in the SQL of {}", m.name(), self.usage.owner());
            self.fault(ErrorCode::NotPermitted, ph.start, msg);
            return;
        }
        let parens = self.parens.get_or_insert_with(|| Parens::of(self.src, self.tokens));
        let call = parse_call_matched(m, self.src, self.tokens, ph.end, parens);
        if let Ok(Some(c)) = &call {
            self.macro_args_to = self.macro_args_to.max(self.tokens[c.close].start);
        }
        match call {
            Err(f) => self.fault(ErrorCode::BadFormat, ph.start, f.message(m)),
            Ok(Some(call)) if m == Macro::ConditionalAll => {
                if let Some(message) = self.conditional_fault(&call) {
                    self.fault(ErrorCode::BadFormat, ph.start, format!("$__conditionalAll: {message}"));
                }
            }
            Ok(_) if m == Macro::AdhocFilters => self.adhoc_macro(ph),
            Ok(_) => {}
        }
    }

    /// Checks the place of `$__adhocFilters` (7.10a, 7.18 items 19 and 20): the SQL of a panel, an `adhoc` variable on
    /// the source of the text, not inside the argument of another macro, and a place of [`adhoc::AdhocPlaces`]: one
    /// term of the top-level conjunction of a `WHERE` or `PREWHERE` clause. The run applies the same rules, so the save
    /// and the run agree.
    fn adhoc_macro(&mut self, ph: &placeholders::Placeholder<'_>) {
        let message = if self.usage != SqlUse::Panel {
            Some(format!("$__adhocFilters is not permitted in the SQL of {} (7.10a)", self.usage.owner()))
        } else if !self.adhoc.on_source {
            Some("$__adhocFilters needs an adhoc variable whose source is the source of the query (7.10a)".to_owned())
        } else if ph.start < self.macro_args_to {
            Some("$__adhocFilters is not permitted inside another macro (7.10a)".to_owned())
        } else if !self
            .adhoc_places
            .get_or_insert_with(|| adhoc::AdhocPlaces::of(self.src, self.tokens))
            .permits(ph.start, ph.end)
        {
            Some(adhoc::PLACE_MESSAGE.to_owned())
        } else {
            None
        };
        if let Some(m) = message {
            self.fault(ErrorCode::NotPermitted, ph.start, m);
        }
    }

    /// Checks a macro of the dialect `postgres` in code (7.22, 7.23, 7.26).
    fn pg_macro_call(&mut self, ph: &placeholders::Placeholder<'_>) {
        if ph.name == "__interval" {
            let msg = "$__interval is valid only as the interval argument of a group macro (7.23)".to_owned();
            self.fault(ErrorCode::BadFormat, ph.start, msg);
            return;
        }
        if ph.name == adhoc::MACRO_NAME {
            let msg = "$__adhocFilters is permitted only on a clickhouse source (7.10a)".to_owned();
            self.fault(ErrorCode::NotPermitted, ph.start, msg);
            return;
        }
        let Some(AnyMacro::Postgres(m)) = lookup(self.dialect, ph.name) else {
            self.fault(ErrorCode::BadFormat, ph.start, format!("${} is not a macro of Hune", shown(ph.name)));
            return;
        };
        if ph.format.is_some() {
            self.fault(ErrorCode::BadFormat, ph.start, format!("{} takes no format", m.name()));
            return;
        }
        if self.usage.has_no_interval() && m.needs_interval() {
            let msg = format!("{} is not permitted in the SQL of {}", m.name(), self.usage.owner());
            self.fault(ErrorCode::NotPermitted, ph.start, msg);
            return;
        }
        let parens = self.parens.get_or_insert_with(|| Parens::of(self.src, self.tokens));
        let call = match parse_pg_call_matched(m, self.src, self.tokens, ph.end, parens) {
            Err(f) => {
                self.fault(ErrorCode::BadFormat, ph.start, f.message_for(m.name(), m.arity()));
                return;
            }
            Ok(call) => call,
        };
        let Some(call) = call else { return };
        if m == PgMacro::ConditionalAll {
            if let Some(message) = self.conditional_fault(&call) {
                self.fault(ErrorCode::BadFormat, ph.start, format!("$__conditionalAll: {message}"));
            }
        } else if m.is_group()
            && let Some((lo, hi)) = call.second
        {
            self.group_interval(ph, m.name(), lo, hi);
        }
    }

    /// Checks the second argument of a group macro (7.23, 7.26 items 4 and 5).
    fn group_interval(&mut self, ph: &placeholders::Placeholder<'_>, name: &str, lo: usize, hi: usize) {
        // A `$__interval` in the argument is not a second fault: the fault of the argument covers it.
        // The argument ranges of the calls nest or do not meet, and the walk meets them in order. So a range
        // that starts before the end of the last marked range is inside it: its marks exist, and a second scan
        // would make nested calls quadratic.
        let (arg_start, arg_end) = (self.tokens[lo].start, self.tokens[hi - 1].end);
        if arg_start >= self.interval_marked_to {
            let mut p = arg_start;
            while let Some(found) = placeholders::next_for(self.dialect, self.src, p, arg_end) {
                p = found.end();
                if let Found::Placeholder(inner) = found
                    && inner.name == "__interval"
                {
                    self.interval_arguments.insert(inner.start);
                }
            }
            self.interval_marked_to = arg_end;
        }
        if let Err(e) = interval_argument_shape(self.src, self.tokens, lo, hi) {
            self.fault(ErrorCode::BadFormat, ph.start, format!("{}: {}", name, e.message));
            return;
        }
        let text = &self.src[arg_start..arg_end];
        if parse_group_interval(text).is_ok_and(|g| g == GroupInterval::Builtin) && self.usage.has_no_interval() {
            let msg = format!("a group macro with $__interval is not permitted in the SQL of {}", self.usage.owner());
            self.fault(ErrorCode::NotPermitted, ph.start, msg);
        }
    }

    /// The second argument of `$__conditionalAll` MUST be exactly one placeholder without a format, and it
    /// MUST name a `query` or a `custom` variable of the document (7.10).
    fn conditional_fault(&self, call: &Call) -> Option<&'static str> {
        let (lo, hi) = call.second?;
        let (start, end) = (self.tokens[lo].start, self.tokens[hi - 1].end);
        match placeholders::next(self.src, start, end) {
            Some(Found::Placeholder(ph)) if ph.start == start && ph.end == end && !ph.name.starts_with("__") => {
                if ph.format.is_some() {
                    Some("the second argument has no format")
                } else {
                    match self.var(ph.name) {
                        None => Some("the second argument is not a variable of the document"),
                        Some(def) if def.list_options().is_none() => {
                            Some("the second argument must be a query variable or a custom variable")
                        }
                        Some(_) => None,
                    }
                }
            }
            _ => Some("the second argument must be exactly one placeholder: $name or ${name}"),
        }
    }

    fn walk(&mut self) {
        let tokens = self.tokens;
        let mut i = 0;
        while i < tokens.len() {
            let t = tokens[i];
            match t.kind {
                k if k.is_code() => {
                    let mut j = i;
                    while j < tokens.len() && tokens[j].kind.is_code() {
                        j += 1;
                    }
                    self.scan(t.start, tokens[j - 1].end, Place::Code);
                    i = j;
                    continue;
                }
                TokenKind::String | TokenKind::QuotedName | TokenKind::EString | TokenKind::PrefixedString => {
                    let unicode_name = t.kind == TokenKind::QuotedName
                        && matches!(self.src.as_bytes().get(t.start), Some(b'U' | b'u'));
                    let place = match t.kind {
                        TokenKind::String => Place::String,
                        TokenKind::EString => Place::EString,
                        TokenKind::PrefixedString => Place::Prefixed,
                        _ if unicode_name => Place::Prefixed,
                        _ => Place::QuotedName,
                    };
                    if t.closed
                        && let Some((lo, hi)) = t.content_range(self.src)
                    {
                        self.scan(lo, hi, place);
                    }
                }
                TokenKind::Heredoc if t.closed && self.dialect == Dialect::Postgres => {
                    let b = self.src.as_bytes();
                    let mut i = t.start + 1;
                    while i < t.end && b[i] != b'$' {
                        i += 1;
                    }
                    let tag = (i + 1).min(t.end) - t.start;
                    self.scan(t.start + tag, t.end - tag, Place::DollarQuote);
                }
                TokenKind::Heredoc if t.closed => {
                    let tag = heredoc_tag_length(self.src.as_bytes(), t.start);
                    self.scan(t.start + tag, t.end - tag, Place::Heredoc);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn words(&mut self) {
        let mut depth = 0usize;
        let tokens = self.tokens;
        for t in tokens {
            match t.kind {
                TokenKind::Punct => match self.src.as_bytes()[t.start] {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                    _ => {}
                },
                TokenKind::Word => {
                    let text = t.text(self.src);
                    if text.eq_ignore_ascii_case("FINAL") {
                        let msg = || "FINAL makes the query slow".to_owned();
                        self.warn_with(WarningKind::FinalOrSettings, t.start, msg);
                    } else if depth == 0 && text.eq_ignore_ascii_case("SETTINGS") {
                        let msg = || "SETTINGS can change the limits of the source account".to_owned();
                        self.warn_with(WarningKind::FinalOrSettings, t.start, msg);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Runs the checks 1 to 8 and 10 to 16 and the warnings 1 to 3 of 7.18 on the stored SQL text. The items 10 to 16
/// are the query parameters of 7.6a: [`super::params::check_stored`] is the same function as the run time.
///
/// `sql` is the SQL lines joined with LF (4.16). `variables` are all variables of the document.
/// The check 9 is [`check_source`].
pub fn save_checks(sql: &str, usage: SqlUse, variables: &[VariableDef]) -> SaveCheckResult {
    save_checks_for(Dialect::ClickHouse, sql, usage, variables)
}

/// Runs the checks of 7.18 for the dialect of the source of the panel or the variable. For `postgres` the
/// changes of 7.26 apply: the lexer of 7.20, the statement check of 7.25, the macros of 7.22 and 7.23, the
/// places of 7.21, and no warning 3. For `clickhouse` it is [`save_checks`].
///
/// Each list keeps the items of the lowest positions: at most `MAX_ERRORS` minus 1 faults and `MAX_WARNINGS`
/// minus 1 warnings (4.35). `more_faults` and `more_warnings` tell that the text has more.
pub fn save_checks_for(dialect: Dialect, sql: &str, usage: SqlUse, variables: &[VariableDef]) -> SaveCheckResult {
    check_bounded(dialect, sql, usage, variables, AdhocSave::default(), MAX_ERRORS - 1, MAX_WARNINGS - 1)
}

/// Runs [`save_checks_for`] with the ad hoc context of the text (7.10a, 7.18 items 19 to 21).
pub fn save_checks_adhoc(
    dialect: Dialect,
    sql: &str,
    usage: SqlUse,
    variables: &[VariableDef],
    adhoc: AdhocSave<'_>,
) -> SaveCheckResult {
    check_bounded(dialect, sql, usage, variables, adhoc, MAX_ERRORS - 1, MAX_WARNINGS - 1)
}

/// Runs [`save_checks_for`] and puts the items into the lists of `report` (6.13, 7.18). `sql_pointer` is the
/// pointer of the member `sql`, for example `/sections/0/panels/2/data/query/sql`.
///
/// The check keeps only the items that the lists can take: the items of the lowest positions in the text.
/// When the text has more items than the room of a list, the list stops with its last item. Thus the work is
/// linear in the length of the text, and the memory holds at most the room of each list.
pub fn save_checks_into(
    dialect: Dialect,
    sql: &str,
    usage: SqlUse,
    variables: &[VariableDef],
    sql_pointer: &str,
    report: &mut SaveReport,
) {
    save_checks_into_adhoc(dialect, sql, usage, variables, AdhocSave::default(), sql_pointer, report);
}

/// Runs [`save_checks_into`] with the ad hoc context of the text (7.10a, 7.18 items 19 to 21).
pub fn save_checks_into_adhoc(
    dialect: Dialect,
    sql: &str,
    usage: SqlUse,
    variables: &[VariableDef],
    adhoc: AdhocSave<'_>,
    sql_pointer: &str,
    report: &mut SaveReport,
) {
    let result = check_bounded(dialect, sql, usage, variables, adhoc, report.error_room(), report.warning_room());
    for f in &result.faults {
        report.push_error(f.to_validation_error(sql_pointer));
    }
    if result.more_faults {
        report.stop_errors();
    }
    for w in &result.warnings {
        let (pointer, message) = w.to_pointer_message(sql_pointer);
        report.push_warning(Warning { pointer, message });
    }
    if result.more_warnings {
        report.stop_warnings();
    }
}

/// Runs the checks and keeps at most `fault_room` faults and `warning_room` warnings, of the lowest positions.
fn check_bounded(
    dialect: Dialect,
    sql: &str,
    usage: SqlUse,
    variables: &[VariableDef],
    adhoc: AdhocSave<'_>,
    fault_room: usize,
    warning_room: usize,
) -> SaveCheckResult {
    let tokens = lex_all_for(dialect, sql);
    let mut c = Checker {
        dialect,
        interval_arguments: std::collections::HashSet::new(),
        interval_marked_to: 0,
        src: sql,
        tokens: &tokens,
        usage,
        vars: variables,
        faults: Lowest::new(fault_room),
        warnings: Lowest::new(warning_room),
        parens: None,
        adhoc_places: None,
        adhoc,
        macro_args_to: 0,
    };
    for t in &tokens {
        if let Some(kind) = token_fault(t) {
            c.fault_with(ErrorCode::BadFormat, t.start, || kind.message().to_owned());
        }
    }
    if let Err(e) = check_statement_for(dialect, sql, &tokens) {
        c.fault(ErrorCode::BadFormat, e.position.offset, e.fault.message().to_owned());
    }
    c.walk();
    if dialect == Dialect::ClickHouse {
        c.words();
    }
    // The query parameters of 7.6a (items 10 to 16): the same rules as the run, on the stored text.
    let found = params::candidates(sql, &tokens);
    if !found.is_empty() {
        let shapes: std::collections::HashMap<&str, ParamShape> =
            variables.iter().map(|v| (v.name.as_str(), v.param_shape())).collect();
        let names = params::text_form_names(dialect, sql, &tokens);
        // The list keeps `fault_room` faults of the lowest positions, and one more fault sets its mark `more`.
        let room = fault_room.saturating_add(1);
        let checked = params::check_stored(dialect, &found, |n| shapes.get(n).copied(), &names, room);
        for f in checked.faults {
            let code = match f.code {
                FaultCode::BadFormat => ErrorCode::BadFormat,
                FaultCode::NotPermitted => ErrorCode::NotPermitted,
                FaultCode::BadReference => ErrorCode::BadReference,
            };
            c.fault(code, f.offset, f.message);
        }
    }
    let (more_faults, more_warnings) = (c.faults.more, c.warnings.more);
    let faults = c.faults.into_sorted();
    let warnings = c.warnings.into_sorted();
    // The positions of the kept items only, in one pass for each list: the offsets are in increasing order.
    let mut cursor = PositionCursor::new(sql);
    let faults = faults
        .into_iter()
        .map(|k| {
            let p = cursor.at(k.offset);
            let (code, message) = k.item;
            SaveFault { code, line_index: p.line as usize - 1, column: p.column as usize, message }
        })
        .collect();
    let mut cursor = PositionCursor::new(sql);
    let warnings = warnings
        .into_iter()
        .map(|k| {
            let p = cursor.at(k.offset);
            let (kind, message) = k.item;
            SaveWarning { kind, line_index: p.line as usize - 1, column: p.column as usize, message }
        })
        .collect();
    SaveCheckResult { faults, warnings, more_faults, more_warnings }
}

/// Check 9 of 7.18: the source id MUST be one that the configuration declares.
///
/// The error has the code `BadReference`. Its pointer is the member `source`, which the caller knows.
pub fn check_source(source: &str, declared: &[&str]) -> Result<(), ValidationError> {
    if declared.contains(&source) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "",
            ErrorCode::BadReference,
            format!("source {source} is not declared in the configuration"),
        ))
    }
}
