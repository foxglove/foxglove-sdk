//! Server info message types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Server info message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#server-info>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "serverInfo", rename_all = "camelCase")]
pub struct ServerInfo {
    /// Free-form information about the server.
    pub name: String,
    /// The optional features supported by this server.
    pub capabilities: Vec<Capability>,
    /// The encodings that may be used for client-side publishing or service call
    /// requests/responses. Only set if client publishing or services are supported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_encodings: Vec<String>,
    /// Optional map of key-value pairs.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
    /// Optional string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}
impl ServerInfo {
    /// Creates a new server info message.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            capabilities: vec![],
            supported_encodings: vec![],
            metadata: HashMap::default(),
            session_id: None,
        }
    }

    /// Sets advertised capabilities.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    /// Sets supported encodings.
    #[must_use]
    pub fn with_supported_encodings(
        mut self,
        encodings: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.supported_encodings = encodings.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Sets session ID.
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

impl JsonMessage for ServerInfo {}

/// A capability advertised in a [`ServerInfo`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to get & set parameters.
    Parameters,
    /// Allow clients to subscribe to parameter changes.
    ParametersSubscribe,
    /// The server may publish binary time messages.
    Time,
    /// Allow clients to call services.
    Services,
    /// Allow clients to subscribe to updates to the connection graph.
    ConnectionGraph,
    /// Allow clients to fetch assets.
    Assets,
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> ServerInfo {
        ServerInfo::new("example server")
    }

    fn message_full() -> ServerInfo {
        message()
            .with_capabilities([Capability::ClientPublish, Capability::Time])
            .with_supported_encodings(["json"])
            .with_metadata(maplit::hashmap! {
                "key".into() => "value".into(),
            })
            .with_session_id("1675789422160")
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_encode_full() {
        insta::assert_json_snapshot!(message_full());
    }

    fn test_roundtrip_inner(orig: ServerInfo) {
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::ServerInfo(orig));
    }

    #[test]
    fn test_roundtrip() {
        test_roundtrip_inner(message());
    }

    #[test]
    fn test_roundtrip_full() {
        test_roundtrip_inner(message_full());
    }
}
