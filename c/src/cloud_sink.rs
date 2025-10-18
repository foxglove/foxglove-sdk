use std::ffi::{c_void, CString};
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::server::{FoxgloveClientChannel, FoxgloveClientMetadata};
use crate::{result_to_c, FoxgloveContext, FoxgloveError, FoxgloveString};

#[repr(C)]
pub struct FoxgloveCloudSinkOptions<'a> {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the server will be created with the default context.
    pub context: *const FoxgloveContext,
    pub callbacks: Option<&'a FoxgloveCloudSinkCallbacks>,
    pub supported_encodings: *const FoxgloveString,
    pub supported_encodings_count: usize,
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

/// Create and start a server.
///
/// Resources must later be freed by calling `foxglove_server_stop`.
///
/// `port` may be 0, in which case an available port will be automatically selected.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// If `name` is supplied in options, it must contain valid UTF8.
/// If `host` is supplied in options, it must contain valid UTF8.
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
    let mut server = foxglove::CloudSink::new();
    if options.supported_encodings_count > 0 {
        if options.supported_encodings.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "supported_encodings is null".to_string(),
            ));
        }
        server = server.supported_encodings(
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
        server = server.listener(Arc::new(callbacks.clone()))
    }
    if !options.context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(options.context) });
        server = server.context(&context);
    }

    let server = server.start_blocking()?;
    Ok(Box::into_raw(Box::new(FoxgloveCloudSink(Some(server)))))
}

/// Stop and shut down `server` and free the resources associated with it.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_cloud_sink_stop(
    server: Option<&mut FoxgloveCloudSink>,
) -> FoxgloveError {
    let Some(server) = server else {
        tracing::error!("foxglove_server_stop called with null server");
        return FoxgloveError::ValueError;
    };

    // Safety: undo the Box::into_raw in foxglove_server_start, safe if this was created by that method
    let mut server = unsafe { Box::from_raw(server) };
    let Some(server) = server.take() else {
        tracing::error!("foxglove_server_stop called with closed server");
        return FoxgloveError::SinkClosed;
    };
    if let Some(waiter) = server.stop() {
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
