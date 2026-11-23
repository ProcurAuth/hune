// SPDX-License-Identifier: MIT OR Apache-2.0
//! The member `allow_custom_value` of a `query` and of a `custom` variable (F41, 4.7 to 4.11, 4.38, 4.42):
//! the fixture with its golden canonical form, one refusal for each error, the typed default of an open
//! `custom` variable and the closed schema.

use hune_core::document::{VariableKind, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/open_variables.json");

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

/// Each file of `invalid/f41_*.json` has one error: its file name, its code and its pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f41_allow_on_text", ErrorCode::UnknownMember, "/variables/4/allow_custom_value"),
    ("f41_allow_text_value", ErrorCode::WrongType, "/variables/1/allow_custom_value"),
    ("f41_allow_number_value", ErrorCode::WrongType, "/variables/3/allow_custom_value"),
    ("f41_allow_null", ErrorCode::WrongType, "/variables/0/allow_custom_value"),
    ("f41_allow_duplicate_member", ErrorCode::DuplicateMember, "/variables/0/allow_custom_value"),
    ("f41_open_default_1025_bytes", ErrorCode::OutOfRange, "/variables/2/default/1"),
    ("f41_open_default_control_character", ErrorCode::BadFormat, "/variables/2/default/1"),
    ("f41_open_default_empty", ErrorCode::OutOfRange, "/variables/2/default/1"),
    ("f41_closed_default_not_an_option", ErrorCode::NotPermitted, "/variables/3/default/0"),
    ("f41_closed_false_default_not_an_option", ErrorCode::NotPermitted, "/variables/3/default/0"),
];

#[test]
fn the_fixture_is_valid_and_its_bytes_are_the_canonical_form() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE);
    // One open and one closed variable of each kind.
    let open: Vec<bool> = doc.document().variables().iter().map(|v| v.kind.is_open()).collect();
    assert_eq!(open, [true, false, true, false, false]);
}

#[test]
fn the_member_follows_default_and_a_false_is_not_in_the_canonical_bytes() {
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap();
    // Two `true` in the fixture: one on a `query` and one on a `custom` variable. The text has two other
    // variables that are closed: `region_closed` and `env_closed`.
    assert_eq!(text.matches("\"allow_custom_value\": true").count(), 2);
    assert!(!text.contains("\"allow_custom_value\": false"));
    for name in ["region_open", "env_open"] {
        let from = text.find(&format!("\"name\": \"{name}\"")).unwrap();
        let end = from + text[from..].find("\"allow_custom_value\"").unwrap();
        let default_at = from + text[from..].find("\"default\"").unwrap();
        assert!(default_at < end, "{name}: the member comes after default");
    }
    // A document with `false` has the same bytes and the same etag as one with no member.
    let mut v = base();
    v["variables"][1]["allow_custom_value"] = json!(false);
    v["variables"][3]["allow_custom_value"] = json!(false);
    let with_false = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let plain = validate(FIXTURE, None).unwrap();
    assert_eq!(with_false.bytes(), plain.bytes());
    assert_eq!(with_false.etag(), plain.etag());
    // The kind keeps the value as a boolean.
    assert!(!with_false.document().variables()[1].kind.is_open());
}

#[test]
fn the_member_comes_before_placement() {
    let mut v = base();
    v["variables"][0]["placement"] = json!({"section": 0, "x": 12, "y": 0, "w": 12, "h": 2});
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let text = String::from_utf8(doc.bytes().to_vec()).unwrap();
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle}"));
    let from = at("\"name\": \"region_open\"");
    let tail = &text[from..];
    let (d, a, p) = (
        tail.find("\"default\"").unwrap(),
        tail.find("\"allow_custom_value\"").unwrap(),
        tail.find("\"placement\"").unwrap(),
    );
    assert!(d < a && a < p, "default, allow_custom_value, placement");
}

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
        .filter(|n| n.starts_with("f41_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 10);
}

#[test]
fn the_rule_of_the_default_follows_the_member() {
    // A closed `custom` variable: each default item is an option (4.11).
    let mut v = base();
    v["variables"][3]["default"] = json!(["staging"]);
    assert!(errors_of(&v).is_empty());
    v["variables"][3]["default"] = json!(["typed"]);
    let e = errors_of(&v);
    assert_eq!((e.len(), e[0].code), (1, ErrorCode::NotPermitted));
    // An open `custom` variable: a typed default of 1 to 1,024 bytes with no control character.
    let mut v = base();
    for ok in ["x", "typed value", &"a".repeat(1024), "a\tb".replace('\t', " ").as_str(), "\u{e9}"] {
        v["variables"][2]["default"] = json!(["prod", ok]);
        assert!(errors_of(&v).is_empty(), "{ok:?}: {:#?}", errors_of(&v));
    }
    // `$__all` keeps its rule: it needs `include_all` and it is the only item.
    v["variables"][2]["default"] = json!(["$__all", "typed"]);
    let e = errors_of(&v);
    assert_eq!((e.len(), e[0].code), (1, ErrorCode::NotPermitted), "{e:#?}");
    let mut v = base();
    v["variables"][2]["include_all"] = json!(false);
    v["variables"][2]["default"] = json!(["$__all"]);
    assert_eq!(errors_of(&v)[0].code, ErrorCode::NotPermitted);
    // A control character in a closed default is refused as `bad_format` first.
    let mut v = base();
    v["variables"][3]["default"] = json!(["a\u{7}"]);
    assert_eq!(errors_of(&v)[0].code, ErrorCode::BadFormat);
}

#[test]
fn a_member_with_a_wrong_type_does_not_add_errors_for_the_default() {
    // The default of a `custom` variable is read with the rule of the closed variable only when the member says so.
    let mut v = base();
    v["variables"][2]["allow_custom_value"] = json!("yes");
    v["variables"][2]["default"] = json!(["typed"]);
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::WrongType, "/variables/2/allow_custom_value"));
}

#[test]
fn the_model_reads_the_member() {
    let doc = validate(FIXTURE, None).unwrap();
    let kinds: Vec<(&str, bool)> =
        doc.document()
            .variables()
            .iter()
            .map(|v| {
                let open = match &v.kind {
                    VariableKind::Query { allow_custom_value, .. }
                    | VariableKind::Custom { allow_custom_value, .. } => *allow_custom_value,
                    VariableKind::Text { .. }
                    | VariableKind::Interval { .. }
                    | VariableKind::Constant { .. }
                    | VariableKind::Adhoc { .. } => false,
                };
                (v.name.as_str(), open)
            })
            .collect();
    assert_eq!(
        kinds,
        [("region_open", true), ("region_closed", false), ("env_open", true), ("env_closed", false), ("note", false)]
    );
}

#[test]
fn a_document_without_the_member_keeps_its_bytes() {
    // 4.42: no migration. The other fixtures have no member and stay valid and canonical.
    for bytes in [
        &include_bytes!("../../../tests/fixtures/dashboards/nodes.json")[..],
        include_bytes!("../../../tests/fixtures/dashboards/repeats.json"),
        include_bytes!("../../../tests/fixtures/dashboards/all_types.json"),
    ] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(doc.bytes(), bytes);
        assert!(!String::from_utf8_lossy(bytes).contains("allow_custom_value"));
    }
}
