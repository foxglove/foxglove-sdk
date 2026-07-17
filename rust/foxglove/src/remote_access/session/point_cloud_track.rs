//! Background point-cloud transcoding for remote access sessions.

use std::sync::Weak;

use bytes::Bytes;
use tokio::runtime::Handle;
use tracing::warn;

use crate::ChannelId;
use crate::remote_access::point_cloud_compression::PointCloudCompressionConfig;
use crate::remote_access::point_cloud_transcode::transcode_point_cloud_message;

use super::RemoteAccessSession;

/// Transcodes `foxglove.PointCloud` messages to Draco-compressed
/// `foxglove.CompressedPointCloud` messages off the logging hot path, and delivers them to
/// the session's subscribers over the regular data path.
///
/// Unlike video, the transcoded message stays on the same channel: the raw cloud is
/// replaced by the compressed payload, and the channel is advertised with the
/// `foxglove.CompressedPointCloud` schema.
///
/// Owns a bounded channel and a background processing task. Dropping the publisher closes
/// the channel, which terminates the task. The task holds no warning state of its own: a
/// transcode failure is reported to the session, which owns the per-channel compression
/// warning (see [`RemoteAccessSession::report_compression_failure`]).
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
    /// compression warnings.
    pub fn new(
        runtime: &Handle,
        session: Weak<RemoteAccessSession>,
        channel_id: ChannelId,
        topic: String,
        config: PointCloudCompressionConfig,
    ) -> Self {
        let (tx, rx) = flume::bounded::<(Bytes, u64)>(Self::CHANNEL_CAPACITY);
        let consumer_rx = rx.clone();
        runtime.spawn(async move {
            while let Ok((data, log_time)) = consumer_rx.recv_async().await {
                let result = tokio::task::spawn_blocking(move || {
                    transcode_point_cloud_message(&data, config.input_schema, &config.options)
                })
                .await;
                let Some(session) = session.upgrade() else {
                    break;
                };
                match result {
                    Ok(Ok(transcoded)) => {
                        // Subscribers are re-resolved at delivery time, since they may have
                        // changed while the message was being transcoded.
                        session.deliver_transcoded_point_cloud(channel_id, &transcoded, log_time);
                    }
                    Ok(Err(e)) => {
                        // The session owns the per-channel warning: it throttles the
                        // report, publishes to current subscribers, and clears it once
                        // failures stop (or the channel is unadvertised).
                        session.report_compression_failure(
                            channel_id,
                            format!("point cloud compression error on topic {topic}: {e}"),
                        );
                    }
                    Err(_) => {
                        // A panic in the encode task is an internal bug; the panic itself
                        // is already logged by the default panic hook. From the channel's
                        // perspective it is just another failure to compress, so surface a
                        // warning through the same throttled path rather than leaking the
                        // raw panic text to viewers. Unlike the data-driven failures
                        // above, this one is not actionable by the user, so mark it an
                        // "internal error" so they don't go auditing their cloud's schema.
                        session.report_compression_failure(
                            channel_id,
                            format!(
                                "point cloud compression error on topic {topic}: internal error"
                            ),
                        );
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
