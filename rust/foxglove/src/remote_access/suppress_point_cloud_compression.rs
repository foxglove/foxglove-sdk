//! The point-cloud-compression opt-out predicate for remote access.

use crate::draco::CompressPointCloudOptions;
use crate::{ChannelDescriptor, RawChannel};

/// Decides, per channel, whether to opt out of point-cloud compression over remote access.
///
/// This callback is invoked when a compressible `foxglove.PointCloud` channel is registered.
/// Returning `true` advertises the channel with its original schema and delivers its messages
/// unchanged.
///
/// Configured via [`Gateway::suppress_point_cloud_compression`] (this trait) or
/// [`Gateway::suppress_point_cloud_compression_fn`] (a closure).
///
/// [`Gateway::suppress_point_cloud_compression`]: crate::remote_access::Gateway::suppress_point_cloud_compression
/// [`Gateway::suppress_point_cloud_compression_fn`]: crate::remote_access::Gateway::suppress_point_cloud_compression_fn
pub trait SuppressPointCloudCompression: Sync + Send {
    /// Returns `true` if the channel should be delivered without point-cloud compression.
    fn should_suppress(&self, channel: &ChannelDescriptor) -> bool;
}

pub(super) struct SuppressPointCloudCompressionFn<F>(pub(super) F)
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send;

impl<F> SuppressPointCloudCompression for SuppressPointCloudCompressionFn<F>
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send,
{
    fn should_suppress(&self, channel: &ChannelDescriptor) -> bool {
        self.0(channel)
    }
}

/// Returns the compression options for a channel, or `None` when compression is disabled, the
/// channel is not compressible, or the gateway's suppression predicate opts it out.
pub(super) fn resolve_point_cloud_compression(
    channel: &RawChannel,
    options: Option<CompressPointCloudOptions>,
    suppress: Option<&dyn SuppressPointCloudCompression>,
) -> Option<CompressPointCloudOptions> {
    let options = options?;
    if !crate::draco::transcode::is_point_cloud_channel(channel) {
        return None;
    }
    if suppress.is_some_and(|suppress| suppress.should_suppress(channel.descriptor())) {
        tracing::debug!(
            topic = %channel.topic(),
            "opted out of point-cloud compression; delivering unmodified"
        );
        return None;
    }
    Some(options)
}

#[cfg(test)]
mod tests {
    use super::{SuppressPointCloudCompressionFn, resolve_point_cloud_compression};
    use crate::draco::CompressPointCloudOptions;
    use crate::{ChannelBuilder, ChannelDescriptor, Context, Encode, RawChannel};
    use std::sync::Arc;

    fn make_channel(topic: &str) -> Arc<RawChannel> {
        let ctx = Context::new();
        ChannelBuilder::new(topic)
            .context(&ctx)
            .message_encoding("protobuf")
            .schema(<crate::messages::PointCloud as Encode>::get_schema().unwrap())
            .build_raw()
            .unwrap()
    }

    #[test]
    fn resolves_compression_per_channel() {
        let options = CompressPointCloudOptions::default();
        let cloud = make_channel("/cloud");
        let suppress =
            SuppressPointCloudCompressionFn(|ch: &ChannelDescriptor| ch.topic() == "/cloud");
        assert_eq!(
            resolve_point_cloud_compression(&cloud, Some(options), Some(&suppress)),
            None
        );

        let other =
            SuppressPointCloudCompressionFn(|ch: &ChannelDescriptor| ch.topic() == "/other");
        assert_eq!(
            resolve_point_cloud_compression(&cloud, Some(options), Some(&other)),
            Some(options)
        );
    }

    #[test]
    fn skips_predicate_when_compression_is_disabled() {
        let cloud = make_channel("/cloud");
        let suppress =
            SuppressPointCloudCompressionFn(|_: &ChannelDescriptor| panic!("unexpected callback"));
        assert_eq!(
            resolve_point_cloud_compression(&cloud, None, Some(&suppress)),
            None
        );
    }
}
