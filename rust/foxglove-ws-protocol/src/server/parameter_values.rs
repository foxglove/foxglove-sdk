use serde::{Deserialize, Serialize};

use crate::parameter::Parameter;
use crate::JsonMessage;

/// Parameter values message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#parameter-values>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "parameterValues", rename_all = "camelCase")]
pub struct ParameterValues {
    /// Parameter values.
    pub parameters: Vec<Parameter>,
    /// ID from a get/set parameters request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl ParameterValues {
    /// Creates a new parameter values message.
    pub fn new(
        parameters: impl IntoIterator<Item = Parameter>,
        id: impl Into<Option<String>>,
    ) -> Self {
        Self {
            parameters: parameters.into_iter().collect(),
            id: id.into(),
        }
    }
}

impl JsonMessage for ParameterValues {}

#[cfg(test)]
mod tests {
    use crate::server::ServerMessage;

    use super::*;

    fn message() -> ParameterValues {
        ParameterValues {
            id: None,
            parameters: vec![
                Parameter::empty("empty"),
                Parameter::float64("f64", 1.23),
                Parameter::float64_array("f64[]", vec![1.23, 4.56]),
                Parameter::byte_array("byte[]", [0x10, 0x20, 0x30]),
                Parameter::bool("bool", true),
            ],
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
        assert_eq!(msg, ServerMessage::ParameterValues(orig));
    }
}
