// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

use std::ffi::{c_char, CStr};

pub struct FoxgloveWebSocketServer(Option<foxglove::WebSocketServerBlockingHandle>);

/// Create and start a server. The server must later be freed with `foxglove_server_destroy`.
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
    Box::into_raw(Box::new(FoxgloveWebSocketServer(Some(
        foxglove::WebSocketServer::new()
            .name(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
            .bind(unsafe { CStr::from_ptr(host) }.to_str().unwrap(), port)
            .start_blocking()
            .expect("Server failed to start"),
    ))))
}

/// Free a server created via `foxglove_server_start`.
///
/// If the server has not already been stopped, it will be stopped automatically.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_server_destroy(server: Option<&mut FoxgloveWebSocketServer>) {
    let Some(server) = server else {
        return;
    };
    if let Some(handle) = server.0.take() {
        handle.stop();
    }
    drop(unsafe { Box::from_raw(server) });
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
