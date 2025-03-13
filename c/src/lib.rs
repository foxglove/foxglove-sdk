// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use bitflags::bitflags;
use std::ffi::{c_char, c_void, CStr};
use std::mem::ManuallyDrop;
use std::sync::Arc;

bitflags! {
    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct FoxgloveServerCapability: u8 {
        /// Allow clients to advertise channels to send data messages to the server.
        const ClientPublish = 1 << 0;
        /// Allow clients to subscribe and make connection graph updates
        const ConnectionGraph = 1 << 1;
        /// Allow clients to get & set parameters.
        const Parameters = 1 << 2;
        /// Inform clients about the latest server time.
        ///
        /// This allows accelerated, slowed, or stepped control over the progress of time. If the
        /// server publishes time data, then timestamps of published messages must originate from the
        /// same time source.
        const Time = 1 << 3;
        /// Allow clients to call services.
        const Services = 1 << 4;
    }
}

impl FoxgloveServerCapability {
    fn iter_websocket_capabilities(self) -> impl Iterator<Item = foxglove::websocket::Capability> {
        self.iter_names().filter_map(|(_s, cap)| match cap {
            FoxgloveServerCapability::ClientPublish => {
                Some(foxglove::websocket::Capability::ClientPublish)
            }
            FoxgloveServerCapability::ConnectionGraph => {
                Some(foxglove::websocket::Capability::ConnectionGraph)
            }
            FoxgloveServerCapability::Parameters => {
                Some(foxglove::websocket::Capability::Parameters)
            }
            FoxgloveServerCapability::Time => Some(foxglove::websocket::Capability::Time),
            FoxgloveServerCapability::Services => Some(foxglove::websocket::Capability::Services),
            _ => None,
        })
    }
}

#[repr(C)]
pub struct FoxgloveServerOptions<'a> {
    pub name: *const c_char,
    pub host: *const c_char,
    pub port: u16,
    pub callbacks: Option<&'a FoxgloveServerCallbacks>,
    pub capabilities: FoxgloveServerCapability,
}

#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveServerCallbacks {
    /// A user-defined value that will be passed to callback functions
    pub context: *const c_void,
    pub on_subscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    pub on_unsubscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    pub on_client_advertise: Option<
        unsafe extern "C" fn(client_channel_id: u32, topic: *const c_char, context: *const c_void),
    >,
    pub on_message_data: Option<
        unsafe extern "C" fn(
            client_channel_id: u32,
            payload: *const u8,
            payload_len: usize,
            context: *const c_void,
        ),
    >,
    pub on_client_unadvertise:
        Option<unsafe extern "C" fn(client_channel_id: u32, context: *const c_void)>,
    // pub on_get_parameters: Option<unsafe extern "C" fn()>
    // pub on_set_parameters: Option<unsafe extern "C" fn()>
    // pub on_parameters_subscribe: Option<unsafe extern "C" fn()>
    // pub on_parameters_unsubscribe: Option<unsafe extern "C" fn()>
    // pub on_connection_graph_subscribe: Option<unsafe extern "C" fn()>
    // pub on_connection_graph_unsubscribe: Option<unsafe extern "C" fn()>
}
unsafe impl Send for FoxgloveServerCallbacks {}
unsafe impl Sync for FoxgloveServerCallbacks {}

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerBlockingHandle>);

use foxglove::websocket::ServerListener;
// cbindgen does not actually generate a declaration for this, so we manually write one in
// after_includes
pub use foxglove::Channel as FoxgloveChannel;

#[repr(C)]
pub struct FoxgloveSchema {
    pub name: *const c_char,
    pub encoding: *const c_char,
    pub data: *const u8,
    pub data_len: usize,
}

/// Create and start a server. The server must later be freed with `foxglove_server_free`.
///
/// `port` may be 0, in which case an available port will be automatically selected.
///
/// # Safety
/// `name` and `host` must be null-terminated strings with valid UTF8.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_server_start(
    options: &FoxgloveServerOptions,
) -> *mut FoxgloveWebSocketServer {
    let name = unsafe { CStr::from_ptr(options.name) }
        .to_str()
        .expect("name is invalid");
    let host = unsafe { CStr::from_ptr(options.host) }
        .to_str()
        .expect("host is invalid");
    let mut server = foxglove::WebSocketServer::new()
        .name(name)
        .capabilities(options.capabilities.iter_websocket_capabilities())
        .bind(host, options.port);
    if let Some(callbacks) = options.callbacks {
        server = server.listener(Arc::new(callbacks.clone()))
    }
    Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        server.start_blocking().expect("Server failed to start"),
    ))))
}

/// Free a server created via `foxglove_server_start`.
///
/// If the server has not already been stopped, it will be stopped automatically.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_free(server: Option<&mut FoxgloveWebSocketServer>) {
    let Some(server) = server else {
        return;
    };
    if let Some(handle) = server.0.take() {
        handle.stop();
    }
    drop(unsafe { Box::from_raw(server) });
}

/// Get the port on which the server is listening.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_get_port(server: Option<&FoxgloveWebSocketServer>) -> u16 {
    let Some(server) = server else {
        panic!("Expected a non-null server");
    };
    let Some(ref handle) = server.0 else {
        panic!("Server already stopped");
    };
    handle.port()
}

/// Stop and shut down a server.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_stop(server: Option<&mut FoxgloveWebSocketServer>) {
    let Some(server) = server else {
        panic!("Expected a non-null server");
    };
    let Some(handle) = server.0.take() else {
        panic!("Server already stopped");
    };
    handle.stop();
}

/// Create a new channel. The channel must later be freed with `foxglove_channel_free`.
///
/// # Safety
/// `topic` and `message_encoding` must be null-terminated strings with valid UTF8. `schema` is an
/// optional pointer to a schema. The schema and the data it points to need only remain alive for
/// the duration of this function call (they will be copied).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_create(
    topic: *const c_char,
    message_encoding: *const c_char,
    schema: *const FoxgloveSchema,
) -> *mut FoxgloveChannel {
    let topic = unsafe { CStr::from_ptr(topic) }
        .to_str()
        .expect("topic is invalid");
    let message_encoding = unsafe { CStr::from_ptr(message_encoding) }
        .to_str()
        .expect("message_encoding is invalid");
    let schema = unsafe {
        schema.as_ref().map(|schema| {
            let name = CStr::from_ptr(schema.name)
                .to_str()
                .expect("schema name is invalid");
            let encoding = CStr::from_ptr(schema.encoding)
                .to_str()
                .expect("schema encoding is invalid");
            let data = std::slice::from_raw_parts(schema.data, schema.data_len);
            foxglove::Schema::new(name, encoding, data)
        })
    };
    Arc::into_raw(
        foxglove::ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema)
            .build()
            .expect("Failed to create channel"),
    )
    .cast_mut()
}

/// Free a channel created via `foxglove_channel_create`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_free(channel: Option<&mut FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    drop(unsafe { Arc::from_raw(channel) });
}

#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_id(channel: Option<&FoxgloveChannel>) -> u64 {
    let channel = channel.expect("channel is required");
    u64::from(channel.id())
}

/// Log a message on a channel.
///
/// # Safety
/// `data` must be non-null, and the range `[data, data + data_len)` must contain initialized data
/// contained within a single allocated object.
///
/// `log_time`, `publish_time`, and `sequence` may be null, or may point to valid, properly-aligned values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_log(
    channel: Option<&FoxgloveChannel>,
    data: *const u8,
    data_len: usize,
    log_time: *const u64,
    publish_time: *const u64,
    sequence: *const u32,
) {
    let channel = channel.expect("channel is required");
    if data.is_null() {
        panic!("data is required");
    }
    // avoid decrementing ref count
    let channel = ManuallyDrop::new(unsafe { Arc::from_raw(channel) });
    channel.log_with_meta(
        unsafe { std::slice::from_raw_parts(data, data_len) },
        foxglove::PartialMetadata {
            log_time: unsafe { log_time.as_ref() }.copied(),
            publish_time: unsafe { publish_time.as_ref() }.copied(),
            sequence: unsafe { sequence.as_ref() }.copied(),
        },
    );
}

impl ServerListener for FoxgloveServerCallbacks {
    fn on_subscribe(
        &self,
        _client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        if let Some(on_subscribe) = self.on_subscribe {
            unsafe { on_subscribe(u64::from(channel.id()), self.context) };
        }
    }

    fn on_unsubscribe(
        &self,
        _client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        if let Some(on_unsubscribe) = self.on_unsubscribe {
            unsafe { on_unsubscribe(u64::from(channel.id()), self.context) };
        }
    }

    fn on_client_advertise(
        &self,
        _client: foxglove::websocket::Client,
        channel: foxglove::websocket::ClientChannelView,
    ) {
        if let Some(on_client_advertise) = self.on_client_advertise {
            unsafe {
                on_client_advertise(
                    u32::from(channel.id()),
                    channel.topic() as *const str as *const c_char,
                    self.context,
                )
            };
        }
    }

    fn on_message_data(
        &self,
        _client: foxglove::websocket::Client,
        channel: foxglove::websocket::ClientChannelView,
        payload: &[u8],
    ) {
        if let Some(on_message_data) = self.on_message_data {
            unsafe {
                on_message_data(
                    u32::from(channel.id()),
                    payload.as_ptr(),
                    payload.len(),
                    self.context,
                )
            };
        }
    }

    fn on_client_unadvertise(
        &self,
        _client: foxglove::websocket::Client,
        channel: foxglove::websocket::ClientChannelView,
    ) {
        if let Some(on_client_unadvertise) = self.on_client_unadvertise {
            unsafe { on_client_unadvertise(u32::from(channel.id()), self.context) };
        }
    }
}
