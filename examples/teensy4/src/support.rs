//! Support library (qualified as `support`) for all examples.

#![no_std]

pub use bsp::hal;
pub use hal::ral;
pub use teensy4_bsp as bsp;

use bsp::common;

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
/// Panics if the USB1, USBPHY1, or USBNC1 imxrt-ral instances are
/// already taken.
pub fn new_bus_adapter() -> imxrt_usbd::full_speed::BusAdapter {
    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::full_speed::BusAdapter::new(Instances::usb1(), &mut ENDPOINT_MEMORY)
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

//
// Keep in sync with the imxrt_usbd::Peripherals example!
//

use imxrt_usbd::Peripherals;
use ral::{usb, usbnc, usbphy};

struct Instances {
    usb: usb::Instance,
    usbnc: usbnc::Instance,
    usbphy: usbphy::Instance,
}

impl Instances {
    /// Panics if the instancs are already taken
    fn usb1() -> Instances {
        Self {
            usb: usb::USB1::take().unwrap(),
            usbnc: usbnc::USBNC1::take().unwrap(),
            usbphy: usbphy::USBPHY1::take().unwrap(),
        }
    }
}

unsafe impl Peripherals for Instances {
    fn core(&self) -> *const () {
        &*self.usb as *const _ as _
    }
    fn non_core(&self) -> *const () {
        &*self.usbnc as *const _ as _
    }
    fn phy(&self) -> *const () {
        &*self.usbphy as *const _ as _
    }
}
