use std::collections::BTreeMap;

use crate::{ChannelId, RawChannel};

/// Information about a channel, which is passed to a [`SinkChannelFilter`].
pub trait FilterableChannel {
    /// The ID of the channel.
    fn id(&self) -> ChannelId;

    /// The channel's topic.
    fn topic(&self) -> &str;

    /// The channel's metadata. Empty if it was not set during construction.
    fn metadata(&self) -> &BTreeMap<String, String>;
}

impl FilterableChannel for RawChannel {
    fn id(&self) -> ChannelId {
        self.id()
    }
    fn topic(&self) -> &str {
        self.topic()
    }
    fn metadata(&self) -> &BTreeMap<String, String> {
        self.metadata()
    }
}

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context.
pub trait SinkChannelFilter: Sync + Send {
    /// Returns true if the channel should be subscribed to.
    fn should_subscribe(&self, channel: &dyn FilterableChannel) -> bool;
}
