//! Internal USB1 driver
//!
//! The goal is to keep this somewhat agnostic from the usb-device
//! bus behaviors, so that it could be used separately.
//!
//! The full-speed driver forces a full speed data rate. See the
//! notes in the `initialize()` implementation.

use super::{
    endpoint::{Endpoint, Status},
    state,
};
use crate::{buffer, qh, ral, td, QH_COUNT};
use usb_device::{
    bus::PollResult,
    endpoint::{EndpointAddress, EndpointType},
    UsbDirection,
};

const EP_INIT: [Option<Endpoint>; QH_COUNT] = [
    None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None,
];

/// Produces an index into the EPs, QHs, and TDs collections
fn index(ep_addr: EndpointAddress) -> usize {
    (ep_addr.index() * 2) + (UsbDirection::In == ep_addr.direction()) as usize
}

/// A full-speed USB driver
///
/// `FullSpeed` itself doesn't provide much of an API. After you allocate a `FullSpeed` with [`new()`](FullSpeed::new),
/// you must
///
/// - call [`initialize()`](FullSpeed::initialize) once
/// - supply endpoint memory with [`set_endpoint_memory()`](USB::set_endpoint_memory)
pub struct FullSpeed {
    endpoints: [Option<Endpoint>; QH_COUNT],
    usb: ral::usb::Instance,
    phy: ral::usbphy::Instance,
    qhs: [Option<&'static mut qh::QH>; QH_COUNT],
    tds: [Option<&'static mut td::TD>; QH_COUNT],
    buffer_allocator: buffer::Allocator,
}

impl FullSpeed {
    /// Create a new `FullSpeed` driver
    ///
    /// Creation does nothing except for assign static memory to the driver.
    /// After creating the driver, call [`initialize()`](USB::initialize).
    ///
    /// # Panics
    ///
    /// Panics if the `usb` instance and the `phy` instances are mismatched.
    pub fn new(usb: ral::usb::Instance, phy: ral::usbphy::Instance) -> Self {
        // Safety: taking static memory. Assumes that the provided
        // USB instance is a singleton, which is the only safe way for it
        // to exist.
        unsafe {
            let (qhs, tds) = match (&*usb as *const _, &*phy as *const _) {
                (ral::usb::USB1, ral::usbphy::USBPHY1) => {
                    (state::steal_qhs(&usb), state::steal_tds(&usb))
                }
                #[cfg(feature = "double-instance")]
                (ral::usb::USB2, ral::usbphy::USBPHY2) => {
                    (state::steal_qhs(&usb), state::steal_tds(&usb))
                }
                _ => panic!("Mismatch USB and USBPHY"),
            };
            FullSpeed {
                endpoints: EP_INIT,
                usb,
                phy,
                qhs,
                tds,
                buffer_allocator: buffer::Allocator::empty(),
            }
        }
    }

    /// Set the region of memory that can be used for endpoint I/O
    ///
    /// This memory will be shared across all endpoints. you should size it
    /// to support all the endpoints that might be allocated by your USB classes.
    ///
    /// After this call, `FullSpeed` assumes that it's the sole owner of `buffer`.
    /// You assume the `unsafe`ty to make that true.
    pub fn set_endpoint_memory(&mut self, buffer: &'static mut [u8]) {
        self.buffer_allocator = buffer::Allocator::new(buffer);
    }

    /// Initialize the USB physical layer, and the USB core registers
    ///
    /// Assumes that the CCM clock gates are enabled, and the PLL is on.
    ///
    /// You **must** call this once, before creating the complete USB
    /// bus.
    pub fn initialize(&mut self) {
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
        // Disable interrupts by default
        ral::write_reg!(ral::usb, self.usb, USBINTR, 0);

        state::assign_endptlistaddr(&self.usb);
    }

    /// Enable (`true`) or disable (`false`) USB interrupts
    pub fn set_interrupts(&mut self, interrupts: bool) {
        if interrupts {
            // Keep this in sync with the poll() behaviors
            ral::write_reg!(ral::usb, self.usb, USBINTR, UE: 1, URE: 1);
        } else {
            ral::write_reg!(ral::usb, self.usb, USBINTR, 0);
        }
    }

    pub fn set_address(&mut self, address: u8) {
        // See the "quirk" note in the UsbBus impl. We're using USBADRA to let
        // the hardware set the address before the status phase.
        ral::write_reg!(ral::usb, self.usb, DEVICEADDR, USBADR: address as u32, USBADRA: 1);
    }

    pub fn attach(&mut self) {
        // TODO should probably be a modify...
        ral::write_reg!(ral::usb, self.usb, USBCMD, RS: 1);
    }

    pub fn bus_reset(&mut self) {
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
    }

    /// Check if the endpoint is valid
    pub fn is_allocated(&self, addr: EndpointAddress) -> bool {
        self.endpoints
            .get(index(addr))
            .map(|ep| ep.is_some())
            .unwrap_or(false)
    }

    /// Read either a setup, or a data buffer, from EP0 OUT
    ///
    /// Return the status if there's a pending transaction, or an error.
    ///
    /// # Panics
    ///
    /// Panics if EP0 OUT isn't allocated.
    pub fn ctrl0_read(&mut self, buffer: &mut [u8]) -> Result<usize, Status> {
        let ctrl_out = self.endpoints[0].as_mut().unwrap();
        if ctrl_out.has_setup(&self.usb) && buffer.len() >= 8 {
            let setup = ctrl_out.read_setup(&self.usb);
            buffer[..8].copy_from_slice(&setup.to_le_bytes());

            ctrl_out.flush(&self.usb);
            ctrl_out.clear_complete(&self.usb);
            let max_packet_len = ctrl_out.max_packet_len();
            ctrl_out.schedule_transfer(&self.usb, max_packet_len);

            Ok(8)
        } else {
            ctrl_out.check_status()?;

            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);

            let read = ctrl_out.read(buffer);
            let max_packet_len = ctrl_out.max_packet_len();
            ctrl_out.schedule_transfer(&self.usb, max_packet_len);

            Ok(read)
        }
    }

    /// Write to the host from EP0 IN
    ///
    /// Schedules the next OUT transfer to satisfy a status phase.
    ///
    /// # Panics
    ///
    /// Panics if EP0 IN isn't allocated, or if EP0 OUT isn't allocated.
    pub fn ctrl0_write(&mut self, buffer: &[u8]) -> Result<usize, Status> {
        let ctrl_in = self.endpoints[1].as_mut().unwrap();
        ctrl_in.check_status()?;

        ctrl_in.clear_nack(&self.usb);

        let written = ctrl_in.write(buffer);
        ctrl_in.schedule_transfer(&self.usb, written);

        if !buffer.is_empty() {
            // Schedule an OUT transfer for the status phase...
            let ctrl_out = self.endpoints[0].as_mut().unwrap();
            ctrl_out.flush(&self.usb);
            ctrl_out.clear_complete(&self.usb);
            ctrl_out.clear_nack(&self.usb);
            ctrl_out.schedule_transfer(&self.usb, 0);
        }

        Ok(written)
    }

    /// Read data from an endpoint, and schedule the next transfer
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    pub fn ep_read(&mut self, buffer: &mut [u8], addr: EndpointAddress) -> Result<usize, Status> {
        let ep = self.endpoints[index(addr)].as_mut().unwrap();
        ep.check_status()?;

        ep.clear_complete(&self.usb);
        ep.clear_nack(&self.usb);

        let read = ep.read(buffer);

        let max_packet_len = ep.max_packet_len();
        ep.schedule_transfer(&self.usb, max_packet_len);

        Ok(read)
    }

    /// Write data to an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated.
    pub fn ep_write(&mut self, buffer: &[u8], addr: EndpointAddress) -> Result<usize, Status> {
        let ep = self.endpoints[index(addr)].as_mut().unwrap();
        ep.check_status()?;

        ep.clear_nack(&self.usb);

        let written = ep.write(buffer);
        ep.schedule_transfer(&self.usb, written);

        Ok(written)
    }

    /// Stall an endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    pub fn ep_stall(&mut self, stall: bool, addr: EndpointAddress) {
        self.endpoints[index(addr)]
            .as_mut()
            .unwrap()
            .set_stalled(&self.usb, stall);
    }

    /// Checks if an endpoint is stalled
    ///
    /// # Panics
    ///
    /// Panics if the endpoint isn't allocated
    pub fn is_ep_stalled(&self, addr: EndpointAddress) -> bool {
        self.endpoints[index(addr)]
            .as_ref()
            .unwrap()
            .is_stalled(&self.usb)
    }

    /// Allocate a buffer from the endpoint memory
    pub fn allocate_buffer(&mut self, max_packet_len: usize) -> Option<buffer::Buffer> {
        self.buffer_allocator.allocate(max_packet_len)
    }

    /// Allocate a specific endpoint
    ///
    /// # Panics
    ///
    /// Panics if the endpoint is already allocated.
    pub fn allocate_ep(
        &mut self,
        addr: EndpointAddress,
        buffer: buffer::Buffer,
        kind: EndpointType,
    ) {
        let qh = self.qhs[index(addr)].take().unwrap();
        let td = self.tds[index(addr)].take().unwrap();

        qh.set_max_packet_len(buffer.len());
        qh.set_zero_length_termination(false);
        qh.set_interrupt_on_setup(
            EndpointType::Control == kind && addr.direction() == UsbDirection::Out,
        );

        td.set_terminate();
        td.clear_status();

        let mut ep = Endpoint::new(addr, qh, td, buffer);
        ep.initialize(&self.usb, kind);
        self.endpoints[index(addr)] = Some(ep);
    }

    /// Enable all non-zero endpoints, and schedule OUT transfers
    ///
    /// This should only be called when the device is configured
    pub fn enable_endpoints(&mut self) {
        for ep in self.endpoints.iter_mut().flat_map(core::convert::identity) {
            ep.enable(&self.usb);
            if ep.address().direction() == UsbDirection::Out {
                let max_packet_len = ep.max_packet_len();
                ep.schedule_transfer(&self.usb, max_packet_len);
            }
        }
    }

    /// Poll for reset or USB traffic
    pub fn poll(&mut self) -> PollResult {
        let usbsts = ral::read_reg!(ral::usb, self.usb, USBSTS);
        ral::write_reg!(ral::usb, self.usb, USBSTS, usbsts);

        use ral::usb::USBSTS;
        if usbsts & USBSTS::URI::mask != 0 {
            PollResult::Reset
        } else if usbsts & USBSTS::UI::mask != 0 {
            // Note: could be complete in one register read, but this is a little
            // easier to comprehend...
            let ep_out = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ERCE);

            let ep_in_complete = ral::read_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE);
            ral::write_reg!(ral::usb, self.usb, ENDPTCOMPLETE, ETCE: ep_in_complete);

            let ep_setup = ral::read_reg!(ral::usb, self.usb, ENDPTSETUPSTAT) as u16;

            PollResult::Data {
                ep_out: ep_out as u16,
                ep_in_complete: ep_in_complete as u16,
                ep_setup,
            }
        } else {
            PollResult::None
        }
    }
}
