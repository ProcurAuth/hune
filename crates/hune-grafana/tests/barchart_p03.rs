// SPDX-License-Identifier: MIT OR Apache-2.0
//! Items P03: the importer maps the fill, the line, the orientation `auto` and the grid
//! lines of a Grafana bar chart (4.18, 4.30, 15.6). The rules for the stacks and the axes stay.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn one(p: Value) -> DashboardImport {
    run_plain(&dash(vec![p], vec![]))
}

fn bar(options: Value, custom: Value) -> Value {
    let mut p = panel(1, "barchart", 0);
    p["options"] = options;
    p["fieldConfig"] = json!({"defaults": {"custom": custom}, "overrides": []});
    p
}

fn options_of(r: &DashboardImport) -> Value {
    first_panel(&doc(r))["options"].clone()
}

#[test]
fn fill_opacity_and_line_width_map_to_the_options() {
    for (fill, line) in [(61, 1), (0, 0), (100, 10), (80, 4)] {
        let r = one(bar(json!({}), json!({"fillOpacity": fill, "lineWidth": line})));
        no_items(&r);
        let o = options_of(&r);
        // The canonical form leaves out a member with its default (4.38, rule 6): 80 and 1.
        let want = |v: i64, default: i64| if v == default { Value::Null } else { json!(v) };
        assert_eq!(o.get("fill_opacity").cloned().unwrap_or(Value::Null), want(fill, 80), "{fill}");
        assert_eq!(o.get("line_width").cloned().unwrap_or(Value::Null), want(line, 1), "{line}");
    }
    // An absent member and `null` are the Grafana default: no member and no item.
    for custom in [json!({}), json!({"fillOpacity": null, "lineWidth": null})] {
        let r = one(bar(json!({}), custom));
        no_items(&r);
        let o = options_of(&r);
        assert!(o.get("fill_opacity").is_none() && o.get("line_width").is_none(), "{o}");
    }
}

#[test]
fn values_out_of_range_are_unsupported_with_the_default() {
    for (key, value) in
        [("fillOpacity", json!(101)), ("fillOpacity", json!(-5)), ("lineWidth", json!(11)), ("lineWidth", json!("2"))]
    {
        let p = bar(json!({}), json!({key: value}));
        let r = one(p.clone());
        let member = format!("fieldConfig.defaults.custom.{key}");
        let i = one_item(&r, &member);
        assert_eq!(i.class, Class::Unsupported, "{key} {value}: {i:#?}");
        assert!(i.reason.contains("integer from 0 to"), "{}", i.reason);
        let o = options_of(&run(&dash(vec![p], vec![])));
        assert!(o.get("fill_opacity").is_none() && o.get("line_width").is_none(), "the default stays: {o}");
    }
}

#[test]
fn the_two_members_in_an_override_are_unsupported() {
    // Format 3 has one fill and one line for all the bars (4.30).
    let mut p = bar(json!({}), json!({}));
    p["fieldConfig"]["overrides"] = json!([{"matcher": {"id": "byName", "options": "a"},
        "properties": [{"id": "custom.fillOpacity", "value": 30}]}]);
    let r = one(p);
    let i = one_item(&r, "fieldConfig.overrides.0.properties.0.value");
    assert_eq!((i.class, i.fallback.clone()), (Class::Unsupported, Some(Fallback::PropertyRemoved)), "{i:#?}");
}

#[test]
fn orientation_auto_maps_and_an_absent_orientation_is_auto() {
    for (given, want) in [(json!("auto"), "auto"), (json!("vertical"), "vertical"), (json!("horizontal"), "horizontal")]
    {
        let r = one(bar(json!({"orientation": given}), json!({})));
        no_items(&r);
        assert_eq!(options_of(&r)["orientation"], json!(want));
    }
    // The default of the Grafana bar chart is `auto` (Grafana 12.4.2, the options of the panel).
    let r = one(bar(json!({}), json!({})));
    no_items(&r);
    assert_eq!(options_of(&r)["orientation"], json!("auto"));
    // An unknown value is unsupported, with the Grafana default.
    let r = run(&dash(vec![bar(json!({"orientation": "diagonal"}), json!({}))], vec![]));
    assert_eq!(one_item(&r, "options.orientation").class, Class::Unsupported);
    assert_eq!(options_of(&r)["orientation"], json!("auto"));
}

#[test]
fn grid_lines_map_to_axis_grid() {
    for (given, want) in [(json!(false), json!(false)), (json!(true), json!(true))] {
        let r = one(bar(json!({}), json!({"axisGridShow": given})));
        no_items(&r);
        let out = doc(&r);
        assert_eq!(first_panel(&out)["fields"]["defaults"]["axis_grid"], want);
    }
    // `null` is Auto: metadata, no member.
    let r = one(bar(json!({}), json!({"axisGridShow": null})));
    no_items(&r);
    let out = doc(&r);
    assert!(first_panel(&out)["fields"]["defaults"].get("axis_grid").is_none());
    // In an override the property maps too. A value that is not a boolean is unsupported.
    let mut p = bar(json!({}), json!({}));
    p["fieldConfig"]["overrides"] = json!([{"matcher": {"id": "byName", "options": "a"},
        "properties": [{"id": "custom.axisGridShow", "value": false}]}]);
    let r = one(p);
    no_items(&r);
    let out = doc(&r);
    assert_eq!(first_panel(&out)["fields"]["overrides"][0]["set"], json!({"axis_grid": false}));
    let r = one(bar(json!({}), json!({"axisGridShow": "off"})));
    let i = one_item(&r, "fieldConfig.defaults.custom.axisGridShow");
    assert_eq!((i.class, i.fallback.clone()), (Class::Unsupported, Some(Fallback::PropertyRemoved)));
}

#[test]
fn grid_lines_off_stays_an_item_in_the_other_types_with_an_axis() {
    // P04: only the bar chart has the property of 4.18.
    for t in ["timeseries", "histogram", "xychart"] {
        let mut p = panel(1, t, 0);
        p["fieldConfig"] = json!({"defaults": {"custom": {"axisGridShow": false}}, "overrides": []});
        let r = one(p);
        assert_eq!(one_item(&r, "fieldConfig.defaults.custom.axisGridShow").class, Class::Unsupported, "{t}");
    }
}

#[test]
fn the_bar_charts_of_the_plugin_example_import_with_no_item_for_the_four_members() {
    // The custom members and the options of the two bar charts of the dashboard "Data Analysis" of the ClickHouse
    // plugin 4.5.1 (measure M1 of the later options). `gradientMode` `hue` maps since the later options, item P10.
    let custom = json!({"axisGridShow": false, "axisLabel": "", "axisPlacement": "auto", "axisSoftMin": 0, "fillOpacity": 61,
        "gradientMode": "hue", "hideFrom": {"legend": false, "tooltip": false, "viz": false}, "lineWidth": 1,
        "scaleDistribution": {"type": "linear"}});
    let options = json!({"barRadius": 0, "barWidth": 0.97, "groupWidth": 0.7, "legend": {"calcs": [], "displayMode": "list",
        "placement": "bottom"}, "orientation": "auto", "showValue": "auto", "stacking": "none", "tooltip": {"mode": "single",
        "sort": "none"}, "xTickLabelRotation": 0, "xTickLabelSpacing": 0});
    let p = bar(options, custom);
    let first = one(p.clone());
    let members: Vec<&str> = first.items.iter().map(|i| i.member.as_str()).collect();
    assert!(members.is_empty(), "{:#?}", first.items);
    let r = run(&dash(vec![p], vec![]));
    let panel = first_panel(&doc(&r)).clone();
    assert_eq!(panel["options"]["orientation"], json!("auto"));
    assert_eq!(panel["options"]["fill_opacity"], json!(61));
    assert!(panel["options"].get("line_width").is_none());
    assert_eq!(panel["fields"]["defaults"], json!({"soft_min": 0, "axis_grid": false}));
}
