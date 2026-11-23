// SPDX-License-Identifier: MIT OR Apache-2.0
//! The small connection pool of a PostgreSQL source (8a.9).
//!
//! The pool holds the idle sessions only. The admission of 8a.10 bounds the sessions in use: a
//! permit gives a connection, or the right to open one. So a source holds `max_concurrent`
//! sessions for the queries, plus the probe session and one cancel session (8a.9).
//!
//! - The pool keeps at most `max_idle` idle sessions (`max_concurrent`).
//! - A session idles 60 s at most. A reaper task closes the older ones.
//! - A session lives 1 h at most. The pool checks the age when the session goes back, and when it
//!   leaves.
//! - A session that met an error, a cap, a deadline or a cancel never goes back: the caller drops it.
//! - [`Pool::clear`] closes each idle session. The source calls it when its state becomes
//!   `misconfigured` (8a.12).
//!
//! The pool does not use `deadpool-postgres`. A pooler in transaction mode is not supported (8a.9).
//! The type is generic over [`Item`], so the unit tests run with no server.

use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

use crate::postgres::session::{MAX_AGE, MAX_IDLE, Session};

/// What the pool needs to know about a session.
pub(crate) trait Item: Send + 'static {
    /// Tells if the connection is open.
    fn is_open(&self) -> bool;
    /// Gives the time since the connection was opened.
    fn age(&self) -> Duration;
}

impl Item for Session {
    fn is_open(&self) -> bool {
        Session::is_open(self)
    }

    fn age(&self) -> Duration {
        Session::age(self)
    }
}

struct Idle<T> {
    item: T,
    since: Instant,
}

struct State<T> {
    idle: Vec<Idle<T>>,
    reaper_running: bool,
}

struct Inner<T> {
    state: Mutex<State<T>>,
    max_idle: usize,
    idle_max: Duration,
    age_max: Duration,
}

/// The pool of idle sessions of one source.
pub(crate) struct Pool<T: Item = Session> {
    inner: Arc<Inner<T>>,
}

impl<T: Item> Clone for Pool<T> {
    fn clone(&self) -> Self {
        Pool { inner: Arc::clone(&self.inner) }
    }
}

impl<T: Item> std::fmt::Debug for Pool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool").field("idle", &self.idle_count()).finish()
    }
}

impl<T: Item> Pool<T> {
    /// Makes a pool with the limits of 8a.9: 60 s idle, 1 h of age.
    pub(crate) fn new(max_idle: usize) -> Pool<T> {
        Pool::with_limits(max_idle, MAX_IDLE, MAX_AGE)
    }

    /// Makes a pool with other limits (the tests use short ones).
    pub(crate) fn with_limits(max_idle: usize, idle_max: Duration, age_max: Duration) -> Pool<T> {
        Pool {
            inner: Arc::new(Inner {
                state: Mutex::new(State { idle: Vec::new(), reaper_running: false }),
                max_idle,
                idle_max,
                age_max,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State<T>> {
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn fresh(&self, entry: &Idle<T>) -> bool {
        entry.item.is_open() && entry.since.elapsed() < self.inner.idle_max && entry.item.age() < self.inner.age_max
    }

    /// Gives the number of idle sessions.
    pub(crate) fn idle_count(&self) -> usize {
        self.lock().idle.len()
    }

    /// Takes the most recent idle session that is still good. It closes the stale ones.
    pub(crate) fn take(&self) -> Option<T> {
        let mut st = self.lock();
        while let Some(entry) = st.idle.pop() {
            if self.fresh(&entry) {
                return Some(entry.item);
            }
        }
        None
    }

    /// Gives a clean session back. A session that is closed or too old is dropped. The pool keeps
    /// at most `max_idle` sessions: the oldest one goes away first.
    pub(crate) fn put(&self, item: T) {
        if !item.is_open() || item.age() >= self.inner.age_max {
            return;
        }
        let start_reaper = {
            let mut st = self.lock();
            st.idle.push(Idle { item, since: Instant::now() });
            if st.idle.len() > self.inner.max_idle {
                st.idle.remove(0);
            }
            !std::mem::replace(&mut st.reaper_running, true)
        };
        if start_reaper {
            self.spawn_reaper();
        }
    }

    /// Closes each idle session.
    pub(crate) fn clear(&self) {
        let old = std::mem::take(&mut self.lock().idle);
        drop(old);
    }

    fn spawn_reaper(&self) {
        let weak = Arc::downgrade(&self.inner);
        // A pool that is used outside a runtime has no reaper: `take` still skips the stale sessions.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(reap(weak));
            }
            Err(_) => self.lock().reaper_running = false,
        }
    }
}

/// Closes the idle sessions that are too old. It ends when the pool has no idle session.
async fn reap<T: Item>(pool: Weak<Inner<T>>) {
    loop {
        let Some(inner) = pool.upgrade() else { return };
        let tick = (inner.idle_max / 4).clamp(Duration::from_millis(10), Duration::from_secs(5));
        drop(inner);
        tokio::time::sleep(tick).await;
        let Some(inner) = pool.upgrade() else { return };
        let (empty, stale) = {
            let mut st = inner.state.lock().unwrap_or_else(PoisonError::into_inner);
            let (keep, stale): (Vec<_>, Vec<_>) = std::mem::take(&mut st.idle)
                .into_iter()
                .partition(|e| e.item.is_open() && e.since.elapsed() < inner.idle_max && e.item.age() < inner.age_max);
            st.idle = keep;
            let empty = st.idle.is_empty();
            if empty {
                st.reaper_running = false;
            }
            (empty, stale)
        };
        // The sessions close here, outside the lock.
        drop(stale);
        if empty {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    struct Fake {
        open: Arc<AtomicBool>,
        born: Instant,
        closed: Arc<AtomicUsize>,
    }

    impl Fake {
        fn new(closed: &Arc<AtomicUsize>) -> Fake {
            Fake { open: Arc::new(AtomicBool::new(true)), born: Instant::now(), closed: Arc::clone(closed) }
        }

        fn aged(closed: &Arc<AtomicUsize>, age: Duration) -> Fake {
            Fake {
                open: Arc::new(AtomicBool::new(true)),
                born: Instant::now().checked_sub(age).expect("clock"),
                closed: Arc::clone(closed),
            }
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.closed.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Item for Fake {
        fn is_open(&self) -> bool {
            self.open.load(Ordering::SeqCst)
        }

        fn age(&self) -> Duration {
            self.born.elapsed()
        }
    }

    fn counter() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    #[test]
    fn the_defaults_are_the_values_of_8a9() {
        assert_eq!(MAX_IDLE, Duration::from_secs(60));
        assert_eq!(MAX_AGE, Duration::from_secs(3600));
    }

    #[test]
    fn put_then_take_gives_the_session_back() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::new(2);
        pool.put(Fake::new(&closed));
        assert_eq!(pool.idle_count(), 1);
        assert!(pool.take().is_some());
        assert_eq!(pool.idle_count(), 0);
        assert!(pool.take().is_none());
    }

    #[test]
    fn the_pool_keeps_at_most_max_idle_and_closes_the_oldest() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::new(2);
        for _ in 0..4 {
            pool.put(Fake::new(&closed));
        }
        assert_eq!(pool.idle_count(), 2);
        assert_eq!(closed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_closed_session_does_not_go_back_and_is_skipped_at_take() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::new(4);
        let dead = Fake::new(&closed);
        dead.open.store(false, Ordering::SeqCst);
        pool.put(dead);
        assert_eq!(pool.idle_count(), 0);
        // A session that closes while it idles is skipped, and closed, at take.
        let live = Fake::new(&closed);
        let flag = Arc::clone(&live.open);
        pool.put(live);
        flag.store(false, Ordering::SeqCst);
        assert!(pool.take().is_none());
    }

    #[test]
    fn a_session_older_than_the_age_limit_is_closed_when_it_goes_back() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::with_limits(4, Duration::from_secs(60), Duration::from_secs(3600));
        pool.put(Fake::aged(&closed, Duration::from_secs(3601)));
        assert_eq!(pool.idle_count(), 0);
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        pool.put(Fake::aged(&closed, Duration::from_secs(3599)));
        assert_eq!(pool.idle_count(), 1);
    }

    #[test]
    fn a_session_that_idles_too_long_is_skipped_at_take() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::with_limits(4, Duration::from_millis(30), Duration::from_secs(3600));
        pool.put(Fake::new(&closed));
        std::thread::sleep(Duration::from_millis(60));
        assert!(pool.take().is_none());
        assert_eq!(closed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_reaper_closes_the_idle_sessions_that_are_too_old() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::with_limits(4, Duration::from_millis(80), Duration::from_secs(3600));
        pool.put(Fake::new(&closed));
        assert_eq!(pool.idle_count(), 1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(pool.idle_count(), 0, "the reaper closed it");
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        // A new put starts a new reaper.
        pool.put(Fake::new(&closed));
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(pool.idle_count(), 0);
    }

    #[test]
    fn clear_closes_each_idle_session() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::new(4);
        pool.put(Fake::new(&closed));
        pool.put(Fake::new(&closed));
        pool.clear();
        assert_eq!(pool.idle_count(), 0);
        assert_eq!(closed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn take_gives_the_most_recent_session_first() {
        let closed = counter();
        let pool: Pool<Fake> = Pool::new(4);
        let old = Fake::new(&closed);
        let old_flag = Arc::clone(&old.open);
        pool.put(old);
        let new = Fake::new(&closed);
        let new_flag = Arc::clone(&new.open);
        pool.put(new);
        let got = pool.take().expect("one");
        assert!(Arc::ptr_eq(&got.open, &new_flag));
        assert!(!Arc::ptr_eq(&got.open, &old_flag));
    }
}
