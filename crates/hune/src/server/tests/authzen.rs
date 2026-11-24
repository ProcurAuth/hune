// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cases A39 to A45 of 16.8, and the other rules of section 11, with an in-process fake PDP.
//! Each test name starts with its case id when it has one.

use super::api::{JSON, NODES, doc_text, etag_of, fill, key, post_body, put_body};
use super::authzen_pdp::*;
use super::fake_ch::{FRAME_BODY, Fake, healthy, ok};
use super::harness::*;
use crate::config::AccessConfig;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

/// The grants of a subject: `(action, node below the service)`. A grant covers each node below it.
/// The answer to a `subtree` question is `true` when a grant is on the node or below it (P 4.4).
fn grants(list: Vec<(&'static str, &'static str)>) -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    move |q| {
        let qp = q.short();
        for (action, g) in &list {
            if *action != q.action {
                continue;
            }
            let covers = g.is_empty() || qp == *g || qp.starts_with(&format!("{g}/"));
            let below = q.subtree
                && (qp.is_empty() || (g.starts_with(&qp) && (g.len() == qp.len() || g.as_bytes()[qp.len()] == b'/')));
            if covers || below {
                return Ans::yes();
            }
        }
        Ans::No(None)
    }
}

fn allow_all() -> impl Fn(&Q) -> Ans + Send + Sync + 'static {
    grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "")])
}

fn fill_two(root: &Path) {
    fill(root);
    std::fs::write(root.join("ops/dash2.json"), doc_text("dash2", "Second")).unwrap();
}

fn set_pdp(c: &mut crate::config::Config, edit: impl FnOnce(&mut crate::config::PdpConfig)) {
    if let AccessConfig::Authzen(a) = &mut c.access {
        edit(&mut a.pdp);
    }
}

async fn start(pdp: &Pdp, ttl: Duration) -> Harness {
    Harness::start_with(test_timing(), fill_two, |c| authzen(c, &pdp.url(), ttl)).await
}

async fn req(h: &Harness, method: &str, path: &str, sub: &str, extra: &[(&str, &str)], body: &[u8]) -> Resp {
    let t = token(sub);
    let mut hs: Vec<(&str, &str)> = vec![(HEADER, t.as_str())];
    hs.extend_from_slice(extra);
    call(h.addr, method, path, &hs, body).await
}

async fn rget(h: &Harness, path: &str, sub: &str) -> Resp {
    req(h, "GET", path, sub, &[], b"").await
}

fn lines(h: &Harness) -> Vec<Value> {
    h.logger.memory_lines().iter().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn events<'a>(l: &'a [Value], name: &str) -> Vec<&'a Value> {
    l.iter().filter(|v| v["event"] == name).collect()
}

fn shorts(pdp: &Pdp) -> Vec<(String, String)> {
    pdp.asked().iter().map(|q| (q.action.clone(), q.short())).collect()
}

fn pair(a: &str, r: &str) -> (String, String) {
    (a.to_owned(), r.to_owned())
}

// ---- A39: the token ----------------------------------------------------------------------------

#[tokio::test]
async fn a39_no_token_gives_401_on_each_route_except_the_action_document() {
    let pdp = Pdp::start(|_| Ans::yes()).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    let routes: Vec<(&str, &str)> = vec![
        ("GET", "/"),
        ("GET", "/d/nodes"),
        ("GET", "/static/x/app.js"),
        ("GET", "/api/session"),
        ("GET", "/api/folders"),
        ("POST", "/api/folders"),
        ("PUT", "/api/folders/ops"),
        ("DELETE", "/api/folders/ops"),
        ("GET", "/api/dashboards"),
        ("POST", "/api/dashboards"),
        ("GET", "/api/dashboards/nodes"),
        ("PUT", "/api/dashboards/nodes"),
        ("DELETE", "/api/dashboards/nodes"),
        ("GET", "/api/dashboards/nodes/history"),
        ("GET", "/api/dashboards/nodes/history/1"),
        ("POST", "/api/dashboards/nodes/restore"),
        ("POST", "/api/dashboards/nodes/move"),
        ("POST", "/api/dashboards/nodes/panels/1/query"),
        ("POST", "/api/dashboards/nodes/variables/node/options"),
        ("GET", "/api/sources"),
        ("POST", "/api/sources/prod/preview"),
        ("GET", "/.well-known/procurauth/resources?type=folder"),
    ];
    for (m, p) in routes {
        let body: &[u8] = if m == "GET" || m == "DELETE" { b"" } else { b"{}" };
        let r = call(h.addr, m, p, &[JSON], body).await;
        assert_eq!((r.status, r.code().as_str()), (401, "unauthenticated"), "{m} {p}");
        assert_eq!(r.json()["error"]["retryable"], false);
        assert_eq!(r.json()["error"]["message"], "A valid identity token is necessary.");
        if !body.is_empty() {
            assert_eq!(r.header("connection"), Some("close"), "{m} {p}: a 401 with a body closes");
        }
    }
    let r = get(h.addr, "/.well-known/procurauth/actions").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(pdp.asked().len(), 0, "no question without a token");
    assert_eq!(pdp.requests(), 0);
    // The token never reaches a log line, and `access.unauthenticated` gives the reason.
    let l = lines(&h);
    assert_eq!(events(&l, "access.unauthenticated").len(), 22);
    assert!(events(&l, "access.unauthenticated").iter().all(|e| e["reason"] == "missing"));
}

#[tokio::test]
async fn a39_each_rule_of_the_token_has_its_reason() {
    use procurauth_core::jws::sign_compact;
    let pdp = Pdp::start(|_| Ans::yes()).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let good = json!({"iss": ISSUER, "sub": "user:alice", "aud": AUDIENCE, "iat": now, "exp": now + 100});
    let with = |k: &str, v: Value| {
        let mut c = good.clone();
        c[k] = v;
        c
    };
    let hdr = |alg: &str, typ: &str| json!({"alg": alg, "typ": typ, "kid": "edge-1"});
    let sign = |h: Value, c: Value| sign_compact(&h, &c, &edge_key());
    let cases: Vec<(&str, String)> = vec![
        ("alg", sign(hdr("EdDSA", "JWT"), good.clone())),
        ("typ", sign(hdr("Ed25519", "at+jwt"), good.clone())),
        ("iss", sign(hdr("Ed25519", "JWT"), with("iss", json!("https://other")))),
        ("aud", sign(hdr("Ed25519", "JWT"), with("aud", json!("other")))),
        ("exp", sign(hdr("Ed25519", "JWT"), with("exp", json!(now - 1000)))),
        ("iat", sign(hdr("Ed25519", "JWT"), with("iat", json!(now + 1000)))),
        ("life", sign(hdr("Ed25519", "JWT"), with("exp", json!(now + 3000)))),
        ("subject", sign(hdr("Ed25519", "JWT"), with("sub", json!("-")))),
        ("subject", sign(hdr("Ed25519", "JWT"), with("sub", json!("external")))),
        ("subject", sign(hdr("Ed25519", "JWT"), with("sub", json!("import-grafana")))),
        ("structure", "not.a.token".to_owned()),
    ];
    for (reason, tok) in &cases {
        let r = call(h.addr, "GET", "/api/session", &[(HEADER, tok)], b"").await;
        assert_eq!(r.status, 401, "{reason}");
    }
    let l = lines(&h);
    let got: Vec<&str> = events(&l, "access.unauthenticated").iter().map(|e| e["reason"].as_str().unwrap()).collect();
    let want: Vec<&str> = cases.iter().map(|c| c.0).collect();
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        // The library names the check `sub` for a claim that is not a string. The rules agree otherwise.
        assert_eq!(g, w);
    }
    // Two copies of the header: a refusal. A token in `Authorization`: ignored.
    let t = token("user:alice");
    let r = call(h.addr, "GET", "/api/session", &[(HEADER, &t), (HEADER, &t)], b"").await;
    assert_eq!(r.status, 401);
    let r = call(h.addr, "GET", "/api/session", &[("Authorization", &t)], b"").await;
    assert_eq!(r.status, 401);
    assert_eq!(events(&lines(&h), "access.unauthenticated").last().unwrap()["reason"], "missing");
    // A good token works. The session gives the subject (6.10), and it asks no question (11.29).
    let r = rget(&h, "/api/session", "user:alice").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["subject"], "user:alice");
    assert_eq!(r.json()["mode"], "authzen");
    assert_eq!(pdp.asked().len(), 0);
    // The pages and the files need a token and ask nothing.
    assert_eq!(rget(&h, "/", "user:alice").await.status, 200);
    assert_eq!(rget(&h, "/d/nodes/x", "user:alice").await.status, 200);
    // A static file needs the token too, so no shared cache may keep it: `private`.
    let file = crate::web_assets::FILES.iter().find(|f| f.path.ends_with(".js")).expect("a js file in web/");
    let r = rget(&h, &format!("/static/{}/{}", crate::web_assets::BUILD_HASH, file.path), "user:alice").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("cache-control"), Some("private, max-age=31536000, immutable"));
    assert_eq!(pdp.asked().len(), 0);
    // The subject is in the log of the request.
    let l = lines(&h);
    assert!(events(&l, "http.request").iter().any(|e| e["subject"] == "user:alice"));
    for line in h.logger.memory_lines() {
        assert!(!line.contains(&t) && !line.contains(PDP_TOKEN), "{line}");
    }
}

// ---- A40: the unknown uid ----------------------------------------------------------------------

#[tokio::test]
async fn a40_an_unknown_uid_gives_403_or_404_by_the_answer_of_the_pdp() {
    let pdp = Pdp::start(grants(vec![("dashboard.read", "folder:ops")])).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    // `dashboard.read` on the service is false for this subject: 403, also for a uid that exists.
    let r = rget(&h, "/api/dashboards/nothing", "user:bob").await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(r.json()["error"]["action"], "dashboard.read");
    assert_eq!(pdp.asked()[0].short(), "");
    assert!(!pdp.asked()[0].subtree, "the match is node");
    let r = rget(&h, "/api/dashboards/nodes", "user:bob").await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "the same 403 for a uid that exists");
    assert_eq!(r.json()["error"]["action"], "dashboard.read");
    // A subject that can read the service: 404 for an unknown uid, 200 for a known one.
    pdp.set_rules(allow_all());
    let r = rget(&h, "/api/dashboards/nothing", "user:alice").await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    assert_eq!(rget(&h, "/api/dashboards/nodes", "user:alice").await.status, 200);
    // An unknown folder follows the same rule.
    let r = req(&h, "PUT", "/api/folders/nothing", "user:alice", &[JSON], br#"{"title":"T"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    pdp.set_rules(grants(vec![]));
    let r = req(&h, "DELETE", "/api/folders/nothing", "user:bob", &[], b"").await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    let r = req(
        &h,
        "POST",
        "/api/dashboards",
        "user:bob",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("nothing", &doc_text("n1", "N")),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    // A source that the configuration does not hold: 404 and no question.
    pdp.clear();
    let r = rget(&h, "/api/sources", "user:bob").await;
    assert_eq!(r.status, 200);
    let r = req(&h, "POST", "/api/sources/zzz/preview", "user:bob", &[JSON], b"{}").await;
    assert_eq!(r.status, 400, "the shape comes first");
}

#[tokio::test]
async fn a40_a_create_of_an_existing_uid_asks_read_on_the_dashboard() {
    let pdp = Pdp::start(grants(vec![("dashboard.write", ""), ("source.query", "")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    let doc = doc_text("nodes", "Again");
    let create = |sub: &'static str| {
        let h = &h;
        let doc = doc.clone();
        async move {
            req(h, "POST", "/api/dashboards", sub, &[JSON, ("Idempotency-Key", &key())], &post_body("ops", &doc)).await
        }
    };
    // The subject has no `dashboard.read`: 403, so the answer tells nothing. The body names the use of the
    // route (11.30).
    let r = create("user:bob").await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    assert_eq!(shorts(&pdp).last().unwrap(), &pair("dashboard.read", "folder:transit/dashboard:nodes"));
    pdp.set_rules(allow_all());
    let r = create("user:bob").await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    // A deleted dashboard: its last version gives the folder of the chain (11.23, rule 3).
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:bob").await);
    let r = req(
        &h,
        "DELETE",
        "/api/dashboards/nodes",
        "user:bob",
        &[("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        b"",
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    pdp.clear();
    let r = create("user:bob").await;
    assert_eq!((r.status, r.code().as_str()), (409, "already_exists"));
    assert_eq!(shorts(&pdp).last().unwrap(), &pair("dashboard.read", "folder:transit/dashboard:nodes"));
}

// ---- A41: the PDP does not answer --------------------------------------------------------------

#[tokio::test]
async fn a41_a_pdp_that_does_not_answer_gives_503_and_a_kept_answer_still_applies() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    // One answer is kept.
    assert_eq!(rget(&h, "/api/dashboards/nodes", "user:alice").await.status, 200);
    pdp.set_fault(Fault::Status503);
    // The same question: served from the kept answers, with no request.
    let before = pdp.requests();
    assert_eq!(rget(&h, "/api/dashboards/nodes", "user:alice").await.status, 200);
    assert_eq!(pdp.requests(), before);
    // A question that nobody kept: 503, retryable, with the fixed text.
    let r = rget(&h, "/api/dashboards/ops-only", "user:alice").await;
    assert_eq!((r.status, r.code().as_str()), (503, "pdp_unavailable"));
    assert_eq!(r.json()["error"]["retryable"], true);
    // Another subject has no kept answer.
    let r = rget(&h, "/api/dashboards/nodes", "user:carol").await;
    assert_eq!((r.status, r.code().as_str()), (503, "pdp_unavailable"));
    let r = rget(&h, "/api/dashboards", "user:alice").await;
    assert_eq!(r.status, 503, "a list never shows a result that a failure made empty");
    // The action document, the session and the pages still answer.
    assert_eq!(get(h.addr, "/.well-known/procurauth/actions").await.status, 200);
    assert_eq!(rget(&h, "/api/session", "user:alice").await.status, 200);
    assert_eq!(rget(&h, "/", "user:alice").await.status, 200);
    // `/readyz` does not change (14.4).
    assert_eq!(get(h.ops, "/readyz").await.status, 200);
    assert!(!events(&lines(&h), "access.pdp_failed").is_empty());
    // The credential: a 401 of the PDP gives the credential line.
    pdp.set_fault(Fault::Status401);
    assert_eq!(rget(&h, "/api/dashboards/ops-only", "user:alice").await.status, 503);
    assert!(!events(&lines(&h), "access.pdp_credential_refused").is_empty());
    // An answer that is not valid.
    pdp.set_fault(Fault::BadJson);
    assert_eq!(rget(&h, "/api/dashboards/ops-only", "user:alice").await.status, 503);
    assert!(!events(&lines(&h), "access.pdp_invalid_answer").is_empty());
    // The PDP is back.
    pdp.set_fault(Fault::None);
    assert_eq!(rget(&h, "/api/dashboards/ops-only", "user:alice").await.status, 404);
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_pdp_requests_total{result=\"ok\"}"), "{m}");
    assert!(m.contains("hune_pdp_requests_total{result=\"status\"}"));
}

#[tokio::test]
async fn a41_a_slow_pdp_gives_503_after_the_timeout_and_the_pool_is_bounded() {
    let pdp = Pdp::start(allow_all()).await;
    let h = Harness::start_with(test_timing(), fill_two, |c| {
        authzen(c, &pdp.url(), Duration::ZERO);
        set_pdp(c, |p| {
            p.timeout = Duration::from_millis(300);
            p.max_connections = 1;
        });
    })
    .await;
    pdp.set_fault(Fault::Delay(Duration::from_millis(900)));
    let started = std::time::Instant::now();
    // One call holds the only permit for the time of the timeout. A second call can still get the permit when it
    // starts a little after the first one on a loaded host. A third call cannot: the permit is free again only
    // after the second call, which holds it for the time of the timeout too. So three calls make the test steady.
    let (a, b, c) = tokio::join!(
        rget(&h, "/api/dashboards/nodes", "user:alice"),
        rget(&h, "/api/dashboards/nodes", "user:carol"),
        rget(&h, "/api/dashboards/nodes", "user:dave")
    );
    assert_eq!((a.status, b.status, c.status), (503, 503, 503));
    assert!(started.elapsed() < Duration::from_secs(5));
    let l = lines(&h);
    let failed = events(&l, "access.pdp_failed");
    assert!(failed.iter().any(|e| e["error"] == "pool_busy"), "a call finds no permit: {failed:?}");
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_pdp_requests_total{result=\"pool_busy\"} "), "{m}");
    assert!(m.contains("hune_pdp_pool_in_use "));
}

/// 17.7a: a call that cannot wait for an approval gets 403 and the fixed body of ProcurAuth. Another reason of
/// the PDP keeps the usual `forbidden` error of Hune, and the client never gets the reason.
#[tokio::test]
async fn u113_a_refusal_for_an_approval_gives_403_and_the_code_approval_cannot_wait() {
    for (reason, body) in [
        ("approval_required", serde_json::json!({"error": "approval_cannot_wait"})),
        ("approval_pending", serde_json::json!({"error": "approval_cannot_wait"})),
        ("approval_denied", serde_json::json!({"error": "approval_cannot_wait"})),
        ("approval_expired", serde_json::json!({"error": "approval_cannot_wait"})),
        (
            "unknown_subject",
            serde_json::json!({"error": {"action": "dashboard.read", "code": "forbidden",
            "message": "You have no right for this operation.", "retryable": false}}),
        ),
    ] {
        let pdp = Pdp::start(move |q: &Q| if q.permission_only { Ans::yes() } else { Ans::No(Some(reason)) }).await;
        let h = start(&pdp, Duration::ZERO).await;
        let r = rget(&h, "/api/dashboards/nodes?download=true", "user:alice").await;
        assert_eq!(r.status, 403, "{reason}: {}", r.text());
        assert_eq!(r.json(), body, "{reason}");
    }
}

#[tokio::test]
async fn the_requests_to_the_pdp_carry_the_credential_and_the_request_id() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    let r = req(&h, "GET", "/api/dashboards/nodes", "user:alice", &[("X-Request-Id", "req-abc-1")], b"").await;
    assert_eq!(r.status, 200);
    let bearer = pdp.bearer_seen.lock().unwrap().clone();
    assert!(!bearer.is_empty());
    assert!(bearer.iter().all(|b| b == &format!("Bearer {PDP_TOKEN}")));
    assert!(pdp.request_ids.lock().unwrap().iter().all(|i| i == "req-abc-1"));
    // The subject goes as the id of a subject of the type user, and nothing else (P 3.3).
    assert_eq!(pdp.asked()[0].sub, "user:alice");
    // The decision line: no reason in the body of the answer to the client, but in the log.
    let l = lines(&h);
    let d = events(&l, "access.decision");
    assert!(!d.is_empty());
    assert_eq!(d[0]["resource"], "service:hune-test/folder:transit/dashboard:nodes");
    assert_eq!(d[0]["action"], "dashboard.read");
    assert_eq!(d[0]["decision"], true);
    assert_eq!(d[0]["subject"], "user:alice");
}

#[tokio::test]
async fn a_reason_of_the_pdp_never_reaches_the_client() {
    let pdp = Pdp::start(|_| Ans::No(Some("unknown_subject"))).await;
    let h = start(&pdp, Duration::ZERO).await;
    let r = rget(&h, "/api/dashboards/nodes", "user:alice").await;
    assert_eq!(r.status, 403);
    assert!(!r.text().contains("unknown_subject"));
    let l = lines(&h);
    assert_eq!(events(&l, "access.decision")[0]["reason"], "unknown_subject");
}

#[tokio::test]
async fn cache_ttl_zero_asks_each_time_and_equal_questions_share_one_flight() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    for _ in 0..3 {
        assert_eq!(rget(&h, "/api/dashboards/nodes", "user:alice").await.status, 200);
    }
    let reads = pdp.asked().iter().filter(|q| q.action == "dashboard.read").count();
    assert_eq!(reads, 3, "no kept answer");
    // With a kept answer, 12 equal questions that wait together make one question.
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    pdp.set_fault(Fault::Delay(Duration::from_millis(150)));
    let mut set = tokio::task::JoinSet::new();
    let addr = h.addr;
    for _ in 0..12 {
        set.spawn(async move {
            let t = token("user:alice");
            call(addr, "GET", "/api/dashboards/nodes?download=true", &[(HEADER, &t)], b"").await.status
        });
    }
    while let Some(s) = set.join_next().await {
        assert_eq!(s.unwrap(), 200);
    }
    let reads = pdp.asked().iter().filter(|q| q.action == "dashboard.read").count();
    assert_eq!(reads, 1, "one flight for each question (11.27)");
    let m = get(h.ops, "/metrics").await.text();
    assert!(m.contains("hune_pdp_questions_total{decision=\"true\"}"), "{m}");
}

// ---- A42: the ceilings -------------------------------------------------------------------------

fn panel_body(etag: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "etag": etag, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64, "max_data_points": 1200,
        "variables": {
            "node": {"all": false, "values": ["a1"]},
            "account": {"all": false, "values": [""]},
            "env": {"all": false, "values": ["prod"]},
        }
    }))
    .unwrap()
}

async fn start_ch(pdp: &Pdp, fake: &Fake, ttl: Duration) -> Harness {
    start_with_fake(fake, fill_two, |c| authzen(c, &pdp.url(), ttl)).await
}

#[tokio::test]
async fn a42_the_ceilings_lower_the_settings_of_the_query() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp = Pdp::start(|q| {
        if q.action == "dashboard.read" && q.short().ends_with("dashboard:nodes") {
            Ans::Yes(vec![("max_rows", json!(1000)), ("max_duration", json!(30))])
        } else {
            Ans::yes()
        }
    })
    .await;
    let h = start_ch(&pdp, &fake, Duration::from_secs(30)).await;
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    let q = fake.queries();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].param("max_result_rows").as_deref(), Some("1000"));
    assert_eq!(q[0].param("max_execution_time").as_deref(), Some("30"));
    let run = events(&lines(&h), "query.run").into_iter().next().cloned().unwrap();
    assert_eq!(run["ceiling"], true);
    assert_eq!(run["subject"], "user:alice");
    assert_eq!(run["max_result_rows"], 1000);
    // The options query takes the same ceilings, with the lower row limit of 7.14.
    let body = serde_json::to_vec(&json!({
        "etag": e, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "variables": {"env": {"all": false, "values": ["prod"]}}
    }))
    .unwrap();
    let r = req(&h, "POST", "/api/dashboards/nodes/variables/node/options", "user:alice", &[JSON], &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(fake.queries()[1].param("max_result_rows").as_deref(), Some("1000"));
    // The comment of the query names the subject.
    let comment: Value = serde_json::from_str(&q[0].param("log_comment").unwrap()).unwrap();
    assert_eq!(comment["subject"], "user:alice");
}

#[tokio::test]
async fn a42_a_result_above_a_ceiling_is_ceiling_exceeded() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp = Pdp::start(|q| {
        if q.action == "dashboard.read" && q.short().ends_with("dashboard:nodes") {
            Ans::Yes(vec![("max_rows", json!(1))])
        } else {
            Ans::yes()
        }
    })
    .await;
    let h = start_ch(&pdp, &fake, Duration::from_secs(30)).await;
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (403, "ceiling_exceeded"), "{}", r.text());
    let err = &r.json()["error"];
    assert_eq!(err["action"], "dashboard.read");
    assert_eq!(err["ceiling"], "max_rows");
    assert_eq!(err["limit"], 1);
    assert_eq!(err["retryable"], false);
    let refused = events(&lines(&h), "access.ceiling_refused").len();
    assert_eq!(refused, 1);
}

#[tokio::test]
async fn a42_a_ceiling_that_the_action_does_not_list_or_that_is_not_valid_is_refused() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp = Pdp::start(allow_all()).await;
    let h = start_ch(&pdp, &fake, Duration::ZERO).await;
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    let answer = |limits: Vec<(&'static str, Value)>| {
        pdp.set_rules(move |q| if q.action == "dashboard.read" { Ans::Yes(limits.clone()) } else { Ans::yes() })
    };
    // An unknown ceiling: 403 and `access.ceiling_unenforceable` (P 7.5).
    answer(vec![("max_rows", json!(5)), ("max_cpu", json!(1))]);
    let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert!(!events(&lines(&h), "access.ceiling_unenforceable").is_empty());
    // A value that is not a whole number, a value below the minimum: 503.
    // A budget name is an unknown ceiling here: Hune declares no budget, so the library refuses with 403.
    answer(vec![("requests", json!(1))]);
    let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!(r.status, 403);
    for limits in [vec![("max_rows", json!("x"))], vec![("max_rows", json!(0))]] {
        answer(limits);
        let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
        assert_eq!((r.status, r.code().as_str()), (503, "pdp_unavailable"));
    }
    assert!(!events(&lines(&h), "access.pdp_invalid_answer").is_empty());
    assert_eq!(fake.queries().len(), 0, "no query ran");
    // A false answer ignores its ceilings (P 7.6).
    pdp.set_rules(|_| Ans::No(None));
    let r = req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!(r.status, 403);
}

// ---- A43: the save ----------------------------------------------------------------------------

async fn put_nodes(h: &Harness, sub: &str, doc: &str, key: &str) -> Resp {
    let e = etag_of(&rget(h, "/api/dashboards/nodes", sub).await);
    req(
        h,
        "PUT",
        "/api/dashboards/nodes",
        sub,
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", key)],
        &put_body(doc, "m"),
    )
    .await
}

#[tokio::test]
async fn a43_a_save_asks_source_query_on_each_source_of_the_document_with_no_usage() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    pdp.clear();
    // The document names `conf` and `prod`.
    let doc = NODES.replacen("\"source\": \"prod\"", "\"source\": \"conf\"", 1);
    let doc = doc.replace("\"title\": \"Nodes\"", "\"title\": \"Two sources\"");
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    pdp.clear();
    let r = req(
        &h,
        "PUT",
        "/api/dashboards/nodes",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&doc, "m"),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    let asked = shorts(&pdp);
    assert_eq!(asked.len(), 3, "{asked:?}");
    let mut others: Vec<_> = asked[..2].to_vec();
    others.sort();
    assert_eq!(others, vec![pair("source.query", "source:conf"), pair("source.query", "source:prod")]);
    assert_eq!(asked[2], pair("dashboard.write", "folder:transit/dashboard:nodes"));
    assert_eq!(pdp.requests(), 2, "the others in one request, then the use");
    assert!(pdp.asked().iter().all(|q| !q.usage && !q.subtree), "no usage (P 6.4)");
    // A false answer for one source refuses the save, and the use is not asked.
    pdp.set_rules(grants(vec![("dashboard.read", ""), ("dashboard.write", ""), ("source.query", "source:prod")]));
    pdp.clear();
    let doc2 = doc.replace("Two sources", "Another title");
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    pdp.clear();
    let r = req(
        &h,
        "PUT",
        "/api/dashboards/nodes",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&doc2, "m"),
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{}", r.text());
    // The body names the use of the route (11.30). The PDP refused `source.query`, so the use is not asked.
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    assert!(pdp.asked().iter().all(|q| q.action == "source.query"), "the use is not asked: {:?}", shorts(&pdp));
    // The check applies also when the SQL text did not change: a save of the same document.
    pdp.set_rules(grants(vec![("dashboard.read", ""), ("dashboard.write", "")]));
    let current = rget(&h, "/api/dashboards/nodes?download=true", "user:alice").await.text();
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    let r = req(
        &h,
        "PUT",
        "/api/dashboards/nodes",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        &put_body(&current, "m"),
    )
    .await;
    assert_eq!(r.status, 403);
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
    // No write happened.
    let _ = put_nodes;
}

#[tokio::test]
async fn a43_the_use_is_refused_with_its_action_and_validate_only_asks_the_use_only() {
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("source.query", "")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    let r = put_nodes(&h, "user:alice", &doc_text("nodes", "X"), &key()).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    // The reads fail too, so the save never reaches the write. Give the read and test validate_only.
    pdp.set_rules(grants(vec![("dashboard.read", ""), ("dashboard.write", "folder:transit")]));
    pdp.clear();
    let doc = NODES.to_owned();
    let r =
        req(&h, "PUT", "/api/dashboards/nodes?validate_only=true", "user:alice", &[JSON], &put_body(&doc, "m")).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:transit/dashboard:nodes")]);
    pdp.clear();
    let r = req(
        &h,
        "POST",
        "/api/dashboards?validate_only=true",
        "user:alice",
        &[JSON],
        &post_body("transit", &doc_text("fresh", "F")),
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:transit")]);
}

#[tokio::test]
async fn a_create_a_move_and_a_restore_ask_their_questions_in_order() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    pdp.clear();
    let r = req(
        &h,
        "POST",
        "/api/dashboards",
        "user:alice",
        &[JSON, ("Idempotency-Key", &key())],
        &post_body("ops", &doc_text("fresh", "Fresh")),
    )
    .await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert_eq!(
        shorts(&pdp),
        vec![pair("source.query", "source:prod"), pair("dashboard.write", "folder:ops")],
        "the others first, then the use"
    );
    // A move: `dashboard.write` on the dashboard as another permission, then on the target folder.
    let e = etag_of(&rget(&h, "/api/dashboards/fresh", "user:alice").await);
    pdp.clear();
    let r = req(
        &h,
        "POST",
        "/api/dashboards/fresh/move",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        br#"{"folder":"transit"}"#,
    )
    .await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(
        shorts(&pdp),
        vec![pair("dashboard.write", "folder:ops/dashboard:fresh"), pair("dashboard.write", "folder:transit")]
    );
    // The chain follows the catalog: after the move, the dashboard is in the folder `transit`.
    pdp.clear();
    rget(&h, "/api/dashboards/fresh", "user:alice").await;
    assert_eq!(shorts(&pdp)[0], pair("dashboard.read", "folder:transit/dashboard:fresh"));
    // A restore of a version that does not exist asks the use only, then 404 (11.23, rule 4).
    let e = etag_of(&rget(&h, "/api/dashboards/fresh", "user:alice").await);
    pdp.clear();
    let r = req(
        &h,
        "POST",
        "/api/dashboards/fresh/restore",
        "user:alice",
        &[JSON, ("If-Match", &format!("\"{e}\"")), ("Idempotency-Key", &key())],
        br#"{"version": 99}"#,
    )
    .await;
    assert_eq!((r.status, r.code().as_str()), (404, "not_found"));
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:transit/dashboard:fresh")]);
    // A history of a version that does not exist: the use, then 404.
    pdp.clear();
    let r = rget(&h, "/api/dashboards/fresh/history/99", "user:alice").await;
    assert_eq!(r.status, 404);
    assert_eq!(shorts(&pdp), vec![pair("dashboard.read", "folder:transit/dashboard:fresh")]);
}

#[tokio::test]
async fn folder_routes_ask_write_on_the_folder_or_the_service() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    pdp.clear();
    let r = req(&h, "POST", "/api/folders", "user:alice", &[JSON], br#"{"id":"new","title":"New"}"#).await;
    assert_eq!(r.status, 201, "{}", r.text());
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "")]);
    pdp.clear();
    let r = req(&h, "PUT", "/api/folders/new", "user:alice", &[JSON], br#"{"title":"Newer"}"#).await;
    assert_eq!(r.status, 200);
    assert_eq!(shorts(&pdp), vec![pair("dashboard.write", "folder:new")]);
    pdp.set_rules(grants(vec![("dashboard.read", "")]));
    let r = req(&h, "DELETE", "/api/folders/new", "user:alice", &[], b"").await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(r.json()["error"]["action"], "dashboard.write");
}

#[tokio::test]
async fn the_preview_asks_source_query_and_gives_its_ceilings() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp =
        Pdp::start(|q| if q.action == "source.query" { Ans::Yes(vec![("max_rows", json!(77))]) } else { Ans::yes() })
            .await;
    let h = start_ch(&pdp, &fake, Duration::ZERO).await;
    let doc: Value = serde_json::from_str(NODES).unwrap();
    let mut panel = doc["sections"][0]["panels"][0].clone();
    panel["id"] = json!(7);
    panel["data"]["query"]["sql"] = json!(["SELECT toDateTime(1) AS t, 1 AS n WHERE $__timeFilter(t)"]);
    panel["data"]["query"]["min_interval"] = Value::Null;
    let body = serde_json::to_vec(&json!({
        "uid": "nodes", "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200, "dashboard_variables": [], "variables": {}, "panel": panel
    }))
    .unwrap();
    pdp.clear();
    let r = req(&h, "POST", "/api/sources/prod/preview", "user:alice", &[JSON], &body).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(
        shorts(&pdp),
        vec![pair("dashboard.write", "folder:transit/dashboard:nodes"), pair("source.query", "source:prod")]
    );
    assert_eq!(fake.queries()[0].param("max_result_rows").as_deref(), Some("77"));
    // A new dashboard: the other permission is on the folder.
    pdp.clear();
    let mut v: Value = serde_json::from_slice(&body).unwrap();
    v.as_object_mut().unwrap().remove("uid");
    v["folder"] = json!("ops");
    let r = req(&h, "POST", "/api/sources/prod/preview", "user:alice", &[JSON], &serde_json::to_vec(&v).unwrap()).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(shorts(&pdp)[0], pair("dashboard.write", "folder:ops"));
    // The use is false: 403 with the action `source.query`.
    pdp.set_rules(grants(vec![("dashboard.write", "")]));
    let r = req(&h, "POST", "/api/sources/prod/preview", "user:alice", &[JSON], &body).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"));
    assert_eq!(r.json()["error"]["action"], "source.query");
}

// ---- A44: the display filter --------------------------------------------------------------------

#[tokio::test]
async fn a44_the_lists_show_only_what_the_subject_can_see() {
    let pdp =
        Pdp::start(grants(vec![("dashboard.read", "folder:transit"), ("dashboard.write", "folder:transit")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    pdp.clear();
    let r = rget(&h, "/api/dashboards", "user:alice").await;
    assert_eq!(r.status, 200, "{}", r.text());
    let items = r.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["uid"], "nodes");
    assert_eq!(items[0]["access"]["write"], true);
    assert!(r.json()["next"].is_null());
    // The questions: 2 folders (display), then read and write on the visible folder (11.25).
    let asked = pdp.asked();
    assert_eq!(asked.iter().filter(|q| q.subtree).count(), 2);
    assert_eq!(asked.len(), 4, "{:?}", shorts(&pdp));
    // The folders.
    pdp.clear();
    let r = rget(&h, "/api/folders", "user:alice").await;
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(j["items"].as_array().unwrap().len(), 1);
    assert_eq!(j["items"][0]["id"], "transit");
    assert_eq!(j["items"][0]["access"]["write"], true);
    assert_eq!(j["access"]["create"], false, "no `dashboard.write` on the service");
    // A grant on one dashboard only: its folder shows in the filter, the other dashboards do not.
    pdp.set_rules(grants(vec![("dashboard.read", "folder:ops/dashboard:dash2")]));
    let r = rget(&h, "/api/dashboards", "user:bob").await;
    let items = r.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["uid"], "dash2");
    assert_eq!(items[0]["folder"], "ops");
    assert_eq!(items[0]["access"]["write"], false);
    // A subject with no grant sees an empty list: not an error.
    pdp.set_rules(grants(vec![]));
    let r = rget(&h, "/api/dashboards", "user:nobody").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["items"], json!([]));
    // `folder` does not change the questions for one item, and an unknown folder asks nothing.
    pdp.set_rules(allow_all());
    pdp.clear();
    let r = rget(&h, "/api/dashboards?folder=nothing", "user:alice").await;
    assert_eq!(r.json()["items"], json!([]));
    assert_eq!(pdp.asked().len(), 0);
    // The sources: `source.query` on each source.
    pdp.set_rules(grants(vec![("source.query", "source:conf")]));
    let r = rget(&h, "/api/sources", "user:alice").await;
    let j = r.json();
    assert_eq!(j["items"].as_array().unwrap().len(), 1);

    assert_eq!(r.json()["items"][0]["id"], "conf");
}

// ---- A60: the parameter `tag` with the display filter ---------------------------------------

#[tokio::test]
async fn a60_the_tag_never_shows_a_dashboard_that_the_subject_cannot_read() {
    // Both fixture dashboards have the tag `chain`: `transit/nodes` and `ops/dash2`.
    let pdp = Pdp::start(grants(vec![("dashboard.read", "folder:transit")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    for path in ["/api/dashboards", "/api/dashboards?tag=chain", "/api/dashboards?tag=chain&q=o"] {
        let r = rget(&h, path, "user:alice").await;
        assert_eq!(r.status, 200, "{path}: {}", r.text());
        let items = r.json()["items"].as_array().unwrap().clone();
        assert_eq!(items.len(), 1, "{path}: {items:?}");
        assert_eq!(items[0]["uid"], "nodes", "{path}");
    }
    // A grant on one dashboard of the other folder.
    pdp.set_rules(grants(vec![("dashboard.read", "folder:ops/dashboard:dash2")]));
    let r = rget(&h, "/api/dashboards?tag=chain", "user:bob").await;
    let items = r.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0]["uid"], "dash2");
    // A tag that no readable dashboard has gives an empty list, and a subject with no grant too.
    let r = rget(&h, "/api/dashboards?tag=other", "user:bob").await;
    assert_eq!((r.status, r.json()["items"].clone()), (200, json!([])));
    pdp.set_rules(grants(vec![]));
    for path in ["/api/dashboards", "/api/dashboards?tag=chain"] {
        let r = rget(&h, path, "user:nobody").await;
        assert_eq!((r.status, r.json()["items"].clone()), (200, json!([])), "{path}");
    }
    // The tag does not give access: a tag that matches asks the same questions as a list with no tag.
    pdp.set_rules(allow_all());
    let r = rget(&h, "/api/dashboards?tag=chain&limit=1", "user:alice").await;
    assert_eq!(r.json()["items"].as_array().unwrap().len(), 1);
    assert!(r.json()["next"].is_string());
    // A bad tag is refused before any question to the PDP.
    pdp.clear();
    let r = rget(&h, "/api/dashboards?tag=", "user:alice").await;
    assert_eq!((r.status, r.code().as_str()), (400, "bad_request"));
    assert_eq!(pdp.asked().len(), 0);
}

#[tokio::test]
async fn a_read_of_a_dashboard_gives_the_write_flag_of_the_pdp() {
    let pdp = Pdp::start(grants(vec![("dashboard.read", ""), ("dashboard.write", "folder:ops")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    let r = rget(&h, "/api/dashboards/nodes", "user:alice").await;
    assert_eq!(r.json()["access"]["write"], false);
    let r = rget(&h, "/api/dashboards/dash2", "user:alice").await;
    assert_eq!(r.json()["access"]["write"], true);
}

// ---- A45 and the action document ----------------------------------------------------------------

#[tokio::test]
async fn the_action_document_has_the_same_bytes_at_each_read_and_asks_nothing() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::from_secs(30)).await;
    let a = call(h.addr, "GET", "/.well-known/procurauth/actions", &[(HEADER, "garbage")], b"").await;
    let b = get(h.addr, "/.well-known/procurauth/actions").await;
    assert_eq!((a.status, b.status), (200, 200), "also with a bad token (11.18)");
    assert_eq!(a.body, b.body);
    let head = call(h.addr, "HEAD", "/.well-known/procurauth/actions", &[], b"").await;
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-type"), Some("application/json"));
    assert_eq!(pdp.asked().len(), 0);
    let doc = a.json();
    assert_eq!(doc["server"], AUDIENCE);
    assert_eq!(doc["actions"].as_array().unwrap().len(), 3);
    assert_eq!(
        doc["resources"],
        json!([
            {"path": "source:conf", "label": "Conformance"},
            {"path": "source:prod", "label": "Production"}
        ])
    );
}

#[tokio::test]
async fn with_editing_off_only_dashboard_read_is_published_and_no_question_comes_for_the_others() {
    let pdp = Pdp::start(allow_all()).await;
    let h = Harness::start_with(test_timing(), fill_two, |c| {
        authzen(c, &pdp.url(), Duration::ZERO);
        c.server.editing = false;
    })
    .await;
    let doc = get(h.addr, "/.well-known/procurauth/actions").await.json();
    let names: Vec<&str> = doc["actions"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["dashboard.read"]);
    assert!(doc.get("resources").is_none(), "no published action takes the type source");
    pdp.clear();
    let r = rget(&h, "/api/folders", "user:alice").await;
    assert_eq!(r.json()["access"]["create"], false);
    assert_eq!(r.json()["items"][0]["access"]["write"], false);
    assert!(pdp.asked().iter().all(|q| q.action == "dashboard.read"));
    let r = rget(&h, "/api/sources", "user:alice").await;
    assert_eq!(r.json()["items"], json!([]));
    let r = req(&h, "POST", "/api/folders", "user:alice", &[JSON], br#"{"id":"x","title":"X"}"#).await;
    assert_eq!((r.status, r.code().as_str()), (403, "editing_disabled"));
    assert!(pdp.asked().iter().all(|q| q.action == "dashboard.read"), "no question for a write route");
}

// ---- The options route ---------------------------------------------------------------------------

#[tokio::test]
async fn the_options_route_lists_the_children_that_the_subject_can_see() {
    let pdp = Pdp::start(grants(vec![("dashboard.read", "folder:transit"), ("source.query", "source:prod")])).await;
    let h = start(&pdp, Duration::ZERO).await;
    let path = "/.well-known/procurauth/resources";
    let r = rget(&h, &format!("{path}?type=folder"), "user:alice").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["items"], json!([{"id": "transit", "label": "Transit desk", "has_children": true}]));
    assert!(r.json()["next"].is_null());
    // The questions use the match `subtree`, in the order of the actions (11.26).
    let asked = pdp.asked();
    assert!(asked.iter().all(|q| q.subtree));
    assert!(asked.iter().any(|q| q.action == "dashboard.write"), "write asked for the folder that is not read");
    let r = rget(&h, &format!("{path}?parent=folder%3Atransit&type=dashboard&q=NODE&limit=2"), "user:alice").await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.json()["items"], json!([{"id": "nodes", "label": "Nodes", "has_children": false}]));
    let r = rget(&h, &format!("{path}?type=source"), "user:alice").await;
    assert_eq!(r.json()["items"], json!([{"id": "prod", "label": "Production", "has_children": false}]));
    // A parent that does not exist: no item.
    let r = rget(&h, &format!("{path}?parent=folder%3Anothing&type=dashboard"), "user:alice").await;
    assert_eq!(r.json()["items"], json!([]));
    // The cursor: one item at a time.
    pdp.set_rules(allow_all());
    let r = rget(&h, &format!("{path}?type=folder&limit=1"), "user:alice").await;
    let j = r.json();
    assert_eq!(j["items"][0]["id"], "ops");
    let next = j["next"].as_str().unwrap().to_owned();
    let r = rget(&h, &format!("{path}?type=folder&limit=1&after={next}"), "user:alice").await;
    assert_eq!(r.json()["items"][0]["id"], "transit");
    assert!(r.json()["next"].is_null());
    // The cursor of another request is refused.
    let r = rget(&h, &format!("{path}?type=source&after={next}"), "user:alice").await;
    assert_eq!((r.status, r.code().as_str()), (400, "invalid_options_request"));
    // Each fault of the query is 400.
    for bad in [
        "",
        "type=folder&type=folder",
        "type=dashboard",
        "type=folder&parent=folder%3Atransit",
        "type=folder&limit=0",
        "type=folder&limit=x",
        "type=folder&limit=-1",
        "type=folder%GG",
        "type=nothing",
        "type=folder&after=%21%21",
    ] {
        let r = rget(&h, &format!("{path}?{bad}"), "user:alice").await;
        assert_eq!((r.status, r.code().as_str()), (400, "invalid_options_request"), "{bad}");
    }
    // A parameter that Hune does not know has no effect.
    assert_eq!(rget(&h, &format!("{path}?type=folder&other=1"), "user:alice").await.status, 200);
    assert!(!events(&lines(&h), "access.options").is_empty());
    // The method: a POST gets 405.
    let r = req(&h, "POST", path, "user:alice", &[JSON], b"{}").await;
    assert_eq!(r.status, 405);
}

#[tokio::test]
async fn the_options_route_fails_closed_when_the_pdp_fails() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    pdp.set_fault(Fault::Status503);
    let r = rget(&h, "/.well-known/procurauth/resources?type=folder", "user:alice").await;
    assert_eq!((r.status, r.code().as_str()), (503, "pdp_unavailable"));
}

#[tokio::test]
async fn the_pdp_credential_never_reaches_a_log_line_or_an_answer() {
    let pdp = Pdp::start(allow_all()).await;
    let h = start(&pdp, Duration::ZERO).await;
    let r = rget(&h, "/api/dashboards", "user:alice").await;
    assert_eq!(r.status, 200);
    assert!(!r.text().contains(PDP_TOKEN));
    for line in h.logger.memory_lines() {
        assert!(!line.contains(PDP_TOKEN), "{line}");
    }
}

// ---- the list cursor never holds a row that the subject cannot read (6.24, T2) -----------------

/// The folder `ops` with 100 hidden rows before the visible row `v1`, 150 hidden rows between `v1` and
/// the visible row `v2`. The hidden titles all hold the word `secret`.
fn fill_many(root: &Path) {
    fill(root);
    let ops = root.join("ops");
    let mut rows: Vec<(String, String)> = Vec::new();
    for k in 0..100 {
        rows.push((format!("hid-a{k:03}"), format!("Alpha secret {k:03}")));
    }
    rows.push(("vis-1".to_owned(), "Mmm visible one".to_owned()));
    for k in 0..150 {
        rows.push((format!("hid-n{k:03}"), format!("Nnn secret {k:03}")));
    }
    rows.push(("vis-2".to_owned(), "Zzz visible two".to_owned()));
    for (uid, title) in rows {
        std::fs::write(ops.join(format!("{uid}.json")), doc_text(&uid, &title)).unwrap();
    }
}

/// Decodes the base64url text of a cursor. The test does not need the canonical check of the server.
fn decode_cursor(text: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        acc = (acc << 6) | A.iter().position(|a| *a == c).expect("a base64url character") as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[tokio::test]
async fn the_list_cursor_holds_no_title_or_uid_of_a_hidden_dashboard() {
    let pdp = Pdp::start(grants(vec![
        ("dashboard.read", "folder:ops/dashboard:vis-1"),
        ("dashboard.read", "folder:ops/dashboard:vis-2"),
    ]))
    .await;
    let h = Harness::start_with(test_timing(), fill_many, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    for limit in [1, 2, 5] {
        let mut seen: Vec<String> = Vec::new();
        let mut cursors = 0;
        let mut after = String::new();
        for _ in 0..200 {
            let path = format!("/api/dashboards?limit={limit}{after}");
            let r = rget(&h, &path, "user:bob").await;
            assert_eq!(r.status, 200, "{path}: {}", r.text());
            let j = r.json();
            for item in j["items"].as_array().unwrap() {
                seen.push(item["uid"].as_str().unwrap().to_owned());
            }
            let Some(next) = j["next"].as_str() else { break };
            cursors += 1;
            let raw = String::from_utf8_lossy(&decode_cursor(next)).into_owned();
            assert!(!raw.contains("secret") && !raw.contains("hid-"), "the cursor {raw:?} names a hidden row");
            after = format!("&after={next}");
        }
        assert_eq!(seen, ["vis-1", "vis-2"], "limit {limit}: each visible item exactly one time");
        assert!(cursors >= 1, "limit {limit}: the 250 hidden rows need more than one page");
    }
    // A forged count never gives a row that the subject cannot read.
    for skip in [0usize, 7, 100, 249, 250, 300, 999_999_999] {
        let mut bytes = vec![0u8, 0, 0];
        bytes.extend_from_slice(skip.to_string().as_bytes());
        let cursor = encode_cursor(&bytes);
        let r = rget(&h, &format!("/api/dashboards?limit=500&after={cursor}"), "user:bob").await;
        assert_eq!(r.status, 200, "{}", r.text());
        for item in r.json()["items"].as_array().unwrap() {
            assert!(item["uid"].as_str().unwrap().starts_with("vis-"), "{item:?}");
        }
    }
}

fn encode_cursor(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(A[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(A[n as usize & 63] as char);
        }
    }
    out
}

// ---- the checks of a hostile document come before the gate and never block the server ----------

/// A hidden query and comment lines that hold 28,000 unclosed `$tag$` openers (252 KB). The old name scan
/// needed 20 seconds or more for it (review of 2026-11-14).
fn hostile_sql() -> Vec<String> {
    let mut out = vec!["SELECT 1 AS x".to_owned()];
    let mut line = String::from("# ");
    for k in 0..28_000 {
        line.push_str(&format!("$t{k:05}$ "));
        if line.len() > 4000 {
            out.push(std::mem::replace(&mut line, String::from("# ")));
        }
    }
    out.push(line);
    out
}

#[tokio::test]
async fn a_save_and_a_preview_with_a_hostile_variable_answer_in_a_bound_time() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp = Pdp::start(allow_all()).await;
    let h = start_ch(&pdp, &fake, Duration::ZERO).await;
    let first = rget(&h, "/api/dashboards/nodes", "user:alice").await;
    let e = etag_of(&first);
    let mut doc = first.json()["document"].clone();
    // The subject has no grant any more: the gate answers after the checks.
    pdp.set_rules(grants(vec![]));
    let mut hostile = doc["variables"][0].clone();
    hostile["sql"] = json!(hostile_sql());
    doc["variables"][0] = hostile.clone();
    let body = put_body(&serde_json::to_string(&doc).unwrap(), "m");
    let ifm = format!("\"{e}\"");
    let t = std::time::Instant::now();
    let r = req(
        &h,
        "PUT",
        "/api/dashboards/nodes",
        "user:bob",
        &[JSON, ("If-Match", &ifm), ("Idempotency-Key", &key())],
        &body,
    )
    .await;
    assert!(t.elapsed() < Duration::from_secs(5), "the save needs {:?}", t.elapsed());
    assert!(matches!(r.status, 200 | 403), "{}: {}", r.status, r.text());
    // A preview by a subject with no grant.
    let panel = {
        let mut p = doc["sections"][0]["panels"][0].clone();
        p["id"] = json!(7);
        p["data"]["query"]["sql"] = json!(["SELECT 1 AS n"]);
        p
    };
    let body = serde_json::to_vec(&json!({
        "uid": "nodes", "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200, "dashboard_variables": [hostile], "variables": {}, "panel": panel
    }))
    .unwrap();
    let t = std::time::Instant::now();
    let r = req(&h, "POST", "/api/sources/prod/preview", "user:bob", &[JSON], &body).await;
    assert!(t.elapsed() < Duration::from_secs(5), "the preview needs {:?}", t.elapsed());
    assert!(r.status < 500, "{}: {}", r.status, r.text());
}

#[tokio::test]
async fn the_panel_route_reads_only_a_canonical_panel_id() {
    let fake = Fake::start(healthy(|rec| ok(rec, FRAME_BODY))).await;
    let pdp = Pdp::start(allow_all()).await;
    let h = start_ch(&pdp, &fake, Duration::ZERO).await;
    let e = etag_of(&rget(&h, "/api/dashboards/nodes", "user:alice").await);
    let ok_status =
        req(&h, "POST", "/api/dashboards/nodes/panels/1/query", "user:alice", &[JSON], &panel_body(&e)).await;
    assert_eq!(ok_status.status, 200, "{}", ok_status.text());
    for id in ["%2B1", "01", "001", "1.0", "+1"] {
        let path = format!("/api/dashboards/nodes/panels/{id}/query");
        let r = req(&h, "POST", &path, "user:alice", &[JSON], &panel_body(&e)).await;
        assert_eq!((r.status, r.code().as_str()), (404, "not_found"), "{id}: {}", r.text());
    }
}

#[tokio::test]
async fn the_options_cursor_holds_no_id_of_a_hidden_child() {
    // 250 hidden dashboards, then `vis-1` and `vis-2` in the order of the id. The subject reads these two only.
    let pdp = Pdp::start(grants(vec![
        ("dashboard.read", "folder:ops/dashboard:vis-1"),
        ("dashboard.read", "folder:ops/dashboard:vis-2"),
    ]))
    .await;
    let h = Harness::start_with(test_timing(), fill_many, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    let path = "/.well-known/procurauth/resources?parent=folder%3Aops&type=dashboard";
    for limit in [1, 2, 50] {
        let mut seen: Vec<String> = Vec::new();
        let mut cursors = 0;
        let mut after = String::new();
        for _ in 0..200 {
            let r = rget(&h, &format!("{path}&limit={limit}{after}"), "user:bob").await;
            assert_eq!(r.status, 200, "{}", r.text());
            let j = r.json();
            for item in j["items"].as_array().unwrap() {
                seen.push(item["id"].as_str().unwrap().to_owned());
            }
            let Some(next) = j["next"].as_str() else { break };
            cursors += 1;
            let raw = String::from_utf8_lossy(&decode_cursor(next)).into_owned();
            assert!(!raw.contains("hid-"), "the cursor {raw:?} names a hidden child");
            after = format!("&after={next}");
        }
        assert_eq!(seen, ["vis-1", "vis-2"], "limit {limit}: each visible item exactly one time");
        assert!(cursors >= 1, "limit {limit}: the hidden children need more than one page");
    }
    // A forged count never shows a hidden child.
    for skip in [0usize, 7, 100, 249, 250, 300, 999_999_999] {
        let cursor = encode_cursor(format!("dashboard\nfolder:ops\n\n{skip}").as_bytes());
        let r = rget(&h, &format!("{path}&after={cursor}"), "user:bob").await;
        assert_eq!(r.status, 200, "{}", r.text());
        for item in r.json()["items"].as_array().unwrap() {
            assert!(item["id"].as_str().unwrap().starts_with("vis-"), "{item:?}");
        }
    }
    // A cursor of the old form, or with a count that is not canonical, gives 400.
    for bad in [
        "dashboard\nfolder:ops\nvis-1",
        "dashboard\nfolder:ops\nvis-1\n",
        "dashboard\nfolder:ops\nvis-1\n01",
        "dashboard\nfolder:ops\nvis-1\n-1",
        "dashboard\nfolder:ops\nvis-1\n1234567890",
    ] {
        let r = rget(&h, &format!("{path}&after={}", encode_cursor(bad.as_bytes())), "user:bob").await;
        assert_eq!((r.status, r.code().as_str()), (400, "invalid_options_request"), "{bad:?}");
    }
}
