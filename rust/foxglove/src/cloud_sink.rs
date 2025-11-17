use std::sync::Arc;

use crate::{
    get_runtime_handle,
    sink_channel_filter::{SinkChannelFilter, SinkChannelFilterFn},
    websocket::{self, Server, ShutdownHandle},
    ChannelDescriptor, Context, FoxgloveError, WebSocketServer, WebSocketServerHandle,
};

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

struct CloudSinkListenerAdapter {
    listener: Arc<dyn CloudSinkListener>,
}

impl websocket::ServerListener for CloudSinkListenerAdapter {
    fn on_message_data(&self, client: Client, channel: &ClientChannel, payload: &[u8]) {
        self.listener.on_message_data(client, channel, payload);
    }

    fn on_subscribe(&self, client: Client, channel: ChannelView) {
        self.listener.on_subscribe(client, channel);
    }

    fn on_unsubscribe(&self, client: Client, channel: ChannelView) {
        self.listener.on_unsubscribe(client, channel);
    }

    fn on_client_advertise(&self, client: Client, channel: &ClientChannel) {
        self.listener.on_client_advertise(client, channel);
    }

    fn on_client_unadvertise(&self, client: Client, channel: &ClientChannel) {
        self.listener.on_client_unadvertise(client, channel);
    }
}

/// A handle to the CloudSink connection.
///
/// This handle can safely be dropped and the connection will run forever.
#[doc(hidden)]
pub struct CloudSinkHandle {
    server: WebSocketServerHandle,
}

impl CloudSinkHandle {
    fn new(server: WebSocketServerHandle) -> Self {
        Self { server }
    }

    /// Gracefully disconnect from the cloud, if connected. Otherwise returns None.
    ///
    /// Returns a handle that can be used to wait for the graceful shutdown to complete.
    pub fn stop(self) -> Option<ShutdownHandle> {
        Some(self.server.stop())
    }
}

/// An CloudSink for live visualization and teleop in Foxglove.
///
/// Must run Foxglove Agent on the same host for this to work.
#[must_use]
#[derive(Clone)]
#[doc(hidden)]
pub struct CloudSink {
    session_id: String,
    capabilities: Vec<websocket::Capability>,
    listener: Option<Arc<dyn CloudSinkListener>>,
    supported_encodings: Vec<String>,
    context: Arc<Context>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    runtime: Option<tokio::runtime::Handle>,
}

impl std::fmt::Debug for CloudSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("session_id", &self.session_id)
            .field("capabilities", &self.capabilities)
            .field("listener", &self.listener.as_ref().map(|_| "..."))
            .field("supported_encodings", &self.supported_encodings)
            .field("context", &self.context)
            .finish()
    }
}

impl Default for CloudSink {
    fn default() -> Self {
        Self {
            session_id: Server::generate_session_id(),
            capabilities: Vec::new(),
            listener: None,
            supported_encodings: Vec::new(),
            context: Context::get_default(),
            channel_filter: None,
            runtime: None,
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
        self.capabilities = vec![websocket::Capability::ClientPublish];
        self.listener = Some(listener);
        self
    }

    /// Configure the set of supported encodings for client requests.
    ///
    /// This is used for both client-side publishing as well as service call request/responses.
    pub fn supported_encodings(
        mut self,
        encodings: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.supported_encodings = encodings.into_iter().map(|e| e.into()).collect();
        self
    }

    /// Set a session ID.
    ///
    /// This allows the client to understand if the connection is a re-connection or if it is
    /// connecting to a new server instance. This can for example be a timestamp or a UUID.
    ///
    /// By default, this is set to the number of milliseconds since the unix epoch.
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = id.into();
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
    /// [`WebSocketServer::start`]), or spawn its own internal runtime (if started with
    /// [`WebSocketServer::start_blocking`]).
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

    /// Sets a channel filter. See [`SinkChannelFilter`] for more information.
    pub fn channel_filter_fn(
        mut self,
        filter: impl Fn(&ChannelDescriptor) -> bool + Sync + Send + 'static,
    ) -> Self {
        self.channel_filter = Some(Arc::new(SinkChannelFilterFn(filter)));
        self
    }

    /// Starts the CloudSink, which maintains a connection in the background.
    ///
    /// Returns a handle that can optionally be used to manage the sink.
    /// The caller can safely drop the handle and the connection will continue in the background.
    /// Use stop() on the returned handle to stop the connection.
    pub async fn start(self) -> Result<CloudSinkHandle, FoxgloveError> {
        let mut server = WebSocketServer::new()
            .session_id(self.session_id)
            .capabilities(self.capabilities)
            .supported_encodings(self.supported_encodings)
            .context(&self.context)
            .tokio_runtime(&self.runtime.unwrap_or_else(get_runtime_handle));
        if let Some(listener) = self.listener {
            server = server.listener(Arc::new(CloudSinkListenerAdapter { listener }));
        }
        let handle = server.start().await?;
        Ok(CloudSinkHandle::new(handle))
    }

    /// Blocking version of [`CloudSink::start`].
    pub fn start_blocking(mut self) -> Result<CloudSinkHandle, FoxgloveError> {
        let runtime = self.runtime.get_or_insert_with(get_runtime_handle).clone();
        let handle = runtime.block_on(self.start())?;
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::websocket::ws_protocol::server::server_info::Capability;
    use crate::websocket::ws_protocol::server::ServerMessage;
    use crate::websocket_client::WebSocketClient;
    use tracing_test::traced_test;

    struct TestListener {}

    impl CloudSinkListener for TestListener {
        fn on_message_data(
            &self,
            _client: Client,
            _client_channel: &ClientChannel,
            _payload: &[u8],
        ) {
        }

        fn on_subscribe(&self, _client: Client, _channel: ChannelView) {}

        fn on_unsubscribe(&self, _client: Client, _channel: ChannelView) {}

        fn on_client_advertise(&self, _client: Client, _channel: &ClientChannel) {}

        fn on_client_unadvertise(&self, _client: Client, _channel: &ClientChannel) {}
    }

    #[traced_test]
    #[tokio::test]
    async fn test_agent_with_client_publish() {
        let ctx = Context::new();
        let cloud_sink = CloudSink::new()
            .listener(Arc::new(TestListener {}))
            .context(&ctx);

        let handle = cloud_sink
            .start()
            .await
            .expect("Failed to start cloud sink");
        let addr = "127.0.0.1:8765";

        let mut client = WebSocketClient::connect(addr)
            .await
            .expect("Failed to connect to agent");

        // Expect to receive ServerInfo message
        let msg = client.recv().await.expect("Failed to receive message");
        match msg {
            ServerMessage::ServerInfo(info) => {
                // Verify the server info contains the ClientPublish capability
                assert!(
                    info.capabilities.contains(&Capability::ClientPublish),
                    "Expected ClientPublish capability"
                );
            }
            _ => panic!("Expected ServerInfo message, got: {msg:?}"),
        }

        let _ = handle.stop();
    }
}
