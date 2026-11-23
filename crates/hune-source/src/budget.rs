// SPDX-License-Identifier: MIT OR Apache-2.0
//! The memory budget of the results in flight (8.4, 8.8, 12.16).

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// The largest reservation that one query can take, in bytes.
pub(crate) const MAX_RESERVATION: u64 = u32::MAX as u64;

/// The budget `server.max_result_memory_bytes`. All sources share one budget.
///
/// A clone shares the same budget. A query reserves `2 x max_result_bytes` before it runs.
/// It keeps the reservation until its frame is complete, it fails or the client leaves.
#[derive(Debug, Clone)]
pub struct ResultMemoryBudget {
    total: u64,
    semaphore: Arc<Semaphore>,
}

/// A reservation of the budget. The drop gives the bytes back.
#[derive(Debug)]
pub struct MemoryReservation {
    _permit: OwnedSemaphorePermit,
    bytes: u64,
}

impl MemoryReservation {
    /// Gives the reserved bytes.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl ResultMemoryBudget {
    /// Makes a budget of `total_bytes`. The value is at most `usize::MAX >> 3`.
    pub fn new(total_bytes: u64) -> ResultMemoryBudget {
        let capped = total_bytes.min((usize::MAX >> 3) as u64);
        ResultMemoryBudget { total: capped, semaphore: Arc::new(Semaphore::new(capped as usize)) }
    }

    /// Gives the size of the budget.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Gives the bytes that no query reserves now.
    pub fn available(&self) -> u64 {
        self.semaphore.available_permits() as u64
    }

    /// Gives the bytes that the queries reserve now.
    pub fn in_use(&self) -> u64 {
        self.total - self.available()
    }

    /// Tells if a reservation of `bytes` can succeed one day.
    pub fn can_hold(&self, bytes: u64) -> bool {
        bytes <= self.total && bytes <= MAX_RESERVATION
    }

    /// Takes a reservation at once, or gives `None`.
    pub fn try_reserve(&self, bytes: u64) -> Option<MemoryReservation> {
        if !self.can_hold(bytes) {
            return None;
        }
        let permit = Arc::clone(&self.semaphore).try_acquire_many_owned(bytes as u32).ok()?;
        Some(MemoryReservation { _permit: permit, bytes })
    }

    /// Waits for a reservation. The caller bounds the wait. The wait is fair: the order of
    /// arrival decides. The future is safe to drop.
    ///
    /// The result is `None` when the budget can never hold `bytes`.
    pub(crate) async fn reserve(&self, bytes: u64) -> Option<MemoryReservation> {
        if !self.can_hold(bytes) {
            return None;
        }
        let permit = Arc::clone(&self.semaphore).acquire_many_owned(bytes as u32).await.ok()?;
        Some(MemoryReservation { _permit: permit, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_add_up_and_return() {
        let b = ResultMemoryBudget::new(100);
        let a = b.try_reserve(60).unwrap();
        assert_eq!(b.in_use(), 60);
        assert_eq!(b.available(), 40);
        assert!(b.try_reserve(50).is_none());
        let c = b.try_reserve(40).unwrap();
        assert_eq!(b.available(), 0);
        drop(a);
        assert_eq!(b.available(), 60);
        drop(c);
        assert_eq!(b.in_use(), 0);
    }

    #[test]
    fn clones_share_the_budget() {
        let b = ResultMemoryBudget::new(10);
        let b2 = b.clone();
        let _r = b.try_reserve(10).unwrap();
        assert!(b2.try_reserve(1).is_none());
        assert_eq!(b2.total(), 10);
    }

    #[test]
    fn a_reservation_above_the_total_never_succeeds() {
        let b = ResultMemoryBudget::new(10);
        assert!(!b.can_hold(11));
        assert!(b.try_reserve(11).is_none());
    }

    #[tokio::test]
    async fn a_waiter_gets_the_bytes_when_they_come_back() {
        let b = ResultMemoryBudget::new(10);
        let r = b.try_reserve(10).unwrap();
        let b2 = b.clone();
        let waiter = tokio::spawn(async move { b2.reserve(5).await.map(|r| r.bytes()) });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(r);
        assert_eq!(waiter.await.unwrap(), Some(5));
    }
}
