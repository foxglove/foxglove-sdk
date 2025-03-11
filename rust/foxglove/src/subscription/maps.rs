//! Subscriber cache.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use crate::channel::ChannelId;

use super::SubscriberVec;

/// A map of global and per-channel subscriptions.
pub(crate) struct SubscriptionMap<K, V> {
    /// Global subscriptions (all channels).
    global: HashMap<K, V>,
    /// Per-channel subscriptions.
    channel: HashMap<ChannelId, HashMap<K, V>>,
}

impl<K, V> Default for SubscriptionMap<K, V> {
    fn default() -> Self {
        Self {
            global: HashMap::default(),
            channel: HashMap::default(),
        }
    }
}
impl<K, V> SubscriptionMap<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Removes all subscriptions.
    pub fn clear(&mut self) {
        self.global.clear();
        self.channel.clear();
    }

    /// Adds a global subscription to all channels.
    pub fn subscribe_global(&mut self, key: K, value: V) -> bool {
        self.global.insert(key, value).is_none()
    }

    /// Adds subscriptions to the specified channels.
    pub fn subscribe_channels(
        &mut self,
        key: K,
        value: V,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) -> bool {
        let mut inserted = false;
        for channel_id in channel_ids {
            inserted |= self
                .channel
                .entry(channel_id)
                .or_default()
                .insert(key.clone(), value.clone())
                .is_none();
        }
        inserted
    }

    /// Removes subscriptions to the specified channels.
    pub fn unsubscribe_channels<Q>(
        &mut self,
        key: &Q,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) -> bool
    where
        Q: ?Sized + Hash + Eq,
        K: Borrow<Q>,
    {
        let mut removed = false;
        for channel_id in channel_ids {
            if let Some(subs) = self.channel.get_mut(&channel_id) {
                if subs.remove(key).is_some() {
                    removed = true;
                    if subs.is_empty() {
                        self.channel.remove(&channel_id);
                    }
                }
            }
        }
        removed
    }

    /// Remove all global and per-channel subscriptions for a particular subscriber.
    pub fn remove_subscriber<Q>(&mut self, key: &Q) -> bool
    where
        Q: ?Sized + Hash + Eq,
        K: Borrow<Q>,
    {
        let mut removed = self.global.remove(key).is_some();
        self.channel.retain(|_, subs| {
            removed |= subs.remove(key).is_some();
            !subs.is_empty()
        });
        removed
    }
}

/// A cached map from channel name to a set of interested subscribers.
///
/// This representation is different from the underlying system of record in [`SubscriptionMap`] in
/// two significant ways:
///
///  - The `channel` map includes global subscribers.
///  - Subscribers are stored as [`SubscriberSet`]s instead of hashmaps.
///
pub(crate) struct SubscriberMap<T> {
    /// The set of global subscribers.
    global: SubscriberVec<T>,
    /// A map from channel name to the set of subscribers interested in that channel, including global
    /// subscribers.
    channel: HashMap<ChannelId, SubscriberVec<T>>,
}

impl<T> Default for SubscriberMap<T> {
    fn default() -> Self {
        Self {
            global: SubscriberVec::default(),
            channel: HashMap::default(),
        }
    }
}

impl<K, V> From<&SubscriptionMap<K, V>> for SubscriberMap<V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    fn from(value: &SubscriptionMap<K, V>) -> Self {
        let global = value.global.values().cloned().collect();
        let mut channel = HashMap::with_capacity(value.channel.len());
        for (&channel_id, subs) in &value.channel {
            // Merge in global subscribers.
            let mut subs = subs.clone();
            subs.extend(value.global.clone());
            channel.insert(channel_id, subs.into_values().collect());
        }
        Self { global, channel }
    }
}

impl<T> SubscriberMap<T> {
    /// Returns true if there is at least one interested subscriber for the channel.
    pub fn has_subscribers(&self, channel_id: ChannelId) -> bool {
        !self.global.is_empty() || self.channel.contains_key(&channel_id)
    }

    /// Returns the set of subscribers interested in the channel.
    ///
    /// The set may be empty if there are no global subscriptions.
    pub fn get(&self, channel_id: ChannelId) -> &SubscriberVec<T> {
        self.channel.get(&channel_id).unwrap_or(&self.global)
    }
}
