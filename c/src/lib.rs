// On by default in rust 2024
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unsafe_attr_outside_unsafe)]

#[cfg(test)]
mod tests;

use std::{ffi::c_char, mem::ManuallyDrop};

mod arena;
mod generated_types;
mod util;
pub use generated_types::*;
#[cfg(not(target_family = "wasm"))]
mod bytes;
#[cfg(not(target_family = "wasm"))]
mod channel;
#[cfg(not(target_family = "wasm"))]
mod channel_descriptor;
#[cfg(not(target_family = "wasm"))]
mod cloud_sink;
#[cfg(not(target_family = "wasm"))]
mod connection_graph;
#[cfg(not(target_family = "wasm"))]
mod fetch_asset;
#[cfg(not(target_family = "wasm"))]
mod logging;
#[cfg(not(target_family = "wasm"))]
mod parameter;
#[cfg(not(target_family = "wasm"))]
mod server;
#[cfg(not(target_family = "wasm"))]
mod service;
#[cfg(not(target_family = "wasm"))]
mod sink_channel_filter;

#[cfg(not(target_family = "wasm"))]
pub use cloud_sink::*;
#[cfg(not(target_family = "wasm"))]
pub use server::*;

#[cfg(not(target_family = "wasm"))]
pub use channel::*;

#[cfg(not(target_family = "wasm"))]
pub use logging::foxglove_set_log_level;

pub use foxglove::Context as FoxgloveContext;

/// A key-value pair of strings.
#[repr(C)]
pub struct FoxgloveKeyValue {
    /// The key
    key: FoxgloveString,
    /// The value
    value: FoxgloveString,
}

/// A collection of metadata items for a channel.
#[repr(C)]
pub struct FoxgloveChannelMetadata {
    /// The items in the metadata collection.
    items: *const FoxgloveKeyValue,
    /// The number of items in the metadata collection.
    count: usize,
}

/// A string with associated length.
#[repr(C)]
pub struct FoxgloveString {
    /// Pointer to valid UTF-8 data
    data: *const c_char,
    /// Number of bytes in the string
    len: usize,
}

#[cfg(not(target_family = "wasm"))]
pub(crate) type FoxgloveSinkId = u64;

impl Default for FoxgloveString {
    fn default() -> Self {
        Self {
            data: "".as_ptr().cast(),
            len: 0,
        }
    }
}

impl FoxgloveString {
    /// Wrapper around [`std::str::from_utf8`].
    ///
    /// # Safety
    ///
    /// The [`data`] field must be valid UTF-8, correctly aligned, and have a length equal to
    /// [`FoxgloveString.len`].
    unsafe fn as_utf8_str(&self) -> Result<&str, std::str::Utf8Error> {
        if self.data.is_null() {
            Ok("")
        } else {
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(self.data.cast(), self.len) })
        }
    }

    pub fn as_ptr(&self) -> *const c_char {
        self.data
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl From<&String> for FoxgloveString {
    fn from(s: &String) -> Self {
        Self {
            data: s.as_ptr().cast(),
            len: s.len(),
        }
    }
}

impl From<&str> for FoxgloveString {
    fn from(s: &str) -> Self {
        Self {
            data: s.as_ptr().cast(),
            len: s.len(),
        }
    }
}

/// An owned string buffer.
///
/// This struct is aliased as `foxglove_string` in the C API.
///
/// cbindgen:no-export
pub struct FoxgloveStringBuf(FoxgloveString);

impl FoxgloveStringBuf {
    /// Creates a new `FoxgloveString` from the provided string.
    fn new(str: String) -> Self {
        // SAFETY: Freed on drop.
        let mut str = ManuallyDrop::new(str);
        str.shrink_to_fit();
        Self(FoxgloveString {
            data: str.as_mut_ptr().cast(),
            len: str.len(),
        })
    }

    /// Wrapper around [`std::str::from_utf8`].
    fn as_str(&self) -> &str {
        // SAFETY: This was constructed from a valid `String`.
        unsafe { self.0.as_utf8_str() }.expect("valid utf-8")
    }

    /// Extracts and returns the inner string.
    fn into_string(self) -> String {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let this = ManuallyDrop::new(self);
        // SAFETY: This was constructed from a valid `String`.
        unsafe { String::from_raw_parts(this.0.data as *mut u8, this.0.len, this.0.len) }
    }
}

impl From<String> for FoxgloveStringBuf {
    fn from(str: String) -> Self {
        Self::new(str)
    }
}

impl From<FoxgloveStringBuf> for String {
    fn from(buf: FoxgloveStringBuf) -> Self {
        buf.into_string()
    }
}

impl AsRef<str> for FoxgloveStringBuf {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Clone for FoxgloveStringBuf {
    fn clone(&self) -> Self {
        self.as_str().to_string().into()
    }
}

impl Drop for FoxgloveStringBuf {
    fn drop(&mut self) {
        let FoxgloveString { data, len } = self.0;
        assert!(!data.is_null());
        // SAFETY: This was constructed from valid `String`.
        drop(unsafe { String::from_raw_parts(data as *mut u8, len, len) })
    }
}

/// A Schema is a description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
/// See the [MCAP spec](https://mcap.dev/spec#schema-op0x03) for more information.
#[repr(C)]
pub struct FoxgloveSchema {
    pub name: FoxgloveString,
    pub encoding: FoxgloveString,
    pub data: *const u8,
    pub data_len: usize,
}
#[cfg(not(target_family = "wasm"))]
impl FoxgloveSchema {
    /// Converts a schema to the native type.
    ///
    /// # Safety
    /// - `name` must be a valid pointer to a UTF-8 string.
    /// - `encoding` must be a valid pointer to a UTF-8 string.
    /// - `data` must be a valid pointer to a buffer of `data_len` bytes.
    unsafe fn to_native(&self) -> Result<foxglove::Schema, foxglove::FoxgloveError> {
        let name = unsafe { self.name.as_utf8_str() }
            .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("schema name invalid: {e}")))?;
        let encoding = unsafe { self.encoding.as_utf8_str() }.map_err(|e| {
            foxglove::FoxgloveError::Utf8Error(format!("schema encoding invalid: {e}"))
        })?;
        let data = if self.data.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.data, self.data_len) }
        };
        Ok(foxglove::Schema::new(name, encoding, data.to_owned()))
    }
}

/// For use by the C++ SDK. Identifies that wrapper as the source of logs.
#[cfg(not(target_family = "wasm"))]
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_internal_register_cpp_wrapper() {
    foxglove::library_version::set_sdk_language("cpp");

    let log_config = std::env::var("FOXGLOVE_LOG_LEVEL")
        .or_else(|_| std::env::var("FOXGLOVE_LOG_STYLE"))
        .ok();
    if log_config.is_some() {
        foxglove_set_log_level(logging::FoxgloveLoggingLevel::Info);
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    DuplicateService,
    MissingRequestEncoding,
    ServicesNotSupported,
    ConnectionGraphNotSupported,
    IoError,
    McapError,
    EncodeError,
    BufferTooShort,
    Base64DecodeError,
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
            foxglove::FoxgloveError::EncodeError(_) => FoxgloveError::EncodeError,
            _ => FoxgloveError::Unspecified,
        }
    }
}

impl FoxgloveError {
    fn to_cstr(self) -> &'static std::ffi::CStr {
        match self {
            FoxgloveError::Ok => c"Ok",
            FoxgloveError::ValueError => c"Value Error",
            FoxgloveError::Utf8Error => c"UTF-8 Error",
            FoxgloveError::SinkClosed => c"Sink Closed",
            FoxgloveError::SchemaRequired => c"Schema Required",
            FoxgloveError::MessageEncodingRequired => c"Message Encoding Required",
            FoxgloveError::ServerAlreadyStarted => c"Server Already Started",
            FoxgloveError::Bind => c"Bind Error",
            FoxgloveError::DuplicateService => c"Duplicate Service",
            FoxgloveError::MissingRequestEncoding => c"Missing Request Encoding",
            FoxgloveError::ServicesNotSupported => c"Services Not Supported",
            FoxgloveError::ConnectionGraphNotSupported => c"Connection Graph Not Supported",
            FoxgloveError::IoError => c"IO Error",
            FoxgloveError::McapError => c"MCAP Error",
            FoxgloveError::EncodeError => c"Encode Error",
            FoxgloveError::BufferTooShort => c"Buffer too short",
            FoxgloveError::Base64DecodeError => c"Base64 decode error",
            FoxgloveError::Unspecified => c"Unspecified Error",
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub(crate) unsafe fn result_to_c<T>(
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

/// A timestamp, represented as an offset from a user-defined epoch.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FoxgloveTimestamp {
    /// Seconds since epoch.
    pub sec: u32,
    /// Additional nanoseconds since epoch.
    pub nsec: u32,
}

impl From<FoxgloveTimestamp> for foxglove::schemas::Timestamp {
    fn from(other: FoxgloveTimestamp) -> Self {
        Self::new(other.sec, other.nsec)
    }
}
/// A signed, fixed-length span of time.
///
/// The duration is represented by a count of seconds (which may be negative), and a count of
/// fractional seconds at nanosecond resolution (which are always positive).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FoxgloveDuration {
    /// Seconds offset.
    sec: i32,
    /// Nanoseconds offset in the positive direction.
    nsec: u32,
}

impl From<FoxgloveDuration> for foxglove::schemas::Duration {
    fn from(other: FoxgloveDuration) -> Self {
        Self::new(other.sec, other.nsec)
    }
}

#[repr(C)]
pub struct FoxglovePlayerState<'a> {
    /// Playback state
    pub playback_state: u8,
    /// Playback speed
    pub playback_speed: f32,
    /// Seek playback time in nanoseconds (only set if a seek has been performed)
    pub seek_time: Option<&'a u64>,
}
