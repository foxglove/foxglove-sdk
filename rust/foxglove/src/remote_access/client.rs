use livekit::id::ParticipantIdentity;

/// Represents a connected remote access client (LiveKit participant).
#[derive(Debug, Clone)]
pub struct Client {
    id: ParticipantIdentity,
}

impl Client {
    pub(crate) fn new(id: ParticipantIdentity) -> Self {
        Self { id }
    }

    /// Returns the identifier for this client.
    pub fn id(&self) -> &str {
        &self.id.0
    }
}
