//! The members that only the browser uses: fields, property sets, thresholds, mappings, links,
//! colours, units, transforms, patterns and the options of the seven panel types (4.17 to 4.33).
//!
//! The base is `all_types.json`: it has the seven panel types and every member at least once.
//! Panels: `/sections/0/panels/0` table, `.../1` timeseries, `.../2` stat (reuse of the table),
//! `/sections/1/panels/0` barchart, `.../1` piechart, `.../2` xychart, `.../3` bargauge.

use hune_core::document::validate;
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::*;
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

const TB: &str = "/sections/0/panels/0";
const TS: &str = "/sections/0/panels/1";
const ST: &str = "/sections/0/panels/2";
const BC: &str = "/sections/1/panels/0";
const PC: &str = "/sections/1/panels/1";
const XY: &str = "/sections/1/panels/2";
const BG: &str = "/sections/1/panels/3";

fn base() -> Value {
    serde_json::from_slice(ALL_TYPES).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
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

fn expect(v: &Value, pointer: &str, code: ErrorCode) -> ValidationError {
    let mut e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    let e = e.remove(0);
    assert_eq!((e.pointer.as_str(), e.code), (pointer, code), "{e:#?}");
    e
}

fn valid(v: &Value) {
    let e = errors_of(v);
    assert!(e.is_empty(), "{e:#?}");
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

fn defaults(panel: &str) -> String {
    format!("{panel}/fields/defaults")
}

#[test]
fn the_base_is_valid() {
    valid(&base());
}

// ------------------------------------------------------------- units

case!(
    unit_unknown,
    |v| {
        set(&mut v, &format!("{}/unit", defaults(TB)), json!("kilobytes"));
    },
    "/sections/0/panels/0/fields/defaults/unit",
    ErrorCode::BadFormat
);
case!(
    unit_suffix_empty,
    |v| {
        set(&mut v, &format!("{}/unit", defaults(TB)), json!("suffix:"));
    },
    "/sections/0/panels/0/fields/defaults/unit",
    ErrorCode::BadFormat
);
case!(
    unit_prefix_too_long,
    |v| {
        set(&mut v, &format!("{}/unit", defaults(TB)), json!(format!("prefix:{}", "x".repeat(33))));
    },
    "/sections/0/panels/0/fields/defaults/unit",
    ErrorCode::BadFormat
);
case!(
    unit_not_text,
    |v| {
        set(&mut v, &format!("{}/unit", defaults(TB)), json!(5));
    },
    "/sections/0/panels/0/fields/defaults/unit",
    ErrorCode::WrongType
);
case!(
    unit_null_is_not_permitted,
    |v| {
        set(&mut v, &format!("{}/unit", defaults(TB)), json!(null));
    },
    "/sections/0/panels/0/fields/defaults/unit",
    ErrorCode::WrongType
);

#[test]
fn units_of_4_20_are_valid() {
    for unit in [
        "none",
        "short",
        "percent",
        "percentunit",
        "ms",
        "s",
        "bytes",
        "decbytes",
        "suffix: pts",
        "suffix:x",
        "prefix:$",
    ] {
        let mut v = base();
        set(&mut v, &format!("{}/unit", defaults(TB)), json!(unit));
        valid(&v);
    }
    let mut v = base();
    set(&mut v, &format!("{}/unit", defaults(TB)), json!(format!("suffix:{}", "é".repeat(32))));
    valid(&v);
}

// ------------------------------------------------- decimals, min, max

case!(
    decimals_too_large,
    |v| {
        set(&mut v, &format!("{}/decimals", defaults(TB)), json!(21));
    },
    "/sections/0/panels/0/fields/defaults/decimals",
    ErrorCode::OutOfRange
);
case!(
    decimals_negative,
    |v| {
        set(&mut v, &format!("{}/decimals", defaults(TB)), json!(-1));
    },
    "/sections/0/panels/0/fields/defaults/decimals",
    ErrorCode::OutOfRange
);
case!(
    decimals_fraction,
    |v| {
        set(&mut v, &format!("{}/decimals", defaults(TB)), json!(1.5));
    },
    "/sections/0/panels/0/fields/defaults/decimals",
    ErrorCode::WrongType
);
case!(
    min_not_a_number,
    |v| {
        set(&mut v, &format!("{}/min", defaults(TB)), json!("0"));
    },
    "/sections/0/panels/0/fields/defaults/min",
    ErrorCode::WrongType
);
case!(
    min_equal_to_max,
    |v| {
        set(&mut v, &format!("{}/min", defaults(TB)), json!(5));
        set(&mut v, &format!("{}/max", defaults(TB)), json!(5));
    },
    "/sections/0/panels/0/fields/defaults/max",
    ErrorCode::OutOfRange
);
case!(
    min_greater_than_max_in_an_override,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/1/set/min"), json!(2));
        set(&mut v, &format!("{TB}/fields/overrides/1/set/max"), json!(1));
    },
    "/sections/0/panels/0/fields/overrides/1/set/max",
    ErrorCode::OutOfRange
);

#[test]
fn a_number_that_is_not_finite_is_out_of_range() {
    let text = String::from_utf8(ALL_TYPES.to_vec()).unwrap().replacen("\"max\": 1000", "\"max\": 1e999", 1);
    let e = validate(text.as_bytes(), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), ("/sections/0/panels/0/fields/defaults/max", ErrorCode::OutOfRange));
}

#[test]
fn numbers_keep_their_double_value() {
    let text = String::from_utf8(ALL_TYPES.to_vec()).unwrap().replacen("\"max\": 1000", "\"max\": 1.0E3", 1);
    let doc = validate(text.as_bytes(), None).unwrap();
    assert_eq!(doc.bytes(), ALL_TYPES);
}

#[test]
fn min_and_max_use_the_scale_of_any_sign() {
    let mut v = base();
    set(&mut v, &format!("{}/min", defaults(TB)), json!(-1e300));
    set(&mut v, &format!("{}/max", defaults(TB)), json!(1e300));
    valid(&v);
}

// --------------------------------------------------------- display names

case!(
    display_name_empty,
    |v| {
        set(&mut v, &format!("{}/display_name", defaults(TB)), json!(""));
    },
    "/sections/0/panels/0/fields/defaults/display_name",
    ErrorCode::OutOfRange
);
case!(
    no_value_empty,
    |v| {
        set(&mut v, &format!("{}/no_value", defaults(TB)), json!(""));
    },
    "/sections/0/panels/0/fields/defaults/no_value",
    ErrorCode::OutOfRange
);
case!(
    no_value_in_an_override,
    |v| {
        insert(&mut v, &format!("{TB}/fields/overrides/1/set"), "no_value", json!("-"));
    },
    "/sections/0/panels/0/fields/overrides/1/set/no_value",
    ErrorCode::NotPermitted
);

#[test]
fn no_value_in_defaults_is_valid_for_every_type() {
    let mut v = base();
    for panel in [TB, TS, ST, BC, PC, XY, BG] {
        insert(&mut v, &defaults(panel), "no_value", json!("n/a"));
    }
    valid(&v);
}

// --------------------------------------------------- table-only properties

#[test]
fn table_only_properties_are_refused_in_other_panels() {
    let cases = [
        ("width", json!(100)),
        ("align", json!("left")),
        ("cell", json!({"type": "auto", "mode": null})),
        ("filterable", json!(true)),
        ("hidden", json!(true)),
        ("inspect", json!(true)),
    ];
    for panel in [TS, ST, BC, PC, XY, BG] {
        for (name, value) in &cases {
            let mut v = base();
            insert(&mut v, &defaults(panel), name, value.clone());
            let pointer = format!("{}/{name}", defaults(panel));
            expect(&v, &pointer, ErrorCode::NotPermitted);
            // The same in an override set.
            let mut v = base();
            set(
                &mut v,
                &format!("{panel}/fields/overrides"),
                json!([{"match": {"by": "name", "value": "a"}, "set": {name.to_string(): value}}]),
            );
            let pointer = format!("{panel}/fields/overrides/0/set/{name}");
            expect(&v, &pointer, ErrorCode::NotPermitted);
        }
    }
}

case!(
    width_too_small,
    |v| {
        set(&mut v, &format!("{}/width", defaults(TB)), json!(9));
    },
    "/sections/0/panels/0/fields/defaults/width",
    ErrorCode::OutOfRange
);
case!(
    width_too_large,
    |v| {
        set(&mut v, &format!("{}/width", defaults(TB)), json!(4001));
    },
    "/sections/0/panels/0/fields/defaults/width",
    ErrorCode::OutOfRange
);
case!(
    align_bad,
    |v| {
        set(&mut v, &format!("{}/align", defaults(TB)), json!("justify"));
    },
    "/sections/0/panels/0/fields/defaults/align",
    ErrorCode::BadFormat
);
case!(
    filterable_wrong_type,
    |v| {
        set(&mut v, &format!("{}/filterable", defaults(TB)), json!(1));
    },
    "/sections/0/panels/0/fields/defaults/filterable",
    ErrorCode::WrongType
);
case!(
    inspect_wrong_type,
    |v| {
        insert(&mut v, &defaults(TB), "inspect", json!("true"));
    },
    "/sections/0/panels/0/fields/defaults/inspect",
    ErrorCode::WrongType
);
case!(
    inspect_null,
    |v| {
        insert(&mut v, &defaults(TB), "inspect", Value::Null);
    },
    "/sections/0/panels/0/fields/defaults/inspect",
    ErrorCode::WrongType
);

/// The member `inspect` of wave H (4.18): a boolean of a table in `fields.defaults` and in an override set. The canonical
/// form keeps it after `footer` and before `hide_from`, and a second validation gives the same bytes.
#[test]
fn inspect_is_a_table_boolean_in_the_canonical_order() {
    let mut v = base();
    insert(&mut v, &defaults(TB), "inspect", json!(true));
    insert(&mut v, &defaults(TB), "hide_from", json!({"legend": false, "tooltip": false, "viz": false}));
    set(
        &mut v,
        &format!("{TB}/fields/overrides"),
        json!([{"match": {"by": "name", "value": "a"}, "set": {"inspect": false}}]),
    );
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(doc.bytes()).unwrap();
    // The canonical text keeps the order of 4.18: `hidden`, then `inspect`, then `hide_from` (the table has no `footer`).
    let i = text.find("\"inspect\": true").unwrap();
    assert!(text[..i].trim_end().ends_with("\"hidden\": false,"), "{}", &text[i.saturating_sub(80)..i + 40]);
    let after = &text[i + "\"inspect\": true".len()..];
    assert!(
        after.trim_start_matches(|c: char| c == ',' || c.is_whitespace()).starts_with("\"hide_from\""),
        "{}",
        &after[..60]
    );
    let canonical: Value = serde_json::from_str(text).unwrap();
    assert_eq!(canonical.pointer(&format!("{}/inspect", defaults(TB))), Some(&json!(true)));
    assert_eq!(canonical.pointer(&format!("{TB}/fields/overrides/0/set/inspect")), Some(&json!(false)));
    let again = validate(doc.bytes(), None).unwrap();
    assert_eq!(again.bytes(), doc.bytes());
}

case!(
    cell_type_bad,
    |v| {
        set(&mut v, &format!("{}/cell/type", defaults(TB)), json!("sparkline"));
    },
    "/sections/0/panels/0/fields/defaults/cell/type",
    ErrorCode::BadFormat
);
case!(
    cell_mode_with_color_text,
    |v| {
        set(&mut v, &format!("{}/cell/mode", defaults(TB)), json!("basic"));
    },
    "/sections/0/panels/0/fields/defaults/cell/mode",
    ErrorCode::NotPermitted
);
case!(
    cell_background_without_mode,
    |v| {
        set(&mut v, &format!("{}/cell", defaults(TB)), json!({"type": "color_background", "mode": null}));
    },
    "/sections/0/panels/0/fields/defaults/cell/mode",
    ErrorCode::NotPermitted
);
case!(
    cell_mode_bad,
    |v| {
        set(&mut v, &format!("{}/cell", defaults(TB)), json!({"type": "color_background", "mode": "lcd"}));
    },
    "/sections/0/panels/0/fields/defaults/cell/mode",
    ErrorCode::BadFormat
);
case!(
    cell_missing_mode,
    |v| {
        remove(&mut v, &format!("{}/cell/mode", defaults(TB)));
    },
    "/sections/0/panels/0/fields/defaults/cell",
    ErrorCode::MissingMember
);

#[test]
fn cell_valid_forms() {
    let mut v = base();
    for cell in [
        json!({"type": "auto", "mode": null}),
        json!({"type": "color_text", "mode": null}),
        json!({"type": "color_background", "mode": "basic"}),
        json!({"type": "color_background", "mode": "gradient"}),
    ] {
        set(&mut v, &format!("{}/cell", defaults(TB)), cell);
        valid(&v);
    }
}

// ---------------------------------------------------------------- colours

#[test]
fn colour_tokens_hex_and_functions_are_valid() {
    let mut colours = vec!["text".to_owned(), "transparent".to_owned()];
    for hue in ["red", "orange", "yellow", "green", "blue", "purple"] {
        for prefix in ["super-light-", "light-", "", "semi-dark-", "dark-"] {
            colours.push(format!("{prefix}{hue}"));
        }
    }
    colours.extend(
        [
            "#abc",
            "#ABCDEF",
            "#12345678",
            "rgb(0, 255, 7)",
            "rgba(1, 2, 3, 0)",
            "rgba(1, 2, 3, 1)",
            "rgba(1, 2, 3, 0.999)",
        ]
        .map(String::from),
    );
    assert_eq!(colours.len(), 39);
    for c in colours {
        let mut v = base();
        set(&mut v, &format!("{}/color", defaults(TB)), json!({"mode": "fixed", "fixed": c}));
        valid(&v);
        let mut v = base();
        set(&mut v, &format!("{}/thresholds/steps/1/color", defaults(TB)), json!(c));
        valid(&v);
    }
}

#[test]
fn bad_colours_are_refused_everywhere_a_colour_is_read() {
    for bad in [
        "Red",
        "#abcd",
        "rgb(256, 0, 0)",
        "rgb(1,2,3)",
        "rgba(1, 2, 3, 2)",
        "rgba(1, 2, 3, 0.1234)",
        "light-text",
        "",
        "#ggg",
    ] {
        let mut v = base();
        set(&mut v, &format!("{TB}/fields/overrides/0/set/color"), json!({"mode": "fixed", "fixed": bad}));
        expect(&v, "/sections/0/panels/0/fields/overrides/0/set/color/fixed", ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &format!("{}/thresholds/steps/0/color", defaults(TB)), json!(bad));
        expect(&v, "/sections/0/panels/0/fields/defaults/thresholds/steps/0/color", ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &format!("{}/mappings/0/color", defaults(TB)), json!(bad));
        expect(&v, "/sections/0/panels/0/fields/defaults/mappings/0/color", ErrorCode::BadFormat);
    }
}

case!(
    color_mode_bad,
    |v| {
        set(&mut v, &format!("{}/color/mode", defaults(TB)), json!("gradient"));
    },
    "/sections/0/panels/0/fields/defaults/color/mode",
    ErrorCode::BadFormat
);
case!(
    color_fixed_mode_without_colour,
    |v| {
        set(&mut v, &format!("{}/color", defaults(TB)), json!({"mode": "fixed", "fixed": null}));
    },
    "/sections/0/panels/0/fields/defaults/color/fixed",
    ErrorCode::NotPermitted
);
case!(
    color_other_mode_with_colour,
    |v| {
        set(&mut v, &format!("{}/color", defaults(TB)), json!({"mode": "thresholds", "fixed": "red"}));
    },
    "/sections/0/panels/0/fields/defaults/color/fixed",
    ErrorCode::NotPermitted
);
case!(
    color_palette_with_colour,
    |v| {
        set(&mut v, &format!("{}/color", defaults(TB)), json!({"mode": "palette", "fixed": "red"}));
    },
    "/sections/0/panels/0/fields/defaults/color/fixed",
    ErrorCode::NotPermitted
);
case!(
    color_missing_fixed,
    |v| {
        remove(&mut v, &format!("{}/color/fixed", defaults(TB)));
    },
    "/sections/0/panels/0/fields/defaults/color",
    ErrorCode::MissingMember
);

// ------------------------------------------------------------- thresholds

fn thresholds_path(panel: &str) -> String {
    format!("{}/thresholds", defaults(panel))
}

case!(
    thresholds_mode_bad,
    |v| {
        set(&mut v, &format!("{}/mode", thresholds_path(TB)), json!("relative"));
    },
    "/sections/0/panels/0/fields/defaults/thresholds/mode",
    ErrorCode::BadFormat
);
case!(
    thresholds_no_step,
    |v| {
        set(&mut v, &format!("{}/steps", thresholds_path(TB)), json!([]));
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps",
    ErrorCode::OutOfRange
);
case!(
    thresholds_first_step_with_a_number,
    |v| {
        set(&mut v, &format!("{}/steps", thresholds_path(TB)), json!([{"value": 1, "color": "red"}]));
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/0/value",
    ErrorCode::NotPermitted
);
case!(
    thresholds_second_step_with_null,
    |v| {
        set(
            &mut v,
            &format!("{}/steps", thresholds_path(TB)),
            json!([{"value": null, "color": "red"}, {"value": null, "color": "blue"}]),
        );
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/1/value",
    ErrorCode::NotPermitted
);
case!(
    thresholds_not_increasing,
    |v| {
        set(
            &mut v,
            &format!("{}/steps", thresholds_path(TB)),
            json!([{"value": null, "color": "red"}, {"value": 5, "color": "blue"}, {"value": 4, "color": "green"}]),
        );
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/2/value",
    ErrorCode::OutOfRange
);
case!(
    thresholds_equal_values,
    |v| {
        set(
            &mut v,
            &format!("{}/steps", thresholds_path(TB)),
            json!([{"value": null, "color": "red"}, {"value": 5, "color": "blue"}, {"value": 5, "color": "green"}]),
        );
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/2/value",
    ErrorCode::OutOfRange
);
case!(
    thresholds_value_wrong_type,
    |v| {
        set(
            &mut v,
            &format!("{}/steps", thresholds_path(TB)),
            json!([{"value": null, "color": "red"}, {"value": "5", "color": "blue"}]),
        );
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/1/value",
    ErrorCode::WrongType
);
case!(
    thresholds_step_missing_value,
    |v| {
        set(&mut v, &format!("{}/steps", thresholds_path(TB)), json!([{"color": "red"}]));
    },
    "/sections/0/panels/0/fields/defaults/thresholds/steps/0",
    ErrorCode::MissingMember
);

#[test]
fn thresholds_negative_values_increase() {
    let mut v = base();
    set(
        &mut v,
        &format!("{}/steps", thresholds_path(TB)),
        json!([{"value": null, "color": "red"}, {"value": -10, "color": "blue"}, {"value": -9.5, "color": "green"}, {"value": 0, "color": "text"}]),
    );
    valid(&v);
}

#[test]
fn thresholds_step_count() {
    let steps = |n: usize| {
        let mut s = vec![json!({"value": null, "color": "red"})];
        s.extend((1..n).map(|i| json!({"value": i, "color": "blue"})));
        Value::Array(s)
    };
    let mut v = base();
    set(&mut v, &format!("{}/steps", thresholds_path(TB)), steps(MAX_THRESHOLD_STEPS));
    valid(&v);
    set(&mut v, &format!("{}/steps", thresholds_path(TB)), steps(MAX_THRESHOLD_STEPS + 1));
    expect(&v, "/sections/0/panels/0/fields/defaults/thresholds/steps", ErrorCode::TooLarge);
}

// --------------------------------------------------------------- mappings

fn mapping_path(i: usize) -> String {
    format!("{}/mappings/{i}", defaults(TB))
}

#[test]
fn mapping_kinds_are_valid() {
    let mut v = base();
    let mappings = json!([
        {"kind": "value", "value": "", "text": "empty", "color": null},
        {"kind": "value", "value": "x".repeat(1024), "text": null, "color": "red"},
        {"kind": "range", "from": 1, "to": 1, "text": "one", "color": null},
        {"kind": "range", "from": -5.5, "to": null, "text": null, "color": "blue"},
        {"kind": "regex", "pattern": "^a(b)$", "text": "$1", "color": null},
        {"kind": "regex", "pattern": "/^a/gi", "text": null, "color": "green"},
        {"kind": "special", "match": "null", "text": "-", "color": null},
        {"kind": "special", "match": "nan", "text": "-", "color": null},
        {"kind": "special", "match": "null_nan", "text": "-", "color": null},
        {"kind": "special", "match": "true", "text": "yes", "color": null},
        {"kind": "special", "match": "false", "text": "no", "color": null},
        {"kind": "special", "match": "empty", "text": "none", "color": null}
    ]);
    set(&mut v, &format!("{}/mappings", defaults(TB)), mappings);
    valid(&v);
}

case!(
    mappings_empty,
    |v| {
        set(&mut v, &format!("{}/mappings", defaults(TB)), json!([]));
    },
    "/sections/0/panels/0/fields/defaults/mappings",
    ErrorCode::OutOfRange
);
case!(
    mapping_kind_unknown,
    |v| {
        set(&mut v, &format!("{}/kind", mapping_path(0)), json!("other"));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0/kind",
    ErrorCode::BadFormat
);
case!(
    mapping_without_text_and_colour,
    |v| {
        set(&mut v, &mapping_path(0), json!({"kind": "value", "value": "1", "text": null, "color": null}));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0",
    ErrorCode::MissingMember
);
case!(
    mapping_text_empty,
    |v| {
        set(&mut v, &format!("{}/text", mapping_path(0)), json!(""));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0/text",
    ErrorCode::OutOfRange
);
case!(
    mapping_text_too_long,
    |v| {
        set(&mut v, &format!("{}/text", mapping_path(0)), json!("a".repeat(257)));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0/text",
    ErrorCode::OutOfRange
);
case!(
    mapping_value_too_long,
    |v| {
        set(&mut v, &format!("{}/value", mapping_path(0)), json!("a".repeat(1025)));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0/value",
    ErrorCode::OutOfRange
);
case!(
    mapping_value_member_of_another_kind,
    |v| {
        insert(&mut v, &mapping_path(0), "pattern", json!("a"));
    },
    "/sections/0/panels/0/fields/defaults/mappings/0/pattern",
    ErrorCode::UnknownMember
);
case!(
    mapping_range_without_bound,
    |v| {
        set(&mut v, &mapping_path(1), json!({"kind": "range", "from": null, "to": null, "text": "x", "color": null}));
    },
    "/sections/0/panels/0/fields/defaults/mappings/1",
    ErrorCode::MissingMember
);
case!(
    mapping_range_from_above_to,
    |v| {
        set(&mut v, &mapping_path(1), json!({"kind": "range", "from": 6, "to": 5, "text": "x", "color": null}));
    },
    "/sections/0/panels/0/fields/defaults/mappings/1",
    ErrorCode::OutOfRange
);
case!(
    mapping_range_bound_wrong_type,
    |v| {
        set(&mut v, &mapping_path(1), json!({"kind": "range", "from": "1", "to": 5, "text": "x", "color": null}));
    },
    "/sections/0/panels/0/fields/defaults/mappings/1/from",
    ErrorCode::WrongType
);
case!(
    mapping_regex_bad_pattern,
    |v| {
        set(&mut v, &format!("{}/pattern", mapping_path(3)), json!("(?i)abc"));
    },
    "/sections/0/panels/0/fields/defaults/mappings/3/pattern",
    ErrorCode::BadFormat
);
case!(
    mapping_regex_grafana_form_without_end,
    |v| {
        set(&mut v, &format!("{}/pattern", mapping_path(3)), json!("/abc"));
    },
    "/sections/0/panels/0/fields/defaults/mappings/3/pattern",
    ErrorCode::BadFormat
);
case!(
    mapping_special_bad,
    |v| {
        set(&mut v, &format!("{}/match", mapping_path(4)), json!("zero"));
    },
    "/sections/0/panels/0/fields/defaults/mappings/4/match",
    ErrorCode::BadFormat
);

#[test]
fn mappings_count() {
    let item = json!({"kind": "value", "value": "1", "text": "a", "color": null});
    let mut v = base();
    set(&mut v, &format!("{}/mappings", defaults(TB)), Value::Array(vec![item.clone(); MAX_MAPPINGS]));
    valid(&v);
    set(&mut v, &format!("{}/mappings", defaults(TB)), Value::Array(vec![item; MAX_MAPPINGS + 1]));
    expect(&v, "/sections/0/panels/0/fields/defaults/mappings", ErrorCode::TooLarge);
}

// ------------------------------------------------------------------ links

fn data_links(v: &mut Value, links: Value) {
    set(v, &format!("{}/links", defaults(TB)), links);
}

#[test]
fn link_url_prefix_rule() {
    let good = [
        "/d/x",
        "?x=1",
        "#top",
        "http://a.example",
        "https://a.example/${__value.raw}",
        "${__url_time_range}",
        "${a}/b",
        "/a${b}",
        "/",
    ];
    for url in good {
        let mut v = base();
        data_links(&mut v, json!([{"title": "t", "url": url, "target_blank": true}]));
        valid(&v);
        let mut v = base();
        set(&mut v, &format!("{TB}/links"), json!([{"title": "t", "url": url, "target_blank": true}]));
        valid(&v);
    }
    let bad = [
        "//evil.example",
        "/\\evil.example",
        "javascript:alert(1)",
        "data:text/html,x",
        "ftp://a",
        "a/b",
        "http${x}",
        "mailto:a@b",
        "HTTP://a",
    ];
    for url in bad {
        let mut v = base();
        data_links(&mut v, json!([{"title": "t", "url": url, "target_blank": true}]));
        expect(&v, "/sections/0/panels/0/fields/defaults/links/0/url", ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &format!("{TB}/links"), json!([{"title": "t", "url": url, "target_blank": false}]));
        expect(&v, "/sections/0/panels/0/links/0/url", ErrorCode::BadFormat);
    }
}

#[test]
fn link_other_rules() {
    let ok = json!({"title": "t", "url": "/x", "target_blank": false});
    let mut v = base();
    data_links(&mut v, json!([]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links", ErrorCode::OutOfRange);
    data_links(&mut v, Value::Array(vec![ok.clone(); MAX_LINKS]));
    valid(&v);
    data_links(&mut v, Value::Array(vec![ok.clone(); MAX_LINKS + 1]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links", ErrorCode::TooLarge);
    data_links(&mut v, json!([{"title": "", "url": "/x", "target_blank": false}]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links/0/title", ErrorCode::OutOfRange);
    data_links(&mut v, json!([{"title": "t", "url": "", "target_blank": false}]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links/0/url", ErrorCode::OutOfRange);
    data_links(&mut v, json!([{"title": "t", "url": format!("/{}", "a".repeat(2048)), "target_blank": false}]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links/0/url", ErrorCode::OutOfRange);
    data_links(&mut v, json!([{"title": "t", "url": format!("/{}", "a".repeat(2047)), "target_blank": false}]));
    valid(&v);
    data_links(&mut v, json!([{"title": "t", "url": "/x", "target_blank": "yes"}]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links/0/target_blank", ErrorCode::WrongType);
    data_links(&mut v, json!([{"title": "t", "url": "/x"}]));
    expect(&v, "/sections/0/panels/0/fields/defaults/links/0", ErrorCode::MissingMember);
}

// -------------------------------------------------------------- overrides

case!(
    override_without_set,
    |v| {
        remove(&mut v, &format!("{TB}/fields/overrides/0/set"));
    },
    "/sections/0/panels/0/fields/overrides/0",
    ErrorCode::MissingMember
);
case!(
    override_empty_set,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/0/set"), json!({}));
    },
    "/sections/0/panels/0/fields/overrides/0/set",
    ErrorCode::MissingMember
);
case!(
    override_unknown_property,
    |v| {
        insert(&mut v, &format!("{TB}/fields/overrides/0/set"), "colour", json!("red"));
    },
    "/sections/0/panels/0/fields/overrides/0/set/colour",
    ErrorCode::UnknownMember
);
case!(
    override_match_by_bad,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/0/match/by"), json!("prefix"));
    },
    "/sections/0/panels/0/fields/overrides/0/match/by",
    ErrorCode::BadFormat
);
case!(
    override_name_empty,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/1/match/value"), json!(""));
    },
    "/sections/0/panels/0/fields/overrides/1/match/value",
    ErrorCode::OutOfRange
);
case!(
    override_name_too_long,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/1/match/value"), json!("a".repeat(1025)));
    },
    "/sections/0/panels/0/fields/overrides/1/match/value",
    ErrorCode::OutOfRange
);
case!(
    override_regex_bad,
    |v| {
        set(&mut v, &format!("{TB}/fields/overrides/0/match/value"), json!("/abc"));
    },
    "/sections/0/panels/0/fields/overrides/0/match/value",
    ErrorCode::BadFormat
);
case!(
    override_match_unknown_member,
    |v| {
        insert(&mut v, &format!("{TB}/fields/overrides/0/match"), "flags", json!("i"));
    },
    "/sections/0/panels/0/fields/overrides/0/match/flags",
    ErrorCode::UnknownMember
);
case!(
    fields_unknown_member,
    |v| {
        insert(&mut v, &format!("{TB}/fields"), "extra", json!({}));
    },
    "/sections/0/panels/0/fields/extra",
    ErrorCode::UnknownMember
);
case!(
    fields_missing_overrides,
    |v| {
        remove(&mut v, &format!("{TB}/fields/overrides"));
    },
    "/sections/0/panels/0/fields",
    ErrorCode::MissingMember
);

#[test]
fn override_by_name_value_is_free_text_and_by_regex_is_a_pattern() {
    let mut v = base();
    // The text "/abc" is a name for `name` but an error for `regex`.
    set(&mut v, &format!("{TB}/fields/overrides/1/match/value"), json!("/abc"));
    valid(&v);
    set(&mut v, &format!("{TB}/fields/overrides/1/match"), json!({"by": "regex", "value": "/abc"}));
    expect(&v, "/sections/0/panels/0/fields/overrides/1/match/value", ErrorCode::BadFormat);
    set(&mut v, &format!("{TB}/fields/overrides/1/match"), json!({"by": "regex", "value": "Total"}));
    valid(&v);
    set(&mut v, &format!("{TB}/fields/overrides/1/match"), json!({"by": "regex", "value": "/^Total$/"}));
    valid(&v);
}

#[test]
fn overrides_count() {
    let item = json!({"match": {"by": "name", "value": "a"}, "set": {"decimals": 1}});
    let mut v = base();
    set(&mut v, &format!("{TB}/fields/overrides"), Value::Array(vec![item.clone(); MAX_OVERRIDES]));
    valid(&v);
    set(&mut v, &format!("{TB}/fields/overrides"), Value::Array(vec![item; MAX_OVERRIDES + 1]));
    expect(&v, "/sections/0/panels/0/fields/overrides", ErrorCode::TooLarge);
}

// --------------------------------------------------------------- patterns

#[test]
fn each_member_that_holds_a_pattern_uses_the_subset() {
    let places = [
        "/variables/0/regex".to_owned(),
        format!("{TB}/fields/overrides/0/match/value"),
        format!("{}/pattern", mapping_path(3)),
        format!("{ST}/options/reduce/fields"),
        format!("{TB}/transforms/0/filters/8/matcher/pattern"),
    ];
    // `ST` is a reuse panel, but it has its own options.
    for place in &places {
        for bad in ["(?i)a", "\\s", "[[:alpha:]]", "a{", "\\A"] {
            let mut v = base();
            set(&mut v, place, json!(bad));
            let e = errors_of(&v);
            assert_eq!(e.len(), 1, "{place} {bad}: {e:#?}");
            assert_eq!((e[0].pointer.as_str(), e[0].code), (place.as_str(), ErrorCode::BadFormat), "{bad}");
        }
        let mut v = base();
        set(&mut v, place, json!("a|b"));
        valid(&v);
        let mut v = base();
        set(&mut v, place, json!(""));
        expect(&v, place, ErrorCode::OutOfRange);
        let mut v = base();
        set(&mut v, place, json!("a".repeat(MAX_PATTERN_BYTES + 1)));
        expect(&v, place, ErrorCode::OutOfRange);
        let mut v = base();
        set(&mut v, place, json!("a".repeat(MAX_PATTERN_BYTES)));
        valid(&v);
    }
}

#[test]
fn grafana_form_applies_to_four_members_and_the_plain_form_to_the_matcher() {
    let grafana = [
        "/variables/0/regex".to_owned(),
        format!("{TB}/fields/overrides/0/match/value"),
        format!("{}/pattern", mapping_path(3)),
        format!("{ST}/options/reduce/fields"),
    ];
    for place in &grafana {
        let mut v = base();
        set(&mut v, place, json!("/a/gimsx"));
        expect(&v, place, ErrorCode::BadFormat);
        set(&mut v, place, json!("/a/gims"));
        valid(&v);
        set(&mut v, place, json!("/a/gg"));
        expect(&v, place, ErrorCode::BadFormat);
        set(&mut v, place, json!("/a"));
        expect(&v, place, ErrorCode::BadFormat);
    }
    // The plain form has no slash syntax: "/a" is the pattern "/a".
    let place = format!("{TB}/transforms/0/filters/8/matcher/pattern");
    for text in ["/a", "/a/", "/a/x"] {
        let mut v = base();
        set(&mut v, &place, json!(text));
        valid(&v);
    }
}

// ------------------------------------------------------------- transforms

fn filter_path(i: usize) -> String {
    format!("{TB}/transforms/0/filters/{i}")
}

case!(
    transform_kind_bad,
    |v| {
        set(&mut v, &format!("{TB}/transforms/0/kind"), json!("prepare_time_series"));
    },
    "/sections/0/panels/0/transforms/0/kind",
    ErrorCode::BadFormat
);
case!(
    transform_mode_bad,
    |v| {
        set(&mut v, &format!("{TB}/transforms/0/mode"), json!("keep"));
    },
    "/sections/0/panels/0/transforms/0/mode",
    ErrorCode::BadFormat
);
case!(
    transform_match_bad,
    |v| {
        set(&mut v, &format!("{TB}/transforms/0/match"), json!("one"));
    },
    "/sections/0/panels/0/transforms/0/match",
    ErrorCode::BadFormat
);
case!(
    transform_filters_empty,
    |v| {
        set(&mut v, &format!("{TB}/transforms/0/filters"), json!([]));
    },
    "/sections/0/panels/0/transforms/0/filters",
    ErrorCode::OutOfRange
);
case!(
    transform_member_of_another_kind,
    |v| {
        insert(&mut v, &format!("{TB}/transforms/1"), "mode", json!("include"));
    },
    "/sections/0/panels/0/transforms/1/mode",
    ErrorCode::UnknownMember
);
case!(
    transform_missing_member,
    |v| {
        remove(&mut v, &format!("{TB}/transforms/1/row_field"));
    },
    "/sections/0/panels/0/transforms/1",
    ErrorCode::MissingMember
);
case!(
    transform_empty_value_bad,
    |v| {
        set(&mut v, &format!("{TB}/transforms/1/empty_value"), json!("zero"));
    },
    "/sections/0/panels/0/transforms/1/empty_value",
    ErrorCode::BadFormat
);
case!(
    transform_field_empty,
    |v| {
        set(&mut v, &format!("{TB}/transforms/1/column_field"), json!(""));
    },
    "/sections/0/panels/0/transforms/1/column_field",
    ErrorCode::OutOfRange
);
case!(
    transform_field_too_long,
    |v| {
        set(&mut v, &format!("{TB}/transforms/1/value_field"), json!("a".repeat(1025)));
    },
    "/sections/0/panels/0/transforms/1/value_field",
    ErrorCode::OutOfRange
);
case!(
    filter_field_empty,
    |v| {
        set(&mut v, &format!("{}/field", filter_path(0)), json!(""));
    },
    "/sections/0/panels/0/transforms/0/filters/0/field",
    ErrorCode::OutOfRange
);
case!(
    matcher_kind_bad,
    |v| {
        set(&mut v, &format!("{}/matcher/kind", filter_path(0)), json!("between"));
    },
    "/sections/0/panels/0/transforms/0/filters/0/matcher/kind",
    ErrorCode::BadFormat
);
case!(
    matcher_equal_wrong_type,
    |v| {
        set(&mut v, &format!("{}/matcher/value", filter_path(0)), json!(true));
    },
    "/sections/0/panels/0/transforms/0/filters/0/matcher/value",
    ErrorCode::WrongType
);
case!(
    matcher_equal_text_too_long,
    |v| {
        set(&mut v, &format!("{}/matcher/value", filter_path(1)), json!("a".repeat(1025)));
    },
    "/sections/0/panels/0/transforms/0/filters/1/matcher/value",
    ErrorCode::OutOfRange
);
case!(
    matcher_greater_with_text,
    |v| {
        set(&mut v, &format!("{}/matcher/value", filter_path(3)), json!("1"));
    },
    "/sections/0/panels/0/transforms/0/filters/3/matcher/value",
    ErrorCode::WrongType
);
case!(
    matcher_greater_without_value,
    |v| {
        remove(&mut v, &format!("{}/matcher/value", filter_path(3)));
    },
    "/sections/0/panels/0/transforms/0/filters/3/matcher",
    ErrorCode::MissingMember
);
case!(
    matcher_range_from_above_to,
    |v| {
        set(&mut v, &format!("{}/matcher", filter_path(7)), json!({"kind": "range", "from": 5, "to": 4}));
    },
    "/sections/0/panels/0/transforms/0/filters/7/matcher/to",
    ErrorCode::OutOfRange
);
case!(
    matcher_range_with_text,
    |v| {
        set(&mut v, &format!("{}/matcher", filter_path(7)), json!({"kind": "range", "from": "a", "to": 4}));
    },
    "/sections/0/panels/0/transforms/0/filters/7/matcher/from",
    ErrorCode::WrongType
);
case!(
    matcher_is_null_with_value,
    |v| {
        insert(&mut v, &format!("{}/matcher", filter_path(9)), "value", json!(1));
    },
    "/sections/0/panels/0/transforms/0/filters/9/matcher/value",
    ErrorCode::UnknownMember
);

#[test]
fn matcher_valid_forms() {
    let mut v = base();
    let matchers = [
        json!({"kind": "equal", "value": 0}),
        json!({"kind": "equal", "value": ""}),
        json!({"kind": "not_equal", "value": -1.5}),
        json!({"kind": "range", "from": 3, "to": 3}),
        json!({"kind": "regex", "pattern": "^[a-z]+$"}),
    ];
    let filters: Vec<Value> = matchers.into_iter().map(|m| json!({"field": "x", "matcher": m})).collect();
    set(&mut v, &format!("{TB}/transforms/0/filters"), Value::Array(filters));
    valid(&v);
}

#[test]
fn transform_and_filter_counts() {
    let filter = json!({"field": "x", "matcher": {"kind": "is_null"}});
    let mut v = base();
    set(&mut v, &format!("{TB}/transforms/0/filters"), Value::Array(vec![filter.clone(); MAX_FILTERS]));
    valid(&v);
    set(&mut v, &format!("{TB}/transforms/0/filters"), Value::Array(vec![filter; MAX_FILTERS + 1]));
    expect(&v, "/sections/0/panels/0/transforms/0/filters", ErrorCode::TooLarge);
}

// ---------------------------------------------------------------- options

fn opt(panel: &str, member: &str) -> String {
    format!("{panel}/options/{member}")
}

#[test]
fn options_are_closed_for_every_type() {
    for panel in [TB, TS, ST, BC, PC, XY, BG] {
        let mut v = base();
        insert(&mut v, &format!("{panel}/options"), "extra", json!(1));
        expect(&v, &opt(panel, "extra"), ErrorCode::UnknownMember);
    }
}

#[test]
fn each_option_member_is_necessary() {
    let members: [(&str, &[&str]); 7] = [
        (TB, &["show_header", "sort"]),
        (
            TS,
            &[
                "draw",
                "interpolation",
                "line_width",
                "fill_opacity",
                "show_points",
                "point_size",
                "span_nulls",
                "stacking",
                "y_scale",
                "legend",
                "tooltip",
            ],
        ),
        (ST, &["reduce", "color_mode", "graph_mode", "text_mode", "orientation"]),
        (
            BC,
            &[
                "orientation",
                "x_field",
                "stacking",
                "show_values",
                "bar_width",
                "group_width",
                "x_tick_rotation",
                "legend",
                "tooltip",
            ],
        ),
        (PC, &["pie_type", "reduce", "labels", "legend", "tooltip"]),
        (XY, &["x_field", "y_fields", "draw", "point_size", "line_width", "legend", "tooltip"]),
        (BG, &["reduce", "orientation", "display_mode", "show_unfilled"]),
    ];
    for (panel, names) in members {
        for name in names {
            let mut v = base();
            remove(&mut v, &opt(panel, name));
            expect(&v, &format!("{panel}/options"), ErrorCode::MissingMember);
        }
    }
}

#[test]
fn every_enum_value_of_every_option_is_valid_and_another_is_not() {
    let enums: [(&str, &str, &[&str]); 24] = [
        (TS, "draw", &["line", "bars", "points"]),
        (TS, "interpolation", &["linear", "smooth", "step_before", "step_after"]),
        (TS, "show_points", &["auto", "always", "never"]),
        (TS, "stacking", &["none", "normal", "percent"]),
        (TS, "y_scale", &["linear", "log"]),
        (TS, "legend/placement", &["bottom", "right"]),
        (TS, "legend/mode", &["list", "table"]),
        (TS, "tooltip/mode", &["single", "multi", "hidden"]),
        (
            ST,
            "reduce/calc",
            &["last", "last_not_null", "first", "first_not_null", "min", "max", "mean", "total", "count"],
        ),
        (ST, "color_mode", &["none", "value", "background"]),
        (ST, "graph_mode", &["none", "area"]),
        (ST, "text_mode", &["auto", "value", "value_and_name", "name", "none"]),
        (ST, "orientation", &["horizontal", "vertical", "auto"]),
        (BC, "orientation", &["vertical", "horizontal"]),
        (BC, "stacking", &["none", "normal", "percent"]),
        (BC, "show_values", &["auto", "always", "never"]),
        (PC, "pie_type", &["pie", "donut"]),
        (XY, "draw", &["points", "lines", "points_and_lines"]),
        (BG, "orientation", &["horizontal", "vertical"]),
        (BG, "display_mode", &["basic", "gradient", "lcd"]),
        (
            BG,
            "reduce/calc",
            &["last", "min", "max", "mean", "total", "count", "first", "last_not_null", "first_not_null"],
        ),
        (BC, "legend/mode", &["list", "table"]),
        (PC, "tooltip/mode", &["single", "multi", "hidden"]),
        (XY, "legend/placement", &["bottom", "right"]),
    ];
    for (panel, member, values) in enums {
        for value in values {
            let mut v = base();
            set(&mut v, &opt(panel, member), json!(value));
            valid(&v);
        }
        let mut v = base();
        set(&mut v, &opt(panel, member), json!("other"));
        expect(&v, &opt(panel, member), ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &opt(panel, member), json!(1));
        expect(&v, &opt(panel, member), ErrorCode::WrongType);
    }
}

#[test]
fn integer_options_have_their_ranges() {
    let ints: [(&str, &str, i64, i64); 8] = [
        (TS, "line_width", 0, 10),
        (TS, "fill_opacity", 0, 100),
        (TS, "point_size", 1, 40),
        (BC, "bar_width", 10, 100),
        (BC, "group_width", 10, 100),
        (BC, "x_tick_rotation", -90, 90),
        (XY, "point_size", 1, 40),
        (XY, "line_width", 0, 10),
    ];
    for (panel, member, min, max) in ints {
        for good in [min, max] {
            let mut v = base();
            set(&mut v, &opt(panel, member), json!(good));
            valid(&v);
        }
        for bad in [min - 1, max + 1] {
            let mut v = base();
            set(&mut v, &opt(panel, member), json!(bad));
            expect(&v, &opt(panel, member), ErrorCode::OutOfRange);
        }
        let mut v = base();
        set(&mut v, &opt(panel, member), json!(min as f64 + 0.5));
        expect(&v, &opt(panel, member), ErrorCode::WrongType);
    }
}

#[test]
fn boolean_options() {
    for (panel, member) in
        [(TB, "show_header"), (TS, "span_nulls"), (BG, "show_unfilled"), (ST, "reduce/values"), (TS, "legend/show")]
    {
        let mut v = base();
        set(&mut v, &opt(panel, member), json!("true"));
        expect(&v, &opt(panel, member), ErrorCode::WrongType);
    }
}

// legend

case!(
    legend_values_duplicate,
    |v| {
        set(&mut v, &opt(TS, "legend/values"), json!(["mean", "last", "mean"]));
    },
    "/sections/0/panels/1/options/legend/values/2",
    ErrorCode::NotUnique
);
case!(
    legend_values_too_many,
    |v| {
        set(&mut v, &opt(TS, "legend/values"), json!(["total", "min", "max", "mean", "last", "last"]));
    },
    "/sections/0/panels/1/options/legend/values",
    ErrorCode::OutOfRange
);
case!(
    legend_values_pie_item_in_a_time_chart,
    |v| {
        set(&mut v, &opt(TS, "legend/values"), json!(["percent"]));
    },
    "/sections/0/panels/1/options/legend/values/0",
    ErrorCode::BadFormat
);
case!(
    legend_values_time_item_in_a_pie,
    |v| {
        set(&mut v, &opt(PC, "legend/values"), json!(["total"]));
    },
    "/sections/1/panels/1/options/legend/values/0",
    ErrorCode::BadFormat
);
case!(
    legend_values_pie_too_many,
    |v| {
        set(&mut v, &opt(PC, "legend/values"), json!(["value", "percent", "value"]));
    },
    "/sections/1/panels/1/options/legend/values",
    ErrorCode::OutOfRange
);
case!(
    legend_unknown_member,
    |v| {
        insert(&mut v, &opt(TS, "legend"), "x", json!(1));
    },
    "/sections/0/panels/1/options/legend/x",
    ErrorCode::UnknownMember
);

#[test]
fn legend_values_valid() {
    let mut v = base();
    set(&mut v, &opt(TS, "legend/values"), json!([]));
    valid(&v);
    set(&mut v, &opt(PC, "legend/values"), json!(["percent"]));
    valid(&v);
    set(&mut v, &opt(PC, "legend/values"), json!(["percent", "value"]));
    valid(&v);
    set(&mut v, &opt(TS, "legend/values"), json!(["last", "total"]));
    valid(&v);
}

// reduce

case!(
    reduce_fields_bad_pattern,
    |v| {
        set(&mut v, &opt(ST, "reduce/fields"), json!("(?=a)"));
    },
    "/sections/0/panels/2/options/reduce/fields",
    ErrorCode::BadFormat
);
case!(
    reduce_missing_member,
    |v| {
        remove(&mut v, &opt(ST, "reduce/calc"));
    },
    "/sections/0/panels/2/options/reduce",
    ErrorCode::MissingMember
);
case!(
    reduce_unknown_member,
    |v| {
        insert(&mut v, &opt(BG, "reduce"), "x", json!(1));
    },
    "/sections/1/panels/3/options/reduce/x",
    ErrorCode::UnknownMember
);

#[test]
fn reduce_fields_null_and_pattern_forms() {
    let mut v = base();
    set(&mut v, &opt(ST, "reduce/fields"), json!(null));
    valid(&v);
    set(&mut v, &opt(ST, "reduce/fields"), json!("/^weight$/"));
    valid(&v);
    set(&mut v, &opt(ST, "reduce/fields"), json!("weight"));
    valid(&v);
}

// table

case!(
    table_sort_too_many,
    |v| {
        set(&mut v, &opt(TB, "sort"), Value::Array(vec![json!({"field": "a", "desc": false}); MAX_SORT_KEYS + 1]));
    },
    "/sections/0/panels/0/options/sort",
    ErrorCode::TooLarge
);
case!(
    table_sort_field_empty,
    |v| {
        set(&mut v, &opt(TB, "sort/0/field"), json!(""));
    },
    "/sections/0/panels/0/options/sort/0/field",
    ErrorCode::OutOfRange
);
case!(
    table_sort_desc_wrong_type,
    |v| {
        set(&mut v, &opt(TB, "sort/0/desc"), json!("yes"));
    },
    "/sections/0/panels/0/options/sort/0/desc",
    ErrorCode::WrongType
);
case!(
    table_sort_unknown_member,
    |v| {
        insert(&mut v, &opt(TB, "sort/0"), "x", json!(1));
    },
    "/sections/0/panels/0/options/sort/0/x",
    ErrorCode::UnknownMember
);

#[test]
fn table_sort_count_boundary() {
    let mut v = base();
    set(&mut v, &opt(TB, "sort"), Value::Array(vec![json!({"field": "a", "desc": false}); MAX_SORT_KEYS]));
    valid(&v);
    set(&mut v, &opt(TB, "sort"), json!([]));
    valid(&v);
}

// barchart and xychart fields

case!(
    barchart_x_field_empty,
    |v| {
        set(&mut v, &opt(BC, "x_field"), json!(""));
    },
    "/sections/1/panels/0/options/x_field",
    ErrorCode::OutOfRange
);
case!(
    barchart_x_field_too_long,
    |v| {
        set(&mut v, &opt(BC, "x_field"), json!("a".repeat(1025)));
    },
    "/sections/1/panels/0/options/x_field",
    ErrorCode::OutOfRange
);
case!(
    xychart_x_field_empty,
    |v| {
        set(&mut v, &opt(XY, "x_field"), json!(""));
    },
    "/sections/1/panels/2/options/x_field",
    ErrorCode::OutOfRange
);
case!(
    xychart_y_field_empty,
    |v| {
        set(&mut v, &opt(XY, "y_fields"), json!(["a", ""]));
    },
    "/sections/1/panels/2/options/y_fields/1",
    ErrorCode::OutOfRange
);
case!(
    xychart_y_fields_too_many,
    |v| {
        set(&mut v, &opt(XY, "y_fields"), Value::Array((0..=MAX_Y_FIELDS).map(|i| json!(format!("y{i}"))).collect()));
    },
    "/sections/1/panels/2/options/y_fields",
    ErrorCode::TooLarge
);

#[test]
fn field_name_options_valid() {
    let mut v = base();
    set(&mut v, &opt(BC, "x_field"), json!(null));
    set(&mut v, &opt(XY, "x_field"), json!(null));
    set(&mut v, &opt(XY, "y_fields"), json!([]));
    valid(&v);
    set(&mut v, &opt(XY, "y_fields"), Value::Array((0..MAX_Y_FIELDS).map(|i| json!(format!("y{i}"))).collect()));
    valid(&v);
}

// piechart labels

case!(
    pie_labels_duplicate,
    |v| {
        set(&mut v, &opt(PC, "labels"), json!(["name", "name"]));
    },
    "/sections/1/panels/1/options/labels/1",
    ErrorCode::NotUnique
);
case!(
    pie_labels_too_many,
    |v| {
        set(&mut v, &opt(PC, "labels"), json!(["name", "value", "percent", "name"]));
    },
    "/sections/1/panels/1/options/labels",
    ErrorCode::OutOfRange
);
case!(
    pie_label_bad,
    |v| {
        set(&mut v, &opt(PC, "labels"), json!(["total"]));
    },
    "/sections/1/panels/1/options/labels/0",
    ErrorCode::BadFormat
);

#[test]
fn pie_labels_valid() {
    let mut v = base();
    set(&mut v, &opt(PC, "labels"), json!([]));
    valid(&v);
    set(&mut v, &opt(PC, "labels"), json!(["percent", "name"]));
    valid(&v);
}

// panels in the other sections of the base

#[test]
fn the_seven_panel_types_are_in_the_base() {
    let doc = validate(ALL_TYPES, None).unwrap();
    let types: Vec<&str> =
        doc.document().panels().map(|(_, p)| hune_core::document::TextEnum::as_str(p.panel_type)).collect();
    assert_eq!(types, ["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge"]);
}

#[test]
fn wrong_type_for_a_whole_object() {
    for place in [
        format!("{TB}/fields"),
        format!("{TB}/fields/defaults"),
        format!("{TB}/fields/overrides/0/match"),
        format!("{}/color", defaults(TB)),
        format!("{}/thresholds", defaults(TB)),
        format!("{TS}/options"),
        format!("{TS}/options/legend"),
        format!("{TB}/transforms/0"),
        format!("{TB}/transforms/0/filters/0/matcher"),
    ] {
        let mut v = base();
        set(&mut v, &place, json!([]));
        expect(&v, &place, ErrorCode::WrongType);
    }
}
