//! Volatile cell that conforms to the RAL's register API

use core::cell::UnsafeCell;

#[repr(transparent)]
pub struct VCell<T>(UnsafeCell<T>);

impl<T> VCell<T> {
    pub const fn new(val: T) -> Self {
        VCell(UnsafeCell::new(val))
    }
}

impl<T: Copy> VCell<T> {
    pub fn read(&self) -> T {
        unsafe { self.0.get().read_volatile() }
    }
    pub fn write(&self, val: T) {
        unsafe { self.0.get().write_volatile(val) }
    }
}
