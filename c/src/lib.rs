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

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerBlockingHandle>);

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
/// If an error occurs, returns null and error will be set with the error details.
/// Remember to call `foxglove_error_free` afterwards.
/// If error is null, no error details are returned.
///
/// # Safety
/// `name` and `host` must be null-terminated strings with valid UTF8.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_server_start(
    options: &FoxgloveServerOptions,
    error: *mut FoxgloveError,
) -> *mut FoxgloveWebSocketServer {
    unsafe {
        match do_foxglove_server_start(options) {
            Ok(server) => Box::into_raw(server),
            Err(err) => {
                set_error(error, err);
                std::ptr::null_mut()
            }
        }
    }
}

unsafe fn do_foxglove_server_start(
    options: &FoxgloveServerOptions,
) -> Result<Box<FoxgloveWebSocketServer>, foxglove::FoxgloveError> {
    let name = unsafe { CStr::from_ptr(options.name) }
        .to_str()
        .map_err(|e| foxglove::FoxgloveError::ValueError(format!("name is invalid: {}", e)))?;
    let host = unsafe { CStr::from_ptr(options.host) }
        .to_str()
        .map_err(|e| foxglove::FoxgloveError::ValueError(format!("host is invalid: {}", e)))?;
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
            .map(|&enc| {
                if enc.is_null() {
                    return Err(foxglove::FoxgloveError::ValueError(
                        "encoding in supported_encodings is null".to_string(),
                    ));
                }
                unsafe { CStr::from_ptr(enc) }.to_str().map_err(|e| {
                    foxglove::FoxgloveError::ValueError(format!(
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
    Ok(Box::new(FoxgloveWebSocketServer(Some(
        server.start_blocking()?,
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
    unsafe fn to_write_options(&self) -> Result<WriteOptions, foxglove::FoxgloveError> {
        let profile = std::str::from_utf8(unsafe {
            std::slice::from_raw_parts(self.profile as *const u8, self.profile_len)
        })
        .map_err(|e| foxglove::FoxgloveError::ValueError(format!("profile is invalid: {}", e)))?;

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

/// Create or open an MCAP file for writing. Must later be freed with `foxglove_mcap_free`.
///
/// If an error occurs, returns null and error will be set with the error details.
/// Remember to call `foxglove_error_free` afterwards.
/// If error is null, no error details are returned.
///
/// # Safety
/// `path` and `profile` must be valid UTF8.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
    error: *mut FoxgloveError,
) -> *mut FoxgloveMcapWriter {
    unsafe {
        match do_foxglove_mcap_open(options) {
            Ok(writer) => Box::into_raw(writer),
            Err(err) => {
                set_error(error, err);
                std::ptr::null_mut()
            }
        }
    }
}

unsafe fn do_foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
) -> Result<Box<FoxgloveMcapWriter>, foxglove::FoxgloveError> {
    let path = std::str::from_utf8(unsafe {
        std::slice::from_raw_parts(options.path as *const u8, options.path_len)
    })
    .map_err(|e| foxglove::FoxgloveError::ValueError(format!("path is invalid: {}", e)))?;

    // Safety: this is safe if the options struct contains valid strings
    let mcap_options = unsafe { options.to_write_options() }?;

    println!("create, truncate: {}, {}", options.create, options.truncate);

    let mut file_options = File::options();
    file_options.write(true);
    if options.create {
        if options.truncate {
            file_options.create(true).truncate(true);
        } else {
            file_options.create_new(true);
        }
    } else if options.truncate {
        file_options.truncate(true);
    } else {
        // Append doesn't make sense with mcap
        return Err(foxglove::FoxgloveError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "append mode not supported with mcap, specify create=true and/or truncate=true",
        )));
    }
    let file = file_options
        .open(path)
        .map_err(foxglove::FoxgloveError::IoError)?;

    let writer = foxglove::McapWriter::with_options(mcap_options).create(BufWriter::new(file))?;
    Ok(Box::new(FoxgloveMcapWriter(Some(writer))))
}

/// Close an MCAP file writer created via `foxglove_mcap_open`.
/// Returns true if file closed without error.
///
/// If an error occurs, returns false and error will be set with the error details.
/// Remember to call `foxglove_error_free` afterwards.
/// If error is null, no error details are returned.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_close(
    writer: Option<&mut FoxgloveMcapWriter>,
    error: *mut FoxgloveError,
) -> bool {
    let Some(writer) = writer else {
        unsafe {
            set_error(
                error,
                foxglove::FoxgloveError::ValueError("writer is null".to_string()),
            );
        }
        return false;
    };
    if let Some(handle) = writer.0.take() {
        if let Err(e) = handle.close() {
            unsafe {
                set_error(error, e);
            }
            return false;
        }
    }
    true
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
        if let Err(e) = handle.close() {
            tracing::error!("failed to close mcap writer: {}", e);
        }
    }
    // Safety: undoes the into_raw in foxglove_mcap_open
    drop(unsafe { Box::from_raw(writer) });
}

/// Free a server created via `foxglove_server_start`.
///
/// If the server has not already been stopped, it will be stopped automatically.
/// Does nothing if server is null.
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
        tracing::error!("foxglove_server_get_port called with null server");
        return 0;
    };
    let Some(ref handle) = server.0 else {
        tracing::debug!("foxgove_server_get_port called with stopped server");
        return 0;
    };
    handle.port()
}

/// Stop and shut down a server.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_stop(server: Option<&mut FoxgloveWebSocketServer>) {
    let Some(server) = server else {
        tracing::error!("foxglove_server_stop called with null server");
        return;
    };
    let Some(handle) = server.0.take() else {
        tracing::debug!("foxglove_server already stopped");
        return;
    };
    handle.stop();
}

/// Create a new channel. The channel must later be freed with `foxglove_channel_free`.
///
/// If an error occurs, returns null and error will be set with the error details.
/// Remember to call `foxglove_error_free` afterwards.
/// If error is null, no error details are returned.
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
    error: *mut FoxgloveError,
) -> *mut FoxgloveChannel {
    unsafe {
        match do_foxglove_channel_create(topic, message_encoding, schema) {
            Ok(channel) => Arc::into_raw(channel).cast_mut(),
            Err(e) => {
                set_error(error, e);
                std::ptr::null_mut()
            }
        }
    }
}

unsafe fn do_foxglove_channel_create(
    topic: *const c_char,
    message_encoding: *const c_char,
    schema: *const FoxgloveSchema,
) -> Result<Arc<foxglove::RawChannel>, foxglove::FoxgloveError> {
    let topic = unsafe { CStr::from_ptr(topic) }
        .to_str()
        .map_err(|e| foxglove::FoxgloveError::ValueError(format!("topic invalid: {}", e)))?;
    let message_encoding = unsafe { CStr::from_ptr(message_encoding) }
        .to_str()
        .map_err(|e| {
            foxglove::FoxgloveError::ValueError(format!("message_encoding invalid: {}", e))
        })?;

    let mut maybe_schema = None;
    if let Some(schema) = unsafe { schema.as_ref() } {
        let name = unsafe { CStr::from_ptr(schema.name) }
            .to_str()
            .map_err(|e| {
                foxglove::FoxgloveError::ValueError(format!("schema name invalid: {}", e))
            })?;
        let encoding = unsafe { CStr::from_ptr(schema.encoding) }
            .to_str()
            .map_err(|e| {
                foxglove::FoxgloveError::ValueError(format!("schema name invalid: {}", e))
            })?;
        let data = unsafe { std::slice::from_raw_parts(schema.data, schema.data_len) };
        maybe_schema = Some(foxglove::Schema::new(name, encoding, data.to_owned()));
    }

    foxglove::ChannelBuilder::new(topic)
        .message_encoding(message_encoding)
        .schema(maybe_schema)
        .build_raw()
}

unsafe fn set_error(error: *mut FoxgloveError, err: foxglove::FoxgloveError) {
    if error.is_null() {
        return;
    }
    unsafe { *error = FoxgloveError::from(err) };
}

/// Free a channel created via `foxglove_channel_create`.
/// # Safety
/// `channel` must be a valid pointer to a `FoxgloveChannel` created via `foxglove_channel_create`.
/// If channel is null, this does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_free(channel: Option<&mut FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    drop(unsafe { Arc::from_raw(channel) });
}

/// Get the ID of a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `FoxgloveChannel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, an invalid id of 0 is returned.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_id(channel: Option<&FoxgloveChannel>) -> u64 {
    let Some(channel) = channel else {
        return 0;
    };
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
    // An assert might be reasonable under different circumstances, but here
    // we don't want to crash the program using the library, on a robot in the field,
    // because it called log incorrectly. Safer to just warn about it and do nothing.
    let Some(channel) = channel else {
        tracing::error!("foxglove_channel_log called with null channel");
        return;
    };
    if data.is_null() || data_len == 0 {
        tracing::error!("foxglove_channel_log called with null or empty data");
        return;
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

#[repr(u8)]
pub enum FoxgloveErrorKind {
    Unspecified,
    ValueError,
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

impl From<&foxglove::FoxgloveError> for FoxgloveErrorKind {
    fn from(error: &foxglove::FoxgloveError) -> Self {
        match error {
            foxglove::FoxgloveError::ValueError(_) => FoxgloveErrorKind::ValueError,
            foxglove::FoxgloveError::SinkClosed => FoxgloveErrorKind::SinkClosed,
            foxglove::FoxgloveError::SchemaRequired => FoxgloveErrorKind::SchemaRequired,
            foxglove::FoxgloveError::MessageEncodingRequired => {
                FoxgloveErrorKind::MessageEncodingRequired
            }
            foxglove::FoxgloveError::ServerAlreadyStarted => {
                FoxgloveErrorKind::ServerAlreadyStarted
            }
            foxglove::FoxgloveError::Bind(_) => FoxgloveErrorKind::Bind,
            foxglove::FoxgloveError::DuplicateChannel(_) => FoxgloveErrorKind::DuplicateChannel,
            foxglove::FoxgloveError::DuplicateService(_) => FoxgloveErrorKind::DuplicateService,
            foxglove::FoxgloveError::MissingRequestEncoding(_) => {
                FoxgloveErrorKind::MissingRequestEncoding
            }
            foxglove::FoxgloveError::ServicesNotSupported => {
                FoxgloveErrorKind::ServicesNotSupported
            }
            foxglove::FoxgloveError::ConnectionGraphNotSupported => {
                FoxgloveErrorKind::ConnectionGraphNotSupported
            }
            foxglove::FoxgloveError::IoError(_) => FoxgloveErrorKind::IoError,
            foxglove::FoxgloveError::McapError(_) => FoxgloveErrorKind::McapError,
            _ => FoxgloveErrorKind::Unspecified,
        }
    }
}

const FOXGLOVE_ERROR_MAGIC: u32 = 0xDEADDEAD;

#[repr(C)]
pub struct FoxgloveError {
    pub kind: FoxgloveErrorKind,
    magic: u32,
    pub message: *const c_char,
}

impl From<foxglove::FoxgloveError> for FoxgloveError {
    fn from(error: foxglove::FoxgloveError) -> Self {
        let message = CString::new(error.to_string()).unwrap();
        Self {
            // This gives a little protection against people passing an unintialized
            // FoxgloveError into foxglove_error_free, which is an easy mistake to make in C.
            magic: FOXGLOVE_ERROR_MAGIC,
            kind: FoxgloveErrorKind::from(&error),
            message: message.into_raw(),
        }
    }
}

/// Free an error returned from a call to a foxglove.
///
/// Note this frees the message field, not the foxglove_error itself
/// (which is typically on the stack)
///
/// # Safety
/// `error` must be zero initialized or have been set by a call to another foxglove function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_error_free(error: *mut FoxgloveError) {
    unsafe {
        let err = &mut *error;
        if err.message.is_null() {
            return;
        }
        assert_eq!(
            err.magic, FOXGLOVE_ERROR_MAGIC,
            "invalid or uninitialized error passed to foxglove_error_free"
        );
        drop(CString::from_raw(err.message as *mut _));
        err.message = std::ptr::null();
    }
}
