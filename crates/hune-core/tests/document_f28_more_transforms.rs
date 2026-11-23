// SPDX-License-Identifier: MIT OR Apache-2.0
//! The eleven transforms of the Grafana-parity work (F28, 4.25a, 4.34, 4.35, 4.38, 4.42): the fixture with its golden canonical
//! form, one refusal for each error, the bounds, the canonical order and the closed schema. The kinds join format 3
//! with no change of the version.

use hune_core::document::{
    BinaryOperator, Calc, ConvertTo, FORMAT_VERSION, JoinMode, LimitEnd, Operand, ReduceCalc, ReduceMode, Transform,
    UnaryOperator, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::MAX_TRANSFORM_ITEMS;
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/transforms_more.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

/// The pointer of the transforms of the first panel of the fixture.
const T: &str = "/sections/0/panels/0/transforms";

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

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

/// The transform `i` of the first panel.
fn t(v: &mut Value, i: usize) -> &mut Value {
    &mut v["sections"][0]["panels"][0]["transforms"][i]
}

fn names(prefix: &str, n: usize) -> Value {
    Value::Array((0..n).map(|i| json!(format!("{prefix}{i:02}"))).collect())
}

/// Each file of `invalid/f28_*.json` has one error: its name, its code and its pointer.
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("organize_hide_absent", ErrorCode::MissingMember, "/sections/0/panels/0/transforms/0"),
    ("organize_hide_not_array", ErrorCode::WrongType, "/sections/0/panels/0/transforms/0/hide"),
    ("organize_hide_65_items", ErrorCode::TooLarge, "/sections/0/panels/0/transforms/0/hide"),
    ("organize_hide_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/0/hide/1"),
    ("organize_order_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/0/order/2"),
    ("organize_rename_same_field", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/0/rename/1"),
    ("organize_rename_name_empty", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/0/rename/0/name"),
    ("organize_member_of_limit", ErrorCode::UnknownMember, "/sections/0/panels/0/transforms/0/count"),
    ("calc_mode_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/1/mode"),
    ("calc_object_absent", ErrorCode::MissingMember, "/sections/0/panels/0/transforms/1"),
    ("calc_two_objects", ErrorCode::UnknownMember, "/sections/0/panels/0/transforms/1/index"),
    ("calc_wrong_object", ErrorCode::MissingMember, "/sections/0/panels/0/transforms/1"),
    ("calc_alias_empty", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/1/alias"),
    ("calc_alias_257_chars", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/1/alias"),
    ("calc_replace_fields_text", ErrorCode::WrongType, "/sections/0/panels/0/transforms/1/replace_fields"),
    ("calc_binary_operator_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/1/binary/operator"),
    ("calc_binary_two_numbers", ErrorCode::NotPermitted, "/sections/0/panels/0/transforms/1/binary"),
    ("calc_operand_both_members", ErrorCode::UnknownMember, "/sections/0/panels/0/transforms/1/binary/left/value"),
    ("calc_operand_kind_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/1/binary/left/kind"),
    ("calc_operand_value_text", ErrorCode::WrongType, "/sections/0/panels/0/transforms/1/binary/right/value"),
    ("calc_unary_operator_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/1/unary/operator"),
    ("calc_reduce_include_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/1/reduce/include/1"),
    ("calc_index_as_percentile_absent", ErrorCode::MissingMember, "/sections/0/panels/0/transforms/1/index"),
    ("sort_empty", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/2/sort"),
    ("sort_9_keys", ErrorCode::TooLarge, "/sections/0/panels/0/transforms/2/sort"),
    ("sort_desc_text", ErrorCode::WrongType, "/sections/0/panels/0/transforms/2/sort/0/desc"),
    ("limit_count_minus_1", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/3/count"),
    ("limit_count_1000001", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/3/count"),
    ("limit_end_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/3/end"),
    ("merge_member_mode", ErrorCode::UnknownMember, "/sections/0/panels/0/transforms/4/mode"),
    ("reduce_mode_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/5/mode"),
    ("reduce_19_reducers", ErrorCode::TooLarge, "/sections/0/panels/0/transforms/5/reducers"),
    ("reduce_reducers_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/5/reducers/1"),
    ("partition_fields_empty", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/6/fields"),
    ("partition_5_fields", ErrorCode::TooLarge, "/sections/0/panels/0/transforms/6/fields"),
    ("partition_fields_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/6/fields/1"),
    ("partition_keep_fields_absent", ErrorCode::MissingMember, "/sections/0/panels/0/transforms/6"),
    ("join_mode_outer_tabular", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/7/mode"),
    ("filter_include_names_duplicate", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/8/include/names/1"),
    ("filter_include_pattern_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/8/include/pattern"),
    ("rename_regex_bad", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/9/regex"),
    ("rename_pattern_257_chars", ErrorCode::OutOfRange, "/sections/0/panels/0/transforms/9/rename_pattern"),
    ("convert_to_enum", ErrorCode::BadFormat, "/sections/0/panels/0/transforms/10/conversions/0/to"),
    ("convert_17_conversions", ErrorCode::TooLarge, "/sections/0/panels/0/transforms/10/conversions"),
    ("convert_duplicate_field", ErrorCode::NotUnique, "/sections/0/panels/0/transforms/10/conversions/1"),
];

#[test]
fn each_refusal_gives_its_errors() {
    for (name, code, pointer) in REFUSALS {
        let bytes =
            std::fs::read(invalid_dir().join(format!("f28_{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = validate(&bytes, None).unwrap_err();
        // A mode with the object of another mode has two errors: the object is absent, and the other is unknown.
        let expected = if *name == "calc_wrong_object" { 2 } else { 1 };
        assert_eq!(e.len(), expected, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
        if *name == "calc_wrong_object" {
            assert_eq!((e[1].code, e[1].pointer.as_str()), (ErrorCode::UnknownMember, format!("{T}/1/unary").as_str()));
        }
    }
}

#[test]
fn there_are_45_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f28_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("f28_{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 45);
}

#[test]
fn the_messages_name_the_rule_and_the_value() {
    let read = |name: &str| {
        let bytes = std::fs::read(invalid_dir().join(format!("f28_{name}.json"))).unwrap();
        validate(&bytes, None).unwrap_err()
    };
    let e = read("limit_count_1000001");
    assert!(e[0].message.contains("0 to 1000000") && e[0].message.contains("1000001"), "{}", e[0].message);
    let e = read("convert_17_conversions");
    assert!(e[0].message.contains("1 to 16") && e[0].message.contains("17"), "{}", e[0].message);
    let e = read("calc_binary_two_numbers");
    assert!(e[0].message.contains("operand") && e[0].message.contains("4.25a"), "{}", e[0].message);
    let e = read("filter_include_pattern_bad");
    assert!(e[0].message.contains("4.24") && e[0].message.contains("(?=a)"), "{}", e[0].message);
}

// ------------------------------------------------------------------ the fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), FIXTURE, "the golden canonical form");
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3, "the eleven kinds join format 3 with no new version");
}

#[test]
fn the_fixture_holds_each_kind_the_modes_and_the_bounds() {
    let doc = validate(FIXTURE, None).unwrap();
    let panels = &doc.document().sections[0].panels;
    assert_eq!(panels.len(), 3);
    let typical = &panels[0].transforms;
    assert_eq!(typical.len(), 11);
    assert!(matches!(typical[0], Transform::Organize { .. }));
    assert!(matches!(typical[1], Transform::CalculateField { .. }));
    assert!(matches!(typical[2], Transform::SortBy { .. }));
    assert!(matches!(typical[3], Transform::Limit { count: 0, end: LimitEnd::First }));
    assert!(matches!(typical[4], Transform::Merge));
    assert!(matches!(typical[5], Transform::Reduce { mode: ReduceMode::SeriesToRows, .. }));
    assert!(matches!(typical[6], Transform::PartitionByValues { keep_fields: false, .. }));
    assert!(matches!(typical[7], Transform::JoinByField { field: None, mode: JoinMode::Outer }));
    assert!(matches!(typical[8], Transform::FilterFieldsByName { .. }));
    assert!(matches!(typical[9], Transform::RenameByRegex { .. }));
    let Transform::ConvertFieldType { conversions } = &typical[10] else { panic!("kind") };
    assert_eq!(conversions[0].to, ConvertTo::Number);
    let Transform::CalculateField { calc, .. } = &typical[1] else { panic!("kind") };
    assert_eq!(
        *calc,
        Calc::Binary { left: Operand::Field("a".into()), right: Operand::Number(2.5), operator: BinaryOperator::Add }
    );
    // The modes.
    let modes = &panels[1].transforms;
    let calcs: Vec<&Calc> = modes
        .iter()
        .map(|t| match t {
            Transform::CalculateField { calc, .. } => calc,
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(matches!(calcs[0], Calc::ReduceRow { reducer: ReduceCalc::Total, .. }));
    assert!(matches!(calcs[1], Calc::Unary { operator: UnaryOperator::Abs, .. }));
    assert!(matches!(calcs[2], Calc::Index { as_percentile: true }));
    // The bounds.
    let bounds = &panels[2].transforms;
    assert!(matches!(bounds[3], Transform::Limit { count: 1_000_000, end: LimitEnd::Last }));
    let Transform::Organize { hide, order, rename } = &bounds[0] else { panic!("kind") };
    assert_eq!((hide.len(), order.len(), rename.len()), (64, 64, 64));
    assert_eq!(rename[0].field.len(), 1024);
    assert_eq!(rename[0].name.chars().count(), 256);
    let Transform::Reduce { reducers, .. } = &bounds[5] else { panic!("kind") };
    assert_eq!(reducers.len(), 18);
    let Transform::ConvertFieldType { conversions } = &bounds[10] else { panic!("kind") };
    assert_eq!(conversions.len(), 16);
    // Each calculation of `calc.mode()` has its member name in the canonical form.
    assert_eq!(calcs[0].mode().to_string(), "reduce_row");
}

#[test]
fn the_canonical_form_does_not_depend_on_the_order_of_the_members() {
    let mut v = base();
    // Write the members of a `calculate_field`, of an operand and of a `limit` in another order.
    *t(&mut v, 1) = json!({
        "binary": {"operator": "add", "right": {"value": 2.5, "kind": "number"}, "left": {"field": "a", "kind": "field"}},
        "replace_fields": false, "alias": "sum_ab", "mode": "binary", "kind": "calculate_field"
    });
    *t(&mut v, 3) = json!({"end": "first", "count": 0, "kind": "limit"});
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let again = validate(FIXTURE, None).unwrap();
    assert_eq!(doc.bytes(), again.bytes());
    assert_eq!(doc.etag(), again.etag());
}

#[test]
fn a_document_with_no_new_kind_keeps_its_bytes() {
    for (name, bytes) in [("minimal", MINIMAL), ("all_types", ALL_TYPES)] {
        let doc = validate(bytes, None).unwrap_or_else(|e| panic!("{name}: {e:#?}"));
        assert_eq!(doc.bytes(), bytes, "{name}");
    }
}

// ------------------------------------------------------------------ bounds and rules

#[test]
fn the_arrays_have_their_bounds() {
    let mut v = base();
    // At the bound: valid.
    t(&mut v, 0)["hide"] = names("h", MAX_TRANSFORM_ITEMS);
    t(&mut v, 0)["order"] = names("o", MAX_TRANSFORM_ITEMS);
    t(&mut v, 8)["include"] = json!({"names": names("n", MAX_TRANSFORM_ITEMS), "pattern": null});
    t(&mut v, 8)["exclude"] = json!({"names": names("x", MAX_TRANSFORM_ITEMS), "pattern": null});
    assert_eq!(errors_of(&v), vec![]);
    // One above the bound.
    for (i, member, pointer) in [(0, "hide", "hide"), (0, "order", "order")] {
        let mut w = v.clone();
        t(&mut w, i)[member] = names("z", MAX_TRANSFORM_ITEMS + 1);
        assert_eq!(one_error(&w), (ErrorCode::TooLarge, format!("{T}/{i}/{pointer}")));
    }
    let mut w = v.clone();
    t(&mut w, 8)["exclude"]["names"] = names("z", MAX_TRANSFORM_ITEMS + 1);
    assert_eq!(one_error(&w), (ErrorCode::TooLarge, format!("{T}/8/exclude/names")));
    // The sort has 1 to 8 keys, the reducers 1 to 18, the partition 1 to 4, the conversions 1 to 16.
    let mut w = base();
    t(&mut w, 5)["reducers"] = json!(ReduceCalc::ALL.iter().map(|r| r.to_string()).collect::<Vec<_>>());
    assert_eq!(errors_of(&w), vec![]);
    t(&mut w, 6)["fields"] = json!(["a", "b", "c", "d"]);
    assert_eq!(errors_of(&w), vec![]);
    t(&mut w, 10)["conversions"] =
        Value::Array((0..16).map(|i| json!({"field": format!("c{i}"), "to": "text"})).collect());
    assert_eq!(errors_of(&w), vec![]);
}

#[test]
fn the_limit_count_has_its_two_bounds() {
    for (count, ok) in [(0, true), (1, true), (1_000_000, true), (-1, false), (1_000_001, false)] {
        let mut v = base();
        t(&mut v, 3)["count"] = json!(count);
        if ok {
            assert_eq!(errors_of(&v), vec![], "{count}");
        } else {
            assert_eq!(one_error(&v), (ErrorCode::OutOfRange, format!("{T}/3/count")), "{count}");
        }
    }
    let mut v = base();
    t(&mut v, 3)["count"] = json!(1e6);
    assert_eq!(one_error(&v), (ErrorCode::WrongType, format!("{T}/3/count")), "an exponent is not an integer");
}

#[test]
fn the_names_have_their_bounds() {
    let mut v = base();
    t(&mut v, 7)["field"] = json!("k".repeat(1024));
    assert_eq!(errors_of(&v), vec![]);
    t(&mut v, 7)["field"] = json!("k".repeat(1025));
    assert_eq!(one_error(&v), (ErrorCode::OutOfRange, format!("{T}/7/field")));
    // The alias and a new name count characters, not bytes: 256 two-byte characters are valid.
    let mut v = base();
    t(&mut v, 1)["alias"] = json!("\u{e9}".repeat(256));
    t(&mut v, 0)["rename"] = json!([{"field": "a", "name": "\u{e9}".repeat(256)}]);
    t(&mut v, 9)["rename_pattern"] = json!("");
    assert_eq!(errors_of(&v), vec![]);
    t(&mut v, 1)["alias"] = json!("\u{e9}".repeat(257));
    assert_eq!(one_error(&v), (ErrorCode::OutOfRange, format!("{T}/1/alias")));
}

#[test]
fn a_control_character_in_a_name_or_a_text_is_refused() {
    for (i, path) in [(0, "rename"), (9, "rename_pattern")] {
        let mut v = base();
        if path == "rename" {
            t(&mut v, 0)["rename"] = json!([{"field": "a", "name": "x\ny"}]);
        } else {
            t(&mut v, i)["rename_pattern"] = json!("x\ty");
        }
        let (code, _) = one_error(&v);
        assert_eq!(code, ErrorCode::BadFormat, "{path}");
    }
}

#[test]
fn the_patterns_follow_4_24() {
    for bad in ["(a", "[a", "(?<=a)b", "\\1", "(?=a)"] {
        let mut v = base();
        t(&mut v, 9)["regex"] = json!(bad);
        assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{T}/9/regex")), "{bad}");
    }
    for good in ["x|y", "^a.*b$", "[a-z]+", "(a|b)c"] {
        let mut v = base();
        t(&mut v, 9)["regex"] = json!(good);
        t(&mut v, 8)["include"]["pattern"] = json!(good);
        assert_eq!(errors_of(&v), vec![], "{good}");
    }
    let mut v = base();
    t(&mut v, 9)["regex"] = json!("");
    assert_eq!(one_error(&v), (ErrorCode::OutOfRange, format!("{T}/9/regex")));
}

#[test]
fn a_binary_with_a_field_on_one_side_is_valid_and_with_two_numbers_is_not() {
    let mut v = base();
    let object = |left: Value, right: Value| {
        json!({"kind": "calculate_field", "mode": "binary", "alias": null, "replace_fields": false,
            "binary": {"left": left, "right": right, "operator": "divide"}})
    };
    let field = || json!({"kind": "field", "field": "a"});
    let number = |n: f64| json!({"kind": "number", "value": n});
    for (l, r) in [(field(), field()), (field(), number(0.0)), (number(-3.5), field())] {
        *t(&mut v, 1) = object(l, r);
        assert_eq!(errors_of(&v), vec![]);
    }
    *t(&mut v, 1) = object(number(1.0), number(2.0));
    assert_eq!(one_error(&v), (ErrorCode::NotPermitted, format!("{T}/1/binary")));
}

#[test]
fn a_member_of_another_kind_is_unknown_for_each_kind() {
    let kinds = [
        (0, "organize"),
        (1, "calculate_field"),
        (2, "sort_by"),
        (3, "limit"),
        (4, "merge"),
        (5, "reduce"),
        (6, "partition_by_values"),
        (7, "join_by_field"),
        (8, "filter_fields_by_name"),
        (9, "rename_by_regex"),
        (10, "convert_field_type"),
    ];
    for (i, kind) in kinds {
        let mut v = base();
        assert_eq!(t(&mut v, i)["kind"], kind);
        t(&mut v, i)["options"] = json!({});
        assert_eq!(one_error(&v), (ErrorCode::UnknownMember, format!("{T}/{i}/options")), "{kind}");
    }
}

#[test]
fn an_unknown_kind_is_bad_format_at_the_kind() {
    let mut v = base();
    t(&mut v, 4)["kind"] = json!("prepare_time_series");
    assert_eq!(one_error(&v), (ErrorCode::BadFormat, format!("{T}/4/kind")));
}

#[test]
fn a_duplicate_member_is_refused_in_a_new_kind() {
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap().replacen(
        "\"end\": \"first\"",
        "\"end\": \"first\",\n \"end\": \"last\"",
        1,
    );
    let e = validate(text.as_bytes(), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(
        (e[0].code, e[0].pointer.as_str()),
        (ErrorCode::DuplicateMember, "/sections/0/panels/0/transforms/3/end")
    );
}

#[test]
fn the_sixteen_transforms_of_a_panel_hold_the_new_kinds() {
    let mut v = base();
    let items = vec![v["sections"][0]["panels"][0]["transforms"][4].clone(); 16];
    v["sections"][0]["panels"][0]["transforms"] = Value::Array(items);
    assert_eq!(errors_of(&v), vec![]);
    v["sections"][0]["panels"][0]["transforms"].as_array_mut().unwrap().push(json!({"kind": "merge"}));
    assert_eq!(one_error(&v), (ErrorCode::TooLarge, T.to_owned()));
}

// ------------------------------------------------------------------ the table of 4.35

/// The table of 4.35 and `limits.rs` give the same value for each limit of 4.25a.
#[test]
fn the_table_of_4_35_equals_the_limits_of_the_code() {
    use hune_core::limits::{
        MAX_MERGE_STEPS, MAX_TRANSFORM_FIELDS, MAX_TRANSFORM_FRAMES, MAX_TRANSFORM_ITEMS, MAX_TRANSFORM_ROWS,
    };
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/02-format-and-storage.md"),
    )
    .unwrap();
    let thousands = |n: usize| {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    for (name, value) in [
        ("MAX_TRANSFORM_ITEMS", MAX_TRANSFORM_ITEMS),
        ("MAX_TRANSFORM_ROWS", MAX_TRANSFORM_ROWS),
        ("MAX_TRANSFORM_FIELDS", MAX_TRANSFORM_FIELDS),
        ("MAX_TRANSFORM_FRAMES", MAX_TRANSFORM_FRAMES),
        ("MAX_MERGE_STEPS", MAX_MERGE_STEPS),
    ] {
        let row = format!("| `{name}` | {} |", thousands(value));
        assert!(spec.contains(&row), "the table of 4.35 has no row {row}");
    }
}
