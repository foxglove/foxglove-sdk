use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use livekit::prelude::{DataTrackFrame, LocalDataTrack, LocalParticipant, PublishError};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::{ChannelId, Metadata};

const FRAME_HEADER_SIZE: usize = 8; // u16 LE flags + u16 LE data_offset + u32 LE sequence

/// Minimum interval between oversized-drop warnings for a track.
///
/// Drops between warnings are counted and folded into the next report.
pub(super) const OVERSIZED_WARN_INTERVAL: Duration = Duration::from_secs(30);

/// Details of a throttled oversized-message drop, surfaced to viewers as a
/// `Status` warning.
///
/// Returned by [`DataTrack::log`] only when the gateway-side warning fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OversizedDropReport {
    /// Oversized messages dropped since the last report, including this one.
    pub dropped_since_last: u64,
    /// The configured per-message size limit, in bytes.
    pub size_limit: usize,
}

/// Manages the lifecycle of a single published data track.
pub(crate) struct DataTrack {
    /// Shared cell where the publish task deposits the track on success.
    /// Read lock-free from the logging hot path.
    track: Arc<OnceLock<LocalDataTrack>>,
    /// Cancelled by [`close`](Self::close) or on drop. Stops retry attempts but
    /// does not interrupt an in-flight `publish_data_track` call.
    close: CancellationToken,
    /// Handle to the spawned publish task.
    task: Option<JoinHandle<()>>,
    /// Per-track monotonic sequence number for packet loss detection.
    sequence: AtomicU32,
    /// Throttles debug log messages when data track messages are dropped.
    drop_throttler: parking_lot::Mutex<crate::throttler::Throttler>,
    /// Per-message size limit in bytes; messages larger than this are dropped
    /// before publish. See `DEFAULT_MAX_DATA_TRACK_MESSAGE_SIZE` for the rationale.
    max_message_size: usize,
    /// Oversized messages dropped since the last warning was logged; reset to
    /// zero each time the throttled warning fires.
    oversized_dropped: AtomicU64,
    /// Throttles warnings emitted when oversized messages are dropped.
    oversized_throttler: parking_lot::Mutex<crate::throttler::Throttler>,
}

impl DataTrack {
    /// Spawn a task to publish a data track, retrying on errors until cancelled.
    ///
    /// The track is named `data-ch-{channel_id}`, which is unique within a session
    /// because channel IDs are never reused.
    ///
    /// `session_cancel` is the session-level cancellation token; it hard-cancels
    /// in-flight `publish_data_track` calls during session teardown. The per-track
    /// [`close`](Self::close) token only stops retry attempts between calls, so a
    /// normal channel removal never yanks a publish out from under the SFU.
    pub fn publish(
        runtime: &Handle,
        local_participant: LocalParticipant,
        channel_id: ChannelId,
        session_cancel: CancellationToken,
        max_message_size: usize,
    ) -> Self {
        let track = Arc::new(OnceLock::new());
        let track_clone = Arc::clone(&track);
        let close = CancellationToken::new();
        let close_clone = close.clone();
        let name = format!("data-ch-{}", u64::from(channel_id));

        let task = runtime.spawn(async move {
            const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
            const MAX_BACKOFF: Duration = Duration::from_secs(3);
            let mut backoff = INITIAL_BACKOFF;

            loop {
                if close_clone.is_cancelled() {
                    return;
                }
                let result = tokio::select! {
                    () = session_cancel.cancelled() => return,
                    result = local_participant.publish_data_track(name.clone()) => result,
                };
                match result {
                    Ok(published) => {
                        track_clone.set(published).ok();
                        debug!("data track {name} published");
                        return;
                    }
                    Err(PublishError::DuplicateName) => {
                        debug!(
                            "data track {name} still being unpublished at SFU, \
                             retrying in {backoff:?}"
                        );
                    }
                    Err(e) => {
                        error!(
                            "failed to publish data track {name}: {e:?}, \
                             retrying in {backoff:?}"
                        );
                    }
                }
                tokio::select! {
                    () = close_clone.cancelled() => return,
                    () = session_cancel.cancelled() => return,
                    () = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        });

        Self {
            track,
            close,
            task: Some(task),
            sequence: AtomicU32::new(0),
            drop_throttler: parking_lot::Mutex::new(crate::throttler::Throttler::new(
                Duration::from_secs(30),
            )),
            max_message_size,
            oversized_dropped: AtomicU64::new(0),
            oversized_throttler: parking_lot::Mutex::new(crate::throttler::Throttler::new(
                OVERSIZED_WARN_INTERVAL,
            )),
        }
    }

    /// Build a sequenced frame and push it to the data track.
    ///
    /// Drops the message (with a throttled warning) if it exceeds
    /// [`max_message_size`](Self::max_message_size), or with a throttled debug
    /// log if the track is not ready or full.
    ///
    /// Returns `Err(OversizedDropReport)` only when an oversized-drop warning
    /// fires. All other delivered or dropped messages return `Ok(())`.
    pub fn log(
        &self,
        channel_id: ChannelId,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), OversizedDropReport> {
        if msg.len() > self.max_message_size {
            if self.oversized_throttler.lock().try_acquire() {
                let dropped = 1 + self.oversized_dropped.swap(0, Ordering::Relaxed);
                warn!(
                    "dropping {}-byte message on channel {channel_id:?}: exceeds \
                     data-track limit of {} bytes ({dropped} dropped since last warning)",
                    msg.len(),
                    self.max_message_size
                );
                return Err(OversizedDropReport {
                    dropped_since_last: dropped,
                    size_limit: self.max_message_size,
                });
            }
            self.oversized_dropped.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        let Some(track) = self.track.get() else {
            if self.drop_throttler.lock().try_acquire() {
                debug!("data track not ready, dropping message for channel {channel_id:?}");
            }
            return Ok(());
        };
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed);
        let mut payload = Vec::with_capacity(FRAME_HEADER_SIZE + msg.len());
        // flags is 0, it's reserved for future use
        payload.extend_from_slice(&0u16.to_le_bytes());
        // data_offset is always FRAME_HEADER_SIZE, but this is part of the frame
        // so that we can more easily add fields here without breaking old clients.
        payload.extend_from_slice(&(FRAME_HEADER_SIZE as u16).to_le_bytes()); // data_offset
        payload.extend_from_slice(&seq.to_le_bytes());
        payload.extend_from_slice(msg);
        let frame = DataTrackFrame::new(payload).with_user_timestamp(metadata.log_time);
        if let Err(e) = track.try_push(frame)
            && self.drop_throttler.lock().try_acquire()
        {
            debug!("data track message dropped for channel {channel_id:?}: {e:?}");
        }
        Ok(())
    }

    /// Oversized messages dropped since the last warning was logged (test-only).
    #[cfg(test)]
    pub fn oversized_dropped(&self) -> u64 {
        self.oversized_dropped.load(Ordering::Relaxed)
    }

    /// Close the data track: stop retrying, wait for any in-flight publish to
    /// complete, then unpublish the track if it was successfully published.
    pub async fn close(&mut self) {
        self.close.cancel();
        if let Some(task) = self.task.take() {
            _ = task.await;
        }
        if let Some(track) = self.track.get() {
            debug!("unpublishing data track {}", track.info().name());
            track.unpublish();
        }
    }
}

impl Drop for DataTrack {
    fn drop(&mut self) {
        self.close.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl DataTrack {
        /// Builds a `DataTrack` with no live LiveKit track, so the size gate in
        /// [`log`](Self::log) can be exercised without a room or participant.
        fn for_test(max_message_size: usize) -> Self {
            Self {
                track: Arc::new(OnceLock::new()),
                close: CancellationToken::new(),
                task: None,
                sequence: AtomicU32::new(0),
                drop_throttler: parking_lot::Mutex::new(crate::throttler::Throttler::new(
                    Duration::from_secs(30),
                )),
                max_message_size,
                oversized_dropped: AtomicU64::new(0),
                oversized_throttler: parking_lot::Mutex::new(crate::throttler::Throttler::new(
                    OVERSIZED_WARN_INTERVAL,
                )),
            }
        }
    }

    #[test]
    fn drops_oversized_message_and_counts_it() {
        let track = DataTrack::for_test(16);
        let channel_id = ChannelId::new(1);
        let metadata = Metadata::default();

        // The first oversized message fires the throttled warning, which resets
        // the pending counter and returns a report for the viewer signal.
        let report = track.log(channel_id, &[0u8; 17], &metadata);
        assert_eq!(
            report,
            Err(OversizedDropReport {
                dropped_since_last: 1,
                size_limit: 16,
            })
        );

        // A second oversized message is throttled: no report, one pending drop.
        assert_eq!(track.log(channel_id, &[0u8; 17], &metadata), Ok(()));
        assert_eq!(track.oversized_dropped(), 1);

        // An at-limit message takes the normal path: no report, not counted.
        assert_eq!(track.log(channel_id, &[0u8; 16], &metadata), Ok(()));
        assert_eq!(track.oversized_dropped(), 1);
    }
}
