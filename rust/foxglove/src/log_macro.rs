use std::mem::ManuallyDrop;
use std::sync::atomic::{
    AtomicPtr,
    Ordering::{Acquire, Release},
};
use std::sync::Arc;

use crate::{Channel, ChannelBuilder, Context, Encode, TypedChannel};

struct TypedChannelPlaceholder {}

impl TypedChannelPlaceholder {
    fn new(channel: Arc<Channel>) -> *mut Self {
        Arc::into_raw(channel) as *mut Self
    }

    unsafe fn log<T: Encode>(channel_ptr: *mut Self, msg: &T) {
        // Safety: we're restoring the Arc<Channel> we leaked into_raw in new()
        let channel_arc = Arc::from_raw(channel_ptr as *mut Channel);
        let typed_channel = ManuallyDrop::new(TypedChannel::<T>::from_channel(channel_arc));
        typed_channel.log(msg);
    }
}

#[cold]
fn create_channel<T: Encode>(
    topic: &str,
    _: &T,
    context: &Arc<Context>,
) -> *mut TypedChannelPlaceholder {
    let channel = ChannelBuilder::new(topic)
        .schema(T::get_schema())
        .message_encoding(T::get_message_encoding())
        .context(context)
        .build()
        .unwrap_or_else(|e| {
            context.get_channel_by_topic(topic).unwrap_or_else(|| {
                panic!("Failed to create channel: {}", e);
            })
        });
    TypedChannelPlaceholder::new(channel)
}

macro_rules! log {
    ($topic:literal, $msg:expr) => {{
        static CHANNEL: AtomicPtr<TypedChannelPlaceholder> = AtomicPtr::new(std::ptr::null_mut());
        let mut channel_ptr = CHANNEL.load(Acquire);
        if channel_ptr.is_null() {
            channel_ptr = create_channel($topic, &$msg, &Context::get_default());
            CHANNEL.store(channel_ptr, Release);
        }
        // Safety: channel_ptr was created above by create_channel, it's safe to pass to log
        unsafe { TypedChannelPlaceholder::log(channel_ptr, &$msg) };
    }};
}

#[cfg(test)]
mod tests {
    use crate::schemas::Log;
    use crate::{testutil::RecordingSink, Context};

    use super::*;

    fn serialize_log(log: &Log) -> Vec<u8> {
        let mut buf = Vec::new();
        log.encode(&mut buf).unwrap();
        buf
    }

    #[test]
    fn test_log() {
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
            log!("test", msg);
            log_messages.push(msg);
        }

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, serialize_log(&log_messages[0]));
        assert_eq!(messages[1].msg, serialize_log(&log_messages[1]));
    }
}
