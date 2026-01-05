use std::any::type_name;

use serde::Serialize;

use crate::websocket::ws_protocol::ParseError;

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

/// Trait for a binary message.
pub trait BinaryMessage<'a>: Sized + 'a {
    /// Parses a binary message from the provided buffer.
    ///
    /// The caller is responsible for stripping off the opcode.
    fn parse_binary(data: &'a [u8]) -> Result<Self, ParseError>;

    /// Encodes a binary message to a new mutable buffer.
    fn to_bytes(&self) -> Vec<u8>;
}
