//! Blocking server implementation.

use std::{error::Error as StdError, net::SocketAddr, sync::Arc};

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
use tracing::warn;

use crate::{
    extract_bearer_token, AuthError, BoxError, BuilderMode, ManifestBuilder, ManifestOpts,
    MaybeChannel, Url, DATA_ROUTE, MANIFEST_ROUTE,
};
use foxglove::{stream::McapStreamHandle, Encode, FoxgloveError};

/// Handle for streaming MCAP data with [`UpstreamServerBlocking`].
///
/// Returned by [`SourceBuilderBlocking::into_stream_handle`] when processing a data request.
pub struct StreamHandleBlocking {
    inner: McapStreamHandle,
}

impl StreamHandleBlocking {
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

/// Builder for constructing a data source, passed to [`UpstreamServerBlocking::build_source`].
pub struct SourceBuilderBlocking<'a> {
    mode: BuilderMode<'a>,
}

impl<'a> SourceBuilderBlocking<'a> {
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
    pub fn into_stream_handle(self) -> Option<StreamHandleBlocking> {
        match self.mode {
            BuilderMode::Manifest { .. } => None,
            BuilderMode::Stream { handle } => Some(StreamHandleBlocking { inner: handle }),
        }
    }
}

/// Blocking upstream server trait.
///
/// Implement this trait to serve manifest and data endpoints without writing `async` code.
///
/// Use [`serve_blocking`] to start the server.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{UpstreamServerBlocking, SourceBuilderBlocking, AuthError, ManifestOpts, Url, generate_source_id};
/// # use foxglove::FoxgloveError;
/// # use chrono::{DateTime, Utc};
/// # #[derive(serde::Deserialize, Hash)]
/// # struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// # #[derive(foxglove::Encode)]
/// # struct MyMessage { value: i32 }
/// # struct MyServer;
/// impl UpstreamServerBlocking for MyServer {
///     type QueryParams = MyParams;
///     type Error = FoxgloveError;
///
///     fn auth(&self, _: Option<&str>, _: &MyParams) -> Result<(), AuthError> {
///         Ok(())
///     }
///
///     fn build_source(
///         &self,
///         params: MyParams,
///         mut source: SourceBuilderBlocking<'_>,
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
///         handle.close()?;
///         Ok(())
///     }
///
///     fn base_url(&self) -> Url {
///         "http://localhost:8080".parse().unwrap()
///     }
/// }
/// ```
pub trait UpstreamServerBlocking: Send + Sync + 'static {
    /// Query parameters extracted from the request URL.
    type QueryParams: DeserializeOwned + Send;

    /// Error type returned from [`build_source`](UpstreamServerBlocking::build_source).
    type Error: StdError + Send + Sync;

    /// Authenticate and authorize the request.
    ///
    /// Return `Ok` to allow access, or `Err` to deny.
    fn auth(&self, bearer_token: Option<&str>, params: &Self::QueryParams)
        -> Result<(), AuthError>;

    /// Builds the data source. This is called for both manifest and data requests.
    ///
    /// # Notes
    ///
    /// An implementation should follow these steps:
    ///
    /// 1. Declare channels using [`SourceBuilderBlocking::channel`].
    /// 2. Describe the data stream in the manifest using [`SourceBuilderBlocking::manifest`].
    /// 3. Stream data using [`SourceBuilderBlocking::into_stream_handle`].
    fn build_source(
        &self,
        params: Self::QueryParams,
        source: SourceBuilderBlocking<'_>,
    ) -> Result<(), Self::Error>;

    /// Returns the base URL for constructing data endpoint URLs in the manifest.
    fn base_url(&self) -> Url;
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

        let mut manifest_builder = ManifestBuilder::new();
        let source_builder = SourceBuilderBlocking {
            mode: BuilderMode::Manifest {
                builder: &mut manifest_builder,
            },
        };

        if let Err(error) = provider.build_source(params, source_builder) {
            warn!(%error, "error building manifest");
            return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }

        // Build manifest
        Ok(manifest_builder.build(provider.base_url()))
    })
    .await;

    match result.expect("panicked while building manifest") {
        Ok(manifest) => Json(manifest).into_response(),
        Err(response) => response,
    }
}

async fn data_handler_blocking<P: UpstreamServerBlocking>(
    State(provider): State<Arc<P>>,
    headers: HeaderMap,
    Query(params): Query<P::QueryParams>,
) -> Response {
    // Auth
    let auth_result = {
        let provider = Arc::clone(&provider);
        tokio::task::spawn_blocking(move || {
            provider
                .auth(extract_bearer_token(&headers), &params)
                .map(|()| params) // Move params back to the calling task.
        })
        .await
        .expect("panicked during auth")
    };
    let params = match auth_result {
        Ok(params) => params,
        Err(e) => return e.into_response(),
    };

    // Build MCAP data stream
    let (handle, mcap_stream) = foxglove::stream::create_mcap_stream();
    let mcap_stream_task = tokio::task::spawn_blocking(move || {
        provider.build_source(
            params,
            SourceBuilderBlocking {
                mode: BuilderMode::Stream { handle },
            },
        )
    });
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

/// Serve both manifest and data endpoints using [`UpstreamServerBlocking`].
///
/// Use this if you cannot or do not want to use `async` in your implementation.
///
/// # Example
///
/// ```no_run
/// # use foxglove_remote_data_loader_upstream::{serve_blocking, UpstreamServerBlocking, SourceBuilderBlocking, AuthError, Url};
/// # use foxglove::FoxgloveError;
/// # use chrono::{DateTime, Utc};
/// # #[derive(serde::Deserialize, Hash)]
/// # struct MyParams { flight_id: String, start_time: DateTime<Utc>, end_time: DateTime<Utc> }
/// # struct MyServer;
/// # impl UpstreamServerBlocking for MyServer {
/// #     type QueryParams = MyParams;
/// #     type Error = FoxgloveError;
/// #     fn auth(&self, _: Option<&str>, _: &MyParams) -> Result<(), AuthError> { Ok(()) }
/// #     fn build_source(&self, _: MyParams, _: SourceBuilderBlocking<'_>) -> Result<(), FoxgloveError> { Ok(()) }
/// #     fn base_url(&self) -> Url { "http://localhost:8080".parse().unwrap() }
/// # }
/// fn main() {
///     serve_blocking(MyServer, "127.0.0.1:8080".parse().unwrap()).unwrap();
/// }
/// ```
pub fn serve_blocking(
    provider: impl UpstreamServerBlocking,
    bind_addr: SocketAddr,
) -> std::io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let provider = Arc::new(provider);
        let app = Router::new()
            .route(MANIFEST_ROUTE, get(manifest_handler_blocking))
            .route(DATA_ROUTE, get(data_handler_blocking))
            .with_state(provider);
        let listener = tokio::net::TcpListener::bind(bind_addr).await?;
        axum::serve(listener, app).await
    })
}
