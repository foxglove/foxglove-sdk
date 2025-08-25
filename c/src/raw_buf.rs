use foxglove::bytes::BufMut;

/// A wrapper for a raw (ptr, len) pair passed in from calling C/C++ code that implements BufMut.
pub(crate) struct RawBuf {
    pub ptr: *mut u8,
    pub len: usize,
    pub pos: usize,
}

unsafe impl BufMut for RawBuf {
    fn remaining_mut(&self) -> usize {
        assert!(self.pos < self.len);
        self.len - self.pos
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.pos += cnt;
    }

    fn chunk_mut(&mut self) -> &mut foxglove::bytes::buf::UninitSlice {
        assert!(self.pos < self.len);
        unsafe {
            foxglove::bytes::buf::UninitSlice::from_raw_parts_mut(
                self.ptr.add(self.pos),
                self.remaining_mut(),
            )
        }
    }
}
