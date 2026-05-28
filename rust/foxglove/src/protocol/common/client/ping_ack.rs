use bytes::{Buf, BufMut};

use crate::protocol::{BinaryPayload, ParseError};

/// Enables the device to compute its own round-trip time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingAck {
    /// Milliseconds since epoch.
    pub device_timestamp: u64,
}

impl<'a> BinaryPayload<'a> for PingAck {
    fn parse_payload(data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 8 {
            return Err(ParseError::BufferTooShort);
        }
        let mut buf = data;
        let device_timestamp = buf.get_u64_le();
        Ok(Self { device_timestamp })
    }

    fn payload_size(&self) -> usize {
        8
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        buf.put_u64_le(self.device_timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let orig = PingAck {
            device_timestamp: 1_711_500_000_000,
        };
        let mut buf = Vec::new();
        orig.write_payload(&mut buf);
        let parsed = PingAck::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_buffer_too_short() {
        let result = PingAck::parse_payload(&[0x01, 0x02, 0x03]);
        assert!(result.is_err());
    }
}
