//! The rows of the state timeline and the status history in 15.6 (4.33b): one test for each new row in the form
//! v1 and in the forms v2, the boundaries of `rowHeight` and `colWidth`, and the members that make items.

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
    fn timeline(options: Value) -> Parts {
        Parts { ptype: "state-timeline", options, defaults: json!({}), overrides: json!([]) }
    }

    fn history(options: Value) -> Parts {
        Parts { ptype: "status-history", options, defaults: json!({}), overrides: json!([]) }
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
        "spec": {"title": "Timeline", "description": "", "tags": [], "cursorSync": "Off",
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

/// Runs a check in the four forms. The check gets the options of the panel and the items of the first run.
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

fn timeline_defaults() -> Value {
    json!({
        "merge_values": true, "show_values": "auto", "align_values": "left", "row_height": 90, "line_width": 0,
        "fill_opacity": 70, "span_nulls": false, "show_labels": true,
        "legend": {"show": true, "placement": "bottom", "mode": "list", "values": []},
        "tooltip": {"mode": "single"}
    })
}

fn history_defaults() -> Value {
    json!({
        "show_values": "auto", "row_height": 90, "col_width": 90, "line_width": 1, "fill_opacity": 70,
        "show_labels": true,
        "legend": {"show": true, "placement": "bottom", "mode": "list", "values": []},
        "tooltip": {"mode": "single"}
    })
}

// ------------------------------------------------------------------ the type and the defaults

#[test]
fn the_two_ids_map_to_the_two_types_with_the_defaults_in_each_form() {
    every_form(&Parts::timeline(json!({})), |form, p, r| {
        assert_eq!(p["type"], "state_timeline", "{form}");
        assert_eq!(p["options"], timeline_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    every_form(&Parts::history(json!({})), |form, p, r| {
        assert_eq!(p["type"], "status_history", "{form}");
        assert_eq!(p["options"], history_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

#[test]
fn the_default_options_of_grafana_make_no_item() {
    let opts = json!({
        "legend": {"calcs": [], "displayMode": "list", "placement": "bottom", "showLegend": true},
        "tooltip": {"mode": "single", "sort": "none"}, "mergeValues": true, "showValue": "auto",
        "alignValue": "left", "rowHeight": 0.9
    });
    let parts = Parts::timeline(opts).custom(json!({
        "fillOpacity": 70, "lineWidth": 0, "spanNulls": false, "insertNulls": false,
        "hideFrom": {"legend": false, "tooltip": false, "viz": false}, "axisPlacement": "auto"
    }));
    every_form(&parts, |form, p, r| {
        assert_eq!(p["options"], timeline_defaults(), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

// ------------------------------------------------------------------ mergeValues, showValue, alignValue

#[test]
fn merge_values_maps_in_a_timeline_and_is_metadata_in_a_history() {
    for value in [true, false] {
        every_form(&Parts::timeline(json!({"mergeValues": value})), |form, p, r| {
            assert_eq!(p["options"]["merge_values"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    every_form(&Parts::history(json!({"mergeValues": false})), |form, p, r| {
        assert_eq!(p["options"], history_defaults(), "{form}: the member is metadata");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    let r = first_run(&Parts::timeline(json!({"mergeValues": "yes"})));
    let i = items(&r, "options.mergeValues");
    assert_eq!(i.len(), 1, "{:#?}", r.items);
    assert_eq!(
        (i[0].class, i[0].fallback.clone()),
        (Class::Unsupported, Some(Fallback::other("the Grafana default of that option")))
    );
}

#[test]
fn show_value_and_align_value_map() {
    for value in ["auto", "always", "never"] {
        every_form(&Parts::timeline(json!({"showValue": value})), |form, p, _| {
            assert_eq!(p["options"]["show_values"], value, "{form}");
        });
        every_form(&Parts::history(json!({"showValue": value})), |form, p, _| {
            assert_eq!(p["options"]["show_values"], value, "{form}");
        });
    }
    for value in ["left", "center", "right"] {
        every_form(&Parts::timeline(json!({"alignValue": value})), |form, p, r| {
            assert_eq!(p["options"]["align_values"], value, "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let r = first_run(&Parts::timeline(json!({"showValue": "sometimes", "alignValue": "middle"})));
    assert_eq!(items(&r, "options.showValue")[0].class, Class::Unsupported);
    assert_eq!(items(&r, "options.alignValue")[0].class, Class::Unsupported);
}

#[test]
fn align_value_is_not_a_member_of_a_status_history() {
    let r = first_run(&Parts::history(json!({"alignValue": "center"})));
    let i = items(&r, "options.alignValue");
    assert_eq!(i.len(), 1, "{:#?}", r.items);
    assert_eq!((i[0].class, i[0].reason.as_str()), (Class::Unsupported, "unknown member"));
}

// ------------------------------------------------------------------ rowHeight and colWidth

#[test]
fn row_height_and_col_width_map_at_the_boundaries() {
    let table: [(f64, i64); 9] =
        [(0.1, 10), (0.2, 20), (0.25, 25), (0.333, 33), (0.5, 50), (0.9, 90), (0.995, 100), (1.0, 100), (0.105, 11)];
    for (input, want) in table {
        every_form(&Parts::timeline(json!({"rowHeight": input})), |form, p, r| {
            assert_eq!(p["options"]["row_height"], want, "{form} {input}");
            assert!(r.items.is_empty(), "{form} {input}");
        });
        every_form(&Parts::history(json!({"rowHeight": input, "colWidth": input})), |form, p, r| {
            assert_eq!(
                (p["options"]["row_height"].clone(), p["options"]["col_width"].clone()),
                (json!(want), json!(want)),
                "{form} {input}"
            );
            assert!(r.items.is_empty(), "{form} {input}");
        });
    }
}

#[test]
fn a_row_height_or_a_col_width_out_of_range_is_unsupported_with_the_fallback_90() {
    for bad in [json!(0), json!(0.05), json!(0.049), json!(1.01), json!(2), json!(-0.5), json!("0.5")] {
        let parts = Parts::timeline(json!({"rowHeight": bad}));
        let r = first_run(&parts);
        let i = items(&r, "options.rowHeight");
        assert_eq!(i.len(), 1, "{bad}: {:#?}", r.items);
        assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::other("90"))), "{bad}");
        for form in FORMS {
            assert_eq!(first_panel(&doc(&import(form, &parts)))["options"]["row_height"], 90, "{form} {bad}");
        }
        let parts = Parts::history(json!({"colWidth": bad}));
        let r = first_run(&parts);
        let i = items(&r, "options.colWidth");
        assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::other("90"))), "{bad}");
        for form in FORMS {
            assert_eq!(first_panel(&doc(&import(form, &parts)))["options"]["col_width"], 90, "{form} {bad}");
        }
    }
}

#[test]
fn col_width_is_not_a_member_of_a_state_timeline() {
    let r = first_run(&Parts::timeline(json!({"colWidth": 0.5})));
    assert_eq!(items(&r, "options.colWidth")[0].reason, "unknown member");
}

// ------------------------------------------------------------------ perPage, insertNulls, spanNulls

#[test]
fn per_page_make_an_item_in_both_types() {
    for parts in [Parts::timeline(json!({"perPage": 2})), Parts::history(json!({"perPage": 5}))] {
        let r = first_run(&parts);
        let i = items(&r, "options.perPage");
        assert_eq!(i.len(), 1, "{:#?}", r.items);
        assert_eq!(i[0].class, Class::Unsupported);
        assert_eq!(i[0].fallback, Some(Fallback::other("the panel shows all the rows")));
        every_form(&parts, |form, p, _| {
            assert!(p["options"].get("per_page").is_none() && p["options"].get("perPage").is_none(), "{form}");
        });
    }
}

#[test]
fn insert_nulls_is_metadata_when_off_and_unsupported_when_on() {
    for off in [json!(false), json!(0)] {
        let r = first_run(&Parts::timeline(json!({})).custom(json!({"insertNulls": off})));
        assert!(r.items.is_empty(), "{:#?}", r.items);
    }
    for on in [json!(true), json!(1_200_000)] {
        for parts in [Parts::timeline(json!({})), Parts::history(json!({}))] {
            let parts = parts.custom(json!({"insertNulls": on}));
            let r = first_run(&parts);
            let i = items(&r, "fieldConfig.defaults.custom.insertNulls");
            assert_eq!(i.len(), 1, "{on}: {:#?}", r.items);
            assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::MemberRemoved)));
            every_form(&parts, |form, p, _| {
                assert_eq!(
                    p["options"].get("span_nulls"),
                    parts.ptype.eq("state-timeline").then_some(&json!(false)),
                    "{form}"
                );
            });
        }
    }
}

#[test]
fn span_nulls_maps_in_a_timeline_and_a_number_is_unsupported() {
    for value in [true, false] {
        every_form(&Parts::timeline(json!({})).custom(json!({"spanNulls": value})), |form, p, r| {
            assert_eq!(p["options"]["span_nulls"], json!(value), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
    let parts = Parts::timeline(json!({})).custom(json!({"spanNulls": 1_200_000}));
    let r = first_run(&parts);
    let i = items(&r, "fieldConfig.defaults.custom.spanNulls");
    assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::other("false"))));
    every_form(&parts, |form, p, _| assert_eq!(p["options"]["span_nulls"], false, "{form}"));
}

#[test]
fn span_nulls_is_metadata_in_a_status_history() {
    for value in [json!(true), json!(false), json!(5)] {
        every_form(&Parts::history(json!({})).custom(json!({"spanNulls": value})), |form, p, r| {
            assert_eq!(p["options"], history_defaults(), "{form}");
            assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
        });
    }
}

// ------------------------------------------------------------------ lineWidth, fillOpacity, axisPlacement

#[test]
fn line_width_and_fill_opacity_map_with_their_ranges() {
    for (w, f) in [(0, 0), (10, 100), (3, 30)] {
        every_form(&Parts::timeline(json!({})).custom(json!({"lineWidth": w, "fillOpacity": f})), |form, p, r| {
            assert_eq!(
                (p["options"]["line_width"].clone(), p["options"]["fill_opacity"].clone()),
                (json!(w), json!(f)),
                "{form}"
            );
            assert!(r.items.is_empty(), "{form}");
        });
        every_form(&Parts::history(json!({})).custom(json!({"lineWidth": w, "fillOpacity": f})), |form, p, r| {
            assert_eq!(
                (p["options"]["line_width"].clone(), p["options"]["fill_opacity"].clone()),
                (json!(w), json!(f)),
                "{form}"
            );
            assert!(r.items.is_empty(), "{form}");
        });
    }
    for (parts, line, fill) in [(Parts::timeline(json!({})), 0, 70), (Parts::history(json!({})), 1, 70)] {
        let parts = parts.custom(json!({"lineWidth": 11, "fillOpacity": 101}));
        let r = first_run(&parts);
        for member in ["fieldConfig.defaults.custom.lineWidth", "fieldConfig.defaults.custom.fillOpacity"] {
            let i = items(&r, member);
            assert_eq!(i.len(), 1, "{member}: {:#?}", r.items);
            assert_eq!(
                (i[0].class, i[0].fallback.clone()),
                (Class::Unsupported, Some(Fallback::other("the default of the type")))
            );
        }
        every_form(&parts, |form, p, _| {
            assert_eq!(
                (p["options"]["line_width"].clone(), p["options"]["fill_opacity"].clone()),
                (json!(line), json!(fill)),
                "{form}"
            );
        });
    }
}

#[test]
fn line_width_in_an_override_is_unsupported_with_the_fallback_property_removed() {
    let mut parts = Parts::timeline(json!({}));
    parts.overrides = json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.lineWidth", "value": 2}, {"id": "custom.fillOpacity", "value": 50}, {"id": "unit", "value": "ms"}]}]);
    let r = first_run(&parts);
    for (member, pointer) in [("lineWidth", "properties/0/value"), ("fillOpacity", "properties/1/value")] {
        let i: Vec<_> = r.items.iter().filter(|i| i.pointer.ends_with(pointer)).collect();
        assert_eq!(i.len(), 1, "{member}: {:#?}", r.items);
        assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::PropertyRemoved)));
    }
    every_form(&parts, |form, p, _| {
        assert_eq!(p["fields"]["overrides"][0]["set"], json!({"unit": "ms"}), "{form}");
        assert_eq!(p["options"], timeline_defaults(), "{form}");
    });
}

#[test]
fn axis_placement_gives_show_labels() {
    for ptype in ["state-timeline", "status-history"] {
        let base = if ptype == "state-timeline" { Parts::timeline(json!({})) } else { Parts::history(json!({})) };
        every_form(&base.clone().custom(json!({"axisPlacement": "hidden"})), |form, p, r| {
            assert_eq!(p["options"]["show_labels"], false, "{ptype} {form}");
            assert!(r.items.is_empty(), "{ptype} {form}: {:#?}", r.items);
        });
        every_form(&base.clone().custom(json!({"axisPlacement": "auto"})), |form, p, r| {
            assert_eq!(p["options"]["show_labels"], true, "{ptype} {form}");
            assert!(r.items.is_empty(), "{ptype} {form}");
        });
        let bad = base.custom(json!({"axisPlacement": "left"}));
        let r = first_run(&bad);
        let i = items(&r, "fieldConfig.defaults.custom.axisPlacement");
        assert_eq!((i[0].class, i[0].fallback.clone()), (Class::Unsupported, Some(Fallback::other("true"))), "{ptype}");
        every_form(&bad, |form, p, _| assert_eq!(p["options"]["show_labels"], true, "{ptype} {form}"));
    }
}

#[test]
fn the_style_members_of_a_time_series_are_dropped_or_metadata() {
    let parts = Parts::timeline(json!({})).custom(json!({
        "drawStyle": "line", "lineInterpolation": "linear", "showPoints": "auto", "pointSize": 5,
        "stacking": {"mode": "none", "group": "A"}, "barAlignment": 0
    }));
    let r = first_run(&parts);
    assert!(r.items.is_empty(), "the Grafana defaults are metadata: {:#?}", r.items);
    let parts =
        Parts::history(json!({})).custom(json!({"drawStyle": "bars", "showPoints": "always", "axisLabel": "x"}));
    let r = first_run(&parts);
    assert_eq!(r.items.len(), 3, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.class == Class::Dropped), "{:#?}", r.items);
    every_form(&parts, |form, p, _| assert_eq!(p["options"], history_defaults(), "{form}"));
}

// ------------------------------------------------------------------ the legend

#[test]
fn the_legend_maps_show_and_placement() {
    let legend = json!({"showLegend": false, "placement": "right", "displayMode": "list", "calcs": []});
    every_form(&Parts::timeline(json!({"legend": legend})), |form, p, r| {
        assert_eq!(
            p["options"]["legend"],
            json!({"show": false, "placement": "right", "mode": "list", "values": []}),
            "{form}"
        );
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
    every_form(&Parts::history(json!({"legend": {"displayMode": "hidden"}})), |form, p, _| {
        assert_eq!(p["options"]["legend"]["show"], false, "{form}");
    });
}

#[test]
fn the_table_mode_of_the_legend_is_changed_to_list() {
    let parts = Parts::timeline(
        json!({"legend": {"displayMode": "table", "placement": "bottom", "showLegend": true, "calcs": []}}),
    );
    let r = first_run(&parts);
    let i = items(&r, "options.legend.displayMode");
    assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{:#?}", r.items);
    every_form(&parts, |form, p, _| assert_eq!(p["options"]["legend"]["mode"], "list", "{form}"));
}

#[test]
fn calcs_sort_by_and_sort_desc_of_the_legend_are_unsupported_with_the_fallback_member_removed() {
    let parts = Parts::history(json!({"legend": {"calcs": ["max"], "sortBy": "Name", "sortDesc": true,
        "displayMode": "table", "placement": "bottom", "showLegend": true}}));
    let r = first_run(&parts);
    for member in ["options.legend.calcs", "options.legend.sortBy", "options.legend.sortDesc"] {
        let i = items(&r, member);
        assert_eq!(i.len(), 1, "{member}: {:#?}", r.items);
        assert_eq!(
            (i[0].class, i[0].fallback.clone()),
            (Class::Unsupported, Some(Fallback::MemberRemoved)),
            "{member}"
        );
    }
    every_form(&parts, |form, p, _| {
        assert_eq!(
            p["options"]["legend"],
            json!({"show": true, "placement": "bottom", "mode": "list", "values": []}),
            "{form}"
        );
    });
    // An empty list and a sort that is off are metadata.
    let r = first_run(&Parts::history(json!({"legend": {"calcs": [], "sortDesc": false}})));
    assert!(r.items.is_empty(), "{:#?}", r.items);
}

// ------------------------------------------------------------------ the tooltip

#[test]
fn the_tooltip_mode_maps() {
    for (input, want) in [("single", "single"), ("multi", "multi"), ("none", "hidden")] {
        every_form(&Parts::timeline(json!({"tooltip": {"mode": input, "sort": "none"}})), |form, p, r| {
            assert_eq!(p["options"]["tooltip"], json!({"mode": want}), "{form}");
            assert!(r.items.is_empty(), "{form}");
        });
    }
}

#[test]
fn a_tooltip_sort_and_hide_zeros_are_changed() {
    for sort in ["asc", "desc"] {
        let parts = Parts::timeline(json!({"tooltip": {"mode": "multi", "sort": sort, "hideZeros": true}}));
        let r = first_run(&parts);
        for member in ["options.tooltip.sort", "options.tooltip.hideZeros"] {
            let i = items(&r, member);
            assert_eq!((i.len(), i[0].class), (1, Class::Changed), "{sort} {member}: {:#?}", r.items);
        }
        every_form(&parts, |form, p, _| assert_eq!(p["options"]["tooltip"], json!({"mode": "multi"}), "{form}"));
    }
    let r = first_run(&Parts::history(json!({"tooltip": {"mode": "single", "sort": "none", "hideZeros": false}})));
    assert!(r.items.is_empty(), "{:#?}", r.items);
}

#[test]
fn max_height_and_max_width_of_the_tooltip_are_listed_when_set() {
    let r = first_run(&Parts::timeline(json!({"tooltip": {"mode": "single", "maxHeight": 600, "maxWidth": 300}})));
    for member in ["options.tooltip.maxHeight", "options.tooltip.maxWidth"] {
        assert_eq!(items(&r, member).len(), 1, "{member}: {:#?}", r.items);
    }
    let r = first_run(&Parts::timeline(json!({"tooltip": {"mode": "single"}})));
    assert!(r.items.is_empty(), "{:#?}", r.items);
}

#[test]
fn timezone_and_annotations_are_metadata_when_empty_and_dropped_when_set() {
    let r = first_run(&Parts::timeline(json!({"timezone": [], "annotations": {"clustering": -1, "multiLane": false}})));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    let r = first_run(&Parts::history(json!({"timezone": ["America/New_York"], "annotations": {"multiLane": true}})));
    for member in ["options.timezone", "options.annotations"] {
        let i = items(&r, member);
        assert_eq!((i.len(), i[0].class), (1, Class::Dropped), "{member}: {:#?}", r.items);
    }
}

// ------------------------------------------------------------------ the field properties

#[test]
fn the_field_properties_of_4_18_map_as_for_a_time_series() {
    let mut parts = Parts::timeline(json!({}));
    parts.defaults = json!({
        "unit": "percent", "decimals": 1, "min": 0, "max": 100,
        "color": {"mode": "thresholds"},
        "thresholds": {"mode": "absolute", "steps": [{"color": "green", "value": null}, {"color": "red", "value": 80}]},
        "mappings": [{"type": "value", "options": {"0": {"text": "OK", "color": "green", "index": 0}}}],
        "displayName": "State", "noValue": "-",
        "custom": {"hideFrom": {"legend": false, "tooltip": true, "viz": false}}
    });
    every_form(&parts, |form, p, r| {
        let d = &p["fields"]["defaults"];
        assert_eq!(
            (d["unit"].clone(), d["decimals"].clone(), d["min"].clone(), d["max"].clone()),
            (json!("percent"), json!(1), json!(0), json!(100)),
            "{form}"
        );
        assert_eq!(d["color"]["mode"], "thresholds", "{form}");
        assert_eq!(d["thresholds"]["steps"][1], json!({"value": 80, "color": "red"}), "{form}");
        assert_eq!(d["mappings"][0], json!({"kind": "value", "value": "0", "text": "OK", "color": "green"}), "{form}");
        assert_eq!((d["display_name"].clone(), d["no_value"].clone()), (json!("State"), json!("-")), "{form}");
        assert_eq!(d["hide_from"], json!({"legend": false, "tooltip": true, "viz": false}), "{form}");
        assert!(r.items.is_empty(), "{form}: {:#?}", r.items);
    });
}

#[test]
fn an_absent_color_stays_absent_so_the_page_takes_the_default_of_the_type() {
    for parts in [Parts::timeline(json!({})), Parts::history(json!({}))] {
        every_form(&parts, |form, p, _| assert!(p["fields"]["defaults"].get("color").is_none(), "{form}"));
    }
}

#[test]
fn a_panel_of_the_two_types_is_not_removed_and_other_types_are_still_unknown() {
    let r = first_run(&Parts { ptype: "state-timelines", ..Parts::timeline(json!({})) });
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved)), "{:#?}", r.items);
    assert!(r.items.iter().any(|i| i.reason.contains("is not a type of 4.13")), "{:#?}", r.items);
}

#[test]
fn the_other_two_forms_of_the_types_read_back_to_the_same_members() {
    let options = json!({"mergeValues": false, "showValue": "never", "alignValue": "right", "rowHeight": 0.5,
        "legend": {"showLegend": true, "placement": "right", "displayMode": "list", "calcs": []},
        "tooltip": {"mode": "multi", "sort": "none"}});
    let parts = Parts::timeline(options)
        .custom(json!({"lineWidth": 2, "fillOpacity": 30, "spanNulls": true, "axisPlacement": "hidden"}));
    let raw = first_panel(&doc(&import("raw", &parts))).clone();
    let keep = |p: &Value| json!([p["type"], p["options"], p["fields"], p["grid"]]);
    for form in ["wrapped", "alpha1", "beta1"] {
        assert_eq!(keep(first_panel(&doc(&import(form, &parts)))), keep(&raw), "{form}");
    }
    assert_eq!(
        raw["options"],
        json!({
            "merge_values": false, "show_values": "never", "align_values": "right", "row_height": 50, "line_width": 2,
            "fill_opacity": 30, "span_nulls": true, "show_labels": false,
            "legend": {"show": true, "placement": "right", "mode": "list", "values": []},
            "tooltip": {"mode": "multi"}
        })
    );
}

// ------------------------------------------------------------------ the 8 measured dashboards (15.14c)

const FIXTURES: [&str; 8] = ["a", "b", "c", "e", "f", "g", "h", "i"];

/// A fixture as the file has it: each panel has its own row (4.14) and the range is two epoch times (4.4).
fn fixture(name: &str) -> Value {
    read_json(&repo_root().join(format!("tests/fixtures/grafana/timelines_{name}.json")))
}

#[test]
fn the_eight_measured_dashboards_import_with_every_panel() {
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
                assert!(matches!(p["type"].as_str(), Some("state_timeline" | "status_history")), "{name}");
            }
        }
        total += got;
        classes.extend(run_plain(&d).items.iter().map(|i| (i.class, i.member.clone())));
    }
    assert_eq!(total, 102);
    // The rows that the fixtures reach: the report holds the items of the rows of 15.6.
    let has = |c: Class, m: &str| classes.iter().any(|(cl, mem)| *cl == c && mem == m);
    assert!(has(Class::Unsupported, "options.perPage"), "{classes:?}");
    assert!(has(Class::Changed, "options.legend.displayMode"), "{classes:?}");
    assert!(has(Class::Changed, "options.tooltip.sort"), "{classes:?}");
    assert!(has(Class::Unsupported, "fieldConfig.defaults.custom.insertNulls"), "{classes:?}");
    assert!(has(Class::Unsupported, "fieldConfig.defaults.custom.spanNulls"), "{classes:?}");
    assert!(has(Class::Unsupported, "options.rowHeight"), "{classes:?}");
}

#[test]
fn the_measured_options_land_in_the_document() {
    let out = doc(&run(&fixture("b")));
    let panels: Vec<&Value> =
        out["sections"].as_array().unwrap().iter().flat_map(|s| s["panels"].as_array().unwrap()).collect();
    let opts = |id: i64| panels.iter().find(|p| p["id"] == id).map(|p| p["options"].clone()).unwrap();
    assert_eq!(opts(2)["row_height"], 50);
    assert_eq!(opts(3)["row_height"], 100);
    assert_eq!(opts(4)["row_height"], 20);
    assert_eq!(opts(5)["row_height"], 90, "rowHeight 0 is unsupported: the fallback is 90");
    assert_eq!((opts(6)["line_width"].clone(), opts(6)["fill_opacity"].clone()), (json!(2), json!(30)));
    assert_eq!((opts(8)["merge_values"].clone(), opts(10)["show_values"].clone()), (json!(false), json!("always")));
    assert_eq!((opts(12)["align_values"].clone(), opts(13)["align_values"].clone()), (json!("center"), json!("right")));
    let out = doc(&run(&fixture("i")));
    for p in out["sections"][0]["panels"].as_array().unwrap() {
        assert_eq!(p["options"]["show_labels"], false);
    }
}

#[test]
fn the_fixtures_have_a_place_for_each_panel_and_a_valid_range() {
    for name in FIXTURES {
        let d = fixture(name);
        assert_eq!(d["uid"], format!("timelines_{name}"), "the file stem is the uid (15.4)");
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

#[test]
fn the_fixtures_list_per_page_and_insert_nulls() {
    let all: Vec<ReportItem> = FIXTURES.iter().flat_map(|n| run_plain(&fixture(n)).items).collect();
    let per_page: Vec<_> = all.iter().filter(|i| i.member == "options.perPage").collect();
    assert_eq!(per_page.len(), 1, "{all:#?}");
    assert_eq!(per_page[0].class, Class::Unsupported);
    let insert: Vec<_> = all.iter().filter(|i| i.member == "fieldConfig.defaults.custom.insertNulls").collect();
    assert_eq!(insert.len(), 2, "{all:#?}");
    assert!(insert.iter().all(|i| i.class == Class::Unsupported && i.fallback == Some(Fallback::MemberRemoved)));
}
