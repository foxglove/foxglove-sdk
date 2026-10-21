//! Server messages for Foxglove protocol v1.

use bytes::Buf;
use serde::Deserialize;

use crate::protocol::{BinaryMessage, BinaryPayload, ParseError};

mod message_data;

// Re-export common messages for consumers using v1
pub use crate::protocol::common::server::advertise;
pub use crate::protocol::common::server::advertise_services;
pub use crate::protocol::common::server::advertise_v2;
pub use crate::protocol::common::server::connection_graph_update;
pub use crate::protocol::common::server::fetch_asset_response;
#[doc(hidden)]
pub use crate::protocol::common::server::playback_state;
pub use crate::protocol::common::server::server_info;
pub use crate::protocol::common::server::status;

#[doc(hidden)]
pub use crate::protocol::common::server::PlaybackState;
pub use crate::protocol::common::server::{
    Advertise, AdvertiseServices, AdvertiseV2, Channel, ConnectionGraphUpdate, FetchAssetResponse,
    ParameterValues, RemoveStatus, ServerInfo, ServiceCallFailure, ServiceCallResponse, Status,
    Time, Unadvertise, UnadvertiseServices,
};
pub use message_data::MessageData;

/// Binary opcodes for v1 server messages.
#[repr(u8)]
pub(crate) enum BinaryOpcode {
    MessageData = 1,
    Time = 2,
    ServiceCallResponse = 3,
    FetchAssetResponse = 4,
    #[doc(hidden)]
    PlaybackState = 5,
}

impl BinaryOpcode {
    pub(crate) fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::Time),
            3 => Some(Self::ServiceCallResponse),
            4 => Some(Self::FetchAssetResponse),
            5 => Some(Self::PlaybackState),
            _ => None,
        }
    }
}

impl<'a> BinaryMessage<'a> for MessageData<'a> {
    const OPCODE: u8 = BinaryOpcode::MessageData as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::common::server::playback_state::PlaybackStatus;
    use assert_matches::assert_matches;

    #[test]
    fn test_time_encode() {
        let message = Time::new(1234567890);
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::Time(message));
    }

    #[test]
    fn test_service_call_response_encode() {
        let message = ServiceCallResponse {
            service_id: 10,
            call_id: 12,
            encoding: "json".into(),
            payload: br#"{"key": "value"}"#.into(),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::ServiceCallResponse(message));
    }

    #[test]
    fn test_fetch_asset_response_encode_asset_data() {
        let message = FetchAssetResponse::asset_data(10, b"data");
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::FetchAssetResponse(message));
    }

    #[test]
    fn test_fetch_asset_response_encode_error_message() {
        let message = FetchAssetResponse::error_message(10, "oh no");
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::FetchAssetResponse(message));
    }

    #[test]
    fn test_playback_state_encode_playing() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            did_seek: false,
            current_time: 12345,
            request_id: None,
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::PlaybackState(message));
    }

    #[test]
    fn test_playback_state_encode_did_seek() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            did_seek: true,
            current_time: 12345,
            request_id: None,
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::PlaybackState(message));
    }

    #[test]
    fn test_playback_state_encode_playing_with_request_id() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            did_seek: false,
            current_time: 12345,
            request_id: Some("i-am-a-request".to_string()),
        };
        let buf = message.to_bytes();
        insta::assert_snapshot!(format!("{:#04x?}", buf));
        let parsed = ServerMessage::parse_binary(&buf).unwrap();
        assert_eq!(parsed, ServerMessage::PlaybackState(message));
    }

    #[test]
    fn test_parse_json_advertise_v1() {
        let json = r#"{"op":"advertise","channels":[
            {"id":1,"topic":"/t","encoding":"json","schemaName":"","schema":""}
        ]}"#;
        let parsed = ServerMessage::parse_json(json).unwrap();
        assert_matches!(parsed, ServerMessage::Advertise(msg) => {
            assert_eq!(msg.channels.len(), 1);
            assert_eq!(msg.channels[0].topic, "/t");
        });
    }

    #[test]
    fn test_parse_json_advertise_v2() {
        let json = r#"{"op":"advertise",
            "schemas":[{"id":1,"name":"S","encoding":"jsonschema","data":"{}"}],
            "channels":[{"id":1,"topic":"/t","encoding":"json","schemaId":1}]
        }"#;
        let parsed = ServerMessage::parse_json(json).unwrap();
        assert_matches!(parsed, ServerMessage::AdvertiseV2(msg) => {
            assert_eq!(msg.schemas.len(), 1);
            assert_eq!(msg.schemas[0].id, 1);
            assert_eq!(msg.channels[0].schema_id, Some(1));
        });

        // A v2 advertisement whose schemas were all sent previously carries no schema table.
        let json = r#"{"op":"advertise","channels":[{"id":1,"topic":"/t","encoding":"json"}]}"#;
        let parsed = ServerMessage::parse_json(json).unwrap();
        assert_matches!(parsed, ServerMessage::AdvertiseV2(msg) => {
            assert!(msg.schemas.is_empty());
            assert_eq!(msg.channels[0].schema_id, None);
        });
    }

    #[test]
    fn test_parse_binary_empty() {
        assert_matches!(
            ServerMessage::parse_binary(b""),
            Err(ParseError::EmptyBinaryMessage)
        );
    }

    #[test]
    fn test_parse_binary_invalid_opcode() {
        assert_matches!(
            ServerMessage::parse_binary(&[0xff]),
            Err(ParseError::InvalidOpcode(0xff))
        );
    }
}

/// A representation of a server message useful for deserializing.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum ServerMessage<'a> {
    ServerInfo(ServerInfo),
    Status(Status),
    RemoveStatus(RemoveStatus),
    Advertise(Advertise<'a>),
    AdvertiseV2(AdvertiseV2<'a>),
    Unadvertise(Unadvertise),
    MessageData(MessageData<'a>),
    Time(Time),
    ParameterValues(ParameterValues),
    AdvertiseServices(AdvertiseServices<'a>),
    UnadvertiseServices(UnadvertiseServices),
    ServiceCallResponse(ServiceCallResponse<'a>),
    ConnectionGraphUpdate(ConnectionGraphUpdate),
    FetchAssetResponse(FetchAssetResponse<'a>),
    ServiceCallFailure(ServiceCallFailure),
    PlaybackState(PlaybackState),
}

impl<'a> ServerMessage<'a> {
    /// Parses a server message from JSON.
    pub fn parse_json(json: &'a str) -> Result<Self, ParseError> {
        let msg = serde_json::from_str::<JsonMessage>(json)?;
        Ok(msg.into())
    }

    /// Parses a server message from a binary buffer.
    pub fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.is_empty() {
            Err(ParseError::EmptyBinaryMessage)
        } else {
            let opcode = data.get_u8();
            match BinaryOpcode::from_repr(opcode) {
                Some(BinaryOpcode::MessageData) => {
                    MessageData::parse_payload(data).map(ServerMessage::MessageData)
                }
                Some(BinaryOpcode::Time) => Time::parse_payload(data).map(ServerMessage::Time),
                Some(BinaryOpcode::ServiceCallResponse) => {
                    ServiceCallResponse::parse_payload(data).map(ServerMessage::ServiceCallResponse)
                }
                Some(BinaryOpcode::FetchAssetResponse) => {
                    FetchAssetResponse::parse_payload(data).map(ServerMessage::FetchAssetResponse)
                }
                Some(BinaryOpcode::PlaybackState) => {
                    PlaybackState::parse_payload(data).map(ServerMessage::PlaybackState)
                }
                None => Err(ParseError::InvalidOpcode(opcode)),
            }
        }
    }

    /// Returns a server message with a static lifetime.
    #[allow(dead_code)]
    pub fn into_owned(self) -> ServerMessage<'static> {
        match self {
            ServerMessage::ServerInfo(m) => ServerMessage::ServerInfo(m),
            ServerMessage::Status(m) => ServerMessage::Status(m),
            ServerMessage::RemoveStatus(m) => ServerMessage::RemoveStatus(m),
            ServerMessage::Advertise(m) => ServerMessage::Advertise(m.into_owned()),
            ServerMessage::AdvertiseV2(m) => ServerMessage::AdvertiseV2(m.into_owned()),
            ServerMessage::Unadvertise(m) => ServerMessage::Unadvertise(m),
            ServerMessage::MessageData(m) => ServerMessage::MessageData(m.into_owned()),
            ServerMessage::Time(m) => ServerMessage::Time(m),
            ServerMessage::ParameterValues(m) => ServerMessage::ParameterValues(m),
            ServerMessage::AdvertiseServices(m) => ServerMessage::AdvertiseServices(m.into_owned()),
            ServerMessage::UnadvertiseServices(m) => ServerMessage::UnadvertiseServices(m),
            ServerMessage::ServiceCallResponse(m) => {
                ServerMessage::ServiceCallResponse(m.into_owned())
            }
            ServerMessage::ConnectionGraphUpdate(m) => ServerMessage::ConnectionGraphUpdate(m),
            ServerMessage::FetchAssetResponse(m) => {
                ServerMessage::FetchAssetResponse(m.into_owned())
            }
            ServerMessage::ServiceCallFailure(m) => ServerMessage::ServiceCallFailure(m),
            ServerMessage::PlaybackState(m) => ServerMessage::PlaybackState(m),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum JsonMessage<'a> {
    ServerInfo(ServerInfo),
    Status(Status),
    RemoveStatus(RemoveStatus),
    #[serde(borrow)]
    Advertise(AnyAdvertise<'a>),
    Unadvertise(Unadvertise),
    ParameterValues(ParameterValues),
    #[serde(borrow)]
    AdvertiseServices(AdvertiseServices<'a>),
    UnadvertiseServices(UnadvertiseServices),
    ConnectionGraphUpdate(ConnectionGraphUpdate),
    ServiceCallFailure(ServiceCallFailure),
}

/// The `advertise` op has two shapes, selected by the negotiated subprotocol.
///
/// The v1 shape is tried first: its required `schemaName` and `schema` fields are absent from v2
/// channels, so a v2 message never matches it.
#[derive(Deserialize)]
#[serde(untagged)]
enum AnyAdvertise<'a> {
    #[serde(borrow)]
    V1(Advertise<'a>),
    #[serde(borrow)]
    V2(AdvertiseV2<'a>),
}

impl<'a> From<JsonMessage<'a>> for ServerMessage<'a> {
    fn from(m: JsonMessage<'a>) -> Self {
        match m {
            JsonMessage::ServerInfo(m) => Self::ServerInfo(m),
            JsonMessage::Status(m) => Self::Status(m),
            JsonMessage::RemoveStatus(m) => Self::RemoveStatus(m),
            JsonMessage::Advertise(AnyAdvertise::V1(m)) => Self::Advertise(m),
            JsonMessage::Advertise(AnyAdvertise::V2(m)) => Self::AdvertiseV2(m),
            JsonMessage::Unadvertise(m) => Self::Unadvertise(m),
            JsonMessage::ParameterValues(m) => Self::ParameterValues(m),
            JsonMessage::AdvertiseServices(m) => Self::AdvertiseServices(m.into_owned()),
            JsonMessage::UnadvertiseServices(m) => Self::UnadvertiseServices(m),
            JsonMessage::ConnectionGraphUpdate(m) => Self::ConnectionGraphUpdate(m),
            JsonMessage::ServiceCallFailure(m) => Self::ServiceCallFailure(m),
        }
    }
}
