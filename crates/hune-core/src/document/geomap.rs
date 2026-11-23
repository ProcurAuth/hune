//! The options of the panel type `geomap` (4.33g, decision ID-K328): the start view and one marker layer.
//!
//! The document holds no tile source. The tile source comes only from the block `geomap` of the configuration (13.12c),
//! so a member such as `basemap` or a URL is `unknown_member` (4.36). The walk, the model and the canonical form of the
//! options are in this module. `walk.rs`, `model.rs` and `canonical.rs` call it.

use super::canonical::Out;
use super::grammar::is_valid_colour;
use super::model::TextEnum;
use super::read::{Ctx, Field, Obj, TextRule};
use crate::error::ErrorCode;
use crate::limits::MAX_VALUE_BYTES;

/// The highest zoom of the start view: the bound of `geomap.max_zoom` (13.12c).
pub const GEOMAP_MAX_ZOOM: i64 = 22;
/// The largest size of a marker, in CSS pixels: the bound of the size editor of Grafana 12.4.2.
pub const GEOMAP_MAX_MARKER_SIZE: i64 = 100;

/// A display name of a field: 1 to 1024 bytes.
const NAME: TextRule = TextRule::bytes(1, MAX_VALUE_BYTES);
/// A colour of 4.19 before its grammar check.
const COLOUR: TextRule = TextRule::bytes(1, 64);

/// The member `mode` of the start view (4.33g).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeomapViewMode {
    /// The centre `lat`, `lon` and the zoom `zoom`.
    Fixed,
    /// The view of all the markers, up to the zoom `zoom`. Only a page with no tile source uses it (9.19i).
    Fit,
}

impl TextEnum for GeomapViewMode {
    const NAMES: &'static [&'static str] = &["fixed", "fit"];
    fn parse(s: &str) -> Option<Self> {
        match s {
            "fixed" => Some(GeomapViewMode::Fixed),
            "fit" => Some(GeomapViewMode::Fit),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            GeomapViewMode::Fixed => "fixed",
            GeomapViewMode::Fit => "fit",
        }
    }
}

/// The member `tooltip` of a `geomap` (4.33g).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeomapTooltip {
    /// The card of the marker under the pointer.
    Details,
    /// The card shows after a click only.
    None,
}

impl TextEnum for GeomapTooltip {
    const NAMES: &'static [&'static str] = &["details", "none"];
    fn parse(s: &str) -> Option<Self> {
        match s {
            "details" => Some(GeomapTooltip::Details),
            "none" => Some(GeomapTooltip::None),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            GeomapTooltip::Details => "details",
            GeomapTooltip::None => "none",
        }
    }
}

/// The object `view` of a `geomap` (4.33g).
#[derive(Debug, Clone, PartialEq)]
pub struct GeomapView {
    /// `fixed` or `fit`.
    pub mode: GeomapViewMode,
    /// The latitude of the centre: -90 to 90.
    pub lat: f64,
    /// The longitude of the centre: -180 to 180.
    pub lon: f64,
    /// The zoom: 0 to 22. With `fit`, the highest zoom.
    pub zoom: u8,
}

/// The object `layer` of a `geomap`: one layer of markers (4.33g).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeomapLayer {
    /// The display name of the latitude field, or `None` for the names `lat` and `latitude`.
    pub lat_field: Option<String>,
    /// The display name of the longitude field, or `None` for the names `lon`, `lng` and `longitude`.
    pub lon_field: Option<String>,
    /// The size of a marker with no `size_field`: 1 to 100 CSS pixels.
    pub size: u8,
    /// The size of the lowest value of `size_field`: 1 to 100 CSS pixels.
    pub size_min: u8,
    /// The size of the highest value of `size_field`: `size_min` to 100 CSS pixels.
    pub size_max: u8,
    /// The display name of the field that gives the size, or `None`.
    pub size_field: Option<String>,
    /// The colour of a marker with no `color_field`: a colour of 4.19.
    pub color: String,
    /// The display name of the field whose colour (9.16) gives the colour of each marker, or `None`.
    pub color_field: Option<String>,
    /// The opacity of the fill: 0 to 100 (percent).
    pub opacity: u8,
}

/// Options of `geomap` (4.33g).
#[derive(Debug, Clone, PartialEq)]
pub struct GeomapOptions {
    /// The start view.
    pub view: GeomapView,
    /// The marker layer.
    pub layer: GeomapLayer,
    /// `details` or `none`.
    pub tooltip: GeomapTooltip,
    /// `true`: the buttons "Zoom in" and "Zoom out" show at the top left.
    pub show_zoom: bool,
    /// `true`: the mouse wheel zooms the map.
    pub mouse_wheel_zoom: bool,
}

const VIEW_MEMBERS: [&str; 4] = ["mode", "lat", "lon", "zoom"];
const LAYER_MEMBERS: [&str; 9] =
    ["lat_field", "lon_field", "size", "size_min", "size_max", "size_field", "color", "color_field", "opacity"];
const OPTION_MEMBERS: [&str; 5] = ["view", "layer", "tooltip", "show_zoom", "mouse_wheel_zoom"];

/// Reads a number in `min..=max`.
fn bounded(cx: &mut Ctx, obj: &mut Obj<'_>, name: &str, min: f64, max: f64) -> Option<f64> {
    let f = obj.get(cx, name)?;
    let v = f.num(cx)?;
    if (min..=max).contains(&v) {
        Some(v)
    } else {
        f.fail(cx, ErrorCode::OutOfRange, &format!("must be {min} to {max}"));
        None
    }
}

/// Reads an integer in `min..=max` that fits in a byte.
fn small(cx: &mut Ctx, obj: &mut Obj<'_>, name: &str, min: i64, max: i64) -> Option<u8> {
    obj.int(cx, name, min, max).and_then(|v| u8::try_from(v).ok())
}

/// Reads a display name or `null`.
fn name_or_null(cx: &mut Ctx, obj: &mut Obj<'_>, name: &str) -> Option<Option<String>> {
    obj.get(cx, name).and_then(|f| f.nullable(|f| f.text(cx, NAME)))
}

fn view(cx: &mut Ctx, f: &Field<'_>) -> Option<GeomapView> {
    let mut obj = f.object(cx, "view")?;
    let mode = obj.en::<GeomapViewMode>(cx, "mode");
    let lat = bounded(cx, &mut obj, "lat", -90.0, 90.0);
    let lon = bounded(cx, &mut obj, "lon", -180.0, 180.0);
    let zoom = small(cx, &mut obj, "zoom", 0, GEOMAP_MAX_ZOOM);
    obj.finish(cx, &VIEW_MEMBERS);
    Some(GeomapView { mode: mode?, lat: lat?, lon: lon?, zoom: zoom? })
}

fn layer(cx: &mut Ctx, f: &Field<'_>) -> Option<GeomapLayer> {
    let mut obj = f.object(cx, "layer")?;
    let lat_field = name_or_null(cx, &mut obj, "lat_field");
    let lon_field = name_or_null(cx, &mut obj, "lon_field");
    let size = small(cx, &mut obj, "size", 1, GEOMAP_MAX_MARKER_SIZE);
    let size_min = small(cx, &mut obj, "size_min", 1, GEOMAP_MAX_MARKER_SIZE);
    let max_field = obj.get(cx, "size_max");
    let size_max =
        max_field.as_ref().and_then(|m| m.int(cx, 1, GEOMAP_MAX_MARKER_SIZE)).and_then(|v| u8::try_from(v).ok());
    let size_field = name_or_null(cx, &mut obj, "size_field");
    let color = obj.get(cx, "color").and_then(|c| {
        let text = c.text(cx, COLOUR)?;
        if is_valid_colour(&text) {
            Some(text)
        } else {
            c.fail(cx, ErrorCode::BadFormat, "must be a colour (4.19)");
            None
        }
    });
    let color_field = name_or_null(cx, &mut obj, "color_field");
    let opacity = small(cx, &mut obj, "opacity", 0, 100);
    obj.finish(cx, &LAYER_MEMBERS);
    // Rule 2 of 4.33g: the size of the highest value is not below the size of the lowest value.
    if let (Some(lo), Some(hi), Some(m)) = (size_min, size_max, max_field.as_ref())
        && hi < lo
    {
        m.fail(cx, ErrorCode::OutOfRange, &format!("must be at least size_min ({lo})"));
        return None;
    }
    Some(GeomapLayer {
        lat_field: lat_field?,
        lon_field: lon_field?,
        size: size?,
        size_min: size_min?,
        size_max: size_max?,
        size_field: size_field?,
        color: color?,
        color_field: color_field?,
        opacity: opacity?,
    })
}

/// Reads the `options` of a `geomap` (4.33g). Each member is necessary, and the object is closed.
pub(super) fn options(cx: &mut Ctx, f: &Field<'_>) -> Option<GeomapOptions> {
    let mut obj = f.object(cx, "options")?;
    let view = obj.get(cx, "view").and_then(|f| view(cx, &f));
    let layer = obj.get(cx, "layer").and_then(|f| layer(cx, &f));
    let tooltip = obj.en::<GeomapTooltip>(cx, "tooltip");
    let show_zoom = obj.boolean(cx, "show_zoom");
    let mouse_wheel_zoom = obj.boolean(cx, "mouse_wheel_zoom");
    obj.finish(cx, &OPTION_MEMBERS);
    Some(GeomapOptions {
        view: view?,
        layer: layer?,
        tooltip: tooltip?,
        show_zoom: show_zoom?,
        mouse_wheel_zoom: mouse_wheel_zoom?,
    })
}

fn text_or_null(v: &Option<String>) -> Out {
    v.as_deref().map_or(Out::Null, |s| Out::Str(s.to_owned()))
}

/// The canonical form of the options (4.38): the members in the order of the tables of 4.33g.
pub(super) fn canonical(o: &GeomapOptions) -> Out {
    let l = &o.layer;
    Out::Obj(vec![
        (
            "view",
            Out::Obj(vec![
                ("mode", Out::Str(o.view.mode.as_str().to_owned())),
                ("lat", Out::Num(o.view.lat)),
                ("lon", Out::Num(o.view.lon)),
                ("zoom", Out::Int(i64::from(o.view.zoom))),
            ]),
        ),
        (
            "layer",
            Out::Obj(vec![
                ("lat_field", text_or_null(&l.lat_field)),
                ("lon_field", text_or_null(&l.lon_field)),
                ("size", Out::Int(i64::from(l.size))),
                ("size_min", Out::Int(i64::from(l.size_min))),
                ("size_max", Out::Int(i64::from(l.size_max))),
                ("size_field", text_or_null(&l.size_field)),
                ("color", Out::Str(l.color.clone())),
                ("color_field", text_or_null(&l.color_field)),
                ("opacity", Out::Int(i64::from(l.opacity))),
            ]),
        ),
        ("tooltip", Out::Str(o.tooltip.as_str().to_owned())),
        ("show_zoom", Out::Bool(o.show_zoom)),
        ("mouse_wheel_zoom", Out::Bool(o.mouse_wheel_zoom)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_enums_read_and_write_their_texts() {
        for v in [GeomapViewMode::Fixed, GeomapViewMode::Fit] {
            assert_eq!(GeomapViewMode::parse(v.as_str()), Some(v));
        }
        for v in [GeomapTooltip::Details, GeomapTooltip::None] {
            assert_eq!(GeomapTooltip::parse(v.as_str()), Some(v));
        }
        for bad in ["", "Fixed", "zero", "coords", "hidden", "single"] {
            assert_eq!(GeomapViewMode::parse(bad), None);
            assert_eq!(GeomapTooltip::parse(bad), None);
        }
        assert_eq!(LAYER_MEMBERS.len(), 9);
        assert_eq!(OPTION_MEMBERS.len(), 5);
    }
}
