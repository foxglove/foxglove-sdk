use crate::channel::ChannelId;
use crate::subscription::{SubscriberVec, SubscriptionManager};
use crate::{Channel, FoxgloveError, Sink, SinkId};
use parking_lot::RwLock;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};

/// A context is a collection of channels and sinks.
///
/// To obtain a reference to the default context, use [`Context::get_default`]. To construct a new
/// context, use [`Context::new`].
pub struct Context {
    /// Map of channels by topic.
    channels: RwLock<HashMap<String, Arc<Channel>>>,
    sinks: RwLock<HashMap<SinkId, Arc<dyn Sink>>>,
    sub: SubscriptionManager<SinkId, Arc<dyn Sink>>,
}

impl Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

impl Context {
    /// Instantiates a new context.
    #[allow(clippy::new_without_default)] // avoid confusion with Context::get_default()
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channels: RwLock::default(),
            sinks: RwLock::default(),
            sub: SubscriptionManager::default(),
        })
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
        let channels = self.channels.read();
        channels.get(topic).cloned()
    }

    /// Adds a channel to the context.
    pub fn add_channel(&self, channel: Arc<Channel>) -> Result<(), FoxgloveError> {
        {
            // Wrapped in a block, so we release the lock immediately.
            let mut channels = self.channels.write();
            let topic = &channel.topic;
            let Entry::Vacant(entry) = channels.entry(topic.clone()) else {
                return Err(FoxgloveError::DuplicateChannel(topic.clone()));
            };
            entry.insert(channel.clone());
        }
        for sink in self.sinks.read().values() {
            sink.add_channel(&channel);
        }
        Ok(())
    }

    /// Removes the channel for the specified topic.
    pub fn remove_channel_for_topic(&self, topic: &str) -> bool {
        let maybe_channel_by_topic = {
            let mut channels = self.channels.write();
            channels.remove(topic)
        };

        let Some(channel_by_topic) = maybe_channel_by_topic else {
            // Channel not found.
            return false;
        };
        let channel = &*channel_by_topic;

        for sink in self.sinks.read().values() {
            sink.remove_channel(channel);
        }
        true
    }

    /// Adds a sink to the context.
    ///
    /// If [`Sink::auto_subscribe`] returns true, the sink will be automatically subscribed to all
    /// present and future channels on the context. Otherwise, the sink is expected to manage its
    /// subscriptions dynamically with [`Context::subscribe_channels`] and
    /// [`Context::unsubscribe_channels`].
    pub fn add_sink(&self, sink: Arc<dyn Sink>) -> bool {
        let sink_id = sink.id();
        match self.sinks.write().entry(sink_id) {
            Entry::Vacant(e) => {
                if sink.auto_subscribe() {
                    self.sub.subscribe_global(sink_id, sink.clone());
                }
                e.insert(sink);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    /// Removes a sink from the context.
    pub fn remove_sink(&self, sink_id: SinkId) -> bool {
        let mut sinks = self.sinks.write();
        if sinks.remove(&sink_id).is_some() {
            self.sub.remove_subscriber(&sink_id);
            true
        } else {
            false
        }
    }

    /// Adds a sink subscription to the specified channels.
    pub fn subscribe_channels(
        &self,
        sink_id: SinkId,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) {
        if let Some(sink) = self.sinks.read().get(&sink_id).cloned() {
            self.sub.subscribe_channels(sink_id, sink, channel_ids);
        }
    }

    /// Removes a sink subscription from the specified channels.
    pub fn unsubscribe_channels(
        &self,
        sink_id: SinkId,
        channel_ids: impl IntoIterator<Item = ChannelId>,
    ) {
        self.sub.unsubscribe_channels(&sink_id, channel_ids);
    }

    /// Returns true if there's at least one sink subscribed to this channel.
    pub fn has_subscribers(&self, channel_id: ChannelId) -> bool {
        self.sub.has_subscribers(channel_id)
    }

    /// Returns the set of sinks that are subscribed to this channel.
    pub fn get_subscribers(&self, channel_id: ChannelId) -> SubscriberVec<Arc<dyn Sink>> {
        self.sub.get_subscribers(channel_id)
    }

    /// Removes all channels and sinks from the context.
    pub fn clear(&self) {
        let channels: HashMap<_, _> = std::mem::take(&mut self.channels.write());
        let mut sinks = self.sinks.write();
        for (_, sink) in sinks.drain() {
            for channel in channels.values() {
                sink.remove_channel(channel);
            }
        }
        self.sub.clear();
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::channel::{ChannelId, ERROR_LOGGING_MESSAGE};
    use crate::collection::collection;
    use crate::context::*;
    use crate::testutil::{ErrorSink, MockSink, RecordingSink};
    use crate::{nanoseconds_since_epoch, Channel, PartialMetadata, Schema};
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel(ctx: &Arc<Context>, id: u64) -> Arc<Channel> {
        Arc::new(Channel {
            context: Arc::downgrade(ctx),
            id: ChannelId::new(id),
            message_sequence: AtomicU32::new(1),
            topic: "topic".to_string(),
            message_encoding: "message_encoding".to_string(),
            schema: Some(Schema::new(
                "name",
                "encoding",
                br#"{
                    "type": "object",
                    "properties": {
                        "msg": {"type": "string"},
                        "count": {"type": "number"},
                    },
                }"#,
            )),
            metadata: collection! {"key".to_string() => "value".to_string()},
        })
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

        let channel = new_test_channel(&ctx, 1);
        ctx.add_channel(channel.clone()).unwrap();
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

        let channel = new_test_channel(&ctx, 1);
        ctx.add_channel(channel.clone()).unwrap();
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
        let channel = Arc::new(new_test_channel(&ctx, 1));
        let msg = b"test_message";

        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));
    }
}
