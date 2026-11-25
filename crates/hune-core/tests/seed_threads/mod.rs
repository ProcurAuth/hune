// SPDX-License-Identifier: MIT OR Apache-2.0
//! Run the seeds of a random test on all cores of the host.
//!
//! Each seed has its own generator and its own counters. A seed gives the same cases as in a run on one thread.
//! The sum of the counters does not depend on the order of the seeds. The test makes its checks on the sum.
//!
//! The first failure stops the other threads after their current seed. The function then panics with the seed
//! and the message of the failed check, so the report of the test names the seed. The checks of a case name the
//! seed and the case too. A person runs one seed again with `HUNE_TEST_SEED`, as before.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The counters of a test. The function `merge` adds the counters of another thread.
pub trait Merge {
    fn merge(&mut self, other: Self);
}

/// The text of a panic payload.
fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic with no text".to_owned()
    }
}

/// Run `run_seed` once for each seed of `seeds`, on up to one thread for each core, and give the sum of the counters.
pub fn run_seeds<S, F>(seeds: &[u64], run_seed: F) -> S
where
    S: Default + Merge + Send,
    F: Fn(u64, &mut S) + Sync,
{
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(seeds.len()).max(1);
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let first_failure: Mutex<Option<(u64, String)>> = Mutex::new(None);
    let total = Mutex::new(S::default());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut mine = S::default();
                while !failed.load(Ordering::SeqCst) {
                    let Some(&seed) = seeds.get(next.fetch_add(1, Ordering::SeqCst)) else { break };
                    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| run_seed(seed, &mut mine))) {
                        failed.store(true, Ordering::SeqCst);
                        let mut slot = first_failure.lock().unwrap_or_else(|e| e.into_inner());
                        if slot.is_none() {
                            *slot = Some((seed, payload_text(payload.as_ref())));
                        }
                        break;
                    }
                }
                total.lock().unwrap_or_else(|e| e.into_inner()).merge(mine);
            });
        }
    });
    if let Some((seed, text)) = first_failure.into_inner().unwrap_or_else(|e| e.into_inner()) {
        panic!("the seed {seed} failed: {text}");
    }
    total.into_inner().unwrap_or_else(|e| e.into_inner())
}
