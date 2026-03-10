//! Tungstenite support.

use tokio_tungstenite::tungstenite::Message;

use crate::protocol::v1::{BinaryMessage, JsonMessage, ParseError, client, server};

impl<'a> TryFrom<&'a Message> for client::ClientMessage<'a> {
    type Error = ParseError;

    fn try_from(msg: &'a Message) -> Result<Self, Self::Error> {
        match msg {
            Message::Text(utf8) => Self::parse_json(utf8),
            Message::Binary(bytes) => Self::parse_binary(bytes),
            _ => Err(ParseError::UnhandledMessageType),
        }
    }
}

impl<'a> TryFrom<&'a Message> for server::ServerMessage<'a> {
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

impl From<&client::MessageData<'_>> for Message {
    fn from(value: &client::MessageData<'_>) -> Self {
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

impl From<&client::Subscribe> for Message {
    fn from(value: &client::Subscribe) -> Self {
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

impl From<&client::Unsubscribe> for Message {
    fn from(value: &client::Unsubscribe) -> Self {
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

impl From<&server::MessageData<'_>> for Message {
    fn from(value: &server::MessageData<'_>) -> Self {
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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tokio_tungstenite::tungstenite::Message;

    use crate::protocol::v1::{BinaryMessage, ParseError, client, server};

    // --- TryFrom<&Message> for ClientMessage ---

    #[test]
    fn test_client_message_try_from_text() {
        let msg = client::Subscribe::new([client::Subscription::new(1, 10)]);
        let json = serde_json::to_string(&msg).unwrap();
        let ws_msg = Message::Text(json.into());
        let parsed = client::ClientMessage::try_from(&ws_msg).unwrap();
        assert_eq!(parsed, client::ClientMessage::Subscribe(msg));
    }

    #[test]
    fn test_client_message_try_from_binary() {
        let msg = client::MessageData::new(30, br#"{"key": "value"}"#);
        let bytes = msg.to_bytes();
        let ws_msg = Message::Binary(bytes.into());
        let parsed = client::ClientMessage::try_from(&ws_msg).unwrap();
        assert_eq!(parsed, client::ClientMessage::MessageData(msg));
    }

    #[test]
    fn test_client_message_try_from_unhandled() {
        let ws_msg = Message::Ping(vec![].into());
        assert_matches!(
            client::ClientMessage::try_from(&ws_msg),
            Err(ParseError::UnhandledMessageType)
        );
    }

    // --- TryFrom<&Message> for ServerMessage ---

    #[test]
    fn test_server_message_try_from_text() {
        let msg = server::ServerInfo::new("test server");
        let json = serde_json::to_string(&msg).unwrap();
        let ws_msg = Message::Text(json.into());
        let parsed = server::ServerMessage::try_from(&ws_msg).unwrap();
        assert_eq!(parsed, server::ServerMessage::ServerInfo(msg));
    }

    #[test]
    fn test_server_message_try_from_binary() {
        let msg = server::Time::new(1234567890);
        let bytes = msg.to_bytes();
        let ws_msg = Message::Binary(bytes.into());
        let parsed = server::ServerMessage::try_from(&ws_msg).unwrap();
        assert_eq!(parsed, server::ServerMessage::Time(msg));
    }

    #[test]
    fn test_server_message_try_from_unhandled() {
        let ws_msg = Message::Ping(vec![].into());
        assert_matches!(
            server::ServerMessage::try_from(&ws_msg),
            Err(ParseError::UnhandledMessageType)
        );
    }
}
