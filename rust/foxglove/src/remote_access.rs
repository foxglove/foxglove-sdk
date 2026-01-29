mod connection;
mod participant;

use thiserror::Error;

pub(crate) use connection::{Connection, ConnectionOptions};

/// An error type for remote access errors.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum RemoteAccessError {
    // Note: don't expose livekit error types here, we don't want them to become part of the public API
    /// An error occurred while writing to the stream.
    #[error("Stream error: {0:?}")]
    StreamError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    /// An I/O error.
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Connection stopped")]
    ConnectionStopped,
}

impl From<livekit::StreamError> for RemoteAccessError {
    fn from(error: livekit::StreamError) -> Self {
        match error {
            livekit::StreamError::Io(e) => RemoteAccessError::IoError(e),
            _ => RemoteAccessError::StreamError(error.to_string()),
        }
    }
}

impl From<livekit::RoomError> for RemoteAccessError {
    fn from(error: livekit::RoomError) -> Self {
        RemoteAccessError::ConnectionError(error.to_string())
    }
}
