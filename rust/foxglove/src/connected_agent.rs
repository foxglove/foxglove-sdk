//! Agent sink for interprocess communication with the Foxglove agent.
//!
//! This sink implementation sends messages to a connected agent process,
//! which can handle recording, uploading, and other agent-specific functionality.

use std::sync::Arc;

use std::time::Duration;

use parking_lot::Mutex;
use tracing::{debug, warn};

use crate::{ChannelId, FoxgloveError, Metadata, RawChannel, Sink, SinkId};

/// Configuration for the agent sink.
#[derive(Debug, Clone)]
pub struct AgentSinkConfig {
    /// Whether to automatically subscribe to all channels.
    pub auto_subscribe: bool,
    /// Maximum number of messages to buffer before dropping.
    pub message_backlog_size: usize,
    /// Timeout for agent operations.
    pub timeout: Duration,
}

impl Default for AgentSinkConfig {
    fn default() -> Self {
        Self {
            auto_subscribe: false,
            message_backlog_size: 1000,
            timeout: Duration::from_secs(30),
        }
    }
}

/// Internal state for the agent sink.
#[derive(Debug, Default)]
struct AgentSinkState {
    /// Whether the sink is connected to an agent.
    connected: bool,
}

/// A sink that sends messages to a connected Foxglove agent.
///
/// This sink acts as a bridge between the Foxglove SDK and the agent process,
/// allowing the agent to receive messages for recording, uploading, or other
/// processing.
///
/// Currently, this is a shim implementation that logs messages but doesn't
/// actually communicate with an agent. In the future, this will be extended
/// to support IPC communication.
#[derive(Debug)]
pub struct ConnectedAgent {
    sink_id: SinkId,
    config: AgentSinkConfig,
    state: Mutex<AgentSinkState>,
    closed: Mutex<bool>,
}

impl ConnectedAgent {
    /// Creates a new agent sink with default configuration.
    pub fn new() -> Arc<Self> {
        Self::with_config(AgentSinkConfig::default())
    }

    /// Creates a new agent sink with the specified configuration.
    pub fn with_config(config: AgentSinkConfig) -> Arc<Self> {
        Arc::new(Self {
            sink_id: SinkId::next(),
            config,
            state: Mutex::new(AgentSinkState::default()),
            closed: Mutex::new(false),
        })
    }

    /// Attempts to connect to the agent.
    ///
    /// This is currently a no-op but will be implemented to establish
    /// IPC communication with the agent process.
    pub fn connect(&self) -> Result<(), FoxgloveError> {
        if *self.closed.lock() {
            return Err(FoxgloveError::SinkClosed);
        }

        let mut state = self.state.lock();
        if state.connected {
            return Ok(());
        }

        // TODO: Implement actual connection logic
        // For now, just simulate a successful connection
        debug!("Agent sink: simulating connection to agent");
        state.connected = true;
        Ok(())
    }

    /// Disconnects from the agent.
    pub fn disconnect(&self) {
        let mut state = self.state.lock();
        if state.connected {
            debug!("Agent sink: disconnecting from agent");
            state.connected = false;
        }
    }

    /// Sends a message to the agent.
    ///
    /// This is currently a no-op but will be implemented to send
    /// messages via IPC to the agent process.
    fn send_message(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        if *self.closed.lock() {
            return Err(FoxgloveError::SinkClosed);
        }

        // Ensure we're connected
        if !self.state.lock().connected {
            if let Err(e) = self.connect() {
                warn!("Agent sink: failed to connect: {}", e);
                return Err(e);
            }
        }

        // TODO: Implement actual message sending via IPC
        // For now, just log the message
        debug!(
            "Agent sink: would send message to agent - topic: {}, size: {} bytes, time: {}",
            channel.topic(),
            msg.len(),
            metadata.log_time
        );

        Ok(())
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
        self.send_message(channel, msg, metadata)
    }

    fn add_channels(&self, channels: &[&Arc<RawChannel>]) -> Option<Vec<ChannelId>> {
        // TODO: Implement channel advertisement to agent
        debug!(
            "Agent sink: would advertise {} channels to agent",
            channels.len()
        );

        for channel in channels {
            debug!(
                "Agent sink: would advertise channel - topic: {}, encoding: {}",
                channel.topic(),
                channel.message_encoding()
            );
        }

        // Return channel IDs if we want to subscribe immediately
        if self.config.auto_subscribe {
            Some(channels.iter().map(|c| c.id()).collect())
        } else {
            None
        }
    }

    fn remove_channel(&self, channel: &RawChannel) {
        // TODO: Implement channel removal notification to agent
        debug!(
            "Agent sink: would notify agent of channel removal - topic: {}",
            channel.topic()
        );
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
        };
        let sink = ConnectedAgent::with_config(config);
        assert_eq!(sink.config.auto_subscribe, false);
        assert_eq!(sink.config.message_backlog_size, 500);
        assert_eq!(sink.config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_agent_sink_connection() {
        let sink = ConnectedAgent::new();

        // Should connect successfully
        assert!(sink.connect().is_ok());
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
