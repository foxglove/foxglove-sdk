use std::borrow::Cow;

use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

/// Message data message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#message-data>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageData<'a> {
    /// Subscription ID.
    pub subscription_id: u32,
    /// Log time.
    pub log_time: u64,
    /// Message data.
    pub data: Cow<'a, [u8]>,
}

impl<'a> MessageData<'a> {
    /// Creates a new message data message.
    pub fn new(subscription_id: u32, log_time: u64, data: impl Into<Cow<'a, [u8]>>) -> Self {
        Self {
            subscription_id,
            log_time,
            data: data.into(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> MessageData<'static> {
        MessageData {
            subscription_id: self.subscription_id,
            log_time: self.log_time,
            data: Cow::Owned(self.data.into_owned()),
        }
    }
}

impl<'a> BinaryMessage<'a> for MessageData<'a> {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 4 + 8 {
            return Err(ParseError::BufferTooShort);
        }
        let subscription_id = data.get_u32_le();
        let log_time = data.get_u64_le();
        Ok(Self {
            subscription_id,
            log_time,
            data: Cow::Borrowed(data),
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let size = 1 + 4 + 8 + self.data.len();
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::MessageData as u8);
        buf.put_u32_le(self.subscription_id);
        buf.put_u64_le(self.log_time);
        buf.put_slice(&self.data);
        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> MessageData<'static> {
        MessageData {
            subscription_id: 30,
            log_time: 1234,
            data: br#"{"key": "value"}"#.into(),
        }
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
        assert_eq!(msg, ServerMessage::MessageData(orig));
    }
}
