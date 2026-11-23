//! The importer of the Grafana geomap (15.6, 4.33g; decision ID-K328, item I42). The document holds no tile source: the base
//! layer and a tile layer `xyz` give a `changed` item and leave the document. The first marker layer maps.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport};
use serde_json::{Value, json};

fn geo(options: Value) -> Value {
    let mut p = panel(1, "geomap", 0);
    p["targets"] = json!([target("SELECT 48.85 AS lat, 2.35 AS lon")]);
    p["options"] = options;
    p
}

fn markers(style: Value, location: Value) -> Value {
    json!({"type": "markers", "name": "Layer 1", "config": {"showLegend": false, "style": style}, "location": location, "tooltip": true})
}

fn one(p: Value) -> DashboardImport {
    run_plain(&dash(vec![p], vec![]))
}

fn options_of(p: Value) -> Value {
    first_panel(&doc(&run(&dash(vec![p], vec![]))))["options"].clone()
}

const DEFAULT_LAYER: &str = r#"{"lat_field":null,"lon_field":null,"size":5,"size_min":2,"size_max":15,"size_field":null,"color":"dark-green","color_field":null,"opacity":40}"#;

#[test]
fn a_panel_with_no_options_gets_the_defaults_of_grafana() {
    let r = one(geo(json!({})));
    no_items(&r);
    let o = first_panel(&doc(&r))["options"].clone();
    assert_eq!(o["view"], json!({"mode": "fixed", "lat": 0, "lon": 0, "zoom": 1}));
    assert_eq!(o["layer"], serde_json::from_str::<Value>(DEFAULT_LAYER).unwrap());
    assert_eq!(
        (o["tooltip"].clone(), o["show_zoom"].clone(), o["mouse_wheel_zoom"].clone()),
        (json!("details"), json!(true), json!(true))
    );
}

#[test]
fn the_base_layer_and_a_tile_layer_are_changed_and_leave_the_document() {
    let p = geo(json!({
        "basemap": {"type": "xyz", "name": "Layer 0", "config": {"url": "https://tiles.example.org/{z}/{x}/{y}.png", "attribution": "x"}},
        "layers": [{"type": "xyz", "name": "Tiles", "config": {"url": "https://evil.example/{z}/{x}/{y}.png"}}, markers(json!({}), json!({"mode": "auto"}))]
    }));
    let r = one(p.clone());
    let base = one_item(&r, "options.basemap");
    assert_eq!(base.class, Class::Changed);
    assert!(base.reason.contains("block geomap"), "{}", base.reason);
    let tile = one_item(&r, "options.layers.0");
    assert_eq!(tile.class, Class::Changed);
    let bytes = String::from_utf8(run(&dash(vec![p], vec![])).document_bytes.unwrap()).unwrap();
    assert!(!bytes.contains("example.org") && !bytes.contains("evil.example") && !bytes.contains("basemap"), "{bytes}");
    // The default base layer of Grafana is changed too: it is a tile source of CARTO.
    let r = one(geo(json!({"basemap": {"type": "default", "name": "Basemap"}})));
    assert_eq!(one_item(&r, "options.basemap").class, Class::Changed);
}

#[test]
fn the_views_map_to_a_centre_and_a_whole_zoom() {
    for (view, want) in [
        (json!({"id": "zero", "lat": 0, "lon": 0, "zoom": 1}), json!({"mode": "fixed", "lat": 0, "lon": 0, "zoom": 1})),
        (
            json!({"id": "coords", "lat": 40.5, "lon": -74.25, "zoom": 6}),
            json!({"mode": "fixed", "lat": 40.5, "lon": -74.25, "zoom": 6}),
        ),
        (json!({"id": "europe", "zoom": 4}), json!({"mode": "fixed", "lat": 46, "lon": 14, "zoom": 4})),
        (json!({"id": "s-asia"}), json!({"mode": "fixed", "lat": 19.5, "lon": 80, "zoom": 4})),
        (json!({"id": "fit", "zoom": 9, "allLayers": true}), json!({"mode": "fit", "lat": 0, "lon": 0, "zoom": 9})),
        (json!({"id": "fit"}), json!({"mode": "fit", "lat": 0, "lon": 0, "zoom": 15})),
        (json!({"id": "fit", "maxZoom": 7}), json!({"mode": "fit", "lat": 0, "lon": 0, "zoom": 7})),
    ] {
        let r = one(geo(json!({"view": view.clone()})));
        no_items(&r);
        assert_eq!(first_panel(&doc(&r))["options"]["view"], want, "{view}");
    }
    // A zoom with a fraction is rounded, with a `changed` item. An unknown view and a zoom out of range are `unsupported`.
    let r = one(geo(json!({"view": {"id": "coords", "lat": 1, "lon": 2, "zoom": 3.6}})));
    assert_eq!(one_item(&r, "options.view.zoom").class, Class::Changed);
    assert_eq!(
        options_of(geo(json!({"view": {"id": "coords", "lat": 1, "lon": 2, "zoom": 3.6}})))["view"]["zoom"],
        json!(4)
    );
    for view in [
        json!({"id": "mars"}),
        json!({"id": "coords", "lat": 91}),
        json!({"id": "coords", "zoom": 30}),
        json!({"id": 7}),
    ] {
        let r = one(geo(json!({"view": view.clone()})));
        assert!(r.items.iter().any(|i| i.class == Class::Unsupported), "{view}: {:#?}", r.items);
    }
    let r = one(geo(json!({"view": {"id": "coords", "shared": true}})));
    assert_eq!(one_item(&r, "options.view.shared").class, Class::Unsupported);
    let r = one(geo(json!({"view": {"id": "fit", "zoom": 5, "padding": 20}})));
    assert_eq!(one_item(&r, "options.view.padding").class, Class::Changed);
}

#[test]
fn the_marker_layer_maps_the_fields_the_size_the_colour_and_the_opacity() {
    let layer = markers(
        json!({"size": {"fixed": 9, "min": 3, "max": 30, "field": "v"}, "color": {"fixed": "#ff0000", "field": "v"}, "opacity": 0.8,
               "symbol": {"mode": "fixed", "fixed": "img/icons/marker/circle.svg"}, "rotation": {"fixed": 0, "mode": "mod"}}),
        json!({"mode": "coords", "latitude": "Y", "longitude": "X"}),
    );
    let r = one(geo(
        json!({"layers": [layer], "tooltip": {"mode": "none"}, "controls": {"showZoom": false, "mouseWheelZoom": false}}),
    ));
    no_items(&r);
    let o = first_panel(&doc(&r))["options"].clone();
    assert_eq!(
        o["layer"],
        json!({"lat_field": "Y", "lon_field": "X", "size": 9, "size_min": 3, "size_max": 30, "size_field": "v", "color": "#ff0000", "color_field": "v", "opacity": 80})
    );
    assert_eq!(
        (o["tooltip"].clone(), o["show_zoom"].clone(), o["mouse_wheel_zoom"].clone()),
        (json!("none"), json!(false), json!(false))
    );
}

#[test]
fn what_a_marker_layer_cannot_give_is_listed() {
    for (layer, member, class) in [
        (
            markers(json!({}), json!({"mode": "geohash", "geohash": "gh"})),
            "options.layers.0.location.mode",
            Class::Unsupported,
        ),
        (
            markers(json!({}), json!({"mode": "lookup", "lookup": "country"})),
            "options.layers.0.location.mode",
            Class::Unsupported,
        ),
        (
            markers(
                json!({"symbol": {"mode": "fixed", "fixed": "img/icons/marker/star.svg"}}),
                json!({"mode": "auto"}),
            ),
            "options.layers.0.config.style.symbol",
            Class::Unsupported,
        ),
        (
            markers(json!({"text": {"fixed": "label"}}), json!({"mode": "auto"})),
            "options.layers.0.config.style.text",
            Class::Unsupported,
        ),
        (
            markers(json!({"opacity": 3}), json!({"mode": "auto"})),
            "options.layers.0.config.style.opacity",
            Class::Unsupported,
        ),
        (
            markers(json!({"size": {"fixed": 500}}), json!({"mode": "auto"})),
            "options.layers.0.config.style.size.fixed",
            Class::Unsupported,
        ),
        (
            markers(json!({"size": {"min": 20, "max": 10}}), json!({"mode": "auto"})),
            "options.layers.0.config.style.size.max",
            Class::Changed,
        ),
    ] {
        let r = one(geo(json!({"layers": [layer]})));
        assert_eq!(one_item(&r, member).class, class, "{member}: {:#?}", r.items);
    }
    // The legend of the layer and a layer that turns its tooltip off.
    let mut l = markers(json!({}), json!({"mode": "auto"}));
    l["config"]["showLegend"] = json!(true);
    l["tooltip"] = json!(false);
    let r = one(geo(json!({"layers": [l.clone()]})));
    assert_eq!(one_item(&r, "options.layers.0.config.showLegend").class, Class::Changed);
    assert_eq!(one_item(&r, "options.layers.0.tooltip").class, Class::Changed);
    assert_eq!(options_of(geo(json!({"layers": [l]})))["tooltip"], json!("none"));
    // A second marker layer and a heatmap layer are `unsupported`; a panel with no marker layer gets the default layer.
    let r = one(geo(
        json!({"layers": [markers(json!({}), json!({})), {"type": "heatmap", "config": {}}, markers(json!({}), json!({}))]}),
    ));
    assert_eq!(items_with(&r, "options.layers.1").len(), 1);
    assert_eq!(items_with(&r, "options.layers.2").len(), 1);
    let r = one(geo(json!({"layers": [{"type": "heatmap", "config": {}}]})));
    assert!(r.items.iter().any(|i| i.member == "options.layers" && i.class == Class::Changed), "{:#?}", r.items);
}

#[test]
fn the_controls_that_hune_has_not_are_listed() {
    let r = one(geo(
        json!({"controls": {"showZoom": true, "mouseWheelZoom": true, "showAttribution": false, "showScale": true, "showMeasure": true, "showDebug": true, "scaleUnits": "m"}}),
    ));
    assert_eq!(one_item(&r, "options.controls.showAttribution").class, Class::Changed);
    for m in ["showScale", "showMeasure", "showDebug"] {
        assert_eq!(one_item(&r, &format!("options.controls.{m}")).class, Class::Unsupported, "{m}");
    }
    let r = one(geo(
        json!({"controls": {"showScale": false, "showMeasure": false, "showDebug": false, "showAttribution": true}}),
    ));
    no_items(&r);
}

#[test]
fn a_map_keeps_the_first_query_only() {
    let mut p = geo(json!({}));
    p["targets"] = json!([target("SELECT 1 AS lat, 2 AS lon"), target("SELECT 3 AS lat, 4 AS lon")]);
    p["targets"][1]["refId"] = json!("B");
    let d = doc(&run(&dash(vec![p], vec![])));
    assert!(first_panel(&d)["data"].get("extra_queries").is_none(), "{}", first_panel(&d));
}
