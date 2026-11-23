//! The rows of 15.6 for the variable kinds `interval` and `constant` (F27, ID-K267, A111 of 16.8): the forms v1,
//! v2alpha1 and v2beta1.

mod common;

use common::*;
use hune_grafana::{Class, DashboardImport, Fallback};
use serde_json::{Value, json};

fn interval_var(extra: Value) -> Value {
    let mut v = json!({"type": "interval", "name": "iv", "label": "", "hide": 0, "query": "1m,10m,1h,1d", "auto": true,
        "auto_count": 30, "auto_min": "10s", "current": {"text": "Auto", "value": "$__auto_interval_iv"},
        "options": [{"text": "Auto", "value": "$__auto_interval_iv"}, {"text": "1m", "value": "1m"}], "refresh": 2});
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

fn constant_var(extra: Value) -> Value {
    let mut v = json!({"type": "constant", "name": "k", "label": "", "hide": 2, "query": "eu-1",
        "current": {}, "options": []});
    for (k, val) in extra.as_object().unwrap() {
        v[k] = val.clone();
    }
    v
}

fn import(vars: Vec<Value>) -> DashboardImport {
    let mut p = panel(1, "stat", 0);
    p["targets"] = json!([target(
        "SELECT toStartOfInterval(ts, toIntervalSecond(parseTimeDelta('${iv}'))) AS t, '${k}' AS n FROM x"
    )]);
    run(&dash(vec![p], vars))
}

fn var0(r: &DashboardImport) -> Value {
    doc(r)["variables"][0].clone()
}

fn by_member<'a>(r: &'a DashboardImport, end: &str) -> Vec<&'a hune_grafana::ReportItem> {
    r.items.iter().filter(|i| i.member.ends_with(end)).collect()
}

#[test]
fn an_interval_variable_maps_all_its_members() {
    let r = import(vec![interval_var(json!({})), constant_var(json!({}))]);
    let d = doc(&r);
    assert_eq!(
        d["variables"][0],
        json!({"kind": "interval", "name": "iv", "label": null, "hide": "none",
            "intervals": ["1m", "10m", "1h", "1d"], "auto": true, "auto_count": 30, "auto_min": "10s",
            "default": "$__auto"})
    );
    assert_eq!(
        d["variables"][1],
        json!({"kind": "constant", "name": "k", "label": null, "hide": "variable", "value": "eu-1"})
    );
    // The only item: the older value of Auto is written as `$__auto`.
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
    assert_eq!(r.items[0].class, Class::Changed);
    assert!(r.items[0].member.ends_with("current.value"), "{}", r.items[0].member);
    // `$__auto` itself has no item.
    let r = import(vec![interval_var(json!({"current": {"text": "Auto", "value": "$__auto"}}))]);
    no_items(&r);
    assert_eq!(var0(&r)["default"], "$__auto");
}

#[test]
fn the_list_is_split_as_grafana_does_and_the_items_that_break_4_10a_are_removed_and_listed() {
    // `1.5h` gives `1` and `5h`; `1M` is an item of Grafana and not of Hune; `-1m` gives `1m`; a repeat stays once.
    let r = import(vec![interval_var(json!({"query": "1m,1.5h,1M;-1m 10m", "current": {}}))]);
    assert_eq!(var0(&r)["intervals"], json!(["1m", "5h", "10m"]));
    let items = by_member(&r, "query");
    assert_eq!(items.len(), 2, "{:#?}", r.items);
    for item in &items {
        assert_eq!(item.class, Class::Unsupported);
        assert_eq!(item.fallback, Some(Fallback::other("the item is removed")));
    }
    assert!(items[0].reason.contains("\"1\"") && items[1].reason.contains("\"1M\""), "{items:#?}");
    // The items that break the grammar: `abc`, `5x`, `0m`, `1M`, 366 days.
    for (list, kept) in [("1m,abc,5x,0m,1M,366d,2y", json!(["1m"])), ("7d,1w,52w,53w", json!(["7d", "1w", "52w"]))] {
        let r = import(vec![interval_var(json!({"query": list, "current": {}}))]);
        assert_eq!(var0(&r)["intervals"], kept, "{list}");
        assert!(r.items.iter().all(|i| i.class == Class::Unsupported), "{list}: {:#?}", r.items);
    }
    // The item list of the unit `M` alone: no item is left.
    for list in ["1M", "abc,5x", "0m;zz"] {
        let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![interval_var(json!({"query": list, "current": {}}))]));
        assert!(!r.importable, "{list}");
        let item = &r.items[r.items.len() - 1];
        assert_eq!(item.fallback, Some(Fallback::DashboardNotImported), "{list}: {:#?}", r.items);
    }
}

#[test]
fn an_empty_list_gives_the_default_list_of_grafana_and_more_than_50_items_the_first_50() {
    for query in [json!(""), json!(null), json!(" ,; ")] {
        let r = import(vec![interval_var(json!({"query": query, "current": {}}))]);
        assert_eq!(var0(&r)["intervals"], json!(["1m", "10m", "30m", "1h", "6h", "12h", "1d", "7d", "14d", "30d"]));
        let items = by_member(&r, "query");
        assert_eq!((items.len(), items[0].class), (1, Class::Changed));
    }
    let list = (1..=60).map(|i| format!("{i}m")).collect::<Vec<_>>().join(",");
    let r = import(vec![interval_var(json!({"query": list, "current": {}}))]);
    let items = var0(&r)["intervals"].as_array().unwrap().clone();
    assert_eq!(items.len(), 50);
    assert_eq!((items[0].clone(), items[49].clone()), (json!("1m"), json!("50m")));
    let item = &by_member(&r, "query")[0];
    assert_eq!((item.class, item.fallback.clone()), (Class::Unsupported, Some(Fallback::other("the first 50 items"))));
    // A query that is not a text stops the dashboard.
    let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![interval_var(json!({"query": 5}))]));
    assert!(!r.importable);
}

#[test]
fn the_members_of_auto() {
    // An absent `auto` is false. The other two members keep their defaults, with no item.
    let mut v = interval_var(json!({"current": {}}));
    for k in ["auto", "auto_count", "auto_min"] {
        v.as_object_mut().unwrap().remove(k);
    }
    let r = import(vec![v]);
    no_items(&r);
    let var = var0(&r);
    assert_eq!(
        (var["auto"].clone(), var["auto_count"].clone(), var["auto_min"].clone()),
        (json!(false), json!(30), json!("10s"))
    );
    // `auto_count`: 1 and 1,000 are kept. 0, 1,001, a fraction and a text give 30 with an item.
    for (count, kept, listed) in [
        (json!(1), 1, false),
        (json!(1000), 1000, false),
        (json!(0), 30, true),
        (json!(1001), 30, true),
        (json!("abc"), 30, true),
        (json!(-5), 30, true),
    ] {
        let r = import(vec![interval_var(json!({"auto_count": count.clone(), "current": {}}))]);
        assert_eq!(var0(&r)["auto_count"], kept, "{count}");
        let items = by_member(&r, "auto_count");
        assert_eq!(items.len(), usize::from(listed), "{count}: {:#?}", r.items);
        if listed {
            assert_eq!((items[0].class, items[0].fallback.clone()), (Class::Unsupported, Some(Fallback::other("30"))));
        }
    }
    // `auto_min`: a table text is kept. `90s` gives `1m`, as Grafana does. An invalid text gives `10s`.
    for (min, kept, class) in [
        ("1ms", "1ms", None),
        ("1y", "1y", None),
        ("30d", "30d", None),
        ("90s", "1m", Some(Class::Changed)),
        ("3h", "3h", None),
        ("4h", "3h", Some(Class::Changed)),
        ("1w", "7d", Some(Class::Changed)),
        ("2y", "1y", Some(Class::Changed)),
        ("zz", "10s", Some(Class::Changed)),
        ("1M", "10s", Some(Class::Changed)),
        ("", "10s", Some(Class::Changed)),
    ] {
        let r = import(vec![interval_var(json!({"auto_min": min, "current": {}}))]);
        assert_eq!(var0(&r)["auto_min"], kept, "{min:?}");
        let items = by_member(&r, "auto_min");
        assert_eq!(items.first().map(|i| i.class), class, "{min:?}: {:#?}", r.items);
    }
}

#[test]
fn the_current_value_gives_the_default() {
    for (value, auto, kept, class) in [
        (json!("$__auto"), true, json!("$__auto"), None),
        (json!("$__auto_interval_iv"), true, json!("$__auto"), Some(Class::Changed)),
        (json!("$__auto_interval_other"), true, json!("$__auto"), Some(Class::Changed)),
        (json!("$__auto"), false, json!(null), Some(Class::Changed)),
        (json!("10m"), true, json!("10m"), None),
        (json!("10m"), false, json!("10m"), None),
        (json!("7m"), true, json!(null), Some(Class::Changed)),
        (json!(["1h"]), true, json!("1h"), None),
        (json!(""), true, json!(null), None),
        (json!(null), true, json!(null), None),
    ] {
        let r = import(vec![interval_var(json!({"auto": auto, "current": {"text": "x", "value": value.clone()}}))]);
        assert_eq!(var0(&r)["default"], kept, "{value} {auto}");
        let items = by_member(&r, "current.value");
        assert_eq!(items.first().map(|i| i.class), class, "{value} {auto}: {:#?}", r.items);
    }
    // An absent `current` gives `null`.
    let mut v = interval_var(json!({}));
    v.as_object_mut().unwrap().remove("current");
    let r = import(vec![v]);
    no_items(&r);
    assert_eq!(var0(&r)["default"], json!(null));
}

#[test]
fn a_constant_is_always_hidden_and_a_visible_one_is_listed() {
    for (hide, listed) in [
        (json!(0), true),
        (json!(1), true),
        (json!(2), false),
        (json!("dontHide"), true),
        (json!("hideVariable"), false),
    ] {
        let r = import(vec![constant_var(json!({"hide": hide.clone()}))]);
        assert_eq!(var0(&r)["hide"], "variable", "{hide}");
        let items = by_member(&r, "hide");
        assert_eq!(items.len(), usize::from(listed), "{hide}: {:#?}", r.items);
        if listed {
            assert_eq!(items[0].class, Class::Changed);
        }
    }
    // An absent `hide` is 0 in Grafana: the constant shows there.
    let mut v = constant_var(json!({}));
    v.as_object_mut().unwrap().remove("hide");
    let r = import(vec![v]);
    assert_eq!(var0(&r)["hide"], "variable");
    assert_eq!(by_member(&r, "hide").len(), 1);
}

#[test]
fn the_value_of_a_constant_and_its_bounds() {
    for (value, kept) in
        [("", ""), ("x", "x"), ("a'b\"c$d\\e;--x", "a'b\"c$d\\e;--x"), (&"a".repeat(1024)[..], &"a".repeat(1024)[..])]
    {
        let r = import(vec![constant_var(json!({"query": value}))]);
        assert_eq!(var0(&r)["value"], kept);
        no_items(&r);
    }
    // An absent value is an empty text.
    let mut v = constant_var(json!({}));
    v.as_object_mut().unwrap().remove("query");
    assert_eq!(var0(&import(vec![v]))["value"], "");
    // 1,025 bytes, a control character and a value that is not a text: the dashboard is not imported.
    for bad in [json!("a".repeat(1025)), json!("a\nb"), json!("a\u{7f}b"), json!("a\u{0}b"), json!(5), json!(true)] {
        let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![constant_var(json!({"query": bad.clone()}))]));
        assert!(!r.importable, "{bad}");
        assert_eq!(r.items.last().unwrap().fallback, Some(Fallback::DashboardNotImported), "{bad}");
    }
    // The stored current value and options are dropped, and listed when they are not empty.
    let r = import(vec![constant_var(
        json!({"current": {"text": "eu-1", "value": "eu-1"}, "options": [{"text": "eu-1", "value": "eu-1"}]}),
    )]);
    assert_eq!(var0(&r)["value"], "eu-1");
    assert_eq!(r.items.iter().filter(|i| i.class == Class::Dropped).count(), 2, "{:#?}", r.items);
    let r = import(vec![constant_var(json!({"description": "A note", "skipUrlSync": true}))]);
    assert_eq!(r.items.iter().filter(|i| i.class == Class::Dropped).count(), 2, "{:#?}", r.items);
    // `skipUrlSync` false and an empty description are metadata.
    no_items(&import(vec![constant_var(json!({"description": "", "skipUrlSync": false}))]));
}

#[test]
fn the_members_of_another_kind_are_metadata() {
    let r = import(vec![interval_var(
        json!({"multi": true, "includeAll": true, "allValue": "x", "sort": 1, "regex": "/a/", "refresh": 2, "current": {}}),
    )]);
    no_items(&r);
    let r = import(vec![constant_var(json!({"multi": true, "includeAll": true, "sort": 1, "allowCustomValue": true}))]);
    no_items(&r);
    // Another member is still reported.
    let r = run_plain(&dash(vec![panel(1, "stat", 0)], vec![interval_var(json!({"nonsense": "x", "current": {}}))]));
    assert!(r.items.iter().any(|i| i.member.ends_with("nonsense")), "{:#?}", r.items);
    // A member `auto` on a `custom` variable is not read by the interval rows.
    let r = run_plain(&dash(
        vec![panel(1, "stat", 0)],
        vec![
            json!({"type": "custom", "name": "c", "query": "a,b", "multi": false, "includeAll": false, "auto": true, "current": {"value": "a"}}),
        ],
    ));
    assert!(r.items.iter().any(|i| i.member.ends_with("auto")), "{:#?}", r.items);
}

#[test]
fn the_imported_sql_passes_the_checks_of_the_new_kinds() {
    // An interval and a constant in a string, a quoted name and code: the checks of 7.18 accept them.
    let mut p = panel(1, "stat", 0);
    p["targets"] = json!([target("SELECT 1 FROM `${k}` WHERE a = '${iv}' AND b = ${iv:singlequote}")]);
    let r = run(&dash(vec![p], vec![interval_var(json!({"current": {}})), constant_var(json!({}))]));
    let d = doc(&r);
    assert_eq!(d["variables"].as_array().unwrap().len(), 2);
    assert!(r.importable);
}

#[test]
fn the_v2_forms() {
    for form in ["alpha1", "beta1"] {
        let interval = json!({"kind": "IntervalVariable", "spec": {"name": "iv", "query": "1m,1M,10m", "auto": true, "auto_min": "90s",
            "auto_count": 50, "hide": "hideLabel", "label": "Bucket", "refresh": 2,
            "current": {"text": "Auto", "value": "$__auto_interval_iv"}, "options": []}});
        let constant = json!({"kind": "ConstantVariable", "spec": {"name": "k", "query": "eu-1", "hide": "dontHide",
            "current": {"text": "eu-1", "value": "eu-1"}}});
        let d = json!({
            "apiVersion": format!("dashboard.grafana.app/v2{form}"),
            "kind": "Dashboard",
            "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
            "spec": {"title": "V2 test", "description": "", "tags": [], "cursorSync": "Off",
                "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
                "variables": [interval, constant], "elements": {}, "layout": {"kind": "RowsLayout", "spec": {"rows": []}}}
        });
        let r = run(&d);
        let out = doc(&r);
        assert_eq!(
            out["variables"][0],
            json!({"kind": "interval", "name": "iv", "label": "Bucket", "hide": "label", "intervals": ["1m", "10m"],
                "auto": true, "auto_count": 50, "auto_min": "1m", "default": "$__auto"}),
            "{form}"
        );
        assert_eq!(
            out["variables"][1],
            json!({"kind": "constant", "name": "k", "label": null, "hide": "variable", "value": "eu-1"}),
            "{form}"
        );
        // The item `1M`, the minimum `90s`, the old value of Auto, the constant that Grafana shows, the dropped current value.
        let classes: Vec<(Class, String)> = r.items.iter().map(|i| (i.class, i.member.clone())).collect();
        assert!(classes.iter().any(|(c, m)| *c == Class::Unsupported && m.ends_with("query")), "{form}: {classes:?}");
        assert!(classes.iter().any(|(c, m)| *c == Class::Changed && m.ends_with("auto_min")), "{form}: {classes:?}");
        assert!(classes.iter().any(|(c, m)| *c == Class::Changed && m.ends_with("hide")), "{form}: {classes:?}");
        assert!(classes.iter().any(|(c, m)| *c == Class::Dropped && m.ends_with("current")), "{form}: {classes:?}");
    }
}
