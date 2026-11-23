// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ad hoc filters on the routes (4.10c, 6.14, 7.7a, 7.10a, 7.18, 13.12).
//!
//! The fake source records each request, so the tests read the SQL body. A refusal sends no byte to the source: the
//! count of the recorded queries does not change.

use super::api::{JSON, NODES, etag_of, fill, key, put_body};
use super::fake_ch::{FRAME_BODY, Fake, Rec, healthy, ok};
use super::harness::*;
use super::query::post_panel;
use serde_json::{Value, json};
use std::path::Path;

const COLUMNS: [&str; 4] = ["env", "node", "weight", "zone_id"];

/// The fixture with the adhoc variable `filters` on `prod` (keys `node` and `env`) and the macro in panel 1.
fn doc() -> Value {
    let mut d: Value = serde_json::from_str(NODES).unwrap();
    d["variables"].as_array_mut().unwrap().push(json!({
        "kind": "adhoc", "name": "filters", "label": "Filters", "hide": "none", "source": "prod", "keys": ["node", "env"]
    }));
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("${node}")).unwrap();
    sql.insert(at + 1, json!("  AND $__adhocFilters"));
    d
}

fn write_doc(d: &Value) -> impl FnOnce(&Path) {
    let text = serde_json::to_string_pretty(d).unwrap();
    move |root: &Path| {
        fill(root);
        std::fs::write(root.join("transit/nodes.json"), text).unwrap();
    }
}

fn fake_ch() -> impl std::future::Future<Output = Fake> {
    Fake::start(healthy(|rec: &Rec| ok(rec, FRAME_BODY)))
}

async fn start(fake: &Fake, d: &Value, columns: Option<&[&str]>) -> Harness {
    let list = columns.map(|c| c.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    start_with_fake(fake, write_doc(d), move |c| c.clickhouse[0].adhoc_columns = list).await
}

async fn etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

fn vars(filters: Value) -> Value {
    let mut v = json!({
        "node": {"all": false, "values": ["a1"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
    });
    if !filters.is_null() {
        v["filters"] = filters;
    }
    v
}

fn body(e: &str, variables: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64, "max_data_points": 1200,
        "variables": variables,
    }))
    .unwrap()
}

fn f(key: &str, op: &str, value: &str) -> Value {
    json!({"key": key, "op": op, "value": value})
}

async fn validate_only(h: &Harness, d: &Value) -> Resp {
    let e = etag(h).await;
    call(
        h.addr,
        "PUT",
        "/api/dashboards/nodes?validate_only=true",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&serde_json::to_string(d).unwrap(), "m"),
    )
    .await
}

fn errors_of(r: &Resp) -> Vec<(String, String)> {
    r.json()["error"]["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| (e["pointer"].as_str().unwrap().to_owned(), e["code"].as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn the_filters_reach_the_sql_through_the_macro_and_the_read_answer_gives_the_permitted_keys() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d, Some(&COLUMNS)).await;
    let r = validate_only(&h, &d).await;
    assert_eq!(r.status, 200, "the document is valid: {}", r.text());
    // 6.14: the permitted keys, and the adhoc variable in the names of the query with the macro.
    let read = get(h.addr, "/api/dashboards/nodes").await.json();
    assert_eq!(read["adhoc"], json!({"filters": ["node", "env"]}));
    let first = read["names"]["panels"]["1"][0].as_array().unwrap().clone();
    assert!(first.iter().any(|n| n == "filters"), "{first:?}");
    assert!(!read["names"]["panels"]["2"][0].as_array().unwrap().iter().any(|n| n == "filters"));
    let e = etag(&h).await;
    // Two filters: the identifiers in backquotes, the values as string literals.
    let r = post_panel(&h, 1, &body(&e, vars(json!({"filters": [f("env", "!=", "x'y\\"), f("node", "=", "")]})))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sql = fake.queries().pop().unwrap().sql();
    assert!(sql.contains("  AND (`env` != 'x\\'y\\\\' AND `node` = '')\n"), "{sql}");
    // No filter, and no member: 1=1.
    for filters in [json!({"filters": []}), Value::Null] {
        let r = post_panel(&h, 1, &body(&e, vars(filters))).await;
        assert_eq!(r.status, 200, "{}", r.text());
        assert!(fake.queries().pop().unwrap().sql().contains("  AND 1=1\n"));
    }
    // A panel without the macro ignores nothing: the filters are checked and the SQL does not change.
    let r = post_panel(&h, 2, &body(&e, vars(json!({"filters": [f("env", "=", "a")]})))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(!fake.queries().pop().unwrap().sql().contains("`env`"));
    // The log line: the count and the keys, never a value.
    let lines: Vec<Value> = h.logger.memory_lines().iter().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect();
    let adhoc: Vec<&Value> = lines.iter().filter(|l| l["event"] == "query.adhoc").collect();
    assert!(!adhoc.is_empty(), "a line query.adhoc");
    assert_eq!(adhoc[0]["count"], 2);
    assert_eq!(adhoc[0]["keys"], json!(["env", "node"]));
    assert_eq!(adhoc[0]["variable"], "filters");
    let all = h.logger.memory_lines().join("\n");
    assert!(!all.contains("x'y") && !all.contains("x\\\\'y"), "no value in the log");
}

#[tokio::test]
async fn an_unknown_key_an_unknown_operator_and_17_filters_give_422_and_no_source_query() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d, Some(&COLUMNS)).await;
    let e = etag(&h).await;
    let before = fake.queries().len();
    let many: Vec<Value> = (0..17).map(|i| f("env", "=", &format!("v{i}"))).collect();
    let cases = vec![
        (vec![f("zone", "=", "a")], "key"),
        // In the configured list but not a key of the variable.
        (vec![f("weight", "=", "1")], "key"),
        (vec![f("Env", "=", "a")], "key"),
        (vec![f("env ", "=", "a")], "key"),
        (vec![f("env", "<", "1")], "operator"),
        (vec![f("env", "=~", "a.*")], "operator"),
        (vec![f("env", "IN", "(1)")], "operator"),
        (vec![f("env", "=", "a\nb")], "control"),
        (vec![f("env", "=", &"x".repeat(1025))], "1024"),
        (vec![f("env", "=", "ok"), f("nope", "=", "x")], "Filter 2"),
        (many, "16 filters"),
    ];
    for (filters, reason) in cases {
        let r = post_panel(&h, 1, &body(&e, vars(json!({ "filters": filters })))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{reason}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "filters");
        let text = r.json()["error"]["reason"].as_str().unwrap().to_owned();
        assert!(text.contains(reason), "{reason}: {text}");
        // The other query routes check the filters too (never ignored): the options of a variable.
        let options = serde_json::to_vec(&json!({
            "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
            "variables": {"env": {"all": false, "values": ["prod"]}, "filters": {"filters": filters}},
        }))
        .unwrap();
        let r = call(h.addr, "POST", "/api/dashboards/nodes/variables/node/options", &[JSON], &options).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "options {reason}: {}", r.text());
    }
    // A member that breaks the closed shape: 400.
    for member in [
        json!({"all": false, "values": []}),
        json!({"filters": [{"key": "env", "op": "="}]}),
        json!({"filters": [{"key": "env", "op": "=", "value": "a", "x": 1}]}),
        json!({"filters": {}}),
        json!([]),
    ] {
        let r = post_panel(&h, 1, &body(&e, vars(member.clone()))).await;
        assert_eq!(r.status, 400, "{member}: {}", r.text());
    }
    assert_eq!(fake.queries().len(), before, "no request reached the source");
}

#[tokio::test]
async fn a_change_of_the_configured_list_fails_closed() {
    let d = doc();
    // The stored document names `node`; the list of the configuration has it no more.
    for columns in [Some(&["env"][..]), None] {
        let fake = fake_ch().await;
        let h = start(&fake, &d, columns).await;
        let read = get(h.addr, "/api/dashboards/nodes").await.json();
        let expected = if columns.is_some() { json!({"filters": ["env"]}) } else { json!({"filters": []}) };
        assert_eq!(read["adhoc"], expected);
        let e = etag(&h).await;
        let before = fake.queries().len();
        let r = post_panel(&h, 1, &body(&e, vars(json!({"filters": [f("node", "=", "a")]})))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
        assert_eq!(fake.queries().len(), before);
        // No filter still runs the panel.
        let r = post_panel(&h, 1, &body(&e, vars(json!({"filters": []})))).await;
        assert_eq!(r.status, 200, "{}", r.text());
        // The save refuses the key that the list has not.
        let r = validate_only(&h, &d).await;
        assert_eq!(r.status, 422, "{}", r.text());
        let errors = errors_of(&r);
        if columns.is_some() {
            assert_eq!(errors, [("/variables/3/keys/0".to_owned(), "not_permitted".to_owned())]);
        } else {
            assert_eq!(errors, [("/variables/3/source".to_owned(), "not_permitted".to_owned())]);
        }
    }
}

#[tokio::test]
async fn the_checks_at_save_follow_the_source_of_the_variable() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc(), Some(&COLUMNS)).await;
    // The macro in a panel of a source with no adhoc variable: the variable leaves the document.
    let mut d = doc();
    d["variables"].as_array_mut().unwrap().pop();
    let r = validate_only(&h, &d).await;
    assert_eq!(errors_of(&r), [("/sections/0/panels/0/data/query/sql/7".to_owned(), "not_permitted".to_owned())]);
    // An unknown source.
    let mut d = doc();
    d["variables"][3]["source"] = json!("nowhere");
    let r = validate_only(&h, &d).await;
    assert!(errors_of(&r).contains(&("/variables/3/source".to_owned(), "bad_reference".to_owned())), "{}", r.text());
    // The macro in the query of a variable.
    let mut d = doc();
    d["variables"][0]["sql"] = json!(["SELECT DISTINCT node FROM analytics.nodes WHERE $__adhocFilters"]);
    let r = validate_only(&h, &d).await;
    assert!(errors_of(&r).contains(&("/variables/0/sql/0".to_owned(), "not_permitted".to_owned())), "{}", r.text());
    // A placeholder of the adhoc variable.
    let mut d = doc();
    d["sections"][0]["panels"][0]["data"]["query"]["sql"][7] = json!("  AND x = '${filters}'");
    let r = validate_only(&h, &d).await;
    assert!(errors_of(&r).contains(&("/sections/0/panels/0/data/query/sql/7".to_owned(), "not_permitted".to_owned())));
    // The run of a stored document that breaks a rule fails closed: the macro after OR.
    let mut d = doc();
    d["sections"][0]["panels"][0]["data"]["query"]["sql"][7] = json!("  OR $__adhocFilters");
    let fake = fake_ch().await;
    let h = start(&fake, &d, Some(&COLUMNS)).await;
    let e = etag(&h).await;
    let before = fake.queries().len();
    let r = post_panel(&h, 1, &body(&e, vars(json!({"filters": []})))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    assert_eq!(fake.queries().len(), before);
}
