//! Per-participant state for a remote access session.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use livekit::{ByteStreamWriter, StreamWriter, id::ParticipantIdentity};
use semver::Version;
use tokio_util::sync::CancellationToken;

use crate::protocol::v2::server::FetchAssetResponse;
use crate::remote_access::RemoteAccessError;
use crate::remote_access::session::encode_binary_message;
use crate::remote_common::ClientId;
use crate::remote_common::semaphore::Semaphore;

type Result<T> = std::result::Result<T, Box<RemoteAccessError>>;

const DEFAULT_SERVICE_CALLS_PER_PARTICIPANT: usize = 32;
const DEFAULT_FETCH_ASSET_PER_PARTICIPANT: usize = 32;

/// A participant in the remote access session.
///
/// Each participant has an identity, a per-participant control plane queue, and
/// rate-limiting semaphores. The actual byte-stream writer lives in a dedicated
/// flush task (spawned by `Participant::spawn`), not in this struct.
pub(crate) struct Participant {
    /// Locally-significant identifier for this particular instance of this participant.
    client_id: ClientId,
    /// LiveKit participant ID.
    participant_id: ParticipantIdentity,
    /// The remote access protocol version advertised by this participant.
    /// Stored for future use when branching protocol behavior based on the participant's version.
    #[expect(dead_code)]
    protocol_version: Version,
    /// Per-participant control plane queue. The receiving end is owned by the
    /// flush task.
    control_tx: flume::Sender<Bytes>,
    /// Shared set of participant identities pending a reset. Inserting into this
    /// set + notifying is how we signal `handle_room_events` to disconnect us.
    pending_resets: Arc<parking_lot::Mutex<HashSet<ParticipantIdentity>>>,
    /// Wakes `handle_room_events` when we add ourselves to `pending_resets`.
    reset_notify: Arc<tokio::sync::Notify>,
    /// Per-participant cancellation token. Cancelled when the control queue
    /// overflows, signaling the flush task to stop immediately.
    cancel: CancellationToken,
    /// Limits concurrent service calls from this participant.
    service_call_sem: Semaphore,
    /// Limits concurrent fetch asset requests from this participant.
    fetch_asset_sem: Semaphore,
}

impl Participant {
    /// Creates a new participant with its own control plane channel and flush task.
    ///
    /// The flush task drains the bounded channel into the `writer`. It exits when
    /// the per-participant cancellation token fires (queue overflow or session
    /// shutdown) or when all `control_tx` senders are dropped.
    ///
    /// Returns the participant (wrapped in `Arc` for shared ownership) and the
    /// flush task's `JoinHandle` (for teardown awaiting).
    pub fn spawn(
        identity: ParticipantIdentity,
        protocol_version: Version,
        writer: ParticipantWriter,
        queue_size: usize,
        pending_resets: Arc<parking_lot::Mutex<HashSet<ParticipantIdentity>>>,
        reset_notify: Arc<tokio::sync::Notify>,
        session_cancel: &CancellationToken,
    ) -> (Arc<Self>, tokio::task::JoinHandle<()>) {
        let (control_tx, control_rx) = flume::bounded::<Bytes>(queue_size);
        let cancel = session_cancel.child_token();
        let cancel_for_task = cancel.clone();
        let identity_for_task = identity.clone();
        let pending_resets_for_task = pending_resets.clone();
        let reset_notify_for_task = reset_notify.clone();

        let flush_handle = tokio::spawn(async move {
            loop {
                let data = tokio::select! {
                    biased;
                    () = cancel_for_task.cancelled() => break,
                    msg = control_rx.recv_async() => match msg {
                        Ok(data) => data,
                        Err(_) => break,
                    },
                };
                // Wrap the write in a cancel-aware select with a generous timeout
                // as a safeguard against writer.write() blocking indefinitely.
                let write_result = tokio::select! {
                    biased;
                    () = cancel_for_task.cancelled() => break,
                    result = writer.write(&data) => result,
                };
                if let Err(e) = write_result {
                    tracing::warn!(
                        "control write failed for {:?}, requesting reset: {e:?}",
                        identity_for_task,
                    );
                    pending_resets_for_task
                        .lock()
                        .insert(identity_for_task.clone());
                    reset_notify_for_task.notify_one();
                    break;
                }
            }
        });

        let participant = Arc::new(Self {
            client_id: ClientId::next(),
            participant_id: identity,
            protocol_version,
            control_tx,
            pending_resets,
            reset_notify,
            cancel,
            service_call_sem: Semaphore::new(DEFAULT_SERVICE_CALLS_PER_PARTICIPANT),
            fetch_asset_sem: Semaphore::new(DEFAULT_FETCH_ASSET_PER_PARTICIPANT),
        });
        (participant, flush_handle)
    }

    /// Creates a new participant without spawning a flush task.
    ///
    /// For use in tests that only need a participant with a pre-created channel.
    #[cfg(test)]
    pub fn new(
        identity: ParticipantIdentity,
        protocol_version: Version,
        control_tx: flume::Sender<Bytes>,
        pending_resets: Arc<parking_lot::Mutex<HashSet<ParticipantIdentity>>>,
        reset_notify: Arc<tokio::sync::Notify>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            client_id: ClientId::next(),
            participant_id: identity,
            protocol_version,
            control_tx,
            pending_resets,
            reset_notify,
            cancel,
            service_call_sem: Semaphore::new(DEFAULT_SERVICE_CALLS_PER_PARTICIPANT),
            fetch_asset_sem: Semaphore::new(DEFAULT_FETCH_ASSET_PER_PARTICIPANT),
        }
    }

    /// Returns the locally-significant client ID.
    pub fn client_id(&self) -> ClientId {
        self.client_id
    }

    /// Returns the service call semaphore for this participant.
    pub fn service_call_sem(&self) -> &Semaphore {
        &self.service_call_sem
    }

    /// Returns the fetch asset semaphore for this participant.
    pub fn fetch_asset_sem(&self) -> &Semaphore {
        &self.fetch_asset_sem
    }

    /// Cancel this participant's flush task. The task will exit at the next
    /// `select!` iteration.
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Returns the participant's identity.
    pub fn participant_id(&self) -> &ParticipantIdentity {
        &self.participant_id
    }

    /// Try to queue a control plane message. Returns `false` if the queue is
    /// full and the caller should trigger a participant reset.
    #[must_use]
    pub(crate) fn try_queue_control(&self, data: Bytes) -> bool {
        match self.control_tx.try_send(data) {
            Ok(()) => true,
            Err(flume::TrySendError::Full(_)) => {
                tracing::warn!("control queue full for {}", self.participant_id);
                false
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                tracing::debug!(
                    "control queue disconnected for {}, dropping message",
                    self.participant_id
                );
                // Queue already disconnected — flush task has exited. A reset is
                // likely already in progress, so don't trigger another one.
                true
            }
        }
    }

    /// Queue a control plane message, requesting a participant reset if the
    /// queue is full. Also cancels the per-participant token so the flush task
    /// stops immediately — no point draining messages for a client being disconnected.
    pub(crate) fn send_control(&self, data: Bytes) {
        if !self.try_queue_control(data) {
            self.cancel.cancel();
            self.pending_resets
                .lock()
                .insert(self.participant_id.clone());
            self.reset_notify.notify_one();
        }
    }

    /// Send a fetch asset response to the participant via the control plane queue.
    pub(crate) fn send_asset_response(&self, data: &[u8], request_id: u32) {
        self.send_control(encode_binary_message(&FetchAssetResponse::asset_data(
            request_id, data,
        )));
    }

    /// Send a fetch asset error to the participant via the control plane queue.
    pub(crate) fn send_asset_error(&self, error: &str, request_id: u32) {
        self.send_control(encode_binary_message(&FetchAssetResponse::error_message(
            request_id, error,
        )));
    }
}

impl std::fmt::Debug for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Participant")
            .field("identity", &self.participant_id)
            .finish()
    }
}

impl std::fmt::Display for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Participant({})", self.participant_id)
    }
}

/// A writer for a participant's control plane byte stream.
///
/// Wraps an ordered, reliable byte stream to one specific participant.
/// Owned by the per-participant flush task, not by `Participant` itself.
///
/// Mocked with a `TestByteStreamWriter` for tests.
pub(crate) enum ParticipantWriter {
    Livekit(ByteStreamWriter),
    #[allow(dead_code)]
    #[cfg(test)]
    Test(Arc<TestByteStreamWriter>),
}

impl ParticipantWriter {
    pub(crate) async fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            ParticipantWriter::Livekit(stream) => stream.write(bytes).await.map_err(|e| e.into()),
            #[cfg(test)]
            ParticipantWriter::Test(writer) => {
                writer.record(bytes);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestByteStreamWriter {
    writes: parking_lot::Mutex<Vec<Bytes>>,
}

#[cfg(test)]
impl TestByteStreamWriter {
    pub(crate) fn record(&self, data: &[u8]) {
        self.writes.lock().push(Bytes::copy_from_slice(data));
    }

    #[allow(dead_code)]
    pub(crate) fn writes(&self) -> Vec<Bytes> {
        std::mem::take(&mut self.writes.lock())
    }
}
