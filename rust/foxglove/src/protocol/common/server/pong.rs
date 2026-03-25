use std::borrow::Cow;

use bytes::BufMut;

use crate::protocol::{BinaryPayload, ParseError};

/// Pong message sent by the server in response to a
/// [`Ping`](crate::protocol::common::client::Ping).
///
/// The payload is echoed verbatim from the ping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pong<'a> {
    /// Opaque payload echoed from the ping.
    pub payload: Cow<'a, [u8]>,
}

impl<'a> Pong<'a> {
    /// Creates a new pong from the given payload.
    #[allow(dead_code)]
    pub fn new(payload: &'a [u8]) -> Self {
        Self {
            payload: Cow::Borrowed(payload),
        }
    }

    /// Returns an owned version with a `'static` lifetime.
    pub fn into_owned(self) -> Pong<'static> {
        Pong {
            payload: Cow::Owned(self.payload.into_owned()),
        }
    }
}

impl<'a> BinaryPayload<'a> for Pong<'a> {
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
        let orig = Pong::new(b"1234567890");
        let mut buf = Vec::new();
        orig.write_payload(&mut buf);
        let parsed = Pong::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_empty_payload() {
        let orig = Pong::new(b"");
        let mut buf = Vec::new();
        orig.write_payload(&mut buf);
        let parsed = Pong::parse_payload(&buf).unwrap();
        assert_eq!(parsed, orig);
    }
}
