use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Fetch asset message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "fetchAsset", rename_all = "camelCase")]
pub struct FetchAsset {
    /// Asset URI.
    pub uri: String,
    /// Request ID.
    pub request_id: u32,
}

impl JsonMessage for FetchAsset {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;

    use super::*;

    fn message() -> FetchAsset {
        FetchAsset {
            uri: "package://foxglove/example.urdf".to_string(),
            request_id: 42,
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
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::FetchAsset(orig));
    }
}
