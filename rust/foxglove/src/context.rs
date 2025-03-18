use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

use crate::channel::ChannelId;
use crate::{Channel, FoxgloveError, Sink, SinkId};

mod subscriptions;
use subscriptions::Subscriptions;

#[derive(Default)]
struct ContextInner {
    channels: HashMap<ChannelId, Arc<Channel>>,
    channels_by_topic: HashMap<String, Arc<Channel>>,
    sinks: HashMap<SinkId, Arc<dyn Sink>>,
    subs: Subscriptions,
}
impl ContextInner {
    /// Returns the channel for the specified topic, if there is one.
    fn get_channel_by_topic(&self, topic: &str) -> Option<&Arc<Channel>> {
        self.channels_by_topic.get(topic)
    }

    /// Adds a channel to the context.
    fn add_channel(&mut self, channel: Arc<Channel>) -> Result<(), FoxgloveError> {
        // Insert channel.
        let topic = channel.topic();
        let Entry::Vacant(entry) = self.channels_by_topic.entry(topic.to_string()) else {
            return Err(FoxgloveError::DuplicateChannel(topic.to_string()));
        };
        entry.insert(channel.clone());
        self.channels.insert(channel.id(), channel.clone());

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

        Ok(())
    }

    /// Removes the channel for the specified topic.
    fn remove_channel_for_topic(&mut self, topic: &str) -> bool {
        let Some(channel) = self.channels_by_topic.remove(topic) else {
            return false;
        };
        self.channels.remove(&channel.id());

        // Remove subscriptions for this channel.
        self.subs.remove_channel_subscriptions(channel.id());

        // Disconnect channel sinks.
        channel.clear_sinks();

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
        // true from `add_channel` to add a subscription synchronously.
        let auto_subscribe = sink.auto_subscribe();
        let mut sub_channel_ids = vec![];
        for channel in self.channels.values() {
            if sink.add_channel(channel) && !auto_subscribe {
                sub_channel_ids.push(channel.id());
            }
        }

        // Add requested subscriptions.
        if !sub_channel_ids.is_empty() && self.subs.subscribe_channels(&sink, &sub_channel_ids) {
            self.update_channel_sinks_by_ids(&sub_channel_ids);
        } else if auto_subscribe && self.subs.subscribe_global(sink.clone()) {
            self.update_channel_sinks(self.channels.values());
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
    fn update_channel_sinks(&self, channels: impl IntoIterator<Item = impl AsRef<Channel>>) {
        for channel in channels {
            let channel = channel.as_ref();
            let sinks = self.subs.get_subscribers(channel.id());
            channel.update_sinks(sinks);
        }
    }

    /// Removes all channels and sinks from the context.
    fn clear(&mut self) {
        self.channels.clear();
        self.channels_by_topic.clear();
        self.sinks.clear();
        self.subs.clear();
    }
}

/// A context is a collection of channels and sinks.
///
/// To obtain a reference to the default context, use [`Context::get_default`]. To construct a new
/// context, use [`Context::new`].
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
        static DEFAULT_CONTEXT: LazyLock<Arc<Context>> = LazyLock::new(Context::new);
        DEFAULT_CONTEXT.clone()
    }

    /// Returns the channel for the specified topic, if there is one.
    pub fn get_channel_by_topic(&self, topic: &str) -> Option<Arc<Channel>> {
        self.0.read().get_channel_by_topic(topic).cloned()
    }

    /// Adds a channel to the context.
    pub fn add_channel(&self, channel: Arc<Channel>) -> Result<(), FoxgloveError> {
        self.0.write().add_channel(channel)
    }

    /// Removes the channel for the specified topic.
    pub fn remove_channel_for_topic(&self, topic: &str) -> bool {
        self.0.write().remove_channel_for_topic(topic)
    }

    /// Adds a sink to the context.
    ///
    /// The sink will be synchronously notified of all registered channels.
    ///
    /// If [`Sink::auto_subscribe`] returns true, the sink will be automatically subscribed to all
    /// present and future channels on the context. Otherwise, the sink is expected to manage its
    /// subscriptions dynamically with [`Context::subscribe_channels`] and
    /// [`Context::unsubscribe_channels`].
    pub fn add_sink(&self, sink: Arc<dyn Sink>) -> bool {
        self.0.write().add_sink(sink)
    }

    /// Removes a sink from the context.
    pub fn remove_sink(&self, sink_id: SinkId) -> bool {
        self.0.write().remove_sink(sink_id)
    }

    /// Subscribes a sink to the specified channels.
    ///
    /// This method has no effect for sinks that return true from [`Sink::auto_subscribe`].
    pub fn subscribe_channels(&self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        self.0.write().subscribe_channels(sink_id, channel_ids);
    }

    /// Unsubscribes a sink from the specified channels.
    ///
    /// This method has no effect for sinks that return true from [`Sink::auto_subscribe`].
    pub fn unsubscribe_channels(&self, sink_id: SinkId, channel_ids: &[ChannelId]) {
        self.0.write().unsubscribe_channels(sink_id, channel_ids);
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.0.write().clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::collection::collection;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::{ErrorSink, MockSink, RecordingSink};
    use crate::{context::*, ChannelBuilder};
    use crate::{nanoseconds_since_epoch, Channel, PartialMetadata, Schema};
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel(ctx: &Arc<Context>, topic: &str) -> Result<Arc<Channel>, FoxgloveError> {
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
            .metadata(collection! {"key".to_string() => "value".to_string()})
            .build()
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

        let recorded1 = sink1.recorded.lock();
        let recorded2 = sink2.recorded.lock();

        assert_eq!(recorded1.len(), 1);
        assert_eq!(recorded2.len(), 1);

        assert_eq!(recorded1[0].channel_id, channel.id());
        assert_eq!(recorded1[0].msg, msg.to_vec());
        let metadata1 = &recorded1[0].metadata;
        assert!(metadata1.log_time >= now);
        assert!(metadata1.publish_time >= now);
        assert_eq!(metadata1.log_time, metadata1.publish_time);
        assert!(metadata1.sequence > 0);

        assert_eq!(recorded2[0].channel_id, channel.id());
        assert_eq!(recorded2[0].msg, msg.to_vec());
        let metadata2 = &recorded2[0].metadata;
        assert!(metadata2.log_time >= now);
        assert!(metadata2.publish_time >= now);
        assert_eq!(metadata2.log_time, metadata2.publish_time);
        assert!(metadata2.sequence > 0);
        assert_eq!(metadata1.sequence, metadata2.sequence);
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
            sequence: Some(1),
            log_time: Some(nanoseconds_since_epoch()),
            publish_time: Some(nanoseconds_since_epoch()),
        };

        channel.log_with_meta(msg, opts);
        assert!(logs_contain(ERROR_LOGGING_MESSAGE));
        assert!(logs_contain("ErrorSink always fails"));

        let recorded = recording_sink.recorded.lock();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].channel_id, channel.id());
        assert_eq!(recorded[0].msg, msg.to_vec());
        let metadata = &recorded[0].metadata;
        assert_eq!(metadata.sequence, opts.sequence.unwrap());
        assert_eq!(metadata.log_time, opts.log_time.unwrap());
        assert_eq!(metadata.publish_time, opts.publish_time.unwrap());
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
        let _ = new_test_channel(&ctx, "topic").unwrap();
        assert!(ctx.remove_channel_for_topic("topic"));
        assert!(ctx.0.read().channels.is_empty());
    }

    #[test]
    fn test_auto_subscribe() {
        let ctx = Context::new();
        let c1 = new_test_channel(&ctx, "t1").unwrap();
        let c2 = new_test_channel(&ctx, "t2").unwrap();
        let sink = Arc::new(RecordingSink::new());

        assert!(!c1.has_sinks());
        assert!(!c2.has_sinks());

        // Auto-subscribe to existing channels.
        ctx.add_sink(sink.clone());
        assert!(c1.has_sinks());
        assert!(c2.has_sinks());

        // Auto-subscribe to new channels.
        assert!(ctx.remove_channel_for_topic(c1.topic()));
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.add_channel(c1.clone()).unwrap();
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
        assert!(ctx.remove_channel_for_topic(c1.topic()));
        ctx.add_channel(c1.clone()).unwrap();
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
        assert!(ctx.remove_channel_for_topic(c1.topic()));
        assert!(!c1.has_sinks());
        assert!(c2.has_sinks());
        ctx.add_channel(c1.clone()).unwrap();
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

        // Sink which returns true from add_channel
        let s1 = Arc::new(
            RecordingSink::new()
                .auto_subscribe(false)
                .add_channel_rval(true),
        );
        ctx.add_sink(s1.clone());

        // Add channel with existing sink.
        let ch = ChannelBuilder::new("topic")
            .message_encoding("json")
            .context(&ctx)
            .build()
            .unwrap();
        assert!(ch.has_sinks());
        ctx.remove_sink(s1.id());

        // Add sink with existing channel.
        assert!(!ch.has_sinks());
        ctx.add_sink(s1.clone());
        assert!(ch.has_sinks());

        // Cleanup
        ctx.remove_sink(s1.id());
        assert!(!ch.has_sinks());

        // Sink which returns false from add_channel
        let s2 = Arc::new(
            RecordingSink::new()
                .auto_subscribe(false)
                .add_channel_rval(false),
        );

        // Add sink with existing channel.
        ctx.add_sink(s2.clone());
        assert!(!ch.has_sinks());

        // Add channel with existing sink.
        assert!(ctx.remove_channel_for_topic(ch.topic()));
        ctx.add_channel(ch.clone()).unwrap();
        assert!(!ch.has_sinks());
    }
}
