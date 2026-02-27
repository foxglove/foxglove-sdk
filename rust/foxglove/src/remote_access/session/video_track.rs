use bytes::Bytes;
use libwebrtc::prelude::*;
use libwebrtc::video_source::native::NativeVideoSource;
use tracing::{debug, error, warn};

use crate::RawChannel;
use crate::img2yuv::{ImageMessage, Yuv420Buffer};

/// The input schema type for a video-capable channel.
///
/// Each variant identifies which message format decoder to use for extracting image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoInputSchema {
    /// `foxglove.CompressedImage` with protobuf encoding.
    FoxgloveCompressedImage,
    /// `foxglove.RawImage` with protobuf encoding.
    FoxgloveRawImage,
    /// ROS 1 `sensor_msgs/CompressedImage` with ros1 encoding.
    #[cfg(feature = "img2yuv-ros1")]
    Ros1CompressedImage,
    /// ROS 1 `sensor_msgs/Image` with ros1 encoding.
    #[cfg(feature = "img2yuv-ros1")]
    Ros1Image,
    /// ROS 2 `sensor_msgs/msg/CompressedImage` with cdr encoding.
    #[cfg(feature = "img2yuv-ros2")]
    Ros2CompressedImage,
    /// ROS 2 `sensor_msgs/msg/Image` with cdr encoding.
    #[cfg(feature = "img2yuv-ros2")]
    Ros2Image,
}

/// Detect the video input schema from an (encoding, schema_name) pair.
///
/// Returns `Some(InputSchema)` if the channel carries an image type we can transcode to video.
pub fn detect_video_schema(encoding: &str, schema_name: &str) -> Option<VideoInputSchema> {
    match (encoding, schema_name) {
        ("protobuf", "foxglove.CompressedImage") => Some(VideoInputSchema::FoxgloveCompressedImage),
        ("protobuf", "foxglove.RawImage") => Some(VideoInputSchema::FoxgloveRawImage),
        #[cfg(feature = "img2yuv-ros1")]
        ("ros1", "sensor_msgs/CompressedImage") => Some(VideoInputSchema::Ros1CompressedImage),
        #[cfg(feature = "img2yuv-ros1")]
        ("ros1", "sensor_msgs/Image") => Some(VideoInputSchema::Ros1Image),
        #[cfg(feature = "img2yuv-ros2")]
        ("cdr", "sensor_msgs/msg/CompressedImage") => Some(VideoInputSchema::Ros2CompressedImage),
        #[cfg(feature = "img2yuv-ros2")]
        ("cdr", "sensor_msgs/msg/Image") => Some(VideoInputSchema::Ros2Image),
        _ => None,
    }
}

/// Convenience function to detect a video input schema from a [`RawChannel`].
pub fn get_video_input_schema(channel: &RawChannel) -> Option<VideoInputSchema> {
    let schema_name = channel.schema().map(|s| s.name.as_str()).unwrap_or("");
    detect_video_schema(channel.message_encoding(), schema_name)
}

/// Newtype wrapping [`I420Buffer`] that implements [`Yuv420Buffer`].
struct I420Yuv420(I420Buffer);

impl Yuv420Buffer for I420Yuv420 {
    fn dimensions(&self) -> (u32, u32) {
        (self.0.width(), self.0.height())
    }

    fn yuv(&self) -> (&[u8], &[u8], &[u8]) {
        self.0.data()
    }

    fn yuv_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        self.0.data_mut()
    }

    fn yuv_strides(&self) -> (u32, u32, u32) {
        self.0.strides()
    }
}

/// Error during video encoding.
#[derive(Debug, thiserror::Error)]
enum VideoEncodeError {
    #[error("failed to decode image message: {0}")]
    Decode(String),
    #[error("failed to convert image to YUV420: {0}")]
    YuvConversion(#[from] crate::img2yuv::Error),
}

/// Publishes video frames to a LiveKit video track.
///
/// Owns a bounded channel and a background processing task. Dropping the publisher
/// closes the channel, which terminates the task.
pub(crate) struct VideoPublisher {
    tx: flume::Sender<(Bytes, u64)>,
    rx: flume::Receiver<(Bytes, u64)>,
    #[allow(dead_code)]
    video_source: NativeVideoSource,
}

impl VideoPublisher {
    /// The bounded channel capacity for frame back-pressure.
    const CHANNEL_CAPACITY: usize = 2;

    /// Creates a new video publisher and spawns the background processing task.
    pub fn new(video_source: NativeVideoSource, input_schema: VideoInputSchema) -> Self {
        let (tx, rx) = flume::bounded::<(Bytes, u64)>(Self::CHANNEL_CAPACITY);
        let source = video_source.clone();
        let consumer_rx = rx.clone();
        tokio::spawn(async move {
            while let Ok((data, log_time_ns)) = consumer_rx.recv_async().await {
                let source = source.clone();
                let result = tokio::task::spawn_blocking(move || {
                    transcode_and_publish(input_schema, &source, &data, log_time_ns)
                })
                .await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        debug!("video encode error: {e}");
                    }
                    Err(e) => {
                        error!("video encode task panicked: {e}");
                    }
                }
            }
        });
        Self {
            tx,
            rx,
            video_source,
        }
    }

    /// Returns a reference to the underlying video source.
    #[allow(dead_code)]
    pub fn video_source(&self) -> &NativeVideoSource {
        &self.video_source
    }

    /// Send a frame for encoding. Non-blocking: if the channel is full, the oldest frame
    /// is dropped to make room (head-drop for minimal latency on live video).
    ///
    /// `log_time_ns` is the message log time in nanoseconds since epoch, forwarded to the
    /// video encoder as frame timestamp.
    pub fn send(&self, data: Bytes, log_time_ns: u64) {
        let msg = (data, log_time_ns);
        match self.tx.try_send(msg) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(msg)) => {
                let _ = self.rx.try_recv();
                let _ = self.tx.try_send(msg);
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                warn!("video publisher channel closed");
            }
        }
    }
}

/// Transcode the image message and publish it as a video frame.
///
/// This function decodes the original image data, encodes it as YUV 4:2:0, and then publishes it
/// to the video track.
fn transcode_and_publish(
    input_schema: VideoInputSchema,
    video_source: &NativeVideoSource,
    data: &[u8],
    log_time_ns: u64,
) -> Result<(), VideoEncodeError> {
    let image_msg = decode_image_message(input_schema, data)?;

    let (width, height) = image_msg
        .image
        .probe_dimensions()
        .map_err(VideoEncodeError::YuvConversion)?;

    // Ensure even dimensions for YUV 4:2:0
    let width = width & !1;
    let height = height & !1;
    if width == 0 || height == 0 {
        return Err(VideoEncodeError::YuvConversion(
            crate::img2yuv::Error::ZeroSized,
        ));
    }

    // Transcode to YUV 4:2:0.
    let mut buffer = I420Yuv420(I420Buffer::new(width, height));
    image_msg
        .image
        .to_yuv420(&mut buffer)
        .map_err(VideoEncodeError::YuvConversion)?;

    // Publish the transcoded image to the video track.
    let frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: (log_time_ns / 1000) as i64,
        buffer: buffer.0,
    };
    video_source.capture_frame(&frame);
    Ok(())
}

/// Decode raw message bytes into an [`ImageMessage`] based on the input schema.
fn decode_image_message<'a>(
    input_schema: VideoInputSchema,
    data: &'a [u8],
) -> Result<ImageMessage<'a>, VideoEncodeError> {
    match input_schema {
        VideoInputSchema::FoxgloveCompressedImage => {
            let msg = <crate::messages::CompressedImage as crate::Decode>::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
        VideoInputSchema::FoxgloveRawImage => {
            let msg = <crate::messages::RawImage as crate::Decode>::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
        #[cfg(feature = "img2yuv-ros1")]
        VideoInputSchema::Ros1CompressedImage => {
            let msg = crate::img2yuv::ros1::Ros1CompressedImage::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
        #[cfg(feature = "img2yuv-ros1")]
        VideoInputSchema::Ros1Image => {
            let msg = crate::img2yuv::ros1::Ros1Image::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
        #[cfg(feature = "img2yuv-ros2")]
        VideoInputSchema::Ros2CompressedImage => {
            let msg = crate::img2yuv::ros2::Ros2CompressedImage::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
        #[cfg(feature = "img2yuv-ros2")]
        VideoInputSchema::Ros2Image => {
            let msg = crate::img2yuv::ros2::Ros2Image::decode(data)
                .map_err(|e| VideoEncodeError::Decode(e.to_string()))?;
            ImageMessage::try_from(msg).map_err(|e| VideoEncodeError::Decode(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foxglove_compressed_image() {
        assert_eq!(
            detect_video_schema("protobuf", "foxglove.CompressedImage"),
            Some(VideoInputSchema::FoxgloveCompressedImage)
        );
    }

    #[test]
    fn test_foxglove_raw_image() {
        assert_eq!(
            detect_video_schema("protobuf", "foxglove.RawImage"),
            Some(VideoInputSchema::FoxgloveRawImage)
        );
    }

    #[cfg(feature = "img2yuv-ros1")]
    #[test]
    fn test_ros1_compressed_image() {
        assert_eq!(
            detect_video_schema("ros1", "sensor_msgs/CompressedImage"),
            Some(VideoInputSchema::Ros1CompressedImage)
        );
    }

    #[cfg(feature = "img2yuv-ros1")]
    #[test]
    fn test_ros1_image() {
        assert_eq!(
            detect_video_schema("ros1", "sensor_msgs/Image"),
            Some(VideoInputSchema::Ros1Image)
        );
    }

    #[cfg(feature = "img2yuv-ros2")]
    #[test]
    fn test_ros2_compressed_image() {
        assert_eq!(
            detect_video_schema("cdr", "sensor_msgs/msg/CompressedImage"),
            Some(VideoInputSchema::Ros2CompressedImage)
        );
    }

    #[cfg(feature = "img2yuv-ros2")]
    #[test]
    fn test_ros2_image() {
        assert_eq!(
            detect_video_schema("cdr", "sensor_msgs/msg/Image"),
            Some(VideoInputSchema::Ros2Image)
        );
    }

    #[test]
    fn test_unknown_schema() {
        assert_eq!(detect_video_schema("json", "SomeCustomType"), None);
        assert_eq!(detect_video_schema("protobuf", "foxglove.Pose"), None);
    }
}
