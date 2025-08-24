//! Subscribe message types.

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Subscribe message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscribe", rename_all = "camelCase")]
pub struct Subscribe {
    /// Subscriptions.
    pub subscriptions: Vec<Subscription>,
}

impl Subscribe {
    /// Creates a new subscribe message.
    pub fn new(subscriptions: impl IntoIterator<Item = Subscription>) -> Self {
        Self {
            subscriptions: subscriptions.into_iter().collect(),
        }
    }
}

impl JsonMessage for Subscribe {}

/// A subscription for a [`Subscribe`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Subscription ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Channel ID.
    pub channel_id: u64,
}

impl Subscription {
    /// Creates a new subscription with the specified channel ID and default subscription ID (0).
    pub fn new(channel_id: u64) -> Self {
        Self {
            id: None,
            channel_id,
        }
    }

    /// Creates a new subscription with the specified channel ID and subscription ID.
    pub fn with_id(id: u32, channel_id: u64) -> Self {
        Self {
            id: Some(id),
            channel_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> Subscribe {
        Subscribe::new([Subscription::with_id(1, 10), Subscription::with_id(2, 20)])
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
        assert_eq!(msg, ClientMessage::Subscribe(orig));
    }

    #[test]
    fn test_subscription_default_id_serialization() {
        // Test that a subscription with default id serializes and deserializes correctly
        let sub = Subscription::new(100);
        let serialized = serde_json::to_string(&sub).unwrap();
        let deserialized: Subscription = serde_json::from_str(&serialized).unwrap();
        assert_eq!(sub, deserialized);
        assert!(deserialized.id.is_none());
        assert_eq!(deserialized.channel_id, 100);

        // Test deserializing JSON without id field (should default to 0)
        let json_without_id = r#"{"channelId": 200}"#;
        let deserialized: Subscription = serde_json::from_str(json_without_id).unwrap();
        assert!(deserialized.id.is_none());
        assert_eq!(deserialized.channel_id, 200);
    }
}
