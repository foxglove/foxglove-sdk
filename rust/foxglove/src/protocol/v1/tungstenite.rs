//! Tungstenite support.

use tokio_tungstenite::tungstenite::Message;

use crate::protocol::v1::{client, server, BinaryMessage, JsonMessage, ParseError};

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

    use crate::protocol::v1::{client, server, BinaryMessage, ParseError};

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

    // --- From<&T> for Message: client messages ---

    #[test]
    fn test_from_client_advertise() {
        let msg = client::Advertise::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_fetch_asset() {
        let msg = client::FetchAsset::new(1, "package://example.urdf");
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_get_parameters() {
        let msg = client::GetParameters::new(["p1"]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_message_data() {
        let msg = client::MessageData::new(30, b"data" as &[u8]);
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_client_service_call_request() {
        let msg = client::ServiceCallRequest {
            service_id: 1,
            call_id: 1,
            encoding: "json".into(),
            payload: b"{}".into(),
        };
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_client_playback_control_request() {
        let msg = client::PlaybackControlRequest {
            playback_command: client::PlaybackCommand::Play,
            playback_speed: 1.0,
            seek_time: None,
            request_id: "req-1".to_string(),
        };
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_client_set_parameters() {
        let msg = client::SetParameters::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_subscribe() {
        let msg = client::Subscribe::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_subscribe_connection_graph() {
        let msg = client::SubscribeConnectionGraph {};
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_subscribe_parameter_updates() {
        let msg = client::SubscribeParameterUpdates::new(["p1"]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_unadvertise() {
        let msg = client::Unadvertise::new([1, 2]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_unsubscribe() {
        let msg = client::Unsubscribe::new([1, 2]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_unsubscribe_connection_graph() {
        let msg = client::UnsubscribeConnectionGraph {};
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_client_unsubscribe_parameter_updates() {
        let msg = client::UnsubscribeParameterUpdates::new(["p1"]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    // --- From<&T> for Message: server messages ---

    #[test]
    fn test_from_server_advertise() {
        let msg = server::Advertise::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_advertise_services() {
        let msg = server::AdvertiseServices::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_connection_graph_update() {
        let msg = server::ConnectionGraphUpdate::default();
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_fetch_asset_response() {
        let msg = server::FetchAssetResponse::asset_data(1, b"data" as &[u8]);
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_server_message_data() {
        let msg = server::MessageData::new(30, 12345, b"data" as &[u8]);
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_server_parameter_values() {
        let msg = server::ParameterValues::new([]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_playback_state() {
        use crate::protocol::common::server::playback_state::PlaybackStatus;
        let msg = server::PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            did_seek: false,
            request_id: None,
        };
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_server_remove_status() {
        let msg = server::RemoveStatus::new(["id-1"]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_server_info() {
        let msg = server::ServerInfo::new("test");
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_service_call_failure() {
        let msg = server::ServiceCallFailure::new(1, 2, "error");
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_service_call_response() {
        let msg = server::ServiceCallResponse {
            service_id: 1,
            call_id: 1,
            encoding: "json".into(),
            payload: b"{}".into(),
        };
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_server_status() {
        let msg = server::Status::info("test");
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_time() {
        let msg = server::Time::new(1234567890);
        assert_matches!(Message::from(&msg), Message::Binary(_));
    }

    #[test]
    fn test_from_server_unadvertise() {
        let msg = server::Unadvertise::new([1u64, 2u64]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }

    #[test]
    fn test_from_server_unadvertise_services() {
        let msg = server::UnadvertiseServices::new([1, 2]);
        assert_matches!(Message::from(&msg), Message::Text(_));
    }
}
