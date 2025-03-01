// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use std::ffi::{c_char, CStr};
use std::sync::Arc;

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerBlockingHandle>);
pub struct FoxgloveChannel(Arc<foxglove::Channel>);

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
    name: *const c_char,
    host: *const c_char,
    port: u16,
) -> *mut FoxgloveWebSocketServer {
    let name = unsafe { CStr::from_ptr(name) }
        .to_str()
        .expect("name is invalid");
    let host = unsafe { CStr::from_ptr(host) }
        .to_str()
        .expect("host is invalid");
    Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        foxglove::WebSocketServer::new()
            .name(name)
            .bind(host, port)
            .start_blocking()
            .expect("Server failed to start"),
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
    Box::into_raw(Box::new(FoxgloveChannel(
        foxglove::ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema)
            .build()
            .expect("Failed to create channel"),
    )))
}

/// Free a channel created via `foxglove_channel_create`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_free(channel: Option<&mut FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    drop(unsafe { Box::from_raw(channel) });
}

/// Log a message on a channel.
///
/// # Safety
/// `data` must be non-null, and the range `[data, data + data_len)` must contain initialized data
/// contained within a single allocated object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_log(
    channel: Option<&mut FoxgloveChannel>,
    data: *const u8,
    data_len: usize,
    log_time: u64,
    publish_time: u64,
    sequence: u32,
) {
    let channel = channel.expect("channel is required");
    if data.is_null() {
        panic!("data is required");
    }
    channel.0.log_with_meta(
        unsafe { std::slice::from_raw_parts(data, data_len) },
        foxglove::PartialMetadata {
            log_time: Some(log_time),
            publish_time: Some(publish_time),
            sequence: Some(sequence),
        },
    );
}
