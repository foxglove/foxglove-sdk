//! This crate provides utilities for quickly building a remote data loader upstream server.
//!
//! It handles server setup, routing, and provides a framework for implementing authentication,
//! manifest generation, and MCAP data streaming with a simple, linear API.
//!
//! # Features
//!
//! - **`async`** (default): Enables the async API ([`UpstreamServer`], [`SourceBuilder`], [`serve`])
//! - **`blocking`**: Enables the blocking API ([`UpstreamServerBlocking`], [`SourceBuilderBlocking`], [`serve_blocking`])
//!
//! # Quick Start
//!
//! 1. Define a server type (e.g., `struct MyServer;`)
//! 2. Implement [`UpstreamServer`] (async) or [`UpstreamServerBlocking`] (sync)
//! 3. Call [`serve`] or [`serve_blocking`] to start the server
//!
//! See `examples/demo.rs` and `examples/demo_blocking.rs` for async and blocking examples, respectively.
//!
//! # Building a data source
//!
//! [`UpstreamServer::build_source`] receives a [`SourceBuilder`]. Your implementation should:
//!
//! 1. **Declare channels** - Call [`SourceBuilder::channel`] to declare channels.
//! 2. **Set manifest metadata** - If [`SourceBuilder::manifest`] returns `Some`, set the manifest options.
//! 3. **Stream data** - If [`SourceBuilder::into_stream_handle`] returns `Some`, log messages to the declared channels.
//!
//! The blocking version works the same way, except it receives a [`SourceBuilderBlocking`] instead.
//!
//! # Endpoints
//!
//! | Route | Purpose |
//! |-------|---------|
//! | `GET /v1/manifest` | Returns a JSON description of the data source |
//! | `GET /v1/data` | Streams MCAP data |

mod manifest;

#[cfg(feature = "async")]
mod serve_async;
#[cfg(feature = "async")]
pub use serve_async::*;

#[cfg(feature = "blocking")]
mod serve_blocking;
#[cfg(feature = "blocking")]
pub use serve_blocking::*;

use std::{hash::Hash, num::NonZeroU16};

pub use axum::BoxError;
use axum::{
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
pub use chrono::{DateTime, Utc};
use tracing::warn;
pub use url::Url;

use foxglove::{stream::McapStreamHandle, Channel, Encode};

/// Route for the manifest endpoint.
pub const MANIFEST_ROUTE: &str = "/v1/manifest";

/// Route for the data endpoint.
pub const DATA_ROUTE: &str = "/v1/data";

/// Error type for authentication and authorization failures.
#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    /// Credentials required but invalid or missing (HTTP 401).
    #[error("unauthenticated")]
    Unauthenticated,

    /// Credentials valid but access denied (HTTP 403).
    #[error("forbidden")]
    Forbidden,

    /// An unexpected error occurred.
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
                warn!(%error, "error during auth");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Metadata for a data source manifest.
#[derive(Debug, Clone)]
pub struct ManifestOpts {
    /// Unique cache key for this data source.
    ///
    /// You can set this manually, or use [`generate_source_id`] to create a stable ID from your
    /// parameters.
    ///
    /// **Important:** Data returned for the same `id` must always be identical.
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

impl Default for ManifestOpts {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            start_time: DateTime::<Utc>::MIN_UTC,
            end_time: DateTime::<Utc>::MAX_UTC,
        }
    }
}

/// Generate a unique source ID for caching.
///
/// The ID is constructed by joining the name, revision, and a hash of the parameters with a hyphen.
///
/// # Arguments
///
/// * `name` - Identifies this type of data source (e.g., "flight-data")
/// * `revision` - Bump when your data generation logic changes
/// * `params` - Parameters that affect the output data
///
/// # Example
///
/// ```rust
/// # use foxglove_remote_data_loader_upstream::generate_source_id;
/// let id = generate_source_id("flight-data", 1, &"flight-123");
/// assert!(id.starts_with("flight-data-r1-"));
/// ```
pub fn generate_source_id(name: &str, revision: u64, params: &impl Hash) -> String {
    struct Blake3Hasher(blake3::Hasher);

    impl std::hash::Hasher for Blake3Hasher {
        fn write(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }

        fn finish(&self) -> u64 {
            unimplemented!("should never be called")
        }
    }

    let mut hasher = Blake3Hasher(blake3::Hasher::new());
    params.hash(&mut hasher);
    let params_hash = hasher.0.finalize();
    format!("{}-r{}-{}", name, revision, params_hash.to_hex())
}

/// A convenience wrapper around a [`foxglove::Channel`] that may or may not exist.
///
/// Returned by [`SourceBuilder::channel`] or [`SourceBuilderBlocking::channel`], which only create
/// a [`foxglove::Channel`] if they were called while in streaming mode.
///
/// This type's methods panic if the underlying channel does not exist.
pub struct MaybeChannel<T: Encode>(Option<Channel<T>>);

impl<T: Encode> MaybeChannel<T> {
    /// Logs a message to the channel with the given timestamp.
    ///
    /// # Panics
    ///
    /// Panics if called in manifest mode.
    pub fn log_with_time(&self, msg: &T, timestamp: impl foxglove::ToUnixNanos) {
        self.0
            .as_ref()
            .expect("called `MaybeChannel::log_with_time()` while in manifest mode")
            .log_with_time(msg, timestamp)
    }

    /// Unwraps the inner channel.
    ///
    /// Use this for advanced operations like `log_with_meta_to_sink()`.
    ///
    /// # Panics
    ///
    /// Panics if called in manifest mode.
    pub fn into_inner(self) -> Channel<T> {
        self.0
            .expect("called `MaybeChannel::into_inner()` while in manifest mode")
    }
}

struct ManifestBuilder {
    manifest_opts: ManifestOpts,
    topics: Vec<manifest::Topic>,
    schemas: Vec<manifest::Schema>,
    next_schema_id: NonZeroU16,
}

impl ManifestBuilder {
    fn new() -> Self {
        Self {
            manifest_opts: ManifestOpts::default(),
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

    fn build(self, base_url: Url) -> manifest::Manifest {
        manifest::Manifest {
            name: Some(self.manifest_opts.name),
            sources: vec![manifest::UpstreamSource::Streamed(
                manifest::StreamedSource {
                    url: base_url
                        .join(DATA_ROUTE)
                        .expect("should always succeed since DATA_ROUTE is valid"),
                    id: Some(self.manifest_opts.id),
                    topics: self.topics,
                    schemas: self.schemas,
                    start_time: self.manifest_opts.start_time,
                    end_time: self.manifest_opts.end_time,
                },
            )],
        }
    }
}

enum BuilderMode<'a> {
    Manifest { builder: &'a mut ManifestBuilder },
    Stream { handle: McapStreamHandle },
}

impl<'a> BuilderMode<'a> {
    fn channel<T: Encode>(&mut self, topic: String) -> MaybeChannel<T> {
        match self {
            BuilderMode::Manifest { builder } => {
                builder.add_channel::<T>(topic);
                MaybeChannel(None)
            }
            BuilderMode::Stream { handle } => {
                MaybeChannel(Some(handle.channel_builder(&topic).build::<T>()))
            }
        }
    }

    fn manifest(&mut self) -> Option<&mut ManifestOpts> {
        match self {
            BuilderMode::Manifest { builder } => Some(&mut builder.manifest_opts),
            BuilderMode::Stream { .. } => None,
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
    fn test_generate_source_id_snapshot() {
        let id = generate_source_id("flight-data", 1, &"ABC123");
        insta::assert_snapshot!(id);
    }

    #[test]
    fn test_manifest_builder_snapshot() {
        #[derive(foxglove::Encode)]
        struct TestMessage {
            value: i32,
        }

        let mut builder = ManifestBuilder::new();
        builder.manifest_opts = ManifestOpts {
            id: "test-id".into(),
            name: "Test Source".into(),
            start_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        };
        builder.add_channel::<TestMessage>("/topic1".into());
        builder.add_channel::<TestMessage>("/topic2".into()); // Same schema type - snapshot will show only 1 schema

        let manifest = builder.build("http://example.com".parse().unwrap());
        insta::assert_json_snapshot!(manifest);
    }
}
