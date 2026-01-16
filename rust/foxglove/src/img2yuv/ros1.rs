//! ROS 1 message decoder.

use crate::schemas::Timestamp;
use bytes::Buf;

use super::{
    CompressedImage, Compression, Endian, Image, ImageMessage, RawImage, RawImageEncoding,
    UnknownCompressionError, UnknownEncodingError,
};

/// An error that occurs while decoding a ROS 1 message.
#[derive(Debug, thiserror::Error)]
pub enum Ros1DecodeError {
    /// Expected more bytes than are present in the buffer.
    #[error("expected {want} more bytes, but only have {avail}")]
    UnexpectedEof {
        /// Number of bytes needed.
        want: usize,
        /// Number of bytes remaining in buffer.
        avail: usize,
    },
    /// Invalid UTF-8 string.
    #[error("ros1 string is not valid utf-8")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    /// Unknown raw image encoding.
    #[error(transparent)]
    UnknownEncoding(#[from] UnknownEncodingError),
    /// Unknown compression codec.
    #[error(transparent)]
    UnknownCompression(#[from] UnknownCompressionError),
}
impl From<bytes::TryGetError> for Ros1DecodeError {
    fn from(e: bytes::TryGetError) -> Self {
        Ros1DecodeError::UnexpectedEof {
            want: e.requested,
            avail: e.available,
        }
    }
}

trait Ros1BufExt<'a>: Buf {
    /// Reads a counted byte buffer from a ROS 1 message.
    fn try_get_ros1_bytes(&mut self) -> Result<&'a [u8], Ros1DecodeError>;

    /// Reads a counted string from a ROS 1 message.
    fn try_get_ros1_str(&mut self) -> Result<&'a str, Ros1DecodeError> {
        let bytes = self.try_get_ros1_bytes()?;
        let str = std::str::from_utf8(bytes)?;
        Ok(str)
    }

    /// Reads a ROS 1 header message.
    fn try_get_ros1_header(&mut self) -> Result<Ros1Header<'a>, Ros1DecodeError> {
        let seq = self.try_get_u32_le()?;
        let sec = self.try_get_u32_le()?;
        let nsec = self.try_get_u32_le()?;
        let frame_id = self.try_get_ros1_str()?;
        Ok(Ros1Header {
            seq,
            sec,
            nsec,
            frame_id,
        })
    }
}
impl<'a> Ros1BufExt<'a> for &'a [u8] {
    fn try_get_ros1_bytes(&mut self) -> Result<&'a [u8], Ros1DecodeError> {
        let len = self.try_get_u32_le()? as usize;
        if self.remaining() < len {
            return Err(Ros1DecodeError::UnexpectedEof {
                want: len,
                avail: self.remaining(),
            });
        }
        let bytes = &self[..len];
        self.advance(len);
        Ok(bytes)
    }
}

/// A ROS 1 `std_msgs/Header` message.
#[derive(Debug, PartialEq, Eq)]
struct Ros1Header<'a> {
    #[allow(dead_code)]
    seq: u32,
    sec: u32,
    nsec: u32,
    frame_id: &'a str,
}

/// A ROS 1 `sensor_msgs/Image` message.
#[derive(Debug, PartialEq, Eq)]
pub struct Ros1Image<'a> {
    header: Ros1Header<'a>,
    height: u32,
    width: u32,
    encoding: &'a str,
    is_bigendian: u8,
    step: u32,
    data: &'a [u8],
}
impl<'a> Ros1Image<'a> {
    /// Decodes a ROS 1 image.
    pub fn decode(mut data: &'a [u8]) -> Result<Self, Ros1DecodeError> {
        let header = data.try_get_ros1_header()?;
        let height = data.try_get_u32_le()?;
        let width = data.try_get_u32_le()?;
        let encoding = data.try_get_ros1_str()?;
        let is_bigendian = data.try_get_u8()?;
        let step = data.try_get_u32_le()?;
        let data = data.try_get_ros1_bytes()?;
        Ok(Self {
            header,
            height,
            width,
            encoding,
            is_bigendian,
            step,
            data,
        })
    }
}
impl<'a> TryFrom<Ros1Image<'a>> for ImageMessage<'a> {
    type Error = Ros1DecodeError;

    fn try_from(image: Ros1Image<'a>) -> std::result::Result<Self, Self::Error> {
        let endian = if image.is_bigendian > 0 {
            Endian::Big
        } else {
            Endian::Little
        };
        let encoding = RawImageEncoding::parse_endian(image.encoding, endian)?;
        Ok(Self {
            timestamp: Some(Timestamp::new(image.header.sec, image.header.nsec)),
            frame_id: image.header.frame_id.to_string(),
            image: Image::Raw(RawImage {
                encoding,
                width: image.width,
                height: image.height,
                stride: image.step,
                data: image.data.into(),
            }),
        })
    }
}

/// A ROS 1 `sensor_msgs/CompressedImage` message.
#[derive(Debug, PartialEq, Eq)]
pub struct Ros1CompressedImage<'a> {
    header: Ros1Header<'a>,
    format: &'a str,
    data: &'a [u8],
}
impl<'a> Ros1CompressedImage<'a> {
    /// Decodes a ROS 1 compressed image.
    pub fn decode(mut data: &'a [u8]) -> Result<Self, Ros1DecodeError> {
        let header = data.try_get_ros1_header()?;
        let format = data.try_get_ros1_str()?;
        let data = data.try_get_ros1_bytes()?;
        Ok(Self {
            header,
            format,
            data,
        })
    }
}
impl<'a> TryFrom<Ros1CompressedImage<'a>> for ImageMessage<'a> {
    type Error = Ros1DecodeError;

    fn try_from(image: Ros1CompressedImage<'a>) -> std::result::Result<Self, Self::Error> {
        let compression = Compression::try_from_ros_format(image.format)?;
        Ok(ImageMessage {
            timestamp: Some(Timestamp::new(image.header.sec, image.header.nsec)),
            frame_id: image.header.frame_id.to_string(),
            image: Image::Compressed(CompressedImage {
                compression,
                data: image.data.into(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    fn make_header() -> Ros1Header<'static> {
        Ros1Header {
            seq: 42,
            sec: 100,
            nsec: 200,
            frame_id: "frame",
        }
    }

    fn write_header(buf: &mut impl BufMut, header: &Ros1Header) {
        buf.put_u32_le(header.seq);
        buf.put_u32_le(header.sec);
        buf.put_u32_le(header.nsec);
        buf.put_u32_le(header.frame_id.len() as u32);
        buf.put_slice(header.frame_id.as_bytes());
    }

    #[test]
    fn test_roundtrip_image() {
        let image = Ros1Image {
            header: make_header(),
            height: 16,
            width: 16,
            encoding: "mono8",
            is_bigendian: 0,
            step: 16,
            data: &(0..=255).collect::<Vec<u8>>(),
        };
        let mut encoded = vec![];
        let buf = &mut encoded;
        write_header(buf, &image.header);
        buf.put_u32_le(image.height);
        buf.put_u32_le(image.width);
        buf.put_u32_le(image.encoding.len() as u32);
        buf.put_slice(image.encoding.as_bytes());
        buf.put_u8(image.is_bigendian);
        buf.put_u32_le(image.step);
        buf.put_u32_le(image.data.len() as u32);
        buf.put_slice(image.data);
        let decoded = Ros1Image::decode(&encoded).unwrap();
        assert_eq!(image, decoded);
    }

    #[test]
    fn test_roundtrip_compressed_image() {
        let image = Ros1CompressedImage {
            header: make_header(),
            format: "special",
            data: &(0..=255).collect::<Vec<u8>>(),
        };
        let mut encoded = vec![];
        let buf = &mut encoded;
        write_header(buf, &image.header);
        buf.put_u32_le(image.format.len() as u32);
        buf.put_slice(image.format.as_bytes());
        buf.put_u32_le(image.data.len() as u32);
        buf.put_slice(image.data);
        let decoded = Ros1CompressedImage::decode(&encoded).unwrap();
        assert_eq!(image, decoded);
    }
}
