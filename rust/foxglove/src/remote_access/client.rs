use std::sync::Arc;

use livekit::id::ParticipantIdentity;

use crate::SinkId;
use crate::remote_access::participant::Participant;
use crate::remote_common::ClientId;

/// Represents a connected remote access client (LiveKit participant).
#[derive(Debug, Clone)]
pub struct Client {
    /// Locally-significant identifier for this particular instance of this participant.
    client_id: ClientId,
    /// LiveKit participant ID.
    participant_id: ParticipantIdentity,
    /// The sink ID for the session this client belongs to.
    sink_id: SinkId,
    /// Participant used for sending asset responses. Only set when created via `with_sender`.
    participant: Option<Arc<Participant>>,
}

impl Client {
    /// Instantiate a new client.
    pub(crate) fn new(
        client_id: ClientId,
        participant_id: ParticipantIdentity,
        sink_id: SinkId,
    ) -> Self {
        Self {
            client_id,
            participant_id,
            sink_id,
            participant: None,
        }
    }

    /// Instantiate a new client with the ability to send asset responses.
    pub(super) fn with_sender(
        client_id: ClientId,
        participant_id: ParticipantIdentity,
        sink_id: SinkId,
        participant: Arc<Participant>,
    ) -> Self {
        Self {
            client_id,
            participant_id,
            sink_id,
            participant: Some(participant),
        }
    }

    /// Returns the locally-significant client ID.
    pub fn id(&self) -> ClientId {
        self.client_id
    }

    /// Returns the client-provided identity.
    ///
    /// This is public for testing purposes, but not intended for end-users.
    #[doc(hidden)]
    pub fn participant_id(&self) -> &str {
        &self.participant_id.0
    }

    /// Returns the sink ID for the session this client belongs to.
    pub fn sink_id(&self) -> SinkId {
        self.sink_id
    }

    /// Send a fetch asset response to the client. Does nothing if client has no sender.
    pub(crate) fn send_asset_response(&self, result: Result<&[u8], &str>, request_id: u32) {
        if let Some(participant) = &self.participant {
            match result {
                Ok(data) => participant.send_asset_response(data, request_id),
                Err(err) => participant.send_asset_error(err, request_id),
            }
        } else {
            tracing::debug!(
                client_id = ?self.client_id,
                participant_id = ?self.participant_id,
                sink_id = ?self.sink_id,
                request_id,
                "send_asset_response called but participant is not set"
            );
        }
    }
}
