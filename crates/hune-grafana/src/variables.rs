// SPDX-License-Identifier: MIT OR Apache-2.0
//! The variables (4.7 to 4.11) and the variable table of 15.6.

use std::collections::{BTreeSet, HashSet};

use hune_core::document::{PatternForm, check_pattern, is_decimal_number, is_valid_variable_name};
use hune_core::limits::{MAX_ADHOC_KEYS, MAX_INTERVALS, MAX_VALUE_BYTES, MAX_VARIABLE_VALUES};
use hune_core::sql::{
    AUTO_VALUE, ItemFault, MAX_AUTO_COUNT, MIN_AUTO_COUNT, auto_min_ms, largest_table_text_at_most, parse_interval_item,
};

use crate::dashboard::Form;
use crate::item::{Ctx, Fallback};
use crate::options::{ImportOptions, SourceKind};
use crate::out::Out;
use crate::query::{Ds, Mapped, UNMAPPED_VARIABLE, kind_of, map_source, read_ds};
use crate::sqlprep::{Sql, prepare};
use crate::util::{as_flag, as_int, chars};
use crate::walk::{Node, Walk};

/// The members of a variable after the mapping.
#[derive(Debug, Clone)]
pub(crate) enum VarKind {
    Query {
        source: String,
        sql: Sql,
        multi: bool,
        include_all: bool,
        all_value: Option<String>,
        refresh: &'static str,
        sort: &'static str,
        regex: Option<String>,
        default: Vec<String>,
        allow_custom_value: bool,
    },
    Custom {
        options: Vec<(String, String)>,
        multi: bool,
        include_all: bool,
        all_value: Option<String>,
        default: Vec<String>,
        allow_custom_value: bool,
    },
    Text {
        default: String,
        number: bool,
        pointer: String,
    },
    Interval {
        intervals: Vec<String>,
        auto: bool,
        auto_count: u32,
        auto_min: String,
        default: Option<String>,
    },
    Constant {
        value: String,
    },
    /// The kind `adhoc` (4.10c): the source and the keys. The document holds no filter.
    Adhoc {
        source: String,
        keys: Vec<String>,
    },
}

/// A variable after the mapping.
#[derive(Debug, Clone)]
pub(crate) struct VarOut {
    pub name: String,
    pub label: Option<String>,
    pub hide: &'static str,
    pub kind: VarKind,
    /// The pointer of `allowCustomValue` and the pointer of `current`, for a `custom` variable that Grafana
    /// opens with an explicit `true`. The check of the places (15.6) can make the variable closed.
    pub open_pointers: Option<(String, String)>,
}

impl VarOut {
    pub(crate) fn to_out(&self) -> Out {
        let mut m: Vec<(String, Out)> = Vec::new();
        let mut put = |k: &str, v: Out| m.push((k.to_owned(), v));
        let kind = match &self.kind {
            VarKind::Query { .. } => "query",
            VarKind::Custom { .. } => "custom",
            VarKind::Text { .. } => "text",
            VarKind::Interval { .. } => "interval",
            VarKind::Constant { .. } => "constant",
            VarKind::Adhoc { .. } => "adhoc",
        };
        put("kind", Out::s(kind));
        put("name", Out::s(&self.name));
        put("label", Out::opt_s(self.label.as_deref()));
        put("hide", Out::s(self.hide));
        match &self.kind {
            VarKind::Query {
                source,
                sql,
                multi,
                include_all,
                all_value,
                refresh,
                sort,
                regex,
                default,
                allow_custom_value,
            } => {
                put("source", Out::s(source));
                put("sql", Out::strings(&sql.lines));
                put("multi", Out::Bool(*multi));
                put("include_all", Out::Bool(*include_all));
                put("all_value", Out::opt_s(all_value.as_deref()));
                put("refresh", Out::s(refresh));
                put("sort", Out::s(sort));
                put("regex", Out::opt_s(regex.as_deref()));
                put("default", Out::strings(default));
                if *allow_custom_value {
                    put("allow_custom_value", Out::Bool(true));
                }
            }
            VarKind::Custom { options, multi, include_all, all_value, default, allow_custom_value } => {
                let opts = options.iter().map(|(t, v)| Out::obj([("text", Out::s(t)), ("value", Out::s(v))])).collect();
                put("options", Out::Arr(opts));
                put("multi", Out::Bool(*multi));
                put("include_all", Out::Bool(*include_all));
                put("all_value", Out::opt_s(all_value.as_deref()));
                put("default", Out::strings(default));
                if *allow_custom_value {
                    put("allow_custom_value", Out::Bool(true));
                }
            }
            VarKind::Text { default, number, .. } => {
                put("value_type", Out::s(if *number { "number" } else { "text" }));
                put("default", Out::s(default));
            }
            VarKind::Interval { intervals, auto, auto_count, auto_min, default } => {
                put("intervals", Out::strings(intervals));
                put("auto", Out::Bool(*auto));
                put("auto_count", Out::Int(i64::from(*auto_count)));
                put("auto_min", Out::s(auto_min));
                put("default", Out::opt_s(default.as_deref()));
            }
            VarKind::Constant { value } => put("value", Out::s(value)),
            VarKind::Adhoc { source, keys } => {
                put("source", Out::s(source));
                put("keys", Out::strings(keys));
            }
        }
        Out::Obj(m)
    }
}

fn not_imported(ctx: &mut Ctx, node: &Node, why: &str) {
    ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::DashboardNotImported);
}

fn hide_of(ctx: &mut Ctx, node: Option<&Node>) -> Option<&'static str> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some("none") };
    let r = match (node.str(), as_int(node.v)) {
        (Some("dontHide"), _) | (_, Some(0)) => "none",
        (Some("hideLabel"), _) | (_, Some(1)) => "label",
        (Some("hideVariable"), _) | (_, Some(2)) => "variable",
        _ => {
            not_imported(ctx, node, "this value of hide has no equal in format 1");
            return None;
        }
    };
    ctx.ok();
    Some(r)
}

fn sort_of(ctx: &mut Ctx, node: Option<&Node>) -> &'static str {
    let Some(node) = node.filter(|n| !n.is_null()) else { return "none" };
    let by_number = |n: i64| match n {
        0 => Some("none"),
        1 => Some("alpha_asc"),
        2 => Some("alpha_desc"),
        3 => Some("num_asc"),
        4 => Some("num_desc"),
        5 => Some("alpha_ci_asc"),
        6 => Some("alpha_ci_desc"),
        7 => Some("natural_asc"),
        8 => Some("natural_desc"),
        _ => None,
    };
    let by_name = |s: &str| match s {
        "disabled" => Some("none"),
        "alphabeticalAsc" => Some("alpha_asc"),
        "alphabeticalDesc" => Some("alpha_desc"),
        "numericalAsc" => Some("num_asc"),
        "numericalDesc" => Some("num_desc"),
        "alphabeticalCaseInsensitiveAsc" => Some("alpha_ci_asc"),
        "alphabeticalCaseInsensitiveDesc" => Some("alpha_ci_desc"),
        "naturalAsc" => Some("natural_asc"),
        "naturalDesc" => Some("natural_desc"),
        _ => None,
    };
    let r = match (node.str(), as_int(node.v)) {
        (Some(s), _) => by_name(s),
        (_, Some(n)) => by_number(n),
        _ => None,
    };
    match r {
        Some(v) => {
            ctx.ok();
            v
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "this sort has no equal in format 1 (19.18)",
                Fallback::other("none"),
            );
            "none"
        }
    }
}

fn refresh_of(ctx: &mut Ctx, node: Option<&Node>) -> Option<&'static str> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some("load") };
    match (node.str(), as_int(node.v)) {
        (Some("onDashboardLoad"), _) | (_, Some(1)) => {
            ctx.ok();
            Some("load")
        }
        (Some("onTimeRangeChanged"), _) | (_, Some(2)) => {
            ctx.ok();
            Some("time_change")
        }
        (Some("never"), _) | (_, Some(0)) => {
            ctx.changed(&node.ptr, Some(node.v), "Hune computes the options at load: Grafana keeps stored options");
            Some("load")
        }
        _ => {
            not_imported(ctx, node, "this value of refresh has no equal in format 1");
            None
        }
    }
}

fn flag(ctx: &mut Ctx, node: Option<&Node>) -> Option<bool> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(false) };
    match as_flag(node.v) {
        Some(b) => {
            ctx.ok();
            Some(b)
        }
        None => {
            not_imported(ctx, node, "the value must be true or false");
            None
        }
    }
}

/// Reads the SQL text of a query variable: a text, or an object of the plugin.
fn find_sql<'a>(ctx: &mut Ctx, query: Option<Node<'a>>, definition: Option<Node<'a>>) -> (Option<Node<'a>>, Ds<'a>) {
    let mut ds = Ds::default();
    let mut found: Option<Node<'a>> = None;
    if let Some(q) = query.filter(|q| !q.is_null()) {
        if let Some(text) = q.str() {
            if !text.is_empty() {
                found = Some(q);
            }
        } else if let Some(mut w) = Walk::new(&q) {
            found = w.take("rawSql").or_else(|| w.take("query")).filter(|n| n.str().is_some());
            ds = read_ds(ctx, w.take("datasource"));
            if let Some(spec) = w.take("spec")
                && let Some(mut sw) = Walk::new(&spec)
            {
                if found.is_none() {
                    found = sw.take("rawSql");
                }
                sw.meta(
                    ctx,
                    &[
                        "editorType",
                        "queryType",
                        "format",
                        "meta",
                        "pluginVersion",
                        "builderOptions",
                        "refId",
                        "editorMode",
                        "rawQuery",
                        "table",
                        "dataset",
                        "columns",
                        "sql",
                    ],
                );
                sw.finish(ctx);
            }
            w.meta(
                ctx,
                &[
                    "kind",
                    "group",
                    "version",
                    "editorType",
                    "queryType",
                    "format",
                    "meta",
                    "pluginVersion",
                    "builderOptions",
                    "refId",
                    "hide",
                    "editorMode",
                    "rawQuery",
                    "table",
                    "dataset",
                    "columns",
                    "sql",
                ],
            );
            w.finish(ctx);
        }
    }
    if let Some(d) = definition {
        if found.is_none() && d.str().is_some_and(|s| !s.is_empty()) {
            found = Some(d);
        } else {
            ctx.meta();
        }
    }
    (found, ds)
}

/// Parses the text of a custom variable: `a,b` or `text : value,...`. `\,` is a comma.
pub(crate) fn parse_custom_query(text: &str) -> Vec<(String, String)> {
    let mut parts: Vec<String> = vec![String::new()];
    let mut chars_iter = text.chars().peekable();
    while let Some(c) = chars_iter.next() {
        match c {
            '\\' if chars_iter.peek() == Some(&',') => {
                chars_iter.next();
                if let Some(p) = parts.last_mut() {
                    p.push(',');
                }
            }
            ',' => parts.push(String::new()),
            other => {
                if let Some(p) = parts.last_mut() {
                    p.push(other);
                }
            }
        }
    }
    parts.into_iter().map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()).map(|p| split_text_value(&p)).collect()
}

fn split_text_value(part: &str) -> (String, String) {
    let bytes = part.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b':'
            && i > 0
            && bytes[i - 1].is_ascii_whitespace()
            && bytes.get(i + 1).is_some_and(u8::is_ascii_whitespace)
        {
            let text = part[..i].trim();
            let value = part[i + 1..].trim();
            if !value.is_empty() {
                return (if text.is_empty() { value.to_owned() } else { text.to_owned() }, value.to_owned());
            }
        }
    }
    (part.to_owned(), part.to_owned())
}

fn current_values(ctx: &mut Ctx, current: Option<Node>) -> Vec<String> {
    let Some(current) = current.filter(|c| !c.is_null()) else { return Vec::new() };
    let Some(mut w) = Walk::new(&current) else {
        ctx.unsupported(&current.ptr, Some(current.v), "current must be an object", Fallback::other("an empty array"));
        return Vec::new();
    };
    let value = w.take("value");
    w.meta(ctx, &["text", "selected"]);
    w.finish(ctx);
    let Some(value) = value else { return Vec::new() };
    let mut out = Vec::new();
    match value.v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(a) => {
            for v in a {
                if let Some(s) = v.as_str() {
                    out.push(s.to_owned());
                }
            }
        }
        serde_json::Value::Null => {}
        other => out.push(other.to_string()),
    }
    out.retain(|s| !s.is_empty());
    out
}

#[allow(clippy::too_many_arguments)]
fn default_list(
    ctx: &mut Ctx,
    current: Option<Node>,
    multi: bool,
    include_all: bool,
    custom_values: Option<&[(String, String)]>,
    open: bool,
) -> Vec<String> {
    let at = current.clone();
    let vals = current_values(ctx, current);
    let empty = || Fallback::other("an empty array");
    let ptr = at.as_ref().map(|n| n.ptr.clone()).unwrap_or_default();
    let value = at.as_ref().map(|n| n.v);
    if vals.is_empty() {
        return vals;
    }
    if vals.len() > MAX_VARIABLE_VALUES || vals.iter().any(|v| v.len() > MAX_VALUE_BYTES) {
        ctx.unsupported(&ptr, value, "a default value has more than 1024 bytes or too many items", empty());
        return Vec::new();
    }
    if vals.iter().any(|v| v == "$__all") {
        if !include_all {
            ctx.unsupported(&ptr, value, "the default is All but include_all is false", empty());
            return Vec::new();
        }
        if vals.len() > 1 {
            ctx.unsupported(&ptr, value, "the item $__all must be the only item", Fallback::other("the default All"));
            return vec!["$__all".to_owned()];
        }
        ctx.ok();
        return vals;
    }
    if !multi && vals.len() > 1 {
        ctx.unsupported(&ptr, value, "a variable without multi has one default value at most", empty());
        return Vec::new();
    }
    // An open `custom` variable takes a typed default (4.11, rule 5): 1 to 1,024 bytes and no control character.
    if open && vals.iter().any(|v| v.chars().any(|c| c < '\u{20}' || c == '\u{7f}')) {
        ctx.unsupported(&ptr, value, "a default value has a control character", empty());
        return Vec::new();
    }
    if !open
        && let Some(options) = custom_values
        && vals.iter().any(|v| !options.iter().any(|(_, ov)| ov == v))
    {
        ctx.unsupported(&ptr, value, "a default value is not the value of an option", empty());
        return Vec::new();
    }
    ctx.ok();
    vals
}

fn custom_options(
    ctx: &mut Ctx,
    query: Option<&Node>,
    options: Option<&Node>,
    uses_options_member: bool,
) -> Option<Vec<(String, String)>> {
    let mut list: Vec<(String, String)> = Vec::new();
    match query.and_then(Node::str).filter(|q| !q.is_empty()) {
        Some(q) => list = parse_custom_query(q),
        None if uses_options_member => {
            if let Some(o) = options {
                for item in o.items() {
                    let text = item.get("text").and_then(|t| t.str().map(str::to_owned));
                    let value = item.get("value").and_then(|t| t.str().map(str::to_owned));
                    if let Some(v) = value.filter(|v| !v.is_empty()) {
                        list.push((text.filter(|t| !t.is_empty()).unwrap_or_else(|| v.clone()), v));
                    }
                }
            }
        }
        None => {}
    }
    let at = query.or(options)?;
    if list.is_empty() || list.len() > MAX_VARIABLE_VALUES {
        not_imported(ctx, at, "a custom variable needs 1 to 10000 options");
        return None;
    }
    let mut seen = BTreeSet::new();
    for (text, value) in &list {
        if value.len() > MAX_VALUE_BYTES || chars(text) > 256 {
            not_imported(ctx, at, "an option text has at most 256 characters and an option value at most 1024 bytes");
            return None;
        }
        if !seen.insert(value.clone()) {
            not_imported(ctx, at, &format!("two options have the value {value:?}"));
            return None;
        }
    }
    ctx.ok();
    Some(list)
}

/// Maps the list of variables. `names` holds the names of all variables of the dashboard.
pub(crate) fn convert_variables(
    ctx: &mut Ctx,
    form: Form,
    opts: &ImportOptions,
    list: Option<Node>,
    names: &BTreeSet<String>,
) -> Vec<VarOut> {
    let Some(list) = list.filter(|l| !l.is_null()) else { return Vec::new() };
    if list.v.as_array().is_none() {
        not_imported(ctx, &list, "the variable list must be an array");
        return Vec::new();
    }
    list.items().iter().filter_map(|item| convert_variable(ctx, form, opts, item, names)).collect()
}

fn convert_variable(
    ctx: &mut Ctx,
    form: Form,
    opts: &ImportOptions,
    item: &Node,
    names: &BTreeSet<String>,
) -> Option<VarOut> {
    let mut w = Walk::new(item).or_else(|| {
        not_imported(ctx, item, "a variable must be an object");
        None
    })?;
    // The data source of an `AdhocVariable` of v2beta1 is at the level of the item, with `group` (15.6).
    let mut outer_ds = None;
    let (type_node, mut sw) = if form.is_v2() {
        let kind = w.take("kind");
        let spec = w.take("spec");
        if kind.as_ref().and_then(Node::str) == Some("AdhocVariable") {
            if w.take("group").is_some() {
                ctx.meta();
            }
            outer_ds = w.take("datasource");
        }
        w.finish(ctx);
        (kind, spec.as_ref().and_then(Walk::new)?)
    } else {
        (w.take("type"), w)
    };
    let kind = match (form.is_v2(), type_node.as_ref().and_then(Node::str)) {
        (false, Some("query")) | (true, Some("QueryVariable")) => "query",
        (false, Some("custom")) | (true, Some("CustomVariable")) => "custom",
        (false, Some("textbox")) | (true, Some("TextVariable")) => "text",
        (false, Some("interval")) | (true, Some("IntervalVariable")) => "interval",
        (false, Some("constant")) | (true, Some("ConstantVariable")) => "constant",
        (false, Some("datasource")) | (true, Some("DatasourceVariable")) => {
            return data_source_variable(ctx, opts, item, &sw);
        }
        (false, Some("adhoc")) | (true, Some("AdhocVariable")) => return adhoc_variable(ctx, opts, item, sw, outer_ds),
        _ => {
            let at = type_node.as_ref().unwrap_or(item);
            not_imported(ctx, at, "this variable type is not in the measured set");
            return None;
        }
    };
    let name_node = sw.take("name");
    let label = sw.take("label");
    let description = sw.take("description");
    let hide = sw.take("hide");
    let query = sw.take("query");
    let definition = sw.take("definition");
    let datasource = sw.take("datasource");
    let multi = sw.take("multi");
    let include_all = sw.take("includeAll");
    let all_value = sw.take("allValue");
    let refresh = sw.take("refresh");
    let sort = sw.take("sort");
    let regex = sw.take("regex");
    let current = sw.take("current");
    let options = sw.take("options");
    let allow_custom = sw.take("allowCustomValue");
    let allow_pointer = sw.node.child_ptr("allowCustomValue");
    let current_pointer = sw.node.child_ptr("current");
    let hide_pointer = sw.node.child_ptr("hide");
    // The members of an `interval` variable: no other kind reads them (15.6).
    let (auto, auto_count, auto_min) = if kind == "interval" {
        (sw.take("auto"), sw.take("auto_count"), sw.take("auto_min"))
    } else {
        (None, None, None)
    };
    let hide_value = hide.clone();
    if let Some(s) = sw.take("skipUrlSync") {
        ctx.drop_member_value(&s, "skipUrlSync has no meaning in Hune");
    }
    sw.finish(ctx);
    if let Some(d) = description.as_ref() {
        ctx.drop_member_value(d, "format 1 has no description of a variable (4.7)");
    }
    let name = match name_node.as_ref().and_then(Node::str).filter(|n| is_valid_variable_name(n)) {
        Some(n) => {
            ctx.ok();
            n.to_owned()
        }
        None => {
            let at = name_node.as_ref().unwrap_or(item);
            not_imported(ctx, at, "the variable name must match the rule of 4.3");
            return None;
        }
    };
    let label = match label.as_ref().filter(|l| !l.is_null()) {
        None => None,
        Some(l) => match l.str() {
            Some("") => None,
            Some(t) if chars(t) <= 256 => {
                ctx.ok();
                Some(t.to_owned())
            }
            _ => {
                not_imported(ctx, l, "the label must be a text of at most 256 characters");
                return None;
            }
        },
    };
    let mut hide = hide_of(ctx, hide.as_ref())?;
    let mut open_pointers = None;
    let kind_out = match kind {
        "interval" => {
            // The members of the other kinds have no meaning for an interval variable: metadata.
            count_meta(
                ctx,
                &[
                    &datasource,
                    &multi,
                    &include_all,
                    &all_value,
                    &sort,
                    &regex,
                    &allow_custom,
                    &definition,
                    &refresh,
                    &options,
                ],
            );
            let intervals = interval_items(ctx, query.as_ref(), item)?;
            let auto_on = flag(ctx, auto.as_ref())?;
            let auto_count = auto_count_of(ctx, auto_count.as_ref());
            let auto_min = auto_min_of(ctx, auto_min.as_ref());
            let default = interval_default(ctx, current, &intervals, auto_on);
            VarKind::Interval { intervals, auto: auto_on, auto_count, auto_min, default }
        }
        "constant" => {
            count_meta(
                ctx,
                &[&datasource, &multi, &include_all, &all_value, &sort, &regex, &allow_custom, &definition, &refresh],
            );
            // The stored current value and options repeat the value: metadata (15.6).
            for n in [&current, &options].into_iter().flatten() {
                ctx.drop_member_value(n, "the value of a constant is the member query (15.6)");
            }
            // Hune never shows a constant, so `hide` is `variable` (4.10b). Grafana shows it for `hide` 0 and 1.
            if hide != "variable" {
                let at = hide_value.as_ref().map_or(hide_pointer.clone(), |n| n.ptr.clone());
                let reason = "Hune never shows a constant: hide is variable (4.10b)";
                ctx.changed(&at, hide_value.as_ref().map(|n| n.v), reason);
                hide = "variable";
            }
            let value = constant_value(ctx, query.as_ref())?;
            VarKind::Constant { value }
        }
        "text" => {
            // A `text` variable is always free (4.10): the member is metadata.
            if allow_custom.is_some() {
                ctx.meta();
            }
            let default_node = query
                .clone()
                .filter(|q| q.str().is_some())
                .or_else(|| current.as_ref().and_then(|c| c.get("value")).filter(|v| v.str().is_some()));
            let default = default_node.as_ref().and_then(Node::str).unwrap_or("").to_owned();
            if default.len() > MAX_VALUE_BYTES {
                not_imported(ctx, default_node.as_ref().unwrap_or(item), "the default has more than 1024 bytes");
                return None;
            }
            // `current` and `options` repeat the default: metadata.
            if current.is_some() {
                ctx.meta();
            }
            if options.is_some() {
                ctx.meta();
            }
            ctx.ok();
            let pointer = default_node.map_or_else(|| item.ptr.clone(), |n| n.ptr);
            VarKind::Text { default, number: false, pointer }
        }
        "query" => {
            let multi = flag(ctx, multi.as_ref())?;
            let include_all = flag(ctx, include_all.as_ref())?;
            let all_value = all_value_of(ctx, all_value.as_ref(), include_all)?;
            let refresh = refresh_of(ctx, refresh.as_ref())?;
            let sort = sort_of(ctx, sort.as_ref());
            let regex = regex_of(ctx, regex.as_ref());
            if options.is_some() {
                ctx.meta();
            }
            let (sql_node, inner_ds) = find_sql(ctx, query, definition);
            let mut ds =
                if datasource.as_ref().is_some_and(|d| !d.is_null()) { read_ds(ctx, datasource) } else { inner_ds };
            let (grafana_uid, source) = match ds.uid.as_deref().map(|u| map_source(ctx, opts, u)) {
                Some(Mapped::Source { key, source, ty }) => {
                    // A reference with no type takes the plugin type of the data source variable (15.9).
                    ds.ty = ds.ty.or(ty);
                    (key, source)
                }
                other => {
                    let at = ds.node.as_ref().or(sql_node.as_ref()).unwrap_or(item);
                    let why = if matches!(other, Some(Mapped::Variable)) {
                        UNMAPPED_VARIABLE
                    } else {
                        "the data source of the variable is not in the source map"
                    };
                    not_imported(ctx, at, why);
                    return None;
                }
            };
            let Some((sql_node, text)) = sql_node.as_ref().and_then(|n| n.str().map(|t| (n, t))) else {
                not_imported(ctx, item, "the query variable has no SQL text");
                return None;
            };
            let Some(dialect) = kind_of(ctx, opts, &ds, &grafana_uid, &source).dialect() else {
                // 15.6 has no row for the query of a variable of the MongoDB plugin.
                let at = ds.node.as_ref().unwrap_or(item);
                not_imported(ctx, at, "the query variable of a MongoDB data source has no row in 15.6");
                return None;
            };
            let sql = prepare(ctx, sql_node, text, names, Fallback::DashboardNotImported, dialect)?;
            let default = default_list(ctx, current, multi, include_all, None, false);
            // Grafana reads an absent member as `true`. A `query` variable is a rule of the page only.
            let allow_custom_value = allow_custom_of(ctx, allow_custom.as_ref(), true);
            VarKind::Query {
                source,
                sql,
                multi,
                include_all,
                all_value,
                refresh,
                sort,
                regex,
                default,
                allow_custom_value,
            }
        }
        _ => {
            let multi = flag(ctx, multi.as_ref())?;
            let include_all = flag(ctx, include_all.as_ref())?;
            let all_value = all_value_of(ctx, all_value.as_ref(), include_all)?;
            if refresh.is_some() {
                ctx.meta();
            }
            if sort.is_some() {
                ctx.meta();
            }
            if regex.as_ref().is_some_and(|r| !r.is_null() && r.str() != Some("")) {
                ctx.meta();
            }
            let uses_options = form.is_v2() && query.as_ref().and_then(Node::str).is_none();
            let list = custom_options(ctx, query.as_ref(), options.as_ref(), uses_options)?;
            if options.is_some() && !uses_options {
                ctx.meta();
            }
            // A closed `custom` variable is a check of the server: an absent member stays closed.
            let allow_custom_value = allow_custom_of(ctx, allow_custom.as_ref(), false);
            if allow_custom.as_ref().is_none_or(Node::is_null) {
                let reason = "Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check";
                ctx.changed(&allow_pointer, None, reason);
            } else if allow_custom_value {
                open_pointers = Some((allow_pointer, current_pointer));
            }
            let default = default_list(ctx, current, multi, include_all, Some(&list), allow_custom_value);
            VarKind::Custom { options: list, multi, include_all, all_value, default, allow_custom_value }
        }
    };
    Some(VarOut { name, label, hide, kind: kind_out, open_pointers })
}

/// Tells if a variable item is a data source variable (15.9), and gives the node that holds its members.
fn data_source_spec<'a>(form: Form, item: &Node<'a>) -> Option<Node<'a>> {
    if form.is_v2() {
        (item.get("kind")?.str() == Some("DatasourceVariable")).then(|| item.get("spec"))?
    } else {
        (item.get("type")?.str() == Some("datasource")).then(|| item.clone())
    }
}

/// The names of the data source variables, with their plugin type: `query` (v1) or `pluginId` (v2). A key of the
/// source map can name them (15.9).
pub(crate) fn data_source_names(form: Form, list: Option<&Node>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for spec in list.map(Node::items).unwrap_or_default().iter().filter_map(|i| data_source_spec(form, i)) {
        let text = |key: &str| spec.get(key).and_then(|n| n.str().map(str::to_owned));
        if let Some(name) = text("name") {
            out.push((name, text(if form.is_v2() { "pluginId" } else { "query" })));
        }
    }
    out
}

/// A data source variable (15.6, 15.9). Hune has no such variable. When the source map names it, each reference
/// gets the mapped source and the variable leaves the document, with one `changed` item. Else the dashboard is not
/// imported. The members of the variable are metadata.
fn data_source_variable(ctx: &mut Ctx, opts: &ImportOptions, item: &Node, sw: &Walk) -> Option<VarOut> {
    let spec = &sw.node;
    for _ in sw.unread() {
        ctx.meta();
    }
    let name = spec.get("name").and_then(|n| n.str().map(str::to_owned)).unwrap_or_default();
    let Some(source) = opts.source_map.get(&name) else {
        let why = format!(
            "the source map does not name the data source variable {name:?}: Hune has no data source variable (15.9)"
        );
        not_imported(ctx, item, &why);
        return None;
    };
    let shown = |key: &str| spec.get(key).map_or_else(|| "absent".to_owned(), |n| crate::item::compact_value(n.v).0);
    let why = format!(
        "the data source variable leaves the document: each reference gets the source {source} of the source map (15.9); regex {}, multi {}, includeAll {}",
        shown("regex"),
        shown("multi"),
        shown("includeAll")
    );
    ctx.changed(&item.ptr, Some(item.v), &why);
    None
}

/// The fallback of an `adhoc` variable that Hune cannot keep (15.6): the panels run with no filter.
const ADHOC_REMOVED: &str = "the variable is removed";

/// The keys that a Grafana `adhoc` variable names (15.6), and the rest of the lists after `MAX_ADHOC_KEYS` keys.
struct GrafanaKeys {
    /// The first `MAX_ADHOC_KEYS` distinct keys at most, with the pointer of the item of each.
    keys: Vec<(String, String)>,
    /// The pointer of the next distinct key and the count of the items from it to the end of the lists, when the lists
    /// name more than `MAX_ADHOC_KEYS` distinct keys.
    rest: Option<(String, usize)>,
}

/// Reads the keys that a Grafana `adhoc` variable names: the static keys of `defaultKeys` (`value`, else `text`), then
/// the keys of `filters` and of `baseFilters`, each one once, in this order (15.6).
///
/// A set finds a key that came before, and the read of keys stops at the distinct key `MAX_ADHOC_KEYS + 1`: after it,
/// the function only counts the items. So a list of many keys costs one linear pass and one item of the report, and a
/// pointer is made only for a key that the function gives (review 3, finding 2).
fn adhoc_grafana_keys(nodes: &[Option<&Node>]) -> GrafanaKeys {
    let mut out = GrafanaKeys { keys: Vec::new(), rest: None };
    let mut seen: HashSet<&str> = HashSet::new();
    for (i, node) in nodes.iter().enumerate() {
        let Some(node) = node else { continue };
        for (at, item) in node.v.as_array().into_iter().flatten().enumerate() {
            if let Some((_, count)) = out.rest.as_mut() {
                *count += 1;
                continue;
            }
            let text = |name: &str| item.get(name).and_then(serde_json::Value::as_str);
            let key = if i == 0 { text("value").or_else(|| text("text")) } else { text("key") };
            let Some(k) = key.filter(|k| seen.insert(k)) else { continue };
            let ptr = format!("{}/{at}", node.ptr);
            if out.keys.len() == MAX_ADHOC_KEYS {
                out.rest = Some((ptr, 1));
            } else {
                out.keys.push((k.to_owned(), ptr));
            }
        }
    }
    out
}

/// An `adhoc` variable (15.6, 4.10c). The variable keeps its kind, with a `changed` item: Hune applies
/// the filters only where a query has `$__adhocFilters`, with the operators `=` and `!=`, and it never wraps a query.
/// Its source comes from the source map and MUST be a ClickHouse source. Its keys are the keys that Grafana names, kept
/// when they are plain column names and, with `-c`, when the configured list of the source holds them. With no key,
/// the keys are the configured list (`changed`). Else the variable is removed (`unsupported`).
fn adhoc_variable(
    ctx: &mut Ctx,
    opts: &ImportOptions,
    item: &Node,
    mut sw: Walk,
    outer_ds: Option<Node>,
) -> Option<VarOut> {
    let removed = || Fallback::other(ADHOC_REMOVED);
    let name_node = sw.take("name");
    let label = sw.take("label");
    let hide = sw.take("hide");
    let datasource = sw.take("datasource").or(outer_ds);
    let filters = sw.take("filters");
    let base = sw.take("baseFilters");
    let default_keys = sw.take("defaultKeys");
    let description = sw.take("description");
    if let Some(s) = sw.take("skipUrlSync") {
        ctx.drop_member_value(&s, "skipUrlSync has no meaning in Hune");
    }
    // Hune has no list of values: each typed value is valid, as with `allowCustomValue` true.
    sw.meta(ctx, &["allowCustomValue", "supportsMultiValueOperators", "current", "options"]);
    sw.finish(ctx);
    if let Some(d) = description.as_ref() {
        ctx.drop_member_value(d, "format 1 has no description of a variable (4.7)");
    }
    let Some(name) = name_node.as_ref().and_then(Node::str).filter(|n| is_valid_variable_name(n)).map(str::to_owned)
    else {
        not_imported(ctx, name_node.as_ref().unwrap_or(item), "the variable name must match the rule of 4.3");
        return None;
    };
    ctx.ok();
    let label = match label.as_ref().filter(|l| !l.is_null()).map(|l| (l, l.str())) {
        None | Some((_, Some(""))) => None,
        Some((_, Some(t))) if chars(t) <= 256 => Some(t.to_owned()),
        Some((l, _)) => {
            not_imported(ctx, l, "the label must be a text of at most 256 characters");
            return None;
        }
    };
    let hide = hide_of(ctx, hide.as_ref())?;
    // The source: the map names it, and its kind is ClickHouse (4.10c).
    let ds = read_ds(ctx, datasource);
    let at = ds.node.clone().unwrap_or_else(|| item.clone());
    let source = match ds.uid.as_deref().map(|u| map_source(ctx, opts, u)) {
        Some(Mapped::Source { key, source, ty }) => {
            let ds = Ds { ty: ds.ty.clone().or(ty), ..ds };
            if kind_of(ctx, opts, &ds, &key, &source) != SourceKind::ClickHouse {
                let why = "an adhoc variable needs a ClickHouse source (4.10c)";
                ctx.unsupported(&at.ptr, Some(at.v), why, removed());
                return None;
            }
            source
        }
        _ => {
            let why = "the data source of the adhoc variable is not in the source map";
            ctx.unsupported(&at.ptr, Some(at.v), why, removed());
            return None;
        }
    };
    // The stored filters of Grafana do not apply in Hune: the document holds no filter (4.10c, rule 3).
    for (node, why) in [
        (
            &filters,
            "Hune keeps no stored filter (4.10c): its key joins keys, and the address or the bar gives the filters",
        ),
        (&base, "Hune has no base filter: it does not apply, and its key joins keys (4.10c)"),
    ] {
        match node.as_ref() {
            Some(n) if n.v.as_array().is_some_and(|a| !a.is_empty()) => ctx.changed(&n.ptr, Some(n.v), why),
            Some(_) => ctx.meta(),
            None => {}
        }
    }
    if default_keys.as_ref().is_some_and(|n| n.v.as_array().is_some_and(|a| !a.is_empty())) {
        ctx.ok();
    }
    let configured = opts.adhoc_columns.get(&source);
    let mut keys: Vec<String> = Vec::new();
    let named = adhoc_grafana_keys(&[default_keys.as_ref(), filters.as_ref(), base.as_ref()]);
    if let Some((ptr, count)) = &named.rest {
        let why = format!(
            "an adhoc variable has {MAX_ADHOC_KEYS} keys at most (4.35): Hune reads the first {MAX_ADHOC_KEYS} distinct keys, and this item and the {} items after it in the key lists are not read",
            count - 1
        );
        ctx.unsupported(ptr, None, &why, Fallback::other("the keys are removed"));
    }
    for (key, ptr) in named.keys {
        let in_list = configured.is_none_or(|list| list.contains(&key));
        if !hune_core::sql::adhoc::is_column_name(&key) || !in_list || keys.len() >= MAX_ADHOC_KEYS {
            let why = if !hune_core::sql::adhoc::is_column_name(&key) {
                "the key is not a plain column name ^[A-Za-z_][A-Za-z0-9_]{0,63}$ (4.10c)"
            } else if !in_list {
                "the key is not in the configured list adhoc_columns of the source (4.10c)"
            } else {
                "an adhoc variable has 64 keys at most (4.35)"
            };
            ctx.unsupported(
                &ptr,
                Some(&serde_json::Value::String(key.clone())),
                why,
                Fallback::other("the key is removed"),
            );
            continue;
        }
        keys.push(key);
    }
    if keys.is_empty() {
        let Some(list) = configured.filter(|l| !l.is_empty()) else {
            let why =
                "Hune needs the keys of the adhoc variable: Grafana names none, and the source has no adhoc_columns";
            ctx.unsupported(&item.ptr, None, why, removed());
            return None;
        };
        keys = list.iter().take(MAX_ADHOC_KEYS).cloned().collect();
        let why = "Grafana reads the keys from the source; Hune takes the configured list adhoc_columns of the source as keys (4.10c)";
        ctx.changed(&item.ptr, None, why);
    }
    let why = "the filters apply only in the queries that have AND $__adhocFilters (7.10a), with the operators = and != only; Hune has no setting additional_table_filters and lists no value";
    ctx.changed(&item.ptr, None, why);
    Some(VarOut { name, label, hide, kind: VarKind::Adhoc { source, keys }, open_pointers: None })
}

/// Counts the members of another kind that an `interval` or a `constant` variable carries: they have no behaviour.
fn count_meta(ctx: &mut Ctx, nodes: &[&Option<Node>]) {
    for _ in nodes.iter().filter(|n| n.as_ref().is_some_and(|n| !n.is_null())) {
        ctx.meta();
    }
}

/// The default list of an interval variable of Grafana (15.6): the list that Grafana uses for an empty text.
const GRAFANA_DEFAULT_INTERVALS: [&str; 10] = ["1m", "10m", "30m", "1h", "6h", "12h", "1d", "7d", "14d", "30d"];

/// Splits the text of an interval variable as Grafana does: at each run of characters that are not a letter, a
/// digit or `_`. A repeated item stays once, in the order of its first place.
pub(crate) fn split_interval_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|p| !p.is_empty()) {
        if !out.iter().any(|o| o == part) {
            out.push(part.to_owned());
        }
    }
    out
}

/// Reads the member `query` of an interval variable (15.6, `intervals`). An item that breaks 4.10a is removed and
/// listed. A variable with no item left stops the import of the dashboard.
fn interval_items(ctx: &mut Ctx, query: Option<&Node>, item: &Node) -> Option<Vec<String>> {
    let at = query.filter(|q| !q.is_null());
    let text = match at {
        None => "",
        Some(q) => match q.str() {
            Some(t) => t,
            None => {
                not_imported(ctx, q, "the query of an interval variable must be a text");
                return None;
            }
        },
    };
    let mut list = split_interval_list(text);
    if list.is_empty() {
        let ptr = at.map_or_else(|| item.child_ptr("query"), |q| q.ptr.clone());
        ctx.changed(&ptr, at.map(|q| q.v), "the list is empty: Hune takes the default list of Grafana");
        return Some(GRAFANA_DEFAULT_INTERVALS.iter().map(|s| (*s).to_owned()).collect());
    }
    let ptr = at.map_or_else(|| item.ptr.clone(), |q| q.ptr.clone());
    let value = at.map(|q| q.v);
    let mut kept: Vec<String> = Vec::new();
    for entry in list.drain(..) {
        match parse_interval_item(&entry) {
            Ok(_) => kept.push(entry),
            Err(fault) => {
                let why = match fault {
                    ItemFault::Format => {
                        "is not an interval text of 4.10a (a number and one unit of ms, s, m, h, d, w or y)"
                    }
                    ItemFault::Range => "is more than 365 days",
                };
                ctx.unsupported(
                    &ptr,
                    value,
                    &format!("the item {entry:?} {why}"),
                    Fallback::other("the item is removed"),
                );
            }
        }
    }
    if kept.is_empty() {
        not_imported(ctx, at.unwrap_or(item), "no item of the interval list is valid in Hune (4.10a)");
        return None;
    }
    if kept.len() > MAX_INTERVALS {
        let reason = format!("the list has {} items and Hune keeps {MAX_INTERVALS} at most", kept.len());
        ctx.unsupported(&ptr, value, &reason, Fallback::other("the first 50 items"));
        kept.truncate(MAX_INTERVALS);
    }
    ctx.ok();
    Some(kept)
}

/// Reads `auto_count` (15.6). An absent member is 30, the default of Grafana. A value outside 1 to 1,000 is
/// `unsupported`, and the fallback is 30.
fn auto_count_of(ctx: &mut Ctx, node: Option<&Node>) -> u32 {
    let Some(node) = node.filter(|n| !n.is_null()) else { return 30 };
    match as_int(node.v).and_then(|v| u32::try_from(v).ok()).filter(|v| (MIN_AUTO_COUNT..=MAX_AUTO_COUNT).contains(v)) {
        Some(v) => {
            ctx.ok();
            v
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "auto_count must be a whole number from 1 to 1000",
                Fallback::other("30"),
            );
            30
        }
    }
}

/// Reads `auto_min` (15.6). A text of the table of 7.4a is kept. Another valid interval text gives the largest
/// text of the table that is not above it. A text of more than 365 days gives `1y`. Another value gives `10s`.
fn auto_min_of(ctx: &mut Ctx, node: Option<&Node>) -> String {
    let Some(node) = node.filter(|n| !n.is_null()) else { return "10s".to_owned() };
    let text = node.str().unwrap_or("");
    if auto_min_ms(text).is_some() {
        ctx.ok();
        return text.to_owned();
    }
    let (result, reason) = match parse_interval_item(text) {
        Ok(ms) => (
            largest_table_text_at_most(ms),
            "auto_min is not a text of the table of 7.4a: Hune takes the largest text of the table that is not above it",
        ),
        Err(ItemFault::Range) => ("1y", "auto_min is more than 365 days: Hune takes 1y"),
        Err(ItemFault::Format) => ("10s", "auto_min is not an interval text of 4.10a: Hune takes 10s"),
    };
    ctx.changed(&node.ptr, Some(node.v), reason);
    result.to_owned()
}

/// Reads `current` of an interval variable (15.6, `default`). `$__auto` and `$__auto_interval_<name>` give
/// `$__auto` when `auto` is on. An item gives the item. Another value gives `null`.
fn interval_default(ctx: &mut Ctx, current: Option<Node>, items: &[String], auto: bool) -> Option<String> {
    let current = current.filter(|c| !c.is_null())?;
    let Some(mut w) = Walk::new(&current) else {
        ctx.unsupported(&current.ptr, Some(current.v), "current must be an object", Fallback::other("null"));
        return None;
    };
    let value = w.take("value");
    w.meta(ctx, &["text", "selected"]);
    w.finish(ctx);
    let value = value.filter(|v| !v.is_null())?;
    let text = match value.v {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(a) if a.len() == 1 => a[0].as_str().unwrap_or(""),
        _ => "",
    };
    if text.is_empty() {
        return None;
    }
    if text == AUTO_VALUE || text.starts_with("$__auto_interval_") {
        if auto {
            if text != AUTO_VALUE {
                ctx.changed(&value.ptr, Some(value.v), "Hune writes the value of Auto as $__auto");
            } else {
                ctx.ok();
            }
            return Some(AUTO_VALUE.to_owned());
        }
        ctx.changed(&value.ptr, Some(value.v), "the variable has no Auto option: the default is the first option");
        return None;
    }
    if items.iter().any(|i| i == text) {
        ctx.ok();
        return Some(text.to_owned());
    }
    ctx.changed(&value.ptr, Some(value.v), "the current value is not an item: the default is the first option");
    None
}

/// Reads the member `query` of a constant variable (15.6, `value`). A value of more than 1,024 bytes or with a
/// control character stops the import of the dashboard.
fn constant_value(ctx: &mut Ctx, query: Option<&Node>) -> Option<String> {
    let Some(node) = query.filter(|q| !q.is_null()) else { return Some(String::new()) };
    let Some(text) = node.str() else {
        not_imported(ctx, node, "the value of a constant must be a text");
        return None;
    };
    if text.len() > MAX_VALUE_BYTES {
        not_imported(ctx, node, "the value of a constant has more than 1024 bytes (4.10b)");
        return None;
    }
    if text.chars().any(|c| c < '\u{20}' || c == '\u{7f}') {
        not_imported(ctx, node, "the value of a constant has a control character (4.10b)");
        return None;
    }
    ctx.ok();
    Some(text.to_owned())
}

/// Reads `allowCustomValue` (15.6). `absent` is the value of an absent or null member. A value that is not
/// a boolean is `unsupported`, and the variable stays closed.
fn allow_custom_of(ctx: &mut Ctx, node: Option<&Node>, absent: bool) -> bool {
    let Some(node) = node.filter(|n| !n.is_null()) else { return absent };
    match node.v.as_bool() {
        Some(b) => {
            ctx.ok();
            b
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "allowCustomValue must be true or false",
                Fallback::other("no member"),
            );
            false
        }
    }
}

fn all_value_of(ctx: &mut Ctx, node: Option<&Node>, include_all: bool) -> Option<Option<String>> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(None) };
    match node.str() {
        Some("") => Some(None),
        Some(_) if !include_all => {
            ctx.ok();
            Some(None)
        }
        Some(t) if t.len() <= MAX_VALUE_BYTES => {
            ctx.ok();
            Some(Some(t.to_owned()))
        }
        _ => {
            not_imported(ctx, node, "allValue must be a text of at most 1024 bytes");
            None
        }
    }
}

fn regex_of(ctx: &mut Ctx, node: Option<&Node>) -> Option<String> {
    let node = node.filter(|n| !n.is_null())?;
    match node.str() {
        Some("") => None,
        Some(t) => match check_pattern(t, PatternForm::Grafana) {
            Ok(()) => {
                ctx.ok();
                Some(t.to_owned())
            }
            Err(e) => {
                ctx.unsupported(
                    &node.ptr,
                    Some(node.v),
                    &format!("the pattern is refused by 4.24 (19.18): {e:?}"),
                    Fallback::other("null"),
                );
                None
            }
        },
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the regex must be a text", Fallback::other("null"));
            None
        }
    }
}

/// Applies the rule of the text variables used in code (15.6, 19.11). Each text variable that a
/// code uses gets `value_type` `number` when its default is a decimal number.
pub(crate) fn apply_text_number_rule(ctx: &mut Ctx, vars: &mut [VarOut], code_uses: &BTreeSet<String>) {
    for v in vars.iter_mut() {
        let VarKind::Text { default, number, pointer } = &mut v.kind else { continue };
        if !code_uses.contains(&v.name) {
            continue;
        }
        if is_decimal_number(default) {
            *number = true;
            ctx.changed(pointer, None, &format!("the variable {} stands in code: value_type is number", v.name));
        } else {
            ctx.unsupported(
                pointer,
                None,
                &format!("the variable {} stands in code and its default is not a decimal number (19.11)", v.name),
                Fallback::DashboardNotImported,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_query_text() {
        assert_eq!(parse_custom_query("a,b"), vec![("a".to_owned(), "a".to_owned()), ("b".to_owned(), "b".to_owned())]);
        assert_eq!(
            parse_custom_query("Production : prod, Pre : pre"),
            vec![("Production".to_owned(), "prod".to_owned()), ("Pre".to_owned(), "pre".to_owned())]
        );
        assert_eq!(
            parse_custom_query("x\\,y,z"),
            vec![("x,y".to_owned(), "x,y".to_owned()), ("z".to_owned(), "z".to_owned())]
        );
        assert!(parse_custom_query(" , ").is_empty());
    }
}
