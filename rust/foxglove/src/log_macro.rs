use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::{Channel, ChannelBuilder, Context, Encode, PartialMetadata, RawChannel};

#[doc(hidden)]
pub struct ChannelPlaceholder {}

impl ChannelPlaceholder {
    pub fn new(channel: Arc<RawChannel>) -> *mut Self {
        Arc::into_raw(channel) as *mut Self
    }

    pub unsafe fn log<T: Encode>(channel_ptr: *mut Self, msg: &T, metadata: PartialMetadata) {
        // Safety: we're restoring the Arc<RawChannel> we leaked into_raw in new()
        let channel_arc = Arc::from_raw(channel_ptr as *mut RawChannel);
        // We can safely create a Channel from any Arc<RawChannel>
        let channel = ManuallyDrop::new(Channel::<T>::from_raw_channel(channel_arc));
        channel.log_with_meta(msg, metadata);
    }
}

#[doc(hidden)]
#[cold]
pub fn create_channel<T: Encode>(
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

/// Log a message for a topic to the default Context.
///
/// $topic: string literal topic name
/// $msg: expression to log, must implement Encode trait
///
/// Optional keyword arguments:
/// - log_time: timestamp when the message was logged. See [`PartialMetadata`].
///
/// If a channel for the topic already exists in the default Context, it will be used.
/// Otherwise, a new channel will be created. Either way, the channel is never removed
/// from the Context. Panics if the existing channel schema or message_encoding
/// is incomptable with $msg.
///
/// Panics if a channel can't be created for $msg.
#[macro_export]
macro_rules! log {
    ($topic:literal, $msg:expr $(,)? ) => {{
        $crate::log_with_meta!($topic, $msg, $crate::PartialMetadata::default())
    }};

    ($topic:literal, $msg:expr, log_time = $log_time:expr $(,)? ) => {{
        $crate::log_with_meta!(
            $topic,
            $msg,
            $crate::PartialMetadata {
                log_time: Some($log_time),
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
        static CHANNEL: std::sync::atomic::AtomicPtr<$crate::log_macro::ChannelPlaceholder> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        let mut channel_ptr = CHANNEL.load(std::sync::atomic::Ordering::Acquire);
        if channel_ptr.is_null() {
            channel_ptr =
                $crate::log_macro::create_channel($topic, &$msg, &$crate::Context::get_default());
            CHANNEL.store(channel_ptr, std::sync::atomic::Ordering::Release);
        }
        // Safety: channel_ptr was created above by create_channel, it's safe to pass to log
        unsafe { $crate::log_macro::ChannelPlaceholder::log(channel_ptr, &$msg, $metadata) };
    }};
}

#[cfg(test)]
mod tests {
    use bytes::BufMut;
    use tracing_test::traced_test;

    use crate::nanoseconds_since_epoch;
    use crate::schemas::{LaserScan, Log};
    use crate::{testutil::RecordingSink, Context};
    use crate::{FoxgloveError, Schema};

    use super::*;
    use crate::testutil::GlobalContextTest;

    fn serialize_log(log: &Log) -> Vec<u8> {
        let mut buf = Vec::new();
        log.encode(&mut buf).unwrap();
        buf
    }

    #[test]
    fn test_log_macro() {
        let _cleanup = GlobalContextTest::new();

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
        log!("foo", log_messages[1]);

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize_log(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);

        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
        assert!(messages[1].metadata.log_time >= now);
    }

    #[test]
    fn test_log_in_loop() {
        let _cleanup = GlobalContextTest::new();

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
        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
        assert_eq!(messages[1].metadata.log_time, 123);
    }

    #[test]
    #[traced_test]
    fn test_log_existing_channel_different_schema_warns() {
        let _cleanup = GlobalContextTest::new();

        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        let _channel = ChannelBuilder::new("foo").build::<LaserScan>();

        log!(
            "foo",
            Log {
                timestamp: None,
                level: 1,
                message: "Hello, world!".to_string(),
                name: "".to_string(),
                file: "".to_string(),
                line: 1,
            }
        );

        assert!(logs_contain(
            "Channel with topic foo already exists in this context"
        ));
    }

    #[test]
    #[traced_test]
    fn test_log_existing_channel_different_encoding_warns() {
        let _cleanup = GlobalContextTest::new();

        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        struct Foo {
            x: u32,
        }

        impl Encode for Foo {
            type Error = FoxgloveError;

            fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
                buf.put_u32(self.x);
                Ok(())
            }

            fn get_schema() -> Option<Schema> {
                None
            }

            fn get_message_encoding() -> String {
                "foo".to_string()
            }
        }

        struct Bar {
            x: u32,
        }

        impl Encode for Bar {
            type Error = FoxgloveError;

            fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
                buf.put_u32(self.x);
                Ok(())
            }

            fn get_schema() -> Option<Schema> {
                None
            }

            fn get_message_encoding() -> String {
                "bar".to_string()
            }
        }

        let _channel = ChannelBuilder::new("foo").build::<Foo>();

        log!("foo", Bar { x: 1 });

        assert!(logs_contain(
            "Channel with topic foo already exists in this context"
        ));
    }
}
