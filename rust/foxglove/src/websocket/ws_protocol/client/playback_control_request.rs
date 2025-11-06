use bytes::{Buf, BufMut};

use crate::websocket::ws_protocol::{BinaryMessage, ParseError};

use super::BinaryOpcode;

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaybackState {
    Playing = 0,
    Paused = 1,
    Buffering = 2,
    Ended = 3,
}

impl TryFrom<u8> for PlaybackState {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Playing),
            1 => Ok(Self::Paused),
            2 => Ok(Self::Buffering),
            3 => Ok(Self::Ended),
            _ => Err(value),
        }
    }
}

#[doc(hidden)]
/// Player state message.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackControlRequest {
    /// Playback state
    pub playback_state: PlaybackState,
    /// Playback speed
    pub playback_speed: f32,
    /// Seek playback time in nanoseconds (only set if a seek has been performed)
    pub seek_time: Option<u64>,
}

impl<'a> BinaryMessage<'a> for PlaybackControlRequest {
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        // Message size: playback_state (1 byte) + playback_speed (4 bytes) + had_seek (1 byte) + seek_time (8 bytes)
        if data.len() < 1 + 4 + 1 + 8 {
            return Err(ParseError::BufferTooShort);
        }

        let state_byte = data.get_u8();
        let playback_state = PlaybackState::try_from(state_byte)
            .map_err(|_| ParseError::InvalidPlaybackState(state_byte))?;

        let playback_speed = data.get_f32_le();
        let had_seek = data.get_u8() != 0;
        let seek_time = if had_seek {
            Some(data.get_u64_le())
        } else {
            None
        };

        Ok(Self {
            playback_state,
            playback_speed,
            seek_time,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        // Message size: opcode (1) + playback_state (1) + playback_speed (4) + had_seek (1) + seek_time (8)
        let mut buf = Vec::with_capacity(1 + 4 + 1 + 8);

        buf.put_u8(BinaryOpcode::PlaybackControlRequest as u8);
        buf.put_u8(self.playback_state as u8);
        buf.put_f32_le(self.playback_speed);
        buf.put_u8(if self.seek_time.is_some() { 1 } else { 0 });
        // To keep the message size constant, write the seek time even if it's None.
        buf.put_u64_le(self.seek_time.unwrap_or(0));
        buf
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    #[test]
    fn test_encode_playing() {
        let message = PlaybackControlRequest {
            playback_state: PlaybackState::Playing,
            playback_speed: 1.0,
            seek_time: None,
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_encode_paused() {
        let message = PlaybackControlRequest {
            playback_state: PlaybackState::Paused,
            playback_speed: 1.0,
            seek_time: None,
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_encode_buffering() {
        let message = PlaybackControlRequest {
            playback_state: PlaybackState::Buffering,
            playback_speed: 1.0,
            seek_time: None,
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_encode_ended() {
        let message = PlaybackControlRequest {
            playback_state: PlaybackState::Ended,
            playback_speed: 1.0,
            seek_time: None,
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_roundtrip_playing_with_seek_time() {
        let orig = PlaybackControlRequest {
            playback_state: PlaybackState::Playing,
            playback_speed: 1.0,
            seek_time: Some(100_500_000_000),
        };
        let buf = orig.to_bytes();
        let msg = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ClientMessage::PlaybackControlRequest(orig));
    }

    #[test]
    fn test_roundtrip_playing_without_seek_time() {
        let orig = PlaybackControlRequest {
            playback_state: PlaybackState::Playing,
            playback_speed: 1.0,
            seek_time: None,
        };
        let buf = orig.to_bytes();
        let msg = ClientMessage::parse_binary(&buf).unwrap();
        assert_eq!(msg, ClientMessage::PlaybackControlRequest(orig));
    }

    #[test]
    fn test_parse_binary_with_seek_time() {
        // Manually construct binary data: opcode + state + speed + had_seek + seek_time
        let mut data = Vec::new();
        data.put_u8(BinaryOpcode::PlaybackControlRequest as u8); // opcode
        data.put_u8(PlaybackState::Playing as u8); // state
        data.put_f32_le(1.5); // speed
        data.put_u8(1); // had_seek = true
        data.put_u64_le(100_500_000_000); // seek_time

        let msg = ClientMessage::parse_binary(&data).unwrap();
        match msg {
            ClientMessage::PlaybackControlRequest(state) => {
                assert_eq!(state.playback_state, PlaybackState::Playing);
                assert_eq!(state.playback_speed, 1.5);
                assert_eq!(state.seek_time, Some(100_500_000_000));
            }
            _ => panic!("Expected PlaybackControlRequest message"),
        }
    }

    #[test]
    fn test_parse_binary_without_seek_time() {
        // Manually construct binary data with had_seek = false (seek_time bytes still present but zeroed)
        let mut data = Vec::new();
        data.put_u8(BinaryOpcode::PlaybackControlRequest as u8); // opcode
        data.put_u8(PlaybackState::Buffering as u8); // state
        data.put_f32_le(2.0); // speed
        data.put_u8(0); // had_seek = false
        data.put_u64_le(0); // seek_time (zeroed out, ignored since had_seek = false)

        let msg = ClientMessage::parse_binary(&data).unwrap();
        match msg {
            ClientMessage::PlaybackControlRequest(state) => {
                assert_eq!(state.playback_state, PlaybackState::Buffering);
                assert_eq!(state.playback_speed, 2.0);
                assert_eq!(state.seek_time, None);
            }
            _ => panic!("Expected PlaybackControlRequest message"),
        }
    }
}
