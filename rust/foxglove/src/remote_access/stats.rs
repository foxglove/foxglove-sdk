use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::throttler::Throttler;

/// Minimum interval between gateway-wide warnings for each warned drop or reset cause.
const WARN_INTERVAL: Duration = Duration::from_secs(30);

/// The quality of the gateway's connection to the remote access server, as estimated by the
/// server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionQuality {
    /// The connection is excellent.
    Excellent,
    /// The connection is good.
    Good,
    /// The connection is poor.
    Poor,
    /// The connection has been lost.
    Lost,
}

impl From<livekit::prelude::ConnectionQuality> for ConnectionQuality {
    fn from(quality: livekit::prelude::ConnectionQuality) -> Self {
        match quality {
            livekit::prelude::ConnectionQuality::Excellent => Self::Excellent,
            livekit::prelude::ConnectionQuality::Good => Self::Good,
            livekit::prelude::ConnectionQuality::Poor => Self::Poor,
            livekit::prelude::ConnectionQuality::Lost => Self::Lost,
        }
    }
}

/// A snapshot of remote access gateway statistics, returned by
/// [`GatewayHandle::stats`](crate::remote_access::GatewayHandle::stats).
///
/// The drop and reset counters are cumulative over the lifetime of the gateway. They are never
/// reset, including when the gateway returns to its dormant state or reconnects, so the
/// difference between two snapshots is the number of events in between. Each counter covers a
/// single cause; there is no per-channel breakdown.
///
/// Some drops are expected in normal operation and are not logged as warnings:
/// [`data_track_not_ready_drops`](Self::data_track_not_ready_drops) for the first few
/// milliseconds after each channel is advertised, and
/// [`video_queue_drops`](Self::video_queue_drops) and
/// [`point_cloud_queue_drops`](Self::point_cloud_queue_drops) whenever encoding is slower than
/// the log rate. [`data_track_full_drops`](Self::data_track_full_drops) and
/// [`participant_resets`](Self::participant_resets) indicate that the link to viewers is
/// congested, and are logged as throttled warnings.
///
/// The remaining fields describe the current session, if any. A session is active while the
/// gateway is joined to a remote access room, which happens when a viewer requests a
/// connection.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct GatewayStats {
    /// Messages dropped because the channel's data track was not published: either it was
    /// still being published after the channel was advertised, or it had been unpublished.
    pub data_track_not_ready_drops: u64,
    /// Messages dropped because the channel's data track send queue was full.
    ///
    /// These messages had already been assigned a sequence number, so viewers also observe them
    /// as gaps in the sequence.
    pub data_track_full_drops: u64,
    /// Messages dropped because they exceeded the maximum data track message size. See
    /// [`Gateway::max_data_track_message_size`](crate::remote_access::Gateway::max_data_track_message_size).
    pub oversized_drops: u64,
    /// Image messages dropped from a full video encoding queue, before encoding.
    pub video_queue_drops: u64,
    /// Point cloud messages dropped from a full point cloud compression queue, before
    /// compression.
    pub point_cloud_queue_drops: u64,
    /// Viewers that were disconnected and reconnected after a control-plane failure, either
    /// because their control message queue overflowed or because a write to their control
    /// stream failed. See
    /// [`Gateway::message_backlog_size`](crate::remote_access::Gateway::message_backlog_size).
    pub participant_resets: u64,
    /// Whether the gateway is currently joined to a remote access session.
    pub session_active: bool,
    /// The number of viewers connected to the current session, or zero if no session is
    /// active.
    pub viewer_count: usize,
    /// The most recent ICE round-trip time for the current session.
    ///
    /// This is sampled every 30 seconds, and is `None` if no session is active or if the
    /// session has not been sampled yet.
    pub ice_rtt: Option<Duration>,
    /// The quality of the gateway's connection for the current session, or `None` if no
    /// session is active.
    pub connection_quality: Option<ConnectionQuality>,
}

/// Gateway-lifetime drop and reset counters, shared by every session the gateway runs.
#[derive(Debug)]
pub(crate) struct GatewayCounters {
    data_track_not_ready_drops: AtomicU64,
    data_track_full_drops: AtomicU64,
    oversized_drops: AtomicU64,
    video_queue_drops: AtomicU64,
    point_cloud_queue_drops: AtomicU64,
    participant_resets: AtomicU64,
    data_track_full_throttler: parking_lot::Mutex<Throttler>,
    participant_reset_throttler: parking_lot::Mutex<Throttler>,
}

impl Default for GatewayCounters {
    fn default() -> Self {
        Self {
            data_track_not_ready_drops: AtomicU64::new(0),
            data_track_full_drops: AtomicU64::new(0),
            oversized_drops: AtomicU64::new(0),
            video_queue_drops: AtomicU64::new(0),
            point_cloud_queue_drops: AtomicU64::new(0),
            participant_resets: AtomicU64::new(0),
            data_track_full_throttler: parking_lot::Mutex::new(Throttler::new(WARN_INTERVAL)),
            participant_reset_throttler: parking_lot::Mutex::new(Throttler::new(WARN_INTERVAL)),
        }
    }
}

impl GatewayCounters {
    pub fn record_data_track_not_ready_drop(&self) {
        self.data_track_not_ready_drops
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Counts a data-track-full drop. Returns the cumulative count if a warning is due, which
    /// happens at most once per [`WARN_INTERVAL`] across the gateway.
    #[must_use]
    pub fn record_data_track_full_drop(&self) -> Option<u64> {
        let total = self.data_track_full_drops.fetch_add(1, Ordering::Relaxed) + 1;
        self.data_track_full_throttler
            .lock()
            .try_acquire()
            .then_some(total)
    }

    pub fn record_oversized_drop(&self) {
        self.oversized_drops.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_video_queue_drops(&self, count: u64) {
        if count > 0 {
            self.video_queue_drops.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub fn record_point_cloud_queue_drops(&self, count: u64) {
        if count > 0 {
            self.point_cloud_queue_drops
                .fetch_add(count, Ordering::Relaxed);
        }
    }

    /// Counts a participant reset. Returns the cumulative count if a warning is due, which
    /// happens at most once per [`WARN_INTERVAL`] across the gateway.
    #[must_use]
    pub fn record_participant_reset(&self) -> Option<u64> {
        let total = self.participant_resets.fetch_add(1, Ordering::Relaxed) + 1;
        self.participant_reset_throttler
            .lock()
            .try_acquire()
            .then_some(total)
    }

    /// Returns a snapshot with the counters filled in, and no active session.
    pub fn snapshot(&self) -> GatewayStats {
        GatewayStats {
            data_track_not_ready_drops: self.data_track_not_ready_drops.load(Ordering::Relaxed),
            data_track_full_drops: self.data_track_full_drops.load(Ordering::Relaxed),
            oversized_drops: self.oversized_drops.load(Ordering::Relaxed),
            video_queue_drops: self.video_queue_drops.load(Ordering::Relaxed),
            point_cloud_queue_drops: self.point_cloud_queue_drops.load(Ordering::Relaxed),
            participant_resets: self.participant_resets.load(Ordering::Relaxed),
            ..GatewayStats::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_cumulative_counts_per_cause() {
        let counters = GatewayCounters::default();
        counters.record_data_track_not_ready_drop();
        counters.record_data_track_not_ready_drop();
        let _ = counters.record_data_track_full_drop();
        counters.record_oversized_drop();
        counters.record_oversized_drop();
        counters.record_oversized_drop();
        counters.record_video_queue_drops(4);
        counters.record_point_cloud_queue_drops(5);
        let _ = counters.record_participant_reset();

        let stats = counters.snapshot();
        assert_eq!(stats.data_track_not_ready_drops, 2);
        assert_eq!(stats.data_track_full_drops, 1);
        assert_eq!(stats.oversized_drops, 3);
        assert_eq!(stats.video_queue_drops, 4);
        assert_eq!(stats.point_cloud_queue_drops, 5);
        assert_eq!(stats.participant_resets, 1);
        assert!(!stats.session_active);
        assert_eq!(stats.viewer_count, 0);
        assert_eq!(stats.ice_rtt, None);
        assert_eq!(stats.connection_quality, None);
    }

    #[test]
    fn warned_causes_are_throttled_but_always_counted() {
        let counters = GatewayCounters::default();

        // The first event of each warned cause is due a warning; later ones within the
        // interval are counted silently.
        assert_eq!(counters.record_data_track_full_drop(), Some(1));
        assert_eq!(counters.record_data_track_full_drop(), None);
        assert_eq!(counters.record_data_track_full_drop(), None);
        assert_eq!(counters.record_participant_reset(), Some(1));
        assert_eq!(counters.record_participant_reset(), None);

        let stats = counters.snapshot();
        assert_eq!(stats.data_track_full_drops, 3);
        assert_eq!(stats.participant_resets, 2);
    }
}
