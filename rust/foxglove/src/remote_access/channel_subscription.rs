use livekit::id::ParticipantIdentity;
use smallvec::SmallVec;

/// Tracks subscribers for a channel along with a version counter.
///
/// The version is incremented on every mutation so that the sender task can detect
/// stale `ChannelWriter`s with a single integer comparison.
#[derive(Clone)]
pub(crate) struct ChannelSubscription {
    subscribers: SmallVec<[ParticipantIdentity; 1]>,
    version: u32,
}

impl ChannelSubscription {
    pub(crate) fn new() -> Self {
        Self {
            subscribers: SmallVec::new(),
            version: 0,
        }
    }

    fn bump_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Returns the current version counter.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns a slice of subscriber identities.
    pub fn subscribers(&self) -> &[ParticipantIdentity] {
        &self.subscribers
    }

    /// Returns true if there are no subscribers.
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// Adds a subscriber, skipping if already present. Bumps the version only if added.
    /// Returns true if the identity was inserted, false if it was already present.
    pub fn add(&mut self, identity: ParticipantIdentity) -> bool {
        if self.subscribers.iter().any(|id| id == &identity) {
            false
        } else {
            self.subscribers.push(identity);
            self.bump_version();
            true
        }
    }

    /// Removes a subscriber by identity and bumps the version.
    ///
    /// Returns `true` if the subscriber was found and removed.
    pub fn remove(&mut self, identity: &ParticipantIdentity) -> bool {
        let Some(pos) = self.subscribers.iter().position(|id| id == identity) else {
            return false;
        };
        self.subscribers.swap_remove(pos);
        self.bump_version();
        true
    }
}
