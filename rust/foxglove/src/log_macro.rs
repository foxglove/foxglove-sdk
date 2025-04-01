use std::mem::ManuallyDrop;
use std::sync::atomic::{
    AtomicPtr,
    Ordering::{Acquire, Release},
};
use std::sync::Arc;

use crate::{Channel, ChannelBuilder, Context, Encode, PartialMetadata, RawChannel};

struct ChannelPlaceholder {}

impl ChannelPlaceholder {
    fn new(channel: Arc<RawChannel>) -> *mut Self {
        Arc::into_raw(channel) as *mut Self
    }

    unsafe fn log<T: Encode>(channel_ptr: *mut Self, msg: &T, metadata: PartialMetadata) {
        // Safety: we're restoring the Arc<Channel> we leaked into_raw in new()
        let channel_arc = Arc::from_raw(channel_ptr as *mut RawChannel);
        // We can safely create a TypedChannel from any Arc<Channel>
        let channel = ManuallyDrop::new(Channel::<T>::from_raw_channel(channel_arc));
        channel.log_with_meta(msg, metadata);
    }
}

#[cold]
fn create_channel<T: Encode>(
    topic: &str,
    _: &T,
    context: &Arc<Context>,
) -> *mut ChannelPlaceholder {
    let channel = ChannelBuilder::new(topic)
        .schema(T::get_schema())
        .message_encoding(T::get_message_encoding())
        .context(context)
        .build_raw()
        .unwrap_or_else(|e| {
            // If the channel already exists, we can use the existing channel
            // only if the schema and message encoding are compatible.
            let existing_channel = context.get_channel_by_topic(topic).unwrap_or_else(|| {
                panic!("Failed to create channel: {}", e);
            });
            let schema = T::get_schema();
            if existing_channel.schema() != schema.as_ref() {
                panic!("Channel {} already exists with different schema", topic);
            }
            if existing_channel.message_encoding() != T::get_message_encoding() {
                panic!(
                    "Channel {} already exists with different message encoding",
                    topic
                );
            }
            existing_channel
        });
    ChannelPlaceholder::new(channel)
}

/// Log a message for a topic.
///
/// $topic: string literal topic name
/// $msg: expression to log, must implement Encode trait
///
/// Optional keyword arguments:
/// - log_time: timestamp when the message was logged
/// - publish_time: timestamp when the message was published
/// - sequence: sequence number of the message
///
/// If a channel for the topic already exists in the Context, it will be used.
/// Otherwise, a new channel will be created.
/// Either way, the channel exists until the end of the process.
///
/// Panics if a channel can't be created for $msg
/// or if $topic names an existing channel, and $msg specifies a schema or message_encoding incomptable with the existing channel.
#[macro_export]
macro_rules! log {
    // Base case with just topic and message
    ($topic:literal, $msg:expr) => {{
        $crate::log_with_meta!($topic, $msg, $crate::PartialMetadata::default())
    }};

    // Cases with different combinations of keyword arguments
    ($topic:literal, $msg:expr, log_time = $log_time:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: Some($log_time),
                publish_time: None,
                sequence: None,
            }
        )
    }};

    ($topic:literal, $msg:expr, publish_time = $publish_time:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: None,
                publish_time: Some($publish_time),
                sequence: None,
            }
        )
    }};

    ($topic:literal, $msg:expr, sequence = $sequence:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: None,
                publish_time: None,
                sequence: Some($sequence),
            }
        )
    }};

    ($topic:literal, $msg:expr, log_time = $log_time:expr, publish_time = $publish_time:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: Some($log_time),
                publish_time: Some($publish_time),
                sequence: None,
            }
        )
    }};

    ($topic:literal, $msg:expr, log_time = $log_time:expr, sequence = $sequence:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: Some($log_time),
                publish_time: None,
                sequence: Some($sequence),
            }
        )
    }};

    ($topic:literal, $msg:expr, publish_time = $publish_time:expr, sequence = $sequence:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: None,
                publish_time: Some($publish_time),
                sequence: Some($sequence),
            }
        )
    }};

    // Case with all keyword arguments specified
    ($topic:literal, $msg:expr, log_time = $log_time:expr, publish_time = $publish_time:expr, sequence = $sequence:expr) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: Some($log_time),
                publish_time: Some($publish_time),
                sequence: Some($sequence),
            }
        )
    }};
}

/// Log a message for a topic with additional metadata.
///
/// $topic: string literal topic name
/// $msg: expression to log, must implement Encode trait
/// $metadata: PartialMetadata struct
#[doc(hidden)]
#[macro_export]
macro_rules! log_with_meta {
    ($topic:literal, $msg:expr, $metadata:expr) => {{
        static CHANNEL: AtomicPtr<ChannelPlaceholder> = AtomicPtr::new(std::ptr::null_mut());
        let mut channel_ptr = CHANNEL.load(Acquire);
        if channel_ptr.is_null() {
            channel_ptr = create_channel($topic, &$msg, &Context::get_default());
            CHANNEL.store(channel_ptr, Release);
        }
        // Safety: channel_ptr was created above by create_channel, it's safe to pass to log
        unsafe { $crate::log_macro::ChannelPlaceholder::log(channel_ptr, &$msg, $metadata) };
    }};
}

#[cfg(test)]
mod tests {
    use crate::nanoseconds_since_epoch;
    use crate::schemas::{Log, ;
    use crate::{testutil::RecordingSink, Context};

    use super::*;

    fn serialize_log(log: &Log) -> Vec<u8> {
        let mut buf = Vec::new();
        log.encode(&mut buf).unwrap();
        buf
    }

    #[test]
    fn test_log() {
        let now = nanoseconds_since_epoch();
        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        let mut log_messages = Vec::new();
        for line in 1..=2 {
            let msg = Log {
                timestamp: None,
                level: 1,
                message: "Hello, world!".to_string(),
                name: "".to_string(),
                file: "".to_string(),
                line,
            };
            log_messages.push(msg);
        }

        log!("foo", log_messages[0], log_time = 123);
        log!("foo", log_messages[1], publish_time = 123);

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize_log(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);
        assert_eq!(messages[0].metadata.publish_time, 123);
        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
        assert!(messages[1].metadata.log_time >= now);
        assert_eq!(messages[1].metadata.publish_time, 123);
        assert!(messages[1].metadata.sequence > messages[0].metadata.sequence);
    }

    #[test]
    fn test_log_in_loop() {
        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        let mut log_messages = Vec::new();
        for line in 1..=2 {
            let msg = Log {
                timestamp: None,
                level: 1,
                message: "Hello, world!".to_string(),
                name: "".to_string(),
                file: "".to_string(),
                line,
            };
            log!("foo", msg, log_time = 123);
            log_messages.push(msg);
        }

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize_log(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);
        assert_eq!(messages[0].metadata.publish_time, 123);
        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
        assert_eq!(messages[1].metadata.log_time, 123);
        assert_eq!(messages[1].metadata.publish_time, 123);
        assert!(messages[1].metadata.sequence > messages[0].metadata.sequence);
    }

    #[test]
    fn test_log_existing_channel_different_schema_panics() {
        let sink = Arc::new(RecordingSink::new());        Context::get_default().add_sink(sink.clone());

        static_channel!(CHANNEL, "bar", Log);

        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        let mut log_messages = Vec::new();
        for line in 1..=2 {
            let msg = Log {
                timestamp: None,
                level: 1,
                message: "Hello, world!".to_string(),
                name: "".to_string(),
                file: "".to_string(),
                line,
            };
            log!("bar", msg, log_time = 123);
            log_messages.push(msg);
        }

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize_log(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);
        assert_eq!(messages[0].metadata.publish_time, 123);
        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
        assert_eq!(messages[1].metadata.log_time, 123);
        assert_eq!(messages[1].metadata.publish_time, 123);
        assert!(messages[1].metadata.sequence > messages[0].metadata.sequence);
    }
}
