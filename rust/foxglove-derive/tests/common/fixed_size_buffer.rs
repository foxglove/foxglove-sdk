use bytes::{buf::UninitSlice, BufMut};

/// For testing only: an implementation of BufMut that will not grow beyond a fixed size
pub(crate) struct FixedSizeBuffer {
    data: Box<[u8]>,
    len: usize,
}

impl FixedSizeBuffer {
    pub(crate) fn with_capacity(cap: usize) -> Self {
        let data = vec![0; cap];
        Self {
            data: data.into_boxed_slice(),
            len: 0,
        }
    }
}

unsafe impl BufMut for FixedSizeBuffer {
    fn remaining_mut(&self) -> usize {
        self.data.len() - self.len
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        UninitSlice::new(&mut self.data[self.len..])
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(cnt <= self.remaining_mut(), "Buffer overflow");
        self.len += cnt;
    }
}
