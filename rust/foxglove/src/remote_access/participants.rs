//! Two-key lookup table for connected participants.
//!
//! Maintains both `ParticipantIdentity` → `Arc<Participant>` and
//! `ClientId` → `Arc<Participant>` indexes. Both reference the same
//! `Arc<Participant>` values and are kept in sync by construction —
//! mutation is only possible through the inherent methods on [`Participants`],
//! all of which update both indexes together.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use livekit::id::ParticipantIdentity;

use crate::remote_access::participant::Participant;
use crate::remote_common::ClientId;

/// Collection of connected participants, indexed by both `ParticipantIdentity`
/// and `ClientId`.
#[derive(Default)]
pub(crate) struct Participants {
    by_identity: HashMap<ParticipantIdentity, Arc<Participant>>,
    by_client_id: HashMap<ClientId, Arc<Participant>>,
}

impl Participants {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `participant` if no participant with the same identity is present.
    ///
    /// Returns `true` on insert, `false` if the identity was already occupied —
    /// in the latter case neither index is modified.
    pub fn insert(&mut self, participant: Arc<Participant>) -> bool {
        let identity = participant.participant_id().clone();
        let Entry::Vacant(v) = self.by_identity.entry(identity) else {
            return false;
        };
        self.by_client_id
            .insert(participant.client_id(), participant.clone());
        v.insert(participant);
        true
    }

    /// Removes and returns the participant for the given identity, if present.
    pub fn remove_by_identity(
        &mut self,
        identity: &ParticipantIdentity,
    ) -> Option<Arc<Participant>> {
        let participant = self.by_identity.remove(identity)?;
        self.by_client_id.remove(&participant.client_id());
        Some(participant)
    }

    /// Returns the participant for the given identity, if present.
    pub fn get_by_identity(&self, identity: &ParticipantIdentity) -> Option<&Arc<Participant>> {
        self.by_identity.get(identity)
    }

    /// Returns the participant for the given `client_id`, if present.
    pub fn get_by_client_id(&self, client_id: ClientId) -> Option<&Arc<Participant>> {
        self.by_client_id.get(&client_id)
    }

    /// Returns `true` if a participant with this identity is registered.
    pub fn contains_identity(&self, identity: &ParticipantIdentity) -> bool {
        self.by_identity.contains_key(identity)
    }

    /// Iterates over all registered participants.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<Participant>> {
        self.by_identity.values()
    }

    /// Returns the number of registered participants.
    pub fn len(&self) -> usize {
        self.by_identity.len()
    }

    /// Removes all participants and returns them.
    pub fn drain(&mut self) -> Vec<Arc<Participant>> {
        self.by_client_id.clear();
        self.by_identity.drain().map(|(_, p)| p).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn make_participant(name: &str) -> Arc<Participant> {
        let identity = ParticipantIdentity(name.to_string());
        let version =
            crate::remote_access::protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, _rx) = flume::bounded(16);
        let pending_resets = Arc::new(parking_lot::Mutex::new(HashSet::new()));
        let reset_notify = Arc::new(tokio::sync::Notify::new());
        let cancel = tokio_util::sync::CancellationToken::new();
        Arc::new(Participant::new(
            identity,
            version,
            tx,
            pending_resets,
            reset_notify,
            cancel,
        ))
    }

    #[test]
    fn insert_returns_true_for_new_identity() {
        let mut ps = Participants::new();
        assert!(ps.insert(make_participant("alice")));
        assert_eq!(ps.len(), 1);
    }

    #[test]
    fn insert_returns_false_for_duplicate_identity() {
        let mut ps = Participants::new();
        assert!(ps.insert(make_participant("alice")));
        assert!(!ps.insert(make_participant("alice")));
        assert_eq!(ps.len(), 1);
    }

    #[test]
    fn insert_populates_both_indexes() {
        let mut ps = Participants::new();
        let p = make_participant("alice");
        let identity = p.participant_id().clone();
        let client_id = p.client_id();
        assert!(ps.insert(p));
        assert!(ps.get_by_identity(&identity).is_some());
        assert!(ps.get_by_client_id(client_id).is_some());
    }

    #[test]
    fn remove_by_identity_clears_both_indexes() {
        let mut ps = Participants::new();
        let p = make_participant("alice");
        let identity = p.participant_id().clone();
        let client_id = p.client_id();
        ps.insert(p);
        assert!(ps.remove_by_identity(&identity).is_some());
        assert!(ps.get_by_identity(&identity).is_none());
        assert!(ps.get_by_client_id(client_id).is_none());
        assert_eq!(ps.len(), 0);
    }

    #[test]
    fn remove_by_identity_returns_none_for_missing() {
        let mut ps = Participants::new();
        let missing = ParticipantIdentity("nobody".to_string());
        assert!(ps.remove_by_identity(&missing).is_none());
    }

    #[test]
    fn duplicate_insert_does_not_disturb_existing_entry() {
        let mut ps = Participants::new();
        let first = make_participant("alice");
        let first_client_id = first.client_id();
        ps.insert(first);
        // Second participant has the same identity but a distinct client_id.
        let second = make_participant("alice");
        let second_client_id = second.client_id();
        assert_ne!(first_client_id, second_client_id);
        assert!(!ps.insert(second));
        // Secondary index must not contain the rejected participant's client_id.
        assert!(ps.get_by_client_id(first_client_id).is_some());
        assert!(ps.get_by_client_id(second_client_id).is_none());
    }

    #[test]
    fn drain_clears_both_indexes_and_returns_all() {
        let mut ps = Participants::new();
        let alice = make_participant("alice");
        let alice_client_id = alice.client_id();
        ps.insert(alice);
        ps.insert(make_participant("bob"));
        let drained = ps.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(ps.len(), 0);
        assert!(ps.get_by_client_id(alice_client_id).is_none());
    }

    #[test]
    fn iter_yields_all_registered_participants() {
        let mut ps = Participants::new();
        ps.insert(make_participant("alice"));
        ps.insert(make_participant("bob"));
        assert_eq!(ps.iter().count(), 2);
    }
}
