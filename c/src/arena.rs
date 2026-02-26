use std::alloc::Layout;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::marker::PhantomPinned;
use std::mem::ManuallyDrop;
use std::mem::{MaybeUninit, align_of, size_of};
use std::pin::Pin;

use foxglove::FoxgloveError;

/// A trait for converting a Foxglove C type to its native Rust () representation
pub trait BorrowToNative {
    type NativeType;

    /// Unsafely borrow this C struct into a native Rust schema struct, which can then be logged.
    ///
    /// We directly reference the C data, and/or copy it into memory allocated from the arena.
    ///
    /// # Safety:
    /// The caller must ensure the result is discarded before the original C data is mutated or freed.
    unsafe fn borrow_to_native(
        &self,
        arena: Pin<&mut Arena>,
    ) -> Result<ManuallyDrop<Self::NativeType>, FoxgloveError>;

    /// If None is passed, returns a FoxgloveError::ValueError.
    /// Otherwise call `borrow_to_native` on it.
    ///
    /// # Safety:
    /// See `borrow_to_native`.
    unsafe fn borrow_option_to_native(
        msg: Option<&Self>,
        arena: Pin<&mut Arena>,
    ) -> Result<ManuallyDrop<Self::NativeType>, FoxgloveError> {
        let Some(msg) = msg else {
            return Err(foxglove::FoxgloveError::ValueError(
                "msg is required".to_string(),
            ));
        };
        unsafe { msg.borrow_to_native(arena) }
    }
}

/// A fixed-size memory arena that allocates aligned arrays of POD types.
/// The arena contains a single inline array and allocates from it.
/// If the arena runs out of space, it returns an OutOfMemory error.
/// The allocated memory is "freed" by dropping the arena, destructors are not run.
pub struct Arena {
    buffer: UnsafeCell<[MaybeUninit<u8>; Arena::SIZE]>,
    offset: Cell<usize>,
    overflow: RefCell<Vec<(*mut u8, Layout)>>,
    // Marker to prevent moving
    _pin: PhantomPinned,
}

impl Arena {
    const SIZE: usize = 128 * 1024; // 128 KB

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
            buffer: UnsafeCell::new([MaybeUninit::uninit(); Self::SIZE]),
            offset: Cell::new(0),
            overflow: RefCell::new(Vec::new()),
            _pin: PhantomPinned,
        }
    }

    /// Allocates an array of `n` elements of type `T` from the arena.
    fn alloc<T>(&self, n: usize) -> *mut T {
        assert!(n > 0, "Cannot allocate 0 elements");
        let element_size = size_of::<T>();
        let bytes_needed = n * element_size;

        // Calculate aligned offset
        let base_addr = self.buffer.get() as usize;
        let aligned_offset =
            (base_addr + self.offset.get()).next_multiple_of(align_of::<T>()) - base_addr;

        // Check if we have enough space
        if aligned_offset + bytes_needed > Self::SIZE {
            let layout = std::alloc::Layout::array::<T>(n).unwrap();
            // SAFETY: layout is valid and non-zero and we don't assume the memory is initialized
            let ptr = unsafe { std::alloc::alloc(layout) };
            self.overflow.borrow_mut().push((ptr, layout));
            return ptr as *mut T;
        }

        // SAFETY: [result, result+n) is properly aligned and within the bounds of buffer
        let result = unsafe { (self.buffer.get() as *mut u8).add(aligned_offset) as *mut T };
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

        // SAFETY: we are not moving the arena, or moving out of it
        let result = self.as_mut().alloc::<S::NativeType>(len);

        // Convert the elements from S to S::NativeType, placing them in the result array
        for i in 0..len {
            unsafe {
                let tmp = (*src.add(i)).borrow_to_native(self.as_mut())?;
                *(result.add(i) as *mut _) = tmp;
            }
        }

        unsafe { Ok(ManuallyDrop::new(Vec::from_raw_parts(result, len, len))) }
    }

    /// Returns how many bytes are currently used in the arena.
    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.offset.get()
    }

    /// Returns how many bytes are available in the arena.
    #[cfg(test)]
    pub fn available(&self) -> usize {
        Self::SIZE - self.offset.get()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        for (ptr, layout) in self.overflow.borrow_mut().drain(..) {
            // SAFETY: ptr was allocated with layout via std::alloc::alloc
            unsafe { std::alloc::dealloc(ptr, layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::pin;

    #[test]
    fn test_allocate_different_types_and_verify_alignment() {
        let arena = pin!(Arena::new());

        // Allocate different types and verify alignment
        let int_ptr = arena.alloc::<i32>(10);
        assert_eq!(int_ptr as usize % align_of::<i32>(), 0);

        let double_ptr = arena.alloc::<f64>(5);
        assert_eq!(double_ptr as usize % align_of::<f64>(), 0);

        #[repr(align(16))]
        struct AlignedStruct {
            #[allow(dead_code)]
            data: [u8; 32],
        }

        let struct_ptr = arena.alloc::<AlignedStruct>(3);
        assert_eq!(struct_ptr as usize % align_of::<AlignedStruct>(), 0);

        // Verify we can write to the allocated memory
        unsafe {
            for i in 0..10 {
                *int_ptr.add(i) = i as i32;
            }

            for i in 0..5 {
                *double_ptr.add(i) = (i as f64) * 1.5;
            }

            // Verify the values were written correctly
            for i in 0..10 {
                assert_eq!(*int_ptr.add(i), i as i32);
            }

            for i in 0..5 {
                assert_eq!(*double_ptr.add(i), (i as f64) * 1.5);
            }
        }
    }

    #[test]
    fn test_allocate_from_heap_when_arena_capacity_exceeded() {
        let arena = pin!(Arena::new());

        // First, nearly fill the arena
        let nearly_full_size = Arena::SIZE - 1024;
        let buffer = arena.alloc::<u8>(nearly_full_size);
        assert!(!buffer.is_null());

        // Verify some data can be written to the arena allocation
        unsafe {
            *buffer = b'A';
            *buffer.add(nearly_full_size - 1) = b'Z';
            assert_eq!(*buffer, b'A');
            assert_eq!(*buffer.add(nearly_full_size - 1), b'Z');
        }

        // Check arena's reported space
        assert!(arena.used() >= nearly_full_size);
        assert_eq!(arena.available(), 1024);

        // Now allocate more than what's left in the arena
        const LARGE_ALLOCATION_SIZE: usize = 8192;
        let large_allocation = arena.alloc::<i32>(LARGE_ALLOCATION_SIZE / size_of::<i32>());
        assert!(!large_allocation.is_null());

        // Verify we can use the overflow allocation
        unsafe {
            for i in 0..(LARGE_ALLOCATION_SIZE / size_of::<i32>()) {
                *large_allocation.add(i) = i as i32;
            }
        }

        // Make several more overflow allocations
        let overflow1 = arena.alloc::<f64>(1000);
        let overflow2 = arena.alloc::<f32>(2000);

        assert!(!overflow1.is_null());
        assert!(!overflow2.is_null());

        // Verify each allocation can be written to
        unsafe {
            *overflow1 = std::f64::consts::PI;
            *overflow2 = std::f32::consts::E;

            assert_eq!(*overflow1, std::f64::consts::PI);
            assert_eq!(*overflow2, std::f32::consts::E);
        }
    }
}
