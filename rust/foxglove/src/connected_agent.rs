//! Agent sink for interprocess communication with the Foxglove agent.
//!
//! This sink implementation sends messages to a connected agent process,
//! which can handle recording, uploading, and other agent-specific functionality.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use flume::Sender;
use parking_lot::Mutex;
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use crate::{ChannelId, FoxgloveError, Metadata, RawChannel, Sink, SinkId};

use crate::websocket::ws_protocol::server::{ServerMessage, MessageData};
use crate::websocket::ws_protocol::BinaryMessage;
use futures_util::SinkExt;
use tracing::{debug, error};


/// Configuration for the agent sink.
#[derive(Debug, Clone)]
pub struct AgentSinkConfig {
    /// Whether to automatically subscribe to all channels.
    pub auto_subscribe: bool,
    /// Maximum number of messages to buffer before dropping.
    pub message_backlog_size: usize,
    /// Timeout for agent operations.
    pub timeout: Duration,
    /// Path to the Unix domain socket for agent communication.
    pub socket_path: std::path::PathBuf,
}

impl Default for AgentSinkConfig {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            message_backlog_size: 1000,
            timeout: Duration::from_secs(30),
            socket_path: std::path::PathBuf::from("/tmp/foxglove-agent.sock"),
        }
    }
}

/// Internal state for the agent sink.
#[derive(Debug, Default)]
struct AgentSinkState {
    /// Whether the sink is connected to an agent.
    connected: bool,
}

/// Unix socket connection to the agent.
#[derive(Debug)]
pub struct UnixSocketConnection {
    stream: Option<Framed<UnixStream, LengthDelimitedCodec>>,
}

impl UnixSocketConnection {
    pub async fn connect<P: AsRef<Path>>(socket_path: P) -> Result<Self, std::io::Error> {
        let stream = UnixStream::connect(socket_path.as_ref()).await?;
        let framed = Framed::new(stream, LengthDelimitedCodec::new());

        Ok(Self {
            stream: Some(framed),
        })
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }

    /// Sends a protocol message to the agent
    async fn send_message(&mut self, message: &ServerMessage<'_>) -> Result<(), std::io::Error> {
        let stream = self.stream.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotConnected, "No active connection")
        })?;

        // Convert to bytes based on message type
        let bytes = match message {
            ServerMessage::MessageData(msg) => msg.to_bytes(),
            ServerMessage::Advertise(msg) => {
                // For JSON messages, we need to serialize to string first
                let json = serde_json::to_string(msg).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                json.into_bytes()
            }
            _ => {
                // For other message types, we'll handle them as needed
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Message type not yet implemented"
                ));
            }
        };

        stream.send(bytes.into()).await?;
        Ok(())
    }
}

/// A poller for the connected agent.
///
/// The poller is responsible for:
/// - Sending messages (from `data_plane` and `control_plane`) to the agent via IPC.
/// - Managing the Unix socket connection.
/// - Waiting for a shutdown signal, and closing the connection.
#[derive(Debug)]
struct Poller {
    connection: Option<UnixSocketConnection>,
    data_plane_rx: flume::Receiver<ServerMessage<'static>>,
    control_plane_rx: flume::Receiver<ServerMessage<'static>>,
    shutdown_rx: tokio::sync::oneshot::Receiver<ShutdownReason>,
}

/// A reason for shutting down the agent connection.
#[derive(Debug, Clone, Copy)]
enum ShutdownReason {
    /// The agent disconnected.
    AgentDisconnected,
    /// The sink has been stopped.
    SinkStopped,
    /// The control plane queue overflowed.
    ControlPlaneQueueFull,
}

impl Poller {
    /// Creates a new poller.
    fn new(
        connection: UnixSocketConnection,
        data_plane_rx: flume::Receiver<ServerMessage<'static>>,
        control_plane_rx: flume::Receiver<ServerMessage<'static>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<ShutdownReason>,
    ) -> Self {
        Self {
            connection: Some(connection),
            data_plane_rx,
            control_plane_rx,
            shutdown_rx,
        }
    }

    /// Runs the main poll loop for the agent connection.
    async fn run(mut self) {
        debug!("Poller::run starting");

        // Send messages from queues to the agent via IPC
        let send_loop = async {
            debug!("Poller send_loop starting");
            while let Ok(message) = tokio::select! {
                msg = self.control_plane_rx.recv_async() => msg,
                msg = self.data_plane_rx.recv_async() => msg,
            } {
                debug!("Poller received message from queue: {:?}", message);
                if let Some(ref mut connection) = self.connection {
                    debug!("Poller sending message via IPC connection");
                    match connection.send_message(&message).await {
                        Ok(_) => {
                            debug!("Poller: sent message via IPC: {:?}", message);
                        }
                        Err(e) => {
                            error!("Error sending message via IPC: {}", e);
                            // TODO: Handle connection errors (reconnect, etc.)
                            break;
                        }
                    }
                } else {
                    debug!("No active connection, dropping message: {:?}", message);
                    break;
                }
            }
            debug!("Poller send_loop ending");
            ShutdownReason::AgentDisconnected
        };

        let reason = tokio::select! {
            _ = send_loop => ShutdownReason::AgentDisconnected,
            r = self.shutdown_rx => r.expect("ConnectedAgent sends before dropping sender"),
        };

        debug!("Poller shutting down: {:?}", reason);
    }
}

/// A sink that sends messages to a connected Foxglove agent.
///
/// This sink acts as a bridge between the Foxglove SDK and the agent process,
/// allowing the agent to receive messages for recording, uploading, or other
/// processing.
#[derive(Debug)]
pub struct ConnectedAgent {
    sink_id: SinkId,
    config: AgentSinkConfig,
    state: Mutex<AgentSinkState>,
    closed: Mutex<bool>,
    poller: parking_lot::Mutex<Option<Poller>>,
    data_plane_tx: parking_lot::Mutex<Option<Sender<ServerMessage<'static>>>>,
    control_plane_tx: parking_lot::Mutex<Option<Sender<ServerMessage<'static>>>>,
    shutdown_tx: parking_lot::Mutex<Option<tokio::sync::oneshot::Sender<ShutdownReason>>>,
}

impl ConnectedAgent {
    /// Creates a new agent sink with an established connection.
    pub fn new(
        config: AgentSinkConfig,
        connection: UnixSocketConnection,
    ) -> Arc<Self> {
        let socket_path = config.socket_path.clone();
        let (data_plane_tx, data_plane_rx) = flume::bounded(config.message_backlog_size);
        let (control_plane_tx, control_plane_rx) = flume::bounded(config.message_backlog_size);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let poller = Poller::new(connection, data_plane_rx, control_plane_rx, shutdown_rx);

        let agent = Arc::new(Self {
            sink_id: SinkId::next(),
            config,
            state: Mutex::new(AgentSinkState { connected: true }),
            closed: Mutex::new(false),
            poller: parking_lot::Mutex::new(Some(poller)),
            data_plane_tx: parking_lot::Mutex::new(Some(data_plane_tx)),
            control_plane_tx: parking_lot::Mutex::new(Some(control_plane_tx)),
            shutdown_tx: parking_lot::Mutex::new(Some(shutdown_tx)),
        });

        debug!("Agent sink: created with connection to agent at {}", socket_path.display());
        agent
    }

    /// Send the message on the data plane, dropping up to retries older messages to make room, if necessary.
    fn send_data_lossy(&self, message: ServerMessage<'static>, _retries: usize) -> bool {
        debug!("send_data_lossy called with message: {:?}", message);

        // TODO: Implement lossy sending like ConnectedClient
        // For now, just try to send and return success/failure
        if let Some(ref tx) = *self.data_plane_tx.lock() {
            match tx.try_send(message) {
                Ok(_) => {
                    debug!("Message successfully sent to data plane queue");
                    true
                }
                Err(_) => {
                    debug!("Data plane queue full, dropping message");
                    false
                }
            }
        } else {
            debug!("No data plane sender available, dropping message");
            false
        }
    }

    /// Send the message on the control plane, disconnecting if the channel is full.
    fn send_control_msg(&self, message: ServerMessage<'static>) -> bool {
        debug!("send_control_msg called with message: {:?}", message);

        if let Some(ref tx) = *self.control_plane_tx.lock() {
            match tx.try_send(message) {
                Ok(_) => {
                    debug!("Message successfully sent to control plane queue");
                    true
                }
                Err(_) => {
                    debug!("Control plane queue full, this should trigger disconnect");
                    // TODO: Implement proper disconnect logic
                    false
                }
            }
        } else {
            debug!("No control plane sender available, dropping message");
            false
        }
    }



    /// Runs the agent's poll loop to completion.
    ///
    /// The poll loop may exit either due to the agent closing the connection, or due to an
    /// internal call to [`ConnectedAgent::shutdown`].
    ///
    /// Panics if called more than once.
    pub async fn run(&self) {
        let poller = self.poller.lock().take().expect("only call run once");
        poller.run().await;
    }

    /// Shuts down the connection by signalling the [`Poller`] to exit.
    pub fn shutdown(&self, reason: ShutdownReason) {
        if let Some(shutdown_tx) = self.shutdown_tx.lock().take() {
            shutdown_tx.send(reason).ok();
        }
    }

    /// Disconnects from the agent.
    pub fn disconnect(&self) {
        let mut state = self.state.lock();
        if state.connected {
            debug!("Agent sink: disconnecting from agent");
            state.connected = false;
        }

        // Signal shutdown
        self.shutdown(ShutdownReason::SinkStopped);
    }
}

impl Sink for ConnectedAgent {
    fn id(&self) -> SinkId {
        self.sink_id
    }

    fn log(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        debug!("ConnectedAgent::log called for channel {} with {} bytes", channel.topic(), msg.len());

        // Create MessageData and send via IPC
        // We need to track channel subscriptions to get the subscription_id
        // For now, use a placeholder subscription_id
        let subscription_id = 1; // TODO: Get actual subscription_id from channel tracking
        let message = ServerMessage::MessageData(MessageData::new(subscription_id, metadata.log_time, msg));

        debug!("Created MessageData message: {:?}", message);

        // Send message data on the data plane (lossy)
        let sent = self.send_data_lossy(message.into_owned(), 10); // Use 10 retries like ConnectedClient
        debug!("Message sent to data plane: {}", sent);

        Ok(())
    }

    fn add_channels(&self, channels: &[&Arc<RawChannel>]) -> Option<Vec<ChannelId>> {
        use crate::websocket::advertise;

        debug!("ConnectedAgent::add_channels called with {} channels", channels.len());

        let message = advertise::advertise_channels(channels.iter().copied());
        if message.channels.is_empty() {
            debug!("No channels to advertise");
            return None;
        }

        debug!("Created Advertise message: {:?}", message);

        // Send the advertisement message to the agent via IPC
        let sent = self.send_control_msg(ServerMessage::Advertise(message).into_owned());
        debug!("Advertise message sent to control plane: {}", sent);

        // Return channel IDs if we want to subscribe immediately
        if self.config.auto_subscribe {
            Some(channels.iter().map(|c| c.id()).collect())
        } else {
            None
        }
    }

    fn remove_channel(&self, channel: &RawChannel) {
        use crate::websocket::ws_protocol::server::Unadvertise;
        let message = Unadvertise::new([channel.id().into()]);
        self.send_control_msg(ServerMessage::Unadvertise(message));
    }

    fn auto_subscribe(&self) -> bool {
        self.config.auto_subscribe
    }
}

impl Drop for ConnectedAgent {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelBuilder, Context, Metadata, Schema};

    fn create_test_channel(ctx: &Arc<Context>, topic: &str) -> Arc<RawChannel> {
        ChannelBuilder::new(topic)
            .context(ctx)
            .message_encoding("json")
            .schema(Schema::new(
                "test_schema",
                "jsonschema",
                br#"{"type": "object", "properties": {"msg": {"type": "string"}}}"#,
            ))
            .build_raw()
            .unwrap()
    }

    #[test]
    fn test_agent_sink_with_config() {
        let config = AgentSinkConfig {
            auto_subscribe: false,
            message_backlog_size: 500,
            timeout: Duration::from_secs(60),
            socket_path: std::path::PathBuf::from("/tmp/test-agent.sock"),
        };
        let sink = ConnectedAgent::with_config(config);
        assert_eq!(sink.config.auto_subscribe, false);
        assert_eq!(sink.config.message_backlog_size, 500);
        assert_eq!(sink.config.timeout, Duration::from_secs(60));
        assert_eq!(sink.config.socket_path, std::path::PathBuf::from("/tmp/test-agent.sock"));
    }

    #[tokio::test]
    async fn test_agent_sink_connection() {
        let sink = ConnectedAgent::new();

        // Should fail to connect since no agent is running
        // This is expected behavior for now
        let result = sink.connect().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_sink_logging() {
        let ctx = Context::new();
        let sink = ConnectedAgent::new();
        let channel = create_test_channel(&ctx, "/test_topic");

        // Add sink to context
        ctx.add_sink(sink.clone());

        // Log a message
        let msg = b"test message";
        let metadata = Metadata { log_time: 123456789 };

        // Should succeed (currently just logs)
        assert!(sink.log(&channel, msg, &metadata).is_ok());
    }

    #[test]
    fn test_agent_sink_auto_subscribe() {
        let sink = ConnectedAgent::new();
        assert!(!sink.auto_subscribe());

        let config = AgentSinkConfig {
            auto_subscribe: true,
            ..Default::default()
        };
        let sink = ConnectedAgent::with_config(config);
        assert!(sink.auto_subscribe());
    }
}
