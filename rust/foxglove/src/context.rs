use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

#[allow(dead_code)]
mod subscriptions;

use crate::log_sink_set::LogSinkSet;
use crate::{Channel, FoxgloveError, Sink};

/// A context is a collection of channels and sinks.
///
/// To obtain a reference to the default context, use [`Context::get_default`]. To construct a new
/// context, use [`Context::new`].
pub struct Context {
    /// Map of channels by topic.
    channels: RwLock<HashMap<String, Arc<Channel>>>,
    sinks: LogSinkSet,
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
            sinks: LogSinkSet::new(),
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
        self.sinks.for_each(|sink| {
            if channel.sinks.add_sink(sink.clone()) {
                sink.add_channel(&channel);
            }
            Ok(())
        });
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

        self.sinks.for_each(|sink| {
            if channel.sinks.remove_sink(sink) {
                sink.remove_channel(channel);
            }
            Ok(())
        });
        true
    }

    /// Adds a sink to the context.
    pub fn add_sink(&self, sink: Arc<dyn Sink>) -> bool {
        if !self.sinks.add_sink(sink.clone()) {
            return false;
        }

        // Add the sink to all existing channels.
        for channel in self.channels.read().values() {
            if channel.sinks.add_sink(sink.clone()) {
                sink.add_channel(channel);
            }
        }

        true
    }

    /// Removes a sink from the context.
    pub fn remove_sink(&self, sink: &Arc<dyn Sink>) -> bool {
        if !self.sinks.remove_sink(sink) {
            return false;
        }

        // TODO this has a bug, if the same sink was added to a channel twice, via two different contexts,
        // this will remove the sink from the channel, even although they're still associated via the other context.
        // If we stored the contexts on the channel, and removed the contexts, it would fix it,
        // But logging would be via an extra indirection to context (slower) and
        // having it associated with the same sink twice would result in two log calls to the sink,
        // which is a more serious bug.
        // I think the solution should be to have both the contexts and the sinks on the channel.
        // This also fixes the problems with Channel::close().
        // FG-9893

        // Remove the sink from all existing channels.
        for channel in self.channels.read().values() {
            if channel.sinks.remove_sink(sink) {
                sink.remove_channel(channel);
            }
        }

        true
    }

    /// Removes all channels and sinks from the context.
    pub fn clear(&self) {
        let channels: HashMap<_, _> = std::mem::take(&mut self.channels.write());
        self.sinks.for_each(|sink| {
            for channel in channels.values() {
                sink.remove_channel(channel);
                channel.sinks.clear();
            }
            Ok(())
        });
        self.sinks.clear();
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::channel::ChannelId;
    use crate::collection::collection;
    use crate::context::*;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::{ErrorSink, MockSink, RecordingSink};
    use crate::{nanoseconds_since_epoch, Channel, PartialMetadata, Schema};
    use std::sync::atomic::AtomicU32;
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel(id: u64) -> Arc<Channel> {
        Arc::new(Channel {
            sinks: LogSinkSet::new(),
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
        let sink: Arc<dyn Sink> = sink;
        assert!(ctx.remove_sink(&sink));

        // Try to remove a sink that doesn't exist
        let sink3: Arc<dyn Sink> = sink3;
        assert!(!ctx.remove_sink(&sink3));

        // Test removing the last sink
        let sink2: Arc<dyn Sink> = sink2;
        assert!(ctx.remove_sink(&sink2));
    }

    #[traced_test]
    #[test]
    fn test_log_calls_sinks() {
        let ctx = Context::new();
        let sink1 = Arc::new(RecordingSink::new());
        let sink2 = Arc::new(RecordingSink::new());

        assert!(ctx.add_sink(sink1.clone()));
        assert!(ctx.add_sink(sink2.clone()));

        let channel = new_test_channel(1);
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

        let channel = new_test_channel(1);
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
        let channel = Arc::new(new_test_channel(1));
        let msg = b"test_message";

        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));
    }
}
