//! Remote access implementation.

mod capability;
mod client;
mod connection;
mod gateway;
mod listener;
mod participant;
mod participants;
pub(super) mod protocol_version;
mod qos;
mod rtt_tracker;
pub mod service;
mod session;
mod session_state;
mod sse;
mod watch;

pub use crate::remote_common::ClientId;
pub use crate::remote_common::connection_graph::ConnectionGraph;
pub use capability::Capability;
pub use client::Client;
pub use connection::ConnectionStatus;
pub use gateway::{Gateway, GatewayHandle};
pub use listener::Listener;
pub use qos::{QosClassifier, QosProfile, QosProfileBuilder, Reliability};

// Re-export parameter types so callers can construct parameter values.
pub use crate::protocol::v2::parameter::{Parameter, ParameterType, ParameterValue};

// Re-export status types so callers can publish and remove status messages.
pub use crate::protocol::v2::server::status::{Level as StatusLevel, Status};

// Re-export fetch-asset types.
pub use crate::remote_common::fetch_asset::AssetHandler;
/// Type alias for the remote-access-specific asset responder.
pub type AssetResponder = crate::remote_common::fetch_asset::AssetResponder<Client>;

use thiserror::Error;

use crate::api_client::FoxgloveApiClientError;

/// Internal error type for the remote access module.
#[derive(Error, Debug)]
pub(super) enum RemoteAccessError {
    /// An error from a LiveKit byte stream operation.
    #[error("Stream error: {0}")]
    Stream(livekit::StreamError),
    /// An error from a LiveKit room connection or operation.
    #[error("Room error: {0}")]
    Room(livekit::RoomError),
    /// A failed Foxglove API call.
    #[error("API error: {0}")]
    Api(FoxgloveApiClientError),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<livekit::StreamError> for RemoteAccessError {
    fn from(error: livekit::StreamError) -> Self {
        match error {
            livekit::StreamError::Io(e) => RemoteAccessError::Io(e),
            other => RemoteAccessError::Stream(other),
        }
    }
}

impl From<livekit::RoomError> for RemoteAccessError {
    fn from(error: livekit::RoomError) -> Self {
        RemoteAccessError::Room(error)
    }
}

impl From<FoxgloveApiClientError> for RemoteAccessError {
    fn from(error: FoxgloveApiClientError) -> Self {
        RemoteAccessError::Api(error)
    }
}

impl From<livekit::StreamError> for Box<RemoteAccessError> {
    fn from(e: livekit::StreamError) -> Self {
        Box::new(RemoteAccessError::from(e))
    }
}

impl From<livekit::RoomError> for Box<RemoteAccessError> {
    fn from(e: livekit::RoomError) -> Self {
        Box::new(RemoteAccessError::from(e))
    }
}

impl From<FoxgloveApiClientError> for Box<RemoteAccessError> {
    fn from(e: FoxgloveApiClientError) -> Self {
        Box::new(RemoteAccessError::from(e))
    }
}

impl From<std::io::Error> for Box<RemoteAccessError> {
    fn from(e: std::io::Error) -> Self {
        Box::new(RemoteAccessError::from(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_io_error_converts_to_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
        let stream_err = livekit::StreamError::Io(io_err);
        let err = RemoteAccessError::from(stream_err);
        assert!(
            matches!(err, RemoteAccessError::Io(_)),
            "StreamError::Io should convert to RemoteAccessError::Io"
        );
    }

    #[test]
    fn stream_non_io_error_converts_to_stream_variant() {
        let stream_err = livekit::StreamError::AlreadyClosed;
        let err = RemoteAccessError::from(stream_err);
        assert!(
            matches!(err, RemoteAccessError::Stream(_)),
            "Non-Io StreamError should convert to RemoteAccessError::Stream"
        );
    }

    #[test]
    fn room_error_converts_to_room_variant() {
        let room_err = livekit::RoomError::AlreadyClosed;
        let err = RemoteAccessError::from(room_err);
        assert!(
            matches!(err, RemoteAccessError::Room(_)),
            "RoomError should convert to RemoteAccessError::Room"
        );
    }

    #[test]
    fn io_error_converts_to_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = RemoteAccessError::from(io_err);
        assert!(
            matches!(err, RemoteAccessError::Io(_)),
            "io::Error should convert to RemoteAccessError::Io"
        );
    }

    /// Helper to produce a `FoxgloveApiClientError` by sending a request with a bad token
    /// to a mock server that returns 401.
    async fn make_api_client_error() -> FoxgloveApiClientError {
        use crate::api_client::test_utils::create_test_server;
        use crate::api_client::{DeviceToken, FoxgloveApiClientBuilder};
        let server = create_test_server().await;
        let client = FoxgloveApiClientBuilder::new(DeviceToken::new("bad-token"))
            .base_url(server.url())
            .build()
            .expect("client should build successfully");
        // `fetch_device_info` will fail with 401 because of the bad token.
        match client.fetch_device_info().await {
            Err(e) => e,
            Ok(_) => panic!("expected fetch_device_info to fail with bad token"),
        }
    }

    #[tokio::test]
    async fn api_client_error_converts_to_api_variant() {
        let err = RemoteAccessError::from(make_api_client_error().await);
        assert!(
            matches!(err, RemoteAccessError::Api(_)),
            "FoxgloveApiClientError should convert to RemoteAccessError::Api"
        );
    }
}
