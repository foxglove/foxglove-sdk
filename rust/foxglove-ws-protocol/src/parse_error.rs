/// An error encountered while parsing a message.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// Invalid opcode for a binary message.
    #[error("Unknown binary opcode {0}")]
    InvalidOpcode(u8),
    /// The buffer for a binary message was too short to decode.
    #[error("Buffer too short")]
    BufferTooShort,
    /// The fetch asset response contained an invalid status code.
    #[error("Invalid fetch asset status {0}")]
    InvalidFetchAssetStatus(u8),
    /// Invalid UTF-8.
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    /// Invalid JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
