//! Blocking server implementation.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Query, RawQuery, State},
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
    #[doc = include_str!("docs/query_params.md")]
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

    /// Context type passed from [`initialize`](UpstreamServer::initialize) to
    /// [`metadata`](UpstreamServer::metadata) and [`stream`](UpstreamServer::stream).
    ///
    /// This can hold anything, but is typically used to store request-specific state (e.g.
    /// [`Channel`](foxglove::Channel)s and query parameters)
    type Context: Send;

    #[doc = include_str!("docs/auth.md")]
    fn auth(&self, bearer_token: Option<&str>, params: &Self::QueryParams)
        -> Result<(), AuthError>;

    #[doc = include_str!("docs/initialize.md")]
    fn initialize(
        &self,
        params: Self::QueryParams,
        registry: &mut impl ChannelRegistry,
    ) -> Result<Self::Context, BoxError>;

    #[doc = include_str!("docs/metadata.md")]
    fn metadata(&self, ctx: Self::Context) -> Result<Metadata, BoxError>;

    #[doc = include_str!("docs/stream.md")]
    fn stream(&self, ctx: Self::Context, handle: StreamHandle) -> Result<(), BoxError>;
}

async fn manifest_handler<P: UpstreamServer>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    RawQuery(query_string): RawQuery,
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
        Ok(manifest_builder.build(metadata, query_string.as_deref()))
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

    let (ctx, stream_handle) = {
        let provider = Arc::clone(&provider);
        match tokio::task::spawn_blocking(move || {
            provider
                .initialize(params, &mut stream_handle)
                .map(move |ctx| (ctx, stream_handle))
        })
        .await
        .expect("panicked during initialization")
        {
            Ok(res) => res,
            Err(error) => {
                error!(%error, "error during initialization");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    let mcap_stream_task = tokio::task::spawn_blocking(move || {
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
///     fn auth(&self, _bearer_token: Option<&str>, _params: &()) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     fn initialize(&self, _params: (), _registry: &mut impl ChannelRegistry) -> Result<(), BoxError> {
///         Ok(())
///     }
///
///     fn metadata(&self, _ctx: ()) -> Result<Metadata, BoxError> {
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
