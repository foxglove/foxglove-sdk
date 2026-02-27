use std::collections::HashMap;
use std::sync::Arc;

use livekit::id::{ParticipantIdentity, TrackSid};
use smallvec::SmallVec;
use tracing::{debug, info};

use crate::protocol::v2::server::advertise;
use crate::remote_access::participant::Participant;
use crate::remote_access::session::{VideoInputSchema, VideoPublisher};
use crate::{ChannelId, RawChannel};

/// State machine for a remote access session.
///
/// Tracks participants, advertised channels, and per-channel subscriptions.
/// Contains no locking; callers are responsible for synchronization.
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
    /// Detected video input schemas for channels.
    video_schemas: HashMap<ChannelId, VideoInputSchema>,
    /// Active video publishers, keyed by channel ID.
    video_publishers: HashMap<ChannelId, Arc<VideoPublisher>>,
    /// Track SIDs for published video tracks.
    video_track_sids: HashMap<ChannelId, TrackSid>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            channels: HashMap::new(),
            subscriptions: HashMap::new(),
            video_schemas: HashMap::new(),
            video_publishers: HashMap::new(),
            video_track_sids: HashMap::new(),
        }
    }

    /// Inserts a participant if not already present.
    ///
    /// Returns the participant that is stored in the map — either the existing one
    /// (on conflict) or the newly inserted one. This avoids a TOCTOU race where a
    /// caller could receive a participant that isn't actually tracked by the session.
    pub fn insert_participant(
        &mut self,
        identity: ParticipantIdentity,
        participant: Arc<Participant>,
    ) -> Arc<Participant> {
        use std::collections::hash_map::Entry;
        match self.participants.entry(identity) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(v) => {
                v.insert(participant.clone());
                participant
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
    pub fn get_participant(&self, identity: &ParticipantIdentity) -> Option<Arc<Participant>> {
        self.participants.get(identity).cloned()
    }

    /// Collects and returns all current participants.
    pub fn collect_participants(&self) -> SmallVec<[Arc<Participant>; 8]> {
        self.participants.values().cloned().collect()
    }

    /// Records a channel as advertised.
    pub fn insert_channel(&mut self, channel: &Arc<RawChannel>) {
        self.channels.insert(channel.id(), channel.clone());
    }

    /// Removes an advertised channel. Returns `true` if it was present.
    pub fn remove_channel(&mut self, channel_id: ChannelId) -> bool {
        self.channels.remove(&channel_id).is_some()
    }

    /// Calls `f` with a reference to the advertised channels map.
    ///
    /// Returns `None` if the channels map is empty; otherwise returns `Some(f(&channels))`.
    pub fn with_channels<R>(
        &self,
        f: impl FnOnce(&HashMap<ChannelId, Arc<RawChannel>>) -> R,
    ) -> Option<R> {
        if self.channels.is_empty() {
            return None;
        }
        Some(f(&self.channels))
    }

    /// Records a video input schema for a channel.
    pub fn insert_video_schema(&mut self, channel_id: ChannelId, schema: VideoInputSchema) {
        self.video_schemas.insert(channel_id, schema);
    }

    /// Returns the video input schema for a channel, if any.
    pub fn get_video_schema(&self, channel_id: &ChannelId) -> Option<VideoInputSchema> {
        self.video_schemas.get(channel_id).copied()
    }

    /// Removes the video input schema for a channel.
    pub fn remove_video_schema(&mut self, channel_id: &ChannelId) {
        self.video_schemas.remove(channel_id);
    }

    /// Inserts a video publisher for a channel.
    pub fn insert_video_publisher(
        &mut self,
        channel_id: ChannelId,
        publisher: Arc<VideoPublisher>,
    ) {
        self.video_publishers.insert(channel_id, publisher);
    }

    /// Returns the video publisher for a channel, if any.
    pub fn get_video_publisher(&self, channel_id: &ChannelId) -> Option<Arc<VideoPublisher>> {
        self.video_publishers.get(channel_id).cloned()
    }

    /// Removes the video publisher for a channel.
    pub fn remove_video_publisher(&mut self, channel_id: &ChannelId) {
        self.video_publishers.remove(channel_id);
    }

    /// Inserts a track SID for a published video track.
    pub fn insert_video_track_sid(&mut self, channel_id: ChannelId, sid: TrackSid) {
        self.video_track_sids.insert(channel_id, sid);
    }

    /// Removes and returns the track SID for a channel, if any.
    pub fn remove_video_track_sid(&mut self, channel_id: &ChannelId) -> Option<TrackSid> {
        self.video_track_sids.remove(channel_id)
    }

    /// Annotates channels in an advertise message with video track metadata
    /// for channels that have a detected video schema.
    pub fn inject_video_track_metadata(&self, advertise: &mut advertise::Advertise<'_>) {
        for ch in &mut advertise.channels {
            if self.video_schemas.contains_key(&ChannelId::new(ch.id)) {
                ch.metadata
                    .insert("foxglove.hasVideoTrack".to_string(), "true".to_string());
            }
        }
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

    /// Collects subscriber identities for a channel, or returns `None` if no
    /// subscriptions exist.
    pub fn collect_subscribers(
        &self,
        channel_id: &ChannelId,
    ) -> Option<SmallVec<[ParticipantIdentity; 8]>> {
        self.subscriptions
            .get(channel_id)
            .map(|ids| ids.iter().cloned().collect())
    }

    /// Returns the number of subscribers for a channel.
    #[cfg(test)]
    pub fn get_subscriber_count(&self, channel_id: &ChannelId) -> usize {
        self.subscriptions.get(channel_id).map_or(0, |s| s.len())
    }

    /// Returns the number of advertised channels.
    #[cfg(test)]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
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
    fn insert_new_participant() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        let stored = state.insert_participant(id.clone(), p);
        assert_eq!(stored.identity(), &id);
        assert!(Arc::ptr_eq(&stored, &state.get_participant(&id).unwrap()));
    }

    #[test]
    fn insert_duplicate_returns_existing() {
        let mut state = SessionState::new();
        let (id, p1) = make_participant("alice");
        let stored1 = state.insert_participant(id.clone(), p1);
        let (_id2, p2) = make_participant("bob");
        let stored2 = state.insert_participant(id, p2);
        assert!(Arc::ptr_eq(&stored1, &stored2));
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
        assert_eq!(state.get_subscriber_count(&ch), 0);
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
        assert_eq!(state.get_subscriber_count(&ch1), 1);
    }

    // ---- channel management ----

    #[test]
    fn insert_and_query_channel() {
        let mut state = SessionState::new();
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        assert_eq!(state.channel_count(), 1);
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
        assert_eq!(state.get_subscriber_count(&ch), 1);
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
        assert_eq!(state.get_subscriber_count(&ch), 1);
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
    fn collect_subscribers_returns_none_for_no_subscriptions() {
        let state = SessionState::new();
        assert!(state.collect_subscribers(&ChannelId::new(1)).is_none());
    }

    #[test]
    fn collect_subscribers_returns_subscriber_identities() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe(&pa, &[ch]);
        state.subscribe(&pb, &[ch]);

        let subs = state.collect_subscribers(&ch).unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&id_a));
        assert!(subs.contains(&id_b));
    }

    #[test]
    fn collect_participants_yields_all() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        state.insert_participant(id_a, pa);
        state.insert_participant(id_b, pb);
        assert_eq!(state.collect_participants().len(), 2);
    }
}
