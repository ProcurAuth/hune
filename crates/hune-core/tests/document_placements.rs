// SPDX-License-Identifier: MIT OR Apache-2.0
//! The member `placement` of a variable (4.7a): the fixture, one refusal for each error, the canonical form
//! and the rules across members (4.34). The member joins format 3 with no change of the version (4.42).

use hune_core::document::{FORMAT_VERSION, Grid, Hide, Placement, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/placements.json");
const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

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

/// Each file of `invalid/k_place_*.json` has one error: its file name, its code and its pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("k_place_section_out_of_range", ErrorCode::BadReference, "/variables/0/placement/section"),
    ("k_place_section_negative", ErrorCode::OutOfRange, "/variables/0/placement/section"),
    ("k_place_section_wrong_type", ErrorCode::WrongType, "/variables/0/placement/section"),
    ("k_place_section_fraction", ErrorCode::WrongType, "/variables/0/placement/section"),
    ("k_place_hide_variable", ErrorCode::NotPermitted, "/variables/4/placement"),
    ("k_place_overlap_panel", ErrorCode::Overlap, "/variables/0/placement"),
    ("k_place_overlap_placement", ErrorCode::Overlap, "/variables/1/placement"),
    ("k_place_x_plus_w", ErrorCode::OutOfRange, "/variables/0/placement/w"),
    ("k_place_x_range", ErrorCode::OutOfRange, "/variables/0/placement/x"),
    ("k_place_y_range", ErrorCode::OutOfRange, "/variables/0/placement/y"),
    ("k_place_w_zero", ErrorCode::OutOfRange, "/variables/0/placement/w"),
    ("k_place_h_range", ErrorCode::OutOfRange, "/variables/0/placement/h"),
    ("k_place_title_empty", ErrorCode::OutOfRange, "/variables/1/placement/title"),
    ("k_place_title_too_long", ErrorCode::OutOfRange, "/variables/1/placement/title"),
    ("k_place_title_wrong_type", ErrorCode::WrongType, "/variables/1/placement/title"),
    ("k_place_x_missing", ErrorCode::MissingMember, "/variables/0/placement"),
    ("k_place_section_missing", ErrorCode::MissingMember, "/variables/0/placement"),
    ("k_place_extra_member", ErrorCode::UnknownMember, "/variables/0/placement/z"),
    ("k_place_not_object", ErrorCode::WrongType, "/variables/0/placement"),
    ("k_place_null", ErrorCode::WrongType, "/variables/0/placement"),
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
fn no_refusal_file_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("k_place_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE);
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3);
}

#[test]
fn the_typed_document_reads_the_placements() {
    let doc = validate(FIXTURE, None).unwrap();
    let vars = doc.document().variables();
    assert_eq!(vars[0].placement, Some(Placement { section: 0, grid: Grid { x: 0, y: 0, w: 6, h: 2 }, title: None }));
    assert_eq!(
        vars[1].placement,
        Some(Placement { section: 1, grid: Grid { x: 0, y: 0, w: 12, h: 2 }, title: Some("Pick nodes".to_owned()) })
    );
    assert!(vars[2].placement.is_some());
    let bar: Vec<&str> = vars.iter().filter(|v| v.placement.is_none()).map(|v| v.name.as_str()).collect();
    assert_eq!(bar, ["account", "region"], "the variables that stay in the bar");
    assert_eq!(vars[1].hide, Hide::Label);
}

// ---------------------------------------------------------- canonical form

#[test]
fn the_placement_is_the_last_member_of_each_kind_and_the_title_is_its_last_member() {
    let text = std::str::from_utf8(FIXTURE).unwrap();
    for (name, last) in [("env", "default"), ("node", "default"), ("min_weight", "default")] {
        let from = text.find(&format!("\"name\": \"{name}\"")).unwrap();
        let var_end = text[from..].find("\"placement\"").unwrap() + from;
        assert!(text[from..var_end].contains(&format!("\"{last}\"")), "{name}");
        // After the placement object, the next text is the end of the variable.
        let after = &text[var_end..];
        let close = after.find("\n    }").unwrap();
        assert!(!after[..close].contains("\"kind\""), "{name}");
    }
    let title = text.find("\"title\": \"Pick nodes\"").unwrap();
    let h = text[..title].rfind("\"h\": 2").unwrap();
    assert!(h < title);
}

#[test]
fn the_members_of_the_input_in_any_order_give_the_canonical_bytes() {
    let mut v = base();
    v["variables"][1]["placement"] = json!({"title": "Pick nodes", "h": 2, "w": 12, "y": 0, "x": 0, "section": 1});
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(out.bytes(), FIXTURE);
}

#[test]
fn a_document_with_no_placement_keeps_its_bytes() {
    for bytes in [NODES, ALL_TYPES] {
        let doc = validate(bytes, None).unwrap();
        assert_eq!(doc.bytes(), bytes);
        assert!(doc.document().variables().iter().all(|v| v.placement.is_none()));
        assert!(!std::str::from_utf8(doc.bytes()).unwrap().contains("\"placement\": {"));
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
fn each_variable_kind_can_have_a_placement() {
    let doc = validate(FIXTURE, None).unwrap();
    let kinds: Vec<&str> = doc
        .document()
        .variables()
        .iter()
        .filter(|v| v.placement.is_some())
        .map(|v| hune_core::document::TextEnum::as_str(v.kind.tag()))
        .collect();
    assert_eq!(kinds, ["custom", "query", "text"]);
}

#[test]
fn hide_none_and_hide_label_permit_a_placement_and_hide_variable_does_not() {
    let mut v = base();
    // `env` has hide = none and `node` has hide = label: the fixture is valid.
    assert_eq!(errors_of(&v), []);
    v["variables"][4]["placement"] = json!({"section": 0, "x": 6, "y": 0, "w": 6, "h": 2});
    assert_eq!(one_error(&v), (ErrorCode::NotPermitted, "/variables/4/placement".to_owned()));
    v["variables"][4]["hide"] = json!("label");
    assert_eq!(errors_of(&v), []);
}

#[test]
fn the_grid_ranges_of_a_panel_apply() {
    let mut v = base();
    // A section with no panel at its far corner: the largest values.
    let far = json!({"section": 1, "x": 23, "y": 10_000, "w": 1, "h": 200});
    v["variables"][0]["placement"] = far;
    assert_eq!(errors_of(&v), []);
    v["variables"][0]["placement"] = json!({"section": 1, "x": 0, "y": 20, "w": 24, "h": 1});
    assert_eq!(errors_of(&v), []);
    v["variables"][0]["placement"] = json!({"section": 1, "x": 0, "y": 20, "w": 25, "h": 1});
    assert_eq!(one_error(&v), (ErrorCode::OutOfRange, "/variables/0/placement/w".to_owned()));
}

#[test]
fn a_title_of_256_characters_is_valid_and_the_member_is_optional() {
    let mut v = base();
    v["variables"][0]["placement"]["title"] = json!("é".repeat(256));
    assert_eq!(errors_of(&v), []);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(
        doc.document().variables()[0].placement.as_ref().unwrap().title.as_ref().map(|t| t.chars().count()),
        Some(256)
    );
}

#[test]
fn touching_rectangles_do_not_overlap_and_the_same_place_in_two_sections_is_valid() {
    let mut v = base();
    // The placement of `env` ends at y 2 and the stat starts at y 2: valid (the fixture).
    // Two placements side by side in one section, and one at the same place in the other section.
    v["variables"][0]["placement"] = json!({"section": 1, "x": 20, "y": 0, "w": 4, "h": 2});
    v["variables"][3]["placement"] = json!({"section": 0, "x": 0, "y": 0, "w": 12, "h": 2});
    v["variables"][1]["placement"]["section"] = json!(0);
    v["variables"][1]["placement"]["x"] = json!(12);
    v["variables"][2]["placement"] = json!({"section": 0, "x": 0, "y": 8, "w": 24, "h": 1});
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_placement_overlaps_the_placements_before_it_and_the_error_is_on_the_later_variable() {
    let mut v = base();
    v["variables"][3]["placement"] = json!({"section": 0, "x": 5, "y": 1, "w": 4, "h": 2});
    // `account` is the fourth variable. It overlaps the placement of `env` (variable 0) and the stat (y 2).
    assert_eq!(one_error(&v), (ErrorCode::Overlap, "/variables/3/placement".to_owned()));
}

#[test]
fn a_placement_overlaps_a_panel_of_its_section_only() {
    let mut v = base();
    // The stat 1 of section 0 is at x 0 to 12, y 2 to 8. The same place in section 1 holds stat 2.
    v["variables"][0]["placement"] = json!({"section": 0, "x": 11, "y": 7, "w": 2, "h": 2});
    assert_eq!(one_error(&v), (ErrorCode::Overlap, "/variables/0/placement".to_owned()));
    v["variables"][0]["placement"] = json!({"section": 0, "x": 12, "y": 7, "w": 2, "h": 2});
    assert_eq!(errors_of(&v), []);
}

#[test]
fn the_section_index_is_checked_against_the_sections_of_the_document() {
    let mut v = base();
    v["sections"].as_array_mut().unwrap().pop();
    // Two placements name section 1, which does not exist now: one error each.
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/variables/1/placement/section", "/variables/2/placement/section"]);
    assert!(e.iter().all(|x| x.code == ErrorCode::BadReference));
    assert!(e[0].message.contains("1 of 1 sections"), "{}", e[0].message);
}

#[test]
fn a_bad_sections_array_adds_no_false_section_error() {
    let mut v = base();
    v["sections"] = json!([]);
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::OutOfRange, "/sections"));
}

#[test]
fn a_rename_moves_the_placement_and_a_delete_removes_it() {
    let mut v = base();
    v["variables"][0]["name"] = json!("environment");
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let renamed = doc.document().variable("environment").unwrap();
    assert_eq!(renamed.placement.as_ref().map(|p| p.grid.w), Some(6));
    let mut v = base();
    v["variables"].as_array_mut().unwrap().remove(0);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(doc.document().variables().iter().filter(|v| v.placement.is_some()).count(), 2);
}

#[test]
fn the_other_errors_of_a_variable_stay_with_a_placement() {
    let mut v = base();
    v["variables"][0]["name"] = json!("1env");
    v["variables"][0]["placement"]["w"] = json!(0);
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/variables/0/name", "/variables/0/placement/w"]);
}

#[test]
fn sixty_four_variables_can_each_have_a_place() {
    let mut v = base();
    let variables: Vec<Value> = (0..64u32)
        .map(|i| {
            json!({"kind": "text", "name": format!("v{i}"), "label": null, "hide": "none", "value_type": "text",
                "default": "", "placement": {"section": 1, "x": (i % 6) * 4, "y": 20 + (i / 6) * 2, "w": 4, "h": 2}})
        })
        .collect();
    v["variables"] = json!(variables);
    assert_eq!(errors_of(&v), []);
}

#[test]
fn a_placement_does_not_count_as_a_panel() {
    // The limit of 200 panels (4.12) counts panels only: 64 placements add nothing to it.
    let mut v = base();
    let variables: Vec<Value> = (0..64u32)
        .map(|i| {
            json!({"kind": "text", "name": format!("v{i}"), "label": null, "hide": "none", "value_type": "text",
                "default": "", "placement": {"section": 1, "x": (i % 6) * 4, "y": 20 + (i / 6) * 2, "w": 4, "h": 2}})
        })
        .collect();
    v["variables"] = json!(variables);
    let stat = v["sections"][0]["panels"][0].clone();
    let panels: Vec<Value> = (0..199u32)
        .map(|i| {
            let mut p = stat.clone();
            p["id"] = json!(100 + i);
            p["grid"] = json!({"x": 0, "y": i * 6, "w": 4, "h": 6});
            p
        })
        .collect();
    v["sections"][0]["panels"] = json!(panels);
    assert_eq!(errors_of(&v), []);
}
