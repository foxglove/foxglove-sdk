//! Unsubscribe message types.

use serde::{Deserialize, Serialize};

/// Unsubscribe from channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "unsubscribe", rename_all = "camelCase")]
pub struct Unsubscribe {
    /// Channel IDs to unsubscribe from.
    pub channel_ids: Vec<u64>,
}

impl Unsubscribe {
    /// Creates a new unsubscribe message.
    pub fn new(channel_ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            channel_ids: channel_ids.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::v2::client::ClientMessage;

    use super::*;

    #[test]
    fn test_parse_json() {
        let json = r#"{"op": "unsubscribe", "channelIds": [1, 2, 3]}"#;
        let msg = ClientMessage::parse_json(json).unwrap();
        assert_eq!(msg, ClientMessage::Unsubscribe(Unsubscribe::new([1, 2, 3])));
    }
}
