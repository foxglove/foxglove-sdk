use bytes::{Buf, BufMut};

use crate::protocol::{BinaryMessage, BinaryPayload, ParseError};

/// A playback command from the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaybackCommand {
    Play = 0,
    Pause = 1,
}

impl TryFrom<u8> for PlaybackCommand {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Play),
            1 => Ok(Self::Pause),
            _ => Err(value),
        }
    }
}

/// A request to control playback from the client
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackControlRequest {
    /// Playback command
    pub playback_command: PlaybackCommand,
    /// Playback speed
    pub playback_speed: f32,
    /// Seek playback time in nanoseconds (only set if a seek has been performed)
    pub seek_time: Option<u64>,
    /// Unique string identifier, used to indicate that a PlaybackState is in response to a particular request from the client.
    /// Should not be an empty string.
    pub request_id: String,
}

impl<'a> BinaryPayload<'a> for PlaybackControlRequest {
    // Message layout:
    // + playback_command (1 byte)
    // + playback_speed (4 bytes)
    // + had_seek (1 byte)
    // + seek_time (8 bytes)
    // + request_id_len (4 bytes)
    // + request_id
    fn parse_payload(mut data: &'a [u8]) -> Result<Self, ParseError> {
        if data.len() < 1 + 4 + 1 + 8 + 4 {
            return Err(ParseError::BufferTooShort);
        }

        let state_byte = data.get_u8();
        let playback_command = PlaybackCommand::try_from(state_byte)
            .map_err(|_| ParseError::InvalidPlaybackCommand(state_byte))?;

        let playback_speed = data.get_f32_le();
        let had_seek = data.get_u8() != 0;
        let seek_time = if had_seek {
            Some(data.get_u64_le())
        } else {
            // Advance the buffer position, discarding the seek time
            data.advance(8);
            None
        };

        let request_id_len = data.get_u32_le() as usize;
        if data.len() < request_id_len {
            return Err(ParseError::BufferTooShort);
        }
        let request_id_bytes = &data[..request_id_len];
        let request_id = std::str::from_utf8(request_id_bytes)?.to_string();

        Ok(Self {
            playback_command,
            playback_speed,
            seek_time,
            request_id,
        })
    }

    fn payload_size(&self) -> usize {
        1 + 4 + 1 + 8 + 4 + self.request_id.len()
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.playback_command as u8);
        buf.put_f32_le(self.playback_speed);
        buf.put_u8(if self.seek_time.is_some() { 1 } else { 0 });
        buf.put_u64_le(self.seek_time.unwrap_or(0));
        buf.put_u32_le(self.request_id.len() as u32);
        buf.put_slice(self.request_id.as_bytes());
    }
}

impl BinaryMessage<'_> for PlaybackControlRequest {
    const OPCODE: u8 = 3;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_play_with_seek_time() {
        let orig = PlaybackControlRequest {
            playback_command: PlaybackCommand::Play,
            playback_speed: 1.0,
            seek_time: Some(100_500_000_000),
            request_id: "some-id".to_string(),
        };
        let mut buf = Vec::new();
        BinaryPayload::write_payload(&orig, &mut buf);
        let parsed = PlaybackControlRequest::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_roundtrip_play_without_seek_time() {
        let orig = PlaybackControlRequest {
            playback_command: PlaybackCommand::Play,
            playback_speed: 1.0,
            seek_time: None,
            request_id: "some-id".to_string(),
        };
        let mut buf = Vec::new();
        BinaryPayload::write_payload(&orig, &mut buf);
        let parsed = PlaybackControlRequest::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_parse_payload_with_seek_time() {
        let request_id = "some-id".to_string();
        // Manually construct binary payload: command + speed + had_seek + seek_time + request_id_len + request_id
        let mut data = Vec::new();
        data.put_u8(PlaybackCommand::Play as u8); // command
        data.put_f32_le(1.5); // speed
        data.put_u8(1); // had_seek = true
        data.put_u64_le(100_500_000_000); // seek_time
        data.put_u32_le(request_id.len() as u32);
        data.put_slice(request_id.as_bytes());

        let parsed = PlaybackControlRequest::parse_payload(&data).unwrap();
        assert_eq!(parsed.playback_command, PlaybackCommand::Play);
        assert_eq!(parsed.playback_speed, 1.5);
        assert_eq!(parsed.seek_time, Some(100_500_000_000));
        assert_eq!(parsed.request_id, "some-id".to_string());
    }

    #[test]
    fn test_parse_payload_without_seek_time() {
        // Manually construct binary payload with had_seek = false (seek_time bytes still present but zeroed)
        let request_id = "some-id".to_string();

        let mut data = Vec::new();
        data.put_u8(PlaybackCommand::Play as u8); // command
        data.put_f32_le(2.0); // speed
        data.put_u8(0); // had_seek = false
        data.put_u64_le(0); // seek_time (zeroed out, ignored since had_seek = false)
        data.put_u32_le(request_id.len() as u32);
        data.put_slice(request_id.as_bytes());

        let parsed = PlaybackControlRequest::parse_payload(&data).unwrap();
        assert_eq!(parsed.playback_command, PlaybackCommand::Play);
        assert_eq!(parsed.playback_speed, 2.0);
        assert_eq!(parsed.seek_time, None);
        assert_eq!(parsed.request_id, "some-id".to_string());
    }
}
