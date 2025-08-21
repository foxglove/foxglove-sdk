use std::{collections::BTreeMap, mem::ManuallyDrop};

use base64::prelude::*;
use foxglove::websocket::{Parameter, ParameterType, ParameterValue};

use crate::bytes::FoxgloveBytes;
use crate::{FoxgloveError, FoxgloveString, FoxgloveStringBuf};

#[cfg(test)]
mod tests;

/// Pushes an element into a raw Vec<T>.
///
/// # Safety
/// The raw parts must be consistent.
unsafe fn raw_vec_push<T>(ptr: &mut *const T, len: &mut usize, cap: &mut usize, elem: T) {
    let mut vec = ManuallyDrop::new(unsafe { Vec::from_raw_parts((*ptr).cast_mut(), *len, *cap) });
    vec.push(elem);
    *ptr = vec.as_ptr();
    *len = vec.len();
    *cap = vec.capacity();
}

/// Clones a raw Vec<T> into a new Vec<T>.
///
/// # Safety
/// The raw parts must be consistent.
unsafe fn raw_vec_clone<T: Clone>(ptr: *const T, len: usize, cap: usize) -> Vec<T> {
    ManuallyDrop::new(unsafe { Vec::from_raw_parts(ptr.cast_mut(), len, cap) })
        .iter()
        .cloned()
        .collect()
}

/// An array of websocket parameters.
///
/// Constructed with `foxglove_parameter_array_create`.
//
// This struct serves as a surrogate reference to a `Vec<FoxgloveParameter>`.
#[repr(C)]
pub struct FoxgloveParameterArray {
    /// Pointer to array of parameters.
    parameters: *const FoxgloveParameter,
    /// Number of valid elements in the array.
    len: usize,
    /// Capacity of the array.
    capacity: usize,
}

impl FoxgloveParameterArray {
    /// Constructs a new parameter array from the provided vec.
    fn from_vec(vec: Vec<FoxgloveParameter>) -> Self {
        // SAFETY: Freed on drop.
        let vec = ManuallyDrop::new(vec);
        Self {
            parameters: vec.as_ptr(),
            len: vec.len(),
            capacity: vec.capacity(),
        }
    }

    /// Constructs an empty parameter array with the specified capacity.
    fn with_capacity(capacity: usize) -> Self {
        Self::from_vec(Vec::with_capacity(capacity))
    }

    /// Moves the parameter array to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover and free the array.
    pub(crate) fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed parameter array from a raw pointer.
    ///
    /// # Safety
    /// The raw pointer must have been obtained from [`Self::into_raw`].
    pub(crate) unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Converts the array into a rust-native representation.
    pub(crate) fn into_native(self) -> Vec<Parameter> {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let this = ManuallyDrop::new(self);
        // SAFETY: The raw parts are maintained correctly.
        let vec =
            unsafe { Vec::from_raw_parts(this.parameters.cast_mut(), this.len, this.capacity) };
        vec.into_iter()
            .map(FoxgloveParameter::into_native)
            .collect()
    }

    /// Pushes a parameter into the array.
    fn push(&mut self, param: FoxgloveParameter) {
        // SAFETY: The raw parts are maintained correctly.
        unsafe {
            raw_vec_push(
                &mut self.parameters,
                &mut self.len,
                &mut self.capacity,
                param,
            )
        };
    }
}

impl FromIterator<Parameter> for FoxgloveParameterArray {
    fn from_iter<I: IntoIterator<Item = Parameter>>(iter: I) -> Self {
        let vec = iter.into_iter().map(FoxgloveParameter::from).collect();
        Self::from_vec(vec)
    }
}

impl Drop for FoxgloveParameterArray {
    fn drop(&mut self) {
        // SAFETY: The raw parts are maintained correctly.
        let vec =
            unsafe { Vec::from_raw_parts(self.parameters.cast_mut(), self.len, self.capacity) };
        drop(vec)
    }
}

/// Creates a new parameter array with the specified capacity.
///
/// The array must be freed with `foxglove_parameter_array_free`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_parameter_array_create(capacity: usize) -> *mut FoxgloveParameterArray {
    FoxgloveParameterArray::with_capacity(capacity).into_raw()
}

/// Pushes a parameter into the array.
///
/// # Safety
/// - `array` must be a valid pointer to an array allocated by `foxglove_parameter_array_create`.
/// - `param` must be a valid parameter to a value allocated by `foxglove_parameter_create` or
///   `foxglove_parameter_clone`. This value is moved into this function, and must not be accessed
///   afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_array_push(
    array: Option<&mut FoxgloveParameterArray>,
    param: *mut FoxgloveParameter,
) -> FoxgloveError {
    if param.is_null() {
        return FoxgloveError::ValueError;
    }
    let param = unsafe { FoxgloveParameter::from_raw(param) };
    if let Some(array) = array {
        array.push(*param);
        FoxgloveError::Ok
    } else {
        FoxgloveError::ValueError
    }
}

/// Frees the parameter array and its contained parameters.
///
/// # Safety
/// - `array` must be a valid pointer to a value allocated by `foxglove_parameter_array_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_array_free(array: *mut FoxgloveParameterArray) {
    if !array.is_null() {
        drop(unsafe { FoxgloveParameterArray::from_raw(array) });
    }
}

/// A websocket parameter.
///
/// Constructed with `foxglove_parameter_create`.
#[repr(C)]
pub struct FoxgloveParameter {
    /// Parameter name.
    // This is wrapped in a ManuallyDrop so that we can move it out, even though we have a custom
    // Drop implementation.
    name: ManuallyDrop<FoxgloveStringBuf>,
    /// Parameter type.
    r#type: FoxgloveParameterType,
    /// Parameter value.
    // This field serves as a surrogate for Option<FoxgloveParameterValue>, using a null pointer to
    // represent `None`.
    value: *const FoxgloveParameterValue,
}

impl FoxgloveParameter {
    /// Creates a new parameter.
    fn new(
        name: impl Into<String>,
        r#type: FoxgloveParameterType,
        value: Option<Box<FoxgloveParameterValue>>,
    ) -> Self {
        let name = ManuallyDrop::new(FoxgloveStringBuf::new(name.into()));
        let value = value.map(Box::into_raw).unwrap_or_else(std::ptr::null_mut);
        Self {
            name,
            r#type,
            value,
        }
    }

    /// Moves the parameter array to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover and free the array.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed parameter from a raw pointer.
    ///
    /// # Safety
    /// The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Converts the parameter into a rust-native representation.
    fn into_native(self) -> Parameter {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let mut this = ManuallyDrop::new(self);
        // SAFETY: The name will not be accessed again.
        let name = unsafe { ManuallyDrop::take(&mut this.name) };
        let value = if this.value.is_null() {
            None
        } else {
            // SAFETY: The value pointer is valid.
            let value = unsafe { FoxgloveParameterValue::from_raw(this.value.cast_mut()) };
            Some(value.into_native())
        };
        Parameter {
            name: name.into(),
            r#type: this.r#type.to_native(),
            value,
        }
    }

    /// Returns a reference to the inner string of a byte array.
    ///
    /// Returns None if the parameter has no value, or the value is not a byte array.
    fn get_byte_array_as_str(&self) -> Option<&str> {
        // SAFETY: Param value pointer is either null or valid.
        let value = unsafe { self.value.as_ref() }?;
        if !matches!(
            (self.r#type, value.tag),
            (
                FoxgloveParameterType::ByteArray,
                FoxgloveParameterValueTag::String
            )
        ) {
            return None;
        }
        // SAFETY: Value was constructed by [`ParameterValue::string`], tag is a valid discriminator.
        Some(unsafe { &value.data.string }.as_str())
    }
}

impl From<Parameter> for FoxgloveParameter {
    fn from(value: Parameter) -> Self {
        FoxgloveParameter::new(
            value.name,
            value.r#type.into(),
            value.value.map(|v| Box::new(v.into())),
        )
    }
}

impl Clone for FoxgloveParameter {
    fn clone(&self) -> Self {
        let value = if self.value.is_null() {
            std::ptr::null_mut()
        } else {
            // SAFETY: The value pointer is valid.
            let value = ManuallyDrop::new(unsafe {
                FoxgloveParameterValue::from_raw(self.value.cast_mut())
            });
            Box::new((**value).clone()).into_raw()
        };
        Self {
            name: self.name.clone(),
            r#type: self.r#type,
            value,
        }
    }
}

impl Drop for FoxgloveParameter {
    fn drop(&mut self) {
        // SAFETY: The name will not be accessed again.
        let name = unsafe { ManuallyDrop::take(&mut self.name) };
        drop(name);
        if !self.value.is_null() {
            // SAFETY: The value pointer is valid.
            let value = unsafe { FoxgloveParameterValue::from_raw(self.value.cast_mut()) };
            drop(value);
        }
    }
}

/// Creates a new parameter.
///
/// The parameter must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
/// - `value` must either be a valid pointer to a value allocated by
///   `foxglove_parameter_value_create`, or NULL. This value is moved into this function, and must
///   not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    r#type: FoxgloveParameterType,
    value: *mut FoxgloveParameterValue,
) -> FoxgloveError {
    // Always consume the provided value.
    let value = if value.is_null() {
        None
    } else {
        Some(unsafe { FoxgloveParameterValue::from_raw(value) })
    };

    if param.is_null() {
        return FoxgloveError::ValueError;
    }

    // Ensure the name is UTF-8, and copy it to the heap.
    let name = unsafe { name.as_utf8_str() };
    let Ok(name) = name else {
        return FoxgloveError::Utf8Error;
    };
    let name = name.to_string();

    let this = FoxgloveParameter::new(name, r#type, value).into_raw();
    unsafe { *param = this };
    FoxgloveError::Ok
}

/// Creates a new empty parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_empty(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
) -> FoxgloveError {
    unsafe {
        foxglove_parameter_create(
            param,
            name,
            FoxgloveParameterType::None,
            std::ptr::null_mut(),
        )
    }
}

/// Creates a new number parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_float64(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    value: f64,
) -> FoxgloveError {
    let value = FoxgloveParameterValue::float64(value).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::Float64, value) }
}

/// Creates a new integer parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_integer(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    value: i64,
) -> FoxgloveError {
    let value = FoxgloveParameterValue::integer(value).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::None, value) }
}

/// Creates a new boolean parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_boolean(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    value: bool,
) -> FoxgloveError {
    let value = FoxgloveParameterValue::boolean(value).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::None, value) }
}

/// Creates a new string parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
/// - `value` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_string(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    value: FoxgloveString,
) -> FoxgloveError {
    let value = unsafe { value.as_utf8_str() };
    let Ok(value) = value else {
        return FoxgloveError::Utf8Error;
    };
    let value = FoxgloveParameterValue::string(value).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::None, value) }
}

/// Creates a new byte array parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
/// - `value` must be a valid `foxglove_bytes`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_byte_array(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    value: FoxgloveBytes,
) -> FoxgloveError {
    let value = unsafe { value.as_slice() };
    let value = BASE64_STANDARD.encode(value);
    let value = FoxgloveParameterValue::string(value).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::ByteArray, value) }
}

/// Creates a new parameter which is an array of float64 values.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
/// - `values` must be a valid pointer to an array of float64 values of `values_len` elements. This
///   value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_float64_array(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    values: *const f64,
    values_len: usize,
) -> FoxgloveError {
    let values = if values.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(values, values_len) }
    };
    let array = values
        .iter()
        .copied()
        .map(ParameterValue::Float64)
        .collect::<FoxgloveParameterValueArray>();
    let value = FoxgloveParameterValue::array(array).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::Float64Array, value) }
}

/// Creates a new parameter which is an array of integer values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_integer_array(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    values: *const i64,
    values_len: usize,
) -> FoxgloveError {
    let values = if values.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(values, values_len) }
    };
    let array = values
        .iter()
        .copied()
        .map(ParameterValue::Integer)
        .collect::<FoxgloveParameterValueArray>();
    let value = FoxgloveParameterValue::array(array).into_raw();
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::None, value) }
}

/// Creates a new parameter which is a dictionary of parameter values.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer.
/// - `name` must be a valid `foxglove_string`. This value is copied by this function.
/// - `dict` must be a valid pointer to a value allocated by
///   `foxglove_parameter_value_dict_create`. This value is moved into this function, and must not
///   be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_create_dict(
    param: *mut *mut FoxgloveParameter,
    name: FoxgloveString,
    dict: *mut FoxgloveParameterValueDict,
) -> FoxgloveError {
    let value = unsafe { foxglove_parameter_value_create_dict(dict) };
    unsafe { foxglove_parameter_create(param, name, FoxgloveParameterType::None, value) }
}

/// Returns an estimate of the decoded length for the byte array in bytes.
///
/// # Safety
/// - `param` must be a valid pointer to a value allocated by `foxglove_parameter_create` or
///   `foxglove_parameter_clone`.
/// - `size` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_get_byte_array_decoded_size(
    param: Option<&FoxgloveParameter>,
    len: *mut usize,
) -> FoxgloveError {
    let Some(param) = param else {
        return FoxgloveError::ValueError;
    };
    let Some(encoded) = param.get_byte_array_as_str() else {
        return FoxgloveError::ValueError;
    };
    let decoded_len = base64::decoded_len_estimate(encoded.len());
    unsafe { *len = decoded_len };
    FoxgloveError::Ok
}

/// Decodes a byte array into the provided buffer.
///
/// The buffer should be at least the size returned by
/// `foxglove_parameter_get_byte_array_decoded_size`.
///
/// On success, updates `len` with the number of bytes written to the provided buffer.
///
/// # Safety
/// - `param` must be a valid pointer to a value allocated by `foxglove_parameter_create` or
///   `foxglove_parameter_clone`.
/// - `data` must be a valid pointer to a writable buffer of size `len`.
/// - `len` must be a valid pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_decode_byte_array(
    param: Option<&FoxgloveParameter>,
    data: *mut u8,
    len: *mut usize,
) -> FoxgloveError {
    let Some(param) = param else {
        return FoxgloveError::ValueError;
    };
    let Some(encoded) = param.get_byte_array_as_str() else {
        return FoxgloveError::ValueError;
    };
    let capacity = unsafe { *len };
    if capacity < base64::decoded_len_estimate(encoded.len()) {
        return FoxgloveError::BufferTooShort;
    }
    let buffer = if data.is_null() {
        &mut []
    } else {
        unsafe { std::slice::from_raw_parts_mut(data, capacity) }
    };
    match BASE64_STANDARD.decode_slice_unchecked(encoded, buffer) {
        Ok(written) => {
            unsafe { *len = written };
            FoxgloveError::Ok
        }
        Err(e) => {
            tracing::warn!("Failed to decode base64: {e}");
            FoxgloveError::Base64DecodeError
        }
    }
}

/// Clones a parameter.
///
/// The value must be freed with `foxglove_parameter_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_array_push`.
///
/// # Safety
/// - `param` must be a valid pointer to a value allocated by `foxglove_parameter_create` or
///   `foxglove_parameter_clone`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_clone(
    param: Option<&FoxgloveParameter>,
) -> *mut FoxgloveParameter {
    if let Some(param) = param {
        param.clone().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// Frees a parameter.
///
/// # Safety
/// - `param` must be a valid pointer to a value allocated by `foxglove_parameter_create` or
///   `foxglove_parameter_clone`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_free(param: *mut FoxgloveParameter) {
    if param.is_null() {
        return;
    }
    let param = unsafe { FoxgloveParameter::from_raw(param) };
    drop(param);
}

/// A parameter type.
///
/// This enum is used to disambiguate `foxglove_parameter` values, in situations where the wire
/// representation is ambiguous.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FoxgloveParameterType {
    /// The parameter value can be inferred from the inner parameter value tag.
    None,
    /// An array of bytes.
    ByteArray,
    /// A decimal or integer value that can be represented as a `float64`.
    Float64,
    /// An array of decimal or integer values that can be represented as `float64`s.
    Float64Array,
}

impl From<ParameterType> for FoxgloveParameterType {
    fn from(value: ParameterType) -> Self {
        match value {
            ParameterType::ByteArray => Self::ByteArray,
            ParameterType::Float64 => Self::Float64,
            ParameterType::Float64Array => Self::Float64Array,
        }
    }
}

impl From<Option<ParameterType>> for FoxgloveParameterType {
    fn from(value: Option<ParameterType>) -> Self {
        value.map(Self::from).unwrap_or(Self::None)
    }
}

impl FoxgloveParameterType {
    fn to_native(self) -> Option<ParameterType> {
        match self {
            Self::None => None,
            Self::ByteArray => Some(ParameterType::ByteArray),
            Self::Float64 => Some(ParameterType::Float64),
            Self::Float64Array => Some(ParameterType::Float64Array),
        }
    }
}

/// A websocket parameter value.
///
/// Constructed with `foxglove_parameter_value_create_*`.
#[repr(C)]
pub struct FoxgloveParameterValue {
    /// A variant discriminator for the `data` union.
    tag: FoxgloveParameterValueTag,
    /// Storage for the value's data.
    data: FoxgloveParameterValueData,
}

/// A variant discriminator for `FoxgloveParameterValueData`.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FoxgloveParameterValueTag {
    Float64,
    Integer,
    Boolean,
    String,
    Array,
    Dict,
}

/// Storage for `FoxgloveParameterValue`.
#[repr(C)]
pub union FoxgloveParameterValueData {
    float64: f64,
    integer: i64,
    boolean: bool,
    string: ManuallyDrop<FoxgloveStringBuf>,
    array: ManuallyDrop<FoxgloveParameterValueArray>,
    dict: ManuallyDrop<FoxgloveParameterValueDict>,
}

impl FoxgloveParameterValue {
    fn new(tag: FoxgloveParameterValueTag, data: FoxgloveParameterValueData) -> Self {
        Self { tag, data }
    }

    /// Constructs a new floating-point value.
    fn float64(number: f64) -> Self {
        Self::new(
            FoxgloveParameterValueTag::Float64,
            FoxgloveParameterValueData { float64: number },
        )
    }

    /// Constructs a new integer value.
    fn integer(integer: i64) -> Self {
        Self::new(
            FoxgloveParameterValueTag::Integer,
            FoxgloveParameterValueData { integer },
        )
    }

    /// Constructs a new boolean value.
    fn boolean(boolean: bool) -> Self {
        Self::new(
            FoxgloveParameterValueTag::Boolean,
            FoxgloveParameterValueData { boolean },
        )
    }

    /// Constructs a new "string" (actually byte array) value.
    fn string(string: impl Into<String>) -> Self {
        // SAFETY: Freed on drop.
        let string = ManuallyDrop::new(FoxgloveStringBuf::new(string.into()));
        Self::new(
            FoxgloveParameterValueTag::String,
            FoxgloveParameterValueData { string },
        )
    }

    /// Constructs a new array value.
    fn array(array: FoxgloveParameterValueArray) -> Self {
        // SAFETY: Freed on drop.
        let array = ManuallyDrop::new(array);
        Self::new(
            FoxgloveParameterValueTag::Array,
            FoxgloveParameterValueData { array },
        )
    }

    /// Constructs a new dictionary value.
    fn dict(dict: FoxgloveParameterValueDict) -> Self {
        // SAFETY: Freed on drop.
        let dict = ManuallyDrop::new(dict);
        Self::new(
            FoxgloveParameterValueTag::Dict,
            FoxgloveParameterValueData { dict },
        )
    }

    /// Moves the parameter value to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover and free the array.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// # Safety
    /// The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Consumes the value, converting it into its native form.
    fn into_native(self) -> ParameterValue {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let mut this = ManuallyDrop::new(self);
        match this.tag {
            FoxgloveParameterValueTag::Float64 => {
                // SAFETY: Constructed by [`Self::float64`].
                ParameterValue::Float64(unsafe { this.data.float64 })
            }
            FoxgloveParameterValueTag::Integer => {
                // SAFETY: Constructed by [`Self::integer`].
                ParameterValue::Integer(unsafe { this.data.integer })
            }
            FoxgloveParameterValueTag::Boolean => {
                // SAFETY: Constructed by [`Self::boolean`].
                ParameterValue::Bool(unsafe { this.data.boolean })
            }
            FoxgloveParameterValueTag::String => {
                // SAFETY: Constructed by [`Self::string`].
                let string = unsafe { &mut this.data.string };
                // SAFETY: The string will not be accessed again.
                let string = unsafe { ManuallyDrop::take(string) };
                ParameterValue::String(string.into())
            }
            FoxgloveParameterValueTag::Array => {
                // SAFETY: Constructed by [`Self::array`].
                let array = unsafe { &mut this.data.array };
                // SAFETY: The array will not be accessed again.
                let array = unsafe { ManuallyDrop::take(array) };
                let vec = array.into_native();
                ParameterValue::Array(vec)
            }
            FoxgloveParameterValueTag::Dict => {
                // SAFETY: Constructed by [`Self::dict`].
                let dict = unsafe { &mut this.data.dict };
                // SAFETY: The dict will not be accessed again.
                let dict = unsafe { ManuallyDrop::take(dict) };
                let map = dict.into_native();
                ParameterValue::Dict(map)
            }
        }
    }
}

impl From<ParameterValue> for FoxgloveParameterValue {
    fn from(value: ParameterValue) -> Self {
        match value {
            ParameterValue::Float64(v) => Self::float64(v),
            ParameterValue::Integer(v) => Self::integer(v),
            ParameterValue::Bool(v) => Self::boolean(v),
            ParameterValue::String(v) => Self::string(v),
            ParameterValue::Array(v) => Self::array(v.into_iter().collect()),
            ParameterValue::Dict(v) => Self::dict(v.into_iter().collect()),
        }
    }
}

impl Clone for FoxgloveParameterValue {
    fn clone(&self) -> Self {
        match self.tag {
            FoxgloveParameterValueTag::Float64 => {
                // SAFETY: Constructed by [`Self::float64`].
                Self::float64(unsafe { self.data.float64 })
            }
            FoxgloveParameterValueTag::Integer => {
                // SAFETY: Constructed by [`Self::integer`].
                Self::integer(unsafe { self.data.integer })
            }
            FoxgloveParameterValueTag::Boolean => {
                // SAFETY: Constructed by [`Self::boolean`].
                Self::boolean(unsafe { self.data.boolean })
            }
            FoxgloveParameterValueTag::String => {
                // SAFETY: Constructed by [`Self::string`].
                let string = unsafe { &self.data.string };
                Self::string(string.as_str())
            }
            FoxgloveParameterValueTag::Array => {
                // SAFETY: Constructed by [`Self::array`].
                let array = unsafe { &self.data.array };
                Self::array((**array).clone())
            }
            FoxgloveParameterValueTag::Dict => {
                // SAFETY: Constructed by [`Self::dict`].
                let dict = unsafe { &self.data.dict };
                Self::dict((**dict).clone())
            }
        }
    }
}

impl Drop for FoxgloveParameterValue {
    fn drop(&mut self) {
        match self.tag {
            FoxgloveParameterValueTag::Float64
            | FoxgloveParameterValueTag::Integer
            | FoxgloveParameterValueTag::Boolean => (),
            FoxgloveParameterValueTag::String => {
                // SAFETY: Constructed by [`Self::string`].
                let string = unsafe { &mut self.data.string };
                // SAFETY: The string will not be accessed again.
                let string = unsafe { ManuallyDrop::take(string) };
                drop(string);
            }
            FoxgloveParameterValueTag::Array => {
                // SAFETY: Constructed by [`Self::array`].
                let array = unsafe { &mut self.data.array };
                // SAFETY: The array will not be accessed again.
                let array = unsafe { ManuallyDrop::take(array) };
                drop(array);
            }
            FoxgloveParameterValueTag::Dict => {
                // SAFETY: Constructed by [`Self::dict`].
                let dict = unsafe { &mut self.data.dict };
                // SAFETY: The dict will not be accessed again.
                let dict = unsafe { ManuallyDrop::take(dict) };
                drop(dict);
            }
        }
    }
}

/// Creates a new float64 parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_parameter_value_create_float64(
    number: f64,
) -> *mut FoxgloveParameterValue {
    FoxgloveParameterValue::float64(number).into_raw()
}

/// Creates a new integer parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_create_integer(
    integer: i64,
) -> *mut FoxgloveParameterValue {
    FoxgloveParameterValue::integer(integer).into_raw()
}

/// Creates a new boolean parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_create_boolean(
    boolean: bool,
) -> *mut FoxgloveParameterValue {
    FoxgloveParameterValue::boolean(boolean).into_raw()
}

/// Creates a new string parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
///
/// # Safety
/// - `string` must be a valid `foxglove_string`. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_create_string(
    value: *mut *mut FoxgloveParameterValue,
    string: FoxgloveString,
) -> FoxgloveError {
    if value.is_null() {
        return FoxgloveError::ValueError;
    }
    let string = unsafe { string.as_utf8_str() };
    let Ok(string) = string else {
        return FoxgloveError::Utf8Error;
    };
    let ptr = FoxgloveParameterValue::string(string).into_raw();
    unsafe { *value = ptr };
    FoxgloveError::Ok
}

/// Creates a new array parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
///
/// # Safety
/// - `array` must be a valid pointer to a value allocated by
///   `foxglove_parameter_value_array_create`. This value is moved into this function, and must not
///   be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_create_array(
    array: *mut FoxgloveParameterValueArray,
) -> *mut FoxgloveParameterValue {
    let array = unsafe { FoxgloveParameterValueArray::from_raw(array) };
    FoxgloveParameterValue::array(*array).into_raw()
}

/// Creates a new dict parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
///
/// # Safety
/// - `dict` must be a valid pointer to a value allocated by
///   `foxglove_parameter_value_dict_create`. This value is moved into this function, and must not be
///   accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_create_dict(
    dict: *mut FoxgloveParameterValueDict,
) -> *mut FoxgloveParameterValue {
    let dict = unsafe { FoxgloveParameterValueDict::from_raw(dict) };
    FoxgloveParameterValue::dict(*dict).into_raw()
}

/// Clones a parameter value.
///
/// The value must be freed with `foxglove_parameter_value_free`, or by passing it to a consuming
/// function such as `foxglove_parameter_create`.
///
/// # Safety
/// - `value` must be a valid pointer to a value allocated by `foxglove_parameter_value_create` or
///   `foxglove_parameter_value_clone`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_clone(
    value: Option<&FoxgloveParameterValue>,
) -> *mut FoxgloveParameterValue {
    if let Some(value) = value {
        value.clone().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

/// Frees a parameter value.
///
/// # Safety
/// - `value` must be a valid pointer to a value allocated by `foxglove_parameter_value_create_*`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_free(value: *mut FoxgloveParameterValue) {
    if !value.is_null() {
        let value = unsafe { FoxgloveParameterValue::from_raw(value) };
        drop(value);
    }
}

/// An array of parameter values.
///
/// Constructed with `foxglove_parameter_value_array_create`.
//
// This struct serves as a surrogate reference to a `Vec<FoxgloveParameterValue>`.
#[repr(C)]
pub struct FoxgloveParameterValueArray {
    /// A pointer to the array of parameter values.
    values: *const FoxgloveParameterValue,
    /// Number of elements in the array.
    len: usize,
    /// Capacity of the array.
    capacity: usize,
}

impl FoxgloveParameterValueArray {
    /// Constructs a new parameter array from the provided vec.
    fn from_vec(vec: Vec<FoxgloveParameterValue>) -> Self {
        // SAFETY: Freed on drop.
        let vec = ManuallyDrop::new(vec);
        Self {
            values: vec.as_ptr(),
            len: vec.len(),
            capacity: vec.capacity(),
        }
    }

    /// Constructs an empty parameter value array with the specified capacity.
    fn with_capacity(capacity: usize) -> Self {
        Self::from_vec(Vec::with_capacity(capacity))
    }

    /// Moves the parameter array to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover and free the array.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed parameter value array from a raw pointer.
    ///
    /// # Safety
    /// The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Converts the value array into a rust-native representation.
    fn into_native(self) -> Vec<ParameterValue> {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let this = ManuallyDrop::new(self);
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { Vec::from_raw_parts(this.values.cast_mut(), this.len, this.len) };
        vec.into_iter()
            .map(FoxgloveParameterValue::into_native)
            .collect()
    }

    /// Pushes a value into the array.
    fn push(&mut self, value: FoxgloveParameterValue) {
        // SAFETY: The raw parts are maintained correctly.
        unsafe { raw_vec_push(&mut self.values, &mut self.len, &mut self.capacity, value) };
    }
}

impl FromIterator<ParameterValue> for FoxgloveParameterValueArray {
    fn from_iter<I: IntoIterator<Item = ParameterValue>>(iter: I) -> Self {
        let vec = iter.into_iter().map(FoxgloveParameterValue::from).collect();
        Self::from_vec(vec)
    }
}

impl Clone for FoxgloveParameterValueArray {
    fn clone(&self) -> Self {
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { raw_vec_clone(self.values, self.len, self.capacity) };
        Self::from_vec(vec)
    }
}

impl Drop for FoxgloveParameterValueArray {
    fn drop(&mut self) {
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { Vec::from_raw_parts(self.values.cast_mut(), self.len, self.capacity) };
        drop(vec)
    }
}

/// Creates a new value array with the specified capacity.
///
/// The parameter must be freed with `foxglove_parameter_value_array_free`, or by passing it to a
/// consuming function such as `foxglove_parameter_value_create_array`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_parameter_value_array_create(
    capacity: usize,
) -> *mut FoxgloveParameterValueArray {
    FoxgloveParameterValueArray::with_capacity(capacity).into_raw()
}

/// Pushes a parameter value into the array.
///
/// # Safety
/// - `array` must be a valid pointer to an array allocated by
///   `foxglove_parameter_value_array_create`.
/// - `value` must be a valid pointer to a value allocated by `foxglove_parameter_value_create_*`.
///   This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_array_push(
    array: Option<&mut FoxgloveParameterValueArray>,
    value: *mut FoxgloveParameterValue,
) -> FoxgloveError {
    if value.is_null() {
        return FoxgloveError::ValueError;
    }
    let value = unsafe { FoxgloveParameterValue::from_raw(value) };
    if let Some(array) = array {
        array.push(*value);
        FoxgloveError::Ok
    } else {
        FoxgloveError::ValueError
    }
}

/// Frees a parameter value array.
///
/// # Safety
/// - `array` is a valid pointer to a value allocated by `foxglove_parameter_value_array_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_array_free(
    array: *mut FoxgloveParameterValueArray,
) {
    if !array.is_null() {
        drop(unsafe { FoxgloveParameterValueArray::from_raw(array) });
    }
}

/// An dictionary of parameter values.
///
/// Constructed with `foxglove_parameter_value_dict_create`.
//
// This struct serves as a surrogate reference to a `Vec<FoxgloveParameterValueDictEntry>`.
#[repr(C)]
pub struct FoxgloveParameterValueDict {
    /// A pointer to the array of dictionary entries.
    entries: *const FoxgloveParameterValueDictEntry,
    /// Number of elements in the dictionary.
    len: usize,
    /// Capacity of the dictionary.
    capacity: usize,
}

impl FoxgloveParameterValueDict {
    /// Constructs a new dict from the provided vec.
    fn from_vec(vec: Vec<FoxgloveParameterValueDictEntry>) -> Self {
        // SAFETY: Freed on drop.
        let vec = ManuallyDrop::new(vec);
        Self {
            entries: vec.as_ptr(),
            len: vec.len(),
            capacity: vec.capacity(),
        }
    }

    /// Constructs a new parameter value dict.
    fn with_capacity(capacity: usize) -> Self {
        Self::from_vec(Vec::with_capacity(capacity))
    }

    /// Moves the dict to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover and free the dict.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed dict from a raw pointer.
    ///
    /// # Safety
    /// The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Converts the array into a rust-native representation.
    fn into_native(self) -> BTreeMap<String, ParameterValue> {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let this = ManuallyDrop::new(self);
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { Vec::from_raw_parts(this.entries.cast_mut(), this.len, this.capacity) };
        vec.into_iter()
            .map(FoxgloveParameterValueDictEntry::into_native)
            .collect()
    }

    /// Inserts a value into the dict.
    fn push(&mut self, entry: FoxgloveParameterValueDictEntry) {
        // SAFETY: The raw parts are maintained correctly.
        unsafe { raw_vec_push(&mut self.entries, &mut self.len, &mut self.capacity, entry) };
    }
}

impl FromIterator<(String, ParameterValue)> for FoxgloveParameterValueDict {
    fn from_iter<I: IntoIterator<Item = (String, ParameterValue)>>(iter: I) -> Self {
        let vec = iter
            .into_iter()
            .map(FoxgloveParameterValueDictEntry::from)
            .collect();
        Self::from_vec(vec)
    }
}

impl Clone for FoxgloveParameterValueDict {
    fn clone(&self) -> Self {
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { raw_vec_clone(self.entries, self.len, self.capacity) };
        Self::from_vec(vec)
    }
}

impl Drop for FoxgloveParameterValueDict {
    fn drop(&mut self) {
        // SAFETY: The raw parts are maintained correctly.
        let vec = unsafe { Vec::from_raw_parts(self.entries.cast_mut(), self.len, self.capacity) };
        drop(vec)
    }
}

/// Creates a new value dict with the specified capacity.
///
/// The parameter must be freed with `foxglove_parameter_value_dict_free`, or by passing it to a
/// consuming function such as `foxglove_parameter_value_create_dict`.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_parameter_value_dict_create(
    capacity: usize,
) -> *mut FoxgloveParameterValueDict {
    FoxgloveParameterValueDict::with_capacity(capacity).into_raw()
}

/// Inserts an entry into the parameter value dict.
///
/// # Safety
/// - `key` must be a valid `foxglove_string`. This value is copied by this function.
/// - `value` must be a valid pointer to a value allocated by `foxglove_parameter_value_create_*`.
///   This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_dict_insert(
    dict: Option<&mut FoxgloveParameterValueDict>,
    key: FoxgloveString,
    value: *mut FoxgloveParameterValue,
) -> FoxgloveError {
    if value.is_null() {
        return FoxgloveError::ValueError;
    }
    let value = unsafe { FoxgloveParameterValue::from_raw(value) };

    let Some(dict) = dict else {
        return FoxgloveError::ValueError;
    };

    let key = unsafe { key.as_utf8_str() };
    let Ok(key) = key else {
        return FoxgloveError::Utf8Error;
    };

    let entry = FoxgloveParameterValueDictEntry::new(key, value);
    dict.push(entry);
    FoxgloveError::Ok
}

/// Frees a parameter value dict.
///
/// # Safety
/// - `dict` is a valid pointer to a value allocated by `foxglove_parameter_value_dict_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_parameter_value_dict_free(dict: *mut FoxgloveParameterValueDict) {
    if !dict.is_null() {
        drop(unsafe { FoxgloveParameterValueDict::from_raw(dict) });
    }
}

/// An dictionary entry for a parameter value.
///
/// Constructed implicitly with `foxglove_parameter_value_dict_insert`.
#[repr(C)]
pub struct FoxgloveParameterValueDictEntry {
    /// The dictionary entry's key.
    // This is wrapped in a ManuallyDrop so that we can move it out, even though we have a custom
    // Drop implementation.
    key: ManuallyDrop<FoxgloveStringBuf>,
    /// The dictionary entry's value.
    value: *const FoxgloveParameterValue,
}

impl FoxgloveParameterValueDictEntry {
    fn new(key: impl Into<String>, value: Box<FoxgloveParameterValue>) -> Self {
        Self {
            key: ManuallyDrop::new(FoxgloveStringBuf::new(key.into())),
            value: Box::into_raw(value),
        }
    }

    fn into_native(self) -> (String, ParameterValue) {
        // SAFETY: We're consuming the underlying values, so don't drop self.
        let mut this = ManuallyDrop::new(self);
        // SAFETY: The key will not be accessed again.
        let key = unsafe { ManuallyDrop::take(&mut this.key) };
        // SAFETY: The value must be a valid pointer.
        let value = unsafe { FoxgloveParameterValue::from_raw(this.value.cast_mut()) };
        let value = value.into_native();
        (key.into(), value)
    }
}

impl From<(String, ParameterValue)> for FoxgloveParameterValueDictEntry {
    fn from(value: (String, ParameterValue)) -> Self {
        Self::new(value.0, Box::new(value.1.into()))
    }
}

impl Clone for FoxgloveParameterValueDictEntry {
    fn clone(&self) -> Self {
        // SAFETY: The value must be a valid pointer.
        let value =
            ManuallyDrop::new(unsafe { FoxgloveParameterValue::from_raw(self.value.cast_mut()) });
        Self::new(self.key.as_str(), Box::new((**value).clone()))
    }
}

impl Drop for FoxgloveParameterValueDictEntry {
    fn drop(&mut self) {
        // SAFETY: The key will not be accessed again.
        let key = unsafe { ManuallyDrop::take(&mut self.key) };
        drop(key);
        // SAFETY: The value must be a valid pointer.
        let value = unsafe { FoxgloveParameterValue::from_raw(self.value.cast_mut()) };
        drop(value);
    }
}
