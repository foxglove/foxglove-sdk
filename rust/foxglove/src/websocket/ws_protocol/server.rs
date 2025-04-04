//! Server messages.

use bytes::Buf;
use serde::Deserialize;

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

pub mod advertise;
pub mod advertise_services;
pub mod connection_graph_update;
pub mod fetch_asset_response;
mod message_data;
mod parameter_values;
mod remove_status;
pub mod server_info;
mod service_call_failure;
mod service_call_response;
pub mod status;
mod time;
mod unadvertise;
mod unadvertise_services;

pub use advertise::Advertise;
pub use advertise_services::AdvertiseServices;
pub use connection_graph_update::ConnectionGraphUpdate;
pub use fetch_asset_response::FetchAssetResponse;
pub use message_data::MessageData;
pub use parameter_values::ParameterValues;
pub use remove_status::RemoveStatus;
pub use server_info::ServerInfo;
pub use service_call_failure::ServiceCallFailure;
pub use service_call_response::ServiceCallResponse;
pub use status::Status;
pub use time::Time;
pub use unadvertise::Unadvertise;
pub use unadvertise_services::UnadvertiseServices;

/// A representation of a server message useful for deserializing.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum ServerMessage<'a> {
    ServerInfo(ServerInfo),
    Status(Status),
    RemoveStatus(RemoveStatus),
    Advertise(Advertise<'a>),
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
                    MessageData::parse_binary(data).map(ServerMessage::MessageData)
                }
                Some(BinaryOpcode::Time) => Time::parse_binary(data).map(ServerMessage::Time),
                Some(BinaryOpcode::ServiceCallResponse) => {
                    ServiceCallResponse::parse_binary(data).map(ServerMessage::ServiceCallResponse)
                }
                Some(BinaryOpcode::FetchAssetResponse) => {
                    FetchAssetResponse::parse_binary(data).map(ServerMessage::FetchAssetResponse)
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

impl<'a> From<JsonMessage<'a>> for ServerMessage<'a> {
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

#[repr(u8)]
enum BinaryOpcode {
    MessageData = 1,
    Time = 2,
    ServiceCallResponse = 3,
    FetchAssetResponse = 4,
}
impl BinaryOpcode {
    fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::Time),
            3 => Some(Self::ServiceCallResponse),
            4 => Some(Self::FetchAssetResponse),
            _ => None,
        }
    }
}
