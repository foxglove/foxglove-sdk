use std::borrow::Cow;

use bytes::BufMut;

use crate::protocol::{BinaryPayload, ParseError};

/// Ping message sent by the client to measure round-trip time.
///
/// The server echoes the payload back in a [`Pong`](crate::protocol::common::server::Pong).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ping<'a> {
    /// Opaque payload echoed back by the server.
    pub payload: Cow<'a, [u8]>,
}

impl<'a> Ping<'a> {
    pub fn into_owned(self) -> Ping<'static> {
        Ping {
            payload: Cow::Owned(self.payload.into_owned()),
        }
    }
}

impl<'a> BinaryPayload<'a> for Ping<'a> {
    fn parse_payload(data: &'a [u8]) -> Result<Self, ParseError> {
        Ok(Self {
            payload: Cow::Borrowed(data),
        })
    }

    fn payload_size(&self) -> usize {
        self.payload.len()
    }

    fn write_payload(&self, buf: &mut impl BufMut) {
        buf.put_slice(&self.payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let orig = Ping {
            payload: Cow::Borrowed(b"1234567890"),
        };
        let mut buf = Vec::new();
        orig.write_payload(&mut buf);
        let parsed = Ping::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_empty_payload() {
        let orig = Ping {
            payload: Cow::Borrowed(b""),
        };
        let mut buf = Vec::new();
        orig.write_payload(&mut buf);
        let parsed = Ping::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }
}
