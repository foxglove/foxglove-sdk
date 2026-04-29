//! Two-key lookup table for connected participants, plus their flush-task
//! join handles.
//!
//! Maintains `ParticipantIdentity` → `Arc<Participant>` and
//! `ParticipantSid` → `Arc<Participant>` indexes over the same
//! `Arc<Participant>` values, and a parallel `ParticipantIdentity` →
//! `JoinHandle<()>` map for each participant's flush-task. All three are kept
//! in sync by construction — mutation is only possible through the inherent
//! methods on [`Participants`].

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use livekit::id::{ParticipantIdentity, ParticipantSid};
use tokio::task::JoinHandle;

use crate::remote_access::participant::Participant;

/// Collection of connected participants, indexed by both `ParticipantIdentity`
/// and `ParticipantSid`, with each participant's flush-task `JoinHandle`
/// stored alongside.
#[derive(Default)]
pub(crate) struct Participants {
    by_identity: HashMap<ParticipantIdentity, Arc<Participant>>,
    by_sid: HashMap<ParticipantSid, Arc<Participant>>,
    flush_handles: HashMap<ParticipantIdentity, JoinHandle<()>>,
}

impl Participants {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `participant` and its flush-task handle if no participant
    /// with the same identity is present.
    ///
    /// Returns `true` on insert, `false` if the identity was already occupied
    /// — in the latter case no index is modified and `flush_handle` is
    /// dropped.
    pub fn insert(&mut self, participant: Arc<Participant>, flush_handle: JoinHandle<()>) -> bool {
        let identity = participant.participant_id().clone();
        let Entry::Vacant(v) = self.by_identity.entry(identity.clone()) else {
            return false;
        };
        self.by_sid
            .insert(participant.participant_sid().clone(), participant.clone());
        self.flush_handles.insert(identity, flush_handle);
        v.insert(participant);
        true
    }

    /// Removes the participant matching `sid`, cancels its flush-task, and
    /// detaches the task handle. Cancellation makes the task exit at its next
    /// `select!` iteration rather than when senders eventually drop; the
    /// detached handle is then dropped without being awaited.
    ///
    /// Returns `None` if no participant with this SID is registered. A miss
    /// is the staleness filter: a `ParticipantDisconnected` for a prior
    /// connection instance, or a queued reset for a participant that's
    /// already been removed, will not match here because each connection
    /// instance gets a unique `ParticipantSid` (a same-identity reconnect
    /// has a *different* SID).
    pub fn remove_by_sid(&mut self, sid: &ParticipantSid) -> Option<Arc<Participant>> {
        let participant = self.by_sid.remove(sid)?;
        let identity = participant.participant_id();
        self.by_identity.remove(identity);
        drop(self.flush_handles.remove(identity));
        participant.cancel();
        Some(participant)
    }

    /// Returns the participant for the given identity, if present.
    pub fn get_by_identity(&self, identity: &ParticipantIdentity) -> Option<&Arc<Participant>> {
        self.by_identity.get(identity)
    }

    /// Returns the participant for the given `ParticipantSid`, if present.
    /// Test-only accessor — production reads the index implicitly via
    /// [`remove_by_sid`].
    #[cfg(test)]
    pub fn get_by_sid(&self, sid: &ParticipantSid) -> Option<&Arc<Participant>> {
        self.by_sid.get(sid)
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

    /// Removes all participants and their flush handles, returning both.
    pub fn drain(&mut self) -> (Vec<Arc<Participant>>, Vec<JoinHandle<()>>) {
        self.by_sid.clear();
        let participants = self.by_identity.drain().map(|(_, p)| p).collect();
        let handles = self.flush_handles.drain().map(|(_, h)| h).collect();
        (participants, handles)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn make_participant(name: &str) -> Arc<Participant> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let identity = ParticipantIdentity(name.to_string());
        let sid = crate::remote_access::participant::test_sid(&format!("{name}-{n}"));
        let (tx, _rx) = flume::bounded(16);
        let pending_resets = Arc::new(parking_lot::Mutex::new(HashSet::new()));
        let reset_notify = Arc::new(tokio::sync::Notify::new());
        let cancel = tokio_util::sync::CancellationToken::new();
        Arc::new(Participant::new(
            identity,
            sid,
            tx,
            pending_resets,
            reset_notify,
            cancel,
        ))
    }

    /// Builds a trivial `JoinHandle<()>` for tests. Must be called from within
    /// a tokio runtime context (provided by `#[tokio::test]`).
    fn dummy_handle() -> JoinHandle<()> {
        tokio::spawn(async {})
    }

    #[tokio::test]
    async fn insert_returns_true_for_new_identity() {
        let mut ps = Participants::new();
        assert!(ps.insert(make_participant("alice"), dummy_handle()));
        assert_eq!(ps.len(), 1);
    }

    #[tokio::test]
    async fn insert_returns_false_for_duplicate_identity() {
        let mut ps = Participants::new();
        assert!(ps.insert(make_participant("alice"), dummy_handle()));
        assert!(!ps.insert(make_participant("alice"), dummy_handle()));
        assert_eq!(ps.len(), 1);
    }

    #[tokio::test]
    async fn insert_populates_both_indexes() {
        let mut ps = Participants::new();
        let p = make_participant("alice");
        let identity = p.participant_id().clone();
        let sid = p.participant_sid().clone();
        assert!(ps.insert(p, dummy_handle()));
        assert!(ps.get_by_identity(&identity).is_some());
        assert!(ps.get_by_sid(&sid).is_some());
    }

    #[tokio::test]
    async fn remove_by_sid_clears_both_indexes() {
        let mut ps = Participants::new();
        let p = make_participant("alice");
        let identity = p.participant_id().clone();
        let sid = p.participant_sid().clone();
        ps.insert(p, dummy_handle());
        assert!(ps.remove_by_sid(&sid).is_some());
        assert!(ps.get_by_identity(&identity).is_none());
        assert!(ps.get_by_sid(&sid).is_none());
        assert_eq!(ps.len(), 0);
    }

    #[test]
    fn remove_by_sid_returns_none_for_missing() {
        let mut ps = Participants::new();
        let missing = crate::remote_access::participant::test_sid("nobody");
        assert!(ps.remove_by_sid(&missing).is_none());
    }

    #[tokio::test]
    async fn duplicate_insert_does_not_disturb_existing_entry() {
        let mut ps = Participants::new();
        let first = make_participant("alice");
        let first_sid = first.participant_sid().clone();
        ps.insert(first, dummy_handle());
        // Second participant has the same identity but a distinct SID.
        let second = make_participant("alice");
        let second_sid = second.participant_sid().clone();
        assert_ne!(first_sid, second_sid);
        assert!(!ps.insert(second, dummy_handle()));
        // Secondary index must not contain the rejected participant's SID.
        assert!(ps.get_by_sid(&first_sid).is_some());
        assert!(ps.get_by_sid(&second_sid).is_none());
    }

    /// Load-bearing invariant for the reset-drain loop: after a participant
    /// is removed and a new one is inserted under the same identity, the old
    /// `ParticipantSid` must not resolve to the replacement. If it did,
    /// `handle_room_events`' drain would spuriously reset the reconnected
    /// participant on a stale flush-failure notification.
    #[tokio::test]
    async fn get_by_sid_does_not_match_replaced_participant() {
        let mut ps = Participants::new();
        let original = make_participant("alice");
        let original_sid = original.participant_sid().clone();
        ps.insert(original, dummy_handle());
        ps.remove_by_sid(&original_sid);

        let replacement = make_participant("alice");
        let replacement_sid = replacement.participant_sid().clone();
        assert_ne!(original_sid, replacement_sid);
        ps.insert(replacement, dummy_handle());

        assert!(
            ps.get_by_sid(&original_sid).is_none(),
            "stale ParticipantSid must not resolve to the replacement participant",
        );
        assert!(
            ps.get_by_sid(&replacement_sid).is_some(),
            "fresh ParticipantSid must resolve to the current participant",
        );
    }

    #[tokio::test]
    async fn drain_clears_both_indexes_and_returns_all() {
        let mut ps = Participants::new();
        let alice = make_participant("alice");
        let alice_sid = alice.participant_sid().clone();
        ps.insert(alice, dummy_handle());
        ps.insert(make_participant("bob"), dummy_handle());
        let (taken, handles) = ps.drain();
        assert_eq!(taken.len(), 2);
        assert_eq!(handles.len(), 2);
        assert_eq!(ps.len(), 0);
        assert!(ps.get_by_sid(&alice_sid).is_none());
    }

    #[tokio::test]
    async fn iter_yields_all_registered_participants() {
        let mut ps = Participants::new();
        ps.insert(make_participant("alice"), dummy_handle());
        ps.insert(make_participant("bob"), dummy_handle());
        assert_eq!(ps.iter().count(), 2);
    }
}
