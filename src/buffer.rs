//! Endpoint memory buffers

use core::ptr::NonNull;

/// Endpoint memory buffer allocator
pub struct Allocator {
    start: *mut u8,
    ptr: *mut u8,
}

impl Allocator {
    /// # Safety
    ///
    /// Caller must ensure `start` points to an allocation of size. Caller must
    /// ensure that no one else is using this memory for anything else.
    pub unsafe fn new(start: NonNull<u8>, size: usize) -> Self {
        let start = start.as_ptr();
        let ptr = start.add(size);
        Allocator { start, ptr }
    }
    /// Allocates a buffer of `size`
    ///
    /// The pointer returned from `allocate` is guaranteed to be at least `size`
    /// bytes large.
    pub fn allocate(&mut self, size: usize) -> Option<NonNull<u8>> {
        let ptr = self.ptr as usize;
        let new_ptr = ptr.checked_sub(size)?;
        let start = self.start as usize;
        if new_ptr < start {
            None
        } else {
            self.ptr = new_ptr as *mut u8;
            NonNull::new(self.ptr) // Some(pointer)
        }
    }
    /// Represents an `Allocator` that does not allocate any memory
    pub fn empty() -> Self {
        Allocator {
            start: core::ptr::null_mut(),
            ptr: core::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::Allocator;
    use core::ptr::NonNull;

    #[test]
    fn allocate_entire_buffer() {
        let mut buffer = [0; 32];
        let mut alloc = unsafe { Allocator::new(NonNull::new_unchecked(buffer.as_mut_ptr()), 32) };
        let ptr = alloc.allocate(32);
        assert!(ptr.is_some());
        assert_eq!(ptr.unwrap().as_ptr(), buffer.as_mut_ptr());

        let ptr = alloc.allocate(1);
        assert!(ptr.is_none());
    }

    #[test]
    fn allocate_partial_buffers() {
        let mut buffer = [0; 32];
        let mut alloc = unsafe { Allocator::new(NonNull::new_unchecked(buffer.as_mut_ptr()), 32) };

        let ptr = alloc.allocate(7);
        assert!(ptr.is_some());
        assert_eq!(ptr.unwrap().as_ptr(), unsafe {
            buffer.as_mut_ptr().add(32 - 7)
        });

        let ptr = alloc.allocate(7);
        assert!(ptr.is_some());
        assert_eq!(ptr.unwrap().as_ptr(), unsafe {
            buffer.as_mut_ptr().add(32 - 14)
        });

        let ptr = alloc.allocate(19);
        assert!(ptr.is_none());
    }
}
