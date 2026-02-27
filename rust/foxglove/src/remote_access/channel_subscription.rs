use livekit::id::ParticipantIdentity;
use smallvec::SmallVec;

/// Tracks subscribers for a channel along with a version counter.
///
/// The version is incremented on every mutation so that the sender task can detect
/// stale `ChannelWriter`s with a single integer comparison.
pub(crate) struct ChannelSubscription {
    subscribers: SmallVec<[ParticipantIdentity; 1]>,
    pub version: u32,
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

    /// Returns a slice of subscriber identities.
    pub fn subscribers(&self) -> &[ParticipantIdentity] {
        &self.subscribers
    }

    /// Returns true if there are no subscribers.
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// Adds a subscriber without checking for duplicates, and bumps the version.
    pub fn push(&mut self, identity: ParticipantIdentity) {
        self.subscribers.push(identity);
        self.bump_version();
    }

    /// Removes a subscriber by identity using swap_remove and bumps the version.
    ///
    /// Returns `true` if the subscriber was found and removed.
    pub fn swap_remove(&mut self, identity: &ParticipantIdentity) -> bool {
        let Some(pos) = self.subscribers.iter().position(|id| id == identity) else {
            return false;
        };
        self.subscribers.swap_remove(pos);
        self.bump_version();
        true
    }

    /// Retains only the subscribers satisfying the predicate, bumping the version if
    /// any were removed.
    pub fn retain(&mut self, f: impl Fn(&ParticipantIdentity) -> bool) {
        let before = self.subscribers.len();
        self.subscribers.retain(|id| f(id));
        if self.subscribers.len() != before {
            self.bump_version();
        }
    }
}
