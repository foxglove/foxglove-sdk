use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Fetch asset message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "fetchAsset", rename_all = "camelCase")]
pub struct FetchAsset {
    /// Request ID.
    pub request_id: u32,
    /// Asset URI.
    pub uri: String,
}

impl FetchAsset {
    /// Creates a new fetch asset message.
    pub fn new(request_id: u32, uri: impl Into<String>) -> Self {
        Self {
            request_id,
            uri: uri.into(),
        }
    }
}

impl JsonMessage for FetchAsset {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> FetchAsset {
        FetchAsset::new(42, "package://foxglove/example.urdf")
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::FetchAsset(orig));
    }
}
