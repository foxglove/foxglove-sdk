//! Per-participant state for a remote access session.

#[cfg(test)]
use std::sync::Arc;

use bytes::Bytes;
use livekit::{ByteStreamWriter, StreamWriter, id::ParticipantIdentity};
use semver::Version;

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
/// flush task (spawned by `add_participant`), not in this struct.
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
    /// flush task spawned in `add_participant`.
    control_tx: flume::Sender<Bytes>,
    /// Limits concurrent service calls from this participant.
    service_call_sem: Semaphore,
    /// Limits concurrent fetch asset requests from this participant.
    fetch_asset_sem: Semaphore,
}

impl Participant {
    /// Creates a new participant.
    pub fn new(
        identity: ParticipantIdentity,
        protocol_version: Version,
        control_tx: flume::Sender<Bytes>,
    ) -> Self {
        Self {
            client_id: ClientId::next(),
            participant_id: identity,
            protocol_version,
            control_tx,
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

    /// Returns the participant's identity.
    pub fn participant_id(&self) -> &ParticipantIdentity {
        &self.participant_id
    }

    /// Try to queue a control plane message. Returns `true` if enqueued, `false`
    /// if the queue is full or disconnected.
    ///
    /// When this returns `false`, the caller should trigger a participant reset
    /// (disconnect + reconnect) — a full queue means the client is not keeping up.
    pub(crate) fn try_queue_control(&self, data: Bytes) -> bool {
        match self.control_tx.try_send(data) {
            Ok(()) => true,
            Err(flume::TrySendError::Full(_)) => {
                tracing::warn!(
                    "control queue full for {}, disconnecting slow client",
                    self.participant_id
                );
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

    /// Send a fetch asset response to the participant via the control plane queue.
    pub(crate) fn send_asset_response(&self, data: &[u8], request_id: u32) {
        // Asset responses are best-effort — if the queue is full, the client is
        // being disconnected anyway and will re-request after reconnection.
        self.try_queue_control(encode_binary_message(&FetchAssetResponse::asset_data(
            request_id, data,
        )));
    }

    /// Send a fetch asset error to the participant via the control plane queue.
    pub(crate) fn send_asset_error(&self, error: &str, request_id: u32) {
        self.try_queue_control(encode_binary_message(&FetchAssetResponse::error_message(
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
