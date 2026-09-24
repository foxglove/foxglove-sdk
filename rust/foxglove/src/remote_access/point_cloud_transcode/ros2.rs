//! ROS 2 `sensor_msgs/msg/PointCloud2` decoder for point-cloud compression.
//!
//! The message structs own their data: the `cdr` crate deserializes through `io::Read` and
//! never borrows from the input buffer, so borrowed fields would carry a lifetime without
//! ever avoiding a copy.

use serde::{Deserialize, Serialize};

use crate::messages::PointCloud;
use crate::remote_access::point_cloud_transcode::point_cloud2::{
    PointCloud2, PointCloud2Error, PointField,
};
use crate::ros2::{NegativeTimestampError, Ros2Header};

/// An error that occurs while decoding a ROS 2 point cloud message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Ros2PointCloudError {
    /// The ROS 2 header timestamp is invalid.
    #[error(transparent)]
    Timestamp(#[from] NegativeTimestampError),
    /// Failed to parse CDR message.
    #[error(transparent)]
    Cdr(#[from] cdr::Error),
    /// The cloud's layout cannot be converted.
    #[error(transparent)]
    Layout(#[from] PointCloud2Error),
}

/// Decodes a ROS 2 `sensor_msgs/msg/PointCloud2` message into a `foxglove.PointCloud`.
pub(crate) fn decode_point_cloud(msg: &[u8]) -> Result<PointCloud, Ros2PointCloudError> {
    Ros2PointCloud2::decode(msg)?.try_into()
}

/// A ROS 2 `sensor_msgs/msg/PointCloud2` message.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Ros2PointCloud2 {
    header: Ros2Header,
    height: u32,
    width: u32,
    fields: Vec<PointField>,
    is_bigendian: bool,
    point_step: u32,
    row_step: u32,
    data: Vec<u8>,
    /// Advisory, and deliberately not consulted; see [`PointCloud2`].
    is_dense: bool,
}

impl Ros2PointCloud2 {
    /// Decodes a ROS 2 point cloud.
    fn decode(data: &[u8]) -> Result<Self, Ros2PointCloudError> {
        Ok(cdr::deserialize::<Self>(data)?)
    }
}

impl TryFrom<Ros2PointCloud2> for PointCloud {
    type Error = Ros2PointCloudError;

    fn try_from(cloud: Ros2PointCloud2) -> Result<Self, Self::Error> {
        let cloud = PointCloud2 {
            timestamp: cloud.header.stamp.try_into()?,
            frame_id: cloud.header.frame_id,
            height: cloud.height,
            width: cloud.width,
            fields: cloud.fields,
            is_bigendian: cloud.is_bigendian,
            point_step: cloud.point_step,
            row_step: cloud.row_step,
            data: cloud.data,
        };
        Ok(cloud.try_into()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::Timestamp;
    use crate::remote_access::point_cloud_transcode::point_cloud2::tests::{
        cloud_data, xyz_fields,
    };
    use crate::ros2::Ros2Time;
    use cdr::{CdrLe, Infinite};

    fn make_cloud(points: &[[f32; 3]]) -> Ros2PointCloud2 {
        Ros2PointCloud2 {
            header: Ros2Header {
                stamp: Ros2Time {
                    sec: 12,
                    nanosec: 34,
                },
                frame_id: "lidar".into(),
            },
            height: 1,
            width: points.len() as u32,
            fields: xyz_fields(),
            is_bigendian: false,
            point_step: 12,
            row_step: 12 * points.len() as u32,
            data: cloud_data(points),
            is_dense: true,
        }
    }

    fn roundtrip(cloud: &Ros2PointCloud2) -> Ros2PointCloud2 {
        let encoded = cdr::serialize::<_, _, CdrLe>(cloud, Infinite).unwrap();
        Ros2PointCloud2::decode(&encoded).unwrap()
    }

    #[test]
    fn test_decodes_and_converts_point_cloud2() {
        let points = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let decoded = roundtrip(&make_cloud(&points));
        let cloud = PointCloud::try_from(decoded).unwrap();

        assert_eq!(cloud.timestamp, Some(Timestamp::new(12, 34)));
        assert_eq!(cloud.frame_id, "lidar");
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.fields.len(), 3);
        assert_eq!(cloud.data, cloud_data(&points));
    }

    #[test]
    fn test_rejects_negative_timestamp() {
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.header.stamp.sec = -1;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(Ros2PointCloudError::Timestamp(_))
        ));
    }

    #[test]
    fn test_propagates_layout_errors() {
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.is_bigendian = true;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(Ros2PointCloudError::Layout(PointCloud2Error::BigEndian))
        ));
    }
}
