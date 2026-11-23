// SPDX-License-Identifier: MIT OR Apache-2.0
//! The rows of the heatmap and the histogram in 15.6 (4.33c, 4.33d): one test for each new row in the form v1
//! and in the forms v2, the boundaries of each range, the 38 schemes, the old heatmap of Grafana 7 to 9, and the
//! 8 measured dashboards (15.14c).

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback, ReportItem};
use serde_json::{Value, json};

const FORMS: [&str; 4] = ["raw", "wrapped", "alpha1", "beta1"];

/// The parts of a test dashboard with one panel.
#[derive(Clone)]
struct Parts {
    ptype: &'static str,
    options: Value,
    defaults: Value,
    overrides: Value,
}

impl Parts {
    fn heatmap(options: Value) -> Parts {
        Parts { ptype: "heatmap", options, defaults: json!({}), overrides: json!([]) }
    }

    fn histogram(options: Value) -> Parts {
        Parts { ptype: "histogram", options, defaults: json!({}), overrides: json!([]) }
    }

    fn custom(mut self, custom: Value) -> Parts {
        self.defaults["custom"] = custom;
        self
    }
}

fn v1(parts: &Parts) -> Value {
    let mut p = panel(1, parts.ptype, 0);
    p["fieldConfig"] = json!({"defaults": parts.defaults, "overrides": parts.overrides});
    p["options"] = parts.options.clone();
    dash(vec![p], vec![])
}

fn v2(form: &str, parts: &Parts) -> Value {
    let field_config = json!({"defaults": parts.defaults, "overrides": parts.overrides});
    let viz = if form == "alpha1" {
        json!({"kind": parts.ptype, "spec": {"pluginVersion": "12.4.2", "options": parts.options, "fieldConfig": field_config}})
    } else {
        json!({"kind": "VizConfig", "group": parts.ptype, "version": "12.4.2", "spec": {"options": parts.options, "fieldConfig": field_config}})
    };
    let spec =
        json!({"rawSql": "SELECT 1", "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"});
    let query = if form == "alpha1" {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "grafana-clickhouse-datasource", "spec": spec},
            "datasource": {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"}, "refId": "A", "hidden": false}})
    } else {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "grafana-clickhouse-datasource",
            "version": "v0", "datasource": {"name": "clickhouse"}, "spec": spec}, "refId": "A", "hidden": false}})
    };
    let element = json!({"kind": "Panel", "spec": {"id": 1, "title": "P1", "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [query], "transformations": [], "queryOptions": {}}},
        "vizConfig": viz}});
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{}", if form == "alpha1" { "alpha1" } else { "beta1" }),
        "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "Buckets", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "autoRefresh": ""},
            "variables": [], "elements": {"panel-1": element},
            "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {
                "x": 0, "y": 0, "width": 12, "height": 4, "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}}
    })
}

fn in_form(form: &str, parts: &Parts) -> Value {
    match form {
        "raw" => v1(parts),
        "wrapped" => json!({"dashboard": v1(parts), "meta": {"slug": "t1"}}),
        other => v2(other, parts),
    }
}

fn import(form: &str, parts: &Parts) -> DashboardImport {
    run(&in_form(form, parts))
}

/// Runs a check in the four forms. The check gets the panel of the document and the items of the run.
fn every_form(parts: &Parts, check: impl Fn(&str, &Value, &DashboardImport)) {
    for form in FORMS {
        let r = import(form, parts);
        let d = doc(&r);
        check(form, &first_panel(&d).clone(), &r);
    }
}

fn items<'a>(r: &'a DashboardImport, member: &str) -> Vec<&'a ReportItem> {
    r.items.iter().filter(|i| i.member == member || i.member.starts_with(&format!("{member}."))).collect()
}

/// The first run with no acceptance: its items name each member that is not mapped.
fn first_run(parts: &Parts) -> DashboardImport {
    run_plain(&in_form("raw", parts))
}

fn heat_defaults() -> Value {
    json!({
        "calculate": false,
        "x_buckets": {"mode": "count", "value": null},
        "y_buckets": {"mode": "count", "value": null, "scale": "linear"},
        "y_axis": "left", "y_reverse": false, "y_min": null, "y_max": null,
        "color": {"mode": "scheme", "scheme": "oranges", "fill": "dark-orange", "scale": "exponential",
                  "exponent": 0.5, "steps": 64, "reverse": false, "min": null, "max": null},
        "cell_gap": 1, "hide_le": 0, "hide_ge": null, "show_legend": true, "tooltip": {"mode": "single"}
    })
}

fn hist_defaults() -> Value {
    json!({
        "bucket_count": 30, "bucket_size": null, "bucket_offset": 0, "combine": false, "stacking": "none",
        "line_width": 1, "fill_opacity": 80,
        "legend": {"show": true, "placement": "bottom", "mode": "list", "values": []},
        "tooltip": {"mode": "single"}
    })
}

fn calc(options: Value) -> Value {
    let mut o = options;
    o["calculate"] = json!(true);
    o
}

fn y_buckets(value: Value) -> Value {
    calc(json!({"calculation": {"yBuckets": value}}))
}

fn x_buckets(value: Value) -> Value {
    calc(json!({"calculation": {"xBuckets": value}}))
}

fn fallback_text(i: &ReportItem) -> String {
    i.fallback.as_ref().map(|f| f.text().to_owned()).unwrap_or_default()
}

// ------------------------------------------------------------------ the type and the defaults

#[test]
fn the_two_ids_map_to_the_two_types_with_the_defaults_in_each_form() {
    every_form(&Parts::heatmap(json!({})), |form, p, r| {
        assert_eq!(p["type"], "heatmap", "{form}");
        assert_eq!(p["options"], heat_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    every_form(&Parts::histogram(json!({})), |form, p, r| {
        assert_eq!(p["type"], "histogram", "{form}");
        assert_eq!(p["options"], hist_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

#[test]
fn the_default_options_of_grafana_make_no_item() {
    let heat = json!({
        "calculate": false, "cellGap": 1, "cellRadius": 0,
        "color": {"exponent": 0.5, "fill": "dark-orange", "mode": "scheme", "reverse": false, "scale": "exponential",
                  "scheme": "Oranges", "steps": 64},
        "exemplars": {"color": "rgba(255,0,255,0.7)"}, "filterValues": {"le": 1e-9},
        "legend": {"show": true}, "rowsFrame": {"layout": "auto"}, "selectionMode": "x", "showValue": "auto",
        "tooltip": {"mode": "single", "showColorScale": false, "yHistogram": false},
        "yAxis": {"axisPlacement": "left"}
    });
    let parts = Parts::heatmap(heat).custom(json!({
        "hideFrom": {"legend": false, "tooltip": false, "viz": false}, "scaleDistribution": {"type": "linear"}
    }));
    every_form(&parts, |form, p, r| {
        assert_eq!(p["options"], heat_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    let hist = json!({
        "bucketCount": 30, "bucketOffset": 0, "combine": false,
        "legend": {"calcs": [], "displayMode": "list", "placement": "bottom", "showLegend": true},
        "tooltip": {"mode": "single", "sort": "none"}
    });
    let parts = Parts::histogram(hist).custom(json!({
        "fillOpacity": 80, "gradientMode": "none", "lineWidth": 1,
        "hideFrom": {"legend": false, "tooltip": false, "viz": false}, "stacking": {"group": "A", "mode": "none"}
    }));
    every_form(&parts, |form, p, r| {
        assert_eq!(p["options"], hist_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

#[test]
fn an_unknown_panel_type_names_the_12_types() {
    let mut p = panel(1, "heatmaps", 0);
    p["options"] = json!({});
    let r = run_plain(&dash(vec![p], vec![]));
    assert!(r.items.iter().any(|i| i.reason.contains("is not a type of 4.13")), "{:#?}", r.items);
}

// ------------------------------------------------------------------ the old heatmap

#[test]
fn the_old_heatmap_of_grafana_7_to_9_is_removed_and_the_report_names_the_first_member() {
    let members = [
        ("cards", json!({"cardPadding": null, "cardRound": null})),
        ("dataFormat", json!("timeseries")),
        ("xBucketSize", json!("10")),
        ("xBucketNumber", json!(20)),
        ("yBucketSize", json!(5)),
        ("yBucketNumber", json!(10)),
        ("yBucketBound", json!("auto")),
        ("hideZeroBuckets", json!(true)),
        ("reverseYBuckets", json!(true)),
    ];
    for (name, value) in &members {
        let mut old = panel(1, "heatmap", 0);
        old[*name] = value.clone();
        let keep = panel(2, "stat", 4);
        let d = dash(vec![old, keep], vec![]);
        let r = run_plain(&d);
        let item = r.items.iter().find(|i| i.fallback == Some(Fallback::PanelRemoved)).expect(name);
        assert_eq!((item.class, item.member.as_str(), item.panel_id), (Class::Unsupported, *name, Some(1)), "{name}");
        // The acceptance of the item removes the panel: the other panel stays.
        let accepted = doc(&run(&d));
        let panels = accepted["sections"][0]["panels"].as_array().unwrap();
        assert_eq!(panels.len(), 1, "{name}");
        assert_eq!(panels[0]["type"], "stat");
    }
    // Two old members: the first member of the list of 15.6 is named.
    let mut old = panel(1, "heatmap", 0);
    old["reverseYBuckets"] = json!(true);
    old["xBucketSize"] = json!("10");
    let r = run_plain(&dash(vec![old], vec![]));
    let item = r.items.iter().find(|i| i.fallback == Some(Fallback::PanelRemoved)).unwrap();
    assert_eq!(item.member, "xBucketSize");
    // A heatmap of the new form with a `cards` member in the options is not the old form.
    let mut new = panel(1, "heatmap", 0);
    new["options"] = json!({"calculate": false});
    let r = run_plain(&dash(vec![new], vec![]));
    assert!(r.items.iter().all(|i| i.fallback != Some(Fallback::PanelRemoved)), "{:#?}", r.items);
}

// ------------------------------------------------------------------ calculate and the buckets

#[test]
fn calculate_maps_and_an_absent_member_gives_false() {
    for value in [true, false] {
        every_form(&Parts::heatmap(json!({"calculate": value})), |form, p, r| {
            assert_eq!(p["options"]["calculate"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
        });
    }
    let r = first_run(&Parts::heatmap(json!({"calculate": "yes"})));
    let i = items(&r, "options.calculate");
    assert_eq!((i.len(), i[0].class), (1, Class::Unsupported), "{:#?}", r.items);
}

#[test]
fn the_x_buckets_map_the_modes_and_the_boundaries() {
    // (Grafana mode, Grafana value, mode of the document, value of the document)
    let ok: [(Value, Value, &str, Value); 11] = [
        (json!("count"), json!("10"), "count", json!(10)),
        (json!("count"), json!("1"), "count", json!(1)),
        (json!("count"), json!("1000"), "count", json!(1000)),
        (json!("count"), json!(20), "count", json!(20)),
        (json!("size"), json!("10m"), "size", json!(600_000)),
        (json!("size"), json!("1ms"), "size", json!(1)),
        (json!("size"), json!("365d"), "size", json!(31_536_000_000_i64)),
        (json!("size"), json!("5000"), "size", json!(5000)),
        (json!("size"), json!(1), "size", json!(1)),
        (json!("size"), json!("1h"), "size", json!(3_600_000)),
        (json!("count"), json!(""), "count", json!(null)),
    ];
    for (mode, value, want_mode, want) in ok {
        let parts = Parts::heatmap(x_buckets(json!({"mode": mode, "value": value})));
        every_form(&parts, |form, p, r| {
            let x = &p["options"]["x_buckets"];
            assert_eq!(
                (x["mode"].clone(), x["value"].clone()),
                (json!(want_mode), want.clone()),
                "{form} {mode} {value}"
            );
            assert!(r.items.is_empty(), "{form} {mode} {value}: {:#?}", r.items);
        });
    }
    // An absent mode is `size`. An absent or empty value gives `count` with `null`.
    let parts = Parts::heatmap(x_buckets(json!({"value": "30s"})));
    every_form(&parts, |form, p, _| {
        assert_eq!(p["options"]["x_buckets"], json!({"mode": "size", "value": 30000}), "{form}")
    });
    for xb in [json!({}), json!({"mode": "size"}), json!({"value": ""}), json!({"mode": "size", "value": null})] {
        every_form(&Parts::heatmap(x_buckets(xb.clone())), |form, p, r| {
            assert_eq!(p["options"]["x_buckets"], json!({"mode": "count", "value": null}), "{form} {xb}");
            assert!(r.items.is_empty(), "{form} {xb}");
        });
    }
    // Values out of range are `unsupported`; the fallback is `count` with `null`.
    for (mode, value) in [
        ("count", json!("0")),
        ("count", json!("1001")),
        ("count", json!("1.5")),
        ("count", json!("ten")),
        ("size", json!("0ms")),
        ("size", json!("366d")),
        ("size", json!("31536000001")),
        ("size", json!("10x")),
        ("size", json!("0")),
    ] {
        let parts = Parts::heatmap(x_buckets(json!({"mode": mode, "value": value})));
        let r = first_run(&parts);
        let i = items(&r, "options.calculation.xBuckets.value");
        assert_eq!(i.len(), 1, "{mode} {value}: {:#?}", r.items);
        assert_eq!(
            (i[0].class, fallback_text(i[0])),
            (Class::Unsupported, "count with null".to_owned()),
            "{mode} {value}"
        );
        every_form(&parts, |form, p, _| {
            assert_eq!(p["options"]["x_buckets"], json!({"mode": "count", "value": null}), "{form} {mode} {value}");
        });
    }
    let r = first_run(&Parts::heatmap(x_buckets(json!({"mode": "bins", "value": "5"}))));
    assert_eq!(items(&r, "options.calculation.xBuckets.mode")[0].class, Class::Unsupported, "{:#?}", r.items);
}

#[test]
fn the_x_scale_has_no_equal() {
    let parts = Parts::heatmap(x_buckets(json!({"mode": "count", "value": "5", "scale": {"type": "log", "log": 2}})));
    let r = first_run(&parts);
    let i = items(&r, "options.calculation.xBuckets.scale");
    assert_eq!((i.len(), i[0].class, i[0].fallback.clone()), (1, Class::Unsupported, Some(Fallback::MemberRemoved)));
    every_form(&parts, |form, p, _| {
        assert_eq!(p["options"]["x_buckets"], json!({"mode": "count", "value": 5}), "{form}");
    });
    // The default linear scale is metadata.
    let parts = Parts::heatmap(x_buckets(json!({"mode": "count", "value": "5", "scale": {"type": "linear"}})));
    assert!(first_run(&parts).items.is_empty());
}

#[test]
fn the_y_buckets_map_the_linear_scale_and_its_boundaries() {
    let ok: [(Value, Value, &str, Value); 8] = [
        (json!("count"), json!("5"), "count", json!(5)),
        (json!("count"), json!("1"), "count", json!(1)),
        (json!("count"), json!("1000"), "count", json!(1000)),
        (json!("size"), json!("20"), "size", json!(20)),
        (json!("size"), json!("0.5"), "size", json!(0.5)),
        (json!("size"), json!(2.5), "size", json!(2.5)),
        (json!("count"), json!(""), "count", json!(null)),
        (json!("size"), json!("0.0001"), "size", json!(0.0001)),
    ];
    for (mode, value, want_mode, want) in ok {
        let parts = Parts::heatmap(y_buckets(json!({"mode": mode, "value": value})));
        every_form(&parts, |form, p, r| {
            let y = &p["options"]["y_buckets"];
            assert_eq!(
                (y["mode"].clone(), y["value"].clone(), y["scale"].clone()),
                (json!(want_mode), want.clone(), json!("linear")),
                "{form} {mode} {value}"
            );
            assert!(r.items.is_empty(), "{form} {mode} {value}: {:#?}", r.items);
        });
    }
    let parts = Parts::heatmap(y_buckets(json!({"value": "3"})));
    every_form(&parts, |form, p, _| {
        assert_eq!(p["options"]["y_buckets"], json!({"mode": "size", "value": 3, "scale": "linear"}), "{form}");
    });
    for (mode, value) in [
        ("count", json!("0")),
        ("count", json!("1001")),
        ("count", json!("2.5")),
        ("size", json!("0")),
        ("size", json!("-1")),
        ("size", json!("big")),
    ] {
        let parts = Parts::heatmap(y_buckets(json!({"mode": mode, "value": value})));
        let r = first_run(&parts);
        let i = items(&r, "options.calculation.yBuckets.value");
        assert_eq!(i.len(), 1, "{mode} {value}: {:#?}", r.items);
        assert_eq!(
            (i[0].class, fallback_text(i[0])),
            (Class::Unsupported, "count with null".to_owned()),
            "{mode} {value}"
        );
        every_form(&parts, |form, p, _| {
            assert_eq!(p["options"]["y_buckets"], json!({"mode": "count", "value": null, "scale": "linear"}), "{form}");
        });
    }
}

#[test]
fn a_log_scale_gives_log2_or_log10_with_the_split() {
    for (base, name) in [(2, "log2"), (10, "log10")] {
        for (value, want) in [
            (json!("1"), json!(1)),
            (json!("2"), json!(2)),
            (json!("16"), json!(16)),
            (json!(3), json!(3)),
            (json!(""), json!(null)),
        ] {
            let parts = Parts::heatmap(y_buckets(
                json!({"mode": "size", "value": value, "scale": {"type": "log", "log": base}}),
            ));
            every_form(&parts, |form, p, r| {
                assert_eq!(
                    p["options"]["y_buckets"],
                    json!({"mode": "size", "value": want, "scale": name}),
                    "{form} {name} {value}"
                );
                assert!(r.items.is_empty(), "{form} {name} {value}: {:#?}", r.items);
            });
        }
    }
    // An absent mode is `size` already: the mode `count` is `changed`.
    let parts = Parts::heatmap(y_buckets(json!({"value": "2", "scale": {"type": "log", "log": 2}})));
    assert!(first_run(&parts).items.is_empty());
    let parts = Parts::heatmap(y_buckets(json!({"mode": "count", "value": "2", "scale": {"type": "log", "log": 2}})));
    let r = first_run(&parts);
    let i = items(&r, "options.calculation.yBuckets.mode");
    assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{:#?}", r.items);
    every_form(&parts, |form, p, _| {
        assert_eq!(p["options"]["y_buckets"], json!({"mode": "size", "value": 2, "scale": "log2"}), "{form}");
    });
    // A split out of 1 to 16 is `unsupported`; the fallback is `null`.
    for value in [json!("0"), json!("17"), json!("1.5"), json!("x")] {
        let parts =
            Parts::heatmap(y_buckets(json!({"mode": "size", "value": value, "scale": {"type": "log", "log": 10}})));
        let r = first_run(&parts);
        let i = items(&r, "options.calculation.yBuckets.value");
        assert_eq!((i.len(), i[0].class, fallback_text(i[0])), (1, Class::Unsupported, "null".to_owned()), "{value}");
        every_form(&parts, |form, p, _| {
            assert_eq!(p["options"]["y_buckets"], json!({"mode": "size", "value": null, "scale": "log10"}), "{form}");
        });
    }
}

#[test]
fn another_scale_is_unsupported_and_the_fallback_is_linear() {
    for scale in [
        json!({"type": "symlog", "log": 10}),
        json!({"type": "log", "log": 3}),
        json!({"type": "log"}),
        json!({"type": "ordinal"}),
    ] {
        let parts = Parts::heatmap(y_buckets(json!({"mode": "size", "value": "5", "scale": scale})));
        let r = first_run(&parts);
        assert!(
            r.items
                .iter()
                .any(|i| i.class == Class::Unsupported && i.member.starts_with("options.calculation.yBuckets.scale")),
            "{scale}: {:#?}",
            r.items
        );
        every_form(&parts, |form, p, _| {
            assert_eq!(
                p["options"]["y_buckets"],
                json!({"mode": "size", "value": 5, "scale": "linear"}),
                "{form} {scale}"
            );
        });
    }
    // The type `linear` with a `log` member is metadata.
    let parts = Parts::heatmap(y_buckets(json!({"mode": "size", "value": "5", "scale": {"type": "linear", "log": 2}})));
    assert!(first_run(&parts).items.is_empty());
}

// ------------------------------------------------------------------ the value axis

#[test]
fn y_axis_placement_and_reverse_map() {
    for (value, want) in [("left", "left"), ("hidden", "hidden")] {
        every_form(&Parts::heatmap(json!({"yAxis": {"axisPlacement": value}})), |form, p, r| {
            assert_eq!(p["options"]["y_axis"], want, "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let parts = Parts::heatmap(json!({"yAxis": {"axisPlacement": "right"}}));
    let r = first_run(&parts);
    let i = items(&r, "options.yAxis.axisPlacement");
    assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{:#?}", r.items);
    every_form(&parts, |form, p, _| assert_eq!(p["options"]["y_axis"], "left", "{form}"));
    let r = first_run(&Parts::heatmap(json!({"yAxis": {"axisPlacement": "top"}})));
    assert_eq!(items(&r, "options.yAxis.axisPlacement")[0].class, Class::Unsupported);
    for value in [true, false] {
        every_form(&Parts::heatmap(json!({"yAxis": {"reverse": value}})), |form, p, r| {
            assert_eq!(p["options"]["y_reverse"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
}

#[test]
fn y_axis_min_and_max_map_numbers_and_number_texts() {
    for (value, want) in [
        (json!(50), json!(50)),
        (json!("0.5"), json!(0.5)),
        (json!(-3), json!(-3)),
        (json!(""), json!(null)),
        (json!(null), json!(null)),
    ] {
        every_form(&Parts::heatmap(json!({"yAxis": {"min": value, "max": value}})), |form, p, r| {
            assert_eq!(
                (p["options"]["y_min"].clone(), p["options"]["y_max"].clone()),
                (want.clone(), want.clone()),
                "{form} {value}"
            );
            assert!(r.items.is_empty(), "{form} {value}: {:#?}", r.items);
        });
    }
    let r = first_run(&Parts::heatmap(json!({"yAxis": {"min": "low", "max": true}})));
    for member in ["options.yAxis.min", "options.yAxis.max"] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class, fallback_text(i[0])), (1, Class::Unsupported, "null".to_owned()), "{member}");
    }
    // With a log scale a value of 0 or less is `unsupported`; the fallback is `null`.
    let scale = json!({"mode": "size", "value": "1", "scale": {"type": "log", "log": 2}});
    let mut options = y_buckets(scale);
    options["yAxis"] = json!({"min": 0, "max": -1});
    let parts = Parts::heatmap(options);
    let r = first_run(&parts);
    assert_eq!(items(&r, "options.yAxis.min").len() + items(&r, "options.yAxis.max").len(), 2, "{:#?}", r.items);
    every_form(&parts, |form, p, _| {
        assert_eq!(
            (p["options"]["y_min"].clone(), p["options"]["y_max"].clone()),
            (json!(null), json!(null)),
            "{form}"
        );
    });
    let mut options = y_buckets(json!({"mode": "size", "value": "1", "scale": {"type": "log", "log": 10}}));
    options["yAxis"] = json!({"min": 0.001, "max": 1000});
    every_form(&Parts::heatmap(options), |form, p, r| {
        assert_eq!(
            (p["options"]["y_min"].clone(), p["options"]["y_max"].clone()),
            (json!(0.001), json!(1000)),
            "{form}"
        );
        assert!(r.items.is_empty(), "{form}");
    });
}

#[test]
fn the_unit_and_the_decimals_of_the_axis_move_to_the_defaults_when_the_heatmap_buckets_the_values() {
    let options = calc(json!({"yAxis": {"unit": "ms", "decimals": 1}}));
    let parts = Parts::heatmap(options.clone());
    let r = first_run(&parts);
    for member in ["options.yAxis.unit", "options.yAxis.decimals"] {
        assert_eq!(items(&r, member)[0].class, Class::Changed, "{member}: {:#?}", r.items);
    }
    every_form(&parts, |form, p, _| {
        assert_eq!(p["fields"]["defaults"], json!({"unit": "ms", "decimals": 1}), "{form}");
    });
    // The defaults keep a member that they have.
    let mut parts = Parts::heatmap(options);
    parts.defaults = json!({"unit": "s", "decimals": 3, "min": 0});
    let r = first_run(&parts);
    assert_eq!(items(&r, "options.yAxis.unit")[0].class, Class::Unsupported);
    every_form(&parts, |form, p, _| {
        assert_eq!(p["fields"]["defaults"], json!({"unit": "s", "decimals": 3, "min": 0}), "{form}");
    });
    // A defaults object with `min` gets the unit before it (4.18, 4.38).
    let mut parts = Parts::heatmap(calc(json!({"yAxis": {"unit": "ms"}})));
    parts.defaults = json!({"min": 0});
    let text = String::from_utf8(import("raw", &parts).document_bytes.unwrap()).unwrap();
    let unit = text.find("\"unit\": \"ms\"").expect("the unit");
    assert!(unit < text.find("\"min\": 0").expect("the min"), "{text}");
    // With `calculate` false Grafana formats the names of the rows: the member is `unsupported`.
    let parts = Parts::heatmap(json!({"calculate": false, "yAxis": {"unit": "ms", "decimals": 1}}));
    let r = first_run(&parts);
    for member in ["options.yAxis.unit", "options.yAxis.decimals"] {
        let i = items(&r, member);
        assert_eq!(
            (i.len(), i[0].class, i[0].fallback.clone()),
            (1, Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{member}"
        );
    }
    every_form(&parts, |form, p, _| assert_eq!(p["fields"]["defaults"], json!({}), "{form}"));
    // A unit that is not in the list is `unsupported`.
    let r = first_run(&Parts::heatmap(calc(json!({"yAxis": {"unit": "parsecs"}}))));
    assert_eq!(items(&r, "options.yAxis.unit")[0].class, Class::Unsupported);
}

#[test]
fn the_axis_width_and_the_axis_label_are_dropped_unless_they_are_empty() {
    every_form(&Parts::heatmap(json!({"yAxis": {"axisWidth": 0, "axisLabel": ""}})), |form, _, r| {
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    let r = first_run(&Parts::heatmap(json!({"yAxis": {"axisWidth": 80, "axisLabel": "ms"}})));
    for member in ["options.yAxis.axisWidth", "options.yAxis.axisLabel"] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}");
    }
}

// ------------------------------------------------------------------ the colours

fn color_of(p: &Value) -> Value {
    p["options"]["color"].clone()
}

#[test]
fn the_colour_members_map_with_their_ranges() {
    let parts = Parts::heatmap(json!({"color": {
        "mode": "opacity", "scheme": "Blues", "fill": "#ff7f0e", "scale": "linear", "exponent": 2, "steps": 128,
        "reverse": true, "min": -5, "max": 5
    }}));
    every_form(&parts, |form, p, r| {
        assert_eq!(
            color_of(p),
            json!({"mode": "opacity", "scheme": "blues", "fill": "#ff7f0e", "scale": "linear", "exponent": 2,
                   "steps": 128, "reverse": true, "min": -5, "max": 5}),
            "{form}"
        );
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    for (member, value, key, want) in [
        ("exponent", json!(0.1), "exponent", json!(0.1)),
        ("steps", json!(2), "steps", json!(2)),
        ("mode", json!("scheme"), "mode", json!("scheme")),
        ("scale", json!("exponential"), "scale", json!("exponential")),
    ] {
        every_form(&Parts::heatmap(json!({"color": {member: value}})), |form, p, r| {
            assert_eq!(color_of(p)[key], want, "{form} {member}");
            assert!(r.items.is_empty(), "{form} {member}");
        });
    }
    // Out of range: `unsupported` with the default of the member.
    for (member, value, key, default) in [
        ("exponent", json!(0), "exponent", json!(0.5)),
        ("exponent", json!(2.5), "exponent", json!(0.5)),
        ("steps", json!(1), "steps", json!(64)),
        ("steps", json!(129), "steps", json!(64)),
        ("mode", json!("gradient"), "mode", json!("scheme")),
        ("scale", json!("log"), "scale", json!("exponential")),
        ("reverse", json!("yes"), "reverse", json!(false)),
        ("min", json!("low"), "min", json!(null)),
        ("max", json!("high"), "max", json!(null)),
    ] {
        let parts = Parts::heatmap(json!({"color": {member: value}}));
        let r = first_run(&parts);
        let i = items(&r, &format!("options.color.{member}"));
        assert_eq!((i.len(), i[0].class), (1, Class::Unsupported), "{member} {value}: {:#?}", r.items);
        every_form(&parts, |form, p, _| assert_eq!(color_of(p)[key], default, "{form} {member} {value}"));
    }
    // An invalid colour is `unsupported`; the default colour stays.
    let parts = Parts::heatmap(json!({"color": {"fill": 12}}));
    assert_eq!(items(&first_run(&parts), "options.color.fill")[0].class, Class::Unsupported);
    every_form(&parts, |form, p, _| assert_eq!(color_of(p)["fill"], "dark-orange", "{form}"));
}

#[test]
fn an_exponent_with_two_decimals_is_changed_to_one_decimal() {
    let parts = Parts::heatmap(json!({"color": {"exponent": 0.55}}));
    let r = first_run(&parts);
    assert_eq!(items(&r, "options.color.exponent")[0].class, Class::Changed, "{:#?}", r.items);
    every_form(&parts, |form, p, _| assert_eq!(color_of(p)["exponent"], json!(0.6), "{form}"));
    let r = first_run(&Parts::heatmap(json!({"color": {"exponent": 1.04}})));
    assert_eq!(items(&r, "options.color.exponent")[0].class, Class::Changed);
}

#[test]
fn a_colour_range_with_min_not_below_max_makes_two_items_and_two_nulls() {
    for (min, max) in [(5, 5), (10, 5)] {
        let parts = Parts::heatmap(json!({"color": {"min": min, "max": max}}));
        let r = first_run(&parts);
        assert_eq!(items(&r, "options.color.min").len() + items(&r, "options.color.max").len(), 2, "{:#?}", r.items);
        every_form(&parts, |form, p, _| {
            assert_eq!((color_of(p)["min"].clone(), color_of(p)["max"].clone()), (json!(null), json!(null)), "{form}");
        });
    }
    every_form(&Parts::heatmap(json!({"color": {"min": 50}})), |form, p, r| {
        assert_eq!((color_of(p)["min"].clone(), color_of(p)["max"].clone()), (json!(50), json!(null)), "{form}");
        assert!(r.items.is_empty());
    });
}

#[test]
fn each_scheme_name_of_grafana_maps_or_is_changed_as_the_row_says() {
    let mapped = [
        ("Oranges", "oranges"),
        ("Blues", "blues"),
        ("Greens", "greens"),
        ("Greys", "greys"),
        ("Purples", "purples"),
        ("Reds", "reds"),
        ("YlOrRd", "yl_or_rd"),
        ("RdYlGn", "rd_yl_gn"),
        ("Spectral", "spectral"),
        ("Viridis", "viridis"),
    ];
    for (name, want) in mapped {
        let parts = Parts::heatmap(json!({"color": {"scheme": name}}));
        every_form(&parts, |form, p, r| {
            assert_eq!(color_of(p)["scheme"], want, "{form} {name}");
            assert!(r.items.is_empty(), "{form} {name}");
        });
    }
    // The 28 other schemes of the 38 of Grafana are `changed` to `oranges`.
    let others = [
        "BrBG",
        "PiYG",
        "PRGn",
        "PuOr",
        "RdBu",
        "RdGy",
        "RdYlBu",
        "Turbo",
        "Cividis",
        "Magma",
        "Inferno",
        "Plasma",
        "Warm",
        "Cool",
        "Cubehelix",
        "CubehelixDefault",
        "BuGn",
        "BuPu",
        "GnBu",
        "OrRd",
        "PuBuGn",
        "PuBu",
        "PuRd",
        "RdPu",
        "YlGnBu",
        "YlGn",
        "YlOrBr",
        "Rainbow",
        "Sinebow",
    ];
    assert_eq!(
        mapped.len() + others.len() - 1,
        38,
        "the 38 schemes of Grafana (the second name of Cubehelix is an alias)"
    );
    for name in others {
        let parts = Parts::heatmap(json!({"color": {"scheme": name}}));
        let r = first_run(&parts);
        let i = items(&r, "options.color.scheme");
        assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{name}");
        every_form(&parts, |form, p, _| assert_eq!(color_of(p)["scheme"], "oranges", "{form} {name}"));
    }
    // A name that Grafana does not know gets the scheme Spectral in Grafana.
    for name in ["Plaid", "oranges", ""] {
        let parts = Parts::heatmap(json!({"color": {"scheme": name}}));
        let r = first_run(&parts);
        assert_eq!(items(&r, "options.color.scheme")[0].class, Class::Changed, "{name}");
        every_form(&parts, |form, p, _| assert_eq!(color_of(p)["scheme"], "spectral", "{form} {name}"));
    }
    let r = first_run(&Parts::heatmap(json!({"color": {"scheme": 3}})));
    assert_eq!(items(&r, "options.color.scheme")[0].class, Class::Unsupported);
}

// ------------------------------------------------------------------ the cells and the legend

#[test]
fn the_cell_gap_maps_at_its_boundaries() {
    for gap in [0, 1, 4, 25] {
        every_form(&Parts::heatmap(json!({"cellGap": gap})), |form, p, r| {
            assert_eq!(p["options"]["cell_gap"], json!(gap), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    for gap in [json!(-1), json!(26), json!("a"), json!(1.5)] {
        let parts = Parts::heatmap(json!({"cellGap": gap}));
        let i = items(&first_run(&parts), "options.cellGap")
            .into_iter()
            .map(|i| (i.class, fallback_text(i)))
            .collect::<Vec<_>>();
        assert_eq!(i, [(Class::Unsupported, "1".to_owned())], "{gap}");
        every_form(&parts, |form, p, _| assert_eq!(p["options"]["cell_gap"], json!(1), "{form} {gap}"));
    }
}

#[test]
fn the_radius_and_the_cell_values_are_dropped_unless_they_are_empty() {
    every_form(&Parts::heatmap(json!({"cellRadius": 0, "cellValues": {"unit": "", "decimals": 0}})), |form, _, r| {
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    let r = first_run(&Parts::heatmap(json!({"cellRadius": 4, "cellValues": {"unit": "short", "decimals": 1}})));
    for member in ["options.cellRadius", "options.cellValues.unit", "options.cellValues.decimals"] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}: {:#?}", r.items);
    }
}

#[test]
fn the_filter_values_map_the_two_limits() {
    let cases = [
        (json!({"le": 100, "ge": 400}), json!(100), json!(400)),
        (json!({"le": 1e-9}), json!(0), json!(null)),
        (json!({"le": null}), json!(null), json!(null)),
        (json!({"ge": 7}), json!(0), json!(7)),
        (json!({"le": -2.5, "ge": null}), json!(-2.5), json!(null)),
        (json!({}), json!(0), json!(null)),
    ];
    for (filter, le, ge) in cases {
        every_form(&Parts::heatmap(json!({"filterValues": filter})), |form, p, r| {
            assert_eq!(
                (p["options"]["hide_le"].clone(), p["options"]["hide_ge"].clone()),
                (le.clone(), ge.clone()),
                "{form} {filter}"
            );
            assert!(r.items.is_empty(), "{form} {filter}: {:#?}", r.items);
        });
    }
    let parts = Parts::heatmap(json!({"filterValues": {"le": "low", "ge": "high"}}));
    let r = first_run(&parts);
    assert_eq!(items(&r, "options.filterValues.le")[0].class, Class::Unsupported);
    assert_eq!(items(&r, "options.filterValues.ge")[0].class, Class::Unsupported);
    every_form(&parts, |form, p, _| {
        assert_eq!(
            (p["options"]["hide_le"].clone(), p["options"]["hide_ge"].clone()),
            (json!(0), json!(null)),
            "{form}"
        );
    });
}

#[test]
fn the_legend_of_a_heatmap_gives_show_legend() {
    for value in [true, false] {
        every_form(&Parts::heatmap(json!({"legend": {"show": value}})), |form, p, r| {
            assert_eq!(p["options"]["show_legend"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let r = first_run(&Parts::heatmap(json!({"legend": {"show": "no"}})));
    assert_eq!(items(&r, "options.legend.show")[0].class, Class::Unsupported);
}

#[test]
fn the_tooltip_of_a_heatmap_maps_the_mode_and_refuses_the_histogram_and_the_colour_bar() {
    for (value, want) in [("single", "single"), ("multi", "multi"), ("none", "hidden")] {
        every_form(&Parts::heatmap(json!({"tooltip": {"mode": value}})), |form, p, r| {
            assert_eq!(p["options"]["tooltip"], json!({"mode": want}), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let parts = Parts::heatmap(json!({"tooltip": {"mode": "single", "yHistogram": true, "showColorScale": true}}));
    let r = first_run(&parts);
    for member in ["options.tooltip.yHistogram", "options.tooltip.showColorScale"] {
        let i = items(&r, member);
        assert_eq!(
            (i.len(), i[0].class, i[0].fallback.clone()),
            (1, Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{member}"
        );
    }
    every_form(&parts, |form, p, _| assert_eq!(p["options"]["tooltip"], json!({"mode": "single"}), "{form}"));
    // `false` is metadata. `maxHeight` and `maxWidth` are listed when they have a value.
    assert!(
        first_run(&Parts::heatmap(json!({"tooltip": {"yHistogram": false, "showColorScale": false}}))).items.is_empty()
    );
    let r = first_run(&Parts::heatmap(json!({"tooltip": {"maxHeight": 600, "maxWidth": 300}})));
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(first_run(&Parts::heatmap(json!({"tooltip": {"maxHeight": 0}}))).items.is_empty());
}

#[test]
fn the_rows_frame_layouts_and_the_other_dropped_members() {
    for layout in ["auto", "unknown"] {
        let r = first_run(&Parts::heatmap(json!({"rowsFrame": {"layout": layout}})));
        assert!(r.items.is_empty(), "{layout}: {:#?}", r.items);
    }
    for layout in ["le", "ge"] {
        let r = first_run(&Parts::heatmap(json!({"rowsFrame": {"layout": layout}})));
        let i = items(&r, "options.rowsFrame.layout");
        assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{layout}");
    }
    let r =
        first_run(&Parts::heatmap(json!({"rowsFrame": {"value": "Count", "yBucketScale": {"type": "log", "log": 2}}})));
    for member in ["options.rowsFrame.value", "options.rowsFrame.yBucketScale"] {
        let i = items(&r, member);
        assert_eq!(
            (i.len(), i[0].class, i[0].fallback.clone()),
            (1, Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{member}"
        );
    }
    let r = first_run(&Parts::heatmap(json!({
        "selectionMode": "xy", "exemplars": {"color": "red"}, "showValue": "always"
    })));
    for member in ["options.selectionMode", "options.exemplars", "options.showValue"] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}");
    }
    let r = first_run(&Parts::heatmap(
        json!({"selectionMode": "x", "exemplars": {"color": "rgba(255,0,255,0.7)"}, "showValue": "auto"}),
    ));
    assert!(r.items.is_empty(), "{:#?}", r.items);
}

#[test]
fn the_custom_members_of_a_heatmap_are_dropped_unless_they_have_the_grafana_default() {
    let parts = Parts::heatmap(json!({})).custom(json!({
        "drawStyle": "bars", "lineWidth": 3, "scaleDistribution": {"type": "log", "log": 2}, "axisPlacement": "right"
    }));
    let r = first_run(&parts);
    for member in [
        "fieldConfig.defaults.custom.drawStyle",
        "fieldConfig.defaults.custom.lineWidth",
        "fieldConfig.defaults.custom.scaleDistribution",
        "fieldConfig.defaults.custom.axisPlacement",
    ] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}: {:#?}", r.items);
    }
    let parts = Parts::heatmap(json!({}))
        .custom(json!({"drawStyle": "line", "lineInterpolation": "linear", "showPoints": "auto", "pointSize": 5}));
    assert!(first_run(&parts).items.is_empty(), "{:#?}", first_run(&parts).items);
    // The hide-from member maps, and the colour and the unit of the fields are field members.
    let mut parts = Parts::heatmap(json!({}));
    parts.defaults = json!({"unit": "ms", "color": {"mode": "thresholds"}, "custom": {"hideFrom": {"legend": false, "tooltip": true, "viz": false}}});
    every_form(&parts, |form, p, r| {
        assert_eq!(
            p["fields"]["defaults"]["hide_from"],
            json!({"legend": false, "tooltip": true, "viz": false}),
            "{form}"
        );
        assert_eq!(p["fields"]["defaults"]["unit"], "ms", "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

// ------------------------------------------------------------------ the histogram

#[test]
fn the_bucket_count_maps_at_its_boundaries() {
    for n in [1, 10, 30, 1000] {
        every_form(&Parts::histogram(json!({"bucketCount": n})), |form, p, r| {
            assert_eq!(p["options"]["bucket_count"], json!(n), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    for n in [json!(0), json!(1001), json!(-5), json!("x"), json!(2.5)] {
        let parts = Parts::histogram(json!({"bucketCount": n}));
        let i = items(&first_run(&parts), "options.bucketCount")
            .into_iter()
            .map(|i| (i.class, fallback_text(i)))
            .collect::<Vec<_>>();
        assert_eq!(i, [(Class::Unsupported, "30".to_owned())], "{n}");
        every_form(&parts, |form, p, _| assert_eq!(p["options"]["bucket_count"], json!(30), "{form} {n}"));
    }
}

#[test]
fn the_bucket_size_and_the_offset_map() {
    for (value, want) in [
        (json!(25), json!(25)),
        (json!(0.01), json!(0.01)),
        (json!(0), json!(null)),
        (json!(""), json!(null)),
        (json!(null), json!(null)),
    ] {
        every_form(&Parts::histogram(json!({"bucketSize": value})), |form, p, r| {
            assert_eq!(p["options"]["bucket_size"], want, "{form} {value}");
            assert!(r.items.is_empty(), "{form} {value}: {:#?}", r.items);
        });
    }
    for value in [json!(-1), json!("big"), json!(true), json!([1])] {
        let parts = Parts::histogram(json!({"bucketSize": value}));
        let i = items(&first_run(&parts), "options.bucketSize")
            .into_iter()
            .map(|i| (i.class, fallback_text(i)))
            .collect::<Vec<_>>();
        assert_eq!(i, [(Class::Unsupported, "null".to_owned())], "{value}");
        every_form(&parts, |form, p, _| assert_eq!(p["options"]["bucket_size"], json!(null), "{form} {value}"));
    }
    for (value, want) in [(json!(10), json!(10)), (json!(-12.5), json!(-12.5)), (json!(0), json!(0))] {
        every_form(&Parts::histogram(json!({"bucketOffset": value})), |form, p, r| {
            assert_eq!(p["options"]["bucket_offset"], want, "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let parts = Parts::histogram(json!({"bucketOffset": "left"}));
    assert_eq!(items(&first_run(&parts), "options.bucketOffset")[0].class, Class::Unsupported);
    every_form(&parts, |form, p, _| assert_eq!(p["options"]["bucket_offset"], json!(0), "{form}"));
}

#[test]
fn combine_the_legend_and_the_tooltip_map_as_for_a_bar_chart() {
    for value in [true, false] {
        every_form(&Parts::histogram(json!({"combine": value})), |form, p, r| {
            assert_eq!(p["options"]["combine"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let r = first_run(&Parts::histogram(json!({"combine": "all"})));
    assert_eq!(items(&r, "options.combine")[0].class, Class::Unsupported);
    let parts = Parts::histogram(json!({
        "legend": {"calcs": ["max"], "displayMode": "table", "placement": "right", "showLegend": true},
        "tooltip": {"mode": "multi", "sort": "desc", "hideZeros": true}
    }));
    every_form(&parts, |form, p, r| {
        assert_eq!(
            p["options"]["legend"],
            json!({"show": true, "placement": "right", "mode": "table", "values": ["max"]}),
            "{form}"
        );
        assert_eq!(p["options"]["tooltip"], json!({"mode": "multi", "sort": "desc", "hide_zeros": true}), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

#[test]
fn stacking_the_line_and_the_fill_come_from_the_custom_members() {
    for mode in ["none", "normal", "percent"] {
        let parts = Parts::histogram(json!({})).custom(json!({"stacking": {"group": "A", "mode": mode}}));
        every_form(&parts, |form, p, r| {
            assert_eq!(p["options"]["stacking"], mode, "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    // The group of the stack is metadata, also when it is not the default.
    let parts = Parts::histogram(json!({})).custom(json!({"stacking": {"group": "B", "mode": "normal"}}));
    assert!(first_run(&parts).items.is_empty());
    for (width, fill) in [(0, 0), (10, 100), (3, 30)] {
        let parts = Parts::histogram(json!({})).custom(json!({"lineWidth": width, "fillOpacity": fill}));
        every_form(&parts, |form, p, r| {
            assert_eq!(
                (p["options"]["line_width"].clone(), p["options"]["fill_opacity"].clone()),
                (json!(width), json!(fill)),
                "{form}"
            );
            assert!(r.items.is_empty(), "{form}");
        });
    }
    for (member, value, key, default) in [
        ("lineWidth", json!(11), "line_width", json!(1)),
        ("lineWidth", json!(-1), "line_width", json!(1)),
        ("fillOpacity", json!(101), "fill_opacity", json!(80)),
        ("stacking", json!({"mode": "group"}), "stacking", json!("none")),
    ] {
        let parts = Parts::histogram(json!({})).custom(json!({member: value}));
        let r = first_run(&parts);
        assert!(!r.items.is_empty(), "{member} {value}");
        every_form(&parts, |form, p, _| assert_eq!(p["options"][key], default, "{form} {member} {value}"));
    }
    // In an override the member is `unsupported`; the property is removed.
    let mut parts = Parts::histogram(json!({}));
    parts.overrides = json!([{"matcher": {"id": "byName", "options": "Count"}, "properties": [{"id": "custom.lineWidth", "value": 3}]}]);
    let r = first_run(&parts);
    let item = r.items.iter().find(|i| i.pointer.ends_with("properties/0/value")).expect("an item");
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::PropertyRemoved)));
}

#[test]
fn the_gradient_mode_is_unsupported_unless_it_is_none() {
    let parts = Parts::histogram(json!({})).custom(json!({"gradientMode": "none"}));
    assert!(first_run(&parts).items.is_empty());
    for mode in ["opacity", "hue", "scheme"] {
        let parts = Parts::histogram(json!({})).custom(json!({"gradientMode": mode}));
        let r = first_run(&parts);
        let i = items(&r, "fieldConfig.defaults.custom.gradientMode");
        assert_eq!(
            (i.len(), i[0].class, i[0].fallback.clone()),
            (1, Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{mode}"
        );
        every_form(&parts, |form, p, _| assert_eq!(p["options"], hist_defaults(), "{form} {mode}"));
    }
}

#[test]
fn the_custom_members_of_a_histogram_that_belong_to_a_time_series_are_dropped() {
    let parts =
        Parts::histogram(json!({})).custom(json!({"drawStyle": "bars", "showPoints": "always", "axisSoftMin": 3}));
    let r = first_run(&parts);
    for member in ["drawStyle", "showPoints", "axisSoftMin"] {
        let i = items(&r, &format!("fieldConfig.defaults.custom.{member}"));
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}: {:#?}", r.items);
    }
}

#[test]
fn the_field_members_of_the_two_panels_map_as_the_rows_of_the_fields() {
    for ptype in ["heatmap", "histogram"] {
        let mut parts = if ptype == "heatmap" { Parts::heatmap(json!({})) } else { Parts::histogram(json!({})) };
        parts.defaults = json!({
            "unit": "percent", "decimals": 2, "min": 0, "max": 100, "displayName": "Count", "noValue": "-",
            "color": {"mode": "palette-classic"},
            "thresholds": {"mode": "absolute", "steps": [{"color": "green", "value": null}, {"color": "red", "value": 80}]},
            "mappings": [{"type": "value", "options": {"1": {"text": "One"}}}],
            "links": [{"title": "Docs", "url": "https://docs.example"}],
            "custom": {"hideFrom": {"legend": true, "tooltip": true, "viz": false}}
        });
        every_form(&parts, |form, p, r| {
            let d = &p["fields"]["defaults"];
            assert_eq!(
                (d["unit"].clone(), d["decimals"].clone(), d["min"].clone(), d["max"].clone()),
                (json!("percent"), json!(2), json!(0), json!(100)),
                "{ptype} {form}"
            );
            assert_eq!(d["display_name"], "Count", "{ptype} {form}");
            assert_eq!(d["hide_from"], json!({"legend": true, "tooltip": true, "viz": false}), "{ptype} {form}");
            assert!(r.items.is_empty(), "{ptype} {form}: {:#?}", r.items);
        });
    }
}

// ------------------------------------------------------------------ the 8 measured dashboards (15.14c)

const FIXTURES: [&str; 8] =
    ["heatmap_a", "heatmap_b", "heatmap_c", "heatmap_d", "heatmap_e", "histogram_f", "heatmap_g", "histogram_h"];

/// A fixture as the file has it: each panel has its own row (4.14) and the range is two epoch times (4.4).
fn fixture(name: &str) -> Value {
    read_json(&repo_root().join(format!("tests/fixtures/grafana/{name}.json")))
}

#[test]
fn the_eight_measured_dashboards_import_with_every_panel_but_the_removed_ones() {
    let mut total = 0;
    let mut classes = std::collections::BTreeSet::new();
    for name in FIXTURES {
        let d = fixture(name);
        let want = d["panels"].as_array().unwrap().len();
        let r = run(&d);
        let out = doc(&r);
        let got: usize =
            out["sections"].as_array().unwrap().iter().map(|s| s["panels"].as_array().unwrap().len()).sum();
        assert_eq!(got, want, "{name}: no panel is removed; {:#?}", r.items);
        for s in out["sections"].as_array().unwrap() {
            for p in s["panels"].as_array().unwrap() {
                assert!(matches!(p["type"].as_str(), Some("heatmap" | "histogram")), "{name}");
            }
        }
        total += got;
        classes.extend(run_plain(&d).items.iter().map(|i| (i.class, i.member.clone())));
    }
    assert_eq!(total, 60);
    // The rows that the fixtures reach: the report holds the items of the rows of 15.6.
    let has = |c: Class, m: &str| classes.iter().any(|(cl, mem)| *cl == c && mem == m);
    assert!(has(Class::Unsupported, "options.tooltip.yHistogram"), "{classes:?}");
    assert!(has(Class::Unsupported, "options.tooltip.showColorScale"), "{classes:?}");
    assert!(has(Class::Changed, "options.rowsFrame.layout"), "{classes:?}");
    assert!(has(Class::Changed, "options.yAxis.axisPlacement"), "{classes:?}");
    assert!(has(Class::Dropped, "options.yAxis.axisWidth"), "{classes:?}");
    assert!(has(Class::Dropped, "options.cellValues.unit"), "{classes:?}");
    assert!(has(Class::Unsupported, "fieldConfig.defaults.custom.gradientMode"), "{classes:?}");
}

#[test]
fn the_measured_options_land_in_the_document() {
    let panels_of = |name: &str| -> Vec<Value> {
        let out = doc(&run(&fixture(name)));
        out["sections"].as_array().unwrap().iter().flat_map(|s| s["panels"].as_array().unwrap().clone()).collect()
    };
    let b = panels_of("heatmap_b");
    let opts = |id: i64| b.iter().find(|p| p["id"] == id).map(|p| p["options"].clone()).unwrap();
    assert_eq!(opts(1)["calculate"], true);
    assert_eq!(
        (opts(2)["x_buckets"].clone(), opts(2)["y_buckets"].clone()),
        (json!({"mode": "count", "value": 10}), json!({"mode": "count", "value": 5, "scale": "linear"}))
    );
    assert_eq!(
        (opts(3)["x_buckets"].clone(), opts(3)["y_buckets"].clone()),
        (json!({"mode": "size", "value": 600_000}), json!({"mode": "size", "value": 20, "scale": "linear"}))
    );
    assert_eq!(opts(4)["y_buckets"], json!({"mode": "size", "value": 1, "scale": "log2"}));
    assert_eq!(opts(5)["y_buckets"], json!({"mode": "size", "value": 1, "scale": "log10"}));
    assert_eq!(opts(6)["y_buckets"], json!({"mode": "size", "value": 2, "scale": "log2"}));
    assert_eq!(opts(7)["color"]["mode"], "opacity");
    assert_eq!((opts(9)["color"]["scheme"].clone(), opts(9)["color"]["steps"].clone()), (json!("viridis"), json!(5)));
    assert_eq!((opts(11)["color"]["min"].clone(), opts(11)["color"]["max"].clone()), (json!(50), json!(100)));
    assert_eq!(opts(12)["cell_gap"], 0);
    let c = panels_of("heatmap_c");
    let opts = |id: i64| c.iter().find(|p| p["id"] == id).map(|p| p["options"].clone()).unwrap();
    assert_eq!(opts(1)["tooltip"], json!({"mode": "single"}));
    assert_eq!(opts(2)["tooltip"], json!({"mode": "multi"}));
    assert_eq!((opts(3)["tooltip"].clone(), opts(3)["show_legend"].clone()), (json!({"mode": "hidden"}), json!(false)));
    assert_eq!((opts(5)["hide_le"].clone(), opts(5)["hide_ge"].clone()), (json!(100), json!(400)));
    assert_eq!((opts(6)["y_min"].clone(), opts(6)["y_max"].clone()), (json!(50), json!(300)));
    assert_eq!(opts(7)["y_axis"], "left");
    assert_eq!(opts(8)["y_axis"], "hidden");
    let c4 = c.iter().find(|p| p["id"] == 4).unwrap();
    assert_eq!(c4["fields"]["defaults"], json!({"unit": "ms"}));
    let f = panels_of("histogram_f");
    let opts = |id: i64| f.iter().find(|p| p["id"] == id).map(|p| p["options"].clone()).unwrap();
    assert_eq!(opts(6)["bucket_size"], json!(0.01));
    assert_eq!(
        (opts(7)["stacking"].clone(), opts(7)["line_width"].clone(), opts(7)["fill_opacity"].clone()),
        (json!("normal"), json!(3), json!(30))
    );
    assert_eq!(opts(8)["legend"]["placement"], "right");
    assert_eq!(opts(10)["stacking"], "percent");
    let h = panels_of("histogram_h");
    let opts = |id: i64| h.iter().find(|p| p["id"] == id).map(|p| p["options"].clone()).unwrap();
    assert_eq!(opts(3)["combine"], true);
    assert_eq!((opts(5)["bucket_size"].clone(), opts(5)["bucket_offset"].clone()), (json!(25), json!(0)));
    assert_eq!((opts(6)["bucket_size"].clone(), opts(6)["bucket_offset"].clone()), (json!(25), json!(10)));
}

#[test]
fn the_fixtures_have_a_place_for_each_panel_and_a_valid_range() {
    for name in FIXTURES {
        let d = fixture(name);
        assert_eq!(d["uid"], name, "the file stem is the uid (15.4)");
        let ys: Vec<i64> =
            d["panels"].as_array().unwrap().iter().map(|p| p["gridPos"]["y"].as_i64().unwrap()).collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{name}: one row for each panel");
        let r = run_plain(&d);
        assert!(
            r.items
                .iter()
                .all(|i| !i.member.starts_with("time.") && i.fallback != Some(Fallback::DashboardNotImported)),
            "{name}: {:#?}",
            r.items
        );
        let accepted = run(&d);
        assert!(accepted.document_bytes.is_some(), "{name}: {:?}", accepted.result);
        assert_eq!(d["time"], json!({"from": "1794528000000", "to": "1794546000000"}), "{name}");
    }
}
