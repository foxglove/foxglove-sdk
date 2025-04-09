use std::borrow::Cow;

use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

/// Client message data message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-message-data>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageData<'a> {
    /// Channel ID.
    pub channel_id: u32,
    /// Message data.
    pub data: Cow<'a, [u8]>,
}

impl<'a> MessageData<'a> {
    /// Creates a new message data message.
    pub fn new(channel_id: u32, data: impl Into<Cow<'a, [u8]>>) -> Self {
        Self {
            channel_id,
            data: data.into(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> MessageData<'static> {
        MessageData {
            channel_id: self.channel_id,
            data: Cow::Owned(self.data.into_owned()),
        }
    }
}

impl<'a> BinaryMessage<'a> for MessageData<'a> {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 4 {
            return Err(ParseError::BufferTooShort);
        }
        let channel_id = data.get_u32_le();
        Ok(Self {
            channel_id,
            data: Cow::Borrowed(data),
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let size = 1 + 4 + self.data.len();
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::MessageData as u8);
        buf.put_u32_le(self.channel_id);
        buf.put_slice(&self.data);
        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> MessageData<'static> {
        MessageData::new(30, br#"{"key": "value"}"#)
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
        assert_eq!(msg, ClientMessage::MessageData(orig));
    }
}
