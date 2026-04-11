use livekit::id::ParticipantIdentity;
use smallvec::SmallVec;

/// Tracks subscribers for a channel.
pub(crate) struct ChannelSubscription {
    subscribers: SmallVec<[ParticipantIdentity; 1]>,
}

impl ChannelSubscription {
    pub(crate) fn new() -> Self {
        Self {
            subscribers: SmallVec::new(),
        }
    }

    /// Returns a slice of subscriber identities.
    pub fn subscribers(&self) -> &[ParticipantIdentity] {
        &self.subscribers
    }

    /// Returns true if there are no subscribers.
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// Adds a subscriber, skipping if already present.
    /// Returns true if the identity was inserted, false if it was already present.
    pub fn add(&mut self, identity: ParticipantIdentity) -> bool {
        if self.subscribers.iter().any(|id| id == &identity) {
            false
        } else {
            self.subscribers.push(identity);
            true
        }
    }

    /// Removes a subscriber by identity.
    ///
    /// Returns `true` if the subscriber was found and removed.
    pub fn remove(&mut self, identity: &ParticipantIdentity) -> bool {
        let Some(pos) = self.subscribers.iter().position(|id| id == identity) else {
            return false;
        };
        self.subscribers.swap_remove(pos);
        true
    }
}
