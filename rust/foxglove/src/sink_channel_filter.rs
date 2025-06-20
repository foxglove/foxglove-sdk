use crate::channel::ChannelDescriptor;

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context.
pub trait SinkChannelFilter: Sync + Send {
    /// Returns true if the channel should be subscribed to.
    fn should_subscribe(&self, channel: &ChannelDescriptor) -> bool;
}

pub(crate) struct SinkChannelFilterFn<F>(pub F)
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send;

impl<F> SinkChannelFilter for SinkChannelFilterFn<F>
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send,
{
    fn should_subscribe(&self, channel: &ChannelDescriptor) -> bool {
        self.0(channel)
    }
}
