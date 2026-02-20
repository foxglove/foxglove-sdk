use serde::{Deserialize, Serialize};

use crate::protocol::JsonMessage;

/// Subscribe connection graph message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe-connection-graph>
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "op",
    rename = "subscribeConnectionGraph",
    rename_all = "camelCase"
)]
pub struct SubscribeConnectionGraph {}

impl JsonMessage for SubscribeConnectionGraph {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(SubscribeConnectionGraph {});
    }

    #[test]
    fn test_roundtrip() {
        let orig = SubscribeConnectionGraph {};
        let buf = orig.to_string();
        let parsed: SubscribeConnectionGraph = serde_json::from_str(&buf).unwrap();
        assert_eq!(parsed, orig);
    }
}
