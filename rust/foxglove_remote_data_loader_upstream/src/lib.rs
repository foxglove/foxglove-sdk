//! Convenience SDK for implementing a remote data loader upstream server.
//!
//! Goals:
//! - Customers never write axum handlers
//! - First-class: manifest-only, data-only, both
//! - Fixed SDK routes: GET /v1/manifest, GET /v1/data
//! - Data is an arbitrary streaming MCAP byte stream (dynamic, not a static file)
//! - Optional AccessControl enforced everywhere (no footguns)

pub mod manifest;

use std::{
    convert::Infallible, error::Error as StdError, future::Future, net::SocketAddr, sync::Arc,
};

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

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error(transparent)]
    Other(Box<dyn StdError + Send + 'static>),
}

impl AuthenticationError {
    pub fn other(error: impl StdError + Send + 'static) -> Self {
        Self::Other(Box::new(error))
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

#[derive(thiserror::Error, Debug)]
pub enum AuthorizationError {
    #[error("forbidden")]
    Forbidden,
    #[error(transparent)]
    Other(Box<dyn StdError + Send + 'static>),
}

impl AuthorizationError {
    pub fn other(error: impl StdError + Send + 'static) -> Self {
        Self::Other(Box::new(error))
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

pub trait Authenticator {
    type Identity: Send;

    fn authenticate(
        &self,
        bearer_token: Option<&str>,
    ) -> impl Future<Output = Result<Self::Identity, AuthenticationError>> + Send;
}

pub trait ManifestProvider: Authenticator {
    type QueryParams: DeserializeOwned + Send;

    fn get_manifest(
        &self,
        identity: Self::Identity,
        query_params: Self::QueryParams,
    ) -> impl Future<Output = Result<manifest::Manifest, impl StdError + Send + 'static>> + Send;
}

pub trait DataProvider: Authenticator {
    type QueryParams: DeserializeOwned + Send + Sync;

    fn authorize_data(
        &self,
        identity: Self::Identity,
        query_params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthorizationError>> + Send;
    fn stream_data(
        &self,
        query_params: Self::QueryParams,
    ) -> impl Future<
        Output = Result<
            impl Stream<Item = Result<Bytes, Infallible>> + Send + 'static,
            impl StdError + Send + 'static,
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

/// Serve a manifest-only server at /v1/manifest
pub async fn serve_manifest<P>(provider: P, addr: SocketAddr) -> std::io::Result<()>
where
    P: ManifestProvider + Send + Sync + 'static,
{
    let provider = Arc::new(provider);
    let app = Router::new()
        .route("/v1/manifest", get(manifest_handler::<P>))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve a data-only server at /v1/data
pub async fn serve_data<P>(provider: P, addr: SocketAddr) -> std::io::Result<()>
where
    P: DataProvider + Send + Sync + 'static,
{
    let provider = Arc::new(provider);
    let app = Router::new()
        .route("/v1/data", get(data_handler::<P>))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve both manifest and data endpoints
pub async fn serve<P>(provider: P, addr: SocketAddr) -> std::io::Result<()>
where
    P: ManifestProvider + DataProvider + Send + Sync + 'static,
{
    let provider = Arc::new(provider);
    let app = Router::new()
        .route("/v1/manifest", get(manifest_handler::<P>))
        .route("/v1/data", get(data_handler::<P>))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
