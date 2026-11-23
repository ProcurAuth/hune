// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the field table, the matchers, the mapping rules and the transformation table of 15.6 (I01).

mod common;

use common::*;
use hune_grafana::{Class, Fallback};
use serde_json::{Value, json};

fn table_with_fields(fc: Value) -> Value {
    let mut p = panel(1, "table", 0);
    p["fieldConfig"] = fc;
    p
}

fn defaults_of(ptype: &str, defaults: Value) -> (hune_grafana::DashboardImport, Value) {
    let mut p = panel(1, ptype, 0);
    p["fieldConfig"] = json!({"defaults": defaults, "overrides": []});
    let r = run(&dash(vec![p], vec![]));
    let out = doc(&r);
    (r, out)
}

fn set_of(defaults: Value) -> (hune_grafana::DashboardImport, Value) {
    let (r, out) = defaults_of("table", defaults);
    let set = first_panel(&out)["fields"]["defaults"].clone();
    (r, set)
}

#[test]
fn simple_properties() {
    let (r, set) =
        set_of(json!({"unit": "percent", "decimals": 2, "min": 0, "max": 99.5, "displayName": "Name", "noValue": "-"}));
    no_items(&r);
    assert_eq!(
        set,
        json!({"unit": "percent", "decimals": 2, "min": 0, "max": 99.5, "display_name": "Name", "no_value": "-"})
    );
    for u in ["none", "short", "percentunit", "ms", "s", "bytes", "decbytes", "suffix: pts", "prefix:$"] {
        let (r, set) = set_of(json!({"unit": u}));
        no_items(&r);
        assert_eq!(set["unit"], u);
    }
    // The units that the Grafana-parity work adds.
    for u in [
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
        let (r, set) = set_of(json!({"unit": u}));
        no_items(&r);
        assert_eq!(set["unit"], u);
    }
    // The units that the later options adds (P01).
    for u in [
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
        let (r, set) = set_of(json!({"unit": u}));
        no_items(&r);
        assert_eq!(set["unit"], u);
    }
    // The Grafana units and forms outside 4.20 stay out.
    for u in ["pixel", "Tbits", "bool:on/off", "si:B", "count:rows", "currency:CHF", "time:YYYY-MM"] {
        let (r, set) = set_of(json!({"unit": u}));
        assert_eq!(set, json!({}), "{u}");
        assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved), "{u}");
    }
    // An unknown unit removes the property. A value out of its range stops the dashboard (no fallback in 15.6).
    let (r, set) = set_of(json!({"unit": "currencyGBP"}));
    assert_eq!(set, json!({}));
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    for bad in [
        json!({"decimals": 25}),
        json!({"min": 5, "max": 5}),
        json!({"displayName": ""}).as_object().map(|_| json!({"displayName": "x".repeat(300)})).unwrap(),
    ] {
        let mut p = panel(1, "table", 0);
        p["fieldConfig"] = json!({"defaults": bad, "overrides": []});
        let r = run(&dash(vec![p], vec![]));
        assert!(r.document_bytes.is_none(), "{bad}");
        assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    }
}

#[test]
fn custom_table_properties() {
    let (r, set) = set_of(
        json!({"custom": {"width": 140, "align": "center", "filterable": true, "hidden": true, "cellOptions": {"type": "color-text"}}}),
    );
    no_items(&r);
    assert_eq!(
        set,
        json!({"width": 140, "align": "center", "cell": {"type": "color_text", "mode": null}, "filterable": true, "hidden": true})
    );
    for (grafana, expected) in [
        (json!({"type": "auto"}), json!({"type": "auto", "mode": null})),
        (json!({"type": "color-background"}), json!({"type": "color_background", "mode": "gradient"})),
        (json!({"type": "color-background", "mode": "basic"}), json!({"type": "color_background", "mode": "basic"})),
        (
            json!({"type": "color-background", "mode": "gradient"}),
            json!({"type": "color_background", "mode": "gradient"}),
        ),
    ] {
        let (r, set) = set_of(json!({"custom": {"cellOptions": grafana}}));
        no_items(&r);
        assert_eq!(set["cell"], expected);
    }
    for (old, expected) in [
        ("auto", json!({"type": "auto", "mode": null})),
        ("color-text", json!({"type": "color_text", "mode": null})),
        ("color-background", json!({"type": "color_background", "mode": "gradient"})),
        ("color-background-solid", json!({"type": "color_background", "mode": "basic"})),
    ] {
        let (r, set) = set_of(json!({"custom": {"displayMode": old}}));
        no_items(&r);
        assert_eq!(set["cell"], expected);
    }
    for other in
        [json!({"type": "sparkline"}), json!({"type": "data-links"}), json!({"type": "image"}), json!({"type": "pill"})]
    {
        let (r, set) = set_of(json!({"custom": {"cellOptions": other}}));
        assert_eq!(set["cell"], json!({"type": "auto", "mode": null}));
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].fallback, Some(Fallback::other("auto")));
    }
}

/// The cell types `gauge` and `json`, and the wrap of the text.
#[test]
fn gauge_and_json_cells() {
    for (grafana, expected) in [
        (json!({"type": "gauge"}), json!({"type": "gauge", "mode": "gradient"})),
        (json!({"type": "gauge", "mode": "basic"}), json!({"type": "gauge", "mode": "basic"})),
        (json!({"type": "gauge", "mode": "gradient"}), json!({"type": "gauge", "mode": "gradient"})),
        (json!({"type": "gauge", "mode": "lcd"}), json!({"type": "gauge", "mode": "lcd"})),
        (json!({"type": "gauge", "mode": "lcd", "valueDisplayMode": "color"}), json!({"type": "gauge", "mode": "lcd"})),
        (json!({"type": "gauge", "valueDisplayMode": "text"}), json!({"type": "gauge", "mode": "gradient"})),
        (json!({"type": "json-view"}), json!({"type": "json", "mode": null})),
    ] {
        let (r, set) = set_of(json!({"custom": {"cellOptions": grafana}}));
        no_items(&r);
        assert_eq!(set["cell"], expected);
    }
    for (old, expected) in [
        ("gradient-gauge", json!({"type": "gauge", "mode": "gradient"})),
        ("lcd-gauge", json!({"type": "gauge", "mode": "lcd"})),
        ("basic", json!({"type": "gauge", "mode": "basic"})),
        ("json-view", json!({"type": "json", "mode": null})),
    ] {
        let (r, set) = set_of(json!({"custom": {"displayMode": old}}));
        no_items(&r);
        assert_eq!(set["cell"], expected);
    }
    // A gauge mode that Hune has not, and a hidden value, keep an item.
    let (r, set) = set_of(json!({"custom": {"cellOptions": {"type": "gauge", "mode": "spiral"}}}));
    assert_eq!(set["cell"], json!({"type": "gauge", "mode": "gradient"}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("gradient")));
    let (r, set) = set_of(json!({"custom": {"cellOptions": {"type": "gauge", "valueDisplayMode": "hidden"}}}));
    assert_eq!(set["cell"], json!({"type": "gauge", "mode": "gradient"}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::MemberRemoved));
    // The cell types are table properties.
    let (r, _) = defaults_of("stat", json!({"custom": {"cellOptions": {"type": "gauge"}}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
}

fn table_options_of(defaults: Value) -> (hune_grafana::DashboardImport, Value) {
    let mut p = panel(1, "table", 0);
    p["fieldConfig"] = json!({"defaults": defaults, "overrides": []});
    let r = run(&dash(vec![p], vec![]));
    let out = doc(&r);
    let options = first_panel(&out)["options"].clone();
    (r, options)
}

/// `custom.wrapText` (Grafana 12) and `custom.cellOptions.wrapText` (Grafana 11) give the table option `wrap`.
#[test]
fn table_wrap_text() {
    for custom in [json!({"wrapText": true}), json!({"cellOptions": {"type": "auto", "wrapText": true}})] {
        let (r, options) = table_options_of(json!({"custom": custom}));
        no_items(&r);
        assert_eq!(options["wrap"], true);
    }
    let (r, options) = table_options_of(json!({"custom": {"wrapText": false}}));
    no_items(&r);
    assert_eq!(options.get("wrap"), None, "the canonical form leaves out the default");
    let (r, options) = table_options_of(json!({"custom": {"align": "auto"}}));
    no_items(&r);
    assert_eq!(options.get("wrap"), None);
    // A wrap in an override is a property that Hune has not: the table option is for the whole table.
    let (r, set) = overrides(json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.wrapText", "value": true}, {"id": "unit", "value": "ms"}]}]));
    assert_eq!(set[0]["set"], json!({"unit": "ms"}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    // The wrap is a table member only.
    let (r, _) = defaults_of("stat", json!({"custom": {"wrapText": true}}));
    assert_eq!(r.items.len(), 1);
}

/// P09: `custom.inspect` gives the property `inspect` of a table (4.18, 15.6).
#[test]
fn table_cell_inspect() {
    let (r, set) = set_of(json!({"custom": {"inspect": true}}));
    no_items(&r);
    assert_eq!(set, json!({"inspect": true}));
    // `false` in the defaults is the Grafana default: metadata, no member.
    let (r, set) = set_of(json!({"custom": {"inspect": false}}));
    no_items(&r);
    assert!(r.metadata > 0);
    assert_eq!(set, json!({}));
    // In an override, both values map: `false` turns off a `true` of the defaults.
    let (r, o) = overrides(json!([
        {"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "custom.inspect", "value": true}]},
        {"matcher": {"id": "byName", "options": "b"}, "properties": [{"id": "custom.inspect", "value": false}]}]));
    no_items(&r);
    assert_eq!(o[0]["set"], json!({"inspect": true}));
    assert_eq!(o[1]["set"], json!({"inspect": false}));
    // A value that is not a boolean stops the dashboard, as `custom.filterable` does.
    let mut p = panel(1, "table", 0);
    p["fieldConfig"] = json!({"defaults": {"custom": {"inspect": "yes"}}, "overrides": []});
    let r = run(&dash(vec![p], vec![]));
    assert!(r.document_bytes.is_none());
    assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    // Another panel type keeps the general rule of a member of `custom`: `true` is removed, `false` is metadata.
    let (r, out) = defaults_of("stat", json!({"custom": {"inspect": true}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    assert_eq!(first_panel(&out)["fields"]["defaults"], json!({}));
    let (r, _) = defaults_of("stat", json!({"custom": {"inspect": false}}));
    no_items(&r);
}

/// The footer of Grafana 12, `custom.footer.reducers`, gives the property `footer`.
#[test]
fn table_footer() {
    let (r, set) = set_of(json!({"custom": {"footer": {"reducers": ["sum", "mean", "lastNotNull"]}}}));
    no_items(&r);
    assert_eq!(set["footer"], json!({"reducers": ["total", "mean", "last_not_null"]}));
    // `countAll` counts each row: it is `count`. The second item of a pair that gives one value is removed.
    let (r, set) = set_of(json!({"custom": {"footer": {"reducers": ["countAll", "count"]}}}));
    assert_eq!(set["footer"], json!({"reducers": ["count"]}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
    // The empty list of every Grafana table is metadata.
    let (r, set) = set_of(json!({"custom": {"footer": {"reducers": []}}}));
    no_items(&r);
    assert!(r.metadata > 0);
    assert_eq!(set.get("footer"), None);
    // A calculation that Hune has not keeps an item.
    let (r, set) = set_of(json!({"custom": {"footer": {"reducers": ["sum", "allValues"]}}}));
    assert_eq!(set["footer"], json!({"reducers": ["total"]}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("item removed")));
    // More than 8 calculations: the first 8 stay.
    let ids = ["sum", "mean", "min", "max", "first", "last", "range", "diff", "median"];
    let (r, set) = set_of(json!({"custom": {"footer": {"reducers": ids}}}));
    assert_eq!(set["footer"]["reducers"].as_array().map(Vec::len), Some(8));
    assert_eq!(r.items.len(), 1);
    // An override carries the same value in the form of the migration of Grafana.
    let (r, o) = overrides(json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.footer.reducers", "value": ["sum"]}]}]));
    no_items(&r);
    assert_eq!(o[0]["set"], json!({"footer": {"reducers": ["total"]}}));
    // A member of the footer that Hune has not keeps an item. The old form of the footer is read by show.
    let (r, _) = set_of(json!({"custom": {"footer": {"reducers": ["sum"], "fields": ["a"]}}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "fieldConfig.defaults.custom.footer.fields");
    // The footer is a table member.
    let (r, _) = defaults_of("stat", json!({"custom": {"footer": {"reducers": ["sum"]}}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
}

/// The footer of Grafana 11 and older is `options.footer`.
#[test]
fn old_table_footer() {
    let mut p = panel(1, "table", 0);
    p["options"] =
        json!({"showHeader": true, "footer": {"show": false, "reducer": ["sum"], "fields": "", "countRows": false}});
    let r = run_plain(&dash(vec![p], vec![]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    // A footer that is shown moves to the property `footer`, as Grafana 12 moves it (P06, the later options).
    let mut p = panel(1, "table", 0);
    p["options"] = json!({"footer": {"show": true, "reducer": ["sum"], "fields": ""}});
    let r = run_plain(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "options.footer");
    assert_eq!((r.items[0].class, r.items[0].fallback.clone()), (Class::Changed, None));
}

/// `custom.hideFrom` gives the property `hide_from`. A table hides a column with `viz`.
#[test]
fn hide_from_property() {
    let (r, o) = overrides_of(
        "timeseries",
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.hideFrom", "value": {"legend": false, "tooltip": true, "viz": true}}]}]),
    );
    no_items(&r);
    assert_eq!(o[0]["set"], json!({"hide_from": {"legend": false, "tooltip": true, "viz": true}}));
    // A member that is absent is false. An override with three false values stays: it shows the series again.
    let (r, o) = overrides_of(
        "timeseries",
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.hideFrom", "value": {"tooltip": true}}]}]),
    );
    no_items(&r);
    assert_eq!(o[0]["set"], json!({"hide_from": {"legend": false, "tooltip": true, "viz": false}}));
    let (r, o) = overrides_of(
        "timeseries",
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "custom.hideFrom", "value": {"legend": false, "tooltip": false, "viz": false}}]}]),
    );
    no_items(&r);
    assert_eq!(o[0]["set"], json!({"hide_from": {"legend": false, "tooltip": false, "viz": false}}));
    // In the defaults, the three false values are the Grafana default: metadata.
    let (r, _) =
        defaults_of("timeseries", json!({"custom": {"hideFrom": {"legend": false, "tooltip": false, "viz": false}}}));
    no_items(&r);
    assert!(r.metadata > 0);
    let (r, out) =
        defaults_of("timeseries", json!({"custom": {"hideFrom": {"legend": true, "tooltip": false, "viz": false}}}));
    no_items(&r);
    assert_eq!(
        first_panel(&out)["fields"]["defaults"],
        json!({"hide_from": {"legend": true, "tooltip": false, "viz": false}})
    );
    // A value that is not a flag, and a member that Grafana has not, keep their items.
    let (r, _) = defaults_of("timeseries", json!({"custom": {"hideFrom": {"legend": "yes"}}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    let (r, _) = defaults_of("timeseries", json!({"custom": {"hideFrom": {"viz": true, "other": true}}}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "fieldConfig.defaults.custom.hideFrom.other");
    // A table hides a column with `viz`: the property `hidden` (4.18).
    let mut p = panel(1, "table", 0);
    p["fieldConfig"] = json!({"defaults": {}, "overrides": [
        {"matcher": {"id": "byName", "options": "a"}, "properties": [
            {"id": "custom.hideFrom", "value": {"legend": false, "tooltip": false, "viz": true}}]},
        {"matcher": {"id": "byName", "options": "b"}, "properties": [
            {"id": "custom.hideFrom", "value": {"legend": false, "tooltip": false, "viz": false}}]}]});
    let r = run(&dash(vec![p], vec![]));
    no_items(&r);
    let out = doc(&r);
    let o = &first_panel(&out)["fields"]["overrides"];
    assert_eq!(o[0]["set"], json!({"hidden": true}));
    assert_eq!(o[1]["set"], json!({"hidden": false}));
}

/// The matcher `byNames`.
#[test]
fn names_matcher() {
    let (r, o) = overrides(json!([
        {"matcher": {"id": "byNames", "options": {"mode": "include", "names": ["a", "b", "a"]}},
         "properties": [{"id": "unit", "value": "percent"}]},
        {"matcher": {"id": "byNames", "options": {"mode": "exclude", "names": ["FRA p99"], "prefix": "All except:", "readOnly": true}},
         "properties": [{"id": "unit", "value": "ms"}]},
        {"matcher": {"id": "byNames", "options": {"names": ["c"]}}, "properties": [{"id": "unit", "value": "s"}]},
    ]));
    no_items(&r);
    assert!(r.metadata >= 2, "prefix and readOnly are metadata");
    assert_eq!(o[0]["match"], json!({"by": "names", "names": ["a", "b"], "mode": "include"}));
    assert_eq!(o[1]["match"], json!({"by": "names", "names": ["FRA p99"], "mode": "exclude"}));
    assert_eq!(o[2]["match"], json!({"by": "names", "names": ["c"], "mode": "include"}));
    // The override that a click on the legend writes.
    let (r, o) = overrides_of(
        "timeseries",
        json!([
            {"__systemRef": "hideSeriesFrom",
             "matcher": {"id": "byNames", "options": {"mode": "exclude", "names": ["x", "y"], "prefix": "All except:", "readOnly": true}},
             "properties": [{"id": "custom.hideFrom", "value": {"legend": false, "tooltip": true, "viz": true}}]},
        ]),
    );
    no_items(&r);
    assert_eq!(
        o[0],
        json!({"match": {"by": "names", "names": ["x", "y"], "mode": "exclude"},
               "set": {"hide_from": {"legend": false, "tooltip": true, "viz": true}}})
    );
}

/// A system reference other than `hideSeriesFrom` is not known: the member keeps its item.
#[test]
fn other_system_references_keep_their_item() {
    let (r, o) = overrides_of(
        "timeseries",
        json!([{"__systemRef": "other",
                "matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "unit", "value": "ms"}]}]),
    );
    assert_eq!(o[0]["set"], json!({"unit": "ms"}));
    assert_eq!(r.items.len(), 1);
    assert!(r.items[0].member.ends_with("__systemRef"));
    assert_eq!(r.items[0].fallback, Some(Fallback::MemberRemoved));
}

#[test]
fn names_matcher_faults_remove_the_override() {
    let many: Vec<String> = (0..65).map(|i| format!("n{i}")).collect();
    for options in [
        json!({"mode": "include", "names": []}),
        json!({"mode": "include", "names": many}),
        json!({"mode": "include", "names": ["a", 1]}),
        json!({"mode": "include", "names": ["a", ""]}),
        json!({"mode": "include", "names": ["a\nb"]}),
        json!({"mode": "include", "names": "a"}),
        json!({"mode": "include"}),
        json!({"mode": "any", "names": ["a"]}),
        json!("a"),
    ] {
        let (r, o) = overrides(json!([{"matcher": {"id": "byNames", "options": options.clone()},
                                       "properties": [{"id": "unit", "value": "percent"}]}]));
        assert_eq!(o, json!([]), "{options}");
        assert_eq!(r.items.len(), 1, "{options}: {:#?}", r.items);
        assert_eq!(r.items[0].fallback, Some(Fallback::other("override removed")));
    }
    let sixty_four: Vec<String> = (0..64).map(|i| format!("n{i}")).collect();
    let (r, o) = overrides(json!([{"matcher": {"id": "byNames", "options": {"names": sixty_four}},
                                   "properties": [{"id": "unit", "value": "percent"}]}]));
    no_items(&r);
    assert_eq!(o[0]["match"]["names"].as_array().map(Vec::len), Some(64));
}

/// The member `scope` of a matcher.
#[test]
fn matcher_scope() {
    let (r, o) = overrides(json!([
        {"matcher": {"id": "byName", "scope": "series", "options": "users"}, "properties": [{"id": "custom.width", "value": 80}]},
        {"matcher": {"id": "byRegexp", "scope": "series", "options": "/^a/"}, "properties": [{"id": "decimals", "value": 1}]},
        {"matcher": {"id": "byNames", "scope": "series", "options": {"names": ["a"]}}, "properties": [{"id": "unit", "value": "s"}]},
    ]));
    no_items(&r);
    assert_eq!(o[0], json!({"match": {"by": "name", "value": "users"}, "set": {"width": 80}}));
    assert_eq!(o[1]["match"], json!({"by": "regex", "value": "/^a/"}));
    assert_eq!(o[2]["match"]["by"], "names");
    for scope in [json!("nested"), json!(""), json!(1), json!(null)] {
        let (r, o) = overrides(json!([{"matcher": {"id": "byName", "scope": scope.clone(), "options": "a"},
                                       "properties": [{"id": "unit", "value": "percent"}]}]));
        assert_eq!(o, json!([]), "{scope}");
        let scope_items: Vec<_> = r.items.iter().filter(|i| i.member.ends_with("matcher.scope")).collect();
        assert_eq!(scope_items.len(), 1, "{scope}: {:#?}", r.items);
        assert_eq!(scope_items[0].fallback, Some(Fallback::other("override removed")));
    }
}

/// `custom.thresholdsStyle.mode` gives the option `thresholds_style` of a time series and of a bar chart.
#[test]
fn thresholds_style() {
    for (grafana, expected) in [
        ("off", None),
        ("line", Some("line")),
        ("dashed", Some("dashed")),
        ("area", Some("area")),
        ("line+area", Some("line_area")),
        ("dashed+area", Some("dashed_area")),
    ] {
        for ptype in ["timeseries", "barchart"] {
            let (r, out) = defaults_of(ptype, json!({"custom": {"thresholdsStyle": {"mode": grafana}}}));
            no_items(&r);
            let options = &first_panel(&out)["options"];
            assert_eq!(options.get("thresholds_style").and_then(Value::as_str), expected, "{ptype} {grafana}");
        }
    }
    // `series` has no equal: the item says so, and the default stays.
    for ptype in ["timeseries", "barchart"] {
        let (r, out) = defaults_of(ptype, json!({"custom": {"thresholdsStyle": {"mode": "series"}}}));
        assert_eq!(r.items.len(), 1, "{ptype}");
        assert_eq!(r.items[0].class, Class::Unsupported);
        assert_eq!(first_panel(&out)["options"].get("thresholds_style"), None);
    }
    // The member is not a property of an override.
    let mut p = panel(1, "timeseries", 0);
    p["fieldConfig"] = json!({"defaults": {}, "overrides": [
        {"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "custom.thresholdsStyle", "value": {"mode": "line"}}]}]});
    let r = run(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    // The other panel types have no such option: the default stays metadata.
    let (r, _) = defaults_of("stat", json!({"custom": {"thresholdsStyle": {"mode": "off"}}}));
    no_items(&r);
}

#[test]
fn table_only_properties_in_another_panel() {
    let (r, out) = defaults_of(
        "stat",
        json!({"custom": {"width": 100, "align": "left", "cellOptions": {"type": "auto"}, "filterable": true, "hidden": true}}),
    );
    assert_eq!(r.items.len(), 5);
    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)));
    assert_eq!(first_panel(&out)["fields"]["defaults"], json!({}));
}

#[test]
fn other_property_ids_and_custom_members() {
    let (r, set) = set_of(
        json!({"nullValueMode": "connected", "custom": {"axisPlacement": "right", "inspect": false, "hideFrom": {"viz": false}}, "fieldMinMax": false}),
    );
    assert_eq!(set, json!({}));
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)));
}

#[test]
fn thresholds() {
    let (r, set) = set_of(
        json!({"thresholds": {"mode": "percentage", "steps": [{"color": "green", "value": null}, {"color": "dark-red", "value": 80.5}, {"color": "#fff", "value": 90}]}}),
    );
    no_items(&r);
    assert_eq!(
        set["thresholds"],
        json!({"mode": "percentage", "steps": [
        {"value": null, "color": "green"}, {"value": 80.5, "color": "dark-red"}, {"value": 90, "color": "#fff"}]})
    );
    // A colour that format 1 does not have is changed to the nearest token.
    let (r, set) = set_of(
        json!({"thresholds": {"steps": [{"color": "rgba(245,54,54,0.9)", "value": null}, {"color": "white", "value": 1}]}}),
    );
    // A CSS colour name (`white`) gives its exact hex colour and needs no item.
    assert_eq!(r.items.len(), 1);
    assert!(r.items.iter().all(|i| i.class == Class::Changed));
    assert_eq!(set["thresholds"]["mode"], "absolute");
    assert_eq!(set["thresholds"]["steps"][0]["color"], "semi-dark-red");
    assert_eq!(set["thresholds"]["steps"][1]["color"], "#ffffff");
    let (r, set) =
        set_of(json!({"mappings": [{"type": "value", "options": {"-1": {"text": "NO DATA", "color": "gray"}}}]}));
    no_items(&r);
    assert_eq!(set["mappings"][0]["color"], "#808080");
    // A v2 export writes the base step with the value 0. Grafana ignores the value of the first step.
    let (r, set) = set_of(
        json!({"thresholds": {"mode": "absolute", "steps": [{"value": 0, "color": "green"}, {"value": 80, "color": "red"}]}}),
    );
    no_items(&r);
    assert_eq!(
        set["thresholds"],
        json!({"mode": "absolute", "steps": [{"value": null, "color": "green"}, {"value": 80, "color": "red"}]})
    );
    // The value of the first step does not count: the second step needs no number above it.
    let (r, set) =
        set_of(json!({"thresholds": {"steps": [{"value": 5, "color": "green"}, {"value": 1, "color": "red"}]}}));
    no_items(&r);
    assert_eq!(set["thresholds"]["steps"][0]["value"], Value::Null);
    assert_eq!(set["thresholds"]["steps"][1]["value"], 1);
    // Bad steps give an unsupported item and the dashboard is not imported.
    for steps in [
        json!([{"color": "red", "value": "x"}]),
        json!([{"color": "red", "value": null}, {"color": "blue", "value": null}]),
        json!([{"color": "red", "value": null}, {"color": "blue", "value": 5}, {"color": "green", "value": 5}]),
        json!([]),
    ] {
        let mut p = panel(1, "table", 0);
        p["fieldConfig"] = json!({"defaults": {"thresholds": {"steps": steps}}, "overrides": []});
        let r = run(&dash(vec![p], vec![]));
        assert!(r.document_bytes.is_none(), "{steps}");
        assert_eq!(r.items[0].fallback, Some(Fallback::DashboardNotImported));
    }
}

#[test]
fn colour_modes() {
    let (r, set) = set_of(json!({"color": {"mode": "fixed", "fixedColor": "blue"}}));
    no_items(&r);
    assert_eq!(set["color"], json!({"mode": "fixed", "fixed": "blue"}));
    let (r, set) = set_of(json!({"color": {"mode": "fixed", "fixedColor": "#FF0000"}}));
    no_items(&r);
    assert_eq!(set["color"]["fixed"], "#FF0000");
    let (r, set) = set_of(json!({"color": {"mode": "fixed", "fixedColor": "#12345"}}));
    assert_eq!(set, json!({}));
    assert_eq!(r.items[0].class, Class::Unsupported);
    assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    let (r, set) = set_of(json!({"color": {"mode": "fixed", "fixedColor": "#7EB26D"}}));
    assert_eq!(set["color"], json!({"mode": "fixed", "fixed": "#7EB26D"}));
    no_items(&r);
    let (r, set) = set_of(json!({"color": {"mode": "fixed", "fixedColor": "rgb(1,2,3)"}}));
    assert!(set["color"]["fixed"].as_str().is_some());
    assert_eq!(r.items[0].class, Class::Changed);
    let (r, set) = set_of(json!({"color": {"mode": "thresholds"}}));
    no_items(&r);
    assert_eq!(set["color"], json!({"mode": "thresholds", "fixed": null}));
    let (r, set) = set_of(json!({"color": {"mode": "palette-classic", "fixedColor": "green", "seriesBy": "last"}}));
    no_items(&r);
    assert_eq!(set["color"], json!({"mode": "palette", "fixed": null}));
    let (r, set) = set_of(json!({"color": {"mode": "palette-classic-by-name"}}));
    assert_eq!(set["color"], json!({"mode": "palette", "fixed": null}));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
    let (r, set) = set_of(json!({"color": {"mode": "shades", "seriesBy": "min"}}));
    assert_eq!(set, json!({}));
    assert_eq!(r.items.len(), 2);
    assert!(r.items.iter().any(|i| i.class == Class::Dropped));
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PropertyRemoved)));
    let (r, _) = set_of(json!({"color": {"mode": "fixed"}}));
    assert_eq!(r.items[0].class, Class::Unsupported);
}

fn mapping_set(mappings: Value) -> (hune_grafana::DashboardImport, Value) {
    let (r, set) = set_of(json!({"mappings": mappings}));
    let m = set.get("mappings").cloned().unwrap_or(Value::Null);
    (r, m)
}

#[test]
fn mappings_split_and_order() {
    // Rule 1 and rule 4: one mapping for each key, in the order of `index`.
    let (r, m) = mapping_set(json!([
        {"type": "value", "options": {"a": {"text": "A", "color": "red", "index": 2}, "b": {"text": "B", "index": 0}, "c": {"color": "blue", "index": 1}}},
        {"type": "range", "options": {"from": 1, "to": 5, "result": {"text": "low", "index": 3}}},
        {"type": "range", "options": {"from": null, "to": 9, "result": {"color": "green", "index": 4}}},
        {"type": "regex", "options": {"pattern": "/x(\\d)/", "result": {"text": "X$1", "index": 6}}},
        {"type": "special", "options": {"match": "null", "result": {"text": "N", "index": 5}}}
    ]));
    no_items(&r);
    let kinds: Vec<String> = m
        .as_array()
        .unwrap()
        .iter()
        .map(|x| format!("{}:{}", x["kind"], x.get("value").or(x.get("match")).unwrap_or(&Value::Null)))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "\"value\":\"b\"",
            "\"value\":\"c\"",
            "\"value\":\"a\"",
            "\"range\":null",
            "\"range\":null",
            "\"special\":\"null\"",
            "\"regex\":null"
        ]
    );
    assert_eq!(m[0], json!({"kind": "value", "value": "b", "text": "B", "color": null}));
    assert_eq!(m[1], json!({"kind": "value", "value": "c", "text": null, "color": "blue"}));
    assert_eq!(m[3], json!({"kind": "range", "from": 1, "to": 5, "text": "low", "color": null}));
    assert_eq!(m[4], json!({"kind": "range", "from": null, "to": 9, "text": null, "color": "green"}));
    assert_eq!(m[6], json!({"kind": "regex", "pattern": "/x(\\d)/", "text": "X$1", "color": null}));
}

#[test]
fn mappings_equal_index_keep_the_input_order() {
    let (_, m) = mapping_set(json!([
        {"type": "special", "options": {"match": "true", "result": {"text": "yes", "index": 0}}},
        {"type": "special", "options": {"match": "false", "result": {"text": "no", "index": 0}}},
        {"type": "range", "options": {"from": 0, "to": 1, "result": {"text": "r"}}}
    ]));
    assert_eq!(m[0]["match"], "true");
    assert_eq!(m[1]["match"], "false");
    assert_eq!(m[2]["kind"], "range");
}

#[test]
fn mappings_special_names() {
    for (g, v) in [
        ("null", "null"),
        ("nan", "nan"),
        ("null+nan", "null_nan"),
        ("true", "true"),
        ("false", "false"),
        ("empty", "empty"),
    ] {
        let (r, m) =
            mapping_set(json!([{"type": "special", "options": {"match": g, "result": {"text": "t", "index": 0}}}]));
        no_items(&r);
        assert_eq!(m[0]["match"], v);
    }
}

#[test]
fn mappings_empty_text_and_missing_values() {
    // Rule 5: an empty text is null, an absent colour is null, a mapping with both null is dropped.
    let (r, m) = mapping_set(json!([
        {"type": "value", "options": {"a": {"text": "", "color": "red", "index": 0}, "b": {"text": "", "index": 1}}},
        {"type": "special", "options": {"match": "empty", "result": {"index": 2}}}
    ]));
    assert_eq!(m.as_array().unwrap().len(), 1);
    assert_eq!(m[0], json!({"kind": "value", "value": "a", "text": null, "color": "red"}));
    assert_eq!(r.items.len(), 2);
    assert!(r.items.iter().all(|i| i.class == Class::Dropped));
}

#[test]
fn mappings_beyond_the_maximum_keep_the_first_256() {
    let mut options = serde_json::Map::new();
    for i in 0..300 {
        options.insert(format!("k{i:03}"), json!({"text": format!("t{i}"), "index": i}));
    }
    let (r, m) = mapping_set(json!([{"type": "value", "options": Value::Object(options)}]));
    assert_eq!(m.as_array().unwrap().len(), 256);
    assert_eq!(m[255]["value"], "k255");
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("the first 256 mappings stay")));
}

#[test]
fn bad_mappings_stop_the_dashboard() {
    for bad in [
        json!([{"type": "unknown", "options": {}}]),
        json!([{"type": "range", "options": {"from": 9, "to": 1, "result": {"text": "x"}}}]),
        json!([{"type": "range", "options": {"result": {"text": "x"}}}]),
        json!([{"type": "special", "options": {"match": "weird", "result": {"text": "x"}}}]),
        json!([{"type": "regex", "options": {"pattern": "(?i)x", "result": {"text": "x"}}}]),
    ] {
        let mut p = panel(1, "table", 0);
        p["fieldConfig"] = json!({"defaults": {"mappings": bad}, "overrides": []});
        let r = run(&dash(vec![p], vec![]));
        assert!(r.document_bytes.is_none(), "{bad}: {:#?}", r.items);
    }
}

#[test]
fn data_links_of_a_field() {
    let (r, set) = set_of(
        json!({"links": [{"title": "Go", "url": "https://x.example/${__data.fields.name}", "targetBlank": true}, {"title": "Bad", "url": "//x"}]}),
    );
    assert_eq!(
        set["links"],
        json!([{"title": "Go", "url": "https://x.example/${__data.fields.name}", "target_blank": true}])
    );
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].fallback, Some(Fallback::LinkRemoved));
}

// ------------------------------------------------------------------ overrides

fn overrides(list: Value) -> (hune_grafana::DashboardImport, Value) {
    overrides_of("table", list)
}

fn overrides_of(ptype: &str, list: Value) -> (hune_grafana::DashboardImport, Value) {
    let mut p = panel(1, ptype, 0);
    p["fieldConfig"] = json!({"defaults": {}, "overrides": list});
    let r = run(&dash(vec![p], vec![]));
    let out = doc(&r);
    let o = first_panel(&out)["fields"]["overrides"].clone();
    (r, o)
}

#[test]
fn override_matchers() {
    let (r, o) = overrides(json!([
        {"matcher": {"id": "byName", "options": "weight"}, "properties": [{"id": "unit", "value": "percent"}]},
        {"matcher": {"id": "byRegexp", "options": "/^ratio.*/i"}, "properties": [{"id": "decimals", "value": 1}]},
        {"matcher": {"id": "byRegexp", "options": "Total"}, "properties": [{"id": "decimals", "value": 3}]},
    ]));
    no_items(&r);
    assert_eq!(o[0], json!({"match": {"by": "name", "value": "weight"}, "set": {"unit": "percent"}}));
    assert_eq!(o[1], json!({"match": {"by": "regex", "value": "/^ratio.*/i"}, "set": {"decimals": 1}}));
    assert_eq!(o[2]["match"]["value"], "Total");
}

#[test]
fn unsupported_matchers_and_patterns() {
    for matcher in [
        json!({"id": "byType", "options": "number"}),
        json!({"id": "byValue", "options": {}}),
        json!({"id": "byRegexp", "options": "(?i)a"}),
        json!({"id": "byRegexp", "options": "\\p{L}"}),
        json!({"id": "byName", "options": ""}),
    ] {
        let (r, o) =
            overrides(json!([{"matcher": matcher.clone(), "properties": [{"id": "unit", "value": "percent"}]}]));
        assert_eq!(o, json!([]), "{matcher}");
        assert_eq!(r.items.len(), 1, "{matcher}");
        assert_eq!(r.items[0].fallback, Some(Fallback::other("override removed")));
    }
}

#[test]
fn override_properties() {
    let (r, o) = overrides(json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [
        {"id": "unit", "value": "ms"}, {"id": "noValue", "value": "x"}, {"id": "custom.drawStyle", "value": "bars"},
        {"id": "custom.width", "value": 50}, {"id": "mystery", "value": 1}, {"id": "mystery2", "value": null}]}]));
    assert_eq!(o[0]["set"], json!({"unit": "ms", "width": 50}));
    assert_eq!(r.items.len(), 3, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.fallback == Some(Fallback::PropertyRemoved)));
    // An override that keeps no property leaves the document.
    let (r, o) = overrides(
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "noValue", "value": "x"}]}]),
    );
    assert_eq!(o, json!([]));
    assert_eq!(r.items.len(), 1);
    // The same property twice: the last value stays.
    let (_, o) = overrides(
        json!([{"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "decimals", "value": 1}, {"id": "decimals", "value": 4}]}]),
    );
    assert_eq!(o[0]["set"]["decimals"], 4);
    // min and max in one set.
    let mut p = table_with_fields(
        json!({"defaults": {}, "overrides": [{"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "min", "value": 5}, {"id": "max", "value": 1}]}]}),
    );
    p["id"] = json!(1);
    let r = run(&dash(vec![p], vec![]));
    assert!(r.document_bytes.is_none());
}

// ------------------------------------------------------------------ transformations

fn transforms(list: Value) -> (hune_grafana::DashboardImport, Value) {
    let mut p = panel(1, "table", 0);
    p["transformations"] = list;
    let r = run(&dash(vec![p], vec![]));
    let out = doc(&r);
    let t = first_panel(&out)["transforms"].clone();
    (r, t)
}

fn filter(id: &str, options: Value) -> Value {
    json!({"id": "filterByValue", "options": {"type": "include", "match": "all", "filters": [{"fieldName": "f", "config": {"id": id, "options": options}}]}})
}

#[test]
fn filter_by_value_matchers() {
    for (id, options, expected) in [
        ("equal", json!({"value": 5}), json!({"kind": "equal", "value": 5})),
        ("equal", json!({"value": "x"}), json!({"kind": "equal", "value": "x"})),
        ("notEqual", json!({"value": "x"}), json!({"kind": "not_equal", "value": "x"})),
        ("greater", json!({"value": 1.5}), json!({"kind": "greater", "value": 1.5})),
        ("greaterOrEqual", json!({"value": 2}), json!({"kind": "greater_or_equal", "value": 2})),
        ("lower", json!({"value": 3}), json!({"kind": "lower", "value": 3})),
        ("lowerOrEqual", json!({"value": 4}), json!({"kind": "lower_or_equal", "value": 4})),
        ("between", json!({"from": 1, "to": 9}), json!({"kind": "range", "from": 1, "to": 9})),
        ("regex", json!({"value": "^a.*"}), json!({"kind": "regex", "pattern": "^a.*"})),
        ("isNull", json!({}), json!({"kind": "is_null"})),
        ("isNotNull", json!({}), json!({"kind": "is_not_null"})),
    ] {
        let (r, t) = transforms(json!([filter(id, options)]));
        no_items(&r);
        assert_eq!(t[0]["kind"], "filter_by_value");
        assert_eq!(t[0]["mode"], "include");
        assert_eq!(t[0]["match"], "all");
        assert_eq!(t[0]["filters"][0], json!({"field": "f", "matcher": expected}), "{id}");
    }
}

#[test]
fn unsupported_matchers_remove_the_transformation() {
    for (id, options) in [
        ("substring", json!({"value": "a"})),
        ("notSubstring", json!({"value": "a"})),
        ("greater", json!({"value": "a"})),
        ("between", json!({"from": 9, "to": 1})),
    ] {
        let (r, t) = transforms(json!([filter(id, options)]));
        assert_eq!(t, json!([]), "{id}");
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].fallback, Some(Fallback::other("transformation removed")));
    }
}

#[test]
fn grouping_to_matrix_and_the_format_transformations() {
    let (r, t) = transforms(
        json!([{"id": "groupingToMatrix", "options": {"columnField": "c", "rowField": "r", "valueField": "v", "emptyValue": "empty"}}]),
    );
    no_items(&r);
    assert_eq!(
        t[0],
        json!({"kind": "grouping_to_matrix", "column_field": "c", "row_field": "r", "value_field": "v", "empty_value": "empty"})
    );
    for e in ["null", "true", "false"] {
        let (_, t) = transforms(
            json!([{"id": "groupingToMatrix", "options": {"columnField": "c", "rowField": "r", "valueField": "v", "emptyValue": e}}]),
        );
        assert_eq!(t[0]["empty_value"], e);
    }
    let (_, t) = transforms(
        json!([{"id": "groupingToMatrix", "options": {"columnField": "c", "rowField": "r", "valueField": "v"}}]),
    );
    assert_eq!(t[0]["empty_value"], "null");
    // prepareTimeSeries and labelsToFields set the format.
    let mut p = panel(1, "timeseries", 0);
    p["transformations"] = json!([{"id": "prepareTimeSeries", "options": {"format": "multi"}}]);
    let r = run(&dash(vec![p], vec![]));
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "time_series");
    assert_eq!(first_panel(&doc(&r))["transforms"], json!([]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].class, Class::Changed);
    let mut p = panel(1, "table", 0);
    p["targets"][0]["format"] = json!(0);
    p["transformations"] = json!([{"id": "labelsToFields", "options": {}}]);
    let r = run(&dash(vec![p], vec![]));
    assert_eq!(first_panel(&doc(&r))["data"]["query"]["format"], "table");
    assert_eq!(r.items[0].class, Class::Changed);
    // Another form of prepareTimeSeries has no equal.
    let mut p = panel(1, "table", 0);
    p["transformations"] = json!([{"id": "prepareTimeSeries", "options": {"format": "wide"}}]);
    let r = run(&dash(vec![p], vec![]));
    assert_eq!(r.items[0].class, Class::Unsupported);
}

#[test]
fn transformation_flags_and_other_ids() {
    let (r, t) = transforms(json!([
        {"id": "seriesToRows", "options": {}},
        {"id": "groupingToMatrix", "disabled": true, "options": {"columnField": "c", "rowField": "r", "valueField": "v"}},
        {"id": "filterByValue", "filter": {"id": "byRefId", "options": "A"}, "options": {"type": "exclude", "match": "any", "filters": [{"fieldName": "f", "config": {"id": "isNull", "options": {}}}]}},
        {"id": "groupingToMatrix", "topic": "series", "options": {"columnField": "c", "rowField": "r", "valueField": "v"}},
    ]));
    assert_eq!(t.as_array().unwrap().len(), 2);
    assert_eq!(r.items.len(), 4, "{:#?}", r.items);
    assert!(r.items.iter().any(|i| i.class == Class::Dropped));
    assert!(r.items.iter().any(|i| i.member.ends_with("filter")));
    assert!(r.items.iter().any(|i| i.member.ends_with("topic")));
}

/// A `custom` member with the Grafana default is metadata (15.5). A member with another value stays an item.
#[test]
fn custom_members_with_grafana_defaults_are_metadata() {
    let defaults = json!({"custom": {
        "axisColorMode": "text", "axisPlacement": "auto", "gradientMode": "none", "hideFrom": {"legend": false, "tooltip": false, "viz": false},
        "lineStyle": {"fill": "solid"}, "barWidthFactor": 0.6, "stacking": {"group": "A", "mode": "none"}
    }});
    let (r, _) = defaults_of("timeseries", defaults);
    no_items(&r);
    assert!(r.metadata > 0);
    // A non-default value keeps its item. `axisColorMode` and `gradientMode` map since the later options (items P10 and P14).
    for (member, value) in [("barWidthFactor", json!(0.8)), ("insertNulls", json!(60000))] {
        let (r, _) = defaults_of("timeseries", json!({"custom": {member: value}}));
        assert_eq!(r.items.len(), 1, "{member}");
        assert_eq!(r.items[0].class, Class::Unsupported);
        assert_eq!(r.items[0].fallback, Some(Fallback::PropertyRemoved));
    }
    let (r, _) = defaults_of("timeseries", json!({"custom": {"stacking": {"group": "B", "mode": "none"}}}));
    assert_eq!(r.items.len(), 1);
    assert!(r.items[0].member.ends_with("stacking.group"));
    // A bar chart maps `fillOpacity` and `lineWidth` to its options (4.30, P03). The linear scale is metadata.
    let (r, _) = defaults_of(
        "barchart",
        json!({"custom": {"fillOpacity": 80, "lineWidth": 1, "scaleDistribution": {"type": "linear"}}}),
    );
    no_items(&r);
    let (r, _) = defaults_of("barchart", json!({"custom": {"fillOpacity": 50}}));
    no_items(&r);
    // In an override the member is not a default of the panel: it keeps its item. (`axisColorMode` maps since the later options, item P14.)
    let mut p = panel(1, "timeseries", 0);
    p["fieldConfig"] = json!({"defaults": {}, "overrides": [
        {"matcher": {"id": "byName", "options": "a"}, "properties": [{"id": "custom.barWidthFactor", "value": 0.6}]}]});
    let r = run(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
}

/// An option that no table names, with the Grafana default value, is metadata (15.5).
#[test]
fn options_with_grafana_defaults_are_metadata() {
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"justifyMode": "auto", "percentChangeColorMode": "standard", "wideLayout": true});
    let r = run_plain(&dash(vec![p], vec![]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    let mut p = panel(1, "table", 0);
    p["options"] = json!({"cellHeight": "sm"});
    let r = run_plain(&dash(vec![p], vec![]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    let mut p = panel(1, "timeseries", 0);
    p["options"] =
        json!({"tooltip": {"mode": "multi", "sort": "none"}, "annotations": {"clustering": -1, "multiLane": false}});
    let r = run_plain(&dash(vec![p], vec![]));
    assert!(r.items.is_empty(), "{:#?}", r.items);
    // Another value stays an item. `wideLayout` = `false` turns a Grafana default off: an item too (P04, the later options).
    // `percentChangeColorMode` = `inverted` maps to a member since the later options, item P11 (4.29): no item.
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"justifyMode": "auto", "percentChangeColorMode": "inverted", "wideLayout": false});
    let r = run_plain(&dash(vec![p], vec![]));
    assert_eq!(r.items.iter().filter(|i| i.member.starts_with("options.")).count(), 1, "{:#?}", r.items);
    assert!(r.items.iter().any(|i| i.member == "options.wideLayout"), "{:#?}", r.items);
    let mut p = panel(1, "table", 0);
    p["options"] = json!({"cellHeight": "lg"});
    let r = run_plain(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "options.cellHeight");
}

/// `justifyMode` gives the option `justify` of a stat.
#[test]
fn stat_justify_mode() {
    for (grafana, expected) in [("auto", None), ("center", Some("center"))] {
        let mut p = panel(1, "stat", 0);
        p["options"] = json!({"justifyMode": grafana});
        let r = run_plain(&dash(vec![p], vec![]));
        assert!(r.items.is_empty(), "{grafana}: {:#?}", r.items);
        let r = run(&dash(vec![panel_with_options("stat", json!({"justifyMode": grafana}))], vec![]));
        let out = doc(&r);
        assert_eq!(first_panel(&out)["options"].get("justify").and_then(Value::as_str), expected, "{grafana}");
    }
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"justifyMode": "left"});
    let r = run_plain(&dash(vec![p], vec![]));
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].member, "options.justifyMode");
    assert_eq!(r.items[0].class, Class::Unsupported);
    // An absent member is the default, with no item.
    let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![]));
    assert!(r.items.is_empty());
}

fn panel_with_options(ptype: &str, options: Value) -> Value {
    let mut p = panel(1, ptype, 0);
    p["options"] = options;
    p
}

/// P01: the unit overrides of the ClickHouse plugin examples (measure M2 of the later options): `string` on the column
/// "Version" of a stat, and `bool` on the columns "Is completed" and "Mutation" of a table. They map as they are, with no item.
#[test]
fn unit_overrides_of_the_plugin_examples() {
    let (r, o) = overrides_of(
        "stat",
        json!([{"matcher": {"id": "byName", "options": "Version"}, "properties": [{"id": "unit", "value": "string"}]}]),
    );
    no_items(&r);
    assert_eq!(o, json!([{"match": {"by": "name", "value": "Version"}, "set": {"unit": "string"}}]));
    let (r, o) = overrides(json!([
        {"matcher": {"id": "byName", "options": "Is completed"}, "properties": [{"id": "unit", "value": "bool"}]},
        {"matcher": {"id": "byName", "options": "Mutation"}, "properties": [{"id": "unit", "value": "bool"}]},
    ]));
    no_items(&r);
    assert_eq!(
        o,
        json!([
            {"match": {"by": "name", "value": "Is completed"}, "set": {"unit": "bool"}},
            {"match": {"by": "name", "value": "Mutation"}, "set": {"unit": "bool"}},
        ])
    );
}
