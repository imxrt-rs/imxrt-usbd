//! Volatile cell that conforms to the RAL's register API
//!
//! `VCell` use volatile reads and writes on an owned type
//! `T`. `VCell` does not support interior mutability, so
//! types using `VCell` must expose mutability. (It's probably
//! not a 'cell' then, but given it's history we'll keep the
//! name...)

use core::ptr;

/// A memory location that requires volatile reads and writes
#[repr(transparent)]
pub struct VCell<T>(T);

impl<T> VCell<T> {
    /// Construct a `VCell` that's initialized to `val`
    pub const fn new(val: T) -> Self {
        VCell(val)
    }
}

impl<T: Copy> VCell<T> {
    /// Perform a volatile read from this memory location
    pub fn read(&self) -> T {
        // Safety: we know this memory is valid...
        unsafe { ptr::read_volatile(&self.0) }
    }
    /// Perform a volatile write at this memory location
    pub fn write(&mut self, val: T) {
        // Safety: we know this memory is valid
        unsafe { ptr::write_volatile(&mut self.0, val) }
    }
}
