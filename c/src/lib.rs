// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use bitflags::bitflags;
use connection_graph::FoxgloveConnectionGraph;
use mcap::{Compression, WriteOptions};
use std::ffi::{c_char, c_void, CString};
use std::fs::File;
use std::io::BufWriter;
use std::mem::ManuallyDrop;
use std::sync::Arc;

pub mod connection_graph;

/// A string with associated length.
#[repr(C)]
pub struct FoxgloveString {
    /// Pointer to valid UTF-8 data
    data: *const c_char,
    /// Number of bytes in the string
    len: usize,
}

impl FoxgloveString {
    /// Wrapper around [`std::str::from_utf8`].
    ///
    /// # Safety
    ///
    /// The [`data`] field must be valid UTF-8, and have a length equal to [`FoxgloveString.len`].
    unsafe fn as_utf8_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(self.data.cast(), self.len) })
    }
}

#[cfg(test)]
impl From<&str> for FoxgloveString {
    fn from(s: &str) -> Self {
        Self {
            data: s.as_ptr().cast(),
            len: s.len(),
        }
    }
}

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
    pub name: FoxgloveString,
    pub host: FoxgloveString,
    pub port: u16,
    pub callbacks: Option<&'a FoxgloveServerCallbacks>,
    pub capabilities: FoxgloveServerCapability,
    pub supported_encodings: *const FoxgloveString,
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
    pub on_connection_graph_subscribe: Option<unsafe extern "C" fn(context: *const c_void)>,
    pub on_connection_graph_unsubscribe: Option<unsafe extern "C" fn(context: *const c_void)>,
    // pub on_get_parameters: Option<unsafe extern "C" fn()>
    // pub on_set_parameters: Option<unsafe extern "C" fn()>
    // pub on_parameters_subscribe: Option<unsafe extern "C" fn()>
    // pub on_parameters_unsubscribe: Option<unsafe extern "C" fn()>
}
unsafe impl Send for FoxgloveServerCallbacks {}
unsafe impl Sync for FoxgloveServerCallbacks {}

// cbindgen does not actually generate a declaration for these,
// so we manually add them in after_includes. If we use type aliases
// instead, cbindgen will generate them, but verbatim naming the rust
// types which don't exist in C.
// pub use foxglove::RawChannel as FoxgloveChannel;
// pub use foxglove::RawWebSocketServerHandle as FoxgloveWebSocketServer;
// pub use foxglove::McapWriterHandle<BufWriter<File>> as FoxgloveMcapWriter;

#[repr(C)]
pub struct FoxgloveSchema {
    pub name: FoxgloveString,
    pub encoding: FoxgloveString,
    pub data: *const u8,
    pub data_len: usize,
}

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
/// If `name` is supplied in options, it must contain valid UTF8.
/// If `host` is supplied in options, it must contain valid UTF8.
/// If `supported_encodings` is supplied in options, all `supported_encodings` must contain valid
/// UTF8, and `supported_encodings` must have length equal to `supported_encodings_count`.
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
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("name is invalid: {}", e)))?;
    let host = unsafe { options.host.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("host is invalid: {}", e)))?;

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
                        "encoding in supported_encodings is invalid: {}",
                        e
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if let Some(callbacks) = options.callbacks {
        server = server.listener(Arc::new(callbacks.clone()))
    }
    let server = server.start_blocking()?;
    Ok(Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        server,
    )))))
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
    server.stop();
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

#[repr(u8)]
pub enum FoxgloveMcapCompression {
    None,
    Zstd,
    Lz4,
}

#[repr(C)]
pub struct FoxgloveMcapOptions {
    pub path: FoxgloveString,
    pub path_len: usize,
    pub truncate: bool,
    pub compression: FoxgloveMcapCompression,
    pub profile: FoxgloveString,
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
    unsafe fn to_write_options(&self) -> Result<WriteOptions, foxglove::FoxgloveError> {
        let profile = unsafe { self.profile.as_utf8_str() }.map_err(|e| {
            foxglove::FoxgloveError::ValueError(format!("profile is invalid: {}", e))
        })?;

        let compression = match self.compression {
            FoxgloveMcapCompression::Zstd => Some(Compression::Zstd),
            FoxgloveMcapCompression::Lz4 => Some(Compression::Lz4),
            _ => None,
        };

        Ok(WriteOptions::default()
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
            .repeat_schemas(self.repeat_schemas))
    }
}

pub struct FoxgloveMcapWriter(Option<foxglove::McapWriterHandle<BufWriter<File>>>);

impl FoxgloveMcapWriter {
    fn take(&mut self) -> Option<foxglove::McapWriterHandle<BufWriter<File>>> {
        self.0.take()
    }
}

/// Create or open an MCAP file for writing.
/// Resources must later be freed with `foxglove_mcap_close`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `path` and `profile` must contain valid UTF8.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
    writer: *mut *mut FoxgloveMcapWriter,
) -> FoxgloveError {
    unsafe {
        let result = do_foxglove_mcap_open(options);
        result_to_c(result, writer)
    }
}

unsafe fn do_foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
) -> Result<*mut FoxgloveMcapWriter, foxglove::FoxgloveError> {
    let path = unsafe { options.path.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("path is invalid: {}", e)))?;

    // Safety: this is safe if the options struct contains valid strings
    let mcap_options = unsafe { options.to_write_options() }?;

    let mut file_options = File::options();
    if options.truncate {
        file_options.create(true).truncate(true);
    } else {
        file_options.create_new(true);
    }
    let file = file_options
        .write(true)
        .open(path)
        .map_err(foxglove::FoxgloveError::IoError)?;

    let writer = foxglove::McapWriter::with_options(mcap_options).create(BufWriter::new(file))?;
    // We can avoid this double indirection if we refactor McapWriterHandle to move the context into the Arc
    // and then add into_raw and from_raw methods to convert the Arc to and from a pointer.
    // This is the simplest solution, and we don't call methods on this, so the double indirection doesn't matter much.
    Ok(Box::into_raw(Box::new(FoxgloveMcapWriter(Some(writer)))))
}

/// Close an MCAP file writer created via `foxglove_mcap_open`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_close(
    writer: Option<&mut FoxgloveMcapWriter>,
) -> FoxgloveError {
    let Some(writer) = writer else {
        tracing::error!("foxglove_mcap_close called with null writer");
        return FoxgloveError::ValueError;
    };
    // Safety: undo the Box::into_raw in foxglove_mcap_open, safe if this was created by that method
    let mut writer = unsafe { Box::from_raw(writer) };
    let Some(writer) = writer.take() else {
        tracing::error!("foxglove_mcap_close called with writer already closed");
        return FoxgloveError::SinkClosed;
    };
    let result = writer.close();
    // We don't care about the return value
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

pub struct FoxgloveChannel(foxglove::RawChannel);

/// Create a new channel. The channel must later be freed with `foxglove_channel_free`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `topic` and `message_encoding` must contain valid UTF8. `schema` is an optional pointer to a
/// schema. The schema and the data it points to need only remain alive for the duration of this
/// function call (they will be copied).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_create(
    topic: FoxgloveString,
    message_encoding: FoxgloveString,
    schema: *const FoxgloveSchema,
    channel: *mut *const FoxgloveChannel,
) -> FoxgloveError {
    if channel.is_null() {
        tracing::error!("channel cannot be null");
        return FoxgloveError::ValueError;
    }
    unsafe {
        let result = do_foxglove_channel_create(topic, message_encoding, schema);
        result_to_c(result, channel)
    }
}

unsafe fn do_foxglove_channel_create(
    topic: FoxgloveString,
    message_encoding: FoxgloveString,
    schema: *const FoxgloveSchema,
) -> Result<*const FoxgloveChannel, foxglove::FoxgloveError> {
    let topic = unsafe { topic.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic invalid: {}", e)))?;
    let message_encoding = unsafe { message_encoding.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("message_encoding invalid: {}", e))
    })?;

    let mut maybe_schema = None;
    if let Some(schema) = unsafe { schema.as_ref() } {
        let name = unsafe { schema.name.as_utf8_str() }.map_err(|e| {
            foxglove::FoxgloveError::Utf8Error(format!("schema name invalid: {}", e))
        })?;
        let encoding = unsafe { schema.encoding.as_utf8_str() }.map_err(|e| {
            foxglove::FoxgloveError::Utf8Error(format!("schema encoding invalid: {}", e))
        })?;

        let data = unsafe { std::slice::from_raw_parts(schema.data, schema.data_len) };
        maybe_schema = Some(foxglove::Schema::new(name, encoding, data.to_owned()));
    }

    foxglove::ChannelBuilder::new(topic)
        .message_encoding(message_encoding)
        .schema(maybe_schema)
        .build_raw()
        .map(|raw_channel| Arc::into_raw(raw_channel) as *const FoxgloveChannel)
}

/// Free a channel created via `foxglove_channel_create`.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
/// If channel is null, this does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_free(channel: Option<&FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    drop(unsafe { Arc::from_raw(channel) });
}

/// Get the ID of a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, an invalid id of 0 is returned.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_id(channel: Option<&FoxgloveChannel>) -> u64 {
    let Some(channel) = channel else {
        return 0;
    };
    u64::from(channel.0.id())
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
) -> FoxgloveError {
    // An assert might be reasonable under different circumstances, but here
    // we don't want to crash the program using the library, on a robot in the field,
    // because it called log incorrectly. It's safer to warn about it and do nothing.
    let Some(channel) = channel else {
        tracing::error!("foxglove_channel_log called with null channel");
        return FoxgloveError::ValueError;
    };
    if data.is_null() || data_len == 0 {
        tracing::error!("foxglove_channel_log called with null or empty data");
        return FoxgloveError::ValueError;
    }
    // avoid decrementing ref count
    let channel = ManuallyDrop::new(unsafe {
        Arc::from_raw(channel as *const _ as *const foxglove::RawChannel)
    });
    channel.log_with_meta(
        unsafe { std::slice::from_raw_parts(data, data_len) },
        foxglove::PartialMetadata {
            log_time: unsafe { log_time.as_ref() }.copied(),
        },
    );
    FoxgloveError::Ok
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

#[repr(u8)]
#[derive(PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum FoxgloveError {
    Ok,
    Unspecified,
    ValueError,
    Utf8Error,
    SinkClosed,
    SchemaRequired,
    MessageEncodingRequired,
    ServerAlreadyStarted,
    Bind,
    DuplicateChannel,
    DuplicateService,
    MissingRequestEncoding,
    ServicesNotSupported,
    ConnectionGraphNotSupported,
    IoError,
    McapError,
}

impl From<foxglove::FoxgloveError> for FoxgloveError {
    fn from(error: foxglove::FoxgloveError) -> Self {
        match error {
            foxglove::FoxgloveError::ValueError(_) => FoxgloveError::ValueError,
            foxglove::FoxgloveError::Utf8Error(_) => FoxgloveError::Utf8Error,
            foxglove::FoxgloveError::SinkClosed => FoxgloveError::SinkClosed,
            foxglove::FoxgloveError::SchemaRequired => FoxgloveError::SchemaRequired,
            foxglove::FoxgloveError::MessageEncodingRequired => {
                FoxgloveError::MessageEncodingRequired
            }
            foxglove::FoxgloveError::ServerAlreadyStarted => FoxgloveError::ServerAlreadyStarted,
            foxglove::FoxgloveError::Bind(_) => FoxgloveError::Bind,
            foxglove::FoxgloveError::DuplicateChannel(_) => FoxgloveError::DuplicateChannel,
            foxglove::FoxgloveError::DuplicateService(_) => FoxgloveError::DuplicateService,
            foxglove::FoxgloveError::MissingRequestEncoding(_) => {
                FoxgloveError::MissingRequestEncoding
            }
            foxglove::FoxgloveError::ServicesNotSupported => FoxgloveError::ServicesNotSupported,
            foxglove::FoxgloveError::ConnectionGraphNotSupported => {
                FoxgloveError::ConnectionGraphNotSupported
            }
            foxglove::FoxgloveError::IoError(_) => FoxgloveError::IoError,
            foxglove::FoxgloveError::McapError(_) => FoxgloveError::McapError,
            _ => FoxgloveError::Unspecified,
        }
    }
}

impl FoxgloveError {
    fn to_cstr(&self) -> &'static std::ffi::CStr {
        match self {
            FoxgloveError::Ok => c"Ok",
            FoxgloveError::ValueError => c"Value Error",
            FoxgloveError::Utf8Error => c"UTF-8 Error",
            FoxgloveError::SinkClosed => c"Sink Closed",
            FoxgloveError::SchemaRequired => c"Schema Required",
            FoxgloveError::MessageEncodingRequired => c"Message Encoding Required",
            FoxgloveError::ServerAlreadyStarted => c"Server Already Started",
            FoxgloveError::Bind => c"Bind Error",
            FoxgloveError::DuplicateChannel => c"Duplicate Channel",
            FoxgloveError::DuplicateService => c"Duplicate Service",
            FoxgloveError::MissingRequestEncoding => c"Missing Request Encoding",
            FoxgloveError::ServicesNotSupported => c"Services Not Supported",
            FoxgloveError::ConnectionGraphNotSupported => c"Connection Graph Not Supported",
            FoxgloveError::IoError => c"IO Error",
            FoxgloveError::McapError => c"MCAP Error",
            _ => c"Unspecified Error",
        }
    }
}

unsafe fn result_to_c<T>(
    result: Result<T, foxglove::FoxgloveError>,
    out_ptr: *mut T,
) -> FoxgloveError {
    match result {
        Ok(value) => {
            // A null out_ptr is allowed if we don't care about the result value,
            // see foxglove_mcap_close for an example.
            if !out_ptr.is_null() {
                unsafe { *out_ptr = value };
            }
            FoxgloveError::Ok
        }
        Err(e) => {
            tracing::error!("{}", e);
            e.into()
        }
    }
}

/// Convert a `FoxgloveError` code to a C string.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_error_to_cstr(error: FoxgloveError) -> *const c_char {
    error.to_cstr().as_ptr() as *const _
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foxglove_string_as_utf8_str() {
        let string = FoxgloveString {
            data: c"test".as_ptr(),
            len: 4,
        };
        let utf8_str = unsafe { string.as_utf8_str() };
        assert_eq!(utf8_str, Ok("test"));

        let string = FoxgloveString {
            data: c"💖".as_ptr(),
            len: 4,
        };
        let utf8_str = unsafe { string.as_utf8_str() };
        assert_eq!(utf8_str, Ok("💖"));
    }
}
