use crate::subscription::SubscriberVec;
use crate::{nanoseconds_since_epoch, Context, Metadata, PartialMetadata, Sink};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Weak};

pub(crate) static ERROR_LOGGING_MESSAGE: &str = "Error logging message";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChannelId(u64);

impl ChannelId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl From<ChannelId> for u64 {
    fn from(id: ChannelId) -> u64 {
        id.0
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A Schema is a description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
/// You can use the well known types provided in the [crate::schemas] module or provide your own.
/// See the [MCAP spec](https://mcap.dev/spec#schema-op0x03) for more information.
#[derive(Clone, PartialEq, Debug)]
pub struct Schema {
    /// An identifier for the schema.
    pub name: String,
    /// The encoding of the schema data. For example "jsonschema" or "protobuf".
    /// The [well-known schema encodings](https://mcap.dev/spec/registry#well-known-schema-encodings) are preferred.
    pub encoding: String,
    /// Must conform to the schema encoding. If encoding is an empty string, data should be 0 length.
    pub data: Cow<'static, [u8]>,
}

impl Schema {
    /// Returns a new schema.
    pub fn new(
        name: impl Into<String>,
        encoding: impl Into<String>,
        data: impl Into<Cow<'static, [u8]>>,
    ) -> Self {
        Self {
            name: name.into(),
            encoding: encoding.into(),
            data: data.into(),
        }
    }

    /// Returns a JSON schema for the specified type.
    pub fn json_schema<T: schemars::JsonSchema>() -> Self {
        let json_schema = schemars::schema_for!(T);
        Self::new(
            std::any::type_name::<T>(),
            "jsonschema",
            serde_json::to_vec(&json_schema).expect("Failed to serialize schema"),
        )
    }
}

/// A log channel that can be used to log binary messages.
///
/// A "channel" is conceptually the same as a [MCAP channel]: it is a stream of messages which all
/// have the same type, or schema. Each channel is instantiated with a unique "topic", or name,
/// which is typically prefixed by a `/`.
///
/// [MCAP channel]: https://mcap.dev/guides/concepts#channel
///
/// If a schema was provided, all messages must be encoded according to the schema.
/// This is not checked. See [`TypedChannel`](crate::TypedChannel) for type-safe channels.
/// Channels are immutable, returned as `Arc<Channel>` and can be shared between threads.
///
/// Channels are created using [`ChannelBuilder`](crate::ChannelBuilder).
///
/// # Example
/// ```
/// use foxglove::{ChannelBuilder, Schema};
/// ```
pub struct Channel {
    pub(crate) context: Weak<Context>,
    /// id is a unique identifier for the channel inside the process,
    /// it's used to map a channel to a channel_id or subscription_id in log sinks.
    pub(crate) id: ChannelId,
    pub(crate) message_sequence: AtomicU32,
    pub(crate) topic: String,
    pub(crate) message_encoding: String,
    pub(crate) schema: Option<Schema>,
    pub(crate) metadata: BTreeMap<String, String>,
}

impl Channel {
    /// Returns the channel ID.
    pub fn id(&self) -> ChannelId {
        self.id
    }

    /// Returns the channel topic.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Returns the channel schema.
    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// Atomically increments and returns the next message sequence number.
    pub fn next_sequence(&self) -> u32 {
        self.message_sequence.fetch_add(1, Relaxed)
    }

    /// Returns true if there's at least one sink subscribed to this channel.
    pub fn has_subscribers(&self) -> bool {
        self.context
            .upgrade()
            .is_some_and(|c| c.has_subscribers(self.id))
    }

    /// Returns the set of sinks that are subscribed to this channel.
    pub(crate) fn get_subscribers(&self) -> SubscriberVec<Arc<dyn Sink>> {
        self.context
            .upgrade()
            .map(|c| c.get_subscribers(self.id))
            .unwrap_or_default()
    }

    /// Logs a message.
    pub fn log(&self, msg: &[u8]) {
        let sinks = self.get_subscribers();
        if !sinks.is_empty() {
            self.log_to_sinks(msg, PartialMetadata::default(), sinks);
        }
    }

    /// Logs a message with additional metadata.
    pub fn log_with_meta(&self, msg: &[u8], metadata: PartialMetadata) {
        let sinks = self.get_subscribers();
        if !sinks.is_empty() {
            self.log_to_sinks(msg, metadata, sinks);
        }
    }

    /// Logs a message to the provided sinks.
    pub(crate) fn log_to_sinks(
        &self,
        msg: &[u8],
        opts: PartialMetadata,
        sinks: impl IntoIterator<Item = Arc<dyn Sink>>,
    ) {
        let mut metadata = Metadata {
            sequence: opts.sequence.unwrap_or_else(|| self.next_sequence()),
            log_time: opts.log_time.unwrap_or_else(nanoseconds_since_epoch),
            publish_time: opts.publish_time.unwrap_or_default(),
        };
        // If publish_time is not set, use log_time.
        if opts.publish_time.is_none() {
            metadata.publish_time = metadata.log_time
        }
        for sink in sinks {
            if let Err(e) = sink.log(self, msg, &metadata) {
                tracing::warn!("{ERROR_LOGGING_MESSAGE}: {e}");
            }
        }
    }
}

#[cfg(test)]
impl PartialEq for Channel {
    fn eq(&self, other: &Self) -> bool {
        self.topic == other.topic
            && self.message_encoding == other.message_encoding
            && self.schema == other.schema
            && self.metadata == other.metadata
            && self.message_sequence.load(Relaxed) == other.message_sequence.load(Relaxed)
    }
}

#[cfg(test)]
impl Eq for Channel {}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("id", &self.id)
            .field("topic", &self.topic)
            .field("message_encoding", &self.message_encoding)
            .field("schema", &self.schema)
            .field("metadata", &self.metadata)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel_builder::ChannelBuilder;
    use crate::collection::collection;
    use crate::testutil::RecordingSink;
    use crate::{Channel, Context};
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
    fn test_channel_new() {
        let ctx = Context::new();
        let topic = "topic";
        let message_encoding = "message_encoding";
        let schema = Schema::new("schema_name", "schema_encoding", &[1, 2, 3]);
        let metadata: BTreeMap<String, String> =
            collection! {"key".to_string() => "value".to_string()};
        let channel = ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema.clone())
            .metadata(metadata.clone())
            .context(&ctx)
            .build()
            .expect("Failed to create channel");
        assert!(u64::from(channel.id) > 0);
        assert_eq!(channel.topic, topic);
        assert_eq!(channel.message_encoding, message_encoding);
        assert_eq!(channel.schema, Some(schema));
        assert_eq!(channel.metadata, metadata);
        assert_eq!(ctx.get_channel_by_topic(topic), Some(channel));
    }

    #[test]
    fn test_channel_next_sequence() {
        let ctx = Context::new();
        let channel = new_test_channel(&ctx, 1);
        assert_eq!(channel.next_sequence(), 1);
        assert_eq!(channel.next_sequence(), 2);
    }

    #[traced_test]
    #[test]
    fn test_channel_log_msg() {
        let ctx = Context::new();
        let channel = Arc::new(new_test_channel(&ctx, 1));
        let msg = vec![1, 2, 3];
        channel.log(&msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));
    }

    #[traced_test]
    #[test]
    fn test_log_msg_success() {
        let ctx = Context::new();
        let recording_sink = Arc::new(RecordingSink::new());

        assert!(ctx.add_sink(recording_sink.clone()));

        let channel = new_test_channel(&ctx, 1);
        ctx.add_channel(channel.clone()).unwrap();
        let msg = b"test_message";

        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));

        let recorded = recording_sink.recorded.lock();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].channel_id, channel.id());
        assert_eq!(recorded[0].msg, msg.to_vec());
        assert_eq!(recorded[0].metadata.sequence, 1);
        assert_eq!(
            recorded[0].metadata.log_time,
            recorded[0].metadata.publish_time
        );
        assert!(recorded[0].metadata.log_time > 1732847588055322395);
    }

    #[test]
    fn test_channel_subscribers() {
        let ctx = Context::new();
        let ch = new_test_channel(&ctx, 1);
        assert!(!ch.has_subscribers());
        assert!(ch.get_subscribers().is_empty());

        // Add a subscriber with no subscriptions.
        let s1 = Arc::new(RecordingSink::new().auto_subscribe(false));
        assert!(ctx.add_sink(s1.clone()));
        assert!(!ch.has_subscribers());
        assert!(ch.get_subscribers().is_empty());

        // Subscribe s1.
        ctx.subscribe_channels(s1.id(), [ch.id()]);
        assert!(ch.has_subscribers());
        let subs = ch.get_subscribers();
        assert_eq!(subs.len(), 1);
        assert_eq!(Arc::as_ptr(&subs[0]), Arc::as_ptr(&s1));

        // Add a subscriber with auto-subscribe enabled (default).
        let s2 = Arc::new(RecordingSink::new());
        assert!(ctx.add_sink(s2.clone()));
        assert!(ch.has_subscribers());
        let subs = ch.get_subscribers();
        assert_eq!(subs.len(), 2);

        // Unsubscribe s1.
        ctx.unsubscribe_channels(s1.id(), [ch.id()]);
        assert!(ch.has_subscribers());
        let subs = ch.get_subscribers();
        assert_eq!(subs.len(), 1);
        assert_eq!(Arc::as_ptr(&subs[0]), Arc::as_ptr(&s2));

        // Unsubscribe has no effect for s2, since auto-subscribe was set.
        ctx.unsubscribe_channels(s2.id(), [ch.id()]);
        assert!(ch.has_subscribers());
        let subs = ch.get_subscribers();
        assert_eq!(subs.len(), 1);
        assert_eq!(Arc::as_ptr(&subs[0]), Arc::as_ptr(&s2));

        // Unregister sinks.
        ctx.remove_sink(s1.id());
        ctx.remove_sink(s2.id());
        assert!(!ch.has_subscribers());
        assert!(ch.get_subscribers().is_empty());
    }
}
