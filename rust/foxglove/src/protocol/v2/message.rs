//! Binary message encoding for Foxglove protocol v2.

/// Trait a binary message with v2 protocol opcodes.
pub trait BinaryMessage {
    /// Encodes the message to bytes with the v2 opcode.
    fn to_bytes(&self) -> Vec<u8>;
}
