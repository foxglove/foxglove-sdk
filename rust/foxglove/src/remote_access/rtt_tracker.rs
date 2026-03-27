use tracing::debug;

/// EWMA smoothing factor. 0.3 means ~70% of the weight comes from recent history.
const EWMA_ALPHA: f64 = 0.3;

/// Tracks round-trip time measurements with an EWMA, mirroring the app-side approach.
pub(crate) struct RttTracker {
    label: &'static str,
    first_sample_excluded: bool,
    latest_ms: Option<f64>,
    ewma_ms: Option<f64>,
    ewma_variance: f64,
}

impl RttTracker {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            first_sample_excluded: false,
            latest_ms: None,
            ewma_ms: None,
            ewma_variance: 0.0,
        }
    }

    /// The first sample is excluded from the EWMA.
    pub fn record_sample(&mut self, rtt_ms: f64) {
        let label = self.label;
        self.latest_ms = Some(rtt_ms);

        if !self.first_sample_excluded {
            self.first_sample_excluded = true;
            debug!("{label} RTT (first, excluded from average): {rtt_ms:.1}ms");
            return;
        }

        let (ewma, std_dev) = match self.ewma_ms {
            None => {
                self.ewma_ms = Some(rtt_ms);
                self.ewma_variance = 0.0;
                (rtt_ms, 0.0)
            }
            Some(prev_ewma) => {
                let diff = rtt_ms - prev_ewma;
                let ewma = prev_ewma + EWMA_ALPHA * diff;
                self.ewma_variance = (1.0 - EWMA_ALPHA) * (self.ewma_variance + EWMA_ALPHA * diff * diff);
                self.ewma_ms = Some(ewma);
                (ewma, self.ewma_variance.sqrt())
            }
        };

        debug!("{label} RTT: {rtt_ms:.1}ms | ewma: {ewma:.1}ms | stddev: {std_dev:.1}ms");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_sample_excluded() {
        let mut tracker = RttTracker::new("test");
        tracker.record_sample(100.0);
        assert_eq!(tracker.latest_ms, Some(100.0));
        assert_eq!(tracker.ewma_ms, None);
    }

    #[test]
    fn test_ewma_initialized_on_second_sample() {
        let mut tracker = RttTracker::new("test");
        tracker.record_sample(999.0); // excluded
        tracker.record_sample(100.0);

        assert_eq!(tracker.ewma_ms, Some(100.0));
        assert_eq!(tracker.ewma_variance, 0.0);
    }

    #[test]
    fn test_ewma_reacts_to_spike() {
        let mut tracker = RttTracker::new("test");
        tracker.record_sample(0.0); // excluded

        // Steady state
        for _ in 0..10 {
            tracker.record_sample(100.0);
        }
        let steady_ewma = tracker.ewma_ms.unwrap();
        assert!((steady_ewma - 100.0).abs() < 0.01);

        // Spike
        tracker.record_sample(500.0);
        let spiked_ewma = tracker.ewma_ms.unwrap();
        // EWMA should react: 100 + 0.3 * (500 - 100) = 220
        assert!((spiked_ewma - 220.0).abs() < 0.01);
        assert!(tracker.ewma_variance.sqrt() > 0.0);
    }
}
