// SPDX-License-Identifier: MIT OR Apache-2.0
//! Several queries in one panel (4.15a): the fixture and its canonical form, one refusal for each error, the
//! rules across members (4.34), the limits (4.35) and the SQL texts (7.18). The members join format 3 with no
//! change of the version (4.42).

use hune_core::document::{FORMAT_VERSION, FieldMatcher, PanelData, QueryFormat, SqlOwner, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::limits::{MAX_PANELS, MAX_QUERIES, MAX_QUERIES_PER_PANEL};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/queries.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const REPEATS: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/repeats.json");
const DOC_FORMAT: &str = include_str!("../../../docs/02-format-and-storage.md");

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

fn invalid_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dashboards/invalid")
}

/// Each file of `invalid/f24_*.json` has one error: its name, code and pointer (4.34).
const REFUSALS: &[(&str, ErrorCode, &str)] = &[
    ("f24_extra_empty", ErrorCode::OutOfRange, "/sections/0/panels/2/data/extra_queries"),
    ("f24_extra_8_items", ErrorCode::OutOfRange, "/sections/0/panels/2/data/extra_queries"),
    ("f24_ref_used_twice_default_a", ErrorCode::NotUnique, "/sections/0/panels/2/data/extra_queries/0/ref"),
    ("f24_ref_used_twice_items", ErrorCode::NotUnique, "/sections/0/panels/0/data/extra_queries/1/ref"),
    ("f24_item_no_ref", ErrorCode::MissingMember, "/sections/0/panels/2/data/extra_queries/0"),
    ("f24_ref_17_chars", ErrorCode::BadFormat, "/sections/0/panels/2/data/extra_queries/0/ref"),
    ("f24_ref_wrong_type", ErrorCode::WrongType, "/sections/0/panels/2/data/extra_queries/0/ref"),
    ("f24_ref_empty", ErrorCode::BadFormat, "/sections/0/panels/2/data/extra_queries/0/ref"),
    ("f24_query_ref_digit_first", ErrorCode::BadFormat, "/sections/0/panels/0/data/query/ref"),
    ("f24_hide_wrong_type_query", ErrorCode::WrongType, "/sections/0/panels/2/data/query/hide"),
    ("f24_hide_wrong_type_item", ErrorCode::WrongType, "/sections/0/panels/2/data/extra_queries/0/hide"),
    ("f24_item_min_interval", ErrorCode::UnknownMember, "/sections/0/panels/2/data/extra_queries/0/min_interval"),
    ("f24_item_unknown_member", ErrorCode::UnknownMember, "/sections/0/panels/2/data/extra_queries/0/title"),
    ("f24_item_duplicate_member", ErrorCode::DuplicateMember, "/sections/0/panels/2/data/extra_queries/0/ref"),
    ("f24_item_format_wrong", ErrorCode::BadFormat, "/sections/0/panels/2/data/extra_queries/0/format"),
    ("f24_item_source_wrong", ErrorCode::BadFormat, "/sections/0/panels/2/data/extra_queries/0/source"),
    ("f24_item_sql_line_end", ErrorCode::BadFormat, "/sections/0/panels/2/data/extra_queries/0/sql/0"),
    ("f24_extra_with_reuse", ErrorCode::NotPermitted, "/sections/0/panels/2/data/extra_queries"),
    ("f24_extra_in_barchart", ErrorCode::NotPermitted, "/sections/0/panels/4/data/extra_queries"),
    ("f24_extra_in_xychart", ErrorCode::NotPermitted, "/sections/0/panels/4/data/extra_queries"),
    ("f24_extra_in_text_panel", ErrorCode::NotPermitted, "/sections/0/panels/4/data"),
    ("f24_extra_with_repeat", ErrorCode::NotPermitted, "/sections/0/panels/2/data/extra_queries"),
    ("f24_extra_in_repeated_section", ErrorCode::NotPermitted, "/sections/0/panels/0/data/extra_queries"),
    ("f24_reuse_barchart_of_multi_query_source", ErrorCode::BadReference, "/sections/0/panels/4/data/reuse/panel"),
    ("f24_reuse_xychart_of_multi_query_source", ErrorCode::BadReference, "/sections/0/panels/4/data/reuse/panel"),
    ("f24_401_queries", ErrorCode::TooLarge, "/sections"),
    ("f24_matcher_no_value", ErrorCode::MissingMember, "/sections/0/panels/0/fields/overrides/0/match"),
    ("f24_matcher_value_17_chars", ErrorCode::BadFormat, "/sections/0/panels/0/fields/overrides/0/match/value"),
    ("f24_matcher_with_names", ErrorCode::UnknownMember, "/sections/0/panels/0/fields/overrides/0/match/names"),
    ("f24_matcher_value_breaks_grammar", ErrorCode::BadFormat, "/sections/0/panels/0/fields/overrides/0/match/value"),
];

#[test]
fn each_refusal_gives_its_one_error() {
    for (name, code, pointer) in REFUSALS {
        let bytes = std::fs::read(invalid_dir().join(format!("{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = validate(&bytes, None).unwrap_err();
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!((e[0].code, e[0].pointer.as_str()), (*code, *pointer), "{name}");
    }
}

#[test]
fn there_are_30_refusal_files_and_none_is_missing_from_the_table() {
    let mut files: Vec<String> = std::fs::read_dir(invalid_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str().map(str::to_owned))
        .filter(|n| n.starts_with("f24_"))
        .collect();
    files.sort();
    let mut names: Vec<String> = REFUSALS.iter().map(|(n, _, _)| format!("{n}.json")).collect();
    names.sort();
    assert_eq!(files, names);
    assert_eq!(files.len(), 30);
}

// ------------------------------------------------------------------ fixture

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(std::str::from_utf8(doc.bytes()).unwrap(), std::str::from_utf8(FIXTURE).unwrap());
    assert_eq!(doc.format_version(), FORMAT_VERSION);
    assert_eq!(FORMAT_VERSION, 3, "the members of several queries add no format version (4.42)");
}

#[test]
fn the_typed_document_reads_the_queries() {
    let doc = validate(FIXTURE, None).unwrap();
    let panels: Vec<_> = doc.document().panels().map(|(_, p)| p).collect();
    let data = |i: usize| match &panels[i].data {
        PanelData::Query(q) => q,
        other => panic!("{other:?}"),
    };
    let refs = |i: usize| data(i).queries().iter().map(|q| q.query_ref.to_owned()).collect::<Vec<_>>();
    assert_eq!(refs(0), ["A", "B", "C"]);
    assert_eq!(refs(1), ["A", "B"]);
    assert_eq!(refs(2), ["A", "B"]);
    assert_eq!(refs(3), ["C", "A"]);
    let sources: Vec<&str> = data(0).queries().iter().map(|q| q.source.as_str()).collect();
    assert_eq!(sources, ["prod", "prod", "conf"], "each query has its own source");
    assert!(data(2).queries()[1].hide && !data(2).queries()[0].hide);
    assert!(data(3).hide && data(3).query_ref == "C");
    assert_eq!(data(3).query_count(), 2);
    assert_eq!(data(0).queries()[2].format, QueryFormat::TimeSeries);
    assert_eq!(data(0).find_query(None).unwrap().query_ref, "A");
    assert_eq!(data(0).find_query(Some("C")).unwrap().position, 2);
    assert!(data(0).find_query(Some("Z")).is_none());
    assert_eq!(panels[0].fields.overrides[0].matcher, FieldMatcher::Query("C".to_owned()));
}

#[test]
fn the_sql_texts_list_each_query_with_its_pointer() {
    let doc = validate(FIXTURE, None).unwrap();
    let texts = doc.document().sql_texts();
    let pointers: Vec<&str> = texts.iter().map(|t| t.pointer.as_str()).collect();
    assert_eq!(
        pointers,
        [
            "/sections/0/panels/0/data/query/sql",
            "/sections/0/panels/0/data/extra_queries/0/sql",
            "/sections/0/panels/0/data/extra_queries/1/sql",
            "/sections/0/panels/1/data/query/sql",
            "/sections/0/panels/1/data/extra_queries/0/sql",
            "/sections/0/panels/2/data/query/sql",
            "/sections/0/panels/2/data/extra_queries/0/sql",
            "/sections/0/panels/3/data/query/sql",
            "/sections/0/panels/3/data/extra_queries/0/sql",
        ]
    );
    // A hidden query is listed too: a save of SQL is the power to run it (11.21).
    assert!(texts.iter().all(|t| matches!(t.owner, SqlOwner::Panel { .. })));
    assert_eq!(texts[2].source.as_str(), "conf");
    assert_eq!(texts[6].line_pointer(0), "/sections/0/panels/2/data/extra_queries/0/sql/0");
}

// ---------------------------------------------------------- canonical form

/// The names of the members of the object that starts at the line `start`, at an indent of `indent` spaces.
fn members_after(text: &str, start: &str, indent: usize) -> Vec<String> {
    let at = text.find(start).unwrap_or_else(|| panic!("{start}"));
    let pad = format!("\n{}\"", " ".repeat(indent));
    let close = format!("\n{}}}", " ".repeat(indent - 2));
    let end = text[at..].find(&close).map_or(text.len(), |e| at + e);
    text[at..end]
        .match_indices(&pad)
        .filter_map(|(i, _)| text[at + i + pad.len()..].split('"').next().map(str::to_owned))
        .collect()
}

#[test]
fn extra_queries_come_after_query_and_ref_and_hide_are_the_last_members() {
    let mut v = base();
    // A document that writes the members in another order, with the default ref `A` and `hide` false.
    v["sections"][0]["panels"][2]["data"] = json!({
        "extra_queries": [{"hide": true, "sql": ["SELECT 1"], "format": "table", "source": "conf", "ref": "B"}],
        "query": {
            "hide": false, "ref": "A", "hide_time_override": true, "time_shift": "1h", "time_from": "now-1h",
            "max_data_points": null, "min_interval": null, "sql": ["SELECT 2"], "format": "table", "source": "prod"
        }
    });
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(doc.bytes()).unwrap().to_owned();
    let at = text.find("Stat with a hidden query").unwrap();
    let data = &text[at..];
    let data_start = data.find("\"data\": {").unwrap();
    let members = |pat: &str, indent: usize| members_after(&data[data_start..], pat, indent);
    assert_eq!(members("\"data\": {", 12), ["query", "extra_queries"]);
    assert_eq!(
        members("\"query\": {", 14),
        ["source", "format", "sql", "min_interval", "max_data_points", "time_from", "time_shift", "hide_time_override"],
        "ref = A and hide = false are left out"
    );
    assert_eq!(members("\"extra_queries\": [", 16), ["ref", "source", "format", "sql", "hide"]);
    // A ref that is not A and hide = true come last in `data.query`, in this order.
    v["sections"][0]["panels"][2]["data"]["query"]["ref"] = json!("Z");
    v["sections"][0]["panels"][2]["data"]["query"]["hide"] = json!(true);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let text = std::str::from_utf8(doc.bytes()).unwrap().to_owned();
    let at = text.find("Stat with a hidden query").unwrap();
    let data = &text[at..];
    let data_start = data.find("\"data\": {").unwrap();
    let query = members_after(&data[data_start..], "\"query\": {", 14);
    assert_eq!(&query[query.len() - 3..], ["hide_time_override", "ref", "hide"]);
}

#[test]
fn a_document_with_one_query_keeps_its_bytes() {
    for bytes in [MINIMAL, ALL_TYPES, NODES, REPEATS] {
        let doc = validate(bytes, None).unwrap();
        assert_eq!(doc.bytes(), bytes);
        let text = std::str::from_utf8(doc.bytes()).unwrap();
        assert!(!text.contains("extra_queries") && !text.contains("\"ref\""));
        assert!(!text.contains("\"hide\": true") && !text.contains("\"hide\": false"));
    }
}

#[test]
fn the_default_ref_and_hide_false_give_the_same_bytes_as_no_member() {
    let plain = validate(NODES, None).unwrap();
    let mut v: Value = serde_json::from_slice(NODES).unwrap();
    v["sections"][0]["panels"][0]["data"]["query"]["ref"] = json!("A");
    v["sections"][0]["panels"][0]["data"]["query"]["hide"] = json!(false);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(doc.bytes(), plain.bytes());
}

#[test]
fn the_canonical_form_is_stable_for_random_documents() {
    // 1,000 documents with 1 to 8 queries: a second parse of the canonical bytes gives the same bytes.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    for round in 0..1_000 {
        let mut v: Value = serde_json::from_slice(MINIMAL).unwrap();
        let count = 1 + next(8) as usize;
        let letters: Vec<String> = {
            let mut all: Vec<String> = ('A'..='H').map(String::from).collect();
            for i in (1..all.len()).rev() {
                all.swap(i, next(i as u64 + 1) as usize);
            }
            all
        };
        let source = |n: u64| if n.is_multiple_of(2) { "prod" } else { "conf" };
        let format = |n: u64| if n.is_multiple_of(2) { "table" } else { "time_series" };
        let mut query = json!({"source": source(next(2)), "format": format(next(2)), "sql": [format!("SELECT {round}")],
            "min_interval": null, "max_data_points": null});
        if next(2) == 0 {
            query["hide"] = json!(next(2) == 0);
        }
        if count > 1 || next(2) == 0 {
            query["ref"] = json!(letters[0]);
        }
        let mut data = json!({"query": query});
        if count > 1 {
            let items: Vec<Value> = (1..count)
                .map(|i| {
                    let mut it = json!({"ref": letters[i], "source": source(next(2)), "format": format(next(2)),
                        "sql": ["SELECT 1", format!("-- {i}")]});
                    if next(3) == 0 {
                        it["hide"] = json!(next(2) == 0);
                    }
                    it
                })
                .collect();
            data["extra_queries"] = json!(items);
        }
        v["sections"][0]["panels"] = json!([{
            "id": 1, "type": "stat", "title": "P", "description": "", "transparent": false,
            "grid": {"x": 0, "y": 0, "w": 12, "h": 4}, "data": data, "transforms": [],
            "fields": {"defaults": {}, "overrides": []},
            "options": {"reduce": {"calc": "last_not_null", "fields": null, "values": false}, "color_mode": "value",
                "graph_mode": "none", "text_mode": "auto", "orientation": "auto"},
            "links": []
        }]);
        let first = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap_or_else(|e| panic!("{round}: {e:#?}"));
        let second = validate(first.bytes(), None).unwrap_or_else(|e| panic!("{round}: {e:#?}"));
        assert_eq!(first.bytes(), second.bytes(), "round {round}");
        assert_eq!(first.document(), second.document(), "round {round}");
    }
}

// ----------------------------------------------------------------- limits

fn stat_panel(id: u32, y: u32, extra: usize) -> Value {
    let mut data = json!({"query": {"source": "prod", "format": "table", "sql": ["SELECT 1"],
        "min_interval": null, "max_data_points": null}});
    if extra > 0 {
        let items: Vec<Value> = (0..extra)
            .map(|i| json!({"ref": format!("R{i}"), "source": "prod", "format": "table", "sql": ["SELECT 1"]}))
            .collect();
        data["extra_queries"] = json!(items);
    }
    json!({
        "id": id, "type": "stat", "title": "P", "description": "", "transparent": false,
        "grid": {"x": 0, "y": y, "w": 24, "h": 1}, "data": data, "transforms": [],
        "fields": {"defaults": {}, "overrides": []},
        "options": {"reduce": {"calc": "last_not_null", "fields": null, "values": false}, "color_mode": "value",
            "graph_mode": "none", "text_mode": "auto", "orientation": "auto"},
        "links": []
    })
}

fn with_panels(panels: Vec<Value>) -> Value {
    let mut v: Value = serde_json::from_slice(MINIMAL).unwrap();
    v["sections"][0]["panels"] = json!(panels);
    v
}

#[test]
fn the_limits_are_the_constants_of_4_35() {
    assert_eq!((MAX_QUERIES_PER_PANEL, MAX_QUERIES), (8, 400));
    assert_eq!(MAX_QUERIES, 2 * MAX_PANELS);
    for (name, value) in [("MAX_QUERIES_PER_PANEL", MAX_QUERIES_PER_PANEL), ("MAX_QUERIES", MAX_QUERIES)] {
        let row =
            DOC_FORMAT.lines().find(|l| l.starts_with(&format!("| `{name}` |"))).unwrap_or_else(|| panic!("{name}"));
        let cell = row.split('|').nth(2).unwrap().trim();
        assert_eq!(cell, value.to_string(), "{name}");
    }
}

#[test]
fn the_boundaries_are_valid() {
    // 8 queries in a panel: `data.query` and 7 items.
    let v = with_panels(vec![stat_panel(1, 0, MAX_QUERIES_PER_PANEL - 1)]);
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    // 400 queries in a document: 50 panels of 8 queries.
    let v = with_panels((0..50).map(|i| stat_panel(i + 1, i, 7)).collect());
    assert!(errors_of(&v).is_empty(), "{:#?}", errors_of(&v));
    // 401 queries: the error is at /sections.
    let mut panels: Vec<Value> = (0..50).map(|i| stat_panel(i + 1, i, 7)).collect();
    panels.push(stat_panel(51, 50, 0));
    let e = errors_of(&with_panels(panels));
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::TooLarge, "/sections"));
    // A ref of 16 characters.
    let mut v = base();
    v["sections"][0]["panels"][2]["data"]["extra_queries"][0]["ref"] = json!("a".repeat(16));
    assert!(errors_of(&v).is_empty());
    // A first query that has `ref` and `hide`: the fixture has one.
    assert!(validate(FIXTURE, None).is_ok());
    // A ref with a digit and an underscore after the first letter.
    v["sections"][0]["panels"][2]["data"]["extra_queries"][0]["ref"] = json!("q_1");
    assert!(errors_of(&v).is_empty());
    // A hidden query counts in the 400.
    let mut panels: Vec<Value> = (0..50).map(|i| stat_panel(i + 1, i, 7)).collect();
    panels[0]["data"]["extra_queries"][0]["hide"] = json!(true);
    assert!(errors_of(&with_panels(panels)).is_empty());
}

#[test]
fn the_order_of_the_refs_does_not_matter() {
    let mut v = base();
    v["sections"][0]["panels"][0]["data"]["query"]["ref"] = json!("C");
    v["sections"][0]["panels"][0]["data"]["extra_queries"][1]["ref"] = json!("A");
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    let panels: Vec<_> = doc.document().panels().map(|(_, p)| p).collect();
    let PanelData::Query(q) = &panels[0].data else { panic!() };
    let refs: Vec<&str> = q.queries().iter().map(|x| x.query_ref).collect();
    assert_eq!(refs, ["C", "B", "A"]);
}

#[test]
fn a_ref_of_the_first_query_that_an_item_uses_is_refused_at_the_item() {
    let mut v = base();
    v["sections"][0]["panels"][0]["data"]["query"]["ref"] = json!("B");
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(
        (e[0].code, e[0].pointer.as_str()),
        (ErrorCode::NotUnique, "/sections/0/panels/0/data/extra_queries/0/ref")
    );
}

#[test]
fn an_item_with_one_of_the_five_members_of_the_panel_is_refused() {
    for (name, value) in [
        ("min_interval", json!("1m")),
        ("max_data_points", json!(100)),
        ("time_from", json!("now-1h")),
        ("time_shift", json!("1h")),
        ("hide_time_override", json!(true)),
    ] {
        let mut v = base();
        v["sections"][0]["panels"][2]["data"]["extra_queries"][0][name] = value;
        let e = errors_of(&v);
        assert_eq!(e.len(), 1, "{name}: {e:#?}");
        assert_eq!(e[0].code, ErrorCode::UnknownMember, "{name}");
        assert_eq!(e[0].pointer, format!("/sections/0/panels/2/data/extra_queries/0/{name}"));
    }
}

/// Adds one item to each panel of the fixture that has a query, and gives the types that took it and the errors.
fn add_item_everywhere(bytes: &[u8]) -> (Vec<String>, Vec<ValidationError>) {
    let mut v: Value = serde_json::from_slice(bytes).unwrap();
    let mut types = Vec::new();
    for panel in v["sections"].as_array_mut().unwrap().iter_mut().flat_map(|s| s["panels"].as_array_mut().unwrap()) {
        // A panel with `repeat` has one query (4.15a, rule 8).
        if panel.get("repeat").is_some() {
            continue;
        }
        if let Some(q) = panel["data"].get("query").cloned() {
            types.push(panel["type"].as_str().unwrap().to_owned());
            panel["data"]["extra_queries"] =
                json!([{"ref": "B", "source": q["source"], "format": q["format"], "sql": q["sql"]}]);
        }
    }
    (types, errors_of(&v))
}

#[test]
fn the_types_that_can_have_extra_queries_are_the_seven_of_4_15a() {
    let mut accepted: Vec<String> = Vec::new();
    for bytes in [
        include_bytes!("../../../tests/fixtures/dashboards/all_types.json").as_slice(),
        include_bytes!("../../../tests/fixtures/dashboards/state_timeline.json").as_slice(),
        include_bytes!("../../../tests/fixtures/dashboards/status_history.json").as_slice(),
    ] {
        let (types, errors) = add_item_everywhere(bytes);
        for t in &types {
            let refused = errors.iter().any(|e| e.code == ErrorCode::NotPermitted);
            let one_of_two = t == "barchart" || t == "xychart";
            assert!(refused || !one_of_two, "{t}");
            if !one_of_two {
                accepted.push(t.clone());
            }
        }
        // The only errors are the `not_permitted` of the two types that draw one frame.
        for e in &errors {
            assert_eq!(e.code, ErrorCode::NotPermitted, "{e:#?}");
            assert!(e.pointer.ends_with("/data/extra_queries"), "{e:#?}");
        }
        assert_eq!(errors.len(), types.iter().filter(|t| *t == "barchart" || *t == "xychart").count(), "{errors:#?}");
    }
    accepted.sort();
    accepted.dedup();
    assert_eq!(accepted, ["bargauge", "piechart", "state_timeline", "status_history", "table", "timeseries"]);
}

#[test]
fn the_errors_of_an_extra_query_use_the_pointer_of_the_item() {
    let mut v = base();
    v["sections"][0]["panels"][0]["data"]["extra_queries"][1]["sql"] = json!(["SELECT 1", "SELECT 2\n"]);
    let e = errors_of(&v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(e[0].pointer, "/sections/0/panels/0/data/extra_queries/1/sql/1");
}
