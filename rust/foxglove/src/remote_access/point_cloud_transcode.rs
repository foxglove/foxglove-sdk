//! Transparent point-cloud transcoding for the remote-access sink.
//!
//! Detects channels carrying point-cloud messages — protobuf-, JSON-, or
//! FlatBuffer-encoded `foxglove.PointCloud`, or CDR-encoded ROS 2
//! `sensor_msgs/msg/PointCloud2` — rewrites their advertisement to the
//! `foxglove.CompressedPointCloud` schema, and transcodes individual messages using the
//! Draco mechanism in [`crate::draco`]. Every input format is decoded to a
//! `foxglove.PointCloud` before Draco encoding.

mod flatbuffer;
mod ros2;

use bytes::Bytes;
use prost::Message as _;

use crate::draco::{DracoEncodeError, compress_point_cloud};
use crate::messages::{PointCloud, descriptors};
use crate::protocol::common::schema as protocol_schema;
use crate::protocol::common::server::advertise;
use crate::remote_access::PointCloudCompression;
use crate::{Decode, RawChannel};

/// An error transcoding a logged point cloud message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TranscodeError {
    #[error("failed to decode PointCloud message: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("failed to decode PointCloud2 message: {0}")]
    Ros2(#[from] ros2::Ros2PointCloudError),
    #[error("failed to decode JSON PointCloud message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to decode FlatBuffer PointCloud message: {0}")]
    Flatbuffer(#[from] flatbuffer::FlatbufferPointCloudError),
    #[error(transparent)]
    Encode(#[from] DracoEncodeError),
}

/// The message format of a compressible point-cloud channel.
///
/// Every input format is decoded to a `foxglove.PointCloud` before Draco encoding, and the
/// channel is delivered as a protobuf-encoded `foxglove.CompressedPointCloud` regardless of
/// the input format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointCloudInputSchema {
    /// `foxglove.PointCloud` with protobuf encoding.
    FoxgloveProtobuf,
    /// `foxglove.PointCloud` with json encoding.
    FoxgloveJson,
    /// `foxglove.PointCloud` with flatbuffer encoding.
    FoxgloveFlatbuffer,
    /// ROS 2 `sensor_msgs/msg/PointCloud2` with cdr encoding.
    Ros2PointCloud2,
}

/// Maps a channel's message encoding and schema name to a point-cloud input format.
fn detect_point_cloud_schema(encoding: &str, schema_name: &str) -> Option<PointCloudInputSchema> {
    match (encoding, schema_name) {
        ("protobuf", "foxglove.PointCloud") => Some(PointCloudInputSchema::FoxgloveProtobuf),
        ("json", "foxglove.PointCloud") => Some(PointCloudInputSchema::FoxgloveJson),
        ("flatbuffer", "foxglove.PointCloud") => Some(PointCloudInputSchema::FoxgloveFlatbuffer),
        ("cdr", "sensor_msgs/msg/PointCloud2") => Some(PointCloudInputSchema::Ros2PointCloud2),
        _ => None,
    }
}

/// Returns the point-cloud input format of a channel, or `None` if the channel does not
/// carry point cloud messages the sink can transcode.
pub(crate) fn point_cloud_input_schema(channel: &RawChannel) -> Option<PointCloudInputSchema> {
    let schema = channel.schema();
    let schema_name = schema.as_ref().map(|s| s.name.as_str()).unwrap_or_default();
    detect_point_cloud_schema(channel.message_encoding(), schema_name)
}

/// Transcodes a serialized point cloud message into a serialized
/// `foxglove.CompressedPointCloud` message.
pub(crate) fn transcode_point_cloud_message(
    msg: &[u8],
    input_schema: PointCloudInputSchema,
    options: &PointCloudCompression,
) -> Result<Bytes, TranscodeError> {
    let mut cloud = match input_schema {
        PointCloudInputSchema::FoxgloveProtobuf => <PointCloud as Decode>::decode(msg)?,
        PointCloudInputSchema::FoxgloveJson => serde_json::from_slice::<PointCloud>(msg)?,
        PointCloudInputSchema::FoxgloveFlatbuffer => flatbuffer::decode_point_cloud(msg)?,
        PointCloudInputSchema::Ros2PointCloud2 => ros2::Ros2PointCloud2::decode(msg)?.try_into()?,
    };
    // The conditioning passes and Draco encoding run on the converted cloud, so every
    // input format gets the same treatment as native ones.
    //
    // Dropping fields first shrinks the data every later pass and the encoder touch.
    // After that, order matters: colors must be retyped before the non-finite filter
    // runs, because packed rgba values with a nonzero alpha have `0xff------` bit
    // patterns — NaN/infinity encodings when read as floats — and retyping to uint32
    // exempts them. Narrowing runs before the filter so a float64 value that overflows
    // f32 becomes non-finite and its point is dropped, rather than failing the whole
    // cloud.
    drop_irrelevant_fields(&mut cloud);
    reinterpret_packed_color_fields(&mut cloud);
    narrow_float64_fields(&mut cloud);
    drop_non_finite_points(&mut cloud);
    let compressed = compress_point_cloud(&cloud, &options.draco_options())?;
    Ok(Bytes::from(compressed.encode_to_vec()))
}

/// Drops per-point fields that carry no value on a compressed remote-access connection,
/// matched by exact (name, type) tuple against `DROPPED` below. The list and the
/// rationale are documented on
/// [`Gateway::point_cloud_compression`](super::Gateway::point_cloud_compression).
///
/// Tuples match exactly, never fuzzily: `scan_idx` (dropped) and `scan_id` (kept) are
/// distinct uint16 fields in the same vendor struct, and event-camera clouds use time
/// as a literal coordinate under tuples that only exact matching avoids.
///
/// Retained fields are repacked contiguously in declaration order, which also sheds
/// any padding bytes. Clouds this pass cannot read (zero or misaligned stride, fields
/// past the stride, unknown field types) are left for the encoder to report precisely.
fn drop_irrelevant_fields(cloud: &mut PointCloud) {
    use crate::messages::PackedElementField;
    use crate::messages::packed_element_field::NumericType;

    const DROPPED: &[(&str, NumericType)] = &[
        // Per-point time
        ("t", NumericType::Uint32),
        ("time", NumericType::Float32),
        ("ts", NumericType::Float32),
        ("time_stamp", NumericType::Uint32),
        ("timestamp", NumericType::Float64),
        ("timestamp_s", NumericType::Int32),
        ("timestamp_us", NumericType::Int32),
        ("lidar_sec", NumericType::Uint32),
        ("lidar_nsec", NumericType::Uint32),
        // Range and angles, derivable from the positions
        ("range", NumericType::Uint32),
        ("range", NumericType::Float32),
        ("distance", NumericType::Float32),
        ("azimuth", NumericType::Float32),
        ("elevation", NumericType::Float32),
        // Per-point indices
        ("point_id", NumericType::Uint32),
        ("scan_idx", NumericType::Uint16),
    ];
    let dropped = |f: &PackedElementField| {
        DROPPED
            .iter()
            .any(|&(name, ty)| f.name == name && f.r#type == ty as i32)
    };

    if !cloud.fields.iter().any(&dropped) {
        return;
    }
    let stride = cloud.point_stride as usize;
    if stride == 0 || !cloud.data.len().is_multiple_of(stride) {
        return;
    }

    // Plan the repack: each retained field's source range and new offset. Bail out
    // (leaving the cloud untouched) on any field the pass can't size or read.
    let mut sources: Vec<(usize, usize)> = Vec::with_capacity(cloud.fields.len());
    let mut fields = Vec::with_capacity(cloud.fields.len());
    let mut new_stride = 0usize;
    for field in &cloud.fields {
        if dropped(field) {
            continue;
        }
        let Some(size) = field_size(field.r#type) else {
            return;
        };
        let offset = field.offset as usize;
        if offset + size > stride {
            return;
        }
        sources.push((offset, size));
        fields.push(PackedElementField {
            offset: new_stride as u32,
            ..field.clone()
        });
        new_stride += size;
    }
    if fields.is_empty() {
        return;
    }

    let mut data = Vec::with_capacity(cloud.data.len() / stride * new_stride);
    for point in cloud.data.chunks_exact(stride) {
        for &(offset, size) in &sources {
            data.extend_from_slice(&point[offset..offset + size]);
        }
    }
    tracing::debug!(
        fields = cloud.fields.len() - fields.len(),
        "dropped irrelevant per-point fields before compression"
    );
    cloud.fields = fields;
    cloud.point_stride = new_stride as u32;
    cloud.data = data.into();
}

/// The size in bytes of a `PackedElementField` numeric type, or `None` if unknown.
fn field_size(numeric_type: i32) -> Option<usize> {
    use crate::messages::packed_element_field::NumericType;

    match NumericType::try_from(numeric_type).ok()? {
        NumericType::Uint8 | NumericType::Int8 => Some(1),
        NumericType::Uint16 | NumericType::Int16 => Some(2),
        NumericType::Uint32 | NumericType::Int32 | NumericType::Float32 => Some(4),
        NumericType::Float64 => Some(8),
        NumericType::Unknown => None,
    }
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

/// Narrows float64 fields to float32 so the cloud can be quantized.
///
/// The kd-tree encoder cannot quantize float64 attributes, and clouds carrying one not
/// on the drop list (doubles from PCL pipelines, vendor fields under nonstandard names)
/// still occur — without this pass they cannot be delivered at all: compression fails,
/// and delivered raw they exceed the data-track message limit. Compression is lossy by
/// design, and positions were already narrowed to float32 by the encoder, so narrowing
/// the remaining float64 fields is in keeping: values retain float32's ~7 significant
/// digits, and a value whose magnitude overflows float32 becomes non-finite, dropping
/// that point in [`drop_non_finite_points`] (which runs after).
///
/// Narrowing halves each float64 field, so the buffer is repacked: every field is
/// assigned a new offset in declaration order and the stride becomes the sum of the field
/// sizes (which also drops any inter-field padding). Layout problems (zero or misaligned
/// stride, fields past the stride, unknown field types) are left for the encoder, which
/// reports them precisely; this pass only rewrites clouds it can read.
fn narrow_float64_fields(cloud: &mut PointCloud) {
    use crate::messages::packed_element_field::NumericType;

    let stride = cloud.point_stride as usize;
    if stride == 0 || !cloud.data.len().is_multiple_of(stride) {
        return;
    }
    struct Field {
        offset: usize,
        size: usize,
        is_f64: bool,
    }
    let mut fields = Vec::with_capacity(cloud.fields.len());
    for f in &cloud.fields {
        let Some(size) = field_size(f.r#type) else {
            return;
        };
        let offset = f.offset as usize;
        if offset + size > stride {
            return;
        }
        fields.push(Field {
            offset,
            size,
            is_f64: f.r#type == NumericType::Float64 as i32,
        });
    }
    if !fields.iter().any(|f| f.is_f64) {
        return;
    }

    // Assign each field a new slot in declaration order. Aliased fields (two fields over
    // the same bytes) become independent copies, which preserves every field's values.
    let mut new_stride = 0;
    let new_offsets: Vec<usize> = fields
        .iter()
        .map(|f| {
            let offset = new_stride;
            new_stride += if f.is_f64 { 4 } else { f.size };
            offset
        })
        .collect();

    let num_points = cloud.data.len() / stride;
    let mut data = vec![0u8; num_points * new_stride];
    for p in 0..num_points {
        let src = &cloud.data[p * stride..(p + 1) * stride];
        let dst = &mut data[p * new_stride..(p + 1) * new_stride];
        for (f, &new_offset) in fields.iter().zip(&new_offsets) {
            if f.is_f64 {
                let v = f64::from_le_bytes(src[f.offset..f.offset + 8].try_into().unwrap());
                dst[new_offset..new_offset + 4].copy_from_slice(&(v as f32).to_le_bytes());
            } else {
                dst[new_offset..new_offset + f.size]
                    .copy_from_slice(&src[f.offset..f.offset + f.size]);
            }
        }
    }

    for (f, (field, &new_offset)) in fields.iter().zip(cloud.fields.iter_mut().zip(&new_offsets)) {
        field.offset = new_offset as u32;
        if f.is_f64 {
            field.r#type = NumericType::Float32 as i32;
        }
    }
    cloud.point_stride = new_stride as u32;
    cloud.data = data.into();
}

/// Removes points containing a non-finite (NaN or infinite) value in any float field.
///
/// Publishers commonly pad invalid returns with NaN — RGBD cameras and rotating lidars
/// mark non-returns this way — but the Draco quantizer derives each attribute's range
/// from its min/max and errors on any non-finite value, which would fail (and drop) the
/// whole cloud. Every float32 field is quantized under kd-tree encoding, so this applies
/// to positions and attributes (intensity, per-point stamps, ...) alike; dropping just
/// the poisoned points delivers the valid ones instead. Values are judged after the same
/// f64-to-f32 narrowing the encoder applies, so a float64 value that only overflows f32
/// is dropped too.
///
/// Layout problems (zero or misaligned stride, fields past the stride) are left for the
/// encoder, which reports them precisely; this pass only filters clouds it can read.
fn drop_non_finite_points(cloud: &mut PointCloud) {
    use crate::messages::packed_element_field::NumericType;

    let stride = cloud.point_stride as usize;
    if stride == 0 || !cloud.data.len().is_multiple_of(stride) {
        return;
    }
    // Only float32/float64 fields can be non-finite; integer fields never poison the
    // quantizer. Packed rgb/rgba color fields were already retyped to uint32, so their
    // bit patterns (NaN encodings whenever alpha is nonzero) are exempt.
    let floats: Vec<(usize, bool)> = cloud
        .fields
        .iter()
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
    if floats.is_empty() {
        return;
    }

    let finite = |point: &[u8]| {
        floats.iter().all(|&(offset, is_f64)| {
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
        "dropped points with non-finite values before compression"
    );
    cloud.data = data.into();
}

/// Rewrites a channel advertisement to report the protobuf-encoded
/// `foxglove.CompressedPointCloud` schema, replacing the original point cloud schema and
/// message encoding (transcoded output is always protobuf, whatever the input).
///
/// The channel id and topic are unchanged. The original schema name is recorded in the
/// channel metadata as `foxglove.originalSchemaName`, so clients can surface the source type
/// of a transparently compressed channel.
pub(crate) fn rewrite_advertisement(channel: &mut advertise::Channel<'_>) {
    let schema_data = protocol_schema::encode_schema_data(
        "protobuf",
        std::borrow::Cow::Borrowed(descriptors::COMPRESSED_POINT_CLOUD),
    )
    .expect("binary schema encoding is infallible");
    channel.metadata.insert(
        "foxglove.originalSchemaName".to_string(),
        channel.schema_name.to_string(),
    );
    channel.encoding = "protobuf".into();
    channel.schema_name = "foxglove.CompressedPointCloud".into();
    channel.schema_encoding = Some("protobuf".into());
    channel.schema = std::borrow::Cow::Owned(schema_data.into_owned());
}

#[cfg(test)]
mod tests {
    use super::{
        PointCloudInputSchema, drop_non_finite_points, narrow_float64_fields,
        point_cloud_input_schema, transcode_point_cloud_message,
    };
    use crate::messages::{PackedElementField, PointCloud, packed_element_field::NumericType};
    use crate::remote_access::PointCloudCompression;
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
        assert_eq!(
            point_cloud_input_schema(&ch),
            Some(PointCloudInputSchema::FoxgloveProtobuf)
        );
    }

    #[test]
    fn test_detects_ros2_point_cloud2() {
        let ch = make_channel(
            "cdr",
            Some(Schema::new("sensor_msgs/msg/PointCloud2", "ros2msg", b"")),
        );
        assert_eq!(
            point_cloud_input_schema(&ch),
            Some(PointCloudInputSchema::Ros2PointCloud2)
        );
    }

    #[test]
    fn test_detects_flatbuffer_point_cloud() {
        let ch = make_channel(
            "flatbuffer",
            Some(Schema::new("foxglove.PointCloud", "flatbuffer", b"")),
        );
        assert_eq!(
            point_cloud_input_schema(&ch),
            Some(PointCloudInputSchema::FoxgloveFlatbuffer)
        );
    }

    #[test]
    fn test_detects_json_point_cloud() {
        let ch = make_channel(
            "json",
            Some(Schema::new("foxglove.PointCloud", "jsonschema", b"{}")),
        );
        assert_eq!(
            point_cloud_input_schema(&ch),
            Some(PointCloudInputSchema::FoxgloveJson)
        );
    }

    #[test]
    fn test_transcodes_json_point_cloud_to_compressed_point_cloud() {
        use crate::Decode;
        use base64::Engine;

        // Wire-format JSON as the Foxglove JSON conventions produce it: base64-encoded
        // data, enum fields as string names.
        let mut data = Vec::new();
        for c in [1.0f32, 2.0, 3.0] {
            data.extend_from_slice(&c.to_le_bytes());
        }
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        let json = serde_json::json!({
            "timestamp": { "sec": 1, "nsec": 2 },
            "frame_id": "lidar",
            "point_stride": 12,
            "fields": [
                { "name": "x", "offset": 0, "type": "FLOAT32" },
                { "name": "y", "offset": 4, "type": "FLOAT32" },
                { "name": "z", "offset": 8, "type": "FLOAT32" },
            ],
            "data": data_b64,
        })
        .to_string();

        let transcoded = transcode_point_cloud_message(
            json.as_bytes(),
            PointCloudInputSchema::FoxgloveJson,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded.as_ref()).unwrap();
        assert_eq!(compressed.format, "draco");
        assert_eq!(compressed.frame_id, "lidar");
        assert!(!compressed.data.is_empty());
    }

    // A ROS 2 `sensor_msgs/msg/PointCloud2`, CDR-encoded like a ROS 2 publisher would.
    #[derive(serde::Serialize)]
    struct Time {
        sec: i32,
        nanosec: u32,
    }
    #[derive(serde::Serialize)]
    struct Header {
        stamp: Time,
        frame_id: String,
    }
    #[derive(serde::Serialize)]
    struct PointField {
        name: String,
        offset: u32,
        datatype: u8,
        count: u32,
    }
    #[derive(serde::Serialize)]
    struct PointCloud2 {
        header: Header,
        height: u32,
        width: u32,
        fields: Vec<PointField>,
        is_bigendian: bool,
        point_step: u32,
        row_step: u32,
        data: Vec<u8>,
        is_dense: bool,
    }

    /// CDR-encodes a float32 xyz PointCloud2 with `width` points per row.
    fn cdr_cloud(points: &[[f32; 3]], width: u32, is_dense: bool) -> Vec<u8> {
        let mut data = Vec::new();
        for point in points {
            for c in point {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        let cloud = PointCloud2 {
            header: Header {
                stamp: Time { sec: 1, nanosec: 2 },
                frame_id: "lidar".into(),
            },
            height: points.len() as u32 / width,
            width,
            fields: ["x", "y", "z"]
                .into_iter()
                .enumerate()
                .map(|(i, name)| PointField {
                    name: name.into(),
                    offset: 4 * i as u32,
                    datatype: 7,
                    count: 1,
                })
                .collect(),
            is_bigendian: false,
            point_step: 12,
            row_step: 12 * width,
            data,
            is_dense,
        };
        cdr::serialize::<_, _, cdr::CdrLe>(&cloud, cdr::Infinite).unwrap()
    }

    #[test]
    fn test_transcodes_cdr_point_cloud2_to_compressed_point_cloud() {
        use crate::Decode;

        let encoded = cdr_cloud(&[[1.0, 2.0, 3.0]], 1, true);
        let transcoded = transcode_point_cloud_message(
            &encoded,
            PointCloudInputSchema::Ros2PointCloud2,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded.as_ref()).unwrap();
        assert_eq!(compressed.format, "draco");
        assert_eq!(compressed.frame_id, "lidar");
        assert!(!compressed.data.is_empty());
    }

    #[test]
    fn test_transcodes_nan_padded_point_cloud2() {
        use crate::Decode;

        // An organized (2x2) cloud padding invalid returns with NaN and declaring
        // `is_dense: false` — the norm for RGBD cameras and rotating lidars. The
        // non-finite filter drops the padding points, so the cloud compresses instead of
        // failing on the quantizer's NaN rejection.
        let encoded = cdr_cloud(
            &[
                [1.0, 2.0, 3.0],
                [f32::NAN, f32::NAN, f32::NAN],
                [f32::NAN, f32::NAN, f32::NAN],
                [4.0, 5.0, 6.0],
            ],
            2,
            false,
        );
        let transcoded = transcode_point_cloud_message(
            &encoded,
            PointCloudInputSchema::Ros2PointCloud2,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded.as_ref()).unwrap();
        assert_eq!(compressed.format, "draco");
        assert!(!compressed.data.is_empty());
    }

    /// A cloud with float32 xyz positions plus a float64 `stamp` field.
    fn stamped_cloud(points: usize) -> PointCloud {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let mut data = Vec::new();
        for i in 0..points {
            for v in [i as f32, i as f32 * 2.0, 0.5] {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&(i as f64).to_le_bytes());
        }
        PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 20,
            fields: vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
                field("stamp", 12, NumericType::Float64),
            ],
            data: data.into(),
        }
    }

    #[test]
    fn test_narrows_float64_fields() {
        // Layout rewrite: the stamp field shrinks from 8 to 4 bytes, so every field gets
        // a new offset and the stride shrinks; the stamp values are narrowed, the rest
        // copied verbatim, and the field type flips to float32.
        let mut cloud = stamped_cloud(3);
        narrow_float64_fields(&mut cloud);

        assert_eq!(cloud.point_stride, 16);
        assert_eq!(cloud.fields.len(), 4);
        assert_eq!(cloud.fields[3].name, "stamp");
        assert_eq!(cloud.fields[3].offset, 12);
        assert_eq!(cloud.fields[3].r#type, NumericType::Float32 as i32);
        assert_eq!(cloud.data.len(), 3 * 16);
        for i in 0..3 {
            let base = i * 16;
            let x = f32::from_le_bytes(cloud.data[base..base + 4].try_into().unwrap());
            let stamp = f32::from_le_bytes(cloud.data[base + 12..base + 16].try_into().unwrap());
            assert_eq!(x, i as f32);
            assert_eq!(stamp, i as f32);
        }

        // A cloud with no float64 fields passes through untouched.
        let mut cloud = xyz_cloud(&[[1.0, 2.0, 3.0]]);
        let before = cloud.data.clone();
        narrow_float64_fields(&mut cloud);
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.data, before);
    }

    #[test]
    fn test_transcodes_float64_fields() {
        // Clouds with float64 fields not on the drop list (like this `stamp`) still
        // occur, and the kd-tree encoder cannot quantize them; narrowing lets them
        // through. Previously these were rejected with UnquantizableField, making the
        // channel undeliverable.
        let options = PointCloudCompression::default();

        let mut buf = Vec::new();
        stamped_cloud(8).encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, PointCloudInputSchema::FoxgloveProtobuf, &options)
            .unwrap();

        // Empty clouds fold to lossless regardless of fields and must round-trip.
        let mut buf = Vec::new();
        stamped_cloud(0).encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, PointCloudInputSchema::FoxgloveProtobuf, &options)
            .unwrap();
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
    fn test_drops_points_with_non_finite_attributes() {
        // A non-finite value in any quantized float attribute — not just positions —
        // fails the whole encode, so the filter must drop the poisoned point. This also
        // covers narrowing overflow: a finite float64 too large for float32 becomes
        // infinity, and end to end the cloud must still transcode.
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let make = |intensities: &[f32]| {
            let mut data = Vec::new();
            for (i, &intensity) in intensities.iter().enumerate() {
                for v in [i as f32, i as f32 * 2.0, 0.5] {
                    data.extend_from_slice(&v.to_le_bytes());
                }
                data.extend_from_slice(&intensity.to_le_bytes());
            }
            PointCloud {
                timestamp: None,
                frame_id: "t".to_string(),
                pose: None,
                point_stride: 16,
                fields: vec![
                    field("x", 0, NumericType::Float32),
                    field("y", 4, NumericType::Float32),
                    field("z", 8, NumericType::Float32),
                    field("intensity", 12, NumericType::Float32),
                ],
                data: data.into(),
            }
        };

        let original = make(&[1.0, f32::NAN, 3.0]);
        let mut cloud = original.clone();
        drop_non_finite_points(&mut cloud);
        // The first and third points survive with their original bytes.
        let mut expected = original.data[..16].to_vec();
        expected.extend_from_slice(&original.data[32..]);
        assert_eq!(cloud.data, expected);

        let mut buf = Vec::new();
        make(&[1.0, f32::NAN, 3.0]).encode(&mut buf).unwrap();
        transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveProtobuf,
            &PointCloudCompression::default(),
        )
        .unwrap();

        // A float64 stamp that overflows float32 narrows to infinity; its point drops
        // and the rest of the cloud transcodes.
        let mut cloud = stamped_cloud(3);
        let stamp_offset = 20 + 12; // second point's stamp
        let mut data = cloud.data.to_vec();
        data[stamp_offset..stamp_offset + 8].copy_from_slice(&1e300f64.to_le_bytes());
        cloud.data = data.into();
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveProtobuf,
            &PointCloudCompression::default(),
        )
        .unwrap();
    }

    #[test]
    fn test_packed_rgba_with_alpha_survives_the_finite_filter() {
        // Packed rgba values with nonzero alpha have 0xff------ bit patterns — NaN
        // encodings when read as floats. The color retype must run before the
        // non-finite filter, or every opaque colored point would be dropped; pin the
        // ordering by counting the points that survive compression.
        use crate::Decode;
        use draco_core::decoder_buffer::DecoderBuffer;
        use draco_core::point_cloud::PointCloud as DracoCloud;
        use draco_core::point_cloud_decoder::PointCloudDecoder;

        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let colors: [u32; 3] = [0xff00_0000, 0xffc8_9664, 0xff0a_141e];
        let mut data = Vec::new();
        for (i, &color) in colors.iter().enumerate() {
            for v in [i as f32, i as f32 * 2.0, 0.5] {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&color.to_le_bytes());
        }
        let cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 16,
            fields: vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
                field("rgba", 12, NumericType::Float32),
            ],
            data: data.into(),
        };
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();

        let transcoded = transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveProtobuf,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded.as_ref()).unwrap();
        let mut decoded = DracoCloud::new();
        let mut dbuf = DecoderBuffer::new(&compressed.data);
        PointCloudDecoder::new()
            .decode(&mut dbuf, &mut decoded)
            .unwrap();
        assert_eq!(decoded.num_points(), colors.len());
    }

    #[test]
    fn test_drops_irrelevant_fields() {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        // An Ouster-style point: xyz + intensity + t + reflectivity + ring + range,
        // with 4 bytes of trailing padding (stride 32, fields end at 28).
        let mut data = Vec::new();
        for i in 0..3u32 {
            for c in [i as f32, i as f32 * 2.0, 0.5f32, 100.0 + i as f32] {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&(1_000_000 * i).to_le_bytes()); // t
            data.extend_from_slice(&(200 + i as u16).to_le_bytes()); // reflectivity
            data.extend_from_slice(&(i as u16).to_le_bytes()); // ring
            data.extend_from_slice(&(5000 + i).to_le_bytes()); // range
            data.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]); // padding
        }
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 32,
            fields: vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
                field("intensity", 12, NumericType::Float32),
                field("t", 16, NumericType::Uint32),
                field("reflectivity", 20, NumericType::Uint16),
                field("ring", 22, NumericType::Uint16),
                field("range", 24, NumericType::Uint32),
            ],
            data: data.into(),
        };

        super::drop_irrelevant_fields(&mut cloud);

        // `t` and `range` are gone; the survivors are repacked contiguously (padding
        // shed too) with recomputed offsets.
        let named: Vec<(&str, u32)> = cloud
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.offset))
            .collect();
        assert_eq!(
            named,
            [
                ("x", 0),
                ("y", 4),
                ("z", 8),
                ("intensity", 12),
                ("reflectivity", 16),
                ("ring", 18),
            ]
        );
        assert_eq!(cloud.point_stride, 20);
        let mut expected = Vec::new();
        for i in 0..3u32 {
            for c in [i as f32, i as f32 * 2.0, 0.5f32, 100.0 + i as f32] {
                expected.extend_from_slice(&c.to_le_bytes());
            }
            expected.extend_from_slice(&(200 + i as u16).to_le_bytes());
            expected.extend_from_slice(&(i as u16).to_le_bytes());
        }
        assert_eq!(cloud.data, expected);
    }

    #[test]
    fn test_drop_matches_exact_tuples_only() {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        // scan_id and scan_idx are distinct uint16 fields in the same vendor struct
        // (Seyond): the index is dropped, the beam id is colorby-able and kept. A
        // right-name-wrong-type field (`t` as float32) is kept too — event-camera
        // clouds use time as a literal coordinate under such tuples.
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 12,
            fields: vec![
                field("t", 0, NumericType::Float32),
                field("scan_id", 4, NumericType::Uint16),
                field("scan_idx", 6, NumericType::Uint16),
                field("timestamp", 8, NumericType::Float32),
            ],
            data: vec![0u8; 24].into(),
        };
        super::drop_irrelevant_fields(&mut cloud);
        let names: Vec<&str> = cloud.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["t", "scan_id", "timestamp"]);
        assert_eq!(cloud.point_stride, 10);

        // No droppable field at all: the cloud passes through untouched.
        let mut cloud = xyz_cloud(&[[1.0, 2.0, 3.0]]);
        let before = cloud.clone();
        super::drop_irrelevant_fields(&mut cloud);
        assert_eq!(cloud, before);
    }

    #[test]
    fn test_drop_leaves_unreadable_clouds_for_the_encoder() {
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        // A retained field of unknown type can't be sized for the repack.
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 8,
            fields: vec![
                field("mystery", 0, NumericType::Unknown),
                field("t", 4, NumericType::Uint32),
            ],
            data: vec![0u8; 16].into(),
        };
        let before = cloud.clone();
        super::drop_irrelevant_fields(&mut cloud);
        assert_eq!(cloud, before);

        // A retained field extending past the stride is a layout error the encoder
        // reports.
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 8,
            fields: vec![
                field("t", 0, NumericType::Uint32),
                field("x", 6, NumericType::Float32),
            ],
            data: vec![0u8; 16].into(),
        };
        let before = cloud.clone();
        super::drop_irrelevant_fields(&mut cloud);
        assert_eq!(cloud, before);

        // Dropping would leave no fields at all; the encoder's missing-position error
        // is clearer than an empty repack.
        let mut cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 4,
            fields: vec![field("range", 0, NumericType::Uint32)],
            data: vec![0u8; 8].into(),
        };
        let before = cloud.clone();
        super::drop_irrelevant_fields(&mut cloud);
        assert_eq!(cloud, before);
    }

    #[test]
    fn test_transcodes_float64_timestamp_cloud() {
        // (timestamp, float64) — the Hesai/Livox/RoboSense convention — is on the drop
        // list, so a cloud that would otherwise be rejected for its float64 field
        // compresses fine once the field is dropped.
        let field = |name: &str, offset: u32, t: NumericType| PackedElementField {
            name: name.to_string(),
            offset,
            r#type: t as i32,
        };
        let mut data = Vec::new();
        for i in 0..8u32 {
            for c in [i as f32, i as f32 * 2.0, 0.5f32] {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&(1.7e9 + i as f64 * 1e-4).to_le_bytes());
        }
        let cloud = PointCloud {
            timestamp: None,
            frame_id: "t".to_string(),
            pose: None,
            point_stride: 20,
            fields: vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
                field("timestamp", 12, NumericType::Float64),
            ],
            data: data.into(),
        };
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveProtobuf,
            &PointCloudCompression::default(),
        )
        .unwrap();
    }

    /// Draco-decodes a transcoded `CompressedPointCloud` message and returns the packed
    /// color values from the generic attribute (unique id 1; POSITION is 0), sorted
    /// because kd-tree encoding reorders points.
    fn decode_packed_colors(transcoded: &[u8]) -> Vec<u32> {
        use crate::Decode;
        use draco_core::decoder_buffer::DecoderBuffer;
        use draco_core::geometry_attribute::GeometryAttributeType;
        use draco_core::point_cloud::PointCloud as DracoCloud;
        use draco_core::point_cloud_decoder::PointCloudDecoder;

        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(transcoded).unwrap();
        let mut decoded = DracoCloud::new();
        let mut dbuf = DecoderBuffer::new(&compressed.data);
        PointCloudDecoder::new()
            .decode(&mut dbuf, &mut decoded)
            .unwrap();
        let attr = decoded.attribute_by_unique_id(1).unwrap();
        assert_eq!(attr.attribute_type(), GeometryAttributeType::Generic);
        let stride = attr.byte_stride() as usize;
        let bytes = attr.buffer().data();
        let mut colors: Vec<u32> = (0..decoded.num_points())
            .map(|p| u32::from_le_bytes(bytes[p * stride..p * stride + 4].try_into().unwrap()))
            .collect();
        colors.sort_unstable();
        colors
    }

    #[test]
    fn test_packed_rgb_survives_compression_bit_exact() {
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

        let transcoded = transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveProtobuf,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let mut expected = colors.to_vec();
        expected.sort_unstable();
        assert_eq!(decode_packed_colors(&transcoded), expected);
    }

    #[test]
    fn test_transcodes_packed_rgb_point_cloud2_bit_exact() {
        // PCL PointXYZRGB over ROS 2: the rgb field declared FLOAT32 (datatype 7) while
        // carrying packed (r << 16) | (g << 8) | b bits. The packed-color retype runs on
        // the converted cloud, so the colors must survive compression bit-exactly for
        // this input too.
        let colors: [u32; 4] = [0x00c8_9664, 0x000a_141e, 0x00ff_0080, 0x0000_ffff];
        let mut data = Vec::new();
        for (i, &color) in colors.iter().enumerate() {
            for c in [i as f32, i as f32 * 2.0, 0.5] {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&color.to_le_bytes());
        }
        let fields = ["x", "y", "z", "rgb"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| PointField {
                name: name.into(),
                offset: 4 * i as u32,
                datatype: 7, // FLOAT32, including rgb: the PCL packed-color declaration
                count: 1,
            })
            .collect();
        let cloud = PointCloud2 {
            header: Header {
                stamp: Time { sec: 1, nanosec: 2 },
                frame_id: "lidar".into(),
            },
            height: 1,
            width: colors.len() as u32,
            fields,
            is_bigendian: false,
            point_step: 16,
            row_step: 16 * colors.len() as u32,
            data,
            is_dense: true,
        };
        let encoded = cdr::serialize::<_, _, cdr::CdrLe>(&cloud, cdr::Infinite).unwrap();

        let transcoded = transcode_point_cloud_message(
            &encoded,
            PointCloudInputSchema::Ros2PointCloud2,
            &PointCloudCompression::default(),
        )
        .unwrap();
        let mut expected = colors.to_vec();
        expected.sort_unstable();
        assert_eq!(decode_packed_colors(&transcoded), expected);
    }

    #[test]
    fn test_transcodes_cloud_with_non_finite_points() {
        // End to end: a NaN-padded cloud compresses (delivering the finite points)
        // instead of erroring, and an all-non-finite cloud folds to the empty-cloud
        // lossless path rather than failing.
        let options = PointCloudCompression::default();

        let cloud = xyz_cloud(&[[1.0, 2.0, 3.0], [f32::NAN, f32::NAN, f32::NAN]]);
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, PointCloudInputSchema::FoxgloveProtobuf, &options)
            .unwrap();

        let cloud = xyz_cloud(&[[f32::NAN, f32::NAN, f32::NAN]]);
        let mut buf = Vec::new();
        cloud.encode(&mut buf).unwrap();
        transcode_point_cloud_message(&buf, PointCloudInputSchema::FoxgloveProtobuf, &options)
            .unwrap();
    }

    #[test]
    fn test_ignores_other_channels() {
        // Wrong schema.
        let ch = make_channel(
            "protobuf",
            <crate::messages::CompressedPointCloud as Encode>::get_schema(),
        );
        assert_eq!(point_cloud_input_schema(&ch), None);

        // Wrong encoding.
        let ch = make_channel(
            "msgpack",
            Some(Schema::new("foxglove.PointCloud", "jsonschema", b"{}")),
        );
        assert_eq!(point_cloud_input_schema(&ch), None);

        // No schema.
        let ch = make_channel("json", None);
        assert_eq!(point_cloud_input_schema(&ch), None);
    }
}
