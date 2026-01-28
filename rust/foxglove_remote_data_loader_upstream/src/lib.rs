//! This crate provides utilities for quickly building a remote data loader upstream server.
//!
//! It handles server setup, routing, and provides a framework for implementing authentication,
//! manifest generation, and MCAP data streaming with a simple API.
//!
//! # Features
//!
//! - **`async`** (default): Enables the async API ([`UpstreamServer`], [`serve`])
//! - **`blocking`**: Enables the blocking API ([`blocking::UpstreamServer`], [`blocking::serve`])
//!
//! # Overview
//!
//! Implement [`UpstreamServer`] to stream data from your backend to Foxglove.
//!
//! For example, to stream flight telemetry, you could define a `FlightServer`
//! implementing [`UpstreamServer`]:
//!
//! 1. **`FlightServer`** holds the database connection
//! 2. **Request parameters** in [`QueryParams`](UpstreamServer::QueryParams) (`flight_id`, `start_time`) identify the data to load
//! 3. **[`auth`](UpstreamServer::auth)** validates credentials
//! 4. **[`initialize`](UpstreamServer::initialize)** creates a [`Channel`]`<Telemetry>` and returns it in a [`Context`](UpstreamServer::Context)
//! 5. **[`metadata`](UpstreamServer::metadata)** returns [`Metadata`] with the flight name and time range
//! 6. **[`stream`](UpstreamServer::stream)** queries rows and logs them to the channel
//!
//! See [`UpstreamServer`] for the trait methods, then call [`serve`] to start the server.
//! For a blocking API, see [`blocking::UpstreamServer`] and [`blocking::serve`].
//!
//! See `examples/demo.rs` or `examples/demo_blocking.rs` for complete examples.

mod manifest;

#[cfg(feature = "async")]
mod serve_async;
#[cfg(feature = "async")]
pub use serve_async::*;

#[cfg(feature = "blocking")]
pub mod blocking;

use std::{
    num::NonZeroU16,
    sync::{Arc, LazyLock},
};

pub use axum::BoxError;
use axum::{
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
pub use chrono::{DateTime, Utc};
use tracing::warn;

use foxglove::{Channel, Context, Encode, RawChannel, Sink, SinkId};

/// Route for the manifest endpoint.
pub const MANIFEST_ROUTE: &str = "/v1/manifest";

/// Route for the data endpoint.
pub const DATA_ROUTE: &str = "/v1/data";

/// Error type for authentication and authorization failures.
///
/// Use `AuthError::Unauthenticated` for missing/invalid credentials (HTTP 401),
/// `AuthError::Forbidden` for valid credentials but denied access (HTTP 403),
/// or use `AuthError::other()` to wrap unexpected errors (HTTP 500).
///
/// # Example
///
/// ```rust
/// # use foxglove_remote_data_loader_upstream::AuthError;
/// async fn auth(token: Option<&str>) -> Result<(), AuthError> {
///     let token = token.ok_or(AuthError::Unauthenticated)?;
///     // Use .map_err(AuthError::other) to convert other errors
///     let _claims = validate_token(token).map_err(AuthError::other)?;
///     Ok(())
/// }
/// # fn validate_token(_: &str) -> Result<(), std::io::Error> { Ok(()) }
/// ```
#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    /// Credentials are required, but are invalid or missing (HTTP 401).
    #[error("unauthenticated")]
    Unauthenticated,

    /// Credentials are recognized, but access is denied (HTTP 403).
    #[error("forbidden")]
    Forbidden,

    /// An unexpected error occurred (HTTP 500).
    #[error(transparent)]
    Other(BoxError),
}

impl AuthError {
    /// Create an error from an arbitrary error payload.
    pub fn other(error: impl Into<BoxError>) -> Self {
        Self::Other(error.into())
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthenticated => StatusCode::UNAUTHORIZED.into_response(),
            Self::Forbidden => StatusCode::FORBIDDEN.into_response(),
            Self::Other(error) => {
                warn!(%error, "unexpected error during auth");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Metadata describing a data source.
///
/// Returned from [`UpstreamServer::metadata`] to describe the data source in the manifest.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Unique cache key for this data source.
    ///
    /// Must be deterministic - the same input parameters must always produce the same ID.
    /// Include all parameters that affect the output data.
    ///
    /// # Example
    ///
    /// ```ignore
    /// id: format!("flight-v1-{}-{}-{}",
    ///     params.flight_id,
    ///     params.start_time,
    ///     params.end_time),
    /// ```
    ///
    /// **Tip:** Include a version prefix (e.g., `"v1-"`) and bump it when your
    /// data generation logic changes to invalidate cached data.
    ///
    /// For automatic field inclusion, consider serializing your params with
    /// [`Serialize`](serde::Serialize) or [`Debug`].
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// Earliest timestamp of any message in the data.
    ///
    /// A lower bound can be used if the exact value is not known.
    pub start_time: DateTime<Utc>,

    /// Latest timestamp of any message in the data.
    ///
    /// An upper bound can be used if the exact value is not known.
    pub end_time: DateTime<Utc>,
}

/// A sink that panics if any message is logged to it.
///
/// Used internally for channels created during manifest generation to catch
/// bugs where code attempts to log messages during metadata generation.
struct PanicSink {
    id: SinkId,
}

impl PanicSink {
    fn new() -> Self {
        Self { id: SinkId::next() }
    }
}

impl Sink for PanicSink {
    fn id(&self) -> SinkId {
        self.id
    }

    fn log(
        &self,
        _channel: &RawChannel,
        _msg: &[u8],
        _metadata: &foxglove::Metadata,
    ) -> Result<(), foxglove::FoxgloveError> {
        panic!("attempted to log message to channel not created for streaming");
    }
}

/// Trait for declaring channels during [`UpstreamServer::initialize`].
pub trait ChannelRegistry: Send {
    /// Declare a channel for logging messages.
    ///
    /// The returned [`Channel<T>`] should be stored in your [`UpstreamServer::Context`], so you can
    /// log messages to it in [`UpstreamServer::stream`] (or the blocking equivalent).
    ///
    /// # Notes
    ///
    /// You should only log messages in your `stream` implementation. If the
    /// initiating HTTP request is not a data streaming request, attempting to log to the returned
    /// channel will panic.
    fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> Channel<T>;
}

static PANIC_CONTEXT: LazyLock<Arc<Context>> = LazyLock::new(|| {
    let context = Context::new();
    context.add_sink(Arc::new(PanicSink::new()));
    context
});

impl ChannelRegistry for ManifestBuilder {
    /// Add a channel to the manifest.
    ///
    /// The returned [`Channel<T>`] is connected to a [`PanicSink`] to catch bugs where logging
    /// happens outside of [`UpstreamServer::stream`].
    fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> Channel<T> {
        let topic = topic.into();
        self.add_channel::<T>(topic.clone());
        PANIC_CONTEXT.channel_builder(&topic).build::<T>()
    }
}

impl ChannelRegistry for StreamHandle {
    /// Add a channel to this handle's MCAP stream.
    ///
    /// The returned [`Channel<T>`] is connected to this handle's [`foxglove::stream::McapStream`].
    fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> Channel<T> {
        self.channel_builder(topic).build::<T>()
    }
}

struct ManifestBuilder {
    topics: Vec<manifest::Topic>,
    schemas: Vec<manifest::Schema>,
    next_schema_id: NonZeroU16,
}

impl ManifestBuilder {
    fn new() -> Self {
        Self {
            topics: Vec::new(),
            schemas: Vec::new(),
            next_schema_id: NonZeroU16::MIN,
        }
    }

    fn add_channel<T: Encode>(&mut self, topic: String) {
        let schema_id = T::get_schema().map(|s| self.add_schema(s));
        self.topics.push(manifest::Topic {
            name: topic,
            message_encoding: T::get_message_encoding(),
            schema_id,
        });
    }

    fn add_schema(&mut self, schema: foxglove::Schema) -> NonZeroU16 {
        // Do not add duplicate schemas.
        let existing = self.schemas.iter().find(|existing| {
            existing.name == schema.name
                && existing.encoding == schema.encoding
                && existing.data.as_ref() == schema.data.as_ref()
        });

        if let Some(existing) = existing {
            existing.id
        } else {
            let id = self.next_schema_id;
            self.next_schema_id = self
                .next_schema_id
                .checked_add(1)
                .expect("should not add more than 65535 schemas");
            self.schemas.push(manifest::Schema {
                id,
                name: schema.name,
                encoding: schema.encoding,
                data: schema.data.into(),
            });
            id
        }
    }

    fn build(self, metadata: Metadata) -> manifest::Manifest {
        manifest::Manifest {
            name: Some(metadata.name),
            sources: vec![manifest::UpstreamSource::Streamed(
                manifest::StreamedSource {
                    url: DATA_ROUTE.to_string(),
                    id: Some(metadata.id),
                    topics: self.topics,
                    schemas: self.schemas,
                    start_time: metadata.start_time,
                    end_time: metadata.end_time,
                },
            )],
        }
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_manifest_builder_snapshot() {
        #[derive(foxglove::Encode)]
        struct TestMessage {
            value: i32,
        }

        let mut builder = ManifestBuilder::new();
        builder.add_channel::<TestMessage>("/topic1".into());
        builder.add_channel::<TestMessage>("/topic2".into()); // Same schema type - snapshot will show only 1 schema

        let metadata = Metadata {
            id: "test-id".into(),
            name: "Test Source".into(),
            start_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        };
        let manifest = builder.build(metadata);
        insta::assert_json_snapshot!(manifest);
    }

    #[test]
    #[should_panic(expected = "attempted to log message to channel not created for streaming")]
    fn test_panic_sink_panics_on_log() {
        #[derive(foxglove::Encode)]
        struct TestMessage {
            value: i32,
        }

        let mut manifest_builder = ManifestBuilder::new();
        let channel = manifest_builder.channel::<TestMessage>("/test");
        // This should panic because we're using a PanicSink
        channel.log(&TestMessage { value: 42 });
    }
}
