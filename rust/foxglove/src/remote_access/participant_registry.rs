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
use semver::Version;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::remote_access::participant::{Participant, ParticipantWriter};
use crate::remote_access::participants::Participants;
use crate::remote_common::ClientId;

/// Owns the participant membership state machine: add / remove / lookup, plus
/// the `pending_resets` channel that lets a flush-task request its own reset.
pub(crate) struct ParticipantRegistry {
    /// Map of connected participants and their flush-task join handles.
    participants: RwLock<Participants>,
    /// Set of `ClientId`s pending a reset (disconnect + reconnect). Populated by
    /// [`Participant::send_control`] on queue overflow and by flush-tasks on
    /// write failure.
    pending_resets: Arc<Mutex<HashSet<ClientId>>>,
    /// Notified when a new reset is inserted into `pending_resets`.
    reset_notify: Arc<Notify>,
    /// Size of the per-participant control-plane queue.
    message_backlog_size: usize,
}

impl ParticipantRegistry {
    pub(crate) fn new(message_backlog_size: usize) -> Self {
        Self {
            participants: RwLock::new(Participants::new()),
            pending_resets: Arc::new(Mutex::new(HashSet::new())),
            reset_notify: Arc::new(Notify::new()),
            message_backlog_size,
        }
    }

    /// Spawns a [`Participant`] and its flush-task against the supplied
    /// `writer` and inserts it into the registry.
    ///
    /// Each byte slice in `initial_messages` is queued on the participant's
    /// control-plane channel after the flush-task is spawned but before the
    /// participant is visible in the registry. This preserves the invariant
    /// that these bytes (typically `ServerInfo` + channel/service
    /// advertisements) are the first the flush-task delivers to the viewer,
    /// ahead of any broadcast that reaches the participant after registration.
    ///
    /// `participant_sid` is stored on the new [`Participant`] so a later
    /// `ParticipantDisconnected` event can be matched against this specific
    /// connection instance rather than the identity alone.
    ///
    /// `session_cancel` is the session's cancellation token; the spawned
    /// flush-task takes a child of it so the task exits on session close.
    ///
    /// Returns `false` without spawning or inserting if a participant already
    /// exists for this identity. Callers that want to avoid opening a stream
    /// in that case should gate on [`has_participant`] before opening the
    /// writer; this gate is only a backstop.
    #[must_use = "register_participant returns false on a same-identity collision; \
                  callers must check (or debug_assert) the result to catch \
                  contract violations of the no-concurrent-call rule"]
    pub(crate) fn register_participant<I>(
        &self,
        id: ParticipantIdentity,
        participant_sid: ParticipantSid,
        version: Version,
        writer: ParticipantWriter,
        session_cancel: &CancellationToken,
        initial_messages: I,
    ) -> bool
    where
        I: IntoIterator<Item = Bytes>,
    {
        if self.participants.read().contains_identity(&id) {
            return false;
        }

        let (participant, flush_handle) = Participant::spawn(
            id,
            participant_sid,
            version,
            writer,
            self.message_backlog_size,
            self.pending_resets.clone(),
            self.reset_notify.clone(),
            session_cancel,
        );

        for msg in initial_messages {
            participant.send_control(msg);
        }

        self.participants.write().insert(participant, flush_handle)
    }

    /// Removes the participant registered under `id` if — and only if — its
    /// stored `participant_sid` matches `expected_sid`. Returns the removed
    /// participant (for its `ClientId`) or `None` if no match (either the
    /// identity isn't registered, or the stored SID belongs to a later
    /// connection instance that must not be torn down).
    ///
    /// Identity alone is ambiguous: a `ParticipantDisconnected` for a prior
    /// instance can arrive after a same-identity reconnect has replaced it.
    /// Requiring the SID at the call site means the caller has already
    /// committed to which *specific* instance it's removing — the event
    /// handler uses the disconnected participant's SID; `reset_participant`
    /// uses the stored participant's SID.
    ///
    /// The flush-task is cancelled and its handle detached; the caller is
    /// responsible for any further cleanup (subscription sweep, listener
    /// callbacks) via [`SessionState::cleanup_for_removed_identity`].
    pub(crate) fn remove_participant(
        &self,
        id: &ParticipantIdentity,
        expected_sid: &ParticipantSid,
    ) -> Option<Arc<Participant>> {
        let mut participants = self.participants.write();
        let current = participants.get_by_identity(id)?;
        if current.participant_sid() != expected_sid {
            return None;
        }
        let removed = participants.remove_by_identity(id)?;
        removed.cancel();
        Some(removed)
    }

    /// Returns the participant for the given identity, if any.
    pub(crate) fn get_participant(&self, id: &ParticipantIdentity) -> Option<Arc<Participant>> {
        self.participants.read().get_by_identity(id).cloned()
    }

    /// Resolves a batch of identities to `Arc<Participant>`s under a single
    /// read lock. Identities with no matching registration are silently
    /// skipped (a participant may have been removed between the identity
    /// snapshot and this call; the missed send is harmless).
    pub(crate) fn resolve_identities<I>(&self, identities: I) -> Vec<Arc<Participant>>
    where
        I: IntoIterator<Item = ParticipantIdentity>,
    {
        let participants = self.participants.read();
        identities
            .into_iter()
            .filter_map(|id| participants.get_by_identity(&id).cloned())
            .collect()
    }

    /// Returns the participant matching the given `ClientId`, if any.
    pub(crate) fn get_participant_by_client_id(
        &self,
        client_id: ClientId,
    ) -> Option<Arc<Participant>> {
        self.participants
            .read()
            .get_by_client_id(client_id)
            .cloned()
    }

    /// Returns true if a participant exists for the given identity.
    pub(crate) fn has_participant(&self, id: &ParticipantIdentity) -> bool {
        self.participants.read().contains_identity(id)
    }

    /// Returns the number of registered participants.
    pub(crate) fn participant_count(&self) -> usize {
        self.participants.read().len()
    }

    /// Clones every currently-registered participant into a `Vec`. Useful for
    /// iterating at broadcast points without holding the read lock.
    pub(crate) fn collect_participants(&self) -> Vec<Arc<Participant>> {
        self.participants.read().iter().cloned().collect()
    }

    /// Drains the pending-reset set and returns its contents.
    pub(crate) fn drain_pending_resets(&self) -> Vec<ClientId> {
        self.pending_resets.lock().drain().collect()
    }

    /// Test-only hook to simulate a flush-task failure by directly inserting
    /// a `ClientId` into the pending-reset set. In production this set is
    /// only written by flush-tasks on write failure and by
    /// `Participant::send_control` on queue overflow.
    #[cfg(test)]
    pub(crate) fn pending_resets(&self) -> &Arc<Mutex<HashSet<ClientId>>> {
        &self.pending_resets
    }

    /// Shared reference to the reset notifier, for use by the session's event
    /// loop `select!`.
    pub(crate) fn reset_notify(&self) -> &Arc<Notify> {
        &self.reset_notify
    }

    /// Cancels every registered participant's flush-task and awaits their
    /// completion. After this call the registry is empty.
    ///
    /// For use at session teardown only — the caller must ensure no further
    /// `register_participant` / `remove_participant` / `reset_participant`
    /// calls can race with this one.
    pub(crate) async fn shutdown(&self) {
        let (participants, handles) = self.participants.write().drain();
        for p in &participants {
            p.cancel();
        }
        let _ = futures_util::future::join_all(handles).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::remote_access::participant::{ParticipantWriter, TestByteStreamWriter, test_sid};
    use crate::remote_access::protocol_version;

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
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();

        let sid = test_sid("alice-1");
        let inserted = registry.register_participant(
            id.clone(),
            sid.clone(),
            version,
            test_writer(),
            &cancel,
            [],
        );
        assert!(inserted);
        assert!(registry.has_participant(&id));

        assert!(registry.remove_participant(&id, &sid).is_some());
        assert!(!registry.has_participant(&id));
    }

    #[tokio::test]
    async fn insert_is_noop_when_identity_already_present() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("alice".to_string());
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();

        assert!(registry.register_participant(
            id.clone(),
            test_sid("alice-1"),
            version.clone(),
            test_writer(),
            &cancel,
            [],
        ));
        assert!(!registry.register_participant(
            id.clone(),
            test_sid("alice-2"),
            version,
            test_writer(),
            &cancel,
            [],
        ));
    }

    /// Regression test for the same-identity reconnect race:
    ///
    /// 1. Attempt 1 (`viewer-1`, LiveKit SID `S1`) is in the registry.
    /// 2. Its flush-task fails its first write and inserts its `ClientId`
    ///    into `pending_resets` (simulated here by calling the set directly).
    /// 3. The reset loop drains `pending_resets` and runs a reset: remove
    ///    attempt 1 (with `S1`), look up the current `RemoteParticipant` from
    ///    LiveKit (attempt 2, SID `S2`, already reconnected), insert attempt
    ///    2 with `S2`.
    /// 4. A `ParticipantDisconnected(viewer-1, sid=S1)` event — queued by
    ///    LiveKit for *attempt 1* before it dropped — is then dispatched to
    ///    `remove_participant(viewer-1, S1)`.
    ///
    /// Because the currently-stored `Participant` has `participant_sid = S2`, the
    /// SID-keyed remove is a no-op. Attempt 2 stays registered.
    #[tokio::test]
    async fn stale_disconnect_must_not_remove_reconnected_participant() {
        let registry = make_registry();
        let cancel = CancellationToken::new();
        let id = ParticipantIdentity("viewer-1".to_string());
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let sid_1 = test_sid("viewer-1-attempt-1");
        let sid_2 = test_sid("viewer-1-attempt-2");

        // Step 1: attempt 1 joins.
        assert!(registry.register_participant(
            id.clone(),
            sid_1.clone(),
            version.clone(),
            test_writer(),
            &cancel,
            [],
        ));
        let attempt_1 = registry.get_participant(&id).expect("attempt 1 present");
        let client_id_1 = attempt_1.client_id();
        assert_eq!(attempt_1.participant_sid(), &sid_1);

        // Step 2: simulate attempt 1's flush-task failing its first write.
        registry.pending_resets().lock().insert(client_id_1);

        // Step 3: drain + reset. Reset = remove (with the stored SID) +
        // re-insert under the new SID.
        let drained = registry.drain_pending_resets();
        assert_eq!(drained, vec![client_id_1]);
        let reset_target = registry
            .get_participant_by_client_id(client_id_1)
            .expect("attempt 1 resolves from drained client_id");
        let stored_version = reset_target.protocol_version().clone();
        let stored_sid = reset_target.participant_sid().clone();
        registry.remove_participant(reset_target.participant_id(), &stored_sid);
        assert!(registry.register_participant(
            id.clone(),
            sid_2.clone(),
            stored_version,
            test_writer(),
            &cancel,
            [],
        ));

        let attempt_2 = registry.get_participant(&id).expect("attempt 2 present");
        assert_ne!(attempt_2.client_id(), client_id_1);
        assert_eq!(attempt_2.participant_sid(), &sid_2);

        // Step 4: stale disconnect carries attempt 1's SID. No-op.
        let removed = registry.remove_participant(&id, &sid_1);
        assert!(removed.is_none());
        assert!(
            registry.has_participant(&id),
            "attempt 2 must still be registered after stale disconnect was ignored",
        );

        // Sanity: matching SID does remove.
        let removed = registry.remove_participant(&id, &sid_2);
        assert!(removed.is_some());
        assert!(!registry.has_participant(&id));
    }
}
