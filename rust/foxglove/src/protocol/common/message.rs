use std::any::type_name;

use bytes::BufMut;
use serde::Serialize;

use crate::protocol::ParseError;

/// Trait for JSON-serializable messages.
pub trait JsonMessage: Serialize {
    /// Converts the message to a JSON string.
    ///
    /// This is infallible since we control the types that implement this trait
    /// and ensure they can always be serialized to JSON.
    fn to_string(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| panic!("failed to encode {} to JSON", type_name::<Self>()))
    }
}

/// Trait for binary message payload encoding/decoding.
///
/// This trait handles the raw payload without the opcode byte.
/// Protocol-specific modules (e.g., v1) provide a `BinaryMessage` trait
/// that adds the appropriate opcode framing.
pub trait BinaryPayload<'a>: Sized + 'a {
    /// Parses a binary payload from the provided buffer.
    ///
    /// The caller is responsible for stripping off the opcode.
    fn parse_payload(data: &'a [u8]) -> Result<Self, ParseError>;

    /// Returns the size of the encoded payload in bytes.
    fn payload_size(&self) -> usize;

    /// Writes the payload into the provided buffer.
    ///
    /// The buffer must have enough capacity to hold the payload.
    fn write_payload(&self, buf: &mut impl BufMut);
}
