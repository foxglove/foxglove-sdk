//! Client messages for Foxglove protocol v1.

use bytes::Buf;
use serde::Deserialize;

use crate::protocol::{BinaryMessage, ParseError};
use crate::protocol::common::client::BinaryOpcode;

mod message_data;
pub mod subscribe;
mod unsubscribe;

pub use crate::protocol::common::client::{
    Advertise, FetchAsset, GetParameters, ServiceCallRequest, SetParameters,
    SubscribeConnectionGraph, SubscribeParameterUpdates, Unadvertise, UnsubscribeConnectionGraph,
    UnsubscribeParameterUpdates,
};
pub use crate::protocol::common::client::advertise;
#[doc(hidden)]
pub use crate::protocol::common::client::{PlaybackCommand, PlaybackControlRequest};
pub use message_data::MessageDataV1;
pub use subscribe::{SubscribeV1, Subscription};
pub use unsubscribe::UnsubscribeV1;

/// A representation of a client message useful for deserializing.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum ClientMessageV1<'a> {
    Subscribe(SubscribeV1),
    Unsubscribe(UnsubscribeV1),
    Advertise(Advertise<'a>),
    Unadvertise(Unadvertise),
    MessageData(MessageDataV1<'a>),
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

impl<'a> ClientMessageV1<'a> {
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
                    MessageDataV1::parse_binary(data).map(ClientMessageV1::MessageData)
                }
                Some(BinaryOpcode::ServiceCallRequest) => {
                    ServiceCallRequest::parse_binary(data).map(ClientMessageV1::ServiceCallRequest)
                }
                Some(BinaryOpcode::PlaybackControlRequest) => {
                    PlaybackControlRequest::parse_binary(data)
                        .map(ClientMessageV1::PlaybackControlRequest)
                }
                None => Err(ParseError::InvalidOpcode(opcode)),
            }
        }
    }

    /// Returns a client message with a static lifetime.
    #[allow(dead_code)]
    pub fn into_owned(self) -> ClientMessageV1<'static> {
        match self {
            ClientMessageV1::Subscribe(m) => ClientMessageV1::Subscribe(m),
            ClientMessageV1::Unsubscribe(m) => ClientMessageV1::Unsubscribe(m),
            ClientMessageV1::Advertise(m) => ClientMessageV1::Advertise(m.into_owned()),
            ClientMessageV1::Unadvertise(m) => ClientMessageV1::Unadvertise(m),
            ClientMessageV1::MessageData(m) => ClientMessageV1::MessageData(m.into_owned()),
            ClientMessageV1::GetParameters(m) => ClientMessageV1::GetParameters(m),
            ClientMessageV1::SetParameters(m) => ClientMessageV1::SetParameters(m),
            ClientMessageV1::SubscribeParameterUpdates(m) => {
                ClientMessageV1::SubscribeParameterUpdates(m)
            }
            ClientMessageV1::UnsubscribeParameterUpdates(m) => {
                ClientMessageV1::UnsubscribeParameterUpdates(m)
            }
            ClientMessageV1::ServiceCallRequest(m) => {
                ClientMessageV1::ServiceCallRequest(m.into_owned())
            }
            ClientMessageV1::SubscribeConnectionGraph => ClientMessageV1::SubscribeConnectionGraph,
            ClientMessageV1::UnsubscribeConnectionGraph => {
                ClientMessageV1::UnsubscribeConnectionGraph
            }
            ClientMessageV1::FetchAsset(m) => ClientMessageV1::FetchAsset(m),
            ClientMessageV1::PlaybackControlRequest(m) => {
                ClientMessageV1::PlaybackControlRequest(m)
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum JsonMessage<'a> {
    Subscribe(SubscribeV1),
    Unsubscribe(UnsubscribeV1),
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

impl<'a> From<JsonMessage<'a>> for ClientMessageV1<'a> {
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
