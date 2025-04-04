use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Remove status message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#remove-status>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "removeStatus", rename_all = "camelCase")]
pub struct RemoveStatus {
    /// IDs of the status messages to be removed.
    pub status_ids: Vec<String>,
}

impl JsonMessage for RemoveStatus {}

#[cfg(test)]
mod tests {
    use crate::server::ServerMessage;

    use super::*;

    fn message() -> RemoveStatus {
        RemoveStatus {
            status_ids: vec!["status-1".into(), "status-2".into(), "status-3".into()],
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
        assert_eq!(msg, ServerMessage::RemoveStatus(orig));
    }
}
