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
/// Panics if the USB1 or USBPHY1 imxrt-ral instances are
/// already taken. The bus adapter owns the USB1 core registers.
/// This function will release USBPHY1.
pub fn new_bus_adapter() -> imxrt_usbd::full_speed::BusAdapter {
    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::full_speed::BusAdapter::new(Peripherals::usb1(), &mut ENDPOINT_MEMORY)
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

struct Peripherals {
    _usb: ral::usb::Instance,
    _phy: ral::usbphy::Instance,
    _nc: ral::usbnc::Instance,
    _analog: ral::usb_analog::Instance,
}

impl Peripherals {
    /// Panics if the instances are already taken
    fn usb1() -> Peripherals {
        Self {
            _usb: ral::usb::USB1::take().unwrap(),
            _phy: ral::usbphy::USBPHY1::take().unwrap(),
            _nc: ral::usbnc::USBNC1::take().unwrap(),
            _analog: ral::usb_analog::USB_ANALOG::take().unwrap(),
        }
    }
}

unsafe impl imxrt_usbd::Peripherals for Peripherals {
    fn instance(&self) -> imxrt_usbd::Instance {
        imxrt_usbd::Instance::USB1
    }
}
