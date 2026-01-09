//! Server messages for Foxglove protocol v1.

use bytes::Buf;
use serde::Deserialize;

use crate::protocol::common::server::BinaryOpcode;
use crate::protocol::{BinaryMessage, ParseError};

mod message_data;

// Re-export common messages for consumers using v1
pub use crate::protocol::common::server::advertise;
pub use crate::protocol::common::server::advertise_services;
pub use crate::protocol::common::server::connection_graph_update;
pub use crate::protocol::common::server::fetch_asset_response;
#[doc(hidden)]
pub use crate::protocol::common::server::playback_state;
pub use crate::protocol::common::server::server_info;
pub use crate::protocol::common::server::status;

#[doc(hidden)]
pub use crate::protocol::common::server::PlaybackState;
pub use crate::protocol::common::server::{
    Advertise, AdvertiseServices, Channel, ConnectionGraphUpdate, FetchAssetResponse,
    ParameterValues, RemoveStatus, ServerInfo, ServiceCallFailure, ServiceCallResponse, Status,
    Time, Unadvertise, UnadvertiseServices,
};
pub use message_data::MessageDataV1;

/// A representation of a server message useful for deserializing.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum ServerMessageV1<'a> {
    ServerInfo(ServerInfo),
    Status(Status),
    RemoveStatus(RemoveStatus),
    Advertise(Advertise<'a>),
    Unadvertise(Unadvertise),
    MessageData(MessageDataV1<'a>),
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

impl<'a> ServerMessageV1<'a> {
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
                    MessageDataV1::parse_binary(data).map(ServerMessageV1::MessageData)
                }
                Some(BinaryOpcode::Time) => Time::parse_binary(data).map(ServerMessageV1::Time),
                Some(BinaryOpcode::ServiceCallResponse) => ServiceCallResponse::parse_binary(data)
                    .map(ServerMessageV1::ServiceCallResponse),
                Some(BinaryOpcode::FetchAssetResponse) => {
                    FetchAssetResponse::parse_binary(data).map(ServerMessageV1::FetchAssetResponse)
                }
                Some(BinaryOpcode::PlaybackState) => {
                    PlaybackState::parse_binary(data).map(ServerMessageV1::PlaybackState)
                }
                None => Err(ParseError::InvalidOpcode(opcode)),
            }
        }
    }

    /// Returns a server message with a static lifetime.
    #[allow(dead_code)]
    pub fn into_owned(self) -> ServerMessageV1<'static> {
        match self {
            ServerMessageV1::ServerInfo(m) => ServerMessageV1::ServerInfo(m),
            ServerMessageV1::Status(m) => ServerMessageV1::Status(m),
            ServerMessageV1::RemoveStatus(m) => ServerMessageV1::RemoveStatus(m),
            ServerMessageV1::Advertise(m) => ServerMessageV1::Advertise(m.into_owned()),
            ServerMessageV1::Unadvertise(m) => ServerMessageV1::Unadvertise(m),
            ServerMessageV1::MessageData(m) => ServerMessageV1::MessageData(m.into_owned()),
            ServerMessageV1::Time(m) => ServerMessageV1::Time(m),
            ServerMessageV1::ParameterValues(m) => ServerMessageV1::ParameterValues(m),
            ServerMessageV1::AdvertiseServices(m) => {
                ServerMessageV1::AdvertiseServices(m.into_owned())
            }
            ServerMessageV1::UnadvertiseServices(m) => ServerMessageV1::UnadvertiseServices(m),
            ServerMessageV1::ServiceCallResponse(m) => {
                ServerMessageV1::ServiceCallResponse(m.into_owned())
            }
            ServerMessageV1::ConnectionGraphUpdate(m) => ServerMessageV1::ConnectionGraphUpdate(m),
            ServerMessageV1::FetchAssetResponse(m) => {
                ServerMessageV1::FetchAssetResponse(m.into_owned())
            }
            ServerMessageV1::ServiceCallFailure(m) => ServerMessageV1::ServiceCallFailure(m),
            ServerMessageV1::PlaybackState(m) => ServerMessageV1::PlaybackState(m),
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
    Advertise(Advertise<'a>),
    Unadvertise(Unadvertise),
    ParameterValues(ParameterValues),
    #[serde(borrow)]
    AdvertiseServices(AdvertiseServices<'a>),
    UnadvertiseServices(UnadvertiseServices),
    ConnectionGraphUpdate(ConnectionGraphUpdate),
    ServiceCallFailure(ServiceCallFailure),
}

impl<'a> From<JsonMessage<'a>> for ServerMessageV1<'a> {
    fn from(m: JsonMessage<'a>) -> Self {
        match m {
            JsonMessage::ServerInfo(m) => Self::ServerInfo(m),
            JsonMessage::Status(m) => Self::Status(m),
            JsonMessage::RemoveStatus(m) => Self::RemoveStatus(m),
            JsonMessage::Advertise(m) => Self::Advertise(m),
            JsonMessage::Unadvertise(m) => Self::Unadvertise(m),
            JsonMessage::ParameterValues(m) => Self::ParameterValues(m),
            JsonMessage::AdvertiseServices(m) => Self::AdvertiseServices(m.into_owned()),
            JsonMessage::UnadvertiseServices(m) => Self::UnadvertiseServices(m),
            JsonMessage::ConnectionGraphUpdate(m) => Self::ConnectionGraphUpdate(m),
            JsonMessage::ServiceCallFailure(m) => Self::ServiceCallFailure(m),
        }
    }
}
