use foxglove::FoxgloveError;
use foxglove::bytes::Bytes;
use std::mem::ManuallyDrop;

/// Create a borrowed Bytes from a raw pointer and length.
///
/// # Safety
///
/// If len > 0, the pointer must be a valid pointer to len bytes.
///
/// The returned Bytes has a static lifetime, but only lives as long as the input data.
/// It's up to the programmer to manage the lifetimes.
pub(crate) unsafe fn bytes_from_raw(ptr: *const u8, len: usize) -> ManuallyDrop<Bytes> {
    if len == 0 {
        return ManuallyDrop::new(Bytes::new());
    }
    unsafe { ManuallyDrop::new(Bytes::from_static(std::slice::from_raw_parts(ptr, len))) }
}

/// Create a borrowed String from a raw pointer and length.
///
/// # Safety
///
/// If len > 0, the pointer must be a valid pointer to len bytes.
///
/// The returned String must never be dropped as a String.
pub(crate) unsafe fn string_from_raw(
    ptr: *const u8,
    len: usize,
    field_name: &str,
) -> Result<ManuallyDrop<String>, FoxgloveError> {
    if len == 0 {
        return Ok(ManuallyDrop::new(String::new()));
    }
    unsafe {
        String::from_utf8(Vec::from_raw_parts(ptr as *mut _, len, len))
            .map(ManuallyDrop::new)
            .map_err(|e| FoxgloveError::Utf8Error(format!("{field_name} invalid: {e}")))
    }
}

/// Create a borrowed vec from a raw pointer and length.
///
/// # Safety
///
/// If len > 0, the pointer must be a valid pointer to len elements.
///
/// The returned Vec must never be dropped as a Vec.
pub(crate) unsafe fn vec_from_raw<T>(ptr: *const T, len: usize) -> ManuallyDrop<Vec<T>> {
    if len == 0 {
        return ManuallyDrop::new(Vec::new());
    }
    unsafe { ManuallyDrop::new(Vec::from_raw_parts(ptr as *mut _, len, len)) }
}
