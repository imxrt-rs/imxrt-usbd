//! Support library (qualified as `support`) for all examples.

#![no_std]

use teensy4_fcb as _;
use teensy4_panic as _;

use imxrt_hal as hal;
use teensy4_pins::common;

pub type LED = hal::gpio::GPIO<common::P13, hal::gpio::Output>;
pub fn configure_led(pad: common::P13) -> LED {
    let mut led = hal::gpio::GPIO::new(pad);
    led.set_fast(true);
    led.output()
}

/// Assign memory for all of the USB's endpoints
///
/// # Panics
///
/// Panics if called more than once.
pub fn set_endpoint_memory(usb: &mut imxrt_usb::USB) {
    use core::sync::atomic;

    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];
    static ONCE_GUARD: atomic::AtomicBool = atomic::AtomicBool::new(false);

    if ONCE_GUARD.swap(true, atomic::Ordering::SeqCst) {
        panic!("Already allocated endpoint memory!");
    }

    unsafe {
        // Safety: ENDPOINT_MEMORY is unlikely to be null
        let ptr = core::ptr::NonNull::new_unchecked(ENDPOINT_MEMORY.as_mut_ptr());
        // Safety: ENDPOINT_MEMORY valid for it's length. With proper scoping
        // and a runtime flag, we ensure it's only available to a single caller.
        usb.set_endpoint_memory(ptr, ENDPOINT_MEMORY.len());
    }
}
