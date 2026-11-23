// SPDX-License-Identifier: MIT OR Apache-2.0
//! The variable kinds `interval` and `constant` on the routes (A103 to A110 of 16.8,
//! C33 and C34 of 16.9).
//!
//! The four routes that check the values of a request are 6.20 (panel query), 6.21 (options of a variable),
//! 6.21a (annotation query) and 6.22 (preview, with a draft panel and with a draft variable).

use super::api::{JSON, NODES, etag_of, fill, key, put_body};
use super::fake_ch::{FRAME_BODY, Fake, Rec, healthy, ok};
use super::harness::*;
use super::query::{draft_panel, panel_body, post_panel, post_preview, preview_body};
use serde_json::{Value, json};
use std::path::Path;

/// The text of the constant of the document: it has the hostile characters of C32.
const CONST: &str = "a'b\"c$d\\e;--x";

/// The fixture dashboard with an `interval` variable `bucket` (auto, 30 steps, 10 s), a closed one `fixed`
/// (no auto), and two constants. The SQL of the panel, of the variable `node` and of the annotation names them.
fn doc() -> Value {
    let mut d: Value = serde_json::from_str(NODES).unwrap();
    d["variables"][0]["sql"] = json!([
        "SELECT DISTINCT node",
        "FROM analytics.nodes",
        "WHERE env = '${env}' AND span = '${bucket}' AND note = '${k}'"
    ]);
    let vars = d["variables"].as_array_mut().unwrap();
    vars.push(json!({
        "kind": "interval", "name": "bucket", "label": "Bucket", "hide": "none",
        "intervals": ["1m", "10m", "1h", "1d"], "auto": true, "auto_count": 30, "auto_min": "10s",
        "default": "$__auto"
    }));
    vars.push(json!({
        "kind": "interval", "name": "fixed", "label": null, "hide": "none",
        "intervals": ["5m", "1w"], "auto": false, "auto_count": 30, "auto_min": "10s", "default": null
    }));
    vars.push(json!({"kind": "constant", "name": "k", "label": "A note", "hide": "variable", "value": CONST}));
    vars.push(json!({"kind": "constant", "name": "plain", "label": null, "hide": "variable", "value": "eu-1"}));
    let sql = d["sections"][0]["panels"][0]["data"]["query"]["sql"].as_array_mut().unwrap();
    let at = sql.iter().position(|l| l.as_str().unwrap().contains("$__timeFilter")).unwrap();
    sql.insert(
        at + 1,
        json!("  AND span = '${bucket}' AND f = '${fixed}' AND note = '${k}' AND zone = '${plain}' AND `${k}` = 1"),
    );
    d["annotations"] = json!([{
        "name": "Deploys", "source": "prod", "color": "red",
        "sql": ["SELECT deployed_at AS time, title, text, tags", "FROM ops.deploys",
                "WHERE env = '${env}' AND span = '${bucket}' AND note = '${k}'"]
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

fn fake_ch() -> impl std::future::Future<Output = Fake> {
    Fake::start(healthy(|rec| ok(rec, FRAME_BODY)))
}

fn vars(bucket: &str, fixed: &str) -> Value {
    json!({
        "node": {"all": false, "values": ["a1"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
        "bucket": {"all": false, "values": [bucket]},
        "fixed": {"all": false, "values": [fixed]},
    })
}

async fn etag(h: &Harness) -> String {
    etag_of(&get(h.addr, "/api/dashboards/nodes").await)
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Answer {
    route: &'static str,
    status: u16,
    code: String,
    sql: Option<String>,
}

/// Calls the five forms of the four routes with the same values. The source records each query.
async fn four_routes(h: &Harness, fake: &Fake, d: &Value, values: Value, len_ms: i64) -> Vec<Answer> {
    let e = etag(h).await;
    let to = 1_794_211_200_000_i64 + len_ms;
    let range = json!({"etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": to});
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
    let panel = draft_panel("SELECT toDateTime(1) AS t, '${bucket}' AS b, '${k}' AS n WHERE $__timeFilter(t)");
    let mut body = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    body["to_ms"] = json!(to);
    body["panel"] = panel;
    body["dashboard_variables"] = d["variables"].clone();
    body["variables"] = values.clone();
    let r = post_preview(h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    add("6.22 panel", r, fake);
    let mut body = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    body["to_ms"] = json!(to);
    body.as_object_mut().unwrap().remove("panel");
    body.as_object_mut().unwrap().remove("max_data_points");
    body["variable"] = json!("node");
    body["dashboard_variables"] = d["variables"].clone();
    body["variables"] = values;
    let r = post_preview(h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    add("6.22 variable", r, fake);
    out
}

const SIX_HOURS: i64 = 21_600_000;

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

// ---- A103: the save and the read -----------------------------------------------------------------

#[tokio::test]
async fn a103_a_document_with_the_two_kinds_is_saved_and_read_back_with_the_same_canonical_bytes() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &doc()).await;
    let r = put_doc(&h, &d, "").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let got = get(h.addr, "/api/dashboards/nodes").await;
    assert_eq!(got.status, 200);
    let first = got.text();
    // The answer is an envelope. The member `document` holds the canonical bytes. The member `names` follows it.
    let start = first.find("\"document\":").unwrap() + "\"document\":".len();
    let document = first[start..first.rfind(",\"names\":").unwrap()].trim();
    let canonical = hune_core::document::validate(document.as_bytes(), None).unwrap();
    // The envelope holds the canonical bytes without the last line end.
    let bytes = String::from_utf8(canonical.bytes().to_vec()).unwrap();
    assert_eq!(bytes.trim_end(), document, "the stored bytes are the canonical bytes");
    assert_eq!(etag_of(&got), canonical.etag().to_string());
    // A second save of the read-back bytes gives the same etag and the same bytes.
    let r = put_doc(&h, &serde_json::from_str(document).unwrap(), "").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let again = get(h.addr, "/api/dashboards/nodes").await;
    assert_eq!(again.text(), first);
    assert_eq!(etag_of(&again), etag_of(&got));
    // The members are in the document that the page reads.
    let j: Value = serde_json::from_str(document).unwrap();
    let kinds: Vec<&str> = j["variables"].as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    assert_eq!(&kinds[3..], ["interval", "interval", "constant", "constant"]);
}

// ---- A104: 20 invalid variables ------------------------------------------------------------------

#[tokio::test]
async fn a104_twenty_invalid_variables_give_422_with_the_code_and_the_pointer() {
    let fake = fake_ch().await;
    let h = start(&fake, &doc()).await;
    type Edit = Box<dyn Fn(&mut Value)>;
    let item = |v: &'static str| -> Edit { Box::new(move |d| d["variables"][3]["intervals"][1] = json!(v)) };
    let set = |i: usize, k: &'static str, v: Value| -> Edit { Box::new(move |d| d["variables"][i][k] = v.clone()) };
    let cases: Vec<(Edit, &str, &str)> = vec![
        (item("1M"), "/variables/3/intervals/1", "bad_format"),
        (item("1.5h"), "/variables/3/intervals/1", "bad_format"),
        (item("0m"), "/variables/3/intervals/1", "bad_format"),
        (item("-1m"), "/variables/3/intervals/1", "bad_format"),
        (item("1 m"), "/variables/3/intervals/1", "bad_format"),
        (item(""), "/variables/3/intervals/1", "bad_format"),
        (item("366d"), "/variables/3/intervals/1", "out_of_range"),
        (set(3, "intervals", json!([])), "/variables/3/intervals", "out_of_range"),
        (set(3, "intervals", json!(["1m", "1m"])), "/variables/3/intervals/1", "not_unique"),
        (
            set(3, "intervals", json!((1..=51).map(|i| format!("{i}m")).collect::<Vec<_>>())),
            "/variables/3/intervals",
            "too_large",
        ),
        (set(3, "auto_count", json!(0)), "/variables/3/auto_count", "out_of_range"),
        (set(3, "auto_count", json!(1001)), "/variables/3/auto_count", "out_of_range"),
        (set(3, "auto_min", json!("90s")), "/variables/3/auto_min", "bad_format"),
        (set(3, "default", json!("7m")), "/variables/3/default", "not_permitted"),
        (set(4, "default", json!("$__auto")), "/variables/4/default", "not_permitted"),
        (set(5, "hide", json!("none")), "/variables/5/hide", "not_permitted"),
        (set(5, "value", json!("a".repeat(1025))), "/variables/5/value", "out_of_range"),
        (set(5, "value", json!("a\u{7}b")), "/variables/5/value", "bad_format"),
        (set(5, "default", json!("x")), "/variables/5/default", "unknown_member"),
        (set(3, "multi", json!(false)), "/variables/3/multi", "unknown_member"),
    ];
    assert_eq!(cases.len(), 20);
    for (edit, pointer, code) in cases {
        let mut d = doc();
        edit(&mut d);
        let r = put_doc(&h, &d, "?validate_only=true").await;
        assert_eq!((r.status, r.code().as_str()), (422, "invalid_document"), "{pointer}: {}", r.text());
        assert_eq!(errors_of(&r), vec![(pointer.to_owned(), code.to_owned())], "{pointer}");
    }
    // The document itself is valid.
    let r = put_doc(&h, &doc(), "?validate_only=true").await;
    assert_eq!(r.status, 200, "{}", r.text());
}

// ---- A105, C33: the values of an interval ----------------------------------------------------------

#[tokio::test]
async fn a105_an_interval_item_runs_and_any_other_value_is_refused_before_a_source_query() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    for item in ["1m", "10m", "1h", "1d"] {
        let answers = four_routes(&h, &fake, &d, vars(item, "5m"), SIX_HOURS).await;
        for a in &answers {
            assert_eq!(a.status, 200, "{item}: {a:?}");
            let sql = a.sql.as_deref().unwrap();
            assert!(sql.contains(&format!("'{item}'")), "{item}: {}: {sql}", a.route);
        }
    }
    let probes = fake.queries().len();
    // `7m` is not an item, an item with a blank, the case variants, the item of the other variable, `$__auto`
    // for the variable with `auto` false, an empty text and a hostile text.
    let mut hostile: Vec<String> =
        ["7m", "1m ", " 1m", "1M", "1H", "1D", "5m", "1w", "", "$__AUTO", "$__auto_interval_bucket"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
    hostile.extend(
        [
            "1m'; DROP TABLE x; --",
            "1m\" OR \"1\"=\"1",
            "1m`",
            "1m\\",
            "1m\u{0}",
            "$bucket",
            "${bucket}",
            "$__from",
            "1m,10m",
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    for i in 0..22 {
        hostile.push(format!("{}m{}", i + 100, "'".repeat(i % 3)));
    }
    assert!(hostile.len() >= 40, "{}", hostile.len());
    for bad in &hostile {
        let answers = four_routes(&h, &fake, &d, vars(bad, "5m"), SIX_HOURS).await;
        for a in &answers {
            assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{bad:?}: {a:?}");
            assert!(a.sql.is_none());
        }
    }
    // `$__auto` for a variable that has `auto` false.
    let answers = four_routes(&h, &fake, &d, vars("1m", "$__auto"), SIX_HOURS).await;
    for a in &answers {
        assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{a:?}");
    }
    // Two values, no value and "All".
    for (all, values) in [(false, json!(["1m", "10m"])), (false, json!([])), (true, json!([]))] {
        let mut v = vars("1m", "5m");
        v["bucket"] = json!({"all": all, "values": values});
        let answers = four_routes(&h, &fake, &d, v, SIX_HOURS).await;
        for a in &answers {
            assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{all} {a:?}");
        }
    }
    assert_eq!(fake.queries().len(), probes, "a query reached the source after the first good requests");
    // The refusal names the variable and never repeats the value.
    let e = etag(&h).await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": vars("7m'evil", "5m")}))).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert_eq!(r.json()["error"]["variable"], "bucket");
    assert!(!r.text().contains("evil"), "{}", r.text());
}

#[tokio::test]
async fn a_request_without_the_member_of_an_interval_is_a_bad_request() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let mut v = vars("1m", "5m");
    v.as_object_mut().unwrap().remove("bucket");
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": v}))).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
}

// ---- A106, C34: a constant -------------------------------------------------------------------------

#[tokio::test]
async fn a106_a_constant_takes_the_document_value_and_a_different_member_is_refused() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    // No member: the value of the document, in each context.
    let answers = four_routes(&h, &fake, &d, vars("1m", "5m"), SIX_HOURS).await;
    for a in &answers {
        assert_eq!(a.status, 200, "{a:?}");
        let sql = a.sql.as_deref().unwrap();
        // The string escapes of 7.8: the quote, the backslash and the text stay in one literal.
        assert!(sql.contains("'a\\'b\"c$d\\\\e;--x'"), "{}: {sql}", a.route);
    }
    assert!(answers[0].sql.as_deref().unwrap().contains("zone = 'eu-1'"), "{:?}", answers[0]);
    // A quoted name gets the name escapes.
    assert!(answers[0].sql.as_deref().unwrap().contains("`a'b\"c$d\\\\e;--x` = 1"), "{:?}", answers[0]);
    // The equal member gives the same SQL.
    let mut with = vars("1m", "5m");
    with["k"] = json!({"all": false, "values": [CONST]});
    let same = four_routes(&h, &fake, &d, with, SIX_HOURS).await;
    assert_eq!(
        same.iter().map(|a| a.sql.clone()).collect::<Vec<_>>(),
        answers.iter().map(|a| a.sql.clone()).collect::<Vec<_>>()
    );
    // A different member, "All", two values and no value: refused, and no source query runs.
    let probes = fake.queries().len();
    for member in [
        json!({"all": false, "values": ["zzz"]}),
        json!({"all": false, "values": [""]}),
        json!({"all": false, "values": []}),
        json!({"all": false, "values": [CONST, CONST]}),
        json!({"all": true, "values": []}),
        json!({"all": true, "values": [CONST]}),
    ] {
        let mut v = vars("1m", "5m");
        v["k"] = member.clone();
        let answers = four_routes(&h, &fake, &d, v, SIX_HOURS).await;
        for a in &answers {
            assert_eq!((a.status, a.code.as_str()), (422, "variable_value_refused"), "{member}: {a:?}");
        }
    }
    assert_eq!(fake.queries().len(), probes);
    let e = etag(&h).await;
    let mut v = vars("1m", "5m");
    v["k"] = json!({"all": false, "values": ["zzz"]});
    let r = post_panel(&h, 1, &panel_body(&e, json!({"variables": v}))).await;
    assert_eq!(r.json()["error"]["variable"], "k");
    assert_eq!(r.json()["error"]["reason"], "A constant takes its value from the document.");
}

// ---- A107: the text of "Auto" ------------------------------------------------------------------------

#[tokio::test]
async fn a107_the_text_of_auto_follows_the_range_and_is_the_same_for_all_the_queries() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    for (len, text) in [(SIX_HOURS, "10m"), (300_000, "10s"), (30 * 86_400_000, "1d")] {
        let answers = four_routes(&h, &fake, &d, vars("$__auto", "5m"), len).await;
        for a in &answers {
            assert_eq!(a.status, 200, "{len}: {a:?}");
            let sql = a.sql.as_deref().unwrap();
            assert!(
                sql.contains(&format!("span = '{text}'")) || sql.contains(&format!("'{text}' AS b")),
                "{len} {}: {sql}",
                a.route
            );
            assert!(!sql.contains("$__auto"), "{sql}");
        }
    }
}

// ---- A108, A109, A110: the other routes -------------------------------------------------------------

#[tokio::test]
async fn a108_the_options_route_keeps_the_error_of_the_interval_macros() {
    let mut d = doc();
    d["variables"][0]["sql"] = json!(["SELECT '${bucket}', '${k}', $__interval_s"]);
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let e = etag(&h).await;
    let body = json!({"etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "variables": vars("1m", "5m")});
    let r = call(
        h.addr,
        "POST",
        "/api/dashboards/nodes/variables/node/options",
        &[JSON],
        &serde_json::to_vec(&body).unwrap(),
    )
    .await;
    // A variable query has no interval (7.18, item 5): the same error as before this feature.
    assert_eq!((r.status, r.code().as_str()), (422, "sql_invalid"), "{}", r.text());
    assert!(fake.queries().is_empty());
}

#[tokio::test]
async fn a110_the_preview_reads_a_constant_from_dashboard_variables_and_not_from_variables() {
    let d = doc();
    let fake = fake_ch().await;
    let h = start(&fake, &d).await;
    let mut body = serde_json::from_slice::<Value>(&preview_body(json!({}))).unwrap();
    body["panel"] =
        draft_panel("SELECT '${bucket}' AS b, '${k}' AS n, '${plain}' AS p, toDateTime(1) AS t WHERE $__timeFilter(t)");
    body["dashboard_variables"] = d["variables"].clone();
    body["variables"] = json!({
        "node": {"all": false, "values": ["a1"]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
        "bucket": {"all": false, "values": ["$__auto"]},
        "fixed": {"all": false, "values": ["5m"]},
    });
    let r = post_preview(&h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sql = fake.queries().last().unwrap().sql();
    assert!(
        sql.contains("'10m' AS b") && sql.contains("'a\\'b\"c$d\\\\e;--x' AS n") && sql.contains("'eu-1' AS p"),
        "{sql}"
    );
    // A draft constant is read from `dashboard_variables`: a changed draft gives a changed SQL.
    body["dashboard_variables"][5]["value"] = json!("changed");
    let r = post_preview(&h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(fake.queries().last().unwrap().sql().contains("'changed' AS n"));
    // A member in `variables` that differs from the draft is refused.
    body["variables"]["k"] = json!({"all": false, "values": [CONST]});
    let probes = fake.queries().len();
    let r = post_preview(&h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    assert_eq!((r.status, r.code().as_str()), (422, "variable_value_refused"), "{}", r.text());
    assert_eq!(fake.queries().len(), probes);
    // A draft variable of the new kinds is not a variable that has options: the route refuses it.
    body["variables"].as_object_mut().unwrap().remove("k");
    body.as_object_mut().unwrap().remove("panel");
    body.as_object_mut().unwrap().remove("max_data_points");
    body["variable"] = json!("bucket");
    let r = post_preview(&h, "prod", &serde_json::to_vec(&body).unwrap()).await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"), "{}", r.text());
}
