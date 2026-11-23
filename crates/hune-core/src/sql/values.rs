//! The variables of a document as the engine needs them, and the variable values of a request (7.7).
//!
//! These types do not depend on the document model. The caller maps the variables of the document
//! to [`VariableDef`].

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

use super::interval::TimeRange;
use super::interval_var::{AUTO_VALUE, auto_text};

/// The most values of one variable in a request (7.7, 7.13).
pub const MAX_VARIABLE_VALUES: usize = 10_000;
/// The most bytes of one value (7.7, 7.13).
pub const MAX_VALUE_BYTES: usize = 1024;

/// The members that `multi`, `include_all`, `all_value` and `allow_custom_value` give to a `query` or a `custom`
/// variable (4.8, 4.9).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListOptions {
    /// The viewer can select more than one value.
    pub multi: bool,
    /// The control has the item "All".
    pub include_all: bool,
    /// The SQL text that "All" gives. `None` gives the list of all options.
    pub all_value: Option<String>,
    /// The variable is open: `allow_custom_value` is `true` (4.7). An open `custom` variable is free text for the server.
    pub open: bool,
}

/// The `value_type` of a `text` variable (4.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextValueType {
    /// Free text.
    Text,
    /// A decimal number.
    Number,
}

/// The kind of a variable and the members that the engine reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableKind {
    /// A `query` variable (4.8).
    Query(ListOptions),
    /// A `custom` variable (4.9). `options` holds the `value` of each option, in document order.
    Custom {
        /// The list members.
        list: ListOptions,
        /// The option values.
        options: Vec<String>,
    },
    /// A `text` variable (4.10).
    Text {
        /// The value type.
        value_type: TextValueType,
    },
    /// An `interval` variable (4.10a).
    Interval(IntervalDef),
    /// A `constant` variable (4.10b). The value is the `value` of the document.
    Constant {
        /// The text of the document: author text.
        value: String,
    },
}

/// The members of an `interval` variable that the engine reads (4.10a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalDef {
    /// The items of `intervals`: the only values that a request can name (7.7).
    pub items: Vec<String>,
    /// The option "Auto": `Some` when `auto` is `true`.
    pub auto: Option<AutoDef>,
}

/// The members `auto_count` and `auto_min` of an `interval` variable with `auto` = `true` (4.10a, 7.4a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoDef {
    /// `auto_count`: 1 to 1,000.
    pub count: u32,
    /// `auto_min`: one of the 30 texts of the table of 7.4a.
    pub min: String,
}

/// One variable of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDef {
    /// The name (4.3).
    pub name: String,
    /// The kind and its members.
    pub kind: VariableKind,
}

impl VariableDef {
    /// Makes a `query` variable.
    pub fn query(name: &str, multi: bool, include_all: bool, all_value: Option<&str>) -> Self {
        let list = ListOptions { multi, include_all, all_value: all_value.map(str::to_owned), open: false };
        VariableDef { name: name.to_owned(), kind: VariableKind::Query(list) }
    }

    /// Makes a `custom` variable.
    pub fn custom(name: &str, options: &[&str], multi: bool, include_all: bool, all_value: Option<&str>) -> Self {
        let list = ListOptions { multi, include_all, all_value: all_value.map(str::to_owned), open: false };
        VariableDef {
            name: name.to_owned(),
            kind: VariableKind::Custom { list, options: options.iter().map(|s| (*s).to_owned()).collect() },
        }
    }

    /// Sets the member `allow_custom_value` of a `query` or a `custom` variable. A `text` variable stays as it is.
    #[must_use]
    pub fn with_open(mut self, open: bool) -> Self {
        if let VariableKind::Query(l) | VariableKind::Custom { list: l, .. } = &mut self.kind {
            l.open = open;
        }
        self
    }

    /// Makes a `text` variable.
    pub fn text(name: &str, value_type: TextValueType) -> Self {
        VariableDef { name: name.to_owned(), kind: VariableKind::Text { value_type } }
    }

    /// Makes an `interval` variable. `auto` is the pair `auto_count` and `auto_min` when `auto` is `true`.
    pub fn interval(name: &str, items: &[&str], auto: Option<(u32, &str)>) -> Self {
        let items = items.iter().map(|s| (*s).to_owned()).collect();
        let auto = auto.map(|(count, min)| AutoDef { count, min: min.to_owned() });
        VariableDef { name: name.to_owned(), kind: VariableKind::Interval(IntervalDef { items, auto }) }
    }

    /// Makes a `constant` variable.
    pub fn constant(name: &str, value: &str) -> Self {
        VariableDef { name: name.to_owned(), kind: VariableKind::Constant { value: value.to_owned() } }
    }

    /// Gives the list members, or `None` for a `text`, an `interval` or a `constant` variable.
    pub fn list_options(&self) -> Option<&ListOptions> {
        match &self.kind {
            VariableKind::Query(l) | VariableKind::Custom { list: l, .. } => Some(l),
            VariableKind::Text { .. } | VariableKind::Interval(_) | VariableKind::Constant { .. } => None,
        }
    }

    /// Tells if the variable has author text only: a closed `custom` variable, an `interval` variable or a
    /// `constant` variable. Only these kinds can stand in a quoted name (7.8).
    pub fn is_closed_custom(&self) -> bool {
        match &self.kind {
            VariableKind::Custom { list, .. } => !list.open,
            VariableKind::Interval(_) | VariableKind::Constant { .. } => true,
            VariableKind::Query(_) | VariableKind::Text { .. } => false,
        }
    }

    /// Tells if a selection of the variable can be a list: the variable is `multi`, or "All"
    /// gives the list of all options. The checks at save use it.
    pub fn may_be_list(&self) -> bool {
        self.list_options().is_some_and(|l| l.multi || (l.include_all && l.all_value.is_none()))
    }

    /// Gives the shape of the variable as a query parameter of ClickHouse (7.6a). A `text` variable, a `constant`, and
    /// a `custom` or `query` variable without `multi` and `include_all` give one value. A `custom` or `query` variable
    /// with `multi` or `include_all` gives a list. An `interval` variable, a variable with an `all_value` and any other
    /// kind cannot be a query parameter.
    pub fn param_shape(&self) -> ParamShape {
        if let Some(l) = self.list_options() {
            if l.all_value.is_some() {
                ParamShape::Refused("a variable with an all_value puts author text in the SQL")
            } else if l.multi || l.include_all {
                ParamShape::List
            } else {
                ParamShape::Scalar
            }
        } else if matches!(self.kind, VariableKind::Text { .. } | VariableKind::Constant { .. }) {
            ParamShape::Scalar
        } else if matches!(self.kind, VariableKind::Interval(_)) {
            ParamShape::Refused("an interval variable keeps the text form")
        } else {
            ParamShape::Refused("this kind of variable keeps the text form")
        }
    }
}

/// The shape of a variable as a query parameter of ClickHouse (7.6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamShape {
    /// The variable gives one value: a scalar type.
    Scalar,
    /// The variable can give a list: an Array type.
    List,
    /// The variable cannot be a query parameter. The text gives the reason.
    Refused(&'static str),
}

/// The value of one variable in a request: `{"all": <boolean>, "values": [<string>, ...]}` (7.7).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VariableValue {
    /// True when the viewer selects "All".
    pub all: bool,
    /// The values as the browser sent them.
    pub values: Vec<String>,
}

/// The member `variables` of a query request: a name and a value for each variable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestValues {
    members: BTreeMap<String, VariableValue>,
}

impl RequestValues {
    /// Makes an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the value of a variable. A second call with the same name replaces the first.
    pub fn insert(&mut self, name: &str, all: bool, values: &[&str]) {
        let values = values.iter().map(|s| (*s).to_owned()).collect();
        self.members.insert(name.to_owned(), VariableValue { all, values });
    }

    /// Gives the value of a variable.
    pub fn get(&self, name: &str) -> Option<&VariableValue> {
        self.members.get(name)
    }

    /// Reads the member `variables` of a request from its JSON value.
    ///
    /// The value MUST be an object. A member MUST be an object with the members `all` (a
    /// boolean) and `values` (an array) and no other member. A value that is not a string gives
    /// `variable_value_refused`. Another fault gives `bad_request`.
    pub fn from_json(value: &serde_json::Value) -> Result<RequestValues, ValuesError> {
        let obj = value.as_object().ok_or_else(|| ValuesError::BadRequest("variables must be an object".into()))?;
        let mut out = RequestValues::new();
        for (name, member) in obj {
            let bad = |what: &str| ValuesError::BadRequest(format!("variables.{name}: {what}"));
            let m = member.as_object().ok_or_else(|| bad("expected an object with all and values"))?;
            if let Some(extra) = m.keys().find(|k| *k != "all" && *k != "values") {
                return Err(bad(&format!("unknown member {extra}")));
            }
            let all = m.get("all").and_then(|v| v.as_bool()).ok_or_else(|| bad("all must be a boolean"))?;
            let items = m.get("values").and_then(|v| v.as_array()).ok_or_else(|| bad("values must be an array"))?;
            let mut values = Vec::with_capacity(items.len().min(MAX_VARIABLE_VALUES + 1));
            for item in items {
                match item.as_str() {
                    Some(s) => values.push(s.to_owned()),
                    None => return Err(refused(name, "Each value must be a string.")),
                }
            }
            out.members.insert(name.clone(), VariableValue { all, values });
        }
        Ok(out)
    }
}

/// The error `422 variable_value_refused`: the variable and the reason (7.7, 7.8, 7.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRefused {
    /// The name of the variable.
    pub variable: String,
    /// The reason, as an English sentence.
    pub reason: String,
}

impl fmt::Display for ValueRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "variable {}: {}", self.variable, self.reason)
    }
}

impl std::error::Error for ValueRefused {}

pub(crate) fn refused(variable: &str, reason: &str) -> ValuesError {
    ValuesError::Refused(ValueRefused { variable: variable.to_owned(), reason: reason.to_owned() })
}

/// The error of the check of the variable values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuesError {
    /// `400 bad_request`: an unknown variable name or a wrong shape. The text is the message.
    BadRequest(String),
    /// `422 variable_value_refused`.
    Refused(ValueRefused),
}

impl fmt::Display for ValuesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValuesError::BadRequest(m) => f.write_str(m),
            ValuesError::Refused(r) => r.fmt(f),
        }
    }
}

impl std::error::Error for ValuesError {}

/// The selection of a variable after the check of 7.7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// "All" with an `all_value`. The text is author text.
    AllValue(String),
    /// One value of a variable that is not `multi`.
    Single {
        /// The value.
        value: String,
        /// True for author text (an option value of a closed `custom` variable).
        author: bool,
    },
    /// A `query` variable that is not `multi` and has no value: the empty selection.
    Empty,
    /// A list: a `multi` variable, or "All" without `all_value`.
    List {
        /// The values, in order.
        values: Vec<String>,
        /// True for author text (the option values of a closed `custom` variable, or the list "All" of a `custom` variable).
        author: bool,
    },
}

/// The class of a variable for the name rule of 7.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableClass {
    /// A `query` variable.
    Query,
    /// A `custom` variable.
    Custom,
    /// A `text` variable.
    Text,
    /// An `interval` variable. It has the rows of a closed `custom` variable that has one value (7.8).
    Interval,
    /// A `constant` variable. It has the rows of a closed `custom` variable that has one value (7.8).
    Constant,
}

/// A checked value and the class of its variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The class of the variable.
    pub class: VariableClass,
    /// True when the variable is open (7.7). An open `custom` variable has the rules of a `query` variable in 7.8.
    pub open: bool,
    /// True when the viewer selected "All" (7.7). The macro `$__conditionalAll` and the format `text` read it.
    pub all: bool,
    /// The selection.
    pub selection: Selection,
}

/// The variables of the document and the checked values of a request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedValues {
    declared: BTreeSet<String>,
    resolved: BTreeMap<String, Resolved>,
    /// The shape of each variable of the document as a query parameter (7.6a).
    shapes: BTreeMap<String, ParamShape>,
}

impl ResolvedValues {
    /// Tells if the document has a variable of this name.
    pub fn is_declared(&self, name: &str) -> bool {
        self.declared.contains(name)
    }

    /// Gives the shape of a variable of the document as a query parameter (7.6a), or `None` for a name that is not a
    /// variable of the document.
    pub fn param_shape(&self, name: &str) -> Option<ParamShape> {
        self.shapes.get(name).copied()
    }

    /// Gives the checked value of a variable, or `None` when the request has no member for it.
    pub fn get(&self, name: &str) -> Option<&Resolved> {
        self.resolved.get(name)
    }
}

/// Tells if the text matches the number grammar `^-?[0-9]+(\.[0-9]+)?$` (7.7).
pub fn is_number_text(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
    digits(int) && frac.is_none_or(digits)
}

/// Checks the variable values of a request against the variables of the document (7.7), with no time range.
///
/// The value `$__auto` of an `interval` variable needs the time range (7.4a), so this function refuses it.
/// A caller that has a time range MUST call [`resolve_values_at`].
pub fn resolve_values(defs: &[VariableDef], request: &RequestValues) -> Result<ResolvedValues, ValuesError> {
    resolve_values_in(defs, request, None)
}

/// Checks the variable values of a request against the variables of the document (7.7).
///
/// It checks every member of the request. It does not check that the SQL has the members that it
/// needs: the expansion does this (an absent member gives `400 bad_request`). An `interval` value MUST be an
/// item of the document, or `$__auto` with `auto` = `true`: the check is a set check, and it comes before any
/// source query. The text of "Auto" comes from `range` (7.4a). A `constant` that the request does not name
/// gets the `value` of the document.
pub fn resolve_values_at(
    defs: &[VariableDef],
    request: &RequestValues,
    range: &TimeRange,
) -> Result<ResolvedValues, ValuesError> {
    resolve_values_in(defs, request, Some(range))
}

fn resolve_values_in(
    defs: &[VariableDef],
    request: &RequestValues,
    range: Option<&TimeRange>,
) -> Result<ResolvedValues, ValuesError> {
    let mut out = ResolvedValues {
        declared: defs.iter().map(|d| d.name.clone()).collect(),
        resolved: BTreeMap::new(),
        shapes: defs.iter().map(|d| (d.name.clone(), d.param_shape())).collect(),
    };
    for (name, member) in &request.members {
        let def = defs.iter().find(|d| &d.name == name).ok_or_else(|| {
            ValuesError::BadRequest(format!("variables.{name}: the document has no variable of this name"))
        })?;
        check_shape(name, member)?;
        out.resolved.insert(name.clone(), resolve_one(def, member, range)?);
    }
    // A constant never takes its value from the request: the document gives it (7.7).
    for def in defs {
        if let VariableKind::Constant { value } = &def.kind {
            out.resolved.entry(def.name.clone()).or_insert_with(|| constant_value(value));
        }
    }
    Ok(out)
}

fn constant_value(value: &str) -> Resolved {
    Resolved {
        class: VariableClass::Constant,
        open: false,
        all: false,
        selection: Selection::Single { value: value.to_owned(), author: true },
    }
}

fn check_shape(name: &str, member: &VariableValue) -> Result<(), ValuesError> {
    if member.values.len() > MAX_VARIABLE_VALUES {
        return Err(refused(name, "A variable takes 10000 values at most."));
    }
    for v in &member.values {
        if v.len() > MAX_VALUE_BYTES {
            return Err(refused(name, "A value has 1024 bytes at most."));
        }
        if v.chars().any(|c| c < '\u{20}' || c == '\u{7f}') {
            return Err(refused(name, "A value has no control character."));
        }
    }
    Ok(())
}

fn resolve_one(def: &VariableDef, member: &VariableValue, range: Option<&TimeRange>) -> Result<Resolved, ValuesError> {
    let name = def.name.as_str();
    let values = &member.values;
    match &def.kind {
        VariableKind::Constant { value } => {
            if member.all || values.len() != 1 || values[0] != *value {
                return Err(refused(name, "A constant takes its value from the document."));
            }
            Ok(constant_value(value))
        }
        VariableKind::Interval(interval) => {
            if member.all {
                return Err(refused(name, "An interval variable has no All value."));
            }
            if values.len() != 1 {
                return Err(refused(name, "An interval variable takes exactly one value."));
            }
            let text = interval_text(name, interval, &values[0], range)?;
            Ok(Resolved {
                class: VariableClass::Interval,
                open: false,
                all: false,
                selection: Selection::Single { value: text, author: true },
            })
        }
        VariableKind::Text { value_type } => {
            if member.all {
                return Err(refused(name, "A text variable has no All value."));
            }
            if values.len() != 1 {
                return Err(refused(name, "A text variable takes exactly one value."));
            }
            if *value_type == TextValueType::Number && !is_number_text(&values[0]) {
                return Err(refused(name, "The value is not a number."));
            }
            Ok(Resolved {
                class: VariableClass::Text,
                open: false,
                all: false,
                selection: Selection::Single { value: values[0].clone(), author: false },
            })
        }
        VariableKind::Query(list) => {
            let selection = select(name, list, member, None)?;
            Ok(Resolved { class: VariableClass::Query, open: list.open, all: member.all, selection })
        }
        VariableKind::Custom { list, options } => {
            let selection = select(name, list, member, Some(options))?;
            Ok(Resolved { class: VariableClass::Custom, open: list.open, all: member.all, selection })
        }
    }
}

fn select(
    name: &str,
    list: &ListOptions,
    member: &VariableValue,
    options: Option<&[String]>,
) -> Result<Selection, ValuesError> {
    let values = &member.values;
    // Author text: the option values of a closed `custom` variable. An open `custom` variable is free text (7.7).
    let author = options.is_some() && !list.open;
    if member.all {
        if !list.include_all {
            return Err(refused(name, "The variable has no All value."));
        }
        return match (&list.all_value, options) {
            (Some(text), _) => {
                if values.is_empty() {
                    Ok(Selection::AllValue(text.clone()))
                } else {
                    Err(refused(name, "The values must be empty when All has an all_value."))
                }
            }
            // The list "All" comes from the options of the document: author text, also for an open variable.
            (None, Some(opts)) => Ok(Selection::List { values: opts.to_vec(), author: true }),
            (None, None) => Ok(Selection::List { values: values.clone(), author: false }),
        };
    }
    match options {
        Some(opts) if !list.open => {
            check_options(name, values, opts)?;
            if !list.multi && values.len() != 1 {
                return Err(refused(name, "The variable takes exactly one value."));
            }
        }
        Some(_) => {
            check_typed(name, values)?;
            if !list.multi && values.len() != 1 {
                return Err(refused(name, "The variable takes exactly one value."));
            }
        }
        None if !list.multi && values.len() > 1 => {
            return Err(refused(name, "The variable takes at most one value."));
        }
        None => {}
    }
    if list.multi {
        return Ok(Selection::List { values: values.clone(), author });
    }
    match values.first() {
        Some(v) => Ok(Selection::Single { value: v.clone(), author }),
        None => Ok(Selection::Empty),
    }
}

/// Gives the text that an `interval` variable puts in the SQL (7.7, 7.4a). The value MUST be an item, or `$__auto`
/// with `auto` = `true`. This check is a security boundary: a value that is not an item never reaches the SQL.
/// It uses a set, so the time grows with the count of the items.
fn interval_text(
    name: &str,
    interval: &IntervalDef,
    value: &str,
    range: Option<&TimeRange>,
) -> Result<String, ValuesError> {
    if value == AUTO_VALUE {
        let Some(auto) = &interval.auto else {
            return Err(refused(name, "The value is not an item of the variable."));
        };
        let Some(range) = range else {
            return Err(refused(name, "The text of Auto needs the time range of the request."));
        };
        return auto_text(range, auto.count, &auto.min)
            .map(str::to_owned)
            .ok_or_else(|| refused(name, "The text of Auto is not possible with the members of the variable."));
    }
    let set: HashSet<&str> = interval.items.iter().map(String::as_str).collect();
    if set.contains(value) {
        Ok(value.to_owned())
    } else {
        Err(refused(name, "The value is not an item of the variable."))
    }
}

/// Checks that each value is an option of a closed `custom` variable (7.7). This check is a security
/// boundary. It uses a set: the time grows with the count of the values and of the options, not with their product.
fn check_options(name: &str, values: &[String], options: &[String]) -> Result<(), ValuesError> {
    if values.is_empty() {
        return Ok(());
    }
    let set: HashSet<&str> = options.iter().map(String::as_str).collect();
    if values.iter().any(|v| !set.contains(v.as_str())) {
        return Err(refused(name, "The value is not an option of the variable."));
    }
    Ok(())
}

/// Checks a typed value of an open `custom` variable (7.7): 1 to 1,024 bytes and not `$__all`. The check of the
/// shape has already refused a value of more than 1,024 bytes and a control character.
fn check_typed(name: &str, values: &[String]) -> Result<(), ValuesError> {
    for v in values {
        if v.is_empty() {
            return Err(refused(name, "A typed value has 1 to 1024 bytes."));
        }
        if v == "$__all" {
            return Err(refused(name, "The value $__all is reserved."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(items: &[(&str, bool, &[&str])]) -> RequestValues {
        let mut r = RequestValues::new();
        for (n, a, v) in items {
            r.insert(n, *a, v);
        }
        r
    }

    fn reason(r: Result<ResolvedValues, ValuesError>) -> String {
        match r {
            Err(ValuesError::Refused(x)) => format!("{}: {}", x.variable, x.reason),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn number_grammar() {
        for ok in ["0", "-5", "12.50", "-0.1", "007"] {
            assert!(is_number_text(ok), "{ok}");
        }
        for bad in ["", "-", ".5", "5.", "1e3", "+1", "1,5", " 1", "1 ", "--1", "1.2.3", "0x1", "\u{661}"] {
            assert!(!is_number_text(bad), "{bad:?}");
        }
    }

    #[test]
    fn custom_rules() {
        let defs = [
            VariableDef::custom("c", &["a", "b"], false, false, None),
            VariableDef::custom("m", &["a", "b"], true, true, None),
            VariableDef::custom("v", &["a", "b"], true, true, Some("x,y")),
        ];
        let ok = resolve_values(&defs, &req(&[("c", false, &["a"]), ("m", false, &["b", "a"])])).unwrap();
        assert_eq!(ok.get("c").unwrap().selection, Selection::Single { value: "a".into(), author: true });
        assert_eq!(
            ok.get("m").unwrap().selection,
            Selection::List { values: vec!["b".into(), "a".into()], author: true }
        );
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &[])]))).starts_with("c:"));
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &["a", "b"])]))).starts_with("c:"));
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &["zz"])]))).contains("not an option"));
        assert!(reason(resolve_values(&defs, &req(&[("c", true, &[])]))).contains("no All"));
        // All.
        let all = resolve_values(&defs, &req(&[("m", true, &[]), ("v", true, &[])])).unwrap();
        assert_eq!(
            all.get("m").unwrap().selection,
            Selection::List { values: vec!["a".into(), "b".into()], author: true }
        );
        assert_eq!(all.get("v").unwrap().selection, Selection::AllValue("x,y".into()));
        assert!(reason(resolve_values(&defs, &req(&[("v", true, &["a"])]))).contains("must be empty"));
        // A multi variable with 0 values is a list with 0 values.
        let none = resolve_values(&defs, &req(&[("m", false, &[])])).unwrap();
        assert_eq!(none.get("m").unwrap().selection, Selection::List { values: vec![], author: true });
    }

    #[test]
    fn query_rules() {
        let defs = [
            VariableDef::query("q", false, true, None),
            VariableDef::query("m", true, true, None),
            VariableDef::query("a", true, true, Some("1,2")),
        ];
        let r = resolve_values(&defs, &req(&[("q", false, &[]), ("m", true, &["x", "y'"]), ("a", true, &[])])).unwrap();
        assert_eq!(r.get("q").unwrap().selection, Selection::Empty);
        assert_eq!(
            r.get("m").unwrap().selection,
            Selection::List { values: vec!["x".into(), "y'".into()], author: false }
        );
        assert_eq!(r.get("a").unwrap().selection, Selection::AllValue("1,2".into()));
        let one = resolve_values(&defs, &req(&[("q", false, &["free"])])).unwrap();
        assert_eq!(one.get("q").unwrap().selection, Selection::Single { value: "free".into(), author: false });
        assert!(reason(resolve_values(&defs, &req(&[("q", false, &["a", "b"])]))).starts_with("q:"));
        let all_q = resolve_values(&defs, &req(&[("q", true, &["a", "b"])])).unwrap();
        assert!(matches!(all_q.get("q").unwrap().selection, Selection::List { .. }));
        let no_all = [VariableDef::query("n", true, false, None)];
        assert!(reason(resolve_values(&no_all, &req(&[("n", true, &[])]))).contains("no All"));
    }

    #[test]
    fn text_rules() {
        let defs = [VariableDef::text("t", TextValueType::Text), VariableDef::text("n", TextValueType::Number)];
        let ok = resolve_values(&defs, &req(&[("t", false, &["it's"]), ("n", false, &["-1.5"])])).unwrap();
        assert_eq!(ok.get("t").unwrap().selection, Selection::Single { value: "it's".into(), author: false });
        assert!(reason(resolve_values(&defs, &req(&[("t", false, &[])]))).starts_with("t:"));
        assert!(reason(resolve_values(&defs, &req(&[("t", false, &["a", "b"])]))).starts_with("t:"));
        assert!(reason(resolve_values(&defs, &req(&[("t", true, &[])]))).starts_with("t:"));
        assert!(reason(resolve_values(&defs, &req(&[("n", false, &["1e3"])]))).contains("not a number"));
        // An empty text is permitted for a text variable.
        assert!(resolve_values(&defs, &req(&[("t", false, &[""])])).is_ok());
    }

    #[test]
    fn value_limits() {
        let defs = [VariableDef::query("m", true, false, None), VariableDef::text("t", TextValueType::Text)];
        let long = "a".repeat(1025);
        assert!(reason(resolve_values(&defs, &req(&[("t", false, &[&long])]))).contains("1024"));
        let edge = "a".repeat(1024);
        assert!(resolve_values(&defs, &req(&[("t", false, &[&edge])])).is_ok());
        for bad in ["a\0b", "a\nb", "a\rb", "a\tb", "a\u{1f}b", "a\u{7f}b"] {
            assert!(reason(resolve_values(&defs, &req(&[("t", false, &[bad])]))).contains("control"), "{bad:?}");
        }
        // U+0080 to U+009F are not in the list of 7.7; U+2028 neither.
        assert!(resolve_values(&defs, &req(&[("t", false, &["a\u{2028}b"])])).is_ok());
        let mut r = RequestValues::new();
        r.members.insert("m".into(), VariableValue { all: false, values: vec!["x".into(); 10_000] });
        assert!(resolve_values(&defs, &r).is_ok());
        r.members.insert("m".into(), VariableValue { all: false, values: vec!["x".into(); 10_001] });
        assert!(reason(resolve_values(&defs, &r)).contains("10000"));
    }

    #[test]
    fn unknown_name_is_bad_request() {
        let defs = [VariableDef::text("t", TextValueType::Text)];
        assert!(matches!(resolve_values(&defs, &req(&[("zz", false, &["a"])])), Err(ValuesError::BadRequest(_))));
    }

    #[test]
    fn from_json() {
        let v = serde_json::json!({"a": {"all": false, "values": ["x", "y"]}, "b": {"all": true, "values": []}});
        let r = RequestValues::from_json(&v).unwrap();
        assert_eq!(r.get("a").unwrap().values, ["x", "y"]);
        assert!(r.get("b").unwrap().all);
        for bad in [
            serde_json::json!([]),
            serde_json::json!({"a": 1}),
            serde_json::json!({"a": {"all": false}}),
            serde_json::json!({"a": {"values": []}}),
            serde_json::json!({"a": {"all": 1, "values": []}}),
            serde_json::json!({"a": {"all": false, "values": [], "z": 1}}),
        ] {
            assert!(matches!(RequestValues::from_json(&bad), Err(ValuesError::BadRequest(_))), "{bad}");
        }
        let v = serde_json::json!({"a": {"all": false, "values": [1]}});
        assert!(matches!(RequestValues::from_json(&v), Err(ValuesError::Refused(_))));
    }

    fn closed_custom() -> [VariableDef; 2] {
        [
            VariableDef::custom("c", &["a", "b"], false, true, None),
            VariableDef::custom("m", &["a", "b"], true, true, Some("x,y")),
        ]
    }

    #[test]
    fn a_closed_custom_variable_refuses_each_value_that_is_not_an_option() {
        // The member absent and `false` are the same. The check is exact: no case, blank or Unicode variant passes.
        for open in [None, Some(false)] {
            let defs: Vec<VariableDef> =
                closed_custom().into_iter().map(|d| d.with_open(open.unwrap_or(false))).collect();
            for bad in ["zz", "A", "a ", " a", "\u{FF41}", "a\u{301}", "\u{200b}a", "Production", "", "a,b", "$__all"] {
                assert!(
                    reason(resolve_values(&defs, &req(&[("c", false, &[bad])]))).contains("not an option"),
                    "{bad:?}"
                );
                // One item outside the options in a list.
                assert!(
                    reason(resolve_values(&defs, &req(&[("m", false, &["a", bad])]))).contains("not an option"),
                    "{bad:?}"
                );
            }
            assert!(resolve_values(&defs, &req(&[("c", false, &["a"]), ("m", false, &["b", "a"])])).is_ok());
        }
    }

    #[test]
    fn an_open_custom_variable_takes_free_text_that_is_not_author_text() {
        let defs: Vec<VariableDef> = closed_custom().into_iter().map(|d| d.with_open(true)).collect();
        let r = resolve_values(&defs, &req(&[("c", false, &["zz'; DROP"]), ("m", false, &["a", "q"])])).unwrap();
        assert_eq!(r.get("c").unwrap().selection, Selection::Single { value: "zz'; DROP".into(), author: false });
        assert!(r.get("c").unwrap().open);
        // A value that is an option is free text too (7.7).
        assert_eq!(
            r.get("m").unwrap().selection,
            Selection::List { values: vec!["a".into(), "q".into()], author: false }
        );
        // The list "All" and `all_value` stay author text.
        let all = resolve_values(&defs, &req(&[("c", true, &[]), ("m", true, &[])])).unwrap();
        assert_eq!(
            all.get("c").unwrap().selection,
            Selection::List { values: vec!["a".into(), "b".into()], author: true }
        );
        assert_eq!(all.get("m").unwrap().selection, Selection::AllValue("x,y".into()));
    }

    #[test]
    fn the_bounds_of_a_typed_value() {
        let defs = [VariableDef::custom("c", &["a"], false, false, None).with_open(true)];
        let multi = [VariableDef::custom("m", &["a"], true, false, None).with_open(true)];
        let edge = "a".repeat(1024);
        let long = "a".repeat(1025);
        let multi_byte = "\u{e9}".repeat(512);
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &[""])]))).contains("1 to 1024"));
        assert!(resolve_values(&defs, &req(&[("c", false, &["x"])])).is_ok());
        assert!(resolve_values(&defs, &req(&[("c", false, &[&edge])])).is_ok());
        assert!(resolve_values(&defs, &req(&[("c", false, &[&multi_byte])])).is_ok());
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &[&long])]))).contains("1024"));
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &[&format!("{multi_byte}x")])]))).contains("1024"));
        for bad in ["a\0b", "a\nb", "a\u{7f}b", "a\u{1f}b"] {
            assert!(reason(resolve_values(&defs, &req(&[("c", false, &[bad])]))).contains("control"), "{bad:?}");
        }
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &["$__all"])]))).contains("reserved"));
        assert!(reason(resolve_values(&multi, &req(&[("m", false, &["x", ""])]))).contains("1 to 1024"));
        assert!(reason(resolve_values(&multi, &req(&[("m", false, &["$__all"])]))).contains("reserved"));
        // A variable that is not `multi` takes exactly one value.
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &[])]))).contains("exactly one"));
        assert!(reason(resolve_values(&defs, &req(&[("c", false, &["x", "y"])]))).contains("exactly one"));
        // The member changes nothing for a `query` variable: the empty text stays valid (7.7).
        let q = [VariableDef::query("q", false, false, None).with_open(true)];
        assert!(resolve_values(&q, &req(&[("q", false, &[""])])).is_ok());
        assert!(resolve_values(&q, &req(&[("q", false, &["$__all"])])).is_ok());
    }

    #[test]
    fn the_member_changes_nothing_for_a_query_variable() {
        let cases: [&[(&str, bool, &[&str])]; 5] = [
            &[("q", false, &["x"])],
            &[("q", false, &[])],
            &[("q", true, &[])],
            &[("m", false, &["a", "b'"])],
            &[("m", true, &["a"])],
        ];
        for open in [false, true] {
            let a = [
                VariableDef::query("q", false, true, None).with_open(open),
                VariableDef::query("m", true, true, None).with_open(open),
            ];
            let b = [VariableDef::query("q", false, true, None), VariableDef::query("m", true, true, None)];
            for items in cases {
                let (ra, rb) = (resolve_values(&a, &req(items)), resolve_values(&b, &req(items)));
                let strip = |r: Result<ResolvedValues, ValuesError>| {
                    r.map(|v| ["q", "m"].map(|n| v.get(n).map(|x| (x.class, x.all, x.selection.clone()))))
                };
                assert_eq!(strip(ra), strip(rb), "{items:?}");
            }
        }
    }

    #[test]
    fn the_option_check_takes_time_in_proportion_to_the_count_not_the_product() {
        // 10,000 values against 10,000 options: a linear `contains` makes 10^8 comparisons (ID-K200).
        // A long common prefix makes each comparison of a linear scan cost a full compare of the texts.
        let prefix = "x".repeat(200);
        let options: Vec<String> = (0..10_000).map(|i| format!("{prefix}{i:05}")).collect();
        let refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let defs = [VariableDef::custom("m", &refs, true, false, None)];
        // The worst case for a linear scan: each value is the last option.
        let last = options[9_999].clone();
        let mut r = RequestValues::new();
        r.members.insert("m".into(), VariableValue { all: false, values: vec![last; 10_000] });
        let start = std::time::Instant::now();
        assert!(resolve_values(&defs, &r).is_ok());
        let ok_time = start.elapsed();
        // A value that is not an option is found after the full scan of the values that come first.
        let mut values: Vec<String> = vec![options[0].clone(); 9_999];
        values.push("not-an-option".into());
        r.members.insert("m".into(), VariableValue { all: false, values });
        let start = std::time::Instant::now();
        assert!(reason(resolve_values(&defs, &r)).contains("not an option"));
        let refuse_time = start.elapsed();
        assert!(ok_time.as_millis() <= 250, "10,000 by 10,000 took {ok_time:?}");
        assert!(refuse_time.as_millis() <= 250, "10,000 by 10,000 took {refuse_time:?}");
    }

    #[test]
    fn may_be_list() {
        assert!(VariableDef::query("a", true, false, None).may_be_list());
        assert!(VariableDef::query("a", false, true, None).may_be_list());
        assert!(!VariableDef::query("a", false, true, Some("x")).may_be_list());
        assert!(!VariableDef::query("a", false, false, None).may_be_list());
        assert!(!VariableDef::text("a", TextValueType::Text).may_be_list());
    }
}
