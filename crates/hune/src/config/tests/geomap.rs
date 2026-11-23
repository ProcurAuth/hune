//! Tests of the optional block `geomap` (13.12c, decision ID-K328): the grammar of `tile_url` with each refusal of the shared
//! list FE-85, the attribution, the zoom, the closed block, the derived `img-src` of `print-config` and of the start log, and W9.

use super::*;

const VECTORS: &str = include_str!("../../../../../tests/frontend/vectors/FE-85-geomap.json");
const TILE: &str = "https://tiles.example.org/osm/{z}/{x}/{y}.png";

/// The local file with a block `geomap` at its end. The texts are quoted as JSON strings, which is the escape form of HCL.
fn with_block(fx: &Fx, tile_url: &str, attribution: &str, max_zoom: &str) -> String {
    format!(
        "{}\ngeomap {{\n  tile_url    = {}\n  attribution = {}\n  max_zoom    = {max_zoom}\n}}\n",
        fx.local(),
        hclconf::value::json_string(tile_url),
        hclconf::value::json_string(attribution)
    )
}

fn vectors() -> serde_json::Value {
    serde_json::from_str(VECTORS).unwrap()
}

#[test]
fn without_the_block_the_feature_is_off() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert!(loaded.config.geomap.is_none());
    assert!(loaded.effective.iter().all(|e| !e.key.starts_with("geomap")));
    assert!(loaded.warnings.iter().all(|w| w.key != "geomap.tile_url"));
    assert!(!render_print_config(&loaded).contains("geomap"));
}

#[test]
fn a_valid_block_gives_the_typed_parts_the_derived_source_and_w9() {
    let fx = Fx::new();
    let loaded = fx.load(&with_block(&fx, TILE, "Tiles: example.org", "19")).unwrap();
    let g = loaded.config.geomap.as_ref().unwrap();
    assert_eq!(g.tile_url.text, TILE);
    assert_eq!(g.tile_url.host, "tiles.example.org");
    assert_eq!(g.attribution, "Tiles: example.org");
    assert_eq!(g.max_zoom, 19);
    assert_eq!(g.img_src(), "'self' data: https://tiles.example.org/osm/");
    let printed = render_print_config(&loaded);
    for line in [
        "geomap.tile_url = \"https://tiles.example.org/osm/{z}/{x}/{y}.png\"  # file\n",
        "geomap.attribution = \"Tiles: example.org\"  # file\n",
        "geomap.max_zoom = 19  # file\n",
        "geomap.img_src = \"'self' data: https://tiles.example.org/osm/\"  # derived\n",
    ] {
        assert!(printed.contains(line), "{line}{printed}");
    }
    // The derived line follows the last key of the block.
    assert!(printed.contains("geomap.max_zoom = 19  # file\ngeomap.img_src = "));
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config["geomap.img_src"], "'self' data: https://tiles.example.org/osm/");
    assert_eq!(config["geomap.max_zoom"], 19);
    let sources: serde_json::Value = serde_json::from_str(&start_log_sources_json(&loaded)).unwrap();
    assert_eq!(sources["geomap.img_src"], "derived");
    let w9: Vec<&Warning> = loaded.warnings.iter().filter(|w| w.key == "geomap.tile_url").collect();
    assert_eq!(w9.len(), 1);
    assert_eq!(
        w9[0].message,
        "geomap.tile_url is set: the browser of each viewer contacts https://tiles.example.org. That host sees the address of the viewer and the area of the map."
    );
    // W9 is the last warning (13.15).
    assert_eq!(loaded.warnings.last().map(|w| w.key.as_str()), Some("geomap.tile_url"));
}

#[test]
fn each_valid_text_of_the_shared_list_loads_with_its_source() {
    let fx = Fx::new();
    for item in vectors()["tile_url"]["valid"].as_array().unwrap() {
        let (text, source) = (item[0].as_str().unwrap(), item[1].as_str().unwrap());
        let loaded = fx.load(&with_block(&fx, text, "a", "0")).unwrap_or_else(|e| panic!("{text}: {e:?}"));
        assert_eq!(loaded.config.geomap.unwrap().img_src(), format!("'self' data: {source}"), "{text}");
    }
}

#[test]
fn each_text_outside_the_grammar_stops_the_start_with_the_key_the_rule_and_the_value() {
    let fx = Fx::new();
    let list = vectors()["tile_url"]["invalid"].as_array().unwrap().clone();
    assert!(list.len() >= 60);
    for item in &list {
        let text = item.as_str().unwrap();
        let errors = fx.errors(&with_block(&fx, text, "a", "0"));
        assert_eq!(errors.len(), 1, "{text:?}: {errors:#?}");
        assert!(errors[0].starts_with("geomap.tile_url: expected "), "{text:?}: {}", errors[0]);
    }
    // A text that passes the key table gets the rule of 13.13, the reason and the value.
    let one = fx.one(&with_block(&fx, "https://192.168.1.10/{z}/{x}/{y}.png", "a", "0"));
    assert!(one.contains(super::super::geomap::TILE_URL_RULE), "{one}");
    assert!(one.contains("(the host is an IP address), found \"https://192.168.1.10/{z}/{x}/{y}.png\""), "{one}");
}

#[test]
fn the_attribution_and_the_zoom_have_their_rules() {
    let fx = Fx::new();
    assert!(fx.load(&with_block(&fx, TILE, &"\u{e9}".repeat(200), "22")).is_ok());
    for (attribution, zoom) in [
        ("", "1"),
        (&"a".repeat(201) as &str, "1"),
        ("a\u{202e}b", "1"),
        ("a\u{200b}b", "1"),
        ("a\u{feff}", "1"),
        ("a\u{1}b", "1"),
        ("a\tb", "1"),
        ("a", "23"),
        ("a", "-1"),
        ("a", "\"5\""),
        ("a", "1.5"),
    ] {
        let errors = fx.errors(&with_block(&fx, TILE, attribution, zoom));
        assert_eq!(errors.len(), 1, "{attribution:?} {zoom}: {errors:#?}");
        assert!(errors[0].starts_with("geomap."), "{}", errors[0]);
    }
}

#[test]
fn the_block_is_closed_and_each_key_is_necessary_in_the_value_form() {
    let fx = Fx::new();
    let base = with_block(&fx, TILE, "a", "3");
    // An unknown attribute, a key that is absent, a second block, a label, and the forms `_env` and `_file`.
    let cases = [
        replace_once(&base, "  max_zoom    = 3\n", "  max_zoom    = 3\n  url = \"x\"\n"),
        replace_once(&base, "  max_zoom    = 3\n", ""),
        replace_once(&base, "  attribution = \"a\"\n", ""),
        format!(
            "{base}\ngeomap {{\n  tile_url = {}\n  attribution = \"a\"\n  max_zoom = 3\n}}\n",
            hclconf::value::json_string(TILE)
        ),
        base.replace("geomap {", "geomap \"x\" {"),
        replace_once(
            &base,
            &format!("  tile_url    = {}\n", hclconf::value::json_string(TILE)),
            "  tile_url_env = \"TILES\"\n",
        ),
        replace_once(
            &base,
            &format!("  tile_url    = {}\n", hclconf::value::json_string(TILE)),
            "  tile_url_file = \"/run/x\"\n",
        ),
        format!("{}\ngeomap {{\n}}\n", fx.local()),
    ];
    for text in &cases {
        let errors = fx.load_env(text, &[("TILES", TILE)]).err().unwrap_or_default();
        assert!(!errors.is_empty(), "{text}");
        assert!(errors.iter().all(|e| e.starts_with("geomap")), "{errors:#?}");
    }
}

#[test]
fn the_key_table_and_the_reference_have_the_block() {
    let block = SCHEMA.block("geomap").unwrap();
    assert!(!block.labelled);
    for path in ["geomap.tile_url", "geomap.attribution", "geomap.max_zoom"] {
        let key = SCHEMA.key(path).unwrap();
        assert_eq!(key.forms, hclconf::Forms::V, "{path}");
        assert!(!key.secret);
        assert!(REFERENCE_DOC.contains(&format!("| `{path}` |")), "{path}");
    }
    assert!(matches!(SCHEMA.key("geomap.max_zoom").unwrap().kind, Kind::Integer { min: 0, max: 22 }));
    // The reference files of 13.16 and 13.17 hold no block `geomap`.
    for profile in [Profile::Local, Profile::Distributed] {
        assert!(!render_reference(ReferenceFormat::Hcl, profile).contains("geomap"));
    }
}
