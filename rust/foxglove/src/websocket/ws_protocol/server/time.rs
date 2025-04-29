use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

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

impl<'a> BinaryMessage<'a> for Time {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 8 {
            return Err(ParseError::BufferTooShort);
        }
        let timestamp = data.get_u64_le();
        Ok(Self { timestamp })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let size = 1 + 8;
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::Time as u8);
        buf.put_u64_le(self.timestamp);
        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> Time {
        Time::new(1234567890)
    }

    #[test]
    fn test_encode() {
        insta::assert_snapshot!(format!("{:#04x?}", message().to_bytes()));
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_bytes();
        let msg = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ServerMessage::Time(orig));
    }
}
