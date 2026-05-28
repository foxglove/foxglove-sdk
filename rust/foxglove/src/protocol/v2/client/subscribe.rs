//! Subscribe message types.

use serde::{Deserialize, Serialize};

/// A channel subscription entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeChannel {
    /// Channel ID to subscribe to.
    pub id: u64,
    /// Whether to request a video track for this channel.
    #[serde(default)]
    pub request_video_track: bool,
}

/// Subscribe to channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "subscribe", rename_all = "camelCase")]
pub struct Subscribe {
    /// Channels to subscribe to.
    pub channels: Vec<SubscribeChannel>,
}

impl Subscribe {
    /// Creates a new subscribe message from subscribe channel entries.
    pub fn new(channels: impl IntoIterator<Item = SubscribeChannel>) -> Self {
        Self {
            channels: channels.into_iter().collect(),
        }
    }

    /// Returns the channel IDs in this subscribe message.
    pub fn channel_ids(&self) -> Vec<u64> {
        self.channels.iter().map(|ch| ch.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::v2::client::ClientMessage;

    use super::*;

    #[test]
    fn test_parse_json() {
        let json = r#"{"op": "subscribe", "channels": [{"id": 10}, {"id": 20, "requestVideoTrack": true}]}"#;
        let msg = ClientMessage::parse_json(json).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Subscribe(Subscribe::new([
                SubscribeChannel {
                    id: 10,
                    request_video_track: false,
                },
                SubscribeChannel {
                    id: 20,
                    request_video_track: true,
                },
            ]))
        );
    }

    #[test]
    fn test_parse_json_simple() {
        let json = r#"{"op": "subscribe", "channels": [{"id": 10}, {"id": 20}, {"id": 30}]}"#;
        let msg = ClientMessage::parse_json(json).unwrap();
        assert_eq!(
            msg,
            ClientMessage::Subscribe(Subscribe::new([
                SubscribeChannel {
                    id: 10,
                    request_video_track: false,
                },
                SubscribeChannel {
                    id: 20,
                    request_video_track: false,
                },
                SubscribeChannel {
                    id: 30,
                    request_video_track: false,
                },
            ]))
        );
    }
}
