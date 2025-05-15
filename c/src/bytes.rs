/// A byte array with associated length.
#[repr(C)]
pub struct FoxgloveBytes {
    /// Pointer to data
    data: *const u8,
    /// Number of bytes
    len: usize,
}
impl FoxgloveBytes {
    /// Access the buffer as a slice.
    ///
    /// # Safety
    ///
    /// The `data` field must be a valid pointer to a buffer of length `len`.
    pub(crate) unsafe fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data, self.len) }
    }

    #[cfg(test)]
    pub(crate) fn from_slice(data: &[u8]) -> Self {
        data.into()
    }
}
impl From<&[u8]> for FoxgloveBytes {
    fn from(value: &[u8]) -> Self {
        Self {
            data: value.as_ptr(),
            len: value.len(),
        }
    }
}
