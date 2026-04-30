//! Participant-lifecycle state machine for a remote access session.
//!
//! Owns the participant map + per-participant flush-task handles, and the
//! `pending_resets` / `reset_notify` signalling surfaces that let a flush-task
//! request its own reset. Deliberately knows nothing about LiveKit
//! [`livekit::Room`]s: the caller (which *does* know about the Room) opens
//! the control-plane stream and hands the resulting writer in. Tests build
//! writers directly and never construct a `Room`.
//!
//! [`RemoteAccessSession`] wraps this registry and holds its own
//! [`SessionState`] for channel/subscription/video bookkeeping.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use livekit::id::{ParticipantIdentity, ParticipantSid};
use parking_lot::{Mutex, RwLock};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::remote_access::participant::{Participant, ParticipantWriter};
use crate::remote_access::participants::Participants;

/// Owns the participant membership state machine: add / remove / lookup, plus
/// the `pending_resets` channel that lets a flush-task request its own reset.
pub(super) struct ParticipantRegistry {
    /// Map of connected participants and their flush-task join handles.
    participants: RwLock<Participants>,
    /// Set of `ParticipantSid`s pending a reset (disconnect + reconnect).
    /// Populated by [`Participant::send_control`] on queue overflow and by
    /// flush-tasks on write failure.
    pending_resets: Arc<Mutex<HashSet<ParticipantSid>>>,
    /// Notified when a new reset is inserted into `pending_resets`.
    reset_notify: Arc<Notify>,
    /// Size of the per-participant control-plane queue.
    message_backlog_size: usize,
}

impl ParticipantRegistry {
    pub(super) fn new(message_backlog_size: usize) -> Self {
        Self {
            participants: RwLock::new(Participants::new()),
            pending_resets: Arc::new(Mutex::new(HashSet::new())),
            reset_notify: Arc::new(Notify::new()),
            message_backlog_size,
        }
    }

    /// Spawns a [`Participant`] and its flush-task against the supplied
    /// `writer` and inserts it into the registry, atomically replacing any
    /// prior registration for the same identity that has a **different**
    /// `ParticipantSid`.
    ///
    /// Each byte slice in `initial_messages` is queued on the new
    /// participant's control-plane channel after the flush-task is spawned
    /// but before the participant is visible in the registry. This preserves
    /// the invariant that these bytes (typically `ServerInfo` +
    /// channel/service advertisements) are the first the flush-task delivers
    /// to the viewer, ahead of any broadcast that reaches the participant
    /// after registration.
    ///
    /// `participant_sid` is stored on the new [`Participant`] and indexed in
    /// the registry so any later operation that targets a specific connection
    /// instance — a `ParticipantDisconnected` event, or a queued reset from a
    /// flush-task write failure — matches this exact instance rather than
    /// some other connection that happens to share the identity.
    ///
    /// `joined_at` is the LiveKit server-assigned join timestamp (ms since
    /// epoch) for this connection instance — it disambiguates two
    /// same-identity registrations independently of arrival order.
    ///
    /// `session_cancel` is the session's cancellation token; the spawned
    /// flush-task takes a child of it so the task exits on session close.
    ///
    /// # Replacement semantics
    ///
    /// LiveKit assigns a fresh [`ParticipantSid`] to each connection
    /// instance, so a same-identity reconnect arrives with a different SID:
    ///
    /// - If no participant is registered for `id`, the new one is inserted
    ///   and `None` is returned.
    /// - If a participant is registered with a **different** `participant_sid`
    ///   and the incoming `joined_at` is **at least** the stored one's, the
    ///   prior registration is removed (its flush-task is cancelled by
    ///   `Participants`' removal path), the new one is inserted in its
    ///   place, and the prior `Arc<Participant>` is returned so the caller
    ///   can run any session-level cleanup (subscription sweep, listener
    ///   callbacks). This handles the case where LiveKit emits the
    ///   reconnect's `ParticipantActive` *before* the prior instance's
    ///   `ParticipantDisconnected`; without atomic replacement the new
    ///   instance would be silently dropped and the late `Disconnected`
    ///   would then evict the only registration, stranding the live viewer.
    /// - If a participant is registered with a **different** `participant_sid`
    ///   but the incoming `joined_at` is **older** than the stored one's,
    ///   the incoming registration is treated as stale: the existing
    ///   registration is left untouched, the freshly-spawned participant
    ///   is cancelled and dropped, and `None` is returned. This protects
    ///   against an out-of-order `ParticipantActive` for a superseded
    ///   instance overwriting a fresher registration.
    /// - If a participant is registered with the **same** `participant_sid`,
    ///   the existing registration is left untouched, the freshly-spawned
    ///   participant is cancelled and dropped, and `None` is returned. This
    ///   path is defensive: in practice LiveKit never reuses a SID across
    ///   instances, so re-registering an identical `(identity, sid)` pair
    ///   indicates a redundant call rather than a true reconnect.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn register_participant<I>(
        &self,
        id: ParticipantIdentity,
        participant_sid: ParticipantSid,
        joined_at: i64,
        writer: ParticipantWriter,
        session_cancel: &CancellationToken,
        initial_messages: I,
    ) -> Option<Arc<Participant>>
    where
        I: IntoIterator<Item = Bytes>,
    {
        let (participant, flush_handle) = Participant::spawn(
            id,
            participant_sid,
            joined_at,
            writer,
            self.message_backlog_size,
            self.pending_resets.clone(),
            self.reset_notify.clone(),
            session_cancel,
        );

        for msg in initial_messages {
            participant.send_control(msg);
        }

        let mut participants = self.participants.write();

        // Same-identity reconnect (different SID): atomically remove the prior
        // registration so this `register_participant` call cannot lose a
        // race with a late `ParticipantDisconnected` for the prior instance.
        // A stale incoming registration (older `joined_at`) is rejected so a
        // reordered `ParticipantActive` for a superseded instance cannot
        // stomp the live one.
        let prior = if let Some(existing) =
            participants.get_by_identity(participant.participant_id())
        {
            if existing.participant_sid() == participant.participant_sid() {
                // Defensive no-op: same instance is already registered. Drop
                // the freshly-spawned participant (and its writer); cancel
                // its flush-task so it exits promptly.
                participant.cancel();
                return None;
            }
            if existing.joined_at() > participant.joined_at() {
                tracing::info!(
                    identity = %participant.participant_id(),
                    existing_sid = %existing.participant_sid(),
                    existing_joined_at = existing.joined_at(),
                    incoming_sid = %participant.participant_sid(),
                    incoming_joined_at = participant.joined_at(),
                    "ignoring stale same-identity registration (incoming joined_at precedes registered instance)",
                );
                participant.cancel();
                return None;
            }
            let prior_sid = existing.participant_sid().clone();
            let prior = participants
                .remove_by_sid(&prior_sid)
                .expect("get_by_identity returned Some; remove must succeed");
            Some(prior)
        } else {
            None
        };

        let inserted = participants.insert(participant, flush_handle);
        debug_assert!(
            inserted,
            "identity slot is vacant after the conditional remove above; insert must succeed",
        );
        prior
    }

    /// Removes the participant whose stored `ParticipantSid` matches
    /// `target_sid`. Returns the removed participant or `None` if no
    /// participant with this SID is registered.
    ///
    /// SID-keyed: a `ParticipantDisconnected` for a prior instance can arrive
    /// after a same-identity reconnect has replaced it, but the reconnected
    /// instance has a *different* SID, so a stale removal misses here rather
    /// than tearing down the replacement.
    ///
    /// `Participants::remove_by_sid` cancels the flush-task and detaches its
    /// handle as part of the removal; the caller is responsible for any
    /// further cleanup (subscription sweep, listener callbacks) via
    /// [`SessionState::cleanup_for_removed_identity`].
    pub(super) fn remove_participant(
        &self,
        target_sid: &ParticipantSid,
    ) -> Option<Arc<Participant>> {
        self.participants.write().remove_by_sid(target_sid)
    }

    /// Returns the participant for the given identity, if any.
    pub(super) fn get_participant(&self, id: &ParticipantIdentity) -> Option<Arc<Participant>> {
        self.participants.read().get_by_identity(id).cloned()
    }

    /// Resolves a batch of identities to `Arc<Participant>`s under a single
    /// read lock. Identities with no matching registration are silently
    /// skipped (a participant may have been removed between the identity
    /// snapshot and this call; the missed send is harmless).
    pub(super) fn resolve_identities<I>(&self, identities: I) -> Vec<Arc<Participant>>
    where
        I: IntoIterator<Item = ParticipantIdentity>,
    {
        let participants = self.participants.read();
        identities
            .into_iter()
            .filter_map(|id| participants.get_by_identity(&id).cloned())
            .collect()
    }

    /// Returns the number of registered participants.
    pub(super) fn participant_count(&self) -> usize {
        self.participants.read().len()
    }

    /// Clones every currently-registered participant into a `Vec`. Useful for
    /// iterating at broadcast points without holding the read lock.
    pub(super) fn collect_participants(&self) -> Vec<Arc<Participant>> {
        self.participants.read().iter().cloned().collect()
    }

    /// Drains the pending-reset set and returns its contents.
    pub(super) fn drain_pending_resets(&self) -> HashSet<ParticipantSid> {
        std::mem::take(&mut *self.pending_resets.lock())
    }

    /// Test-only hook to simulate a flush-task failure by directly inserting
    /// a `ParticipantSid` into the pending-reset set. In production this set
    /// is only written by flush-tasks on write failure and by
    /// `Participant::send_control` on queue overflow.
    #[cfg(test)]
    pub(super) fn pending_resets(&self) -> &Arc<Mutex<HashSet<ParticipantSid>>> {
        &self.pending_resets
    }

    /// Shared reference to the reset notifier, for use by the session's event
    /// loop `select!`.
    pub(super) fn reset_notify(&self) -> &Arc<Notify> {
        &self.reset_notify
    }

    /// Cancels every registered participant's flush-task and awaits their
    /// completion. After this call the registry is empty.
    ///
    /// For use at session teardown only — the caller must ensure no further
    /// `register_participant` / `remove_participant` / `reset_participant`
    /// calls can race with this one.
    pub(super) async fn shutdown(&self) {
        let handles = self.participants.write().drain();
        let _ = futures_util::future::join_all(handles).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::remote_access::participant::{ParticipantWriter, TestByteStreamWriter, test_sid};

    fn make_registry() -> ParticipantRegistry {
        ParticipantRegistry::new(16)
    }

    fn test_writer() -> ParticipantWriter {
        ParticipantWriter::Test(Arc::new(TestByteStreamWriter::default()))
    }

    #[tokio::test]
    async fn insert_then_remove_roundtrip() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("alice".to_string());

        let sid = test_sid("alice-1");
        let prior = registry.register_participant(
            id.clone(),
            sid.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(prior.is_none(), "no prior registration expected");
        assert!(registry.get_participant(&id).is_some());

        assert!(registry.remove_participant(&sid).is_some());
        assert!(registry.get_participant(&id).is_none());
    }

    /// `register_participant` for a same-identity, **different**-SID call must
    /// atomically replace the prior registration and return the prior
    /// participant for caller-side cleanup.
    #[tokio::test]
    async fn register_replaces_prior_participant_with_different_sid() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("alice".to_string());
        let sid_1 = test_sid("alice-1");
        let sid_2 = test_sid("alice-2");

        assert!(
            registry
                .register_participant(id.clone(), sid_1.clone(), 1_000, test_writer(), &cancel, [],)
                .is_none()
        );
        let original = registry.get_participant(&id).expect("first present");
        let original_client_id = original.client_id();
        drop(original);

        let prior = registry
            .register_participant(id.clone(), sid_2.clone(), 2_000, test_writer(), &cancel, [])
            .expect("prior registration must be returned for cleanup");
        assert_eq!(prior.client_id(), original_client_id);
        assert_eq!(prior.participant_sid(), &sid_1);

        let current = registry.get_participant(&id).expect("replacement present");
        assert_eq!(current.participant_sid(), &sid_2);
        assert_ne!(current.client_id(), original_client_id);
    }

    /// `register_participant` for a same-identity, **same**-SID call is a
    /// defensive no-op: the existing registration is preserved and the call
    /// returns `None`.
    #[tokio::test]
    async fn register_is_noop_when_same_identity_and_sid_already_present() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("alice".to_string());
        let sid = test_sid("alice-1");

        assert!(
            registry
                .register_participant(id.clone(), sid.clone(), 1_000, test_writer(), &cancel, [],)
                .is_none()
        );
        let original = registry.get_participant(&id).expect("first present");
        let original_client_id = original.client_id();
        drop(original);

        let prior = registry.register_participant(
            id.clone(),
            sid.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(
            prior.is_none(),
            "same-(identity, sid) re-register must be a no-op",
        );

        // The original registration must still be the live one.
        let current = registry
            .get_participant(&id)
            .expect("original still present");
        assert_eq!(current.client_id(), original_client_id);
        assert_eq!(current.participant_sid(), &sid);
    }

    /// Regression test for the same-identity reconnect race:
    ///
    /// 1. Attempt 1 (`viewer-1`, LiveKit SID `S1`) is in the registry.
    /// 2. Its flush-task fails its first write and inserts its
    ///    `ParticipantSid` into `pending_resets` (simulated here by calling
    ///    the set directly).
    /// 3. The reset loop drains `pending_resets` and runs a reset: remove
    ///    attempt 1 (with `S1`), look up the current `RemoteParticipant` from
    ///    LiveKit (attempt 2, SID `S2`, already reconnected), insert attempt
    ///    2 with `S2`.
    /// 4. A `ParticipantDisconnected(viewer-1, sid=S1)` event — queued by
    ///    LiveKit for *attempt 1* before it dropped — is then dispatched to
    ///    `remove_participant(S1)`.
    ///
    /// Because no participant has `participant_sid = S1` anymore (attempt 1
    /// was removed in step 3, and attempt 2 has S2), the SID-keyed remove is
    /// a no-op. Attempt 2 stays registered.
    #[tokio::test]
    async fn stale_disconnect_must_not_remove_reconnected_participant() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("viewer-1".to_string());
        let sid_1 = test_sid("viewer-1-attempt-1");
        let sid_2 = test_sid("viewer-1-attempt-2");

        // Step 1: attempt 1 joins.
        assert!(
            registry
                .register_participant(id.clone(), sid_1.clone(), 1_000, test_writer(), &cancel, [],)
                .is_none()
        );
        let attempt_1 = registry.get_participant(&id).expect("attempt 1 present");
        assert_eq!(attempt_1.participant_sid(), &sid_1);

        // Step 2: simulate attempt 1's flush-task failing its first write.
        registry.pending_resets().lock().insert(sid_1.clone());

        // Step 3: drain + reset. Reset = remove (with the stored SID) +
        // re-insert under the new SID.
        let drained = registry.drain_pending_resets();
        assert_eq!(drained, HashSet::from([sid_1.clone()]));
        assert!(registry.remove_participant(&sid_1).is_some());
        assert!(
            registry
                .register_participant(id.clone(), sid_2.clone(), 2_000, test_writer(), &cancel, [],)
                .is_none()
        );

        let attempt_2 = registry.get_participant(&id).expect("attempt 2 present");
        assert_ne!(attempt_2.participant_sid(), &sid_1);
        assert_eq!(attempt_2.participant_sid(), &sid_2);

        // Step 4: stale disconnect carries attempt 1's SID. No-op.
        let removed = registry.remove_participant(&sid_1);
        assert!(removed.is_none());
        assert!(
            registry.get_participant(&id).is_some(),
            "attempt 2 must still be registered after stale disconnect was ignored",
        );

        // Sanity: matching SID does remove.
        let removed = registry.remove_participant(&sid_2);
        assert!(removed.is_some());
        assert!(registry.get_participant(&id).is_none());
    }

    /// Regression test for the symmetric variant of the same-identity reconnect
    /// race (companion to
    /// [`stale_disconnect_must_not_remove_reconnected_participant`]):
    ///
    /// 1. Attempt 1 (`viewer-1`, SID `S1`) is in the registry. Its flush-task
    ///    is healthy — no reset is in flight.
    /// 2. The viewer reconnects under a new SID `S2`. LiveKit emits
    ///    `ParticipantActive(viewer-1, S2)` BEFORE
    ///    `ParticipantDisconnected(viewer-1, S1)` — possible under impairment
    ///    when the server's `disconnect_existing` admits the new join before
    ///    the old teardown propagates to other participants.
    /// 3. The session calls `register_participant(viewer-1, S2, ..)`. The
    ///    registry must REPLACE the prior `S1` registration with the new `S2`
    ///    one, not silently no-op.
    /// 4. The late `ParticipantDisconnected(viewer-1, S1)` arrives. SID-keyed
    ///    `remove_participant` no-ops (the registered SID is now `S2`).
    ///
    /// Without the fix, step 3 silently fails (the registry's slot is already
    /// occupied) and step 4 removes attempt 1 — leaving NOTHING registered for
    /// `viewer-1` while attempt 2 is alive in the LiveKit room. The viewer is
    /// stranded and the gateway never sends it data.
    #[tokio::test]
    async fn same_identity_reconnect_must_replace_prior_registration() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("viewer-1".to_string());
        let sid_1 = test_sid("viewer-1-attempt-1");
        let sid_2 = test_sid("viewer-1-attempt-2");

        // Step 1: attempt 1 joins normally.
        let prior = registry.register_participant(
            id.clone(),
            sid_1.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(prior.is_none(), "no prior registration expected");
        let attempt_1 = registry.get_participant(&id).expect("attempt 1 present");
        let client_id_1 = attempt_1.client_id();
        drop(attempt_1);

        // Step 2: reconnect under a new SID. ParticipantActive(S2) arrives
        // before ParticipantDisconnected(S1). The session calls
        // `register_participant` for the new instance; the registry must
        // atomically replace the prior registration and return it.
        let prior = registry
            .register_participant(id.clone(), sid_2.clone(), 2_000, test_writer(), &cancel, [])
            .expect("prior registration must be returned for cleanup");
        assert_eq!(prior.participant_sid(), &sid_1);
        assert_eq!(prior.client_id(), client_id_1);

        // Step 3: late stale Disconnected(S1) arrives. SID-keyed remove must
        // no-op because the registered SID is now S2.
        assert!(registry.remove_participant(&sid_1).is_none());

        // Step 4: attempt 2 must still be registered.
        let current = registry.get_participant(&id).expect(
            "attempt 2 must remain registered after a late stale Disconnected for the prior SID",
        );
        assert_eq!(current.participant_sid(), &sid_2);
        assert_ne!(current.client_id(), client_id_1);
    }

    /// `register_participant` must reject a same-identity registration whose
    /// `joined_at` is older than the currently stored one. Without this
    /// check, two `ParticipantActive` events processed out of order — the
    /// older instance landing after the newer one — would stomp the live
    /// registration with a superseded one.
    #[tokio::test]
    async fn register_is_noop_when_incoming_joined_at_precedes_existing() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("viewer-1".to_string());
        let sid_old = test_sid("viewer-1-attempt-1");
        let sid_new = test_sid("viewer-1-attempt-2");

        // The newer instance lands first.
        assert!(
            registry
                .register_participant(
                    id.clone(),
                    sid_new.clone(),
                    2_000,
                    test_writer(),
                    &cancel,
                    [],
                )
                .is_none()
        );
        let current_client_id = registry
            .get_participant(&id)
            .expect("newer instance present")
            .client_id();

        // A delayed `ParticipantActive` for the older instance arrives. It
        // must not displace the newer registration.
        let prior = registry.register_participant(
            id.clone(),
            sid_old.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(
            prior.is_none(),
            "stale same-identity registration must be a no-op",
        );

        let current = registry
            .get_participant(&id)
            .expect("newer instance still registered");
        assert_eq!(current.participant_sid(), &sid_new);
        assert_eq!(
            current.client_id(),
            current_client_id,
            "stale registration must not have replaced the newer instance",
        );
    }

    /// Companion to the test above: a registration whose `joined_at` is
    /// newer than the currently stored one must replace it as before. This
    /// pins down that the monotonicity check uses `>` (strictly older) and
    /// not `>=`, so equal timestamps still fall through to "later wins"
    /// rather than blocking a legitimate reconnect.
    #[tokio::test]
    async fn register_replaces_when_incoming_joined_at_is_equal_or_newer() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("viewer-1".to_string());
        let sid_1 = test_sid("viewer-1-attempt-1");
        let sid_2 = test_sid("viewer-1-attempt-2");
        let sid_3 = test_sid("viewer-1-attempt-3");

        let _ = registry.register_participant(
            id.clone(),
            sid_1.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );

        // Equal `joined_at`, different SID: replace (later wins).
        let prior = registry.register_participant(
            id.clone(),
            sid_2.clone(),
            1_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(
            prior.is_some(),
            "equal-joined_at, different-SID must replace"
        );
        assert_eq!(
            registry
                .get_participant(&id)
                .expect("replacement present")
                .participant_sid(),
            &sid_2,
        );

        // Strictly newer `joined_at`: replace.
        let prior = registry.register_participant(
            id.clone(),
            sid_3.clone(),
            2_000,
            test_writer(),
            &cancel,
            [],
        );
        assert!(prior.is_some(), "newer-joined_at must replace");
        assert_eq!(
            registry
                .get_participant(&id)
                .expect("replacement present")
                .participant_sid(),
            &sid_3,
        );
    }
}
