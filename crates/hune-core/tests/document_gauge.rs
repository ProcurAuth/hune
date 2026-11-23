// SPDX-License-Identifier: MIT OR Apache-2.0
//! The panel type `gauge` (P12) (4.13, 4.33e): the classic gauge of Grafana 12.4.2.
//!
//! The type is a new value of the enum `type` (4.42, rule 3). Its `options` has the member `reduce` (4.26) and nine optional members.
//! An absent optional member has its default value, and the canonical writer leaves out a member that has its default value.
//! The base is `all_types.json`: its bar gauge becomes a gauge.

use hune_core::document::{
    GAUGE_MIN_SIZE, GaugeOptions, GaugeSizing, PanelOptions, PanelType, StatOrientation, TextEnum, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const PANEL: &str = "/sections/1/panels/3";
const OPTIONS: &str = "/sections/1/panels/3/options";

/// The document with a gauge in the place of the bar gauge: the options hold `reduce` only.
fn base() -> Value {
    let mut v: Value = serde_json::from_slice(ALL_TYPES).unwrap();
    let p = v.pointer_mut(PANEL).unwrap();
    p["type"] = json!("gauge");
    p["options"] = json!({"reduce": {"calc": "mean", "fields": "/^g/", "values": false}});
    v
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn with(member: &str, value: Value) -> Value {
    let mut v = base();
    v.pointer_mut(OPTIONS).unwrap().as_object_mut().unwrap().insert(member.to_owned(), value);
    v
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{e:#?}");
}

fn options(v: &Value) -> GaugeOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let p = &doc.document().sections[1].panels[3];
    assert_eq!(p.panel_type, PanelType::Gauge);
    match &p.options {
        PanelOptions::Gauge(o) => o.clone(),
        other => panic!("not a gauge: {other:?}"),
    }
}

/// The canonical options (4.38) as a JSON text with no blank, in the order of the writer. No value of these tests holds a blank.
fn canonical_options(v: &Value) -> String {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let text = String::from_utf8(doc.bytes().to_vec()).unwrap();
    let at = text.find("\"type\": \"gauge\"").unwrap();
    let start = at + text[at..].find("\"options\": ").unwrap() + "\"options\": ".len();
    let end = start + text[start..].find("\"links\"").unwrap();
    let compact: String = text[start..end].chars().filter(|c| !c.is_whitespace()).collect();
    compact.trim_end_matches(',').to_owned()
}

#[test]
fn the_type_text_is_gauge() {
    assert_eq!(PanelType::Gauge.as_str(), "gauge");
    assert_eq!(GaugeSizing::Manual.as_str(), "manual");
    assert_eq!(GAUGE_MIN_SIZE, 75);
}

#[test]
fn absent_members_have_the_grafana_defaults() {
    let o = options(&base());
    assert_eq!(o.orientation, StatOrientation::Auto);
    assert!(!o.show_threshold_labels);
    assert!(o.show_threshold_markers);
    assert_eq!((o.neutral, o.title_size, o.value_size), (None, None, None));
    assert_eq!((o.sizing, o.min_width, o.min_height), (GaugeSizing::Auto, 75, 75));
    assert_eq!(canonical_options(&base()), r#"{"reduce":{"calc":"mean","fields":"/^g/","values":false}}"#);
}

#[test]
fn each_member_takes_its_values_and_the_writer_keeps_the_order_of_4_33e() {
    let mut v = base();
    // The members in an order that is not the order of the table: the canonical form sorts them.
    *v.pointer_mut(OPTIONS).unwrap() = json!({
        "min_height": 600, "min_width": 0, "sizing": "manual", "value_size": 200, "title_size": 1, "neutral": -2.5,
        "show_threshold_markers": false, "show_threshold_labels": true, "orientation": "vertical",
        "reduce": {"calc": "last_not_null", "fields": null, "values": true}
    });
    let o = options(&v);
    assert_eq!(o.orientation, StatOrientation::Vertical);
    assert!(o.show_threshold_labels && !o.show_threshold_markers);
    assert_eq!((o.neutral, o.title_size, o.value_size), (Some(-2.5), Some(1), Some(200)));
    assert_eq!((o.sizing, o.min_width, o.min_height), (GaugeSizing::Manual, 0, 600));
    assert_eq!(
        canonical_options(&v),
        concat!(
            r#"{"reduce":{"calc":"last_not_null","fields":null,"values":true},"orientation":"vertical","show_threshold_labels":true,"#,
            r#""show_threshold_markers":false,"neutral":-2.5,"title_size":1,"value_size":200,"sizing":"manual","min_width":0,"min_height":600}"#
        )
    );
}

#[test]
fn a_member_with_its_default_value_leaves_the_canonical_form() {
    for (member, value) in [
        ("orientation", json!("auto")),
        ("show_threshold_labels", json!(false)),
        ("show_threshold_markers", json!(true)),
        ("sizing", json!("auto")),
        ("min_width", json!(75)),
        ("min_height", json!(75)),
    ] {
        let v = with(member, value.clone());
        assert_eq!(options(&v), options(&base()), "{member}");
        assert_eq!(canonical_options(&v), canonical_options(&base()), "{member} = {value}");
    }
    // A neutral value of 0 is a value, not the default.
    assert_eq!(options(&with("neutral", json!(0))).neutral, Some(0.0));
    assert!(canonical_options(&with("neutral", json!(0))).contains(r#""neutral":0"#));
}

#[test]
fn the_other_values_of_the_enums_are_valid() {
    for value in ["auto", "horizontal", "vertical"] {
        assert_eq!(options(&with("orientation", json!(value))).orientation.as_str(), value);
    }
    for value in ["auto", "manual"] {
        assert_eq!(options(&with("sizing", json!(value))).sizing.as_str(), value);
    }
}

#[test]
fn values_outside_the_table_are_refused_at_the_member() {
    let cases: [(&str, Value, ErrorCode); 16] = [
        ("orientation", json!("diagonal"), ErrorCode::BadFormat),
        ("orientation", json!(1), ErrorCode::WrongType),
        ("show_threshold_labels", json!("true"), ErrorCode::WrongType),
        ("show_threshold_markers", json!(null), ErrorCode::WrongType),
        ("neutral", json!("30"), ErrorCode::WrongType),
        ("neutral", json!(null), ErrorCode::WrongType),
        ("title_size", json!(0), ErrorCode::OutOfRange),
        ("title_size", json!(201), ErrorCode::OutOfRange),
        ("title_size", json!(12.5), ErrorCode::WrongType),
        ("value_size", json!(-1), ErrorCode::OutOfRange),
        ("sizing", json!("Manual"), ErrorCode::BadFormat),
        ("min_width", json!(601), ErrorCode::OutOfRange),
        ("min_width", json!(-1), ErrorCode::OutOfRange),
        ("min_height", json!(75.5), ErrorCode::WrongType),
        ("min_height", json!("75"), ErrorCode::WrongType),
        ("show_unfilled", json!(true), ErrorCode::UnknownMember),
    ];
    for (member, value, code) in cases {
        let e = errors_of(&with(member, value.clone()));
        assert_eq!(e.len(), 1, "{member} = {value}: {e:#?}");
        assert_eq!(
            (e[0].pointer.as_str(), e[0].code),
            (format!("{OPTIONS}/{member}").as_str(), code),
            "{member} = {value}: {e:#?}"
        );
    }
}

#[test]
fn reduce_is_necessary_and_the_options_of_a_bar_gauge_are_unknown() {
    let mut v = base();
    v.pointer_mut(OPTIONS).unwrap().as_object_mut().unwrap().remove("reduce");
    expect(&v, OPTIONS, ErrorCode::MissingMember);
    // The old options of the bar gauge are not members of a gauge.
    let mut v = base();
    *v.pointer_mut(OPTIONS).unwrap() =
        json!({"reduce": {"calc": "mean", "fields": null, "values": false}, "display_mode": "lcd"});
    expect(&v, &format!("{OPTIONS}/display_mode"), ErrorCode::UnknownMember);
}

#[test]
fn a_bar_gauge_does_not_take_the_members_of_a_gauge() {
    let mut v: Value = serde_json::from_slice(ALL_TYPES).unwrap();
    v.pointer_mut(OPTIONS).unwrap().as_object_mut().unwrap().insert("neutral".into(), json!(1));
    expect(&v, &format!("{OPTIONS}/neutral"), ErrorCode::UnknownMember);
}

#[test]
fn the_canonical_form_is_a_fixed_point() {
    let v = with("neutral", json!(12.75));
    let first = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let second = validate(first.bytes(), None).unwrap();
    assert_eq!(first.bytes(), second.bytes());
    assert_eq!(first.etag(), second.etag());
}
