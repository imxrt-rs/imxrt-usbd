#![no_std]

mod buffer;
mod pll;
mod qh;
mod td;
mod vcell;

use imxrt_ral as ral;

//
// Helpers
//

/// Initialize all USB physical, analog clocks, and core registers.
/// Assumes that the CCM clock gates are enabled.
fn initialize(
    usb: &ral::usb::Instance,
    phy: &ral::usbphy::Instance,
    ccm_analog: &ral::ccm_analog::Instance,
) {
    pll::initialize(ccm_analog);

    ral::write_reg!(ral::usbphy, phy, CTRL_SET, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, SFTRST: 1);
    ral::write_reg!(ral::usbphy, phy, CTRL_CLR, CLKGATE: 1);
    ral::write_reg!(ral::usbphy, phy, PWD, 0);

    ral::modify_reg!(ral::usb, usb, USBCMD, RST: 1);
    while ral::read_reg!(ral::usb, usb, USBCMD, RST == 1) {}

    ral::write_reg!(ral::usb, usb, USBMODE, CM: CM_2, SLOM: 1);
    ral::modify_reg!(ral::usb, usb, PORTSC1, PFSC: 1);
    ral::modify_reg!(ral::usb, usb, USBSTS, |usbsts| usbsts);
    ral::write_reg!(ral::usb, usb, USBINTR, 0);
}

fn set_address(usb: &ral::usb::Instance, address: u8) {
    ral::write_reg!(ral::usb, usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
}

fn set_enpoint_list_address(usb: &ral::usb::Instance, eplistaddr: *const ()) {
    ral::write_reg!(ral::usb, usb, ASYNCLISTADDR, eplistaddr as u32);
}

fn attach(usb: &ral::usb::Instance) {
    ral::write_reg!(ral::usb, usb, USBCMD, RS: 1);
}

fn bus_reset(usb: &ral::usb::Instance) {
    ral::modify_reg!(ral::usb, usb, ENDPTSTAT, |endptstat| endptstat);

    ral::modify_reg!(ral::usb, usb, ENDPTCOMPLETE, |endptcomplete| {
        endptcomplete
    });
    ral::modify_reg!(ral::usb, usb, ENDPTNAK, |endptnak| endptnak);
    ral::write_reg!(ral::usb, usb, ENDPTNAKEN, 0);

    while ral::read_reg!(ral::usb, usb, ENDPTPRIME) != 0 {}
    ral::write_reg!(ral::usb, usb, ENDPTFLUSH, u32::max_value());
    while ral::read_reg!(ral::usb, usb, ENDPTFLUSH) != 0 {}

    debug_assert!(
        ral::read_reg!(ral::usb, usb, PORTSC1, PR == 1),
        "Took too long to handle bus reset"
    );
}
