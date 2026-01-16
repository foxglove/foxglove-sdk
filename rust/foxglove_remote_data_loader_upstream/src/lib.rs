//! This crate provides utilities for quickly building a remote data loader upstream server.
//!
//! It handles server setup, routing, and provides a framework for implementing authentication,
//! manifest generation, and MCAP data streaming with a simple, linear API.
//!
//! # Quick Start
//!
//! 1. Define a server type (e.g., `struct MyServer;`)
//! 2. Implement [`UpstreamServer`] (async) or [`UpstreamServerBlocking`] (sync)
//! 3. Call [`serve`] or [`serve_blocking`] to start the server
//!
//! See `examples/demo.rs` for an async example.
//!
//! # API Flow
//!
//! The [`UpstreamServer::build_source`] method receives a [`SourceBuilder`] and follows this flow:
//!
//! 1. **Declare channels** - Call [`SourceBuilder::channel`] to declare topics
//! 2. **Set manifest metadata** - If [`SourceBuilder::manifest`] returns `Some`, set the opts
//! 3. **Stream data** - If [`SourceBuilder::into_stream_handle`] returns `Some`, log data and finish
//!
//! # Endpoints
//!
//! | Route | Purpose |
//! |-------|---------|
//! | `GET /v1/manifest` | Returns manifest JSON with source metadata |
//! | `GET /v1/data` | Streams MCAP data |

pub mod manifest;

use std::{
    collections::hash_map::DefaultHasher,
    error::Error as StdError,
    future::Future,
    hash::{Hash, Hasher},
    net::SocketAddr,
    num::NonZeroU16,
    sync::Arc,
};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use foxglove::{stream::McapStreamHandle, Channel, Encode, FoxgloveError};
use futures::StreamExt;
use manifest::{Schema, StreamedSource, Topic, UpstreamSource};
use serde::de::DeserializeOwned;
use tokio::runtime::Handle;
use tracing::warn;

// ============================================================================
// Auth types
// ============================================================================

/// Error type for authentication and authorization failures.
#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    /// Credentials invalid or missing (HTTP 401).
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),

    /// Credentials valid but access denied (HTTP 403).
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// An unexpected error occurred.
    #[error(transparent)]
    Other(Box<dyn StdError + Send>),
}

impl AuthError {
    /// Create an unauthenticated error with a message.
    pub fn unauthenticated(msg: impl Into<String>) -> Self {
        Self::Unauthenticated(msg.into())
    }

    /// Create a forbidden error with a message.
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// Create an error from an arbitrary error payload.
    pub fn other(error: impl Into<Box<dyn StdError + Send>>) -> Self {
        Self::Other(error.into())
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthenticated(_) => StatusCode::UNAUTHORIZED.into_response(),
            Self::Forbidden(_) => StatusCode::FORBIDDEN.into_response(),
            Self::Other(error) => {
                warn!(%error, "auth error");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

// ============================================================================
// ManifestOpts
// ============================================================================

/// Metadata for a data source manifest.
///
/// Set these fields when [`SourceBuilder::manifest`] returns `Some`.
#[derive(Debug, Clone)]
pub struct ManifestOpts {
    /// Unique cache key for this data source.
    ///
    /// Use [`generate_source_id`] to create a stable ID from your parameters.
    ///
    /// **Important:** Data returned for the same `id` must always be identical.
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// Earliest timestamp in the data.
    pub start_time: DateTime<Utc>,

    /// Latest timestamp in the data.
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
/// # Arguments
///
/// * `name` - Identifies this type of data source (e.g., "flight-data")
/// * `revision` - Bump when your data generation logic changes
/// * `params` - Parameters that affect the output data
///
/// # Example
///
/// ```rust
/// use foxglove_remote_data_loader_upstream::generate_source_id;
///
/// let id = generate_source_id("flight-data", 1, &"flight-123");
/// assert!(id.starts_with("flight-data-r1-"));
/// ```
pub fn generate_source_id(name: &str, revision: u64, params: &impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    revision.hash(&mut hasher);
    params.hash(&mut hasher);

    let slug = name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "-");
    format!("{}-r{}-{:016x}", slug, revision, hasher.finish())
}

// ============================================================================
// StreamHandle (async)
// ============================================================================

/// Handle for streaming MCAP data (async version).
///
/// Returned by [`SourceBuilder::into_stream_handle`] when processing a data request.
pub struct StreamHandle {
    handle: McapStreamHandle,
}

impl StreamHandle {
    /// Flush buffered MCAP data to the stream.
    pub async fn flush(&mut self) -> Result<(), FoxgloveError> {
        self.handle.flush().await
    }

    /// Finish writing and close the MCAP stream.
    ///
    /// This must be called to ensure all data is written.
    pub async fn finish(self) -> Result<(), FoxgloveError> {
        self.handle.close().await
    }
}

// ============================================================================
// StreamHandleBlocking (sync)
// ============================================================================

/// Handle for streaming MCAP data (blocking version).
///
/// Returned by [`SourceBuilderBlocking::into_stream_handle`] when processing a data request.
pub struct StreamHandleBlocking {
    handle: McapStreamHandle,
}

impl StreamHandleBlocking {
    /// Flush buffered MCAP data to the stream.
    pub fn flush(&mut self) -> Result<(), FoxgloveError> {
        Handle::current().block_on(self.handle.flush())
    }

    /// Finish writing and close the MCAP stream.
    ///
    /// This must be called to ensure all data is written.
    pub fn finish(self) -> Result<(), FoxgloveError> {
        Handle::current().block_on(self.handle.close())
    }
}

// ============================================================================
// MaybeChannel
// ============================================================================

/// A channel that may or may not be active.
///
/// In manifest mode, this wraps `None` and will panic if you attempt to log.
/// In streaming mode, this wraps a real channel.
///
/// Use [`MaybeChannel::log`] to log messages, or [`MaybeChannel::into_inner`]
/// to get the underlying [`Channel<T>`] for advanced operations.
pub struct MaybeChannel<T: Encode>(Option<Channel<T>>);

impl<T: Encode> MaybeChannel<T> {
    /// Logs a message to the channel.
    ///
    /// # Panics
    ///
    /// Panics if called in manifest mode.
    pub fn log(&self, msg: &T) {
        self.0
            .as_ref()
            .expect(
                "cannot log in manifest mode; \
                 check source.manifest().is_none() before logging",
            )
            .log(msg)
    }

    /// Unwraps the inner channel.
    ///
    /// Use this for advanced operations like `log_with_time()` or `log_with_meta()`.
    ///
    /// # Panics
    ///
    /// Panics if called in manifest mode.
    pub fn into_inner(self) -> Channel<T> {
        self.0.expect(
            "cannot unwrap channel in manifest mode; \
             check source.manifest().is_none() before unwrapping",
        )
    }
}

// ============================================================================
// SourceBuilder (async)
// ============================================================================

enum SourceMode<'a> {
    Manifest {
        manifest_opts: &'a mut ManifestOpts,
        topics: &'a mut Vec<Topic>,
        schemas: &'a mut Vec<Schema>,
        next_schema_id: NonZeroU16,
    },
    Stream {
        handle: McapStreamHandle,
    },
}

impl<'a> SourceMode<'a> {
    fn channel<T: Encode>(&mut self, topic: String) -> MaybeChannel<T> {
        match self {
            SourceMode::Manifest {
                topics,
                schemas,
                next_schema_id,
                ..
            } => {
                // Capture schema info for manifest
                let schema = T::get_schema();
                let schema_id = if let Some(s) = schema {
                    // Check if we already have this schema
                    let existing = schemas.iter().find(|existing| {
                        existing.name == s.name
                            && existing.encoding == s.encoding
                            && existing.data.as_ref() == s.data.as_ref()
                    });

                    if let Some(existing) = existing {
                        Some(existing.id)
                    } else {
                        let id = *next_schema_id;
                        *next_schema_id = next_schema_id.checked_add(1).unwrap();
                        schemas.push(Schema {
                            id,
                            name: s.name,
                            encoding: s.encoding,
                            data: s.data.into(),
                        });
                        Some(id)
                    }
                } else {
                    None
                };

                topics.push(Topic {
                    name: topic,
                    message_encoding: T::get_message_encoding(),
                    schema_id,
                });

                MaybeChannel(None)
            }
            SourceMode::Stream { handle } => {
                MaybeChannel(Some(handle.channel_builder(&topic).build::<T>()))
            }
        }
    }

    fn manifest(&mut self) -> Option<&mut ManifestOpts> {
        match self {
            SourceMode::Manifest { manifest_opts, .. } => Some(manifest_opts),
            SourceMode::Stream { .. } => None,
        }
    }
}

/// Builder for constructing a data source (async version).
///
/// Passed to [`UpstreamServer::build_source`].
pub struct SourceBuilder<'a> {
    mode: SourceMode<'a>,
}

impl<'a> SourceBuilder<'a> {
    /// Declare a channel for logging messages.
    ///
    /// The channel's schema is automatically captured for the manifest.
    /// In streaming mode, messages logged to this channel go to the MCAP stream.
    ///
    /// Returns a [`MaybeChannel`] that wraps `None` in manifest mode and a real
    /// channel in streaming mode. Calling `log()` in manifest mode will panic.
    pub fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> MaybeChannel<T> {
        self.mode.channel(topic.into())
    }

    /// Returns manifest options if this is a manifest request.
    ///
    /// If `Some`, set the fields to provide metadata for the manifest.
    /// If `None`, this is a data request - proceed to streaming.
    pub fn manifest(&mut self) -> Option<&mut ManifestOpts> {
        self.mode.manifest()
    }

    /// Consume the builder and return the stream handle if this is a data request.
    ///
    /// If `Some`, log data to your channels and call `handle.finish()`.
    /// If `None`, this is a manifest request - return early.
    pub fn into_stream_handle(self) -> Option<StreamHandle> {
        match self.mode {
            SourceMode::Manifest { .. } => None,
            SourceMode::Stream { handle } => Some(StreamHandle { handle }),
        }
    }
}

// ============================================================================
// SourceBuilderBlocking (sync)
// ============================================================================

/// Builder for constructing a data source (blocking version).
///
/// Passed to [`UpstreamServerBlocking::build_source`].
pub struct SourceBuilderBlocking<'a> {
    mode: SourceMode<'a>,
}

impl<'a> SourceBuilderBlocking<'a> {
    /// Declare a channel for logging messages.
    ///
    /// The channel's schema is automatically captured for the manifest.
    /// In streaming mode, messages logged to this channel go to the MCAP stream.
    ///
    /// Returns a [`MaybeChannel`] that wraps `None` in manifest mode and a real
    /// channel in streaming mode. Calling `log()` in manifest mode will panic.
    pub fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> MaybeChannel<T> {
        self.mode.channel(topic.into())
    }

    /// Returns manifest options if this is a manifest request.
    pub fn manifest(&mut self) -> Option<&mut ManifestOpts> {
        self.mode.manifest()
    }

    /// Consume the builder and return the stream handle if this is a data request.
    pub fn into_stream_handle(self) -> Option<StreamHandleBlocking> {
        match self.mode {
            SourceMode::Manifest { .. } => None,
            SourceMode::Stream { handle } => Some(StreamHandleBlocking { handle }),
        }
    }
}

// ============================================================================
// UpstreamServer trait (async)
// ============================================================================

/// Async upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints asynchronously.
///
/// # Example
///
/// ```rust,ignore
/// impl UpstreamServer for MyServer {
///     type QueryParams = MyParams;
///     type Error = Infallible;
///
///     async fn auth(&self, token: Option<&str>, params: &MyParams) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     async fn build_source(
///         &self,
///         params: MyParams,
///         mut source: SourceBuilder<'_>,
///     ) -> Result<(), Infallible> {
///         let channel = source.channel::<MyMessage>("/topic");
///
///         if let Some(opts) = source.manifest() {
///             *opts = ManifestOpts { ... };
///         }
///
///         let Some(mut handle) = source.into_stream_handle() else {
///             return Ok(());
///         };
///
///         channel.log(&message);
///         handle.finish().await?;
///         Ok(())
///     }
/// }
/// ```
pub trait UpstreamServer: Send + Sync + 'static {
    /// Query parameters extracted from the request URL.
    ///
    /// Use `#[derive(Deserialize)]` with `#[serde(rename_all = "camelCase")]`.
    type QueryParams: DeserializeOwned + Send;

    /// Error type returned from [`build_source`](UpstreamServer::build_source).
    type Error: StdError + Send;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok(())` to allow access, or an [`AuthError`] to deny.
    fn auth(
        &self,
        bearer_token: Option<&str>,
        params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;

    /// Build the data source.
    ///
    /// This method is called for both manifest and data requests.
    /// Use the [`SourceBuilder`] to declare channels and set options.
    fn build_source(
        &self,
        params: Self::QueryParams,
        source: SourceBuilder<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

// ============================================================================
// UpstreamServerBlocking trait (sync)
// ============================================================================

/// Blocking upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints synchronously.
/// No async code or tokio imports required in your implementation.
///
/// Use [`serve_blocking`] to start the server.
pub trait UpstreamServerBlocking: Send + Sync + 'static {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send + Clone + 'static;

    /// Error type returned from [`build_source`](UpstreamServerBlocking::build_source).
    type Error: StdError + Send + 'static;

    /// Authenticate and authorize the request.
    fn auth(&self, bearer_token: Option<&str>, params: &Self::QueryParams)
        -> Result<(), AuthError>;

    /// Build the data source.
    fn build_source(
        &self,
        params: Self::QueryParams,
        source: SourceBuilderBlocking<'_>,
    ) -> Result<(), Self::Error>;
}

// ============================================================================
// Route handlers
// ============================================================================

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn manifest_handler<P: UpstreamServer>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    // Auth
    if let Err(e) = provider.auth(extract_bearer_token(&headers), &params).await {
        return e.into_response();
    }

    // Build source in manifest mode
    let mut manifest_opts = ManifestOpts::default();
    let mut topics = Vec::new();
    let mut schemas = Vec::new();

    let source = SourceBuilder {
        mode: SourceMode::Manifest {
            manifest_opts: &mut manifest_opts,
            topics: &mut topics,
            schemas: &mut schemas,
            next_schema_id: NonZeroU16::MIN,
        },
    };

    if let Err(error) = provider.build_source(params, source).await {
        warn!(%error, "build_source error");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Validate opts
    if manifest_opts.id.is_empty() {
        warn!("manifest opts id is empty");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Build manifest
    let manifest = manifest::Manifest {
        name: Some(manifest_opts.name),
        sources: vec![UpstreamSource::Streamed(StreamedSource {
            url: DATA_ROUTE.parse().expect("valid url"),
            id: Some(manifest_opts.id),
            topics,
            schemas,
            start_time: manifest_opts.start_time,
            end_time: manifest_opts.end_time,
        })],
    };

    Json(manifest).into_response()
}

async fn data_handler<P: UpstreamServer>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    // Auth
    if let Err(e) = provider.auth(extract_bearer_token(&headers), &params).await {
        return e.into_response();
    }

    // Create the MCAP stream
    let (handle, stream) = foxglove::stream::create_mcap_stream();

    // Build source in stream mode
    let source = SourceBuilder {
        mode: SourceMode::Stream { handle },
    };

    if let Err(error) = provider.build_source(params, source).await {
        warn!(%error, "build_source error");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Body::from_stream(stream.map(Ok::<_, std::convert::Infallible>)).into_response()
}

/// Route for the manifest endpoint.
pub const MANIFEST_ROUTE: &str = "/v1/manifest";

/// Route for the data endpoint.
pub const DATA_ROUTE: &str = "/v1/data";

/// Serve both manifest and data endpoints (async).
///
/// # Example
///
/// ```rust,ignore
/// #[tokio::main]
/// async fn main() {
///     serve(MyServer::new(), "127.0.0.1:8080".parse().unwrap()).await.unwrap();
/// }
/// ```
pub async fn serve(provider: impl UpstreamServer, addr: SocketAddr) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler::<_>))
        .route(DATA_ROUTE, get(data_handler::<_>))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

/// Serve both manifest and data endpoints (blocking).
///
/// Uses an internal tokio runtime. No async code required in your implementation.
///
/// # Example
///
/// ```rust,ignore
/// fn main() {
///     serve_blocking(MyServer::new(), "127.0.0.1:8080".parse().unwrap()).unwrap();
/// }
/// ```
pub fn serve_blocking(
    provider: impl UpstreamServerBlocking,
    addr: SocketAddr,
) -> std::io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(serve_blocking_inner(provider, addr))
}

async fn serve_blocking_inner(
    provider: impl UpstreamServerBlocking,
    addr: SocketAddr,
) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler_blocking))
        .route(DATA_ROUTE, get(data_handler_blocking))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

async fn manifest_handler_blocking<P: UpstreamServerBlocking>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    let token = extract_bearer_token(&headers).map(|s| s.to_owned());

    let result = tokio::task::spawn_blocking(move || {
        // Auth
        if let Err(e) = provider.auth(token.as_deref(), &params) {
            return Err(e.into_response());
        }

        // Build source in manifest mode
        let mut manifest_opts = ManifestOpts::default();
        let mut topics = Vec::new();
        let mut schemas = Vec::new();

        let source = SourceBuilderBlocking {
            mode: SourceMode::Manifest {
                manifest_opts: &mut manifest_opts,
                topics: &mut topics,
                schemas: &mut schemas,
                next_schema_id: NonZeroU16::MIN,
            },
        };

        if let Err(error) = provider.build_source(params, source) {
            warn!(%error, "build_source error");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }

        // Validate opts
        if manifest_opts.id.is_empty() {
            warn!("manifest opts id is empty");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }

        // Build manifest
        let manifest = manifest::Manifest {
            name: Some(manifest_opts.name),
            sources: vec![UpstreamSource::Streamed(StreamedSource {
                url: DATA_ROUTE.parse().expect("valid url"),
                id: Some(manifest_opts.id),
                topics,
                schemas,
                start_time: manifest_opts.start_time,
                end_time: manifest_opts.end_time,
            })],
        };

        Ok(manifest)
    })
    .await;

    match result {
        Ok(Ok(manifest)) => Json(manifest).into_response(),
        Ok(Err(response)) => response,
        Err(e) => {
            warn!(%e, "spawn_blocking panicked");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn data_handler_blocking<P: UpstreamServerBlocking>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    let token = extract_bearer_token(&headers).map(|s| s.to_owned());

    // Create stream before spawning blocking task
    let (handle, stream) = foxglove::stream::create_mcap_stream();

    let build_result = tokio::task::spawn_blocking(move || {
        // Auth
        if let Err(e) = provider.auth(token.as_deref(), &params) {
            return Err(e.into_response());
        }

        // Build source in stream mode
        let source = SourceBuilderBlocking {
            mode: SourceMode::Stream { handle },
        };

        if let Err(error) = provider.build_source(params, source) {
            warn!(%error, "build_source error");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }

        Ok(())
    })
    .await;

    match build_result {
        Ok(Ok(())) => {
            Body::from_stream(stream.map(Ok::<_, std::convert::Infallible>)).into_response()
        }
        Ok(Err(response)) => response,
        Err(e) => {
            warn!(%e, "build_source spawn_blocking panicked");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
