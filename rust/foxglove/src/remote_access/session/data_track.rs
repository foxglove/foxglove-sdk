use std::sync::{Arc, OnceLock};
use std::time::Duration;

use livekit::prelude::{LocalDataTrack, LocalParticipant, PublishError};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::ChannelId;

/// Manages the lifecycle of a single published data track.
pub(crate) struct DataTrack {
    /// Shared cell where the publish task deposits the track on success.
    /// Read lock-free from the logging hot path.
    track: Arc<OnceLock<LocalDataTrack>>,
    /// Child token of the session cancellation token.
    /// Cancelled by [`close`](Self::close), or when the session shuts down.
    cancel: CancellationToken,
    /// Handle to the spawned publish task.
    task: JoinHandle<()>,
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
        topic: &str,
        cancel: CancellationToken,
    ) -> Self {
        let track = Arc::new(OnceLock::new());
        let track_clone = Arc::clone(&track);
        let cancel_clone = cancel.clone();
        let name = format!("data-ch-{}", u64::from(channel_id));
        let topic = topic.to_owned();

        let task = runtime.spawn(async move {
            const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
            const MAX_BACKOFF: Duration = Duration::from_secs(3);
            let mut backoff = INITIAL_BACKOFF;

            loop {
                match local_participant.publish_data_track(name.clone()).await {
                    Ok(published) => {
                        track_clone.set(published).ok();
                        return;
                    }
                    Err(PublishError::DuplicateName) => {
                        debug!(
                            "data track {name} ({topic}) still being unpublished at SFU, \
                             retrying in {backoff:?}"
                        );
                    }
                    Err(e) => {
                        error!(
                            "failed to publish data track {name} ({topic}): {e:?}, \
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
            task,
        }
    }

    /// Returns the underlying LiveKit track if publishing has completed successfully.
    pub fn get(&self) -> Option<&LocalDataTrack> {
        self.track.get()
    }

    /// Close the data track: cancel any in-flight publish, wait for it to settle,
    /// then unpublish the track if it was successfully published.
    pub async fn close(self) {
        self.cancel.cancel();
        _ = self.task.await;
        if let Some(track) = self.track.get() {
            track.unpublish();
        }
    }
}
