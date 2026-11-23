// SPDX-License-Identifier: MIT OR Apache-2.0
//! The members (P10 and P14): the option `gradient_mode` of a `timeseries` and of a `barchart`
//! (4.28, 4.30), the options `log_base`, `span_nulls_ms` and `bar_alignment` of a `timeseries` (4.28), and the properties
//! `line_dash_array`, `axis_centered_zero`, `axis_width`, `axis_border` and `axis_color` (4.18).
//!
//! Each member is optional. An absent member has its default value, and the canonical writer leaves out a member that has its
//! default value. The base is `all_types.json`, which has none of the new members.

use hune_core::document::{AxisColor, BarAlignment, GradientMode, LOG_BASE, PanelOptions, TextEnum, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

const TS: &str = "/sections/0/panels/1";
const ST: &str = "/sections/0/panels/2";
const BC: &str = "/sections/1/panels/0";
const XY: &str = "/sections/1/panels/2";

fn base() -> Value {
    serde_json::from_slice(ALL_TYPES).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn insert(v: &mut Value, pointer: &str, key: &str, new: Value) {
    v.pointer_mut(pointer)
        .unwrap_or_else(|| panic!("no value at {pointer}"))
        .as_object_mut()
        .unwrap()
        .insert(key.to_owned(), new);
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{e:#?}");
}

fn canonical(v: &Value) -> Value {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    serde_json::from_slice(doc.bytes()).unwrap()
}

fn text_of(v: &Value) -> String {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    String::from_utf8(doc.bytes().to_vec()).unwrap()
}

fn in_order(text: &str, names: &[&str]) -> bool {
    let mut from = 0;
    for n in names {
        match text[from..].find(&format!("\"{n}\"")) {
            Some(i) => from += i + n.len(),
            None => return false,
        }
    }
    true
}

fn options_of(v: &Value, section: usize, panel: usize) -> PanelOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    doc.document().sections[section].panels[panel].options.clone()
}

#[test]
fn absent_members_have_their_defaults() {
    match options_of(&base(), 0, 1) {
        PanelOptions::Timeseries(o) => {
            assert_eq!(
                (o.gradient_mode, o.log_base, o.span_nulls_ms, o.bar_alignment),
                (GradientMode::None, 10, None, BarAlignment::Center)
            );
        }
        other => panic!("{other:?}"),
    }
    match options_of(&base(), 1, 0) {
        PanelOptions::Barchart(o) => assert_eq!(o.gradient_mode, GradientMode::None),
        other => panic!("{other:?}"),
    }
    assert_eq!(LOG_BASE, 10);
    let doc = validate(&serde_json::to_vec(&base()).unwrap(), None).unwrap();
    let d = &doc.document().sections[0].panels[1].fields.defaults;
    assert_eq!(
        (d.line_dash_array.clone(), d.axis_centered_zero, d.axis_width, d.axis_border, d.axis_color),
        (None, None, None, None, None)
    );
}

#[test]
fn the_options_take_their_values_and_keep_the_order_of_4_28() {
    let mut v = base();
    let o = format!("{TS}/options");
    insert(&mut v, &o, "gradient_mode", json!("scheme"));
    insert(&mut v, &o, "log_base", json!(2));
    insert(&mut v, &o, "span_nulls_ms", json!(150000));
    insert(&mut v, &o, "bar_alignment", json!("before"));
    match options_of(&v, 0, 1) {
        PanelOptions::Timeseries(t) => {
            assert_eq!(
                (t.gradient_mode, t.log_base, t.span_nulls_ms, t.bar_alignment),
                (GradientMode::Scheme, 2, Some(150000), BarAlignment::Before)
            );
        }
        other => panic!("{other:?}"),
    }
    let text = text_of(&v);
    assert!(
        in_order(
            &text,
            &["y_scale", "legend", "tooltip", "gradient_mode", "log_base", "span_nulls_ms", "bar_alignment", "fields"]
        ),
        "{text}"
    );
    for mode in ["opacity", "hue", "scheme"] {
        let mut b = base();
        insert(&mut b, &format!("{BC}/options"), "gradient_mode", json!(mode));
        assert_eq!(canonical(&b).pointer(&format!("{BC}/options/gradient_mode")), Some(&json!(mode)));
        assert!(in_order(&text_of(&b), &["x_tick_rotation", "tooltip", "gradient_mode", "fields"]));
    }
    assert_eq!(GradientMode::Hue.as_str(), "hue");
    assert_eq!(BarAlignment::After.as_str(), "after");
}

#[test]
fn the_canonical_form_leaves_out_the_defaults() {
    let mut v = base();
    insert(&mut v, &format!("{TS}/options"), "gradient_mode", json!("none"));
    insert(&mut v, &format!("{TS}/options"), "log_base", json!(10));
    insert(&mut v, &format!("{TS}/options"), "bar_alignment", json!("center"));
    insert(&mut v, &format!("{BC}/options"), "gradient_mode", json!("none"));
    assert_eq!(canonical(&v), canonical(&base()));
    // A log base and a threshold that have no effect (a linear scale, `span_nulls` true) are valid: the page ignores them (9.19).
    let mut w = base();
    insert(&mut w, &format!("{TS}/options"), "log_base", json!(2));
    *w.pointer_mut(&format!("{TS}/options/span_nulls")).unwrap() = json!(true);
    insert(&mut w, &format!("{TS}/options"), "span_nulls_ms", json!(1));
    assert!(errors_of(&w).is_empty(), "{:#?}", errors_of(&w));
}

#[test]
fn values_out_of_range_or_of_another_type_are_refused() {
    let ts = format!("{TS}/options");
    for (key, bad, code) in [
        ("gradient_mode", json!("Hue"), ErrorCode::BadFormat),
        ("gradient_mode", json!(1), ErrorCode::WrongType),
        ("log_base", json!(3), ErrorCode::OutOfRange),
        ("log_base", json!(1), ErrorCode::OutOfRange),
        ("log_base", json!(16), ErrorCode::OutOfRange),
        ("log_base", json!(2.5), ErrorCode::WrongType),
        ("log_base", json!("2"), ErrorCode::WrongType),
        ("span_nulls_ms", json!(0), ErrorCode::OutOfRange),
        ("span_nulls_ms", json!(-5), ErrorCode::OutOfRange),
        ("span_nulls_ms", json!(1_000_000_000_001_i64), ErrorCode::OutOfRange),
        ("span_nulls_ms", json!(1.5), ErrorCode::WrongType),
        ("span_nulls_ms", json!(null), ErrorCode::WrongType),
        ("bar_alignment", json!(-1), ErrorCode::WrongType),
        ("bar_alignment", json!("left"), ErrorCode::BadFormat),
    ] {
        let mut v = base();
        insert(&mut v, &ts, key, bad.clone());
        expect(&v, &format!("{ts}/{key}"), code);
    }
    let mut big = base();
    insert(&mut big, &ts, "span_nulls_ms", json!(1_000_000_000_000_i64));
    assert!(errors_of(&big).is_empty());
}

#[test]
fn the_options_belong_to_their_types() {
    // A stat and an xy chart have closed options with no gradient (4.29, 4.32). A bar chart has no log base and no alignment (4.30).
    for (pointer, key, value) in [
        (ST, "gradient_mode", json!("hue")),
        (XY, "gradient_mode", json!("hue")),
        (BC, "log_base", json!(2)),
        (BC, "bar_alignment", json!("after")),
        (BC, "span_nulls_ms", json!(1000)),
    ] {
        let mut v = base();
        insert(&mut v, &format!("{pointer}/options"), key, value);
        expect(&v, &format!("{pointer}/options/{key}"), ErrorCode::UnknownMember);
    }
}

#[test]
fn the_axis_properties_are_time_series_and_bar_chart_properties() {
    for pointer in [TS, BC] {
        let mut v = base();
        let d = format!("{pointer}/fields/defaults");
        insert(&mut v, &d, "axis_centered_zero", json!(true));
        insert(&mut v, &d, "axis_width", json!(80));
        insert(&mut v, &d, "axis_border", json!(true));
        insert(&mut v, &d, "axis_color", json!("series"));
        let c = canonical(&v);
        assert_eq!(c.pointer(&format!("{d}/axis_width")), Some(&json!(80)));
        assert_eq!(c.pointer(&format!("{d}/axis_color")), Some(&json!("series")));
        assert!(in_order(&text_of(&v), &["unit", "axis_centered_zero", "axis_width", "axis_border", "axis_color"]));
        // The same members in an override.
        let mut w = base();
        let item = json!({"match": {"by": "name", "value": "a"}, "set": {"axis_width": 0, "axis_border": false, "axis_color": "text", "axis_centered_zero": false}});
        w.pointer_mut(&format!("{pointer}/fields/overrides")).unwrap().as_array_mut().unwrap().push(item);
        assert!(errors_of(&w).is_empty(), "{:#?}", errors_of(&w));
    }
    assert_eq!(AxisColor::Series.as_str(), "series");
    // Another type gives `not_permitted`.
    for key in ["axis_centered_zero", "axis_width", "axis_border", "axis_color", "line_dash_array"] {
        let mut v = base();
        let value = match key {
            "axis_width" => json!(10),
            "axis_color" => json!("text"),
            "line_dash_array" => json!([10, 20]),
            _ => json!(true),
        };
        insert(&mut v, &format!("{ST}/fields/defaults"), key, value);
        expect(&v, &format!("{ST}/fields/defaults/{key}"), ErrorCode::NotPermitted);
    }
    // The dash array is a time series property only.
    let mut b = base();
    insert(&mut b, &format!("{BC}/fields/defaults"), "line_dash_array", json!([10, 20]));
    expect(&b, &format!("{BC}/fields/defaults/line_dash_array"), ErrorCode::NotPermitted);
}

#[test]
fn bad_axis_values_are_refused() {
    let d = format!("{TS}/fields/defaults");
    for (key, bad, code) in [
        ("axis_width", json!(501), ErrorCode::OutOfRange),
        ("axis_width", json!(-1), ErrorCode::OutOfRange),
        ("axis_width", json!(3.5), ErrorCode::WrongType),
        ("axis_border", json!("true"), ErrorCode::WrongType),
        ("axis_centered_zero", json!(null), ErrorCode::WrongType),
        ("axis_color", json!("blue"), ErrorCode::BadFormat),
    ] {
        let mut v = base();
        insert(&mut v, &d, key, bad.clone());
        expect(&v, &format!("{d}/{key}"), code);
    }
}

#[test]
fn the_dash_array_has_1_to_8_integers_from_0_to_100() {
    let d = format!("{TS}/fields/defaults");
    for good in [json!([10]), json!([10, 20]), json!([30, 3, 3]), json!([0, 20]), json!([100, 0, 1, 2, 3, 4, 5, 6])] {
        let mut v = base();
        insert(&mut v, &d, "line_dash", json!("dash"));
        insert(&mut v, &d, "line_dash_array", good.clone());
        assert_eq!(canonical(&v).pointer(&format!("{d}/line_dash_array")), Some(&good));
        assert!(in_order(&text_of(&v), &["line_dash", "line_dash_array"]));
    }
    // With no `line_dash`, the array alone is valid: the line has dashes (9.19).
    let mut alone = base();
    insert(&mut alone, &d, "line_dash_array", json!([20, 10]));
    assert!(errors_of(&alone).is_empty());
    for (bad, code, at) in [
        (json!([]), ErrorCode::OutOfRange, String::new()),
        (json!([1, 1, 1, 1, 1, 1, 1, 1, 1]), ErrorCode::TooLarge, String::new()),
        (json!([0, 0]), ErrorCode::OutOfRange, String::new()),
        (json!([10, 101]), ErrorCode::OutOfRange, "/1".to_owned()),
        (json!([10, -1]), ErrorCode::OutOfRange, "/1".to_owned()),
        (json!([10, 2.5]), ErrorCode::WrongType, "/1".to_owned()),
        (json!("10, 20"), ErrorCode::WrongType, String::new()),
    ] {
        let mut v = base();
        insert(&mut v, &d, "line_dash_array", bad.clone());
        expect(&v, &format!("{d}/line_dash_array{at}"), code);
    }
}
