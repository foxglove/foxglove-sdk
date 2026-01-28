//! Blocking server implementation.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures::{FutureExt, StreamExt};
use serde::de::DeserializeOwned;
use tokio::runtime::Handle;
use tracing::error;

use crate::{
    extract_bearer_token, AuthError, BoxError, ChannelRegistry, ManifestBuilder, Metadata,
    DATA_ROUTE, MANIFEST_ROUTE,
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
    ///
    /// This method will block until the buffer is flushed.
    pub fn flush(&mut self) -> Result<(), FoxgloveError> {
        Handle::current().block_on(self.inner.flush())
    }

    /// Stop logging events and flush any buffered data.
    ///
    /// Like [`Self::flush`], this method will block until the buffer is flushed.
    ///
    /// This method will return an error if the MCAP writer fails to finish.
    pub fn close(self) -> Result<(), FoxgloveError> {
        Handle::current().block_on(self.inner.close())
    }

    /// Get the current size of the buffer.
    ///
    /// This can be used in conjunction with [`Self::flush`] to ensure the buffer does
    /// not grow unbounded.
    pub fn buffer_size(&mut self) -> usize {
        self.inner.buffer_size()
    }
}

/// Blocking upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints without writing `async` code.
///
/// Use [`serve`] to start the server.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{blocking, ChannelRegistry, AuthError, Metadata, BoxError};
/// # use foxglove::Channel;
/// # use chrono::{DateTime, Utc};
/// # #[derive(serde::Deserialize)]
/// # struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// # impl MyParams {
/// #     fn slug(&self) -> String {
/// #         format!("{}-{}-{}", self.flight_id, self.start_time, self.end_time)
/// #     }
/// # }
/// # #[derive(foxglove::Encode)]
/// # struct MyMessage { value: i32 }
/// # struct MyServer;
/// # struct FlightInfo { name: String }
/// # impl MyServer { fn get_flight(&self, _: &str) -> Result<FlightInfo, std::io::Error> { Ok(FlightInfo { name: "test".into() }) } }
///
/// struct MyContext {
///     params: MyParams,
///     channel: Channel<MyMessage>,
///     flight_info: FlightInfo,
/// }
///
/// impl blocking::UpstreamServer for MyServer {
///     type QueryParams = MyParams;
///     type Context = MyContext;
///
///     fn auth(&self, _token: Option<&str>, _params: &MyParams) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     fn initialize(
///         &self,
///         params: MyParams,
///         reg: &mut impl ChannelRegistry,
///     ) -> Result<MyContext, BoxError> {
///         let flight_info = self.get_flight(&params.flight_id)?;
///         Ok(MyContext {
///             params,
///             channel: reg.channel("/topic"),
///             flight_info,
///         })
///     }
///
///     fn metadata(&self, ctx: MyContext) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             // Stable identifier for caching - include all params that affect output
///             id: format!("example-v1-{}", ctx.params.slug()),
///             name: ctx.flight_info.name,
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
///     }
///
///     fn stream(
///         &self,
///         ctx: MyContext,
///         handle: blocking::StreamHandle,
///     ) -> Result<(), BoxError> {
///         ctx.channel.log_with_time(&MyMessage { value: 42 }, 123467890u64);
///         handle.close()?;
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
    /// # Example
    ///
    /// ```rust
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

    /// Context type that holds channels and any shared state between methods.
    ///
    /// Create this in [`initialize`](Self::initialize) and receive it in
    /// [`metadata`](Self::metadata) or [`stream`](Self::stream).
    type Context;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok(())` to allow access. Return `Err(AuthError::Unauthenticated)` for
    /// missing/invalid credentials (401), `Err(AuthError::Forbidden)` for valid credentials
    /// but denied access (403), or use `?` to convert any error to a 500 response.
    fn auth(&self, bearer_token: Option<&str>, params: &Self::QueryParams)
        -> Result<(), AuthError>;

    /// Initialize the data source by declaring channels and preparing context.
    ///
    /// Called for both manifest and data requests. Use the [`ChannelRegistry`] to declare
    /// channels, and return a context containing the channels and any other state needed
    /// by [`metadata`](Self::metadata) or [`stream`](Self::stream).
    fn initialize(
        &self,
        params: Self::QueryParams,
        registry: &mut impl ChannelRegistry,
    ) -> Result<Self::Context, BoxError>;

    /// Return metadata describing the data source.
    ///
    /// Called only for manifest requests (`/v1/manifest`).
    fn metadata(&self, ctx: Self::Context) -> Result<Metadata, BoxError>;

    /// Stream MCAP data.
    ///
    /// Called only for data requests (`/v1/data`). Use channels from the context
    /// to log messages, then call [`StreamHandle::close`] when done.
    fn stream(&self, ctx: Self::Context, handle: StreamHandle) -> Result<(), BoxError>;
}

async fn manifest_handler<P: UpstreamServer>(
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

        // Initialize
        let mut manifest_builder = ManifestBuilder::new();
        let ctx = match provider.initialize(params, &mut manifest_builder) {
            Ok(ctx) => ctx,
            Err(error) => {
                error!(%error, "error during initialization");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        };

        // Get metadata
        let metadata = match provider.metadata(ctx) {
            Ok(m) => m,
            Err(error) => {
                error!(%error, "error getting metadata");
                return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        };

        // Build manifest
        Ok(manifest_builder.build(metadata))
    })
    .await;

    match result.expect("panicked while building manifest") {
        Ok(manifest) => Json(manifest).into_response(),
        Err(response) => response,
    }
}

async fn data_handler<P: UpstreamServer>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    // Auth (in blocking task)
    let auth_result = {
        let provider = Arc::clone(&provider);
        let token = extract_bearer_token(&headers).map(|s| s.to_owned());
        tokio::task::spawn_blocking(move || {
            provider.auth(token.as_deref(), &params).map(|()| params)
        })
        .await
        .expect("panicked during auth")
    };
    let params = match auth_result {
        Ok(params) => params,
        Err(e) => return e.into_response(),
    };

    // Build MCAP data stream
    let (mut stream_handle, mcap_stream) = foxglove::stream::create_mcap_stream();

    let mcap_stream_task = tokio::task::spawn_blocking(move || {
        // Initialize with the stream handle
        let ctx = provider.initialize(params, &mut stream_handle)?;

        // Stream data
        provider.stream(
            ctx,
            StreamHandle {
                inner: stream_handle,
            },
        )
    });

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

/// Serve both manifest and data endpoints using [`UpstreamServer`].
///
/// Use this if you cannot or do not want to use `async` in your implementation.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{blocking, ChannelRegistry, AuthError, Metadata, BoxError};
/// # use chrono::{DateTime, Utc};
///
/// struct MyServer;
///
/// impl blocking::UpstreamServer for MyServer {
///     type QueryParams = ();
///     type Context = ();
///
///     fn auth(&self, _: Option<&str>, _: &()) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     fn initialize(&self, _: (), _: &mut impl ChannelRegistry) -> Result<(), BoxError> {
///         Ok(())
///     }
///
///     fn metadata(&self, _: ()) -> Result<Metadata, BoxError> {
///         Ok(Metadata {
///             id: "my-source".into(),
///             name: "My Source".into(),
///             start_time: DateTime::<Utc>::MIN_UTC,
///             end_time: DateTime::<Utc>::MAX_UTC,
///         })
///     }
///
///     fn stream(&self, _: (), handle: blocking::StreamHandle) -> Result<(), BoxError> {
///         handle.close()?;
///         Ok(())
///     }
/// }
///
/// fn main() {
///     blocking::serve(MyServer, "127.0.0.1:8080".parse().unwrap()).unwrap();
/// }
/// ```
pub fn serve(provider: impl UpstreamServer, bind_addr: SocketAddr) -> std::io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let provider = Arc::new(provider);
        let app = Router::new()
            .route(MANIFEST_ROUTE, get(manifest_handler))
            .route(DATA_ROUTE, get(data_handler))
            .with_state(provider);
        let listener = tokio::net::TcpListener::bind(bind_addr).await?;
        axum::serve(listener, app).await
    })
}
