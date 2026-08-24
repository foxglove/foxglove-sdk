//! The per-channel point-cloud compression policy for remote access.

use crate::draco::DracoEncodeOptions;
use crate::remote_access::qos::Reliability;
use crate::{ChannelDescriptor, RawChannel};

/// Selects the compression algorithm and settings applied to a point cloud.
///
/// [`Draco`](Self::Draco) is currently the only algorithm; the enum is non-exhaustive so
/// others can be added without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointCloudCompression {
    /// Compress with [Google Draco](https://google.github.io/draco/).
    Draco(DracoEncodeOptions),
}

impl PointCloudCompression {
    pub(crate) fn draco_options(&self) -> DracoEncodeOptions {
        match self {
            PointCloudCompression::Draco(options) => *options,
        }
    }
}

impl Default for PointCloudCompression {
    fn default() -> Self {
        Self::Draco(DracoEncodeOptions::default())
    }
}

/// Resolved per-channel compression configuration: the input format to decode and the Draco
/// settings to encode with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointCloudCompressionConfig {
    /// The message format of the channel's input.
    pub(crate) input_schema: crate::remote_access::point_cloud_transcode::PointCloudInputSchema,
    /// The Draco encoding settings.
    pub(crate) options: PointCloudCompression,
}

/// Selects, per channel, the point-cloud compression applied over remote access.
///
/// This callback is invoked when a compressible point-cloud channel — one carrying a
/// supported input schema, currently protobuf-, JSON-, or FlatBuffer-encoded
/// `foxglove.PointCloud`, or CDR-encoded `sensor_msgs/msg/PointCloud2` — is registered with Lossy QoS. Returning
/// `Some(options)` compresses the channel's messages with those
/// settings; returning `None` advertises the channel with its original schema and delivers
/// its messages unchanged. Return `None` unconditionally to disable point-cloud
/// compression entirely.
///
/// When no policy is configured, every compressible Lossy channel is compressed with
/// [`PointCloudCompression::default()`].
///
/// Channels classified as [`Reliability::Reliable`] are always delivered unmodified on the
/// control stream (compression is skipped automatically), so this callback is not consulted
/// for them.
///
/// Configured via [`Gateway::point_cloud_compression`] (this trait) or
/// [`Gateway::point_cloud_compression_fn`] (a closure).
///
/// [`Gateway::point_cloud_compression`]: crate::remote_access::Gateway::point_cloud_compression
/// [`Gateway::point_cloud_compression_fn`]: crate::remote_access::Gateway::point_cloud_compression_fn
pub trait PointCloudCompressionPolicy: Sync + Send {
    /// Returns the compression settings for the channel, or `None` to deliver it without
    /// point-cloud compression.
    fn compression(&self, channel: &ChannelDescriptor) -> Option<PointCloudCompression>;
}

pub(super) struct PointCloudCompressionPolicyFn<F>(pub(super) F)
where
    F: Fn(&ChannelDescriptor) -> Option<PointCloudCompression> + Sync + Send;

impl<F> PointCloudCompressionPolicy for PointCloudCompressionPolicyFn<F>
where
    F: Fn(&ChannelDescriptor) -> Option<PointCloudCompression> + Sync + Send,
{
    fn compression(&self, channel: &ChannelDescriptor) -> Option<PointCloudCompression> {
        self.0(channel)
    }
}

/// Returns the compression configuration for a channel, or `None` when the channel is
/// not compressible, QoS is Reliable, or the gateway's compression policy opts it out.
///
/// Reliable channels keep the raw point cloud on the control bytestream: compression is
/// lossy (and the publisher may drop when behind), which would violate the Reliable
/// contract.
///
/// Options are valid by construction, but lossless options provide no size reduction over
/// the raw cloud, so a channel whose policy returns them is delivered unmodified, with a
/// warning.
pub(super) fn resolve_point_cloud_compression(
    channel: &RawChannel,
    policy: Option<&dyn PointCloudCompressionPolicy>,
    reliability: Reliability,
) -> Option<PointCloudCompressionConfig> {
    let input_schema =
        crate::remote_access::point_cloud_transcode::point_cloud_input_schema(channel)?;
    if reliability == Reliability::Reliable {
        tracing::debug!(
            topic = %channel.topic(),
            "skipping point-cloud compression for Reliable channel; delivering unmodified"
        );
        return None;
    }
    let options = match policy {
        Some(policy) => match policy.compression(channel.descriptor()) {
            Some(options) => options,
            None => {
                tracing::debug!(
                    topic = %channel.topic(),
                    "opted out of point-cloud compression; delivering unmodified"
                );
                return None;
            }
        },
        None => PointCloudCompression::default(),
    };
    if options.draco_options().is_lossless() {
        tracing::warn!(
            topic = %channel.topic(),
            "lossless point-cloud compression provides no size reduction over the raw \
             cloud; delivering unmodified"
        );
        return None;
    }
    Some(PointCloudCompressionConfig {
        input_schema,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        PointCloudCompression, PointCloudCompressionPolicyFn, resolve_point_cloud_compression,
    };
    use crate::draco::DracoEncodeOptions;
    use crate::remote_access::qos::Reliability;
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

    fn options_with_bits(quantization_bits: u8) -> PointCloudCompression {
        PointCloudCompression::Draco(
            DracoEncodeOptions::with_quantization_bits(quantization_bits).unwrap(),
        )
    }

    #[test]
    fn compresses_with_defaults_when_no_policy_is_set() {
        let cloud = make_channel("/cloud");
        let config = resolve_point_cloud_compression(&cloud, None, Reliability::Lossy)
            .expect("channel should resolve");
        assert_eq!(config.options, PointCloudCompression::default());
        assert_eq!(
            config.input_schema,
            crate::remote_access::point_cloud_transcode::PointCloudInputSchema::FoxgloveProtobuf
        );
    }

    #[test]
    fn resolves_compression_per_channel() {
        let cloud = make_channel("/cloud");

        // The policy opts this channel out.
        let opt_out = PointCloudCompressionPolicyFn(|ch: &ChannelDescriptor| {
            (ch.topic() != "/cloud").then(PointCloudCompression::default)
        });
        assert_eq!(
            resolve_point_cloud_compression(&cloud, Some(&opt_out), Reliability::Lossy),
            None
        );

        // The policy selects per-channel options.
        let tuned = PointCloudCompressionPolicyFn(|ch: &ChannelDescriptor| {
            (ch.topic() == "/cloud").then(|| options_with_bits(10))
        });
        let config = resolve_point_cloud_compression(&cloud, Some(&tuned), Reliability::Lossy)
            .expect("channel should resolve");
        assert_eq!(config.options, options_with_bits(10));
        assert_eq!(
            config.input_schema,
            crate::remote_access::point_cloud_transcode::PointCloudInputSchema::FoxgloveProtobuf
        );
    }

    #[test]
    fn skips_compression_when_qos_is_reliable() {
        let cloud = make_channel("/cloud");
        let policy = PointCloudCompressionPolicyFn(
            |_: &ChannelDescriptor| -> Option<PointCloudCompression> {
                panic!("unexpected callback")
            },
        );
        assert_eq!(
            resolve_point_cloud_compression(&cloud, Some(&policy), Reliability::Reliable),
            None
        );
    }

    #[test]
    fn skips_compression_for_lossless_options() {
        // Lossless Draco provides no size reduction, so on the transparent path it is
        // pure overhead; deliver the raw cloud instead. (Out-of-range options are
        // unrepresentable: DracoEncodeOptions validates at construction.)
        let cloud = make_channel("/cloud");
        let policy = PointCloudCompressionPolicyFn(|_: &ChannelDescriptor| {
            Some(PointCloudCompression::Draco(DracoEncodeOptions::lossless()))
        });
        assert_eq!(
            resolve_point_cloud_compression(&cloud, Some(&policy), Reliability::Lossy),
            None
        );
    }
}
