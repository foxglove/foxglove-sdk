//! Parameter-subscription bookkeeping for a remote access session.
//!
//! Tracks which participants are subscribed to which parameter names. Lifecycle
//! is independent of channel subscriptions, so this lives in its own struct
//! alongside [`crate::remote_access::channel_registry::ChannelRegistry`].

use std::collections::{HashMap, HashSet};

use livekit::id::ParticipantIdentity;

/// Tracks parameter-name → set of subscribed participant identities.
pub(super) struct ParameterSubscriptions {
    subscribers_by_name: HashMap<String, HashSet<ParticipantIdentity>>,
}

impl ParameterSubscriptions {
    pub(super) fn new() -> Self {
        Self {
            subscribers_by_name: HashMap::new(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity(name: &str) -> ParticipantIdentity {
        ParticipantIdentity(name.to_string())
    }

    #[test]
    fn first_subscriber_is_reported() {
        let mut subs = ParameterSubscriptions::new();
        let id = make_identity("alice");

        let new_names = subs.subscribe(&id, vec!["p1".into()]);
        assert_eq!(new_names, vec!["p1".to_string()]);
    }

    #[test]
    fn second_subscriber_is_not_reported_as_first() {
        let mut subs = ParameterSubscriptions::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");

        let _ = subs.subscribe(&id_a, vec!["p1".into()]);
        let new_names = subs.subscribe(&id_b, vec!["p1".into()]);
        assert!(new_names.is_empty());
        assert_eq!(subs.subscribers("p1").unwrap().len(), 2);
    }

    #[test]
    fn duplicate_subscribe_is_idempotent() {
        let mut subs = ParameterSubscriptions::new();
        let id = make_identity("alice");

        let _ = subs.subscribe(&id, vec!["p1".into()]);
        let new_names = subs.subscribe(&id, vec!["p1".into()]);
        assert!(new_names.is_empty());
        assert_eq!(subs.subscribers("p1").unwrap().len(), 1);
    }

    #[test]
    fn subscribe_multiple_names_at_once() {
        let mut subs = ParameterSubscriptions::new();
        let id = make_identity("alice");

        let new_names = subs.subscribe(&id, vec!["p1".into(), "p2".into()]);
        assert_eq!(new_names.len(), 2);
        assert!(new_names.contains(&"p1".to_string()));
        assert!(new_names.contains(&"p2".to_string()));
    }

    #[test]
    fn last_unsubscriber_is_reported() {
        let mut subs = ParameterSubscriptions::new();
        let id = make_identity("alice");

        let _ = subs.subscribe(&id, vec!["p1".into()]);
        let old_names = subs.unsubscribe(&id, vec!["p1".into()]);
        assert_eq!(old_names, vec!["p1".to_string()]);
        assert!(subs.subscribers("p1").is_none());
    }

    #[test]
    fn non_last_unsubscriber_is_not_reported() {
        let mut subs = ParameterSubscriptions::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");

        let _ = subs.subscribe(&id_a, vec!["p1".into()]);
        let _ = subs.subscribe(&id_b, vec!["p1".into()]);
        let old_names = subs.unsubscribe(&id_a, vec!["p1".into()]);
        assert!(old_names.is_empty());
        assert_eq!(subs.subscribers("p1").unwrap().len(), 1);
    }

    #[test]
    fn unsubscribe_unknown_name_is_noop() {
        let mut subs = ParameterSubscriptions::new();
        let id = make_identity("alice");

        let old_names = subs.unsubscribe(&id, vec!["missing".into()]);
        assert!(old_names.is_empty());
    }

    #[test]
    fn subscribers_returns_none_for_unknown() {
        let subs = ParameterSubscriptions::new();
        assert!(subs.subscribers("nope").is_none());
    }

    #[test]
    fn cleanup_for_removed_identity_drops_orphaned_names() {
        let mut subs = ParameterSubscriptions::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");

        let _ = subs.subscribe(&id_a, vec!["p1".into(), "p2".into()]);
        let _ = subs.subscribe(&id_b, vec!["p2".into()]);

        let last = subs.cleanup_for_removed_identity(&id_a);
        assert_eq!(last, vec!["p1".to_string()]);
        assert!(subs.subscribers("p1").is_none());
        assert_eq!(subs.subscribers("p2").unwrap().len(), 1);
    }

    #[test]
    fn cleanup_for_unknown_identity_is_noop() {
        let mut subs = ParameterSubscriptions::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");

        let _ = subs.subscribe(&id_a, vec!["p1".into()]);
        let last = subs.cleanup_for_removed_identity(&id_b);
        assert!(last.is_empty());
        assert_eq!(subs.subscribers("p1").unwrap().len(), 1);
    }
}
