//! Subscribe message types.

use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};

use crate::protocol::common::client::BinaryOpcode;
use crate::protocol::{BinaryMessage, ParseError};

/// Subscribe to a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscribe", rename_all = "camelCase")]
pub struct Subscribe {
    /// Channel IDs to subscribe to.
    pub channel_ids: Vec<u32>,
}

impl Subscribe {
    /// Creates a new subscribe message.
    pub fn new(channel_ids: impl IntoIterator<Item = u32>) -> Self {
        Self {
            channel_ids: channel_ids.into_iter().collect(),
        }
    }
}

impl BinaryMessage<'_> for Subscribe {
    fn parse_binary(mut data: &[u8]) -> Result<Self, ParseError> {
        if data.len() < 4 {
            return Err(ParseError::BufferTooShort);
        }
        let count = data.get_u32_le() as usize;
        if data.len() < count * 4 {
            return Err(ParseError::BufferTooShort);
        }
        let mut channel_ids = Vec::with_capacity(count);
        for _ in 0..count {
            channel_ids.push(data.get_u32_le());
        }
        Ok(Self { channel_ids })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let size = 1 + 4 + self.channel_ids.len() * 4;
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::Subscribe as u8);
        buf.put_u32_le(self.channel_ids.len() as u32);
        for &channel_id in &self.channel_ids {
            buf.put_u32_le(channel_id);
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::v2::client::ClientMessage;

    use super::*;

    fn message() -> Subscribe {
        Subscribe::new([10, 20, 30])
    }

    #[test]
    fn test_encode() {
        insta::assert_snapshot!(format!("{:#04x?}", message().to_bytes()));
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_bytes();
        let msg = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ClientMessage::Subscribe(orig));
    }
}
