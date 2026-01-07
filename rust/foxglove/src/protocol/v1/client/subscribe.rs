//! Subscribe message types.

use serde::{Deserialize, Serialize};

use crate::protocol::JsonMessage;

/// Subscribe message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscribe", rename_all = "camelCase")]
pub struct SubscribeV1 {
    /// Subscriptions.
    pub subscriptions: Vec<Subscription>,
}

impl SubscribeV1 {
    /// Creates a new subscribe message.
    pub fn new(subscriptions: impl IntoIterator<Item = Subscription>) -> Self {
        Self {
            subscriptions: subscriptions.into_iter().collect(),
        }
    }
}

impl JsonMessage for SubscribeV1 {}

/// A subscription for a [`SubscribeV1`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Subscription ID.
    pub id: u32,
    /// Channel ID.
    pub channel_id: u64,
}

impl Subscription {
    /// Creates a new subscription with the specified channel ID and subscription ID.
    pub fn new(id: u32, channel_id: u64) -> Self {
        Self { id, channel_id }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::v1::client::ClientMessageV1;

    use super::*;

    fn message() -> SubscribeV1 {
        SubscribeV1::new([Subscription::new(1, 10), Subscription::new(2, 20)])
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ClientMessageV1::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessageV1::Subscribe(orig));
    }
}
