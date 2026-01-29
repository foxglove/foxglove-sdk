//! Binary message encoding for Foxglove protocol v1.

/// Trait a binary message with v1 protocol opcodes.
pub trait BinaryMessage {
    /// Encodes the message to bytes with the v1 opcode.
    fn to_bytes(&self) -> Vec<u8>;
}
