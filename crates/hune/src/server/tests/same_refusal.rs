//! Decision ID-K321, point 2 (owner answer of 2026-11-21): a `403` does not show whether a uid or a folder exists.
//!
//! For each route that names a uid or a folder, a subject without the right sends the same request for a uid (or a
//! folder) that exists and for one that does not exist. The two answers are equal byte for byte: the status, each
//! header and the body. Neither request reads the store or validates a document, and a request of the save and of
//! the create runs the pre-pass in both cases (11.20, 11.23, 11.30). Before the fix, the member `action` was
//! `dashboard.read` for an unknown uid and `dashboard.write` (or `source.query`) for a uid that exists.

use super::api::{JSON, ZERO_ETAG, doc_text, fill, post_body, put_body};
use super::authzen_pdp::*;
use super::harness::*;
use crate::server::handlers::prepass::tests_hook;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::PoisonError;
use std::sync::atomic::Ordering;
use std::time::Duration;

const KNOWN: &str = "oracle-k";
const UNKNOWN: &str = "oracle-u";
const ETAG: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// The same request id for both requests, so that the headers can be equal.
const RID: &str = "k321-probe";
const KEY: &str = "0194b3c6-6f1e-7d2a-9b4c-00000000a321";
/// The tests that send the pairs hold this lock: the record of the pre-pass is for the whole process, and the
/// pairs of each test have the same uids.
pub(super) static PAIRS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) fn fill_known(root: &Path) {
    fill(root);
    std::fs::write(root.join(format!("transit/{KNOWN}.json")), doc_text(KNOWN, "Known")).unwrap();
}

/// A subject with no grant, and a subject with `source.query` only: both have no right on the dashboards.
pub(super) fn rules(q: &Q) -> Ans {
    if q.sub == "user:mallory" && q.action == "source.query" { Ans::yes() } else { Ans::No(None) }
}

pub(super) async fn ask(h: &Harness, sub: &str, method: &str, path: &str, body: &[u8]) -> Resp {
    let t = token(sub);
    let mut hs: Vec<(&str, &str)> =
        vec![(HEADER, t.as_str()), ("X-Request-Id", RID), ("If-Match", ZERO_ETAG), ("Idempotency-Key", KEY)];
    if !body.is_empty() {
        hs.push(JSON);
    }
    call(h.addr, method, path, &hs, body).await
}

/// The count of the store operations of the handlers, from `/metrics`.
async fn store_ops(h: &Harness) -> u64 {
    let text = get(h.ops, "/metrics").await.text();
    text.lines()
        .filter(|l| l.starts_with("hune_store_operations_total{"))
        .filter_map(|l| l.rsplit(' ').next().and_then(|v| v.parse::<u64>().ok()))
        .sum()
}

/// The headers of an answer, without `Date`: the clock can change between two answers.
fn headers(r: &Resp) -> Vec<(String, String)> {
    r.headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("date")).cloned().collect()
}

fn query_body(points: bool) -> Vec<u8> {
    let mut v = json!({"etag": ETAG, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
                       "variables": {}});
    if points {
        v["max_data_points"] = json!(100);
    }
    serde_json::to_vec(&v).unwrap()
}

fn preview_body(member: &str, value: &str) -> Vec<u8> {
    let doc: Value = serde_json::from_str(&doc_text(KNOWN, "Known")).unwrap();
    let panel = doc["sections"][0]["panels"][0].clone();
    serde_json::to_vec(&json!({
        member: value, "from_ms": 1_794_211_200_000_i64, "to_ms": 1_794_232_800_000_i64,
        "max_data_points": 1200, "dashboard_variables": [], "variables": {}, "panel": panel
    }))
    .unwrap()
}

/// One pair: the request on the item that exists, and the request on the item that does not exist.
pub(super) struct Pair {
    pub(super) what: &'static str,
    pub(super) method: &'static str,
    pub(super) known: (String, Vec<u8>),
    pub(super) unknown: (String, Vec<u8>),
    /// The uid of the document of the body, when the route runs the pre-pass.
    prepass: Option<(&'static str, &'static str)>,
}

fn by_uid(what: &'static str, method: &'static str, tail: &str, body: &[u8]) -> Pair {
    Pair {
        what,
        method,
        known: (format!("/api/dashboards/{KNOWN}{tail}"), body.to_vec()),
        unknown: (format!("/api/dashboards/{UNKNOWN}{tail}"), body.to_vec()),
        prepass: None,
    }
}

pub(super) fn pairs() -> Vec<Pair> {
    let save = |uid: &str| put_body(&doc_text(uid, "Changed"), "m");
    let mut out = vec![
        Pair {
            what: "save",
            method: "PUT",
            known: (format!("/api/dashboards/{KNOWN}"), save(KNOWN)),
            unknown: (format!("/api/dashboards/{UNKNOWN}"), save(UNKNOWN)),
            prepass: Some((KNOWN, UNKNOWN)),
        },
        Pair {
            what: "save validate_only",
            method: "PUT",
            known: (format!("/api/dashboards/{KNOWN}?validate_only=true"), save(KNOWN)),
            unknown: (format!("/api/dashboards/{UNKNOWN}?validate_only=true"), save(UNKNOWN)),
            prepass: None,
        },
        by_uid("delete", "DELETE", "", b""),
        by_uid("restore", "POST", "/restore", br#"{"version":1}"#),
        by_uid("move", "POST", "/move", br#"{"folder":"ops"}"#),
        Pair {
            what: "move to a folder",
            method: "POST",
            known: (format!("/api/dashboards/{KNOWN}/move"), br#"{"folder":"ops"}"#.to_vec()),
            unknown: (format!("/api/dashboards/{KNOWN}/move"), br#"{"folder":"nofolder"}"#.to_vec()),
            prepass: None,
        },
        by_uid("read", "GET", "", b""),
        by_uid("download", "GET", "?download=true", b""),
        by_uid("history", "GET", "/history", b""),
        by_uid("version", "GET", "/history/1", b""),
        by_uid("panel query", "POST", "/panels/1/query", &query_body(true)),
        by_uid("options query", "POST", "/variables/node/options", &query_body(false)),
        by_uid("annotation query", "POST", "/annotations/0/query", &query_body(false)),
        Pair {
            what: "preview of a saved dashboard",
            method: "POST",
            known: ("/api/sources/prod/preview".to_owned(), preview_body("uid", KNOWN)),
            unknown: ("/api/sources/prod/preview".to_owned(), preview_body("uid", UNKNOWN)),
            prepass: None,
        },
        Pair {
            what: "preview in a folder",
            method: "POST",
            known: ("/api/sources/prod/preview".to_owned(), preview_body("folder", "ops")),
            unknown: ("/api/sources/prod/preview".to_owned(), preview_body("folder", "nofolder")),
            prepass: None,
        },
        Pair {
            what: "rename of a folder",
            method: "PUT",
            known: ("/api/folders/ops".to_owned(), br#"{"title":"T"}"#.to_vec()),
            unknown: ("/api/folders/nofolder".to_owned(), br#"{"title":"T"}"#.to_vec()),
            prepass: None,
        },
        Pair {
            what: "delete of a folder",
            method: "DELETE",
            known: ("/api/folders/ops".to_owned(), Vec::new()),
            unknown: ("/api/folders/nofolder".to_owned(), Vec::new()),
            prepass: None,
        },
    ];
    for (what, path) in [("create", "/api/dashboards"), ("create validate_only", "/api/dashboards?validate_only=true")]
    {
        out.push(Pair {
            what,
            method: "POST",
            known: (path.to_owned(), post_body("ops", &doc_text("oracle-new", "New"))),
            unknown: (path.to_owned(), post_body("nofolder", &doc_text("oracle-new2", "New"))),
            prepass: (what == "create").then_some(("oracle-new", "oracle-new2")),
        });
    }
    out
}

/// The action of the use of the route of a pair (11.21).
pub(super) fn use_of(p: &Pair) -> &'static str {
    match (p.method, p.known.0.as_str()) {
        (_, "/api/sources/prod/preview") => "source.query",
        ("GET", _) => "dashboard.read",
        (_, path) if path.contains("/query") || path.contains("/options") => "dashboard.read",
        _ => "dashboard.write",
    }
}

fn prepass_reads(uid: &str) -> usize {
    tests_hook::READ.lock().unwrap_or_else(PoisonError::into_inner).iter().filter(|u| *u == uid).count()
}

#[tokio::test]
async fn k321_a_refusal_is_the_same_for_a_uid_or_a_folder_that_exists_and_one_that_does_not() {
    let _pairs = PAIRS.lock().await;
    let pdp = Pdp::start(rules).await;
    let h = Harness::start_with(test_timing(), fill_known, |c| authzen(c, &pdp.url(), Duration::ZERO)).await;
    let state = h.state();
    for sub in ["user:eve", "user:mallory"] {
        for p in pairs() {
            let what = format!("{sub} {} {}", p.what, p.method);
            let ops_before = store_ops(&h).await;
            let checks_before = state.validations_run.load(Ordering::SeqCst);
            let reads_before = p.prepass.map(|(k, u)| (prepass_reads(k), prepass_reads(u)));
            pdp.clear();
            let a = ask(&h, sub, p.method, &p.known.0, &p.known.1).await;
            let pdp_known = pdp.requests();
            pdp.clear();
            let b = ask(&h, sub, p.method, &p.unknown.0, &p.unknown.1).await;
            let pdp_unknown = pdp.requests();
            assert_eq!((a.status, a.code().as_str()), (403, "forbidden"), "{what}: {}", a.text());
            assert_eq!(b.status, a.status, "{what}: {}", b.text());
            assert_eq!(headers(&b), headers(&a), "{what}");
            assert_eq!(b.text(), a.text(), "{what}");
            // The body names the use of the route (11.30), whatever question the PDP refused.
            assert_eq!(a.json()["error"]["action"], use_of(&p), "{what}");
            // Neither request reads the store or validates a document (11.20).
            assert_eq!(store_ops(&h).await, ops_before, "{what}: a store operation");
            assert_eq!(state.validations_run.load(Ordering::SeqCst), checks_before, "{what}: a validation");
            // The save and the create read the sources of both documents before the lookup (11.21, 11.23).
            if let (Some((k, u)), Some((rk, ru))) = (p.prepass, reads_before) {
                assert_eq!((prepass_reads(k), prepass_reads(u)), (rk + 1, ru + 1), "{what}: the pre-pass");
            }
            // A subject with no grant gets one request to the PDP in both cases.
            if sub == "user:eve" {
                assert_eq!((pdp_known, pdp_unknown), (1, 1), "{what}");
            }
        }
    }
}
