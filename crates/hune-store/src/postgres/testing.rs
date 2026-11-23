// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fault injection of the PostgreSQL backend for the conformance suite (5.33, item 3).
//! This module is compiled only for the tests and with the feature `conformance`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use super::PgStore;

/// A point where the next commit fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgFaultPoint {
    /// The write returns `Unavailable` before `COMMIT`. The server rolls the transaction back.
    BeforeCommit,
    /// The write returns `Unavailable` after `COMMIT`. This is a lost answer: the write is durable.
    AfterCommit,
}

/// The faults of one store. A clone shares the state.
#[derive(Debug, Clone, Default)]
pub struct PgFaults {
    before: Arc<AtomicU32>,
    after: Arc<AtomicU32>,
    session_sql: Arc<Mutex<Option<String>>>,
}

impl PgFaults {
    /// Arms one fault: the next commit that reaches `point` fails one time.
    pub fn inject(&self, point: PgFaultPoint) {
        match point {
            PgFaultPoint::BeforeCommit => self.before.fetch_add(1, Ordering::SeqCst),
            PgFaultPoint::AfterCommit => self.after.fetch_add(1, Ordering::SeqCst),
        };
    }

    /// Sends one statement on the next connection that the pool gives, before the caller uses it.
    /// For example `SET default_transaction_read_only = on` makes the next write fail with `25006`.
    pub fn run_on_next_connection(&self, sql: &str) {
        if let Ok(mut g) = self.session_sql.lock() {
            *g = Some(sql.to_owned());
        }
    }

    pub(super) fn take(&self, point: PgFaultPoint) -> bool {
        let counter = match point {
            PgFaultPoint::BeforeCommit => &self.before,
            PgFaultPoint::AfterCommit => &self.after,
        };
        let mut n = counter.load(Ordering::SeqCst);
        while n > 0 {
            match counter.compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => return true,
                Err(now) => n = now,
            }
        }
        false
    }

    pub(super) fn take_session_sql(&self) -> Option<String> {
        self.session_sql.lock().ok().and_then(|mut g| g.take())
    }
}

impl PgStore {
    /// The fault handle of this store (tests only).
    pub fn faults(&self) -> PgFaults {
        self.inner.faults.clone()
    }

    /// The number of connections that the pool holds and the number that wait (tests only).
    pub fn pool_status(&self) -> (usize, usize) {
        let s = self.inner.pool.status();
        (s.size, s.available)
    }
}

/// The `SystemTime` of a `UtcMillis`, as the backend writes it (tests only).
pub fn system_time(t: crate::types::UtcMillis) -> std::time::SystemTime {
    super::rows::to_system(t)
}
