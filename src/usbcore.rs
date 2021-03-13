//! USB Core module for the usb-device endpoint-trait prototype
//!
//! This module provides an implementation of the new usb-device
//! endpoint-trait design. It exists along with the default 0.2
//! usb-device release. We've renamed the unrelease crate to
//! `endpoint-trait` to make the distinction clear.

mod allocator;
mod endpoint;

use endpoint::Endpoint;

use crate::ral;

use endpoint_trait::{endpoint::EndpointAddress, usbcore::PollResult, Result, UsbError};

/// USB full-speed bus
pub struct FullSpeed {
    usb: &'static ral::usb::RegisterBlock,
    phy: &'static ral::usbphy::RegisterBlock,
    allocator: Option<allocator::Allocator<Self>>,
    /// Need to track interrupt configuation, since it will
    /// be reset when the device calls enable()
    enable_interrupts: bool,
}

impl FullSpeed {
    /// Create a full-speed driver
    ///
    /// You must ensure that no one else is using the endpoint memory!
    pub fn new<P: crate::Peripherals>(peripherals: P, buffer: &'static mut [u8]) -> Self {
        let ral::Instances { usb, usbphy: phy } = ral::instances(peripherals);
        let buffers = crate::buffer::Allocator::new(buffer);

        let usb: &ral::usb::RegisterBlock = &*usb;
        let phy: &ral::usbphy::RegisterBlock = &*phy;

        // Safety: lifetime transmute, references are static.
        let usb = unsafe { core::mem::transmute(usb) };
        let phy = unsafe { core::mem::transmute(phy) };

        let full_speed = FullSpeed {
            usb,
            phy,
            // Safety: we own `peripherals`, and the user guarantees that there's only
            // one `peripherals` instance per USB peripheral.
            allocator: Some(unsafe { allocator::Allocator::new(usb, buffers) }),
            enable_interrupts: false,
        };
        full_speed
    }

    /// Enable (`true`) or disable (`false`) USB interrupts
    pub fn set_interrupts(&mut self, interrupts: bool) {
        self.enable_interrupts = interrupts;
        if interrupts {
            // Keep this in sync with the poll() behaviors
            ral::write_reg!(ral::usb, self.usb, USBINTR, UE: 1, URE: 1);
        } else {
            ral::write_reg!(ral::usb, self.usb, USBINTR, 0);
        }
    }
}

impl endpoint_trait::usbcore::UsbCore for FullSpeed {
    type EndpointAllocator = allocator::Allocator<Self>;
    type EndpointIn = Endpoint;
    type EndpointOut = Endpoint;

    const QUIRK_SET_ADDRESS_BEFORE_STATUS: bool = true;

    fn create_allocator(&mut self) -> Self::EndpointAllocator {
        self.allocator.take().unwrap()
    }

    fn enable(&mut self, allocator: Self::EndpointAllocator) -> Result<()> {
        self.allocator = Some(allocator);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_SET, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, SFTRST: 1);
        ral::write_reg!(ral::usbphy, self.phy, CTRL_CLR, CLKGATE: 1);
        ral::write_reg!(ral::usbphy, self.phy, PWD, 0);

        ral::modify_reg!(ral::usb, self.usb, USBCMD, RST: 1);
        while ral::read_reg!(ral::usb, self.usb, USBCMD, RST == 1) {}

        ral::write_reg!(ral::usb, self.usb, USBMODE, CM: CM_2, SLOM: 1);

        // This forces the bus to run at full speed, not high speed. Specifically,
        // it disables the chirp. If you're interested in playing with a high-speed
        // USB driver, you'll want to remove this line, or clear PFSC.
        ral::modify_reg!(ral::usb, self.usb, PORTSC1, PFSC: 1);

        ral::modify_reg!(ral::usb, self.usb, USBSTS, |usbsts| usbsts);
        self.set_interrupts(self.enable_interrupts);

        allocator::assign_endptlistaddr(self.usb);
        ral::write_reg!(ral::usb, self.usb, USBCMD, RS: 1);
        debug!("ENABLED");
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        ral::modify_reg!(ral::usb, self.usb, ENDPTSTAT, |endptstat| endptstat);

        ral::modify_reg!(ral::usb, self.usb, ENDPTCOMPLETE, |endptcomplete| {
            endptcomplete
        });
        ral::modify_reg!(ral::usb, self.usb, ENDPTNAK, |endptnak| endptnak);
        ral::write_reg!(ral::usb, self.usb, ENDPTNAKEN, 0);

        while ral::read_reg!(ral::usb, self.usb, ENDPTPRIME) != 0 {}
        ral::write_reg!(ral::usb, self.usb, ENDPTFLUSH, u32::max_value());
        while ral::read_reg!(ral::usb, self.usb, ENDPTFLUSH) != 0 {}

        debug_assert!(
            ral::read_reg!(ral::usb, self.usb, PORTSC1, PR == 1),
            "Took too long to handle bus reset"
        );
        debug!("RESET");
        Ok(())
    }

    fn poll(&mut self) -> Result<PollResult> {
        let usbsts = ral::read_reg!(ral::usb, self.usb, USBSTS);
        ral::write_reg!(ral::usb, self.usb, USBSTS, usbsts);

        use ral::usb::USBSTS;
        if usbsts & USBSTS::URI::mask != 0 {
            return Ok(PollResult::Reset);
        }

        if usbsts & USBSTS::UI::mask != 0 {
            trace!(
                "{:X} {:X}",
                ral::read_reg!(ral::usb, self.usb, ENDPTSETUPSTAT),
                ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE)
            );
            // Note: could be complete in one register read, but this is a little
            // easier to comprehend...
            let ep_out = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ERCE) as u16;

            let ep_in_complete = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE);
            ral::write_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE: ep_in_complete);

            let ep_setup = ral::read_reg!(ral::usb, self.usb, ENDPTSETUPSTAT) as u16;

            Ok(PollResult::Data {
                ep_out: ep_out | ep_setup,
                ep_in_complete: ep_in_complete as u16,
            })
        } else {
            Err(UsbError::WouldBlock)
        }
    }

    fn set_device_address(&mut self, addr: u8) -> Result<()> {
        // See the "quirk" note. We're using USBADRA to let
        // the hardware set the address before the status phase.
        ral::write_reg!(ral::usb, self.usb, DEVICEADDR, USBADR: addr as u32, USBADRA: 1);
        Ok(())
    }

    fn set_stalled(&mut self, ep_addr: EndpointAddress, _: bool) -> Result<()> {
        panic!("set_stalled called on UsbCore with {:?}", ep_addr);
    }

    fn is_stalled(&mut self, ep_addr: EndpointAddress) -> Result<bool> {
        panic!("is_stalled called on UsbCore with {:?}", ep_addr);
    }

    fn suspend(&mut self) -> Result<()> {
        todo!("Signal suspend from poll()");
    }

    fn resume(&mut self) -> Result<()> {
        todo!("Signal resume from poll()");
    }
}
