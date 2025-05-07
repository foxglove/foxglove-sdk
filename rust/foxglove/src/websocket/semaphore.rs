//! A semaphore for admission control

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A non-blocking counting semaphore for concurrency control.
///
/// Decrements the inner counter when acquired.
#[derive(Debug, Clone)]
pub(crate) struct Semaphore(Arc<AtomicUsize>);

impl Semaphore {
    /// Constructs a new semaphore.
    pub fn new(count: usize) -> Self {
        Self(Arc::new(AtomicUsize::new(count)))
    }

    /// Attempts to acquire the semaphore.
    pub fn try_acquire(&self) -> Option<SemaphoreGuard> {
        loop {
            let current = self.0.load(Ordering::Acquire);
            if current == 0 {
                return None;
            }
            if self
                .0
                .compare_exchange(current, current - 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(SemaphoreGuard(self.0.clone()));
            }
        }
    }
}

/// A counting semaphore guard.
///
/// Increments the inner counter when dropped.
#[derive(Debug)]
pub(crate) struct SemaphoreGuard(Arc<AtomicUsize>);

impl Drop for SemaphoreGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::task::JoinSet;

    #[test]
    fn test_semaphore() {
        let sem = Semaphore::new(3);
        assert_eq!(sem.0.load(Ordering::Acquire), 3);

        let g1 = sem.try_acquire().unwrap();
        let g2 = sem.try_acquire().unwrap();
        let g3 = sem.try_acquire().unwrap();
        assert_eq!(sem.0.load(Ordering::Acquire), 0);
        assert!(sem.try_acquire().is_none());

        drop(g1);
        assert_eq!(sem.0.load(Ordering::Acquire), 1);
        assert!(sem.try_acquire().is_some());

        drop(g2);
        drop(g3);
        assert_eq!(sem.0.load(Ordering::Acquire), 3);
        assert!(sem.try_acquire().is_some());
    }

    #[tokio::test]
    async fn test_concurrent() {
        let sem = Semaphore::new(5);
        let flags = Arc::new(AtomicUsize::new(5));
        let mut tasks = JoinSet::new();

        const PLAYERS: u64 = 100;
        const ROUNDS: u64 = 100;
        for id in 0..PLAYERS {
            let sem = sem.clone();
            let flags = flags.clone();
            tasks.spawn(async move {
                for round in 0..ROUNDS {
                    let sleep_time = Duration::from_micros((id + round) % 13 + 1);
                    if let Some(_guard) = sem.try_acquire() {
                        let prev = flags.fetch_sub(1, Ordering::Relaxed);
                        assert_ne!(prev, 0);
                        tokio::time::sleep(sleep_time).await;
                        flags.fetch_add(1, Ordering::Relaxed);
                    } else {
                        tokio::time::sleep(sleep_time).await;
                    }
                }
            });
        }

        tasks.join_all().await;
    }
}
