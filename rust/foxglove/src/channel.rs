use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use delegate::delegate;
use serde::{Deserialize, Serialize};
use smallbytes::SmallBytes;

use crate::{ChannelBuilder, Encode, FoxgloveError, PartialMetadata, Schema};

mod raw_channel;
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
    pub fn new(topic: impl Into<String>) -> Result<Self, FoxgloveError> {
        ChannelBuilder::new(topic).build()
    }

    pub(crate) fn from_raw_channel(raw_channel: Arc<RawChannel>) -> Self {
        Self {
            inner: raw_channel,
            _phantom: std::marker::PhantomData,
        }
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
        /// Future messages logged to the channel will not be received by any sink.
        pub fn close(&self);
    } }

    /// Encodes the message and logs it on the channel.
    pub fn log(&self, msg: &T) {
        if self.has_sinks() {
            self.log_to_sinks(msg, PartialMetadata::default());
        }
    }

    /// Encodes the message and logs it on the channel with additional metadata.
    pub fn log_with_meta(&self, msg: &T, metadata: PartialMetadata) {
        if self.has_sinks() {
            self.log_to_sinks(msg, metadata);
        }
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

/// Registers a static [`Channel`] for the provided topic and message type.
///
/// This macro is a wrapper around [`LazyLock<Channel<T>>`](std::sync::LazyLock),
/// which initializes the channel lazily upon first use. If the initialization fails (e.g., due to
/// [`FoxgloveError::DuplicateChannel`]), the program will panic.
///
/// If you don't require a static variable, you can just use [`Channel::new()`] directly.
///
/// The channel is created with the provided visibility and identifier, and the topic and message type.
///
/// # Example
/// ```
/// use foxglove::static_channel;
/// use foxglove::schemas::{FrameTransform, SceneUpdate};
///
/// // A locally-scoped typed channel.
/// static_channel!(TF, "/tf", FrameTransform);
///
/// // A pub(crate)-scoped typed channel.
/// static_channel!(pub(crate) BOXES, "/boxes", SceneUpdate);
///
/// // Usage (you would populate the structs, rather than using `default()`).
/// TF.log(&FrameTransform::default());
/// BOXES.log(&SceneUpdate::default());
/// ```
#[macro_export]
macro_rules! static_channel {
    ($vis:vis $ident: ident, $topic: literal, $ty: ty) => {
        $vis static $ident: std::sync::LazyLock<$crate::Channel<$ty>> =
            std::sync::LazyLock::new(|| match $crate::Channel::new($topic) {
                Ok(channel) => channel,
                Err(e) => {
                    panic!("Failed to create channel for {}: {:?}", $topic, e);
                }
            });
    };
}

#[cfg(test)]
mod test {
    use crate::channel_builder::ChannelBuilder;
    use crate::collection::collection;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::RecordingSink;
    use crate::{Context, RawChannel, Schema};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel(ctx: &Arc<Context>) -> Arc<RawChannel> {
        RawChannel::new(
            ctx,
            "topic".into(),
            "message_encoding".into(),
            Some(Schema::new(
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
            collection! {"key".to_string() => "value".to_string()},
        )
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
            .build_raw()
            .expect("Failed to create channel");
        assert!(u64::from(channel.id()) > 0);
        assert_eq!(channel.topic(), topic);
        assert_eq!(channel.message_encoding(), message_encoding);
        assert_eq!(channel.schema(), Some(&schema));
        assert_eq!(channel.metadata(), &metadata);
        assert_eq!(ctx.get_channel_by_topic(topic), Some(channel));
    }

    #[test]
    fn test_channel_next_sequence() {
        let ctx = Context::new();
        let channel = new_test_channel(&ctx);
        assert_eq!(channel.next_sequence(), 1);
        assert_eq!(channel.next_sequence(), 2);
    }

    #[traced_test]
    #[test]
    fn test_channel_log_msg() {
        let ctx = Context::new();
        let channel = Arc::new(new_test_channel(&ctx));
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

        let channel = new_test_channel(&ctx);
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
}
