// SPDX-License-Identifier: MIT OR Apache-2.0
//! Items P12: the importer maps the classic gauge of Grafana 12.4.2 (15.6, 4.33e).
//! The options of the gauge map to the optional members of 4.33e, `custom.neutral` of the defaults becomes the option `neutral`,
//! the sizes are read with `sizing` = `manual` only, and a member that no row names is an item.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn gauge(options: Value, defaults: Value) -> Value {
    let mut p = panel(1, "gauge", 0);
    p["options"] = options;
    p["fieldConfig"] = json!({"defaults": defaults, "overrides": []});
    p
}

fn one(p: Value) -> DashboardImport {
    run_plain(&dash(vec![p], vec![]))
}

fn options_of(r: &DashboardImport) -> Value {
    first_panel(&doc(r))["options"].clone()
}

/// The options that Grafana 12.4.2 writes for a new classic gauge.
fn grafana_defaults() -> Value {
    json!({
        "reduceOptions": {"values": false, "calcs": ["lastNotNull"], "fields": ""},
        "orientation": "auto", "showThresholdLabels": false, "showThresholdMarkers": true,
        "sizing": "auto", "minVizWidth": 75, "minVizHeight": 75
    })
}

#[test]
fn a_new_grafana_gauge_imports_with_no_item() {
    let r = one(gauge(
        grafana_defaults(),
        json!({"thresholds": {"mode": "absolute", "steps": [{"color": "green", "value": null}, {"color": "red", "value": 80}]}, "color": {"mode": "thresholds"}}),
    ));
    no_items(&r);
    let p = first_panel(&doc(&r)).clone();
    assert_eq!(p["type"], json!("gauge"));
    // Each member has its default: the canonical form keeps `reduce` only (4.38, rule 6).
    assert_eq!(p["options"], json!({"reduce": {"calc": "last_not_null", "fields": null, "values": false}}));
}

#[test]
fn each_option_maps_to_its_member() {
    let mut o = grafana_defaults();
    o["orientation"] = json!("vertical");
    o["showThresholdLabels"] = json!(true);
    o["showThresholdMarkers"] = json!(false);
    o["sizing"] = json!("manual");
    o["minVizWidth"] = json!(200);
    o["minVizHeight"] = json!(0);
    o["text"] = json!({"titleSize": 30, "valueSize": 50.0});
    o["reduceOptions"] = json!({"values": true, "calcs": ["max"], "fields": "/^cpu/"});
    let r = one(gauge(o, json!({"custom": {"neutral": 30.5}})));
    no_items(&r);
    assert_eq!(
        options_of(&r),
        json!({
            "reduce": {"calc": "max", "fields": "/^cpu/", "values": true}, "orientation": "vertical", "show_threshold_labels": true,
            "show_threshold_markers": false, "neutral": 30.5, "title_size": 30, "value_size": 50, "sizing": "manual",
            "min_width": 200, "min_height": 0
        })
    );
}

#[test]
fn the_sizes_with_sizing_auto_are_metadata() {
    let mut o = grafana_defaults();
    o["minVizWidth"] = json!(300);
    o["minVizHeight"] = json!(9999);
    let r = one(gauge(o, json!({})));
    no_items(&r);
    let options = options_of(&r);
    assert!(
        options.get("min_width").is_none() && options.get("min_height").is_none() && options.get("sizing").is_none(),
        "{options}"
    );
    // The same with no member `sizing`: Grafana reads the sizes only with `manual`.
    let r = one(gauge(json!({"minVizWidth": 300}), json!({})));
    no_items(&r);
}

#[test]
fn values_outside_the_ranges_are_unsupported_and_leave_the_document() {
    // The options, the member of the item, the words of the reason and the member of Hune that stays out.
    let cases: [(Value, &str, &str, &str); 6] = [
        (json!({"text": {"titleSize": 0}}), "options.text.titleSize", "integer from 1 to 200", "title_size"),
        (json!({"text": {"valueSize": 12.5}}), "options.text.valueSize", "integer from 1 to 200", "value_size"),
        (json!({"text": {"valueSize": 201}}), "options.text.valueSize", "integer from 1 to 200", "value_size"),
        (json!({"sizing": "manual", "minVizWidth": 601}), "options.minVizWidth", "integer from 0 to 600", "min_width"),
        (
            json!({"sizing": "manual", "minVizHeight": "75"}),
            "options.minVizHeight",
            "integer from 0 to 600",
            "min_height",
        ),
        (json!({"sizing": "fit"}), "options.sizing", "no value table", "sizing"),
    ];
    for (options, member, reason, key) in cases {
        let r = one(gauge(options.clone(), json!({})));
        let i = one_item(&r, member);
        assert_eq!(i.class, Class::Unsupported, "{options}: {i:#?}");
        assert!(i.reason.contains(reason), "{options}: {}", i.reason);
        let o = options_of(&run(&dash(vec![gauge(options.clone(), json!({}))], vec![])));
        assert!(o.get(key).is_none(), "{options}: {key} stays out: {o}");
    }
}

#[test]
fn neutral_is_read_from_the_defaults_only() {
    let r = one(gauge(json!({}), json!({"custom": {"neutral": "x"}})));
    let i = one_item(&r, "fieldConfig.defaults.custom.neutral");
    assert_eq!((i.class, i.fallback.clone()), (Class::Unsupported, Some(Fallback::MemberRemoved)), "{i:#?}");
    // In an override, `neutral` is a panel option of Hune: it has no meaning for one field.
    let mut p = gauge(json!({}), json!({}));
    p["fieldConfig"]["overrides"] =
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "custom.neutral", "value": 10}]}]);
    let r = one(p);
    let i = one_item(&r, "fieldConfig.overrides.0.properties.0");
    assert_eq!(i.class, Class::Unsupported, "{i:#?}");
}

#[test]
fn members_of_the_new_gauge_and_unknown_members_are_items() {
    let mut o = grafana_defaults();
    o["shape"] = json!("circle");
    o["text"] = json!({"titleSize": 20, "frobnicate": 3});
    let r = one(gauge(o.clone(), json!({})));
    assert_eq!(one_item(&r, "options.shape").class, Class::Unsupported);
    assert_eq!(one_item(&r, "options.text.frobnicate").class, Class::Unsupported);
    // The member that the importer knows stays.
    assert_eq!(options_of(&run(&dash(vec![gauge(o, json!({}))], vec![])))["title_size"], json!(20));
}

#[test]
fn the_dashboard_is_importable_and_the_type_counts_as_mapped() {
    let r = one(gauge(grafana_defaults(), json!({"min": 0, "max": 100, "unit": "percent", "decimals": 1})));
    no_items(&r);
    assert!(r.document_bytes.is_some(), "the dashboard is importable");
    let p = first_panel(&doc(&r)).clone();
    assert_eq!(p["fields"]["defaults"], json!({"unit": "percent", "decimals": 1, "min": 0, "max": 100}));
}
