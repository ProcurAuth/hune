// SPDX-License-Identifier: MIT OR Apache-2.0
//! The members that the Grafana-parity work adds to format 2 (the first set, in format 3 since the second set).
//!
//! Each member is optional. An absent member has its default value, and the canonical writer
//! leaves out a member that has its default value. The base is `all_types.json`, which has none of
//! the new members. `f_members.json` has each new member at least once.

use hune_core::document::{
    CellMode, CellType, FieldMatcher, LegendSortBy, LegendValue, NamesMode, PanelOptions, ReduceCalc, StatJustify,
    TextEnum, ThresholdsStyle, TooltipSort, validate,
};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::*;
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const F_MEMBERS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/f_members.json");

const TB: &str = "/sections/0/panels/0";
const TS: &str = "/sections/0/panels/1";
const ST: &str = "/sections/0/panels/2";
const BC: &str = "/sections/1/panels/0";
const PC: &str = "/sections/1/panels/1";
const XY: &str = "/sections/1/panels/2";
const BG: &str = "/sections/1/panels/3";
const ALL_PANELS: [&str; 7] = [TB, TS, ST, BC, PC, XY, BG];

fn base() -> Value {
    serde_json::from_slice(ALL_TYPES).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn at<'a>(v: &'a mut Value, pointer: &str) -> &'a mut Value {
    v.pointer_mut(pointer).unwrap_or_else(|| panic!("no value at {pointer}"))
}

/// Replaces the value at the pointer, or adds the member when it is absent.
fn set(v: &mut Value, pointer: &str, new: Value) {
    if let Some(old) = v.pointer_mut(pointer) {
        *old = new;
        return;
    }
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    at(v, parent).as_object_mut().unwrap().insert(key.to_owned(), new);
}

fn insert(v: &mut Value, pointer: &str, key: &str, new: Value) {
    at(v, pointer).as_object_mut().unwrap().insert(key.to_owned(), new);
}

fn remove(v: &mut Value, pointer: &str) {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    at(v, parent).as_object_mut().unwrap().remove(key);
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

fn canonical(v: &Value) -> Vec<u8> {
    validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}")).bytes().to_vec()
}

fn set_prop(panel: &str, extra: &str) -> String {
    format!("{panel}/fields/defaults{extra}")
}

fn override_matcher(v: &mut Value, panel: &str, matcher: Value) {
    let pointer = format!("{panel}/fields/overrides");
    set(v, &pointer, json!([{"match": matcher, "set": {"unit": "ms"}}]));
}

// -------------------------------------------------- fixture and canonical form

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(F_MEMBERS, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), F_MEMBERS);
    assert_eq!(doc.format_version(), 3);
}

#[test]
fn the_fixture_holds_each_new_member() {
    let text = std::str::from_utf8(F_MEMBERS).unwrap();
    for needle in [
        "\"justify\": \"center\"",
        "\"thresholds_style\": \"dashed_area\"",
        "\"thresholds_style\": \"line\"",
        "\"wrap\": true",
        "\"sort_by\": \"max\"",
        "\"sort_desc\": true",
        "\"sort\": \"desc\"",
        "\"sort\": \"asc\"",
        "\"hide_zeros\": true",
        "\"hide_from\"",
        "\"footer\"",
        "\"by\": \"names\"",
        "\"mode\": \"exclude\"",
        "\"mode\": \"include\"",
        "\"type\": \"gauge\"",
        "\"type\": \"json\"",
        "\"mode\": \"lcd\"",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    for unit in [
        "ns",
        "\u{b5}s",
        "m",
        "h",
        "d",
        "dtdurationms",
        "dtdurations",
        "bits",
        "bps",
        "Bps",
        "binBps",
        "reqps",
        "ops",
        "rps",
        "iops",
        "locale",
        "dateTimeAsIso",
        "dateTimeFromNow",
        "bool_yes_no",
        "currencyUSD",
        "currencyEUR",
    ] {
        let mut v = base();
        set(&mut v, &set_prop(TS, "/unit"), json!(unit));
        valid(&v);
    }
}

#[test]
fn the_typed_model_reads_the_new_members() {
    let doc = validate(F_MEMBERS, None).unwrap();
    let panels: Vec<_> = doc.document().panels().map(|(_, p)| p).collect();
    let PanelOptions::Stat(stat) = &panels[2].options else { panic!("stat") };
    assert_eq!(stat.justify, StatJustify::Center);
    assert_eq!(stat.reduce.calc, ReduceCalc::Diff);
    let PanelOptions::Timeseries(ts) = &panels[1].options else { panic!("timeseries") };
    assert_eq!(ts.thresholds_style, ThresholdsStyle::DashedArea);
    assert_eq!(ts.legend.sort_by, Some(LegendSortBy::Max));
    assert!(ts.legend.sort_desc);
    assert_eq!(ts.legend.values, [LegendValue::Max, LegendValue::Median, LegendValue::LastNotNull, LegendValue::Range]);
    assert_eq!((ts.tooltip.sort, ts.tooltip.hide_zeros), (TooltipSort::Desc, true));
    let PanelOptions::Table(table) = &panels[0].options else { panic!("table") };
    assert!(table.wrap);
    let footer = panels[0].fields.defaults.footer.as_ref().unwrap();
    assert_eq!(footer.reducers, [ReduceCalc::Total, ReduceCalc::Count, ReduceCalc::StdDev]);
    let overrides = &panels[0].fields.overrides;
    let FieldMatcher::Names { names, mode } = &overrides[1].matcher else { panic!("names") };
    assert_eq!((names.as_slice(), *mode), (["payload".to_owned(), "extra".to_owned()].as_slice(), NamesMode::Include));
    let cell = overrides[0].set.cell.unwrap();
    assert_eq!((cell.cell_type, cell.mode), (CellType::Gauge, Some(CellMode::Lcd)));
    let hide = panels[1].fields.overrides[0].set.hide_from.unwrap();
    assert_eq!((hide.legend, hide.tooltip, hide.viz), (false, true, true));
    let FieldMatcher::Names { mode, .. } = &panels[1].fields.overrides[0].matcher else { panic!("names") };
    assert_eq!(*mode, NamesMode::Exclude);
}

#[test]
fn an_explicit_default_gives_the_bytes_of_an_absent_member() {
    let plain = canonical(&base());
    assert_eq!(plain, ALL_TYPES);
    let mut v = base();
    set(&mut v, &format!("{ST}/options/justify"), json!("auto"));
    set(&mut v, &format!("{TS}/options/thresholds_style"), json!("off"));
    set(&mut v, &format!("{BC}/options/thresholds_style"), json!("off"));
    set(&mut v, &format!("{TB}/options/wrap"), json!(false));
    for panel in [TS, BC, PC, XY] {
        set(&mut v, &format!("{panel}/options/tooltip/sort"), json!("none"));
        set(&mut v, &format!("{panel}/options/tooltip/hide_zeros"), json!(false));
    }
    set(&mut v, &format!("{TS}/options/legend/sort_desc"), json!(false));
    assert_eq!(canonical(&v), plain);
}

#[test]
fn a_member_that_is_not_the_default_is_written_in_table_order() {
    let mut v = base();
    set(&mut v, &format!("{ST}/options/justify"), json!("center"));
    let text = String::from_utf8(canonical(&v)).unwrap();
    assert!(text.contains("\"orientation\": \"vertical\",\n            \"justify\": \"center\"\n"), "{text}");
    let mut v = base();
    set(&mut v, &format!("{TS}/options/legend/sort_desc"), json!(true));
    set(&mut v, &format!("{TS}/options/legend/sort_by"), json!("mean"));
    set(&mut v, &format!("{TS}/options/tooltip/hide_zeros"), json!(true));
    set(&mut v, &format!("{TS}/options/tooltip/sort"), json!("asc"));
    let text = String::from_utf8(canonical(&v)).unwrap();
    assert!(text.contains("\"sort_by\": \"mean\",\n              \"sort_desc\": true\n"), "{text}");
    assert!(
        text.contains("\"mode\": \"hidden\",\n              \"sort\": \"asc\",\n              \"hide_zeros\": true\n")
    );
}

#[test]
fn the_new_members_are_not_in_the_old_fixtures() {
    let text = std::str::from_utf8(ALL_TYPES).unwrap();
    for name in ["justify", "thresholds_style", "\"wrap\"", "sort_by", "sort_desc", "hide_zeros", "hide_from", "footer"]
    {
        assert!(!text.contains(name), "{name}");
    }
}

// ------------------------------------------------------- options of the panels

#[test]
fn stat_justify_values() {
    for value in ["auto", "center"] {
        let mut v = base();
        set(&mut v, &format!("{ST}/options/justify"), json!(value));
        valid(&v);
    }
    let mut v = base();
    set(&mut v, &format!("{ST}/options/justify"), json!("left"));
    expect(&v, &format!("{ST}/options/justify"), ErrorCode::BadFormat);
    let mut v = base();
    set(&mut v, &format!("{ST}/options/justify"), json!(true));
    expect(&v, &format!("{ST}/options/justify"), ErrorCode::WrongType);
    let mut v = base();
    set(&mut v, &format!("{ST}/options/justify"), Value::Null);
    expect(&v, &format!("{ST}/options/justify"), ErrorCode::WrongType);
}

#[test]
fn justify_is_a_member_of_stat_only() {
    for panel in [TB, TS, BC, PC, XY, BG] {
        let mut v = base();
        insert(&mut v, &format!("{panel}/options"), "justify", json!("center"));
        expect(&v, &format!("{panel}/options/justify"), ErrorCode::UnknownMember);
    }
}

#[test]
fn thresholds_style_values_in_timeseries_and_barchart() {
    for panel in [TS, BC] {
        for value in ["off", "line", "dashed", "area", "line_area", "dashed_area"] {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/thresholds_style"), json!(value));
            valid(&v);
        }
        for bad in ["series", "line+area", "Line"] {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/thresholds_style"), json!(bad));
            expect(&v, &format!("{panel}/options/thresholds_style"), ErrorCode::BadFormat);
        }
        let mut v = base();
        set(&mut v, &format!("{panel}/options/thresholds_style"), json!(1));
        expect(&v, &format!("{panel}/options/thresholds_style"), ErrorCode::WrongType);
    }
    for panel in [TB, ST, PC, XY, BG] {
        let mut v = base();
        insert(&mut v, &format!("{panel}/options"), "thresholds_style", json!("line"));
        expect(&v, &format!("{panel}/options/thresholds_style"), ErrorCode::UnknownMember);
    }
}

#[test]
fn table_wrap_is_a_boolean_of_the_table_only() {
    for value in [true, false] {
        let mut v = base();
        set(&mut v, &format!("{TB}/options/wrap"), json!(value));
        valid(&v);
    }
    let mut v = base();
    set(&mut v, &format!("{TB}/options/wrap"), json!("yes"));
    expect(&v, &format!("{TB}/options/wrap"), ErrorCode::WrongType);
    let mut v = base();
    insert(&mut v, &format!("{TS}/options"), "wrap", json!(true));
    expect(&v, &format!("{TS}/options/wrap"), ErrorCode::UnknownMember);
}

#[test]
fn footer_reducers_rules() {
    let footer = format!("{}/footer", set_prop(TB, ""));
    let mut v = base();
    set(&mut v, &footer.replace("/footer", ""), json!({"footer": {"reducers": ["total"]}}));
    valid(&v);
    for calc in ReduceCalc::NAMES {
        let mut v = base();
        set(&mut v, &footer.replace("/footer", ""), json!({"footer": {"reducers": [calc]}}));
        valid(&v);
    }
    let parent = footer.replace("/footer", "");
    let put = |reducers: Value| {
        let mut v = base();
        set(&mut v, &parent, json!({"footer": {"reducers": reducers}}));
        v
    };
    expect(&put(json!([])), &format!("{footer}/reducers"), ErrorCode::OutOfRange);
    let nine: Vec<&str> = ReduceCalc::NAMES.iter().copied().take(MAX_FOOTER_REDUCERS + 1).collect();
    expect(&put(json!(nine)), &format!("{footer}/reducers"), ErrorCode::OutOfRange);
    let eight: Vec<&str> = ReduceCalc::NAMES.iter().copied().take(MAX_FOOTER_REDUCERS).collect();
    valid(&put(json!(eight)));
    expect(&put(json!(["total", "total"])), &format!("{footer}/reducers/1"), ErrorCode::NotUnique);
    expect(&put(json!(["sum"])), &format!("{footer}/reducers/0"), ErrorCode::BadFormat);
    expect(&put(json!([1])), &format!("{footer}/reducers/0"), ErrorCode::WrongType);
    expect(&put(json!("total")), &format!("{footer}/reducers"), ErrorCode::WrongType);
    let mut v = base();
    set(&mut v, &parent, json!({"footer": {}}));
    expect(&v, &footer, ErrorCode::MissingMember);
    let mut v = base();
    set(&mut v, &parent, json!({"footer": {"reducers": ["total"], "show": true}}));
    expect(&v, &format!("{footer}/show"), ErrorCode::UnknownMember);
    let mut v = base();
    set(&mut v, &parent, json!({"footer": null}));
    expect(&v, &footer, ErrorCode::WrongType);
}

#[test]
fn footer_is_permitted_in_a_table_only_and_in_an_override() {
    for panel in [TS, ST, BC, PC, XY, BG] {
        let mut v = base();
        set(&mut v, &set_prop(panel, ""), json!({"footer": {"reducers": ["total"]}}));
        expect(&v, &format!("{}/footer", set_prop(panel, "")), ErrorCode::NotPermitted);
    }
    let mut v = base();
    set(
        &mut v,
        &format!("{TB}/fields/overrides"),
        json!([{"match": {"by": "name", "value": "a"}, "set": {"footer": {"reducers": ["mean", "max"]}}}]),
    );
    valid(&v);
}

// ------------------------------------------------------ the property hide_from

#[test]
fn hide_from_has_three_necessary_booleans() {
    for panel in ALL_PANELS {
        for bits in 0..8u8 {
            let mut v = base();
            let value = json!({"legend": bits & 1 != 0, "tooltip": bits & 2 != 0, "viz": bits & 4 != 0});
            set(&mut v, &set_prop(panel, ""), json!({"hide_from": value}));
            valid(&v);
        }
    }
    let at_hide = format!("{}/hide_from", set_prop(TS, ""));
    for missing in ["legend", "tooltip", "viz"] {
        let mut value = json!({"legend": false, "tooltip": true, "viz": true});
        value.as_object_mut().unwrap().remove(missing);
        let mut v = base();
        set(&mut v, &set_prop(TS, ""), json!({"hide_from": value}));
        expect(&v, &at_hide, ErrorCode::MissingMember);
    }
    let mut v = base();
    set(&mut v, &set_prop(TS, ""), json!({"hide_from": {"legend": false, "tooltip": 1, "viz": true}}));
    expect(&v, &format!("{at_hide}/tooltip"), ErrorCode::WrongType);
    let mut v = base();
    set(&mut v, &set_prop(TS, ""), json!({"hide_from": {"legend": false, "tooltip": true, "viz": true, "x": 1}}));
    expect(&v, &format!("{at_hide}/x"), ErrorCode::UnknownMember);
    let mut v = base();
    set(&mut v, &set_prop(TS, ""), json!({"hide_from": null}));
    expect(&v, &at_hide, ErrorCode::WrongType);
    let mut v = base();
    set(&mut v, &set_prop(TS, ""), json!({"hide_from": true}));
    expect(&v, &at_hide, ErrorCode::WrongType);
}

#[test]
fn the_matcher_names_has_names_and_mode() {
    let at_match = format!("{TS}/fields/overrides/0/match");
    for mode in ["include", "exclude"] {
        let mut v = base();
        override_matcher(&mut v, TS, json!({"by": "names", "names": ["a", "b"], "mode": mode}));
        valid(&v);
    }
    let put = |matcher: Value| {
        let mut v = base();
        override_matcher(&mut v, TS, matcher);
        v
    };
    expect(&put(json!({"by": "names", "names": ["a"]})), &at_match, ErrorCode::MissingMember);
    expect(&put(json!({"by": "names", "mode": "include"})), &at_match, ErrorCode::MissingMember);
    expect(
        &put(json!({"by": "names", "names": ["a"], "mode": "all"})),
        &format!("{at_match}/mode"),
        ErrorCode::BadFormat,
    );
    expect(&put(json!({"by": "names", "names": ["a"], "mode": 1})), &format!("{at_match}/mode"), ErrorCode::WrongType);
    expect(
        &put(json!({"by": "names", "names": ["a"], "mode": "include", "value": "a"})),
        &format!("{at_match}/value"),
        ErrorCode::UnknownMember,
    );
    expect(
        &put(json!({"by": "name", "value": "a", "names": ["a"]})),
        &format!("{at_match}/names"),
        ErrorCode::UnknownMember,
    );
    expect(
        &put(json!({"by": "regex", "value": "/a/", "mode": "include"})),
        &format!("{at_match}/mode"),
        ErrorCode::UnknownMember,
    );
    expect(
        &put(json!({"by": "other", "names": ["a"], "mode": "include"})),
        &format!("{at_match}/by"),
        ErrorCode::BadFormat,
    );
}

#[test]
fn the_names_of_a_matcher_have_their_limits() {
    let names_at = format!("{TS}/fields/overrides/0/match/names");
    let put = |names: Value| {
        let mut v = base();
        override_matcher(&mut v, TS, json!({"by": "names", "names": names, "mode": "include"}));
        v
    };
    expect(&put(json!([])), &names_at, ErrorCode::OutOfRange);
    let many = |n: usize| Value::Array((0..n).map(|i| json!(format!("n{i}"))).collect());
    valid(&put(many(MAX_MATCHER_NAMES)));
    expect(&put(many(MAX_MATCHER_NAMES + 1)), &names_at, ErrorCode::TooLarge);
    expect(&put(json!([""])), &format!("{names_at}/0"), ErrorCode::OutOfRange);
    expect(&put(json!(["a", 1])), &format!("{names_at}/1"), ErrorCode::WrongType);
    expect(&put(json!("a")), &names_at, ErrorCode::WrongType);
    let long = "x".repeat(MAX_VALUE_BYTES);
    valid(&put(json!([long])));
    let too_long = "x".repeat(MAX_VALUE_BYTES + 1);
    expect(&put(json!([too_long])), &format!("{names_at}/0"), ErrorCode::OutOfRange);
    expect(&put(json!(["a\nb"])), &format!("{names_at}/0"), ErrorCode::BadFormat);
    // A name can repeat: the matcher is a set.
    valid(&put(json!(["a", "a"])));
}

// -------------------------------------------------------------- the cell types

#[test]
fn cell_types_gauge_and_json() {
    let cell_at = format!("{}/cell", set_prop(TB, ""));
    let put = |cell: Value| {
        let mut v = base();
        set(&mut v, &set_prop(TB, ""), json!({"cell": cell}));
        v
    };
    for mode in ["basic", "gradient", "lcd"] {
        valid(&put(json!({"type": "gauge", "mode": mode})));
    }
    valid(&put(json!({"type": "json", "mode": null})));
    for mode in ["gradient", "basic"] {
        valid(&put(json!({"type": "color_background", "mode": mode})));
    }
    expect(&put(json!({"type": "gauge", "mode": null})), &format!("{cell_at}/mode"), ErrorCode::NotPermitted);
    expect(&put(json!({"type": "gauge", "mode": "other"})), &format!("{cell_at}/mode"), ErrorCode::BadFormat);
    expect(&put(json!({"type": "json", "mode": "basic"})), &format!("{cell_at}/mode"), ErrorCode::NotPermitted);
    expect(&put(json!({"type": "auto", "mode": "lcd"})), &format!("{cell_at}/mode"), ErrorCode::NotPermitted);
    expect(&put(json!({"type": "color_text", "mode": "basic"})), &format!("{cell_at}/mode"), ErrorCode::NotPermitted);
    expect(&put(json!({"type": "color_background", "mode": "lcd"})), &format!("{cell_at}/mode"), ErrorCode::BadFormat);
    expect(
        &put(json!({"type": "color_background", "mode": null})),
        &format!("{cell_at}/mode"),
        ErrorCode::NotPermitted,
    );
    expect(&put(json!({"type": "sparkline", "mode": null})), &format!("{cell_at}/type"), ErrorCode::BadFormat);
    expect(&put(json!({"type": "gauge"})), &cell_at, ErrorCode::MissingMember);
}

#[test]
fn cell_types_stay_in_the_table() {
    for panel in [TS, ST, BC, PC, XY, BG] {
        let mut v = base();
        set(&mut v, &set_prop(panel, ""), json!({"cell": {"type": "gauge", "mode": "basic"}}));
        expect(&v, &format!("{}/cell", set_prop(panel, "")), ErrorCode::NotPermitted);
    }
}

// -------------------------------------------- legend, tooltip and calculations

#[test]
fn legend_sort_by_values() {
    let calcs = [
        "total",
        "min",
        "max",
        "mean",
        "last",
        "last_not_null",
        "first",
        "first_not_null",
        "count",
        "median",
        "std_dev",
        "variance",
        "range",
        "diff",
        "diff_percent",
        "delta",
        "distinct_count",
        "change_count",
    ];
    for panel in [TS, BC, XY] {
        for value in std::iter::once("name").chain(calcs) {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/legend/sort_by"), json!(value));
            valid(&v);
        }
        for bad in ["value", "percent"] {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/legend/sort_by"), json!(bad));
            expect(&v, &format!("{panel}/options/legend/sort_by"), ErrorCode::BadFormat);
        }
    }
    for value in ["name", "value", "percent"] {
        let mut v = base();
        set(&mut v, &format!("{PC}/options/legend/sort_by"), json!(value));
        valid(&v);
    }
    for bad in ["max", "total", "Name"] {
        let mut v = base();
        set(&mut v, &format!("{PC}/options/legend/sort_by"), json!(bad));
        expect(&v, &format!("{PC}/options/legend/sort_by"), ErrorCode::BadFormat);
    }
    let mut v = base();
    set(&mut v, &format!("{TS}/options/legend/sort_by"), json!(1));
    expect(&v, &format!("{TS}/options/legend/sort_by"), ErrorCode::WrongType);
    let mut v = base();
    set(&mut v, &format!("{TS}/options/legend/sort_by"), Value::Null);
    expect(&v, &format!("{TS}/options/legend/sort_by"), ErrorCode::WrongType);
    // A sort key that is not in `values` is permitted: the viewer shows no sort then.
    let mut v = base();
    set(&mut v, &format!("{BC}/options/legend/sort_by"), json!("max"));
    valid(&v);
}

#[test]
fn legend_sort_desc_and_tooltip_members() {
    for panel in ALL_PANELS.iter().filter(|p| **p != TB && **p != ST && **p != BG) {
        for value in [true, false] {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/legend/sort_desc"), json!(value));
            valid(&v);
        }
        let mut v = base();
        set(&mut v, &format!("{panel}/options/legend/sort_desc"), json!("true"));
        expect(&v, &format!("{panel}/options/legend/sort_desc"), ErrorCode::WrongType);
        for sort in ["none", "asc", "desc"] {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/tooltip/sort"), json!(sort));
            valid(&v);
        }
        let mut v = base();
        set(&mut v, &format!("{panel}/options/tooltip/sort"), json!("up"));
        expect(&v, &format!("{panel}/options/tooltip/sort"), ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &format!("{panel}/options/tooltip/hide_zeros"), json!(1));
        expect(&v, &format!("{panel}/options/tooltip/hide_zeros"), ErrorCode::WrongType);
        let mut v = base();
        set(&mut v, &format!("{panel}/options/tooltip/hide_zeros"), json!(true));
        valid(&v);
    }
    let mut v = base();
    insert(&mut v, &format!("{TS}/options/legend"), "sort", json!("max"));
    expect(&v, &format!("{TS}/options/legend/sort"), ErrorCode::UnknownMember);
    let mut v = base();
    insert(&mut v, &format!("{TS}/options/tooltip"), "sort_by", json!("max"));
    expect(&v, &format!("{TS}/options/tooltip/sort_by"), ErrorCode::UnknownMember);
}

#[test]
fn calc_and_legend_values_hold_the_new_names() {
    let new =
        ["median", "std_dev", "variance", "range", "diff", "diff_percent", "delta", "distinct_count", "change_count"];
    for calc in new {
        for pointer in [
            format!("{ST}/options/reduce/calc"),
            format!("{BG}/options/reduce/calc"),
            format!("{PC}/options/reduce/calc"),
        ] {
            let mut v = base();
            set(&mut v, &pointer, json!(calc));
            valid(&v);
        }
    }
    let names: Vec<&str> = ReduceCalc::NAMES.to_vec();
    assert_eq!(names.len(), 18);
    for name in new {
        assert!(names.contains(&name), "{name}");
    }
    // `legend.values` of a non-pie panel: each calc name. A `piechart` keeps `value` and `percent`.
    for panel in [TS, BC, XY] {
        for calc in &names {
            let mut v = base();
            set(&mut v, &format!("{panel}/options/legend/values"), json!([calc]));
            valid(&v);
        }
        let mut v = base();
        set(&mut v, &format!("{panel}/options/legend/values"), json!(["value"]));
        expect(&v, &format!("{panel}/options/legend/values/0"), ErrorCode::BadFormat);
        let mut v = base();
        set(&mut v, &format!("{panel}/options/legend/values"), json!(["median", "range", "diff", "delta", "count"]));
        valid(&v);
        let mut v = base();
        set(
            &mut v,
            &format!("{panel}/options/legend/values"),
            json!(["median", "range", "diff", "delta", "count", "min"]),
        );
        expect(&v, &format!("{panel}/options/legend/values"), ErrorCode::OutOfRange);
        let mut v = base();
        set(&mut v, &format!("{panel}/options/legend/values"), json!(["median", "median"]));
        expect(&v, &format!("{panel}/options/legend/values/1"), ErrorCode::NotUnique);
    }
    let mut v = base();
    set(&mut v, &format!("{PC}/options/legend/values"), json!(["median"]));
    expect(&v, &format!("{PC}/options/legend/values/0"), ErrorCode::BadFormat);
}

#[test]
fn the_text_enums_round_trip() {
    for name in LegendSortBy::NAMES {
        assert_eq!(LegendSortBy::parse(name).unwrap().as_str(), *name);
    }
    for name in LegendValue::NAMES {
        assert_eq!(LegendValue::parse(name).unwrap().as_str(), *name);
    }
    assert!(LegendSortBy::parse("sum").is_none());
}

#[test]
fn a_property_set_keeps_the_table_order_with_the_new_members() {
    let mut v = base();
    set(
        &mut v,
        &set_prop(TB, ""),
        json!({
            "hide_from": {"legend": false, "tooltip": false, "viz": true},
            "footer": {"reducers": ["total"]},
            "hidden": true,
            "unit": "ns"
        }),
    );
    let text = String::from_utf8(canonical(&v)).unwrap();
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("\"unit\": \"ns\"") < at("\"hidden\": true"));
    assert!(at("\"hidden\": true") < at("\"footer\""));
    assert!(at("\"footer\"") < at("\"hide_from\""));
}

#[test]
fn the_units_of_wave_h_are_valid_in_a_document() {
    for unit in [
        "string",
        "bool",
        "bool_on_off",
        "kbytes",
        "mbytes",
        "gbytes",
        "tbytes",
        "deckbytes",
        "decmbytes",
        "decgbytes",
        "dectbytes",
        "decbits",
        "KBs",
        "MBs",
        "GBs",
        "Kbits",
        "Mbits",
        "Gbits",
        "rowsps",
        "rowspm",
        "recps",
        "cps",
        "wps",
        "opm",
        "reqpm",
        "rpm",
        "dthms",
        "dtdhms",
        "hertz",
        "sishort",
    ] {
        let mut v = base();
        set(&mut v, &set_prop(TS, "/unit"), json!(unit));
        valid(&v);
        let text = String::from_utf8(canonical(&v)).unwrap();
        assert!(text.contains(&format!("\"unit\": \"{unit}\"")), "{unit}");
    }
}

#[test]
fn units_outside_the_list_are_still_refused() {
    for unit in
        ["\u{3bc}s", "BPS", "Percent", "kbits", "µs ", "currencyGBP", "dtdurationmss", "us", "pixel", "bool:on/off"]
    {
        let mut v = base();
        set(&mut v, &set_prop(TS, "/unit"), json!(unit));
        expect(&v, &set_prop(TS, "/unit"), ErrorCode::BadFormat);
    }
    let mut v = base();
    remove(&mut v, &set_prop(TS, "/unit"));
    valid(&v);
}
