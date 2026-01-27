//! Async server implementation.

use std::{future::Future, net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures::{FutureExt, StreamExt};
use serde::de::DeserializeOwned;
use tracing::warn;

use crate::{
    extract_bearer_token, AuthError, BoxError, ChannelRegistry, Metadata, DATA_ROUTE,
    MANIFEST_ROUTE,
};
use foxglove::{stream::McapStreamHandle, FoxgloveError};

/// Handle for streaming MCAP data with [`UpstreamServer`].
///
/// Returned by the framework when calling [`UpstreamServer::stream`].
pub struct StreamHandle {
    inner: McapStreamHandle,
}

impl StreamHandle {
    /// Flush the MCAP writer's buffer.
    pub async fn flush(&mut self) -> Result<(), FoxgloveError> {
        self.inner.flush().await
    }

    /// Stop logging events and flush any buffered data.
    ///
    /// This method will return an error if the MCAP writer fails to finish.
    pub async fn close(self) -> Result<(), FoxgloveError> {
        self.inner.close().await
    }

    /// Get the current size of the buffer.
    ///
    /// This can be used in conjunction with [`Self::flush`] to ensure the buffer does
    /// not grow unbounded.
    pub fn buffer_size(&mut self) -> usize {
        self.inner.buffer_size()
    }
}

/// Async upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints asynchronously.
///
/// Use [`serve`] to start the server.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{UpstreamServer, ChannelRegistry, AuthError, Metadata, StreamHandle, BoxError, generate_source_id};
/// # use foxglove::Channel;
/// # use chrono::{DateTime, Utc};
/// # #[derive(serde::Deserialize, Hash)]
/// # struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// # #[derive(foxglove::Encode)]
/// # struct MyMessage { value: i32 }
/// # struct MyServer;
/// # struct FlightInfo { name: String }
/// # impl MyServer { async fn get_flight(&self, _: &str) -> Result<FlightInfo, std::io::Error> { Ok(FlightInfo { name: "test".into() }) } }
///
/// struct MyContext {
///     flight_id: String,
///     channel: Channel<MyMessage>,
///     flight_info: FlightInfo,
/// }
///
/// impl UpstreamServer for MyServer {
///     type QueryParams = MyParams;
///     type Context = MyContext;
///
///     async fn auth(&self, _token: Option<&str>, _params: &MyParams) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     async fn initialize(
///         &self,
///         params: MyParams,
///         reg: &mut ChannelRegistry,
///     ) -> Result<MyContext, BoxError> {
///         let flight_info = self.get_flight(&params.flight_id).await?;
///         Ok(MyContext {
///             flight_id: params.flight_id,
///             channel: reg.channel("/topic"),
///             flight_info,
///         })
///     }
///
///     async fn metadata(&self, ctx: MyContext) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             id: generate_source_id("example", 1, &ctx.flight_id),
///             name: ctx.flight_info.name,
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
///     }
///
///     async fn stream(
///         &self,
///         ctx: MyContext,
///         handle: StreamHandle,
///     ) -> Result<(), BoxError> {
///         ctx.channel.log_with_time(&MyMessage { value: 42 }, 123467890u64);
///         handle.close().await?;
///         Ok(())
///     }
/// }
/// ```
pub trait UpstreamServer: Send + Sync + 'static {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send;

    /// Context type that holds channels and any shared state between methods.
    ///
    /// Create this in [`initialize`](Self::initialize) and receive it in
    /// [`metadata`](Self::metadata) or [`stream`](Self::stream).
    type Context: Send;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok(())` to allow access. Return `Err(AuthError::Unauthenticated)` for
    /// missing/invalid credentials (401), `Err(AuthError::Forbidden)` for valid credentials
    /// but denied access (403), or use `?` to convert any error to a 500 response.
    fn auth(
        &self,
        bearer_token: Option<&str>,
        params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;

    /// Initialize the data source by declaring channels and preparing context.
    ///
    /// Called for both manifest and data requests. Use the [`ChannelRegistry`] to declare
    /// channels, and return a context containing the channels and any other state needed
    /// by [`metadata`](Self::metadata) or [`stream`](Self::stream).
    fn initialize(
        &self,
        params: Self::QueryParams,
        registry: &mut ChannelRegistry,
    ) -> impl Future<Output = Result<Self::Context, BoxError>> + Send;

    /// Return metadata describing the data source.
    ///
    /// Called only for manifest requests (`/v1/manifest`).
    fn metadata(
        &self,
        ctx: Self::Context,
    ) -> impl Future<Output = Result<Metadata, BoxError>> + Send;

    /// Stream MCAP data.
    ///
    /// Called only for data requests (`/v1/data`). Use channels from the context
    /// to log messages, then call [`StreamHandle::close`] when done.
    fn stream(
        &self,
        ctx: Self::Context,
        handle: StreamHandle,
    ) -> impl Future<Output = Result<(), BoxError>> + Send;
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

    // Initialize
    let mut registry = ChannelRegistry::new_for_manifest();
    let ctx = match provider.initialize(params, &mut registry).await {
        Ok(ctx) => ctx,
        Err(error) => {
            warn!(%error, "error during initialization");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Get metadata
    let metadata = match provider.metadata(ctx).await {
        Ok(m) => m,
        Err(error) => {
            warn!(%error, "error getting metadata");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Build manifest
    let manifest = registry.into_manifest_builder().build(metadata);
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

    // Build output stream
    let (mcap_handle, mcap_stream) = foxglove::stream::create_mcap_stream();

    let mcap_stream_task = tokio::spawn(async move {
        // Initialize with the stream handle
        let mut registry = ChannelRegistry::new_for_stream(mcap_handle);
        let ctx = provider.initialize(params, &mut registry).await?;

        // Extract the handle back from the registry
        let (_, handle) = registry.into_parts();
        let handle = handle.expect("stream mode registry should have handle");

        // Stream data
        provider.stream(ctx, StreamHandle { inner: handle }).await
    });

    // Catch any errors during streaming
    let error_stream = mcap_stream_task
        .into_stream()
        .filter_map(|result| async move {
            match result.expect("panicked while streaming data") {
                Ok(()) => None,
                Err(e) => Some(Err(e)),
            }
        });

    let combined = mcap_stream.map(Ok::<_, BoxError>).chain(error_stream);
    Body::from_stream(combined).into_response()
}

/// Serve both manifest and data endpoints (async).
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{serve, UpstreamServer, ChannelRegistry, AuthError, Metadata, StreamHandle, BoxError};
///
/// struct MyServer;
///
/// impl UpstreamServer for MyServer {
///     type QueryParams = ();
///     type Context = ();
///
///     async fn auth(&self, _: Option<&str>, _: &()) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     async fn initialize(&self, _: (), _: &mut ChannelRegistry) -> Result<(), BoxError> {
///         Ok(())
///     }
///
///     async fn metadata(&self, _: ()) -> Result<Metadata, BoxError> {
///         Ok(Metadata::default())
///     }
///
///     async fn stream(&self, _: (), handle: StreamHandle) -> Result<(), BoxError> {
///         handle.close().await?;
///         Ok(())
///     }
/// }
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     serve(MyServer, "0.0.0.0:8080".parse().unwrap()).await
/// }
/// ```
pub async fn serve(provider: impl UpstreamServer, bind_addr: SocketAddr) -> std::io::Result<()> {
    let provider = Arc::new(provider);
    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler::<_>))
        .route(DATA_ROUTE, get(data_handler::<_>))
        .with_state(provider);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await
}
