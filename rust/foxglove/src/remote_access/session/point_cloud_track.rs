//! Background point-cloud transcoding for remote access sessions.

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::Mutex;
use tokio::runtime::Handle;
use tracing::{error, warn};

use crate::ChannelId;
use crate::draco::CompressPointCloudOptions;
use crate::draco::transcode::transcode_point_cloud_message;
use crate::protocol::v2::server::Status;

use super::RemoteAccessSession;

/// Interval between throttled warnings for repeated point-cloud transcode failures.
const TRANSCODE_WARN_INTERVAL: Duration = Duration::from_secs(30);

/// Quiet period before clearing a channel's compression warning. Longer than the warn
/// throttle so an intermittently-failing topic doesn't flap between warning and cleared:
/// with the two equal, a topic failing at just over the warn interval would clear on a
/// success and republish on the next failure, at the throttle cadence, forever.
const TRANSCODE_RECOVERY_QUIET_PERIOD: Duration =
    Duration::from_secs(2 * TRANSCODE_WARN_INTERVAL.as_secs());

/// Stable per-channel id for the compression-failure warning, so a repeat replaces the
/// previous status in the app's problem list instead of stacking a new entry, and
/// recovery or teardown can remove it.
fn compression_status_id(channel_id: ChannelId) -> String {
    format!("point-cloud-compression-{}", u64::from(channel_id))
}

/// A per-channel [`WarningState`] shared across publisher generations.
///
/// The warning status id is per-channel, but publisher tasks come and go with the
/// subscriber count. Sharing the state means a publisher created for a resubscribe
/// inherits the live warning (and the throttle window) rather than starting blind, so
/// overlapping publishers can't fight over the channel's single status: a draining
/// predecessor won't retract a warning its successor owns, and the successor won't
/// silently re-publish.
pub(in crate::remote_access) type SharedWarningState = Arc<Mutex<WarningState>>;

/// The lifecycle of a channel's viewer-facing compression warning, with explicit
/// timestamps so the timing rules are unit-testable without sleeping.
///
/// Warnings are throttled to [`TRANSCODE_WARN_INTERVAL`]; a live warning is cleared —
/// with the throttle reset, so a fresh failure after genuine recovery reports
/// immediately — only once recovery has been sustained for
/// [`TRANSCODE_RECOVERY_QUIET_PERIOD`]. Clearing on the first success would strobe the
/// status (and the log) at message rate on a mixed good/bad stream, while clearing
/// without resetting the throttle would leave intermittent failures mostly invisible.
pub(in crate::remote_access) struct WarningState {
    /// Earliest instant the next throttled warning may be emitted; `None` emits
    /// immediately.
    next_warn_at: Option<Instant>,
    /// True while a warning status is live on viewers.
    active: bool,
    /// When the most recent transcode failure happened, gating removal.
    last_failure: Option<Instant>,
}

impl WarningState {
    pub(in crate::remote_access) const fn new() -> Self {
        Self {
            next_warn_at: None,
            active: false,
            last_failure: None,
        }
    }

    /// Acquires the shared warn throttle: returns true if a warning may be emitted at
    /// `now`, starting the next throttle window. Shared between transcode errors and
    /// task panics, so neither can flood the log.
    fn try_warn(&mut self, now: Instant) -> bool {
        if self.next_warn_at.is_some_and(|at| now < at) {
            return false;
        }
        self.next_warn_at = Some(now + TRANSCODE_WARN_INTERVAL);
        true
    }

    /// Records a transcode failure at `now`; returns true if a warning should be
    /// published (throttled).
    fn on_failure(&mut self, now: Instant) -> bool {
        self.last_failure = Some(now);
        self.try_warn(now)
    }

    /// Marks the warning status live on viewers, after it was actually published.
    fn published(&mut self) {
        self.active = true;
    }

    /// Records a successful transcode at `now`; returns true if the live warning should
    /// be removed because recovery has been sustained for the quiet period.
    fn on_success(&mut self, now: Instant) -> bool {
        if !self.active
            || self
                .last_failure
                .is_some_and(|at| now.duration_since(at) < TRANSCODE_RECOVERY_QUIET_PERIOD)
        {
            return false;
        }
        self.clear();
        true
    }

    /// Clears the warning and resets the throttle, so a channel with no remaining
    /// publisher (teardown) or genuine recovery starts clean and a fresh failure reports
    /// immediately.
    fn clear(&mut self) {
        self.active = false;
        self.next_warn_at = None;
    }

    /// True if a warning status is live on viewers.
    fn active(&self) -> bool {
        self.active
    }
}

/// Transcodes `foxglove.PointCloud` messages to Draco-compressed
/// `foxglove.CompressedPointCloud` messages off the logging hot path, and delivers them to
/// the session's subscribers over the regular data path.
///
/// Unlike video, the transcoded message stays on the same channel: the raw cloud is
/// replaced by the compressed payload, and the channel is advertised with the
/// `foxglove.CompressedPointCloud` schema.
///
/// Owns a bounded channel and a background processing task. Dropping the publisher closes
/// the channel, which terminates the task.
pub(crate) struct PointCloudPublisher {
    tx: flume::Sender<(Bytes, u64)>,
    rx: flume::Receiver<(Bytes, u64)>,
}

impl PointCloudPublisher {
    /// The bounded channel capacity for message back-pressure.
    const CHANNEL_CAPACITY: usize = 2;

    /// Creates a new publisher and spawns the background processing task.
    ///
    /// `topic` is the channel's topic, used to name the channel in viewer-facing
    /// compression warnings. `warning` is the channel's shared warning state, so this
    /// publisher inherits any warning a predecessor left live (see [`SharedWarningState`]).
    pub fn new(
        runtime: &Handle,
        session: Weak<RemoteAccessSession>,
        channel_id: ChannelId,
        topic: String,
        options: CompressPointCloudOptions,
        warning: SharedWarningState,
    ) -> Self {
        let (tx, rx) = flume::bounded::<(Bytes, u64)>(Self::CHANNEL_CAPACITY);
        let consumer_rx = rx.clone();
        runtime.spawn(async move {
            while let Ok((data, log_time)) = consumer_rx.recv_async().await {
                let result = tokio::task::spawn_blocking(move || {
                    transcode_point_cloud_message(&data, &options)
                })
                .await;
                match result {
                    Ok(Ok(transcoded)) => {
                        // Subscribers are re-resolved at delivery time, since they may have
                        // changed while the message was being transcoded.
                        let Some(session) = session.upgrade() else {
                            break;
                        };
                        if warning.lock().on_success(Instant::now()) {
                            session.remove_status(vec![compression_status_id(channel_id)]);
                        }
                        session.deliver_transcoded_point_cloud(channel_id, &transcoded, log_time);
                    }
                    Ok(Err(e)) => {
                        if warning.lock().on_failure(Instant::now()) {
                            let message =
                                format!("point cloud compression error on topic {topic}: {e}");
                            warn!("{message}");
                            if let Some(session) = session.upgrade() {
                                session.publish_status(
                                    Status::warning(message)
                                        .with_id(compression_status_id(channel_id)),
                                );
                                warning.lock().published();
                            }
                        }
                    }
                    Err(e) => {
                        if warning.lock().try_warn(Instant::now()) {
                            error!("point cloud compression task panicked: {e}");
                        }
                    }
                }
            }
            // The publisher is dropped when the channel loses its last data subscriber
            // (or the session shuts down): retract a live warning so it doesn't sit in
            // participants' problem lists with nothing left running to clear it. A
            // resubscribe can create a successor publisher while this task drains its
            // final in-flight encode, though; the successor shares this warning state and
            // owns the per-channel status id, so retract only when no publisher is
            // registered for the channel — and clear the shared state, so a future
            // publisher for this channel starts clean rather than believing a
            // now-removed warning is still live.
            if let Some(session) = session.upgrade()
                && session
                    .channel_registry
                    .read()
                    .get_point_cloud_publisher(&channel_id)
                    .is_none()
            {
                let mut warning = warning.lock();
                if warning.active() {
                    warning.clear();
                    drop(warning);
                    session.remove_status(vec![compression_status_id(channel_id)]);
                }
            }
        });
        Self { tx, rx }
    }

    /// Send a message for transcoding. Non-blocking: if the channel is full, the oldest
    /// message is dropped to make room (head-drop for minimal latency on live data).
    ///
    /// `log_time` is the message log time in nanoseconds since epoch.
    pub fn send(&self, data: Bytes, log_time: u64) {
        let msg = (data, log_time);
        match self.tx.try_send(msg) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(msg)) => {
                let _ = self.rx.try_recv();
                let _ = self.tx.try_send(msg);
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                warn!("point cloud publisher channel closed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shorthand for `t0 + secs`, keeping the timelines below readable.
    fn at(t0: Instant, secs: u64) -> Instant {
        t0 + Duration::from_secs(secs)
    }

    /// A failure at `secs` that publishes; panics if the publish was throttled.
    fn fail_and_publish(warning: &mut WarningState, t0: Instant, secs: u64) {
        assert!(
            warning.on_failure(at(t0, secs)),
            "failure at t0+{secs}s should publish"
        );
        warning.published();
    }

    #[test]
    fn first_failure_publishes_and_repeats_are_throttled() {
        let t0 = Instant::now();
        let mut warning = WarningState::new();

        fail_and_publish(&mut warning, t0, 0);
        // Repeats inside the throttle window are recorded but not re-published.
        assert!(!warning.on_failure(at(t0, 1)));
        assert!(!warning.on_failure(at(t0, 29)));
        assert!(warning.active());
        // The next window re-publishes (replacing the status under its stable id).
        assert!(warning.on_failure(at(t0, 30)));
    }

    #[test]
    fn mixed_stream_keeps_warning_up() {
        // The first regression this state machine guards: a stream interleaving good
        // and bad clouds must keep the warning up continuously — successes between
        // failures neither remove the status nor let failures go unreported.
        let t0 = Instant::now();
        let mut warning = WarningState::new();

        fail_and_publish(&mut warning, t0, 0);
        for secs in [1, 10, 29] {
            assert!(
                !warning.on_success(at(t0, secs)),
                "success at t0+{secs}s must not remove the warning"
            );
            assert!(warning.active());
        }
        assert!(warning.on_failure(at(t0, 31)), "still failing: re-publish");
        assert!(warning.active());
    }

    #[test]
    fn slow_failure_cadence_does_not_flap() {
        // The second regression: with the quiet period equal to the warn interval, a
        // topic failing at just over the interval flapped warning -> cleared -> warning
        // forever. Any failure cadence up to the quiet period must keep the status up.
        let t0 = Instant::now();
        let mut warning = WarningState::new();

        fail_and_publish(&mut warning, t0, 0);
        // 10 Hz successes, a failure every ~31s: no success may clear the warning.
        assert!(!warning.on_success(at(t0, 30)));
        fail_and_publish(&mut warning, t0, 31);
        assert!(!warning.on_success(at(t0, 61)));
        fail_and_publish(&mut warning, t0, 62);
        assert!(warning.active());
    }

    #[test]
    fn sustained_recovery_clears_and_re_failure_reports_immediately() {
        let t0 = Instant::now();
        let mut warning = WarningState::new();

        fail_and_publish(&mut warning, t0, 0);
        // Sustained recovery: the first success past the quiet period clears.
        assert!(!warning.on_success(at(t0, 59)));
        assert!(warning.on_success(at(t0, 60)));
        assert!(!warning.active());
        // Only one removal per recovery.
        assert!(!warning.on_success(at(t0, 61)));
        // A fresh failure after genuine recovery reports immediately: the throttle was
        // reset along with the removal.
        assert!(warning.on_failure(at(t0, 62)));
    }

    #[test]
    fn success_without_live_warning_removes_nothing() {
        let t0 = Instant::now();
        let mut warning = WarningState::new();
        // No failure ever happened.
        assert!(!warning.on_success(at(t0, 100)));

        // A failure whose warning never made it to viewers (the session was already
        // gone when the publish was attempted) leaves nothing to remove either.
        assert!(warning.on_failure(at(t0, 200)));
        assert!(!warning.on_success(at(t0, 300)));
        assert!(!warning.active());
    }

    #[test]
    fn panic_log_shares_the_throttle_window() {
        // Panics share the warn throttle with transcode errors, so neither floods the
        // log; a panic burst also delays the next status publish, and vice versa.
        let t0 = Instant::now();
        let mut warning = WarningState::new();

        assert!(warning.try_warn(at(t0, 0)));
        assert!(!warning.on_failure(at(t0, 10)), "inside the panic's window");
        assert!(!warning.try_warn(at(t0, 20)));
        assert!(warning.on_failure(at(t0, 30)));
    }
}
