// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use mcap::WriteOptions;
use std::ffi::{c_char, c_void, CStr};
use std::fs::File;
use std::io::BufWriter;
use std::mem::ManuallyDrop;
use std::sync::Arc;

#[repr(C)]
pub struct FoxgloveServerOptions<'a> {
    pub name: *const c_char,
    pub host: *const c_char,
    pub port: u16,
    pub callbacks: Option<&'a FoxgloveServerCallbacks>,
}

#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveServerCallbacks {
    /// A user-defined value that will be passed to callback functions
    pub context: *const c_void,
    pub on_subscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    pub on_unsubscribe: Option<unsafe extern "C" fn(channel_id: u64, context: *const c_void)>,
    // pub on_client_advertise: Option<unsafe extern "C" fn()>
    // pub on_message_data: Option<unsafe extern "C" fn(client_channel_id: u32, payload: *const u8, payload_len: usize)>,
    // pub on_client_unadvertise: Option<unsafe extern "C" fn()>
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
        .bind(host, options.port);
    if let Some(callbacks) = options.callbacks {
        server = server.listener(Arc::new(callbacks.clone()))
    }
    Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        server.start_blocking().expect("Server failed to start"),
    ))))
}

#[repr(C)]
pub struct FoxgloveMcapOptions {
    pub path: *const c_char,
    pub path_len: usize,
    pub create: bool,
    pub truncate: bool,
    // pub compression: Option<Compression>,
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

        WriteOptions::default()
            .profile(profile)
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
}
