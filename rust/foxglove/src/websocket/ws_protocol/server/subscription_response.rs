//! Subscription response message types.

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// A subscription for a [`Subscribe`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Subscription ID.
    pub id: u32,
    /// Channel ID.
    pub channel_id: u64,
}

/// Subscription response message.
///
/// This message contains the subscriptions that were processed by the server,
/// in the same order as they were received in the Subscribe message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscriptionResponse", rename_all = "camelCase")]
pub struct SubscriptionResponse {
    /// Subscriptions in the same order as received in the Subscribe message.
    pub subscriptions: Vec<Subscription>,
}

impl SubscriptionResponse {
    /// Creates a new subscription response message.
    pub fn new(subscriptions: impl IntoIterator<Item = Subscription>) -> Self {
        Self {
            subscriptions: subscriptions.into_iter().collect(),
        }
    }
}

impl JsonMessage for SubscriptionResponse {}

#[cfg(test)]
mod tests {
    use crate::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> SubscriptionResponse {
        SubscriptionResponse::new([
            Subscription {
                id: 1,
                channel_id: 10,
            },
            Subscription {
                id: 2,
                channel_id: 20,
            },
        ])
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::SubscriptionResponse(orig));
    }
}
