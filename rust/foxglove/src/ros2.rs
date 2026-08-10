//! Shared ROS 2 wire-format types for CDR decoders.
//!
//! The `std_msgs/msg/Header` prefix (a `builtin_interfaces/msg/Time` stamp plus a frame
//! id) is common to every ROS 2 sensor message the SDK decodes. Both the image decoder
//! ([`crate::img2yuv::ros2`]) and the point-cloud decoder
//! (`remote_access::point_cloud_transcode::ros2`) deserialize it with these types, so the
//! wire layout and the negative-timestamp policy have exactly one home.
//!
//! The structs own their data: the `cdr` crate deserializes through `io::Read` and never
//! borrows from the input buffer, so borrowed fields would carry a lifetime without ever
//! avoiding a copy.

use serde::{Deserialize, Serialize};

use crate::messages::Timestamp;

/// The ROS 2 header timestamp is negative.
///
/// Decoder error enums wrap this with `#[error(transparent)]` so the message reaches
/// their consumers unchanged.
#[derive(Debug, thiserror::Error)]
#[error("ros2 header timestamp is negative")]
pub(crate) struct NegativeTimestampError;

/// A ROS 2 `builtin_interfaces/msg/Time` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Ros2Time {
    pub(crate) sec: i32,
    pub(crate) nanosec: u32,
}

impl TryFrom<Ros2Time> for Timestamp {
    type Error = NegativeTimestampError;

    fn try_from(value: Ros2Time) -> Result<Self, Self::Error> {
        if value.sec < 0 {
            return Err(NegativeTimestampError);
        }
        // `sec` is bounded by `i32::MAX`, so the nanosecond carry cannot overflow the u32
        // seconds field, and `new` cannot panic here.
        Ok(Timestamp::new(value.sec as u32, value.nanosec))
    }
}

/// A ROS 2 `std_msgs/msg/Header` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Ros2Header {
    pub(crate) stamp: Ros2Time,
    pub(crate) frame_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_converts_timestamp() {
        let ts = Timestamp::try_from(Ros2Time {
            sec: 100,
            nanosec: 200,
        })
        .unwrap();
        assert_eq!(ts, Timestamp::new(100, 200));
    }

    #[test]
    fn test_rejects_negative_timestamp() {
        let err = Timestamp::try_from(Ros2Time {
            sec: -1,
            nanosec: 0,
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "ros2 header timestamp is negative");
    }
}
