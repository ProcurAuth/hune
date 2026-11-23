//! The panel types `state_timeline` and `status_history` (4.33b): the fixtures and their canonical form, the
//! boundaries of the options, one refusal for each error, the rules of the legend and of the tooltip, and the
//! refusal of an unknown type. The types join format 3 with no change of the version (4.42).

use hune_core::document::{
    AlignValues, FORMAT_VERSION, LegendMode, LegendPlacement, PanelData, PanelOptions, PanelType, ShowValues,
    TooltipMode, TooltipSort, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const TIMELINE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/state_timeline.json");
const HISTORY: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/status_history.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const TEXT_PANEL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/text_panel.json");

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

fn timeline() -> Value {
    serde_json::from_slice(TIMELINE).unwrap()
}

fn history() -> Value {
    serde_json::from_slice(HISTORY).unwrap()
}

/// The invalid files are small documents: the panel 0 is a `state_timeline`, the panel 1 is a `status_history`.
const T: &str = "/sections/0/panels/0";
const H: &str = "/sections/0/panels/1";

/// Each file of `invalid/f30_*.json` has one error: its name, its code and its pointer (4.33b).
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("st_row_height_9", ErrorCode::OutOfRange, "/sections/0/panels/0/options/row_height"),
    ("st_row_height_101", ErrorCode::OutOfRange, "/sections/0/panels/0/options/row_height"),
    ("st_line_width_11", ErrorCode::OutOfRange, "/sections/0/panels/0/options/line_width"),
    ("st_fill_opacity_101", ErrorCode::OutOfRange, "/sections/0/panels/0/options/fill_opacity"),
    ("st_row_height_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/options/row_height"),
    ("st_merge_values_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("st_show_values_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/show_values"),
    ("st_align_values_bad", ErrorCode::BadFormat, "/sections/0/panels/0/options/align_values"),
    ("st_span_nulls_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("st_show_labels_absent", ErrorCode::MissingMember, "/sections/0/panels/0/options"),
    ("st_col_width_in_timeline", ErrorCode::UnknownMember, "/sections/0/panels/0/options/col_width"),
    ("sh_merge_values_in_history", ErrorCode::UnknownMember, "/sections/0/panels/1/options/merge_values"),
    ("sh_span_nulls_in_history", ErrorCode::UnknownMember, "/sections/0/panels/1/options/span_nulls"),
    ("sh_col_width_9", ErrorCode::OutOfRange, "/sections/0/panels/1/options/col_width"),
    ("sh_col_width_101", ErrorCode::OutOfRange, "/sections/0/panels/1/options/col_width"),
    ("sh_row_height_9", ErrorCode::OutOfRange, "/sections/0/panels/1/options/row_height"),
    ("sh_line_width_11", ErrorCode::OutOfRange, "/sections/0/panels/1/options/line_width"),
    ("sh_fill_opacity_101", ErrorCode::OutOfRange, "/sections/0/panels/1/options/fill_opacity"),
    ("sh_col_width_absent", ErrorCode::MissingMember, "/sections/0/panels/1/options"),
    ("options_unknown_member", ErrorCode::UnknownMember, "/sections/0/panels/0/options/extra"),
    ("options_duplicate_member", ErrorCode::DuplicateMember, "/sections/0/panels/0/options/merge_values"),
    ("legend_mode_table", ErrorCode::NotPermitted, "/sections/0/panels/0/options/legend/mode"),
    ("legend_values_not_empty", ErrorCode::NotPermitted, "/sections/0/panels/1/options/legend/values"),
    ("legend_sort_by", ErrorCode::NotPermitted, "/sections/0/panels/0/options/legend/sort_by"),
    ("legend_sort_desc", ErrorCode::NotPermitted, "/sections/0/panels/1/options/legend/sort_desc"),
    ("tooltip_sort_asc", ErrorCode::NotPermitted, "/sections/0/panels/0/options/tooltip/sort"),
    ("tooltip_hide_zeros_true", ErrorCode::NotPermitted, "/sections/0/panels/1/options/tooltip/hide_zeros"),
    ("defaults_axis_placement", ErrorCode::NotPermitted, "/sections/0/panels/0/fields/defaults/axis_placement"),
    ("override_line_width", ErrorCode::NotPermitted, "/sections/0/panels/1/fields/overrides/0/set/line_width"),
    ("type_state_timelines", ErrorCode::BadFormat, "/sections/0/panels/0/type"),
];

#[test]
fn each_refusal_gives_its_one_error() {
    for (name, code, pointer) in REFUSALS {
        let bytes =
            std::fs::read(invalid_dir().join(format!("f30_{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut e = validate(&bytes, None).unwrap_err();
        if *name == "type_state_timelines" {
            // A binary that does not know the type refuses it at `type`. Its options are not read as a known type.
            e.retain(|x| x.pointer == *pointer);
        }
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
    }
}

#[test]
fn there_are_30_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f30_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("f30_{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 30);
}

#[test]
fn the_messages_name_the_rule_and_the_value() {
    let read = |name: &str| {
        let bytes = std::fs::read(invalid_dir().join(format!("f30_{name}.json"))).unwrap();
        validate(&bytes, None).unwrap_err()
    };
    let e = read("st_row_height_9");
    assert!(e[0].message.contains("10 to 100") && e[0].message.contains('9'), "{}", e[0].message);
    let e = read("legend_mode_table");
    assert!(e[0].message.contains("list") && e[0].message.contains("table"), "{}", e[0].message);
    let e = read("tooltip_sort_asc");
    assert!(e[0].message.contains("no sort") && e[0].message.contains("asc"), "{}", e[0].message);
    let e = read("st_col_width_in_timeline");
    assert!(e[0].message.contains("col_width") && e[0].message.contains("merge_values"), "{}", e[0].message);
}

// ------------------------------------------------------------------ fixtures

#[test]
fn the_fixtures_are_valid_and_canonical() {
    for (name, bytes) in [("state_timeline", TIMELINE), ("status_history", HISTORY)] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}: the golden canonical form");
        assert_eq!(doc.format_version(), FORMAT_VERSION);
    }
    assert_eq!(FORMAT_VERSION, 3, "the two types add no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_options() {
    let doc = validate(TIMELINE, None).unwrap();
    let panels = &doc.document().sections()[0].panels;
    assert_eq!(panels.len(), 5);
    let PanelOptions::StateTimeline(first) = &panels[0].options else { panic!("not a state timeline") };
    assert_eq!(panels[0].panel_type, PanelType::StateTimeline);
    assert!(first.merge_values && !first.span_nulls && first.show_labels);
    assert_eq!((first.show_values, first.align_values), (ShowValues::Auto, AlignValues::Left));
    assert_eq!((first.row_height, first.line_width, first.fill_opacity), (90, 0, 70));
    assert_eq!(
        (first.legend.mode, first.legend.placement, first.legend.show),
        (LegendMode::List, LegendPlacement::Bottom, true)
    );
    assert!(first.legend.values.is_empty() && first.legend.sort_by.is_none() && !first.legend.sort_desc);
    assert_eq!(
        (first.tooltip.mode, first.tooltip.sort, first.tooltip.hide_zeros),
        (TooltipMode::Single, TooltipSort::None, false)
    );
    assert_eq!(panels[0].options.panel_type(), PanelType::StateTimeline);
    let PanelOptions::StateTimeline(low) = &panels[1].options else { panic!() };
    assert_eq!((low.row_height, low.line_width, low.fill_opacity), (10, 0, 0));
    assert!(!low.merge_values && low.span_nulls && !low.show_labels);
    assert_eq!((low.show_values, low.align_values), (ShowValues::Never, AlignValues::Center));
    assert_eq!(
        (low.legend.show, low.legend.placement, low.tooltip.mode),
        (false, LegendPlacement::Right, TooltipMode::Multi)
    );
    let PanelOptions::StateTimeline(high) = &panels[2].options else { panic!() };
    assert_eq!((high.row_height, high.line_width, high.fill_opacity), (100, 10, 100));
    assert_eq!(
        (high.show_values, high.align_values, high.tooltip.mode),
        (ShowValues::Always, AlignValues::Right, TooltipMode::Hidden)
    );
    assert!(matches!(panels[3].data, PanelData::Reuse { panel: 1, with_transforms: false }));
    assert_eq!(panels[4].repeat.as_ref().map(|r| r.variable.as_str()), Some("host"));

    let doc = validate(HISTORY, None).unwrap();
    let panels = &doc.document().sections()[0].panels;
    let PanelOptions::StatusHistory(first) = &panels[0].options else { panic!("not a status history") };
    assert_eq!(panels[0].panel_type, PanelType::StatusHistory);
    assert_eq!((first.row_height, first.col_width, first.line_width, first.fill_opacity), (90, 90, 1, 70));
    let PanelOptions::StatusHistory(narrow) = &panels[1].options else { panic!() };
    assert_eq!((narrow.row_height, narrow.col_width, narrow.line_width, narrow.fill_opacity), (10, 10, 0, 0));
    let PanelOptions::StatusHistory(wide) = &panels[2].options else { panic!() };
    assert_eq!((wide.row_height, wide.col_width, wide.line_width, wide.fill_opacity), (100, 100, 10, 100));
    assert_eq!(PanelType::ALL.len(), 15);
    assert_eq!(PanelType::StateTimeline.to_string(), "state_timeline");
    assert_eq!(PanelType::StatusHistory.to_string(), "status_history");
}

#[test]
fn the_boundaries_are_valid_and_one_beyond_is_not() {
    for (member, low, high) in [("row_height", 10, 100), ("line_width", 0, 10), ("fill_opacity", 0, 100)] {
        for (doc, i) in [(timeline(), 0), (history(), 0)] {
            for ok in [low, high] {
                let mut v = doc.clone();
                v["sections"][0]["panels"][i]["options"][member] = json!(ok);
                assert!(errors_of(&v).is_empty(), "{member} {ok}");
            }
            for bad in [low - 1, high + 1] {
                let mut v = doc.clone();
                v["sections"][0]["panels"][i]["options"][member] = json!(bad);
                let p = format!("/sections/0/panels/{i}/options/{member}");
                assert_eq!(one_error(&v), (ErrorCode::OutOfRange, p), "{member} {bad}");
            }
        }
    }
    for ok in [10, 100] {
        let mut v = history();
        v["sections"][0]["panels"][0]["options"]["col_width"] = json!(ok);
        assert!(errors_of(&v).is_empty(), "col_width {ok}");
    }
    for bad in [9, 101] {
        let mut v = history();
        v["sections"][0]["panels"][0]["options"]["col_width"] = json!(bad);
        assert_eq!(
            one_error(&v),
            (ErrorCode::OutOfRange, format!("{H}/options/col_width").replace("panels/1", "panels/0"))
        );
    }
}

#[test]
fn the_percent_members_are_integers() {
    for text in ["90.0", "9e1", "90.5"] {
        let mut s = String::from_utf8(TIMELINE.to_vec()).unwrap();
        s = s.replacen("\"row_height\": 90,", &format!("\"row_height\": {text},"), 1);
        let e = validate(s.as_bytes(), None).unwrap_err();
        assert_eq!(e.len(), 1, "{text}: {e:#?}");
        assert_eq!(
            (e[0].code, e[0].pointer.as_str()),
            (ErrorCode::WrongType, "/sections/0/panels/0/options/row_height")
        );
    }
}

// ------------------------------------------------------------ canonical form

#[test]
fn the_members_of_options_come_in_the_order_of_4_33b() {
    let text = std::str::from_utf8(validate(TIMELINE, None).unwrap().bytes()).unwrap().to_owned();
    let options = "          \"options\": {\n            \"merge_values\": true,\n            \"show_values\": \"auto\",\n            \"align_values\": \"left\",\n            \"row_height\": 90,\n            \"line_width\": 0,\n            \"fill_opacity\": 70,\n            \"span_nulls\": false,\n            \"show_labels\": true,\n            \"legend\": {\n              \"show\": true,\n              \"placement\": \"bottom\",\n              \"mode\": \"list\",\n              \"values\": []\n            },\n            \"tooltip\": {\n              \"mode\": \"single\"\n            }\n          },";
    assert!(text.contains(options), "{text}");
    let text = std::str::from_utf8(validate(HISTORY, None).unwrap().bytes()).unwrap().to_owned();
    let options = "          \"options\": {\n            \"show_values\": \"auto\",\n            \"row_height\": 90,\n            \"col_width\": 90,\n            \"line_width\": 1,\n            \"fill_opacity\": 70,\n            \"show_labels\": true,\n            \"legend\": {\n              \"show\": true,\n              \"placement\": \"bottom\",\n              \"mode\": \"list\",\n              \"values\": []\n            },\n            \"tooltip\": {\n              \"mode\": \"single\"\n            }\n          },";
    assert!(text.contains(options), "{text}");
}

#[test]
fn the_members_of_the_input_in_any_order_give_the_canonical_bytes() {
    let mut v = timeline();
    v["sections"][0]["panels"][0]["options"] = json!({
        "tooltip": {"mode": "single"},
        "legend": {"values": [], "mode": "list", "placement": "bottom", "show": true},
        "show_labels": true, "span_nulls": false, "fill_opacity": 70, "line_width": 0, "row_height": 90,
        "align_values": "left", "show_values": "auto", "merge_values": true
    });
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(out.bytes(), TIMELINE);
    let mut v = history();
    v["sections"][0]["panels"][0]["options"] = json!({
        "tooltip": {"hide_zeros": false, "sort": "none", "mode": "single"},
        "legend": {"values": [], "mode": "list", "placement": "bottom", "show": true},
        "show_labels": true, "fill_opacity": 70, "line_width": 1, "col_width": 90, "row_height": 90,
        "show_values": "auto"
    });
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(
        out.bytes(),
        HISTORY,
        "sort none and hide_zeros false are valid, and the canonical form leaves them out"
    );
}

#[test]
fn a_document_with_no_such_panel_keeps_its_bytes() {
    for (name, bytes) in [("minimal", MINIMAL), ("all_types", ALL_TYPES), ("text_panel", TEXT_PANEL)] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}");
    }
}

// ------------------------------------------------------ rules across members

#[test]
fn the_legend_rules_give_not_permitted_at_each_member() {
    let mut v = timeline();
    let legend = &mut v["sections"][0]["panels"][0]["options"]["legend"];
    *legend = json!({"show": true, "placement": "bottom", "mode": "table", "values": ["last"], "sort_by": "name", "sort_desc": true});
    let got: Vec<(ErrorCode, String)> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
    let base = "/sections/0/panels/0/options/legend";
    assert_eq!(
        got,
        [
            (ErrorCode::NotPermitted, format!("{base}/mode")),
            (ErrorCode::NotPermitted, format!("{base}/values")),
            (ErrorCode::NotPermitted, format!("{base}/sort_by")),
            (ErrorCode::NotPermitted, format!("{base}/sort_desc")),
        ]
    );
    // Show and placement work as in 4.26.
    let mut v = history();
    v["sections"][0]["panels"][0]["options"]["legend"] =
        json!({"show": false, "placement": "right", "mode": "list", "values": []});
    assert!(errors_of(&v).is_empty());
    v["sections"][0]["panels"][0]["options"]["legend"]["placement"] = json!("top");
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, "/sections/0/panels/0/options/legend/placement".to_owned()));
    // A value that is not a legend value is `bad_format` and not `not_permitted`.
    v["sections"][0]["panels"][0]["options"]["legend"] =
        json!({"show": true, "placement": "bottom", "mode": "list", "values": ["bogus"]});
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, "/sections/0/panels/0/options/legend/values/0".to_owned()));
}

#[test]
fn the_tooltip_rules_give_not_permitted_at_each_member() {
    let mut v = history();
    v["sections"][0]["panels"][0]["options"]["tooltip"] = json!({"mode": "multi", "sort": "desc", "hide_zeros": true});
    let got: Vec<(ErrorCode, String)> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
    let base = "/sections/0/panels/0/options/tooltip";
    assert_eq!(
        got,
        [(ErrorCode::NotPermitted, format!("{base}/sort")), (ErrorCode::NotPermitted, format!("{base}/hide_zeros"))]
    );
    for mode in ["single", "multi", "hidden"] {
        v["sections"][0]["panels"][0]["options"]["tooltip"] = json!({"mode": mode});
        assert!(errors_of(&v).is_empty(), "{mode}");
    }
    v["sections"][0]["panels"][0]["options"]["tooltip"] = json!({"mode": "pinned"});
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{base}/mode")));
    v["sections"][0]["panels"][0]["options"]["tooltip"] = json!({"mode": "single", "sort": "up"});
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{base}/sort")));
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
        for doc in [timeline(), history()] {
            let mut v = doc;
            v["sections"][0]["panels"][0]["fields"]["defaults"][member] = value.clone();
            let e = errors_of(&v);
            assert!(
                e.iter().any(|x| x.code == ErrorCode::NotPermitted
                    && x.pointer == format!("/sections/0/panels/0/fields/defaults/{member}")),
                "{member}: {e:#?}"
            );
            let mut v = timeline();
            v["sections"][0]["panels"][0]["fields"]["overrides"] =
                json!([{"match": {"by": "name", "value": "state"}, "set": {member: value.clone()}}]);
            let e = errors_of(&v);
            assert!(
                e.iter().any(|x| x.code == ErrorCode::NotPermitted
                    && x.pointer == format!("/sections/0/panels/0/fields/overrides/0/set/{member}")),
                "{member}: {e:#?}"
            );
        }
    }
}

#[test]
fn the_field_members_of_4_18_are_valid_for_a_row() {
    for doc in [timeline(), history()] {
        let mut v = doc;
        v["sections"][0]["panels"][0]["fields"]["defaults"] = json!({
            "unit": "percent", "decimals": 2, "min": 0, "max": 100,
            "color": {"mode": "thresholds", "fixed": null},
            "thresholds": {"mode": "absolute", "steps": [{"value": null, "color": "green"}, {"value": 80, "color": "red"}]},
            "mappings": [{"kind": "value", "value": "1", "text": "One", "color": "blue"}],
            "links": [{"title": "Docs", "url": "https://docs.example", "target_blank": true}],
            "display_name": "State", "no_value": "-",
            "hide_from": {"legend": true, "tooltip": true, "viz": false}
        });
        assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    }
}

#[test]
fn the_new_types_have_no_reduce_and_no_limit_of_their_own() {
    let mut v = timeline();
    v["sections"][0]["panels"][0]["options"]["reduce"] = json!({"calc": "last", "fields": null, "values": false});
    assert_eq!(one_error(&v), (ErrorCode::UnknownMember, format!("{T}/options/reduce")));
}

#[test]
fn the_type_changes_the_options_it_needs() {
    // The type of a panel changes with its options kept: the other type refuses the members it does not know.
    let mut v = timeline();
    v["sections"][0]["panels"][0]["type"] = json!("status_history");
    let got: Vec<(ErrorCode, String)> = errors_of(&v).into_iter().map(|e| (e.code, e.pointer)).collect();
    assert!(got.contains(&(ErrorCode::MissingMember, format!("{T}/options"))), "{got:#?}");
    assert!(got.contains(&(ErrorCode::UnknownMember, format!("{T}/options/merge_values"))), "{got:#?}");
    assert!(got.contains(&(ErrorCode::UnknownMember, format!("{T}/options/span_nulls"))), "{got:#?}");
    assert!(got.contains(&(ErrorCode::UnknownMember, format!("{T}/options/align_values"))), "{got:#?}");
}

#[test]
fn a_binary_that_does_not_know_the_type_refuses_it_at_type() {
    for name in ["state_timelines", "status_histories", "timeline", ""] {
        let mut v = timeline();
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
    let mut v = timeline();
    v["sections"][0]["panels"][1]["id"] = json!(1);
    assert_eq!(one_error(&v), (ErrorCode::NotUnique, "/sections/0/panels/1/id".to_owned()));
    // A reuse of a panel of the new type is valid, and a reuse of a reuse is not.
    let mut v = timeline();
    v["sections"][0]["panels"][1]["data"] = json!({"reuse": {"panel": 4, "with_transforms": false}});
    assert_eq!(one_error(&v), (ErrorCode::BadReference, "/sections/0/panels/1/data/reuse/panel".to_owned()));
    let mut v = timeline();
    v["sections"][0]["panels"][1]["data"] = json!({"reuse": {"panel": 1, "with_transforms": true}});
    assert!(errors_of(&v).is_empty());
    // The two types have the members `transforms`, `fields` and `data` of a time series.
    let mut v = history();
    v["sections"][0]["panels"][0].as_object_mut().unwrap().remove("data");
    assert_eq!(one_error(&v), (ErrorCode::MissingMember, "/sections/0/panels/0".to_owned()));
}

#[test]
fn the_documents_of_this_file_hold_every_member_of_the_table() {
    // A panel of the other type in the same document is valid: the two types live together.
    let mut v = timeline();
    let h = history();
    let mut panel = h["sections"][0]["panels"][0].clone();
    panel["id"] = json!(60);
    panel["grid"] = json!({"x": 0, "y": 40, "w": 24, "h": 8});
    v["sections"][0]["panels"].as_array_mut().unwrap().push(panel);
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
}
