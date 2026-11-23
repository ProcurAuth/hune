// SPDX-License-Identifier: MIT OR Apache-2.0
//! Items P02: the data link tokens of Grafana 12 (9.18) and the two links of the table
//! "Traces table" of the dashboard "Simple ClickHouse OTel Dashboard" of the ClickHouse plugin 4.5.1 (15.6).

mod common;

use common::*;
use hune_grafana::{LinkPrefix, import_dashboard};
use serde_json::{Value, json};

/// The two overrides of the plugin dashboard, as it ships them: each URL has U+FEFF around `&`.
fn otel_overrides() -> Value {
    json!([
        {"matcher": {"id": "byName", "options": "Trace ID"}, "properties": [
            {"id": "custom.width", "value": 207},
            {"id": "links", "value": [{"title": "__value.raw",
                "url": "/d/8klBUGfVk/otel-traces?${__url_time_range}\u{feff}&\u{feff}${serviceName:queryparam}\u{feff}&var-trace_id=${__value.raw}"}]}
        ]},
        {"matcher": {"id": "byName", "options": "Service Name"}, "properties": [
            {"id": "custom.width", "value": 184},
            {"id": "links", "value": [{"title": "__value.raw",
                "url": "/d/8klBUGfVk/otel-traces?${__url_time_range}\u{feff}&\u{feff}${trace_id:queryparam}\u{feff}&var-serviceName=${__value.raw}"}]}
        ]}
    ])
}

fn otel_dashboard() -> Value {
    let mut p = panel(4, "table", 0);
    p["title"] = json!("Traces table");
    p["fieldConfig"] = json!({"defaults": {}, "overrides": otel_overrides()});
    dash(vec![p], vec![])
}

fn urls(d: &Value) -> Vec<String> {
    first_panel(d)["fields"]["overrides"]
        .as_array()
        .expect("overrides")
        .iter()
        .map(|o| o["set"]["links"][0]["url"].as_str().expect("url").to_owned())
        .collect()
}

#[test]
fn the_two_otel_links_import_with_no_item_and_lose_the_u_feff() {
    let r = run_plain(&otel_dashboard());
    no_items(&r);
    assert_eq!(
        urls(&doc(&r)),
        [
            "/d/8klBUGfVk/otel-traces?${__url_time_range}&${serviceName:queryparam}&var-trace_id=${__value.raw}",
            "/d/8klBUGfVk/otel-traces?${__url_time_range}&${trace_id:queryparam}&var-serviceName=${__value.raw}",
        ]
    );
}

#[test]
fn the_link_prefix_rewrites_the_two_otel_links_to_the_hune_uid() {
    let mut o = base_options();
    o.uid_map.insert("8klBUGfVk".to_owned(), "otel".to_owned());
    o.link_prefix = Some(LinkPrefix::new("/", "/dashboards/").expect("prefix"));
    let r = import_dashboard(&otel_dashboard(), &o);
    no_items(&r);
    assert_eq!(
        urls(&doc(&r)),
        [
            "/dashboards/d/otel/otel-traces?${__url_time_range}&${serviceName:queryparam}&var-trace_id=${__value.raw}",
            "/dashboards/d/otel/otel-traces?${__url_time_range}&${trace_id:queryparam}&var-serviceName=${__value.raw}",
        ]
    );
}

#[test]
fn each_token_of_grafana_12_maps_and_another_format_stays_unsupported() {
    let url = "/x?t=${__value.time}&n=${__value.numeric}&s=${__series.name}&z=${__timezone}&u=${__dashboard.uid}&${env:queryparam}";
    let mut p = panel(1, "table", 0);
    p["links"] = json!([{"title": "Panel", "url": url}]);
    p["fieldConfig"] = json!({"defaults": {"links": [{"title": "Cell", "url": url}]}, "overrides": []});
    let r = run_plain(&dash(vec![p], vec![]));
    no_items(&r);
    let d = doc(&r);
    assert_eq!(first_panel(&d)["links"][0]["url"], url);
    assert_eq!(first_panel(&d)["fields"]["defaults"]["links"][0]["url"], url);
    for bad in ["/x?${env:raw}", "/x?${__value.calc}", "/x?${__field.labels.host}", "/x?${__from:queryparam}"] {
        let mut p = panel(1, "table", 0);
        p["links"] = json!([{"title": "Panel", "url": bad}]);
        let r = run_plain(&dash(vec![p], vec![]));
        let item = one_item(&r, "links");
        assert!(item.reason.contains("is not in the list of 9.18"), "{bad}: {}", item.reason);
    }
}
