//! Channel subscription registry

#![allow(dead_code)]

use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;
use smallvec::SmallVec;

mod maps;
use maps::{SubscriberMap, SubscriptionMap};

use crate::channel::ChannelId;
#[cfg(test)]
mod tests;

/// A set of distinct subscribers.
///
/// We use a [`SmallVec`] to avoid heap allocations for lookups, when the number of concurrent
/// subscribers (i.e., sinks) is reasonably small.
pub(crate) type SubscriberVec<T> = SmallVec<[T; 4]>;

/// A structure for tracking and querying channel subscriptions.
///
/// This structure is composed of two parts. A system of record for tracking subscriptions
/// (`subscriptions`) and a cache for fast lookups (`subscribers`). Subscribers (`V`, typically
/// `Arc<dyn Sink>`) are uniquely identified by a hashable key (`K`, typically `SinkId`).
///
/// A subscription may be for a specific channel, or for all channels. When logging a message to a
/// channel, we need to return the union of subscriptions for that particular channel, and global
/// subscriptions. Rather than construct (and deduplicate) this union on the fly, the manager
/// precomputes a cache for handling lookups.
pub(crate) struct SubscriptionManager<K, V> {
    /// Current subscriptions.
    subscriptions: Mutex<SubscriptionMap<K, V>>,
    /// Cached map from channel to interested subscribers.
    subscribers: ArcSwap<SubscriberMap<V>>,
}

impl<K, V> Default for SubscriptionManager<K, V> {
    fn default() -> Self {
        Self {
            subscriptions: Mutex::default(),
            subscribers: ArcSwap::default(),
        }
    }
}

impl<K, V> SubscriptionManager<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Removes all subscriptions.
    pub fn clear(&self) {
        let mut subs = self.subscriptions.lock();
        subs.clear();
        self.subscribers.store(Arc::default());
    }

    /// Adds a global subscription to all channels.
    pub fn subscribe_global(&self, key: K, value: V) {
        let mut subs = self.subscriptions.lock();
        if subs.subscribe_global(key, value) {
            self.subscribers.store(Arc::new((&*subs).into()));
        }
    }

    /// Adds subscriptions to the specified channels.
    pub fn subscribe_channels(
        &self,
        key: K,
        value: V,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) {
        let mut subs = self.subscriptions.lock();
        if subs.subscribe_channels(key, value, channel_ids) {
            self.subscribers.store(Arc::new((&*subs).into()));
        }
    }

    /// Removes subscriptions to the specified channels.
    pub fn unsubscribe_channels<Q>(&self, key: &Q, channels: impl IntoIterator<Item = ChannelId>)
    where
        Q: ?Sized + Hash + Eq,
        K: Borrow<Q>,
    {
        let mut subs = self.subscriptions.lock();
        if subs.unsubscribe_channels(key, channels) {
            self.subscribers.store(Arc::new((&*subs).into()));
        }
    }

    /// Removes all global and per-channel subscriptions for a particular subscriber.
    pub fn remove_subscriber<Q>(&self, key: &Q)
    where
        Q: ?Sized + Hash + Eq,
        K: Borrow<Q>,
    {
        let mut subs = self.subscriptions.lock();
        if subs.remove_subscriber(key) {
            self.subscribers.store(Arc::new((&*subs).into()));
        }
    }

    /// Returns true if there is at least one interested subscriber for the channel.
    pub fn has_subscribers(&self, channel_id: ChannelId) -> bool {
        self.subscribers.load().has_subscribers(channel_id)
    }

    /// Returns the set of subscribers interested in the channel.
    ///
    /// The set may be empty if there are no global subscriptions.
    pub fn get_subscribers(&self, channel_id: ChannelId) -> SubscriberVec<V> {
        self.subscribers.load().get(channel_id).clone()
    }
}
