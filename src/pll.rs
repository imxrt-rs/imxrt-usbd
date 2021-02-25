//! USB PLL support

use crate::ral;

/// TODO only supports USB1 PLL; should support both
pub fn initialize(ccm_analog: &ral::ccm_analog::Instance) {
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
}
