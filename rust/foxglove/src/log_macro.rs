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
            // We specified a message encoding, so the builder cannot fail.
            unreachable!("Failed to create channel {e}")
        });
    ChannelPlaceholder::new(channel)
}

/// Log a message for a topic to the default Context.
///
/// $topic: string literal topic name
/// $msg: expression to log, must implement Encode trait
///
/// Optional keyword arguments:
/// - log_time: timestamp when the message was logged. It can be a u64 (nanoseconds since epoch),
///   a foxglove [`Timestamp`][crate::schemas::Timestamp], a [`SystemTime`][std::time::SystemTime],
///   or anything else that implements [`ToUnixNanos`][crate::ToUnixNanos].
///
/// If a channel for the topic already exists in the default Context, it will be used.
/// Otherwise, a new channel will be created. Either way, the channel is never removed
/// from the Context. Panics if the existing channel schema or message_encoding
/// is incompatible with $msg.
///
/// The type of message to log! should be consistent for each call site to log!.
/// This is usually true in Rust, but it's possible in a function generic on the message type,
/// to pass different message types in the same call site for log!, for the same underlying
/// channel with no error at compile time or runtime. The schema will still be the schema the
/// channel was first created with, and the messages won't match the schema,
/// and will not behave well in the Foxglove app. You should avoid doing this.
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
            $crate::PartialMetadata::with_log_time($log_time)
        )
    }};
}

/// Log a message for a topic with additional metadata. See [`log!`] for more information.
///
/// $topic: string literal topic name
/// $msg: expression to log, must implement Encode trait
/// $metadata: [`PartialMetadata`] struct.
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
    use crate::schemas::{Color, LaserScan, Log, Timestamp};
    use crate::{testutil::RecordingSink, Context};
    use crate::{FoxgloveError, Schema};

    use super::*;
    use crate::testutil::GlobalContextTest;

    fn serialize<T: Encode>(msg: &T) -> Vec<u8> {
        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        buf
    }

    #[test]
    fn test_log_macro() {
        let _cleanup = GlobalContextTest::new();

        let now = nanoseconds_since_epoch();
        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        let mut log_messages = Vec::new();
        for line in 1..=3 {
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

        let timestamp = Timestamp::now();

        log!("foo", log_messages[0], log_time = 123);
        log!("foo", log_messages[1]);
        log!("foo", log_messages[2], log_time = timestamp);
        log!("foo", Color::default());

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].msg, serialize(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);

        assert_eq!(messages[1].msg, serialize(&log_messages[1]));
        assert!(messages[1].metadata.log_time >= now);

        assert_eq!(messages[2].msg, serialize(&log_messages[2]));
        assert_eq!(messages[2].metadata.log_time, timestamp.total_nanos());

        assert_eq!(messages[3].msg, serialize(&Color::default()));
        assert!(messages[3].metadata.log_time >= now);

        // Even though there are four log! callsites, which each construct separate channels, the
        // channels were exactly the same, and we properly deduped them when they were registered to the context.
        assert_eq!(messages[0].channel_id, messages[1].channel_id);
        assert_eq!(messages[0].channel_id, messages[2].channel_id);

        // The final callsite used a different schema, and so it is a distinct channel.
        assert_ne!(messages[0].channel_id, messages[3].channel_id);
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
        assert_eq!(messages[0].msg, serialize(&log_messages[0]));
        assert_eq!(messages[0].metadata.log_time, 123);
        assert_eq!(messages[1].msg, serialize(&log_messages[1]));
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

    #[test]
    #[traced_test]
    fn test_log_macro_inside_generic_function() {
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

        fn generic_func<T: Encode>(x: T) {
            log!("foo", x);
        }

        generic_func(Foo { x: 1 });
        generic_func(Bar { x: 1 });

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize(&Foo { x: 1 }));
        assert_eq!(messages[1].msg, serialize(&Bar { x: 1 }));
    }
}
