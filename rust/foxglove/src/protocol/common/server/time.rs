use bytes::{Buf, BufMut};

use crate::protocol::{BinaryMessage, BinaryPayload, ParseError};

/// Time message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#time>.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    /// Timestamp in nanoseconds.
    pub timestamp: u64,
}

impl Time {
    /// Creates a new time message.
    pub fn new(timestamp: u64) -> Self {
        Self { timestamp }
    }
}

impl<'a> BinaryPayload<'a> for Time {
    fn parse_payload(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 8 {
            return Err(ParseError::BufferTooShort);
        }
        let timestamp = data.get_u64_le();
        Ok(Self { timestamp })
    }

    fn payload_size(&self) -> usize {
        8
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        buf.put_u64_le(self.timestamp);
    }
}

impl BinaryMessage<'_> for Time {
    const OPCODE: u8 = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Time {
        Time::new(1234567890)
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let mut buf = Vec::new();
        BinaryPayload::write_payload(&orig, &mut buf);
        let parsed = Time::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }
}
