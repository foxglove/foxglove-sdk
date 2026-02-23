use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    ChannelDescriptor, Context, FoxgloveError,
    sink_channel_filter::{SinkChannelFilter, SinkChannelFilterFn},
};

use tokio::task::JoinHandle;

use super::connection::{RemoteAccessConnection, RemoteAccessConnectionOptions};
use super::{Capability, Listener};

/// A handle to the remote access gateway connection.
///
/// This handle can safely be dropped and the connection will run forever.
#[doc(hidden)]
pub struct GatewayHandle {
    connection: Arc<RemoteAccessConnection>,
    runner: JoinHandle<()>,
}

impl GatewayHandle {
    fn new(connection: Arc<RemoteAccessConnection>) -> Self {
        let runner = connection.clone().spawn_run_until_cancelled();

        Self { connection, runner }
    }

    /// Gracefully disconnect from the remote access connection, if connected.
    ///
    /// Returns a JoinHandle that will allow waiting until the connection has been fully closed.
    pub fn stop(self) -> JoinHandle<()> {
        self.connection.shutdown();
        self.runner
    }
}

const FOXGLOVE_DEVICE_TOKEN_ENV: &str = "FOXGLOVE_DEVICE_TOKEN";
const FOXGLOVE_API_URL_ENV: &str = "FOXGLOVE_API_URL";
const FOXGLOVE_API_TIMEOUT_ENV: &str = "FOXGLOVE_API_TIMEOUT";

/// A remote access gateway for live visualization and teleop in Foxglove.
///
/// You may only create one gateway at a time for the device.
#[must_use]
#[doc(hidden)]
#[derive(Default)]
pub struct Gateway {
    options: RemoteAccessConnectionOptions,
    device_token: Option<String>,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<Duration>,
}

impl std::fmt::Debug for Gateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gateway")
            .field("options", &self.options)
            .finish()
    }
}

impl Gateway {
    /// Creates a new Gateway with default options.
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
    pub fn listener(mut self, listener: Arc<dyn Listener>) -> Self {
        self.options.listener = Some(listener);
        self
    }

    /// Sets capabilities to advertise in the server info message.
    pub fn capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.options.capabilities = capabilities.into_iter().collect();
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
        self.options.context = Arc::downgrade(ctx);
        self
    }

    /// Configure the tokio runtime for the gateway to use for async tasks.
    ///
    /// By default, the gateway will use either the current runtime (if started with
    /// [`Gateway::start`]), or spawn its own internal runtime (if started with
    /// [`Gateway::start_blocking`]).
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

    /// Set the message backlog size.
    ///
    /// The sink buffers outgoing log entries into a queue. If the backlog size is exceeded, the
    /// oldest entries will be dropped.
    ///
    /// By default, the sink will buffer 1024 messages.
    pub fn message_backlog_size(mut self, size: usize) -> Self {
        self.options.message_backlog_size = Some(size);
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

    /// Starts the remote access gateway, which will establish a connection in the background.
    ///
    /// Returns a handle that can optionally be used to manage the gateway.
    /// The caller can safely drop the handle and the connection will continue in the background.
    /// Use stop() on the returned handle to stop the connection.
    ///
    /// Returns an error if no device token is provided and the `FOXGLOVE_DEVICE_TOKEN`
    /// environment variable is not set.
    pub fn start(mut self) -> Result<GatewayHandle, FoxgloveError> {
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
        Ok(GatewayHandle::new(Arc::new(connection)))
    }
}
