use super::ws_protocol::client::subscribe;
use crate::ChannelId;

/// A subscription ID.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct SubscriptionId(u32);

impl SubscriptionId {
    /// Creates a new subscription ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl From<SubscriptionId> for u32 {
    fn from(id: SubscriptionId) -> u32 {
        id.0
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A client subscription with typed IDs.
pub(crate) struct Subscription {
    pub id: SubscriptionId,
    pub channel_id: ChannelId,
}
impl From<subscribe::Subscription> for Subscription {
    fn from(value: subscribe::Subscription) -> Self {
        Self {
            id: SubscriptionId::new(value.id),
            channel_id: ChannelId::new(value.channel_id),
        }
    }
}
