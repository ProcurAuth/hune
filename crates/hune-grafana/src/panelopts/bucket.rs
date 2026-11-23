// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options of the types `heatmap` and `histogram` (15.6, 4.33c, 4.33d).
//!
//! The heatmap of Grafana 10 and later is the only form that the importer reads. The old heatmap of Grafana 7
//! to 9 has panel members that no row names: [`old_heatmap_member`] finds the first one.

use hune_core::document::{is_valid_unit, parse_duration_ms};
use serde_json::Value;

use super::{
    Ctx, Customs, Fallback, Node, OptWalk, Out, Row, as_flag, as_int, as_num, bool_opt, custom, default_fallback,
    enum_opt, int_opt_with, legend, same, tooltip,
};

/// The members that only a heatmap of Grafana 7 to 9 has at the top level of the panel, in the order of 15.6.
const OLD_HEATMAP_MEMBERS: [&str; 9] = [
    "cards",
    "dataFormat",
    "xBucketSize",
    "xBucketNumber",
    "yBucketSize",
    "yBucketNumber",
    "yBucketBound",
    "hideZeroBuckets",
    "reverseYBuckets",
];

/// Finds the first member of the old heatmap at the top level of a panel of the form v1. A panel that has one
/// is `unsupported` with the fallback "panel removed". The function gives `true` in that case (15.6).
pub(crate) fn old_heatmap_member(ctx: &mut Ctx, panel: &Node) -> bool {
    let Some(found) = OLD_HEATMAP_MEMBERS.iter().find_map(|name| panel.get(name)) else { return false };
    let reason = "this is a heatmap of Grafana 7 to 9: Hune reads the heatmap of Grafana 10 and later only (4.33c)";
    ctx.unsupported(&found.ptr, Some(found.v), reason, Fallback::PanelRemoved);
    true
}

// ------------------------------------------------------------------ small readers

/// The text of a value of a bucket member: a text, or a number that Grafana writes as a number.
fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Tells if a value of a bucket member is empty: absent, `null` or the empty text.
fn is_empty_value(node: Option<&Node>) -> bool {
    node.is_none_or(|n| n.v.is_null() || n.v.as_str().is_some_and(|s| s.trim().is_empty()))
}

/// Reads a whole number in `min..=max` from a text of digits or from a whole JSON number.
fn whole(text: &str, min: i64, max: i64) -> Option<i64> {
    let n: i64 = if text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty() {
        text.parse().ok()?
    } else {
        let f: f64 = text.parse().ok()?;
        as_int(&serde_json::json!(f))?
    };
    (min..=max).contains(&n).then_some(n)
}

/// Reads a finite number from a number or from a number text.
fn number_or_text(v: &Value) -> Option<f64> {
    match v {
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        other => as_num(other),
    }
}

/// The kind of a bucket object: the rule of its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The time axis: the size is a duration of 1 ms to 365 days.
    Time,
    /// A linear value axis: the size is a number above 0.
    Linear,
    /// A log value axis: the value is the split, a whole number from 1 to 16.
    Log,
}

/// The longest time bucket: 365 days in milliseconds (4.33c).
const MAX_TIME_SIZE_MS: i64 = 31_536_000_000;

/// Maps the members `mode` and `value` of a bucket object. It gives the mode and the value of the document.
/// An absent mode is `size` (the rule of Grafana). An absent or empty value gives `count` with `null`.
/// A log scale has the mode `size`: a mode `count` is `changed`.
fn bucket(ctx: &mut Ctx, mode: Option<&Node>, value: Option<&Node>, kind: Kind) -> (&'static str, Out) {
    let count_null = ("count", Out::Null);
    let mode_text = match mode.map(|m| (m, m.str())) {
        None => "size",
        Some((_, Some("count"))) => "count",
        Some((_, Some("size"))) => "size",
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "the mode must be count or size", Fallback::other("count with null"));
            if let Some(v) = value {
                ctx.unsupported(&v.ptr, Some(v.v), "the value has no meaning with this mode", Fallback::other("null"));
            }
            return if kind == Kind::Log { ("size", Out::Null) } else { count_null };
        }
    };
    if kind == Kind::Log {
        match mode.filter(|_| mode_text == "count") {
            Some(m) => {
                ctx.changed(&m.ptr, Some(m.v), "the mode of a log scale is size: the value is the split (4.33c)")
            }
            None if mode.is_some() => ctx.ok(),
            None => {}
        }
        if is_empty_value(value) {
            if value.is_some() {
                ctx.ok();
            }
            return ("size", Out::Null);
        }
        let node = value.expect("a value that is not empty exists");
        return match value_text(node.v).and_then(|t| whole(&t, 1, 16)) {
            Some(n) => {
                ctx.ok();
                ("size", Out::Int(n))
            }
            None => {
                let why = "the split of a log scale must be a whole number from 1 to 16";
                ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::other("null"));
                ("size", Out::Null)
            }
        };
    }
    if is_empty_value(value) {
        // The mode has no effect without a value: Grafana makes the same buckets.
        ctx.ok();
        return count_null;
    }
    let node = value.expect("a value that is not empty exists");
    let text = value_text(node.v);
    let fail = |ctx: &mut Ctx, why: &str| {
        ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::other("count with null"));
        count_null
    };
    match (mode_text, kind) {
        ("count", _) => match text.as_deref().and_then(|t| whole(t, 1, 1000)) {
            Some(n) => {
                ctx.ok();
                ("count", if kind == Kind::Time { Out::Int(n) } else { Out::Num(n as f64) })
            }
            None => fail(ctx, "the count must be a whole number from 1 to 1000"),
        },
        (_, Kind::Time) => {
            let ms = text.as_deref().and_then(|t| {
                parse_duration_ms(t)
                    .and_then(|m| i64::try_from(m).ok())
                    .or_else(|| t.bytes().all(|b| b.is_ascii_digit()).then(|| t.parse::<i64>().ok()).flatten())
            });
            match ms.filter(|m| (1..=MAX_TIME_SIZE_MS).contains(m)) {
                Some(ms) => {
                    ctx.ok();
                    ("size", Out::Int(ms))
                }
                None => fail(ctx, "the size must be a length of 1 ms to 365 days, as 10m, or a whole number of ms"),
            }
        }
        _ => match number_or_text(node.v).filter(|n| *n > 0.0) {
            Some(n) => {
                ctx.ok();
                ("size", Out::Num(n))
            }
            None => fail(ctx, "the size must be a number above 0"),
        },
    }
}

/// Maps `calculation.xBuckets`. The member `scale` has no equal: Grafana refuses a log scale on the x axis.
fn x_buckets(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "xBuckets");
    let mode = w.take("mode");
    let value = w.take("value");
    let scale = w.take("scale");
    w.finish(ctx);
    if let Some(s) = scale.as_ref() {
        let linear = s.get("type").and_then(|t| t.str().map(|t| t == "linear")).unwrap_or(false);
        if crate::item::is_trivial(s.v) || (linear && s.v.as_object().is_some_and(|o| o.len() == 1)) {
            ctx.meta();
        } else {
            ctx.unsupported(
                &s.ptr,
                Some(s.v),
                "a scale of the time axis has no equal (4.33c)",
                Fallback::MemberRemoved,
            );
        }
    }
    let (mode, value) = bucket(ctx, mode.as_ref(), value.as_ref(), Kind::Time);
    Out::obj([("mode", Out::s(mode)), ("value", value)])
}

/// The scale of the value axis: `linear`, or the base of a log scale.
#[derive(Clone, Copy, PartialEq, Eq)]
enum YScale {
    Linear,
    Log2,
    Log10,
}

impl YScale {
    fn as_str(self) -> &'static str {
        match self {
            YScale::Linear => "linear",
            YScale::Log2 => "log2",
            YScale::Log10 => "log10",
        }
    }
}

/// Maps `calculation.yBuckets.scale`: `linear`, or `log` with the base 2 or 10. Another scale is `unsupported`.
fn y_scale(ctx: &mut Ctx, node: Option<Node>) -> YScale {
    let Some(node) = node.filter(|n| !n.is_null()) else { return YScale::Linear };
    let Some(mut w) = super::Walk::new(&node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the scale must be an object", Fallback::other("linear"));
        return YScale::Linear;
    };
    let kind = w.take("type");
    let base = w.take("log");
    w.finish(ctx);
    match kind.as_ref().map(|k| (k, k.str())) {
        None | Some((_, Some("linear"))) => {
            if base.is_some() {
                ctx.meta();
            }
            ctx.ok();
            YScale::Linear
        }
        Some((_, Some("log"))) => match base.as_ref().map(|b| (b, as_int(b.v))) {
            Some((_, Some(2))) => {
                ctx.ok();
                YScale::Log2
            }
            Some((_, Some(10))) => {
                ctx.ok();
                YScale::Log10
            }
            Some((b, _)) => {
                ctx.unsupported(
                    &b.ptr,
                    Some(b.v),
                    "the base of a log scale must be 2 or 10",
                    Fallback::other("linear"),
                );
                YScale::Linear
            }
            None => {
                ctx.unsupported(
                    &node.ptr,
                    Some(node.v),
                    "a log scale needs the base 2 or 10",
                    Fallback::other("linear"),
                );
                YScale::Linear
            }
        },
        Some((k, _)) => {
            ctx.unsupported(&k.ptr, Some(k.v), "the scale must be linear or log (4.33c)", Fallback::other("linear"));
            YScale::Linear
        }
    }
}

/// Maps `calculation.yBuckets`. It gives the object and the scale.
fn y_buckets(ctx: &mut Ctx, node: Option<Node>) -> (Out, YScale) {
    let mut w = OptWalk::new(ctx, node, "yBuckets");
    let mode = w.take("mode");
    let value = w.take("value");
    let scale_node = w.take("scale");
    w.finish(ctx);
    let scale = y_scale(ctx, scale_node);
    let kind = if scale == YScale::Linear { Kind::Linear } else { Kind::Log };
    let (mode, value) = bucket(ctx, mode.as_ref(), value.as_ref(), kind);
    (Out::obj([("mode", Out::s(mode)), ("value", value), ("scale", Out::s(scale.as_str()))]), scale)
}

/// Maps `calculation`: the two bucket objects of the document, and the scale of the value axis.
fn calculation(ctx: &mut Ctx, node: Option<Node>) -> (Out, Out, YScale) {
    let mut w = OptWalk::new(ctx, node, "calculation");
    let x = w.take("xBuckets");
    let y = w.take("yBuckets");
    w.finish(ctx);
    let x = x_buckets(ctx, x);
    let (y, scale) = y_buckets(ctx, y);
    (x, y, scale)
}

// ------------------------------------------------------------------ the value axis

/// A value that moves from `options.yAxis` to the defaults of the fields (15.6).
pub(crate) struct Pending {
    ptr: String,
    value: Value,
    member: &'static str,
    out: Out,
}

/// The result of the mapping of the options of a heatmap.
pub(crate) struct HeatmapOut {
    pub options: Out,
    /// The `unit` and the `decimals` of `yAxis` of a heatmap that buckets the values.
    pub pending: Vec<Pending>,
}

/// Maps `yAxis.min` and `yAxis.max`: a number or a number text. An empty member gives `null`.
fn axis_limit(ctx: &mut Ctx, node: Option<&Node>, log: bool) -> Out {
    let Some(node) = node.filter(|n| !is_empty_value(Some(*n))) else {
        ctx.ok();
        return Out::Null;
    };
    match number_or_text(node.v) {
        Some(n) if log && n <= 0.0 => {
            ctx.unsupported(&node.ptr, Some(node.v), "a log scale needs a value above 0", Fallback::other("null"));
            Out::Null
        }
        Some(n) => {
            ctx.ok();
            Out::Num(n)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the value must be a number", Fallback::other("null"));
            Out::Null
        }
    }
}

/// Maps `yAxis`: the options of the value axis, and the unit and the decimals to move.
fn y_axis(ctx: &mut Ctx, node: Option<Node>, calculate: bool, log: bool) -> (Out, Out, Out, Out, Vec<Pending>) {
    let mut w = OptWalk::new(ctx, node, "yAxis");
    let placement = w.take("axisPlacement");
    let reverse = w.take("reverse");
    let min = w.take("min");
    let max = w.take("max");
    let unit = w.take("unit");
    let decimals = w.take("decimals");
    let width = w.take("axisWidth");
    let label = w.take("axisLabel");
    w.finish(ctx);
    let table: [Row; 3] = [("left", "left", false), ("hidden", "hidden", false), ("right", "left", true)];
    let place = enum_opt(ctx, placement.as_ref(), &table, "left");
    let reverse = bool_opt(ctx, reverse.as_ref(), false);
    let min = axis_limit(ctx, min.as_ref(), log);
    let max = axis_limit(ctx, max.as_ref(), log);
    for n in [width.as_ref(), label.as_ref()].into_iter().flatten() {
        if crate::item::is_trivial(n.v) {
            ctx.meta();
        } else {
            ctx.dropped(&n.ptr, Some(n.v), "the engine sizes the axis and the heatmap has no axis label (9.24)");
        }
    }
    let mut pending = Vec::new();
    for (node, member) in [(unit.as_ref(), "unit"), (decimals.as_ref(), "decimals")] {
        let Some(n) = node else { continue };
        let out = if member == "unit" {
            n.str().filter(|u| !u.is_empty() && is_valid_unit(u)).map(Out::s)
        } else {
            as_int(n.v).filter(|d| (0..=20).contains(d)).map(Out::Int)
        };
        if crate::item::is_trivial(n.v) && member == "unit" || n.is_null() {
            ctx.meta();
        } else if !calculate {
            let why = "Grafana formats the names of the rows with it: Hune does not (4.33c)";
            ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::MemberRemoved);
        } else if let Some(out) = out {
            pending.push(Pending { ptr: n.ptr.clone(), value: n.v.clone(), member, out });
        } else {
            let why = if member == "unit" { "the unit is not in the list of 4.20" } else { "decimals must be 0 to 20" };
            ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::MemberRemoved);
        }
    }
    (place, reverse, min, max, pending)
}

/// Moves the `unit` and the `decimals` of `yAxis` to `fields.defaults` when the defaults have no such member.
pub(crate) fn merge_axis_units(ctx: &mut Ctx, fields: &mut Out, pending: Vec<Pending>) {
    for p in pending {
        let Out::Obj(top) = fields else { return };
        let Some((_, Out::Obj(defaults))) = top.iter_mut().find(|(k, _)| k == "defaults") else { return };
        if defaults.iter().any(|(k, _)| k == p.member) {
            let why = "fields.defaults has this member already: the member of the defaults stays (4.33c)";
            ctx.unsupported(&p.ptr, Some(&p.value), why, Fallback::MemberRemoved);
            continue;
        }
        let at =
            if p.member == "unit" { 0 } else { defaults.iter().position(|(k, _)| k == "unit").map_or(0, |i| i + 1) };
        defaults.insert(at, (p.member.to_owned(), p.out));
        ctx.changed(&p.ptr, Some(&p.value), &format!("the value moves to fields.defaults.{} (4.33c)", p.member));
    }
}

// ------------------------------------------------------------------ the colours

/// The schemes of the 10 values of 4.33c, by the name that Grafana writes.
const SCHEMES: [(&str, &str); 10] = [
    ("Oranges", "oranges"),
    ("Blues", "blues"),
    ("Greens", "greens"),
    ("Greys", "greys"),
    ("Purples", "purples"),
    ("Reds", "reds"),
    ("YlOrRd", "yl_or_rd"),
    ("RdYlGn", "rd_yl_gn"),
    ("Spectral", "spectral"),
    ("Viridis", "viridis"),
];

/// The other 28 schemes of the 38 of Grafana, and the second name of Cubehelix. Each one is `changed` to `oranges`.
const OTHER_SCHEMES: [&str; 29] = [
    "BrBG",
    "PiYG",
    "PRGn",
    "PuOr",
    "RdBu",
    "RdGy",
    "RdYlBu",
    "Turbo",
    "Cividis",
    "Magma",
    "Inferno",
    "Plasma",
    "Warm",
    "Cool",
    "Cubehelix",
    "CubehelixDefault",
    "BuGn",
    "BuPu",
    "GnBu",
    "OrRd",
    "PuBuGn",
    "PuBu",
    "PuRd",
    "RdPu",
    "YlGnBu",
    "YlGn",
    "YlOrBr",
    "Rainbow",
    "Sinebow",
];

fn scheme(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::s("oranges");
    };
    match node.str() {
        Some(name) => {
            if let Some((_, to)) = SCHEMES.iter().find(|(from, _)| *from == name) {
                ctx.ok();
                Out::s(to)
            } else if OTHER_SCHEMES.contains(&name) {
                ctx.changed(&node.ptr, Some(node.v), "Hune has 10 schemes: this scheme becomes oranges (4.33c)");
                Out::s("oranges")
            } else {
                // A name that Grafana does not know gets the scheme Spectral in Grafana.
                ctx.changed(&node.ptr, Some(node.v), "Grafana does not know this scheme: it uses Spectral (4.33c)");
                Out::s("spectral")
            }
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the scheme must be a text", Fallback::other("oranges"));
            Out::s("oranges")
        }
    }
}

fn exponent(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Num(0.5);
    };
    match as_num(node.v) {
        Some(v) if (0.1..=2.0).contains(&((v * 10.0).round() / 10.0)) => {
            let near = (v * 10.0).round() / 10.0;
            if (near - v).abs() < 1e-9 {
                ctx.ok();
            } else {
                ctx.changed(
                    &node.ptr,
                    Some(node.v),
                    "the exponent has one decimal: it becomes the nearest value (4.33c)",
                );
            }
            Out::Num(near)
        }
        _ => {
            ctx.unsupported(&node.ptr, Some(node.v), "the exponent must be 0.1 to 2", Fallback::other("0.5"));
            Out::Num(0.5)
        }
    }
}

/// Maps `color.min` and `color.max`: a number, or `null` when absent. The value `Some` is a number.
fn color_limit(ctx: &mut Ctx, node: Option<&Node>) -> Option<f64> {
    let node = node.filter(|n| !n.is_null())?;
    match as_num(node.v) {
        Some(n) => {
            ctx.ok();
            Some(n)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the value must be a number", Fallback::other("null"));
            None
        }
    }
}

fn color(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "color");
    let mode = w.take("mode");
    let scheme_node = w.take("scheme");
    let fill = w.take("fill");
    let scale = w.take("scale");
    let exp = w.take("exponent");
    let steps = w.take("steps");
    let reverse = w.take("reverse");
    let min = w.take("min");
    let max = w.take("max");
    w.finish(ctx);
    let fill_out = match fill.as_ref() {
        None => {
            ctx.ok();
            "dark-orange".to_owned()
        }
        Some(n) => {
            crate::colour::colour(ctx, n, Fallback::other("dark-orange")).unwrap_or_else(|| "dark-orange".to_owned())
        }
    };
    let lo = color_limit(ctx, min.as_ref());
    let hi = color_limit(ctx, max.as_ref());
    let (lo, hi) = match (lo, hi, min.as_ref(), max.as_ref()) {
        (Some(a), Some(b), Some(mn), Some(mx)) if a >= b => {
            let why = "min must be lower than max (4.33c)";
            ctx.unsupported(&mn.ptr, Some(mn.v), why, Fallback::other("null"));
            ctx.unsupported(&mx.ptr, Some(mx.v), why, Fallback::other("null"));
            (None, None)
        }
        other => (other.0, other.1),
    };
    Out::obj([
        ("mode", enum_opt(ctx, mode.as_ref(), &same(&["scheme", "opacity"]), "scheme")),
        ("scheme", scheme(ctx, scheme_node.as_ref())),
        ("fill", Out::Str(fill_out)),
        ("scale", enum_opt(ctx, scale.as_ref(), &same(&["exponential", "linear"]), "exponential")),
        ("exponent", exponent(ctx, exp.as_ref())),
        ("steps", int_opt_with(ctx, steps.as_ref(), 2, 128, 64, default_fallback())),
        ("reverse", bool_opt(ctx, reverse.as_ref(), false)),
        ("min", lo.map_or(Out::Null, Out::Num)),
        ("max", hi.map_or(Out::Null, Out::Num)),
    ])
}

// ------------------------------------------------------------------ the other heatmap options

/// Maps `filterValues`: `le` gives `hide_le` and `ge` gives `hide_ge`. The default `le` of Grafana, 1e-9, gives 0
///. An explicit `null` gives `null`.
fn filter_values(ctx: &mut Ctx, node: Option<Node>) -> (Out, Out) {
    let mut w = OptWalk::new(ctx, node, "filterValues");
    // An explicit `null` is a value here: it gives `null`. The walk of an object skips it, so read the raw node.
    let le_node = w.w.as_mut().and_then(|w| w.take("le"));
    let ge_node = w.w.as_mut().and_then(|w| w.take("ge"));
    w.finish(ctx);
    let limit = |ctx: &mut Ctx, n: Option<&Node>, default: Out, grafana_default: Option<f64>| -> Out {
        let Some(n) = n else {
            ctx.ok();
            return default;
        };
        if n.is_null() {
            ctx.ok();
            return Out::Null;
        }
        match as_num(n.v) {
            Some(v) if grafana_default == Some(v) => {
                ctx.ok();
                Out::Int(0)
            }
            Some(v) => {
                ctx.ok();
                Out::Num(v)
            }
            None => {
                ctx.unsupported(&n.ptr, Some(n.v), "the value must be a number", Fallback::other("the default"));
                default
            }
        }
    };
    let le = limit(ctx, le_node.as_ref(), Out::Int(0), Some(1e-9));
    let ge = limit(ctx, ge_node.as_ref(), Out::Null, None);
    (le, ge)
}

/// Maps `legend` of a heatmap: `show` gives `show_legend`.
fn heat_legend(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "legend");
    let show = w.take("show");
    w.finish(ctx);
    bool_opt(ctx, show.as_ref(), true)
}

/// Maps `tooltip` of a heatmap. `yHistogram` and `showColorScale` equal to `true` have no equal.
fn heat_tooltip(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "tooltip");
    let mode = w.take("mode");
    let y_histogram = w.take("yHistogram");
    let color_scale = w.take("showColorScale");
    let sort = w.take("sort");
    let hide_zeros = w.take("hideZeros");
    w.finish(ctx);
    let table: [Row; 3] = [("single", "single", false), ("multi", "multi", false), ("none", "hidden", false)];
    let mode_out = enum_opt(ctx, mode.as_ref(), &table, "single");
    for (n, what) in
        [(y_histogram.as_ref(), "the histogram of the column"), (color_scale.as_ref(), "the colour bar in the card")]
    {
        let Some(n) = n else { continue };
        match as_flag(n.v) {
            Some(false) => ctx.meta(),
            _ => ctx.unsupported(
                &n.ptr,
                Some(n.v),
                &format!("the card of Hune has no {what} (4.33c)"),
                Fallback::MemberRemoved,
            ),
        }
    }
    if let Some(s) = sort.as_ref() {
        match s.str() {
            Some("none") => ctx.ok(),
            Some("asc" | "desc") => {
                ctx.changed(&s.ptr, Some(s.v), "the tooltip of this panel has no sort: the sort becomes none (4.33c)")
            }
            _ => ctx.unsupported(&s.ptr, Some(s.v), "no value table of 15.6 names this value", default_fallback()),
        }
    }
    if let Some(h) = hide_zeros.as_ref() {
        match as_flag(h.v) {
            Some(false) => ctx.ok(),
            Some(true) => {
                ctx.changed(&h.ptr, Some(h.v), "the tooltip of this panel has no hide-zeros: it becomes false (4.33c)")
            }
            None => ctx.unsupported(&h.ptr, Some(h.v), "the value must be true or false", default_fallback()),
        }
    }
    Out::obj([("mode", mode_out)])
}

/// Maps `rowsFrame`: the layouts `le` and `ge` are `changed`, because the label stays at the middle of the row.
fn rows_frame(ctx: &mut Ctx, node: Option<Node>) {
    let mut w = OptWalk::new(ctx, node, "rowsFrame");
    let layout = w.take("layout");
    let value = w.take("value");
    let scale = w.take("yBucketScale");
    w.finish(ctx);
    if let Some(l) = layout.as_ref() {
        match l.str() {
            Some("auto" | "unknown") => ctx.meta(),
            Some("le" | "ge") => {
                ctx.changed(&l.ptr, Some(l.v), "the label of a row stays at the middle of the row (4.33c)")
            }
            _ => ctx.unsupported(
                &l.ptr,
                Some(l.v),
                "the layout must be auto, le, ge or unknown",
                Fallback::MemberRemoved,
            ),
        }
    }
    for n in [value.as_ref(), scale.as_ref()].into_iter().flatten() {
        if crate::item::is_trivial(n.v) {
            ctx.meta();
        } else {
            let why = "the axis of a frame that is already bucketed is ordinal (4.33c)";
            ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::MemberRemoved);
        }
    }
}

/// A member that Hune does not have: metadata when it has the default of Grafana, else `dropped`.
fn drop_unless_default(ctx: &mut Ctx, node: Option<&Node>, default: &Value, reason: &str) {
    let Some(n) = node else { return };
    if crate::item::is_trivial(n.v) || n.v == default {
        ctx.meta();
    } else {
        ctx.dropped(&n.ptr, Some(n.v), reason);
    }
}

/// Maps the `options` of a `heatmap` (15.6, 4.33c).
pub(crate) fn heatmap_options(ctx: &mut Ctx, options: Option<Node>) -> HeatmapOut {
    let mut w = OptWalk::new(ctx, options, "options");
    let calculate = w.take("calculate");
    let calculation_node = w.take("calculation");
    let y_axis_node = w.take("yAxis");
    let color_node = w.take("color");
    let cell_gap = w.take("cellGap");
    let cell_radius = w.take("cellRadius");
    let cell_values = w.take("cellValues");
    let filter_node = w.take("filterValues");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    let rows_node = w.take("rowsFrame");
    let selection = w.take("selectionMode");
    let exemplars = w.take("exemplars");
    let show_value = w.take("showValue");
    w.finish(ctx);
    let calc = bool_opt(ctx, calculate.as_ref(), false);
    let calc_flag = matches!(calc, Out::Bool(true));
    let (x, y, scale) = calculation(ctx, calculation_node);
    let (place, reverse, min, max, pending) = y_axis(ctx, y_axis_node, calc_flag, scale != YScale::Linear);
    let color_out = color(ctx, color_node);
    let gap = int_opt_with(ctx, cell_gap.as_ref(), 0, 25, 1, Fallback::other("1"));
    if let Some(n) = cell_radius.as_ref() {
        drop_unless_default(ctx, Some(n), &Value::Null, "Hune draws square cells (4.33c)");
    }
    let mut values = OptWalk::new(ctx, cell_values, "cellValues");
    let (unit, decimals) = (values.take("unit"), values.take("decimals"));
    values.finish(ctx);
    for n in [unit.as_ref(), decimals.as_ref()] {
        drop_unless_default(ctx, n, &Value::Null, "Hune writes the count in the short form (4.33c)");
    }
    let (le, ge) = filter_values(ctx, filter_node);
    let show = heat_legend(ctx, legend_node);
    let tip = heat_tooltip(ctx, tooltip_node);
    rows_frame(ctx, rows_node);
    drop_unless_default(ctx, selection.as_ref(), &serde_json::json!("x"), "Hune has no selection by drag (4.33c)");
    drop_unless_default(
        ctx,
        exemplars.as_ref(),
        &serde_json::json!({"color": "rgba(255,0,255,0.7)"}),
        "Hune has no exemplar (4.33c)",
    );
    drop_unless_default(ctx, show_value.as_ref(), &serde_json::json!("auto"), "Hune has no value in the cell (4.33c)");
    let options = Out::obj([
        ("calculate", calc),
        ("x_buckets", x),
        ("y_buckets", y),
        ("y_axis", place),
        ("y_reverse", reverse),
        ("y_min", min),
        ("y_max", max),
        ("color", color_out),
        ("cell_gap", gap),
        ("hide_le", le),
        ("hide_ge", ge),
        ("show_legend", show),
        ("tooltip", tip),
    ]);
    HeatmapOut { options, pending }
}

// ------------------------------------------------------------------ the histogram

/// Maps `bucketSize`: a number above 0. An absent member, 0 and the empty text give `null`.
fn bucket_size(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Null;
    };
    match &node.v {
        Value::Null => Out::Null,
        Value::String(s) if s.is_empty() => {
            ctx.ok();
            Out::Null
        }
        Value::Number(_) => match as_num(node.v) {
            Some(0.0) => {
                ctx.ok();
                Out::Null
            }
            Some(n) if n > 0.0 => {
                ctx.ok();
                Out::Num(n)
            }
            _ => {
                ctx.unsupported(&node.ptr, Some(node.v), "the size must be a number above 0", Fallback::other("null"));
                Out::Null
            }
        },
        _ => {
            ctx.unsupported(&node.ptr, Some(node.v), "the size must be a number above 0", Fallback::other("null"));
            Out::Null
        }
    }
}

fn bucket_offset(ctx: &mut Ctx, node: Option<&Node>) -> Out {
    let Some(node) = node else {
        ctx.ok();
        return Out::Int(0);
    };
    match as_num(node.v) {
        Some(n) => {
            ctx.ok();
            Out::Num(n)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the offset must be a finite number", Fallback::other("0"));
            Out::Int(0)
        }
    }
}

/// Maps the `options` of a `histogram` (15.6, 4.33d). The members `stacking`, `lineWidth` and `fillOpacity` come
/// from the defaults of the fields (`customs`).
pub(crate) fn histogram_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let count = w.take("bucketCount");
    let size = w.take("bucketSize");
    let offset = w.take("bucketOffset");
    let combine = w.take("combine");
    let legend_node = w.take("legend");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    let type_fb = || Fallback::other("the default of the type");
    Out::obj([
        ("bucket_count", int_opt_with(ctx, count.as_ref(), 1, 1000, 30, Fallback::other("30"))),
        ("bucket_size", bucket_size(ctx, size.as_ref())),
        ("bucket_offset", bucket_offset(ctx, offset.as_ref())),
        ("combine", bool_opt(ctx, combine.as_ref(), false)),
        ("stacking", enum_opt(ctx, custom(customs, "stacking.mode"), &same(&["none", "normal", "percent"]), "none")),
        ("line_width", int_opt_with(ctx, custom(customs, "lineWidth"), 0, 10, 1, type_fb())),
        ("fill_opacity", int_opt_with(ctx, custom(customs, "fillOpacity"), 0, 100, 80, type_fb())),
        ("legend", legend(ctx, legend_node, false)),
        ("tooltip", tooltip(ctx, tooltip_node)),
    ])
}
