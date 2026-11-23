//! The migrations of the document format (4.42): golden pairs, errors and purity.
//!
//! The golden files are in `tests/fixtures/dashboards/`: `format1/` holds the inputs of the first
//! change, `format2/` holds its outputs and the inputs of the second change, and the folder
//! itself holds the outputs of the second change (format 3).

use hune_core::document::{FORMAT_VERSION, migrate_1_to_2, migrate_1_to_3, migrate_2_to_3, validate};
use hune_core::error::ErrorCode;
use hune_core::etag::Etag;
use hune_core::ids::Uid;
use hune_core::limits::MAX_ERRORS;
use serde_json::{Value, json};

const IN1_MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format1/minimal.json");
const IN1_NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format1/nodes.json");
const IN1_ALL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format1/all_types.json");
const IN1_ALL_COMPACT: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format1/all_types.compact.json");

const V2_MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/minimal.json");
const V2_NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/nodes.json");
const V2_ALL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/all_types.json");
const V2_ALL_COMPACT: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/all_types.compact.json");
const V2_F_MEMBERS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/f_members.json");

const OUT_MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const OUT_NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const OUT_ALL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const OUT_ALL_COMPACT: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.compact.json");
const OUT_F_MEMBERS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/f_members.json");

/// The files of `format1/` with the output that the chain gives for each of them.
const CHAIN: [(&[u8], &[u8]); 4] =
    [(IN1_MINIMAL, OUT_MINIMAL), (IN1_NODES, OUT_NODES), (IN1_ALL, OUT_ALL), (IN1_ALL_COMPACT, OUT_ALL)];

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

/// The description of `all_types` names the format. The input of format 1 says "format 1", and
/// the files of the next formats keep that text, because a migration changes no text.
fn with_format_1_description(bytes: &[u8]) -> String {
    text(bytes).replace("format 2 at least once", "format 1 at least once")
}

fn json_of(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn the_format_is_3() {
    assert_eq!(FORMAT_VERSION, 3);
}

// ------------------------------------------------------------- 1 to 2

#[test]
fn golden_1_to_2_minimal() {
    let out = migrate_1_to_2(IN1_MINIMAL, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(text(&out), text(V2_MINIMAL));
}

#[test]
fn golden_1_to_2_nodes() {
    let uid = Uid::parse("nodes").unwrap();
    let out = migrate_1_to_2(IN1_NODES, Some(&uid)).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(text(&out), text(V2_NODES));
    // The only changes are the version and one member in each of the 3 panels.
    assert_eq!(text(&out).lines().count(), text(IN1_NODES).lines().count() + 3);
}

#[test]
fn golden_1_to_2_all_types() {
    let out = migrate_1_to_2(IN1_ALL, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(text(&out), with_format_1_description(V2_ALL));
}

#[test]
fn golden_1_to_2_compact_gives_canonical_bytes() {
    let out = migrate_1_to_2(IN1_ALL_COMPACT, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(text(&out), with_format_1_description(V2_ALL));
    // The compact file of format 2 is not canonical, but it migrates to the canonical file.
    let again = migrate_2_to_3(V2_ALL_COMPACT, None).unwrap();
    assert_eq!(text(again.bytes()), text(OUT_ALL));
}

#[test]
fn every_panel_gets_false_and_the_rest_is_unchanged() {
    let out = migrate_1_to_2(IN1_ALL, None).unwrap();
    let mut old = json_of(IN1_ALL);
    old["format_version"] = json!(2);
    for section in old["sections"].as_array_mut().unwrap() {
        for panel in section["panels"].as_array_mut().unwrap() {
            panel.as_object_mut().unwrap().insert("transparent".to_owned(), json!(false));
        }
    }
    assert_eq!(json_of(&out), old);
}

#[test]
fn the_migration_1_to_2_is_pure() {
    let a = migrate_1_to_2(IN1_NODES, None).unwrap();
    let b = migrate_1_to_2(IN1_NODES, None).unwrap();
    assert_eq!(a, b);
    assert_eq!(Etag::of(&a), Etag::of(&b));
}

#[test]
fn a_document_that_is_not_format_1_gives_format_version() {
    for (input, message) in [
        (V2_NODES, "not loadable: format 2"),
        (OUT_NODES, "not loadable: format 3"),
        (b"{\"format_version\": 1.0}".as_slice(), "not loadable: format 1.0"),
        (b"{\"title\": \"a\"}".as_slice(), "not loadable: format_version is absent"),
    ] {
        for result in [migrate_1_to_2(input, None).map(|_| ()), migrate_1_to_3(input, None).map(|_| ())] {
            let e = result.unwrap_err();
            assert_eq!(e.len(), 1, "{message}");
            assert_eq!(
                (e[0].code, e[0].pointer.as_str(), e[0].message.as_str()),
                (ErrorCode::FormatVersion, "/format_version", message)
            );
        }
    }
}

#[test]
fn a_panel_that_already_has_transparent_is_not_a_format_1_panel() {
    let mut v = json_of(IN1_NODES);
    v["sections"][1]["panels"][0].as_object_mut().unwrap().insert("transparent".to_owned(), json!(true));
    let e = migrate_1_to_2(&serde_json::to_vec(&v).unwrap(), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UnknownMember, "/sections/1/panels/0/transparent"));
}

/// A document of format 1 whose first section has `count` panels that already have `transparent`, and `links`.
/// The steps find their errors before the validation, so a panel needs no other member.
fn transparent_panels(count: usize) -> Vec<u8> {
    let mut v = json_of(IN1_NODES);
    v["sections"][0]["panels"] = Value::Array(vec![json!({"transparent": true}); count]);
    v["links"] = json!([]);
    serde_json::to_vec(&v).unwrap()
}

#[test]
fn d7_the_errors_of_the_steps_have_the_cap_of_4_36_step_7() {
    // D7 (c) of the security review of 2026-11-16: the list of the first step had no cap. A long list stops at
    // `MAX_ERRORS` items, and the last item is `too_many_errors`.
    for count in [MAX_ERRORS - 1, MAX_ERRORS, 20_000] {
        let bytes = transparent_panels(count);
        for e in [migrate_1_to_2(&bytes, None).unwrap_err(), migrate_1_to_3(&bytes, None).unwrap_err()] {
            assert_eq!(e.len(), MAX_ERRORS, "{count}");
            for (i, x) in e[..MAX_ERRORS - 1].iter().enumerate() {
                assert_eq!(
                    (x.code, x.pointer.clone()),
                    (ErrorCode::UnknownMember, format!("/sections/0/panels/{i}/transparent"))
                );
            }
            let last = e.last().unwrap();
            assert_eq!((last.code, last.pointer.as_str()), (ErrorCode::TooManyErrors, ""));
        }
    }
    // A short list keeps each error, and `links` comes last, with no `too_many_errors`.
    for count in [1, MAX_ERRORS - 2] {
        let e = migrate_1_to_3(&transparent_panels(count), None).unwrap_err();
        assert_eq!(e.len(), count + 1);
        assert_eq!((e[count].code, e[count].pointer.as_str()), (ErrorCode::UnknownMember, "/links"));
    }
}

#[test]
fn a_document_of_format_1_with_links_gives_unknown_member() {
    let mut v = json_of(IN1_MINIMAL);
    v["links"] = json!([]);
    let bytes = serde_json::to_vec(&v).unwrap();
    for e in [migrate_1_to_2(&bytes, None).unwrap_err(), migrate_1_to_3(&bytes, None).unwrap_err()] {
        assert_eq!(e.len(), 1, "{e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UnknownMember, "/links"));
    }
}

#[test]
fn errors_of_the_document_have_the_pointers_of_the_input() {
    let mut v = json_of(IN1_NODES);
    v["title"] = json!(5);
    v["sections"][0]["panels"][1]["grid"]["w"] = json!(30);
    let bytes = serde_json::to_vec(&v).unwrap();
    for e in [migrate_1_to_2(&bytes, None).unwrap_err(), migrate_1_to_3(&bytes, None).unwrap_err()] {
        let got: Vec<(&str, ErrorCode)> = e.iter().map(|x| (x.pointer.as_str(), x.code)).collect();
        assert!(got.contains(&("/title", ErrorCode::WrongType)), "{got:?}");
        assert!(got.contains(&("/sections/0/panels/1/grid/w", ErrorCode::OutOfRange)), "{got:?}");
        assert!(e.iter().all(|x| !x.pointer.ends_with("/transparent") && x.pointer != "/links"), "{got:?}");
    }
}

#[test]
fn the_uid_is_checked_against_the_location() {
    let other = Uid::parse("other").unwrap();
    for e in [
        migrate_1_to_2(IN1_NODES, Some(&other)).unwrap_err(),
        migrate_1_to_3(IN1_NODES, Some(&other)).unwrap_err(),
        migrate_2_to_3(V2_NODES, Some(&other)).unwrap_err(),
    ] {
        assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UidMismatch, "/uid"));
    }
}

#[test]
fn bytes_that_are_not_json_give_the_syntax_error() {
    for version in [1, 2] {
        let input = format!("{{\n  \"format_version\": {version},\n  \"uid\": x}}");
        let e =
            if version == 1 { migrate_1_to_3(input.as_bytes(), None) } else { migrate_2_to_3(input.as_bytes(), None) }
                .unwrap_err();
        assert_eq!((e[0].code, e[0].line), (ErrorCode::Syntax, Some(3)));
    }
    for e in [migrate_1_to_3(b"[]", None).unwrap_err(), migrate_2_to_3(b"[]", None).unwrap_err()] {
        assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::WrongType, ""));
    }
}

#[test]
fn a_wrong_shape_is_left_to_the_validation() {
    // `sections` is not an array and a panel is not an object: the migration adds nothing there,
    // and the validation gives the errors.
    let mut v = json_of(IN1_NODES);
    v["sections"][0]["panels"][0] = json!(7);
    let e = migrate_1_to_3(&serde_json::to_vec(&v).unwrap(), None).unwrap_err();
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::WrongType, "/sections/0/panels/0"));
    let mut v = json_of(IN1_NODES);
    v["sections"] = json!("x");
    let e = migrate_1_to_3(&serde_json::to_vec(&v).unwrap(), None).unwrap_err();
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::WrongType, "/sections"));
}

#[test]
fn a_number_in_the_input_keeps_its_value() {
    // The numbers of the compact fixture have non-canonical texts (1.50, 1e3, -0). The canonical
    // output has the form of 4.38 and the same values.
    let out = migrate_1_to_2(IN1_ALL_COMPACT, None).unwrap();
    let out = text(&out);
    assert!(out.contains("\"value\": 1.5"));
    assert!(out.contains("\"value\": 1000"));
    assert!(!out.contains("1.50"));
}

// ------------------------------------------------------------- 2 to 3

#[test]
fn golden_2_to_3() {
    let uid = Uid::parse("nodes").unwrap();
    let out = migrate_2_to_3(V2_NODES, Some(&uid)).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(text(out.bytes()), text(OUT_NODES));
    assert_eq!(out.format_version(), 3);
    for (input, output) in
        [(V2_MINIMAL, OUT_MINIMAL), (V2_ALL, OUT_ALL), (V2_ALL_COMPACT, OUT_ALL), (V2_F_MEMBERS, OUT_F_MEMBERS)]
    {
        let out = migrate_2_to_3(input, None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(text(out.bytes()), text(output));
    }
}

#[test]
fn the_compact_output_file_validates_to_the_canonical_file() {
    let again = validate(OUT_ALL_COMPACT, None).unwrap();
    assert_eq!(text(again.bytes()), text(OUT_ALL));
}

#[test]
fn the_worked_example_grows_by_one_line() {
    // 4.42: the change is `"links": [],` and the new version number. 4.40 gives the size and the etag.
    let out = migrate_2_to_3(V2_NODES, None).unwrap();
    assert_eq!(V2_NODES.len(), 8382);
    assert_eq!(out.bytes().len(), 8397);
    assert_eq!(out.bytes().len() - V2_NODES.len(), "  \"links\": [],\n".len());
    assert_eq!(out.etag().to_hex(), "50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501");
    assert_eq!(text(V2_NODES).lines().count() + 1, text(out.bytes()).lines().count());
}

#[test]
fn only_the_version_and_links_change() {
    let out = migrate_2_to_3(V2_ALL, None).unwrap();
    let mut old = json_of(V2_ALL);
    old["format_version"] = json!(3);
    old["links"] = json!([]);
    assert_eq!(json_of(out.bytes()), old);
}

#[test]
fn the_migration_2_to_3_is_pure_and_its_result_is_valid() {
    for input in [V2_MINIMAL, V2_NODES, V2_ALL, V2_ALL_COMPACT, V2_F_MEMBERS] {
        let a = migrate_2_to_3(input, None).unwrap();
        let b = migrate_2_to_3(input, None).unwrap();
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.etag(), b.etag());
        let again = validate(a.bytes(), None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(again.bytes(), a.bytes());
        assert_eq!(again.etag(), a.etag());
    }
}

#[test]
fn a_second_call_gives_format_version() {
    for input in [V2_MINIMAL, V2_ALL] {
        let out = migrate_2_to_3(input, None).unwrap();
        let e = migrate_2_to_3(out.bytes(), None).unwrap_err();
        assert_eq!(e.len(), 1);
        assert_eq!(
            (e[0].code, e[0].pointer.as_str(), e[0].message.as_str()),
            (ErrorCode::FormatVersion, "/format_version", "not loadable: format 3")
        );
    }
}

#[test]
fn a_document_of_format_1_is_not_a_format_2_document() {
    let e = migrate_2_to_3(IN1_MINIMAL, None).unwrap_err();
    assert_eq!((e[0].code, e[0].message.as_str()), (ErrorCode::FormatVersion, "not loadable: format 1"));
}

#[test]
fn a_document_of_format_2_with_links_gives_unknown_member() {
    let mut v = json_of(V2_MINIMAL);
    v["links"] = json!([]);
    let e = migrate_2_to_3(&serde_json::to_vec(&v).unwrap(), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UnknownMember, "/links"));
}

#[test]
fn a_document_of_format_2_that_is_not_valid_gives_the_errors_of_format_3() {
    let mut v = json_of(V2_NODES);
    v["title"] = json!("");
    v["time"]["timezone"] = json!("UTC");
    let e = migrate_2_to_3(&serde_json::to_vec(&v).unwrap(), None).unwrap_err();
    let got: Vec<(&str, ErrorCode)> = e.iter().map(|x| (x.pointer.as_str(), x.code)).collect();
    assert_eq!(got, [("/title", ErrorCode::OutOfRange), ("/time/timezone", ErrorCode::BadFormat)]);
}

#[test]
fn the_migration_does_not_check_for_the_optional_members_of_format_3() {
    // A hand edit of a format 2 document can add a member of format 3. The migration keeps it.
    let mut v = json_of(V2_NODES);
    v["time"]["timezone"] = json!("Europe/Paris");
    let out = migrate_2_to_3(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert!(text(out.bytes()).contains("\"timezone\": \"Europe/Paris\""));
}

// -------------------------------------------------------------- chain

#[test]
fn the_chain_gives_the_bytes_of_the_files_of_format_3() {
    for (input, output) in CHAIN {
        let out = migrate_1_to_3(input, None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(text(out.bytes()), with_format_1_description(output));
        assert_eq!(out.format_version(), 3);
    }
}

#[test]
fn the_chain_equals_the_two_steps() {
    for (input, _) in CHAIN {
        let step = migrate_1_to_2(input, None).unwrap();
        let two = migrate_2_to_3(&step, None).unwrap_or_else(|e| panic!("{e:#?}"));
        let one = migrate_1_to_3(input, None).unwrap();
        assert_eq!(two.bytes(), one.bytes());
        assert_eq!(two.etag(), one.etag());
    }
}

#[test]
fn the_output_of_the_first_step_is_the_canonical_form_of_format_2() {
    // The files of format2/ that are canonical are the outputs of the first step (the description of
    // `all_types` names the format of its input).
    assert_eq!(text(&migrate_1_to_2(IN1_MINIMAL, None).unwrap()), text(V2_MINIMAL));
    assert_eq!(text(&migrate_1_to_2(IN1_NODES, None).unwrap()), text(V2_NODES));
}
