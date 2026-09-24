//! Shared ROS 1 wire-format helpers.
//!
//! ROS 1 messages are serialized little-endian with no alignment padding; strings and
//! variable-length arrays are prefixed with a u32 length. Both the image decoder
//! ([`crate::img2yuv::ros1`]) and the point-cloud decoder
//! (`remote_access::point_cloud_transcode::ros1`) read messages with these helpers, so the
//! wire layout of the `std_msgs/Header` prefix has exactly one home.

use bytes::Buf;

use crate::messages::Timestamp;

/// An error that occurs while reading a ROS 1 message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Ros1WireError {
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
    /// The timestamp cannot be represented (excess nanoseconds overflow the seconds field).
    #[error("timestamp out of range")]
    InvalidTimestamp,
}
impl From<bytes::TryGetError> for Ros1WireError {
    fn from(e: bytes::TryGetError) -> Self {
        Ros1WireError::UnexpectedEof {
            want: e.requested,
            avail: e.available,
        }
    }
}

pub(crate) trait Ros1BufExt<'a>: Buf {
    /// Reads a counted byte buffer from a ROS 1 message.
    fn try_get_ros1_bytes(&mut self) -> Result<&'a [u8], Ros1WireError>;

    /// Reads a counted string from a ROS 1 message.
    fn try_get_ros1_str(&mut self) -> Result<&'a str, Ros1WireError> {
        let bytes = self.try_get_ros1_bytes()?;
        let str = std::str::from_utf8(bytes)?;
        Ok(str)
    }

    /// Reads a ROS 1 header message.
    fn try_get_ros1_header(&mut self) -> Result<Ros1Header<'a>, Ros1WireError> {
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
    fn try_get_ros1_bytes(&mut self) -> Result<&'a [u8], Ros1WireError> {
        let len = self.try_get_u32_le()? as usize;
        if self.remaining() < len {
            return Err(Ros1WireError::UnexpectedEof {
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
pub(crate) struct Ros1Header<'a> {
    #[allow(dead_code)]
    pub(crate) seq: u32,
    pub(crate) sec: u32,
    pub(crate) nsec: u32,
    pub(crate) frame_id: &'a str,
}
impl Ros1Header<'_> {
    /// Returns the header timestamp, rejecting values that overflow the seconds field.
    pub(crate) fn timestamp(&self) -> Result<Timestamp, Ros1WireError> {
        Timestamp::new_checked(self.sec, self.nsec).ok_or(Ros1WireError::InvalidTimestamp)
    }
}
