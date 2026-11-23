//! Wave H, step 0 (decision ID-K326, case I47 of 16.13): a `false` or a `0` that turns a Grafana default off (P04),
//! the old footer of a table (P06), `prepareTimeSeries` with the format `many` (P07), and the sizes of a bar gauge
//! with no manual size (P08).

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn one(p: Value) -> DashboardImport {
    run_plain(&dash(vec![p], vec![]))
}

fn with_custom(ptype: &str, custom: Value) -> Value {
    let mut p = panel(1, ptype, 0);
    p["fieldConfig"] = json!({"defaults": {"custom": custom}, "overrides": []});
    p
}

fn custom_member(key: &str, value: Value) -> Value {
    let mut c = json!({});
    c[key] = value;
    c
}

fn assert_class(r: &DashboardImport, member: &str, class: Class, fallback: Option<Fallback>) {
    let i = one_item(r, member);
    assert_eq!((i.class, i.fallback.clone()), (class, fallback), "{member}: {i:#?}");
}

// ------------------------------------------------------------------ P04: a value that turns a default off

#[test]
fn p04_grid_lines_off_is_an_item_in_each_type_with_an_axis() {
    // A `barchart` maps the member (P03): the test below.
    for t in ["timeseries", "histogram", "xychart"] {
        let r = one(with_custom(t, json!({"axisGridShow": false})));
        let member = "fieldConfig.defaults.custom.axisGridShow";
        assert_class(&r, member, Class::Unsupported, Some(Fallback::PropertyRemoved));
        let i = one_item(&r, member);
        assert!(i.reason.contains("grid lines"), "{t}: {}", i.reason);
        assert_eq!(i.value.as_deref(), Some("false"));
    }
    // An override that hides the grid lines of the axis of one series is an item too.
    let mut p = panel(1, "timeseries", 0);
    p["fieldConfig"] = json!({"defaults": {}, "overrides": [{"matcher": {"id": "byName", "options": "a"},
        "properties": [{"id": "custom.axisGridShow", "value": false}]}]});
    assert_class(
        &one(p),
        "fieldConfig.overrides.0.properties.0.value",
        Class::Unsupported,
        Some(Fallback::PropertyRemoved),
    );
    // `null` keeps the rule of 15.5. A type with no axis keeps it too.
    no_items(&one(with_custom("timeseries", json!({"axisGridShow": null}))));
    for t in ["table", "stat", "bargauge", "piechart"] {
        no_items(&one(with_custom(t, json!({"axisGridShow": false}))));
    }
}

#[test]
fn p04_wide_layout_off_is_an_item() {
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"wideLayout": false});
    let r = one(p);
    assert_class(&r, "options.wideLayout", Class::Unsupported, Some(Fallback::MemberRemoved));
    assert!(one_item(&r, "options.wideLayout").reason.contains("wide"), "{:#?}", r.items);
    // `true` is the Grafana default: metadata. Another value is an unknown value.
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"wideLayout": true});
    no_items(&one(p));
    let mut p = panel(1, "stat", 0);
    p["options"] = json!({"wideLayout": "yes"});
    assert_eq!(one_item(&one(p), "options.wideLayout").reason, "unknown member");
}

/// The bar chart maps its fill, its line and its grid lines (P03, 4.18, 4.30): `0` and `false` are never metadata.
#[test]
fn p04_a_bar_chart_with_no_fill_no_line_or_no_grid_keeps_the_value() {
    for key in ["fillOpacity", "lineWidth"] {
        let r = run(&dash(vec![with_custom("barchart", custom_member(key, json!(0)))], vec![]));
        no_items(&r);
        let out = doc(&r);
        let member = if key == "fillOpacity" { "fill_opacity" } else { "line_width" };
        assert_eq!(first_panel(&out)["options"][member], json!(0), "{key}");
    }
    let r = run(&dash(vec![with_custom("barchart", json!({"axisGridShow": false}))], vec![]));
    no_items(&r);
    assert_eq!(first_panel(&doc(&r))["fields"]["defaults"]["axis_grid"], json!(false));
    // In a time series, 0 maps to the options of 4.28.
    let r = run(&dash(vec![with_custom("timeseries", json!({"fillOpacity": 0, "lineWidth": 0}))], vec![]));
    no_items(&r);
    let out = doc(&r);
    let o = &first_panel(&out)["options"];
    assert_eq!((o["fill_opacity"].clone(), o["line_width"].clone()), (json!(0), json!(0)));
}

// ------------------------------------------------------------------ P06: the old footer of a table

fn table_footer(footer: Value) -> Value {
    let mut p = panel(1, "table", 0);
    p["options"] = json!({"showHeader": true, "footer": footer});
    p
}

/// The member `fields` of the imported panel, after the owner accepts each item.
fn fields_after(footer: Value) -> (DashboardImport, Value) {
    let d = dash(vec![table_footer(footer)], vec![]);
    let first = run_plain(&d);
    let out = doc(&run(&d));
    (first, first_panel(&out)["fields"].clone())
}

#[test]
fn p06_the_old_footer_moves_as_grafana_moves_it() {
    // `fields` empty: all the fields, so the defaults.
    let (r, f) = fields_after(json!({"show": true, "reducer": ["sum"], "fields": "", "countRows": false}));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_class(&r, "options.footer", Class::Changed, None);
    assert_eq!(f, json!({"defaults": {"footer": {"reducers": ["total"]}}, "overrides": []}));
    // The property takes its place in the canonical order of 4.18: `doc` checks that the bytes are canonical.
    let mut p = table_footer(json!({"show": true, "reducer": ["mean"], "fields": []}));
    p["fieldConfig"] = json!({"defaults": {"custom": {"filterable": true, "width": 120,
        "hideFrom": {"viz": true, "legend": false, "tooltip": false}}, "unit": "ms"}, "overrides": []});
    let out = doc(&run(&dash(vec![p], vec![])));
    assert_eq!(
        first_panel(&out)["fields"]["defaults"],
        json!({"unit": "ms", "width": 120, "filterable": true, "hidden": true, "footer": {"reducers": ["mean"]}})
    );
    // One field: an override with the matcher `name`. Two fields: the matcher `names`.
    let (_, f) = fields_after(json!({"show": true, "reducer": ["sum"], "fields": ["x"], "countRows": false}));
    assert_eq!(
        f,
        json!({"defaults": {}, "overrides": [{"match": {"by": "name", "value": "x"}, "set": {"footer": {"reducers": ["total"]}}}]})
    );
    let (_, f) = fields_after(json!({"show": true, "reducer": ["max", "mean"], "fields": ["x", "y", "x"]}));
    assert_eq!(
        f["overrides"],
        json!([{"match": {"by": "names", "names": ["x", "y"], "mode": "include"}, "set": {"footer": {"reducers": ["max", "mean"]}}}])
    );
    // countRows with the reducer count: Grafana counts the rows, whatever the fields. The reason names the difference.
    let (r, f) = fields_after(json!({"show": true, "reducer": ["count"], "fields": ["x"], "countRows": true}));
    assert_eq!(f, json!({"defaults": {"footer": {"reducers": ["count"]}}, "overrides": []}));
    assert!(one_item(&r, "options.footer").reason.contains("row count"), "{:#?}", r.items);
    // countRows with another reducer: Grafana reads the fields.
    let (_, f) = fields_after(json!({"show": true, "reducer": ["sum"], "fields": ["x"], "countRows": true}));
    assert_eq!(f["overrides"][0]["match"], json!({"by": "name", "value": "x"}));
}

#[test]
fn p06_the_old_footer_faults() {
    // A footer that is not shown is metadata. The pages that its member enablePagination turns on are an item.
    no_items(&one(table_footer(json!({"show": false, "reducer": ["sum"], "fields": "", "countRows": false}))));
    no_items(&one(table_footer(json!({"show": false, "enablePagination": false}))));
    let r = one(table_footer(json!({"show": false, "reducer": ["sum"], "fields": "", "enablePagination": true})));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    let i = one_item(&r, "options.footer.enablePagination");
    assert_eq!(
        (i.class, i.fallback.clone()),
        (Class::Unsupported, Some(Fallback::other("the panel shows all the rows")))
    );
    // A reducer with no equal: the item of the footer row of 15.6, and no move.
    let r = one(table_footer(json!({"show": true, "reducer": ["allValues"], "fields": ""})));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_class(&r, "options.footer.reducer.0", Class::Unsupported, Some(Fallback::other("item removed")));
    let r = one(table_footer(json!({"show": true, "reducer": "sum"})));
    assert_class(&r, "options.footer.reducer", Class::Unsupported, Some(Fallback::PropertyRemoved));
    // No reducer: no footer, no item.
    no_items(&one(table_footer(json!({"show": true, "fields": ""}))));
    // `fields` must be a list of field names.
    for fields in [json!("x"), json!(["x", 3]), json!([""]), json!(7)] {
        let r = one(table_footer(json!({"show": true, "reducer": ["sum"], "fields": fields})));
        assert_eq!(r.items.len(), 1, "{fields}: {:#?}", r.items);
        assert_class(&r, "options.footer.fields", Class::Unsupported, Some(Fallback::other("footer removed")));
    }
    // An unknown member of a footer that is shown keeps the rule of 15.5.
    let r = one(table_footer(json!({"show": true, "reducer": ["sum"], "fields": "", "colour": "red"})));
    assert_eq!(one_item(&r, "options.footer.colour").reason, "unknown member");
}

/// The counts of the members of an old footer: (mapped, metadata) above the same table with no footer.
fn footer_counts(footer: Value) -> (u32, u32) {
    let mut p = panel(1, "table", 0);
    p["options"] = json!({"showHeader": true});
    let base = one(p);
    let r = one(table_footer(footer));
    (r.mapped - base.mapped, r.metadata - base.metadata)
}

#[test]
fn p06_each_member_of_the_old_footer_is_counted_once() {
    // `show` and the reducer map. `fields` = "" and `countRows` = false have no effect.
    assert_eq!(footer_counts(json!({"show": true, "reducer": ["sum"], "fields": "", "countRows": false})), (2, 2));
    // With `countRows` and `count`, `countRows` and the reducer map, and Grafana does not read `fields`.
    assert_eq!(footer_counts(json!({"show": true, "reducer": ["count"], "fields": ["x"], "countRows": true})), (3, 1));
    // `countRows` with another reducer has no effect. A name of `fields` maps.
    assert_eq!(footer_counts(json!({"show": true, "reducer": ["sum"], "fields": ["x"], "countRows": true})), (3, 1));
    // No reducer: no footer, and each other member is metadata.
    assert_eq!(footer_counts(json!({"show": true, "reducer": null, "fields": "", "countRows": false})), (1, 3));
    assert_eq!(footer_counts(json!({"show": true})), (1, 0));
    // A footer that is not shown is one metadata member.
    assert_eq!(footer_counts(json!({"show": false, "reducer": ["sum"], "fields": ""})), (0, 1));
}

#[test]
fn p06_the_names_of_fields_have_a_cap_and_linear_work() {
    let names: Vec<String> = (0..64).map(|i| format!("f{i}")).collect();
    let (r, f) = fields_after(json!({"show": true, "reducer": ["sum"], "fields": names}));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(f["overrides"][0]["match"]["names"].as_array().map(Vec::len), Some(64));
    // 65 different names: the footer is removed with one item.
    let names: Vec<String> = (0..65).map(|i| format!("f{i}")).collect();
    let r = one(table_footer(json!({"show": true, "reducer": ["sum"], "fields": names})));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_class(&r, "options.footer.fields", Class::Unsupported, Some(Fallback::other("footer removed")));
    assert!(one_item(&r, "options.footer.fields").reason.contains("1 to 64 field names"), "{:#?}", r.items);
    // 200,000 names that repeat give one name. 200,000 different names stop at the 65th.
    let same = vec!["x"; 200_000];
    let (_, f) = fields_after(json!({"show": true, "reducer": ["sum"], "fields": same}));
    assert_eq!(f["overrides"][0]["match"], json!({"by": "name", "value": "x"}));
    let many: Vec<String> = (0..200_000).map(|i| format!("f{i}")).collect();
    let r = one(table_footer(json!({"show": true, "reducer": ["sum"], "fields": many})));
    assert_class(&r, "options.footer.fields", Class::Unsupported, Some(Fallback::other("footer removed")));
}

// ------------------------------------------------------------------ P07: prepareTimeSeries with the format many

#[test]
fn p07_prepare_time_series_many_is_multi() {
    for (format, class) in [
        ("many", Class::Changed),
        ("multi", Class::Changed),
        ("wide", Class::Unsupported),
        ("long", Class::Unsupported),
    ] {
        let mut p = panel(1, "timeseries", 0);
        p["transformations"] = json!([{"id": "prepareTimeSeries", "options": {"format": format}}]);
        let d = dash(vec![p], vec![]);
        let r = run_plain(&d);
        assert_eq!(one_item(&r, "transformations.0").class, class, "{format}");
        if class == Class::Changed {
            let out = doc(&run(&d));
            assert_eq!(first_panel(&out)["data"]["query"]["format"], "time_series", "{format}");
        }
    }
}

// ------------------------------------------------------------------ P08: the sizes of a bar gauge

fn bargauge(extra: Value) -> Value {
    let mut p = panel(1, "bargauge", 0);
    let mut o = json!({"reduceOptions": {"values": false, "calcs": ["lastNotNull"], "fields": ""},
        "orientation": "horizontal", "displayMode": "gradient", "showUnfilled": true});
    for (k, v) in extra.as_object().unwrap() {
        o[k] = v.clone();
    }
    p["options"] = o;
    p
}

#[test]
fn p08_bar_gauge_sizes_with_no_manual_size_are_metadata() {
    for extra in [
        json!({"minVizHeight": 10, "minVizWidth": 0, "maxVizHeight": 300}),
        json!({"minVizHeight": 10}),
        json!({"sizing": "auto", "minVizHeight": 20, "minVizWidth": 10, "maxVizHeight": 40}),
    ] {
        no_items(&one(bargauge(extra)));
    }
    // With the manual size, Grafana reads the sizes: each one stays an item (H18).
    let r = one(bargauge(json!({"sizing": "manual", "minVizHeight": 20, "minVizWidth": 10, "maxVizHeight": 40})));
    for m in ["options.sizing", "options.minVizHeight", "options.minVizWidth", "options.maxVizHeight"] {
        assert_class(&r, m, Class::Unsupported, Some(Fallback::MemberRemoved));
    }
    assert_eq!(r.items.len(), 4, "{:#?}", r.items);
    // Another value of sizing is an unknown value. Grafana then does not read the sizes: they are metadata.
    let r = one(bargauge(json!({"sizing": "fixed", "minVizHeight": 20})));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(one_item(&r, "options.sizing").reason, "unknown member");
}

// ------------------------------------------------------------------ the golden case

/// The text of a golden file: the document, then one line for each item.
fn golden_text(r: &DashboardImport) -> String {
    let mut s = serde_json::to_string_pretty(&doc(r)).expect("json");
    s.push('\n');
    for i in &r.items {
        let fallback = i.fallback.as_ref().map_or("-", |f| f.text());
        s.push_str(&format!("{} [{}] {} | {}\n", i.id, i.class.as_str(), i.reason, fallback));
    }
    s
}

#[test]
fn golden_case_of_the_four_items() {
    let mut bar = with_custom("barchart", json!({"axisGridShow": false, "fillOpacity": 0, "lineWidth": 0}));
    bar["options"] = json!({"orientation": "vertical", "showValue": "auto", "stacking": "none"});
    let mut stat = panel(2, "stat", 4);
    stat["options"] = json!({"wideLayout": false, "textMode": "value_and_name"});
    let mut table = table_footer(json!({"show": true, "reducer": ["sum"], "fields": ["b"], "countRows": false}));
    table["id"] = json!(3);
    table["gridPos"]["y"] = json!(8);
    let mut ts = with_custom("timeseries", json!({"axisGridShow": false}));
    ts["id"] = json!(4);
    ts["gridPos"]["y"] = json!(12);
    ts["transformations"] = json!([{"id": "prepareTimeSeries", "options": {"format": "many"}}]);
    let mut gauge = bargauge(json!({"minVizHeight": 10, "minVizWidth": 0, "maxVizHeight": 300}));
    gauge["id"] = json!(5);
    gauge["gridPos"]["y"] = json!(16);
    let d = dash(vec![bar, stat, table, ts, gauge], vec![]);
    let first = run_plain(&d);
    let accepted = run(&d);
    assert_eq!(first.items.len(), accepted.items.len());
    check_golden("h0_defaults.txt", &golden_text(&accepted));
}
