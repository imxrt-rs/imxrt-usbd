//! Support library (qualified as `support`) for all examples.

#![no_std]

use teensy4_fcb as _;
use teensy4_panic as _;

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
pub fn new_bus_adapter() -> imxrt_usbd::usb1::BusAdapter {
    let usb = ral::usb::USB1::take().unwrap();
    let usbphy = ral::usbphy::USBPHY1::take().unwrap();

    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::usb1::BusAdapter::new(usb, usbphy, &mut ENDPOINT_MEMORY)
    }
}

pub mod ccm {
    use super::ral;

    /// Initialize CCM clocks for USB1
    pub fn initialize(ccm: &ral::ccm::Instance, ccm_analog: &ral::ccm_analog::Instance) {
        // Enable the PLL...
        loop {
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, ENABLE == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, ENABLE: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, POWER == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, POWER: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, LOCK == 0) {
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, BYPASS == 1) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_CLR, BYPASS: 1);
                continue;
            }
            if ral::read_reg!(ral::ccm_analog, ccm_analog, PLL_USB1, EN_USB_CLKS == 0) {
                ral::write_reg!(ral::ccm_analog, ccm_analog, PLL_USB1_SET, EN_USB_CLKS: 1);
                continue;
            }
            break;
        }

        // Enable the clock gates...
        ral::modify_reg!(ral::ccm, ccm, CCGR6, CG1: 0b11, CG0: 0b11);
    }
}
