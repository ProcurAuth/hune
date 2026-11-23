// SPDX-License-Identifier: MIT OR Apache-2.0
//! Error codes, limits and the rules across members of the document format (4.34 to 4.37).
//!
//! Each test changes one place of the worked example (4.40) and checks the one error that
//! follows: its pointer and its code.

use hune_core::document::validate;
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::ids::Uid;
use hune_core::limits::*;
use serde_json::{Value, json};

const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const GRID_ERRORS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/invalid/grid_w_and_extra_member.json");
const FORMAT_4: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/invalid/format_4.json");
const FORMAT_2: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format2/nodes.json");
const FORMAT_1: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/format1/nodes.json");
const DUPLICATE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/invalid/duplicate_member.json");
const SYNTAX: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/invalid/syntax_error.json");

/// The worked example as a JSON value. The variables are 0 `node` (query), 1 `account`
/// (text) and 2 `env` (custom). The panels are `/sections/0/panels/0` (timeseries, id 1),
/// `/sections/0/panels/1` (stat, reuse of 2) and `/sections/1/panels/0` (table, id 2).
fn base() -> Value {
    serde_json::from_slice(NODES).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    match validate(&serde_json::to_vec(v).unwrap(), None) {
        Ok(_) => Vec::new(),
        Err(e) => e,
    }
}

fn at<'a>(v: &'a mut Value, pointer: &str) -> &'a mut Value {
    v.pointer_mut(pointer).unwrap_or_else(|| panic!("no value at {pointer}"))
}

fn set(v: &mut Value, pointer: &str, new: Value) {
    *at(v, pointer) = new;
}

fn remove(v: &mut Value, pointer: &str) {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    at(v, parent).as_object_mut().unwrap().remove(key);
}

fn insert(v: &mut Value, pointer: &str, key: &str, new: Value) {
    at(v, pointer).as_object_mut().unwrap().insert(key.to_owned(), new);
}

/// Checks that the value gives exactly one error, and returns it.
fn one(v: &Value) -> ValidationError {
    let mut e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    e.remove(0)
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) -> ValidationError {
    let e = one(v);
    assert_eq!((e.pointer.as_str(), e.code), (pointer, code), "{e:#?}");
    assert!(e.line.is_none() && e.column.is_none());
    e
}

macro_rules! case {
    ($name:ident, |$v:ident| $body:block, $ptr:expr, $code:expr) => {
        #[test]
        fn $name() {
            let mut $v = base();
            $body
            expect(&$v, $ptr, $code);
        }
    };
}

const P0: &str = "/sections/0/panels/0";
const P1: &str = "/sections/0/panels/1";
const T0: &str = "/sections/1/panels/0";

// ----------------------------------------------------------- steps 1 to 4

#[test]
fn encoding_byte_order_mark() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(NODES);
    let e = validate(&bytes, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].line, e[0].column), (ErrorCode::Encoding, Some(1), Some(1)));
    assert_eq!(e[0].pointer, "");
}

#[test]
fn encoding_invalid_utf8_gives_line_and_column() {
    let e = validate(b"{\n  \"title\": \"\xC3\x28\"\n}", None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].line, e[0].column), (ErrorCode::Encoding, Some(2), Some(13)));
}

#[test]
fn syntax_error_has_line_and_column() {
    let e = validate(b"{\n  \"format_version\": 3,\n  \"uid\": x}", None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].line, e[0].column), (ErrorCode::Syntax, Some(3), Some(10)));
}

#[test]
fn syntax_error_fixture() {
    let e = validate(SYNTAX, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].line), (ErrorCode::Syntax, Some(5)));
}

#[test]
fn syntax_error_columns_count_characters() {
    let e = validate("{\"é\u{1F600}\": ?}".as_bytes(), None).unwrap_err();
    assert_eq!((e[0].line, e[0].column), (Some(1), Some(8)));
}

#[test]
fn syntax_trailing_data_and_empty_input() {
    for bad in ["{} {}", "", "   ", "{\"a\":1}x"] {
        let e = validate(bad.as_bytes(), None).unwrap_err();
        assert_eq!((e.len(), e[0].code), (1, ErrorCode::Syntax), "{bad:?}");
    }
}

#[test]
fn too_large_input() {
    let big = vec![b' '; MAX_DOCUMENT_BYTES + 1];
    let e = validate(&big, None).unwrap_err();
    assert_eq!((e.len(), e[0].code, e[0].pointer.as_str()), (1, ErrorCode::TooLarge, ""));
    // The size check comes before the parse: a large invalid text gives too_large.
    let big = vec![b'x'; MAX_DOCUMENT_BYTES + 1];
    assert_eq!(validate(&big, None).unwrap_err()[0].code, ErrorCode::TooLarge);
}

#[test]
fn input_of_exactly_the_limit_is_read() {
    let mut bytes = b"{".to_vec();
    bytes.resize(MAX_DOCUMENT_BYTES, b' ');
    assert_eq!(validate(&bytes, None).unwrap_err()[0].code, ErrorCode::Syntax);
}

#[test]
fn too_deep() {
    let deep = format!("{{\"a\":{}{}}}", "[".repeat(MAX_JSON_DEPTH - 1), "]".repeat(MAX_JSON_DEPTH - 1));
    // Depth 32: the unknown member is read, then refused as unknown (the JSON layer accepts it).
    assert_eq!(validate(deep.as_bytes(), None).unwrap_err()[0].code, ErrorCode::FormatVersion);
    let deeper = format!("{{\"a\":{}{}}}", "[".repeat(MAX_JSON_DEPTH), "]".repeat(MAX_JSON_DEPTH));
    let e = validate(deeper.as_bytes(), None).unwrap_err();
    assert_eq!((e.len(), e[0].code), (1, ErrorCode::TooDeep));
    assert_eq!(e[0].pointer, format!("/a{}", "/0".repeat(MAX_JSON_DEPTH - 1)));
}

#[test]
fn duplicate_member_is_an_error_and_stops() {
    let e = validate(DUPLICATE, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::DuplicateMember, "/title"));
    let text =
        String::from_utf8(NODES.to_vec()).unwrap().replacen("\"to\": \"now\",", "\"to\": \"now\", \"to\": \"now\",", 1);
    let e = validate(text.as_bytes(), None).unwrap_err();
    assert_eq!((e.len(), e[0].code, e[0].pointer.as_str()), (1, ErrorCode::DuplicateMember, "/time/to"));
}

#[test]
fn duplicate_is_found_after_decoding_escapes() {
    let e = validate(b"{\"a\": 1, \"\\u0061\": 2}", None).unwrap_err();
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::DuplicateMember, "/a"));
}

#[test]
fn top_level_must_be_an_object() {
    for bad in ["[]", "1", "\"x\"", "null", "true"] {
        let e = validate(bad.as_bytes(), None).unwrap_err();
        assert_eq!((e.len(), e[0].code, e[0].pointer.as_str()), (1, ErrorCode::WrongType, ""), "{bad}");
    }
}

// ----------------------------------------------------------- format version

#[test]
fn format_version_4_gives_one_error() {
    let e = validate(FORMAT_4, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::FormatVersion, "/format_version"));
    assert_eq!(e[0].message, "not loadable: format 4");
}

/// A stored document of format 2 is "not loadable" until `migrate-documents` rewrites it (4.42, item 2).
#[test]
fn format_version_2_is_not_loadable() {
    let e = validate(FORMAT_2, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::FormatVersion, "/format_version"));
    assert_eq!(e[0].message, "not loadable: format 2");
}

/// A stored document of format 1 is "not loadable" until `migrate-documents` rewrites it (4.42, item 2).
#[test]
fn format_version_1_is_not_loadable() {
    let e = validate(FORMAT_1, None).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::FormatVersion, "/format_version"));
    assert_eq!(e[0].message, "not loadable: format 1");
}

#[test]
fn format_version_stops_even_with_other_errors() {
    let mut v = base();
    set(&mut v, "/format_version", json!(4));
    set(&mut v, "/title", json!(5));
    let e = one(&v);
    assert_eq!((e.code, e.message.as_str()), (ErrorCode::FormatVersion, "not loadable: format 4"));
}

#[test]
fn format_version_absent_or_not_the_integer_3() {
    let mut v = base();
    remove(&mut v, "/format_version");
    assert_eq!(one(&v).code, ErrorCode::FormatVersion);
    for bad in [json!("3"), json!(3.5), json!(0), json!(1), json!(2), json!(null), json!(true), json!(-1)] {
        let mut v = base();
        set(&mut v, "/format_version", bad);
        assert_eq!(one(&v).code, ErrorCode::FormatVersion);
    }
    // 3.0 is a number with a fraction, not the integer 3.
    let text =
        String::from_utf8(NODES.to_vec()).unwrap().replacen("\"format_version\": 3", "\"format_version\": 3.0", 1);
    assert_eq!(validate(text.as_bytes(), None).unwrap_err()[0].code, ErrorCode::FormatVersion);
    let text =
        String::from_utf8(NODES.to_vec()).unwrap().replacen("\"format_version\": 3", "\"format_version\": 3e0", 1);
    assert_eq!(validate(text.as_bytes(), None).unwrap_err()[0].code, ErrorCode::FormatVersion);
}

// ---------------------------------------------------------------- uid

#[test]
fn uid_mismatch() {
    let other = Uid::parse("other").unwrap();
    let e = validate(NODES, Some(&other)).unwrap_err();
    assert_eq!(e.len(), 1);
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::UidMismatch, "/uid"));
    let same = Uid::parse("nodes").unwrap();
    assert!(validate(NODES, Some(&same)).is_ok());
    assert!(validate(NODES, None).is_ok());
}

#[test]
fn uid_grammar_rule_of_4_3() {
    let long = "a".repeat(41);
    let forty = "a".repeat(40);
    for bad in ["", "Nodes", "-a", "a_b", "a.b", "a b", long.as_str(), "é"] {
        let mut v = base();
        set(&mut v, "/uid", json!(bad));
        expect(&v, "/uid", ErrorCode::BadFormat);
    }
    for good in ["a", "0", "a-b", "9-9", forty.as_str()] {
        let mut v = base();
        set(&mut v, "/uid", json!(good));
        assert!(errors_of(&v).is_empty(), "{good}");
    }
}

#[test]
fn uid_mismatch_is_not_checked_for_a_bad_uid() {
    let mut v = base();
    set(&mut v, "/uid", json!("BAD"));
    let other = Uid::parse("other").unwrap();
    let e = validate(&serde_json::to_vec(&v).unwrap(), Some(&other)).unwrap_err();
    assert_eq!((e.len(), e[0].code), (1, ErrorCode::BadFormat));
}

// ------------------------------------------------------ the example of 4.37

#[test]
fn example_of_4_37() {
    let e = validate(GRID_ERRORS, None).unwrap_err();
    let want = vec![
        ValidationError {
            pointer: "/sections/0/panels/2/grid/w".to_owned(),
            code: ErrorCode::OutOfRange,
            message: "w must be 1 to 24; value 30".to_owned(),
            line: None,
            column: None,
        },
        ValidationError {
            pointer: "/sections/0/panels/2/grid/heigth".to_owned(),
            code: ErrorCode::UnknownMember,
            message: "member heigth is not permitted in grid; permitted members: x, y, w, h".to_owned(),
            line: None,
            column: None,
        },
    ];
    assert_eq!(e, want);
    let json = serde_json::to_value(&e).unwrap();
    assert_eq!(json[0]["code"], "out_of_range");
    assert_eq!(json[1]["code"], "unknown_member");
    assert_eq!(json[0]["line"], Value::Null);
}

// ------------------------------------------------------ one test per code

#[test]
fn code_unknown_member_at_every_level_in_input_order() {
    let mut v = base();
    insert(&mut v, "", "zz", json!(1));
    insert(&mut v, "", "aa", json!(1));
    insert(&mut v, "/time", "extra", json!(1));
    insert(&mut v, P0, "extra", json!(1));
    insert(&mut v, &format!("{P0}/options"), "extra", json!(1));
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    // Table members first (the walk), then the unknown members of each object. The test value
    // is sorted by name, so "aa" comes before "zz" in the input.
    assert_eq!(
        pointers,
        ["/time/extra", "/sections/0/panels/0/options/extra", "/sections/0/panels/0/extra", "/aa", "/zz"]
    );
    assert!(e.iter().all(|x| x.code == ErrorCode::UnknownMember));
    assert!(
        e[3].message.starts_with("member aa is not permitted in document; permitted members: format_version, uid,")
    );
}

#[test]
fn unknown_member_name_is_escaped_in_the_pointer() {
    let mut v = base();
    insert(&mut v, "", "a/b~c", json!(1));
    expect(&v, "/a~1b~0c", ErrorCode::UnknownMember);
}

case!(
    code_missing_member_top,
    |v| {
        remove(&mut v, "/title");
    },
    "",
    ErrorCode::MissingMember
);
case!(
    code_missing_member_grid,
    |v| {
        remove(&mut v, &format!("{P0}/grid/h"));
    },
    "/sections/0/panels/0/grid",
    ErrorCode::MissingMember
);
case!(
    code_wrong_type_title,
    |v| {
        set(&mut v, "/title", json!(5));
    },
    "/title",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_object,
    |v| {
        set(&mut v, "/time", json!([]));
    },
    "/time",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_array,
    |v| {
        set(&mut v, "/tags", json!("a"));
    },
    "/tags",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_bool,
    |v| {
        set(&mut v, "/variables/0/multi", json!("true"));
    },
    "/variables/0/multi",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_integer_with_fraction,
    |v| {
        set(&mut v, &format!("{P0}/grid/x"), json!(1.5));
    },
    "/sections/0/panels/0/grid/x",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_null_where_not_permitted,
    |v| {
        set(&mut v, "/title", json!(null));
    },
    "/title",
    ErrorCode::WrongType
);
case!(
    code_out_of_range_title,
    |v| {
        set(&mut v, "/title", json!(""));
    },
    "/title",
    ErrorCode::OutOfRange
);
case!(
    code_bad_format_cursor_sync,
    |v| {
        set(&mut v, "/cursor_sync", json!("both"));
    },
    "/cursor_sync",
    ErrorCode::BadFormat
);
case!(
    code_not_unique_tags,
    |v| {
        set(&mut v, "/tags", json!(["a", "b", "a"]));
    },
    "/tags/2",
    ErrorCode::NotUnique
);
case!(
    code_bad_reference_reuse,
    |v| {
        set(&mut v, &format!("{P1}/data/reuse/panel"), json!(99));
    },
    "/sections/0/panels/1/data/reuse/panel",
    ErrorCode::BadReference
);
case!(
    code_not_permitted_collapsed,
    |v| {
        set(&mut v, "/sections/0/collapsed", json!(true));
    },
    "/sections/0/collapsed",
    ErrorCode::NotPermitted
);

case!(
    code_missing_member_transparent,
    |v| {
        remove(&mut v, &format!("{P0}/transparent"));
    },
    "/sections/0/panels/0",
    ErrorCode::MissingMember
);
case!(
    code_wrong_type_transparent,
    |v| {
        set(&mut v, &format!("{P0}/transparent"), json!("true"));
    },
    "/sections/0/panels/0/transparent",
    ErrorCode::WrongType
);
case!(
    code_wrong_type_transparent_number,
    |v| {
        set(&mut v, &format!("{T0}/transparent"), json!(1));
    },
    "/sections/1/panels/0/transparent",
    ErrorCode::WrongType
);

#[test]
fn transparent_true_and_false_are_valid() {
    for flag in [true, false] {
        let mut v = base();
        set(&mut v, &format!("{P0}/transparent"), json!(flag));
        assert!(errors_of(&v).is_empty(), "{flag}");
    }
}

#[test]
fn code_overlap() {
    let mut v = base();
    set(&mut v, &format!("{P1}/grid"), json!({"x": 10, "y": 4, "w": 8, "h": 8}));
    expect(&v, "/sections/0/panels/1/grid", ErrorCode::Overlap);
}

#[test]
fn code_too_many_errors() {
    let mut v = base();
    for i in 0..300 {
        insert(&mut v, "", &format!("x{i:03}"), json!(0));
    }
    let e = errors_of(&v);
    assert_eq!(e.len(), MAX_ERRORS);
    assert!(e[..MAX_ERRORS - 1].iter().all(|x| x.code == ErrorCode::UnknownMember));
    let last = e.last().unwrap();
    assert_eq!((last.code, last.pointer.as_str()), (ErrorCode::TooManyErrors, ""));
}

#[test]
fn exactly_199_errors_have_no_too_many_errors() {
    let mut v = base();
    for i in 0..199 {
        insert(&mut v, "", &format!("x{i:03}"), json!(0));
    }
    let e = errors_of(&v);
    assert_eq!(e.len(), 199);
    assert!(e.iter().all(|x| x.code == ErrorCode::UnknownMember));
    insert(&mut v, "", "x999", json!(0));
    let e = errors_of(&v);
    assert_eq!(e.len(), 200);
    assert_eq!(e[199].code, ErrorCode::TooManyErrors);
}

#[test]
fn message_shows_at_most_200_characters_of_the_value() {
    let mut v = base();
    set(&mut v, "/title", json!("é".repeat(5000)));
    let e = one(&v);
    assert_eq!(e.code, ErrorCode::OutOfRange);
    assert!(
        e.message.chars().count() < 200 + "title must be 1 to 256 characters; value ".len() + 1,
        "{}",
        e.message.len()
    );
}

// ------------------------------------------------------------ top-level

case!(
    title_too_long,
    |v| {
        set(&mut v, "/title", json!("a".repeat(257)));
    },
    "/title",
    ErrorCode::OutOfRange
);
case!(
    title_control_character,
    |v| {
        set(&mut v, "/title", json!("a\u{1}b"));
    },
    "/title",
    ErrorCode::BadFormat
);
case!(
    title_delete_character,
    |v| {
        set(&mut v, "/title", json!("a\u{7f}b"));
    },
    "/title",
    ErrorCode::BadFormat
);
case!(
    title_nul_character,
    |v| {
        set(&mut v, "/title", json!("a\u{0}b"));
    },
    "/title",
    ErrorCode::BadFormat
);
case!(
    title_tab_is_a_control_character,
    |v| {
        set(&mut v, "/title", json!("a\tb"));
    },
    "/title",
    ErrorCode::BadFormat
);
case!(
    description_too_long,
    |v| {
        set(&mut v, "/description", json!("a".repeat(4097)));
    },
    "/description",
    ErrorCode::OutOfRange
);
case!(
    tags_too_many,
    |v| {
        set(&mut v, "/tags", json!((0..33).map(|i| format!("t{i}")).collect::<Vec<_>>()));
    },
    "/tags",
    ErrorCode::TooLarge
);
case!(
    tag_empty,
    |v| {
        set(&mut v, "/tags", json!([""]));
    },
    "/tags/0",
    ErrorCode::OutOfRange
);
case!(
    tag_too_long,
    |v| {
        set(&mut v, "/tags", json!(["a".repeat(65)]));
    },
    "/tags/0",
    ErrorCode::OutOfRange
);

#[test]
fn limits_at_the_boundary_are_valid() {
    let mut v = base();
    set(&mut v, "/title", json!("é".repeat(256)));
    set(&mut v, "/description", json!("é".repeat(4096)));
    set(&mut v, "/tags", json!((0..32).map(|i| format!("{i:0>2}{}", "é".repeat(62))).collect::<Vec<_>>()));
    assert!(errors_of(&v).is_empty());
}

// ----------------------------------------------------------------- time

case!(
    time_from_bad,
    |v| {
        set(&mut v, "/time/from", json!("now-0h"));
    },
    "/time/from",
    ErrorCode::BadFormat
);
case!(
    time_to_bad,
    |v| {
        set(&mut v, "/time/to", json!("now+1h"));
    },
    "/time/to",
    ErrorCode::BadFormat
);
case!(
    time_absolute_too_large,
    |v| {
        set(&mut v, "/time/to", json!("4294967296000"));
    },
    "/time/to",
    ErrorCode::BadFormat
);
case!(
    time_leading_zero,
    |v| {
        set(&mut v, "/time/from", json!("01731100000000"));
    },
    "/time/from",
    ErrorCode::BadFormat
);
case!(
    time_timezone_bad,
    |v| {
        set(&mut v, "/time/timezone", json!("local"));
    },
    "/time/timezone",
    ErrorCode::BadFormat
);
case!(
    time_refresh_not_a_choice,
    |v| {
        set(&mut v, "/time/refresh", json!("7s"));
    },
    "/time/refresh",
    ErrorCode::BadFormat
);
case!(
    time_refresh_wrong_type,
    |v| {
        set(&mut v, "/time/refresh", json!(5));
    },
    "/time/refresh",
    ErrorCode::WrongType
);
case!(
    time_absolute_from_after_to,
    |v| {
        set(&mut v, "/time/from", json!("2000"));
        set(&mut v, "/time/to", json!("1000"));
    },
    "/time/to",
    ErrorCode::OutOfRange
);
case!(
    time_absolute_from_equals_to,
    |v| {
        set(&mut v, "/time/from", json!("1000"));
        set(&mut v, "/time/to", json!("1000"));
    },
    "/time/to",
    ErrorCode::OutOfRange
);

#[test]
fn time_valid_forms() {
    for refresh in REFRESH_CHOICES {
        let mut v = base();
        set(&mut v, "/time/refresh", json!(refresh));
        assert!(errors_of(&v).is_empty(), "{refresh}");
    }
    let mut v = base();
    set(&mut v, "/time/refresh", json!(null));
    set(&mut v, "/time/from", json!("1000"));
    set(&mut v, "/time/to", json!("now"));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/time/from", json!("0"));
    set(&mut v, "/time/to", json!("4294967295999"));
    assert!(errors_of(&v).is_empty());
}

// ------------------------------------------------------ sections and counts

fn section_template() -> Value {
    json!({"title": "S", "collapsed": false, "panels": []})
}

#[test]
fn sections_count() {
    let mut v = base();
    set(&mut v, "/sections", json!([]));
    expect(&v, "/sections", ErrorCode::OutOfRange);
    let mut v = base();
    let mut sections = vec![json!({"title": null, "collapsed": false, "panels": []})];
    sections.extend((0..49).map(|_| section_template()));
    set(&mut v, "/sections", Value::Array(sections.clone()));
    assert!(errors_of(&v).is_empty());
    sections.push(section_template());
    set(&mut v, "/sections", Value::Array(sections));
    expect(&v, "/sections", ErrorCode::TooLarge);
}

#[test]
fn variables_count() {
    let text_var = |i: usize| json!({"kind": "text", "name": format!("v{i}"), "label": null, "hide": "none", "value_type": "text", "default": ""});
    let mut v = base();
    set(&mut v, "/variables", Value::Array((0..64).map(text_var).collect()));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables", Value::Array((0..65).map(text_var).collect()));
    expect(&v, "/variables", ErrorCode::TooLarge);
}

fn panel_template(id: u32) -> Value {
    let mut p = base().pointer(P0).unwrap().clone();
    p["id"] = json!(id);
    p["grid"] = json!({"x": 0, "y": id, "w": 24, "h": 1});
    p
}

#[test]
fn panels_per_section_limit() {
    let mut v = base();
    set(
        &mut v,
        "/sections",
        json!([{"title": null, "collapsed": false, "panels": (1..=200).map(panel_template).collect::<Vec<_>>()}]),
    );
    assert!(errors_of(&v).is_empty());
    set(
        &mut v,
        "/sections",
        json!([{"title": null, "collapsed": false, "panels": (1..=201).map(panel_template).collect::<Vec<_>>()}]),
    );
    expect(&v, "/sections/0/panels", ErrorCode::TooLarge);
}

#[test]
fn panels_in_all_sections_limit() {
    let mut v = base();
    set(
        &mut v,
        "/sections",
        json!([
            {"title": null, "collapsed": false, "panels": (1..=100).map(panel_template).collect::<Vec<_>>()},
            {"title": "S", "collapsed": false, "panels": (101..=201).map(panel_template).collect::<Vec<_>>()}
        ]),
    );
    expect(&v, "/sections", ErrorCode::TooLarge);
    set(
        &mut v,
        "/sections",
        json!([
            {"title": null, "collapsed": false, "panels": (1..=100).map(panel_template).collect::<Vec<_>>()},
            {"title": "S", "collapsed": false, "panels": (101..=200).map(panel_template).collect::<Vec<_>>()}
        ]),
    );
    assert!(errors_of(&v).is_empty());
}

case!(
    section_title_empty,
    |v| {
        set(&mut v, "/sections/1/title", json!(""));
    },
    "/sections/1/title",
    ErrorCode::OutOfRange
);
case!(
    section_title_null_after_the_first,
    |v| {
        set(&mut v, "/sections/1/title", json!(null));
    },
    "/sections/1/title",
    ErrorCode::NotPermitted
);
case!(
    section_collapsed_wrong_type,
    |v| {
        set(&mut v, "/sections/1/collapsed", json!(0));
    },
    "/sections/1/collapsed",
    ErrorCode::WrongType
);
case!(
    section_collapsed_with_null_title,
    |v| {
        set(&mut v, "/sections/0/collapsed", json!(true));
    },
    "/sections/0/collapsed",
    ErrorCode::NotPermitted
);

#[test]
fn section_collapsed_and_null_title_after_the_first_gives_two_errors() {
    let mut v = base();
    set(&mut v, "/sections/1/title", json!(null));
    set(&mut v, "/sections/1/collapsed", json!(true));
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/sections/1/title", "/sections/1/collapsed"]);
    assert!(e.iter().all(|x| x.code == ErrorCode::NotPermitted));
}

// -------------------------------------------------------------- variables

case!(
    variable_name_bad_start,
    |v| {
        set(&mut v, "/variables/1/name", json!("1a"));
    },
    "/variables/1/name",
    ErrorCode::BadFormat
);
case!(
    variable_name_reserved,
    |v| {
        set(&mut v, "/variables/1/name", json!("__x"));
    },
    "/variables/1/name",
    ErrorCode::BadFormat
);
case!(
    variable_name_dash,
    |v| {
        set(&mut v, "/variables/1/name", json!("a-b"));
    },
    "/variables/1/name",
    ErrorCode::BadFormat
);
case!(
    variable_name_too_long,
    |v| {
        set(&mut v, "/variables/1/name", json!("a".repeat(65)));
    },
    "/variables/1/name",
    ErrorCode::BadFormat
);
case!(
    variable_name_duplicate,
    |v| {
        set(&mut v, "/variables/1/name", json!("node"));
    },
    "/variables/1/name",
    ErrorCode::NotUnique
);
case!(
    variable_label_empty,
    |v| {
        set(&mut v, "/variables/1/label", json!(""));
    },
    "/variables/1/label",
    ErrorCode::OutOfRange
);
case!(
    variable_hide_bad,
    |v| {
        set(&mut v, "/variables/1/hide", json!("all"));
    },
    "/variables/1/hide",
    ErrorCode::BadFormat
);
case!(
    variable_kind_unknown,
    |v| {
        set(&mut v, "/variables/1/kind", json!("datasource"));
    },
    "/variables/1/kind",
    ErrorCode::BadFormat
);
case!(
    variable_missing_kind_member,
    |v| {
        remove(&mut v, "/variables/1/value_type");
    },
    "/variables/1",
    ErrorCode::MissingMember
);
case!(
    variable_member_of_another_kind,
    |v| {
        insert(&mut v, "/variables/1", "multi", json!(true));
    },
    "/variables/1/multi",
    ErrorCode::UnknownMember
);
case!(
    query_source_bad,
    |v| {
        set(&mut v, "/variables/0/source", json!("Prod"));
    },
    "/variables/0/source",
    ErrorCode::BadFormat
);
case!(
    query_sql_empty,
    |v| {
        set(&mut v, "/variables/0/sql", json!([]));
    },
    "/variables/0/sql",
    ErrorCode::OutOfRange
);
case!(
    query_sql_line_feed,
    |v| {
        set(&mut v, "/variables/0/sql", json!(["SELECT 1", "a\nb"]));
    },
    "/variables/0/sql/1",
    ErrorCode::BadFormat
);
case!(
    query_sql_carriage_return,
    |v| {
        set(&mut v, "/variables/0/sql", json!(["a\rb"]));
    },
    "/variables/0/sql/0",
    ErrorCode::BadFormat
);
case!(
    query_sql_control_character,
    |v| {
        set(&mut v, "/variables/0/sql", json!(["a\u{1}b"]));
    },
    "/variables/0/sql/0",
    ErrorCode::BadFormat
);
case!(
    query_sql_not_text,
    |v| {
        set(&mut v, "/variables/0/sql", json!(["a", 1]));
    },
    "/variables/0/sql/1",
    ErrorCode::WrongType
);
case!(
    query_sql_not_an_array,
    |v| {
        set(&mut v, "/variables/0/sql", json!("SELECT 1"));
    },
    "/variables/0/sql",
    ErrorCode::WrongType
);
case!(
    query_all_value_with_include_all_false,
    |v| {
        set(&mut v, "/variables/0/include_all", json!(false));
        set(&mut v, "/variables/0/default", json!([]));
        set(&mut v, "/variables/0/all_value", json!("1"));
    },
    "/variables/0/all_value",
    ErrorCode::NotPermitted
);
case!(
    query_all_value_empty,
    |v| {
        set(&mut v, "/variables/0/all_value", json!(""));
    },
    "/variables/0/all_value",
    ErrorCode::OutOfRange
);
case!(
    query_all_value_too_long,
    |v| {
        set(&mut v, "/variables/0/all_value", json!("a".repeat(1025)));
    },
    "/variables/0/all_value",
    ErrorCode::OutOfRange
);
case!(
    query_refresh_bad,
    |v| {
        set(&mut v, "/variables/0/refresh", json!("never"));
    },
    "/variables/0/refresh",
    ErrorCode::BadFormat
);
case!(
    query_sort_bad,
    |v| {
        set(&mut v, "/variables/0/sort", json!("random"));
    },
    "/variables/0/sort",
    ErrorCode::BadFormat
);
case!(
    query_regex_bad_grafana_form,
    |v| {
        set(&mut v, "/variables/0/regex", json!("/abc"));
    },
    "/variables/0/regex",
    ErrorCode::BadFormat
);
case!(
    query_regex_outside_the_subset,
    |v| {
        set(&mut v, "/variables/0/regex", json!("/(?i)abc/"));
    },
    "/variables/0/regex",
    ErrorCode::BadFormat
);
case!(
    query_regex_empty,
    |v| {
        set(&mut v, "/variables/0/regex", json!(""));
    },
    "/variables/0/regex",
    ErrorCode::OutOfRange
);

#[test]
fn query_variable_valid_sort_values() {
    for sort in [
        "none",
        "alpha_asc",
        "alpha_desc",
        "num_asc",
        "num_desc",
        "alpha_ci_asc",
        "alpha_ci_desc",
        "natural_asc",
        "natural_desc",
    ] {
        let mut v = base();
        set(&mut v, "/variables/0/sort", json!(sort));
        assert!(errors_of(&v).is_empty(), "{sort}");
    }
}

#[test]
fn sql_limit_in_bytes_after_joining() {
    let mut v = base();
    set(&mut v, "/variables/0/sql", json!(["a".repeat(MAX_SQL_BYTES)]));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables/0/sql", json!(["a".repeat(MAX_SQL_BYTES + 1)]));
    expect(&v, "/variables/0/sql", ErrorCode::TooLarge);
    // Two lines: the line feed between them counts for one byte.
    set(&mut v, "/variables/0/sql", json!(["a".repeat(131_072), "a".repeat(131_072)]));
    expect(&v, "/variables/0/sql", ErrorCode::TooLarge);
    set(&mut v, "/variables/0/sql", json!(["a".repeat(131_072), "a".repeat(131_071)]));
    assert!(errors_of(&v).is_empty());
    // Bytes, not characters.
    set(&mut v, "/variables/0/sql", json!(["é".repeat(MAX_SQL_BYTES / 2 + 1)]));
    expect(&v, "/variables/0/sql", ErrorCode::TooLarge);
}

#[test]
fn sql_line_may_hold_a_tab() {
    let mut v = base();
    set(&mut v, "/variables/0/sql", json!(["SELECT\t1"]));
    assert!(errors_of(&v).is_empty());
}

#[test]
fn panel_sql_has_the_same_rules() {
    let mut v = base();
    set(&mut v, &format!("{P0}/data/query/sql"), json!(["a\nb"]));
    expect(&v, "/sections/0/panels/0/data/query/sql/0", ErrorCode::BadFormat);
    set(&mut v, &format!("{P0}/data/query/sql"), json!([]));
    expect(&v, "/sections/0/panels/0/data/query/sql", ErrorCode::OutOfRange);
    set(&mut v, &format!("{P0}/data/query/sql"), json!(["a".repeat(MAX_SQL_BYTES + 1)]));
    expect(&v, "/sections/0/panels/0/data/query/sql", ErrorCode::TooLarge);
}

// ------------------------------------------------------ default (4.11)

case!(
    default_multi_false_two_items,
    |v| {
        set(&mut v, "/variables/2/default", json!(["prod", "preprod"]));
    },
    "/variables/2/default/1",
    ErrorCode::NotPermitted
);
case!(
    default_all_without_include_all,
    |v| {
        set(&mut v, "/variables/2/default", json!(["$__all"]));
    },
    "/variables/2/default/0",
    ErrorCode::NotPermitted
);
case!(
    default_all_with_other_item,
    |v| {
        set(&mut v, "/variables/0/default", json!(["$__all", "x"]));
    },
    "/variables/0/default/0",
    ErrorCode::NotPermitted
);
case!(
    default_custom_not_an_option,
    |v| {
        set(&mut v, "/variables/2/default", json!(["nope"]));
    },
    "/variables/2/default/0",
    ErrorCode::NotPermitted
);
case!(
    default_item_empty,
    |v| {
        set(&mut v, "/variables/0/default", json!([""]));
    },
    "/variables/0/default/0",
    ErrorCode::OutOfRange
);
case!(
    default_item_too_long,
    |v| {
        set(&mut v, "/variables/0/default", json!(["a".repeat(1025)]));
    },
    "/variables/0/default/0",
    ErrorCode::OutOfRange
);

#[test]
fn default_rules_that_are_valid() {
    let mut v = base();
    // For a query variable an item can be absent from the options.
    set(&mut v, "/variables/0/default", json!(["a", "b"]));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables/0/default", json!([]));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables/2/default", json!([]));
    assert!(errors_of(&v).is_empty());
    // With multi, several option values.
    set(&mut v, "/variables/2/multi", json!(true));
    set(&mut v, "/variables/2/default", json!(["prod", "preprod"]));
    assert!(errors_of(&v).is_empty());
}

#[test]
fn default_count_limit() {
    let mut v = base();
    let items = |n: usize| Value::Array((0..n).map(|i| json!(format!("v{i}"))).collect());
    set(&mut v, "/variables/0/default", items(MAX_VARIABLE_VALUES));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables/0/default", items(MAX_VARIABLE_VALUES + 1));
    expect(&v, "/variables/0/default", ErrorCode::TooLarge);
}

// -------------------------------------------------------------- custom

case!(
    custom_options_empty,
    |v| {
        set(&mut v, "/variables/2/options", json!([]));
        set(&mut v, "/variables/2/default", json!([]));
    },
    "/variables/2/options",
    ErrorCode::OutOfRange
);
case!(
    custom_option_duplicate_value,
    |v| {
        set(&mut v, "/variables/2/options/1/value", json!("prod"));
    },
    "/variables/2/options/1/value",
    ErrorCode::NotUnique
);
case!(
    custom_option_text_empty,
    |v| {
        set(&mut v, "/variables/2/options/0/text", json!(""));
    },
    "/variables/2/options/0/text",
    ErrorCode::OutOfRange
);
case!(
    custom_option_text_too_long,
    |v| {
        set(&mut v, "/variables/2/options/0/text", json!("a".repeat(257)));
    },
    "/variables/2/options/0/text",
    ErrorCode::OutOfRange
);
case!(
    custom_option_value_empty,
    |v| {
        set(&mut v, "/variables/2/options/0/value", json!(""));
        set(&mut v, "/variables/2/default", json!([]));
    },
    "/variables/2/options/0/value",
    ErrorCode::OutOfRange
);
case!(
    custom_option_unknown_member,
    |v| {
        insert(&mut v, "/variables/2/options/0", "x", json!(1));
    },
    "/variables/2/options/0/x",
    ErrorCode::UnknownMember
);
case!(
    custom_all_value_with_include_all_false,
    |v| {
        set(&mut v, "/variables/2/all_value", json!("x"));
    },
    "/variables/2/all_value",
    ErrorCode::NotPermitted
);

#[test]
fn custom_options_count_limit() {
    let mut v = base();
    let options = |n: usize| Value::Array((0..n).map(|i| json!({"text": "t", "value": format!("v{i}")})).collect());
    set(&mut v, "/variables/2/options", options(MAX_VARIABLE_VALUES));
    set(&mut v, "/variables/2/default", json!([]));
    assert!(errors_of(&v).is_empty());
    set(&mut v, "/variables/2/options", options(MAX_VARIABLE_VALUES + 1));
    expect(&v, "/variables/2/options", ErrorCode::TooLarge);
}

// ---------------------------------------------------------------- text

case!(
    text_variable_number_default_bad,
    |v| {
        set(&mut v, "/variables/1/value_type", json!("number"));
        set(&mut v, "/variables/1/default", json!("abc"));
    },
    "/variables/1/default",
    ErrorCode::BadFormat
);
case!(
    text_variable_number_default_empty,
    |v| {
        set(&mut v, "/variables/1/value_type", json!("number"));
    },
    "/variables/1/default",
    ErrorCode::BadFormat
);
case!(
    text_variable_default_too_long,
    |v| {
        set(&mut v, "/variables/1/default", json!("a".repeat(1025)));
    },
    "/variables/1/default",
    ErrorCode::OutOfRange
);
case!(
    text_variable_value_type_bad,
    |v| {
        set(&mut v, "/variables/1/value_type", json!("date"));
    },
    "/variables/1/value_type",
    ErrorCode::BadFormat
);

#[test]
fn text_variable_number_default_valid() {
    let mut v = base();
    set(&mut v, "/variables/1/value_type", json!("number"));
    for d in ["0", "-1", "12.5", "-0.25"] {
        set(&mut v, "/variables/1/default", json!(d));
        assert!(errors_of(&v).is_empty(), "{d}");
    }
    set(&mut v, "/variables/1/value_type", json!("text"));
    set(&mut v, "/variables/1/default", json!(""));
    assert!(errors_of(&v).is_empty());
}

// ------------------------------------------------------------ cycles

fn query_variable(name: &str, sql: &str) -> Value {
    json!({"kind": "query", "name": name, "label": null, "hide": "none", "source": "prod", "sql": sql.split('\n').collect::<Vec<_>>(),
           "multi": false, "include_all": false, "all_value": null, "refresh": "load", "sort": "none", "regex": null, "default": []})
}

fn with_variables(vars: Vec<Value>) -> Value {
    let mut v = base();
    set(&mut v, "/variables", Value::Array(vars));
    for panel in [P0, T0] {
        set(&mut v, &format!("{panel}/data/query/sql"), json!(["SELECT 1"]));
    }
    v
}

#[test]
fn variable_cycle_self_reference() {
    let v = with_variables(vec![query_variable("a", "SELECT $a")]);
    expect(&v, "/variables/0/sql", ErrorCode::BadReference);
    let v = with_variables(vec![query_variable("a", "SELECT ${a}")]);
    expect(&v, "/variables/0/sql", ErrorCode::BadReference);
    let v = with_variables(vec![query_variable("a", "SELECT ${a:csv}")]);
    expect(&v, "/variables/0/sql", ErrorCode::BadReference);
}

#[test]
fn variable_cycle_of_three_is_reported_once_at_the_first_in_array_order() {
    let v = with_variables(vec![
        query_variable("x", "SELECT 1"),
        query_variable("c", "SELECT $a"),
        query_variable("a", "SELECT $b"),
        query_variable("b", "SELECT ${c}"),
    ]);
    expect(&v, "/variables/1/sql", ErrorCode::BadReference);
}

#[test]
fn two_cycles_give_two_errors() {
    let v = with_variables(vec![
        query_variable("a", "SELECT $b"),
        query_variable("b", "SELECT $a"),
        query_variable("c", "SELECT $c"),
    ]);
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/variables/0/sql", "/variables/2/sql"]);
}

#[test]
fn no_cycle_cases() {
    // A chain, a diamond, a reference to a name that is not a variable, to a text variable,
    // and placeholders in a comment, a heredoc or a longer name.
    let text_var =
        json!({"kind": "text", "name": "t", "label": null, "hide": "none", "value_type": "text", "default": ""});
    let v = with_variables(vec![
        query_variable("a", "SELECT $b, $c"),
        query_variable("b", "SELECT $d"),
        query_variable("c", "SELECT $d, $t, $unknown, $__timeFilter(x)"),
        query_variable("d", "SELECT 1 -- $a\n/* $a */, $$ $a $$, $dd"),
        text_var,
    ]);
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    // The longest run names the variable: $env_name is not $env.
    let v = with_variables(vec![query_variable("env", "SELECT $env_name")]);
    assert!(errors_of(&v).is_empty());
    // ${env}_name names env.
    let v = with_variables(vec![query_variable("env", "SELECT ${env}_name")]);
    expect(&v, "/variables/0/sql", ErrorCode::BadReference);
}

#[test]
fn placeholder_in_a_string_is_a_reference() {
    let v = with_variables(vec![query_variable("a", "SELECT '$b'"), query_variable("b", "SELECT \"${a}\"")]);
    expect(&v, "/variables/0/sql", ErrorCode::BadReference);
}

// --------------------------------------------------------------- panels

case!(
    panel_id_zero,
    |v| {
        set(&mut v, &format!("{P0}/id"), json!(0));
    },
    "/sections/0/panels/0/id",
    ErrorCode::OutOfRange
);
case!(
    panel_id_too_large,
    |v| {
        set(&mut v, &format!("{P0}/id"), json!(2_147_483_648u64));
    },
    "/sections/0/panels/0/id",
    ErrorCode::OutOfRange
);
case!(
    panel_id_fraction,
    |v| {
        set(&mut v, &format!("{P0}/id"), json!(1.5));
    },
    "/sections/0/panels/0/id",
    ErrorCode::WrongType
);
case!(
    panel_id_duplicate,
    |v| {
        set(&mut v, &format!("{T0}/id"), json!(1));
        set(&mut v, &format!("{P1}/data/reuse/panel"), json!(1));
    },
    "/sections/1/panels/0/id",
    ErrorCode::NotUnique
);
case!(
    panel_type_bad,
    |v| {
        set(&mut v, &format!("{P0}/type"), json!("news"));
    },
    "/sections/0/panels/0/type",
    ErrorCode::BadFormat
);
case!(
    panel_title_too_long,
    |v| {
        set(&mut v, &format!("{P0}/title"), json!("a".repeat(257)));
    },
    "/sections/0/panels/0/title",
    ErrorCode::OutOfRange
);
case!(
    panel_description_too_long,
    |v| {
        set(&mut v, &format!("{P0}/description"), json!("a".repeat(4097)));
    },
    "/sections/0/panels/0/description",
    ErrorCode::OutOfRange
);

#[test]
fn panel_id_range_is_valid_at_the_ends() {
    let mut v = base();
    set(&mut v, &format!("{P0}/id"), json!(2_147_483_647));
    assert!(errors_of(&v).is_empty());
    set(&mut v, &format!("{P0}/id"), json!(1));
    set(&mut v, &format!("{P0}/title"), json!(""));
    assert!(errors_of(&v).is_empty());
}

#[test]
fn panel_type_and_options_must_agree() {
    let mut v = base();
    set(&mut v, &format!("{P0}/type"), json!("stat"));
    let e = errors_of(&v);
    assert!(!e.is_empty());
    assert!(e.iter().all(|x| x.pointer.starts_with("/sections/0/panels/0/options")), "{e:#?}");
    assert!(e.iter().any(|x| x.code == ErrorCode::MissingMember));
    assert!(e.iter().any(|x| x.code == ErrorCode::UnknownMember));
}

// ----------------------------------------------------------------- grid

case!(
    grid_x_out_of_range,
    |v| {
        set(&mut v, &format!("{P0}/grid/x"), json!(24));
    },
    "/sections/0/panels/0/grid/x",
    ErrorCode::OutOfRange
);
case!(
    grid_x_negative,
    |v| {
        set(&mut v, &format!("{P0}/grid/x"), json!(-1));
    },
    "/sections/0/panels/0/grid/x",
    ErrorCode::OutOfRange
);
case!(
    grid_y_out_of_range,
    |v| {
        set(&mut v, &format!("{P0}/grid/y"), json!(10_001));
    },
    "/sections/0/panels/0/grid/y",
    ErrorCode::OutOfRange
);
case!(
    grid_w_zero,
    |v| {
        set(&mut v, &format!("{P0}/grid/w"), json!(0));
    },
    "/sections/0/panels/0/grid/w",
    ErrorCode::OutOfRange
);
case!(
    grid_w_too_large,
    |v| {
        set(&mut v, &format!("{P0}/grid/w"), json!(25));
    },
    "/sections/0/panels/0/grid/w",
    ErrorCode::OutOfRange
);
case!(
    grid_h_zero,
    |v| {
        set(&mut v, &format!("{P0}/grid/h"), json!(0));
    },
    "/sections/0/panels/0/grid/h",
    ErrorCode::OutOfRange
);
case!(
    grid_h_too_large,
    |v| {
        set(&mut v, &format!("{P0}/grid/h"), json!(201));
    },
    "/sections/0/panels/0/grid/h",
    ErrorCode::OutOfRange
);
case!(
    grid_x_plus_w,
    |v| {
        set(&mut v, &format!("{P0}/grid/x"), json!(20));
        set(&mut v, &format!("{P0}/grid/w"), json!(5));
        set(&mut v, &format!("{P0}/grid/y"), json!(100));
        set(&mut v, &format!("{P1}/grid/y"), json!(200));
    },
    "/sections/0/panels/0/grid/w",
    ErrorCode::OutOfRange
);

#[test]
fn grid_valid_extremes() {
    let mut v = base();
    set(&mut v, &format!("{P0}/grid"), json!({"x": 23, "y": 10_000, "w": 1, "h": 200}));
    assert!(errors_of(&v).is_empty());
    set(&mut v, &format!("{P0}/grid"), json!({"x": 0, "y": 0, "w": 24, "h": 1}));
    set(&mut v, &format!("{P1}/grid"), json!({"x": 0, "y": 1, "w": 24, "h": 1}));
    assert!(errors_of(&v).is_empty());
}

#[test]
fn overlap_rules() {
    // The panels touch edge to edge: no overlap.
    let mut v = base();
    set(&mut v, &format!("{P0}/grid"), json!({"x": 0, "y": 0, "w": 12, "h": 4}));
    set(&mut v, &format!("{P1}/grid"), json!({"x": 12, "y": 0, "w": 12, "h": 4}));
    assert!(errors_of(&v).is_empty());
    set(&mut v, &format!("{P1}/grid"), json!({"x": 0, "y": 4, "w": 12, "h": 4}));
    assert!(errors_of(&v).is_empty());
    // One cell in common: overlap, reported at the second panel in array order.
    set(&mut v, &format!("{P1}/grid"), json!({"x": 11, "y": 3, "w": 5, "h": 5}));
    expect(&v, "/sections/0/panels/1/grid", ErrorCode::Overlap);
    // Two sections have their own grid: the same place is valid.
    let mut v = base();
    set(&mut v, &format!("{T0}/grid"), json!({"x": 0, "y": 0, "w": 16, "h": 8}));
    assert!(errors_of(&v).is_empty());
}

// ----------------------------------------------------------------- data

case!(
    data_with_both_members,
    |v| {
        insert(
            &mut v,
            &format!("{P1}/data"),
            "query",
            json!({"source": "prod", "format": "table", "sql": ["SELECT 1"], "min_interval": null, "max_data_points": null}),
        );
    },
    "/sections/0/panels/1/data/reuse",
    ErrorCode::NotPermitted
);
case!(
    data_with_no_member,
    |v| {
        set(&mut v, &format!("{P1}/data"), json!({}));
    },
    "/sections/0/panels/1/data",
    ErrorCode::MissingMember
);
case!(
    data_unknown_member,
    |v| {
        insert(&mut v, &format!("{P1}/data"), "other", json!(1));
    },
    "/sections/0/panels/1/data/other",
    ErrorCode::UnknownMember
);
case!(
    data_source_bad,
    |v| {
        set(&mut v, &format!("{P0}/data/query/source"), json!("Prod_1"));
    },
    "/sections/0/panels/0/data/query/source",
    ErrorCode::BadFormat
);
case!(
    data_format_bad,
    |v| {
        set(&mut v, &format!("{P0}/data/query/format"), json!("wide"));
    },
    "/sections/0/panels/0/data/query/format",
    ErrorCode::BadFormat
);
case!(
    data_min_interval_fraction,
    |v| {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!("1.5m"));
    },
    "/sections/0/panels/0/data/query/min_interval",
    ErrorCode::BadFormat
);
case!(
    data_min_interval_two_units,
    |v| {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!("1m30s"));
    },
    "/sections/0/panels/0/data/query/min_interval",
    ErrorCode::BadFormat
);
case!(
    data_min_interval_zero,
    |v| {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!("0ms"));
    },
    "/sections/0/panels/0/data/query/min_interval",
    ErrorCode::BadFormat
);
case!(
    data_min_interval_too_large,
    |v| {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!("2d"));
    },
    "/sections/0/panels/0/data/query/min_interval",
    ErrorCode::OutOfRange
);
case!(
    data_min_interval_too_large_in_hours,
    |v| {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!("25h"));
    },
    "/sections/0/panels/0/data/query/min_interval",
    ErrorCode::OutOfRange
);
case!(
    data_max_data_points_zero,
    |v| {
        set(&mut v, &format!("{P0}/data/query/max_data_points"), json!(0));
    },
    "/sections/0/panels/0/data/query/max_data_points",
    ErrorCode::OutOfRange
);
case!(
    data_max_data_points_too_large,
    |v| {
        set(&mut v, &format!("{P0}/data/query/max_data_points"), json!(10_001));
    },
    "/sections/0/panels/0/data/query/max_data_points",
    ErrorCode::OutOfRange
);
case!(
    data_max_data_points_fraction,
    |v| {
        set(&mut v, &format!("{P0}/data/query/max_data_points"), json!(5.5));
    },
    "/sections/0/panels/0/data/query/max_data_points",
    ErrorCode::WrongType
);
case!(
    reuse_with_transforms_wrong_type,
    |v| {
        set(&mut v, &format!("{P1}/data/reuse/with_transforms"), json!("yes"));
    },
    "/sections/0/panels/1/data/reuse/with_transforms",
    ErrorCode::WrongType
);

#[test]
fn data_valid_extremes() {
    let mut v = base();
    for (interval, points) in [("1ms", 1), ("1d", 10_000), ("24h", 500), ("1440m", 7), ("86400s", 8), ("86400000ms", 9)]
    {
        set(&mut v, &format!("{P0}/data/query/min_interval"), json!(interval));
        set(&mut v, &format!("{P0}/data/query/max_data_points"), json!(points));
        assert!(errors_of(&v).is_empty(), "{interval}");
    }
}

#[test]
fn reuse_must_name_another_panel() {
    let mut v = base();
    set(&mut v, &format!("{P1}/data/reuse/panel"), json!(3));
    let e = expect(&v, "/sections/0/panels/1/data/reuse/panel", ErrorCode::BadReference);
    assert!(e.message.contains("this panel"), "{}", e.message);
}

#[test]
fn reuse_chain_is_invalid() {
    let mut v = base();
    // Panel 2 (a table with a query) now reuses panel 3, and panel 3 reuses panel 2.
    set(&mut v, &format!("{T0}/data"), json!({"reuse": {"panel": 3, "with_transforms": false}}));
    let e = errors_of(&v);
    let pointers: Vec<&str> = e.iter().map(|x| x.pointer.as_str()).collect();
    assert_eq!(pointers, ["/sections/0/panels/1/data/reuse/panel", "/sections/1/panels/0/data/reuse/panel"]);
    assert!(e.iter().all(|x| x.code == ErrorCode::BadReference));
}

#[test]
fn reuse_of_a_reuse_through_a_third_panel() {
    let mut v = base();
    let mut third = panel_template(9);
    third["data"] = json!({"reuse": {"panel": 3, "with_transforms": false}});
    third["grid"] = json!({"x": 0, "y": 20, "w": 4, "h": 4});
    v.pointer_mut("/sections/0/panels").unwrap().as_array_mut().unwrap().push(third);
    expect(&v, "/sections/0/panels/2/data/reuse/panel", ErrorCode::BadReference);
}

#[test]
fn reuse_can_name_a_panel_in_a_later_section_and_a_collapsed_one() {
    // The worked example already does it: panel 3 comes before panel 2 and is in another section.
    assert!(errors_of(&base()).is_empty());
    let mut v = base();
    set(&mut v, "/sections/1/collapsed", json!(true));
    assert!(errors_of(&v).is_empty());
}

// ------------------------------------------------- lists on a panel

#[test]
fn panel_list_limits() {
    let transform = json!({"kind": "grouping_to_matrix", "column_field": "a", "row_field": "b", "value_field": "c", "empty_value": "null"});
    let link = json!({"title": "t", "url": "/x", "target_blank": false});
    let mut v = base();
    set(&mut v, &format!("{P0}/transforms"), Value::Array(vec![transform.clone(); MAX_TRANSFORMS]));
    set(&mut v, &format!("{P0}/links"), Value::Array(vec![link.clone(); MAX_LINKS]));
    assert!(errors_of(&v).is_empty());
    set(&mut v, &format!("{P0}/transforms"), Value::Array(vec![transform; MAX_TRANSFORMS + 1]));
    expect(&v, "/sections/0/panels/0/transforms", ErrorCode::TooLarge);
    set(&mut v, &format!("{P0}/transforms"), json!([]));
    set(&mut v, &format!("{P0}/links"), Value::Array(vec![link; MAX_LINKS + 1]));
    expect(&v, "/sections/0/panels/0/links", ErrorCode::TooLarge);
}

#[test]
fn several_errors_in_the_walk_order() {
    let mut v = base();
    set(&mut v, "/title", json!(""));
    set(&mut v, &format!("{P0}/grid/x"), json!(30));
    set(&mut v, "/variables/1/hide", json!("x"));
    insert(&mut v, "", "zz", json!(1));
    let pointers: Vec<String> = errors_of(&v).into_iter().map(|e| e.pointer).collect();
    assert_eq!(pointers, ["/title", "/variables/1/hide", "/sections/0/panels/0/grid/x", "/zz"]);
}
