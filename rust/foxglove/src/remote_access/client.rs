use livekit::id::ParticipantIdentity;

use crate::remote_common::ClientId;

/// Represents a connected remote access client (LiveKit participant).
#[derive(Debug, Clone)]
pub struct Client {
    id: ClientId,
    identity: ParticipantIdentity,
}

impl Client {
    pub(crate) fn new(id: ClientId, identity: ParticipantIdentity) -> Self {
        Self { id, identity }
    }

    /// Returns the locally-significant client ID.
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Returns the client-provided identity.
    #[doc(hidden)]
    pub fn client_key(&self) -> &str {
        &self.identity.0
    }
}
