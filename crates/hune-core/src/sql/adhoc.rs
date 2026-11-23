// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ad hoc filters of a ClickHouse source (4.10c, 7.7a, 7.10a).
//!
//! The module does no I/O. It reads the request member of an `adhoc` variable, binds each filter to a key of the
//! document and to an item of the configured list `clickhouse "<id>".adhoc_columns`, and makes the text of the macro
//! `$__adhocFilters`.
//!
//! The rules that make the text safe:
//!
//! 1. The request key is only compared. It MUST equal a key of the variable and an item of the configured list,
//!    byte for byte. The identifier text of the SQL comes from the configured string, never from the request bytes.
//! 2. The identifier is in backquotes with the name escapes of 7.8. A configured item matches
//!    `^[A-Za-z_][A-Za-z0-9_]{0,63}$` (13.13), so no escape changes it.
//! 3. The operator is `=` or `!=`, from a closed list. Another text gives `variable_value_refused`.
//! 4. Each value is a string literal with the string escapes of 7.8. The expansion records the range of each literal,
//!    and the property P of 7.11 checks it as a free value.
//! 5. A filter is never ignored: a fault of one filter refuses the request before any source query.

use super::escape::{escape_name_content, quote_string};
use super::lexer::{Token, TokenKind};
use super::values::{MAX_VALUE_BYTES, ValueRefused};
use crate::limits::{MAX_ADHOC_FILTERS, MAX_ADHOC_KEYS};

/// The name of the macro, without the `$` (7.10a).
pub const MACRO_NAME: &str = "__adhocFilters";

/// The most items of the configured list `clickhouse "<id>".adhoc_columns` (13.12).
pub const MAX_ADHOC_COLUMNS: usize = 256;

/// The two operators of a filter (7.7a). The list is closed: `<`, `>`, `IN`, `=~` and `!~` are not operators of Hune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdhocOp {
    /// `=`
    Eq,
    /// `!=`
    Ne,
}

impl AdhocOp {
    /// Reads an operator text. The match is exact: `==`, `<>` and a text with a blank are not operators.
    pub fn parse(text: &str) -> Option<AdhocOp> {
        match text {
            "=" => Some(AdhocOp::Eq),
            "!=" => Some(AdhocOp::Ne),
            _ => None,
        }
    }

    /// Gives the text of the operator in the request and in the SQL.
    pub fn as_str(self) -> &'static str {
        match self {
            AdhocOp::Eq => "=",
            AdhocOp::Ne => "!=",
        }
    }
}

/// Tells if a text is a plain column name of 4.10c and 13.12: `^[A-Za-z_][A-Za-z0-9_]{0,63}$`.
pub fn is_column_name(text: &str) -> bool {
    let b = text.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// One filter of a request after the shape check (7.7a). The texts are the bytes of the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFilter {
    /// The key of the request. The engine only compares it.
    pub key: String,
    /// The operator text of the request.
    pub op: String,
    /// The value of the request.
    pub value: String,
}

/// The error of [`read_member`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdhocError {
    /// `400 bad_request`: the member does not have the closed shape. The text is the message.
    BadRequest(String),
    /// `422 variable_value_refused`: a cap, an operator, a key or a value of 7.7.
    Refused(ValueRefused),
}

fn refused(variable: &str, reason: String) -> ValueRefused {
    ValueRefused { variable: variable.to_owned(), reason }
}

/// Reads the member of one `adhoc` variable in the member `variables` of a request (7.7a). The member is the closed
/// object `{"filters": [{"key": ..., "op": ..., "value": ...}, ...]}`.
///
/// A wrong shape gives [`AdhocError::BadRequest`]. More than `MAX_ADHOC_FILTERS` filters gives
/// [`AdhocError::Refused`] before the read of the items, so the work is bounded. The checks of the operator, the key
/// and the value come later, in [`bind`], because the key needs the document and the configuration.
pub fn read_member(variable: &str, member: &serde_json::Value) -> Result<Vec<RequestFilter>, AdhocError> {
    let bad = |what: &str| AdhocError::BadRequest(format!("variables.{variable}: {what}"));
    let obj = member.as_object().ok_or_else(|| bad("expected an object with the member filters"))?;
    if let Some(extra) = obj.keys().find(|k| *k != "filters") {
        return Err(bad(&format!("unknown member {extra}")));
    }
    let items = obj.get("filters").and_then(|v| v.as_array()).ok_or_else(|| bad("filters must be an array"))?;
    if items.len() > MAX_ADHOC_FILTERS {
        return Err(AdhocError::Refused(refused(
            variable,
            format!("A request has {MAX_ADHOC_FILTERS} filters at most for one variable."),
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let f = item.as_object().ok_or_else(|| bad(&format!("filters.{i} must be an object")))?;
        if let Some(extra) = f.keys().find(|k| !matches!(k.as_str(), "key" | "op" | "value")) {
            return Err(bad(&format!("filters.{i}: unknown member {extra}")));
        }
        let text = |name: &str| {
            f.get(name)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| bad(&format!("filters.{i}.{name} must be a string")))
        };
        out.push(RequestFilter { key: text("key")?, op: text("op")?, value: text("value")? });
    }
    Ok(out)
}

/// One filter of a run: the configured column, the operator and the value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound<'a> {
    /// The item of the configured list. It is the only source of the identifier text.
    column: &'a str,
    op: AdhocOp,
    value: String,
}

/// The filters of one `adhoc` variable for one run, bound to the configured strings (7.7a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdhocRun<'a> {
    variable: &'a str,
    filters: Vec<Bound<'a>>,
}

/// Binds the filters of a request to the keys of the variable and to the configured list (7.7a). It runs before any
/// source query, on each query route that receives the filters.
///
/// - `variable` is the name of the `adhoc` variable, for the errors.
/// - `keys` is the member `keys` of the variable in the document.
/// - `configured` is `clickhouse "<id>".adhoc_columns` of the source of the variable, or an empty list when the key is
///   absent. An empty list permits no key.
///
/// The checks, for each filter in order: the operator, the key (in `keys` AND in `configured`, byte for byte), then
/// the value (0 to 1,024 bytes, no control character). The first fault gives the error, and the reason names the
/// position of the filter, never its value.
pub fn bind<'a>(
    variable: &'a str,
    keys: &[String],
    configured: &'a [String],
    filters: &[RequestFilter],
) -> Result<AdhocRun<'a>, ValueRefused> {
    if filters.len() > MAX_ADHOC_FILTERS {
        return Err(refused(variable, format!("A request has {MAX_ADHOC_FILTERS} filters at most for one variable.")));
    }
    let mut out = Vec::with_capacity(filters.len());
    for (i, f) in filters.iter().enumerate() {
        let n = i + 1;
        let op = AdhocOp::parse(&f.op)
            .ok_or_else(|| refused(variable, format!("Filter {n}: the operator must be = or !=.")))?;
        let column = configured
            .iter()
            .find(|c| c.as_bytes() == f.key.as_bytes())
            .filter(|_| keys.iter().any(|k| k.as_bytes() == f.key.as_bytes()))
            .ok_or_else(|| refused(variable, format!("Filter {n}: the key is not a permitted key of the variable.")))?;
        if f.value.len() > MAX_VALUE_BYTES {
            return Err(refused(variable, format!("Filter {n}: a value has 1024 bytes at most.")));
        }
        if f.value.chars().any(|c| c < '\u{20}' || c == '\u{7f}') {
            return Err(refused(variable, format!("Filter {n}: a value has no control character.")));
        }
        out.push(Bound { column: column.as_str(), op, value: f.value.clone() });
    }
    Ok(AdhocRun { variable, filters: out })
}

impl<'a> AdhocRun<'a> {
    /// A run with no filter: the macro gives `1=1`.
    pub fn empty(variable: &'a str) -> AdhocRun<'a> {
        AdhocRun { variable, filters: Vec::new() }
    }

    /// The name of the `adhoc` variable.
    pub fn variable(&self) -> &'a str {
        self.variable
    }

    /// The count of filters.
    pub fn len(&self) -> usize {
        self.filters.len()
    }

    /// Tells if the run has no filter.
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// The configured columns of the filters, in the order of the request: the keys of the log line (14.7). The log
    /// never holds a value.
    pub fn columns(&self) -> Vec<&'a str> {
        self.filters.iter().map(|f| f.column).collect()
    }

    /// The text of the macro (7.10a) and the byte ranges of the value literals in it.
    ///
    /// No filter gives `1=1`. Else the text is `(` + the filters joined by ` AND ` + `)`. A filter is
    /// `` `column` `` + ` = ` or ` != ` + the value as a string literal of 7.8. Each range covers one literal with its
    /// quote marks: the property P checks it as a sequence of one string token (7.11).
    pub fn render(&self) -> (String, Vec<(usize, usize)>) {
        if self.filters.is_empty() {
            return ("1=1".to_owned(), Vec::new());
        }
        let mut text = String::from("(");
        let mut ranges = Vec::with_capacity(self.filters.len());
        for (i, f) in self.filters.iter().enumerate() {
            if i > 0 {
                text.push_str(" AND ");
            }
            text.push('`');
            escape_name_content(f.column, '`', &mut text);
            text.push_str("` ");
            text.push_str(f.op.as_str());
            text.push(' ');
            let start = text.len();
            text.push_str(&quote_string(&f.value));
            ranges.push((start, text.len()));
        }
        text.push(')');
        (text, ranges)
    }
}

/// The words that end the search back for `WHERE` or `PREWHERE` at depth 0: another clause starts there (7.10a).
const STOP_WORDS: [&str; 22] = [
    "SELECT",
    "FROM",
    "JOIN",
    "ON",
    "USING",
    "WITH",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "OFFSET",
    "SETTINGS",
    "FORMAT",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "WINDOW",
    "QUALIFY",
    "ARRAY",
    "SAMPLE",
    "INTO",
    "BY",
];

/// The words that end the `WHERE` or `PREWHERE` clause after the macro, at depth 0 (7.10a).
const END_WORDS: [&str; 14] = [
    "GROUP",
    "ORDER",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "SETTINGS",
    "FORMAT",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "WINDOW",
    "QUALIFY",
    "WHERE",
    "INTO",
];

/// One code token (not a blank and not a comment) and the facts of the one pass of [`AdhocPlaces::of`].
#[derive(Debug, Clone, Copy)]
struct CodeFacts {
    /// The start offset of the token.
    start: usize,
    /// The count of open `(` and `[` before the token. A `(` or `[` has the depth outside it, a `)` or `]` too. A
    /// `)` with no match gives a depth under 0.
    depth: i64,
    /// The count of `CASE` minus the count of `END` at depth 0 before the token.
    level: i64,
    /// The code index of the `WHERE` or `PREWHERE` at depth 0 of the clause of the token: the nearest one before it,
    /// with no stop word at depth 0 between them.
    clause: Option<usize>,
    /// An `OR`, a `?` or an `AS` at depth 0 and at the level of the clause word comes after the clause word and before
    /// the token.
    bad_before: bool,
    /// The token is the `AND` of a `BETWEEN` (at depth 0 and at the level of the clause word).
    between_and: bool,
    /// The token is the word `AND`.
    and: bool,
    /// A set operation `EXCEPT` at depth 0 comes before the token. `* EXCEPT (...)` is a column transformer.
    except_before: bool,
    /// The first code index from the token on that ends a clause: a depth under 0, or a `;` or an end word at depth 0.
    next_end: usize,
}

/// The permitted places of `$__adhocFilters` in one stored text (7.10a, rule 4; 7.18, item 20).
///
/// A permitted macro is one term of the top-level conjunction of the `WHERE` or `PREWHERE` clause of the outer
/// statement. So the clause is exactly `rest AND filters`, and a filter never adds a row that the rest of the clause
/// refuses. The six conditions, all on the code tokens of the stored text:
///
/// 1. The `$` of the macro is at depth 0 of the statement: the count of `(` and `[` minus `)` and `]` before it is 0.
/// 2. The nearest clause word at depth 0 before it is `WHERE` or `PREWHERE`. The search stops at a stop word.
/// 3. No set operation `EXCEPT` at depth 0 comes before it. A column transformer `* EXCEPT (...)` is permitted.
/// 4. The code token before it is the clause word, or a logical `AND` that is not the `AND` of a `BETWEEN`. It is not
///    inside `CASE ... END`.
/// 5. The code token after it is `AND`, the end of the clause (a `)` at depth under 0, `;`, or an end word), or no
///    token.
/// 6. The clause has no `OR`, no `?` and no `AS` at depth 0 outside `CASE ... END`.
///
/// [`AdhocPlaces::of`] reads the tokens in one forward pass and one backward pass. [`AdhocPlaces::permits`] then costs
/// two binary searches, so a text with many macros costs time in proportion to its length. The save (7.18) and the run
/// (7.11) use this one type, so they agree.
#[derive(Debug, Clone)]
pub struct AdhocPlaces {
    code: Vec<CodeFacts>,
    /// For each level, the code indices of the `OR`, `?` and `AS` tokens at depth 0, in increasing order.
    bad_at_level: std::collections::HashMap<i64, Vec<usize>>,
}

impl AdhocPlaces {
    /// Reads the tokens of `src` once. `tokens` MUST be the tokens of `src` from the lexer of `clickhouse`.
    pub fn of(src: &str, tokens: &[Token]) -> AdhocPlaces {
        let word = |t: &Token, list: &[&str]| {
            t.kind == TokenKind::Word && list.iter().any(|w| t.text(src).eq_ignore_ascii_case(w))
        };
        let punct = |t: &Token, p: u8| t.kind == TokenKind::Punct && src.as_bytes().get(t.start) == Some(&p);
        let code_tokens: Vec<&Token> = tokens.iter().filter(|t| !t.kind.is_trivia()).collect();
        let mut code = Vec::with_capacity(code_tokens.len());
        let mut bad_at_level: std::collections::HashMap<i64, Vec<usize>> = std::collections::HashMap::new();
        let (mut depth, mut level) = (0i64, 0i64);
        // The state of the clause of the search back: its word, its level, an `OR`, `?` or `AS` in it, and the count
        // of `BETWEEN` that wait for their `AND`.
        let mut clause: Option<(usize, i64)> = None;
        let (mut clause_bad, mut pending_between) = (false, 0usize);
        let mut except_seen = false;
        for (k, t) in code_tokens.iter().enumerate() {
            if punct(t, b')') || punct(t, b']') {
                depth -= 1;
            }
            let mut facts = CodeFacts {
                start: t.start,
                depth,
                level,
                clause: clause.map(|(w, _)| w),
                bad_before: clause_bad,
                between_and: false,
                and: word(t, &["AND"]),
                except_before: except_seen,
                next_end: 0,
            };
            if depth == 0 {
                let is_case = word(t, &["CASE"]);
                let is_end = word(t, &["END"]);
                let bad = !is_case && !is_end && (word(t, &["OR", "AS"]) || punct(t, b'?'));
                if bad {
                    bad_at_level.entry(level).or_default().push(k);
                }
                if word(t, &["WHERE", "PREWHERE"]) {
                    clause = Some((k, level));
                    clause_bad = false;
                    pending_between = 0;
                } else if word(t, &STOP_WORDS) {
                    clause = None;
                } else if clause.is_some_and(|(_, l)| l == level) {
                    if word(t, &["BETWEEN"]) {
                        pending_between += 1;
                    } else if facts.and && pending_between > 0 {
                        pending_between -= 1;
                        facts.between_and = true;
                    } else if bad {
                        clause_bad = true;
                    }
                }
                if word(t, &["EXCEPT"]) && !(k > 0 && punct(code_tokens[k - 1], b'*')) {
                    except_seen = true;
                }
                if is_case {
                    level += 1;
                } else if is_end {
                    level -= 1;
                }
            }
            code.push(facts);
            if punct(t, b'(') || punct(t, b'[') {
                depth += 1;
            }
        }
        let mut next_end = code.len();
        for k in (0..code.len()).rev() {
            let t = code_tokens[k];
            if code[k].depth < 0 || (code[k].depth == 0 && (punct(t, b';') || word(t, &END_WORDS))) {
                next_end = k;
            }
            code[k].next_end = next_end;
        }
        AdhocPlaces { code, bad_at_level }
    }

    /// Tells if the macro from the byte offset `start` to the byte offset `end` has a permitted place. A code token
    /// MUST start at `start`. `end` is the offset after the macro: after `$__adhocFilters` or after
    /// `${__adhocFilters}`.
    pub fn permits(&self, start: usize, end: usize) -> bool {
        let code = &self.code;
        let m = code.partition_point(|c| c.start < start);
        let Some(mac) = code.get(m).filter(|c| c.start == start) else { return false };
        // Rules 1 and 2.
        let Some(w) = mac.clause.filter(|_| mac.depth == 0) else { return false };
        let clause = &code[w];
        // Rule 3.
        if clause.except_before {
            return false;
        }
        // Rule 4, and rule 6 before the macro.
        let Some(before) = m.checked_sub(1).and_then(|k| code.get(k)) else { return false };
        let joined = m - 1 == w || (before.and && before.depth == 0 && !before.between_and);
        if mac.bad_before || mac.level != clause.level || !joined {
            return false;
        }
        // Rule 5.
        let q = code.partition_point(|c| c.start < end);
        let Some(after) = code.get(q) else { return true };
        if !after.and && after.next_end != q {
            return false;
        }
        // Rule 6 after the macro: no `OR`, `?` or `AS` at depth 0 and at the level of the token after the macro, up to
        // the end of the clause.
        let Some(list) = self.bad_at_level.get(&after.level) else { return true };
        list.get(list.partition_point(|&k| k < q)).is_none_or(|&k| k >= after.next_end)
    }
}

/// Tells if the macro from the byte offset `start` to `end` of `src` has a permitted place (7.10a). It reads the whole
/// text: a caller with more than one macro in a text builds one [`AdhocPlaces`] and asks it for each macro.
pub fn place_is_permitted(src: &str, tokens: &[Token], start: usize, end: usize) -> bool {
    AdhocPlaces::of(src, tokens).permits(start, end)
}

/// The message of a macro in a place that 7.10a does not permit.
pub const PLACE_MESSAGE: &str = "$__adhocFilters must be one AND term at the top level of the WHERE or PREWHERE clause \
     of the outer query, so that a filter never adds a row that the rest of the clause refuses";

/// Checks the member `keys` of an `adhoc` variable against the configured list of its source (7.18, item 18). It gives
/// the index of each key that is not in the list. `configured` is empty when the source has no list.
pub fn keys_outside(keys: &[String], configured: &[String]) -> Vec<usize> {
    keys.iter()
        .enumerate()
        .filter(|(_, k)| !configured.iter().any(|c| c.as_bytes() == k.as_bytes()))
        .map(|(i, _)| i)
        .collect()
}

/// The permitted keys of a variable for the page (6.14): the keys of the document that are in the configured list, in
/// the order of the document. At most `MAX_ADHOC_KEYS` items.
pub fn permitted_keys<'k>(keys: &'k [String], configured: &[String]) -> Vec<&'k str> {
    keys.iter()
        .filter(|k| configured.iter().any(|c| c.as_bytes() == k.as_bytes()))
        .take(MAX_ADHOC_KEYS)
        .map(String::as_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn f(key: &str, op: &str, value: &str) -> RequestFilter {
        RequestFilter { key: key.into(), op: op.into(), value: value.into() }
    }

    #[test]
    fn the_member_has_a_closed_shape() {
        let ok = read_member("filters", &json!({"filters": [{"key": "host", "op": "=", "value": "a"}]})).unwrap();
        assert_eq!(ok, vec![f("host", "=", "a")]);
        assert_eq!(read_member("filters", &json!({"filters": []})).unwrap(), Vec::new());
        for bad in [
            json!([]),
            json!({}),
            json!({"filters": {}}),
            json!({"filters": [], "all": false}),
            json!({"filters": [1]}),
            json!({"filters": [{"key": "host", "op": "="}]}),
            json!({"filters": [{"key": "host", "op": "=", "value": 1}]}),
            json!({"filters": [{"key": "host", "op": "=", "value": "a", "x": 1}]}),
            json!({"all": false, "values": []}),
        ] {
            assert!(matches!(read_member("filters", &bad), Err(AdhocError::BadRequest(_))), "{bad}");
        }
        let many: Vec<_> = (0..17).map(|_| json!({"key": "host", "op": "=", "value": "a"})).collect();
        assert!(matches!(read_member("filters", &json!({"filters": many})), Err(AdhocError::Refused(_))));
        let sixteen: Vec<_> = (0..16).map(|_| json!({"key": "host", "op": "=", "value": "a"})).collect();
        assert_eq!(read_member("filters", &json!({"filters": sixteen})).unwrap().len(), 16);
    }

    #[test]
    fn bind_checks_the_operator_the_key_and_the_value() {
        let keys = list(&["host", "env"]);
        let cfg = list(&["env", "host", "region"]);
        let run = bind("filters", &keys, &cfg, &[f("host", "=", "a"), f("env", "!=", "")]).unwrap();
        assert_eq!(run.columns(), ["host", "env"]);
        for (filter, reason) in [
            (f("host", "<", "1"), "operator"),
            (f("host", "==", "1"), "operator"),
            (f("host", " =", "1"), "operator"),
            (f("host", "IN", "1"), "operator"),
            (f("host", "=~", "1"), "operator"),
            (f("region", "=", "1"), "key"),
            (f("other", "=", "1"), "key"),
            (f("Host", "=", "1"), "key"),
            (f("host ", "=", "1"), "key"),
            (f("ho\u{0}st", "=", "1"), "key"),
            (f("host", "=", "a\nb"), "control"),
            (f("host", "=", "a\u{7f}"), "control"),
            (f("host", "=", &"x".repeat(1025)), "1024"),
        ] {
            let e = bind("filters", &keys, &cfg, std::slice::from_ref(&filter)).unwrap_err();
            assert!(e.reason.contains(reason), "{filter:?}: {}", e.reason);
            assert!(!e.reason.contains(&filter.value) || filter.value.len() < 2, "the reason holds no value");
        }
        assert!(bind("filters", &keys, &cfg, &[f("host", "=", &"x".repeat(1024))]).is_ok());
        // A key of the document that the configured list has not: refused (a change of the list fails closed).
        assert!(bind("filters", &list(&["zone"]), &cfg, &[f("zone", "=", "a")]).is_err());
        // No configured list: no key.
        assert!(bind("filters", &keys, &[], &[f("host", "=", "a")]).is_err());
        let many: Vec<_> = (0..17).map(|_| f("host", "=", "a")).collect();
        assert!(bind("filters", &keys, &cfg, &many).is_err());
    }

    #[test]
    fn the_identifier_comes_from_the_configured_string() {
        let cfg = list(&["host"]);
        let keys = list(&["host"]);
        let request = f("host", "=", "a");
        let run = bind("filters", &keys, &cfg, std::slice::from_ref(&request)).unwrap();
        // The column is the memory of the configured item, not of the request key and not of the document key.
        assert!(std::ptr::eq(run.columns()[0].as_ptr(), cfg[0].as_ptr()));
        assert!(!std::ptr::eq(run.columns()[0].as_ptr(), request.key.as_ptr()));
        assert!(!std::ptr::eq(run.columns()[0].as_ptr(), keys[0].as_ptr()));
    }

    #[test]
    fn render_gives_one_one_or_the_filters_in_parentheses() {
        assert_eq!(AdhocRun::empty("filters").render(), ("1=1".to_owned(), Vec::new()));
        let keys = list(&["host", "env"]);
        let run = bind("filters", &keys, &keys, &[f("host", "=", "a'b\\c"), f("env", "!=", "")]).unwrap();
        let (text, ranges) = run.render();
        assert_eq!(text, "(`host` = 'a\\'b\\\\c' AND `env` != '')");
        let shown: Vec<&str> = ranges.iter().map(|(a, b)| &text[*a..*b]).collect();
        assert_eq!(shown, ["'a\\'b\\\\c'", "''"]);
    }

    include!("../../tests/adhoc_common/places.rs");

    fn places_permit(sql: &str) -> bool {
        let tokens = super::super::lexer::lex(sql).unwrap();
        let (start, end) = macro_range(sql);
        let one = AdhocPlaces::of(sql, &tokens).permits(start, end);
        assert_eq!(one, place_is_permitted(sql, &tokens, start, end), "{sql}");
        one
    }

    #[test]
    fn the_place_is_one_term_of_the_top_level_conjunction() {
        for sql in PLACE_ACCEPT {
            assert!(places_permit(sql), "accepted: {sql}");
        }
        for sql in PLACE_WIDEN.iter().chain(&PLACE_REFUSED) {
            assert!(!places_permit(sql), "refused: {sql}");
        }
        // Each macro of a text has its own answer.
        let sql = "SELECT 1 FROM t WHERE $__adhocFilters AND x IN (SELECT y FROM u WHERE $__adhocFilters) \
                   AND $__adhocFilters";
        let tokens = super::super::lexer::lex(sql).unwrap();
        let places = AdhocPlaces::of(sql, &tokens);
        let starts: Vec<usize> = sql.match_indices("$__adhocFilters").map(|(at, _)| at).collect();
        let answers: Vec<bool> = starts.iter().map(|&at| places.permits(at, at + 15)).collect();
        assert_eq!(answers, [true, false, true]);
        // A start where no token starts, and an offset past the end.
        assert!(!places.permits(starts[0] + 1, starts[0] + 15));
        assert!(!places.permits(sql.len() + 5, sql.len() + 9));
        // An unmatched `)` before the macro gives a depth under 0: refused.
        assert!(!places_permit("SELECT 1 FROM t WHERE x) AND $__adhocFilters"));
        // `END` with no `CASE` changes the level: refused.
        assert!(!places_permit("SELECT 1 FROM t WHERE x END AND $__adhocFilters"));
    }

    #[test]
    fn keys_outside_and_permitted_keys() {
        let keys = list(&["host", "env", "zone"]);
        let cfg = list(&["env", "host"]);
        assert_eq!(keys_outside(&keys, &cfg), [2]);
        assert_eq!(keys_outside(&keys, &[]), [0, 1, 2]);
        assert_eq!(permitted_keys(&keys, &cfg), ["host", "env"]);
        assert!(permitted_keys(&keys, &[]).is_empty());
    }

    #[test]
    fn column_names() {
        for ok in ["a", "_", "A_1", &"a".repeat(64)] {
            assert!(is_column_name(ok), "{ok}");
        }
        for bad in ["", "1a", "a.b", "a-b", "a b", "é", &"a".repeat(65), "a`", "m['k']"] {
            assert!(!is_column_name(bad), "{bad}");
        }
    }
}
