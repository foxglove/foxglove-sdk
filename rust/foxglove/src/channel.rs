use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use delegate::delegate;
use serde::{Deserialize, Serialize};
use smallbytes::SmallBytes;

use crate::{metadata::ToUnixNanos, ChannelBuilder, Encode, PartialMetadata, Schema};

mod lazy_channel;
mod raw_channel;

pub use lazy_channel::{LazyChannel, LazyRawChannel};
pub use raw_channel::RawChannel;

/// Stack buffer size to use for encoding messages.
const STACK_BUFFER_SIZE: usize = 256 * 1024;

/// Uniquely identifies a channel in the context of this program.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChannelId(u64);

impl ChannelId {
    /// Returns a new ChannelId
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Allocates the next channel ID.
    pub(crate) fn next() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Relaxed);
        Self(id)
    }
}

impl From<ChannelId> for u64 {
    fn from(id: ChannelId) -> u64 {
        id.0
    }
}

impl From<u64> for ChannelId {
    fn from(value: u64) -> Self {
        ChannelId::new(value)
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A channel for messages that implement [`Encode`].
///
/// Channels are immutable, returned as `Arc<Channel>` and can be shared between threads.
#[derive(Debug)]
pub struct Channel<T: Encode> {
    inner: Arc<RawChannel>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Encode> Channel<T> {
    /// Constructs a new typed channel with default settings.
    ///
    /// If you want to override the channel configuration, use [`ChannelBuilder`].
    ///
    /// You should choose a unique topic name per channel for compatibility with the Foxglove app.
    pub fn new(topic: impl Into<String>) -> Self {
        ChannelBuilder::new(topic).build()
    }

    /// Constructs a new typed channel from a raw channel.
    ///
    /// This is intended for internal use only.
    /// We're trusting the caller that the channel was created with the same type T as being used to call this.
    #[doc(hidden)]
    pub fn from_raw_channel(raw_channel: Arc<RawChannel>) -> Self {
        Self {
            inner: raw_channel,
            _phantom: std::marker::PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn into_inner(self) -> Arc<RawChannel> {
        self.inner
    }

    delegate! { to self.inner {
        /// Returns the channel ID.
        pub fn id(&self) -> ChannelId;

        /// Returns the topic name of the channel.
        pub fn topic(&self) -> &str;

        /// Returns the channel schema.
        pub fn schema(&self) -> Option<&Schema>;

        /// Returns the message encoding for this channel.
        pub fn message_encoding(&self) -> &str;

        /// Returns the metadata for this channel.
        pub fn metadata(&self) -> &BTreeMap<String, String>;

        /// Returns true if there's at least one sink subscribed to this channel.
        pub fn has_sinks(&self) -> bool;

        /// Closes the channel, removing it from the context.
        ///
        /// You can use this to explicitly unadvertise the channel to sinks that subscribe to
        /// channels dynamically, such as the [`WebSocketServer`][crate::WebSocketServer].
        ///
        /// Attempts to log on a closed channel will elicit a throttled warning message.
        pub fn close(&self);
    } }

    /// Encodes the message and logs it on the channel.
    ///
    /// The buffering behavior depends on the log sink; see [`McapWriter`][crate::McapWriter] and
    /// [`WebSocketServer`][crate::WebSocketServer] for details.
    pub fn log(&self, msg: &T) {
        self.log_with_meta(msg, PartialMetadata::default());
    }

    /// Encodes the message and logs it on the channel with additional metadata.
    ///
    /// The buffering behavior depends on the log sink; see [`McapWriter`][crate::McapWriter] and
    /// [`WebSocketServer`][crate::WebSocketServer] for details.
    pub fn log_with_meta(&self, msg: &T, metadata: PartialMetadata) {
        if self.has_sinks() {
            self.log_to_sinks(msg, metadata);
        } else {
            self.inner.log_warn_if_closed();
        }
    }

    /// Encodes the message and logs it on the channel with the given `timestamp`.
    /// `timestamp` can be a u64 (nanoseconds since epoch), a foxglove [`Timestamp`][crate::schemas::Timestamp],
    /// a [`SystemTime`][std::time::SystemTime], or anything else that implements [`ToUnixNanos`][crate::ToUnixNanos].
    ///
    /// The buffering behavior depends on the log sink; see [`McapWriter`][crate::McapWriter] and
    /// [`WebSocketServer`][crate::WebSocketServer] for details.
    pub fn log_with_time(&self, msg: &T, timestamp: impl ToUnixNanos) {
        self.log_with_meta(msg, PartialMetadata::with_log_time(timestamp))
    }

    fn log_to_sinks(&self, msg: &T, metadata: PartialMetadata) {
        // Try to avoid heap allocation by using a stack buffer.
        let mut buf: SmallBytes<STACK_BUFFER_SIZE> = SmallBytes::new();
        if let Some(estimated_size) = msg.encoded_len() {
            buf.reserve(estimated_size);
        }

        msg.encode(&mut buf).unwrap();
        self.inner.log_to_sinks(&buf, metadata);
    }
}

#[cfg(test)]
mod test {
    use crate::channel_builder::ChannelBuilder;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::RecordingSink;
    use crate::{Context, FoxgloveError, RawChannel, Schema};
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel(ctx: &Arc<Context>) -> Result<Arc<RawChannel>, FoxgloveError> {
        ChannelBuilder::new("/topic")
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
            .build_raw()
    }

    #[test]
    fn test_channel_new() {
        let ctx = Context::new();
        let topic = "topic";
        let message_encoding = "message_encoding";
        let schema = Schema::new("schema_name", "schema_encoding", &[1, 2, 3]);
        let metadata = maplit::btreemap! {"key".to_string() => "value".to_string()};
        let channel = ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema.clone())
            .metadata(metadata.clone())
            .context(&ctx)
            .build_raw()
            .expect("Failed to create channel");
        assert!(u64::from(channel.id()) > 0);
        assert_eq!(channel.topic(), topic);
        assert_eq!(channel.message_encoding(), message_encoding);
        assert_eq!(channel.schema(), Some(&schema));
        assert_eq!(channel.metadata(), &metadata);
        assert_eq!(ctx.get_channel_by_topic(topic), Some(channel));
    }

    #[traced_test]
    #[test]
    fn test_channel_log_msg() {
        let ctx = Context::new();
        let channel = new_test_channel(&ctx).unwrap();
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

        let channel = new_test_channel(&ctx).unwrap();
        let msg = b"test_message";

        channel.log(msg);
        assert!(!logs_contain(ERROR_LOGGING_MESSAGE));

        let messages = recording_sink.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel_id, channel.id());
        assert_eq!(messages[0].msg, msg.to_vec());
        assert!(messages[0].metadata.log_time > 1732847588055322395);
    }

    #[traced_test]
    #[test]
    fn test_channel_close() {
        let ctx = Context::new();
        let ch = new_test_channel(&ctx).unwrap();
        ch.log(b"");
        assert!(!logs_contain("Cannot log on closed channel for /topic"));

        // Explicitly close the channel.
        ch.close();
        ch.log(b"");
        assert!(logs_contain("Cannot log on closed channel for /topic"));
    }

    #[traced_test]
    #[test]
    fn test_channel_closed_by_context() {
        let ctx = Context::new();
        let ch = new_test_channel(&ctx).unwrap();
        ch.log(b"");
        assert!(!logs_contain("Cannot log on closed channel for /topic"));

        // Drop the context, which effectively closes the channel.
        drop(ctx);
        ch.log(b"");
        assert!(logs_contain("Cannot log on closed channel for /topic"));
    }
}
