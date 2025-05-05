//! Websocket server

use std::fmt::{Debug, Display};
use std::future::Future;
use std::sync::Arc;

use crate::websocket::service::Service;
use crate::websocket::{
    create_server, AssetHandler, AsyncAssetHandlerFn, BlockingAssetHandlerFn, Capability, Client,
    ConnectionGraph, Parameter, Server, ServerOptions, Status,
};
use crate::{get_runtime_handle, Context, FoxgloveError};

/// A websocket server for live visualization.
///
/// ### Buffering
///
/// Logged messages are queued in a channel for each client and delivered in a background task. If a
/// queue fills, perhaps because of a slow client, then the oldest messages will be dropped. The
/// queue size is configurable with [`WebSocketServer::message_backlog_size`] when creating the
/// server.
///
/// Other protocol messages, including status updates, are delivered from a separate "control"
/// queue, using the same configured queue size. If the control queue fills, then the slow client is
/// dropped.
#[must_use]
#[derive(Debug)]
pub struct WebSocketServer {
    host: String,
    port: u16,
    options: ServerOptions,
    context: Arc<Context>,
}

impl Default for WebSocketServer {
    fn default() -> Self {
        let options = ServerOptions {
            session_id: Some(Server::generate_session_id()),
            ..ServerOptions::default()
        };
        Self {
            host: "127.0.0.1".into(),
            port: 8765,
            options,
            context: Context::get_default(),
        }
    }
}

impl WebSocketServer {
    /// Creates a new websocket server with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the websocket server name to advertise to clients.
    ///
    /// By default, the server is not given a name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.options.name = Some(name.into());
        self
    }

    /// Bind a TCP port.
    ///
    /// `port` may be 0, in which case an available port will be automatically selected.
    ///
    /// By default, the server will bind to `127.0.0.1:8765`.
    pub fn bind(mut self, host: impl Into<String>, port: u16) -> Self {
        self.host = host.into();
        self.port = port;
        self
    }

    /// Sets the server capabilities to advertise to the client.
    ///
    /// By default, the server does not advertise any capabilities.
    pub fn capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.options.capabilities = Some(capabilities.into_iter().collect());
        self
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn crate::websocket::ServerListener>) -> Self {
        self.options.listener = Some(listener);
        self
    }

    /// Configure the handler for fetching assets.
    /// There can only be one asset handler, exclusive with the other fetch_asset_handler methods.
    pub fn fetch_asset_handler(mut self, handler: Box<dyn AssetHandler>) -> Self {
        self.options.fetch_asset_handler = Some(handler);
        self
    }

    /// Configure a synchronous, blocking function as a fetch asset handler.
    /// There can only be one asset handler, exclusive with the other fetch_asset_handler methods.
    pub fn fetch_asset_handler_blocking_fn<F, T, Err>(mut self, handler: F) -> Self
    where
        F: Fn(Client, String) -> Result<T, Err> + Send + Sync + 'static,
        T: AsRef<[u8]>,
        Err: Display,
    {
        self.options.fetch_asset_handler =
            Some(Box::new(BlockingAssetHandlerFn(Arc::new(handler))));
        self
    }

    /// Configure an asynchronous function as a fetch asset handler.
    /// There can only be one asset handler, exclusive with the other fetch_asset_handler methods.
    pub fn fetch_asset_handler_async_fn<F, Fut, T, Err>(mut self, handler: F) -> Self
    where
        F: Fn(Client, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, Err>> + Send + 'static,
        T: AsRef<[u8]>,
        Err: Display,
    {
        self.options.fetch_asset_handler = Some(Box::new(AsyncAssetHandlerFn(Arc::new(handler))));
        self
    }

    /// Set the message backlog size.
    ///
    /// The server buffers outgoing log entries into a queue. If the backlog size is exceeded, the
    /// oldest entries will be dropped.
    ///
    /// By default, the server will buffer 1024 messages.
    pub fn message_backlog_size(mut self, size: usize) -> Self {
        self.options.message_backlog_size = Some(size);
        self
    }

    /// Configure the set of services to advertise to clients.
    ///
    /// Automatically adds [`Capability::Services`] to the set of advertised capabilities.
    ///
    /// Note that services can by dynamically registered and unregistered later using
    /// [`WebSocketServerHandle::add_services`] and [`WebSocketServerHandle::remove_services`].
    pub fn services(mut self, services: impl IntoIterator<Item = Service>) -> Self {
        self.options.services.clear();
        for service in services {
            let name = service.name().to_string();
            if let Some(s) = self.options.services.insert(name.clone(), service) {
                tracing::warn!("Redefining service {}", s.name());
            }
        }
        self
    }

    /// Configure the set of supported encodings for client requests.
    ///
    /// This is used for both client-side publishing as well as service call request/responses.
    pub fn supported_encodings(
        mut self,
        encodings: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.options.supported_encodings = Some(encodings.into_iter().map(|e| e.into()).collect());
        self
    }

    /// Set a session ID.
    ///
    /// This allows the client to understand if the connection is a re-connection or if it is
    /// connecting to a new server instance. This can for example be a timestamp or a UUID.
    ///
    /// By default, this is set to the number of milliseconds since the unix epoch.
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.options.session_id = Some(id.into());
        self
    }

    /// Configure the tokio runtime for the server to use for async tasks.
    ///
    /// By default, the server will use either the current runtime (if started with
    /// [`WebSocketServer::start`]), or spawn its own internal runtime (if started with
    /// [`WebSocketServer::start_blocking`]).
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    pub fn tokio_runtime(mut self, handle: &tokio::runtime::Handle) -> Self {
        self.options.runtime = Some(handle.clone());
        self
    }

    /// Sets the context for this sink.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Starts the websocket server.
    ///
    /// Returns a handle that can optionally be used to gracefully shutdown the server. The caller
    /// can safely drop the handle, and the server will run forever.
    pub async fn start(self) -> Result<WebSocketServerHandle, FoxgloveError> {
        let server = create_server(&self.context, self.options);
        server.start(&self.host, self.port).await?;
        Ok(WebSocketServerHandle(server))
    }

    /// Starts the websocket server.
    ///
    /// Returns a handle that can optionally be used to gracefully shutdown the server. The caller
    /// can safely drop the handle, and the server will run forever.
    ///
    /// If you choose to use this blocking interface rather than [`WebSocketServer::start`],
    /// the SDK will spawn a multi-threaded [tokio] runtime.
    ///
    /// This method will panic if invoked from an asynchronous execution context. Use
    /// [`WebSocketServer::start`] instead.
    ///
    /// [tokio]: https://docs.rs/tokio/latest/tokio/
    pub fn start_blocking(mut self) -> Result<WebSocketServerHandle, FoxgloveError> {
        let runtime = self
            .options
            .runtime
            .get_or_insert_with(get_runtime_handle)
            .clone();
        let handle = runtime.block_on(self.start())?;
        Ok(handle)
    }
}

/// A handle to the websocket server.
///
/// This handle can safely be dropped and the server will run forever.
pub struct WebSocketServerHandle(Arc<Server>);

impl Debug for WebSocketServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WebSocketServerHandle").finish()
    }
}

impl WebSocketServerHandle {
    /// Returns the local port that the server is listening on.
    pub fn port(&self) -> u16 {
        self.0.port()
    }

    /// Advertises support for the provided services.
    ///
    /// These services will be available for clients to use until they are removed with
    /// [`remove_services`][WebSocketServerHandle::remove_services].
    ///
    /// This method will fail if the server was not configured with [`Capability::Services`].
    pub fn add_services(
        &self,
        services: impl IntoIterator<Item = Service>,
    ) -> Result<(), FoxgloveError> {
        self.0.add_services(services.into_iter().collect())
    }

    /// Removes services that were previously advertised.
    pub fn remove_services(&self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        self.0.remove_services(names);
    }

    /// Publishes the current server timestamp to all clients.
    #[doc(hidden)]
    #[cfg(feature = "unstable")]
    pub fn broadcast_time(&self, timestamp_nanos: u64) {
        self.0.broadcast_time(timestamp_nanos);
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    pub fn clear_session(&self, new_session_id: Option<String>) {
        self.0.clear_session(new_session_id);
    }

    /// Publishes parameter values to all subscribed clients.
    pub fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        self.0.publish_parameter_values(parameters);
    }

    /// Publishes a status message to all clients.
    ///
    /// For more information, refer to the [Status][status] message specification.
    ///
    /// [status]: https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#status
    pub fn publish_status(&self, status: Status) {
        self.0.publish_status(status);
    }

    /// Removes status messages by id from all clients.
    ///
    /// For more information, refer to the [Remove Status][remove-status] message specification.
    ///
    /// [remove-status]: https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#remove-status
    pub fn remove_status(&self, status_ids: Vec<String>) {
        self.0.remove_status(status_ids);
    }

    /// Publishes a connection graph update to all subscribed clients.
    /// The update is published as a difference from the current graph to replacement_graph.
    /// When a client first subscribes to connection graph updates, it receives the current graph.
    pub fn publish_connection_graph(
        &self,
        replacement_graph: ConnectionGraph,
    ) -> Result<(), FoxgloveError> {
        self.0.replace_connection_graph(replacement_graph)
    }

    /// Gracefully shutdown the websocket server.
    pub fn stop(self) {
        self.0.stop();
    }
}
