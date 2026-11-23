// SPDX-License-Identifier: MIT OR Apache-2.0
//! The grammar of the MongoDB query tree (7.29 to 7.33) and the string leaves of 7.34.
//!
//! The grammar is an allowlist. A stage, an operator or a form that the tables of 7.31 do not list is
//! refused, at any depth and in any position. This includes the keys of nested expressions and each name
//! that starts with `$_`. The module has three jobs:
//!
//! 1. [`shape_top`] checks the top object of 7.29 and makes the [`MongoQuery`].
//! 2. [`check_top`] walks the whole tree with the contexts Q, E, A and W (7.30), the name places and
//!    the collections of 7.33. It runs on the stored tree T0 (placeholders are values of an unknown
//!    type) and on the expanded tree T1 (the guard of step 6 of 7.35).
//! 3. [`classify`] reads one string value (7.34): a macro, a whole-leaf placeholder, an embedded
//!    placeholder, a field path, a variable reference or text. The expansion uses the same function, so
//!    the check and the expansion cannot disagree.
//!
//! A fault is data ([`Fault`]): the item L1 to L9 of 7.36, the code of 4.37, the path of the value and a
//! message. The caller turns the path into a position with [`crate::mongo::json::Span::locate`].

use crate::error::ErrorCode;
use crate::mongo::json::{Seg, pointer};
use crate::mongo::limits::{
    MAX_PIPELINE_DEPTH, MAX_QUERY_KEY_BYTES, MAX_QUERY_STAGES, MAX_REGEX_BYTES, MAX_SKIP_LIMIT, MAX_SORT_KEYS,
};
use crate::mongo::tree::{FIND_MEMBERS, FindSpec, MongoQuery, QueryBody};
use crate::mongo::value::{Document, MongoValue};
use crate::report::shown;
use crate::sql::SqlUse;
use crate::sql::placeholders::{Format, is_name_byte};
use crate::sql::values::{ResolvedValues, VariableDef};

// ---- The tables of 7.31 ----

/// The stages of `aggregate` (7.31).
pub const STAGES: &[&str] = &[
    "$match",
    "$project",
    "$addFields",
    "$set",
    "$unset",
    "$group",
    "$sort",
    "$limit",
    "$skip",
    "$count",
    "$unwind",
    "$replaceRoot",
    "$replaceWith",
    "$sortByCount",
    "$bucket",
    "$bucketAuto",
    "$facet",
    "$setWindowFields",
    "$densify",
    "$fill",
    "$lookup",
    "$unionWith",
];

/// The operators of a field in the context Q (7.31).
pub const Q_FIELD_OPS: &[&str] = &[
    "$eq",
    "$ne",
    "$gt",
    "$gte",
    "$lt",
    "$lte",
    "$in",
    "$nin",
    "$not",
    "$exists",
    "$type",
    "$size",
    "$all",
    "$elemMatch",
    "$mod",
    "$regex",
    "$options",
];

/// The keys of a filter that join other filters or that hold an expression (7.30, G4).
pub const Q_LOGIC_KEYS: &[&str] = &["$and", "$or", "$nor", "$expr"];

/// The operators of the context E (7.31).
pub const E_OPS: &[&str] = &[
    "$eq",
    "$ne",
    "$gt",
    "$gte",
    "$lt",
    "$lte",
    "$cmp",
    "$and",
    "$or",
    "$not",
    "$in",
    "$cond",
    "$ifNull",
    "$switch",
    "$add",
    "$subtract",
    "$multiply",
    "$divide",
    "$mod",
    "$abs",
    "$ceil",
    "$floor",
    "$round",
    "$trunc",
    "$sqrt",
    "$pow",
    "$exp",
    "$ln",
    "$log",
    "$log10",
    "$concat",
    "$substrBytes",
    "$substrCP",
    "$toLower",
    "$toUpper",
    "$trim",
    "$ltrim",
    "$rtrim",
    "$split",
    "$strLenBytes",
    "$strLenCP",
    "$strcasecmp",
    "$indexOfBytes",
    "$indexOfCP",
    "$replaceOne",
    "$replaceAll",
    "$regexMatch",
    "$regexFind",
    "$regexFindAll",
    "$arrayElemAt",
    "$first",
    "$last",
    "$size",
    "$slice",
    "$concatArrays",
    "$filter",
    "$map",
    "$reduce",
    "$isArray",
    "$reverseArray",
    "$range",
    "$zip",
    "$arrayToObject",
    "$objectToArray",
    "$sortArray",
    "$firstN",
    "$lastN",
    "$maxN",
    "$minN",
    "$indexOfArray",
    "$setUnion",
    "$setIntersection",
    "$setDifference",
    "$setEquals",
    "$setIsSubset",
    "$anyElementTrue",
    "$allElementsTrue",
    "$dateTrunc",
    "$dateAdd",
    "$dateSubtract",
    "$dateDiff",
    "$dateFromParts",
    "$dateFromString",
    "$dateToString",
    "$dateToParts",
    "$year",
    "$month",
    "$dayOfMonth",
    "$dayOfWeek",
    "$dayOfYear",
    "$hour",
    "$minute",
    "$second",
    "$millisecond",
    "$week",
    "$isoWeek",
    "$isoWeekYear",
    "$isoDayOfWeek",
    "$toDate",
    "$convert",
    "$toInt",
    "$toLong",
    "$toDouble",
    "$toDecimal",
    "$toString",
    "$toBool",
    "$toObjectId",
    "$isNumber",
    "$type",
    "$mergeObjects",
    "$getField",
    "$setField",
    "$unsetField",
    "$literal",
    "$let",
];

/// The accumulators of the context A (7.31).
pub const A_OPS: &[&str] = &[
    "$sum",
    "$avg",
    "$min",
    "$max",
    "$count",
    "$first",
    "$last",
    "$push",
    "$addToSet",
    "$stdDevPop",
    "$stdDevSamp",
    "$top",
    "$bottom",
    "$topN",
    "$bottomN",
    "$firstN",
    "$lastN",
    "$maxN",
    "$minN",
    "$median",
    "$percentile",
    "$mergeObjects",
];

/// The window operators that are not accumulators (7.31). The context W is these names and A.
pub const W_ONLY_OPS: &[&str] = &[
    "$rank",
    "$denseRank",
    "$documentNumber",
    "$shift",
    "$derivative",
    "$integral",
    "$expMovingAvg",
    "$covariancePop",
    "$covarianceSamp",
    "$locf",
    "$linearFill",
];

fn in_table(table: &[&str], name: &str) -> bool {
    !name.is_empty() && table.contains(&name)
}

fn is_e_op(name: &str) -> bool {
    in_table(E_OPS, name)
}

fn is_w_op(name: &str) -> bool {
    in_table(A_OPS, name) || in_table(W_ONLY_OPS, name)
}

/// Gives the reason of 7.32 for a refused name, or `None` for a name that is in no table of 7.32.
pub fn refusal_reason(name: &str) -> Option<&'static str> {
    Some(match name {
        "$out" | "$merge" => "They write to a collection.",
        "$function" | "$accumulator" | "$where" => "They run JavaScript on the server.",
        "mapReduce" | "$code" | "$codeWScope" => "Server-side JavaScript in any other form.",
        "$currentOp"
        | "$listSessions"
        | "$listLocalSessions"
        | "$listSampledQueries"
        | "$queryStats"
        | "$planCacheStats" => "They show the work of other users, or of the server.",
        "$collStats" | "$indexStats" | "$listCatalog" | "$listSearchIndexes" => {
            "They show the size, the use and the structure of the server."
        }
        "$changeStream" | "$changeStreamSplitLargeEvent" => "They open a stream that does not end.",
        "$graphLookup" => "It is a recursive join.",
        "$search" | "$searchMeta" | "$vectorSearch" | "$documents" | "$geoNear" | "$redact" | "$text"
        | "$jsonSchema" => "Not needed, or they need a server feature that the role cannot check.",
        "$sample" | "$sampleRate" | "$rand" => "The result is not the same at each run.",
        "$$NOW" | "$$CLUSTER_TIME" | "$$USER_ROLES" | "$$SEARCH_META" => {
            "The time is a macro of 7.34. The others show the server or the role."
        }
        "$meta" | "$comment" | "$natural" => "Internal or not needed.",
        n if n.starts_with("$_") => "Internal or not needed.",
        "allowDiskUse"
        | "hint"
        | "collation"
        | "let"
        | "readConcern"
        | "maxTimeMS"
        | "writeConcern"
        | "explain"
        | "bypassDocumentValidation"
        | "batchSize"
        | "cursor" => "An author cannot change the command of Hune.",
        _ => return None,
    })
}

/// The message of a refused name (7.32): `<name> is not permitted in a MongoDB query of Hune`, with the
/// reason when 7.32 gives one.
pub fn refusal_message(name: &str) -> String {
    match refusal_reason(name) {
        Some(r) => format!("{name} is not permitted in a MongoDB query of Hune: {r}"),
        None => format!("{name} is not permitted in a MongoDB query of Hune"),
    }
}

// ---- Faults and warnings ----

/// The item of 7.36 that a fault belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// A fault of the profile J, or a limit of 7.28.
    L1,
    /// A top object that breaks 7.29.
    L2,
    /// A stage, an operator or a form that is in no table of 7.31, or that is in 7.32.
    L3,
    /// A shape that breaks 7.30 (G2 to G9) or the row of 7.31.
    L4,
    /// A `$__name` that is not a macro of 7.34.
    L5,
    /// A placeholder in a key, in a name place, or in a stored string that starts with `$`.
    L6,
    /// A format that 7.34 does not list for the place, or a list with no format in a whole-leaf placeholder.
    L7,
    /// A collection or a database that the source does not list.
    L8,
    /// In the query of a variable: `$__interval_ms` or `$__interval_s`.
    L9,
}

impl Item {
    /// Gives the name of the item, for example `L6`.
    pub fn as_str(self) -> &'static str {
        match self {
            Item::L1 => "L1",
            Item::L2 => "L2",
            Item::L3 => "L3",
            Item::L4 => "L4",
            Item::L5 => "L5",
            Item::L6 => "L6",
            Item::L7 => "L7",
            Item::L8 => "L8",
            Item::L9 => "L9",
        }
    }
}

/// One fault of the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// The item of 7.36.
    pub item: Item,
    /// The code of 4.37.
    pub code: ErrorCode,
    /// The path of the value that has the fault.
    pub path: Vec<Seg>,
    /// True when the fault is on the last key of the path, and not on its value.
    pub at_key: bool,
    /// The English message. It holds no part of a value.
    pub message: String,
}

impl Fault {
    /// Gives the JSON pointer inside the query text, for example `/aggregate/2/$out`.
    pub fn pointer(&self) -> String {
        pointer(&self.path)
    }

    fn top(item: Item, code: ErrorCode, message: impl Into<String>) -> Fault {
        Fault { item, code, path: Vec::new(), at_key: false, message: message.into() }
    }
}

/// The kind of a warning of 7.36.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarnKind {
    /// Warning 1: a `${name}` or `${name:format}` whose name is not a variable of the document.
    UnknownVariable,
    /// Warning 2: a `[[name]]` whose name is a variable of the document.
    DoubleBracket,
    /// Warning 3: a macro name in a longer string. The text stays as it is.
    MacroInText,
}

/// One warning of 7.36.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The kind.
    pub kind: WarnKind,
    /// The path of the string.
    pub path: Vec<Seg>,
    /// The English message.
    pub message: String,
}

/// The result of [`check_top`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// The faults, in the order of the walk.
    pub faults: Vec<Fault>,
    /// The warnings.
    pub warnings: Vec<Warning>,
    /// True when the walk found more faults than the list holds.
    pub more_faults: bool,
    /// True when the walk found more warnings than the list holds.
    pub more_warnings: bool,
}

/// The most faults that one walk collects. The walk goes on, so the work stays bounded by the tree.
pub const MAX_FAULTS: usize = 100;

/// The most warnings that one walk of [`check_top`] collects: the room of an empty list of 6.13 (4.35).
const MAX_WALK_WARNINGS: usize = crate::limits::MAX_WARNINGS - 1;

/// A list of faults with a cap. A fault past the cap is counted, but the list does not keep it.
struct Faults {
    list: Vec<Fault>,
    limit: usize,
    /// The faults found, kept or not.
    found: usize,
}

impl Faults {
    fn new(limit: usize) -> Faults {
        Faults { list: Vec::new(), limit, found: 0 }
    }

    fn push(&mut self, fault: Fault) {
        self.found += 1;
        if self.list.len() < self.limit {
            self.list.push(fault);
        }
    }

    fn more(&self) -> bool {
        self.found > self.list.len()
    }
}

// ---- The variables of the document ----

/// What the grammar needs to know about the variables: is a name a variable, and can a selection of it
/// be a list. The caller gives the variables of the document (save) or of the request (run).
pub trait VariableNames {
    /// Gives `None` when `name` is not a variable. Else it gives `Some(may_be_list)`.
    fn lookup(&self, name: &str) -> Option<bool>;
}

/// The variables of the document, for the checks at save.
pub struct DefNames<'a>(pub &'a [VariableDef]);

impl VariableNames for DefNames<'_> {
    fn lookup(&self, name: &str) -> Option<bool> {
        self.0.iter().find(|d| d.name == name).map(VariableDef::may_be_list)
    }
}

/// The variables of a request, for the run. The list test is left to the expansion, which sees the selection.
pub struct ResolvedNames<'a>(pub &'a ResolvedValues);

impl VariableNames for ResolvedNames<'_> {
    fn lookup(&self, name: &str) -> Option<bool> {
        self.0.is_declared(name).then_some(false)
    }
}

/// No variable: the expanded tree T1 has no placeholder.
pub struct NoVariables;

impl VariableNames for NoVariables {
    fn lookup(&self, _name: &str) -> Option<bool> {
        None
    }
}

// ---- The macros and the string classes of 7.34 ----

/// A macro of 7.34.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Macro {
    /// `$__timeFrom`: the Date of `from_ms`.
    TimeFrom,
    /// `$__timeTo`: the Date of `to_ms`.
    TimeTo,
    /// `$__from`: the Int64 `from_ms`.
    From,
    /// `$__to`: the Int64 `to_ms`.
    To,
    /// `$__interval_ms`: the Int64 `I`.
    IntervalMs,
    /// `$__interval_s`: the Int64 `S`.
    IntervalS,
}

impl Macro {
    /// Reads a macro name, with the two leading underscores and without the `$`.
    pub fn from_name(name: &str) -> Option<Macro> {
        Some(match name {
            "__timeFrom" => Macro::TimeFrom,
            "__timeTo" => Macro::TimeTo,
            "__from" => Macro::From,
            "__to" => Macro::To,
            "__interval_ms" => Macro::IntervalMs,
            "__interval_s" => Macro::IntervalS,
            _ => return None,
        })
    }

    /// Gives the name with the `$`.
    pub fn name(self) -> &'static str {
        match self {
            Macro::TimeFrom => "$__timeFrom",
            Macro::TimeTo => "$__timeTo",
            Macro::From => "$__from",
            Macro::To => "$__to",
            Macro::IntervalMs => "$__interval_ms",
            Macro::IntervalS => "$__interval_s",
        }
    }

    /// Tells if the macro needs the interval of 7.4. A variable query has no interval.
    pub fn needs_interval(self) -> bool {
        matches!(self, Macro::IntervalMs | Macro::IntervalS)
    }
}

/// The format of a whole-leaf placeholder (7.34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WholeFormat {
    /// No format, or `text`: a string.
    Text,
    /// `number`: an Int32, an Int64 or a Double.
    Number,
    /// `date`: a Date.
    Date,
    /// `oid`: an ObjectId.
    Oid,
    /// `bool`: a boolean.
    Bool,
    /// `json`: an array of strings.
    Json,
    /// `json_number`: an array of numbers.
    JsonNumber,
    /// `json_oid`: an array of ObjectIds.
    JsonOid,
}

impl WholeFormat {
    fn parse(name: Option<&str>) -> Option<WholeFormat> {
        Some(match name {
            None | Some("text") => WholeFormat::Text,
            Some("number") => WholeFormat::Number,
            Some("date") => WholeFormat::Date,
            Some("oid") => WholeFormat::Oid,
            Some("bool") => WholeFormat::Bool,
            Some("json") => WholeFormat::Json,
            Some("json_number") => WholeFormat::JsonNumber,
            Some("json_oid") => WholeFormat::JsonOid,
            _ => return None,
        })
    }

    /// Tells if the format gives an array.
    pub fn is_list(self) -> bool {
        matches!(self, WholeFormat::Json | WholeFormat::JsonNumber | WholeFormat::JsonOid)
    }
}

/// The format of an embedded placeholder (7.34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbFormat {
    /// No format.
    None,
    /// `raw`.
    Raw,
    /// `csv`.
    Csv,
    /// A format of 7.9 that is valid in a string.
    Std(Format),
}

/// A part of an embedded string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part<'s> {
    /// Text of the author.
    Text(&'s str),
    /// A placeholder.
    Var {
        /// The name of the variable.
        name: &'s str,
        /// The format.
        format: EmbFormat,
        /// True when a selection of the variable can be a list.
        may_list: bool,
    },
}

/// The fault of a string, as the parts of a [`Fault`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bad {
    /// The item.
    pub item: Item,
    /// The code.
    pub code: ErrorCode,
    /// The message.
    pub message: String,
}

fn bad(item: Item, code: ErrorCode, message: impl Into<String>) -> Class<'static> {
    Class::Bad(Bad { item, code, message: message.into() })
}

/// What a string value is (7.34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class<'s> {
    /// Text with no placeholder and no `$` at the start.
    Text,
    /// A field path: one `$` and valid steps.
    Path,
    /// A variable reference: `$$` and a permitted name.
    VarRef,
    /// Exactly one macro.
    Macro(Macro),
    /// Exactly one placeholder.
    Whole {
        /// The name of the variable.
        name: &'s str,
        /// The format.
        format: WholeFormat,
    },
    /// Text with one placeholder or more. The string does not start with `$`.
    Embedded(Vec<Part<'s>>),
    /// A fault.
    Bad(Bad),
}

/// A warning that [`classify`] finds.
pub type ClassWarning = (WarnKind, String);

struct Found<'s> {
    start: usize,
    end: usize,
    name: &'s str,
    format: Option<&'s str>,
    braced: bool,
    /// `Some(may_list)` for a variable of the document. `None` for a macro candidate (`__`).
    variable: Option<bool>,
}

/// Finds the placeholders of 7.6 in `s[from..]`: the variables of the document and the `$__name` forms.
/// An unbraced name takes the longest run of name bytes. A `${` that does not match 7.6 stays text. A
/// `${name}` whose name is not a variable gives an `undeclared` entry (warning 1).
fn scan<'s>(
    s: &'s str,
    from: usize,
    vars: &dyn VariableNames,
    macros_on: bool,
    undeclared: &mut Vec<String>,
) -> Vec<Found<'s>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut p = from;
    while p < b.len() {
        if b[p] != b'$' {
            p += 1;
            continue;
        }
        let (name_start, braced) = if b.get(p + 1) == Some(&b'{') { (p + 2, true) } else { (p + 1, false) };
        let mut n = name_start;
        while n < b.len() && is_name_byte(b[n]) {
            n += 1;
        }
        if n == name_start {
            p += if braced { 2 } else { 1 };
            continue;
        }
        let name = &s[name_start..n];
        let (end, format) = if braced {
            match b.get(n) {
                Some(b'}') => (n + 1, None),
                Some(b':') => {
                    let f0 = n + 1;
                    let mut f1 = f0;
                    while f1 < b.len() && is_name_byte(b[f1]) {
                        f1 += 1;
                    }
                    if f1 == f0 || b.get(f1) != Some(&b'}') {
                        p += 2;
                        continue;
                    }
                    (f1 + 1, Some(&s[f0..f1]))
                }
                _ => {
                    p += 2;
                    continue;
                }
            }
        } else {
            (n, None)
        };
        if name.starts_with("__") {
            if macros_on {
                out.push(Found { start: p, end, name, format, braced, variable: None });
            }
        } else if let Some(may_list) = vars.lookup(name) {
            out.push(Found { start: p, end, name, format, braced, variable: Some(may_list) });
        } else if braced {
            undeclared.push(s[p..end].to_owned());
        }
        p = end.max(p + 1);
    }
    out
}

/// Gives the fault of the steps of a field path or a field name (G2), or `None`. The text has no `$` at
/// the start. A `.` separates the steps. A step is not empty and holds no `$`.
fn name_fault(text: &str) -> Option<String> {
    if text.is_empty() || text.len() > MAX_QUERY_KEY_BYTES {
        return Some("a field name has 1 to 256 bytes".to_owned());
    }
    if text.chars().any(|c| c == '$' || c == '\0' || c.is_control()) {
        return Some("a field name holds no $, no U+0000 and no control character".to_owned());
    }
    if text.split('.').any(str::is_empty) {
        return Some("a step of a field path is not empty".to_owned());
    }
    None
}

fn var_ref_class(s: &str) -> Class<'static> {
    let rest = &s[2..];
    let (name, steps) = match rest.split_once('.') {
        Some((n, r)) => (n, Some(r)),
        None => (rest, None),
    };
    let lower = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
    if !(matches!(name, "ROOT" | "CURRENT" | "REMOVE") || lower) {
        let full = format!("$${name}");
        return match refusal_reason(&full) {
            Some(_) => bad(Item::L3, ErrorCode::NotPermitted, refusal_message(&full)),
            None => bad(Item::L3, ErrorCode::NotPermitted, format!("$${name} is not a variable that Hune permits")),
        };
    }
    if let Some(steps) = steps
        && let Some(f) = name_fault(steps)
    {
        return bad(Item::L4, ErrorCode::BadFormat, format!("a variable reference: {f}"));
    }
    Class::VarRef
}

fn emb_format(name: Option<&str>) -> Result<EmbFormat, String> {
    let Some(name) = name else { return Ok(EmbFormat::None) };
    match name {
        "raw" => Ok(EmbFormat::Raw),
        "csv" => Ok(EmbFormat::Csv),
        "singlequote" | "sqlstring" => Err(format!("the format {name} is not valid in a MongoDB query")),
        "number" | "date" | "oid" | "bool" | "json_number" | "json_oid" => {
            Err(format!("the format {name} is valid only when the string is exactly one placeholder"))
        }
        other => match Format::parse(other) {
            Some(f) if f.literal_only() => Ok(EmbFormat::Std(f)),
            _ => Err(format!("the format {other} is not in the list of formats")),
        },
    }
}

/// Reads one string value (7.34). `vars` names the variables of the document. With `macros_on`, the names
/// `$__x` are macros. The expanded tree has no macro and no variable.
pub fn classify<'s>(s: &'s str, vars: &dyn VariableNames, macros_on: bool, warns: &mut Vec<ClassWarning>) -> Class<'s> {
    if !s.contains('$') && !s.contains("[[") {
        return Class::Text;
    }
    // Warning 2: `[[name]]` with the name of a variable of the document.
    let b = s.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'[' && b[i + 1] == b'[' {
            let mut n = i + 2;
            while n < b.len() && is_name_byte(b[n]) {
                n += 1;
            }
            if n > i + 2
                && b.get(n) == Some(&b']')
                && b.get(n + 1) == Some(&b']')
                && vars.lookup(&s[i + 2..n]).is_some()
            {
                warns.push((
                    WarnKind::DoubleBracket,
                    format!("[[{}]] is not a placeholder: write ${{{}}}", &s[i + 2..n], &s[i + 2..n]),
                ));
            }
        }
        i += 1;
    }
    if !s.contains('$') {
        return Class::Text;
    }
    let mut undeclared = Vec::new();
    if s.starts_with("$$") {
        let found = scan(s, 2, vars, macros_on, &mut undeclared);
        if found.iter().any(|f| f.variable.is_some()) {
            return bad(
                Item::L6,
                ErrorCode::NotPermitted,
                "a placeholder is not permitted in a string that starts with $$ (7.34)",
            );
        }
        return var_ref_class(s);
    }
    let found = scan(s, 0, vars, macros_on, &mut undeclared);
    for u in undeclared {
        warns.push((WarnKind::UnknownVariable, format!("{} is not a variable of the document", shown(&u))));
    }
    let single = found.len() == 1 && found[0].start == 0 && found[0].end == s.len();
    if let Some(after_dollar) = s.strip_prefix('$') {
        if single {
            let f = &found[0];
            match f.variable {
                Some(may_list) => {
                    let Some(format) = WholeFormat::parse(f.format) else {
                        return bad(
                            Item::L7,
                            ErrorCode::BadFormat,
                            format!(
                                "the format {} is not valid when the string is exactly one placeholder",
                                f.format.unwrap_or("")
                            ),
                        );
                    };
                    if may_list && !format.is_list() {
                        return bad(
                            Item::L7,
                            ErrorCode::BadFormat,
                            format!(
                                "{} can hold a list: use the format json, json_number or json_oid",
                                &s[f.start..f.end]
                            ),
                        );
                    }
                    return Class::Whole { name: f.name, format };
                }
                None => {
                    if f.format.is_some() {
                        return bad(Item::L5, ErrorCode::BadFormat, format!("{} takes no format", name_of(f)));
                    }
                    return match Macro::from_name(f.name) {
                        Some(m) => Class::Macro(m),
                        None => bad(Item::L5, ErrorCode::BadFormat, format!("${} is not a macro of Hune", f.name)),
                    };
                }
            }
        }
        if macros_on && (s.starts_with("$__") || s.starts_with("${__")) {
            return bad(
                Item::L5,
                ErrorCode::BadFormat,
                "a string that starts with $__ is exactly one macro, or it is an error (7.34)",
            );
        }
        if found.iter().any(|f| f.variable.is_some()) {
            return bad(
                Item::L6,
                ErrorCode::NotPermitted,
                "a placeholder is not permitted in a stored string that starts with $ (7.34)",
            );
        }
        for f in found.iter().filter(|f| f.variable.is_none()) {
            warns.push((WarnKind::MacroInText, format!("{} stays text in a longer string", shown(&name_of(f)))));
        }
        return match name_fault(after_dollar) {
            Some(m) => bad(Item::L4, ErrorCode::BadFormat, format!("a field path: {m}")),
            None => Class::Path,
        };
    }
    // The string does not start with `$`: embedded placeholders, or text.
    let mut parts = Vec::new();
    let mut at = 0;
    let mut any = false;
    for f in &found {
        match f.variable {
            Some(may_list) => {
                let format = match emb_format(f.format) {
                    Ok(x) => x,
                    Err(m) => return bad(Item::L7, ErrorCode::BadFormat, m),
                };
                if may_list && matches!(format, EmbFormat::None | EmbFormat::Raw) {
                    return bad(
                        Item::L7,
                        ErrorCode::BadFormat,
                        format!("{} can hold a list: use a format such as csv or json", &s[f.start..f.end]),
                    );
                }
                if f.start > at {
                    parts.push(Part::Text(&s[at..f.start]));
                }
                parts.push(Part::Var { name: f.name, format, may_list });
                at = f.end;
                any = true;
            }
            None => {
                warns.push((WarnKind::MacroInText, format!("{} stays text in a longer string", shown(&name_of(f)))))
            }
        }
    }
    if !any {
        return Class::Text;
    }
    if at < s.len() {
        parts.push(Part::Text(&s[at..]));
    }
    Class::Embedded(parts)
}

fn name_of(f: &Found<'_>) -> String {
    if f.braced { format!("${{{}}}", f.name) } else { format!("${}", f.name) }
}

/// Tells if a key holds a placeholder or a macro name (a fault of 7.34: a key holds no `$`).
fn key_placeholder(key: &str, vars: &dyn VariableNames, macros_on: bool) -> bool {
    if !key.contains('$') {
        return false;
    }
    let mut undeclared = Vec::new();
    !scan(key, 0, vars, macros_on, &mut undeclared).is_empty()
}

// ---- The top object (7.29) ----

fn top_fault(item: Item, code: ErrorCode, path: Vec<Seg>, at_key: bool, message: impl Into<String>) -> Fault {
    Fault { item, code, path, at_key, message: message.into() }
}

fn key_seg(k: &str) -> Seg {
    Seg::Key(k.to_owned())
}

/// Checks the shape of the top object (7.29) and makes the [`MongoQuery`]. It does not check the grammar
/// below the members: [`check_top`] does. The values of `skip` and `limit` of a `find` MUST be integers in
/// the text. The shape faults are the items L1, L2 and L3. The list holds at most [`MAX_FAULTS`] faults.
pub fn shape_top(top: &MongoValue) -> Result<MongoQuery, Vec<Fault>> {
    shape_top_limited(top, MAX_FAULTS).map_err(|(faults, _)| faults)
}

/// Runs [`shape_top`] with a list of at most `limit` faults. The error holds the list, and `true` when the
/// object has more faults than the list holds. With `limit` 0, a fault still gives the error.
pub fn shape_top_limited(top: &MongoValue, limit: usize) -> Result<MongoQuery, (Vec<Fault>, bool)> {
    let mut faults = Faults::new(limit);
    let Some(doc) = top.as_document() else {
        faults.push(Fault::top(Item::L2, ErrorCode::BadFormat, "the query text is one JSON object"));
        let more = faults.more();
        return Err((faults.list, more));
    };
    let mut collection: Option<String> = None;
    let mut database: Option<String> = None;
    let mut aggregate: Option<Vec<MongoValue>> = None;
    let mut find: Option<FindSpec> = None;
    for (k, v) in doc {
        let path = vec![key_seg(k)];
        match k.as_str() {
            "collection" => match v {
                MongoValue::String(s) => collection = Some(s.clone()),
                _ => faults.push(top_fault(Item::L2, ErrorCode::BadFormat, path, false, "collection must be a text")),
            },
            "database" => match v {
                MongoValue::String(s) => database = Some(s.clone()),
                _ => faults.push(top_fault(Item::L2, ErrorCode::BadFormat, path, false, "database must be a text")),
            },
            "aggregate" => match v {
                MongoValue::Array(stages) if stages.is_empty() => {
                    faults.push(top_fault(Item::L2, ErrorCode::BadFormat, path, false, "aggregate has 1 to 64 stages"))
                }
                MongoValue::Array(stages) if stages.len() > MAX_QUERY_STAGES => {
                    faults.push(top_fault(Item::L1, ErrorCode::TooLarge, path, false, "aggregate has 1 to 64 stages"))
                }
                MongoValue::Array(stages) => aggregate = Some(stages.clone()),
                _ => faults.push(top_fault(Item::L2, ErrorCode::BadFormat, path, false, "aggregate must be an array")),
            },
            "find" => find = shape_find(v, &mut faults).or(find),
            other => faults.push(top_fault(Item::L3, ErrorCode::NotPermitted, path, true, refusal_message(other))),
        }
    }
    if collection.is_none() {
        faults.push(Fault::top(Item::L2, ErrorCode::BadFormat, "the member collection is necessary"));
    }
    let body_count =
        usize::from(doc.iter().any(|(k, _)| k == "aggregate")) + usize::from(doc.iter().any(|(k, _)| k == "find"));
    if body_count != 1 {
        faults.push(Fault::top(Item::L2, ErrorCode::BadFormat, "the query has exactly one of aggregate and find"));
    }
    if faults.found > 0 {
        let more = faults.more();
        return Err((faults.list, more));
    }
    let body = match (aggregate, find) {
        (Some(stages), None) => QueryBody::Aggregate(stages),
        (None, Some(spec)) => QueryBody::Find(spec),
        _ => {
            faults.push(Fault::top(Item::L2, ErrorCode::BadFormat, "the query has exactly one of aggregate and find"));
            let more = faults.more();
            return Err((faults.list, more));
        }
    };
    Ok(MongoQuery { collection: collection.unwrap_or_default(), database, body })
}

fn shape_find(v: &MongoValue, faults: &mut Faults) -> Option<FindSpec> {
    let Some(doc) = v.as_document() else {
        faults.push(top_fault(Item::L2, ErrorCode::BadFormat, vec![key_seg("find")], false, "find must be an object"));
        return None;
    };
    let mut spec = FindSpec::default();
    let before = faults.found;
    for (k, val) in doc {
        let path = vec![key_seg("find"), key_seg(k)];
        let need_doc = |slot: &mut Option<Document>, faults: &mut Faults| match val {
            MongoValue::Document(d) => *slot = Some(d.clone()),
            _ => faults.push(top_fault(
                Item::L2,
                ErrorCode::BadFormat,
                path.clone(),
                false,
                format!("{k} must be an object"),
            )),
        };
        match k.as_str() {
            "filter" => {
                let mut slot = None;
                need_doc(&mut slot, faults);
                spec.filter = slot.unwrap_or_default();
            }
            "projection" => need_doc(&mut spec.projection, faults),
            "sort" => need_doc(&mut spec.sort, faults),
            "skip" | "limit" => {
                let (lo, what) = if k == "skip" { (0u64, "skip") } else { (1u64, "limit") };
                let n = match val {
                    MongoValue::Int32(n) => i64::from(*n),
                    MongoValue::Int64(n) => *n,
                    _ => -1,
                };
                if n < lo as i64 || n as u64 > MAX_SKIP_LIMIT {
                    let msg = format!("{what} is an integer from {lo} to 1000000000");
                    faults.push(top_fault(Item::L2, ErrorCode::BadFormat, path, false, msg));
                } else if what == "skip" {
                    spec.skip = n as u64;
                } else {
                    spec.limit = Some(n as u64);
                }
            }
            other => faults.push(top_fault(Item::L3, ErrorCode::NotPermitted, path, true, refusal_message(other))),
        }
    }
    debug_assert!(FIND_MEMBERS.len() == 5);
    (faults.found == before).then_some(spec)
}

// ---- The walk ----

/// The phase of a check: the stored tree T0 or the expanded tree T1 (7.35).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The tree of the text. A string can be a placeholder or a macro.
    Stored,
    /// The tree after the expansion. A string is a value.
    Expanded,
}

/// The input of [`check_top`].
pub struct Check<'a> {
    /// The phase.
    pub phase: Phase,
    /// The variables.
    pub vars: &'a dyn VariableNames,
    /// The key `collections` of the source (7.33).
    pub collections: &'a [String],
    /// The key `databases` of the source (7.33).
    pub databases: &'a [String],
    /// The use of the query: a variable query takes no interval macro (L9).
    pub usage: SqlUse,
}

#[derive(Clone, Copy)]
enum Pr<'a> {
    K(&'a str),
    I(usize),
}

/// What a string leaf is, for the callers that need a type at the place.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leaf {
    Text,
    Path,
    VarRef,
    Whole,
    Macro(Macro),
    Embedded,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Value,
    Name,
}

struct Walker<'a, 'c> {
    check: &'c Check<'c>,
    path: Vec<Pr<'a>>,
    faults: Vec<Fault>,
    warnings: Vec<Warning>,
    fault_limit: usize,
    warning_limit: usize,
    more_faults: bool,
    more_warnings: bool,
    stages: usize,
    stage_limit_reported: bool,
}

/// Checks the grammar of the whole tree (7.29 to 7.33). `top` is the top object that [`shape_top`]
/// accepted, in the form of the text. The path of each fault is relative to it.
///
/// The report holds at most [`MAX_FAULTS`] faults and `MAX_WARNINGS` minus 1 warnings (4.35), the first ones
/// of the walk.
pub fn check_top(top: &MongoValue, check: &Check<'_>) -> Report {
    check_top_limited(top, check, MAX_FAULTS, MAX_WALK_WARNINGS)
}

/// Runs [`check_top`] with a list of at most `fault_limit` faults and `warning_limit` warnings. The walk goes
/// on past a full list, so the flags of the report tell if the tree has more. A full list makes no item.
pub fn check_top_limited(top: &MongoValue, check: &Check<'_>, fault_limit: usize, warning_limit: usize) -> Report {
    let mut w = Walker {
        check,
        path: Vec::new(),
        faults: Vec::new(),
        warnings: Vec::new(),
        fault_limit,
        warning_limit,
        more_faults: false,
        more_warnings: false,
        stages: 0,
        stage_limit_reported: false,
    };
    w.top(top);
    Report { faults: w.faults, warnings: w.warnings, more_faults: w.more_faults, more_warnings: w.more_warnings }
}

impl<'a> Walker<'a, '_> {
    fn owned_path(&self) -> Vec<Seg> {
        self.path
            .iter()
            .map(|p| match p {
                Pr::K(k) => Seg::Key((*k).to_owned()),
                Pr::I(i) => Seg::Index(*i),
            })
            .collect()
    }

    fn fault(&mut self, item: Item, code: ErrorCode, at_key: bool, message: impl Into<String>) {
        if self.faults.len() >= self.fault_limit {
            self.more_faults = true;
            return;
        }
        let path = self.owned_path();
        self.faults.push(Fault { item, code, path, at_key, message: message.into() });
    }

    fn bad_format(&mut self, message: impl Into<String>) {
        self.fault(Item::L4, ErrorCode::BadFormat, false, message);
    }

    fn refuse(&mut self, name: &str) {
        self.fault(Item::L3, ErrorCode::NotPermitted, true, refusal_message(name));
    }

    fn macros_on(&self) -> bool {
        self.check.phase == Phase::Stored
    }

    // ---- keys ----

    /// Checks a key that is not an operator name (G2). It gives false when the key has a fault.
    fn field_key(&mut self, k: &str) -> bool {
        if key_placeholder(k, self.check.vars, self.macros_on()) {
            self.fault(Item::L6, ErrorCode::NotPermitted, true, "a placeholder is not permitted in a key (7.34)");
            return false;
        }
        if let Some(m) = name_fault(k) {
            self.fault(Item::L4, ErrorCode::BadFormat, true, m);
            return false;
        }
        true
    }

    /// Checks the key of an operator or a stage: a placeholder is a fault. It gives false on a fault.
    fn op_key(&mut self, k: &str, table_ok: bool) -> bool {
        if key_placeholder(k, self.check.vars, self.macros_on()) {
            self.fault(Item::L6, ErrorCode::NotPermitted, true, "a placeholder is not permitted in a key (7.34)");
            return false;
        }
        if !table_ok {
            self.refuse(k);
            return false;
        }
        true
    }

    // ---- strings ----

    fn string(&mut self, s: &str, place: Place) -> Leaf {
        let mut warns = Vec::new();
        let class = classify(s, self.check.vars, self.macros_on(), &mut warns);
        if self.check.phase == Phase::Stored {
            for (kind, message) in warns {
                if self.warnings.len() >= self.warning_limit {
                    self.more_warnings = true;
                    break;
                }
                let path = self.owned_path();
                self.warnings.push(Warning { kind, path, message });
            }
        }
        let leaf = match class {
            Class::Text => Leaf::Text,
            Class::Path => Leaf::Path,
            Class::VarRef => Leaf::VarRef,
            Class::Whole { .. } => Leaf::Whole,
            Class::Embedded(_) => Leaf::Embedded,
            Class::Macro(m) => {
                if m.needs_interval() && self.check.usage != SqlUse::Panel {
                    // An annotation has no interval, as a variable (7.36, L9).
                    let owner = if self.check.usage == SqlUse::Annotation { "an annotation" } else { "a variable" };
                    self.fault(
                        Item::L9,
                        ErrorCode::NotPermitted,
                        false,
                        format!("{} is not permitted in the query of {owner}", m.name()),
                    );
                    return Leaf::Failed;
                }
                Leaf::Macro(m)
            }
            Class::Bad(b) => {
                self.fault(b.item, b.code, false, b.message);
                return Leaf::Failed;
            }
        };
        if place == Place::Name && matches!(leaf, Leaf::Whole | Leaf::Macro(_) | Leaf::Embedded) {
            self.fault(
                Item::L6,
                ErrorCode::NotPermitted,
                false,
                "a placeholder or a macro is not permitted in a place that names an object of the server (7.34)",
            );
            return Leaf::Failed;
        }
        leaf
    }

    /// A name place that holds a field name or a dotted path with no `$`.
    fn name_field(&mut self, v: &'a MongoValue) {
        let MongoValue::String(s) = v else {
            self.bad_format("a name is a text");
            return;
        };
        match self.string(s, Place::Name) {
            Leaf::Failed => {}
            Leaf::Text => {
                if let Some(m) = name_fault(s) {
                    self.bad_format(m);
                }
            }
            _ => self.bad_format("a field name has no $ at the start"),
        }
    }

    /// A name place that holds a field path with one `$` (the path of `$unwind`).
    fn name_path(&mut self, v: &'a MongoValue) {
        let MongoValue::String(s) = v else {
            self.bad_format("a field path is a text");
            return;
        };
        match self.string(s, Place::Name) {
            Leaf::Failed | Leaf::Path => {}
            _ => self.bad_format("a field path starts with one $"),
        }
    }

    /// A name place that holds a collection name of the list `collections` (7.33).
    fn collection_name(&mut self, v: &'a MongoValue) {
        if matches!(v, MongoValue::Document(_)) {
            self.fault(
                Item::L3,
                ErrorCode::NotPermitted,
                false,
                "a collection name is never an object: another database is not permitted (7.33)",
            );
            return;
        }
        let MongoValue::String(s) = v else {
            self.bad_format("a collection name is a text");
            return;
        };
        match self.string(s, Place::Name) {
            Leaf::Failed => {}
            Leaf::Text if self.check.collections.iter().any(|c| c == s) => {}
            _ => self.fault(
                Item::L8,
                ErrorCode::BadReference,
                false,
                "the collection is not in the key collections of the source (7.33)",
            ),
        }
    }

    fn database_name(&mut self, v: &'a MongoValue) {
        let MongoValue::String(s) = v else {
            self.bad_format("a database name is a text");
            return;
        };
        match self.string(s, Place::Name) {
            Leaf::Failed => {}
            Leaf::Text if self.check.databases.iter().any(|d| d == s) => {}
            _ => self.fault(
                Item::L8,
                ErrorCode::BadReference,
                false,
                "the database is not in the key databases of the source (7.33)",
            ),
        }
    }

    // ---- leaves with a type ----

    /// An integer in `lo..=hi`, or a whole-leaf placeholder or a macro (the type is known after the expansion).
    fn int_leaf(&mut self, v: &'a MongoValue, lo: i64, hi: i64, what: &str) {
        let n = match v {
            MongoValue::Int32(n) => Some(i64::from(*n)),
            MongoValue::Int64(n) => Some(*n),
            MongoValue::String(s) => {
                match self.string(s, Place::Value) {
                    Leaf::Whole | Leaf::Failed => {}
                    // The interval macros are Int64 values from 1 to 86,400,000 (7.4). `$__from` and `$__to` are
                    // Int64 values that no range of 7.29 holds, so they are never an integer of a small range.
                    Leaf::Macro(Macro::IntervalMs | Macro::IntervalS) if lo <= 1 && hi >= 86_400_000 => {}
                    _ => self.bad_format(format!("{what} is an integer from {lo} to {hi}")),
                }
                return;
            }
            _ => None,
        };
        match n {
            Some(n) if (lo..=hi).contains(&n) => {}
            _ => self.bad_format(format!("{what} is an integer from {lo} to {hi}")),
        }
    }

    /// A boolean or a placeholder.
    fn bool_leaf(&mut self, v: &'a MongoValue, what: &str) {
        match v {
            MongoValue::Bool(_) => {}
            MongoValue::String(s) => match self.string(s, Place::Value) {
                Leaf::Whole | Leaf::Failed => {}
                _ => self.bad_format(format!("{what} is a boolean")),
            },
            _ => self.bad_format(format!("{what} is a boolean")),
        }
    }

    /// A number (or a typed leaf) in a place that takes a number.
    fn scalar_leaf(&mut self, v: &'a MongoValue, what: &str) {
        match v {
            MongoValue::Array(_) | MongoValue::Document(_) => self.bad_format(format!("{what} is one value")),
            MongoValue::String(s) => {
                self.string(s, Place::Value);
            }
            _ => {}
        }
    }

    /// A regular expression pattern. A pattern of the text above `MAX_REGEX_BYTES` is a fault.
    fn regex_pattern(&mut self, v: &'a MongoValue) {
        let MongoValue::String(s) = v else {
            self.bad_format("a regular expression is a text");
            return;
        };
        let leaf = self.string(s, Place::Value);
        if matches!(leaf, Leaf::Macro(_)) {
            self.bad_format("a regular expression is a text: a macro is not a text");
        }
        if matches!(leaf, Leaf::Text | Leaf::Path | Leaf::VarRef) && s.len() > MAX_REGEX_BYTES {
            self.fault(Item::L1, ErrorCode::TooLarge, false, "a regular expression pattern has 1024 bytes at most");
        }
    }

    /// The options of a regular expression: the letters `i`, `m`, `s` and `x` only.
    fn regex_options(&mut self, v: &'a MongoValue) {
        let MongoValue::String(s) = v else {
            self.bad_format("the options of a regular expression are a text");
            return;
        };
        match self.string(s, Place::Value) {
            Leaf::Text if s.chars().all(|c| matches!(c, 'i' | 'm' | 's' | 'x')) => {}
            Leaf::Text => self.bad_format("the options of a regular expression are the letters i, m, s and x only"),
            Leaf::Embedded | Leaf::Whole | Leaf::Failed => {}
            _ => self.bad_format("the options of a regular expression are the letters i, m, s and x only"),
        }
    }

    // ---- the top object ----

    fn top(&mut self, top: &'a MongoValue) {
        let Some(doc) = top.as_document() else { return };
        for (k, v) in doc {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "collection" => self.collection_name(v),
                "database" => self.database_name(v),
                "aggregate" => {
                    if let MongoValue::Array(stages) = v {
                        self.pipeline(stages, 0);
                    }
                }
                "find" => self.find(v),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn find(&mut self, v: &'a MongoValue) {
        let Some(doc) = v.as_document() else { return };
        for (k, val) in doc {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "filter" => self.q_doc(val, false),
                "projection" => self.projection(val),
                "sort" => self.sort_spec(val),
                "skip" => self.int_leaf(val, 0, MAX_SKIP_LIMIT as i64, "skip"),
                "limit" => self.int_leaf(val, 1, MAX_SKIP_LIMIT as i64, "limit"),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn projection(&mut self, v: &'a MongoValue) {
        let Some(doc) = v.as_document() else {
            self.bad_format("a projection is an object");
            return;
        };
        let (mut include, mut exclude) = (false, false);
        for (k, val) in doc {
            self.path.push(Pr::K(k));
            self.field_key(k);
            let flag = match val {
                MongoValue::Bool(b) => Some(*b),
                MongoValue::Int32(0) | MongoValue::Int64(0) => Some(false),
                MongoValue::Int32(1) | MongoValue::Int64(1) => Some(true),
                MongoValue::String(s) => {
                    match self.string(s, Place::Value) {
                        Leaf::Whole | Leaf::Failed => {}
                        _ => self.bad_format("a value of a projection is 0, 1, true or false"),
                    }
                    None
                }
                _ => {
                    self.bad_format("a value of a projection is 0, 1, true or false");
                    None
                }
            };
            if k != "_id" {
                match flag {
                    Some(true) => include = true,
                    Some(false) => exclude = true,
                    None => {}
                }
            }
            self.path.pop();
        }
        if include && exclude {
            self.bad_format("a projection does not mix inclusion and exclusion, except for _id");
        }
    }

    /// A sort object: 1 to 32 keys, each value 1 or -1.
    fn sort_spec(&mut self, v: &'a MongoValue) {
        let Some(doc) = v.as_document() else {
            self.bad_format("a sort is an object");
            return;
        };
        if doc.is_empty() || doc.len() > MAX_SORT_KEYS {
            self.bad_format("a sort has 1 to 32 keys");
        }
        for (k, val) in doc {
            self.path.push(Pr::K(k));
            self.field_key(k);
            let ok = match val {
                MongoValue::Int32(n) => matches!(n, 1 | -1),
                MongoValue::Int64(n) => matches!(n, 1 | -1),
                MongoValue::Double(f) => *f == 1.0 || *f == -1.0,
                MongoValue::String(s) => {
                    matches!(self.string(s, Place::Value), Leaf::Whole | Leaf::Failed)
                }
                _ => false,
            };
            if !ok {
                self.bad_format("a value of a sort is 1 or -1");
            }
            self.path.pop();
        }
    }

    // ---- pipelines and stages ----

    fn pipeline(&mut self, stages: &'a [MongoValue], level: usize) {
        for (i, st) in stages.iter().enumerate() {
            self.path.push(Pr::I(i));
            self.stage(st, level);
            self.path.pop();
        }
    }

    /// A nested pipeline: an array of stages, one level deeper (G9).
    fn nested_pipeline(&mut self, v: &'a MongoValue, level: usize) {
        let MongoValue::Array(stages) = v else {
            self.bad_format("a pipeline is an array of stages");
            return;
        };
        if level + 1 > MAX_PIPELINE_DEPTH {
            self.fault(Item::L1, ErrorCode::TooDeep, false, "a query has 3 levels of nested pipelines at most");
            return;
        }
        if stages.is_empty() {
            self.bad_format("a pipeline has 1 to 64 stages");
            return;
        }
        self.pipeline(stages, level + 1);
    }

    fn count_stage(&mut self) {
        self.stages += 1;
        if self.stages > MAX_QUERY_STAGES && !self.stage_limit_reported {
            self.stage_limit_reported = true;
            self.fault(Item::L1, ErrorCode::TooLarge, false, "a query has 64 stages at most, nested stages included");
        }
    }

    /// Checks that the members of an object are in `allowed`. It gives false when a member is not.
    fn members(&mut self, doc: &'a Document, allowed: &[&str], stage: &str) -> bool {
        let mut ok = true;
        for (k, _) in doc {
            if !allowed.contains(&k.as_str()) {
                self.path.push(Pr::K(k));
                self.fault(Item::L4, ErrorCode::BadFormat, true, format!("{k} is not a member of {stage}"));
                self.path.pop();
                ok = false;
            }
        }
        ok
    }

    fn require(&mut self, doc: &Document, names: &[&str], stage: &str) {
        for n in names {
            if !doc.iter().any(|(k, _)| k == n) {
                self.bad_format(format!("{stage} needs the member {n}"));
            }
        }
    }

    fn stage(&mut self, v: &'a MongoValue, level: usize) {
        let Some(doc) = v.as_document() else {
            self.bad_format("a stage is an object with exactly one key");
            return;
        };
        if doc.len() != 1 {
            self.bad_format("a stage is an object with exactly one key");
            return;
        }
        let (name, arg) = (&doc[0].0, &doc[0].1);
        self.path.push(Pr::K(name));
        self.count_stage();
        if self.op_key(name, in_table(STAGES, name)) {
            self.stage_arg(name, arg, level);
        }
        self.path.pop();
    }

    fn stage_arg(&mut self, name: &'a str, arg: &'a MongoValue, level: usize) {
        match name {
            "$match" => self.q_doc(arg, false),
            "$project" | "$addFields" | "$set" => match arg {
                MongoValue::Document(d) => self.expr_members(d),
                _ => self.bad_format(format!("{name} takes an object")),
            },
            "$unset" => match arg {
                MongoValue::String(_) => self.name_field(arg),
                MongoValue::Array(items) if (1..=64).contains(&items.len()) => {
                    for (i, it) in items.iter().enumerate() {
                        self.path.push(Pr::I(i));
                        self.name_field(it);
                        self.path.pop();
                    }
                }
                _ => self.bad_format("$unset takes a field name or an array of 1 to 64 field names"),
            },
            "$group" => self.group(arg),
            "$sort" => self.sort_spec(arg),
            "$limit" => self.int_leaf(arg, 1, MAX_SKIP_LIMIT as i64, "$limit"),
            "$skip" => self.int_leaf(arg, 0, MAX_SKIP_LIMIT as i64, "$skip"),
            "$count" => {
                self.name_field(arg);
                if let MongoValue::String(s) = arg
                    && s.contains('.')
                {
                    self.bad_format("the name of $count has no .");
                }
            }
            "$unwind" => self.unwind(arg),
            "$replaceRoot" => match arg {
                MongoValue::Document(d) => {
                    if self.members(d, &["newRoot"], "$replaceRoot") {
                        self.require(d, &["newRoot"], "$replaceRoot");
                    }
                    for (k, val) in d {
                        if k == "newRoot" {
                            self.path.push(Pr::K(k));
                            match val {
                                MongoValue::String(s) => {
                                    self.string(s, Place::Name);
                                }
                                other => self.expr(other),
                            }
                            self.path.pop();
                        }
                    }
                }
                _ => self.bad_format("$replaceRoot takes an object with the member newRoot"),
            },
            "$replaceWith" | "$sortByCount" => self.expr(arg),
            "$bucket" => self.bucket(arg),
            "$bucketAuto" => self.bucket_auto(arg),
            "$facet" => self.facet(arg, level),
            "$setWindowFields" => self.set_window_fields(arg),
            "$densify" => self.densify(arg),
            "$fill" => self.fill(arg),
            "$lookup" => self.lookup(arg, level),
            "$unionWith" => self.union_with(arg, level),
            _ => {}
        }
    }

    fn group(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$group takes an object with the member _id");
            return;
        };
        self.require(d, &["_id"], "$group");
        if d.len() > 65 {
            self.bad_format("$group has _id and 64 other members at most");
        }
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if k == "_id" {
                self.expr(val);
            } else if self.field_key(k) {
                self.accumulator(val);
            }
            self.path.pop();
        }
    }

    fn accumulators(&mut self, v: &'a MongoValue) {
        let Some(d) = v.as_document() else {
            self.bad_format("output is an object of accumulators");
            return;
        };
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if self.field_key(k) {
                self.accumulator(val);
            }
            self.path.pop();
        }
    }

    /// An accumulator: an object with one operator of the context A.
    fn accumulator(&mut self, v: &'a MongoValue) {
        let Some(d) = v.as_document() else {
            self.bad_format("an accumulator is an object with one operator");
            return;
        };
        if d.len() != 1 {
            self.bad_format("an accumulator is an object with one operator");
            return;
        }
        let (k, arg) = (&d[0].0, &d[0].1);
        self.path.push(Pr::K(k));
        if self.op_key(k, in_table(A_OPS, k)) {
            self.expr(arg);
        }
        self.path.pop();
    }

    fn unwind(&mut self, arg: &'a MongoValue) {
        match arg {
            MongoValue::String(_) => self.name_path(arg),
            MongoValue::Document(d) => {
                self.members(d, &["path", "includeArrayIndex", "preserveNullAndEmptyArrays"], "$unwind");
                self.require(d, &["path"], "$unwind");
                for (k, val) in d {
                    self.path.push(Pr::K(k));
                    match k.as_str() {
                        "path" => self.name_path(val),
                        "includeArrayIndex" => self.name_field(val),
                        "preserveNullAndEmptyArrays" => self.bool_leaf(val, "preserveNullAndEmptyArrays"),
                        _ => {}
                    }
                    self.path.pop();
                }
            }
            _ => self.bad_format("$unwind takes a field path or an object"),
        }
    }

    fn bucket(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$bucket takes an object");
            return;
        };
        self.members(d, &["groupBy", "boundaries", "default", "output"], "$bucket");
        self.require(d, &["groupBy", "boundaries"], "$bucket");
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "groupBy" => self.expr(val),
                "boundaries" => match val {
                    MongoValue::Array(items) if (2..=1000).contains(&items.len()) => {
                        for (i, it) in items.iter().enumerate() {
                            self.path.push(Pr::I(i));
                            self.scalar_leaf(it, "a boundary");
                            self.path.pop();
                        }
                    }
                    MongoValue::String(s) => {
                        if !matches!(self.string(s, Place::Value), Leaf::Whole | Leaf::Failed) {
                            self.bad_format("boundaries is an array of 2 to 1000 values");
                        }
                    }
                    _ => self.bad_format("boundaries is an array of 2 to 1000 values"),
                },
                "default" => self.scalar_leaf(val, "default"),
                "output" => self.accumulators(val),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn bucket_auto(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$bucketAuto takes an object");
            return;
        };
        self.members(d, &["groupBy", "buckets", "output"], "$bucketAuto");
        self.require(d, &["groupBy", "buckets"], "$bucketAuto");
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "groupBy" => self.expr(val),
                "buckets" => self.int_leaf(val, 1, 1000, "buckets"),
                "output" => self.accumulators(val),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn facet(&mut self, arg: &'a MongoValue, level: usize) {
        if level >= 1 {
            self.fault(Item::L3, ErrorCode::NotPermitted, true, "$facet is not permitted in a nested pipeline (G9)");
            return;
        }
        let Some(d) = arg.as_document() else {
            self.bad_format("$facet takes an object of pipelines");
            return;
        };
        if d.is_empty() || d.len() > 16 {
            self.bad_format("$facet has 1 to 16 members");
        }
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if self.field_key(k) {
                self.nested_pipeline(val, level);
            }
            self.path.pop();
        }
    }

    fn set_window_fields(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$setWindowFields takes an object");
            return;
        };
        self.members(d, &["partitionBy", "sortBy", "output"], "$setWindowFields");
        self.require(d, &["output"], "$setWindowFields");
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "partitionBy" => self.expr(val),
                "sortBy" => self.sort_spec(val),
                "output" => self.window_output(val),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn window_output(&mut self, v: &'a MongoValue) {
        let Some(d) = v.as_document() else {
            self.bad_format("output is an object");
            return;
        };
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if self.field_key(k) {
                self.window_member(val);
            }
            self.path.pop();
        }
    }

    /// A member of `output` of `$setWindowFields`: one operator of W and an optional `window`.
    fn window_member(&mut self, v: &'a MongoValue) {
        let Some(d) = v.as_document() else {
            self.bad_format("a member of output is an object with one operator and an optional window");
            return;
        };
        let ops = d.iter().filter(|(k, _)| k != "window").count();
        if ops != 1 {
            self.bad_format("a member of output has exactly one operator, and an optional window");
        }
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if k == "window" || self.op_key(k, is_w_op(k)) {
                self.expr(val);
            }
            self.path.pop();
        }
    }

    fn name_array(&mut self, v: &'a MongoValue, what: &str) {
        match v {
            MongoValue::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    self.name_field(it);
                    self.path.pop();
                }
            }
            _ => self.bad_format(format!("{what} is an array of field names")),
        }
    }

    fn densify(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$densify takes an object");
            return;
        };
        self.members(d, &["field", "partitionByFields", "range"], "$densify");
        self.require(d, &["field", "range"], "$densify");
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "field" => self.name_field(val),
                "partitionByFields" => self.name_array(val, "partitionByFields"),
                "range" => self.expr(val),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn fill(&mut self, arg: &'a MongoValue) {
        let Some(d) = arg.as_document() else {
            self.bad_format("$fill takes an object");
            return;
        };
        self.members(d, &["partitionBy", "partitionByFields", "sortBy", "output"], "$fill");
        self.require(d, &["output"], "$fill");
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "partitionBy" => self.expr(val),
                "partitionByFields" => self.name_array(val, "partitionByFields"),
                "sortBy" => self.sort_spec(val),
                "output" => match val {
                    MongoValue::Document(o) => self.expr_members(o),
                    _ => self.bad_format("output is an object"),
                },
                _ => {}
            }
            self.path.pop();
        }
    }

    fn lookup(&mut self, arg: &'a MongoValue, level: usize) {
        if level >= MAX_PIPELINE_DEPTH {
            self.fault(
                Item::L3,
                ErrorCode::NotPermitted,
                true,
                "$lookup is not permitted at the third level of nested pipelines (G9)",
            );
            return;
        }
        let Some(d) = arg.as_document() else {
            self.bad_format("$lookup takes an object");
            return;
        };
        let has = |n: &str| d.iter().any(|(k, _)| k == n);
        let form2 = has("pipeline") || has("let");
        if form2 {
            self.members(d, &["from", "let", "pipeline", "as"], "$lookup");
            self.require(d, &["pipeline", "as"], "$lookup");
        } else {
            self.members(d, &["from", "localField", "foreignField", "as"], "$lookup");
            self.require(d, &["localField", "foreignField", "as"], "$lookup");
        }
        if !has("from") {
            self.fault(
                Item::L3,
                ErrorCode::NotPermitted,
                false,
                "$lookup with no from is not permitted: the query leaves the allowlist (7.33)",
            );
        }
        for (k, val) in d {
            self.path.push(Pr::K(k));
            match k.as_str() {
                "from" => match val {
                    MongoValue::Document(_) => self.fault(
                        Item::L3,
                        ErrorCode::NotPermitted,
                        false,
                        "from is never an object: another database is not permitted (7.33)",
                    ),
                    _ => self.collection_name(val),
                },
                "as" | "localField" | "foreignField" => self.name_field(val),
                "let" => match val {
                    MongoValue::Document(o) => self.expr_members(o),
                    _ => self.bad_format("let is an object"),
                },
                "pipeline" => self.nested_pipeline(val, level),
                _ => {}
            }
            self.path.pop();
        }
    }

    fn union_with(&mut self, arg: &'a MongoValue, level: usize) {
        match arg {
            MongoValue::String(_) => self.collection_name(arg),
            MongoValue::Document(d) => {
                self.members(d, &["coll", "pipeline"], "$unionWith");
                self.require(d, &["coll"], "$unionWith");
                for (k, val) in d {
                    self.path.push(Pr::K(k));
                    match k.as_str() {
                        "coll" => self.collection_name(val),
                        "pipeline" => self.nested_pipeline(val, level),
                        _ => {}
                    }
                    self.path.pop();
                }
            }
            _ => self.bad_format("$unionWith takes a collection name or an object"),
        }
    }

    // ---- the context Q ----

    /// A filter object. In `$elemMatch`, the operators of a field are also keys.
    fn q_doc(&mut self, v: &'a MongoValue, in_elem: bool) {
        let Some(doc) = v.as_document() else {
            self.bad_format("a filter is an object");
            return;
        };
        for (k, val) in doc {
            self.path.push(Pr::K(k));
            if k.starts_with('$') {
                let table_ok = in_table(Q_LOGIC_KEYS, k) || (in_elem && in_table(Q_FIELD_OPS, k));
                if self.op_key(k, table_ok) {
                    match k.as_str() {
                        "$and" | "$or" | "$nor" => self.q_logic(val),
                        "$expr" => self.expr(val),
                        _ => self.q_op(k, val),
                    }
                }
            } else if self.field_key(k) {
                self.q_field_value(val);
            }
            self.path.pop();
        }
    }

    fn q_logic(&mut self, v: &'a MongoValue) {
        match v {
            MongoValue::Array(items) if (1..=32).contains(&items.len()) => {
                for (i, it) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    self.q_doc(it, false);
                    self.path.pop();
                }
            }
            _ => self.bad_format("$and, $or and $nor take an array of 1 to 32 filters"),
        }
    }

    /// The value of a field in a filter: a literal, or an object of operators of Q.
    fn q_field_value(&mut self, v: &'a MongoValue) {
        match v {
            MongoValue::Document(d) if d.iter().any(|(k, _)| k.starts_with('$')) => {
                if d.iter().any(|(k, _)| !k.starts_with('$')) {
                    self.bad_format("an object that has a key with $ has only keys with $ (G3, G4)");
                    return;
                }
                for (k, val) in d {
                    self.path.push(Pr::K(k));
                    if self.op_key(k, in_table(Q_FIELD_OPS, k)) {
                        self.q_op(k, val);
                    }
                    self.path.pop();
                }
            }
            other => self.q_lit(other),
        }
    }

    fn q_op(&mut self, name: &'a str, arg: &'a MongoValue) {
        match name {
            "$eq" | "$ne" | "$gt" | "$gte" | "$lt" | "$lte" => self.q_lit(arg),
            "$in" | "$nin" => self.q_array(arg, name),
            "$all" => match arg {
                MongoValue::Array(items) => {
                    for (i, it) in items.iter().enumerate() {
                        self.path.push(Pr::I(i));
                        match it {
                            MongoValue::Document(d) if d.len() == 1 && d[0].0 == "$elemMatch" => {
                                self.path.push(Pr::K("$elemMatch"));
                                self.q_doc(&d[0].1, true);
                                self.path.pop();
                            }
                            other => self.q_lit(other),
                        }
                        self.path.pop();
                    }
                }
                other => self.q_array(other, name),
            },
            "$not" => match arg {
                MongoValue::Document(d) if !d.is_empty() => {
                    for (k, val) in d {
                        self.path.push(Pr::K(k));
                        if self.op_key(k, in_table(Q_FIELD_OPS, k)) {
                            self.q_op(k, val);
                        }
                        self.path.pop();
                    }
                }
                _ => self.bad_format("$not takes an object of operators"),
            },
            "$exists" => match arg {
                MongoValue::Array(_) | MongoValue::Document(_) | MongoValue::Null => {
                    self.bad_format("$exists takes a boolean or a number")
                }
                other => self.scalar_leaf(other, "$exists"),
            },
            "$type" => match arg {
                MongoValue::Array(items) => {
                    for (i, it) in items.iter().enumerate() {
                        self.path.push(Pr::I(i));
                        self.scalar_leaf(it, "$type");
                        self.path.pop();
                    }
                }
                other => self.scalar_leaf(other, "$type"),
            },
            "$size" => self.int_leaf(arg, 0, i64::from(i32::MAX), "$size"),
            "$elemMatch" => self.q_doc(arg, true),
            "$mod" => match arg {
                MongoValue::Array(items) if items.len() == 2 => {
                    for (i, it) in items.iter().enumerate() {
                        self.path.push(Pr::I(i));
                        self.scalar_leaf(it, "$mod");
                        self.path.pop();
                    }
                }
                MongoValue::String(_) => self.q_array(arg, name),
                _ => self.bad_format("$mod takes an array of 2 numbers"),
            },
            "$regex" => self.regex_pattern(arg),
            "$options" => self.regex_options(arg),
            _ => {}
        }
    }

    /// An array of literals, or a whole-leaf placeholder that gives an array.
    fn q_array(&mut self, v: &'a MongoValue, name: &str) {
        match v {
            MongoValue::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    self.q_lit(it);
                    self.path.pop();
                }
            }
            MongoValue::String(s) => {
                if !matches!(self.string(s, Place::Value), Leaf::Whole | Leaf::Failed) {
                    self.bad_format(format!("{name} takes an array"));
                }
            }
            _ => self.bad_format(format!("{name} takes an array")),
        }
    }

    /// A literal: a value with no key that starts with `$` at any depth (G4).
    fn q_lit(&mut self, v: &'a MongoValue) {
        match v {
            MongoValue::Document(d) => {
                for (k, val) in d {
                    self.path.push(Pr::K(k));
                    if k.starts_with('$') {
                        if key_placeholder(k, self.check.vars, self.macros_on()) {
                            self.fault(
                                Item::L6,
                                ErrorCode::NotPermitted,
                                true,
                                "a placeholder is not permitted in a key (7.34)",
                            );
                        } else {
                            self.fault(
                                Item::L3,
                                ErrorCode::NotPermitted,
                                true,
                                format!(
                                    "{k} is not permitted in a literal object: a literal has no key that starts with $"
                                ),
                            );
                        }
                    } else if self.field_key(k) {
                        self.q_lit(val);
                    }
                    self.path.pop();
                }
            }
            MongoValue::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    self.q_lit(it);
                    self.path.pop();
                }
            }
            MongoValue::String(s) => {
                self.string(s, Place::Value);
            }
            _ => {}
        }
    }

    // ---- the context E ----

    /// The members of an object whose values are expressions (the keys are field names).
    fn expr_members(&mut self, d: &'a Document) {
        for (k, val) in d {
            self.path.push(Pr::K(k));
            if self.field_key(k) {
                self.expr(val);
            }
            self.path.pop();
        }
    }

    fn expr(&mut self, v: &'a MongoValue) {
        match v {
            MongoValue::String(s) => {
                self.string(s, Place::Value);
            }
            MongoValue::Array(items) => {
                for (i, it) in items.iter().enumerate() {
                    self.path.push(Pr::I(i));
                    self.expr(it);
                    self.path.pop();
                }
            }
            MongoValue::Document(d) => {
                let dollars = d.iter().filter(|(k, _)| k.starts_with('$')).count();
                if dollars == 0 {
                    self.expr_members(d);
                } else if d.len() != 1 {
                    self.bad_format("an object with a key that starts with $ has exactly one key (G3)");
                } else {
                    let (k, arg) = (&d[0].0, &d[0].1);
                    self.path.push(Pr::K(k));
                    if self.op_key(k, is_e_op(k)) {
                        self.e_operator(k, arg);
                    }
                    self.path.pop();
                }
            }
            _ => {}
        }
    }

    /// The argument of an operator of E. Most arguments are expressions. A few have a name place.
    fn e_operator(&mut self, name: &'a str, arg: &'a MongoValue) {
        match name {
            "$literal" => self.q_lit(arg),
            "$getField" | "$setField" | "$unsetField" => match arg {
                MongoValue::String(_) => self.name_field(arg),
                MongoValue::Document(d) if d.iter().all(|(k, _)| !k.starts_with('$')) => {
                    for (k, val) in d {
                        self.path.push(Pr::K(k));
                        if k == "field" {
                            self.name_field(val);
                        } else if self.field_key(k) {
                            self.expr(val);
                        }
                        self.path.pop();
                    }
                }
                _ => {
                    self.bad_format(format!("{name} takes a field name or an object with the members field and input"))
                }
            },
            "$map" | "$filter" => match arg {
                MongoValue::Document(d) if d.iter().all(|(k, _)| !k.starts_with('$')) => {
                    for (k, val) in d {
                        self.path.push(Pr::K(k));
                        if k == "as" {
                            self.name_field(val);
                        } else if self.field_key(k) {
                            self.expr(val);
                        }
                        self.path.pop();
                    }
                }
                _ => self.bad_format(format!("{name} takes an object")),
            },
            "$regexMatch" | "$regexFind" | "$regexFindAll" => match arg {
                MongoValue::Document(d) if d.iter().all(|(k, _)| !k.starts_with('$')) => {
                    for (k, val) in d {
                        self.path.push(Pr::K(k));
                        if !self.field_key(k) {
                        } else if k == "regex" && matches!(val, MongoValue::String(_)) {
                            self.regex_pattern(val);
                        } else if k == "options" {
                            self.regex_options(val);
                        } else {
                            self.expr(val);
                        }
                        self.path.pop();
                    }
                }
                _ => self.bad_format(format!("{name} takes an object")),
            },
            _ => self.expr(arg),
        }
    }
}

/// Gives the first-listed name of 7.31 and 7.32 that a test can use: the count of names of the tables.
#[cfg(test)]
mod tests {
    use super::*;

    fn top(text: &str) -> MongoValue {
        crate::mongo::json::parse_text(text).unwrap().value
    }

    fn check(text: &str) -> Report {
        let t = top(text);
        let cols = vec!["c".to_owned(), "d".to_owned()];
        let dbs = vec!["db".to_owned()];
        let chk = Check {
            phase: Phase::Stored,
            vars: &NoVariables,
            collections: &cols,
            databases: &dbs,
            usage: SqlUse::Panel,
        };
        shape_top(&t).map(|_| check_top(&t, &chk)).unwrap_or_else(|f| Report { faults: f, ..Report::default() })
    }

    #[test]
    fn e_table_has_no_empty_slot_in_use() {
        assert!(E_OPS.iter().filter(|n| !n.is_empty()).count() > 100);
        assert!(is_e_op("$dateTrunc"));
        assert!(!is_e_op(""));
        assert!(!is_e_op("$sum"));
    }

    #[test]
    fn simple_pipeline_is_accepted() {
        let r = check(r#"{"collection":"c","aggregate":[{"$match":{"a":{"$gt":1}}},{"$limit":5}]}"#);
        assert!(r.faults.is_empty(), "{:?}", r.faults);
    }

    #[test]
    fn refused_names_are_found_at_depth() {
        let r = check(r#"{"collection":"c","aggregate":[{"$addFields":{"x":{"$add":[1,{"$function":{}}]}}}]}"#);
        assert_eq!(r.faults.len(), 1);
        assert_eq!(r.faults[0].item, Item::L3);
        assert_eq!(r.faults[0].pointer(), "/aggregate/0/$addFields/x/$add/1/$function");
        assert!(r.faults[0].message.starts_with("$function is not permitted in a MongoDB query of Hune"));
    }

    #[test]
    fn dollar_underscore_names_are_refused() {
        let r = check(r#"{"collection":"c","aggregate":[{"$match":{"$_x":1}}]}"#);
        assert_eq!(r.faults.len(), 1);
        assert_eq!(r.faults[0].code, ErrorCode::NotPermitted);
    }
}
