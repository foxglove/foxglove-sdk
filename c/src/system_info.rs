//! C FFI bindings for [`foxglove::system_info::SystemInfoPublisher`].

use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::time::Duration;

use foxglove::system_info::{SystemInfoHandle, SystemInfoPublisher};

use crate::{FoxgloveContext, FoxgloveError, FoxgloveString, result_to_c};

/// Opaque handle to a running system info publisher.
///
/// The handle is created by [`foxglove_system_info_publisher_start`] and freed by
/// [`foxglove_system_info_publisher_stop`].
pub struct FoxgloveSystemInfoPublisher(SystemInfoHandle);

/// Options for [`foxglove_system_info_publisher_start`].
///
/// All fields are optional. To use the default for any field, leave the field
/// zero-initialized (e.g. by setting it via `memset` or `= {0}`).
///
/// # Safety
/// - `context`, when non-null, must be a valid pointer to a context created via
///   `foxglove_context_new`.
/// - `topic`, when non-empty, must be a valid UTF-8 string.
#[repr(C)]
pub struct FoxgloveSystemInfoPublisherOptions<'a> {
    /// Optional context to publish on. When null, the default global context is used.
    pub context: *const FoxgloveContext,

    /// Optional channel topic name. If `data` is null or `len` is 0, defaults to `/sysinfo`.
    pub topic: FoxgloveString,

    /// Optional refresh interval, in milliseconds.
    ///
    /// When null or zero, defaults to 1000 ms (1 second). Clamped to a minimum of 200 ms.
    pub refresh_interval_ms: Option<&'a u64>,
}

/// Start the system info publisher.
///
/// On success, writes a non-null handle to `out_publisher`. On failure, returns an error
/// code and `out_publisher` is left untouched.
///
/// The returned handle must be freed by calling [`foxglove_system_info_publisher_stop`].
///
/// # Safety
/// - `options` must be a valid pointer to a [`FoxgloveSystemInfoPublisherOptions`] struct.
/// - `out_publisher` must be a valid, writable pointer to a `*mut FoxgloveSystemInfoPublisher`.
/// - See the safety notes on [`FoxgloveSystemInfoPublisherOptions`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_system_info_publisher_start(
    options: Option<&FoxgloveSystemInfoPublisherOptions>,
    out_publisher: *mut *mut FoxgloveSystemInfoPublisher,
) -> FoxgloveError {
    let result = unsafe { do_start(options) };
    unsafe { result_to_c(result, out_publisher) }
}

unsafe fn do_start(
    options: Option<&FoxgloveSystemInfoPublisherOptions>,
) -> Result<*mut FoxgloveSystemInfoPublisher, foxglove::FoxgloveError> {
    let Some(options) = options else {
        return Err(foxglove::FoxgloveError::ValueError(
            "options must not be null".to_string(),
        ));
    };

    let mut builder = SystemInfoPublisher::new();

    if !options.context.is_null() {
        let ctx = ManuallyDrop::new(unsafe { Arc::from_raw(options.context) });
        builder = builder.context(&ctx);
    }

    let topic = unsafe { options.topic.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic invalid: {e}")))?;
    if !topic.is_empty() {
        builder = builder.topic(topic);
    }

    if let Some(&refresh_ms) = options.refresh_interval_ms
        && refresh_ms > 0
    {
        builder = builder.refresh_interval(Duration::from_millis(refresh_ms));
    }

    let handle = builder.start();
    Ok(Box::into_raw(Box::new(FoxgloveSystemInfoPublisher(handle))))
}

/// Stop the system info publisher and free its resources.
///
/// This aborts the background task. After calling this function, the handle is invalid
/// and must not be used again. Passing a null pointer is a no-op.
///
/// # Safety
/// - `publisher`, when non-null, must be a handle returned by
///   [`foxglove_system_info_publisher_start`] that has not already been passed to this
///   function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_system_info_publisher_stop(
    publisher: *mut FoxgloveSystemInfoPublisher,
) -> FoxgloveError {
    if publisher.is_null() {
        return FoxgloveError::Ok;
    }
    let publisher = unsafe { Box::from_raw(publisher) };
    publisher.0.abort();
    FoxgloveError::Ok
}
