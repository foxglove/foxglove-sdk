use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Unsubscribe message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unsubscribe>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "unsubscribe", rename_all = "camelCase")]
pub struct Unsubscribe {
    /// Subscription IDs.
    pub subscription_ids: Vec<u32>,
}

impl Unsubscribe {
    /// Creates a new unsubscribe message.
    pub fn new(subscription_ids: impl IntoIterator<Item = u32>) -> Self {
        Self {
            subscription_ids: subscription_ids.into_iter().collect(),
        }
    }
}

impl JsonMessage for Unsubscribe {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> Unsubscribe {
        Unsubscribe::new([1, 2, 3])
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
        assert_eq!(msg, ClientMessage::Unsubscribe(orig));
    }
}
