// SPDX-License-Identifier: MIT OR Apache-2.0
//! The panel type `text` (4.33a): the fixture and its canonical form, the boundaries of the limits, one refusal
//! for each error, the rules across members (4.34) and the constants (4.35). The type joins format 3 with no
//! change of the version (4.42).

use hune_core::document::{FORMAT_VERSION, PanelData, PanelOptions, PanelType, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::{MAX_TEXT_BYTES, MAX_TEXT_LINE_BYTES, MAX_TEXT_LINES};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/text_panel.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
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

/// The text panel of the example of 4.33a: the panel 7 of the fixture, in the first section.
fn example(v: &mut Value) -> &mut Value {
    &mut v["sections"][0]["panels"][1]
}

fn one_error(v: &Value) -> (ErrorCode, String) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    (e[0].code, e[0].pointer.clone())
}

const P: &str = "/sections/0/panels/1";

/// Each file of `invalid/f29_*.json` has one error: its file name, its code and its pointer (4.33a).
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("mode_html", ErrorCode::BadFormat, "/sections/0/panels/1/options/mode"),
    ("mode_code", ErrorCode::BadFormat, "/sections/0/panels/1/options/mode"),
    ("mode_absent", ErrorCode::MissingMember, "/sections/0/panels/1/options"),
    ("mode_wrong_type", ErrorCode::WrongType, "/sections/0/panels/1/options/mode"),
    ("content_not_array", ErrorCode::WrongType, "/sections/0/panels/1/options/content"),
    ("content_item_not_text", ErrorCode::WrongType, "/sections/0/panels/1/options/content/1"),
    ("line_lf", ErrorCode::BadFormat, "/sections/0/panels/1/options/content/1"),
    ("line_cr", ErrorCode::BadFormat, "/sections/0/panels/1/options/content/1"),
    ("line_nul", ErrorCode::BadFormat, "/sections/0/panels/1/options/content/1"),
    ("line_soh", ErrorCode::BadFormat, "/sections/0/panels/1/options/content/1"),
    ("line_del", ErrorCode::BadFormat, "/sections/0/panels/1/options/content/1"),
    ("line_8193_bytes", ErrorCode::TooLarge, "/sections/0/panels/1/options/content/1"),
    ("lines_2001", ErrorCode::TooLarge, "/sections/0/panels/1/options/content"),
    ("text_65537_bytes", ErrorCode::TooLarge, "/sections/0/panels/1/options/content"),
    ("has_data", ErrorCode::NotPermitted, "/sections/0/panels/1/data"),
    ("has_transforms", ErrorCode::NotPermitted, "/sections/0/panels/1/transforms"),
    ("has_fields", ErrorCode::NotPermitted, "/sections/0/panels/1/fields"),
    ("has_repeat", ErrorCode::NotPermitted, "/sections/0/panels/1/repeat"),
    ("options_unknown_member", ErrorCode::UnknownMember, "/sections/0/panels/1/options/extra"),
    ("options_duplicate_member", ErrorCode::DuplicateMember, "/sections/0/panels/1/options/mode"),
    ("reuse_of_text_panel", ErrorCode::BadReference, "/sections/0/panels/0/data/reuse/panel"),
    ("options_of_text_in_table", ErrorCode::UnknownMember, "/sections/0/panels/0/options/content"),
];

#[test]
fn each_refusal_gives_its_one_error() {
    for (name, code, pointer) in REFUSALS {
        let bytes =
            std::fs::read(invalid_dir().join(format!("f29_{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = validate(&bytes, None).unwrap_err();
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
    }
}

#[test]
fn there_are_22_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f29_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("f29_{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 22);
}

#[test]
fn the_messages_name_the_rule_and_the_value() {
    let bytes = std::fs::read(invalid_dir().join("f29_mode_html.json")).unwrap();
    let e = validate(&bytes, None).unwrap_err();
    assert!(e[0].message.contains("markdown") && e[0].message.contains("html"), "{}", e[0].message);
    let bytes = std::fs::read(invalid_dir().join("f29_has_data.json")).unwrap();
    let e = validate(&bytes, None).unwrap_err();
    assert!(e[0].message.contains("a text panel has no query"), "{}", e[0].message);
    let bytes = std::fs::read(invalid_dir().join("f29_reuse_of_text_panel.json")).unwrap();
    let e = validate(&bytes, None).unwrap_err();
    assert!(e[0].message.contains("a text panel has no query"), "{}", e[0].message);
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE, "the golden canonical form");
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3, "the type text adds no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_text_panels() {
    let doc = validate(FIXTURE, None).unwrap();
    let sections = doc.document().sections();
    let panel = &sections[0].panels[1];
    assert_eq!(panel.panel_type, PanelType::Text);
    assert_eq!(panel.data, PanelData::None);
    assert!(panel.transforms.is_empty() && panel.fields.overrides.is_empty() && panel.repeat.is_none());
    let PanelOptions::Text(options) = &panel.options else { panic!("not a text panel") };
    assert_eq!(options.text(), "# Runbook\n\nRegion: $region. See [the wiki](https://wiki.example/runbook).");
    assert_eq!(options.mode.to_string(), "markdown");
    assert_eq!(panel.options.panel_type(), PanelType::Text);
    let cases = &sections[1].panels;
    let content = |i: usize| match &cases[i].options {
        PanelOptions::Text(t) => t.content.clone(),
        _ => panic!("not a text panel"),
    };
    assert!(content(0).is_empty(), "an empty array is an empty text");
    assert_eq!(content(1)[0], "Tab:\there");
    assert!(content(1)[1].contains('\u{2028}') && content(1)[2].contains('\u{1F600}'));
    assert!(cases[1].transparent);
    assert_eq!(PanelType::ALL.len(), 15);
}

#[test]
fn the_boundaries_are_valid() {
    let doc = validate(FIXTURE, None).unwrap();
    let panels = &doc.document().sections()[1].panels;
    let lens = |i: usize| match &panels[i].options {
        PanelOptions::Text(t) => {
            (t.content.len(), t.content.iter().map(String::len).max().unwrap_or(0), t.text().len())
        }
        _ => panic!("not a text panel"),
    };
    assert_eq!(lens(2).0, MAX_TEXT_LINES);
    assert_eq!(lens(3).1, MAX_TEXT_LINE_BYTES);
    assert!(panels[3].options.panel_type() == PanelType::Text);
    assert_eq!(lens(4).2, MAX_TEXT_BYTES);
    // The last character of the long line has two bytes: the limit counts bytes, not characters.
    let mut v = base();
    v["sections"][1]["panels"][3]["options"]["content"] = json!(["é".repeat(4096)]);
    assert!(errors_of(&v).is_empty());
    v["sections"][1]["panels"][3]["options"]["content"] = json!(["é".repeat(4096) + "a"]);
    assert_eq!(one_error(&v), (ErrorCode::TooLarge, "/sections/1/panels/3/options/content/0".to_owned()));
}

#[test]
fn a_line_that_is_too_long_and_a_text_that_is_too_large_are_not_the_same_error() {
    let mut v = base();
    // 8 lines of 8192 bytes are 65,543 bytes with the line ends: no line is too long, the text is.
    v["sections"][0]["panels"][1]["options"]["content"] = json!(vec!["x".repeat(8192); 8]);
    assert_eq!(one_error(&v), (ErrorCode::TooLarge, format!("{P}/options/content")));
    v["sections"][0]["panels"][1]["options"]["content"] = json!(vec!["x".repeat(8192); 7]);
    assert!(errors_of(&v).is_empty(), "7 lines of 8192 bytes are 57,350 bytes");
}

#[test]
fn a_tab_is_permitted_and_the_other_control_characters_are_not() {
    let mut v = base();
    example(&mut v)["options"]["content"] = json!(["a\tb"]);
    assert!(errors_of(&v).is_empty());
    for c in ['\u{0}', '\u{1}', '\u{8}', '\u{b}', '\u{c}', '\u{1f}', '\u{7f}', '\n', '\r'] {
        example(&mut v)["options"]["content"] = json!([format!("a{c}b")]);
        assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{P}/options/content/0")), "{c:?}");
    }
    // U+0085 and U+2028 are not control characters of the rule: the line can hold them.
    example(&mut v)["options"]["content"] = json!(["a\u{85}b\u{2028}c\u{2029}"]);
    assert!(errors_of(&v).is_empty());
}

#[test]
fn the_markdown_is_never_checked() {
    let mut v = base();
    example(&mut v)["options"]["content"] = json!([
        "<script>alert(1)</script>",
        "[x](javascript:alert(1))",
        "![a](http://h/i.png)",
        "$unknown ${x:bad}",
        "```",
        "> > > >"
    ]);
    assert!(errors_of(&v).is_empty(), "the validation reads lines only (4.33a, rule 5)");
}

// ------------------------------------------------------------ canonical form

#[test]
fn a_text_panel_has_the_members_of_4_38_in_order() {
    let text = std::str::from_utf8(validate(FIXTURE, None).unwrap().bytes()).unwrap().to_owned();
    let panel = "        {\n          \"id\": 7,\n          \"type\": \"text\",\n          \"title\": \"Notes\",\n          \"description\": \"\",\n          \"transparent\": false,\n          \"grid\": {\n            \"x\": 12,\n            \"y\": 0,\n            \"w\": 12,\n            \"h\": 8\n          },\n          \"options\": {\n            \"mode\": \"markdown\",\n            \"content\": [\n              \"# Runbook\",\n              \"\",\n              \"Region: $region. See [the wiki](https://wiki.example/runbook).\"\n            ]\n          },\n          \"links\": []\n        }";
    assert!(text.contains(panel), "{text}");
    assert!(
        text.contains("\"options\": {\n            \"mode\": \"markdown\",\n            \"content\": []\n          }")
    );
}

#[test]
fn the_members_of_the_input_in_any_order_give_the_canonical_bytes() {
    let mut v = base();
    v["sections"][0]["panels"][1] = json!({
        "links": [],
        "options": {"content": ["# Runbook", "", "Region: $region. See [the wiki](https://wiki.example/runbook)."], "mode": "markdown"},
        "grid": {"h": 8, "w": 12, "y": 0, "x": 12},
        "transparent": false,
        "description": "",
        "title": "Notes",
        "type": "text",
        "id": 7
    });
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(out.bytes(), FIXTURE);
}

#[test]
fn the_text_is_not_changed_by_the_writer() {
    // No trim, no clamp: a line of spaces, a line end space and a tab at the start stay as they are.
    let mut v = base();
    example(&mut v)["options"]["content"] = json!(["  indented", "trailing  ", "\tTab", "   ", ""]);
    let out = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let back: Value = serde_json::from_slice(out.bytes()).unwrap();
    assert_eq!(
        back["sections"][0]["panels"][1]["options"]["content"],
        json!(["  indented", "trailing  ", "\tTab", "   ", ""])
    );
}

#[test]
fn a_document_with_no_text_panel_keeps_its_bytes() {
    for (name, bytes) in [("minimal", MINIMAL), ("all_types", ALL_TYPES)] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}");
    }
}

// ------------------------------------------------------ rules across members

#[test]
fn a_text_panel_gives_not_permitted_for_each_query_member_at_once() {
    let mut v = base();
    let p = example(&mut v);
    p["data"] = json!(1);
    p["transforms"] = json!("x");
    p["fields"] = json!([]);
    p["repeat"] = json!({"variable": "region", "direction": "v"});
    let e = errors_of(&v);
    let got: Vec<(ErrorCode, &str)> = e.iter().map(|x| (x.code, x.pointer.as_str())).collect();
    assert_eq!(
        got,
        [
            (ErrorCode::NotPermitted, "/sections/0/panels/1/data"),
            (ErrorCode::NotPermitted, "/sections/0/panels/1/transforms"),
            (ErrorCode::NotPermitted, "/sections/0/panels/1/fields"),
            (ErrorCode::NotPermitted, "/sections/0/panels/1/repeat"),
        ],
        "the value of the member is not read"
    );
}

#[test]
fn a_panel_of_another_type_still_needs_its_members() {
    let mut v = base();
    // The table panel (index 0) loses `data`: the old rule stays.
    v["sections"][0]["panels"][0].as_object_mut().unwrap().remove("data");
    assert_eq!(one_error(&v), (ErrorCode::MissingMember, "/sections/0/panels/0".to_owned()));
    // The type `text` on a table panel: its `data` is refused, and its table options are not text options.
    let mut v = base();
    v["sections"][0]["panels"][0]["type"] = json!("text");
    let e = errors_of(&v);
    assert!(e.iter().any(|x| x.code == ErrorCode::NotPermitted && x.pointer == "/sections/0/panels/0/data"), "{e:#?}");
}

#[test]
fn a_reuse_of_a_text_panel_gives_bad_reference() {
    let mut v = base();
    v["sections"][0]["panels"][0]["data"] = json!({"reuse": {"panel": 7, "with_transforms": false}});
    assert_eq!(one_error(&v), (ErrorCode::BadReference, "/sections/0/panels/0/data/reuse/panel".to_owned()));
    // A reuse of the table panel stays valid.
    let mut v = base();
    v["sections"][1]["panels"][0]["data"] = json!({"reuse": {"panel": 1, "with_transforms": false}});
    v["sections"][1]["panels"][0]["type"] = json!("table");
    v["sections"][1]["panels"][0]["transforms"] = json!([]);
    v["sections"][1]["panels"][0]["fields"] = json!({"defaults": {}, "overrides": []});
    v["sections"][1]["panels"][0]["options"] = json!({"show_header": true, "sort": []});
    assert!(errors_of(&v).is_empty());
}

#[test]
fn the_ids_and_the_places_of_a_text_panel_follow_the_common_rules() {
    let mut v = base();
    v["sections"][0]["panels"][1]["id"] = json!(1);
    assert_eq!(one_error(&v), (ErrorCode::NotUnique, "/sections/0/panels/1/id".to_owned()));
    let mut v = base();
    v["sections"][0]["panels"][1]["grid"] = json!({"x": 0, "y": 0, "w": 12, "h": 8});
    assert_eq!(errors_of(&v)[0].code, ErrorCode::Overlap);
    let mut v = base();
    v["sections"][0]["panels"][1]["links"] =
        json!([{"title": "Wiki", "url": "https://wiki.example", "target_blank": true}]);
    assert!(errors_of(&v).is_empty(), "a text panel has links (4.33a)");
    v["sections"][0]["panels"][1]["links"] = json!([{"title": "Wiki", "url": "javascript:x", "target_blank": true}]);
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{P}/links/0/url")));
}

#[test]
fn a_binary_that_does_not_know_the_type_refuses_it_at_type() {
    let mut v = base();
    v["sections"][0]["panels"][1]["type"] = json!("texts");
    let e = errors_of(&v);
    assert!(e.iter().any(|x| x.code == ErrorCode::BadFormat && x.pointer == "/sections/0/panels/1/type"), "{e:#?}");
    assert!(e.iter().all(|x| x.pointer.starts_with("/sections/0/panels/1")), "no error in another panel: {e:#?}");
}

#[test]
fn the_text_of_a_title_and_the_options_follow_the_common_limits() {
    let mut v = base();
    example(&mut v)["title"] = json!("t".repeat(257));
    assert_eq!(one_error(&v), (ErrorCode::OutOfRange, format!("{P}/title")));
}

// ------------------------------------------------------------ the constants

#[test]
fn the_constants_equal_the_table_of_4_35() {
    for (name, value) in [
        ("MAX_TEXT_LINES", MAX_TEXT_LINES),
        ("MAX_TEXT_LINE_BYTES", MAX_TEXT_LINE_BYTES),
        ("MAX_TEXT_BYTES", MAX_TEXT_BYTES),
    ] {
        let row = DOC_FORMAT
            .lines()
            .find(|l| l.starts_with(&format!("| `{name}` |")))
            .unwrap_or_else(|| panic!("{name} has no row in 4.35"));
        let cell = row.split('|').nth(2).unwrap().trim();
        let number: String = cell.split(" (").next().unwrap().chars().filter(|c| *c != ',').collect();
        assert_eq!(number, value.to_string(), "{name}");
    }
    assert_eq!((MAX_TEXT_LINES, MAX_TEXT_LINE_BYTES, MAX_TEXT_BYTES), (2_000, 8_192, 65_536));
}

// ------------------------------------------------------------ the parity fixtures (15.14c)

#[test]
fn the_three_parity_fixtures_are_valid_and_canonical() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/parity");
    for name in ["text-constructs", "text-variables", "text-links"] {
        let bytes = std::fs::read(dir.join(format!("{name}.json"))).unwrap();
        let doc = validate(&bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes.as_slice(), "{name}");
        let panels: Vec<_> = doc.document().panels().collect();
        assert_eq!(panels.len(), 1, "{name}");
        assert_eq!(panels[0].1.panel_type, PanelType::Text, "{name}");
    }
}

#[test]
fn the_first_parity_fixture_has_no_variable_and_the_second_has_placeholders() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/parity");
    let first = std::fs::read_to_string(dir.join("text-constructs.json")).unwrap();
    assert!(!first.contains('$'));
    let second = std::fs::read_to_string(dir.join("text-variables.json")).unwrap();
    assert!(second.contains("${region:csv}") && second.contains("$tier"));
}
