// SPDX-License-Identifier: MIT OR Apache-2.0
//! The member `repeat` of a panel and of a section (4.13a): the fixture, one refusal for each error, the
//! canonical form, the rules across members (4.34) and the limits (4.35). The member joins format 3 with
//! no change of the version (4.42).

use hune_core::document::{FORMAT_VERSION, PanelRepeat, RepeatDirection, SectionRepeat, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::{MAX_REPEAT_PER_DASHBOARD, MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/repeats.json");
const PLACEMENTS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/placements.json");
const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const F_MEMBERS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/f_members.json");
const DOC_FORMAT: &str = include_str!("../../../docs/02-format-and-storage.md");

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

fn one_error(v: &Value) -> (ErrorCode, String) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    (e[0].code, e[0].pointer.clone())
}

/// Each file of `invalid/f23_*.json` has one error: its file name, its code and its pointer (4.34).
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f23_variable_unknown", ErrorCode::BadReference, "/sections/0/panels/0/repeat/variable"),
    ("f23_variable_text", ErrorCode::NotPermitted, "/sections/0/panels/0/repeat/variable"),
    ("f23_variable_not_multi", ErrorCode::NotPermitted, "/sections/0/panels/0/repeat/variable"),
    ("f23_direction_bad", ErrorCode::BadFormat, "/sections/0/panels/0/repeat/direction"),
    ("f23_direction_missing", ErrorCode::MissingMember, "/sections/0/panels/0/repeat"),
    ("f23_max_per_row_with_v", ErrorCode::NotPermitted, "/sections/0/panels/1/repeat/max_per_row"),
    ("f23_max_per_row_zero", ErrorCode::OutOfRange, "/sections/0/panels/0/repeat/max_per_row"),
    ("f23_max_per_row_25", ErrorCode::OutOfRange, "/sections/0/panels/0/repeat/max_per_row"),
    ("f23_max_per_row_wrong_type", ErrorCode::WrongType, "/sections/0/panels/0/repeat/max_per_row"),
    ("f23_repeat_unknown_member", ErrorCode::UnknownMember, "/sections/0/panels/0/repeat/gap"),
    ("f23_repeat_duplicate_member", ErrorCode::DuplicateMember, "/sections/0/panels/0/repeat/direction"),
    ("f23_panel_reuse_with_repeat", ErrorCode::NotPermitted, "/sections/2/panels/2/repeat"),
    ("f23_reuse_of_repeat_panel", ErrorCode::BadReference, "/sections/2/panels/2/data/reuse/panel"),
    ("f23_reuse_of_panel_in_repeated_section", ErrorCode::BadReference, "/sections/2/panels/2/data/reuse/panel"),
    ("f23_h_shares_row", ErrorCode::Overlap, "/sections/0/panels/0/repeat/direction"),
    ("f23_section_title_null", ErrorCode::NotPermitted, "/sections/0/repeat"),
    ("f23_section_with_repeating_panel", ErrorCode::NotPermitted, "/sections/1/panels/1/repeat"),
    ("f23_section_with_reuse_panel", ErrorCode::NotPermitted, "/sections/1/panels/1/data/reuse"),
    ("f23_section_with_placement", ErrorCode::NotPermitted, "/variables/3/placement/section"),
    ("f23_section_variable_unknown", ErrorCode::BadReference, "/sections/1/repeat/variable"),
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
fn there_are_20_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f23_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 20);
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE);
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3, "the repeat adds no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_repeats() {
    let doc = validate(FIXTURE, None).unwrap();
    let sections = doc.document().sections();
    let panels = &sections[0].panels;
    assert_eq!(
        panels[0].repeat,
        Some(PanelRepeat { variable: "host".to_owned(), direction: RepeatDirection::H, max_per_row: Some(3) })
    );
    assert_eq!(
        panels[1].repeat,
        Some(PanelRepeat { variable: "region".to_owned(), direction: RepeatDirection::V, max_per_row: None })
    );
    assert!(panels[2].repeat.is_none() && panels[3].repeat.is_none());
    assert_eq!(sections[1].repeat, Some(SectionRepeat { variable: "host".to_owned() }));
    assert!(sections[0].repeat.is_none() && sections[2].repeat.is_none());
    assert_eq!(sections[2].panels[0].repeat.as_ref().map(|r| r.max_per_row), Some(None));
}

// ---------------------------------------------------------- canonical form

#[test]
fn repeat_is_the_last_member_of_a_panel_and_of_a_section() {
    let text = std::str::from_utf8(validate(FIXTURE, None).unwrap().bytes()).unwrap().to_owned();
    // The panel: `links` and then `repeat`, and `max_per_row` last in the object `repeat`.
    let panel = "          \"links\": [],\n          \"repeat\": {\n            \"variable\": \"host\",\n            \"direction\": \"h\",\n            \"max_per_row\": 3\n          }\n        },";
    assert!(text.contains(panel), "{text}");
    // The section: `panels` and then `repeat`.
    let section = "      ],\n      \"repeat\": {\n        \"variable\": \"host\"\n      }\n    },";
    assert!(text.contains(section), "{text}");
}

#[test]
fn an_absent_max_per_row_is_not_written_and_a_given_value_is_not_changed() {
    let text = std::str::from_utf8(FIXTURE).unwrap();
    let v = base();
    assert!(v["sections"][2]["panels"][0]["repeat"].get("max_per_row").is_none());
    assert_eq!(text.matches("\"max_per_row\"").count(), 1, "the writer adds no default value 4");
    let mut v = base();
    v["sections"][2]["panels"][0]["repeat"]["max_per_row"] = json!(4);
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(std::str::from_utf8(out.bytes()).unwrap().matches("\"max_per_row\": 4").count(), 1);
}

#[test]
fn the_members_of_the_input_in_any_order_give_the_canonical_bytes() {
    let mut v = base();
    v["sections"][0]["panels"][0]["repeat"] = json!({"max_per_row": 3, "direction": "h", "variable": "host"});
    let mut moved = v["sections"][1].as_object().unwrap().clone();
    moved.remove("repeat");
    let mut reordered = serde_json::Map::new();
    reordered.insert("repeat".to_owned(), json!({"variable": "host"}));
    for (k, x) in moved {
        reordered.insert(k, x);
    }
    v["sections"][1] = Value::Object(reordered);
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(out.bytes(), FIXTURE);
}

#[test]
fn a_document_with_no_repeat_keeps_its_bytes() {
    for bytes in [PLACEMENTS, NODES, ALL_TYPES, F_MEMBERS] {
        let doc = validate(bytes, None).unwrap();
        assert_eq!(doc.bytes(), bytes);
        assert!(!std::str::from_utf8(doc.bytes()).unwrap().contains("\"repeat\""));
        assert!(
            doc.document().sections().iter().all(|s| s.repeat.is_none() && s.panels.iter().all(|p| p.repeat.is_none()))
        );
    }
}

#[test]
fn the_canonical_form_is_stable() {
    let first = validate(FIXTURE, None).unwrap();
    let second = validate(first.bytes(), None).unwrap();
    assert_eq!((first.bytes(), first.etag()), (second.bytes(), second.etag()));
}

// -------------------------------------------------------------------- rules

#[test]
fn a_repeat_variable_can_be_hidden_or_placed_and_a_query_or_custom_variable_can_repeat() {
    let mut v = base();
    v["variables"][0]["hide"] = json!("variable");
    assert_eq!(errors_of(&v), []);
    let mut v = base();
    v["variables"][1]["placement"] = json!({"section": 0, "x": 0, "y": 12, "w": 6, "h": 2});
    assert_eq!(errors_of(&v), []);
    // The fixture holds a `custom` variable (host) and a `query` variable (region) as repeat variables.
    let doc = validate(FIXTURE, None).unwrap();
    let names: Vec<&str> = doc.document().variables().iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["host", "region", "env", "account"]);
}

#[test]
fn a_variable_with_multi_false_cannot_repeat_and_with_multi_true_can() {
    let mut v = base();
    v["variables"][0]["multi"] = json!(false);
    v["variables"][0]["default"] = json!(["a"]);
    v["variables"][0]["include_all"] = json!(false);
    // The panel 1, the section 1 and the panel 6 name `host`: one error each.
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(
        pointers,
        ["/sections/0/panels/0/repeat/variable", "/sections/1/repeat/variable", "/sections/2/panels/0/repeat/variable"]
    );
    assert!(e.iter().all(|x| x.code == ErrorCode::NotPermitted));
}

#[test]
fn max_per_row_is_valid_from_1_to_24_with_h() {
    for n in [1, 4, 24] {
        let mut v = base();
        v["sections"][0]["panels"][0]["repeat"]["max_per_row"] = json!(n);
        assert_eq!(errors_of(&v), [], "{n}");
    }
    let mut v = base();
    v["sections"][0]["panels"][0]["repeat"]["max_per_row"] = json!(2.5);
    assert_eq!(one_error(&v), (ErrorCode::WrongType, "/sections/0/panels/0/repeat/max_per_row".to_owned()));
    let mut v = base();
    v["sections"][0]["panels"][0]["repeat"]["max_per_row"] = Value::Null;
    assert_eq!(one_error(&v), (ErrorCode::WrongType, "/sections/0/panels/0/repeat/max_per_row".to_owned()));
}

#[test]
fn the_repeat_object_must_be_an_object_and_a_section_repeat_has_one_member() {
    let mut v = base();
    v["sections"][0]["panels"][0]["repeat"] = Value::Null;
    assert_eq!(one_error(&v), (ErrorCode::WrongType, "/sections/0/panels/0/repeat".to_owned()));
    let mut v = base();
    v["sections"][1]["repeat"] = json!({});
    assert_eq!(one_error(&v), (ErrorCode::MissingMember, "/sections/1/repeat".to_owned()));
    let mut v = base();
    v["sections"][1]["repeat"] = json!({"variable": "host", "direction": "h"});
    assert_eq!(one_error(&v), (ErrorCode::UnknownMember, "/sections/1/repeat/direction".to_owned()));
    let mut v = base();
    v["sections"][1]["repeat"]["variable"] = json!(3);
    assert_eq!(one_error(&v), (ErrorCode::WrongType, "/sections/1/repeat/variable".to_owned()));
}

#[test]
fn a_panel_with_v_can_share_rows_and_a_panel_with_h_can_touch_other_rows() {
    // The panel 2 (v) has the panel 3 beside it in the fixture: valid.
    assert_eq!(errors_of(&base()), []);
    // The panel 1 (h) ends at row 4 and the panel 2 starts at row 4: the rows touch and do not share.
    let mut v = base();
    v["sections"][0]["panels"][0]["grid"] = json!({"x": 0, "y": 0, "w": 24, "h": 4});
    assert_eq!(errors_of(&v), []);
    // One row unit of overlap is an error.
    v["sections"][0]["panels"][0]["grid"]["h"] = json!(5);
    let e = errors_of(&v);
    let codes: Vec<(ErrorCode, &str)> = e.iter().map(|x| (x.code, x.pointer.as_str())).collect();
    assert!(codes.contains(&(ErrorCode::Overlap, "/sections/0/panels/0/repeat/direction")), "{codes:?}");
}

#[test]
fn the_stored_x_and_w_of_a_panel_with_h_do_not_count_for_the_rows() {
    let mut v = base();
    // The panel 1 has x 0 and w 6: no column overlap with the panel 3, but the band has the full width.
    v["sections"][0]["panels"][0]["grid"] = json!({"x": 0, "y": 4, "w": 6, "h": 4});
    v["sections"][0]["panels"][1]["grid"] = json!({"x": 6, "y": 4, "w": 2, "h": 4});
    // The panel 2 repeats with v and shares the rows 4 to 7 with the panel 1.
    assert_eq!(one_error(&v), (ErrorCode::Overlap, "/sections/0/panels/0/repeat/direction".to_owned()));
}

#[test]
fn a_placement_in_the_rows_of_a_panel_with_h_is_an_error_at_the_panel() {
    let mut v = base();
    v["variables"][3]["placement"] = json!({"section": 2, "x": 12, "y": 0, "w": 6, "h": 2});
    let e = errors_of(&v);
    let codes: Vec<(ErrorCode, &str)> = e.iter().map(|x| (x.code, x.pointer.as_str())).collect();
    assert_eq!(codes, [(ErrorCode::Overlap, "/sections/2/panels/0/repeat/direction")], "{e:#?}");
    // A placement below the band is valid.
    v["variables"][3]["placement"] = json!({"section": 2, "x": 12, "y": 8, "w": 6, "h": 2});
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_panel_with_h_in_another_section_row_is_not_a_conflict() {
    let mut v = base();
    // The panel 5 of the section 1 has the rows 0 to 3, like the panel 1 of the section 0.
    assert_eq!(v["sections"][1]["panels"][0]["grid"]["y"], 0);
    v["sections"][1].as_object_mut().unwrap().remove("repeat");
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_section_that_repeats_needs_a_title_and_holds_no_repeat_and_no_reuse() {
    // A section with a title and plain panels is valid (the fixture). The first section can repeat with a title.
    let mut v = base();
    v["sections"][0]["title"] = json!("First");
    v["sections"][0]["repeat"] = json!({"variable": "region"});
    v["sections"][0]["panels"][0].as_object_mut().unwrap().remove("repeat");
    v["sections"][0]["panels"][1].as_object_mut().unwrap().remove("repeat");
    assert_eq!(errors_of(&v), []);
    // A copy of a section with a collapsed header is valid.
    v["sections"][0]["collapsed"] = json!(true);
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_repeated_section_and_a_repeating_panel_in_it_give_the_error_at_the_panel() {
    let mut v = base();
    v["sections"][1]["panels"][0]["repeat"] = json!({"variable": "region", "direction": "v"});
    assert_eq!(one_error(&v), (ErrorCode::NotPermitted, "/sections/1/panels/0/repeat".to_owned()));
}

#[test]
fn a_reuse_of_a_panel_that_does_not_repeat_is_valid() {
    // The panel 7 reuses the panel 8 in the fixture.
    assert_eq!(errors_of(&base()), []);
}

#[test]
fn a_placement_in_a_section_that_does_not_repeat_is_valid() {
    let mut v = base();
    v["variables"][3]["placement"] = json!({"section": 2, "x": 0, "y": 12, "w": 6, "h": 2});
    assert_eq!(errors_of(&v), []);
    v["variables"][3]["placement"]["section"] = json!(1);
    assert_eq!(one_error(&v), (ErrorCode::NotPermitted, "/variables/3/placement/section".to_owned()));
}

#[test]
fn a_bad_variable_does_not_make_a_false_unknown_variable_error() {
    let mut v = base();
    v["variables"][0]["name"] = json!("1host");
    // The variable `host` has a bad name: its name is not known. The repeats that name it add no error.
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::BadFormat, "/variables/0/name"));
}

#[test]
fn the_other_errors_of_a_panel_stay_with_a_repeat() {
    let mut v = base();
    v["sections"][0]["panels"][0]["title"] = json!(1);
    v["sections"][0]["panels"][0]["repeat"]["variable"] = json!("nothing");
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/sections/0/panels/0/title", "/sections/0/panels/0/repeat/variable"]);
}

#[test]
fn the_stored_counts_do_not_count_copies() {
    // 200 panels with a repeat of 24 values stay valid: the limits of 4.12 count the stored panels.
    let mut v = base();
    let stat = v["sections"][0]["panels"][2].clone();
    let panels: Vec<Value> = (0..200u32)
        .map(|i| {
            let mut p = stat.clone();
            p["id"] = json!(100 + i);
            p["grid"] = json!({"x": 0, "y": i * 5, "w": 24, "h": 4});
            p["repeat"] = json!({"variable": "host", "direction": "h"});
            p
        })
        .collect();
    v["sections"] = json!([{"title": null, "collapsed": false, "panels": panels}]);
    assert_eq!(errors_of(&v), []);
}

// ------------------------------------------------------------------- limits

#[test]
fn the_constants_are_the_values_of_the_table_of_limits() {
    assert_eq!((MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION, MAX_REPEAT_PER_DASHBOARD), (24, 12, 120));
    for (name, value) in [
        ("MAX_REPEAT_PER_PANEL", MAX_REPEAT_PER_PANEL),
        ("MAX_REPEAT_PER_SECTION", MAX_REPEAT_PER_SECTION),
        ("MAX_REPEAT_PER_DASHBOARD", MAX_REPEAT_PER_DASHBOARD),
    ] {
        let row = DOC_FORMAT
            .lines()
            .find(|l| l.starts_with(&format!("| `{name}` |")))
            .unwrap_or_else(|| panic!("{name} has no row in 4.35"));
        let cell = row.split('|').nth(2).unwrap().trim();
        assert_eq!(cell, value.to_string(), "{name}");
    }
}

// ------------------------------------------------------------ the parity fixtures (15.14c)

#[test]
fn the_three_parity_fixtures_are_valid_and_canonical() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/parity");
    for name in ["repeat-h", "repeat-v", "repeat-section"] {
        let bytes = std::fs::read(dir.join(format!("{name}.json"))).unwrap();
        let doc = validate(&bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes.as_slice(), "{name}");
    }
}
