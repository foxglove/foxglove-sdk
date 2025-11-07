use crate::websocket::ws_protocol::{BinaryMessage, ParseError};
use bytes::{Buf, BufMut};

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaybackStatus {
    Playing = 0,
    Paused = 1,
    Buffering = 2,
    Ended = 3,
}

impl TryFrom<u8> for PlaybackStatus {
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

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackState {
    pub status: PlaybackStatus,
    pub current_time: u64,
    pub playback_speed: f32,
    pub request_id: Option<String>,
}

impl<'a> BinaryMessage<'a> for PlaybackState {
    // Message binary layout
    // 1: status
    // 8: current time
    // 4: playback speed
    // 4: request_id length (0 if no request id sent)
    // rest: request_id
    fn parse_binary(mut data: &'a [u8]) -> Result<Self, ParseError> {
        const HEADER_LEN: usize = 1 + 8 + 4 + 4;
        if data.len() < HEADER_LEN {
            return Err(ParseError::BufferTooShort);
        }

        let status_byte = data.get_u8();
        let status = PlaybackStatus::try_from(status_byte)
            .map_err(|_| ParseError::InvalidPlaybackStatus(status_byte))?;

        let current_time = data.get_u64_le();
        let playback_speed = data.get_f32_le();
        let request_id_len = data.get_u32_le() as usize;
        let request_id = if request_id_len == 0 {
            None
        } else {
            if data.len() < request_id_len {
                return Err(ParseError::BufferTooShort);
            }
            let request_id_bytes = &data[..request_id_len];
            let request_id_str = std::str::from_utf8(request_id_bytes)?.to_string();
            Some(request_id_str)
        };

        Ok(Self {
            status,
            current_time,
            playback_speed,
            request_id,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let request_id_len: u32 = match &self.request_id {
            Some(request_id) => request_id.len() as u32,
            None => 0,
        };

        let mut buf = Vec::with_capacity(1 + 8 + 4 + 4 + (request_id_len as usize));
        buf.put_u8(self.status as u8);
        buf.put_u64_le(self.current_time);
        buf.put_f32_le(self.playback_speed);
        buf.put_u32_le(request_id_len);
        if let Some(request_id) = &self.request_id {
            buf.put_slice(request_id.as_bytes());
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    // TODO: Add opcode here

    #[test]
    fn test_encode_playing() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            request_id: None,
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_encode_playing_with_request_id() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            request_id: Some("i-am-a-request".to_string()),
        };
        insta::assert_snapshot!(format!("{:#04x?}", message.to_bytes()));
    }

    #[test]
    fn test_roundtrip_with_request_id() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            request_id: Some("i-am-a-request".to_string()),
        };

        let parse_result = PlaybackState::parse_binary(&message.to_bytes());
        assert_matches!(parse_result, Ok(parse_result) => {
            assert_eq!(parse_result, message);
        });
    }

    #[test]
    fn test_roundtrip_without_request_id() {
        let message = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            request_id: None,
        };

        let parse_result = PlaybackState::parse_binary(&message.to_bytes());
        assert_matches!(parse_result, Ok(parse_result) => {
            assert_eq!(parse_result, message);
        });
    }

    #[test]
    fn test_bad_request_id_length() {
        let mut message_bytes: Vec<u8> = [].to_vec();
        message_bytes.put_u8(0x0);
        message_bytes.put_u64_le(500);
        message_bytes.put_f32_le(1.0);
        message_bytes.put_u32_le(10_000); // size of the request_id, way more bytes than we have
        message_bytes.put_slice(b"i-am-but-a-smol-id");

        let parse_result = PlaybackState::parse_binary(&message_bytes);
        assert_matches!(parse_result, Err(ParseError::BufferTooShort));
    }
}
