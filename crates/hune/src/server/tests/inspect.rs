// SPDX-License-Identifier: MIT OR Apache-2.0
//! The expanded SQL in the answer of a panel query (`server.query_inspect`, F22).
//!
//! The rules under test: the key `false` never shows the SQL; the key `true` shows the SQL that the
//! source got and nothing else; a configuration secret never shows; in the mode `authzen` only a
//! subject that has `source.query` and `dashboard.write` gets the SQL.

use super::api::{JSON, fill};
use super::authzen_pdp::*;
use super::fake_ch::{FRAME_BODY, Fake, error, healthy};
use super::harness::*;
use super::query::{draft_panel, etag, panel_body, post_panel, post_preview, preview_body, queries};
use crate::config::Config;
use serde_json::{Value, json};
use std::time::Duration;

/// A value of a variable that looks like a secret. It is user data, so it can show in the SQL.
const LOOKS_SECRET: &str = "sk_live_4eC39HqLyjWDarjtT1zdp7dc";

fn inspect(on: bool) -> impl FnOnce(&mut Config) {
    move |c| c.server.query_inspect = on
}

fn secret_variables() -> Value {
    json!({"variables": {
        "node": {"all": false, "values": [LOOKS_SECRET]},
        "account": {"all": false, "values": [""]},
        "env": {"all": false, "values": ["prod"]},
    }})
}

/// Each string of the configuration that must never be in an answer or a log line.
fn config_secrets(c: &Config) -> Vec<String> {
    let mut out: Vec<String> = c.clickhouse.iter().map(|b| b.password.expose().to_owned()).collect();
    if let crate::config::AccessConfig::Authzen(a) = &c.access {
        out.push(a.pdp.token.expose().to_owned());
    }
    out.retain(|s| !s.is_empty());
    out
}

fn assert_no_secret(h: &Harness, texts: &[String]) {
    let secrets = config_secrets(&h.state().config);
    assert!(!secrets.is_empty());
    for secret in &secrets {
        for t in texts {
            assert!(!t.contains(secret.as_str()), "a secret shows in: {t}");
        }
        for line in h.logger.memory_lines() {
            assert!(!line.contains(secret.as_str()), "a secret shows in a log line: {line}");
        }
    }
}

// ---- The open mode ---------------------------------------------------------------------------------

#[tokio::test]
async fn the_default_gives_no_sql_and_the_session_says_false() {
    // The configuration of the tests has the default value.
    assert!(!test_config(std::path::Path::new("/nonexistent")).server.query_inspect);
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, |_| {}).await;
    assert_eq!(get(h.addr, "/api/session").await.json()["query_inspect"], false);
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("sql").is_none());
    assert!(!r.text().contains("toStartOfInterval"));
}

#[tokio::test]
async fn a_false_key_never_shows_the_sql_even_in_a_preview() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, inspect(false)).await;
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, secret_variables())).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("sql").is_none());
    assert!(!r.text().contains(LOOKS_SECRET));
    let r = post_preview(&h, "prod", &preview_body(json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("sql").is_none());
    // An error answer has no SQL either.
    assert_no_secret(&h, &[r.text()]);
}

#[tokio::test]
async fn a_true_key_gives_the_sql_that_the_source_got_and_the_session_says_true() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, inspect(true)).await;
    assert_eq!(get(h.addr, "/api/session").await.json()["query_inspect"], true);
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let j = r.json();
    let sent = fake.queries()[0].sql();
    assert_eq!(j["sql"], sent.as_str(), "the member is the SQL text that ClickHouse got");
    assert!(sent.contains("node IN ('a1','b2')") && !sent.contains("$__"), "{sent}");
    // The other members stay: the frame is intact.
    assert_eq!(j["rows"], 2);
    assert_eq!(j["etag"], e);
    assert_eq!(j["interval_ms"], 60_000);
    assert_eq!(j["values"][0], json!([1794211200, 1794211260]));
    // `sql` is the last member and the answer is one JSON object.
    assert!(r.text().trim_end().ends_with('}'));
    // The log lines hold the hash, not the SQL.
    for line in h.logger.memory_lines() {
        assert!(!line.contains("toStartOfInterval") && !line.contains("node IN"), "{line}");
    }
    // The preview gives the SQL of the draft.
    let r = post_preview(&h, "prod", &preview_body(json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sent = fake.queries().last().unwrap().sql();
    assert_eq!(r.json()["sql"], sent.as_str());
    assert!(sent.contains("toDateTime(1)"), "{sent}");
}

#[tokio::test]
async fn the_sql_holds_a_variable_value_that_looks_like_a_secret_and_never_a_configuration_secret() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, inspect(true)).await;
    let e = etag(&h, "nodes").await;
    let mut answers = Vec::new();
    // Panel 1 with the value that looks like a secret: it is data of the request, so it shows.
    let r = post_panel(&h, 1, &panel_body(&e, secret_variables())).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let sql = r.json()["sql"].as_str().unwrap().to_owned();
    assert!(sql.contains(LOOKS_SECRET), "{sql}");
    answers.push(r.text());
    // Panel 2 and the preview.
    let r = post_panel(&h, 2, &panel_body(&e, json!({"max_data_points": 100}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    answers.push(r.text());
    let r = post_preview(&h, "prod", &preview_body(json!({}))).await;
    answers.push(r.text());
    // The password of the source, the user, the settings and the headers are not in an answer.
    assert_eq!(fake.queries()[0].header("x-clickhouse-key"), Some("secret-password"));
    for text in &answers {
        assert!(!text.contains("secret-password"), "{text}");
        assert!(!text.to_lowercase().contains("x-clickhouse"), "{text}");
        assert!(!text.contains("max_result_rows") && !text.contains("log_comment"), "{text}");
        assert!(!text.contains(&fake.addr.to_string()), "{text}");
    }
    assert_no_secret(&h, &answers);
    // The value is data of the request, never a log item.
    for line in h.logger.memory_lines() {
        assert!(!line.contains(LOOKS_SECRET), "{line}");
    }
}

#[tokio::test]
async fn an_answer_that_is_an_error_has_no_sql() {
    let fake = Fake::start(healthy(|rec| error(rec, 500, 62, "Syntax error near secret-password"))).await;
    let h = start_with_fake(&fake, fill, inspect(true)).await;
    let e = etag(&h, "nodes").await;
    let r = post_panel(&h, 1, &panel_body(&e, secret_variables())).await;
    assert_ne!(r.status, 200);
    assert!(r.json().get("sql").is_none() && !r.text().contains("toStartOfInterval"), "{}", r.text());
}

#[tokio::test]
async fn the_json_of_the_sql_member_escapes_the_text() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let h = start_with_fake(&fake, fill, inspect(true)).await;
    let e = etag(&h, "nodes").await;
    // A value with a quote, a backslash and a double quote stays one valid JSON string.
    let value = "a'b\\c\"e";
    let body = panel_body(
        &e,
        json!({"variables": {
            "node": {"all": false, "values": [value]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
        }}),
    );
    let r = post_panel(&h, 1, &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["sql"], fake.queries()[0].sql().as_str());
}

// ---- The mode authzen --------------------------------------------------------------------------------

fn grants(list: Vec<(&'static str, &'static str)>) -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    move |q| {
        let qp = q.short();
        for (action, g) in &list {
            if *action == q.action && (g.is_empty() || qp == *g || qp.starts_with(&format!("{g}/"))) {
                return Ans::yes();
            }
        }
        Ans::No(None)
    }
}

async fn authzen_start(pdp: &Pdp, fake: &Fake, on: bool) -> Harness {
    start_with_fake(fake, fill, |c| {
        authzen(c, &pdp.url(), Duration::ZERO);
        c.server.query_inspect = on;
    })
    .await
}

async fn authzen_post(h: &Harness, path: &str, body: &[u8]) -> Resp {
    let t = token("user:alice");
    call(h.addr, "POST", path, &[(HEADER, t.as_str()), JSON], body).await
}

async fn authzen_etag(h: &Harness) -> String {
    let t = token("user:alice");
    let r = call(h.addr, "GET", "/api/dashboards/nodes", &[(HEADER, t.as_str())], b"").await;
    super::api::etag_of(&r)
}

#[tokio::test]
async fn authzen_the_sql_needs_source_query_and_dashboard_write_besides_the_read() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])).await;
    let h = authzen_start(&pdp, &fake, true).await;
    let e = authzen_etag(&h).await;
    let path = "/api/dashboards/nodes/panels/1/query";
    let body = panel_body(&e, secret_variables());
    let mut texts = Vec::new();

    // A subject with the three rights gets the SQL.
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["sql"], fake.queries()[0].sql().as_str());
    texts.push(r.text());

    // A viewer: the read only. The data comes, the SQL does not.
    pdp.set_rules(grants(vec![("dashboard.read", "")]));
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("sql").is_none(), "{}", r.text());
    assert!(!r.text().contains(LOOKS_SECRET) && !r.text().contains("toStartOfInterval"));
    assert_eq!(r.json()["rows"], 2);
    texts.push(r.text());

    // The read and `source.query` without `dashboard.write`: no SQL.
    pdp.set_rules(grants(vec![("dashboard.read", ""), ("source.query", "")]));
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200);
    assert!(r.json().get("sql").is_none());

    // The read and `dashboard.write` without `source.query`: no SQL.
    pdp.set_rules(grants(vec![("dashboard.read", ""), ("dashboard.write", "")]));
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200);
    assert!(r.json().get("sql").is_none());

    // The grants are on other nodes: `source.query` on the other source and `dashboard.write` on another
    // dashboard do not give the SQL.
    pdp.set_rules(grants(vec![
        ("dashboard.read", ""),
        ("dashboard.write", "folder:ops"),
        ("source.query", "source:conf"),
    ]));
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200);
    assert!(r.json().get("sql").is_none());

    // A refusal with a reason: the SQL is left out, the reason does not show, and the answer stays valid.
    pdp.set_rules(|q| if q.action == "dashboard.read" { Ans::yes() } else { Ans::No(Some("reason of the PDP")) });
    let r = authzen_post(&h, path, &body).await;
    assert_eq!(r.status, 200);
    assert!(r.json().get("sql").is_none());
    assert!(!r.text().contains("reason of the PDP"));

    assert_no_secret(&h, &texts);
    assert!(texts.iter().all(|t| !t.contains(PDP_TOKEN)));
}

#[tokio::test]
async fn authzen_with_the_key_false_asks_no_extra_question_and_gives_no_sql() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])).await;
    let h = authzen_start(&pdp, &fake, false).await;
    let e = authzen_etag(&h).await;
    pdp.clear();
    let r = authzen_post(&h, "/api/dashboards/nodes/panels/1/query", &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert!(r.json().get("sql").is_none());
    let asked: Vec<String> = pdp.asked().iter().map(|q| q.action.clone()).collect();
    assert_eq!(asked, vec!["dashboard.read".to_owned()], "only the use question");
}

#[tokio::test]
async fn authzen_the_extra_question_has_the_two_permissions_of_the_preview() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])).await;
    let h = authzen_start(&pdp, &fake, true).await;
    let e = authzen_etag(&h).await;
    pdp.clear();
    let r = authzen_post(&h, "/api/dashboards/nodes/panels/1/query", &panel_body(&e, json!({}))).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let mut asked: Vec<(String, String)> = pdp.asked().iter().map(|q| (q.action.clone(), q.short())).collect();
    asked.sort();
    assert_eq!(
        asked,
        vec![
            ("dashboard.read".to_owned(), "folder:transit/dashboard:nodes".to_owned()),
            ("dashboard.write".to_owned(), "folder:transit/dashboard:nodes".to_owned()),
            ("source.query".to_owned(), "source:prod".to_owned()),
        ]
    );
}

#[tokio::test]
async fn authzen_a_preview_gives_the_sql_to_the_subject_that_may_preview() {
    let fake = Fake::start(queries(FRAME_BODY)).await;
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])).await;
    let h = authzen_start(&pdp, &fake, true).await;
    let body = {
        let mut v: Value = serde_json::from_slice(&preview_body(json!({}))).unwrap();
        v["panel"] = draft_panel("SELECT toDateTime(1) AS t, 1 AS n WHERE $__timeFilter(t)");
        serde_json::to_vec(&v).unwrap()
    };
    let r = authzen_post(&h, "/api/sources/prod/preview", &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["sql"], fake.queries().last().unwrap().sql().as_str());
    // Without the right to preview, the route refuses (403): no SQL and no data.
    pdp.set_rules(grants(vec![("dashboard.read", "")]));
    let r = authzen_post(&h, "/api/sources/prod/preview", &body).await;
    assert_eq!(r.status, 403);
    assert!(!r.text().contains("toDateTime"));
}

// ---- A secret that comes from `_file` or `_env` ---------------------------------------------------

/// Loads the example file `local.hcl` with `query_inspect = true` and the password of the source
/// from a file or from a variable: the secret goes through the real loader of the configuration.
fn loaded_password(form: &str) -> (Config, String) {
    use crate::config::{CapturedEnvironment, load};
    let dir = tempfile::tempdir().unwrap();
    let marker = format!("pw-marker-{form}-7Hq2");
    let pw_file = dir.path().join("pw");
    std::fs::write(&pw_file, format!("{marker}\n")).unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/config/ca-a.pem"), &ca).unwrap();
    let volume = dir.path().join("volume");
    std::fs::create_dir_all(&volume).unwrap();
    let mut text = include_str!("../../../../../tests/fixtures/config/local.hcl")
        .replace("/run/secrets/clickhouse/ca.pem", &ca.display().to_string())
        .replace("/var/lib/hune/dashboards", &std::fs::canonicalize(&volume).unwrap().display().to_string())
        .replace(
            "  editing                 = true\n",
            "  editing                 = true\n  query_inspect           = true\n",
        );
    let env: Vec<(String, String)> = if form == "env" {
        text = text.replace(
            "password_file      = \"/run/secrets/clickhouse/password\"",
            "password_env       = \"HUNE_TEST_CH_PASSWORD\"",
        );
        vec![("HUNE_TEST_CH_PASSWORD".to_owned(), marker.clone())]
    } else {
        text = text.replace("/run/secrets/clickhouse/password", &pw_file.display().to_string());
        Vec::new()
    };
    assert!(text.contains("query_inspect           = true"));
    let file = dir.path().join("hune.hcl");
    std::fs::write(&file, text).unwrap();
    let loaded = load(&file, &CapturedEnvironment::from_pairs(env)).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(loaded.config.server.query_inspect);
    assert_eq!(loaded.config.clickhouse[0].password.expose(), marker);
    (loaded.config, marker)
}

#[tokio::test]
async fn a_password_from_a_file_or_a_variable_never_shows_in_an_answer_or_a_log_line() {
    for form in ["file", "env"] {
        let (loaded, marker) = loaded_password(form);
        let fake = Fake::start(queries(FRAME_BODY)).await;
        let h = start_with_fake(&fake, fill, |c| {
            c.server.query_inspect = loaded.server.query_inspect;
            c.clickhouse[0].password = loaded.clickhouse[0].password.clone();
        })
        .await;
        let e = etag(&h, "nodes").await;
        let mut answers = Vec::new();
        for panel in [1, 2] {
            let r = post_panel(&h, panel, &panel_body(&e, secret_variables())).await;
            assert_eq!(r.status, 200, "{}", r.text());
            assert!(r.json()["sql"].is_string());
            answers.push(r.text());
        }
        answers.push(post_preview(&h, "prod", &preview_body(json!({}))).await.text());
        // ClickHouse got the password in a header; the answers and the logs never have it.
        assert_eq!(fake.queries()[0].header("x-clickhouse-key"), Some(marker.as_str()));
        for text in &answers {
            assert!(!text.contains(&marker), "{form}: {text}");
        }
        for line in h.logger.memory_lines() {
            assert!(!line.contains(&marker), "{form}: {line}");
        }
    }
}
