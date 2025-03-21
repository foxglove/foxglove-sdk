use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

use serde::{Deserialize, Serialize};

mod raw_channel;
pub use raw_channel::Channel;

/// Uniquely identifies a channel in the context of this program.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChannelId(u64);

impl ChannelId {
    #[cfg(test)]
    pub(crate) fn new(id: u64) -> Self {
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

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test {
    use crate::channel_builder::ChannelBuilder;
    use crate::collection::collection;
    use crate::log_sink_set::ERROR_LOGGING_MESSAGE;
    use crate::testutil::RecordingSink;
    use crate::{Channel, Context, Schema};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use tracing_test::traced_test;

    fn new_test_channel() -> Arc<Channel> {
        Channel::new(
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
            .build()
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
        let channel = new_test_channel();
        assert_eq!(channel.next_sequence(), 1);
        assert_eq!(channel.next_sequence(), 2);
    }

    #[traced_test]
    #[test]
    fn test_channel_log_msg() {
        let channel = Arc::new(new_test_channel());
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

        let channel = new_test_channel();
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
