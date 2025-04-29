use std::time::{Duration, Instant};

/// A time tracker for throttling the frequency of an action.
#[derive(Debug)]
pub(crate) struct Throttler {
    interval: Duration,
    next_at: Option<Instant>,
}
impl Throttler {
    /// Create a new throttler with the specified duration.
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_at: None,
        }
    }

    /// Returns true if the action is allowed now, and updates the internal timestamp.
    pub fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        if self.next_at.is_some_and(|t| now < t) {
            false
        } else {
            self.next_at = Some(now + self.interval);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttler() {
        let interval = Duration::from_millis(10);
        let mut throttler = Throttler::new(interval);
        assert!(throttler.try_acquire());
        assert!(!throttler.try_acquire());
        assert!(!throttler.try_acquire());
        std::thread::sleep(interval);
        assert!(throttler.try_acquire());
        assert!(!throttler.try_acquire());
        assert!(!throttler.try_acquire());
    }
}
