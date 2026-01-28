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
use tracing::error;

use crate::{
    extract_bearer_token, AuthError, BoxError, ChannelRegistry, ManifestBuilder, Metadata,
    DATA_ROUTE, MANIFEST_ROUTE,
};
pub use foxglove::stream::McapStreamHandle as StreamHandle;

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
/// #[derive(serde::Deserialize, Hash)]
/// struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// #[derive(foxglove::Encode)]
/// struct MyMessage { value: i32 }
/// struct MyServer;
/// struct FlightInfo { name: String }
/// impl MyServer {
///     async fn get_flight(&self, _: &str) -> Result<FlightInfo, std::io::Error> {
///         Ok(FlightInfo { name: "test".into() })
///     }
/// }
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
///         reg: &mut impl ChannelRegistry,
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
    /// Parameters that identify the data to load.
    ///
    /// In the Foxglove app, remote data sources are opened using a URL like:
    ///
    /// ```text
    /// https://app.foxglove.dev/view?ds=remote-data-loader&ds.dataLoaderUrl=https%3A%2F%2Fremote-data-loader.example.com&ds.flightId=ABC&ds.startTime=2024-01-01T00:00:00Z
    /// ```
    ///
    /// The `ds.*` parameters (except `ds.dataLoaderUrl`) are forwarded to your upstream server with
    /// the `ds.` prefix stripped:
    ///
    /// ```text
    /// GET /v1/manifest?flightId=ABC&startTime=2024-01-01T00:00:00Z
    /// GET /v1/data?flightId=ABC&startTime=2024-01-01T00:00:00Z
    /// ```
    ///
    /// These parameters are deserialized into an instance of
    /// [`QueryParams`](`UpstreamServer::QueryParams`) using [`serde::Deserialize`].
    ///
    /// If you want to pass these parameters to [`generate_source_id`](`crate::generate_source_id`)
    /// to construct unique deterministic cache keys, you should also derive [`Hash`].
    ///
    /// # Example
    ///
    /// ```
    /// # use chrono::{DateTime, Utc};
    /// # use serde::Deserialize;
    /// # use std::hash::Hash;
    /// #[derive(Deserialize, Hash)]
    /// #[serde(rename_all = "camelCase")]
    /// struct MyParams {
    ///     flight_id: String,
    ///     start_time: DateTime<Utc>,
    ///     end_time: DateTime<Utc>,
    /// }
    /// ```
    type QueryParams: DeserializeOwned + Send;

    /// Context type that holds channels and any shared state between methods.
    ///
    /// Create this in [`initialize`](UpstreamServer::initialize) and receive it in
    /// [`metadata`](UpstreamServer::metadata) or [`stream`](UpstreamServer::stream).
    type Context;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok(())` to allow access, or an `Err` to deny access.
    /// See [`AuthError`] for possible error values.
    fn auth(
        &self,
        bearer_token: Option<&str>,
        params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;

    /// Initialize the data source by declaring channels and preparing context.
    ///
    /// Called for both manifest and data requests. Use the [`ChannelRegistry`] to declare channels,
    /// and return a context containing the channels and any other state needed by
    /// [`metadata`](UpstreamServer::metadata) or [`stream`](UpstreamServer::stream).
    fn initialize(
        &self,
        params: Self::QueryParams,
        registry: &mut impl ChannelRegistry,
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
    /// Called only for data requests (`/v1/data`). Use channels from the context to log messages,
    /// then call [`StreamHandle::close`] when done.
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
    let mut manifest_builder = ManifestBuilder::new();
    let ctx = match provider.initialize(params, &mut manifest_builder).await {
        Ok(ctx) => ctx,
        Err(error) => {
            error!(%error, "error during initialization");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Get metadata
    let metadata = match provider.metadata(ctx).await {
        Ok(m) => m,
        Err(error) => {
            error!(%error, "error getting metadata");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Build manifest
    let manifest = manifest_builder.build(metadata);
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

    // Run stream in its own task, since the response won't start until we return from this
    // function.
    let (mut stream_handle, mcap_stream) = foxglove::stream::create_mcap_stream();
    let mcap_stream_task = tokio::spawn(async move {
        let ctx = provider.initialize(params, &mut stream_handle).await?;
        provider.stream(ctx, stream_handle).await
    });

    // Catch any errors during streaming.
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
/// # use chrono::{DateTime, Utc};
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
///     async fn initialize(&self, _: (), _: &mut impl ChannelRegistry) -> Result<(), BoxError> {
///         Ok(())
///     }
///
///     async fn metadata(&self, _: ()) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             id: "my-source".into(),
///             name: "My Source".into(),
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
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
