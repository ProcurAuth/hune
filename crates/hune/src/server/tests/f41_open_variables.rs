// SPDX-License-Identifier: MIT OR Apache-2.0
//! The member `allow_custom_value` on the routes (F41, A90 to A98 of 16.8, C20 to C22 of 16.9).
//!
//! The four routes that check the values of a request are 6.20 (panel query), 6.21 (options of a variable),
//! 6.21a (annotation query) and 6.22 (preview, with a draft panel and with a draft variable).

use super::api::{JSON, NODES, etag_of, fill, key, put_body};
use super::fake_ch::{FRAME_BODY, Fake, Rec, healthy, ok};
use super::harness::*;
use super::query::{draft_panel, panel_body, post_panel, post_preview, preview_body};
use serde_json::{Value, json};
use std::path::Path;

/// The member to set on a variable: `None` removes it.
type Member = Option<bool>;

fn set_member(v: &mut Value, member: Member) {
    match member {
        Some(b) => v["allow_custom_value"] = json!(b),
        None => {
            v.as_object_mut().unwrap().remove("allow_custom_value");
        }
    }
}

/// The fixture dashboard with the variables `node` (query, parents `env`), `env` (custom, one value) and
/// `zones` (custom, several values), and one annotation. The members come from the arguments.
fn doc(node: Member, env: Member, zones: Member) -> Value {
    let mut d: Value = serde_json::from_str(NODES).unwrap();
    d["variables"][0]["sql"] = json!(["SELECT DISTINCT node", "FROM analytics.nodes", "WHERE env = '${env}'"]);
    set_member(&mut d["variables"][0], node);
    set_member(&mut d["variables"][2], env);
    d["variables"].as_array_mut().unwrap().push(json!({
        "kind": "custom", "name": "zones", "label": null, "hide": "none",
        "options": [{"text": "A", "value": "a"}, {"text": "B", "value": "b"}],
        "multi": true, "include_all": true, "all_value": null, "default": []
    }));
    set_member(&mut d["variables"][3], zones);
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("$__timeFilter")).unwrap();
    sql.insert(at + 1, json!("  AND env = '${env}' AND zone IN (${zones:singlequote})"));
    d["annotations"] = json!([{
        "name": "Deploys", "source": "prod", "color": "red",
        "sql": ["SELECT deployed_at AS time, title, text, tags", "FROM ops.deploys", "WHERE env = '${env}'"]
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

async fn start(fake: &Fake, d: &Value) -> Harness {
    start_with_fake(fake, write_doc(d), |_| {}).await
}

fn vars(env: &[&str], zones: &[&str]) -> Value {
    json!({
        "node": {"all": false, "values": ["a1"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": env},
        "zones": {"all": false, "values": zones},
    })
}

/// The answer of one route: the status, the code of an error and the SQL that the source got.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Answer {
    route: &'static str,
    status: u16,
    code: String,
    sql: Option<String>,
}

async fn etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

/// Calls the five forms of the four routes with the same values. The source records each query.
async fn four_routes(h: &Harness, fake: &Fake, d: &Value, values: Value) -> Vec<Answer> {
    let e = etag(h).await;
    let range = json!({"etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64});
    let with = |extra: Value| {
        let mut b = range.clone();
        b["variables"] = values.clone();
        for (k, v) in extra.as_object().unwrap() {
            b[k] = v.clone();
        }
        serde_json::to_vec(&b).unwrap()
    };
    let mut out = Vec::new();
    let mut seen = fake.queries().len();
    let mut add = |route: &'static str, r: Resp, fake: &Fake| {
        let queries: Vec<Rec> = fake.queries().into_iter().skip(seen).collect();
        seen += queries.len();
        let code = if r.status == 200 { String::new() } else { r.code() };
        out.push(Answer { route, status: r.status, code, sql: queries.first().map(Rec::sql) });
    };
    let r = post_panel(h, 1, &with(json!({"max_data_points": 1200}))).await;
    add("6.20", r, fake);
    let r = call(h.addr, "POST", "/api/dashboards/nodes/variables/node/options", &[JSON], &with(json!({}))).await;
    add("6.21", r, fake);
    let r = call(h.addr, "POST", "/api/dashboards/nodes/annotations/0/query", &[JSON], &with(json!({}))).await;
    add("6.21a", r, fake);
    let panel = draft_panel("SELECT toDateTime(1) AS t, '${env}' AS n WHERE $__timeFilter(t)");
    let body = preview_body(json!({"panel": panel, "dashboard_variables": d["variables"], "variables": values}));
    let r = post_preview(h, "prod", &body).await;
    add("6.22 panel", r, fake);
    let body = preview_body(json!({
        "panel": null, "max_data_points": null, "variable": "node",
        "dashboard_variables": d["variables"], "variables": values
    }));
    let r = post_preview(h, "prod", &body).await;
    add("6.22 variable", r, fake);
    out
}

fn fake_ch() -> impl std::future::Future<Output = Fake> {
    Fake::start(healthy(|rec| ok(rec, FRAME_BODY)))
}

// ---- A90, A91: a closed custom variable ---------------------------------------------------------

#[tokio::test]
async fn a90_a_closed_custom_variable_refuses_a_value_outside_the_options_on_the_four_routes() {
    for member in [None, Some(false)] {
        let d = doc(None, member, member);
        let fake = fake_ch().await;
        let h = start(&fake, &d).await;
        // A value outside the options, and a value with another case, a blank and another Unicode form.
        for bad in ["evil", "PROD", "prod ", " prod", "prod\u{a0}", "pro\u{64}\u{301}", "\u{ff50}rod", "Production", ""]
        {
            let answers = four_routes(&h, &fake, &d, vars(&[bad], &["a"])).await;
            for a in &answers {
                assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{member:?} {bad:?} {a:?}");
            }
        }
        // A list with one item outside the options, and the text of an option instead of its value.
        for list in [vec!["a", "zz"], vec!["zz"], vec!["A"], vec!["a", "b", "a "]] {
            let answers = four_routes(&h, &fake, &d, vars(&["prod"], &list)).await;
            for a in &answers {
                assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{member:?} {list:?} {a:?}");
            }
        }
        // No source query ran for any of them (the test ClickHouse is the source of the log).
        assert!(fake.queries().iter().all(Rec::is_probe), "a query reached the source: {:?}", fake.queries());
        // A good request still runs on the four routes.
        let answers = four_routes(&h, &fake, &d, vars(&["prod"], &["a", "b"])).await;
        for a in &answers {
            assert!(a.sql.is_some(), "{a:?}");
        }
    }
}

#[tokio::test]
async fn a91_the_refusal_names_the_variable_and_the_reason() {
    let d = doc(None, None, None);
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": vars(&["evil"], &["a"])}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    let err = r.json()["error"].clone();
    assert_eq!(err["variable"], "env");
    assert!(err["reason"].as_str().unwrap().contains("not an option"));
    // The refusal never repeats the value.
    assert!(!r.text().contains("evil"), "{}", r.text());
    assert!(fake.queries().iter().all(Rec::is_probe));
}

// ---- A92, A94, C20: an open custom variable -------------------------------------------------------

#[tokio::test]
async fn a92_an_open_custom_variable_gives_a_literal_for_a_hostile_value() {
    let d = doc(Some(true), Some(true), Some(true));
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    // C20: the value of the injection test of 16.9.
    let hostile = "x' OR 1=1 --";
    let answers = four_routes(&h, &fake, &d, vars(&[hostile], &["a"])).await;
    for a in &answers {
        assert_eq!(a.status, 200, "{a:?}");
        let sql = a.sql.as_deref().unwrap();
        // One escaped literal: the quote is escaped and the text stays inside the string.
        assert!(sql.contains("'x\\' OR 1=1 --'"), "{}: {sql}", a.route);
        assert!(!sql.contains("= 'x' OR"), "{}: {sql}", a.route);
    }
    // A value in a list in a string: the format singlequote makes literals.
    let answers = four_routes(&h, &fake, &d, vars(&["prod"], &["a'b", "c"])).await;
    assert!(answers[0].sql.as_deref().unwrap().contains("zone IN ('a\\'b','c')"), "{:?}", answers[0]);
    // The value that is an option stays free text: no error and the same literal.
    let answers = four_routes(&h, &fake, &d, vars(&["preprod"], &["a"])).await;
    assert!(answers[0].sql.as_deref().unwrap().contains("env = 'preprod'"));
}

#[tokio::test]
async fn a94_the_bounds_of_a_typed_value() {
    let d = doc(Some(true), Some(true), Some(true));
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let edge = "a".repeat(1024);
    let long = "a".repeat(1025);
    let cases: Vec<(&str, String, u16)> = vec![
        ("0 bytes", String::new(), 422),
        ("1 byte", "x".into(), 200),
        ("1,024 bytes", edge, 200),
        ("1,025 bytes", long, 422),
        ("a control character", "a\u{1}b".into(), 422),
        ("DEL", "a\u{7f}b".into(), 422),
        ("the reserved value", "$__all".into(), 422),
    ];
    for (name, value, status) in cases {
        let answers = four_routes(&h, &fake, &d, vars(&[value.as_str()], &["a"])).await;
        for a in &answers {
            assert_eq!(a.status, status, "{name}: {a:?}");
            if status == 422 {
                assert_eq!(a.code, "variable_value_refused", "{name}: {a:?}");
            }
        }
    }
    // A variable that is not `multi` takes exactly one value.
    let e = etag(&h).await;
    let r = post_panel(
        &h,
        1,
        &panel_body(
            &e,
            json!({"variables": {
                "node": {"all": false, "values": ["a1"]},
                "account": {"all": false, "values": [""]},
                "env": {"all": false, "values": ["a", "b"]},
            }}),
        ),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"));
}

#[tokio::test]
async fn the_list_all_of_an_open_custom_variable_stays_a_list_of_the_document() {
    let d = doc(Some(true), Some(true), Some(true));
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let mut v = vars(&["prod"], &[]);
    v["zones"] = json!({"all": true, "values": []});
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": v}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let q = fake.queries();
    let sql = q.last().unwrap().sql();
    assert!(sql.contains("zone IN ('a','b')"), "{sql}");
}

// ---- A93: the save, the validation and the restore ----------------------------------------------

fn quoted_name_doc(env: Member) -> Value {
    let mut d = doc(None, env, None);
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("FROM analytics")).unwrap();
    sql[at] = json!("FROM analytics.`${env}`");
    d
}

async fn put_doc(h: &Harness, d: &Value, query: &str) -> Resp {
    let e = etag(h).await;
    call(
        h.addr,
        "PUT",
        &format!("/api/dashboards/nodes{query}"),
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
async fn a93_an_open_custom_variable_in_a_quoted_name_is_refused_at_save_and_at_validate_only() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc(None, None, None)).await;
    // The closed variable is saved: a quoted name is valid for it.
    let r = put_doc(&h, &quoted_name_doc(None), "").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let r = put_doc(&h, &quoted_name_doc(Some(false)), "?validate_only=true").await;
    assert_eq!(r.status, 200, "{}", r.text());
    // The open variable is refused with `not_permitted` at the line of the SQL.
    for query in ["?validate_only=true", ""] {
        let r = put_doc(&h, &quoted_name_doc(Some(true)), query).await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{query} {}", r.text());
        let errors = errors_of(&r);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].0.starts_with("/sections/0/panels/0/data/query/sql/"), "{errors:?}");
        assert_eq!(errors[0].1, "not_permitted");
    }
    // An open variable in a string, a list or a number is valid.
    let r = put_doc(&h, &doc(Some(true), Some(true), Some(true)), "?validate_only=true").await;
    assert_eq!(r.status, 200, "{}", r.text());
}

#[tokio::test]
async fn a93_a_restore_refuses_a_version_with_an_open_custom_variable_in_a_quoted_name() {
    let bad = serde_json::to_string_pretty(&quoted_name_doc(Some(true))).unwrap();
    let fake = fake_ch().await;
    let h = start_with_fake(
        &fake,
        |root| {
            fill(root);
            std::fs::write(root.join("transit/nodes.json"), &bad).unwrap();
        },
        |_| {},
    )
    .await;
    // Version 1 is the file, which the core validator accepts and the checks of 7.18 refuse.
    let e0 = etag(&h).await;
    let r = put_doc(&h, &doc(None, None, None), "").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let e1 = etag_of(&r);
    assert_ne!(e0, e1);
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/restore",
        &[JSON, ("If-Match", &format!("\"{e1}\"")), ("Idempotency-Key", &key())],
        br#"{"version":1}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(errors_of(&r)[0].1, "not_permitted");
}

#[tokio::test]
async fn the_preview_refuses_an_open_custom_variable_in_a_quoted_name() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc(None, None, None)).await;
    let d = quoted_name_doc(Some(true));
    let panel = d["sections"][0]["panels"][0].clone();
    let body = preview_body(
        json!({"panel": panel, "dashboard_variables": d["variables"], "variables": vars(&["prod"], &["a"])}),
    );
    let r = post_preview(&h, "prod", &body).await;
    assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
    assert_eq!(errors_of(&r)[0].1, "not_permitted");
    // The same draft with a closed variable runs: the preview reads the member as the save does.
    let d = quoted_name_doc(Some(false));
    let panel = d["sections"][0]["panels"][0].clone();
    let body = preview_body(
        json!({"panel": panel, "dashboard_variables": d["variables"], "variables": vars(&["prod"], &["a"])}),
    );
    let r = post_preview(&h, "prod", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(fake.queries().last().unwrap().sql().contains("analytics.`prod`"));
}

// ---- A96, C21, C22: a query variable -------------------------------------------------------------

#[tokio::test]
async fn a96_a_query_variable_gives_the_same_answer_with_the_member_true_and_false() {
    let values = |v: &[&str]| {
        json!({
            "node": {"all": false, "values": v},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
            "zones": {"all": false, "values": ["a"]},
        })
    };
    let hostile: [&[&str]; 6] = [&["not-an-option"], &["x' OR 1=1 --"], &["$secret", "b"], &["a1", "zz"], &[], &["\\"]];
    let mut compared = 0;
    for list in hostile {
        let mut answers: Vec<Vec<Answer>> = Vec::new();
        for member in [None, Some(false), Some(true)] {
            let d = doc(member, None, None);
            let fake = fake_ch().await;
            let h = start(&fake, &d).await;
            answers.push(four_routes(&h, &fake, &d, values(list)).await);
        }
        assert_eq!(answers[0], answers[1], "{list:?}: absent and false");
        assert_eq!(answers[1], answers[2], "{list:?}: false and true");
        compared += answers[0].len();
    }
    assert_eq!(compared, 30);
}

#[tokio::test]
async fn c22_a_query_variable_with_a_value_outside_its_options_runs_as_free_text() {
    let d = doc(Some(false), None, None);
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let mut v = vars(&["prod"], &["a"]);
    v["node"] = json!({"all": false, "values": ["zz'; DROP TABLE t; --", "b"]});
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": v}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sql = fake.queries().last().unwrap().sql();
    assert!(sql.contains("node IN ('zz\\'; DROP TABLE t; --','b')"), "{sql}");
}

// ---- A97: the time of the check -------------------------------------------------------------------

#[tokio::test]
async fn a97_the_check_of_a_closed_custom_variable_takes_time_in_proportion_to_the_count() {
    let mut d = doc(None, None, None);
    let options: Vec<Value> =
        (0..10_000).map(|i| json!({"text": format!("Option {i}"), "value": format!("{i:05}")})).collect();
    d["variables"][3]["options"] = json!(options);
    d["variables"][3]["default"] = json!([]);
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let mut values: Vec<String> = (0..9_999).map(|i| format!("{i:05}")).collect();
    values.push("not-an-option".into());
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    let mut v = vars(&["prod"], &[]);
    v["zones"] = json!({"all": false, "values": refs});
    let body = panel_body(&e, json!({"variables": v}));
    let start = std::time::Instant::now();
    let r = post_panel(&h, 1, &body).await;
    let elapsed = start.elapsed();
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert!(elapsed.as_millis() <= 250, "the request took {elapsed:?}");
    assert!(fake.queries().iter().all(Rec::is_probe));
}

// ---- A98: the closed schema -----------------------------------------------------------------------

#[tokio::test]
async fn a98_the_member_on_a_text_variable_or_with_a_wrong_type_is_refused() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc(None, None, None)).await;
    let base = doc(None, None, None);
    let mut text = base.clone();
    text["variables"][1]["allow_custom_value"] = json!(true);
    let mut as_text = base.clone();
    as_text["variables"][0]["allow_custom_value"] = json!("true");
    let mut as_number = base.clone();
    as_number["variables"][2]["allow_custom_value"] = json!(1);
    for (d, pointer, code) in [
        (text, "/variables/1/allow_custom_value", "unknown_member"),
        (as_text, "/variables/0/allow_custom_value", "wrong_type"),
        (as_number, "/variables/2/allow_custom_value", "wrong_type"),
    ] {
        let r = put_doc(&h, &d, "?validate_only=true").await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{}", r.text());
        assert_eq!(errors_of(&r), vec![(pointer.to_owned(), code.to_owned())]);
    }
}

// ---- the questions to the PDP -------------------------------------------------------------------

#[tokio::test]
async fn the_questions_to_the_pdp_are_the_same_with_and_without_the_member() {
    use super::authzen_pdp::*;
    use std::time::Duration;
    let mut runs: Vec<Vec<(String, String)>> = Vec::new();
    for (node, env) in [(None, None), (Some(true), Some(true))] {
        let d = doc(node, env, env);
        let fake = fake_ch().await;
        let pdp = Pdp::start(|_| Ans::yes()).await;
        let h = start_with_fake(&fake, write_doc(&d), |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
        let token = token("user:alice");
        let auth = [(HEADER, token.as_str()), JSON];
        let e = etag_of(&call(h.addr, "GET", "/api/dashboards/nodes", &auth[..1], b"").await);
        pdp.clear();
        let range = json!({"etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64});
        let mut b = range.clone();
        b["variables"] = vars(&["prod"], &["a"]);
        let mut panel = b.clone();
        panel["max_data_points"] = json!(1200);
        let routes = [
            ("/api/dashboards/nodes/panels/1/query", panel),
            ("/api/dashboards/nodes/variables/node/options", b.clone()),
            ("/api/dashboards/nodes/annotations/0/query", b.clone()),
        ];
        for (path, body) in routes {
            let r = call(h.addr, "POST", path, &auth, &serde_json::to_vec(&body).unwrap()).await;
            assert_eq!(r.status, 200, "{path}: {}", r.text());
        }
        let r = call(
            h.addr,
            "PUT",
            "/api/dashboards/nodes?validate_only=true",
            &[auth[0], JSON, ("If-Match", &format!("\"{e}\""))],
            &put_body(&serde_json::to_string(&d).unwrap(), "m"),
        )
        .await;
        assert_eq!(r.status, 200, "{}", r.text());
        runs.push(pdp.asked().iter().map(|q| (q.action.clone(), q.short())).collect());
    }
    assert!(!runs[0].is_empty());
    assert_eq!(runs[0], runs[1]);
}
