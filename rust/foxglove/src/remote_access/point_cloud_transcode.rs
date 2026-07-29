//! Transparent point-cloud transcoding for the remote-access sink.
//!
//! Detects channels carrying `foxglove.PointCloud` messages, rewrites their advertisement to
//! the `foxglove.CompressedPointCloud` schema, and transcodes individual messages using the
//! Draco mechanism in [`crate::draco`].

use bytes::Bytes;
use prost::Message as _;

use crate::draco::{DracoEncodeError, compress_point_cloud};
use crate::messages::{PointCloud, descriptors};
use crate::protocol::common::schema as protocol_schema;
use crate::protocol::common::server::advertise;
use crate::remote_access::CompressPointCloudOptions;
use crate::{Decode, RawChannel};

/// An error transcoding a logged `foxglove.PointCloud` message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TranscodeError {
    #[error("failed to decode PointCloud message: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    Encode(#[from] DracoEncodeError),
}

/// Returns true if the channel carries `foxglove.PointCloud` messages that the sink can
/// transcode.
pub(crate) fn is_point_cloud_channel(channel: &RawChannel) -> bool {
    channel.message_encoding() == "protobuf"
        && channel
            .schema()
            .is_some_and(|s| s.name == "foxglove.PointCloud")
}

/// Transcodes a serialized `foxglove.PointCloud` message into a serialized
/// `foxglove.CompressedPointCloud` message.
pub(crate) fn transcode_point_cloud_message(
    msg: &[u8],
    options: &CompressPointCloudOptions,
) -> Result<Bytes, TranscodeError> {
    let cloud = <PointCloud as Decode>::decode(msg)?;
    let compressed = compress_point_cloud(&cloud, &options.draco_options())?;
    Ok(Bytes::from(compressed.encode_to_vec()))
}

/// Rewrites a channel advertisement to report the `foxglove.CompressedPointCloud`
/// schema, replacing the original `foxglove.PointCloud` schema.
///
/// The channel id, topic, encoding, and metadata are unchanged.
pub(crate) fn rewrite_advertisement(channel: &mut advertise::Channel<'_>) {
    let schema_data = protocol_schema::encode_schema_data(
        "protobuf",
        std::borrow::Cow::Borrowed(descriptors::COMPRESSED_POINT_CLOUD),
    )
    .expect("binary schema encoding is infallible");
    channel.schema_name = "foxglove.CompressedPointCloud".into();
    channel.schema_encoding = Some("protobuf".into());
    channel.schema = std::borrow::Cow::Owned(schema_data.into_owned());
}

#[cfg(test)]
mod tests {
    use super::{TranscodeError, is_point_cloud_channel, transcode_point_cloud_message};
    use crate::draco::DracoEncodeError;
    use crate::messages::{PackedElementField, PointCloud, packed_element_field::NumericType};
    use crate::remote_access::CompressPointCloudOptions;
    use crate::{ChannelBuilder, Context, Encode, RawChannel, Schema};
    use std::sync::Arc;

    fn make_channel(encoding: &str, schema: Option<Schema>) -> Arc<RawChannel> {
        let ctx = Context::new();
        let mut builder = ChannelBuilder::new("/topic")
            .context(&ctx)
            .message_encoding(encoding);
        if let Some(schema) = schema {
            builder = builder.schema(schema);
        }
        builder.build_raw().unwrap()
    }

    #[test]
    fn test_detects_protobuf_point_cloud() {
        let ch = make_channel("protobuf", <PointCloud as Encode>::get_schema());
        assert!(is_point_cloud_channel(&ch));
    }

    #[test]
    fn test_transcode_rejects_float64_fields() {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let make = |with_f64: bool, points: usize| {
            let stride = if with_f64 { 20 } else { 16 };
            let mut fields = vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
            ];
            if with_f64 {
                fields.push(field("stamp", 12, NumericType::Float64));
            } else {
                fields.push(field("intensity", 12, NumericType::Float32));
            }
            let mut data = Vec::new();
            for i in 0..points {
                for v in [i as f32, i as f32 * 2.0, 0.5] {
                    data.extend_from_slice(&v.to_le_bytes());
                }
                if with_f64 {
                    data.extend_from_slice(&(i as f64).to_le_bytes());
                } else {
                    data.extend_from_slice(&1.0f32.to_le_bytes());
                }
            }
            let cloud = PointCloud {
                timestamp: None,
                frame_id: "t".to_string(),
                pose: None,
                point_stride: stride,
                fields,
                data: data.into(),
            };
            let mut buf = Vec::new();
            cloud.encode(&mut buf).unwrap();
            buf
        };
        let options = CompressPointCloudOptions::default();

        // A float64 field with quantization configured is rejected, naming the field.
        let err = transcode_point_cloud_message(&make(true, 8), &options).unwrap_err();
        assert!(matches!(
            err,
            TranscodeError::Encode(DracoEncodeError::UnquantizableField { ref name }) if name == "stamp"
        ));

        // No float64 field: transcodes fine.
        transcode_point_cloud_message(&make(false, 8), &options).unwrap();

        // Empty clouds fold to lossless regardless of fields and must round-trip.
        transcode_point_cloud_message(&make(true, 0), &options).unwrap();
    }

    #[test]
    fn test_ignores_other_channels() {
        // Wrong schema.
        let ch = make_channel(
            "protobuf",
            <crate::messages::CompressedPointCloud as Encode>::get_schema(),
        );
        assert!(!is_point_cloud_channel(&ch));

        // Wrong encoding.
        let ch = make_channel(
            "json",
            Some(Schema::new("foxglove.PointCloud", "jsonschema", b"{}")),
        );
        assert!(!is_point_cloud_channel(&ch));

        // No schema.
        let ch = make_channel("json", None);
        assert!(!is_point_cloud_channel(&ch));
    }
}
