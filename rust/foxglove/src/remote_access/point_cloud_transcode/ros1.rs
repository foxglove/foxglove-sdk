//! ROS 1 `sensor_msgs/PointCloud2` decoder for point-cloud compression.

use bytes::Buf;

use crate::messages::PointCloud;
use crate::remote_access::point_cloud_transcode::point_cloud2::{
    PointCloud2, PointCloud2Error, PointField,
};
use crate::ros1::{Ros1BufExt, Ros1WireError};

/// An error that occurs while decoding a ROS 1 point cloud message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Ros1PointCloudError {
    /// Failed to parse the ROS 1 message.
    #[error(transparent)]
    Wire(#[from] Ros1WireError),
    /// The cloud's layout cannot be converted.
    #[error(transparent)]
    Layout(#[from] PointCloud2Error),
}

/// Decodes a ROS 1 `sensor_msgs/PointCloud2` message into a `foxglove.PointCloud`.
pub(crate) fn decode_point_cloud(msg: &[u8]) -> Result<PointCloud, Ros1PointCloudError> {
    Ok(decode_point_cloud2(msg)?.try_into()?)
}

/// Reads a ROS 1 `sensor_msgs/PointCloud2` message.
fn decode_point_cloud2(mut msg: &[u8]) -> Result<PointCloud2, Ros1WireError> {
    let header = msg.try_get_ros1_header()?;
    let height = msg.try_get_u32_le()?;
    let width = msg.try_get_u32_le()?;
    let num_fields = msg.try_get_u32_le()? as usize;
    // Each field occupies at least 13 bytes on the wire, which bounds the allocation by
    // the message length rather than the untrusted count.
    let mut fields = Vec::with_capacity(num_fields.min(msg.remaining() / 13));
    for _ in 0..num_fields {
        let name = msg.try_get_ros1_str()?.to_string();
        let offset = msg.try_get_u32_le()?;
        let datatype = msg.try_get_u8()?;
        let count = msg.try_get_u32_le()?;
        fields.push(PointField {
            name,
            offset,
            datatype,
            count,
        });
    }
    let is_bigendian = msg.try_get_u8()? != 0;
    let point_step = msg.try_get_u32_le()?;
    let row_step = msg.try_get_u32_le()?;
    let data = msg.try_get_ros1_bytes()?.to_vec();
    // `is_dense` is advisory and deliberately not consulted; see `PointCloud2`.
    let _is_dense = msg.try_get_u8()?;
    Ok(PointCloud2 {
        timestamp: header.timestamp()?,
        frame_id: header.frame_id.to_string(),
        height,
        width,
        fields,
        is_bigendian,
        point_step,
        row_step,
        data,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::messages::Timestamp;
    use crate::remote_access::point_cloud_transcode::point_cloud2::tests::{
        cloud_data, make_cloud,
    };
    use bytes::BufMut;

    /// Serializes a `sensor_msgs/PointCloud2` as a ROS 1 publisher would.
    pub(crate) fn encode_point_cloud2(cloud: &PointCloud2) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.put_u32_le(7); // seq
        buf.put_u32_le(cloud.timestamp.sec());
        buf.put_u32_le(cloud.timestamp.nsec());
        buf.put_u32_le(cloud.frame_id.len() as u32);
        buf.put_slice(cloud.frame_id.as_bytes());
        buf.put_u32_le(cloud.height);
        buf.put_u32_le(cloud.width);
        buf.put_u32_le(cloud.fields.len() as u32);
        for field in &cloud.fields {
            buf.put_u32_le(field.name.len() as u32);
            buf.put_slice(field.name.as_bytes());
            buf.put_u32_le(field.offset);
            buf.put_u8(field.datatype);
            buf.put_u32_le(field.count);
        }
        buf.put_u8(cloud.is_bigendian.into());
        buf.put_u32_le(cloud.point_step);
        buf.put_u32_le(cloud.row_step);
        buf.put_u32_le(cloud.data.len() as u32);
        buf.put_slice(&cloud.data);
        buf.put_u8(1); // is_dense
        buf
    }

    #[test]
    fn test_roundtrips_point_cloud2() {
        let cloud = make_cloud(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let decoded = decode_point_cloud2(&encode_point_cloud2(&cloud)).unwrap();
        assert_eq!(decoded, cloud);
    }

    #[test]
    fn test_decodes_genpy_serialized_point_cloud2() {
        // Serialized by genpy (ROS Noetic) from:
        //   header: {seq: 7, stamp: {secs: 1234, nsecs: 5678}, frame_id: "lidar"}
        //   height: 1, width: 2, point_step: 16, row_step: 32, is_dense: true
        //   fields: x/y/z FLOAT32 at 0/4/8, intensity UINT16 at 12, all count 1
        //   data: (1, 2, 3, 100), (4, 5, 6, 200), each point padded to 16 bytes
        #[rustfmt::skip]
        let encoded: &[u8] = &[
            0x07, 0x00, 0x00, 0x00, 0xd2, 0x04, 0x00, 0x00, 0x2e, 0x16, 0x00, 0x00,
            0x05, 0x00, 0x00, 0x00, 0x6c, 0x69, 0x64, 0x61, 0x72, 0x01, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x07, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x79, 0x04, 0x00, 0x00, 0x00, 0x07, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x7a, 0x08, 0x00, 0x00, 0x00, 0x07, 0x01,
            0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x69, 0x6e, 0x74, 0x65, 0x6e,
            0x73, 0x69, 0x74, 0x79, 0x0c, 0x00, 0x00, 0x00, 0x04, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x20, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
            0x40, 0x40, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x40, 0x00, 0x00,
            0xa0, 0x40, 0x00, 0x00, 0xc0, 0x40, 0xc8, 0x00, 0x00, 0x00, 0x01,
        ];

        let field = |name: &str, offset, datatype| PointField {
            name: name.into(),
            offset,
            datatype,
            count: 1,
        };
        let mut data = Vec::new();
        for (xyz, intensity) in [([1.0f32, 2.0, 3.0], 100u16), ([4.0, 5.0, 6.0], 200)] {
            for c in xyz {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&intensity.to_le_bytes());
            data.extend_from_slice(&[0, 0]);
        }
        let expected = PointCloud2 {
            timestamp: Timestamp::new(1234, 5678),
            frame_id: "lidar".into(),
            height: 1,
            width: 2,
            fields: vec![
                field("x", 0, 7),
                field("y", 4, 7),
                field("z", 8, 7),
                field("intensity", 12, 4),
            ],
            is_bigendian: false,
            point_step: 16,
            row_step: 32,
            data,
        };
        assert_eq!(decode_point_cloud2(encoded).unwrap(), expected);
    }

    #[test]
    fn test_decodes_and_converts_point_cloud2() {
        let points = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let encoded = encode_point_cloud2(&make_cloud(&points));
        let cloud = decode_point_cloud(&encoded).unwrap();

        assert_eq!(cloud.timestamp, Some(Timestamp::new(12, 34)));
        assert_eq!(cloud.frame_id, "lidar");
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.fields.len(), 3);
        assert_eq!(cloud.data, cloud_data(&points));
    }

    #[test]
    fn test_rejects_truncated_message() {
        let encoded = encode_point_cloud2(&make_cloud(&[[1.0, 2.0, 3.0]]));
        // Every proper prefix is missing at least the trailing `is_dense` byte.
        for len in 0..encoded.len() {
            assert!(
                matches!(
                    decode_point_cloud(&encoded[..len]),
                    Err(Ros1PointCloudError::Wire(
                        Ros1WireError::UnexpectedEof { .. }
                    ))
                ),
                "prefix length {len}"
            );
        }
    }

    #[test]
    fn test_rejects_absurd_field_count_without_allocating() {
        let mut encoded = encode_point_cloud2(&make_cloud(&[]));
        // The field count follows the header (seq, sec, nsec, frame_id) and dimensions.
        let offset = 12 + 4 + "lidar".len() + 8;
        encoded[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            decode_point_cloud(&encoded),
            Err(Ros1PointCloudError::Wire(
                Ros1WireError::UnexpectedEof { .. }
            ))
        ));
    }

    #[test]
    fn test_rejects_overflowing_timestamp() {
        let mut encoded = encode_point_cloud2(&make_cloud(&[[1.0, 2.0, 3.0]]));
        // Excess nanoseconds carry into seconds, overflowing the u32 seconds field.
        encoded[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        encoded[8..12].copy_from_slice(&1_000_000_000u32.to_le_bytes());
        assert!(matches!(
            decode_point_cloud(&encoded),
            Err(Ros1PointCloudError::Wire(Ros1WireError::InvalidTimestamp))
        ));
    }

    #[test]
    fn test_propagates_layout_errors() {
        let mut cloud = make_cloud(&[[1.0, 2.0, 3.0]]);
        cloud.is_bigendian = true;
        assert!(matches!(
            decode_point_cloud(&encode_point_cloud2(&cloud)),
            Err(Ros1PointCloudError::Layout(PointCloud2Error::BigEndian))
        ));
    }
}
