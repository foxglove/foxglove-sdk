use std::ffi::{c_void, CString};
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::channel_descriptor::FoxgloveChannelDescriptor;
use crate::server::{FoxgloveClientChannel, FoxgloveClientMetadata};
use crate::sink_channel_filter::ChannelFilter;
use crate::{result_to_c, FoxgloveContext, FoxgloveError, FoxgloveString};

#[repr(C)]
pub struct FoxgloveCloudSinkOptions<'a> {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the server will be created with the default context.
    pub context: *const FoxgloveContext,
    pub callbacks: Option<&'a FoxgloveCloudSinkCallbacks>,
    pub supported_encodings: *const FoxgloveString,
    pub supported_encodings_count: usize,

    /// Context provided to the `sink_channel_filter` callback.
    pub sink_channel_filter_context: *const c_void,

    /// A filter for channels that can be used to subscribe to or unsubscribe from channels.
    ///
    /// This can be used to omit one or more channels from a sink, but still log all channels to another
    /// sink in the same context. Return false to disable logging of this channel.
    ///
    /// This method is invoked from the client's main poll loop and must not block.
    ///
    /// # Safety
    /// - If provided, the handler callback must be a pointer to the filter callback function,
    ///   and must remain valid until the server is stopped.
    pub sink_channel_filter: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel: *const FoxgloveChannelDescriptor,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveCloudSinkCallbacks {
    /// A user-defined value that will be passed to callback functions
    pub context: *const c_void,
    pub on_subscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel_id: u64,
            client: FoxgloveClientMetadata,
        ),
    >,
    pub on_unsubscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel_id: u64,
            client: FoxgloveClientMetadata,
        ),
    >,
    pub on_client_advertise: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveClientChannel,
        ),
    >,
    pub on_message_data: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            client_channel_id: u32,
            payload: *const u8,
            payload_len: usize,
        ),
    >,
    pub on_client_unadvertise: Option<
        unsafe extern "C" fn(client_id: u32, client_channel_id: u32, context: *const c_void),
    >,
}
unsafe impl Send for FoxgloveCloudSinkCallbacks {}
unsafe impl Sync for FoxgloveCloudSinkCallbacks {}

pub struct FoxgloveCloudSink(Option<foxglove::CloudSinkHandle>);

impl FoxgloveCloudSink {
    fn take(&mut self) -> Option<foxglove::CloudSinkHandle> {
        self.0.take()
    }
}

/// Create and start a cloud sink.
///
/// Resources must later be freed by calling `foxglove_cloud_sink_stop`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// If `supported_encodings` is supplied in options, all `supported_encodings` must contain valid
/// UTF8, and `supported_encodings` must have length equal to `supported_encodings_count`.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_cloud_sink_start(
    options: &FoxgloveCloudSinkOptions,
    server: *mut *mut FoxgloveCloudSink,
) -> FoxgloveError {
    unsafe {
        let result = do_foxglove_cloud_sink_start(options);
        result_to_c(result, server)
    }
}

unsafe fn do_foxglove_cloud_sink_start(
    options: &FoxgloveCloudSinkOptions,
) -> Result<*mut FoxgloveCloudSink, foxglove::FoxgloveError> {
    let mut sink = foxglove::CloudSink::new();
    if options.supported_encodings_count > 0 {
        if options.supported_encodings.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "supported_encodings is null".to_string(),
            ));
        }
        sink = sink.supported_encodings(
            unsafe {
                std::slice::from_raw_parts(
                    options.supported_encodings,
                    options.supported_encodings_count,
                )
            }
            .iter()
            .map(|enc| {
                if enc.data.is_null() {
                    return Err(foxglove::FoxgloveError::ValueError(
                        "encoding in supported_encodings is null".to_string(),
                    ));
                }
                unsafe { enc.as_utf8_str() }.map_err(|e| {
                    foxglove::FoxgloveError::Utf8Error(format!(
                        "encoding in supported_encodings is invalid: {e}"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if let Some(callbacks) = options.callbacks {
        sink = sink.listener(Arc::new(callbacks.clone()))
    }
    if let Some(sink_channel_filter) = options.sink_channel_filter {
        sink = sink.channel_filter(Arc::new(ChannelFilter::new(
            options.sink_channel_filter_context,
            sink_channel_filter,
        )));
    }
    if !options.context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(options.context) });
        sink = sink.context(&context);
    }

    let server = sink.start_blocking()?;
    Ok(Box::into_raw(Box::new(FoxgloveCloudSink(Some(server)))))
}

/// Stop and shut down cloud `sink` and free the resources associated with it.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_cloud_sink_stop(sink: Option<&mut FoxgloveCloudSink>) -> FoxgloveError {
    let Some(sink) = sink else {
        tracing::error!("foxglove_sink_stop called with null sink");
        return FoxgloveError::ValueError;
    };

    // Safety: undo the Box::into_raw in foxglove_cloud_sink_start, safe if this was created by that method
    let mut sink = unsafe { Box::from_raw(sink) };
    let Some(sink) = sink.take() else {
        tracing::error!("foxglove_sink_stop called with closed sink");
        return FoxgloveError::SinkClosed;
    };
    if let Some(waiter) = sink.stop() {
        waiter.wait_blocking();
    }
    FoxgloveError::Ok
}

impl foxglove::CloudSinkListener for FoxgloveCloudSinkCallbacks {
    fn on_subscribe(
        &self,
        client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        if let Some(on_subscribe) = self.on_subscribe {
            let c_client_metadata = FoxgloveClientMetadata {
                id: client.id().into(),
                sink_id: client.sink_id().map(|id| id.into()).unwrap_or(0),
            };
            unsafe { on_subscribe(self.context, channel.id().into(), c_client_metadata) };
        }
    }

    fn on_unsubscribe(
        &self,
        client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        if let Some(on_unsubscribe) = self.on_unsubscribe {
            let c_client_metadata = FoxgloveClientMetadata {
                id: client.id().into(),
                sink_id: client.sink_id().map(|id| id.into()).unwrap_or(0),
            };
            unsafe { on_unsubscribe(self.context, channel.id().into(), c_client_metadata) };
        }
    }

    fn on_client_advertise(
        &self,
        client: foxglove::websocket::Client,
        channel: &foxglove::websocket::ClientChannel,
    ) {
        let Some(on_client_advertise) = self.on_client_advertise else {
            return;
        };
        let topic = CString::new(channel.topic.clone()).unwrap();
        let encoding = CString::new(channel.encoding.clone()).unwrap();
        let schema_name = CString::new(channel.schema_name.clone()).unwrap();
        let schema_encoding = channel
            .schema_encoding
            .as_ref()
            .map(|enc| CString::new(enc.clone()).unwrap());
        let c_channel = FoxgloveClientChannel {
            id: channel.id.into(),
            topic: topic.as_ptr(),
            encoding: encoding.as_ptr(),
            schema_name: schema_name.as_ptr(),
            schema_encoding: schema_encoding
                .as_ref()
                .map(|enc| enc.as_ptr())
                .unwrap_or(std::ptr::null()),
            schema: channel
                .schema
                .as_ref()
                .map(|schema| schema.as_ptr() as *const c_void)
                .unwrap_or(std::ptr::null()),
            schema_len: channel
                .schema
                .as_ref()
                .map(|schema| schema.len())
                .unwrap_or(0),
        };
        unsafe { on_client_advertise(self.context, client.id().into(), &raw const c_channel) };
    }

    fn on_message_data(
        &self,
        client: foxglove::websocket::Client,
        channel: &foxglove::websocket::ClientChannel,
        payload: &[u8],
    ) {
        if let Some(on_message_data) = self.on_message_data {
            unsafe {
                on_message_data(
                    self.context,
                    client.id().into(),
                    channel.id.into(),
                    payload.as_ptr(),
                    payload.len(),
                )
            };
        }
    }

    fn on_client_unadvertise(
        &self,
        client: foxglove::websocket::Client,
        channel: &foxglove::websocket::ClientChannel,
    ) {
        if let Some(on_client_unadvertise) = self.on_client_unadvertise {
            unsafe { on_client_unadvertise(client.id().into(), channel.id.into(), self.context) };
        }
    }
}
