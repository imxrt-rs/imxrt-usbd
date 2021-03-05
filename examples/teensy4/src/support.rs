//! Support library (qualified as `support`) for all examples.

#![no_std]

use teensy4_fcb as _;

use hal::ral;
use imxrt_hal as hal;
use teensy4_pins::common;

pub type LED = hal::gpio::GPIO<common::P13, hal::gpio::Output>;
pub fn configure_led(pad: common::P13) -> LED {
    let mut led = hal::gpio::GPIO::new(pad);
    led.set_fast(true);
    led.output()
}

/// Allocates a `BusAdapter`
///
/// # Panics
///
/// Panics if the USB1 and USBPHY1 imxrt-ral instances are
/// already taken.
pub fn new_bus_adapter() -> imxrt_usbd::full_speed::BusAdapter {
    let usb = ral::usb::USB1::take().unwrap();
    let usbphy = ral::usbphy::USBPHY1::take().unwrap();

    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::full_speed::BusAdapter::new(usb, usbphy, &mut ENDPOINT_MEMORY)
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("{}", info);
    for _ in 0..10_000 {
        imxrt_uart_log::dma::poll();
    }
    teensy4_panic::sos()
}

#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    const SCB_VTOR: *mut u32 = 0xE000_ED08 as *mut u32;
    core::ptr::write_volatile(SCB_VTOR, 0x60001400 /* ORIGIN(FLASH) */);

    const CCM_CLPCR: *mut u32 = 0x400F_C054 as *mut _;
    CCM_CLPCR.write_volatile(CCM_CLPCR.read_volatile() & !0b11);
}
