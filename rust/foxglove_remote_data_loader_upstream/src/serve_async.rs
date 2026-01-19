//! Async server implementation.

use std::{error::Error as StdError, future::Future, net::SocketAddr, sync::Arc};

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
    extract_bearer_token, AuthError, BoxError, BuilderMode, ManifestBuilder, ManifestOpts,
    MaybeChannel, Url, DATA_ROUTE, MANIFEST_ROUTE,
};
use foxglove::{stream::McapStreamHandle, Encode, FoxgloveError};

/// Handle for streaming MCAP data with [`UpstreamServer`].
///
/// Returned by [`SourceBuilder::into_stream_handle`] when processing a data request.
pub struct StreamHandle {
    inner: McapStreamHandle,
}

impl StreamHandle {
    /// Flush buffered MCAP data to the stream.
    pub async fn flush(&mut self) -> Result<(), FoxgloveError> {
        self.inner.flush().await
    }

    /// Finish writing and close the MCAP stream.
    ///
    /// This must be called to ensure all data is written.
    pub async fn finish(self) -> Result<(), FoxgloveError> {
        self.inner.close().await
    }
}

/// Builder for constructing a data source, passed to [`UpstreamServer::build_source`].
pub struct SourceBuilder<'a> {
    mode: BuilderMode<'a>,
}

impl<'a> SourceBuilder<'a> {
    /// Declare a channel for logging messages.
    ///
    /// In manifest mode, adds the channel to the manifest but returns an empty [`MaybeChannel`].
    /// In streaming mode, returns a [`MaybeChannel`] which can be used to log messages.
    pub fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> MaybeChannel<T> {
        self.mode.channel(topic.into())
    }

    /// Returns a mutable reference to manifest options if this is a manifest request.
    ///
    /// If `Some`, you should set the fields to provide metadata for the manifest.
    pub fn manifest(&mut self) -> Option<&mut ManifestOpts> {
        self.mode.manifest()
    }

    /// Consume the reader to get a stream handle if this is a data request.
    ///
    /// If `Some`, this is a data request and you can log messages to channels declared earlier
    /// using [`channel`](`Self::channel`).
    pub fn into_stream_handle(self) -> Option<StreamHandle> {
        match self.mode {
            BuilderMode::Manifest { .. } => None,
            BuilderMode::Stream { handle } => Some(StreamHandle { inner: handle }),
        }
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
/// # use foxglove_remote_data_loader_upstream::{UpstreamServer, SourceBuilder, AuthError, ManifestOpts, Url, generate_source_id};
/// # use foxglove::FoxgloveError;
/// # use chrono::{DateTime, Utc};
/// # #[derive(serde::Deserialize, Hash)]
/// # struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// # #[derive(foxglove::Encode)]
/// # struct MyMessage { value: i32 }
/// # struct MyServer;
/// impl UpstreamServer for MyServer {
///     type QueryParams = MyParams;
///     type Error = FoxgloveError;
///
///     async fn auth(&self, _: Option<&str>, _: &MyParams) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     async fn build_source(
///         &self,
///         params: MyParams,
///         mut source: SourceBuilder<'_>,
///     ) -> Result<(), FoxgloveError> {
///         let channel = source.channel::<MyMessage>("/topic");
///
///         if let Some(opts) = source.manifest() {
///             *opts = ManifestOpts {
///                 id: generate_source_id("example", 1, &params),
///                 name: format!("Flight {}", params.flight_id),
///                 start_time: params.start_time,
///                 end_time: params.end_time,
///             };
///         }
///
///         let Some(handle) = source.into_stream_handle() else {
///             return Ok(());
///         };
///
///         channel.log(&MyMessage { value: 42 });
///         handle.finish().await?;
///         Ok(())
///     }
///
///     fn base_url(&self) -> Url {
///         "http://localhost:8080".parse().unwrap()
///     }
/// }
/// ```
pub trait UpstreamServer: Send + Sync + 'static {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send;

    /// Error type returned from [`build_source`](UpstreamServer::build_source).
    type Error: StdError + Send + Sync;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok` to allow access, or `Err` to deny.
    fn auth(
        &self,
        bearer_token: Option<&str>,
        params: &Self::QueryParams,
    ) -> impl Future<Output = Result<(), AuthError>> + Send;

    /// Builds the data source. This is called for both manifest and data requests.
    ///
    /// # Notes
    ///
    /// An implementation should follow these steps:
    ///
    /// 1. Declare channels using [`SourceBuilder::channel`].
    /// 2. Describe the data stream in the manifest using [`SourceBuilder::manifest`].
    /// 3. Stream data using [`SourceBuilder::into_stream_handle`].
    fn build_source(
        &self,
        params: Self::QueryParams,
        source: SourceBuilder<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Returns the base URL for constructing data endpoint URLs in the manifest.
    fn base_url(&self) -> Url;
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

    // Build manifest
    let mut builder = ManifestBuilder::new();
    let source = SourceBuilder {
        mode: BuilderMode::Manifest {
            builder: &mut builder,
        },
    };

    if let Err(error) = provider.build_source(params, source).await {
        warn!(%error, "error building manifest");
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let manifest = builder.build(provider.base_url());
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
    let (handle, mcap_stream) = foxglove::stream::create_mcap_stream();
    let builder = SourceBuilder {
        mode: BuilderMode::Stream { handle },
    };
    let mcap_stream_task =
        tokio::spawn(async move { provider.build_source(params, builder).await });

    // Catch any errors during streaming
    let error_stream = mcap_stream_task
        .into_stream()
        .filter_map(|result| async move {
            match result.expect("panicked while streaming data") {
                Ok(()) => None,
                Err(e) => Some(Err(e.into())),
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
/// # use std::convert::Infallible;
/// # use foxglove_remote_data_loader_upstream::{serve, UpstreamServer, SourceBuilder, AuthError, Url};
///
/// struct MyServer;
///
/// impl UpstreamServer for MyServer {
///     type QueryParams = ();
///     type Error = Infallible;
///
///     async fn auth(&self, token: Option<&str>, params: &Self::QueryParams) -> Result<(), AuthError> {
///         todo!()
///     }
///
///     async fn build_source(&self, params: Self::QueryParams, source: SourceBuilder<'_>) -> Result<(), Infallible> {
///         todo!()
///     }
///
///     fn base_url(&self) -> Url {
///         "http://localhost:8080".parse().unwrap()
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
