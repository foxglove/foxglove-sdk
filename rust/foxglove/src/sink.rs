use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use smallvec::SmallVec;

use crate::metadata::Metadata;
use crate::{ChannelId, FoxgloveError, RawChannel};

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
impl std::fmt::Display for SinkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A [`Sink`] writes a message from a channel to a destination.
///
/// Sinks are thread-safe and can be shared between threads. Usually you'd use our implementations
/// like [`McapWriter`](crate::McapWriter) or [`WebSocketServer`](crate::WebSocketServer).
#[doc(hidden)]
pub trait Sink: Send + Sync {
    /// Returns the sink's unique ID.
    fn id(&self) -> SinkId;

    /// Writes the message for the channel to the sink.
    ///
    /// Metadata contains optional message metadata that may be used by some sink implementations.
    fn log(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError>;

    /// Called when new channels are made available within the [`Context`][ctx].
    ///
    /// Sinks can track channels seen, and do new channel-related things the first time they see a
    /// channel, rather than in this method. The choice is up to the implementor.
    ///
    /// When the sink is first registered with a context, this callback is automatically invoked
    /// with each of the channels registered to that context.
    ///
    /// For sinks that manage their channel subscriptions dynamically, note that it is NOT safe to
    /// call [`Context::subscribe_channels`][sub] from the context of this callback. If the sink
    /// wants to subscribe to channels immediately, it may return a list of corresponding channel
    /// IDs.
    ///
    /// For sinks that [auto-subscribe][Sink::auto_subscribe] to all channels, the return value of
    /// this method is ignored.
    ///
    /// [ctx]: crate::Context
    /// [sub]: crate::Context::subscribe_channels
    fn add_channels(&self, _channel: &[&Arc<RawChannel>]) -> Option<Vec<ChannelId>> {
        None
    }

    /// Called when a new channel is made available within the [`Context`][crate::Context].
    ///
    /// See [`Sink::add_channels`] for additional details.
    ///
    /// For sinks that manage their channel subscriptions dynamically, this function may return
    /// true to immediately subscribe to the channel.
    #[doc(hidden)]
    fn add_channel(&self, channel: &Arc<RawChannel>) -> bool {
        self.add_channels(&[channel])
            .is_some_and(|ids| ids.contains(&channel.id()))
    }

    /// Called when a channel is unregistered from the [`Context`][ctx].
    ///
    /// Sinks can clean up any channel-related state they have or take other actions.
    ///
    /// For sinks that manage their channel subscriptions dynamically, it is not necessary to call
    /// [`Context::unsubscribe_channels`][unsub] for this sink; subscriptions for a channel are
    /// automatically removed when that channel is removed.
    ///
    /// [ctx]: crate::Context
    /// [unsub]: crate::Context::unsubscribe_channels
    fn remove_channel(&self, _channel: &RawChannel) {}

    /// Indicates whether this sink automatically subscribes to all channels.
    ///
    /// The default implementation returns true.
    ///
    /// A sink implementation may return false to indicate that it intends to manage its
    /// subscriptions dynamically using [`Sink::add_channel`],
    /// [`Context::subscribe_channels`][sub], and [`Context::unsubscribe_channels`][unsub].
    ///
    /// [sub]: crate::Context::subscribe_channels
    /// [unsub]: crate::Context::unsubscribe_channels
    fn auto_subscribe(&self) -> bool {
        true
    }
}

/// A small group of sinks.
///
/// We use a [`SmallVec`] to improve cache locality and reduce heap allocations when working with a
/// small number of sinks, which is typically the case.
pub(crate) type SmallSinkVec = SmallVec<[Arc<dyn Sink>; 6]>;
