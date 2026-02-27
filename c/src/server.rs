use crate::channel_descriptor::FoxgloveChannelDescriptor;
use crate::connection_graph::FoxgloveConnectionGraph;
use crate::fetch_asset::{FetchAssetHandler, FoxgloveFetchAssetResponder};
use crate::service::FoxgloveService;
use crate::sink_channel_filter::ChannelFilter;
use bitflags::bitflags;
use std::collections::HashMap;
use std::ffi::{CString, c_char, c_void};
use std::mem::ManuallyDrop;
use std::sync::Arc;

use crate::parameter::FoxgloveParameterArray;
use crate::playback_control_request::FoxglovePlaybackControlRequest;
use crate::playback_state::FoxglovePlaybackState;

use crate::{
    FoxgloveContext, FoxgloveError, FoxgloveKeyValue, FoxgloveSinkId, FoxgloveString, result_to_c,
};

// Easier to get reasonable C output from cbindgen with constants rather than directly exporting the bitflags macro
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct FoxgloveServerCapability {
    pub flags: u8,
}
/// Allow clients to advertise channels to send data messages to the server.
pub const FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH: u8 = 1 << 0;
/// Allow clients to subscribe and make connection graph updates
pub const FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH: u8 = 1 << 1;
/// Allow clients to get & set parameters.
pub const FOXGLOVE_SERVER_CAPABILITY_PARAMETERS: u8 = 1 << 2;
/// Inform clients about the latest server time.
///
/// This allows accelerated, slowed, or stepped control over the progress of time. If the
/// server publishes time data, then timestamps of published messages must originate from the
/// same time source.
pub const FOXGLOVE_SERVER_CAPABILITY_TIME: u8 = 1 << 3;
/// Allow clients to call services.
pub const FOXGLOVE_SERVER_CAPABILITY_SERVICES: u8 = 1 << 4;
/// Allow clients to request assets. If you supply an asset handler to the server, this capability
/// will be advertised automatically.
pub const FOXGLOVE_SERVER_CAPABILITY_ASSETS: u8 = 1 << 5;
/// Indicates that the server is capable of responding to playback control requests from controls
/// in the Foxglove app. This requires the server to specify the `data_start_time` and
/// `data_end_time` fields in `foxglove_server_options`.
pub const FOXGLOVE_SERVER_CAPABILITY_PLAYBACK_CONTROL: u8 = 1 << 6;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FoxgloveServerCapabilityBitFlags: u8 {
        const ClientPublish = FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH;
        const ConnectionGraph = FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH;
        const Parameters = FOXGLOVE_SERVER_CAPABILITY_PARAMETERS;
        const Time = FOXGLOVE_SERVER_CAPABILITY_TIME;
        const Services = FOXGLOVE_SERVER_CAPABILITY_SERVICES;
        const Assets = FOXGLOVE_SERVER_CAPABILITY_ASSETS;
        const PlaybackControl = FOXGLOVE_SERVER_CAPABILITY_PLAYBACK_CONTROL;
    }
}

impl FoxgloveServerCapabilityBitFlags {
    fn iter_websocket_capabilities(self) -> impl Iterator<Item = foxglove::websocket::Capability> {
        self.iter_names().filter_map(|(_s, cap)| match cap {
            FoxgloveServerCapabilityBitFlags::ClientPublish => {
                Some(foxglove::websocket::Capability::ClientPublish)
            }
            FoxgloveServerCapabilityBitFlags::ConnectionGraph => {
                Some(foxglove::websocket::Capability::ConnectionGraph)
            }
            FoxgloveServerCapabilityBitFlags::Parameters => {
                Some(foxglove::websocket::Capability::Parameters)
            }
            FoxgloveServerCapabilityBitFlags::Time => Some(foxglove::websocket::Capability::Time),
            FoxgloveServerCapabilityBitFlags::Services => {
                Some(foxglove::websocket::Capability::Services)
            }
            FoxgloveServerCapabilityBitFlags::Assets => {
                Some(foxglove::websocket::Capability::Assets)
            }
            FoxgloveServerCapabilityBitFlags::PlaybackControl => {
                Some(foxglove::websocket::Capability::PlaybackControl)
            }
            _ => None,
        })
    }
}

impl From<FoxgloveServerCapability> for FoxgloveServerCapabilityBitFlags {
    fn from(bits: FoxgloveServerCapability) -> Self {
        Self::from_bits_retain(bits.flags)
    }
}

#[repr(C)]
pub struct FoxgloveServerOptions<'a> {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the server will be created with the default context.
    pub context: *const FoxgloveContext,
    pub name: FoxgloveString,
    pub host: FoxgloveString,
    pub port: u16,
    pub callbacks: Option<&'a FoxgloveServerCallbacks>,
    pub capabilities: FoxgloveServerCapability,
    pub supported_encodings: *const FoxgloveString,
    pub supported_encodings_count: usize,

    /// Optional information about the server, which is shared with clients.
    ///
    /// # Safety
    /// - If provided, the `server_info` must be a valid pointer to an array of valid
    ///   `FoxgloveKeyValue`s with `server_info_count` elements.
    pub server_info: *const FoxgloveKeyValue,
    pub server_info_count: usize,

    /// Context provided to the `fetch_asset` callback.
    pub fetch_asset_context: *const c_void,

    /// Fetch an asset with the given URI and return it via the responder.
    ///
    /// This method is invoked from the client's main poll loop and must not block. If blocking or
    /// long-running behavior is required, the implementation should return immediately and handle
    /// the request asynchronously.
    ///
    /// The `uri` provided to the callback is only valid for the duration of the callback. If the
    /// implementation wishes to retain its data for a longer lifetime, it must copy data out of
    /// it.
    ///
    /// The `responder` provided to the callback represents an unfulfilled response. The
    /// implementation must eventually call either `foxglove_fetch_asset_respond_ok` or
    /// `foxglove_fetch_asset_respond_error`, exactly once, in order to complete the request. It is
    /// safe to invoke these completion functions synchronously from the context of the callback.
    ///
    /// # Safety
    /// - If provided, the handler callback must be a pointer to the fetch asset callback function,
    ///   and must remain valid until the server is stopped.
    pub fetch_asset: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            uri: *const FoxgloveString,
            responder: *mut FoxgloveFetchAssetResponder,
        ),
    >,

    /// TLS configuration: PEM-formatted x509 certificate for the server.
    pub tls_cert: *const u8,
    /// TLS configuration: Length of cert bytes
    pub tls_cert_len: usize,
    /// TLS configuration: PEM-formatted pkcs8 private key for the server.
    pub tls_key: *const u8,
    /// TLS configuration: Length of key bytes
    pub tls_key_len: usize,

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

    /// If the server is sending data from a fixed time range, and has the PlaybackControl capability,
    /// the start time of the data range.
    pub playback_start_time: Option<&'a u64>,
    /// If the server is sending data from a fixed time range, and has the PlaybackControl capability,
    /// the end time of the data range.
    pub playback_end_time: Option<&'a u64>,

    /// Optional session ID for the server.
    ///
    /// This allows the client to understand if the connection is a re-connection or if it is
    /// connecting to a new server instance. This can for example be a timestamp or a UUID.
    ///
    /// By default, the server will generate a session ID based on the current time.
    ///
    /// # Safety
    /// - If provided, the `session_id` must be a valid pointer to a null-terminated UTF-8 string.
    pub session_id: Option<&'a FoxgloveString>,
}

#[repr(C)]
pub struct FoxgloveClientChannel {
    pub id: u32,
    pub topic: *const c_char,
    pub encoding: *const c_char,
    pub schema_name: *const c_char,
    pub schema_encoding: *const c_char,
    pub schema: *const c_void,
    pub schema_len: usize,
}

#[repr(C)]
pub struct FoxgloveClientMetadata {
    pub id: u32,
    pub sink_id: FoxgloveSinkId,
}

#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveServerCallbacks {
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
    /// Callback invoked when a client requests parameters.
    ///
    /// Requires `FOXGLOVE_CAPABILITY_PARAMETERS`.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `param_names` argument is guaranteed to be non-NULL. These arguments point to buffers
    /// that are valid and immutable for the duration of the call. If the callback wishes to store
    /// these values, they must be copied out.
    ///
    /// This function should return the named parameters, or all parameters if `param_names` is
    /// empty. The return value must be allocated with `foxglove_parameter_array_create`. Ownership
    /// of this value is transferred to the callee, who is responsible for freeing it. A NULL return
    /// value is treated as an empty array.
    pub on_get_parameters: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            request_id: *const FoxgloveString,
            param_names: *const FoxgloveString,
            param_names_len: usize,
        ) -> *mut FoxgloveParameterArray,
    >,
    /// Callback invoked when a client sets parameters.
    ///
    /// Requires `FOXGLOVE_CAPABILITY_PARAMETERS`.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `params` argument is guaranteed to be non-NULL. These arguments point to buffers that
    /// are valid and immutable for the duration of the call. If the callback wishes to store these
    /// values, they must be copied out.
    ///
    /// This function should return the updated parameters. The return value must be allocated with
    /// `foxglove_parameter_array_create`. Ownership of this value is transferred to the callee, who
    /// is responsible for freeing it. A NULL return value is treated as an empty array.
    ///
    /// All clients subscribed to updates for the returned parameters will be notified. Note that if a
    /// returned parameter is unset, it will not be published to clients.
    pub on_set_parameters: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            request_id: *const FoxgloveString,
            params: *const FoxgloveParameterArray,
        ) -> *mut FoxgloveParameterArray,
    >,
    /// Callback invoked when a client subscribes to the named parameters for the first time.
    ///
    /// Requires `FOXGLOVE_CAPABILITY_PARAMETERS`.
    ///
    /// The `param_names` argument is guaranteed to be non-NULL. This argument points to buffers
    /// that are valid and immutable for the duration of the call. If the callback wishes to store
    /// these values, they must be copied out.
    pub on_parameters_subscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            param_names: *const FoxgloveString,
            param_names_len: usize,
        ),
    >,
    /// Callback invoked when the last client unsubscribes from the named parameters.
    ///
    /// Requires `FOXGLOVE_CAPABILITY_PARAMETERS`.
    ///
    /// The `param_names` argument is guaranteed to be non-NULL. This argument points to buffers
    /// that are valid and immutable for the duration of the call. If the callback wishes to store
    /// these values, they must be copied out.
    pub on_parameters_unsubscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            param_names: *const FoxgloveString,
            param_names_len: usize,
        ),
    >,
    pub on_connection_graph_subscribe: Option<unsafe extern "C" fn(context: *const c_void)>,
    pub on_connection_graph_unsubscribe: Option<unsafe extern "C" fn(context: *const c_void)>,
    pub on_client_connect: Option<unsafe extern "C" fn(context: *const c_void)>,
    pub on_client_disconnect: Option<unsafe extern "C" fn(context: *const c_void)>,

    /// Callback invoked when a client sends a playback control request message.
    ///
    /// Requires `FOXGLOVE_CAPABILITY_PLAYBACK_CONTROL`.
    ///
    /// `playback_control_request` is an input parameter and guaranteed to be non-NULL.
    /// `playback_state` is a non-NULL output pointer to a struct that has already been allocated.
    /// The caller should fill its fields with the appropriate state of playback, in response to
    /// the input playback control request.
    pub on_playback_control_request: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            playback_control_request: *const FoxglovePlaybackControlRequest,
            playback_state: *mut FoxglovePlaybackState,
        ),
    >,
}
unsafe impl Send for FoxgloveServerCallbacks {}
unsafe impl Sync for FoxgloveServerCallbacks {}

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerHandle>);

impl FoxgloveWebSocketServer {
    fn as_ref(&self) -> Option<&foxglove::WebSocketServerHandle> {
        self.0.as_ref()
    }

    fn take(&mut self) -> Option<foxglove::WebSocketServerHandle> {
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
///
/// - If `name` is supplied in options, it must contain valid UTF8.
/// - If `host` is supplied in options, it must contain valid UTF8.
/// - If `supported_encodings` is supplied in options, all `supported_encodings` must contain valid
///   UTF8, and `supported_encodings` must have length equal to `supported_encodings_count`.
/// - If `server_info` is supplied in options, all `server_info` must contain valid UTF8, and
///   `server_info` must have length equal to `server_info_count`.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_server_start(
    options: &FoxgloveServerOptions,
    server: *mut *mut FoxgloveWebSocketServer,
) -> FoxgloveError {
    unsafe {
        let result = do_foxglove_server_start(options);
        result_to_c(result, server)
    }
}

unsafe fn do_foxglove_server_start(
    options: &FoxgloveServerOptions,
) -> Result<*mut FoxgloveWebSocketServer, foxglove::FoxgloveError> {
    let name = unsafe { options.name.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("name is invalid: {e}")))?;
    let host = unsafe { options.host.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("host is invalid: {e}")))?;

    let mut server = foxglove::WebSocketServer::new()
        .name(name)
        .capabilities(
            FoxgloveServerCapabilityBitFlags::from(options.capabilities)
                .iter_websocket_capabilities(),
        )
        .bind(host, options.port);
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
        server = server.listener(Arc::new(callbacks.clone()));
    }
    if let Some(fetch_asset) = options.fetch_asset {
        server = server.fetch_asset_handler(Box::new(FetchAssetHandler::new(
            options.fetch_asset_context,
            fetch_asset,
        )));
    }
    if let Some(sink_channel_filter) = options.sink_channel_filter {
        server = server.channel_filter(Arc::new(ChannelFilter::new(
            options.sink_channel_filter_context,
            sink_channel_filter,
        )));
    }
    if !options.context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(options.context) });
        server = server.context(&context);
    }
    if !options.tls_cert.is_null() || !options.tls_key.is_null() {
        if options.tls_cert.is_null() || options.tls_key.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "Invalid TLS configuration (null pointer)".to_string(),
            ));
        }
        if options.tls_cert_len == 0 || options.tls_key_len == 0 {
            return Err(foxglove::FoxgloveError::ValueError(
                "Invalid TLS configuration (empty certificate or key)".to_string(),
            ));
        }
        let cert = unsafe { std::slice::from_raw_parts(options.tls_cert, options.tls_cert_len) };
        let key = unsafe { std::slice::from_raw_parts(options.tls_key, options.tls_key_len) };
        let tls_identity = foxglove::websocket::TlsIdentity {
            cert: cert.to_vec(),
            key: key.to_vec(),
        };
        server = server.tls(tls_identity);
    }
    if options.server_info_count > 0 {
        if options.server_info.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "server_info is null".to_string(),
            ));
        }
        let opts_info = options.server_info;
        let mut server_info = HashMap::new();
        for i in 0..options.server_info_count {
            let kv = unsafe { &*opts_info.add(i) };
            if kv.key.data.is_null() || kv.value.data.is_null() {
                return Err(foxglove::FoxgloveError::ValueError(
                    "null key or value in server_info".to_string(),
                ));
            }
            let key = unsafe { kv.key.as_utf8_str() }?;
            let value = unsafe { kv.value.as_utf8_str() }?;
            server_info.insert(key.to_string(), value.to_string());
        }
        server = server.server_info(server_info);
    }

    if let (Some(&playback_start_time), Some(&playback_end_time)) =
        (options.playback_start_time, options.playback_end_time)
    {
        server = server.playback_time_range(playback_start_time, playback_end_time);
    }

    if let Some(session_id) = options.session_id {
        if session_id.data.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "session_id is null".to_string(),
            ));
        }
        let session_id_str = unsafe { session_id.as_utf8_str() }.map_err(|e| {
            foxglove::FoxgloveError::Utf8Error(format!("session_id is invalid: {e}"))
        })?;
        server = server.session_id(session_id_str.to_string());
    }

    let server = server.start_blocking()?;
    Ok(Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        server,
    )))))
}

/// Publishes the current server timestamp to all clients.
///
/// Requires the `FOXGLOVE_CAPABILITY_TIME` capability.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_broadcast_time(
    server: Option<&FoxgloveWebSocketServer>,
    timestamp_nanos: u64,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    server.broadcast_time(timestamp_nanos);
    FoxgloveError::Ok
}

/// Publishes the current playback state to all clients.
///
/// Requires the `FOXGLOVE_CAPABILITY_PLAYBACK_CONTROL` capability.
///
/// # Safety
/// - `playback_state` must be a valid pointer to a playback state that lives for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_broadcast_playback_state(
    server: Option<&FoxgloveWebSocketServer>,
    playback_state: Option<&FoxglovePlaybackState>,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };

    let Some(playback_state) = playback_state else {
        return FoxgloveError::ValueError;
    };

    match unsafe { playback_state.to_native() } {
        Ok(playback_state) => {
            server.broadcast_playback_state(playback_state);
            FoxgloveError::Ok
        }
        Err(e) => FoxgloveError::from(e),
    }
}

/// Sets a new session ID and notifies all clients, causing them to reset their state.
///
/// If `session_id` is not provided, generates a new one based on the current timestamp.
///
/// # Safety
/// - `session_id` must either be NULL, or a valid pointer to a UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_clear_session(
    server: Option<&FoxgloveWebSocketServer>,
    session_id: Option<&FoxgloveString>,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    let Ok(session_id) = session_id
        .map(|id| unsafe { id.as_utf8_str().map(|id| id.to_string()) })
        .transpose()
    else {
        return FoxgloveError::Utf8Error;
    };
    server.clear_session(session_id);
    FoxgloveError::Ok
}

/// Adds a service to the server.
///
/// # Safety
/// - `server` must be a valid pointer to a server started with `foxglove_server_start`.
/// - `service` must be a valid pointer to a service allocated by `foxglove_service_create`. This
///   value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_add_service(
    server: Option<&FoxgloveWebSocketServer>,
    service: *mut FoxgloveService,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    if service.is_null() {
        return FoxgloveError::ValueError;
    }
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    let service = unsafe { FoxgloveService::from_raw(service) };
    server
        .add_services([service.into_inner()])
        .err()
        .map(FoxgloveError::from)
        .unwrap_or(FoxgloveError::Ok)
}

/// Removes a service from the server.
///
/// # Safety
/// - `server` must be a valid pointer to a server started with `foxglove_server_start`.
/// - `service_name` must be a valid pointer to a UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_remove_service(
    server: Option<&FoxgloveWebSocketServer>,
    service_name: FoxgloveString,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    let service_name = unsafe { service_name.as_utf8_str() };
    let Ok(service_name) = service_name else {
        return FoxgloveError::Utf8Error;
    };
    server.remove_services([service_name]);
    FoxgloveError::Ok
}

/// Get the port on which the server is listening.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_get_port(server: Option<&mut FoxgloveWebSocketServer>) -> u16 {
    let Some(server) = server else {
        tracing::error!("foxglove_server_get_port called with null server");
        return 0;
    };
    let Some(server) = server.as_ref() else {
        tracing::error!("foxglove_server_get_port called with closed server");
        return 0;
    };
    server.port()
}

/// Get the number of currently connected clients.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_get_client_count(
    server: Option<&mut FoxgloveWebSocketServer>,
) -> usize {
    let Some(server) = server else {
        tracing::error!("foxglove_server_get_client_count called with null server");
        return 0;
    };
    let Some(server) = server.as_ref() else {
        tracing::error!("foxglove_server_get_client_count called with closed server");
        return 0;
    };
    server.client_count()
}

/// Stop and shut down `server` and free the resources associated with it.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_stop(
    server: Option<&mut FoxgloveWebSocketServer>,
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
    server.stop().wait_blocking();
    FoxgloveError::Ok
}

/// Publish parameter values to all subscribed clients.
///
/// # Safety
/// - `params` must be a valid parameter to a value allocated by `foxglove_parameter_array_create`.
///   This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_publish_parameter_values(
    server: Option<&mut FoxgloveWebSocketServer>,
    params: *mut FoxgloveParameterArray,
) -> FoxgloveError {
    if params.is_null() {
        tracing::error!("foxglove_server_publish_parameter_values called with null params");
        return FoxgloveError::ValueError;
    };
    let params = unsafe { FoxgloveParameterArray::from_raw(params) };
    let Some(server) = server else {
        tracing::error!("foxglove_server_publish_parameter_values called with null server");
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        tracing::error!("foxglove_server_publish_connection_graph called with closed server");
        return FoxgloveError::SinkClosed;
    };
    server.publish_parameter_values(params.into_native());
    FoxgloveError::Ok
}

/// Publish a connection graph to the server.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_publish_connection_graph(
    server: Option<&mut FoxgloveWebSocketServer>,
    graph: Option<&mut FoxgloveConnectionGraph>,
) -> FoxgloveError {
    let Some(server) = server else {
        tracing::error!("foxglove_server_publish_connection_graph called with null server");
        return FoxgloveError::ValueError;
    };
    let Some(graph) = graph else {
        tracing::error!("foxglove_server_publish_connection_graph called with null graph");
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        tracing::error!("foxglove_server_publish_connection_graph called with closed server");
        return FoxgloveError::SinkClosed;
    };
    match server.publish_connection_graph(graph.0.clone()) {
        Ok(_) => FoxgloveError::Ok,
        Err(e) => FoxgloveError::from(e),
    }
}

/// Level indicator for a server status message.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FoxgloveServerStatusLevel {
    Info,
    Warning,
    Error,
}
impl From<FoxgloveServerStatusLevel> for foxglove::websocket::StatusLevel {
    fn from(value: FoxgloveServerStatusLevel) -> Self {
        match value {
            FoxgloveServerStatusLevel::Info => Self::Info,
            FoxgloveServerStatusLevel::Warning => Self::Warning,
            FoxgloveServerStatusLevel::Error => Self::Error,
        }
    }
}

/// Publishes a status message to all clients.
///
/// The server may send this message at any time. Client developers may use it for debugging
/// purposes, display it to the end user, or ignore it.
///
/// The caller may optionally provide a message ID, which can be used in a subsequent call to
/// `foxglove_server_remove_status`.
///
/// # Safety
/// - `message` must be a valid pointer to a UTF-8 string, which must remain valid for the duration
///   of this call.
/// - `id` must either be NULL, or a valid pointer to a UTF-8 string, which must remain valid for
///   the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_publish_status(
    server: Option<&mut FoxgloveWebSocketServer>,
    level: FoxgloveServerStatusLevel,
    message: FoxgloveString,
    id: Option<&FoxgloveString>,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    let message = unsafe { message.as_utf8_str() };
    let Ok(message) = message else {
        return FoxgloveError::Utf8Error;
    };
    let id = id.map(|id| unsafe { id.as_utf8_str() }).transpose();
    let Ok(id) = id else {
        return FoxgloveError::Utf8Error;
    };
    let mut status = foxglove::websocket::Status::new(level.into(), message);
    if let Some(id) = id {
        status = status.with_id(id);
    }
    server.publish_status(status);
    FoxgloveError::Ok
}

/// Removes status messages from all clients.
///
/// Previously published status messages are referenced by ID.
///
/// # Safety
/// - `ids` must be a valid pointer to an array of pointers to valid UTF-8 strings, all of which
///   must remain valid for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_server_remove_status(
    server: Option<&mut FoxgloveWebSocketServer>,
    ids: *const FoxgloveString,
    ids_count: usize,
) -> FoxgloveError {
    let Some(server) = server else {
        return FoxgloveError::ValueError;
    };
    let Some(server) = server.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    if ids.is_null() {
        return FoxgloveError::ValueError;
    }
    let ids = unsafe { std::slice::from_raw_parts(ids, ids_count) }
        .iter()
        .map(|id| unsafe { id.as_utf8_str().map(|id| id.to_string()) })
        .collect::<Result<Vec<_>, _>>();
    let Ok(ids) = ids else {
        return FoxgloveError::Utf8Error;
    };
    server.remove_status(ids);
    FoxgloveError::Ok
}

impl foxglove::websocket::ServerListener for FoxgloveServerCallbacks {
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

    fn on_get_parameters(
        &self,
        client: foxglove::websocket::Client,
        param_names: Vec<String>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::websocket::Parameter> {
        let Some(on_get_parameters) = self.on_get_parameters else {
            return vec![];
        };

        let c_request_id = request_id.map(FoxgloveString::from);
        let c_param_names: Vec<_> = param_names.iter().map(FoxgloveString::from).collect();
        let raw = unsafe {
            on_get_parameters(
                self.context,
                client.id().into(),
                c_request_id
                    .as_ref()
                    .map(|id| id as *const _)
                    .unwrap_or_else(std::ptr::null),
                c_param_names.as_ptr(),
                c_param_names.len(),
            )
        };
        if raw.is_null() {
            vec![]
        } else {
            // SAFETY: The caller must return a valid pointer to an array allocated by
            // `foxglove_parameter_array_create`.
            unsafe { FoxgloveParameterArray::from_raw(raw).into_native() }
        }
    }

    fn on_set_parameters(
        &self,
        client: foxglove::websocket::Client,
        params: Vec<foxglove::websocket::Parameter>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::websocket::Parameter> {
        let Some(on_set_parameters) = self.on_set_parameters else {
            return vec![];
        };

        let c_request_id = request_id.map(FoxgloveString::from);
        let params: FoxgloveParameterArray = params.into_iter().collect();
        let c_params = params.into_raw();
        let raw = unsafe {
            on_set_parameters(
                self.context,
                client.id().into(),
                c_request_id
                    .as_ref()
                    .map(|id| id as *const _)
                    .unwrap_or_else(std::ptr::null),
                c_params,
            )
        };
        // SAFETY: This is the same pointer we just converted into raw.
        drop(unsafe { FoxgloveParameterArray::from_raw(c_params) });
        if raw.is_null() {
            vec![]
        } else {
            // SAFETY: The caller must return a valid pointer to an array allocated by
            // `foxglove_parameter_array_create`.
            unsafe { FoxgloveParameterArray::from_raw(raw).into_native() }
        }
    }

    fn on_parameters_subscribe(&self, param_names: Vec<String>) {
        let Some(on_parameters_subscribe) = self.on_parameters_subscribe else {
            return;
        };
        let c_param_names: Vec<_> = param_names.iter().map(FoxgloveString::from).collect();
        unsafe {
            on_parameters_subscribe(self.context, c_param_names.as_ptr(), c_param_names.len())
        };
    }

    fn on_parameters_unsubscribe(&self, param_names: Vec<String>) {
        let Some(on_parameters_unsubscribe) = self.on_parameters_unsubscribe else {
            return;
        };
        let c_param_names: Vec<_> = param_names.iter().map(FoxgloveString::from).collect();
        unsafe {
            on_parameters_unsubscribe(self.context, c_param_names.as_ptr(), c_param_names.len())
        };
    }

    fn on_connection_graph_subscribe(&self) {
        if let Some(on_connection_graph_subscribe) = self.on_connection_graph_subscribe {
            unsafe { on_connection_graph_subscribe(self.context) };
        }
    }

    fn on_connection_graph_unsubscribe(&self) {
        if let Some(on_connection_graph_unsubscribe) = self.on_connection_graph_unsubscribe {
            unsafe { on_connection_graph_unsubscribe(self.context) };
        }
    }

    fn on_client_connect(&self) {
        if let Some(on_client_connect) = self.on_client_connect {
            unsafe { on_client_connect(self.context) };
        }
    }

    fn on_client_disconnect(&self) {
        if let Some(on_client_disconnect) = self.on_client_disconnect {
            unsafe { on_client_disconnect(self.context) };
        }
    }

    fn on_playback_control_request(
        &self,
        playback_control_request: foxglove::websocket::PlaybackControlRequest,
    ) -> Option<foxglove::websocket::PlaybackState> {
        let on_playback_control_request = self.on_playback_control_request?;

        let c_playback_control_request = FoxglovePlaybackControlRequest {
            playback_command: playback_control_request.playback_command as u8,
            playback_speed: playback_control_request.playback_speed,
            seek_time: playback_control_request.seek_time.as_ref(),
            request_id: (&playback_control_request.request_id).into(),
        };

        let mut c_playback_state = FoxglovePlaybackState {
            status: 0,
            current_time: 0,
            playback_speed: 0.0,
            did_seek: false,
            request_id: (&playback_control_request.request_id).into(),
        };

        unsafe {
            on_playback_control_request(
                self.context,
                &raw const c_playback_control_request,
                &raw mut c_playback_state,
            );
        };

        // SAFETY: playback_control_request stays valid for the duration of this call
        unsafe { c_playback_state.to_native().ok() }
    }
}
