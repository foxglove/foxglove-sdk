use crate::{ChannelId, RawChannel};

/// Information about a channel.
#[derive(Debug)]
pub struct ChannelView<'a> {
    id: ChannelId,
    topic: &'a str,
}

impl ChannelView<'_> {
    /// Returns the channel ID.
    pub fn id(&self) -> ChannelId {
        self.id
    }

    /// Returns the topic of the channel.
    pub fn topic(&self) -> &str {
        self.topic
    }
}

impl<'a> From<&'a RawChannel> for ChannelView<'a> {
    fn from(value: &'a RawChannel) -> Self {
        Self {
            id: value.id(),
            topic: value.topic(),
        }
    }
}
