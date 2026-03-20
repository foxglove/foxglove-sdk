use livekit::id::ParticipantIdentity;

use crate::remote_common::ClientId;

/// Represents a connected remote access client (LiveKit participant).
#[derive(Debug, Clone)]
pub struct Client {
    /// Locally-significant identifier for this particular instance of this participant.
    client_id: ClientId,
    /// LiveKit participant ID.
    participant_id: ParticipantIdentity,
}

impl Client {
    /// Instantiate a new client.
    pub(crate) fn new(client_id: ClientId, participant_id: ParticipantIdentity) -> Self {
        Self {
            client_id,
            participant_id,
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
}
