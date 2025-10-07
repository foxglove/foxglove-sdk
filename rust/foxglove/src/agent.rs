use std::sync::Arc;

use flume::{Receiver, Sender};
use tracing::error;

use crate::{
    websocket::{Capability, Server, ShutdownHandle},
    Context, FoxgloveError, WebSocketServer, WebSocketServerHandle,
};

/// A handle to the Agent Remote Connection.
///
/// This handle can safely be dropped and the agent will run forever.
#[doc(hidden)]
pub struct AgentHandle {
    options: Agent,
    server: Option<WebSocketServerHandle>,
    connecting: bool,
    connect_rx: Receiver<Result<WebSocketServerHandle, FoxgloveError>>,
    connect_tx: Sender<Result<WebSocketServerHandle, FoxgloveError>>,
}

impl AgentHandle {
    fn new(options: Agent) -> Self {
        let (connect_tx, connect_rx) = flume::bounded(1);
        Self {
            options,
            server: None,
            connecting: false,
            connect_rx,
            connect_tx,
        }
    }

    fn start_connection(&mut self) {
        if self.server.is_some() {
            // Connected
            return;
        }
        if !self.connecting {
            self.connecting = true;
            // Create a new "connection". Currently this starts the server.
            let options = self.options.clone();
            let sender = self.connect_tx.clone();
            tokio::spawn(async move {
                let mut builder = WebSocketServer::new()
                    .session_id(options.session_id)
                    .capabilities(options.capabilities)
                    .message_backlog_size(options.message_backlog_size)
                    .supported_encodings(options.supported_encodings)
                    .context(&options.context);
                if let Some(listener) = options.listener {
                    builder = builder.listener(listener);
                }
                let result = builder.start().await;
                if let Err(e) = sender.send_async(result).await {
                    error!("Failed to send connection result: {e:?}");
                }
            });
        }
    }

    /// Ensures that the Foxglove Agent is connected.
    /// If connected this returns Ok immediately.
    /// Otherwise connects and blocks until the connection is established or fails and returns the result.
    ///
    /// Note: currently this starts a server for Agent to connect to, and blocks until it's started.
    /// There isn't a way to wait until the Agent has connected to that server.
    /// This behavior will change soon.
    pub async fn ensure_connected(&mut self) -> Result<(), FoxgloveError> {
        // This code is a copy of the code in ensure_connected_blocking, if you change one, change both.
        if self.server.is_some() {
            // Connected
            return Ok(());
        }
        self.start_connection();
        // Wait for the connection
        let result = self
            .connect_rx
            .recv_async()
            .await
            .map_err(|e| FoxgloveError::Unspecified(Box::new(e)));
        self.connecting = false;
        self.server = Some(result??);
        Ok(())
    }

    /// Blocking version of [`AgentHandle::ensure_connected`].
    pub fn ensure_connected_blocking(&mut self) -> Result<(), FoxgloveError> {
        // This code is a copy of the code in ensure_connected, if you change one, change both.
        if self.server.is_some() {
            // Connected
            return Ok(());
        }
        self.start_connection();
        // Wait for the connection
        let result = self
            .connect_rx
            .recv()
            .map_err(|e| FoxgloveError::Unspecified(Box::new(e)));
        self.connecting = false;
        self.server = Some(result??);
        Ok(())
    }

    /// Gracefully shut down the agent, if connected. Otherwise returns None.
    ///
    /// Returns a handle that can be used to wait for the graceful shutdown to complete. If the
    /// handle is dropped, all client tasks will be immediately aborted.
    pub fn stop(self) -> Option<ShutdownHandle> {
        if let Some(handle) = self.server {
            return Some(handle.stop());
        }
        None
    }
}

/// An Agent Remote Connection for live visualization and teleop in Foxglove.
///
/// Must run Foxglove Agent on the same host for this to work.
#[must_use]
#[derive(Clone)]
#[doc(hidden)]
pub struct Agent {
    session_id: String,
    capabilities: Vec<Capability>,
    listener: Option<Arc<dyn crate::websocket::ServerListener>>,
    message_backlog_size: usize,
    supported_encodings: Vec<String>,
    context: Arc<Context>,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("session_id", &self.session_id)
            .field("capabilities", &self.capabilities)
            .field("listener", &self.listener.as_ref().map(|_| "..."))
            .field("message_backlog_size", &self.message_backlog_size)
            .field("supported_encodings", &self.supported_encodings)
            .field("context", &self.context)
            .finish()
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            session_id: Server::generate_session_id(),
            capabilities: Vec::new(),
            listener: None,
            message_backlog_size: 1024,
            supported_encodings: Vec::new(),
            context: Context::get_default(),
        }
    }
}

impl Agent {
    /// Creates a new websocket server with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the server capabilities to advertise to the client.
    ///
    /// By default, the server does not advertise any capabilities.
    pub fn capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn crate::websocket::ServerListener>) -> Self {
        self.listener = Some(listener);
        self
    }

    /// Set the message backlog size.
    ///
    /// The server buffers outgoing log entries into a queue. If the backlog size is exceeded, the
    /// oldest entries will be dropped.
    ///
    /// By default, the server will buffer 1024 messages.
    pub fn message_backlog_size(mut self, size: usize) -> Self {
        self.message_backlog_size = size;
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

    /// Starts the Agent Remote Connection.
    ///
    /// Returns a handle that can optionally be used to manage the connection.
    /// The caller can safely drop the handle, and the agent will reconnect automatically, forever.
    pub fn create(self) -> Result<AgentHandle, FoxgloveError> {
        let mut handle = AgentHandle::new(self);
        handle.start_connection();
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::websocket::ws_protocol::server::server_info::Capability as ProtocolCapability;
    use crate::websocket::ws_protocol::server::ServerMessage;
    use crate::websocket_client::WebSocketClient;
    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn test_agent_with_client_publish() {
        let ctx = Context::new();
        let agent = Agent::new()
            .capabilities([Capability::ClientPublish])
            .context(&ctx);

        let mut handle = agent.create().expect("Failed to create agent");
        handle.ensure_connected().await.expect("Failed to connect");
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
                    info.capabilities
                        .contains(&ProtocolCapability::ClientPublish),
                    "Expected ClientPublish capability"
                );
            }
            _ => panic!("Expected ServerInfo message, got: {msg:?}"),
        }

        let _ = handle.stop();
    }
}
