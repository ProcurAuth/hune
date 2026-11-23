//! The query of the MongoDB plugin (15.6, rows of `haohanyang-mongodb-datasource`): the text of the
//! stages, the member `collection` and the member `database`, and the checks of 7.28 to 7.33.
//!
//! The plugin writes a JSON array of stages in `queryText`. The importer builds the object of 7.29 from
//! it, and it cuts the object into lines (4.16). A text outside the strict profile J (7.28) is
//! `unsupported`: the importer reads no shell syntax and no JavaScript (7.27). Two forms that Grafana
//! writes are `changed` first: the wrapper of `$__from` and `$__to`, and the bare token. The old form
//! `[[name]]` is `changed` to `${name}`.

use std::collections::BTreeSet;

use hune_core::limits::MAX_SQL_BYTES;
use hune_core::mongo::{MongoValue, SaveContext, WarnKind, parse_text, save_checks, to_query_json};
use hune_core::sql::placeholders::{self, BraceFault, Found, is_name_byte, next_double_bracket};
use hune_core::sql::{SqlUse, VariableDef};

use crate::item::{Ctx, Fallback};
use crate::options::{MongoLists, SourceKind};
use crate::sqlprep::Sql;
use crate::walk::Node;

/// One token of the text. The tokenizer knows strings, the five marks of JSON and the other words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mark(u8),
    Str,
    Word,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Cuts the text into tokens. It gives `None` for a string that has no end: the parser reports it.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if matches!(c, b'{' | b'}' | b'[' | b']' | b':' | b',') {
            out.push(Token { kind: Kind::Mark(c), start: i, end: i + 1 });
            i += 1;
        } else if c == b'"' {
            let start = i;
            i += 1;
            loop {
                match b.get(i)? {
                    b'\\' => i += 2,
                    b'"' => break,
                    _ => i += 1,
                }
            }
            i += 1;
            out.push(Token { kind: Kind::Str, start, end: i });
        } else {
            let start = i;
            while i < b.len()
                && !b[i].is_ascii_whitespace()
                && !matches!(b[i], b'{' | b'}' | b'[' | b']' | b':' | b',' | b'"')
            {
                i += 1;
            }
            out.push(Token { kind: Kind::Word, start, end: i });
        }
    }
    Some(out)
}

/// The content of a string token, without its quote marks.
fn content<'a>(text: &'a str, t: &Token) -> &'a str {
    &text[t.start + 1..t.end - 1]
}

/// The edits that the text gets before the parse, and how many of each.
#[derive(Default)]
struct Rewrite {
    /// `(start, end, new text)`, in the order of the text.
    edits: Vec<(usize, usize, String)>,
    wrappers: usize,
    bare: usize,
    brackets: Vec<String>,
}

/// The macro that a name of the plugin gives: `$__from` gives `$__timeFrom`, `$__to` gives `$__timeTo`.
fn wrapper_macro(name: &str) -> Option<&'static str> {
    match name {
        "$__from" => Some("$__timeFrom"),
        "$__to" => Some("$__timeTo"),
        _ => None,
    }
}

/// Finds the forms of Grafana that the importer changes (15.6).
fn rewrite(text: &str, vars: &BTreeSet<String>) -> Rewrite {
    let mut r = Rewrite::default();
    let Some(tokens) = tokenize(text) else { return r };
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        // The wrapper `{"$date": {"$numberLong": "$__from"}}`.
        if let Some(w) = tokens.get(i..i + 9)
            && w[0].kind == Kind::Mark(b'{')
            && w[1].kind == Kind::Str
            && content(text, &w[1]) == "$date"
            && w[2].kind == Kind::Mark(b':')
            && w[3].kind == Kind::Mark(b'{')
            && w[4].kind == Kind::Str
            && content(text, &w[4]) == "$numberLong"
            && w[5].kind == Kind::Mark(b':')
            && w[6].kind == Kind::Str
            && w[7].kind == Kind::Mark(b'}')
            && w[8].kind == Kind::Mark(b'}')
            && let Some(name) = wrapper_macro(content(text, &w[6]))
        {
            r.edits.push((w[0].start, w[8].end, format!("\"{name}\"")));
            r.wrappers += 1;
            i += 9;
            continue;
        }
        match t.kind {
            Kind::Word if matches!(&text[t.start..t.end], "$__from" | "$__to") => {
                r.edits.push((t.start, t.end, format!("\"{}\"", &text[t.start..t.end])));
                r.bare += 1;
            }
            Kind::Str => {
                let (lo, hi) = (t.start + 1, t.end - 1);
                let mut p = lo;
                while let Some((at, name)) = next_double_bracket(text, p, hi) {
                    p = at + 2 + name.len() + 2;
                    if vars.contains(name) {
                        r.edits.push((at, p, format!("${{{name}}}")));
                        if !r.brackets.iter().any(|b| b == name) {
                            r.brackets.push(name.to_owned());
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    r
}

/// Applies the edits of [`rewrite`] to the text.
fn apply(text: &str, edits: &[(usize, usize, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, new) in edits {
        out.push_str(&text[last..*start]);
        out.push_str(new);
        last = *end;
    }
    out.push_str(&text[last..]);
    out
}

/// Gives the line and the column in characters, from 1, of a byte offset.
fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// Makes the lines of the object of 7.29 from the stages: one line for each key and one for each
/// stage, in the compact Extended JSON of 7.28.
fn object_lines(collection: &str, database: Option<&str>, stages: &[MongoValue]) -> Vec<String> {
    let text = |s: &str| to_query_json(&MongoValue::String(s.to_owned()));
    let mut lines = vec!["{".to_owned(), format!("  \"collection\": {},", text(collection))];
    if let Some(d) = database {
        lines.push(format!("  \"database\": {},", text(d)));
    }
    lines.push("  \"aggregate\": [".to_owned());
    for (i, stage) in stages.iter().enumerate() {
        let comma = if i + 1 < stages.len() { "," } else { "" };
        lines.push(format!("    {}{comma}", to_query_json(stage)));
    }
    lines.push("  ]".to_owned());
    lines.push("}".to_owned());
    lines
}

/// The members of a MongoDB query that the mapping reads: `queryText`, `collection` and `database`.
pub(crate) struct MongoMembers<'a> {
    pub query_text: Option<Node<'a>>,
    pub collection: Option<Node<'a>>,
    pub database: Option<Node<'a>>,
    /// The node of the query, for an item about a member that is absent.
    pub query: Node<'a>,
}

/// Prepares the object of 7.29 from the members of the plugin. The class of each fault follows 15.6:
/// the result is `None` when the panel is removed.
pub(crate) fn prepare_mongo(ctx: &mut Ctx, m: &MongoMembers<'_>, vars: &BTreeSet<String>) -> Option<Sql> {
    prepare_mongo_with(ctx, m, vars, &Fallback::PanelRemoved)
}

/// As [`prepare_mongo`], with the fallback of each fault: `panel removed` for a panel, `item removed` for an
/// annotation (15.6). A query above `MAX_SQL_BYTES` keeps the fallback "the dashboard is not imported", as SQL does.
pub(crate) fn prepare_mongo_with(
    ctx: &mut Ctx,
    m: &MongoMembers<'_>,
    vars: &BTreeSet<String>,
    removed: &Fallback,
) -> Option<Sql> {
    let Some(node) = m.query_text.as_ref() else {
        ctx.unsupported(&m.query.ptr, Some(m.query.v), "the query has no queryText", removed.clone());
        return None;
    };
    let Some(raw) = node.str() else {
        ctx.unsupported(&node.ptr, Some(node.v), "queryText must be a text", removed.clone());
        return None;
    };
    if raw.trim().is_empty() {
        ctx.unsupported(&node.ptr, Some(node.v), "the queryText is empty", removed.clone());
        return None;
    }
    let collection = match m.collection.as_ref().filter(|c| !c.is_null()) {
        Some(c) => match c.str().filter(|t| !t.is_empty()) {
            Some(t) => {
                ctx.ok();
                t
            }
            None => {
                ctx.unsupported(&c.ptr, Some(c.v), "the collection must be a text that is not empty", removed.clone());
                return None;
            }
        },
        None => {
            ctx.unsupported(&m.query.ptr, Some(m.query.v), "the query has no collection", removed.clone());
            return None;
        }
    };
    let database = match m.database.as_ref().filter(|d| !d.is_null()) {
        Some(d) => match d.str() {
            Some("") => {
                ctx.meta();
                None
            }
            Some(t) => {
                ctx.ok();
                Some(t)
            }
            None => {
                ctx.unsupported(&d.ptr, Some(d.v), "the database must be a text", removed.clone());
                return None;
            }
        },
        None => None,
    };
    let rewritten = rewrite(raw, vars);
    let text = apply(raw, &rewritten.edits);
    if rewritten.wrappers > 0 {
        ctx.changed(
            &node.ptr,
            Some(node.v),
            &format!(
                "{} wrapper(s) of $__from or $__to are written as the macros $__timeFrom and $__timeTo (7.34)",
                rewritten.wrappers
            ),
        );
    }
    if rewritten.bare > 0 {
        ctx.changed(
            &node.ptr,
            Some(node.v),
            &format!(
                "{} bare token(s) $__from or $__to are written as a string: Hune gives the Int64 as a typed value (7.34)",
                rewritten.bare
            ),
        );
    }
    if !rewritten.brackets.is_empty() {
        ctx.changed(
            &node.ptr,
            Some(node.v),
            &format!("the old form [[name]] is rewritten to ${{name}} for: {}", rewritten.brackets.join(", ")),
        );
    }
    let parsed = match parse_text(&text) {
        Ok(p) => p,
        Err(e) => {
            let (line, column) = line_column(&text, e.offset);
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                &format!(
                    "queryText is not a JSON text of the profile of 7.28 (Hune reads no shell syntax and no JavaScript): {}, line {line}, column {column}",
                    e.message
                ),
                removed.clone(),
            );
            return None;
        }
    };
    let MongoValue::Array(stages) = parsed.value else {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "queryText must be one JSON array of stages: another form has no row in 15.6",
            removed.clone(),
        );
        return None;
    };
    if stages.is_empty() {
        ctx.unsupported(&node.ptr, Some(node.v), "the pipeline has no stage", removed.clone());
        return None;
    }
    let lines = object_lines(collection, database, &stages);
    let text = lines.join("\n");
    if text.len() > MAX_SQL_BYTES {
        ctx.unsupported(
            &node.ptr,
            None,
            &format!("the query has {} bytes: the limit is {MAX_SQL_BYTES}", text.len()),
            Fallback::DashboardNotImported,
        );
        return None;
    }
    ctx.ok();
    Some(Sql {
        lines,
        text,
        pointer: node.ptr.clone(),
        code_uses: BTreeSet::new(),
        code_raw_uses: BTreeSet::new(),
        kind: SourceKind::MongoDb,
    })
}

/// Tells if a prepared query names the variable `name` in a placeholder of 7.6: `$name`, `${name}` or
/// `${name:format}`. The scan is the placeholder scan of `hune-core`, not a search for a text, so `$names` does
/// not name `name`. 7.34 has formats that 7.9 does not list: `number`, `date`, `oid`, `bool`, `json_number` and
/// `json_oid`. Thus `${name:x}` counts for each format name `x`. The old form `[[name]]` is `${name}` in the
/// text: [`prepare_mongo`] rewrote it.
pub(crate) fn names_variable(text: &str, name: &str) -> bool {
    let mut p = 0;
    while let Some(found) = placeholders::next(text, p, text.len()) {
        p = found.end();
        let found_name = match &found {
            Found::Placeholder(ph) => ph.name,
            Found::BadBrace { start, fault: BraceFault::UnknownFormat(format), .. } if !format.is_empty() => {
                let rest = &text[start + 2..];
                &rest[..rest.bytes().position(|b| !is_name_byte(b)).unwrap_or(rest.len())]
            }
            Found::BadBrace { .. } => continue,
        };
        if found_name == name {
            return true;
        }
    }
    false
}

/// The faults and the changes that the checks of 7.36 find in a prepared query.
pub(crate) struct Checked {
    /// The reason of each fault, with the line and the column.
    pub faults: Vec<String>,
    /// The reason of each warning that the importer lists as `changed`.
    pub changes: Vec<String>,
}

/// Runs the checks of 7.36 on the lines of a prepared query. `lists` are the lists of the block when the
/// command has a configuration. Without them, the checks of the collections and the databases wait for the
/// save: the item L8 is not a fault here (15.6).
pub(crate) fn check(sql: &Sql, defs: &[VariableDef], usage: SqlUse, lists: Option<&MongoLists>) -> Checked {
    use hune_core::error::ErrorCode;
    let empty = MongoLists::default();
    let l = lists.unwrap_or(&empty);
    let ctx = SaveContext { variables: defs, collections: &l.collections, databases: &l.databases, usage };
    let result = save_checks(&sql.lines, &ctx);
    let faults = result
        .faults
        .iter()
        .filter(|f| lists.is_some() || f.code != ErrorCode::BadReference)
        .map(|f| format!("line {}, column {}: {}", f.line_index + 1, f.column, f.message))
        .collect();
    let changes = result
        .warnings
        .iter()
        .filter(|w| w.kind == WarnKind::MacroInText)
        .map(|w| format!("line {}, column {}: {}", w.line_index + 1, w.column, w.message))
        .collect();
    Checked { faults, changes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_wrapper_the_bare_token_and_the_double_bracket_are_found_outside_and_inside_strings() {
        let text = r#"[{"$match":{"a":{"$gte":{"$date":{"$numberLong":"$__from"}},"$lt":$__to},"b":"[[x]] [[y]]","c":"$__from"}}]"#;
        let r = rewrite(text, &vars(&["x"]));
        assert_eq!((r.wrappers, r.bare, r.brackets.clone()), (1, 1, vec!["x".to_owned()]));
        let out = apply(text, &r.edits);
        assert_eq!(out, r#"[{"$match":{"a":{"$gte":"$__timeFrom","$lt":"$__to"},"b":"${x} [[y]]","c":"$__from"}}]"#);
        assert!(parse_text(&out).is_ok());
    }

    #[test]
    fn a_text_with_a_string_that_has_no_end_gets_no_edit() {
        let r = rewrite(r#"[{"a": "$__from"#, &vars(&[]));
        assert!(r.edits.is_empty());
    }

    #[test]
    fn the_lines_are_one_for_each_key_and_one_for_each_stage() {
        let stages = match parse_text(r#"[{"$match":{"a":1}},{"$limit":5}]"#).unwrap().value {
            MongoValue::Array(a) => a,
            _ => unreachable!(),
        };
        assert_eq!(
            object_lines("c", Some("d"), &stages),
            [
                "{",
                "  \"collection\": \"c\",",
                "  \"database\": \"d\",",
                "  \"aggregate\": [",
                "    {\"$match\":{\"a\":1}},",
                "    {\"$limit\":5}",
                "  ]",
                "}"
            ]
        );
    }

    #[test]
    fn a_variable_is_named_in_each_placeholder_form_and_only_there() {
        for text in [
            r#"{"a": "$env"}"#,
            r#"{"a": "${env}"}"#,
            r#"{"a": "x ${env:csv}"}"#,
            r#"{"a": "${env:json}"}"#,
            r#"{"a": "${env:number}"}"#,
            r#"{"a": "${env:json_oid}"}"#,
            r#"{"a": "$other", "b": "${env:date}"}"#,
        ] {
            assert!(names_variable(text, "env"), "{text}");
        }
        for text in [
            r#"{"a": "$envx"}"#,
            r#"{"a": "${envx}"}"#,
            r#"{"a": "env"}"#,
            r#"{"a": "[[env]]"}"#,
            r#"{"a": "${env:}"}"#,
            r#"{"a": "${env x}"}"#,
            r#"{"a": "$_env"}"#,
            r#"{"a": "${"}"#,
        ] {
            assert!(!names_variable(text, "env"), "{text}");
        }
    }

    #[test]
    fn the_line_and_the_column_count_characters() {
        assert_eq!(line_column("ab\néc", 5), (2, 2));
        assert_eq!(line_column("x", 99), (1, 2));
    }
}
