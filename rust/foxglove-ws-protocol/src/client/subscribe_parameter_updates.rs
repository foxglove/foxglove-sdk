use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Subscribe parameter updates message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe-parameter-update>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename = "subscribeParameterUpdates",
    rename_all = "camelCase"
)]
pub struct SubscribeParameterUpdates {
    /// Parameter names.
    pub parameter_names: Vec<String>,
}

impl SubscribeParameterUpdates {
    /// Creates a new SubscribeParameterUpdates from an iterator of strings.
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            parameter_names: names.into_iter().map(Into::into).collect(),
        }
    }
}

impl JsonMessage for SubscribeParameterUpdates {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;

    use super::*;

    fn message() -> SubscribeParameterUpdates {
        SubscribeParameterUpdates {
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
        assert_eq!(msg, ClientMessage::SubscribeParameterUpdates(orig));
    }
}
