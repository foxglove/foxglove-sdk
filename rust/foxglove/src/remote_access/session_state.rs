use std::collections::HashMap;
use std::sync::Arc;

use livekit::id::ParticipantIdentity;
use smallvec::SmallVec;
use tracing::{debug, info};

use crate::remote_access::participant::Participant;
use crate::{ChannelId, RawChannel};

/// Pure state machine for a remote access session.
///
/// Tracks participants, advertised channels, and per-channel subscriptions.
/// All methods take `&mut self`; the caller (RemoteAccessSession) handles synchronization.
///
/// Methods that modify subscriptions return the set of channel IDs whose subscription
/// status changed (first subscriber added or last subscriber removed), so the caller
/// can notify the Context.
pub(crate) struct SessionState {
    participants: HashMap<ParticipantIdentity, Arc<Participant>>,
    /// Channels that have been advertised to participants.
    channels: HashMap<ChannelId, Arc<RawChannel>>,
    /// Maps channel ID to the participant identities subscribed to that channel.
    subscriptions: HashMap<ChannelId, SmallVec<[ParticipantIdentity; 1]>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            channels: HashMap::new(),
            subscriptions: HashMap::new(),
        }
    }

    /// Inserts a participant. Returns `true` if the participant was newly added,
    /// `false` if it was already present (in which case the entry is unchanged).
    pub fn insert_participant(
        &mut self,
        identity: ParticipantIdentity,
        participant: Arc<Participant>,
    ) -> bool {
        use std::collections::hash_map::Entry;
        match self.participants.entry(identity) {
            Entry::Occupied(_) => false,
            Entry::Vacant(v) => {
                v.insert(participant);
                true
            }
        }
    }

    /// Removes a participant and all of its subscriptions.
    ///
    /// Returns channel IDs that lost their last subscriber (now have zero subscribers).
    pub fn remove_participant(
        &mut self,
        identity: &ParticipantIdentity,
    ) -> SmallVec<[ChannelId; 4]> {
        if self.participants.remove(identity).is_none() {
            return SmallVec::new();
        }
        info!("removed participant {identity:?}");

        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        self.subscriptions.retain(|&channel_id, subscribers| {
            subscribers.retain(|id| id != identity);
            if subscribers.is_empty() {
                last_unsubscribed.push(channel_id);
                false
            } else {
                true
            }
        });
        last_unsubscribed
    }

    /// Returns the participant for the given identity, if present.
    pub fn get_participant(&self, identity: &ParticipantIdentity) -> Option<&Arc<Participant>> {
        self.participants.get(identity)
    }

    /// Returns an iterator over all participants.
    pub fn participants(&self) -> impl Iterator<Item = &Arc<Participant>> {
        self.participants.values()
    }

    /// Records a channel as advertised.
    pub fn insert_channel(&mut self, channel: &Arc<RawChannel>) {
        self.channels.insert(channel.id(), channel.clone());
    }

    /// Removes an advertised channel. Returns `true` if it was present.
    pub fn remove_channel(&mut self, channel_id: ChannelId) -> bool {
        self.channels.remove(&channel_id).is_some()
    }

    /// Returns the advertised channels.
    pub fn channels(&self) -> &HashMap<ChannelId, Arc<RawChannel>> {
        &self.channels
    }

    /// Subscribes a participant to the given channels.
    ///
    /// Returns channel IDs that gained their first subscriber.
    pub fn subscribe(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let subscribers = self.subscriptions.entry(channel_id).or_default();
            if subscribers.contains(participant.identity()) {
                info!("{participant} is already subscribed to channel {channel_id:?}; ignoring",);
                continue;
            }
            let is_first = subscribers.is_empty();
            subscribers.push(participant.identity().clone());
            debug!("{participant} subscribed to channel {channel_id:?}");
            if is_first {
                first_subscribed.push(channel_id);
            }
        }
        first_subscribed
    }

    /// Unsubscribes a participant from the given channels.
    ///
    /// Returns channel IDs that lost their last subscriber.
    pub fn unsubscribe(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let Some(subscribers) = self.subscriptions.get_mut(&channel_id) else {
                info!("{participant} is not subscribed to channel {channel_id:?}; ignoring",);
                continue;
            };
            let Some(pos) = subscribers
                .iter()
                .position(|id| id == participant.identity())
            else {
                info!("{participant} is not subscribed to channel {channel_id:?}; ignoring",);
                continue;
            };
            subscribers.swap_remove(pos);
            debug!("{participant} unsubscribed from channel {channel_id:?}");
            if subscribers.is_empty() {
                self.subscriptions.remove(&channel_id);
                last_unsubscribed.push(channel_id);
            }
        }
        last_unsubscribed
    }

    /// Returns the subscriber identities for a channel.
    pub fn get_subscribers(
        &self,
        channel_id: &ChannelId,
    ) -> Option<&SmallVec<[ParticipantIdentity; 1]>> {
        self.subscriptions.get(channel_id)
    }

    /// Returns the number of subscribers for a channel, or `None` if no subscriptions exist.
    #[cfg(test)]
    pub fn get_subscriber_count(&self, channel_id: &ChannelId) -> Option<usize> {
        self.subscriptions.get(channel_id).map(|s| s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_access::participant::ParticipantWriter;

    fn make_participant(name: &str) -> (ParticipantIdentity, Arc<Participant>) {
        let identity = ParticipantIdentity(name.to_string());
        let writer = Arc::new(crate::remote_access::participant::TestByteStreamWriter::default());
        let participant = Arc::new(Participant::new(
            identity.clone(),
            ParticipantWriter::Test(writer),
        ));
        (identity, participant)
    }

    fn make_channel(topic: &str) -> Arc<RawChannel> {
        use crate::{ChannelBuilder, Context, Schema};
        let ctx = Context::new();
        ChannelBuilder::new(topic)
            .context(&ctx)
            .message_encoding("json")
            .schema(Schema::new("S", "jsonschema", b"{}"))
            .build_raw()
            .unwrap()
    }

    // ---- participant management ----

    #[test]
    fn insert_new_participant_returns_true() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        assert!(state.insert_participant(id, p));
    }

    #[test]
    fn insert_duplicate_participant_returns_false() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        assert!(state.insert_participant(id.clone(), p.clone()));
        assert!(!state.insert_participant(id, p));
    }

    #[test]
    fn get_participant_returns_existing() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        assert!(state.get_participant(&id).is_some());
    }

    #[test]
    fn get_participant_returns_none_for_missing() {
        let state = SessionState::new();
        let id = ParticipantIdentity("nobody".to_string());
        assert!(state.get_participant(&id).is_none());
    }

    #[test]
    fn remove_missing_participant_is_noop() {
        let mut state = SessionState::new();
        let id = ParticipantIdentity("nobody".to_string());
        let last = state.remove_participant(&id);
        assert!(last.is_empty());
    }

    #[test]
    fn remove_participant_cleans_up_subscriptions() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p.clone());

        let ch = ChannelId::new(1);
        state.subscribe(&p, &[ch]);

        let last = state.remove_participant(&id);
        assert_eq!(last.as_slice(), &[ch]);
        assert!(state.get_subscriber_count(&ch).is_none());
    }

    #[test]
    fn remove_participant_reports_only_last_unsubscribed_channels() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        state.insert_participant(id_a.clone(), pa.clone());
        state.insert_participant(id_b.clone(), pb.clone());

        let ch1 = ChannelId::new(10);
        let ch2 = ChannelId::new(20);

        // Both subscribe to ch1; only alice subscribes to ch2
        state.subscribe(&pa, &[ch1, ch2]);
        state.subscribe(&pb, &[ch1]);

        let last = state.remove_participant(&id_a);
        // ch1 still has bob, so only ch2 should be reported
        assert_eq!(last.as_slice(), &[ch2]);
        assert_eq!(state.get_subscriber_count(&ch1), Some(1));
    }

    // ---- channel management ----

    #[test]
    fn insert_and_query_channel() {
        let mut state = SessionState::new();
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        assert!(state.channels().contains_key(&ch.id()));
    }

    #[test]
    fn remove_channel_returns_true_when_present() {
        let mut state = SessionState::new();
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        assert!(state.remove_channel(ch.id()));
    }

    #[test]
    fn remove_channel_returns_false_when_absent() {
        let mut state = SessionState::new();
        assert!(!state.remove_channel(ChannelId::new(999)));
    }

    // ---- subscription management ----

    #[test]
    fn first_subscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let first = state.subscribe(&p, &[ch]);
        assert_eq!(first.as_slice(), &[ch]);
    }

    #[test]
    fn second_subscriber_is_not_reported_as_first() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe(&pa, &[ch]);
        let first = state.subscribe(&pb, &[ch]);
        assert!(first.is_empty());
    }

    #[test]
    fn duplicate_subscribe_is_idempotent() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe(&p, &[ch]);
        let first = state.subscribe(&p, &[ch]);
        assert!(first.is_empty());
        assert_eq!(state.get_subscriber_count(&ch), Some(1));
    }

    #[test]
    fn subscribe_multiple_channels_at_once() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);

        let first = state.subscribe(&p, &[ch1, ch2]);
        assert_eq!(first.len(), 2);
        assert!(first.contains(&ch1));
        assert!(first.contains(&ch2));
    }

    #[test]
    fn last_unsubscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe(&p, &[ch]);
        let last = state.unsubscribe(&p, &[ch]);
        assert_eq!(last.as_slice(), &[ch]);
    }

    #[test]
    fn unsubscribe_with_remaining_subscribers_is_not_reported() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe(&pa, &[ch]);
        state.subscribe(&pb, &[ch]);

        let last = state.unsubscribe(&pa, &[ch]);
        assert!(last.is_empty());
        assert_eq!(state.get_subscriber_count(&ch), Some(1));
    }

    #[test]
    fn unsubscribe_when_not_subscribed_is_noop() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let last = state.unsubscribe(&p, &[ch]);
        assert!(last.is_empty());
    }

    #[test]
    fn unsubscribe_multiple_channels_at_once() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);

        state.subscribe(&p, &[ch1, ch2]);
        let last = state.unsubscribe(&p, &[ch1, ch2]);
        assert_eq!(last.len(), 2);
        assert!(last.contains(&ch1));
        assert!(last.contains(&ch2));
    }

    #[test]
    fn get_subscribers_returns_none_for_no_subscriptions() {
        let state = SessionState::new();
        assert!(state.get_subscribers(&ChannelId::new(1)).is_none());
    }

    #[test]
    fn get_subscribers_returns_subscriber_identities() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe(&pa, &[ch]);
        state.subscribe(&pb, &[ch]);

        let subs = state.get_subscribers(&ch).unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&id_a));
        assert!(subs.contains(&id_b));
    }

    #[test]
    fn participants_iterator_yields_all() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        state.insert_participant(id_a, pa);
        state.insert_participant(id_b, pb);
        assert_eq!(state.participants().count(), 2);
    }
}
