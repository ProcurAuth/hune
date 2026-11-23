//! The panel types `heatmap` and `histogram` (4.33c, 4.33d): the fixtures and their canonical form, the
//! boundaries of the options, one refusal for each error, the rules across members (4.34) and the refusal of an
//! unknown type. The types join format 3 with no change of the version (4.42).

use hune_core::document::{
    BucketMode, FORMAT_VERSION, HeatColorMode, HeatColorScale, HeatScale, HeatScheme, HeatYAxis, LegendMode, PanelData,
    PanelOptions, PanelType, Stacking, TooltipMode, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const HEATMAP: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/heatmap.json");
const HISTOGRAM: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/histogram.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const TEXT_PANEL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/text_panel.json");
const TIMELINE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/state_timeline.json");

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn one_error(v: &Value) -> (ErrorCode, String) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    (e[0].code, e[0].pointer.clone())
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

fn heatmap() -> Value {
    serde_json::from_slice(HEATMAP).unwrap()
}

fn histogram() -> Value {
    serde_json::from_slice(HISTOGRAM).unwrap()
}

/// Gives the pointer of a member of the options of the panel with this index in the first section.
fn at(panel: usize, member: &str) -> String {
    format!("/sections/0/panels/{panel}/options/{member}")
}

fn options(v: &mut Value, panel: usize) -> &mut Value {
    &mut v["sections"][0]["panels"][panel]["options"]
}

/// Each file of `invalid/f31_*.json` has one error: its name, its code and its pointer (4.33c, 4.33d).
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("heatmap_type_heatmaps", ErrorCode::BadFormat, "/sections/0/panels/0/type"),
    ("histogram_type_histograms", ErrorCode::BadFormat, "/sections/0/panels/1/type"),
    ("heatmap_calculate_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("heatmap_calculate_text", ErrorCode::WrongType, "/sections/0/panels/0/options/calculate"),
    ("heatmap_x_mode_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/x_buckets/mode"),
    ("heatmap_x_count_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_count_1001", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_count_fraction", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_size_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_size_over_a_year", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_size_null", ErrorCode::OutOfRange, "/sections/0/panels/0/options/x_buckets/value"),
    ("heatmap_x_scale_member", ErrorCode::UnknownMember, "/sections/0/panels/0/options/x_buckets/scale"),
    ("heatmap_y_count_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_count_1001", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_count_fraction", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_size_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_size_null", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_scale_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/y_buckets/scale"),
    ("heatmap_log2_count_mode", ErrorCode::NotPermitted, "/sections/0/panels/0/options/y_buckets/mode"),
    ("heatmap_log10_count_mode", ErrorCode::NotPermitted, "/sections/0/panels/0/options/y_buckets/mode"),
    ("heatmap_log_value_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_log_value_17", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_log_value_fraction", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_buckets/value"),
    ("heatmap_y_min_0_log", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_min"),
    ("heatmap_y_max_negative_log", ErrorCode::OutOfRange, "/sections/0/panels/0/options/y_max"),
    ("heatmap_y_axis_right", ErrorCode::BadFormat, "/sections/0/panels/0/options/y_axis"),
    ("heatmap_y_reverse_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("heatmap_y_min_text", ErrorCode::WrongType, "/sections/0/panels/0/options/y_min"),
    ("heatmap_color_mode_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/color/mode"),
    ("heatmap_color_scheme_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/color/scheme"),
    ("heatmap_color_fill_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/color/fill"),
    ("heatmap_color_scale_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/color/scale"),
    ("heatmap_color_exponent_0", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/exponent"),
    ("heatmap_color_exponent_2_1", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/exponent"),
    ("heatmap_color_exponent_two_decimals", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/exponent"),
    ("heatmap_color_steps_1", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/steps"),
    ("heatmap_color_steps_129", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/steps"),
    ("heatmap_color_reverse_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options/color"),
    ("heatmap_color_min_equals_max", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/max"),
    ("heatmap_color_min_above_max", ErrorCode::OutOfRange, "/sections/0/panels/0/options/color/max"),
    ("heatmap_cell_gap_26", ErrorCode::OutOfRange, "/sections/0/panels/0/options/cell_gap"),
    ("heatmap_hide_le_text", ErrorCode::WrongType, "/sections/0/panels/0/options/hide_le"),
    ("heatmap_hide_ge_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("heatmap_show_legend_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("heatmap_tooltip_sort_asc", ErrorCode::NotPermitted, "/sections/0/panels/0/options/tooltip/sort"),
    ("heatmap_tooltip_hide_zeros_true", ErrorCode::NotPermitted, "/sections/0/panels/0/options/tooltip/hide_zeros"),
    ("heatmap_legend_object", ErrorCode::UnknownMember, "/sections/0/panels/0/options/legend"),
    ("heatmap_options_duplicate_member", ErrorCode::DuplicateMember, "/sections/0/panels/0/options/cell_gap"),
    ("heatmap_override_line_width", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/overrides/0/set/line_width"),
    ("histogram_bucket_count_0", ErrorCode::OutOfRange, "/sections/0/panels/1/options/bucket_count"),
    ("histogram_bucket_count_1001", ErrorCode::OutOfRange, "/sections/0/panels/1/options/bucket_count"),
    ("histogram_bucket_size_0", ErrorCode::OutOfRange, "/sections/0/panels/1/options/bucket_size"),
    ("histogram_bucket_size_negative", ErrorCode::OutOfRange, "/sections/0/panels/1/options/bucket_size"),
    ("histogram_bucket_offset_text", ErrorCode::WrongType, "/sections/0/panels/1/options/bucket_offset"),
    ("histogram_stacking_bad", ErrorCode::BadFormat, "/sections/0/panels/1/options/stacking"),
    ("histogram_line_width_11", ErrorCode::OutOfRange, "/sections/0/panels/1/options/line_width"),
    ("histogram_fill_opacity_101", ErrorCode::OutOfRange, "/sections/0/panels/1/options/fill_opacity"),
    ("histogram_combine_absent", ErrorCode::MissingMember, "/sections/0/panels/1/options"),
    ("histogram_gradient_mode_member", ErrorCode::UnknownMember, "/sections/0/panels/1/options/gradient_mode"),
    (
        "histogram_defaults_axis_placement",
        ErrorCode::NotPermitted,
        "/sections/0/panels/1/fields/defaults/axis_placement",
    ),
];

#[test]
fn each_refusal_gives_its_one_error() {
    for (name, code, pointer) in REFUSALS {
        let bytes =
            std::fs::read(invalid_dir().join(format!("f31_{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut e = validate(&bytes, None).unwrap_err();
        if name.contains("_type_") {
            // A binary that does not know the type refuses it at `type`. Its options are not read as a known type.
            e.retain(|x| x.pointer == *pointer);
        }
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
    }
}

#[test]
fn there_are_60_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f31_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("f31_{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 60);
}

#[test]
fn the_messages_name_the_rule_and_the_value() {
    let read = |name: &str| {
        let bytes = std::fs::read(invalid_dir().join(format!("f31_{name}.json"))).unwrap();
        validate(&bytes, None).unwrap_err()
    };
    let e = read("heatmap_log_value_17");
    assert!(e[0].message.contains("1 to 16") && e[0].message.contains("17"), "{}", e[0].message);
    let e = read("heatmap_log2_count_mode");
    assert!(e[0].message.contains("size") && e[0].message.contains("count"), "{}", e[0].message);
    let e = read("heatmap_color_min_equals_max");
    assert!(e[0].message.contains("greater than min") && e[0].message.contains('5'), "{}", e[0].message);
    let e = read("heatmap_tooltip_sort_asc");
    assert!(e[0].message.contains("no sort") && e[0].message.contains("asc") && e[0].message.contains("4.33c"));
    let e = read("heatmap_y_min_0_log");
    assert!(e[0].message.contains("above 0") && e[0].message.contains('0'), "{}", e[0].message);
    let e = read("histogram_bucket_count_1001");
    assert!(e[0].message.contains("1 to 1000") && e[0].message.contains("1001"), "{}", e[0].message);
}

// ------------------------------------------------------------------ fixtures

#[test]
fn the_fixtures_are_valid_and_canonical() {
    for (name, bytes) in [("heatmap", HEATMAP), ("histogram", HISTOGRAM)] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}: the golden canonical form");
        assert_eq!(doc.format_version(), FORMAT_VERSION);
    }
    assert_eq!(FORMAT_VERSION, 3, "the two types add no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_heatmap_options() {
    let doc = validate(HEATMAP, None).unwrap();
    let panels = &doc.document().sections()[0].panels;
    assert_eq!(panels.len(), 8);
    let PanelOptions::Heatmap(d) = &panels[0].options else { panic!("not a heatmap") };
    assert_eq!(panels[0].panel_type, PanelType::Heatmap);
    assert_eq!(panels[0].options.panel_type(), PanelType::Heatmap);
    assert!(!d.calculate && !d.y_reverse && d.show_legend);
    assert_eq!((d.x_buckets.mode, d.x_buckets.value), (BucketMode::Count, None));
    assert_eq!((d.y_buckets.mode, d.y_buckets.value, d.y_buckets.scale), (BucketMode::Count, None, HeatScale::Linear));
    assert_eq!(
        (d.y_axis, d.y_min, d.y_max, d.cell_gap, d.hide_le, d.hide_ge),
        (HeatYAxis::Left, None, None, 1, Some(0.0), None)
    );
    assert_eq!(
        (d.color.mode, d.color.scheme, d.color.scale),
        (HeatColorMode::Scheme, HeatScheme::Oranges, HeatColorScale::Exponential)
    );
    assert_eq!(
        (d.color.exponent, d.color.steps, d.color.reverse, d.color.min, d.color.max),
        (0.5, 64, false, None, None)
    );
    assert_eq!(d.color.fill, "dark-orange");
    assert_eq!(d.tooltip.mode, TooltipMode::Single);
    let PanelOptions::Heatmap(low) = &panels[1].options else { panic!() };
    assert_eq!((low.x_buckets.value, low.y_buckets.value), (Some(1), Some(1.0)));
    assert_eq!(
        (low.color.mode, low.color.scheme, low.color.exponent, low.color.steps),
        (HeatColorMode::Opacity, HeatScheme::Viridis, 0.1, 2)
    );
    assert_eq!(
        (low.color.min, low.color.max, low.cell_gap, low.hide_le, low.hide_ge),
        (Some(-5.0), Some(5.0), 0, Some(-1.5), Some(-0.5))
    );
    assert_eq!(
        (low.y_axis, low.y_reverse, low.y_min, low.y_max, low.show_legend),
        (HeatYAxis::Hidden, true, Some(-100.0), Some(100.0), false)
    );
    assert_eq!(low.tooltip.mode, TooltipMode::Hidden);
    let PanelOptions::Heatmap(high) = &panels[2].options else { panic!() };
    assert_eq!(
        (high.x_buckets.value, high.y_buckets.value, high.color.exponent, high.color.steps),
        (Some(1000), Some(1000.0), 2.0, 128)
    );
    assert_eq!((high.cell_gap, high.hide_le, high.hide_ge, high.color.max), (25, None, Some(1e21), Some(1e21)));
    assert_eq!(high.tooltip.mode, TooltipMode::Multi);
    let PanelOptions::Heatmap(size) = &panels[3].options else { panic!() };
    assert_eq!(
        (size.x_buckets.mode, size.x_buckets.value, size.y_buckets.value),
        (BucketMode::Size, Some(1), Some(0.0001))
    );
    let PanelOptions::Heatmap(log) = &panels[4].options else { panic!() };
    assert_eq!(
        (log.x_buckets.value, log.y_buckets.scale, log.y_buckets.value),
        (Some(31_536_000_000), HeatScale::Log2, Some(16.0))
    );
    let PanelOptions::Heatmap(log10) = &panels[5].options else { panic!() };
    assert_eq!(
        (log10.y_buckets.scale, log10.y_buckets.mode, log10.y_buckets.value),
        (HeatScale::Log10, BucketMode::Size, None)
    );
    assert!(matches!(panels[6].data, PanelData::Reuse { panel: 1, with_transforms: true }));
    assert_eq!(panels[7].repeat.as_ref().map(|r| r.variable.as_str()), Some("host"));
}

#[test]
fn the_typed_document_reads_the_histogram_options() {
    let doc = validate(HISTOGRAM, None).unwrap();
    let panels = &doc.document().sections()[0].panels;
    assert_eq!(panels.len(), 6);
    let PanelOptions::Histogram(d) = &panels[0].options else { panic!("not a histogram") };
    assert_eq!(panels[0].panel_type, PanelType::Histogram);
    assert_eq!(panels[0].options.panel_type(), PanelType::Histogram);
    assert_eq!(
        (d.bucket_count, d.bucket_size, d.bucket_offset, d.combine, d.stacking),
        (30, None, 0.0, false, Stacking::None)
    );
    assert_eq!(
        (d.line_width, d.fill_opacity, d.legend.mode, d.tooltip.mode),
        (1, 80, LegendMode::List, TooltipMode::Single)
    );
    let PanelOptions::Histogram(low) = &panels[1].options else { panic!() };
    assert_eq!((low.bucket_count, low.bucket_size, low.bucket_offset, low.combine), (1, Some(1e-9), -12.5, true));
    assert_eq!((low.stacking, low.line_width, low.fill_opacity, low.legend.show), (Stacking::Normal, 0, 0, false));
    let PanelOptions::Histogram(high) = &panels[2].options else { panic!() };
    assert_eq!(
        (high.bucket_count, high.bucket_size, high.stacking, high.line_width, high.fill_opacity),
        (1000, Some(1e21), Stacking::Percent, 10, 100)
    );
    assert_eq!(
        (high.legend.mode, high.legend.values.len(), high.legend.sort_desc, high.tooltip.hide_zeros),
        (LegendMode::Table, 3, true, true)
    );
    assert_eq!(PanelType::Heatmap.to_string(), "heatmap");
    assert_eq!(PanelType::Histogram.to_string(), "histogram");
    assert_eq!(PanelType::ALL.len(), 15);
}

// ------------------------------------------------------------ boundaries

#[test]
fn the_integer_boundaries_are_valid_and_one_beyond_is_not() {
    // (the panel, the path of the member, the lowest, the highest)
    for (panel, path, low, high) in [
        (0, "cell_gap", 0, 25),
        (0, "color/steps", 2, 128),
        (1, "bucket_count", 1, 1000),
        (1, "line_width", 0, 10),
        (1, "fill_opacity", 0, 100),
    ] {
        for ok in [low, high] {
            let mut v = heatmap();
            let mut w = histogram();
            let (doc, i) = if panel == 0 { (&mut v, 1) } else { (&mut w, 0) };
            let mut o = options(doc, i);
            let parts: Vec<&str> = path.split('/').collect();
            for p in &parts[..parts.len() - 1] {
                o = &mut o[*p];
            }
            o[parts[parts.len() - 1]] = json!(ok);
            assert!(errors_of(doc).is_empty(), "{path} {ok}: {:#?}", errors_of(doc));
        }
        for bad in [low - 1, high + 1] {
            let mut v = heatmap();
            let mut w = histogram();
            let (doc, i) = if panel == 0 { (&mut v, 1) } else { (&mut w, 0) };
            let mut o = options(doc, i);
            let parts: Vec<&str> = path.split('/').collect();
            for p in &parts[..parts.len() - 1] {
                o = &mut o[*p];
            }
            o[parts[parts.len() - 1]] = json!(bad);
            assert_eq!(one_error(doc), (ErrorCode::OutOfRange, at(i, path)), "{path} {bad}");
        }
    }
}

#[test]
fn the_bucket_values_follow_the_mode_and_the_scale() {
    // (x mode, x value, valid)
    for (mode, value, ok) in [
        ("count", json!(1), true),
        ("count", json!(1000), true),
        ("count", json!(null), true),
        ("count", json!(0), false),
        ("count", json!(1001), false),
        ("count", json!(1.5), false),
        ("count", json!(10.0), false),
        ("size", json!(1), true),
        ("size", json!(31_536_000_000_i64), true),
        ("size", json!(31_536_000_001_i64), false),
        ("size", json!(0), false),
        ("size", json!(null), false),
    ] {
        let mut v = heatmap();
        options(&mut v, 1)["x_buckets"] = json!({"mode": mode, "value": value});
        if ok {
            assert!(errors_of(&v).is_empty(), "x {mode} {value}");
        } else {
            assert_eq!(one_error(&v), (ErrorCode::OutOfRange, at(1, "x_buckets/value")), "x {mode} {value}");
        }
    }
    // (y mode, y value, y scale, valid)
    for (mode, value, scale, ok) in [
        ("count", json!(1), "linear", true),
        ("count", json!(1000), "linear", true),
        ("count", json!(null), "linear", true),
        ("count", json!(1001), "linear", false),
        ("size", json!(0.5), "linear", true),
        ("size", json!(1e-9), "linear", true),
        ("size", json!(0), "linear", false),
        ("size", json!(-1), "linear", false),
        ("size", json!(null), "linear", false),
        ("size", json!(1), "log2", true),
        ("size", json!(16), "log2", true),
        ("size", json!(null), "log10", true),
        ("size", json!(0), "log10", false),
        ("size", json!(17), "log2", false),
        ("size", json!(1.5), "log10", false),
    ] {
        let mut v = heatmap();
        options(&mut v, 1)["y_buckets"] = json!({"mode": mode, "value": value, "scale": scale});
        // The panel 1 has `y_min` and `y_max` that a log scale refuses: the fixture uses null here.
        options(&mut v, 1)["y_min"] = json!(null);
        options(&mut v, 1)["y_max"] = json!(null);
        if ok {
            assert!(errors_of(&v).is_empty(), "y {mode} {value} {scale}: {:#?}", errors_of(&v));
        } else {
            assert_eq!(one_error(&v), (ErrorCode::OutOfRange, at(1, "y_buckets/value")), "y {mode} {value} {scale}");
        }
    }
    // A value of another type is `wrong_type`.
    let mut v = heatmap();
    options(&mut v, 1)["x_buckets"] = json!({"mode": "count", "value": "10"});
    assert_eq!(one_error(&v), (ErrorCode::WrongType, at(1, "x_buckets/value")));
}

#[test]
fn the_exponent_has_one_decimal_at_most() {
    for ok in
        [json!(0.1), json!(0.5), json!(1), json!(1.5), json!(1.9), json!(2), json!(2.0), json!(0.30000000000000004)]
    {
        let mut v = heatmap();
        options(&mut v, 1)["color"]["exponent"] = ok.clone();
        assert!(errors_of(&v).is_empty(), "{ok}");
    }
    for bad in [json!(0), json!(0.09), json!(0.55), json!(2.1), json!(-1), json!(1.25)] {
        let mut v = heatmap();
        options(&mut v, 1)["color"]["exponent"] = bad.clone();
        assert_eq!(one_error(&v), (ErrorCode::OutOfRange, at(1, "color/exponent")), "{bad}");
    }
}

#[test]
fn the_numbers_and_the_null_members_are_valid() {
    let mut v = heatmap();
    let o = options(&mut v, 0);
    for (member, value) in
        [("y_min", json!(-0.5)), ("y_max", json!(1e9)), ("hide_le", json!(null)), ("hide_ge", json!(3))]
    {
        o[member] = value;
    }
    assert!(errors_of(&v).is_empty());
    let mut w = histogram();
    for (offset, size) in [(json!(-1e9), json!(1e-9)), (json!(0.25), json!(null)), (json!(1e21), json!(1e21))] {
        options(&mut w, 0)["bucket_offset"] = offset;
        options(&mut w, 0)["bucket_size"] = size;
        assert!(errors_of(&w).is_empty());
    }
}

// ------------------------------------------------------------ canonical form

#[test]
fn the_members_of_options_come_in_the_order_of_the_tables() {
    let text = std::str::from_utf8(validate(HEATMAP, None).unwrap().bytes()).unwrap().to_owned();
    let options = "          \"options\": {\n            \"calculate\": false,\n            \"x_buckets\": {\n              \"mode\": \"count\",\n              \"value\": null\n            },\n            \"y_buckets\": {\n              \"mode\": \"count\",\n              \"value\": null,\n              \"scale\": \"linear\"\n            },\n            \"y_axis\": \"left\",\n            \"y_reverse\": false,\n            \"y_min\": null,\n            \"y_max\": null,\n            \"color\": {\n              \"mode\": \"scheme\",\n              \"scheme\": \"oranges\",\n              \"fill\": \"dark-orange\",\n              \"scale\": \"exponential\",\n              \"exponent\": 0.5,\n              \"steps\": 64,\n              \"reverse\": false,\n              \"min\": null,\n              \"max\": null\n            },\n            \"cell_gap\": 1,\n            \"hide_le\": 0,\n            \"hide_ge\": null,\n            \"show_legend\": true,\n            \"tooltip\": {\n              \"mode\": \"single\"\n            }\n          },";
    assert!(text.contains(options), "{text}");
    let text = std::str::from_utf8(validate(HISTOGRAM, None).unwrap().bytes()).unwrap().to_owned();
    let options = "          \"options\": {\n            \"bucket_count\": 30,\n            \"bucket_size\": null,\n            \"bucket_offset\": 0,\n            \"combine\": false,\n            \"stacking\": \"none\",\n            \"line_width\": 1,\n            \"fill_opacity\": 80,\n            \"legend\": {\n              \"show\": true,\n              \"placement\": \"bottom\",\n              \"mode\": \"list\",\n              \"values\": []\n            },\n            \"tooltip\": {\n              \"mode\": \"single\"\n            }\n          },";
    assert!(text.contains(options), "{text}");
}

#[test]
fn the_members_of_the_input_in_any_order_give_the_canonical_bytes() {
    let mut v = heatmap();
    options(&mut v, 0).as_object_mut().unwrap().clear();
    *options(&mut v, 0) = json!({
        "tooltip": {"hide_zeros": false, "sort": "none", "mode": "single"},
        "show_legend": true, "hide_ge": null, "hide_le": 0, "cell_gap": 1,
        "color": {"max": null, "min": null, "reverse": false, "steps": 64, "exponent": 0.50, "scale": "exponential",
                  "fill": "dark-orange", "scheme": "oranges", "mode": "scheme"},
        "y_max": null, "y_min": null, "y_reverse": false, "y_axis": "left",
        "y_buckets": {"scale": "linear", "value": null, "mode": "count"},
        "x_buckets": {"value": null, "mode": "count"}, "calculate": false
    });
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(
        out.bytes(),
        HEATMAP,
        "sort none and hide_zeros false are valid, and the canonical form leaves them out"
    );
    let mut w = histogram();
    *options(&mut w, 0) = json!({
        "tooltip": {"sort": "none", "mode": "single"},
        "legend": {"values": [], "mode": "list", "placement": "bottom", "show": true},
        "fill_opacity": 80, "line_width": 1, "stacking": "none", "combine": false, "bucket_offset": 0.0,
        "bucket_size": null, "bucket_count": 30
    });
    let out = validate(&serde_json::to_vec(&w).unwrap(), None).unwrap();
    assert_eq!(out.bytes(), HISTOGRAM);
}

#[test]
fn a_document_with_no_such_panel_keeps_its_bytes() {
    for (name, bytes) in
        [("minimal", MINIMAL), ("all_types", ALL_TYPES), ("text_panel", TEXT_PANEL), ("timeline", TIMELINE)]
    {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}");
    }
}

// ------------------------------------------------------ rules across members

#[test]
fn a_log_scale_needs_the_mode_size_and_a_positive_axis() {
    for scale in ["log2", "log10"] {
        let mut v = heatmap();
        options(&mut v, 1)["y_buckets"] = json!({"mode": "count", "value": null, "scale": scale});
        let got: Vec<_> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
        // The `y_min` and `y_max` of the panel 1 are not above 0 with the fixture values -100 and 100.
        assert_eq!(
            got,
            [(ErrorCode::NotPermitted, at(1, "y_buckets/mode")), (ErrorCode::OutOfRange, at(1, "y_min"))],
            "{scale}"
        );
        let mut v = heatmap();
        options(&mut v, 1)["y_buckets"] = json!({"mode": "size", "value": 1, "scale": scale});
        options(&mut v, 1)["y_min"] = json!(1e-9);
        options(&mut v, 1)["y_max"] = json!(0);
        assert_eq!(one_error(&v), (ErrorCode::OutOfRange, at(1, "y_max")), "{scale}");
        options(&mut v, 1)["y_max"] = json!(1e-3);
        assert!(errors_of(&v).is_empty());
    }
    // A linear scale accepts a negative axis.
    let mut v = heatmap();
    options(&mut v, 0)["y_min"] = json!(-10);
    options(&mut v, 0)["y_max"] = json!(0);
    assert!(errors_of(&v).is_empty());
}

#[test]
fn the_colour_range_needs_min_below_max() {
    for (lo, hi, ok) in [
        (json!(0), json!(1), true),
        (json!(-1), json!(-0.5), true),
        (json!(1), json!(1), false),
        (json!(2), json!(1), false),
    ] {
        let mut v = heatmap();
        options(&mut v, 0)["color"]["min"] = lo.clone();
        options(&mut v, 0)["color"]["max"] = hi.clone();
        if ok {
            assert!(errors_of(&v).is_empty(), "{lo} {hi}");
        } else {
            assert_eq!(one_error(&v), (ErrorCode::OutOfRange, at(0, "color/max")), "{lo} {hi}");
        }
    }
    // One end alone is valid, and a `null` is valid.
    let mut v = heatmap();
    options(&mut v, 0)["color"]["min"] = json!(5);
    assert!(errors_of(&v).is_empty());
}

#[test]
fn the_tooltip_of_a_heatmap_has_no_sort_and_no_hide_zeros() {
    let mut v = heatmap();
    options(&mut v, 0)["tooltip"] = json!({"mode": "multi", "sort": "desc", "hide_zeros": true});
    let got: Vec<_> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
    assert_eq!(
        got,
        [(ErrorCode::NotPermitted, at(0, "tooltip/sort")), (ErrorCode::NotPermitted, at(0, "tooltip/hide_zeros"))]
    );
    for mode in ["single", "multi", "hidden"] {
        options(&mut v, 0)["tooltip"] = json!({"mode": mode});
        assert!(errors_of(&v).is_empty(), "{mode}");
    }
    options(&mut v, 0)["tooltip"] = json!({"mode": "pinned"});
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, at(0, "tooltip/mode")));
}

#[test]
fn the_legend_and_the_tooltip_of_a_histogram_have_all_their_members() {
    let mut v = histogram();
    options(&mut v, 2)["legend"] = json!({"show": true, "placement": "right", "mode": "table", "values": ["last", "max"], "sort_by": "last", "sort_desc": true});
    options(&mut v, 2)["tooltip"] = json!({"mode": "multi", "sort": "asc", "hide_zeros": true});
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    options(&mut v, 2)["legend"]["values"] = json!(["last", "last"]);
    assert_eq!(one_error(&v), (ErrorCode::NotUnique, at(2, "legend/values/1")));
}

#[test]
fn the_heatmap_has_a_member_show_legend_and_no_legend_object() {
    let mut v = heatmap();
    options(&mut v, 0)["legend"] = json!({"show": true, "placement": "bottom", "mode": "list", "values": []});
    assert_eq!(one_error(&v), (ErrorCode::UnknownMember, at(0, "legend")));
    let mut v = heatmap();
    options(&mut v, 0).as_object_mut().unwrap().remove("show_legend");
    assert_eq!(one_error(&v), (ErrorCode::MissingMember, "/sections/0/panels/0/options".to_owned()));
}

#[test]
fn the_style_members_of_a_series_are_not_permitted_in_a_property_set() {
    for member in [
        "draw",
        "line_width",
        "fill_opacity",
        "show_points",
        "point_size",
        "line_dash",
        "transform",
        "axis_placement",
        "axis_label",
        "soft_min",
        "soft_max",
    ] {
        let value = match member {
            "draw" => json!("lines"),
            "line_width" | "fill_opacity" | "point_size" => json!(1),
            "show_points" => json!("auto"),
            "line_dash" => json!("solid"),
            "transform" => json!("negative_y"),
            "axis_placement" => json!("left"),
            "axis_label" => json!("x"),
            _ => json!(1),
        };
        for (doc, i) in [(heatmap(), 0), (histogram(), 0)] {
            let mut v = doc;
            v["sections"][0]["panels"][i]["fields"]["defaults"][member] = value.clone();
            let e = errors_of(&v);
            assert!(
                e.iter().any(|x| x.code == ErrorCode::NotPermitted
                    && x.pointer == format!("/sections/0/panels/{i}/fields/defaults/{member}")),
                "{member}: {e:#?}"
            );
            let mut v = heatmap();
            v["sections"][0]["panels"][i]["fields"]["overrides"] =
                json!([{"match": {"by": "name", "value": "latency"}, "set": {member: value.clone()}}]);
            let e = errors_of(&v);
            assert!(
                e.iter().any(|x| x.code == ErrorCode::NotPermitted
                    && x.pointer == format!("/sections/0/panels/{i}/fields/overrides/0/set/{member}")),
                "{member}: {e:#?}"
            );
        }
    }
}

#[test]
fn the_field_members_of_4_18_are_valid_for_the_two_types() {
    for doc in [heatmap(), histogram()] {
        let mut v = doc;
        v["sections"][0]["panels"][0]["fields"]["defaults"] = json!({
            "unit": "percent", "decimals": 2, "min": 0, "max": 100,
            "color": {"mode": "thresholds", "fixed": null},
            "thresholds": {"mode": "absolute", "steps": [{"value": null, "color": "green"}, {"value": 80, "color": "red"}]},
            "mappings": [{"kind": "value", "value": "1", "text": "One", "color": "blue"}],
            "links": [{"title": "Docs", "url": "https://docs.example", "target_blank": true}],
            "display_name": "Count", "no_value": "-",
            "hide_from": {"legend": true, "tooltip": true, "viz": false}
        });
        assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    }
}

#[test]
fn the_types_have_no_reduce_and_the_options_of_the_other_type_are_refused() {
    let mut v = heatmap();
    options(&mut v, 0)["reduce"] = json!({"calc": "last", "fields": null, "values": false});
    assert_eq!(one_error(&v), (ErrorCode::UnknownMember, at(0, "reduce")));
    // The type changes with the options kept: the other type refuses the members it does not know.
    let mut v = heatmap();
    v["sections"][0]["panels"][0]["type"] = json!("histogram");
    let got: Vec<(ErrorCode, String)> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
    assert!(got.contains(&(ErrorCode::MissingMember, "/sections/0/panels/0/options".to_owned())), "{got:#?}");
    assert!(got.contains(&(ErrorCode::UnknownMember, at(0, "calculate"))), "{got:#?}");
    assert!(got.contains(&(ErrorCode::UnknownMember, at(0, "x_buckets"))), "{got:#?}");
}

#[test]
fn a_binary_that_does_not_know_the_type_refuses_it_at_type() {
    for name in ["heatmaps", "histograms", "heat_map", "bucket", ""] {
        let mut v = heatmap();
        v["sections"][0]["panels"][0]["type"] = json!(name);
        let e = errors_of(&v);
        assert!(
            e.iter().any(|x| x.code == ErrorCode::BadFormat && x.pointer == "/sections/0/panels/0/type"),
            "{name}: {e:#?}"
        );
        assert!(e.iter().all(|x| x.pointer.starts_with("/sections/0/panels/0")), "{name}: no error in another panel");
    }
}

#[test]
fn the_ids_the_places_and_the_reuse_follow_the_common_rules() {
    let mut v = heatmap();
    v["sections"][0]["panels"][1]["id"] = json!(1);
    assert_eq!(one_error(&v), (ErrorCode::NotUnique, "/sections/0/panels/1/id".to_owned()));
    // A reuse of a panel of the new type is valid, and a reuse of a reuse is not.
    let mut v = heatmap();
    v["sections"][0]["panels"][1]["data"] = json!({"reuse": {"panel": 7, "with_transforms": false}});
    assert_eq!(one_error(&v), (ErrorCode::BadReference, "/sections/0/panels/1/data/reuse/panel".to_owned()));
    let mut v = heatmap();
    v["sections"][0]["panels"][1]["data"] = json!({"reuse": {"panel": 1, "with_transforms": true}});
    assert!(errors_of(&v).is_empty());
    // A histogram can reuse a heatmap and the other way round: the frame is the frame of a query.
    let mut v = histogram();
    let h = heatmap();
    let panel = h["sections"][0]["panels"][0].clone();
    v["sections"][0]["panels"].as_array_mut().unwrap().push(panel);
    v["sections"][0]["panels"][6]["id"] = json!(60);
    v["sections"][0]["panels"][6]["grid"] = json!({"x": 0, "y": 40, "w": 12, "h": 8});
    v["sections"][0]["panels"][6]["data"] = json!({"reuse": {"panel": 1, "with_transforms": false}});
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    // The two types have the members `transforms`, `fields` and `data` of a time series.
    let mut v = histogram();
    v["sections"][0]["panels"][0].as_object_mut().unwrap().remove("data");
    assert_eq!(one_error(&v), (ErrorCode::MissingMember, "/sections/0/panels/0".to_owned()));
}

#[test]
fn the_two_types_can_have_extra_queries() {
    // 4.15a: the two types are not in the list of the types that have one query only.
    for (doc, panel) in [(heatmap(), 0), (histogram(), 0)] {
        let mut v = doc;
        v["sections"][0]["panels"][panel]["data"]["query"]["extra_queries"] = json!([
            {"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT now() AS time, 2 AS value"]}
        ]);
        let e = errors_of(&v);
        assert!(e.iter().all(|x| x.code != ErrorCode::NotPermitted), "{e:#?}");
    }
}

#[test]
fn a_panel_of_the_other_types_lives_with_the_new_types() {
    let mut v = heatmap();
    let h = histogram();
    let mut panel = h["sections"][0]["panels"][0].clone();
    panel["id"] = json!(60);
    panel["grid"] = json!({"x": 0, "y": 40, "w": 24, "h": 8});
    v["sections"][0]["panels"].as_array_mut().unwrap().push(panel);
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
}
