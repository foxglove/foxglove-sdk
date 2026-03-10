use crate::protocol::{BinaryMessage, BinaryPayload, ParseError};
use bytes::{Buf, BufMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
/// The status playback of data that the server is providing
pub enum PlaybackStatus {
    /// Playing at the requested playback speed
    Playing = 0,
    /// Playback paused
    Paused = 1,
    /// Server is not yet playing back data because it is performing a prerequisite required operation
    Buffering = 2,
    /// The end of the available data has been reached
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
/// The state of the server playing back data.
///
/// Should be sent in response to a PlaybackControlRequest, or any time the
/// state of playback has changed; for example, reaching the end of data, or an external mechanism
/// causes playback to pause.
///
/// Only relevant if the `PlaybackControl` capability is enabled.
pub struct PlaybackState {
    /// The status of server data playback
    pub status: PlaybackStatus,
    /// The current time of playback, in absolute nanoseconds
    pub current_time: u64,
    /// The speed of playback, as a factor of realtime
    pub playback_speed: f32,
    /// Whether a seek forward or backward in time triggered this message to be emitted
    pub did_seek: bool,
    /// If this message is being emitted in response to a PlaybackControlRequest message, the
    /// request_id from that message. Set this to None if the state of playback has been changed
    /// by any other condition.
    pub request_id: Option<String>,
}

impl<'a> BinaryPayload<'a> for PlaybackState {
    // Message layout:
    // + status (1 byte)
    // + current_time (8 bytes)
    // + playback_speed (4 bytes)
    // + did_seek (1 byte)
    // + request_id_len (4 bytes)
    // + request_id
    fn parse_payload(mut data: &'a [u8]) -> Result<Self, ParseError> {
        const HEADER_LEN: usize = 1 + 8 + 4 + 1 + 4;
        if data.len() < HEADER_LEN {
            return Err(ParseError::BufferTooShort);
        }

        let status_byte = data.get_u8();
        let status = PlaybackStatus::try_from(status_byte)
            .map_err(|_| ParseError::InvalidPlaybackStatus(status_byte))?;

        let current_time = data.get_u64_le();
        let playback_speed = data.get_f32_le();
        let did_seek = data.get_u8() != 0;
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
            did_seek,
            request_id,
        })
    }

    fn payload_size(&self) -> usize {
        let request_id_len = self.request_id.as_ref().map_or(0, |id| id.len());
        1 + 8 + 4 + 1 + 4 + request_id_len
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        let request_id_len: u32 = match &self.request_id {
            Some(request_id) => request_id.len() as u32,
            None => 0,
        };

        buf.put_u8(self.status as u8);
        buf.put_u64_le(self.current_time);
        buf.put_f32_le(self.playback_speed);
        buf.put_u8(self.did_seek as u8);
        buf.put_u32_le(request_id_len);
        if let Some(request_id) = &self.request_id {
            buf.put_slice(request_id.as_bytes());
        }
    }
}

impl BinaryMessage<'_> for PlaybackState {
    const OPCODE: u8 = 5;
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_roundtrip_with_request_id() {
        let orig = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            did_seek: false,
            request_id: Some("i-am-a-request".to_string()),
        };

        let mut buf = Vec::new();
        BinaryPayload::write_payload(&orig, &mut buf);
        let parsed = PlaybackState::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_roundtrip_without_request_id() {
        let orig = PlaybackState {
            status: PlaybackStatus::Playing,
            playback_speed: 1.0,
            current_time: 12345,
            did_seek: false,
            request_id: None,
        };

        let mut buf = Vec::new();
        BinaryPayload::write_payload(&orig, &mut buf);
        let parsed = PlaybackState::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_bad_request_id_length() {
        let mut message_bytes: Vec<u8> = [].to_vec();
        message_bytes.put_u8(0x0);
        message_bytes.put_u64_le(500);
        message_bytes.put_f32_le(1.0);
        message_bytes.put_u32_le(10_000); // size of the request_id, way more bytes than we have
        message_bytes.put_slice(b"i-am-but-a-smol-id");

        let parse_result = PlaybackState::parse_payload(&message_bytes);
        assert_matches!(parse_result, Err(ParseError::BufferTooShort));
    }
}
