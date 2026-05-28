use bytes::Buf;

/// A trait representing a message serialized by the SDK, which can be decoded.
pub trait Decode {
    /// The error type returned by methods in this trait.
    type Error: std::error::Error;

    /// Decode a message from a serialized buffer.
    fn decode(buf: impl Buf) -> Result<Self, Self::Error>
    where
        Self: Sized;
}
