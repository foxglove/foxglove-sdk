use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Get parameters message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#get-parameters>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "getParameters", rename_all = "camelCase")]
pub struct GetParameters {
    /// Parameter names.
    pub parameter_names: Vec<String>,
    /// Request ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl GetParameters {
    /// Creates a new get parameters message.
    pub fn new(parameter_names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            parameter_names: parameter_names.into_iter().map(|s| s.into()).collect(),
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

impl JsonMessage for GetParameters {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> GetParameters {
        GetParameters::new(["param1", "param2"])
    }

    fn message_with_id() -> GetParameters {
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

    fn test_roundtrip_inner(orig: GetParameters) {
        let buf = orig.to_string();
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::GetParameters(orig));
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
