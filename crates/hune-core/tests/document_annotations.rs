// SPDX-License-Identifier: MIT OR Apache-2.0
//! The member `annotations` (4.5b): the fixture and its canonical form, one refusal for each error, the rules
//! across members (4.34), the limits (4.35) and the SQL texts (7.18). The member joins format 3 with no
//! change of the version (4.42).

use hune_core::document::{FORMAT_VERSION, SqlOwner, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::{
    MAX_ANNOTATION_PANELS, MAX_ANNOTATION_ROWS, MAX_ANNOTATION_TAG_CHARS, MAX_ANNOTATION_TAGS,
    MAX_ANNOTATION_TEXT_BYTES, MAX_ANNOTATION_TITLE_CHARS, MAX_ANNOTATIONS, MAX_ANNOTATIONS_PER_PANEL, MAX_PANELS,
    MAX_TITLE_CHARS,
};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/annotations.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
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

/// The files that the save checks of the server refuse (7.18). `hune-core` reads them as valid documents.
const SERVER_REFUSALS: [&str; 5] = [
    "f25_source_unknown",
    "f25_macro_time_interval",
    "f25_macro_time_interval_ms",
    "f25_macro_interval_s",
    "f25_macro_interval_ms",
];

/// Each file of `invalid/f25_*.json` that the format refuses has one error: its name, code and pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f25_nine_annotations", ErrorCode::TooLarge, "/annotations"),
    ("f25_name_repeated", ErrorCode::NotUnique, "/annotations/1/name"),
    ("f25_panels_repeated_id", ErrorCode::NotUnique, "/annotations/0/panels/1"),
    ("f25_panels_unknown_id", ErrorCode::BadReference, "/annotations/0/panels/1"),
    ("f25_panels_empty", ErrorCode::OutOfRange, "/annotations/0/panels"),
    ("f25_panels_201_items", ErrorCode::TooLarge, "/annotations/0/panels"),
    ("f25_panels_id_zero", ErrorCode::OutOfRange, "/annotations/0/panels/0"),
    ("f25_panels_not_array", ErrorCode::WrongType, "/annotations/0/panels"),
    ("f25_unknown_member", ErrorCode::UnknownMember, "/annotations/0/format"),
    ("f25_color_absent", ErrorCode::MissingMember, "/annotations/0"),
    ("f25_color_invalid", ErrorCode::BadFormat, "/annotations/0/color"),
    ("f25_source_absent", ErrorCode::MissingMember, "/annotations/0"),
    ("f25_source_bad_id", ErrorCode::BadFormat, "/annotations/0/source"),
    ("f25_name_empty", ErrorCode::OutOfRange, "/annotations/0/name"),
    ("f25_name_257", ErrorCode::OutOfRange, "/annotations/0/name"),
    ("f25_sql_empty", ErrorCode::OutOfRange, "/annotations/0/sql"),
    ("f25_sql_line_lf", ErrorCode::BadFormat, "/annotations/0/sql/0"),
    ("f25_enabled_wrong_type", ErrorCode::WrongType, "/annotations/0/enabled"),
    ("f25_annotations_not_array", ErrorCode::WrongType, "/annotations"),
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
fn there_are_24_refusal_files_and_none_is_missing_from_the_tables() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f25_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS
        .iter()
        .map(|(n, _, _)| format!("{n}.json"))
        .chain(SERVER_REFUSALS.iter().map(|n| format!("{n}.json")))
        .collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 24);
}

#[test]
fn the_files_of_the_save_checks_are_valid_for_the_format() {
    for name in SERVER_REFUSALS {
        let bytes = std::fs::read(invalid_dir().join(format!("{name}.json"))).unwrap();
        validate(&bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
    }
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(std::str::from_utf8(doc.bytes()).unwrap(), std::str::from_utf8(FIXTURE).unwrap());
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3, "the member annotations adds no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_annotations() {
    let doc = validate(FIXTURE, None).unwrap();
    let a = doc.document().annotations();
    assert_eq!(a.len(), MAX_ANNOTATIONS);
    assert_eq!((a[0].name.as_str(), a[0].source.as_str(), a[0].color.as_str()), ("Deploys", "prod", "red"));
    assert_eq!(a[0].panels.as_deref(), Some(&[20u32, 21][..]));
    assert!(a[0].enabled && a[1].panels.is_none());
    assert!(!a[2].enabled);
    assert_eq!(a[6].name.chars().count(), 256);
    let colours: Vec<&str> = a.iter().map(|x| x.color.as_str()).collect();
    assert_eq!(
        colours,
        [
            "red",
            "#1f78c1",
            "rgb(255, 120, 0)",
            "semi-dark-blue",
            "green",
            "transparent",
            "text",
            "rgba(10, 20, 30, 0.5)"
        ]
    );
    // The order of the array that the author wrote stays: 20, 30, 10, 21.
    assert_eq!(a[6].panels.as_deref(), Some(&[20u32, 30, 10, 21][..]));
}

// ---------------------------------------------------------- canonical form

#[test]
fn annotations_come_after_variables_and_before_sections() {
    let text = std::str::from_utf8(validate(FIXTURE, None).unwrap().bytes()).unwrap().to_owned();
    let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s}"));
    assert!(
        at("\n  \"variables\": [") < at("\n  \"annotations\": [")
            && at("\n  \"annotations\": [") < at("\n  \"sections\": [")
    );
}

#[test]
fn the_members_of_an_annotation_come_in_the_order_of_4_5b_and_defaults_are_left_out() {
    let mut v = base();
    // A document that writes the members in another order, `enabled` true and a name of each kind.
    v["annotations"] = json!([{
        "panels": [21, 20],
        "enabled": true,
        "color": "red",
        "sql": ["SELECT 1 AS time"],
        "source": "prod",
        "name": "A"
    }]);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(doc.bytes()).unwrap().to_owned();
    assert_eq!(member_order(&text, 6), ["name", "source", "sql", "color", "panels"]);
    let out: Value = serde_json::from_slice(doc.bytes()).unwrap();
    assert_eq!(out["annotations"][0]["panels"], json!([21, 20]), "the array keeps its order");
    // `enabled` false stays, and it comes before `panels`.
    v["annotations"][0]["enabled"] = json!(false);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(doc.bytes()).unwrap().to_owned();
    assert_eq!(member_order(&text, 6), ["name", "source", "sql", "color", "enabled", "panels"]);
}

/// The names of the members of the first annotation, in the order of the canonical text. The members have the
/// indent of `indent` spaces.
fn member_order(text: &str, indent: usize) -> Vec<String> {
    let start = text.find("\n  \"annotations\": [").expect("annotations");
    let pad = format!("\n{}\"", " ".repeat(indent));
    let end = text[start..].find("\n    }").map_or(text.len(), |e| start + e);
    text[start..end]
        .match_indices(&pad)
        .filter_map(|(i, _)| text[start + i + pad.len()..].split('"').next().map(str::to_owned))
        .collect()
}

#[test]
fn an_empty_array_is_valid_and_the_canonical_form_has_no_member() {
    let mut v: Value = serde_json::from_slice(MINIMAL).unwrap();
    v["annotations"] = json!([]);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(doc.bytes(), MINIMAL, "the bytes equal the bytes of the same document with no member");
    assert!(doc.document().annotations().is_empty());
}

#[test]
fn a_document_with_no_annotation_keeps_its_bytes() {
    for bytes in [MINIMAL, ALL_TYPES, NODES] {
        let doc = validate(bytes, None).unwrap();
        assert_eq!(doc.bytes(), bytes);
        assert!(!std::str::from_utf8(doc.bytes()).unwrap().contains("annotations"));
    }
}

// ----------------------------------------------------------------- limits

#[test]
fn the_boundaries_are_valid() {
    // 8 annotations (the fixture), a name of 256 characters (the fixture) and 200 panels in `panels`.
    let mut v: Value = serde_json::from_slice(MINIMAL).unwrap();
    let panels: Vec<Value> = (1..=200)
        .map(|id| {
            json!({
                "id": id, "type": "text", "title": format!("P{id}"), "description": "", "transparent": false,
                "grid": {"x": ((id - 1) % 2) * 12, "y": ((id - 1) / 2) * 2, "w": 12, "h": 2},
                "options": {"mode": "markdown", "content": ["x"]}, "links": []
            })
        })
        .collect();
    v["sections"][0]["panels"] = Value::Array(panels);
    v["annotations"] = json!([{
        "name": "All", "source": "prod", "sql": ["SELECT 1 AS time"], "color": "red",
        "panels": (1..=200).collect::<Vec<i32>>()
    }]);
    let e = errors_of(&v);
    assert!(e.is_empty(), "{e:#?}");
    // The 201st item is `too_large`.
    v["annotations"][0]["panels"] = json!((1..=201).collect::<Vec<i32>>());
    let e = errors_of(&v);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::TooLarge, "/annotations/0/panels"));
}

#[test]
fn the_other_members_have_their_ranges() {
    let mut v = base();
    v["annotations"][0]["name"] = json!("M".repeat(256));
    v["annotations"][1]["name"] = json!("M");
    assert!(errors_of(&v).is_empty());
    v["annotations"][0]["enabled"] = json!(null);
    let e = errors_of(&v);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::WrongType, "/annotations/0/enabled"));
}

#[test]
fn an_old_reading_refuses_nothing_but_the_member_is_unknown_in_a_panel() {
    // The member is for the top-level object only.
    let mut v = base();
    v["sections"][0]["annotations"] = json!([]);
    let e = errors_of(&v);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UnknownMember, "/sections/0/annotations"));
}

#[test]
fn the_constants_equal_the_table_of_4_35() {
    assert_eq!(
        (
            MAX_ANNOTATIONS,
            MAX_ANNOTATION_PANELS,
            MAX_ANNOTATION_ROWS,
            MAX_ANNOTATIONS_PER_PANEL,
            MAX_ANNOTATION_TITLE_CHARS,
            MAX_ANNOTATION_TEXT_BYTES,
            MAX_ANNOTATION_TAGS,
            MAX_ANNOTATION_TAG_CHARS
        ),
        (8, 200, 1_000, 2_000, 256, 2_048, 8, 64)
    );
    assert_eq!(MAX_ANNOTATION_PANELS, MAX_PANELS, "it equals MAX_PANELS");
    assert_eq!(MAX_ANNOTATION_TITLE_CHARS, MAX_TITLE_CHARS, "it equals MAX_TITLE_CHARS");
    for row in [
        "| `MAX_ANNOTATIONS` | 8 |",
        "| `MAX_ANNOTATION_PANELS` | 200 |",
        "| `MAX_ANNOTATION_ROWS` | 1,000 |",
        "| `MAX_ANNOTATIONS_PER_PANEL` | 2,000 |",
        "| `MAX_ANNOTATION_TITLE_CHARS` | 256 |",
        "| `MAX_ANNOTATION_TEXT_BYTES` | 2,048 |",
        "| `MAX_ANNOTATION_TAGS`, `MAX_ANNOTATION_TAG_CHARS` | 8, 64 |",
    ] {
        assert!(DOC_FORMAT.contains(row), "docs/02 4.35 has no row {row}");
    }
}

// ------------------------------------------------------------- SQL texts

#[test]
fn the_sql_texts_list_the_annotations_after_the_variables() {
    let doc = validate(FIXTURE, None).unwrap();
    let texts = doc.document().sql_texts();
    let annotation_pointers: Vec<&str> =
        texts.iter().filter(|t| matches!(t.owner, SqlOwner::Annotation { .. })).map(|t| t.pointer.as_str()).collect();
    assert_eq!(annotation_pointers.len(), 8);
    assert_eq!(annotation_pointers[0], "/annotations/0/sql");
    assert_eq!(annotation_pointers[7], "/annotations/7/sql");
    assert_eq!(texts[0].owner, SqlOwner::Annotation { index: 0 }, "the fixture has no `query` variable");
    assert_eq!(texts[1].line_pointer(2), "/annotations/1/sql/2");
    assert_eq!(texts[0].text().lines().count(), 3);
}
