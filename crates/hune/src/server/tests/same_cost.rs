//! Decision ID-K325, point 1 (owner answer of 2026-11-22): a uid or a folder that exists and one that does not exist
//! cost the same PDP requests on each write route.
//!
//! For each write route, with `cache_ttl` of 0 s and of 30 s, a subject with no grant and a subject with
//! `source.query` send the request for the item that exists and the request for the item that does not exist, three
//! times each. Each request gives the same count of PDP requests, the same count of questions, no kept answer read
//! (the metric `hune_pdp_cache_hits_total`) and no kept answer written (11.23, 11.27). Before the fix, the question of
//! 11.23 was a kept answer after its first use, the questions of an item that exists were kept answers on the next
//! request, and a save or a create of a subject with `source.query` sent two requests for an item that exists and one
//! for an item that does not exist.

use super::authzen_pdp::*;
use super::harness::*;
use super::same_refusal::{PAIRS, Pair, ask, fill_known, pairs, rules, use_of};
use std::time::Duration;

/// What one request cost: the PDP requests, the questions, the kept answers read and the kept answers written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cost {
    requests: usize,
    questions: usize,
    cache_hits: u64,
    kept: isize,
}

async fn cache_hits(h: &Harness) -> u64 {
    let text = get(h.ops, "/metrics").await.text();
    text.lines()
        .find_map(|l| l.strip_prefix("hune_pdp_cache_hits_total ").and_then(|v| v.trim().parse::<u64>().ok()))
        .unwrap_or(0)
}

fn kept(h: &Harness) -> isize {
    let access = h.state().access.clone().expect("mode authzen");
    isize::try_from(access.pep().kept_answers()).unwrap()
}

async fn cost(h: &Harness, pdp: &Pdp, sub: &str, method: &str, (path, body): &(String, Vec<u8>)) -> Cost {
    let (hits, kept_before) = (cache_hits(h).await, kept(h));
    pdp.clear();
    let r = ask(h, sub, method, path, body).await;
    assert_eq!((r.status, r.code().as_str()), (403, "forbidden"), "{sub} {method} {path}: {}", r.text());
    Cost {
        requests: pdp.requests(),
        questions: pdp.asked().len(),
        cache_hits: cache_hits(h).await - hits,
        kept: kept(h) - kept_before,
    }
}

/// The write routes of the pairs of `same_refusal`: each route whose use is not `dashboard.read` (11.21).
fn write_pairs() -> Vec<Pair> {
    pairs().into_iter().filter(|p| use_of(p) != "dashboard.read").collect()
}

#[tokio::test]
async fn k325_a_write_route_costs_the_same_pdp_requests_for_an_item_that_exists_and_one_that_does_not() {
    let _pairs = PAIRS.lock().await;
    for ttl in [Duration::ZERO, Duration::from_secs(30)] {
        let pdp = Pdp::start(rules).await;
        let h = Harness::start_with(test_timing(), fill_known, |c| authzen(c, &pdp.url(), ttl)).await;
        let routes = write_pairs();
        assert_eq!(routes.len(), 12, "save, validate_only, delete, restore, 2 moves, 2 previews, 2 folders, 2 creates");
        for sub in ["user:eve", "user:mallory"] {
            for p in &routes {
                let what = format!("ttl {ttl:?} {sub} {} {}", p.what, p.method);
                let (mut known, mut unknown) = (Vec::new(), Vec::new());
                for round in 0..3 {
                    // The order changes at each round, so neither path always comes first.
                    let first_known = round % 2 == 0;
                    let first = if first_known { &p.known } else { &p.unknown };
                    let second = if first_known { &p.unknown } else { &p.known };
                    let a = cost(&h, &pdp, sub, p.method, first).await;
                    let b = cost(&h, &pdp, sub, p.method, second).await;
                    let (k, u) = if first_known { (a, b) } else { (b, a) };
                    known.push(k);
                    unknown.push(u);
                }
                assert_eq!(known, unknown, "{what}: the item that exists and the one that does not");
                // `source.query` is granted: a save and a create ask it, then the use or the question of 11.23.
                let asks_sources = sub == "user:mallory" && (p.what == "save" || p.what == "create");
                let requests = if asks_sources { 2 } else { 1 };
                let expected = Cost { requests, questions: requests, cache_hits: 0, kept: 0 };
                assert_eq!(known, vec![expected; 3], "{what}: each request asks the PDP, with no kept answer");
            }
        }
    }
}

async fn pool_in_use(h: &Harness) -> Option<i64> {
    let text = get(h.ops, "/metrics").await.text();
    text.lines().find_map(|l| l.strip_prefix("hune_pdp_pool_in_use ").and_then(|v| v.trim().parse::<i64>().ok()))
}

/// A call with no kept answer runs in its own task. When the client leaves during the call, the call still ends, and
/// the count of the permits in use comes back to 0 (11.13). Before the fix, a call of `cache_ttl = "0s"` that the
/// client left kept the gauge `hune_pdp_pool_in_use` at 1, and each write route now takes this path.
#[tokio::test]
async fn k325_a_call_that_the_client_leaves_still_ends_its_count() {
    for ttl in [Duration::ZERO, Duration::from_secs(30)] {
        let pdp = Pdp::start(rules).await;
        let h = Harness::start_with(test_timing(), fill_known, |c| authzen(c, &pdp.url(), ttl)).await;
        pdp.set_fault(Fault::Delay(Duration::from_millis(400)));
        let t = token("user:eve");
        let mut c = Client::connect(h.addr).await;
        c.send(&request("DELETE", "/api/folders/ops", &[(HEADER, &t)], b"")).await;
        let started = std::time::Instant::now();
        while pdp.requests() == 0 {
            assert!(started.elapsed() < Duration::from_secs(5), "ttl {ttl:?}: the call starts");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // The client leaves before the answer of the PDP.
        drop(c);
        let mut last = pool_in_use(&h).await;
        while last != Some(0) && started.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(20)).await;
            last = pool_in_use(&h).await;
        }
        assert_eq!(last, Some(0), "ttl {ttl:?}: the permits in use after the end of the call");
    }
}

/// The question of 11.23 is never kept, also on a read route: each request for an unknown uid sends one PDP request.
#[tokio::test]
async fn k325_the_question_of_11_23_is_never_a_kept_answer() {
    let _pairs = PAIRS.lock().await;
    let pdp = Pdp::start(rules).await;
    let h = Harness::start_with(test_timing(), fill_known, |c| authzen(c, &pdp.url(), Duration::from_secs(30))).await;
    for p in pairs().iter().filter(|p| use_of(p) == "dashboard.read") {
        for round in 0..3 {
            let c = cost(&h, &pdp, "user:eve", p.method, &p.unknown).await;
            assert_eq!(c, Cost { requests: 1, questions: 1, cache_hits: 0, kept: 0 }, "{} round {round}", p.what);
            assert_eq!(pdp.asked()[0].short(), "", "{}: dashboard.read on the service", p.what);
        }
    }
}
