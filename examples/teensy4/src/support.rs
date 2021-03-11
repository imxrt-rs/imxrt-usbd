//! Support library (qualified as `support`) for all examples.

#![no_std]

pub use bsp::hal;
pub use hal::ral;
pub use teensy4_bsp as bsp;

pub use bsp::configure_led;
pub use bsp::LED;

/// Allocates a `FullSpeed` USB driver
///
/// # Panics
///
/// Panics if any of the `imxrt-ral` USB instances are already
/// taken.
pub fn new_full_speed() -> imxrt_usbd::usbcore::FullSpeed {
    // If we're here, we have exclusive access to ENDPOINT_MEMORY
    static mut ENDPOINT_MEMORY: [u8; 4096] = [0; 4096];

    unsafe {
        // Safety: With proper scoping and checks for singleton access, we ensure the memory is
        // only available to a single caller.
        imxrt_usbd::usbcore::FullSpeed::new(Peripherals::usb1(), &mut ENDPOINT_MEMORY)
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
        ral::modify_reg!(ral::ccm, ccm, CCGR6, CG0: 0b11);
    }
}
