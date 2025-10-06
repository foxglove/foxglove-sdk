use std::sync::Arc;

use crate::{
    get_runtime_handle,
    websocket::{Capability, Server, ServerOptions, ShutdownHandle},
    Context, FoxgloveError, WebSocketServer, WebSocketServerHandle,
};

/// A handle to the Agent Remote Connection.
///
/// This handle can safely be dropped and the agent will run forever.
pub struct AgentHandle(WebSocketServerHandle);

impl AgentHandle {
    /// Gracefully shut down the agent.
    ///
    /// Returns a handle that can be used to wait for the graceful shutdown to complete. If the
    /// handle is dropped, all client tasks will be immediately aborted.
    pub fn stop(self) -> ShutdownHandle {
        self.0.stop()
    }

    /// Returns the local port that the agent is listening on.
    #[cfg(test)]
    fn port(&self) -> u16 {
        self.0.port()
    }
}

/// An Agent Remote Connection for live visualization and teleop in Foxglove.
///
/// Must run Foxglove Agent on the same host for this to work.
#[must_use]
#[derive(Debug)]
pub struct Agent {
    options: ServerOptions,
    context: Arc<Context>,
}

impl Default for Agent {
    fn default() -> Self {
        let options = ServerOptions {
            session_id: Some(Server::generate_session_id()),
            ..ServerOptions::default()
        };
        Self {
            options,
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
        self.options.capabilities = Some(capabilities.into_iter().collect());
        self
    }

    /// Configure an event listener to receive client message events.
    pub fn listener(mut self, listener: Arc<dyn crate::websocket::ServerListener>) -> Self {
        self.options.listener = Some(listener);
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

    /// Sets the context for this sink.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Starts the Agent Remote Connection.
    ///
    /// Returns a handle that can optionally be used to gracefully shutdown the agent. The caller
    /// can safely drop the handle, and the agent will run forever.
    pub async fn start(self) -> Result<AgentHandle, FoxgloveError> {
        let handle = WebSocketServer::with_options(self.options)
            .context(&self.context)
            .start()
            .await?;
        Ok(AgentHandle(handle))
    }

    /// Starts the Agent Remote Connection.
    ///
    /// Returns a handle that can optionally be used to gracefully shutdown the agent. The caller
    /// can safely drop the handle, and the agent will run forever.
    ///
    /// If you choose to use this blocking interface rather than [`Agent::start`],
    /// the SDK will spawn a multi-threaded [tokio] runtime.
    ///
    /// This method will panic if invoked from an asynchronous execution context. Use
    /// [`Agent::start`] instead.
    ///
    /// [tokio]: https://docs.rs/tokio/latest/tokio/
    pub fn start_blocking(mut self) -> Result<AgentHandle, FoxgloveError> {
        let runtime = self
            .options
            .runtime
            .get_or_insert_with(get_runtime_handle)
            .clone();
        let handle = runtime.block_on(self.start())?;
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

        let handle = agent.start().await.expect("Failed to start agent");
        let addr = format!("127.0.0.1:{}", handle.port());

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
