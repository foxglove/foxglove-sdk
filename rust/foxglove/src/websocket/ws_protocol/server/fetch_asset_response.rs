//! Fetch asset response types.

use std::borrow::Cow;

use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

/// Fetch asset response message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset-response>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchAssetResponse<'a> {
    /// Request ID.
    pub request_id: u32,
    /// Payload.
    pub payload: Payload<'a>,
}

impl<'a> FetchAssetResponse<'a> {
    /// Creates a new error message response.
    pub fn error_message(request_id: u32, message: impl Into<Cow<'a, str>>) -> Self {
        Self {
            request_id,
            payload: Payload::ErrorMessage(message.into()),
        }
    }

    /// Creates a new asset data response.
    pub fn asset_data(request_id: u32, data: impl Into<Cow<'a, [u8]>>) -> Self {
        Self {
            request_id,
            payload: Payload::AssetData(data.into()),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> FetchAssetResponse<'static> {
        FetchAssetResponse {
            request_id: self.request_id,
            payload: self.payload.into_owned(),
        }
    }
}

impl<'a> BinaryMessage<'a> for FetchAssetResponse<'a> {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 4 + 1 + 4 {
            return Err(ParseError::BufferTooShort);
        }
        let request_id = data.get_u32_le();
        let raw_status = data.get_u8();
        let Some(status) = Status::from_repr(raw_status) else {
            return Err(ParseError::InvalidFetchAssetStatus(raw_status));
        };
        let error_message_len = data.get_u32_le() as usize;
        if data.len() < error_message_len {
            return Err(ParseError::BufferTooShort);
        }
        let payload = match status {
            Status::Success => {
                data.advance(error_message_len);
                Payload::AssetData(Cow::Borrowed(data))
            }
            Status::Error => {
                let msg = std::str::from_utf8(&data[..error_message_len])?;
                Payload::ErrorMessage(Cow::Borrowed(msg))
            }
        };
        Ok(Self {
            request_id,
            payload,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let (status, error_message_len, payload_len, payload) = match &self.payload {
            Payload::ErrorMessage(msg) => (Status::Error, msg.len(), msg.len(), msg.as_bytes()),
            Payload::AssetData(data) => (Status::Success, 0, data.len(), data.as_ref()),
        };
        let size = 1 + 4 + 1 + 4 + payload_len;
        let mut buf = Vec::with_capacity(size);
        buf.put_u8(BinaryOpcode::FetchAssetResponse as u8);
        buf.put_u32_le(self.request_id);
        buf.put_u8(status as u8);
        buf.put_u32_le(error_message_len as u32);
        buf.put_slice(payload);
        buf
    }
}

/// Status code.
#[repr(u8)]
enum Status {
    Success = 0,
    Error = 1,
}
impl Status {
    fn from_repr(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Success),
            1 => Some(Self::Error),
            _ => None,
        }
    }
}

/// Payload for a [`FetchAssetResponse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload<'a> {
    /// An error response.
    ErrorMessage(Cow<'a, str>),
    /// A successful response.
    AssetData(Cow<'a, [u8]>),
}

impl Payload<'_> {
    /// Returns an owned version of this payload.
    pub fn into_owned(self) -> Payload<'static> {
        match self {
            Payload::ErrorMessage(msg) => Payload::ErrorMessage(Cow::Owned(msg.into_owned())),
            Payload::AssetData(data) => Payload::AssetData(Cow::Owned(data.into_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn asset_data() -> FetchAssetResponse<'static> {
        FetchAssetResponse::asset_data(10, b"data")
    }

    fn error_message() -> FetchAssetResponse<'static> {
        FetchAssetResponse::error_message(10, "oh no")
    }

    #[test]
    fn test_encode_asset_data() {
        insta::assert_snapshot!(format!("{:#04x?}", asset_data().to_bytes()));
    }

    #[test]
    fn test_encode_error_message() {
        insta::assert_snapshot!(format!("{:#04x?}", error_message().to_bytes()));
    }

    #[test]
    fn test_parse() {
        assert_matches!(
            FetchAssetResponse::parse_binary(b""),
            Err(ParseError::BufferTooShort)
        );
        assert_matches!(
            FetchAssetResponse::parse_binary(&[0; 8]),
            Err(ParseError::BufferTooShort)
        );

        let mut buf = Vec::new();
        buf.put_u32_le(10);
        buf.put_u8(2);
        buf.put_u32_le(0);
        assert_matches!(
            FetchAssetResponse::parse_binary(&buf),
            Err(ParseError::InvalidFetchAssetStatus(2))
        );

        let mut buf = Vec::new();
        buf.put_u32_le(10);
        buf.put_u8(1);
        buf.put_u32_le(1);
        assert_matches!(
            FetchAssetResponse::parse_binary(&buf),
            Err(ParseError::BufferTooShort)
        );
    }

    #[test]
    fn test_roundtrip_asset_data() {
        let orig = asset_data();
        let buf = orig.to_bytes();
        let msg = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ServerMessage::FetchAssetResponse(orig));
    }

    #[test]
    fn test_roundtrip_error_message() {
        let orig = error_message();
        let buf = orig.to_bytes();
        let msg = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ServerMessage::FetchAssetResponse(orig));
    }
}
