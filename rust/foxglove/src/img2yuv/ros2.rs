//! ROS 2 message decoder.

use std::borrow::Cow;

use crate::schemas::Timestamp;
use serde::{Deserialize, Serialize};

use super::{
    CompressedImage, Compression, Endian, Image, ImageMessage, RawImage, RawImageEncoding,
    UnknownCompressionError, UnknownEncodingError,
};

/// An error that occurs while decoding a ROS 2 message.
#[derive(Debug, thiserror::Error)]
pub enum Ros2DecodeError {
    /// The ROS 2 header timestamp is negative.
    #[error("ros2 header timestamp is negative")]
    NegativeTimestamp,
    /// Failed to parse CDR message.
    #[error(transparent)]
    Cdr(#[from] cdr::Error),
    /// Unknown raw image encoding.
    #[error(transparent)]
    UnknownEncoding(#[from] UnknownEncodingError),
    /// Unknown compression codec.
    #[error(transparent)]
    UnknownCompression(#[from] UnknownCompressionError),
}

/// A ROS 2 `builtin_interfaces/msg/Time` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Ros2Time {
    sec: i32,
    nanosec: u32,
}
impl TryFrom<Ros2Time> for Timestamp {
    type Error = Ros2DecodeError;

    fn try_from(value: Ros2Time) -> Result<Self, Self::Error> {
        if value.sec < 0 {
            return Err(Ros2DecodeError::NegativeTimestamp);
        }
        Ok(Timestamp::new(value.sec as u32, value.nanosec))
    }
}

/// A ROS 2 `std_msgs/msg/Header` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Ros2Header<'a> {
    stamp: Ros2Time,
    frame_id: Cow<'a, str>,
}

/// A ROS 2 `sensor_msgs/msg/Image` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Ros2Image<'a> {
    header: Ros2Header<'a>,
    height: u32,
    width: u32,
    encoding: Cow<'a, str>,
    is_bigendian: u8,
    step: u32,
    data: Cow<'a, [u8]>,
}
impl<'a> Ros2Image<'a> {
    /// Decodes a ROS 2 image.
    pub fn decode(data: &'a [u8]) -> Result<Self, Ros2DecodeError> {
        Ok(cdr::deserialize::<Self>(data)?)
    }
}
impl<'a> TryFrom<Ros2Image<'a>> for ImageMessage<'a> {
    type Error = Ros2DecodeError;

    fn try_from(image: Ros2Image<'a>) -> std::result::Result<Self, Self::Error> {
        let endian = if image.is_bigendian > 0 {
            Endian::Big
        } else {
            Endian::Little
        };
        let encoding = RawImageEncoding::parse_endian(&image.encoding, endian)?;
        Ok(Self {
            timestamp: Some(image.header.stamp.try_into()?),
            frame_id: image.header.frame_id.into(),
            image: Image::Raw(RawImage {
                encoding,
                width: image.width,
                height: image.height,
                stride: image.step,
                data: image.data,
            }),
        })
    }
}

/// A ROS 2 `sensor_msgs/msg/CompressedImage` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Ros2CompressedImage<'a> {
    header: Ros2Header<'a>,
    format: Cow<'a, str>,
    data: Cow<'a, [u8]>,
}
impl<'a> Ros2CompressedImage<'a> {
    /// Decodes a ROS 2 compressed image.
    pub fn decode(data: &'a [u8]) -> Result<Self, Ros2DecodeError> {
        Ok(cdr::deserialize::<Self>(data)?)
    }
}
impl<'a> TryFrom<Ros2CompressedImage<'a>> for ImageMessage<'a> {
    type Error = Ros2DecodeError;

    fn try_from(image: Ros2CompressedImage<'a>) -> std::result::Result<Self, Self::Error> {
        let compression = Compression::try_from_ros_format(&image.format)?;
        Ok(ImageMessage {
            timestamp: Some(image.header.stamp.try_into()?),
            frame_id: image.header.frame_id.into(),
            image: Image::Compressed(CompressedImage {
                compression,
                data: image.data,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use cdr::{CdrLe, Infinite};

    use super::*;

    fn make_header() -> Ros2Header<'static> {
        Ros2Header {
            stamp: Ros2Time {
                sec: 100,
                nanosec: 200,
            },
            frame_id: "frame".into(),
        }
    }

    #[test]
    fn test_roundtrip_image() {
        let image = Ros2Image {
            header: make_header(),
            height: 16,
            width: 16,
            encoding: "mono8".into(),
            is_bigendian: 0,
            step: 16,
            data: Cow::Owned((0..=255).collect()),
        };
        let encoded = cdr::serialize::<_, _, CdrLe>(&image, Infinite).unwrap();
        let decoded: Ros2Image = cdr::deserialize(&encoded).unwrap();
        assert_eq!(image, decoded);
    }

    #[test]
    fn test_roundtrip_compressed_image() {
        let image = Ros2CompressedImage {
            header: make_header(),
            format: "special".into(),
            data: Cow::Owned((0..=255).collect()),
        };
        let encoded = cdr::serialize::<_, _, CdrLe>(&image, Infinite).unwrap();
        let decoded: Ros2CompressedImage = cdr::deserialize(&encoded).unwrap();
        assert_eq!(image, decoded);
    }
}
