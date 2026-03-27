use std::collections::VecDeque;

use tracing::debug;

const RTT_ROLLING_WINDOW_SIZE: usize = 10;

/// Tracks round-trip time measurements with a rolling window, mirroring the app-side approach.
pub(crate) struct RttTracker {
    first_sample_excluded: bool,
    samples: VecDeque<f64>,
}

impl RttTracker {
    pub fn new() -> Self {
        Self {
            first_sample_excluded: false,
            samples: VecDeque::with_capacity(RTT_ROLLING_WINDOW_SIZE),
        }
    }

    /// The first sample is excluded from the rolling average.
    pub fn record_sample(&mut self, rtt_ms: f64) {
        if !self.first_sample_excluded {
            self.first_sample_excluded = true;
            debug!("RTT (first, excluded from average): {rtt_ms:.1}ms");
            return;
        }

        self.samples.push_back(rtt_ms);
        if self.samples.len() > RTT_ROLLING_WINDOW_SIZE {
            self.samples.pop_front();
        }

        let n = self.samples.len() as f64;
        let sum: f64 = self.samples.iter().sum();
        let avg = sum / n;

        let variance: f64 = self.samples.iter().map(|s| (s - avg).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        debug!(
            "RTT: {rtt_ms:.1}ms | avg: {avg:.1}ms | stddev: {std_dev:.1}ms (n={})",
            self.samples.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_sample_excluded() {
        let mut tracker = RttTracker::new();
        tracker.record_sample(100.0);
        // First sample is excluded; only one sample tracked so no rolling stats yet
        assert!(tracker.samples.is_empty());
    }

    #[test]
    fn test_rolling_average() {
        let mut tracker = RttTracker::new();
        tracker.record_sample(999.0); // excluded
        tracker.record_sample(10.0);
        tracker.record_sample(20.0);
        tracker.record_sample(30.0);

        assert_eq!(tracker.samples.len(), 3);
    }

    #[test]
    fn test_window_size_limit() {
        let mut tracker = RttTracker::new();
        tracker.record_sample(0.0); // excluded

        for i in 1..=15 {
            tracker.record_sample(i as f64);
        }
        assert_eq!(tracker.samples.len(), RTT_ROLLING_WINDOW_SIZE);
    }
}
