use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use livekit::prelude::{DataTrackFrame, LocalDataTrack, LocalParticipant, PublishError};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::{ChannelId, Metadata};

const FRAME_HEADER_SIZE: usize = 8; // u16 LE flags + u16 LE data_offset + u32 LE sequence

/// Manages the lifecycle of a single published data track.
pub(crate) struct DataTrack {
    /// Shared cell where the publish task deposits the track on success.
    /// Read lock-free from the logging hot path.
    track: Arc<OnceLock<LocalDataTrack>>,
    /// Child token of the session cancellation token.
    /// Cancelled by [`close`](Self::close), or when the session shuts down.
    cancel: CancellationToken,
    /// Handle to the spawned publish task.
    task: Option<JoinHandle<()>>,
    /// Per-track monotonic sequence number for packet loss detection.
    sequence: AtomicU32,
    /// Throttles debug log messages when data track messages are dropped.
    drop_throttler: parking_lot::Mutex<crate::throttler::Throttler>,
}

impl DataTrack {
    /// Spawn a task to publish a data track, retrying on errors until cancelled.
    ///
    /// The track is named `data-ch-{channel_id}`, which is unique within a session
    /// because channel IDs are never reused.
    pub fn publish(
        runtime: &Handle,
        local_participant: LocalParticipant,
        channel_id: ChannelId,
        cancel: CancellationToken,
    ) -> Self {
        let track = Arc::new(OnceLock::new());
        let track_clone = Arc::clone(&track);
        let cancel_clone = cancel.clone();
        let name = format!("data-ch-{}", u64::from(channel_id));

        let task = runtime.spawn(async move {
            const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
            const MAX_BACKOFF: Duration = Duration::from_secs(3);
            let mut backoff = INITIAL_BACKOFF;

            loop {
                let result = tokio::select! {
                    () = cancel_clone.cancelled() => return,
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
                    () = cancel_clone.cancelled() => return,
                    () = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        });

        Self {
            track,
            cancel,
            task: Some(task),
            sequence: AtomicU32::new(0),
            drop_throttler: parking_lot::Mutex::new(crate::throttler::Throttler::new(
                Duration::from_secs(30),
            )),
        }
    }

    /// Build a sequenced frame and push it to the data track.
    /// Drops with a throttled debug log if the track is not ready or full.
    pub fn log(&self, channel_id: ChannelId, msg: &[u8], metadata: &Metadata) {
        let Some(track) = self.track.get() else {
            if self.drop_throttler.lock().try_acquire() {
                debug!("data track not ready, dropping message for channel {channel_id:?}");
            }
            return;
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
        if let Err(e) = track.try_push(frame) {
            if self.drop_throttler.lock().try_acquire() {
                debug!("data track message dropped for channel {channel_id:?}: {e:?}");
            }
        }
    }

    /// Close the data track: cancel any in-flight publish, wait for it to settle,
    /// then unpublish the track if it was successfully published.
    pub async fn close(&mut self) {
        debug!("closing data track");
        self.cancel.cancel();
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
        self.cancel.cancel();
    }
}
