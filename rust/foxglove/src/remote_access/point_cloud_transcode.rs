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
    let mut cloud = <PointCloud as Decode>::decode(msg)?;
    reinterpret_packed_color_fields(&mut cloud);
    drop_non_finite_points(&mut cloud);
    let compressed = compress_point_cloud(&cloud, &options.draco_options())?;
    Ok(Bytes::from(compressed.encode_to_vec()))
}

/// Retypes packed-color fields declared float32 to uint32 so they survive quantization.
///
/// PCL's `PointXYZRGB` convention declares the `rgb`/`rgba` field float32 while packing
/// `(a << 24) | (r << 16) | (g << 8) | b` into the bits — integer data masquerading as
/// denormal floats. The kd-tree encoder quantizes every float32 attribute, and quantizing
/// a range of denormals collapses nearly every color in the cloud to a single wrong
/// value; integer attributes are copied losslessly instead, so flipping the declared type
/// preserves the packed bits exactly. Only the declared type changes — both types are
/// four bytes, so offsets, stride, and data are untouched — and the uint32 declaration is
/// itself a common `PointCloud2` convention: the app already force-reads `rgb`/`rgba`
/// color fields as uint32 whatever the declared type.
///
/// Name-matching is a heuristic, like the encoder's `x`/`y`/`z` handling: a field named
/// `rgb` carrying a genuine continuous float would be retyped needlessly (still lossless,
/// merely un-quantized), which is strictly safer than the certain mangling of packed
/// colors today.
fn reinterpret_packed_color_fields(cloud: &mut PointCloud) {
    use crate::messages::packed_element_field::NumericType;

    for field in &mut cloud.fields {
        if matches!(field.name.as_str(), "rgb" | "rgba")
            && field.r#type == NumericType::Float32 as i32
        {
            field.r#type = NumericType::Uint32 as i32;
        }
    }
}

/// Removes points whose position contains a non-finite (NaN or infinite) coordinate.
///
/// Publishers commonly pad invalid returns with NaN — RGBD cameras and rotating lidars
/// mark non-returns this way — but the Draco quantizer derives its range from the
/// position min/max and errors on any non-finite coordinate, which would fail (and drop)
/// the whole cloud. Dropping just the invalid points delivers the valid ones instead; a
/// non-finite position is unrenderable, so nothing a viewer uses is lost. Coordinates are
/// judged after the same f64-to-f32 narrowing the encoder applies, so a float64
/// coordinate that only overflows f32 is dropped too.
///
/// Layout problems (zero or misaligned stride, coordinates past the stride) are left for
/// the encoder, which reports them precisely; this pass only filters clouds it can read.
fn drop_non_finite_points(cloud: &mut PointCloud) {
    use crate::messages::packed_element_field::NumericType;

    let stride = cloud.point_stride as usize;
    if stride == 0 || !cloud.data.len().is_multiple_of(stride) {
        return;
    }
    // Only float32/float64 x/y/z coordinates can be non-finite; integer coordinates (and
    // non-position fields, which Draco carries verbatim) never poison the quantizer.
    let coords: Vec<(usize, bool)> = cloud
        .fields
        .iter()
        .filter(|f| matches!(f.name.as_str(), "x" | "y" | "z"))
        .filter_map(|f| {
            let offset = f.offset as usize;
            if f.r#type == NumericType::Float32 as i32 && offset + 4 <= stride {
                Some((offset, false))
            } else if f.r#type == NumericType::Float64 as i32 && offset + 8 <= stride {
                Some((offset, true))
            } else {
                None
            }
        })
        .collect();
    if coords.is_empty() {
        return;
    }

    let finite = |point: &[u8]| {
        coords.iter().all(|&(offset, is_f64)| {
            let v = if is_f64 {
                f64::from_le_bytes(point[offset..offset + 8].try_into().unwrap()) as f32
            } else {
                f32::from_le_bytes(point[offset..offset + 4].try_into().unwrap())
            };
            v.is_finite()
        })
    };

    // The common case is a fully finite cloud; detect it with a scan so it passes through
    // without copying.
    if cloud.data.chunks_exact(stride).all(finite) {
        return;
    }
    let mut data = Vec::with_capacity(cloud.data.len());
    for point in cloud.data.chunks_exact(stride) {
        if finite(point) {
            data.extend_from_slice(point);
        }
    }
    tracing::debug!(
        dropped = (cloud.data.len() - data.len()) / stride,
        "dropped points with non-finite positions before compression"
    );
    cloud.data = data.into();
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
    use super::{
        TranscodeError, drop_non_finite_points, is_point_cloud_channel,
        transcode_point_cloud_message,
    };
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

    /// A float32 xyz cloud from raw points.
    fn xyz_cloud(points: &[[f32; 3]]) -> PointCloud {
        let field = |name: &str, offset: u32| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: NumericType::Float32 as i32,
        };
        let mut data = Vec::new();
        for point in points {
            for c in point {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 12,
            fields: vec![field("x", 0), field("y", 4), field("z", 8)],
            data: data.into(),
        }
    }

    #[test]
    fn test_drops_non_finite_points() {
        // NaN-padded invalid returns (`is_dense == false` in ROS terms) are the common
        // case for organized clouds; the quantizer rejects non-finite coordinates, so the
        // invalid points must be dropped rather than failing the whole cloud.
        let mut cloud = xyz_cloud(&[
            [1.0, 2.0, 3.0],
            [f32::NAN, f32::NAN, f32::NAN],
            [4.0, 5.0, 6.0],
            [f32::INFINITY, 5.0, 6.0],
        ]);
        drop_non_finite_points(&mut cloud);
        assert_eq!(
            cloud.data,
            xyz_cloud(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]).data
        );

        // A fully finite cloud passes through untouched.
        let mut cloud = xyz_cloud(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let before = cloud.data.clone();
        drop_non_finite_points(&mut cloud);
        assert_eq!(cloud.data, before);
    }

    #[test]
    fn test_drops_f64_positions_that_overflow_f32() {
        // The encoder narrows float64 coordinates to f32; a value that is finite as f64
        // but overflows f32 becomes infinite after narrowing, so it must be dropped by
        // the same rule.
        let field = |name: &str, offset: u32| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: NumericType::Float64 as i32,
        };
        let mut data = Vec::new();
        for point in [[1.0f64, 2.0, 3.0], [1e300, 5.0, 6.0]] {
            for c in point {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 24,
            fields: vec![field("x", 0), field("y", 8), field("z", 16)],
            data: data.into(),
        };
        drop_non_finite_points(&mut cloud);
        assert_eq!(cloud.data.len(), 24);
        assert_eq!(
            f64::from_le_bytes(cloud.data[0..8].try_into().unwrap()),
            1.0
        );
    }

    #[test]
    fn test_reinterprets_packed_color_fields() {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let mut cloud = xyz_cloud(&[[1.0, 2.0, 3.0]]);
        cloud.point_stride = 32;
        cloud.fields.extend([
            field("rgb", 12, NumericType::Float32),
            field("rgba", 16, NumericType::Float32),
            // Untouched: a continuous float, and color fields already integer-typed.
            field("intensity", 20, NumericType::Float32),
            field("rgb", 24, NumericType::Uint32),
            field("rgba", 28, NumericType::Uint8),
        ]);

        super::reinterpret_packed_color_fields(&mut cloud);

        let types: Vec<i32> = cloud.fields.iter().map(|f| f.r#type).collect();
        assert_eq!(
            types,
            [
                NumericType::Float32 as i32, // x
                NumericType::Float32 as i32, // y
                NumericType::Float32 as i32, // z
                NumericType::Uint32 as i32,  // rgb: retyped
                NumericType::Uint32 as i32,  // rgba: retyped
                NumericType::Float32 as i32, // intensity: not a color field
                NumericType::Uint32 as i32,  // rgb: already uint32
                NumericType::Uint8 as i32,   // rgba: not float32
            ]
        );
    }

    #[test]
    fn test_packed_rgb_survives_compression_bit_exact() {
        use crate::Decode;
        use draco_core::decoder_buffer::DecoderBuffer;
        use draco_core::geometry_attribute::GeometryAttributeType;
        use draco_core::point_cloud::PointCloud as DracoCloud;
        use draco_core::point_cloud_decoder::PointCloudDecoder;

        // PCL PointXYZRGB: rgb declared float32, carrying (r << 16) | (g << 8) | b in the
        // bits. Quantized as a float attribute (a range of denormals), nearly every color
        // collapses to a single wrong value; retyped to uint32 the packed bits must
        // round-trip exactly.
        let colors: [u32; 4] = [0x00c8_9664, 0x000a_141e, 0x00ff_0080, 0x0000_ffff];
        let mut data = Vec::new();
        for (i, &color) in colors.iter().enumerate() {
            for c in [i as f32, i as f32 * 2.0, 0.5] {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&f32::from_bits(color).to_le_bytes());
        }
        let field = |name: &str, offset: u32| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: NumericType::Float32 as i32,
        };
        let cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 16,
            fields: vec![
                field("x", 0),
                field("y", 4),
                field("z", 8),
                field("rgb", 12),
            ],
            data: data.into(),
        };
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();

        let transcoded =
            transcode_point_cloud_message(&buf, &CompressPointCloudOptions::default()).unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded.as_ref()).unwrap();

        let mut decoded = DracoCloud::new();
        let mut dbuf = DecoderBuffer::new(&compressed.data);
        PointCloudDecoder::new()
            .decode(&mut dbuf, &mut decoded)
            .unwrap();
        // POSITION is attribute 0; rgb is the generic attribute with unique id 1.
        let attr = decoded.attribute_by_unique_id(1).unwrap();
        assert_eq!(attr.attribute_type(), GeometryAttributeType::Generic);
        let stride = attr.byte_stride() as usize;
        let bytes = attr.buffer().data();
        let mut roundtripped: Vec<u32> = (0..decoded.num_points())
            .map(|p| u32::from_le_bytes(bytes[p * stride..p * stride + 4].try_into().unwrap()))
            .collect();
        // kd-tree encoding reorders points, so compare as sets.
        roundtripped.sort_unstable();
        let mut expected = colors.to_vec();
        expected.sort_unstable();
        assert_eq!(roundtripped, expected);
    }

    #[test]
    fn test_transcodes_cloud_with_non_finite_points() {
        // End to end: a NaN-padded cloud compresses (delivering the finite points)
        // instead of erroring, and an all-non-finite cloud folds to the empty-cloud
        // lossless path rather than failing.
        let options = CompressPointCloudOptions::default();

        let cloud = xyz_cloud(&[[1.0, 2.0, 3.0], [f32::NAN, f32::NAN, f32::NAN]]);
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, &options).unwrap();

        let cloud = xyz_cloud(&[[f32::NAN, f32::NAN, f32::NAN]]);
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, &options).unwrap();
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
