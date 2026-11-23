//! Wave H, item P11 (decision ID-K326): the importer maps the percent change and the text sizes of a Grafana stat
//! (15.6, 4.29). The Grafana defaults stay metadata, as the plugin samples write them (`showPercentChange: false`).

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn stat(options: Value) -> Value {
    let mut p = panel(1, "stat", 0);
    p["options"] = options;
    p
}

fn one(options: Value) -> DashboardImport {
    run_plain(&dash(vec![stat(options)], vec![]))
}

fn options_of(options: Value) -> Value {
    first_panel(&doc(&run(&dash(vec![stat(options)], vec![]))))["options"].clone()
}

const NEW: [&str; 4] = ["percent_change", "percent_change_color", "title_size", "value_size"];

#[test]
fn the_four_members_map() {
    let given = json!({"showPercentChange": true, "percentChangeColorMode": "inverted", "text": {"titleSize": 20, "valueSize": 40}});
    let r = one(given.clone());
    no_items(&r);
    let o = options_of(given);
    assert_eq!(o["percent_change"], json!(true));
    assert_eq!(o["percent_change_color"], json!("inverted"));
    assert_eq!((o["title_size"].clone(), o["value_size"].clone()), (json!(20), json!(40)));
    let o = options_of(json!({"showPercentChange": true, "percentChangeColorMode": "same_as_value"}));
    assert_eq!(o["percent_change_color"], json!("same_as_value"));
    // The ends of the range of the Grafana editor, and a whole number written with a point.
    let o = options_of(json!({"text": {"titleSize": 1, "valueSize": 200.0}}));
    assert_eq!((o["title_size"].clone(), o["value_size"].clone()), (json!(1), json!(200)));
}

#[test]
fn the_grafana_defaults_are_metadata_and_give_no_member() {
    for given in [
        json!({"showPercentChange": false, "percentChangeColorMode": "standard", "text": {}}),
        json!({"text": {"titleSize": 0, "valueSize": null}}),
        json!({"text": {"valueSize": 0}}),
        json!({"showPercentChange": null, "text": null}),
    ] {
        let r = one(given.clone());
        no_items(&r);
        let o = options_of(given.clone());
        for key in NEW {
            assert!(o.get(key).is_none(), "{given}: {key} in {o}");
        }
    }
    // Each default counts as metadata, as before the item.
    let plain = one(json!({}));
    let r = one(json!({"showPercentChange": false, "percentChangeColorMode": "standard", "text": {}}));
    assert_eq!(r.metadata, plain.metadata + 3);
    assert_eq!(r.mapped, plain.mapped);
    let r = one(json!({"showPercentChange": true, "percentChangeColorMode": "inverted", "text": {"titleSize": 9}}));
    assert_eq!(r.mapped, plain.mapped + 3);
}

#[test]
fn bad_values_are_unsupported_and_leave_the_default() {
    for (given, member, reason) in [
        (json!({"showPercentChange": "yes"}), "options.showPercentChange", "true or false"),
        (json!({"showPercentChange": 1}), "options.showPercentChange", "true or false"),
        (json!({"percentChangeColorMode": "rainbow"}), "options.percentChangeColorMode", "value table"),
        (json!({"text": {"titleSize": 12.5}}), "options.text.titleSize", "integer from 1 to 200"),
        (json!({"text": {"titleSize": 201}}), "options.text.titleSize", "integer from 1 to 200"),
        (json!({"text": {"valueSize": -3}}), "options.text.valueSize", "integer from 1 to 200"),
        (json!({"text": {"valueSize": "40"}}), "options.text.valueSize", "integer from 1 to 200"),
        (json!({"text": {"percentSize": 30}}), "options.text.percentSize", "percent change"),
    ] {
        let r = one(given.clone());
        let i = one_item(&r, member);
        assert_eq!(i.class, Class::Unsupported, "{given}: {i:#?}");
        assert!(i.reason.contains(reason), "{given}: {}", i.reason);
        let o = options_of(given.clone());
        for key in NEW {
            assert!(o.get(key).is_none(), "{given}: {key} in {o}");
        }
    }
    // The fallback of a bad size and of the percent size is "member removed": the page fits the text.
    let r = one(json!({"text": {"valueSize": 999, "percentSize": 30}}));
    for i in &r.items {
        assert_eq!(i.fallback, Some(Fallback::MemberRemoved), "{i:#?}");
    }
    // A `text` that is not an object, and a member of `text` that no row names.
    let r = one(json!({"text": "big"}));
    assert_eq!(one_item(&r, "options.text").class, Class::Unsupported);
    let r = one(json!({"text": {"titleSize": 20, "frobnicate": 3}}));
    assert_eq!(one_item(&r, "options.text.frobnicate").class, Class::Unsupported);
    assert_eq!(options_of(json!({"text": {"titleSize": 20, "frobnicate": 3}}))["title_size"], json!(20));
}

#[test]
fn a_valid_size_stays_when_the_other_one_is_bad() {
    let given = json!({"showPercentChange": true, "text": {"titleSize": 300, "valueSize": 48}});
    let r = one(given.clone());
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    let o = options_of(given);
    assert_eq!(
        (o.get("title_size"), o["value_size"].clone(), o["percent_change"].clone()),
        (None, json!(48), json!(true))
    );
}
