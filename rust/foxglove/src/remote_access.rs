//! Remote access implementation.

mod capability;
mod client;
mod connection;
mod credentials_provider;
mod gateway;
mod listener;
mod participant;
mod session;
mod session_state;

pub use capability::Capability;
pub use client::Client;
pub use gateway::{Gateway, GatewayHandle};
pub use listener::Listener;

use thiserror::Error;

use crate::api_client::FoxgloveApiClientError;

use self::credentials_provider::CredentialsError;

/// Internal error type for the remote access module.
#[derive(Error, Debug)]
pub(super) enum RemoteAccessError {
    /// An error from a LiveKit byte stream operation.
    #[error("Stream error: {0}")]
    Stream(livekit::StreamError),
    /// An error from a LiveKit room connection or operation.
    #[error("Room error: {0}")]
    Room(livekit::RoomError),
    /// An authentication or credential error.
    #[error("Authentication error: {0}")]
    Auth(FoxgloveApiClientError),
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

/// The only Foxglove API calls in this module are `fetch_device_info` and
/// `authorize_remote_viz`, both of which are auth-related, so mapping all
/// API client errors to `Auth` is appropriate here.
impl From<FoxgloveApiClientError> for RemoteAccessError {
    fn from(error: FoxgloveApiClientError) -> Self {
        RemoteAccessError::Auth(error)
    }
}

impl From<CredentialsError> for RemoteAccessError {
    fn from(error: CredentialsError) -> Self {
        match error {
            CredentialsError::FetchFailed(e) => RemoteAccessError::Auth(e),
        }
    }
}

impl RemoteAccessError {
    /// Returns `true` if this error is likely auth-related and credentials should be refreshed.
    ///
    /// All `Room` errors are treated as potentially auth-related, even though only a subset
    /// (e.g. `RoomError::Engine(EngineError::Signal(SignalError::Client(401, ...)))`) truly
    /// indicate expired or invalid credentials. The blanket match is intentional: credential
    /// refresh is cheap, and the only call site is a retry loop with a 30-second interval, so
    /// an unnecessary refresh has negligible cost. The native `RoomError` is preserved here so
    /// that this logic can be refined in the future if needed (e.g. to skip refresh for
    /// `AlreadyClosed` or `TrackAlreadyPublished`).
    pub(super) fn should_clear_credentials(&self) -> bool {
        matches!(
            self,
            RemoteAccessError::Auth(_) | RemoteAccessError::Room(_)
        )
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

    #[test]
    fn should_clear_credentials_for_room_errors() {
        let err = RemoteAccessError::Room(livekit::RoomError::AlreadyClosed);
        assert!(err.should_clear_credentials());
    }

    #[test]
    fn should_not_clear_credentials_for_io_errors() {
        let err = RemoteAccessError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken",
        ));
        assert!(!err.should_clear_credentials());
    }

    #[test]
    fn should_not_clear_credentials_for_stream_errors() {
        let err = RemoteAccessError::Stream(livekit::StreamError::AlreadyClosed);
        assert!(!err.should_clear_credentials());
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
    async fn api_client_error_converts_to_auth_variant() {
        let err = RemoteAccessError::from(make_api_client_error().await);
        assert!(
            matches!(err, RemoteAccessError::Auth(_)),
            "FoxgloveApiClientError should convert to RemoteAccessError::Auth"
        );
    }

    #[tokio::test]
    async fn credentials_error_converts_to_auth_variant() {
        let api_err = make_api_client_error().await;
        let cred_err = CredentialsError::FetchFailed(api_err);
        let err = RemoteAccessError::from(cred_err);
        assert!(
            matches!(err, RemoteAccessError::Auth(_)),
            "CredentialsError::FetchFailed should convert to RemoteAccessError::Auth"
        );
    }

    #[tokio::test]
    async fn should_clear_credentials_for_auth_errors() {
        let err = RemoteAccessError::from(make_api_client_error().await);
        assert!(err.should_clear_credentials());
    }
}
