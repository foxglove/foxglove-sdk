//! Client messages.

use bytes::Buf;
use serde::Deserialize;

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

pub mod advertise;
mod fetch_asset;
mod get_parameters;
mod message_data;
mod service_call_request;
mod set_parameters;
pub mod subscribe;
mod subscribe_connection_graph;
mod subscribe_parameter_updates;
mod unadvertise;
mod unsubscribe;
mod unsubscribe_connection_graph;
mod unsubscribe_parameter_updates;

pub use advertise::Advertise;
pub use fetch_asset::FetchAsset;
pub use get_parameters::GetParameters;
pub use message_data::MessageData;
pub use service_call_request::ServiceCallRequest;
pub use set_parameters::SetParameters;
pub use subscribe::Subscribe;
pub use subscribe_connection_graph::SubscribeConnectionGraph;
pub use subscribe_parameter_updates::SubscribeParameterUpdates;
pub use unadvertise::Unadvertise;
pub use unsubscribe::Unsubscribe;
pub use unsubscribe_connection_graph::UnsubscribeConnectionGraph;
pub use unsubscribe_parameter_updates::UnsubscribeParameterUpdates;

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
                    MessageData::parse_binary(data).map(ClientMessage::MessageData)
                }
                Some(BinaryOpcode::ServiceCallRequest) => {
                    ServiceCallRequest::parse_binary(data).map(ClientMessage::ServiceCallRequest)
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

#[repr(u8)]
enum BinaryOpcode {
    MessageData = 1,
    ServiceCallRequest = 2,
}
impl BinaryOpcode {
    fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::ServiceCallRequest),
            _ => None,
        }
    }
}
