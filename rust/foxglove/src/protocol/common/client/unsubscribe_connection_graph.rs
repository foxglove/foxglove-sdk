use serde::{Deserialize, Serialize};

use crate::protocol::JsonMessage;

/// Unsubscribe connection graph message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unsubscribe-connection-graph>
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename = "unsubscribeConnectionGraph",
    rename_all = "camelCase"
)]
pub struct UnsubscribeConnectionGraph {}

impl JsonMessage for UnsubscribeConnectionGraph {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(UnsubscribeConnectionGraph {});
    }

    #[test]
    fn test_roundtrip() {
        let orig = UnsubscribeConnectionGraph {};
        let buf = orig.to_string();
        let parsed: UnsubscribeConnectionGraph = serde_json::from_str(&buf).unwrap();
        assert_eq!(parsed, orig);
    }
}
