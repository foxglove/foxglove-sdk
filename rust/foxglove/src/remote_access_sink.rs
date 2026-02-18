use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    remote_access::{RemoteAccessConnection, RemoteAccessConnectionOptions},
    sink_channel_filter::{SinkChannelFilter, SinkChannelFilterFn},
    websocket, ChannelDescriptor, Context, FoxgloveError,
};

use tokio::task::JoinHandle;
pub use websocket::{ChannelView, Client, ClientChannel};

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
#[doc(hidden)]
pub trait RemoteAccessSinkListener: Send + Sync {
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

/// A handle to the RemoteAccessSink connection.
///
/// This handle can safely be dropped and the connection will run forever.
#[doc(hidden)]
pub struct RemoteAccessSinkHandle {
    connection: Arc<RemoteAccessConnection>,
    runner: JoinHandle<()>,
}

impl RemoteAccessSinkHandle {
    fn new(connection: Arc<RemoteAccessConnection>) -> Self {
        let runner = connection.clone().spawn_run_until_cancelled();

        Self { connection, runner }
    }

    /// Gracefully disconnect from the remote access connection, if connected.
    ///
    /// Returns a JoinHandle that will allow waiting until the connection has been fully closed.
    pub fn stop(self) -> JoinHandle<()> {
        // Do we need to do something like the WebSocketServerHandle and return a ShutdownHandle
        // that lets us block until the RemoteAccessConnection is completely shutdown?
        self.connection.shutdown();
        self.runner
    }
}

const FOXGLOVE_DEVICE_TOKEN_ENV: &str = "FOXGLOVE_DEVICE_TOKEN";
const FOXGLOVE_API_URL_ENV: &str = "FOXGLOVE_API_URL";
const FOXGLOVE_API_TIMEOUT_ENV: &str = "FOXGLOVE_API_TIMEOUT";

/// A RemoteAccessSink for live visualization and teleop in Foxglove.
///
/// You may only create one RemoteAccessSink at a time for the device.
#[must_use]
#[doc(hidden)]
#[derive(Default)]
pub struct RemoteAccessSink {
    options: RemoteAccessConnectionOptions,
    device_token: Option<String>,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<Duration>,
}

impl std::fmt::Debug for RemoteAccessSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAccessSink")
            .field("options", &self.options)
            .finish()
    }
}

impl RemoteAccessSink {
    /// Creates a new RemoteAccessSink with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the server name reported in the ServerInfo message.
    ///
    /// If not set, the device name from the Foxglove platform is used.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.options.name = Some(name.into());
        self
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn RemoteAccessSinkListener>) -> Self {
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
        self.options.context = ctx.clone();
        self
    }

    /// Configure the tokio runtime for the server to use for async tasks.
    ///
    /// By default, the server will use either the current runtime (if started with
    /// [`RemoteAccessSink::start`]), or spawn its own internal runtime (if started with
    /// [`RemoteAccessSink::start_blocking`]).
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

    /// Sets the device token for authenticating with the Foxglove platform.
    ///
    /// If not set, the token is read from the `FOXGLOVE_DEVICE_TOKEN` environment variable.
    pub fn device_token(mut self, token: impl Into<String>) -> Self {
        self.device_token = Some(token.into());
        self
    }

    /// Sets the Foxglove API base URL.
    ///
    /// If not set, the URL is read from the `FOXGLOVE_API_URL` environment variable,
    /// falling back to `https://api.foxglove.dev`.
    pub fn foxglove_api_url(mut self, url: impl Into<String>) -> Self {
        self.foxglove_api_url = Some(url.into());
        self
    }

    /// Sets the timeout for Foxglove API requests.
    ///
    /// If not set, the timeout is read from the `FOXGLOVE_API_TIMEOUT` environment variable
    /// (in seconds), falling back to 30 seconds.
    pub fn foxglove_api_timeout(mut self, timeout: Duration) -> Self {
        self.foxglove_api_timeout = Some(timeout);
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

    /// Starts the RemoteAccessSink, which will establish a connection in the background.
    ///
    /// Returns a handle that can optionally be used to manage the sink.
    /// The caller can safely drop the handle and the connection will continue in the background.
    /// Use stop() on the returned handle to stop the connection.
    ///
    /// Returns an error if no device token is provided and the `FOXGLOVE_DEVICE_TOKEN`
    /// environment variable is not set.
    pub fn start(mut self) -> Result<RemoteAccessSinkHandle, FoxgloveError> {
        self.options.device_token = self
            .device_token
            .or_else(|| std::env::var(FOXGLOVE_DEVICE_TOKEN_ENV).ok())
            .ok_or_else(|| {
                FoxgloveError::ConfigurationError(format!(
                    "No device token provided. Set the {FOXGLOVE_DEVICE_TOKEN_ENV} environment variable or call .device_token() on the builder."
                ))
            })?;
        self.options.foxglove_api_url = self
            .foxglove_api_url
            .or_else(|| std::env::var(FOXGLOVE_API_URL_ENV).ok());
        self.options.foxglove_api_timeout = self.foxglove_api_timeout.or_else(|| {
            std::env::var(FOXGLOVE_API_TIMEOUT_ENV)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
        });
        let connection = RemoteAccessConnection::new(self.options);
        Ok(RemoteAccessSinkHandle::new(Arc::new(connection)))
    }
}
