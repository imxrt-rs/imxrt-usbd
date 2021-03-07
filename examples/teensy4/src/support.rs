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
    let phy = ral::usbphy::USBPHY1::take().unwrap();
    ral::write_reg!(ral::usbphy, phy, CTRL_SET, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, CLKGATE: 1);
    ral::write_reg!(ral::usbphy, phy, PWD, 0);
    ral::usbphy::USBPHY1::release(phy);

    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::full_speed::BusAdapter::new(CoreRegisters::usb1(), &mut ENDPOINT_MEMORY)
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

use ral::usb;

struct CoreRegisters {
    _usb: ral::usb::Instance,
}

impl CoreRegisters {
    /// Panics if the instance is already taken
    fn usb1() -> CoreRegisters {
        Self {
            _usb: usb::USB1::take().unwrap(),
        }
    }
}

unsafe impl imxrt_usbd::CoreRegisters for CoreRegisters {
    fn instance(&self) -> imxrt_usbd::Instance {
        imxrt_usbd::Instance::USB1
    }
}
