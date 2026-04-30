use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use livekit::id::{ParticipantIdentity, TrackSid};
use smallvec::SmallVec;
use tracing::{debug, info};

use crate::protocol::v2::server::advertise;

use crate::remote_access::qos::{QosProfile, Reliability};
use crate::remote_access::session::{DataTrack, VideoInputSchema, VideoMetadata, VideoPublisher};
use crate::{ChannelDescriptor, ChannelId, RawChannel};

/// Channels and parameters that lost their last subscriber when a participant was removed.
pub(super) struct RemovedSubscriptions {
    /// Channel IDs that lost their last subscriber (of any type).
    pub(super) last_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// Channel IDs that lost their last video subscriber.
    pub(super) last_video_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// Descriptors for all channels the participant was subscribed to at removal time.
    pub(super) subscribed_descriptors: SmallVec<[ChannelDescriptor; 4]>,
    /// Client channels that were advertised by the removed participant.
    pub(super) client_channels: Vec<ChannelDescriptor>,
    /// Parameter names that lost their last subscriber.
    pub(super) last_param_unsubscribed: Vec<String>,
}

/// Result of subscribing a participant to channels.
pub(super) struct SubscribeResult {
    /// Channel IDs that gained their first subscriber.
    pub(super) first_subscribed: SmallVec<[ChannelId; 4]>,
    /// Descriptors for all channels where this participant was actually added.
    pub(super) newly_subscribed_descriptors: SmallVec<[ChannelDescriptor; 4]>,
}

/// Result of unsubscribing a participant from channels.
pub(super) struct UnsubscribeResult {
    /// Channel IDs that lost their last subscriber.
    pub(super) last_unsubscribed: SmallVec<[ChannelId; 4]>,
    /// Descriptors for all channels where this participant was actually removed.
    pub(super) actually_unsubscribed_descriptors: SmallVec<[ChannelDescriptor; 4]>,
}

/// State machine for a remote access session.
///
/// Tracks participants, advertised channels, and per-channel subscriptions.
/// Contains no locking; callers are responsible for synchronization.
///
/// Subscriptions are tracked in two maps:
/// - `subscriptions`: all subscribers regardless of type, for Context first/last notifications.
/// - `video_subscribers`: video subscribers, for managing video track lifecycle.
///
/// A subscriber is a "data subscriber" if they appear in `subscriptions` but not in
/// `video_subscribers`. See [`Self::has_data_subscribers`].
pub(super) struct SessionState {
    /// Channels that have been advertised to participants.
    channels: HashMap<ChannelId, Arc<RawChannel>>,
    /// QoS profile per channel.
    qos_profiles: HashMap<ChannelId, QosProfile>,
    /// All subscriber identities per channel, regardless of subscription type.
    subscriptions: HashMap<ChannelId, SmallVec<[ParticipantIdentity; 1]>>,
    /// Data tracks for advertised channels.
    /// Lifecycle follows channel advertise/unadvertise, not subscribe/unsubscribe.
    data_tracks: HashMap<ChannelId, DataTrack>,
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
            channels: HashMap::new(),
            qos_profiles: HashMap::new(),
            subscriptions: HashMap::new(),
            data_tracks: HashMap::new(),
            video_subscribers: HashMap::new(),
            video_schemas: HashMap::new(),
            video_publishers: HashMap::new(),
            video_track_sids: HashMap::new(),
            video_metadata: HashMap::new(),
            client_channels: HashMap::new(),
            subscribed_parameters: HashMap::new(),
        }
    }

    /// Sweeps `identity` out of every subscription map and returns the
    /// descriptors of channels that lost their last subscriber (and other
    /// aftercare info) for the caller to fire listener callbacks on.
    ///
    /// Does not touch the participant registry — the caller is expected to
    /// have already removed the participant entry there. No-op if `identity`
    /// has no subscriptions or client channels.
    #[must_use]
    pub fn cleanup_for_removed_identity(
        &mut self,
        identity: &ParticipantIdentity,
    ) -> RemovedSubscriptions {
        info!("cleaning up state for removed identity {identity:?}");

        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        let mut subscribed_descriptors: SmallVec<[ChannelDescriptor; 4]> = SmallVec::new();
        for (&channel_id, subscribers) in &mut self.subscriptions {
            if let Some(pos) = subscribers.iter().position(|id| id == identity) {
                subscribers.swap_remove(pos);
                debug_assert!(
                    self.channels.contains_key(&channel_id),
                    "Channel {channel_id:?} has subscribers but is not advertised"
                );
                if let Some(descriptor) = self.channels.get(&channel_id).map(|ch| ch.descriptor()) {
                    subscribed_descriptors.push(descriptor.clone());
                }
                if subscribers.is_empty() {
                    last_unsubscribed.push(channel_id);
                }
            }
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
            last_unsubscribed,
            last_video_unsubscribed,
            subscribed_descriptors,
            client_channels,
            last_param_unsubscribed,
        }
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

    /// Returns all subscriber identities for the given channel.
    pub fn channel_subscriber_identities(
        &self,
        channel_id: &ChannelId,
    ) -> SmallVec<[ParticipantIdentity; 4]> {
        self.subscriptions
            .get(channel_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Records a channel as advertised.
    pub fn insert_channel(&mut self, channel: &Arc<RawChannel>) {
        self.channels.insert(channel.id(), channel.clone());
    }

    /// Records the QoS profile for a channel.
    pub fn insert_qos_profile(&mut self, channel_id: ChannelId, qos: QosProfile) {
        self.qos_profiles.insert(channel_id, qos);
    }

    /// Returns the QoS profile for a channel, defaulting to [`QosProfile::default()`].
    pub fn qos_profile(&self, channel_id: &ChannelId) -> QosProfile {
        self.qos_profiles
            .get(channel_id)
            .copied()
            .unwrap_or_default()
    }

    /// Returns identities of participants that have data subscriptions for a channel.
    ///
    /// A "data subscriber" is one in `subscriptions` but not `video_subscribers`.
    pub fn data_subscriber_identities(
        &self,
        channel_id: &ChannelId,
    ) -> SmallVec<[ParticipantIdentity; 4]> {
        let Some(subscribers) = self.subscriptions.get(channel_id) else {
            return SmallVec::new();
        };
        let video_subs = self.video_subscribers.get(channel_id);
        subscribers
            .iter()
            .filter(|identity| !video_subs.is_some_and(|vs| vs.contains(identity)))
            .cloned()
            .collect()
    }

    /// Returns `true` if the channel is currently advertised.
    pub fn has_channel(&self, channel_id: &ChannelId) -> bool {
        self.channels.contains_key(channel_id)
    }

    /// Removes an advertised channel. Returns `true` if it was present.
    ///
    /// Does NOT remove `data_tracks` — the caller is responsible for calling
    /// `teardown_data_track()` which removes the track and unpublishes it.
    pub fn remove_channel(&mut self, channel_id: ChannelId) -> bool {
        self.subscriptions.remove(&channel_id);
        self.qos_profiles.remove(&channel_id);
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
    fn remove_video_metadata(&mut self, channel_id: &ChannelId) {
        self.video_metadata.remove(channel_id);
    }

    /// Annotates channels in an advertise message with video metadata for channels that have a
    /// detected video schema.
    pub fn add_metadata_to_advertisement(&self, advertise: &mut advertise::Advertise<'_>) {
        for ch in &mut advertise.channels {
            let channel_id = ChannelId::new(ch.id);
            if self.qos_profile(&channel_id).reliability == Reliability::Reliable {
                ch.metadata
                    .insert("foxglove.reliable".to_string(), "true".to_string());
            }
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
    /// Returns:
    /// - `first_subscribed`: channel IDs that gained their first subscriber (for context notifications).
    /// - `newly_subscribed_descriptors`: descriptors for all channels where this participant was
    ///   actually added (for listener callbacks). Excludes channels already subscribed to.
    #[must_use]
    pub fn subscribe(
        &mut self,
        identity: &ParticipantIdentity,
        channel_ids: &[ChannelId],
    ) -> SubscribeResult {
        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        let mut newly_subscribed_descriptors: SmallVec<[ChannelDescriptor; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let subscribers = self.subscriptions.entry(channel_id).or_default();
            if subscribers.contains(identity) {
                info!("{identity:?} is already subscribed to channel {channel_id:?}; ignoring");
                continue;
            }
            let is_first = subscribers.is_empty();
            subscribers.push(identity.clone());
            debug!("{identity:?} subscribed to channel {channel_id:?}");
            debug_assert!(
                self.channels.contains_key(&channel_id),
                "Subscribing to channel {channel_id:?} which is not advertised"
            );
            if let Some(descriptor) = self.get_channel_descriptor(&channel_id) {
                newly_subscribed_descriptors.push(descriptor.clone());
            }
            if is_first {
                first_subscribed.push(channel_id);
            }
        }
        SubscribeResult {
            first_subscribed,
            newly_subscribed_descriptors,
        }
    }

    /// Unsubscribes a participant from the given channels.
    ///
    /// Returns:
    /// - `last_unsubscribed`: channel IDs that lost their last subscriber (for context notifications).
    /// - `actually_unsubscribed_descriptors`: descriptors for all channels where this participant
    ///   was actually removed (for listener callbacks). Excludes channels not subscribed to.
    #[must_use]
    pub fn unsubscribe(
        &mut self,
        identity: &ParticipantIdentity,
        channel_ids: &[ChannelId],
    ) -> UnsubscribeResult {
        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        let mut actually_unsubscribed_descriptors: SmallVec<[ChannelDescriptor; 4]> =
            SmallVec::new();
        for &channel_id in channel_ids {
            let Some(subscribers) = self.subscriptions.get_mut(&channel_id) else {
                info!("{identity:?} is not subscribed to channel {channel_id:?}; ignoring");
                continue;
            };
            let Some(pos) = subscribers.iter().position(|id| id == identity) else {
                info!("{identity:?} is not subscribed to channel {channel_id:?}; ignoring");
                continue;
            };
            subscribers.swap_remove(pos);
            debug!("{identity:?} unsubscribed from channel {channel_id:?}");
            debug_assert!(
                self.channels.contains_key(&channel_id),
                "Unsubscribing from channel {channel_id:?} which is not advertised"
            );
            if let Some(descriptor) = self.channels.get(&channel_id).map(|ch| ch.descriptor()) {
                actually_unsubscribed_descriptors.push(descriptor.clone());
            }
            if subscribers.is_empty() {
                last_unsubscribed.push(channel_id);
            }
        }
        UnsubscribeResult {
            last_unsubscribed,
            actually_unsubscribed_descriptors,
        }
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
        identity: &ParticipantIdentity,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let subscribers = self.video_subscribers.entry(channel_id).or_default();
            if subscribers.contains(identity) {
                continue;
            }
            let is_first = subscribers.is_empty();
            subscribers.push(identity.clone());
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
        identity: &ParticipantIdentity,
        channel_ids: &[ChannelId],
    ) -> SmallVec<[ChannelId; 4]> {
        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        for &channel_id in channel_ids {
            let Some(subscribers) = self.video_subscribers.get_mut(&channel_id) else {
                continue;
            };
            let Some(pos) = subscribers.iter().position(|id| id == identity) else {
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

    /// Returns true if a channel has at least one subscriber that is not a video subscriber.
    pub fn has_data_subscribers(&self, channel_id: &ChannelId) -> bool {
        let total = self.subscriptions.get(channel_id).map_or(0, |s| s.len());
        let video = self
            .video_subscribers
            .get(channel_id)
            .map_or(0, |s| s.len());
        debug_assert!(
            video <= total,
            "Video subscribers {video} must be less than or equal to total subscribers {total}"
        );
        total > video
    }

    /// Returns the data track for a channel if the channel has at least one data subscriber
    /// AND the track has been published. This is the single gate used by `Sink::log`.
    pub fn get_subscribed_data_track(&self, channel_id: &ChannelId) -> Option<&DataTrack> {
        if !self.has_data_subscribers(channel_id) {
            return None;
        }
        self.data_tracks.get(channel_id)
    }

    pub fn insert_data_track(&mut self, channel_id: ChannelId, track: DataTrack) {
        let old = self.data_tracks.insert(channel_id, track);
        debug_assert!(
            old.is_none(),
            "insert_data_track called for channel {channel_id:?} that already has a data track; \
             the old track's background publish task is orphaned"
        );
    }

    pub fn remove_data_track(&mut self, channel_id: &ChannelId) -> Option<DataTrack> {
        self.data_tracks.remove(channel_id)
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

    fn make_identity(name: &str) -> ParticipantIdentity {
        ParticipantIdentity(name.to_string())
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

    fn make_client_channel(channel_id: u64, topic: &str) -> ChannelDescriptor {
        ChannelDescriptor::new(
            ChannelId::new(channel_id),
            topic.to_string(),
            "json".to_string(),
            Default::default(),
            None,
        )
    }

    // ---- subscribe / unsubscribe ----

    #[test]
    fn first_subscriber_is_reported() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);

        let result = state.subscribe(&id, &[ch.id()]);
        assert_eq!(result.first_subscribed.as_slice(), &[ch.id()]);
        assert_eq!(result.newly_subscribed_descriptors.len(), 1);
        assert_eq!(result.newly_subscribed_descriptors[0].id(), ch.id());
    }

    #[test]
    fn second_subscriber_is_not_reported_as_first() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);

        let _ = state.subscribe(&id_a, &[ch.id()]);
        let result = state.subscribe(&id_b, &[ch.id()]);
        assert!(result.first_subscribed.is_empty());
        assert_eq!(result.newly_subscribed_descriptors.len(), 1);
    }

    #[test]
    fn duplicate_subscribe_is_idempotent() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);

        let _ = state.subscribe(&id, &[ch.id()]);
        let result = state.subscribe(&id, &[ch.id()]);
        assert!(result.first_subscribed.is_empty());
        assert!(result.newly_subscribed_descriptors.is_empty());
        assert_eq!(state.subscriptions[&ch.id()].len(), 1);
    }

    #[test]
    fn subscribe_multiple_channels_at_once() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch1 = make_channel("/topic1");
        let ch2 = make_channel("/topic2");
        state.insert_channel(&ch1);
        state.insert_channel(&ch2);

        let result = state.subscribe(&id, &[ch1.id(), ch2.id()]);
        assert_eq!(result.first_subscribed.len(), 2);
        assert_eq!(result.newly_subscribed_descriptors.len(), 2);
    }

    #[test]
    fn last_unsubscriber_is_reported() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);

        let _ = state.subscribe(&id, &[ch.id()]);
        let result = state.unsubscribe(&id, &[ch.id()]);
        assert_eq!(result.last_unsubscribed.as_slice(), &[ch.id()]);
        assert_eq!(result.actually_unsubscribed_descriptors.len(), 1);
    }

    #[test]
    fn unsubscribe_with_remaining_subscribers_is_not_reported() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);

        let _ = state.subscribe(&id_a, &[ch.id()]);
        let _ = state.subscribe(&id_b, &[ch.id()]);

        let result = state.unsubscribe(&id_a, &[ch.id()]);
        assert!(result.last_unsubscribed.is_empty());
        assert_eq!(state.subscriptions[&ch.id()].len(), 1);
    }

    #[test]
    fn unsubscribe_when_not_subscribed_is_noop() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let result = state.unsubscribe(&id, &[ChannelId::new(1)]);
        assert!(result.last_unsubscribed.is_empty());
    }

    // ---- video subscribers ----

    #[test]
    fn first_video_subscriber_is_reported() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let first = state.subscribe_video(&id, &[ChannelId::new(1)]);
        assert_eq!(first.as_slice(), &[ChannelId::new(1)]);
    }

    #[test]
    fn last_video_unsubscriber_is_reported() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let _ = state.subscribe_video(&id, &[ChannelId::new(1)]);
        let last = state.unsubscribe_video(&id, &[ChannelId::new(1)]);
        assert_eq!(last.as_slice(), &[ChannelId::new(1)]);
    }

    #[test]
    fn video_only_subscriber_is_not_a_data_subscriber() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id, &[ch.id()]);
        let _ = state.subscribe_video(&id, &[ch.id()]);
        assert!(!state.has_data_subscribers(&ch.id()));
    }

    #[test]
    fn switching_from_video_to_data() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id, &[ch.id()]);
        let _ = state.subscribe_video(&id, &[ch.id()]);
        assert!(!state.has_data_subscribers(&ch.id()));
        let _ = state.unsubscribe_video(&id, &[ch.id()]);
        assert!(state.has_data_subscribers(&ch.id()));
    }

    // ---- cleanup_for_removed_identity ----

    #[test]
    fn cleanup_missing_identity_is_noop() {
        let mut state = SessionState::new();
        let cleanup = state.cleanup_for_removed_identity(&make_identity("nobody"));
        assert!(cleanup.last_unsubscribed.is_empty());
        assert!(cleanup.last_video_unsubscribed.is_empty());
        assert!(cleanup.subscribed_descriptors.is_empty());
        assert!(cleanup.client_channels.is_empty());
        assert!(cleanup.last_param_unsubscribed.is_empty());
    }

    #[test]
    fn cleanup_sweeps_subscriptions() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id, &[ch.id()]);

        let cleanup = state.cleanup_for_removed_identity(&id);
        assert_eq!(cleanup.last_unsubscribed.as_slice(), &[ch.id()]);
        assert!(!state.has_data_subscribers(&ch.id()));
    }

    #[test]
    fn cleanup_reports_only_last_unsubscribed_channels() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch1 = make_channel("/topic1");
        let ch2 = make_channel("/topic2");
        state.insert_channel(&ch1);
        state.insert_channel(&ch2);

        let _ = state.subscribe(&id_a, &[ch1.id(), ch2.id()]);
        let _ = state.subscribe(&id_b, &[ch1.id()]);

        let cleanup = state.cleanup_for_removed_identity(&id_a);
        // ch1 still has bob, so only ch2 should be reported.
        assert_eq!(cleanup.last_unsubscribed.as_slice(), &[ch2.id()]);
        assert_eq!(state.subscriptions[&ch1.id()].len(), 1);
    }

    #[test]
    fn cleanup_sweeps_video_subscriptions() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id, &[ch.id()]);
        let _ = state.subscribe_video(&id, &[ch.id()]);

        let cleanup = state.cleanup_for_removed_identity(&id);
        assert_eq!(cleanup.last_unsubscribed.as_slice(), &[ch.id()]);
        assert_eq!(cleanup.last_video_unsubscribed.as_slice(), &[ch.id()]);
    }

    #[test]
    fn cleanup_returns_subscribed_descriptors() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch1 = make_channel("/topic1");
        let ch2 = make_channel("/topic2");
        state.insert_channel(&ch1);
        state.insert_channel(&ch2);
        let _ = state.subscribe(&id, &[ch1.id(), ch2.id()]);

        let cleanup = state.cleanup_for_removed_identity(&id);
        assert_eq!(cleanup.subscribed_descriptors.len(), 2);
    }

    #[test]
    fn cleanup_sweeps_client_channels() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        state.insert_client_channel(&id, make_client_channel(1, "/cmd_vel"));
        state.insert_client_channel(&id, make_client_channel(2, "/joy"));

        let cleanup = state.cleanup_for_removed_identity(&id);
        assert_eq!(cleanup.client_channels.len(), 2);
        assert!(
            state
                .remove_client_channel(&id, ChannelId::new(1))
                .is_none(),
            "map entry must be gone",
        );
    }

    #[test]
    fn cleanup_for_mixed_video_preferences() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id_a, &[ch.id()]);
        let _ = state.subscribe(&id_b, &[ch.id()]);
        let _ = state.subscribe_video(&id_a, &[ch.id()]);

        // Remove alice: channel keeps bob, loses its last video subscriber.
        let cleanup = state.cleanup_for_removed_identity(&id_a);
        assert!(cleanup.last_unsubscribed.is_empty());
        assert_eq!(cleanup.last_video_unsubscribed.as_slice(), &[ch.id()]);
        assert!(state.has_data_subscribers(&ch.id()));
    }

    // ---- channel + client-channel lookups ----

    #[test]
    fn channel_subscriber_identities_empty_for_unknown_channel() {
        let state = SessionState::new();
        assert!(
            state
                .channel_subscriber_identities(&ChannelId::new(999))
                .is_empty()
        );
    }

    #[test]
    fn channel_subscriber_identities_returns_subscribers() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id_a, &[ch.id()]);
        let _ = state.subscribe(&id_b, &[ch.id()]);

        let result = state.channel_subscriber_identities(&ch.id());
        assert_eq!(result.len(), 2);
        assert!(result.contains(&id_a));
        assert!(result.contains(&id_b));
    }

    #[test]
    fn channel_subscriber_identities_empty_after_remove_channel() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_channel("/topic1");
        state.insert_channel(&ch);
        let _ = state.subscribe(&id, &[ch.id()]);
        assert_eq!(state.channel_subscriber_identities(&ch.id()).len(), 1);

        state.remove_channel(ch.id());
        assert!(state.channel_subscriber_identities(&ch.id()).is_empty());
    }

    #[test]
    fn insert_client_channel_is_noop_for_duplicate() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        let ch = make_client_channel(1, "/cmd");
        assert!(state.insert_client_channel(&id, ch.clone()));
        assert!(!state.insert_client_channel(&id, ch));
    }

    #[test]
    fn remove_client_channel_returns_descriptor() {
        let mut state = SessionState::new();
        let id = make_identity("alice");
        state.insert_client_channel(&id, make_client_channel(1, "/cmd"));
        let removed = state.remove_client_channel(&id, ChannelId::new(1));
        assert_eq!(removed.unwrap().topic(), "/cmd");
    }

    #[test]
    fn get_client_channel_returns_none_for_unknown() {
        let state = SessionState::new();
        assert!(
            state
                .get_client_channel(&make_identity("nobody"), ChannelId::new(1))
                .is_none()
        );
    }

    // ---- channels / qos ----

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
    fn qos_profile_defaults_to_lossy() {
        let state = SessionState::new();
        assert_eq!(
            state.qos_profile(&ChannelId::new(42)).reliability,
            Reliability::Lossy,
        );
    }

    #[test]
    fn insert_and_retrieve_qos_profile() {
        let mut state = SessionState::new();
        let ch = make_channel("/config");
        state.insert_channel(&ch);
        let qos = QosProfile::builder()
            .reliability(Reliability::Reliable)
            .build();
        state.insert_qos_profile(ch.id(), qos);
        assert_eq!(
            state.qos_profile(&ch.id()).reliability,
            Reliability::Reliable
        );
    }

    #[test]
    fn remove_channel_cleans_up_qos_profile() {
        let mut state = SessionState::new();
        let ch = make_channel("/config");
        state.insert_channel(&ch);
        state.insert_qos_profile(
            ch.id(),
            QosProfile::builder()
                .reliability(Reliability::Reliable)
                .build(),
        );
        state.remove_channel(ch.id());
        assert_eq!(state.qos_profile(&ch.id()).reliability, Reliability::Lossy);
    }

    // ---- data_subscriber_identities ----

    #[test]
    fn data_subscriber_identities_empty_when_no_subscribers() {
        let state = SessionState::new();
        assert!(
            state
                .data_subscriber_identities(&ChannelId::new(1))
                .is_empty()
        );
    }

    #[test]
    fn data_subscriber_identities_returns_data_only_subscribers() {
        let mut state = SessionState::new();
        let id_a = make_identity("alice");
        let id_b = make_identity("bob");
        let ch = make_channel("/data");
        state.insert_channel(&ch);
        // Both subscribe (data). Bob also subscribes to video.
        let _ = state.subscribe(&id_a, &[ch.id()]);
        let _ = state.subscribe(&id_b, &[ch.id()]);
        let _ = state.subscribe_video(&id_b, &[ch.id()]);

        let subs = state.data_subscriber_identities(&ch.id());
        assert_eq!(subs.len(), 1);
        assert_eq!(&subs[0], &id_a);
    }

    // ---- advertise-metadata rendering ----

    #[test]
    fn add_metadata_to_advertisement_injects_video_metadata() {
        let mut state = SessionState::new();
        let ch = make_channel("/camera");
        state.insert_channel(&ch);
        state.insert_video_schema(ch.id(), VideoInputSchema::FoxgloveRawImage);

        let mut msg = advertise::advertise_channels(std::iter::once(&ch)).into_owned();
        state.add_metadata_to_advertisement(&mut msg);
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.hasVideoTrack"),
            Some(&"true".to_string()),
        );

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
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoFrameId"),
            "empty frame_id should not be advertised",
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
        assert_eq!(
            msg.channels[0].metadata.get("foxglove.hasVideoTrack"),
            Some(&"true".to_string()),
        );
        assert!(
            !msg.channels[0]
                .metadata
                .contains_key("foxglove.videoSourceEncoding")
        );
    }
}
