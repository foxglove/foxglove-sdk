//! Server info message types.

use std::collections::HashMap;

use crate::JsonMessage;
use serde::{Deserialize, Serialize};

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
    pub supported_encodings: Vec<String>,
    /// Optional map of key-value pairs.
    pub metadata: HashMap<String, String>,
    /// Optional string.
    pub session_id: String,
}

impl JsonMessage for ServerInfo {}

#[cfg(test)]
mod tests {
    use crate::server::ServerMessage;

    use super::*;

    fn message() -> ServerInfo {
        ServerInfo {
            name: "example server".into(),
            capabilities: vec![Capability::ClientPublish, Capability::Time],
            supported_encodings: vec!["json".into()],
            metadata: maplit::hashmap! {
                "key".into() => "value".into(),
            },
            session_id: "1675789422160".into(),
        }
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::ServerInfo(orig));
    }
}
