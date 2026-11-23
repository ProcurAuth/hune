//! The panel type `logs` and the query format `logs` (4.33f, 4.15; decision ID-K326, item F33).
//!
//! Each option of the type is optional: an absent member has its default value, and the canonical writer leaves
//! out a member with its default value. The format `logs` is permitted in a panel of the type `logs` only. A logs
//! panel draws one frame: it has no `extra_queries`, and a reuse names a source with one query.

use hune_core::document::{LogsOptions, LogsOrder, PanelOptions, PanelType, QueryFormat, TextEnum, validate};
use hune_core::error::{ErrorCode, ValidationError};
use serde_json::{Value, json};

const P: &str = "/sections/0/panels/0";

fn logs_panel(id: u32, options: Value) -> Value {
    json!({
        "id": id, "type": "logs", "title": "Logs", "description": "", "transparent": false,
        "grid": {"x": 0, "y": 0, "w": 24, "h": 8},
        "data": {"query": {"source": "prod", "format": "logs", "sql": ["SELECT t, body, level FROM app.logs ORDER BY t DESC LIMIT 1000"], "min_interval": null, "max_data_points": null}},
        "transforms": [], "fields": {"defaults": {}, "overrides": []}, "options": options, "links": []
    })
}

fn table_panel(id: u32, y: u32, format: &str) -> Value {
    json!({
        "id": id, "type": "table", "title": "Table", "description": "", "transparent": false,
        "grid": {"x": 0, "y": y, "w": 24, "h": 8},
        "data": {"query": {"source": "prod", "format": format, "sql": ["SELECT 1"], "min_interval": null, "max_data_points": null}},
        "transforms": [], "fields": {"defaults": {}, "overrides": []}, "options": {"show_header": true, "sort": []}, "links": []
    })
}

fn doc_of(panels: Vec<Value>) -> Value {
    json!({
        "format_version": 3, "uid": "logs-doc", "title": "Logs", "description": "", "tags": [],
        "time": {"from": "now-6h", "to": "now", "timezone": "utc", "refresh": null},
        "cursor_sync": "off", "links": [], "variables": [],
        "sections": [{"title": null, "collapsed": false, "panels": panels}]
    })
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn expect(v: &Value, pointer: &str, code: ErrorCode) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{e:#?}");
}

fn options_of(v: &Value) -> LogsOptions {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let p = &doc.document().sections[0].panels[0];
    assert_eq!(p.panel_type, PanelType::Logs);
    match &p.options {
        PanelOptions::Logs(o) => *o,
        other => panic!("not a logs panel: {other:?}"),
    }
}

fn canonical(v: &Value) -> (String, Value) {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let text = String::from_utf8(doc.bytes().to_vec()).unwrap();
    let value = serde_json::from_str(&text).unwrap();
    (text, value)
}

#[test]
fn the_type_and_the_format_have_their_texts() {
    assert_eq!(PanelType::Logs.as_str(), "logs");
    assert_eq!(QueryFormat::Logs.as_str(), "logs");
    assert_eq!(LogsOrder::NAMES, &["newest_first", "oldest_first"]);
    assert_eq!(PanelType::parse("logs"), Some(PanelType::Logs));
}

#[test]
fn empty_options_take_the_defaults_of_grafana_and_stay_empty_in_the_canonical_form() {
    let d = doc_of(vec![logs_panel(1, json!({}))]);
    let o = options_of(&d);
    assert_eq!((o.show_time, o.wrap, o.details, o.order), (false, false, true, LogsOrder::NewestFirst));
    let (_, c) = canonical(&d);
    assert_eq!(c["sections"][0]["panels"][0]["options"], json!({}));
    assert_eq!(c["sections"][0]["panels"][0]["data"]["query"]["format"], "logs");
}

#[test]
fn a_member_with_its_default_value_is_valid_and_leaves_the_canonical_form() {
    let d = doc_of(vec![logs_panel(
        1,
        json!({"show_time": false, "wrap": false, "details": true, "order": "newest_first"}),
    )]);
    let (_, c) = canonical(&d);
    assert_eq!(c["sections"][0]["panels"][0]["options"], json!({}));
}

#[test]
fn the_other_values_are_kept_in_the_order_of_the_table() {
    let d = doc_of(vec![logs_panel(
        1,
        json!({"order": "oldest_first", "details": false, "wrap": true, "show_time": true}),
    )]);
    let o = options_of(&d);
    assert_eq!((o.show_time, o.wrap, o.details, o.order), (true, true, false, LogsOrder::OldestFirst));
    let (text, c) = canonical(&d);
    assert_eq!(
        c["sections"][0]["panels"][0]["options"],
        json!({"show_time": true, "wrap": true, "details": false, "order": "oldest_first"})
    );
    let at = |name: &str| text.find(&format!("\"{name}\"")).unwrap();
    assert!(at("show_time") < at("wrap") && at("wrap") < at("details") && at("details") < at("order"), "{text}");
    // A second validation of the canonical bytes gives the same bytes (4.38).
    let again = canonical(&c).0;
    assert_eq!(again, text);
}

#[test]
fn a_bad_option_gives_one_error_at_its_member() {
    let o = format!("{P}/options");
    let cases: [(Value, String, ErrorCode); 6] = [
        (json!({"show_time": "yes"}), format!("{o}/show_time"), ErrorCode::WrongType),
        (json!({"wrap": 1}), format!("{o}/wrap"), ErrorCode::WrongType),
        (json!({"details": null}), format!("{o}/details"), ErrorCode::WrongType),
        (json!({"order": "Descending"}), format!("{o}/order"), ErrorCode::BadFormat),
        (json!({"order": 1}), format!("{o}/order"), ErrorCode::WrongType),
        (json!({"dedup": "none"}), format!("{o}/dedup"), ErrorCode::UnknownMember),
    ];
    for (options, pointer, code) in cases {
        expect(&doc_of(vec![logs_panel(1, options)]), &pointer, code);
    }
    expect(&doc_of(vec![logs_panel(1, json!([]))]), &o, ErrorCode::WrongType);
}

#[test]
fn the_format_logs_is_refused_outside_a_logs_panel() {
    let d = doc_of(vec![table_panel(1, 0, "logs")]);
    expect(&d, &format!("{P}/data/query/format"), ErrorCode::NotPermitted);
    let e = errors_of(&d);
    assert!(e[0].message.contains("4.33f"), "{}", e[0].message);
    // The same rule for an extra query of a table.
    let mut t = table_panel(1, 0, "table");
    t["data"]["extra_queries"] = json!([{"ref": "B", "source": "prod", "format": "logs", "sql": ["SELECT 2"]}]);
    expect(&doc_of(vec![t]), &format!("{P}/data/extra_queries/0/format"), ErrorCode::NotPermitted);
}

#[test]
fn a_logs_panel_takes_each_format_and_the_other_types_take_table_and_time_series() {
    for f in ["logs", "table", "time_series"] {
        let mut p = logs_panel(1, json!({}));
        p["data"]["query"]["format"] = json!(f);
        assert!(errors_of(&doc_of(vec![p])).is_empty(), "{f}");
    }
    assert!(errors_of(&doc_of(vec![table_panel(1, 0, "table"), table_panel(2, 8, "time_series")])).is_empty());
    // An unknown format is still bad_format.
    let mut p = logs_panel(1, json!({}));
    p["data"]["query"]["format"] = json!("log");
    expect(&doc_of(vec![p]), &format!("{P}/data/query/format"), ErrorCode::BadFormat);
}

#[test]
fn a_logs_panel_draws_one_frame() {
    let mut p = logs_panel(1, json!({}));
    p["data"]["extra_queries"] = json!([{"ref": "B", "source": "prod", "format": "logs", "sql": ["SELECT 2"]}]);
    expect(&doc_of(vec![p]), &format!("{P}/data/extra_queries"), ErrorCode::NotPermitted);
    // A reuse names a source with one query.
    let mut source = table_panel(1, 0, "table");
    source["data"]["extra_queries"] = json!([{"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 2"]}]);
    let mut reuse = logs_panel(2, json!({}));
    reuse["grid"]["y"] = json!(8);
    reuse["data"] = json!({"reuse": {"panel": 1, "with_transforms": false}});
    expect(
        &doc_of(vec![source.clone(), reuse.clone()]),
        "/sections/0/panels/1/data/reuse/panel",
        ErrorCode::BadReference,
    );
    source["data"].as_object_mut().unwrap().remove("extra_queries");
    assert!(errors_of(&doc_of(vec![source, reuse])).is_empty());
}

#[test]
fn the_style_members_of_a_series_are_not_permitted_and_the_shared_members_are() {
    let mut p = logs_panel(1, json!({"show_time": true}));
    p["fields"]["defaults"] = json!({"unit": "short", "links": [{"title": "x", "url": "/d/x", "target_blank": false}]});
    p["transforms"] = json!([{"kind": "organize", "hide": ["b"], "order": ["t", "body"], "rename": []}]);
    let e = errors_of(&doc_of(vec![p.clone()]));
    assert!(e.is_empty(), "{e:#?}");
    p["fields"]["defaults"] = json!({"draw": "line"});
    let e = errors_of(&doc_of(vec![p]));
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(
        (e[0].pointer.as_str(), e[0].code),
        (format!("{P}/fields/defaults/draw").as_str(), ErrorCode::NotPermitted)
    );
}
