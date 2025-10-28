use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Playing {
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerTime {
    pub sec: u64,
    pub nsec: u64,
}

impl PlayerTime {
    pub fn new(sec: u64, nsec: u64) -> Self {
        Self { sec, nsec }
    }
}

/// Player state message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "playerState", rename_all = "camelCase")]
pub struct PlayerState {
    /// Playing state
    pub playing: Playing,
    /// Playback speed
    pub playback_speed: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Seek playback time
    pub seek_playback_time: Option<PlayerTime>,
    /// Previous playback time (will be defined if a seek has occurred)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek_previous_playback_time: Option<PlayerTime>,
}

impl JsonMessage for PlayerState {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    #[test]
    fn test_encode_paused() {
        let message_paused = PlayerState {
            playing: Playing::Paused,
            playback_speed: 0.5,
            seek_previous_playback_time: None,
            seek_playback_time: Some(PlayerTime {
                sec: 123,
                nsec: 456,
            }),
        };
        insta::assert_json_snapshot!(message_paused);
    }

    #[test]
    fn test_encode_with_previous_time() {
        let message_with_previous_time = PlayerState {
            playing: Playing::Playing,
            playback_speed: 1.0,
            seek_previous_playback_time: Some(PlayerTime {
                sec: 1000000,
                nsec: 500000000,
            }),
            seek_playback_time: Some(PlayerTime {
                sec: 1000005,
                nsec: 0,
            }),
        };
        insta::assert_json_snapshot!(message_with_previous_time);
    }

    #[test]
    fn test_roundtrip() {
        let message = PlayerState {
            playing: Playing::Playing,
            playback_speed: 1.0,
            seek_previous_playback_time: None,
            seek_playback_time: Some(PlayerTime {
                sec: 1234567,
                nsec: 890123456,
            }),
        };

        let buf = message.to_string();
        let parsed = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(parsed, ClientMessage::PlayerState(message));
    }

    #[test]
    fn test_parse_json() {
        let json = r#"{"op":"playerState","playing":"playing","playbackSpeed":1.5,"seekPlaybackTime":{"sec":100,"nsec":500000000}}"#;
        let msg = ClientMessage::parse_json(json).unwrap();
        match msg {
            ClientMessage::PlayerState(state) => {
                assert_eq!(state.playing, Playing::Playing);
                assert_eq!(state.playback_speed, 1.5);
                assert_eq!(
                    state.seek_playback_time,
                    Some(PlayerTime {
                        sec: 100,
                        nsec: 500000000,
                    })
                );
                assert_eq!(state.seek_previous_playback_time, None);
            }
            _ => panic!("Expected PlayerState message"),
        }
    }

    #[test]
    fn test_parse_json_with_previous_time() {
        let json = r#"{"op":"playerState","playing":"paused","playbackSpeed":2.0,"seekPreviousPlaybackTime":{"sec":50,"nsec":250000000},"seekPlaybackTime":{"sec":100,"nsec":500000000}}"#;
        let msg = ClientMessage::parse_json(json).unwrap();
        match msg {
            ClientMessage::PlayerState(state) => {
                assert_eq!(state.playing, Playing::Paused);
                assert_eq!(state.playback_speed, 2.0);
                assert_eq!(
                    state.seek_playback_time,
                    Some(PlayerTime {
                        sec: 100,
                        nsec: 500000000,
                    })
                );
                assert_eq!(
                    state.seek_previous_playback_time,
                    Some(PlayerTime {
                        sec: 50,
                        nsec: 250000000,
                    })
                );
            }
            _ => panic!("Expected PlayerState message"),
        }
    }
}
