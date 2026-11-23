// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options of the type `geomap` (15.6, 4.33g).
//!
//! The document holds no tile source: the importer drops the base layer of Grafana and the URL of an `xyz` layer
//! with a `changed` item. The tile source of Hune is the block `geomap` of the configuration (13.12c). The first layer
//! of the type `markers` gives the marker layer. Each other layer is `unsupported`.

use super::{Ctx, Fallback, Node, OptWalk, Out, as_flag, as_int, as_num, bool_opt, default_fallback, text_or_null};
use crate::colour::colour;

/// The view presets of Grafana 12.4.2 (the list of the view editor): the id, the latitude, the longitude and the zoom.
const PRESETS: [(&str, f64, f64, i64); 11] = [
    ("north-america", 40.0, -100.0, 4),
    ("south-america", -20.0, -60.0, 3),
    ("europe", 46.0, 14.0, 4),
    ("africa", 0.0, 30.0, 3),
    ("west-asia", 26.0, 53.0, 4),
    ("s-asia", 19.5, 80.0, 4),
    ("se-asia", 10.0, 106.0, 4),
    ("e-asia", 33.0, 120.0, 4),
    ("australia", -25.0, 135.0, 4),
    ("oceania", -10.0, -140.0, 3),
    ("zero", 0.0, 0.0, 1),
];

/// The marker of a circle in Grafana: the only symbol that Hune draws.
const CIRCLE: &str = "img/icons/marker/circle.svg";

/// The reason of the `changed` item of a tile source of Grafana.
const TILE_SOURCE: &str = "the document holds no tile source: the tile source of Hune is the block geomap of the configuration (13.12c); without it the map has a plain background";

/// Marks members as metadata when they are present.
fn meta(ctx: &mut Ctx, w: &mut OptWalk, keys: &[&str]) {
    for key in keys {
        if w.take(key).is_some() {
            ctx.meta();
        }
    }
}

/// A member that Hune cannot give: `unsupported` with the fallback "member removed", unless its value is
/// `null`, `false`, `0` or the empty text.
fn removed(ctx: &mut Ctx, node: Option<Node>, reason: &str) {
    let Some(n) = node else { return };
    let off = n.v.is_null()
        || n.v.as_bool() == Some(false)
        || n.v.as_f64() == Some(0.0)
        || n.v.as_str().is_some_and(str::is_empty);
    if off {
        ctx.meta();
    } else {
        ctx.unsupported(&n.ptr, Some(n.v), reason, Fallback::MemberRemoved);
    }
}

/// Reads a number in `min..=max`, or the default with an `unsupported` item.
fn number_in(ctx: &mut Ctx, node: Option<&Node>, min: f64, max: f64, default: f64) -> f64 {
    let Some(n) = node else { return default };
    match as_num(n.v).filter(|v| (min..=max).contains(v)) {
        Some(v) => {
            ctx.ok();
            v
        }
        None => {
            ctx.unsupported(
                &n.ptr,
                Some(n.v),
                &format!("the value must be a number from {min} to {max}"),
                default_fallback(),
            );
            default
        }
    }
}

/// Reads a whole number in `min..=max`. A number with a fraction is rounded, with a `changed` item.
fn whole_in(ctx: &mut Ctx, node: Option<&Node>, min: i64, max: i64, default: i64) -> i64 {
    let Some(n) = node else { return default };
    let exact = as_int(n.v);
    let rounded = exact.or_else(|| as_num(n.v).filter(|f| f.abs() < 1.0e9).map(|f| f.round() as i64));
    match rounded.filter(|v| (min..=max).contains(v)) {
        Some(v) if exact.is_some() => {
            ctx.ok();
            v
        }
        Some(v) => {
            ctx.changed(&n.ptr, Some(n.v), &format!("the value becomes the whole number {v}"));
            v
        }
        None => {
            let reason = format!("the value must be a number from {min} to {max}");
            ctx.unsupported(&n.ptr, Some(n.v), &reason, default_fallback());
            default
        }
    }
}

/// Maps `options.view` (15.6): the centre and the zoom of the start view.
fn view(ctx: &mut Ctx, node: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, node, "view");
    let id = w.take("id");
    let lat = w.take("lat");
    let lon = w.take("lon");
    let zoom = w.take("zoom");
    let padding = w.take("padding");
    let max_zoom = w.take("maxZoom");
    meta(ctx, &mut w, &["allLayers", "lastOnly", "layer", "noRepeat"]);
    removed(ctx, w.take("shared"), "the view is not shared between panels");
    removed(ctx, w.take("minZoom"), "the start view has no lowest zoom");
    w.finish(ctx);
    // An absent id is the view "zero" of Grafana: the centre (0, 0).
    let (mode, centre, default_zoom) = match id.as_ref() {
        None => ("fixed", Some((0.0, 0.0)), 1),
        Some(n) => match n.str() {
            Some("fit") => {
                ctx.ok();
                ("fit", None, 15)
            }
            Some("coords") => {
                ctx.ok();
                ("fixed", None, 1)
            }
            Some(t) if PRESETS.iter().any(|p| p.0 == t) => {
                ctx.ok();
                let (_, la, lo, z) = PRESETS.iter().find(|p| p.0 == t).copied().unwrap_or(PRESETS[10]);
                ("fixed", Some((la, lo)), z)
            }
            _ => {
                let fallback = Fallback::other("the view (0, 0) at zoom 1");
                ctx.unsupported(&n.ptr, Some(n.v), "no view of Grafana 12.4.2 has this id", fallback);
                ("fixed", Some((0.0, 0.0)), 1)
            }
        },
    };
    let (la, lo) = match centre {
        Some(c) => {
            // A preset has its own centre: the members `lat` and `lon` have no effect in Grafana.
            for _ in [&lat, &lon].into_iter().flatten() {
                ctx.meta();
            }
            c
        }
        None => (number_in(ctx, lat.as_ref(), -90.0, 90.0, 0.0), number_in(ctx, lon.as_ref(), -180.0, 180.0, 0.0)),
    };
    // With `fit`, Grafana takes `zoom`, else `maxZoom`, as the highest zoom. Another view limits the zoom with
    // `maxZoom`: Hune has no such limit for one panel.
    let zoom_node = if mode == "fit" && zoom.is_none() { max_zoom.as_ref() } else { zoom.as_ref() };
    if mode != "fit" {
        removed(
            ctx,
            max_zoom.clone(),
            "the start view has no highest zoom of its own (the block geomap of the configuration gives it, 13.12c)",
        );
    } else if zoom.is_some() && max_zoom.is_some() {
        ctx.meta();
    }
    let z = whole_in(ctx, zoom_node, 0, 22, default_zoom);
    if mode == "fit"
        && let Some(p) = padding.as_ref()
    {
        if as_num(p.v) == Some(5.0) {
            ctx.ok();
        } else {
            ctx.changed(&p.ptr, Some(p.v), "the padding of the view of all the markers is 5 percent");
        }
    } else if padding.is_some() {
        ctx.meta();
    }
    Out::obj([("mode", Out::s(mode)), ("lat", Out::Num(la)), ("lon", Out::Num(lo)), ("zoom", Out::Int(z))])
}

/// Maps `options.controls` (15.6) to the two switches. The other controls have no equal.
fn controls(ctx: &mut Ctx, node: Option<Node>) -> (Out, Out) {
    let mut w = OptWalk::new(ctx, node, "controls");
    let show_zoom = w.take("showZoom");
    let wheel = w.take("mouseWheelZoom");
    if let Some(n) = w.take("showAttribution") {
        if as_flag(n.v) == Some(false) {
            ctx.changed(&n.ptr, Some(n.v), "the attribution of the tile source always shows (9.19i)");
        } else {
            ctx.ok();
        }
    }
    removed(ctx, w.take("showScale"), "Hune draws no scale on the map");
    removed(ctx, w.take("showMeasure"), "Hune has no measure tools on the map");
    removed(ctx, w.take("showDebug"), "Hune has no debug view of the map");
    meta(ctx, &mut w, &["scaleUnits"]);
    w.finish(ctx);
    (bool_opt(ctx, show_zoom.as_ref(), true), bool_opt(ctx, wheel.as_ref(), true))
}

/// The default marker layer (4.33g, 9.19i): the size and the colour of Grafana 12.4.2.
fn default_layer() -> Out {
    Out::obj([
        ("lat_field", Out::Null),
        ("lon_field", Out::Null),
        ("size", Out::Int(5)),
        ("size_min", Out::Int(2)),
        ("size_max", Out::Int(15)),
        ("size_field", Out::Null),
        ("color", Out::s("dark-green")),
        ("color_field", Out::Null),
        ("opacity", Out::Int(40)),
    ])
}

/// Maps `location` of a marker layer to the two names of fields.
fn location(ctx: &mut Ctx, node: Option<Node>) -> (Out, Out) {
    let mut w = OptWalk::new(ctx, node, "location");
    let mode = w.take("mode");
    let lat = w.take("latitude");
    let lon = w.take("longitude");
    meta(ctx, &mut w, &["geohash", "lookup", "gazetteer", "wkt", "h3"]);
    w.finish(ctx);
    let names = [&lat, &lon].into_iter().flatten().count();
    match mode.as_ref().map(|n| (n, n.str())) {
        None => {
            (0..names).for_each(|_| ctx.meta());
            (Out::Null, Out::Null)
        }
        Some((_, Some("auto"))) => {
            ctx.ok();
            (0..names).for_each(|_| ctx.meta());
            (Out::Null, Out::Null)
        }
        Some((_, Some("coords"))) => {
            ctx.ok();
            (text_or_null(ctx, lat.as_ref()), text_or_null(ctx, lon.as_ref()))
        }
        Some((n, _)) => {
            let reason = "Hune reads the position from a latitude field and a longitude field only (4.33g)";
            ctx.unsupported(&n.ptr, Some(n.v), reason, Fallback::other("the fields lat and lon"));
            (0..names).for_each(|_| ctx.meta());
            (Out::Null, Out::Null)
        }
    }
}

/// Maps `config.style` of a marker layer: the size, the colour and the opacity.
fn style(ctx: &mut Ctx, node: Option<Node>) -> [(&'static str, Out); 7] {
    let mut w = OptWalk::new(ctx, node, "style");
    let size = w.take("size");
    let color = w.take("color");
    let opacity = w.take("opacity");
    let symbol = w.take("symbol");
    let text = w.take("text");
    let align = w.take("symbolAlign");
    let rotation = w.take("rotation");
    meta(ctx, &mut w, &["textConfig", "lineWidth"]);
    w.finish(ctx);
    let mut s = OptWalk::new(ctx, size, "size");
    let fixed = s.take("fixed");
    let min = s.take("min");
    let max = s.take("max");
    let size_field = s.take("field");
    if let Some(m) = s.take("mode") {
        if m.str() == Some("linear") {
            ctx.ok();
        } else {
            ctx.unsupported(
                &m.ptr,
                Some(m.v),
                "the size of a marker is linear in the value (9.19i)",
                Fallback::other("linear"),
            );
        }
    }
    s.finish(ctx);
    let fixed = whole_in(ctx, fixed.as_ref(), 1, 100, 5);
    let lo = whole_in(ctx, min.as_ref(), 1, 100, 2);
    let mut hi = whole_in(ctx, max.as_ref(), 1, 100, 15);
    if hi < lo {
        // The largest size of 4.33g is not below the smallest size. The item names `max`, or `min` when `max` is
        // absent: the default 15 is then below the value of `min`.
        if let Some(n) = max.as_ref().or(min.as_ref()) {
            ctx.changed(
                &n.ptr,
                Some(n.v),
                &format!("the largest size is not below the smallest size: it becomes {lo}"),
            );
        }
        hi = lo;
    }
    let mut c = OptWalk::new(ctx, color, "color");
    let fixed_colour = c.take("fixed");
    let color_field = c.take("field");
    c.finish(ctx);
    let fixed_colour = match fixed_colour.as_ref() {
        Some(n) => colour(ctx, n, Fallback::other("dark-green")).unwrap_or_else(|| "dark-green".to_owned()),
        None => "dark-green".to_owned(),
    };
    let percent = match opacity.as_ref() {
        None => 40,
        Some(n) => match as_num(n.v).filter(|v| (0.0..=1.0).contains(v)) {
            Some(v) => {
                ctx.ok();
                (v * 100.0).round() as i64
            }
            None => {
                ctx.unsupported(&n.ptr, Some(n.v), "the value must be a number from 0 to 1", Fallback::other("0.4"));
                40
            }
        },
    };
    if let Some(n) = symbol.as_ref() {
        let fixed = n.get("fixed");
        let mode_fixed = n.get("mode").is_none_or(|m| m.str() == Some("fixed"));
        if mode_fixed && fixed.as_ref().is_none_or(|f| f.str() == Some(CIRCLE)) {
            ctx.ok();
        } else {
            ctx.unsupported(
                &n.ptr,
                Some(n.v),
                "Hune draws each marker as a circle (9.19i)",
                Fallback::other("a circle"),
            );
        }
    }
    if let Some(n) = text.as_ref() {
        let set = ["fixed", "field"].iter().any(|k| n.get(k).is_some_and(|v| v.str().is_some_and(|t| !t.is_empty())));
        if set {
            ctx.unsupported(&n.ptr, Some(n.v), "Hune draws no text on a marker", Fallback::MemberRemoved);
        } else {
            ctx.meta();
        }
    }
    if let Some(n) = align.as_ref() {
        let centre = n.get("horizontal").is_none_or(|h| h.str() == Some("center"))
            && n.get("vertical").is_none_or(|v| v.str() == Some("center"));
        if centre {
            ctx.meta();
        } else {
            ctx.unsupported(&n.ptr, Some(n.v), "a marker has its centre on the position", Fallback::MemberRemoved);
        }
    }
    if let Some(n) = rotation.as_ref() {
        let none = n.get("field").is_none_or(|f| f.str().is_none_or(str::is_empty))
            && n.get("fixed").is_none_or(|f| as_num(f.v) == Some(0.0));
        if none {
            ctx.meta();
        } else {
            ctx.unsupported(&n.ptr, Some(n.v), "a circle has no rotation", Fallback::MemberRemoved);
        }
    }
    [
        ("size", Out::Int(fixed)),
        ("size_min", Out::Int(lo)),
        ("size_max", Out::Int(hi)),
        ("size_field", text_or_null(ctx, size_field.as_ref())),
        ("color", Out::Str(fixed_colour)),
        ("color_field", text_or_null(ctx, color_field.as_ref())),
        ("opacity", Out::Int(percent)),
    ]
}

/// Maps one marker layer. The result is the layer and `false` when the layer turns its tooltip off.
fn marker_layer(ctx: &mut Ctx, node: &Node) -> (Out, bool) {
    let mut w = OptWalk::new(ctx, Some(node.clone()), "layer");
    let _ = w.take("type");
    ctx.ok();
    let loc = w.take("location");
    let config = w.take("config");
    let tip = w.take("tooltip");
    meta(ctx, &mut w, &["name", "opacity"]);
    removed(ctx, w.take("filterData"), "the layer takes the frame of the first query (4.15a)");
    w.finish(ctx);
    let (lat_field, lon_field) = location(ctx, loc);
    let mut c = OptWalk::new(ctx, config, "config");
    let style_node = c.take("style");
    if let Some(n) = c.take("showLegend") {
        if as_flag(n.v) == Some(true) {
            ctx.changed(
                &n.ptr,
                Some(n.v),
                "Hune draws no legend of the layer: the card of a marker gives its values (9.19i)",
            );
        } else {
            ctx.meta();
        }
    }
    c.finish(ctx);
    let [size, size_min, size_max, size_field, color, color_field, opacity] = style(ctx, style_node);
    let tooltip_on = match tip.as_ref() {
        Some(n) if as_flag(n.v) == Some(false) => {
            ctx.changed(&n.ptr, Some(n.v), "the card of a marker shows after a click (the option tooltip none)");
            false
        }
        Some(_) => {
            ctx.ok();
            true
        }
        None => true,
    };
    let out = Out::members(vec![
        ("lat_field", lat_field),
        ("lon_field", lon_field),
        size,
        size_min,
        size_max,
        size_field,
        color,
        color_field,
        opacity,
    ]);
    (out, tooltip_on)
}

/// Maps `options.layers`: the first layer of the type `markers`. A tile layer `xyz` is a tile source.
fn layers(ctx: &mut Ctx, node: Option<Node>) -> (Out, bool) {
    let Some(node) = node.filter(|n| !n.is_null()) else { return (default_layer(), true) };
    if node.v.as_array().is_none() {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "the value must be an array",
            Fallback::other("the default marker layer"),
        );
        return (default_layer(), true);
    }
    let mut found: Option<(Out, bool)> = None;
    for item in node.items() {
        let kind = item.get("type").and_then(|t| t.str().map(str::to_owned));
        match kind.as_deref() {
            Some("markers") if found.is_none() => found = Some(marker_layer(ctx, &item)),
            Some("xyz") => ctx.changed(&item.ptr, Some(item.v), TILE_SOURCE),
            _ => ctx.unsupported(
                &item.ptr,
                Some(item.v),
                "Hune draws one layer of markers only (4.33g)",
                Fallback::other("layer removed"),
            ),
        }
    }
    found.unwrap_or_else(|| {
        ctx.changed(&node.ptr, Some(node.v), "the panel has no layer of markers: the default layer of 4.33g applies");
        (default_layer(), true)
    })
}

/// Maps the `options` of a `geomap` (15.6, 4.33g).
pub(super) fn geomap_options(ctx: &mut Ctx, options: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let view_node = w.take("view");
    let controls_node = w.take("controls");
    let basemap = w.take("basemap");
    let layers_node = w.take("layers");
    let tooltip_node = w.take("tooltip");
    w.finish(ctx);
    if let Some(b) = basemap.as_ref() {
        ctx.changed(&b.ptr, Some(b.v), TILE_SOURCE);
    }
    let view = view(ctx, view_node);
    let (show_zoom, wheel) = controls(ctx, controls_node);
    let (layer, layer_tooltip) = layers(ctx, layers_node);
    let mut t = OptWalk::new(ctx, tooltip_node, "tooltip");
    let mode = t.take("mode");
    t.finish(ctx);
    let tooltip = match mode.as_ref().map(|n| (n, n.str())) {
        None => "details",
        Some((_, Some(m @ ("details" | "none")))) => {
            ctx.ok();
            m
        }
        Some((n, _)) => {
            ctx.unsupported(&n.ptr, Some(n.v), "no value table of 15.6 names this value", Fallback::other("details"));
            "details"
        }
    };
    let tooltip = if layer_tooltip { tooltip } else { "none" };
    Out::obj([
        ("view", view),
        ("layer", layer),
        ("tooltip", Out::s(tooltip)),
        ("show_zoom", show_zoom),
        ("mouse_wheel_zoom", wheel),
    ])
}
