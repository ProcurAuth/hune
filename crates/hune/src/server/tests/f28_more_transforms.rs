//! The eleven transforms of milestone F on the routes (F28, ID-K240, A100 to A102 of 16.8). The server only
//! validates and stores the members: it runs no code for a transform (9.21a).

use super::api::{JSON, etag_of, fill, key, post_body, put};
use super::harness::*;
use hune_grafana::{Class, ImportOptions, import_dashboard, parse_acceptance_file};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../../../../tests/fixtures/dashboards/transforms_more.json");

async fn create(h: &Harness, doc: &str) -> Resp {
    call(h.addr, "POST", "/api/dashboards", &[JSON, ("Idempotency-Key", &key())], &post_body("transit", doc)).await
}

fn stored(h: &Harness, uid: &str) -> Vec<u8> {
    std::fs::read(std::fs::canonicalize(h.dir.path()).unwrap().join(format!("transit/{uid}.json")))
        .expect("the file of the document")
}

fn pointers(r: &Resp) -> Vec<(String, String)> {
    r.json()["error"]["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| (e["pointer"].as_str().unwrap().to_owned(), e["code"].as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// The fixture with another uid, and the first panel only (the transforms of 4.25a, one of each kind).
fn small(uid: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut d: Value = serde_json::from_str(FIXTURE).unwrap();
    d["uid"] = json!(uid);
    d["sections"][0]["panels"].as_array_mut().unwrap().truncate(1);
    edit(&mut d);
    serde_json::to_string(&d).unwrap()
}

fn transform(d: &mut Value, i: usize) -> &mut Value {
    &mut d["sections"][0]["panels"][0]["transforms"][i]
}

#[tokio::test]
async fn a100_a_document_with_the_eleven_kinds_is_saved_and_read_back_with_the_canonical_bytes() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let r = create(&h, FIXTURE).await;
    assert_eq!(r.status, 201, "{}", r.text());
    // The fixture is the canonical form: the file holds the same bytes, and the etag is the one of these bytes.
    let canonical = hune_core::document::validate(FIXTURE.as_bytes(), None).unwrap();
    assert_eq!(canonical.bytes(), FIXTURE.as_bytes());
    assert_eq!(stored(&h, "transforms-more"), FIXTURE.as_bytes());
    assert_eq!(etag_of(&r), canonical.etag().to_hex());
    let read = get(h.addr, "/api/dashboards/transforms-more").await;
    assert_eq!(read.status, 200, "{}", read.text());
    assert_eq!(etag_of(&read), canonical.etag().to_hex());
    let kinds: Vec<String> = read.json()["document"]["sections"][0]["panels"][0]["transforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        kinds,
        [
            "organize",
            "calculate_field",
            "sort_by",
            "limit",
            "merge",
            "reduce",
            "partition_by_values",
            "join_by_field",
            "filter_fields_by_name",
            "rename_by_regex",
            "convert_field_type"
        ]
    );
    // The same document with the members in another order gives the same bytes and the same etag (4.38, 4.39).
    let mut shuffled: Value = serde_json::from_str(FIXTURE).unwrap();
    shuffled["uid"] = json!("transforms-order");
    *transform(&mut shuffled, 1) = json!({
        "binary": {"operator": "add", "right": {"value": 2.5, "kind": "number"}, "left": {"field": "a", "kind": "field"}},
        "replace_fields": false, "alias": "sum_ab", "mode": "binary", "kind": "calculate_field"
    });
    *transform(&mut shuffled, 3) = json!({"end": "first", "count": 0, "kind": "limit"});
    let r = create(&h, &serde_json::to_string(&shuffled).unwrap()).await;
    assert_eq!(r.status, 201, "{}", r.text());
    let expected =
        String::from_utf8(FIXTURE.as_bytes().to_vec()).unwrap().replace("transforms-more", "transforms-order");
    assert_eq!(String::from_utf8(stored(&h, "transforms-order")).unwrap(), expected);
    // A save of the stored document with the same bytes keeps the etag.
    let e = etag_of(&read);
    let r = put(&h, "transforms-more", &e, FIXTURE, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(etag_of(&r), e);
    assert_eq!(stored(&h, "transforms-more"), FIXTURE.as_bytes());
    // A document with no new kind keeps its bytes.
    let plain =
        std::fs::read_to_string(std::fs::canonicalize(h.dir.path()).unwrap().join("transit/nodes.json")).unwrap();
    assert_eq!(plain, super::api::NODES);
}

#[tokio::test]
async fn a101_twenty_invalid_transforms_give_422_with_the_code_and_the_pointer_of_4_37() {
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let t = "/sections/0/panels/0/transforms";
    type Case = (&'static str, usize, Box<dyn FnOnce(&mut Value)>, String, &'static str);
    let set = |member: &'static str, v: Value| -> Box<dyn FnOnce(&mut Value)> {
        Box::new(move |t: &mut Value| t[member] = v)
    };
    let cases: Vec<Case> = vec![
        ("a member of another kind", 0, set("count", json!(1)), format!("{t}/0/count"), "unknown_member"),
        (
            "an absent member",
            0,
            Box::new(|t| {
                t.as_object_mut().unwrap().remove("hide");
            }),
            format!("{t}/0"),
            "missing_member",
        ),
        ("a wrong type", 0, set("hide", json!("b")), format!("{t}/0/hide"), "wrong_type"),
        ("a duplicate name", 0, set("order", json!(["a", "a"])), format!("{t}/0/order/1"), "not_unique"),
        ("an unknown mode", 1, set("mode", json!("ternary")), format!("{t}/1/mode"), "bad_format"),
        (
            "two numbers",
            1,
            Box::new(|t| t["binary"]["left"] = json!({"kind": "number", "value": 1})),
            format!("{t}/1/binary"),
            "not_permitted",
        ),
        ("an empty alias", 1, set("alias", json!("")), format!("{t}/1/alias"), "out_of_range"),
        (
            "a second object of mode",
            1,
            set("index", json!({"as_percentile": false})),
            format!("{t}/1/index"),
            "unknown_member",
        ),
        (
            "9 sort keys",
            2,
            set("sort", Value::Array((0..9).map(|i| json!({"field": format!("f{i}"), "desc": false})).collect())),
            format!("{t}/2/sort"),
            "too_large",
        ),
        ("a count of -1", 3, set("count", json!(-1)), format!("{t}/3/count"), "out_of_range"),
        ("a count of 1,000,001", 3, set("count", json!(1_000_001)), format!("{t}/3/count"), "out_of_range"),
        ("a member of merge", 4, set("mode", json!("outer")), format!("{t}/4/mode"), "unknown_member"),
        ("19 reducers", 5, set("reducers", json!(vec!["max"; 19])), format!("{t}/5/reducers"), "too_large"),
        ("an empty fields", 6, set("fields", json!([])), format!("{t}/6/fields"), "out_of_range"),
        ("5 fields", 6, set("fields", json!(["a", "b", "c", "d", "e"])), format!("{t}/6/fields"), "too_large"),
        ("an unknown join mode", 7, set("mode", json!("outer_tabular")), format!("{t}/7/mode"), "bad_format"),
        (
            "a bad pattern",
            8,
            set("include", json!({"names": [], "pattern": "(?=a)"})),
            format!("{t}/8/include/pattern"),
            "bad_format",
        ),
        ("a bad regex", 9, set("regex", json!("(a")), format!("{t}/9/regex"), "bad_format"),
        (
            "17 conversions",
            10,
            set(
                "conversions",
                Value::Array((0..17).map(|i| json!({"field": format!("c{i}"), "to": "text"})).collect()),
            ),
            format!("{t}/10/conversions"),
            "too_large",
        ),
    ];
    assert_eq!(cases.len(), 19);
    let mut all = cases;
    all.push((
        "a duplicate conversion",
        10,
        set("conversions", json!([{"field": "a", "to": "text"}, {"field": "a", "to": "number"}])),
        format!("{t}/10/conversions/1"),
        "not_unique",
    ));
    assert_eq!(all.len(), 20);
    for (name, i, edit, pointer, code) in all {
        let doc = small("transforms-bad", |d| edit(transform(d, i)));
        let r = create(&h, &doc).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{name}: {}", r.text());
        assert_eq!(pointers(&r), vec![(pointer, code.to_owned())], "{name}");
    }
    // The refusals did not store a file.
    assert!(!std::fs::canonicalize(h.dir.path()).unwrap().join("transit/transforms-bad.json").exists());
}

/// A Grafana dashboard with one table panel that has the eleven ids of 15.6, with the options of the measure.
fn grafana_dashboard() -> Value {
    let target = json!({"datasource": {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"}, "editorType": "sql",
        "format": 1, "rawSql": "SELECT 1 AS a, 2 AS b", "refId": "A", "queryType": "table", "pluginVersion": "4.5.1"});
    json!({
        "uid": "grafana-more", "title": "Grafana more", "schemaVersion": 41, "time": {"from": "now-6h", "to": "now"},
        "refresh": "", "templating": {"list": []},
        "panels": [{
            "id": 1, "type": "table", "title": "Table", "datasource": {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"},
            "gridPos": {"h": 8, "w": 24, "x": 0, "y": 0}, "targets": [target],
            "fieldConfig": {"defaults": {}, "overrides": []}, "options": {},
            "transformations": [
                {"id": "organize", "options": {"excludeByName": {"b": true}, "indexByName": {"a": 0}, "renameByName": {"a": "A"}, "includeByName": {"a": true}}},
                {"id": "calculateField", "options": {"mode": "binary", "binary": {"left": "a", "operator": "*", "right": "2"}, "alias": "Double"}},
                {"id": "sortBy", "options": {"sort": [{"field": "a", "desc": true}], "reverse": true}},
                {"id": "limit", "options": {"limitField": "-3"}},
                {"id": "merge", "options": {}},
                {"id": "reduce", "options": {"reducers": ["max", "sum", "p50"]}},
                {"id": "partitionByValues", "options": {"fields": ["a"], "naming": {"asLabels": false}}},
                {"id": "joinByField", "options": {"byField": "a", "mode": "outerTabular"}},
                {"id": "filterFieldsByName", "options": {"include": {"names": ["a"]}}},
                {"id": "renameByRegex", "options": {"regex": "(.*)", "renamePattern": "$1"}},
                {"id": "convertFieldType", "options": {"conversions": [{"targetField": "a", "destinationType": "string"}, {"targetField": "b", "destinationType": "enum"}]}}
            ]
        }]
    })
}

#[tokio::test]
async fn a102_the_import_of_the_eleven_ids_gives_the_rows_of_15_6_and_the_saved_document_is_stable() {
    let options = ImportOptions::with_source_map(&[("clickhouse", "prod")]);
    let dash = grafana_dashboard();
    let first = import_dashboard(&dash, &options);
    // The classes and the fallbacks of 15.6, in the order of the pointers.
    let rows: Vec<(String, Class, Option<String>)> = first
        .items
        .iter()
        .map(|i| (i.member.clone(), i.class, i.fallback.as_ref().map(|f| f.text().to_owned())))
        .collect();
    let unsupported = |m: &str, fb: &str| (m.to_owned(), Class::Unsupported, Some(fb.to_owned()));
    let expected = vec![
        unsupported("transformations.0.options.includeByName", "member removed"),
        ("transformations.2.options.reverse".to_owned(), Class::Dropped, None),
        ("transformations.5.options.reducers.2".to_owned(), Class::Unsupported, Some("reducer removed".to_owned())),
        ("transformations.6".to_owned(), Class::Changed, None),
        ("transformations.6.options.naming.asLabels".to_owned(), Class::Changed, None),
        unsupported("transformations.7.options.mode", "transformation removed"),
        ("transformations.10.options.conversions".to_owned(), Class::Changed, None),
        unsupported("transformations.10.options.conversions.1.destinationType", "conversion removed"),
    ];
    let mut sorted_expected = expected.clone();
    sorted_expected.sort_by(|a, b| a.0.cmp(&b.0));
    let mut got = rows.clone();
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got, sorted_expected, "{rows:#?}");
    // The second import accepts each item: the document has the new kinds.
    let ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    let mut accepted = options.clone();
    accepted.accept = parse_acceptance_file(&ids.join("\n")).unwrap();
    let second = import_dashboard(&dash, &accepted);
    let bytes = second.document_bytes.clone().expect("a document");
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let kinds: Vec<&str> = doc["sections"][0]["panels"][0]["transforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "organize",
            "calculate_field",
            "sort_by",
            "limit",
            "merge",
            "reduce",
            "partition_by_values",
            "filter_fields_by_name",
            "rename_by_regex",
            "convert_field_type"
        ]
    );
    let t = &doc["sections"][0]["panels"][0]["transforms"];
    assert_eq!(t[3], json!({"kind": "limit", "count": 3, "end": "last"}));
    assert_eq!(t[5]["reducers"], json!(["max", "total"]));
    // The server saves the imported document, and a second save gives the same bytes.
    let h = Harness::start_with(test_timing(), fill, |_| {}).await;
    let text = String::from_utf8(bytes.clone()).unwrap();
    let r = create(&h, &text).await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert_eq!(stored(&h, "grafana-more"), bytes);
    let read = get(h.addr, "/api/dashboards/grafana-more").await;
    let r = put(&h, "grafana-more", &etag_of(&read), &text, &key()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(stored(&h, "grafana-more"), bytes);
    assert_eq!(etag_of(&r), etag_of(&read));
}
