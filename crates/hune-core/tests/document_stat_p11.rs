//! The members of wave H, item P11 (decision ID-K326): the options `percent_change`, `percent_change_color`, `title_size` and
//! `value_size` of a `stat` (4.29).
//!
//! Each member is optional. An absent member has its default value, and the canonical writer leaves out a member that has its
//! default value. The base is `all_types.json`, which has none of the new members.

use hune_core::document::{PanelOptions, PercentChangeColor, StatOptions, TextEnum, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

const ST: &str = "/sections/0/panels/2";
const BG: &str = "/sections/1/panels/3";
const NEW: [&str; 4] = ["percent_change", "percent_change_color", "title_size", "value_size"];

fn base() -> Value {
    serde_json::from_slice(ALL_TYPES).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn insert(v: &mut Value, pointer: &str, key: &str, new: Value) {
    v.pointer_mut(pointer).unwrap().as_object_mut().unwrap().insert(key.to_owned(), new);
}

fn canonical_text(v: &Value) -> String {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    String::from_utf8(doc.bytes().to_vec()).unwrap()
}

fn canonical(v: &Value) -> Value {
    serde_json::from_str(&canonical_text(v)).unwrap()
}

fn stat_options(v: &Value) -> StatOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    match &doc.document().sections[0].panels[2].options {
        PanelOptions::Stat(o) => o.clone(),
        other => panic!("not a stat: {other:?}"),
    }
}

#[test]
fn absent_members_have_the_grafana_defaults() {
    let o = stat_options(&base());
    assert!(!o.percent_change);
    assert_eq!(o.percent_change_color, PercentChangeColor::Standard);
    assert_eq!((o.title_size, o.value_size), (None, None));
    let c = canonical(&base());
    for key in NEW {
        assert_eq!(c.pointer(&format!("{ST}/options/{key}")), None, "{key}");
    }
}

#[test]
fn each_member_takes_its_values_and_keeps_the_order_of_4_29() {
    for (color, text) in [
        (PercentChangeColor::Standard, "standard"),
        (PercentChangeColor::Inverted, "inverted"),
        (PercentChangeColor::SameAsValue, "same_as_value"),
    ] {
        assert_eq!(color.as_str(), text);
    }
    for (title, value) in [(1, 200), (200, 1), (20, 40)] {
        let mut v = base();
        let options = format!("{ST}/options");
        insert(&mut v, &options, "value_size", json!(value));
        insert(&mut v, &options, "percent_change_color", json!("inverted"));
        insert(&mut v, &options, "title_size", json!(title));
        insert(&mut v, &options, "percent_change", json!(true));
        let o = stat_options(&v);
        assert!(o.percent_change);
        assert_eq!(o.percent_change_color, PercentChangeColor::Inverted);
        assert_eq!((o.title_size, o.value_size), (Some(title), Some(value)));
        // The canonical form keeps the order of the table of 4.29, after `orientation`.
        let text = canonical_text(&v);
        let mut from = text.find("\"orientation\"").unwrap();
        for key in NEW {
            let at = text[from..]
                .find(&format!("\"{key}\""))
                .unwrap_or_else(|| panic!("{key} after the one before it: {text}"));
            from += at + key.len();
        }
    }
}

#[test]
fn the_canonical_form_leaves_out_the_defaults() {
    let mut v = base();
    insert(&mut v, &format!("{ST}/options"), "percent_change", json!(false));
    insert(&mut v, &format!("{ST}/options"), "percent_change_color", json!("standard"));
    assert_eq!(canonical_text(&v), canonical_text(&base()));
    let mut w = base();
    insert(&mut w, &format!("{ST}/options"), "percent_change_color", json!("same_as_value"));
    let c = canonical(&w);
    assert_eq!(c.pointer(&format!("{ST}/options/percent_change_color")), Some(&json!("same_as_value")));
    assert_eq!(c.pointer(&format!("{ST}/options/percent_change")), None);
}

#[test]
fn values_out_of_range_or_of_another_type_are_refused() {
    for (key, bad, code) in [
        ("percent_change", json!(null), ErrorCode::WrongType),
        ("percent_change", json!("true"), ErrorCode::WrongType),
        ("percent_change", json!(1), ErrorCode::WrongType),
        ("percent_change_color", json!("Standard"), ErrorCode::BadFormat),
        ("percent_change_color", json!("background"), ErrorCode::BadFormat),
        ("percent_change_color", json!(null), ErrorCode::WrongType),
        ("title_size", json!(0), ErrorCode::OutOfRange),
        ("title_size", json!(201), ErrorCode::OutOfRange),
        ("title_size", json!(-5), ErrorCode::OutOfRange),
        ("title_size", json!(12.5), ErrorCode::WrongType),
        ("title_size", json!(null), ErrorCode::WrongType),
        ("value_size", json!(1_000_000_000_000_i64), ErrorCode::OutOfRange),
        ("value_size", json!("40"), ErrorCode::WrongType),
        ("value_size", json!(0), ErrorCode::OutOfRange),
    ] {
        let mut v = base();
        insert(&mut v, &format!("{ST}/options"), key, bad.clone());
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{key} = {bad}: {e:#?}");
        assert_eq!(e[0].pointer, format!("{ST}/options/{key}"), "{key} = {bad}");
        assert_eq!(e[0].code, code, "{key} = {bad}: {e:#?}");
    }
}

#[test]
fn the_members_are_not_options_of_another_type() {
    for key in NEW {
        let mut v = base();
        let value = if key == "percent_change" {
            json!(true)
        } else if key == "percent_change_color" {
            json!("inverted")
        } else {
            json!(20)
        };
        insert(&mut v, &format!("{BG}/options"), key, value);
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{key}: {e:#?}");
        assert_eq!(
            (e[0].pointer.clone(), e[0].code),
            (format!("{BG}/options/{key}"), ErrorCode::UnknownMember),
            "{key}"
        );
    }
}
