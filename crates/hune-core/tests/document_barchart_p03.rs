// SPDX-License-Identifier: MIT OR Apache-2.0
//! The members (P03): the options `fill_opacity` and `line_width` of a `barchart`,
//! the value `auto` of its `orientation` (4.30), and the property `axis_grid` (4.18).
//!
//! Each member is optional. An absent member has its default value, and the canonical writer leaves out a member
//! that has its default value. The base is `all_types.json`, which has none of the new members.

use hune_core::document::{BAR_FILL_OPACITY, BAR_LINE_WIDTH, BarchartOrientation, PanelOptions, TextEnum, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

const TS: &str = "/sections/0/panels/1";
const BC: &str = "/sections/1/panels/0";
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

fn insert(v: &mut Value, pointer: &str, key: &str, new: Value) {
    at(v, pointer).as_object_mut().unwrap().insert(key.to_owned(), new);
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{e:#?}");
}

fn canonical(v: &Value) -> Value {
    let bytes = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}")).bytes().to_vec();
    serde_json::from_slice(&bytes).unwrap()
}

/// The canonical text (4.38), whose members keep the order of the writer.
fn canonical_text(v: &Value) -> String {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    String::from_utf8(doc.bytes().to_vec()).unwrap()
}

/// The positions of the names in the text, each one searched after the one before it.
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

fn bar_options(v: &Value) -> hune_core::document::BarchartOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    match &doc.document().sections[1].panels[0].options {
        PanelOptions::Barchart(o) => o.clone(),
        other => panic!("not a bar chart: {other:?}"),
    }
}

#[test]
fn absent_members_have_the_grafana_defaults() {
    let o = bar_options(&base());
    assert_eq!((o.fill_opacity, o.line_width), (80, 1));
    assert_eq!((BAR_FILL_OPACITY, BAR_LINE_WIDTH), (80, 1));
    let doc = validate(&serde_json::to_vec(&base()).unwrap(), None).unwrap();
    assert_eq!(doc.document().sections[1].panels[0].fields.defaults.axis_grid, None);
}

#[test]
fn fill_opacity_and_line_width_take_their_whole_range() {
    for (fill, line) in [(0, 0), (61, 4), (100, 10), (1, 9)] {
        let mut v = base();
        insert(&mut v, &format!("{BC}/options"), "fill_opacity", json!(fill));
        insert(&mut v, &format!("{BC}/options"), "line_width", json!(line));
        let o = bar_options(&v);
        assert_eq!((i64::from(o.fill_opacity), i64::from(o.line_width)), (fill, line));
        // The canonical form keeps a value that is not the default, at the end of the options, in the order of 4.30.
        let text = canonical_text(&v);
        let names = ["x_tick_rotation", "legend", "tooltip", "fill_opacity", "line_width", "fields"];
        assert!(in_order(&text, &names), "{text}");
    }
}

#[test]
fn the_canonical_form_leaves_out_the_defaults() {
    let mut v = base();
    insert(&mut v, &format!("{BC}/options"), "fill_opacity", json!(80));
    insert(&mut v, &format!("{BC}/options"), "line_width", json!(1));
    assert_eq!(canonical(&v), canonical(&base()));
    let mut w = base();
    insert(&mut w, &format!("{BC}/options"), "fill_opacity", json!(61));
    let c = canonical(&w);
    assert_eq!(c.pointer(&format!("{BC}/options/fill_opacity")), Some(&json!(61)));
    assert_eq!(c.pointer(&format!("{BC}/options/line_width")), None);
}

#[test]
fn values_out_of_range_or_of_another_type_are_refused() {
    for (key, bad, code) in [
        ("fill_opacity", json!(101), ErrorCode::OutOfRange),
        ("fill_opacity", json!(-1), ErrorCode::OutOfRange),
        ("fill_opacity", json!(61.5), ErrorCode::WrongType),
        ("fill_opacity", json!(null), ErrorCode::WrongType),
        ("fill_opacity", json!("61"), ErrorCode::WrongType),
        ("line_width", json!(11), ErrorCode::OutOfRange),
        ("line_width", json!(-1), ErrorCode::OutOfRange),
        ("line_width", json!(true), ErrorCode::WrongType),
    ] {
        let mut v = base();
        insert(&mut v, &format!("{BC}/options"), key, bad.clone());
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{key} = {bad}: {e:#?}");
        assert_eq!(e[0].pointer, format!("{BC}/options/{key}"), "{key} = {bad}");
        assert_eq!(e[0].code, code, "{key} = {bad}: {e:#?}");
    }
}

#[test]
fn the_two_options_are_not_options_of_another_type() {
    // A bar gauge has a closed options object with no fill and no line (4.33).
    let mut v = base();
    insert(&mut v, &format!("{BG}/options"), "fill_opacity", json!(61));
    expect(&v, &format!("{BG}/options/fill_opacity"), ErrorCode::UnknownMember);
}

#[test]
fn orientation_auto_is_a_bar_chart_value_only() {
    let mut v = base();
    *at(&mut v, &format!("{BC}/options/orientation")) = json!("auto");
    assert_eq!(bar_options(&v).orientation, BarchartOrientation::Auto);
    assert_eq!(canonical(&v).pointer(&format!("{BC}/options/orientation")), Some(&json!("auto")));
    assert_eq!(BarchartOrientation::Auto.as_str(), "auto");
    // The bar gauge keeps its two values (4.33).
    let mut w = base();
    *at(&mut w, &format!("{BG}/options/orientation")) = json!("auto");
    expect(&w, &format!("{BG}/options/orientation"), ErrorCode::BadFormat);
    let mut x = base();
    *at(&mut x, &format!("{BC}/options/orientation")) = json!("Auto");
    expect(&x, &format!("{BC}/options/orientation"), ErrorCode::BadFormat);
}

#[test]
fn axis_grid_is_a_bar_chart_property_in_the_defaults_and_in_an_override() {
    for value in [true, false] {
        let mut v = base();
        insert(&mut v, &format!("{BC}/fields/defaults"), "axis_grid", json!(value));
        let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
        assert_eq!(doc.document().sections[1].panels[0].fields.defaults.axis_grid, Some(value));
        // The canonical order of 4.18: after `unit` (and after `soft_max`, which the base does not have).
        let text = canonical_text(&v);
        let at = text.find("\"axis_grid\"").unwrap();
        let from = text[..at].rfind("\"defaults\"").unwrap();
        let to = at + text[at..].find('}').unwrap() + 1;
        let compact: String = text[from..to].chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(compact, format!("\"defaults\":{{\"unit\":\"bytes\",\"axis_grid\":{value}}}"));
    }
    let mut v = base();
    let item = json!({"match": {"by": "name", "value": "a"}, "set": {"soft_max": 10, "axis_grid": false}});
    at(&mut v, &format!("{BC}/fields/overrides")).as_array_mut().unwrap().push(item);
    let c = canonical(&v);
    assert_eq!(c.pointer(&format!("{BC}/fields/overrides/0/set/axis_grid")), Some(&json!(false)));
    assert!(in_order(&canonical_text(&v), &["overrides", "soft_max", "axis_grid"]));
}

#[test]
fn axis_grid_is_refused_in_another_type_and_as_null_or_text() {
    let mut v = base();
    insert(&mut v, &format!("{TS}/fields/defaults"), "axis_grid", json!(false));
    expect(&v, &format!("{TS}/fields/defaults/axis_grid"), ErrorCode::NotPermitted);
    for bad in [json!(null), json!("off"), json!(0)] {
        let mut w = base();
        insert(&mut w, &format!("{BC}/fields/defaults"), "axis_grid", bad.clone());
        expect(&w, &format!("{BC}/fields/defaults/axis_grid"), ErrorCode::WrongType);
    }
}
