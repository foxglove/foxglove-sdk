//! Subscribe message types.

use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Subscribe message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscribe", rename_all = "camelCase")]
pub struct Subscribe {
    /// Subscriptions.
    pub subscriptions: Vec<Subscription>,
}

/// A subscription for a [`Subscribe`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Subscription ID.
    pub id: u32,
    /// Channel ID.
    pub channel_id: u64,
}

impl JsonMessage for Subscribe {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;

    use super::*;

    fn message() -> Subscribe {
        Subscribe {
            subscriptions: vec![
                Subscription {
                    id: 1,
                    channel_id: 10,
                },
                Subscription {
                    id: 2,
                    channel_id: 20,
                },
            ],
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
        assert_eq!(msg, ClientMessage::Subscribe(orig));
    }
}
