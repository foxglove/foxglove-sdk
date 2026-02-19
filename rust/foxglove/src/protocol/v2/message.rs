//! Binary message encoding for Foxglove protocol v2.

use bytes::BufMut;

/// Trait for a binary message with v2 protocol opcodes.
pub trait BinaryMessage {
    /// Returns the total encoded length of this message (opcode byte + payload).
    fn encoded_len(&self) -> usize;

    /// Encodes the message (opcode byte + payload) into the provided buffer.
    fn encode(&self, buf: &mut impl BufMut);

    /// Encodes the message to a new `Vec<u8>`.
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_len());
        self.encode(&mut buf);
        buf
    }
}
