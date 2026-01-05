use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Remove status message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#remove-status>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "removeStatus", rename_all = "camelCase")]
pub struct RemoveStatus {
    /// IDs of the status messages to be removed.
    pub status_ids: Vec<String>,
}

impl RemoveStatus {
    /// Creates a new remove status message.
    pub fn new(status_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            status_ids: status_ids.into_iter().map(|s| s.into()).collect(),
        }
    }
}

impl JsonMessage for RemoveStatus {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> RemoveStatus {
        RemoveStatus::new(["status-1", "status-2", "status-3"])
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
        assert_eq!(msg, ServerMessage::RemoveStatus(orig));
    }
}
