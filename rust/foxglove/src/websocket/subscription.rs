use super::ws_protocol::client::subscribe;
use crate::ChannelId;

/// A client subscription with typed IDs.
// REVIEW: Does this struct need to exist?
pub(crate) struct Subscription {
    pub channel_id: ChannelId,
}
impl From<subscribe::Subscription> for Subscription {
    fn from(value: subscribe::Subscription) -> Self {
        Self {
            channel_id: ChannelId::new(value.channel_id),
        }
    }
}
