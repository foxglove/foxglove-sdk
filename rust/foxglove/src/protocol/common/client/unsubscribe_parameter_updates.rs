use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Unsubscribe parameter updates message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unsubscribe-parameter-update>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename = "unsubscribeParameterUpdates",
    rename_all = "camelCase"
)]
pub struct UnsubscribeParameterUpdates {
    /// Parameter names.
    pub parameter_names: Vec<String>,
}

impl UnsubscribeParameterUpdates {
    /// Creates a new UnsubscribeParameterUpdates from an iterator of strings.
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            parameter_names: names.into_iter().map(Into::into).collect(),
        }
    }
}

impl JsonMessage for UnsubscribeParameterUpdates {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> UnsubscribeParameterUpdates {
        UnsubscribeParameterUpdates {
            parameter_names: vec!["param1".to_string(), "param2".to_string()],
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
        assert_eq!(msg, ClientMessage::UnsubscribeParameterUpdates(orig));
    }
}
