// SPDX-License-Identifier: MIT OR Apache-2.0
//! Items P10 and P14 (importer row I44): the importer maps `custom.gradientMode` of a time series and of a bar
//! chart, and the cosmetic series members `lineStyle.dash`, `scaleDistribution.log` 2, `spanNulls` as a number, `barAlignment`,
//! `axisCenteredZero`, `axisWidth`, `axisBorderShow` and `axisColorMode` (4.18, 4.28, 4.30, 15.6). `symlog` stays unsupported.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport};
use serde_json::{Value, json};

fn one(p: Value) -> DashboardImport {
    run_plain(&dash(vec![p], vec![]))
}

fn with(ptype: &str, custom: Value) -> Value {
    let mut p = panel(1, ptype, 0);
    p["fieldConfig"] = json!({"defaults": {"custom": custom}, "overrides": []});
    p
}

fn mapped(p: Value) -> Value {
    let r = one(p);
    no_items(&r);
    first_panel(&doc(&r)).clone()
}

#[test]
fn the_gradient_mode_maps_to_the_option() {
    for t in ["timeseries", "barchart"] {
        for mode in ["opacity", "hue", "scheme"] {
            let out = mapped(with(t, json!({"gradientMode": mode})));
            assert_eq!(out["options"]["gradient_mode"], json!(mode), "{t} {mode}");
        }
        // `none` and `null` are the default: no member.
        for v in [json!("none"), json!(null)] {
            let out = mapped(with(t, json!({"gradientMode": v})));
            assert!(out["options"].get("gradient_mode").is_none(), "{t}: {out}");
        }
        let r = one(with(t, json!({"gradientMode": "rainbow"})));
        assert_eq!(one_item(&r, "fieldConfig.defaults.custom.gradientMode").class, Class::Unsupported, "{t}");
    }
    // A histogram has no gradient (4.33d): the item stays.
    let r = one(with("histogram", json!({"gradientMode": "hue"})));
    assert_eq!(one_item(&r, "fieldConfig.defaults.custom.gradientMode").class, Class::Unsupported);
}

#[test]
fn the_axis_look_maps_in_the_defaults_and_in_an_override() {
    for t in ["timeseries", "barchart"] {
        let out = mapped(with(
            t,
            json!({"axisCenteredZero": true, "axisWidth": 3, "axisBorderShow": true, "axisColorMode": "series"}),
        ));
        let d = &out["fields"]["defaults"];
        assert_eq!(
            (
                d["axis_centered_zero"].clone(),
                d["axis_width"].clone(),
                d["axis_border"].clone(),
                d["axis_color"].clone()
            ),
            (json!(true), json!(3), json!(true), json!("series")),
            "{t}"
        );
        // The Grafana defaults in `defaults` are metadata.
        let out = mapped(with(
            t,
            json!({"axisCenteredZero": false, "axisBorderShow": false, "axisColorMode": "text", "axisWidth": null}),
        ));
        assert_eq!(out["fields"]["defaults"], json!({}), "{t}");
        // An override keeps an explicit value.
        let mut p = with(t, json!({}));
        p["fieldConfig"]["overrides"] = json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
            {"id": "custom.axisBorderShow", "value": false}, {"id": "custom.axisColorMode", "value": "text"}, {"id": "custom.axisWidth", "value": 0}]}]);
        let out = mapped(p);
        assert_eq!(
            out["fields"]["overrides"][0]["set"],
            json!({"axis_width": 0, "axis_border": false, "axis_color": "text"}),
            "{t}"
        );
        for (key, bad) in [
            ("axisWidth", json!(501)),
            ("axisWidth", json!(2.5)),
            ("axisColorMode", json!("rainbow")),
            ("axisBorderShow", json!("yes")),
        ] {
            let r = one(with(t, json!({key: bad})));
            assert_eq!(
                one_item(&r, &format!("fieldConfig.defaults.custom.{key}")).class,
                Class::Unsupported,
                "{t} {key}"
            );
        }
    }
}

#[test]
fn span_nulls_as_a_number_is_the_null_threshold() {
    for (given, span, ms) in [
        (json!(150000), json!(false), json!(150000)),
        (json!(1500.5), json!(false), json!(1501)),
        (json!(-1), json!(false), Value::Null),
        (json!(0), json!(false), Value::Null),
        (json!(true), json!(true), Value::Null),
    ] {
        let out = mapped(with("timeseries", json!({"spanNulls": given})));
        assert_eq!(out["options"]["span_nulls"], span, "{given}");
        assert_eq!(out["options"].get("span_nulls_ms").cloned().unwrap_or(Value::Null), ms, "{given}");
    }
    // A threshold above 10^12 ms joins each run, as `true` does: changed.
    let r = one(with("timeseries", json!({"spanNulls": 2e12})));
    assert_eq!(one_item(&r, "fieldConfig.defaults.custom.spanNulls").class, Class::Changed);
    assert_eq!(
        first_panel(&doc(&run(&dash(vec![with("timeseries", json!({"spanNulls": 2e12}))], vec![]))))["options"]["span_nulls"],
        json!(true)
    );
}

#[test]
fn the_bar_alignment_maps_to_the_option() {
    for (given, want) in [(-1, Some("before")), (0, None), (1, Some("after"))] {
        let out = mapped(with("timeseries", json!({"drawStyle": "bars", "barAlignment": given})));
        assert_eq!(out["options"].get("bar_alignment").and_then(Value::as_str), want, "{given}");
    }
    let r = one(with("timeseries", json!({"barAlignment": 2})));
    assert_eq!(one_item(&r, "fieldConfig.defaults.custom.barAlignment").class, Class::Unsupported);
}

#[test]
fn the_log_scale_of_base_2_maps_and_symlog_stays_unsupported() {
    let out = mapped(with("timeseries", json!({"scaleDistribution": {"type": "log", "log": 2}})));
    assert_eq!((out["options"]["y_scale"].clone(), out["options"]["log_base"].clone()), (json!("log"), json!(2)));
    let out = mapped(with("timeseries", json!({"scaleDistribution": {"type": "log", "log": 10}})));
    assert!(out["options"].get("log_base").is_none());
    // The base of a linear scale is metadata.
    let out = mapped(with("timeseries", json!({"scaleDistribution": {"type": "linear", "log": 2}})));
    assert!(out["options"].get("log_base").is_none());
    let r = one(with("timeseries", json!({"scaleDistribution": {"type": "log", "log": 3}})));
    assert_eq!(one_item(&r, "fieldConfig.defaults.custom.scaleDistribution.log").class, Class::Unsupported);
    // The symmetric log scale is for a later wave (19.20a).
    let r = one(with("timeseries", json!({"scaleDistribution": {"type": "symlog", "log": 2, "linearThreshold": 1}})));
    assert!(
        r.items
            .iter()
            .any(|i| i.member.starts_with("fieldConfig.defaults.custom.scaleDistribution")
                && i.class == Class::Unsupported),
        "{:#?}",
        r.items
    );
}

#[test]
fn the_dash_array_maps_to_line_dash_array() {
    for (style, want) in [
        (json!({"fill": "dash", "dash": [10, 20]}), json!({"line_dash": "dash", "line_dash_array": [10, 20]})),
        (json!({"fill": "dot", "dash": [0, 20]}), json!({"line_dash": "dot", "line_dash_array": [0, 20]})),
        (json!({"fill": "dash", "dash": [30, 3, 3]}), json!({"line_dash": "dash", "line_dash_array": [30, 3, 3]})),
        (json!({"fill": "dash", "dash": [10, 10]}), json!({"line_dash": "dash"})),
        (json!({"fill": "dot", "dash": [0, 10]}), json!({"line_dash": "dot"})),
        // Grafana draws 10, 10 for a dot with no array, and dashes for an array with no fill.
        (json!({"fill": "dot"}), json!({"line_dash": "dot", "line_dash_array": [10, 10]})),
        (json!({"dash": [20, 10]}), json!({"line_dash_array": [20, 10]})),
        (json!({"fill": "solid", "dash": [10, 20]}), json!({})),
    ] {
        let out = mapped(with("timeseries", json!({"lineStyle": style})));
        assert_eq!(out["fields"]["defaults"], want, "{style}");
    }
    for bad in [json!([10, 200]), json!([0, 0]), json!([]), json!([1, 2, 3, 4, 5, 6, 7, 8, 9]), json!("10, 20")] {
        let r = one(with("timeseries", json!({"lineStyle": {"fill": "dash", "dash": bad}})));
        assert_eq!(one_item(&r, "fieldConfig.defaults.custom.lineStyle.dash").class, Class::Unsupported, "{bad}");
        assert_eq!(
            first_panel(&doc(&run(&dash(
                vec![with("timeseries", json!({"lineStyle": {"fill": "dash", "dash": bad}}))],
                vec![]
            ))))["fields"]["defaults"],
            json!({"line_dash": "dash"})
        );
    }
}

#[test]
fn the_panels_of_the_plugin_examples_import_with_no_item_for_these_members() {
    // The custom members of four panels of the ClickHouse plugin 4.5.1 examples (measure M1 of the later options): the two gradient time series
    // of "Query Analysis", a bar chart of "Data Analysis" and the time series with `axisWidth` of "Data Analysis".
    let hide = json!({"legend": false, "tooltip": false, "viz": false});
    let requests = json!({"axisLabel": "", "axisPlacement": "auto", "barAlignment": 0, "drawStyle": "bars", "fillOpacity": 0, "gradientMode": "opacity",
        "hideFrom": hide, "lineInterpolation": "linear", "lineWidth": 1, "pointSize": 5, "scaleDistribution": {"type": "linear"}, "showPoints": "auto",
        "spanNulls": false, "stacking": {"group": "A", "mode": "normal"}, "thresholdsStyle": {"mode": "off"}});
    let memory = json!({"axisLabel": "", "axisPlacement": "auto", "barAlignment": 0, "drawStyle": "line", "fillOpacity": 20, "gradientMode": "scheme",
        "hideFrom": hide, "lineInterpolation": "smooth", "lineWidth": 3, "pointSize": 5, "scaleDistribution": {"type": "linear"}, "showPoints": "auto",
        "spanNulls": false, "stacking": {"group": "A", "mode": "none"}, "thresholdsStyle": {"mode": "off"}});
    let engines = json!({"axisGridShow": false, "axisLabel": "", "axisPlacement": "auto", "axisSoftMin": 0, "fillOpacity": 61, "gradientMode": "hue",
        "hideFrom": hide, "lineWidth": 1, "scaleDistribution": {"type": "linear"}});
    let parts = json!({"axisLabel": "Rows in part", "axisPlacement": "auto", "axisWidth": 3, "barAlignment": 0, "drawStyle": "line", "fillOpacity": 10,
        "gradientMode": "none", "hideFrom": hide, "lineInterpolation": "linear", "lineStyle": {"fill": "solid"}, "lineWidth": 1, "pointSize": 5,
        "scaleDistribution": {"type": "linear"}, "showPoints": "always", "spanNulls": true, "stacking": {"group": "A", "mode": "none"},
        "thresholdsStyle": {"mode": "off"}});
    let out = mapped(with("timeseries", requests));
    assert_eq!(
        (out["options"]["gradient_mode"].clone(), out["options"]["draw"].clone()),
        (json!("opacity"), json!("bars"))
    );
    let mut m = with("timeseries", memory);
    m["fieldConfig"]["defaults"]["color"] = json!({"mode": "continuous-GrYlRd"});
    let out = mapped(m);
    assert_eq!(out["options"]["gradient_mode"], json!("scheme"));
    let out = mapped(with("barchart", engines));
    assert_eq!(
        (out["options"]["gradient_mode"].clone(), out["options"]["fill_opacity"].clone()),
        (json!("hue"), json!(61))
    );
    let out = mapped(with("timeseries", parts));
    assert_eq!(out["fields"]["defaults"]["axis_width"], json!(3));
    assert_eq!(out["fields"]["defaults"]["axis_label"], json!("Rows in part"));
}
