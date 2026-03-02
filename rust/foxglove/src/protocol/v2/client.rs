//! Client messages for Foxglove protocol v2

use bytes::Buf;
use serde::Deserialize;

use crate::protocol::{BinaryMessage, BinaryPayload, ParseError};

pub mod subscribe;
mod unsubscribe;

#[doc(hidden)]
pub use crate::protocol::common::client::PlaybackControlRequest;
pub use crate::protocol::common::client::{
    Advertise, FetchAsset, GetParameters, MessageData, ServiceCallRequest, SetParameters,
    SubscribeParameterUpdates, Unadvertise, UnsubscribeParameterUpdates,
};
pub use subscribe::Subscribe;
pub use unsubscribe::Unsubscribe;

/// Binary opcodes for v2 client messages.
#[repr(u8)]
pub(crate) enum BinaryOpcode {
    MessageData = 1,
    ServiceCallRequest = 2,
    #[doc(hidden)]
    PlaybackControlRequest = 3,
    Subscribe = 4,
    Unsubscribe = 5,
}

impl BinaryOpcode {
    pub(crate) fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::ServiceCallRequest),
            3 => Some(Self::PlaybackControlRequest),
            4 => Some(Self::Subscribe),
            5 => Some(Self::Unsubscribe),
            _ => None,
        }
    }
}

impl BinaryMessage<'_> for Subscribe {
    const OPCODE: u8 = BinaryOpcode::Subscribe as u8;
}

impl BinaryMessage<'_> for Unsubscribe {
    const OPCODE: u8 = BinaryOpcode::Unsubscribe as u8;
}

/// A representation of a client message useful for deserializing.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum ClientMessage<'a> {
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Advertise(Advertise<'a>),
    Unadvertise(Unadvertise),
    MessageData(MessageData<'a>),
    GetParameters(GetParameters),
    SetParameters(SetParameters),
    SubscribeParameterUpdates(SubscribeParameterUpdates),
    UnsubscribeParameterUpdates(UnsubscribeParameterUpdates),
    ServiceCallRequest(ServiceCallRequest<'a>),
    SubscribeConnectionGraph,
    UnsubscribeConnectionGraph,
    FetchAsset(FetchAsset),
    #[doc(hidden)]
    PlaybackControlRequest(PlaybackControlRequest),
}

impl<'a> ClientMessage<'a> {
    /// Parses a client message from JSON.
    pub fn parse_json(json: &'a str) -> Result<Self, ParseError> {
        let msg = serde_json::from_str::<JsonMessage>(json)?;
        Ok(msg.into())
    }

    /// Parses a client message from a binary buffer.
    pub fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.is_empty() {
            Err(ParseError::EmptyBinaryMessage)
        } else {
            let opcode = data.get_u8();
            match BinaryOpcode::from_repr(opcode) {
                Some(BinaryOpcode::MessageData) => {
                    MessageData::parse_payload(data).map(ClientMessage::MessageData)
                }
                Some(BinaryOpcode::ServiceCallRequest) => {
                    ServiceCallRequest::parse_payload(data).map(ClientMessage::ServiceCallRequest)
                }
                Some(BinaryOpcode::PlaybackControlRequest) => {
                    PlaybackControlRequest::parse_payload(data)
                        .map(ClientMessage::PlaybackControlRequest)
                }
                Some(BinaryOpcode::Subscribe) => {
                    Subscribe::parse_payload(data).map(ClientMessage::Subscribe)
                }
                Some(BinaryOpcode::Unsubscribe) => {
                    Unsubscribe::parse_payload(data).map(ClientMessage::Unsubscribe)
                }
                None => Err(ParseError::InvalidOpcode(opcode)),
            }
        }
    }

    /// Returns a client message with a static lifetime.
    #[allow(dead_code)]
    pub fn into_owned(self) -> ClientMessage<'static> {
        match self {
            ClientMessage::Subscribe(m) => ClientMessage::Subscribe(m),
            ClientMessage::Unsubscribe(m) => ClientMessage::Unsubscribe(m),
            ClientMessage::Advertise(m) => ClientMessage::Advertise(m.into_owned()),
            ClientMessage::Unadvertise(m) => ClientMessage::Unadvertise(m),
            ClientMessage::MessageData(m) => ClientMessage::MessageData(m.into_owned()),
            ClientMessage::GetParameters(m) => ClientMessage::GetParameters(m),
            ClientMessage::SetParameters(m) => ClientMessage::SetParameters(m),
            ClientMessage::SubscribeParameterUpdates(m) => {
                ClientMessage::SubscribeParameterUpdates(m)
            }
            ClientMessage::UnsubscribeParameterUpdates(m) => {
                ClientMessage::UnsubscribeParameterUpdates(m)
            }
            ClientMessage::ServiceCallRequest(m) => {
                ClientMessage::ServiceCallRequest(m.into_owned())
            }
            ClientMessage::SubscribeConnectionGraph => ClientMessage::SubscribeConnectionGraph,
            ClientMessage::UnsubscribeConnectionGraph => ClientMessage::UnsubscribeConnectionGraph,
            ClientMessage::FetchAsset(m) => ClientMessage::FetchAsset(m),
            ClientMessage::PlaybackControlRequest(m) => ClientMessage::PlaybackControlRequest(m),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum JsonMessage<'a> {
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    #[serde(borrow)]
    Advertise(Advertise<'a>),
    Unadvertise(Unadvertise),
    GetParameters(GetParameters),
    SetParameters(SetParameters),
    SubscribeParameterUpdates(SubscribeParameterUpdates),
    UnsubscribeParameterUpdates(UnsubscribeParameterUpdates),
    SubscribeConnectionGraph,
    UnsubscribeConnectionGraph,
    FetchAsset(FetchAsset),
}

impl<'a> From<JsonMessage<'a>> for ClientMessage<'a> {
    fn from(m: JsonMessage<'a>) -> Self {
        match m {
            JsonMessage::Subscribe(m) => Self::Subscribe(m),
            JsonMessage::Unsubscribe(m) => Self::Unsubscribe(m),
            JsonMessage::Advertise(m) => Self::Advertise(m),
            JsonMessage::Unadvertise(m) => Self::Unadvertise(m),
            JsonMessage::GetParameters(m) => Self::GetParameters(m),
            JsonMessage::SetParameters(m) => Self::SetParameters(m),
            JsonMessage::SubscribeParameterUpdates(m) => Self::SubscribeParameterUpdates(m),
            JsonMessage::UnsubscribeParameterUpdates(m) => Self::UnsubscribeParameterUpdates(m),
            JsonMessage::SubscribeConnectionGraph => Self::SubscribeConnectionGraph,
            JsonMessage::UnsubscribeConnectionGraph => Self::UnsubscribeConnectionGraph,
            JsonMessage::FetchAsset(m) => Self::FetchAsset(m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::common::client::PlaybackCommand;
    use assert_matches::assert_matches;

    #[test]
    fn test_message_data_encode() {
        let message = MessageData::new(30, br#"{"key": "value"}"#);
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::MessageData(message));
    }

    #[test]
    fn test_service_call_request_encode() {
        let message = ServiceCallRequest {
            service_id: 10,
            call_id: 12,
            encoding: "json".into(),
            payload: br#"{"key": "value"}"#.into(),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::ServiceCallRequest(message));
    }

    #[test]
    fn test_playback_control_request_encode() {
        let message = PlaybackControlRequest {
            playback_command: PlaybackCommand::Play,
            playback_speed: 1.0,
            seek_time: None,
            request_id: "some-id".to_string(),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::PlaybackControlRequest(message));
    }

    #[test]
    fn test_playback_control_request_encode_play_with_seek() {
        let message = PlaybackControlRequest {
            playback_command: PlaybackCommand::Play,
            playback_speed: 1.0,
            seek_time: Some(123_456_789),
            request_id: "some-id".to_string(),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::PlaybackControlRequest(message));
    }

    #[test]
    fn test_playback_control_request_encode_pause() {
        let message = PlaybackControlRequest {
            playback_command: PlaybackCommand::Pause,
            playback_speed: 1.0,
            seek_time: None,
            request_id: "some-id".to_string(),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::PlaybackControlRequest(message));
    }

    #[test]
    fn test_subscribe_encode() {
        let message = Subscribe::new([10, 20, 30]);
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::Subscribe(message));
    }

    #[test]
    fn test_unsubscribe_encode() {
        let message = Unsubscribe::new([1, 2, 3]);
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::Unsubscribe(message));
    }

    #[test]
    fn test_parse_binary_empty() {
        assert_matches!(
            ClientMessage::parse_binary(b""),
            Err(ParseError::EmptyBinaryMessage)
        );
    }

    #[test]
    fn test_parse_binary_invalid_opcode() {
        assert_matches!(
            ClientMessage::parse_binary(&[0xff]),
            Err(ParseError::InvalidOpcode(0xff))
        );
    }
}
