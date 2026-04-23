use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::time::Duration;

use bitflags::bitflags;

use crate::channel_descriptor::FoxgloveChannelDescriptor;
use crate::connection_graph::FoxgloveConnectionGraph;
use crate::fetch_asset::{FetchAssetHandler, FoxgloveFetchAssetResponder};
use crate::parameter::FoxgloveParameterArray;
use crate::server::FoxgloveServerStatusLevel;
use crate::service::FoxgloveService;
use crate::sink_channel_filter::ChannelFilter;
use crate::util::parse_key_value_array;
use crate::{
    FoxgloveContext, FoxgloveError, FoxgloveKeyValue, FoxgloveSinkId, FoxgloveString, result_to_c,
};

/// The reliability policy for a channel's data delivery.
#[repr(u8)]
pub enum FoxgloveReliability {
    /// Data is sent over unreliable data tracks. This is the default.
    Lossy = 0,
    /// Data is sent over the reliable control channel (ordered, guaranteed delivery).
    Reliable = 1,
}

/// Quality-of-service profile for a channel.
#[repr(C)]
pub struct FoxgloveQosProfile {
    pub reliability: FoxgloveReliability,
}

impl From<FoxgloveQosProfile> for foxglove::remote_access::QosProfile {
    fn from(profile: FoxgloveQosProfile) -> Self {
        let reliability = match profile.reliability {
            FoxgloveReliability::Lossy => foxglove::remote_access::Reliability::Lossy,
            FoxgloveReliability::Reliable => foxglove::remote_access::Reliability::Reliable,
        };
        foxglove::remote_access::QosProfile::builder()
            .reliability(reliability)
            .build()
    }
}

/// A QoS classifier that wraps a C callback.
#[derive(Clone)]
struct QosClassifier {
    callback_context: *const c_void,
    callback:
        unsafe extern "C" fn(*const c_void, *const FoxgloveChannelDescriptor) -> FoxgloveQosProfile,
}

impl QosClassifier {
    fn new(
        callback_context: *const c_void,
        callback: unsafe extern "C" fn(
            *const c_void,
            *const FoxgloveChannelDescriptor,
        ) -> FoxgloveQosProfile,
    ) -> Self {
        Self {
            callback_context,
            callback,
        }
    }
}

unsafe impl Send for QosClassifier {}
unsafe impl Sync for QosClassifier {}

impl foxglove::remote_access::QosClassifier for QosClassifier {
    fn classify(
        &self,
        channel: &foxglove::ChannelDescriptor,
    ) -> foxglove::remote_access::QosProfile {
        let c_channel_descriptor = FoxgloveChannelDescriptor(channel.clone());
        let profile =
            unsafe { (self.callback)(self.callback_context, &raw const c_channel_descriptor) };
        profile.into()
    }
}

/// The status of the remote access gateway connection.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoxgloveConnectionStatus {
    /// The gateway is attempting to establish or re-establish a connection.
    Connecting = 0,
    /// The gateway is connected and handling events.
    Connected = 1,
    /// The gateway is shutting down. Listener callbacks may still be in progress.
    ShuttingDown = 2,
    /// The gateway has been shut down. No further listener callbacks will be invoked.
    Shutdown = 3,
}

impl From<foxglove::remote_access::ConnectionStatus> for FoxgloveConnectionStatus {
    fn from(status: foxglove::remote_access::ConnectionStatus) -> Self {
        match status {
            foxglove::remote_access::ConnectionStatus::Connecting => Self::Connecting,
            foxglove::remote_access::ConnectionStatus::Connected => Self::Connected,
            foxglove::remote_access::ConnectionStatus::ShuttingDown => Self::ShuttingDown,
            foxglove::remote_access::ConnectionStatus::Shutdown => Self::Shutdown,
        }
    }
}

// Capabilities
// ============

/// Capabilities for the remote access gateway. These are advertised to clients.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct FoxgloveGatewayCapability {
    pub flags: u8,
}

/// Allow clients to advertise channels to send data messages to the server.
pub const FOXGLOVE_GATEWAY_CAPABILITY_CLIENT_PUBLISH: u8 = 1 << 0;
/// Allow clients to get, set, and subscribe to parameter updates.
pub const FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS: u8 = 1 << 1;
/// Allow clients to call services.
pub const FOXGLOVE_GATEWAY_CAPABILITY_SERVICES: u8 = 1 << 2;
/// Allow clients to subscribe and make connection graph updates.
pub const FOXGLOVE_GATEWAY_CAPABILITY_CONNECTION_GRAPH: u8 = 1 << 3;
/// Allow clients to request assets.
pub const FOXGLOVE_GATEWAY_CAPABILITY_ASSETS: u8 = 1 << 4;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FoxgloveGatewayCapabilityBitFlags: u8 {
        const ClientPublish = FOXGLOVE_GATEWAY_CAPABILITY_CLIENT_PUBLISH;
        const Parameters = FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS;
        const Services = FOXGLOVE_GATEWAY_CAPABILITY_SERVICES;
        const ConnectionGraph = FOXGLOVE_GATEWAY_CAPABILITY_CONNECTION_GRAPH;
        const Assets = FOXGLOVE_GATEWAY_CAPABILITY_ASSETS;
    }
}

impl FoxgloveGatewayCapabilityBitFlags {
    fn iter_gateway_capabilities(
        self,
    ) -> impl Iterator<Item = foxglove::remote_access::Capability> {
        self.iter_names().filter_map(|(_s, cap)| match cap {
            FoxgloveGatewayCapabilityBitFlags::ClientPublish => {
                Some(foxglove::remote_access::Capability::ClientPublish)
            }
            FoxgloveGatewayCapabilityBitFlags::Parameters => {
                Some(foxglove::remote_access::Capability::Parameters)
            }
            FoxgloveGatewayCapabilityBitFlags::Services => {
                Some(foxglove::remote_access::Capability::Services)
            }
            FoxgloveGatewayCapabilityBitFlags::ConnectionGraph => {
                Some(foxglove::remote_access::Capability::ConnectionGraph)
            }
            FoxgloveGatewayCapabilityBitFlags::Assets => {
                Some(foxglove::remote_access::Capability::Assets)
            }
            _ => None,
        })
    }
}

impl From<FoxgloveGatewayCapability> for FoxgloveGatewayCapabilityBitFlags {
    fn from(bits: FoxgloveGatewayCapability) -> Self {
        Self::from_bits_retain(bits.flags)
    }
}

// Callbacks
// =========

/// Callbacks for the remote access gateway.
///
/// These methods are invoked from time-sensitive contexts and must not block.
#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveGatewayCallbacks {
    /// A user-defined value that will be passed to callback functions.
    pub context: *const c_void,

    /// Callback invoked when the gateway connection status changes.
    pub on_connection_status_changed:
        Option<unsafe extern "C" fn(context: *const c_void, status: FoxgloveConnectionStatus)>,

    /// Callback invoked when a client subscribes to a channel.
    pub on_subscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveChannelDescriptor,
        ),
    >,

    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    /// Also invoked when a subscribed channel is removed from the context.
    pub on_unsubscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveChannelDescriptor,
        ),
    >,

    /// Callback invoked when a client message is received.
    pub on_message_data: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveChannelDescriptor,
            payload: *const u8,
            payload_len: usize,
        ),
    >,

    /// Callback invoked when a client advertises a client channel.
    pub on_client_advertise: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveChannelDescriptor,
        ),
    >,

    /// Callback invoked when a client unadvertises a client channel.
    pub on_client_unadvertise: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            channel: *const FoxgloveChannelDescriptor,
        ),
    >,

    /// Callback invoked when a client requests parameters.
    ///
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `param_names` argument is guaranteed to be non-NULL. These arguments point to buffers
    /// that are valid and immutable for the duration of the call. If the callback wishes to store
    /// these values, they must be copied out.
    ///
    /// This function should return the named parameters, or all parameters if `param_names` is
    /// empty. The return value must be allocated with `foxglove_parameter_array_create`. Ownership
    /// of this value is transferred to the callee. A NULL return value is treated as empty.
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
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `params` argument is guaranteed to be non-NULL. These arguments point to buffers that
    /// are valid and immutable for the duration of the call. If the callback wishes to store these
    /// values, they must be copied out.
    ///
    /// This function should return the updated parameters. The return value must be allocated with
    /// `foxglove_parameter_array_create`. Ownership is transferred to the callee. A NULL return
    /// value is treated as empty.
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
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`.
    pub on_parameters_subscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            param_names: *const FoxgloveString,
            param_names_len: usize,
        ),
    >,

    /// Callback invoked when the last client unsubscribes from the named parameters.
    ///
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`.
    pub on_parameters_unsubscribe: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            param_names: *const FoxgloveString,
            param_names_len: usize,
        ),
    >,

    /// Callback invoked when the first client subscribes to connection graph updates.
    ///
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_CONNECTION_GRAPH`.
    pub on_connection_graph_subscribe: Option<unsafe extern "C" fn(context: *const c_void)>,

    /// Callback invoked when the last client unsubscribes from connection graph updates.
    ///
    /// Requires `FOXGLOVE_GATEWAY_CAPABILITY_CONNECTION_GRAPH`.
    pub on_connection_graph_unsubscribe: Option<unsafe extern "C" fn(context: *const c_void)>,
}

// SAFETY: The `context` pointer and callback function pointers are provided by the C caller,
// who is responsible for ensuring they are safe to invoke from any thread. This is documented
// on the `FoxgloveGatewayCallbacks` struct and `FoxgloveGatewayOptions`.
unsafe impl Send for FoxgloveGatewayCallbacks {}
unsafe impl Sync for FoxgloveGatewayCallbacks {}

impl foxglove::remote_access::Listener for FoxgloveGatewayCallbacks {
    fn on_connection_status_changed(&self, status: foxglove::remote_access::ConnectionStatus) {
        if let Some(cb) = self.on_connection_status_changed {
            unsafe { cb(self.context, FoxgloveConnectionStatus::from(status)) };
        }
    }

    fn on_subscribe(
        &self,
        client: &foxglove::remote_access::Client,
        channel: &foxglove::ChannelDescriptor,
    ) {
        if let Some(cb) = self.on_subscribe {
            let c_channel = FoxgloveChannelDescriptor(channel.clone());
            unsafe { cb(self.context, client.id().into(), &raw const c_channel) };
        }
    }

    fn on_unsubscribe(
        &self,
        client: &foxglove::remote_access::Client,
        channel: &foxglove::ChannelDescriptor,
    ) {
        if let Some(cb) = self.on_unsubscribe {
            let c_channel = FoxgloveChannelDescriptor(channel.clone());
            unsafe { cb(self.context, client.id().into(), &raw const c_channel) };
        }
    }

    fn on_message_data(
        &self,
        client: &foxglove::remote_access::Client,
        channel: &foxglove::ChannelDescriptor,
        payload: &[u8],
    ) {
        if let Some(cb) = self.on_message_data {
            let c_channel = FoxgloveChannelDescriptor(channel.clone());
            unsafe {
                cb(
                    self.context,
                    client.id().into(),
                    &raw const c_channel,
                    payload.as_ptr(),
                    payload.len(),
                )
            };
        }
    }

    fn on_client_advertise(
        &self,
        client: &foxglove::remote_access::Client,
        channel: &foxglove::ChannelDescriptor,
    ) {
        if let Some(cb) = self.on_client_advertise {
            let c_channel = FoxgloveChannelDescriptor(channel.clone());
            unsafe { cb(self.context, client.id().into(), &raw const c_channel) };
        }
    }

    fn on_client_unadvertise(
        &self,
        client: &foxglove::remote_access::Client,
        channel: &foxglove::ChannelDescriptor,
    ) {
        if let Some(cb) = self.on_client_unadvertise {
            let c_channel = FoxgloveChannelDescriptor(channel.clone());
            unsafe { cb(self.context, client.id().into(), &raw const c_channel) };
        }
    }

    fn on_get_parameters(
        &self,
        client: &foxglove::remote_access::Client,
        param_names: Vec<String>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::remote_access::Parameter> {
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
        client: &foxglove::remote_access::Client,
        parameters: Vec<foxglove::remote_access::Parameter>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::remote_access::Parameter> {
        let Some(on_set_parameters) = self.on_set_parameters else {
            return vec![];
        };
        let c_request_id = request_id.map(FoxgloveString::from);
        let params: FoxgloveParameterArray = parameters.into_iter().collect();
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
}

// Options
// =======

/// Options for creating a remote access gateway.
///
/// # Safety
/// - `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
/// - `name` must be a valid UTF-8 string.
/// - `device_token` must be a valid UTF-8 string, or empty to use the
///   `FOXGLOVE_DEVICE_TOKEN` environment variable.
/// - If `supported_encodings` is supplied, all entries must contain valid UTF-8, and
///   `supported_encodings` must have length equal to `supported_encodings_count`.
/// - If `server_info` is supplied, all entries must contain valid UTF-8, and `server_info` must
///   have length equal to `server_info_count`.
#[repr(C)]
pub struct FoxgloveGatewayOptions<'a> {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the gateway will be created with the default context.
    pub context: *const FoxgloveContext,
    pub name: FoxgloveString,
    pub device_token: FoxgloveString,
    pub callbacks: Option<&'a FoxgloveGatewayCallbacks>,
    pub capabilities: FoxgloveGatewayCapability,
    pub supported_encodings: *const FoxgloveString,
    pub supported_encodings_count: usize,

    /// Optional information about the gateway, which is shared with clients via the ServerInfo
    /// message.
    ///
    /// # Safety
    /// - If provided, the `server_info` must be a valid pointer to an array of valid
    ///   `FoxgloveKeyValue`s with `server_info_count` elements.
    pub server_info: *const FoxgloveKeyValue,
    pub server_info_count: usize,

    /// Context provided to the `sink_channel_filter` callback.
    pub sink_channel_filter_context: *const c_void,

    /// A filter for channels.
    ///
    /// Return false to disable logging of this channel.
    /// This method is invoked from the client's main poll loop and must not block.
    pub sink_channel_filter: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel: *const FoxgloveChannelDescriptor,
        ) -> bool,
    >,

    /// Context provided to the `qos_classifier` callback.
    pub qos_classifier_context: *const c_void,

    /// A QoS classifier for channels.
    ///
    /// Returns a [`FoxgloveQosProfile`] for the given channel, determining how data is delivered.
    /// If not set, all channels use the default lossy profile.
    pub qos_classifier: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel: *const FoxgloveChannelDescriptor,
        ) -> FoxgloveQosProfile,
    >,

    /// Context provided to the `fetch_asset` callback.
    pub fetch_asset_context: *const c_void,

    /// Fetch an asset with the given URI and return it via the responder.
    ///
    /// This method is invoked from a time-sensitive context and must not block. If blocking or
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
    /// If provided, the Assets capability will be advertised automatically.
    ///
    /// # Safety
    /// - If provided, the handler callback must be a pointer to the fetch asset callback function,
    ///   and must remain valid until the gateway is stopped.
    pub fetch_asset: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            uri: *const FoxgloveString,
            responder: *mut FoxgloveFetchAssetResponder,
        ),
    >,

    /// Optional Foxglove API base URL override. Empty string uses the default.
    pub foxglove_api_url: FoxgloveString,

    /// Optional Foxglove API timeout in seconds.
    pub foxglove_api_timeout_secs: Option<&'a u64>,

    /// Optional message backlog size override.
    pub message_backlog_size: Option<&'a usize>,
}

// Handle
// ======

pub struct FoxgloveGateway(Option<foxglove::remote_access::GatewayHandle>);

impl FoxgloveGateway {
    fn as_ref(&self) -> Option<&foxglove::remote_access::GatewayHandle> {
        self.0.as_ref()
    }

    fn take(&mut self) -> Option<foxglove::remote_access::GatewayHandle> {
        self.0.take()
    }
}

// FFI functions
// =============

/// Start a remote access gateway with the given options.
///
/// On success, the `gateway` output parameter will be set to a valid pointer.
/// On failure, an error code is returned.
///
/// # Safety
/// - `options` must be a valid pointer to a `FoxgloveGatewayOptions` struct with all fields
///   satisfying the documented safety requirements.
/// - If `server_info` is supplied in options, all `server_info` must contain valid UTF8, and
///   `server_info` must have length equal to `server_info_count`.
/// - `gateway` must be a valid pointer to a `*mut FoxgloveGateway`.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_gateway_start(
    options: &FoxgloveGatewayOptions,
    gateway: *mut *mut FoxgloveGateway,
) -> FoxgloveError {
    unsafe {
        let result = do_foxglove_gateway_start(options);
        result_to_c(result, gateway)
    }
}

unsafe fn do_foxglove_gateway_start(
    options: &FoxgloveGatewayOptions,
) -> Result<*mut FoxgloveGateway, foxglove::FoxgloveError> {
    let name = unsafe { options.name.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("name is invalid: {e}")))?;

    let mut gateway = foxglove::remote_access::Gateway::new().capabilities(
        FoxgloveGatewayCapabilityBitFlags::from(options.capabilities).iter_gateway_capabilities(),
    );

    if !name.is_empty() {
        gateway = gateway.name(name);
    }

    // Device token
    let device_token = unsafe { options.device_token.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("device_token is invalid: {e}")))?;
    if !device_token.is_empty() {
        gateway = gateway.device_token(device_token);
    }

    // Supported encodings
    if options.supported_encodings_count > 0 {
        if options.supported_encodings.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "supported_encodings is null".to_string(),
            ));
        }
        gateway = gateway.supported_encodings(
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

    if options.server_info_count > 0 {
        let server_info = unsafe {
            parse_key_value_array(
                options.server_info,
                options.server_info_count,
                "server_info",
            )?
        };
        gateway = gateway.server_info(server_info);
    }

    // Callbacks
    if let Some(callbacks) = options.callbacks {
        gateway = gateway.listener(Arc::new(callbacks.clone()));
    }

    // Channel filter
    if let Some(sink_channel_filter) = options.sink_channel_filter {
        gateway = gateway.channel_filter(Arc::new(ChannelFilter::new(
            options.sink_channel_filter_context,
            sink_channel_filter,
        )));
    }

    // QoS classifier
    if let Some(qos_classifier) = options.qos_classifier {
        gateway = gateway.qos_classifier(Arc::new(QosClassifier::new(
            options.qos_classifier_context,
            qos_classifier,
        )));
    }

    // Fetch asset handler
    if let Some(fetch_asset) = options.fetch_asset {
        gateway = gateway.fetch_asset_handler(Box::new(FetchAssetHandler::new(
            options.fetch_asset_context,
            fetch_asset,
        )));
    }

    // Context
    if !options.context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(options.context) });
        gateway = gateway.context(&context);
    }

    // Foxglove API URL
    let api_url = unsafe { options.foxglove_api_url.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("foxglove_api_url is invalid: {e}"))
    })?;
    if !api_url.is_empty() {
        gateway = gateway.foxglove_api_url(api_url);
    }

    // Foxglove API timeout
    if let Some(&timeout_secs) = options.foxglove_api_timeout_secs {
        gateway = gateway.foxglove_api_timeout(Duration::from_secs(timeout_secs));
    }

    // Message backlog size
    if let Some(&backlog_size) = options.message_backlog_size {
        gateway = gateway.message_backlog_size(backlog_size);
    }

    let handle = gateway.start()?;
    Ok(Box::into_raw(Box::new(FoxgloveGateway(Some(handle)))))
}

/// Stop and shut down the gateway and free the resources associated with it.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_gateway_stop(gateway: Option<&mut FoxgloveGateway>) -> FoxgloveError {
    let Some(gateway) = gateway else {
        tracing::error!("foxglove_gateway_stop called with null gateway");
        return FoxgloveError::ValueError;
    };

    // Safety: undo the Box::into_raw in foxglove_gateway_start, safe if this was created by that method
    let mut gateway = unsafe { Box::from_raw(gateway) };
    let Some(handle) = gateway.take() else {
        tracing::error!("foxglove_gateway_stop called with closed gateway");
        return FoxgloveError::SinkClosed;
    };
    handle.stop_blocking();
    FoxgloveError::Ok
}

/// Get the current connection status of the gateway.
///
/// Returns `Shutdown` if the gateway pointer is null or the gateway has been stopped.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_gateway_connection_status(
    gateway: Option<&FoxgloveGateway>,
) -> FoxgloveConnectionStatus {
    let Some(gateway) = gateway else {
        return FoxgloveConnectionStatus::Shutdown;
    };
    let Some(handle) = gateway.as_ref() else {
        return FoxgloveConnectionStatus::Shutdown;
    };
    FoxgloveConnectionStatus::from(handle.connection_status())
}

/// Get the sink ID of the gateway's current session.
///
/// Returns 0 if the gateway pointer is null, the gateway has been stopped,
/// or no session is currently active.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_gateway_sink_id(gateway: Option<&FoxgloveGateway>) -> FoxgloveSinkId {
    let Some(gateway) = gateway else {
        return 0;
    };
    let Some(handle) = gateway.as_ref() else {
        return 0;
    };
    handle.sink_id().map(|id| id.into()).unwrap_or(0)
}

/// Adds a service to the gateway and advertises it to connected clients.
///
/// # Safety
/// - `gateway` must be a valid pointer to a gateway started with `foxglove_gateway_start`.
/// - `service` must be a valid pointer to a service allocated by `foxglove_service_create`. This
///   value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_gateway_add_service(
    gateway: Option<&FoxgloveGateway>,
    service: *mut FoxgloveService,
) -> FoxgloveError {
    if service.is_null() {
        return FoxgloveError::ValueError;
    }
    let service = unsafe { FoxgloveService::from_raw(service) };
    let Some(gateway) = gateway else {
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    handle
        .add_services([service.into_inner()])
        .err()
        .map(FoxgloveError::from)
        .unwrap_or(FoxgloveError::Ok)
}

/// Removes a service from the gateway.
///
/// # Safety
/// - `gateway` must be a valid pointer to a gateway started with `foxglove_gateway_start`.
/// - `service_name` must be a valid UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_gateway_remove_service(
    gateway: Option<&FoxgloveGateway>,
    service_name: FoxgloveString,
) -> FoxgloveError {
    let Some(gateway) = gateway else {
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    let service_name = unsafe { service_name.as_utf8_str() };
    let Ok(service_name) = service_name else {
        return FoxgloveError::Utf8Error;
    };
    handle.remove_services([service_name]);
    FoxgloveError::Ok
}

/// Publish parameter values to all subscribed clients.
///
/// # Safety
/// - `gateway` must be a valid pointer to a gateway started with `foxglove_gateway_start`.
/// - `params` must be a valid pointer to a value allocated by `foxglove_parameter_array_create`.
///   This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_gateway_publish_parameter_values(
    gateway: Option<&FoxgloveGateway>,
    params: *mut FoxgloveParameterArray,
) -> FoxgloveError {
    if params.is_null() {
        tracing::error!("foxglove_gateway_publish_parameter_values called with null params");
        return FoxgloveError::ValueError;
    }
    let params = unsafe { FoxgloveParameterArray::from_raw(params) };
    let Some(gateway) = gateway else {
        tracing::error!("foxglove_gateway_publish_parameter_values called with null gateway");
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
        tracing::error!("foxglove_gateway_publish_parameter_values called with closed gateway");
        return FoxgloveError::SinkClosed;
    };
    handle.publish_parameter_values(params.into_native());
    FoxgloveError::Ok
}

/// Publishes a status message to all connected participants.
///
/// The caller may optionally provide a message ID, which can be used in a subsequent call to
/// `foxglove_gateway_remove_status`.
///
/// # Safety
/// - `gateway` must be a valid pointer to a gateway started with `foxglove_gateway_start`.
/// - `message` must be a valid UTF-8 string, which must remain valid for the duration of this
///   call.
/// - `id` must either be NULL, or a pointer to a valid UTF-8 string, which must remain valid for
///   the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_gateway_publish_status(
    gateway: Option<&FoxgloveGateway>,
    level: FoxgloveServerStatusLevel,
    message: FoxgloveString,
    id: Option<&FoxgloveString>,
) -> FoxgloveError {
    let Some(gateway) = gateway else {
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
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
    let mut status = foxglove::remote_access::Status::new(level.into(), message);
    if let Some(id) = id {
        status = status.with_id(id);
    }
    handle.publish_status(status);
    FoxgloveError::Ok
}

/// Removes status messages from all connected participants.
///
/// Previously published status messages are referenced by ID.
///
/// # Safety
/// - `gateway` must be a valid pointer to a gateway started with `foxglove_gateway_start`.
/// - `ids` must be a pointer to an array of valid UTF-8 strings, all of which must remain valid
///   for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_gateway_remove_status(
    gateway: Option<&FoxgloveGateway>,
    ids: *const FoxgloveString,
    ids_count: usize,
) -> FoxgloveError {
    let Some(gateway) = gateway else {
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
        return FoxgloveError::SinkClosed;
    };
    if ids_count == 0 {
        return FoxgloveError::Ok;
    }
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
    handle.remove_status(ids);
    FoxgloveError::Ok
}

/// Publish a connection graph to the gateway.
///
/// Requires `FOXGLOVE_GATEWAY_CAPABILITY_CONNECTION_GRAPH`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_gateway_publish_connection_graph(
    gateway: Option<&FoxgloveGateway>,
    graph: Option<&FoxgloveConnectionGraph>,
) -> FoxgloveError {
    let Some(gateway) = gateway else {
        tracing::error!("foxglove_gateway_publish_connection_graph called with null gateway");
        return FoxgloveError::ValueError;
    };
    let Some(graph) = graph else {
        tracing::error!("foxglove_gateway_publish_connection_graph called with null graph");
        return FoxgloveError::ValueError;
    };
    let Some(handle) = gateway.as_ref() else {
        tracing::error!("foxglove_gateway_publish_connection_graph called with closed gateway");
        return FoxgloveError::SinkClosed;
    };
    match handle.publish_connection_graph(graph.0.clone()) {
        Ok(_) => FoxgloveError::Ok,
        Err(e) => FoxgloveError::from(e),
    }
}
