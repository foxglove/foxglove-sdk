use std::{collections::HashMap, sync::Arc};

use crate::{
    cloud::{CloudConnection, CloudConnectionOptions},
    sink_channel_filter::{SinkChannelFilter, SinkChannelFilterFn},
    websocket::{self},
    ChannelDescriptor, Context, FoxgloveError,
};

use tokio::task::JoinHandle;
pub use websocket::{ChannelView, Client, ClientChannel};

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait CloudSinkListener: Send + Sync {
    /// Callback invoked when a client message is received.
    fn on_message_data(&self, _client: Client, _client_channel: &ClientChannel, _payload: &[u8]) {}
    /// Callback invoked when a client subscribes to a channel.
    /// Only invoked if the channel is associated with the sink and isn't already subscribed to by the client.
    fn on_subscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    /// Only invoked for channels that had an active subscription from the client.
    fn on_unsubscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client advertises a client channel.
    fn on_client_advertise(&self, _client: Client, _channel: &ClientChannel) {}
    /// Callback invoked when a client unadvertises a client channel.
    fn on_client_unadvertise(&self, _client: Client, _channel: &ClientChannel) {}
}

/// A handle to the CloudSink connection.
///
/// This handle can safely be dropped and the connection will run forever.
#[doc(hidden)]
pub struct CloudSinkHandle {
    connection: Arc<CloudConnection>,
    runner: JoinHandle<()>,
}

impl CloudSinkHandle {
    fn new(connection: Arc<CloudConnection>) -> Self {
        let runner = tokio::spawn(connection.clone().run_until_cancelled());

        Self { connection, runner }
    }

    /// Gracefully disconnect from the cloud, if connected.
    ///
    /// Returns a JoinHandle that will allow waiting until the connection has been fully closed.
    pub fn stop(self) -> JoinHandle<()> {
        // Do we need to do something like the WebSocketServerHandle and return a ShutdownHandle
        // that lets us block until the CloudConnection is completely shutdown?
        self.connection.shutdown();
        self.runner
    }
}

/// An CloudSink for live visualization and teleop in Foxglove.
///
/// Must run Foxglove Agent on the same host for this to work.
#[must_use]
#[derive(Clone)]
#[doc(hidden)]
pub struct CloudSink {
    options: CloudConnectionOptions,
    context: Arc<Context>,
}

impl std::fmt::Debug for CloudSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudSink")
            .field("options", &self.options)
            .field("context", &self.context)
            .finish()
    }
}

impl Default for CloudSink {
    fn default() -> Self {
        Self {
            options: CloudConnectionOptions::default(),
            context: Context::get_default(),
        }
    }
}

impl CloudSink {
    /// Creates a new websocket server with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn CloudSinkListener>) -> Self {
        self.options.capabilities = vec![websocket::Capability::ClientPublish];
        self.options.listener = Some(listener);
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
        self.options.session_id = id.into();
        self
    }

    /// Sets metadata as reported via the ServerInfo message.
    #[doc(hidden)]
    pub fn server_info(mut self, info: HashMap<String, String>) -> Self {
        self.options.server_info = Some(info);
        self
    }

    /// Sets the context for this sink.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Configure the tokio runtime for the server to use for async tasks.
    ///
    /// By default, the server will use either the current runtime (if started with
    /// [`CloudSink::start`]), or spawn its own internal runtime (if started with
    /// [`CloudSink::start_blocking`]).
    #[doc(hidden)]
    pub fn tokio_runtime(mut self, handle: &tokio::runtime::Handle) -> Self {
        self.options.runtime = Some(handle.clone());
        self
    }

    /// Sets a [`SinkChannelFilter`].
    ///
    /// The filter is a function that takes a channel and returns a boolean indicating whether the
    /// channel should be logged.
    pub fn channel_filter(mut self, filter: Arc<dyn SinkChannelFilter>) -> Self {
        self.options.channel_filter = Some(filter);
        self
    }

    /// Sets a channel filter. See [`SinkChannelFilter`] for more information.
    pub fn channel_filter_fn(
        mut self,
        filter: impl Fn(&ChannelDescriptor) -> bool + Sync + Send + 'static,
    ) -> Self {
        self.options.channel_filter = Some(Arc::new(SinkChannelFilterFn(filter)));
        self
    }

    /// Starts the CloudSink, which will establish a connection in the background.
    ///
    /// Returns a handle that can optionally be used to manage the sink.
    /// The caller can safely drop the handle and the connection will continue in the background.
    /// Use stop() on the returned handle to stop the connection.
    pub fn start(self) -> Result<CloudSinkHandle, FoxgloveError> {
        let connection = CloudConnection::new(self.options);
        Ok(CloudSinkHandle::new(Arc::new(connection)))
    }
}
