// SPDX-License-Identifier: MIT OR Apache-2.0
//! The panel types and their options (4.26 to 4.33b), with the value tables of 15.6.

use hune_core::document::{PatternForm, check_pattern};
use hune_core::limits::{MAX_MATCHER_NAMES, MAX_SORT_KEYS, MAX_VALUE_BYTES};

use crate::fields::Customs;
use crate::item::{Ctx, Fallback};
use crate::out::Out;
use crate::util::{as_flag, as_int, as_num, chars};
use crate::walk::{Node, Walk};

mod bucket;
mod gauge;
mod geomap;
mod logs;
mod stat;

/// The 12 panel types (4.13, 4.33a to 4.33d).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pt {
    Table,
    Timeseries,
    Stat,
    Barchart,
    Piechart,
    Xychart,
    Bargauge,
    Text,
    StateTimeline,
    StatusHistory,
    Heatmap,
    Histogram,
    Logs,
    Gauge,
    Geomap,
}

impl Pt {
    pub(crate) fn parse(s: &str) -> Option<Pt> {
        Some(match s {
            "table" => Pt::Table,
            "timeseries" => Pt::Timeseries,
            "stat" => Pt::Stat,
            "barchart" => Pt::Barchart,
            "piechart" => Pt::Piechart,
            "xychart" => Pt::Xychart,
            "bargauge" => Pt::Bargauge,
            "text" => Pt::Text,
            "state-timeline" => Pt::StateTimeline,
            "status-history" => Pt::StatusHistory,
            "heatmap" => Pt::Heatmap,
            "histogram" => Pt::Histogram,
            "logs" => Pt::Logs,
            "gauge" => Pt::Gauge,
            "geomap" => Pt::Geomap,
            _ => return None,
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Pt::Table => "table",
            Pt::Timeseries => "timeseries",
            Pt::Stat => "stat",
            Pt::Barchart => "barchart",
            Pt::Piechart => "piechart",
            Pt::Xychart => "xychart",
            Pt::Bargauge => "bargauge",
            Pt::Text => "text",
            Pt::StateTimeline => "state_timeline",
            Pt::StatusHistory => "status_history",
            Pt::Heatmap => "heatmap",
            Pt::Histogram => "histogram",
            Pt::Logs => "logs",
            Pt::Gauge => "gauge",
            Pt::Geomap => "geomap",
        }
    }
}

fn default_fallback() -> Fallback {
    Fallback::other("the Grafana default of that option")
}

/// The walk of an object that can be absent.
struct OptWalk<'a> {
    w: Option<Walk<'a>>,
}

impl<'a> OptWalk<'a> {
    fn new(ctx: &mut Ctx, node: Option<Node<'a>>, what: &str) -> OptWalk<'a> {
        let node = node.filter(|n| !n.is_null());
        let w = node.as_ref().and_then(|n| {
            let w = Walk::new(n);
            if w.is_none() {
                ctx.unsupported(&n.ptr, Some(n.v), &format!("{what} must be an object"), Fallback::MemberRemoved);
            }
            w
        });
        OptWalk { w }
    }

    fn take(&mut self, key: &str) -> Option<Node<'a>> {
        self.w.as_mut().and_then(|w| w.take(key)).filter(|n| !n.is_null())
    }

    fn finish(self, ctx: &mut Ctx) {
        if let Some(w) = self.w {
            w.finish(ctx);
        }
    }
}

/// One row of a value table: the Grafana value, the value of format 1 and the class `changed`.
type Row = (&'static str, &'static str, bool);

fn enum_opt(ctx: &mut Ctx, node: Option<&Node>, table: &[Row], default: &str) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::s(default);
    };
    if let Some(text) = node.str()
        && let Some((_, to, changed)) = table.iter().find(|(from, _, _)| *from == text)
    {
        if *changed {
            ctx.changed(&node.ptr, Some(node.v), &format!("the value {text} becomes {to}"));
        } else {
            ctx.ok();
        }
        return Out::s(to);
    }
    ctx.unsupported(&node.ptr, Some(node.v), "no value table of 15.6 names this value", default_fallback());
    Out::s(default)
}

fn same(values: &[&'static str]) -> Vec<Row> {
    values.iter().map(|v| (*v, *v, false)).collect()
}

fn int_opt(ctx: &mut Ctx, node: Option<&Node>, min: i64, max: i64, default: i64) -> Out {
    int_opt_with(ctx, node, min, max, default, default_fallback())
}

/// Like [`int_opt`], with the fallback that the report names for a value out of range.
fn int_opt_with(ctx: &mut Ctx, node: Option<&Node>, min: i64, max: i64, default: i64, fallback: Fallback) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Int(default);
    };
    match as_int(node.v).filter(|v| (min..=max).contains(v)) {
        Some(v) => {
            ctx.ok();
            Out::Int(v)
        }
        None => {
            let reason = format!("the value must be an integer from {min} to {max}");
            ctx.unsupported(&node.ptr, Some(node.v), &reason, fallback);
            Out::Int(default)
        }
    }
}

fn bool_opt(ctx: &mut Ctx, node: Option<&Node>, default: bool) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Bool(default);
    };
    match as_flag(node.v) {
        Some(b) => {
            ctx.ok();
            Out::Bool(b)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the value must be true or false", default_fallback());
            Out::Bool(default)
        }
    }
}

fn text_or_null(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Null;
    };
    match node.str() {
        Some("") => {
            ctx.ok();
            Out::Null
        }
        Some(t) if t.len() <= 1024 => {
            ctx.ok();
            Out::s(t)
        }
        _ => {
            ctx.unsupported(&node.ptr, Some(node.v), "the value must be a text of 1 to 1024 bytes", default_fallback());
            Out::Null
        }
    }
}

fn legend(ctx: &mut Ctx, node: Option<Node>, pie: bool) -> Out {
    let mut w = OptWalk::new(ctx, node, "legend");
    let show = w.take("showLegend");
    let mode = w.take("displayMode");
    let placement = w.take("placement");
    let values = w.take(if pie { "values" } else { "calcs" });
    let sort_by = w.take("sortBy");
    let sort_desc = w.take("sortDesc");
    w.finish(ctx);
    let mut visible = match &show {
        Some(s) => match as_flag(s.v) {
            Some(b) => {
                ctx.ok();
                b
            }
            None => {
                ctx.unsupported(&s.ptr, Some(s.v), "showLegend must be true or false", default_fallback());
                true
            }
        },
        None => true,
    };
    let mode_out = match mode.as_ref().map(|m| (m, m.str())) {
        None => "list",
        Some((_, Some("hidden"))) => {
            visible = false;
            ctx.ok();
            "list"
        }
        Some((_, Some(m @ ("list" | "table")))) => {
            ctx.ok();
            m
        }
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "no value table of 15.6 names this value", default_fallback());
            "list"
        }
    };
    let default_place = if pie { "right" } else { "bottom" };
    let place = enum_opt(ctx, placement.as_ref(), &same(&["bottom", "right"]), default_place);
    let vals = legend_values(ctx, values, pie);
    let mut members =
        vec![("show", Out::Bool(visible)), ("placement", place), ("mode", Out::s(mode_out)), ("values", vals)];
    if let Some(by) = sort_by.as_ref().and_then(|n| legend_sort_by(ctx, n, pie)) {
        members.push(("sort_by", Out::s(by)));
    }
    if let Some(n) = sort_desc.as_ref() {
        match as_flag(n.v) {
            Some(b) => {
                members.push(("sort_desc", Out::Bool(b)));
                ctx.ok();
            }
            None => ctx.unsupported(&n.ptr, Some(n.v), "sortDesc must be true or false", Fallback::MemberRemoved),
        }
    }
    Out::members(members)
}

/// The title of a legend column in Grafana and the value of `sort_by`. The title of a calculation
/// is the name that Grafana gives to its reducer.
fn legend_sort_by(ctx: &mut Ctx, node: &Node, pie: bool) -> Option<&'static str> {
    let by = match (pie, node.str()) {
        (_, Some("Name")) => Some("name"),
        (true, Some("Value")) => Some("value"),
        (true, Some("Percent")) => Some("percent"),
        (false, Some(title)) => reducer_by_title(title),
        _ => None,
    };
    match by {
        Some(by) => {
            ctx.ok();
            Some(by)
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "this legend column has no equal in the legend table (4.26)",
                Fallback::MemberRemoved,
            );
            None
        }
    }
}

/// The reducers that the legend of Hune has, by their Grafana title.
fn reducer_by_title(title: &str) -> Option<&'static str> {
    Some(match title {
        "Last *" => "last_not_null",
        "Last" => "last",
        "First *" => "first_not_null",
        "First" => "first",
        "Min" => "min",
        "Max" => "max",
        "Mean" => "mean",
        "Median" => "median",
        "Variance" => "variance",
        "StdDev" => "std_dev",
        "Total" => "total",
        "Count" => "count",
        "Range" => "range",
        "Delta" => "delta",
        "Difference" => "diff",
        "Difference percent" => "diff_percent",
        "Distinct Count" => "distinct_count",
        "Change Count" => "change_count",
        _ => return None,
    })
}

fn legend_values(ctx: &mut Ctx, node: Option<Node>, pie: bool) -> Out {
    let Some(node) = node else { return Out::Arr(vec![]) };
    let Some(_) = node.v.as_array() else {
        ctx.unsupported(&node.ptr, Some(node.v), "the value must be an array", default_fallback());
        return Out::Arr(vec![]);
    };
    let mut out: Vec<String> = Vec::new();
    for item in node.items() {
        let mapped: Option<&str> = match (pie, item.str()) {
            (true, Some(v @ ("value" | "percent"))) => Some(v),
            (false, Some(id)) => calc_name(id),
            _ => None,
        };
        match mapped {
            None => ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "this reducer has no equal in format 2",
                Fallback::other("item removed"),
            ),
            Some(to) => {
                ctx.ok();
                if out.iter().any(|o| o == to) {
                    ctx.changed(&item.ptr, Some(item.v), "two items give the same value: the second is removed");
                } else {
                    out.push(to.to_owned());
                }
            }
        }
    }
    if !pie && out.len() > 5 {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "the legend table has 5 values at most (4.26)",
            Fallback::other("the first 5 items stay"),
        );
        out.truncate(5);
    }
    Out::strings(&out)
}

fn tooltip(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "tooltip");
    let mode = w.take("mode");
    let sort = w.take("sort");
    let hide_zeros = w.take("hideZeros");
    w.finish(ctx);
    let table: [Row; 3] = [("single", "single", false), ("multi", "multi", false), ("none", "hidden", false)];
    let mut members = vec![("mode", enum_opt(ctx, mode.as_ref(), &table, "single"))];
    if let Some(sort) = sort.as_ref() {
        members.push(("sort", enum_opt(ctx, Some(sort), &same(&["none", "asc", "desc"]), "none")));
    }
    if let Some(n) = hide_zeros.as_ref() {
        members.push(("hide_zeros", bool_opt(ctx, Some(n), false)));
    }
    Out::members(members)
}

fn reduce(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "reduceOptions");
    let calcs = w.take("calcs");
    let fields = w.take("fields");
    let values = w.take("values");
    w.finish(ctx);
    let calc = reduce_calc(ctx, calcs);
    let fields_out = match fields.as_ref().map(|f| (f, f.str())) {
        None | Some((_, Some(""))) => {
            ctx.ok();
            Out::Null
        }
        Some((f, Some(t))) => match check_pattern(t, PatternForm::Grafana) {
            Ok(()) => {
                ctx.ok();
                Out::s(t)
            }
            Err(e) => {
                ctx.unsupported(
                    &f.ptr,
                    Some(f.v),
                    &format!("the pattern is refused by 4.24: {e:?}"),
                    Fallback::other("null"),
                );
                Out::Null
            }
        },
        Some((f, None)) => {
            ctx.unsupported(&f.ptr, Some(f.v), "fields must be a text", Fallback::other("null"));
            Out::Null
        }
    };
    let values_out = bool_opt(ctx, values.as_ref(), false);
    Out::obj([("calc", Out::s(calc)), ("fields", fields_out), ("values", values_out)])
}

/// The `calc` value of 4.26 for a Grafana reducer id. A reducer without an equal gives `None`.
pub(crate) fn calc_name(id: &str) -> Option<&'static str> {
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

fn reduce_calc(ctx: &mut Ctx, node: Option<Node>) -> &'static str {
    let Some(node) = node else {
        ctx.ok();
        return "last_not_null";
    };
    let items = node.items();
    if node.v.as_array().is_none() {
        ctx.unsupported(&node.ptr, Some(node.v), "calcs must be an array", Fallback::other("last_not_null"));
        return "last_not_null";
    }
    match items.len() {
        0 => {
            ctx.changed(&node.ptr, Some(node.v), "no reducer: the Grafana default last_not_null");
            "last_not_null"
        }
        n => {
            let first = &items[0];
            let name = first.str().and_then(calc_name);
            if n > 1 {
                let fb = Fallback::other(&format!("the first item: {}", name.unwrap_or("last_not_null")));
                ctx.unsupported(&node.ptr, Some(node.v), "format 1 has one reducer: the other items are removed", fb);
                return name.unwrap_or("last_not_null");
            }
            match name {
                Some(name) => {
                    ctx.ok();
                    name
                }
                None => {
                    ctx.unsupported(
                        &first.ptr,
                        Some(first.v),
                        "this reducer has no equal in format 1",
                        Fallback::other("last_not_null"),
                    );
                    "last_not_null"
                }
            }
        }
    }
}

fn percent_opt(ctx: &mut Ctx, node: Option<&Node>, default: i64) -> Out {
    percent_opt_with(ctx, node, default, default_fallback())
}

/// Like [`percent_opt`], with the fallback that the report names for a value out of range.
fn percent_opt_with(ctx: &mut Ctx, node: Option<&Node>, default: i64, fallback: Fallback) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Int(default);
    };
    match as_num(node.v).map(|n| (n * 100.0).round()).filter(|n| (10.0..=100.0).contains(n)) {
        Some(n) => {
            ctx.ok();
            Out::Int(n as i64)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the value must be a number from 0.1 to 1", fallback);
            Out::Int(default)
        }
    }
}

fn custom<'a, 'b>(customs: &'b Customs<'a>, key: &str) -> Option<&'b Node<'a>> {
    customs.get(key)
}

/// Maps the footer of a table of Grafana 11 and older: `options.footer` (15.6). A footer that is not shown has no
/// effect. Grafana 12.4.2 moves a footer that is shown to `custom.footer.reducers`: in the defaults, or in one override
/// for the names of `fields`. With `countRows` and the first reducer `count`, the list is `countAll`, in the defaults.
/// The importer makes the same move into `fields`, with one `changed` item.
fn old_footer(ctx: &mut Ctx, node: &Node, fields: &mut Out) {
    // Grafana 10 holds the pages of rows in the footer. Grafana reads this member also when the footer is not shown.
    if let Some(p) = node.get("enablePagination").filter(|p| as_flag(p.v) == Some(true)) {
        let why = "Hune has no pages of rows: the panel shows all the rows";
        ctx.unsupported(&p.ptr, Some(p.v), why, Fallback::other("the panel shows all the rows"));
    }
    let Some(mut w) = Walk::new(node).filter(|_| node.get("show").and_then(|s| as_flag(s.v)) == Some(true)) else {
        ctx.meta();
        return;
    };
    let reducer = w.take("reducer");
    let names = w.take("fields");
    let count_node = w.take("countRows");
    let count_rows = count_node.as_ref().is_some_and(|c| as_flag(c.v) == Some(true));
    if w.take("show").is_some() {
        ctx.ok();
    }
    if w.take("enablePagination").is_some_and(|p| as_flag(p.v) != Some(true)) {
        ctx.meta();
    }
    w.finish(ctx);
    let reducer_present = reducer.is_some();
    let Some(reducer) = reducer.filter(|r| !r.is_null()) else {
        // No reducer: no footer. Each member that is present has no effect.
        let present = [reducer_present, names.is_some(), count_node.is_some()];
        present.iter().filter(|p| **p).for_each(|_| ctx.meta());
        return;
    };
    let first_is_count = reducer.at(0).is_some_and(|r| r.str() == Some("count"));
    let (set, names) = if count_rows && first_is_count {
        // Grafana counts the rows and does not read `fields`.
        ctx.ok();
        ctx.ok();
        if names.is_some() {
            ctx.meta();
        }
        (Out::obj([("reducers", Out::strings(&["count"]))]), Vec::new())
    } else {
        // `countRows` with another first reducer has no effect in Grafana.
        if count_node.is_some() {
            ctx.meta();
        }
        let Ok(list) = footer_names(ctx, names.as_ref()) else { return };
        let Some(set) = crate::fields::footer_reducers(ctx, &reducer) else { return };
        (set, list)
    };
    let why = if count_rows && first_is_count {
        "Grafana 12 moves this footer to custom.footer.reducers [countAll]: it shows the row count in the first column, \
         and Hune shows it in each column (4.18)"
    } else {
        "Grafana 12 moves this footer to custom.footer.reducers: the importer moves it in the same way (4.18)"
    };
    let matcher = match names.as_slice() {
        [] => {
            crate::fields::set_default_property(fields, "footer", set);
            ctx.changed(&node.ptr, Some(node.v), why);
            return;
        }
        [one] => Out::obj([("by", Out::s("name")), ("value", Out::s(one))]),
        many => Out::obj([("by", Out::s("names")), ("names", Out::strings(many)), ("mode", Out::s("include"))]),
    };
    let item = Out::obj([("match", matcher), ("set", Out::obj([("footer", set)]))]);
    if crate::fields::push_override(fields, item) {
        ctx.changed(&node.ptr, Some(node.v), why);
    } else {
        let why = "more overrides than MAX_OVERRIDES: the override of the footer has no place";
        ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::other("footer removed"));
    }
}

/// Reads `options.footer.fields`: absent, `""` and `[]` give no name, so the footer is for all the fields. Else a
/// list of 1 to `MAX_MATCHER_NAMES` different names of 1 to 1024 bytes with no control character. A value that
/// breaks the rule gives an item, and the footer is removed.
fn footer_names(ctx: &mut Ctx, node: Option<&Node>) -> Result<Vec<String>, ()> {
    let all = |n: &Node| n.is_null() || n.str() == Some("") || n.v.as_array().is_some_and(Vec::is_empty);
    let Some(node) = node.filter(|n| !all(n)) else {
        if node.is_some() {
            ctx.meta();
        }
        return Ok(Vec::new());
    };
    let mut list: Vec<String> = Vec::new();
    let control = |c: char| c <= '\u{1f}' || c == '\u{7f}';
    // The list stops at the first fault, and it never holds more than `MAX_MATCHER_NAMES` names: the work is linear.
    let good = node.v.as_array().is_some_and(|items| {
        items.iter().all(|item| match item.as_str() {
            Some(n) if !n.is_empty() && n.len() <= MAX_VALUE_BYTES && !n.chars().any(control) => {
                if list.iter().any(|x| x == n) {
                    return true;
                }
                list.push(n.to_owned());
                list.len() <= MAX_MATCHER_NAMES
            }
            _ => false,
        })
    });
    if !good {
        let why =
            format!("fields must be a list of 1 to {MAX_MATCHER_NAMES} field names of 1 to {MAX_VALUE_BYTES} bytes");
        ctx.unsupported(&node.ptr, Some(node.v), &why, Fallback::other("footer removed"));
        return Err(());
    }
    ctx.ok();
    Ok(list)
}

fn table_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs, fields: &mut Out) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let header = w.take("showHeader");
    let sort = w.take("sortBy");
    let footer = w.take("footer");
    w.finish(ctx);
    if let Some(footer) = footer.as_ref() {
        old_footer(ctx, footer, fields);
    }
    let header = bool_opt(ctx, header.as_ref(), true);
    let mut keys = Vec::new();
    if let Some(sort) = sort {
        if sort.v.as_array().is_none() {
            ctx.unsupported(&sort.ptr, Some(sort.v), "sortBy must be an array", default_fallback());
        } else {
            for item in sort.items() {
                let Some(mut iw) = Walk::new(&item) else {
                    ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "a sort key must be an object",
                        Fallback::other("item removed"),
                    );
                    continue;
                };
                let name = iw.take("displayName");
                let desc = iw.take("desc");
                iw.finish(ctx);
                match name.as_ref().and_then(Node::str).filter(|n| !n.is_empty() && n.len() <= 1024) {
                    Some(n) => {
                        let d = desc.as_ref().and_then(|d| as_flag(d.v)).unwrap_or(false);
                        keys.push(Out::obj([("field", Out::s(n)), ("desc", Out::Bool(d))]));
                        ctx.ok();
                    }
                    None => ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "a sort key needs a displayName of 1 to 1024 bytes",
                        Fallback::other("item removed"),
                    ),
                }
            }
            if keys.len() > MAX_SORT_KEYS {
                ctx.unsupported(
                    &sort.ptr,
                    Some(sort.v),
                    "more sort keys than MAX_SORT_KEYS",
                    Fallback::other("the first 8 keys stay"),
                );
                keys.truncate(MAX_SORT_KEYS);
            }
        }
    }
    let mut members = vec![("show_header", header), ("sort", Out::Arr(keys))];
    if let Some(wrap) = custom(customs, "wrapText") {
        members.push(("wrap", bool_opt(ctx, Some(wrap), false)));
    }
    Out::members(members)
}

fn timeseries_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    let draw = enum_opt(ctx, custom(customs, "drawStyle"), &same(&["line", "bars", "points"]), "line");
    let interpolation = enum_opt(
        ctx,
        custom(customs, "lineInterpolation"),
        &[
            ("linear", "linear", false),
            ("smooth", "smooth", false),
            ("stepBefore", "step_before", false),
            ("stepAfter", "step_after", false),
        ],
        "linear",
    );
    let line_width = int_opt(ctx, custom(customs, "lineWidth"), 0, 10, 1);
    let fill = int_opt(ctx, custom(customs, "fillOpacity"), 0, 100, 0);
    let points = enum_opt(ctx, custom(customs, "showPoints"), &same(&["auto", "always", "never"]), "auto");
    let size = int_opt(ctx, custom(customs, "pointSize"), 1, 40, 5);
    // `spanNulls` as a number of ms is `span_nulls_ms` . 0 and below join no run (Grafana); a threshold above the cap joins
    // each run, as `true` does. Grafana compares `gap < ms`, so a fraction rounds up for the times in whole ms.
    let mut span_ms = None;
    let span = match custom(customs, "spanNulls") {
        Some(n) if n.v.is_number() => {
            let ms = as_num(n.v).unwrap_or(0.0).ceil();
            if ms > hune_core::document::MAX_SPAN_NULLS_MS as f64 {
                ctx.changed(&n.ptr, Some(n.v), "a threshold above 10^12 ms joins each run of nulls: span_nulls true");
                Out::Bool(true)
            } else {
                if ms >= 1.0 {
                    span_ms = Some(Out::Int(ms as i64));
                }
                ctx.ok();
                Out::Bool(false)
            }
        }
        other => bool_opt(ctx, other, false),
    };
    let stacking = enum_opt(ctx, custom(customs, "stacking.mode"), &same(&["none", "normal", "percent"]), "none");
    let y = enum_opt(ctx, custom(customs, "scaleDistribution.type"), &same(&["linear", "log"]), "linear");
    let log2 = custom(customs, "scaleDistribution.log").is_some();
    let mut members = vec![
        ("draw", draw),
        ("interpolation", interpolation),
        ("line_width", line_width),
        ("fill_opacity", fill),
        ("show_points", points),
        ("point_size", size),
        ("span_nulls", span),
        ("stacking", stacking),
        ("y_scale", y),
        ("legend", legend(ctx, legend_node, false)),
        ("tooltip", tooltip(ctx, tooltip_node)),
    ];
    thresholds_style(ctx, customs, &mut members);
    // The optional members (P10 and P14) (4.28).
    gradient_mode(ctx, customs, &mut members);
    if log2 {
        members.push(("log_base", Out::Int(2)));
    }
    if let Some(ms) = span_ms {
        members.push(("span_nulls_ms", ms));
    }
    if let Some(n) = custom(customs, "barAlignment").filter(|n| !n.is_null()) {
        match as_int(n.v) {
            Some(a @ -1..=1) => {
                ctx.ok();
                members.push(("bar_alignment", Out::s(["before", "center", "after"][(a + 1) as usize])));
            }
            _ => ctx.unsupported(&n.ptr, Some(n.v), "the bar alignment must be -1, 0 or 1", default_fallback()),
        }
    }
    Out::members(members)
}

/// The value of `custom.gradientMode` as the option `gradient_mode` (4.28, 4.30). `null` is the default `none`.
fn gradient_mode(ctx: &mut Ctx, customs: &Customs, members: &mut Vec<(&'static str, Out)>) {
    if let Some(n) = custom(customs, "gradientMode").filter(|n| !n.is_null()) {
        members.push(("gradient_mode", enum_opt(ctx, Some(n), &same(&["none", "opacity", "hue", "scheme"]), "none")));
    }
}

/// The values of `custom.thresholdsStyle.mode` and of `thresholds_style` (4.28, 4.30). The Grafana
/// value `series` has no equal: the item says so and the default `off` stays.
fn thresholds_style(ctx: &mut Ctx, customs: &Customs, members: &mut Vec<(&'static str, Out)>) {
    let Some(node) = custom(customs, "thresholdsStyle.mode") else { return };
    let table: [Row; 6] = [
        ("off", "off", false),
        ("line", "line", false),
        ("dashed", "dashed", false),
        ("area", "area", false),
        ("line+area", "line_area", false),
        ("dashed+area", "dashed_area", false),
    ];
    members.push(("thresholds_style", enum_opt(ctx, Some(node), &table, "off")));
}

fn stat_options(ctx: &mut Ctx, options: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let reduce_node = w.take("reduceOptions");
    let color = w.take("colorMode");
    let graph = w.take("graphMode");
    let text = w.take("textMode");
    let orient = w.take("orientation");
    let justify = w.take("justifyMode");
    let wide = w.take("wideLayout");
    let show_change = w.take("showPercentChange");
    let change_color = w.take("percentChangeColorMode");
    let text_sizes = w.take("text");
    w.finish(ctx);
    // `false` turns off a Grafana default, so the rule of 15.5 for an unknown member does not make it metadata.
    if let Some(n) = wide.as_ref() {
        if n.v.as_bool() == Some(false) {
            let why = "false puts the name above the value in a wide tile: Hune puts it at the left (9.19)";
            ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::MemberRemoved);
        } else {
            ctx.unknown_member(&n.ptr, n.v);
        }
    }
    let color_table: [Row; 4] = [
        ("none", "none", false),
        ("value", "value", false),
        ("background", "background", false),
        ("background_solid", "background", true),
    ];
    let mut members = vec![
        ("reduce", reduce(ctx, reduce_node)),
        ("color_mode", enum_opt(ctx, color.as_ref(), &color_table, "value")),
        ("graph_mode", enum_opt(ctx, graph.as_ref(), &same(&["none", "area"]), "area")),
        (
            "text_mode",
            enum_opt(ctx, text.as_ref(), &same(&["auto", "value", "value_and_name", "name", "none"]), "auto"),
        ),
        ("orientation", enum_opt(ctx, orient.as_ref(), &same(&["auto", "horizontal", "vertical"]), "auto")),
    ];
    if let Some(justify) = justify.as_ref() {
        members.push(("justify", enum_opt(ctx, Some(justify), &same(&["auto", "center"]), "auto")));
    }
    stat::percent_and_text(ctx, &mut members, show_change.as_ref(), change_color.as_ref(), text_sizes);
    Out::members(members)
}

fn barchart_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let orient = w.take("orientation");
    let x_field = w.take("xField");
    let stacking = w.take("stacking");
    let show = w.take("showValue");
    let bar = w.take("barWidth");
    let group = w.take("groupWidth");
    let rotation = w.take("xTickLabelRotation");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    // `auto` has its equal (4.30). An absent orientation is `auto`, the default of the Grafana panel.
    let mut members = vec![
        ("orientation", enum_opt(ctx, orient.as_ref(), &same(&["vertical", "horizontal", "auto"]), "auto")),
        ("x_field", text_or_null(ctx, x_field.as_ref())),
        ("stacking", enum_opt(ctx, stacking.as_ref(), &same(&["none", "normal", "percent"]), "none")),
        ("show_values", enum_opt(ctx, show.as_ref(), &same(&["auto", "always", "never"]), "auto")),
        ("bar_width", percent_opt(ctx, bar.as_ref(), 97)),
        ("group_width", percent_opt(ctx, group.as_ref(), 70)),
        ("x_tick_rotation", int_opt(ctx, rotation.as_ref(), -90, 90, 0)),
        ("legend", legend(ctx, legend_node, false)),
        ("tooltip", tooltip(ctx, tooltip_node)),
    ];
    thresholds_style(ctx, customs, &mut members);
    // The optional fill and line of the bars (4.30, P03). `null` is the Grafana default, as an absent member.
    for (key, member, max, default) in [("fillOpacity", "fill_opacity", 100, 80), ("lineWidth", "line_width", 10, 1)] {
        if let Some(n) = custom(customs, key).filter(|n| !n.is_null()) {
            members.push((member, int_opt(ctx, Some(n), 0, max, default)));
        }
    }
    gradient_mode(ctx, customs, &mut members);
    Out::members(members)
}

fn piechart_options(ctx: &mut Ctx, options: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let pie_type = w.take("pieType");
    let reduce_node = w.take("reduceOptions");
    let labels = w.take("displayLabels");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    let mut label_list: Vec<String> = Vec::new();
    if let Some(labels) = labels {
        if labels.v.as_array().is_none() {
            ctx.unsupported(&labels.ptr, Some(labels.v), "displayLabels must be an array", default_fallback());
        } else {
            for item in labels.items() {
                match item.str() {
                    Some(l @ ("name" | "value" | "percent")) => {
                        ctx.ok();
                        if !label_list.iter().any(|x| x == l) {
                            label_list.push(l.to_owned());
                        }
                    }
                    _ => ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "this label has no equal in format 1",
                        Fallback::other("item removed"),
                    ),
                }
            }
        }
    }
    Out::obj([
        ("pie_type", enum_opt(ctx, pie_type.as_ref(), &same(&["pie", "donut"]), "pie")),
        ("reduce", reduce(ctx, reduce_node)),
        ("labels", Out::strings(&label_list)),
        ("legend", legend(ctx, legend_node, true)),
        ("tooltip", tooltip(ctx, tooltip_node)),
    ])
}

/// Reads `x` or `y` of a series of an xychart: a field name (Grafana 11), or the matcher
/// `{matcher: {id: byName, options: <name>}}` (Grafana 12). It gives the node of the name.
fn series_field<'a>(ctx: &mut Ctx, node: &Node<'a>) -> Option<Node<'a>> {
    if node.str().is_some() {
        return Some(node.clone());
    }
    let fail = |ctx: &mut Ctx, why: &str| {
        ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::other("series removed"));
        None
    };
    let Some(mut w) = Walk::new(node) else { return fail(ctx, "the field must be a name or a matcher") };
    let matcher = w.take("matcher");
    w.finish(ctx);
    let Some(mut m) = matcher.as_ref().and_then(Walk::new) else {
        return fail(ctx, "the field must have a matcher with an id and options");
    };
    let id = m.take("id");
    let options = m.take("options");
    m.finish(ctx);
    match (id.as_ref().and_then(Node::str), options) {
        (Some("byName"), Some(name)) if name.str().is_some() => Some(name),
        _ => fail(ctx, "only the matcher byName with a field name has an equal (4.32)"),
    }
}

/// Reads `frame` of a series of an xychart. Format 2 has one frame for each panel: only the
/// first frame (the matcher `byIndex` 0) has an equal.
fn series_frame(ctx: &mut Ctx, node: &Node) {
    let index = node
        .get("matcher")
        .and_then(|m| match (m.get("id"), m.get("options")) {
            (Some(id), Some(o)) if id.str() == Some("byIndex") => as_int(o.v),
            _ => None,
        })
        .or_else(|| as_int(node.v));
    if index == Some(0) {
        ctx.meta();
    } else {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "a panel has one frame in format 2",
            Fallback::other("series removed"),
        );
    }
}

fn xychart_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let series = w.take("series");
    let mapping = w.take("mapping").or_else(|| w.take("seriesMapping"));
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    // `auto`: Grafana ignores the series and chooses the fields. This is `x_field` null and no `y_fields`.
    let auto = match mapping.as_ref() {
        None => false,
        Some(m) => match m.str() {
            Some("auto") => true,
            Some("manual") => false,
            _ => {
                ctx.unsupported(&m.ptr, Some(m.v), "the mapping must be auto or manual", default_fallback());
                false
            }
        },
    };
    let mut x_field = Out::Null;
    let mut x_set = false;
    let mut y_fields: Vec<String> = Vec::new();
    let mut show: Option<Node> = None;
    if let Some(series) = series {
        if series.v.as_array().is_none() {
            ctx.unsupported(&series.ptr, Some(series.v), "series must be an array", default_fallback());
        } else {
            for item in series.items() {
                let Some(mut sw) = Walk::new(&item) else {
                    ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "a series must be an object",
                        Fallback::other("series removed"),
                    );
                    continue;
                };
                let x = sw.take("x").and_then(|n| series_field(ctx, &n));
                let y = sw.take("y").and_then(|n| series_field(ctx, &n));
                let sh = sw.take("show");
                if let Some(frame) = sw.take("frame") {
                    series_frame(ctx, &frame);
                }
                sw.finish(ctx);
                if let Some(x) = x.filter(|x| x.str().is_some_and(|t| !t.is_empty())) {
                    let text = x.str().unwrap_or_default();
                    if !x_set {
                        x_field = text_or_null(ctx, Some(&x));
                        x_set = true;
                    } else if x_field != Out::s(text) {
                        ctx.unsupported(
                            &x.ptr,
                            Some(x.v),
                            "format 1 has one x field for the panel",
                            Fallback::other("series removed"),
                        );
                        continue;
                    }
                }
                if let Some(y) = y.filter(|y| y.str().is_some_and(|t| !t.is_empty())) {
                    let text = y.str().unwrap_or_default().to_owned();
                    if chars(&text) > 0 && text.len() <= 1024 {
                        if !y_fields.contains(&text) {
                            y_fields.push(text);
                        }
                        ctx.ok();
                    }
                }
                if show.is_none() {
                    show = sh;
                }
            }
        }
    }
    let draw_table: [Row; 3] =
        [("points", "points", false), ("lines", "lines", false), ("points+lines", "points_and_lines", false)];
    if auto {
        x_field = Out::Null;
        y_fields.clear();
    }
    // The member `show` is in the series (Grafana 11) or in `custom.show` (Grafana 12).
    let show = show.as_ref().or_else(|| custom(customs, "show"));
    Out::obj([
        ("x_field", x_field),
        ("y_fields", Out::strings(&y_fields)),
        ("draw", enum_opt(ctx, show, &draw_table, "points")),
        ("point_size", int_opt(ctx, custom(customs, "pointSize"), 1, 40, 5)),
        ("line_width", int_opt(ctx, custom(customs, "lineWidth"), 0, 10, 1)),
        ("legend", legend(ctx, legend_node, false)),
        ("tooltip", tooltip(ctx, tooltip_node)),
    ])
}

fn bargauge_options(ctx: &mut Ctx, options: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let reduce_node = w.take("reduceOptions");
    let orient = w.take("orientation");
    let mode = w.take("displayMode");
    let unfilled = w.take("showUnfilled");
    let sizing = w.take("sizing");
    let sizes = [w.take("minVizHeight"), w.take("minVizWidth"), w.take("maxVizHeight")];
    w.finish(ctx);
    // Grafana reads the three sizes only with `sizing` = `manual` (15.6). Else they are metadata.
    let manual = sizing.as_ref().is_some_and(|s| s.str() == Some("manual"));
    match sizing.as_ref() {
        Some(s) if s.str() == Some("auto") => ctx.meta(),
        Some(s) => ctx.unknown_member(&s.ptr, s.v),
        None => {}
    }
    for n in sizes.iter().flatten() {
        if manual {
            ctx.unknown_member(&n.ptr, n.v);
        } else {
            ctx.meta();
        }
    }
    let orient_table: [Row; 3] =
        [("horizontal", "horizontal", false), ("vertical", "vertical", false), ("auto", "horizontal", true)];
    Out::obj([
        ("reduce", reduce(ctx, reduce_node)),
        ("orientation", enum_opt(ctx, orient.as_ref(), &orient_table, "horizontal")),
        ("display_mode", enum_opt(ctx, mode.as_ref(), &same(&["basic", "gradient", "lcd"]), "gradient")),
        ("show_unfilled", bool_opt(ctx, unfilled.as_ref(), true)),
    ])
}

/// Maps the `options` of a panel to the member `options` of format 1. The old footer of a table moves to `fields`.
fn options_out(ctx: &mut Ctx, ptype: Pt, options: Option<Node>, customs: &Customs, fields: &mut Out) -> Out {
    match ptype {
        Pt::Table => table_options(ctx, options, customs, fields),
        Pt::Timeseries => timeseries_options(ctx, options, customs),
        Pt::Stat => stat_options(ctx, options),
        Pt::Barchart => barchart_options(ctx, options, customs),
        Pt::Piechart => piechart_options(ctx, options),
        Pt::Xychart => xychart_options(ctx, options, customs),
        Pt::Bargauge => bargauge_options(ctx, options),
        // The importer maps the options of a text panel in `text.rs`: `panel.rs` never calls this arm.
        Pt::Text => Out::obj([]),
        Pt::StateTimeline => timeline_options(ctx, options, customs, false),
        Pt::StatusHistory => timeline_options(ctx, options, customs, true),
        Pt::Heatmap => bucket::heatmap_options(ctx, options).options,
        Pt::Histogram => bucket::histogram_options(ctx, options, customs),
        Pt::Logs => logs::logs_options(ctx, options),
        Pt::Gauge => gauge::gauge_options(ctx, options, customs),
        Pt::Geomap => geomap::geomap_options(ctx, options),
    }
}

/// Maps the `options` of a panel, and moves to the field configuration what the options hold for it: the unit and
/// the decimals of the value axis of a heatmap that buckets the values, and the old footer of a table (15.6).
/// `fields` is the field configuration.
pub(crate) fn options_with_fields(
    ctx: &mut Ctx,
    ptype: Pt,
    options: Option<Node>,
    customs: &Customs,
    mut fields: Out,
) -> (Out, Out) {
    if ptype != Pt::Heatmap {
        return (options_out(ctx, ptype, options, customs, &mut fields), fields);
    }
    let mapped = bucket::heatmap_options(ctx, options);
    bucket::merge_axis_units(ctx, &mut fields, mapped.pending);
    (mapped.options, fields)
}

pub(crate) use bucket::old_heatmap_member;

/// The `legend` of a state timeline or a status history (15.6). The mode `table` becomes `list`. A list of
/// calculations that is not empty, `sortBy` and `sortDesc` have no equal: the member is removed.
fn timeline_legend(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "legend");
    let show = w.take("showLegend");
    let mode = w.take("displayMode");
    let placement = w.take("placement");
    let calcs = w.take("calcs");
    let sort_by = w.take("sortBy");
    let sort_desc = w.take("sortDesc");
    w.finish(ctx);
    let mut visible = match &show {
        Some(s) => match as_flag(s.v) {
            Some(b) => {
                ctx.ok();
                b
            }
            None => {
                ctx.unsupported(&s.ptr, Some(s.v), "showLegend must be true or false", default_fallback());
                true
            }
        },
        None => true,
    };
    match mode.as_ref().map(|m| (m, m.str())) {
        None | Some((_, Some("list"))) => ctx.ok(),
        Some((_, Some("hidden"))) => {
            visible = false;
            ctx.ok();
        }
        Some((m, Some("table"))) => ctx.changed(&m.ptr, Some(m.v), "the table mode of the legend becomes list (4.33b)"),
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "no value table of 15.6 names this value", default_fallback())
        }
    }
    let place = enum_opt(ctx, placement.as_ref(), &same(&["bottom", "right"]), "bottom");
    if let Some(c) = calcs.as_ref() {
        if c.v.as_array().is_some_and(Vec::is_empty) {
            ctx.meta();
        } else {
            ctx.unsupported(
                &c.ptr,
                Some(c.v),
                "the legend of this panel has no values (4.33b)",
                Fallback::MemberRemoved,
            );
        }
    }
    if let Some(n) = sort_by.as_ref() {
        ctx.unsupported(&n.ptr, Some(n.v), "the legend of this panel has no sort (4.33b)", Fallback::MemberRemoved);
    }
    if let Some(n) = sort_desc.as_ref() {
        match as_flag(n.v) {
            Some(false) => ctx.meta(),
            _ => ctx.unsupported(
                &n.ptr,
                Some(n.v),
                "the legend of this panel has no sort (4.33b)",
                Fallback::MemberRemoved,
            ),
        }
    }
    Out::obj([
        ("show", Out::Bool(visible)),
        ("placement", place),
        ("mode", Out::s("list")),
        ("values", Out::Arr(vec![])),
    ])
}

/// The `tooltip` of a state timeline or a status history (15.6). A sort other than `none` and `hideZeros`
/// equal to `true` become the default. `maxHeight` and `maxWidth` are not read here: the walk lists them.
fn timeline_tooltip(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "tooltip");
    let mode = w.take("mode");
    let sort = w.take("sort");
    let hide_zeros = w.take("hideZeros");
    w.finish(ctx);
    let table: [Row; 3] = [("single", "single", false), ("multi", "multi", false), ("none", "hidden", false)];
    let mode_out = enum_opt(ctx, mode.as_ref(), &table, "single");
    if let Some(s) = sort.as_ref() {
        match s.str() {
            Some("none") => ctx.ok(),
            Some("asc" | "desc") => {
                ctx.changed(&s.ptr, Some(s.v), "the tooltip of this panel has no sort: the sort becomes none (4.33b)");
            }
            _ => ctx.unsupported(&s.ptr, Some(s.v), "no value table of 15.6 names this value", default_fallback()),
        }
    }
    if let Some(h) = hide_zeros.as_ref() {
        match as_flag(h.v) {
            Some(false) => ctx.ok(),
            Some(true) => {
                ctx.changed(&h.ptr, Some(h.v), "the tooltip of this panel has no hide-zeros: it becomes false (4.33b)");
            }
            None => ctx.unsupported(&h.ptr, Some(h.v), "the value must be true or false", default_fallback()),
        }
    }
    Out::obj([("mode", mode_out)])
}

/// The `custom.spanNulls` of a state timeline: a number (a time threshold) has no equal.
fn span_nulls(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    match node {
        Some(n) if n.v.is_number() => {
            ctx.unsupported(
                &n.ptr,
                Some(n.v),
                "a time threshold has no equal in format 3 (4.33b)",
                Fallback::other("false"),
            );
            Out::Bool(false)
        }
        other => bool_opt(ctx, other, false),
    }
}

/// The `custom.axisPlacement` of these panels gives `show_labels`: `hidden` gives `false`, `auto` gives `true`.
fn show_labels(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    match node.map(|n| (n, n.str())) {
        None => {
            ctx.ok();
            Out::Bool(true)
        }
        Some((_, Some("auto"))) => {
            ctx.meta();
            Out::Bool(true)
        }
        Some((_, Some("hidden"))) => {
            ctx.ok();
            Out::Bool(false)
        }
        Some((n, _)) => {
            ctx.unsupported(
                &n.ptr,
                Some(n.v),
                "the placement must be auto or hidden for this panel (4.33b)",
                Fallback::other("true"),
            );
            Out::Bool(true)
        }
    }
}

/// The options of a `state-timeline` and of a `status-history` (15.6, 4.33b).
fn timeline_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs, history: bool) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let merge = w.take("mergeValues");
    let show = w.take("showValue");
    let align = if history { None } else { w.take("alignValue") };
    let row = w.take("rowHeight");
    let col = if history { w.take("colWidth") } else { None };
    let per_page = w.take("perPage");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    let zone = w.take("timezone");
    let annotations = w.take("annotations");
    w.finish(ctx);
    if history && merge.is_some() {
        ctx.meta();
    }
    if let Some(n) = per_page.as_ref() {
        ctx.unsupported(
            &n.ptr,
            Some(n.v),
            "Hune has no pages of rows: the panel shows all the rows",
            Fallback::other("the panel shows all the rows"),
        );
    }
    for n in [zone.as_ref(), annotations.as_ref()].into_iter().flatten() {
        if crate::item::is_trivial(n.v) || crate::item::is_grafana_default(&n.ptr, n.v) {
            ctx.meta();
        } else {
            ctx.dropped(&n.ptr, Some(n.v), "Hune has one zone for the dashboard and no annotation (15.6)");
        }
    }
    let row_fb = Fallback::other("90");
    let type_fb = || Fallback::other("the default of the type");
    let mut members: Vec<(&str, Out)> = Vec::new();
    if !history {
        members.push(("merge_values", bool_opt(ctx, merge.as_ref(), true)));
    }
    members.push(("show_values", enum_opt(ctx, show.as_ref(), &same(&["auto", "always", "never"]), "auto")));
    if !history {
        members.push(("align_values", enum_opt(ctx, align.as_ref(), &same(&["left", "center", "right"]), "left")));
    }
    members.push(("row_height", percent_opt_with(ctx, row.as_ref(), 90, row_fb.clone())));
    if history {
        members.push(("col_width", percent_opt_with(ctx, col.as_ref(), 90, row_fb)));
    }
    let (line_default, fill_default) = if history { (1, 70) } else { (0, 70) };
    members.push(("line_width", int_opt_with(ctx, custom(customs, "lineWidth"), 0, 10, line_default, type_fb())));
    members.push(("fill_opacity", int_opt_with(ctx, custom(customs, "fillOpacity"), 0, 100, fill_default, type_fb())));
    if !history {
        members.push(("span_nulls", span_nulls(ctx, custom(customs, "spanNulls"))));
    }
    members.push(("show_labels", show_labels(ctx, custom(customs, "axisPlacement"))));
    members.push(("legend", timeline_legend(ctx, legend_node)));
    members.push(("tooltip", timeline_tooltip(ctx, tooltip_node)));
    Out::members(members)
}
