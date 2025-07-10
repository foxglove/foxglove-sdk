use crate::{
    FoxgloveChannelDescriptor, FoxgloveChannelMetadata, FoxgloveKeyValue, FoxgloveSchema,
    FoxgloveString,
};

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
        let metadata_items_ptr = if !channel.metadata().is_empty() {
            let metadata_items: Vec<FoxgloveKeyValue> = channel
                .metadata()
                .iter()
                .map(|(key, value)| FoxgloveKeyValue {
                    key: FoxgloveString::from(key),
                    value: FoxgloveString::from(value),
                })
                .collect();
            // Safety: we will call from_raw after the callback returns
            Some(Box::into_raw(Box::new(metadata_items)))
        } else {
            None
        };

        let schema_name = channel.schema().map(|s| s.name.clone()).unwrap_or_default();
        let schema_encoding = channel
            .schema()
            .map(|s| s.encoding.clone())
            .unwrap_or_default();

        let c_channel = if let Some(metadata_items_ptr) = metadata_items_ptr {
            let metadata = Box::new(FoxgloveChannelMetadata {
                items: unsafe { (*metadata_items_ptr).as_ptr() },
                count: unsafe { (*metadata_items_ptr).len() },
            });

            FoxgloveChannelDescriptor {
                topic: FoxgloveString::from(channel.topic()),
                encoding: FoxgloveString::from(channel.message_encoding()),
                schema_name: FoxgloveString::from(&schema_name),
                schema_encoding: FoxgloveString::from(&schema_encoding),
                // Safety: we will call from_raw after the callback returns
                metadata: Box::into_raw(metadata),
            }
        } else {
            FoxgloveChannelDescriptor {
                topic: FoxgloveString::from(channel.topic()),
                encoding: FoxgloveString::from(channel.message_encoding()),
                metadata: std::ptr::null(),
                schema_name: FoxgloveString::from(&schema_name),
                schema_encoding: FoxgloveString::from(&schema_encoding),
            }
        };

        let result = unsafe { (self.callback)(self.callback_context, &raw const c_channel) };

        if !c_channel.metadata.is_null() {
            unsafe {
                // Safety: we called into_raw above
                drop(Box::from_raw(
                    c_channel.metadata as *mut FoxgloveChannelMetadata,
                ));
            }
        }
        if let Some(metadata_items_ptr) = metadata_items_ptr {
            unsafe {
                // Safety: we called into_raw above
                drop(Box::from_raw(metadata_items_ptr));
            }
        }

        result
    }
}
