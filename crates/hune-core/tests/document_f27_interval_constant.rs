// SPDX-License-Identifier: MIT OR Apache-2.0
//! The variable kinds `interval` and `constant` in the document (F27, 4.7, 4.10a, 4.10b, 4.34, 4.35, 4.38, 4.42):
//! the fixture with its golden canonical form, one refusal for each error, the bounds and the closed schema.

use hune_core::document::{VariableKind, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::MAX_INTERVALS;
use hune_core::sql::AUTO_TABLE;
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/variable_kinds.json");

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

/// Each file of `invalid/f27_*.json` has one error: its file name, its code and its pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f27_auto_absent", ErrorCode::MissingMember, "/variables/0"),
    ("f27_auto_count_0", ErrorCode::OutOfRange, "/variables/0/auto_count"),
    ("f27_auto_count_1001", ErrorCode::OutOfRange, "/variables/0/auto_count"),
    ("f27_auto_count_fraction", ErrorCode::WrongType, "/variables/0/auto_count"),
    ("f27_auto_min_90s", ErrorCode::BadFormat, "/variables/0/auto_min"),
    ("f27_auto_min_week", ErrorCode::BadFormat, "/variables/0/auto_min"),
    ("f27_constant_1025_bytes", ErrorCode::OutOfRange, "/variables/3/value"),
    ("f27_constant_control_character", ErrorCode::BadFormat, "/variables/2/value"),
    ("f27_constant_default_member", ErrorCode::UnknownMember, "/variables/2/default"),
    ("f27_constant_hide_label", ErrorCode::NotPermitted, "/variables/2/hide"),
    ("f27_constant_hide_none", ErrorCode::NotPermitted, "/variables/2/hide"),
    ("f27_constant_placement", ErrorCode::NotPermitted, "/variables/2/placement"),
    ("f27_constant_value_absent", ErrorCode::MissingMember, "/variables/2"),
    ("f27_default_array", ErrorCode::WrongType, "/variables/0/default"),
    ("f27_default_auto_with_auto_false", ErrorCode::NotPermitted, "/variables/1/default"),
    ("f27_default_not_an_item", ErrorCode::NotPermitted, "/variables/0/default"),
    ("f27_interval_multi_member", ErrorCode::UnknownMember, "/variables/0/multi"),
    ("f27_intervals_51", ErrorCode::TooLarge, "/variables/0/intervals"),
    ("f27_intervals_empty", ErrorCode::OutOfRange, "/variables/0/intervals"),
    ("f27_intervals_repeated", ErrorCode::NotUnique, "/variables/0/intervals/2"),
    ("f27_item_366_days", ErrorCode::OutOfRange, "/variables/0/intervals/1"),
    ("f27_item_capital_m", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_item_decimal", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_item_empty", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_item_negative", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_item_number", ErrorCode::WrongType, "/variables/0/intervals/1"),
    ("f27_item_space", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_item_zero", ErrorCode::BadFormat, "/variables/0/intervals/1"),
    ("f27_repeat_names_constant", ErrorCode::NotPermitted, "/sections/0/panels/0/repeat/variable"),
    ("f27_repeat_names_interval", ErrorCode::NotPermitted, "/sections/0/panels/0/repeat/variable"),
];

#[test]
fn the_fixture_is_valid_and_its_bytes_are_the_canonical_form() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE);
    let kinds: Vec<String> = doc.document().variables().iter().map(|v| v.kind.tag().to_string()).collect();
    assert_eq!(kinds, ["interval", "interval", "constant", "constant"]);
    match &doc.document().variables()[1].kind {
        VariableKind::Interval { intervals, auto, auto_count, auto_min, default } => {
            assert_eq!(intervals.len(), MAX_INTERVALS);
            assert_eq!((*auto, *auto_count, auto_min.as_str(), default), (false, 1000, "1y", &None));
        }
        other => panic!("{other:?}"),
    }
    match &doc.document().variables()[3].kind {
        VariableKind::Constant { value } => assert_eq!(value.len(), 1024),
        other => panic!("{other:?}"),
    }
    assert!(doc.document().variables()[0].placement.is_some());
    assert!(doc.document().variables()[2].placement.is_none());
}

#[test]
fn the_members_come_in_the_order_of_4_38() {
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap();
    let from = text.find("\"name\": \"bucket\"").unwrap();
    let tail = &text[from..];
    let at = |n: &str| tail.find(&format!("\"{n}\"")).unwrap_or_else(|| panic!("{n}"));
    let order = ["intervals", "auto", "auto_count", "auto_min", "default", "placement"].map(at);
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    // `auto_count` and `auto_min` stay when `auto` is false, and `default` stays when it is null.
    let from = text.find("\"name\": \"window\"").unwrap();
    let tail = &text[from..text.find("\"name\": \"empty_note\"").unwrap()];
    assert!(tail.contains("\"auto\": false") && tail.contains("\"auto_count\": 1000"));
    assert!(tail.contains("\"auto_min\": \"1y\"") && tail.contains("\"default\": null"));
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
        .filter(|n| n.starts_with("f27_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 30);
}

#[test]
fn the_bounds_of_an_interval_variable() {
    // 1 item and 50 items; `auto_count` 1 and 1,000; every text of the table of 7.4a as `auto_min`.
    let mut v = base();
    v["variables"][0]["intervals"] = json!(["1ms"]);
    v["variables"][0]["default"] = Value::Null;
    assert!(errors_of(&v).is_empty());
    for count in [1, 1000] {
        v["variables"][0]["auto_count"] = json!(count);
        assert!(errors_of(&v).is_empty(), "{count}");
    }
    for (_, text) in AUTO_TABLE {
        v["variables"][0]["auto_min"] = json!(text);
        assert!(errors_of(&v).is_empty(), "{text}");
    }
    // The items at the bounds of the grammar: 9 digits, 365 days in every unit.
    for item in ["999999999ms", "999999999s", "1ms", "8760h", "525600m", "365d", "52w", "1y", "31536000s"] {
        let mut v = base();
        v["variables"][0]["intervals"] = json!([item]);
        v["variables"][0]["default"] = Value::Null;
        let e = errors_of(&v);
        // 999,999,999 s is more than 365 days: the others are valid.
        if item == "999999999s" {
            assert_eq!((e.len(), e[0].code), (1, ErrorCode::OutOfRange), "{item}");
        } else {
            assert!(e.is_empty(), "{item}: {e:#?}");
        }
    }
    // The text of the table of 7.4a that is not an item of the grammar does not exist: each one is an item.
    for (_, text) in AUTO_TABLE {
        let mut v = base();
        v["variables"][0]["intervals"] = json!([text]);
        v["variables"][0]["default"] = Value::Null;
        assert!(errors_of(&v).is_empty(), "{text}");
    }
}

#[test]
fn the_default_of_an_interval_variable() {
    let mut v = base();
    for ok in [json!(null), json!("$__auto"), json!("10m"), json!("1d")] {
        v["variables"][0]["default"] = ok.clone();
        assert!(errors_of(&v).is_empty(), "{ok}");
    }
    for bad in [json!("7m"), json!(""), json!("$__AUTO"), json!("$__auto_interval_bucket"), json!(5), json!(["1m"])] {
        v["variables"][0]["default"] = bad.clone();
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{bad}: {e:#?}");
        assert_eq!(e[0].pointer, "/variables/0/default", "{bad}");
    }
    // `$__auto` needs `auto` = true.
    let mut v = base();
    v["variables"][0]["auto"] = json!(false);
    let e = errors_of(&v);
    assert_eq!((e.len(), e[0].code), (1, ErrorCode::NotPermitted));
    v["variables"][0]["default"] = json!("1h");
    assert!(errors_of(&v).is_empty());
    // An item that is not valid gives its own error and no false error for `default`.
    let mut v = base();
    v["variables"][0]["intervals"] = json!(["1m", "1M"]);
    v["variables"][0]["default"] = json!("1M");
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(e[0].pointer, "/variables/0/intervals/1");
}

#[test]
fn a_constant_value_has_0_to_1024_bytes_and_no_control_character() {
    for ok in ["", "x", "a'b\"c$d\\e;--x", "\u{e9}", "${x} $__from", &"a".repeat(1024)] {
        let mut v = base();
        v["variables"][2]["value"] = json!(ok);
        assert!(errors_of(&v).is_empty(), "{ok:?}");
    }
    // 512 two-byte characters are 1,024 bytes. 513 are 1,026 bytes.
    let mut v = base();
    v["variables"][2]["value"] = json!("\u{e9}".repeat(512));
    assert!(errors_of(&v).is_empty());
    v["variables"][2]["value"] = json!("\u{e9}".repeat(513));
    assert_eq!(errors_of(&v)[0].code, ErrorCode::OutOfRange);
    for c in ['\u{0}', '\u{1f}', '\u{9}', '\u{a}', '\u{d}', '\u{7f}'] {
        let mut v = base();
        v["variables"][2]["value"] = json!(format!("a{c}b"));
        let e = errors_of(&v);
        assert_eq!((e.len(), e[0].code), (1, ErrorCode::BadFormat), "{:?}", c);
    }
}

#[test]
fn a_constant_has_hide_variable_and_no_placement() {
    let mut v = base();
    assert!(errors_of(&v).is_empty());
    for hide in ["none", "label"] {
        v["variables"][2]["hide"] = json!(hide);
        let e = errors_of(&v);
        assert_eq!((e.len(), e[0].code, e[0].pointer.as_str()), (1, ErrorCode::NotPermitted, "/variables/2/hide"));
    }
    v["variables"][2]["hide"] = json!("bad");
    assert_eq!(errors_of(&v)[0].code, ErrorCode::BadFormat);
    // An `interval` variable can have any `hide`. A placement needs `hide` other than `variable`.
    let mut v = base();
    for hide in ["none", "label", "variable"] {
        v["variables"][1]["hide"] = json!(hide);
        assert!(errors_of(&v).is_empty(), "{hide}");
    }
}

#[test]
fn a_document_without_the_kinds_keeps_its_bytes() {
    // 4.42: no migration. The other fixtures stay valid and canonical.
    for bytes in [
        &include_bytes!("../../../tests/fixtures/dashboards/nodes.json")[..],
        include_bytes!("../../../tests/fixtures/dashboards/open_variables.json"),
        include_bytes!("../../../tests/fixtures/dashboards/repeats.json"),
        include_bytes!("../../../tests/fixtures/dashboards/all_types.json"),
    ] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(doc.bytes(), bytes);
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("\"interval\"") || text.contains("\"min_interval\""));
        assert!(!text.contains("\"kind\": \"constant\""));
    }
}

#[test]
fn the_same_bytes_give_the_same_etag_and_a_second_write_is_stable() {
    let a = validate(FIXTURE, None).unwrap();
    let again = validate(a.bytes(), None).unwrap();
    assert_eq!(a.etag(), again.etag());
    assert_eq!(a.bytes(), again.bytes());
    // A compact copy of the same document has the same canonical bytes.
    let compact = serde_json::to_vec(&base()).unwrap();
    let c = validate(&compact, None).unwrap();
    assert_eq!(c.bytes(), FIXTURE);
    assert_eq!(c.etag(), a.etag());
}

#[test]
fn the_members_of_another_kind_are_unknown_members() {
    let members: [(usize, &str, Value); 8] = [
        (0, "multi", json!(false)),
        (0, "include_all", json!(false)),
        (0, "options", json!([])),
        (0, "value", json!("x")),
        (0, "allow_custom_value", json!(true)),
        (2, "intervals", json!(["1m"])),
        (2, "auto", json!(false)),
        (3, "default", json!(null)),
    ];
    for (i, member, value) in members {
        let mut v = base();
        v["variables"][i][member] = value;
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{member}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.clone()), (ErrorCode::UnknownMember, format!("/variables/{i}/{member}")));
    }
    // A duplicate member stops the read.
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap().replacen(
        "\"auto\": true,",
        "\"auto\": true,\n      \"auto\": true,",
        1,
    );
    let e = validate(text.as_bytes(), None).unwrap_err();
    assert_eq!(e[0].code, ErrorCode::DuplicateMember);
}

#[test]
fn the_kinds_are_new_values_of_format_3() {
    // The fixture says `format_version` 3, and there is no format 4.
    assert_eq!(base()["format_version"], json!(3));
    let mut v = base();
    v["format_version"] = json!(4);
    assert!(!errors_of(&v).is_empty());
    // An unknown kind is `bad_format` at the member `kind`, as an old binary gives for these two kinds.
    let mut v = base();
    v["variables"][0]["kind"] = json!("datasource");
    let e = errors_of(&v);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::BadFormat, "/variables/0/kind"));
}

#[test]
fn a_repeat_cannot_name_the_new_kinds() {
    for name in ["bucket", "window", "empty_note", "long_note"] {
        let mut v = base();
        v["sections"][0]["panels"][0]["repeat"] = json!({"variable": name, "direction": "v"});
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!(
            (e[0].code, e[0].pointer.as_str()),
            (ErrorCode::NotPermitted, "/sections/0/panels/0/repeat/variable")
        );
    }
}
