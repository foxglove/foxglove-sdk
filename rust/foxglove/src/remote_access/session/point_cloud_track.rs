//! Background point-cloud transcoding for remote access sessions.

use std::sync::Weak;
use std::time::Duration;

use bytes::Bytes;
use tokio::runtime::Handle;
use tracing::{error, warn};

use crate::ChannelId;
use crate::draco::CompressPointCloudOptions;
use crate::draco::transcode::transcode_point_cloud_message;
use crate::protocol::v2::server::Status;
use crate::throttler::Throttler;

use super::RemoteAccessSession;

/// Interval between throttled warnings for repeated point-cloud transcode failures.
const TRANSCODE_WARN_INTERVAL: Duration = Duration::from_secs(30);

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
    pub fn new(
        runtime: &Handle,
        session: Weak<RemoteAccessSession>,
        channel_id: ChannelId,
        options: CompressPointCloudOptions,
    ) -> Self {
        let (tx, rx) = flume::bounded::<(Bytes, u64)>(Self::CHANNEL_CAPACITY);
        let consumer_rx = rx.clone();
        runtime.spawn(async move {
            // Throttles compression warnings so a stream of bad clouds doesn't flood the log.
            let mut warn_throttler = Throttler::new(TRANSCODE_WARN_INTERVAL);
            while let Ok((data, log_time)) = consumer_rx.recv_async().await {
                let result = tokio::task::spawn_blocking(move || {
                    transcode_point_cloud_message(&data, &options)
                })
                .await;
                match result {
                    Ok(Ok(encoded)) => {
                        // Subscribers are re-resolved at delivery time, since they may have
                        // changed while the message was being transcoded.
                        let Some(session) = session.upgrade() else {
                            break;
                        };
                        session.deliver_transcoded_point_cloud(channel_id, &encoded, log_time);
                    }
                    Ok(Err(e)) => {
                        if warn_throttler.try_acquire() {
                            let message = format!(
                                "point cloud compression error on channel {channel_id:?}: {e}"
                            );
                            warn!("{message}");
                            if let Some(session) = session.upgrade() {
                                session.publish_status(Status::warning(message));
                            }
                        }
                    }
                    Err(e) => {
                        // Throttled like transcode errors: a panic that recurs on every
                        // message must not flood the log.
                        if warn_throttler.try_acquire() {
                            error!("point cloud compression task panicked: {e}");
                        }
                    }
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
