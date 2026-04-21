use std::{collections::HashMap, fmt::Display, future::Future, sync::Arc, time::Duration};

use indexmap::IndexSet;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::{
    ChannelDescriptor, Context, FoxgloveError, SinkChannelFilter, SinkId,
    protocol::v2::parameter::Parameter,
    remote_common::connection_graph::ConnectionGraph,
    remote_common::fetch_asset::{AssetHandler, AsyncAssetHandlerFn, BlockingAssetHandlerFn},
    remote_common::service::{Service, ServiceMap},
    runtime::get_runtime_handle,
    sink_channel_filter::SinkChannelFilterFn,
};

use super::qos::{QosClassifier, QosClassifierFn, QosProfile};

use super::connection::{ConnectionParams, ConnectionStatus, RemoteAccessConnection};
use super::{Capability, Client, Listener};

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

    /// Returns the sink ID of the current session, if one is active.
    #[doc(hidden)]
    pub fn sink_id(&self) -> Option<SinkId> {
        self.connection.sink_id()
    }

    /// Adds new services, and advertises them to all connected participants.
    ///
    /// This method will fail if the services capability was not declared
    /// ([`ServicesNotSupported`](FoxgloveError::ServicesNotSupported)), if a service name is
    /// not unique ([`DuplicateService`](FoxgloveError::DuplicateService)), or if a service has
    /// no request encoding and the gateway has no supported encodings
    /// ([`MissingRequestEncoding`](FoxgloveError::MissingRequestEncoding)).
    pub fn add_services(
        &self,
        services: impl IntoIterator<Item = Service>,
    ) -> Result<(), FoxgloveError> {
        self.connection.add_services(services.into_iter().collect())
    }

    /// Removes services that were previously advertised.
    ///
    /// Unrecognized service names are silently ignored.
    pub fn remove_services(&self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        self.connection.remove_services(names);
    }

    /// Publishes parameter values to all subscribed clients.
    pub fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        self.connection.publish_parameter_values(parameters);
    }

    /// Publishes a status message to all connected participants.
    ///
    /// This can be used to communicate information, warnings, and errors to the Foxglove app. An
    /// ID may be included in the status to later remove it by referencing that ID.
    pub fn publish_status(&self, status: super::Status) {
        self.connection.publish_status(status);
    }

    /// Removes status messages by ID from all connected participants.
    pub fn remove_status(&self, status_ids: Vec<String>) {
        self.connection.remove_status(status_ids);
    }

    /// Publishes a [ConnectionGraph] update to all subscribed clients.
    ///
    /// Requires the [`ConnectionGraph`](Capability::ConnectionGraph) capability.
    ///
    /// The update is published as a difference from the current graph to `replacement_graph`.
    /// When a client first subscribes to connection graph updates, it receives the current graph.
    pub fn publish_connection_graph(
        &self,
        replacement_graph: ConnectionGraph,
    ) -> Result<(), FoxgloveError> {
        self.connection.replace_connection_graph(replacement_graph)
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
        let params = ConnectionParams {
            name: None,
            device_token: String::new(),
            foxglove_api_url: None,
            foxglove_api_timeout: None,
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            fetch_asset_handler: None,
            runtime: runtime.clone(),
            channel_filter: None,
            qos_classifier: None,
            server_info: None,
            message_backlog_size: None,
            context: std::sync::Weak::new(),
            sysinfo: None,
        };
        let services = Arc::new(parking_lot::RwLock::new(ServiceMap::default()));
        let connection = RemoteAccessConnection::new(params, services);
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
pub struct Gateway {
    name: Option<String>,
    device_token: Option<String>,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<Duration>,
    listener: Option<Arc<dyn Listener>>,
    capabilities: Vec<Capability>,
    supported_encodings: Option<IndexSet<String>>,
    services: HashMap<String, Service>,
    fetch_asset_handler: Option<Box<dyn AssetHandler<Client>>>,
    runtime: Option<Handle>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    qos_classifier: Option<Arc<dyn QosClassifier>>,
    server_info: Option<HashMap<String, String>>,
    message_backlog_size: Option<usize>,
    context: std::sync::Weak<Context>,
    sysinfo: Option<Duration>,
}

impl Default for Gateway {
    fn default() -> Self {
        Self {
            name: None,
            device_token: None,
            foxglove_api_url: None,
            foxglove_api_timeout: None,
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            services: HashMap::new(),
            fetch_asset_handler: None,
            runtime: None,
            channel_filter: None,
            qos_classifier: None,
            server_info: None,
            message_backlog_size: None,
            context: Arc::downgrade(&Context::get_default()),
            sysinfo: None,
        }
    }
}

impl std::fmt::Debug for Gateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dbg = f.debug_struct("Gateway");
        dbg.field("name", &self.name)
            .field("has_device_token", &self.device_token.is_some())
            .field("foxglove_api_url", &self.foxglove_api_url)
            .field("foxglove_api_timeout", &self.foxglove_api_timeout)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .field("num_services", &self.services.len())
            .field(
                "has_fetch_asset_handler",
                &self.fetch_asset_handler.is_some(),
            )
            .field("has_runtime", &self.runtime.is_some())
            .field("has_channel_filter", &self.channel_filter.is_some())
            .field("has_qos_classifier", &self.qos_classifier.is_some())
            .field("server_info", &self.server_info)
            .field("message_backlog_size", &self.message_backlog_size)
            .field("has_context", &(self.context.strong_count() > 0));
        dbg.field("sysinfo", &self.sysinfo);
        dbg.finish()
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
        self.name = Some(name.into());
        self
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn Listener>) -> Self {
        self.listener = Some(listener);
        self
    }

    /// Sets capabilities to advertise in the server info message.
    pub fn capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    /// Configure the set of supported encodings for client requests.
    ///
    /// This is used for both client-side publishing as well as service call request/responses.
    pub fn supported_encodings(
        mut self,
        encodings: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.supported_encodings = Some(encodings.into_iter().map(|e| e.into()).collect());
        self
    }

    /// Sets metadata as reported via the ServerInfo message.
    #[doc(hidden)]
    pub fn server_info(mut self, info: HashMap<String, String>) -> Self {
        self.server_info = Some(info);
        self
    }

    /// Sets the context for this sink.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = Arc::downgrade(ctx);
        self
    }

    /// Enables or disables publishing process and system statistics to the
    /// `/sysinfo` topic.
    ///
    /// When set to `Some(interval)`, the gateway publishes a
    /// [`SystemInfo`][crate::system_info::SystemInfo] message at the given
    /// `interval` for the duration of the gateway's lifetime. Clamped to a minimum of 200ms.
    ///
    /// Defaults to `None` (disabled).
    pub fn sysinfo(mut self, refresh_interval: Option<Duration>) -> Self {
        self.sysinfo = refresh_interval;
        self
    }

    /// Configure the tokio runtime for the gateway to use for async tasks.
    ///
    /// By default, the gateway will use either the current runtime, or spawn its own internal runtime.
    #[doc(hidden)]
    pub fn tokio_runtime(mut self, handle: &tokio::runtime::Handle) -> Self {
        self.runtime = Some(handle.clone());
        self
    }

    /// Sets a [`SinkChannelFilter`].
    ///
    /// The filter is a function that takes a channel and returns a boolean indicating whether the
    /// channel should be logged.
    pub fn channel_filter(mut self, filter: Arc<dyn SinkChannelFilter>) -> Self {
        self.channel_filter = Some(filter);
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

    /// Set the per-participant control plane message queue size.
    ///
    /// Each participant gets an independent queue of this size. If a participant's
    /// queue fills up (because it is not reading fast enough), it will be disconnected
    /// and asked to reconnect.
    ///
    /// By default, each participant gets a queue of 1024 messages.
    pub fn message_backlog_size(mut self, size: usize) -> Self {
        self.message_backlog_size = Some(size);
        self
    }

    /// Sets a channel filter. See [`SinkChannelFilter`] for more information.
    pub fn channel_filter_fn(
        mut self,
        filter: impl Fn(&ChannelDescriptor) -> bool + Sync + Send + 'static,
    ) -> Self {
        self.channel_filter = Some(Arc::new(SinkChannelFilterFn(filter)));
        self
    }

    /// Sets a [`QosClassifier`] for assigning quality-of-service profiles to channels.
    ///
    /// The classifier is invoked when channels are registered and determines how data for
    /// each channel is delivered to remote participants.
    ///
    /// If not set, all channels use the default [`QosProfile`].
    pub fn qos_classifier(mut self, classifier: Arc<dyn QosClassifier>) -> Self {
        self.qos_classifier = Some(classifier);
        self
    }

    /// Sets a QoS classifier function. See [`QosClassifier`] for more information.
    pub fn qos_classifier_fn(
        mut self,
        classifier: impl Fn(&ChannelDescriptor) -> QosProfile + Sync + Send + 'static,
    ) -> Self {
        self.qos_classifier = Some(Arc::new(QosClassifierFn(classifier)));
        self
    }

    /// Configure the set of services to advertise to clients.
    ///
    /// Automatically adds [`Capability::Services`] to the set of advertised capabilities.
    pub fn services(mut self, services: impl IntoIterator<Item = Service>) -> Self {
        self.services.clear();
        for service in services {
            let name = service.name().to_string();
            if let Some(s) = self.services.insert(name, service) {
                tracing::warn!("Redefining service {}", s.name());
            }
        }
        self
    }

    /// Configure the handler for fetching assets.
    /// There can only be one asset handler, exclusive with the other fetch_asset_handler methods.
    pub fn fetch_asset_handler(mut self, handler: Box<dyn AssetHandler<Client>>) -> Self {
        self.fetch_asset_handler = Some(handler);
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
        self.fetch_asset_handler = Some(Box::new(BlockingAssetHandlerFn(Arc::new(handler))));
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
        self.fetch_asset_handler = Some(Box::new(AsyncAssetHandlerFn(Arc::new(handler))));
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
        let device_token = self
            .device_token
            .or_else(|| std::env::var(FOXGLOVE_DEVICE_TOKEN_ENV).ok())
            .ok_or_else(|| {
                FoxgloveError::ConfigurationError(format!(
                    "No device token provided. Set the {FOXGLOVE_DEVICE_TOKEN_ENV} environment variable or call .device_token() on the builder."
                ))
            })?;
        let foxglove_api_url = self
            .foxglove_api_url
            .or_else(|| std::env::var(FOXGLOVE_API_URL_ENV).ok());
        let foxglove_api_timeout = self.foxglove_api_timeout.or_else(|| {
            std::env::var(FOXGLOVE_API_TIMEOUT_ENV)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
        });
        // If the gateway was declared with services, automatically add the "services" capability
        // and the set of supported request encodings.
        if !self.services.is_empty() {
            if !self.capabilities.contains(&Capability::Services) {
                self.capabilities.push(Capability::Services);
            }
            let encodings = self
                .supported_encodings
                .get_or_insert_with(Default::default);
            for svc in self.services.values() {
                if let Some(encoding) = svc.request_encoding() {
                    encodings.insert(encoding.to_string());
                }
            }
            if encodings.is_empty() {
                if let Some(svc) = self
                    .services
                    .values()
                    .find(|s| s.request_encoding().is_none())
                {
                    return Err(FoxgloveError::MissingRequestEncoding(
                        svc.name().to_string(),
                    ));
                }
            }
        }
        // If the gateway was declared with a fetch asset handler, automatically add the "assets" capability.
        if self.fetch_asset_handler.is_some() && !self.capabilities.contains(&Capability::Assets) {
            self.capabilities.push(Capability::Assets);
        }
        // Conversely, the "assets" capability requires a fetch asset handler.
        if self.capabilities.contains(&Capability::Assets) && self.fetch_asset_handler.is_none() {
            return Err(FoxgloveError::ConfigurationError(
                "The Assets capability requires a fetch asset handler. \
                 Use fetch_asset_handler(), fetch_asset_handler_blocking_fn(), \
                 or fetch_asset_handler_async_fn()."
                    .to_string(),
            ));
        }
        let runtime = self.runtime.unwrap_or_else(get_runtime_handle);
        let services = Arc::new(parking_lot::RwLock::new(ServiceMap::from_iter(
            self.services.into_values(),
        )));
        let params = ConnectionParams {
            name: self.name,
            device_token,
            foxglove_api_url,
            foxglove_api_timeout,
            listener: self.listener,
            capabilities: self.capabilities,
            supported_encodings: self.supported_encodings,
            fetch_asset_handler: self.fetch_asset_handler.map(Arc::from),
            runtime: runtime.clone(),
            channel_filter: self.channel_filter,
            qos_classifier: self.qos_classifier,
            server_info: self.server_info,
            message_backlog_size: self.message_backlog_size,
            context: self.context,
            sysinfo: self.sysinfo,
        };
        let connection = RemoteAccessConnection::new(params, services);
        Ok(GatewayHandle::new(Arc::new(connection), runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FoxgloveError;
    use crate::remote_common::service::{Service, ServiceSchema};

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

    #[test]
    fn test_initial_service_missing_request_encoding() {
        // Services configured at creation time are also validated for request encodings.
        let svc =
            Service::builder("/s", ServiceSchema::new("")).handler_fn(|_| Ok::<_, String>(b""));
        let result = Gateway::new()
            .device_token("test-token")
            .services([svc])
            .start();
        assert!(matches!(
            result,
            Err(FoxgloveError::MissingRequestEncoding(_))
        ));
    }

    #[test]
    fn test_assets_capability_without_handler() {
        // Advertising the Assets capability without a handler is a configuration error.
        let result = Gateway::new()
            .device_token("test-token")
            .capabilities([Capability::Assets])
            .start();
        assert!(matches!(result, Err(FoxgloveError::ConfigurationError(_))));
    }
}
