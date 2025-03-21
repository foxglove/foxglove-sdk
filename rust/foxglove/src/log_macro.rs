use std::mem::ManuallyDrop;
use std::sync::atomic::{
    AtomicPtr,
    Ordering::{Acquire, Release},
};
use std::sync::Arc;

use crate::{Channel, ChannelBuilder, Encode, TypedChannel};

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
fn create_channel<T: Encode>(topic: &str, _: &T) -> *mut TypedChannelPlaceholder {
    ChannelBuilder::new(topic)
        .schema(T::get_schema())
        .message_encoding(T::get_message_encoding())
        .build()
        .map(TypedChannelPlaceholder::new)
        .unwrap_or_else(|e| {
            panic!("Failed to create channel: {}", e);
        })
}

macro_rules! log {
    ($topic:literal, $msg:expr) => {{
        static CHANNEL: AtomicPtr<TypedChannelPlaceholder> = AtomicPtr::new(std::ptr::null_mut());
        let mut channel_ptr = CHANNEL.load(Acquire);
        if channel_ptr.is_null() {
            channel_ptr = create_channel($topic, &$msg);
            CHANNEL.store(channel_ptr, Release);
        }
        // Safety: channel_ptr was created above by create_channel, it's safe to pass to log
        unsafe { TypedChannelPlaceholder::log(channel_ptr, &$msg) };
    }};
}

#[cfg(test)]
mod tests {
    use crate::{testutil::RecordingSink, Context};

    use super::*;

    #[test]
    fn test_log() {
        let sink = Arc::new(RecordingSink::new());
        Context::get_default().add_sink(sink.clone());

        for _ in 0..2 {
            log!("test", b"Hello, world!");
        }

        let messages = sink.take_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].msg, b"Hello, world!");
        assert_eq!(messages[1].msg, b"Hello, world!");
    }
}
