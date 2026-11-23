// SPDX-License-Identifier: MIT OR Apache-2.0
//! The eleven transformations of the Grafana-parity work (15.6, 4.25a). Each function reads the options of one Grafana
//! transformation and gives the item of the member `transforms`, or `None` when the transformation has no item.
//!
//! The classes follow the measure on Grafana 12.4.2 (19.20j): an option with no measured effect is `dropped`, and
//! an option that Hune cannot copy and that changes the result is `unsupported` with its fallback.

use std::sync::LazyLock;

use hune_core::document::{PatternForm, check_pattern};
use hune_core::limits::{
    MAX_CONVERSIONS, MAX_LIMIT_COUNT, MAX_PARTITION_FIELDS, MAX_SORT_KEYS, MAX_TRANSFORM_ITEMS,
    MAX_TRANSFORM_NAME_CHARS, MAX_TRANSFORM_REDUCERS, MAX_VALUE_BYTES,
};
use serde_json::Value;

use crate::item::{Ctx, Fallback, is_trivial};
use crate::out::Out;
use crate::util::{as_flag, as_num, chars};
use crate::walk::{Node, Walk};

fn removed() -> Fallback {
    Fallback::other("transformation removed")
}

static EMPTY: LazyLock<Value> = LazyLock::new(|| Value::Object(serde_json::Map::new()));

/// Opens the options. An absent or `null` value is an empty object. A value of another type is `unsupported`.
fn open<'a>(ctx: &mut Ctx, options: Option<Node<'a>>, at: &Node<'a>, id: &str) -> Option<Walk<'a>> {
    match options {
        Some(o) if !o.is_null() => {
            let walk = Walk::new(&o);
            if walk.is_none() {
                ctx.unsupported(&o.ptr, Some(o.v), &format!("the options of {id} must be an object"), removed());
            }
            walk
        }
        _ => Walk::new(&Node { v: &EMPTY, ptr: at.child_ptr("options") }),
    }
}

/// A name of 1 to 1024 bytes with no control character (4.25a).
fn valid_name(t: &str) -> bool {
    !t.is_empty() && t.len() <= MAX_VALUE_BYTES && !t.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
}

/// A text of 0 to 256 characters with no control character.
fn valid_short(t: &str, min: usize) -> bool {
    (min..=MAX_TRANSFORM_NAME_CHARS).contains(&chars(t)) && !t.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
}

/// Lists a member that has no effect: nothing when the value is trivial, else a `dropped` item.
fn no_effect(ctx: &mut Ctx, n: &Node, reason: &str) {
    if is_trivial(n.v) {
        ctx.meta();
    } else {
        ctx.dropped(&n.ptr, Some(n.v), reason);
    }
}

/// Reads a field name. A text that is not a name is `unsupported` and removes the transformation.
fn name_of(ctx: &mut Ctx, n: &Node, what: &str) -> Option<String> {
    match n.str() {
        Some(t) if valid_name(t) => Some(t.to_owned()),
        _ => {
            ctx.unsupported(&n.ptr, Some(n.v), &format!("{what} must be a name of 1 to 1024 bytes"), removed());
            None
        }
    }
}

/// Reads an array of names of 0 to `max` items. A duplicate is removed and listed as `changed`.
fn names_of(ctx: &mut Ctx, n: &Node, max: usize, what: &str) -> Option<Vec<String>> {
    let items = n.items();
    if n.v.as_array().is_none() || items.len() > max {
        ctx.unsupported(&n.ptr, Some(n.v), &format!("{what} must be an array of at most {max} names"), removed());
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for item in &items {
        let name = name_of(ctx, item, what)?;
        if out.contains(&name) {
            ctx.changed(&item.ptr, Some(item.v), "the name is twice in the list: the second one is removed");
        } else {
            out.push(name);
        }
    }
    Some(out)
}

/// The members of an object that holds a name and a value, in the order of the keys. `None` after an item.
fn entries<'a>(ctx: &mut Ctx, n: &Node<'a>, what: &str) -> Option<Vec<(String, Node<'a>)>> {
    if n.is_null() {
        return Some(Vec::new());
    }
    let Some(map) = n.v.as_object() else {
        ctx.unsupported(&n.ptr, Some(n.v), &format!("{what} must be an object"), removed());
        return None;
    };
    Some(map.iter().map(|(k, v)| (k.clone(), Node { v, ptr: n.child_ptr(k) })).collect())
}

// ------------------------------------------------------------------ organize

pub(crate) fn organize(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "organize")?;
    let exclude = w.take("excludeByName");
    let index = w.take("indexByName");
    let rename = w.take("renameByName");
    let include = w.take("includeByName");
    w.finish(ctx);
    if let Some(i) = include.as_ref() {
        if is_trivial(i.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&i.ptr, Some(i.v), "includeByName has no equal in format 3", Fallback::MemberRemoved);
        }
    }
    let mut hide: Vec<String> = Vec::new();
    for (name, v) in entries(ctx, exclude.as_ref().unwrap_or(&null_node(at)), "excludeByName")? {
        match as_flag(v.v) {
            Some(true) if valid_name(&name) => hide.push(name),
            Some(false) => ctx.meta(),
            _ => {
                ctx.unsupported(
                    &v.ptr,
                    Some(v.v),
                    "an excludeByName item must be a name with true or false",
                    removed(),
                );
                return None;
            }
        }
    }
    let mut order: Vec<(f64, String)> = Vec::new();
    let mut tie = false;
    for (name, v) in entries(ctx, index.as_ref().unwrap_or(&null_node(at)), "indexByName")? {
        match as_num(v.v) {
            Some(i) if valid_name(&name) => {
                tie |= order.iter().any(|(j, _)| *j == i);
                order.push((i, name));
            }
            _ => {
                ctx.unsupported(&v.ptr, Some(v.v), "an indexByName item must be a name with a number", removed());
                return None;
            }
        }
    }
    if tie && let Some(i) = index.as_ref() {
        ctx.changed(&i.ptr, Some(i.v), "equal indexes: the names come in the order of the keys of the file");
    }
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut renames: Vec<Out> = Vec::new();
    for (name, v) in entries(ctx, rename.as_ref().unwrap_or(&null_node(at)), "renameByName")? {
        match v.str() {
            Some("") => ctx.dropped(&v.ptr, Some(v.v), "an empty new name has no effect"),
            Some(t) if valid_name(&name) && valid_short(t, 1) => {
                renames.push(Out::obj([("field", Out::Str(name)), ("name", Out::s(t))]));
            }
            _ => {
                ctx.unsupported(&v.ptr, Some(v.v), "a new name must be 1 to 256 characters", removed());
                return None;
            }
        }
    }
    if hide.len() > MAX_TRANSFORM_ITEMS || order.len() > MAX_TRANSFORM_ITEMS || renames.len() > MAX_TRANSFORM_ITEMS {
        ctx.unsupported(&at.ptr, Some(at.v), "organize has more than 64 names in a list", removed());
        return None;
    }
    ctx.ok();
    let order: Vec<String> = order.into_iter().map(|(_, n)| n).collect();
    Some(Out::obj([
        ("kind", Out::s("organize")),
        ("hide", Out::strings(&hide)),
        ("order", Out::strings(&order)),
        ("rename", Out::Arr(renames)),
    ]))
}

/// A `null` node: the value of an absent member.
fn null_node<'a>(at: &Node<'a>) -> Node<'a> {
    static NULL: Value = Value::Null;
    Node { v: &NULL, ptr: at.ptr.clone() }
}

// ------------------------------------------------------------------ calculateField

/// The calculations of 4.26 that a Grafana reducer id gives.
pub(crate) fn reducer(id: &str) -> Option<&'static str> {
    Some(match id {
        "lastNotNull" => "last_not_null",
        "last" => "last",
        "firstNotNull" => "first_not_null",
        "first" => "first",
        "min" => "min",
        "max" => "max",
        "mean" => "mean",
        "sum" => "total",
        "count" => "count",
        "median" => "median",
        "stdDev" => "std_dev",
        "variance" => "variance",
        "range" => "range",
        "diff" => "diff",
        "diffperc" => "diff_percent",
        "delta" => "delta",
        "distinctCount" => "distinct_count",
        "changeCount" => "change_count",
        _ => return None,
    })
}

/// Tells if a text is a number in the form of the rule of 9.21a, rule 12 (a decimal number with an optional exponent).
fn is_number_text(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let whole = digits(i);
    i += whole;
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        frac = digits(i + 1);
        i += 1 + frac;
    }
    if whole + frac == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        j += usize::from(matches!(b.get(j), Some(b'+' | b'-')));
        let e = digits(j);
        if e == 0 {
            return false;
        }
        i = j + e;
    }
    i == b.len() && t.parse::<f64>().is_ok_and(f64::is_finite)
}

/// An operand of `binary`: a field name or a number.
fn operand(ctx: &mut Ctx, n: &Node) -> Option<Out> {
    let field = |ctx: &mut Ctx, at: &Node, t: &str| -> Option<Out> {
        if t.is_empty() {
            ctx.unsupported(&at.ptr, Some(at.v), "an empty operand gives no new field in Grafana", removed());
            None
        } else if t.contains('$') || !valid_name(t) {
            ctx.unsupported(&at.ptr, Some(at.v), "an operand with $ or no valid name has no equal", removed());
            None
        } else {
            Some(Out::obj([("kind", Out::s("field")), ("field", Out::s(t))]))
        }
    };
    let number = |v: f64| Some(Out::obj([("kind", Out::s("number")), ("value", Out::Num(v))]));
    match n.v {
        Value::String(t) if is_number_text(t) => number(t.parse::<f64>().ok()?),
        Value::String(t) => field(ctx, n, t),
        Value::Number(_) => number(as_num(n.v)?),
        Value::Object(m) => {
            if let Some(fixed) = m.get("fixed") {
                return match fixed {
                    Value::String(t) if is_number_text(t) => number(t.parse::<f64>().ok()?),
                    Value::Number(_) => number(as_num(fixed)?),
                    _ => {
                        ctx.unsupported(&n.ptr, Some(n.v), "a fixed operand must be a number", removed());
                        None
                    }
                };
            }
            let matcher = m.get("matcher").and_then(Value::as_object);
            let by_name = matcher.is_some_and(|x| x.get("id").and_then(Value::as_str) == Some("byName"));
            match matcher.and_then(|x| x.get("options")).and_then(Value::as_str) {
                Some(t) if by_name => field(ctx, n, t),
                _ => {
                    ctx.unsupported(&n.ptr, Some(n.v), "only the matcher byName has an equal as an operand", removed());
                    None
                }
            }
        }
        _ => {
            ctx.unsupported(&n.ptr, Some(n.v), "this operand has no equal in format 3", removed());
            None
        }
    }
}

pub(crate) fn calculate_field(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "calculateField")?;
    let mode = w.take("mode");
    let alias = w.take("alias");
    let replace = w.take("replaceFields");
    let binary = w.take("binary");
    let reduce = w.take("reduce");
    let unary = w.take("unary");
    let index = w.take("index");
    let ignore = w.take("ignoreNulls");
    let null_mode = w.take("nullValueMode");
    let series = w.take("timeSeries");
    let cumulative = w.take("cumulative");
    let window = w.take("window");
    w.finish(ctx);
    let mode_text = mode.as_ref().and_then(Node::str);
    let mode_at = mode.clone().unwrap_or_else(|| at.clone());
    let key = match mode_text {
        Some("binary") => "binary",
        Some("reduceRow") => "reduce_row",
        Some("unary") => "unary",
        Some("index") => "index",
        Some(m @ ("cumulativeFunctions" | "windowFunctions")) => {
            ctx.unsupported(
                &mode_at.ptr,
                Some(mode_at.v),
                &format!("the mode {m} has no equal in format 3"),
                removed(),
            );
            return None;
        }
        _ => {
            ctx.unsupported(&mode_at.ptr, Some(mode_at.v), "calculateField needs a mode that has an equal", removed());
            return None;
        }
    };
    // The objects of the other modes have no effect.
    for (name, node) in [("binary", &binary), ("reduce", &reduce), ("unary", &unary), ("index", &index)] {
        let used = matches!(
            (key, name),
            ("binary", "binary") | ("reduce_row", "reduce") | ("unary", "unary") | ("index", "index")
        );
        if !used && node.is_some() {
            ctx.meta();
        }
    }
    for _ in [&cumulative, &window].into_iter().flatten() {
        // The members of the modes that Hune does not have: they have no effect in the mode that was read.
        ctx.meta();
    }
    if let Some(s) = series.as_ref() {
        no_effect(ctx, s, "the member timeSeries has no member in format 3");
    }
    if let Some(n) = ignore.as_ref() {
        no_effect(ctx, n, "ignoreNulls has no effect (measured)");
    }
    let alias_out = match alias.as_ref() {
        None => Out::Null,
        Some(a) if a.is_null() => Out::Null,
        Some(a) => match a.str() {
            Some("") => Out::Null,
            Some(t) if valid_short(t, 1) => Out::s(t),
            _ => {
                ctx.unsupported(&a.ptr, Some(a.v), "an alias must be 1 to 256 characters", removed());
                return None;
            }
        },
    };
    let replace_out = match replace.as_ref() {
        None => false,
        Some(r) => match as_flag(r.v) {
            Some(b) => b,
            None => {
                ctx.unsupported(&r.ptr, Some(r.v), "replaceFields must be true or false", removed());
                return None;
            }
        },
    };
    // `nullValueMode` is at the top or in the object `reduce`.
    let mut null_nodes: Vec<Node> = null_mode.into_iter().collect();
    let object = match key {
        "binary" => {
            let b = binary.filter(|b| b.v.is_object());
            let Some(b) = b else {
                ctx.unsupported(&at.ptr, Some(at.v), "mode binary needs the object binary", removed());
                return None;
            };
            let mut bw = Walk::new(&b)?;
            let left = bw.take("left");
            let op = bw.take("operator");
            let right = bw.take("right");
            bw.finish(ctx);
            let operator = match op.as_ref().and_then(Node::str) {
                Some("+") => "add",
                Some("-") => "subtract",
                Some("*") => "multiply",
                Some("/") => "divide",
                _ => {
                    let o = op.unwrap_or_else(|| b.clone());
                    ctx.unsupported(&o.ptr, Some(o.v), "this operator has no equal: use + - * or /", removed());
                    return None;
                }
            };
            let (Some(l), Some(r)) = (left, right) else {
                ctx.unsupported(&b.ptr, Some(b.v), "binary needs a left and a right operand", removed());
                return None;
            };
            let (lo, ro) = (operand(ctx, &l)?, operand(ctx, &r)?);
            let is_number =
                |o: &Out| matches!(o, Out::Obj(m) if m.first().is_some_and(|(_, k)| *k == Out::s("number")));
            if is_number(&lo) && is_number(&ro) {
                ctx.unsupported(&b.ptr, Some(b.v), "two numbers as operands have no equal in format 3", removed());
                return None;
            }
            ("binary", Out::obj([("left", lo), ("right", ro), ("operator", Out::s(operator))]))
        }
        "reduce_row" => {
            let Some(r) = reduce.filter(|r| r.v.is_object()) else {
                ctx.unsupported(&at.ptr, Some(at.v), "mode reduceRow needs the object reduce", removed());
                return None;
            };
            let mut rw = Walk::new(&r)?;
            let id = rw.take("reducer");
            let include = rw.take("include");
            let rignore = rw.take("ignoreNulls");
            null_nodes.extend(rw.take("nullValueMode"));
            rw.finish(ctx);
            if let Some(n) = rignore.as_ref() {
                no_effect(ctx, n, "ignoreNulls has no effect (measured)");
            }
            let Some(calc) = id.as_ref().and_then(Node::str).and_then(reducer) else {
                let at = id.unwrap_or_else(|| r.clone());
                ctx.unsupported(&at.ptr, Some(at.v), "this reducer has no calculation in 4.26", removed());
                return None;
            };
            let include = match include.as_ref() {
                None => Vec::new(),
                Some(i) if i.is_null() => Vec::new(),
                Some(i) => names_of(ctx, i, MAX_TRANSFORM_ITEMS, "include")?,
            };
            ("reduce", Out::obj([("reducer", Out::s(calc)), ("include", Out::strings(&include))]))
        }
        "unary" => {
            let Some(u) = unary.filter(|u| u.v.is_object()) else {
                ctx.unsupported(&at.ptr, Some(at.v), "mode unary needs the object unary", removed());
                return None;
            };
            let mut uw = Walk::new(&u)?;
            let op = uw.take("operator");
            let field = uw.take("fieldName");
            uw.finish(ctx);
            let operator = match op.as_ref().and_then(Node::str) {
                Some(o @ ("abs" | "exp" | "ln" | "floor" | "ceil" | "round" | "percent")) => o,
                _ => {
                    let o = op.unwrap_or_else(|| u.clone());
                    ctx.unsupported(&o.ptr, Some(o.v), "this operator has no equal in format 3", removed());
                    return None;
                }
            };
            let Some(f) = field else {
                ctx.unsupported(&u.ptr, Some(u.v), "unary needs a fieldName", removed());
                return None;
            };
            let field = name_of(ctx, &f, "fieldName")?;
            ("unary", Out::obj([("operator", Out::s(operator)), ("field", Out::Str(field))]))
        }
        _ => {
            let mut as_percentile = false;
            if let Some(i) = index.filter(|i| !i.is_null()) {
                let Some(mut iw) = Walk::new(&i) else {
                    ctx.unsupported(&i.ptr, Some(i.v), "index must be an object", removed());
                    return None;
                };
                let p = iw.take("asPercentile");
                iw.finish(ctx);
                if let Some(p) = p {
                    let Some(b) = as_flag(p.v) else {
                        ctx.unsupported(&p.ptr, Some(p.v), "asPercentile must be true or false", removed());
                        return None;
                    };
                    as_percentile = b;
                }
            }
            ("index", Out::obj([("as_percentile", Out::Bool(as_percentile))]))
        }
    };
    for n in &null_nodes {
        let text = n.str().unwrap_or("");
        if text == "null as zero" && key == "reduce_row" {
            ctx.unsupported(&n.ptr, Some(n.v), "the mode null as zero has no equal in format 3", removed());
            return None;
        }
        no_effect(ctx, n, "nullValueMode has no effect (measured)");
    }
    ctx.ok();
    Some(Out::members(vec![
        ("kind", Out::s("calculate_field")),
        ("mode", Out::s(key)),
        ("alias", alias_out),
        ("replace_fields", Out::Bool(replace_out)),
        object,
    ]))
}

// ------------------------------------------------------------------ sortBy, limit, merge

pub(crate) fn sort_by(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "sortBy")?;
    let sort = w.take("sort");
    let reverse = w.take("reverse");
    w.finish(ctx);
    if let Some(r) = reverse.as_ref() {
        no_effect(ctx, r, "reverse has no effect (measured)");
    }
    let items = match sort.as_ref() {
        None => Vec::new(),
        Some(s) if s.is_null() => Vec::new(),
        Some(s) if s.v.as_array().is_some() => s.items(),
        Some(s) => {
            ctx.unsupported(&s.ptr, Some(s.v), "sort must be an array", removed());
            return None;
        }
    };
    if items.is_empty() {
        ctx.dropped(&at.ptr, Some(at.v), "an empty sort gives no transformation");
        return None;
    }
    let mut keys: Vec<(String, bool)> = Vec::new();
    for item in &items {
        let Some(mut kw) = Walk::new(item) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a sort key must be an object", removed());
            return None;
        };
        let field = kw.take("field");
        let desc = kw.take("desc");
        kw.finish(ctx);
        let Some(f) = field else {
            ctx.unsupported(&item.ptr, Some(item.v), "a sort key needs a field", removed());
            return None;
        };
        let name = name_of(ctx, &f, "field")?;
        let desc = match desc.as_ref() {
            None => false,
            Some(d) => match as_flag(d.v) {
                Some(b) => b,
                None => {
                    ctx.unsupported(&d.ptr, Some(d.v), "desc must be true or false", removed());
                    return None;
                }
            },
        };
        if keys.contains(&(name.clone(), desc)) {
            ctx.dropped(&item.ptr, Some(item.v), "the same sort key twice: the second one has no effect");
        } else {
            keys.push((name, desc));
        }
    }
    if keys.len() > MAX_SORT_KEYS {
        if let Some(s) = sort.as_ref() {
            ctx.unsupported(&s.ptr, Some(s.v), "more than 8 sort keys", Fallback::other("the first 8 keys stay"));
        }
        keys.truncate(MAX_SORT_KEYS);
    }
    ctx.ok();
    Some(Out::obj([
        ("kind", Out::s("sort_by")),
        (
            "sort",
            Out::Arr(
                keys.into_iter()
                    .map(|(field, desc)| Out::obj([("field", Out::Str(field)), ("desc", Out::Bool(desc))]))
                    .collect(),
            ),
        ),
    ]))
}

pub(crate) fn limit(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "limit")?;
    let field = w.take("limitField");
    w.finish(ctx);
    let Some(f) = field else {
        ctx.dropped(&at.ptr, Some(at.v), "a limit with no limitField keeps all rows (measured)");
        return None;
    };
    let value = match f.v {
        Value::Number(_) => as_num(f.v),
        Value::String(t) if t.contains('$') => {
            ctx.unsupported(&f.ptr, Some(f.v), "a variable in limitField has no equal in format 3", removed());
            return None;
        }
        Value::String(t) if !t.trim().is_empty() => t.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
        _ => None,
    };
    let Some(v) = value else {
        ctx.dropped(&f.ptr, Some(f.v), "a limitField that is not a number keeps all rows (measured)");
        return None;
    };
    let mut count = v.abs().floor();
    let end = if v < 0.0 { "last" } else { "first" };
    if count > MAX_LIMIT_COUNT as f64 {
        ctx.unsupported(&f.ptr, Some(f.v), "limitField above 1,000,000", Fallback::other("count = 1,000,000"));
        count = MAX_LIMIT_COUNT as f64;
    } else {
        ctx.ok();
    }
    Some(Out::obj([("kind", Out::s("limit")), ("count", Out::Int(count as i64)), ("end", Out::s(end))]))
}

pub(crate) fn merge(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let w = open(ctx, options, at, "merge")?;
    w.finish(ctx);
    ctx.ok();
    Some(Out::obj([("kind", Out::s("merge"))]))
}

// ------------------------------------------------------------------ reduce, partitionByValues

pub(crate) fn reduce(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "reduce")?;
    let mode = w.take("mode");
    let reducers = w.take("reducers");
    let fields = w.take("fields");
    let time = w.take("includeTimeField");
    let labels = w.take("labelsToFields");
    w.finish(ctx);
    for n in [&time, &labels].into_iter().flatten() {
        no_effect(ctx, n, "this option has no effect (measured)");
    }
    if let Some(f) = fields.as_ref() {
        if is_trivial(f.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&f.ptr, Some(f.v), "the option fields gives an error in Grafana 12.4.2", removed());
            return None;
        }
    }
    let mode_out = match mode.as_ref().map(|m| (m, m.str())) {
        None => "series_to_rows",
        Some((_, Some("seriesToRows"))) => "series_to_rows",
        Some((_, Some("reduceFields"))) => "reduce_fields",
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "this mode has no equal in format 3", removed());
            return None;
        }
    };
    let mut calcs: Vec<&'static str> = Vec::new();
    match reducers.as_ref() {
        None => calcs.push("max"),
        Some(r) if r.v.as_array().is_some() => {
            let items = r.items();
            if items.is_empty() {
                ctx.dropped(&r.ptr, Some(r.v), "a reduce with no reducer does not change the frame (measured)");
                return None;
            }
            for item in &items {
                match item.str().and_then(reducer) {
                    Some(c) if calcs.contains(&c) => {
                        ctx.changed(
                            &item.ptr,
                            Some(item.v),
                            "the reducer is twice in the list: the second one is removed",
                        );
                    }
                    Some(c) => calcs.push(c),
                    None => ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "this reducer has no calculation in 4.26",
                        Fallback::other("reducer removed"),
                    ),
                }
            }
            if calcs.is_empty() {
                ctx.unsupported(&r.ptr, Some(r.v), "no reducer is left in the list", removed());
                return None;
            }
        }
        Some(r) => {
            ctx.unsupported(&r.ptr, Some(r.v), "reducers must be an array", removed());
            return None;
        }
    }
    debug_assert!(calcs.len() <= MAX_TRANSFORM_REDUCERS);
    ctx.ok();
    Some(Out::obj([("kind", Out::s("reduce")), ("mode", Out::s(mode_out)), ("reducers", Out::strings(&calcs))]))
}

pub(crate) fn partition_by_values(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "partitionByValues")?;
    let fields = w.take("fields");
    let keep = w.take("keepFields");
    let naming = w.take("naming");
    w.finish(ctx);
    let Some(f) = fields.filter(|f| f.v.as_array().is_some_and(|a| !a.is_empty())) else {
        ctx.unsupported(&at.ptr, Some(at.v), "partitionByValues needs 1 to 4 fields", removed());
        return None;
    };
    let names = names_of(ctx, &f, MAX_PARTITION_FIELDS, "fields")?;
    let keep_fields = match keep.as_ref() {
        None => false,
        Some(k) => match as_flag(k.v) {
            Some(b) => b,
            None => {
                ctx.unsupported(&k.ptr, Some(k.v), "keepFields must be true or false", removed());
                return None;
            }
        },
    };
    if let Some(n) = naming.as_ref().filter(|n| !n.is_null()) {
        let Some(mut nw) = Walk::new(n) else {
            ctx.unsupported(&n.ptr, Some(n.v), "naming must be an object", removed());
            return None;
        };
        if let Some(a) = nw.take("asLabels")
            && as_flag(a.v) == Some(false)
        {
            ctx.changed(&a.ptr, Some(a.v), "Hune always makes labels");
        }
        for rest in nw.unread() {
            no_effect(ctx, &rest, "this naming option has no member in format 3");
        }
    }
    // The class of the row is `changed` (19.20j): Hune splits each frame, and Grafana splits the first frame only.
    ctx.changed(&at.ptr, Some(at.v), "Hune splits each frame; Grafana splits the first frame only (19.20j)");
    Some(Out::obj([
        ("kind", Out::s("partition_by_values")),
        ("fields", Out::strings(&names)),
        ("keep_fields", Out::Bool(keep_fields)),
    ]))
}

// ------------------------------------------------------------------ joinByField

pub(crate) fn join_by_field(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "joinByField")?;
    let by = w.take("byField");
    let mode = w.take("mode");
    w.finish(ctx);
    let field = match by.as_ref() {
        None => Out::Null,
        Some(b) if b.is_null() || b.str() == Some("") => Out::Null,
        Some(b) => Out::Str(name_of(ctx, b, "byField")?),
    };
    let mode_out = match mode.as_ref().map(|m| (m, m.str())) {
        None | Some((_, Some("outer"))) => "outer",
        Some((_, Some("inner"))) => "inner",
        Some((m, Some("outerTabular"))) => {
            ctx.unsupported(&m.ptr, Some(m.v), "the mode outerTabular has no equal in format 3", removed());
            return None;
        }
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "this mode has no equal in format 3", removed());
            return None;
        }
    };
    ctx.ok();
    Some(Out::obj([("kind", Out::s("join_by_field")), ("field", field), ("mode", Out::s(mode_out))]))
}

// ------------------------------------------------------------------ filterFieldsByName, renameByRegex

/// The object `include` or `exclude`: the members `names` and `pattern`.
fn name_filter(ctx: &mut Ctx, n: Option<Node>, what: &str) -> Option<Out> {
    let empty = || Out::obj([("names", Out::Arr(Vec::new())), ("pattern", Out::Null)]);
    let Some(n) = n.filter(|n| !n.is_null()) else { return Some(empty()) };
    let Some(mut w) = Walk::new(&n) else {
        ctx.unsupported(&n.ptr, Some(n.v), &format!("{what} must be an object"), removed());
        return None;
    };
    let names = w.take("names");
    let pattern = w.take("pattern");
    let variable = w.take("variable");
    w.finish(ctx);
    if let Some(v) = variable.as_ref() {
        if is_trivial(v.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&v.ptr, Some(v.v), "a variable has no equal in format 3", removed());
            return None;
        }
    }
    let list = match names.as_ref() {
        None => Vec::new(),
        Some(l) if l.is_null() => Vec::new(),
        Some(l) => names_of(ctx, l, MAX_TRANSFORM_ITEMS, "names")?,
    };
    let pattern_out = match pattern.as_ref() {
        None => Out::Null,
        Some(p) if p.is_null() || p.str() == Some("") => Out::Null,
        Some(p) => match p.str() {
            Some(t) if check_pattern(t, PatternForm::Grafana).is_ok() => Out::s(t),
            _ => {
                ctx.unsupported(&p.ptr, Some(p.v), "the pattern is refused by 4.24", removed());
                return None;
            }
        },
    };
    Some(Out::obj([("names", Out::strings(&list)), ("pattern", pattern_out)]))
}

pub(crate) fn filter_fields_by_name(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "filterFieldsByName")?;
    let include = w.take("include");
    let exclude = w.take("exclude");
    let by_variable = w.take("byVariable");
    let variable = w.take("variable");
    w.finish(ctx);
    for v in [&by_variable, &variable].into_iter().flatten() {
        if is_trivial(v.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&v.ptr, Some(v.v), "a variable has no equal in format 3", removed());
            return None;
        }
    }
    let include = name_filter(ctx, include, "include")?;
    let exclude = name_filter(ctx, exclude, "exclude")?;
    ctx.ok();
    Some(Out::obj([("kind", Out::s("filter_fields_by_name")), ("include", include), ("exclude", exclude)]))
}

pub(crate) fn rename_by_regex(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "renameByRegex")?;
    let regex = w.take("regex");
    let pattern = w.take("renamePattern");
    w.finish(ctx);
    let Some(r) = regex.filter(|r| !r.is_null()) else {
        ctx.dropped(&at.ptr, Some(at.v), "a renameByRegex with no regex does not change a name (measured)");
        return None;
    };
    let Some(text) = r.str().filter(|t| check_pattern(t, PatternForm::Grafana).is_ok()) else {
        ctx.unsupported(&r.ptr, Some(r.v), "the regex is refused by 4.24", removed());
        return None;
    };
    let replacement = match pattern.as_ref().and_then(|p| p.str().map(|t| (p, t))) {
        Some((_, t)) if valid_short(t, 0) => t.to_owned(),
        Some((p, _)) => {
            ctx.unsupported(&p.ptr, Some(p.v), "renamePattern must be 0 to 256 characters", removed());
            return None;
        }
        None => {
            ctx.unsupported(&at.ptr, Some(at.v), "renameByRegex needs a renamePattern that is a text", removed());
            return None;
        }
    };
    ctx.ok();
    Some(Out::obj([
        ("kind", Out::s("rename_by_regex")),
        ("regex", Out::s(text)),
        ("rename_pattern", Out::Str(replacement)),
    ]))
}

// ------------------------------------------------------------------ convertFieldType

pub(crate) fn convert_field_type(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let mut w = open(ctx, options, at, "convertFieldType")?;
    let conversions = w.take("conversions");
    w.finish(ctx);
    let Some(c) = conversions.filter(|c| c.v.as_array().is_some()) else {
        ctx.unsupported(&at.ptr, Some(at.v), "convertFieldType needs an array conversions", removed());
        return None;
    };
    let mut out: Vec<Out> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    for item in c.items() {
        let Some(mut iw) = Walk::new(&item) else {
            ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "a conversion must be an object",
                Fallback::other("conversion removed"),
            );
            continue;
        };
        let target = iw.take("targetField");
        let to = iw.take("destinationType");
        let format = iw.take("dateFormat");
        let join = iw.take("joinWith");
        let enum_config = iw.take("enumConfig");
        iw.finish(ctx);
        let gone = || Fallback::other("conversion removed");
        let mut unsupported_member = false;
        for n in [&format, &join, &enum_config].into_iter().flatten() {
            if is_trivial(n.v) {
                ctx.meta();
            } else {
                ctx.unsupported(&n.ptr, Some(n.v), "this option has no equal in format 3", gone());
                unsupported_member = true;
            }
        }
        let dest = match to.as_ref().and_then(Node::str) {
            Some("string") => "text",
            Some("number") => "number",
            Some("time") => "time",
            Some("boolean") => "boolean",
            _ => {
                let at = to.clone().unwrap_or_else(|| item.clone());
                ctx.unsupported(&at.ptr, Some(at.v), "this destination type has no equal in format 3", gone());
                continue;
            }
        };
        let Some(field) = target.as_ref().and_then(Node::str).filter(|t| valid_name(t)) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a conversion needs a targetField of 1 to 1024 bytes", gone());
            continue;
        };
        if unsupported_member {
            continue;
        }
        if fields.iter().any(|f| f == field) {
            ctx.unsupported(&item.ptr, Some(item.v), "a second conversion of the same field has no equal", gone());
            continue;
        }
        if out.len() >= MAX_CONVERSIONS {
            ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "more than 16 conversions",
                Fallback::other("the first 16 conversions stay"),
            );
            continue;
        }
        fields.push(field.to_owned());
        out.push(Out::obj([("field", Out::s(field)), ("to", Out::s(dest))]));
    }
    if out.is_empty() {
        return None;
    }
    // The rules of the values differ from Grafana (19.20j): the class is `changed`.
    ctx.changed(&c.ptr, Some(c.v), "the rules of the values differ from Grafana (19.20j)");
    Some(Out::obj([("kind", Out::s("convert_field_type")), ("conversions", Out::Arr(out))]))
}
