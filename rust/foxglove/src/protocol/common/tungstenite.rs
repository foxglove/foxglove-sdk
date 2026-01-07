//! Tungstenite support.

use tokio_tungstenite::tungstenite::Message;

use crate::{
    protocol::v1::{client, server},
    protocol::{BinaryMessage, JsonMessage, ParseError},
};

impl<'a> TryFrom<&'a Message> for client::ClientMessageV1<'a> {
    type Error = ParseError;

    fn try_from(msg: &'a Message) -> Result<Self, Self::Error> {
        match msg {
            Message::Text(utf8) => Self::parse_json(utf8),
            Message::Binary(bytes) => Self::parse_binary(bytes),
            _ => Err(ParseError::UnhandledMessageType),
        }
    }
}

impl<'a> TryFrom<&'a Message> for server::ServerMessageV1<'a> {
    type Error = ParseError;

    fn try_from(msg: &'a Message) -> Result<Self, Self::Error> {
        match msg {
            Message::Text(utf8) => Self::parse_json(utf8),
            Message::Binary(bytes) => Self::parse_binary(bytes),
            _ => Err(ParseError::UnhandledMessageType),
        }
    }
}

impl From<&client::Advertise<'_>> for Message {
    fn from(value: &client::Advertise<'_>) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::FetchAsset> for Message {
    fn from(value: &client::FetchAsset) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::GetParameters> for Message {
    fn from(value: &client::GetParameters) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::MessageDataV1<'_>> for Message {
    fn from(value: &client::MessageDataV1<'_>) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&client::ServiceCallRequest<'_>> for Message {
    fn from(value: &client::ServiceCallRequest<'_>) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&client::PlaybackControlRequest> for Message {
    fn from(value: &client::PlaybackControlRequest) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&client::SetParameters> for Message {
    fn from(value: &client::SetParameters) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::SubscribeV1> for Message {
    fn from(value: &client::SubscribeV1) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::SubscribeConnectionGraph> for Message {
    fn from(value: &client::SubscribeConnectionGraph) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::SubscribeParameterUpdates> for Message {
    fn from(value: &client::SubscribeParameterUpdates) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::Unadvertise> for Message {
    fn from(value: &client::Unadvertise) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::UnsubscribeV1> for Message {
    fn from(value: &client::UnsubscribeV1) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::UnsubscribeConnectionGraph> for Message {
    fn from(value: &client::UnsubscribeConnectionGraph) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&client::UnsubscribeParameterUpdates> for Message {
    fn from(value: &client::UnsubscribeParameterUpdates) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::Advertise<'_>> for Message {
    fn from(value: &server::Advertise<'_>) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::AdvertiseServices<'_>> for Message {
    fn from(value: &server::AdvertiseServices<'_>) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::ConnectionGraphUpdate> for Message {
    fn from(value: &server::ConnectionGraphUpdate) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::FetchAssetResponse<'_>> for Message {
    fn from(value: &server::FetchAssetResponse<'_>) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&server::MessageDataV1<'_>> for Message {
    fn from(value: &server::MessageDataV1<'_>) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&server::ParameterValues> for Message {
    fn from(value: &server::ParameterValues) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::PlaybackState> for Message {
    fn from(value: &server::PlaybackState) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&server::RemoveStatus> for Message {
    fn from(value: &server::RemoveStatus) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::ServerInfo> for Message {
    fn from(value: &server::ServerInfo) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::ServiceCallFailure> for Message {
    fn from(value: &server::ServiceCallFailure) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::ServiceCallResponse<'_>> for Message {
    fn from(value: &server::ServiceCallResponse<'_>) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&server::Status> for Message {
    fn from(value: &server::Status) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::Time> for Message {
    fn from(value: &server::Time) -> Self {
        Message::Binary(value.to_bytes().into())
    }
}

impl From<&server::Unadvertise> for Message {
    fn from(value: &server::Unadvertise) -> Self {
        Message::Text(value.to_string().into())
    }
}

impl From<&server::UnadvertiseServices> for Message {
    fn from(value: &server::UnadvertiseServices) -> Self {
        Message::Text(value.to_string().into())
    }
}
