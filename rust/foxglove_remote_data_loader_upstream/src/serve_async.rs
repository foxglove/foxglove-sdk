//! Async server implementation.

use std::{future::Future, net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, RawQuery, State},
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

/// Async upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints asynchronously.
///
/// Use [`serve`] to start the server.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{UpstreamServer, ChannelRegistry, Channel, AuthError, Metadata, BoxError};
/// # use chrono::{DateTime, Utc};
/// #[derive(serde::Deserialize)]
/// struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// impl MyParams {
///     fn slug(&self) -> String {
///         format!("{}-{}-{}", self.flight_id, self.start_time, self.end_time)
///     }
/// }
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
///     params: MyParams,
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
///             params,
///             channel: reg.channel("/topic"),
///             flight_info,
///         })
///     }
///
///     async fn metadata(&self, ctx: MyContext) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             // Stable identifier for caching - include all params that affect output
///             id: format!("example-v1-{}", ctx.params.slug()),
///             name: ctx.flight_info.name,
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
///     }
///
///     async fn stream(&self, ctx: MyContext) -> Result<(), BoxError> {
///         ctx.channel.log(&MyMessage { value: 42 }, 123467890u64);
///         Ok(())
///     }
/// }
/// ```
pub trait UpstreamServer: Send + Sync + 'static {
    #[doc = include_str!("docs/query_params.md")]
    ///
    /// # Example
    ///
    /// ```
    /// # use chrono::{DateTime, Utc};
    /// # use serde::Deserialize;
    /// #[derive(Deserialize)]
    /// #[serde(rename_all = "camelCase")]
    /// struct MyParams {
    ///     flight_id: String,
    ///     start_time: DateTime<Utc>,
    ///     end_time: DateTime<Utc>,
    /// }
    /// ```
    type QueryParams: DeserializeOwned + Send;

    /// Context type passed from [`initialize`](UpstreamServer::initialize) to
    /// [`metadata`](UpstreamServer::metadata) and [`stream`](UpstreamServer::stream).
    ///
    /// This can hold anything, but is typically used to store request-specific state (e.g.
    /// [`Channel`]s and query parameters)
    type Context: Send;

    #[doc = include_str!("docs/auth.md")]
    fn auth(
        &self,
        bearer_token: Option<&str>,
        params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;

    #[doc = include_str!("docs/initialize.md")]
    fn initialize(
        &self,
        params: Self::QueryParams,
        registry: &mut impl ChannelRegistry,
    ) -> impl Future<Output = Result<Self::Context, BoxError>> + Send;

    #[doc = include_str!("docs/metadata.md")]
    fn metadata(
        &self,
        ctx: Self::Context,
    ) -> impl Future<Output = Result<Metadata, BoxError>> + Send;

    #[doc = include_str!("docs/stream.md")]
    fn stream(
        &self,
        ctx: Self::Context,
    ) -> impl Future<Output = Result<(), BoxError>> + Send;
}

async fn manifest_handler<P: UpstreamServer>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    RawQuery(query_string): RawQuery,
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
    let manifest = manifest_builder.build(metadata, query_string.as_deref());
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
    let (mut stream_handle, mcap_stream) = crate::stream::create_stream();
    let ctx = match provider.initialize(params, &mut stream_handle).await {
        Ok(ctx) => ctx,
        Err(error) => {
            error!(%error, "error during initialization");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let mcap_stream_task = tokio::spawn(async move {
        let result = provider.stream(ctx).await;
        let close_result = stream_handle.close().await;
        result.and(close_result)
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
/// # use foxglove_remote_data_loader_upstream::{serve, UpstreamServer, ChannelRegistry, AuthError, Metadata, BoxError};
/// # use chrono::{DateTime, Utc};
///
/// struct MyServer;
///
/// impl UpstreamServer for MyServer {
///     type QueryParams = ();
///     type Context = ();
///
///     async fn auth(&self, _bearer_token: Option<&str>, _params: &()) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     async fn initialize(&self, _params: (), _registry: &mut impl ChannelRegistry) -> Result<(), BoxError> {
///         Ok(())
///     }
///
///     async fn metadata(&self, _ctx: ()) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             id: "my-source".into(),
///             name: "My Source".into(),
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
///     }
///
///     async fn stream(&self, _ctx: ()) -> Result<(), BoxError> {
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
