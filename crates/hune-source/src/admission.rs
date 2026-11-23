//! Admission control (8.8): the permits of one query before it goes to ClickHouse.
//!
//! A query takes three things in this order. It takes a permit of its subject, when the source
//! has a per-subject limit. It takes a permit of its source. It reserves memory in the budget.
//! The three waits share one deadline. At most 256 queries wait for one source.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio::time::{Instant, timeout_at};

use crate::budget::{MemoryReservation, ResultMemoryBudget};

/// The most queries that wait for one source (8.8, rule 5).
pub(crate) const MAX_WAITING: usize = 256;

/// The reason why the admission refused a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Busy {
    /// The wait queue of the source holds 256 queries.
    QueueFull,
    /// The deadline `queue_wait` passed.
    Timeout,
    /// The memory budget can never hold the reservation of one query.
    BudgetTooSmall,
}

impl Busy {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Busy::QueueFull => "The source has too many queries that wait for a place.",
            Busy::Timeout => "The source had no free place within the queue wait.",
            Busy::BudgetTooSmall => "The memory budget of the results is smaller than the reservation of one query.",
        }
    }
}

/// The counters of the admission, for the metrics of section 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionStats {
    /// The queries that hold a permit of the source.
    pub in_flight: usize,
    /// The queries that wait for a place.
    pub waiting: usize,
    /// The bytes that the memory budget reserves for all sources.
    pub memory_in_use: u64,
}

#[derive(Debug, Default)]
struct SubjectTable {
    entries: HashMap<String, (Arc<Semaphore>, usize)>,
}

#[derive(Debug)]
struct PerSubject {
    limit: u32,
    table: Arc<Mutex<SubjectTable>>,
}

/// The use of one subject entry. The drop removes the entry when no query uses it.
#[derive(Debug)]
struct SubjectLease {
    table: Arc<Mutex<SubjectTable>>,
    key: String,
    semaphore: Arc<Semaphore>,
}

impl Drop for SubjectLease {
    fn drop(&mut self) {
        let mut t = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = t.entries.get_mut(&self.key) {
            entry.1 -= 1;
            if entry.1 == 0 {
                t.entries.remove(&self.key);
            }
        }
    }
}

impl PerSubject {
    fn lease(&self, subject: &str) -> SubjectLease {
        let mut t = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let entry =
            t.entries.entry(subject.to_owned()).or_insert_with(|| (Arc::new(Semaphore::new(self.limit as usize)), 0));
        entry.1 += 1;
        SubjectLease { table: Arc::clone(&self.table), key: subject.to_owned(), semaphore: Arc::clone(&entry.0) }
    }
}

/// The count of a query that waits. The drop takes the query out of the count.
struct WaitGuard<'a>(&'a AtomicUsize);

impl Drop for WaitGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The permits of one query. The drop gives all of them back.
#[derive(Debug)]
pub(crate) struct Permits {
    _memory: MemoryReservation,
    _source: OwnedSemaphorePermit,
    _subject: Option<(OwnedSemaphorePermit, SubjectLease)>,
}

/// The admission control of one source.
#[derive(Debug)]
pub(crate) struct Admission {
    source: Arc<Semaphore>,
    max_concurrent: usize,
    per_subject: Option<PerSubject>,
    waiting: AtomicUsize,
    max_waiting: usize,
    budget: ResultMemoryBudget,
    reserve_bytes: u64,
    queue_wait: Duration,
}

impl Admission {
    pub(crate) fn new(
        max_concurrent: u32,
        max_concurrent_per_subject: Option<u32>,
        queue_wait: Duration,
        budget: ResultMemoryBudget,
        reserve_bytes: u64,
    ) -> Admission {
        Admission {
            source: Arc::new(Semaphore::new(max_concurrent as usize)),
            max_concurrent: max_concurrent as usize,
            per_subject: max_concurrent_per_subject
                .map(|limit| PerSubject { limit, table: Arc::new(Mutex::new(SubjectTable::default())) }),
            waiting: AtomicUsize::new(0),
            max_waiting: MAX_WAITING,
            budget,
            reserve_bytes,
            queue_wait,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_max_waiting(mut self, n: usize) -> Admission {
        self.max_waiting = n;
        self
    }

    /// The deadline of the three waits for a query that arrives now.
    pub(crate) fn deadline(&self) -> Instant {
        Instant::now() + self.queue_wait
    }

    pub(crate) fn stats(&self) -> AdmissionStats {
        AdmissionStats {
            in_flight: self.max_concurrent - self.source.available_permits().min(self.max_concurrent),
            waiting: self.waiting.load(Ordering::Acquire),
            memory_in_use: self.budget.in_use(),
        }
    }

    /// Counts a query that waits, one time. Gives `QueueFull` when 256 queries already wait.
    fn enter_wait<'a>(&'a self, guard: &mut Option<WaitGuard<'a>>) -> Result<(), Busy> {
        if guard.is_some() {
            return Ok(());
        }
        if self.waiting.fetch_add(1, Ordering::AcqRel) >= self.max_waiting {
            self.waiting.fetch_sub(1, Ordering::AcqRel);
            return Err(Busy::QueueFull);
        }
        *guard = Some(WaitGuard(&self.waiting));
        Ok(())
    }

    async fn take<'a>(
        &'a self,
        semaphore: &Arc<Semaphore>,
        deadline: Instant,
        guard: &mut Option<WaitGuard<'a>>,
    ) -> Result<OwnedSemaphorePermit, Busy> {
        match Arc::clone(semaphore).try_acquire_owned() {
            Ok(p) => return Ok(p),
            Err(TryAcquireError::Closed) => return Err(Busy::Timeout),
            Err(TryAcquireError::NoPermits) => {}
        }
        self.enter_wait(guard)?;
        match timeout_at(deadline, Arc::clone(semaphore).acquire_owned()).await {
            Ok(Ok(p)) => Ok(p),
            _ => Err(Busy::Timeout),
        }
    }

    /// Takes the permits of the steps 1 to 3 of 8.8 before `deadline`.
    ///
    /// The future is safe to drop: a permit that it holds goes back.
    pub(crate) async fn acquire(&self, subject: &str, deadline: Instant) -> Result<Permits, Busy> {
        if !self.budget.can_hold(self.reserve_bytes) {
            return Err(Busy::BudgetTooSmall);
        }
        let mut guard = None;
        let subject_part = match &self.per_subject {
            Some(ps) => {
                let lease = ps.lease(subject);
                let permit = self.take(&lease.semaphore, deadline, &mut guard).await?;
                Some((permit, lease))
            }
            None => None,
        };
        let source = self.take(&self.source, deadline, &mut guard).await?;
        let memory = match self.budget.try_reserve(self.reserve_bytes) {
            Some(m) => m,
            None => {
                self.enter_wait(&mut guard)?;
                match timeout_at(deadline, self.budget.reserve(self.reserve_bytes)).await {
                    Ok(Some(m)) => m,
                    Ok(None) => return Err(Busy::BudgetTooSmall),
                    Err(_) => return Err(Busy::Timeout),
                }
            }
        };
        Ok(Permits { _memory: memory, _source: source, _subject: subject_part })
    }

    #[cfg(test)]
    fn subject_entries(&self) -> usize {
        self.per_subject.as_ref().map_or(0, |p| p.table.lock().unwrap().entries.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admission(max: u32, per_subject: Option<u32>, wait_ms: u64, budget: u64, reserve: u64) -> Admission {
        Admission::new(max, per_subject, Duration::from_millis(wait_ms), ResultMemoryBudget::new(budget), reserve)
    }

    #[tokio::test]
    async fn the_source_semaphore_bounds_the_queries() {
        let a = admission(2, None, 50, 1000, 10);
        let p1 = a.acquire("-", a.deadline()).await.unwrap();
        let _p2 = a.acquire("-", a.deadline()).await.unwrap();
        assert_eq!(a.stats().in_flight, 2);
        let t = std::time::Instant::now();
        assert_eq!(a.acquire("-", a.deadline()).await.unwrap_err(), Busy::Timeout);
        assert!(t.elapsed() >= Duration::from_millis(40));
        drop(p1);
        let _p3 = a.acquire("-", a.deadline()).await.unwrap();
        assert_eq!(a.stats().waiting, 0);
    }

    #[tokio::test]
    async fn a_zero_wait_refuses_at_once_when_full() {
        let a = admission(1, None, 0, 1000, 10);
        let _p = a.acquire("-", a.deadline()).await.unwrap();
        assert_eq!(a.acquire("-", a.deadline()).await.unwrap_err(), Busy::Timeout);
    }

    #[tokio::test]
    async fn the_subject_semaphore_bounds_one_subject_only() {
        let a = admission(4, Some(1), 30, 1000, 10);
        let _alice = a.acquire("user:alice", a.deadline()).await.unwrap();
        assert_eq!(a.acquire("user:alice", a.deadline()).await.unwrap_err(), Busy::Timeout);
        // Another subject still passes.
        let _bob = a.acquire("user:bob", a.deadline()).await.unwrap();
        assert_eq!(a.stats().in_flight, 2);
    }

    #[tokio::test]
    async fn a_subject_that_waits_does_not_hold_a_source_place() {
        let a = admission(2, Some(1), 30, 1000, 10);
        let _alice = a.acquire("a", a.deadline()).await.unwrap();
        assert!(a.acquire("a", a.deadline()).await.is_err());
        assert_eq!(a.stats().in_flight, 1);
    }

    #[tokio::test]
    async fn a_subject_entry_goes_away_with_its_last_use() {
        let a = admission(4, Some(2), 30, 1000, 10);
        let p = a.acquire("x", a.deadline()).await.unwrap();
        let q = a.acquire("y", a.deadline()).await.unwrap();
        assert_eq!(a.subject_entries(), 2);
        drop(p);
        assert_eq!(a.subject_entries(), 1);
        drop(q);
        assert_eq!(a.subject_entries(), 0);
        // A refused query leaves no entry either.
        let a = admission(1, Some(1), 5, 1000, 10);
        let _p = a.acquire("x", a.deadline()).await.unwrap();
        assert!(a.acquire("x", a.deadline()).await.is_err());
        assert!(a.acquire("z", a.deadline()).await.is_err());
        assert_eq!(a.subject_entries(), 1);
    }

    #[tokio::test]
    async fn the_memory_budget_bounds_the_queries() {
        // Two queries of 40 bytes do not fit in a budget of 70.
        let a = admission(8, None, 30, 70, 40);
        let p = a.acquire("-", a.deadline()).await.unwrap();
        assert_eq!(a.stats().memory_in_use, 40);
        assert_eq!(a.acquire("-", a.deadline()).await.unwrap_err(), Busy::Timeout);
        // The failed query gave back its source permit.
        assert_eq!(a.stats().in_flight, 1);
        drop(p);
        assert_eq!(a.stats().memory_in_use, 0);
        let _p = a.acquire("-", a.deadline()).await.unwrap();
    }

    #[tokio::test]
    async fn a_budget_that_is_too_small_refuses_at_once() {
        let a = admission(8, None, 5000, 10, 40);
        let t = std::time::Instant::now();
        assert_eq!(a.acquire("-", a.deadline()).await.unwrap_err(), Busy::BudgetTooSmall);
        assert!(t.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn the_budget_is_shared_between_admissions() {
        let budget = ResultMemoryBudget::new(100);
        let a = Admission::new(4, None, Duration::from_millis(20), budget.clone(), 60);
        let b = Admission::new(4, None, Duration::from_millis(20), budget.clone(), 60);
        let _pa = a.acquire("-", a.deadline()).await.unwrap();
        assert_eq!(b.acquire("-", b.deadline()).await.unwrap_err(), Busy::Timeout);
    }

    #[tokio::test]
    async fn the_wait_queue_is_bounded() {
        let a = Arc::new(admission(1, None, 2000, 1000, 10).with_max_waiting(2));
        let p = a.acquire("-", a.deadline()).await.unwrap();
        let mut waiters = Vec::new();
        for _ in 0..2 {
            let a2 = Arc::clone(&a);
            waiters.push(tokio::spawn(async move { a2.acquire("-", a2.deadline()).await.map(|_| ()) }));
        }
        while a.stats().waiting < 2 {
            tokio::task::yield_now().await;
        }
        // The third waiter finds the queue full: it does not wait for the deadline.
        let t = std::time::Instant::now();
        assert_eq!(a.acquire("-", a.deadline()).await.unwrap_err(), Busy::QueueFull);
        assert!(t.elapsed() < Duration::from_millis(500));
        assert_eq!(a.stats().waiting, 2);
        drop(p);
        for w in waiters {
            // The first waiter gets the place and drops it at the end of its task.
            assert!(w.await.unwrap().is_ok());
        }
        assert_eq!(a.stats().waiting, 0);
    }

    #[tokio::test]
    async fn waiters_get_the_place_in_arrival_order() {
        let a = Arc::new(admission(1, None, 5000, 1000, 10));
        let p = a.acquire("-", a.deadline()).await.unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for i in 0..3 {
            let (a2, o2) = (Arc::clone(&a), Arc::clone(&order));
            handles.push(tokio::spawn(async move {
                let permit = a2.acquire("-", a2.deadline()).await.unwrap();
                o2.lock().unwrap().push(i);
                drop(permit);
            }));
            while a.stats().waiting <= i {
                tokio::task::yield_now().await;
            }
        }
        drop(p);
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn a_dropped_wait_gives_nothing_away() {
        let a = Arc::new(admission(1, Some(1), 5000, 1000, 10));
        let p = a.acquire("s", a.deadline()).await.unwrap();
        let a2 = Arc::clone(&a);
        let h = tokio::spawn(async move { a2.acquire("s", a2.deadline()).await.map(|_| ()) });
        while a.stats().waiting < 1 {
            tokio::task::yield_now().await;
        }
        h.abort();
        let _ = h.await;
        assert_eq!(a.stats().waiting, 0);
        drop(p);
        assert_eq!(a.subject_entries(), 0);
        assert!(a.acquire("s", a.deadline()).await.is_ok());
    }
}
