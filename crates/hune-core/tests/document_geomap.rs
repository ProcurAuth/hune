//! The panel type `geomap` (4.33g, decision ID-K328): the closed options, their ranges, the canonical order, the rule of the
//! sizes, the frame of one query, and no tile source in the document.

use hune_core::document::{GeomapOptions, GeomapTooltip, GeomapViewMode, PanelOptions, PanelType, TextEnum, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const P: &str = "/sections/0/panels/0";

fn options() -> Value {
    json!({
        "view": {"mode": "fixed", "lat": 0, "lon": 0, "zoom": 1},
        "layer": {
            "lat_field": null, "lon_field": null, "size": 5, "size_min": 2, "size_max": 15, "size_field": null,
            "color": "dark-green", "color_field": null, "opacity": 40
        },
        "tooltip": "details", "show_zoom": true, "mouse_wheel_zoom": true
    })
}

fn geomap_panel(options: Value) -> Value {
    json!({
        "id": 1, "type": "geomap", "title": "Map", "description": "", "transparent": false,
        "grid": {"x": 0, "y": 0, "w": 12, "h": 9},
        "data": {"query": {"source": "prod", "format": "table", "sql": ["SELECT lat, lon, v FROM geo.points"], "min_interval": null, "max_data_points": null}},
        "transforms": [], "fields": {"defaults": {}, "overrides": []}, "options": options, "links": []
    })
}

fn doc_of(panel: Value) -> Value {
    json!({
        "format_version": 3, "uid": "geo-doc", "title": "Geo", "description": "", "tags": [],
        "time": {"from": "now-6h", "to": "now", "timezone": "utc", "refresh": null},
        "cursor_sync": "off", "links": [], "variables": [],
        "sections": [{"title": null, "collapsed": false, "panels": [panel]}]
    })
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{e:#?}");
}

fn with(path: &str, value: Value) -> Value {
    let mut o = options();
    *o.pointer_mut(path).unwrap_or_else(|| panic!("{path}")) = value;
    doc_of(geomap_panel(o))
}

fn typed(v: &Value) -> GeomapOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let p = &doc.document().sections[0].panels[0];
    assert_eq!(p.panel_type, PanelType::Geomap);
    match &p.options {
        PanelOptions::Geomap(o) => o.clone(),
        other => panic!("not a map: {other:?}"),
    }
}

#[test]
fn the_new_panel_is_valid_and_keeps_its_values() {
    assert_eq!(PanelType::Geomap.as_str(), "geomap");
    let o = typed(&doc_of(geomap_panel(options())));
    assert_eq!(o.view.mode, GeomapViewMode::Fixed);
    assert_eq!((o.view.lat, o.view.lon, o.view.zoom), (0.0, 0.0, 1));
    assert_eq!((o.layer.size, o.layer.size_min, o.layer.size_max, o.layer.opacity), (5, 2, 15, 40));
    assert_eq!(o.layer.color, "dark-green");
    assert_eq!(o.tooltip, GeomapTooltip::Details);
    assert!(o.show_zoom && o.mouse_wheel_zoom);
}

#[test]
fn the_canonical_form_has_the_order_of_the_tables() {
    // An input in another order gives the canonical order of 4.33g.
    let o = json!({
        "mouse_wheel_zoom": false, "show_zoom": false, "tooltip": "none",
        "layer": {"opacity": 100, "color_field": "v", "color": "#ff0000", "size_field": "v", "size_max": 30, "size_min": 3, "size": 9, "lon_field": "X", "lat_field": "Y"},
        "view": {"zoom": 6, "lon": -74.5, "lat": 40.25, "mode": "fit"}
    });
    let doc = validate(&serde_json::to_vec(&doc_of(geomap_panel(o))).unwrap(), None).unwrap();
    let text = String::from_utf8(doc.bytes().to_vec()).unwrap();
    let order = [
        "\"view\"",
        "\"mode\": \"fit\"",
        "\"lat\": 40.25",
        "\"lon\": -74.5",
        "\"zoom\": 6",
        "\"layer\"",
        "\"lat_field\": \"Y\"",
        "\"lon_field\": \"X\"",
        "\"size\": 9",
        "\"size_min\": 3",
        "\"size_max\": 30",
        "\"size_field\": \"v\"",
        "\"color\": \"#ff0000\"",
        "\"color_field\": \"v\"",
        "\"opacity\": 100",
        "\"tooltip\": \"none\"",
        "\"show_zoom\": false",
        "\"mouse_wheel_zoom\": false",
    ];
    let mut from = 0;
    for part in order {
        let at = text[from..].find(part).unwrap_or_else(|| panic!("{part} after {from}: {text}"));
        from += at + part.len();
    }
    // The canonical bytes validate to the same bytes (4.38).
    let again = validate(doc.bytes(), None).unwrap();
    assert_eq!(again.bytes(), doc.bytes());
}

#[test]
fn each_range_has_its_bounds() {
    for (path, ok, bad) in [
        ("/view/lat", vec![json!(-90), json!(90), json!(12.5)], vec![json!(-90.0001), json!(90.5)]),
        ("/view/lon", vec![json!(-180), json!(180)], vec![json!(-180.01), json!(181)]),
        ("/view/zoom", vec![json!(0), json!(22)], vec![json!(23), json!(-1)]),
        ("/layer/size", vec![json!(1), json!(100)], vec![json!(0), json!(101)]),
        ("/layer/size_min", vec![json!(1), json!(15)], vec![json!(0), json!(101)]),
        ("/layer/size_max", vec![json!(2), json!(100)], vec![json!(0), json!(101)]),
        ("/layer/opacity", vec![json!(0), json!(100)], vec![json!(-1), json!(101)]),
    ] {
        for v in ok {
            assert!(errors_of(&with(path, v.clone())).is_empty(), "{path} {v}");
        }
        for v in bad {
            expect(&with(path, v.clone()), &format!("{P}/options{path}"), ErrorCode::OutOfRange);
        }
    }
    expect(&with("/view/zoom", json!(1.5)), &format!("{P}/options/view/zoom"), ErrorCode::WrongType);
    expect(&with("/view/lat", json!("0")), &format!("{P}/options/view/lat"), ErrorCode::WrongType);
}

#[test]
fn the_enums_the_names_and_the_colour_are_checked() {
    expect(&with("/view/mode", json!("zero")), &format!("{P}/options/view/mode"), ErrorCode::BadFormat);
    expect(&with("/view/mode", json!("coords")), &format!("{P}/options/view/mode"), ErrorCode::BadFormat);
    expect(&with("/tooltip", json!("multi")), &format!("{P}/options/tooltip"), ErrorCode::BadFormat);
    expect(&with("/layer/color", json!("not a colour")), &format!("{P}/options/layer/color"), ErrorCode::BadFormat);
    expect(&with("/layer/lat_field", json!("")), &format!("{P}/options/layer/lat_field"), ErrorCode::OutOfRange);
    expect(
        &with("/layer/size_field", json!("a".repeat(1025))),
        &format!("{P}/options/layer/size_field"),
        ErrorCode::OutOfRange,
    );
    assert!(errors_of(&with("/layer/color_field", json!("<b>x</b>"))).is_empty());
    expect(&with("/show_zoom", json!(1)), &format!("{P}/options/show_zoom"), ErrorCode::WrongType);
}

#[test]
fn the_largest_size_is_not_below_the_smallest_size() {
    let mut o = options();
    o["layer"]["size_min"] = json!(20);
    o["layer"]["size_max"] = json!(19);
    expect(&doc_of(geomap_panel(o.clone())), &format!("{P}/options/layer/size_max"), ErrorCode::OutOfRange);
    o["layer"]["size_max"] = json!(20);
    assert!(errors_of(&doc_of(geomap_panel(o))).is_empty());
}

#[test]
fn the_options_are_closed_and_hold_no_tile_source() {
    for (path, key) in
        [("", "basemap"), ("", "layers"), ("", "controls"), ("/view", "id"), ("/layer", "url"), ("/layer", "tile_url")]
    {
        let mut o = options();
        let at = if path.is_empty() { &mut o } else { o.pointer_mut(path).unwrap() };
        at.as_object_mut().unwrap().insert(key.to_owned(), json!("https://tiles.example.org/{z}/{x}/{y}.png"));
        let ptr = format!("{P}/options{path}/{key}");
        expect(&doc_of(geomap_panel(o)), &ptr, ErrorCode::UnknownMember);
    }
    for key in ["view", "layer", "tooltip", "show_zoom", "mouse_wheel_zoom"] {
        let mut o = options();
        o.as_object_mut().unwrap().remove(key);
        expect(&doc_of(geomap_panel(o)), &format!("{P}/options"), ErrorCode::MissingMember);
    }
    let mut o = options();
    o["layer"].as_object_mut().unwrap().remove("opacity");
    expect(&doc_of(geomap_panel(o)), &format!("{P}/options/layer"), ErrorCode::MissingMember);
}

#[test]
fn a_map_draws_the_frame_of_one_query() {
    let mut p = geomap_panel(options());
    p["data"]["extra_queries"] =
        json!([{"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 1"], "hide": false}]);
    let e = errors_of(&doc_of(p));
    assert!(
        e.iter().any(|e| e.pointer == format!("{P}/data/extra_queries") && e.code == ErrorCode::NotPermitted),
        "{e:#?}"
    );
}

#[test]
fn the_style_members_of_a_series_are_not_for_a_map() {
    let mut p = geomap_panel(options());
    p["fields"]["defaults"] = json!({"line_width": 2});
    let e = errors_of(&doc_of(p));
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(e[0].code, ErrorCode::NotPermitted);
}
