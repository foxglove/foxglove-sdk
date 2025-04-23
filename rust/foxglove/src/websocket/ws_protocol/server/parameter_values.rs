use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::parameter::Parameter;
use crate::websocket::ws_protocol::JsonMessage;

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
    pub fn new(parameters: impl IntoIterator<Item = Parameter>) -> Self {
        Self {
            parameters: parameters.into_iter().collect(),
            id: None,
        }
    }

    /// Sets the request ID.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl JsonMessage for ParameterValues {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> ParameterValues {
        ParameterValues::new([
            Parameter::empty("empty"),
            Parameter::float64("f64", 1.23),
            Parameter::float64_array("f64[]", [1.23, 4.56]),
            Parameter::string("str", "hello"),
            Parameter::byte_array("byte[]", &[0x10, 0x20, 0x30]),
            Parameter::bool("bool", true),
        ])
    }

    fn message_with_id() -> ParameterValues {
        message().with_id("my-id")
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_encode_with_id() {
        insta::assert_json_snapshot!(message_with_id());
    }

    fn test_roundtrip_inner(orig: ParameterValues) {
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::ParameterValues(orig));
    }

    #[test]
    fn test_roundtrip() {
        test_roundtrip_inner(message())
    }

    #[test]
    fn test_roundtrip_with_id() {
        test_roundtrip_inner(message_with_id())
    }
}
