use crate::ChannelDescriptor;

use super::client::Client;

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait Listener: Send + Sync {
    /// Callback invoked when a client message is received.
    fn on_message_data(&self, _client: Client, _channel: &ChannelDescriptor, _payload: &[u8]) {}
    /// Callback invoked when a client subscribes to a channel.
    fn on_subscribe(&self, _client: Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    fn on_unsubscribe(&self, _client: Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client advertises a client channel.
    fn on_client_advertise(&self, _client: Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client unadvertises a client channel.
    fn on_client_unadvertise(&self, _client: Client, _channel: &ChannelDescriptor) {}
}
