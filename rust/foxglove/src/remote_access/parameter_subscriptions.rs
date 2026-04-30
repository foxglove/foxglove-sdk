//! Parameter-subscription bookkeeping for a remote access session.
//!
//! Tracks which participants are subscribed to which parameter names. Lifecycle
//! is independent of channel subscriptions, so this lives in its own struct
//! alongside [`crate::remote_access::channel_registry::ChannelRegistry`].

use std::collections::{HashMap, HashSet};

use livekit::id::ParticipantIdentity;

/// Tracks parameter-name → set of subscribed participant identities.
#[derive(Default)]
pub(super) struct ParameterSubscriptions {
    subscribers_by_name: HashMap<String, HashSet<ParticipantIdentity>>,
}

impl ParameterSubscriptions {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Add parameter subscriptions for a participant.
    ///
    /// Returns parameter names that are newly subscribed (i.e. had no prior subscribers).
    pub(super) fn subscribe(
        &mut self,
        identity: &ParticipantIdentity,
        names: Vec<String>,
    ) -> Vec<String> {
        let mut new_names = Vec::new();
        for name in names {
            let subscribers = self.subscribers_by_name.entry(name.clone()).or_default();
            if subscribers.insert(identity.clone()) && subscribers.len() == 1 {
                new_names.push(name);
            }
        }
        new_names
    }

    /// Remove parameter subscriptions for a participant.
    ///
    /// Returns parameter names that lost their last subscriber.
    pub(super) fn unsubscribe(
        &mut self,
        identity: &ParticipantIdentity,
        names: Vec<String>,
    ) -> Vec<String> {
        let mut old_names = Vec::new();
        for name in names {
            if let Some(subscribers) = self.subscribers_by_name.get_mut(&name) {
                subscribers.remove(identity);
                if subscribers.is_empty() {
                    self.subscribers_by_name.remove(&name);
                    old_names.push(name);
                }
            }
        }
        old_names
    }

    /// Returns the set of participant identities subscribed to a parameter.
    pub(super) fn subscribers(&self, name: &str) -> Option<&HashSet<ParticipantIdentity>> {
        self.subscribers_by_name.get(name)
    }

    /// Sweep `identity` out of every parameter-subscription set.
    ///
    /// Returns parameter names that lost their last subscriber. No-op if `identity` was not
    /// subscribed to any parameter.
    pub(super) fn cleanup_for_removed_identity(
        &mut self,
        identity: &ParticipantIdentity,
    ) -> Vec<String> {
        let mut last_unsubscribed = Vec::new();
        self.subscribers_by_name.retain(|name, subscribers| {
            subscribers.remove(identity);
            if subscribers.is_empty() {
                last_unsubscribed.push(name.clone());
                false
            } else {
                true
            }
        });
        last_unsubscribed
    }
}
