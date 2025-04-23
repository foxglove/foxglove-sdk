use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::parameter::Parameter;
use crate::websocket::ws_protocol::JsonMessage;

/// Set parameters message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#set-parameters>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "setParameters", rename_all = "camelCase")]
pub struct SetParameters {
    /// Parameters.
    pub parameters: Vec<Parameter>,
    /// Request ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl SetParameters {
    /// Creates a new set parameters message.
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

impl JsonMessage for SetParameters {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;
    use crate::websocket::ws_protocol::parameter::Parameter;

    use super::*;

    fn message() -> SetParameters {
        SetParameters::new([
            Parameter::empty("empty"),
            Parameter::float64("f64", 1.23),
            Parameter::float64_array("f64[]", [1.23, 4.56]),
            Parameter::string("str", "hello"),
            Parameter::byte_array("byte[]", &[0x10, 0x20, 0x30]),
            Parameter::bool("bool", true),
        ])
    }

    fn message_with_id() -> SetParameters {
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

    fn test_roundtrip_inner(orig: SetParameters) {
        let buf = orig.to_string();
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::SetParameters(orig));
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
