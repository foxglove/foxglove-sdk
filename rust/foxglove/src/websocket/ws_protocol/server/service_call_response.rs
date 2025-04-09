use std::borrow::Cow;

use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

/// Service call response message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#service-call-response>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCallResponse<'a> {
    /// Service ID.
    pub service_id: u32,
    /// Call ID.
    pub call_id: u32,
    /// Encoding.
    pub encoding: Cow<'a, str>,
    /// Payload.
    pub payload: Cow<'a, [u8]>,
}

impl ServiceCallResponse<'_> {
    /// Gives the message a static lifetime by cloning borrowed references.
    pub fn into_owned(self) -> ServiceCallResponse<'static> {
        ServiceCallResponse {
            service_id: self.service_id,
            call_id: self.call_id,
            encoding: Cow::Owned(self.encoding.into_owned()),
            payload: Cow::Owned(self.payload.into_owned()),
        }
    }
}

impl<'a> BinaryMessage<'a> for ServiceCallResponse<'a> {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 4 + 4 + 4 {
            return Err(ParseError::BufferTooShort);
        }
        let service_id = data.get_u32_le();
        let call_id = data.get_u32_le();
        let encoding_len = data.get_u32_le() as usize;
        if data.len() < encoding_len {
            return Err(ParseError::BufferTooShort);
        }
        let encoding = Cow::Borrowed(std::str::from_utf8(&data[..encoding_len])?);
        data.advance(encoding_len);
        Ok(Self {
            service_id,
            call_id,
            encoding,
            payload: Cow::Borrowed(data),
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let size = 1 + 4 + 4 + 4 + self.encoding.len() + self.payload.len();
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::ServiceCallResponse as u8);
        buf.put_u32_le(self.service_id);
        buf.put_u32_le(self.call_id);
        buf.put_u32_le(self.encoding.len() as u32);
        buf.put_slice(self.encoding.as_bytes());
        buf.put_slice(&self.payload);
        buf
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> ServiceCallResponse<'static> {
        ServiceCallResponse {
            service_id: 10,
            call_id: 12,
            encoding: "json".into(),
            payload: br#"{"key": "value"}"#.into(),
        }
    }

    #[test]
    fn test_encode() {
        insta::assert_snapshot!(format!("{:#04x?}", message().to_bytes()));
    }

    #[test]
    fn test_parse() {
        assert_matches!(
            ServiceCallResponse::parse_binary(b""),
            Err(ParseError::BufferTooShort)
        );
        assert_matches!(
            ServiceCallResponse::parse_binary(&[0; 11]),
            Err(ParseError::BufferTooShort)
        );
        let mut buf = Vec::new();
        buf.put_u32_le(10);
        buf.put_u32_le(12);
        buf.put_u32_le(1);
        assert_matches!(
            ServiceCallResponse::parse_binary(&buf),
            Err(ParseError::BufferTooShort)
        );
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_bytes();
        let msg = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ServerMessage::ServiceCallResponse(orig));
    }
}
