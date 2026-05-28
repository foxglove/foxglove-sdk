use foxglove::FoxgloveError;
use foxglove::bytes::Bytes;
use std::mem::ManuallyDrop;

#[cfg(not(target_family = "wasm"))]
use std::collections::HashMap;

#[cfg(not(target_family = "wasm"))]
use crate::FoxgloveKeyValue;

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

/// Parse a C array of [`FoxgloveKeyValue`] into a `HashMap<String, String>`.
///
/// # Safety
///
/// If `count > 0`, `ptr` must be a valid pointer to `count` initialized elements. Each key
/// and value must contain valid UTF-8.
#[cfg(not(target_family = "wasm"))]
pub(crate) unsafe fn parse_key_value_array(
    ptr: *const FoxgloveKeyValue,
    count: usize,
    field_name: &str,
) -> Result<HashMap<String, String>, FoxgloveError> {
    if ptr.is_null() {
        return Err(FoxgloveError::ValueError(format!("{field_name} is null")));
    }
    let mut map = HashMap::with_capacity(count);
    for i in 0..count {
        let kv = unsafe { &*ptr.add(i) };
        if kv.key.data.is_null() || kv.value.data.is_null() {
            return Err(FoxgloveError::ValueError(format!(
                "null key or value in {field_name}"
            )));
        }
        let key = unsafe { kv.key.as_utf8_str() }?;
        let value = unsafe { kv.value.as_utf8_str() }?;
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}
