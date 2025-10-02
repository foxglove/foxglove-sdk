use crate::channel_descriptor::FoxgloveChannelDescriptor;

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context. The callback should return false to disable logging of this channel.
///
/// This method is invoked from the client's main poll loop and must not block.
#[derive(Clone)]
pub(crate) struct ChannelFilter {
    callback_context: *const std::ffi::c_void,
    callback:
        unsafe extern "C" fn(*const std::ffi::c_void, *const FoxgloveChannelDescriptor) -> bool,
}

impl ChannelFilter {
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

unsafe impl Send for ChannelFilter {}
unsafe impl Sync for ChannelFilter {}
impl foxglove::SinkChannelFilter for ChannelFilter {
    /// Indicate whether the channel should be subscribed to.
    ///
    /// # Safety
    /// The channel descriptor is valid only as long as the callback.
    fn should_subscribe(&self, channel: &foxglove::ChannelDescriptor) -> bool {
        let c_channel_descriptor = FoxgloveChannelDescriptor(channel.clone());
        unsafe { (self.callback)(self.callback_context, &raw const c_channel_descriptor) }
    }
}
