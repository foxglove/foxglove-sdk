use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    ChannelDescriptor, Context, FoxgloveError,
    remote_common::service::Service,
    runtime::get_runtime_handle,
    sink_channel_filter::{SinkChannelFilter, SinkChannelFilterFn},
};

use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use super::connection::{ConnectionStatus, RemoteAccessConnection, RemoteAccessConnectionOptions};
use super::{Capability, Listener};

/// A handle to the remote access gateway connection.
///
/// This handle can safely be dropped and the connection will run forever.
pub struct GatewayHandle {
    connection: Arc<RemoteAccessConnection>,
    runner: JoinHandle<()>,
    runtime: Handle,
}

impl GatewayHandle {
    fn new(connection: Arc<RemoteAccessConnection>, runtime: Handle) -> Self {
        let runner = connection.clone().spawn_run_until_cancelled();

        Self {
            connection,
            runner,
            runtime,
        }
    }

    /// Returns the current connection status.
    pub fn connection_status(&self) -> ConnectionStatus {
        self.connection.status()
    }

    /// Gracefully disconnect from the remote access connection, if connected.
    ///
    /// Returns a JoinHandle that will allow waiting until the connection has been fully closed.
    pub fn stop(self) -> JoinHandle<()> {
        self.connection.shutdown();
        self.runner
    }

    #[cfg(test)]
    fn with_runner(runner: JoinHandle<()>, runtime: Handle) -> Self {
        let connection = RemoteAccessConnection::new(RemoteAccessConnectionOptions::default());
        Self {
            connection: Arc::new(connection),
            runner,
            runtime,
        }
    }

    /// Gracefully disconnect and wait for the connection to close from a blocking context.
    ///
    /// This method will panic if invoked from an asynchronous execution context. Use
    /// [`GatewayHandle::stop`] instead.
    pub fn stop_blocking(self) {
        self.connection.shutdown();
        if let Err(e) = self.runtime.block_on(self.runner) {
            tracing::warn!("Gateway connection task panicked: {e}");
        }
    }
}

const FOXGLOVE_DEVICE_TOKEN_ENV: &str = "FOXGLOVE_DEVICE_TOKEN";
const FOXGLOVE_API_URL_ENV: &str = "FOXGLOVE_API_URL";
const FOXGLOVE_API_TIMEOUT_ENV: &str = "FOXGLOVE_API_TIMEOUT";

/// A remote access gateway for live visualization and teleop in Foxglove.
///
/// You may only create one gateway at a time for the device.
#[must_use]
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
    /// By default, the gateway will use either the current runtime, or spawn its own internal runtime.
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

    /// Configure the set of services to advertise to clients.
    ///
    /// Automatically adds [`Capability::Services`] to the set of advertised capabilities.
    pub fn services(mut self, services: impl IntoIterator<Item = Service>) -> Self {
        self.options.services.clear();
        for service in services {
            let name = service.name().to_string();
            if let Some(s) = self.options.services.insert(name, service) {
                tracing::warn!("Redefining service {}", s.name());
            }
        }
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
        // If the gateway was declared with services, automatically add the "services" capability
        // and the set of supported request encodings.
        if !self.options.services.is_empty() {
            if !self.options.capabilities.contains(&Capability::Services) {
                self.options.capabilities.push(Capability::Services);
            }
            let encodings = self
                .options
                .supported_encodings
                .get_or_insert_with(Default::default);
            for svc in self.options.services.values() {
                if let Some(encoding) = svc.request_encoding() {
                    encodings.insert(encoding.to_string());
                }
            }
        }
        let runtime = self
            .options
            .runtime
            .get_or_insert_with(get_runtime_handle)
            .clone();
        let connection = RemoteAccessConnection::new(self.options);
        Ok(GatewayHandle::new(Arc::new(connection), runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_blocking_clean_shutdown() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let runner = rt.spawn(async {});
        let handle = GatewayHandle::with_runner(runner, rt.handle().clone());
        handle.stop_blocking();
    }

    #[test]
    fn stop_blocking_logs_panic() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let runner = rt.spawn(async { panic!("test panic") });
        // Allow the task to run and panic.
        std::thread::sleep(std::time::Duration::from_millis(10));
        let handle = GatewayHandle::with_runner(runner, rt.handle().clone());
        // Should not panic; should log a warning.
        handle.stop_blocking();
    }
}
