use std::cell::Cell;
use std::marker::PhantomPinned;
use std::mem::ManuallyDrop;
use std::mem::{align_of, size_of, MaybeUninit};
use std::pin::Pin;
use std::ptr;

use foxglove::FoxgloveError;

/// A trait for converting a Foxglove C type to its native Rust () representation
pub trait BorrowToNative {
    type NativeType;

    unsafe fn borrow_to_native(
        &self,
        arena: Pin<&mut Arena>,
    ) -> Result<ManuallyDrop<Self::NativeType>, FoxgloveError>;
}

/// A fixed-size memory arena that allocates aligned arrays of POD types.
/// The arena contains a single inline array and allocates from it.
/// If the arena runs out of space, it returns an OutOfMemory error.
/// The allocated memory is "freed" by dropping the arena, destructors are not run.
pub struct Arena {
    buffer: [MaybeUninit<u8>; Arena::SIZE],
    offset: Cell<usize>,
    // Marker to prevent moving
    _pin: PhantomPinned,
}

impl Arena {
    pub const SIZE: usize = 512 * 1024; // 512 KB

    /// Creates a new empty Arena
    ///
    /// Example usage:
    /// ```
    /// let mut arena_pin = std::pin::pin!(Arena::new());
    /// let arena = arena_pin.as_mut();
    /// // use arena map or map_one methods
    /// ```
    pub const fn new() -> Self {
        Self {
            buffer: [MaybeUninit::uninit(); Self::SIZE],
            offset: Cell::new(0),
            _pin: PhantomPinned,
        }
    }

    /// Allocates an array of `n` elements of type `T` from the arena.
    fn alloc<T>(&self, n: usize) -> *mut T {
        let element_size = size_of::<T>();
        let bytes_needed = n * element_size;

        // Calculate aligned offset
        let aligned_offset = self.offset.get().next_multiple_of(align_of::<T>());

        // Check if we have enough space
        if aligned_offset + bytes_needed > Self::SIZE {
            panic!("Arena out of memory");
        }

        // SAFETY: [result, result+n) is properly aligned and within the bounds of buffer
        let result = unsafe { self.buffer.as_ptr().add(aligned_offset) as *mut T };
        self.offset.set(aligned_offset + bytes_needed);
        result
    }

    /// Maps elements from a slice to a new array allocated from the arena.
    pub unsafe fn map<S: BorrowToNative>(
        mut self: Pin<&mut Self>,
        src: *const S,
        len: usize,
    ) -> Result<ManuallyDrop<Vec<S::NativeType>>, FoxgloveError> {
        if len == 0 {
            return Ok(ManuallyDrop::new(Vec::new()));
        }

        let result = self.as_mut().alloc::<S::NativeType>(len);

        // Convert the elements from S to T, placing them in the result array
        for i in 0..len {
            unsafe {
                let tmp = (&*src.add(i)).borrow_to_native(self.as_mut())?;
                *(result.add(i) as *mut _) = tmp;
            }
        }

        unsafe { Ok(ManuallyDrop::new(Vec::from_raw_parts(result, len, len))) }
    }

    /// Returns how many bytes are currently used in the arena.
    #[allow(dead_code)]
    pub fn used(&self) -> usize {
        self.offset.get()
    }

    /// Returns how many bytes are available in the arena.
    #[allow(dead_code)]
    pub fn available(&self) -> usize {
        Self::SIZE - self.offset.get()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}
