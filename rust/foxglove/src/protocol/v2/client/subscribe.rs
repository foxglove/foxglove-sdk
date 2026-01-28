//! Subscribe message types.

use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};

use crate::protocol::{BinaryPayload, ParseError};

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

impl BinaryPayload<'_> for Subscribe {
    fn parse_payload(mut data: &[u8]) -> Result<Self, ParseError> {
        if data.len() < 4 {
            return Err(ParseError::BufferTooShort);
        }
        let count = data.get_u32_le() as usize;
        let required_len = count.checked_mul(4).ok_or(ParseError::BufferTooShort)?;
        if data.len() < required_len {
            return Err(ParseError::BufferTooShort);
        }
        let mut channel_ids = Vec::with_capacity(count);
        for _ in 0..count {
            channel_ids.push(data.get_u32_le());
        }
        Ok(Self { channel_ids })
    }

    fn payload_size(&self) -> usize {
        4 + self.channel_ids.len() * 4
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        buf.put_u32_le(self.channel_ids.len() as u32);
        for &channel_id in &self.channel_ids {
            buf.put_u32_le(channel_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::v2::client::ClientMessage;
    use crate::protocol::v2::message::BinaryMessage;

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
