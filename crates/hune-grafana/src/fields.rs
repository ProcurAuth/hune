//! The field configuration (4.17 to 4.23): defaults, overrides, properties, thresholds,
//! colours, mappings and links. The rows are those of the field table of 15.6.

use std::collections::BTreeMap;

use hune_core::document::{PatternForm, check_pattern, is_valid_unit, is_valid_url_prefix};
use hune_core::limits::{
    MAX_FOOTER_REDUCERS, MAX_LINKS, MAX_MAPPINGS, MAX_MATCHER_NAMES, MAX_OVERRIDES, MAX_THRESHOLD_STEPS, MAX_URL_BYTES,
    MAX_VALUE_BYTES,
};
use serde_json::Value;

use crate::colour::colour;
use crate::item::{Class, Ctx, Fallback};
use crate::options::ImportOptions;
use crate::out::Out;
use crate::panelopts::{Pt, calc_name};
use crate::util::{as_flag, as_int, as_num, chars};
use crate::walk::{Node, Walk};

/// The context of the field configuration of one panel.
pub(crate) struct FieldCx<'a> {
    pub opts: &'a ImportOptions,
    pub ptype: Pt,
    /// The refs of the queries of the panel: the Grafana `refId` and the ref of Hune (15.6).
    pub refs: &'a crate::query::RefMap,
}

/// The values of `defaults.custom` that format 1 reads as panel options (4.28, 4.32).
pub(crate) type Customs<'a> = BTreeMap<&'static str, Node<'a>>;

const ORDER: [&str; 35] = [
    "unit",
    "decimals",
    "min",
    "max",
    "color",
    "thresholds",
    "mappings",
    "links",
    "display_name",
    "no_value",
    "width",
    "align",
    "cell",
    "filterable",
    "hidden",
    "footer",
    "inspect",
    "hide_from",
    "axis_placement",
    "axis_label",
    "soft_min",
    "soft_max",
    "axis_grid",
    "draw",
    "line_width",
    "fill_opacity",
    "line_dash",
    "show_points",
    "point_size",
    "transform",
    "line_dash_array",
    "axis_centered_zero",
    "axis_width",
    "axis_border",
    "axis_color",
];

/// A property set (4.18): the members that are set, in the canonical order.
#[derive(Default)]
pub(crate) struct Props {
    slots: Vec<(usize, Out)>,
    min: Option<(f64, String)>,
    max: Option<(f64, String)>,
    soft_min: Option<(f64, String)>,
    soft_max: Option<(f64, String)>,
}

impl Props {
    fn set(&mut self, name: &str, out: Out) {
        let order = ORDER.iter().position(|n| *n == name).unwrap_or(ORDER.len());
        self.slots.retain(|(o, _)| *o != order);
        self.slots.push((order, out));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Checks `min` less than `max` (4.18, rule 2). The item `unsupported` names the pointer of `max`.
    fn check_range(&mut self, ctx: &mut Ctx) {
        if let (Some((min, _)), Some((max, max_ptr))) = (&self.min, &self.max)
            && min >= max
        {
            let ptr = max_ptr.clone();
            ctx.unsupported(
                &ptr,
                None,
                "min must be less than max in one property set",
                Fallback::DashboardNotImported,
            );
        }
    }

    /// Checks `soft_min` less than `soft_max` (4.18, rule 7). A pair that breaks the rule is `unsupported`:
    /// both members are removed (15.6).
    fn check_soft(&mut self, ctx: &mut Ctx) {
        if let (Some((min, _)), Some((max, max_ptr))) = (&self.soft_min, &self.soft_max)
            && min >= max
        {
            let ptr = max_ptr.clone();
            ctx.unsupported(
                &ptr,
                None,
                "axisSoftMin must be less than axisSoftMax in one property set",
                prop_removed(),
            );
            self.slots.retain(|(o, _)| ORDER[*o] != "soft_min" && ORDER[*o] != "soft_max");
        }
    }

    fn into_out(mut self) -> Out {
        self.slots.sort_by_key(|(o, _)| *o);
        Out::Obj(self.slots.into_iter().map(|(o, v)| (ORDER[o].to_owned(), v)).collect())
    }
}

fn bad_type(ctx: &mut Ctx, node: &Node, what: &str, fallback: Fallback) {
    ctx.unsupported(&node.ptr, Some(node.v), what, fallback);
}

fn prop_removed() -> Fallback {
    Fallback::PropertyRemoved
}

/// Maps one Grafana property: a member of `defaults` or the `id` and `value` of an override item.
/// `key` is the member name, for example `unit` or `custom.width`.
pub(crate) fn property<'a>(
    fcx: &FieldCx<'_>,
    ctx: &mut Ctx,
    key: &str,
    node: &Node<'a>,
    in_defaults: bool,
    props: &mut Props,
    customs: &mut Customs<'a>,
) {
    if node.is_null() {
        // A null soft limit is metadata (15.6). Another null property has no effect and counts as mapped.
        if matches!(key, "custom.axisSoftMin" | "custom.axisSoftMax") {
            ctx.meta();
        } else {
            ctx.ok();
        }
        return;
    }
    let table_only = [
        "custom.width",
        "custom.align",
        "custom.cellOptions",
        "custom.displayMode",
        "custom.filterable",
        "custom.hidden",
        "custom.footer",
        "custom.footer.reducers",
    ];
    if table_only.contains(&key) && fcx.ptype != Pt::Table {
        let reason = "this property is only for a table panel (4.18)";
        ctx.unsupported(&node.ptr, Some(node.v), reason, prop_removed());
        return;
    }
    match key {
        "unit" => match node.str() {
            Some("") => ctx.ok(),
            Some(u) if is_valid_unit(u) => {
                props.set("unit", Out::s(u));
                ctx.ok();
            }
            Some(_) => ctx.unsupported(&node.ptr, Some(node.v), "the unit is not in the list of 4.20", prop_removed()),
            None => bad_type(ctx, node, "the unit must be a text", prop_removed()),
        },
        "decimals" => match as_int(node.v).filter(|d| (0..=20).contains(d)) {
            Some(d) => {
                props.set("decimals", Out::Int(d));
                ctx.ok();
            }
            None => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "decimals must be an integer from 0 to 20",
                Fallback::DashboardNotImported,
            ),
        },
        "min" | "max" => match as_num(node.v) {
            Some(n) => {
                props.set(key, Out::Num(n));
                if key == "min" {
                    props.min = Some((n, node.ptr.clone()));
                } else {
                    props.max = Some((n, node.ptr.clone()));
                }
                ctx.ok();
            }
            None => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the value must be a finite number",
                Fallback::DashboardNotImported,
            ),
        },
        "displayName" => match node.str() {
            Some("") => ctx.ok(),
            Some(t) if (1..=256).contains(&chars(t)) => {
                props.set("display_name", Out::s(t));
                ctx.ok();
            }
            _ => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the display name must be a text of 1 to 256 characters",
                Fallback::DashboardNotImported,
            ),
        },
        "noValue" => {
            if !in_defaults {
                let reason = "no_value is permitted only in the defaults (4.18)";
                ctx.unsupported(&node.ptr, Some(node.v), reason, prop_removed());
            } else {
                match node.str() {
                    Some("") => ctx.ok(),
                    Some(t) if (1..=256).contains(&chars(t)) => {
                        props.set("no_value", Out::s(t));
                        ctx.ok();
                    }
                    _ => ctx.unsupported(
                        &node.ptr,
                        Some(node.v),
                        "the text must have 1 to 256 characters",
                        Fallback::DashboardNotImported,
                    ),
                }
            }
        }
        "thresholds" => {
            if let Some(out) = thresholds(ctx, node) {
                props.set("thresholds", out);
            }
        }
        "color" => {
            if let Some(out) = colour_property(ctx, node) {
                props.set("color", out);
            }
        }
        "mappings" => {
            if let Some(out) = mappings(ctx, node) {
                props.set("mappings", out);
            }
        }
        "links" => {
            let list = links(ctx, fcx.opts, node);
            if !list.is_empty() {
                props.set("links", Out::Arr(list));
            }
        }
        "custom.width" => match as_int(node.v).filter(|w| (10..=4000).contains(w)) {
            Some(w) => {
                props.set("width", Out::Int(w));
                ctx.ok();
            }
            None => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the width must be an integer from 10 to 4000",
                Fallback::DashboardNotImported,
            ),
        },
        "custom.align" => match node.str() {
            Some("") => ctx.ok(),
            Some(a @ ("auto" | "left" | "center" | "right")) => {
                props.set("align", Out::s(a));
                ctx.ok();
            }
            _ => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the align is not auto, left, center or right",
                Fallback::DashboardNotImported,
            ),
        },
        "custom.cellOptions" => {
            let out = cell_options(ctx, node, in_defaults, customs);
            props.set("cell", out);
        }
        "custom.footer" | "custom.footer.reducers" => footer_property(ctx, key, node, props),
        "custom.hideFrom" => hide_from_property(fcx, ctx, node, in_defaults, props),
        "custom.displayMode" => {
            let out = cell_display_mode(ctx, node);
            props.set("cell", out);
        }
        // The inspect of a cell (4.18, 15.6): `false` in the defaults is the Grafana default, so it is metadata.
        "custom.inspect" if fcx.ptype == Pt::Table => match as_flag(node.v) {
            Some(false) if in_defaults => ctx.meta(),
            Some(b) => {
                props.set("inspect", Out::Bool(b));
                ctx.ok();
            }
            None => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the value must be true or false",
                Fallback::DashboardNotImported,
            ),
        },
        "custom.filterable" | "custom.hidden" => match as_flag(node.v) {
            Some(b) => {
                let name = if key == "custom.filterable" { "filterable" } else { "hidden" };
                props.set(name, Out::Bool(b));
                ctx.ok();
            }
            None => ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the value must be true or false",
                Fallback::DashboardNotImported,
            ),
        },
        _ => {
            if !series_property(fcx, ctx, key, node, in_defaults, props) {
                custom_source_or_other(fcx, ctx, key, node, in_defaults, customs);
            }
        }
    }
}

/// The five style members of an override of a `timeseries` (15.6).
const STYLE_MEMBERS: [&str; 5] = ["drawStyle", "lineWidth", "fillOpacity", "showPoints", "pointSize"];

/// Maps the members of the axes and of the style of a series (4.18, 15.6): `custom.axisPlacement`,
/// `axisLabel`, `axisSoftMin`, `axisSoftMax`, `lineStyle` and `transform`, and the five style members in an
/// override. Gives `false` when the key is not one of these or the panel type has no such property: then the
/// general rule of a `custom` member applies (metadata for a default value, else `unsupported`).
fn series_property(
    fcx: &FieldCx<'_>,
    ctx: &mut Ctx,
    key: &str,
    node: &Node,
    in_defaults: bool,
    props: &mut Props,
) -> bool {
    let timeseries = fcx.ptype == Pt::Timeseries;
    let axis_type = timeseries || fcx.ptype == Pt::Barchart;
    let Some(name) = key.strip_prefix("custom.") else { return false };
    match name {
        "axisPlacement" if timeseries => axis_placement(ctx, node, in_defaults, props),
        "axisLabel" if axis_type => axis_label(ctx, node, props),
        "axisSoftMin" | "axisSoftMax" if axis_type => soft_limit(ctx, name, node, props),
        "axisGridShow" if fcx.ptype == Pt::Barchart => axis_grid(ctx, node, props),
        // Wave H, item P14 (4.18): the look of the y axis.
        "axisCenteredZero" | "axisBorderShow" | "axisWidth" | "axisColorMode" if axis_type => {
            axis_look(ctx, name, node, in_defaults, props)
        }
        "lineStyle" if timeseries => line_style(ctx, node, in_defaults, props),
        "transform" if timeseries => series_transform(ctx, node, props),
        n if !in_defaults && timeseries && STYLE_MEMBERS.contains(&n) => style_member(ctx, n, node, props),
        _ => return false,
    }
    true
}

fn axis_placement(ctx: &mut Ctx, node: &Node, in_defaults: bool, props: &mut Props) {
    match node.str() {
        Some("auto") if in_defaults => ctx.meta(),
        Some(p @ ("auto" | "left" | "right" | "hidden")) => {
            props.set("axis_placement", Out::s(p));
            ctx.ok();
        }
        Some("") => ctx.meta(),
        _ => {
            ctx.unsupported(&node.ptr, Some(node.v), "the placement is not auto, left, right or hidden", prop_removed())
        }
    }
}

fn axis_label(ctx: &mut Ctx, node: &Node, props: &mut Props) {
    let Some(text) = node.str() else {
        bad_type(ctx, node, "the axis label must be a text", prop_removed());
        return;
    };
    if text.is_empty() {
        ctx.meta();
        return;
    }
    if chars(text) > 256 {
        let cut: String = text.chars().take(256).collect();
        ctx.changed(&node.ptr, Some(node.v), "the axis label has more than 256 characters: it is cut");
        props.set("axis_label", Out::Str(cut));
    } else {
        props.set("axis_label", Out::s(text));
        ctx.ok();
    }
}

/// Maps `custom.axisGridShow` of a `barchart` (4.18, wave H item P03): `true` (On) and `false` (Off) map. `null`
/// is Auto, the state of an absent member: metadata.
fn axis_grid(ctx: &mut Ctx, node: &Node, props: &mut Props) {
    match node.v.as_bool() {
        Some(on) => {
            props.set("axis_grid", Out::Bool(on));
            ctx.ok();
        }
        None if node.v.is_null() => ctx.meta(),
        None => ctx.unsupported(&node.ptr, Some(node.v), "the value must be true or false", prop_removed()),
    }
}

/// Maps `custom.axisCenteredZero`, `axisBorderShow`, `axisWidth` and `axisColorMode` (4.18, wave H item P14). The Grafana default in
/// `defaults` (`false`, `text`) and `null` are metadata; an override keeps an explicit value, so it can undo the defaults.
fn axis_look(ctx: &mut Ctx, name: &str, node: &Node, in_defaults: bool, props: &mut Props) {
    if node.v.is_null() {
        ctx.meta();
        return;
    }
    let (slot, out, default) = match name {
        "axisWidth" => ("axis_width", as_int(node.v).filter(|v| (0..=500).contains(v)).map(Out::Int), false),
        "axisColorMode" => (
            "axis_color",
            node.str().filter(|v| matches!(*v, "text" | "series")).map(Out::s),
            node.str() == Some("text"),
        ),
        _ => {
            let slot = if name == "axisBorderShow" { "axis_border" } else { "axis_centered_zero" };
            (slot, node.v.as_bool().map(Out::Bool), node.v == &Value::Bool(false))
        }
    };
    match out {
        Some(_) if default && in_defaults => ctx.meta(),
        Some(out) => {
            props.set(slot, out);
            ctx.ok();
        }
        None => ctx.unsupported(&node.ptr, Some(node.v), "this value is out of the range of 4.18", prop_removed()),
    }
}

fn soft_limit(ctx: &mut Ctx, name: &str, node: &Node, props: &mut Props) {
    let Some(n) = as_num(node.v) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the soft limit must be a finite number", prop_removed());
        return;
    };
    if name == "axisSoftMin" {
        props.set("soft_min", Out::Num(n));
        props.soft_min = Some((n, node.ptr.clone()));
    } else {
        props.set("soft_max", Out::Num(n));
        props.soft_max = Some((n, node.ptr.clone()));
    }
    ctx.ok();
}

/// Maps `custom.lineStyle`: `fill` gives `line_dash` and `dash` gives `line_dash_array` (wave H, item P14). Grafana draws `[10, 10]` when
/// `dash` is absent, also for `dot` (whose default in Hune is `[0, 10]`), and draws dashes for a `dash` with no `fill`. An array that is not 1 to 8
/// integers from 0 to 100 (one above 0) is `unsupported`, and the default of the fill stays.
fn line_style(ctx: &mut Ctx, node: &Node, in_defaults: bool, props: &mut Props) {
    let Some(mut w) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "lineStyle must be an object", prop_removed());
        return;
    };
    let fill = w.take("fill");
    let dash = w.take("dash");
    w.finish(ctx);
    let value = match fill.as_ref().map(|f| f.str()) {
        Some(Some(f @ ("solid" | "dash" | "dot"))) => Some(f),
        None => None,
        Some(_) => {
            let at = fill.as_ref().unwrap_or(node);
            ctx.unsupported(&at.ptr, Some(at.v), "the fill is not solid, dash or dot", prop_removed());
            return;
        }
    };
    if value == Some("solid") || (value.is_none() && dash.is_none()) {
        if dash.is_some() {
            ctx.meta();
        }
        if value.is_none() || in_defaults {
            ctx.meta();
            return;
        }
        props.set("line_dash", Out::s("solid"));
        ctx.ok();
        return;
    }
    let array: Option<Vec<i64>> = match &dash {
        None => Some(vec![10, 10]),
        Some(d) => {
            d.v.as_array()
                .filter(|a| (1..=8).contains(&a.len()))
                .and_then(|a| a.iter().map(|x| as_int(x).filter(|v| (0..=100).contains(v))).collect())
        }
    };
    match array.filter(|a| a.iter().any(|v| *v > 0)) {
        Some(a) => {
            let own = if value == Some("dot") { [0, 10] } else { [10, 10] };
            let write = value.is_none() || a != own;
            if write {
                props.set("line_dash_array", Out::Arr(a.into_iter().map(Out::Int).collect()));
            }
            if dash.is_some() {
                if write { ctx.ok() } else { ctx.meta() }
            }
        }
        None => {
            let d = dash.as_ref().unwrap_or(node);
            let why =
                "the dash array is not 1 to 8 integers from 0 to 100: the line has the default dashes of the fill";
            ctx.unsupported(&d.ptr, Some(d.v), why, Fallback::other("the fill value"));
        }
    }
    if let Some(v) = value {
        props.set("line_dash", Out::s(v));
    }
    ctx.ok();
}

fn series_transform(ctx: &mut Ctx, node: &Node, props: &mut Props) {
    match node.str() {
        Some("negative-Y") => {
            props.set("transform", Out::s("negative_y"));
            ctx.ok();
        }
        Some("") => ctx.meta(),
        _ => ctx.unsupported(&node.ptr, Some(node.v), "this transform has no equal in 4.18", prop_removed()),
    }
}

/// The five style members of an override of a `timeseries`: the value tables of 4.28 apply, with the ranges
/// of 4.18.
fn style_member(ctx: &mut Ctx, name: &str, node: &Node, props: &mut Props) {
    let (slot, out) = match name {
        "drawStyle" => ("draw", node.str().filter(|v| matches!(*v, "line" | "bars" | "points")).map(Out::s)),
        "showPoints" => ("show_points", node.str().filter(|v| matches!(*v, "auto" | "always" | "never")).map(Out::s)),
        "lineWidth" => ("line_width", as_int(node.v).filter(|v| (0..=10).contains(v)).map(Out::Int)),
        "fillOpacity" => ("fill_opacity", as_int(node.v).filter(|v| (0..=100).contains(v)).map(Out::Int)),
        _ => ("point_size", as_int(node.v).filter(|v| (1..=40).contains(v)).map(Out::Int)),
    };
    match out {
        Some(out) => {
            props.set(slot, out);
            ctx.ok();
        }
        None => ctx.unsupported(&node.ptr, Some(node.v), "this value is out of the range of 4.18", prop_removed()),
    }
}

fn custom_source_or_other<'a>(
    fcx: &FieldCx<'_>,
    ctx: &mut Ctx,
    key: &str,
    node: &Node<'a>,
    in_defaults: bool,
    customs: &mut Customs<'a>,
) {
    let sources: &[&'static str] = match fcx.ptype {
        Pt::Timeseries => &[
            "drawStyle",
            "lineInterpolation",
            "lineWidth",
            "fillOpacity",
            "showPoints",
            "pointSize",
            "spanNulls",
            "stacking",
            "scaleDistribution",
            "thresholdsStyle",
            "gradientMode",
            "barAlignment",
        ],
        Pt::Barchart => &["thresholdsStyle", "fillOpacity", "lineWidth", "gradientMode"],
        Pt::Xychart => &["show", "pointSize", "lineWidth"],
        Pt::Table => &["wrapText"],
        Pt::StateTimeline => &["lineWidth", "fillOpacity", "spanNulls", "axisPlacement"],
        Pt::StatusHistory => &["lineWidth", "fillOpacity", "axisPlacement"],
        Pt::Histogram => &["lineWidth", "fillOpacity", "stacking"],
        Pt::Gauge => &["neutral"],
        _ => &[],
    };
    if let Some(name) = key.strip_prefix("custom.")
        && let Some(src) = sources.iter().find(|s| **s == name)
    {
        if !in_defaults {
            let reason = "this member of custom is a panel option: it has no meaning in an override (15.6)";
            ctx.unsupported(&node.ptr, Some(node.v), reason, prop_removed());
            return;
        }
        // Two sources are objects: the member `mode` or `type` gives the value.
        // The member `pointSize` of an xychart of Grafana 12 is an object too: `fixed` gives the value.
        let nested = match *src {
            "stacking" => Some("mode"),
            "scaleDistribution" => Some("type"),
            "thresholdsStyle" => Some("mode"),
            "pointSize" if fcx.ptype == Pt::Xychart && node.v.is_object() => Some("fixed"),
            _ => None,
        };
        match nested {
            None => {
                customs.insert(src, node.clone());
                ctx.ok();
            }
            Some(inner) => {
                let Some(mut w) = Walk::new(node) else {
                    ctx.unsupported(&node.ptr, Some(node.v), "the value must be an object", prop_removed());
                    return;
                };
                if let Some(n) = w.take(inner) {
                    let flat: &'static str = match *src {
                        "stacking" => "stacking.mode",
                        "thresholdsStyle" => "thresholdsStyle.mode",
                        "pointSize" => "pointSize",
                        _ => "scaleDistribution.type",
                    };
                    customs.insert(flat, n);
                    ctx.ok();
                }
                if *src == "stacking" && fcx.ptype == Pt::Histogram {
                    // The group of a stack has no meaning for a histogram (15.6): it is metadata.
                    w.meta(ctx, &["group"]);
                }
                match *src {
                    "scaleDistribution" => log_base(ctx, &mut w, customs),
                    "pointSize" => point_size_range(ctx, &mut w),
                    _ => {}
                }
                w.finish(ctx);
            }
        }
        return;
    }
    if matches!(fcx.ptype, Pt::StateTimeline | Pt::StatusHistory)
        && let Some(name) = key.strip_prefix("custom.")
    {
        timeline_custom(fcx, ctx, name, node, in_defaults);
        return;
    }
    if matches!(fcx.ptype, Pt::Heatmap | Pt::Histogram)
        && let Some(name) = key.strip_prefix("custom.")
    {
        bucket_custom(fcx, ctx, name, node, in_defaults);
        return;
    }
    if let Some(why) = key.strip_prefix("custom.").and_then(|name| turns_default_off(fcx.ptype, name, node.v)) {
        ctx.unsupported(&node.ptr, Some(node.v), why, prop_removed());
        return;
    }
    if crate::item::is_trivial(node.v) || (in_defaults && custom_default(fcx.ptype, key, node)) {
        ctx.meta();
        return;
    }
    let reason = if key.starts_with("custom.") {
        "this member of custom is not a panel option of 4.28 or 4.32"
    } else {
        "this property is not in the property set of 4.18"
    };
    ctx.unsupported(&node.ptr, Some(node.v), reason, prop_removed());
}

/// Maps a member of `custom` that is not an option of a state timeline or a status history (15.6, 4.33b).
/// `spanNulls` of a status history is metadata. `insertNulls` is metadata when it is off, else `unsupported`.
/// The style members of a time series are `dropped`, and metadata when they have the Grafana default.
fn timeline_custom(fcx: &FieldCx<'_>, ctx: &mut Ctx, name: &str, node: &Node, in_defaults: bool) {
    let key = format!("custom.{name}");
    if name == "spanNulls" && fcx.ptype == Pt::StatusHistory {
        ctx.meta();
    } else if name == "insertNulls" {
        if crate::item::is_trivial(node.v) {
            ctx.meta();
        } else {
            let reason = "Hune has no option to insert nulls (19.20d)";
            ctx.unsupported(&node.ptr, Some(node.v), reason, Fallback::MemberRemoved);
        }
    } else if crate::item::is_trivial(node.v)
        || (in_defaults && (custom_default(fcx.ptype, &key, node) || series_default(name, node.v)))
    {
        ctx.meta();
    } else {
        ctx.dropped(&node.ptr, Some(node.v), "this style of a series has no equal in this panel (4.33b)");
    }
}

/// Maps a member of `custom` that is not an option of a `heatmap` or a `histogram` (15.6, 4.33c, 4.33d). The
/// member `gradientMode` of a histogram is `unsupported` when it is not `none`. The style members of a time
/// series and of the axes are `dropped`, and metadata when they have the Grafana default.
fn bucket_custom(fcx: &FieldCx<'_>, ctx: &mut Ctx, name: &str, node: &Node, in_defaults: bool) {
    let key = format!("custom.{name}");
    if name == "gradientMode" && fcx.ptype == Pt::Histogram {
        if crate::item::is_trivial(node.v) || node.v == &serde_json::json!("none") {
            ctx.meta();
        } else {
            let reason = "the bars have no gradient (4.33d)";
            ctx.unsupported(&node.ptr, Some(node.v), reason, Fallback::MemberRemoved);
        }
    } else if let Some(why) = turns_default_off(fcx.ptype, name, node.v) {
        ctx.unsupported(&node.ptr, Some(node.v), why, prop_removed());
    } else if crate::item::is_trivial(node.v)
        || (in_defaults && (custom_default(fcx.ptype, &key, node) || series_default(name, node.v)))
    {
        ctx.meta();
    } else {
        ctx.dropped(&node.ptr, Some(node.v), "this style of a series has no equal in this panel (4.33c, 4.33d)");
    }
}

/// A `false` or a `0` of a `custom` member that turns off a feature that Grafana has on by default (15.5). The rule
/// of 15.5 does not make it metadata: the item gives the reason. The other values keep the rule of their row.
fn turns_default_off(ptype: Pt, name: &str, value: &Value) -> Option<&'static str> {
    // A `barchart` maps `axisGridShow`, `fillOpacity` and `lineWidth` (4.18, 4.30: wave H, item P03).
    let axes = matches!(ptype, Pt::Timeseries | Pt::Histogram | Pt::Xychart);
    match name {
        "axisGridShow" if axes && value == &Value::Bool(false) => {
            Some("false hides the grid lines of the axis: Hune draws them for each axis that shows (9.19)")
        }
        _ => None,
    }
}

/// Tells if a style member of a time series has the Grafana default (15.6, the panels of 4.33b).
fn series_default(name: &str, value: &Value) -> bool {
    use serde_json::json;
    let table: [(&str, Value); 6] = [
        ("drawStyle", json!("line")),
        ("lineInterpolation", json!("linear")),
        ("showPoints", json!("auto")),
        ("pointSize", json!(5)),
        ("stacking", json!({"mode": "none", "group": "A"})),
        ("barAlignment", json!(0)),
    ];
    table.iter().any(|(n, d)| *n == name && d == value)
}

/// The member `log` of `custom.scaleDistribution` (15.6). The bases 10 and 2 have an equal (`log_base`, wave H item P14): the base 2 of a log
/// scale goes to the options. A `log` of a linear scale is metadata. Another base is an unsupported value.
fn log_base<'a>(ctx: &mut Ctx, w: &mut Walk<'a>, customs: &mut Customs<'a>) {
    let Some(base) = w.take("log") else { return };
    let is_log = w.node.get("type").and_then(|t| t.str().map(|t| t == "log")).unwrap_or(false);
    if !is_log || base.v.as_f64() == Some(10.0) {
        ctx.meta();
    } else if base.v.as_f64() == Some(2.0) {
        customs.insert("scaleDistribution.log", base);
        ctx.ok();
    } else {
        let reason = "a log scale has the base 2 or 10 only (4.28)";
        ctx.unsupported(&base.ptr, Some(base.v), reason, Fallback::other("base 10"));
    }
}

/// The members `min` and `max` of `custom.pointSize` of an xychart (15.6). The format has one size
/// for all the points: only the Grafana default range 1 to 100 has no effect on the result.
fn point_size_range(ctx: &mut Ctx, w: &mut Walk<'_>) {
    for (key, default) in [("min", 1.0), ("max", 100.0)] {
        let Some(n) = w.take(key) else { continue };
        if n.v.as_f64() == Some(default) {
            ctx.meta();
        } else {
            let reason = "the size of a point does not follow the data: format 2 has one size (point_size)";
            ctx.unsupported(&n.ptr, Some(n.v), reason, Fallback::other("fixed size"));
        }
    }
}

/// Tells if a `custom` member has the Grafana default of 15.5. The pointer test fits both forms. The members
/// `fillOpacity` and `lineWidth` of a bar chart are options of 4.30 (wave H, item P03), so they do not come here.
fn custom_default(_ptype: Pt, key: &str, node: &Node) -> bool {
    crate::item::is_grafana_default(&format!("/fieldConfig/defaults/{}", key.replace('.', "/")), node.v)
}

fn cell_options<'a>(ctx: &mut Ctx, node: &Node<'a>, in_defaults: bool, customs: &mut Customs<'a>) -> Out {
    let auto = Out::obj([("type", Out::s("auto")), ("mode", Out::Null)]);
    let Some(mut w) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "cellOptions must be an object", Fallback::other("auto"));
        return auto;
    };
    let kind = w.take("type");
    let mode = w.take("mode");
    let value_mode = w.take("valueDisplayMode");
    let wrap = w.take("wrapText");
    w.finish(ctx);
    if let Some(wrap) = wrap {
        // Grafana 11 holds the wrap of the text here. Grafana 12 holds it in `custom.wrapText`.
        if in_defaults {
            customs.insert("wrapText", wrap);
        } else {
            let reason = "wrapText is a table option (4.27): it has no meaning in an override";
            ctx.unsupported(&wrap.ptr, Some(wrap.v), reason, prop_removed());
        }
    }
    let Some(kind) = kind else {
        ctx.ok();
        return auto;
    };
    match kind.str() {
        Some("auto") => {
            ctx.ok();
            auto
        }
        Some("color-text") => {
            ctx.ok();
            Out::obj([("type", Out::s("color_text")), ("mode", Out::Null)])
        }
        Some("color-background") => {
            let m = match mode.as_ref().and_then(Node::str) {
                Some(m @ ("gradient" | "basic")) => m,
                None => "gradient",
                Some(_) => {
                    let n = mode.as_ref().unwrap_or(&kind);
                    ctx.unsupported(
                        &n.ptr,
                        Some(n.v),
                        "the mode is not gradient or basic",
                        Fallback::other("gradient"),
                    );
                    "gradient"
                }
            };
            ctx.ok();
            Out::obj([("type", Out::s("color_background")), ("mode", Out::s(m))])
        }
        Some("gauge") => {
            let m = match mode.as_ref().and_then(Node::str) {
                Some(m @ ("gradient" | "basic" | "lcd")) => m,
                None => "gradient",
                Some(_) => {
                    let n = mode.as_ref().unwrap_or(&kind);
                    ctx.unsupported(
                        &n.ptr,
                        Some(n.v),
                        "the mode is not basic, gradient or lcd",
                        Fallback::other("gradient"),
                    );
                    "gradient"
                }
            };
            match value_mode.as_ref() {
                Some(v) if v.str() == Some("hidden") => ctx.unsupported(
                    &v.ptr,
                    Some(v.v),
                    "the gauge cell always shows its value (4.18)",
                    Fallback::MemberRemoved,
                ),
                Some(v) if matches!(v.str(), Some("color" | "text")) => ctx.ok(),
                Some(v) => ctx.unsupported(
                    &v.ptr,
                    Some(v.v),
                    "valueDisplayMode is not color, text or hidden",
                    Fallback::MemberRemoved,
                ),
                None => {}
            }
            ctx.ok();
            Out::obj([("type", Out::s("gauge")), ("mode", Out::s(m))])
        }
        Some("json-view") => {
            ctx.ok();
            Out::obj([("type", Out::s("json")), ("mode", Out::Null)])
        }
        _ => {
            ctx.unsupported(
                &kind.ptr,
                Some(kind.v),
                "this cell type is not in the property set (19.15)",
                Fallback::other("auto"),
            );
            auto
        }
    }
}

fn cell_display_mode(ctx: &mut Ctx, node: &Node) -> Out {
    let auto = Out::obj([("type", Out::s("auto")), ("mode", Out::Null)]);
    match node.str() {
        Some("auto") => {
            ctx.ok();
            auto
        }
        Some("color-text") => {
            ctx.ok();
            Out::obj([("type", Out::s("color_text")), ("mode", Out::Null)])
        }
        Some("color-background") => {
            ctx.ok();
            Out::obj([("type", Out::s("color_background")), ("mode", Out::s("gradient"))])
        }
        Some("color-background-solid") => {
            ctx.ok();
            Out::obj([("type", Out::s("color_background")), ("mode", Out::s("basic"))])
        }
        Some(gauge @ ("gradient-gauge" | "lcd-gauge" | "basic")) => {
            ctx.ok();
            let mode = match gauge {
                "gradient-gauge" => "gradient",
                "lcd-gauge" => "lcd",
                _ => "basic",
            };
            Out::obj([("type", Out::s("gauge")), ("mode", Out::s(mode))])
        }
        Some("json-view") => {
            ctx.ok();
            Out::obj([("type", Out::s("json")), ("mode", Out::Null)])
        }
        _ => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "this cell type is not in the property set (19.15)",
                Fallback::other("auto"),
            );
            auto
        }
    }
}

/// Maps `custom.footer` (an object with `reducers`) or `custom.footer.reducers` (the array).
fn footer_property(ctx: &mut Ctx, key: &str, node: &Node, props: &mut Props) {
    let list = if key == "custom.footer" {
        let Some(mut w) = Walk::new(node) else {
            ctx.unsupported(&node.ptr, Some(node.v), "footer must be an object", prop_removed());
            return;
        };
        let reducers = w.take("reducers");
        w.finish(ctx);
        match reducers {
            Some(r) => r,
            None => {
                ctx.meta();
                return;
            }
        }
    } else {
        node.clone()
    };
    if let Some(out) = footer_reducers(ctx, &list) {
        props.set("footer", out);
    }
}

/// Maps a list of Grafana reducers to the object `footer` of 4.18 (the row `custom.footer` of 15.6). Gives `None`
/// when no item maps. The old footer of a table (`options.footer.reducer`) uses it too.
pub(crate) fn footer_reducers(ctx: &mut Ctx, list: &Node) -> Option<Out> {
    if list.v.as_array().is_none() {
        ctx.unsupported(&list.ptr, Some(list.v), "reducers must be an array", prop_removed());
        return None;
    }
    let items = list.items();
    if items.is_empty() {
        ctx.meta();
        return None;
    }
    let mut out: Vec<&'static str> = Vec::new();
    for item in &items {
        let name = match item.str() {
            Some("countAll") => Some("count"),
            Some(id) => calc_name(id),
            None => None,
        };
        match name {
            None => ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "this reducer has no equal in the footer of a table (4.18)",
                Fallback::other("item removed"),
            ),
            Some(n) if out.contains(&n) => {
                ctx.changed(&item.ptr, Some(item.v), "two items give the same calculation: the second is removed");
            }
            Some(_) if out.len() >= MAX_FOOTER_REDUCERS => ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "more calculations than MAX_FOOTER_REDUCERS",
                Fallback::other("item removed"),
            ),
            Some(n) => {
                out.push(n);
                ctx.ok();
            }
        }
    }
    (!out.is_empty()).then(|| Out::obj([("reducers", Out::strings(&out))]))
}

/// Sets one property of `fields.defaults` in the canonical order of 4.18. A property that is set is replaced.
pub(crate) fn set_default_property(fields: &mut Out, name: &str, value: Out) {
    let Out::Obj(members) = fields else { return };
    let Some((_, Out::Obj(props))) = members.iter_mut().find(|(k, _)| k == "defaults") else { return };
    let order = |n: &str| ORDER.iter().position(|o| *o == n).unwrap_or(ORDER.len());
    props.retain(|(k, _)| k != name);
    let at = props.iter().position(|(k, _)| order(k) > order(name)).unwrap_or(props.len());
    props.insert(at, (name.to_owned(), value));
}

/// Adds one item at the end of `fields.overrides`. Gives `false` when the list has `MAX_OVERRIDES` items.
pub(crate) fn push_override(fields: &mut Out, item: Out) -> bool {
    let Out::Obj(members) = fields else { return false };
    let Some((_, Out::Arr(list))) = members.iter_mut().find(|(k, _)| k == "overrides") else { return false };
    if list.len() >= MAX_OVERRIDES {
        return false;
    }
    list.push(item);
    true
}

/// Maps `custom.hideFrom`. A chart panel keeps the three flags in `hide_from`. A table column is
/// hidden by `viz` only, so a table gets the property `hidden` (Grafana 12 hides a column with
/// `custom.hideFrom.viz`).
fn hide_from_property(fcx: &FieldCx<'_>, ctx: &mut Ctx, node: &Node, in_defaults: bool, props: &mut Props) {
    let Some(mut w) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "hideFrom must be an object", prop_removed());
        return;
    };
    let parts = [w.take("legend"), w.take("tooltip"), w.take("viz")];
    w.finish(ctx);
    let mut bits = [false; 3];
    for (bit, part) in bits.iter_mut().zip(&parts) {
        let Some(part) = part else { continue };
        match as_flag(part.v) {
            Some(b) => *bit = b,
            None => {
                ctx.unsupported(&part.ptr, Some(part.v), "the value must be true or false", prop_removed());
                return;
            }
        }
    }
    if in_defaults && bits == [false; 3] {
        // The default of Grafana: nothing is hidden.
        ctx.meta();
        return;
    }
    if fcx.ptype == Pt::Table {
        props.set("hidden", Out::Bool(bits[2]));
    } else {
        props.set(
            "hide_from",
            Out::obj([("legend", Out::Bool(bits[0])), ("tooltip", Out::Bool(bits[1])), ("viz", Out::Bool(bits[2]))]),
        );
    }
    ctx.ok();
}

fn thresholds(ctx: &mut Ctx, node: &Node) -> Option<Out> {
    let fail = |ctx: &mut Ctx, n: &Node, why: &str| {
        ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::DashboardNotImported);
        None
    };
    let Some(mut w) = Walk::new(node) else { return fail(ctx, node, "thresholds must be an object") };
    let mode = w.take("mode");
    let steps = w.take("steps");
    w.finish(ctx);
    let mode_text = match mode.as_ref().map(|m| (m, m.str())) {
        None => "absolute",
        Some((_, Some(m @ ("absolute" | "percentage")))) => m,
        Some((m, _)) => return fail(ctx, m, "the mode is not absolute or percentage"),
    };
    let Some(steps) = steps.filter(|s| s.v.is_array()) else {
        return fail(ctx, node, "thresholds needs an array steps");
    };
    let items = steps.items();
    if items.is_empty() || items.len() > MAX_THRESHOLD_STEPS {
        return fail(ctx, &steps, "thresholds needs 1 to 32 steps");
    }
    let mut out = Vec::new();
    let mut last: Option<f64> = None;
    for (i, item) in items.iter().enumerate() {
        let Some(mut sw) = Walk::new(item) else { return fail(ctx, item, "a step must be an object") };
        let value = sw.take("value");
        let color = sw.take("color");
        sw.finish(ctx);
        let value_out = match value.as_ref().map(|v| (v, v.v)) {
            None | Some((_, Value::Null)) => {
                if i != 0 {
                    return fail(ctx, item, "only the first step has the value null");
                }
                Out::Null
            }
            Some((v, num)) => match as_num(num) {
                // Grafana reads the first step as the base step, whatever its value. A v2 export
                // writes the base as 0. The step is the base (4.21): its value becomes null.
                Some(_) if i == 0 => Out::Null,
                Some(n) if last.is_none_or(|l| n > l) => {
                    last = Some(n);
                    Out::Num(n)
                }
                _ => {
                    return fail(
                        ctx,
                        v,
                        "the first step needs null and each other step a number above the step before",
                    );
                }
            },
        };
        let Some(color) = color else { return fail(ctx, item, "a step needs a colour") };
        let c = colour(ctx, &color, Fallback::DashboardNotImported)?;
        out.push(Out::obj([("value", value_out), ("color", Out::Str(c))]));
    }
    ctx.ok();
    Some(Out::obj([("mode", Out::s(mode_text)), ("steps", Out::Arr(out))]))
}

/// The mode of 4.19 for a Grafana colour mode that has no colour member and loses nothing. `fixed`, `thresholds` and the
/// palette modes that give a `changed` item are not here. `palette-classic` gives `palette`, the one series palette (9.16).
fn colour_mode(id: &str) -> Option<&'static str> {
    Some(match id {
        "palette-classic" => "palette",
        "continuous-GrYlRd" => "continuous_gr_yl_rd",
        "continuous-RdYlGr" => "continuous_rd_yl_gr",
        "continuous-BlYlRd" => "continuous_bl_yl_rd",
        "continuous-YlRd" => "continuous_yl_rd",
        "continuous-BlPu" => "continuous_bl_pu",
        "continuous-YlBl" => "continuous_yl_bl",
        "continuous-blues" => "continuous_blues",
        "continuous-reds" => "continuous_reds",
        "continuous-greens" => "continuous_greens",
        "continuous-purples" => "continuous_purples",
        _ => return None,
    })
}

fn colour_property(ctx: &mut Ctx, node: &Node) -> Option<Out> {
    let Some(mut w) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "color must be an object", prop_removed());
        return None;
    };
    let mode = w.take("mode");
    let fixed = w.take("fixedColor");
    if let Some(series) = w.take("seriesBy") {
        if series.str() == Some("last") || series.is_null() {
            ctx.meta();
        } else {
            ctx.dropped(&series.ptr, Some(series.v), "seriesBy has no meaning in Hune");
        }
    }
    w.finish(ctx);
    let mode_node = mode?;
    match mode_node.str() {
        Some("fixed") => {
            let Some(f) = fixed.filter(|f| !f.is_null() && f.str() != Some("")) else {
                ctx.unsupported(&mode_node.ptr, Some(mode_node.v), "the mode fixed needs fixedColor", prop_removed());
                return None;
            };
            let c = colour(ctx, &f, prop_removed())?;
            Some(Out::obj([("mode", Out::s("fixed")), ("fixed", Out::Str(c))]))
        }
        Some("thresholds") => {
            ctx.ok();
            Some(Out::obj([("mode", Out::s("thresholds")), ("fixed", Out::Null)]))
        }
        // The other Grafana palette modes become the one series palette of Hune (4.19, 9.16): a listed `changed` item, the series keep a colour.
        Some(id @ ("palette-classic-by-name" | "palette-saturated")) => {
            ctx.changed(
                &mode_node.ptr,
                Some(mode_node.v),
                &format!("the Grafana palette mode {id} becomes the series palette of Hune (mode palette)"),
            );
            Some(Out::obj([("mode", Out::s("palette")), ("fixed", Out::Null)]))
        }
        Some(id) => match colour_mode(id) {
            Some(mode) => {
                ctx.ok();
                Some(Out::obj([("mode", Out::s(mode)), ("fixed", Out::Null)]))
            }
            None => {
                let why = "this colour mode has no equal in 4.19";
                ctx.unsupported(&mode_node.ptr, Some(mode_node.v), why, prop_removed());
                None
            }
        },
        None => {
            ctx.unsupported(&mode_node.ptr, Some(mode_node.v), "the colour mode must be a text", prop_removed());
            None
        }
    }
}

struct RawMapping {
    index: i64,
    seq: usize,
    out: Out,
}

fn mapping_result(
    ctx: &mut Ctx,
    kind_ptr: &str,
    result: Option<Node>,
) -> Option<(Option<String>, Option<String>, i64)> {
    // The result holds text, color and index. A node that is absent gives no text and no colour.
    let Some(r) = result else {
        let _ = kind_ptr;
        return Some((None, None, i64::MAX));
    };
    let mut w = Walk::new(&r)?;
    let text = w.take("text");
    let color = w.take("color");
    let index = w.take("index");
    w.finish(ctx);
    let text = text.and_then(|t| t.str().filter(|s| !s.is_empty()).map(str::to_owned));
    let color = match color.filter(|c| !c.is_null() && c.str() != Some("")) {
        Some(c) => colour(ctx, &c, Fallback::other("mapping colour removed")),
        None => None,
    };
    let index = index.and_then(|i| as_int(i.v)).unwrap_or(i64::MAX);
    Some((text, color, index))
}

/// Maps the mappings of a property by the split and order rule of 15.6.
fn mappings(ctx: &mut Ctx, node: &Node) -> Option<Out> {
    let Some(arr) = node.v.as_array() else {
        ctx.unsupported(&node.ptr, Some(node.v), "mappings must be an array", Fallback::DashboardNotImported);
        return None;
    };
    let mut raw: Vec<RawMapping> = Vec::new();
    let mut seq = 0usize;
    for item in node.items() {
        let Some(mut w) = Walk::new(&item) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a mapping must be an object", Fallback::DashboardNotImported);
            return None;
        };
        let kind = w.take("type");
        let options = w.take("options");
        w.finish(ctx);
        let (Some(kind), Some(options)) = (kind, options) else {
            ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "a mapping needs type and options",
                Fallback::DashboardNotImported,
            );
            return None;
        };
        let Some(kind_text) = kind.str() else {
            ctx.unsupported(&kind.ptr, Some(kind.v), "the mapping type must be a text", Fallback::DashboardNotImported);
            return None;
        };
        match kind_text {
            "value" => {
                // The keys are sorted: the input order of the keys is not kept (15.6, rule 4).
                let Some(map) = options.v.as_object() else {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "options must be an object",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                };
                for key in map.keys() {
                    let entry = options.get(key)?;
                    let (text, color, index) = mapping_result(ctx, &entry.ptr, Some(entry.clone()))?;
                    push_mapping(ctx, &mut raw, &mut seq, &entry, text, color, index, |t, c| {
                        Out::obj([("kind", Out::s("value")), ("value", Out::s(key)), ("text", t), ("color", c)])
                    });
                }
            }
            "range" => {
                let Some(mut ow) = Walk::new(&options) else {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "options must be an object",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                };
                let from = ow.take("from");
                let to = ow.take("to");
                let result = ow.take("result");
                ow.finish(ctx);
                let bound = |ctx: &mut Ctx, n: Option<Node>| -> Option<Out> {
                    match n {
                        None => Some(Out::Null),
                        Some(n) if n.is_null() => Some(Out::Null),
                        Some(n) => match as_num(n.v) {
                            Some(v) => Some(Out::Num(v)),
                            None => {
                                ctx.unsupported(
                                    &n.ptr,
                                    Some(n.v),
                                    "a bound must be a number or null",
                                    Fallback::DashboardNotImported,
                                );
                                None
                            }
                        },
                    }
                };
                let (from_out, to_out) = (bound(ctx, from.clone())?, bound(ctx, to.clone())?);
                if let (Out::Num(f), Out::Num(t)) = (&from_out, &to_out)
                    && f > t
                {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "from must be at most to",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                }
                if matches!((&from_out, &to_out), (Out::Null, Out::Null)) {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "a range needs a bound",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                }
                let (text, color, index) = mapping_result(ctx, &options.ptr, result.clone())?;
                push_mapping(ctx, &mut raw, &mut seq, &options, text, color, index, |t, c| {
                    Out::obj([
                        ("kind", Out::s("range")),
                        ("from", from_out.clone()),
                        ("to", to_out.clone()),
                        ("text", t),
                        ("color", c),
                    ])
                });
            }
            "regex" => {
                let Some(mut ow) = Walk::new(&options) else {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "options must be an object",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                };
                let pattern = ow.take("pattern");
                let result = ow.take("result");
                ow.finish(ctx);
                let Some(p) = pattern.as_ref().and_then(Node::str) else {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "a regex mapping needs a pattern",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                };
                if let Err(e) = check_pattern(p, PatternForm::Grafana) {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        &format!("the pattern is refused by 4.24: {e:?}"),
                        Fallback::DashboardNotImported,
                    );
                    return None;
                }
                let (text, color, index) = mapping_result(ctx, &options.ptr, result.clone())?;
                let p = p.to_owned();
                push_mapping(ctx, &mut raw, &mut seq, &options, text, color, index, |t, c| {
                    Out::obj([("kind", Out::s("regex")), ("pattern", Out::s(&p)), ("text", t), ("color", c)])
                });
            }
            "special" => {
                let Some(mut ow) = Walk::new(&options) else {
                    ctx.unsupported(
                        &options.ptr,
                        Some(options.v),
                        "options must be an object",
                        Fallback::DashboardNotImported,
                    );
                    return None;
                };
                let m = ow.take("match");
                let result = ow.take("result");
                ow.finish(ctx);
                let name = match m.as_ref().and_then(Node::str) {
                    Some("null") => "null",
                    Some("nan") => "nan",
                    Some("null+nan") => "null_nan",
                    Some("true") => "true",
                    Some("false") => "false",
                    Some("empty") => "empty",
                    _ => {
                        ctx.unsupported(
                            &options.ptr,
                            Some(options.v),
                            "this special match has no equal in format 1",
                            Fallback::DashboardNotImported,
                        );
                        return None;
                    }
                };
                let (text, color, index) = mapping_result(ctx, &options.ptr, result.clone())?;
                push_mapping(ctx, &mut raw, &mut seq, &options, text, color, index, |t, c| {
                    Out::obj([("kind", Out::s("special")), ("match", Out::s(name)), ("text", t), ("color", c)])
                });
            }
            _ => {
                ctx.unsupported(
                    &kind.ptr,
                    Some(kind.v),
                    "this mapping type has no equal in format 1",
                    Fallback::DashboardNotImported,
                );
                return None;
            }
        }
    }
    let _ = arr;
    raw.sort_by_key(|m| (m.index, m.seq));
    let mut list: Vec<Out> = raw.into_iter().map(|m| m.out).collect();
    if list.len() > MAX_MAPPINGS {
        ctx.unsupported(
            &node.ptr,
            None,
            "more mappings than MAX_MAPPINGS",
            Fallback::other("the first 256 mappings stay"),
        );
        list.truncate(MAX_MAPPINGS);
    }
    if list.is_empty() {
        ctx.ok();
        return None;
    }
    ctx.ok();
    Some(Out::Arr(list))
}

#[allow(clippy::too_many_arguments)]
fn push_mapping(
    ctx: &mut Ctx,
    raw: &mut Vec<RawMapping>,
    seq: &mut usize,
    at: &Node,
    text: Option<String>,
    color: Option<String>,
    index: i64,
    make: impl FnOnce(Out, Out) -> Out,
) {
    if text.is_none() && color.is_none() {
        ctx.dropped(&at.ptr, Some(at.v), "the mapping has no text and no colour");
        return;
    }
    if text.as_ref().is_some_and(|t| chars(t) > 256) {
        ctx.unsupported(&at.ptr, Some(at.v), "the text has more than 256 characters", Fallback::DashboardNotImported);
        return;
    }
    let out = make(Out::opt_s(text.as_deref()), Out::opt_s(color.as_deref()));
    raw.push(RawMapping { index, seq: *seq, out });
    *seq += 1;
}

/// Checks the tokens `${...}` of a link URL against the list of 9.18. Gives the reason of a fault.
fn url_tokens_fault(url: &str) -> Option<String> {
    let mut rest = url;
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else {
            return Some("a ${ has no closing }".to_owned());
        };
        let inner = &after[..end];
        let known = matches!(
            inner,
            "__value.raw"
                | "__value.text"
                | "__value.time"
                | "__value.numeric"
                | "__field.name"
                | "__series.name"
                | "__from"
                | "__to"
                | "__timezone"
                | "__dashboard.uid"
                | "__url_time_range"
                | "__all_variables"
        );
        let data = inner.strip_prefix("__data.fields.").is_some_and(|n| !n.is_empty());
        // The form `${__data.fields["name"]}` can hold a `}` in the name: read to the `]}`.
        if let Some(named) = after.strip_prefix("__data.fields[\"") {
            match named.find("\"]}") {
                Some(close) => {
                    rest = &named[close + 3..];
                    continue;
                }
                None => return Some("the token __data.fields[\"...\"] is not closed".to_owned()),
            }
        }
        // A variable, as `${name}` or with the format `queryparam` of 9.18.
        let name = inner.strip_suffix(":queryparam").unwrap_or(inner);
        let variable = !name.starts_with("__")
            && !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !name.as_bytes()[0].is_ascii_digit();
        if !(known || data || variable) {
            return Some(format!("the token ${{{inner}}} is not in the list of 9.18"));
        }
        rest = &after[end + 1..];
    }
    None
}

/// Finds the Grafana dashboard that a link names: `.../d/<uid>` with a uid of the run.
fn grafana_dashboard_in(url: &str, opts: &ImportOptions) -> Option<String> {
    let prefix = url.split("${").next().unwrap_or("");
    let at = prefix.find("/d/")?;
    let tail = &prefix[at + 3..];
    let uid: String = tail.chars().take_while(|c| !matches!(c, '/' | '?' | '#')).collect();
    opts.uid_map.contains_key(&uid).then_some(uid)
}

/// Rewrites a link that starts with `<old>d/<grafana uid>` (15.17).
pub(crate) fn rewrite_link(ctx: &mut Ctx, opts: &ImportOptions, node: &Node, url: &str) -> String {
    if let Some(p) = &opts.link_prefix {
        for (grafana, hune) in &opts.uid_map {
            let old = format!("{}d/{grafana}", p.old);
            if let Some(rest) = url.strip_prefix(&old) {
                let boundary = rest.is_empty() || rest.starts_with(['/', '?', '#', '$']);
                if boundary {
                    return format!("{}d/{hune}{rest}", p.new);
                }
            }
        }
    } else if let Some(uid) = grafana_dashboard_in(url, opts) {
        ctx.changed(
            &node.ptr,
            Some(node.v),
            &format!("the link names the Grafana dashboard {uid}: give --link-prefix to rewrite it"),
        );
    }
    url.to_owned()
}

/// Maps a list of links (panel links or data links). A link that format 1 refuses is removed.
pub(crate) fn links(ctx: &mut Ctx, opts: &ImportOptions, node: &Node) -> Vec<Out> {
    let mut out = Vec::new();
    if node.v.as_array().is_none() {
        ctx.unsupported(&node.ptr, Some(node.v), "links must be an array", Fallback::LinkRemoved);
        return out;
    }
    for item in node.items() {
        let Some(mut w) = Walk::new(&item) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a link must be an object", Fallback::LinkRemoved);
            continue;
        };
        let title = w.take("title");
        let url = w.take("url");
        let blank = w.take("targetBlank");
        w.finish(ctx);
        let (Some(title), Some(url)) = (title.and_then(|t| t.str().map(str::to_owned)), url) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a link needs a title and a url", Fallback::LinkRemoved);
            continue;
        };
        let Some(url_text) = url.str() else {
            ctx.unsupported(&url.ptr, Some(url.v), "the url must be a text", Fallback::LinkRemoved);
            continue;
        };
        // The link editor of Grafana writes U+FEFF around a token, and Grafana 12.4.2 builds the link without it (15.6).
        let url_text = rewrite_link(ctx, opts, &url, &url_text.replace('\u{feff}', ""));
        let fault = if !(1..=256).contains(&chars(&title)) {
            Some("the title must have 1 to 256 characters".to_owned())
        } else if url_text.is_empty() || url_text.len() > MAX_URL_BYTES {
            Some("the url must have 1 to 2048 bytes".to_owned())
        } else if !is_valid_url_prefix(&url_text) {
            Some("the prefix of the url breaks the rule of 4.23".to_owned())
        } else {
            url_tokens_fault(&url_text)
        };
        if let Some(why) = fault {
            ctx.unsupported(&url.ptr, Some(url.v), &why, Fallback::LinkRemoved);
            continue;
        }
        let target_blank = blank.as_ref().and_then(|b| as_flag(b.v)).unwrap_or(false);
        out.push(Out::obj([
            ("title", Out::Str(title)),
            ("url", Out::Str(url_text)),
            ("target_blank", Out::Bool(target_blank)),
        ]));
        ctx.ok();
    }
    if out.len() > MAX_LINKS {
        ctx.unsupported(&node.ptr, None, "more links than MAX_LINKS", Fallback::other("the first 16 links stay"));
        out.truncate(MAX_LINKS);
    }
    out
}

/// Maps the options of a matcher `byNames`: the names and the mode. Gives the matcher of 4.17.
fn names_matcher(ctx: &mut Ctx, options: Option<&Node>, matcher: &Node) -> Option<Out> {
    let removed = || Fallback::other("override removed");
    let at = options.unwrap_or(matcher);
    let Some(mut w) = options.and_then(Walk::new) else {
        ctx.unsupported(&at.ptr, Some(at.v), "the options of byNames must be an object", removed());
        return None;
    };
    let names = w.take("names");
    let mode = w.take("mode");
    // The text "All except:" and the flag readOnly serve the editor of Grafana.
    w.meta(ctx, &["prefix", "readOnly"]);
    w.finish(ctx);
    let mode_text = match mode.as_ref().map(|m| (m, m.str())) {
        None => "include",
        Some((_, Some(m @ ("include" | "exclude")))) => m,
        Some((m, _)) => {
            ctx.unsupported(&m.ptr, Some(m.v), "the mode is not include or exclude", removed());
            return None;
        }
    };
    let Some(names) = names.filter(|n| n.v.is_array()) else {
        ctx.unsupported(&at.ptr, Some(at.v), "byNames needs an array names", removed());
        return None;
    };
    let items = names.items();
    let mut list: Vec<&str> = Vec::new();
    for item in &items {
        let Some(name) = item.str() else {
            ctx.unsupported(&item.ptr, Some(item.v), "a name must be a text", removed());
            return None;
        };
        if name.is_empty() || name.len() > MAX_VALUE_BYTES || name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}') {
            let why = "a name must have 1 to 1024 bytes and no control character";
            ctx.unsupported(&item.ptr, Some(item.v), why, removed());
            return None;
        }
        if !list.contains(&name) {
            list.push(name);
        }
    }
    if list.is_empty() || list.len() > MAX_MATCHER_NAMES {
        ctx.unsupported(&names.ptr, Some(names.v), "byNames needs 1 to 64 different names", removed());
        return None;
    }
    ctx.ok();
    Some(Out::obj([("by", Out::s("names")), ("names", Out::strings(&list)), ("mode", Out::s(mode_text))]))
}

fn override_out(fcx: &FieldCx<'_>, ctx: &mut Ctx, node: &Node) -> Option<Out> {
    let mut w = Walk::new(node)?;
    let matcher = w.take("matcher");
    let properties = w.take("properties");
    // Grafana writes this member when a click on the legend hides a series (the matcher is `byNames`).
    if node.get("__systemRef").is_some_and(|r| r.str() == Some("hideSeriesFrom")) {
        w.meta(ctx, &["__systemRef"]);
    }
    w.finish(ctx);
    let removed = Fallback::other("override removed");
    let matcher = matcher?;
    let Some(mut mw) = Walk::new(&matcher) else {
        ctx.unsupported(&matcher.ptr, Some(matcher.v), "the matcher must be an object", removed);
        return None;
    };
    let id = mw.take("id");
    let options = mw.take("options");
    let scope = mw.take("scope");
    mw.finish(ctx);
    if let Some(scope) = scope {
        // Grafana 13 writes `series` on the matcher of a column width. A frame of Hune has no nested frame.
        if scope.str() == Some("series") {
            ctx.ok();
        } else {
            let why = "this matcher scope has no equal: a frame of Hune has no nested frame";
            ctx.unsupported(&scope.ptr, Some(scope.v), why, removed);
            return None;
        }
    }
    let by = match id.as_ref().and_then(Node::str) {
        Some("byName") => "name",
        Some("byRegexp") => "regex",
        Some("byNames") => "names",
        Some("byFrameRefID") => "query",
        _ => {
            let at = id.as_ref().unwrap_or(&matcher);
            ctx.unsupported(&at.ptr, Some(at.v), "this matcher has no equal in format 2", removed);
            return None;
        }
    };
    let match_out = if by == "names" {
        names_matcher(ctx, options.as_ref(), &matcher)?
    } else {
        let value = options.as_ref().and_then(Node::str).unwrap_or("");
        let at = options.as_ref().unwrap_or(&matcher);
        if value.is_empty() || value.len() > MAX_VALUE_BYTES {
            ctx.unsupported(&at.ptr, Some(at.v), "the matcher value must have 1 to 1024 bytes", removed);
            return None;
        }
        if by == "regex"
            && let Err(e) = check_pattern(value, PatternForm::Grafana)
        {
            ctx.unsupported(&at.ptr, Some(at.v), &format!("the pattern is refused by 4.24: {e:?}"), removed);
            return None;
        }
        let mut value = value.to_owned();
        if by == "query" {
            // A refId that the importer changed is changed in the matcher too (15.6). Else the value is a ref (4.3).
            if let Some(mapped) = fcx.refs.get(&value).filter(|m| **m != value) {
                let why = format!("the refId of a query was changed: the matcher names the ref {mapped}");
                ctx.changed(&at.ptr, Some(at.v), &why);
                value = mapped.clone();
            } else if !hune_core::document::is_valid_query_ref(&value) {
                let why = "the refId of a matcher byFrameRefID must match ^[A-Za-z][A-Za-z0-9_]{0,15}$ (4.3)";
                ctx.unsupported(&at.ptr, Some(at.v), why, removed);
                return None;
            }
        }
        ctx.ok();
        Out::obj([("by", Out::s(by)), ("value", Out::s(&value))])
    };
    let mut props = Props::default();
    let mut customs = Customs::new();
    if let Some(list) = properties {
        for p in list.items() {
            let Some(mut pw) = Walk::new(&p) else {
                ctx.unsupported(&p.ptr, Some(p.v), "a property must be an object", Fallback::PropertyRemoved);
                continue;
            };
            let pid = pw.take("id");
            let pvalue = pw.take("value");
            pw.finish(ctx);
            let (Some(pid), Some(pvalue)) = (pid.as_ref().and_then(|i| i.str().map(str::to_owned)), pvalue) else {
                ctx.unsupported(&p.ptr, Some(p.v), "a property needs an id and a value", Fallback::PropertyRemoved);
                continue;
            };
            property(fcx, ctx, &pid, &pvalue, false, &mut props, &mut customs);
        }
    }
    props.check_range(ctx);
    props.check_soft(ctx);
    if props.is_empty() {
        return None;
    }
    Some(Out::obj([("match", match_out), ("set", props.into_out())]))
}

/// Maps the field configuration of a panel. Gives the member `fields` and the custom values
/// that the options of the panel read.
pub(crate) fn field_config<'a>(fcx: &FieldCx<'_>, ctx: &mut Ctx, node: Option<Node<'a>>) -> (Out, Customs<'a>) {
    let mut customs = Customs::new();
    let mut defaults = Props::default();
    let mut overrides: Vec<Out> = Vec::new();
    if let Some(node) = node.filter(|n| !n.is_null()) {
        match Walk::new(&node) {
            None => ctx.unsupported(&node.ptr, Some(node.v), "fieldConfig must be an object", Fallback::MemberRemoved),
            Some(mut w) => {
                let d = w.take("defaults");
                let o = w.take("overrides");
                w.finish(ctx);
                if let Some(d) = d.filter(|d| !d.is_null()) {
                    let keys: Vec<String> = d.v.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
                    for key in keys {
                        let Some(member) = d.get(&key) else { continue };
                        if key == "custom" {
                            let ckeys: Vec<String> =
                                member.v.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
                            for ck in ckeys {
                                if let Some(cn) = member.get(&ck) {
                                    property(fcx, ctx, &format!("custom.{ck}"), &cn, true, &mut defaults, &mut customs);
                                }
                            }
                        } else {
                            property(fcx, ctx, &key, &member, true, &mut defaults, &mut customs);
                        }
                    }
                    defaults.check_range(ctx);
                    defaults.check_soft(ctx);
                }
                if let Some(o) = o.filter(|o| !o.is_null()) {
                    if o.v.as_array().is_none() {
                        ctx.unsupported(&o.ptr, Some(o.v), "overrides must be an array", Fallback::MemberRemoved);
                    } else {
                        for item in o.items() {
                            if let Some(out) = override_out(fcx, ctx, &item) {
                                overrides.push(out);
                            }
                        }
                        if overrides.len() > MAX_OVERRIDES {
                            ctx.unsupported(
                                &o.ptr,
                                None,
                                "more overrides than MAX_OVERRIDES",
                                Fallback::DashboardNotImported,
                            );
                        }
                    }
                }
            }
        }
    }
    let _ = Class::Mapped;
    (Out::obj([("defaults", defaults.into_out()), ("overrides", Out::Arr(overrides))]), customs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_tokens() {
        assert!(url_tokens_fault("https://x/${__value.raw}?a=${var1}&t=${__url_time_range}").is_none());
        assert!(url_tokens_fault("/d/${__data.fields[\"a b\"]}").is_none());
        // The tokens of Grafana 12 (P02).
        for ok in [
            "/x/${__series.name}",
            "/x?t=${__value.time}&n=${__value.numeric}&z=${__timezone}&u=${__dashboard.uid}",
            "/x?${serviceName:queryparam}&${trace_id:queryparam}",
        ] {
            assert!(url_tokens_fault(ok).is_none(), "{ok}");
        }
        for bad in [
            "/x/${v:percentencode}",
            "/x/${v:raw}",
            "/x/${__from:queryparam}",
            "/x/${:queryparam}",
            "/x/${1v:queryparam}",
        ] {
            assert!(url_tokens_fault(bad).is_some(), "{bad}");
        }
        assert!(url_tokens_fault("/x/${open").is_some());
    }
}
