use crate::{FoxgloveChannelDescriptor, FoxgloveString};

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context. The callback should return false to disable logging of this channel.
///
/// This method is invoked from the client's main poll loop and must not block.
#[derive(Clone)]
pub(crate) struct SinkChannelFilterHandler {
    callback_context: *const std::ffi::c_void,
    callback:
        unsafe extern "C" fn(*const std::ffi::c_void, *const FoxgloveChannelDescriptor) -> bool,
}

impl SinkChannelFilterHandler {
    /// Create a new sink channel filter handler.
    pub fn new(
        callback_context: *const std::ffi::c_void,
        callback: unsafe extern "C" fn(
            *const std::ffi::c_void,
            *const FoxgloveChannelDescriptor,
        ) -> bool,
    ) -> Self {
        Self {
            callback_context,
            callback,
        }
    }
}

unsafe impl Send for SinkChannelFilterHandler {}
unsafe impl Sync for SinkChannelFilterHandler {}
impl foxglove::SinkChannelFilter for SinkChannelFilterHandler {
    /// Indicate whether the channel should be subscribed to.
    /// Safety: the channel is valid only as long as the callback.
    fn should_subscribe(&self, channel: &foxglove::ChannelDescriptor) -> bool {
        // Create a FoxgloveChannelDescriptor that wraps the Rust ChannelDescriptor
        // The callback will receive a pointer to this wrapper
        let c_channel = FoxgloveChannelDescriptor {
            topic: FoxgloveString::from(channel.topic()),
            encoding: FoxgloveString::from(channel.message_encoding()),
        };
        unsafe { (self.callback)(self.callback_context, &raw const c_channel) }
    }
}
