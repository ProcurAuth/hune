// SPDX-License-Identifier: MIT OR Apache-2.0
//! The query parameters of ClickHouse on the routes (7.6a, 8.3, 8.5; A140 and A141 of 16.8).
//!
//! The fake source records each request, so the tests read the request target, the pairs `param_<name>` and the
//! SQL body. A refusal sends no byte to the source.

use super::api::{JSON, NODES, etag_of, fill, key, put_body};
use super::fake_ch::{FRAME_BODY, Fake, Rec, Step, healthy, ok};
use super::harness::*;
use super::query::{draft_panel, post_panel, post_preview, preview_body};
use serde_json::{Value, json};
use std::path::Path;

/// The fixture with query parameters: panel 1 binds `node` (a list), `account` (a text) and `limit` (a number
/// text); the variable `node` binds `env`; the annotation binds `env`. The constant `k` holds a form.
fn doc() -> Value {
    let mut d: Value = serde_json::from_str(NODES).unwrap();
    d["variables"][0]["sql"] = json!(["SELECT DISTINCT node", "FROM analytics.nodes", "WHERE env = {env:String}"]);
    let vars = d["variables"].as_array_mut().unwrap();
    vars.push(
        json!({"kind": "text", "name": "limit", "label": null, "hide": "none", "value_type": "text", "default": "0"}),
    );
    vars.push(json!({"kind": "constant", "name": "k", "label": null, "hide": "variable", "value": "{x:Identifier}"}));
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("${node}")).unwrap();
    sql[at] = json!("  AND node IN {node:Array(String)} AND account = {account:String} AND weight > {limit:UInt64}");
    d["annotations"] = json!([{
        "name": "Deploys", "source": "prod", "color": "red",
        "sql": ["SELECT deployed_at AS time, title, text, tags", "FROM ops.deploys", "WHERE env = {env:String}"]
    }]);
    d
}

fn write_doc(d: &Value) -> impl FnOnce(&Path) {
    let text = serde_json::to_string_pretty(d).unwrap();
    move |root: &Path| {
        fill(root);
        std::fs::write(root.join("transit/nodes.json"), text).unwrap();
    }
}

/// A source that answers a frame, or the status 414 with no ClickHouse code when the SQL holds `/*414*/`.
fn fake_ch() -> impl std::future::Future<Output = Fake> {
    Fake::start(healthy(|rec: &Rec| {
        if rec.sql().contains("/*414*/") {
            vec![Step::Send(b"HTTP/1.1 414 URI Too Long\r\nContent-Length: 0\r\n\r\n".to_vec())]
        } else {
            ok(rec, FRAME_BODY)
        }
    }))
}

async fn start(fake: &Fake, d: &Value) -> Harness {
    start_with_fake(fake, write_doc(d), |_| {}).await
}

async fn etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

fn vars(node: &[&str], account: &str, limit: &str) -> Value {
    json!({
        "node": {"all": false, "values": node},
        "account": {"all": false, "values": [account]},
        "env": {"all": false, "values": ["prod"]},
        "limit": {"all": false, "values": [limit]},
    })
}

fn body(e: &str, variables: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64, "max_data_points": 1200,
        "variables": variables,
    }))
    .unwrap()
}

/// The names of the pairs `param_<name>` of a request target, in their order, and the text after `log_comment`.
fn param_names(rec: &Rec) -> Vec<String> {
    let query = rec.target.split_once('?').map_or("", |x| x.1);
    query.split('&').filter_map(|p| p.strip_prefix("param_")).map(|p| p.split('=').next().unwrap().to_owned()).collect()
}

async fn put_doc(h: &Harness, d: &Value) -> Resp {
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

// ---- A140: the values go in the URL, the SQL keeps the forms ----------------------------------------

#[tokio::test]
async fn a140_the_values_go_in_the_url_after_log_comment_and_the_sql_keeps_the_forms() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let r = put_doc(&h, &d).await;
    assert_eq!(r.status, 200, "the document is valid: {}", r.text());
    let e = etag(&h).await;
    // A panel query.
    let r = post_panel(&h, 1, &body(&e, vars(&["a1", "b'2\\"], "x]\\,y é", "4096"))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let rec = fake.queries().pop().unwrap();
    let sql = rec.sql();
    assert!(
        sql.contains("AND node IN {node:Array(String)} AND account = {account:String} AND weight > {limit:UInt64}")
    );
    for value in ["a1", "b'2", "x]", "4096"] {
        assert!(!sql.contains(value), "the body holds no value: {value}");
    }
    assert_eq!(param_names(&rec), ["account", "limit", "node"], "the names in byte order");
    let log_comment = rec.target.find("&log_comment=").unwrap();
    assert!(rec.target.find("&param_account=").unwrap() > log_comment, "the pairs come after log_comment");
    assert_eq!(rec.param("param_node").as_deref(), Some("['a1','b\\'2\\\\']"));
    assert_eq!(rec.param("param_account").as_deref(), Some("x]\\\\,y é"));
    assert_eq!(rec.param("param_limit").as_deref(), Some("4096"));
    // The member `sql` of Inspect is off in this configuration; the hash of the log line is the hash of the SQL:
    // the same values give the same hash, and other values give the same SQL (PQ).
    let r = post_panel(&h, 1, &body(&e, vars(&["zz"], "other", "1"))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(fake.queries().pop().unwrap().sql(), sql, "PQ: the same SQL for other values");
    // The options route and the annotation route bind `env`.
    let options = serde_json::to_vec(&json!({
        "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "variables": {"env": {"all": false, "values": ["prod"]}},
    }))
    .unwrap();
    let r = call(h.addr, "POST", "/api/dashboards/nodes/variables/node/options", &[JSON], &options).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let rec = fake.queries().pop().unwrap();
    assert!(rec.sql().ends_with("WHERE env = {env:String}"));
    assert_eq!((param_names(&rec), rec.param("param_env")), (vec!["env".to_owned()], Some("prod".to_owned())));
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &[JSON], &options).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let rec = fake.queries().pop().unwrap();
    assert_eq!(rec.param("param_env").as_deref(), Some("prod"));
    // The preview of a draft panel.
    let mut b = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    b["panel"] = draft_panel("SELECT toDateTime(1) AS t, {account:String} AS w WHERE $__timeFilter(t)");
    b["dashboard_variables"] = d["variables"].clone();
    b["variables"] = json!({"account": {"all": false, "values": ["it's"]}});
    let r = post_preview(&h, "prod", &serde_json::to_vec(&b).unwrap()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let rec = fake.queries().pop().unwrap();
    assert_eq!(rec.param("param_account").as_deref(), Some("it's"));
    // The member `names` of 6.14 names the variables of the forms, so the page sends their values.
    let names = get(h.addr, "/api/dashboards/nodes").await.json()["names"].clone();
    let first = names["panels"]["1"][0].as_array().unwrap();
    for n in ["node", "account", "limit"] {
        assert!(first.iter().any(|x| x == n), "{n}: {names}");
    }
    assert_eq!(names["variables"]["node"], json!(["env"]), "{names}");
    assert_eq!(names["annotations"][0], json!(["env"]), "{names}");
}

// ---- A141: the refusals send no byte to the source ------------------------------------------------

#[tokio::test]
async fn a141_a_refused_value_a_cap_and_a_forged_form_give_422_and_no_query() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let before = fake.queries().len();
    // The grammar of the type: the reason names the variable and the type, never the value.
    for limit in ["-1", "1.5", "18446744073709551616", "1e3", "x", "007", "-0"] {
        let r = post_panel(&h, 1, &body(&e, vars(&["a1"], "w", limit))).await;
        assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{limit}: {}", r.text());
        assert_eq!(r.json()["error"]["variable"], "limit");
        let reason = r.json()["error"]["reason"].as_str().unwrap().to_owned();
        assert!(reason.contains("UInt64") && !reason.contains(limit), "{reason}");
    }
    // The cap of the items of an Array value.
    let many: Vec<String> = (0..1001).map(|i| format!("v{i}")).collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    let r = post_panel(&h, 1, &body(&e, vars(&refs, "w", "1"))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert!(r.json()["error"]["reason"].as_str().unwrap().ends_with("use the text form for long lists."));
    // The cap of the bytes: three values of 1,024 blanks take more than 4,096 bytes after the encoding.
    let wide = " ".repeat(1024);
    let r = post_panel(&h, 1, &body(&e, vars(&[&wide, &wide], &wide, "1"))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    // A constant in code makes a new form in the final text: the second scan refuses it.
    let mut b = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    b["panel"] = draft_panel("SELECT toDateTime(1) AS t, $k, {account:String} WHERE $__timeFilter(t)");
    b["dashboard_variables"] = d["variables"].clone();
    b["variables"] = json!({"account": {"all": false, "values": ["w"]}});
    let r = post_preview(&h, "prod", &serde_json::to_vec(&b).unwrap()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    assert!(r.json()["error"].get("line").is_none(), "a fault of the final text has no position");
    // A loose form in a draft gives `invalid_document` with the pointer of the line.
    b["panel"] = draft_panel("SELECT toDateTime(1) AS t, { account:String} WHERE $__timeFilter(t)");
    let r = post_preview(&h, "prod", &serde_json::to_vec(&b).unwrap()).await;
    assert_eq!(r.status, 422, "{}", r.text());
    assert_eq!(fake.queries().len(), before, "no refusal sent a byte to the source");
}

#[tokio::test]
async fn a141_the_save_refuses_each_fault_of_7_18_items_10_to_15() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc()).await;
    let line = "/sections/0/panels/0/data/query/sql/6";
    let cases: [(&str, &str); 8] = [
        ("  AND account = { account:String}", "bad_format"),
        ("  AND account = {`account`:String}", "bad_format"),
        ("  AND account = {account:Identifier}", "not_permitted"),
        ("  AND account = {nope:String}", "bad_reference"),
        ("  AND account = {account:Array(String)}", "not_permitted"),
        ("  AND node IN {node:String}", "not_permitted"),
        ("  AND account = {account:String} AND w2 = {account:Int64}", "not_permitted"),
        ("  AND account = {account:String} AND w2 = '$account'", "not_permitted"),
    ];
    for (text, code) in cases {
        let mut d = doc();
        d["sections"][0]["panels"][0]["data"]["query"]["sql"][6] = json!(text);
        let r = put_doc(&h, &d).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{text}: {}", r.text());
        assert_eq!(errors_of(&r), vec![(line.to_owned(), code.to_owned())], "{text}");
    }
}

// ---- 8.5: a status 414 or 431 of a proxy -----------------------------------------------------------

#[tokio::test]
async fn a_status_414_of_a_proxy_gives_422_query_error_and_keeps_the_source_up() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let r = post_panel(&h, 1, &body(&e, vars(&["a1"], "w", "1"))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let state = |j: Value| j["items"][0]["state"].as_str().unwrap().to_owned();
    assert_eq!(state(get(h.addr, "/api/sources").await.json()), "up");
    let mut b = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    b["panel"] = draft_panel("SELECT toDateTime(1) AS t /*414*/, {account:String} WHERE $__timeFilter(t)");
    b["dashboard_variables"] = d["variables"].clone();
    b["variables"] = json!({"account": {"all": false, "values": ["w"]}});
    let r = post_preview(&h, "prod", &serde_json::to_vec(&b).unwrap()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "query_error"), "{}", r.text());
    let err = &r.json()["error"];
    assert!(err["clickhouse_code"].is_null(), "{err}");
    assert_eq!(err["source"], "prod");
    assert!(err["query_id"].is_string());
    assert_eq!(err["retryable"], false);
    assert_eq!(state(get(h.addr, "/api/sources").await.json()), "up", "a long request never marks a source down");
    let r = post_panel(&h, 1, &body(&e, vars(&["a1"], "w", "1"))).await;
    assert_eq!(r.status, 200, "{}", r.text());
}
