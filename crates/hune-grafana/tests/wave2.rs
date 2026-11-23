// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Grafana-parity work, the second set: the importer rows of 15.6 for format 3 (cases I30, I31 and I32). Each case
//! runs in the four input forms: v1 raw, v1 wrapped, v2alpha1 and v2beta1.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback, import_dashboard};
use serde_json::{Value, json};

const FORMS: [&str; 4] = ["raw", "wrapped", "alpha1", "beta1"];

// ------------------------------------------------------------------ the four forms

/// The parts of a test dashboard. The dashboard has one panel, and a second panel when `reuse` is set.
struct Parts {
    ptype: &'static str,
    timezone: Option<Value>,
    links: Option<Value>,
    defaults: Value,
    overrides: Value,
    /// The members of the panel that the form puts in `queryOptions` (v2) or in the panel (v1).
    query_options: Value,
    /// A second panel with the data source `-- Dashboard --` and the same query options.
    reuse: bool,
}

impl Parts {
    fn new(ptype: &'static str) -> Parts {
        Parts {
            ptype,
            timezone: None,
            links: None,
            defaults: json!({}),
            overrides: json!([]),
            query_options: json!({}),
            reuse: false,
        }
    }
}

fn v1(parts: &Parts) -> Value {
    let mut p = panel(1, parts.ptype, 0);
    p["fieldConfig"] = json!({"defaults": parts.defaults, "overrides": parts.overrides});
    for (k, v) in parts.query_options.as_object().into_iter().flatten() {
        p[k] = v.clone();
    }
    let mut panels = vec![p];
    if parts.reuse {
        let mut r = panel(2, "stat", 4);
        r["datasource"] = json!({"type": "datasource", "uid": "-- Dashboard --"});
        r["targets"] =
            json!([{"datasource": {"type": "datasource", "uid": "-- Dashboard --"}, "panelId": 1, "refId": "A"}]);
        for (k, v) in parts.query_options.as_object().into_iter().flatten() {
            r[k] = v.clone();
        }
        panels.push(r);
    }
    let mut d = dash(panels, vec![]);
    if let Some(t) = &parts.timezone {
        d["timezone"] = t.clone();
    }
    if let Some(l) = &parts.links {
        d["links"] = l.clone();
    }
    d
}

fn v2_query(form: &str, ds: Value, spec: Value) -> Value {
    if form == "alpha1" {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "grafana-clickhouse-datasource", "spec": spec},
            "datasource": ds, "refId": "A", "hidden": false}})
    } else {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "grafana-clickhouse-datasource",
            "version": "v0", "datasource": {"name": ds["uid"].as_str().unwrap_or("clickhouse")}, "spec": spec}, "refId": "A", "hidden": false}})
    }
}

fn v2_element(form: &str, id: i64, ptype: &str, parts: &Parts, reuse: bool) -> Value {
    let field_config = json!({"defaults": parts.defaults, "overrides": parts.overrides});
    let viz = if form == "alpha1" {
        json!({"kind": ptype, "spec": {"pluginVersion": "12.2.1", "options": {}, "fieldConfig": field_config}})
    } else {
        json!({"kind": "VizConfig", "group": ptype, "version": "12.2.1", "spec": {"options": {}, "fieldConfig": field_config}})
    };
    let query = if reuse {
        let spec = json!({"panelId": 1});
        if form == "alpha1" {
            json!({"kind": "PanelQuery", "spec": {"query": {"kind": "datasource", "spec": spec},
                "datasource": {"type": "datasource", "uid": "-- Dashboard --"}, "refId": "A", "hidden": false}})
        } else {
            json!({"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "datasource", "version": "v0",
                "datasource": {"name": "-- Dashboard --"}, "spec": spec}, "refId": "A", "hidden": false}})
        }
    } else {
        let spec = json!({"rawSql": "SELECT 1", "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"});
        v2_query(form, json!({"type": "grafana-clickhouse-datasource", "uid": "clickhouse"}), spec)
    };
    json!({"kind": "Panel", "spec": {"id": id, "title": format!("P{id}"), "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [query], "transformations": [], "queryOptions": parts.query_options}},
        "vizConfig": viz}})
}

fn v2(form: &str, parts: &Parts) -> Value {
    let mut elements = json!({"panel-1": v2_element(form, 1, parts.ptype, parts, false)});
    let mut items = vec![json!({"kind": "GridLayoutItem", "spec": {"x": 0, "y": 0, "width": 12, "height": 4,
        "element": {"kind": "ElementReference", "name": "panel-1"}}})];
    if parts.reuse {
        elements["panel-2"] = v2_element(form, 2, "stat", parts, true);
        items.push(json!({"kind": "GridLayoutItem", "spec": {"x": 0, "y": 4, "width": 12, "height": 4,
            "element": {"kind": "ElementReference", "name": "panel-2"}}}));
    }
    let mut time = json!({"from": "now-6h", "to": "now", "autoRefresh": ""});
    if let Some(t) = &parts.timezone {
        time["timezone"] = t.clone();
    }
    let mut d = json!({
        "apiVersion": format!("dashboard.grafana.app/v2{}", if form == "alpha1" { "alpha1" } else { "beta1" }),
        "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "Wave 2", "description": "", "tags": [], "cursorSync": "Off", "timeSettings": time,
            "variables": [], "elements": elements,
            "layout": {"kind": "GridLayout", "spec": {"items": items}}}
    });
    if let Some(l) = &parts.links {
        d["spec"]["links"] = l.clone();
    }
    d
}

fn in_form(form: &str, parts: &Parts) -> Value {
    match form {
        "raw" => v1(parts),
        "wrapped" => json!({"dashboard": v1(parts), "meta": {"slug": "t1"}}),
        other => v2(other, parts),
    }
}

/// Imports the dashboard in one form. The second run accepts the items of the first (see `run`).
fn import(form: &str, parts: &Parts) -> DashboardImport {
    run(&in_form(form, parts))
}

fn every_form(parts: &Parts, check: impl Fn(&str, &DashboardImport)) {
    for form in FORMS {
        check(form, &import(form, parts));
    }
}

fn items_of<'a>(r: &'a DashboardImport, member: &str) -> Vec<&'a hune_grafana::ReportItem> {
    r.items.iter().filter(|i| i.member == member || i.member.starts_with(&format!("{member}."))).collect()
}

// ------------------------------------------------------------------ I30: dashboard members

#[test]
fn i30_timezone() {
    let cases: [(Value, &str, Option<Class>); 9] = [
        (json!("Europe/Paris"), "Europe/Paris", None),
        (json!("America/Argentina/Buenos_Aires"), "America/Argentina/Buenos_Aires", None),
        (json!("browser"), "browser", None),
        (json!(""), "browser", None),
        (json!("utc"), "utc", None),
        (json!("UTC"), "utc", None),
        (json!("default"), "browser", Some(Class::Changed)),
        (json!("GMT"), "browser", Some(Class::Unsupported)),
        (json!("Europe/New York"), "browser", Some(Class::Unsupported)),
    ];
    for (input, expected, class) in cases {
        let mut parts = Parts::new("stat");
        parts.timezone = Some(input.clone());
        every_form(&parts, |form, r| {
            assert_eq!(doc(r)["time"]["timezone"], expected, "{form} {input}");
            let items: Vec<_> = r.items.iter().filter(|i| i.member.ends_with("timezone")).collect();
            assert_eq!(items.first().map(|i| i.class), class, "{form} {input}: {:#?}", r.items);
            if class == Some(Class::Unsupported) {
                assert_eq!(items[0].fallback, Some(Fallback::other("browser")), "{form} {input}");
            }
        });
    }
}

fn link_item(kind: &str, extra: Value) -> Value {
    let mut base = if kind == "link" {
        json!({"type": "link", "title": "Runbook", "url": "https://wiki.example.test/runbook"})
    } else {
        json!({"type": "dashboards", "title": "Related", "tags": ["chain"], "asDropdown": true})
    };
    for (k, v) in extra.as_object().into_iter().flatten() {
        base[k] = v.clone();
    }
    base
}

#[test]
fn i30_links_of_both_types_and_their_flags() {
    let mut parts = Parts::new("stat");
    parts.links = Some(json!([
        link_item(
            "link",
            json!({"targetBlank": true, "includeVars": true, "keepTime": false, "tooltip": "", "icon": "external link"})
        ),
        link_item(
            "dashboards",
            json!({"targetBlank": false, "includeVars": true, "keepTime": true, "tooltip": "", "icon": "dashboard", "url": ""})
        ),
        link_item("link", json!({"url": "/d/other", "title": "Other"})),
    ]));
    every_form(&parts, |form, r| {
        no_items(r);
        let links = doc(r)["links"].clone();
        assert_eq!(
            links,
            json!([
                {"kind": "link", "title": "Runbook", "url": "https://wiki.example.test/runbook",
                 "target_blank": true, "include_vars": true, "keep_time": false},
                {"kind": "dashboards", "title": "Related", "tags": ["chain"], "as_dropdown": true,
                 "target_blank": false, "include_vars": true, "keep_time": true},
                {"kind": "link", "title": "Other", "url": "/d/other",
                 "target_blank": false, "include_vars": false, "keep_time": false},
            ]),
            "{form}"
        );
    });
}

#[test]
fn i30_a_dashboard_without_links_has_an_empty_array() {
    every_form(&Parts::new("stat"), |form, r| {
        no_items(r);
        assert_eq!(doc(r)["links"], json!([]), "{form}");
    });
}

#[test]
fn i30_empty_titles_are_changed() {
    let mut parts = Parts::new("stat");
    parts.links = Some(json!([
        link_item("link", json!({"title": ""})),
        link_item("dashboards", json!({"title": "", "asDropdown": true})),
        link_item("dashboards", json!({"title": "", "asDropdown": false})),
    ]));
    every_form(&parts, |form, r| {
        let links = doc(r)["links"].clone();
        assert_eq!(links[0]["title"], "Link", "{form}");
        assert_eq!(links[1]["title"], "Dashboards", "{form}");
        // An empty title is valid without a dropdown (4.5a).
        assert_eq!(links[2]["title"], "", "{form}");
        assert_eq!(r.items.len(), 2, "{form}: {:#?}", r.items);
        assert!(r.items.iter().all(|i| i.class == Class::Changed), "{form}");
    });
}

#[test]
fn i30_tags_are_checked() {
    let nine: Vec<String> = (1..=9).map(|i| format!("t{i}")).collect();
    let eight: Vec<String> = (1..=8).map(|i| format!("t{i}")).collect();
    let mut parts = Parts::new("stat");
    parts.links = Some(json!([
        link_item("dashboards", json!({"tags": nine})),
        link_item("dashboards", json!({"tags": eight})),
        link_item("dashboards", json!({"tags": ["a", "b", "a"]})),
        link_item("dashboards", json!({"tags": [""]})),
        link_item("dashboards", json!({"tags": ["x".repeat(65)]})),
    ]));
    every_form(&parts, |form, r| {
        let links = doc(r)["links"].as_array().unwrap().clone();
        assert_eq!(links.len(), 2, "{form}");
        assert_eq!(links[0]["tags"].as_array().unwrap().len(), 8, "{form}");
        assert_eq!(links[1]["tags"], json!(["a", "b"]), "{form}");
        let unsupported: Vec<_> = r.items.iter().filter(|i| i.class == Class::Unsupported).collect();
        assert_eq!(unsupported.len(), 3, "{form}: {:#?}", r.items);
        assert!(unsupported.iter().all(|i| i.fallback == Some(Fallback::other("item removed"))), "{form}");
        assert_eq!(r.items.iter().filter(|i| i.class == Class::Changed).count(), 1, "{form}");
    });
}

#[test]
fn i30_more_than_sixteen_links_keep_the_first_sixteen() {
    let mut parts = Parts::new("stat");
    parts.links = Some(json!(
        (0..17)
            .map(|i| link_item("link", json!({"title": format!("L{i}"), "url": format!("/l{i}")})))
            .collect::<Vec<_>>()
    ));
    every_form(&parts, |form, r| {
        let links = doc(r)["links"].as_array().unwrap().clone();
        assert_eq!(links.len(), 16, "{form}");
        assert_eq!(links[15]["title"], "L15", "{form}");
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(
            (r.items[0].class, r.items[0].fallback.clone()),
            (Class::Unsupported, Some(Fallback::other("item removed")))
        );
    });
}

#[test]
fn i30_tooltip_and_icon_and_other_members() {
    let mut parts = Parts::new("stat");
    parts.links = Some(json!([
        link_item("link", json!({"tooltip": "Open it", "icon": "bolt"})),
        link_item("link", json!({"tags": ["a"]})),
        link_item("dashboards", json!({"url": "/x"})),
        link_item("link", json!({"type": "panel"})),
        link_item("link", json!({"url": "javascript:alert(1)"})),
        link_item("link", json!({"url": "//evil.test/x"})),
        link_item("link", json!({"includeVars": "maybe"})),
        link_item("link", json!({"zzz": 1})),
    ]));
    every_form(&parts, |form, r| {
        // The first item stays: the tooltip and the icon are dropped and listed.
        let links = doc(r)["links"].as_array().unwrap().clone();
        assert_eq!(links.len(), 1, "{form}: {links:?}");
        let dropped: Vec<_> = r.items.iter().filter(|i| i.class == Class::Dropped).collect();
        assert_eq!(dropped.len(), 2, "{form}: {:#?}", r.items);
        let unsupported = r.items.iter().filter(|i| i.class == Class::Unsupported).count();
        assert_eq!(unsupported, 7, "{form}: {:#?}", r.items);
    });
}

// ----------------------------------------------------- I30: the time override of a panel

fn query_of(r: &DashboardImport, panel: usize) -> Value {
    doc(r)["sections"][0]["panels"][panel]["data"]["query"].clone()
}

#[test]
fn i30_time_from_forms() {
    let cases = [
        ("6h", Some("now-6h")),
        ("30m", Some("now-30m")),
        ("2d", Some("now-2d")),
        ("1w", Some("now-1w")),
        ("3M", Some("now-3M")),
        ("1y", Some("now-1y")),
        ("90s", Some("now-90s")),
        ("now-6h", Some("now-6h")),
        ("now-5d/d", Some("now-5d/d")),
        ("now/d", Some("now/d")),
        ("now/w", Some("now/w")),
        ("", None),
    ];
    for (input, expected) in cases {
        let mut parts = Parts::new("timeseries");
        parts.query_options = json!({"timeFrom": input});
        every_form(&parts, |form, r| {
            assert_eq!(query_of(r, 0).get("time_from").and_then(Value::as_str), expected, "{form} {input}");
            no_items(r);
        });
    }
}

#[test]
fn i30_a_time_from_that_is_refused_is_removed() {
    for input in ["now", "now+1h", "${range}", "6H", "0h", "1.5h", "now-6h/", "now/Q", "abc", "1d/d"] {
        let mut parts = Parts::new("timeseries");
        parts.query_options = json!({"timeFrom": input});
        every_form(&parts, |form, r| {
            assert!(query_of(r, 0).get("time_from").is_none(), "{form} {input}");
            let item = items_of(r, "timeFrom");
            let item = if item.is_empty() { items_of(r, "queryOptions.timeFrom") } else { item };
            assert_eq!(item.len(), 1, "{form} {input}: {:#?}", r.items);
            assert_eq!((item[0].class, item[0].fallback.clone()), (Class::Unsupported, Some(Fallback::MemberRemoved)));
        });
    }
}

#[test]
fn i30_time_shift_forms() {
    for (input, expected) in
        [("1h", Some("1h")), ("2d", Some("2d")), ("1w", Some("1w")), ("1M", Some("1M")), ("", None)]
    {
        let mut parts = Parts::new("timeseries");
        parts.query_options = json!({"timeShift": input});
        every_form(&parts, |form, r| {
            assert_eq!(query_of(r, 0).get("time_shift").and_then(Value::as_str), expected, "{form} {input}");
            no_items(r);
        });
    }
    for input in ["1d/d", "now-1d", "-1h", "+1h", "${shift}", "x"] {
        let mut parts = Parts::new("timeseries");
        parts.query_options = json!({"timeShift": input});
        every_form(&parts, |form, r| {
            assert!(query_of(r, 0).get("time_shift").is_none(), "{form} {input}");
            assert_eq!(r.items.len(), 1, "{form} {input}: {:#?}", r.items);
            assert_eq!(r.items[0].class, Class::Unsupported, "{form} {input}");
        });
    }
}

#[test]
fn i30_hide_time_override() {
    let mut parts = Parts::new("timeseries");
    parts.query_options = json!({"timeFrom": "now-24h", "timeShift": "1d", "hideTimeOverride": true});
    every_form(&parts, |form, r| {
        let q = query_of(r, 0);
        assert_eq!(
            (q["time_from"].as_str(), q["time_shift"].as_str(), q["hide_time_override"].as_bool()),
            (Some("now-24h"), Some("1d"), Some(true)),
            "{form}"
        );
        no_items(r);
    });
    // `false` is metadata. The canonical form leaves the member out.
    let mut parts = Parts::new("timeseries");
    parts.query_options = json!({"hideTimeOverride": false});
    every_form(&parts, |form, r| {
        assert!(query_of(r, 0).get("hide_time_override").is_none(), "{form}");
        no_items(r);
        assert!(r.metadata > 0, "{form}");
    });
    // Another value is refused.
    let mut parts = Parts::new("timeseries");
    parts.query_options = json!({"hideTimeOverride": "yes"});
    every_form(&parts, |form, r| {
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(
            (r.items[0].class, r.items[0].fallback.clone()),
            (Class::Unsupported, Some(Fallback::MemberRemoved))
        );
    });
}

#[test]
fn i30_a_panel_with_the_dashboard_data_source_has_no_time_override() {
    let mut parts = Parts::new("timeseries");
    parts.reuse = true;
    parts.query_options = json!({"timeFrom": "6h", "timeShift": "1h", "hideTimeOverride": true});
    every_form(&parts, |form, r| {
        let out = doc(r);
        // The first panel keeps the three members. The reuse has none (4.15).
        assert_eq!(query_of(r, 0)["time_from"], "now-6h", "{form}");
        assert_eq!(
            out["sections"][0]["panels"][1]["data"],
            json!({"reuse": {"panel": 1, "with_transforms": false}}),
            "{form}"
        );
        assert_eq!(r.items.len(), 3, "{form}: {:#?}", r.items);
        assert!(
            r.items.iter().all(|i| i.class == Class::Unsupported && i.fallback == Some(Fallback::MemberRemoved)),
            "{form}"
        );
    });
}

// ------------------------------------------------------------------ I31: the properties

/// The three panel types of I31.
const TYPES: [&str; 3] = ["timeseries", "barchart", "table"];

fn override_of(property: &str, value: Value) -> Value {
    json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": property, "value": value}]}])
}

fn defaults_set(r: &DashboardImport) -> Value {
    doc(r)["sections"][0]["panels"][0]["fields"]["defaults"].clone()
}

fn override_set(r: &DashboardImport) -> Value {
    doc(r)["sections"][0]["panels"][0]["fields"]["overrides"][0]["set"].clone()
}

#[test]
fn i31_colour_modes() {
    let modes = [
        ("palette-classic", "palette"),
        ("continuous-GrYlRd", "continuous_gr_yl_rd"),
        ("continuous-RdYlGr", "continuous_rd_yl_gr"),
        ("continuous-BlYlRd", "continuous_bl_yl_rd"),
        ("continuous-YlRd", "continuous_yl_rd"),
        ("continuous-BlPu", "continuous_bl_pu"),
        ("continuous-YlBl", "continuous_yl_bl"),
        ("continuous-blues", "continuous_blues"),
        ("continuous-reds", "continuous_reds"),
        ("continuous-greens", "continuous_greens"),
        ("continuous-purples", "continuous_purples"),
    ];
    for ptype in TYPES {
        for (id, mode) in modes {
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"color": {"mode": id, "fixedColor": "green", "seriesBy": "last"}});
            parts.overrides = override_of("color", json!({"mode": id}));
            every_form(&parts, |form, r| {
                no_items(r);
                assert_eq!(defaults_set(r)["color"], json!({"mode": mode, "fixed": null}), "{form} {ptype} {id}");
                assert_eq!(override_set(r)["color"], json!({"mode": mode, "fixed": null}), "{form} {ptype} {id}");
            });
        }
        // Each Grafana palette mode gives the one series palette, with one `changed` item.
        for id in ["palette-classic-by-name", "palette-saturated"] {
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"color": {"mode": id, "fixedColor": "green", "seriesBy": "last"}});
            parts.overrides = override_of("color", json!({"mode": id}));
            every_form(&parts, |form, r| {
                assert_eq!(defaults_set(r)["color"], json!({"mode": "palette", "fixed": null}), "{form} {ptype} {id}");
                assert_eq!(override_set(r)["color"], json!({"mode": "palette", "fixed": null}), "{form} {ptype} {id}");
                assert_eq!(r.items.len(), 2, "{form} {ptype} {id}: {:#?}", r.items);
                assert!(
                    r.items.iter().all(|i| i.class == Class::Changed && i.fallback.is_none()),
                    "{form} {ptype} {id}"
                );
                assert!(
                    r.items.iter().all(|i| i.reason.contains(id) && i.reason.contains("series palette")),
                    "{form} {ptype} {id}"
                );
            });
        }
        // The mode `shades` has no equal.
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"color": {"mode": "shades", "fixedColor": "green"}});
        every_form(&parts, |form, r| {
            assert!(defaults_set(r).get("color").is_none(), "{form} {ptype}");
            assert_eq!(r.items.len(), 1, "{form} {ptype}: {:#?}", r.items);
            assert_eq!(
                (r.items[0].class, r.items[0].fallback.clone()),
                (Class::Unsupported, Some(Fallback::PropertyRemoved))
            );
        });
    }
}

#[test]
fn i31_axis_placement() {
    for ptype in TYPES {
        for value in ["left", "right", "hidden"] {
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"custom": {"axisPlacement": value}});
            parts.overrides = override_of("custom.axisPlacement", json!(value));
            every_form(&parts, |form, r| {
                if ptype == "timeseries" {
                    no_items(r);
                    assert_eq!(defaults_set(r)["axis_placement"], value, "{form}");
                    assert_eq!(override_set(r)["axis_placement"], value, "{form}");
                } else {
                    // Another type: the value has no equal. The two items name defaults and override.
                    assert_eq!(r.items.len(), 2, "{form} {ptype}: {:#?}", r.items);
                    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)), "{form} {ptype}");
                    assert!(defaults_set(r).get("axis_placement").is_none(), "{form} {ptype}");
                }
            });
        }
        // `auto` in the defaults is metadata, in each type.
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"custom": {"axisPlacement": "auto"}});
        every_form(&parts, |form, r| {
            no_items(r);
            assert!(defaults_set(r).get("axis_placement").is_none(), "{form} {ptype}");
            assert!(r.metadata > 0, "{form} {ptype}");
        });
    }
}

#[test]
fn i31_axis_label() {
    for ptype in TYPES {
        let long = "L".repeat(300);
        for (input, expected, class) in [
            (json!("Latency"), Some("Latency".to_owned()), None),
            (json!(""), None, None),
            (json!(long), Some("L".repeat(256)), Some(Class::Changed)),
        ] {
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"custom": {"axisLabel": input}});
            parts.overrides = override_of("custom.axisLabel", input.clone());
            every_form(&parts, |form, r| {
                let applies = ptype != "table";
                if applies {
                    assert_eq!(
                        defaults_set(r).get("axis_label").and_then(Value::as_str),
                        expected.as_deref(),
                        "{form} {ptype}"
                    );
                    assert_eq!(override_set_opt(r, "axis_label"), expected.clone().map(Value::from), "{form} {ptype}");
                    let changed = r.items.iter().filter(|i| i.class == Class::Changed).count();
                    assert_eq!(changed, usize::from(class.is_some()) * 2, "{form} {ptype}: {:#?}", r.items);
                } else if expected.is_some() {
                    assert_eq!(r.items.len(), 2, "{form} {ptype}: {:#?}", r.items);
                    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)), "{form}");
                } else {
                    no_items(r);
                }
            });
        }
    }
}

/// A member of the first override set, or `None` when the override or the member is absent.
fn override_set_opt(r: &DashboardImport, member: &str) -> Option<Value> {
    let out = doc(r);
    out["sections"][0]["panels"][0]["fields"]["overrides"].get(0).and_then(|o| o["set"].get(member).cloned())
}

#[test]
fn i31_soft_limits() {
    for ptype in TYPES {
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"custom": {"axisSoftMin": 0, "axisSoftMax": 100.5}});
        parts.overrides = json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
            {"id": "custom.axisSoftMin", "value": -5}, {"id": "custom.axisSoftMax", "value": 5}]}]);
        every_form(&parts, |form, r| {
            if ptype == "table" {
                // The minimum 0 is trivial and gives metadata. The other three values are refused.
                assert_eq!(r.items.len(), 3, "{form}: {:#?}", r.items);
                return;
            }
            no_items(r);
            let d = defaults_set(r);
            assert_eq!((d["soft_min"].as_f64(), d["soft_max"].as_f64()), (Some(0.0), Some(100.5)), "{form} {ptype}");
            let o = override_set(r);
            assert_eq!((o["soft_min"].as_f64(), o["soft_max"].as_f64()), (Some(-5.0), Some(5.0)), "{form} {ptype}");
        });
        // `null` is metadata.
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"custom": {"axisSoftMin": null, "axisSoftMax": null}});
        every_form(&parts, |form, r| {
            no_items(r);
            assert!(r.metadata >= 2, "{form} {ptype}");
        });
        // A pair with `axisSoftMin` not below `axisSoftMax` is refused: both are removed.
        if ptype != "table" {
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"custom": {"axisSoftMin": 10, "axisSoftMax": 10}});
            every_form(&parts, |form, r| {
                let d = defaults_set(r);
                assert!(d.get("soft_min").is_none() && d.get("soft_max").is_none(), "{form} {ptype}");
                assert_eq!(r.items.len(), 1, "{form} {ptype}: {:#?}", r.items);
                assert_eq!(
                    (r.items[0].class, r.items[0].fallback.clone()),
                    (Class::Unsupported, Some(Fallback::PropertyRemoved))
                );
            });
        }
    }
}

#[test]
fn i31_style_members_in_an_override_and_in_the_defaults() {
    for ptype in TYPES {
        let mut parts = Parts::new(ptype);
        parts.overrides = json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
            {"id": "custom.drawStyle", "value": "bars"},
            {"id": "custom.lineWidth", "value": 2},
            {"id": "custom.fillOpacity", "value": 30},
            {"id": "custom.showPoints", "value": "never"},
            {"id": "custom.pointSize", "value": 4}]}]);
        every_form(&parts, |form, r| {
            if ptype == "timeseries" {
                no_items(r);
                let o = override_set(r);
                assert_eq!(
                    o,
                    json!({"draw": "bars", "line_width": 2, "fill_opacity": 30, "show_points": "never", "point_size": 4}),
                    "{form}"
                );
            } else {
                // The five members in an override of another type are refused (15.6).
                assert_eq!(r.items.len(), 5, "{form} {ptype}: {:#?}", r.items);
                assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)), "{form} {ptype}");
            }
        });
    }
    // In the defaults, the five members map to the options of the panel (4.28).
    let mut parts = Parts::new("timeseries");
    parts.defaults = json!({"custom": {"drawStyle": "bars", "lineWidth": 2, "fillOpacity": 30, "showPoints": "never", "pointSize": 4}});
    every_form(&parts, |form, r| {
        no_items(r);
        let o = doc(r)["sections"][0]["panels"][0]["options"].clone();
        assert_eq!(
            (
                o["draw"].as_str(),
                o["line_width"].as_i64(),
                o["fill_opacity"].as_i64(),
                o["show_points"].as_str(),
                o["point_size"].as_i64()
            ),
            (Some("bars"), Some(2), Some(30), Some("never"), Some(4)),
            "{form}"
        );
        assert!(defaults_set(r).get("draw").is_none(), "{form}");
    });
    // A value out of range is refused.
    let mut parts = Parts::new("timeseries");
    parts.overrides = json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.drawStyle", "value": "area"},
        {"id": "custom.lineWidth", "value": 11},
        {"id": "custom.fillOpacity", "value": 101},
        {"id": "custom.showPoints", "value": "sometimes"},
        {"id": "custom.pointSize", "value": 0}]}]);
    every_form(&parts, |form, r| {
        assert_eq!(r.items.len(), 5, "{form}: {:#?}", r.items);
        assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)), "{form}");
    });
}

#[test]
fn i31_line_style() {
    for ptype in TYPES {
        for (fill, dash) in
            [("solid", None), ("dash", None), ("dash", Some(json!([10, 10]))), ("dot", Some(json!([0, 10])))]
        {
            let mut style = json!({"fill": fill});
            if let Some(d) = &dash {
                style["dash"] = d.clone();
            }
            let mut parts = Parts::new(ptype);
            parts.defaults = json!({"custom": {"lineStyle": style}});
            parts.overrides = override_of("custom.lineStyle", style.clone());
            every_form(&parts, |form, r| {
                if ptype != "timeseries" {
                    // Only a timeseries has the member. The default `{"fill": "solid"}` is metadata.
                    // In the defaults the default value is metadata. In an override it is not a default.
                    let wanted = if fill == "solid" { 1 } else { 2 };
                    assert_eq!(r.items.len(), wanted, "{form} {ptype} {fill}: {:#?}", r.items);
                    return;
                }
                no_items(r);
                // `solid` in the defaults is metadata, and in an override it is a value.
                assert_eq!(
                    defaults_set(r).get("line_dash").and_then(Value::as_str),
                    (fill != "solid").then_some(fill),
                    "{form} {fill}"
                );
                assert_eq!(override_set(r)["line_dash"], fill, "{form} {fill}");
            });
        }
    }
    // An array that is not the default gives `line_dash_array`. An array out of the range of 4.18 keeps the fill and
    // gives an item.
    let mut parts = Parts::new("timeseries");
    parts.defaults = json!({"custom": {"lineStyle": {"fill": "dash", "dash": [5, 5]}}});
    every_form(&parts, |form, r| {
        no_items(r);
        assert_eq!(defaults_set(r)["line_dash"], "dash", "{form}");
        assert_eq!(defaults_set(r)["line_dash_array"], json!([5, 5]), "{form}");
    });
    let mut parts = Parts::new("timeseries");
    parts.defaults = json!({"custom": {"lineStyle": {"fill": "dash", "dash": [5, 500]}}});
    every_form(&parts, |form, r| {
        assert_eq!(defaults_set(r)["line_dash"], "dash", "{form}");
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(
            (r.items[0].class, r.items[0].fallback.clone()),
            (Class::Unsupported, Some(Fallback::other("the fill value")))
        );
    });
    // Another fill is refused.
    let mut parts = Parts::new("timeseries");
    parts.defaults = json!({"custom": {"lineStyle": {"fill": "square"}}});
    every_form(&parts, |form, r| {
        assert!(defaults_set(r).get("line_dash").is_none(), "{form}");
        assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved), "{form}");
    });
}

#[test]
fn i31_transform() {
    for ptype in TYPES {
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"custom": {"transform": "negative-Y"}});
        parts.overrides = override_of("custom.transform", json!("negative-Y"));
        every_form(&parts, |form, r| {
            if ptype == "timeseries" {
                no_items(r);
                assert_eq!(defaults_set(r)["transform"], "negative_y", "{form}");
                assert_eq!(override_set(r)["transform"], "negative_y", "{form}");
            } else {
                assert_eq!(r.items.len(), 2, "{form} {ptype}: {:#?}", r.items);
            }
        });
        let mut parts = Parts::new(ptype);
        parts.defaults = json!({"custom": {"transform": "constant"}});
        every_form(&parts, |form, r| {
            assert!(defaults_set(r).get("transform").is_none(), "{form} {ptype}");
            assert_eq!(
                (r.items[0].class, r.items[0].fallback.clone()),
                (Class::Unsupported, Some(Fallback::PropertyRemoved))
            );
        });
    }
}

// --------------------------------------------------------------------- I32

#[test]
fn i32_each_document_has_format_3_and_links() {
    for form in FORMS {
        let parts = Parts::new("stat");
        let r = import(form, &parts);
        let out = doc(&r);
        assert_eq!(out["format_version"], 3, "{form}");
        assert_eq!(out["links"], json!([]), "{form}");
        // The member `links` is the member after `cursor_sync`.
        let text = std::str::from_utf8(r.document_bytes.as_ref().unwrap()).unwrap();
        assert!(text.contains("\"cursor_sync\": \"off\",\n  \"links\": [],\n  \"variables\""), "{form}");
    }
}

#[test]
fn i32_the_dry_run_without_acceptance_blocks_only_on_the_items_that_block() {
    // Links and the time override give no block: their fallbacks remove one member or one item.
    let mut parts = Parts::new("timeseries");
    parts.links = Some(json!([link_item("link", json!({"url": "//evil.test"}))]));
    parts.query_options = json!({"timeFrom": "now"});
    let r = import_dashboard(&in_form("raw", &parts), &base_options());
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.class == Class::Unsupported));
}
