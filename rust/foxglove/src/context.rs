use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use parking_lot::RwLock;
use smallvec::SmallVec;
use tracing::warn;

use crate::{ChannelBuilder, ChannelId, McapWriter, RawChannel, Sink, SinkId};

mod lazy_context;
mod subscriptions;

pub use lazy_context::LazyContext;
use subscriptions::Subscriptions;

#[derive(Default)]
struct ContextInner {
    channels: HashMap<ChannelId, Arc<RawChannel>>,
    channels_by_topic: HashMap<String, SmallVec<[Arc<RawChannel>; 1]>>,
    sinks: HashMap<SinkId, Arc<dyn Sink>>,
    subs: Subscriptions,
}
impl ContextInner {
    /// Returns the channel for the specified topic, if there is one.
    ///
    /// If multiple channels use the same topic name, this will return the first channel that was
    /// added to this context.
    fn get_channel_by_topic(&self, topic: &str) -> Option<&Arc<RawChannel>> {
        self.channels_by_topic.get(topic)?.first()
    }

    /// Adds a channel to the context.
    fn add_channel(&mut self, channel: Arc<RawChannel>) -> Arc<RawChannel> {
        let topic = channel.topic();

        // If a substantially identical channel already exists, just return that.
        let topic_channels = self.channels_by_topic.entry(topic.to_string()).or_default();
        if let Some(matching) = topic_channels.iter().find(|c| channel.matches(c)) {
            return matching.clone();
        }

        // Friends don't let friends create multiple channels on the same topic.
        if !topic_channels.is_empty() {
            warn!(
                "Channel with topic {topic} already exists in this context; \
                 use a unique topic for each channel"
            );
        }

        // Add the channel to the indexes.
        self.channels.insert(channel.id(), channel.clone());
        topic_channels.push(channel.clone());

        // Notify sinks of new channel. Sinks that dynamically manage subscriptions may return true
        // from `add_channel` to add a subscription synchronously.
        for sink in self.sinks.values() {
            if sink.add_channel(&channel) && !sink.auto_subscribe() {
                self.subs.subscribe_channels(sink, &[channel.id()]);
            }
        }

        // Connect channel sinks.
        let sinks = self.subs.get_subscribers(channel.id());
        channel.update_sinks(sinks);
        channel
    }

    /// Removes a channel from the context.
    fn remove_channel(&mut self, channel_id: ChannelId) -> bool {
        let Some(channel) = self.channels.remove(&channel_id) else {
            return false;
        };

        // Remove the channel from the topic index.
        if let Some(topic_channels) = self.channels_by_topic.get_mut(channel.topic()) {
            topic_channels.retain(|c| c.id() != channel_id);
            if topic_channels.is_empty() {
                self.channels_by_topic.remove(channel.topic());
            }
        }

        // Remove subscriptions for this channel.
        self.subs.remove_channel_subscriptions(channel.id());

        // Close the channel and remove sinks.
        channel.remove_from_context();

        // Notify sinks of removed channel.
        for sink in self.sinks.values() {
            sink.remove_channel(&channel);
        }

        true
    }

    /// Adds a sink to the context.
    fn add_sink(&mut self, sink: Arc<dyn Sink>) -> bool {
        let sink_id = sink.id();
        let Entry::Vacant(entry) = self.sinks.entry(sink_id) else {
            return false;
        };
        entry.insert(sink.clone());

        // Notify sink of existing channels. Sinks that dynamically manage subscriptions may return
        // a set of channel IDs that they want to subscribe to immediately.
        let channels: Vec<_> = self.channels.values().collect();
        let ids = if !channels.is_empty() {
            sink.add_channels(&channels)
        } else {
            None
        };

        // Add requested subscriptions.
        if sink.auto_subscribe() {
            if self.subs.subscribe_global(sink.clone()) {
                self.update_channel_sinks(&channels);
            }
        } else if let Some(mut ids) = ids {
            ids.retain(|id| self.channels.contains_key(id));
            if !ids.is_empty() && self.subs.subscribe_channels(&sink, &ids) {
                self.update_channel_sinks_by_ids(&ids);
            }
        }

        true
    }

    /// Removes a sink from the context.
    fn remove_sink(&mut self, sink_id: SinkId) -> bool {
        // Remove sink's subscriptions. If this wasn't a no-op, update channel sinks.
        if self.subs.remove_subscriber(sink_id) {
            self.update_channel_sinks(self.channels.values());
        }

        self.sinks.remove(&sink_id).is_some()
    }

    /// Subscribes a sink to the specified channels.
    fn subscribe_channels(&mut self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        if let Some(sink) = self.sinks.get(&sink_id) {
            if self.subs.subscribe_channels(sink, channel_ids) {
                self.update_channel_sinks_by_ids(channel_ids);
            }
        }
    }

    /// Unsubscribes a sink from the specified channels.
    fn unsubscribe_channels(&mut self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        if self.subs.unsubscribe_channels(sink_id, channel_ids) {
            self.update_channel_sinks_by_ids(channel_ids);
        }
    }

    /// Updates the set of connected sinks on the specified channels, given by their IDs.
    fn update_channel_sinks_by_ids(&self, channel_ids: &[ChannelId]) {
        let channels = channel_ids.iter().filter_map(|id| self.channels.get(id));
        self.update_channel_sinks(channels);
    }

    /// Updates the set of connected sinks on the specified channels.
    fn update_channel_sinks(&self, channels: impl IntoIterator<Item = impl AsRef<RawChannel>>) {
        for channel in channels {
            let channel = channel.as_ref();
            let sinks = self.subs.get_subscribers(channel.id());
            channel.update_sinks(sinks);
        }
    }

    /// Removes all channels and sinks from the context.
    fn clear(&mut self) {
        for (_, channel) in self.channels.drain() {
            // Close the channel and remove sinks.
            channel.remove_from_context();

            // Notify sink of removed channel.
            for sink in self.sinks.values() {
                sink.remove_channel(&channel);
            }
        }
        self.channels_by_topic.clear();
        self.sinks.clear();
        self.subs.clear();
    }
}

/// A context is the binding between channels and sinks.
///
/// Each channel and each sink belongs to exactly one context. Sinks receive advertisements about
/// channels on the context, and can optionally subscribe to receive logged messages on those
/// channels.
///
/// When the context is dropped, its corresponding channels and sinks will be disconnected from one
/// another, and logging will stop. Attempts to log on a channel after its context has been dropped
/// will elicit a throttled warning message.
///
/// Since many applications only need a single context, the SDK provides a static default context
/// for convenience. To obtain a reference to the default context, use [`Context::get_default`].
///
/// It is also possible to create explicit contexts:
///
/// ```
/// use foxglove::schemas::Log;
/// use foxglove::{Context, FoxgloveError};
///
/// // Create a channel for the "/log" topic.
/// let topic = "/topic";
/// let ctx_a = Context::new();
/// let chan_a = ctx_a.channel_builder(topic).build();
/// chan_a.log(&Log {
///     message: "hello a".into(),
///     ..Log::default()
/// });
///
/// // We can re-use the same topic name on channels if they're in different contexts. This may be
/// // useful for logging to different MCAP sinks.
/// let ctx_b = Context::new();
/// let chan_b = ctx_b.channel_builder(topic).build();
/// chan_b.log(&Log {
///     message: "hello b".into(),
///     ..Log::default()
/// });
/// ```
pub struct Context(RwLock<ContextInner>);

impl Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Context").finish_non_exhaustive()
    }
}

impl Context {
    /// Instantiates a new context.
    #[allow(clippy::new_without_default)] // avoid confusion with Context::get_default()
    pub fn new() -> Arc<Self> {
        Arc::new(Self(RwLock::default()))
    }

    /// Returns a reference to the default context.
    ///
    /// If there is no default context, this function instantiates one.
    pub fn get_default() -> Arc<Self> {
        Arc::clone(LazyContext::get_default())
    }

    /// Returns a channel builder for a channel in this context.
    ///
    /// You should choose a unique topic name per channel for compatibility with the Foxglove app.
    pub fn channel_builder(self: &Arc<Self>, topic: impl Into<String>) -> ChannelBuilder {
        ChannelBuilder::new(topic).context(self)
    }

    /// Returns a builder for an MCAP writer in this context.
    pub fn mcap_writer(self: &Arc<Self>) -> McapWriter {
        McapWriter::new().context(self)
    }

    /// Returns a builder for a websocket server in this context.
    #[cfg(feature = "live_visualization")]
    pub fn websocket_server(self: &Arc<Self>) -> crate::WebSocketServer {
        crate::WebSocketServer::new().context(self)
    }

    /// Returns the channel for the specified topic, if there is one.
    ///
    /// If multiple channels use the same topic name, this will return the first channel that was
    /// added to this context.
    pub fn get_channel_by_topic(&self, topic: &str) -> Option<Arc<RawChannel>> {
        self.0.read().get_channel_by_topic(topic).cloned()
    }

    /// Adds a channel to the context, or returns a channel with the same topic and schema.
    ///
    /// This is deliberately `pub(crate)` to ensure that the channel's context linkage remains
    /// consistent. Publicly, the only way to add a channel to a context is by constructing it via
    /// a [`ChannelBuilder`][crate::ChannelBuilder].
    pub(crate) fn add_channel(&self, channel: Arc<RawChannel>) -> Arc<RawChannel> {
        self.0.write().add_channel(channel)
    }

    /// Removes a channel from the context.
    ///
    /// This is deliberately `pub(crate)` to ensure that the channel's context linkage remains
    /// consistent. Publicly, the only way to remove a channel from a context is by calling
    /// [`RawChannel::close`], or by dropping the context entirely.
    pub(crate) fn remove_channel(&self, channel_id: ChannelId) -> bool {
        self.0.write().remove_channel(channel_id)
    }

    /// Adds a sink to the context.
    ///
    /// The sink will be synchronously notified of all registered channels.
    ///
    /// If [`Sink::auto_subscribe`] returns true, the sink will be automatically subscribed to all
    /// present and future channels on the context. Otherwise, the sink is expected to manage its
    /// subscriptions dynamically with [`Context::subscribe_channels`] and
    /// [`Context::unsubscribe_channels`].
    #[doc(hidden)] // Hidden until Sink is public
    pub fn add_sink(&self, sink: Arc<dyn Sink>) -> bool {
        self.0.write().add_sink(sink)
    }

    /// Removes a sink from the context.
    #[doc(hidden)] // Hidden until Sink is public.
    pub fn remove_sink(&self, sink_id: SinkId) -> bool {
        self.0.write().remove_sink(sink_id)
    }

    /// Subscribes a sink to the specified channels.
    ///
    /// This method has no effect for sinks that return true from [`Sink::auto_subscribe`].
    #[doc(hidden)] // Hidden until Sink is public.
    pub fn subscribe_channels(&self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        self.0.write().subscribe_channels(sink_id, channel_ids);
    }

    /// Unsubscribes a sink from the specified channels.
    ///
    /// This method has no effect for sinks that return true from [`Sink::auto_subscribe`].
    #[doc(hidden)] // Hidden until Sink is public.
    pub fn unsubscribe_channels(&self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        self.0.write().unsubscribe_channels(sink_id, channel_ids);
    }

    /// Removes all channels and sinks from the context.
    pub(crate) fn clear(&self) {
        self.0.write().clear();
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::context::*;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::{ErrorSink, MockSink, RecordingSink};
    use crate::{nanoseconds_since_epoch, PartialMetadata, RawChannel, Schema};
    use crate::{ChannelBuilder, FoxgloveError};
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel_builder(ctx: &Arc<Context>, topic: &str) -> ChannelBuilder {
        ChannelBuilder::new(topic)
            .context(ctx)
            .message_encoding("message_encoding")
            .schema(Schema::new(
                "name",
                "encoding",
                br#"{
                    "type": "object",
                    "properties": {
                        "msg": {"type": "string"},
                        "count": {"type": "number"},
                    },
                }"#,
            ))
            .metadata(maplit::btreemap! {"key".to_string() => "value".to_string()})
    }

    fn new_test_channel(ctx: &Arc<Context>, topic: &str) -> Result<Arc<RawChannel>, FoxgloveError> {
        new_test_channel_builder(ctx, topic).build_raw()
    }

    #[test]
    fn test_add_and_remove_sink() {
        let ctx = Context::new();
        let sink = Arc::new(MockSink::default());
        let sink2 = Arc::new(MockSink::default());
        let sink3 = Arc::new(MockSink::default());

        // Test adding a sink
        assert!(ctx.add_sink(sink.clone()));
        // Can't add it twice
        assert!(!ctx.add_sink(sink.clone()));
        assert!(ctx.add_sink(sink2.clone()));

        // Test removing a sink
        assert!(ctx.remove_sink(sink.id()));

        // Try to remove a sink that doesn't exist
        assert!(!ctx.remove_sink(sink3.id()));

        // Test removing the last sink
        assert!(ctx.remove_sink(sink2.id()));
    }

    #[traced_test]
    #[test]
    fn test_log_calls_sinks() {
        let ctx = Context::new();
        let sink1 = Arc::new(RecordingSink::new());
        let sink2 = Arc::new(RecordingSink::new());

        assert!(ctx.add_sink(sink1.clone()));
        assert!(ctx.add_sink(sink2.clone()));

        let channel = new_test_channel(&ctx, "topic").unwrap();
        let msg = b"test_message";

        let now = nanoseconds_since_epoch();

        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));

        let messages1 = sink1.take_messages();
        let messages2 = sink2.take_messages();

        assert_eq!(messages1.len(), 1);
        assert_eq!(messages2.len(), 1);

        assert_eq!(messages1[0].channel_id, channel.id());
        assert_eq!(messages1[0].msg, msg.to_vec());
        let metadata1 = &messages1[0].metadata;
        assert!(metadata1.log_time >= now);

        assert_eq!(messages2[0].channel_id, channel.id());
        assert_eq!(messages2[0].msg, msg.to_vec());
        let metadata2 = &messages2[0].metadata;
        assert!(metadata2.log_time >= now);
    }

    #[traced_test]
    #[test]
    fn test_log_calls_other_sinks_after_error() {
        let ctx = Context::new();
        let error_sink = Arc::new(ErrorSink::default());
        let recording_sink = Arc::new(RecordingSink::new());

        assert!(ctx.add_sink(error_sink.clone()));
        assert!(!ctx.add_sink(error_sink.clone()));
        assert!(ctx.add_sink(recording_sink.clone()));

        let channel = new_test_channel(&ctx, "topic").unwrap();
        let msg = b"test_message";
        let opts = PartialMetadata {
            log_time: Some(nanoseconds_since_epoch()),
        };

        channel.log_with_meta(msg, opts);
        assert!(logs_contain(ERROR_LOGGING_MESSAGE));
        assert!(logs_contain("ErrorSink always fails"));

        let messages = recording_sink.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel_id, channel.id());
        assert_eq!(messages[0].msg, msg.to_vec());
        let metadata = &messages[0].metadata;
        assert_eq!(metadata.log_time, opts.log_time.unwrap());
    }

    #[traced_test]
    #[test]
    fn test_log_msg_no_sinks() {
        let ctx = Context::new();
        let channel = new_test_channel(&ctx, "topic").unwrap();
        let msg = b"test_message";
        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));
    }

    #[test]
    fn test_remove_channel() {
        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "topic").unwrap();
        assert!(ctx.remove_channel(ch.id()));
        assert!(ctx.0.read().channels.is_empty());
    }

    #[test]
    fn test_auto_subscribe() {
        let ctx = Context::new();
        let c1 = new_test_channel(&ctx, "t1").unwrap();
        let c2 = new_test_channel(&ctx, "t2").unwrap();
        let sink = Arc::new(RecordingSink::new().auto_subscribe(true));

        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // Auto-subscribe to existing channels.
        ctx.add_sink(sink.clone());
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());

        // Auto-subscribe to new channels.
        assert!(ctx.remove_channel(c1.id()));
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.add_channel(c1.clone());
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());

        // Sink subscriptions are removed with the sink.
        ctx.remove_sink(sink.id());
        assert!(!c1.has_sinks());
    }

    #[test]
    fn test_no_auto_subscribe() {
        let ctx = Context::new();
        let c1 = new_test_channel(&ctx, "t1").unwrap();
        let c2 = new_test_channel(&ctx, "t2").unwrap();
        let sink = Arc::new(RecordingSink::new().auto_subscribe(false));

        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // No auto-subscribe to existing channels.
        ctx.add_sink(sink.clone());
        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // No auto-subscribe to new channels.
        assert!(ctx.remove_channel(c1.id()));
        ctx.add_channel(c1.clone());
        assert!(!c1.has_sinks());

        // Subscribe to a channel.
        ctx.subscribe_channels(sink.id(), &[c1.id()]);
        assert!(c1.has_sinks());
        assert!(!c2.has_sinks());
        ctx.subscribe_channels(sink.id(), &[c2.id()]);
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());

        // If a channel is removed and re-added, its subscriptions are lost. This isn't a workflow
        // we expect to happen. Note that the sink will receive `remove_channel` and `add_channel`
        // callbacks, so it has an opportunity to reinstall subscriptions if it wants to.
        assert!(ctx.remove_channel(c1.id()));
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.add_channel(c1.clone());
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.subscribe_channels(sink.id(), &[c1.id()]);
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());

        // Unsubscribe from a channel.
        ctx.unsubscribe_channels(sink.id(), &[c1.id()]);
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());

        // Sink subscriptions are removed with the sink.
        ctx.subscribe_channels(sink.id(), &[c1.id(), c2.id()]);
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.remove_sink(sink.id());
        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());
    }

    #[test]
    fn test_sink_subscribe_on_channel_add() {
        let ctx = Context::new();

        // Sink which automatically subscribes to t1, but not t2.
        let s1 = Arc::new(
            RecordingSink::new()
                .auto_subscribe(false)
                .add_channels(|channels| {
                    Some(
                        channels
                            .iter()
                            .filter_map(|c| {
                                if c.topic() == "t1" {
                                    Some(c.id())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    )
                }),
        );
        ctx.add_sink(s1.clone());

        // Add channels with existing sink.
        let c1 = new_test_channel(&ctx, "t1").unwrap();
        let c2 = new_test_channel(&ctx, "t2").unwrap();
        assert!(c1.has_sinks());
        assert!(!c2.has_sinks());

        // Remove sink.
        ctx.remove_sink(s1.id());
        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // Add sink with existing channels.
        ctx.add_sink(s1.clone());
        assert!(c1.has_sinks());
        assert!(!c2.has_sinks());

        // Cleanup
        ctx.remove_sink(s1.id());
        assert!(!c1.has_sinks());

        // Sink which never auto-subscribes to anything.
        let s2 = Arc::new(
            RecordingSink::new()
                .auto_subscribe(false)
                .add_channels(|_| None),
        );

        // Add sink with existing channels.
        ctx.add_sink(s2.clone());
        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // Remove channels.
        assert!(ctx.remove_channel(c1.id()));
        assert!(ctx.remove_channel(c2.id()));

        // Add channels with existing sink.
        ctx.add_channel(c1.clone());
        ctx.add_channel(c2.clone());
        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());
    }

    #[test]
    fn test_no_add_channels_cb() {
        let ctx = Context::new();
        let s1 = Arc::new(RecordingSink::new().add_channels(|_| unreachable!("no channels!")));
        ctx.add_sink(s1.clone());
    }

    #[test]
    fn test_supports_multiple_channels_with_same_topic() {
        let ctx = Context::new();
        let c1 = new_test_channel(&ctx, "topic").unwrap();
        let c2 = new_test_channel_builder(&ctx, "topic")
            .schema(None)
            .build_raw()
            .unwrap();
        assert_ne!(c1.id(), c2.id());
        assert_eq!(c1.topic(), c2.topic());
    }

    #[test]
    #[traced_test]
    fn test_get_channel_by_topic_with_duplicate() {
        let ctx = Context::new();
        let c1 = new_test_channel(&ctx, "dupe").unwrap();
        let c2 = new_test_channel_builder(&ctx, "dupe")
            .message_encoding("different")
            .build_raw()
            .unwrap();
        assert!(logs_contain(
            "Channel with topic dupe already exists in this context"
        ));

        // Returns the oldest matching channel.
        let channel = ctx.get_channel_by_topic("dupe");
        assert!(channel.is_some());
        assert_eq!(channel.unwrap().id(), c1.id());

        // If we remove the oldest, it returns the next oldest.
        assert!(ctx.remove_channel(c1.id()));
        let channel = ctx.get_channel_by_topic("dupe");
        assert!(channel.is_some());
        assert_eq!(channel.unwrap().id(), c2.id());

        // Nothing left that matches.
        assert!(ctx.remove_channel(c2.id()));
        let channel = ctx.get_channel_by_topic("dupe");
        assert!(channel.is_none());

        // It is safe to add a new channel with the same topic.
        let c3 = new_test_channel(&ctx, "dupe").unwrap();
        let channel = ctx.get_channel_by_topic("dupe");
        assert!(channel.is_some());
        assert_eq!(channel.unwrap().id(), c3.id());
    }

    #[test]
    fn test_add_channel_or_return_matching_channel() {
        let ctx = Context::new();

        // Same topic, different properties.
        let _ = new_test_channel_builder(&ctx, "dupe")
            .message_encoding("different")
            .build_raw()
            .unwrap();
        let _ = new_test_channel_builder(&ctx, "dupe")
            .schema(None)
            .build_raw()
            .unwrap();
        let _ = new_test_channel_builder(&ctx, "dupe")
            .metadata(maplit::btreemap! {"it's".into() => "different".into()})
            .build_raw()
            .unwrap();

        // Actual matches.
        let c1 = new_test_channel(&ctx, "dupe").unwrap();
        assert_eq!(ctx.0.read().channels.len(), 4);

        // Reuses the matching channel.
        let c2 = new_test_channel(&ctx, "dupe").unwrap();
        assert_eq!(c1.id(), c2.id());
        assert_eq!(Arc::as_ptr(&c1), Arc::as_ptr(&c2));
        assert_eq!(ctx.0.read().channels.len(), 4);

        // No matches, creates a new channel.
        assert!(ctx.remove_channel(c1.id()));
        assert_eq!(ctx.0.read().channels.len(), 3);
        let _ = new_test_channel(&ctx, "dupe").unwrap();
        assert_eq!(ctx.0.read().channels.len(), 4);
    }
}
