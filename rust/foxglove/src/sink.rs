use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use smallvec::SmallVec;

use crate::channel::Channel;
use crate::metadata::Metadata;
use crate::FoxgloveError;

/// Uniquely identifies a [`Sink`] in the context of this program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SinkId(u64);
impl SinkId {
    /// Allocates the next sink ID.
    pub fn next() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Self(id)
    }
}

/// A [`Sink`] writes a message from a [`Channel`] to a destination.
///
/// Sinks are thread-safe and can be shared between threads. Usually you'd use our implementations
/// like [`McapWriter`](crate::McapWriter) or [`WebSocketServer`](crate::WebSocketServer).
pub trait Sink: Send + Sync {
    /// Returns the sink's unique ID.
    fn id(&self) -> SinkId;

    /// Writes the message for the channel to the sink.
    ///
    /// Metadata contains optional message metadata that may be used by some sink implementations.
    fn log(&self, channel: &Channel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError>;

    /// Called when a new channel is made available within the [`Context`][crate::Context].
    ///
    /// Sinks can track channels seen, and do new channel-related things the first time they see a
    /// channel, rather than in this method. The choice is up to the implementor.
    ///
    /// When the sink is first registered with a context, this callback is automatically invoked
    /// with each of the channels registered to that context.
    fn add_channel(&self, _channel: &Arc<Channel>) {}

    /// Called when a channel is unregistered from the [`Context`][crate::Context].
    ///
    /// Sinks can clean up any channel-related state they have or take other actions.
    fn remove_channel(&self, _channel: &Channel) {}

    /// Indicates whether this sink automatically subscribes to all channels.
    ///
    /// The default implementation returns true.
    ///
    /// A sink implementation may return false to indicate that it intends to manage its
    /// subscriptions dynamically using
    /// [`Context::subscribe_channels`][crate::Context::subscribe_channels] and
    /// [`Context::unsubscribe_channels`][crate::Context::unsubscribe_channels].
    fn auto_subscribe(&self) -> bool {
        true
    }
}

/// A small group of sinks.
///
/// We use a [`SmallVec`] to improve cache locality and reduce heap allocations when working with a
/// small number of sinks, which is typically the case.
pub(crate) type SmallSinkVec = SmallVec<[Arc<dyn Sink>; 6]>;
