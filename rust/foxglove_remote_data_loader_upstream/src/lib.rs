//! This crate provides utilities for quickly building a remote data loader upstream server. It
//! handles server setup and routing, and provides a framework for implementing authentication,
//! manifest generation, and data streaming.
//!
//! # Quick Start
//!
//! Define a server type (e.g. `struct MyServer;`) and implement these three traits:
//!
//! - [`Authenticator`] - Authenticate the identity of the requestor from a bearer token.
//! - [`ManifestProvider`] - Produce [manifests](manifest::Manifest) describing a collection of
//!   data sources matching a query.
//! - [`DataProvider`] - Stream MCAP data from an individual source listed in a manifest.
//!
//! Then, call [`serve`] to start the server.
//!
//! See `examples/demo.rs` in the crate directory for a complete example.
//!
//! See also [`serve_manifest`] and [`serve_data`] for serving only manifests or only data,
//! respectively.
//!
//! # Endpoints
//!
//! The SDK serves these fixed routes:
//!
//! | Route | Handler |
//! |-------|---------|
//! | `GET /v1/manifest` | Calls [`ManifestProvider::get_manifest`] |
//! | `GET /v1/data` | Calls [`DataProvider::stream_data`] |
//!
//! # Auth
//!
//! Every request first calls [`Authenticator::authenticate`] with the bearer token (if present).
//! The returned identity is passed to the [`ManifestProvider::get_manifest`] and
//! [`DataProvider::authorize_data`] methods.

pub mod manifest;

use std::{error::Error as StdError, future::Future, net::SocketAddr, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::{Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures::Stream;
use serde::de::DeserializeOwned;
use tracing::warn;

/// Error type returned from [`Authenticator::authenticate`] when authentication fails.
#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    /// A bearer token was required, but was either missing or invalid.
    #[error("unauthenticated")]
    Unauthenticated,
    /// An unexpected error occurred while attempting to authenticate the request.
    #[error(transparent)]
    Other(Box<dyn StdError + Send>),
}

impl AuthenticationError {
    /// Creates a new [`AuthenticationError::Other`] from an arbitrary error payload.
    pub fn other(error: impl Into<Box<dyn StdError + Send>>) -> Self {
        Self::Other(error.into())
    }
}

impl IntoResponse for AuthenticationError {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthenticated => StatusCode::UNAUTHORIZED.into_response(),
            Self::Other(error) => {
                warn!(error, "error during authentication");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Error type returned from [`DataProvider::authorize_data`] when authorization fails.
#[derive(thiserror::Error, Debug)]
pub enum AuthorizationError {
    /// The authenticated identity is not authorized to access the requested data.
    #[error("forbidden")]
    Forbidden,
    /// An unexpected error occurred while attempting to authorize the request.
    #[error(transparent)]
    Other(Box<dyn StdError + Send>),
}

impl AuthorizationError {
    /// Creates a new [`AuthorizationError::Other`] from an arbitrary error payload.
    pub fn other(error: impl Into<Box<dyn StdError + Send>>) -> Self {
        Self::Other(error.into())
    }
}

impl IntoResponse for AuthorizationError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN.into_response(),
            Self::Other(error) => {
                warn!(error, "error during authorization");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Authenticates incoming requests.
///
/// Implement this trait to validate bearer tokens and extract user identity.
/// The identity is passed to [`ManifestProvider::get_manifest`] and [`DataProvider::authorize_data`].
///
/// # No Authentication
///
/// If you don't want authentication, you can use `()` for the identity type:
///
/// ```rust,ignore
/// impl Authenticator for MyServer {
///     type Identity = ();
///     async fn authenticate(&self, _: Option<&str>) -> Result<(), AuthenticationError> {
///         Ok(())
///     }
/// }
/// ```
pub trait Authenticator: Send + Sync + 'static {
    /// The identity type returned from [`authenticate`](Authenticator::authenticate).
    ///
    /// This should contain any information needed to authorize requests.
    type Identity: Send;

    /// Authenticate the request using the provided bearer token.
    ///
    /// Return [`AuthenticationError::Unauthenticated`] if the token is missing or invalid.
    fn authenticate(
        &self,
        bearer_token: Option<&str>,
    ) -> impl Future<Output = Result<Self::Identity, AuthenticationError>> + Send;
}

/// Provides manifests describing available data sources.
///
/// The manifest tells Foxglove what data is available and where to fetch it.
/// Each source in the manifest includes a URL pointing to the data endpoint.
///
/// # Query Parameters
///
/// Define a struct with `#[derive(Deserialize)]` for your query parameters.
/// Use `#[serde(rename_all = "camelCase")]` to match URL conventions.
pub trait ManifestProvider: Authenticator {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send;

    /// Error type returned from [`get_manifest`](ManifestProvider::get_manifest).
    type Error: StdError;

    /// Generate a [`Manifest`](manifest::Manifest) containing the MCAP URLs matching the given
    /// query.
    ///
    /// Unlike [`DataProvider::stream_data`], this method does not need to be idempotent.
    ///
    /// ## Security
    ///
    /// The returned manifest **must** only include MCAP URLs that are accessible to the given
    /// identity. Otherwise, unauthorized requestors may be able to access data they should not be
    /// able to access. In particular, downstream caches are not required to recheck authorization
    /// with upstream before serving data.
    fn get_manifest(
        &self,
        identity: Self::Identity,
        query_params: Self::QueryParams,
    ) -> impl Future<Output = Result<manifest::Manifest, Self::Error>> + Send;
}

/// Streams MCAP data for a specific source.
///
/// # Authorization
///
/// [`authorize_data`](DataProvider::authorize_data) is called before streaming to check
/// if the authenticated identity can access the requested resource. Return
/// [`AuthorizationError::Forbidden`] to deny access.
///
/// # Error Types
///
/// - [`StreamInitError`](DataProvider::StreamInitError): Returned if the stream cannot be created
///   (e.g., source not found)
/// - [`StreamDataError`](DataProvider::StreamDataError): Emitted within the stream if an error
///   occurs during streaming
pub trait DataProvider: Authenticator {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send;

    /// Error type returned from [`stream_data`](DataProvider::stream_data) if the stream could not be created.
    type StreamInitError: StdError;

    /// Type of errors within the stream returned from [`stream_data`](DataProvider::stream_data).
    type StreamDataError: StdError + Send + Sync;

    /// Check if the authenticated identity can access the requested data.
    fn authorize_data(
        &self,
        identity: Self::Identity,
        query_params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthorizationError>> + Send;

    /// Stream MCAP data for the requested source.
    ///
    /// This is only called after [`authorize_data`](DataProvider::authorize_data) succeeds.
    ///
    /// ## Idempotency
    ///
    /// Whenever this method succeeds, it **must** always return the same data for the same query
    /// parameters, independent of the identity of the requestor. Otherwise, different clients may
    /// see inconsistent data due to caching downstream.
    fn stream_data(
        &self,
        query_params: Self::QueryParams,
    ) -> impl Future<
        Output = Result<
            impl Stream<Item = Result<Bytes, Self::StreamDataError>> + Send + 'static,
            Self::StreamInitError,
        >,
    > + Send;
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn manifest_handler<P: ManifestProvider>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    let identity = match provider.authenticate(extract_bearer_token(&headers)).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let manifest = match provider.get_manifest(identity, params).await {
        Ok(manifest) => manifest,
        Err(error) => {
            warn!(%error, "manifest error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    Json(manifest).into_response()
}

async fn data_handler<P: DataProvider>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    let token = extract_bearer_token(&headers);
    let identity = match provider.authenticate(token).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = provider.authorize_data(identity, &params).await {
        return e.into_response();
    }
    let stream = match provider.stream_data(params).await {
        Ok(stream) => stream,
        Err(error) => {
            warn!(%error, "data stream error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    Body::from_stream(stream).into_response()
}

/// Route for the manifest endpoint.
pub const MANIFEST_ROUTE: &str = "/v1/manifest";

/// Route for the data endpoint.
pub const DATA_ROUTE: &str = "/v1/data";

/// Serve a manifest-only server.
///
/// Use this when you only need to serve manifests (e.g., the data is hosted elsewhere).
/// The manifest endpoint is served at [`MANIFEST_ROUTE`].
pub async fn serve_manifest(
    provider: impl ManifestProvider,
    addr: SocketAddr,
) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

/// Serve a data-only server.
///
/// Use this when manifests are served by a different service.
/// The data endpoint is served at [`DATA_ROUTE`].
pub async fn serve_data(provider: impl DataProvider, addr: SocketAddr) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(DATA_ROUTE, get(data_handler))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

/// Serve both manifest and data endpoints.
///
/// Endpoints:
/// - [`MANIFEST_ROUTE`] (`/v1/manifest`) - serves manifests via the [`ManifestProvider`] impl
/// - [`DATA_ROUTE`] (`/v1/data`) - streams MCAP data via the [`DataProvider`] impl
pub async fn serve(
    provider: impl ManifestProvider + DataProvider,
    addr: SocketAddr,
) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler))
        .route(DATA_ROUTE, get(data_handler))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
