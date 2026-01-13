//! Client messages for Foxglove protocol v1.

use bytes::Buf;
use serde::Deserialize;

use crate::protocol::common::client::BinaryOpcode;
use crate::protocol::{BinaryMessage, ParseError};

pub mod subscribe;
mod unsubscribe;

pub use crate::protocol::common::client::advertise;
pub use crate::protocol::common::client::{
    Advertise, FetchAsset, GetParameters, MessageData, ServiceCallRequest, SetParameters,
    SubscribeConnectionGraph, SubscribeParameterUpdates, Unadvertise, UnsubscribeConnectionGraph,
    UnsubscribeParameterUpdates,
};
#[doc(hidden)]
pub use crate::protocol::common::client::{PlaybackCommand, PlaybackControlRequest};
pub use subscribe::{Subscribe, Subscription};
pub use unsubscribe::Unsubscribe;

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
                    MessageData::parse_binary(data).map(ClientMessage::MessageData)
                }
                Some(BinaryOpcode::ServiceCallRequest) => {
                    ServiceCallRequest::parse_binary(data).map(ClientMessage::ServiceCallRequest)
                }
                Some(BinaryOpcode::PlaybackControlRequest) => {
                    PlaybackControlRequest::parse_binary(data)
                        .map(ClientMessage::PlaybackControlRequest)
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
