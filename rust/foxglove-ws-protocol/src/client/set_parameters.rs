use serde::{Deserialize, Serialize};

use crate::parameter::Parameter;
use crate::JsonMessage;

/// Set parameters message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#set-parameters>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "setParameters", rename_all = "camelCase")]
pub struct SetParameters {
    /// Parameters.
    pub parameters: Vec<Parameter>,
    /// Request ID.
    pub id: Option<String>,
}

impl JsonMessage for SetParameters {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;
    use crate::parameter::Parameter;

    use super::*;

    fn message() -> SetParameters {
        SetParameters {
            parameters: vec![
                Parameter::empty("empty"),
                Parameter::float64("f64", 1.23),
                Parameter::float64_array("f64[]", vec![1.23, 4.56]),
                Parameter::byte_array("byte[]", [0x10, 0x20, 0x30]),
                Parameter::bool("bool", true),
            ],
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
        assert_eq!(msg, ClientMessage::SetParameters(orig));
    }
}
