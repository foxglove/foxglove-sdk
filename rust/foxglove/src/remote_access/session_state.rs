use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use livekit::id::{ParticipantIdentity, TrackSid};
use smallvec::SmallVec;
use tracing::{debug, info};

use crate::protocol::v2::server::advertise;
use crate::remote_access::channel_subscription::ChannelSubscription;
use crate::remote_access::participant::Participant;
use crate::remote_access::session::{VideoInputSchema, VideoMetadata, VideoPublisher};
use crate::remote_common::ClientId;
use crate::{ChannelDescriptor, ChannelId, RawChannel};

/// Channels and parameters that lost their last subscriber when a participant was removed.
pub(crate) struct RemovedSubscriptions {
    /// The locally-significant client ID of the removed participant.
    pub client_id: Option<ClientId>,
    /// Channel IDs that lost their last subscriber (of any type).
    pub last_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// Channel IDs that lost their last video subscriber.
    pub last_video_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// Client channels that were advertised by the removed participant.
    pub client_channels: Vec<ChannelDescriptor>,
    /// Parameter names that lost their last subscriber.
    pub last_param_unsubscribed: Vec<String>,
}

/// Result of subscribing a participant to channels.
pub(crate) struct SubscribeResult {
    /// Channel IDs that gained their first subscriber.
    pub first_subscribed: SmallVec<[ChannelId; 4]>,
    /// All channel IDs where this participant was actually added.
    pub newly_subscribed: SmallVec<[ChannelId; 4]>,
}

/// Result of unsubscribing a participant from channels.
pub(crate) struct UnsubscribeResult {
    /// Channel IDs that lost their last subscriber.
    pub last_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// All channel IDs where this participant was actually removed.
    pub actually_unsubscribed: SmallVec<[ChannelId; 4]>,
}

/// State machine for a remote access session.
///
/// Tracks participants, advertised channels, and per-channel subscriptions.
/// Contains no locking; callers are responsible for synchronization.
///
/// Subscriptions are tracked in three maps:
/// - `subscriptions`: all subscribers regardless of type, for Context first/last notifications.
/// - `data_subscriptions`: data subscribers, for multicast `ChannelWriter` lifecycle.
/// - `video_subscribers`: video subscribers, for managing video track lifecycle.
pub(crate) struct SessionState {
    participants: HashMap<ParticipantIdentity, Arc<Participant>>,
    /// Channels that have been advertised to participants.
    channels: HashMap<ChannelId, Arc<RawChannel>>,
    /// All subscriber identities per channel, regardless of subscription type.
    subscriptions: HashMap<ChannelId, SmallVec<[ParticipantIdentity; 1]>>,
    /// Data subscriber identities and version counters per channel.
    data_subscriptions: HashMap<ChannelId, ChannelSubscription>,
    /// Video subscriber identities per channel.
    video_subscribers: HashMap<ChannelId, SmallVec<[ParticipantIdentity; 1]>>,
    /// Detected video input schemas for channels.
    video_schemas: HashMap<ChannelId, VideoInputSchema>,
    /// Active video publishers, keyed by channel ID.
    video_publishers: HashMap<ChannelId, Arc<VideoPublisher>>,
    /// Track SIDs for published video tracks.
    video_track_sids: HashMap<ChannelId, TrackSid>,
    /// Video metadata last advertised for each video channel.
    video_metadata: HashMap<ChannelId, VideoMetadata>,
    /// Client-advertised channels, keyed by participant identity then client-assigned channel ID.
    client_channels: HashMap<ParticipantIdentity, HashMap<ChannelId, ChannelDescriptor>>,
    /// Parameters subscribed to by participants, keyed by parameter name.
    subscribed_parameters: HashMap<String, HashSet<ParticipantIdentity>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            participants: HashMap::new(),
            channels: HashMap::new(),
            subscriptions: HashMap::new(),
            data_subscriptions: HashMap::new(),
            video_subscribers: HashMap::new(),
            video_schemas: HashMap::new(),
            video_publishers: HashMap::new(),
            video_track_sids: HashMap::new(),
            video_metadata: HashMap::new(),
            client_channels: HashMap::new(),
            subscribed_parameters: HashMap::new(),
        }
    }

    /// Inserts a participant if not already present.
    ///
    /// Returns true if this is a new participant, or false if there was already a participant
    /// registered with this identity.
    pub fn insert_participant(
        &mut self,
        identity: ParticipantIdentity,
        participant: Arc<Participant>,
    ) -> bool {
        use std::collections::hash_map::Entry;
        if let Entry::Vacant(v) = self.participants.entry(identity) {
            v.insert(participant);
            true
        } else {
            false
        }
    }

    /// Removes a participant and all of its subscriptions.
    ///
    /// Returns the channels that lost their last subscriber or video subscriber,
    /// and any client channels that were advertised by the participant.
    #[must_use]
    pub fn remove_participant(&mut self, identity: &ParticipantIdentity) -> RemovedSubscriptions {
        let Some(participant) = self.participants.remove(identity) else {
            return RemovedSubscriptions {
                client_id: None,
                last_unsubscribed: SmallVec::new(),
                last_video_unsubscribed: SmallVec::new(),
                client_channels: Vec::new(),
                last_param_unsubscribed: Vec::new(),
            };
        };
        let client_id = participant.client_id();
        info!("removed participant {identity:?}");

        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for (&channel_id, subscribers) in &mut self.subscriptions {
            if let Some(pos) = subscribers.iter().position(|id| id == identity) {
                subscribers.swap_remove(pos);
                if subscribers.is_empty() {
                    last_unsubscribed.push(channel_id);
                }
            }
        }

        for sub in self.data_subscriptions.values_mut() {
            sub.remove(identity);
        }

        let mut last_video_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        self.video_subscribers.retain(|&channel_id, subscribers| {
            subscribers.retain(|id| id != identity);
            if subscribers.is_empty() {
                last_video_unsubscribed.push(channel_id);
                false
            } else {
                true
            }
        });

        let client_channels = self
            .client_channels
            .remove(identity)
            .map(|map| map.into_values().collect())
            .unwrap_or_default();

        let mut last_param_unsubscribed = Vec::new();
        self.subscribed_parameters.retain(|name, subscribers| {
            subscribers.remove(identity);
            if subscribers.is_empty() {
                last_param_unsubscribed.push(name.clone());
                false
            } else {
                true
            }
        });

        RemovedSubscriptions {
            client_id: Some(client_id),
            last_unsubscribed,
            last_video_unsubscribed,
            client_channels,
            last_param_unsubscribed,
        }
    }

    /// Returns the participant for the given identity, if present.
    pub fn get_participant(&self, identity: &ParticipantIdentity) -> Option<Arc<Participant>> {
        self.participants.get(identity).cloned()
    }

    /// Returns true if there is a participant for the given identity.
    pub fn has_participant(&self, identity: &ParticipantIdentity) -> bool {
        self.participants.contains_key(identity)
    }

    /// Collects and returns all current participants.
    pub fn collect_participants(&self) -> SmallVec<[Arc<Participant>; 8]> {
        self.participants.values().cloned().collect()
    }

    /// Records a client-advertised channel for a participant.
    ///
    /// Returns `true` if the channel was inserted, or `false` if the participant already
    /// had a channel with the same ID advertised.
    pub fn insert_client_channel(
        &mut self,
        identity: &ParticipantIdentity,
        channel: ChannelDescriptor,
    ) -> bool {
        debug_assert!(
            self.participants.contains_key(identity),
            "Participant does not exist for identity: {identity:?}"
        );
        if !self.participants.contains_key(identity) {
            return false;
        }
        let map = self.client_channels.entry(identity.clone()).or_default();
        match map.entry(channel.id()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(v) => {
                v.insert(channel);
                true
            }
        }
    }

    /// Returns a client-advertised channel for a participant, if present.
    pub fn get_client_channel(
        &self,
        identity: &ParticipantIdentity,
        channel_id: ChannelId,
    ) -> Option<&ChannelDescriptor> {
        self.client_channels.get(identity)?.get(&channel_id)
    }

    /// Removes and returns a client-advertised channel for a participant.
    pub fn remove_client_channel(
        &mut self,
        identity: &ParticipantIdentity,
        channel_id: ChannelId,
    ) -> Option<ChannelDescriptor> {
        let map = self.client_channels.get_mut(identity)?;
        let descriptor = map.remove(&channel_id)?;
        if map.is_empty() {
            self.client_channels.remove(identity);
        }
        Some(descriptor)
    }

    /// Returns the descriptor for an advertised server channel.
    pub fn get_channel_descriptor(&self, channel_id: &ChannelId) -> Option<&ChannelDescriptor> {
        self.channels.get(channel_id).map(|ch| ch.descriptor())
    }

    /// Returns all channel IDs that the given participant is subscribed to.
    pub fn subscribed_channel_ids(
        &self,
        identity: &ParticipantIdentity,
    ) -> SmallVec<[ChannelId; 4]> {
        self.subscriptions
            .iter()
            .filter_map(|(&channel_id, subscribers)| {
                if subscribers.contains(identity) {
                    Some(channel_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Records a channel as advertised.
    pub fn insert_channel(&mut self, channel: &Arc<RawChannel>) {
        self.channels.insert(channel.id(), channel.clone());
    }

    /// Returns `true` if the channel is currently advertised.
    pub fn has_channel(&self, channel_id: &ChannelId) -> bool {
        self.channels.contains_key(channel_id)
    }

    /// Removes an advertised channel. Returns `true` if it was present.
    pub fn remove_channel(&mut self, channel_id: ChannelId) -> bool {
        self.subscriptions.remove(&channel_id);
        self.data_subscriptions.remove(&channel_id);
        self.video_subscribers.remove(&channel_id);
        self.video_metadata.remove(&channel_id);
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

    /// Removes the video input schema and associated video metadata for a channel.
    pub fn remove_video_schema(&mut self, channel_id: &ChannelId) {
        self.video_schemas.remove(channel_id);
        self.video_metadata.remove(channel_id);
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

    /// Returns an iterator over video publishers keyed by channel ID.
    pub fn iter_video_publishers(
        &self,
    ) -> impl Iterator<Item = (&ChannelId, &Arc<VideoPublisher>)> {
        self.video_publishers.iter()
    }

    /// Stores video metadata for a video channel.
    pub fn insert_video_metadata(&mut self, channel_id: ChannelId, metadata: VideoMetadata) {
        self.video_metadata.insert(channel_id, metadata);
    }

    /// Removes video metadata for a video channel.
    #[cfg(test)]
    pub fn remove_video_metadata(&mut self, channel_id: &ChannelId) {
        self.video_metadata.remove(channel_id);
    }

    /// Annotates channels in an advertise message with video metadata for channels that have a
    /// detected video schema.
    pub fn add_metadata_to_advertisement(&self, advertise: &mut advertise::Advertise<'_>) {
        for ch in &mut advertise.channels {
            let channel_id = ChannelId::new(ch.id);
            if self.video_schemas.contains_key(&channel_id) {
                ch.metadata
                    .insert("foxglove.hasVideoTrack".to_string(), "true".to_string());
            }
            if let Some(meta) = self.video_metadata.get(&channel_id) {
                ch.metadata.insert(
                    "foxglove.videoSourceEncoding".to_string(),
                    meta.encoding.as_str().to_string(),
                );
                if !meta.frame_id.is_empty() {
                    ch.metadata
                        .insert("foxglove.videoFrameId".to_string(), meta.frame_id.clone());
                }
            }
        }
    }

    /// Subscribes a participant to the given channels.
    ///
    /// Returns two sets of channel IDs:
    /// - `first_subscribed`: channels that gained their first subscriber (for context notifications).
    /// - `newly_subscribed`: all channels where this participant was actually added (for listener
    ///   callbacks). This excludes channels the participant was already subscribed to.
    #[must_use]
    pub fn subscribe(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> SubscribeResult {
        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        let mut newly_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let subscribers = self.subscriptions.entry(channel_id).or_default();
            if subscribers.contains(participant.participant_id()) {
                info!("{participant} is already subscribed to channel {channel_id:?}; ignoring");
                continue;
            }
            let is_first = subscribers.is_empty();
            subscribers.push(participant.participant_id().clone());
            debug!("{participant} subscribed to channel {channel_id:?}");
            newly_subscribed.push(channel_id);
            if is_first {
                first_subscribed.push(channel_id);
            }
        }
        SubscribeResult {
            first_subscribed,
            newly_subscribed,
        }
    }

    /// Unsubscribes a participant from the given channels.
    ///
    /// Returns two sets of channel IDs:
    /// - `last_unsubscribed`: channels that lost their last subscriber (for context notifications).
    /// - `actually_unsubscribed`: all channels where this participant was actually removed (for
    ///   listener callbacks). This excludes channels the participant was not subscribed to.
    #[must_use]
    pub fn unsubscribe(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> UnsubscribeResult {
        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        let mut actually_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let Some(subscribers) = self.subscriptions.get_mut(&channel_id) else {
                info!("{participant} is not subscribed to channel {channel_id:?}; ignoring");
                continue;
            };
            let Some(pos) = subscribers
                .iter()
                .position(|id| id == participant.participant_id())
            else {
                info!("{participant} is not subscribed to channel {channel_id:?}; ignoring");
                continue;
            };
            subscribers.swap_remove(pos);
            debug!("{participant} unsubscribed from channel {channel_id:?}");
            actually_unsubscribed.push(channel_id);
            if subscribers.is_empty() {
                last_unsubscribed.push(channel_id);
            }
        }
        UnsubscribeResult {
            last_unsubscribed,
            actually_unsubscribed,
        }
    }

    /// Adds a participant to data subscriptions for the given channels.
    ///
    /// The caller is responsible for calling [`Self::subscribe`] separately, if necessary.
    pub fn subscribe_data(&mut self, participant: &Participant, channel_ids: &[ChannelId]) {
        for &channel_id in channel_ids {
            let sub = self
                .data_subscriptions
                .entry(channel_id)
                .or_insert_with(ChannelSubscription::new);
            if sub.subscribers().contains(participant.participant_id()) {
                continue;
            }
            sub.add(participant.participant_id().clone());
        }
    }

    /// Removes a participant from data subscriptions for the given channels.
    ///
    /// The caller is responsible for calling [`Self::unsubscribe`] separately, if necessary.
    pub fn unsubscribe_data(&mut self, participant: &Participant, channel_ids: &[ChannelId]) {
        for &channel_id in channel_ids {
            let Some(sub) = self.data_subscriptions.get_mut(&channel_id) else {
                continue;
            };
            if !sub.remove(participant.participant_id()) {
                continue;
            }
        }
    }

    /// Returns the number of connected participants.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Returns the total number of active participant subscriptions across all channels.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.values().map(|s| s.len()).sum()
    }

    /// Returns the number of active video tracks being published.
    pub fn video_track_count(&self) -> usize {
        self.video_track_sids.len()
    }

    /// Adds a participant to video subscribers for the given channels.
    ///
    /// The caller is responsible for calling [`Self::subscribe`] separately, if necessary.
    ///
    /// Returns channel IDs that gained their first video subscriber.
    #[must_use]
    pub fn subscribe_video(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let subscribers = self.video_subscribers.entry(channel_id).or_default();
            if subscribers.contains(participant.participant_id()) {
                continue;
            }
            let is_first = subscribers.is_empty();
            subscribers.push(participant.participant_id().clone());
            if is_first {
                first_subscribed.push(channel_id);
            }
        }
        first_subscribed
    }

    /// Removes a participant from video subscribers for the given channels.
    ///
    /// The caller is responsible for calling [`Self::unsubscribe`] separately, if necessary.
    ///
    /// Returns channel IDs that lost their last video subscriber.
    #[must_use]
    pub fn unsubscribe_video(
        &mut self,
        participant: &Participant,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let Some(subscribers) = self.video_subscribers.get_mut(&channel_id) else {
                continue;
            };
            let Some(pos) = subscribers
                .iter()
                .position(|id| id == participant.participant_id())
            else {
                continue;
            };
            subscribers.swap_remove(pos);
            if subscribers.is_empty() {
                self.video_subscribers.remove(&channel_id);
                last_unsubscribed.push(channel_id);
            }
        }
        last_unsubscribed
    }

    /// Returns true if a channel has any data subscribers.
    pub fn has_data_subscribers(&self, channel_id: &ChannelId) -> bool {
        self.get_data_subscription(channel_id).is_some()
    }

    /// Returns the data subscription for a channel, if it has subscribers.
    pub fn get_data_subscription(&self, channel_id: &ChannelId) -> Option<&ChannelSubscription> {
        let sub = self.data_subscriptions.get(channel_id)?;
        if sub.is_empty() { None } else { Some(sub) }
    }

    /// Add parameter subscriptions for a participant.
    ///
    /// Returns parameter names that are newly subscribed (i.e. had no prior subscribers).
    pub fn subscribe_parameters(
        &mut self,
        identity: &ParticipantIdentity,
        names: Vec<String>,
    ) -> Vec<String> {
        let mut new_names = Vec::new();
        for name in names {
            let subscribers = self.subscribed_parameters.entry(name.clone()).or_default();
            if subscribers.insert(identity.clone()) && subscribers.len() == 1 {
                new_names.push(name);
            }
        }
        new_names
    }

    /// Remove parameter subscriptions for a participant.
    ///
    /// Returns parameter names that lost their last subscriber.
    pub fn unsubscribe_parameters(
        &mut self,
        identity: &ParticipantIdentity,
        names: Vec<String>,
    ) -> Vec<String> {
        let mut old_names = Vec::new();
        for name in names {
            if let Some(subscribers) = self.subscribed_parameters.get_mut(&name) {
                subscribers.remove(identity);
                if subscribers.is_empty() {
                    self.subscribed_parameters.remove(&name);
                    old_names.push(name);
                }
            }
        }
        old_names
    }

    /// Returns the set of participant identities subscribed to a parameter.
    pub fn parameter_subscribers(&self, name: &str) -> Option<&HashSet<ParticipantIdentity>> {
        self.subscribed_parameters.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::img2yuv::{ImageEncoding, RawImageEncoding};
    use crate::remote_access::participant::ParticipantWriter;

    fn make_participant(name: &str) -> (ParticipantIdentity, Arc<Participant>) {
        let identity = ParticipantIdentity(name.to_string());
        let writer = Arc::new(crate::remote_access::participant::TestByteStreamWriter::default());
        let version =
            crate::remote_access::protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let participant = Arc::new(Participant::new(
            identity.clone(),
            version,
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

    #[test]
    fn insert_new_participant() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        assert!(state.insert_participant(id.clone(), p));
    }

    #[test]
    fn insert_existing_participant() {
        let mut state = SessionState::new();
        let (id, p1) = make_participant("alice");
        assert!(state.insert_participant(id.clone(), p1));
        let (_, p2) = make_participant("bob");
        assert!(!state.insert_participant(id, p2));
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
        let removed = state.remove_participant(&id);
        assert!(removed.last_unsubscribed.is_empty());
        assert!(removed.last_video_unsubscribed.is_empty());
    }

    #[test]
    fn remove_participant_cleans_up_subscriptions() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p.clone());

        let ch = ChannelId::new(1);
        let _ = state.subscribe(&p, &[ch]);
        state.subscribe_data(&p, &[ch]);

        let removed = state.remove_participant(&id);
        assert_eq!(removed.last_unsubscribed.as_slice(), &[ch]);
        assert!(!state.has_data_subscribers(&ch));
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
        let _ = state.subscribe(&pa, &[ch1, ch2]);
        state.subscribe_data(&pa, &[ch1, ch2]);
        let _ = state.subscribe(&pb, &[ch1]);
        state.subscribe_data(&pb, &[ch1]);

        let removed = state.remove_participant(&id_a);
        // ch1 still has bob, so only ch2 should be reported
        assert_eq!(removed.last_unsubscribed.as_slice(), &[ch2]);
        assert_eq!(state.subscriptions[&ch1].len(), 1);
    }

    #[test]
    fn remove_participant_cleans_up_video_subscriptions() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p.clone());

        let ch = ChannelId::new(1);
        let _ = state.subscribe(&p, &[ch]);
        let _ = state.subscribe_video(&p, &[ch]);

        let removed = state.remove_participant(&id);
        assert_eq!(removed.last_unsubscribed.as_slice(), &[ch]);
        assert_eq!(removed.last_video_unsubscribed.as_slice(), &[ch]);
    }

    #[test]
    fn insert_and_query_channel() {
        let mut state = SessionState::new();
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        assert_eq!(state.channels.len(), 1);
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

    #[test]
    fn first_subscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let result = state.subscribe(&p, &[ch]);
        assert_eq!(result.first_subscribed.as_slice(), &[ch]);
        assert_eq!(result.newly_subscribed.as_slice(), &[ch]);
    }

    #[test]
    fn second_subscriber_is_not_reported_as_first() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        let _ = state.subscribe(&pa, &[ch]);
        let result = state.subscribe(&pb, &[ch]);
        assert!(result.first_subscribed.is_empty());
        assert_eq!(result.newly_subscribed.as_slice(), &[ch]);
    }

    #[test]
    fn duplicate_subscribe_is_idempotent() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe(&p, &[ch]);
        let result = state.subscribe(&p, &[ch]);
        assert!(result.first_subscribed.is_empty());
        assert!(result.newly_subscribed.is_empty());
        assert_eq!(state.subscriptions[&ch].len(), 1);
    }

    #[test]
    fn subscribe_multiple_channels_at_once() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);

        let result = state.subscribe(&p, &[ch1, ch2]);
        assert_eq!(result.first_subscribed.len(), 2);
        assert!(result.first_subscribed.contains(&ch1));
        assert!(result.first_subscribed.contains(&ch2));
        assert_eq!(result.newly_subscribed.len(), 2);
    }

    #[test]
    fn last_unsubscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe(&p, &[ch]);
        let result = state.unsubscribe(&p, &[ch]);
        assert_eq!(result.last_unsubscribed.as_slice(), &[ch]);
        assert_eq!(result.actually_unsubscribed.as_slice(), &[ch]);
    }

    #[test]
    fn unsubscribe_with_remaining_subscribers_is_not_reported() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        let _ = state.subscribe(&pa, &[ch]);
        let _ = state.subscribe(&pb, &[ch]);

        let result = state.unsubscribe(&pa, &[ch]);
        assert!(result.last_unsubscribed.is_empty());
        assert_eq!(result.actually_unsubscribed.as_slice(), &[ch]);
        assert_eq!(state.subscriptions[&ch].len(), 1);
    }

    #[test]
    fn unsubscribe_when_not_subscribed_is_noop() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let result = state.unsubscribe(&p, &[ch]);
        assert!(result.last_unsubscribed.is_empty());
        assert!(result.actually_unsubscribed.is_empty());
    }

    #[test]
    fn unsubscribe_multiple_channels_at_once() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);

        let _ = state.subscribe(&p, &[ch1, ch2]);
        let result = state.unsubscribe(&p, &[ch1, ch2]);
        assert_eq!(result.last_unsubscribed.len(), 2);
        assert!(result.last_unsubscribed.contains(&ch1));
        assert!(result.last_unsubscribed.contains(&ch2));
        assert_eq!(result.actually_unsubscribed.len(), 2);
    }

    #[test]
    fn get_data_subscription_returns_none_for_no_subscriptions() {
        let state = SessionState::new();
        assert!(state.get_data_subscription(&ChannelId::new(1)).is_none());
    }

    #[test]
    fn get_data_subscription_returns_subscriber_identities() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe_data(&pa, &[ch]);
        state.subscribe_data(&pb, &[ch]);

        let sub = state.get_data_subscription(&ch).unwrap();
        assert_eq!(sub.subscribers().len(), 2);
        assert!(sub.subscribers().contains(&id_a));
        assert!(sub.subscribers().contains(&id_b));
    }

    #[test]
    fn subscription_version_increments_on_subscribe() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe_data(&pa, &[ch]);
        let v1 = state.get_data_subscription(&ch).unwrap().version();

        state.subscribe_data(&pb, &[ch]);
        let v2 = state.get_data_subscription(&ch).unwrap().version();

        assert_ne!(v1, v2);
    }

    #[test]
    fn subscription_version_does_not_increment_on_duplicate_subscribe() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe_data(&p, &[ch]);
        let v1 = state.get_data_subscription(&ch).unwrap().version();

        state.subscribe_data(&p, &[ch]);
        let v2 = state.get_data_subscription(&ch).unwrap().version();

        assert_eq!(v1, v2);
    }

    #[test]
    fn subscription_version_increments_on_unsubscribe() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.subscribe_data(&pa, &[ch]);
        state.subscribe_data(&pb, &[ch]);
        let v1 = state.get_data_subscription(&ch).unwrap().version();

        state.unsubscribe_data(&pa, &[ch]);
        let v2 = state.get_data_subscription(&ch).unwrap().version();

        assert_ne!(v1, v2);
    }

    #[test]
    fn subscription_version_increments_on_remove_participant() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        state.insert_participant(id_a.clone(), pa.clone());
        state.insert_participant(id_b, pb.clone());

        let _ = state.subscribe(&pa, &[ch]);
        let _ = state.subscribe(&pb, &[ch]);
        state.subscribe_data(&pa, &[ch]);
        state.subscribe_data(&pb, &[ch]);
        let v1 = state.get_data_subscription(&ch).unwrap().version();

        let _ = state.remove_participant(&id_a);
        let v2 = state.get_data_subscription(&ch).unwrap().version();

        assert_ne!(v1, v2);
    }

    #[test]
    fn version_preserved_across_unsubscribe_resubscribe() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe_data(&p, &[ch]);
        let v1 = state.data_subscriptions.get(&ch).unwrap().version();

        state.unsubscribe_data(&p, &[ch]);
        // Entry should still exist with a bumped version.
        let v2 = state.data_subscriptions.get(&ch).unwrap().version();
        assert_ne!(v1, v2, "unsubscribe should bump version");

        state.subscribe_data(&p, &[ch]);
        let v3 = state.data_subscriptions.get(&ch).unwrap().version();
        assert_ne!(v2, v3, "resubscribe should bump version");
        assert_ne!(v1, v3, "resubscribe version should differ from original");
    }

    #[test]
    fn version_preserved_across_remove_participant_resubscribe() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p.clone());

        let ch = ChannelId::new(1);
        let _ = state.subscribe(&p, &[ch]);
        state.subscribe_data(&p, &[ch]);
        let v1 = state.data_subscriptions.get(&ch).unwrap().version();

        let _ = state.remove_participant(&id);
        let v2 = state.data_subscriptions.get(&ch).unwrap().version();
        assert_ne!(v1, v2, "remove_participant should bump version");

        // Re-add participant and resubscribe.
        let (id2, p2) = make_participant("alice");
        state.insert_participant(id2, p2.clone());
        state.subscribe_data(&p2, &[ch]);
        let v3 = state.data_subscriptions.get(&ch).unwrap().version();
        assert_ne!(v2, v3, "resubscribe after remove should bump version");
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

    #[test]
    fn first_video_subscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let first = state.subscribe_video(&p, &[ch]);
        assert_eq!(first.as_slice(), &[ch]);
    }

    #[test]
    fn second_video_subscriber_is_not_reported_as_first() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&pa, &[ch]);
        let first = state.subscribe_video(&pb, &[ch]);
        assert!(first.is_empty());
    }

    #[test]
    fn duplicate_video_subscribe_is_idempotent() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&p, &[ch]);
        let first = state.subscribe_video(&p, &[ch]);
        assert!(first.is_empty());
    }

    #[test]
    fn last_video_unsubscriber_is_reported() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&p, &[ch]);
        let last = state.unsubscribe_video(&p, &[ch]);
        assert_eq!(last.as_slice(), &[ch]);
    }

    #[test]
    fn video_unsubscribe_with_remaining_is_not_reported() {
        let mut state = SessionState::new();
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&pa, &[ch]);
        let _ = state.subscribe_video(&pb, &[ch]);

        let last = state.unsubscribe_video(&pa, &[ch]);
        assert!(last.is_empty());
    }

    #[test]
    fn video_unsubscribe_when_not_subscribed_is_noop() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let last = state.unsubscribe_video(&p, &[ch]);
        assert!(last.is_empty());
    }

    #[test]
    fn no_subscribers_means_no_data_subscribers() {
        let state = SessionState::new();
        let ch = ChannelId::new(1);
        assert!(!state.has_data_subscribers(&ch));
        assert!(state.get_data_subscription(&ch).is_none());
    }

    #[test]
    fn data_only_subscriber() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe_data(&p, &[ch]);
        assert!(state.has_data_subscribers(&ch));
        let subs = state.get_data_subscription(&ch).unwrap();
        assert_eq!(subs.subscribers().len(), 1);
        assert!(subs.subscribers().contains(&id));
    }

    #[test]
    fn video_only_subscriber_is_not_a_data_subscriber() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&p, &[ch]);
        assert!(!state.has_data_subscribers(&ch));
        assert!(state.get_data_subscription(&ch).is_none());
    }

    #[test]
    fn mixed_subscribers_data_and_video() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);

        // Alice wants video, Bob wants data.
        let _ = state.subscribe_video(&pa, &[ch]);
        state.subscribe_data(&pb, &[ch]);

        assert!(state.has_data_subscribers(&ch));
        let subs = state.get_data_subscription(&ch).unwrap();
        assert_eq!(subs.subscribers().len(), 1);
        assert!(subs.subscribers().contains(&id_b));
        assert!(!subs.subscribers().contains(&id_a));
    }

    #[test]
    fn switching_from_video_to_data() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        let _ = state.subscribe_video(&p, &[ch]);
        assert!(!state.has_data_subscribers(&ch));

        // Alice switches to data.
        let _ = state.unsubscribe_video(&p, &[ch]);
        state.subscribe_data(&p, &[ch]);
        assert!(state.has_data_subscribers(&ch));
        let subs = state.get_data_subscription(&ch).unwrap();
        assert!(subs.subscribers().contains(&id));
    }

    #[test]
    fn switching_from_data_to_video() {
        let mut state = SessionState::new();
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);

        state.subscribe_data(&p, &[ch]);
        assert!(state.has_data_subscribers(&ch));

        // Alice switches to video.
        state.unsubscribe_data(&p, &[ch]);
        let _ = state.subscribe_video(&p, &[ch]);
        assert!(!state.has_data_subscribers(&ch));
        assert!(state.video_subscribers.contains_key(&ch));
    }

    #[test]
    fn remove_participant_with_mixed_video_preferences() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        state.insert_participant(id_a.clone(), pa.clone());
        state.insert_participant(id_b.clone(), pb.clone());

        let ch = ChannelId::new(1);
        // alice=video, bob=data — both in the unified map
        let _ = state.subscribe(&pa, &[ch]);
        let _ = state.subscribe(&pb, &[ch]);
        let _ = state.subscribe_video(&pa, &[ch]);
        state.subscribe_data(&pb, &[ch]);

        // Remove alice: channel keeps bob, but loses its last video subscriber.
        let removed = state.remove_participant(&id_a);
        assert!(removed.last_unsubscribed.is_empty(), "bob still subscribed");
        assert_eq!(removed.last_video_unsubscribed.as_slice(), &[ch]);

        // Bob is the only subscriber and he's a data subscriber.
        assert!(state.has_data_subscribers(&ch));
    }

    #[test]
    fn add_metadata_to_advertisement_injects_video_metadata() {
        let mut state = SessionState::new();
        let ch = make_channel("/camera");
        state.insert_channel(&ch);
        state.insert_video_schema(ch.id(), VideoInputSchema::FoxgloveRawImage);

        // Before any video metadata, only hasVideoTrack should be present.
        let mut msg = advertise::advertise_channels(std::iter::once(&ch)).into_owned();
        state.add_metadata_to_advertisement(&mut msg);
        assert_eq!(msg.channels.len(), 1);
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.hasVideoTrack"),
            Some(&"true".to_string()),
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoSourceEncoding")
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoFrameId")
        );

        // After inserting video metadata, encoding and frame_id should appear.
        state.insert_video_metadata(
            ch.id(),
            VideoMetadata {
                encoding: ImageEncoding::Raw(RawImageEncoding::Rgb8),
                frame_id: "camera_optical_frame".to_string(),
            },
        );
        let mut msg = advertise::advertise_channels(std::iter::once(&ch)).into_owned();
        state.add_metadata_to_advertisement(&mut msg);
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.videoSourceEncoding"),
            Some(&"rgb8".to_string()),
        );
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.videoFrameId"),
            Some(&"camera_optical_frame".to_string()),
        );
    }

    #[test]
    fn add_metadata_to_advertisement_omits_empty_frame_id() {
        let mut state = SessionState::new();
        let ch = make_channel("/camera");
        state.insert_channel(&ch);
        state.insert_video_schema(ch.id(), VideoInputSchema::FoxgloveRawImage);
        state.insert_video_metadata(
            ch.id(),
            VideoMetadata {
                encoding: ImageEncoding::Raw(RawImageEncoding::Mono8),
                frame_id: String::new(),
            },
        );
        let mut msg = advertise::advertise_channels(std::iter::once(&ch)).into_owned();
        state.add_metadata_to_advertisement(&mut msg);
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.videoSourceEncoding"),
            Some(&"mono8".to_string()),
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoFrameId"),
            "empty frame_id should not be advertised"
        );
    }

    #[test]
    fn remove_video_metadata_clears_from_advertisement() {
        let mut state = SessionState::new();
        let ch = make_channel("/camera");
        state.insert_channel(&ch);
        state.insert_video_schema(ch.id(), VideoInputSchema::FoxgloveRawImage);
        state.insert_video_metadata(
            ch.id(),
            VideoMetadata {
                encoding: ImageEncoding::Raw(RawImageEncoding::Rgb8),
                frame_id: "frame".to_string(),
            },
        );
        state.remove_video_metadata(&ch.id());

        let mut msg = advertise::advertise_channels(std::iter::once(&ch)).into_owned();
        state.add_metadata_to_advertisement(&mut msg);
        // hasVideoTrack should still be present (schema persists), but metadata should be gone.
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.hasVideoTrack"),
            Some(&"true".to_string()),
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoSourceEncoding")
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoFrameId")
        );
    }

    #[test]
    fn remove_participant_video_subscriber_while_other_video_remains() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (id_b, pb) = make_participant("bob");
        state.insert_participant(id_a.clone(), pa.clone());
        state.insert_participant(id_b.clone(), pb.clone());

        let ch = ChannelId::new(1);
        let _ = state.subscribe(&pa, &[ch]);
        let _ = state.subscribe(&pb, &[ch]);
        let _ = state.subscribe_video(&pa, &[ch]);
        let _ = state.subscribe_video(&pb, &[ch]);

        // Remove alice: bob still has video.
        let removed = state.remove_participant(&id_a);
        assert!(removed.last_unsubscribed.is_empty());
        assert!(removed.last_video_unsubscribed.is_empty());
        assert!(!state.has_data_subscribers(&ch));
    }

    fn make_client_channel(channel_id: u64, topic: &str) -> ChannelDescriptor {
        ChannelDescriptor::new(
            ChannelId::new(channel_id),
            topic.to_string(),
            "json".to_string(),
            Default::default(),
            None,
        )
    }

    #[test]
    fn insert_client_channel_succeeds_for_new_channel() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        let ch = make_client_channel(1, "/cmd");

        assert!(state.insert_client_channel(&id, ch));
    }

    #[test]
    fn insert_client_channel_returns_false_for_duplicate() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        let ch = make_client_channel(1, "/cmd");

        assert!(state.insert_client_channel(&id, ch.clone()));
        assert!(!state.insert_client_channel(&id, ch));
    }

    #[test]
    fn remove_client_channel_returns_descriptor() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        let ch = make_client_channel(1, "/cmd");

        state.insert_client_channel(&id, ch);
        let removed = state.remove_client_channel(&id, ChannelId::new(1));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().topic(), "/cmd");
    }

    #[test]
    fn remove_client_channel_returns_none_for_unknown_channel() {
        let mut state = SessionState::new();
        let (id, _) = make_participant("alice");

        assert!(
            state
                .remove_client_channel(&id, ChannelId::new(99))
                .is_none()
        );
    }

    #[test]
    fn remove_participant_cleans_up_client_channels() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);

        state.insert_client_channel(&id, make_client_channel(1, "/cmd_vel"));
        state.insert_client_channel(&id, make_client_channel(2, "/joy"));

        let removed = state.remove_participant(&id);
        assert_eq!(removed.client_channels.len(), 2);
        // Channel map entry should be cleaned up.
        assert!(
            state
                .remove_client_channel(&id, ChannelId::new(1))
                .is_none()
        );
    }

    #[test]
    fn remove_participant_with_no_client_channels_yields_empty_vec() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);

        let removed = state.remove_participant(&id);
        assert!(removed.client_channels.is_empty());
    }

    #[test]
    fn get_client_channel_returns_channel() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        let ch = make_client_channel(1, "/cmd");

        state.insert_client_channel(&id, ch);

        let result = state.get_client_channel(&id, ChannelId::new(1));
        assert!(result.is_some());
        assert_eq!(result.unwrap().topic(), "/cmd");
    }

    #[test]
    fn get_client_channel_returns_none_for_unknown_participant() {
        let state = SessionState::new();
        let id = ParticipantIdentity("nobody".to_string());
        assert!(state.get_client_channel(&id, ChannelId::new(1)).is_none());
    }

    #[test]
    fn get_client_channel_returns_none_for_unknown_channel() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p);
        state.insert_client_channel(&id, make_client_channel(1, "/cmd"));
        assert!(state.get_client_channel(&id, ChannelId::new(99)).is_none());
    }

    #[test]
    fn get_channel_descriptor_returns_descriptor() {
        let mut state = SessionState::new();
        let ch = make_channel("/topic1");
        let ch_id = ch.id();
        state.insert_channel(&ch);

        let result = state.get_channel_descriptor(&ch_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().topic(), "/topic1");
    }

    #[test]
    fn get_channel_descriptor_returns_none_for_unknown() {
        let state = SessionState::new();
        assert!(state.get_channel_descriptor(&ChannelId::new(999)).is_none());
    }

    #[test]
    fn subscribed_channel_ids_returns_all_subscriptions() {
        let mut state = SessionState::new();
        let (id, p) = make_participant("alice");
        state.insert_participant(id.clone(), p.clone());

        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);
        let _ = state.subscribe(&p, &[ch1, ch2]);

        let ids = state.subscribed_channel_ids(&id);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&ch1));
        assert!(ids.contains(&ch2));
    }

    #[test]
    fn subscribed_channel_ids_returns_empty_for_unknown_participant() {
        let state = SessionState::new();
        let id = ParticipantIdentity("nobody".to_string());
        assert!(state.subscribed_channel_ids(&id).is_empty());
    }

    #[test]
    fn subscribed_channel_ids_excludes_other_participants() {
        let mut state = SessionState::new();
        let (id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");

        let ch1 = ChannelId::new(1);
        let ch2 = ChannelId::new(2);
        let _ = state.subscribe(&pa, &[ch1]);
        let _ = state.subscribe(&pb, &[ch2]);

        let ids = state.subscribed_channel_ids(&id_a);
        assert_eq!(ids.as_slice(), &[ch1]);
    }
}
