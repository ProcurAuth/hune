// SPDX-License-Identifier: MIT OR Apache-2.0
//! The members that the second set of Grafana-parity features adds to format 3 (4.5a, 4.6, 4.15, 4.18, 4.19): the
//! fixture, one refusal for each error, the canonical form and the grammars.

use hune_core::document::{
    DashboardLink, FORMAT_VERSION, PanelData, Timezone, is_valid_zone_name, parse_time_from, parse_time_shift, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::{MAX_DASHBOARD_LINKS, MAX_LINK_TAGS, MAX_TIMEZONE_BYTES};
use serde_json::{Value, json};

const F2_MEMBERS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/f2_members.json");

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn base() -> Value {
    serde_json::from_slice(F2_MEMBERS).unwrap()
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

/// Each file of `invalid/f2_*.json` has one error: its file name, its code and its pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f2_links_not_array", ErrorCode::WrongType, "/links"),
    ("f2_links_too_many", ErrorCode::TooLarge, "/links"),
    ("f2_links_missing", ErrorCode::MissingMember, ""),
    ("f2_link_kind_missing", ErrorCode::MissingMember, "/links/0"),
    ("f2_link_kind_bad", ErrorCode::BadFormat, "/links/0/kind"),
    ("f2_link_with_tags", ErrorCode::UnknownMember, "/links/0/tags"),
    ("f2_dashboards_with_url", ErrorCode::UnknownMember, "/links/0/url"),
    ("f2_link_title_empty", ErrorCode::OutOfRange, "/links/0/title"),
    ("f2_dashboards_dropdown_title_empty", ErrorCode::OutOfRange, "/links/0/title"),
    ("f2_link_url_prefix", ErrorCode::BadFormat, "/links/0/url"),
    ("f2_link_url_too_long", ErrorCode::OutOfRange, "/links/0/url"),
    ("f2_link_title_wrong_type", ErrorCode::WrongType, "/links/0/title"),
    ("f2_dashboards_tags_too_many", ErrorCode::TooLarge, "/links/0/tags"),
    ("f2_dashboards_tags_duplicate", ErrorCode::NotUnique, "/links/0/tags/1"),
    ("f2_dashboards_tag_empty", ErrorCode::OutOfRange, "/links/0/tags/0"),
    ("f2_link_flag_missing", ErrorCode::MissingMember, "/links/0"),
    ("f2_link_flag_wrong_type", ErrorCode::WrongType, "/links/0/include_vars"),
    ("f2_dashboards_as_dropdown_missing", ErrorCode::MissingMember, "/links/0"),
    ("f2_timezone_no_slash", ErrorCode::BadFormat, "/time/timezone"),
    ("f2_timezone_blank", ErrorCode::BadFormat, "/time/timezone"),
    ("f2_timezone_empty_part", ErrorCode::BadFormat, "/time/timezone"),
    ("f2_timezone_too_long", ErrorCode::BadFormat, "/time/timezone"),
    ("f2_timezone_wrong_type", ErrorCode::WrongType, "/time/timezone"),
    ("f2_time_from_no_now", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_from"),
    ("f2_time_from_now_alone", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_from"),
    ("f2_time_from_plus", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_from"),
    ("f2_time_from_absolute", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_from"),
    ("f2_time_from_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/data/query/time_from"),
    ("f2_time_shift_sign", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_shift"),
    ("f2_time_shift_round", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/time_shift"),
    ("f2_time_shift_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/data/query/time_shift"),
    ("f2_hide_time_override_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/data/query/hide_time_override"),
    ("f2_reuse_time_from", ErrorCode::UnknownMember, "/sections/0/panels/3/data/reuse/time_from"),
    ("f2_reuse_time_shift", ErrorCode::UnknownMember, "/sections/0/panels/3/data/reuse/time_shift"),
    ("f2_reuse_hide_time_override", ErrorCode::UnknownMember, "/sections/0/panels/3/data/reuse/hide_time_override"),
    ("f2_axis_placement_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/defaults/axis_placement"),
    ("f2_axis_placement_barchart", ErrorCode::NotPermitted, "/sections/0/panels/2/fields/defaults/axis_placement"),
    ("f2_axis_placement_stat", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/defaults/axis_placement"),
    ("f2_axis_placement_null", ErrorCode::WrongType, "/sections/0/panels/0/fields/defaults/axis_placement"),
    ("f2_axis_label_empty", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/defaults/axis_label"),
    ("f2_axis_label_stat", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/defaults/axis_label"),
    ("f2_soft_min_stat", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/defaults/soft_min"),
    ("f2_soft_max_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/fields/defaults/soft_max"),
    ("f2_soft_order", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/defaults/soft_max"),
    ("f2_soft_order_override", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/overrides/0/set/soft_max"),
    ("f2_draw_in_defaults", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/draw"),
    ("f2_draw_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/overrides/0/set/draw"),
    ("f2_draw_barchart", ErrorCode::NotPermitted, "/sections/0/panels/2/fields/overrides/0/set/draw"),
    ("f2_line_width_in_defaults", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/line_width"),
    ("f2_line_width_range", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/overrides/0/set/line_width"),
    ("f2_fill_opacity_range", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/overrides/0/set/fill_opacity"),
    ("f2_fill_opacity_in_defaults", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/fill_opacity"),
    ("f2_line_dash_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/defaults/line_dash"),
    ("f2_line_dash_stat", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/defaults/line_dash"),
    ("f2_show_points_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/overrides/0/set/show_points"),
    ("f2_show_points_in_defaults", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/show_points"),
    ("f2_point_size_range", ErrorCode::OutOfRange, "/sections/0/panels/0/fields/overrides/0/set/point_size"),
    ("f2_point_size_in_defaults", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/point_size"),
    ("f2_transform_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/defaults/transform"),
    ("f2_transform_stat", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/defaults/transform"),
    ("f2_color_mode_bad", ErrorCode::BadFormat, "/sections/0/panels/0/fields/defaults/color/mode"),
    ("f2_color_continuous_with_fixed", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/color/fixed"),
    ("f2_color_palette_with_fixed", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/color/fixed"),
    ("f2_color_palette_by_name_removed", ErrorCode::BadFormat, "/sections/0/panels/0/fields/defaults/color/mode"),
];

#[test]
fn each_refusal_gives_its_one_error() {
    for (name, code, pointer) in REFUSALS {
        let bytes = std::fs::read(invalid_dir().join(format!("{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = validate(&bytes, None).unwrap_err();
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
    }
}

#[test]
fn the_removed_mode_palette_by_name_names_the_one_palette() {
    let bytes = std::fs::read(invalid_dir().join("f2_color_palette_by_name_removed.json")).unwrap();
    let e = validate(&bytes, None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(e[0].message.contains("palette_by_name") && e[0].message.contains("mode palette"), "{}", e[0].message);
}

#[test]
fn no_refusal_file_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f2_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(F2_MEMBERS, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), F2_MEMBERS);
    assert_eq!(doc.format_version(), FORMAT_VERSION);
}

#[test]
fn the_fixture_holds_each_new_member_and_value() {
    let text = std::str::from_utf8(F2_MEMBERS).unwrap();
    let mut needles: Vec<String> = [
        "\"links\"",
        "\"kind\": \"link\"",
        "\"kind\": \"dashboards\"",
        "\"as_dropdown\"",
        "\"include_vars\"",
        "\"keep_time\"",
        "\"timezone\": \"Europe/Paris\"",
        "\"time_from\"",
        "\"time_shift\"",
        "\"hide_time_override\"",
        "\"axis_placement\"",
        "\"axis_label\"",
        "\"soft_min\"",
        "\"soft_max\"",
        "\"draw\": \"bars\"",
        "\"line_width\": 2",
        "\"fill_opacity\": 30",
        "\"line_dash\": \"dot\"",
        "\"line_dash\": \"dash\"",
        "\"show_points\": \"never\"",
        "\"point_size\": 4",
        "\"transform\": \"negative_y\"",
        "\"transform\": \"none\"",
    ]
    .map(str::to_owned)
    .into();
    for mode in [
        "palette",
        "continuous_gr_yl_rd",
        "continuous_rd_yl_gr",
        "continuous_bl_yl_rd",
        "continuous_yl_rd",
        "continuous_bl_pu",
        "continuous_yl_bl",
        "continuous_blues",
        "continuous_reds",
        "continuous_greens",
        "continuous_purples",
    ] {
        needles.push(format!("\"mode\": \"{mode}\""));
    }
    for needle in needles {
        assert!(text.contains(&needle), "the fixture has no {needle}");
    }
}

#[test]
fn the_typed_document_reads_the_new_members() {
    let doc = validate(F2_MEMBERS, None).unwrap();
    let d = doc.document();
    assert_eq!(d.time.timezone, Timezone::Named("Europe/Paris".to_owned()));
    assert_eq!(d.links.len(), 3);
    assert!(matches!(&d.links[0], DashboardLink::Link { target_blank: true, .. }));
    assert!(matches!(&d.links[1], DashboardLink::Dashboards { as_dropdown: true, keep_time: true, .. }));
    let PanelData::Query(q) = &d.sections[0].panels[0].data else { panic!("not a query") };
    assert_eq!(q.time_from.as_deref(), Some("now-24h"));
    assert_eq!(q.time_shift.as_deref(), Some("1w"));
    assert!(q.hide_time_override);
}

// ------------------------------------------------------------ canonical form

#[test]
fn a_false_hide_time_override_is_not_written() {
    let mut v = base();
    v["sections"][0]["panels"][0]["data"]["query"]["hide_time_override"] = json!(false);
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert!(!std::str::from_utf8(out.bytes()).unwrap().contains("\"hide_time_override\": false"));
    assert!(matches!(&out.document().sections[0].panels[0].data, PanelData::Query(q) if !q.hide_time_override));
}

#[test]
fn absent_time_members_are_not_written() {
    let mut v = base();
    let query = v["sections"][0]["panels"][0]["data"]["query"].as_object_mut().unwrap();
    for name in ["time_from", "time_shift", "hide_time_override"] {
        query.remove(name);
    }
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(out.bytes()).unwrap();
    // Panel 2 of the fixture keeps its own members.
    assert_eq!(text.matches("\"time_from\"").count(), 1);
    assert_eq!(text.matches("\"hide_time_override\"").count(), 0);
}

#[test]
fn the_members_of_a_query_keep_their_table_order() {
    let text = std::str::from_utf8(F2_MEMBERS).unwrap();
    let at = |name: &str| text.find(name).unwrap_or_else(|| panic!("no {name}"));
    assert!(at("\"max_data_points\"") < at("\"time_from\""));
    assert!(at("\"time_from\"") < at("\"time_shift\""));
    assert!(at("\"time_shift\"") < at("\"hide_time_override\""));
}

#[test]
fn a_property_set_keeps_the_table_order_with_the_new_members() {
    let mut v = base();
    // The input order is not the canonical order.
    v["sections"][0]["panels"][0]["fields"]["overrides"] = json!([{
        "match": {"by": "name", "value": "x"},
        "set": {
            "transform": "negative_y", "point_size": 4, "show_points": "never", "line_dash": "dot", "fill_opacity": 30,
            "line_width": 2, "draw": "bars", "soft_max": 100, "soft_min": 0, "axis_label": "L", "axis_placement": "right",
            "hide_from": {"legend": false, "tooltip": false, "viz": false}, "unit": "ms"
        }
    }]);
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(out.bytes()).unwrap();
    let order = [
        "\"unit\"",
        "\"hide_from\"",
        "\"axis_placement\"",
        "\"axis_label\"",
        "\"soft_min\"",
        "\"soft_max\"",
        "\"draw\": \"bars\"",
        "\"line_width\": 2",
        "\"fill_opacity\": 30",
        "\"line_dash\": \"dot\"",
        "\"show_points\": \"never\"",
        "\"point_size\": 4",
        "\"transform\": \"negative_y\"",
    ];
    let start = text.find("\"by\": \"name\"").unwrap();
    let mut last = start;
    for key in order {
        let at = text[start..].find(key).map(|i| i + start).unwrap_or_else(|| panic!("no {key}"));
        assert!(at > last, "{key} is out of order");
        last = at;
    }
}

// ------------------------------------------------------------------- limits

#[test]
fn the_limits_are_the_values_of_4_35() {
    assert_eq!((MAX_DASHBOARD_LINKS, MAX_LINK_TAGS, MAX_TIMEZONE_BYTES), (16, 8, 64));
}

#[test]
fn sixteen_links_and_eight_tags_are_valid() {
    let mut v = base();
    let link = json!({"kind": "dashboards", "title": "a", "tags": ["1", "2", "3", "4", "5", "6", "7", "8"],
        "as_dropdown": false, "target_blank": false, "include_vars": false, "keep_time": false});
    v["links"] = json!(vec![link; 16]);
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_dashboards_link_without_title_is_valid_without_a_dropdown() {
    let mut v = base();
    v["links"] = json!([{"kind": "dashboards", "title": "", "tags": [], "as_dropdown": false,
        "target_blank": false, "include_vars": false, "keep_time": false}]);
    assert_eq!(errors_of(&v), []);
    v["links"][0]["as_dropdown"] = json!(true);
    let e = errors_of(&v);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::OutOfRange, "/links/0/title"));
}

#[test]
fn a_link_url_follows_the_prefix_rule_of_4_23() {
    let mut v = base();
    for url in ["/dashboards/d/other", "?x=1", "#a", "http://h/p", "${__url_time_range}", "${a}//b"] {
        v["links"] = json!([{"kind": "link", "title": "t", "url": url, "target_blank": false,
            "include_vars": false, "keep_time": false}]);
        assert_eq!(errors_of(&v), [], "{url}");
    }
}

// ---------------------------------------------------------------- grammars

#[test]
fn zone_names() {
    for good in ["Europe/Paris", "America/Argentina/Buenos_Aires", "Etc/GMT+5", "europe/paris", "Etc/GMT-14", "a/b"] {
        assert!(is_valid_zone_name(good), "{good}");
    }
    for bad in
        ["UTC", "GMT", "browser", "utc", "Europe/", "/Paris", "Europe/New York", "a/b/c/d", "1a/b", "a//b", "", "a/b c"]
    {
        assert!(!is_valid_zone_name(bad), "{bad}");
    }
    assert!(is_valid_zone_name(&format!("a/{}", "b".repeat(62))));
    assert!(!is_valid_zone_name(&format!("a/{}", "b".repeat(63))));
}

#[test]
fn a_zone_name_reads_as_named_and_the_fixed_values_stay() {
    for (text, want) in [
        ("browser", Timezone::Browser),
        ("utc", Timezone::Utc),
        ("Asia/Kolkata", Timezone::Named("Asia/Kolkata".to_owned())),
    ] {
        let mut v = base();
        v["time"]["timezone"] = json!(text);
        let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
        assert_eq!(doc.document().time.timezone, want);
        assert_eq!(doc.document().time.timezone.as_str(), text);
    }
}

#[test]
fn time_from_grammar() {
    for good in ["now-6h", "now/d", "now-1d/d", "now-5d/d", "now-30m", "now/w", "now/M", "now-1y/y", "now-999999999s"] {
        assert!(parse_time_from(good).is_some(), "{good}");
    }
    for bad in [
        "now",
        "6h",
        "now+1h",
        "1731100000000",
        "now-0h",
        "now-6H",
        "now/h/d",
        "now-6h/",
        "now/",
        "",
        "now-1.5h",
        "${v}",
    ] {
        assert!(parse_time_from(bad).is_none(), "{bad}");
    }
}

#[test]
fn time_shift_grammar() {
    for good in ["1d", "1w", "6h", "30m", "1M", "1y", "10s", "999999999d"] {
        assert!(parse_time_shift(good).is_some(), "{good}");
    }
    for bad in ["-1d", "1d/d", "now-1d", "0d", "1", "d", "1D", "1.5h", "1d ", "", "1ms", "01d"] {
        assert!(parse_time_shift(bad).is_none(), "{bad}");
    }
}
