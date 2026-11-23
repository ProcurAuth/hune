//! The eleven transformations of milestone F in the import (F28, 15.6, 4.25a, 19.20j): the classes, the fallbacks
//! and the document for each Grafana id. The cases come from the measure on Grafana 12.4.2.

mod common;

use common::*;
use hune_grafana::{Class, Fallback};
use serde_json::{Value, json};

/// Imports a table panel with the transformations and gives the result and the `transforms` of its document.
fn transforms(list: Value) -> (hune_grafana::DashboardImport, Value) {
    let mut p = panel(1, "table", 0);
    p["transformations"] = list;
    let r = run(&dash(vec![p], vec![]));
    let out = doc(&r);
    let t = first_panel(&out)["transforms"].clone();
    (r, t)
}

fn one(id: &str, options: Value) -> Value {
    json!([{"id": id, "options": options}])
}

fn removed() -> Option<Fallback> {
    Some(Fallback::other("transformation removed"))
}

/// Classes of the items, in the order of the report.
fn classes(r: &hune_grafana::DashboardImport) -> Vec<(Class, String)> {
    r.items.iter().map(|i| (i.class, i.member.clone())).collect()
}

// ------------------------------------------------------------------ mapped with no item

#[test]
fn each_kind_maps_with_no_report_item() {
    let cases: Vec<(Value, Value)> = vec![
        (
            one(
                "organize",
                json!({"excludeByName": {"a": true, "b": false}, "indexByName": {"z": 0, "x": 1}, "renameByName": {"x": "X"}}),
            ),
            json!({"kind": "organize", "hide": ["a"], "order": ["z", "x"], "rename": [{"field": "x", "name": "X"}]}),
        ),
        (
            one(
                "calculateField",
                json!({"mode": "binary", "binary": {"left": "x", "operator": "+", "right": "y"}, "alias": "S", "replaceFields": true, "reduce": {"reducer": "sum"}}),
            ),
            json!({"kind": "calculate_field", "mode": "binary", "alias": "S", "replace_fields": true,
                   "binary": {"left": {"kind": "field", "field": "x"}, "right": {"kind": "field", "field": "y"}, "operator": "add"}}),
        ),
        (
            one(
                "calculateField",
                json!({"mode": "reduceRow", "reduce": {"reducer": "lastNotNull", "include": ["x", "y"]}}),
            ),
            json!({"kind": "calculate_field", "mode": "reduce_row", "alias": null, "replace_fields": false,
                   "reduce": {"reducer": "last_not_null", "include": ["x", "y"]}}),
        ),
        (
            one(
                "calculateField",
                json!({"mode": "unary", "unary": {"operator": "abs", "fieldName": "v"}, "alias": ""}),
            ),
            json!({"kind": "calculate_field", "mode": "unary", "alias": null, "replace_fields": false,
                   "unary": {"operator": "abs", "field": "v"}}),
        ),
        (
            one("calculateField", json!({"mode": "index", "index": {"asPercentile": true}})),
            json!({"kind": "calculate_field", "mode": "index", "alias": null, "replace_fields": false,
                   "index": {"as_percentile": true}}),
        ),
        (
            one("calculateField", json!({"mode": "index"})),
            json!({"kind": "calculate_field", "mode": "index", "alias": null, "replace_fields": false,
                   "index": {"as_percentile": false}}),
        ),
        (
            one("sortBy", json!({"fields": {}, "sort": [{"field": "x"}, {"field": "z", "desc": true}]})),
            json!({"kind": "sort_by", "sort": [{"field": "x", "desc": false}, {"field": "z", "desc": true}]}),
        ),
        (one("limit", json!({"limitField": 2})), json!({"kind": "limit", "count": 2, "end": "first"})),
        (one("limit", json!({"limitField": "3"})), json!({"kind": "limit", "count": 3, "end": "first"})),
        (one("limit", json!({"limitField": 0})), json!({"kind": "limit", "count": 0, "end": "first"})),
        (one("limit", json!({"limitField": -1})), json!({"kind": "limit", "count": 1, "end": "last"})),
        (one("limit", json!({"limitField": "-5"})), json!({"kind": "limit", "count": 5, "end": "last"})),
        (one("limit", json!({"limitField": 2.5})), json!({"kind": "limit", "count": 2, "end": "first"})),
        (one("limit", json!({"limitField": -2.5})), json!({"kind": "limit", "count": 2, "end": "last"})),
        (one("merge", json!({})), json!({"kind": "merge"})),
        (
            one(
                "reduce",
                json!({"mode": "reduceFields", "reducers": ["min", "sum", "diffperc", "stdDev", "distinctCount", "changeCount"]}),
            ),
            json!({"kind": "reduce", "mode": "reduce_fields", "reducers": ["min", "total", "diff_percent", "std_dev", "distinct_count", "change_count"]}),
        ),
        (one("reduce", json!({})), json!({"kind": "reduce", "mode": "series_to_rows", "reducers": ["max"]})),
        (
            one(
                "reduce",
                json!({"mode": "seriesToRows", "reducers": ["lastNotNull", "firstNotNull"], "fields": "", "includeTimeField": false}),
            ),
            json!({"kind": "reduce", "mode": "series_to_rows", "reducers": ["last_not_null", "first_not_null"]}),
        ),
        (
            one("joinByField", json!({"byField": "host", "mode": "inner"})),
            json!({"kind": "join_by_field", "field": "host", "mode": "inner"}),
        ),
        (one("joinByField", json!({})), json!({"kind": "join_by_field", "field": null, "mode": "outer"})),
        (
            one("seriesToColumns", json!({"byField": "", "mode": "outer"})),
            json!({"kind": "join_by_field", "field": null, "mode": "outer"}),
        ),
        (
            one(
                "filterFieldsByName",
                json!({"include": {"names": ["x", "y"], "pattern": "^a"}, "exclude": {"pattern": "/b/i"}}),
            ),
            json!({"kind": "filter_fields_by_name", "include": {"names": ["x", "y"], "pattern": "^a"}, "exclude": {"names": [], "pattern": "/b/i"}}),
        ),
        (
            one("filterFieldsByName", json!({})),
            json!({"kind": "filter_fields_by_name", "include": {"names": [], "pattern": null}, "exclude": {"names": [], "pattern": null}}),
        ),
        (
            one("renameByRegex", json!({"regex": "(.*)_total", "renamePattern": "$1"})),
            json!({"kind": "rename_by_regex", "regex": "(.*)_total", "rename_pattern": "$1"}),
        ),
        (
            one("renameByRegex", json!({"regex": "^cpu_", "renamePattern": ""})),
            json!({"kind": "rename_by_regex", "regex": "^cpu_", "rename_pattern": ""}),
        ),
    ];
    for (input, expected) in cases {
        let (r, t) = transforms(input.clone());
        no_items(&r);
        assert_eq!(t, json!([expected]), "{input}");
    }
}

#[test]
fn operands_of_binary() {
    let case = |left: Value, op: &str, right: Value| {
        transforms(one(
            "calculateField",
            json!({"mode": "binary", "binary": {"left": left, "operator": op, "right": right}}),
        ))
    };
    let field = |n: &str| json!({"kind": "field", "field": n});
    let number = |v: f64| json!({"kind": "number", "value": serde_json::from_str::<Value>(&v.to_string()).unwrap()});
    for (left, op, right, l, o, rr) in [
        (json!("x"), "*", json!("2"), field("x"), "multiply", number(2.0)),
        (json!("2"), "-", json!("x"), number(2.0), "subtract", field("x")),
        (json!("x"), "/", json!("1.5"), field("x"), "divide", number(1.5)),
        (json!("x"), "*", json!("-2"), field("x"), "multiply", number(-2.0)),
        (
            json!({"matcher": {"id": "byName", "options": "x"}}),
            "+",
            json!({"matcher": {"id": "byName", "options": "y"}}),
            field("x"),
            "add",
            field("y"),
        ),
        (json!({"fixed": "10"}), "+", json!("x"), number(10.0), "add", field("x")),
        (json!("x"), "+", json!(7), field("x"), "add", number(7.0)),
    ] {
        let (r, t) = case(left.clone(), op, right.clone());
        no_items(&r);
        assert_eq!(t[0]["binary"], json!({"left": l, "right": rr, "operator": o}), "{left} {op} {right}");
    }
}

// ------------------------------------------------------------------ unsupported and dropped

#[test]
fn unsupported_options_remove_the_transformation() {
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": "x", "operator": "%", "right": "y"}}),
            "operator",
        ),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": "x", "operator": "+", "right": "$v"}}),
            "operand with $",
        ),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": "x", "operator": "+", "right": ""}}),
            "empty operand",
        ),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": {"matcher": {"id": "byType", "options": "number"}}, "operator": "+", "right": "y"}}),
            "byType",
        ),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": "1", "operator": "+", "right": "2"}}),
            "two numbers",
        ),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": {"fixed": "x"}, "operator": "+", "right": "y"}}),
            "fixed text",
        ),
        ("calculateField", json!({"mode": "reduceRow", "reduce": {"reducer": "allValues"}}), "allValues"),
        ("calculateField", json!({"mode": "reduceRow", "reduce": {"reducer": "p50"}}), "p50"),
        (
            "calculateField",
            json!({"mode": "reduceRow", "reduce": {"reducer": "sum", "nullValueMode": "null as zero"}}),
            "null as zero",
        ),
        (
            "calculateField",
            json!({"mode": "reduceRow", "nullValueMode": "null as zero", "reduce": {"reducer": "sum"}}),
            "null as zero, top",
        ),
        (
            "calculateField",
            json!({"mode": "cumulativeFunctions", "cumulative": {"field": "x", "reducer": "sum"}}),
            "cumulative",
        ),
        ("calculateField", json!({"mode": "windowFunctions", "window": {"field": "x", "windowSize": 5}}), "window"),
        ("calculateField", json!({"mode": "unary", "unary": {"operator": "sqrt", "fieldName": "v"}}), "sqrt"),
        (
            "calculateField",
            json!({"mode": "binary", "binary": {"left": "x", "operator": "+", "right": "y"}, "alias": "a".repeat(257)}),
            "alias 257",
        ),
        ("calculateField", json!({"binary": {"left": "x", "operator": "+", "right": "y"}}), "no mode"),
        ("limit", json!({"limitField": "$n"}), "variable"),
        ("reduce", json!({"reducers": ["max"], "fields": "/^x$/"}), "fields"),
        ("reduce", json!({"reducers": ["allIsNull", "step"]}), "no reducer left"),
        ("joinByField", json!({"byField": "k", "mode": "outerTabular"}), "outerTabular"),
        ("filterFieldsByName", json!({"include": {"pattern": "(?=a)"}}), "bad pattern"),
        ("filterFieldsByName", json!({"include": {"variable": "v"}, "byVariable": true}), "variable"),
        ("renameByRegex", json!({"regex": "(?<=a)b", "renamePattern": "c"}), "bad regex"),
        ("renameByRegex", json!({"regex": "a"}), "no pattern"),
        ("partitionByValues", json!({"fields": []}), "no fields"),
        ("partitionByValues", json!({"fields": ["a", "b", "c", "d", "e"]}), "5 fields"),
        ("organize", json!({"renameByName": {"x": "n".repeat(257)}}), "new name 257"),
        ("sortBy", json!({"sort": [{"field": "x", "desc": "up"}]}), "desc text"),
        ("convertFieldType", json!({"conversions": [{"targetField": "x", "destinationType": "enum"}]}), "only enum"),
        ("convertFieldType", json!({}), "no conversions"),
        ("merge", json!(5), "options not an object"),
    ];
    for (id, options, why) in cases {
        let (r, t) = transforms(one(id, options.clone()));
        assert_eq!(t, json!([]), "{id} {why}");
        assert!(!r.items.is_empty(), "{id} {why}");
        let item = r.items.iter().find(|i| {
            i.class == Class::Unsupported
                && (i.fallback == removed() || i.fallback == Some(Fallback::other("conversion removed")))
        });
        assert!(item.is_some(), "{id} {why}: {:#?}", r.items);
    }
}

#[test]
fn options_with_no_measured_effect_are_dropped() {
    // `ignoreNulls`, `nullValueMode` (other than null as zero), `timeSeries`, `reverse`, `includeTimeField`, `labelsToFields`.
    let (r, t) = transforms(one(
        "calculateField",
        json!({"mode": "reduceRow", "reduce": {"reducer": "sum", "include": ["x"]}, "ignoreNulls": true, "nullValueMode": "connected", "timeSeries": true}),
    ));
    assert_eq!(t[0]["kind"], "calculate_field");
    assert_eq!(
        classes(&r),
        vec![
            (Class::Dropped, "transformations.0.options.ignoreNulls".to_owned()),
            (Class::Dropped, "transformations.0.options.nullValueMode".to_owned()),
            (Class::Dropped, "transformations.0.options.timeSeries".to_owned()),
        ]
    );
    let (r, t) = transforms(one("sortBy", json!({"sort": [{"field": "x"}], "reverse": true})));
    assert_eq!(t[0]["sort"], json!([{"field": "x", "desc": false}]));
    assert_eq!(classes(&r), vec![(Class::Dropped, "transformations.0.options.reverse".to_owned())]);
    let (r, t) =
        transforms(one("reduce", json!({"reducers": ["max"], "includeTimeField": true, "labelsToFields": true})));
    assert_eq!(t[0]["reducers"], json!(["max"]));
    assert_eq!(r.items.len(), 2);
    assert!(r.items.iter().all(|i| i.class == Class::Dropped));
    // A trivial value is metadata.
    let (r, _) = transforms(one("sortBy", json!({"sort": [{"field": "x"}], "reverse": false})));
    no_items(&r);
}

#[test]
fn dropped_transformations_have_no_item_in_the_document() {
    for (id, options, member) in [
        ("limit", json!({}), "transformations.0"),
        ("limit", json!({"limitField": ""}), "transformations.0.options.limitField"),
        ("limit", json!({"limitField": "abc"}), "transformations.0.options.limitField"),
        ("sortBy", json!({"sort": []}), "transformations.0"),
        ("reduce", json!({"reducers": []}), "transformations.0.options.reducers"),
        ("renameByRegex", json!({}), "transformations.0"),
    ] {
        let (r, t) = transforms(one(id, options.clone()));
        assert_eq!(t, json!([]), "{id} {options}");
        let item = &r.items[0];
        assert_eq!((item.class, item.member.as_str()), (Class::Dropped, member), "{id} {options}");
    }
}

#[test]
fn the_other_classes_of_the_table() {
    // `includeByName`: unsupported, the member is removed, the other members stay.
    let (r, t) = transforms(one("organize", json!({"includeByName": {"a": true}, "excludeByName": {"b": true}})));
    assert_eq!(t[0]["hide"], json!(["b"]));
    assert_eq!(r.items.len(), 1);
    assert_eq!((r.items[0].class, r.items[0].fallback.clone()), (Class::Unsupported, Some(Fallback::MemberRemoved)));
    // An empty new name is dropped. An empty `includeByName` is metadata.
    let (r, t) = transforms(one("organize", json!({"includeByName": {}, "renameByName": {"x": ""}})));
    assert_eq!(t[0]["rename"], json!([]));
    assert_eq!(classes(&r), vec![(Class::Dropped, "transformations.0.options.renameByName.x".to_owned())]);
    // `limit` above 1,000,000: the fallback is the count 1,000,000.
    let (r, t) = transforms(one("limit", json!({"limitField": 2_000_000})));
    assert_eq!(t[0], json!({"kind": "limit", "count": 1_000_000, "end": "first"}));
    assert_eq!(
        (r.items[0].class, r.items[0].fallback.clone()),
        (Class::Unsupported, Some(Fallback::other("count = 1,000,000")))
    );
    // `sortBy` with 9 keys: the first 8 keys stay.
    let keys: Vec<Value> = (0..9).map(|i| json!({"field": format!("f{i}")})).collect();
    let (r, t) = transforms(one("sortBy", json!({"sort": keys})));
    assert_eq!(t[0]["sort"].as_array().unwrap().len(), 8);
    assert_eq!(r.items[0].fallback, Some(Fallback::other("the first 8 keys stay")));
    // `reduce`: a reducer that has no calculation leaves the list.
    let (r, t) = transforms(one("reduce", json!({"reducers": ["max", "allIsNull", "min"]})));
    assert_eq!(t[0]["reducers"], json!(["max", "min"]));
    assert_eq!(
        (r.items[0].class, r.items[0].fallback.clone()),
        (Class::Unsupported, Some(Fallback::other("reducer removed")))
    );
    // `partitionByValues`: asLabels false is changed, the other naming members are dropped.
    let (r, t) = transforms(one(
        "partitionByValues",
        json!({"fields": ["host"], "naming": {"asLabels": false, "append": true}}),
    ));
    assert_eq!(t[0], json!({"kind": "partition_by_values", "fields": ["host"], "keep_fields": false}));
    assert_eq!(
        classes(&r),
        vec![
            (Class::Changed, "transformations.0".to_owned()),
            (Class::Dropped, "transformations.0.options.naming.append".to_owned()),
            (Class::Changed, "transformations.0.options.naming.asLabels".to_owned())
        ]
    );
    // The class of the row is changed: one item for each transformation, with its keys.
    let (r, t) = transforms(one("partitionByValues", json!({"fields": ["host", "env"], "keepFields": true})));
    assert_eq!(t[0], json!({"kind": "partition_by_values", "fields": ["host", "env"], "keep_fields": true}));
    assert_eq!(classes(&r), vec![(Class::Changed, "transformations.0".to_owned())]);
    // `convertFieldType`: the class is changed. `string` gives `text`. A conversion with no equal leaves the list.
    let (r, t) = transforms(one(
        "convertFieldType",
        json!({"conversions": [
            {"targetField": "a", "destinationType": "string"},
            {"targetField": "b", "destinationType": "number"},
            {"targetField": "c", "destinationType": "time", "dateFormat": "YYYY"},
            {"targetField": "d", "destinationType": "other"},
            {"targetField": "e", "destinationType": "boolean", "joinWith": ""}
        ]}),
    ));
    assert_eq!(
        t[0]["conversions"],
        json!([{"field": "a", "to": "text"}, {"field": "b", "to": "number"}, {"field": "e", "to": "boolean"}])
    );
    let by_class = |c: Class| r.items.iter().filter(|i| i.class == c).count();
    assert_eq!((by_class(Class::Changed), by_class(Class::Unsupported)), (1, 2));
    // An equal index gives a `changed` item.
    let (r, t) = transforms(one("organize", json!({"indexByName": {"y": 0, "x": 0, "z": 1}})));
    assert_eq!(t[0]["order"], json!(["x", "y", "z"]));
    assert_eq!(r.items[0].class, Class::Changed);
}

#[test]
fn more_than_16_transformations_keep_the_first_16() {
    let list: Vec<Value> = (0..17).map(|_| json!({"id": "merge", "options": {}})).collect();
    let (r, t) = transforms(Value::Array(list));
    assert_eq!(t.as_array().unwrap().len(), 16);
    assert_eq!(r.items.len(), 1);
    assert_eq!(
        (r.items[0].class, r.items[0].fallback.clone()),
        (Class::Unsupported, Some(Fallback::other("the first 16 items stay")))
    );
}

/// A v2 dashboard with one table panel that has the transformations `list` (an item is `{kind, spec}`).
fn v2_table(form: &str, list: Value) -> Value {
    let query = if form == "alpha1" {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "grafana-clickhouse-datasource", "spec": {"rawSql": "SELECT 1", "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"}},
            "datasource": {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"}, "refId": "A", "hidden": false}})
    } else {
        json!({"kind": "PanelQuery", "spec": {"query": {"kind": "DataQuery", "group": "grafana-clickhouse-datasource", "version": "v0",
            "datasource": {"name": "clickhouse"}, "spec": {"rawSql": "SELECT 1", "format": 1, "editorType": "sql", "queryType": "table", "pluginVersion": "4.5.1"}}, "refId": "A", "hidden": false}})
    };
    let viz = if form == "alpha1" {
        json!({"kind": "table", "spec": {"pluginVersion": "12.2.1", "options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    } else {
        json!({"kind": "VizConfig", "group": "table", "version": "12.2.1", "spec": {"options": {}, "fieldConfig": {"defaults": {}, "overrides": []}}})
    };
    let element = json!({"kind": "Panel", "spec": {"id": 1, "title": "P1", "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [query], "transformations": list, "queryOptions": {}}}, "vizConfig": viz}});
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{}", if form == "alpha1" { "alpha1" } else { "beta1" }),
        "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "V2 test", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": [], "elements": {"panel-1": element},
            "layout": {"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {"title": "", "collapse": false, "hideHeader": true,
                "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {"x": 0, "y": 0, "width": 12, "height": 4,
                    "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}}}]}}}
    })
}

#[test]
fn the_v2_form_gives_the_same_items_as_the_v1_form() {
    for form in ["alpha1", "beta1"] {
        let list = json!([
            {"kind": "organize", "spec": {"id": "organize", "options": {"excludeByName": {"a": true}}}},
            {"kind": "seriesToColumns", "spec": {"id": "seriesToColumns", "options": {"byField": "k", "mode": "outerTabular"}}},
            {"kind": "limit", "spec": {"id": "limit", "options": {"limitField": 5}}},
        ]);
        let r = run(&v2_table(form, list));
        let out = doc(&r);
        let t = &first_panel(&out)["transforms"];
        assert_eq!(t.as_array().unwrap().len(), 2, "{form}");
        assert_eq!(t[0]["hide"], json!(["a"]));
        assert_eq!(t[1], json!({"kind": "limit", "count": 5, "end": "first"}));
        assert_eq!(r.items.len(), 1, "{form}: {:#?}", r.items);
        assert_eq!(r.items[0].member, "transformations.1.spec.options.mode");
    }
}

#[test]
fn every_document_that_the_import_gives_is_valid_and_canonical() {
    // `doc` validates and compares the bytes. The mix covers the kinds in one panel.
    let list = json!([
        {"id": "organize", "options": {"excludeByName": {"a": true}, "indexByName": {"b": 0}, "renameByName": {"b": "B"}}},
        {"id": "calculateField", "options": {"mode": "binary", "binary": {"left": "x", "operator": "/", "right": "100"}, "alias": "Pct"}},
        {"id": "sortBy", "options": {"sort": [{"field": "Pct", "desc": true}]}},
        {"id": "limit", "options": {"limitField": 10}},
        {"id": "merge", "options": {}},
        {"id": "reduce", "options": {"reducers": ["max"]}},
        {"id": "partitionByValues", "options": {"fields": ["host"]}},
        {"id": "joinByField", "options": {"byField": "host"}},
        {"id": "filterFieldsByName", "options": {"include": {"names": ["host"]}}},
        {"id": "renameByRegex", "options": {"regex": "(.*)", "renamePattern": "$1"}},
        {"id": "convertFieldType", "options": {"conversions": [{"targetField": "x", "destinationType": "number"}]}},
    ]);
    let (r, t) = transforms(list);
    assert_eq!(t.as_array().unwrap().len(), 11);
    // The two kinds with the class `changed` have one item each.
    assert_eq!(r.items.len(), 2, "{:#?}", r.items);
    assert!(r.items.iter().all(|i| i.class == Class::Changed));
}
