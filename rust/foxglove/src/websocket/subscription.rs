use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::sync::Weak;

use crate::SinkId;
use crate::{channel::ChannelId, Context};

use super::ClientId;

#[cfg(test)]
mod tests;

/// An aggregator for client subscriptions.
pub(crate) struct SubscriptionAggregator {
    context: Weak<Context>,
    sink_id: SinkId,
    client: parking_lot::Mutex<HashMap<ChannelId, HashSet<ClientId>>>,
}

impl SubscriptionAggregator {
    /// Creates a new subscription aggregator.
    pub fn new(context: Weak<Context>, sink_id: SinkId) -> Self {
        Self {
            context,
            sink_id,
            client: parking_lot::Mutex::default(),
        }
    }

    /// Subscribes the client to the provided channels.
    ///
    /// For any channel that didn't have any previous client subscribers, this method propagates
    /// the subscribe request to the context.
    pub fn subscribe_channels(
        &self,
        client_id: ClientId,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) {
        let mut client_subs = self.client.lock();
        let mut new_channel_ids = vec![];
        for channel_id in channel_ids {
            match client_subs.entry(channel_id) {
                Entry::Vacant(e) => {
                    new_channel_ids.push(channel_id);
                    e.insert(HashSet::from_iter([client_id]));
                }
                Entry::Occupied(mut e) => {
                    let client_ids = e.get_mut();
                    #[cfg(debug_assertions)]
                    assert!(
                        !client_ids.is_empty(),
                        "empty sets are removed by unsubscribe"
                    );
                    client_ids.insert(client_id);
                }
            }
        }
        if !new_channel_ids.is_empty() {
            if let Some(ctx) = self.context.upgrade() {
                ctx.subscribe_channels(self.sink_id, new_channel_ids);
            }
        }
    }

    /// Unsubscribes the client from the provided channels.
    ///
    /// For any channel that no longer has client subscribers, this method propagates the
    /// unsubscribe request to the context.
    pub fn unsubscribe_channels(
        &self,
        client_id: ClientId,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) {
        let mut client_subs = self.client.lock();
        let mut old_channel_ids = vec![];
        for channel_id in channel_ids {
            if let Some(e) = client_subs.get_mut(&channel_id) {
                if e.remove(&client_id) && e.is_empty() {
                    client_subs.remove(&channel_id);
                    old_channel_ids.push(channel_id);
                }
            }
        }
        if !old_channel_ids.is_empty() {
            if let Some(ctx) = self.context.upgrade() {
                ctx.unsubscribe_channels(self.sink_id, old_channel_ids);
            }
        }
    }
}
