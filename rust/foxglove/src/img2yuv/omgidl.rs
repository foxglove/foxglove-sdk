//! OMG IDL message decoder.
//!
//! Foxglove `CompressedImage` and `RawImage` messages defined in OMG IDL are serialized with CDR.
//! Unlike the ROS 2 types (see [`super::ros2`]), these use the Foxglove field layout: a
//! `foxglove::Time` timestamp, and a compressed `format` / raw `encoding` string using the
//! Foxglove vocabulary.

use std::borrow::Cow;

use crate::messages::Timestamp;
use serde::{Deserialize, Serialize};

use super::{
    Compression, Endian, Image, ImageMessage, RawImageEncoding, UnknownCompressionError,
    UnknownEncodingError,
};

/// An error that occurs while decoding an OMG IDL message.
#[derive(Debug, thiserror::Error)]
pub enum OmgidlDecodeError {
    /// Failed to parse CDR message.
    #[error(transparent)]
    Cdr(#[from] cdr::Error),
    /// The timestamp cannot be represented (excess nanoseconds overflow the seconds field).
    #[error("timestamp out of range")]
    InvalidTimestamp,
    /// Unknown raw image encoding.
    #[error(transparent)]
    UnknownEncoding(#[from] UnknownEncodingError),
    /// Unknown compression codec.
    #[error(transparent)]
    UnknownCompression(#[from] UnknownCompressionError),
}

/// A `foxglove::Time` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct OmgidlTime {
    sec: u32,
    nsec: u32,
}
impl TryFrom<OmgidlTime> for Timestamp {
    type Error = OmgidlDecodeError;

    fn try_from(value: OmgidlTime) -> Result<Self, Self::Error> {
        Timestamp::new_checked(value.sec, value.nsec).ok_or(OmgidlDecodeError::InvalidTimestamp)
    }
}

/// A `foxglove::RawImage` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OmgidlRawImage<'a> {
    timestamp: OmgidlTime,
    frame_id: Cow<'a, str>,
    width: u32,
    height: u32,
    encoding: Cow<'a, str>,
    step: u32,
    data: Cow<'a, [u8]>,
}
impl<'a> OmgidlRawImage<'a> {
    /// Decodes a `foxglove::RawImage`.
    pub fn decode(data: &'a [u8]) -> Result<Self, OmgidlDecodeError> {
        Ok(cdr::deserialize::<Self>(data)?)
    }
}
impl<'a> TryFrom<OmgidlRawImage<'a>> for ImageMessage<'a> {
    type Error = OmgidlDecodeError;

    fn try_from(image: OmgidlRawImage<'a>) -> std::result::Result<Self, Self::Error> {
        // Pixel values in Foxglove RawImage messages are always little-endian.
        let encoding = RawImageEncoding::parse_endian(&image.encoding, Endian::Little)?;
        Ok(Self {
            timestamp: Some(image.timestamp.try_into()?),
            frame_id: image.frame_id.into(),
            image: Image::Raw(super::RawImage {
                encoding,
                width: image.width,
                height: image.height,
                stride: image.step,
                data: image.data,
            }),
        })
    }
}

/// A `foxglove::CompressedImage` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OmgidlCompressedImage<'a> {
    timestamp: OmgidlTime,
    frame_id: Cow<'a, str>,
    data: Cow<'a, [u8]>,
    format: Cow<'a, str>,
}
impl<'a> OmgidlCompressedImage<'a> {
    /// Decodes a `foxglove::CompressedImage`.
    pub fn decode(data: &'a [u8]) -> Result<Self, OmgidlDecodeError> {
        Ok(cdr::deserialize::<Self>(data)?)
    }
}
impl<'a> TryFrom<OmgidlCompressedImage<'a>> for ImageMessage<'a> {
    type Error = OmgidlDecodeError;

    fn try_from(image: OmgidlCompressedImage<'a>) -> std::result::Result<Self, Self::Error> {
        let compression = Compression::try_from_ros_format(&image.format)?;
        Ok(ImageMessage {
            timestamp: Some(image.timestamp.try_into()?),
            frame_id: image.frame_id.into(),
            image: Image::Compressed(super::CompressedImage {
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

    #[test]
    fn test_roundtrip_raw_image() {
        let image = OmgidlRawImage {
            timestamp: OmgidlTime {
                sec: 100,
                nsec: 200,
            },
            frame_id: "frame".into(),
            height: 16,
            width: 16,
            encoding: "mono8".into(),
            step: 16,
            data: Cow::Owned((0..=255).collect()),
        };
        let encoded = cdr::serialize::<_, _, CdrLe>(&image, Infinite).unwrap();
        let decoded: OmgidlRawImage = cdr::deserialize(&encoded).unwrap();
        assert_eq!(image, decoded);
    }

    #[test]
    fn test_roundtrip_compressed_image() {
        let image = OmgidlCompressedImage {
            timestamp: OmgidlTime {
                sec: 100,
                nsec: 200,
            },
            frame_id: "frame".into(),
            data: Cow::Owned((0..=255).collect()),
            format: "png".into(),
        };
        let encoded = cdr::serialize::<_, _, CdrLe>(&image, Infinite).unwrap();
        let decoded: OmgidlCompressedImage = cdr::deserialize(&encoded).unwrap();
        assert_eq!(image, decoded);
    }

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_compressed_image_into_message() {
        let image = OmgidlCompressedImage {
            timestamp: OmgidlTime { sec: 1, nsec: 2 },
            frame_id: "frame".into(),
            data: Cow::Owned(vec![0, 1, 2, 3]),
            format: "png".into(),
        };
        let msg = ImageMessage::try_from(image).unwrap();
        assert_eq!(msg.frame_id, "frame");
        match msg.image {
            Image::Compressed(image) => assert_eq!(image.compression, Compression::Png),
            other => panic!("expected compressed image, got {other:?}"),
        }
    }

    #[test]
    fn test_raw_image_rejects_overflowing_timestamp() {
        // Excess nanoseconds carry into seconds, overflowing the u32 seconds field.
        let image = OmgidlRawImage {
            timestamp: OmgidlTime {
                sec: u32::MAX,
                nsec: 1_000_000_000,
            },
            frame_id: "frame".into(),
            height: 1,
            width: 1,
            encoding: "mono8".into(),
            step: 1,
            data: Cow::Owned(vec![0]),
        };
        let err = ImageMessage::try_from(image).unwrap_err();
        assert!(matches!(err, OmgidlDecodeError::InvalidTimestamp));
    }
}
