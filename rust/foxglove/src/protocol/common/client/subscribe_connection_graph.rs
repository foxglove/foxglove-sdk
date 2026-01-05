use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

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
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(SubscribeConnectionGraph {});
    }

    #[test]
    fn test_roundtrip() {
        let orig = SubscribeConnectionGraph {};
        let buf = orig.to_string();
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::SubscribeConnectionGraph);
    }
}
