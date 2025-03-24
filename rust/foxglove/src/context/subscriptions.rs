//! Sink subscriptions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::sink::SmallSinkVec;
use crate::{ChannelId, Sink, SinkId};

#[cfg(test)]
mod tests;

/// A collection of sink subscriptions on channels.
///
/// A sink may either statically subscribe to all channels, or it may dynamically subscribe to
/// particular channels. A particular sink's disposition is indicated by [`Sink::auto_subscribe`].
///
/// When logging a message to a channel, we need to take the union of (a) global subscribers and
/// (b) per-channel subscribers. This collection ensures that (a) and (b) are always disjoint sets,
/// by ensuring that a subscriber is never a member of both `global` and `by_channel` maps
/// simultaneously.
#[derive(Default)]
pub(crate) struct Subscriptions {
    /// Global subscriptions (all channels).
    global: HashMap<SinkId, Arc<dyn Sink>>,
    /// Per-channel subscriptions.
    by_channel: HashMap<ChannelId, HashMap<SinkId, Arc<dyn Sink>>>,
}

impl Subscriptions {
    /// Removes all subscriptions.
    pub fn clear(&mut self) {
        self.global.clear();
        self.by_channel.clear();
    }

    /// Adds a global subscription to all channels.
    ///
    /// Removes any existing per-channel subscriptions for this subscriber.
    ///
    /// Returns false if the subscribe already has a global subscription.
    pub fn subscribe_global(&mut self, sink: Arc<dyn Sink>) -> bool {
        let sink_id = sink.id();
        if self.global.insert(sink_id, sink).is_none() {
            self.by_channel.retain(|_, subs| {
                subs.remove(&sink_id);
                !subs.is_empty()
            });
            true
        } else {
            false
        }
    }

    /// Adds subscriptions to the specified channels.
    ///
    /// Has no effect if the subscriber has a global subscription.
    ///
    /// Returns false if the subscriber already has subscriptions to all of the channels, has a
    /// global subscription, or the list of channels is empty.
    pub fn subscribe_channels(&mut self, sink: &Arc<dyn Sink>, channel_ids: &[ChannelId]) -> bool {
        let sink_id = sink.id();
        if self.global.contains_key(&sink_id) {
            return false;
        }
        let mut inserted = false;
        for &channel_id in channel_ids {
            inserted |= self
                .by_channel
                .entry(channel_id)
                .or_default()
                .insert(sink_id, sink.clone())
                .is_none();
        }
        inserted
    }

    /// Removes subscriptions to the specified channels.
    ///
    /// Has no effect if the subscriber has a global subscription.
    ///
    /// Returns false if the subscriber is not subscribed to any of the channels, has a global
    /// subscription, or if the list of channels is empty.
    pub fn unsubscribe_channels(&mut self, sink_id: SinkId, channel_ids: &[ChannelId]) -> bool {
        let mut removed = false;
        for &channel_id in channel_ids {
            if let Some(subs) = self.by_channel.get_mut(&channel_id) {
                if subs.remove(&sink_id).is_some() {
                    removed = true;
                    if subs.is_empty() {
                        self.by_channel.remove(&channel_id);
                    }
                }
            }
        }
        removed
    }

    /// Remove all per-channel subscriptions for the specified channel.
    ///
    /// Returns false if there were no per-channel subscriptions for the channel.
    pub fn remove_channel_subscriptions(&mut self, channel_id: ChannelId) -> bool {
        self.by_channel.remove(&channel_id).is_some()
    }

    /// Remove all global and per-channel subscriptions for a particular subscriber.
    ///
    /// Returns false if the subscriber did not have any subscriptions.
    pub fn remove_subscriber(&mut self, sink_id: SinkId) -> bool {
        if self.global.remove(&sink_id).is_some() {
            true
        } else {
            let mut removed = false;
            self.by_channel.retain(|_, subs| {
                removed |= subs.remove(&sink_id).is_some();
                !subs.is_empty()
            });
            removed
        }
    }

    /// Returns the set of subscribers for the channel.
    ///
    /// The set will be empty if there are no subscribers.
    pub fn get_subscribers(&self, channel_id: ChannelId) -> SmallSinkVec {
        let mut result: SmallSinkVec = self.global.values().cloned().collect();
        if let Some(subs) = self.by_channel.get(&channel_id) {
            result.extend(subs.values().cloned());
        }
        result
    }
}
