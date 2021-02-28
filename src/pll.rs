//! USB PLL support

use crate::ral;

pub fn initialize(ccm_analog: &ral::ccm_analog::Instance, usb: &ral::usb::Instance) {
    // Dispatch to the correct PLL_USB registers...
    let pll_usb = match &**usb as *const _ {
        ral::usb::USB1 => PllUsb {
            PLL_USB: &ccm_analog.PLL_USB1,
            PLL_USB_SET: &ccm_analog.PLL_USB1_SET,
            PLL_USB_CLR: &ccm_analog.PLL_USB1_CLR,
        },
        #[cfg(feature = "double-instance")]
        ral::usb::USB2 => PllUsb {
            PLL_USB: &ccm_analog.PLL_USB2,
            PLL_USB_SET: &ccm_analog.PLL_USB2_SET,
            PLL_USB_CLR: &ccm_analog.PLL_USB2_CLR,
        },
        _ => panic!("Unhandled USB instance"),
    };

    loop {
        if ral::read_reg!(crate::pll, &pll_usb, PLL_USB, ENABLE == 0) {
            ral::write_reg!(crate::pll, &pll_usb, PLL_USB_SET, ENABLE: 1);
            continue;
        }
        if ral::read_reg!(crate::pll, &pll_usb, PLL_USB, POWER == 0) {
            ral::write_reg!(crate::pll, &pll_usb, PLL_USB_SET, POWER: 1);
            continue;
        }
        if ral::read_reg!(crate::pll, &pll_usb, PLL_USB, LOCK == 0) {
            continue;
        }
        if ral::read_reg!(crate::pll, &pll_usb, PLL_USB, BYPASS == 1) {
            ral::write_reg!(crate::pll, &pll_usb, PLL_USB_CLR, BYPASS: 1);
            continue;
        }
        if ral::read_reg!(crate::pll, &pll_usb, PLL_USB, EN_USB_CLKS == 0) {
            ral::write_reg!(crate::pll, &pll_usb, PLL_USB_SET, EN_USB_CLKS: 1);
            continue;
        }
        break;
    }
}

/// RAL register remapper
#[allow(non_snake_case)]
struct PllUsb<'a> {
    PLL_USB: &'a ral::RWRegister<u32>,
    PLL_USB_SET: &'a ral::RWRegister<u32>,
    PLL_USB_CLR: &'a ral::RWRegister<u32>,
}

#[allow(non_snake_case)]
mod PLL_USB {
    pub use super::ral::ccm_analog::PLL_USB1::*;
}

#[allow(non_snake_case)]
mod PLL_USB_SET {
    pub use super::ral::ccm_analog::PLL_USB1_SET::*;
}

#[allow(non_snake_case)]
mod PLL_USB_CLR {
    pub use super::ral::ccm_analog::PLL_USB1_CLR::*;
}
