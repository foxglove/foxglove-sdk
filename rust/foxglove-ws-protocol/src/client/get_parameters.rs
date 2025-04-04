use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Get parameters message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#get-parameters>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "getParameters", rename_all = "camelCase")]
pub struct GetParameters {
    /// Parameter names.
    pub parameter_names: Vec<String>,
    /// Request ID.
    pub id: Option<String>,
}

impl JsonMessage for GetParameters {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;

    use super::*;

    fn message() -> GetParameters {
        GetParameters {
            parameter_names: vec!["param1".to_string(), "param2".to_string()],
            id: Some("request-123".to_string()),
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
        assert_eq!(msg, ClientMessage::GetParameters(orig));
    }
}
