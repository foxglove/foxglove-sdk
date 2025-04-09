// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use bitflags::bitflags;
use mcap::{Compression, WriteOptions};
use std::ffi::{c_char, c_void, CStr, CString};
use std::fs::File;
use std::io::BufWriter;
use std::mem::ManuallyDrop;
use std::sync::Arc;

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

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FoxgloveServerCapabilityBitFlags: u8 {
        const ClientPublish = FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH;
        const ConnectionGraph = FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH;
        const Parameters = FOXGLOVE_SERVER_CAPABILITY_PARAMETERS;
        const Time = FOXGLOVE_SERVER_CAPABILITY_TIME;
        const Services = FOXGLOVE_SERVER_CAPABILITY_SERVICES;
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
    pub name: *const c_char,
    pub host: *const c_char,
    pub port: u16,
    pub callbacks: Option<&'a FoxgloveServerCallbacks>,
    pub capabilities: FoxgloveServerCapability,
    pub supported_encodings: *const *const c_char,
    pub supported_encodings_count: usize,
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
#[derive(Clone)]
pub struct FoxgloveServerCallbacks {
    /// A user-defined value that will be passed to callback functions
    pub context: *const c_void,
    pub on_subscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    pub on_unsubscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    pub on_client_advertise: Option<
        unsafe extern "C" fn(
            client_id: u32,
            channel: *const FoxgloveClientChannel,
            context: *const c_void,
        ),
    >,
    pub on_message_data: Option<
        unsafe extern "C" fn(
            client_id: u32,
            client_channel_id: u32,
            payload: *const u8,
            payload_len: usize,
            context: *const c_void,
        ),
    >,
    pub on_client_unadvertise: Option<
        unsafe extern "C" fn(client_id: u32, client_channel_id: u32, context: *const c_void),
    >,
    // pub on_get_parameters: Option<unsafe extern "C" fn()>
    // pub on_set_parameters: Option<unsafe extern "C" fn()>
    // pub on_parameters_subscribe: Option<unsafe extern "C" fn()>
    // pub on_parameters_unsubscribe: Option<unsafe extern "C" fn()>
    // pub on_connection_graph_subscribe: Option<unsafe extern "C" fn()>
    // pub on_connection_graph_unsubscribe: Option<unsafe extern "C" fn()>
}
unsafe impl Send for FoxgloveServerCallbacks {}
unsafe impl Sync for FoxgloveServerCallbacks {}

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerHandle>);

// cbindgen does not actually generate a declaration for this, so we manually write one in
// after_includes
pub use foxglove::RawChannel as FoxgloveChannel;

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
        .capabilities(
            FoxgloveServerCapabilityBitFlags::from(options.capabilities)
                .iter_websocket_capabilities(),
        )
        .bind(host, options.port);
    if options.supported_encodings_count > 0 {
        server = server.supported_encodings(
            unsafe {
                std::slice::from_raw_parts(
                    options.supported_encodings,
                    options.supported_encodings_count,
                )
            }
            .iter()
            .map(|&enc| {
                assert!(!enc.is_null());
                unsafe { CStr::from_ptr(enc) }
                    .to_str()
                    .expect("encoding is invalid")
            }),
        );
    }
    if let Some(callbacks) = options.callbacks {
        server = server.listener(Arc::new(callbacks.clone()))
    }
    Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        server.start_blocking().expect("Server failed to start"),
    ))))
}

#[repr(u8)]
pub enum FoxgloveMcapCompression {
    None,
    Zstd,
    Lz4,
}

#[repr(C)]
pub struct FoxgloveMcapOptions {
    pub path: *const c_char,
    pub path_len: usize,
    pub create: bool,
    pub truncate: bool,
    pub compression: FoxgloveMcapCompression,
    pub profile: *const c_char,
    pub profile_len: usize,
    // The library option is not provided here, because it is ignored by our Rust SDK
    /// chunk_size of 0 is treated as if it was omitted (None)
    pub chunk_size: u64,
    pub use_chunks: bool,
    pub disable_seeking: bool,
    pub emit_statistics: bool,
    pub emit_summary_offsets: bool,
    pub emit_message_indexes: bool,
    pub emit_chunk_indexes: bool,
    pub emit_attachment_indexes: bool,
    pub emit_metadata_indexes: bool,
    pub repeat_channels: bool,
    pub repeat_schemas: bool,
}

impl FoxgloveMcapOptions {
    unsafe fn to_write_options(&self) -> WriteOptions {
        let profile = std::str::from_utf8(unsafe {
            std::slice::from_raw_parts(self.profile as *const u8, self.profile_len)
        })
        .expect("profile is invalid");

        let compression = match self.compression {
            FoxgloveMcapCompression::Zstd => Some(Compression::Zstd),
            FoxgloveMcapCompression::Lz4 => Some(Compression::Lz4),
            _ => None,
        };

        WriteOptions::default()
            .profile(profile)
            .compression(compression)
            .chunk_size(if self.chunk_size > 0 {
                Some(self.chunk_size)
            } else {
                None
            })
            .use_chunks(self.use_chunks)
            .disable_seeking(self.disable_seeking)
            .emit_statistics(self.emit_statistics)
            .emit_summary_offsets(self.emit_summary_offsets)
            .emit_message_indexes(self.emit_message_indexes)
            .emit_chunk_indexes(self.emit_chunk_indexes)
            .emit_attachment_indexes(self.emit_attachment_indexes)
            .emit_metadata_indexes(self.emit_metadata_indexes)
            .repeat_channels(self.repeat_channels)
            .repeat_schemas(self.repeat_schemas)
    }
}

pub struct FoxgloveMcapWriter(Option<foxglove::McapWriterHandle<BufWriter<File>>>);

/// Create or open an MCAP file for writing. Must later be freed with `foxglove_mcap_free`.
///
/// # Safety
/// `path`, `profile`, and `library` must be valid UTF8.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
) -> *mut FoxgloveMcapWriter {
    let path = std::str::from_utf8(unsafe {
        std::slice::from_raw_parts(options.path as *const u8, options.path_len)
    })
    .expect("path is invalid");

    // Safety: this is safe if the options struct contains valid strings
    let mcap_options = unsafe { options.to_write_options() };

    let file = File::options()
        .write(true)
        .create(options.create)
        .truncate(options.truncate)
        .open(path)
        .expect("Failed to open file");
    let writer = foxglove::McapWriter::with_options(mcap_options)
        .create(BufWriter::new(file))
        .expect("Failed to create writer");
    Box::into_raw(Box::new(FoxgloveMcapWriter(Some(writer))))
}

/// Close an MCAP file writer created via `foxglove_mcap_open`.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_close(writer: Option<&mut FoxgloveMcapWriter>) {
    let Some(writer) = writer else {
        return;
    };
    if let Some(handle) = writer.0.take() {
        handle.close().expect("Failed to close writer");
    }
}

/// Free an MCAP file writer created via `foxglove_mcap_open`.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_free(writer: Option<&mut FoxgloveMcapWriter>) {
    let Some(writer) = writer else {
        return;
    };
    if let Some(handle) = writer.0.take() {
        handle.close().expect("Failed to close writer");
    }
    // Safety: undoes the into_raw in foxglove_mcap_open
    drop(unsafe { Box::from_raw(writer) });
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
    let schema = unsafe { schema.as_ref() }.map(|schema| {
        let name = unsafe { CStr::from_ptr(schema.name) }
            .to_str()
            .expect("schema name is invalid");
        let encoding = unsafe { CStr::from_ptr(schema.encoding) }
            .to_str()
            .expect("schema encoding is invalid");
        let data = unsafe { std::slice::from_raw_parts(schema.data, schema.data_len) };
        foxglove::Schema::new(name, encoding, data.to_owned())
    });
    Arc::into_raw(
        foxglove::ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema)
            .build_raw()
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
/// `log_time` may be null or may point to a valid value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_log(
    channel: Option<&FoxgloveChannel>,
    data: *const u8,
    data_len: usize,
    log_time: *const u64,
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
        },
    );
}

/// For use by the C++ SDK. Identifies that wrapper as the source of logs.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_internal_register_cpp_wrapper() {
    foxglove::library_version::set_sdk_language("cpp");
}

impl foxglove::websocket::ServerListener for FoxgloveServerCallbacks {
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
        unsafe { on_client_advertise(client.id().into(), &c_channel, self.context) };
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
                    client.id().into(),
                    channel.id.into(),
                    payload.as_ptr(),
                    payload.len(),
                    self.context,
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
