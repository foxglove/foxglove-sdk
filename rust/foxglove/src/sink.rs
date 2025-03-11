use crate::channel::Channel;
use crate::metadata::Metadata;
use crate::FoxgloveError;
use std::sync::Arc;

/// A [`Sink`] writes a message from a [`Channel`] to a destination.
///
/// Sinks are thread-safe and can be shared between threads. Usually you'd use our implementations
/// like [`McapWriter`](crate::McapWriter) or [`WebSocketServer`](crate::WebSocketServer).
pub trait Sink: Send + Sync {
    /// Writes the message for the channel to the sink.
    ///
    /// Metadata contains optional message metadata that may be used by some sink implementations.
    fn log(&self, channel: &Channel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError>;

    /// Called when a new channel is made available within the [`Namespace`][crate::Namespace].
    ///
    /// Sinks can track channels seen, and do new channel-related things the first time they see a
    /// channel, rather than in this method. The choice is up to the implementor.
    ///
    /// When the sink is first registered with a namespace, this callback is automatically invoked
    /// with each of the channels registered to that namespace.
    fn add_channel(&self, _channel: &Arc<Channel>) {}

    /// Called when a channel is unregistered from the [`Namespacde`][crate::Namespace].
    ///
    /// Sinks can clean up any channel-related state they have or take other actions.
    fn remove_channel(&self, _channel: &Channel) {}
}
